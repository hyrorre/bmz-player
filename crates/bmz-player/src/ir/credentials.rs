use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// IR provider ごとの認証情報。
///
/// docs/ir.md は OS credential store を最終目標としているが、v1 では
/// プロファイルディレクトリ配下の JSON ファイル (unix では 0600) に保存する。
/// `data/` は gitignore 管理のためリポジトリには入らない。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrStoredCredentials {
    pub provider: String,
    pub account_id: String,
    #[serde(default)]
    pub display_name: String,
    pub access_token: String,
    pub refresh_token: String,
    /// access_token の失効時刻 (unix 秒)。不明なら None。
    #[serde(default)]
    pub expires_at: Option<i64>,
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

pub fn load_credentials(
    profile_root: &Path,
    provider: &str,
) -> Result<Option<IrStoredCredentials>> {
    let path = credentials_path(profile_root, provider);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read IR credentials: {}", path.display()))?;
    let credentials = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse IR credentials: {}", path.display()))?;
    Ok(Some(credentials))
}

pub fn save_credentials(profile_root: &Path, credentials: &IrStoredCredentials) -> Result<()> {
    let path = credentials_path(profile_root, &credentials.provider);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(credentials)?;
    std::fs::write(&path, raw)
        .with_context(|| format!("failed to write IR credentials: {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn delete_credentials(profile_root: &Path, provider: &str) -> Result<bool> {
    let path = credentials_path(profile_root, provider);
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&path)?;
    Ok(true)
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
        let credentials = IrStoredCredentials {
            provider: "bmz-official".to_string(),
            account_id: "account-1".to_string(),
            display_name: "Player".to_string(),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            expires_at: Some(1_700_000_000),
        };

        save_credentials(&root, &credentials).unwrap();
        let loaded = load_credentials(&root, "bmz-official").unwrap();
        assert_eq!(loaded, Some(credentials));

        assert!(delete_credentials(&root, "bmz-official").unwrap());
        assert_eq!(load_credentials(&root, "bmz-official").unwrap(), None);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn needs_refresh_uses_expiry_margin() {
        let credentials = IrStoredCredentials {
            provider: "bmz-official".to_string(),
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
}
