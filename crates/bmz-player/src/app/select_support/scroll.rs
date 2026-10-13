use std::time::{Duration, Instant};

/// Signed distance from the displayed position to the selected row, in row units.
/// The selection timer is independent: retargeting must sample this clock first.
#[derive(Debug, Default)]
pub(in crate::app) struct SelectBarScroll {
    started_at: Option<Instant>,
    duration: Duration,
    remaining: f64,
}

impl SelectBarScroll {
    pub(in crate::app) fn remaining_at(&self, now: Instant) -> f64 {
        let Some(started_at) = self.started_at else { return 0.0 };
        if self.duration.is_zero() {
            return 0.0;
        }
        let progress =
            now.saturating_duration_since(started_at).as_secs_f64() / self.duration.as_secs_f64();
        self.remaining * (1.0 - progress).clamp(0.0, 1.0)
    }

    pub(in crate::app) fn move_by(&mut self, rows: i32, now: Instant, duration: Duration) {
        self.remaining = self.remaining_at(now) + f64::from(rows);
        self.started_at = Some(now);
        self.duration = duration;
    }

    pub(in crate::app) fn reset(&mut self) {
        *self = Self::default();
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
        assert!((scroll.remaining_at(again) - 1.4).abs() < 1e-9);
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
                let expected = 7.0 * (1.0 - elapsed.as_secs_f64());
                assert!((scroll.remaining_at(now + elapsed) - expected).abs() < 1e-8);
            }
        }
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
