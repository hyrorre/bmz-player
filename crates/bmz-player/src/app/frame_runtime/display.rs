use std::cell::RefCell;
use std::time::{Duration, Instant};

use bmz_render::renderer::WgpuPresentMode;
use winit::{monitor::MonitorHandle, window::Window};

use crate::window_config::RefreshRate;

/// Query display timing only when it can replace the application's foreground cap.
pub(in crate::app) fn resolve_display_frame_limit(
    configured: u32,
    focused: bool,
    suspended: bool,
    effective_mode: Option<WgpuPresentMode>,
    refresh: impl FnOnce() -> (Option<RefreshRate>, u64),
) -> (u32, u64) {
    if configured == 0
        || !focused
        || suspended
        || !matches!(effective_mode, Some(WgpuPresentMode::Fifo | WgpuPresentMode::FifoRelaxed))
    {
        return (configured, 0);
    }
    let (rate, revision) = refresh();
    let limit = if rate.is_some_and(|rate| rate.covered_by(configured)) { 0 } else { configured };
    (limit, revision)
}

#[derive(Default)]
pub(in crate::app) struct DisplayTimingCache(RefCell<DisplayTiming>);

#[derive(Default)]
struct DisplayTiming {
    monitor: Option<MonitorHandle>,
    queried_at: Option<Instant>,
    rate: Option<RefreshRate>,
    revision: u64,
}

impl DisplayTimingCache {
    pub(in crate::app) fn invalidate(&self) {
        self.0.borrow_mut().queried_at = None;
    }

    pub(in crate::app) fn refresh_rate(
        &self,
        window: Option<&Window>,
    ) -> (Option<RefreshRate>, u64) {
        let Some(window) = window else { return (None, 0) };
        let monitor = window.current_monitor();
        let now = Instant::now();
        let mut cache = self.0.borrow_mut();
        if cache.monitor != monitor
            || cache.queried_at.is_none_or(|at| now.duration_since(at) >= Duration::from_secs(1))
        {
            let rate = monitor
                .as_ref()
                .and_then(|monitor| crate::window_config::current_refresh_rate(window, monitor));
            if cache.monitor != monitor || cache.rate != rate || cache.queried_at.is_none() {
                cache.revision = cache.revision.wrapping_add(1);
                tracing::debug!(?rate, monitor = ?monitor.as_ref().and_then(MonitorHandle::name),
                    revision = cache.revision, "frame pacing display timing changed");
            }
            cache.monitor = monitor;
            cache.queried_at = Some(Instant::now());
            cache.rate = rate;
        }
        (cache.rate, cache.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_refresh_has_an_exact_boundary_without_tolerance() {
        for (numerator, denominator, below, at) in [
            (60_000, 1_001, 59, 60),
            (240_000, 1_001, 239, 240),
            (240, 1, 239, 240),
            (240_084, 1_000, 240, 241),
        ] {
            let rate = Some(RefreshRate { numerator, denominator });
            for mode in [WgpuPresentMode::Fifo, WgpuPresentMode::FifoRelaxed] {
                for (fps, expected) in [(below, below), (at, 0), (at * 2, 0)] {
                    assert_eq!(
                        resolve_display_frame_limit(fps, true, false, Some(mode), || (rate, 7)),
                        (expected, 7)
                    );
                }
            }
        }
    }

    #[test]
    fn other_modes_background_suspension_and_unlimited_do_not_query_display_timing() {
        for (fps, focused, suspended, mode) in [
            (0, true, false, Some(WgpuPresentMode::Fifo)),
            (240, false, false, Some(WgpuPresentMode::Fifo)),
            (30, true, true, Some(WgpuPresentMode::Fifo)),
            (10, false, true, Some(WgpuPresentMode::FifoRelaxed)),
            (240, true, false, Some(WgpuPresentMode::Mailbox)),
            (240, true, false, Some(WgpuPresentMode::Immediate)),
            (240, true, false, None),
        ] {
            assert_eq!(
                resolve_display_frame_limit(fps, focused, suspended, mode, || {
                    panic!("this policy must not query the display")
                }),
                (fps, 0)
            );
        }
    }

    #[test]
    fn unknown_or_invalid_refresh_preserves_the_cap_and_display_revision() {
        for rate in [
            None,
            Some(RefreshRate { numerator: 0, denominator: 1 }),
            Some(RefreshRate { numerator: 60, denominator: 0 }),
        ] {
            assert_eq!(
                resolve_display_frame_limit(240, true, false, Some(WgpuPresentMode::Fifo), || (
                    rate, 9
                )),
                (240, 9)
            );
        }
        assert!(RefreshRate { numerator: u32::MAX, denominator: u32::MAX }.covered_by(u32::MAX));
    }
}
