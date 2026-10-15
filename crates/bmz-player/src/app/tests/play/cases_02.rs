use super::*;

#[test]
fn decide_cancel_chord_accepts_e1_e2_and_e2_e3() {
    assert!(decide_cancel_chord_pressed(true, true, false));
    assert!(decide_cancel_chord_pressed(false, true, true));
    assert!(decide_cancel_chord_pressed(true, true, true));
    assert!(!decide_cancel_chord_pressed(true, false, true));
    assert!(!decide_cancel_chord_pressed(false, true, false));
}

#[test]
fn decide_fadeout_scene_elapsed_enters_scene_tail_on_early_skip() {
    let elapsed = decide_fadeout_scene_elapsed(
        Duration::from_millis(100),
        Duration::from_millis(250),
        Duration::from_millis(2500),
        Duration::from_millis(1000),
        DecideFadeoutSceneTiming::DefaultTail,
    );

    assert_eq!(elapsed, Duration::from_millis(1750));
}

#[test]
fn decide_fadeout_scene_elapsed_stretches_detected_tail_fadeout() {
    let elapsed = decide_fadeout_scene_elapsed(
        Duration::from_millis(100),
        Duration::from_millis(500),
        Duration::from_millis(2500),
        Duration::from_millis(1000),
        DecideFadeoutSceneTiming::TailStart(Duration::from_millis(2300)),
    );

    assert_eq!(elapsed, Duration::from_millis(2400));
}

#[test]
fn decide_fadeout_scene_elapsed_stays_direct_when_timer_fadeout_exists() {
    let elapsed = decide_fadeout_scene_elapsed(
        Duration::from_millis(100),
        Duration::from_millis(0),
        Duration::from_millis(2500),
        Duration::from_millis(500),
        DecideFadeoutSceneTiming::DirectOnly,
    );

    assert_eq!(elapsed, Duration::from_millis(100));
}

#[test]
fn decide_fadeout_scene_elapsed_does_not_rewind_auto_fadeout() {
    let elapsed = decide_fadeout_scene_elapsed(
        Duration::from_millis(2500),
        Duration::from_millis(250),
        Duration::from_millis(2500),
        Duration::from_millis(1000),
        DecideFadeoutSceneTiming::DefaultTail,
    );

    assert_eq!(elapsed, Duration::from_millis(2750));
}

