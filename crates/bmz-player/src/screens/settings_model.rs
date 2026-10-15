use bmz_core::lane::KeyMode;
use bmz_render::scene::SelectRowKind;

use crate::config::app_config::{AppConfig, AudioBackend, AudioBufferSizeMode};
use crate::config::app_settings_registry::{AppSettingsEntryId, format_app_settings_value};
use crate::config::key_config::{
    KEY_BINDING_SLOTS, KEY_CONFIG_MODES, KeyBindingGroup, KeyBindingTarget, binding_row_label,
    format_play_binding, key_binding_targets_for_group, key_mode_binding_targets,
    key_mode_settings_path,
};
use crate::config::profile_config::ProfileConfig;
use crate::config::settings_registry::{SettingsEntryId, format_settings_value};
use crate::i18n::{AppLocale, Localizer};
use crate::screens::select_model::SelectItem;

pub const CONFIG_ROOT_PATH: &str = "bmz-settings:";
const CONFIG_VOLUME_PATH: &str = "bmz-settings:volume";
const CONFIG_AUDIO_PATH: &str = "bmz-settings:audio";
const CONFIG_JUDGE_PATH: &str = "bmz-settings:judge";
const CONFIG_PLAY_PATH: &str = "bmz-settings:play";
const CONFIG_PLAY_SEVEN_TO_NINE_PATH: &str = "bmz-settings:play:seven-to-nine";
const CONFIG_PLAY_ASSIST_PATH: &str = "bmz-settings:play:assist";
const CONFIG_PLAY_ASSIST_NOTE_PATH: &str = "bmz-settings:play:assist:note";
const CONFIG_PLAY_ASSIST_JUDGE_PATH: &str = "bmz-settings:play:assist:judge";
const CONFIG_DISPLAY_PATH: &str = "bmz-settings:display";
const CONFIG_VIDEO_PATH: &str = "bmz-settings:video";
const CONFIG_INPUT_PATH: &str = "bmz-settings:input";
const CONFIG_SELECT_PATH: &str = "bmz-settings:select";
const CONFIG_REPLAY_PATH: &str = "bmz-settings:replay";
const CONFIG_UI_PATH: &str = "bmz-settings:ui";
pub const CONFIG_KEYS_PATH: &str = "bmz-settings:keys";
const CONFIG_KEYS_COMMON_PATH: &str = "bmz-settings:keys:common";
const CONFIG_KEYS_SELECT_PATH: &str = "bmz-settings:keys:select";
const CONFIG_KEYS_PLAY_PATH: &str = "bmz-settings:keys:play";
const CONFIG_KEYS_RESULT_PATH: &str = "bmz-settings:keys:result";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPath<'a> {
    Root,
    Volume,
    Audio,
    Judge,
    Play,
    PlaySevenToNine,
    PlayAssist,
    PlayAssistNote,
    PlayAssistJudge,
    Display,
    Video,
    Input,
    Select,
    Replay,
    Ui,
    KeysRoot,
    KeysCommon,
    KeysSelect,
    KeysPlay,
    KeysResult,
    KeysMode(KeyMode),
    Unknown(&'a str),
}

pub fn parse_settings_path(path: &str) -> Option<SettingsPath<'_>> {
    let rest = path.strip_prefix(CONFIG_ROOT_PATH)?;
    match rest {
        "" => Some(SettingsPath::Root),
        "volume" => Some(SettingsPath::Volume),
        "audio" => Some(SettingsPath::Audio),
        "judge" => Some(SettingsPath::Judge),
        "play" => Some(SettingsPath::Play),
        "play:seven-to-nine" => Some(SettingsPath::PlaySevenToNine),
        "play:assist" => Some(SettingsPath::PlayAssist),
        "play:assist:note" => Some(SettingsPath::PlayAssistNote),
        "play:assist:judge" => Some(SettingsPath::PlayAssistJudge),
        "display" => Some(SettingsPath::Display),
        "video" => Some(SettingsPath::Video),
        "input" => Some(SettingsPath::Input),
        "select" => Some(SettingsPath::Select),
        "replay" => Some(SettingsPath::Replay),
        "ui" => Some(SettingsPath::Ui),
        "keys" => Some(SettingsPath::KeysRoot),
        "keys:common" => Some(SettingsPath::KeysCommon),
        "keys:select" => Some(SettingsPath::KeysSelect),
        "keys:play" => Some(SettingsPath::KeysPlay),
        "keys:result" => Some(SettingsPath::KeysResult),
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
    settings_breadcrumb_for_locale(path, AppLocale::DEFAULT)
}

