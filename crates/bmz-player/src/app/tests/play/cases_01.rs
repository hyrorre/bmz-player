use super::*;

#[test]
fn smoke_play_frame_counter_only_exits_at_the_requested_count() {
    assert_eq!(count_smoke_play_frame(0, 3), (1, false));
    assert_eq!(count_smoke_play_frame(2, 3), (3, true));
    assert_eq!(count_smoke_play_frame(u32::MAX, 1), (u32::MAX, true));
}

#[test]
fn player_name_and_fps_are_applied_to_every_scene() {
    let mut scenes = [
        AppSceneSnapshot::Select(SelectSnapshot::default()),
        AppSceneSnapshot::Play(RenderSnapshot::default()),
        bmz_render::sample::sample_result_scene(),
    ];

    for scene in &mut scenes {
        apply_skin_runtime_info_to_scene(scene, "Test Player", 237);
        match scene {
            AppSceneSnapshot::Select(snapshot) => {
                assert_eq!(snapshot.player_name, "Test Player");
                assert_eq!(snapshot.current_fps, 237);
            }
            AppSceneSnapshot::Decide(snapshot) | AppSceneSnapshot::Play(snapshot) => {
                assert_eq!(snapshot.player_name, "Test Player");
                assert_eq!(snapshot.current_fps, 237);
            }
            AppSceneSnapshot::Result(snapshot) => {
                assert_eq!(snapshot.player_name, "Test Player");
                assert_eq!(snapshot.current_fps, 237);
            }
        }
    }
}

#[test]
fn active_play_visual_offset_sync_preserves_auto_adjusted_value() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);

    sync_active_play_visual_offset_to_profile(&mut profile, 1_000, true);

    assert_eq!(profile.judge.visual_offset_us, 1_000);
    assert_eq!(crate::config::play::play_offsets_from_profile(&profile).visual_offset_us, 1_000);

    sync_active_play_visual_offset_to_profile(&mut profile, 2_000, false);
    assert_eq!(profile.judge.visual_offset_us, 1_000);
}

#[test]
fn pending_play_uses_preload_input_before_session_install() {
    use bmz_core::input::InputKind;
    use bmz_gameplay::input::backend::InputBackend;

    let preload_input = SharedInputBackend::default();
    assert!(play_input_backend_for_context(None, false, None, Some(&preload_input)).is_none());

    let selected = play_input_backend_for_context(None, true, None, Some(&preload_input)).unwrap();
    crate::input::winit::handle_key_parts(
        &selected,
        PhysicalKey::Code(KeyCode::KeyZ),
        ElementState::Pressed,
        false,
    );

    let events = preload_input.clone().drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, InputKind::Press);
}

#[test]
fn pending_play_input_updates_keybeam_before_session_install() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let binding = crate::config::play::lane_binding_for_chart_with_slots(
        &profile.input,
        KeyMode::K7,
        Default::default(),
    );
    let mut visual = PendingPlayVisualInput::new(KeyMode::K7, binding, false);
    let press = physical_key_to_device_input(
        PhysicalKey::Code(KeyCode::KeyZ),
        ElementState::Pressed,
        false,
    )
    .unwrap();

    visual.apply_event(&press, TimeUs(100_000));
    let mut snapshot = RenderSnapshot::default();
    crate::screens::play_snapshot::refresh_pending_play_input_visuals(
        &mut snapshot,
        visual.key_mode,
        visual.lane_keyon_started_at,
        visual.lane_keyoff_started_at,
        visual.lane_scratch_angle_delta_ms,
        TimeUs(150_000),
    );

    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], Some(50));
    assert_eq!(snapshot.keyoff_ms[Lane::Key1.index()], None);

    let release = physical_key_to_device_input(
        PhysicalKey::Code(KeyCode::KeyZ),
        ElementState::Released,
        false,
    )
    .unwrap();
    visual.apply_event(&release, TimeUs(160_000));
    crate::screens::play_snapshot::refresh_pending_play_input_visuals(
        &mut snapshot,
        visual.key_mode,
        visual.lane_keyon_started_at,
        visual.lane_keyoff_started_at,
        visual.lane_scratch_angle_delta_ms,
        TimeUs(175_000),
    );
    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], None);
    assert_eq!(snapshot.keyoff_ms[Lane::Key1.index()], Some(15));
}

