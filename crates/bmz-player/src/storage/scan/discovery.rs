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
    pub(super) archives: Vec<PathBuf>,
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

/// Directories read in parallel before their results are merged.
const DISCOVERY_DIRECTORY_CHUNK: usize = 256;

fn discover_chart_files_native(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
    mut on_discovered: impl FnMut(u32),
) -> ChartDiscovery {
    let mut discovery = ChartDiscovery { complete: true, ..Default::default() };
    // Linked directories are followed once, and never into a tree that is already
    // scanned. A cycle needs a link, so plain directories need no identity check.
    let linked_roots = std::sync::Mutex::new(
        scan.follow_symlinks.then(|| root.canonicalize().ok()).flatten().into_iter().collect(),
    );
    let mut discovered_count = 0_u32;
    let mut level = vec![root.to_path_buf()];

    // Breadth-first, one directory level at a time: every directory of a level is
    // independent, so slow stat/readdir calls overlap across rayon workers.
    while !level.is_empty() {
        let mut next_level = Vec::new();
        for chunk in level.chunks(DISCOVERY_DIRECTORY_CHUNK) {
            let listings: Vec<DirectoryListing> = chunk
                .par_iter()
                .map(|dir| list_directory(root, dir, recursive, scan, &linked_roots))
                .collect();
            for (dir, listing) in chunk.iter().zip(listings) {
                if dir == root && listing.readable {
                    discovery.root_readable = true;
                }
                discovery.complete &= listing.complete;
                for entry in listing.entries {
                    discovery.entries.push(entry);
                    discovered_count = discovered_count.saturating_add(1);
                    on_discovered(discovered_count);
                }
                discovery.archives.extend(listing.archives);
                discovery.issues.extend(listing.issues);
                next_level.extend(listing.subdirs);
            }
        }
        level = next_level;
    }

    discovery
}

#[derive(Default)]
struct DirectoryListing {
    readable: bool,
    complete: bool,
    entries: Vec<ChartFileEntry>,
    archives: Vec<PathBuf>,
    issues: Vec<ScanDiscoveryIssue>,
    subdirs: Vec<PathBuf>,
}

/// Reads one directory. Only chart files and followed links are stat-ed: on Unix
/// `DirEntry::metadata` is an extra syscall per entry, while `file_type` comes from
/// the directory listing itself.
fn list_directory(
    root: &Path,
    dir: &Path,
    recursive: bool,
    scan: &ScanConfig,
    linked_roots: &std::sync::Mutex<Vec<PathBuf>>,
) -> DirectoryListing {
    let mut listing = DirectoryListing { complete: true, ..Default::default() };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => {
            listing.readable = true;
            entries
        }
        Err(error) => {
            listing.complete = false;
            record_discovery_issue(
                &mut listing.issues,
                root,
                dir,
                ScanDiscoveryOperation::OpenDirectory,
                error,
            );
            return listing;
        }
    };
    let mut charts_in_dir = Vec::new();
    let mut has_document = false;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                listing.complete = false;
                record_discovery_issue(
                    &mut listing.issues,
                    root,
                    dir,
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

        let mut file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                listing.complete = false;
                record_discovery_issue(
                    &mut listing.issues,
                    root,
                    &path,
                    ScanDiscoveryOperation::ReadFileType,
                    error,
                );
                continue;
            }
        };
        let mut metadata = None;
        if file_type.is_symlink() {
            if !scan.follow_symlinks {
                continue;
            }
            let target = match std::fs::metadata(&path) {
                Ok(target) => target,
                Err(error) => {
                    listing.complete = false;
                    record_discovery_issue(
                        &mut listing.issues,
                        root,
                        &path,
                        ScanDiscoveryOperation::ReadMetadata,
                        error,
                    );
                    continue;
                }
            };
            file_type = target.file_type();
            if file_type.is_dir() && !(recursive && claim_linked_directory(&path, linked_roots)) {
                continue;
            }
            metadata = Some(target);
        }

        if file_type.is_file() && is_document_file_name(&file_name) {
            has_document = true;
        }

        if file_type.is_dir() {
            if recursive {
                listing.subdirs.push(path);
            }
        } else if file_type.is_file() && crate::chart_source::is_archive_file(&path) {
            match discover_archive_charts(&path, scan.skip_hidden) {
                Ok(entries) => {
                    listing.archives.push(path.clone());
                    listing.entries.extend(entries);
                }
                Err(error) => {
                    listing.complete = false;
                    record_discovery_issue(
                        &mut listing.issues,
                        root,
                        &path,
                        ScanDiscoveryOperation::ReadArchive,
                        io::Error::other(format!("{error:#}")),
                    );
                }
            }
        } else if file_type.is_file() && is_chart_file_name(&file_name) {
            let metadata = match metadata.map_or_else(|| entry.metadata(), Ok) {
                Ok(metadata) => metadata,
                Err(error) => {
                    listing.complete = false;
                    record_discovery_issue(
                        &mut listing.issues,
                        root,
                        &path,
                        ScanDiscoveryOperation::ReadMetadata,
                        error,
                    );
                    continue;
                }
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
        }
    }
    charts_in_dir.iter_mut().for_each(|entry| entry.has_document = has_document);
    listing.entries.extend(charts_in_dir);
    listing
}

/// Claims a linked directory for this scan unless its target lies inside the root
/// or an already followed link, which would list the same charts twice or loop.
fn claim_linked_directory(link: &Path, linked_roots: &std::sync::Mutex<Vec<PathBuf>>) -> bool {
    let Ok(target) = link.canonicalize() else {
        return false;
    };
    let mut linked_roots = linked_roots.lock().unwrap_or_else(|poison| poison.into_inner());
    if linked_roots.iter().any(|scanned| target.starts_with(scanned)) {
        return false;
    }
    linked_roots.push(target);
    true
}

pub(super) fn discover_archive_charts(
    path: &Path,
    skip_hidden: bool,
) -> Result<Vec<ChartFileEntry>> {
    let index = crate::song_archive::inspect(path, &Default::default())?;
    let modified_at = std::fs::metadata(path)?
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |time| time.as_secs() as i64);
    Ok(index
        .entries
        .iter()
        .filter(|entry| {
            entry.is_chart()
                && !entry.name.split('/').any(|part| {
                    part == "__MACOSX"
                        || part.starts_with("._")
                        || (skip_hidden && part.starts_with('.'))
                })
        })
        .map(|entry| ChartFileEntry {
            path: crate::chart_source::ChartLocator::Archive {
                container: path.to_path_buf(),
                entry: entry.name.clone(),
            }
            .to_path_buf(),
            file_size: entry.size,
            modified_at,
            has_document: crate::chart_source::archive_folder_has_document(&index, &entry.name),
        })
        .collect())
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
                "bms" | "bme" | "bml" | "pms" | "bmson"
            )
        })
        .unwrap_or(false)
}

pub(super) fn is_hidden_name(name: &std::ffi::OsStr) -> bool {
    name.to_str().map(|name| name.starts_with('.')).unwrap_or(false)
}