pub fn settings_breadcrumb_for_locale(path: &str, locale: AppLocale) -> String {
    let text = Localizer::new(locale);
    let root = text.text("settings-category-root");
    match parse_settings_path(path) {
        Some(SettingsPath::Root) | None => root,
        Some(SettingsPath::Volume) => breadcrumb(&root, &text.text("settings-category-volume")),
        Some(SettingsPath::Audio) => breadcrumb(&root, &text.text("settings-audio-title")),
        Some(SettingsPath::Judge) => breadcrumb(&root, &text.text("settings-category-judge")),
        Some(SettingsPath::Play) => breadcrumb(&root, &text.text("settings-category-play")),
        Some(SettingsPath::PlaySevenToNine) => format!(
            "{} > {} > {}",
            root,
            text.text("settings-category-play"),
            text.text("settings-category-seven-to-nine")
        ),
        Some(SettingsPath::PlayAssist) => format!(
            "{} > {} > {}",
            root,
            text.text("settings-category-play"),
            text.text("settings-category-assist")
        ),
        Some(SettingsPath::PlayAssistNote) => {
            assist_breadcrumb(&root, text, "settings-category-assist-note")
        }
        Some(SettingsPath::PlayAssistJudge) => {
            assist_breadcrumb(&root, text, "settings-category-assist-judge")
        }
        Some(SettingsPath::Display) => breadcrumb(&root, &text.text("settings-category-display")),
        Some(SettingsPath::Video) => breadcrumb(&root, &text.text("settings-video-title")),
        Some(SettingsPath::Input) => breadcrumb(&root, &text.text("settings-category-input")),
        Some(SettingsPath::Select) => breadcrumb(&root, &text.text("settings-category-select")),
        Some(SettingsPath::Replay) => breadcrumb(&root, &text.text("settings-category-replay")),
        Some(SettingsPath::Ui) => breadcrumb(&root, &text.text("settings-category-ui")),
        Some(SettingsPath::KeysRoot) => breadcrumb(&root, &text.text("settings-category-keys")),
        Some(SettingsPath::KeysCommon) => format!(
            "{} > {} > {}",
            root,
            text.text("settings-category-keys"),
            text.text("settings-category-common")
        ),
        Some(SettingsPath::KeysSelect) => format!(
            "{} > {} > {}",
            root,
            text.text("settings-category-keys"),
            text.text("settings-nav-select")
        ),
        Some(SettingsPath::KeysPlay) => format!(
            "{} > {} > {}",
            root,
            text.text("settings-category-keys"),
            text.text("settings-nav-play")
        ),
        Some(SettingsPath::KeysResult) => format!(
            "{} > {} > {}",
            root,
            text.text("settings-category-keys"),
            text.text("settings-nav-result")
        ),
        Some(SettingsPath::KeysMode(key_mode)) => {
            format!("{} > {} > {}", root, text.text("settings-category-keys"), key_mode.as_str())
        }
        Some(SettingsPath::Unknown(_)) => root,
    }
}

fn breadcrumb(root: &str, section: &str) -> String {
    format!("{root} > {section}")
}

