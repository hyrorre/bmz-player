use std::io::Write;
use std::path::Path;

use anyhow::Result;

use super::app_config::{AppConfig, normalize_song_root_paths};
use super::profile_config::ProfileConfig;

pub fn save_app_config(path: &Path, config: &AppConfig) -> Result<()> {
    atomic_write(path, &serialize_app_config(config)?)?;
    Ok(())
}

fn serialize_app_config(config: &AppConfig) -> Result<String> {
    let mut config = config.clone();
    if let Some(state) = config.cli_window_baseline.take() {
        if state.0.mode.is_some() {
            config.video.mode = state.1.mode;
        }
        if state.0.size.is_some() {
            config.video.width = state.1.width;
            config.video.height = state.1.height;
        }
        if state.0.monitor.is_some() {
            config.video.monitor_name = state.1.monitor_name;
        }
    }
    normalize_song_root_paths(&mut config.songs.roots);
    Ok(toml::to_string_pretty(&config)?)
}

pub fn save_profile_config(path: &Path, profile: &ProfileConfig) -> Result<()> {
    let mut profile = profile.clone();
    profile.clear_cli_play();
    profile.migrate_legacy_key_mode_conversion();
    profile.sync_active_play_mode();
    profile.normalize_play_mode_configs();
    atomic_write(path, &toml::to_string_pretty(&profile)?)?;
    Ok(())
}

fn atomic_write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::PathEntry;

    #[test]
    fn serialize_app_config_normalizes_and_deduplicates_song_roots() {
        let mut config = AppConfig::default();
        config.songs.roots = vec![
            PathEntry { path: "//?/G:/BMS".to_string(), enabled: true, recursive: true },
            PathEntry { path: "G:/BMS".to_string(), enabled: false, recursive: false },
        ];

        let text = serialize_app_config(&config).unwrap();
        let saved: AppConfig = toml::from_str(&text).unwrap();

        assert_eq!(saved.songs.roots.len(), 1);
        assert_eq!(saved.songs.roots[0].path, "G:/BMS");
        assert!(saved.songs.roots[0].enabled);
        assert!(saved.songs.roots[0].recursive);
    }
}
