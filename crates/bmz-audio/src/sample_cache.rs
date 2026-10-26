//! Bounded reuse of decoded keysounds. The sample bank owns playback; this
//! cache only keeps recent immutable PCM and never caches missing/failed files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::loader::{SampleLoadError, SampleLoader};
use crate::sample::DecodedSample;

// Empty/very short samples still own keys and metadata.
const MAX_ENTRIES: usize = 4096;

#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    #[cfg(unix)]
    changed: (u64, i64, i64),
}

enum SourceStamp {
    File(FileStamp),
    // Pointer equality identifies the exact bytes used by the pinned archive
    // path. Retain/count them so allocator address reuse cannot fake a hit.
    Archive(Arc<[u8]>),
}

impl SourceStamp {
    fn read(path: &Path) -> Option<Self> {
        match bmz_song_resource::SongPath::parse(path) {
            bmz_song_resource::SongPath::Archive { .. } => {
                bmz_song_resource::read(path)?.ok().map(Self::Archive)
            }
            bmz_song_resource::SongPath::Invalid => None,
            bmz_song_resource::SongPath::File => {
                let meta = std::fs::metadata(path).ok()?;
                if !meta.is_file() {
                    return None;
                }
                #[cfg(unix)]
                use std::os::unix::fs::MetadataExt;
                Some(Self::File(FileStamp {
                    len: meta.len(),
                    modified: meta.modified().ok()?,
                    created: meta.created().ok(),
                    #[cfg(unix)]
                    changed: (meta.ino(), meta.ctime(), meta.ctime_nsec()),
                }))
            }
        }
    }

    fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(a), Self::File(b)) => a == b,
            (Self::Archive(a), Self::Archive(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::Archive(bytes) => bytes.len(),
            Self::File(_) => 0,
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    output_rate: u32,
    gain: u32,
}

struct Entry {
    stamp: SourceStamp,
    sample: Arc<DecodedSample>,
    bytes: usize,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SampleCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub retained_bytes: usize,
}

pub struct SampleCache {
    entries: HashMap<Key, Entry>,
    limit: usize,
    clock: u64,
    stats: SampleCacheStats,
}

impl SampleCache {
    pub fn new(limit_bytes: usize) -> Self {
        Self { entries: HashMap::new(), limit: limit_bytes, clock: 0, stats: Default::default() }
    }

    pub fn stats(&self) -> SampleCacheStats {
        self.stats
    }

    fn get(&mut self, key: &Key, stamp: &SourceStamp) -> Option<Arc<DecodedSample>> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(key)
            && entry.stamp.matches(stamp)
        {
            entry.last_used = self.clock;
            self.stats.hits += 1;
            return Some(entry.sample.clone());
        }
        self.stats.misses += 1;
        self.remove(key);
        None
    }

    fn remove(&mut self, key: &Key) {
        if let Some(old) = self.entries.remove(key) {
            self.stats.retained_bytes -= old.bytes;
        }
    }

    fn insert(&mut self, key: Key, stamp: SourceStamp, sample: Arc<DecodedSample>) {
        self.remove(&key);
        let bytes = sample
            .frames
            .capacity()
            .saturating_mul(size_of::<f32>())
            .saturating_add(stamp.retained_bytes());
        if bytes > self.limit {
            return;
        }
        while self.stats.retained_bytes.saturating_add(bytes) > self.limit
            || self.entries.len() >= MAX_ENTRIES
        {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| Key { path: key.path.clone(), ..*key })
            else {
                break;
            };
            self.remove(&key);
        }
        self.clock = self.clock.wrapping_add(1);
        self.stats.retained_bytes += bytes;
        self.entries.insert(key, Entry { stamp, sample, bytes, last_used: self.clock });
    }
}

pub struct CachedSampleLoader<L> {
    loader: L,
    cache: Arc<Mutex<SampleCache>>,
}

impl<L> CachedSampleLoader<L> {
    pub fn new(loader: L, cache: Arc<Mutex<SampleCache>>) -> Self {
        Self { loader, cache }
    }
}

