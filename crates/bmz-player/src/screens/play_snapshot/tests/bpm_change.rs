use super::*;
use bmz_chart::model::{ScrollEvent, SpeedEvent, TimingEvent, TimingEventKind};

fn timing_event(tick: u64, time_ms: i64, kind: TimingEventKind) -> TimingEvent {
    TimingEvent { tick: ChartTick(tick), time: TimeUs(time_ms * 1_000), kind }
}

/// 120 BPM -> 150 at 1000ms, STOP + repeated 150 at 1800ms, 100 at 3100ms,
/// then 200 and back to 100 on the same instant at 4300ms.
fn chart_with_preview_changes() -> PlayableChart {
    let mut chart = chart_with_bpm_changes();
    chart.timing_events = vec![
        timing_event(1_920, 1_000, TimingEventKind::BpmChange { bpm: 150.0 }),
        timing_event(3_840, 1_800, TimingEventKind::Stop { duration_us: 500_000 }),
        timing_event(3_840, 1_800, TimingEventKind::BpmChange { bpm: 150.0 }),
        timing_event(5_760, 3_100, TimingEventKind::BpmChange { bpm: 100.0 }),
        timing_event(7_680, 4_300, TimingEventKind::BpmChange { bpm: 200.0 }),
        timing_event(7_680, 4_300, TimingEventKind::BpmChange { bpm: 100.0 }),
    ];
    chart
}

#[test]
fn bpm_change_points_include_stop_edges_and_skip_repeats_and_same_instant_reverts() {
    let points = build_bpm_change_points(&chart_with_preview_changes());
    assert_eq!(
        points,
        vec![
            BpmChangePoint { time: TimeUs(1_000_000), from_bpm: 120.0, to_bpm: 150.0 },
            BpmChangePoint { time: TimeUs(1_800_000), from_bpm: 150.0, to_bpm: 0.0 },
            BpmChangePoint { time: TimeUs(2_300_000), from_bpm: 0.0, to_bpm: 150.0 },
            BpmChangePoint { time: TimeUs(3_100_000), from_bpm: 150.0, to_bpm: 100.0 },
        ]
    );
    assert!(build_bpm_change_points(&chart()).is_empty());
}

#[test]
fn non_stop_bpm_change_points_match_session_now_bpm_switch() {
    let chart = chart_with_preview_changes();
    let timing_map =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    for point in build_bpm_change_points(&chart)
        .into_iter()
        .filter(|point| point.from_bpm != 0.0 && point.to_bpm != 0.0)
    {
        assert_eq!(timing_map.bpm_at_time(TimeUs(point.time.0 - 1)), point.from_bpm);
        assert_eq!(timing_map.bpm_at_time(point.time), point.to_bpm);
    }
}

#[test]
fn bpm_change_preview_reports_next_and_last_change_in_real_time() {
    let points = build_bpm_change_points(&chart_with_preview_changes());

    let before = bpm_change_preview(&points, TimeUs(-200_000), 100);
    let next = before.next.expect("first change");
    assert_eq!(next.remaining_ms, 1_200);
    assert_eq!((next.from_bpm, next.to_bpm), (120.0, 150.0));
    assert_eq!(before.since_last_change_ms, None);

    // The change instant belongs to the new BPM, matching `bpm_at_time`.
    let at_change = bpm_change_preview(&points, TimeUs(1_000_000), 100);
    assert_eq!(at_change.since_last_change_ms, Some(0));
    let next = at_change.next.expect("second change");
    assert_eq!(next.remaining_ms, 800);
    assert_eq!((next.from_bpm, next.to_bpm), (150.0, 0.0));

    let during_stop = bpm_change_preview(&points, TimeUs(2_000_000), 100);
    let next = during_stop.next.expect("STOP release");
    assert_eq!(next.remaining_ms, 300);
    assert_eq!((next.from_bpm, next.to_bpm), (0.0, 150.0));
    assert_eq!(during_stop.since_last_change_ms, Some(200));

    let after_last = bpm_change_preview(&points, TimeUs(5_000_000), 100);
    assert_eq!(after_last.next, None);
    assert_eq!(after_last.since_last_change_ms, Some(1_900));
}

