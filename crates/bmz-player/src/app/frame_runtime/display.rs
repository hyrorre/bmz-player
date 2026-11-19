use std::cell::RefCell;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::time::{Duration, Instant};

use bmz_render::renderer::WgpuPresentMode;
use winit::{
    monitor::{MonitorHandle, VideoModeHandle},
    raw_window_handle::{HasDisplayHandle, RawDisplayHandle},
    window::{Fullscreen, Window},
};

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
pub(in crate::app) struct DisplayTimingCache(Box<RefCell<DisplayTiming>>);

struct DisplayTiming {
    monitor: Option<MonitorHandle>,
    exclusive_mode: Option<VideoModeHandle>,
    query: DisplayQueryState,
    worker: Option<RefreshWorker>,
}

impl Default for DisplayTiming {
    fn default() -> Self {
        let worker = RefreshWorker::spawn(|request| {
            crate::window_config::current_refresh_rate(
                request.monitor.as_ref()?,
                request.exclusive_mode.as_ref(),
            )
        })
        .map_err(|error| tracing::warn!(%error, "could not start display timing worker"))
        .ok();
        Self { monitor: None, exclusive_mode: None, query: DisplayQueryState::default(), worker }
    }
}

const QUERY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
struct DisplayQueryState {
    rate: Option<RefreshRate>,
    revision: u64,
    generation: u64,
    pending: Option<u64>,
    last_started: Option<Instant>,
}

impl DisplayQueryState {
    fn invalidate(&mut self) {
        // Reject in-flight results without changing the published display revision.
        self.generation = self.generation.wrapping_add(1);
    }

    fn monitor_changed(&mut self) {
        self.invalidate();
        self.rate = None;
        self.revision = self.revision.wrapping_add(1);
    }

    fn set_rate(&mut self, rate: Option<RefreshRate>) {
        let rate = rate.filter(|rate| rate.is_valid());
        if self.rate != rate {
            self.rate = rate;
            self.revision = self.revision.wrapping_add(1);
            tracing::debug!(?rate, revision = self.revision, "frame pacing display timing changed");
        }
    }

    fn complete(&mut self, result: RefreshResult) {
        if self.pending != Some(result.generation) {
            return;
        }
        self.pending = None;
        if result.generation == self.generation {
            self.set_rate(result.rate);
        }
    }

    fn due(&self, now: Instant) -> bool {
        self.pending.is_none()
            && self.last_started.is_none_or(|at| now.duration_since(at) >= QUERY_INTERVAL)
    }

    fn started(&mut self, now: Instant) {
        self.pending = Some(self.generation);
        self.last_started = Some(now);
    }
}

struct RefreshRequest {
    generation: u64,
    monitor: Option<MonitorHandle>,
    exclusive_mode: Option<VideoModeHandle>,
}

struct RefreshResult {
    generation: u64,
    rate: Option<RefreshRate>,
}

struct RefreshWorker {
    // Boxed: winit's Linux monitor and video mode handles make the request large.
    requests: SyncSender<Box<RefreshRequest>>,
    results: Receiver<RefreshResult>,
}

impl RefreshWorker {
    fn spawn(
        query: impl Fn(&RefreshRequest) -> Option<RefreshRate> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (requests, receive_requests) = mpsc::sync_channel::<Box<RefreshRequest>>(1);
        let (send_results, results) = mpsc::sync_channel(1);
        // Do not join on the window thread: an OS query may still be blocked at shutdown.
        let _ = std::thread::Builder::new().name("display-timing".into()).spawn(move || {
            while let Ok(request) = receive_requests.recv() {
                let started = Instant::now();
                let rate = query(&request);
                tracing::debug!(
                    query_us = started.elapsed().as_micros() as u64,
                    generation = request.generation,
                    ?rate,
                    "frame pacing display timing queried"
                );
                if send_results
                    .send(RefreshResult { generation: request.generation, rate })
                    .is_err()
                {
                    break;
                }
            }
        })?;
        Ok(Self { requests, results })
    }
}

