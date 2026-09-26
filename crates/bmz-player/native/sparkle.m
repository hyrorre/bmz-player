#import <AppKit/AppKit.h>
#import <Sparkle/Sparkle.h>
#include <stdbool.h>

// All entry points and delegate callbacks run on winit's Cocoa main thread.
@interface BMZUpdater : NSObject <SPUUserDriver, SPUUpdaterDelegate>
@property(nonatomic, strong) SPUUpdater *updater;
@property(nonatomic, strong) NSMutableArray<NSDictionary *> *events;
@property(copy) void (^choice)(SPUUserUpdateChoice);
@property(copy) void (^cancellation)(void);
@property(copy) void (^installHandler)(void);
@property BOOL ready;
@property BOOL approved;
@property BOOL report;
@property BOOL paused;
@property BOOL errorReported;
@property uint64_t received;
@property uint64_t total;
@property BOOL extracting;
@property(copy) NSString *selectedFeed;
@end

static BMZUpdater *driver;
static NSString *lastEvent;
static BOOL testInstallHandlerInvoked;

@implementation BMZUpdater
- (void)emit:(NSDictionary *)event { [self.events addObject:event]; }
- (void)progress {
    // Coalesce progress so a long frame cannot accumulate an unbounded queue.
    if ([self.events.lastObject[@"event"] isEqual:@"progress"]) [self.events removeLastObject];
    [self emit:@{@"event":@"progress", @"received":@(self.received), @"total":@(self.total), @"extracting":@(self.extracting)}];
}
- (void)showUpdatePermissionRequest:(SPUUpdatePermissionRequest *)request reply:(void (^)(SUUpdatePermissionResponse *))reply {
    reply([[SUUpdatePermissionResponse alloc] initWithAutomaticUpdateChecks:NO sendSystemProfile:NO]);
}
- (void)showUserInitiatedUpdateCheckWithCancellation:(void (^)(void))cancellation { self.cancellation = cancellation; }
- (void)showUpdateFoundWithAppcastItem:(SUAppcastItem *)item state:(SPUUserUpdateState *)state reply:(void (^)(SPUUserUpdateChoice))reply {
    self.cancellation = nil;
    if (self.paused) { reply(state.stage == SPUUserUpdateStageInstalling ? SPUUserUpdateChoiceSkip : SPUUserUpdateChoiceDismiss); return; }
    self.choice = reply;
    self.ready = state.stage == SPUUserUpdateStageInstalling;
    [self emit:@{@"event":@"available", @"version":item.displayVersionString, @"installable":item.informationOnlyUpdate ? @NO : @YES}];
    if (self.ready) [self emit:@{@"event":@"ready"}];
}
- (void)showUpdateReleaseNotesWithDownloadData:(SPUDownloadData *)downloadData {}
- (void)showUpdateReleaseNotesFailedToDownloadWithError:(NSError *)error {}
- (void)showUpdateNotFoundWithError:(NSError *)error acknowledgement:(void (^)(void))acknowledgement {
    if (self.report && !self.paused) [self emit:@{@"event":@"current"}];
    acknowledgement();
}
- (void)showUpdaterError:(NSError *)error acknowledgement:(void (^)(void))acknowledgement {
    [self reportUpdateError:error];
    self.approved = NO; self.installHandler = nil;
    acknowledgement();
}
- (void)reportUpdateError:(NSError *)error {
    // Sparkle also completes a successful no-update check through didAbortWithError.
    // The user-driver callback already reports that result as "current" when requested.
    if ([error.domain isEqualToString:SUSparkleErrorDomain] && error.code == SUNoUpdateError) return;
    if (self.paused || self.errorReported || !(self.report || self.choice || self.received || self.approved)) return;
    self.errorReported = YES;
    if ([error.domain isEqualToString:SUSparkleErrorDomain] && error.code == SUInstallationCanceledError) {
        [self emit:@{@"event":@"canceled"}];
    } else {
        [self emit:@{@"event":@"error", @"message":error.localizedDescription}];
    }
}
- (void)showDownloadInitiatedWithCancellation:(void (^)(void))cancellation {
    self.cancellation = cancellation; self.received = 0; self.total = 0; self.extracting = NO; [self progress];
}
- (void)showDownloadDidReceiveExpectedContentLength:(uint64_t)length { self.total = length; [self progress]; }
- (void)showDownloadDidReceiveDataOfLength:(uint64_t)length { self.received += length; [self progress]; }
- (void)showDownloadDidStartExtractingUpdate { self.cancellation = nil; self.extracting = YES; [self progress]; }
- (void)showExtractionReceivedProgress:(double)progress {}
- (void)showReadyToInstallAndRelaunch:(void (^)(SPUUserUpdateChoice))reply {
    if (self.paused) { reply(SPUUserUpdateChoiceSkip); return; }
    self.choice = reply; self.ready = YES; [self emit:@{@"event":@"ready"}];
}
- (void)showInstallingUpdateWithApplicationTerminated:(BOOL)terminated retryTerminatingApplication:(void (^)(void))retry {}
- (void)showUpdateInstalledAndRelaunched:(BOOL)relaunched acknowledgement:(void (^)(void))acknowledgement { acknowledgement(); }
- (void)dismissUpdateInstallation { self.choice = nil; self.cancellation = nil; self.ready = NO; self.extracting = NO; }
- (BOOL)updater:(SPUUpdater *)updater shouldDownloadReleaseNotesForUpdate:(SUAppcastItem *)item { return NO; }
- (NSString *)feedURLStringForUpdater:(SPUUpdater *)updater { return self.selectedFeed; }
- (BOOL)updater:(SPUUpdater *)updater shouldPostponeRelaunchForUpdate:(SUAppcastItem *)item untilInvokingBlock:(void (^)(void))handler {
    if (!self.approved || self.installHandler) return NO;
    self.installHandler = handler;
    [self emit:@{@"event":@"shutdown"}];
    return YES;
}
- (void)updater:(SPUUpdater *)updater didAbortWithError:(NSError *)error {
    [self reportUpdateError:error];
    self.approved = NO; self.installHandler = nil;
}
@end