#[test]
fn pending_play_input_state_hands_off_without_resetting_keybeam_timer() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let binding = crate::config::play::lane_binding_for_chart_with_slots(
        &profile.input,
        KeyMode::K7,
        Default::default(),
    );
    let mut visual = PendingPlayVisualInput::new(KeyMode::K7, binding, false);
    let press = physical_key_to_device_input(
        PhysicalKey::Code(KeyCode::KeyZ),
        ElementState::Pressed,
        false,
    )
    .unwrap();
    visual.apply_event(&press, TimeUs(100_000));
    let input = SharedInputBackend::default();
    input.push_shared_event(press);
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(app_test_chart()),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
    );

    handoff_pending_play_visual_input(&mut session, &input, &visual);
    let mut snapshot = RenderSnapshot { play_elapsed_time: TimeUs(150_000), ..Default::default() };
    crate::screens::play_snapshot::refresh_play_skin_visuals_with_input_elapsed(
        &mut snapshot,
        &session,
        TimeUs(150_000),
    );

    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(100_000)));
    assert_eq!(snapshot.keyon_ms[Lane::Key1.index()], Some(50));
    assert!(input.clone().drain_events().is_empty());
}

#[test]
fn pending_play_input_suppresses_human_keybeam_for_full_autoplay() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let binding = crate::config::play::lane_binding_for_chart_with_slots(
        &profile.input,
        KeyMode::K7,
        Default::default(),
    );
    let mut visual = PendingPlayVisualInput::new(KeyMode::K7, binding, true);
    let press = physical_key_to_device_input(
        PhysicalKey::Code(KeyCode::KeyZ),
        ElementState::Pressed,
        false,
    )
    .unwrap();

    visual.apply_event(&press, TimeUs(100_000));

    assert_eq!(visual.lane_keyon_started_at[Lane::Key1.index()], None);
}

#[test]
fn play_control_hold_state_rebuilds_from_pressed_controls() {
    let input = crate::config::play_input::default_profile_input();
    let play_input = play_option_input_for(&input, KeyMode::K7);
    let keyboard =
        |control: &str| (W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey(control.to_string()));
    let pressed = HashSet::from([keyboard("Q"), keyboard("W"), keyboard("E")]);

    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&pressed, &play_input),
        (true, true, true)
    );

    let pressed = HashSet::from([keyboard("Q")]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&pressed, &play_input),
        (true, false, false)
    );

    let pressed = HashSet::from([keyboard("W")]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&pressed, &play_input),
        (false, true, false)
    );
}

#[test]
fn autoplay_replay_speed_keys_use_top_row_hold_priority() {
    let keyboard =
        |control: &str| (W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey(control.to_string()));
    let input = crate::config::play_input::default_profile_input();
    let play_input = play_option_input_for(&input, KeyMode::K7);
    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(&HashSet::new(), Some(&play_input)),
        100
    );
    for (control, expected) in [
        ("1", 25),
        ("Numpad1", 25),
        ("2", 50),
        ("Numpad2", 50),
        ("3", 200),
        ("Numpad3", 200),
        ("4", 300),
        ("Numpad4", 300),
    ] {
        assert_eq!(
            autoplay_replay_playback_rate_from_pressed_inputs(
                &HashSet::from([keyboard(control)]),
                Some(&play_input),
            ),
            expected
        );
    }
    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(
            &HashSet::from([keyboard("4"), keyboard("2"),]),
            Some(&play_input),
        ),
        50
    );
    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(
            &HashSet::from([(DeviceId(7), PhysicalControl::GamepadButton("1".to_string()),)]),
            Some(&play_input),
        ),
        100
    );
}

