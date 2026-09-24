use super::*;

#[test]
fn cli_overrides_reach_game_session_and_auto_scratch() {
    let crate::cli::Command::Run(options) = crate::cli::parse_command([
        "chart.bms",
        "--gauge",
        "hard",
        "--gas",
        "off",
        "--hispeed",
        "3",
        "--auto-scratch",
        "on",
        "--bga",
        "off",
    ])
    .unwrap() else {
        panic!()
    };
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.set_cli_play(options.play_overrides);
    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    assert_eq!(session.gauge.selected, GaugeType::Hard);
    assert_eq!(session.hispeed, 3.0);
    assert!(!session.bga_enabled);
    assert!(session.autoplay.is_some());
    let autoplay = session.autoplay.as_ref().unwrap();
    assert!(autoplay.is_lane_enabled(Lane::Scratch));
    assert!(autoplay.is_lane_enabled(Lane::Scratch2));
    assert!(!autoplay.is_lane_enabled(Lane::Key1));
    profile.clear_cli_play();
    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    assert!(session.autoplay.is_none());
}

#[test]
fn build_game_session_uses_profile_play_settings() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.auto_play = true;
    profile.play.note_retention = true;
    profile.judge.input_offset_us = 123;
    let chart = Arc::new(chart());

    let session = build_game_session(chart, &profile, PlaySessionOptions::default());

    assert_eq!(session.state, PlayState::Ready);
    assert_eq!(session.gauge.selected, GaugeType::Normal);
    assert!(session.autoplay.is_some());
    assert_eq!(session.offsets.input_offset_us, 123);
    assert!((session.audio_mix.master_volume - 0.5).abs() < 1e-6);
    assert_eq!(session.audio_clock.sample_rate, 48_000);
    assert_eq!(session.hispeed, 2.0);
    assert_eq!(session.hidden_cover, 0.0);
    assert!(session.bga_enabled);
    assert_eq!(session.poor_bga_duration_us, 500_000);
    assert_eq!(session.bga_stretch, 1);
    assert!(session.note_retention);
}

#[test]
fn build_game_session_uses_visual_offset_auto_adjust_from_profile() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.judge.visual_offset_auto_adjust = true;
    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert!(session.input_offset_auto_adjust_enabled);
    assert!(session.input_offset_auto_adjust.is_some());
}

#[test]
fn g_battle_keeps_primary_input_offset_auto_adjust_enabled() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.judge.visual_offset_auto_adjust = true;
    let mut battle_chart = chart();
    battle_chart.metadata.key_mode = KeyMode::K7;
    let opponent_chart = Arc::new(battle_chart.clone());
    let session = build_game_session(
        Arc::new(battle_chart),
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
            opponent_chart: Some(opponent_chart),
            ..PlaySessionOptions::default()
        },
    );

    assert!(session.input_offset_auto_adjust.is_some());
    assert!(session.replay_lane_mask.is_none());
    assert!(session.battle_opponent.is_some());
    let opponent = session.battle_opponent.as_ref().unwrap();
    assert!(opponent.replay_player.is_none());
    assert!(opponent.autoplay.is_some());
    assert!(opponent.display_uses_primary_arrangement);
    assert!(opponent.publish_display_judgements);
    assert_eq!(session.primary_key_mode, KeyMode::K7);
}