#[test]
fn bpm_change_preview_scales_durations_by_playback_rate() {
    let points = build_bpm_change_points(&chart_with_preview_changes());

    let slow = bpm_change_preview(&points, TimeUs(1_500_000), 50);
    assert_eq!(slow.next.unwrap().remaining_ms, 600);
    assert_eq!(slow.since_last_change_ms, Some(1_000));

    let fast = bpm_change_preview(&points, TimeUs(1_500_000), 200);
    assert_eq!(fast.next.unwrap().remaining_ms, 150);
    assert_eq!(fast.since_last_change_ms, Some(250));
}

#[test]
fn render_snapshot_and_projection_use_lane_clock_for_bpm_change_preview() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(
        Arc::new(chart_with_preview_changes()),
        &profile,
        PlaySessionOptions::default(),
    );
    session.offsets.visual_offset_us = 100_000;

    let snapshot = build_render_snapshot(&session, TimeUs(500_000), &[], None);
    let next = snapshot.bpm_change_preview.next.expect("upcoming change");
    // lane time = 600ms, so the 1000ms change is 400ms away.
    assert_eq!(next.remaining_ms, 400);
    assert_eq!(next.to_bpm, 150.0);

    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut projected = snapshot.clone();
    let mut clock = ProjectionClock::default();
    pool[0].project(&mut projected, TimeUs(1_200_000), &mut clock);
    assert_eq!(projected.now_bpm, 150.0);
    let preview = projected.bpm_change_preview;
    assert_eq!(preview.since_last_change_ms, Some(300));
    let next = preview.next.unwrap();
    assert_eq!(next.remaining_ms, 500);
    assert_eq!(next.to_bpm, 0.0);
}

#[test]
fn scroll_only_changes_include_zero_and_negative_values() {
    let mut chart = chart();
    chart.scroll_events = [(1, 0.0), (2, -1.0), (3, -2.0), (4, 1.0)]
        .into_iter()
        .map(|(second, factor)| ScrollEvent {
            tick: ChartTick(second * 1920),
            time: TimeUs(second as i64 * 1_000_000),
            factor,
        })
        .collect();
    assert_eq!(
        build_bpm_change_points(&chart),
        vec![
            BpmChangePoint { time: TimeUs(1_000_000), from_bpm: 120.0, to_bpm: 0.0 },
            BpmChangePoint { time: TimeUs(2_000_000), from_bpm: 0.0, to_bpm: -120.0 },
            BpmChangePoint { time: TimeUs(3_000_000), from_bpm: -120.0, to_bpm: -240.0 },
            BpmChangePoint { time: TimeUs(4_000_000), from_bpm: -240.0, to_bpm: 120.0 },
        ]
    );
}

#[test]
fn simultaneous_bpm_and_scroll_changes_compare_the_final_product() {
    let mut chart = chart();
    chart.timing_events = vec![timing_event(1920, 1000, TimingEventKind::BpmChange { bpm: 240.0 })];
    chart.scroll_events = vec![
        ScrollEvent { tick: ChartTick(1920), time: TimeUs(1_000_000), factor: 0.5 },
        ScrollEvent { tick: ChartTick(5760), time: TimeUs(2_000_000), factor: 2.0 },
        ScrollEvent { tick: ChartTick(5760), time: TimeUs(2_000_000), factor: 1.0 },
    ];
    assert_eq!(
        build_bpm_change_points(&chart),
        vec![BpmChangePoint { time: TimeUs(2_000_000), from_bpm: 120.0, to_bpm: 240.0 }]
    );
}

