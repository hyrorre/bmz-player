//! プレイモード別入力 binding の inherit 解決。

use std::collections::{BTreeMap, HashMap, HashSet};

use std::fmt;

use bmz_core::input::ScratchDirection;
use bmz_core::lane::{KeyMode, Lane};
use bmz_gameplay::input::backend::{DeviceId, PhysicalControl};
use bmz_gameplay::input::binding::{BindingEntry, LaneBinding};

use super::play::lane_from_config;
use super::profile_config::{
    BindingConfigEntry, LaneConfig, PlayModeInputConfig, ProfileInputConfig, ScratchDirectionConfig,
};
use crate::input::gilrs::GamepadSlotMap;

#[derive(Debug, PartialEq, Eq)]
pub enum InheritError {
    Disallowed { child: KeyMode, parent: KeyMode },
    UnknownKey { key: String },
    Cycle { chain: Vec<KeyMode> },
    RootWithInherit { mode: KeyMode },
}

impl fmt::Display for InheritError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disallowed { child, parent } => {
                write!(f, "inherit from {} to {} is not allowed", parent.as_str(), child.as_str())
            }
            Self::UnknownKey { key } => write!(f, "unknown play map key: {key}"),
            Self::Cycle { chain } => write!(f, "inherit cycle detected: {chain:?}"),
            Self::RootWithInherit { mode } => {
                write!(f, "root mode {} cannot declare inherit", mode.as_str())
            }
        }
    }
}