#[test]
fn battle_target_uses_battle_presentation_while_session_mode_stays_normal() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut primary = chart();
    primary.metadata.key_mode = KeyMode::K14;
    primary.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
    primary.lane_notes[Lane::Key8.index()].push(note(2, Lane::Key8, 1_000_000));
    // The cloned opponent note is presentation-only, so the chart semantic
    // total remains owned by the one primary note.
    primary.total_notes = 1;
    let mut opponent = chart();
    opponent.metadata.key_mode = KeyMode::K7;
    let session = build_game_session(
        Arc::new(primary),
        &profile,
        PlaySessionOptions {
            play_config_key_mode: Some(KeyMode::K7),
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
            opponent_chart: Some(Arc::new(opponent)),
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(session.session_mode_index, 0);
    assert_eq!(session.primary_key_mode, KeyMode::K7);
    assert!(session.display_only_lane_mask[Lane::Key8.index()]);
    assert!(session.battle_opponent.is_some());
    assert_eq!(session.scored_total_notes, 1);
    assert!(!session.battle_opponent.as_ref().unwrap().publish_display_judgements);
}

#[test]
fn seven_to_nine_7k_rule_uses_7k_judgement_and_projects_7k_replay() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut converted = chart();
    converted.metadata.key_mode = KeyMode::K9;
    converted.lane_notes[Lane::Key9.index()].push(note(1, Lane::Key9, 1_000_000));
    converted.total_notes = 1;
    let replay = bmz_core::replay::ReplayEvent {
        lane: Lane::Scratch,
        kind: InputKind::Press,
        time: TimeUs(1_025_000),
        device_kind: bmz_core::input::InputDeviceKind::Keyboard,
        scratch_direction: None,
    };
    let mut session = build_game_session(
        Arc::new(converted),
        &profile,
        PlaySessionOptions {
            key_mode_conversion: KeyModeConversionConfig::SevenToNine,
            seven_to_nine_pattern: SevenToNinePattern::Sc9Key1To7,
            seven_to_nine_rule_mode: SevenToNineRuleMode::Keys7,
            replay_player: Some(ReplayPlayer::new(vec![replay])),
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(session.play_config_key_mode, KeyMode::K9);
    assert_eq!(session.primary_key_mode, KeyMode::K7);
    assert!(session.replay_lane_projection.is_some());
    let judgements = bmz_gameplay::session::process_replay_inputs(&mut session, TimeUs(1_025_000));
    assert_eq!(judgements.len(), 1);
    assert_eq!(judgements[0].lane, Lane::Key9);
    assert_eq!(judgements[0].judge, bmz_core::judge::Judge::PGreat);
}

#[test]
fn session_uses_source_mode_presentation_settings_for_battle_layout() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.normalize_play_mode_configs();
    profile.lane.hispeed = 2.75;
    profile.lane.target_green_number = 285;
    profile.judge.visual_offset_us = 6_000;
    profile.sync_active_play_mode();
    profile.activate_play_mode(KeyMode::K14);
    profile.lane.hispeed = 4.5;
    profile.lane.target_green_number = 220;
    profile.judge.visual_offset_us = -12_000;
    profile.sync_active_play_mode();

    let mut battle_chart = chart();
    battle_chart.metadata.key_mode = KeyMode::K14;
    let session = build_game_session(
        Arc::new(battle_chart),
        &profile,
        PlaySessionOptions {
            play_config_key_mode: Some(KeyMode::K7),
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(session.play_config_key_mode, KeyMode::K7);
    assert_eq!(session.hispeed, 2.75);
    assert_eq!(session.target_green_number, 285);
    assert_eq!(session.offsets.visual_offset_us, 6_000);
}

#[test]
fn autoplay_battle_keeps_genuine_double_chart_lanes_scored() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut double_chart = chart();
    double_chart.metadata.key_mode = KeyMode::K10;
    double_chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
    double_chart.lane_notes[Lane::Key8.index()].push(note(2, Lane::Key8, 1_000_000));
    double_chart.total_notes = 2;

    let session = build_game_session(
        Arc::new(double_chart),
        &profile,
        PlaySessionOptions {
            play_config_key_mode: Some(KeyMode::K10),
            session_mode: SessionMode::AutoplayBattle,
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(session.primary_key_mode, KeyMode::K10);
    assert_eq!(session.scored_total_notes, 2);
    assert!(!session.display_only_lane_mask[Lane::Key8.index()]);
    assert!(session.opponent_score.is_none());
    assert!(session.autoplay.is_some());
}

#[test]
fn autoplay_battle_marks_only_expanded_single_play_lanes_as_display_only() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut expanded_chart = chart();
    expanded_chart.metadata.key_mode = KeyMode::K10;
    expanded_chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
    expanded_chart.lane_notes[Lane::Key8.index()].push(note(2, Lane::Key8, 1_000_000));
    expanded_chart.total_notes = 2;

    let session = build_game_session(
        Arc::new(expanded_chart),
        &profile,
        PlaySessionOptions {
            play_config_key_mode: Some(KeyMode::K5),
            session_mode: SessionMode::AutoplayBattle,
            ..PlaySessionOptions::default()
        },
    );

    assert_eq!(session.primary_key_mode, KeyMode::K5);
    assert_eq!(session.scored_total_notes, 1);
    assert!(session.display_only_lane_mask[Lane::Key8.index()]);
    assert!(session.opponent_score.is_some());
}

#[test]
fn g_battle_off_autoplays_only_the_expanded_opponent_lanes() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut expanded_chart = chart();
    expanded_chart.metadata.key_mode = KeyMode::K14;
    let session = build_game_session(
        Arc::new(expanded_chart),
        &profile,
        PlaySessionOptions {
            play_config_key_mode: Some(KeyMode::K7),
            session_mode: SessionMode::GBattle,
            ..PlaySessionOptions::default()
        },
    );

    let autoplay = session.autoplay.as_ref().expect("BATTLE OFF opponent autoplay");
    assert!(!autoplay.is_lane_enabled(Lane::Scratch));
    assert!(!autoplay.is_lane_enabled(Lane::Key1));
    assert!(autoplay.is_lane_enabled(Lane::Scratch2));
    assert!(autoplay.is_lane_enabled(Lane::Key8));
    assert!(autoplay.is_lane_enabled(Lane::Key14));
}

#[test]
fn build_game_session_uses_release_bounce_settings_from_profile() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.input.keyboard_release_bounce_ms = 3;
    profile.input.controller_release_bounce_ms = 8;

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(
        session.input_system.bounce_filter.config(),
        bmz_gameplay::input::bounce::InputBounceConfig {
            keyboard_threshold_us: 3_000,
            controller_threshold_us: 8_000,
        }
    );
}

#[test]
fn build_game_session_applies_judge_algorithm_from_profile() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.judge.judge_algorithm = JudgeAlgorithmConfig::Duration;

    let duration = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    assert_eq!(duration.judge.algorithm, JudgeAlgorithm::Duration);
}

