//! Keep repaint requests alive while Windows runs its move/resize modal loop.
//!
//! That loop dispatches window messages but does not run winit's normal
//! `WaitUntil` / `about_to_wait` scheduling. A window timer requests WM_PAINT;
//! winit still delivers RedrawRequested and the existing frame limiter decides
//! whether to render. A skipped frame cannot stop the periodic requests.

use anyhow::{Context, Result, bail};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::InvalidateRect,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            KillTimer, SetTimer, USER_TIMER_MINIMUM, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE,
            WM_NCDESTROY, WM_TIMER,
        },
    },
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

// Use a process-unique address instead of a small timer ID used by native code.
static TIMER_ID: u8 = 0;

fn timer_id() -> usize {
    std::ptr::addr_of!(TIMER_ID) as usize
}

pub(super) fn install(window: &Window) -> Result<()> {
    let handle = window.window_handle().context("window handle unavailable")?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        bail!("expected a Win32 window");
    };
    // SAFETY: called on the thread that just created this live window. The
    // subclass owns no borrowed state and removes itself on WM_NCDESTROY.
    if unsafe { SetWindowSubclass(handle.hwnd.get() as HWND, Some(window_proc), timer_id(), 0) }
        == 0
    {
        bail!("SetWindowSubclass failed");
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    subclass_id: usize,
    _reference_data: usize,
) -> LRESULT {
    // SAFETY: Windows invokes this procedure on the owning thread with a live
    // HWND. No Rust app state is borrowed and no synchronous rendering is done.
    unsafe {
        match message {
            WM_ENTERSIZEMOVE => {
                // This is a fallback wakeup, not a replacement FPS limiter.
                // Windows timers have a minimum 10 ms interval, so modal-loop
                // rendering need not reach the normal configured frame rate.
                if SetTimer(hwnd, timer_id(), USER_TIMER_MINIMUM, None) == 0 {
                    tracing::warn!("failed to start window move/resize redraw timer");
                }
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            WM_TIMER if wparam == timer_id() && lparam == 0 => {
                InvalidateRect(hwnd, std::ptr::null(), 0);
                return 0;
            }
            WM_EXITSIZEMOVE => {
                KillTimer(hwnd, timer_id());
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            WM_NCDESTROY => {
                KillTimer(hwnd, timer_id());
                RemoveWindowSubclass(hwnd, Some(window_proc), subclass_id);
            }
            _ => {}
        }
        // In particular, winit must still receive enter/exit and destroy.
        DefSubclassProc(hwnd, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::{
        Graphics::Gdi::{GetUpdateRect, ValidateRect},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW,
            SendMessageW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
        },
    };

    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn modal_timer_retries_repaint_and_stops_on_exit() {
        // Exercise real Win32 timer dispatch without a winit event loop or GPU.
        // WS_VISIBLE is required for an update region. Keep the tool window
        // offscreen and non-activating; it never touches app runtime data.
        unsafe {
            let window = TestWindow(CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                windows_sys::w!("STATIC"),
                windows_sys::w!("BMZ modal redraw test"),
                WS_POPUP | WS_VISIBLE,
                -32000,
                -32000,
                64,
                64,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            ));
            assert!(!window.0.is_null());
            assert_ne!(SetWindowSubclass(window.0, Some(window_proc), timer_id(), 0), 0);
            SendMessageW(window.0, WM_ENTERSIZEMOVE, 0, 0);

            for _ in 0..2 {
                // Simulate a handled paint, including one skipped by FPS pacing.
                ValidateRect(window.0, std::ptr::null());
                let deadline = Instant::now() + Duration::from_secs(2);
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, window.0, WM_TIMER, WM_TIMER, PM_REMOVE) == 0 {
                    assert!(Instant::now() < deadline, "modal redraw timer did not fire");
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert_eq!(message.wParam, timer_id());
                DispatchMessageW(&message);
                assert_ne!(GetUpdateRect(window.0, std::ptr::null_mut(), 0), 0);
            }

            SendMessageW(window.0, WM_EXITSIZEMOVE, 0, 0);
            // KillTimer returns zero when this HWND no longer owns the timer.
            assert_eq!(KillTimer(window.0, timer_id()), 0);
            ValidateRect(window.0, std::ptr::null());
            SendMessageW(window.0, WM_TIMER, timer_id() + 1, 0);
            assert_eq!(GetUpdateRect(window.0, std::ptr::null_mut(), 0), 0);
        }
    }
}