#[test]
fn autoplay_replay_speed_keys_defer_to_current_play_bindings() {
    let keyboard =
        |control: &str| (W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey(control.to_string()));
    let mut input = crate::config::play_input::default_profile_input();
    apply_play_binding(
        &mut input,
        KeyMode::K7,
        KeyBindingTarget::Key { lane: LaneConfig::Key1, slot: KeyBindingSlot::KeyboardPrimary },
        "1",
    )
    .unwrap();
    apply_play_binding(
        &mut input,
        KeyMode::K7,
        KeyBindingTarget::Action {
            action: InputActionConfig::E4,
            slot: KeyBindingSlot::KeyboardPrimary,
        },
        "3",
    )
    .unwrap();
    let play_input = play_option_input_for(&input, KeyMode::K7);

    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(
            &HashSet::from([keyboard("1")]),
            Some(&play_input),
        ),
        100
    );
    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(
            &HashSet::from([keyboard("3")]),
            Some(&play_input),
        ),
        100
    );
    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(
            &HashSet::from([keyboard("1"), keyboard("2")]),
            Some(&play_input),
        ),
        50
    );
    assert!(!is_unassigned_autoplay_replay_playback_rate_key(
        PhysicalKey::Code(KeyCode::Digit1),
        Some(&play_input),
    ));
    assert!(is_unassigned_autoplay_replay_playback_rate_key(
        PhysicalKey::Code(KeyCode::Numpad1),
        Some(&play_input),
    ));
    assert!(is_unassigned_autoplay_replay_playback_rate_key(
        PhysicalKey::Code(KeyCode::Digit2),
        Some(&play_input),
    ));
    assert!(is_unassigned_autoplay_replay_playback_rate_key(
        PhysicalKey::Code(KeyCode::Numpad2),
        Some(&play_input),
    ));

    apply_play_binding(
        &mut input,
        KeyMode::K7,
        KeyBindingTarget::Key { lane: LaneConfig::Key1, slot: KeyBindingSlot::KeyboardSecondary },
        "Numpad1",
    )
    .unwrap();
    let play_input = play_option_input_for(&input, KeyMode::K7);
    assert_eq!(
        autoplay_replay_playback_rate_from_pressed_inputs(
            &HashSet::from([keyboard("Numpad1")]),
            Some(&play_input),
        ),
        100
    );
    assert!(!is_unassigned_autoplay_replay_playback_rate_key(
        PhysicalKey::Code(KeyCode::Numpad1),
        Some(&play_input),
    ));
}

#[test]
fn autoplay_replay_speed_keys_override_remapped_play_shortcuts() {
    for &action in crate::config::profile_config::PLAY_KEYBOARD_SHORTCUT_ACTIONS {
        for slot in [KeyBindingSlot::KeyboardPrimary, KeyBindingSlot::KeyboardSecondary] {
            for (control, key, rate) in [
                ("1", KeyCode::Digit1, 25),
                ("2", KeyCode::Digit2, 50),
                ("3", KeyCode::Digit3, 200),
                ("4", KeyCode::Digit4, 300),
            ] {
                let mut input = crate::config::play_input::default_profile_input();
                let target = KeyBindingTarget::Action { action, slot };
                apply_play_binding(&mut input, KeyMode::K7, target, control).unwrap();
                let pressed = HashSet::from([(
                    W_KEYBOARD_DEVICE_ID,
                    PhysicalControl::KeyboardKey(control.to_string()),
                )]);
                let play_input = play_option_input_for(&input, KeyMode::K7);
                assert!(is_unassigned_autoplay_replay_playback_rate_key(
                    PhysicalKey::Code(key),
                    Some(&play_input),
                ));
                assert_eq!(
                    autoplay_replay_playback_rate_from_pressed_inputs(&pressed, Some(&play_input)),
                    rate,
                );
                assert!(
                    keyboard_lane_action(
                        &ControlInputEvent::keyboard_parts(
                            PhysicalKey::Code(key),
                            ElementState::Pressed,
                            false,
                        ),
                        &input,
                    )
                    .is_some()
                );

                clear_play_binding(&mut input, KeyMode::K7, target).unwrap();
                let play_input = play_option_input_for(&input, KeyMode::K7);
                assert!(is_unassigned_autoplay_replay_playback_rate_key(
                    PhysicalKey::Code(key),
                    Some(&play_input),
                ));
                assert_eq!(
                    autoplay_replay_playback_rate_from_pressed_inputs(&pressed, Some(&play_input)),
                    rate,
                );
            }
        }
    }
}

