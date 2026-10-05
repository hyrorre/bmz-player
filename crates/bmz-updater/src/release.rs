//! Signed release.json: readable rianIR builds, archive checksums and update policy.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use crate::manifest::{PackageKind, ReleaseManifest, ReleasePackage, version};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema: String,
    pub client: String,
    pub version: String,
    pub git_commit: String,
    pub builds: Vec<Build>,
    pub artifacts: Vec<Artifact>,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub id: String,
    pub platform: String,
    pub arch: String,
    pub package_kind: String,
    pub client_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub name: String,
    pub build: String,
    pub kind: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub update: Option<Update>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    pub min_updater_protocol: u32,
    pub bridge_tag: Option<String>,
}

const BUILDS: &[(&str, &str, &str, &str)] = &[
    ("windows-x64", "windows", "x86_64", "portable-installer"),
    ("macos-arm64", "macos", "aarch64", "app"),
    ("macos-x64", "macos", "x86_64", "app"),
    ("linux-x64-flatpak", "linux", "x86_64", "flatpak"),
    ("linux-x64-tar", "linux", "x86_64", "tar"),
];
const ARTIFACTS: &[(&str, &str, &str)] = &[
    ("windows-x64-portable.zip", "windows-x64", "portable"),
    ("windows-x64-setup.exe", "windows-x64", "installer"),
    ("macos-arm64.app.zip", "macos-arm64", "app"),
    ("macos-x64.app.zip", "macos-x64", "app"),
    ("linux-x64.flatpak", "linux-x64-flatpak", "flatpak"),
    ("linux-x64.tar.gz", "linux-x64-tar", "tar"),
    ("linux-x64-sources.tar.gz", "linux-x64-tar", "sources"),
];

pub fn verify_release(bytes: &[u8], public_key: &str) -> Result<Release> {
    ensure!(bytes.len() <= 1024 * 1024, "release manifest too large");
    // Struct deserialization rejects duplicate and unknown fields at every level.
    // Do not use any URL/update instructions until authentication succeeds.
    let release: Release = serde_json::from_slice(bytes)?;
    let mut value: Value = serde_json::from_slice(bytes)?;
    value.as_object_mut().context("expected release object")?.remove("signature");
    let payload = canonical_json(&value)?;
    let key: [u8; 32] = STANDARD
        .decode(public_key)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid update public key"))?;
    let signature = Signature::from_slice(&STANDARD.decode(&release.signature)?)?;
    VerifyingKey::from_bytes(&key)?
        .verify_strict(payload.as_bytes(), &signature)
        .context("release signature verification failed")?;
    release.validate()?;
    Ok(release)
}

// RFC 8785 for this schema's integer-only profile. Serialize the original JSON,
// not reconstructed structs: omitted/null fields must not escape authentication.
fn canonical_json(value: &Value) -> Result<String> {
    Ok(match value {
        Value::Object(fields) => {
            let mut keys: Vec<_> = fields.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let fields = keys
                .into_iter()
                .map(|key| {
                    Ok(format!("{}:{}", serde_json::to_string(key)?, canonical_json(&fields[key])?))
                })
                .collect::<Result<Vec<_>>>()?;
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => {
            format!("[{}]", items.iter().map(canonical_json).collect::<Result<Vec<_>>>()?.join(","))
        }
        Value::Number(number) => {
            ensure!(
                number.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991),
                "expected a nonnegative safe integer"
            );
            number.to_string()
        }
        _ => serde_json::to_string(value)?,
    })
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl Release {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "bmz-release-manifest-v1" && self.client == "bmz-player",
            "unsupported release schema/client"
        );
        let current = version(&self.version)?;
        ensure!(current.to_string() == self.version, "noncanonical release version");
        ensure!(lower_hex(&self.git_commit, 40), "invalid release commit");
        ensure!(
            self.builds.len() == BUILDS.len() && self.artifacts.len() == ARTIFACTS.len(),
            "incomplete release inventory"
        );
        let mut build_ids = HashSet::new();
        for build in &self.builds {
            ensure!(build_ids.insert(&build.id), "duplicate build");
            ensure!(
                BUILDS.contains(&(
                    build.id.as_str(),
                    build.platform.as_str(),
                    build.arch.as_str(),
                    build.package_kind.as_str()
                )),
                "invalid build identity"
            );
            ensure!(lower_hex(&build.client_hash, 64), "invalid client hash");
        }
        let mut names = HashSet::new();
        for artifact in &self.artifacts {
            ensure!(names.insert(&artifact.name), "duplicate release artifact");
            ensure!(
                ARTIFACTS.iter().any(|(suffix, build, kind)| {
                    artifact.name == format!("bmz-player-v{}-{suffix}", self.version)
                        && artifact.build == *build
                        && artifact.kind == *kind
                }),
                "invalid release artifact identity"
            );
            ensure!(
                artifact.url
                    == format!(
                        "https://github.com/hyrorre/bmz-player/releases/download/v{}/{}",
                        self.version, artifact.name
                    ),
                "invalid release URL"
            );
            ensure!(
                artifact.size > 0
                    && artifact.size < 2_147_483_648
                    && lower_hex(&artifact.sha256, 64),
                "invalid artifact size/hash"
            );
            ensure!(
                artifact.update.is_some() == (artifact.build == "windows-x64"),
                "invalid update target"
            );
            if let Some(update) = &artifact.update {
                ensure!(update.min_updater_protocol > 0, "invalid updater protocol");
                ensure!(
                    update.min_updater_protocol == 1 || update.bridge_tag.is_some(),
                    "missing bridge release"
                );
                if let Some(bridge) = &update.bridge_tag {
                    ensure!(version(bridge)? < current, "bridge must precede release");
                }
            }
        }
        Ok(())
    }

    pub fn windows_updates(&self) -> ReleaseManifest {
        ReleaseManifest {
            schema: 1,
            packages: self
                .artifacts
                .iter()
                .filter_map(|artifact| {
                    let update = artifact.update.as_ref()?;
                    Some(ReleasePackage {
                        version: self.version.clone(),
                        target: artifact.build.clone(),
                        kind: if artifact.kind == "portable" {
                            PackageKind::Portable
                        } else {
                            PackageKind::Installer
                        },
                        min_updater_protocol: update.min_updater_protocol,
                        bridge_tag: update.bridge_tag.clone(),
                        name: artifact.name.clone(),
                        url: artifact.url.clone(),
                        size: artifact.size,
                        sha256: artifact.sha256.clone(),
                    })
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
