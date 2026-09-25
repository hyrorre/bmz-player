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

int main(void) {
    @autoreleasepool {
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
