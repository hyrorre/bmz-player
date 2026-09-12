use super::*;

#[test]
fn build_game_session_maps_profile_bga_expand() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    profile.play.bga_expand = BgaExpandConfig::Full;
    let full = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    profile.play.bga_expand = BgaExpandConfig::KeepAspect;
    let keep = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    profile.play.bga_expand = BgaExpandConfig::Off;
    let off = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(full.bga_stretch, 0);
    assert_eq!(keep.bga_stretch, 1);
    assert_eq!(off.bga_stretch, 8);
}

#[test]
fn build_game_session_maps_profile_bga_mode() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    profile.play.bga = BgaModeConfig::Off;
    let off = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    profile.play.bga = BgaModeConfig::Auto;
    let auto_human = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    let auto_autoplay = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions { autoplay: true, ..PlaySessionOptions::default() },
    );
    let auto_replay = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions {
            replay_player: Some(ReplayPlayer::default()),
            ..PlaySessionOptions::default()
        },
    );

    assert!(!off.bga_enabled);
    assert!(!auto_human.bga_enabled);
    assert!(auto_autoplay.bga_enabled);
    assert!(auto_replay.bga_enabled);
}

#[test]
fn build_game_session_copies_selected_play_slot_offsets() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.skin.play7_offsets.push(crate::config::profile_config::SkinOffsetConfig {
        name: None,
        id: 42,
        x: 1,
        y: 2,
        w: 3,
        h: 4,
        r: 5,
        a: -6,
    });
    profile.skin.play14_offsets.push(crate::config::profile_config::SkinOffsetConfig {
        name: None,
        id: 42,
        h: 99,
        ..Default::default()
    });

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(
        session.skin_offsets,
        vec![PlaySkinOffset { id: 42, x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 }]
    );
}

#[test]
fn build_game_session_uses_active_offsets_instead_of_skin_history() {
    use crate::config::profile_config::{SkinHistoryEntryConfig, SkinOffsetConfig};

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.skin.play7 = "data/skins/ECFN/play/play7.luaskin".to_string();
    profile.skin.play7_offsets = vec![SkinOffsetConfig { id: 30, h: 12, ..Default::default() }];
    profile.skin.history.insert(
        profile.skin.play7.clone(),
        SkinHistoryEntryConfig {
            offsets: vec![SkinOffsetConfig { id: 30, h: 48, ..Default::default() }],
            ..Default::default()
        },
    );

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(
        session.skin_offsets,
        vec![PlaySkinOffset { id: 30, x: 0, y: 0, w: 0, h: 12, r: 0, a: 0 }]
    );
}

#[test]
fn build_game_session_keeps_active_offsets_with_empty_skin_history() {
    use crate::config::profile_config::{SkinHistoryEntryConfig, SkinOffsetConfig};

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.skin.play7 = "resource:skins/ECFN/play/play7.luaskin".to_string();
    profile.skin.play7_offsets = vec![
        SkinOffsetConfig { id: 43, a: 180, ..Default::default() },
        SkinOffsetConfig { id: 44, a: 110, ..Default::default() },
    ];
    profile.skin.history.insert(profile.skin.play7.clone(), SkinHistoryEntryConfig::default());

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(
        session.skin_offsets,
        vec![
            PlaySkinOffset { id: 43, x: 0, y: 0, w: 0, h: 0, r: 0, a: 180 },
            PlaySkinOffset { id: 44, x: 0, y: 0, w: 0, h: 0, r: 0, a: 110 },
        ]
    );
}

#[test]
fn build_game_session_clamps_profile_hispeed() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hispeed = 21.0;
    let high = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    profile.lane.hispeed = 0.005;
    let low = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(high.hispeed, 20.0);
    assert_eq!(low.hispeed, 0.01);
}

#[test]
fn build_game_session_preserves_green_number_below_legacy_hispeed_floor() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hispeed = 0.5;
    profile.lane.sudden = 286;
    profile.lane.lift = 150;
    profile.lane.lift_enabled = true;
    profile.lane.target_green_number = 251;
    let mut high_bpm_chart = chart();
    high_bpm_chart.metadata.initial_bpm = 2_222.0;

    let session = build_game_session(
        Arc::new(high_bpm_chart),
        &profile,
        PlaySessionOptions { hs_fix: HsFixOption::StartBpm, ..PlaySessionOptions::default() },
    );

    assert!((session.hispeed - 0.145_620_94).abs() < 0.000_001, "hispeed={}", session.hispeed);
    assert!(session.hispeed < 0.5);
    let duration_ms = crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
        2_222.0,
        session.hispeed,
        session.lane_cover,
        session.lift,
        1.0,
    )
    .round() as i32;
    assert_eq!(bmz_render::skin::duration_to_green_number_ms(duration_ms), 251);
}

