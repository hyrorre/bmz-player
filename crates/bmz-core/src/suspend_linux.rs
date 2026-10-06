//! Linux CLOCK_BOOTTIME includes suspend; CLOCK_MONOTONIC does not.
//! Bracket samples to reject a descheduled observer. No wall-clock dependency.
#[derive(Default)]
pub struct SuspendMonitor {
    previous: Option<u128>,
}
impl SuspendMonitor {
    pub fn poll(&mut self) -> bool {
        fn clock(id: libc::clockid_t) -> Option<u128> {
            let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
            if unsafe { libc::clock_gettime(id, &mut t) } != 0 {
                return None;
            }
            Some(u128::try_from(t.tv_sec).ok()? * 1_000_000_000 + u128::try_from(t.tv_nsec).ok()?)
        }
        let (Some(before), Some(boot), Some(after)) = (
            clock(libc::CLOCK_MONOTONIC),
            clock(libc::CLOCK_BOOTTIME),
            clock(libc::CLOCK_MONOTONIC),
        ) else {
            return false;
        };
        self.observe(before, boot, after)
    }
    fn observe(&mut self, before: u128, boot: u128, after: u128) -> bool {
        let Some(width) = after.checked_sub(before).filter(|width| *width <= 1_000_000) else {
            return false;
        };
        let Some(offset) = boot.checked_sub(before + width / 2) else {
            return false;
        };
        self.previous.replace(offset).is_some_and(|previous| previous.abs_diff(offset) > 20_000_000)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suspend_resets_but_slow_samples_do_not_manufacture_resume() {
        let mut monitor = SuspendMonitor::default();
        assert!(!monitor.observe(100, 200, 102));
        assert!(!monitor.observe(200, 300, 202));
        assert!(monitor.observe(300, 1_000_000_400, 302));
        assert!(!monitor.observe(400, 1_000_000_500, 402));
        assert!(!monitor.observe(500, 2_000_000_500, 3_000_000_500));
        assert!(!monitor.observe(500, 1_000_000_600, 502));
    }
}
