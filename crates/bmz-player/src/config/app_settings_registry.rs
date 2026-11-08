use super::app_config::{
    AppConfig, AudioBackend, AudioBufferSizeMode, AudioOutputMode, AudioSampleRateMode,
    FrameLatencyModeConfig, InternalResolutionModeConfig, RendererBackend, VsyncModeConfig,
    WindowMode,
};
use crate::i18n::{AppLocale, Localizer};

const AUDIO_SAMPLE_RATES: [u32; 5] = [44_100, 48_000, 96_000, 192_000, 384_000];

/// 選曲画面から編集できる `data/config.toml` の音声・映像項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppSettingsEntryId {
    AudioBackend,
    AudioOutputMode,
    AudioSampleRate,
    AudioBufferMode,
    AudioBufferSize,
    AudioOutputDevice,
    AudioAsioDriver,
    AudioOutputChannelPair,
    VideoWindowMode,
    VideoWidth,
    VideoHeight,
    VideoInternalResolution,
    VideoMonitor,
    VideoVsyncMode,
    VideoFrameLatencyMode,
    VideoTargetFps,
    VideoBackgroundFps,
    VideoRenderer,
}

impl AppSettingsEntryId {
    pub const VIDEO_ENTRIES: &'static [Self] = &[
        Self::VideoWindowMode,
        Self::VideoWidth,
        Self::VideoHeight,
        Self::VideoInternalResolution,
        Self::VideoMonitor,
        Self::VideoVsyncMode,
        Self::VideoFrameLatencyMode,
        Self::VideoTargetFps,
        Self::VideoBackgroundFps,
        Self::VideoRenderer,
    ];

    pub const fn is_audio(self) -> bool {
        matches!(
            self,
            Self::AudioBackend
                | Self::AudioOutputMode
                | Self::AudioSampleRate
                | Self::AudioBufferMode
                | Self::AudioBufferSize
                | Self::AudioOutputDevice
                | Self::AudioAsioDriver
                | Self::AudioOutputChannelPair
        )
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::AudioBackend => "AUDIO BACKEND",
            Self::AudioOutputMode => "OUTPUT MODE",
            Self::AudioSampleRate => "SAMPLE RATE",
            Self::AudioBufferMode => "BUFFER MODE",
            Self::AudioBufferSize => "BUFFER SIZE",
            Self::AudioOutputDevice => "OUTPUT DEVICE",
            Self::AudioAsioDriver => "ASIO DRIVER",
            Self::AudioOutputChannelPair => "OUTPUT CHANNEL",
            Self::VideoWindowMode => "WINDOW MODE",
            Self::VideoWidth => "WINDOW WIDTH",
            Self::VideoHeight => "WINDOW HEIGHT",
            Self::VideoInternalResolution => "INTERNAL RESOLUTION",
            Self::VideoMonitor => "FULLSCREEN DISPLAY",
            Self::VideoVsyncMode => "VSYNC",
            Self::VideoFrameLatencyMode => "FRAME LATENCY",
            Self::VideoTargetFps => "TARGET FPS",
            Self::VideoBackgroundFps => "BACKGROUND FPS",
            Self::VideoRenderer => "RENDERER",
        }
    }

    pub const fn description_key(self) -> &'static str {
        match self {
            Self::AudioBackend => "settings-entry-description-audio-backend",
            Self::AudioOutputMode => "settings-entry-description-audio-output-mode",
            Self::AudioSampleRate => "settings-entry-description-audio-sample-rate",
            Self::AudioBufferMode => "settings-entry-description-audio-buffer-mode",
            Self::AudioBufferSize => "settings-entry-description-audio-buffer-size",
            Self::AudioOutputDevice => "settings-entry-description-audio-output-device",
            Self::AudioAsioDriver => "settings-entry-description-audio-asio-driver",
            Self::AudioOutputChannelPair => "settings-entry-description-audio-output-channel",
            Self::VideoWindowMode => "settings-entry-description-video-window-mode",
            Self::VideoWidth | Self::VideoHeight => "settings-entry-description-video-window-size",
            Self::VideoInternalResolution => "settings-entry-description-video-internal-resolution",
            Self::VideoMonitor => "settings-entry-description-video-monitor",
            Self::VideoVsyncMode => "settings-entry-description-video-vsync",
            Self::VideoFrameLatencyMode => "settings-entry-description-video-frame-latency",
            Self::VideoTargetFps => "settings-entry-description-video-target-fps",
            Self::VideoBackgroundFps => "settings-entry-description-video-background-fps",
            Self::VideoRenderer => "settings-entry-description-video-renderer",
        }
    }
}

