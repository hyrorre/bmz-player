use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::paths::normalize_library_path;

pub const DEFAULT_DISCORD_APPLICATION_ID: &str = "1524506927315419448";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(skip)]
    pub cli_window_baseline: Option<Box<(crate::cli::WindowOverrides, VideoConfig)>>,
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
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tables: DifficultyTablesConfig,
    #[serde(default)]
    pub downloads: ChartDownloadsConfig,
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

/// 曲ルートを正規化し、区切り文字やWindows拡張パス接頭辞だけが異なる重複を取り除く。
///
/// 表示順と設定値を安定させるため、同一パスでは先頭の entry を残す。
pub fn normalize_song_root_paths(roots: &mut Vec<PathEntry>) -> bool {
    let mut changed = false;
    for root in roots.iter_mut() {
        let normalized = normalize_library_path(&root.path);
        if root.path != normalized {
            root.path = normalized;
            changed = true;
        }
    }

    let mut seen = HashSet::with_capacity(roots.len());
    roots.retain(|root| {
        if seen.insert(root.path.clone()) {
            true
        } else {
            changed = true;
            false
        }
    });
    changed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    pub follow_symlinks: bool,
    pub skip_hidden: bool,
    /// Windows の Everything インデックスを曲ファイル探索に使用する。
    /// 利用できない場合は通常のファイルシステム探索へフォールバックする。
    #[serde(default)]
    pub use_everything: bool,
    pub auto_rescan_on_startup: bool,
    pub rescan_missing_files: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "AudioConfigWire")]
pub struct AudioConfig {
    pub backend: AudioBackend,
    /// OS の通常共有出力、Windows の IAudioClient3 低遅延共有出力、または WASAPI 排他出力。
    #[serde(default)]
    pub output_mode: AudioOutputMode,
    pub output_device: String,
    /// `sample_rate_mode` が `Fixed` のときに要求するサンプルレート(Hz)。
    pub sample_rate: u32,
    /// サンプルレートの決定方法。`Auto` はドライバ / OS 既定を使用。
    #[serde(default)]
    pub sample_rate_mode: AudioSampleRateMode,
    pub buffer_size_mode: AudioBufferSizeMode,
    pub buffer_size: u32,
    pub asio_driver: String,
    /// 出力するステレオチャンネルペア(0 始まり)。0 = 1-2ch, 1 = 3-4ch, 2 = 5-6ch …。
    /// Babyface など多チャンネル出力デバイスで出力先ペアを選ぶ。デバイスの
    /// チャンネル数を超える指定はストリーム生成時にクランプされる。
    #[serde(default)]
    pub output_channel_pair: u32,
}

/// `exclusive_mode` は output mode 導入前の設定との読み込み互換にだけ使用する。
#[derive(Deserialize)]
struct AudioConfigWire {
    backend: AudioBackend,
    #[serde(default)]
    output_mode: Option<AudioOutputMode>,
    output_device: String,
    sample_rate: u32,
    #[serde(default)]
    sample_rate_mode: AudioSampleRateMode,
    buffer_size_mode: AudioBufferSizeMode,
    buffer_size: u32,
    #[serde(default)]
    exclusive_mode: bool,
    asio_driver: String,
    #[serde(default)]
    output_channel_pair: u32,
}