#[test]
fn build_game_session_initializes_floating_hispeed_for_chart_bpm() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    // Stale value from a 120 BPM chart with green number 300.
    profile.lane.hispeed = 4.0;
    let mut fast_chart = chart();
    fast_chart.metadata.initial_bpm = 240.0;

    let session = build_game_session(
        Arc::new(fast_chart),
        &profile,
        PlaySessionOptions { hs_fix: HsFixOption::StartBpm, ..PlaySessionOptions::default() },
    );

    assert_eq!(session.hispeed_mode, HispeedMode::Floating);
    assert_eq!(session.target_green_number, 300);
    assert!((session.hispeed - 2.0).abs() < f32::EPSILON);
}

#[test]
fn build_game_session_compensates_floating_hispeed_for_practice_rate() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.target_green_number = 300;
    let mut fast_chart = chart();
    fast_chart.metadata.initial_bpm = 240.0;

    let normal = build_game_session(
        Arc::new(fast_chart.clone()),
        &profile,
        PlaySessionOptions {
            hs_fix: HsFixOption::StartBpm,
            playback_rate_percent: 100,
            ..PlaySessionOptions::default()
        },
    );
    let double_speed = build_game_session(
        Arc::new(fast_chart),
        &profile,
        PlaySessionOptions {
            session_mode: SessionMode::Practice,
            hs_fix: HsFixOption::StartBpm,
            playback_rate_percent: 200,
            ..PlaySessionOptions::default()
        },
    );

    assert!((double_speed.hispeed - normal.hispeed / 2.0).abs() < f32::EPSILON);
    assert_eq!(double_speed.audio_clock.playback_rate_percent(), 200);
    let duration_ms = crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
        crate::screens::play_snapshot::effective_bpm_for_playback_rate(240.0, 200) as f32,
        double_speed.hispeed,
        double_speed.lane_cover,
        double_speed.lift,
        1.0,
    )
    .round() as i32;
    assert_eq!(bmz_render::skin::duration_to_green_number_ms(duration_ms), 300);
}

#[test]
fn build_game_session_scales_judge_windows_to_keep_practice_wall_time_fixed() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let normal = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    let mut double_speed = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions {
            session_mode: SessionMode::Practice,
            playback_rate_percent: 200,
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(
        double_speed.judge.window_set.note.pgreat_us,
        normal.judge.window_set.note.pgreat_us * 2
    );
    assert_eq!(
        double_speed.judge.window_set.note.bad_slow_us,
        normal.judge.window_set.note.bad_slow_us * 2
    );
    assert_eq!(double_speed.base_judge_windows, normal.base_judge_windows);

    bmz_gameplay::session::sync_judge_windows(&mut double_speed, TimeUs(0));
    assert_eq!(
        double_speed.judge.window_set.note.pgreat_us,
        normal.judge.window_set.note.pgreat_us * 2
    );
}

#[test]
fn practice_gauge_inherits_profile_auto_shift_mode() {
    use crate::config::profile_config::{
        BottomShiftableGaugeConfig, GaugeAutoShiftConfig, GaugeTypeConfig,
    };

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.gauge = GaugeTypeConfig::Hard;
    profile.play.gauge_auto_shift = GaugeAutoShiftConfig::BestClear;
    profile.play.bottom_shiftable_gauge = BottomShiftableGaugeConfig::Normal;
    let mut options = PlaySessionOptions {
        gauge_override: Some(GaugeType::Hard),
        gauge_auto_shift: GaugeAutoShiftMode::BestClear,
        bottom_shiftable_gauge: GaugeType::Normal,
        ..PlaySessionOptions::default()
    };

    let (selected, mode, bottom) =
        practice_gauge_runtime_options(&profile, PracticeGaugeType::Easy, &options);
    assert_eq!(selected, GaugeType::Easy);
    assert_eq!(mode, GaugeAutoShiftMode::BestClear);
    assert_eq!(bottom, GaugeType::Normal);
    let session = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions {
            session_mode: SessionMode::Practice,
            gauge_override: Some(selected),
            gauge_auto_shift: mode,
            bottom_shiftable_gauge: bottom,
            ..PlaySessionOptions::default()
        },
    );
    assert_eq!(session.gauge.auto_shift_mode, GaugeAutoShiftMode::BestClear);
    assert_eq!(session.gauge.selected, GaugeType::Hazard);
    assert_eq!(session.gauge.bottom_shiftable_gauge, GaugeType::Normal);

    profile.play.gauge_auto_shift = GaugeAutoShiftConfig::SelectToUnder;
    options.gauge_auto_shift = GaugeAutoShiftMode::SelectToUnder;
    let (selected, mode, _) =
        practice_gauge_runtime_options(&profile, PracticeGaugeType::Easy, &options);
    assert_eq!(selected, GaugeType::Hard);
    assert_eq!(mode, GaugeAutoShiftMode::SelectToUnder);
}

