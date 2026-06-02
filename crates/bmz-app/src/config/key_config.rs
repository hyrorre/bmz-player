use bmz_core::lane::KeyMode;

use super::play::{lane_from_config, lane_to_config};
use super::play_input::{
    default_play_bindings, gamepad_play_binding, play_binding, resolve_play_bindings,
};
use super::profile_config::{
    BindingConfigEntry, LaneConfig, PlayModeInputConfig, ProfileConfig, ProfileInputConfig,
};

/// 選曲画面のキー設定で編集対象とする KEY モード。
pub const KEY_CONFIG_MODES: &[KeyMode] = &[KeyMode::K5, KeyMode::K7, KeyMode::K10, KeyMode::K14];

/// 1 レーンあたりの割り当てスロット。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyBindingSlot {
    KeyboardPrimary,
    KeyboardSecondary,
    Controller,
}

pub const KEY_BINDING_SLOTS: &[KeyBindingSlot] = &[
    KeyBindingSlot::KeyboardPrimary,
    KeyBindingSlot::KeyboardSecondary,
    KeyBindingSlot::Controller,
];

impl KeyBindingSlot {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::KeyboardPrimary => "KEYBOARD",
            Self::KeyboardSecondary => "KEYBOARD SUB",
            Self::Controller => "CONTROLLER",
        }
    }

    pub fn device(self) -> &'static str {
        match self {
            Self::KeyboardPrimary | Self::KeyboardSecondary => "keyboard",
            Self::Controller => "gamepad",
        }
    }

    pub fn listen_hint(self) -> &'static str {
        match self {
            Self::KeyboardPrimary | Self::KeyboardSecondary => "PRESS KEY",
            Self::Controller => "PRESS BTN",
        }
    }
}

pub fn key_mode_settings_path(keys_root: &str, key_mode: KeyMode) -> String {
    format!("{keys_root}:{}", key_mode.play_map_key())
}

pub fn lane_entries_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    key_mode.active_lanes().iter().map(|&lane| lane_to_config(lane)).collect()
}

pub fn lane_label(lane: LaneConfig) -> &'static str {
    match lane {
        LaneConfig::Scratch => "SCRATCH",
        LaneConfig::Scratch2 => "SCRATCH 2",
        LaneConfig::Key1 => "KEY 1",
        LaneConfig::Key2 => "KEY 2",
        LaneConfig::Key3 => "KEY 3",
        LaneConfig::Key4 => "KEY 4",
        LaneConfig::Key5 => "KEY 5",
        LaneConfig::Key6 => "KEY 6",
        LaneConfig::Key7 => "KEY 7",
        LaneConfig::Key8 => "KEY 8",
        LaneConfig::Key9 => "KEY 9",
        LaneConfig::Key10 => "KEY 10",
        LaneConfig::Key11 => "KEY 11",
        LaneConfig::Key12 => "KEY 12",
        LaneConfig::Key13 => "KEY 13",
        LaneConfig::Key14 => "KEY 14",
    }
}

pub fn binding_row_label(lane: LaneConfig, slot: KeyBindingSlot) -> String {
    format!("{} ({})", lane_label(lane), slot.suffix())
}

pub fn format_play_binding(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    lane: LaneConfig,
    slot: KeyBindingSlot,
) -> String {
    format_slot_control(&resolved_play_bindings(&profile.input, key_mode), lane, slot)
}

fn resolved_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Vec<BindingConfigEntry> {
    resolve_play_bindings(input, key_mode).unwrap_or_else(|_| default_play_bindings(key_mode))
}

fn format_slot_control(
    bindings: &[BindingConfigEntry],
    lane: LaneConfig,
    slot: KeyBindingSlot,
) -> String {
    match slot {
        KeyBindingSlot::KeyboardPrimary => keyboard_controls_for_lane(bindings, lane)
            .first()
            .cloned()
            .unwrap_or_else(|| "(none)".to_string()),
        KeyBindingSlot::KeyboardSecondary => keyboard_controls_for_lane(bindings, lane)
            .get(1)
            .cloned()
            .unwrap_or_else(|| "(none)".to_string()),
        KeyBindingSlot::Controller => {
            let controls = gamepad_controls_for_lane(bindings, lane);
            if controls.is_empty() { "(none)".to_string() } else { controls.join(" / ") }
        }
    }
}

fn keyboard_controls_for_lane(bindings: &[BindingConfigEntry], lane: LaneConfig) -> Vec<String> {
    bindings
        .iter()
        .filter(|entry| entry.device == "keyboard" && entry.lane == Some(lane))
        .map(|entry| entry.control.clone())
        .collect()
}

fn gamepad_controls_for_lane(bindings: &[BindingConfigEntry], lane: LaneConfig) -> Vec<String> {
    bindings
        .iter()
        .filter(|entry| entry.device == "gamepad" && entry.lane == Some(lane))
        .map(|entry| entry.control.clone())
        .collect()
}

fn remove_lane_device_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    device: &str,
) {
    bindings.retain(|entry| !(entry.device == device && entry.lane == Some(lane)));
}

fn remove_control_from_device(bindings: &mut Vec<BindingConfigEntry>, device: &str, control: &str) {
    bindings.retain(|entry| !(entry.device == device && entry.control == control));
}

fn write_lane_keyboard_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    primary: Option<&str>,
    secondary: Option<&str>,
) {
    remove_lane_device_bindings(bindings, lane, "keyboard");
    if let Some(control) = primary.filter(|value| !value.is_empty()) {
        bindings.push(play_binding(control, lane));
    }
    if let Some(control) = secondary.filter(|value| !value.is_empty()) {
        bindings.push(play_binding(control, lane));
    }
}

