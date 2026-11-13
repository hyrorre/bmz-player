use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::profile_config::IrConfig;
use crate::ir::provider_key::configured_provider_key;
use crate::ir::secret_store::SecretSlot;

static CREDENTIAL_STORE_LOCK: Mutex<()> = Mutex::new(());

/// IR provider ごとの認証情報。
///
/// 保存先は `secret_store` 経由で、profile.toml の `[ir] credential_store`
/// により File (既定) / OS credential store を切り替える。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrStoredCredentials {
    pub provider: String,
    #[serde(default)]
    pub origin: String,
    pub account_id: String,
    #[serde(default)]
    pub display_name: String,
    pub access_token: String,
    pub refresh_token: String,
    /// access_token の失効時刻 (unix 秒)。不明なら None。
    #[serde(default)]
    pub expires_at: Option<i64>,
}

pub fn credential_origin(base_url: &str) -> Result<String> {
    let url = reqwest::Url::parse(base_url).context("invalid IR base URL")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https") && url.has_host(),
        "invalid IR origin"
    );
    Ok(url.origin().ascii_serialization())
}

fn builtin_origin(provider: &str) -> Result<Option<String>> {
    let url = match provider.to_ascii_lowercase().as_str() {
        super::bmz_official::BMZ_IR_PROVIDER | "bmz-official" => {
            super::bmz_official::BMZ_IR_DEFAULT_BASE_URL
        }
        super::bms_ir::BMS_IR_PROVIDER => super::bms_ir::BMS_IR_DEFAULT_BASE_URL,
        super::bokutachi::PROVIDER => super::bokutachi::DEFAULT_BASE_URL,
        super::rian_ir::RIAN_IR_PROVIDER | "rianir" => super::rian_ir::RIAN_IR_DEFAULT_BASE_URL,
        _ => return Ok(None),
    };
    credential_origin(url).map(Some)
}

pub fn validate_login_origin(
    config: &IrConfig,
    row_index: Option<usize>,
    provider: &str,
    origin: &str,
) -> Result<()> {
    anyhow::ensure!(
        builtin_origin(provider)?.is_none_or(|builtin| builtin == origin),
        "IR provider key '{provider}' is reserved for the built-in origin"
    );
    let conflicts = config.providers.iter().enumerate().any(|(index, entry)| {
        Some(index) != row_index
            && entry.enabled
            && configured_provider_key(entry).is_some_and(|key| key.eq_ignore_ascii_case(provider))
            && credential_origin(&entry.base_url).is_ok_and(|other| other != origin)
    });
    anyhow::ensure!(
        !conflicts,
        "IR provider key '{provider}' is already in use at a different origin"
    );
    Ok(())
}

impl IrStoredCredentials {
    /// `now` 時点で access token を更新すべきか。失効の 60 秒前から true。
    pub fn needs_refresh(&self, now: i64) -> bool {
        match self.expires_at {
            Some(expires_at) => now >= expires_at - 60,
            None => false,
        }
    }
}

pub fn credentials_path(profile_root: &Path, provider: &str) -> PathBuf {
    profile_root.join("ir").join(format!("{provider}.json"))
}

fn credentials_slot<'a>(profile_root: &'a Path, provider: &'a str) -> SecretSlot<'a> {
    SecretSlot::new(
        "ir",
        provider,
        profile_root,
        credentials_path(profile_root, provider),
        crate::ir::secret_store::store_mode(profile_root),
    )
}

pub fn load_credentials(
    profile_root: &Path,
    provider: &str,
    base_url: &str,
) -> Result<Option<IrStoredCredentials>> {
    let _guard = CREDENTIAL_STORE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    load_matching_credentials(profile_root, provider, base_url)
}

fn load_matching_credentials(
    profile_root: &Path,
    provider: &str,
    base_url: &str,
) -> Result<Option<IrStoredCredentials>> {
    let Ok(origin) = credential_origin(base_url) else {
        return Ok(None);
    };
    let builtin = builtin_origin(provider)?;
    let Some(raw) = credentials_slot(profile_root, provider)
        .load_if(|raw| credentials_match(raw, provider, &origin, builtin.as_deref()))?
    else {
        return Ok(None);
    };
    let mut credentials: IrStoredCredentials =
        serde_json::from_str(&raw).context("failed to parse stored IR credentials")?;
    if credentials.origin.is_empty() {
        credentials.origin = origin;
        write_credentials(profile_root, &credentials)?;
    }
    Ok(Some(credentials))
}

