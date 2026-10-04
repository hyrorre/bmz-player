use super::*;

#[test]
fn rhythm_timer_resets_at_bar_lines_and_integrates_bpm_and_stops() {
    use bmz_chart::timing::{TickTimingEvent, TickTimingEventKind, build_timing_map};

    let constant = build_timing_map(120.0, Vec::new());
    let bars = vec![
        BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) },
        BarLine { measure: 1, tick: ChartTick(3_840), time: TimeUs(2_000_000) },
    ];
    assert_eq!(rhythm_timer_elapsed_ms(&constant, &bars, TimeUs(-1)), None);
    assert_eq!(rhythm_timer_elapsed_ms(&constant, &bars, TimeUs(1_000_000)), Some(2_000));
    assert_eq!(rhythm_timer_elapsed_ms(&constant, &bars, TimeUs(2_500_000)), Some(1_000));

    let bpm_change = build_timing_map(
        120.0,
        vec![TickTimingEvent { tick: ChartTick(960), kind: TickTimingEventKind::SetBpm(60.0) }],
    );
    assert_eq!(rhythm_timer_elapsed_ms(&bpm_change, &[], TimeUs(1_500_000)), Some(2_000));

    let with_stop = build_timing_map(
        120.0,
        vec![TickTimingEvent {
            tick: ChartTick(960),
            kind: TickTimingEventKind::StopRaw { value: 96 },
        }],
    );
    assert_eq!(rhythm_timer_elapsed_ms(&with_stop, &[], TimeUs(1_500_000)), Some(3_000));
}

#[test]
fn quarter_note_timer_tracks_real_milliseconds_since_latest_quarter() {
    use bmz_chart::timing::{TickTimingEvent, TickTimingEventKind, build_timing_map};

    let bars = vec![
        BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) },
        BarLine { measure: 1, tick: ChartTick(3_840), time: TimeUs(2_000_000) },
    ];
    let constant = build_timing_map(120.0, Vec::new());
    assert_eq!(quarter_note_elapsed_ms(&constant, &bars, TimeUs(-1)), None);
    assert_eq!(quarter_note_elapsed_ms(&constant, &bars, TimeUs(505_000)), Some(5));
    assert_eq!(quarter_note_elapsed_ms(&constant, &bars, TimeUs(1_158_000)), Some(158));

    let bpm_change = build_timing_map(
        120.0,
        vec![TickTimingEvent { tick: ChartTick(960), kind: TickTimingEventKind::SetBpm(60.0) }],
    );
    assert_eq!(quarter_note_elapsed_ms(&bpm_change, &bars, TimeUs(1_505_000)), Some(5));
}

#[test]
fn pms_missed_note_fall_waits_for_late_bad_and_uses_no_speed_rate() {
    use bmz_chart::timing::{TickTimingEvent, TickTimingEventKind, build_timing_map};

    let constant = build_timing_map(120.0, Vec::new());
    assert!(approx_eq(
        pms_missed_note_fall_progress(&constant, ChartTick(0), TimeUs(0), 200_000, TimeUs(200_000),),
        0.0,
    ));
    assert!(approx_eq(
        pms_missed_note_fall_progress(&constant, ChartTick(0), TimeUs(0), 200_000, TimeUs(700_000),),
        0.25,
    ));

    let with_stop = build_timing_map(
        120.0,
        vec![TickTimingEvent {
            tick: ChartTick(0),
            kind: TickTimingEventKind::StopRaw { value: 96 },
        }],
    );
    assert!(approx_eq(
        pms_missed_note_fall_progress(
            &with_stop,
            ChartTick(0),
            TimeUs(0),
            200_000,
            TimeUs(1_200_000),
        ),
        0.0,
    ));
    assert!(approx_eq(
        pms_missed_note_fall_progress(
            &with_stop,
            ChartTick(0),
            TimeUs(0),
            200_000,
            TimeUs(1_700_000),
        ),
        0.25,
    ));
}

