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

/// Native discovery in rounds. A round reads whole directory trees breadth-first in
/// parallel and only collects the symbolic links it meets; the links are then adopted
/// one by one in a fixed order, so the chosen paths never depend on rayon scheduling
/// or on `read_dir` order. Adopted link targets form the next round.
///
/// Every tree is identified by its canonical path. A real subdirectory's canonical
/// path is its parent's plus its name, so only links and the root cost a canonicalize.
/// A link is adopted only when its target lies outside every tree adopted so far and
/// does not contain the root; a target that contains an earlier tree is read with
/// that tree excluded. Each directory is therefore read at most once, real paths
/// under the root win over links, and the walk ends even with cyclic links.
fn discover_chart_files_native(
    root: &Path,
    recursive: bool,
    scan: &ScanConfig,
    mut on_discovered: impl FnMut(u32),
) -> ChartDiscovery {
    let mut discovery = ChartDiscovery { complete: true, ..Default::default() };
    let root_canonical = if scan.follow_symlinks {
        match root.canonicalize() {
            Ok(canonical) => Some(canonical),
            // `read_dir` below reports a missing root.
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                discovery.complete = false;
                record_discovery_issue(
                    &mut discovery.issues,
                    root,
                    root,
                    ScanDiscoveryOperation::ReadMetadata,
                    error,
                );
                None
            }
        }
    } else {
        None
    };
    let mut walk = DiscoveryWalk {
        root,
        recursive,
        skip_hidden: scan.skip_hidden,
        follow_links: root_canonical.is_some(),
        trees: root_canonical.iter().cloned().collect(),
        root_canonical: root_canonical.clone(),
        regular_files: HashSet::new(),
        linked_files: Vec::new(),
        discovered_count: 0,
    };
    let mut units = vec![DirectoryUnit { path: root.to_path_buf(), canonical: root_canonical }];
    while !units.is_empty() {
        let mut linked_dirs = walk.read_trees(units, &mut discovery, &mut on_discovered);
        linked_dirs.sort_by(|a, b| link_order(&a.link, &b.link));
        units = Vec::new();
        for link in linked_dirs {
            // Ancestors, not a scan of every tree: thousands of links stay linear.
            if let Some(tree) = link.target.ancestors().find(|dir| walk.trees.contains(*dir)) {
                tracing::debug!(
                    link = %link.link.display(),
                    target = %link.target.display(),
                    scanned_as = %tree.display(),
                    "skipping song folder link into an already scanned folder"
                );
                continue;
            }
            if walk.root_canonical.as_ref().is_some_and(|root| root.starts_with(&link.target)) {
                tracing::debug!(
                    link = %link.link.display(),
                    target = %link.target.display(),
                    "skipping song folder link that contains the song root"
                );
                continue;
            }
            walk.trees.insert(link.target.clone());
            units.push(DirectoryUnit { path: link.link, canonical: Some(link.target) });
        }
    }
    walk.adopt_linked_files(&mut discovery, &mut on_discovered);
    discovery
}

struct DiscoveryWalk<'a> {
    root: &'a Path,
    recursive: bool,
    skip_hidden: bool,
    follow_links: bool,
    root_canonical: Option<PathBuf>,
    /// Canonical paths of the root and every adopted link target.
    trees: HashSet<PathBuf>,
    /// Canonical paths of chart and archive files read through real directories.
    regular_files: HashSet<PathBuf>,
    linked_files: Vec<LinkedFile>,
    discovered_count: u32,
}

struct DirectoryUnit {
    /// Path as the library stores it, below the configured root or a link.
    path: PathBuf,
    /// Present only while links are followed.
    canonical: Option<PathBuf>,
}

struct LinkedDirectory {
    link: PathBuf,
    target: PathBuf,
}

struct LinkedFile {
    link: PathBuf,
    target: PathBuf,
    metadata: std::fs::Metadata,
    archive: bool,
    has_document: bool,
}

/// Shallower links first, then by path, so the same tree always picks the same link.
fn link_order(a: &Path, b: &Path) -> std::cmp::Ordering {
    a.components().count().cmp(&b.components().count()).then_with(|| a.cmp(b))
}

