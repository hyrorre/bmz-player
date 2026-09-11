use super::*;

/// 指定スロットへキーボード / コントローラー割り当てを更新する。
pub fn apply_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
    control: &str,
) -> Result<(), crate::config::play_input::InheritError> {
    if let KeyBindingTarget::Action { action, slot } = target {
        apply_action_binding(input, action, slot, control);
        return Ok(());
    }

    let lane = lane_for_target(target);
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let mut bindings = resolve_play_bindings(input, key_mode)?;
    match target {
        KeyBindingTarget::Key { lane, slot } => {
            let keyboard = read_lane_keyboard_slots(&bindings, lane);
            let primary = keyboard.primary;
            let secondary = keyboard.secondary;

            match slot {
                KeyBindingSlot::KeyboardPrimary => {
                    remove_lane_device_bindings(&mut bindings, lane, "keyboard");
                    write_lane_keyboard_bindings(
                        &mut bindings,
                        lane,
                        Some(control),
                        secondary.as_deref(),
                    );
                }
                KeyBindingSlot::KeyboardSecondary => {
                    remove_lane_device_bindings(&mut bindings, lane, "keyboard");
                    write_lane_keyboard_bindings(
                        &mut bindings,
                        lane,
                        primary.as_deref(),
                        Some(control),
                    );
                }
                KeyBindingSlot::Controller
                | KeyBindingSlot::Controller1P
                | KeyBindingSlot::Controller2P => {
                    write_lane_gamepad_bindings_for_device(
                        &mut bindings,
                        lane,
                        slot.device(),
                        &[control.to_string()],
                    );
                }
            }
        }
        KeyBindingTarget::Scratch { lane, direction, slot } => match slot {
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                let mut keyboard = read_scratch_keyboard_slots(&bindings, lane);
                keyboard.set(direction, slot, Some(control.to_string()));
                write_scratch_keyboard_bindings(&mut bindings, lane, &keyboard);
            }
            KeyBindingSlot::Controller
            | KeyBindingSlot::Controller1P
            | KeyBindingSlot::Controller2P => {
                let mut gamepad =
                    read_scratch_gamepad_slots_for_device(&bindings, lane, slot.device());
                gamepad.set(direction, Some(control.to_string()));
                write_scratch_gamepad_bindings_for_device(
                    &mut bindings,
                    lane,
                    slot.device(),
                    &gamepad,
                );
            }
        },
        KeyBindingTarget::Action { .. } => unreachable!("action binding is handled above"),
    }

    persist_bindings(input, key_mode, bindings)
}

/// 指定スロットの割り当てを削除する。
pub fn clear_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> Result<(), crate::config::play_input::InheritError> {
    if let KeyBindingTarget::Action { action, slot } = target {
        clear_action_binding(input, action, slot);
        return Ok(());
    }

    let lane = lane_for_target(target);
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let mut bindings = resolve_play_bindings(input, key_mode)?;

    match target {
        KeyBindingTarget::Key { lane, slot } => {
            let keyboard = read_lane_keyboard_slots(&bindings, lane);
            let primary = keyboard.primary;
            let secondary = keyboard.secondary;

            match slot {
                KeyBindingSlot::KeyboardPrimary => {
                    remove_lane_device_bindings(&mut bindings, lane, "keyboard");
                    write_lane_keyboard_bindings(&mut bindings, lane, None, secondary.as_deref());
                }
                KeyBindingSlot::KeyboardSecondary => {
                    remove_lane_device_bindings(&mut bindings, lane, "keyboard");
                    write_lane_keyboard_bindings(&mut bindings, lane, primary.as_deref(), None);
                }
                KeyBindingSlot::Controller
                | KeyBindingSlot::Controller1P
                | KeyBindingSlot::Controller2P => {
                    write_lane_gamepad_bindings_for_device(&mut bindings, lane, slot.device(), &[]);
                }
            }
        }
        KeyBindingTarget::Scratch { lane, direction, slot } => match slot {
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                let mut keyboard = read_scratch_keyboard_slots(&bindings, lane);
                keyboard.set(direction, slot, None);
                write_scratch_keyboard_bindings(&mut bindings, lane, &keyboard);
            }
            KeyBindingSlot::Controller
            | KeyBindingSlot::Controller1P
            | KeyBindingSlot::Controller2P => {
                let mut gamepad =
                    read_scratch_gamepad_slots_for_device(&bindings, lane, slot.device());
                gamepad.set(direction, None);
                write_scratch_gamepad_bindings_for_device(
                    &mut bindings,
                    lane,
                    slot.device(),
                    &gamepad,
                );
            }
        },
        KeyBindingTarget::Action { .. } => unreachable!("action binding is handled above"),
    }

    persist_bindings(input, key_mode, bindings)
}