impl DisplayTimingCache {
    pub(in crate::app) fn invalidate(&self) {
        self.0.borrow_mut().query.invalidate();
    }

    pub(in crate::app) fn refresh_rate(
        &self,
        window: Option<&Window>,
    ) -> (Option<RefreshRate>, u64) {
        let Some(window) = window else { return (None, 0) };
        if !window
            .display_handle()
            .is_ok_and(|handle| reports_current_refresh_rate(handle.as_raw()))
        {
            return (None, 0);
        }
        let monitor = window.current_monitor();
        let exclusive_mode = match window.fullscreen() {
            Some(Fullscreen::Exclusive(mode)) => Some(mode),
            _ => None,
        };
        let now = Instant::now();
        let mut cache = self.0.borrow_mut();
        if cache.monitor != monitor {
            cache.query.monitor_changed();
            cache.monitor = monitor;
        } else if cache.exclusive_mode != exclusive_mode {
            cache.query.invalidate();
        }
        cache.exclusive_mode = exclusive_mode;

        // The render/event path never waits for a query, including after invalidation.
        match cache.worker.as_ref().map(|worker| worker.results.try_recv()) {
            Some(Ok(result)) => cache.query.complete(result),
            Some(Err(TryRecvError::Disconnected)) => cache.worker_failed(),
            _ => {}
        }
        if cache.monitor.is_some() && cache.query.due(now) {
            let request = Box::new(RefreshRequest {
                generation: cache.query.generation,
                monitor: cache.monitor.clone(),
                exclusive_mode: cache.exclusive_mode.clone(),
            });
            match cache.worker.as_ref().map(|worker| worker.requests.try_send(request)) {
                Some(Ok(())) => cache.query.started(now),
                Some(Err(TrySendError::Disconnected(_))) => cache.worker_failed(),
                _ => {}
            }
        }
        (cache.query.rate, cache.query.revision)
    }
}

/// winit's X11 monitor handles keep the refresh rate seen when the monitor list was
/// read, so an existing window can report the old Hz after an xrandr change. Treat
/// it as unknown to keep the configured cap instead of uncapping above the display.
fn reports_current_refresh_rate(display: RawDisplayHandle) -> bool {
    !matches!(display, RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_))
}

