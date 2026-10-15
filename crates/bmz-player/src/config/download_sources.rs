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
            Self::Custom => "",
            Self::Ginger => "https://gingerrush.com/download/package/{md5}",
            Self::Konmai => "https://bms.alvorna.com/api/hash?md5={md5}",
            Self::Wriggle => "https://bms.wrigglebug.xyz/download/package/{md5}",
        }
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
    fn source_hash_requirements_are_explicit() {
        for preset in
            [HttpDownloadPreset::Ginger, HttpDownloadPreset::Konmai, HttpDownloadPreset::Wriggle]
        {
            assert!(template_supports_hashes(preset.api_url(), true, false));
            assert!(!template_supports_hashes(preset.api_url(), false, true));
        }
        assert!(template_supports_hashes("https://custom.test/{sha256}", false, true));
        assert!(!template_supports_hashes("https://custom.test/{md5}/{sha256}", false, true));
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