#[test]
fn preview_threshold_uses_unrounded_changes_and_includes_exactly_one() {
    let mut chart = chart();
    chart.timing_events = [(1000, 120.75), (2000, 121.75), (3000, 121.0), (4000, 120.0)]
        .into_iter()
        .map(|(time_ms, bpm)| timing_event(0, time_ms, TimingEventKind::BpmChange { bpm }))
        .collect();
    assert_eq!(
        build_bpm_change_points(&chart),
        vec![
            BpmChangePoint { time: TimeUs(2_000_000), from_bpm: 120.75, to_bpm: 121.75 },
            BpmChangePoint { time: TimeUs(4_000_000), from_bpm: 121.0, to_bpm: 120.0 },
        ]
    );
}

#[test]
fn stop_with_simultaneous_bpm_and_scroll_changes_resumes_at_the_new_product() {
    let mut chart = chart();
    chart.timing_events = vec![
        timing_event(1920, 1000, TimingEventKind::BpmChange { bpm: 240.0 }),
        timing_event(1920, 1000, TimingEventKind::Stop { duration_us: 500_000 }),
    ];
    chart.scroll_events = vec![
        ScrollEvent { tick: ChartTick(1920), time: TimeUs(1_000_000), factor: 0.5 },
        ScrollEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: -1.0 },
    ];
    assert_eq!(
        build_bpm_change_points(&chart),
        vec![
            BpmChangePoint { time: TimeUs(1_000_000), from_bpm: 120.0, to_bpm: 0.0 },
            BpmChangePoint { time: TimeUs(1_500_000), from_bpm: 0.0, to_bpm: 120.0 },
            BpmChangePoint { time: TimeUs(2_000_000), from_bpm: 120.0, to_bpm: -240.0 },
        ]
    );
}

#[test]
fn consecutive_stops_have_no_intermediate_resume_and_zero_duration_stop_is_ignored() {
    let mut chart = chart();
    chart.timing_events = vec![
        timing_event(1920, 1000, TimingEventKind::Stop { duration_us: 500_000 }),
        timing_event(1920, 1000, TimingEventKind::Stop { duration_us: 250_000 }),
        timing_event(3840, 2750, TimingEventKind::Stop { duration_us: 0 }),
    ];
    assert_eq!(
        build_bpm_change_points(&chart),
        vec![
            BpmChangePoint { time: TimeUs(1_000_000), from_bpm: 120.0, to_bpm: 0.0 },
            BpmChangePoint { time: TimeUs(1_750_000), from_bpm: 0.0, to_bpm: 120.0 },
        ]
    );
}

#[test]
fn speed_interpolation_does_not_create_preview_changes() {
    let mut chart = chart();
    chart.speed_events =
        vec![SpeedEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: 3.0 }];
    assert!(build_bpm_change_points(&chart).is_empty());
}

#[test]
fn scroll_preview_matches_renderer_tick_boundaries_even_with_legacy_event_times() {
    let mut chart = chart();
    chart.scroll_events = vec![ScrollEvent { tick: ChartTick(1920), time: TimeUs(0), factor: 2.0 }];
    let points = build_bpm_change_points(&chart);
    assert_eq!(
        points,
        vec![BpmChangePoint { time: TimeUs(1_000_000), from_bpm: 120.0, to_bpm: 240.0 }]
    );
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.offsets.visual_offset_us = 100_000;
    let snapshot = build_render_snapshot(&session, TimeUs(500_000), &[], None);
    let next = snapshot.bpm_change_preview.next.unwrap();
    assert_eq!((next.remaining_ms, next.from_bpm, next.to_bpm), (400, 120.0, 240.0));
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let pool = PlayfieldProjection::pool(&session, &cache);
    let mut projected = snapshot.clone();
    pool[0].project(&mut projected, TimeUs(1_000_000), &mut ProjectionClock::default());
    assert_eq!(projected.bpm_change_preview.since_last_change_ms, Some(100));
    assert_eq!(projected.bpm_change_preview.next, None);
}