impl std::error::Error for InheritError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InheritRule {
    FilterOnly,
    Remap(&'static [(Lane, Lane)]),
}

const REMAP_4K: [(Lane, Lane); 4] = [
    (Lane::Key1, Lane::Key1),
    (Lane::Key2, Lane::Key2),
    (Lane::Key3, Lane::Key4),
    (Lane::Key4, Lane::Key5),
];

const REMAP_6K: [(Lane, Lane); 6] = [
    (Lane::Key1, Lane::Key1),
    (Lane::Key2, Lane::Key2),
    (Lane::Key3, Lane::Key3),
    (Lane::Key4, Lane::Key5),
    (Lane::Key5, Lane::Key6),
    (Lane::Key6, Lane::Key7),
];

fn implicit_inherit(child: KeyMode) -> Option<KeyMode> {
    match child {
        KeyMode::K5 | KeyMode::K4 | KeyMode::K6 => Some(KeyMode::K7),
        KeyMode::K10 => Some(KeyMode::K14),
        KeyMode::K7 | KeyMode::K8 | KeyMode::K14 | KeyMode::K9 => None,
    }
}

fn inherit_rule(child: KeyMode, parent: KeyMode) -> Option<InheritRule> {
    match (child, parent) {
        (KeyMode::K5, KeyMode::K7) | (KeyMode::K10, KeyMode::K14) | (KeyMode::K8, KeyMode::K7) => {
            Some(InheritRule::FilterOnly)
        }
        (KeyMode::K4, KeyMode::K7) | (KeyMode::K4, KeyMode::K5) => {
            Some(InheritRule::Remap(&REMAP_4K))
        }
        (KeyMode::K6, KeyMode::K7) => Some(InheritRule::Remap(&REMAP_6K)),
        _ => None,
    }
}

fn is_root_mode(mode: KeyMode) -> bool {
    matches!(mode, KeyMode::K7 | KeyMode::K14 | KeyMode::K9)
}

/// profile 内の明示 inherit 宣言を検証する。
pub fn validate_play_inherit_config(input: &ProfileInputConfig) -> Result<(), InheritError> {
    for (key, config) in &input.play {
        let Some(child) = KeyMode::from_play_map_key(key) else {
            continue;
        };
        if let Some(inherit_key) = config.inherit.as_deref() {
            if is_root_mode(child) {
                return Err(InheritError::RootWithInherit { mode: child });
            }
            let parent = KeyMode::from_play_map_key(inherit_key)
                .ok_or_else(|| InheritError::UnknownKey { key: inherit_key.to_string() })?;
            inherit_rule(child, parent).ok_or(InheritError::Disallowed { child, parent })?;
        }
    }
    Ok(())
}

pub fn lane_binding_for_key_mode(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Result<LaneBinding, InheritError> {
    lane_binding_for_key_mode_with_slots(input, key_mode, GamepadSlotMap::default())
}

pub fn lane_binding_for_key_mode_with_slots(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    slots: GamepadSlotMap,
) -> Result<LaneBinding, InheritError> {
    let bindings = resolve_play_bindings(input, key_mode)?;
    Ok(LaneBinding {
        entries: bindings
            .into_iter()
            .filter_map(|entry| {
                let lane = entry.lane?;
                Some(BindingEntry {
                    device: binding_device_from_config(&entry.device, slots),
                    control: control_from_config(&entry.device, &entry.control),
                    lane: lane_from_config(lane),
                    scratch_direction: scratch_direction_from_binding(lane, &entry),
                })
            })
            .collect(),
    })
}

fn scratch_direction_from_binding(
    lane: LaneConfig,
    entry: &BindingConfigEntry,
) -> Option<ScratchDirection> {
    if !matches!(lane, LaneConfig::Scratch | LaneConfig::Scratch2) {
        return None;
    }
    match entry.scratch {
        Some(ScratchDirectionConfig::Up) => Some(ScratchDirection::Up),
        Some(ScratchDirectionConfig::Down) => Some(ScratchDirection::Down),
        None => infer_scratch_direction_from_control(&entry.control),
    }
}

fn infer_scratch_direction_from_control(control: &str) -> Option<ScratchDirection> {
    if control.contains("ScratchUp") || control.ends_with('-') || control == "Button9" {
        Some(ScratchDirection::Up)
    } else if control.contains("ScratchDown") || control.ends_with('+') || control == "Button8" {
        Some(ScratchDirection::Down)
    } else {
        None
    }
}

pub fn resolve_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    let mut chain = Vec::new();
    resolve_play_bindings_inner(input, key_mode, &mut chain, &mut HashSet::new())
}

fn resolve_play_bindings_inner(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    chain: &mut Vec<KeyMode>,
    visiting: &mut HashSet<KeyMode>,
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    if !visiting.insert(key_mode) {
        chain.push(key_mode);
        return Err(InheritError::Cycle { chain: chain.clone() });
    }
    chain.push(key_mode);

    let play_config = input.play.get(key_mode.play_map_key());
    let explicit_parent = play_config
        .and_then(|config| config.inherit.as_deref())
        .map(|key| {
            KeyMode::from_play_map_key(key)
                .ok_or_else(|| InheritError::UnknownKey { key: key.to_string() })
        })
        .transpose()?;

    if is_root_mode(key_mode) && explicit_parent.is_some() {
        visiting.remove(&key_mode);
        return Err(InheritError::RootWithInherit { mode: key_mode });
    }

    let parent = explicit_parent.or_else(|| implicit_inherit(key_mode));

    let resolved = if let Some(parent_mode) = parent {
        inherit_rule(key_mode, parent_mode)
            .ok_or(InheritError::Disallowed { child: key_mode, parent: parent_mode })?;
        let parent_bindings = resolve_play_bindings_inner(input, parent_mode, chain, visiting)?;
        let mut resolved = apply_inherit(key_mode, parent_mode, &parent_bindings)?;
        if let Some(overrides) = play_config
            .map(|config| config.bindings.as_slice())
            .filter(|bindings| !bindings.is_empty())
        {
            resolved = merge_lane_overrides(resolved, overrides);
        }
        resolved
    } else {
        let own = play_config.map(|config| config.bindings.as_slice()).unwrap_or(&[]);
        if own.is_empty() { default_play_bindings(key_mode) } else { own.to_vec() }
    };

    visiting.remove(&key_mode);
    chain.pop();
    Ok(resolved)
}

fn apply_inherit(
    child: KeyMode,
    parent: KeyMode,
    parent_bindings: &[BindingConfigEntry],
) -> Result<Vec<BindingConfigEntry>, InheritError> {
    let rule = inherit_rule(child, parent).ok_or(InheritError::Disallowed { child, parent })?;
    let parent_by_lane = lane_binding_map(parent_bindings);

    let mut out = match rule {
        InheritRule::FilterOnly => parent_bindings
            .iter()
            .filter(|entry| {
                entry
                    .lane
                    .is_some_and(|lane| child.active_lanes().contains(&lane_from_config(lane)))
            })
            .cloned()
            .collect(),
        InheritRule::Remap(remap) => {
            let mut remapped = Vec::with_capacity(remap.len());
            for &(child_lane, parent_lane) in remap {
                let parent_config = parent_lane_to_config(parent_lane);
                let Some(entry) = parent_by_lane.get(&parent_config) else {
                    continue;
                };
                remapped.push(BindingConfigEntry {
                    device: entry.device.clone(),
                    control: entry.control.clone(),
                    lane: Some(lane_to_config(child_lane)),
                    action: None,
                    scratch: entry.scratch,
                });
            }
            remapped
        }
    };

    if matches!(rule, InheritRule::FilterOnly) {
        out.retain(|entry| {
            entry.lane.is_some_and(|lane| child.active_lanes().contains(&lane_from_config(lane)))
        });
    }

    Ok(out)
}

fn merge_lane_overrides(
    mut base: Vec<BindingConfigEntry>,
    overrides: &[BindingConfigEntry],
) -> Vec<BindingConfigEntry> {
    for override_entry in overrides {
        let Some(lane) = override_entry.lane else {
            continue;
        };
        base.retain(|entry| entry.lane != Some(lane));
        base.push(override_entry.clone());
    }
    base
}

fn lane_binding_map(bindings: &[BindingConfigEntry]) -> HashMap<LaneConfig, BindingConfigEntry> {
    let mut map = HashMap::new();
    for entry in bindings {
        let Some(lane) = entry.lane else { continue };
        match map.get(&lane) {
            None => {
                map.insert(lane, entry.clone());
            }
            Some(existing) if existing.device != "keyboard" && entry.device == "keyboard" => {
                map.insert(lane, entry.clone());
            }
            _ => {}
        }
    }
    map
}

fn parent_lane_to_config(lane: Lane) -> LaneConfig {
    match lane {
        Lane::Scratch => LaneConfig::Scratch,
        Lane::Key1 => LaneConfig::Key1,
        Lane::Key2 => LaneConfig::Key2,
        Lane::Key3 => LaneConfig::Key3,
        Lane::Key4 => LaneConfig::Key4,
        Lane::Key5 => LaneConfig::Key5,
        Lane::Key6 => LaneConfig::Key6,
        Lane::Key7 => LaneConfig::Key7,
        Lane::Scratch2 => LaneConfig::Scratch2,
        Lane::Key8 => LaneConfig::Key8,
        Lane::Key9 => LaneConfig::Key9,
        Lane::Key10 => LaneConfig::Key10,
        Lane::Key11 => LaneConfig::Key11,
        Lane::Key12 => LaneConfig::Key12,
        Lane::Key13 => LaneConfig::Key13,
        Lane::Key14 => LaneConfig::Key14,
    }
}

fn lane_to_config(lane: Lane) -> LaneConfig {
    parent_lane_to_config(lane)
}

pub fn is_gamepad_device(device: &str) -> bool {
    gamepad_player_index(device).is_some() || device.trim().eq_ignore_ascii_case("gamepad")
}

fn binding_device_from_config(device: &str, slots: GamepadSlotMap) -> Option<DeviceId> {
    gamepad_player_index(device).and_then(|index| slots.device_id_for_player(index))
}

pub fn gamepad_player_index(device: &str) -> Option<u32> {
    let lower = device.trim().to_ascii_lowercase();
    let suffix = lower.strip_prefix("gamepad")?;
    if suffix.is_empty() {
        return None;
    }
    suffix.parse::<u32>().ok().filter(|index| *index > 0)
}

fn control_from_config(device: &str, control: &str) -> PhysicalControl {
    match device.to_ascii_lowercase().as_str() {
        device if is_gamepad_device(device) => PhysicalControl::GamepadButton(control.to_string()),
        "hid" => control
            .parse::<u32>()
            .map(PhysicalControl::HidButton)
            .unwrap_or_else(|_| PhysicalControl::KeyboardKey(control.to_string())),
        _ => PhysicalControl::KeyboardKey(control.to_string()),
    }
}

pub fn default_play_bindings(key_mode: KeyMode) -> Vec<BindingConfigEntry> {
    match key_mode {
        KeyMode::K7 => default_play_7k_bindings(),
        KeyMode::K8 => default_play_8k_bindings(),
        KeyMode::K14 => default_play_14k_bindings(),
        KeyMode::K9 => default_play_9k_bindings(),
        KeyMode::K5 | KeyMode::K4 | KeyMode::K6 | KeyMode::K10 => Vec::new(),
    }
}

pub fn default_play_7k_bindings() -> Vec<BindingConfigEntry> {
    let mut bindings = default_play_7k_keyboard_bindings();
    bindings.extend(default_play_7k_gamepad_bindings());
    bindings
}

pub fn default_play_7k_keyboard_bindings() -> Vec<BindingConfigEntry> {
    vec![
        scratch_play_binding("LShift", LaneConfig::Scratch, ScratchDirectionConfig::Up),
        scratch_play_binding("LControl", LaneConfig::Scratch, ScratchDirectionConfig::Down),
        play_binding("Z", LaneConfig::Key1),
        play_binding("S", LaneConfig::Key2),
        play_binding("X", LaneConfig::Key3),
        play_binding("D", LaneConfig::Key4),
        play_binding("C", LaneConfig::Key5),
        play_binding("F", LaneConfig::Key6),
        play_binding("V", LaneConfig::Key7),
    ]
}

pub fn default_play_7k_gamepad_bindings() -> Vec<BindingConfigEntry> {
    vec![
        gamepad_scratch_play_binding_for_device(
            "gamepad",
            "Axis1+",
            LaneConfig::Scratch,
            ScratchDirectionConfig::Up,
        ),
        gamepad_scratch_play_binding_for_device(
            "gamepad",
            "Axis1-",
            LaneConfig::Scratch,
            ScratchDirectionConfig::Down,
        ),
        gamepad_play_binding("Button1", LaneConfig::Key1),
        gamepad_play_binding("Button2", LaneConfig::Key2),
        gamepad_play_binding("Button3", LaneConfig::Key3),
        gamepad_play_binding("Button4", LaneConfig::Key4),
        gamepad_play_binding("Button5", LaneConfig::Key5),
        gamepad_play_binding("Button6", LaneConfig::Key6),
        gamepad_play_binding("Button7", LaneConfig::Key7),
    ]
}

pub fn default_play_14k_bindings() -> Vec<BindingConfigEntry> {
    let mut bindings = vec![
        scratch_play_binding("LShift", LaneConfig::Scratch, ScratchDirectionConfig::Up),
        scratch_play_binding("LControl", LaneConfig::Scratch, ScratchDirectionConfig::Down),
        play_binding("Z", LaneConfig::Key1),
        play_binding("S", LaneConfig::Key2),
        play_binding("X", LaneConfig::Key3),
        play_binding("D", LaneConfig::Key4),
        play_binding("C", LaneConfig::Key5),
        play_binding("F", LaneConfig::Key6),
        play_binding("V", LaneConfig::Key7),
        scratch_play_binding("RShift", LaneConfig::Scratch2, ScratchDirectionConfig::Up),
        scratch_play_binding("RControl", LaneConfig::Scratch2, ScratchDirectionConfig::Down),
        play_binding("M", LaneConfig::Key8),
        play_binding("K", LaneConfig::Key9),
        play_binding("Comma", LaneConfig::Key10),
        play_binding("L", LaneConfig::Key11),
        play_binding("Period", LaneConfig::Key12),
        play_binding("Semicolon", LaneConfig::Key13),
        play_binding("Slash", LaneConfig::Key14),
    ];
    bindings.extend([
        gamepad_scratch_play_binding_for_device(
            "gamepad1",
            "Axis1+",
            LaneConfig::Scratch,
            ScratchDirectionConfig::Up,
        ),
        gamepad_scratch_play_binding_for_device(
            "gamepad1",
            "Axis1-",
            LaneConfig::Scratch,
            ScratchDirectionConfig::Down,
        ),
        gamepad_play_binding_for_device("gamepad1", "Button1", LaneConfig::Key1),
        gamepad_play_binding_for_device("gamepad1", "Button2", LaneConfig::Key2),
        gamepad_play_binding_for_device("gamepad1", "Button3", LaneConfig::Key3),
        gamepad_play_binding_for_device("gamepad1", "Button4", LaneConfig::Key4),
        gamepad_play_binding_for_device("gamepad1", "Button5", LaneConfig::Key5),
        gamepad_play_binding_for_device("gamepad1", "Button6", LaneConfig::Key6),
        gamepad_play_binding_for_device("gamepad1", "Button7", LaneConfig::Key7),
    ]);
    bindings.extend([
        gamepad_scratch_play_binding_for_device(
            "gamepad2",
            "Axis1-",
            LaneConfig::Scratch2,
            ScratchDirectionConfig::Up,
        ),
        gamepad_scratch_play_binding_for_device(
            "gamepad2",
            "Axis1+",
            LaneConfig::Scratch2,
            ScratchDirectionConfig::Down,
        ),
        gamepad_play_binding_for_device("gamepad2", "Button1", LaneConfig::Key8),
        gamepad_play_binding_for_device("gamepad2", "Button2", LaneConfig::Key9),
        gamepad_play_binding_for_device("gamepad2", "Button3", LaneConfig::Key10),
        gamepad_play_binding_for_device("gamepad2", "Button4", LaneConfig::Key11),
        gamepad_play_binding_for_device("gamepad2", "Button5", LaneConfig::Key12),
        gamepad_play_binding_for_device("gamepad2", "Button6", LaneConfig::Key13),
        gamepad_play_binding_for_device("gamepad2", "Button7", LaneConfig::Key14),
    ]);
    bindings
}

pub fn default_play_9k_bindings() -> Vec<BindingConfigEntry> {
    vec![
        play_binding("Z", LaneConfig::Key1),
        play_binding("S", LaneConfig::Key2),
        play_binding("X", LaneConfig::Key3),
        play_binding("D", LaneConfig::Key4),
        play_binding("C", LaneConfig::Key5),
        play_binding("F", LaneConfig::Key6),
        play_binding("V", LaneConfig::Key7),
        play_binding("G", LaneConfig::Key8),
        play_binding("B", LaneConfig::Key9),
    ]
}

pub fn default_play_8k_bindings() -> Vec<BindingConfigEntry> {
    vec![
        play_binding("Z", LaneConfig::Key1),
        play_binding("S", LaneConfig::Key2),
        play_binding("X", LaneConfig::Key3),
        play_binding("D", LaneConfig::Key4),
        play_binding("C", LaneConfig::Key5),
        play_binding("F", LaneConfig::Key6),
        play_binding("V", LaneConfig::Key7),
        play_binding("G", LaneConfig::Key8),
    ]
}

pub fn play_binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "keyboard".to_string(),
        control: control.to_string(),
        lane: Some(lane),
        action: None,
        scratch: None,
    }
}