#[test]
fn decide_scene_fadeout_tail_start_detects_scene_end_black_fade() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 6,
                "w": 1920,
                "h": 1080,
                "scene": 2500,
                "fadeout": 1000,
                "destination": [
                    { "id": -110, "loop": 800, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 255 },
                        { "time": 800, "a": 0 }
                    ] },
                    { "id": -110, "loop": 2500, "dst": [
                        { "time": 2300, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 2500, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();

    assert_eq!(decide_scene_fadeout_tail_start(Some(&document)), Some(2300));
}

#[test]
fn decide_scene_fadeout_tail_start_ignores_scene_tail_when_timer_fadeout_exists() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 6,
                "w": 1920,
                "h": 1080,
                "scene": 2500,
                "fadeout": 500,
                "destination": [
                    { "id": -110, "loop": 2000, "dst": [
                        { "time": 1500, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 2000, "a": 255 }
                    ] },
                    { "id": -110, "loop": 500, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 500, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();

    assert!(document_has_fadeout_timer_black(&document));
    assert_eq!(decide_fadeout_scene_timing(Some(&document)), DecideFadeoutSceneTiming::DirectOnly);
    assert_eq!(decide_scene_fadeout_tail_start(Some(&document)), None);
}

#[test]
fn lr2_decide_keeps_scene_clock_and_skips_for_close_or_fadeout() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"{ "type": 6, "lr2": true, "w": 640, "h": 480, "scene": 3000, "fadeout": 550 }"#,
    )
    .unwrap();
    // No scene-tail jump: LR2 draws the exit with its timer 2 / 3 objects.
    assert_eq!(decide_fadeout_scene_timing(Some(&document)), DecideFadeoutSceneTiming::DirectOnly);
    assert_eq!(lr2_decide_close_ms(&document), 550);
    document.close = 200;
    assert_eq!(lr2_decide_close_ms(&document), 200);
    document.lr2 = false;
    assert_eq!(decide_fadeout_scene_timing(Some(&document)), DecideFadeoutSceneTiming::DefaultTail);
}

#[test]
fn bga_option_cycles_on_auto_off() {
    assert!(matches!(cycle_bga_option(BgaModeConfig::On), BgaModeConfig::Auto));
    assert!(matches!(cycle_bga_option(BgaModeConfig::Auto), BgaModeConfig::Off));
    assert!(matches!(cycle_bga_option(BgaModeConfig::Off), BgaModeConfig::On));
}

#[test]
fn retry_preload_always_builds_fresh_audio_for_the_retried_chart() {
    assert_eq!(
        retry_preload_kind(ResultRetryMode::SameArrange, true),
        RetryPreloadKind::CachedChartWithFreshAudio
    );
    assert_eq!(
        retry_preload_kind(ResultRetryMode::SameArrange, false),
        RetryPreloadKind::ReimportedChartWithFreshAudio
    );
    assert_eq!(
        retry_preload_kind(ResultRetryMode::DifferentArrange, true),
        RetryPreloadKind::ReimportedChartWithFreshAudio
    );
    assert_eq!(
        retry_preload_kind(ResultRetryMode::DifferentArrange, false),
        RetryPreloadKind::ReimportedChartWithFreshAudio
    );
}

#[test]
fn hispeed_action_maps_left_and_right_presses() {
    assert_eq!(
        hispeed_action(PhysicalKey::Code(KeyCode::ArrowLeft), ElementState::Pressed, false),
        Some(HispeedChange::Down)
    );
    assert_eq!(
        hispeed_action(PhysicalKey::Code(KeyCode::ArrowRight), ElementState::Pressed, false),
        Some(HispeedChange::Up)
    );
}

#[test]
fn hispeed_action_rejects_releases_and_other_keys() {
    assert_eq!(
        hispeed_action(PhysicalKey::Code(KeyCode::ArrowLeft), ElementState::Released, false),
        None
    );
    assert_eq!(
        hispeed_action(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false),
        None
    );
}

#[test]
fn adjusted_hispeed_uses_configured_step_and_clamps_range() {
    assert_eq!(adjusted_hispeed(2.0, HispeedChange::Up, 0.25), 2.25);
    assert_eq!(adjusted_hispeed(2.0, HispeedChange::Down, 0.25), 1.75);
    assert_eq!(adjusted_hispeed(2.0, HispeedChange::Up, 0.5), 2.5);
    assert_eq!(adjusted_hispeed(20.0, HispeedChange::Up, 0.5), 20.0);
    assert_eq!(adjusted_hispeed(0.01, HispeedChange::Down, 0.5), 0.01);
}

#[test]
fn pending_hispeed_changes_use_displayed_mode_without_mutating_profile() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let profile_hispeed = profile.lane.hispeed;
    let mut lane = PendingPlayLaneState {
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Floating,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: true,
        lift_enabled: true,
        hidden_enabled: false,
        lane_cover_visible: true,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: false,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::Hispeed(HispeedChange::Up),
        &profile,
        120.0,
        false,
    ));

    assert_eq!(lane.hispeed, 2.5);
    assert_eq!(lane.target_green_number, 300);
    assert_eq!(profile.lane.hispeed, profile_hispeed);
}

#[test]
fn pending_disabled_covers_use_zero_cover_as_auto_adjust_trigger() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 1.0,
        hispeed_mode: HispeedMode::Floating,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.25,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: false,
        lift_enabled: false,
        hidden_enabled: false,
        lane_cover_visible: false,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: true,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP),
        &profile,
        240.0,
        false,
    ));

    assert!((lane.hispeed - 2.0).abs() < 0.000_1, "hispeed={}", lane.hispeed);
    assert!((lane.lane_cover - 0.25).abs() < f32::EPSILON);
    assert_eq!(lane.lift, 0.0);
    assert_eq!(lane.hidden_cover, 0.0);
}

