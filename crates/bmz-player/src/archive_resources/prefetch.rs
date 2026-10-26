use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, Weak};
use std::time::Duration;

type LoadLocks = Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>;
static LOAD_LOCKS: OnceLock<LoadLocks> = OnceLock::new();
type SpeculativeSets = Mutex<Vec<(Weak<SongResourceSet>, Vec<String>)>>;
static SPECULATIVE: OnceLock<SpeculativeSets> = OnceLock::new();
const MAX_PREFETCH_FOLDER_BYTES: u64 = 256 << 20;

/// Serialize a container's first read. Play joins a pending Select prefetch
/// instead of decompressing the same solid block concurrently.
pub(super) fn with_load_lock<T>(
    container: &Path,
    control: &ArchiveControl,
    load: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let key = std::fs::canonicalize(container)?;
    let lock = {
        let mut locks = LOAD_LOCKS.get_or_init(Mutex::default).lock().unwrap();
        locks.retain(|_, lock| lock.strong_count() > 0);
        match locks.get(&key).and_then(Weak::upgrade) {
            Some(lock) => lock,
            None => {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(key, Arc::downgrade(&lock));
                lock
            }
        }
    };
    let _guard = loop {
        control.check()?;
        match lock.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
        }
    };
    load()
}

pub(super) fn take_speculative(resources: &Arc<SongResourceSet>) -> Vec<String> {
    let mut sets = SPECULATIVE.get_or_init(Mutex::default).lock().unwrap();
    sets.retain(|(set, _)| set.strong_count() > 0);
    let Some(index) = sets.iter().position(|(set, _)| set.ptr_eq(&Arc::downgrade(resources)))
    else {
        return Vec::new();
    };
    sets.swap_remove(index).1
}

/// Prime a solid archive after a stable Select choice. Unused speculative
/// entries remain marked, so the actual chart load still trims them normally.
pub(crate) fn prefetch_chart(
    container: &Path,
    chart: &str,
    control: &ArchiveControl,
) -> Result<bool> {
    prefetch_chart_with_byte_limit(container, chart, control, SET_TRIM_BYTES)
}

pub(super) fn prefetch_chart_with_byte_limit(
    container: &Path,
    chart: &str,
    control: &ArchiveControl,
    byte_limit: u64,
) -> Result<bool> {
    with_load_lock(container, control, || {
        let index = song_archive::inspect(container, control)?;
        if !index.solid {
            return Ok(false);
        }
        // Hovering must not allocate a multi-gigabyte song folder. Larger
        // folders retain the ordinary on-demand path and its existing limits.
        let folder = chart.rsplit_once('/').map_or("", |(folder, _)| folder);
        let bytes = index
            .entries
            .iter()
            .filter(|entry| in_folder(&entry.name, folder))
            .map(|entry| entry.size)
            .fold(0u64, u64::saturating_add);
        if bytes > MAX_PREFETCH_FOLDER_BYTES {
            return Ok(false);
        }
        let resources = resources_for_index(container, &index);
        // A cached chart makes open_indexed_chart reuse the set without adding
        // its folder again, including when an actual Play exceeded this budget.
        let additional_bytes = if resources.loaded(chart).is_some() {
            0
        } else {
            let missing: HashSet<String> = resources
                .unloaded(
                    index
                        .entries
                        .iter()
                        .filter(|entry| !entry.is_directory && in_folder(&entry.name, folder))
                        .map(|entry| entry.name.as_str()),
                )
                .into_iter()
                .collect();
            index
                .entries
                .iter()
                .filter(|entry| missing.contains(&entry.name))
                .map(|entry| entry.size)
                .fold(0u64, u64::saturating_add)
        };
        let loaded_bytes = resources.loaded_bytes();
        if additional_bytes > 0 && loaded_bytes.saturating_add(additional_bytes) > byte_limit {
            tracing::debug!(
                path = %container.display(),
                chart,
                loaded_bytes,
                additional_bytes,
                byte_limit,
                "selected archive prefetch exceeds resource budget"
            );
            return Ok(false);
        }
        let opened = open_indexed_chart(chart, control, &index, resources)?;
        if !opened.speculative.is_empty() {
            let mut sets = SPECULATIVE.get_or_init(Mutex::default).lock().unwrap();
            sets.retain(|(set, _)| set.strong_count() > 0);
            sets.push((Arc::downgrade(&opened.resources), opened.speculative));
        }
        Ok(true)
    })
}
