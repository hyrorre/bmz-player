use super::*;
use std::collections::{HashMap, VecDeque};
use std::sync::{OnceLock, Weak, atomic::AtomicU64};

struct CachedIndex {
    path: PathBuf,
    stamp: ArchiveStamp,
    limits: ArchiveLimits,
    index: ArchiveIndex,
}

static INDEX: OnceLock<Mutex<VecDeque<CachedIndex>>> = OnceLock::new();

pub(super) fn cached_index(
    path: &Path,
    stamp: &ArchiveStamp,
    limits: &ArchiveLimits,
) -> Option<ArchiveIndex> {
    INDEX
        .get_or_init(Mutex::default)
        .lock()
        .unwrap()
        .iter()
        .find(|cached| cached.path == path && cached.stamp == *stamp && cached.limits == *limits)
        .map(|cached| cached.index.clone())
}

pub(super) fn remember_index(
    path: PathBuf,
    stamp: ArchiveStamp,
    limits: ArchiveLimits,
    index: ArchiveIndex,
) {
    let mut indexes = INDEX.get_or_init(Mutex::default).lock().unwrap();
    indexes.retain(|cached| cached.path != path);
    indexes.push_back(CachedIndex { path, stamp, limits, index });
    while indexes.len() > 32
        || indexes.iter().map(|cached| cached.index.entries.len()).sum::<usize>() > 100_000
    {
        indexes.pop_front();
    }
}

type Generations = Mutex<HashMap<PathBuf, (String, ArchiveGeneration)>>;
static GENERATIONS: OnceLock<Generations> = OnceLock::new();
const MAX_GENERATIONS: usize = 1 << 16;

pub(super) fn known_generation(path: &Path, stamp: &str) -> Option<ArchiveGeneration> {
    let generations = GENERATIONS.get_or_init(Mutex::default).lock().unwrap();
    generations
        .get(path)
        .filter(|(known, _)| known == stamp)
        .map(|(_, generation)| generation.clone())
}

pub(super) fn remember_generation(path: PathBuf, stamp: String, generation: ArchiveGeneration) {
    let mut generations = GENERATIONS.get_or_init(Mutex::default).lock().unwrap();
    if generations.len() >= MAX_GENERATIONS && !generations.contains_key(&path) {
        // Entries are tiny; a reset only costs one rehash per archive touched afterwards.
        generations.clear();
    }
    generations.insert(path, (stamp, generation));
}

fn forget_generation(path: &Path) {
    GENERATIONS.get_or_init(Mutex::default).lock().unwrap().remove(path);
    INDEX.get_or_init(Mutex::default).lock().unwrap().retain(|cached| cached.path != path);
}

type MaterializeLocks = Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>;
static LOCKS: OnceLock<MaterializeLocks> = OnceLock::new();
static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);

