use crate::config::app_config::UpdateChannelConfig;
use anyhow::{Context, Result, ensure};
use bmz_updater::manifest::{PackageKind, PackageManifest, ReleasePackage, safe_relative};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

pub mod sparkle;
const GITHUB_API_REPO: &str = "https://api.github.com/repos/hyrorre/bmz-player";
pub const RELEASES_PAGE_URL: &str = "https://github.com/hyrorre/bmz-player/releases";
const PUBLIC_KEY: Option<&str> = option_env!("BMZ_UPDATE_PUBLIC_KEY");

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
    WindowsPortable,
    MacosAppZip,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedUpdate {
    pub candidate: UpdateCandidate,
    pub path: PathBuf,
    pub work: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct DownloadProgress {
    pub received: AtomicU64,
    pub total: AtomicU64,
    pub extracting: AtomicBool,
    pub cancel: AtomicBool,
    pub paused: AtomicBool,
}

impl DownloadProgress {
    pub fn checkpoint(&self) -> Result<()> {
        ensure!(!self.cancel.load(Ordering::Relaxed), "update download canceled");
        Ok(())
    }
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

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("bmz-player/{}", current_version()))
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()?)
}

pub fn installed_package() -> Option<(PathBuf, PackageManifest)> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?.to_path_buf();
    let manifest = PackageManifest::read(&root).ok()?;
    (manifest.version == current_version()
        && manifest.target == format!("windows-{}", target_arch()))
    .then_some((root, manifest))
}

pub fn startup_guard() -> Result<Option<Vec<File>>> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()?;
        let root = exe.parent().context("missing executable directory")?;
        ensure!(
            bmz_updater::transaction::pending_update(root)?.is_none(),
            "前回の更新が中断されました。updater/bmz-updater.exe（旧配置では直下のbmz-updater.exe） --recover \"{}\" を実行してください。",
            root.display()
        );
        if PackageManifest::read_optional(root)?.is_some_and(|p| p.kind == PackageKind::Portable) {
            return Ok(Some(bmz_updater::process::instance_guard(root)?));
        }
    }
    Ok(None)
}

pub async fn check_for_update(channel: UpdateChannelConfig) -> Result<Option<UpdateCandidate>> {
    let client = client()?;
    let Some(mut release) = fetch_release_for_channel(&client, channel).await? else {
        return Ok(None);
    };
    if !is_newer_version(&release.tag_name, current_version()) {
        return Ok(None);
    }
    let mut selected = None;
    if let Some((_, installed)) = installed_package() {
        if let Some(key) = PUBLIC_KEY.filter(|key| !key.is_empty()) {
            for _ in 0..8 {
                let Some(metadata) = release.assets.iter().find(|a| a.name == "updates.json")
                else {
                    return Ok(None);
                };
                ensure!(metadata.size <= 1024 * 1024, "update manifest too large");
                let bytes =
                    bounded_bytes(&client, &metadata.browser_download_url, 1024 * 1024).await?;
                let signed = bmz_updater::manifest::verify_release(&bytes, key)?;
                let Some(target) = signed
                    .packages
                    .into_iter()
                    .find(|p| p.target == installed.target && p.kind == installed.kind)
                else {
                    return Ok(None);
                };
                ensure!(
                    target.version == release.tag_name.trim_start_matches('v'),
                    "release/manifest version mismatch"
                );
                if target.min_updater_protocol <= bmz_updater::PROTOCOL {
                    selected = Some(asset_from_signed(&target));
                    break;
                }
                let bridge = target.bridge_tag.context(
                    "この更新には新しいupdaterが必要です。リリースページから更新してください。",
                )?;
                ensure!(
                    is_newer_version(&bridge, current_version())
                        && is_newer_version(&release.tag_name, &bridge),
                    "invalid bridge release"
                );
                let tag = format!("v{}", bmz_updater::manifest::version(&bridge)?);
                release = client
                    .get(format!("{GITHUB_API_REPO}/releases/tags/{tag}"))
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?;
                ensure!(
                    !release.draft
                        && (channel == UpdateChannelConfig::Prerelease || !release.prerelease),
                    "bridge outside selected channel"
                );
            }
            ensure!(selected.is_some(), "too many bridge releases");
        } else if installed.kind == PackageKind::Installer {
            selected = select_asset(
                &release.assets,
                &release.tag_name,
                "windows",
                target_arch(),
                Some(PackageKind::Installer),
            );
        }
    } else if cfg!(target_os = "macos") {
        selected = select_asset(&release.assets, &release.tag_name, "macos", target_arch(), None);
    }
    if let Some(asset) = selected.as_mut()
        && asset.sha256.is_none()
    {
        asset.sha256 = fetch_sha256(&client, &release.assets, &asset.name).await?;
    }
    Ok(Some(candidate(release, selected)))
}

