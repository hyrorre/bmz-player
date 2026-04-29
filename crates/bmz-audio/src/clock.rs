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