pub fn scratch_play_binding(
    control: &str,
    lane: LaneConfig,
    scratch: ScratchDirectionConfig,
) -> BindingConfigEntry {
    let mut entry = play_binding(control, lane);
    entry.scratch = Some(scratch);
    entry
}

pub fn gamepad_play_binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    gamepad_play_binding_for_device("gamepad", control, lane)
}

pub fn gamepad_play_binding_for_device(
    device: &str,
    control: &str,
    lane: LaneConfig,
) -> BindingConfigEntry {
    BindingConfigEntry {
        device: device.to_string(),
        control: control.to_string(),
        lane: Some(lane),
        action: None,
        scratch: None,
    }
}

pub fn gamepad_scratch_play_binding_for_device(
    device: &str,
    control: &str,
    lane: LaneConfig,
    scratch: ScratchDirectionConfig,
) -> BindingConfigEntry {
    let mut entry = gamepad_play_binding_for_device(device, control, lane);
    entry.scratch = Some(scratch);
    entry
}

pub fn normalize_profile_input(input: &mut ProfileInputConfig) {
    if !input.legacy_bindings.is_empty() {
        let (ui, play) = migrate_legacy_bindings(&input.legacy_bindings);
        if input.ui.bindings.is_empty() && !ui.is_empty() {
            input.ui.bindings = ui;
        }
        if input.play.is_empty() && !play.is_empty() {
            input.play = play;
        }
        input.legacy_bindings.clear();
    }
    normalize_play_map_keys(&mut input.play);
    if input.ui.bindings.is_empty() {
        input.ui.bindings = super::profile_config::default_ui_bindings();
    }
}

