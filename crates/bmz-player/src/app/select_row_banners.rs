//! Bounded, demand-driven songlist thumbnails. I/O is confined to two workers;
//! snapshots contain only leased GPU handles. Source and image identities are
//! separate so different charts/relative paths can share one decoded texture.
use bmz_render::assets::RgbaImageAsset;
use bmz_render::scene::SelectRowBannerSnapshot;
use bmz_render::skin::SkinTextureId;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

mod worker;
use worker::{Completion, ImageKey, Job, Pool};

const WORKERS: usize = 2;
const MAX_SOURCES: usize = 256;
const CPU_LIMIT: usize = 32 * 1024 * 1024;
const GPU_LIMIT: usize = 32 * 1024 * 1024;
const MAX_THUMB_BYTES: usize = 1024 * 256 * 4;
const REFRESH: Duration = Duration::from_secs(1);
const RETRY: Duration = Duration::from_secs(30);

#[derive(Debug, PartialEq, Eq, Hash)]
struct Source {
    folder: String,
    file: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceRef<'a> {
    pub folder: &'a str,
    pub file: &'a str,
}

impl Source {
    fn matches(&self, source: SourceRef<'_>) -> bool {
        self.folder == source.folder && self.file == source.file
    }
}

struct SourceState {
    revision: Option<Arc<ImageKey>>,
    checked: Option<Instant>,
    loading: bool,
    used: u64,
}
struct CpuImage {
    image: Arc<RgbaImageAsset>,
    used: u64,
}
struct GpuImage {
    view: SelectRowBannerSnapshot,
    bytes: usize,
    used: u64,
}
struct Retired {
    texture: SkinTextureId,
    lease: Weak<()>,
    bytes: usize,
}

pub(super) struct RowBannerRuntime {
    sources: HashMap<Arc<Source>, SourceState>,
    window: Vec<(Arc<Source>, bool)>,
    wanted: Vec<Arc<Source>>,
    visible: HashSet<Arc<Source>>,
    cpu: HashMap<Arc<ImageKey>, CpuImage>,
    gpu: HashMap<Arc<ImageKey>, GpuImage>,
    decoding: HashSet<Arc<ImageKey>>,
    failed: HashMap<Arc<ImageKey>, Instant>,
    upload_failed: HashMap<Arc<ImageKey>, Instant>,
    retired: Vec<Retired>,
    cpu_bytes: usize,
    gpu_bytes: usize,
    cpu_limit: usize,
    gpu_limit: usize,
    clock: u64,
    epoch: u64,
    active: usize,
    pool: Option<Pool>,
    pool_retry: Option<Instant>,
}

impl Default for RowBannerRuntime {
    fn default() -> Self {
        Self {
            sources: HashMap::new(),
            window: Vec::new(),
            wanted: Vec::new(),
            visible: HashSet::new(),
            cpu: HashMap::new(),
            gpu: HashMap::new(),
            decoding: HashSet::new(),
            failed: HashMap::new(),
            retired: Vec::new(),
            upload_failed: HashMap::new(),
            cpu_bytes: 0,
            gpu_bytes: 0,
            cpu_limit: CPU_LIMIT,
            gpu_limit: GPU_LIMIT,
            clock: 0,
            epoch: 0,
            active: 0,
            pool: None,
            pool_retry: None,
        }
    }
}

impl RowBannerRuntime {
    pub fn has_demand(&self) -> bool {
        !self.wanted.is_empty()
    }

    pub fn has_retired(&self) -> bool {
        !self.retired.is_empty()
    }

