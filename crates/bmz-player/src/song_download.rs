use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::{Response, Url};
use serde_json::Value;
use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

mod bulk;
mod http_response;
mod manager;
pub use bulk::{
    BulkDownloadBatch, BulkDownloadCounts, BulkDownloadPlan, BulkDownloadPreview,
    BulkDownloadScope, prepare_bulk_download,
};
#[cfg(test)]
mod test_support;
pub use manager::{
    ChartDownloadManager, DownloadControl, DownloadServices, DownloadState, DownloadTask,
    DownloadTaskId,
};

use crate::config::app_config::ChartDownloadsConfig;

const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_EXTRACTED_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_EXTRACTED_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChartDownloadMetadata {
    pub md5: String,
    pub sha256: String,
    pub url: String,
    pub append_url: String,
    pub ipfs: String,
    pub append_ipfs: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartDownloadSource {
    Ipfs,
    Http,
}

impl ChartDownloadSource {
    pub fn directory_name(self) -> &'static str {
        match self {
            Self::Ipfs => "ipfs",
            Self::Http => "http",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Ipfs => "IPFS",
            Self::Http => "HTTP",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingChartAction {
    Ipfs { api_url: String, cids: Vec<String> },
    Http { api_url: String, md5: String, sha256: String },
    Browser(Vec<String>),
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct ChartDownloadRequest {
    pub request_id: u64,
    pub identity: ChartDownloadIdentity,
    pub action: MissingChartAction,
    pub title: String,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ChartDownloadResult {
    pub request_id: u64,
    pub identity: ChartDownloadIdentity,
    pub source: ChartDownloadSource,
    pub root_dir: PathBuf,
    pub chart_dir: PathBuf,
}

/// File hashes, independent of the table row and the local chart registration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ChartDownloadIdentity {
    pub sha256: Option<[u8; 32]>,
    pub md5: Option<[u8; 16]>,
}

impl ChartDownloadIdentity {
    pub fn overlaps(&self, other: &Self) -> bool {
        match (self.sha256, other.sha256) {
            (Some(a), Some(b)) => {
                a == b
                    && match (self.md5, other.md5) {
                        (Some(a), Some(b)) => a == b,
                        _ => true,
                    }
            }
            _ => self.md5.is_some() && self.md5 == other.md5,
        }
    }
    pub fn from_metadata(metadata: &ChartDownloadMetadata) -> Self {
        Self {
            sha256: crate::storage::common::hex_to_hash(metadata.sha256.trim()).ok(),
            md5: crate::storage::common::hex_to_hash(metadata.md5.trim()).ok(),
        }
    }

    pub fn matches(&self, chart: &crate::storage::library_db::ChartListItem) -> bool {
        if let Some(sha256) = self.sha256 {
            chart.sha256 == sha256 && self.md5.is_none_or(|md5| chart.md5 == md5)
        } else {
            self.md5.is_some_and(|md5| chart.md5 == md5)
        }
    }
}

pub fn registered_download_identity(
    db: &crate::storage::library_db::LibraryDatabase,
    identity: &ChartDownloadIdentity,
    chart_dir: &Path,
) -> Result<bool> {
    let mut folders = db.list_descendant_folder_paths(&chart_dir.to_string_lossy())?;
    folders.push(chart_dir.to_string_lossy().into_owned());
    let folder_refs: Vec<_> = folders.iter().map(String::as_str).collect();
    Ok(db.list_charts_in_folders(&folder_refs)?.iter().any(|chart| identity.matches(chart)))
}

#[derive(Debug)]
pub struct ChartDownloadFailure {
    pub title: String,
    pub error: anyhow::Error,
}

#[derive(Debug, Default)]
pub struct ChartDownloadBatchResult {
    pub completed: Vec<ChartDownloadResult>,
    pub failures: Vec<ChartDownloadFailure>,
}

pub fn choose_missing_chart_action(
    config: &ChartDownloadsConfig,
    metadata: &ChartDownloadMetadata,
) -> MissingChartAction {
    let cids = [&metadata.ipfs, &metadata.append_ipfs]
        .into_iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if config.ipfs_enabled && !config.ipfs_api_url.trim().is_empty() && !cids.is_empty() {
        return MissingChartAction::Ipfs { api_url: config.ipfs_api_url.trim().to_string(), cids };
    }

    let identity = ChartDownloadIdentity::from_metadata(metadata);
    let http_api_url = config.effective_http_api_url();
    if config.http_enabled
        && crate::config::download_sources::template_supports_hashes(
            http_api_url,
            identity.md5.is_some(),
            identity.sha256.is_some(),
        )
    {
        return MissingChartAction::Http {
            api_url: http_api_url.to_string(),
            md5: identity
                .md5
                .map(|hash| crate::storage::common::hash_to_hex(&hash))
                .unwrap_or_default(),
            sha256: identity
                .sha256
                .map(|hash| crate::storage::common::hash_to_hex(&hash))
                .unwrap_or_default(),
        };
    }

    let urls = validated_browser_urls([&metadata.url, &metadata.append_url]);
    if urls.is_empty() {
        MissingChartAction::Unavailable
    } else {
        MissingChartAction::Browser(urls)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_browser_url(url: &str) -> Result<()> {
    // xdg-open via desktop_command keeps the tar launcher's PipeWire paths out of the browser.
    crate::external_process::desktop_command("xdg-open").arg(url).spawn()?;
    Ok(())
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn open_browser_url(url: &str) -> Result<()> {
    Ok(webbrowser::open(url)?)
}

pub fn open_browser_urls(urls: &[String]) -> Result<usize> {
    let urls = validated_browser_urls(urls.iter());
    for url in &urls {
        open_browser_url(url).with_context(|| format!("ブラウザでURLを開けませんでした: {url}"))?;
    }
    Ok(urls.len())
}

pub fn validated_browser_urls<'a>(urls: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut seen = HashSet::new();
    urls.into_iter()
        .filter_map(|value| {
            let value = value.trim();
            let parsed = Url::parse(value).ok()?;
            if !matches!(parsed.scheme(), "http" | "https") || !seen.insert(value.to_string()) {
                return None;
            }
            Some(value.to_string())
        })
        .collect()
}

#[cfg(test)]
mod public_url_tests {
    use super::*;

    #[test]
    fn browser_urls_reject_non_http_and_deduplicate() {
        let https = "https://example.test/song".to_string();
        let duplicate = https.clone();
        let file = "file:///tmp/song".to_string();
        assert_eq!(validated_browser_urls([&https, &duplicate, &file]), vec![https]);
    }
}

pub async fn download_chart(request: ChartDownloadRequest) -> Result<ChartDownloadResult> {
    download_chart_controlled(request, Arc::default(), Arc::default()).await
}

async fn download_chart_controlled(
    request: ChartDownloadRequest,
    control: Arc<DownloadControl>,
    services: Arc<DownloadServices>,
) -> Result<ChartDownloadResult> {
    control.check()?;
    tokio::select! {
        biased;
        _ = control.cancelled() => bail!("download cancelled"),
        result = download_chart_inner(request, &control, &services) => result,
    }
}

async fn download_chart_inner(
    request: ChartDownloadRequest,
    control: &DownloadControl,
    services: &DownloadServices,
) -> Result<ChartDownloadResult> {
    control.set_state(DownloadState::Resolving);
    let source = match &request.action {
        MissingChartAction::Ipfs { .. } => ChartDownloadSource::Ipfs,
        MissingChartAction::Http { .. } => ChartDownloadSource::Http,
        MissingChartAction::Browser(_) | MissingChartAction::Unavailable => {
            bail!("ダウンロード以外の取得方法が指定されました")
        }
    };
    let root_dir = request.data_dir.join("songs").join(source.directory_name());
    std::fs::create_dir_all(&root_dir)
        .with_context(|| format!("保存先を作成できませんでした: {}", root_dir.display()))?;

    let stamp = unique_stamp();
    let archive_path = root_dir.join(format!(".bmz-{stamp}.download"));
    let staging_dir = root_dir.join(format!(".bmz-{stamp}.extract"));
    std::fs::create_dir(&staging_dir).with_context(|| {
        format!("展開用フォルダを作成できませんでした: {}", staging_dir.display())
    })?;
    let mut cleanup = DownloadCleanup::new(archive_path.clone(), staging_dir.clone());

    let client = reqwest::Client::builder()
        .user_agent(concat!("bmz-player/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(30 * 60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let mut cache_owner = None;
    let hash_hint = match &request.action {
        MissingChartAction::Ipfs { api_url, cids } => {
            anyhow::ensure!(!cids.is_empty(), "IPFS action requires at least one CID");
            for (index, cid) in cids.iter().enumerate() {
                let response = request_ipfs_archive(&client, api_url, cid).await?;
                save_response_controlled(response, &archive_path, control).await?;
                let package_dir = staging_dir.join(format!(".package-{index}"));
                std::fs::create_dir(&package_dir)?;
                {
                    let _permit = services.extraction.lock().unwrap_or_else(|e| e.into_inner());
                    control.check()?;
                    control.set_state(DownloadState::Extracting);
                    extract_archive_controlled(&archive_path, &package_dir, control)?;
                }
                merge_ipfs_package(&package_dir, &staging_dir)?;
                std::fs::remove_file(&archive_path).ok();
                if index + 1 < cids.len() {
                    tracing::debug!("extracting additional IPFS package");
                }
            }
            normalize_ipfs_arg(cids.first().expect("IPFS action has at least one CID"))?
        }
        MissingChartAction::Http { api_url, md5, sha256 } => {
            let scope = format!("{}\n{}", request.data_dir.display(), api_url);
            let api_url = expand_http_api_url(api_url, md5, sha256)?;
            let response = client.get(api_url.clone()).send().await?.error_for_status()?;
            let metadata_base = response.url().clone();
            let (archive_url, direct_response) = match http_response::inspect(response).await? {
                http_response::HttpPayload::Metadata(value) => {
                    let download_url = http_response::metadata_download_url(&value)?;
                    (resolve_http_url(&metadata_base, download_url)?, None)
                }
                http_response::HttpPayload::Archive(response, prefix) => {
                    (response.url().clone(), Some((response, prefix)))
                }
            };
            let key = format!("{scope}\n{archive_url}");
            // The first owner publishes an extracted directory. Followers keep
            // their own hashes and verify each one after the shared partial scan.
            let mut direct_response = direct_response;
            loop {
                control.check()?;
                let (shared, owner) = {
                    let mut cache = services.archives.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(shared) = cache.get(&key) {
                        (Arc::clone(shared), false)
                    } else {
                        let shared = Arc::new(std::sync::Mutex::new(None));
                        cache.insert(key.clone(), Arc::clone(&shared));
                        (shared, true)
                    }
                };
                if owner {
                    cache_owner =
                        Some(ArchiveOwner { services, key: key.clone(), shared, completed: false });
                    break;
                }
                // A cancelled owner releases its claim; a waiting task can take
                // over without inheriting the other task's cancellation.
                let cached = shared.lock().unwrap_or_else(|e| e.into_inner()).clone();
                match cached {
                    Some(Ok(mut result)) if result.chart_dir.is_dir() => {
                        result.request_id = request.request_id;
                        result.identity = request.identity;
                        return Ok(result);
                    }
                    Some(_) => {
                        let mut cache = services.archives.lock().unwrap_or_else(|e| e.into_inner());
                        if cache.get(&key).is_some_and(|current| Arc::ptr_eq(current, &shared)) {
                            cache.remove(&key);
                        }
                    }
                    None => {
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                }
            }
            let archive_response = match direct_response.take() {
                Some(response) => response,
                None => (client.get(archive_url).send().await?.error_for_status()?, Vec::new()),
            };
            anyhow::ensure!(
                !response_is_json(&archive_response.0),
                "archive endpoint returned JSON"
            );
            save_response_with_prefix(
                archive_response.0,
                archive_response.1,
                &archive_path,
                control,
            )
            .await?;
            {
                let _permit = services.extraction.lock().unwrap_or_else(|e| e.into_inner());
                control.check()?;
                control.set_state(DownloadState::Extracting);
                extract_archive_controlled(&archive_path, &staging_dir, control)?;
            }
            if md5.is_empty() { sha256.clone() } else { md5.clone() }
        }
        MissingChartAction::Browser(_) | MissingChartAction::Unavailable => unreachable!(),
    };

    control.check()?;
    let final_dir = unique_final_dir(&root_dir, &request.title, &hash_hint);
    std::fs::rename(&staging_dir, &final_dir).with_context(|| {
        format!("展開済みフォルダを保存できませんでした: {}", final_dir.display())
    })?;
    std::fs::remove_file(&archive_path).ok();
    cleanup.disarm();

    let result = ChartDownloadResult {
        request_id: request.request_id,
        identity: request.identity,
        source,
        root_dir,
        chart_dir: final_dir,
    };
    if let Some(owner) = &mut cache_owner {
        *owner.shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(Ok(result.clone()));
        owner.completed = true;
    }
    Ok(result)
}

struct ArchiveOwner<'a> {
    services: &'a DownloadServices,
    key: String,
    shared: Arc<std::sync::Mutex<manager::ArchiveResult>>,
    completed: bool,
}

impl Drop for ArchiveOwner<'_> {
    fn drop(&mut self) {
        if !self.completed {
            *self.shared.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(Err("archive owner ended".into()));
            self.services.archives.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.key);
        }
    }
}

/// Downloads every request sequentially so one failed course stage does not
/// cancel acquisition of the remaining stages.
pub async fn download_charts(requests: Vec<ChartDownloadRequest>) -> ChartDownloadBatchResult {
    let mut result = ChartDownloadBatchResult::default();
    for request in requests {
        let title = request.title.clone();
        match download_chart(request).await {
            Ok(completed) => result.completed.push(completed),
            Err(error) => result.failures.push(ChartDownloadFailure { title, error }),
        }
    }
    result
}

async fn request_ipfs_archive(
    client: &reqwest::Client,
    api_url: &str,
    cid: &str,
) -> Result<reqwest::Response> {
    let url = ipfs_download_url(api_url, cid)?;
    // Keep beatoraja-compatible GET endpoints and custom templates unchanged.
    // Only the generated RPC endpoint may retry a method rejection as POST.
    let response = client.get(url.clone()).send().await?;
    let response = if response.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED
        && !api_url.contains("{cid}")
    {
        drop(response);
        client.post(url).send().await?
    } else {
        response
    };
    Ok(response.error_for_status()?)
}

fn ipfs_download_url(api_url: &str, cid: &str) -> Result<Url> {
    let cid = normalize_ipfs_arg(cid)?;
    if api_url.contains("{cid}") {
        let url = Url::parse(&api_url.replace("{cid}", &cid))?;
        validate_http_scheme(&url)?;
        return Ok(url);
    }

    let mut url = Url::parse(api_url.trim()).context("IPFS API URLが不正です")?;
    validate_http_scheme(&url)?;
    let path = url.path().trim_end_matches('/');
    if !path.ends_with("/api/v0/get") {
        let new_path =
            if path.is_empty() { "/api/v0/get".to_string() } else { format!("{path}/api/v0/get") };
        url.set_path(&new_path);
    }
    url.query_pairs_mut()
        .append_pair("arg", &cid)
        .append_pair("archive", "true")
        .append_pair("compress", "true");
    Ok(url)
}

fn normalize_ipfs_arg(value: &str) -> Result<String> {
    let value = value.trim();
    let value = value.strip_prefix("ipfs://").unwrap_or(value);
    let value = value.strip_prefix("/ipfs/").unwrap_or(value);
    let cid = value.split('/').next().unwrap_or_default();
    if !(20..=128).contains(&cid.len()) || !cid.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        bail!("難易度表のIPFS CIDが不正です")
    }
    if value.split('/').any(|part| part == "." || part == "..")
        || value.bytes().any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        bail!("難易度表のIPFSパスが不正です")
    }
    Ok(value.to_string())
}

fn expand_http_api_url(template: &str, md5: &str, sha256: &str) -> Result<Url> {
    anyhow::ensure!(
        crate::config::download_sources::template_supports_hashes(
            template,
            !md5.is_empty(),
            !sha256.is_empty()
        ),
        "HTTP API template requires an unavailable hash"
    );
    let mut expanded = template.trim().to_string();
    let has_placeholder =
        expanded.contains("{md5}") || expanded.contains("{sha256}") || expanded.contains("%s");
    if !has_placeholder {
        bail!("HTTP API URLには {md5}、{sha256}、または %s を含めてください")
    }
    expanded = expanded.replace("{md5}", md5).replace("{sha256}", sha256).replace("%s", md5);
    let url = Url::parse(&expanded).context("HTTP API URLが不正です")?;
    validate_http_scheme(&url)?;
    Ok(url)
}

fn validate_http_scheme(url: &Url) -> Result<()> {
    if matches!(url.scheme(), "http" | "https") {
        Ok(())
    } else {
        bail!("HTTP/HTTPS以外のURLは利用できません")
    }
}

fn response_is_json(response: &Response) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("json"))
}

#[cfg(test)]
fn find_download_url(value: &Value) -> Option<&str> {
    http_response::metadata_download_url(value).ok()
}

fn resolve_http_url(base: &Url, value: &str) -> Result<Url> {
    let url = base.join(value.trim()).context("HTTP APIのダウンロードURLが不正です")?;
    validate_http_scheme(&url)?;
    Ok(url)
}

async fn save_response_controlled(
    response: Response,
    path: &Path,
    control: &DownloadControl,
) -> Result<()> {
    save_response_with_prefix(response, Vec::new(), path, control).await
}

async fn save_response_with_prefix(
    mut response: Response,
    prefix: Vec<u8>,
    path: &Path,
    control: &DownloadControl,
) -> Result<()> {
    control.set_state(DownloadState::Downloading);
    let content_length = response.content_length();
    control.progress(0, content_length);
    if response.content_length().is_some_and(|length| length > MAX_DOWNLOAD_BYTES) {
        bail!("ダウンロードサイズが上限を超えています")
    }
    let mut file = File::create(path)?;
    let mut total = 0_u64;
    let mut prefix = Some(prefix);
    let mut signature = Vec::new();
    let mut verified = false;
    loop {
        let chunk = if let Some(prefix) = prefix.take() {
            prefix
        } else if let Some(chunk) = response.chunk().await? {
            chunk.to_vec()
        } else {
            break;
        };
        control.check()?;
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_DOWNLOAD_BYTES {
            bail!("ダウンロードサイズが上限を超えています")
        }
        if !verified {
            signature.extend_from_slice(&chunk);
            if signature.len() < 6 {
                continue;
            }
            anyhow::ensure!(
                signature.starts_with(&[0x1f, 0x8b])
                    || signature.starts_with(&[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]),
                "unsupported archive signature (expected tar.gz or 7z)"
            );
            file.write_all(&signature)?;
            signature.clear();
            verified = true;
        } else {
            file.write_all(&chunk)?;
        }
        control.progress(total, content_length);
    }
    file.flush()?;
    if total == 0 {
        bail!("ダウンロードしたアーカイブが空です")
    }
    anyhow::ensure!(verified, "archive header is incomplete");
    Ok(())
}

fn extract_archive_controlled(
    archive_path: &Path,
    destination: &Path,
    control: &DownloadControl,
) -> Result<()> {
    let mut signature = [0_u8; 8];
    let read = File::open(archive_path)?.read(&mut signature)?;
    if read >= 2 && signature[..2] == [0x1f, 0x8b] {
        extract_tar_gz_controlled(archive_path, destination, control)
    } else if read >= 6 && signature[..6] == [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c] {
        extract_7z(archive_path, destination, control)
    } else {
        bail!("対応していないアーカイブ形式です（tar.gz / 7z のみ対応）")
    }
}

#[cfg(test)]
fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<()> {
    extract_tar_gz_controlled(archive_path, destination, &DownloadControl::default())
}

fn extract_tar_gz_controlled(
    archive_path: &Path,
    destination: &Path,
    control: &DownloadControl,
) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(CancelReader { reader: file, control });
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    let mut count = 0_usize;
    let mut total = 0_u64;
    for entry in archive.entries()? {
        control.check()?;
        let mut entry = entry?;
        validate_archive_path(&entry.path()?.to_string_lossy())?;
        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            bail!("アーカイブ内のファイル数が上限を超えています")
        }
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            bail!("リンクなどの非対応エントリを含むアーカイブです")
        }
        if entry_type.is_file() {
            let size = entry.size();
            if size > MAX_EXTRACTED_FILE_BYTES {
                bail!("アーカイブ内の単一ファイルが大きすぎます")
            }
            total = total.saturating_add(size);
            if total > MAX_EXTRACTED_BYTES {
                bail!("アーカイブの展開サイズが上限を超えています")
            }
        }
        if !entry.unpack_in(destination)? {
            bail!("保存先の外へ出るアーカイブパスを拒否しました")
        }
    }
    Ok(())
}