impl From<AudioConfigWire> for AudioConfig {
    fn from(value: AudioConfigWire) -> Self {
        let output_mode = value.output_mode.unwrap_or({
            if value.exclusive_mode { AudioOutputMode::Exclusive } else { AudioOutputMode::Shared }
        });
        Self {
            backend: value.backend,
            output_mode,
            output_device: value.output_device,
            sample_rate: value.sample_rate,
            sample_rate_mode: value.sample_rate_mode,
            buffer_size_mode: value.buffer_size_mode,
            buffer_size: value.buffer_size,
            asio_driver: value.asio_driver,
            output_channel_pair: value.output_channel_pair,
        }
    }
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum AudioOutputMode {
    #[default]
    Shared,
    SharedLowLatency,
    Exclusive,
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
    /// フルスクリーン時に使用するモニター。空文字列はプライマリモニターを意味する。
    #[serde(default)]
    pub monitor_name: String,
    pub width: u32,
    pub height: u32,
    /// 内部描画にウィンドウ解像度を使うか、現在シーンのスキン解像度を使うか。
    #[serde(default)]
    pub internal_resolution: InternalResolutionModeConfig,
    pub vsync_mode: VsyncModeConfig,
    /// Surfaceに許可するin-flight frame数の決定方法。
    #[serde(default)]
    pub frame_latency_mode: FrameLatencyModeConfig,
    /// 目標 FPS。0 はフレームペーサーによる待機を行わず、無制限を意味する。
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

/// 内部描画の解像度の決定方法。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum InternalResolutionModeConfig {
    /// 現在のウィンドウ解像度で内部描画する。
    #[default]
    Native,
    /// 現在のシーンの `SkinDocument` が宣言する幅・高さで内部描画する。
    Skin,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum FrameLatencyModeConfig {
    /// macOSのImmediateだけ2、それ以外は1を使う。
    #[default]
    Auto,
    /// 常に1を使い、入力から表示までの待ちを優先する。
    LowLatency,
    /// 常に2を使い、フレームペーシングの安定を優先する。
    Stable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalInputConfig {
    pub backend: InputBackendKind,
    #[serde(default)]
    pub gamepad_backend: GamepadBackendKind,
    pub keyboard_enabled: bool,
    pub gamepad_enabled: bool,
    /// 論理スロット `gamepad1` / `gamepad2` に割り当てるbackend非依存のデバイスID。
    #[serde(default, skip_serializing_if = "gamepad_stable_slots_unassigned")]
    pub gamepad_slot_device_ids: [Option<String>; 2],
    /// 旧設定との互換用gilrs `GamepadId`。stable IDへ移行後は保存しない。
    #[serde(
        default = "default_gamepad_slot_gilrs_ids",
        skip_serializing_if = "gamepad_slots_unassigned"
    )]
    pub gamepad_slot_gilrs_ids: [Option<u32>; 2],
    /// プレイ開始時に解決したbmz-gameplayのDeviceId。設定ファイルには保存しない。
    #[serde(skip)]
    pub gamepad_slot_runtime_device_ids: [Option<u32>; 2],
}

fn default_gamepad_slot_gilrs_ids() -> [Option<u32>; 2] {
    [None, None]
}

fn gamepad_slots_unassigned(slots: &[Option<u32>; 2]) -> bool {
    slots.iter().all(|slot| slot.is_none())
}

fn gamepad_stable_slots_unassigned(slots: &[Option<String>; 2]) -> bool {
    slots.iter().all(|slot| slot.is_none())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum GamepadBackendKind {
    Auto,
    #[default]
    Gilrs,
    RawInput,
    /// 既存configの読み込み互換と実験ビルド用にのみ保持する。
    GameInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum InputBackendKind {
    Auto,
    Winit,
    RawInput,
    /// 旧configの読み込み互換用。load時にAutoへ移行する。
    Hid,
    /// 旧configの読み込み互換用。load時にAutoへ移行する。
    Midi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]
    pub level: LogLevel,
    #[serde(default = "default_true")]
    pub file_logging: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { level: LogLevel::Info, file_logging: true }
    }
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
    "https://monibms.github.io/Dystopia/dystopia.html",
    "https://mocha-repository.info/table/ln_header.json",
    "https://ladymade-star.github.io/luminous/",
    "http://minddnim.web.fc2.com/sara/3rd_hard/bms_sara_3rd_hard.html",
    "https://egret9.github.io/Scramble/",
    "https://deltabms.yaruki0.net/table/data/dpdelta_head.json",
    "https://deltabms.yaruki0.net/table/data/insane_head.json",
    "https://stellabms.xyz/dpst/table.html",
    "https://stellabms.xyz/dp/table.html",
    "https://pmsdifficulty.xxxxxxxx.jp/PMSdifficulty.html",
    "https://pmsdifficulty.xxxxxxxx.jp/insane_PMSdifficulty.html",
    "https://pmsdifficulty.xxxxxxxx.jp/_pastoral_insane_table.html",
    "https://pmsdifficulty.xxxxxxxx.jp/_pastoral_upper.html",
    "https://hibyethere.github.io/table/",
    "https://classmaterma.github.io/4UE/table.html",
    "https://classmaterma.github.io/UE/table.html",
    "https://classmaterma.github.io/8UE/table.html",
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChartDownloadsConfig {
    #[serde(default)]
    pub ipfs_enabled: bool,
    #[serde(default)]
    pub ipfs_api_url: String,
    #[serde(default)]
    pub http_enabled: bool,
    #[serde(default)]
    pub http_api_url: String,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            cli_window_baseline: None,
            version: 1,
            active_profile: "default".to_string(),
            songs: SongPathsConfig { roots: Vec::new() },
            scan: ScanConfig {
                follow_symlinks: true,
                skip_hidden: true,
                use_everything: false,
                auto_rescan_on_startup: false,
                rescan_missing_files: true,
            },
            audio: AudioConfig {
                backend: AudioBackend::Auto,
                output_mode: AudioOutputMode::Shared,
                output_device: String::new(),
                sample_rate: 48_000,
                sample_rate_mode: AudioSampleRateMode::Auto,
                buffer_size_mode: AudioBufferSizeMode::Fixed,
                buffer_size: 256,
                asio_driver: String::new(),
                output_channel_pair: 0,
            },
            video: VideoConfig {
                mode: WindowMode::Windowed,
                monitor_name: String::new(),
                width: 1280,
                height: 720,
                internal_resolution: InternalResolutionModeConfig::Native,
                vsync_mode: VsyncModeConfig::Vsync,
                frame_latency_mode: FrameLatencyModeConfig::Auto,
                target_fps: 240,
                frame_limit_in_background: 60,
                renderer: RendererBackend::Auto,
            },
            screenshot: ScreenshotConfig::default(),
            obs: ObsConfig::default(),
            select: MusicSelectConfig::default(),
            input: GlobalInputConfig {
                backend: InputBackendKind::Auto,
                gamepad_backend: GamepadBackendKind::Gilrs,
                keyboard_enabled: true,
                gamepad_enabled: true,
                gamepad_slot_device_ids: [None, None],
                gamepad_slot_gilrs_ids: default_gamepad_slot_gilrs_ids(),
                gamepad_slot_runtime_device_ids: [None, None],
            },
            logging: LoggingConfig::default(),
            tables: DifficultyTablesConfig::default(),
            downloads: ChartDownloadsConfig::default(),
            updates: UpdatesConfig::default(),
            discord: DiscordConfig::default(),
        }
    }
}

#[cfg(test)]
#[path = "app_config/tests.rs"]
mod tests;
