use bmz_core::lane::KeyMode;
use bmz_render::scene::SelectRowKind;

use crate::config::key_config::{
    KEY_BINDING_SLOTS, KEY_CONFIG_MODES, KeyBindingTarget, ScratchDirection, binding_row_label,
    format_play_binding, key_lanes_for_key_mode, key_mode_settings_path,
    scratch_lanes_for_key_mode,
};
use crate::config::profile_config::ProfileConfig;
use crate::config::settings_registry::{SettingsEntryId, format_settings_value};
use crate::screens::select_model::SelectItem;

pub const CONFIG_ROOT_PATH: &str = "bmz-settings:";
const CONFIG_VOLUME_PATH: &str = "bmz-settings:volume";
const CONFIG_JUDGE_PATH: &str = "bmz-settings:judge";
const CONFIG_PLAY_PATH: &str = "bmz-settings:play";
const CONFIG_DISPLAY_PATH: &str = "bmz-settings:display";
pub const CONFIG_KEYS_PATH: &str = "bmz-settings:keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPath<'a> {
    Root,
    Volume,
    Judge,
    Play,
    Display,
    KeysRoot,
    KeysMode(KeyMode),
    Unknown(&'a str),
}

pub fn parse_settings_path(path: &str) -> Option<SettingsPath<'_>> {
    let rest = path.strip_prefix(CONFIG_ROOT_PATH)?;
    match rest {
        "" => Some(SettingsPath::Root),
        "volume" => Some(SettingsPath::Volume),
        "judge" => Some(SettingsPath::Judge),
        "play" => Some(SettingsPath::Play),
        "display" => Some(SettingsPath::Display),
        "keys" => Some(SettingsPath::KeysRoot),
        _ if let Some(mode_key) = rest.strip_prefix("keys:") => {
            KeyMode::from_play_map_key(mode_key).map(SettingsPath::KeysMode)
        }
        other => Some(SettingsPath::Unknown(other)),
    }
}

pub fn in_settings_stack(stack: &[String]) -> bool {
    stack.last().is_some_and(|path| path.starts_with(CONFIG_ROOT_PATH))
}