#[test]
fn build_game_session_uses_hsfix_to_select_hispeed_mode() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = BaseHispeedConfig::Classic;
    profile.lane.floating_policy = FloatingPolicyConfig::Toggle;
    profile.lane.hispeed = 4.0;
    profile.lane.target_green_number = 300;
    let normal = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    assert_eq!(normal.hispeed_mode, HispeedMode::Classic);
    assert_eq!(normal.hispeed, 4.0);

    profile.lane.base_hispeed = BaseHispeedConfig::Normal;
    let floating = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions { hs_fix: HsFixOption::StartBpm, ..PlaySessionOptions::default() },
    );
    assert_eq!(floating.hispeed_mode, HispeedMode::Floating);
    assert!((floating.hispeed - 4.0).abs() < f32::EPSILON);
}

#[test]
fn build_game_session_maps_all_hispeed_configurations() {
    let cases = [
        (HispeedConfigPreset::Normal, HispeedMode::Normal),
        (HispeedConfigPreset::Classic, HispeedMode::Classic),
        (HispeedConfigPreset::Floating, HispeedMode::Floating),
        (HispeedConfigPreset::NormalFloating, HispeedMode::Normal),
        (HispeedConfigPreset::ClassicFloating, HispeedMode::Classic),
    ];

    for (preset, expected_mode) in cases {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.set_hispeed_config(preset);
        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        assert_eq!(session.hispeed_mode, expected_mode, "preset={preset:?}");
    }
}

#[test]
fn build_game_session_applies_no_speed_constraint_without_profile_lane_settings() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hispeed = 4.0;
    profile.lane.sudden = 400;
    profile.lane.lift = 200;
    profile.lane.hidden = 300;
    profile.play.lane_effect = LaneEffectConfig::HiddenSudden;

    let session = build_game_session(
        Arc::new(chart()),
        &profile,
        PlaySessionOptions {
            hs_fix: HsFixOption::MaxBpm,
            speed_constraint: bmz_core::course::CourseSpeedConstraint::NoSpeed,
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(session.hispeed, 1.0);
    assert_eq!(session.hispeed_mode, HispeedMode::Classic);
    assert_eq!(session.lane_cover, 0.0);
    assert_eq!(session.lift, 0.0);
    assert_eq!(session.hidden_cover, 0.0);
}

#[test]
fn build_game_session_initializes_floating_hispeed_for_hsfix_base_bpm() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    let mut bpm_chart = chart();
    bpm_chart.metadata.initial_bpm = 120.0;
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 240.0 },
    });

    let session = build_game_session(
        Arc::new(bpm_chart),
        &profile,
        PlaySessionOptions { hs_fix: HsFixOption::MaxBpm, ..PlaySessionOptions::default() },
    );

    assert_eq!(session.hsfix_base_bpm, 240.0);
    assert_eq!(session.hsfix_index, 2);
    assert!((session.hispeed - 2.0).abs() < f32::EPSILON);
}

#[test]
fn build_practice_session_preserves_preloaded_hsfix_and_rule_mode() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.target_green_number = 300;
    profile.play.rule_mode = RuleMode::Lr2Oraja;
    let mut bpm_chart = chart();
    bpm_chart.metadata.initial_bpm = 120.0;
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 240.0 },
    });
    let options = PlaySessionOptions {
        play_config_key_mode: Some(KeyMode::K7),
        session_mode: SessionMode::Practice,
        hs_fix: HsFixOption::MaxBpm,
        rule_mode: RuleMode::Lr2Oraja,
        ..PlaySessionOptions::default()
    };

    let prepared = build_practice_prepared_from_preloaded(
        preloaded_play_session(bpm_chart),
        &profile,
        &PracticeProperty::default(),
        options,
        Box::new(NullInputBackend),
    );

    assert_eq!(prepared.session.hsfix_index, 2);
    assert_eq!(prepared.session.hsfix_base_bpm, 240.0);
    assert_eq!(prepared.session.hispeed_mode, HispeedMode::Floating);
    assert_eq!(prepared.session.rule_mode, RuleMode::Lr2Oraja);
    assert_eq!(prepared.skin_attempt.hsfix_index, Some(2));
}