#[derive(Debug, Clone)]
pub enum AppSettingsChoices {
    None,
    Text(Vec<String>),
    AudioBackends(Vec<AudioBackend>),
    Renderers(Vec<RendererBackend>),
}

pub fn format_app_settings_value(
    config: &AppConfig,
    entry_id: AppSettingsEntryId,
    locale: AppLocale,
) -> String {
    let text = Localizer::new(locale);
    match entry_id {
        AppSettingsEntryId::AudioBackend => audio_backend_label(&config.audio.backend, text),
        AppSettingsEntryId::AudioOutputMode => {
            audio_output_mode_label(&config.audio.output_mode, text)
        }
        AppSettingsEntryId::AudioSampleRate => {
            if config.audio.sample_rate_mode == AudioSampleRateMode::Auto {
                text.text("settings-audio-auto-driver-default")
            } else {
                audio_sample_rate_label(config.audio.sample_rate)
            }
        }
        AppSettingsEntryId::AudioBufferMode => match config.audio.buffer_size_mode {
            AudioBufferSizeMode::Auto => text.text("common-auto"),
            AudioBufferSizeMode::Fixed => text.text("common-fixed"),
        },
        AppSettingsEntryId::AudioBufferSize => format!("{} frames", config.audio.buffer_size),
        AppSettingsEntryId::AudioOutputDevice => {
            display_optional_name(&config.audio.output_device, text.text("common-default"))
        }
        AppSettingsEntryId::AudioAsioDriver => {
            display_optional_name(&config.audio.asio_driver, text.text("common-unspecified"))
        }
        AppSettingsEntryId::AudioOutputChannelPair => {
            audio_channel_pair_label(config.audio.output_channel_pair)
        }
        AppSettingsEntryId::VideoWindowMode => window_mode_label(&config.video.mode, text),
        AppSettingsEntryId::VideoWidth => format!("{} px", config.video.width),
        AppSettingsEntryId::VideoHeight => format!("{} px", config.video.height),
        AppSettingsEntryId::VideoInternalResolution => {
            internal_resolution_label(&config.video.internal_resolution, text)
        }
        AppSettingsEntryId::VideoMonitor => display_optional_name(
            &config.video.monitor_name,
            text.text("settings-video-primary-monitor"),
        ),
        AppSettingsEntryId::VideoVsyncMode => match config.video.vsync_mode {
            VsyncModeConfig::Vsync => "Vsync (Fifo)".to_string(),
            VsyncModeConfig::AdaptiveVsync => "Adaptive Vsync (Fifo Relaxed)".to_string(),
            VsyncModeConfig::VsyncOff => "Vsync Off (Immediate)".to_string(),
            VsyncModeConfig::FastVsync => "Fast Vsync (Mailbox)".to_string(),
        },
        AppSettingsEntryId::VideoFrameLatencyMode => {
            frame_latency_label(config.video.frame_latency_mode, text)
        }
        AppSettingsEntryId::VideoTargetFps => {
            if config.video.target_fps == 0 {
                "UNLIMITED".to_string()
            } else {
                format!("{} FPS", config.video.target_fps)
            }
        }
        AppSettingsEntryId::VideoBackgroundFps => {
            format!("{} FPS", config.video.frame_limit_in_background)
        }
        AppSettingsEntryId::VideoRenderer => renderer_label(&config.video.renderer, text),
    }
}

