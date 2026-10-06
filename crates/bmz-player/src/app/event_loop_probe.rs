//! Explicit synthetic wake-to-dispatch probe. This never injects keyboard input.
use super::AppUserEvent;
use bmz_core::latency::LatencyHistogram;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use winit::event_loop::EventLoopProxy;

const INTERVAL: Duration = Duration::from_millis(11);
const WARMUP: Duration = Duration::from_secs(2);
const CAPACITY: usize = 32;

#[derive(Debug, Clone, Copy)]
pub(super) struct Ticket {
    sent_at: Instant,
    epoch: u64,
}

#[derive(Default)]
struct Shared {
    epoch: AtomicU64,
    pending: AtomicUsize,
    sent: AtomicU64,
    capacity_skips: AtomicU64,
    missed_ticks: AtomicU64,
}

impl Shared {
    // Single sender; the receiver only subtracts. A stalled window cannot grow
    // winit's unbounded user-event queue indefinitely.
    fn reserve(&self) -> bool {
        if self.pending.load(Ordering::Acquire) >= CAPACITY {
            self.capacity_skips.fetch_add(1, Ordering::Relaxed);
            false
        } else {
            self.pending.fetch_add(1, Ordering::AcqRel);
            true
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Context {
    pub scene: &'static str,
    pub focused: bool,
    pub generation: Option<u64>,
}

struct Samples {
    epoch: u64,
    start: Instant,
    histogram: LatencyHistogram,
    warmup: u64,
    rejected: u64,
}

impl Samples {
    fn new(epoch: u64, start: Instant) -> Self {
        Self { epoch, start, histogram: LatencyHistogram::default(), warmup: 0, rejected: 0 }
    }
    fn record(&mut self, ticket: Ticket, now: Instant) {
        if ticket.epoch != self.epoch || ticket.sent_at < self.start || ticket.sent_at > now {
            self.rejected += 1;
        } else if ticket.sent_at.duration_since(self.start) < WARMUP {
            self.warmup += 1;
        } else if let Ok(ns) = u64::try_from(now.duration_since(ticket.sent_at).as_nanos()) {
            self.histogram.record(ns);
        } else {
            self.rejected += 1;
        }
    }
}

pub(super) struct Probe {
    shared: Arc<Shared>,
    stop: mpsc::Sender<()>,
    worker: Option<JoinHandle<()>>,
    context: Option<Context>,
    samples: Samples,
    last_receive: Instant,
    last_log: Instant,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    suspend: bmz_core::suspend::SuspendMonitor,
}

impl Probe {
    pub(super) fn start(proxy: EventLoopProxy<AppUserEvent>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let sender_shared = shared.clone();
        let (stop, receiver) = mpsc::channel();
        let worker = thread::Builder::new().name("bmz-window-probe".into()).spawn(move || {
            let mut deadline = Instant::now() + INTERVAL;
            while let Err(mpsc::RecvTimeoutError::Timeout) =
                receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                let now = Instant::now();
                let missed =
                    now.saturating_duration_since(deadline).as_nanos() / INTERVAL.as_nanos();
                sender_shared.missed_ticks.fetch_add(missed as u64, Ordering::Relaxed);
                // No catch-up burst after suspension or scheduler starvation.
                deadline = if missed > 0 { now + INTERVAL } else { deadline + INTERVAL };
                if !sender_shared.reserve() {
                    continue;
                }
                let ticket = Ticket {
                    sent_at: Instant::now(),
                    epoch: sender_shared.epoch.load(Ordering::Acquire),
                };
                if proxy.send_event(AppUserEvent::LatencyProbe(ticket)).is_err() {
                    sender_shared.pending.fetch_sub(1, Ordering::AcqRel);
                    break;
                }
                sender_shared.sent.fetch_add(1, Ordering::Relaxed);
            }
        })?;
        let now = Instant::now();
        Ok(Self {
            shared,
            stop,
            worker: Some(worker),
            context: None,
            samples: Samples::new(0, now),
            last_receive: now,
            last_log: now,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            suspend: bmz_core::suspend::SuspendMonitor::default(),
        })
    }

    pub(super) fn set_context(&mut self, context: Context) {
        if self.context != Some(context) {
            self.reset("context_change");
            self.context = Some(context);
        }
    }

    pub(super) fn reset(&mut self, reason: &str) {
        self.log(reason);
        let epoch = self.shared.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let now = Instant::now();
        self.samples = Samples::new(epoch, now);
        self.last_log = now;
        self.last_receive = now;
    }

    pub(super) fn receive(&mut self, ticket: Ticket) {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if self.suspend.poll() {
            self.reset("clock_resume");
        }
        let now = Instant::now();
        self.shared.pending.fetch_sub(1, Ordering::AcqRel);
        if now.duration_since(self.last_receive) > Duration::from_secs(1) {
            self.reset("long_dispatch_gap");
        }
        self.last_receive = now;
        self.samples.record(ticket, now);
        if now.duration_since(self.last_log) >= Duration::from_secs(1) {
            self.log("periodic");
            self.last_log = now;
        }
    }

    fn log(&self, reason: &str) {
        let Some(context) = self.context else {
            return;
        };
        let summary = serde_json::json!({
            "schema": 1, "kind": "window_event_loop_probe", "epoch": self.samples.epoch,
            "scene": context.scene, "focused": context.focused, "generation": context.generation,
            "reason": reason, "clock": "process Instant", "synthetic": true,
            "interval_ns": INTERVAL.as_nanos(), "capacity": CAPACITY,
            "warmup_seconds": WARMUP.as_secs(), "warmup_samples": self.samples.warmup,
            "rejected_samples": self.samples.rejected,
            "enqueue_to_dispatch_ns": self.samples.histogram.summary(),
            "sent_process": self.shared.sent.load(Ordering::Relaxed),
            "pending_process": self.shared.pending.load(Ordering::Acquire),
            "capacity_skips_process": self.shared.capacity_skips.load(Ordering::Relaxed),
            "producer_missed_ticks_process": self.shared.missed_ticks.load(Ordering::Relaxed),
            "physical_press_to_output_ns": null, "winit_os_to_receive_ns": null,
        });
        tracing::info!("BMZ_LATENCY_JSON {summary}");
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.log("shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_stale_and_future_tickets_are_not_zero_latency_samples() {
        let now = Instant::now();
        let mut samples = Samples::new(3, now);
        samples.record(Ticket { epoch: 3, sent_at: now }, now + Duration::from_millis(1));
        let sent_at = now + WARMUP;
        samples.record(Ticket { epoch: 2, sent_at }, sent_at);
        samples.record(Ticket { epoch: 3, sent_at }, now);
        samples.record(Ticket { epoch: 3, sent_at }, sent_at + Duration::from_millis(100));
        assert_eq!(samples.warmup, 1);
        assert_eq!(samples.rejected, 2);
        assert_eq!(samples.histogram.summary().count, 1);
        assert_eq!(samples.histogram.summary().max, 100_000_000);
        assert_eq!(Samples::new(4, sent_at).histogram.summary().count, 0);
    }

    #[test]
    fn stalled_window_queue_is_bounded_and_resumes_after_dispatch() {
        let shared = Shared::default();
        for _ in 0..CAPACITY {
            assert!(shared.reserve());
        }
        for _ in 0..100 {
            assert!(!shared.reserve());
        }
        assert_eq!(shared.capacity_skips.load(Ordering::Relaxed), 100);
        assert_eq!(shared.pending.load(Ordering::Relaxed), CAPACITY);
        shared.pending.fetch_sub(1, Ordering::AcqRel);
        assert!(shared.reserve());
    }
}