impl DisplayTiming {
    fn worker_failed(&mut self) {
        tracing::warn!("display timing worker stopped; preserving configured FPS cap");
        self.worker = None;
        self.query.set_rate(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x11_refresh_rates_are_treated_as_unknown() {
        use winit::raw_window_handle::{
            AppKitDisplayHandle, WindowsDisplayHandle, XcbDisplayHandle, XlibDisplayHandle,
        };
        assert!(!reports_current_refresh_rate(RawDisplayHandle::Xlib(XlibDisplayHandle::new(
            None, 0
        ))));
        assert!(!reports_current_refresh_rate(RawDisplayHandle::Xcb(XcbDisplayHandle::new(
            None, 0
        ))));
        assert!(reports_current_refresh_rate(RawDisplayHandle::Windows(
            WindowsDisplayHandle::new()
        )));
        assert!(reports_current_refresh_rate(RawDisplayHandle::AppKit(AppKitDisplayHandle::new())));
    }

    #[test]
    fn failed_refresh_query_keeps_worker_alive_and_recovers_after_monitor_change() {
        let rate = Some(RefreshRate { numerator: 240_084, denominator: 1_000 });
        let worker = RefreshWorker::spawn(move |request| {
            // The old monitor lookup fails; the next monitor has valid timing.
            if request.generation == 0 { None } else { rate }
        })
        .unwrap();
        let mut state = DisplayQueryState::default();
        state.set_rate(rate);
        let policy = |state: &DisplayQueryState| {
            resolve_display_frame_limit(241, true, false, Some(WgpuPresentMode::Fifo), || {
                (state.rate, state.revision)
            })
            .0
        };
        assert_eq!(policy(&state), 0);
        let now = Instant::now();
        for (generation, expected_limit) in [(0, 241), (1, 0)] {
            if generation != 0 {
                state.monitor_changed();
                assert_eq!(policy(&state), 241);
            }
            let at = now + QUERY_INTERVAL * generation as u32;
            assert!(state.due(at));
            assert!(
                worker
                    .requests
                    .try_send(Box::new(RefreshRequest {
                        generation,
                        monitor: None,
                        exclusive_mode: None,
                    }))
                    .is_ok()
            );
            state.started(at);
            let result = worker.results.recv_timeout(Duration::from_secs(5)).unwrap();
            state.complete(result);
            assert_eq!(policy(&state), expected_limit);
            assert!(state.pending.is_none());
        }
    }

    #[test]
    fn event_bursts_preserve_display_revision_and_do_not_bypass_query_throttle() {
        let now = Instant::now();
        let rate = Some(RefreshRate { numerator: 60_000, denominator: 1_001 });
        let mut state = DisplayQueryState::default();
        state.set_rate(rate);
        state.started(now);
        for _ in 0..100 {
            state.invalidate();
        }
        // The query started before these events must not publish its result.
        state.complete(RefreshResult {
            generation: 0,
            rate: Some(RefreshRate { numerator: 240, denominator: 1 }),
        });
        assert_eq!((state.rate, state.revision, state.pending), (rate, 1, None));
        assert!(!state.due(now + QUERY_INTERVAL - Duration::from_nanos(1)));
        assert!(state.due(now + QUERY_INTERVAL));

        state.started(now + QUERY_INTERVAL);
        state.complete(RefreshResult { generation: state.generation, rate });
        assert_eq!((state.rate, state.revision), (rate, 1));
        assert!(!state.due(now + QUERY_INTERVAL));
    }

    #[test]
    fn monitor_and_rate_changes_publish_revisions_but_stale_results_cannot_overwrite_them() {
        let mut state = DisplayQueryState::default();
        let rate = Some(RefreshRate { numerator: 240_084, denominator: 1_000 });
        state.set_rate(rate);
        state.monitor_changed();
        assert_eq!((state.rate, state.revision), (None, 2));
        let now = Instant::now();
        state.started(now);
        state.complete(RefreshResult { generation: 0, rate });
        assert_eq!((state.rate, state.revision, state.pending), (None, 2, Some(1)));
        assert!(!state.due(now + QUERY_INTERVAL * 10));
        state.complete(RefreshResult { generation: 1, rate });
        assert_eq!((state.rate, state.revision), (rate, 3));
        state.set_rate(Some(RefreshRate { numerator: 60, denominator: 1 }));
        assert_eq!(state.revision, 4);
        state.set_rate(None);
        assert_eq!((state.rate, state.revision), (None, 5));
        state.set_rate(Some(RefreshRate { numerator: 0, denominator: 1 }));
        assert_eq!((state.rate, state.revision), (None, 5));
    }

    #[test]
    fn blocked_os_query_does_not_block_polling_or_allow_another_in_flight_query() {
        let (entered, receive_entered) = mpsc::sync_channel(1);
        let (release, receive_release) = mpsc::sync_channel(1);
        let worker = RefreshWorker::spawn(move |_| {
            entered.send(()).unwrap();
            receive_release.recv().unwrap();
            Some(RefreshRate { numerator: 240, denominator: 1 })
        })
        .unwrap();
        let mut state = DisplayQueryState::default();
        let now = Instant::now();
        assert!(
            worker
                .requests
                .try_send(Box::new(RefreshRequest {
                    generation: 0,
                    monitor: None,
                    exclusive_mode: None,
                }))
                .is_ok()
        );
        state.started(now);
        receive_entered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(worker.results.try_recv(), Err(TryRecvError::Empty)));
        state.invalidate();
        assert!(!state.due(now + QUERY_INTERVAL * 10));
        release.send(()).unwrap();
        let result = worker.results.recv_timeout(Duration::from_secs(5)).unwrap();
        state.complete(result);
        assert_eq!((state.rate, state.revision), (None, 0));
        assert!(state.due(now + QUERY_INTERVAL * 10));
    }

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
