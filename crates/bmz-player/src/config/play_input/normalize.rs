use super::*;
use crate::config::profile_config::{GamepadScratchConfig, InputActionConfig};

pub fn normalize_profile_input(input: &mut ProfileInputConfig) {
    migrate_legacy_analog_scratch_config(input);
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
        input.ui.bindings = crate::config::profile_config::default_ui_bindings();
    }
    migrate_configurable_shortcut_bindings(&mut input.ui);
}

fn migrate_configurable_shortcut_bindings(ui: &mut crate::config::profile_config::UiInputConfig) {
    use crate::config::profile_config::{
        CONFIGURABLE_SHORTCUT_MIGRATIONS, UI_INPUT_BINDING_VERSION,
        default_configurable_shortcut_bindings,
    };

    if ui.version >= UI_INPUT_BINDING_VERSION {
        return;
    }
    let defaults = default_configurable_shortcut_bindings();
    for &(version, actions) in CONFIGURABLE_SHORTCUT_MIGRATIONS {
        if ui.version >= version {
            continue;
        }
        for &action in actions {
            if ui.bindings.iter().any(|entry| entry.action == Some(action)) {
                continue;
            }
            ui.bindings
                .extend(defaults.iter().filter(|entry| entry.action == Some(action)).cloned());
        }
    }
    if ui.version < 3 {
        migrate_select_digit_shortcuts(ui, &defaults);
    }
    if ui.version < 5 {
        let keyboard_defaults = crate::config::profile_config::default_keyboard_bindings();
        normalize_select_digit_defaults(ui, &keyboard_defaults);
    }
    ui.version = UI_INPUT_BINDING_VERSION;
}

fn migrate_select_digit_shortcuts(
    ui: &mut crate::config::profile_config::UiInputConfig,
    defaults: &[BindingConfigEntry],
) {
    use InputActionConfig::{SelectOpenDocuments, SelectReplayCycle, SelectSameFolder};

    // ReplayCycle / SameFolder previously had a runtime Numpad fallback. Materialize
    // those defaults while adding the top-row binding so the legacy fallback
    // remains available after the fallback is removed.
    for (action, top_row, numpad) in
        [(SelectReplayCycle, "4", "Numpad4"), (SelectSameFolder, "8", "Numpad8")]
    {
        let has_action = ui.bindings.iter().any(|entry| entry.action == Some(action));
        let has_legacy_default = ui.bindings.iter().any(|entry| {
            entry.action == Some(action) && entry.device == "keyboard" && entry.control == numpad
        });
        if !has_action || has_legacy_default {
            add_default_shortcut(ui, defaults, action, top_row);
        }
    }

    // OpenDocuments was already a fully configurable action. Add the top-row
    // companion only when the old Numpad9 default is still present, preserving a
    // deliberate clear or replacement made on an older profile.
    let has_legacy_documents_default = ui.bindings.iter().any(|entry| {
        entry.action == Some(SelectOpenDocuments)
            && entry.device == "keyboard"
            && entry.control == "Numpad9"
    });
    if has_legacy_documents_default {
        add_default_shortcut(ui, defaults, SelectOpenDocuments, "9");
    }
}

/// Consolidate legacy top-row/keypad pairs to the top-row binding.  A custom
/// binding is left untouched, while a legacy keypad-only default is converted
/// to the canonical top-row key.
fn normalize_select_digit_defaults(
    ui: &mut crate::config::profile_config::UiInputConfig,
    defaults: &[BindingConfigEntry],
) {
    use InputActionConfig::{
        SelectDifficultyFilter, SelectLnMode, SelectModeFilter, SelectOpenDocuments,
        SelectOpenKeyConfig, SelectReplayCycle, SelectReplayPlay, SelectRivalCycle,
        SelectSameFolder, SelectSort,
    };

    for (action, top_row, numpad) in [
        (SelectDifficultyFilter, "0", "Numpad0"),
        (SelectModeFilter, "1", "Numpad1"),
        (SelectSort, "2", "Numpad2"),
        (SelectLnMode, "3", "Numpad3"),
        (SelectReplayCycle, "4", "Numpad4"),
        (SelectReplayPlay, "5", "Numpad5"),
        (SelectOpenKeyConfig, "6", "Numpad6"),
        (SelectRivalCycle, "7", "Numpad7"),
        (SelectSameFolder, "8", "Numpad8"),
        (SelectOpenDocuments, "9", "Numpad9"),
    ] {
        let controls: Vec<String> = ui
            .bindings
            .iter()
            .filter(|entry| entry.action == Some(action) && entry.device == "keyboard")
            .map(|entry| entry.control.clone())
            .collect();

        let keypad_only = controls.len() == 1 && controls[0] == numpad;
        let top_and_keypad = controls.len() == 2
            && controls.iter().any(|control| control == top_row)
            && controls.iter().any(|control| control == numpad);
        if keypad_only {
            add_default_shortcut(ui, defaults, action, top_row);
        }
        if keypad_only || top_and_keypad {
            ui.bindings.retain(|entry| {
                !(entry.action == Some(action)
                    && entry.device == "keyboard"
                    && entry.control == numpad)
            });
        }
    }
}