#[test]
fn simple_scroll_ranges_skip_past_chart_objects() {
    let bars = vec![
        BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) },
        BarLine { measure: 1, tick: ChartTick(960), time: TimeUs(500_000) },
        BarLine { measure: 2, tick: ChartTick(1_920), time: TimeUs(1_000_000) },
        BarLine { measure: 3, tick: ChartTick(2_880), time: TimeUs(1_500_000) },
        BarLine { measure: 4, tick: ChartTick(3_840), time: TimeUs(2_000_000) },
    ];
    let visible = visible_bar_lines(&bars, TimeUs(1_000_000), Some((1_920.0, 2_880.0)));
    assert_eq!(visible.iter().map(|bar| bar.tick.0).collect::<Vec<_>>(), vec![1_920, 2_880]);

    let events = vec![
        bmz_chart::model::TimingEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            kind: TimingEventKind::BpmChange { bpm: 120.0 },
        },
        bmz_chart::model::TimingEvent {
            tick: ChartTick(1_920),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::Stop { duration_us: 100_000 },
        },
        bmz_chart::model::TimingEvent {
            tick: ChartTick(2_880),
            time: TimeUs(1_500_000),
            kind: TimingEventKind::BpmChange { bpm: 150.0 },
        },
        bmz_chart::model::TimingEvent {
            tick: ChartTick(3_840),
            time: TimeUs(2_000_000),
            kind: TimingEventKind::Stop { duration_us: 100_000 },
        },
    ];
    let visible = visible_timing_events(&events, TimeUs(1_000_000), Some((1_920.0, 2_880.0)));
    assert_eq!(visible.iter().map(|event| event.tick.0).collect::<Vec<_>>(), vec![1_920, 2_880]);
}

#[test]
fn simple_scroll_long_note_range_keeps_crossing_notes_without_past_scan() {
    use bmz_chart::model::{LongNotePair, LongNoteStyle};

    let long = |start_tick: u64, end_tick: u64| LongNotePair {
        lane: Lane::Key1,
        style: LongNoteStyle::ChannelPair,
        mode: None,
        start_note_id: NoteId(start_tick as u32),
        end_note_id: NoteId(end_tick as u32),
        start_tick: ChartTick(start_tick),
        end_tick: ChartTick(end_tick),
        start_time: TimeUs(start_tick as i64),
        end_time: TimeUs(end_tick as i64),
        sound: None,
    };
    let longs = vec![long(0, 960), long(480, 3_000), long(1_920, 2_880), long(4_000, 5_000)];
    let prefix = [960, 3_000, 3_000, 5_000];
    let visible = visible_long_notes(&longs, &prefix, TimeUs(1_920), Some((1_920.0, 3_000.0)))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(visible, vec![1, 2]);
}

#[test]
fn simple_scroll_time_line_range_uses_visible_seconds_only() {
    use bmz_chart::timing::build_timing_map;

    let timing_map = build_timing_map(120.0, Vec::new());
    let upper_tick = timing_map.time_to_tick_f64(TimeUs(184_000_000));
    assert_eq!(
        visible_time_line_seconds(&timing_map, 600, TimeUs(180_100_000), Some(upper_tick)),
        181..185
    );
    // SCROLL/SPEED path keeps all candidates because negative SCROLL can make
    // past lines visible again.
    assert_eq!(visible_time_line_seconds(&timing_map, 600, TimeUs(180_100_000), None), 1..601);
}

#[test]
fn simple_scroll_time_line_range_handles_stop_tick_boundaries() {
    use bmz_chart::timing::{TickTimingEvent, TickTimingEventKind, build_timing_map};

    let timing_map = build_timing_map(
        120.0,
        vec![TickTimingEvent {
            tick: ChartTick(1_920),
            kind: TickTimingEventKind::StopRaw { value: 192 },
        }],
    );
    let during_stop_tick = timing_map.time_to_tick_f64(TimeUs(1_050_000));
    assert!(
        timing_map.time_to_tick_f64(TimeUs(1_000_000))
            <= timing_map.time_to_tick_f64(TimeUs(1_050_000))
    );
    assert!(
        timing_map.time_to_tick_f64(TimeUs(1_050_000))
            <= timing_map.time_to_tick_f64(TimeUs(1_100_000))
    );

    let visible = visible_time_line_seconds(&timing_map, 5, TimeUs(0), Some(during_stop_tick));
    for second in 1..=5 {
        let tick = timing_map.time_to_tick_f64(TimeUs(second * 1_000_000));
        assert_eq!(visible.contains(&second), tick <= during_stop_tick);
    }
}

