use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const DEFAULT_DISCORD_APPLICATION_ID: &str = "1524506927315419448";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u32,
    pub active_profile: String,
    pub songs: SongPathsConfig,
    pub scan: ScanConfig,
    pub audio: AudioConfig,
    pub video: VideoConfig,
    #[serde(default)]
    pub screenshot: ScreenshotConfig,
    #[serde(default)]
    pub obs: ObsConfig,
    #[serde(default)]
    pub select: MusicSelectConfig,
    pub input: GlobalInputConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tables: DifficultyTablesConfig,
    #[serde(default)]
    pub updates: UpdatesConfig,
    #[serde(default)]
    pub discord: DiscordConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongPathsConfig {
    pub roots: Vec<PathEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathEntry {
    pub path: String,
    pub enabled: bool,
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    pub follow_symlinks: bool,
    pub skip_hidden: bool,
    pub auto_rescan_on_startup: bool,
    pub rescan_missing_files: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub backend: AudioBackend,
    pub output_device: String,
    /// `sample_rate_mode` が `Fixed` のときに要求するサンプルレート(Hz)。
    pub sample_rate: u32,
    /// サンプルレートの決定方法。`Auto` はドライバ / OS 既定を使用。
    #[serde(default)]
    pub sample_rate_mode: AudioSampleRateMode,
    pub buffer_size_mode: AudioBufferSizeMode,
    pub buffer_size: u32,
    pub exclusive_mode: bool,
    pub asio_driver: String,
    /// 出力するステレオチャンネルペア(0 始まり)。0 = 1-2ch, 1 = 3-4ch, 2 = 5-6ch …。
    /// Babyface など多チャンネル出力デバイスで出力先ペアを選ぶ。デバイスの
    /// チャンネル数を超える指定はストリーム生成時にクランプされる。
    #[serde(default)]
    pub output_channel_pair: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AudioBackend {
    Auto,
    Wasapi,
    Asio,
    CoreAudio,
    Alsa,
    Pulse,
    PipeWire,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AudioBufferSizeMode {
    Auto,
    Fixed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AudioSampleRateMode {
    /// ドライバ / OS が返す既定サンプルレートを使う。ASIO でドライバ側レートと
    /// 食い違って無音になるのを避けるための既定。
    #[default]
    Auto,
    /// `AudioConfig::sample_rate` の値を要求する。
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub mode: WindowMode,
    pub width: u32,
    pub height: u32,
    pub vsync_mode: VsyncModeConfig,
    pub target_fps: u32,
    pub frame_limit_in_background: u32,
    pub renderer: RendererBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicSelectConfig {
    #[serde(default = "default_scroll_duration_low_ms")]
    pub scroll_duration_low_ms: u32,
    #[serde(default = "default_scroll_duration_high_ms")]
    pub scroll_duration_high_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotConfig {
    #[serde(default = "default_screenshot_dir")]
    pub dir: String,
    #[serde(default = "default_true")]
    pub copy_to_clipboard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_obs_host")]
    pub host: String,
    #[serde(default = "default_obs_port")]
    pub port: u16,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_obs_record_stop_wait_ms")]
    pub record_stop_wait_ms: u64,
    #[serde(default)]
    pub recording_mode: ObsRecordingMode,
    #[serde(default)]
    pub scenes: BTreeMap<String, String>,
    #[serde(default)]
    pub actions: BTreeMap<String, ObsActionConfig>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ObsRecordingMode {
    #[default]
    KeepAll,
    OnScreenshot,
    OnReplay,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ObsActionConfig {
    #[default]
    None,
    StartRecord,
    StopRecord,
}

impl Default for ObsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_obs_host(),
            port: default_obs_port(),
            password: String::new(),
            record_stop_wait_ms: default_obs_record_stop_wait_ms(),
            recording_mode: ObsRecordingMode::KeepAll,
            scenes: BTreeMap::new(),
            actions: BTreeMap::new(),
        }
    }
}

fn default_obs_host() -> String {
    "localhost".to_string()
}

fn default_obs_port() -> u16 {
    4455
}

fn default_obs_record_stop_wait_ms() -> u64 {
    5000
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self { dir: default_screenshot_dir(), copy_to_clipboard: true }
    }
}

pub fn default_screenshot_dir() -> String {
    "screenshots".to_string()
}

impl Default for MusicSelectConfig {
    fn default() -> Self {
        Self {
            scroll_duration_low_ms: default_scroll_duration_low_ms(),
            scroll_duration_high_ms: default_scroll_duration_high_ms(),
        }
    }
}

pub fn default_scroll_duration_low_ms() -> u32 {
    300
}

pub fn default_scroll_duration_high_ms() -> u32 {
    50
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum WindowMode {
    Windowed,
    BorderlessFullscreen,
    ExclusiveFullscreen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum RendererBackend {
    Auto,
    Vulkan,
    Metal,
    Dx12,
    Gl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum VsyncModeConfig {
    #[default]
    Vsync,
    AdaptiveVsync,
    VsyncOff,
    FastVsync,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalInputConfig {
    pub backend: InputBackendKind,
    pub keyboard_enabled: bool,
    pub gamepad_enabled: bool,
    pub midi_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum InputBackendKind {
    Auto,
    Winit,
    RawInput,
    Hid,
    Midi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub file_logging: bool,
}

pub const DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS: &[&str] = &[
    "https://darksabun.club/table/archive/normal1/",
    "https://darksabun.club/table/archive/insane1/",
    "https://rattoto10.jounin.jp/table.html",
    "https://rattoto10.jounin.jp/table_insane.html",
    "https://rattoto10.jounin.jp/table_overjoy.html",
    "https://stellabms.xyz/st/table.html",
    "https://stellabms.xyz/sl/table.html",
    "https://stellabms.xyz/so/table.html",
    "https://stellabms.xyz/sn/table.html",
    "https://mplwtch.github.io/Solomon/",
    "https://mocha-repository.info/table/ln_header.json",
    "https://ladymade-star.github.io/luminous/",
    "http://minddnim.web.fc2.com/sara/3rd_hard/bms_sara_3rd_hard.html",
    "https://egret9.github.io/Scramble/",
    "https://classmaterma.github.io/4UE/table.html",
    "https://classmaterma.github.io/UE/table.html",
    "https://classmaterma.github.io/8UE/table.html",
    "https://hibyethere.github.io/table/",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyTablesConfig {
    #[serde(default)]
    pub sources: Vec<DifficultyTableSource>,
    #[serde(default)]
    pub auto_fetch_on_startup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifficultyTableSource {
    pub url: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatesConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub channel: UpdateChannelConfig,
    #[serde(default = "default_update_check_on_startup")]
    pub check_on_startup: bool,
    #[serde(default)]
    pub skipped_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "should_skip_discord_application_id")]
    pub application_id: String,
    #[serde(default = "default_discord_large_image_key")]
    pub large_image_key: String,
    #[serde(default = "default_discord_large_image_text")]
    pub large_image_text: String,
    #[serde(default = "default_true")]
    pub show_song_details: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum UpdateChannelConfig {
    #[default]
    Stable,
    Prerelease,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            channel: UpdateChannelConfig::Stable,
            check_on_startup: default_update_check_on_startup(),
            skipped_version: String::new(),
        }
    }
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            application_id: String::new(),
            large_image_key: default_discord_large_image_key(),
            large_image_text: default_discord_large_image_text(),
            show_song_details: true,
        }
    }
}

impl Default for DifficultyTablesConfig {
    fn default() -> Self {
        Self {
            sources: DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS
                .iter()
                .map(|url| DifficultyTableSource { url: (*url).to_string(), enabled: true })
                .collect(),
            auto_fetch_on_startup: false,
        }
    }
}

pub fn ensure_default_difficulty_table_sources(config: &mut AppConfig) {
    for &url in DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS {
        if !config.tables.sources.iter().any(|source| source.url == url) {
            config
                .tables
                .sources
                .push(DifficultyTableSource { url: url.to_string(), enabled: true });
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_update_check_on_startup() -> bool {
    !cfg!(debug_assertions)
}

fn default_discord_large_image_key() -> String {
    "bmz".to_string()
}

fn default_discord_large_image_text() -> String {
    "BMZ Player".to_string()
}

fn should_skip_discord_application_id(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value == DEFAULT_DISCORD_APPLICATION_ID
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            active_profile: "default".to_string(),
            songs: SongPathsConfig { roots: Vec::new() },
            scan: ScanConfig {
                follow_symlinks: true,
                skip_hidden: true,
                auto_rescan_on_startup: false,
                rescan_missing_files: true,
            },
            audio: AudioConfig {
                backend: AudioBackend::Auto,
                output_device: String::new(),
                sample_rate: 48_000,
                sample_rate_mode: AudioSampleRateMode::Auto,
                buffer_size_mode: AudioBufferSizeMode::Fixed,
                buffer_size: 256,
                exclusive_mode: false,
                asio_driver: String::new(),
                output_channel_pair: 0,
            },
            video: VideoConfig {
                mode: WindowMode::Windowed,
                width: 1280,
                height: 720,
                vsync_mode: VsyncModeConfig::Vsync,
                target_fps: 240,
                frame_limit_in_background: 60,
                renderer: RendererBackend::Auto,
            },
            screenshot: ScreenshotConfig::default(),
            obs: ObsConfig::default(),
            select: MusicSelectConfig::default(),
            input: GlobalInputConfig {
                backend: InputBackendKind::Auto,
                keyboard_enabled: true,
                gamepad_enabled: true,
                midi_enabled: false,
            },
            logging: LoggingConfig { level: LogLevel::Info, file_logging: true },
            tables: DifficultyTablesConfig::default(),
            updates: UpdatesConfig::default(),
            discord: DiscordConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_defaults_screenshot_settings() {
        let config = AppConfig::default();

        assert_eq!(config.screenshot.dir, "screenshots");
        assert!(config.screenshot.copy_to_clipboard);
    }

    #[test]
    fn app_config_defaults_scan_symlinks_and_background_frame_limit() {
        let config = AppConfig::default();

        assert!(config.scan.follow_symlinks);
        assert!(!config.scan.auto_rescan_on_startup);
        assert_eq!(config.video.vsync_mode, VsyncModeConfig::Vsync);
        assert_eq!(config.video.frame_limit_in_background, 60);
    }

    #[test]
    fn app_config_defaults_discord_presence_disabled() {
        let config = AppConfig::default();

        assert!(!config.discord.enabled);
        assert!(config.discord.application_id.is_empty());
        assert_eq!(config.discord.large_image_key, "bmz");
        assert_eq!(config.discord.large_image_text, "BMZ Player");
        assert!(config.discord.show_song_details);
    }

    #[test]
    fn app_config_does_not_serialize_builtin_discord_application_id() {
        let mut config = AppConfig::default();
        config.discord.application_id = DEFAULT_DISCORD_APPLICATION_ID.to_string();

        let toml = toml::to_string(&config).unwrap();

        assert!(!toml.contains(DEFAULT_DISCORD_APPLICATION_ID));
    }

    #[test]
    fn app_config_serializes_vsync_mode_without_legacy_keys() {
        let toml = toml::to_string(&AppConfig::default()).unwrap();

        assert!(toml.contains("vsync_mode = \"Vsync\""));
        assert!(!toml.contains("vsync ="));
        assert!(!toml.contains("present_mode"));
    }

    #[test]
    fn app_config_defaults_include_builtin_difficulty_tables() {
        let config = AppConfig::default();

        assert_eq!(config.tables.sources.len(), DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS.len());
        assert!(config.tables.sources.iter().all(|source| source.enabled));
        assert_eq!(config.tables.sources[0].url, DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS[0]);
    }

    #[test]
    fn app_config_defaults_update_settings() {
        let config = AppConfig::default();

        assert!(config.updates.enabled);
        assert_eq!(config.updates.channel, UpdateChannelConfig::Stable);
        assert_eq!(config.updates.check_on_startup, !cfg!(debug_assertions));
        assert!(config.updates.skipped_version.is_empty());
    }

    #[test]
    fn app_config_defaults_obs_settings() {
        let config = AppConfig::default();

        assert!(!config.obs.enabled);
        assert_eq!(config.obs.host, "localhost");
        assert_eq!(config.obs.port, 4455);
        assert_eq!(config.obs.record_stop_wait_ms, 5000);
        assert_eq!(config.obs.recording_mode, ObsRecordingMode::KeepAll);
        assert!(config.obs.scenes.is_empty());
        assert!(config.obs.actions.is_empty());
    }

    #[test]
    fn ensure_default_difficulty_tables_adds_missing_without_reenabling_existing() {
        let disabled_url = DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS[0].to_string();
        let mut config = AppConfig {
            tables: DifficultyTablesConfig {
                sources: vec![DifficultyTableSource { url: disabled_url.clone(), enabled: false }],
                auto_fetch_on_startup: true,
            },
            ..AppConfig::default()
        };

        ensure_default_difficulty_table_sources(&mut config);

        assert_eq!(config.tables.sources.len(), DEFAULT_DIFFICULTY_TABLE_SOURCE_URLS.len());
        assert!(!config.tables.sources[0].enabled);
        assert_eq!(config.tables.sources[0].url, disabled_url);
        assert!(config.tables.auto_fetch_on_startup);
    }

    #[test]
    fn app_config_loads_missing_screenshot_section() {
        let mut toml = toml::to_string(&AppConfig::default()).unwrap();
        let start = toml.find("[screenshot]").unwrap();
        let end =
            toml[start + 1..].find("\n[").map(|offset| start + 1 + offset).unwrap_or(toml.len());
        toml.replace_range(start..end, "");

        let config: AppConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.screenshot.dir, "screenshots");
        assert!(config.screenshot.copy_to_clipboard);
    }

    #[test]
    fn app_config_loads_missing_updates_section() {
        let mut toml = toml::to_string(&AppConfig::default()).unwrap();
        let start = toml.find("[updates]").unwrap();
        let end =
            toml[start + 1..].find("\n[").map(|offset| start + 1 + offset).unwrap_or(toml.len());
        toml.replace_range(start..end, "");

        let config: AppConfig = toml::from_str(&toml).unwrap();

        assert!(config.updates.enabled);
        assert_eq!(config.updates.channel, UpdateChannelConfig::Stable);
    }

    #[test]
    fn app_config_loads_missing_obs_section() {
        let mut toml = toml::to_string(&AppConfig::default()).unwrap();
        let start = toml.find("[obs]").unwrap();
        let end =
            toml[start + 1..].find("\n[").map(|offset| start + 1 + offset).unwrap_or(toml.len());
        toml.replace_range(start..end, "");

        let config: AppConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.obs, ObsConfig::default());
    }
}
