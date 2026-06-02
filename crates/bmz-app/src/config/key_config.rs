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
            Self::KeyboardPrimary | Self::KeyboardSecondary => {
                "PRESS KEY  Deleteキーで割り当てを解除"
            }
            Self::Controller => "PRESS BTN",
        }
    }
}

/// スクラッチの上下方向（UI / 選曲入力用。`Lane::Scratch` は増やさない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScratchDirection {
    Up,
    Down,
}

/// キー設定 UI の 1 行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyBindingTarget {
    Key { lane: LaneConfig, slot: KeyBindingSlot },
    Scratch { lane: LaneConfig, direction: ScratchDirection, slot: KeyBindingSlot },
}

impl KeyBindingTarget {
    pub fn slot(self) -> KeyBindingSlot {
        match self {
            Self::Key { slot, .. } | Self::Scratch { slot, .. } => slot,
        }
    }
}

pub fn key_mode_settings_path(keys_root: &str, key_mode: KeyMode) -> String {
    format!("{keys_root}:{}", key_mode.play_map_key())
}

pub fn is_scratch_lane(lane: LaneConfig) -> bool {
    matches!(lane, LaneConfig::Scratch | LaneConfig::Scratch2)
}

pub fn lane_entries_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    key_mode.active_lanes().iter().map(|&lane| lane_to_config(lane)).collect()
}

pub fn scratch_lanes_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    lane_entries_for_key_mode(key_mode).into_iter().filter(|&lane| is_scratch_lane(lane)).collect()
}