#[test]
fn main_bpm_uses_bpm_with_most_notes() {
    let mut bpm_chart = chart();
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 180.0 },
    });
    bpm_chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 0));
    bpm_chart.lane_notes[Lane::Key2.index()].push(note(2, Lane::Key2, 1_100_000));
    bpm_chart.lane_notes[Lane::Key3.index()].push(note(3, Lane::Key3, 1_200_000));
    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        bpm_chart.metadata.initial_bpm,
        &bpm_chart.timing_events,
    );

    assert_eq!(hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MainBpm), 180.0);
}

#[test]
fn main_bpm_ignores_invisible_notes() {
    let mut bpm_chart = chart();
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 180.0 },
    });
    bpm_chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 0));
    bpm_chart.lane_notes[Lane::Key2.index()].push(note(2, Lane::Key2, 1_100_000));
    bpm_chart.lane_notes[Lane::Key3.index()].push(note(3, Lane::Key3, 1_200_000));
    for (id, lane) in [(4, Lane::Key4), (5, Lane::Key5), (6, Lane::Key6), (7, Lane::Key7)] {
        let mut invisible = note(id, lane, 0);
        invisible.kind = NoteKind::Invisible;
        bpm_chart.lane_notes[lane.index()].push(invisible);
    }
    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        bpm_chart.metadata.initial_bpm,
        &bpm_chart.timing_events,
    );

    assert_eq!(hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MainBpm), 180.0);
}

#[test]
fn min_bpm_preserves_positive_values_below_one() {
    let mut bpm_chart = chart();
    bpm_chart.metadata.initial_bpm = 189.0;
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 0.96 },
    });
    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        bpm_chart.metadata.initial_bpm,
        &bpm_chart.timing_events,
    );

    assert_eq!(hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MinBpm), 0.96);
}

#[test]
fn build_game_session_accepts_custom_input_backend() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("Z".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    let chart = Arc::new(chart());
    let mut session = build_game_session_with_input_backend(
        chart,
        &profile,
        PlaySessionOptions::default(),
        Box::new(backend),
    );
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: None,
    };

    let inputs = session.input_system.collect_game_inputs(&ctx);

    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].lane, Lane::Key1);
}

#[test]
fn expanded_g_battle_uses_the_source_key_mode_binding() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("M".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    let mut expanded_chart = chart();
    expanded_chart.metadata.key_mode = KeyMode::K14;
    let mut session = build_game_session_with_input_backend(
        Arc::new(expanded_chart),
        &profile,
        PlaySessionOptions {
            play_config_key_mode: Some(KeyMode::K7),
            session_mode: SessionMode::GBattle,
            ..PlaySessionOptions::default()
        },
        Box::new(backend),
    );
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: None,
    };

    let inputs = session.input_system.collect_game_inputs(&ctx);

    assert!(inputs.is_empty(), "14K-only M binding must not control a 7K battle session");
}

