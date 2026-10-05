use bmz_core::latency::{AtomicLatencyHistogram, DistributionSummary};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug, Default)]
pub(super) struct OutputTiming {
    epoch: AtomicU64,
    frames: AtomicLatencyHistogram,
    interval_ns: AtomicLatencyHistogram,
    pub(super) duration_ns: AtomicLatencyHistogram,
    prediction_ns: AtomicLatencyHistogram,
    invalid_predictions: AtomicU64,
    unavailable_predictions: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutputTimingSummary {
    pub epoch: u64,
    pub frames: DistributionSummary,
    pub interval_ns: DistributionSummary,
    pub duration_ns: DistributionSummary,
    pub prediction_ns: DistributionSummary,
    pub invalid_predictions: u64,
    pub unavailable_predictions: u64,
}

impl OutputTiming {
    pub(super) fn reset(&self) {
        self.frames.reset();
        self.interval_ns.reset();
        self.duration_ns.reset();
        self.prediction_ns.reset();
        self.invalid_predictions.store(0, Ordering::Relaxed);
        self.unavailable_predictions.store(0, Ordering::Relaxed);
        self.epoch.fetch_add(1, Ordering::Release);
    }
    pub(super) fn observe(
        &self,
        frames: usize,
        now: Instant,
        previous: Option<Instant>,
        timestamp: cpal::OutputStreamTimestamp,
    ) {
        self.observe_delay(
            frames,
            now,
            previous,
            timestamp.playback.checked_duration_since(timestamp.callback),
        );
    }

    // CPAL 0.18.1's PipeWire fallback manufactures a one-period prediction and
    // provides no validity bit. Do not mix that with a server delay estimate.
    pub(super) fn observe_unavailable(
        &self,
        frames: usize,
        now: Instant,
        previous: Option<Instant>,
    ) {
        self.observe_frames(frames, now, previous);
        self.unavailable_predictions.fetch_add(1, Ordering::Relaxed);
    }

    fn observe_frames(&self, frames: usize, now: Instant, previous: Option<Instant>) {
        self.frames.record(frames as u64);
        if let Some(previous) = previous.and_then(|previous| now.checked_duration_since(previous)) {
            self.interval_ns.record(previous.as_nanos().min(u64::MAX as u128) as u64);
        }
    }

    /// The caller subtracts timestamps in the backend's own clock domain.
    pub(super) fn observe_delay(
        &self,
        frames: usize,
        now: Instant,
        previous: Option<Instant>,
        delay: Option<std::time::Duration>,
    ) {
        self.observe_frames(frames, now, previous);
        match delay.filter(|d| !d.is_zero() && d.as_secs() < 1) {
            Some(delay) => self.prediction_ns.record(delay.as_nanos() as u64),
            None => {
                self.invalid_predictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn summary(&self) -> OutputTimingSummary {
        OutputTimingSummary {
            epoch: self.epoch.load(Ordering::Acquire),
            frames: self.frames.summary(),
            interval_ns: self.interval_ns.summary(),
            duration_ns: self.duration_ns.summary(),
            prediction_ns: self.prediction_ns.summary(),
            invalid_predictions: self.invalid_predictions.load(Ordering::Relaxed),
            unavailable_predictions: self.unavailable_predictions.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_server_prediction_is_not_a_zero_latency_sample() {
        let timing = OutputTiming::default();
        timing.observe_unavailable(512, Instant::now(), None);
        let summary = timing.summary();
        assert_eq!(summary.frames.max, 512);
        assert_eq!(summary.prediction_ns.count, 0);
        assert_eq!(summary.unavailable_predictions, 1);
        assert_eq!(summary.invalid_predictions, 0);
    }
    #[test]
    fn measures_actual_frames_and_excludes_invalid_predictions() {
        let timing = OutputTiming::default();
        let now = Instant::now();
        for (frames, playback) in [(64, 102), (128, 99), (256, 100)] {
            timing.observe(
                frames,
                now,
                None,
                cpal::OutputStreamTimestamp {
                    callback: cpal::StreamInstant::from_millis(100),
                    playback: cpal::StreamInstant::from_millis(playback),
                },
            );
        }
        let summary = timing.summary();
        assert_eq!(summary.frames.count, 3);
        assert_eq!(summary.frames.max, 256);
        assert_eq!(summary.prediction_ns.count, 1);
        assert_eq!(summary.prediction_ns.max, 2_000_000);
        assert_eq!(summary.invalid_predictions, 2);
        timing.reset();
        assert_eq!(timing.summary().epoch, 1);
        assert_eq!(timing.summary().frames.count, 0);
        assert_eq!(timing.summary().invalid_predictions, 0);
    }
}