#[test]
fn placeholder_session_visuals_use_visual_offset_for_skin_judge_timing() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.judge.input_offset_us = 3_000;
    profile.judge.visual_offset_us = 4_000;
    profile.judge.visual_offset_auto_adjust = true;
    let options = PlaySessionOptions::default();
    let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

    assert_eq!(snapshot.judge_timing_offset_ms, 4);
    assert!(snapshot.judge_timing_auto_adjust);
}

#[test]
fn placeholder_session_visuals_preserve_preloaded_meta_images() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let options = PlaySessionOptions::default();
    let stagefile_size = bmz_render::skin::SkinImageSize { width: 320.0, height: 240.0 };
    let mut snapshot = bmz_render::snapshot::RenderSnapshot {
        stagefile_background: true,
        stagefile_image_size: Some(stagefile_size),
        backbmp_background: true,
        ..Default::default()
    };

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

    assert!(snapshot.stagefile_background);
    assert_eq!(snapshot.stagefile_image_size, Some(stagefile_size));
    assert!(snapshot.backbmp_background);
}

#[test]
fn placeholder_session_visuals_initialize_floating_hispeed_for_ready_display() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    // Stale value from a different BPM should not leak into READY display.
    profile.lane.hispeed = 4.0;
    let options =
        PlaySessionOptions { hs_fix: HsFixOption::StartBpm, ..PlaySessionOptions::default() };
    let mut snapshot =
        bmz_render::snapshot::RenderSnapshot { now_bpm: 240.0, ..Default::default() };

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

    assert!((snapshot.hispeed - 2.0).abs() < f32::EPSILON);
    assert_eq!(snapshot.hispeed_mode_index, 1);
    assert_eq!(snapshot.note_display_duration_ms, 500);
}