#[test]
fn play_control_hold_state_keeps_legacy_and_default_e1_fallbacks() {
    let mut legacy_input = crate::config::play_input::default_profile_input();
    legacy_input.ui.bindings.retain(|entry| entry.action != Some(InputActionConfig::E1));
    legacy_input.start_key = Some("E".to_string());
    let legacy_play_input = play_option_input_for(&legacy_input, KeyMode::K7);
    let legacy_pressed =
        HashSet::from([(W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey("E".to_string()))]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&legacy_pressed, &legacy_play_input),
        (true, false, true)
    );

    legacy_input.start_key = None;
    let fallback_play_input = play_option_input_for(&legacy_input, KeyMode::K7);
    let fallback_pressed =
        HashSet::from([(W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey("Q".to_string()))]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&fallback_pressed, &fallback_play_input),
        (true, false, false)
    );
}

#[test]
fn egui_keyboard_routing_switches_to_play_only_for_e1_e2_controls_and_holds() {
    let input = crate::config::play_input::default_profile_input();
    let mut play_input = play_option_input_for(&input, KeyMode::K7);
    let keyboard = |control: &str| PhysicalControl::KeyboardKey(control.to_string());

    assert!(keyboard_input_bypasses_egui(
        true,
        false,
        false,
        false,
        Some(&keyboard("Q")),
        Some(&play_input),
    ));
    assert!(keyboard_input_bypasses_egui(
        true,
        false,
        false,
        false,
        Some(&keyboard("W")),
        Some(&play_input),
    ));
    assert!(!keyboard_input_bypasses_egui(
        true,
        false,
        false,
        false,
        Some(&keyboard("ArrowLeft")),
        Some(&play_input),
    ));
    assert!(keyboard_input_bypasses_egui(
        true,
        true,
        false,
        false,
        Some(&keyboard("ArrowLeft")),
        Some(&play_input),
    ));
    assert!(keyboard_input_bypasses_egui(
        true,
        false,
        true,
        false,
        Some(&keyboard("ArrowUp")),
        Some(&play_input),
    ));
    assert!(!keyboard_input_bypasses_egui(
        false,
        false,
        false,
        false,
        Some(&keyboard("Q")),
        Some(&play_input),
    ));
    assert!(keyboard_input_bypasses_egui(
        false,
        false,
        false,
        true,
        Some(&keyboard("ArrowLeft")),
        Some(&play_input),
    ));

    play_input.binding.entries.push(bmz_gameplay::input::binding::BindingEntry {
        device: Some(W_KEYBOARD_DEVICE_ID),
        control: keyboard("Q"),
        lane: Lane::Key1,
        scratch_direction: None,
    });
    assert!(!keyboard_input_bypasses_egui(
        true,
        false,
        false,
        false,
        Some(&keyboard("Q")),
        Some(&play_input),
    ));
}

#[test]
fn raw_keyboard_is_blocked_only_by_practice_overlay_without_e1_e2_hold() {
    assert!(egui_blocks_raw_play_keyboard(true, false, false));
    assert!(!egui_blocks_raw_play_keyboard(true, true, false));
    assert!(!egui_blocks_raw_play_keyboard(true, false, true));
    assert!(!egui_blocks_raw_play_keyboard(false, false, false));
}

#[test]
fn window_keyboard_capture_is_exclusive_only_in_practice() {
    assert!(!egui_blocks_window_keyboard_route(true, false, false, true));
    assert!(egui_blocks_window_keyboard_route(true, true, false, false));
    assert!(egui_blocks_window_keyboard_route(true, true, false, true));
    assert!(!egui_blocks_window_keyboard_route(true, true, true, true));
}

#[test]
fn non_play_keyboard_is_blocked_only_when_egui_consumes_it() {
    assert!(!egui_blocks_window_keyboard_route(false, false, false, false));
    assert!(egui_blocks_window_keyboard_route(false, false, false, true));
}

