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
