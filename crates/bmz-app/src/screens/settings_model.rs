use bmz_render::scene::SelectRowKind;

use crate::config::profile_config::ProfileConfig;
use crate::config::settings_registry::{SettingsEntryId, format_settings_value};
use crate::screens::select_model::SelectItem;

pub const CONFIG_ROOT_PATH: &str = "bmz-settings:";
const CONFIG_VOLUME_PATH: &str = "bmz-settings:volume";
const CONFIG_JUDGE_PATH: &str = "bmz-settings:judge";
const CONFIG_PLAY_PATH: &str = "bmz-settings:play";
const CONFIG_DISPLAY_PATH: &str = "bmz-settings:display";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPath<'a> {
    Root,
    Volume,
    Judge,
    Play,
    Display,
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
            SelectItem::AdvancedSettings,
        ],
        Some(SettingsPath::Volume) => config_items(SettingsEntryId::VOLUME_ENTRIES),
        Some(SettingsPath::Judge) => config_items(SettingsEntryId::JUDGE_ENTRIES),
        Some(SettingsPath::Play) => config_items(SettingsEntryId::PLAY_ENTRIES),
        Some(SettingsPath::Display) => config_items(SettingsEntryId::DISPLAY_ENTRIES),
        Some(SettingsPath::Unknown(_)) | None => Vec::new(),
    }
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

    #[test]
    fn parse_settings_paths() {
        assert_eq!(parse_settings_path(CONFIG_ROOT_PATH), Some(SettingsPath::Root));
        assert_eq!(parse_settings_path(CONFIG_VOLUME_PATH), Some(SettingsPath::Volume));
        assert_eq!(parse_settings_path(CONFIG_JUDGE_PATH), Some(SettingsPath::Judge));
        assert_eq!(parse_settings_path(CONFIG_PLAY_PATH), Some(SettingsPath::Play));
        assert_eq!(parse_settings_path(CONFIG_DISPLAY_PATH), Some(SettingsPath::Display));
        assert!(parse_settings_path("/songs").is_none());
    }

    #[test]
    fn settings_root_lists_categories() {
        let items = load_settings_items(CONFIG_ROOT_PATH);
        assert_eq!(items.len(), 5);
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
    fn settings_play_lists_gauge_entry() {
        let items = load_settings_items(CONFIG_PLAY_PATH);
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::Gauge
        )));
    }
}
