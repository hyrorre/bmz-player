use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use everything_ipc::wm::{EverythingClient, FileInfo, RequestFlags};

use super::discovery::{
    ChartDiscovery, ChartFileEntry, DiscoveredArchive, discover_archive_charts,
    enclosing_song_folders, folder_has_loose_chart, is_chart_file_name, is_document_file_name,
    is_excluded_name, record_discovery_issue, usize_to_u32,
};
use super::{ScanConfig, ScanDiscoveryBackend, ScanDiscoveryOperation};

const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;
const FILETIME_TICKS_PER_SECOND: u64 = 10_000_000;
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything found nothing below the root: the root is empty or not indexed. Native
/// discovery takes over quietly, without reporting Everything as unavailable.
#[derive(Debug)]
pub(super) struct NoIndexedFiles;

impl std::fmt::Display for NoIndexedFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Everything returned no matching files")
    }
}

impl std::error::Error for NoIndexedFiles {}

pub(super) fn discover_chart_files_everything(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
    mut on_discovered: impl FnMut(u32),
) -> Result<ChartDiscovery> {
    if !scan.follow_symlinks {
        bail!("Everything discovery cannot preserve follow_symlinks=false");
    }
    // Everything can briefly retain stale results. Opening the root first also
    // preserves the native backend's unreadable-root behavior.
    std::fs::read_dir(root)
        .with_context(|| format!("song root is not readable: {}", root.display()))?;

    let absolute_root = std::path::absolute(root)
        .with_context(|| format!("failed to make song root absolute: {}", root.display()))?;
    let query = everything_query(&absolute_root)?;
    // The background service alone does not answer searches; the Everything app must
    // be running in this session. Instances named like 1.5 alpha's "1.5a" are not found.
    let client = EverythingClient::new()
        .context("Everything is not running (the Everything service alone cannot search)")?;
    if !client.is_db_loaded() {
        bail!("Everything database is not loaded");
    }
    if !client.is_file_info_indexed(FileInfo::FileSize) {
        bail!("Everything file-size indexing is disabled");
    }
    if !client.is_file_info_indexed(FileInfo::DateModified) {
        bail!("Everything date-modified indexing is disabled");
    }

    let request_flags = RequestFlags::FileName
        | RequestFlags::Path
        | RequestFlags::Size
        | RequestFlags::DateModified;
    let results = client
        .query_wait(&query)
        .request_flags(request_flags)
        .timeout(QUERY_TIMEOUT)
        .call()
        .with_context(|| format!("Everything query failed: {query}"))?;

    if results.len() != results.total_len() {
        bail!(
            "Everything returned a partial result set ({}/{})",
            results.len(),
            results.total_len()
        );
    }
    // An empty result can also mean that the configured root is not indexed.
    // Native discovery is cheap for a genuinely empty root and is the safe fallback.
    if results.is_empty() {
        return Err(NoIndexedFiles.into());
    }

    let mut entries = Vec::new();
    let mut archives = Vec::new();
    let mut issues = Vec::new();
    let mut complete = true;
    let mut document_folders = HashSet::new();
    let mut verified = HashMap::new();
    for item in results.iter() {
        let parent = item
            .get_str(RequestFlags::Path)
            .context("Everything result is missing its parent path")?;
        let file_name = item
            .get_str(RequestFlags::FileName)
            .context("Everything result is missing its file name")?;
        let absolute_path = PathBuf::from(parent.to_os_string()).join(file_name.to_os_string());
        let Some(relative_path) = path_relative_to_root(&absolute_path, &absolute_root) else {
            continue;
        };
        if !recursive && relative_path.parent().is_some_and(|parent| !parent.as_os_str().is_empty())
        {
            continue;
        }
        if relative_path
            .components()
            .any(|component| matches!(component, Component::Normal(name) if is_excluded_name(name)))
        {
            continue;
        }

        let name = relative_path.file_name().unwrap_or_default();
        let output_path =
            if root.is_absolute() { absolute_path } else { root.join(&relative_path) };
        if is_document_file_name(name) {
            if let Some(parent) = output_path.parent() {
                document_folders.insert(parent.to_path_buf());
            }
            continue;
        }
        if crate::chart_source::is_archive_file(&output_path) {
            if inside_song_folder(root, &relative_path, true, &mut verified) {
                continue;
            }
            match discover_archive_charts(&output_path) {
                Ok((charts, generation)) => {
                    entries.extend(charts);
                    archives.push(DiscoveredArchive { path: output_path, generation });
                }
                Err(error) => {
                    complete &= crate::song_archive::is_unsupported(&error);
                    record_discovery_issue(
                        &mut issues,
                        root,
                        &output_path,
                        ScanDiscoveryOperation::ReadArchive,
                        std::io::Error::other(format!("{error:#}")),
                    );
                }
            }
            continue;
        }
        if !is_chart_file_name(name)
            || inside_song_folder(root, &relative_path, false, &mut verified)
        {
            continue;
        }

        let file_size = item
            .get_size(RequestFlags::Size)
            .context("Everything result is missing its file size")?;
        let modified = item
            .get_time(RequestFlags::DateModified)
            .context("Everything result is missing its modified time")?;
        let filetime = ((modified.dwHighDateTime as u64) << 32) | modified.dwLowDateTime as u64;
        entries.push(ChartFileEntry {
            path: output_path,
            file_size,
            modified_at: filetime_to_unix_seconds(filetime),
            has_document: false,
        });
    }

    for entry in &mut entries {
        entry.has_document |=
            entry.path.parent().is_some_and(|parent| document_folders.contains(parent));
    }
    for count in 1..=usize_to_u32(entries.len()) {
        on_discovered(count);
    }

    tracing::debug!(
        version = ?client.get_version(),
        query_results = results.len(),
        charts = entries.len(),
        root = %root.display(),
        "Everything song discovery query complete"
    );

    Ok(ChartDiscovery {
        entries,
        archives,
        complete,
        issues,
        root_readable: true,
        backend: ScanDiscoveryBackend::Everything,
        fallback_reason: None,
    })
}

