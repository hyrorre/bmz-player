use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageKind {
    Portable,
    Installer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub schema: u32,
    pub kind: PackageKind,
    pub target: String,
    pub version: String,
    /// Minimum reader/apply protocol, not the bundled helper's own version.
    pub min_updater_protocol: u32,
    pub files: Vec<PackageFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasePackage {
    pub version: String,
    pub target: String,
    pub kind: PackageKind,
    pub min_updater_protocol: u32,
    /// A signed older release that upgrades the updater before this package can be read.
    pub bridge_tag: Option<String>,
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub schema: u32,
    pub packages: Vec<ReleasePackage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedManifest {
    pub payload: String,
    pub signature: String,
}

pub fn verify_release(bytes: &[u8], public_key: &str) -> Result<ReleaseManifest> {
    ensure!(bytes.len() <= 1024 * 1024, "update manifest too large");
    let signed: SignedManifest = serde_json::from_slice(bytes)?;
    let key: [u8; 32] = STANDARD
        .decode(public_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid update public key"))?;
    let payload = STANDARD.decode(signed.payload)?;
    let signature = Signature::from_slice(&STANDARD.decode(signed.signature)?)?;
    VerifyingKey::from_bytes(&key)?
        .verify_strict(&payload, &signature)
        .context("update signature verification failed")?;
    let manifest: ReleaseManifest = serde_json::from_slice(&payload)?;
    ensure!(manifest.schema == 1, "unsupported update manifest schema");
    for package in &manifest.packages {
        version(&package.version)?;
        safe_relative(&package.name)?;
        ensure!(!package.name.contains('/'), "invalid package filename");
        ensure!(valid_hash(&package.sha256), "invalid package hash");
        ensure!(package.size > 0 && package.size <= 8 * 1024 * 1024 * 1024, "invalid package size");
        let prefix = format!(
            "https://github.com/hyrorre/bmz-player/releases/download/v{}/",
            package.version
        );
        ensure!(package.url == format!("{prefix}{}", package.name), "invalid update URL");
    }
    Ok(manifest)
}

pub fn version(value: &str) -> Result<semver::Version> {
    semver::Version::parse(value.trim().strip_prefix('v').unwrap_or(value.trim()))
        .context("invalid release version")
}

pub fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn hash_file(path: &Path) -> Result<String> {
    hash_file_checked(path, &mut || Ok(()))
}

fn hash_file_checked(path: &Path, checkpoint: &mut impl FnMut() -> Result<()>) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0; 64 * 1024];
    loop {
        checkpoint()?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Validate using Windows rules even when package tests run on another OS.
pub fn safe_relative(value: &str) -> Result<PathBuf> {
    ensure!(!value.is_empty() && value.len() <= 1024, "invalid path length");
    for part in value.split('/') {
        ensure!(!part.is_empty() && part != "." && part != "..", "unsafe path: {value}");
        ensure!(
            !part.ends_with(['.', ' '])
                && !part.chars().any(|c| c < ' ' || "\\:<>\"|?*".contains(c)),
            "unsafe Windows path: {value}"
        );
        let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved_port = (base.starts_with("COM") || base.starts_with("LPT"))
            && base[3..].chars().count() == 1
            && "123456789¹²³".contains(&base[3..]);
        ensure!(
            !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&base.as_str())
                && !reserved_port,
            "reserved Windows path: {value}"
        );
    }
    Ok(PathBuf::from(value))
}

pub fn managed_path(value: &str) -> Result<PathBuf> {
    let path = safe_relative(value)?;
    let lower = value.to_ascii_lowercase();
    ensure!(
        value == super::MANIFEST
            || value == "bmz-player.exe"
            || value == "bmz-updater.exe"
            || (lower.ends_with(".dll") && !value.contains('/'))
            || value.starts_with("resources/"),
        "package must not manage user files: {value}"
    );
    Ok(path)
}

/// Reject links/reparse points at every existing component, including junctions.
pub fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    safe_relative(relative)?;
    let mut path = root.to_path_buf();
    reject_link(&path)?;
    for component in relative.split('/') {
        path.push(component);
        reject_link(&path)?;
    }
    Ok(path)
}

pub fn reject_link(path: &Path) -> Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    ensure!(!meta.file_type().is_symlink(), "symlink not allowed: {}", path.display());
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "reparse point not allowed: {}",
            path.display()
        );
    }
    Ok(())
}

impl PackageManifest {
    pub fn read(root: &Path) -> Result<Self> {
        let path = checked_path(root, super::MANIFEST)?;
        ensure!(path.metadata()?.len() <= 16 * 1024 * 1024, "package manifest too large");
        let manifest: Self = serde_json::from_reader(File::open(path)?)?;
        manifest.validate()?;
        Ok(manifest)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && self.min_updater_protocol <= super::PROTOCOL,
            "package requires a newer updater"
        );
        ensure!(
            self.target == "windows-x64" || self.target == "windows-arm64",
            "unsupported package target"
        );
        version(&self.version)?;
        ensure!(!self.files.is_empty() && self.files.len() <= 100_000, "invalid file count");
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for entry in &self.files {
            managed_path(&entry.path)?;
            ensure!(
                entry.path != super::MANIFEST && seen.insert(entry.path.to_lowercase()),
                "duplicate/reserved package file"
            );
            ensure!(valid_hash(&entry.sha256), "invalid file hash");
            total = total.checked_add(entry.size).context("package size overflow")?;
        }
        ensure!(total <= 16 * 1024 * 1024 * 1024, "package expands beyond limit");
        for required in ["bmz-player.exe", "bmz-updater.exe"] {
            ensure!(self.files.iter().any(|f| f.path == required), "missing {required}");
        }
        Ok(())
    }
    pub fn verify_files(
        &self,
        root: &Path,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        for entry in &self.files {
            checkpoint()?;
            let path = checked_path(root, &entry.path)?;
            if !path.is_file()
                || path.metadata()?.len() != entry.size
                || !hash_file_checked(&path, &mut checkpoint)?.eq_ignore_ascii_case(&entry.sha256)
            {
                bail!("package file verification failed: {}", entry.path);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn rejects_windows_aliases_traversal_user_data_and_reserved_names() {
        for path in [
            "../bmz-player.exe",
            "C:/app.exe",
            "/root",
            "resources/../data/score.db",
            "resources/x:stream",
            "resources/NUL.txt",
            "resources/COM1",
            "resources/LPT¹",
            "resources/a.",
            "resources/a ",
            "resources\\a",
            "data/score.db",
            "resources//a",
        ] {
            assert!(managed_path(path).is_err(), "{path}");
        }
        assert!(managed_path("resources/日本語 skin.png").is_ok());
        assert!(managed_path("bmz-updater.exe").is_ok());
    }

    #[test]
    fn authenticates_payload_before_decoding_release_instructions() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let payload = br#"{"schema":1,"packages":[]}"#;
        let mut signed = SignedManifest {
            payload: STANDARD.encode(payload),
            signature: STANDARD.encode(key.sign(payload).to_bytes()),
        };
        let public = STANDARD.encode(key.verifying_key().to_bytes());
        assert!(verify_release(&serde_json::to_vec(&signed).unwrap(), &public).is_ok());
        signed.payload = STANDARD.encode(br#"{"schema":2,"packages":[]}"#);
        assert!(verify_release(&serde_json::to_vec(&signed).unwrap(), &public).is_err());
        assert!(
            verify_release(&serde_json::to_vec(&signed).unwrap(), &STANDARD.encode([8; 32]))
                .is_err()
        );
    }
}
