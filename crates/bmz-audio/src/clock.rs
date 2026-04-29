use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bmz_core::time::TimeUs;

#[derive(Debug, Clone)]
pub struct AudioClock {
    pub sample_rate: u32,
    pub start_output_frame: u64,
    pub chart_zero_time_us: i64,
    pub current_frame: Arc<AtomicU64>,
    pub running: bool,
}

impl AudioClock {
    pub fn stopped(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            start_output_frame: 0,
            chart_zero_time_us: 0,
            current_frame: Arc::new(AtomicU64::new(0)),
            running: false,
        }
    }

    pub fn with_position(
        sample_rate: u32,
        start_output_frame: u64,
        chart_zero_time_us: i64,
        current_frame: Arc<AtomicU64>,
        running: bool,
    ) -> Self {
        Self { sample_rate, start_output_frame, chart_zero_time_us, current_frame, running }
    }

    pub fn now(&self) -> TimeUs {
        if !self.running {
            return TimeUs(self.chart_zero_time_us);
        }

        let frame = self.current_frame.load(Ordering::Relaxed);
        let delta_frames = frame.saturating_sub(self.start_output_frame);
        let delta_us = frame_to_us(delta_frames, self.sample_rate);
        TimeUs(self.chart_zero_time_us + delta_us)
    }

    pub fn time_to_output_frame(&self, time: TimeUs) -> u64 {
        let delta_us = (time.0 - self.chart_zero_time_us).max(0) as u128;
        let delta_frames = delta_us * self.sample_rate as u128 / 1_000_000u128;
        self.start_output_frame + delta_frames as u64
    }
}

pub fn frame_to_us(frame: u64, sample_rate: u32) -> i64 {
    ((frame as u128 * 1_000_000u128) / sample_rate as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_clock_reports_chart_zero_time() {
        let clock = AudioClock::stopped(48_000);

        assert_eq!(clock.now(), TimeUs(0));
        assert_eq!(clock.time_to_output_frame(TimeUs(1_000_000)), 48_000);
    }
}