#[test]
fn fast_slow_filter_applies_threshold_only_to_pgreat() {
    use crate::config::profile_config::FastSlowDisplayScope;
    let judgement = |judge, side, delta_us| {
        display_judgement(
            &JudgementEvent {
                note_id: Some(NoteId(1)),
                lane: Lane::Key1,
                judge,
                side,
                delta: TimeUs(delta_us),
                time: TimeUs(1_000),
                affects_score: true,
            },
            1,
            TimeUs(1_000),
        )
    };

    // ThresholdMs: 閾値内の PGREAT は side だけでなく ±ms 表示も隠す。
    let mut snapshot = RenderSnapshot {
        recent_judgements: vec![judgement(Judge::PGreat, TimingSide::Fast, -4_999)],
        ..RenderSnapshot::default()
    };
    apply_fast_slow_display_filter(&mut snapshot, 5, FastSlowDisplayScope::ThresholdMs);
    assert_eq!(snapshot.recent_judgements[0].side, None);
    assert_eq!(snapshot.recent_judgements[0].text, "PGREAT");
    assert!(snapshot.recent_judgements[0].timing_ms_suppressed);

    // ThresholdMs: 閾値ちょうどの PGREAT は両方表示。
    let mut snapshot = RenderSnapshot {
        recent_judgements: vec![judgement(Judge::PGreat, TimingSide::Fast, -5_000)],
        ..RenderSnapshot::default()
    };
    apply_fast_slow_display_filter(&mut snapshot, 5, FastSlowDisplayScope::ThresholdMs);
    assert_eq!(snapshot.recent_judgements[0].side, Some(TimingSide::Fast));
    assert_eq!(snapshot.recent_judgements[0].text, "PGREAT FAST");
    assert!(!snapshot.recent_judgements[0].timing_ms_suppressed);

    // ThresholdMs: GREAT 以下は閾値内でも FAST/SLOW と ±ms 表示を保持する。
    for (judge, side, delta_us) in [
        (Judge::Great, TimingSide::Fast, -2_000),
        (Judge::Good, TimingSide::Slow, 2_000),
        (Judge::Bad, TimingSide::Fast, -2_000),
        (Judge::Poor, TimingSide::Slow, 2_000),
        (Judge::EmptyPoor, TimingSide::Fast, -2_000),
    ] {
        let mut snapshot = RenderSnapshot {
            recent_judgements: vec![judgement(judge, side, delta_us)],
            ..RenderSnapshot::default()
        };
        apply_fast_slow_display_filter(&mut snapshot, 5, FastSlowDisplayScope::ThresholdMs);
        assert_eq!(snapshot.recent_judgements[0].side, Some(side), "judge={judge:?}");
        assert!(!snapshot.recent_judgements[0].timing_ms_suppressed, "judge={judge:?}");
        assert!(
            snapshot.recent_judgements[0].text.ends_with(match side {
                TimingSide::Fast => " FAST",
                TimingSide::Slow => " SLOW",
            }),
            "judge={judge:?} text={}",
            snapshot.recent_judgements[0].text
        );
    }

    // Auto: 通常プレイの PGREAT は side を隠すが ±ms 表示は beatoraja 準拠で隠さない。
    let mut snapshot = RenderSnapshot {
        recent_judgements: vec![judgement(Judge::PGreat, TimingSide::Fast, -2_000)],
        ..RenderSnapshot::default()
    };
    apply_fast_slow_display_filter(&mut snapshot, 5, FastSlowDisplayScope::Auto);
    assert_eq!(snapshot.recent_judgements[0].side, None);
    assert!(!snapshot.recent_judgements[0].timing_ms_suppressed);

    // Replay でも Auto は GREAT 以下表示として扱い、PGREAT side は隠す。
    let mut replay_snapshot = RenderSnapshot {
        replay_playback: true,
        recent_judgements: vec![judgement(Judge::PGreat, TimingSide::Fast, -2_000)],
        ..RenderSnapshot::default()
    };
    apply_fast_slow_display_filter(&mut replay_snapshot, 5, FastSlowDisplayScope::Auto);
    assert_eq!(replay_snapshot.recent_judgements[0].side, None);
    assert!(!replay_snapshot.recent_judgements[0].timing_ms_suppressed);

    // ThresholdMs + 0ms は全判定表示なので、リプレイ PGREAT の FAST/SLOW も保持する。
    let mut replay_all_snapshot = RenderSnapshot {
        replay_playback: true,
        recent_judgements: vec![judgement(Judge::PGreat, TimingSide::Fast, -2_000)],
        ..RenderSnapshot::default()
    };
    apply_fast_slow_display_filter(&mut replay_all_snapshot, 0, FastSlowDisplayScope::ThresholdMs);
    assert_eq!(replay_all_snapshot.recent_judgements[0].side, Some(TimingSide::Fast));
    assert!(!replay_all_snapshot.recent_judgements[0].timing_ms_suppressed);
}