fn assist_breadcrumb(root: &str, text: Localizer, leaf_key: &str) -> String {
    format!(
        "{} > {} > {} > {}",
        root,
        text.text("settings-category-play"),
        text.text("settings-category-assist"),
        text.text(leaf_key)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigSelectRow {
    pub entry_id: SettingsEntryId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfigSelectRow {
    pub entry_id: AppSettingsEntryId,
}

impl AppConfigSelectRow {
    pub fn label(self) -> &'static str {
        self.entry_id.label()
    }

    pub fn value_text(self, config: &AppConfig, locale: AppLocale) -> String {
        format_app_settings_value(config, self.entry_id, locale)
    }

    pub fn description_text(self, profile: &ProfileConfig) -> String {
        Localizer::new(profile.ui.locale()).text(self.entry_id.description_key())
    }
}

impl ConfigSelectRow {
    pub fn label(self) -> &'static str {
        self.entry_id.label()
    }

    pub fn value_text(self, profile: &ProfileConfig) -> String {
        format_settings_value(profile, self.entry_id)
    }

    pub fn description_text(self, profile: &ProfileConfig) -> String {
        Localizer::new(profile.ui.locale()).text(self.entry_id.description_key())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindingSelectRow {
    pub key_mode: KeyMode,
    pub target: KeyBindingTarget,
}

impl KeyBindingSelectRow {
    pub fn label(self) -> String {
        binding_row_label(self.key_mode, self.target)
    }

    pub fn value_text(self, profile: &ProfileConfig) -> String {
        format_play_binding(profile, self.key_mode, self.target)
    }

    pub fn description_text(self, profile: &ProfileConfig) -> String {
        Localizer::new(profile.ui.locale()).text("settings-key-binding-description")
    }
}

pub fn settings_root_item() -> SelectItem {
    settings_root_item_for_locale(AppLocale::DEFAULT)
}

pub fn settings_root_item_for_locale(locale: AppLocale) -> SelectItem {
    SelectItem::Folder {
        path: CONFIG_ROOT_PATH.to_string(),
        name: Localizer::new(locale).text("settings-category-root"),
        kind: SelectRowKind::SettingsRoot,
        summary: None,
    }
}

pub fn load_settings_items(path: &str) -> Vec<SelectItem> {
    load_settings_items_for_locale(path, AppLocale::DEFAULT)
}

pub fn load_settings_items_for_locale(path: &str, locale: AppLocale) -> Vec<SelectItem> {
    load_settings_items_for_config(
        path,
        locale,
        &AppConfig::default(),
        &ProfileConfig::new_default("default", "Default", 0),
    )
}

pub fn load_settings_items_for_config(
    path: &str,
    locale: AppLocale,
    app_config: &AppConfig,
    profile_config: &ProfileConfig,
) -> Vec<SelectItem> {
    let text = Localizer::new(locale);
    let settings_path = parse_settings_path(path);
    let mut items = match settings_path {
        Some(SettingsPath::Root) => vec![
            SelectItem::Folder {
                path: CONFIG_VOLUME_PATH.to_string(),
                name: text.text("settings-category-volume"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_AUDIO_PATH.to_string(),
                name: text.text("settings-audio-title"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_JUDGE_PATH.to_string(),
                name: text.text("settings-category-judge"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_PLAY_PATH.to_string(),
                name: text.text("settings-category-play"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_DISPLAY_PATH.to_string(),
                name: text.text("settings-category-display"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_VIDEO_PATH.to_string(),
                name: text.text("settings-video-title"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_INPUT_PATH.to_string(),
                name: text.text("settings-category-input"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_SELECT_PATH.to_string(),
                name: text.text("settings-category-select"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_REPLAY_PATH.to_string(),
                name: text.text("settings-category-replay"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_UI_PATH.to_string(),
                name: text.text("settings-category-ui"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::Folder {
                path: CONFIG_KEYS_PATH.to_string(),
                name: text.text("settings-category-keys"),
                kind: SelectRowKind::SettingsFolder,
                summary: None,
            },
            SelectItem::AdvancedSettings,
        ],
        Some(SettingsPath::Volume) => config_items(SettingsEntryId::VOLUME_ENTRIES),
        Some(SettingsPath::Audio) => audio_items(app_config),
        Some(SettingsPath::Judge) => config_items(SettingsEntryId::JUDGE_ENTRIES),
        Some(SettingsPath::Play) => play_items(locale),
        Some(SettingsPath::PlaySevenToNine) => config_items(SettingsEntryId::SEVEN_TO_NINE_ENTRIES),
        Some(SettingsPath::PlayAssist) => assist_items(locale),
        Some(SettingsPath::PlayAssistNote) => config_items(SettingsEntryId::ASSIST_NOTE_ENTRIES),
        Some(SettingsPath::PlayAssistJudge) => config_items(SettingsEntryId::ASSIST_JUDGE_ENTRIES),
        Some(SettingsPath::Display) => display_items(profile_config),
        Some(SettingsPath::Video) => app_config_items(AppSettingsEntryId::VIDEO_ENTRIES),
        Some(SettingsPath::Input) => config_items(SettingsEntryId::INPUT_ENTRIES),
        Some(SettingsPath::Select) => config_items(SettingsEntryId::SELECT_ENTRIES),
        Some(SettingsPath::Replay) => config_items(SettingsEntryId::REPLAY_ENTRIES),
        Some(SettingsPath::Ui) => config_items(SettingsEntryId::UI_ENTRIES),
        Some(SettingsPath::KeysRoot) => key_mode_folder_items(locale),
        Some(SettingsPath::KeysCommon) => key_group_binding_items(KeyBindingGroup::Common),
        Some(SettingsPath::KeysSelect) => key_group_binding_items(KeyBindingGroup::Select),
        Some(SettingsPath::KeysPlay) => key_group_binding_items(KeyBindingGroup::Play),
        Some(SettingsPath::KeysResult) => key_group_binding_items(KeyBindingGroup::Result),
        Some(SettingsPath::KeysMode(key_mode)) => key_binding_items(key_mode),
        Some(SettingsPath::Unknown(_)) | None => Vec::new(),
    };
    if !items.is_empty() {
        let action = if settings_path == Some(SettingsPath::Root) {
            SelectItem::SettingsClose
        } else {
            SelectItem::SettingsBack
        };
        items.insert(0, action);
    }
    items
}

fn settings_folder(path: &str, name: String) -> SelectItem {
    SelectItem::Folder {
        path: path.to_string(),
        name,
        kind: SelectRowKind::SettingsFolder,
        summary: None,
    }
}

fn audio_items(app_config: &AppConfig) -> Vec<SelectItem> {
    let mut entries = vec![AppSettingsEntryId::AudioBackend];
    if app_config.audio.backend == AudioBackend::Wasapi {
        entries.push(AppSettingsEntryId::AudioOutputMode);
    }
    entries.push(AppSettingsEntryId::AudioSampleRate);
    entries.push(AppSettingsEntryId::AudioBufferMode);
    if app_config.audio.buffer_size_mode == AudioBufferSizeMode::Fixed {
        entries.push(AppSettingsEntryId::AudioBufferSize);
    }
    if app_config.audio.backend == AudioBackend::Asio {
        entries.push(AppSettingsEntryId::AudioAsioDriver);
        entries.push(AppSettingsEntryId::AudioOutputChannelPair);
    } else {
        entries.push(AppSettingsEntryId::AudioOutputDevice);
    }
    let mut items = app_config_items(&entries);
    items.push(SelectItem::ApplyAudioSettings);
    items
}

fn play_items(locale: AppLocale) -> Vec<SelectItem> {
    let text = Localizer::new(locale);
    let mut items = Vec::new();
    for entry_id in SettingsEntryId::PLAY_ENTRIES {
        items.push(SelectItem::Config(ConfigSelectRow { entry_id: *entry_id }));
        if *entry_id == SettingsEntryId::KeyModeConversion {
            items.push(settings_folder(
                CONFIG_PLAY_SEVEN_TO_NINE_PATH,
                text.text("settings-category-seven-to-nine"),
            ));
            items.push(settings_folder(
                CONFIG_PLAY_ASSIST_PATH,
                text.text("settings-category-assist"),
            ));
        }
    }
    items
}

fn assist_items(locale: AppLocale) -> Vec<SelectItem> {
    let text = Localizer::new(locale);
    let mut items = config_items(SettingsEntryId::ASSIST_ENTRIES);
    items.push(settings_folder(
        CONFIG_PLAY_ASSIST_NOTE_PATH,
        text.text("settings-category-assist-note"),
    ));
    items.push(settings_folder(
        CONFIG_PLAY_ASSIST_JUDGE_PATH,
        text.text("settings-category-assist-judge"),
    ));
    items
}

fn key_mode_folder_items(locale: AppLocale) -> Vec<SelectItem> {
    let text = Localizer::new(locale);
    [
        (CONFIG_KEYS_COMMON_PATH, "settings-category-common"),
        (CONFIG_KEYS_SELECT_PATH, "settings-nav-select"),
        (CONFIG_KEYS_PLAY_PATH, "settings-nav-play"),
        (CONFIG_KEYS_RESULT_PATH, "settings-nav-result"),
    ]
    .into_iter()
    .map(|(path, label)| SelectItem::Folder {
        path: path.to_string(),
        name: text.text(label),
        kind: SelectRowKind::SettingsFolder,
        summary: None,
    })
    .chain(KEY_CONFIG_MODES.iter().copied().map(|key_mode| SelectItem::Folder {
        path: key_mode_settings_path(CONFIG_KEYS_PATH, key_mode),
        name: key_mode.as_str().to_string(),
        kind: SelectRowKind::SettingsFolder,
        summary: None,
    }))
    .collect()
}

fn key_group_binding_items(group: KeyBindingGroup) -> Vec<SelectItem> {
    KEY_BINDING_SLOTS
        .iter()
        .copied()
        .flat_map(|slot| {
            key_binding_targets_for_group(group, slot).into_iter().map(|target| {
                SelectItem::KeyBinding(KeyBindingSelectRow { key_mode: KeyMode::K7, target })
            })
        })
        .collect()
}

fn key_binding_items(key_mode: KeyMode) -> Vec<SelectItem> {
    let hispeed_rows = (key_mode == KeyMode::K8)
        .then_some(SettingsEntryId::HISPEED_8K_ENTRIES)
        .into_iter()
        .flatten()
        .copied()
        .map(|entry_id| SelectItem::Config(ConfigSelectRow { entry_id }));
    let binding_rows = KEY_BINDING_SLOTS.iter().copied().flat_map(|slot| {
        key_mode_binding_targets(key_mode, slot)
            .into_iter()
            .map(move |target| SelectItem::KeyBinding(KeyBindingSelectRow { key_mode, target }))
    });
    hispeed_rows.chain(binding_rows).collect()
}

fn config_items(entries: &'static [SettingsEntryId]) -> Vec<SelectItem> {
    entries
        .iter()
        .copied()
        .map(|entry_id| SelectItem::Config(ConfigSelectRow { entry_id }))
        .collect()
}

fn display_items(profile: &ProfileConfig) -> Vec<SelectItem> {
    let preset = profile.lane.hispeed_config();
    SettingsEntryId::DISPLAY_ENTRIES
        .iter()
        .copied()
        .filter(|entry_id| entry_id.visible_for_hispeed_config(preset))
        .map(|entry_id| SelectItem::Config(ConfigSelectRow { entry_id }))
        .collect()
}

fn app_config_items(entries: &[AppSettingsEntryId]) -> Vec<SelectItem> {
    entries
        .iter()
        .copied()
        .map(|entry_id| SelectItem::AppConfig(AppConfigSelectRow { entry_id }))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::key_config::{COMMON_ACTIONS, KeyBindingSlot, ScratchDirection};
    use crate::config::profile_config::{InputActionConfig, LaneConfig};

    #[test]
    fn parse_settings_paths() {
        assert_eq!(parse_settings_path(CONFIG_ROOT_PATH), Some(SettingsPath::Root));
        assert_eq!(parse_settings_path(CONFIG_VOLUME_PATH), Some(SettingsPath::Volume));
        assert_eq!(parse_settings_path(CONFIG_AUDIO_PATH), Some(SettingsPath::Audio));
        assert_eq!(parse_settings_path(CONFIG_JUDGE_PATH), Some(SettingsPath::Judge));
        assert_eq!(parse_settings_path(CONFIG_PLAY_PATH), Some(SettingsPath::Play));
        assert_eq!(
            parse_settings_path(CONFIG_PLAY_SEVEN_TO_NINE_PATH),
            Some(SettingsPath::PlaySevenToNine)
        );
        assert_eq!(parse_settings_path(CONFIG_PLAY_ASSIST_PATH), Some(SettingsPath::PlayAssist));
        assert_eq!(
            parse_settings_path(CONFIG_PLAY_ASSIST_NOTE_PATH),
            Some(SettingsPath::PlayAssistNote)
        );
        assert_eq!(
            parse_settings_path(CONFIG_PLAY_ASSIST_JUDGE_PATH),
            Some(SettingsPath::PlayAssistJudge)
        );
        assert_eq!(parse_settings_path(CONFIG_DISPLAY_PATH), Some(SettingsPath::Display));
        assert_eq!(parse_settings_path(CONFIG_VIDEO_PATH), Some(SettingsPath::Video));
        assert_eq!(parse_settings_path(CONFIG_INPUT_PATH), Some(SettingsPath::Input));
        assert_eq!(parse_settings_path(CONFIG_SELECT_PATH), Some(SettingsPath::Select));
        assert_eq!(parse_settings_path(CONFIG_REPLAY_PATH), Some(SettingsPath::Replay));
        assert_eq!(parse_settings_path(CONFIG_UI_PATH), Some(SettingsPath::Ui));
        assert_eq!(parse_settings_path(CONFIG_KEYS_PATH), Some(SettingsPath::KeysRoot));
        assert_eq!(parse_settings_path(CONFIG_KEYS_COMMON_PATH), Some(SettingsPath::KeysCommon));
        assert_eq!(
            parse_settings_path("bmz-settings:keys:7k"),
            Some(SettingsPath::KeysMode(KeyMode::K7))
        );
        assert!(parse_settings_path("/songs").is_none());
    }

    #[test]
    fn settings_root_lists_categories() {
        let items = load_settings_items(CONFIG_ROOT_PATH);
        assert_eq!(items.len(), 13);
        assert!(matches!(items.first(), Some(SelectItem::SettingsClose)));
        assert!(matches!(
            settings_root_item(),
            SelectItem::Folder { kind: SelectRowKind::SettingsRoot, .. }
        ));
        assert!(matches!(items.last(), Some(SelectItem::AdvancedSettings)));
        assert!(matches!(
            &items[1],
            SelectItem::Folder { name, .. } if name == "音量"
        ));

        let english = load_settings_items_for_locale(CONFIG_ROOT_PATH, AppLocale::En);
        assert!(matches!(
            &english[1],
            SelectItem::Folder { name, .. } if name == "Volume"
        ));
        assert_eq!(
            settings_breadcrumb_for_locale(CONFIG_KEYS_COMMON_PATH, AppLocale::Ko),
            "설정 > 키 설정 > 공통"
        );
    }

    #[test]
    fn settings_audio_lists_backend_specific_entries_and_apply_action() {
        let mut app_config = AppConfig::default();
        app_config.audio.backend = AudioBackend::Wasapi;
        let profile = ProfileConfig::new_default("default", "Default", 0);
        let wasapi =
            load_settings_items_for_config(CONFIG_AUDIO_PATH, AppLocale::Ja, &app_config, &profile);
        assert!(wasapi.iter().any(|item| matches!(
            item,
            SelectItem::AppConfig(row)
                if row.entry_id == AppSettingsEntryId::AudioOutputMode
        )));
        assert!(matches!(wasapi.last(), Some(SelectItem::ApplyAudioSettings)));

        app_config.audio.backend = AudioBackend::Asio;
        let asio =
            load_settings_items_for_config(CONFIG_AUDIO_PATH, AppLocale::Ja, &app_config, &profile);
        assert!(asio.iter().any(|item| matches!(
            item,
            SelectItem::AppConfig(row)
                if row.entry_id == AppSettingsEntryId::AudioAsioDriver
        )));
        assert!(asio.iter().any(|item| matches!(
            item,
            SelectItem::AppConfig(row)
                if row.entry_id == AppSettingsEntryId::AudioOutputChannelPair
        )));
        assert!(!asio.iter().any(|item| matches!(
            item,
            SelectItem::AppConfig(row)
                if row.entry_id == AppSettingsEntryId::AudioOutputDevice
        )));
    }

    #[test]
    fn settings_video_lists_every_video_entry() {
        let items = load_settings_items(CONFIG_VIDEO_PATH);
        assert_eq!(items.len(), AppSettingsEntryId::VIDEO_ENTRIES.len() + 1);
        for entry_id in AppSettingsEntryId::VIDEO_ENTRIES {
            assert!(items.iter().any(|item| matches!(
                item,
                SelectItem::AppConfig(row) if row.entry_id == *entry_id
            )));
        }
    }

    #[test]
    fn settings_select_lists_random_select_and_random_mix_options() {
        let items = load_settings_items(CONFIG_SELECT_PATH);
        assert_eq!(items.len(), SettingsEntryId::SELECT_ENTRIES.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::SelectRandomSelect
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::RandomMixTargetLevel
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::RandomMixStages
        )));
    }

    #[test]
    fn settings_volume_lists_entries() {
        let items = load_settings_items(CONFIG_VOLUME_PATH);
        assert_eq!(items.len(), SettingsEntryId::VOLUME_ENTRIES.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(
            matches!(&items[1], SelectItem::Config(row) if row.entry_id == SettingsEntryId::NormalizeChartVolume)
        );
        assert!(
            matches!(&items[2], SelectItem::Config(row) if row.entry_id == SettingsEntryId::NormalizeSystemBgmVolume)
        );
        assert!(
            matches!(&items[3], SelectItem::Config(row) if row.entry_id == SettingsEntryId::MasterVolume)
        );
    }

    #[test]
    fn settings_keys_lists_key_mode_folders() {
        let items = load_settings_items(CONFIG_KEYS_PATH);
        assert_eq!(items.len(), KEY_CONFIG_MODES.len() + 5);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(matches!(
            &items[1],
            SelectItem::Folder { name, path, .. }
                if name == "共通" && path == CONFIG_KEYS_COMMON_PATH
        ));
        assert!(matches!(
            &items[2],
            SelectItem::Folder { name, path, .. }
                if name == "選曲" && path == CONFIG_KEYS_SELECT_PATH
        ));
        assert!(matches!(
            &items[3],
            SelectItem::Folder { name, path, .. }
                if name == "プレイ" && path == CONFIG_KEYS_PLAY_PATH
        ));
        assert!(matches!(
            &items[4],
            SelectItem::Folder { name, path, .. }
                if name == "リザルト" && path == CONFIG_KEYS_RESULT_PATH
        ));
        assert!(matches!(
            &items[5],
            SelectItem::Folder { name, path, .. }
                if name == "4K" && path == "bmz-settings:keys:4k"
        ));
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let expected_path = format!("bmz-settings:keys:{}", key_mode.play_map_key());
            assert!(items.iter().any(|item| matches!(
                item,
                SelectItem::Folder { name, path, .. }
                    if name == key_mode.as_str() && path == &expected_path
            )));
        }
    }

    #[test]
    fn settings_keys_common_lists_configurable_actions() {
        let items = load_settings_items(CONFIG_KEYS_COMMON_PATH);
        assert_eq!(items.len(), COMMON_ACTIONS.len() * KEY_BINDING_SLOTS.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(matches!(
            &items[1],
            SelectItem::KeyBinding(row)
                if row.target == KeyBindingTarget::Action {
                    action: InputActionConfig::E1,
                    slot: KeyBindingSlot::KeyboardPrimary,
                }
        ));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::KeyBinding(row)
                if row.target == KeyBindingTarget::Action {
                    action: InputActionConfig::E4,
                    slot: KeyBindingSlot::Controller,
                }
        )));
        let select_items = load_settings_items(CONFIG_KEYS_SELECT_PATH);
        assert!(select_items.iter().any(|item| matches!(
            item,
            SelectItem::KeyBinding(row)
                if row.target == KeyBindingTarget::Action {
                    action: InputActionConfig::Screenshot,
                    slot: KeyBindingSlot::KeyboardPrimary,
                }
        )));
        assert!(!items.iter().any(|item| matches!(
            item,
            SelectItem::KeyBinding(row)
                if matches!(
                    row.target,
                    KeyBindingTarget::Action {
                        action: InputActionConfig::SelectEnter | InputActionConfig::SelectOptionBga,
                        ..
                    }
                )
        )));
    }

    #[test]
    fn settings_keys_7k_lists_lanes() {
        let items = load_settings_items("bmz-settings:keys:7k");
        assert_eq!(items.len(), 9 * KEY_BINDING_SLOTS.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(matches!(
            &items[1],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Up,
                        slot: KeyBindingSlot::KeyboardPrimary,
                    }
        ));
        assert!(matches!(
            &items[2],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Down,
                        slot: KeyBindingSlot::KeyboardPrimary,
                    }
        ));
        assert!(matches!(
            &items[10],
            SelectItem::KeyBinding(row)
                if row.key_mode == KeyMode::K7
                    && row.target == KeyBindingTarget::Scratch {
                        lane: LaneConfig::Scratch,
                        direction: ScratchDirection::Up,
                        slot: KeyBindingSlot::KeyboardSecondary,
                    }
        ));
        assert!(matches!(
            &items[19],
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
        assert_eq!(items.len(), 18 * KEY_BINDING_SLOTS.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::KeyBinding(row)
                if row.target == KeyBindingTarget::Key {
                    lane: LaneConfig::Key1,
                    slot: KeyBindingSlot::Controller1P,
                }
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::KeyBinding(row)
                if row.target == KeyBindingTarget::Key {
                    lane: LaneConfig::Key8,
                    slot: KeyBindingSlot::Controller2P,
                }
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::KeyBinding(row)
                if row.target == KeyBindingTarget::Scratch {
                    lane: LaneConfig::Scratch2,
                    direction: ScratchDirection::Up,
                    slot: KeyBindingSlot::Controller2P,
                }
        )));
    }

    #[test]
    fn settings_keys_extension_modes_list_lanes() {
        for (key_mode, rows_per_slot) in
            [(KeyMode::K4, 4), (KeyMode::K6, 6), (KeyMode::K8, 8), (KeyMode::K9, 9)]
        {
            let items =
                load_settings_items(&format!("bmz-settings:keys:{}", key_mode.play_map_key()));
            let hispeed_rows =
                if key_mode == KeyMode::K8 { SettingsEntryId::HISPEED_8K_ENTRIES.len() } else { 0 };
            assert_eq!(items.len(), rows_per_slot * KEY_BINDING_SLOTS.len() + hispeed_rows + 1);
            assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
            assert!(items.iter().any(|item| matches!(
                item,
                SelectItem::KeyBinding(row) if row.key_mode == key_mode
            )));
        }
    }

    #[test]
    fn settings_keys_8k_lists_each_hispeed_direction() {
        let items = load_settings_items("bmz-settings:keys:8k");
        let entries: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                SelectItem::Config(row)
                    if SettingsEntryId::HISPEED_8K_ENTRIES.contains(&row.entry_id) =>
                {
                    Some(row.entry_id)
                }
                _ => None,
            })
            .collect();

        assert_eq!(entries, SettingsEntryId::HISPEED_8K_ENTRIES);
    }

    #[test]
    fn settings_play_lists_gauge_entry() {
        let items = load_settings_items(CONFIG_PLAY_PATH);
        assert_eq!(items.len(), SettingsEntryId::PLAY_ENTRIES.len() + 3);
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::Gauge
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::RuleMode
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::LnModePolicy
        )));
        assert!(!items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::Assist
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::SessionMode
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Folder { path, .. } if path == CONFIG_PLAY_SEVEN_TO_NINE_PATH
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Folder { path, .. } if path == CONFIG_PLAY_ASSIST_PATH
        )));
    }

    #[test]
    fn settings_play_subfolders_list_conversion_and_assist_fields() {
        let seven_to_nine = load_settings_items(CONFIG_PLAY_SEVEN_TO_NINE_PATH);
        assert_eq!(seven_to_nine.len(), SettingsEntryId::SEVEN_TO_NINE_ENTRIES.len() + 1);
        assert!(seven_to_nine.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::SevenToNineRuleMode
        )));

        let assist = load_settings_items(CONFIG_PLAY_ASSIST_PATH);
        assert_eq!(assist.len(), SettingsEntryId::ASSIST_ENTRIES.len() + 3);
        assert!(assist.iter().any(|item| matches!(
            item,
            SelectItem::Folder { path, .. } if path == CONFIG_PLAY_ASSIST_NOTE_PATH
        )));
        assert!(assist.iter().any(|item| matches!(
            item,
            SelectItem::Folder { path, .. } if path == CONFIG_PLAY_ASSIST_JUDGE_PATH
        )));

        let note = load_settings_items(CONFIG_PLAY_ASSIST_NOTE_PATH);
        assert_eq!(note.len(), SettingsEntryId::ASSIST_NOTE_ENTRIES.len() + 1);
        let judge = load_settings_items(CONFIG_PLAY_ASSIST_JUDGE_PATH);
        assert_eq!(judge.len(), SettingsEntryId::ASSIST_JUDGE_ENTRIES.len() + 1);
    }

    #[test]
    fn settings_display_starts_with_hispeed_config_and_hides_unused_hispeed_entries() {
        let app_config = AppConfig::default();
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let common = [
            SettingsEntryId::SuddenEnabled,
            SettingsEntryId::Sudden,
            SettingsEntryId::LiftEnabled,
            SettingsEntryId::Lift,
            SettingsEntryId::HiddenEnabled,
            SettingsEntryId::Hidden,
            SettingsEntryId::Constant,
            SettingsEntryId::ConstantFadeMs,
        ];
        let cases = [
            (
                crate::config::profile_config::HispeedConfigPreset::Normal,
                vec![SettingsEntryId::HispeedMode, SettingsEntryId::NormalHispeedLevel],
            ),
            (
                crate::config::profile_config::HispeedConfigPreset::Classic,
                vec![
                    SettingsEntryId::HispeedMode,
                    SettingsEntryId::Hispeed,
                    SettingsEntryId::ClassicHispeedStep,
                ],
            ),
            (
                crate::config::profile_config::HispeedConfigPreset::Lr2,
                vec![
                    SettingsEntryId::HispeedMode,
                    SettingsEntryId::Hispeed,
                    SettingsEntryId::ClassicHispeedStep,
                ],
            ),
            (
                crate::config::profile_config::HispeedConfigPreset::Floating,
                vec![
                    SettingsEntryId::HispeedMode,
                    SettingsEntryId::FloatingHispeedStep,
                    SettingsEntryId::TargetGreenNumber,
                    SettingsEntryId::NoteDisplayDurationMs,
                    SettingsEntryId::HispeedAutoAdjust,
                ],
            ),
            (
                crate::config::profile_config::HispeedConfigPreset::NormalFloating,
                vec![
                    SettingsEntryId::HispeedMode,
                    SettingsEntryId::NormalHispeedLevel,
                    SettingsEntryId::FloatingHispeedStep,
                    SettingsEntryId::TargetGreenNumber,
                    SettingsEntryId::NoteDisplayDurationMs,
                    SettingsEntryId::HispeedAutoAdjust,
                ],
            ),
            (
                crate::config::profile_config::HispeedConfigPreset::ClassicFloating,
                vec![
                    SettingsEntryId::HispeedMode,
                    SettingsEntryId::Hispeed,
                    SettingsEntryId::ClassicHispeedStep,
                    SettingsEntryId::FloatingHispeedStep,
                    SettingsEntryId::TargetGreenNumber,
                    SettingsEntryId::NoteDisplayDurationMs,
                    SettingsEntryId::HispeedAutoAdjust,
                ],
            ),
        ];

        for (preset, mut expected) in cases {
            profile.lane.set_hispeed_config(preset);
            let items = load_settings_items_for_config(
                CONFIG_DISPLAY_PATH,
                AppLocale::Ja,
                &app_config,
                &profile,
            );
            let actual = items
                .iter()
                .filter_map(|item| match item {
                    SelectItem::Config(row) => Some(row.entry_id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            expected.extend(common);
            assert_eq!(actual, expected, "{preset:?}");
        }
    }

    #[test]
    fn settings_input_lists_supported_entries() {
        let items = load_settings_items(CONFIG_INPUT_PATH);
        assert_eq!(items.len(), SettingsEntryId::INPUT_ENTRIES.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row)
                if row.entry_id == SettingsEntryId::AnalogScratchSensitivity1P
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::KeyboardReleaseBounceMs
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::ControllerReleaseBounceMs
        )));
    }

    #[test]
    fn settings_replay_lists_slot_rules() {
        let items = load_settings_items(CONFIG_REPLAY_PATH);
        assert_eq!(items.len(), SettingsEntryId::REPLAY_ENTRIES.len() + 1);
        assert!(matches!(items.first(), Some(SelectItem::SettingsBack)));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::ReplaySlot4Rule
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::ReplayCompress
        )));
    }

    #[test]
    fn settings_ui_lists_language_and_fps() {
        let items = load_settings_items(CONFIG_UI_PATH);
        assert_eq!(items.len(), SettingsEntryId::UI_ENTRIES.len() + 1);
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::Language
        )));
        assert!(items.iter().any(|item| matches!(
            item,
            SelectItem::Config(row) if row.entry_id == SettingsEntryId::ShowFps
        )));
    }

    #[test]
    fn every_setting_entry_has_a_localized_item_description() {
        let entry_sets = [
            SettingsEntryId::VOLUME_ENTRIES,
            SettingsEntryId::JUDGE_ENTRIES,
            SettingsEntryId::PLAY_ENTRIES,
            SettingsEntryId::SEVEN_TO_NINE_ENTRIES,
            SettingsEntryId::ASSIST_ENTRIES,
            SettingsEntryId::ASSIST_NOTE_ENTRIES,
            SettingsEntryId::ASSIST_JUDGE_ENTRIES,
            SettingsEntryId::DISPLAY_ENTRIES,
            SettingsEntryId::INPUT_ENTRIES,
            SettingsEntryId::SELECT_ENTRIES,
            SettingsEntryId::HISPEED_8K_ENTRIES,
            SettingsEntryId::REPLAY_ENTRIES,
            SettingsEntryId::UI_ENTRIES,
        ];
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        for locale in AppLocale::SUPPORTED {
            profile.ui.language = locale.code().to_string();
            for entry_id in entry_sets
                .into_iter()
                .flatten()
                .copied()
                .chain(std::iter::once(SettingsEntryId::Assist))
            {
                let key = entry_id.description_key();
                let description = ConfigSelectRow { entry_id }.description_text(&profile);
                assert_ne!(description, key, "{}: {entry_id:?}", locale.code());
                assert!(!description.is_empty(), "{}: {entry_id:?}", locale.code());
            }
        }
    }

    #[test]
    fn every_app_setting_entry_has_a_localized_item_description() {
        let profile = ProfileConfig::new_default("default", "Default", 0);
        let mut entries = AppSettingsEntryId::VIDEO_ENTRIES.to_vec();
        entries.extend([
            AppSettingsEntryId::AudioBackend,
            AppSettingsEntryId::AudioOutputMode,
            AppSettingsEntryId::AudioSampleRate,
            AppSettingsEntryId::AudioBufferMode,
            AppSettingsEntryId::AudioBufferSize,
            AppSettingsEntryId::AudioOutputDevice,
            AppSettingsEntryId::AudioAsioDriver,
            AppSettingsEntryId::AudioOutputChannelPair,
        ]);
        for locale in AppLocale::SUPPORTED {
            let mut localized = profile.clone();
            localized.ui.language = locale.code().to_string();
            for entry_id in &entries {
                let description =
                    AppConfigSelectRow { entry_id: *entry_id }.description_text(&localized);
                assert_ne!(description, entry_id.description_key());
            }
        }
    }
}