fn credentials_match(
    raw: &str,
    provider: &str,
    origin: &str,
    builtin: Option<&str>,
) -> Result<bool> {
    let credentials: IrStoredCredentials =
        serde_json::from_str(raw).context("failed to parse stored IR credentials")?;
    Ok(credentials.provider == provider
        && (credentials.origin == origin
            || credentials.origin.is_empty() && builtin == Some(origin)))
}

pub fn save_credentials(profile_root: &Path, credentials: &IrStoredCredentials) -> Result<()> {
    let _guard = CREDENTIAL_STORE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    write_credentials(profile_root, credentials)
}

pub(super) fn replace_refreshed_credentials(
    profile_root: &Path,
    previous: &IrStoredCredentials,
    refreshed: &IrStoredCredentials,
) -> Result<()> {
    anyhow::ensure!(previous.origin == refreshed.origin, "IR refresh changed the origin");
    let _guard = CREDENTIAL_STORE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    anyhow::ensure!(
        load_matching_credentials(profile_root, &previous.provider, &previous.origin)?.as_ref()
            == Some(previous),
        "IR credentials changed during refresh; retry with the current account"
    );
    write_credentials(profile_root, refreshed)
}

fn write_credentials(profile_root: &Path, credentials: &IrStoredCredentials) -> Result<()> {
    let raw = serde_json::to_string_pretty(credentials)?;
    credentials_slot(profile_root, &credentials.provider).save(&raw)
}

pub fn delete_credentials(profile_root: &Path, provider: &str, base_url: &str) -> Result<bool> {
    let _guard = CREDENTIAL_STORE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let Ok(origin) = credential_origin(base_url) else {
        return Ok(false);
    };
    let builtin = builtin_origin(provider)?;
    credentials_slot(profile_root, provider)
        .delete_if(|raw| credentials_match(raw, provider, &origin, builtin.as_deref()))
}

