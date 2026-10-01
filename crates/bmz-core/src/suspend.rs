//! macOS 10.12+ public Mach clocks. Detect suspend even when both Instant and
//! IOHID uptime stop during sleep. No wall clock is used for event conversion.
#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}
unsafe extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> i32;
    fn mach_absolute_time() -> u64;
    fn mach_continuous_time() -> u64;
}

pub struct SuspendMonitor {
    timebase: Timebase,
    previous: Option<u64>,
}
impl Default for SuspendMonitor {
    fn default() -> Self {
        let mut timebase = Timebase { numer: 0, denom: 0 };
        // Stack storage is valid for the synchronous system call.
        unsafe {
            mach_timebase_info(&mut timebase);
        }
        Self { timebase, previous: None }
    }
}
impl SuspendMonitor {
    pub fn poll(&mut self) -> bool {
        // These public calls have no ownership or thread-affinity requirements.
        let absolute = unsafe { mach_absolute_time() };
        let continuous = unsafe { mach_continuous_time() };
        let after = unsafe { mach_absolute_time() };
        // A descheduled sampler must not manufacture a suspend transition.
        let Some(width) = after.checked_sub(absolute) else {
            return false;
        };
        if self.timebase.denom == 0
            || u128::from(width) * u128::from(self.timebase.numer) / u128::from(self.timebase.denom)
                > 1_000_000
        {
            return false;
        }
        let Some(offset) = continuous.checked_sub(absolute + width / 2) else {
            return false;
        };
        self.observe(offset)
    }
    fn observe(&mut self, offset: u64) -> bool {
        let previous = self.previous.replace(offset);
        self.timebase.denom != 0
            && previous.is_some_and(|previous| {
                u128::from(previous.abs_diff(offset)) * u128::from(self.timebase.numer)
                    / u128::from(self.timebase.denom)
                    > 20_000_000
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uptime_pause_is_detected_without_wall_time() {
        let mut monitor =
            SuspendMonitor { timebase: Timebase { numer: 125, denom: 3 }, previous: None };
        assert!(!monitor.observe(100));
        assert!(!monitor.observe(101));
        assert!(monitor.observe(24_000_100));
        assert!(!monitor.observe(24_000_101));
    }
}