#[test]
fn load_game_session_for_chart_imports_linked_file() {
    let path = write_temp_bms(
        "\
#TITLE Linked
#BPM 120
#00011:01
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let profile = ProfileConfig::new_default("default", "Default", 1);

    let session =
        load_game_session_for_chart(&library_db, chart_id, &profile, PlaySessionOptions::default())
            .unwrap();

    assert_eq!(session.chart.metadata.title, "Linked");

    std::fs::remove_file(path).unwrap();
}

#[test]
fn normal_session_battle_target_preloads_an_expanded_opponent_chart() {
    let path = write_temp_bms(
        "\
#TITLE Battle Target
#BPM 120
#00019:01
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    assert_eq!(imported.chart.metadata.key_mode, KeyMode::K7);
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let options = PlaySessionOptions {
        session_mode: SessionMode::Normal,
        battle_opponent: Some(BattleOpponentOptions {
            replay_player: Some(ReplayPlayer::default()),
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
        ..PlaySessionOptions::default()
    };

    let preloaded = preload_play_session_for_chart(&library_db, chart_id, options, 1.0).unwrap();

    assert_eq!(preloaded.chart.metadata.key_mode, KeyMode::K14);
    assert_eq!(preloaded.chart.total_notes, imported.chart.total_notes);
    assert_eq!(
        preloaded.opponent_chart.as_ref().map(|chart| chart.metadata.key_mode),
        Some(KeyMode::K7)
    );

    std::fs::remove_file(path).unwrap();
}

#[test]
fn g_battle_presents_the_primary_final_arrangement_on_both_sides() {
    let path = write_temp_bms(
        "\
#TITLE G Battle Same Arrangement
#BPM 120
#00011:0100
#00019:0001
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let options = PlaySessionOptions {
        session_mode: SessionMode::GBattle,
        arrange: ArrangeOption::Mirror,
        arrange_2p: ArrangeOption::Normal,
        ..PlaySessionOptions::default()
    };

    let preloaded = preload_play_session_for_chart(&library_db, chart_id, options, 1.0).unwrap();

    assert_eq!(preloaded.chart.metadata.key_mode, KeyMode::K14);
    assert_eq!(preloaded.applied_arrange.arrange_2p, ArrangeOption::Mirror);
    assert_eq!(preloaded.chart.lane_notes[Lane::Key7.index()].len(), 1);
    assert_eq!(preloaded.chart.lane_notes[Lane::Key14.index()].len(), 1);
    assert_eq!(
        preloaded.chart.lane_notes[Lane::Key7.index()][0].time,
        preloaded.chart.lane_notes[Lane::Key14.index()][0].time
    );
    assert_eq!(
        preloaded.chart.lane_notes[Lane::Key1.index()][0].time,
        preloaded.chart.lane_notes[Lane::Key8.index()][0].time
    );

    std::fs::remove_file(path).unwrap();
}

#[test]
fn g_battle_target_replay_judges_its_chart_but_displays_primary_arrangement() {
    let path = write_temp_bms(
        "\
#TITLE G Battle Target Same Arrangement
#BPM 120
#00011:0100
#00019:0001
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let options = PlaySessionOptions {
        session_mode: SessionMode::GBattle,
        arrange: ArrangeOption::Mirror,
        battle_opponent: Some(BattleOpponentOptions {
            replay_player: Some(ReplayPlayer::default()),
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
        ..PlaySessionOptions::default()
    };

    let preloaded = preload_play_session_for_chart(&library_db, chart_id, options, 1.0).unwrap();

    assert_eq!(preloaded.chart.lane_notes[Lane::Key7.index()].len(), 1);
    assert_eq!(preloaded.chart.lane_notes[Lane::Key14.index()].len(), 1);
    assert_eq!(
        preloaded.chart.lane_notes[Lane::Key7.index()][0].time,
        preloaded.chart.lane_notes[Lane::Key14.index()][0].time
    );
    assert_eq!(
        preloaded.chart.lane_notes[Lane::Key1.index()][0].time,
        preloaded.chart.lane_notes[Lane::Key8.index()][0].time
    );
    let opponent = preloaded.opponent_chart.as_ref().expect("opponent chart");
    assert_eq!(opponent.lane_notes[Lane::Key1.index()].len(), 1);
    assert_ne!(
        opponent.lane_notes[Lane::Key1.index()][0].time,
        preloaded.chart.lane_notes[Lane::Key1.index()][0].time
    );

    std::fs::remove_file(path).unwrap();
}

#[test]
fn load_transformed_chart_applies_start_note_margin() {
    let path = write_temp_bms(
        "\
#TITLE Early Note
#BPM 120
#00011:01
#00201:01
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let source_first =
        imported.chart.lane_notes.iter().flatten().map(|note| note.time.0).min().unwrap();
    assert_eq!(source_first, 0);

    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();

    let transformed =
        load_transformed_chart_for_play(&library_db, chart_id, &PlaySessionOptions::default())
            .unwrap();
    let play_first =
        transformed.chart.lane_notes.iter().flatten().map(|note| note.time.0).min().unwrap();
    assert_eq!(play_first, 1_000_000);

    let source = load_source_chart_for_chart(&library_db, chart_id, None).unwrap();
    let source_first_again =
        source.lane_notes.iter().flatten().map(|note| note.time.0).min().unwrap();
    assert_eq!(source_first_again, 0, "source chart must stay unshifted");

    std::fs::remove_file(path).unwrap();
}

#[test]
fn load_transformed_chart_marks_beatoraja_arrange_assists() {
    let path = write_temp_bms(
        "\
#TITLE Arrange Assist
#BPM 120
#00011:01
#00016:01
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();

    for arrange in [
        ArrangeOption::Spiral,
        ArrangeOption::HRandom,
        ArrangeOption::AllScratch,
        ArrangeOption::RandomEx,
        ArrangeOption::SRandomEx,
    ] {
        let options = PlaySessionOptions { arrange, arrange_seed: Some(1), ..Default::default() };
        let transformed = load_transformed_chart_for_play(&library_db, chart_id, &options).unwrap();
        assert_eq!(transformed.applied_arrange.arrange, arrange);
        assert_eq!(
            transformed.assist_runtime.level,
            bmz_gameplay::session::AssistLevel::LightAssist,
            "{arrange:?}"
        );
        assert!(!transformed.assist_runtime.score_update_enabled(), "{arrange:?}");
    }

    let scoreable = load_transformed_chart_for_play(
        &library_db,
        chart_id,
        &PlaySessionOptions {
            arrange: ArrangeOption::SRandom,
            arrange_seed: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(scoreable.assist_runtime.level, bmz_gameplay::session::AssistLevel::None);
    assert!(scoreable.assist_runtime.score_update_enabled());

    std::fs::remove_file(path).unwrap();
}

#[test]
fn load_transformed_chart_applies_only_compatible_key_mode_conversions() {
    let seven_key_path = write_temp_bms(
        "\
#TITLE Seven Key Conversion
#BPM 120
#00011:01
#00012:01
#00013:01
#00014:01
#00015:01
#00018:01
#00019:01
",
    );
    let five_key_path = write_temp_bms(
        "\
#TITLE Five Key Conversion
#BPM 120
#00011:01
",
    );
    let seven_key = import_bms_chart(&seven_key_path, None, true).unwrap();
    let five_key = import_bms_chart(&five_key_path, None, true).unwrap();
    assert_eq!(seven_key.chart.metadata.key_mode, KeyMode::K7);
    assert_eq!(five_key.chart.metadata.key_mode, KeyMode::K5);

    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let seven_key_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &seven_key_path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &seven_key.chart,
        })
        .unwrap();
    let five_key_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &five_key_path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &five_key.chart,
        })
        .unwrap();

    for (conversion, target) in [
        (KeyModeConversionConfig::SpToDp, KeyMode::K14),
        (KeyModeConversionConfig::SevenToNine, KeyMode::K9),
        (KeyModeConversionConfig::SevenToSix, KeyMode::K6),
    ] {
        let options = PlaySessionOptions { key_mode_conversion: conversion, ..Default::default() };
        let transformed =
            load_transformed_chart_for_play(&library_db, seven_key_id, &options).unwrap();
        assert_eq!(transformed.chart.metadata.key_mode, target);
        assert_eq!(transformed.applied_arrange.key_mode_conversion, conversion);
        assert_eq!(
            transformed.score_save_disabled,
            conversion != KeyModeConversionConfig::SevenToNine
        );
    }

    let nine_key_rules = PlaySessionOptions {
        key_mode_conversion: KeyModeConversionConfig::SevenToNine,
        seven_to_nine_rule_mode: SevenToNineRuleMode::Keys9,
        ..Default::default()
    };
    let transformed =
        load_transformed_chart_for_play(&library_db, seven_key_id, &nine_key_rules).unwrap();
    assert_eq!(transformed.chart.metadata.key_mode, KeyMode::K9);
    assert!(transformed.score_save_disabled);

    let incompatible = PlaySessionOptions {
        key_mode_conversion: KeyModeConversionConfig::SevenToNine,
        ..Default::default()
    };
    let transformed =
        load_transformed_chart_for_play(&library_db, five_key_id, &incompatible).unwrap();
    assert_eq!(transformed.chart.metadata.key_mode, KeyMode::K5);
    assert_eq!(transformed.applied_arrange.key_mode_conversion, KeyModeConversionConfig::Off);
    assert!(!transformed.score_save_disabled);

    std::fs::remove_file(seven_key_path).unwrap();
    std::fs::remove_file(five_key_path).unwrap();
}

#[test]
fn load_game_session_counts_cn_ends_from_source_chart() {
    let path = write_temp_bms(
        "\
#TITLE Source CN
#BPM 120
#LNMODE 2
#LNOBJ ZZ
#00011:01ZZ
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    assert_eq!(imported.chart.total_notes, 1);
    assert_eq!(imported.chart.long_notes.len(), 1);
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let stored = library_db.list_charts_by_ids(&[chart_id]).unwrap().remove(0);
    assert_eq!(stored.total_notes, 1);
    assert_eq!(stored.ln_counts.defined_cn_pairs, 1);
    assert_eq!(stored.scored_total_notes_for_setting(LnPolicySetting::AutoLn), 2);
    library_db
        .conn()
        .execute(
            "UPDATE charts SET total_notes = 999, mode = '14K' WHERE id = ?1",
            rusqlite::params![chart_id],
        )
        .unwrap();
    let source_chart = load_source_chart_for_chart(&library_db, chart_id, None).unwrap();
    assert_eq!(source_chart.metadata.key_mode, KeyMode::K5);
    assert_eq!(source_chart.identity.file_sha256, imported.chart.identity.file_sha256);
    assert_eq!(
        scored_note_count_for_chart(&library_db, chart_id, &PlaySessionOptions::default()).unwrap(),
        2,
        "course pre-count must ignore stale library totals"
    );
    let course_ln_fallback = PlaySessionOptions {
        ln_mode_override: Some(bmz_chart::model::LongNoteMode::Ln),
        ..Default::default()
    };
    let fallback_metrics =
        scored_chart_metrics_for_chart(&library_db, chart_id, &course_ln_fallback).unwrap();
    assert_eq!(fallback_metrics.total_notes, 2);
    assert_eq!(fallback_metrics.ln_mode, Some(bmz_chart::model::LongNoteMode::Cn));
    let force_ln_setting = PlaySessionOptions {
        ln_policy_setting: LnPolicySetting::ForceLn,
        ln_mode_override: Some(bmz_chart::model::LongNoteMode::Hcn),
        ..Default::default()
    };
    let transformed =
        load_transformed_chart_for_play(&library_db, chart_id, &force_ln_setting).unwrap();
    assert_eq!(transformed.score_key.ln_policy, crate::ln_policy::LnScorePolicy::ForceLn);
    assert!(transformed.source_ln_profile.has_defined_cn);
    assert!(!transformed.source_ln_profile.has_defined_ln);
    assert!(
        transformed
            .chart
            .long_notes
            .iter()
            .all(|pair| pair.mode == Some(bmz_chart::model::LongNoteMode::Ln))
    );
    let battle = PlaySessionOptions { double_option: DoubleOption::Battle, ..Default::default() };
    assert_eq!(scored_note_count_for_chart(&library_db, chart_id, &battle).unwrap(), 4);
    let profile = ProfileConfig::new_default("default", "Default", 1);

    let session =
        load_game_session_for_chart(&library_db, chart_id, &profile, PlaySessionOptions::default())
            .unwrap();

    assert_eq!(session.chart.total_notes, 1);
    assert_eq!(session.scored_total_notes, 2);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn load_prepared_play_session_for_chart_loads_audio_samples() {
    let (path, wav_path) = write_temp_bms_with_wav(
        "\
#TITLE Prepared
#BPM 120
#WAV01 test.wav
#00011:01
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let profile = ProfileConfig::new_default("default", "Default", 1);

    let prepared = load_prepared_play_session_for_chart(
        &library_db,
        chart_id,
        &profile,
        PlaySessionOptions::default(),
    )
    .unwrap();
    let library_length_ms = library_db.list_charts_by_ids(&[chart_id]).unwrap()[0].length_ms;

    assert_eq!(prepared.session.chart.metadata.title, "Prepared");
    assert_eq!(prepared.chart_length_ms, library_length_ms.max(0) as u64);
    assert_eq!(prepared.audio.mixer.output_sample_rate, 48_000);
    assert!(matches!(prepared.sample_report[0].status, LoadedSampleStatus::Loaded));
    assert!(prepared.audio.samples.get(SoundId(0)).is_some());

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(wav_path).unwrap();
}

#[test]
fn preload_reports_prepared_chart_before_audio_progress() {
    let (path, wav_path) = write_temp_bms_with_wav(
        "\
#TITLE Arrange preview
#BPM 120
#WAV01 test.wav
#00011:01
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    let reported_chart = RefCell::new(None);

    let preloaded = preload_play_session_for_chart_with_callbacks(
        &library_db,
        chart_id,
        PlaySessionOptions {
            arrange: ArrangeOption::Random,
            arrange_seed: Some(42),
            ..Default::default()
        },
        1.0,
        |chart| {
            *reported_chart.borrow_mut() = Some(chart.clone());
        },
        |_, _| {
            assert!(
                reported_chart.borrow().is_some(),
                "prepared chart must be available before WAV progress"
            );
        },
    )
    .unwrap();

    let reported_chart = reported_chart.into_inner().expect("reported chart");
    assert!(Arc::ptr_eq(&reported_chart.chart, &preloaded.chart));
    assert_eq!(reported_chart.chart_length_ms, preloaded.chart_length_ms);
    assert_eq!(reported_chart.applied_arrange.pattern, preloaded.applied_arrange.pattern);
    assert_eq!(reported_chart.applied_arrange.arrange, ArrangeOption::Random);

    let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();
    crate::screens::play_snapshot::apply_prepared_chart_to_render_snapshot(
        &mut snapshot,
        &reported_chart.chart,
        &reported_chart.render_snapshot_cache,
        false,
    );
    assert_eq!(snapshot.total_notes, 1);
    assert!(!snapshot.judge_graph_density.is_empty());
    assert!(!snapshot.bpm_graph_segments.is_empty());

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(wav_path).unwrap();
}

#[test]
fn cached_chart_normalization_uses_profile_output_gain() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(conn);
    let chart = chart();
    let chart_id = library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: std::path::Path::new("/songs/cached-normalization.bms"),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();
    library_db
        .write_chart_normalization_analysis(
            chart_id,
            ChartNormalizationAnalysis {
                loudness_lufs: -20.0,
                short_term_lufs: -20.0,
                sample_peak: 4.0,
            },
        )
        .unwrap();
    let audio = AudioEngine::new(48_000);

    let full_scale =
        load_or_compute_chart_normalization_gain(&library_db, chart_id, &chart, &audio, 1.0, false)
            .unwrap();
    let profile_scale = load_or_compute_chart_normalization_gain(
        &library_db,
        chart_id,
        &chart,
        &audio,
        0.25,
        false,
    )
    .unwrap();
    let peak_ceiling = 10.0f32.powf(-1.0 / 20.0);

    assert!((4.0 * full_scale - peak_ceiling).abs() < 0.001);
    assert!((4.0 * profile_scale * 0.25 - peak_ceiling).abs() < 0.001);
    assert!(profile_scale > full_scale);
}

#[test]
fn battle_chart_normalization_matches_autoplay_and_shared_cache() {
    let (path, wav_path) = write_temp_bms_with_wav(
        "#TITLE Battle normalization\n#BPM 120\n#PLAYER 1\n#WAV01 test.wav\n#00001:01\n#00011:01\n#00151:0101\n",
    );
    let pcm = (0..48_000)
        .flat_map(|i| {
            let sample =
                ((i as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin() * 24_000.0) as i16;
            sample.to_le_bytes()
        })
        .collect::<Vec<_>>();
    std::fs::write(&wav_path, [wav_header(1, 1, 48_000, 16, pcm.len() as u32), pcm].concat())
        .unwrap();
    let imported = import_bms_chart(&path, None, true).unwrap();
    assert!(!imported.chart.long_notes.is_empty());
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    let mut baseline = None;
    for mode in [SessionMode::AutoplayBattle, SessionMode::GBattle, SessionMode::Autoplay] {
        // Reimport clears the cache so each mode exercises fresh analysis.
        let id = db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: &path,
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &imported.chart,
            })
            .unwrap();
        let loaded = preload_play_session_for_chart_with_callbacks(
            &db,
            id,
            PlaySessionOptions { session_mode: mode, ..Default::default() },
            1.0,
            |_| {},
            |_, _| {},
        )
        .unwrap();
        let analysis = db.chart_normalization_analysis_by_chart_id(id).unwrap().unwrap();
        let values = [
            analysis.loudness_lufs,
            analysis.short_term_lufs,
            analysis.sample_peak,
            loaded.chart_normalization_gain,
        ];
        assert!(loaded.chart_normalization_gain < 1.0);
        if let Some(expected) = baseline {
            assert_eq!(values, expected);
        } else {
            baseline = Some(values);
        }
        if mode.is_battle() {
            let raw = analyze_chart_loudness(&loaded.chart, &loaded.audio.samples, 48_000).unwrap();
            assert!(raw.peak_abs > analysis.sample_peak);
        }
        let cached = preload_play_session_for_chart_with_callbacks(
            &db,
            id,
            PlaySessionOptions { session_mode: SessionMode::Autoplay, ..Default::default() },
            1.0,
            |_| {},
            |_, _| {},
        )
        .unwrap();
        assert_eq!(cached.chart_normalization_gain, loaded.chart_normalization_gain);
    }
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(wav_path).unwrap();
}
