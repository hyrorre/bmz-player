use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use bmz_core::time::TimeUs;

#[derive(Debug, Clone)]
pub struct AudioClock {
    pub sample_rate: u32,
    pub start_output_frame: u64,
    pub chart_zero_time_us: i64,
    pub current_frame: Arc<AtomicU64>,
    pub running: bool,
    playback_rate_percent: u16,
    started_at: Option<Instant>,
    /// App-wide policy, frozen for this play. Offline clocks use frame scheduling.
    pub schedule_ahead: bool,
}

pub const MIN_PLAYBACK_RATE_PERCENT: u16 = 25;
pub const MAX_PLAYBACK_RATE_PERCENT: u16 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackRateChange {
    pub changed_at: Option<Instant>,
    pub anchor_output_frame: u64,
    pub old_rate_percent: u16,
    pub new_rate_percent: u16,
}

pub const fn clamp_playback_rate_percent(rate: u16) -> u16 {
    if rate < MIN_PLAYBACK_RATE_PERCENT {
        MIN_PLAYBACK_RATE_PERCENT
    } else if rate > MAX_PLAYBACK_RATE_PERCENT {
        MAX_PLAYBACK_RATE_PERCENT
    } else {
        rate
    }
}

impl AudioClock {
    pub fn stopped(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            start_output_frame: 0,
            chart_zero_time_us: 0,
            current_frame: Arc::new(AtomicU64::new(0)),
            running: false,
            playback_rate_percent: 100,
            started_at: None,
            schedule_ahead: true,
        }
    }

    pub fn with_position(
        sample_rate: u32,
        start_output_frame: u64,
        chart_zero_time_us: i64,
        current_frame: Arc<AtomicU64>,
        running: bool,
    ) -> Self {
        Self {
            sample_rate,
            start_output_frame,
            chart_zero_time_us,
            current_frame,
            running,
            playback_rate_percent: 100,
            started_at: None,
            schedule_ahead: true,
        }
    }

    pub fn set_playback_rate_percent(&mut self, rate: u16) -> Option<PlaybackRateChange> {
        self.set_playback_rate_percent_at(rate, Instant::now())
    }

    fn set_playback_rate_percent_at(
        &mut self,
        rate: u16,
        changed_at: Instant,
    ) -> Option<PlaybackRateChange> {
        let rate = clamp_playback_rate_percent(rate);
        let old_rate_percent = self.playback_rate_percent;
        if rate == old_rate_percent {
            return None;
        }

        let anchor_output_frame = self.current_frame.load(Ordering::Relaxed);
        if self.running {
            if self.started_at.is_some() {
                self.chart_zero_time_us = self.now_at(changed_at).0;
                self.started_at = Some(changed_at);
            } else {
                self.chart_zero_time_us = self.now().0;
            }
            self.start_output_frame = anchor_output_frame;
        }
        self.playback_rate_percent = rate;
        Some(PlaybackRateChange {
            changed_at: self.started_at.map(|_| changed_at),
            anchor_output_frame,
            old_rate_percent,
            new_rate_percent: rate,
        })
    }

    pub const fn playback_rate_percent(&self) -> u16 {
        self.playback_rate_percent
    }

    /// Starts the chart clock from a monotonic timestamp.
    ///
    /// Output-frame coordinates remain available for audio scheduling, while gameplay time
    /// advances continuously from [`Instant`] instead of stepping once per audio callback.
    pub fn start(&mut self, chart_zero_time: TimeUs) {
        self.start_at(chart_zero_time, Instant::now());
    }

    pub fn start_at(&mut self, chart_zero_time: TimeUs, started_at: Instant) {
        self.chart_zero_time_us = chart_zero_time.0;
        self.start_output_frame = self.current_frame.load(Ordering::Relaxed);
        self.running = true;
        self.started_at = Some(started_at);
    }

    pub fn pause(&mut self) {
        if self.running {
            self.chart_zero_time_us = self.now().0;
            self.start_output_frame = self.current_frame.load(Ordering::Relaxed);
        }
        self.running = false;
        self.started_at = None;
    }

    pub fn pause_at(&mut self, chart_time: TimeUs) {
        self.chart_zero_time_us = chart_time.0;
        self.start_output_frame = self.current_frame.load(Ordering::Relaxed);
        self.running = false;
        self.started_at = None;
    }

    pub fn now(&self) -> TimeUs {
        self.now_at(Instant::now())
    }

    pub fn now_at(&self, frame_at: Instant) -> TimeUs {
        if !self.running {
            return TimeUs(self.chart_zero_time_us);
        }

        if let Some(started_at) = self.started_at {
            let wall_elapsed_us =
                frame_at.saturating_duration_since(started_at).as_micros().min(i64::MAX as u128)
                    as i64;
            let elapsed_us = scale_chart_time(wall_elapsed_us, self.playback_rate_percent);
            return TimeUs(self.chart_zero_time_us.saturating_add(elapsed_us));
        }

        // Tests and clocks restored from an explicit output position do not have a monotonic
        // anchor. Preserve the old hardware-frame calculation for those snapshots.
        let frame = self.current_frame.load(Ordering::Relaxed);
        let delta_frames = frame.saturating_sub(self.start_output_frame);
        let delta_us = scale_chart_time(
            frame_to_us(delta_frames, self.sample_rate),
            self.playback_rate_percent,
        );
        TimeUs(self.chart_zero_time_us + delta_us)
    }

    pub fn time_to_output_frame(&self, time: TimeUs) -> u64 {
        let delta_us = i128::from(time.0) - i128::from(self.chart_zero_time_us);
        let magnitude_us = if delta_us < 0 { (-delta_us) as u128 } else { delta_us as u128 };
        let delta_frames =
            magnitude_us.saturating_mul(u128::from(self.sample_rate)).saturating_mul(100)
                / (1_000_000u128 * u128::from(self.playback_rate_percent));
        let delta_frames = delta_frames.min(u128::from(u64::MAX)) as u64;
        if delta_us < 0 {
            // Practice can start partway through a chart. Keep past BGM events in
            // output-frame coordinates so catch-up begins at their elapsed sample
            // position instead of restarting every earlier sound at frame zero.
            self.start_output_frame.saturating_sub(delta_frames)
        } else {
            self.start_output_frame.saturating_add(delta_frames)
        }
    }

    pub fn time_to_instant(&self, time: TimeUs) -> Option<Instant> {
        let origin = self.started_at?;
        let us = (i128::from(time.0) - i128::from(self.chart_zero_time_us)) * 100
            / i128::from(self.playback_rate_percent);
        let duration =
            std::time::Duration::from_micros(us.unsigned_abs().min(u128::from(u64::MAX)) as u64);
        if us < 0 { origin.checked_sub(duration) } else { origin.checked_add(duration) }
    }

    /// Returns chart time elapsed since a timestamp, using the gameplay clock.
    ///
    /// Time before `since`, such as the READY margin before chart time zero, is excluded.
    pub fn elapsed_since(&self, since: TimeUs) -> TimeUs {
        if !self.running {
            return TimeUs(0);
        }

        TimeUs(self.now().0.saturating_sub(since.0).max(0))
    }
}

