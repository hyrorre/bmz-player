//! Read-only song archives. Callers retain the archive locator; extracted paths are disposable.

mod cache;
mod formats;
mod safety;

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::SystemTime;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use cache::materialize;
use formats::Backend;
use safety::{MeteredWriter, validate_entries};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub compressed_bytes: u64,
    pub extracted_bytes: u64,
    pub file_bytes: u64,
    pub chart_bytes: u64,
    pub header_bytes: u64,
    pub entries: usize,
    pub dictionary_bytes: u64,
    pub reader_workspace_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            compressed_bytes: 8 << 30,
            extracted_bytes: 16 << 30,
            file_bytes: 4 << 30,
            chart_bytes: 64 << 20,
            header_bytes: 64 << 20,
            entries: 100_000,
            dictionary_bytes: 256 << 20,
            reader_workspace_bytes: 512 << 20,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveControl {
    pub limits: ArchiveLimits,
    pub cancelled: Option<Arc<AtomicBool>>,
}

impl ArchiveControl {
    pub fn check(&self) -> Result<()> {
        ensure!(
            !self.cancelled.as_ref().is_some_and(|flag| flag.load(Ordering::Relaxed)),
            "archive operation cancelled"
        );
        Ok(())
    }
}

/// A cheap invalidation token, used with the caller's container path, never as content identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArchiveStamp {
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    /// Unix inode and status-change time; unlike mtime, ctime cannot be set back by tools.
    pub changed: Option<(u64, i64)>,
}

impl ArchiveStamp {
    /// Stable text form for persisting a stamp alongside the generation it was hashed at.
    pub fn token(&self) -> String {
        fn time(value: Option<SystemTime>) -> String {
            match value.map(|time| time.duration_since(SystemTime::UNIX_EPOCH)) {
                Some(Ok(after)) => after.as_nanos().to_string(),
                Some(Err(before)) => format!("-{}", before.duration().as_nanos()),
                None => "_".to_string(),
            }
        }
        let changed =
            self.changed.map_or("_".to_string(), |(inode, ctime)| format!("{inode}.{ctime}"));
        format!("v1:{}:{}:{}:{changed}", self.size, time(self.modified), time(self.created))
    }
}

pub fn metadata_stamp(path: &Path) -> Result<ArchiveStamp> {
    stamp(&std::fs::metadata(path)?)
}

fn stamp(metadata: &std::fs::Metadata) -> Result<ArchiveStamp> {
    ensure!(metadata.is_file(), "archive is not a regular file");
    Ok(ArchiveStamp {
        size: metadata.len(),
        modified: metadata.modified().ok(),
        created: metadata.created().ok(),
        changed: changed(metadata),
    })
}

#[cfg(unix)]
fn changed(metadata: &std::fs::Metadata) -> Option<(u64, i64)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.ino(), metadata.ctime().saturating_mul(1_000_000_000) + metadata.ctime_nsec()))
}

#[cfg(not(unix))]
fn changed(_: &std::fs::Metadata) -> Option<(u64, i64)> {
    None
}

/// A whole-archive hash remembered for the stamp it was computed at, so unchanged
/// archives are not re-read on every play, preview or process start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationRecord {
    pub path: PathBuf,
    pub stamp: String,
    pub generation: ArchiveGeneration,
}

/// The remembered generation for `path`, if its stamp still matches the file.
pub fn generation_record(path: &Path) -> Option<GenerationRecord> {
    let path = std::fs::canonicalize(path).ok()?;
    let stamp = metadata_stamp(&path).ok()?.token();
    let generation = cache::known_generation(&path, &stamp)?;
    Some(GenerationRecord { path, stamp, generation })
}

/// Seed a persisted record. A stale record is harmless: lookups require an equal stamp.
pub fn remember_generation(record: GenerationRecord) {
    cache::remember_generation(record.path, record.stamp, record.generation);
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArchiveGeneration {
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub is_directory: bool,
}

impl ArchiveEntry {
    pub fn is_chart(&self) -> bool {
        !self.is_directory
            && !self
                .name
                .split('/')
                .any(|component| component == "__MACOSX" || component.starts_with("._"))
            && Path::new(&self.name).extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
                ["bms", "bme", "bml", "pms", "bmson"]
                    .iter()
                    .any(|candidate| ext.eq_ignore_ascii_case(candidate))
            })
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveIndex {
    pub generation: ArchiveGeneration,
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Debug)]
pub struct ArchiveChart {
    pub bytes: Vec<u8>,
    pub generation: ArchiveGeneration,
}

#[derive(Debug, Clone)]
pub struct MaterializedArchive {
    pub root: PathBuf,
    pub generation: ArchiveGeneration,
}

pub fn is_archive(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
        ["zip", "rar", "7z"].iter().any(|candidate| ext.eq_ignore_ascii_case(candidate))
    })
}

pub fn inspect(path: &Path, control: &ArchiveControl) -> Result<ArchiveIndex> {
    control.check()?;
    let path = std::fs::canonicalize(path)?;
    let current = metadata_stamp(&path)?;
    if let Some(index) = cache::cached_index(&path, &current, &control.limits) {
        return Ok(index);
    }
    let opened = Opened::new(&path, control)?;
    cache::remember_index(path, opened.stamp, control.limits.clone(), opened.index.clone());
    Ok(opened.index)
}

