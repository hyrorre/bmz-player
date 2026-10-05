use super::*;

#[test]
fn experimental_detail_options_defaults_off_and_round_trips() {
    let mut config = AppConfig::default();
    assert!(!config.select.experimental_detail_options);
    let old =
        toml::to_string(&config).unwrap().replace("experimental_detail_options = false\n", "");
    assert!(!toml::from_str::<AppConfig>(&old).unwrap().select.experimental_detail_options);
    config.select.experimental_detail_options = true;
    let reloaded: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert!(reloaded.select.experimental_detail_options);
}

#[test]
fn app_config_loads_missing_logging_section_with_defaults() {
    let serialized = toml::to_string(&AppConfig::default()).unwrap();
    let mut document: toml::Value = toml::from_str(&serialized).unwrap();
    document.as_table_mut().unwrap().remove("logging");

    let config: AppConfig = document.try_into().unwrap();

    assert_eq!(config.logging.level, LogLevel::Info);
    assert!(config.logging.file_logging);
}

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
    assert!(!config.scan.use_everything);
    assert!(!config.scan.auto_rescan_on_startup);
    assert_eq!(config.video.vsync_mode, VsyncModeConfig::Vsync);
    assert_eq!(config.video.frame_latency_mode, FrameLatencyModeConfig::Auto);
    assert_eq!(config.video.internal_resolution, InternalResolutionModeConfig::Native);
    assert_eq!(config.video.frame_limit_in_background, 60);
}

