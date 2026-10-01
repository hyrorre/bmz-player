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

// kind: 1 connect, 2 disconnect, 3 button, 4 axis, 5 history gap,
// 6 timer, 8 held-button baseline, 9 axis baseline.
typedef void (*BMZGCPadEvent)(void *, uint64_t, uint32_t, int32_t, uint32_t, float, double, const char *);

API_AVAILABLE(macos(11.0))
@interface BMZGCPads : NSObject
@property (nonatomic, strong) dispatch_queue_t queue;
@property (nonatomic, strong) dispatch_source_t timer;
@property (nonatomic, strong) NSMutableDictionary<NSNumber *, GCController *> *controllers;
@property (nonatomic, strong) NSMutableArray<id> *observers;
@property void *context;
@property BMZGCPadEvent callback;
@property uint64_t generation;
@property uint32_t nextDevice;
@property BOOL closed;
- (void)attach:(GCController *)controller;
- (void)install:(GCController *)controller device:(uint32_t)device;
- (void)clear:(GCController *)controller;
- (void)consume:(GCControllerInputState<GCDevicePhysicalInputStateDiff> *)state
         device:(uint32_t)device baseline:(BOOL)baseline API_AVAILABLE(macos(14.0));
@end

@implementation BMZGCPads
- (void)clear:(GCController *)controller {
    if (@available(macOS 14.0, *)) {
        GCControllerLiveInput *input = controller.input.unmappedInput ?: controller.input;
        input.inputStateAvailableHandler = nil;
    } else {
        for (GCControllerButtonInput *button in controller.physicalInputProfile.buttons.allValues) {
            button.pressedChangedHandler = nil;
            button.valueChangedHandler = nil;
        }
        GCControllerDirectionPad *dpad = controller.physicalInputProfile.dpads[GCInputDirectionPad];
        dpad.up.pressedChangedHandler = nil;
        dpad.down.pressedChangedHandler = nil;
        dpad.left.pressedChangedHandler = nil;
        dpad.right.pressedChangedHandler = nil;
        for (GCControllerAxisInput *axis in controller.physicalInputProfile.axes.allValues)
            axis.valueChangedHandler = nil;
    }
}
- (void)attach:(GCController *)controller {
    if ([self.controllers.allValues containsObject:controller]) return;
    uint32_t device = ++self.nextDevice;
    self.controllers[@(device)] = controller;
    BOOL modern = NO;
    if (@available(macOS 14.0, *)) modern = YES;
    NSString *name = controller.vendorName ?: controller.productCategory;
    self.callback(self.context, self.generation, device, 1, modern ? 1 : 0, 0, 0, name.UTF8String);
    [self install:controller device:device];
}
- (void)consume:(GCControllerInputState<GCDevicePhysicalInputStateDiff> *)state
         device:(uint32_t)device baseline:(BOOL)baseline {
    if (@available(macOS 14.0, *)) {
        NSArray<GCInputButtonName> *names = @[GCInputButtonA, GCInputButtonB, GCInputButtonX, GCInputButtonY,
            GCInputLeftShoulder, GCInputRightShoulder, GCInputLeftTrigger, GCInputRightTrigger,
            GCInputLeftThumbstickButton, GCInputRightThumbstickButton,
            GCInputButtonMenu, GCInputButtonOptions, GCInputButtonHome];
        for (uint32_t i = 0; i < names.count; ++i) {
            id<GCButtonElement> button = state.buttons[names[i]];
            if (button && (baseline || [state changeForElement:button] == GCDevicePhysicalInputElementChanged)) {
                self.callback(self.context, self.generation, device, baseline ? 8 : 3, i,
                    button.pressedInput.isPressed ? 1 : 0, button.pressedInput.lastPressedStateTimestamp, NULL);
                if (i == 6 || i == 7) self.callback(self.context, self.generation, device, baseline ? 9 : 4,
                    i - 2, button.pressedInput.value, button.pressedInput.lastValueTimestamp, NULL);
            }
        }
        id<GCDirectionPadElement> dpad = state.dpads[GCInputDirectionPad];
        if (dpad && (baseline || [state changeForElement:dpad] == GCDevicePhysicalInputElementChanged)) {
            NSArray<id<GCPressedStateInput>> *directions = @[dpad.up, dpad.down, dpad.left, dpad.right];
            for (uint32_t i = 0; i < directions.count; ++i) {
                id<GCPressedStateInput> input = directions[i];
                self.callback(self.context, self.generation, device, baseline ? 8 : 3, 13 + i,
                    input.isPressed ? 1 : 0, input.lastPressedStateTimestamp, NULL);
            }
        }
        NSArray<GCInputDirectionPadName> *sticks = @[GCInputLeftThumbstick, GCInputRightThumbstick];
        for (uint32_t i = 0; i < sticks.count; ++i) {
            id<GCDirectionPadElement> stick = state.dpads[sticks[i]];
            if (stick && (baseline || [state changeForElement:stick] == GCDevicePhysicalInputElementChanged)) {
                self.callback(self.context, self.generation, device, baseline ? 9 : 4,
                    i * 2, stick.xAxis.value, stick.xAxis.lastValueTimestamp, NULL);
                self.callback(self.context, self.generation, device, baseline ? 9 : 4,
                    i * 2 + 1, stick.yAxis.value, stick.yAxis.lastValueTimestamp, NULL);
            }
        }
    }
}
- (void)install:(GCController *)controller device:(uint32_t)device {
    [self clear:controller];
    controller.handlerQueue = self.queue;
    uint64_t generation = self.generation;
    __weak BMZGCPads *weakSelf = self;
    if (@available(macOS 14.0, *)) {
        GCControllerLiveInput *input = controller.input.unmappedInput ?: controller.input;
        input.queue = self.queue;
        input.inputStateQueueDepth = 128;
        while ([input nextInputState]) {} // discard the previous route's history
        [self consume:(id)[input capture] device:device baseline:YES];
        input.inputStateAvailableHandler = ^(id<GCDevicePhysicalInput> physical) {
            BMZGCPads *owner = weakSelf;
            if (!owner || owner.closed || owner.generation != generation || !owner.controllers[@(device)]) return;
            GCControllerInputState<GCDevicePhysicalInputStateDiff> *state;
            while ((state = (id)[physical nextInputState])) {
                BOOL gap = state.changedElements == nil;
                if (gap) owner.callback(owner.context, generation, device, 5, 0, 0, state.lastEventTimestamp, NULL);
                [owner consume:state device:device baseline:gap];
            }
        };
    } else {
        // Individual callbacks carry the scheduled value, unlike reading a
        // mutable profile in a profile-wide callback. Timestamps are receipt times.
        GCPhysicalInputProfile *profile = controller.physicalInputProfile;
        NSArray<NSString *> *names = @[GCInputButtonA, GCInputButtonB, GCInputButtonX, GCInputButtonY,
            GCInputLeftShoulder, GCInputRightShoulder, GCInputLeftTrigger, GCInputRightTrigger,
            GCInputLeftThumbstickButton, GCInputRightThumbstickButton,
            GCInputButtonMenu, GCInputButtonOptions, GCInputButtonHome];
        for (uint32_t i = 0; i < names.count; ++i) {
            GCControllerButtonInput *button = profile.buttons[names[i]];
            if (!button) continue;
            self.callback(self.context, generation, device, 8, i, button.isPressed ? 1 : 0, 0, NULL);
            button.pressedChangedHandler = ^(GCControllerButtonInput *element, float value, BOOL pressed) {
                (void)element; (void)value;
                BMZGCPads *owner = weakSelf;
                if (owner && !owner.closed && owner.generation == generation && owner.controllers[@(device)])
                    owner.callback(owner.context, generation, device, 3, i, pressed ? 1 : 0, 0, NULL);
            };
        }
        GCControllerDirectionPad *dpad = profile.dpads[GCInputDirectionPad];
        NSArray<GCControllerButtonInput *> *directions = dpad ? @[dpad.up, dpad.down, dpad.left, dpad.right] : @[];
        for (uint32_t i = 0; i < directions.count; ++i) {
            GCControllerButtonInput *button = directions[i];
            uint32_t code = 13 + i;
            self.callback(self.context, generation, device, 8, code, button.isPressed ? 1 : 0, 0, NULL);
            button.pressedChangedHandler = ^(GCControllerButtonInput *element, float value, BOOL pressed) {
                (void)element; (void)value;
                BMZGCPads *owner = weakSelf;
                if (owner && !owner.closed && owner.generation == generation && owner.controllers[@(device)])
                    owner.callback(owner.context, generation, device, 3, code, pressed ? 1 : 0, 0, NULL);
            };
        }
        NSArray<NSString *> *sticks = @[GCInputLeftThumbstick, GCInputRightThumbstick];
        for (uint32_t i = 0; i < sticks.count; ++i) {
            GCControllerDirectionPad *stick = profile.dpads[sticks[i]];
            NSArray *axes = stick ? @[stick.xAxis, stick.yAxis] : @[];
            for (uint32_t j = 0; j < axes.count; ++j) {
                GCControllerAxisInput *axis = axes[j];
                uint32_t code = i * 2 + j;
                self.callback(self.context, generation, device, 9, code, axis.value, 0, NULL);
                axis.valueChangedHandler = ^(GCControllerAxisInput *element, float value) {
                    (void)element;
                    BMZGCPads *owner = weakSelf;
                    if (owner && !owner.closed && owner.generation == generation && owner.controllers[@(device)])
                        owner.callback(owner.context, generation, device, 4, code, value, 0, NULL);
                };
            }
        }
        for (uint32_t i = 0; i < 2; ++i) {
            GCControllerButtonInput *trigger = profile.buttons[i == 0 ? GCInputLeftTrigger : GCInputRightTrigger];
            if (!trigger) continue;
            self.callback(self.context, generation, device, 9, 4 + i, trigger.value, 0, NULL);
            trigger.valueChangedHandler = ^(GCControllerButtonInput *element, float value, BOOL pressed) {
                (void)element; (void)pressed;
                BMZGCPads *owner = weakSelf;
                if (owner && !owner.closed && owner.generation == generation && owner.controllers[@(device)])
                    owner.callback(owner.context, generation, device, 4, 4 + i, value, 0, NULL);
            };
        }
    }
}
@end

