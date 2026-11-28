use super::*;
use bmz_gameplay::judge::window::judge_windows_for_keymode_and_rule_mode;

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
    profile.play.lane_effect = LaneEffectConfig::Sudden;
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
        if double_speed.lanecover_enabled { double_speed.lane_cover } else { 0.0 },
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
fn build_freq_replay_keeps_recorded_windows_after_viewing_speed_changes() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    for rate in [25, 100, 200, 300] {
        let mut source = chart();
        source.metadata.key_mode = KeyMode::K7;
        source.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
        source.total_notes = 1;
        let mut session = build_game_session(
            Arc::new(source),
            &profile,
            PlaySessionOptions {
                playback_rate_percent: 200,
                freq_semitones: 12,
                replay_player: Some(ReplayPlayer {
                    events: vec![bmz_core::replay::ReplayEvent {
                        applied_after: None,
                        lane: Lane::Key1,
                        kind: InputKind::Press,
                        time: TimeUs(1_030_000),
                        device_kind: Default::default(),
                        scratch_direction: None,
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert_eq!(session.replay_judge_rate_percent, 200);
        session.audio_clock.set_playback_rate_percent(rate);
        bmz_gameplay::session::sync_judge_windows(&mut session, TimeUs(1_030_000));
        let events = bmz_gameplay::session::process_replay_inputs(&mut session, TimeUs(1_030_000));
        assert_eq!(events[0].judge, bmz_core::judge::Judge::PGreat);
        assert_eq!(session.score.ex_score(), 2);
    }
}

#[test]
fn dx_pop_replays_use_the_recorded_judge_revision() {
    use crate::storage::replay::{REPLAY_JUDGE_REVISION, ReplayFile};

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Dx;
    let build = |key_mode, revision| {
        let mut source = chart();
        source.metadata.key_mode = key_mode;
        let mut replay =
            ReplayFile::new([0; 32], 1, None, ArrangeOption::Normal, None, None, vec![]);
        replay.judge_revision = revision;
        build_game_session(
            Arc::new(source),
            &profile,
            PlaySessionOptions { replay_player: Some(replay.player()), ..Default::default() },
        )
        .base_judge_windows
    };
    let iidx = build(KeyMode::K7, REPLAY_JUDGE_REVISION);
    for (revision, note_good, end_good, early_good, margin) in [
        (0, 87_500, 217_000, None, 200_000),
        (1, 116_667, 116_667, Some(233_333), 116_667),
        (4, 116_667, 116_667, Some(233_333), 116_667),
        (REPLAY_JUDGE_REVISION, 116_667, 116_667, Some(281_667), 165_000),
    ] {
        let pop = build(KeyMode::K9, revision);
        assert_eq!(pop.note.good_us, note_good);
        assert_eq!(pop.long_note_end.good_us, end_good);
        assert_eq!(pop.long_note_end_fast.map(|window| window.good_us), early_good);
        assert_eq!(pop.long_note_release_margin_us, margin);
        assert_eq!(build(KeyMode::K7, revision), iidx);
    }
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
fn practice_displays_source_hidden_notes_without_revealing_masked_notes() {
    use crate::screens::play_snapshot::{
        PlayfieldProjection, ProjectionClock,
        build_render_snapshot_with_target_and_bga_frames_cached,
    };
    use bmz_render::snapshot::NoteVisualKind;

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.show_hidden_notes = true;
    let mut source = chart();
    let mut hidden = note(1, Lane::Key1, 1_750_000);
    hidden.kind = NoteKind::Invisible;
    source.lane_notes[Lane::Key1.index()].push(hidden);
    source.lane_notes[Lane::Key2.index()].push(note(2, Lane::Key2, 1_750_000));
    source.lane_notes[Lane::Key3.index()].push(note(3, Lane::Key3, 2_200_000));
    let preloaded = preloaded_play_session(source);
    let source_session =
        build_game_session(Arc::clone(&preloaded.chart), &profile, PlaySessionOptions::default());
    let mut projections =
        PlayfieldProjection::pool(&source_session, &preloaded.render_snapshot_cache);
    let prepared = build_practice_prepared_from_preloaded(
        preloaded,
        &profile,
        &PracticeProperty { start_time_ms: 2_000, end_time_ms: 3_000, ..Default::default() },
        PlaySessionOptions::default(),
        Box::new(NullInputBackend),
    );

    let now = TimeUs(1_500_000);
    let snapshot = build_render_snapshot_with_target_and_bga_frames_cached(
        &prepared.session,
        now,
        &[],
        None,
        None,
        None,
        &Default::default(),
        &prepared.render_snapshot_cache,
    );
    assert_eq!(prepared.session.chart.notes_for_lane(Lane::Key2)[0].kind, NoteKind::Invisible);
    assert_eq!(snapshot.visible_notes[Lane::Key1.index()][0].kind, NoteVisualKind::Invisible);
    assert!(snapshot.visible_notes[Lane::Key2.index()].is_empty());
    assert_eq!(snapshot.visible_notes[Lane::Key3.index()][0].kind, NoteVisualKind::Tap);

    // Updating a slot that still holds the source chart rebuilds its cache and
    // copies the hidden-note IDs from the prepared practice session.
    let projection = Arc::get_mut(&mut projections[0]).unwrap();
    projection.update(&prepared.session, now);
    let mut projected = snapshot.clone();
    projection.project(&mut projected, now, &mut ProjectionClock::default());
    assert_eq!(projected.visible_notes, snapshot.visible_notes);
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
        &PracticeProperty {
            gauge_category: Some(GaugeProperty::Pms),
            gauge: PracticeGaugeType::Normal,
            start_gauge: 30,
            ..Default::default()
        },
        options,
        Box::new(NullInputBackend),
    );

    assert_eq!(prepared.session.hsfix_index, 2);
    assert_eq!(prepared.session.hsfix_base_bpm, 240.0);
    assert_eq!(prepared.session.hispeed_mode, HispeedMode::Floating);
    assert_eq!(prepared.session.rule_mode, RuleMode::Lr2Oraja);
    assert_eq!(prepared.session.gauge.current().value, 30.0);
    assert_eq!(prepared.session.gauge.current().definition.max, 120.0);
    assert_eq!(prepared.skin_attempt.hsfix_index, Some(2));
}

#[test]
fn practice_autosave_preserves_rule_dependent_gauges_across_rule_changes() {
    use crate::config::profile_config::GaugeTypeConfig;
    use crate::screens::practice::{
        PracticeRuleContext, load_practice_property, save_practice_property,
    };
    use RuleMode::{Beatoraja, Dx, Lr2Oraja};

    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
    let mut source = chart();
    let hash = &source.identity.file_sha256;
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    for modes in [
        [Lr2Oraja, Beatoraja, Dx, Lr2Oraja],
        [Beatoraja, Lr2Oraja, Dx, Beatoraja],
        [Dx, Lr2Oraja, Beatoraja, Dx],
    ] {
        for (version, category, key_mode, capped_rule) in [
            (0, None, KeyMode::K9, Lr2Oraja),
            (1, Some(GaugeProperty::Pms), KeyMode::K9, Lr2Oraja),
            (1, None, KeyMode::K9, Lr2Oraja),
            (2, None, KeyMode::K9, Lr2Oraja),
            (2, Some(GaugeProperty::Pms), KeyMode::K7, Dx),
        ] {
            source.metadata.key_mode = key_mode;
            let saved = PracticeProperty {
                format_version: version,
                gauge_category: category,
                start_gauge: 117,
                ..Default::default()
            };
            save_practice_property(&paths, hash, &saved).unwrap();
            for mode in modes {
                profile.play.rule_mode = mode;
                let rules = PracticeRuleContext { rule_mode: mode, key_mode };
                let loaded = load_practice_property(
                    &paths,
                    hash,
                    &source,
                    GaugeTypeConfig::Normal,
                    rules,
                    &Default::default(),
                )
                .unwrap();
                let prepared = build_practice_prepared_from_preloaded(
                    preloaded_play_session(source.clone()),
                    &profile,
                    &loaded,
                    PlaySessionOptions::default(),
                    Box::new(NullInputBackend),
                );
                let gauge = prepared.session.gauge.current();
                let expected = if mode == capped_rule { (100.0, 100.0) } else { (117.0, 120.0) };
                assert_eq!((gauge.value, gauge.definition.max), expected, "{modes:?} {mode:?}");
                assert_eq!(loaded.gauge_category, category);
                assert_eq!(loaded.start_gauge, 117);
                assert_eq!(loaded.format_version, version.max(1));
                save_practice_property(&paths, hash, &loaded).unwrap();
            }
        }
    }
}

#[test]
fn practice_dx_preloaded_gauges_match_effective_rules_and_keep_fixed_scoring() {
    use crate::screens::practice::{PracticeRuleContext, clamp_practice_property};

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Dx;
    for (key_mode, conversion, conversion_rule, expected_mode, initial, max) in [
        (
            KeyMode::K7,
            KeyModeConversionConfig::Off,
            SevenToNineRuleMode::Keys7,
            KeyMode::K7,
            22,
            100,
        ),
        (
            KeyMode::K9,
            KeyModeConversionConfig::Off,
            SevenToNineRuleMode::Keys9,
            KeyMode::K9,
            30,
            120,
        ),
        (
            KeyMode::K9,
            KeyModeConversionConfig::SevenToNine,
            SevenToNineRuleMode::Keys7,
            KeyMode::K7,
            22,
            100,
        ),
        (
            KeyMode::K9,
            KeyModeConversionConfig::SevenToNine,
            SevenToNineRuleMode::Keys9,
            KeyMode::K9,
            30,
            120,
        ),
    ] {
        let mut source = chart();
        source.metadata.key_mode = key_mode;
        source.end_time = TimeUs(2_000_000);
        source.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
        let options = PlaySessionOptions {
            session_mode: SessionMode::Practice,
            key_mode_conversion: conversion,
            seven_to_nine_rule_mode: conversion_rule,
            rule_mode: RuleMode::Dx,
            ..Default::default()
        };
        let rules =
            PracticeRuleContext::for_play_session(profile.play.rule_mode, key_mode, &options);
        let mut property = PracticeProperty {
            start_gauge: 120,
            gauge_category: Some(GaugeProperty::FiveKeys),
            judgerank: 1,
            total: Some(10.0),
            ..Default::default()
        };
        assert_eq!(rules.key_mode, expected_mode);
        assert_eq!(rules.gauge_bounds(&property).initial, initial);
        assert_eq!(rules.gauge_bounds(&property).max, max);
        clamp_practice_property(&mut property, &source, rules);
        assert_eq!(property.start_gauge, 120);
        assert_eq!(rules.gauge_bounds(&property).start(&property), max);
        let prepared = build_practice_prepared_from_preloaded(
            preloaded_play_session(source.clone()),
            &profile,
            &property,
            options.clone(),
            Box::new(NullInputBackend),
        );
        assert_eq!(prepared.session.primary_key_mode, rules.key_mode);
        assert_eq!(prepared.session.gauge.current().value, max as f32);
        assert_eq!(prepared.session.gauge.current().definition.max, max as f32);
        assert_eq!(
            prepared.session.base_judge_windows,
            judge_windows_for_keymode_and_rule_mode(expected_mode, RuleMode::Dx)
        );

        property.judgerank = 400;
        property.total = Some(5000.0);
        property.gauge_category = Some(GaugeProperty::Pms);
        let changed_ignored_settings = build_practice_prepared_from_preloaded(
            preloaded_play_session(source),
            &profile,
            &property,
            options,
            Box::new(NullInputBackend),
        );
        assert_eq!(
            prepared.session.gauge.current().definition.values,
            changed_ignored_settings.session.gauge.current().definition.values
        );
        assert_eq!(
            prepared.session.base_judge_windows,
            changed_ignored_settings.session.base_judge_windows
        );
    }
}

#[test]
fn practice_dx_start_keeps_pop_gauge_above_one_hundred_during_gas() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Dx;
    for mode in [GaugeAutoShiftMode::Off, GaugeAutoShiftMode::BestClear] {
        for gauge in PracticeGaugeType::VALUES {
            let mut source = chart();
            source.metadata.key_mode = KeyMode::K9;
            let property = PracticeProperty { gauge, start_gauge: 120, ..Default::default() };
            let prepared = build_practice_prepared_from_preloaded(
                preloaded_play_session(source),
                &profile,
                &property,
                PlaySessionOptions {
                    gauge_override: Some(GaugeType::Normal),
                    gauge_auto_shift: mode,
                    ..Default::default()
                },
                Box::new(NullInputBackend),
            );
            for member in &prepared.session.gauge.gauges {
                let expected = if matches!(
                    member.definition.gauge_type,
                    GaugeType::AssistEasy
                        | GaugeType::Easy
                        | GaugeType::Normal
                        | GaugeType::Hard
                        | GaugeType::ExHard
                ) {
                    120.0
                } else {
                    100.0
                };
                assert_eq!(
                    member.value, expected,
                    "{mode:?} {gauge:?} {:?}",
                    member.definition.gauge_type
                );
            }
            if mode == GaugeAutoShiftMode::BestClear
                && !matches!(
                    gauge,
                    PracticeGaugeType::Class
                        | PracticeGaugeType::ExClass
                        | PracticeGaugeType::ExHardClass
                )
            {
                assert_eq!(prepared.session.gauge.selected, GaugeType::Hazard);
                assert_eq!(prepared.session.gauge.current().value, 100.0);
            }
        }
    }
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

    assert_eq!(
        hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MainBpm, false),
        180.0
    );
}

#[test]
fn main_bpm_counts_only_taps_and_long_starts_with_positive_scroll() {
    let mut bpm_chart = chart();
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(1_000),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 180.0 },
    });
    bpm_chart.scroll_events = vec![
        bmz_chart::model::ScrollEvent {
            tick: bmz_core::time::ChartTick(0),
            time: TimeUs(0),
            factor: 2.0,
        },
        bmz_chart::model::ScrollEvent {
            tick: bmz_core::time::ChartTick(100),
            time: TimeUs(100_000),
            factor: 1.5,
        },
        bmz_chart::model::ScrollEvent {
            tick: bmz_core::time::ChartTick(1_000),
            time: TimeUs(1_000_000),
            factor: 3.0,
        },
        bmz_chart::model::ScrollEvent {
            tick: bmz_core::time::ChartTick(2_000),
            time: TimeUs(2_000_000),
            factor: -2.0,
        },
        bmz_chart::model::ScrollEvent {
            tick: bmz_core::time::ChartTick(3_000),
            time: TimeUs(3_000_000),
            factor: 0.0,
        },
    ];
    bpm_chart.speed_events = vec![
        bmz_chart::model::SpeedEvent {
            tick: bmz_core::time::ChartTick(0),
            time: TimeUs(0),
            factor: 1.0,
        },
        bmz_chart::model::SpeedEvent {
            tick: bmz_core::time::ChartTick(1_000),
            time: TimeUs(1_000_000),
            factor: 2.0,
        },
        bmz_chart::model::SpeedEvent {
            tick: bmz_core::time::ChartTick(2_000),
            time: TimeUs(2_000_000),
            factor: 1.0,
        },
        bmz_chart::model::SpeedEvent {
            tick: bmz_core::time::ChartTick(3_000),
            time: TimeUs(3_000_000),
            factor: 1.0,
        },
    ];

    let mut long_start = note(1, Lane::Key1, 0);
    long_start.kind = NoteKind::LongStart;
    bpm_chart.lane_notes[Lane::Key1.index()].push(long_start);
    bpm_chart.lane_notes[Lane::Key2.index()].push(note(2, Lane::Key2, 0));
    bpm_chart.lane_notes[Lane::Key7.index()].push(note(3, Lane::Key7, 100_000));
    for (id, lane) in [(10, Lane::Key3), (11, Lane::Key4), (12, Lane::Key5), (13, Lane::Key6)] {
        let mut long_end = note(id, lane, 1_000_000);
        long_end.kind = NoteKind::LongEnd;
        bpm_chart.lane_notes[lane.index()].push(long_end);
    }
    for id in 20..25 {
        bpm_chart.lane_notes[Lane::Key3.index()].push(note(id, Lane::Key3, 2_000_000));
    }
    for id in 30..36 {
        bpm_chart.lane_notes[Lane::Key4.index()].push(note(id, Lane::Key4, 3_000_000));
    }

    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        bpm_chart.metadata.initial_bpm,
        &bpm_chart.timing_events,
    );

    assert_eq!(
        hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MainBpm, false),
        240.0
    );
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

    assert_eq!(
        hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MainBpm, false),
        180.0
    );
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

    assert_eq!(hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MinBpm, false), 0.96);
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
            replay_judge_rate_percent: 100,
            gauge: None,
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            arrange_seed: None,
            arrange_seed_2p: None,
            legacy_arrange_seed: false,
            packed_seed: None,
            lr2_seed: None,
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
            replay_judge_rate_percent: 100,
            gauge: None,
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            arrange_seed: None,
            arrange_seed_2p: None,
            legacy_arrange_seed: false,
            packed_seed: None,
            lr2_seed: None,
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
fn g_battle_seed_target_from_a_d_battle_row_places_the_opponent_with_its_p1_seed() {
    use crate::screens::play_start::{
        BattleTarget, BattleTargetPlayback, PlayStartOptions, SeedScheme,
        play_session_options_from_start,
    };

    let path = write_temp_bms(
        "\
#TITLE D-Battle Seed
#BPM 120
#00111:01
#00212:01
#00313:01
#00414:01
#00515:01
#00618:01
#00719:01
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
    let key_times = |chart: &PlayableChart| {
        Lane::ALL[Lane::Key1.index()..=Lane::Key7.index()]
            .iter()
            .map(|lane| {
                chart.lane_notes[lane.index()].iter().map(|note| note.time).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let p1_seed = 0x123456;
    let rival = preload_play_session_for_chart(
        &library_db,
        chart_id,
        PlaySessionOptions {
            arrange: ArrangeOption::Random,
            arrange_seed: Some(p1_seed),
            ..PlaySessionOptions::default()
        },
        1.0,
    )
    .unwrap();
    assert_ne!(key_times(&rival.chart), key_times(&imported.chart));

    // The row's key_mode is 7K, but D-BATTLE packed both sides as P1 + P2 * 2^24.
    let target = BattleTarget {
        provider: "bms-ir".into(),
        score_id: "score".into(),
        player_id: "player".into(),
        player_name: "RIVAL".into(),
        rank: 1,
        ex_score: 100,
        gauge: None,
        playback: BattleTargetPlayback::Seed {
            arrange: ArrangeOption::Random,
            arrange_2p: ArrangeOption::Random,
            double_option: DoubleOption::Battle,
            packed_seed: Some(p1_seed | (0x654321 << 24)),
            seed_scheme: SeedScheme::Beatoraja24,
        },
    };
    let options = play_session_options_from_start(
        &Default::default(),
        PlayStartOptions {
            session_mode: SessionMode::GBattle,
            battle_target: Some(target),
            ..Default::default()
        },
    );
    let preloaded = preload_play_session_for_chart(&library_db, chart_id, options, 1.0).unwrap();

    let opponent = preloaded.opponent_chart.as_ref().expect("opponent chart");
    assert_eq!(opponent.metadata.key_mode, KeyMode::K7);
    assert_eq!(key_times(opponent), key_times(&rival.chart));

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

fn register_temp_bms(body: &str) -> (std::path::PathBuf, LibraryDatabase, i64) {
    let path = write_temp_bms(body);
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
    (path, library_db, chart_id)
}

#[test]
fn load_transformed_chart_marks_auto_scratch_and_removes_it_from_scoring() {
    let auto_scratch = |double_option| PlaySessionOptions {
        assist: AssistOptionConfig { auto_scratch: true, ..Default::default() },
        double_option,
        ..Default::default()
    };

    let (path, library_db, chart_id) =
        register_temp_bms("#TITLE Scratch\n#BPM 120\n#00011:01\n#00016:01\n");
    let transformed =
        load_transformed_chart_for_play(&library_db, chart_id, &auto_scratch(DoubleOption::Off))
            .unwrap();
    // LR2: no score, the clear counts as an assist clear.
    assert_eq!(transformed.assist_runtime.level, bmz_gameplay::session::AssistLevel::Assist);
    assert!(!transformed.assist_runtime.score_update_enabled());
    // beatoraja / Endless Dream move the auto scratch to the background.
    let scratch_kinds = |chart: &PlayableChart, lane: Lane| {
        chart.notes_for_lane(lane).iter().map(|note| note.kind).collect::<Vec<_>>()
    };
    assert_eq!(scratch_kinds(&transformed.chart, Lane::Scratch), [NoteKind::DummyTap]);
    assert_eq!(scored_note_count(&transformed.chart), 1);
    assert!(!transformed.chart.metadata.kaleid.effective.dummy);
    // A replay follows its key log, so a live AUTO SCRATCH setting does not apply.
    let replay = load_transformed_chart_for_play(
        &library_db,
        chart_id,
        &PlaySessionOptions {
            replay_player: Some(ReplayPlayer::default()),
            ..auto_scratch(DoubleOption::Off)
        },
    )
    .unwrap();
    assert_eq!(scratch_kinds(&replay.chart, Lane::Scratch), [NoteKind::Tap]);
    assert_eq!(scored_note_count(&replay.chart), 2);
    let battle_replay = load_transformed_chart_for_play(
        &library_db,
        chart_id,
        &PlaySessionOptions {
            replay_player: Some(ReplayPlayer::default()),
            ..auto_scratch(DoubleOption::Battle)
        },
    )
    .unwrap();
    assert_eq!(battle_replay.applied_arrange.double_option, DoubleOption::Battle);
    assert_eq!(scored_note_count(&battle_replay.chart), 4);

    // D-BATTLE with AUTO SCRATCH is BATTLE AS: its own bucket, not an assist.
    let battle =
        load_transformed_chart_for_play(&library_db, chart_id, &auto_scratch(DoubleOption::Battle))
            .unwrap();
    assert_eq!(battle.applied_arrange.double_option, DoubleOption::BattleAutoScratch);
    assert_eq!(
        battle.score_key.double_option,
        crate::select_options::DoubleOptionScoreBucket::BattleAutoScratch
    );
    assert_eq!(battle.assist_runtime.level, bmz_gameplay::session::AssistLevel::None);
    for lane in [Lane::Scratch, Lane::Scratch2] {
        assert_eq!(scratch_kinds(&battle.chart, lane), [NoteKind::DummyTap]);
    }
    assert_eq!(scored_note_count(&battle.chart), 2);
    assert_eq!(battle.chart.metadata.source_note_counts.unwrap().total, 2);
    std::fs::remove_file(path).unwrap();

    let (path, library_db, chart_id) =
        register_temp_bms("#TITLE No Scratch\n#BPM 120\n#00011:01\n");
    let no_scratch =
        load_transformed_chart_for_play(&library_db, chart_id, &auto_scratch(DoubleOption::Off))
            .unwrap();
    assert_eq!(no_scratch.assist_runtime.level, bmz_gameplay::session::AssistLevel::None);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn battle_as_preload_sends_source_counts_after_forced_ln_policy() {
    use crate::ir::payload::{IrSubmissionContext, build_score_submission};
    use crate::screens::play_finish::play_result_from_session;
    use std::fmt::Write as _;

    let profile = ProfileConfig::new_default("default", "Default", 1);
    let double = DoubleOption::BattleAutoScratch;
    // Enough pairs to exercise the default TOTAL above its 260-point floor.
    let pairs = 200;
    for (source_lnmode, setting, cn_count) in
        [(1, LnPolicySetting::ForceCn, pairs), (2, LnPolicySetting::ForceLn, 0)]
    {
        let mut body = format!(
            "#TITLE Battle AS LN Policy\n#BPM 120\n#LNMODE {source_lnmode}\n#LNOBJ ZZ\n#00016:01\n"
        );
        for measure in 0..pairs {
            writeln!(body, "#{measure:03}11:01ZZ").unwrap();
        }
        let (path, library_db, chart_id) = register_temp_bms(&body);
        let prepared = load_prepared_play_session_for_chart(
            &library_db,
            chart_id,
            &profile,
            PlaySessionOptions {
                double_option: double,
                ln_policy_setting: setting,
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::remove_file(path).unwrap();

        let payload = build_score_submission(
            &prepared.session.chart,
            &play_result_from_session(&prepared.session),
            IrSubmissionContext {
                played_at: 1,
                play_duration_ms: None,
                chart_length_ms: Some(prepared.chart_length_ms),
                ln_policy: prepared.score_key.ln_policy,
                source_ln_profile: prepared.source_ln_profile,
                gauge_option: "normal".into(),
                device_type: bmz_core::input::InputDeviceKind::Keyboard,
                idempotency_key: "battle-as-ln-policy".into(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                double_option: double.score_bucket(),
                applied_double_option: prepared.applied_arrange.double_option,
                arrange_seed: None,
                random_seed: None,
                seed_scheme: String::new(),
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                rule_mode: RuleMode::Beatoraja.as_str().into(),
                course_stage: false,
                replay_hash: None,
            },
        );
        let source_notes = pairs + cn_count + 1;
        assert_eq!(payload.chart.notes.total, source_notes, "{setting:?}");
        assert_eq!(payload.chart.notes.ln, pairs);
        assert_eq!(payload.chart.notes.cn, cn_count);
        assert_eq!(payload.chart.ln_profile.has_defined_cn, source_lnmode == 2);
        assert_eq!(
            payload.chart.total,
            Some(bmz_gameplay::gauge::default_gauge_total(source_notes))
        );
        assert_eq!(payload.result.notes, 2 * (pairs + cn_count));
    }
}

#[test]
fn cli_auto_scratch_keeps_branch_bgm_and_leaves_new_scratches_unscored() {
    let (path, library_db, chart_id) = register_temp_bms(concat!(
        "#TITLE Conditional Scratch\n#BPM 120\n",
        "#WAV01 scratch.wav\n#WAV02 branch.wav\n#WAV03 common.wav\n#WAV04 later-scratch.wav\n",
        "#00016:01\n#00201:03\n#00311:0101\n",
        "#CONDITIONAL 1\n#WHEN AUTO==0\n#00101:02\n#00216:04\n#DEFAULT\n#ENDCONDITIONAL\n",
    ));
    let crate::cli::Command::Run(options) =
        crate::cli::parse_command(["chart.bms", "--auto-scratch", "on"]).unwrap()
    else {
        panic!("expected play command")
    };
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.set_cli_play(options.play_overrides);
    assert!(!profile.play.assist.auto_scratch);
    let mut prepared = load_prepared_play_session_for_chart(
        &library_db,
        chart_id,
        &profile,
        PlaySessionOptions::default(),
    )
    .unwrap();
    assert!(prepared.score_save_disabled);

    let mut direct =
        load_game_session_for_chart(&library_db, chart_id, &profile, PlaySessionOptions::default())
            .unwrap();
    std::fs::remove_file(path).unwrap();
    for session in [&mut prepared.session, &mut direct] {
        let scratch_time = session.chart.notes_for_lane(Lane::Scratch)[0].time.0;
        let old_common_key = *session.chart.metadata.conditional_bgm_keys.last().unwrap();
        assert_eq!(session.chart.bgm_events.len(), 2);
        assert_eq!(session.chart.metadata.conditional_bgm_keys.len(), 2);
        let mut audio = bmz_audio::queue::ScheduledSoundQueue::new();
        session.audio_clock.start_output_frame = 0;
        session.audio_clock.running = true;
        for time in [scratch_time, scratch_time + 2_010_000, scratch_time + 4_010_000] {
            session
                .audio_clock
                .current_frame
                .store((time * 48 / 1000) as u64, std::sync::atomic::Ordering::Relaxed);
            bmz_gameplay::session::advance_session_frame(session, &mut audio);
        }
        assert_eq!(session.conditional.decisions[0].branch, 0);
        assert_eq!(session.chart.bgm_events.len(), 4);
        assert_eq!(session.chart.metadata.conditional_bgm_keys.len(), 4);
        let heard: Vec<_> = audio
            .drain_all()
            .map(|event| {
                session
                    .chart
                    .sounds
                    .iter()
                    .find(|asset| asset.id == event.sound_id)
                    .unwrap()
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        for sound in ["scratch.wav", "branch.wav", "common.wav", "later-scratch.wav"] {
            assert_eq!(heard.iter().filter(|name| *name == sound).count(), 1, "{heard:?}");
        }
        assert!(session.chart.metadata.conditional_bgm_keys.contains(&old_common_key));
        assert!(session.chart.notes_for_lane(Lane::Scratch).iter().any(|note| {
            note.time.0 == scratch_time + 4_000_000 && note.kind == NoteKind::DummyTap
        }));
        assert_eq!(session.scored_total_notes, 2);
        assert_eq!((session.score.ex_score(), session.score.combo, session.score.bp()), (0, 0, 0));
    }
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
        reported_chart.score_key.rule_mode,
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

    let full_scale = load_or_compute_chart_normalization_gain(
        &library_db,
        chart_id,
        &chart,
        &audio,
        1.0,
        false,
        false,
    )
    .unwrap();
    let profile_scale = load_or_compute_chart_normalization_gain(
        &library_db,
        chart_id,
        &chart,
        &audio,
        0.25,
        false,
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

fn direct_hispeed_profile(base: BaseHispeedConfig, hispeed: f32) -> ProfileConfig {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = base;
    profile.lane.floating_policy = FloatingPolicyConfig::Disabled;
    profile.lane.hispeed = hispeed;
    profile
}

fn bpm_120_to_300_chart() -> PlayableChart {
    let mut bpm_chart = chart();
    bpm_chart.metadata.initial_bpm = 120.0;
    bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: TimingEventKind::BpmChange { bpm: 300.0 },
    });
    bpm_chart
}

#[test]
fn build_game_session_treats_lr2_hsfix_hispeed_as_bpm150_value() {
    let profile = direct_hispeed_profile(BaseHispeedConfig::Lr2, 3.0);
    let build = |hs_fix| {
        build_game_session(
            Arc::new(bpm_120_to_300_chart()),
            &profile,
            PlaySessionOptions { hs_fix, ..PlaySessionOptions::default() },
        )
    };

    let off = build(HsFixOption::Off);
    let start = build(HsFixOption::StartBpm);
    let max = build(HsFixOption::MaxBpm);

    assert_eq!(off.hispeed_mode, HispeedMode::Classic);
    assert_eq!(off.hispeed, 3.0);
    assert!((start.hispeed - 3.75).abs() < 0.000_01, "hispeed={}", start.hispeed);
    assert_eq!(max.hispeed_mode, HispeedMode::Classic);
    assert!(max.lr2_hispeed);
    assert!((max.hispeed - 1.5).abs() < 0.000_01, "hispeed={}", max.hispeed);
    for session in [&off, &start, &max] {
        assert_eq!(crate::screens::play_snapshot::display_hispeed_for_session(session), 3.0);
    }
    // MAX BPMでの表示時間は、BPM150・HS 3.00と同じになる。
    let max_bpm_duration = crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
        300.0,
        max.hispeed,
        0.0,
        0.0,
        1.0,
    );
    let reference_duration = crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
        150.0, 3.0, 0.0, 0.0, 1.0,
    );
    assert!((max_bpm_duration - reference_duration).abs() < 0.01);
}

#[test]
fn build_game_session_compensates_lr2_hsfix_hispeed_for_practice_rate() {
    let profile = direct_hispeed_profile(BaseHispeedConfig::Lr2, 3.0);

    let session = build_game_session(
        Arc::new(bpm_120_to_300_chart()),
        &profile,
        PlaySessionOptions {
            session_mode: SessionMode::Practice,
            hs_fix: HsFixOption::MaxBpm,
            playback_rate_percent: 200,
            ..PlaySessionOptions::default()
        },
    );

    assert!((session.hispeed - 0.75).abs() < 0.000_01, "hispeed={}", session.hispeed);
    assert_eq!(crate::screens::play_snapshot::display_hispeed_for_session(&session), 3.0);
}

#[test]
fn placeholder_session_visuals_scale_lr2_hsfix_hispeed() {
    let profile = direct_hispeed_profile(BaseHispeedConfig::Lr2, 3.0);
    let options =
        PlaySessionOptions { hs_fix: HsFixOption::MaxBpm, ..PlaySessionOptions::default() };
    let mut snapshot = bmz_render::snapshot::RenderSnapshot {
        now_bpm: 120.0,
        max_bpm: 300.0,
        ..Default::default()
    };

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

    assert_eq!(snapshot.hispeed_mode_index, 0);
    assert!((snapshot.hispeed - 1.5).abs() < 0.000_01, "hispeed={}", snapshot.hispeed);
    assert_eq!(snapshot.hispeed_display, Some(3.0));
    assert_eq!(snapshot.hispeed_config_index, 5);
}

#[test]
fn build_game_session_keeps_classic_hispeed_unscaled_with_hsfix() {
    let profile = direct_hispeed_profile(BaseHispeedConfig::Classic, 3.0);

    for hs_fix in [HsFixOption::Off, HsFixOption::StartBpm, HsFixOption::MaxBpm] {
        let session = build_game_session(
            Arc::new(bpm_120_to_300_chart()),
            &profile,
            PlaySessionOptions { hs_fix, ..PlaySessionOptions::default() },
        );
        assert_eq!(session.hispeed_mode, HispeedMode::Classic);
        assert!(!session.lr2_hispeed);
        assert_eq!(session.hispeed, 3.0, "hs_fix={hs_fix:?}");
        assert_eq!(crate::screens::play_snapshot::display_hispeed_for_session(&session), 3.0);
    }

    let options =
        PlaySessionOptions { hs_fix: HsFixOption::MaxBpm, ..PlaySessionOptions::default() };
    let mut snapshot = bmz_render::snapshot::RenderSnapshot {
        now_bpm: 120.0,
        max_bpm: 300.0,
        ..Default::default()
    };
    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
    assert_eq!(snapshot.hispeed, 3.0);
    assert_eq!(snapshot.hispeed_config_index, 1);
}
