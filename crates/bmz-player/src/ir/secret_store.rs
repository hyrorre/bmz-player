//! IR の秘密情報 (refresh token / device key) の保存先抽象。
//!
//! docs/ir.md §2 の方針に従い、OS credential store
//! (macOS Keychain / Windows Credential Manager / Linux Secret Service) を
//! サポートする。開発時は再ビルドのたびに Keychain の許可ダイアログが出て
//! 煩わしいため、既定はプロファイル配下のファイル保存 (`File`) とし、
//! `profile.toml` の `[ir] credential_store = "Os"` で OS ストアへ切り替える。
//!
//! `Os` 選択時、既存のファイル保存があれば初回アクセスで OS ストアへ移行し、
//! 元ファイルを削除する。OS ストアが使えない環境ではファイルへフォールバック
//! する (警告ログつき)。

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};

use crate::config::profile_config::IrCredentialStoreConfig;

/// プロセス全体の保存先設定。起動時に profile config から一度だけ設定する。
/// 未設定なら `File`。credentials / device key の既存 API (profile_root +
/// provider のみ受け取る) を維持するためのプロセスグローバル。
static STORE_MODE: OnceLock<IrCredentialStoreConfig> = OnceLock::new();

pub fn set_store_mode(mode: IrCredentialStoreConfig) {
    let _ = STORE_MODE.set(mode);
}

pub fn store_mode() -> IrCredentialStoreConfig {
    STORE_MODE.get().copied().unwrap_or_default()
}

/// keyring の service 名。`kind` は "ir" (token) / "ir-device-key"。
fn keyring_service(kind: &str, provider: &str) -> String {
    format!("bmz.{kind}.{provider}")
}

/// keyring の user 名。同一マシン上の複数プロファイルが衝突しないよう
/// プロファイルディレクトリの絶対パスを使う。
fn keyring_user(profile_root: &Path) -> String {
    std::fs::canonicalize(profile_root)
        .unwrap_or_else(|_| profile_root.to_path_buf())
        .display()
        .to_string()
}

fn keyring_entry(kind: &str, provider: &str, profile_root: &Path) -> Result<keyring::Entry> {
    keyring::Entry::new(&keyring_service(kind, provider), &keyring_user(profile_root))
        .context("failed to open OS credential store entry")
}

/// 秘密情報 1 件分のストア。`kind` ごと (token / device key) に分ける。
pub struct SecretSlot<'a> {
    kind: &'static str,
    provider: &'a str,
    profile_root: &'a Path,
    file_path: PathBuf,
    store: IrCredentialStoreConfig,
}

impl<'a> SecretSlot<'a> {
    pub fn new(
        kind: &'static str,
        provider: &'a str,
        profile_root: &'a Path,
        file_path: PathBuf,
        store: IrCredentialStoreConfig,
    ) -> Self {
        Self { kind, provider, profile_root, file_path, store }
    }

    /// 保存済みの JSON 文字列を読む。`Os` 設定ならファイルからの移行も行う。
    pub fn load(&self) -> Result<Option<String>> {
        match self.store {
            IrCredentialStoreConfig::File => self.load_file(),
            IrCredentialStoreConfig::Os => {
                match keyring_entry(self.kind, self.provider, self.profile_root).and_then(|entry| {
                    match entry.get_password() {
                        Ok(secret) => Ok(Some(secret)),
                        Err(keyring::Error::NoEntry) => Ok(None),
                        Err(error) => {
                            Err(anyhow::Error::from(error)).context("OS credential store read")
                        }
                    }
                }) {
                    Ok(Some(secret)) => Ok(Some(secret)),
                    Ok(None) => {
                        // ファイル保存からの移行: 読めたら OS ストアへ書いて
                        // 元ファイルを消す。
                        let Some(secret) = self.load_file()? else {
                            return Ok(None);
                        };
                        if self.save_os(&secret).is_ok() {
                            let _ = std::fs::remove_file(&self.file_path);
                            tracing::info!(
                                kind = self.kind,
                                provider = self.provider,
                                "migrated IR secret from file to OS credential store"
                            );
                        }
                        Ok(Some(secret))
                    }
                    Err(error) => {
                        tracing::warn!(%error, "OS credential store unavailable; falling back to file");
                        self.load_file()
                    }
                }
            }
        }
    }

    pub fn save(&self, secret: &str) -> Result<()> {
        match self.store {
            IrCredentialStoreConfig::File => self.save_file(secret),
            IrCredentialStoreConfig::Os => self.save_os(secret).or_else(|error| {
                tracing::warn!(%error, "OS credential store unavailable; falling back to file");
                self.save_file(secret)
            }),
        }
    }

    /// 削除する。`Os` 設定時は OS ストアとファイルの両方を消す
    /// (設定切替後の取り残しを防ぐ)。`File` 設定時は OS ストアに触れない
    /// (不要な Keychain アクセスを避ける)。
    pub fn delete(&self) -> Result<bool> {
        let mut removed = false;
        if self.store == IrCredentialStoreConfig::Os
            && let Ok(entry) = keyring_entry(self.kind, self.provider, self.profile_root)
        {
            match entry.delete_credential() {
                Ok(()) => removed = true,
                Err(keyring::Error::NoEntry) => {}
                Err(error) => {
                    tracing::warn!(%error, "failed to delete secret from OS credential store");
                }
            }
        }
        if self.file_path.exists() {
            std::fs::remove_file(&self.file_path)?;
            removed = true;
        }
        Ok(removed)
    }

    fn load_file(&self) -> Result<Option<String>> {
        if !self.file_path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&self.file_path)
            .with_context(|| format!("failed to read IR secret: {}", self.file_path.display()))?;
        Ok(Some(raw))
    }

    fn save_file(&self, secret: &str) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.file_path, secret)
            .with_context(|| format!("failed to write IR secret: {}", self.file_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.file_path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    fn save_os(&self, secret: &str) -> Result<()> {
        keyring_entry(self.kind, self.provider, self.profile_root)?
            .set_password(secret)
            .context("failed to write secret to OS credential store")
    }
}