pub(super) fn same_origin_redirects() -> reqwest::redirect::Policy {
    let limited = reqwest::redirect::Policy::default();
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().last().is_some_and(|from| from.origin() != attempt.url().origin()) {
            attempt.error("IR redirect changed the origin")
        } else {
            limited.redirect(attempt)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-ir-cred-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn credentials_round_trip() {
        let root = temp_root("round-trip");
        let url = super::super::bmz_official::BMZ_IR_DEFAULT_BASE_URL;
        let credentials = IrStoredCredentials {
            provider: "bmz-official".to_string(),
            origin: credential_origin(url).unwrap(),
            account_id: "account-1".to_string(),
            display_name: "Player".to_string(),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Some(1_700_000_000),
        };

        save_credentials(&root, &credentials).unwrap();
        let loaded = load_credentials(&root, "bmz-official", url).unwrap();
        assert_eq!(loaded, Some(credentials));

        assert!(delete_credentials(&root, "bmz-official", url).unwrap());
        assert_eq!(load_credentials(&root, "bmz-official", url).unwrap(), None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn needs_refresh_uses_expiry_margin() {
        let credentials = IrStoredCredentials {
            provider: "bmz-official".to_string(),
            origin: String::new(),
            account_id: String::new(),
            display_name: String::new(),
            access_token: String::new(),
            refresh_token: String::new(),
            expires_at: Some(1_000),
        };
        assert!(!credentials.needs_refresh(900));
        assert!(credentials.needs_refresh(940));
        assert!(credentials.needs_refresh(1_001));
    }

    #[test]
    fn credentials_are_loaded_by_exact_provider_key_only() {
        let root = temp_root("exact-provider-key");
        let credentials = IrStoredCredentials {
            provider: "bmz".to_string(),
            origin: credential_origin(super::super::bmz_official::BMZ_IR_DEFAULT_BASE_URL).unwrap(),
            account_id: "prod".to_string(),
            display_name: "Prod".to_string(),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Some(1_700_000_000),
        };

        save_credentials(&root, &credentials).unwrap();

        assert_eq!(load_credentials(&root, "bmz-dev", &credentials.origin).unwrap(), None);
        assert_eq!(load_credentials(&root, "bmz", &credentials.origin).unwrap(), Some(credentials));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn server_provider_keys_cannot_access_files_outside_the_ir_directory() {
        let root = temp_root("untrusted-provider");
        let profile = root.join("profile");
        std::fs::create_dir_all(profile.join("ir")).unwrap();
        let victim = root.join("victim.json");
        std::fs::write(&victim, "dummy original file").unwrap();
        let absolute = root.join("victim").to_string_lossy().into_owned();
        for provider in [
            "../../victim",
            "..\\..\\victim",
            absolute.as_str(),
            "",
            "a/b",
            "a:b",
            "CON",
            "nul",
            "COM1",
            "LPT9",
            ".",
            "..",
            "bad\0key",
            "日本語",
        ] {
            let credentials = IrStoredCredentials {
                provider: provider.to_string(),
                origin: "https://example.test".into(),
                account_id: "dummy".into(),
                display_name: String::new(),
                access_token: "dummy".into(),
                refresh_token: "dummy".into(),
                expires_at: None,
            };
            assert!(save_credentials(&profile, &credentials).is_err(), "{provider:?}");
            assert!(
                load_credentials(&profile, provider, &credentials.origin).is_err(),
                "{provider:?}"
            );
            assert!(
                delete_credentials(&profile, provider, &credentials.origin).is_err(),
                "{provider:?}"
            );
            assert_eq!(std::fs::read_to_string(&victim).unwrap(), "dummy original file");
        }
        assert!(std::fs::read_dir(profile.join("ir")).unwrap().next().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    fn fixture(provider: &str, origin: &str) -> IrStoredCredentials {
        IrStoredCredentials {
            provider: provider.into(),
            origin: origin.into(),
            account_id: "player".into(),
            display_name: "Player".into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: None,
        }
    }

    #[test]
    fn credentials_are_bound_to_the_normalized_origin() {
        let root = tempfile::tempdir().unwrap();
        let origin = "https://ir.example.test";
        let credentials = fixture("custom", origin);
        save_credentials(root.path(), &credentials).unwrap();
        for other in
            ["http://ir.example.test", "https://ir.example.test:8443", "https://other.example.test"]
        {
            assert_eq!(load_credentials(root.path(), "custom", other).unwrap(), None);
            assert!(!delete_credentials(root.path(), "custom", other).unwrap());
        }
        assert_eq!(
            load_credentials(root.path(), "custom", "https://IR.example.test:443/api/?foo=1#tag")
                .unwrap(),
            Some(credentials)
        );
    }

    #[test]
    fn legacy_credentials_are_migrated_only_for_builtin_keys_at_their_default_origin() {
        let root = tempfile::tempdir().unwrap();
        for provider in ["bmz", "bmz-official", "bms-ir", "rian-ir", "custom"] {
            let path = credentials_path(root.path(), provider);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut raw = serde_json::to_value(fixture(provider, "")).unwrap();
            raw.as_object_mut().unwrap().remove("origin");
            let raw = raw.to_string();
            std::fs::write(&path, &raw).unwrap();
            assert_eq!(
                load_credentials(root.path(), provider, "https://custom.example.test").unwrap(),
                None
            );
            assert!(
                !delete_credentials(root.path(), provider, "https://custom.example.test").unwrap()
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
            if let Some(origin) = builtin_origin(provider).unwrap() {
                let migrated = load_credentials(root.path(), provider, &origin).unwrap().unwrap();
                assert_eq!(migrated.origin, origin);
                let stored: IrStoredCredentials =
                    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
                assert_eq!(stored, migrated);
            }
        }
    }

    #[test]
    fn login_rejects_reserved_keys_and_enabled_rows_at_other_origins() {
        let origin = "https://custom.example.test";
        let mut config = IrConfig::default();
        for key in ["bmz", "BMZ-OFFICIAL", "bms-ir", "rian-ir"] {
            assert!(validate_login_origin(&config, None, key, origin).is_err());
            assert!(
                validate_login_origin(&config, None, key, &builtin_origin(key).unwrap().unwrap())
                    .is_ok()
            );
        }
        let mut entry = crate::config::profile_config::IrProviderConfig::custom();
        entry.base_url = "https://other.example.test/".into();
        entry.provider_key = "custom".into();
        entry.enabled = true;
        config.providers.push(entry);
        let index = config.providers.len() - 1;
        assert!(validate_login_origin(&config, None, "CUSTOM", origin).is_err());
        assert!(validate_login_origin(&config, Some(index), "custom", origin).is_ok());
        assert!(
            validate_login_origin(&config, None, "custom", "https://other.example.test").is_ok()
        );
        config.providers[index].enabled = false;
        assert!(validate_login_origin(&config, None, "custom", origin).is_ok());
    }
}
