use super::*;

#[test]
fn active_lane_state_keeps_green_number_captured_when_switching_to_fhs() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );
    let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    session.audio_clock = bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);
    let expected_target = current_green_number(&session, session.audio_clock.now());
    assert_ne!(expected_target, session.target_green_number);

    assert!(apply_play_option_control_to_session(
        &mut session,
        PlayOptionControl::ToggleHispeedMode,
        false,
        0.25,
    ));
    assert_eq!(session.hispeed_mode, HispeedMode::Floating);
    assert_eq!(session.target_green_number, expected_target);

    // CHSへ戻ってHSを変更しても、終了時の現在緑数字でtargetを上書きしない。
    session.hispeed = 1.0;
    assert!(apply_play_option_control_to_session(
        &mut session,
        PlayOptionControl::ToggleHispeedMode,
        false,
        0.25,
    ));
    let state = active_lane_state_for_session(&session);

    assert_eq!(state.hispeed_mode, HispeedMode::Classic);
    assert_eq!(state.target_green_number, expected_target);
}

#[test]
fn play_option_control_maps_seven_key_lane_and_scratch_targets() {
    let input = crate::config::play_input::default_profile_input();
    let keys = SelectKeyBindings::from_profile(&input);
    let play_input = play_option_input_for(&input, KeyMode::K7);

    assert_eq!(
        keyboard_play_option("W", true, true, &keys, &play_input, &input),
        Some(PlayOptionControl::ToggleHispeedMode)
    );
    assert_eq!(
        keyboard_play_option("Z", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Down))
    );
    assert_eq!(
        keyboard_play_option("V", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Down))
    );
    assert_eq!(
        keyboard_play_option("S", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Up))
    );
    assert_eq!(
        keyboard_play_option("F", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Up))
    );
    assert_eq!(
        keyboard_play_option("LShift", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::LaneCover(LaneCoverChange::Up))
    );
    assert_eq!(
        keyboard_play_option("LControl", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::LaneCover(LaneCoverChange::Down))
    );
}

#[test]
fn play_option_control_maps_scratch_for_scratchless_key_modes() {
    let input = crate::config::play_input::default_profile_input();
    let keys = SelectKeyBindings::from_profile(&input);

    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        let play_input = play_option_input_for(&input, key_mode);
        assert_eq!(
            keyboard_play_option("LShift", true, false, &keys, &play_input, &input),
            Some(PlayOptionControl::LaneCover(LaneCoverChange::Up)),
            "{} Scratch Up",
            key_mode.as_str(),
        );
        assert_eq!(
            keyboard_play_option("LControl", true, false, &keys, &play_input, &input),
            Some(PlayOptionControl::LaneCover(LaneCoverChange::Down)),
            "{} Scratch Down",
            key_mode.as_str(),
        );
        assert_eq!(
            keyboard_play_option("LShift", false, true, &keys, &play_input, &input),
            Some(PlayOptionControl::GreenNumber(GreenNumberChange::Up)),
            "{} Scratch Up green number",
            key_mode.as_str(),
        );
        assert_eq!(
            keyboard_play_option("LControl", false, true, &keys, &play_input, &input),
            Some(PlayOptionControl::GreenNumber(GreenNumberChange::Down)),
            "{} Scratch Down green number",
            key_mode.as_str(),
        );
    }
}

#[test]
fn play_option_control_maps_e2_to_mode_specific_green_number_direction() {
    let input = crate::config::play_input::default_profile_input();
    let keys = SelectKeyBindings::from_profile(&input);
    let play_input = play_option_input_for(&input, KeyMode::K7);

    assert_eq!(
        keyboard_play_option("Z", false, true, &keys, &play_input, &input),
        Some(PlayOptionControl::GreenNumber(GreenNumberChange::Down))
    );
    assert_eq!(
        keyboard_play_option("S", false, true, &keys, &play_input, &input),
        Some(PlayOptionControl::GreenNumber(GreenNumberChange::Up))
    );
    assert_eq!(
        keyboard_play_option("LShift", false, true, &keys, &play_input, &input),
        Some(PlayOptionControl::GreenNumber(GreenNumberChange::Up))
    );
    assert_eq!(
        keyboard_play_option("LControl", false, true, &keys, &play_input, &input),
        Some(PlayOptionControl::GreenNumber(GreenNumberChange::Down))
    );
    assert_eq!(keyboard_play_option("Z", true, true, &keys, &play_input, &input), None);
}