fn extract_7z(archive_path: &Path, destination: &Path, control: &DownloadControl) -> Result<()> {
    let mut count = 0_usize;
    let mut total = 0_u64;
    sevenz_rust2::decompress_file_with_extract_fn(
        archive_path,
        destination,
        |entry, reader, path| {
            let unix_type = (entry.windows_attributes >> 16) & 0o170000;
            if entry.is_anti_item
                || (entry.windows_attributes & 0x400) != 0
                || !matches!(unix_type, 0 | 0o100000 | 0o040000)
            {
                return Err(sevenz_rust2::Error::Other("unsupported archive entry".into()));
            }
            if control.is_cancelled() {
                return Err(sevenz_rust2::Error::Other("download cancelled".into()));
            }
            validate_archive_path(entry.name())
                .map_err(|e| sevenz_rust2::Error::Other(e.to_string().into()))?;
            count += 1;
            if count > MAX_ARCHIVE_ENTRIES {
                return Err(sevenz_rust2::Error::Other(std::borrow::Cow::Borrowed(
                    "archive entry count exceeds the limit",
                )));
            }
            if !entry.is_directory {
                if entry.size > MAX_EXTRACTED_FILE_BYTES {
                    return Err(sevenz_rust2::Error::Other(std::borrow::Cow::Borrowed(
                        "archive entry exceeds the size limit",
                    )));
                }
                total = total.saturating_add(entry.size);
                if total > MAX_EXTRACTED_BYTES {
                    return Err(sevenz_rust2::Error::Other(std::borrow::Cow::Borrowed(
                        "archive extracted size exceeds the limit",
                    )));
                }
            }
            sevenz_rust2::default_entry_extract_fn(
                entry,
                &mut CancelReader { reader, control },
                path,
            )
        },
    )
    .context("7zアーカイブを展開できませんでした")
}