void *bmz_gc_pads_open(void *context, BMZGCPadEvent callback) {
    if (@available(macOS 11.0, *)) {
        BMZGCPads *owner = [BMZGCPads new];
        owner.queue = dispatch_queue_create("bmz-gc-pads", DISPATCH_QUEUE_SERIAL);
        owner.context = context;
        owner.callback = callback;
        owner.controllers = [NSMutableDictionary new];
        owner.observers = [NSMutableArray new];
        __weak BMZGCPads *weakOwner = owner;
        for (NSString *name in @[GCControllerDidConnectNotification, GCControllerDidDisconnectNotification]) {
            id token = [NSNotificationCenter.defaultCenter addObserverForName:name object:nil queue:nil
                usingBlock:^(NSNotification *notification) {
                    BMZGCPads *live = weakOwner;
                    if (!live) return;
                    GCController *controller = notification.object;
                    BOOL connect = [notification.name isEqualToString:GCControllerDidConnectNotification];
                    dispatch_async(live.queue, ^{
                        BMZGCPads *current = weakOwner;
                        if (!current || current.closed) return;
                        if (connect) [current attach:controller];
                        else {
                            NSNumber *device = [current.controllers allKeysForObject:controller].firstObject;
                            if (!device) return;
                            [current clear:controller];
                            [current.controllers removeObjectForKey:device];
                            current.callback(current.context, current.generation, device.unsignedIntValue, 2, 0, 0, 0, NULL);
                        }
                    });
                }];
            [owner.observers addObject:token];
        }
        dispatch_sync(owner.queue, ^{ for (GCController *controller in GCController.controllers) [owner attach:controller]; });
        owner.timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, owner.queue);
        dispatch_source_set_timer(owner.timer, dispatch_time(DISPATCH_TIME_NOW, 0), NSEC_PER_MSEC, NSEC_PER_MSEC / 4);
        dispatch_source_set_event_handler(owner.timer, ^{
            BMZGCPads *current = weakOwner;
            if (current && !current.closed) current.callback(current.context, current.generation, 0, 6, 0, 0, 0, NULL);
        });
        dispatch_resume(owner.timer);
        return (__bridge_retained void *)owner;
    }
    return NULL;
}
void bmz_gc_pads_generation(void *handle, uint64_t generation) {
    if (@available(macOS 11.0, *)) {
        BMZGCPads *owner = (__bridge BMZGCPads *)handle;
        dispatch_sync(owner.queue, ^{
            owner.generation = generation;
            for (NSNumber *device in owner.controllers) [owner install:owner.controllers[device] device:device.unsignedIntValue];
        });
    }
}
void bmz_gc_pads_close(void *handle) {
    if (@available(macOS 11.0, *)) {
        BMZGCPads *owner = (__bridge_transfer BMZGCPads *)handle;
        for (id token in owner.observers) [NSNotificationCenter.defaultCenter removeObserver:token];
        dispatch_sync(owner.queue, ^{
            owner.closed = YES;
            dispatch_source_cancel(owner.timer);
            for (GCController *controller in owner.controllers.allValues) [owner clear:controller];
            [owner.controllers removeAllObjects];
        });
    }
}
