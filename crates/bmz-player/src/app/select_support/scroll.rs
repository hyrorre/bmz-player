use std::time::{Duration, Instant};

/// Signed distance from the displayed position to the selected row, in row units.
/// The selection timer is independent: retargeting must sample this clock first.
#[derive(Debug, Default)]
pub(in crate::app) struct SelectBarScroll {
    started_at: Option<Instant>,
    duration: Duration,
    remaining: f64,
    remaining_velocity: f64,
    /// The current segment started from rest, so its velocity is the start velocity.
    from_rest: bool,
}

impl SelectBarScroll {
    pub(in crate::app) fn remaining_at(&self, now: Instant) -> f64 {
        self.sample_at(now).0
    }

    /// Evaluate the residual and its derivative (rows/second) on the same clock.
    fn sample_at(&self, now: Instant) -> (f64, f64) {
        let Some(started_at) = self.started_at else { return (0.0, 0.0) };
        if self.duration.is_zero() {
            return (0.0, 0.0);
        }
        let seconds = self.duration.as_secs_f64();
        let u = now.saturating_duration_since(started_at).as_secs_f64() / seconds;
        if u >= 1.0 {
            return (0.0, 0.0);
        }
        // Cubic Hermite to residual=0, velocity=0. Factoring (1-u) avoids
        // cancellation near arrival; do not ease each individual row interval.
        let rest = 1.0 - u;
        let tangent = self.remaining_velocity * seconds;
        let remaining = rest * rest * (self.remaining * (1.0 + 2.0 * u) + tangent * u);
        let velocity = rest * (-6.0 * self.remaining * u + tangent * (1.0 - 3.0 * u)) / seconds;
        (remaining, velocity)
    }

    pub(in crate::app) fn move_by(&mut self, rows: i32, now: Instant, duration: Duration) {
        let (remaining, carried) = self.sample_at(now);
        let remaining = remaining + f64::from(rows);
        // Only a move from rest gets an initial velocity; retargeting keeps the
        // carried velocity so held and reversed moves stay continuous. Moves
        // batched at the start instant re-derive it, matching a single jump.
        let from_rest = carried == 0.0 || (self.from_rest && self.started_at == Some(now));
        let seconds = duration.as_secs_f64();
        let velocity = if from_rest && seconds > 0.0 {
            -START_TANGENT_PER_DISTANCE * remaining / seconds
        } else {
            carried
        };
        self.remaining = remaining;
        self.remaining_velocity = velocity;
        self.from_rest = from_rest;
        self.started_at = Some(now);
        self.duration = bounded_scroll_duration(self.remaining, velocity, duration);
    }