struct CancelReader<'a, R> {
    reader: R,
    control: &'a DownloadControl,
}
impl<R: Read> Read for CancelReader<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.control.is_cancelled() {
            return Err(std::io::Error::other("download cancelled"));
        }
        self.reader.read(bytes)
    }
}

fn validate_archive_path(name: &str) -> Result<()> {
    anyhow::ensure!(
        !name.starts_with(['/', '\\'])
            && !name.contains(':')
            && !name.contains('\0')
            && !name.split(['/', '\\']).any(|part| part == ".."),
        "unsafe archive entry path"
    );
    Ok(())
}

fn merge_ipfs_package(package_dir: &Path, destination: &Path) -> Result<()> {
    let entries = std::fs::read_dir(package_dir)?.collect::<std::io::Result<Vec<_>>>()?;
    let source = if entries.len() == 1 && entries[0].file_type()?.is_dir() {
        entries[0].path()
    } else {
        package_dir.to_path_buf()
    };
    merge_directory_contents(&source, destination)?;
    if package_dir.exists() {
        std::fs::remove_dir_all(package_dir)?;
    }
    Ok(())
}

fn merge_directory_contents(source: &Path, destination: &Path) -> Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            std::fs::create_dir_all(&to)?;
            merge_directory_contents(&from, &to)?;
            std::fs::remove_dir(&from)?;
        } else {
            if to.exists() {
                std::fs::remove_file(&to)?;
            }
            std::fs::rename(&from, &to)?;
        }
    }
    Ok(())
}

