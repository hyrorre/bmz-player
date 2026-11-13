use super::*;

#[test]
fn positive_scroll_bound_keeps_every_visible_tick_across_changes_and_duplicates() {
    let timing = bmz_chart::timing::build_timing_map(120.0, Vec::new());
    for segments in [
        vec![],
        vec![(0.0, 1.0)],
        vec![(96.0, 0.5), (192.0, 4.0), (192.0, 0.25), (400.0, 2.0)],
        vec![(0.0, 0.000001), (600.0, 1000.0)],
    ] {
        let cache = ScrollIntegralCache::from_segments(segments);
        for cursor in [-50.0, 0.0, 95.9, 96.0, 191.9, 192.0, 399.0, 600.0, 2000.0] {
            for speed in [0.01, 1.0, 10.0] {
                let context = ScrollContext {
                    timing_map: &timing,
                    hispeed: 2.345,
                    visible_lane_fraction: 0.73,
                    lookahead_ticks: 768.0,
                    scroll_integral: &cache,
                    speed,
                };
                let upper = context.simple_tick_upper_bound(cursor).unwrap();
                for tick in (0..10000).map(|tick| tick as f64 * 0.25).chain([
                    upper - 0.000001,
                    upper,
                    upper + 0.000001,
                ]) {
                    let progress = context.progress_from_delta(context.scroll_delta(cursor, tick));
                    if (0.0..=1.0).contains(&progress) {
                        assert!(tick <= upper, "cursor={cursor}, tick={tick}, upper={upper}");
                    }
                }
            }
        }
    }
}

#[test]
fn non_forward_scroll_and_speed_keep_unrestricted_candidates() {
    let timing = bmz_chart::timing::build_timing_map(120.0, Vec::new());
    for factor in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let cache = ScrollIntegralCache::from_segments([(1000.0, factor)]);
        assert!(cache.tick_after_distance(0.0, 100.0).is_none());
    }
    let cache = ScrollIntegralCache::from_segments([]);
    for speed in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        let context = ScrollContext {
            timing_map: &timing,
            hispeed: 1.0,
            visible_lane_fraction: 1.0,
            lookahead_ticks: 768.0,
            scroll_integral: &cache,
            speed,
        };
        assert!(context.simple_tick_upper_bound(0.0).is_none());
    }
}

#[test]
fn large_scroll_history_keeps_notes_whose_distance_rounds_to_zero() {
    let cache = ScrollIntegralCache::from_segments([(0.0, 1e100), (100.0, 1e-100)]);
    assert_eq!(cache.delta(200.0, 10_000.0), 0.0);
    let bound = cache.tick_after_distance(200.0, 768.0).unwrap_or(f64::INFINITY);
    assert!(bound >= 10_000.0);
}