fn candidate(release: GithubRelease, asset: Option<UpdateAsset>) -> UpdateCandidate {
    UpdateCandidate {
        version: release.tag_name.trim_start_matches('v').to_owned(),
        title: release.name.unwrap_or_else(|| release.tag_name.clone()),
        tag: release.tag_name,
        html_url: release.html_url,
        body: release.body.unwrap_or_default(),
        published_at: release.published_at,
        prerelease: release.prerelease,
        asset,
    }
}

fn asset_from_signed(package: &ReleasePackage) -> UpdateAsset {
    UpdateAsset {
        name: package.name.clone(),
        download_url: package.url.clone(),
        size: package.size,
        sha256: Some(package.sha256.clone()),
        kind: match package.kind {
            PackageKind::Portable => UpdateAssetKind::WindowsPortable,
            PackageKind::Installer => UpdateAssetKind::WindowsInstaller,
        },
    }
}

async fn bounded_bytes(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(bytes.len().saturating_add(chunk.len()) <= limit, "response exceeds size limit");
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn download_update(
    candidate: UpdateCandidate,
    cache_dir: &Path,
    progress: Arc<DownloadProgress>,
) -> Result<DownloadedUpdate> {
    let asset = candidate.asset.as_ref().context("この環境向けの更新ファイルがありません")?;
    let expected = asset.sha256.as_deref().context("更新ファイルの SHA256 が見つかりません")?;
    ensure!(bmz_updater::manifest::valid_hash(expected), "invalid update hash");
    ensure!(asset.size > 0 && asset.size <= 8 * 1024 * 1024 * 1024, "invalid update size");
    safe_relative(&asset.name)?;
    ensure!(!asset.name.contains('/'), "invalid asset filename");
    let version = bmz_updater::manifest::version(&candidate.version)?.to_string();
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|error| anyhow::anyhow!("update nonce: {error}"))?;
    let dir = cache_dir
        .join("updates")
        .join(version)
        .join(format!("{:032x}", u128::from_ne_bytes(nonce)));
    std::fs::create_dir_all(&dir)?;
    ensure!(
        bmz_updater::process::available_space(&dir)? >= asset.size + 64 * 1024 * 1024,
        "insufficient disk space"
    );
    let path = bmz_updater::manifest::checked_path(&dir, &asset.name)?;
    let partial = path.with_extension("download");
    bmz_updater::manifest::reject_link(&partial)?;
    let result = async {
        let mut output = File::create(&partial)?;
        let mut response = client()?.get(&asset.download_url).send().await?.error_for_status()?;
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        progress.total.store(asset.size, Ordering::Relaxed);
        while let Some(chunk) = response.chunk().await? {
            progress.checkpoint()?;
            received += chunk.len() as u64;
            ensure!(received <= asset.size, "update exceeds expected size");
            output.write_all(&chunk)?;
            hasher.update(&chunk);
            progress.received.store(received, Ordering::Relaxed);
        }
        ensure!(received == asset.size, "update size mismatch");
        ensure!(
            format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected),
            "update SHA256 mismatch"
        );
        output.sync_all()?;
        drop(output);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        std::fs::rename(&partial, &path)?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result?;
    progress.checkpoint()?;
    let work = if asset.kind == UpdateAssetKind::WindowsPortable {
        progress.extracting.store(true, Ordering::Relaxed);
        let (root, installed) =
            installed_package().context("portable installation metadata missing")?;
        ensure!(installed.kind == PackageKind::Portable, "not a portable installation");
        let work = bmz_updater::transaction::new_work_dir(&root)?;
        let new =
            bmz_updater::archive::extract(&path, &work.join("stage"), || progress.checkpoint())?;
        ensure!(
            new.version == candidate.version && new.target == installed.target,
            "downloaded package identity mismatch"
        );
        bmz_updater::transaction::preflight(&root, &work)?;
        Some(work)
    } else {
        None
    };
    Ok(DownloadedUpdate { candidate, path, work })
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
pub fn is_newer_version(candidate: &str, current: &str) -> bool {
    match (bmz_updater::manifest::version(candidate), bmz_updater::manifest::version(current)) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => false,
    }
}