#[test]
fn play_ready_is_blocked_while_e1_or_e2_is_held() {
    assert!(!play_ready_blocked_by_control_holds(false, false));
    assert!(play_ready_blocked_by_control_holds(true, false));
    assert!(play_ready_blocked_by_control_holds(false, true));
    assert!(play_ready_blocked_by_control_holds(true, true));
}

#[test]
fn quick_retry_input_is_routed_only_during_play_ending() {
    assert!(should_route_quick_retry_input(true, false, true));
    assert!(!should_route_quick_retry_input(true, false, false));
    assert!(!should_route_quick_retry_input(false, false, true));
    assert!(!should_route_quick_retry_input(true, true, true));
}

#[test]
fn play_ready_waits_one_second_after_last_e1_or_e2_hold() {
    let last_control_hold_at = Instant::now();

    assert!(play_ready_blocked_by_recent_control_hold(
        Some(last_control_hold_at),
        last_control_hold_at + Duration::from_millis(999)
    ));
    assert!(play_ready_blocked_by_recent_control_hold(
        Some(last_control_hold_at),
        last_control_hold_at + Duration::from_secs(1)
    ));
    assert!(!play_ready_blocked_by_recent_control_hold(
        Some(last_control_hold_at),
        last_control_hold_at + Duration::from_millis(1_001)
    ));
}

#[test]
fn play_ready_has_no_release_delay_without_prior_control_hold() {
    assert!(!play_ready_blocked_by_recent_control_hold(None, Instant::now()));
}

#[test]
fn practice_config_play_exit_uses_the_same_leave_path_as_escape() {
    assert!(play_exit_should_leave_practice(Some(PracticePhase::Config)));
    assert!(!play_exit_should_leave_practice(Some(PracticePhase::Playing)));
    assert!(!play_exit_should_leave_practice(None));
}

#[test]
fn play_analog_lane_cover_delta_maps_scratch_bindings() {
    let gamepad_keys =
        SelectKeyBindings::from_profile(&ProfileConfig::new_default("default", "Default", 1).input);

    assert_eq!(play_analog_lane_cover_delta("Axis1", 4, &gamepad_keys), Some(-4));
    assert_eq!(play_analog_lane_cover_delta("Axis1", -4, &gamepad_keys), Some(4));
    assert_eq!(play_analog_lane_cover_delta("Axis2", -4, &gamepad_keys), None);
    assert_eq!(play_analog_lane_cover_delta("Axis1", 0, &gamepad_keys), None);
}

#[test]
fn play_analog_green_number_uses_opposite_direction_from_lane_cover() {
    assert_eq!(green_number_change_from_analog_steps(1), GreenNumberChange::Up);
    assert_eq!(green_number_change_from_analog_steps(-1), GreenNumberChange::Down);
}

#[test]
fn play_exit_hold_timer_uses_beatoraja_default_duration() {
    let default_hold = Duration::from_millis(1_000);
    let start = Instant::now();
    let mut held_since = None;

    update_play_exit_hold_started_at(&mut held_since, true, false, start);
    assert!(held_since.is_none());

    update_play_exit_hold_started_at(&mut held_since, true, true, start);
    assert_eq!(held_since, Some(start));
    assert!(!play_exit_hold_elapsed(held_since, start + default_hold / 2, default_hold));
    assert!(play_exit_hold_elapsed(held_since, start + default_hold, default_hold));

    update_play_exit_hold_started_at(&mut held_since, false, true, start + default_hold);
    assert!(held_since.is_none());
}

#[test]
fn play_exit_chord_requires_e2_and_e3() {
    assert!(play_exit_chord_pressed(true, true));
    assert!(!play_exit_chord_pressed(true, false));
    assert!(!play_exit_chord_pressed(false, true));
    assert!(!play_exit_chord_pressed(false, false));
}

