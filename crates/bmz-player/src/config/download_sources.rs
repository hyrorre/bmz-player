use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpDownloadPreset {
    #[default]
    Custom,
    Ginger,
    Konmai,
    Wriggle,
}

impl HttpDownloadPreset {
    pub const ALL: [Self; 4] = [Self::Ginger, Self::Konmai, Self::Wriggle, Self::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "Custom",
            Self::Ginger => "Ginger",
            Self::Konmai => "Konmai",
            Self::Wriggle => "Wriggle",
        }
    }

    pub fn api_url(self) -> &'static str {
        match self {
            // Keep the retired variant readable in saved configurations, but
            // never supply a default endpoint for an unavailable service.
            Self::Custom | Self::Wriggle => "",
            Self::Ginger => "https://gingerrush.com/download/package/{md5}",
            Self::Konmai => "https://bms.alvorna.com/api/hash?md5={md5}",
        }
    }

    pub fn is_available(self) -> bool {
        self != Self::Wriggle
    }
}

/// A template declares exactly which hashes it needs. Presets use MD5 only;
/// a custom SHA256-only endpoint must explicitly use {sha256}.
pub fn template_supports_hashes(template: &str, has_md5: bool, has_sha256: bool) -> bool {
    let needs_md5 = template.contains("{md5}") || template.contains("%s");
    let needs_sha256 = template.contains("{sha256}");
    (needs_md5 || needs_sha256) && (!needs_md5 || has_md5) && (!needs_sha256 || has_sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_download_configs_extract_and_archive_choice_roundtrips() {
        use crate::config::app_config::ChartDownloadsConfig;
        let mut config: ChartDownloadsConfig = toml::from_str("http_enabled = true\n").unwrap();
        assert!(!config.http_keep_archive);
        config.http_keep_archive = true;
        let restored: ChartDownloadsConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored, config);
    }
    #[test]
    fn save_dirs_default_to_data_songs_subfolders_and_resolve_relative_paths() {
        use crate::config::app_config::ChartDownloadsConfig;
        let data_dir = std::path::Path::new("bmz-data");
        let mut config: ChartDownloadsConfig = toml::from_str(
            "http_enabled = true
",
        )
        .unwrap();
        assert!(config.ipfs_save_dir.is_empty() && config.http_save_dir.is_empty());
        assert_eq!(config.ipfs_save_path(data_dir), data_dir.join("songs/ipfs"));
        assert_eq!(config.http_save_path(data_dir), data_dir.join("songs/http"));
        config.ipfs_save_dir = "  ".into();
        assert_eq!(config.ipfs_save_path(data_dir), data_dir.join("songs/ipfs"));
        // 指定したフォルダをそのまま使い、方式別のサブフォルダを付けない。
        config.ipfs_save_dir = "downloads/bms-ipfs".into();
        assert_eq!(config.ipfs_save_path(data_dir), data_dir.join("downloads/bms-ipfs"));
        assert_eq!(config.http_save_path(data_dir), data_dir.join("songs/http"));
        // 空白を含むフォルダ名は、前後の空白も含めてそのまま使う。
        config.http_save_dir = " My BMS ".into();
        assert_eq!(config.http_save_path(data_dir), data_dir.join(" My BMS "));
        let absolute = std::env::temp_dir().join("bmz downloads ");
        config.http_save_dir = absolute.to_string_lossy().into_owned();
        assert_eq!(config.http_save_path(data_dir), absolute);
        let restored: ChartDownloadsConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored, config);
    }

    #[test]
    fn legacy_save_dir_migrates_to_per_source_folders() {
        use crate::config::app_config::ChartDownloadsConfig;
        let data_dir = std::path::Path::new("bmz-data");
        let mut config: ChartDownloadsConfig = toml::from_str(
            "save_dir = \"My BMS\"
",
        )
        .unwrap();
        config.migrate_legacy_save_dir();
        assert!(config.legacy_save_dir.is_empty());
        assert_eq!(config.ipfs_save_path(data_dir), data_dir.join("My BMS").join("ipfs"));
        assert_eq!(config.http_save_path(data_dir), data_dir.join("My BMS").join("http"));
        let saved = toml::to_string(&config).unwrap();
        assert!(!saved.contains("save_dir = \"My BMS\""), "{saved}");
        let mut restored: ChartDownloadsConfig = toml::from_str(&saved).unwrap();
        restored.migrate_legacy_save_dir();
        assert_eq!(restored, config);

        // 方式別の値が既にあれば旧値で上書きしない。空白だけの旧値は未設定として扱う。
        let mut config: ChartDownloadsConfig = toml::from_str(
            "save_dir = \"old\"
http_save_dir = \"new-http\"
",
        )
        .unwrap();
        config.migrate_legacy_save_dir();
        assert_eq!(config.http_save_dir, "new-http");
        assert_eq!(config.ipfs_save_path(data_dir), data_dir.join("old").join("ipfs"));
        let mut config: ChartDownloadsConfig = toml::from_str(
            "save_dir = \"  \"
",
        )
        .unwrap();
        config.migrate_legacy_save_dir();
        assert!(config.ipfs_save_dir.is_empty() && config.http_save_dir.is_empty());
    }

    #[test]
    fn source_hash_requirements_are_explicit() {
        for preset in [HttpDownloadPreset::Ginger, HttpDownloadPreset::Konmai] {
            assert!(template_supports_hashes(preset.api_url(), true, false));
            assert!(!template_supports_hashes(preset.api_url(), false, true));
        }
        assert!(template_supports_hashes("https://custom.test/{sha256}", false, true));
        assert!(!template_supports_hashes("https://custom.test/{md5}/{sha256}", false, true));
    }

    #[test]
    fn retired_wriggle_preset_has_no_default_but_preserves_explicit_urls() {
        use crate::config::app_config::ChartDownloadsConfig;
        let mut config: ChartDownloadsConfig =
            toml::from_str("http_source = 'wriggle'\nhttp_enabled = true\n").unwrap();
        assert_eq!(config.http_source, HttpDownloadPreset::Wriggle);
        assert!(!config.http_source.is_available());
        assert!(config.effective_http_api_url().is_empty());
        assert!(config.http_enabled);
        for preset in
            [HttpDownloadPreset::Ginger, HttpDownloadPreset::Konmai, HttpDownloadPreset::Custom]
        {
            assert!(preset.is_available());
        }
        config.http_api_url = "https://custom.test/{md5}".into();
        let reloaded: ChartDownloadsConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(reloaded.effective_http_api_url(), "https://custom.test/{md5}");
        assert_eq!(reloaded.http_source, HttpDownloadPreset::Wriggle);
    }

    #[test]
    fn legacy_custom_urls_and_disabled_http_remain_authoritative() {
        use crate::config::app_config::ChartDownloadsConfig;
        let mut config: ChartDownloadsConfig =
            toml::from_str("http_enabled = false\nhttp_api_url = 'https://custom.test/{sha256}'\n")
                .unwrap();
        assert!(!config.http_enabled);
        assert_eq!(config.http_source, HttpDownloadPreset::Custom);
        assert_eq!(config.http_parallelism, 3);
        config.http_source = HttpDownloadPreset::Ginger;
        assert_eq!(config.effective_http_api_url(), "https://custom.test/{sha256}");
        config.http_api_url.clear();
        assert_eq!(config.effective_http_api_url(), HttpDownloadPreset::Ginger.api_url());
    }
}