/// 現在表示中の共通グループだけをデフォルトへ戻す。
pub fn restore_action_group_defaults(
    input: &mut ProfileInputConfig,
    group: KeyBindingGroup,
    slot: KeyBindingSlot,
) {
    let defaults = crate::config::profile_config::default_ui_bindings();
    for &action in actions_for_group(group) {
        let default_control = defaults.iter().find_map(|entry| {
            if entry.action != Some(action) || !device_matches(&entry.device, slot.device()) {
                return None;
            }
            match slot {
                KeyBindingSlot::KeyboardPrimary => (entry.keyboard_slot.is_none()
                    || entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Primary))
                .then(|| entry.control.clone()),
                KeyBindingSlot::KeyboardSecondary => (entry.keyboard_slot
                    == Some(KeyboardBindingSlotConfig::Secondary))
                .then(|| entry.control.clone()),
                KeyBindingSlot::Controller
                | KeyBindingSlot::Controller1P
                | KeyBindingSlot::Controller2P => Some(entry.control.clone()),
            }
        });
        let target = KeyBindingTarget::Action { action, slot };
        match default_control {
            Some(control) => apply_play_binding(input, KeyMode::K7, target, &control)
                .expect("default action binding is valid"),
            None => clear_play_binding(input, KeyMode::K7, target)
                .expect("default action binding is valid"),
        }
    }
}

/// 現在表示中のキーモードと入力スロットだけをデフォルトへ戻す。
pub fn restore_key_mode_defaults(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    slot: KeyBindingSlot,
) -> Result<(), crate::config::play_input::InheritError> {
    let default_input = crate::config::play_input::default_profile_input();
    let defaults = resolve_play_bindings(&default_input, key_mode)?;
    for target in key_mode_binding_targets(key_mode, slot) {
        let default_control = default_control_for_target(&defaults, target);
        match default_control {
            Some(control) => apply_play_binding(input, key_mode, target, &control)?,
            None => clear_play_binding(input, key_mode, target)?,
        }
    }
    Ok(())
}

fn actions_for_group(group: KeyBindingGroup) -> &'static [InputActionConfig] {
    match group {
        KeyBindingGroup::Common => COMMON_ACTIONS,
        KeyBindingGroup::Select => SELECT_ACTIONS,
        KeyBindingGroup::Play => PLAY_ACTIONS,
        KeyBindingGroup::Result => RESULT_ACTIONS,
    }
}

fn default_control_for_target(
    defaults: &[BindingConfigEntry],
    target: KeyBindingTarget,
) -> Option<String> {
    match target {
        KeyBindingTarget::Key { lane, slot } => defaults.iter().find_map(|entry| {
            (entry.lane == Some(lane)
                && entry.action.is_none()
                && device_matches(&entry.device, slot.device())
                && (slot.is_controller()
                    || match slot {
                        KeyBindingSlot::KeyboardPrimary => {
                            entry.keyboard_slot.is_none()
                                || entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Primary)
                        }
                        KeyBindingSlot::KeyboardSecondary => {
                            entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Secondary)
                        }
                        _ => unreachable!(),
                    }))
            .then(|| entry.control.clone())
        }),
        KeyBindingTarget::Scratch { lane, direction, slot } => defaults.iter().find_map(|entry| {
            (entry.lane == Some(lane)
                && entry.action.is_none()
                && entry.scratch
                    == Some(match direction {
                        ScratchDirection::Up => ScratchDirectionConfig::Up,
                        ScratchDirection::Down => ScratchDirectionConfig::Down,
                    })
                && device_matches(&entry.device, slot.device())
                && (slot.is_controller()
                    || match slot {
                        KeyBindingSlot::KeyboardPrimary => {
                            entry.keyboard_slot.is_none()
                                || entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Primary)
                        }
                        KeyBindingSlot::KeyboardSecondary => {
                            entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Secondary)
                        }
                        _ => unreachable!(),
                    }))
            .then(|| entry.control.clone())
        }),
        KeyBindingTarget::Action { .. } => None,
    }
}

pub fn snapshot_play_mode_config(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Option<PlayModeInputConfig> {
    input.play.get(key_mode.play_map_key()).cloned()
}

pub fn restore_play_mode_config(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    config: Option<PlayModeInputConfig>,
) {
    match config {
        Some(config) => {
            input.play.insert(key_mode.play_map_key().to_string(), config);
        }
        None => {
            input.play.remove(key_mode.play_map_key());
        }
    }
}

pub(super) fn ensure_play_mode_config(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
) -> &mut PlayModeInputConfig {
    input.play.entry(key_mode.play_map_key().to_string()).or_insert_with(|| PlayModeInputConfig {
        inherit: None,
        bindings: default_play_bindings(key_mode),
        ..Default::default()
    })
}
