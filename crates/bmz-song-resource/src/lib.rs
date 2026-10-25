//! Song files read from archives into memory, found by their `pack.zip!/entry` path.
//!
//! Charts inside ZIP / RAR / 7z keep virtual asset paths such as
//! `C:/songs/pack.zip!/song/kick.wav`. The app reads the entries a chart needs into a
//! [`SongResourceSet`] and [`install`]s it; chart import, audio, image and video loaders ask
//! [`exists`] / [`read`] before touching the file system, so no archive is extracted to disk.
//! Ordinary paths are not handled here and callers keep their own file system access.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};

/// Archive formats whose entries can appear in a virtual path.
pub const ARCHIVE_EXTENSIONS: &[&str] = &["zip", "rar", "7z"];

/// Sets kept for retry, practice and the Select screen after their chart's load finished.
/// A play holds its own set, so eviction never takes one a running play still reads.
const RECENT_SET_LIMIT: usize = 3;
/// Loaded bytes kept across recent sets. The newest set is kept even when it is larger.
const RECENT_BYTES_LIMIT: u64 = 1 << 30;
/// Index-only sets made by the fallback for archives no load has installed.
const FALLBACK_SET_LIMIT: usize = 2;

/// How a path relates to song archives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongPath {
    /// Not inside an archive; read it from the file system.
    File,
    /// An entry inside `container`, normalized with `/` separators.
    Archive { container: PathBuf, entry: String },
    /// Inside an archive but escaping its root or otherwise unusable. Never fall back to the
    /// file system for it: `pack.zip!/../x` would name a file outside the archive.
    Invalid,
}

impl SongPath {
    pub fn parse(path: &Path) -> Self {
        let text = normalize_separators(&path.to_string_lossy());
        let lower = text.to_ascii_lowercase();
        for (index, _) in text.match_indices('!') {
            let rest = &text[index + 1..];
            if !rest.is_empty() && !rest.starts_with('/') {
                continue;
            }
            let is_archive = ARCHIVE_EXTENSIONS
                .iter()
                .any(|extension| lower[..index].ends_with(&format!(".{extension}")));
            if !is_archive {
                continue;
            }
            return match normalize_entry(rest) {
                Some(entry) => Self::Archive { container: PathBuf::from(&text[..index]), entry },
                None => Self::Invalid,
            };
        }
        Self::File
    }
}

/// `\` to `/`, without Windows verbatim prefixes, as the library stores paths.
fn normalize_separators(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let Some(rest) = normalized.strip_prefix("//?/") else {
        return normalized;
    };
    if rest.get(..4).is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC/")) {
        format!("//{}", &rest[4..])
    } else {
        rest.to_owned()
    }
}

/// Resolves `.` and `..` inside the archive. `None` when the entry leaves the root.
fn normalize_entry(entry: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in entry.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part if part.contains(':') || part.contains('\0') => return None,
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Registry key for a container. Windows file names ignore ASCII case.
fn container_key(container: &Path) -> String {
    let key = normalize_separators(&container.to_string_lossy());
    if cfg!(windows) { key.to_ascii_lowercase() } else { key }
}

type Fetch = dyn Fn(&[String]) -> io::Result<Vec<(String, Vec<u8>)>> + Send + Sync;

/// The entry names of one archive generation and the entries read into memory.
pub struct SongResourceSet {
    container: PathBuf,
    generation: String,
    exact: HashSet<Arc<str>>,
    folded: HashMap<String, Arc<str>>,
    files: RwLock<HashMap<Arc<str>, Arc<[u8]>>>,
    fetch: Option<Box<Fetch>>,
}

impl std::fmt::Debug for SongResourceSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SongResourceSet")
            .field("container", &self.container)
            .field("generation", &self.generation)
            .field("entries", &self.exact.len())
            .field("loaded_bytes", &self.loaded_bytes())
            .finish()
    }
}

impl SongResourceSet {
    /// `names` lists every file entry; loading happens separately.
    pub fn new(
        container: impl Into<PathBuf>,
        generation: impl Into<String>,
        names: impl IntoIterator<Item = String>,
    ) -> Self {
        let exact: HashSet<Arc<str>> = names.into_iter().map(Arc::from).collect();
        let folded = exact.iter().map(|name| (fold(name), name.clone())).collect();
        Self {
            container: container.into(),
            generation: generation.into(),
            exact,
            folded,
            files: RwLock::new(HashMap::new()),
            fetch: None,
        }
    }