/// Publish a complete immutable generation; no user cache is chosen or pruned here.
pub fn materialize(
    path: &Path,
    expected: &ArchiveGeneration,
    cache_root: &Path,
    control: &ArchiveControl,
) -> Result<MaterializedArchive> {
    control.check()?;
    let path = std::fs::canonicalize(path)?;
    let index = inspect(&path, control)?;
    ensure!(index.generation == *expected, "song archive generation changed");
    std::fs::create_dir_all(cache_root)?;
    let cache_root = std::fs::canonicalize(cache_root)?;
    let archives = directory(&cache_root, "song-archives")?;
    // Keep identical archives at distinct source locations independent.
    let source_key = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
    let source_root = directory(&archives, &source_key)?;
    let target = source_root.join(&index.generation.fingerprint);
    let lock = target_lock(&target);
    let _guard = acquire(&lock, control)?;
    ensure!(
        inspect(&path, control)?.generation == *expected,
        "song archive generation changed while waiting"
    );
    if complete(&target, expected, &index.entries, control)? {
        return Ok(MaterializedArchive {
            root: target.join("content"),
            generation: expected.clone(),
        });
    }
    if target.try_exists()? {
        // A damaged private cache is recoverable. Keep it outside published generations;
        // never follow links or remove a generation that a running session may still use.
        plain_directory(&target)?;
        let quarantine = loop {
            let candidate = source_root.join(format!(
                ".invalid-{}-{}",
                std::process::id(),
                NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
            ));
            if cache_metadata(&candidate)?.is_none() {
                break candidate;
            }
        };
        std::fs::rename(&target, quarantine)?;
    }
    let stage = loop {
        let stage = source_root.join(format!(
            ".stage-{}-{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&stage) {
            Ok(()) => break Stage(stage),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let content = directory(&stage.0, "content")?;
    let mut opened = Opened::new(&path, control)?;
    ensure!(
        opened.index.generation == *expected,
        "song archive generation changed before extraction"
    );
    for entry in &opened.index.entries {
        if entry.is_directory {
            entry_directory(&content, &entry.name)?;
        }
    }
    opened.extract(control, Selection::All, &mut |entry| {
        let target = content.join(&entry.name);
        if let Some((parent, _)) = entry.name.rsplit_once('/') {
            entry_directory(&content, parent)?;
        }
        Ok(Box::new(std::fs::OpenOptions::new().write(true).create_new(true).open(target)?))
    })?;
    opened.verify_unchanged(&path, control)?;
    // Rehash before publication: a source rewrite must never publish mixed-generation bytes.
    let mut verification =
        CheckedFile { file: std::fs::File::open(&path)?, control: control.clone() };
    if fingerprint(&mut verification)? != *expected {
        // The remembered generation no longer describes these bytes; hash afresh next time.
        forget_generation(&path);
        anyhow::bail!("song archive changed during extraction");
    }
    let marker_path = stage.0.join("complete.json");
    let mut marker = std::fs::OpenOptions::new().create_new(true).write(true).open(marker_path)?;
    serde_json::to_writer(&mut marker, expected)?;
    marker.sync_all()?;
    drop(marker);
    control.check()?;
    match std::fs::rename(&stage.0, &target) {
        Ok(()) => {}
        Err(_) if complete(&target, expected, &index.entries, control)? => {} // Another process published the same generation.
        Err(error) => return Err(error.into()),
    }
    Ok(MaterializedArchive { root: target.join("content"), generation: expected.clone() })
}

fn target_lock(target: &Path) -> Arc<Mutex<()>> {
    let mut locks = LOCKS.get_or_init(Mutex::default).lock().unwrap();
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(target).and_then(Weak::upgrade) {
        lock
    } else {
        let lock = Arc::new(Mutex::new(()));
        locks.insert(target.to_owned(), Arc::downgrade(&lock));
        lock
    }
}

fn acquire<'a>(
    lock: &'a Mutex<()>,
    control: &ArchiveControl,
) -> Result<std::sync::MutexGuard<'a, ()>> {
    loop {
        control.check()?;
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(20))
            }
            Err(error) => anyhow::bail!("archive cache lock poisoned: {error}"),
        }
    }
}

