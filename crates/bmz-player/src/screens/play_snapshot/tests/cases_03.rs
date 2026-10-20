use super::*;

#[test]
fn speed_at_returns_one_before_first_event() {
    let segments = [(1000.0, 2.0)];
    assert!((super::speed_at(&segments, 500.0) - 1.0).abs() < 1e-6);
    assert!((super::speed_at(&segments, 1000.0) - 2.0).abs() < 1e-6);
    assert!((super::speed_at(&segments, 2000.0) - 2.0).abs() < 1e-6);
}

#[test]
fn speed_at_keeps_last_factor_at_duplicate_tick() {
    let segments = [(0.0, 1.0), (1000.0, 2.0), (1000.0, 3.0), (2000.0, 5.0)];

    assert!((super::speed_at(&segments, 1000.0) - 3.0).abs() < 1e-6);
    assert!((super::speed_at(&segments, 1500.0) - 4.0).abs() < 1e-6);
}

#[test]
fn accumulate_scroll_preserves_boundaries_reverse_and_negative_factors() {
    let segments = [(0.0, 2.0), (100.0, 0.5), (200.0, -1.0)];
    let integral = ScrollIntegralCache::from_segments(segments);

    assert!((super::accumulate_scroll(&segments, 0.0, 50.0) - 100.0).abs() < 1e-6);
    assert!((super::accumulate_scroll(&segments, 0.0, 150.0) - 225.0).abs() < 1e-6);
    assert!((super::accumulate_scroll(&segments, 0.0, 250.0) - 200.0).abs() < 1e-6);
    assert!((super::accumulate_scroll(&segments, 50.0, 250.0) - 100.0).abs() < 1e-6);
    assert!((super::accumulate_scroll(&segments, 250.0, 50.0) + 100.0).abs() < 1e-6);
    assert_eq!(super::accumulate_scroll(&segments, 50.0, 50.0), 0.0);
    for (from_tick, to_tick) in
        [(0.0, 50.0), (0.0, 150.0), (0.0, 250.0), (50.0, 250.0), (250.0, 50.0), (50.0, 50.0)]
    {
        let expected = super::accumulate_scroll(&segments, from_tick, to_tick);
        assert!((integral.delta(from_tick, to_tick) - expected).abs() < 1e-6);
    }

    let duplicate_tick = [(0.0, 2.0), (0.0, 3.0), (100.0, 1.0)];
    assert!((super::accumulate_scroll(&duplicate_tick, 0.0, 50.0) - 150.0).abs() < 1e-6);
    let duplicate_integral = ScrollIntegralCache::from_segments(duplicate_tick);
    assert!((duplicate_integral.delta(0.0, 50.0) - 150.0).abs() < 1e-6);

    let delayed_event = [(100.0, 2.0), (200.0, 0.5)];
    let delayed_integral = ScrollIntegralCache::from_segments(delayed_event);
    for (from_tick, to_tick) in [(0.0, 50.0), (0.0, 100.0), (50.0, 150.0), (250.0, 50.0)] {
        let expected = super::accumulate_scroll(&delayed_event, from_tick, to_tick);
        assert!((delayed_integral.delta(from_tick, to_tick) - expected).abs() < 1e-6);
    }
    assert_eq!(delayed_integral.delta(0.0, f64::EPSILON / 2.0), 0.0);
}

#[test]
fn build_render_snapshot_applies_speed_factor() {
    use bmz_chart::model::SpeedEvent;
    let mut chart = chart();
    chart.speed_events = vec![SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 }];
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
    let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
    assert!((y - 1.0).abs() < 1e-3, "expected ~1.0 with SPEED 2.0, got {y}");
    assert_eq!(snapshot.note_display_duration_ms, 1000);
}

#[test]
fn build_render_snapshot_interpolates_speed_between_events() {
    use bmz_chart::model::SpeedEvent;
    let mut chart = chart();
    // BPM 120 / 4 拍 = 3840 ticks。SPEED を tick=0..3840 で 1.0→3.0 へ補間。
    // chart() のノートは TimeUs(1_000_000) = 1920 ticks (中央) なので、
    // 補間値は 2.0 になる。base 進捗 0.5 × SPEED 2.0 = 1.0 (画面上端)。
    chart.speed_events = vec![
        SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 1.0 },
        SpeedEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: 3.0 },
    ];
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
    let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
    assert!(
        (y - 1.0).abs() < 1e-3,
        "expected ~1.0 from linear interpolation (0.5 base × 2.0 mid speed), got {y}"
    );
}

