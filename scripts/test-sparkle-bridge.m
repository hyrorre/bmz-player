// Compile against the pinned SDK to exercise the actual Objective-C/JSON boundary.
#import "../crates/bmz-player/native/sparkle.m"
#include <assert.h>

@interface BMZTestItem : NSObject
@property(getter=isInformationOnlyUpdate) BOOL informationOnlyUpdate;
@property(copy) NSString *displayVersionString;
@end
@implementation BMZTestItem
@end

@interface BMZTestState : NSObject
@property SPUUserUpdateStage stage;
@end
@implementation BMZTestState
@end

static void assert_event(NSString *expected) {
    const char *json = bmz_sparkle_poll();
    assert(json != NULL);
    NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *event = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    assert([event[@"event"] isEqualToString:expected]);
}

static void test_update_results(void) {
    // Delegate result handling never sends messages to the updater itself.
    SPUUpdater *updater = (id)[NSObject new];
    NSError *noUpdate = [NSError errorWithDomain:SUSparkleErrorDomain code:SUNoUpdateError userInfo:nil];
    NSError *failure = [NSError errorWithDomain:SUSparkleErrorDomain code:SUDownloadError userInfo:nil];
    for (int report = 0; report < 2; ++report) {
        driver = [BMZUpdater new]; driver.events = [NSMutableArray new]; driver.report = report;
        // Sparkle acknowledges the user-driver result before notifying its delegate.
        [driver showUpdateNotFoundWithError:noUpdate acknowledgement:^{
            [driver updater:updater didAbortWithError:noUpdate];
        }];
        if (report) assert_event(@"current");
        assert(bmz_sparkle_poll() == NULL);
        [driver showUpdaterError:failure acknowledgement:^{
            [driver dismissUpdateInstallation];
            [driver updater:updater didAbortWithError:failure];
        }];
        if (report) assert_event(@"error");
        assert(bmz_sparkle_poll() == NULL);
    }

    driver = [BMZUpdater new]; driver.events = [NSMutableArray new];
    driver.approved = YES; driver.installHandler = ^{};
    [driver updater:updater didAbortWithError:failure];
    assert_event(@"error");
    assert(!driver.approved && driver.installHandler == nil);
    assert(bmz_sparkle_poll() == NULL);

    driver = [BMZUpdater new]; driver.events = [NSMutableArray new]; driver.report = YES;
    NSError *canceled = [NSError errorWithDomain:SUSparkleErrorDomain code:SUInstallationCanceledError userInfo:nil];
    [driver updater:updater didAbortWithError:canceled];
    assert_event(@"canceled");
    assert(bmz_sparkle_poll() == NULL);
    driver.errorReported = NO; driver.paused = YES;
    [driver updater:updater didAbortWithError:failure];
    assert(bmz_sparkle_poll() == NULL);
}

int main(void) {
    @autoreleasepool {
        test_update_results();
        driver = [BMZUpdater new];
        driver.events = [NSMutableArray new];
        BMZTestItem *item = [BMZTestItem new];
        item.displayVersionString = @"0.5.0";
        BMZTestState *state = [BMZTestState new];
        for (int informational = 0; informational < 2; ++informational) {
            item.informationOnlyUpdate = informational;
            state.stage = SPUUserUpdateStageNotDownloaded;
            [driver showUpdateFoundWithAppcastItem:(id)item state:(id)state reply:^(SPUUserUpdateChoice choice) {}];
            const char *json = bmz_sparkle_poll();
            assert(json != NULL);
            NSData *data = [[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding];
            NSDictionary *event = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
            NSNumber *installable = event[@"installable"];
            assert(CFGetTypeID((__bridge CFTypeRef)installable) == CFBooleanGetTypeID());
            assert(installable.boolValue == !informational);
        }
        driver.paused = YES;
        state.stage = SPUUserUpdateStageInstalling;
        __block BOOL canceled = NO;
        [driver showUpdateFoundWithAppcastItem:(id)item state:(id)state reply:^(SPUUserUpdateChoice choice) {
            canceled = choice == SPUUserUpdateChoiceSkip;
        }];
        assert(canceled);

        // The relaunch continuation is consumed exactly once. It must not replace the
        // NSApplication delegate or start a second Cocoa run loop.
        [NSApplication sharedApplication];
        NSObject *delegate = [NSObject new];
        [NSApp setDelegate:(id)delegate];
        bmz_sparkle_test_stage_install_handler();
        assert([NSApp delegate] == (id)delegate);
        assert(bmz_sparkle_resume_install());
        assert(!bmz_sparkle_resume_install());
        assert(bmz_sparkle_test_install_handler_invoked());
        assert([NSApp delegate] == (id)delegate);
        puts("Sparkle bridge contract tests passed");
    }
    return 0;
}
