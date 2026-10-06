use super::*;

#[test]
fn hsfix_scroll_is_applied_once_during_recalculation_and_mode_switching() {
    use bmz_chart::model::{NoteEvent, NoteKind, ScrollEvent};
    use bmz_core::ids::NoteId;
    use bmz_core::time::ChartTick;

    let mut chart = app_test_chart();
    chart.scroll_events = vec![
        ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 },
        ScrollEvent { tick: ChartTick(960), time: TimeUs(500_000), factor: 4.0 },
    ];
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(960),
        time: TimeUs(500_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    for hs_fix in [HsFixOption::MinBpm, HsFixOption::MaxBpm, HsFixOption::MainBpm] {
        let mut session = crate::screens::play_session::build_game_session(
            std::sync::Arc::new(chart.clone()),
            &profile,
            crate::screens::play_session::PlaySessionOptions { hs_fix, ..Default::default() },
        );
        assert_eq!(session.hsfix_base_bpm, 480.0);
        session.hispeed_auto_adjust = true;
        reset_floating_hispeed_if_enabled(&mut session, false);
        assert!((session.hispeed - 1.0).abs() < 0.0001);
        assert_eq!(current_green_number(&session, TimeUs(0)), 300);
        assert_eq!(current_full_lane_green_number(&session, TimeUs(0)), 300);
        assert!((hispeed_for_normal_level(&session, 18, TimeUs(0)) - 1.0).abs() < 0.0001);

        let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        session.audio_clock =
            bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);
        session.lane_cover = 0.25;
        // 開始後の自動調整は現在位置の120 BPM × SCROLL 2を使う。
        reset_floating_hispeed_if_enabled(&mut session, false);
        assert!((session.hispeed - 1.5).abs() < 0.0001);
        // 自動調整OFFでは曲開始後もHS-FIXの実効480 BPMを使う。
        session.hispeed_auto_adjust = false;
        reset_floating_hispeed_if_enabled(&mut session, false);
        assert!((session.hispeed - 0.75).abs() < 0.0001);
        apply_lane_cover_step_to_session(&mut session, &mut PlayLaneTarget::Sudden, -0.25, false);
        assert!((session.hispeed - 0.5).abs() < 0.0001);
    }
}

#[test]
fn floating_hispeed_recalculation_uses_hsfix_base_before_chart_start() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = 120.0;
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: bmz_chart::model::TimingEventKind::BpmChange { bpm: 240.0 },
    });
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::MaxBpm,
            ..Default::default()
        },
    );
    session.lane_cover = 0.25;

    reset_floating_hispeed_if_enabled(&mut session, false);

    assert_eq!(session.hsfix_base_bpm, 240.0);
    assert!((session.hispeed - 1.5).abs() < 0.000_1, "hispeed={}", session.hispeed);
}

#[test]
fn floating_hispeed_recalculation_preserves_sub_one_hsfix_base() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.target_green_number = 999;
    profile.lane.sudden = 950;
    profile.lane.hispeed_auto_adjust = false;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = 189.0;
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: bmz_chart::model::TimingEventKind::BpmChange { bpm: 0.96 },
    });
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::MinBpm,
            ..Default::default()
        },
    );

    reset_floating_hispeed_if_enabled(&mut session, false);

    assert_eq!(session.hsfix_base_bpm, 0.96);
    assert_eq!(floating_hispeed_target_bpm(&session, TimeUs(0)), 0.96);
    assert!((session.hispeed - 7.507_51).abs() < 0.000_1, "hispeed={}", session.hispeed);
}

#[test]
fn floating_hispeed_recalculation_uses_current_bpm_after_chart_start() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.hispeed_auto_adjust = true;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = 120.0;
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: bmz_chart::model::TimingEventKind::BpmChange { bpm: 240.0 },
    });
    let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::MaxBpm,
            ..Default::default()
        },
    );
    session.audio_clock = bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);

    apply_lane_cover_step_to_session(&mut session, &mut PlayLaneTarget::Lift, -0.25, false);

    assert_eq!(session.hsfix_base_bpm, 240.0);
    assert!((session.hispeed - 3.0).abs() < 0.000_1, "hispeed={}", session.hispeed);
}

