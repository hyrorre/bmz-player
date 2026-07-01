use bmz_core::lane::KeyMode;

use super::play::{lane_from_config, lane_to_config};
use super::play_input::{
    default_play_bindings, gamepad_play_binding, is_gamepad_device, play_binding,
    resolve_play_bindings,
};
use super::profile_config::{
    BindingConfigEntry, InputActionConfig, LaneConfig, PlayModeInputConfig, ProfileConfig,
    ProfileInputConfig, ScratchDirectionConfig,
};

/// 選曲画面のキー設定で編集対象とする KEY モード。
pub const KEY_CONFIG_MODES: &[KeyMode] = &[
    KeyMode::K4,
    KeyMode::K5,
    KeyMode::K6,
    KeyMode::K7,
    KeyMode::K8,
    KeyMode::K9,
    KeyMode::K10,
    KeyMode::K14,
];

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
    Action { action: InputActionConfig, slot: KeyBindingSlot },
}

impl KeyBindingTarget {
    pub fn slot(self) -> KeyBindingSlot {
        match self {
            Self::Key { slot, .. } | Self::Scratch { slot, .. } | Self::Action { slot, .. } => slot,
        }
    }
}

pub const COMMON_ACTIONS: &[InputActionConfig] = &[
    InputActionConfig::E1,
    InputActionConfig::E2,
    InputActionConfig::E3,
    InputActionConfig::E4,
    InputActionConfig::SelectFavoriteSong,
    InputActionConfig::SelectFavoriteChart,
    InputActionConfig::SelectSameFolder,
];

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
        LaneConfig::Key8 => "2P KEY 1",
        LaneConfig::Key9 => "2P KEY 2",
        LaneConfig::Key10 => "2P KEY 3",
        LaneConfig::Key11 => "2P KEY 4",
        LaneConfig::Key12 => "2P KEY 5",
        LaneConfig::Key13 => "2P KEY 6",
        LaneConfig::Key14 => "2P KEY 7",
    }
}

pub fn lane_label_for_key_mode(key_mode: KeyMode, lane: LaneConfig) -> &'static str {
    match (key_mode, lane) {
        (KeyMode::K8 | KeyMode::K9, LaneConfig::Key8) => "KEY 8",
        (KeyMode::K9, LaneConfig::Key9) => "KEY 9",
        _ => lane_label(lane),
    }
}

pub fn binding_row_label(key_mode: KeyMode, target: KeyBindingTarget) -> String {
    match target {
        KeyBindingTarget::Key { lane, slot } => {
            format!("{} ({})", lane_label_for_key_mode(key_mode, lane), slot.suffix())
        }
        KeyBindingTarget::Scratch { lane, direction, slot } => {
            let dir = match direction {
                ScratchDirection::Up => "UP",
                ScratchDirection::Down => "DOWN",
            };
            format!("{} {} ({})", lane_label_for_key_mode(key_mode, lane), dir, slot.suffix())
        }
        KeyBindingTarget::Action { action, slot } => {
            format!("{} ({})", action_label(action), slot.suffix())
        }
    }
}

pub fn action_label(action: InputActionConfig) -> &'static str {
    match action {
        InputActionConfig::E1 => "E1",
        InputActionConfig::E2 => "E2",
        InputActionConfig::E3 => "E3",
        InputActionConfig::E4 => "E4",
        InputActionConfig::SelectEnter => "ENTER",
        InputActionConfig::SelectOptionArrange => "OPTION ARRANGE",
        InputActionConfig::SelectOptionGauge => "OPTION GAUGE",
        InputActionConfig::SelectOptionAssist => "OPTION ASSIST",
        InputActionConfig::SelectOptionBga => "OPTION BGA",
        InputActionConfig::SelectFavoriteSong => "FAVORITE SONG",
        InputActionConfig::SelectFavoriteChart => "FAVORITE CHART",
        InputActionConfig::SelectSameFolder => "SAME FOLDER",
    }
}

pub fn is_scratch_up_control(control: &str) -> bool {
    control.contains("ScratchUp")
        || control.ends_with('-')
        || control == "Axis1-"
        || control == "Axis2-"
        || control == "Button9"
}

pub fn is_scratch_down_control(control: &str) -> bool {
    control.contains("ScratchDown")
        || control.ends_with('+')
        || control == "Axis1+"
        || control == "Axis2+"
        || control == "Button8"
}

pub fn format_play_binding(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> String {
    match target {
        KeyBindingTarget::Action { .. } => format_action_binding(&profile.input, target),
        _ => format_target_control(&resolved_play_bindings(&profile.input, key_mode), target),
    }
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
        KeyBindingTarget::Action { .. } => "(none)".to_string(),
    }
}