    /// Compare borrowed metadata first: an unchanged window allocates no paths
    /// or strings. No filesystem calls or Lua callbacks are made here.
    pub fn reconcile<'a>(&mut self, sources: impl Iterator<Item = (SourceRef<'a>, bool)> + Clone) {
        // Leave room for the only two old sources that can still be in flight.
        let sources = sources.take(MAX_SOURCES - WORKERS);
        self.clock = self.clock.wrapping_add(1);
        let unchanged = self.window.len() == sources.clone().count()
            && self.window.iter().zip(sources.clone()).all(
                |((old, visible), (source, new_visible))| {
                    old.matches(source) && *visible == new_visible
                },
            );
        if !unchanged {
            self.window = sources
                .map(|(source, visible)| {
                    (
                        self.sources
                            .keys()
                            .find(|key| key.matches(source))
                            .cloned()
                            .unwrap_or_else(|| {
                                Arc::new(Source {
                                    folder: source.folder.into(),
                                    file: source.file.into(),
                                })
                            }),
                        visible,
                    )
                })
                .collect();
            self.wanted.clear();
            self.visible.clear();
            for (source, visible) in &self.window {
                if *visible {
                    self.visible.insert(source.clone());
                }
                if self.wanted.contains(source) {
                    continue;
                }
                self.wanted.push(source.clone());
                self.sources.entry(source.clone()).or_insert(SourceState {
                    revision: None,
                    checked: None,
                    loading: false,
                    used: self.clock,
                });
            }
            self.trim_sources();
        }
        for source in &self.wanted {
            let Some(state) = self.sources.get_mut(source) else {
                continue;
            };
            state.used = self.clock;
            if let Some(key) = &state.revision {
                if let Some(cpu) = self.cpu.get_mut(key) {
                    cpu.used = self.clock;
                }
                if let Some(gpu) = self.gpu.get_mut(key) {
                    gpu.used = self.clock;
                }
            }
        }
    }

    fn wanted_image(&self, key: &Arc<ImageKey>) -> bool {
        self.wanted.iter().any(|source| {
            self.sources.get(source).is_some_and(|s| s.revision.as_ref() == Some(key))
        })
    }

    fn trim_sources(&mut self) {
        while self.sources.len() > MAX_SOURCES {
            let victim = self
                .sources
                .iter()
                .filter(|(source, state)| !state.loading && !self.wanted.contains(source))
                .min_by_key(|(_, state)| state.used)
                .map(|(source, _)| source.clone());
            let Some(victim) = victim else {
                break;
            };
            self.sources.remove(&victim);
        }
        if self.failed.len() > MAX_SOURCES {
            self.failed
                .retain(|key, _| self.sources.values().any(|s| s.revision.as_ref() == Some(key)));
        }
        if self.upload_failed.len() > MAX_SOURCES {
            self.upload_failed
                .retain(|key, _| self.sources.values().any(|s| s.revision.as_ref() == Some(key)));
        }
    }

    fn make_cpu_room(&mut self, bytes: usize) -> bool {
        while self.cpu_bytes.saturating_add(bytes) > self.cpu_limit
            || self.cpu.len() + self.decoding.len() >= MAX_SOURCES
        {
            let victim = self
                .cpu
                .iter()
                .filter(|(key, _)| self.gpu.contains_key(*key) || !self.wanted_image(key))
                .min_by_key(|(_, image)| image.used)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else {
                return false;
            };
            if let Some(image) = self.cpu.remove(&victim) {
                self.cpu_bytes -= image.image.pixels.len();
            }
        }
        true
    }

    fn next_job(&mut self, now: Instant) -> Option<Job> {
        if self.active >= WORKERS {
            return None;
        }
        for index in 0..self.wanted.len() {
            let source = self.wanted[index].clone();
            let state = self.sources.get(&source)?;
            if let Some(key) = state.revision.clone()
                && !self.cpu.contains_key(&key)
                && !self.gpu.contains_key(&key)
                && !self.decoding.contains(&key)
                && self.failed.get(&key).is_none_or(|failed| now.duration_since(*failed) >= RETRY)
                && self.make_cpu_room((self.decoding.len() + 1) * MAX_THUMB_BYTES)
            {
                self.decoding.insert(key.clone());
                self.active += 1;
                return Some(Job::Decode { epoch: self.epoch, key });
            }
            let state = self.sources.get_mut(&source)?;
            let interval = if state.revision.is_some() { REFRESH } else { RETRY };
            if !state.loading
                && state.checked.is_none_or(|checked| now.duration_since(checked) >= interval)
            {
                state.loading = true;
                self.active += 1;
                return Some(Job::Resolve { epoch: self.epoch, source });
            }
        }
        None
    }

    fn complete(&mut self, completion: Completion, now: Instant) {
        self.active = self.active.saturating_sub(1);
        match completion {
            Completion::Resolved { epoch, source, result } => {
                if epoch != self.epoch {
                    return;
                }
                if let Some(state) = self.sources.get_mut(&source) {
                    state.loading = false;
                    state.checked = Some(now);
                    state.revision = match result {
                        Ok(revision) => revision,
                        Err(error) => {
                            tracing::debug!(%error, "skipping row banner source");
                            None
                        }
                    };
                }
            }
            Completion::Decoded { epoch, key, result } => {
                if epoch != self.epoch {
                    return;
                }
                self.decoding.remove(&key);
                match result {
                    Ok(image) => {
                        self.failed.remove(&key);
                        if !self.cpu.contains_key(&key) && self.make_cpu_room(image.pixels.len()) {
                            self.cpu_bytes += image.pixels.len();
                            self.cpu.insert(key, CpuImage { image, used: self.clock });
                        }
                    }
                    Err(error) => {
                        tracing::debug!(%error, "skipping row banner image");
                        self.failed.insert(key, now);
                    }
                }
            }
        }
        self.trim_sources();
    }

    pub fn tick(&mut self, now: Instant) {
        while let Some(completion) = self.pool.as_ref().and_then(|pool| pool.rx.try_recv().ok()) {
            self.complete(completion, now);
        }
        if self.wanted.is_empty() {
            return;
        }
        if self.pool.is_none() && self.pool_retry.is_none_or(|retry| now >= retry) {
            match Pool::new() {
                Ok(pool) => self.pool = Some(pool),
                Err(error) => {
                    tracing::warn!(%error, "failed to start row banner workers");
                    self.pool_retry = Some(now + Duration::from_secs(5));
                }
            }
        }
        if self.pool.is_none() {
            return;
        }
        while let Some(job) = self.next_job(now) {
            // The scheduler admits at most WORKERS outstanding jobs, including
            // completed results waiting for this thread. This cannot queue an
            // unbounded backlog while scrolling or while Select is inactive.
            if let Err(error) = self.pool.as_ref().unwrap().tx.try_send(job) {
                let job = match error {
                    std::sync::mpsc::TrySendError::Full(job)
                    | std::sync::mpsc::TrySendError::Disconnected(job) => job,
                };
                let error = anyhow::anyhow!("row banner worker unavailable");
                self.complete(
                    match job {
                        Job::Resolve { epoch, source } => {
                            Completion::Resolved { epoch, source, result: Err(error) }
                        }
                        Job::Decode { epoch, key } => {
                            Completion::Decoded { epoch, key, result: Err(error) }
                        }
                    },
                    now,
                );
                break;
            }
        }
    }

    fn retire(&mut self, image: GpuImage) {
        self.retired.push(Retired {
            texture: image.view.texture,
            lease: Arc::downgrade(&image.view.lease),
            bytes: image.bytes,
        });
    }

    /// Uploads are prioritized and capped at two thumbnails / 2 MiB per frame.
    /// Retired but still leased textures remain charged to the GPU budget.
    pub fn upload_ready(
        &mut self,
        now: Instant,
        mut upload: impl FnMut(&RgbaImageAsset) -> Option<SelectRowBannerSnapshot>,
    ) {
        let mut uploads = 0;
        let mut uploaded_bytes = 0;
        for index in 0..self.wanted.len() {
            if uploads >= 2 {
                break;
            }
            if !self.visible.contains(&self.wanted[index]) {
                continue;
            }
            let Some(key) = self.sources.get(&self.wanted[index]).and_then(|s| s.revision.clone())
            else {
                continue;
            };
            if self.gpu.contains_key(&key) {
                continue;
            }
            if self
                .upload_failed
                .get(&key)
                .is_some_and(|failed| now.duration_since(*failed) < RETRY)
            {
                continue;
            }
            let Some(cpu) = self.cpu.get(&key) else {
                continue;
            };
            let image = cpu.image.clone();
            let bytes = image.pixels.len();
            if uploaded_bytes + bytes > 2 * MAX_THUMB_BYTES {
                continue;
            }
            while self.gpu_bytes + bytes > self.gpu_limit
                || self.gpu.len() + self.retired.len() >= MAX_SOURCES
            {
                let victim = self
                    .gpu
                    .iter()
                    .filter(|(key, _)| !self.wanted_image(key))
                    .min_by_key(|(_, image)| image.used)
                    .map(|(key, _)| key.clone());
                let Some(victim) = victim else {
                    break;
                };
                let image = self.gpu.remove(&victim).unwrap();
                self.retire(image);
            }
            if self.gpu_bytes + bytes > self.gpu_limit
                || self.gpu.len() + self.retired.len() >= MAX_SOURCES
            {
                continue;
            }
            uploads += 1;
            uploaded_bytes += bytes;
            if let Some(view) = upload(&image) {
                self.upload_failed.remove(&key);
                self.gpu_bytes += bytes;
                self.gpu.insert(key, GpuImage { view, bytes, used: self.clock });
            } else {
                self.upload_failed.insert(key, now);
            }
        }
        self.trim_sources();
    }

    pub fn image(&self, source: SourceRef<'_>) -> Option<SelectRowBannerSnapshot> {
        let source = self.wanted.iter().find(|key| key.matches(source))?;
        let key = self.sources.get(source)?.revision.as_ref()?;
        Some(self.gpu.get(key)?.view.clone())
    }

    pub fn collect_retired(&mut self, mut release: impl FnMut(SkinTextureId)) {
        self.retired.retain(|retired| {
            if retired.lease.strong_count() != 0 {
                return true;
            }
            release(retired.texture);
            self.gpu_bytes -= retired.bytes;
            false
        });
    }

    pub fn deactivate(&mut self) {
        self.window.clear();
        self.wanted.clear();
        self.visible.clear();
        for (_, image) in std::mem::take(&mut self.gpu) {
            self.retire(image);
        }
        self.tick(Instant::now());
    }

    pub fn invalidate(&mut self) {
        self.deactivate();
        self.epoch = self.epoch.wrapping_add(1);
        self.sources.clear();
        self.cpu.clear();
        self.cpu_bytes = 0;
        self.decoding.clear();
        self.failed.clear();
        self.upload_failed.clear();
    }
}

#[cfg(test)]
mod tests;
