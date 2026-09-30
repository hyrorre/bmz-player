use bmz_core::latency::{AtomicLatencyHistogram, DistributionSummary};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Debug, Default)]
pub(super) struct OutputTiming {
    frames: AtomicLatencyHistogram,
    interval_ns: AtomicLatencyHistogram,
    pub(super) duration_ns: AtomicLatencyHistogram,
    prediction_ns: AtomicLatencyHistogram,
    invalid_predictions: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutputTimingSummary {
    pub frames: DistributionSummary,
    pub interval_ns: DistributionSummary,
    pub duration_ns: DistributionSummary,
    pub prediction_ns: DistributionSummary,
    pub invalid_predictions: u64,
}

impl OutputTiming {
    pub(super) fn observe(
        &self,
        frames: usize,
        now: Instant,
        previous: Option<Instant>,
        timestamp: cpal::OutputStreamTimestamp,
    ) {
        self.frames.record(frames as u64);
        if let Some(previous) = previous {
            self.interval_ns
                .record(now.duration_since(previous).as_nanos().min(u64::MAX as u128) as u64);
        }
        // Both values belong to CPAL's stream clock, never subtract Instant/BMZ time.
        match timestamp
            .playback
            .checked_duration_since(timestamp.callback)
            .filter(|d| !d.is_zero() && d.as_secs() < 1)
        {
            Some(delay) => self.prediction_ns.record(delay.as_nanos() as u64),
            None => {
                self.invalid_predictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn summary(&self) -> OutputTimingSummary {
        OutputTimingSummary {
            frames: self.frames.summary(),
            interval_ns: self.interval_ns.summary(),
            duration_ns: self.duration_ns.summary(),
            prediction_ns: self.prediction_ns.summary(),
            invalid_predictions: self.invalid_predictions.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    }
}