#[test]
fn build_render_snapshot_compresses_distance_with_scroll_factor_half() {
    use bmz_chart::model::ScrollEvent;
    let mut chart = chart();
    chart.scroll_events = vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 0.5 }];
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    // 1/2 進捗 × SCROLL 0.5 = 1/4 進捗。
    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
    let y = snapshot.visible_notes[Lane::Key1.index()][0].y;
    assert!((y - 0.25).abs() < 1e-3, "expected ~0.25 with SCROLL 0.5, got {y}");
}

#[test]
fn build_render_snapshot_hides_note_with_negative_scroll() {
    use bmz_chart::model::ScrollEvent;
    let mut chart = chart();
    // factor < 0 は逆スクロール。delta が負になり描画対象外。
    chart.scroll_events = vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: -1.0 }];
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);
    assert!(snapshot.visible_notes[Lane::Key1.index()].is_empty());
}

#[test]
fn build_render_snapshot_reports_lane_cover_changing_and_note_display_duration() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 2.0;
    session.lane_cover = 0.25;
    session.lanecover_enabled = true;
    session.lane_cover_visible = true;
    session.lane_cover_changing = true;

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

    assert!(snapshot.lane_cover_changing);
    assert_eq!(snapshot.note_display_duration_ms, 750);
}

#[test]
fn build_render_snapshot_reports_gauge_skin_timer_elapsed() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.gauge_increase_started_at = Some(TimeUs(100_000));
    session.gauge_max_started_at = Some(TimeUs(250_000));

    let snapshot = build_render_snapshot(&session, TimeUs(400_000), &[], None);

    assert_eq!(snapshot.gauge_increase_elapsed_ms, Some(300));
    assert_eq!(snapshot.gauge_max_elapsed_ms, Some(150));
}

#[test]
fn build_render_snapshot_reports_independent_opponent_skin_timers() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions {
            session_mode: SessionMode::GBattle,
            battle_opponent: Some(BattleOpponentOptions {
                replay_player: None,
                gauge: None,
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                arrange_seed: None,
                arrange_seed_2p: None,
                legacy_arrange_seed: false,
                packed_seed: None,
                bms_random_choices: None,
                bms_switch_choices: None,
                arrange_pattern: None,
                s_random_scheme: SRandomScheme::default(),
                s_random_scheme_2p: None,
                h_random_threshold_ms: None,
            }),
            opponent_chart: Some(Arc::new(chart())),
            ..PlaySessionOptions::default()
        },
    );
    let opponent = session.opponent.independent.as_mut().unwrap();
    opponent.full_combo_started_at = Some(TimeUs(100_000));
    opponent.gauge_increase_started_at = Some(TimeUs(150_000));
    opponent.gauge_max_started_at = Some(TimeUs(250_000));

    let snapshot = build_render_snapshot(&session, TimeUs(400_000), &[], None);
    let opponent = snapshot.opponent.as_ref().unwrap();

    assert_eq!(opponent.full_combo_elapsed_ms, Some(300));
    assert_eq!(opponent.gauge_increase_elapsed_ms, Some(250));
    assert_eq!(opponent.gauge_max_elapsed_ms, Some(150));

    // A retained display-only opponent must never override the independent one.
    session.opponent.score = Some(session.score.clone());
    session.opponent.score.as_mut().unwrap().combo = 999;
    session.opponent.gauge = Some(session.gauge.clone());
    session.opponent.full_combo_started_at = Some(TimeUs(900_000));
    let independent = session.opponent.independent.as_mut().unwrap();
    independent.scored_total_notes = 13;
    independent.full_combo_started_at = Some(TimeUs(900_000));
    let snapshot = build_render_snapshot(&session, TimeUs(400_000), &[], None);
    let opponent = snapshot.opponent.unwrap();
    assert_eq!(opponent.total_notes, 13);
    assert_ne!(opponent.combo, 999);
    assert_eq!(opponent.full_combo_elapsed_ms, Some(0));

    session.opponent.independent = None;
    let snapshot = build_render_snapshot(&session, TimeUs(400_000), &[], None);
    let opponent = snapshot.opponent.unwrap();
    assert_eq!(opponent.combo, 999);
    assert_eq!(opponent.total_notes, session.scored_total_notes);
    assert_eq!(opponent.full_combo_elapsed_ms, None);
}