fn scale_chart_time(wall_time_us: i64, playback_rate_percent: u16) -> i64 {
    ((i128::from(wall_time_us) * i128::from(playback_rate_percent)) / 100)
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

pub fn frame_to_us(frame: u64, sample_rate: u32) -> i64 {
    ((frame as u128 * 1_000_000u128) / sample_rate as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stopped_clock_reports_chart_zero_time() {
        let clock = AudioClock::stopped(48_000);

        assert_eq!(clock.now(), TimeUs(0));
        assert_eq!(clock.time_to_output_frame(TimeUs(1_000_000)), 48_000);
        assert_eq!(clock.elapsed_since(TimeUs(0)), TimeUs(0));
    }

    #[test]
    fn pause_at_freezes_exact_chart_time_until_restarted() {
        let current_frame = Arc::new(AtomicU64::new(48_000));
        let mut clock = AudioClock::with_position(48_000, 0, 0, Arc::clone(&current_frame), true);

        clock.pause_at(TimeUs(1_250_000));
        current_frame.store(96_000, Ordering::Relaxed);

        assert_eq!(clock.now(), TimeUs(1_250_000));
        assert!(!clock.running);
        clock.start_at(TimeUs(1_250_000), Instant::now());
        assert!(clock.running);
    }

    #[test]
    fn elapsed_since_excludes_negative_chart_margin() {
        let current_frame = Arc::new(AtomicU64::new(192_000));
        let clock =
            AudioClock::with_position(48_000, 96_000, -1_000_000, Arc::clone(&current_frame), true);

        assert_eq!(clock.time_to_output_frame(TimeUs(0)), 144_000);
        assert_eq!(clock.elapsed_since(TimeUs(0)), TimeUs(1_000_000));
    }

    #[test]
    fn running_clock_advances_from_frame_instant_between_audio_callbacks() {
        let current_frame = Arc::new(AtomicU64::new(256));
        let started_at = Instant::now();
        let mut clock = AudioClock::with_position(48_000, 0, 0, Arc::clone(&current_frame), false);
        clock.start_at(TimeUs(-1_000_000), started_at);

        assert_eq!(clock.now_at(started_at), TimeUs(-1_000_000));
        assert_eq!(clock.now_at(started_at + Duration::from_micros(4_167)), TimeUs(-995_833));
        assert_eq!(current_frame.load(Ordering::Relaxed), 256);
    }

    #[test]
    fn playback_rate_change_keeps_monotonic_clock_continuous() {
        let current_frame = Arc::new(AtomicU64::new(48_000));
        let started_at = Instant::now();
        let changed_at = started_at + Duration::from_secs(1);
        let mut clock = AudioClock::with_position(48_000, 0, 0, current_frame, false);
        clock.start_at(TimeUs(0), started_at);

        let change = clock.set_playback_rate_percent_at(300, changed_at).unwrap();

        assert_eq!(change.anchor_output_frame, 48_000);
        assert_eq!(change.old_rate_percent, 100);
        assert_eq!(change.new_rate_percent, 300);
        assert_eq!(clock.now_at(changed_at), TimeUs(1_000_000));
        assert_eq!(clock.now_at(changed_at + Duration::from_secs(1)), TimeUs(4_000_000));
    }

    #[test]
    fn playback_rate_change_keeps_output_frame_clock_continuous() {
        let current_frame = Arc::new(AtomicU64::new(48_000));
        let mut clock = AudioClock::with_position(48_000, 0, 0, Arc::clone(&current_frame), true);

        let change = clock.set_playback_rate_percent(25).unwrap();
        assert_eq!(change.anchor_output_frame, 48_000);
        assert_eq!(clock.now(), TimeUs(1_000_000));

        current_frame.store(96_000, Ordering::Relaxed);
        assert_eq!(clock.now(), TimeUs(1_250_000));
    }

    #[test]
    fn playback_rate_is_clamped_to_autoplay_range() {
        let mut clock = AudioClock::stopped(48_000);
        assert_eq!(clock.set_playback_rate_percent(1).unwrap().new_rate_percent, 25);
        assert_eq!(clock.set_playback_rate_percent(400).unwrap().new_rate_percent, 300);
    }
}