#[test]
fn lane_cover_change_uses_hsfix_base_when_hispeed_auto_adjust_is_off() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.hispeed_auto_adjust = false;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = 120.0;
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: bmz_chart::model::TimingEventKind::BpmChange { bpm: 240.0 },
    });
    let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::MaxBpm,
            ..Default::default()
        },
    );
    session.audio_clock = bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);

    apply_lane_cover_step_to_session(&mut session, &mut PlayLaneTarget::Lift, -0.25, false);

    assert!(!session.hispeed_auto_adjust);
    assert!((session.hispeed - 1.5).abs() < 0.000_1, "hispeed={}", session.hispeed);
}

#[test]
fn egui_lane_profile_cover_change_keeps_runtime_nhs_hispeed() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let before = profile.lane.clone();
    let mut edited = profile.lane.clone();
    edited.sudden = 250;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );
    session.hispeed = 3.5;

    assert!(apply_profile_lane_settings_to_session(
        &mut session,
        &before,
        profile.play.lane_effect,
        &edited,
        profile.play.lane_effect,
        false,
        false,
    ));
    assert!((session.hispeed - 3.5).abs() < f32::EPSILON);
    assert!((session.lane_cover - 0.25).abs() < f32::EPSILON);
}

#[test]
fn egui_lane_profile_changes_do_not_modify_no_speed_session() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let before = profile.lane.clone();
    let mut edited = profile.lane.clone();
    edited.hispeed = 4.0;
    edited.floating_policy = FloatingPolicyConfig::Locked;
    edited.sudden = 250;
    edited.lift = 100;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            speed_constraint: bmz_core::course::CourseSpeedConstraint::NoSpeed,
            ..Default::default()
        },
    );

    assert!(!apply_profile_lane_settings_to_session(
        &mut session,
        &before,
        profile.play.lane_effect,
        &edited,
        profile.play.lane_effect,
        true,
        false,
    ));
    assert_eq!(session.hispeed, 1.0);
    assert_eq!(session.hispeed_mode, HispeedMode::Classic);
    assert_eq!(session.lane_cover, 0.0);
    assert_eq!(session.lift, 0.0);
}

#[test]
fn egui_lane_profile_cannot_enable_constant_during_practice() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let before = profile.lane.clone();
    let mut edited = profile.lane.clone();
    edited.constant_enabled = true;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            session_mode: SessionMode::Practice,
            ..Default::default()
        },
    );

    assert!(apply_profile_lane_settings_to_session(
        &mut session,
        &before,
        profile.play.lane_effect,
        &edited,
        profile.play.lane_effect,
        false,
        true,
    ));
    assert!(!session.constant_enabled);
}

#[test]
fn egui_lane_profile_target_change_recalculates_fhs_hispeed() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    let before = profile.lane.clone();
    let mut edited = profile.lane.clone();
    edited.target_green_number = 320;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::StartBpm,
            ..Default::default()
        },
    );

    assert!(apply_profile_lane_settings_to_session(
        &mut session,
        &before,
        profile.play.lane_effect,
        &edited,
        profile.play.lane_effect,
        false,
        false,
    ));
    assert_eq!(session.hispeed_mode, HispeedMode::Floating);
    assert_eq!(session.target_green_number, 320);
    assert!((session.hispeed - 3.75).abs() < 0.000_1, "hispeed={}", session.hispeed);
}

#[test]
fn chart_play_start_boundary_waits_until_running_clock_reaches_zero() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );

    assert!(!chart_play_has_started(&session));

    session.audio_clock =
        bmz_audio::clock::AudioClock::with_position(48_000, 0, -1_000_000, frame.clone(), true);
    assert!(!chart_play_has_started(&session));

    frame.store(48_000, std::sync::atomic::Ordering::Relaxed);
    assert!(chart_play_has_started(&session));
}

#[test]
fn lane_cover_step_moves_one_profile_unit() {
    assert!((LANE_COVER_STEP - 0.001).abs() < f32::EPSILON);
}

#[test]
fn lane_cover_step_accelerates_on_key_repeat() {
    assert_eq!(
        lane_cover_step(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false),
        Some(0.001)
    );
    assert_eq!(
        lane_cover_step(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, true),
        Some(0.01)
    );
    assert_eq!(
        lane_cover_step(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Pressed, true),
        Some(-0.01)
    );
}

