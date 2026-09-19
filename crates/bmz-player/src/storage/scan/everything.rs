use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use everything_ipc::wm::{EverythingClient, FileInfo, RequestFlags};

use super::discovery::{
    ChartDiscovery, ChartFileEntry, is_chart_file_name, is_document_file_name, usize_to_u32,
};
use super::{ScanConfig, ScanDiscoveryBackend};

const FILETIME_UNIX_EPOCH: u64 = 116_444_736_000_000_000;
const FILETIME_TICKS_PER_SECOND: u64 = 10_000_000;
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

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
    let client = EverythingClient::new().context("Everything IPC window is unavailable")?;
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
        bail!("Everything returned no matching files");
    }

    let mut chart_entries = Vec::new();
    let mut document_folders = HashSet::new();
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
        if scan.skip_hidden && has_dot_prefixed_component(&relative_path) {
            continue;
        }

        let output_path =
            if root.is_absolute() { absolute_path } else { root.join(&relative_path) };
        if is_document_file_name(output_path.file_name().unwrap_or_default()) {
            if let Some(parent) = output_path.parent() {
                document_folders.insert(parent.to_path_buf());
            }
            continue;
        }
        if !is_chart_file_name(output_path.file_name().unwrap_or_default()) {
            continue;
        }

        let file_size = item
            .get_size(RequestFlags::Size)
            .context("Everything result is missing its file size")?;
        let modified = item
            .get_time(RequestFlags::DateModified)
            .context("Everything result is missing its modified time")?;
        let filetime = ((modified.dwHighDateTime as u64) << 32) | modified.dwLowDateTime as u64;
        chart_entries.push(ChartFileEntry {
            path: output_path,
            file_size,
            modified_at: filetime_to_unix_seconds(filetime),
            has_document: false,
        });
    }

    for entry in &mut chart_entries {
        entry.has_document =
            entry.path.parent().is_some_and(|parent| document_folders.contains(parent));
    }
    for count in 1..=usize_to_u32(chart_entries.len()) {
        on_discovered(count);
    }

    tracing::debug!(
        version = ?client.get_version(),
        query_results = results.len(),
        charts = chart_entries.len(),
        root = %root.display(),
        "Everything song discovery query complete"
    );

    Ok(ChartDiscovery {
        entries: chart_entries,
        issues: Vec::new(),
        complete: true,
        root_readable: true,
        backend: ScanDiscoveryBackend::Everything,
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
    Ok(format!(r#"file: "{root}" ext:bms;bme;bml;bmc;pms;bmson;txt"#))
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

fn has_dot_prefixed_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(|name| name.starts_with('.')),
        _ => false,
    })
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
    fn query_quotes_root_and_adds_trailing_separator() {
        assert_eq!(
            everything_query(Path::new(r"G:\BMS")).unwrap(),
            r#"file: "G:\BMS\" ext:bms;bme;bml;bmc;pms;bmson;txt"#
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
    fn hidden_filter_checks_every_relative_component() {
        assert!(has_dot_prefixed_component(Path::new(r".root.bms")));
        assert!(has_dot_prefixed_component(Path::new(r"folder\.cache\song.bms")));
        assert!(!has_dot_prefixed_component(Path::new(r"folder\song.bms")));
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