#[test]
fn update_render_snapshot_play_options_refreshes_ready_snapshot_values() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    let mut snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

    session.hispeed = 2.0;
    session.lane_cover = 0.25;
    session.lanecover_enabled = true;
    session.lane_cover_visible = true;
    session.lane_cover_changing = true;
    update_render_snapshot_play_options(&mut snapshot, &session, TimeUs(0));

    assert_eq!(snapshot.hispeed, 2.0);
    assert_eq!(snapshot.lane_cover, 0.25);
    assert!(snapshot.lane_cover_changing);
    assert_eq!(snapshot.note_display_duration_ms, 750);
}

#[test]
fn build_render_snapshot_keeps_notes_after_judge_cursor_advances() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.judge.lanes[Lane::Key1.index()].next_note_index = 1;

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

    assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].processed_judge, None);
}

#[test]
fn build_render_snapshot_routes_invisible_and_mine_correctly() {
    let mut chart = chart();
    chart.lane_notes[Lane::Key2.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key2,
        kind: NoteKind::Invisible,
        tick: ChartTick(0),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.lane_notes[Lane::Key3.index()].push(NoteEvent {
        id: NoteId(3),
        lane: Lane::Key3,
        kind: NoteKind::Mine,
        tick: ChartTick(0),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: Some(8.0),
    });
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

    assert_eq!(snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    assert!(snapshot.visible_notes[Lane::Key2.index()].is_empty());
    assert!(snapshot.visible_notes[Lane::Key3.index()].is_empty());
    // Mine は visible_mines 側に振り分けられる。damage も保持。
    assert_eq!(snapshot.visible_mines[Lane::Key3.index()].len(), 1);
    assert_eq!(snapshot.visible_mines[Lane::Key3.index()][0].damage, 8.0);
    assert!(snapshot.visible_mines[Lane::Key1.index()].is_empty());
    assert!(snapshot.visible_mines[Lane::Key2.index()].is_empty());
}

#[test]
fn build_render_snapshot_copies_recent_inputs() {
    use bmz_core::input::{InputDeviceKind, InputEvent, InputKind, InputSource};

    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.recent_inputs.push(InputEvent {
        lane: Lane::Key3,
        kind: InputKind::Press,
        time: TimeUs(42_000),
        source: InputSource::Human,
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    });

    let snapshot = build_render_snapshot(&session, TimeUs(50_000), &[], None);

    assert_eq!(snapshot.recent_inputs.len(), 1);
    assert_eq!(snapshot.recent_inputs[0].lane, Lane::Key3);
    assert_eq!(snapshot.recent_inputs[0].time, TimeUs(42_000));
}

#[test]
fn build_render_snapshot_sums_judge_counts() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.score.apply(&JudgementEvent {
        note_id: Some(NoteId(1)),
        lane: Lane::Key1,
        judge: Judge::PGreat,
        side: TimingSide::Fast,
        delta: TimeUs(-1_000),
        time: TimeUs(1_000),
        affects_score: true,
    });
    session.score.apply(&JudgementEvent {
        note_id: None,
        lane: Lane::Key1,
        judge: Judge::EmptyPoor,
        side: TimingSide::Slow,
        delta: TimeUs(40_000),
        time: TimeUs(2_000),
        affects_score: true,
    });

    let snapshot = build_render_snapshot(&session, TimeUs(0), &[], None);

    assert_eq!(snapshot.judge_counts.pgreat, 1);
    assert_eq!(snapshot.judge_counts.empty_poor, 1);
    assert_eq!(snapshot.fast_slow_counts.fast_pgreat, 1);
    assert_eq!(snapshot.fast_slow_counts.slow_empty_poor, 1);
}

#[test]
fn build_render_snapshot_marks_replay_playback() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let normal = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    let replay = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions {
            replay_player: Some(bmz_gameplay::replay::ReplayPlayer::default()),
            ..PlaySessionOptions::default()
        },
    );

    assert!(!build_render_snapshot(&normal, TimeUs(0), &[], None).replay_playback);
    assert!(build_render_snapshot(&replay, TimeUs(0), &[], None).replay_playback);
}

#[test]
fn build_render_snapshot_carries_chart_total_gauge() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut source = chart();
    source.metadata.total = Some(350.0);
    let session = build_game_session(Arc::new(source), &profile, PlaySessionOptions::default());

    assert_eq!(build_render_snapshot(&session, TimeUs(0), &[], None).chart_total_gauge, 350.0);
}