#[test]
fn bga_texture_ids_do_not_overlap_beatoraja_skin_ranges() {
    // skin_loader::SkinKind::first_texture_id と同じ割当。
    const SELECT_SKIN_BASE: u32 = 20_000;
    const RESULT_SKIN_BASE: u32 = 30_000;
    // result スキンが数千 PNG あっても BGA 帯に届かないこと。
    const MAX_RESULT_SKIN_TEXTURES: u32 = 10_000;

    const {
        assert!(CHART_BGA_TEXTURE_BASE >= RESULT_SKIN_BASE + MAX_RESULT_SKIN_TEXTURES);
        assert!(CHART_BGA_TEXTURE_BASE > SELECT_SKIN_BASE);
    }
    assert_eq!(bga_texture_id(BgaAssetId(0)), CHART_BGA_TEXTURE_BASE);
}

#[test]
fn display_duration_uses_current_bpm_and_absolute_lane_range() {
    assert_eq!(display_duration_ms_for_bpm_hispeed(120.0, 1.0, 0.0, 0.0, 1.0).round() as i32, 2000);
    assert_eq!(display_duration_ms_for_bpm_hispeed(240.0, 1.0, 0.0, 0.0, 1.0).round() as i32, 1000);
    assert_eq!(
        display_duration_ms_for_bpm_hispeed(0.96, 1.0, 0.0, 0.0, 1.0).round() as i32,
        250_000
    );
    assert_eq!(display_duration_ms_for_bpm_hispeed(88.0, 2.75, 0.0, 0.0, 1.0).round() as i32, 992);
    assert_eq!(display_duration_ms_for_bpm_hispeed(88.0, 2.75, 0.59, 0.0, 1.0).round() as i32, 407);
    assert_eq!(
        display_duration_ms_for_bpm_hispeed(120.0, 1.0, 0.25, 0.2, 1.0).round() as i32,
        1100
    );
    assert_eq!(display_duration_ms_for_bpm_hispeed(120.0, 1.0, 0.0, 0.0, 2.0).round() as i32, 1000);
    assert_eq!(
        display_duration_ms_for_bpm_hispeed(
            effective_bpm_for_playback_rate(120.0, 200) as f32,
            1.0,
            0.0,
            0.0,
            1.0,
        )
        .round() as i32,
        1000
    );
}