impl DiscoveryWalk<'_> {
    fn read_trees(
        &mut self,
        mut level: Vec<DirectoryUnit>,
        discovery: &mut ChartDiscovery,
        on_discovered: &mut impl FnMut(u32),
    ) -> Vec<LinkedDirectory> {
        let mut linked_dirs = Vec::new();
        // Breadth-first, one directory level at a time: every directory of a level is
        // independent, so slow stat/readdir calls overlap across rayon workers.
        while !level.is_empty() {
            let mut next_level = Vec::new();
            for chunk in level.chunks(DISCOVERY_DIRECTORY_CHUNK) {
                let listings: Vec<DirectoryListing> =
                    chunk.par_iter().map(|unit| self.list_directory(unit)).collect();
                for (unit, listing) in chunk.iter().zip(listings) {
                    if unit.path == self.root && listing.readable {
                        discovery.root_readable = true;
                    }
                    discovery.complete &= listing.complete;
                    for entry in listing.entries {
                        discovery.entries.push(entry);
                        self.discovered_count = self.discovered_count.saturating_add(1);
                        on_discovered(self.discovered_count);
                    }
                    discovery.archives.extend(listing.archives);
                    discovery.issues.extend(listing.issues);
                    next_level.extend(listing.subdirs);
                    linked_dirs.extend(listing.linked_dirs);
                    self.linked_files.extend(listing.linked_files);
                    self.regular_files.extend(listing.regular_files);
                }
            }
            level = next_level;
        }
        linked_dirs
    }

    /// Linked chart and archive files are kept unless their target was already read
    /// through a real directory or an earlier link, in [`link_order`].
    fn adopt_linked_files(
        &mut self,
        discovery: &mut ChartDiscovery,
        on_discovered: &mut impl FnMut(u32),
    ) {
        let mut linked_files = std::mem::take(&mut self.linked_files);
        linked_files.sort_by(|a, b| link_order(&a.link, &b.link));
        for file in linked_files {
            if !self.regular_files.insert(file.target.clone()) {
                tracing::debug!(
                    link = %file.link.display(),
                    target = %file.target.display(),
                    "skipping chart link to an already scanned file"
                );
                continue;
            }
            let entries = if file.archive {
                match discover_archive_charts(&file.link, self.skip_hidden) {
                    Ok(entries) => {
                        discovery.archives.push(file.link.clone());
                        entries
                    }
                    Err(error) => {
                        discovery.complete = false;
                        record_discovery_issue(
                            &mut discovery.issues,
                            self.root,
                            &file.link,
                            ScanDiscoveryOperation::ReadArchive,
                            io::Error::other(format!("{error:#}")),
                        );
                        continue;
                    }
                }
            } else {
                vec![chart_file_entry(file.link, &file.metadata, file.has_document)]
            };
            for entry in entries {
                discovery.entries.push(entry);
                self.discovered_count = self.discovered_count.saturating_add(1);
                on_discovered(self.discovered_count);
            }
        }
    }

    /// Reads one directory. Only chart files and links are stat-ed: on Unix
    /// `DirEntry::metadata` is an extra syscall per entry, while `file_type` comes
    /// from the directory listing itself.
    fn list_directory(&self, unit: &DirectoryUnit) -> DirectoryListing {
        let root = self.root;
        let dir = unit.path.as_path();
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
        let mut linked_files_in_dir = Vec::new();
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
            if self.skip_hidden && is_hidden_name(&file_name) {
                continue;
            }
            let path = entry.path();
            // Windows/macOS hidden attributes belong to the directory entry itself;
            // do not follow a link before deciding whether to skip it.
            if self.skip_hidden && cfg!(any(windows, target_os = "macos")) {
                match entry.metadata() {
                    Ok(metadata) if is_hidden_metadata(&metadata) => continue,
                    Ok(_) => {}
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
                }
            }
            let file_type = match entry.file_type() {
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

            if file_type.is_symlink() {
                if !self.follow_links {
                    continue;
                }
                let target = match std::fs::metadata(&path) {
                    Ok(target) => target,
                    Err(error) => {
                        listing.link_failed(root, &path, error);
                        continue;
                    }
                };
                if target.is_dir() {
                    if !self.recursive {
                        continue;
                    }
                    match path.canonicalize() {
                        Ok(target) => {
                            listing.linked_dirs.push(LinkedDirectory { link: path, target })
                        }
                        Err(error) => listing.link_failed(root, &path, error),
                    }
                } else if target.is_file() {
                    if is_document_file_name(&file_name) {
                        has_document = true;
                    }
                    let archive = crate::chart_source::is_archive_file(&path);
                    if !archive && !is_chart_file_name(&file_name) {
                        continue;
                    }
                    match path.canonicalize() {
                        Ok(canonical) => linked_files_in_dir.push(LinkedFile {
                            link: path,
                            target: canonical,
                            metadata: target,
                            archive,
                            has_document: false,
                        }),
                        Err(error) => listing.link_failed(root, &path, error),
                    }
                }
                continue;
            }

            if file_type.is_file() && is_document_file_name(&file_name) {
                has_document = true;
            }

            if file_type.is_dir() {
                if !self.recursive {
                    continue;
                }
                let canonical = unit.canonical.as_ref().map(|parent| parent.join(&file_name));
                // Another tree of this scan reads that directory under its own path.
                if canonical.as_ref().is_some_and(|canonical| self.trees.contains(canonical)) {
                    continue;
                }
                listing.subdirs.push(DirectoryUnit { path, canonical });
            } else if file_type.is_file() && crate::chart_source::is_archive_file(&path) {
                if let Some(parent) = &unit.canonical {
                    listing.regular_files.push(parent.join(&file_name));
                }
                match discover_archive_charts(&path, self.skip_hidden) {
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
                let metadata = match entry.metadata() {
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
                if let Some(parent) = &unit.canonical {
                    listing.regular_files.push(parent.join(&file_name));
                }
                charts_in_dir.push(chart_file_entry(path, &metadata, false));
            }
        }
        charts_in_dir.iter_mut().for_each(|entry| entry.has_document = has_document);
        listing.entries.extend(charts_in_dir);
        linked_files_in_dir.iter_mut().for_each(|file| file.has_document = has_document);
        listing.linked_files = linked_files_in_dir;
        listing
    }
}

fn chart_file_entry(
    path: PathBuf,
    metadata: &std::fs::Metadata,
    has_document: bool,
) -> ChartFileEntry {
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ChartFileEntry { path, file_size: metadata.len(), modified_at, has_document }
}

#[derive(Default)]
struct DirectoryListing {
    readable: bool,
    complete: bool,
    entries: Vec<ChartFileEntry>,
    archives: Vec<PathBuf>,
    issues: Vec<ScanDiscoveryIssue>,
    subdirs: Vec<DirectoryUnit>,
    linked_dirs: Vec<LinkedDirectory>,
    linked_files: Vec<LinkedFile>,
    regular_files: Vec<PathBuf>,
}

impl DirectoryListing {
    /// A link whose target no longer exists, or that resolves through itself, can hold
    /// no charts, so it does not make the scan incomplete. Any other failure, such as a
    /// denied target, might hide charts and blocks pruning like an unreadable folder.
    fn link_failed(&mut self, root: &Path, link: &Path, error: io::Error) {
        if let Some(reason) = unresolvable_link_reason(&error) {
            tracing::debug!(
                root = %root.display(),
                link = %link.display(),
                reason,
                error = %error,
                "skipping unresolvable song link"
            );
            return;
        }
        self.complete = false;
        record_discovery_issue(
            &mut self.issues,
            root,
            link,
            ScanDiscoveryOperation::ReadMetadata,
            error,
        );
    }
}

pub(super) fn unresolvable_link_reason(error: &io::Error) -> Option<&'static str> {
    if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) {
        return Some("dangling");
    }
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ELOOP) {
        return Some("loop");
    }
    // ERROR_CANT_RESOLVE_FILENAME: the reparse points form a loop.
    #[cfg(windows)]
    if error.raw_os_error() == Some(1921) {
        return Some("loop");
    }
    None
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
                        || (skip_hidden && is_hidden_name(std::ffi::OsStr::new(part)))
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
    #[cfg(windows)]
    {
        let _ = name;
        false
    }
    #[cfg(not(windows))]
    name.to_str().map(|name| name.starts_with('.')).unwrap_or(false)
}

pub(super) fn is_hidden_metadata(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_HIDDEN;
        metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        metadata.st_flags() & libc::UF_HIDDEN != 0
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = metadata;
        false
    }
}
