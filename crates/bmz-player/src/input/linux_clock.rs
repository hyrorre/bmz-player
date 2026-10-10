//! EVIOCSCLOCKID selects CLOCK_MONOTONIC. Bracket a read against BMZ's
//! process-local Instant clock; do not assume equal origins or use wall time.
use bmz_gameplay::input::backend::monotonic_timestamp_ns;

pub(super) fn clock_ns(clock: libc::clockid_t) -> Option<u128> {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // Valid stack storage; clock_gettime has no ownership requirements.
    if unsafe { libc::clock_gettime(clock, &mut time) } != 0 {
        return None;
    }
    u128::try_from(time.tv_sec)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(u128::try_from(time.tv_nsec).ok()?)
}

pub(super) struct Clock {
    os_anchor: u128,
    bmz_anchor: u128,
    boot_offset: u128,
}

impl Clock {
    /// Retries a bracket stretched by preemption before giving up.
    pub(super) fn sample() -> Option<(u128, u128, u128)> {
        (0..4).find_map(|_| Self::sample_once())
    }
    fn sample_once() -> Option<(u128, u128, u128)> {
        let before = monotonic_timestamp_ns();
        let os = clock_ns(libc::CLOCK_MONOTONIC)?;
        let boot = clock_ns(libc::CLOCK_BOOTTIME)?;
        let after = monotonic_timestamp_ns();
        (after.checked_sub(before)? <= 1_000_000).then_some((
            os,
            before + (after - before) / 2,
            boot.checked_sub(os)?,
        ))
    }
    pub(super) fn new(sample: (u128, u128, u128)) -> Self {
        Self { os_anchor: sample.0, bmz_anchor: sample.1, boot_offset: sample.2 }
    }
    pub(super) fn continuous(&self, sample: (u128, u128, u128)) -> bool {
        self.map(sample.0).is_some_and(|expected| expected.abs_diff(sample.1) <= 2_000_000)
            && sample.2.abs_diff(self.boot_offset) <= 20_000_000
    }
    fn map(&self, os: u128) -> Option<u128> {
        if os >= self.os_anchor {
            self.bmz_anchor.checked_add(os - self.os_anchor)
        } else {
            self.bmz_anchor.checked_sub(self.os_anchor - os)
        }
    }
    pub(super) fn convert(&self, seconds: i64, micros: i64, now_os: u128) -> Option<u128> {
        if !(0..1_000_000).contains(&micros) {
            return None;
        }
        let os = u128::try_from(seconds)
            .ok()?
            .checked_mul(1_000_000_000)?
            .checked_add(micros as u128 * 1_000)?;
        if os < self.os_anchor || os > now_os || os == 0 {
            return None;
        }
        self.map(os)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_events_keep_time_and_invalid_or_initial_events_are_rejected() {
        let clock = Clock::new((10_000_000_000, 1_000_000, 5_000_000_000));
        assert_eq!(clock.convert(10, 500, 11_000_000_000), Some(1_500_000));
        assert_eq!(clock.convert(9, 999_999, 11_000_000_000), None);
        assert_eq!(clock.convert(12, 0, 11_000_000_000), None);
        assert_eq!(clock.convert(-1, 0, 11_000_000_000), None);
        assert_eq!(clock.convert(10, 1_000_000, 11_000_000_000), None);
        assert!(!clock.continuous((11_000_000_000, 1_001_000_000, 6_000_000_000)));
        assert!(clock.continuous((11_000_000_000, 1_001_000_000, 5_000_000_000)));
        assert!(!clock.continuous((11_000_000_000, 2_001_000_000, 5_000_000_000)));
    }
}
