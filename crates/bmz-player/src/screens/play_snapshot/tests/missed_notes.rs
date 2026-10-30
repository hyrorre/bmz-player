use super::*;
use crate::screens::play_snapshot::missed_notes::{MAX_MISSED_NOTE_FALL, MissedNoteFallCache};

#[test]
fn missed_note_cache_matches_scalar_fall_across_bpm_stops_and_compressed_ticks() {
    use bmz_chart::model::{TimingEvent, TimingEventKind};

    let mut chart = chart();
    chart.timing_events = [
        (0, 0, TimingEventKind::Stop { duration_us: 1_000_000 }),
        (960, 1_500_000, TimingEventKind::BpmChange { bpm: 180.0 }),
        (960, 1_500_000, TimingEventKind::Stop { duration_us: 250_000 }),
        (960, 1_750_000, TimingEventKind::Stop { duration_us: 500_000 }),
        (1920, 2_583_333, TimingEventKind::BpmChange { bpm: 60.0 }),
        (2880, 3_583_333, TimingEventKind::Stop { duration_us: 2_000_000 }),
        (2880, 5_583_333, TimingEventKind::BpmChange { bpm: 240.0 }),
        (2880, 5_583_334, TimingEventKind::BpmChange { bpm: 90.0 }),
    ]
    .into_iter()
    .map(|(tick, time, kind)| TimingEvent { tick: ChartTick(tick), time: TimeUs(time), kind })
    .collect();
    let timing = TimingMap::from_chart_timing_events(120.0, &chart.timing_events);
    chart.lane_notes[Lane::Key1.index()] = [0, 480, 960, 1440, 1920, 2880, 3840]
        .into_iter()
        .enumerate()
        .map(|(index, tick)| {
            tap_note(index as u32 + 1, Lane::Key1, tick, timing.tick_to_time(ChartTick(tick)).0)
        })
        .collect();
    let cache = MissedNoteFallCache::new(&chart, &timing);
    for now in [
        -1, 0, 100_000, 1_499_999, 1_500_000, 1_750_000, 2_250_000, 3_000_000, 4_000_000,
        5_583_333, 5_583_334, 6_000_000, 20_000_000, 50_000_000,
    ] {
        for delay in [0, 150_000, 175_000, 1_200_000] {
            let frame = cache.frame(TimeUs(now), delay);
            for (index, note) in chart.lane_notes[Lane::Key1.index()].iter().enumerate() {
                let expected = pms_missed_note_fall_progress(
                    &timing,
                    note.tick,
                    note.time,
                    delay,
                    TimeUs(now),
                );
                let actual = frame.progress(Lane::Key1, index);
                if expected <= MAX_MISSED_NOTE_FALL {
                    assert!(
                        approx_eq(actual.unwrap(), expected),
                        "now={now} delay={delay} note={index}"
                    );
                    assert!(frame.first_note(Lane::Key1) <= index);
                } else {
                    assert_eq!(actual, None);
                }
            }
        }
    }
}

#[test]
fn missed_note_range_preserves_boundary_and_uses_current_delay_after_rewind() {
    let mut chart = chart();
    chart.lane_notes[Lane::Key1.index()] =
        vec![tap_note(1, Lane::Key1, 0, 0), tap_note(2, Lane::Key1, 1920, 1_000_000)];
    let cache = MissedNoteFallCache::new(&chart, &TimingMap::from_chart_timing_events(120.0, &[]));
    let boundary = cache.frame(TimeUs(16_175_000), 175_000);
    assert_eq!(boundary.first_note(Lane::Key1), 0);
    assert_eq!(boundary.progress(Lane::Key1, 0), Some(8.0));
    let expired = cache.frame(TimeUs(17_000_000), 175_000);
    assert_eq!(expired.first_note(Lane::Key1), 1);
    assert_eq!(expired.progress(Lane::Key1, 0), None);
    let wider = cache.frame(TimeUs(17_000_000), 2_000_000);
    assert_eq!(wider.first_note(Lane::Key1), 0);
    assert_eq!(wider.progress(Lane::Key1, 0), Some(7.5));
    let rewound = cache.frame(TimeUs(100_000), 175_000);
    assert_eq!(rewound.first_note(Lane::Key1), 0);
    assert_eq!(rewound.progress(Lane::Key1, 0), Some(0.0));
}

#[test]
fn missed_note_candidate_count_stays_bounded_in_long_chart() {
    let mut chart = chart();
    chart.lane_notes[Lane::Key1.index()] = (0..10_000_u32)
        .map(|index| {
            tap_note(index + 1, Lane::Key1, u64::from(index) * 96, i64::from(index) * 50_000)
        })
        .collect();
    let timing = TimingMap::from_chart_timing_events(120.0, &[]);
    let cache = MissedNoteFallCache::new(&chart, &timing);
    for now in [50_000_000, 450_000_000] {
        let frame = cache.frame(TimeUs(now), 175_000);
        let notes = &chart.lane_notes[Lane::Key1.index()];
        let past = notes.partition_point(|note| note.time.0 < now);
        let first = frame.first_note(Lane::Key1);
        assert!(first > 0);
        assert!(past - first <= 324, "expired history must be skipped before scanning");
        assert!(frame.progress(Lane::Key1, first).is_some());
        assert_eq!(frame.progress(Lane::Key1, first - 1), None);
    }
}