    /// Reads entries that were not loaded up front, e.g. after a chart asked for a file the
    /// preload did not predict. It receives actual entry names.
    pub fn with_fetch(
        mut self,
        fetch: impl Fn(&[String]) -> io::Result<Vec<(String, Vec<u8>)>> + Send + Sync + 'static,
    ) -> Self {
        self.fetch = Some(Box::new(fetch));
        self
    }

    pub fn container(&self) -> &Path {
        &self.container
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    /// The archive's own name for `entry`. BMS authors rely on case-insensitive file
    /// systems, so an exact match wins and a case-folded match follows.
    pub fn entry_name(&self, entry: &str) -> Option<&str> {
        self.exact.get(entry).or_else(|| self.folded.get(&fold(entry))).map(|name| &**name)
    }

    pub fn contains(&self, entry: &str) -> bool {
        self.entry_name(entry).is_some()
    }

    pub fn insert(&self, entry: &str, bytes: Vec<u8>) {
        if let Some(name) = self.exact.get(entry).cloned() {
            self.files.write().unwrap_or_else(|e| e.into_inner()).insert(name, bytes.into());
        }
    }

    /// Bytes already in memory, without fetching.
    pub fn loaded(&self, entry: &str) -> Option<Arc<[u8]>> {
        let name = self.entry_name(entry)?;
        self.files.read().unwrap_or_else(|e| e.into_inner()).get(name).cloned()
    }

    /// Entries among `entries` that exist in the archive but are not in memory yet.
    pub fn unloaded<'a>(&self, entries: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let files = self.files.read().unwrap_or_else(|e| e.into_inner());
        let mut missing: Vec<String> = entries
            .into_iter()
            .filter_map(|entry| self.entry_name(entry))
            .filter(|name| !files.contains_key(*name))
            .map(str::to_owned)
            .collect();
        missing.sort();
        missing.dedup();
        missing
    }

    /// Keeps only the loaded entries `keep` accepts, to free what a chart turned out not to use.
    pub fn retain_loaded(&self, mut keep: impl FnMut(&str) -> bool) {
        self.files.write().unwrap_or_else(|e| e.into_inner()).retain(|name, _| keep(name));
    }

    pub fn loaded_bytes(&self) -> u64 {
        self.files
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|bytes| bytes.len() as u64)
            .sum()
    }

    pub fn loaded_count(&self) -> usize {
        self.files.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Bytes of `entry`, reading it through the fetch callback when it is not loaded.
    pub fn read(&self, entry: &str) -> io::Result<Arc<[u8]>> {
        let name = self.entry_name(entry).ok_or_else(|| not_found(&self.container, entry))?;
        if let Some(bytes) = self.files.read().unwrap_or_else(|e| e.into_inner()).get(name) {
            return Ok(bytes.clone());
        }
        let fetch = self.fetch.as_ref().ok_or_else(|| not_found(&self.container, entry))?;
        for (fetched, bytes) in fetch(&[name.to_owned()])? {
            self.insert(&fetched, bytes);
        }
        self.files
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
            .ok_or_else(|| not_found(&self.container, entry))
    }
}

fn fold(name: &str) -> String {
    name.to_lowercase()
}

fn not_found(container: &Path, entry: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("song archive entry not found: {}!/{entry}", container.display()),
    )
}

type Fallback = dyn Fn(&Path) -> io::Result<Arc<SongResourceSet>> + Send + Sync;

#[derive(Default)]
struct Registry {
    recent: VecDeque<(String, Arc<SongResourceSet>)>,
    /// Every installed set, found again while an owner such as a running play holds it.
    held: Vec<(String, Weak<SongResourceSet>)>,
    fallback_sets: VecDeque<(String, Arc<SongResourceSet>)>,
    fallback: Option<Arc<Fallback>>,
}

static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);

fn with_registry<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    let mut guard = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    f(guard.get_or_insert_with(Registry::default))
}

/// Makes `set` the newest set for its container, replacing an older generation.
pub fn install(set: Arc<SongResourceSet>) {
    let key = container_key(&set.container);
    with_registry(|registry| {
        registry.recent.retain(|(other, _)| *other != key);
        registry.fallback_sets.retain(|(other, _)| *other != key);
        registry.held.retain(|(other, held)| *other != key && held.strong_count() > 0);
        registry.held.push((key.clone(), Arc::downgrade(&set)));
        registry.recent.push_front((key, set));
        let mut bytes = 0;
        let mut kept = 0;
        registry.recent.retain(|(_, set)| {
            bytes += set.loaded_bytes();
            kept += 1;
            kept == 1 || (kept <= RECENT_SET_LIMIT && bytes <= RECENT_BYTES_LIMIT)
        });
    });
}