fn add_default_shortcut(
    ui: &mut crate::config::profile_config::UiInputConfig,
    defaults: &[BindingConfigEntry],
    action: InputActionConfig,
    control: &str,
) {
    if ui.bindings.iter().any(|entry| {
        entry.action == Some(action) && entry.device == "keyboard" && entry.control == control
    }) {
        return;
    }
    if let Some(entry) = defaults.iter().find(|entry| {
        entry.action == Some(action) && entry.device == "keyboard" && entry.control == control
    }) {
        ui.bindings.push(entry.clone());
    }
}

fn migrate_legacy_analog_scratch_config(input: &mut ProfileInputConfig) {
    let nested_is_default = input.gamepad1 == GamepadScratchConfig::default()
        && input.gamepad2 == GamepadScratchConfig::default();
    if nested_is_default {
        if let Some(sensitivity) = input.legacy_analog_scratch_sensitivity {
            input.gamepad1.analog_scratch_sensitivity = sensitivity;
            input.gamepad2.analog_scratch_sensitivity = sensitivity;
        }
        if let Some(threshold) = input.legacy_analog_scratch_threshold {
            input.gamepad1.analog_scratch_threshold = threshold;
            input.gamepad2.analog_scratch_threshold = threshold;
        }
    }
    input.legacy_analog_scratch_sensitivity = None;
    input.legacy_analog_scratch_threshold = None;
}

pub fn default_profile_input() -> ProfileInputConfig {
    let mut play = BTreeMap::new();
    play.insert(
        KeyMode::K7.play_map_key().to_string(),
        PlayModeInputConfig {
            inherit: None,
            bindings: default_play_7k_bindings(),
            ..Default::default()
        },
    );
    ProfileInputConfig {
        scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
        select_input_mode: crate::config::profile_config::SelectInputModeConfig::Key7Key14,
        start_key: None,
        ui: crate::config::profile_config::UiInputConfig {
            version: crate::config::profile_config::UI_INPUT_BINDING_VERSION,
            bindings: crate::config::profile_config::default_ui_bindings(),
        },
        play,
        legacy_bindings: Vec::new(),
        legacy_analog_scratch_sensitivity: None,
        analog_scratch_timeout_ms: 500,
        legacy_analog_scratch_threshold: None,
        gamepad1: GamepadScratchConfig::default(),
        gamepad2: GamepadScratchConfig::default(),
        analog_ticks_per_scroll: 3,
        lane_cover_step: crate::config::profile_config::default_lane_cover_step(),
        lane_cover_hold_delay_ms: crate::config::profile_config::default_lane_cover_hold_delay_ms(),
        lane_cover_hold_step: crate::config::profile_config::default_lane_cover_hold_step(),
        lane_cover_wheel_step: crate::config::profile_config::default_lane_cover_wheel_step(),
        keyboard_release_bounce_ms: 0,
        controller_release_bounce_ms: 0,
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
            PlayModeInputConfig { inherit: None, bindings: play_7k, ..Default::default() },
        );
    }
    if !play_14k.is_empty() {
        play.insert(
            KeyMode::K14.play_map_key().to_string(),
            PlayModeInputConfig { inherit: None, bindings: play_14k, ..Default::default() },
        );
    }
    (ui_bindings, play)
}