#[test]
fn lane_cover_step_clamps_sudden_and_lift_to_combined_range() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );

    session.lift = 0.2;
    session.lane_cover = 0.79;
    session.lane_cover_visible = true;
    let mut lane_target = PlayLaneTarget::Lift;
    assert!(apply_lane_cover_step_to_session(&mut session, &mut lane_target, -0.02, false,));
    assert!((session.lane_cover - 0.8).abs() < 0.000_01);

    session.lane_cover = 0.3;
    session.lift = 0.69;
    session.lane_cover_visible = false;
    assert!(apply_lane_cover_step_to_session(&mut session, &mut lane_target, 0.02, false,));
    assert!((session.lift - 0.7).abs() < 0.000_01);
}

#[test]
fn disabled_covers_use_zero_cover_as_auto_adjust_trigger() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.lift_enabled = false;
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.hispeed_auto_adjust = true;
    profile.lane.target_green_number = 300;
    profile.lane.sudden = 250;
    profile.lane.lift = 200;
    profile.lane.hidden = 300;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::StartBpm,
            ..Default::default()
        },
    );
    let mut lane_target = PlayLaneTarget::Lift;
    session.hispeed = 1.0;

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::LaneCoverDelta(0.1),
        false,
        0.5,
    ));
    assert!(!toggle_lane_cover_visibility(&mut session, false));
    assert!((session.hispeed - 4.0).abs() < 0.000_1, "hispeed={}", session.hispeed);
    assert!((session.lane_cover - 0.25).abs() < f32::EPSILON);
    assert_eq!(session.lift, 0.0);
    assert_eq!(session.hidden_cover, 0.0);
    assert!(!session.lane_cover_visible);
}

#[test]
fn disabled_cover_trigger_recalculates_with_current_bpm() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.lift_enabled = false;
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.hispeed_auto_adjust = true;
    profile.lane.target_green_number = 300;
    profile.lane.sudden = 250;
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = 120.0;
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: bmz_chart::model::TimingEventKind::BpmChange { bpm: 240.0 },
    });
    let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(48_000));
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::MaxBpm,
            ..Default::default()
        },
    );
    session.audio_clock = bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);
    session.hispeed = 1.0;
    let mut lane_target = PlayLaneTarget::Lift;

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP),
        false,
        0.5,
    ));

    assert!((session.hispeed - 2.0).abs() < 0.000_1, "hispeed={}", session.hispeed);
    assert!((session.lane_cover - 0.25).abs() < f32::EPSILON);
}

#[test]
fn disabled_covers_without_auto_adjust_change_hispeed_directly() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.lift_enabled = false;
    profile.lane.hispeed_auto_adjust = false;
    profile.lane.sudden = 250;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );
    let mut lane_target = PlayLaneTarget::Lift;
    session.hispeed = 2.0;

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP),
        false,
        0.25,
    ));
    assert_eq!(session.hispeed, 2.25);

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::AnalogLaneCoverDelta(3.0 * LANE_COVER_STEP),
        false,
        0.25,
    ));
    assert!((session.hispeed - 2.28).abs() < 0.000_1, "hispeed={}", session.hispeed);
    assert!((session.lane_cover - 0.25).abs() < f32::EPSILON);
}

#[test]
fn lift_and_hidden_share_lane_actions_with_an_explicit_target() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Hidden;
    profile.lane.lift_enabled = true;
    profile.lane.lift = 200;
    profile.lane.hidden = 300;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );
    let mut lane_target = PlayLaneTarget::Lift;

    assert!(apply_lane_cover_step_to_session(&mut session, &mut lane_target, 0.1, false,));
    assert!((session.lift - 0.3).abs() < f32::EPSILON);
    assert!((session.hidden_cover - 0.3).abs() < f32::EPSILON);

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::ToggleHispeedMode,
        false,
        0.25,
    ));
    assert_eq!(lane_target, PlayLaneTarget::Hidden);
    assert_eq!(session.hispeed_mode, HispeedMode::Classic);

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::LaneCoverDelta(0.1),
        false,
        0.25,
    ));
    assert!((session.hidden_cover - 0.4).abs() < f32::EPSILON);

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::AnalogLaneCoverDelta(-0.1),
        false,
        0.25,
    ));
    assert!((session.hidden_cover - 0.3).abs() < f32::EPSILON);
}