#[test]
fn play_option_control_applies_eight_key_default_and_override() {
    let mut input = crate::config::play_input::default_profile_input();
    let keys = SelectKeyBindings::from_profile(&input);
    let play_input = play_option_input_for(&input, KeyMode::K8);

    assert_eq!(
        keyboard_play_option("Z", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Up))
    );
    assert!(crate::config::play_input::set_eight_key_hispeed_direction(
        &mut input,
        LaneConfig::Key1,
        HispeedDirectionConfig::Down,
    ));
    assert_eq!(
        keyboard_play_option("Z", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Down))
    );
}

#[test]
fn play_option_control_distinguishes_two_player_gamepads() {
    let mut input = crate::config::play_input::default_profile_input();
    input.play.insert(
        KeyMode::K14.play_map_key().to_string(),
        crate::config::profile_config::PlayModeInputConfig {
            inherit: None,
            bindings: vec![
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad1",
                    "Button1",
                    LaneConfig::Key1,
                ),
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad2",
                    "Button1",
                    LaneConfig::Key9,
                ),
            ],
            ..Default::default()
        },
    );
    let slots = crate::input::gamepad::GamepadSlotMap::from_device_ids([
        Some(DeviceId(11)),
        Some(DeviceId(22)),
    ]);
    let play_input = PlayOptionInput::new(
        KeyMode::K14,
        crate::config::play::lane_binding_for_chart_with_slots(&input, KeyMode::K14, slots),
        &input,
        slots,
    );
    let control = PhysicalControl::GamepadButton("Button1".to_string());

    assert_eq!(
        play_option_control_for_input(
            DeviceId(11),
            &control,
            true,
            false,
            Some(&play_input),
            &input,
        ),
        Some(PlayOptionControl::Hispeed(HispeedChange::Down))
    );
    assert_eq!(
        play_option_control_for_input(
            DeviceId(22),
            &control,
            true,
            false,
            Some(&play_input),
            &input,
        ),
        Some(PlayOptionControl::Hispeed(HispeedChange::Up))
    );
}

#[test]
fn bounce_bypass_requires_synthesized_axis_bound_to_profile_scratch_lane() {
    let mut input = crate::config::play_input::default_profile_input();
    input.play.insert(
        KeyMode::K14.play_map_key().to_string(),
        crate::config::profile_config::PlayModeInputConfig {
            inherit: None,
            bindings: vec![
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad1",
                    "Axis1+",
                    LaneConfig::Scratch,
                ),
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad1",
                    "Axis2+",
                    LaneConfig::Key1,
                ),
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad1",
                    "Axis3+",
                    LaneConfig::Scratch2,
                ),
            ],
            ..Default::default()
        },
    );
    let slots = crate::input::gamepad::GamepadSlotMap::from_device_ids([Some(DeviceId(11)), None]);
    let binding =
        crate::config::play::lane_binding_for_chart_with_slots(&input, KeyMode::K14, slots);
    let event = |name: &str, device_id, synthesized_analog_axis| {
        crate::input::gamepad::GamepadButtonEvent {
            name: name.to_string(),
            device_id,
            pressed: true,
            timestamp: bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(1),
            synthesized_analog_axis,
        }
    };

    assert!(should_bypass_analog_scratch_bounce(
        &event("Axis1+", DeviceId(11), true),
        Some(&binding),
    ));
    assert!(!should_bypass_analog_scratch_bounce(
        &event("Axis2+", DeviceId(11), true),
        Some(&binding),
    ));
    assert!(should_bypass_analog_scratch_bounce(
        &event("Axis3+", DeviceId(11), true),
        Some(&binding),
    ));
    assert!(!should_bypass_analog_scratch_bounce(
        &event("Axis1+", DeviceId(11), false),
        Some(&binding),
    ));
    assert!(!should_bypass_analog_scratch_bounce(
        &event("Axis1+", DeviceId(22), true),
        Some(&binding),
    ));
    assert!(!should_bypass_analog_scratch_bounce(&event("Axis1+", DeviceId(11), true), None,));
}