pub fn key_lanes_for_key_mode(key_mode: KeyMode) -> Vec<LaneConfig> {
    lane_entries_for_key_mode(key_mode).into_iter().filter(|&lane| !is_scratch_lane(lane)).collect()
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

pub fn binding_row_label(target: KeyBindingTarget) -> String {
    match target {
        KeyBindingTarget::Key { lane, slot } => {
            format!("{} ({})", lane_label(lane), slot.suffix())
        }
        KeyBindingTarget::Scratch { lane, direction, slot } => {
            let dir = match direction {
                ScratchDirection::Up => "UP",
                ScratchDirection::Down => "DOWN",
            };
            format!("{} {} ({})", lane_label(lane), dir, slot.suffix())
        }
    }
}

pub fn is_scratch_up_control(control: &str) -> bool {
    control.contains("ScratchUp")
        || control.ends_with('-')
        || control == "AxisLeftX-"
        || control == "AxisRightX-"
        || control == "Button9"
}

pub fn is_scratch_down_control(control: &str) -> bool {
    control.contains("ScratchDown")
        || control.ends_with('+')
        || control == "AxisLeftX+"
        || control == "AxisRightX+"
        || control == "Button8"
}

pub fn format_play_binding(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> String {
    format_target_control(&resolved_play_bindings(&profile.input, key_mode), target)
}

fn resolved_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Vec<BindingConfigEntry> {
    resolve_play_bindings(input, key_mode).unwrap_or_else(|_| default_play_bindings(key_mode))
}

fn format_target_control(bindings: &[BindingConfigEntry], target: KeyBindingTarget) -> String {
    match target {
        KeyBindingTarget::Key { lane, slot } => match slot {
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
        },
        KeyBindingTarget::Scratch { lane, direction, slot } => match slot {
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                read_scratch_keyboard_slots(bindings, lane)
                    .get(direction, slot)
                    .unwrap_or_else(|| "(none)".to_string())
            }
            KeyBindingSlot::Controller => read_scratch_gamepad_slots(bindings, lane)
                .get(direction)
                .unwrap_or_else(|| "(none)".to_string()),
        },
    }
}

#[derive(Debug, Clone, Default)]
struct ScratchKeyboardSlots {
    up_primary: Option<String>,
    down_primary: Option<String>,
    up_secondary: Option<String>,
    down_secondary: Option<String>,
}

impl ScratchKeyboardSlots {
    fn get(self, direction: ScratchDirection, slot: KeyBindingSlot) -> Option<String> {
        match (direction, slot) {
            (ScratchDirection::Up, KeyBindingSlot::KeyboardPrimary) => self.up_primary,
            (ScratchDirection::Down, KeyBindingSlot::KeyboardPrimary) => self.down_primary,
            (ScratchDirection::Up, KeyBindingSlot::KeyboardSecondary) => self.up_secondary,
            (ScratchDirection::Down, KeyBindingSlot::KeyboardSecondary) => self.down_secondary,
            (_, KeyBindingSlot::Controller) => None,
        }
    }

    fn set(&mut self, direction: ScratchDirection, slot: KeyBindingSlot, control: Option<String>) {
        match (direction, slot) {
            (ScratchDirection::Up, KeyBindingSlot::KeyboardPrimary) => self.up_primary = control,
            (ScratchDirection::Down, KeyBindingSlot::KeyboardPrimary) => {
                self.down_primary = control
            }
            (ScratchDirection::Up, KeyBindingSlot::KeyboardSecondary) => {
                self.up_secondary = control
            }
            (ScratchDirection::Down, KeyBindingSlot::KeyboardSecondary) => {
                self.down_secondary = control
            }
            (_, KeyBindingSlot::Controller) => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ScratchGamepadSlots {
    up: Option<String>,
    down: Option<String>,
}

impl ScratchGamepadSlots {
    fn get(self, direction: ScratchDirection) -> Option<String> {
        match direction {
            ScratchDirection::Up => self.up,
            ScratchDirection::Down => self.down,
        }
    }

    fn set(&mut self, direction: ScratchDirection, control: Option<String>) {
        match direction {
            ScratchDirection::Up => self.up = control,
            ScratchDirection::Down => self.down = control,
        }
    }
}

fn read_scratch_keyboard_slots(
    bindings: &[BindingConfigEntry],
    lane: LaneConfig,
) -> ScratchKeyboardSlots {
    let keys = keyboard_controls_for_lane(bindings, lane);
    let mut slots = ScratchKeyboardSlots::default();
    if keys.is_empty() {
        return slots;
    }
    slots.up_primary = keys.first().cloned();
    slots.down_primary = keys.get(1).cloned().or_else(|| keys.first().cloned());
    slots.up_secondary = keys.get(2).cloned();
    slots.down_secondary = keys.get(3).cloned().or_else(|| keys.get(2).cloned());
    slots
}

fn read_scratch_gamepad_slots(
    bindings: &[BindingConfigEntry],
    lane: LaneConfig,
) -> ScratchGamepadSlots {
    let mut slots = ScratchGamepadSlots::default();
    let mut undirected = Vec::new();

    for control in gamepad_controls_for_lane(bindings, lane) {
        if is_scratch_up_control(&control) {
            slots.up = Some(control);
        } else if is_scratch_down_control(&control) {
            slots.down = Some(control);
        } else {
            undirected.push(control);
        }
    }

    if let Some(control) = undirected.into_iter().next() {
        if slots.up.is_none() {
            slots.up = Some(control.clone());
        }
        if slots.down.is_none() {
            slots.down = Some(control);
        }
    }

    slots
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

fn write_scratch_keyboard_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    slots: &ScratchKeyboardSlots,
) {
    remove_lane_device_bindings(bindings, lane, "keyboard");
    for control in [
        slots.up_primary.as_deref(),
        slots.down_primary.as_deref(),
        slots.up_secondary.as_deref(),
        slots.down_secondary.as_deref(),
    ] {
        if let Some(control) = control.filter(|value| !value.is_empty()) {
            bindings.push(play_binding(control, lane));
        }
    }
}

fn write_scratch_gamepad_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    slots: &ScratchGamepadSlots,
) {
    remove_lane_device_bindings(bindings, lane, "gamepad");
    for control in [slots.up.as_deref(), slots.down.as_deref()] {
        if let Some(control) = control.filter(|value| !value.is_empty()) {
            bindings.push(gamepad_play_binding(control, lane));
        }
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

fn lane_for_target(target: KeyBindingTarget) -> LaneConfig {
    match target {
        KeyBindingTarget::Key { lane, .. } | KeyBindingTarget::Scratch { lane, .. } => lane,
    }
}

/// 指定スロットへキーボード / コントローラー割り当てを更新する。
pub fn apply_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
    control: &str,
) -> Result<(), super::play_input::InheritError> {
    let lane = lane_for_target(target);
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let slot = target.slot();
    let mut bindings = resolve_play_bindings(input, key_mode)?;
    remove_control_from_device(&mut bindings, slot.device(), control);

    match target {
        KeyBindingTarget::Key { lane, slot } => {
            let controls = keyboard_controls_for_lane(&bindings, lane);
            let primary = controls.first().cloned();
            let secondary = controls.get(1).cloned();
            let gamepad = gamepad_controls_for_lane(&bindings, lane);

            remove_lane_device_bindings(&mut bindings, lane, "keyboard");
            remove_lane_device_bindings(&mut bindings, lane, "gamepad");

            match slot {
                KeyBindingSlot::KeyboardPrimary => {
                    write_lane_keyboard_bindings(
                        &mut bindings,
                        lane,
                        Some(control),
                        secondary.as_deref(),
                    );
                    write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
                }
                KeyBindingSlot::KeyboardSecondary => {
                    write_lane_keyboard_bindings(
                        &mut bindings,
                        lane,
                        primary.as_deref(),
                        Some(control),
                    );
                    write_lane_gamepad_bindings(&mut bindings, lane, &gamepad);
                }
                KeyBindingSlot::Controller => {
                    write_lane_keyboard_bindings(
                        &mut bindings,
                        lane,
                        primary.as_deref(),
                        secondary.as_deref(),
                    );
                    write_lane_gamepad_bindings(&mut bindings, lane, &[control.to_string()]);
                }
            }
        }
        KeyBindingTarget::Scratch { lane, direction, slot } => {
            let mut keyboard = read_scratch_keyboard_slots(&bindings, lane);
            let mut gamepad = read_scratch_gamepad_slots(&bindings, lane);

            match slot {
                KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                    keyboard.set(direction, slot, Some(control.to_string()));
                    write_scratch_keyboard_bindings(&mut bindings, lane, &keyboard);
                    write_scratch_gamepad_bindings(&mut bindings, lane, &gamepad);
                }
                KeyBindingSlot::Controller => {
                    gamepad.set(direction, Some(control.to_string()));
                    write_scratch_keyboard_bindings(&mut bindings, lane, &keyboard);
                    write_scratch_gamepad_bindings(&mut bindings, lane, &gamepad);
                }
            }
        }
    }

    persist_bindings(input, key_mode, bindings)
}

/// 指定スロットの割り当てを削除する。
pub fn clear_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> Result<(), super::play_input::InheritError> {
    let lane = lane_for_target(target);
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let mut bindings = resolve_play_bindings(input, key_mode)?;

    match target {
        KeyBindingTarget::Key { lane, slot } => {
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
        }
        KeyBindingTarget::Scratch { lane, direction, slot } => {
            let mut keyboard = read_scratch_keyboard_slots(&bindings, lane);
            let mut gamepad = read_scratch_gamepad_slots(&bindings, lane);

            match slot {
                KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary => {
                    keyboard.set(direction, slot, None);
                    write_scratch_keyboard_bindings(&mut bindings, lane, &keyboard);
                    write_scratch_gamepad_bindings(&mut bindings, lane, &gamepad);
                }
                KeyBindingSlot::Controller => {
                    gamepad.set(direction, None);
                    write_scratch_keyboard_bindings(&mut bindings, lane, &keyboard);
                    write_scratch_gamepad_bindings(&mut bindings, lane, &gamepad);
                }
            }
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

    fn key_target(lane: LaneConfig, slot: KeyBindingSlot) -> KeyBindingTarget {
        KeyBindingTarget::Key { lane, slot }
    }

    fn scratch_target(
        lane: LaneConfig,
        direction: ScratchDirection,
        slot: KeyBindingSlot,
    ) -> KeyBindingTarget {
        KeyBindingTarget::Scratch { lane, direction, slot }
    }

    #[test]
    fn lane_entries_follow_active_lanes() {
        assert_eq!(lane_entries_for_key_mode(KeyMode::K7).len(), 8);
        assert_eq!(scratch_lanes_for_key_mode(KeyMode::K7).len(), 1);
        assert_eq!(key_lanes_for_key_mode(KeyMode::K7).len(), 7);
        assert_eq!(scratch_lanes_for_key_mode(KeyMode::K14).len(), 2);
    }

    #[test]
    fn apply_play_binding_keeps_primary_and_secondary_separate() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
            "Z",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
            "Q",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
            ),
            "Z"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
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
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
            "Q",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            key_target(LaneConfig::Key2, KeyBindingSlot::KeyboardPrimary),
            "Q",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
            ),
            "(none)"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key2, KeyBindingSlot::KeyboardPrimary),
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
            key_target(LaneConfig::Key1, KeyBindingSlot::Controller),
            "Button9",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::Controller),
            ),
            "Button9"
        );
        assert_ne!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
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
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
            "Z",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
            "Q",
        )
        .unwrap();
        clear_play_binding(
            &mut profile.input,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
            ),
            "Z"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
            ),
            "(none)"
        );
    }

    #[test]
    fn scratch_keyboard_up_and_down_are_independent() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Up,
                KeyBindingSlot::KeyboardPrimary,
            ),
            "Q",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Down,
                KeyBindingSlot::KeyboardPrimary,
            ),
            "W",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(
                    LaneConfig::Scratch,
                    ScratchDirection::Up,
                    KeyBindingSlot::KeyboardPrimary,
                ),
            ),
            "Q"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(
                    LaneConfig::Scratch,
                    ScratchDirection::Down,
                    KeyBindingSlot::KeyboardPrimary,
                ),
            ),
            "W"
        );
    }

    #[test]
    fn scratch_controller_up_and_down_are_independent() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::Controller),
            "AxisLeftX-",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller),
            "AxisLeftX+",
        )
        .unwrap();
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(
                    LaneConfig::Scratch,
                    ScratchDirection::Up,
                    KeyBindingSlot::Controller,
                ),
            ),
            "AxisLeftX-"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(
                    LaneConfig::Scratch,
                    ScratchDirection::Down,
                    KeyBindingSlot::Controller,
                ),
            ),
            "AxisLeftX+"
        );
    }

    #[test]
    fn default_scratch_keyboard_shows_same_key_for_up_and_down() {
        let profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(
                    LaneConfig::Scratch,
                    ScratchDirection::Up,
                    KeyBindingSlot::KeyboardPrimary,
                ),
            ),
            "LShift"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(
                    LaneConfig::Scratch,
                    ScratchDirection::Down,
                    KeyBindingSlot::KeyboardPrimary,
                ),
            ),
            "LShift"
        );
    }
}