/// Publish one entry for Select images and previews without expanding the archive.
/// Only the current generation of each source is kept; play caches are separate.
pub fn materialize_entry(
    path: &Path,
    expected: &ArchiveGeneration,
    entry: &str,
    cache_root: &Path,
    control: &ArchiveControl,
) -> Result<PathBuf> {
    control.check()?;
    let path = std::fs::canonicalize(path)?;
    let index = inspect(&path, control)?;
    ensure!(index.generation == *expected, "song archive generation changed");
    let selected = index
        .entries
        .iter()
        .find(|candidate| !candidate.is_directory && candidate.name == entry)
        .context("archive asset entry not found")?
        .clone();
    std::fs::create_dir_all(cache_root)?;
    let cache_root = std::fs::canonicalize(cache_root)?;
    let assets = directory(&cache_root, "song-archive-assets")?;
    let source_key = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
    let source_root = directory(&assets, &source_key)?;
    let generation_root = directory(&source_root, &expected.fingerprint)?;
    prune_other_generations(&source_root, &expected.fingerprint);
    let target = generation_root.join(&selected.name);
    let lock = target_lock(&target);
    let _guard = acquire(&lock, control)?;
    let parent = match selected.name.rsplit_once('/') {
        Some((parent, _)) => {
            entry_directory(&generation_root, parent)?;
            generation_root.join(parent)
        }
        None => generation_root.clone(),
    };
    if let Some(metadata) = cache_metadata(&target)? {
        ensure!(!is_link(&metadata), "archive cache contains a link");
        if metadata.is_file() && metadata.len() == selected.size {
            return Ok(target);
        }
    }
    let stage = loop {
        let stage = parent.join(format!(
            ".stage-{}-{}",
            std::process::id(),
            NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&stage) {
            Ok(file) => break (StageFile(stage), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let (mut stage, file) = stage;
    let mut opened = Opened::new(&path, control)?;
    ensure!(
        opened.index.generation == *expected,
        "song archive generation changed before extraction"
    );
    let mut file = Some(file);
    opened.extract(control, Selection::One(&selected.name), &mut |_| {
        Ok(Box::new(file.take().context("archive asset selected twice")?))
    })?;
    opened.verify_unchanged(&path, control)?;
    if cache_metadata(&target)?.is_some_and(|metadata| metadata.is_file() && !is_link(&metadata)) {
        // A previous damaged copy; regular files are replaced, links are never followed.
        std::fs::remove_file(&target)?;
    }
    std::fs::rename(&stage.0, &target)?;
    stage.0 = PathBuf::new();
    Ok(target)
}

/// Best effort: an image or preview loader may still hold an older file open.
fn prune_other_generations(source_root: &Path, keep: &str) {
    let Ok(children) = std::fs::read_dir(source_root) else {
        return;
    };
    for child in children.flatten() {
        let name = child.file_name();
        if name.to_str() == Some(keep) {
            continue;
        }
        if child.file_type().is_ok_and(|kind| kind.is_dir()) {
            let _ = std::fs::remove_dir_all(child.path());
        }
    }
}

struct StageFile(PathBuf);
impl Drop for StageFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn complete(
    path: &Path,
    expected: &ArchiveGeneration,
    entries: &[ArchiveEntry],
    control: &ArchiveControl,
) -> Result<bool> {
    if !path.try_exists()? {
        return Ok(false);
    }
    plain_directory(path)?;
    let content = path.join("content");
    let marker = path.join("complete.json");
    let content_metadata = cache_metadata(&content)?;
    if let Some(metadata) = &content_metadata {
        ensure!(!is_link(metadata), "unsafe archive cache directory link");
    }
    let marker_metadata = cache_metadata(&marker)?;
    if let Some(metadata) = &marker_metadata {
        ensure!(!is_link(metadata), "unsafe archive cache marker link");
    }
    let mut valid =
        if marker_metadata.is_some_and(|metadata| metadata.is_file() && metadata.len() <= 1024) {
            serde_json::from_reader::<_, ArchiveGeneration>(std::fs::File::open(marker)?)
                .is_ok_and(|actual| actual == *expected)
        } else {
            false
        };
    if !content_metadata.is_some_and(|metadata| metadata.is_dir()) {
        return Ok(false);
    }
    let mut directories = std::collections::HashSet::new();
    for entry in entries {
        control.check()?;
        let target = content.join(&entry.name);
        let mut parent = content.clone();
        let mut parents_exist = true;
        if let Some((prefix, _)) = entry.name.rsplit_once('/') {
            for component in prefix.split('/') {
                parent.push(component);
                if directories.contains(&parent) {
                    continue;
                }
                let Some(metadata) = cache_metadata(&parent)? else {
                    parents_exist = false;
                    break;
                };
                ensure!(!is_link(&metadata), "archive cache contains a link");
                if !metadata.is_dir() {
                    parents_exist = false;
                    break;
                }
                directories.insert(parent.clone());
            }
        }
        if !parents_exist {
            valid = false;
            continue;
        }
        let Some(metadata) = cache_metadata(&target)? else {
            valid = false;
            continue;
        };
        ensure!(!is_link(&metadata), "archive cache contains a link");
        if entry.is_directory {
            if !metadata.is_dir() {
                valid = false;
            }
        } else if !metadata.is_file() || metadata.len() != entry.size {
            valid = false;
        }
    }
    Ok(valid)
}

fn cache_metadata(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn directory(parent: &Path, name: &str) -> Result<PathBuf> {
    let child = parent.join(name);
    match std::fs::create_dir(&child) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    plain_directory(&child)?;
    Ok(child)
}

fn entry_directory(root: &Path, name: &str) -> Result<()> {
    let mut current = root.to_owned();
    for component in name.split('/') {
        current = directory(&current, component)?;
    }
    Ok(())
}

fn plain_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(metadata.is_dir() && !is_link(&metadata), "unsafe archive cache directory");
    Ok(())
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        // This unique directory is only created inside the explicitly supplied cache root.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