fn format_action_binding(input: &ProfileInputConfig, target: KeyBindingTarget) -> String {
    let KeyBindingTarget::Action { action, slot } = target else {
        return "(none)".to_string();
    };
    let controls: Vec<_> = input
        .ui
        .bindings
        .iter()
        .filter(|entry| entry.device == slot.device() && entry.action == Some(action))
        .map(|entry| entry.control.clone())
        .collect();
    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            controls.first().cloned().unwrap_or_else(|| "(none)".to_string())
        }
        KeyBindingSlot::KeyboardSecondary => {
            controls.get(1).cloned().unwrap_or_else(|| "(none)".to_string())
        }
        KeyBindingSlot::Controller => {
            if controls.is_empty() {
                "(none)".to_string()
            } else {
                controls.join(" / ")
            }
        }
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

    // 明示の direction タグを最優先する。コントロール名 (+/-) からの推測は
    // 旧 entry 向けのフォールバックで、軸極性が逆のデバイスでは当てにならない。
    for entry in bindings.iter().filter(|e| is_gamepad_device(&e.device) && e.lane == Some(lane)) {
        let control = entry.control.clone();
        match entry.scratch {
            Some(ScratchDirectionConfig::Up) => slots.up = Some(control),
            Some(ScratchDirectionConfig::Down) => slots.down = Some(control),
            None => {
                if is_scratch_up_control(&control) {
                    slots.up = Some(control);
                } else if is_scratch_down_control(&control) {
                    slots.down = Some(control);
                } else {
                    undirected.push(control);
                }
            }
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
        .filter(|entry| is_gamepad_device(&entry.device) && entry.lane == Some(lane))
        .map(|entry| entry.control.clone())
        .collect()
}

fn remove_lane_device_bindings(
    bindings: &mut Vec<BindingConfigEntry>,
    lane: LaneConfig,
    device: &str,
) {
    bindings.retain(|entry| !(device_matches(&entry.device, device) && entry.lane == Some(lane)));
}

fn remove_control_from_device(bindings: &mut Vec<BindingConfigEntry>, device: &str, control: &str) {
    bindings.retain(|entry| !(device_matches(&entry.device, device) && entry.control == control));
}

fn remove_ui_control_from_device(input: &mut ProfileInputConfig, device: &str, control: &str) {
    input
        .ui
        .bindings
        .retain(|entry| !(device_matches(&entry.device, device) && entry.control == control));
}

fn action_controls_for_slot(
    input: &ProfileInputConfig,
    action: InputActionConfig,
    slot: KeyBindingSlot,
) -> Vec<String> {
    input
        .ui
        .bindings
        .iter()
        .filter(|entry| {
            device_matches(&entry.device, slot.device()) && entry.action == Some(action)
        })
        .map(|entry| entry.control.clone())
        .collect()
}

fn remove_action_device_bindings(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    device: &str,
) {
    input
        .ui
        .bindings
        .retain(|entry| !(device_matches(&entry.device, device) && entry.action == Some(action)));
}

fn device_matches(entry_device: &str, requested_device: &str) -> bool {
    if requested_device == "gamepad" {
        is_gamepad_device(entry_device)
    } else {
        entry_device == requested_device
    }
}

fn write_action_keyboard_bindings(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    primary: Option<&str>,
    secondary: Option<&str>,
) {
    remove_action_device_bindings(input, action, "keyboard");
    if let Some(control) = primary.filter(|value| !value.is_empty()) {
        input.ui.bindings.push(action_binding_for_device("keyboard", control, action));
    }
    if let Some(control) = secondary.filter(|value| !value.is_empty()) {
        input.ui.bindings.push(action_binding_for_device("keyboard", control, action));
    }
}

fn write_action_gamepad_bindings(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    controls: &[String],
) {
    remove_action_device_bindings(input, action, "gamepad");
    for control in controls {
        if !control.is_empty() {
            input.ui.bindings.push(action_binding_for_device("gamepad", control, action));
        }
    }
}

fn action_binding_for_device(
    device: &str,
    control: &str,
    action: InputActionConfig,
) -> BindingConfigEntry {
    BindingConfigEntry {
        device: device.to_string(),
        control: control.to_string(),
        lane: None,
        action: Some(action),
        scratch: None,
    }
}

fn apply_action_binding(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    slot: KeyBindingSlot,
    control: &str,
) {
    let keyboard = action_controls_for_slot(input, action, KeyBindingSlot::KeyboardPrimary);
    let primary = keyboard.first().cloned();
    let secondary = keyboard.get(1).cloned();
    let gamepad = action_controls_for_slot(input, action, KeyBindingSlot::Controller);

    remove_ui_control_from_device(input, slot.device(), control);
    remove_action_device_bindings(input, action, "keyboard");
    remove_action_device_bindings(input, action, "gamepad");

    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            write_action_keyboard_bindings(input, action, Some(control), secondary.as_deref());
            write_action_gamepad_bindings(input, action, &gamepad);
        }
        KeyBindingSlot::KeyboardSecondary => {
            write_action_keyboard_bindings(input, action, primary.as_deref(), Some(control));
            write_action_gamepad_bindings(input, action, &gamepad);
        }
        KeyBindingSlot::Controller => {
            write_action_keyboard_bindings(input, action, primary.as_deref(), secondary.as_deref());
            write_action_gamepad_bindings(input, action, &[control.to_string()]);
        }
    }
}