/// The installed set for `container`, while the registry or an owner still holds it.
pub fn find(container: &Path) -> Option<Arc<SongResourceSet>> {
    let key = container_key(container);
    with_registry(|registry| registry.installed(&key))
}

impl Registry {
    fn installed(&self, key: &str) -> Option<Arc<SongResourceSet>> {
        self.recent.iter().find(|(other, _)| other == key).map(|(_, set)| set.clone()).or_else(
            || self.held.iter().find(|(other, _)| other == key).and_then(|(_, set)| set.upgrade()),
        )
    }
}

/// Builds a set for an archive no load installed, e.g. after eviction. Reads through it
/// fetch entries one at a time, which is correct but slow for solid archives.
pub fn set_fallback(
    fallback: impl Fn(&Path) -> io::Result<Arc<SongResourceSet>> + Send + Sync + 'static,
) {
    with_registry(|registry| registry.fallback = Some(Arc::new(fallback)));
}

/// Drops the registry's own references, as eviction would. Sets an owner such as a running
/// play still holds stay reachable, and the fallback stays.
pub fn clear() {
    with_registry(|registry| {
        registry.recent.clear();
        registry.held.retain(|(_, held)| held.strong_count() > 0);
        registry.fallback_sets.clear();
    });
}

fn resolve(container: &Path) -> io::Result<Arc<SongResourceSet>> {
    let key = container_key(container);
    let fallback = with_registry(|registry| {
        let found = registry.installed(&key).or_else(|| {
            registry
                .fallback_sets
                .iter()
                .find(|(other, _)| *other == key)
                .map(|(_, set)| set.clone())
        });
        match found {
            Some(set) => Ok(set),
            None => Err(registry.fallback.clone()),
        }
    });
    let fallback = match fallback {
        Ok(set) => return Ok(set),
        Err(fallback) => fallback,
    };
    let fallback = fallback.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("song archive is not loaded: {}", container.display()),
        )
    })?;
    // The fallback reads the archive index; never hold the registry lock meanwhile.
    let set = fallback(container)?;
    with_registry(|registry| {
        registry.fallback_sets.retain(|(other, _)| *other != key);
        registry.fallback_sets.push_front((key, set.clone()));
        registry.fallback_sets.truncate(FALLBACK_SET_LIMIT);
    });
    Ok(set)
}

/// Whether `path` names an existing archive entry. `None` for ordinary paths.
pub fn exists(path: &Path) -> Option<bool> {
    match SongPath::parse(path) {
        SongPath::File => None,
        SongPath::Invalid => Some(false),
        SongPath::Archive { container, entry } => {
            Some(resolve(&container).is_ok_and(|set| set.contains(&entry)))
        }
    }
}

/// Bytes of an archive entry. `None` for ordinary paths.
pub fn read(path: &Path) -> Option<io::Result<Arc<[u8]>>> {
    match SongPath::parse(path) {
        SongPath::File => None,
        SongPath::Invalid => Some(Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("song asset escapes its archive: {}", path.display()),
        ))),
        SongPath::Archive { container, entry } => {
            Some(resolve(&container).and_then(|set| set.read(&entry)))
        }
    }
}

/// [`exists`] for archive entries, the file system otherwise.
pub fn file_exists(path: &Path) -> bool {
    exists(path).unwrap_or_else(|| path.exists())
}

/// [`exists`] for archive entries, which are always files; `Path::is_file` otherwise.
pub fn is_file(path: &Path) -> bool {
    exists(path).unwrap_or_else(|| path.is_file())
}

/// [`read`] for archive entries, the file system otherwise.
pub fn read_file(path: &Path) -> io::Result<Arc<[u8]>> {
    read(path).unwrap_or_else(|| std::fs::read(path).map(Arc::from))
}

/// Size for diagnostics: loaded archive bytes, or file metadata.
pub fn file_len(path: &Path) -> Option<u64> {
    match SongPath::parse(path) {
        SongPath::File => std::fs::metadata(path).ok().map(|metadata| metadata.len()),
        SongPath::Invalid => None,
        SongPath::Archive { container, entry } => {
            find(&container)?.loaded(&entry).map(|bytes| bytes.len() as u64)
        }
    }
}

#[cfg(test)]
mod tests;
