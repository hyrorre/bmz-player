use std::ffi::c_void;
use std::ptr;

use super::RefreshRate;

#[repr(C)]
struct DisplayPeriod {
    time_value: i64,
    time_scale: i32,
    flags: i32,
}

const CV_TIME_IS_INDEFINITE: i32 = 1;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCopyDisplayMode(display: u32) -> *mut c_void;
    fn CGDisplayModeGetRefreshRate(mode: *mut c_void) -> f64;
    fn CGDisplayModeRelease(mode: *mut c_void);
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    fn CVDisplayLinkCreateWithCGDisplay(display: u32, link: *mut *mut c_void) -> i32;
    fn CVDisplayLinkGetNominalOutputVideoRefreshPeriod(link: *mut c_void) -> DisplayPeriod;
    fn CVDisplayLinkRelease(link: *mut c_void);
}

pub(super) fn current_refresh_rate(display: u32) -> Option<RefreshRate> {
    if display == 0 {
        return None;
    }
    // SAFETY: The copied mode is null for an invalid display ID. A non-null
    // result is owned here and remains valid until released after reading it.
    let hz = unsafe {
        let mode = CGDisplayCopyDisplayMode(display);
        if mode.is_null() {
            return None;
        }
        let hz = CGDisplayModeGetRefreshRate(mode);
        CGDisplayModeRelease(mode);
        hz
    };
    if hz > 0.0 {
        return from_display_mode_hz(hz);
    }

    // SAFETY: A successful creation returns an owned link in the out pointer.
    // Read the period while the link is live, then release it on this thread.
    let period = unsafe {
        let mut link = ptr::null_mut();
        if CVDisplayLinkCreateWithCGDisplay(display, &mut link) != 0 || link.is_null() {
            return None;
        }
        let period = CVDisplayLinkGetNominalOutputVideoRefreshPeriod(link);
        CVDisplayLinkRelease(link);
        period
    };
    period.refresh_rate()
}

fn from_display_mode_hz(hz: f64) -> Option<RefreshRate> {
    // Round upward so quantization cannot drop the rate to a lower integer FPS cap.
    let millihertz = (hz * 1_000.0).ceil();
    (1.0..=f64::from(u32::MAX))
        .contains(&millihertz)
        .then_some(RefreshRate { numerator: millihertz as u32, denominator: 1_000 })
}

impl DisplayPeriod {
    fn refresh_rate(self) -> Option<RefreshRate> {
        if self.flags & CV_TIME_IS_INDEFINITE != 0 {
            return None;
        }
        // CVTime represents seconds as time_value / time_scale. Invert the
        // fraction without dividing away the fractional Hz.
        let numerator = u32::try_from(self.time_scale).ok()?;
        let denominator = u32::try_from(self.time_value).ok()?;
        let rate = RefreshRate { numerator, denominator };
        rate.is_valid().then_some(rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_display_period_preserves_fractional_fps_boundary() {
        for (time_scale, time_value, below, covered) in
            [(60_000, 1_001, 59, 60), (240_084, 1_000, 240, 241)]
        {
            let rate = DisplayPeriod { time_scale, time_value, flags: 0 }.refresh_rate().unwrap();
            assert_eq!(
                rate,
                RefreshRate { numerator: time_scale as u32, denominator: time_value as u32 }
            );
            assert!(!rate.covered_by(below));
            assert!(rate.covered_by(covered));
        }
    }

    #[test]
    fn invalid_nominal_display_period_leaves_rate_unknown() {
        for (time_scale, time_value, flags) in [
            (60_000, 1_001, CV_TIME_IS_INDEFINITE),
            (0, 1_001, 0),
            (-60_000, 1_001, 0),
            (60_000, 0, 0),
            (60_000, -1_001, 0),
            (60_000, i64::from(u32::MAX) + 1, 0),
        ] {
            assert_eq!(DisplayPeriod { time_scale, time_value, flags }.refresh_rate(), None);
        }
        assert_eq!(current_refresh_rate(0), None);
    }

    #[test]
    fn display_mode_rate_does_not_round_down_across_an_integer_boundary() {
        let exact = from_display_mode_hz(120.0).unwrap();
        assert!(exact.covered_by(120));
        assert!(!exact.covered_by(119));
        let fractional = from_display_mode_hz(120.000_1).unwrap();
        assert!(!fractional.covered_by(120));
        assert!(fractional.covered_by(121));
        assert_eq!(from_display_mode_hz(f64::INFINITY), None);
    }
}
