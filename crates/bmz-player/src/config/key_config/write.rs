use super::*;

pub(super) fn write_action_keyboard_bindings(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    primary: Option<&str>,
    secondary: Option<&str>,
) {
    remove_action_device_bindings(input, action, "keyboard");
    if let Some(control) = primary.filter(|value| !value.is_empty()) {
        input.ui.bindings.push(action_binding_for_device(
            "keyboard",
            control,
            action,
            Some(KeyboardBindingSlotConfig::Primary),
        ));
    }
    if let Some(control) = secondary.filter(|value| !value.is_empty()) {
        input.ui.bindings.push(action_binding_for_device(
            "keyboard",
            control,
            action,
            Some(KeyboardBindingSlotConfig::Secondary),
        ));
    }
}

/// コントローラー側の E1〜E4 などを、指定したデバイス (`gamepad` / `gamepad1` / `gamepad2`)
/// の entry だけ書き換える。他のデバイスの entry は残す。
///
/// `controls` が空のとき、共通設定 (`gamepad`) は割り当てなしになる。1P / 2P の個別設定では
/// `control = ""` の entry を1件書き、共通設定を継承しない「明示的な未割り当て」を表す。
pub(super) fn write_action_gamepad_bindings(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    device: &str,
    controls: &[String],
) {
    remove_action_exact_device_bindings(input, action, device);
    let controls: Vec<&String> = controls.iter().filter(|control| !control.is_empty()).collect();
    if controls.is_empty() && is_side_specific_gamepad_device(device) {
        input.ui.bindings.push(action_binding_for_device(device, "", action, None));
        return;
    }
    for control in controls {
        input.ui.bindings.push(action_binding_for_device(device, control, action, None));
    }
}

pub(super) fn action_binding_for_device(
    device: &str,
    control: &str,
    action: InputActionConfig,
    keyboard_slot: Option<KeyboardBindingSlotConfig>,
) -> BindingConfigEntry {
    BindingConfigEntry {
        device: device.to_string(),
        control: control.to_string(),
        keyboard_slot,
        lane: None,
        action: Some(action),
        scratch: None,
    }
}

pub(super) fn apply_action_binding(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    slot: KeyBindingSlot,
    control: &str,
) {
    let keyboard = read_action_keyboard_slots(input, action);
    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            write_action_keyboard_bindings(
                input,
                action,
                Some(control),
                keyboard.secondary.as_deref(),
            );
        }
        KeyBindingSlot::KeyboardSecondary => {
            write_action_keyboard_bindings(
                input,
                action,
                keyboard.primary.as_deref(),
                Some(control),
            );
        }
        // 共通・1P・2P は互いの entry を消さず、編集した側だけを書き換える。
        KeyBindingSlot::Controller
        | KeyBindingSlot::Controller1P
        | KeyBindingSlot::Controller2P => {
            write_action_gamepad_bindings(input, action, slot.device(), &[control.to_string()]);
        }
    }
}

pub(super) fn clear_action_binding(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    slot: KeyBindingSlot,
) {
    let keyboard = read_action_keyboard_slots(input, action);
    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            write_action_keyboard_bindings(input, action, None, keyboard.secondary.as_deref());
        }
        KeyBindingSlot::KeyboardSecondary => {
            write_action_keyboard_bindings(input, action, keyboard.primary.as_deref(), None);
        }
        // 1P / 2P は明示的な未割り当てとして保存し、共通設定へ戻さない。
        KeyBindingSlot::Controller
        | KeyBindingSlot::Controller1P
        | KeyBindingSlot::Controller2P => {
            write_action_gamepad_bindings(input, action, slot.device(), &[]);
        }
    }
}

pub(super) fn write_lane_keyboard_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    primary: Option<&str>,
    secondary: Option<&str>,
) {
    remove_lane_device_bindings(bindings, lane, "keyboard");
    if let Some(control) = primary.filter(|value| !value.is_empty()) {
        let mut entry = play_binding(control, lane);
        entry.keyboard_slot = Some(KeyboardBindingSlotConfig::Primary);
        bindings.push(entry);
    }
    if let Some(control) = secondary.filter(|value| !value.is_empty()) {
        let mut entry = play_binding(control, lane);
        entry.keyboard_slot = Some(KeyboardBindingSlotConfig::Secondary);
        bindings.push(entry);
    }
}

pub(super) fn write_scratch_keyboard_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    slots: &ScratchKeyboardSlots,
) {
    remove_lane_device_bindings(bindings, lane, "keyboard");
    for (control, direction, keyboard_slot) in [
        (
            slots.up_primary.as_deref(),
            ScratchDirectionConfig::Up,
            KeyboardBindingSlotConfig::Primary,
        ),
        (
            slots.down_primary.as_deref(),
            ScratchDirectionConfig::Down,
            KeyboardBindingSlotConfig::Primary,
        ),
        (
            slots.up_secondary.as_deref(),
            ScratchDirectionConfig::Up,
            KeyboardBindingSlotConfig::Secondary,
        ),
        (
            slots.down_secondary.as_deref(),
            ScratchDirectionConfig::Down,
            KeyboardBindingSlotConfig::Secondary,
        ),
    ] {
        if let Some(control) = control.filter(|value| !value.is_empty()) {
            let mut entry = scratch_play_binding(control, lane, direction);
            entry.keyboard_slot = Some(keyboard_slot);
            bindings.push(entry);
        }
    }
}

pub(super) fn write_scratch_gamepad_bindings_for_device(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    device: &str,
    slots: &ScratchGamepadSlots,
) {
    remove_lane_device_bindings(bindings, lane, device);
    if device != "gamepad" {
        // 番号付きへ書き込むときは wildcard を消して二重マッチを防ぐ。
        bindings.retain(|entry| {
            !(entry.device.eq_ignore_ascii_case("gamepad") && entry.lane == Some(lane))
        });
    }
    for (control, direction) in [
        (slots.up.as_deref(), ScratchDirectionConfig::Up),
        (slots.down.as_deref(), ScratchDirectionConfig::Down),
    ] {
        if let Some(control) = control.filter(|value| !value.is_empty()) {
            let mut entry = gamepad_play_binding_for_device(device, control, lane);
            entry.scratch = Some(direction);
            bindings.push(entry);
        }
    }
}

pub(super) fn write_lane_gamepad_bindings_for_device(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    device: &str,
    controls: &[String],
) {
    remove_lane_device_bindings(bindings, lane, device);
    if device != "gamepad" {
        bindings.retain(|entry| {
            !(entry.device.eq_ignore_ascii_case("gamepad") && entry.lane == Some(lane))
        });
    }
    for control in controls {
        if !control.is_empty() {
            bindings.push(gamepad_play_binding_for_device(device, control, lane));
        }
    }
}

pub(super) fn persist_bindings(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    bindings: Vec<BindingConfigEntry>,
) -> Result<(), crate::config::play_input::InheritError> {
    let config = ensure_play_mode_config(input, key_mode);
    config.inherit = None;
    config.bindings = bindings;
    Ok(())
}

pub(super) fn lane_for_target(target: KeyBindingTarget) -> LaneConfig {
    match target {
        KeyBindingTarget::Key { lane, .. } | KeyBindingTarget::Scratch { lane, .. } => lane,
        KeyBindingTarget::Action { .. } => LaneConfig::Key1,
    }
}