#[test]
fn missed_note_range_keeps_future_notes_with_inconsistent_ticks() {
    let mut chart = chart();
    chart.lane_notes[Lane::Key1.index()] =
        vec![tap_note(1, Lane::Key1, 0, 0), tap_note(2, Lane::Key1, 0, 20_000_000)];
    let cache = MissedNoteFallCache::new(&chart, &TimingMap::from_chart_timing_events(120.0, &[]));
    assert_eq!(cache.frame(TimeUs(17_000_000), 175_000).first_note(Lane::Key1), 1);
}

#[test]
fn missed_note_snapshot_prunes_history_with_retention_and_practice_rates() {
    for key_mode in [KeyMode::K7, KeyMode::K9] {
        for rate in [50, 100, 200] {
            let mut chart = chart();
            chart.metadata.key_mode = key_mode;
            chart.lane_notes[Lane::Key1.index()] =
                vec![tap_note(1, Lane::Key1, 0, 0), tap_note(2, Lane::Key1, 1920, 1_000_000)];
            let mut profile = ProfileConfig::new_default("default", "Default", 1);
            profile.play.note_retention = true;
            let mut session = build_game_session(
                Arc::new(chart),
                &profile,
                PlaySessionOptions { playback_rate_percent: rate, ..Default::default() },
            );
            session.missed_note_fall = key_mode == KeyMode::K7;
            session.judge.judged_notes.insert(NoteId(1), Judge::Poor);
            session.judge.judged_notes.insert(NoteId(2), Judge::Bad);
            let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
            for now in [17_000_000, 100_000] {
                let snapshot = build_render_snapshot_with_target_and_bga_frames_cached(
                    &session,
                    TimeUs(now),
                    &[],
                    None,
                    None,
                    None,
                    &BgaFrameCatalog::new(),
                    &cache,
                );
                for note in
                    session.chart.notes_for_lane(Lane::Key1).iter().filter(|note| note.time.0 < now)
                {
                    let expected = pms_missed_note_fall_progress(
                        &session.timing_map,
                        note.tick,
                        note.time,
                        session.judge.window_set.note.empty_poor_slow_us,
                        TimeUs(now),
                    );
                    let actual = snapshot.visible_notes[Lane::Key1.index()]
                        .iter()
                        .find(|visible| visible.time == note.time);
                    if expected <= MAX_MISSED_NOTE_FALL {
                        assert!(approx_eq(actual.unwrap().pms_miss_fall.unwrap(), expected));
                    } else {
                        assert!(actual.is_none(), "retention must not restore expired misses");
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "CPU benchmark; run alone with --release --ignored --nocapture"]
fn profile_missed_note_snapshot_history() -> anyhow::Result<()> {
    use bmz_chart::model::{TimingEvent, TimingEventKind};
    use bmz_chart::timing::{TickTimingEvent, TickTimingEventKind, build_timing_map};

    anyhow::ensure!(!cfg!(debug_assertions), "run this probe with --release");
    let changes = (1..=500)
        .map(|index| TickTimingEvent {
            tick: ChartTick(index * 1920),
            kind: TickTimingEventKind::SetBpm(if index % 2 == 0 { 120.0 } else { 121.0 }),
        })
        .collect::<Vec<_>>();
    let timing = build_timing_map(120.0, changes.clone());
    let mut chart = chart();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    for index in 0..10_000_u32 {
        let lane = KeyMode::K7.active_lanes()[1 + index as usize % 7];
        let tick = u64::from(index) * 96;
        chart.lane_notes[lane.index()].push(tap_note(
            index + 1,
            lane,
            tick,
            timing.tick_to_time(ChartTick(tick)).0,
        ));
    }
    chart.timing_events = changes
        .into_iter()
        .map(|event| TimingEvent {
            tick: event.tick,
            time: timing.tick_to_time(event.tick),
            kind: match event.kind {
                TickTimingEventKind::SetBpm(bpm) => TimingEventKind::BpmChange { bpm },
                _ => unreachable!(),
            },
        })
        .collect();
    chart.total_notes = 10_000;
    chart.end_time = timing.tick_to_time(ChartTick(960_000));
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    for id in 1..=10_000 {
        session
            .judge
            .judged_notes
            .insert(NoteId(id), if id % 10 == 0 { Judge::Poor } else { Judge::PGreat });
    }
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let bga = BgaFrameCatalog::new();
    for enabled in [false, true] {
        session.missed_note_fall = enabled;
        for fraction in [0.1, 0.9] {
            let now = TimeUs((session.chart.end_time.0 as f64 * fraction) as i64);
            let mut timings = Vec::new();
            for iteration in 0..103 {
                let start = std::time::Instant::now();
                std::hint::black_box(build_render_snapshot_with_target_and_bga_frames_cached(
                    &session,
                    now,
                    &[],
                    None,
                    None,
                    None,
                    &bga,
                    &cache,
                ));
                if iteration >= 3 {
                    timings.push(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            timings.sort_by(f64::total_cmp);
            eprintln!(
                "missed_note_fall={enabled} progress={fraction} median_ms={:.6} p95_ms={:.6}",
                timings[50], timings[94]
            );
        }
    }
    Ok(())
}