#[test]
fn build_render_snapshot_filters_visible_notes_and_formats_judgements() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    let judgements = vec![JudgementEvent {
        note_id: Some(NoteId(1)),
        lane: Lane::Key1,
        judge: Judge::EmptyPoor,
        side: TimingSide::Slow,
        delta: TimeUs(5_000),
        time: TimeUs(1_000),
        affects_score: true,
    }];

    let snapshot = build_render_snapshot(&session, TimeUs(0), &judgements, None);

    assert_eq!(snapshot.combo, 0);
    assert_eq!(snapshot.max_combo, 0);
    assert_eq!(snapshot.ex_score, 0);
    assert_eq!(snapshot.total_notes, 1);
    assert_eq!(snapshot.past_notes, 0);
    assert!(snapshot.recent_inputs.is_empty());
    assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].y, 0.5);
    assert_eq!(snapshot.recent_judgements[0].lane, Lane::Key1);
    assert_eq!(snapshot.recent_judgements[0].text, "EMPTY POOR SLOW");
    assert_eq!(snapshot.recent_judgements[0].delta_us, 5_000);
}

#[test]
fn build_render_snapshot_preserves_dp_combo_at_each_judgement() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.score.combo = 6;
    let judgements = [
        JudgementEvent {
            note_id: None,
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Fast,
            delta: TimeUs(0),
            time: TimeUs(1_000),
            affects_score: true,
        },
        JudgementEvent {
            note_id: None,
            lane: Lane::Key8,
            judge: Judge::PGreat,
            side: TimingSide::Fast,
            delta: TimeUs(0),
            time: TimeUs(2_000),
            affects_score: true,
        },
    ];
    session.recent_display_judgements = vec![
        bmz_gameplay::session::DisplayJudgementEvent {
            judgement: judgements[0].clone(),
            combo: 5,
            display_time: judgements[0].time,
        },
        bmz_gameplay::session::DisplayJudgementEvent {
            judgement: judgements[1].clone(),
            combo: 6,
            display_time: judgements[1].time,
        },
    ];
    session.recent_judgements = judgements.to_vec();

    let snapshot = build_render_snapshot(&session, TimeUs(3_000), &judgements, None);

    assert_eq!(
        snapshot
            .recent_judgements
            .iter()
            .map(|event| (event.lane, event.combo))
            .collect::<Vec<_>>(),
        [(Lane::Key1, 5), (Lane::Key8, 6)]
    );
}

#[test]
fn visual_offset_moves_lane_objects_without_advancing_effect_timers() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    session.offsets.visual_offset_us = 500_000;
    session.full_combo_started_at = Some(TimeUs(0));
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(0));

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

    assert_eq!(snapshot.time, TimeUs(0));
    assert_eq!(snapshot.play_elapsed_time, TimeUs(0));
    assert_eq!(snapshot.full_combo_elapsed_ms, Some(0));
    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], Some(0));
    let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
    assert!((y - 0.25).abs() < 1e-3, "expected lane y ~0.25, got {y}");
}

#[test]
fn judged_notes_remain_visible_until_their_scheduled_time() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let outcome = session.judge.process_input(
        &session.chart,
        InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(990_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        },
    );
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(session.judge.lanes[Lane::Key1.index()].next_note_index, 1);

    let before_scheduled_time = build_render_snapshot(&session, TimeUs(990_000), &[], None);
    assert_eq!(before_scheduled_time.visible_notes[Lane::Key1.index()].len(), 1);
    assert_eq!(
        before_scheduled_time.visible_notes[Lane::Key1.index()][0].processed_judge,
        Some(outcome.events[0].judge)
    );

    let after_scheduled_time = build_render_snapshot(&session, TimeUs(1_000_001), &[], None);
    assert!(after_scheduled_time.visible_notes[Lane::Key1.index()].is_empty());
}

#[test]
fn note_retention_keeps_unjudged_tap_on_judge_line_until_poor() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.note_retention = true;
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let retained = build_render_snapshot(&session, TimeUs(1_000_001), &[], None);
    let notes = &retained.visible_notes[Lane::Key1.index()];
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].y, 0.0);
    assert_eq!(notes[0].processed_judge, None);

    let miss_time = TimeUs(
        1_000_000_i64.saturating_add(session.judge.window_set.note.bad_slow_us).saturating_add(1),
    );
    let outcome = session.judge.process_misses(&session.chart, miss_time);
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].judge, Judge::Poor);

    let processed = build_render_snapshot(&session, miss_time, &outcome.events, None);
    assert!(processed.visible_notes[Lane::Key1.index()].is_empty());
}