#[test]
fn play_option_control_prioritizes_two_player_lane_over_other_devices_e2_button() {
    let mut input = crate::config::play_input::default_profile_input();
    input.ui.bindings.retain(|entry| {
        entry.action != Some(InputActionConfig::E2)
            || !crate::config::play_input::is_gamepad_device(&entry.device)
    });
    input.ui.bindings.push(crate::config::profile_config::BindingConfigEntry {
        device: "gamepad1".to_string(),
        control: "Button10".to_string(),
        keyboard_slot: None,
        lane: None,
        action: Some(InputActionConfig::E2),
        scratch: None,
    });
    input.ui.bindings.push(crate::config::profile_config::BindingConfigEntry {
        device: "gamepad1".to_string(),
        control: "Button9".to_string(),
        keyboard_slot: None,
        lane: None,
        action: Some(InputActionConfig::E1),
        scratch: None,
    });
    input.play.insert(
        KeyMode::K14.play_map_key().to_string(),
        crate::config::profile_config::PlayModeInputConfig {
            inherit: None,
            bindings: vec![
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad1",
                    "Button1",
                    LaneConfig::Key1,
                ),
                crate::config::play_input::gamepad_play_binding_for_device(
                    "gamepad2",
                    "Button10",
                    LaneConfig::Key9,
                ),
            ],
            ..Default::default()
        },
    );
    let slots = crate::input::gamepad::GamepadSlotMap::from_device_ids([
        Some(DeviceId(11)),
        Some(DeviceId(22)),
    ]);
    let play_input = PlayOptionInput::new(
        KeyMode::K14,
        crate::config::play::lane_binding_for_chart_with_slots(&input, KeyMode::K14, slots),
        &input,
        slots,
    );
    let control = PhysicalControl::GamepadButton("Button10".to_string());

    assert_eq!(
        play_option_control_for_input(
            DeviceId(11),
            &control,
            true,
            true,
            Some(&play_input),
            &input,
        ),
        Some(PlayOptionControl::ToggleHispeedMode)
    );
    assert_eq!(
        play_option_control_for_input(
            DeviceId(11),
            &PhysicalControl::GamepadButton("Button9".to_string()),
            true,
            true,
            Some(&play_input),
            &input,
        ),
        Some(PlayOptionControl::ToggleHispeedMode)
    );
    assert_eq!(
        play_option_control_for_input(
            DeviceId(22),
            &control,
            true,
            false,
            Some(&play_input),
            &input,
        ),
        Some(PlayOptionControl::Hispeed(HispeedChange::Up))
    );
    assert_eq!(
        play_option_control_for_input(
            DeviceId(22),
            &control,
            false,
            true,
            Some(&play_input),
            &input,
        ),
        Some(PlayOptionControl::GreenNumber(GreenNumberChange::Up))
    );

    let p2_lane_pressed = HashSet::from([(DeviceId(22), control.clone())]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&p2_lane_pressed, &play_input),
        (false, false, false)
    );
    let p1_e2_pressed = HashSet::from([(DeviceId(11), control)]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&p1_e2_pressed, &play_input),
        (false, true, false)
    );
}

#[test]
fn floating_hispeed_formula_uses_green_number_and_lane_cover() {
    assert_eq!(hispeed_for_green_number_values(300.0, 1.0, 120.0, 1.0), 4.0);
    assert_eq!(hispeed_for_green_number_values(300.0, 0.5, 120.0, 1.0), 2.0);
    assert_eq!(hispeed_for_green_number_values(300.0, 1.0, 240.0, 1.0), 2.0);
    assert_eq!(hispeed_for_green_number_values(300.0, 1.0, 0.96, 1.0), 500.0);
    assert_eq!(hispeed_for_green_number_values(300.0, 1.0, 120.0, 2.0), 2.0);
    assert!(
        (hispeed_for_green_number_values(295.0, 0.93, 120.0, 1.0) - 3.783_051).abs() < 0.000_01
    );
}

#[test]
fn green_number_change_uses_the_displayed_integer_duration() {
    assert_eq!(green_number_from_display_duration(500.0), 300);
    assert_eq!(green_number_from_display_duration(500.6), 301);
}

#[test]
fn active_lane_state_saves_current_green_number_for_nhs() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    apply_current_play_options_to_profile(
        &mut profile,
        Some(2.0),
        Some(ActiveLaneState {
            lane_cover: 0.0,
            lift: 0.0,
            hidden_cover: 0.0,
            sudden_enabled: true,
            lift_enabled: true,
            hidden_enabled: false,
            hispeed_mode: HispeedMode::Normal,
            base_hispeed_mode: HispeedMode::Normal,
            floating_policy: FloatingPolicy::Toggle,
            normal_hispeed_level: 18,
            target_green_number: 600,
        }),
        CurrentPlayOptions {
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            target: TargetOption::None,
            gauge: GaugeTypeConfig::Normal,
            gauge_auto_shift: GaugeAutoShiftConfig::Off,
            bottom_shiftable_gauge: BottomShiftableGaugeConfig::Easy,
            double_option: DoubleOption::Off,
            hs_fix: HsFixOption::Off,
            session_mode: SessionMode::Normal,
        },
        42,
    );

    assert_eq!(profile.lane.base_hispeed, BaseHispeedConfig::Normal);
    assert_eq!(profile.lane.floating_policy, FloatingPolicyConfig::Toggle);
    assert_eq!(profile.lane.target_green_number, 600);
}