pub fn settings_breadcrumb(path: &str) -> String {
    match parse_settings_path(path) {
        Some(SettingsPath::Root) | None => "設定".to_string(),
        Some(SettingsPath::Volume) => "設定 > 音量".to_string(),
        Some(SettingsPath::Judge) => "設定 > 判定".to_string(),
        Some(SettingsPath::Play) => "設定 > プレイ".to_string(),
        Some(SettingsPath::Display) => "設定 > 表示".to_string(),
        Some(SettingsPath::KeysRoot) => "設定 > キー設定".to_string(),
        Some(SettingsPath::KeysMode(key_mode)) => {
            format!("設定 > キー設定 > {}", key_mode.as_str())
        }
        Some(SettingsPath::Unknown(_)) => "設定".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigSelectRow {
    pub entry_id: SettingsEntryId,
}

impl ConfigSelectRow {
    pub fn label(self) -> &'static str {
        self.entry_id.label()
    }

    pub fn value_text(self, profile: &ProfileConfig) -> String {
        format_settings_value(profile, self.entry_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindingSelectRow {
    pub key_mode: KeyMode,
    pub target: KeyBindingTarget,
}

impl KeyBindingSelectRow {
    pub fn label(self) -> String {
        binding_row_label(self.target)
    }

    pub fn value_text(self, profile: &ProfileConfig) -> String {
        format_play_binding(profile, self.key_mode, self.target)
    }
}

pub fn settings_root_item() -> SelectItem {
    SelectItem::Folder {
        path: CONFIG_ROOT_PATH.to_string(),
        name: "設定".to_string(),
        kind: SelectRowKind::SettingsFolder,
    }
}

pub fn load_settings_items(path: &str) -> Vec<SelectItem> {
    match parse_settings_path(path) {
        Some(SettingsPath::Root) => vec![
            SelectItem::Folder {
                path: CONFIG_VOLUME_PATH.to_string(),
                name: "音量".to_string(),
                kind: SelectRowKind::SettingsFolder,
            },
            SelectItem::Folder {
                path: CONFIG_JUDGE_PATH.to_string(),
                name: "判定".to_string(),
                kind: SelectRowKind::SettingsFolder,
            },
            SelectItem::Folder {
                path: CONFIG_PLAY_PATH.to_string(),
                name: "プレイ".to_string(),
                kind: SelectRowKind::SettingsFolder,
            },
            SelectItem::Folder {
                path: CONFIG_DISPLAY_PATH.to_string(),
                name: "表示".to_string(),
                kind: SelectRowKind::SettingsFolder,
            },
            SelectItem::Folder {
                path: CONFIG_KEYS_PATH.to_string(),
                name: "キー設定".to_string(),
                kind: SelectRowKind::SettingsFolder,
            },
            SelectItem::AdvancedSettings,
        ],
        Some(SettingsPath::Volume) => config_items(SettingsEntryId::VOLUME_ENTRIES),
        Some(SettingsPath::Judge) => config_items(SettingsEntryId::JUDGE_ENTRIES),
        Some(SettingsPath::Play) => config_items(SettingsEntryId::PLAY_ENTRIES),
        Some(SettingsPath::Display) => config_items(SettingsEntryId::DISPLAY_ENTRIES),
        Some(SettingsPath::KeysRoot) => key_mode_folder_items(),
        Some(SettingsPath::KeysMode(key_mode)) => key_binding_items(key_mode),
        Some(SettingsPath::Unknown(_)) | None => Vec::new(),
    }
}

fn key_mode_folder_items() -> Vec<SelectItem> {
    KEY_CONFIG_MODES
        .iter()
        .copied()
        .map(|key_mode| SelectItem::Folder {
            path: key_mode_settings_path(CONFIG_KEYS_PATH, key_mode),
            name: key_mode.as_str().to_string(),
            kind: SelectRowKind::SettingsFolder,
        })
        .collect()
}

fn key_binding_items(key_mode: KeyMode) -> Vec<SelectItem> {
    let scratch_lanes = scratch_lanes_for_key_mode(key_mode);
    let key_lanes = key_lanes_for_key_mode(key_mode);
    KEY_BINDING_SLOTS
        .iter()
        .copied()
        .flat_map(|slot| {
            let scratch_rows = scratch_lanes.iter().copied().flat_map(move |lane| {
                [ScratchDirection::Up, ScratchDirection::Down].into_iter().map(move |direction| {
                    SelectItem::KeyBinding(KeyBindingSelectRow {
                        key_mode,
                        target: KeyBindingTarget::Scratch { lane, direction, slot },
                    })
                })
            });
            let key_rows = key_lanes.iter().copied().map(move |lane| {
                SelectItem::KeyBinding(KeyBindingSelectRow {
                    key_mode,
                    target: KeyBindingTarget::Key { lane, slot },
                })
            });
            scratch_rows.chain(key_rows)
        })
        .collect()
}

fn config_items(entries: &'static [SettingsEntryId]) -> Vec<SelectItem> {
    entries
        .iter()
        .copied()
        .map(|entry_id| SelectItem::Config(ConfigSelectRow { entry_id }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::key_config::KeyBindingSlot;
    use crate::config::profile_config::LaneConfig;

    #[test]
    fn parse_settings_paths() {
        assert_eq!(parse_settings_path(CONFIG_ROOT_PATH), Some(SettingsPath::Root));
        assert_eq!(parse_settings_path(CONFIG_VOLUME_PATH), Some(SettingsPath::Volume));
        assert_eq!(parse_settings_path(CONFIG_JUDGE_PATH), Some(SettingsPath::Judge));
        assert_eq!(parse_settings_path(CONFIG_PLAY_PATH), Some(SettingsPath::Play));
        assert_eq!(parse_settings_path(CONFIG_DISPLAY_PATH), Some(SettingsPath::Display));
        assert_eq!(parse_settings_path(CONFIG_KEYS_PATH), Some(SettingsPath::KeysRoot));
        assert_eq!(
            parse_settings_path("bmz-settings:keys:7k"),
            Some(SettingsPath::KeysMode(KeyMode::K7))
        );
        assert!(parse_settings_path("/songs").is_none());
    }

    #[test]
    fn settings_root_lists_categories() {
        let items = load_settings_items(CONFIG_ROOT_PATH);
        assert_eq!(items.len(), 6);
        assert!(matches!(items.last(), Some(SelectItem::AdvancedSettings)));
        assert!(matches!(
            &items[0],
            SelectItem::Folder { name, .. } if name == "音量"
        ));
    }

    #[test]
    fn settings_volume_lists_entries() {
        let items = load_settings_items(CONFIG_VOLUME_PATH);
        assert_eq!(items.len(), 6);
        assert!(
            matches!(&items[0], SelectItem::Config(row) if row.entry_id == SettingsEntryId::MasterVolume)
        );
    }

    #[test]
    fn settings_keys_lists_key_mode_folders() {
        let items = load_settings_items(CONFIG_KEYS_PATH);
        assert_eq!(items.len(), KEY_CONFIG_MODES.len());
        assert!(matches!(
            &items[1],
            SelectItem::Folder { name, path, .. }
                if name == "7K" && path == "bmz-settings:keys:7k"
        ));
    }

    #[test]
    fn settings_keys_7k_lists_lanes() {
        let items = load_settings_items("bmz-settings:keys:7k");
        assert_eq!(items.len(), 9 * KEY_BINDING_SLOTS.len());
        assert!(matches!(
            &items[0],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Up,
                        slot: KeyBindingSlot::KeyboardPrimary,
                    }
        ));
        assert!(matches!(
            &items[1],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Down,
                        slot: KeyBindingSlot::KeyboardPrimary,
                    }
        ));
        assert!(matches!(
            &items[9],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Up,
                        slot: KeyBindingSlot::KeyboardSecondary,
                    }
        ));
        assert!(matches!(
            &items[18],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Up,
                        slot: KeyBindingSlot::Controller,
                    }
        ));
    }

    #[test]
    fn settings_keys_14k_lists_lanes() {
        let items = load_settings_items("bmz-settings:keys:14k");
        assert_eq!(items.len(), 18 * KEY_BINDING_SLOTS.len());
    }

    #[test]
    fn settings_play_lists_gauge_entry() {
        let items = load_settings_items(CONFIG_PLAY_PATH);
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::Gauge
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::RuleMode
        )));
    }
}