pub fn adjust_app_settings_value(
    config: &mut AppConfig,
    entry_id: AppSettingsEntryId,
    choices: &AppSettingsChoices,
    direction: i32,
) -> bool {
    if direction == 0 {
        return false;
    }
    let before = format_app_settings_value(config, entry_id, AppLocale::DEFAULT);
    match entry_id {
        AppSettingsEntryId::AudioBackend => {
            if let AppSettingsChoices::AudioBackends(values) = choices {
                cycle_value(&mut config.audio.backend, values, direction);
            }
        }
        AppSettingsEntryId::AudioOutputMode => cycle_value(
            &mut config.audio.output_mode,
            &[
                AudioOutputMode::Shared,
                AudioOutputMode::SharedLowLatency,
                AudioOutputMode::Exclusive,
            ],
            direction,
        ),
        AppSettingsEntryId::AudioSampleRate => {
            let current = if config.audio.sample_rate_mode == AudioSampleRateMode::Auto {
                0
            } else {
                AUDIO_SAMPLE_RATES
                    .iter()
                    .position(|rate| *rate == config.audio.sample_rate)
                    .map(|index| index + 1)
                    .unwrap_or(0)
            };
            let next = cycle_index(current, AUDIO_SAMPLE_RATES.len() + 1, direction);
            if next == 0 {
                config.audio.sample_rate_mode = AudioSampleRateMode::Auto;
            } else {
                config.audio.sample_rate_mode = AudioSampleRateMode::Fixed;
                config.audio.sample_rate = AUDIO_SAMPLE_RATES[next - 1];
            }
        }
        AppSettingsEntryId::AudioBufferMode => cycle_value(
            &mut config.audio.buffer_size_mode,
            &[AudioBufferSizeMode::Auto, AudioBufferSizeMode::Fixed],
            direction,
        ),
        AppSettingsEntryId::AudioBufferSize => {
            config.audio.buffer_size =
                adjust_bounded(config.audio.buffer_size, direction, 16, 16, 4096);
        }
        AppSettingsEntryId::AudioOutputDevice => {
            if let AppSettingsChoices::Text(values) = choices {
                cycle_value(&mut config.audio.output_device, values, direction);
            }
        }
        AppSettingsEntryId::AudioAsioDriver => {
            if let AppSettingsChoices::Text(values) = choices {
                cycle_value(&mut config.audio.asio_driver, values, direction);
            }
        }
        AppSettingsEntryId::AudioOutputChannelPair => {
            config.audio.output_channel_pair =
                cycle_index(config.audio.output_channel_pair as usize, 6, direction) as u32;
        }
        AppSettingsEntryId::VideoWindowMode => cycle_value(
            &mut config.video.mode,
            &[
                WindowMode::Windowed,
                WindowMode::BorderlessFullscreen,
                WindowMode::ExclusiveFullscreen,
            ],
            direction,
        ),
        AppSettingsEntryId::VideoWidth => {
            config.video.width = adjust_bounded(config.video.width, direction, 10, 640, 3840);
        }
        AppSettingsEntryId::VideoHeight => {
            config.video.height = adjust_bounded(config.video.height, direction, 10, 480, 2160);
        }
        AppSettingsEntryId::VideoInternalResolution => cycle_value(
            &mut config.video.internal_resolution,
            &[InternalResolutionModeConfig::Native, InternalResolutionModeConfig::Skin],
            direction,
        ),
        AppSettingsEntryId::VideoMonitor => {
            if let AppSettingsChoices::Text(values) = choices {
                cycle_value(&mut config.video.monitor_name, values, direction);
            }
        }
        AppSettingsEntryId::VideoVsyncMode => cycle_value(
            &mut config.video.vsync_mode,
            &[
                VsyncModeConfig::Vsync,
                VsyncModeConfig::AdaptiveVsync,
                VsyncModeConfig::VsyncOff,
                VsyncModeConfig::FastVsync,
            ],
            direction,
        ),
        AppSettingsEntryId::VideoFrameLatencyMode => cycle_value(
            &mut config.video.frame_latency_mode,
            &[
                FrameLatencyModeConfig::Auto,
                FrameLatencyModeConfig::LowLatency,
                FrameLatencyModeConfig::Stable,
            ],
            direction,
        ),
        AppSettingsEntryId::VideoTargetFps => {
            config.video.target_fps = adjust_unbounded(config.video.target_fps, direction);
        }
        AppSettingsEntryId::VideoBackgroundFps => {
            config.video.frame_limit_in_background =
                adjust_bounded(config.video.frame_limit_in_background, direction, 1, 1, 120);
        }
        AppSettingsEntryId::VideoRenderer => {
            if let AppSettingsChoices::Renderers(values) = choices {
                cycle_value(&mut config.video.renderer, values, direction);
            }
        }
    }
    before != format_app_settings_value(config, entry_id, AppLocale::DEFAULT)
}

fn adjust_bounded(value: u32, direction: i32, step: u32, min: u32, max: u32) -> u32 {
    let delta = i64::from(direction) * i64::from(step);
    (i64::from(value) + delta).clamp(i64::from(min), i64::from(max)) as u32
}

fn adjust_unbounded(value: u32, direction: i32) -> u32 {
    if direction > 0 {
        value.saturating_add(direction as u32)
    } else {
        value.saturating_sub(direction.unsigned_abs())
    }
}

fn cycle_value<T: Clone + PartialEq>(value: &mut T, values: &[T], direction: i32) {
    if values.is_empty() {
        return;
    }
    let current = values
        .iter()
        .position(|candidate| candidate == value)
        .unwrap_or_else(|| if direction > 0 { values.len() - 1 } else { 0 });
    *value = values[cycle_index(current, values.len(), direction)].clone();
}

fn cycle_index(current: usize, len: usize, direction: i32) -> usize {
    (current as i64 + i64::from(direction)).rem_euclid(len as i64) as usize
}

fn display_optional_name(value: &str, fallback: String) -> String {
    if value.trim().is_empty() { fallback } else { value.to_string() }
}