impl<L: SampleLoader> SampleLoader for CachedSampleLoader<L> {
    fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
        self.loader.load(path)
    }

    fn duration_ms_hint(&mut self, path: &Path) -> Option<i64> {
        self.loader.duration_ms_hint(path)
    }

    fn load_for_engine(
        &mut self,
        path: &Path,
        volwav: f32,
        output_rate: u32,
    ) -> Result<Arc<DecodedSample>, SampleLoadError> {
        let key = Key { path: path.to_path_buf(), output_rate, gain: volwav.to_bits() };
        let before = SourceStamp::read(path);
        if let Some(stamp) = &before
            && let Ok(mut cache) = self.cache.lock()
            && let Some(sample) = cache.get(&key, stamp)
        {
            return Ok(sample);
        }
        let sample = self.loader.load_for_engine(path, volwav, output_rate)?;
        // Do not retain a decode if the source changed while it was read.
        if let (Some(before), Some(after)) = (before, SourceStamp::read(path))
            && before.matches(&after)
            && let Ok(mut cache) = self.cache.lock()
        {
            cache.insert(key, after, sample.clone());
        }
        Ok(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::WavSampleLoader;

    fn wav(path: &Path, samples: &[i16]) {
        let data_len = samples.len() as u32 * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn reuses_pcm_but_reloads_changed_missing_and_failed_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.wav");
        let cache = Arc::new(Mutex::new(SampleCache::new(1024)));
        let mut loader = CachedSampleLoader::new(WavSampleLoader, cache.clone());
        std::fs::write(&path, b"broken").unwrap();
        assert!(loader.load_for_engine(&path, 1.0, 48_000).is_err());
        wav(&path, &[16384]);
        let first = loader.load_for_engine(&path, 1.0, 48_000).unwrap();
        let next = loader.load_for_engine(&path, 1.0, 48_000).unwrap();
        assert!(Arc::ptr_eq(&first, &next));
        assert_eq!(first.frames, [0.5]);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        wav(&path, &[-8192]);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified + std::time::Duration::from_secs(2))
            .unwrap();
        let same_size = loader.load_for_engine(&path, 1.0, 48_000).unwrap();
        assert_eq!(same_size.frames, [-0.25]);
        assert!(!Arc::ptr_eq(&first, &same_size));
        wav(&path, &[-16384, 8192]);
        let changed = loader.load_for_engine(&path, 1.0, 48_000).unwrap();
        assert_eq!(changed.frames, [-0.5, 0.25]);
        assert!(!Arc::ptr_eq(&first, &changed));
        std::fs::remove_file(&path).unwrap();
        assert!(loader.load_for_engine(&path, 1.0, 48_000).is_err());
        assert_eq!(cache.lock().unwrap().stats().hits, 1);
    }

    #[test]
    fn rate_and_volwav_have_separate_entries_and_lru_stays_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.wav");
        wav(&path, &[16384, -16384]);
        let cache = Arc::new(Mutex::new(SampleCache::new(24)));
        let mut loader = CachedSampleLoader::new(WavSampleLoader, cache.clone());
        let normal = loader.load_for_engine(&path, 1.0, 48_000).unwrap();
        let gain = loader.load_for_engine(&path, 0.5, 48_000).unwrap();
        assert_eq!(gain.frames, [0.25, -0.25]);
        let resampled = loader.load_for_engine(&path, 1.0, 24_000).unwrap();
        assert_eq!(resampled.sample_rate, 24_000);
        assert_eq!(resampled.frame_count(), 1);
        assert!(Arc::ptr_eq(&normal, &loader.load_for_engine(&path, 1.0, 48_000).unwrap()));
        let other = dir.path().join("other.wav");
        wav(&other, &[1, 2]);
        loader.load_for_engine(&other, 1.0, 48_000).unwrap();
        assert!(cache.lock().unwrap().stats().retained_bytes <= 24);
        assert!(!Arc::ptr_eq(&gain, &loader.load_for_engine(&path, 0.5, 48_000).unwrap()));
        // Evicting a cache entry never destroys an active bank's sample.
        assert_eq!(normal.frames, [0.5, -0.5]);
    }

    #[test]
    fn archive_cache_keeps_the_exact_resource_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key.wav");
        wav(&path, &[16384]);
        let bytes = std::fs::read(&path).unwrap();
        let container = dir.path().join("set.zip");
        let set = Arc::new(bmz_song_resource::SongResourceSet::new(
            &container,
            "old",
            ["key.wav".into()],
        ));
        set.insert("key.wav", bytes);
        bmz_song_resource::install(set.clone());
        let pinned = set.path("key.wav");
        let cache = Arc::new(Mutex::new(SampleCache::new(1024)));
        let mut loader = CachedSampleLoader::new(WavSampleLoader, cache.clone());
        let old = loader.load_for_engine(&pinned, 1.0, 48_000).unwrap();
        assert!(Arc::ptr_eq(&old, &loader.load_for_engine(&pinned, 1.0, 48_000).unwrap()));
        wav(&path, &[-16384]);
        let next = Arc::new(bmz_song_resource::SongResourceSet::new(
            &container,
            "new",
            ["key.wav".into()],
        ));
        next.insert("key.wav", std::fs::read(path).unwrap());
        bmz_song_resource::install(next.clone());
        let changed = loader.load_for_engine(&next.path("key.wav"), 1.0, 48_000).unwrap();
        assert_eq!(changed.frames, [-0.5]);
        assert_eq!(loader.load_for_engine(&pinned, 1.0, 48_000).unwrap().frames, [0.5]);
    }

    #[test]
    fn empty_samples_cannot_grow_metadata_without_bound() {
        let mut cache = SampleCache::new(1024);
        let sample =
            Arc::new(DecodedSample { channels: 1, sample_rate: 48_000, frames: Vec::new() });
        for index in 0..MAX_ENTRIES + 1 {
            cache.insert(
                Key {
                    path: format!("{index}.wav").into(),
                    output_rate: 48_000,
                    gain: 1f32.to_bits(),
                },
                SourceStamp::Archive(Arc::from([])),
                sample.clone(),
            );
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert_eq!(cache.stats().retained_bytes, 0);
        assert!(!cache.entries.contains_key(&Key {
            path: "0.wav".into(),
            output_rate: 48_000,
            gain: 1f32.to_bits()
        }));
    }

    #[test]
    fn cache_budget_counts_allocated_pcm_capacity() {
        let mut frames = Vec::with_capacity(1024);
        frames.push(0.25);
        let mut cache = SampleCache::new(16);
        cache.insert(
            Key { path: "large-reserve.wav".into(), output_rate: 48_000, gain: 1f32.to_bits() },
            SourceStamp::Archive(Arc::from([])),
            Arc::new(DecodedSample { channels: 1, sample_rate: 48_000, frames }),
        );
        assert!(cache.entries.is_empty());
        assert_eq!(cache.stats().retained_bytes, 0);
    }
}
