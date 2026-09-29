use super::*;

pub(super) const SKIN_DOCUMENT_CACHE_LIMIT_ENTRIES: usize = 16;
pub(super) const SKIN_SOURCE_ASSET_CACHE_LIMIT_BYTES: usize = 256 * 1024 * 1024;
pub(super) const SKIN_FONT_CACHE_LIMIT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Default)]
pub struct SkinDocumentCache {
    pub(super) entries: Vec<SkinDocumentCacheEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct SkinDocumentCacheKey {
    pub(super) path: PathBuf,
    pub(super) kind: SkinKind,
    pub(super) library_roots: Vec<PathBuf>,
    pub(super) modified: Option<SystemTime>,
    pub(super) len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SkinDocumentDependencyFingerprint {
    pub(super) lua_runtime_mode: bmz_skin::LuaSkinRuntimeMode,
    pub(super) number_values: BTreeMap<i32, i32>,
    pub(super) text_values: BTreeMap<i32, String>,
    pub(super) option_values: BTreeMap<i32, bool>,
    pub(super) event_index_values: BTreeMap<i32, i32>,
    pub(super) offset_values: BTreeMap<String, bmz_skin::LuaSkinOffsetValue>,
    pub(super) offset_id_values: BTreeMap<i32, bmz_skin::LuaSkinOffsetValue>,
    pub(super) file_values: BTreeMap<String, String>,
    pub(super) loaded_files: BTreeMap<PathBuf, SkinLoadedFileDependency>,
    pub(super) virtual_io_files: BTreeMap<String, Option<String>>,
}

#[derive(Clone)]
pub(super) struct SkinDocumentCacheEntry {
    pub(super) key: SkinDocumentCacheKey,
    pub(super) fingerprint: SkinDocumentDependencyFingerprint,
    pub(super) document: SkinDocument,
    pub(super) files: BTreeMap<String, String>,
    pub(super) dependencies: SkinLoadDependencies,
}

impl SkinDocumentCache {
    pub(super) fn get_lr2(
        &mut self,
        key: &SkinDocumentCacheKey,
        skin_path: &Path,
        options: &BTreeMap<String, String>,
        files: &BTreeMap<String, String>,
    ) -> Option<(SkinDocument, BTreeMap<String, String>)> {
        let entry_index = self.entries.iter().position(|entry| {
            entry.key == *key
                && !entry.dependencies.opaque
                && lr2_document_dependency_fingerprint(
                    skin_path,
                    options,
                    files,
                    &entry.dependencies,
                )
                .is_ok_and(|fingerprint| fingerprint == entry.fingerprint)
        })?;
        let entry = self.entries.remove(entry_index);
        let document = entry.document.clone();
        let files = entry.files.clone();
        self.entries.push(entry);
        Some((document, files))
    }

    pub(super) fn get_lua(
        &mut self,
        key: &SkinDocumentCacheKey,
        options: &BTreeMap<String, String>,
        files: &BTreeMap<String, String>,
        runtime_state: &LuaLoadRuntimeState,
    ) -> Option<(SkinDocument, BTreeMap<String, String>)> {
        let entry_index = self.entries.iter().position(|entry| {
            entry.key == *key
                && !entry.dependencies.opaque
                && document_dependency_fingerprint(
                    &entry.document,
                    options,
                    files,
                    runtime_state,
                    &entry.dependencies,
                )
                .is_some_and(|fingerprint| fingerprint == entry.fingerprint)
        })?;
        let entry = self.entries.remove(entry_index);
        let document = entry.document.clone();
        let files = entry.files.clone();
        self.entries.push(entry);
        Some((document, files))
    }

    pub(super) fn insert_lr2(
        &mut self,
        key: SkinDocumentCacheKey,
        fingerprint: SkinDocumentDependencyFingerprint,
        document: SkinDocument,
        files: BTreeMap<String, String>,
        dependencies: SkinLoadDependencies,
    ) {
        self.insert(key, fingerprint, document, files, dependencies);
    }

    pub(super) fn insert_lua(
        &mut self,
        key: SkinDocumentCacheKey,
        fingerprint: SkinDocumentDependencyFingerprint,
        document: SkinDocument,
        files: BTreeMap<String, String>,
        dependencies: SkinLoadDependencies,
    ) {
        self.insert(key, fingerprint, document, files, dependencies);
    }

    pub(super) fn insert(
        &mut self,
        key: SkinDocumentCacheKey,
        fingerprint: SkinDocumentDependencyFingerprint,
        document: SkinDocument,
        files: BTreeMap<String, String>,
        dependencies: SkinLoadDependencies,
    ) {
        if dependencies.opaque {
            return;
        }
        self.entries.retain(|entry| entry.key != key || entry.fingerprint != fingerprint);
        self.entries.push(SkinDocumentCacheEntry {
            key,
            fingerprint,
            document,
            files,
            dependencies,
        });
        while self.entries.len() > SKIN_DOCUMENT_CACHE_LIMIT_ENTRIES {
            self.entries.remove(0);
        }
    }
}

#[derive(Default)]
pub struct SkinSourceAssetCache {
    pub(super) entries: HashMap<SkinSourceAssetCacheKey, RgbaImageAsset>,
    pub(super) total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkinSourceAssetCacheKey {
    pub(super) path: PathBuf,
    pub(super) modified: Option<SystemTime>,
    pub(super) len: u64,
    pub(super) is_video: bool,
}

impl SkinSourceAssetCache {
    pub(super) fn get(&self, key: &SkinSourceAssetCacheKey) -> Option<RgbaImageAsset> {
        self.entries.get(key).cloned()
    }

    pub(super) fn insert(&mut self, key: SkinSourceAssetCacheKey, asset: RgbaImageAsset) {
        let bytes = asset.pixels.len();
        if let Some(old) = self.entries.remove(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(old.pixels.len());
        }
        if bytes > SKIN_SOURCE_ASSET_CACHE_LIMIT_BYTES {
            return;
        }
        if self.total_bytes.saturating_add(bytes) > SKIN_SOURCE_ASSET_CACHE_LIMIT_BYTES {
            self.entries.clear();
            self.total_bytes = 0;
        }
        self.total_bytes += bytes;
        self.entries.insert(key, asset);
    }
}

pub struct SkinFontCache {
    pub(super) entries: HashMap<SkinFontCacheKey, CachedSkinFontEntry>,
    pub(super) total_bytes: usize,
    pub(super) limit_bytes: usize,
    pub(super) access_clock: u64,
}

pub(super) struct CachedSkinFontEntry {
    pub(super) data: DecodedFontData,
    pub(super) bytes: usize,
    pub(super) last_used: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkinFontCacheKey {
    pub(super) path: PathBuf,
    pub(super) modified: Option<SystemTime>,
    pub(super) len: u64,
    pub(super) is_bitmap: bool,
    pub(super) pages: Vec<(PathBuf, Option<SystemTime>, u64)>,
}

impl SkinFontCache {
    pub(super) fn get(&mut self, key: &SkinFontCacheKey) -> Option<DecodedFontData> {
        let access = self.next_access();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = access;
        Some(entry.data.clone())
    }

    pub(super) fn insert(&mut self, key: SkinFontCacheKey, data: DecodedFontData) {
        let bytes = font_data_cache_bytes(&data);
        if let Some(old) = self.entries.remove(&key) {
            self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
        }
        if bytes > self.limit_bytes {
            return;
        }
        self.evict_until_fits(bytes);
        let access = self.next_access();
        self.total_bytes += bytes;
        self.entries.insert(key, CachedSkinFontEntry { data, bytes, last_used: access });
    }

    pub(super) fn evict_until_fits(&mut self, incoming_bytes: usize) {
        while self.total_bytes.saturating_add(incoming_bytes) > self.limit_bytes {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(old) = self.entries.remove(&key) {
                self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
            }
        }
    }

    pub(super) fn next_access(&mut self) -> u64 {
        self.access_clock = self.access_clock.wrapping_add(1);
        self.access_clock
    }

    #[cfg(test)]
    pub(super) fn with_limit_bytes(limit_bytes: usize) -> Self {
        Self { limit_bytes, ..Self::default() }
    }
}

impl Default for SkinFontCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            limit_bytes: SKIN_FONT_CACHE_LIMIT_BYTES,
            access_clock: 0,
        }
    }
}

pub struct SkinGpuTextureCache {
    pub(super) entries: HashMap<SkinSourceAssetCacheKey, CachedSkinGpuTexture>,
    pub(super) next_texture_ids: HashMap<SkinKind, u32>,
    allocations: HashMap<SkinTextureId, std::sync::Weak<()>>,
    free_texture_ids: Vec<SkinTextureId>,
    access_clock: u64,
    pub(super) limit_bytes: usize,
}

impl Default for SkinGpuTextureCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            next_texture_ids: HashMap::new(),
            allocations: HashMap::new(),
            free_texture_ids: Vec::new(),
            access_clock: 0,
            limit_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedSkinGpuTexture {
    pub texture: SkinTextureId,
    pub size: SkinImageSize,
    pub lease: Arc<()>,
    last_used: u64,
}

impl SkinGpuTextureCache {
    pub fn get(&mut self, key: &SkinSourceAssetCacheKey) -> Option<CachedSkinGpuTexture> {
        self.access_clock = self.access_clock.wrapping_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.access_clock;
        Some(entry.clone())
    }

