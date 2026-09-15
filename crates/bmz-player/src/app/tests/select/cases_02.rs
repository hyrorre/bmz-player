use super::*;

#[test]
fn select_control_action_does_not_hardcode_button2_as_back() {
    let mut input = crate::config::play_input::default_profile_input();
    let play7 = input.play.get_mut(KeyMode::K7.play_map_key()).expect("7K bindings");
    for entry in &mut play7.bindings {
        if entry.device == "gamepad" && entry.control == "Button2" {
            entry.lane = Some(LaneConfig::Key3);
        }
    }
    let keys = SelectKeyBindings::from_profile(&input);

    assert!(keys.is_enter("Button2"));
    assert_eq!(select_control_action("Button2", &keys), Some(SelectAction::EnterOrPlay));
    assert_eq!(select_control_action("Button1", &keys), Some(SelectAction::EnterOrPlay));
}

#[test]
fn key9_select_input_maps_configured_lane_keys() {
    let keys = select_keys_9k();

    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyF), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Next))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyD), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Previous))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyC), ElementState::Pressed, false, &keys),
        Some(SelectAction::EnterOrPlay)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyV), ElementState::Pressed, false, &keys),
        Some(SelectAction::EnterOrPlay)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyX), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
    assert_eq!(target_cycle_from_control("G", &keys), Some(TargetCycle::Next));
    assert_eq!(target_cycle_from_control("B", &keys), Some(TargetCycle::Previous));
}

#[test]
fn select_action_rejects_releases_repeats_and_other_keys() {
    let keys = default_select_keys();
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Released, false, &keys),
        None
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Pressed, true, &keys),
        None
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyA), ElementState::Pressed, false, &keys),
        None
    );
}

#[test]
fn select_wheel_move_maps_vertical_scroll_to_selection_movement() {
    assert_eq!(
        select_wheel_move(MouseScrollDelta::LineDelta(0.0, 1.0)),
        Some(SelectMove::Previous)
    );
    assert_eq!(select_wheel_move(MouseScrollDelta::LineDelta(0.0, -1.0)), Some(SelectMove::Next));
    assert_eq!(select_wheel_move(MouseScrollDelta::LineDelta(3.0, 0.0)), None);
}

#[test]
fn select_wheel_move_supports_pixel_delta() {
    assert_eq!(
        select_wheel_move(MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
            0.0, 12.0
        ))),
        Some(SelectMove::Previous)
    );
    assert_eq!(
        select_wheel_move(MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
            0.0, -12.0
        ))),
        Some(SelectMove::Next)
    );
}

#[test]
fn lane_cover_wheel_change_maps_vertical_scroll() {
    assert_eq!(
        lane_cover_wheel_change(MouseScrollDelta::LineDelta(0.0, 1.0)),
        Some(LaneCoverChange::Up)
    );
    assert_eq!(
        lane_cover_wheel_change(MouseScrollDelta::LineDelta(0.0, -1.0)),
        Some(LaneCoverChange::Down)
    );
    assert_eq!(lane_cover_wheel_change(MouseScrollDelta::LineDelta(1.0, 0.0)), None);
}

#[test]
fn select_click_event_arg_matches_beatoraja_click_types() {
    let rect = Rect { x: 0.2, y: 0.3, width: 0.4, height: 0.2 };
    assert_eq!(select_click_event_arg(0, MouseButton::Left, rect, 0.3, 0.4), Some(1));
    assert_eq!(select_click_event_arg(0, MouseButton::Right, rect, 0.3, 0.4), Some(-1));
    assert_eq!(select_click_event_arg(1, MouseButton::Right, rect, 0.3, 0.4), Some(1));
    assert_eq!(select_click_event_arg(2, MouseButton::Left, rect, 0.39, 0.4), Some(-1));
    assert_eq!(select_click_event_arg(2, MouseButton::Left, rect, 0.41, 0.4), Some(1));
    assert_eq!(select_click_event_arg(3, MouseButton::Left, rect, 0.3, 0.39), Some(1));
    assert_eq!(select_click_event_arg(3, MouseButton::Left, rect, 0.3, 0.41), Some(-1));
    assert_eq!(select_click_event_arg(4, MouseButton::Left, rect, 0.3, 0.4), None);
}