bool bmz_sparkle_available(void) {
    return [[NSBundle mainBundle] objectForInfoDictionaryKey:@"SUPublicEDKey"] != nil;
}

void bmz_sparkle_check(bool prerelease, bool report) {
    NSCAssert([NSThread isMainThread], @"Sparkle requires the main thread");
    NSString *key = prerelease ? @"BMZPrereleaseFeedURL" : @"SUFeedURL";
    if (!driver) {
        driver = [BMZUpdater new]; driver.events = [NSMutableArray new];
        driver.selectedFeed = [[NSBundle mainBundle] objectForInfoDictionaryKey:key];
        driver.updater = [[SPUUpdater alloc] initWithHostBundle:[NSBundle mainBundle] applicationBundle:[NSBundle mainBundle] userDriver:driver delegate:driver];
        driver.updater.automaticallyChecksForUpdates = NO;
        driver.updater.automaticallyDownloadsUpdates = NO;
        driver.updater.sendsSystemProfile = NO;
        NSError *error = nil;
        if (![driver.updater startUpdater:&error]) { [driver emit:@{@"event":@"error", @"message":error.localizedDescription}]; return; }
    }
    if (driver.updater.sessionInProgress) return;
    driver.report = report; driver.paused = NO; driver.approved = NO; driver.errorReported = NO;
    driver.selectedFeed = [[NSBundle mainBundle] objectForInfoDictionaryKey:key];
    // BMZ owns scheduling and suppression, even for automatic startup checks.
    [driver.updater checkForUpdates];
}

void bmz_sparkle_action(int action) {
    if (!driver) return;
    if (action == 3 && driver.approved) return;
    if (action == 0 || action == 3) {
        if (action == 3 && driver.paused) return;
        driver.paused = YES;
        [driver.events removeAllObjects];
        BOOL hadWork = driver.choice != nil || driver.cancellation != nil || driver.extracting;
        if (driver.cancellation) { void (^cancel)(void) = driver.cancellation; driver.cancellation = nil; cancel(); }
        if (driver.choice) {
            void (^reply)(SPUUserUpdateChoice) = driver.choice; driver.choice = nil;
            // Dismiss at Ready installs on quit. Skip cancels that pending installation.
            reply(driver.ready ? SPUUserUpdateChoiceSkip : SPUUserUpdateChoiceDismiss);
        }
        if (hadWork) [driver emit:@{@"event":@"canceled"}];
        return;
    }
    if (!driver.choice) { if (action == 1) { driver.paused = NO; driver.report = YES; driver.errorReported = NO; [driver.updater checkForUpdates]; } return; }
    if ((action == 1 && driver.ready) || (action == 2 && !driver.ready)) return;
    driver.approved = action == 2;
    driver.report = YES;
    void (^reply)(SPUUserUpdateChoice) = driver.choice; driver.choice = nil;
    reply(SPUUserUpdateChoiceInstall);
}

const char *bmz_sparkle_poll(void) {
    if (!driver.events.count) return NULL;
    NSDictionary *event = driver.events.firstObject; [driver.events removeObjectAtIndex:0];
    NSData *json = [NSJSONSerialization dataWithJSONObject:event options:0 error:nil];
    lastEvent = [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
    return lastEvent.UTF8String;
}

bool bmz_sparkle_resume_install(void) {
    NSCAssert([NSThread isMainThread], @"Sparkle requires the main thread");
    if (!driver.approved || !driver.installHandler) return false;
    void (^handler)(void) = driver.installHandler; driver.installHandler = nil;
    handler();
    return true;
}

// Regression harness entry points. They exercise the same stored-handler path while a real
// winit EventLoop owns NSApplication and its delegate; no updater session or network is needed.
void bmz_sparkle_test_stage_install_handler(void) {
    if (!driver) { driver = [BMZUpdater new]; driver.events = [NSMutableArray new]; }
    driver.approved = YES;
    testInstallHandlerInvoked = NO;
    driver.installHandler = ^{
        testInstallHandlerInvoked = YES;
    };
    [driver emit:@{@"event":@"shutdown"}];
}

bool bmz_sparkle_test_install_handler_invoked(void) { return testInstallHandlerInvoked; }