    pub fn insert(
        &mut self,
        key: SkinSourceAssetCacheKey,
        texture: SkinTextureId,
        size: SkinImageSize,
    ) {
        self.access_clock = self.access_clock.wrapping_add(1);
        let lease = self.lease_texture(texture);
        self.entries.insert(
            key,
            CachedSkinGpuTexture { texture, size, lease, last_used: self.access_clock },
        );
    }

    pub(super) fn lease_texture(&mut self, texture: SkinTextureId) -> Arc<()> {
        if let Some(lease) = self.allocations.get(&texture).and_then(std::sync::Weak::upgrade) {
            return lease;
        }
        let lease = Arc::new(());
        self.allocations.insert(texture, Arc::downgrade(&lease));
        lease
    }

    /// Active scenes and pending decode/upload leases cannot be evicted.
    /// The caller removes returned IDs from the renderer while holding this lock.
    pub fn evict_unused(&mut self, active: &HashSet<SkinTextureId>) -> Vec<SkinTextureId> {
        let bytes = |entry: &CachedSkinGpuTexture| {
            (entry.size.width as usize).saturating_mul(entry.size.height as usize).saturating_mul(4)
        };
        let mut total = self.entries.values().map(bytes).fold(0usize, usize::saturating_add);
        while total > self.limit_bytes {
            let Some(key) = self
                .entries
                .iter()
                .filter(|(_, entry)| {
                    !active.contains(&entry.texture) && Arc::strong_count(&entry.lease) == 1
                })
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                total = total.saturating_sub(bytes(&entry));
            }
        }
        let removed: Vec<_> = self
            .allocations
            .iter()
            .filter(|(texture, lease)| !active.contains(texture) && lease.strong_count() == 0)
            .map(|(texture, _)| *texture)
            .collect();
        for texture in &removed {
            self.allocations.remove(texture);
        }
        self.free_texture_ids.extend(removed.iter().copied());
        removed
    }

    pub(super) fn allocate_texture_id(&mut self, kind: SkinKind) -> SkinTextureId {
        if let Some(texture) = self.free_texture_ids.pop() {
            return texture;
        }
        let next = self.next_texture_ids.entry(kind).or_insert_with(|| kind.first_texture_id());
        let texture = SkinTextureId(*next);
        *next = next.saturating_add(1);
        texture
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_texture_ids.clear();
        self.allocations.clear();
        self.free_texture_ids.clear();
    }
}