fn unique_final_dir(root: &Path, title: &str, hash_hint: &str) -> PathBuf {
    let title = sanitize_name(title);
    let hash =
        hash_hint.chars().filter(|ch| ch.is_ascii_alphanumeric()).take(12).collect::<String>();
    let stem = if hash.is_empty() { title } else { format!("{title}-{hash}") };
    root.join(format!("{stem}-{}", unique_stamp()))
}

fn sanitize_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim().trim_matches('.').trim().chars().take(80).collect::<String>();
    if sanitized.is_empty() { "download".to_string() } else { sanitized }
}

fn unique_stamp() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

struct DownloadCleanup {
    archive_path: PathBuf,
    staging_dir: PathBuf,
    armed: bool,
}

impl DownloadCleanup {
    fn new(archive_path: PathBuf, staging_dir: PathBuf) -> Self {
        Self { archive_path, staging_dir, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for DownloadCleanup {
    fn drop(&mut self) {
        if self.armed {
            std::fs::remove_file(&self.archive_path).ok();
            std::fs::remove_dir_all(&self.staging_dir).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> ChartDownloadMetadata {
        ChartDownloadMetadata {
            md5: "00112233445566778899aabbccddeeff".to_string(),
            sha256: "11".repeat(32),
            url: "https://example.com/song".to_string(),
            append_url: "https://example.com/diff".to_string(),
            ipfs: "/ipfs/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3ktekzrxql4i5f3u".to_string(),
            append_ipfs: String::new(),
        }
    }

    #[test]
    fn disabled_downloads_fall_back_to_browser_urls() {
        let action = choose_missing_chart_action(&ChartDownloadsConfig::default(), &metadata());

        assert_eq!(
            action,
            MissingChartAction::Browser(vec![
                "https://example.com/song".to_string(),
                "https://example.com/diff".to_string()
            ])
        );
    }

    #[test]
    fn ipfs_has_priority_over_http_and_browser() {
        let config = ChartDownloadsConfig {
            http_source: Default::default(),
            http_parallelism: 3,
            ipfs_enabled: true,
            ipfs_api_url: "http://127.0.0.1:5001".to_string(),
            http_enabled: true,
            http_api_url: "https://example.com/{md5}".to_string(),
        };

        assert!(matches!(
            choose_missing_chart_action(&config, &metadata()),
            MissingChartAction::Ipfs { .. }
        ));
    }

    #[test]
    fn http_is_used_when_ipfs_has_no_cid() {
        let config = ChartDownloadsConfig {
            http_source: Default::default(),
            http_parallelism: 3,
            ipfs_enabled: true,
            ipfs_api_url: "http://127.0.0.1:5001".to_string(),
            http_enabled: true,
            http_api_url: "https://example.com/{md5}".to_string(),
        };
        let mut metadata = metadata();
        metadata.ipfs.clear();

        assert!(matches!(
            choose_missing_chart_action(&config, &metadata),
            MissingChartAction::Http { .. }
        ));
    }

    #[test]
    fn http_template_supports_all_placeholders() {
        let url =
            expand_http_api_url("https://example.com/%s?md5={md5}&sha={sha256}", "aabb", "ccdd")
                .unwrap();

        assert_eq!(url.as_str(), "https://example.com/aabb?md5=aabb&sha=ccdd");
    }

    #[test]
    fn sha256_only_charts_require_a_sha256_http_template() {
        use crate::config::download_sources::HttpDownloadPreset;
        let mut metadata = metadata();
        metadata.md5.clear();
        metadata.ipfs.clear();
        let mut config = ChartDownloadsConfig {
            http_enabled: true,
            http_api_url: "https://api.test/{sha256}".into(),
            ..Default::default()
        };
        assert!(
            matches!(choose_missing_chart_action(&config, &metadata), MissingChartAction::Http { md5, .. } if md5.is_empty())
        );
        config.http_api_url.clear();
        config.http_source = HttpDownloadPreset::Ginger;
        assert!(matches!(
            choose_missing_chart_action(&config, &metadata),
            MissingChartAction::Browser(_)
        ));
        metadata.md5 = "../not-a-hash".into();
        assert!(matches!(
            choose_missing_chart_action(&config, &metadata),
            MissingChartAction::Browser(_)
        ));
        config.http_enabled = false;
        config.http_api_url = "https://api.test/{sha256}".into();
        assert!(matches!(
            choose_missing_chart_action(&config, &metadata),
            MissingChartAction::Browser(_)
        ));
    }

    #[test]
    fn ipfs_base_url_builds_kubo_get_endpoint() {
        let url = ipfs_download_url(
            "http://127.0.0.1:5001/",
            "/ipfs/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3ktekzrxql4i5f3u",
        )
        .unwrap();

        assert_eq!(url.path(), "/api/v0/get");
        assert!(url.query().unwrap().contains("archive=true"));
        assert!(url.query().unwrap().contains("compress=true"));
    }

    #[tokio::test]
    async fn ipfs_rpc_retries_only_method_rejections_and_preserves_templates() {
        use std::io::{Read, Write};
        for (template, statuses, expected) in [
            (false, vec![200], vec!["GET"]),
            (false, vec![405, 200], vec!["GET", "POST"]),
            (true, vec![405], vec!["GET"]),
            (false, vec![500], vec!["GET"]),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let final_status = *statuses.last().unwrap();
            let server = std::thread::spawn(move || {
                let mut requests = Vec::new();
                for status in statuses {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                    let mut request = Vec::new();
                    let mut byte = [0];
                    while !request.ends_with(b"\r\n\r\n") {
                        assert_eq!(stream.read(&mut byte).unwrap(), 1);
                        request.push(byte[0]);
                    }
                    requests.push(
                        String::from_utf8(request).unwrap().lines().next().unwrap().to_string(),
                    );
                    write!(stream, "HTTP/1.1 {status} Response\r\nContent-Length: 7\r\nConnection: close\r\n\r\narchive").unwrap();
                }
                requests
            });
            let api = if template {
                format!("http://{address}/custom/{{cid}}")
            } else {
                format!("http://{address}")
            };
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap();
            let result = request_ipfs_archive(&client, &api, &metadata().ipfs).await;
            if final_status == 200 {
                assert_eq!(result.unwrap().text().await.unwrap(), "archive");
            } else {
                assert_eq!(
                    result
                        .unwrap_err()
                        .downcast_ref::<reqwest::Error>()
                        .unwrap()
                        .status()
                        .unwrap()
                        .as_u16(),
                    final_status
                );
            }
            let requests = server.join().unwrap();
            assert_eq!(
                requests
                    .iter()
                    .map(|line| line.split_whitespace().next().unwrap())
                    .collect::<Vec<_>>(),
                expected
            );
            if !template {
                assert!(requests.iter().all(|line| line.contains("/api/v0/get?arg=")
                    && line.contains("archive=true")
                    && line.contains("compress=true")));
            }
        }
    }

    #[test]
    fn ginger_and_konmai_json_shapes_resolve_download_urls() {
        let ginger = serde_json::json!({"downloadURL": "https://cdn.example/song.7z"});
        let konmai = serde_json::json!({"data": {"song_url": "https://cdn.example/song.7z"}});

        assert_eq!(find_download_url(&ginger), Some("https://cdn.example/song.7z"));
        assert_eq!(find_download_url(&konmai), Some("https://cdn.example/song.7z"));
    }

    #[test]
    fn browser_urls_reject_non_http_and_duplicates() {
        let values = vec![
            "javascript:alert(1)".to_string(),
            "https://example.com".to_string(),
            "https://example.com".to_string(),
        ];

        assert_eq!(validated_browser_urls(&values), vec!["https://example.com"]);
    }

    #[tokio::test]
    async fn batch_download_continues_after_each_failed_request() {
        let requests = ["First", "Second"]
            .into_iter()
            .map(|title| ChartDownloadRequest {
                request_id: 0,
                identity: ChartDownloadIdentity::default(),
                action: MissingChartAction::Unavailable,
                title: title.to_string(),
                data_dir: PathBuf::new(),
            })
            .collect();

        let result = download_charts(requests).await;

        assert!(result.completed.is_empty());
        assert_eq!(
            result.failures.iter().map(|failure| failure.title.as_str()).collect::<Vec<_>>(),
            ["First", "Second"]
        );
    }

    #[test]
    fn saved_directory_name_removes_platform_separators() {
        assert_eq!(sanitize_name("A/B:C*D?"), "A_B_C_D_");
        assert_eq!(sanitize_name("..."), "download");
    }

    #[test]
    fn tar_gz_extracts_regular_files_under_destination() {
        let root = std::env::temp_dir().join(format!("bmz-download-test-{}", unique_stamp()));
        let archive_path = root.join("song.tar.gz");
        let destination = root.join("out");
        std::fs::create_dir_all(&root).unwrap();
        let file = File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(3);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "cid/song/test.bms", &b"BMS"[..]).unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        extract_tar_gz(&archive_path, &destination).unwrap();

        assert_eq!(std::fs::read(destination.join("cid/song/test.bms")).unwrap(), b"BMS");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tar_gz_rejects_symbolic_links() {
        let root = std::env::temp_dir().join(format!("bmz-download-test-{}", unique_stamp()));
        let archive_path = root.join("song.tar.gz");
        let destination = root.join("out");
        std::fs::create_dir_all(&root).unwrap();
        let file = File::create(&archive_path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        header.set_link_name("../outside").unwrap();
        header.set_cksum();
        builder.append_data(&mut header, "unsafe-link", std::io::empty()).unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();

        assert!(extract_tar_gz(&archive_path, &destination).is_err());
        assert!(!root.join("outside").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ipfs_package_wrapper_is_flattened_before_final_save() {
        let root = std::env::temp_dir().join(format!("bmz-download-test-{}", unique_stamp()));
        let package = root.join(".package-0");
        let wrapped = package.join("bafy-test/song");
        std::fs::create_dir_all(&wrapped).unwrap();
        std::fs::write(wrapped.join("test.bms"), b"BMS").unwrap();

        merge_ipfs_package(&package, &root).unwrap();

        assert_eq!(std::fs::read(root.join("song/test.bms")).unwrap(), b"BMS");
        assert!(!package.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
