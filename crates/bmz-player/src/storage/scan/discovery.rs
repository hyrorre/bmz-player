use super::*;

#[derive(Debug, Clone)]
pub struct ChartFileEntry {
    pub path: PathBuf,
    pub file_size: u64,
    pub modified_at: i64,
    pub has_document: bool,
}

#[derive(Debug, Default)]
pub(super) struct ChartDiscovery {
    pub(super) entries: Vec<ChartFileEntry>,
    pub(super) issues: Vec<ScanDiscoveryIssue>,
    pub(super) complete: bool,
    pub(super) root_readable: bool,
    pub(super) backend: ScanDiscoveryBackend,
}

pub fn discover_chart_files(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
) -> Result<Vec<ChartFileEntry>> {
    Ok(discover_chart_files_with_progress(root, recursive, scan, |_| {}).entries)
}

pub(super) fn discover_chart_files_with_progress(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
    on_discovered: impl FnMut(u32),
) -> ChartDiscovery {
    #[cfg(windows)]
    let mut on_discovered = on_discovered;

    #[cfg(windows)]
    if scan.use_everything {
        match super::everything::discover_chart_files_everything(
            root,
            recursive,
            scan,
            &mut on_discovered,
        ) {
            Ok(discovery) => return discovery,
            Err(error) => {
                tracing::warn!(
                    root = %root.display(),
                    error = %error,
                    "Everything song discovery failed; falling back to native discovery"
                );
                let mut discovery =
                    discover_chart_files_native(root, recursive, scan, on_discovered);
                discovery.backend = ScanDiscoveryBackend::NativeFallback;
                return discovery;
            }
        }
    }

    #[cfg(not(windows))]
    if scan.use_everything {
        tracing::warn!(
            root = %root.display(),
            "Everything song discovery is only available on Windows; falling back to native discovery"
        );
        let mut discovery = discover_chart_files_native(root, recursive, scan, on_discovered);
        discovery.backend = ScanDiscoveryBackend::NativeFallback;
        return discovery;
    }

    discover_chart_files_native(root, recursive, scan, on_discovered)
}

fn discover_chart_files_native(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
    mut on_discovered: impl FnMut(u32),
) -> ChartDiscovery {
    let mut discovery = ChartDiscovery { complete: true, ..Default::default() };
    let mut dirs = vec![root.to_path_buf()];
    let mut discovered_count = 0_u32;

    while let Some(dir) = dirs.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => {
                if dir == root {
                    discovery.root_readable = true;
                }
                entries
            }
            Err(error) => {
                discovery.complete = false;
                record_discovery_issue(
                    &mut discovery.issues,
                    root,
                    &dir,
                    ScanDiscoveryOperation::OpenDirectory,
                    error,
                );
                continue;
            }
        };
        let mut charts_in_dir = Vec::new();
        let mut has_document = false;
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    discovery.complete = false;
                    record_discovery_issue(
                        &mut discovery.issues,
                        root,
                        &dir,
                        ScanDiscoveryOperation::ReadEntry,
                        error,
                    );
                    continue;
                }
            };
            let file_name = entry.file_name();
            if scan.skip_hidden && is_hidden_name(&file_name) {
                continue;
            }
            let path = entry.path();

            let (file_type, meta_opt) = if scan.follow_symlinks {
                let meta = match entry.metadata() {
                    Ok(meta) => meta,
                    Err(error) => {
                        discovery.complete = false;
                        record_discovery_issue(
                            &mut discovery.issues,
                            root,
                            &path,
                            ScanDiscoveryOperation::ReadMetadata,
                            error,
                        );
                        continue;
                    }
                };
                let ft = meta.file_type();
                (ft, Some(meta))
            } else {
                let file_type = match entry.file_type() {
                    Ok(file_type) => file_type,
                    Err(error) => {
                        discovery.complete = false;
                        record_discovery_issue(
                            &mut discovery.issues,
                            root,
                            &path,
                            ScanDiscoveryOperation::ReadFileType,
                            error,
                        );
                        continue;
                    }
                };
                (file_type, None)
            };

            if file_type.is_file() && is_document_file_name(&file_name) {
                has_document = true;
            }

            if file_type.is_dir() {
                if recursive {
                    dirs.push(path);
                }
            } else if file_type.is_file() && is_chart_file_name(&file_name) {
                let metadata = match meta_opt {
                    Some(metadata) => metadata,
                    None => match entry.metadata() {
                        Ok(metadata) => metadata,
                        Err(error) => {
                            discovery.complete = false;
                            record_discovery_issue(
                                &mut discovery.issues,
                                root,
                                &path,
                                ScanDiscoveryOperation::ReadMetadata,
                                error,
                            );
                            continue;
                        }
                    },
                };
                let modified_at = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                charts_in_dir.push(ChartFileEntry {
                    path,
                    file_size: metadata.len(),
                    modified_at,
                    has_document: false,
                });
                discovered_count = discovered_count.saturating_add(1);
                on_discovered(discovered_count);
            }
        }
        charts_in_dir.iter_mut().for_each(|entry| entry.has_document = has_document);
        discovery.entries.extend(charts_in_dir);
    }

    discovery
}

pub(super) fn record_discovery_issue(
    issues: &mut Vec<ScanDiscoveryIssue>,
    root: &Path,
    path: &Path,
    operation: ScanDiscoveryOperation,
    error: io::Error,
) {
    tracing::warn!(
        root = %root.display(),
        path = %path.display(),
        operation = operation.as_str(),
        error_kind = ?error.kind(),
        raw_os_error = ?error.raw_os_error(),
        error = %error,
        "skipping inaccessible song scan path"
    );
    issues.push(ScanDiscoveryIssue {
        root: root.to_path_buf(),
        path: path.to_path_buf(),
        operation,
        error_kind: error.kind(),
        raw_os_error: error.raw_os_error(),
        message: error.to_string(),
    });
}

pub(super) fn usize_to_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

pub(super) fn is_document_file_name(name: &std::ffi::OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
}

pub(crate) fn folder_has_document(folder: &Path) -> bool {
    std::fs::read_dir(folder).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|file_type| file_type.is_file())
                && is_document_file_name(&entry.file_name())
        })
    })
}

pub(super) fn is_chart_file_name(name: &std::ffi::OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "bms" | "bme" | "bml" | "bmc" | "pms" | "bmson"
            )
        })
        .unwrap_or(false)
}

pub(super) fn is_hidden_name(name: &std::ffi::OsStr) -> bool {
    name.to_str().map(|name| name.starts_with('.')).unwrap_or(false)
}