#[test]
fn note_retention_off_culls_unjudged_tap_after_scheduled_time() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let snapshot = build_render_snapshot(&session, TimeUs(1_000_001), &[], None);

    assert!(!session.note_retention);
    assert!(snapshot.visible_notes[Lane::Key1.index()].is_empty());
}

#[test]
fn note_retention_uses_visual_time_but_waits_for_gameplay_judgement() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.note_retention = true;
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    session.offsets.visual_offset_us = 50_000;

    let snapshot = build_render_snapshot(&session, TimeUs(960_000), &[], None);
    let notes = &snapshot.visible_notes[Lane::Key1.index()];

    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].y, 0.0);
    assert_eq!(notes[0].processed_judge, None);
}

#[test]
fn note_retention_does_not_keep_past_mines_or_long_notes() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.note_retention = true;

    let mut mine_chart = chart();
    mine_chart.lane_notes[Lane::Key1.index()][0].kind = NoteKind::Mine;
    mine_chart.lane_notes[Lane::Key1.index()][0].damage = Some(8.0);
    let mine_session =
        build_game_session(Arc::new(mine_chart), &profile, PlaySessionOptions::default());
    let mine_snapshot = build_render_snapshot(&mine_session, TimeUs(1_000_001), &[], None);
    assert!(mine_snapshot.visible_mines[Lane::Key1.index()].is_empty());

    let long_session = build_game_session(
        Arc::new(chart_with_long_note()),
        &profile,
        PlaySessionOptions::default(),
    );
    let long_snapshot = build_render_snapshot(&long_session, TimeUs(1_500_001), &[], None);
    assert!(long_snapshot.visible_long_notes.is_empty());
}

#[test]
fn build_render_snapshot_culls_past_and_far_future_notes() {
    let mut chart = chart();
    chart.lane_notes[Lane::Key1.index()] = vec![
        tap_note(1, Lane::Key1, 960, 500_000),
        tap_note(2, Lane::Key1, 1_920, 1_000_000),
        tap_note(3, Lane::Key1, 3_840, 2_000_000),
        tap_note(4, Lane::Key1, 7_680, 4_000_000),
    ];
    chart.total_notes = 4;
    chart.end_time = TimeUs(4_000_000);
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let snapshot = build_render_snapshot(&session, TimeUs(1_000_000), &[], None);
    let notes = &snapshot.visible_notes[Lane::Key1.index()];

    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].time, TimeUs(1_000_000));
    assert_eq!(notes[0].y, 0.0);
    assert_eq!(notes[1].time, TimeUs(2_000_000));
    assert_eq!(notes[1].y, 0.5);
}

#[test]
fn build_render_snapshot_keeps_k9_poor_note_while_falling() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K9;
    chart.lane_notes[Lane::Key1.index()] = vec![tap_note(1, Lane::Key1, 0, 0)];
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.note_retention = true;
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    session.judge.judged_notes.insert(NoteId(1), Judge::Poor);
    let bad_slow_us = session.judge.window_set.note.bad_slow_us.max(0);

    let falling =
        build_render_snapshot(&session, TimeUs(bad_slow_us.saturating_add(500_000)), &[], None);
    let notes = &falling.visible_notes[Lane::Key1.index()];
    assert_eq!(notes.len(), 1);
    assert!(notes[0].y < 0.0);

    let finished =
        build_render_snapshot(&session, TimeUs(bad_slow_us.saturating_add(2_500_000)), &[], None);
    assert!(finished.visible_notes[Lane::Key1.index()].is_empty());
}

#[test]
fn end_of_note_timer_counts_from_last_note_time() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    let before = build_render_snapshot(&session, TimeUs(999_999), &[], None);
    let at_end = build_render_snapshot(&session, TimeUs(1_000_000), &[], None);
    let after = build_render_snapshot(&session, TimeUs(1_250_000), &[], None);

    assert_eq!(before.end_of_note_elapsed_ms, None);
    assert_eq!(at_end.end_of_note_elapsed_ms, Some(0));
    assert_eq!(after.end_of_note_elapsed_ms, Some(250));
}