#[test]
fn placeholder_session_visuals_use_hsfix_to_select_hispeed_mode() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hispeed = 4.0;
    profile.lane.target_green_number = 300;
    let options = PlaySessionOptions::default();
    let mut snapshot =
        bmz_render::snapshot::RenderSnapshot { now_bpm: 240.0, ..Default::default() };

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

    assert_eq!(snapshot.hispeed, 4.0);
    assert_eq!(snapshot.hispeed_mode_index, 0);
}

#[test]
fn placeholder_session_visuals_apply_no_speed_constraint() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hispeed = 4.0;
    profile.lane.sudden = 400;
    profile.lane.lift = 200;
    profile.lane.hidden = 300;
    profile.play.lane_effect = LaneEffectConfig::HiddenSudden;
    let options = PlaySessionOptions {
        hs_fix: HsFixOption::MaxBpm,
        speed_constraint: bmz_core::course::CourseSpeedConstraint::NoSpeed,
        ..PlaySessionOptions::default()
    };
    let mut snapshot = bmz_render::snapshot::RenderSnapshot {
        now_bpm: 240.0,
        max_bpm: 480.0,
        ..Default::default()
    };

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

    assert_eq!(snapshot.hispeed, 1.0);
    assert_eq!(snapshot.hispeed_mode_index, 0);
    assert_eq!(snapshot.lane_cover, 0.0);
    assert_eq!(snapshot.lift, 0.0);
    assert_eq!(snapshot.hidden_cover, 0.0);
}

#[test]
fn placeholder_session_visuals_match_session_bga_modes() {
    for (mode, profile_autoplay, option_autoplay, replay, expected) in [
        (BgaModeConfig::On, false, false, false, true),
        (BgaModeConfig::Auto, false, false, false, false),
        (BgaModeConfig::Auto, false, true, false, true),
        (BgaModeConfig::Auto, false, false, true, true),
        (BgaModeConfig::Auto, true, false, false, true),
        (BgaModeConfig::Off, true, true, false, false),
    ] {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.bga = mode;
        profile.play.auto_play = profile_autoplay;
        let options = PlaySessionOptions {
            autoplay: option_autoplay,
            replay_player: replay.then(ReplayPlayer::default),
            ..PlaySessionOptions::default()
        };
        let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();

        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
        let session = build_game_session(Arc::new(chart()), &profile, options);

        assert_eq!(snapshot.bga_enabled, expected, "mode={mode:?}");
        assert_eq!(snapshot.bga_enabled, session.bga_enabled, "mode={mode:?}");
    }
}

#[test]
fn placeholder_session_visuals_expose_score_save_and_play_modes() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    for (options, save, replay, practice) in [
        (PlaySessionOptions::default(), true, false, false),
        (
            PlaySessionOptions { autoplay: true, ..PlaySessionOptions::default() },
            false,
            false,
            false,
        ),
        (
            PlaySessionOptions {
                replay_player: Some(ReplayPlayer::default()),
                ..PlaySessionOptions::default()
            },
            false,
            true,
            false,
        ),
        (
            PlaySessionOptions {
                session_mode: SessionMode::Practice,
                ..PlaySessionOptions::default()
            },
            false,
            false,
            true,
        ),
        (
            PlaySessionOptions { score_save_disabled: true, ..PlaySessionOptions::default() },
            false,
            false,
            false,
        ),
        (
            PlaySessionOptions {
                assist_runtime: bmz_gameplay::session::AssistRuntime {
                    level: bmz_gameplay::session::AssistLevel::Assist,
                    ..Default::default()
                },
                ..PlaySessionOptions::default()
            },
            false,
            false,
            false,
        ),
    ] {
        let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();
        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
        assert_eq!(snapshot.score_save_enabled, save);
        assert_eq!(snapshot.replay_playback, replay);
        assert_eq!(snapshot.practice_mode, practice);
    }

    let mut battle_snapshot = bmz_render::snapshot::RenderSnapshot::default();
    apply_placeholder_session_visuals(
        &mut battle_snapshot,
        &profile,
        KeyMode::K7,
        &PlaySessionOptions { session_mode: SessionMode::GBattle, ..PlaySessionOptions::default() },
    );
    assert!(!battle_snapshot.replay_playback);
    assert!(battle_snapshot.score_save_enabled);
}

