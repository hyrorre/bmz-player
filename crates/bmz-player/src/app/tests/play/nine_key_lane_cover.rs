use super::*;

fn remapped_input() -> ProfileInputConfig {
    use crate::config::key_config::{KeyBindingSlot, KeyBindingTarget, apply_play_binding};

    let mut input = crate::config::play_input::default_profile_input();
    input.lane_cover_step = 27;
    input.lane_cover_hold_step = 7;
    input.lane_cover_hold_delay_ms = 45;
    for (slot, up, down) in [
        (KeyBindingSlot::KeyboardPrimary, "H", "J"),
        (KeyBindingSlot::Controller, "Button2", "Button4"),
    ] {
        for (lane, control) in [(LaneConfig::Key8, up), (LaneConfig::Key9, down)] {
            apply_play_binding(
                &mut input,
                KeyMode::K9,
                KeyBindingTarget::Key { lane, slot },
                control,
            )
            .unwrap();
        }
    }
    input
}

#[test]
fn nine_key_cover_buttons_match_cursor_keys_with_normal_and_fine_steps() {
    let input = remapped_input();
    let play_input = play_option_input_for(&input, KeyMode::K9);
    for (device, controls) in [
        (
            W_KEYBOARD_DEVICE_ID,
            [PhysicalControl::KeyboardKey("H".into()), PhysicalControl::KeyboardKey("J".into())],
        ),
        (
            DeviceId(42),
            [
                PhysicalControl::GamepadButton("Button2".into()),
                PhysicalControl::GamepadButton("Button4".into()),
            ],
        ),
    ] {
        for (control, cursor) in controls.into_iter().zip([KeyCode::ArrowUp, KeyCode::ArrowDown]) {
            for fine in [false, true] {
                let option = play_option_control_for_input(
                    device,
                    &control,
                    !fine,
                    fine,
                    Some(&play_input),
                    &input,
                )
                .unwrap();
                let cursor = ControlInputEvent::keyboard_parts(
                    PhysicalKey::Code(cursor),
                    ElementState::Pressed,
                    false,
                );
                assert_eq!(
                    lane_action_from_option(option, false),
                    keyboard_lane_action_with_fine(&cursor, &input, fine),
                    "{device:?} {control:?} fine={fine}",
                );
            }
            for held in [false, true] {
                assert_eq!(
                    play_option_control_for_input(
                        device,
                        &control,
                        held,
                        held,
                        Some(&play_input),
                        &input,
                    ),
                    None,
                );
            }
        }
    }
}

#[test]
fn nine_key_cover_holds_match_cursor_repeats_and_stop_on_modifier_or_key_release() {
    let input = remapped_input();
    let start = Instant::now();
    let delay = Duration::from_millis(u64::from(input.lane_cover_hold_delay_ms));
    let tick = Duration::from_micros(LANE_COVER_HOLD_TICK_US);
    for fine in [false, true] {
        for direction in [-1, 1] {
            let control = (DeviceId(42), PhysicalControl::GamepadButton("Button2".into()));
            let mut hold = LaneCoverKeyHold::new(control.clone(), direction, fine, true, start);
            let mut cursor = LaneCoverKeyHold::new(
                (W_KEYBOARD_DEVICE_ID, PhysicalControl::KeyboardKey("ArrowUp".into())),
                direction,
                fine,
                false,
                start,
            );
            let pressed = HashSet::from([control.clone()]);
            assert!(hold.is_held(&pressed, !fine, fine));
            assert!(!hold.is_held(&pressed, false, false));
            assert!(!hold.is_held(&pressed, true, true));
            assert!(!hold.is_held(&pressed, fine, !fine));
            assert!(!hold.is_held(&HashSet::new(), !fine, fine));
            let other_pad = HashSet::from([(DeviceId(43), control.1)]);
            assert!(!hold.is_held(&other_pad, !fine, fine));
            for elapsed in [delay - Duration::from_millis(1), delay, delay + tick * 3] {
                let units = hold.take_due_units(start + elapsed, &input);
                assert_eq!(units, cursor.take_due_units(start + elapsed, &input));
                if elapsed == delay {
                    assert_eq!(units, direction * if fine { 1 } else { 7 });
                }
            }
        }
    }
}

#[test]
fn nine_key_cover_shortcuts_preserve_other_modes_and_nine_key_hispeed_buttons() {
    let input = crate::config::play_input::default_profile_input();
    for key_mode in [KeyMode::K8, KeyMode::K14] {
        let play_input = play_option_input_for(&input, key_mode);
        let control =
            play_input.binding.entries.iter().find(|entry| entry.lane == Lane::Key8).unwrap();
        assert!(matches!(
            play_option_control_for_input(
                control.device.unwrap_or(W_KEYBOARD_DEVICE_ID),
                &control.control,
                true,
                false,
                Some(&play_input),
                &input,
            ),
            Some(PlayOptionControl::Hispeed(_)),
        ));
    }
    let play_input = play_option_input_for(&input, KeyMode::K9);
    for (control, down) in [("Z", true), ("S", false), ("V", true)] {
        for fine in [false, true] {
            let change = play_option_control_for_input(
                W_KEYBOARD_DEVICE_ID,
                &PhysicalControl::KeyboardKey(control.into()),
                !fine,
                fine,
                Some(&play_input),
                &input,
            );
            assert_eq!(
                change,
                Some(match (fine, down) {
                    (false, true) => PlayOptionControl::Hispeed(HispeedChange::Down),
                    (false, false) => PlayOptionControl::Hispeed(HispeedChange::Up),
                    (true, true) => PlayOptionControl::GreenNumber(GreenNumberChange::Down),
                    (true, false) => PlayOptionControl::GreenNumber(GreenNumberChange::Up),
                }),
            );
        }
    }
}
