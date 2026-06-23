use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::app_config::UpdateChannelConfig;

const GITHUB_API_REPO: &str = "https://api.github.com/repos/hyrorre/bmz-player";
pub const RELEASES_PAGE_URL: &str = "https://github.com/hyrorre/bmz-player/releases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub version: String,
    pub tag: String,
    pub title: String,
    pub html_url: String,
    pub body: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub asset: Option<UpdateAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub kind: UpdateAssetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAssetKind {
    WindowsInstaller,
    MacosAppZip,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedUpdate {
    pub candidate: UpdateCandidate,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

pub async fn check_for_update(channel: UpdateChannelConfig) -> Result<Option<UpdateCandidate>> {
    let client = reqwest::Client::builder()
        .user_agent(format!("bmz-player/{}", current_version()))
        .build()?;
    let Some(release) = fetch_release_for_channel(&client, channel).await? else {
        return Ok(None);
    };
    if !is_newer_version(&release.tag_name, current_version()) {
        return Ok(None);
    }

    let version = release_version(&release.tag_name);
    let mut asset = select_asset_for_current_target(&release.assets, &version);
    if let Some(asset) = asset.as_mut()
        && asset.sha256.is_none()
    {
        asset.sha256 = fetch_sha256_from_release_sums(&client, &release.assets, &asset.name)
            .await
            .ok()
            .flatten();
    }

    let title = release.name.clone().unwrap_or_else(|| release.tag_name.clone());
    Ok(Some(UpdateCandidate {
        version,
        tag: release.tag_name,
        title,
        html_url: release.html_url,
        body: release.body.unwrap_or_default(),
        published_at: release.published_at,
        prerelease: release.prerelease,
        asset,
    }))
}

pub async fn download_update(
    candidate: UpdateCandidate,
    cache_dir: &Path,
) -> Result<DownloadedUpdate> {
    let asset = candidate
        .asset
        .clone()
        .ok_or_else(|| anyhow::anyhow!("この環境向けの更新ファイルがありません"))?;
    let expected_sha256 = asset
        .sha256
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("更新ファイルの SHA256 が見つかりません"))?;
    let client = reqwest::Client::builder()
        .user_agent(format!("bmz-player/{}", current_version()))
        .build()?;
    let bytes = client
        .get(&asset.download_url)
        .send()
        .await
        .context("failed to request update asset")?
        .error_for_status()
        .context("update asset request failed")?
        .bytes()
        .await
        .context("failed to download update asset")?;
    if asset.size > 0 && bytes.len() as u64 != asset.size {
        bail!("更新ファイルのサイズが一致しません: expected {}, got {}", asset.size, bytes.len());
    }

    let actual_sha256 = sha256_hex(&bytes);
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        bail!(
            "更新ファイルの SHA256 が一致しません: expected {expected_sha256}, got {actual_sha256}"
        );
    }

    let dir = cache_dir.join("updates").join(&candidate.version);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&asset.name);
    let tmp_path = path.with_extension("download");
    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(DownloadedUpdate { candidate, path })
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn is_newer_version(candidate: &str, current: &str) -> bool {
    match (VersionKey::parse(candidate), VersionKey::parse(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => candidate.trim_start_matches('v') > current.trim_start_matches('v'),
    }
}

fn release_version(tag: &str) -> String {
    tag.trim().trim_start_matches('v').to_string()
}

async fn fetch_release_for_channel(
    client: &reqwest::Client,
    channel: UpdateChannelConfig,
) -> Result<Option<GithubRelease>> {
    match channel {
        UpdateChannelConfig::Stable => {
            let url = format!("{GITHUB_API_REPO}/releases/latest");
            let response =
                client.get(url).send().await.context("failed to request latest release")?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            let release = response
                .error_for_status()
                .context("latest release request failed")?
                .json::<GithubRelease>()
                .await
                .context("failed to decode latest release")?;
            Ok((!release.draft).then_some(release))
        }
        UpdateChannelConfig::Prerelease => {
            let url = format!("{GITHUB_API_REPO}/releases?per_page=20");
            let releases = client
                .get(url)
                .send()
                .await
                .context("failed to request releases")?
                .error_for_status()
                .context("releases request failed")?
                .json::<Vec<GithubRelease>>()
                .await
                .context("failed to decode releases")?;
            Ok(releases.into_iter().find(|release| !release.draft))
        }
    }
}

fn select_asset_for_current_target(assets: &[GithubAsset], version: &str) -> Option<UpdateAsset> {
    select_asset_for_target(assets, version, target_platform(), target_arch())
}

fn select_asset_for_target(
    assets: &[GithubAsset],
    version: &str,
    platform: &str,
    arch: &str,
) -> Option<UpdateAsset> {
    let version_prefix = format!("bmz-player-v{version}-");
    let (suffix, kind) = match platform {
        "windows" => (format!("windows-{arch}-setup.exe"), UpdateAssetKind::WindowsInstaller),
        "macos" => (format!("macos-{arch}.app.zip"), UpdateAssetKind::MacosAppZip),
        _ => return None,
    };
    assets
        .iter()
        .find(|asset| asset.name.starts_with(&version_prefix) && asset.name.ends_with(&suffix))
        .map(|asset| UpdateAsset {
            name: asset.name.clone(),
            download_url: asset.browser_download_url.clone(),
            size: asset.size,
            sha256: asset.digest.as_deref().and_then(parse_sha256_digest),
            kind,
        })
}

async fn fetch_sha256_from_release_sums(
    client: &reqwest::Client,
    assets: &[GithubAsset],
    target_name: &str,
) -> Result<Option<String>> {
    let Some(sums_asset) = assets.iter().find(|asset| asset.name == "SHA256SUMS.txt") else {
        return Ok(None);
    };
    let text = client
        .get(&sums_asset.browser_download_url)
        .send()
        .await
        .context("failed to request SHA256SUMS.txt")?
        .error_for_status()
        .context("SHA256SUMS.txt request failed")?
        .text()
        .await
        .context("failed to download SHA256SUMS.txt")?;
    Ok(parse_sha256_sums(&text, target_name))
}

fn parse_sha256_digest(digest: &str) -> Option<String> {
    let value = digest.strip_prefix("sha256:")?;
    is_sha256_hex(value).then(|| value.to_ascii_lowercase())
}

fn parse_sha256_sums(text: &str, target_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == target_name && is_sha256_hex(hash)).then(|| hash.to_ascii_lowercase())
    })
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(target_os = "windows")]
fn target_platform() -> &'static str {
    "windows"
}