#[test]
fn pending_disabled_covers_without_auto_adjust_change_hispeed_directly() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.25,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: false,
        lift_enabled: false,
        hidden_enabled: false,
        lane_cover_visible: false,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP),
        &profile,
        120.0,
        false,
    ));
    assert_eq!(lane.hispeed, 2.25);

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::AnalogLaneCoverDelta(3.0 * LANE_COVER_STEP),
        &profile,
        120.0,
        false,
    ));
    assert!((lane.hispeed - 2.28).abs() < 0.000_1, "hispeed={}", lane.hispeed);
    assert!((lane.lane_cover - 0.25).abs() < f32::EPSILON);
}

#[test]
fn pending_hidden_cover_reverses_digital_and_analog_lane_actions() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.3,
        sudden_enabled: false,
        lift_enabled: false,
        hidden_enabled: true,
        lane_cover_visible: false,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP),
        &profile,
        120.0,
        false,
    ));
    assert!((lane.hidden_cover - 0.301).abs() < 0.000_1);

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::AnalogLaneCoverDelta(-LANE_COVER_STEP),
        &profile,
        120.0,
        false,
    ));
    assert!((lane.hidden_cover - 0.3).abs() < 0.000_1);
}

#[test]
fn pending_lane_state_preserves_sub_one_hsfix_bpm() {
    let mut snapshot =
        RenderSnapshot { hispeed_mode_index: 1, min_bpm: 0.96, ..Default::default() };
    let mode_config = PlayModeConfig {
        target_green_number: 300,
        hispeed_auto_adjust: false,
        ..PlayModeConfig::default()
    };
    let mut lane =
        PendingPlayLaneState::from_snapshot(&snapshot, &mode_config, HsFixOption::MinBpm, 100);
    assert_eq!(lane.hsfix_base_bpm, 0.96);

    snapshot.min_bpm = 0.5;
    lane.sync_chart_bpm(&snapshot, HsFixOption::MinBpm);
    assert_eq!(lane.hsfix_base_bpm, 0.5);
}

#[test]
fn pending_green_number_change_switches_displayed_state_to_floating() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: true,
        lift_enabled: true,
        hidden_enabled: false,
        lane_cover_visible: true,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::GreenNumberDelta(1),
        &profile,
        120.0,
        false,
    ));

    assert_eq!(lane.hispeed_mode, HispeedMode::Floating);
    assert_eq!(lane.target_green_number, 601);
    let expected =
        crate::screens::play_snapshot::hispeed_for_green_number_values(601.0, 1.0, 120.0, 1.0);
    assert!((lane.hispeed - expected).abs() < 0.000_1, "hispeed={}", lane.hispeed);
}

#[test]
fn pending_floating_to_normal_uses_full_lane_rounding_boundaries() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    for (green_number, expected_level) in [(580, 7), (581, 6)] {
        let mut lane = PendingPlayLaneState {
            hispeed: crate::screens::play_snapshot::hispeed_for_green_number_values(
                green_number as f32,
                1.0,
                120.0,
                1.0,
            ),
            hispeed_mode: HispeedMode::Floating,
            base_hispeed_mode: HispeedMode::Normal,
            floating_policy: FloatingPolicy::Toggle,
            normal_hispeed_level: 18,
            target_green_number: 300,
            lane_cover: 0.25,
            lift: 0.10,
            hidden_cover: 0.0,
            sudden_enabled: true,
            lift_enabled: false,
            hidden_enabled: false,
            lane_cover_visible: true,
            lane_target: PlayLaneTarget::Lift,
            lane_cover_changing: true,
            hsfix_base_bpm: 120.0,
            classic_hsfix_scale: 1.0,
            hispeed_auto_adjust: false,
            playback_rate_percent: 100,
        };

        assert!(apply_pending_play_lane_action_to_state(
            &mut lane,
            PlayLaneAction::ToggleHispeedMode,
            &profile,
            120.0,
            false,
        ));

        assert_eq!(lane.hispeed_mode, HispeedMode::Normal);
        assert_eq!(lane.normal_hispeed_level, expected_level);
        let expected_green = crate::config::play::normal_hispeed_green_number(expected_level);
        assert_eq!(lane.current_full_lane_green_number(120.0), expected_green);
    }
}

