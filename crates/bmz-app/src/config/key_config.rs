use bmz_core::lane::KeyMode;

use super::play_input::{default_play_bindings, play_binding, resolve_play_bindings};
use super::profile_config::{
    BindingConfigEntry, LaneConfig, PlayModeInputConfig, ProfileConfig, ProfileInputConfig,
};

/// 選曲画面のキー設定で編集する 7KEY レーン一覧。
pub const KEY7_LANE_ENTRIES: &[(LaneConfig, &'static str)] = &[
    (LaneConfig::Scratch, "SCRATCH"),
    (LaneConfig::Key1, "KEY 1"),
    (LaneConfig::Key2, "KEY 2"),
    (LaneConfig::Key3, "KEY 3"),
    (LaneConfig::Key4, "KEY 4"),
    (LaneConfig::Key5, "KEY 5"),
    (LaneConfig::Key6, "KEY 6"),
    (LaneConfig::Key7, "KEY 7"),
];

pub fn format_play_keyboard_binding(profile: &ProfileConfig, lane: LaneConfig) -> String {
    format_keyboard_controls(&resolved_play_bindings(&profile.input), lane)
}

fn resolved_play_bindings(input: &ProfileInputConfig) -> Vec<BindingConfigEntry> {
    resolve_play_bindings(input, KeyMode::K7).unwrap_or_else(|_| default_play_bindings(KeyMode::K7))
}

fn format_keyboard_controls(bindings: &[BindingConfigEntry], lane: LaneConfig) -> String {
    let keys: Vec<&str> = bindings
        .iter()
        .filter(|entry| entry.device == "keyboard" && entry.lane == Some(lane))
        .map(|entry| entry.control.as_str())
        .collect();
    if keys.is_empty() { "(none)".to_string() } else { keys.join(" / ") }
}

/// 7KEY のキーボード割り当てを更新し、解決済み bindings を profile に書き戻す。
pub fn apply_play_keyboard_binding(
    input: &mut ProfileInputConfig,
    lane: LaneConfig,
    control: &str,
) -> Result<(), super::play_input::InheritError> {
    let mut bindings = resolve_play_bindings(input, KeyMode::K7)?;
    bindings.retain(|entry| !(entry.device == "keyboard" && entry.control == control));
    bindings.retain(|entry| !(entry.device == "keyboard" && entry.lane == Some(lane)));
    bindings.push(play_binding(control, lane));

    let config = ensure_play_mode_config(input, KeyMode::K7);
    config.inherit = None;
    config.bindings = bindings;
    Ok(())
}

pub fn snapshot_play_bindings(input: &ProfileInputConfig) -> Vec<BindingConfigEntry> {
    let config = input.play.get(KeyMode::K7.play_map_key());
    config.map(|c| c.bindings.clone()).unwrap_or_default()
}

pub fn restore_play_bindings(input: &mut ProfileInputConfig, bindings: Vec<BindingConfigEntry>) {
    if bindings.is_empty() {
        input.play.remove(KeyMode::K7.play_map_key());
        return;
    }
    let config = ensure_play_mode_config(input, KeyMode::K7);
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
    fn apply_play_keyboard_binding_moves_duplicate_key() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_keyboard_binding(&mut profile.input, LaneConfig::Key1, "Q").unwrap();
        apply_play_keyboard_binding(&mut profile.input, LaneConfig::Key2, "Q").unwrap();
        assert_eq!(format_play_keyboard_binding(&profile, LaneConfig::Key1), "(none)");
        assert_eq!(format_play_keyboard_binding(&profile, LaneConfig::Key2), "Q");
    }

    #[test]
    fn apply_play_keyboard_binding_replaces_lane_key() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        apply_play_keyboard_binding(&mut profile.input, LaneConfig::Scratch, "Space").unwrap();
        assert_eq!(format_play_keyboard_binding(&profile, LaneConfig::Scratch), "Space");
    }
}