#[cfg(target_os = "macos")]
fn target_platform() -> &'static str {
    "macos"
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn target_platform() -> &'static str {
    "linux"
}

#[cfg(target_arch = "x86_64")]
fn target_arch() -> &'static str {
    "x64"
}

#[cfg(target_arch = "aarch64")]
fn target_arch() -> &'static str {
    "arm64"
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn target_arch() -> &'static str {
    "unknown"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct VersionKey {
    major: u64,
    minor: u64,
    patch: u64,
}

impl VersionKey {
    fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim().trim_start_matches('v');
        let core = trimmed.split(['-', '+']).next()?;
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        Some(Self { major, minor, patch })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str, digest: Option<&str>) -> GithubAsset {
        GithubAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.test/{name}"),
            size: 123,
            digest: digest.map(str::to_string),
        }
    }

    #[test]
    fn version_comparison_uses_numeric_segments() {
        assert!(is_newer_version("v0.10.0", "0.9.9"));
        assert!(is_newer_version("0.2.1", "0.2.0"));
        assert!(!is_newer_version("v0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.1.9", "0.2.0"));
    }

    #[test]
    fn sha256_sums_parser_matches_target_asset() {
        let text = "\
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  one.zip\n\
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb *two.zip\n";

        assert_eq!(
            parse_sha256_sums(text, "two.zip").as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
        assert_eq!(parse_sha256_sums(text, "missing.zip"), None);
    }

    #[test]
    fn target_asset_selection_prefers_platform_package() {
        let digest = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let assets = vec![
            asset("bmz-player-v0.2.0-windows-x64-portable.zip", None),
            asset("bmz-player-v0.2.0-windows-x64-setup.exe", Some(digest)),
            asset("SHA256SUMS.txt", None),
        ];

        let selected = select_asset_for_target(&assets, "0.2.0", "windows", "x64").unwrap();

        assert_eq!(selected.name, "bmz-player-v0.2.0-windows-x64-setup.exe");
        assert_eq!(selected.kind, UpdateAssetKind::WindowsInstaller);
        assert_eq!(
            selected.sha256.as_deref(),
            Some("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        );
    }
}