pub fn scan_charts(
    path: &Path,
    control: &ArchiveControl,
    mut on_chart: impl FnMut(&ArchiveEntry, &[u8]) -> Result<()>,
) -> Result<ArchiveIndex> {
    let mut opened = Opened::new(path, control)?;
    let pending = Arc::new(Mutex::new(None::<(ArchiveEntry, Vec<u8>)>));
    let flush = |pending: &Arc<Mutex<Option<(ArchiveEntry, Vec<u8>)>>>,
                 visitor: &mut dyn FnMut(&ArchiveEntry, &[u8]) -> Result<()>|
     -> Result<()> {
        if let Some((entry, bytes)) = pending.lock().unwrap().take() {
            visitor(&entry, &bytes)?;
        }
        Ok(())
    };
    opened.extract(control, Selection::Charts, &mut |entry| {
        flush(&pending, &mut on_chart)?;
        *pending.lock().unwrap() = Some((entry.clone(), Vec::new()));
        Ok(Box::new(ChartWriter(pending.clone())))
    })?;
    opened.verify_unchanged(path, control)?;
    flush(&pending, &mut on_chart)?;
    let path = std::fs::canonicalize(path)?;
    cache::remember_index(path, opened.stamp, control.limits.clone(), opened.index.clone());
    Ok(opened.index)
}

pub fn read_chart(path: &Path, entry: &str, control: &ArchiveControl) -> Result<ArchiveChart> {
    let mut opened = Opened::new(path, control)?;
    let selected = opened
        .index
        .entries
        .iter()
        .find(|candidate| candidate.name == entry)
        .context("archive chart entry not found")?;
    ensure!(selected.is_chart(), "archive entry is not a chart");
    let pending = Arc::new(Mutex::new(Some((selected.clone(), Vec::new()))));
    opened.extract(control, Selection::One(entry), &mut |_| {
        Ok(Box::new(ChartWriter(pending.clone())))
    })?;
    opened.verify_unchanged(path, control)?;
    let (_, bytes) = pending.lock().unwrap().take().context("archive chart was not read")?;
    Ok(ArchiveChart { bytes, generation: opened.index.generation })
}

type PendingChart = Arc<Mutex<Option<(ArchiveEntry, Vec<u8>)>>>;
struct ChartWriter(PendingChart);
impl Write for ChartWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().as_mut().unwrap().1.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Selection<'a> {
    All,
    Charts,
    One(&'a str),
}
impl Selection<'_> {
    fn includes(self, entry: &ArchiveEntry) -> bool {
        !entry.is_directory
            && match self {
                Self::All => true,
                Self::Charts => entry.is_chart(),
                Self::One(name) => entry.name == name,
            }
    }
}

pub(super) struct Opened {
    backend: Backend,
    index: ArchiveIndex,
    stamp: ArchiveStamp,
}

impl Opened {
    fn new(path: &Path, control: &ArchiveControl) -> Result<Self> {
        control.check()?;
        ensure!(is_archive(path), "unsupported song archive extension");
        let canonical = std::fs::canonicalize(path)?;
        let mut file = CheckedFile { file: std::fs::File::open(path)?, control: control.clone() };
        let before = stamp(&file.file.metadata()?)?;
        ensure!(
            before.size <= control.limits.compressed_bytes,
            "archive compressed bytes exceed limit"
        );
        let token = before.token();
        let generation = match cache::known_generation(&canonical, &token) {
            Some(generation) => generation,
            None => {
                let generation = fingerprint(&mut file)?;
                file.seek(SeekFrom::Start(0))?;
                generation
            }
        };
        let (backend, entries) = Backend::open(file, control)?;
        validate_entries(&entries, &control.limits)?;
        ensure!(metadata_stamp(path)? == before, "archive changed while reading index");
        cache::remember_generation(canonical, token, generation.clone());
        Ok(Self { backend, index: ArchiveIndex { generation, entries }, stamp: before })
    }

    fn extract(
        &mut self,
        control: &ArchiveControl,
        selected: Selection<'_>,
        open: &mut dyn FnMut(&ArchiveEntry) -> Result<Box<dyn Write>>,
    ) -> Result<()> {
        let meters = Arc::new(Mutex::new(safety::Meters::default()));
        self.backend.extract(&self.index.entries, control, selected, &mut |entry| {
            let writer =
                if selected.includes(entry) { open(entry)? } else { Box::new(std::io::sink()) };
            Ok(Box::new(MeteredWriter::new(writer, entry, control.clone(), meters.clone())))
        })?;
        control.check()?;
        ensure!(meters.lock().unwrap().complete, "archive entry ended before declared size");
        Ok(())
    }

    fn verify_unchanged(&self, path: &Path, control: &ArchiveControl) -> Result<()> {
        control.check()?;
        ensure!(metadata_stamp(path)? == self.stamp, "archive changed during reading");
        Ok(())
    }
}

fn fingerprint(reader: &mut impl Read) -> Result<ArchiveGeneration> {
    let mut digest = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    loop {
        let len = reader.read(&mut bytes)?;
        if len == 0 {
            break;
        }
        digest.update(&bytes[..len]);
    }
    Ok(ArchiveGeneration { fingerprint: format!("v1-{:x}", digest.finalize()) })
}

struct CheckedFile {
    file: std::fs::File,
    control: ArchiveControl,
}
impl Read for CheckedFile {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.control.check().map_err(std::io::Error::other)?;
        self.file.read(bytes)
    }
}
impl Seek for CheckedFile {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.control.check().map_err(std::io::Error::other)?;
        self.file.seek(from)
    }
}

#[cfg(test)]
mod tests;