fn audio_backend_label(backend: &AudioBackend, text: Localizer) -> String {
    match backend {
        AudioBackend::Auto => text.text("common-auto-select"),
        AudioBackend::Wasapi => "WASAPI".to_string(),
        AudioBackend::Asio => "ASIO".to_string(),
        AudioBackend::CoreAudio => "Core Audio".to_string(),
        AudioBackend::CoreAudioIoProc => "Core Audio IOProc".to_string(),
        AudioBackend::Alsa => "ALSA".to_string(),
        AudioBackend::Pulse => "PulseAudio".to_string(),
        AudioBackend::PipeWire => "PipeWire".to_string(),
    }
}

fn audio_output_mode_label(mode: &AudioOutputMode, text: Localizer) -> String {
    match mode {
        AudioOutputMode::Shared => text.text("settings-audio-output-mode-shared"),
        AudioOutputMode::SharedLowLatency => text.text("settings-audio-output-mode-low-latency"),
        AudioOutputMode::Exclusive => text.text("settings-audio-output-mode-exclusive"),
    }
}

fn audio_sample_rate_label(hz: u32) -> String {
    if hz.is_multiple_of(1000) {
        format!("{}kHz", hz / 1000)
    } else {
        format!("{:.1}kHz", hz as f64 / 1000.0)
    }
}

fn audio_channel_pair_label(pair: u32) -> String {
    let left = pair * 2 + 1;
    format!("{}-{}ch", left, left + 1)
}

fn window_mode_label(mode: &WindowMode, text: Localizer) -> String {
    match mode {
        WindowMode::Windowed => text.text("settings-windowed"),
        WindowMode::BorderlessFullscreen => text.text("settings-borderless-fullscreen"),
        WindowMode::ExclusiveFullscreen => text.text("settings-exclusive-fullscreen"),
    }
}

fn internal_resolution_label(mode: &InternalResolutionModeConfig, text: Localizer) -> String {
    match mode {
        InternalResolutionModeConfig::Native => {
            text.text("settings-video-internal-resolution-native")
        }
        InternalResolutionModeConfig::Skin => text.text("settings-video-internal-resolution-skin"),
    }
}

fn frame_latency_label(mode: FrameLatencyModeConfig, text: Localizer) -> String {
    match mode {
        FrameLatencyModeConfig::Auto => text.text("settings-video-frame-latency-auto"),
        FrameLatencyModeConfig::LowLatency => text.text("settings-video-frame-latency-low-latency"),
        FrameLatencyModeConfig::Stable => text.text("settings-video-frame-latency-stable"),
    }
}

fn renderer_label(backend: &RendererBackend, text: Localizer) -> String {
    match backend {
        RendererBackend::Auto => text.text("common-auto-select"),
        RendererBackend::Vulkan => "Vulkan".to_string(),
        RendererBackend::Metal => "Metal".to_string(),
        RendererBackend::Dx12 => "DirectX 12".to_string(),
        RendererBackend::Gl => "OpenGL".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_buffer_adjustment_reaches_and_stops_at_16_frames() {
        let mut config = AppConfig::default();
        config.audio.buffer_size = 32;
        assert!(adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::AudioBufferSize,
            &AppSettingsChoices::None,
            -1
        ));
        assert_eq!(config.audio.buffer_size, 16);
        assert!(!adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::AudioBufferSize,
            &AppSettingsChoices::None,
            -1
        ));
    }

    #[test]
    fn sample_rate_cycles_between_auto_and_fixed_presets() {
        let mut config = AppConfig::default();
        assert!(adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::AudioSampleRate,
            &AppSettingsChoices::None,
            1,
        ));
        assert_eq!(config.audio.sample_rate_mode, AudioSampleRateMode::Fixed);
        assert_eq!(config.audio.sample_rate, 44_100);

        assert!(adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::AudioSampleRate,
            &AppSettingsChoices::None,
            -1,
        ));
        assert_eq!(config.audio.sample_rate_mode, AudioSampleRateMode::Auto);
    }

    #[test]
    fn dynamic_text_choices_keep_the_default_choice() {
        let mut config = AppConfig::default();
        let choices = AppSettingsChoices::Text(vec![String::new(), "Speakers".to_string()]);
        assert!(adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::AudioOutputDevice,
            &choices,
            1,
        ));
        assert_eq!(config.audio.output_device, "Speakers");
    }

    #[test]
    fn video_numeric_values_use_ui_bounds() {
        let mut config = AppConfig::default();
        config.video.width = 3840;
        assert!(!adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::VideoWidth,
            &AppSettingsChoices::None,
            1,
        ));
        config.video.frame_limit_in_background = 1;
        assert!(!adjust_app_settings_value(
            &mut config,
            AppSettingsEntryId::VideoBackgroundFps,
            &AppSettingsChoices::None,
            -1,
        ));
    }
}
