use super::*;

#[test]
fn add_ln_is_visible_and_shows_its_hold_with_or_without_extra_notes() {
    use std::fmt::Write;
    use std::path::Path;

    use bmz_chart::import::{AssetLookup, BmsRandomSource, import_chart_bytes_with_assets};

    use crate::assist::{apply_chart_assists, apply_extra_note_assist};
    use crate::config::profile_config::{AssistLongNoteMode, AssistOptionConfig};

    let mut bms = String::from("#PLAYER 1\n#BPM 120\n#WAV01 tap.wav\n#WAV02 bgm.wav\n");
    bms.push_str("#00112:01\n#00101:00020000\n");
    // Seed 0 converts two later Key1 taps before the early Key2 tap.
    for measure in 3..19 {
        writeln!(bms, "#{measure:03}11:01\n#{measure:03}01:00020000").unwrap();
    }
    let source = import_chart_bytes_with_assets(
        Path::new("add-ln.bms"),
        bms.as_bytes(),
        BmsRandomSource::Seed(Some(0)),
        AssetLookup::Declared,
    )
    .unwrap()
    .chart;
    let profile = ProfileConfig::new_default("default", "Default", 1);
    for depth in [0, 1, 16] {
        let mut chart = source.clone();
        let config = AssistOptionConfig {
            long_note_mode: AssistLongNoteMode::AddLn,
            long_note_rate: 0.15,
            extra_note_depth: depth,
            ..Default::default()
        };
        let mut runtime = apply_chart_assists(&mut chart, config, 0);
        assert_eq!(
            apply_extra_note_assist(&mut chart, config, &mut runtime, false).is_some(),
            depth > 0
        );
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.hispeed = 1.0;
        let pair = session.chart.long_notes.iter().find(|ln| ln.lane == Lane::Key2).unwrap();
        let (start, end, id) = (pair.start_time, pair.end_time, pair.start_note_id);

        let before = build_render_snapshot(&session, TimeUs(start.0 - 150_000), &[], None);
        assert_eq!(before.visible_long_notes.len(), 1, "depth={depth}");
        assert_eq!(before.visible_long_notes[0].lane, Lane::Key2);
        assert_eq!(before.visible_long_notes[0].body_state, LongBodyState::Inactive);

        let input = |kind, time| InputEvent {
            lane: Lane::Key2,
            kind,
            time,
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        };
        session.judge.process_input(&session.chart, input(InputKind::Press, start));
        let held = build_render_snapshot(&session, TimeUs(start.0 + 150_000), &[], None);
        assert_eq!(held.visible_long_notes.len(), 1, "depth={depth}");
        assert_eq!(held.visible_long_notes[0].lane, Lane::Key2);
        assert_eq!(held.visible_long_notes[0].body_state, LongBodyState::Processing);

        session.judge.process_input(&session.chart, input(InputKind::Release, end));
        assert_eq!(session.judge.judged_notes.get(&id), Some(&Judge::PGreat));
    }
}

#[test]
fn early_capture_starts_the_hold_in_every_long_note_mode() {
    use bmz_chart::model::LongNoteMode::{Cn, Hcn, Hln, Ln};
    use bmz_gameplay::session::update_hcn_lane_timers;

    let profile = ProfileConfig::new_default("default", "Default", 1);
    let now = TimeUs(490_000);
    let lane = Lane::Key1.index();
    for mode in [Ln, Cn, Hcn, Hln] {
        let mut chart = chart_with_long_note();
        chart.long_notes[0].mode = Some(mode);
        let mut session =
            build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
        session.judge.process_input(
            &session.chart,
            InputEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: now,
                source: InputSource::Human,
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            },
        );
        update_hcn_lane_timers(&mut session, now);
        let snap = build_render_snapshot(&session, now, &[], None);
        assert_eq!(snap.visible_long_notes[0].body_state, LongBodyState::Processing, "{mode:?}");
        assert_eq!(snap.hold_ms[lane], Some(0), "{mode:?}");
        assert_eq!(snap.hcn_active_ms[lane], (mode == Hln).then_some(0), "{mode:?}");
    }
}

#[test]
fn negative_bpm_fold_moves_past_unjudged_notes_back_into_the_snapshot() {
    use bmz_chart::model::{BarLine, TimingEvent, TimingEventKind};

    let mut chart = chart();
    chart.lane_notes[Lane::Key1.index()][0] = tap_note(1, Lane::Key1, 1_728, 900_000);
    chart.bar_lines.push(BarLine { measure: 1, tick: ChartTick(1_728), time: TimeUs(900_000) });
    chart.timing_events.push(TimingEvent {
        tick: ChartTick(1_920),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmReflection,
    });
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    let before = build_render_snapshot(&session, TimeUs(800_000), &[], None);
    let at = build_render_snapshot(&session, TimeUs(1_000_000), &[], None);
    let after = build_render_snapshot(&session, TimeUs(1_200_000), &[], None);

    assert!(at.visible_notes[Lane::Key1.index()].is_empty());
    assert_eq!(after.visible_notes[Lane::Key1.index()].len(), 1);
    assert!(approx_eq(
        before.visible_notes[Lane::Key1.index()][0].y,
        after.visible_notes[Lane::Key1.index()][0].y
    ));
    assert!(approx_eq(before.bar_lines[0].y, after.bar_lines[0].y));
    assert_eq!(after.now_bpm, 120.0);

    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    let [projection, ..] = super::super::projection::PlayfieldProjection::pool(&session, &cache);
    let mut projected = before;
    projection.project(&mut projected, TimeUs(1_200_000), &mut Default::default());
    assert_eq!(projected.time, TimeUs(1_200_000));
    assert!(approx_eq(
        projected.visible_notes[Lane::Key1.index()][0].y,
        after.visible_notes[Lane::Key1.index()][0].y
    ));
}

#[test]
fn negative_bpm_fold_keeps_the_missed_hln_body_until_its_end() {
    use bmz_chart::model::{LongNoteMode, TimingEvent, TimingEventKind};
    use bmz_render::snapshot::LongBodyState;

    let mut chart = chart_with_long_note();
    chart.long_notes[0].mode = Some(LongNoteMode::Hln);
    chart.timing_events.push(TimingEvent {
        tick: ChartTick(1_920),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmReflection,
    });
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    session.judge.process_misses(&session.chart, TimeUs(900_000));
    assert_eq!(session.judge.judged_notes.get(&NoteId(1)), Some(&Judge::Poor));
    assert!(session.judge.lanes[Lane::Key1.index()].active_long.is_none());

    let after = build_render_snapshot(&session, TimeUs(1_010_000), &[], None);
    assert_eq!(after.visible_long_notes.len(), 1);
    assert_eq!(after.visible_long_notes[0].body_state, LongBodyState::HcnDamage);

    session.judge.process_input(
        &session.chart,
        InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(1_020_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        },
    );
    let held = build_render_snapshot(&session, TimeUs(1_020_000), &[], None);
    assert_eq!(held.visible_long_notes.len(), 1);
    assert_eq!(held.visible_long_notes[0].body_state, LongBodyState::HcnActive);
    let ended = build_render_snapshot(&session, TimeUs(1_500_000), &[], None);
    assert!(ended.visible_long_notes.is_empty());
}

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
                    reflection_tick: None,
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
            reflection_tick: None,
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
