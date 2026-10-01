// Foreground and Mach clock helpers shared by GameController and optional IOHID.
// Public APIs only; no AppKit calls from the input capture thread.
#include <CoreFoundation/CoreFoundation.h>
#include <Carbon/Carbon.h>
#include <mach/mach_time.h>
#include <unistd.h>

int bmz_keyboard_foreground(void) {
    ProcessSerialNumber process;
    pid_t pid = 0;
    if (GetFrontProcess(&process) != noErr ||
        GetProcessPID(&process, &pid) != noErr || pid != getpid()) return 0;
    // WindowServer metadata (no image capture or Accessibility). Reject a
    // minimized/hidden application even if it remains the front process while
    // winit is stalled. No AppKit calls are made from this thread.
    CFArrayRef windows = CGWindowListCopyWindowInfo(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID);
    if (!windows) return 0;
    int visible = 0;
    for (CFIndex i = 0; i < CFArrayGetCount(windows); ++i) {
        CFDictionaryRef window = CFArrayGetValueAtIndex(windows, i);
        CFNumberRef owner = CFDictionaryGetValue(window, kCGWindowOwnerPID);
        CFNumberRef layer = CFDictionaryGetValue(window, kCGWindowLayer);
        int ownerPID = 0, windowLayer = -1;
        if (owner && layer && CFNumberGetValue(owner, kCFNumberIntType, &ownerPID) &&
            CFNumberGetValue(layer, kCFNumberIntType, &windowLayer) && ownerPID == pid && windowLayer == 0) {
            visible = 1; break;
        }
    }
    CFRelease(windows);
    return visible;
}
uint64_t bmz_keyboard_ticks(uint32_t *numer, uint32_t *denom) {
    mach_timebase_info_data_t info;
    if (mach_timebase_info(&info) != KERN_SUCCESS) return 0;
    *numer = info.numer;
    *denom = info.denom;
    return mach_absolute_time();
}