#[test]
fn classic_hispeed_rounding_restores_quarter_steps() {
    assert_eq!(clamp_hispeed_for_profile(3.783_051, HispeedMode::Classic, 0.25), 3.75);
}

#[test]
fn custom_hispeed_step_preserves_non_quarter_profile_values() {
    assert_eq!(clamp_hispeed_for_profile(2.3, HispeedMode::Classic, 0.3), 2.3);
    assert_eq!(clamp_hispeed_for_profile(2.37, HispeedMode::Floating, 0.5), 2.37);
    assert_eq!(clamp_hispeed_for_profile(0.145_620_94, HispeedMode::Floating, 0.5), 0.145_620_94);
    assert_eq!(clamp_hispeed_for_profile(0.01, HispeedMode::Classic, 0.25), 0.01);
}

#[test]
fn gauge_option_cycle_includes_auto_shift() {
    assert_eq!(cycle_gauge_option(GaugeTypeConfig::ExHard), GaugeTypeConfig::Hazard);
    assert_eq!(
        cycle_gauge_auto_shift_option(GaugeAutoShiftConfig::Off),
        GaugeAutoShiftConfig::Continue
    );
    assert_eq!(gauge_auto_shift_as_str(GaugeAutoShiftConfig::BestClear), "BEST CLEAR");
    assert_eq!(
        cycle_bottom_shiftable_gauge_with_direction(BottomShiftableGaugeConfig::Normal, 1),
        BottomShiftableGaugeConfig::AssistEasy
    );
    assert_eq!(bottom_shiftable_gauge_as_str(BottomShiftableGaugeConfig::Easy), "EASY");
    assert_eq!(cycle_gauge_option(GaugeTypeConfig::AutoShift), GaugeTypeConfig::Hazard);
}

#[test]
fn apply_current_play_options_updates_profile_defaults() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    apply_current_play_options_to_profile(
        &mut profile,
        Some(3.37),
        Some(ActiveLaneState {
            lane_cover: 0.42,
            lift: 0.1,
            hidden_cover: 0.0,
            sudden_enabled: true,
            lift_enabled: true,
            hidden_enabled: false,
            hispeed_mode: HispeedMode::Floating,
            base_hispeed_mode: HispeedMode::Classic,
            floating_policy: FloatingPolicy::Toggle,
            normal_hispeed_level: 18,
            target_green_number: 280,
        }),
        CurrentPlayOptions {
            arrange: ArrangeOption::Mirror,
            arrange_2p: ArrangeOption::Random,
            target: TargetOption::RankAaa,
            gauge: GaugeTypeConfig::Hard,
            gauge_auto_shift: GaugeAutoShiftConfig::BestClear,
            bottom_shiftable_gauge: BottomShiftableGaugeConfig::Normal,
            double_option: DoubleOption::Flip,
            hs_fix: HsFixOption::MainBpm,
            session_mode: SessionMode::Autoplay,
        },
        42,
    );

    assert_eq!(profile.lane.hispeed, 3.37);
    assert_eq!(profile.lane.sudden, 420);
    assert_eq!(profile.lane.lift, 100);
    assert_eq!(profile.lane.base_hispeed, BaseHispeedConfig::Classic);
    assert_eq!(profile.lane.floating_policy, FloatingPolicyConfig::Toggle);
    assert_eq!(profile.lane.target_green_number, 280);
    assert!(matches!(profile.play.random, RandomOptionConfig::Mirror));
    assert!(matches!(profile.play.random2, RandomOptionConfig::Random));
    assert!(matches!(profile.play.target, TargetOptionConfig::RankAaa));
    assert!(matches!(profile.play.gauge, GaugeTypeConfig::Hard));
    assert!(matches!(profile.play.gauge_auto_shift, GaugeAutoShiftConfig::BestClear));
    assert!(matches!(profile.play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::Normal));
    assert!(matches!(profile.play.double_option, DoubleOptionConfig::Flip));
    assert!(matches!(profile.play.hs_fix, HsFixConfig::MainBpm));
    assert!(profile.play.auto_play);
    assert_eq!(profile.play.assist, crate::config::profile_config::AssistOptionConfig::default());
    assert_eq!(profile.updated_at, 42);
}