    pub(in crate::app) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Hermite start tangent in distance units: 0 is smoothstep, 3 is easeOutCubic.
const START_TANGENT_PER_DISTANCE: f64 = 1.5;

fn bounded_scroll_duration(remaining: f64, velocity: f64, requested: Duration) -> Duration {
    if requested.is_zero() || velocity == 0.0 {
        return requested;
    }
    let limit = if remaining * velocity < 0.0 {
        // A tangent of at most 3 times the distance is monotone. Shorten time
        // rather than clamp velocity, preserving continuity at retargeting.
        3.0 * remaining.abs() / velocity.abs()
    } else {
        // Reversal (or retargeting exactly onto the current position) must
        // first brake. h10=u*(1-u)^2 <= 4/27 bounds travel outside the interval
        // between the current position and the target to a quarter row.
        const MAX_REVERSAL_DRIFT_ROWS: f64 = 0.25;
        MAX_REVERSAL_DRIFT_ROWS * 27.0 / (4.0 * velocity.abs())
    };
    if limit < requested.as_secs_f64() {
        Duration::from_secs_f64(limit).max(Duration::from_nanos(1))
    } else {
        requested
    }
}

pub(in crate::app) fn select_scroll_repeat_interval(duration: Duration) -> Duration {
    // LR2 connects held movements before the preceding movement finishes.
    duration.saturating_sub(Duration::from_millis(20).min(duration / 2))
}

pub(in crate::app) fn select_scroll_center_index(
    selected_index: usize,
    item_len: usize,
    remaining: f64,
) -> usize {
    if item_len == 0 {
        return 0;
    }
    let whole = remaining.trunc().rem_euclid(item_len as f64) as usize;
    (selected_index + item_len - whole) % item_len
}

pub(in crate::app) fn select_scroll_fraction(remaining: f64) -> f32 {
    // Split in f64 before converting: e.g. 1.99999999 must not become 2.0
    // and lose a whole row when a separate visual window already consumed it.
    (remaining.abs().fract() as f32).min(1.0 - f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_and_reversed_moves_preserve_displayed_position() {
        let now = Instant::now();
        let duration = Duration::from_millis(100);
        let mut scroll = SelectBarScroll::default();
        scroll.move_by(1, now, duration);
        let again = now + Duration::from_millis(60);
        let before = 1.0 - scroll.remaining_at(again);
        scroll.move_by(1, again, duration);
        let rest = 0.4_f64;
        let expected = 1.0 + rest * rest * (2.2 - START_TANGENT_PER_DISTANCE * 0.6);
        assert!((scroll.remaining_at(again) - expected).abs() < 1e-9);
        assert!((2.0 - scroll.remaining_at(again) - before).abs() < 1e-9);
        let reverse = again + Duration::from_millis(10);
        let before = 2.0 - scroll.remaining_at(reverse);
        scroll.move_by(-1, reverse, duration);
        assert!((1.0 - scroll.remaining_at(reverse) - before).abs() < 1e-9);
        assert_eq!(scroll.remaining_at(reverse + duration), 0.0);
    }

    #[test]
    fn batch_page_and_wrap_keep_the_entire_distance() {
        let now = Instant::now();
        let mut scroll = SelectBarScroll::default();
        scroll.move_by(7, now, Duration::from_millis(100));
        assert_eq!(scroll.remaining_at(now), 7.0);
        assert_eq!(select_scroll_center_index(2, 10, 7.0), 5);
        assert_eq!(select_scroll_center_index(0, 10, 1.4), 9);
        assert_eq!(select_scroll_center_index(9, 10, -1.4), 0);
        scroll.move_by(3, now, Duration::from_millis(100));
        assert_eq!(scroll.remaining_at(now), 10.0);
        scroll.reset();
        assert_eq!(scroll.remaining_at(now), 0.0);
        scroll.move_by(-2, now, Duration::ZERO);
        assert_eq!(scroll.remaining_at(now), 0.0);
        assert_eq!(select_scroll_center_index(0, 0, 100.0), 0);
    }

    #[test]
    fn jump_matches_five_simultaneous_single_moves() {
        let now = Instant::now();
        let duration = Duration::from_millis(100);
        let mut jump = SelectBarScroll::default();
        jump.move_by(5, now, duration);
        let mut presses = SelectBarScroll::default();
        for _ in 0..5 {
            presses.move_by(1, now, duration);
        }
        for ms in [0, 10, 40, 70, 99, 100] {
            let at = now + Duration::from_millis(ms);
            assert!((jump.remaining_at(at) - presses.remaining_at(at)).abs() < 1e-9);
        }
    }

    #[test]
    fn held_moves_connect_without_unbounded_backlog() {
        let now = Instant::now();
        let duration = Duration::from_millis(50);
        let interval = select_scroll_repeat_interval(duration);
        assert_eq!(interval, Duration::from_millis(30));
        let mut scroll = SelectBarScroll::default();
        let mut target = 0.0;
        for step in 0..100 {
            let time = now + interval * step;
            let before = target - scroll.remaining_at(time);
            scroll.move_by(1, time, duration);
            target += 1.0;
            assert!((target - scroll.remaining_at(time) - before).abs() < 1e-9);
            assert!(scroll.remaining_at(time) < 1.667);
        }
        assert_eq!(scroll.remaining_at(now + interval * 99 + duration), 0.0);
        assert!(!select_scroll_repeat_interval(Duration::from_millis(1)).is_zero());
    }

    #[test]
    fn interpolation_uses_elapsed_time_at_60_120_and_144_hz() {
        let now = Instant::now();
        for hz in [60, 120, 144] {
            let mut scroll = SelectBarScroll::default();
            scroll.move_by(7, now, Duration::from_secs(1));
            for frame in 0..=hz {
                let elapsed = Duration::from_secs_f64(f64::from(frame) / f64::from(hz));
                let u = elapsed.as_secs_f64();
                let expected =
                    7.0 * (1.0 - u).powi(2) * (1.0 + (2.0 - START_TANGENT_PER_DISTANCE) * u);
                assert!((scroll.remaining_at(now + elapsed) - expected).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn retargeting_preserves_position_and_velocity_with_different_durations() {
        let start = Instant::now();
        let mut scroll = SelectBarScroll::default();
        let mut target = 0.0;
        for (elapsed_ms, rows, duration_ms) in
            [(0, 1, 300), (90, 1, 50), (100, -1, 300), (105, 7, 100), (130, -36, 50)]
        {
            let now = start + Duration::from_millis(elapsed_ms);
            let (remaining, velocity) = scroll.sample_at(now);
            let position = target - remaining;
            scroll.move_by(rows, now, Duration::from_millis(duration_ms));
            target += f64::from(rows);
            let (remaining, after_velocity) = scroll.sample_at(now);
            assert!((target - remaining - position).abs() < 1e-9);
            if velocity == 0.0 {
                let start_velocity = -START_TANGENT_PER_DISTANCE * remaining
                    / Duration::from_millis(duration_ms).as_secs_f64();
                assert!((after_velocity - start_velocity).abs() < 1e-9);
            } else {
                assert!((after_velocity - velocity).abs() < 1e-9);
            }
            let epsilon = Duration::from_nanos(1);
            let difference =
                (scroll.remaining_at(now + epsilon) - remaining) / epsilon.as_secs_f64();
            assert!(
                (difference - after_velocity).abs() < 0.01,
                "time={elapsed_ms}, difference={difference}, velocity={velocity}"
            );
        }
    }

    #[test]
    fn arrival_is_exact_and_rest_starts_with_initial_velocity() {
        let start = Instant::now();
        let duration = Duration::from_millis(100);
        let seconds = duration.as_secs_f64();
        let mut scroll = SelectBarScroll::default();
        scroll.move_by(7, start, duration);
        let (remaining, velocity) = scroll.sample_at(start);
        assert_eq!(remaining, 7.0);
        assert!((velocity + START_TANGENT_PER_DISTANCE * 7.0 / seconds).abs() < 1e-9);
        let half = 7.0 * 0.25 * (2.0 - START_TANGENT_PER_DISTANCE * 0.5);
        assert!((scroll.remaining_at(start + duration / 2) - half).abs() < 1e-9);
        assert_eq!(scroll.sample_at(start + duration), (0.0, 0.0));
        assert_eq!(scroll.sample_at(start + duration * 2), (0.0, 0.0));
        let (_, velocity) = scroll.sample_at(start + duration - Duration::from_nanos(1));
        assert!(velocity.abs() < 1e-5);
        scroll.move_by(-1, start + duration, duration);
        let (remaining, velocity) = scroll.sample_at(start + duration);
        assert_eq!(remaining, -1.0);
        assert!((velocity - START_TANGENT_PER_DISTANCE / seconds).abs() < 1e-9);
    }

    #[test]
    fn reversal_brakes_with_bounded_drift_and_no_position_clamp() {
        let start = Instant::now();
        let mut scroll = SelectBarScroll::default();
        let initial_duration = Duration::from_millis(50);
        scroll.move_by(1, start, initial_duration);
        let reverse = start + initial_duration / 2;
        let (before_remaining, before_velocity) = scroll.sample_at(reverse);
        scroll.move_by(-1, reverse, Duration::from_millis(300));
        assert!(scroll.duration < Duration::from_millis(300));
        let initial_remaining = before_remaining - 1.0;
        assert_eq!(scroll.sample_at(reverse), (initial_remaining, before_velocity));
        assert!(scroll.remaining_at(reverse + Duration::from_micros(1)) < initial_remaining);
        let mut changed_direction = false;
        for frame in 0..=1000 {
            let now = reverse + scroll.duration.mul_f64(f64::from(frame) / 1000.0);
            let (remaining, velocity) = scroll.sample_at(now);
            assert!(remaining >= initial_remaining - 0.25 - 1e-8);
            assert!(remaining <= 1e-9);
            changed_direction |= velocity > 0.0;
        }
        assert!(changed_direction);
        assert_eq!(scroll.sample_at(reverse + scroll.duration), (0.0, 0.0));
    }

    #[test]
    fn nearby_target_shortens_time_without_overshooting_or_changing_velocity() {
        let start = Instant::now();
        let mut scroll = SelectBarScroll::default();
        scroll.move_by(2, start, Duration::from_millis(100));
        let again = start + Duration::from_millis(20);
        let (remaining, velocity) = scroll.sample_at(again);
        scroll.move_by(-1, again, Duration::from_millis(300));
        assert!(scroll.duration < Duration::from_millis(300));
        assert_eq!(scroll.sample_at(again).0, remaining - 1.0);
        assert!((scroll.sample_at(again).1 - velocity).abs() < 1e-9);
        let mut previous = remaining - 1.0;
        for frame in 0..=1000 {
            let now = again + scroll.duration.mul_f64(f64::from(frame) / 1000.0);
            let (remaining, velocity) = scroll.sample_at(now);
            assert!(remaining >= -1e-9 && remaining <= previous + 1e-9);
            assert!(velocity <= 1e-8);
            previous = remaining;
        }
    }

    #[test]
    fn zero_distance_retarget_brakes_then_returns_and_reset_discards_velocity() {
        let start = Instant::now();
        // A carried smoothstep segment exactly one row from its target midway.
        let mut scroll = SelectBarScroll {
            started_at: Some(start),
            duration: Duration::from_millis(100),
            remaining: 2.0,
            ..Default::default()
        };
        let again = start + Duration::from_millis(50);
        let velocity = scroll.sample_at(again).1;
        assert!(velocity != 0.0);
        scroll.move_by(-1, again, Duration::from_millis(300));
        assert_eq!(scroll.sample_at(again), (0.0, velocity));
        assert!(scroll.remaining_at(again + scroll.duration / 3) < 0.0);
        assert!(scroll.remaining_at(again + scroll.duration / 3).abs() <= 0.25 + 1e-8);
        assert_eq!(scroll.sample_at(again + scroll.duration), (0.0, 0.0));
        scroll.reset();
        assert_eq!(scroll.sample_at(again), (0.0, 0.0));
        scroll.move_by(-7, again, Duration::from_millis(100));
        let (remaining, velocity) = scroll.sample_at(again);
        assert_eq!(remaining, -7.0);
        assert!((velocity - START_TANGENT_PER_DISTANCE * 7.0 / 0.1).abs() < 1e-9);
        scroll.move_by(1, again + Duration::from_millis(10), Duration::ZERO);
        assert_eq!(scroll.sample_at(again + Duration::from_millis(10)), (0.0, 0.0));
    }

    #[test]
    fn visual_window_fraction_cannot_round_across_a_row_boundary() {
        for remaining in [0.999_999_999, 1.999_999_999, -1.999_999_999, 1_000_000.4] {
            let fraction = select_scroll_fraction(remaining);
            assert!((0.0..1.0).contains(&fraction));
            assert!((f64::from(fraction) - remaining.abs().fract()).abs() < 1e-6);
        }
        assert_eq!(select_scroll_fraction(100.0), 0.0);
    }
}