fn clear_action_binding(
    input: &mut ProfileInputConfig,
    action: InputActionConfig,
    slot: KeyBindingSlot,
) {
    let keyboard = action_controls_for_slot(input, action, KeyBindingSlot::KeyboardPrimary);
    let primary = keyboard.first().cloned();
    let secondary = keyboard.get(1).cloned();
    let gamepad = action_controls_for_slot(input, action, KeyBindingSlot::Controller);

    remove_action_device_bindings(input, action, "keyboard");
    remove_action_device_bindings(input, action, "gamepad");

    match slot {
        KeyBindingSlot::KeyboardPrimary => {
            write_action_keyboard_bindings(input, action, None, secondary.as_deref());
            write_action_gamepad_bindings(input, action, &gamepad);
        }
        KeyBindingSlot::KeyboardSecondary => {
            write_action_keyboard_bindings(input, action, primary.as_deref(), None);
            write_action_gamepad_bindings(input, action, &gamepad);
        }
        KeyBindingSlot::Controller => {
            write_action_keyboard_bindings(input, action, primary.as_deref(), secondary.as_deref());
            write_action_gamepad_bindings(input, action, &[]);
        }
    }
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
    for (control, direction) in [
        (slots.up.as_deref(), ScratchDirectionConfig::Up),
        (slots.down.as_deref(), ScratchDirectionConfig::Down),
    ] {
        if let Some(control) = control.filter(|value| !value.is_empty()) {
            let mut entry = gamepad_play_binding(control, lane);
            entry.scratch = Some(direction);
            bindings.push(entry);
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
        KeyBindingTarget::Action { .. } => LaneConfig::Key1,
    }
}

/// 指定スロットへキーボード / コントローラー割り当てを更新する。
pub fn apply_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
    control: &str,
) -> Result<(), super::play_input::InheritError> {
    if let KeyBindingTarget::Action { action, slot } = target {
        apply_action_binding(input, action, slot, control);
        return Ok(());
    }

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
        KeyBindingTarget::Action { .. } => unreachable!("action binding is handled above"),
    }

    persist_bindings(input, key_mode, bindings)
}

/// 指定スロットの割り当てを削除する。
pub fn clear_play_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    target: KeyBindingTarget,
) -> Result<(), super::play_input::InheritError> {
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
        KeyBindingTarget::Action { .. } => unreachable!("action binding is handled above"),
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

    fn action_target(action: InputActionConfig, slot: KeyBindingSlot) -> KeyBindingTarget {
        KeyBindingTarget::Action { action, slot }
    }

    #[test]
    fn lane_label_names_double_play_keys_by_side() {
        assert_eq!(lane_label(LaneConfig::Key1), "KEY 1");
        assert_eq!(lane_label(LaneConfig::Key7), "KEY 7");
        assert_eq!(lane_label(LaneConfig::Key8), "2P KEY 1");
        assert_eq!(lane_label(LaneConfig::Key14), "2P KEY 7");
    }

    #[test]
    fn lane_label_for_key_mode_names_pms_extra_keys() {
        assert_eq!(lane_label_for_key_mode(KeyMode::K8, LaneConfig::Key8), "KEY 8");
        assert_eq!(lane_label_for_key_mode(KeyMode::K9, LaneConfig::Key8), "KEY 8");
        assert_eq!(lane_label_for_key_mode(KeyMode::K9, LaneConfig::Key9), "KEY 9");
        assert_eq!(lane_label_for_key_mode(KeyMode::K14, LaneConfig::Key8), "2P KEY 1");
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
    fn apply_action_binding_keeps_primary_secondary_and_controller_separate() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
            "R",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
            "T",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::Controller),
            "Button10",
        )
        .unwrap();

        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
            ),
            "R"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
            ),
            "T"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                action_target(InputActionConfig::E4, KeyBindingSlot::Controller),
            ),
            "Button10"
        );
    }

    #[test]
    fn clear_action_binding_removes_selected_slot_only() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
            "R",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
            "T",
        )
        .unwrap();

        clear_play_binding(
            &mut profile.input,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
        )
        .unwrap();

        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
            ),
            "R"
        );
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
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
            "Axis1-",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller),
            "Axis1+",
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
            "Axis1-"
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
            "Axis1+"
        );
    }

    #[test]
    fn scratch_controller_keeps_both_directions_with_reversed_axis_polarity() {
        // 軸極性が逆のデバイス: UP に '+'、DOWN に '-' を割り当てても
        // 名前推測で再分類されず、両方向が保持される。
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::Controller),
            "Axis5+",
        )
        .unwrap();
        apply_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller),
            "Axis5-",
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
            "Axis5+"
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
            "Axis5-"
        );
    }

    #[test]
    fn default_scratch_keyboard_shows_separate_keys_for_up_and_down() {
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
            "LControl"
        );
    }
}