#[test]
fn select_key_bindings_builds_correct_hints() {
    let keys = default_select_keys();
    assert!(keys.key_hint().contains("Z/X/C/V"), "enter keys in hint: {}", keys.key_hint());
    assert!(keys.key_hint().contains("/S/D/F:BACK"), "back keys in hint: {}", keys.key_hint());
    assert!(keys.key_hint().contains(" Q"), "start key in hint: {}", keys.key_hint());
    assert!(keys.option_hint().contains("F1 MENU"), "menu in hint: {}", keys.option_hint());
    assert!(keys.option_hint().contains("F5:RELOAD"), "reload in hint: {}", keys.option_hint());
    assert!(
        keys.option_hint().contains("Q+K1/K2:1P ARR"),
        "1P arrange in hint: {}",
        keys.option_hint()
    );
    assert!(
        keys.option_hint().contains("Q+2P K1/K2:2P ARR"),
        "2P arrange in hint: {}",
        keys.option_hint()
    );
    assert!(keys.option_hint().contains("Q+K5:HS-FIX"), "HS-FIX in hint: {}", keys.option_hint());
    assert!(
        keys.option_hint().contains("Q+K6:DP OPT"),
        "DP option in hint: {}",
        keys.option_hint()
    );
    assert!(
        keys.option_hint().contains("Q+UP/DOWN:TARGET"),
        "target in hint: {}",
        keys.option_hint()
    );
}

#[test]
fn select_option_panel_maps_start_and_select_holds() {
    assert_eq!(select_option_panel_for_holds(false, false), 0);
    assert_eq!(select_option_panel_for_holds(true, false), 1);
    assert_eq!(select_option_panel_for_holds(false, true), 2);
    assert_eq!(select_option_panel_for_holds(true, true), 3);
}

#[test]
fn select_option_panel_transition_plays_open_and_close_sounds() {
    use crate::system_sound::SoundType;

    assert_eq!(select_option_panel_sound_for_transition(0, 1), Some(SoundType::OptionOpen));
    assert_eq!(select_option_panel_sound_for_transition(3, 0), Some(SoundType::OptionClose));
    assert_eq!(select_option_panel_sound_for_transition(1, 2), None);
    assert_eq!(select_option_panel_sound_for_transition(2, 3), None);
    assert_eq!(select_option_panel_sound_for_transition(0, 0), None);

    assert_eq!(
        select_option_panel_sound_for_scene_transition(AppSceneKind::Select, 0, 1),
        Some(SoundType::OptionOpen)
    );
    for scene in [AppSceneKind::Decide, AppSceneKind::Play, AppSceneKind::Result] {
        assert_eq!(select_option_panel_sound_for_scene_transition(scene, 0, 1), None);
        assert_eq!(select_option_panel_sound_for_scene_transition(scene, 1, 0), None);
    }
}

#[test]
fn select_option_panel_transition_tracks_independent_off_timers() {
    let base = Instant::now();
    let mut current = 1;
    let mut on_started_at = base;
    let mut off_started_at = [None; 6];

    assert!(transition_select_option_panel(
        &mut current,
        &mut on_started_at,
        &mut off_started_at,
        2,
        base + Duration::from_millis(100),
    ));
    assert_eq!(current, 2);
    assert_eq!(off_started_at[0], Some(base + Duration::from_millis(100)));
    assert_eq!(off_started_at[1], None);

    assert!(transition_select_option_panel(
        &mut current,
        &mut on_started_at,
        &mut off_started_at,
        0,
        base + Duration::from_millis(200),
    ));
    assert_eq!(off_started_at[0], Some(base + Duration::from_millis(100)));
    assert_eq!(off_started_at[1], Some(base + Duration::from_millis(200)));

    assert!(transition_select_option_panel(
        &mut current,
        &mut on_started_at,
        &mut off_started_at,
        1,
        base + Duration::from_millis(300),
    ));
    assert_eq!(off_started_at[0], None);
    assert_eq!(off_started_at[1], Some(base + Duration::from_millis(200)));
    assert!(!transition_select_option_panel(
        &mut current,
        &mut on_started_at,
        &mut off_started_at,
        1,
        base + Duration::from_millis(400),
    ));
}

