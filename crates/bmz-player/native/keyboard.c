// All manager operations and callbacks run on the owning capture thread.
// Public APIs only; no AppKit application/delegate and no exclusive device open.
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/hid/IOHIDManager.h>
#include <IOKit/hidsystem/IOHIDLib.h>
#include <stdlib.h>

typedef void (*BMZKeyboardEvent)(void *, uintptr_t, uint32_t, uint64_t, int);
typedef struct {
    IOHIDManagerRef manager;
    BMZKeyboardEvent callback;
    void *context;
} BMZKeyboard;

int bmz_keyboard_access(void) {
    if (__builtin_available(macOS 10.15, *))
        return IOHIDCheckAccess(kIOHIDRequestTypeListenEvent);
    return 0;
}
void bmz_keyboard_request_access(void) {
    if (__builtin_available(macOS 10.15, *))
        (void)IOHIDRequestAccess(kIOHIDRequestTypeListenEvent);
}
static void value(void *ctx, IOReturn result, void *sender, IOHIDValueRef value) {
    (void)sender;
    BMZKeyboard *keyboard = ctx;
    if (result != kIOReturnSuccess) {
        keyboard->callback(keyboard->context, 0, 0, 0, -2);
        return;
    }
    IOHIDElementRef element = IOHIDValueGetElement(value);
    if (IOHIDElementGetUsagePage(element) != 7) {
        if (IOHIDValueGetIntegerValue(value) != 0)
            keyboard->callback(keyboard->context, 0, 0xffff, 0, 1);
        return;
    }
    keyboard->callback(keyboard->context, (uintptr_t)IOHIDElementGetDevice(element),
        IOHIDElementGetUsage(element), IOHIDValueGetTimeStamp(value),
        IOHIDValueGetIntegerValue(value) != 0);
}
static void removed(void *ctx, IOReturn result, void *sender, IOHIDDeviceRef device) {
    (void)result; (void)sender;
    BMZKeyboard *keyboard = ctx;
    keyboard->callback(keyboard->context, (uintptr_t)device, 0, 0, -1);
}
void *bmz_keyboard_open(void *ctx, BMZKeyboardEvent callback, int32_t *error) {
    *error = kIOReturnNotPermitted;
    // Check without prompting BEFORE opening protected devices.
    if (bmz_keyboard_access() != 0) return NULL;
    *error = kIOReturnNoMemory;
    BMZKeyboard *keyboard = calloc(1, sizeof(*keyboard));
    if (!keyboard) return NULL;
    keyboard->manager = IOHIDManagerCreate(kCFAllocatorDefault, 0);
    if (!keyboard->manager) { free(keyboard); return NULL; }
    keyboard->callback = callback;
    keyboard->context = ctx;
    int page = 1, usage = 6;
    CFNumberRef pageNumber = CFNumberCreate(NULL, kCFNumberIntType, &page);
    CFNumberRef usageNumber = CFNumberCreate(NULL, kCFNumberIntType, &usage);
    const void *keys[] = { CFSTR(kIOHIDDeviceUsagePageKey), CFSTR(kIOHIDDeviceUsageKey) };
    const void *values[] = { pageNumber, usageNumber };
    CFDictionaryRef match = CFDictionaryCreate(NULL, keys, values, 2,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    IOHIDManagerSetDeviceMatching(keyboard->manager, match);
    CFRelease(match); CFRelease(pageNumber); CFRelease(usageNumber);
    IOHIDManagerRegisterInputValueCallback(keyboard->manager, value, keyboard);
    IOHIDManagerRegisterDeviceRemovalCallback(keyboard->manager, removed, keyboard);
    IOHIDManagerScheduleWithRunLoop(keyboard->manager, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
    *error = IOHIDManagerOpen(keyboard->manager, 0);
    if (*error != kIOReturnSuccess) {
        IOHIDManagerUnscheduleFromRunLoop(keyboard->manager, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
        IOHIDManagerClose(keyboard->manager, 0);
        CFRelease(keyboard->manager); free(keyboard); return NULL;
    }
    return keyboard;
}
void bmz_keyboard_wait(void) {
    // Event-driven wait; timeout is only for focus/permission housekeeping.
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, true);
}
void bmz_keyboard_close(void *ptr) {
    BMZKeyboard *keyboard = ptr;
    IOHIDManagerUnscheduleFromRunLoop(keyboard->manager, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
    IOHIDManagerRegisterInputValueCallback(keyboard->manager, NULL, NULL);
    IOHIDManagerRegisterDeviceRemovalCallback(keyboard->manager, NULL, NULL);
    IOHIDManagerClose(keyboard->manager, 0);
    CFRelease(keyboard->manager);
    free(keyboard);
}
