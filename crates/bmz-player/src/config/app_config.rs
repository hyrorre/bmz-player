use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub version: u32,
    pub active_profile: String,
    pub songs: SongPathsConfig,
    pub scan: ScanConfig,
    pub audio: AudioConfig,
    pub video: VideoConfig,
    #[serde(default)]
    pub select: MusicSelectConfig,
    pub input: GlobalInputConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tables: DifficultyTablesConfig,
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
    pub vsync: bool,
    #[serde(default)]
    pub present_mode: PresentModeConfig,
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
pub enum PresentModeConfig {
    /// `vsync` の値から従来通り AutoVsync / AutoNoVsync を選ぶ。
    #[default]
    Auto,
    AutoVsync,
    AutoNoVsync,
    Immediate,
    Mailbox,
    Fifo,
    FifoRelaxed,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

fn default_true() -> bool {
    true
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
                follow_symlinks: false,
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
                vsync: true,
                present_mode: PresentModeConfig::Auto,
                target_fps: 240,
                frame_limit_in_background: 30,
                renderer: RendererBackend::Auto,
            },
            select: MusicSelectConfig::default(),
            input: GlobalInputConfig {
                backend: InputBackendKind::Auto,
                keyboard_enabled: true,
                gamepad_enabled: true,
                midi_enabled: false,
            },
            logging: LoggingConfig { level: LogLevel::Info, file_logging: true },
            tables: DifficultyTablesConfig::default(),
        }
    }
}
