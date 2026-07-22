use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::{Response, Url};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

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
    pub action: MissingChartAction,
    pub title: String,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ChartDownloadResult {
    pub source: ChartDownloadSource,
    pub root_dir: PathBuf,
    pub chart_dir: PathBuf,
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

    if config.http_enabled
        && !config.http_api_url.trim().is_empty()
        && !metadata.md5.trim().is_empty()
    {
        return MissingChartAction::Http {
            api_url: config.http_api_url.trim().to_string(),
            md5: metadata.md5.trim().to_ascii_lowercase(),
            sha256: metadata.sha256.trim().to_ascii_lowercase(),
        };
    }

    let urls = validated_browser_urls([&metadata.url, &metadata.append_url]);
    if urls.is_empty() {
        MissingChartAction::Unavailable
    } else {
        MissingChartAction::Browser(urls)
    }
}

pub fn open_browser_urls(urls: &[String]) -> Result<usize> {
    let urls = validated_browser_urls(urls.iter());
    for url in &urls {
        webbrowser::open(url).with_context(|| format!("ブラウザでURLを開けませんでした: {url}"))?;
    }
    Ok(urls.len())
}

fn validated_browser_urls<'a>(urls: impl IntoIterator<Item = &'a String>) -> Vec<String> {
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

pub async fn download_chart(request: ChartDownloadRequest) -> Result<ChartDownloadResult> {
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

    let hash_hint = match &request.action {
        MissingChartAction::Ipfs { api_url, cids } => {
            for (index, cid) in cids.iter().enumerate() {
                let url = ipfs_download_url(api_url, cid)?;
                let response = client.get(url).send().await?.error_for_status()?;
                save_response(response, &archive_path).await?;
                let package_dir = staging_dir.join(format!(".package-{index}"));
                std::fs::create_dir(&package_dir)?;
                extract_archive(&archive_path, &package_dir)?;
                merge_ipfs_package(&package_dir, &staging_dir)?;
                std::fs::remove_file(&archive_path).ok();
                if index + 1 < cids.len() {
                    tracing::debug!("extracting additional IPFS package");
                }
            }
            normalize_ipfs_arg(cids.first().expect("IPFS action has at least one CID"))?
        }
        MissingChartAction::Http { api_url, md5, sha256 } => {
            let api_url = expand_http_api_url(api_url, md5, sha256)?;
            let response = client.get(api_url.clone()).send().await?.error_for_status()?;
            let archive_response = if response_is_json(&response) {
                let content_length = response.content_length().unwrap_or(0);
                if content_length > MAX_METADATA_BYTES {
                    bail!("HTTP APIのJSON応答が大きすぎます")
                }
                let body = response.bytes().await?;
                if body.len() as u64 > MAX_METADATA_BYTES {
                    bail!("HTTP APIのJSON応答が大きすぎます")
                }
                let value: Value = serde_json::from_slice(&body)
                    .context("HTTP APIのJSON応答を解析できませんでした")?;
                let download_url = find_download_url(&value)
                    .ok_or_else(|| anyhow::anyhow!("HTTP API応答にダウンロードURLがありません"))?;
                let download_url = resolve_http_url(&api_url, download_url)?;
                client.get(download_url).send().await?.error_for_status()?
            } else {
                response
            };
            save_response(archive_response, &archive_path).await?;
            extract_archive(&archive_path, &staging_dir)?;
            md5.clone()
        }
        MissingChartAction::Browser(_) | MissingChartAction::Unavailable => unreachable!(),
    };

    let final_dir = unique_final_dir(&root_dir, &request.title, &hash_hint);
    std::fs::rename(&staging_dir, &final_dir).with_context(|| {
        format!("展開済みフォルダを保存できませんでした: {}", final_dir.display())
    })?;
    std::fs::remove_file(&archive_path).ok();
    cleanup.disarm();

    Ok(ChartDownloadResult { source, root_dir, chart_dir: final_dir })
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

fn find_download_url(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    for key in ["downloadURL", "downloadUrl", "download_url", "song_url", "url"] {
        if let Some(url) =
            object.get(key).and_then(Value::as_str).filter(|url| !url.trim().is_empty())
        {
            return Some(url);
        }
    }
    object.get("data").and_then(find_download_url)
}

fn resolve_http_url(base: &Url, value: &str) -> Result<Url> {
    let url = base.join(value.trim()).context("HTTP APIのダウンロードURLが不正です")?;
    validate_http_scheme(&url)?;
    Ok(url)
}

async fn save_response(mut response: Response, path: &Path) -> Result<()> {
    if response.content_length().is_some_and(|length| length > MAX_DOWNLOAD_BYTES) {
        bail!("ダウンロードサイズが上限を超えています")
    }
    let mut file = tokio::fs::File::create(path).await?;
    let mut total = 0_u64;
    while let Some(chunk) = response.chunk().await? {
        total = total.saturating_add(chunk.len() as u64);
        if total > MAX_DOWNLOAD_BYTES {
            bail!("ダウンロードサイズが上限を超えています")
        }
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    if total == 0 {
        bail!("ダウンロードしたアーカイブが空です")
    }
    Ok(())
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    let mut signature = [0_u8; 8];
    let read = File::open(archive_path)?.read(&mut signature)?;
    if read >= 2 && signature[..2] == [0x1f, 0x8b] {
        extract_tar_gz(archive_path, destination)
    } else if read >= 6 && signature[..6] == [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c] {
        extract_7z(archive_path, destination)
    } else {
        bail!("対応していないアーカイブ形式です（tar.gz / 7z のみ対応）")
    }
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let file = File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(false);
    archive.set_preserve_ownerships(false);
    let mut count = 0_usize;
    let mut total = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
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

fn extract_7z(archive_path: &Path, destination: &Path) -> Result<()> {
    let mut count = 0_usize;
    let mut total = 0_u64;
    sevenz_rust2::decompress_file_with_extract_fn(
        archive_path,
        destination,
        |entry, reader, path| {
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
            sevenz_rust2::default_entry_extract_fn(entry, reader, path)
        },
    )
    .context("7zアーカイブを展開できませんでした")
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
    let candidate = root.join(&stem);
    if !candidate.exists() { candidate } else { root.join(format!("{stem}-{}", unique_stamp())) }
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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
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