pub fn default_profile_input() -> ProfileInputConfig {
    let mut play = BTreeMap::new();
    play.insert(
        KeyMode::K7.play_map_key().to_string(),
        PlayModeInputConfig { inherit: None, bindings: default_play_7k_bindings() },
    );
    ProfileInputConfig {
        scratch_mode: super::profile_config::ScratchInputMode::Normal,
        select_input_mode: super::profile_config::SelectInputModeConfig::Key7Key14,
        start_key: None,
        ui: super::profile_config::UiInputConfig {
            bindings: super::profile_config::default_ui_bindings(),
        },
        play,
        legacy_bindings: Vec::new(),
        analog_scratch_sensitivity: 1.0,
        analog_scratch_timeout_ms: 500,
        analog_scratch_threshold: 100,
        analog_ticks_per_scroll: 3,
    }
}

pub fn normalize_play_map_keys(play: &mut BTreeMap<String, PlayModeInputConfig>) {
    let old = std::mem::take(play);
    for (key, value) in old {
        play.insert(normalize_play_map_key(&key), value);
    }
}

pub fn normalize_play_map_key(key: &str) -> String {
    key.trim().to_ascii_lowercase()
}

pub fn migrate_legacy_bindings(
    legacy: &[BindingConfigEntry],
) -> (Vec<BindingConfigEntry>, BTreeMap<String, PlayModeInputConfig>) {
    let mut ui_bindings = Vec::new();
    let mut play_7k = Vec::new();
    let mut play_14k = Vec::new();

    for entry in legacy {
        if entry.action.is_some() {
            ui_bindings.push(entry.clone());
            continue;
        }
        let Some(lane) = entry.lane else { continue };
        match lane {
            LaneConfig::Scratch
            | LaneConfig::Key1
            | LaneConfig::Key2
            | LaneConfig::Key3
            | LaneConfig::Key4
            | LaneConfig::Key5
            | LaneConfig::Key6
            | LaneConfig::Key7 => play_7k.push(entry.clone()),
            LaneConfig::Scratch2
            | LaneConfig::Key8
            | LaneConfig::Key9
            | LaneConfig::Key10
            | LaneConfig::Key11
            | LaneConfig::Key12
            | LaneConfig::Key13
            | LaneConfig::Key14 => play_14k.push(entry.clone()),
        }
    }

    let mut play = BTreeMap::new();
    if !play_7k.is_empty() {
        play.insert(
            KeyMode::K7.play_map_key().to_string(),
            PlayModeInputConfig { inherit: None, bindings: play_7k },
        );
    }
    if !play_14k.is_empty() {
        play.insert(
            KeyMode::K14.play_map_key().to_string(),
            PlayModeInputConfig { inherit: None, bindings: play_14k },
        );
    }
    (ui_bindings, play)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{
        ProfileInputConfig, SelectInputModeConfig, UiInputConfig, default_ui_bindings,
    };

    fn sample_7k_input() -> ProfileInputConfig {
        let mut play = BTreeMap::new();
        play.insert(
            "7k".to_string(),
            PlayModeInputConfig { inherit: None, bindings: default_play_7k_bindings() },
        );
        ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            select_input_mode: SelectInputModeConfig::Key7Key14,
            start_key: None,
            ui: UiInputConfig { bindings: default_ui_bindings() },
            play,
            legacy_bindings: Vec::new(),
            analog_scratch_sensitivity: 1.0,
            analog_scratch_timeout_ms: 500,
            analog_scratch_threshold: 100,
            analog_ticks_per_scroll: 3,
        }
    }

    #[test]
    fn five_k_inherits_seven_k_without_config() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K5).unwrap();
        let lanes: HashSet<_> = bindings.iter().filter_map(|e| e.lane).collect();
        assert!(lanes.contains(&LaneConfig::Scratch));
        assert!(lanes.contains(&LaneConfig::Key5));
        assert!(!lanes.contains(&LaneConfig::Key6));
    }

    #[test]
    fn ten_k_inherits_fourteen_k_without_config() {
        let mut play = BTreeMap::new();
        play.insert(
            "14k".to_string(),
            PlayModeInputConfig { inherit: None, bindings: default_play_14k_bindings() },
        );
        let input = ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            select_input_mode: SelectInputModeConfig::Key7Key14,
            start_key: None,
            ui: UiInputConfig::default(),
            play,
            legacy_bindings: Vec::new(),
            analog_scratch_sensitivity: 1.0,
            analog_scratch_timeout_ms: 500,
            analog_scratch_threshold: 100,
            analog_ticks_per_scroll: 3,
        };
        let bindings = resolve_play_bindings(&input, KeyMode::K10).unwrap();
        assert!(bindings.iter().any(|e| e.lane == Some(LaneConfig::Key8)));
        assert!(!bindings.iter().any(|e| e.lane == Some(LaneConfig::Key6)));
    }

    #[test]
    fn four_k_remaps_parent_lanes() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K4).unwrap();
        let key = |lane: LaneConfig| {
            bindings
                .iter()
                .filter(|entry| entry.device == "keyboard")
                .find(|entry| entry.lane == Some(lane))
                .map(|entry| entry.control.as_str())
                .unwrap()
        };
        assert_eq!(key(LaneConfig::Key1), "Z");
        assert_eq!(key(LaneConfig::Key2), "S");
        assert_eq!(key(LaneConfig::Key3), "D");
        assert_eq!(key(LaneConfig::Key4), "C");
    }

    #[test]
    fn six_k_remaps_parent_lanes() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K6).unwrap();
        let key = |lane: LaneConfig| {
            bindings
                .iter()
                .filter(|entry| entry.device == "keyboard")
                .find(|entry| entry.lane == Some(lane))
                .map(|entry| entry.control.as_str())
                .unwrap()
        };
        assert_eq!(key(LaneConfig::Key4), "C");
        assert_eq!(key(LaneConfig::Key5), "F");
        assert_eq!(key(LaneConfig::Key6), "V");
    }

    #[test]
    fn eight_k_uses_scratchless_default_lanes() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K8).unwrap();
        assert!(!bindings.iter().any(|e| e.lane == Some(LaneConfig::Scratch)));
        assert!(bindings.iter().any(|e| e.lane == Some(LaneConfig::Key1)));
        assert!(bindings.iter().any(|e| e.lane == Some(LaneConfig::Key8)));
    }

    #[test]
    fn nine_k_does_not_inherit_seven_k() {
        let input = sample_7k_input();
        assert!(resolve_play_bindings(&input, KeyMode::K9).is_ok());
        let mut play = input.play.clone();
        play.insert(
            "9k".to_string(),
            PlayModeInputConfig { inherit: Some("7k".into()), bindings: Vec::new() },
        );
        let input = ProfileInputConfig { play, ..input };
        assert_eq!(
            validate_play_inherit_config(&input),
            Err(InheritError::RootWithInherit { mode: KeyMode::K9 })
        );
    }

    #[test]
    fn four_k_inherit_five_k_allowed() {
        let input = sample_7k_input();
        let mut play = input.play.clone();
        play.insert(
            "4k".to_string(),
            PlayModeInputConfig { inherit: Some("5k".into()), bindings: Vec::new() },
        );
        let input = ProfileInputConfig { play, ..input };
        validate_play_inherit_config(&input).unwrap();
        let bindings = resolve_play_bindings(&input, KeyMode::K4).unwrap();
        assert_eq!(bindings.len(), 4);
    }

    #[test]
    fn six_k_inherit_five_k_rejected() {
        let mut play = BTreeMap::new();
        play.insert(
            "6k".to_string(),
            PlayModeInputConfig { inherit: Some("5k".into()), bindings: Vec::new() },
        );
        let input = ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            select_input_mode: SelectInputModeConfig::Key7Key14,
            start_key: None,
            ui: UiInputConfig::default(),
            play,
            legacy_bindings: Vec::new(),
            analog_scratch_sensitivity: 1.0,
            analog_scratch_timeout_ms: 500,
            analog_scratch_threshold: 100,
            analog_ticks_per_scroll: 3,
        };
        assert_eq!(
            validate_play_inherit_config(&input),
            Err(InheritError::Disallowed { child: KeyMode::K6, parent: KeyMode::K5 })
        );
    }

    #[test]
    fn migrate_legacy_splits_ui_and_play() {
        let legacy = crate::config::profile_config::default_bindings();
        let (ui, play) = migrate_legacy_bindings(&legacy);
        assert!(ui.iter().any(|e| e.action.is_some()));
        assert!(play.contains_key("7k"));
    }

    #[test]
    fn gamepad_numbered_devices_resolve_to_specific_device_ids() {
        let mut input = sample_7k_input();
        input.play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![
                    gamepad_play_binding_for_device("gamepad1", "Button1", LaneConfig::Key1),
                    gamepad_play_binding_for_device("gamepad2", "Button1", LaneConfig::Key8),
                ],
            },
        );

        let binding = lane_binding_for_key_mode(&input, KeyMode::K14).unwrap();

        assert_eq!(
            binding.resolve(DeviceId(16), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
        assert_eq!(
            binding.resolve(DeviceId(17), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key8)
        );
        assert_eq!(
            binding.resolve(DeviceId(18), &PhysicalControl::GamepadButton("Button1".into())),
            None
        );
    }

    #[test]
    fn gamepad_slot_map_remaps_logical_players_to_assigned_gilrs_ids() {
        let mut input = sample_7k_input();
        input.play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![
                    gamepad_play_binding_for_device("gamepad1", "Button1", LaneConfig::Key1),
                    gamepad_play_binding_for_device("gamepad2", "Button1", LaneConfig::Key8),
                ],
            },
        );

        // Swap: logical 1P → gilrs id 1 (DeviceId 17), logical 2P → gilrs id 0 (DeviceId 16)
        let slots = GamepadSlotMap::from_slot_ids([Some(1), Some(0)]);
        let binding = lane_binding_for_key_mode_with_slots(&input, KeyMode::K14, slots).unwrap();

        assert_eq!(
            binding.resolve(DeviceId(17), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
        assert_eq!(
            binding.resolve(DeviceId(16), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key8)
        );
    }

    #[test]
    fn numbered_gamepads_above_two_remain_device_specific() {
        let mut input = sample_7k_input();
        input.play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![gamepad_play_binding_for_device(
                    "gamepad3",
                    "Button1",
                    LaneConfig::Key1,
                )],
            },
        );

        let binding = lane_binding_for_key_mode(&input, KeyMode::K14).unwrap();
        let control = PhysicalControl::GamepadButton("Button1".into());
        assert_eq!(binding.resolve(DeviceId(18), &control), Some(Lane::Key1));
        assert_eq!(binding.resolve(DeviceId(16), &control), None);
    }

    #[test]
    fn gamepad_wildcard_still_matches_any_gamepad_device() {
        let input = sample_7k_input();
        let binding = lane_binding_for_key_mode(&input, KeyMode::K7).unwrap();

        assert_eq!(
            binding.resolve(DeviceId(16), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
        assert_eq!(
            binding.resolve(DeviceId(17), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
    }

    #[test]
    fn default_fourteen_k_gamepad_uses_two_numbered_devices() {
        let bindings = default_play_14k_bindings();

        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad1"
                && entry.control == "Button1"
                && entry.lane == Some(LaneConfig::Key1)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad1"
                && entry.control == "Axis1+"
                && entry.lane == Some(LaneConfig::Scratch)
                && entry.scratch == Some(ScratchDirectionConfig::Up)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad1"
                && entry.control == "Axis1-"
                && entry.lane == Some(LaneConfig::Scratch)
                && entry.scratch == Some(ScratchDirectionConfig::Down)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad2"
                && entry.control == "Button1"
                && entry.lane == Some(LaneConfig::Key8)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad2"
                && entry.control == "Axis1-"
                && entry.lane == Some(LaneConfig::Scratch2)
                && entry.scratch == Some(ScratchDirectionConfig::Up)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad2"
                && entry.control == "Axis1+"
                && entry.lane == Some(LaneConfig::Scratch2)
                && entry.scratch == Some(ScratchDirectionConfig::Down)
        }));
        assert!(
            !bindings
                .iter()
                .any(|entry| { entry.device == "gamepad" && entry.control == "Button14" })
        );
    }
}
