//! IOHID timestamps are mach_absolute_time ticks (uptime, excluding sleep).
//! Anchor against BMZ's process-local Instant clock without using wall time.
#[derive(Debug)]
pub(super) struct MachClock {
    numer: u32,
    denom: u32,
    anchor_ticks: u64,
    anchor_ns: u128,
    oldest_valid_tick: Option<u64>,
    pub(super) epoch: u64,
}

impl MachClock {
    /// GameController reports Mach host time in seconds, not raw ticks.
    #[cfg(target_os = "macos")]
    pub(super) fn convert_seconds(
        &mut self,
        seconds: f64,
        now_ticks: u64,
        now_ns: u128,
    ) -> Option<u128> {
        if !seconds.is_finite() || seconds <= 0.0 || self.numer == 0 || self.denom == 0 {
            return None;
        }
        let ticks = seconds * 1_000_000_000.0 * f64::from(self.denom) / f64::from(self.numer);
        if ticks >= u64::MAX as f64 {
            return None;
        }
        self.convert(ticks.round() as u64, now_ticks, now_ns)
    }
    pub(super) fn rebase(&mut self, ticks: u64, ns: u128) {
        self.anchor_ticks = ticks;
        self.anchor_ns = ns;
        self.oldest_valid_tick = Some(ticks);
        self.epoch = self.epoch.wrapping_add(1);
    }
    pub(super) fn new(ticks: u64, ns: u128, numer: u32, denom: u32) -> Self {
        Self { numer, denom, anchor_ticks: ticks, anchor_ns: ns, oldest_valid_tick: None, epoch: 0 }
    }

    fn duration(&self, ticks: u64) -> Option<u128> {
        if self.numer == 0 || self.denom == 0 {
            return None;
        }
        Some(u128::from(ticks) * u128::from(self.numer) / u128::from(self.denom))
    }

    fn at(&self, ticks: u64) -> Option<u128> {
        if ticks >= self.anchor_ticks {
            self.anchor_ns.checked_add(self.duration(ticks - self.anchor_ticks)?)
        } else {
            self.anchor_ns.checked_sub(self.duration(self.anchor_ticks - ticks)?)
        }
    }

    /// Rebase after an observed clock discontinuity (including sleep on clocks
    /// with different suspend semantics). The triggering event is invalid, not
    /// silently moved to now. Ordinary delayed events preserve their timestamp.
    pub(super) fn convert(&mut self, event: u64, now_ticks: u64, now_ns: u128) -> Option<u128> {
        let expected = self.at(now_ticks)?;
        if expected.abs_diff(now_ns) > 20_000_000 {
            self.rebase(now_ticks, now_ns);
            return None;
        }
        if event == 0
            || event > now_ticks
            || self.oldest_valid_tick.is_some_and(|oldest| event < oldest)
        {
            return None;
        }
        self.at(event).filter(|time| *time <= now_ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn gamecontroller_seconds_are_not_raw_ticks_and_rebase_rejects_old_history() {
        let mut clock = MachClock::new(24_000_000, 1_000_000_000, 125, 3);
        assert_eq!(clock.convert_seconds(1.001, 24_048_000, 1_002_000_000), Some(1_001_000_000));
        assert_eq!(clock.convert_seconds(f64::NAN, 24_048_000, 1_002_000_000), None);
        assert_eq!(clock.convert_seconds(1.003, 24_048_000, 1_002_000_000), None);
        clock.rebase(24_048_000, 1_002_000_000);
        assert_eq!(clock.convert_seconds(1.001, 24_048_000, 1_002_000_000), None);
    }
    #[test]
    fn ticks_rounding_old_events_and_invalid_values() {
        let mut clock = MachClock::new(100, 1_000, 125, 3);
        assert_eq!(clock.convert(99, 103, 1_125), Some(959));
        assert_eq!(clock.convert(101, 103, 1_125), Some(1_041));
        assert_eq!(clock.convert(0, 103, 1_125), None);
        assert_eq!(clock.convert(104, 103, 1_125), None);
        assert_eq!(MachClock::new(100, 0, 1, 1).convert(99, 100, 0), None);
        assert_eq!(MachClock::new(1, 0, 1, 0).convert(1, 1, 0), None);
    }
    #[test]
    fn wide_multiplication_and_sleep_rebase() {
        let mut clock = MachClock::new(1, 0, u32::MAX, 1);
        let ns = u128::from(u64::MAX - 1) * u128::from(u32::MAX);
        assert_eq!(clock.convert(u64::MAX, u64::MAX, ns), Some(ns));
        let mut clock = MachClock::new(1, 0, 1, 1);
        assert_eq!(clock.convert(2, 3, 5_000_000_000), None);
        assert_eq!(clock.epoch, 1);
        assert_eq!(clock.convert(2, 4, 5_000_000_001), None);
        assert_eq!(clock.convert(4, 4, 5_000_000_001), Some(5_000_000_001));
    }
}
