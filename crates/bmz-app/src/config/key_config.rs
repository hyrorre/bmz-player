use bmz_core::lane::KeyMode;

use super::play::{lane_from_config, lane_to_config};
use super::play_input::{default_play_bindings, play_binding, resolve_play_bindings};
use super::profile_config::{
    BindingConfigEntry, LaneConfig, PlayModeInputConfig, ProfileConfig, ProfileInputConfig,
};

/// 選曲画面のキー設定で編集対象とする KEY モード。
pub const KEY_CONFIG_MODES: &[KeyMode] = &[KeyMode::K5, KeyMode::K7, KeyMode::K10, KeyMode::K14];

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

pub fn format_play_keyboard_binding(
    profile: &ProfileConfig,
    key_mode: KeyMode,
    lane: LaneConfig,
) -> String {
    format_keyboard_controls(&resolved_play_bindings(&profile.input, key_mode), lane)
}

fn resolved_play_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
) -> Vec<BindingConfigEntry> {
    resolve_play_bindings(input, key_mode).unwrap_or_else(|_| default_play_bindings(key_mode))
}

fn format_keyboard_controls(bindings: &[BindingConfigEntry], lane: LaneConfig) -> String {
    let keys: Vec<&str> = bindings
        .iter()
        .filter(|entry| entry.device == "keyboard" && entry.lane == Some(lane))
        .map(|entry| entry.control.as_str())
        .collect();
    if keys.is_empty() { "(none)".to_string() } else { keys.join(" / ") }
}

/// 指定 KEY モードのキーボード割り当てを更新し、解決済み bindings を profile に書き戻す。
pub fn apply_play_keyboard_binding(
    input: &mut ProfileInputConfig,
    key_mode: KeyMode,
    lane: LaneConfig,
    control: &str,
) -> Result<(), super::play_input::InheritError> {
    if !key_mode.active_lanes().contains(&lane_from_config(lane)) {
        return Ok(());
    }

    let mut bindings = resolve_play_bindings(input, key_mode)?;
    bindings.retain(|entry| !(entry.device == "keyboard" && entry.control == control));
    bindings.retain(|entry| !(entry.device == "keyboard" && entry.lane == Some(lane)));
    bindings.push(play_binding(control, lane));

    let config = ensure_play_mode_config(input, key_mode);
    config.inherit = None;
    config.bindings = bindings;
    Ok(())
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
    fn apply_play_keyboard_binding_moves_duplicate_key() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_keyboard_binding(&mut profile.input, KeyMode::K7, LaneConfig::Key1, "Q")
            .unwrap();
        apply_play_keyboard_binding(&mut profile.input, KeyMode::K7, LaneConfig::Key2, "Q")
            .unwrap();
        assert_eq!(format_play_keyboard_binding(&profile, KeyMode::K7, LaneConfig::Key1), "(none)");
        assert_eq!(format_play_keyboard_binding(&profile, KeyMode::K7, LaneConfig::Key2), "Q");
    }

    #[test]
    fn apply_play_keyboard_binding_replaces_lane_key() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_keyboard_binding(&mut profile.input, KeyMode::K7, LaneConfig::Scratch, "Space")
            .unwrap();
        assert_eq!(
            format_play_keyboard_binding(&profile, KeyMode::K7, LaneConfig::Scratch),
            "Space"
        );
    }

    #[test]
    fn apply_play_keyboard_binding_isolated_per_key_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_keyboard_binding(&mut profile.input, KeyMode::K7, LaneConfig::Key1, "Q")
            .unwrap();
        apply_play_keyboard_binding(&mut profile.input, KeyMode::K14, LaneConfig::Key1, "W")
            .unwrap();
        assert_eq!(format_play_keyboard_binding(&profile, KeyMode::K7, LaneConfig::Key1), "Q");
        assert_eq!(format_play_keyboard_binding(&profile, KeyMode::K14, LaneConfig::Key1), "W");
    }
}