#[test]
fn pending_normal_hispeed_action_changes_level_and_ignores_lane_effects() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 4.0,
        hispeed_mode: HispeedMode::Normal,
        base_hispeed_mode: HispeedMode::Normal,
        floating_policy: FloatingPolicy::Disabled,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.40,
        lift: 0.20,
        hidden_cover: 0.0,
        sudden_enabled: true,
        lift_enabled: true,
        hidden_enabled: false,
        lane_cover_visible: true,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::Hispeed(HispeedChange::Up),
        &profile,
        120.0,
        false,
    ));

    assert_eq!(lane.normal_hispeed_level, 19);
    assert_eq!(lane.current_full_lane_green_number(120.0), 280);
}

#[test]
fn pending_hispeed_policy_rejects_unavailable_controls() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let base = PendingPlayLaneState {
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Disabled,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: true,
        lift_enabled: false,
        hidden_enabled: false,
        lane_cover_visible: true,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    let mut disabled = base;
    assert!(!apply_pending_play_lane_action_to_state(
        &mut disabled,
        PlayLaneAction::ToggleHispeedMode,
        &profile,
        120.0,
        false,
    ));
    assert!(!apply_pending_play_lane_action_to_state(
        &mut disabled,
        PlayLaneAction::GreenNumberDelta(1),
        &profile,
        120.0,
        false,
    ));

    let mut locked = PendingPlayLaneState {
        hispeed_mode: HispeedMode::Floating,
        floating_policy: FloatingPolicy::Locked,
        ..base
    };
    assert!(!apply_pending_play_lane_action_to_state(
        &mut locked,
        PlayLaneAction::ToggleHispeedMode,
        &profile,
        120.0,
        false,
    ));
}

#[test]
fn pending_lane_state_rejects_all_no_speed_controls() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 2.0,
        hispeed_mode: HispeedMode::Floating,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: true,
        lift_enabled: true,
        hidden_enabled: false,
        lane_cover_visible: true,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: true,
        hsfix_base_bpm: 120.0,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    for action in [
        PlayLaneAction::ToggleHispeedMode,
        PlayLaneAction::Hispeed(HispeedChange::Up),
        PlayLaneAction::LaneCoverDelta(-LANE_COVER_STEP),
        PlayLaneAction::AnalogLaneCoverDelta(-LANE_COVER_STEP),
        PlayLaneAction::GreenNumberDelta(1),
        PlayLaneAction::ToggleLaneCoverVisibility,
    ] {
        assert!(
            !apply_pending_play_lane_action_to_state(&mut lane, action, &profile, 120.0, true,)
        );
    }
    assert_eq!(lane.hispeed_mode, HispeedMode::Floating);
    assert_eq!(lane.hispeed, 2.0);
    assert_eq!(lane.target_green_number, 300);
    assert_eq!(lane.lane_cover, 0.0);
    assert_eq!(lane.lift, 0.0);
    assert!(lane.lane_cover_visible);
}

#[test]
fn pending_lane_actions_replay_once_on_loaded_session() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );
    let initial_hispeed = session.hispeed;
    let hispeed_step = hispeed_step_for_profile(&profile, session.hispeed_mode);
    let mut lane_target = PlayLaneTarget::Lift;

    replay_pending_play_lane_actions(
        &mut session,
        &mut lane_target,
        &[PlayLaneAction::Hispeed(HispeedChange::Up)],
        &profile,
        false,
    );

    assert_eq!(session.hispeed, initial_hispeed + hispeed_step);
    replay_pending_play_lane_actions(
        &mut session,
        &mut lane_target,
        &[PlayLaneAction::LaneCoverDelta(-LANE_COVER_STEP)],
        &profile,
        false,
    );
    assert!((session.lane_cover - LANE_COVER_STEP).abs() < f32::EPSILON);
}
