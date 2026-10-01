// Framework objects and handlers are owned by a serial queue. Weak block
// captures plus a synchronous close fence keep Rust callback storage alive.
#import <Foundation/Foundation.h>
#import <GameController/GameController.h>
#include <stdint.h>

typedef void (*BMZGCKeyEvent)(void *, uint64_t, uint32_t, int32_t);

API_AVAILABLE(macos(11.0))
@interface BMZGCKeyboard : NSObject
@property (nonatomic, strong) dispatch_queue_t queue;
@property (nonatomic, strong) dispatch_source_t timer;
@property (nonatomic, strong) GCKeyboard *keyboard;
@property (nonatomic, strong) NSMutableArray<id> *observers;
@property (nonatomic, strong) NSArray<NSNumber *> *codes;
@property void *context;
@property BMZGCKeyEvent callback;
@property uint64_t generation;
@property uint64_t attachment;
@property BOOL closed;
- (void)install;
- (void)detach;
@end

@implementation BMZGCKeyboard
- (void)detach {
    ++self.attachment;
    for (NSNumber *code in self.codes) {
        [self.keyboard.keyboardInput buttonForKeyCode:code.longValue].pressedChangedHandler = nil;
    }
    self.keyboard = nil;
    self.callback(self.context, self.generation, 0, -1);
}
- (void)install {
    [self detach];
    self.keyboard = GCKeyboard.coalescedKeyboard;
    if (!self.keyboard) return;
    self.keyboard.handlerQueue = self.queue;
    uint64_t generation = self.generation, attachment = self.attachment;
    __weak BMZGCKeyboard *weakSelf = self;
    self.callback(self.context, generation, 0, -10);
    for (NSNumber *code in self.codes) {
        GCControllerButtonInput *button = [self.keyboard.keyboardInput buttonForKeyCode:code.longValue];
        if (!button) continue;
        uint32_t usage = code.unsignedIntValue;
        self.callback(self.context, generation, usage, -11);
        // Held keys across retry/focus/backend changes need a fresh release
        // before they can start another gameplay press.
        __block BOOL armed = !button.isPressed;
        button.pressedChangedHandler = ^(GCControllerButtonInput *key, float value, BOOL pressed) {
            (void)key; (void)value;
            BMZGCKeyboard *owner = weakSelf;
            if (!owner || owner.closed || owner.generation != generation || owner.attachment != attachment) return;
            if (!pressed) armed = YES;
            if (armed) owner.callback(owner.context, generation, usage, pressed ? 1 : 0);
        };
    }
    self.callback(self.context, generation, 0, -12);
}
@end

void *bmz_gc_keyboard_open(void *context, BMZGCKeyEvent callback,
                           const uint32_t *codes, size_t count) {
    if (@available(macOS 11.0, *)) {
        BMZGCKeyboard *owner = [BMZGCKeyboard new];
        owner.queue = dispatch_queue_create("bmz-gc-keyboard", DISPATCH_QUEUE_SERIAL);
        owner.context = context;
        owner.callback = callback;
        NSMutableArray *list = [NSMutableArray new];
        for (size_t i = 0; i < count; ++i) [list addObject:@(codes[i])];
        owner.codes = list;
        owner.observers = [NSMutableArray new];
        __weak BMZGCKeyboard *weakOwner = owner;
        for (NSString *name in @[GCKeyboardDidConnectNotification, GCKeyboardDidDisconnectNotification]) {
            id token = [NSNotificationCenter.defaultCenter addObserverForName:name object:nil queue:nil
                usingBlock:^(NSNotification *notification) {
                    (void)notification;
                    BMZGCKeyboard *live = weakOwner;
                    if (!live) return;
                    dispatch_async(live.queue, ^{
                        BMZGCKeyboard *current = weakOwner;
                        if (current && !current.closed) [current install];
                    });
                }];
            [owner.observers addObject:token];
        }
        dispatch_sync(owner.queue, ^{ [owner install]; });
        owner.timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, owner.queue);
        dispatch_source_set_timer(owner.timer, dispatch_time(DISPATCH_TIME_NOW, 0),
                                 10 * NSEC_PER_MSEC, NSEC_PER_MSEC);
        dispatch_source_set_event_handler(owner.timer, ^{
            BMZGCKeyboard *current = weakOwner;
            if (current && !current.closed) current.callback(current.context, current.generation, 0, -3);
        });
        dispatch_resume(owner.timer);
        return (__bridge_retained void *)owner;
    }
    return NULL;
}

void bmz_gc_keyboard_generation(void *handle, uint64_t generation) {
    if (@available(macOS 11.0, *)) {
        BMZGCKeyboard *owner = (__bridge BMZGCKeyboard *)handle;
        dispatch_sync(owner.queue, ^{
            owner.generation = generation;
            [owner install];
        });
    }
}

void bmz_gc_keyboard_close(void *handle) {
    if (@available(macOS 11.0, *)) {
        BMZGCKeyboard *owner = (__bridge_transfer BMZGCKeyboard *)handle;
        for (id token in owner.observers) [NSNotificationCenter.defaultCenter removeObserver:token];
        dispatch_sync(owner.queue, ^{
            owner.closed = YES;
            dispatch_source_cancel(owner.timer);
            [owner detach];
        });
        // All blocks that can use context run on queue and check closed. Queued
        // notifications retain no Rust storage and cannot reinstall handlers.
    }
}