#[test]
fn decide_control_action_skips_with_1p_and_2p_decide_keys() {
    let keys = select_keys_with_full_2p_bindings();

    assert_eq!(decide_control_action("Z", &keys), Some(DecideAction::Confirm));
    assert_eq!(decide_control_action("M", &keys), Some(DecideAction::Confirm));
    assert_eq!(decide_control_action("P2K7", &keys), Some(DecideAction::Confirm));
    assert_eq!(decide_control_action("S", &keys), None);
    assert_eq!(decide_control_action("P2K6", &keys), None);
}

#[test]
fn play_control_hold_state_treats_e4_as_e1() {
    let input = crate::config::play_input::default_profile_input();
    let play_input = play_option_input_for(&input, KeyMode::K7);
    let keyboard =
        |control: &str| (W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey(control.to_string()));

    // 既定のE4はR。
    let pressed = HashSet::from([keyboard("R")]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&pressed, &play_input),
        (true, false, false)
    );
    let pressed = HashSet::from([keyboard("R"), keyboard("W")]);
    assert_eq!(
        play_control_hold_state_from_pressed_inputs(&pressed, &play_input),
        (true, true, false)
    );
    assert!(play_input.is_action(
        W_KEYBOARD_DEVICE_ID,
        &PhysicalControl::KeyboardKey("R".to_string()),
        InputActionConfig::E1,
    ));
    assert!(!play_input.is_action(
        W_KEYBOARD_DEVICE_ID,
        &PhysicalControl::KeyboardKey("Q".to_string()),
        InputActionConfig::E4,
    ));
}

#[test]
fn play_option_control_toggles_floating_with_e4_and_e2() {
    let input = crate::config::play_input::default_profile_input();
    let keys = SelectKeyBindings::from_profile(&input);
    let play_input = play_option_input_for(&input, KeyMode::K7);

    // E2 hold中にE4、E4 hold中にE2のどちらでも切替。
    assert_eq!(
        keyboard_play_option("R", false, true, &keys, &play_input, &input),
        Some(PlayOptionControl::ToggleHispeedMode)
    );
    assert_eq!(
        keyboard_play_option("W", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::ToggleHispeedMode)
    );
}

#[test]
fn play_skin_input_mirrors_e4_to_e1_only_in_play() {
    let mut skin_input = SkinLogicalInputSnapshot::default();
    skin_input.held[3] = true;

    let mut play = AppSceneSnapshot::Play(Default::default());
    apply_skin_logical_input_to_scene(&mut play, skin_input);
    let AppSceneSnapshot::Play(play) = play else { unreachable!() };
    assert!(play.skin_input.held[0]);
    assert!(play.skin_input.held[3]);

    let mut decide = AppSceneSnapshot::Decide(Default::default());
    apply_skin_logical_input_to_scene(&mut decide, skin_input);
    let AppSceneSnapshot::Decide(decide) = decide else { unreachable!() };
    assert!(!decide.skin_input.held[0]);
}

#[test]
fn play_e1_hold_survives_releasing_one_of_e1_and_e4() {
    let input = crate::config::play_input::default_profile_input();
    let play_input = play_option_input_for(&input, KeyMode::K7);
    let key = |control: &str| PhysicalControl::KeyboardKey(control.to_string());
    let both = HashSet::from([(W_KEYBOARD_DEVICE_ID, key("Q")), (W_KEYBOARD_DEVICE_ID, key("R"))]);
    let only_e4 = HashSet::from([(W_KEYBOARD_DEVICE_ID, key("R"))]);

    // Qを離してもRを押していればE1 holdを維持する (解放前後どちらの集合でも)。
    for pressed in [&both, &only_e4] {
        assert_eq!(
            play_e1_held_after_input(pressed, &play_input, W_KEYBOARD_DEVICE_ID, &key("Q"), false),
            Some(true)
        );
    }
    assert_eq!(
        play_e1_held_after_input(
            &HashSet::new(),
            &play_input,
            W_KEYBOARD_DEVICE_ID,
            &key("R"),
            false
        ),
        Some(false)
    );
    assert_eq!(
        play_e1_held_after_input(&both, &play_input, W_KEYBOARD_DEVICE_ID, &key("W"), true),
        None
    );
}