#[test]
fn egui_cover_enable_changes_apply_to_the_active_session() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.hidden = 400;
    let before = profile.lane.clone();
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );

    assert!(apply_profile_lane_settings_to_session(
        &mut session,
        &before,
        LaneEffectConfig::Off,
        &profile.lane,
        LaneEffectConfig::Hidden,
        false,
        false,
    ));
    assert!(session.hidden_enabled);
    assert!((session.hidden_cover - 0.4).abs() < f32::EPSILON);

    assert!(apply_profile_lane_settings_to_session(
        &mut session,
        &profile.lane,
        LaneEffectConfig::Hidden,
        &profile.lane,
        LaneEffectConfig::Off,
        false,
        false,
    ));
    assert!(!session.hidden_enabled);
    assert_eq!(session.hidden_cover, 0.0);
}

#[test]
fn play_start_double_press_registers_within_window() {
    let mut last = None;
    let t0 = Instant::now();
    assert!(!register_play_start_double_press(&mut last, t0));
    assert_eq!(last, Some(t0));

    let t1 = t0 + Duration::from_millis(200);
    assert!(register_play_start_double_press(&mut last, t1));
    assert_eq!(last, None);
}

#[test]
fn play_start_double_press_expires_outside_window() {
    let mut last = None;
    let t0 = Instant::now();
    assert!(!register_play_start_double_press(&mut last, t0));

    let t1 = t0 + PLAY_START_DOUBLE_PRESS_WINDOW + Duration::from_millis(1);
    assert!(!register_play_start_double_press(&mut last, t1));
    assert_eq!(last, Some(t1));
}

#[test]
fn toggle_lane_cover_visibility_flips_sudden_display() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );
    session.lane_cover_visible = true;

    toggle_lane_cover_visibility(&mut session, false);
    assert!(!session.lane_cover_visible);

    toggle_lane_cover_visibility(&mut session, false);
    assert!(session.lane_cover_visible);
}

#[test]
fn green_number_step_switches_normal_hispeed_to_floating() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );

    assert!(apply_green_number_step_to_session(&mut session, 1, false));

    assert_eq!(session.hispeed_mode, HispeedMode::Floating);
    assert_eq!(session.target_green_number, 601);
    assert!(session.hispeed < 2.0);
}

#[test]
fn active_lane_state_rejects_all_no_speed_controls() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );

    let mut lane_target = PlayLaneTarget::Lift;
    for action in [
        PlayLaneAction::ToggleHispeedMode,
        PlayLaneAction::Hispeed(HispeedChange::Up),
        PlayLaneAction::LaneCoverDelta(-LANE_COVER_STEP),
        PlayLaneAction::AnalogLaneCoverDelta(-LANE_COVER_STEP),
        PlayLaneAction::GreenNumberDelta(1),
        PlayLaneAction::ToggleLaneCoverVisibility,
    ] {
        assert!(!apply_play_lane_action_to_session(
            &mut session,
            &mut lane_target,
            action,
            true,
            0.25,
        ));
    }
    assert_eq!(session.hispeed_mode, HispeedMode::Classic);
    assert_eq!(session.target_green_number, 300);
    assert_eq!(session.hispeed, 2.0);
    assert_eq!(session.lane_cover, 0.0);
    assert_eq!(session.lift, 0.0);
    assert!(!session.lane_cover_visible);
}

#[test]
fn no_speed_lane_state_is_not_selected_for_profile_save() {
    let lane_state = ActiveLaneState {
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: true,
        lift_enabled: true,
        hidden_enabled: false,
        hispeed_mode: HispeedMode::Normal,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Toggle,
        normal_hispeed_level: 18,
        target_green_number: 300,
    };

    let (hispeed, lane_state) = lane_state_for_profile_save(true, Some(1.0), Some(lane_state));

    assert!(hispeed.is_none());
    assert!(lane_state.is_none());
}

#[test]
fn floating_hispeed_change_keeps_target_green_during_play() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::StartBpm,
            ..Default::default()
        },
    );

    let hispeed = session.hispeed;
    apply_hispeed_change_to_session(&mut session, HispeedChange::Up, 0.5);

    assert_eq!(session.hispeed, hispeed + 0.5);
    assert_eq!(session.target_green_number, 300);
}

#[test]
fn e1_hispeed_change_keeps_target_green_during_play() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::StartBpm,
            ..Default::default()
        },
    );

    assert!(apply_play_option_control_to_session(
        &mut session,
        PlayOptionControl::Hispeed(HispeedChange::Up),
        false,
        0.5,
    ));

    assert_eq!(session.target_green_number, 300);
}