#[test]
fn practice_session_disables_legacy_profile_autoplay() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.auto_play = true;
    profile.lane.constant_enabled = true;
    let options =
        PlaySessionOptions { session_mode: SessionMode::Practice, ..PlaySessionOptions::default() };
    let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();

    apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
    let session = build_game_session(Arc::new(chart()), &profile, options);

    assert!(snapshot.practice_mode);
    assert!(!snapshot.autoplay);
    assert!(!snapshot.score_save_enabled);
    assert!(session.autoplay.is_none());
    assert!(!session.constant_enabled);
}

#[test]
fn placeholder_session_visuals_match_session_bga_expand() {
    for (expand, expected) in
        [(BgaExpandConfig::Full, 0), (BgaExpandConfig::KeepAspect, 1), (BgaExpandConfig::Off, 8)]
    {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.bga_expand = expand;
        let options = PlaySessionOptions::default();
        let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();

        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);
        let session = build_game_session(Arc::new(chart()), &profile, options);

        assert_eq!(snapshot.bga_stretch, expected, "expand={expand:?}");
        assert_eq!(snapshot.bga_stretch, session.bga_stretch, "expand={expand:?}");
    }
}

#[test]
fn build_game_session_picks_gauge_property_from_chart_keymode() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut chart_k5 = chart();
    chart_k5.metadata.key_mode = KeyMode::K5;
    let mut chart_k7 = chart();
    chart_k7.metadata.key_mode = KeyMode::K7;

    let session_k5 =
        build_game_session(Arc::new(chart_k5), &profile, PlaySessionOptions::default());
    let session_k7 =
        build_game_session(Arc::new(chart_k7), &profile, PlaySessionOptions::default());

    // FIVEKEYS CLASS: PG/GR=0.01, BAD=-0.5。SEVENKEYS CLASS: PG=0.15, BAD=-1.5。
    assert_eq!(class_gauge_values(&session_k5)[0], 0.01);
    assert_eq!(class_gauge_values(&session_k5)[3], -0.5);
    assert_eq!(class_gauge_values(&session_k7)[0], 0.15);
    assert_eq!(class_gauge_values(&session_k7)[3], -1.5);
}

#[test]
fn build_game_session_uses_gauge_property_override() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    // チャートは K7 だが、option で LR2 を強制する。
    let options =
        PlaySessionOptions { gauge_property: Some(GaugeProperty::Lr2), ..Default::default() };
    let session = build_game_session(Arc::new(chart()), &profile, options);

    // LR2 CLASS: BAD=-2.0、PG=0.10。
    assert_eq!(class_gauge_values(&session)[3], -2.0);
    assert_eq!(class_gauge_values(&session)[0], 0.10);
}

#[test]
fn build_game_session_applies_lr2oraja_rule_mode() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Lr2Oraja;

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(session.rule_mode, RuleMode::Lr2Oraja);
    assert_eq!(session.base_judge_window.pgreat_us, 21_000);
    assert_eq!(session.base_judge_window.empty_poor_slow_us, 0);
    let hard = session
        .gauge
        .gauges
        .iter()
        .find(|g| g.definition.gauge_type == GaugeType::Hard)
        .expect("Hard gauge present");
    assert_eq!(hard.definition.guts, &[(32.0, 0.6)]);
    assert_eq!(hard.definition.death, 2.0);
}

#[test]
fn build_game_session_applies_dx_rule_mode() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Dx;

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(session.rule_mode, RuleMode::Dx);
    assert_eq!(session.base_judge_window.pgreat_us, 16_666);
    assert_eq!(session.judge.windows.pgreat_us, 16_666);
    let hard = session
        .gauge
        .gauges
        .iter()
        .find(|g| g.definition.gauge_type == GaugeType::Hard)
        .expect("Hard gauge present");
    assert_eq!(hard.definition.values, [0.16, 0.16, 0.0, -4.5, -9.0, -4.5]);
}