async fn fetch_release_for_channel(
    client: &reqwest::Client,
    channel: UpdateChannelConfig,
) -> Result<Option<GithubRelease>> {
    let releases: Vec<GithubRelease> = client
        .get(format!("{GITHUB_API_REPO}/releases?per_page=100"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(releases
        .into_iter()
        .filter(|r| !r.draft && (channel == UpdateChannelConfig::Prerelease || !r.prerelease))
        .filter(|r| {
            bmz_updater::manifest::version(&r.tag_name)
                .is_ok_and(|v| channel == UpdateChannelConfig::Prerelease || v.pre.is_empty())
        })
        .max_by_key(|r| bmz_updater::manifest::version(&r.tag_name).ok()))
}

fn select_asset(
    assets: &[GithubAsset],
    version: &str,
    platform: &str,
    arch: &str,
    kind: Option<PackageKind>,
) -> Option<UpdateAsset> {
    let (suffix, kind) = match (platform, kind) {
        ("windows", Some(PackageKind::Installer)) => {
            (format!("windows-{arch}-setup.exe"), UpdateAssetKind::WindowsInstaller)
        }
        ("windows", Some(PackageKind::Portable)) => {
            (format!("windows-{arch}-portable.zip"), UpdateAssetKind::WindowsPortable)
        }
        ("macos", _) => (format!("macos-{arch}.app.zip"), UpdateAssetKind::MacosAppZip),
        _ => return None,
    };
    let name = format!("bmz-player-v{}-{suffix}", version.trim_start_matches('v'));
    assets.iter().find(|a| a.name == name).map(|a| UpdateAsset {
        name: a.name.clone(),
        download_url: a.browser_download_url.clone(),
        size: a.size,
        sha256: a
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .filter(|d| bmz_updater::manifest::valid_hash(d))
            .map(str::to_owned),
        kind,
    })
}

async fn fetch_sha256(
    client: &reqwest::Client,
    assets: &[GithubAsset],
    target: &str,
) -> Result<Option<String>> {
    let Some(asset) = assets.iter().find(|a| a.name == "SHA256SUMS.txt") else {
        return Ok(None);
    };
    let bytes = bounded_bytes(client, &asset.browser_download_url, 1024 * 1024).await?;
    Ok(parse_sha256_sums(std::str::from_utf8(&bytes)?, target))
}

fn parse_sha256_sums(text: &str, target: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        (parts.next()?.trim_start_matches('*') == target && bmz_updater::manifest::valid_hash(hash))
            .then(|| hash.to_ascii_lowercase())
    })
}

pub fn target_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_respect_prereleases_and_reject_invalid_versions() {
        assert!(is_newer_version("v0.10.0", "0.9.9"));
        assert!(is_newer_version("0.4.0", "0.4.0-rc.2"));
        assert!(is_newer_version("0.4.0-rc.10", "0.4.0-rc.2"));
        assert!(!is_newer_version("0.4.0-rc.1", "0.4.0"));
        assert!(!is_newer_version("../../bad", "0.4.0"));
    }
    #[test]
    fn package_kind_is_explicit_and_never_switches_portable_to_installer() {
        let assets: Vec<_> = ["portable.zip", "setup.exe"]
            .into_iter()
            .map(|suffix| GithubAsset {
                name: format!("bmz-player-v0.5.0-windows-x64-{suffix}"),
                browser_download_url: String::new(),
                size: 1,
                digest: None,
            })
            .collect();
        assert_eq!(
            select_asset(&assets, "0.5.0", "windows", "x64", Some(PackageKind::Portable))
                .unwrap()
                .kind,
            UpdateAssetKind::WindowsPortable
        );
        assert_eq!(
            select_asset(&assets, "0.5.0", "windows", "x64", Some(PackageKind::Installer))
                .unwrap()
                .kind,
            UpdateAssetKind::WindowsInstaller
        );
        assert!(select_asset(&assets, "0.5.0", "windows", "x64", None).is_none());
        assert!(
            select_asset(&assets[1..], "0.5.0", "windows", "x64", Some(PackageKind::Portable))
                .is_none()
        );
    }
    #[test]
    fn sums_match_exact_asset() {
        let hash = "ab".repeat(32);
        assert_eq!(parse_sha256_sums(&format!("{hash} *two.zip\n"), "two.zip"), Some(hash));
        assert!(parse_sha256_sums("bad two.zip", "two.zip").is_none());
    }
}