#[test]
fn shift_detail_panel_survives_other_keys_and_closes_after_reconciliation() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    for entry in &mut profile.input.ui.bindings {
        if entry.device == "keyboard" {
            match entry.action {
                Some(InputActionConfig::E1) => entry.control = "LShift".into(),
                Some(InputActionConfig::E2) => entry.control = "RShift".into(),
                _ => {}
            }
        }
    }
    let keys = SelectKeyBindings::from_profile(&profile.input);
    let mut runtime = AppInputRuntime::default();
    for (key, repeat) in
        [(KeyCode::ShiftLeft, false), (KeyCode::ShiftRight, true), (KeyCode::KeyD, false)]
    {
        runtime.track_control(&ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(key),
            ElementState::Pressed,
            repeat,
        ));
    }
    let panel = |runtime: &AppInputRuntime| {
        let (start, select, _) =
            select_hold_state_from_pressed_controls(&runtime.pressed_controls, &keys);
        select_option_panel_for_holds(start, select)
    };
    assert_eq!(panel(&runtime), 3);
    runtime.reconcile_keyboard_releases(&[PhysicalKey::Code(KeyCode::ShiftLeft)]);
    assert_eq!(panel(&runtime), 2);
    runtime.reconcile_keyboard_releases(&[PhysicalKey::Code(KeyCode::ShiftRight)]);
    assert_eq!(panel(&runtime), 0);
}

#[test]
fn select_hold_state_rebuilds_from_pressed_controls() {
    let keys = default_select_keys();
    let pressed = HashSet::from(["Q".to_string(), "W".to_string()]);

    let (start_held, select_held, e_action_holds) =
        select_hold_state_from_pressed_controls(&pressed, &keys);

    assert!(start_held);
    assert!(select_held);
    assert!(e_action_holds.contains(&InputActionConfig::E1));
    assert!(e_action_holds.contains(&InputActionConfig::E2));

    let pressed = HashSet::from(["W".to_string()]);
    let (start_held, select_held, e_action_holds) =
        select_hold_state_from_pressed_controls(&pressed, &keys);

    assert!(!start_held);
    assert!(select_held);
    assert!(!e_action_holds.contains(&InputActionConfig::E1));
    assert!(e_action_holds.contains(&InputActionConfig::E2));
}

#[test]
fn select_analog_scroll_delta_maps_scratch_bindings() {
    let gamepad_keys =
        SelectKeyBindings::from_profile(&ProfileConfig::new_default("default", "Default", 1).input);
    // Axis1+ = scratch up (Previous = 負), Axis1- = scratch down (Next = 正)
    assert_eq!(select_analog_scroll_delta("Axis1", 4, &gamepad_keys), Some(-4));
    assert_eq!(select_analog_scroll_delta("Axis1", -4, &gamepad_keys), Some(4));
    assert_eq!(select_analog_scroll_delta("Axis2", -4, &gamepad_keys), None);
    assert_eq!(select_analog_scroll_delta("Axis1", 0, &gamepad_keys), None);
    assert_eq!(select_analog_scroll_delta("Axis3", 4, &gamepad_keys), None);
}

#[test]
fn settings_edit_analog_scroll_uses_scratch_direction() {
    assert_eq!(settings_edit_direction_from_analog_scroll(3), 1);
    assert_eq!(settings_edit_direction_from_analog_scroll(-2), -1);
    assert_eq!(settings_edit_direction_from_analog_scroll(0), 0);
}

#[test]
fn settings_edit_mouse_wheel_uses_scroll_direction() {
    assert_eq!(settings_edit_direction_from_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0)), 1);
    assert_eq!(
        settings_edit_direction_from_mouse_wheel(MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -12.0)
        )),
        -1
    );
}

#[test]
fn update_analog_scroll_buffer_suppresses_until_idle() {
    let mut buffer = 0;
    let mut suppress = true;
    // 回転継続中 (idle=false) は捨て続ける
    update_analog_scroll_buffer(&mut buffer, &mut suppress, false, 5);
    assert_eq!(buffer, 0);
    assert!(suppress);
    // 一度止まった後の tick から蓄積再開
    update_analog_scroll_buffer(&mut buffer, &mut suppress, true, 2);
    assert_eq!(buffer, 2);
    assert!(!suppress);
    update_analog_scroll_buffer(&mut buffer, &mut suppress, false, 3);
    assert_eq!(buffer, 5);
    // 通常時も idle で端数を破棄
    update_analog_scroll_buffer(&mut buffer, &mut suppress, true, 1);
    assert_eq!(buffer, 1);
}