fn write_lane_gamepad_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    controls: &[String],
) {
    remove_lane_device_bindings(bindings, lane, "gamepad");
    for control in controls {
        if !control.is_empty() {
            bindings.push(gamepad_play_binding(control, lane));
        }
    }
}

fn persist_bindings(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    bindings: Vec<BindingConfigEntry>,
) -> Result<(), super::play_input::InheritError> {
    let config = ensure_play_mode_config(input, key_mode);
    config.inherit = None;
    config.bindings = bindings;
    Ok(())
}

/// 指定スロットへキーボード / コントローラー割り当てを更新する。
pub fn apply_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    lane: LaneConfig,
    slot: KeyBindingSlot,
    control: &str,
) -> Result<(), super::play_input::InheritError> {
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let mut bindings = resolve_play_bindings(input, key_mode)?;
    remove_control_from_device(&mut bindings, slot.device(), control);

    let controls = keyboard_controls_for_lane(&bindings, lane);
    let primary = controls.first().cloned();
    let secondary = controls.get(1).cloned();
    let mut gamepad = gamepad_controls_for_lane(&bindings, lane);

    remove_lane_device_bindings(&mut bindings, lane, "keyboard");
    remove_lane_device_bindings(&mut bindings, lane, "gamepad");

    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            write_lane_keyboard_bindings(&mut bindings, lane, Some(control), secondary.as_deref());
            write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
        }
        KeyBindingSlot::KeyboardSecondary => {
            write_lane_keyboard_bindings(&mut bindings, lane, primary.as_deref(), Some(control));
            write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
        }
        KeyBindingSlot::Controller => {
            gamepad = vec![control.to_string()];
            write_lane_keyboard_bindings(
                &mut bindings,
                lane,
                primary.as_deref(),
                secondary.as_deref(),
            );
            write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
        }
    }

    persist_bindings(input, key_mode, bindings)
}

/// 指定スロットの割り当てを削除する。
pub fn clear_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    lane: LaneConfig,
    slot: KeyBindingSlot,
) -> Result<(), super::play_input::InheritError> {
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let mut bindings = resolve_play_bindings(input, key_mode)?;
    let controls = keyboard_controls_for_lane(&bindings, lane);
    let primary = controls.first().cloned();
    let secondary = controls.get(1).cloned();
    let gamepad = gamepad_controls_for_lane(&bindings, lane);

    remove_lane_device_bindings(&mut bindings, lane, "keyboard");
    remove_lane_device_bindings(&mut bindings, lane, "gamepad");

    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            write_lane_keyboard_bindings(&mut bindings, lane, None, secondary.as_deref());
            write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
        }
        KeyBindingSlot::KeyboardSecondary => {
            write_lane_keyboard_bindings(&mut bindings, lane, primary.as_deref(), None);
            write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
        }
        KeyBindingSlot::Controller => {
            write_lane_keyboard_bindings(
                &mut bindings,
                lane,
                primary.as_deref(),
                secondary.as_deref(),
            );
            write_lane_gamepad_bindings(&mut bindings, lane, &[]);
        }
    }

    persist_bindings(input, key_mode, bindings)
}

pub fn snapshot_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Vec<BindingConfigEntry> {
    input
        .play
        .get(key_mode.play_map_key())
        .map(|config| config.bindings.clone())
        .unwrap_or_default()
}

pub fn restore_play_bindings(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    bindings: Vec<BindingConfigEntry>,
) {
    if bindings.is_empty() {
        input.play.remove(key_mode.play_map_key());
        return;
    }
    let config = ensure_play_mode_config(input, key_mode);
    config.bindings = bindings;
}

fn ensure_play_mode_config(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
) -> &mut PlayModeInputConfig {
    input.play.entry(key_mode.play_map_key().to_string()).or_insert_with(|| PlayModeInputConfig {
        inherit: None,
        bindings: default_play_bindings(key_mode),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::ProfileConfig;

    #[test]
    fn lane_entries_follow_active_lanes() {
        assert_eq!(lane_entries_for_key_mode(KeyMode::K7).len(), 8);
        assert_eq!(lane_entries_for_key_mode(KeyMode::K10).len(), 12);
        assert_eq!(lane_entries_for_key_mode(KeyMode::K14).len(), 16);
    }

    #[test]
    fn apply_play_binding_keeps_primary_and_secondary_separate() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::KeyboardPrimary,
            "Z",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::KeyboardSecondary,
            "Q",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::KeyboardPrimary
            ),
            "Z"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::KeyboardSecondary
            ),
            "Q"
        );
    }

    #[test]
    fn apply_play_binding_moves_duplicate_keyboard_key() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::KeyboardPrimary,
            "Q",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key2,
            KeyBindingSlot::KeyboardPrimary,
            "Q",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::KeyboardPrimary
            ),
            "(none)"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key2,
                KeyBindingSlot::KeyboardPrimary
            ),
            "Q"
        );
    }

    #[test]
    fn apply_play_binding_sets_controller_without_touching_keyboard() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::Controller,
            "Button9",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::Controller
            ),
            "Button9"
        );
        assert_ne!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::KeyboardPrimary
            ),
            "(none)"
        );
    }

    #[test]
    fn clear_play_binding_removes_selected_slot_only() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::KeyboardPrimary,
            "Z",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::KeyboardSecondary,
            "Q",
        )
        .unwrap();
        clear_play_binding(
            &mut profile.input,
            KeyMode::K7,
            LaneConfig::Key1,
            KeyBindingSlot::KeyboardSecondary,
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::KeyboardPrimary
            ),
            "Z"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                LaneConfig::Key1,
                KeyBindingSlot::KeyboardSecondary
            ),
            "(none)"
        );
    }
}