fn folder_key(path: &Path) -> String {
    crate::paths::normalize_library_path(&path.to_string_lossy()).to_lowercase()
}

fn inside_song_folder(
    root: &Path,
    relative: &Path,
    archive: bool,
    verified: &mut HashMap<String, bool>,
) -> bool {
    enclosing_song_folders(relative, archive).any(|ancestor| {
        // The index can still contain deleted charts or miss newly added ones.
        *verified
            .entry(folder_key(ancestor))
            .or_insert_with(|| folder_has_loose_chart(&root.join(ancestor)))
    })
}

fn everything_query(root: &Path) -> Result<String> {
    let mut root = root.to_string_lossy().replace('/', "\\");
    if root.contains('"') {
        bail!("song root contains a quote and cannot be used in an Everything query");
    }
    if !root.ends_with('\\') {
        root.push('\\');
    }
    let extensions = bmz_song_resource::ARCHIVE_EXTENSIONS
        .iter()
        .map(|extension| extension.rsplit('.').next().unwrap())
        .collect::<Vec<_>>()
        .join(";");
    Ok(format!(r#"file: "{root}" ext:bms;bme;bml;bmc;pms;bmson;txt;{extensions}"#))
}

fn path_relative_to_root(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative.to_path_buf());
    }

    // Windows paths are case-insensitive. Everything normally returns the same
    // casing as the configured root; this covers drive/root casing differences.
    let path_text = path.to_string_lossy().replace('/', "\\");
    let root_text = root.to_string_lossy().replace('/', "\\");
    let root_without_separator = root_text.trim_end_matches('\\');
    if !path_text.get(..root_without_separator.len())?.eq_ignore_ascii_case(root_without_separator)
    {
        return None;
    }
    let suffix = path_text.get(root_without_separator.len()..)?;
    if !suffix.is_empty() && !suffix.starts_with('\\') {
        return None;
    }
    Some(PathBuf::from(suffix.trim_start_matches('\\')))
}

fn filetime_to_unix_seconds(filetime: u64) -> i64 {
    filetime
        .checked_sub(FILETIME_UNIX_EPOCH)
        .map(|ticks| (ticks / FILETIME_TICKS_PER_SECOND).min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_folder_filter_keeps_direct_charts_and_ignores_stale_index_matches() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path();
        std::fs::create_dir(path.join("Song")).unwrap();
        std::fs::create_dir(path.join("Stale")).unwrap();
        std::fs::write(path.join("Song/a.bms"), "").unwrap();
        let mut verified = HashMap::new();
        let mut excluded = |relative: &str, archive| {
            inside_song_folder(path, Path::new(relative), archive, &mut verified)
        };
        assert!(!excluded("SONG/a.bms", false));
        assert!(excluded("Song/nested/b.bms", false));
        assert!(excluded("Song/pack.zip", true));
        assert!(excluded("Song/nested/pack.zip", true));
        assert!(!excluded("Song-other/pack.zip", true));
        assert!(!excluded("Stale/nested/b.bms", false));
        assert!(!excluded("Stale/pack.zip", true));
    }

    #[test]
    fn query_quotes_root_and_adds_trailing_separator() {
        assert_eq!(
            everything_query(Path::new(r"G:\BMS")).unwrap(),
            r#"file: "G:\BMS\" ext:bms;bme;bml;bmc;pms;bmson;txt;zip;rar;7z;7zip;gz;tgz;lzh;lha"#
        );
    }

    #[test]
    fn relative_path_comparison_is_case_insensitive() {
        assert_eq!(
            path_relative_to_root(Path::new(r"g:\bms\Folder\song.bms"), Path::new(r"G:\BMS")),
            Some(PathBuf::from(r"Folder\song.bms"))
        );
        assert_eq!(
            path_relative_to_root(Path::new(r"G:\BMS-old\song.bms"), Path::new(r"G:\BMS")),
            None
        );
    }

    #[test]
    fn filetime_conversion_matches_unix_seconds() {
        assert_eq!(filetime_to_unix_seconds(FILETIME_UNIX_EPOCH), 0);
        assert_eq!(
            filetime_to_unix_seconds(FILETIME_UNIX_EPOCH + 42 * FILETIME_TICKS_PER_SECOND),
            42
        );
        assert_eq!(filetime_to_unix_seconds(FILETIME_UNIX_EPOCH - 1), 0);
    }
}