#[test]
fn profile_play_option_changes_disable_random_and_autoplay_without_rollback() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.random = RandomOptionConfig::Random;
    profile.play.random2 = RandomOptionConfig::Mirror;
    profile.play.session_mode = None;
    profile.play.auto_play = true;
    let before = profile.play.clone();
    let current = select_play_options_from_profile(&before);

    profile.play.random = RandomOptionConfig::Off;
    profile.play.random2 = RandomOptionConfig::Off;
    profile.play.auto_play = false;
    let synced = merge_changed_select_play_options_from_profile(current, &before, &profile.play);

    assert_eq!(synced.arrange, ArrangeOption::Normal);
    assert_eq!(synced.arrange_2p, ArrangeOption::Normal);
    assert_eq!(synced.session_mode, SessionMode::Normal);

    apply_current_play_options_to_profile(&mut profile, None, None, synced, 42);
    assert_eq!(profile.play.random, RandomOptionConfig::Off);
    assert_eq!(profile.play.random2, RandomOptionConfig::Off);
    assert!(!profile.play.auto_play);
}

fn classic_hsfix_session(hispeed: f32) -> (ProfileConfig, bmz_gameplay::session::GameSession) {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = BaseHispeedConfig::Classic;
    profile.lane.floating_policy = FloatingPolicyConfig::Disabled;
    profile.lane.hispeed = hispeed;
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = 300.0;
    let session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::StartBpm,
            ..crate::screens::play_session::PlaySessionOptions::default()
        },
    );
    (profile, session)
}

#[test]
fn classic_hsfix_hispeed_change_steps_bpm150_value() {
    let (_, mut session) = classic_hsfix_session(3.0);
    assert!((session.hispeed - 1.5).abs() < 0.000_01, "hispeed={}", session.hispeed);

    assert!(apply_play_option_control_to_session(
        &mut session,
        PlayOptionControl::Hispeed(HispeedChange::Up),
        false,
        0.25,
    ));

    assert!((session.hispeed - 1.625).abs() < 0.000_01, "hispeed={}", session.hispeed);
    assert_eq!(crate::screens::play_snapshot::display_hispeed_for_session(&session), 3.25);
}

#[test]
fn classic_hsfix_profile_hispeed_edit_and_save_use_bpm150_value() {
    let (mut profile, mut session) = classic_hsfix_session(3.0);
    let before = profile.lane.clone();
    let mut edited = profile.lane.clone();
    edited.hispeed = 4.0;

    assert!(apply_profile_lane_settings_to_session(
        &mut session,
        &before,
        profile.play.lane_effect,
        &edited,
        profile.play.lane_effect,
        false,
        false,
    ));
    assert!((session.hispeed - 2.0).abs() < 0.000_01, "hispeed={}", session.hispeed);

    apply_lane_state_to_profile(
        &mut profile,
        Some(crate::screens::play_snapshot::display_hispeed_for_session(&session)),
        Some(active_lane_state_for_session(&session)),
    );
    assert_eq!(profile.lane.hispeed, 4.0);
}

#[test]
fn pending_classic_hsfix_hispeed_tracks_chart_base_bpm() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut lane = PendingPlayLaneState {
        hispeed: 1.5,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Disabled,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: false,
        lift_enabled: false,
        hidden_enabled: false,
        lane_cover_visible: false,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: false,
        hsfix_base_bpm: 300.0,
        classic_hsfix_scale: 0.5,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };

    assert!(apply_pending_play_lane_action_to_state(
        &mut lane,
        PlayLaneAction::Hispeed(HispeedChange::Up),
        &profile,
        300.0,
        false,
    ));
    assert!((lane.hispeed - 1.625).abs() < 0.000_01, "hispeed={}", lane.hispeed);
    assert_eq!(lane.display_hispeed(), 3.25);

    let snapshot = bmz_render::snapshot::RenderSnapshot { max_bpm: 150.0, ..Default::default() };
    lane.sync_chart_bpm(&snapshot, HsFixOption::MaxBpm);
    assert!((lane.hispeed - 3.25).abs() < 0.000_01, "hispeed={}", lane.hispeed);
    assert_eq!(lane.display_hispeed(), 3.25);
}