#[test]
fn app_config_loads_missing_everything_setting_as_disabled() {
    let toml =
        toml::to_string(&AppConfig::default()).unwrap().replace("use_everything = false\n", "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert!(!config.scan.use_everything);
}

#[test]
fn app_config_loads_missing_frame_latency_mode_as_auto() {
    let toml = toml::to_string(&AppConfig::default())
        .unwrap()
        .replace("frame_latency_mode = \"Auto\"\n", "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.video.frame_latency_mode, FrameLatencyModeConfig::Auto);
}

#[test]
fn app_config_round_trips_explicit_frame_latency_mode() {
    let mut config = AppConfig::default();
    config.video.frame_latency_mode = FrameLatencyModeConfig::Stable;

    let toml = toml::to_string(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(loaded.video.frame_latency_mode, FrameLatencyModeConfig::Stable);
}

#[test]
fn app_config_defaults_fullscreen_monitor_to_primary() {
    let config = AppConfig::default();

    assert!(config.video.monitor_name.is_empty());
}

#[test]
fn app_config_loads_missing_fullscreen_monitor_as_primary() {
    let toml = toml::to_string(&AppConfig::default()).unwrap().replace("monitor_name = \"\"\n", "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert!(config.video.monitor_name.is_empty());
}

#[test]
fn app_config_round_trips_unlimited_target_fps() {
    let mut config = AppConfig::default();
    config.video.target_fps = 0;

    let toml = toml::to_string(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(loaded.video.target_fps, 0);
}

#[test]
fn app_config_round_trips_skin_internal_resolution() {
    let mut config = AppConfig::default();
    config.video.internal_resolution = InternalResolutionModeConfig::Skin;

    let toml = toml::to_string(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(loaded.video.internal_resolution, InternalResolutionModeConfig::Skin);
}

#[test]
fn app_config_loads_missing_internal_resolution_as_native() {
    let toml = toml::to_string(&AppConfig::default())
        .unwrap()
        .replace("internal_resolution = \"Native\"\n", "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.video.internal_resolution, InternalResolutionModeConfig::Native);
}

#[test]
fn app_config_defaults_gamepad_backend_to_gilrs() {
    let config = AppConfig::default();

    assert_eq!(config.input.gamepad_backend, GamepadBackendKind::Gilrs);
}

#[test]
fn app_config_round_trips_raw_input_gamepad_backend() {
    let mut config = AppConfig::default();
    config.input.gamepad_backend = GamepadBackendKind::RawInput;

    let toml = toml::to_string(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(loaded.input.gamepad_backend, GamepadBackendKind::RawInput);
}

#[test]
fn app_config_defaults_audio_output_to_standard_shared_mode() {
    let config = AppConfig::default();

    assert_eq!(config.audio.output_mode, AudioOutputMode::Shared);
}

#[test]
fn app_config_loads_missing_audio_output_mode_as_standard_shared() {
    let toml =
        toml::to_string(&AppConfig::default()).unwrap().replace("output_mode = \"Shared\"\n", "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.audio.output_mode, AudioOutputMode::Shared);
}

#[test]
fn app_config_round_trips_low_latency_shared_audio_mode() {
    let mut config = AppConfig::default();
    config.audio.output_mode = AudioOutputMode::SharedLowLatency;

    let toml = toml::to_string(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(loaded.audio.output_mode, AudioOutputMode::SharedLowLatency);
}

#[test]
fn app_config_round_trips_exclusive_audio_mode() {
    let mut config = AppConfig::default();
    config.audio.output_mode = AudioOutputMode::Exclusive;

    let toml = toml::to_string(&config).unwrap();
    let loaded: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(loaded.audio.output_mode, AudioOutputMode::Exclusive);
}

#[test]
fn legacy_exclusive_flag_migrates_to_exclusive_output_mode() {
    let toml = toml::to_string(&AppConfig::default())
        .unwrap()
        .replace("output_mode = \"Shared\"\n", "exclusive_mode = true\n");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.audio.output_mode, AudioOutputMode::Exclusive);
    assert!(!toml::to_string(&config).unwrap().contains("exclusive_mode"));
}

#[test]
fn explicit_output_mode_takes_priority_over_legacy_exclusive_flag() {
    let toml = toml::to_string(&AppConfig::default())
        .unwrap()
        .replace("output_mode = \"Shared\"", "output_mode = \"Shared\"\nexclusive_mode = true");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.audio.output_mode, AudioOutputMode::Shared);
}

#[test]
fn app_config_loads_missing_gamepad_backend_as_gilrs() {
    let toml = toml::to_string(&AppConfig::default())
        .unwrap()
        .replace("gamepad_backend = \"Gilrs\"\n", "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.input.gamepad_backend, GamepadBackendKind::Gilrs);
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
fn app_config_defaults_chart_downloads_disabled_without_api_urls() {
    let config = AppConfig::default();

    assert!(!config.downloads.ipfs_enabled);
    assert!(config.downloads.ipfs_api_url.is_empty());
    assert!(!config.downloads.http_enabled);
    assert!(config.downloads.http_api_url.is_empty());
}

#[test]
fn app_config_loads_missing_chart_downloads_section() {
    let mut serialized = toml::to_string(&AppConfig::default()).unwrap();
    let start = serialized.find("[downloads]").unwrap();
    let end = serialized[start + 1..]
        .find("\n[")
        .map(|offset| start + 1 + offset)
        .unwrap_or(serialized.len());
    serialized.replace_range(start..end, "");

    let config: AppConfig = toml::from_str(&serialized).unwrap();

    assert_eq!(config.downloads, ChartDownloadsConfig::default());
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
fn normalize_song_roots_keeps_first_entry_and_is_idempotent() {
    let mut roots = vec![
        PathEntry { path: r"\\?\G:\BMS\songs".to_string(), enabled: false, recursive: false },
        PathEntry { path: "G:/BMS/songs".to_string(), enabled: true, recursive: true },
        PathEntry { path: "H:/BMS".to_string(), enabled: true, recursive: true },
    ];

    assert!(normalize_song_root_paths(&mut roots));
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].path, "G:/BMS/songs");
    assert!(!roots[0].enabled);
    assert!(!roots[0].recursive);
    assert_eq!(roots[1].path, "H:/BMS");
    assert!(!normalize_song_root_paths(&mut roots));
}

#[test]
fn app_config_loads_missing_screenshot_section() {
    let mut toml = toml::to_string(&AppConfig::default()).unwrap();
    let start = toml.find("[screenshot]").unwrap();
    let end = toml[start + 1..].find("\n[").map(|offset| start + 1 + offset).unwrap_or(toml.len());
    toml.replace_range(start..end, "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.screenshot.dir, "screenshots");
    assert!(config.screenshot.copy_to_clipboard);
}

#[test]
fn app_config_loads_missing_updates_section() {
    let mut toml = toml::to_string(&AppConfig::default()).unwrap();
    let start = toml.find("[updates]").unwrap();
    let end = toml[start + 1..].find("\n[").map(|offset| start + 1 + offset).unwrap_or(toml.len());
    toml.replace_range(start..end, "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert!(config.updates.enabled);
    assert_eq!(config.updates.channel, UpdateChannelConfig::Stable);
}

#[test]
fn app_config_loads_missing_obs_section() {
    let mut toml = toml::to_string(&AppConfig::default()).unwrap();
    let start = toml.find("[obs]").unwrap();
    let end = toml[start + 1..].find("\n[").map(|offset| start + 1 + offset).unwrap_or(toml.len());
    toml.replace_range(start..end, "");

    let config: AppConfig = toml::from_str(&toml).unwrap();

    assert_eq!(config.obs, ObsConfig::default());
}
#[test]
fn macos_keyboard_backend_round_trip_preserves_opt_in() {
    let mut config = AppConfig::default();
    assert_eq!(config.input.backend, InputBackendKind::Auto);
    config.input.backend = InputBackendKind::MacOsHid;
    let loaded: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(loaded.input.backend, InputBackendKind::MacOsHid);
    config.input.backend = InputBackendKind::MacOsGameController;
    let loaded: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(loaded.input.backend, InputBackendKind::MacOsGameController);
    config.input.gamepad_backend = GamepadBackendKind::GameController;
    let loaded: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(loaded.input.gamepad_backend, GamepadBackendKind::GameController);
}
