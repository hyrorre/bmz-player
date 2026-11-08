use std::time::{Duration, Instant};

/// A producer-owned segment, copied to every published projection. Rendering
/// samples the same segment even if publications or window frames are skipped.
#[derive(Clone, Copy)]
pub(crate) struct HispeedTransition {
    from: f32,
    target: f32,
    started_at: Option<Instant>,
    duration: Duration,
}

impl HispeedTransition {
    pub(crate) fn new(hispeed: f32) -> Self {
        Self { from: hispeed, target: hispeed, started_at: None, duration: Duration::ZERO }
    }

    pub(crate) fn value_at(&self, now: Instant) -> f32 {
        let Some(started_at) = self.started_at else { return self.target };
        if self.duration.is_zero() {
            return self.target;
        }
        let elapsed = now.saturating_duration_since(started_at);
        if elapsed >= self.duration {
            return self.target;
        }
        let progress = (elapsed.as_secs_f64() / self.duration.as_secs_f64()) as f32;
        self.from + (self.target - self.from) * progress
    }

    pub(crate) fn retarget(&mut self, target: f32, now: Instant, duration: Duration) {
        if target == self.target && duration == self.duration {
            return;
        }
        self.from = self.value_at(now);
        self.target = target;
        self.started_at = Some(now);
        self.duration = duration;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_motion_retargets_without_a_position_jump() {
        let start = Instant::now();
        let duration = Duration::from_millis(500);
        let mut transition = HispeedTransition::new(1.0);
        transition.retarget(3.0, start, duration);
        for (ms, expected) in [(0, 1.0), (125, 1.5), (250, 2.0), (500, 3.0), (1000, 3.0)] {
            assert_eq!(transition.value_at(start + Duration::from_millis(ms)), expected);
        }
        let midway = start + Duration::from_millis(250);
        transition.retarget(1.0, midway, duration);
        assert_eq!(transition.value_at(midway), 2.0);
        // Unchanged observations must not restart the segment.
        transition.retarget(1.0, midway + duration / 2, duration);
        assert_eq!(transition.value_at(midway + duration / 2), 1.5);
        assert_eq!(transition.value_at(midway + duration), 1.0);
    }

    #[test]
    fn changing_duration_retimes_the_current_motion_and_zero_finishes_immediately() {
        let start = Instant::now();
        let mut transition = HispeedTransition::new(1.0);
        transition.retarget(3.0, start, Duration::from_millis(500));
        let changed_at = start + Duration::from_millis(125);
        transition.retarget(3.0, changed_at, Duration::from_millis(1000));
        assert_eq!(transition.value_at(changed_at), 1.5);
        assert_eq!(transition.value_at(changed_at + Duration::from_millis(500)), 2.25);
        assert_eq!(transition.value_at(changed_at + Duration::from_millis(1000)), 3.0);
        transition.retarget(3.0, changed_at + Duration::from_millis(500), Duration::ZERO);
        assert_eq!(transition.value_at(changed_at + Duration::from_millis(500)), 3.0);
    }

    #[test]
    fn zero_duration_is_immediate_and_new_sessions_start_at_their_hispeed() {
        let now = Instant::now();
        let mut transition = HispeedTransition::new(2.0);
        assert_eq!(transition.value_at(now), 2.0);
        transition.retarget(4.0, now, Duration::ZERO);
        assert_eq!(transition.value_at(now), 4.0);
        transition.retarget(0.01, now, Duration::from_millis(1000));
        assert_eq!(transition.value_at(now + Duration::from_millis(1000)), 0.01);
    }
}
