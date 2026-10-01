use super::*;

pub(in crate::ui) fn difficulty_table_source_label(
    source_url: &str,
    difficulty_tables: &[DifficultyTableRecord],
) -> String {
    difficulty_tables
        .iter()
        .find(|table| table.source_url == source_url && !table.name.trim().is_empty())
        .map(|table| table.name.clone())
        .unwrap_or_else(|| source_url.to_string())
}

pub(in crate::ui) fn song_folder_display_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    std::path::Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_owned()
}

pub(in crate::ui) fn audio_backend_label(backend: &AudioBackend, text: Localizer) -> String {
    match backend {
        AudioBackend::Auto => tr!(text, "common-auto-select"),
        AudioBackend::Wasapi => "WASAPI".to_owned(),
        AudioBackend::Asio => "ASIO".to_owned(),
        AudioBackend::CoreAudio => "Core Audio".to_owned(),
        AudioBackend::Alsa => "ALSA".to_owned(),
        AudioBackend::Pulse => "PulseAudio".to_owned(),
        AudioBackend::PipeWire => "PipeWire".to_owned(),
    }
}

pub(in crate::ui) fn audio_output_mode_label(mode: &AudioOutputMode, text: Localizer) -> String {
    match mode {
        AudioOutputMode::Shared => tr!(text, "settings-audio-output-mode-shared"),
        AudioOutputMode::SharedLowLatency => {
            tr!(text, "settings-audio-output-mode-low-latency")
        }
        AudioOutputMode::Exclusive => tr!(text, "settings-audio-output-mode-exclusive"),
    }
}

pub(in crate::ui) fn audio_buffer_size_mode_label(
    mode: &AudioBufferSizeMode,
    text: Localizer,
) -> String {
    match mode {
        AudioBufferSizeMode::Auto => tr!(text, "common-auto"),
        AudioBufferSizeMode::Fixed => tr!(text, "common-fixed"),
    }
}

/// 出力チャンネルペア(0 始まり)を "1-2ch" のような表示文字列にする。
pub(in crate::ui) fn audio_channel_pair_label(pair: u32) -> String {
    let left = pair * 2 + 1;
    format!("{}-{}ch", left, left + 1)
}

/// サンプルレート(Hz)を "48kHz" / "44.1kHz" のような表示文字列にする。
pub(in crate::ui) fn audio_sample_rate_label(hz: u32) -> String {
    if hz.is_multiple_of(1000) {
        format!("{}kHz", hz / 1000)
    } else {
        format!("{:.1}kHz", hz as f64 / 1000.0)
    }
}

pub(in crate::ui) fn update_channel_label(channel: UpdateChannelConfig) -> &'static str {
    match channel {
        UpdateChannelConfig::Stable => "Stable",
        UpdateChannelConfig::Prerelease => "Prerelease",
    }
}

pub(in crate::ui) fn window_mode_label(mode: &WindowMode, text: Localizer) -> String {
    match mode {
        WindowMode::Windowed => tr!(text, "settings-windowed"),
        WindowMode::BorderlessFullscreen => tr!(text, "settings-borderless-fullscreen"),
        WindowMode::ExclusiveFullscreen => tr!(text, "settings-exclusive-fullscreen"),
    }
}

pub(in crate::ui) fn renderer_backend_label(backend: &RendererBackend, text: Localizer) -> String {
    match backend {
        RendererBackend::Auto => tr!(text, "common-auto-select"),
        RendererBackend::Vulkan => "Vulkan".to_owned(),
        RendererBackend::Metal => "Metal".to_owned(),
        RendererBackend::Dx12 => "DirectX 12".to_owned(),
        RendererBackend::Gl => "OpenGL".to_owned(),
    }
}

pub(in crate::ui) fn internal_resolution_mode_label(
    mode: &InternalResolutionModeConfig,
    text: Localizer,
) -> String {
    match mode {
        InternalResolutionModeConfig::Native => {
            tr!(text, "settings-video-internal-resolution-native")
        }
        InternalResolutionModeConfig::Skin => {
            tr!(text, "settings-video-internal-resolution-skin")
        }
    }
}

pub(in crate::ui) fn available_renderer_backends() -> Vec<RendererBackend> {
    bmz_render::available_wgpu_backends()
        .into_iter()
        .map(|backend| match backend {
            bmz_render::WgpuBackend::Auto => RendererBackend::Auto,
            bmz_render::WgpuBackend::Vulkan => RendererBackend::Vulkan,
            bmz_render::WgpuBackend::Metal => RendererBackend::Metal,
            bmz_render::WgpuBackend::Dx12 => RendererBackend::Dx12,
            bmz_render::WgpuBackend::Gl => RendererBackend::Gl,
        })
        .collect()
}

pub(in crate::ui) fn vsync_mode_label(mode: &VsyncModeConfig) -> &'static str {
    match mode {
        VsyncModeConfig::Vsync => "Vsync",
        VsyncModeConfig::AdaptiveVsync => "Adaptive Vsync",
        VsyncModeConfig::VsyncOff => "Vsync Off",
        VsyncModeConfig::FastVsync => "Fast Vsync",
    }
}

pub(in crate::ui) fn frame_latency_mode_label(
    mode: FrameLatencyModeConfig,
    text: Localizer,
) -> String {
    match mode {
        FrameLatencyModeConfig::Auto => tr!(text, "settings-video-frame-latency-auto"),
        FrameLatencyModeConfig::LowLatency => {
            tr!(text, "settings-video-frame-latency-low-latency")
        }
        FrameLatencyModeConfig::Stable => tr!(text, "settings-video-frame-latency-stable"),
    }
}

pub(in crate::ui) fn input_backend_label(backend: &InputBackendKind, text: Localizer) -> String {
    match backend {
        InputBackendKind::Auto => tr!(text, "common-auto-select"),
        InputBackendKind::Winit => "winit".to_owned(),
        InputBackendKind::MacOsHid => "macOS IOHID".to_owned(),
        InputBackendKind::MacOsGameController => "GCKeyboard".to_owned(),
        InputBackendKind::RawInput => tr!(text, "settings-input-raw-input"),
        // load時にAutoへ移行する旧config互換variant。
        InputBackendKind::Hid | InputBackendKind::Midi => tr!(text, "common-auto-select"),
    }
}

pub(in crate::ui) fn gamepad_backend_label(
    backend: &GamepadBackendKind,
    text: Localizer,
) -> String {
    match backend {
        GamepadBackendKind::Auto => tr!(text, "common-auto-select"),
        GamepadBackendKind::Gilrs => "gilrs".to_owned(),
        GamepadBackendKind::RawInput => tr!(text, "settings-input-raw-input"),
        GamepadBackendKind::GameInput => tr!(text, "settings-input-gameinput"),
        GamepadBackendKind::GameController => "GameController".to_owned(),
    }
}

pub(in crate::ui) fn log_level_label(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

pub(in crate::ui) fn add_difficulty_table_source(
    sources: &mut Vec<DifficultyTableSource>,
    url: &str,
    text: Localizer,
) -> Result<(), String> {
    if url.is_empty() {
        return Err(tr!(text, "settings-tables-url-required"));
    }
    if sources.iter().any(|source| source.url == url) {
        return Err(tr!(text, "settings-tables-url-duplicate"));
    }
    sources.push(DifficultyTableSource { url: url.to_string(), enabled: true });
    Ok(())
}
