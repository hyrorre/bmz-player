use std::path::Path;

use anyhow::Result;

use super::app_config::{
    AppConfig, ensure_default_difficulty_table_sources, normalize_song_root_paths,
};
use super::play_input::{normalize_profile_input, validate_play_inherit_config};
use super::profile_config::ProfileConfig;

pub fn load_app_config(path: &Path) -> Result<AppConfig> {
    let text = std::fs::read_to_string(path)?;
    parse_app_config(&text)
}

fn parse_app_config(text: &str) -> Result<AppConfig> {
    let mut config: AppConfig = toml::from_str(text)?;
    normalize_song_root_paths(&mut config.songs.roots);
    ensure_default_difficulty_table_sources(&mut config);
    for slot in &mut config.input.gamepad_slot_device_ids {
        if slot.as_deref().is_some_and(|id| id.starts_with("gc-session:")) {
            *slot = None;
        }
    }
    crate::input::availability::BackendAvailability::current().normalize_config(&mut config.input);
    Ok(config)
}

pub fn load_profile_config(path: &Path) -> Result<ProfileConfig> {
    let text = std::fs::read_to_string(path)?;
    parse_profile_config(&text)
}

fn parse_profile_config(text: &str) -> Result<ProfileConfig> {
    let mut config: ProfileConfig = toml::from_str(text)?;
    config.migrate_legacy_key_mode_conversion();
    config.normalize_play_mode_configs();
    config.skin.migrate_legacy_offsets();
    config.skin.migrate_legacy_ambient_option();
    config.skin.migrate_antique_bga_brightness();
    config.ir.normalize_builtin_providers();
    normalize_profile_input(&mut config.input);
    validate_play_inherit_config(&config.input).map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::{GamepadBackendKind, InputBackendKind, PathEntry};

    #[test]
    fn loaded_input_choices_are_available_on_the_current_platform() {
        let available = crate::input::availability::BackendAvailability::current();
        for backend in [
            InputBackendKind::RawInput,
            InputBackendKind::MacOsHid,
            InputBackendKind::MacOsGameController,
        ] {
            let mut config = AppConfig::default();
            config.input.backend = backend;
            config.input.gamepad_backend = GamepadBackendKind::GameController;
            let loaded = parse_app_config(&toml::to_string(&config).unwrap()).unwrap();
            assert!(available.keyboards().any(|choice| choice == loaded.input.backend));
            assert!(available.gamepads().any(|choice| choice == loaded.input.gamepad_backend));
        }
    }

    #[test]
    fn parse_app_config_respects_iohid_feature() {
        let mut config = AppConfig::default();
        config.input.backend = InputBackendKind::MacOsHid;
        let loaded = parse_app_config(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(
            loaded.input.backend,
            if cfg!(all(target_os = "macos", feature = "macos-iohid")) {
                InputBackendKind::MacOsHid
            } else {
                InputBackendKind::Auto
            }
        );
        config.input.backend = loaded.input.backend.clone();
        assert_eq!(
            toml::to_string(&loaded.input).unwrap(),
            toml::to_string(&config.input).unwrap()
        );
    }

    #[test]
    fn parse_profile_config_migrates_only_antique_bga_brightness_and_preserves_new_choice() {
        use crate::config::profile_config::{SkinHistoryEntryConfig, SkinOffsetConfig};
        let antique = "data/skins/mz-select/play/antique/system/play7main.luaskin";
        let old = SkinOffsetConfig {
            id: 54,
            name: Some("BGAの明るさ(-255 ~ 0)".into()),
            a: -128,
            ..Default::default()
        };
        let new = SkinOffsetConfig {
            name: Some("前面BGAの明るさ(-255 ~ 0)".into()),
            a: -64,
            ..old.clone()
        };
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.skin.play7 = antique.replace('/', "\\");
        profile.skin.play7_offsets = vec![old.clone()];
        profile.skin.play5 = antique.into();
        profile.skin.play5_offsets = vec![old.clone(), new.clone()];
        profile.skin.play9 = "other/play.luaskin".into();
        profile.skin.play9_offsets = vec![old.clone()];
        let history_key = format!("play7::{antique}");
        profile.skin.history.insert(
            history_key.clone(),
            SkinHistoryEntryConfig { offsets: vec![old.clone()], ..Default::default() },
        );
        let loaded = parse_profile_config(&toml::to_string(&profile).unwrap()).unwrap();
        assert_eq!(loaded.skin.play7_offsets[0].name, new.name);
        assert_eq!(loaded.skin.play7_offsets[0].a, -128);
        assert_eq!(loaded.skin.play5_offsets, vec![new]);
        assert_eq!(loaded.skin.play9_offsets, vec![old]);
        assert_eq!(loaded.skin.history[&history_key].offsets, loaded.skin.play7_offsets);
        let reloaded = parse_profile_config(&toml::to_string(&loaded).unwrap()).unwrap();
        assert_eq!(
            toml::to_string(&loaded.skin).unwrap(),
            toml::to_string(&reloaded.skin).unwrap()
        );
    }

    #[test]
    fn parse_profile_config_migrates_antique_ambient_and_history_without_overwriting_new_choice() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.skin.play7_options.insert("Ambientモード (BMZ)".into(), "ON".into());
        profile.skin.play5_options.insert("Ambientモード (BMZ)".into(), "ON".into());
        profile.skin.play5_options.insert("Ambient".into(), "OFF".into());
        let mut history = crate::config::profile_config::SkinHistoryEntryConfig::default();
        history.options.insert("Ambientモード (BMZ)".into(), "ON".into());
        profile.skin.history.insert("antique".into(), history);
        let loaded = parse_profile_config(&toml::to_string(&profile).unwrap()).unwrap();
        assert_eq!(loaded.skin.play7_options["Ambient"], "ON");
        assert_eq!(loaded.skin.play5_options["Ambient"], "OFF");
        assert_eq!(loaded.skin.history["antique"].options["Ambient"], "ON");
        assert!(!loaded.skin.play7_options.contains_key("Ambientモード (BMZ)"));
        let reloaded = parse_profile_config(&toml::to_string(&loaded).unwrap()).unwrap();
        assert_eq!(
            toml::to_string(&loaded.skin).unwrap(),
            toml::to_string(&reloaded.skin).unwrap()
        );
    }

    #[test]
    fn parse_app_config_normalizes_and_deduplicates_song_roots() {
        let mut config = AppConfig::default();
        config.songs.roots = vec![
            PathEntry { path: r"\\?\G:\BMS".to_string(), enabled: false, recursive: false },
            PathEntry { path: "G:/BMS".to_string(), enabled: true, recursive: true },
        ];
        let text = toml::to_string(&config).unwrap();

        let loaded = parse_app_config(&text).unwrap();

        assert_eq!(loaded.songs.roots.len(), 1);
        assert_eq!(loaded.songs.roots[0].path, "G:/BMS");
        assert!(!loaded.songs.roots[0].enabled);
        assert!(!loaded.songs.roots[0].recursive);
    }

    #[test]
    fn parse_app_config_migrates_removed_input_backends_to_auto() {
        for backend in [InputBackendKind::Hid, InputBackendKind::Midi] {
            let mut config = AppConfig::default();
            config.input.backend = backend;

            let loaded = parse_app_config(&toml::to_string(&config).unwrap()).unwrap();

            assert_eq!(loaded.input.backend, InputBackendKind::Auto);
        }
    }

    #[cfg(not(all(windows, feature = "experimental-gameinput")))]
    #[test]
    fn parse_app_config_migrates_disabled_gameinput_to_gilrs() {
        let mut config = AppConfig::default();
        config.input.gamepad_backend = GamepadBackendKind::GameInput;

        let loaded = parse_app_config(&toml::to_string(&config).unwrap()).unwrap();

        assert_eq!(loaded.input.gamepad_backend, GamepadBackendKind::Gilrs);
    }

    #[test]
    fn parse_profile_config_restores_builtin_ir_provider_slots() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.ir.providers.clear();
        let text = toml::to_string(&profile).unwrap();

        let loaded = parse_profile_config(&text).unwrap();

        assert_eq!(loaded.ir.providers.len(), 3);
        assert_eq!(
            loaded.ir.providers[0],
            crate::config::profile_config::IrProviderConfig::bmz_ir()
        );
        assert_eq!(
            loaded.ir.providers[1],
            crate::config::profile_config::IrProviderConfig::rian_ir()
        );
        assert_eq!(
            loaded.ir.providers[2],
            crate::config::profile_config::IrProviderConfig::bms_ir()
        );
    }

    #[test]
    fn parse_profile_config_migrates_legacy_lane_values_to_all_key_modes() {
        use bmz_core::lane::KeyMode;

        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play_mode.clear();
        profile.lane.target_green_number = 267;
        profile.judge.visual_offset_us = -9_000;

        let loaded = parse_profile_config(&toml::to_string(&profile).unwrap()).unwrap();

        for key_mode in crate::config::profile_config::PLAY_MODE_CONFIG_MODES {
            let config = loaded.play_mode_config(key_mode);
            assert_eq!(config.target_green_number, 267, "{}", key_mode.as_str());
            assert_eq!(config.visual_offset_us, -9_000, "{}", key_mode.as_str());
        }
        assert_eq!(loaded.active_play_mode, KeyMode::K7);
    }

    #[test]
    fn parse_profile_config_preserves_distinct_key_mode_values() {
        use bmz_core::lane::KeyMode;

        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.normalize_play_mode_configs();
        profile.activate_play_mode(KeyMode::K14);
        profile.lane.target_green_number = 225;
        profile.judge.visual_offset_us = 13_000;
        profile.sync_active_play_mode();

        let loaded = parse_profile_config(&toml::to_string(&profile).unwrap()).unwrap();

        assert_eq!(loaded.play_mode_config(KeyMode::K7).target_green_number, 300);
        assert_eq!(loaded.play_mode_config(KeyMode::K7).visual_offset_us, 0);
        assert_eq!(loaded.play_mode_config(KeyMode::K14).target_green_number, 225);
        assert_eq!(loaded.play_mode_config(KeyMode::K14).visual_offset_us, 13_000);
    }
}
