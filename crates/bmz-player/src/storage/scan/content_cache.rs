use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bmz_chart::import::error::ImportWarning;
use bmz_chart::import::{
    AssetLookup, BmsRandomSource, chart_source_format, import_chart_bytes_with_assets,
};
use bmz_chart::model::ChartSourceFormat;
use sha2::{Digest, Sha256};

use super::*;
use crate::storage::library_db::{ChartImportAnalysis, PreparedChartImport};

// Keep scan data bounded independently of library size, including charts with
// exceptionally large speed graphs and warning lists.
const MAX_CACHE_BYTES: usize = 192 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 65_536;

type ContentKey = ([u8; 32], ChartSourceFormat);
type CacheSlot = Arc<Mutex<Option<Arc<ScanChart>>>>;

pub(super) struct ScanChart {
    pub prepared: PreparedChartImport,
    pub warnings: Vec<ImportWarning>,
}

pub(super) enum ScanImportError {
    Chart(ImportError),
    Encode(anyhow::Error),
}

#[derive(Default)]
pub(super) struct ImportWork {
    pub parse: Duration,
    pub analysis: Duration,
    pub encode: Duration,
    pub reused: bool,
}

#[derive(Default)]
struct Entries {
    by_content: HashMap<ContentKey, CacheSlot>,
    retained_bytes: usize,
}

/// A scan owns this cache across roots and drops it when the scan ends.
pub(super) struct ChartContentCache {
    duplicate_sizes: HashSet<u64>,
    entries: Mutex<Entries>,
}

impl ChartContentCache {
    pub fn new(sizes: impl IntoIterator<Item = u64>) -> Self {
        let mut seen = HashSet::new();
        let duplicate_sizes = sizes.into_iter().filter(|size| !seen.insert(*size)).collect();
        Self { duplicate_sizes, entries: Mutex::default() }
    }

    pub fn import(
        &self,
        path: &Path,
        bytes: &[u8],
        work: &mut ImportWork,
    ) -> Result<Arc<ScanChart>, ScanImportError> {
        if !self.duplicate_sizes.contains(&(bytes.len() as u64)) {
            return parse_chart(path, bytes, work);
        }

        // Size only gates the lookup. The bytes must match, and the extension must
        // select the same interpreter (.pms and .bmson have separate semantics).
        let key = (Sha256::digest(bytes).into(), chart_source_format(path));
        let slot = {
            let mut entries = self.entries.lock().unwrap();
            let may_insert = entries.by_content.len() < MAX_CACHE_ENTRIES
                && entries.retained_bytes < MAX_CACHE_BYTES;
            match entries.by_content.entry(key) {
                Entry::Occupied(entry) => Some(Arc::clone(entry.get())),
                Entry::Vacant(entry) if may_insert => {
                    Some(Arc::clone(entry.insert(Arc::default())))
                }
                Entry::Vacant(_) => None,
            }
        };
        let Some(slot) = slot else {
            return parse_chart(path, bytes, work);
        };
        // The first worker produces the result. Other workers with the same key
        // share it without serializing unrelated imports on the map lock.
        let mut cached = slot.lock().unwrap();
        if let Some(chart) = &*cached {
            work.reused = true;
            return Ok(Arc::clone(chart));
        }
        let chart = parse_chart(path, bytes, work)?;
        let bytes = chart.prepared.heap_bytes()
            + size_of::<ScanChart>()
            + chart.warnings.capacity() * size_of::<ImportWarning>()
            + chart.warnings.iter().map(warning_heap_bytes).sum::<usize>();
        if !chart.warnings.iter().any(|warning| {
            matches!(
                warning,
                ImportWarning::MissingSoundFile { .. } | ImportWarning::MissingBmpFile { .. }
            )
        }) {
            let mut entries = self.entries.lock().unwrap();
            if bytes <= MAX_CACHE_BYTES - entries.retained_bytes {
                entries.retained_bytes += bytes;
                *cached = Some(Arc::clone(&chart));
            }
        }
        Ok(chart)
    }
}

fn warning_heap_bytes(warning: &ImportWarning) -> usize {
    match warning {
        ImportWarning::ParserDiagnostic { code, message } => code.capacity() + message.capacity(),
        ImportWarning::UnsupportedCommand { command } => command.capacity(),
        _ => 0,
    }
}

fn parse_chart(
    path: &Path,
    bytes: &[u8],
    work: &mut ImportWork,
) -> Result<Arc<ScanChart>, ScanImportError> {
    let started = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| {
        import_chart_bytes_with_assets(
            path,
            bytes,
            BmsRandomSource::Seed(None),
            AssetLookup::Declared,
        )
    }));
    work.parse = started.elapsed();
    let imported = result
        .unwrap_or_else(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            Err(ImportError::Parse {
                path: path.to_path_buf(),
                message: format!("chart import panicked: {message}"),
            })
        })
        .map_err(ScanImportError::Chart)?;
    let started = Instant::now();
    let analysis = ChartImportAnalysis::new(&imported.chart);
    work.analysis = started.elapsed();
    let started = Instant::now();
    let prepared = analysis.encode(&imported.chart).map_err(ScanImportError::Encode)?;
    work.encode = started.elapsed();
    Ok(Arc::new(ScanChart { prepared, warnings: imported.warnings }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_keeps_bms_and_pms_separate_even_with_identical_bytes() {
        let bytes = b"#PLAYER 1\n#TITLE shared\n#BPM 120\n#WAV01 key.wav\n#00111:01\n";
        let cache = ChartContentCache::new([bytes.len() as u64; 3]);
        let import = |path, work: &mut ImportWork| {
            cache.import(Path::new(path), bytes, work).ok().expect("chart imports")
        };
        let bms = import("first.bms", &mut ImportWork::default());
        let mut pms_work = ImportWork::default();
        let pms = import("second.pms", &mut pms_work);
        assert!(!pms_work.reused);
        assert!(!Arc::ptr_eq(&bms, &pms));
        let mut bme_work = ImportWork::default();
        let bme = import("third.bme", &mut bme_work);
        assert!(bme_work.reused);
        assert!(Arc::ptr_eq(&bms, &bme));
    }
}
