use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};
use bmz_core::lane::KeyMode;
use bmz_render::assets::{RgbaImageAsset, load_png_rgba};
use bmz_render::bitmap_font::{BitmapFont, load_bitmap_font};
use bmz_render::plan::TextureId;
use bmz_render::renderer::{GpuUploader, PreparedTexture, Renderer};
use bmz_render::skin::{
    DestinationListEntry, SkinContext, SkinDocument, SkinDocumentTexture, SkinFilepathDef,
    SkinImageSize, SkinManifest, SkinTextureId, default_skin_manifest_for_root,
};
use bmz_skin::{
    LuaLoadRuntimeState, SkinKind as DecodeSkinKind, SkinLoadDependencies, SkinLoadedFileDependency,
};
use rayon::prelude::*;

use crate::config::profile_config::{SkinConfig, SkinOffsetConfig};
use crate::paths::{AppPaths, resolve_app_paths};

/// `SkinConfig` から key_mode に対応するプレイスキン path / options / files / offsets を借用する。
pub struct PlaySkinSelection<'a> {
    pub key_mode: KeyMode,
    pub path: &'a str,
    pub options: &'a BTreeMap<String, String>,
    pub files: &'a BTreeMap<String, String>,
    pub offsets: &'a [SkinOffsetConfig],
}

/// `SkinConfig` から key_mode に応じたプレイスキン設定の参照を取り出す。
pub fn play_skin_selection_for(skin: &SkinConfig, key_mode: KeyMode) -> PlaySkinSelection<'_> {
    match key_mode {
        KeyMode::K5 => PlaySkinSelection {
            key_mode,
            path: skin.play5.as_str(),
            options: &skin.play5_options,
            files: &skin.play5_files,
            offsets: &skin.play5_offsets,
        },
        KeyMode::K4 => PlaySkinSelection {
            key_mode,
            path: skin.play4.as_str(),
            options: &skin.play4_options,
            files: &skin.play4_files,
            offsets: &skin.play4_offsets,
        },
        KeyMode::K6 => PlaySkinSelection {
            key_mode,
            path: skin.play6.as_str(),
            options: &skin.play6_options,
            files: &skin.play6_files,
            offsets: &skin.play6_offsets,
        },
        KeyMode::K7 => PlaySkinSelection {
            key_mode,
            path: skin.play7.as_str(),
            options: &skin.play7_options,
            files: &skin.play7_files,
            offsets: &skin.play7_offsets,
        },
        KeyMode::K8 => PlaySkinSelection {
            key_mode,
            path: skin.play8.as_str(),
            options: &skin.play8_options,
            files: &skin.play8_files,
            offsets: &skin.play8_offsets,
        },
        KeyMode::K10 => PlaySkinSelection {
            key_mode,
            path: skin.play10.as_str(),
            options: &skin.play10_options,
            files: &skin.play10_files,
            offsets: &skin.play10_offsets,
        },
        KeyMode::K14 => PlaySkinSelection {
            key_mode,
            path: skin.play14.as_str(),
            options: &skin.play14_options,
            files: &skin.play14_files,
            offsets: &skin.play14_offsets,
        },
        KeyMode::K9 => PlaySkinSelection {
            key_mode,
            path: skin.play9.as_str(),
            options: &skin.play9_options,
            files: &skin.play9_files,
            offsets: &skin.play9_offsets,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkinKind {
    Play,
    Select,
    Decide,
    Result,
}

impl SkinKind {
    fn first_texture_id(self) -> u32 {
        match self {
            SkinKind::Play => 10_000,
            SkinKind::Select => 20_000,
            SkinKind::Decide => 25_000,
            SkinKind::Result => 30_000,
        }
    }

    fn warn_missing_required_sources(self) -> bool {
        matches!(self, SkinKind::Play)
    }

    fn font_namespace(self) -> &'static str {
        match self {
            SkinKind::Play => "play",
            SkinKind::Select => "select",
            SkinKind::Decide => "decide",
            SkinKind::Result => "result",
        }
    }
}

pub fn default_skin_document_path_from_paths(app_paths: &AppPaths, kind: SkinKind) -> PathBuf {
    let file_name = match kind {
        SkinKind::Play => "play7.json",
        SkinKind::Select => "select.json",
        SkinKind::Decide => "decide.json",
        SkinKind::Result => "result.json",
    };
    default_skin_root_from_paths(app_paths).join(file_name)
}

pub fn default_play_skin_document_path_from_paths(
    app_paths: &AppPaths,
    key_mode: KeyMode,
) -> PathBuf {
    let file_name = match key_mode {
        KeyMode::K4 => "play4.json",
        KeyMode::K5 => "play5.json",
        KeyMode::K6 => "play6.json",
        KeyMode::K7 => "play7.json",
        KeyMode::K8 => "play8.json",
        KeyMode::K9 => "play9.json",
        KeyMode::K10 => "play10.json",
        KeyMode::K14 => "play14.json",
    };
    default_skin_root_from_paths(app_paths).join(file_name)
}

/// バックグラウンドスレッドでデコード可能な 1 スキンぶんの中間データ。
/// Renderer に触らず Send-safe な値だけを保持する。
pub struct DecodedSkin {
    pub kind: SkinKind,
    pub document: SkinDocument,
    pub fonts: Vec<DecodedFont>,
    pub sources: Vec<DecodedSource>,
    pub stats: SkinDecodeStats,
}

#[derive(Debug, Clone, Default)]
pub struct SkinDecodeStats {
    pub document_us: u64,
    pub document_cache_hits: usize,
    pub document_cache_misses: usize,
    pub document_cache_uncacheable: usize,
    pub document_cache_disabled: usize,
    pub font_count: usize,
    pub font_decode_us: u64,
    pub font_payload_skipped: usize,
    pub font_cache_hits: usize,
    pub font_cache_misses: usize,
    pub font_cache_uncacheable: usize,
    pub font_cache_disabled: usize,
    pub source_task_count: usize,
    pub source_decode_us: u64,
    pub builtin_source_count: usize,
    pub image_source_count: usize,
    pub video_source_count: usize,
    pub source_cache_hits: usize,
    pub source_cache_misses: usize,
    pub source_cache_uncacheable: usize,
    pub source_cache_disabled: usize,
    pub video_source_cache_hits: usize,
    pub video_source_cache_misses: usize,
    pub video_source_cache_uncacheable: usize,
    pub video_source_cache_disabled: usize,
    pub source_texture_cache_hits: usize,
    pub video_source_texture_cache_hits: usize,
    pub source_texture_cache_hit_bytes: usize,
    pub video_source_texture_cache_hit_bytes: usize,
    pub decoded_source_count: usize,
    pub decoded_source_bytes: usize,
}

pub struct DecodedFont {
    pub stored_id: String,
    pub path: PathBuf,
    pub data: Option<DecodedFontData>,
    pub cache_key: Option<SkinFontCacheKey>,
}

#[derive(Clone)]
pub enum DecodedFontData {
    Vector(Vec<u8>),
    Bitmap(BitmapFont),
}

pub struct DecodedSource {
    pub source_id: String,
    pub path: PathBuf,
    pub texture: SkinTextureId,
    pub asset: Option<RgbaImageAsset>,
    pub size: SkinImageSize,
    pub cache_key: Option<SkinSourceAssetCacheKey>,
    pub is_video: bool,
}

pub type SharedSkinSourceAssetCache = Arc<Mutex<SkinSourceAssetCache>>;
pub type SharedSkinDocumentCache = Arc<Mutex<SkinDocumentCache>>;
pub type SharedSkinFontCache = Arc<Mutex<SkinFontCache>>;
pub type SharedSkinGpuTextureCache = Arc<Mutex<SkinGpuTextureCache>>;

const SKIN_DOCUMENT_CACHE_LIMIT_ENTRIES: usize = 16;
const SKIN_SOURCE_ASSET_CACHE_LIMIT_BYTES: usize = 256 * 1024 * 1024;
const SKIN_FONT_CACHE_LIMIT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Default)]
pub struct SkinDocumentCache {
    entries: Vec<SkinDocumentCacheEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SkinDocumentCacheKey {
    path: PathBuf,
    kind: SkinKind,
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkinDocumentDependencyFingerprint {
    number_values: BTreeMap<i32, i32>,
    text_values: BTreeMap<i32, String>,
    option_values: BTreeMap<i32, bool>,
    file_values: BTreeMap<String, String>,
    loaded_files: BTreeMap<PathBuf, SkinLoadedFileDependency>,
    virtual_io_files: BTreeMap<String, Option<String>>,
}

#[derive(Clone)]
struct SkinDocumentCacheEntry {
    key: SkinDocumentCacheKey,
    fingerprint: SkinDocumentDependencyFingerprint,
    document: SkinDocument,
    files: BTreeMap<String, String>,
    dependencies: SkinLoadDependencies,
}

impl SkinDocumentCache {
    fn get_lr2(
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

    fn get_lua(
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

    fn insert_lr2(
        &mut self,
        key: SkinDocumentCacheKey,
        fingerprint: SkinDocumentDependencyFingerprint,
        document: SkinDocument,
        files: BTreeMap<String, String>,
        dependencies: SkinLoadDependencies,
    ) {
        self.insert(key, fingerprint, document, files, dependencies);
    }

    fn insert_lua(
        &mut self,
        key: SkinDocumentCacheKey,
        fingerprint: SkinDocumentDependencyFingerprint,
        document: SkinDocument,
        files: BTreeMap<String, String>,
        dependencies: SkinLoadDependencies,
    ) {
        self.insert(key, fingerprint, document, files, dependencies);
    }

    fn insert(
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
    entries: HashMap<SkinSourceAssetCacheKey, RgbaImageAsset>,
    total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkinSourceAssetCacheKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    is_video: bool,
}

impl SkinSourceAssetCache {
    fn get(&self, key: &SkinSourceAssetCacheKey) -> Option<RgbaImageAsset> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: SkinSourceAssetCacheKey, asset: RgbaImageAsset) {
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
    entries: HashMap<SkinFontCacheKey, CachedSkinFontEntry>,
    total_bytes: usize,
    limit_bytes: usize,
    access_clock: u64,
}

struct CachedSkinFontEntry {
    data: DecodedFontData,
    bytes: usize,
    last_used: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkinFontCacheKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    is_bitmap: bool,
}

impl SkinFontCache {
    fn get(&mut self, key: &SkinFontCacheKey) -> Option<DecodedFontData> {
        let access = self.next_access();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = access;
        Some(entry.data.clone())
    }

    fn insert(&mut self, key: SkinFontCacheKey, data: DecodedFontData) {
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

    fn evict_until_fits(&mut self, incoming_bytes: usize) {
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

    fn next_access(&mut self) -> u64 {
        self.access_clock = self.access_clock.wrapping_add(1);
        self.access_clock
    }

    #[cfg(test)]
    fn with_limit_bytes(limit_bytes: usize) -> Self {
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

#[derive(Default)]
pub struct SkinGpuTextureCache {
    entries: HashMap<SkinSourceAssetCacheKey, CachedSkinGpuTexture>,
    next_texture_ids: HashMap<SkinKind, u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct CachedSkinGpuTexture {
    pub texture: SkinTextureId,
    pub size: SkinImageSize,
}

impl SkinGpuTextureCache {
    pub fn get(&self, key: &SkinSourceAssetCacheKey) -> Option<CachedSkinGpuTexture> {
        self.entries.get(key).copied()
    }

    pub fn insert(
        &mut self,
        key: SkinSourceAssetCacheKey,
        texture: SkinTextureId,
        size: SkinImageSize,
    ) {
        self.entries.insert(key, CachedSkinGpuTexture { texture, size });
    }

    fn allocate_texture_id(&mut self, kind: SkinKind) -> SkinTextureId {
        let next = self.next_texture_ids.entry(kind).or_insert_with(|| kind.first_texture_id());
        let texture = SkinTextureId(*next);
        *next = next.saturating_add(1);
        texture
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.next_texture_ids.clear();
    }
}

enum SourceDecodeTask {
    File { index: usize, source_id: String, path: PathBuf },
    Video { index: usize, source_id: String, path: PathBuf },
    Builtin { index: usize, source_id: String, path: PathBuf, asset: RgbaImageAsset },
}

struct DecodedSourceResult {
    index: usize,
    source_id: String,
    path: PathBuf,
    asset: Option<RgbaImageAsset>,
    size: SkinImageSize,
    is_video: bool,
    cached_texture: Option<SkinTextureId>,
    cache_key: Option<SkinSourceAssetCacheKey>,
    source_status: Option<SourceCacheStatus>,
    texture_status: Option<TextureCacheStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceCacheStatus {
    Hit,
    Miss,
    Uncacheable,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextureCacheStatus {
    Hit,
    Miss,
    Uncacheable,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontCacheStatus {
    Hit,
    Miss,
    SkippedInstalled,
    Uncacheable,
    Disabled,
}

/// GPU アップロード済みの 1 ソース。upload worker が `DecodedSource` から生成する。
pub struct PreparedSource {
    pub source_id: String,
    pub path: PathBuf,
    pub texture: SkinTextureId,
    pub prepared: Option<PreparedTexture>,
    pub size: SkinImageSize,
    pub is_video: bool,
    pub cache_key: Option<SkinSourceAssetCacheKey>,
}

/// decode + GPU アップロードまで終わった 1 スキンぶん。upload worker → main で渡す。
/// `PreparedTexture` (= wgpu::Texture/View) は `Send` なのでスレッド間で受け渡せる。
pub struct UploadedSkin {
    pub kind: SkinKind,
    pub document: SkinDocument,
    pub fonts: Vec<DecodedFont>,
    pub prepared: Vec<PreparedSource>,
    pub decode_stats: SkinDecodeStats,
    pub upload_stats: SkinUploadStats,
}

#[derive(Debug, Clone, Default)]
pub struct SkinUploadStats {
    pub upload_us: u64,
    pub source_count: usize,
    pub texture_cache_hits: usize,
    pub texture_cache_misses: usize,
    pub texture_cache_uncacheable: usize,
    pub texture_cache_disabled: usize,
    pub video_texture_cache_hits: usize,
    pub video_texture_cache_misses: usize,
    pub video_texture_cache_uncacheable: usize,
    pub video_texture_cache_disabled: usize,
    pub uploaded_source_count: usize,
    pub uploaded_source_bytes: usize,
    pub uploaded_video_source_count: usize,
    pub uploaded_video_source_bytes: usize,
}

/// `DecodedSkin` の全ソースを GPU へアップロードして `UploadedSkin` を返す。
/// upload worker スレッドから呼ぶ (`uploader` は `Renderer::gpu_uploader` の clone)。
pub fn upload_decoded_skin(uploader: &GpuUploader, decoded: DecodedSkin) -> UploadedSkin {
    upload_decoded_skin_with_texture_cache(uploader, decoded, None)
}

pub fn upload_decoded_skin_with_texture_cache(
    uploader: &GpuUploader,
    decoded: DecodedSkin,
    texture_cache: Option<&SharedSkinGpuTextureCache>,
) -> UploadedSkin {
    let upload_start = Instant::now();
    let DecodedSkin { kind, document, fonts, sources, stats: decode_stats } = decoded;
    let mut upload_stats = SkinUploadStats::default();
    let prepared = sources
        .into_iter()
        .filter_map(|source| {
            upload_stats.source_count += 1;
            let DecodedSource { source_id, path, texture, asset, size, cache_key, is_video } =
                source;
            let Some(asset) = asset else {
                upload_stats.texture_cache_hits += 1;
                if is_video {
                    upload_stats.video_texture_cache_hits += 1;
                }
                return Some(PreparedSource {
                    source_id,
                    path,
                    texture,
                    prepared: None,
                    size,
                    is_video,
                    cache_key: None,
                });
            };
            if let Err(error) = asset.validate() {
                tracing::warn!(
                    source_id = %source_id,
                    path = %path.display(),
                    %error,
                    "skipping invalid beatoraja skin source"
                );
                return None;
            }
            match (texture_cache, cache_key.as_ref()) {
                (Some(texture_cache), Some(cache_key)) => {
                    if let Ok(cache) = texture_cache.lock()
                        && let Some(cached) = cache.get(cache_key)
                    {
                        upload_stats.texture_cache_hits += 1;
                        if is_video {
                            upload_stats.video_texture_cache_hits += 1;
                        }
                        return Some(PreparedSource {
                            source_id,
                            path,
                            texture: cached.texture,
                            prepared: None,
                            size: cached.size,
                            is_video,
                            cache_key: None,
                        });
                    }
                    upload_stats.texture_cache_misses += 1;
                    if is_video {
                        upload_stats.video_texture_cache_misses += 1;
                    }
                }
                (Some(_), None) => {
                    upload_stats.texture_cache_uncacheable += 1;
                    if is_video {
                        upload_stats.video_texture_cache_uncacheable += 1;
                    }
                }
                (None, _) => {
                    upload_stats.texture_cache_disabled += 1;
                    if is_video {
                        upload_stats.video_texture_cache_disabled += 1;
                    }
                }
            }
            let texture = texture_cache
                .and_then(|cache| {
                    cache.lock().ok().map(|mut cache| cache.allocate_texture_id(kind))
                })
                .unwrap_or(texture);
            upload_stats.uploaded_source_count += 1;
            upload_stats.uploaded_source_bytes =
                upload_stats.uploaded_source_bytes.saturating_add(asset.pixels.len());
            if is_video {
                upload_stats.uploaded_video_source_count += 1;
                upload_stats.uploaded_video_source_bytes =
                    upload_stats.uploaded_video_source_bytes.saturating_add(asset.pixels.len());
            }
            let prepared = uploader.upload(asset.width, asset.height, &asset.pixels);
            Some(PreparedSource {
                source_id,
                path,
                texture,
                prepared: Some(prepared),
                size,
                is_video,
                cache_key,
            })
        })
        .collect();
    upload_stats.upload_us = elapsed_us(upload_start);
    UploadedSkin { kind, document, fonts, prepared, decode_stats, upload_stats }
}

pub fn default_skin_root() -> PathBuf {
    resolve_app_paths()
        .map(|paths| paths.default_skin_root())
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default"))
}

pub fn default_skin_root_from_paths(app_paths: &AppPaths) -> PathBuf {
    app_paths.default_skin_root()
}

pub fn apply_default_skin(renderer: &mut Renderer) -> Result<()> {
    let app_paths = resolve_app_paths()?;
    apply_default_skin_from_paths(renderer, &app_paths)
}

pub fn apply_default_skin_from_paths(renderer: &mut Renderer, app_paths: &AppPaths) -> Result<()> {
    let manifest = load_default_skin_into_renderer_from_paths(renderer, app_paths)?;
    let skin_path = default_play_skin_document_path_from_paths(app_paths, KeyMode::K7);
    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play)?;
    install_decoded_skin(renderer, decoded, manifest)
}

/// `profile.toml` の `[skin] play` 設定からスキンをロードする。
/// 空文字列 → デフォルト JSON スキン、`.json`/`.luaskin`/`.lua`/`.lr2skin`
/// 拡張子 → beatoraja スキンとして扱う。BMZ TOML skin directory は非対応。
pub fn apply_skin_from_config(
    renderer: &mut Renderer,
    app_paths: &AppPaths,
    play_skin_path: &str,
) -> Result<()> {
    if play_skin_path.is_empty() {
        return apply_default_skin_from_paths(renderer, app_paths);
    }
    let path = app_paths.resolve_path_ref(play_skin_path)?;
    if is_decodable_skin_path(&path) {
        apply_beatoraja_json_skin(renderer, &path)
    } else {
        anyhow::bail!(
            "unsupported skin path (BMZ TOML skin directories are no longer supported): {}",
            path.display()
        )
    }
}

pub fn apply_beatoraja_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Play)
}

pub fn apply_beatoraja_select_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Select)
}

pub fn apply_beatoraja_result_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Result)
}

pub fn apply_beatoraja_decide_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Decide)
}

fn apply_beatoraja_json_skin_for_kind(
    renderer: &mut Renderer,
    skin_path: &Path,
    kind: SkinKind,
) -> Result<()> {
    let manifest = load_default_skin_into_renderer(renderer)?;
    let decoded = decode_beatoraja_skin(skin_path, kind)?;
    install_decoded_skin(renderer, decoded, manifest)
}

/// デフォルトスキンの manifest と PNG テクスチャを renderer に取り込む。
/// 起動時に 1 回だけ呼ばれることを想定 (同じテクスチャを複数回 upsert しても害は無いが無駄)。
pub fn load_default_skin_into_renderer(renderer: &mut Renderer) -> Result<SkinManifest> {
    let default_root = default_skin_root();
    load_default_skin_root_into_renderer(renderer, &default_root)
}

pub fn load_default_skin_into_renderer_from_paths(
    renderer: &mut Renderer,
    app_paths: &AppPaths,
) -> Result<SkinManifest> {
    let default_root = default_skin_root_from_paths(app_paths);
    load_default_skin_root_into_renderer(renderer, &default_root)
}

fn load_default_skin_root_into_renderer(
    renderer: &mut Renderer,
    default_root: &Path,
) -> Result<SkinManifest> {
    let manifest = default_skin_manifest_for_root(default_root);

    for texture in manifest.resolve_textures(default_root) {
        renderer.load_png_texture(texture.id, &texture.path).with_context(|| {
            format!(
                "failed to load default skin texture {}: {}",
                texture.id.0,
                texture.path.display()
            )
        })?;
    }
    Ok(manifest)
}

/// beatoraja JSON skin の document/フォント/PNG ソースを並列にデコードする。
/// Renderer には触らないので Send-safe で、別スレッドからも呼べる。
pub fn decode_beatoraja_skin(skin_path: &Path, kind: SkinKind) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options(skin_path, kind, &BTreeMap::new(), &BTreeMap::new())
}

/// `decode_beatoraja_skin` のカスタマイズオプション / ファイル選択付き版。
///
/// `options` はオプション名 -> 選択肢名の対応。JSON スキンは選択肢の `op`
/// コード列へ、Lua スキンはそのまま渡して展開する。
///
/// `files` は filepath 定義名 -> 選択ファイルのスキンルート相対パスの対応。
/// Lua スキンは `skin_config.get_path` の解決へ、JSON スキンは `source` /
/// `font` のワイルドカード解決へ反映する。
pub fn decode_beatoraja_skin_with_options(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options_and_runtime_state(
        skin_path,
        kind,
        options,
        files,
        &LuaLoadRuntimeState::default(),
    )
}

pub fn decode_beatoraja_skin_with_options_and_runtime_state(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options_and_runtime_state_and_source_cache(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        None,
        None,
    )
}

pub fn decode_beatoraja_skin_with_options_and_runtime_state_and_source_cache(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    source_cache: Option<SharedSkinSourceAssetCache>,
    font_cache: Option<SharedSkinFontCache>,
) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        skin_path,
        kind,
        options,
        files,
        runtime_state,
        None,
        source_cache,
        None,
        font_cache,
        None,
    )
}

pub fn decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    document_cache: Option<SharedSkinDocumentCache>,
    source_cache: Option<SharedSkinSourceAssetCache>,
    texture_cache: Option<SharedSkinGpuTextureCache>,
    font_cache: Option<SharedSkinFontCache>,
    installed_fonts: Option<HashMap<String, SkinFontCacheKey>>,
) -> Result<DecodedSkin> {
    let document_start = Instant::now();
    let LoadedSkinDocumentForDecode { mut document, files: resolved_files, cache_status } =
        load_skin_document(skin_path, kind, options, files, runtime_state, document_cache)?;
    let document_us = elapsed_us(document_start);
    // フォント ID は scene 横断的に Renderer のグローバルマップに登録されるので、
    // play / select / result で同じ "0" 等が衝突する。namespace を付与して隔離する。
    // text 定義の font 参照側も同じ namespace を付ける。
    let font_namespace = kind.font_namespace();
    for text in &mut document.text {
        if !text.font.is_empty() {
            text.font = format!("{}:{}", font_namespace, text.font);
        }
    }
    let skin_root = skin_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let required_sources: HashSet<String> =
        required_skin_source_ids(&document).into_iter().map(str::to_string).collect();
    let warn_missing_required = kind.warn_missing_required_sources();

    // フォントを並列にデコードする。
    let font_tasks: Vec<_> = document
        .font
        .iter()
        .filter_map(|font| {
            if font.id.is_empty() || font.path.is_empty() {
                return None;
            }
            let font_path =
                resolve_json_skin_asset_path(&skin_root, &font.path, &document, &resolved_files)?;
            if !is_supported_font_path(&font_path) {
                tracing::debug!(
                    font_id = %font.id,
                    path = %font_path.display(),
                    "skipping unsupported beatoraja skin font"
                );
                return None;
            }
            let stored_id = format!("{}:{}", font_namespace, font.id);
            Some((stored_id, font_path))
        })
        .collect();

    let font_count = font_tasks.len();
    let font_decode_start = Instant::now();
    let decoded_fonts: Vec<(DecodedFont, FontCacheStatus)> = font_tasks
        .into_par_iter()
        .filter_map(|(stored_id, font_path)| {
            let cache_key = skin_font_cache_key(&font_path);
            if let (Some(installed_fonts), Some(cache_key)) =
                (installed_fonts.as_ref(), cache_key.as_ref())
                && installed_fonts.get(&stored_id) == Some(cache_key)
            {
                return Some((
                    DecodedFont {
                        stored_id,
                        path: font_path,
                        data: None,
                        cache_key: Some(cache_key.clone()),
                    },
                    FontCacheStatus::SkippedInstalled,
                ));
            }
            match decode_font_with_cache_key(&font_path, font_cache.as_ref(), cache_key) {
                Ok((data, status, cache_key)) => Some((
                    DecodedFont { stored_id, path: font_path, data: Some(data), cache_key },
                    status,
                )),
                Err(error) => {
                    tracing::warn!(
                        font_id = %stored_id,
                        path = %font_path.display(),
                        %error,
                        "failed to load beatoraja skin font"
                    );
                    None
                }
            }
        })
        .collect();
    let font_decode_us = elapsed_us(font_decode_start);
    let mut font_payload_skipped = 0;
    let mut font_cache_hits = 0;
    let mut font_cache_misses = 0;
    let mut font_cache_uncacheable = 0;
    let mut font_cache_disabled = 0;
    for (_, status) in &decoded_fonts {
        match status {
            FontCacheStatus::Hit => font_cache_hits += 1,
            FontCacheStatus::Miss => font_cache_misses += 1,
            FontCacheStatus::SkippedInstalled => font_payload_skipped += 1,
            FontCacheStatus::Uncacheable => font_cache_uncacheable += 1,
            FontCacheStatus::Disabled => font_cache_disabled += 1,
        }
    }
    let fonts: Vec<DecodedFont> = decoded_fonts.into_iter().map(|(font, _)| font).collect();

    // ソースは ID 順を保つため、まず resolved path リストを順次組み立て、
    // PNG/動画先頭フレームのデコード本体だけを並列実行する。
    let source_tasks: Vec<SourceDecodeTask> = document
        .source
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            if let Some(asset) = lr2_builtin_source_asset(&source.path) {
                return Some(SourceDecodeTask::Builtin {
                    index,
                    source_id: source.id.clone(),
                    path: PathBuf::from(&source.path),
                    asset,
                });
            }
            let source_path = resolve_json_skin_source_path(
                &skin_root,
                &source.path,
                &document,
                &resolved_files,
            )?;
            let extension = source_path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if extension == "png" {
                return Some(SourceDecodeTask::File {
                    index,
                    source_id: source.id.clone(),
                    path: source_path,
                });
            }
            if is_skin_video_source_extension(&extension) {
                return Some(SourceDecodeTask::Video {
                    index,
                    source_id: source.id.clone(),
                    path: source_path,
                });
            }
            {
                tracing::debug!(
                    source_id = %source.id,
                    path = %source_path.display(),
                    "skipping unsupported beatoraja skin source"
                );
                None
            }
        })
        .collect();

    let source_task_count = source_tasks.len();
    let source_decode_start = Instant::now();
    let mut decoded_pairs: Vec<DecodedSourceResult> = source_tasks
        .into_par_iter()
        .filter_map(|task| match task {
            SourceDecodeTask::Builtin { index, source_id, path, asset } => {
                let size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
                Some(DecodedSourceResult {
                    index,
                    source_id,
                    path,
                    asset: Some(asset),
                    size,
                    is_video: false,
                    cached_texture: None,
                    cache_key: None,
                    source_status: None,
                    texture_status: None,
                })
            }
            SourceDecodeTask::File { index, source_id, path: source_path } => {
                let (cached_texture, cache_key, texture_status) =
                    lookup_source_texture_cache(texture_cache.as_ref(), &source_path, false);
                if let Some(cached_texture) = cached_texture {
                    return Some(DecodedSourceResult {
                        index,
                        source_id,
                        path: source_path,
                        asset: None,
                        size: cached_texture.size,
                        is_video: false,
                        cached_texture: Some(cached_texture.texture),
                        cache_key,
                        source_status: None,
                        texture_status: Some(texture_status),
                    });
                }
                match load_source_asset_with_cache(
                    &source_path,
                    false,
                    source_cache.as_ref(),
                    || load_png_rgba(&source_path),
                ) {
                    Ok((asset, status)) => {
                        let size = SkinImageSize {
                            width: asset.width as f32,
                            height: asset.height as f32,
                        };
                        Some(DecodedSourceResult {
                            index,
                            source_id,
                            path: source_path,
                            asset: Some(asset),
                            size,
                            is_video: false,
                            cached_texture: None,
                            cache_key,
                            source_status: Some(status),
                            texture_status: Some(texture_status),
                        })
                    }
                    Err(error) => {
                        if warn_missing_required && required_sources.contains(&source_id) {
                            tracing::warn!(
                                source_id = %source_id,
                                path = %source_path.display(),
                                %error,
                                "failed to load beatoraja skin source"
                            );
                        } else {
                            tracing::debug!(
                                source_id = %source_id,
                                path = %source_path.display(),
                                %error,
                                "skipping unused missing beatoraja skin source"
                            );
                        }
                        None
                    }
                }
            }
            SourceDecodeTask::Video { index, source_id, path: source_path } => {
                let (cached_texture, cache_key, texture_status) =
                    lookup_source_texture_cache(texture_cache.as_ref(), &source_path, true);
                if let Some(cached_texture) = cached_texture {
                    return Some(DecodedSourceResult {
                        index,
                        source_id,
                        path: source_path,
                        asset: None,
                        size: cached_texture.size,
                        is_video: true,
                        cached_texture: Some(cached_texture.texture),
                        cache_key,
                        source_status: None,
                        texture_status: Some(texture_status),
                    });
                }
                match load_source_asset_with_cache(
                    &source_path,
                    true,
                    source_cache.as_ref(),
                    || load_skin_video_first_frame_rgba(&source_path),
                ) {
                    Ok((asset, status)) => {
                        let size = SkinImageSize {
                            width: asset.width as f32,
                            height: asset.height as f32,
                        };
                        Some(DecodedSourceResult {
                            index,
                            source_id,
                            path: source_path,
                            asset: Some(asset),
                            size,
                            is_video: true,
                            cached_texture: None,
                            cache_key,
                            source_status: Some(status),
                            texture_status: Some(texture_status),
                        })
                    }
                    Err(error) => {
                        tracing::warn!(
                            source_id = %source_id,
                            path = %source_path.display(),
                            %error,
                            "failed to load beatoraja skin video source"
                        );
                        None
                    }
                }
            }
        })
        .collect();
    let source_decode_us = elapsed_us(source_decode_start);
    decoded_pairs.sort_by_key(|decoded| decoded.index);

    let mut stats = SkinDecodeStats {
        document_us,
        document_cache_hits: usize::from(cache_status == DocumentCacheStatus::Hit),
        document_cache_misses: usize::from(cache_status == DocumentCacheStatus::Miss),
        document_cache_uncacheable: usize::from(cache_status == DocumentCacheStatus::Uncacheable),
        document_cache_disabled: usize::from(cache_status == DocumentCacheStatus::Disabled),
        font_count,
        font_decode_us,
        font_payload_skipped,
        font_cache_hits,
        font_cache_misses,
        font_cache_uncacheable,
        font_cache_disabled,
        source_task_count,
        source_decode_us,
        ..Default::default()
    };
    for decoded in &decoded_pairs {
        stats.decoded_source_count += 1;
        if let Some(asset) = &decoded.asset {
            stats.decoded_source_bytes =
                stats.decoded_source_bytes.saturating_add(asset.pixels.len());
        }
        if matches!(decoded.texture_status, Some(TextureCacheStatus::Hit)) {
            stats.source_texture_cache_hits += 1;
            let bytes = (decoded.size.width.max(0.0) as usize)
                .saturating_mul(decoded.size.height.max(0.0) as usize)
                .saturating_mul(4);
            stats.source_texture_cache_hit_bytes =
                stats.source_texture_cache_hit_bytes.saturating_add(bytes);
            if decoded.is_video {
                stats.video_source_texture_cache_hits += 1;
                stats.video_source_texture_cache_hit_bytes =
                    stats.video_source_texture_cache_hit_bytes.saturating_add(bytes);
            }
        }
        match (decoded.is_video, &decoded.source_status, &decoded.texture_status) {
            (_, None, None) => stats.builtin_source_count += 1,
            (true, None, Some(TextureCacheStatus::Hit)) => stats.video_source_count += 1,
            (false, None, Some(TextureCacheStatus::Hit)) => stats.image_source_count += 1,
            (true, Some(_), _) => stats.video_source_count += 1,
            (false, Some(_), _) => stats.image_source_count += 1,
            (_, None, Some(_)) => {}
        }
        match decoded.source_status {
            Some(SourceCacheStatus::Hit) => {
                stats.source_cache_hits += 1;
                if decoded.is_video {
                    stats.video_source_cache_hits += 1;
                }
            }
            Some(SourceCacheStatus::Miss) => {
                stats.source_cache_misses += 1;
                if decoded.is_video {
                    stats.video_source_cache_misses += 1;
                }
            }
            Some(SourceCacheStatus::Uncacheable) => {
                stats.source_cache_uncacheable += 1;
                if decoded.is_video {
                    stats.video_source_cache_uncacheable += 1;
                }
            }
            Some(SourceCacheStatus::Disabled) => {
                stats.source_cache_disabled += 1;
                if decoded.is_video {
                    stats.video_source_cache_disabled += 1;
                }
            }
            None => {}
        }
    }

    let mut next_texture_id = kind.first_texture_id();
    let sources: Vec<DecodedSource> = decoded_pairs
        .into_iter()
        .map(|decoded| {
            let texture = decoded.cached_texture.unwrap_or_else(|| {
                let texture = SkinTextureId(next_texture_id);
                next_texture_id += 1;
                texture
            });
            DecodedSource {
                source_id: decoded.source_id,
                path: decoded.path,
                texture,
                asset: decoded.asset,
                size: decoded.size,
                cache_key: decoded.cache_key,
                is_video: decoded.is_video,
            }
        })
        .collect();

    Ok(DecodedSkin { kind, document, fonts, sources, stats })
}

fn lr2_builtin_source_asset(path: &str) -> Option<RgbaImageAsset> {
    let pixel = match path {
        "bmz://lr2/black" => [0, 0, 0, 255],
        "bmz://lr2/white" => [255, 255, 255, 255],
        // BACKBMP itself is drawn by the play snapshot path.  Keep a transparent
        // source so LR2 CSV objects using IMAGE_BACKBMP can be decoded without
        // failing texture resolution when the chart has no backbmp.
        "bmz://lr2/backbmp" => [0, 0, 0, 0],
        _ => return None,
    };
    Some(RgbaImageAsset { width: 1, height: 1, pixels: pixel.to_vec() })
}

fn is_skin_video_source_extension(extension: &str) -> bool {
    matches!(extension, "mp4" | "wmv" | "m4v" | "webm" | "mpg" | "mpeg" | "m1v" | "m2v" | "avi")
}

fn load_source_asset_with_cache<F>(
    path: &Path,
    is_video: bool,
    source_cache: Option<&SharedSkinSourceAssetCache>,
    load: F,
) -> Result<(RgbaImageAsset, SourceCacheStatus)>
where
    F: FnOnce() -> Result<RgbaImageAsset>,
{
    let Some(source_cache) = source_cache else {
        return load().map(|asset| (asset, SourceCacheStatus::Disabled));
    };
    let Some(key) = skin_source_asset_cache_key(path, is_video) else {
        return load().map(|asset| (asset, SourceCacheStatus::Uncacheable));
    };
    if let Ok(cache) = source_cache.lock()
        && let Some(asset) = cache.get(&key)
    {
        return Ok((asset, SourceCacheStatus::Hit));
    }
    let asset = load()?;
    if let Ok(mut cache) = source_cache.lock() {
        cache.insert(key, asset.clone());
    }
    Ok((asset, SourceCacheStatus::Miss))
}

fn lookup_source_texture_cache(
    texture_cache: Option<&SharedSkinGpuTextureCache>,
    path: &Path,
    is_video: bool,
) -> (Option<CachedSkinGpuTexture>, Option<SkinSourceAssetCacheKey>, TextureCacheStatus) {
    let key = skin_source_asset_cache_key(path, is_video);
    match (texture_cache, key.as_ref()) {
        (Some(texture_cache), Some(key)) => {
            if let Ok(cache) = texture_cache.lock()
                && let Some(texture) = cache.get(key)
            {
                return (Some(texture), Some(key.clone()), TextureCacheStatus::Hit);
            }
            (None, Some(key.clone()), TextureCacheStatus::Miss)
        }
        (Some(_), None) => (None, None, TextureCacheStatus::Uncacheable),
        (None, _) => (None, key, TextureCacheStatus::Disabled),
    }
}

fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn skin_source_asset_cache_key(path: &Path, is_video: bool) -> Option<SkinSourceAssetCacheKey> {
    let metadata = fs::metadata(path).ok()?;
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some(SkinSourceAssetCacheKey {
        path,
        modified: metadata.modified().ok(),
        len: metadata.len(),
        is_video,
    })
}

fn skin_document_cache_key(path: &Path, kind: SkinKind) -> Option<SkinDocumentCacheKey> {
    let metadata = fs::metadata(path).ok()?;
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some(SkinDocumentCacheKey {
        path,
        kind,
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

/// beatoraja の設定ファイルを読む Lua スキン向けの、個人情報を含まない読取専用設定。
///
/// ホスト側の beatoraja 設定や BMZ の入力割当は公開せず、入力監視は BMZ のイベント処理を
/// 正とする。各 mode の空設定は WMII の設定探索を安全に完了させるためだけに供給する。
fn lua_compat_virtual_io_files() -> BTreeMap<String, String> {
    const PLAYER_CONFIG: &str = concat!(
        "{",
        "\"mode5\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode7\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode9\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode10\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode14\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode24\":{\"keyboard\":{},\"controller\":[],\"midi\":{}},",
        "\"mode24double\":{\"keyboard\":{},\"controller\":[],\"midi\":{}}",
        "}",
    );
    BTreeMap::from([
        ("config_sys.json".to_string(), "{\"playername\":\"bmz\"}".to_string()),
        ("player/bmz/config_player.json".to_string(), PLAYER_CONFIG.to_string()),
    ])
}

fn lua_virtual_io_files(runtime_state: &LuaLoadRuntimeState) -> BTreeMap<String, String> {
    let mut files = lua_compat_virtual_io_files();
    files.extend(runtime_state.virtual_io_files.clone());
    files
}

fn lr2_document_dependency_fingerprint(
    skin_path: &Path,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    dependencies: &SkinLoadDependencies,
) -> Result<SkinDocumentDependencyFingerprint> {
    let option_values = bmz_skin::load_lr2_csv_skin_dependency_option_values(
        skin_path,
        options,
        dependencies.option_values.keys().copied(),
    )?;
    let file_values = dependencies
        .files
        .iter()
        .map(|name| (name.clone(), files.get(name).cloned().unwrap_or_default()))
        .collect();
    let loaded_files = current_loaded_file_dependencies(&dependencies.loaded_files)
        .context("failed to inspect lr2 skin loaded file dependencies")?;
    Ok(SkinDocumentDependencyFingerprint {
        number_values: BTreeMap::new(),
        text_values: BTreeMap::new(),
        option_values,
        file_values,
        loaded_files,
        virtual_io_files: BTreeMap::new(),
    })
}

fn document_dependency_fingerprint(
    document: &SkinDocument,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    dependencies: &SkinLoadDependencies,
) -> Option<SkinDocumentDependencyFingerprint> {
    let enabled_options = enabled_options_from_selections(document, options);
    let property_ops = document_property_ops(document);
    let number_values = dependencies
        .number_values
        .keys()
        .map(|ref_id| {
            let value = runtime_state.number_values.get(ref_id).copied().unwrap_or_default();
            (*ref_id, value)
        })
        .collect();
    let text_values = dependencies
        .text_values
        .keys()
        .map(|ref_id| {
            let value = runtime_state.text_values.get(ref_id).cloned().unwrap_or_default();
            (*ref_id, value)
        })
        .collect();
    let option_values = dependencies
        .option_values
        .keys()
        .map(|option_id| {
            let value = if property_ops.contains(option_id) {
                enabled_options.contains(option_id)
            } else {
                runtime_state.option_values.get(option_id).copied().unwrap_or(false)
            };
            (*option_id, value)
        })
        .collect();
    let file_values = dependencies
        .files
        .iter()
        .map(|name| (name.clone(), files.get(name).cloned().unwrap_or_default()))
        .collect();
    let loaded_files = current_loaded_file_dependencies(&dependencies.loaded_files).ok()?;
    let virtual_files = lua_virtual_io_files(runtime_state);
    let virtual_io_files = dependencies
        .virtual_io_files
        .keys()
        .map(|path| (path.clone(), virtual_files.get(path).cloned()))
        .collect();
    Some(SkinDocumentDependencyFingerprint {
        number_values,
        text_values,
        option_values,
        file_values,
        loaded_files,
        virtual_io_files,
    })
}

fn document_property_ops(document: &SkinDocument) -> HashSet<i32> {
    document.property.iter().flat_map(|property| property.item.iter().map(|item| item.op)).collect()
}

fn current_loaded_file_dependencies(
    loaded_files: &BTreeMap<PathBuf, SkinLoadedFileDependency>,
) -> Result<BTreeMap<PathBuf, SkinLoadedFileDependency>> {
    let mut result = BTreeMap::new();
    for path in loaded_files.keys() {
        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to read loaded lua skin file: {}", path.display()))?;
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        result.insert(
            path,
            SkinLoadedFileDependency { modified: metadata.modified().ok(), len: metadata.len() },
        );
    }
    Ok(result)
}

fn load_skin_video_first_frame_rgba(path: &Path) -> Result<RgbaImageAsset> {
    let frame = bmz_video::decode_first_frame(path)
        .with_context(|| format!("failed to decode first video frame: {}", path.display()))?;
    Ok(RgbaImageAsset { width: frame.width, height: frame.height, pixels: frame.rgba })
}

struct LoadedSkinDocumentForDecode {
    document: SkinDocument,
    files: BTreeMap<String, String>,
    cache_status: DocumentCacheStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentCacheStatus {
    Hit,
    Miss,
    Uncacheable,
    Disabled,
}

fn load_skin_document(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
    document_cache: Option<SharedSkinDocumentCache>,
) -> Result<LoadedSkinDocumentForDecode> {
    if is_lr2_skin_path(skin_path)
        && let Some(document_cache) = document_cache.as_ref()
        && let Some(key) = skin_document_cache_key(skin_path, kind)
    {
        if let Ok(mut cache) = document_cache.lock()
            && let Some((mut document, mut resolved_files)) =
                cache.get_lr2(&key, skin_path, options, files)
        {
            for (name, selected) in files {
                resolved_files.insert(name.clone(), selected.clone());
            }
            document.user_selected_options =
                Some(enabled_options_from_selections(&document, options));
            return Ok(LoadedSkinDocumentForDecode {
                document,
                files: resolved_files,
                cache_status: DocumentCacheStatus::Hit,
            });
        }
        let mut loaded =
            load_skin_document_uncached(skin_path, kind, options, files, runtime_state)?;
        loaded.cache_status = DocumentCacheStatus::Miss;
        if let Ok(mut cache) = document_cache.lock()
            && let Ok(fingerprint) =
                lr2_document_dependency_fingerprint(skin_path, options, files, &loaded.dependencies)
        {
            cache.insert_lr2(
                key,
                fingerprint,
                loaded.document.clone(),
                loaded.files.clone(),
                loaded.dependencies,
            );
        }
        return Ok(LoadedSkinDocumentForDecode {
            document: loaded.document,
            files: loaded.files,
            cache_status: loaded.cache_status,
        });
    }
    if is_lua_skin_path(skin_path)
        && let Some(document_cache) = document_cache.as_ref()
        && let Some(key) = skin_document_cache_key(skin_path, kind)
    {
        if let Ok(mut cache) = document_cache.lock()
            && let Some((mut document, mut resolved_files)) =
                cache.get_lua(&key, options, files, runtime_state)
        {
            for (name, selected) in files {
                resolved_files.insert(name.clone(), selected.clone());
            }
            document.user_selected_options =
                Some(enabled_options_from_selections(&document, options));
            return Ok(LoadedSkinDocumentForDecode {
                document,
                files: resolved_files,
                cache_status: DocumentCacheStatus::Hit,
            });
        }
        let mut loaded =
            load_skin_document_uncached(skin_path, kind, options, files, runtime_state)?;
        loaded.cache_status = DocumentCacheStatus::Miss;
        if let Ok(mut cache) = document_cache.lock()
            && let Some(fingerprint) = document_dependency_fingerprint(
                &loaded.document,
                options,
                files,
                runtime_state,
                &loaded.dependencies,
            )
        {
            cache.insert_lua(
                key,
                fingerprint,
                loaded.document.clone(),
                loaded.files.clone(),
                loaded.dependencies,
            );
        }
        return Ok(LoadedSkinDocumentForDecode {
            document: loaded.document,
            files: loaded.files,
            cache_status: loaded.cache_status,
        });
    }

    let cache_status = if document_cache.is_some() {
        DocumentCacheStatus::Uncacheable
    } else {
        DocumentCacheStatus::Disabled
    };
    let mut loaded = load_skin_document_uncached(skin_path, kind, options, files, runtime_state)?;
    loaded.cache_status = cache_status;
    Ok(LoadedSkinDocumentForDecode {
        document: loaded.document,
        files: loaded.files,
        cache_status: loaded.cache_status,
    })
}

struct LoadedSkinDocumentWithDependencies {
    document: SkinDocument,
    files: BTreeMap<String, String>,
    dependencies: SkinLoadDependencies,
    cache_status: DocumentCacheStatus,
}

fn load_skin_document_uncached(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &LuaLoadRuntimeState,
) -> Result<LoadedSkinDocumentWithDependencies> {
    let (mut document, mut resolved_files, dependencies) = if is_lua_skin_path(skin_path) {
        // Lua スキンはオプション選択 (名前 -> 選択肢名) とファイル選択
        // (filepath 定義名 -> 相対パス) をそのまま渡す。
        let virtual_io_files = lua_virtual_io_files(runtime_state);
        let loaded = bmz_skin::load_lua_skin_with_runtime_state_and_virtual_io_files(
            skin_path,
            options,
            files,
            runtime_state,
            &virtual_io_files,
        )
        .with_context(|| format!("failed to load lua skin: {}", skin_path.display()))?;
        for warning in loaded.warnings {
            tracing::warn!(
                path = %skin_path.display(),
                kind = ?kind,
                warning = %warning.message,
                "lua skin load warning"
            );
        }
        (loaded.document, loaded.files, loaded.dependencies)
    } else if is_lr2_skin_path(skin_path) {
        let loaded = bmz_skin::load_lr2_csv_skin(skin_path, decode_skin_kind(kind), options, files)
            .with_context(|| format!("failed to load lr2 csv skin: {}", skin_path.display()))?;
        for warning in loaded.warnings {
            tracing::warn!(
                path = %skin_path.display(),
                kind = ?kind,
                warning = %warning.message,
                "lr2 csv skin load warning"
            );
        }
        (loaded.document, BTreeMap::new(), loaded.dependencies)
    } else {
        let document =
            bmz_skin::load_beatoraja_json_skin_with_defaults(skin_path).with_context(|| {
                format!("failed to load beatoraja json skin: {}", skin_path.display())
            })?;
        if options.is_empty() {
            (document, BTreeMap::new(), SkinLoadDependencies::default())
        } else {
            // JSON スキンは property 定義から選択肢の op コード列を組み立て、
            // それを有効オプションとして再デコードする。
            let enabled = enabled_options_from_selections(&document, options);
            let document =
                bmz_skin::load_beatoraja_json_skin(skin_path, &enabled).with_context(|| {
                    format!(
                        "failed to load beatoraja json skin with options: {}",
                        skin_path.display()
                    )
                })?;
            (document, BTreeMap::new(), SkinLoadDependencies::default())
        }
    };
    for (name, selected) in files {
        resolved_files.insert(name.clone(), selected.clone());
    }
    // レンダー時の `enabled_options()` がユーザ選択を反映するように、
    // 選択値から算出した op コード列を document に格納する。
    // (選択が空でもデフォルト計算結果と同じになるため、常に設定して問題ない)
    document.user_selected_options = Some(enabled_options_from_selections(&document, options));
    Ok(LoadedSkinDocumentWithDependencies {
        document,
        files: resolved_files,
        dependencies,
        cache_status: DocumentCacheStatus::Disabled,
    })
}

/// property 定義とユーザ選択 (オプション名 -> 選択肢名) から、JSON スキンの
/// 有効オプション (op コード列) を組み立てる。
///
/// 選択が無い property は `def` (空なら先頭 item) の op を使う。
pub(crate) fn enabled_options_from_selections(
    document: &SkinDocument,
    selections: &BTreeMap<String, String>,
) -> Vec<i32> {
    let options = document
        .property
        .iter()
        .filter_map(|property| {
            let selected = selected_property_item(property, selections)
                .or_else(|| default_property_item(property));
            selected.map(|item| item.op)
        })
        .collect();
    document.with_internal_enabled_options(options)
}

fn selected_property_item<'a>(
    property: &'a bmz_render::skin::SkinPropertyDef,
    selections: &BTreeMap<String, String>,
) -> Option<&'a bmz_render::skin::SkinPropertyItemDef> {
    let value = selections.get(&property.name)?;
    if let Ok(op) = value.parse::<i32>() {
        return property.item.iter().find(|item| item.op == op);
    }
    property.item.iter().find(|item| &item.name == value)
}

fn default_property_item(
    property: &bmz_render::skin::SkinPropertyDef,
) -> Option<&bmz_render::skin::SkinPropertyItemDef> {
    property
        .item
        .iter()
        .find(|item| !property.def.is_empty() && item.name == property.def)
        .or_else(|| property.item.first())
}

fn decode_skin_kind(kind: SkinKind) -> DecodeSkinKind {
    match kind {
        SkinKind::Play => DecodeSkinKind::Play,
        SkinKind::Select => DecodeSkinKind::Select,
        SkinKind::Decide => DecodeSkinKind::Decide,
        SkinKind::Result => DecodeSkinKind::Result,
    }
}

pub fn is_decodable_skin_path(path: &Path) -> bool {
    is_json_skin_path(path) || is_lua_skin_path(path) || is_lr2_skin_path(path)
}

pub fn is_json_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

pub fn is_lua_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("luaskin") || ext.eq_ignore_ascii_case("lua"))
}

pub fn is_lr2_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2skin"))
}

fn decode_font(path: &Path) -> Result<DecodedFontData> {
    if is_bitmap_font_path(path) {
        Ok(DecodedFontData::Bitmap(load_bitmap_font(path)?))
    } else {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read font: {}", path.display()))?;
        Ok(DecodedFontData::Vector(bytes))
    }
}

#[cfg(test)]
fn decode_font_with_cache(
    path: &Path,
    font_cache: Option<&SharedSkinFontCache>,
) -> Result<(DecodedFontData, FontCacheStatus, Option<SkinFontCacheKey>)> {
    decode_font_with_cache_key(path, font_cache, skin_font_cache_key(path))
}

fn decode_font_with_cache_key(
    path: &Path,
    font_cache: Option<&SharedSkinFontCache>,
    key: Option<SkinFontCacheKey>,
) -> Result<(DecodedFontData, FontCacheStatus, Option<SkinFontCacheKey>)> {
    let Some(font_cache) = font_cache else {
        return decode_font(path).map(|data| (data, FontCacheStatus::Disabled, None));
    };
    let Some(key) = key else {
        return decode_font(path).map(|data| (data, FontCacheStatus::Uncacheable, None));
    };
    if let Ok(mut cache) = font_cache.lock()
        && let Some(data) = cache.get(&key)
    {
        return Ok((data, FontCacheStatus::Hit, Some(key)));
    }
    let data = decode_font(path)?;
    if let Ok(mut cache) = font_cache.lock() {
        cache.insert(key.clone(), data.clone());
    }
    Ok((data, FontCacheStatus::Miss, Some(key)))
}

fn skin_font_cache_key(path: &Path) -> Option<SkinFontCacheKey> {
    let metadata = fs::metadata(path).ok()?;
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    Some(SkinFontCacheKey {
        is_bitmap: is_bitmap_font_path(&path),
        path,
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

fn font_data_cache_bytes(data: &DecodedFontData) -> usize {
    match data {
        DecodedFontData::Vector(bytes) => bytes.len(),
        DecodedFontData::Bitmap(font) => font
            .pages
            .values()
            .map(|page| page.image.pixels.len())
            .fold(font.glyphs.len().saturating_mul(64), usize::saturating_add),
    }
}

/// Phase A でデコードした成果物を Renderer に取り込み、scene context を更新する。
/// `default_manifest` は `load_default_skin_into_renderer` で取得した値を渡す。
/// 一括 install するので、PNG/フォント数が多いと 1 フレーム分のコストになる。
/// 起動直後や同期パスではこちらを使うが、ランタイム中はフレーム分散する方が望ましい。
pub fn install_decoded_skin(
    renderer: &mut Renderer,
    decoded: DecodedSkin,
    default_manifest: SkinManifest,
) -> Result<()> {
    let DecodedSkin { kind, document, fonts, sources, stats: _ } = decoded;

    for font in fonts {
        install_decoded_font(renderer, font);
    }

    let document_textures: Vec<SkinDocumentTexture> =
        sources.into_iter().filter_map(|source| install_decoded_source(renderer, source)).collect();

    set_decoded_skin_context(renderer, kind, default_manifest, document, document_textures, false);
    Ok(())
}

/// 1 個のフォントを renderer に登録する。フレーム分散インストールから呼ばれる。
pub fn install_decoded_font(renderer: &mut Renderer, font: DecodedFont) -> bool {
    let DecodedFont { stored_id, path, data, cache_key: _ } = font;
    let Some(data) = data else {
        tracing::debug!(
            font_id = %stored_id,
            path = %path.display(),
            "skipping beatoraja skin font install because payload is already installed"
        );
        return false;
    };
    let result: Result<()> = match data {
        DecodedFontData::Vector(bytes) => renderer.install_font_bytes(stored_id.clone(), bytes),
        DecodedFontData::Bitmap(bitmap) => {
            renderer.install_bitmap_font(stored_id.clone(), bitmap);
            Ok(())
        }
    };
    let success = result.is_ok();
    match result {
        Ok(()) => tracing::info!(
            font_id = %stored_id,
            path = %path.display(),
            "loaded beatoraja skin font"
        ),
        Err(error) => tracing::warn!(
            font_id = %stored_id,
            path = %path.display(),
            %error,
            "failed to install beatoraja skin font"
        ),
    }
    success
}

/// 1 個の PNG ソースを renderer にアップロードし、対応する SkinDocumentTexture を返す。
/// アップロードに失敗した場合は None。
pub fn install_decoded_source(
    renderer: &mut Renderer,
    source: DecodedSource,
) -> Option<SkinDocumentTexture> {
    let DecodedSource { source_id, path, texture, asset, size, cache_key: _, is_video: _ } = source;
    let Some(asset) = asset else {
        tracing::debug!(
            source_id = %source_id,
            texture_id = texture.0,
            path = %path.display(),
            "reusing beatoraja skin source texture"
        );
        return Some(SkinDocumentTexture { source_id, texture, source_size: size });
    };
    let source_size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
    if let Err(error) = renderer.upsert_image_asset(TextureId(texture.0), &asset) {
        tracing::warn!(
            source_id = %source_id,
            texture_id = texture.0,
            path = %path.display(),
            %error,
            "failed to upload beatoraja skin source"
        );
        return None;
    }
    tracing::info!(
        source_id = %source_id,
        texture_id = texture.0,
        path = %path.display(),
        "loaded beatoraja skin source"
    );
    Some(SkinDocumentTexture { source_id, texture, source_size })
}

/// 取り込み済みのフォント/PNG から SkinContext を組み立てて scene context にセットする。
///
/// プレイ中に `play7_files` などだけ変えた場合、`preserve_play_dynamic_timers` を true にすると
/// グルーヴ枠など `timer_observe_boolean` 由来のアニメ経過を維持できる。
pub fn set_decoded_skin_context(
    renderer: &mut Renderer,
    kind: SkinKind,
    default_manifest: SkinManifest,
    document: SkinDocument,
    document_textures: Vec<SkinDocumentTexture>,
    preserve_play_dynamic_timers: bool,
) {
    let context =
        SkinContext::from_manifest_and_document(default_manifest, document, document_textures);
    match kind {
        SkinKind::Play => {
            renderer.set_play_skin_context(context, preserve_play_dynamic_timers);
        }
        SkinKind::Select => renderer.set_select_skin_context(context),
        SkinKind::Decide => renderer.set_decide_skin_context(context),
        SkinKind::Result => renderer.set_result_skin_context(context),
    }
}

fn is_supported_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("ttf" | "otf" | "ttc" | "fnt")
    )
}

fn is_bitmap_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("fnt")
    )
}

fn resolve_json_skin_source_path(
    skin_root: &Path,
    source_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    resolve_json_skin_asset_path(skin_root, source_path, document, files)
}

fn resolve_json_skin_asset_path(
    skin_root: &Path,
    asset_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let normalized = asset_path.replace('\\', "/");
    if !normalized.contains('*') {
        return Some(resolve_case_insensitive_path(&skin_root.join(normalized)));
    }

    let filepath =
        document.filepath.iter().find(|filepath| filepath.path.replace('\\', "/") == normalized);

    // 0. ユーザが明示的に「ランダム」を選んだときは、def を無視して候補から
    //    ランダムに選ぶ (beatoraja のファイル選択 "Random" 相当)。
    if let Some(filepath) = filepath
        && files.get(&filepath.name).is_some_and(|selected| selected == RANDOM_FILE_SELECTION)
    {
        return resolve_wildcard_path(skin_root, &normalized, None);
    }

    // 1. パスが filepath 定義と完全一致するときは、選択ファイルをそのまま使う。
    if let Some(filepath) = filepath
        && let Some(selected) = files.get(&filepath.name).filter(|selected| !selected.is_empty())
        && let Some(path) =
            resolve_selected_skin_file_for_pattern(skin_root, &filepath.path, selected)
    {
        return Some(path);
    }

    // 2. 完全一致しなくても、filepath 定義の `*` が asset_path の `*` と同じ
    //    位置に来るなら、選択値からワイルドカード部分を抽出して埋め込む
    //    (例: 定義 `custom/laser/*` で選択 `custom/laser/veryshort` のとき、
    //         ソース `custom/laser/*/main.png` を `custom/laser/veryshort/main.png` へ)。
    if let Some(substituted) = substitute_filepath_choice(&normalized, &document.filepath, files) {
        let candidate = resolve_case_insensitive_path(&skin_root.join(&substituted));
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    // `def` が空、または beatoraja のランダム指定 ("Random") のときは具体的な
    // 優先ファイルを持たず、候補からランダムに選ぶ。
    let preferred = filepath.and_then(|filepath| {
        (!filepath.def.is_empty() && filepath.def != "Random").then_some(filepath.def.as_str())
    });
    resolve_wildcard_path(skin_root, &normalized, preferred)
}

/// filepath 定義のワイルドカードと一致するユーザ選択値を `asset_path` の
/// ワイルドカード位置に埋め込んだ相対パスを返す。
///
/// 一致条件: `asset_path` と filepath 定義の `path` が `*` 直前の prefix を
/// 共有していること。選択値からも同じ prefix（および suffix）を剥がして
/// ワイルドカード相当の文字列を取り出し、`asset_path` の `*` を置換する。
fn substitute_filepath_choice(
    asset_path: &str,
    filepaths: &[SkinFilepathDef],
    files: &BTreeMap<String, String>,
) -> Option<String> {
    let (asset_before, asset_after) = asset_path.split_once('*')?;
    for filepath in filepaths {
        let def_path = filepath.path.replace('\\', "/");
        let Some((def_prefix, def_suffix)) = def_path.split_once('*') else {
            continue;
        };
        if def_prefix != asset_before {
            continue;
        }
        let Some(selected) = files.get(&filepath.name).filter(|selected| !selected.is_empty())
        else {
            continue;
        };
        let selected = selected.replace('\\', "/");
        let wildcard_value = selected
            .strip_prefix(def_prefix)
            .and_then(|stripped| stripped.strip_suffix(def_suffix).or(Some(stripped)))
            .or_else(|| {
                selected
                    .strip_prefix(def_prefix.rsplit('/').next().unwrap_or_default())
                    .and_then(|stripped| stripped.strip_suffix(def_suffix).or(Some(stripped)))
            })?;
        return Some(format!("{asset_before}{wildcard_value}{asset_after}"));
    }
    None
}

/// ユーザ選択のスキンルート相対パスを解決する。
/// 絶対パスやスキンルート外への脱出を含む選択は無効として `None` を返す。
fn resolve_selected_skin_file(skin_root: &Path, selected: &str) -> Option<PathBuf> {
    use std::path::Component;

    let relative = Path::new(selected);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return None;
    }
    let candidate = resolve_case_insensitive_path(&skin_root.join(relative));
    candidate.is_file().then_some(candidate)
}

fn resolve_selected_skin_file_for_pattern(
    skin_root: &Path,
    pattern: &str,
    selected: &str,
) -> Option<PathBuf> {
    if let Some(path) = resolve_selected_skin_file(skin_root, selected) {
        return Some(path);
    }
    let pattern = strip_beatoraja_asset_filter(pattern).replace('\\', "/");
    let star = pattern.find('*')?;
    let prefix = &pattern[..star];
    let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
    let directory = &prefix[..slash];
    resolve_selected_skin_file(skin_root, &format!("{directory}{}", selected.replace('\\', "/")))
}

fn resolve_wildcard_path(
    skin_root: &Path,
    pattern: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    let pattern = strip_beatoraja_asset_filter(pattern);
    let star = pattern.find('*')?;
    let (prefix, suffix_with_star) = pattern.split_at(star);
    let suffix = &suffix_with_star[1..];
    let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
    let (directory, filename_prefix) = prefix.split_at(slash);
    let directory = skin_root.join(directory);

    if let Some(suffix) = suffix.strip_prefix('/') {
        return resolve_wildcard_directory_path(&directory, filename_prefix, suffix, preferred);
    }

    let candidates = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            starts_with_ignore_ascii_case(file_name, filename_prefix)
                && ends_with_ignore_ascii_case(file_name, suffix)
        })
        .collect::<Vec<_>>();
    if let Some(preferred) = preferred
        && let Some(candidate) = candidates.iter().find(|path| {
            let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            let stem = path.file_stem().and_then(|name| name.to_str()).unwrap_or_default();
            file_name.eq_ignore_ascii_case(preferred) || stem.eq_ignore_ascii_case(preferred)
        })
    {
        return Some(candidate.clone());
    }

    choose_wildcard_candidate(candidates)
}

fn resolve_case_insensitive_path(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };
    let Some(parent) = path.parent() else {
        return path.to_path_buf();
    };
    let parent = resolve_case_insensitive_path(parent);
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return parent.join(file_name);
    };
    entries
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(file_name))
        })
        .map(|entry| entry.path())
        .unwrap_or_else(|| parent.join(file_name))
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

fn strip_beatoraja_asset_filter(pattern: &str) -> &str {
    pattern.split_once('|').map_or(pattern, |(path, _)| path)
}

/// beatoraja のファイル選択カスタマイズで「ランダム」を表す番兵値。
/// `def == "Random"` や、設定パネルでユーザが明示的にランダムを選んだとき、
/// `files` マップにこの文字列が入る。具体ファイル名と衝突しないよう beatoraja
/// 同様 "Random" を用いる。
pub(crate) const RANDOM_FILE_SELECTION: &str = "Random";

/// ワイルドカードのマッチ候補から 1 つを選ぶ。
///
/// beatoraja の `SkinLoader.getPath` は、ユーザ選択 (filemap) が無いワイルドカードを
/// ロードごとに `Math.random()` でランダム解決する。`def == "Random"` の filepath も
/// 同様にランダムへ展開される (`SkinHeader.setSkinConfigProperty`)。これに合わせ、
/// `preferred` (具体的な def 値 / ユーザ選択) が候補に無いときはランダムに選ぶ。
fn choose_wildcard_candidate(candidates: Vec<PathBuf>) -> Option<PathBuf> {
    if candidates.len() <= 1 {
        return candidates.into_iter().next();
    }
    let index = random_wildcard_index(candidates.len());
    candidates.into_iter().nth(index)
}

/// `0..len` の範囲でロードごとに変わる擬似乱数インデックスを返す。
///
/// `RandomState` はプロセス内でランダムなキーを持ち、`new()` ごとに異なる状態に
/// なるため、同じ値をハッシュしても呼び出しごとに違う結果になる。追加の乱数
/// クレートを増やさずに beatoraja 相当の「毎ロードでランダム」を満たす。
fn random_wildcard_index(len: usize) -> usize {
    use std::hash::BuildHasher;

    debug_assert!(len > 0);
    let hash = std::collections::hash_map::RandomState::new().hash_one(len as u64);
    (hash % len as u64) as usize
}

fn required_skin_source_ids(document: &SkinDocument) -> HashSet<&str> {
    let destination_ids = destination_ids(document);
    let image_sources = document
        .image
        .iter()
        .map(|image| (image.id.as_str(), image.src.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut required = HashSet::new();

    for image in &document.image {
        if destination_ids.contains(image.id.as_str()) {
            required.insert(image.src.as_str());
        }
    }
    for imageset in &document.imageset {
        if destination_ids.contains(imageset.id.as_str()) {
            for image_id in &imageset.images {
                if let Some(source_id) = image_sources.get(image_id.as_str()) {
                    required.insert(*source_id);
                }
            }
        }
    }
    for value in &document.value {
        if destination_ids.contains(value.id.as_str()) {
            required.insert(value.src.as_str());
        }
    }
    for slider in &document.slider {
        if destination_ids.contains(slider.id.as_str()) {
            required.insert(slider.src.as_str());
        }
    }
    for graph in &document.graph {
        if destination_ids.contains(graph.id.as_str()) {
            required.insert(graph.src.as_str());
        }
    }
    for cover in document.hidden_cover.iter().chain(&document.lift_cover) {
        if destination_ids.contains(cover.id.as_str()) {
            required.insert(cover.src.as_str());
        }
    }
    if let Some(note) = &document.note {
        for image_id in note
            .note
            .iter()
            .chain(note.lnstart.iter())
            .chain(note.lnend.iter())
            .chain(note.lnbody.iter())
            .chain(note.lnactive.iter())
            .chain(note.hcnstart.iter())
            .chain(note.hcnend.iter())
            .chain(note.hcnbody.iter())
            .chain(note.hcnactive.iter())
            .chain(note.hcndamage.iter())
            .chain(note.hcnreactive.iter())
            .chain(note.mine.iter())
            .chain(note.hidden.iter())
            .chain(note.processed.iter())
        {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    if let Some(gauge) = &document.gauge {
        for image_id in &gauge.nodes {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    for gauge in &document.gauges {
        for image_id in &gauge.nodes {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    for judge in &document.judge {
        for destination in judge.images.iter().chain(judge.numbers.iter()) {
            if let Some(source_id) = image_sources.get(destination.id.as_str()) {
                required.insert(*source_id);
            }
            if let Some(value) = document.value.iter().find(|value| value.id == destination.id) {
                required.insert(value.src.as_str());
            }
        }
    }

    required
}

fn destination_ids(document: &SkinDocument) -> HashSet<&str> {
    let mut ids = HashSet::new();
    for entry in &document.destination {
        match entry {
            DestinationListEntry::Single(destination) => {
                ids.insert(destination.id.as_str());
            }
            DestinationListEntry::Conditional { destinations, .. } => {
                for destination in destinations {
                    ids.insert(destination.id.as_str());
                }
            }
        }
    }
    ids
}

fn resolve_wildcard_directory_path(
    directory: &Path,
    directory_prefix: &str,
    suffix: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    let mut candidates = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(directory_prefix))
        })
        .map(|path| path.join(suffix))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    if let Some(preferred) = preferred
        && let Some(candidate) = candidates.iter().find(|path| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == preferred)
        })
    {
        return Some(candidate.clone());
    }

    choose_wildcard_candidate(candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::hint::black_box;
    use std::time::Instant;

    use bmz_core::time::TimeUs;
    use bmz_render::plan::{DrawCommand, DrawPlan};
    use bmz_render::renderer::Renderer;
    use bmz_render::scene::{AppSceneSnapshot, SelectRowSnapshot, SelectSnapshot};
    use bmz_render::skin::{
        DestinationListEntry, DynamicTimerRuntime, SkinContext, SkinDocumentRenderExt,
        SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinManifest, SkinRenderItem,
        SkinTextState,
    };

    fn test_app_paths() -> AppPaths {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        AppPaths::from_dirs(data.clone(), data.clone(), data.join("cache"), data.join("logs"))
    }

    #[test]
    fn default_skin_root_contains_json_documents() {
        let root = default_skin_root();
        for file_name in ["select.json", "decide.json", "result.json", "play7.json"] {
            assert!(root.join(file_name).is_file(), "missing bundled default {file_name}");
        }
    }

    #[test]
    fn bundled_default_json_skin_documents_decode() {
        let app_paths = test_app_paths();
        for (kind, expected_type) in
            [(SkinKind::Select, 5), (SkinKind::Decide, 6), (SkinKind::Result, 7)]
        {
            let path = default_skin_document_path_from_paths(&app_paths, kind);
            let decoded = decode_beatoraja_skin(&path, kind)
                .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));
            assert_eq!(decoded.document.skin_type, expected_type);
            assert!(!decoded.sources.is_empty(), "{} has no image sources", path.display());
        }

        for (key_mode, expected_type) in [
            (KeyMode::K4, 22),
            (KeyMode::K5, 1),
            (KeyMode::K6, 23),
            (KeyMode::K7, 0),
            (KeyMode::K8, 24),
            (KeyMode::K9, 4),
            (KeyMode::K10, 3),
            (KeyMode::K14, 2),
        ] {
            let path = default_play_skin_document_path_from_paths(&app_paths, key_mode);
            let decoded = decode_beatoraja_skin(&path, SkinKind::Play)
                .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));
            assert_eq!(decoded.document.skin_type, expected_type);
            assert!(decoded.document.note.is_some(), "{} has no note definition", path.display());
            assert!(
                decoded.document.note.as_ref().is_some_and(|note| !note.group.is_empty()),
                "{} has no bar line group",
                path.display()
            );
            assert!(
                destination_ids(&decoded.document).contains("keybeam_img"),
                "{} has no keybeam destination",
                path.display()
            );
            assert!(!decoded.sources.is_empty(), "{} has no image sources", path.display());
        }
    }

    #[test]
    fn lua_compat_virtual_io_contains_only_sanitized_beatoraja_config() {
        let files = lua_compat_virtual_io_files();
        assert_eq!(files.len(), 2);

        let system: serde_json::Value =
            serde_json::from_str(&files["config_sys.json"]).expect("system config should be JSON");
        assert_eq!(system, serde_json::json!({ "playername": "bmz" }));

        let player: serde_json::Value =
            serde_json::from_str(&files["player/bmz/config_player.json"])
                .expect("player config should be JSON");
        let player = player.as_object().expect("player config should be an object");
        assert_eq!(
            player.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "mode5",
                "mode7",
                "mode9",
                "mode10",
                "mode14",
                "mode24",
                "mode24double"
            ])
        );
        for mode in player.values() {
            assert_eq!(mode["keyboard"], serde_json::json!({}));
            assert_eq!(mode["controller"], serde_json::json!([]));
            assert_eq!(mode["midi"], serde_json::json!({}));
        }
    }

    #[test]
    fn wmii_result_decodes_with_virtual_io_and_graph_default() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options =
            BTreeMap::from([("Expand Panel".to_string(), "ON - GRAPH DEFAULT".to_string())]);
        let runtime_state = LuaLoadRuntimeState {
            number_values: BTreeMap::new(),
            text_values: BTreeMap::new(),
            option_values: BTreeMap::from([(51, true), (160, true)]),
            ..LuaLoadRuntimeState::default()
        };
        let loaded = load_skin_document_uncached(
            &skin_path,
            SkinKind::Result,
            &options,
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("unmodified WMII result should decode through the BMZ loader");

        assert_eq!(loaded.document.result_panel_default, Some(2));
        assert_eq!(
            loaded
                .document
                .image
                .iter()
                .find(|image| image.id == "BtnGraphData")
                .and_then(|image| image.act),
            Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_GRAPH)
        );
        assert_eq!(
            loaded
                .document
                .image
                .iter()
                .find(|image| image.id == "BtnIrData")
                .and_then(|image| image.act),
            Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_IR)
        );
        let favorite = loaded
            .document
            .image
            .iter()
            .find(|image| image.id == "favorite")
            .expect("WMII result favorite button should decode");
        assert_eq!(favorite.ref_id, 90);
        assert_eq!(favorite.act, Some(90));
        assert_eq!(favorite.divy, 3);
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.draw.contains("result_panel(1)")
        )));
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.draw.contains("result_panel(2)")
        )));
        let destinations = loaded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                DestinationListEntry::Single(destination) => Some(destination),
                DestinationListEntry::Conditional { .. } => None,
            })
            .collect::<Vec<_>>();
        assert!(destinations.iter().any(|destination| destination.id == "randomButton1p"));
        let random_key = destinations
            .iter()
            .find(|destination| destination.id == "randomKeySet1P_1")
            .expect("7K Result should retain the RANDOM lane placement destinations");
        assert!(random_key.draw.contains("event_index(42)"));
        let rank_aaa = destinations
            .iter()
            .find(|destination| {
                destination.id == "rankBig_AAA" && destination.loop_time == Some(100)
            })
            .expect("rankBig_AAA should survive malformed op repair");
        assert_eq!(rank_aaa.op, [300, 920]);
        assert_eq!(rank_aaa.loop_time, Some(100));
        assert_eq!(rank_aaa.filter, 1);
        assert_eq!(rank_aaa.dst.len(), 2);
        for (id, rank) in [("AAA_BG", 300), ("AA_BG", 301), ("A_BG", 302)] {
            let backgrounds = destinations
                .iter()
                .filter(|destination| {
                    destination.id == id && matches!(destination.loop_time, Some(500 | 600 | 700))
                })
                .collect::<Vec<_>>();
            assert_eq!(backgrounds.len(), 3, "expected three {id} animations");
            assert!(backgrounds.iter().all(|destination| destination.op == [90, rank]));
        }
        let clear_backgrounds = destinations
            .iter()
            .filter(|destination| {
                destination.id == "clearBG"
                    && matches!(destination.loop_time, Some(500 | 600 | 700))
            })
            .collect::<Vec<_>>();
        assert_eq!(clear_backgrounds.len(), 3);
        assert!(clear_backgrounds.iter().all(|destination| destination.op == [90]));
        let expanded_timing_values = destinations
            .iter()
            .filter(|destination| {
                matches!(
                    destination.id.as_str(),
                    "timingAvg"
                        | "timingAvgAdot"
                        | "timingDotMS"
                        | "durationAvg"
                        | "durationAvgAdot"
                        | "stddav"
                        | "stddaAdot"
                ) && destination.dst.first().is_some_and(|entry| {
                    matches!(
                        entry,
                        bmz_render::skin::SkinDstEntry::Frame(frame)
                            if frame.x.is_some_and(|x| x >= 1_000)
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(expanded_timing_values.len(), 12);
        assert!(
            expanded_timing_values.iter().all(|destination| {
                destination.draw.contains("result_panel(2)")
                    && !destination.draw.contains("result_panel(0)")
                    && !destination.draw.contains("result_panel(1)")
            }),
            "expanded timing values must stay hidden on the IR panel: {:?}",
            expanded_timing_values
                .iter()
                .map(|destination| (destination.id.as_str(), destination.draw.as_str()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            loaded.dependencies.virtual_io_files.get("config_sys.json"),
            Some(&Some("{\"playername\":\"bmz\"}".to_string()))
        );
        assert!(loaded.dependencies.virtual_io_files.contains_key("player/bmz/config_player.json"));
    }

    #[test]
    fn wmii_course_result_uses_native_stage_titles_and_result_data() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/result/courseResult.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let runtime_state = LuaLoadRuntimeState {
            text_values: BTreeMap::from([
                (150, "Stage One".to_string()),
                (151, "Stage Two".to_string()),
                (152, "Stage Three".to_string()),
                (153, "Stage Four".to_string()),
            ]),
            option_values: BTreeMap::from([(160, true), (290, true)]),
            virtual_io_files: BTreeMap::from([(
                "skin/WMII_FHD/result/courseData.json".to_string(),
                serde_json::json!({
                    "songs": [
                        { "stage": 1, "score": 1000, "gauge": 80, "miss": 10, "rate": 0.5 },
                        { "stage": 2, "score": 2000, "gauge": 81, "miss": 11, "rate": 0.6 },
                        { "stage": 3, "score": 3000, "gauge": 82, "miss": 12, "rate": 0.7 },
                        { "stage": 4, "score": 3456, "gauge": 88, "miss": 13, "rate": 0.75 }
                    ]
                })
                .to_string(),
            )]),
            ..LuaLoadRuntimeState::default()
        };
        let loaded = load_skin_document_uncached(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("unmodified WMII course result should decode with native stage data");

        for (id, expected) in
            [("stage_gauge4", "88"), ("stage_score4", "3456"), ("stage_miss4", "13")]
        {
            let value = loaded
                .document
                .value
                .iter()
                .find(|value| value.id == id)
                .unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(value.value_expr, expected, "unexpected {id} expression");
        }
        let graph = loaded
            .document
            .graph
            .iter()
            .find(|graph| graph.id == "stage_scoreGraph4")
            .expect("missing stage 4 score-rate graph");
        assert_eq!(graph.value_expr, "0.75");
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination) if destination.id == "courseTitle4"
        )));
        assert_eq!(
            loaded
                .document
                .value
                .iter()
                .find(|value| value.id == "courseClearRate")
                .map(|value| value.value_expr.as_str()),
            Some(bmz_render::skin::SKIN_EXPR_COURSE_CLEAR_RATE)
        );
        assert_eq!(
            loaded.dependencies.virtual_io_files.get("skin/WMII_FHD/result/courseData.json"),
            runtime_state
                .virtual_io_files
                .get("skin/WMII_FHD/result/courseData.json")
                .cloned()
                .map(Some)
                .as_ref()
        );
    }

    #[test]
    fn modern_chic_result_bakes_runtime_song_label_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ModernChic/result.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let runtime_state = LuaLoadRuntimeState {
            text_values: BTreeMap::from([
                (10, "Song".to_string()),
                (11, "Subtitle".to_string()),
                (12, "Song Subtitle".to_string()),
                (13, "Genre".to_string()),
                (14, "Artist".to_string()),
                (1003, "Table ★12".to_string()),
            ]),
            ..LuaLoadRuntimeState::default()
        };
        let loaded = load_skin_document_uncached(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("unmodified ModernChic result should decode with runtime song text");
        let bottom = loaded
            .document
            .text
            .iter()
            .find(|text| text.id == "bottomResult")
            .expect("ModernChic bottomResult text");
        assert_eq!(bottom.constant_text, "Song Subtitle / Artist / Genre / Table ★12");
    }

    #[test]
    fn luxe_flat_result_decodes_local_panel_state_and_tab_actions() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Luxez-Flat/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let runtime_state = LuaLoadRuntimeState {
            option_values: BTreeMap::from([(50, false), (51, true)]),
            ..LuaLoadRuntimeState::default()
        };
        let loaded = load_skin_document_uncached(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("unmodified Luxe Flat result should decode through the BMZ loader");

        assert_eq!(loaded.document.result_panel_default, Some(2));
        assert_eq!(
            loaded
                .document
                .image
                .iter()
                .find(|image| image.id == "result_modeselect_graph_data_off")
                .and_then(|image| image.act),
            Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_GRAPH)
        );
        assert_eq!(
            loaded
                .document
                .image
                .iter()
                .find(|image| image.id == "result_modeselect_ir_ranking_off")
                .and_then(|image| image.act),
            Some(bmz_render::skin::SKIN_EVENT_RESULT_PANEL_IR)
        );
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.draw.contains("result_panel(1)")
        )));
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.draw.contains("result_panel(2)")
        )));
        assert_eq!(
            loaded
                .document
                .value
                .iter()
                .find(|value| value.id == "rank_diff_count")
                .map(|value| value.value_expr.as_str()),
            Some("bmz:nearest_rank_diff_abs")
        );
        assert_eq!(
            loaded
                .document
                .value
                .iter()
                .find(|value| value.id == "ir_scorerate1")
                .map(|value| value.value_expr.as_str()),
            Some("bmz:ir_score_rate_integer:1")
        );
        assert_eq!(
            loaded
                .document
                .value
                .iter()
                .find(|value| value.id == "ir_scorerate_dot1")
                .map(|value| value.value_expr.as_str()),
            Some("bmz:ir_score_rate_fraction:1")
        );
        assert!(loaded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == "rank_diff_aaa_plus"
                    && destination.draw.contains("nearest_rank(AAA,plus)")
        )));
    }

    #[test]
    fn wmii_result_renders_bmz_player_version_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::from([("Display Version".to_string(), "ON".to_string())]),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(
            decoded.document.text.iter().any(|text| text.id == "version" && text.ref_id == 1010),
            "WMII version text should retain STRING_VERSION ref 1010"
        );
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let items = decoded.document.static_render_items(
            &sources,
            &SkinDrawState { elapsed_ms: 2_000, ..SkinDrawState::default() },
            &SkinTextState::default(),
        );

        assert!(items.iter().any(|item| matches!(
            item,
            SkinRenderItem::Text { text, .. }
                if text == &format!("bmz-player {}", env!("CARGO_PKG_VERSION"))
        )));
    }

    #[test]
    fn wmii_result_uses_runtime_combo_break_for_clear_animation() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options =
            BTreeMap::from([("Expand Panel".to_string(), "ON - GRAPH DEFAULT".to_string())]);
        let load = |combo_break: i32| {
            load_skin_document_uncached(
                &skin_path,
                SkinKind::Result,
                &options,
                &BTreeMap::new(),
                &LuaLoadRuntimeState {
                    number_values: BTreeMap::from([(425, combo_break)]),
                    text_values: BTreeMap::new(),
                    option_values: BTreeMap::from([(51, true), (160, true)]),
                    ..LuaLoadRuntimeState::default()
                },
            )
            .expect("unmodified WMII result should decode")
        };
        let destination_ids = |loaded: &LoadedSkinDocumentWithDependencies| {
            loaded
                .document
                .destination
                .iter()
                .filter_map(|entry| match entry {
                    DestinationListEntry::Single(destination) => Some(destination.id.clone()),
                    DestinationListEntry::Conditional { .. } => None,
                })
                .collect::<Vec<_>>()
        };

        let full_combo = load(0);
        let full_combo_ids = destination_ids(&full_combo);
        assert!(full_combo_ids.iter().any(|id| id == "result_FULL"));
        assert!(full_combo_ids.iter().any(|id| id == "result_COMBO"));
        assert!(!full_combo_ids.iter().any(|id| id == "result_CLEAR"));

        let normal_clear = load(1);
        let normal_clear_ids = destination_ids(&normal_clear);
        assert!(normal_clear_ids.iter().any(|id| id == "result_CLEAR"));
        assert!(!normal_clear_ids.iter().any(|id| id == "result_FULL"));
        assert!(!normal_clear_ids.iter().any(|id| id == "result_COMBO"));
    }

    fn filepath_def(name: &str, path: &str, def: &str) -> SkinFilepathDef {
        SkinFilepathDef {
            category: String::new(),
            name: name.to_string(),
            path: path.to_string(),
            def: def.to_string(),
        }
    }

    #[test]
    fn substitute_filepath_choice_replaces_wildcard_in_asset_path() {
        let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
        let mut files = BTreeMap::new();
        files.insert("レーザー".to_string(), "veryshort".to_string());

        let result = substitute_filepath_choice("custom/laser/*/main.png", &filepaths, &files);
        assert_eq!(result.as_deref(), Some("custom/laser/veryshort/main.png"));
    }

    #[test]
    fn substitute_filepath_choice_strips_def_suffix_from_selection() {
        let filepaths = vec![filepath_def("icon", "icon-*.png", "")];
        let mut files = BTreeMap::new();
        files.insert("icon".to_string(), "icon-blue.png".to_string());

        let result = substitute_filepath_choice("icon-*.png", &filepaths, &files);
        assert_eq!(result.as_deref(), Some("icon-blue.png"));
    }

    #[test]
    fn resolve_skin_source_accepts_beatoraja_filename_selection() {
        let root = unique_test_dir("bmz-json-source-filename");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/default.png"), []).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
        )
        .unwrap();
        let files = BTreeMap::from([("Parts".to_string(), "default.png".to_string())]);

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
    }

    #[test]
    fn resolve_skin_source_still_accepts_legacy_relative_selection() {
        let root = unique_test_dir("bmz-json-source-relative");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/default.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png" }
                ]
            }
            "#,
        )
        .unwrap();
        let files = BTreeMap::from([("Parts".to_string(), "parts/default.png".to_string())]);

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
    }

    #[test]
    fn substitute_filepath_choice_returns_none_when_prefix_mismatch() {
        let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
        let mut files = BTreeMap::new();
        files.insert("レーザー".to_string(), "custom/laser/veryshort".to_string());

        // asset の prefix が定義と一致しない
        let result = substitute_filepath_choice("other/path/*.png", &filepaths, &files);
        assert_eq!(result, None);
    }

    #[test]
    fn enabled_options_includes_unselected_property_default_for_real_skin() {
        // 実際の Starseeker play7.luaskin で「スコアグラフ=On」のみ選択した時、
        // 未選択の「プレーサイド」のデフォルト (1P=920) と「スコアグラフ=On」(901)
        // の両方が enabled_options に入ることを確認する。
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            eprintln!("skipping: skin not present at {}", skin_path.display());
            return;
        }
        let mut selections = BTreeMap::new();
        selections.insert("スコアグラフ".to_string(), "On".to_string());

        let loaded = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &selections,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
        )
        .expect("load skin document");
        let ops = enabled_options_from_selections(&loaded.document, &selections);
        assert!(ops.contains(&901), "expected 901 in ops, got {ops:?}");
        assert!(ops.contains(&920), "expected 920 (1P default) in ops, got {ops:?}");
    }

    #[test]
    fn enabled_options_rejects_stale_numeric_selection() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "property": [
                    {
                        "name": "Graph",
                        "def": "AC",
                        "item": [
                            { "name": "AC", "op": 922 },
                            { "name": "TYPE-M", "op": 923 }
                        ]
                    }
                ]
            }
            "#,
        )
        .unwrap();
        let selections = BTreeMap::from([("Graph".to_string(), "999".to_string())]);

        assert_eq!(enabled_options_from_selections(&document, &selections), vec![922]);
    }

    #[test]
    fn substitute_filepath_choice_returns_none_when_no_selection() {
        let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
        let files: BTreeMap<String, String> = BTreeMap::new();

        let result = substitute_filepath_choice("custom/laser/*/main.png", &filepaths, &files);
        assert_eq!(result, None);
    }

    #[test]
    fn default_skin_can_be_applied_to_renderer() {
        let mut renderer = Renderer::default();

        apply_default_skin(&mut renderer).unwrap();
    }

    #[test]
    fn beatoraja_default_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/play7.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn beatoraja_default_select_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/select.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_select_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_play7_1p_json_skin_can_be_applied_when_available() {
        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7-1p.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn beatoraja_default_result_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/result.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_result_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_result_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ECFN/RESULT/result-converted.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_result_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_select_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ECFN/select/select-converted.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_select_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_select_lua_skin_decodes_movie_source_first_frame_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ECFN/select/select.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let mv = decoded.sources.iter().find(|source| source.source_id == "mv").unwrap();

        let mv_path = mv.path.to_string_lossy().replace('\\', "/");
        assert!(mv_path.ends_with("mv/default.mp4"));
        let asset = mv.asset.as_ref().expect("movie first frame should decode");
        assert!(asset.width > 0);
        assert!(asset.height > 0);
        assert_eq!(asset.pixels.len(), asset.width as usize * asset.height as usize * 4);
    }

    #[test]
    #[ignore = "manual select skin profiling helper"]
    fn profile_ecfn_select_plan_generation() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ECFN/select/select.luaskin");
        if !skin_path.is_file() {
            eprintln!("skip: {} is missing", skin_path.display());
            return;
        }

        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let document_textures = decoded.sources.iter().map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: SkinImageSize { width: source.size.width, height: source.size.height },
        });
        let skin = SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            decoded.document,
            document_textures,
        );
        let rows = (0..25)
            .map(|index| SelectRowSnapshot {
                index,
                title: format!("War in the Mirrorworld[{index:02}]"),
                artist: "Aoi".to_string(),
                difficulty_name: "ANOTHER".to_string(),
                play_level: "12".to_string(),
                total_notes: 2253,
                chart_normal_notes: 2167,
                chart_scratch_notes: 86,
                chart_density: 19.0,
                chart_peak_density: 38.0,
                chart_end_density: 25.0,
                min_bpm: 171.0,
                max_bpm: 171.0,
                chart_main_bpm: 171.0,
                initial_bpm: 171.0,
                length_ms: 115_000,
                ..SelectRowSnapshot::default()
            })
            .collect();
        let mut runtime = DynamicTimerRuntime::default();
        let mut snapshot = SelectSnapshot {
            time: TimeUs(0),
            selection_time: TimeUs(0),
            chart_count: 1_000,
            selected_index: 12,
            rows,
            stage_background: true,
            banner_image: true,
            ..SelectSnapshot::default()
        };

        for frame in 0..30 {
            snapshot.time = TimeUs(frame * 16_666);
            black_box(DrawPlan::from_scene_with_skin(
                &AppSceneSnapshot::Select(snapshot.clone()),
                &skin,
                &mut runtime,
            ));
        }

        let frames = 300;
        let start = Instant::now();
        let mut commands = 0_usize;
        for frame in 0..frames {
            snapshot.time = TimeUs((frame + 30) * 16_666);
            let plan = DrawPlan::from_scene_with_skin(
                &AppSceneSnapshot::Select(snapshot.clone()),
                &skin,
                &mut runtime,
            );
            commands += plan.commands.len();
            black_box(plan);
        }
        let elapsed = start.elapsed();
        println!(
            "profile_ecfn_select_plan_generation frames={frames} avg_plan_ms={:.3} avg_commands={}",
            elapsed.as_secs_f64() * 1000.0 / frames as f64,
            commands / frames as usize
        );
    }

    #[test]
    #[ignore = "manual select skin profiling helper"]
    fn profile_rgba_frame_clone_cost() {
        let width = 1920_usize;
        let height = 1080_usize;
        let rgba = vec![127_u8; width * height * 4];
        let frames = 240;

        let clone_start = Instant::now();
        let mut cloned_len = 0_usize;
        for _ in 0..frames {
            let cloned = black_box(rgba.clone());
            cloned_len += black_box(cloned.len());
        }
        let clone_elapsed = clone_start.elapsed();

        let borrow_start = Instant::now();
        let mut borrowed_len = 0_usize;
        for _ in 0..frames {
            borrowed_len += black_box(rgba.as_slice()).len();
        }
        let borrow_elapsed = borrow_start.elapsed();

        assert_eq!(cloned_len, borrowed_len);
        println!(
            "profile_rgba_frame_clone_cost frames={frames} bytes={} avg_clone_ms={:.3} avg_borrow_ms={:.6}",
            rgba.len(),
            clone_elapsed.as_secs_f64() * 1000.0 / frames as f64,
            borrow_elapsed.as_secs_f64() * 1000.0 / frames as f64
        );
    }

    #[test]
    fn m_select_lua_select_skin_renders_items_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/mz-select/music_select.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let document_textures =
            decoded.sources.iter().map(|source| bmz_render::skin::SkinDocumentTexture {
                source_id: source.source_id.clone(),
                texture: source.texture,
                source_size: bmz_render::skin::SkinImageSize {
                    width: source.size.width,
                    height: source.size.height,
                },
            });
        let context = bmz_render::skin::SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            decoded.document,
            document_textures,
        );
        assert!(context.document().is_some_and(|document| document.skin_type == 5));
        let snapshot = bmz_render::scene::SelectSnapshot {
            rows: vec![bmz_render::scene::SelectRowSnapshot {
                title: "Song".to_string(),
                ..Default::default()
            }],
            chart_count: 1,
            ..Default::default()
        };
        let items = context.select_document_items_with_dynamic_timers(&snapshot, None);
        assert!(!items.is_empty(), "m_select select skin should produce render items");
        assert!(
            items
                .iter()
                .any(|item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "Song")),
            "m_select select skin should render the song title text"
        );
    }

    #[test]
    fn luxe_flat_lua_select_skin_keeps_operating_time_refs_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Luxez-Flat/music_select.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Select).unwrap();
        for ref_id in 27..=29 {
            assert!(
                decoded.document.value.iter().any(|value| value.ref_id == ref_id),
                "Luxe Flat should retain operating-time ref {ref_id}"
            );
        }
    }

    #[test]
    fn rm_skin_play_lua_skins_can_be_decoded_when_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rm-skin");
        let cases = [
            (root.join("play5main.luaskin"), SkinKind::Play),
            (root.join("play7main.luaskin"), SkinKind::Play),
            (root.join("play9main.luaskin"), SkinKind::Play),
        ];
        for (skin_path, kind) in cases {
            if !skin_path.is_file() {
                continue;
            }
            let decoded = decode_beatoraja_skin(&skin_path, kind).unwrap();
            assert!(!decoded.document.destination.is_empty(), "{}", skin_path.display());
        }
    }

    #[test]
    fn ecfn_lua_skins_can_be_decoded_when_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN");
        let cases = [
            (root.join("select/select.luaskin"), SkinKind::Select),
            (root.join("play/play7.luaskin"), SkinKind::Play),
            (root.join("RESULT/result.luaskin"), SkinKind::Result),
        ];
        for (skin_path, kind) in cases {
            if !skin_path.is_file() {
                continue;
            }
            let decoded = decode_beatoraja_skin(&skin_path, kind).unwrap();
            assert!(!decoded.document.destination.is_empty());
        }
    }

    #[test]
    fn ecfn_play7_uses_default_filepaths_when_defs_are_missing() {
        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        for (source_id, suffix) in [
            ("6", "laser/default.png"),
            ("7", "notes/default.png"),
            ("12", "lanecover/default.png"),
        ] {
            let source = decoded
                .sources
                .iter()
                .find(|source| source.source_id == source_id)
                .unwrap_or_else(|| panic!("ECFN source {source_id} should decode"));
            let path = source.path.to_string_lossy().replace('\\', "/");
            assert!(
                path.ends_with(suffix),
                "ECFN source {source_id} should resolve to {suffix}, got {path}"
            );
        }
    }

    #[test]
    fn luxe_flat_lua_select_skin_keeps_score_availability_guards_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Luxez-Flat/music_select.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Select).unwrap();
        let clear_state = decoded
            .document
            .destination
            .iter()
            .find_map(|entry| match entry {
                DestinationListEntry::Single(destination)
                    if destination.id == "default_playerdata_state_clear" =>
                {
                    Some(destination)
                }
                DestinationListEntry::Single(_) | DestinationListEntry::Conditional { .. } => None,
            })
            .expect("Luxe Flat should retain the player clear-state destination");
        assert_eq!(clear_state.draw, "select_score_available()");
    }

    #[test]
    fn mz_select_lua_select_skin_keeps_local_score_availability_guards() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/mz-select/music_select.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Select).unwrap();
        let guarded = decoded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                DestinationListEntry::Single(destination)
                    if destination.id.starts_with("default_playerdata_")
                        && destination.draw == "select_score_available()" =>
                {
                    Some(destination.id.as_str())
                }
                DestinationListEntry::Single(_) | DestinationListEntry::Conditional { .. } => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(guarded.len(), 21, "mz-select player-data score guards: {guarded:?}");
        assert!(guarded.contains(&"default_playerdata_state_clear"));
        assert!(guarded.contains(&"default_playerdata_score_count"));
        assert!(guarded.contains(&"default_playerdata_scorerate_dot_count"));
    }

    #[test]
    fn mz_select_result_uses_runtime_decisions_and_draws_note_graphs() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/mz-select/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let runtime_state = LuaLoadRuntimeState {
            number_values: BTreeMap::from([
                (74, 100),
                (153, 354),
                (370, 7),
                (371, 5),
                (374, -12),
                (375, -50),
                (410, 20),
                (411, 10),
                (412, 8),
                (413, 4),
                (414, 3),
                (415, 2),
                (416, 1),
                (417, 1),
                (418, 1),
                (419, 1),
                (421, 1),
                (422, 1),
            ]),
            ..LuaLoadRuntimeState::default()
        };
        let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime_state,
        )
        .expect("decode mz-select result skin with runtime result values");

        let timing = decoded
            .document
            .text
            .iter()
            .find(|text| text.id == "timing")
            .expect("mz-select timing text");
        assert_eq!(timing.constant_text, "平均12.5ms遅い");
        let clear_state = decoded
            .document
            .image
            .iter()
            .find(|image| image.id == "clear_state")
            .expect("mz-select clear update image");
        assert_eq!(clear_state.x, 0, "current clear above previous should use UP image");
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination) if destination.id == "win"
        )));
        assert!(!decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination) if destination.id == "draw"
        )));

        let document_textures = decoded.sources.iter().map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: source.size,
        });
        let context = SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            decoded.document,
            document_textures,
        );
        let graph = std::sync::Arc::new(bmz_render::snapshot::ResultGraphSnapshot {
            judge_graph_buckets: vec![
                bmz_render::snapshot::ResultJudgeGraphBucket { values: [0, 10, 5, 2, 1, 1] },
                bmz_render::snapshot::ResultJudgeGraphBucket { values: [0, 8, 4, 2, 1, 0] },
            ],
            early_late_graph_buckets: vec![
                bmz_render::snapshot::ResultEarlyLateGraphBucket {
                    values: [0, 10, 4, 2, 1, 0, 3, 2, 1, 0],
                },
                bmz_render::snapshot::ResultEarlyLateGraphBucket {
                    values: [0, 8, 3, 2, 1, 0, 4, 2, 1, 0],
                },
            ],
            judge_graph_density: vec![12, 18],
            ..bmz_render::snapshot::ResultGraphSnapshot::default()
        });
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 500,
            result_failed: Some(false),
            total_notes: 100,
            key_mode: KeyMode::K7,
            ..bmz_render::skin::SkinDrawState::default()
        };
        let items = context.static_document_items_for_result_state_and_text(
            &graph,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );
        let populated_batches = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::RectBatch { rects, .. } if !rects.is_empty()
                )
            })
            .count();
        assert_eq!(populated_batches, 2, "JUDGE and FAST/SLOW graph batches should render");
        assert!(
            !items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Rect {
                    color,
                    blend: bmz_render::skin::BlendMode::Add,
                    ..
                } if color.r == 0.0 && color.g == 0.0 && color.b == 0.0
            )),
            "additive black gauge backgrounds must not cover the two note graphs"
        );
    }

    #[test]
    fn ecfn_play7_judge_combo_x_matches_beatoraja_layout_when_available() {
        use std::collections::HashMap;

        use bmz_render::skin::{SkinDocumentTexture, SkinImageSize, SkinRenderItem, SkinTextureId};

        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let mock_texture = SkinDocumentTexture {
            source_id: "mock".to_string(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 1920.0, height: 1080.0 },
        };
        let sources: HashMap<String, SkinDocumentTexture> = decoded
            .document
            .source
            .iter()
            .map(|source| (source.id.clone(), mock_texture.clone()))
            .chain(
                decoded
                    .document
                    .value
                    .iter()
                    .map(|value| (value.src.clone(), mock_texture.clone())),
            )
            .chain(
                decoded
                    .document
                    .image
                    .iter()
                    .map(|image| (image.src.clone(), mock_texture.clone())),
            )
            .collect();
        let items =
            decoded.document.judge_render_items("PGREAT", 42, 100, &sources).expect("judge items");
        let digit_xs: Vec<f32> = items
            .iter()
            .skip(1)
            .filter_map(|item| match item {
                SkinRenderItem::Image { rect, .. } => Some(rect.x),
                _ => None,
            })
            .collect();
        assert_eq!(digit_xs.len(), 2);
        let expected_first = 334.0 / 1920.0;
        let expected_second = 392.0 / 1920.0;
        assert!(
            (digit_xs[0] - expected_first).abs() < 0.001,
            "first digit x={} expected {expected_first}",
            digit_xs[0]
        );
        assert!(
            (digit_xs[1] - expected_second).abs() < 0.001,
            "second digit x={} expected {expected_second}",
            digit_xs[1]
        );
    }

    #[test]
    fn ecfn_play7_pre_notes_judge_line_renders_in_front_when_available() {
        use std::collections::HashMap;

        use bmz_render::skin::{
            SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinRenderItem, SkinTextState,
        };

        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let image_15 = decoded
            .document
            .image
            .iter()
            .find(|image| image.id == "15")
            .expect("ECFN id=15 image should decode");
        assert_eq!((image_15.src.as_str(), image_15.x, image_15.y), ("0", 16, 0));
        let image_15_map = decoded.document.image_map();
        let mapped_15 = image_15_map.get("15").expect("ECFN id=15 image should map");
        assert_eq!((mapped_15.src.as_str(), mapped_15.x, mapped_15.y), ("0", 16, 0));
        let system_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "0")
            .map(|source| source.texture)
            .expect("ECFN source 0 should decode");
        let system_size = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "0")
            .map(|source| SkinImageSize { width: source.size.width, height: source.size.height })
            .expect("ECFN source 0 should decode");
        let sources: HashMap<String, SkinDocumentTexture> = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect();

        let (behind, front, _) = decoded.document.static_render_items_split(
            &sources,
            &SkinDrawState::default(),
            &SkinTextState::default(),
        );

        assert!(
            behind.iter().all(|item| !matches!(
                item,
                SkinRenderItem::Image {
                    texture,
                    rect,
                    ..
                } if *texture == system_texture
                    && (rect.y - 715.0 / 1080.0).abs() < 0.001
                    && (rect.height - 8.0 / 1080.0).abs() < 0.001
            )),
            "ECFN judge line should not remain behind notes"
        );
        assert!(
            front.iter().any(|item| matches!(
                item,
                SkinRenderItem::Image {
                    texture,
                    rect,
                    uv,
                    ..
                } if *texture == system_texture
                    && (rect.y - 715.0 / 1080.0).abs() < 0.001
                    && (rect.height - 8.0 / 1080.0).abs() < 0.001
                    && (uv.x - 16.0 / system_size.width).abs() < 0.001
                    && uv.y.abs() < 0.001
            )),
            "expected ECFN id=15 judge line in front items; got {front:?}"
        );
    }

    #[test]
    fn ecfn_play14_judge1_combo_is_right_of_judge_when_available() {
        use std::collections::HashMap;

        use bmz_core::lane::Lane;
        use bmz_render::skin::{
            MAX_JUDGE_REGIONS, SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinRenderItem,
            SkinTextureId,
        };

        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play14.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play)
            .expect("ECFN play14 should decode with default options");
        let judge0 =
            decoded.document.judge.iter().find(|judge| judge.id == "judge").expect("judge");
        let judge1 =
            decoded.document.judge.iter().find(|judge| judge.id == "judge1").expect("judge1");
        assert_eq!(judge0.index, 0);
        assert_eq!(judge1.index, 1);

        let mock_texture = SkinDocumentTexture {
            source_id: "mock".to_string(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 1920.0, height: 1080.0 },
        };
        let sources: HashMap<String, SkinDocumentTexture> = decoded
            .document
            .source
            .iter()
            .map(|source| (source.id.clone(), mock_texture.clone()))
            .chain(
                decoded
                    .document
                    .value
                    .iter()
                    .map(|value| (value.src.clone(), mock_texture.clone())),
            )
            .chain(
                decoded
                    .document
                    .image
                    .iter()
                    .map(|image| (image.src.clone(), mock_texture.clone())),
            )
            .collect();

        let mut judge_ms = [None; MAX_JUDGE_REGIONS];
        let mut judge_index = [None; MAX_JUDGE_REGIONS];
        judge_ms[0] = Some(100);
        judge_ms[1] = Some(100);
        judge_index[0] = Some(0);
        judge_index[1] = Some(0);
        let state = SkinDrawState { judge_ms, judge_index, combo: 42, ..SkinDrawState::default() };

        let left_items = decoded
            .document
            .judge_render_items_for_def(judge0, 0, 42, 100, &sources, &state)
            .expect("left judge");
        let right_items = decoded
            .document
            .judge_render_items_for_def(judge1, 0, 42, 100, &sources, &state)
            .expect("right judge");
        let left_digit = left_items
            .iter()
            .skip(1)
            .find_map(|item| match item {
                SkinRenderItem::Image { rect, .. } => Some(rect.x),
                _ => None,
            })
            .expect("left combo digit");
        let right_digit = right_items
            .iter()
            .skip(1)
            .find_map(|item| match item {
                SkinRenderItem::Image { rect, .. } => Some(rect.x),
                _ => None,
            })
            .expect("right combo digit");
        assert!(
            right_digit > left_digit,
            "judge1 digit x={right_digit} should be right of judge x={left_digit}"
        );

        let region = bmz_render::skin::lane_judge_region(
            Lane::Key8.index(),
            bmz_core::lane::LANE_COUNT,
            decoded.document.judge_region_count(),
        );
        assert_eq!(region, 1);
    }

    #[test]
    fn starseeker_play_lua_skin_can_be_decoded_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert!(!decoded.document.destination.is_empty());
    }

    #[test]
    fn starseeker_frame_filepath_selection_merges_frame_destinations_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let mut files = BTreeMap::new();
        files.insert("フレーム".to_string(), "custom/frame/AC_SP/starseeker".to_string());

        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &files,
        )
        .expect("decode starseeker frame skin");

        assert!(
            decoded.document.source.iter().any(|source| source.id == "main_frame"),
            "expected main_frame source from starseeker frameL.lua"
        );
        assert!(
            decoded
                .document
                .all_destinations(&[])
                .iter()
                .any(|destination| destination.id == "base_L" || destination.id == "base_R"),
            "expected frame panel destinations from starseeker frameL.lua"
        );
    }

    #[test]
    fn starseeker_default_frame_uses_same_directory_for_lua_parts_and_sources_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play)
            .expect("decode starseeker default frame skin");
        let main_frame = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "main_frame")
            .expect("main_frame source should be decoded from selected frame");

        assert!(
            main_frame.path.components().any(|component| component.as_os_str() == "TM_default"),
            "expected default frame source under TM_default, got {}",
            main_frame.path.display()
        );
    }

    #[test]
    fn starseeker_result_lua_skin_renders_stat_details_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([
            ("F/Sリスト".to_string(), "Default".to_string()),
            ("逆サイド詳細フレーム".to_string(), "ON".to_string()),
            ("プレーサイド".to_string(), "1P".to_string()),
        ]);
        let files = BTreeMap::from([
            ("使用テーマ".to_string(), "Theme/starseeker".to_string()),
            ("フォント".to_string(), "_font/TYPE-M".to_string()),
            ("シャッター".to_string(), "Shutter/TYPE-M".to_string()),
        ]);
        let decoded =
            decode_beatoraja_skin_with_options(&skin_path, SkinKind::Result, &options, &files)
                .expect("decode starseeker result skin");
        let destinations = decoded.document.all_destinations(&[]);
        let slow_judgement_timing = destinations
            .iter()
            .find(|destination| destination.id == "judge_adv_s")
            .expect("starseeker result should keep SLOW timing label destination");
        let fast_judgement_timing = destinations
            .iter()
            .find(|destination| destination.id == "judge_adv_f")
            .expect("starseeker result should keep FAST timing label destination");
        assert_eq!(slow_judgement_timing.draw, "number(374) < 0 or number(375) < 0");
        assert_eq!(fast_judgement_timing.draw, "number(374) > 0 or number(375) > 0");
        assert!(
            decoded.document.all_destinations(&[]).iter().any(|destination| {
                matches!(
                    destination.id.as_str(),
                    "judge_detail" | "judgegraph" | "fsgraph" | "timingGraph"
                )
            }),
            "starseeker result stat destinations should survive lua conversion"
        );
        assert!(
            decoded.document.source.iter().any(|source| source.id == "jud_detail_main"),
            "starseeker result document should keep jud_detail_main source; sources: {:?}",
            decoded.document.source.iter().map(|source| source.id.as_str()).collect::<Vec<_>>()
        );
        let stat_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "jud_detail_main")
            .map(|source| source.texture)
            .expect("starseeker result should load jud_detail_main source");
        let document_textures =
            decoded.sources.iter().map(|source| bmz_render::skin::SkinDocumentTexture {
                source_id: source.source_id.clone(),
                texture: source.texture,
                source_size: bmz_render::skin::SkinImageSize {
                    width: source.size.width,
                    height: source.size.height,
                },
            });
        let context = bmz_render::skin::SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            decoded.document,
            document_textures,
        );
        let bmz_render::scene::AppSceneSnapshot::Result(mut snapshot) =
            bmz_render::sample::sample_result_scene()
        else {
            panic!("sample result scene");
        };
        snapshot.elapsed_time = bmz_core::time::TimeUs(1_000_000);
        snapshot.judge_counts = bmz_render::snapshot::DisplayJudgeCounts {
            pgreat: 120,
            great: 40,
            good: 12,
            bad: 4,
            poor: 3,
            empty_poor: 2,
        };
        snapshot.fast_slow_counts = bmz_render::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 80,
            slow_pgreat: 40,
            fast_great: 12,
            slow_great: 28,
            fast_good: 4,
            slow_good: 8,
            fast_bad: 1,
            slow_bad: 3,
            fast_poor: 1,
            slow_poor: 2,
            fast_empty_poor: 1,
            slow_empty_poor: 1,
        };
        let graph = std::sync::Arc::make_mut(&mut snapshot.graph);
        graph.judge_graph_density = vec![1, 3, 2, 4];
        graph.timing_points = vec![
            bmz_render::snapshot::ResultTimingPoint {
                time_ms: 100,
                delta_us: -12_000,
                judge: bmz_core::judge::Judge::Great,
            },
            bmz_render::snapshot::ResultTimingPoint {
                time_ms: 200,
                delta_us: 8_000,
                judge: bmz_core::judge::Judge::PGreat,
            },
        ];

        let plan = bmz_render::plan::DrawPlan::from_scene_with_skin(
            &bmz_render::scene::AppSceneSnapshot::Result(snapshot),
            &context,
            &mut bmz_render::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            bmz_render::plan::DrawCommand::Image { texture, .. }
                if *texture == bmz_render::plan::TextureId(stat_texture.0)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            bmz_render::plan::DrawCommand::Rect { rect, .. }
                if rect.x > 0.70 && rect.y > 0.20 && rect.y < 0.55
        )));
    }

    #[test]
    fn starseeker_result_misscount_diff_uses_runtime_number_color_block() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/result/result.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([
            ("F/Sリスト".to_string(), "Default".to_string()),
            ("逆サイド詳細フレーム".to_string(), "ON".to_string()),
            ("プレーサイド".to_string(), "1P".to_string()),
        ]);
        let files = BTreeMap::from([
            ("使用テーマ".to_string(), "Theme/starseeker".to_string()),
            ("フォント".to_string(), "_font/TYPE-M".to_string()),
            ("シャッター".to_string(), "Shutter/TYPE-M".to_string()),
        ]);
        let runtime_state = LuaLoadRuntimeState {
            number_values: BTreeMap::from([(178, -1)]),
            text_values: BTreeMap::new(),
            option_values: BTreeMap::new(),
            ..LuaLoadRuntimeState::default()
        };
        let decoded = decode_beatoraja_skin_with_options_and_runtime_state(
            &skin_path,
            SkinKind::Result,
            &options,
            &files,
            &runtime_state,
        )
        .expect("decode starseeker result skin with misscount diff");

        let diff_misscount = decoded
            .document
            .value
            .iter()
            .find(|value| value.id == "Diff_Misscount")
            .expect("starseeker result should define Diff_Misscount");

        assert_eq!(diff_misscount.ref_id, 178);
        assert_eq!(diff_misscount.y, 345);
    }

    #[test]
    fn play_skin_selection_for_returns_per_mode_fields() {
        let mut skin = SkinConfig {
            play4: "skin4.json".to_string(),
            play5: "skin5.json".to_string(),
            play6: "skin6.json".to_string(),
            play7: "skin7.json".to_string(),
            play8: "skin8.json".to_string(),
            play9: "skin9.json".to_string(),
            play10: "skin10.json".to_string(),
            play14: "skin14.json".to_string(),
            ..SkinConfig::default()
        };
        skin.play4_options.insert("g".to_string(), "r".to_string());
        skin.play5_options.insert("a".to_string(), "x".to_string());
        skin.play6_options.insert("f".to_string(), "q".to_string());
        skin.play7_options.insert("b".to_string(), "y".to_string());
        skin.play8_options.insert("h".to_string(), "n".to_string());
        skin.play9_options.insert("e".to_string(), "p".to_string());
        skin.play10_files.insert("c".to_string(), "z.png".to_string());
        skin.play14_files.insert("d".to_string(), "w.png".to_string());
        skin.play7_offsets.push(SkinOffsetConfig { id: 30, h: 7, ..Default::default() });
        skin.play14_offsets.push(SkinOffsetConfig { id: 30, h: 14, ..Default::default() });

        let s4 = play_skin_selection_for(&skin, KeyMode::K4);
        assert_eq!(s4.path, "skin4.json");
        assert!(s4.options.contains_key("g"));

        let s5 = play_skin_selection_for(&skin, KeyMode::K5);
        assert_eq!(s5.path, "skin5.json");
        assert!(s5.options.contains_key("a"));

        let s6 = play_skin_selection_for(&skin, KeyMode::K6);
        assert_eq!(s6.path, "skin6.json");
        assert!(s6.options.contains_key("f"));

        let s7 = play_skin_selection_for(&skin, KeyMode::K7);
        assert_eq!(s7.path, "skin7.json");
        assert!(s7.options.contains_key("b"));
        assert_eq!(s7.offsets[0].h, 7);

        let s8 = play_skin_selection_for(&skin, KeyMode::K8);
        assert_eq!(s8.path, "skin8.json");
        assert!(s8.options.contains_key("h"));

        let s9 = play_skin_selection_for(&skin, KeyMode::K9);
        assert_eq!(s9.path, "skin9.json");
        assert!(s9.options.contains_key("e"));

        let s10 = play_skin_selection_for(&skin, KeyMode::K10);
        assert_eq!(s10.path, "skin10.json");
        assert!(s10.files.contains_key("c"));

        let s14 = play_skin_selection_for(&skin, KeyMode::K14);
        assert_eq!(s14.path, "skin14.json");
        assert!(s14.files.contains_key("d"));
        assert_eq!(s14.offsets[0].h, 14);
    }

    #[test]
    fn apply_skin_from_config_empty_path_uses_default_skin() {
        let mut renderer = Renderer::default();
        let app_paths = test_app_paths();

        apply_skin_from_config(&mut renderer, &app_paths, "").unwrap();
    }

    #[test]
    fn apply_skin_from_config_rejects_toml_skin_directory() {
        let mut renderer = Renderer::default();
        let app_paths = test_app_paths();
        let path = default_skin_root();

        let error = apply_skin_from_config(&mut renderer, &app_paths, path.to_str().unwrap())
            .unwrap_err()
            .to_string();

        assert!(error.contains("BMZ TOML skin directories are no longer supported"), "{error}");
    }

    #[test]
    fn apply_skin_from_config_json_path_loads_beatoraja_skin_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/play7.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();
        let app_paths = test_app_paths();

        apply_skin_from_config(&mut renderer, &app_paths, skin_path.to_str().unwrap()).unwrap();
    }

    #[test]
    fn apply_skin_from_config_lua_path_loads_beatoraja_skin_when_available() {
        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();
        let app_paths = test_app_paths();

        apply_skin_from_config(&mut renderer, &app_paths, skin_path.to_str().unwrap()).unwrap();
    }

    #[test]
    fn rmz_play8_lua_skin_decodes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play8main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert_eq!(decoded.document.skin_type, 24);
        let note = decoded.document.note.as_ref().expect("play8 skin should define notes");
        assert_eq!(note.note.len(), 8);
        assert_eq!(note.dst.len(), 8);
    }

    #[test]
    fn rmz_play7_lanecover_green_renders_green_number_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Rmz-skin/play7main.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let lanecover_green_value = decoded
            .document
            .value
            .iter()
            .find(|value| value.id == "lanecover-green")
            .expect("Rmz lanecover green value should decode");
        assert_eq!(
            lanecover_green_value.value_expr, "0.6*number(312)",
            "decoded value: {lanecover_green_value:?}"
        );
        let source = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "play_system_src")
            .expect("Rmz play system source should decode");
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            total_duration_ms: 500,
            duration_green_ms: Some(300),
            lane_cover_changing: true,
            lanecover_enabled: true,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );
        assert!(
            !items.iter().any(
                |item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "FHS")
            ),
            "FHS mark should stay hidden while NHS is active"
        );
        let digit_width = 20.0;
        let source_candidates = items
            .iter()
            .filter_map(|item| {
                if let bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. } = item
                    && *texture == source.texture
                {
                    Some((
                        (rect.x * 1920.0).round() as i32,
                        (rect.y * 1080.0).round() as i32,
                        (uv.x * source.size.width / digit_width).round() as i32,
                        (uv.y * source.size.height).round() as i32,
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let mut digits = items
            .iter()
            .filter_map(|item| {
                if let bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, .. } = item
                    && *texture == source.texture
                    && (rect.y * 1080.0 - 10.0).abs() < 2.0
                    && (rect.x * 1920.0 - 849.0).abs() < 80.0
                {
                    let digit = (uv.x * source.size.width / digit_width).round() as i32;
                    Some(((rect.x * 1920.0).round() as i32, digit))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        digits.sort_by_key(|(x, _)| *x);
        let digits = digits.into_iter().map(|(_, digit)| digit).collect::<Vec<_>>();

        assert_eq!(digits, vec![3, 0, 0], "source candidates: {source_candidates:?}");

        let fhs_state = bmz_render::skin::SkinDrawState { hispeed_mode_index: 1, ..state.clone() };
        let fhs_items = decoded.document.static_render_items(
            &sources,
            &fhs_state,
            &bmz_render::skin::SkinTextState::default(),
        );
        assert!(
            fhs_items.iter().any(
                |item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "FHS")
            ),
            "FHS mark should render while FHS is active"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_decodes_play_document_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        assert_eq!(decoded.document.name, "WMII FHD play AC");
        assert!(decoded.document.w >= 1920);
        assert!(decoded.document.source.len() >= 10);
        assert!(decoded.document.image.len() >= 100);
        assert!(
            decoded.document.source.iter().any(|source| source.id == "110")
                && decoded.document.source.iter().any(|source| source.id == "111"),
            "expected LR2 black/white reference sources"
        );
        let note = decoded.document.note.as_ref().expect("lr2 play skin should define notes");
        assert!(!note.group.is_empty());
        assert!(decoded.document.gauge.is_some());
        assert!(decoded.document.bga.is_some());
        assert!(
            decoded.sources.len() >= 10,
            "expected WMII sources to decode, got {}; source paths: {:?}; decoded: {:?}",
            decoded.sources.len(),
            decoded.document.source.iter().map(|source| source.path.as_str()).collect::<Vec<_>>(),
            decoded.sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>()
        );
        let black = decoded.sources.iter().find(|source| source.source_id == "110").unwrap();
        let white = decoded.sources.iter().find(|source| source.source_id == "111").unwrap();
        assert_eq!(black.asset.as_ref().unwrap().pixels, vec![0, 0, 0, 255]);
        assert_eq!(white.asset.as_ref().unwrap().pixels, vec![255, 255, 255, 255]);
    }

    #[test]
    fn wmii_fhd_lr2skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn wmii_fhd_lr2skin_produces_static_play_items_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([
            ("GRAPH SIDE".to_string(), "LEFT".to_string()),
            ("Score Graph".to_string(), "On".to_string()),
        ]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            ready_timer_ms: Some(2_000),
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_play_fadeout_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let black_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "110")
            .map(|source| source.texture)
            .expect("WMII black reference source should decode");
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState { fadeout_ms: Some(500), ..Default::default() };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == black_texture
                        && rect.width >= 0.99
                        && rect.height >= 0.99
                        && tint.a > 0.99
            )),
            "expected WMII timer=2 fadeout to draw an opaque fullscreen black image"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_decodes_auto_judge_button_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Displayjudge".to_string(), "ON".to_string())]);
        let decoded = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
        )
        .unwrap()
        .document;
        let candidates = decoded
            .image
            .iter()
            .filter(|image| image.divx == 1 && image.divy >= 2 && image.h > 0)
            .map(|image| {
                format!(
                    "src={} x={} y={} w={} h={} divy={} ref={} act={:?}",
                    image.src,
                    image.x,
                    image.y,
                    image.w,
                    image.h,
                    image.divy,
                    image.ref_id,
                    image.act
                )
            })
            .collect::<Vec<_>>();
        let auto_judge = decoded
            .image
            .iter()
            .find(|image| image.act == Some(75) && image.divx == 1 && image.divy >= 2)
            .unwrap_or_else(|| {
                panic!(
                    "WMII auto judge button should decode; candidates: {}",
                    candidates.join(" | ")
                )
            });

        assert_eq!(auto_judge.ref_id, 0);
        assert_eq!(auto_judge.click, 0);
        assert!(
            auto_judge.h > 0,
            "WMII auto judge button should keep a positive source height: {auto_judge:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_ac_bga_frame_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let frame_image = decoded
            .document
            .image
            .iter()
            .find(|image| image.src == "2" && image.x == 1016 && image.y == 1276 && image.w == 389)
            .expect("WMII AC frame image should decode");
        let mut destinations = Vec::new();
        for entry in &decoded.document.destination {
            match entry {
                bmz_render::skin::DestinationListEntry::Single(destination) => {
                    destinations.push(destination);
                }
                bmz_render::skin::DestinationListEntry::Conditional {
                    destinations: nested,
                    ..
                } => {
                    destinations.extend(nested.iter());
                }
            }
        }
        let frame_destination = destinations
            .into_iter()
            .find(|destination| {
                destination.id == frame_image.id
                    && destination.op.contains(&33)
                    && destination.op.contains(&41)
                    && destination.op.contains(&30)
            })
            .expect("WMII AC frame destination should decode");
        assert!(
            frame_destination.dst.len() >= 2,
            "expected WMII AC frame destination keyframes, got {:?}",
            frame_destination.dst
        );
        let frame_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "2")
            .expect("WMII AC frame source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            ready_timer_ms: Some(2_000),
            has_bga: true,
            bga_enabled: true,
            autoplay: true,
            skin_loaded: true,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == frame_texture
                        && (rect.width - 389.0 / 1920.0).abs() < 0.001
                        && tint.a > 0.5
            )),
            "expected WMII AC BGA frame item from source 2; got {items:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_uses_full_note_lane_region_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let area = decoded
            .document
            .note_lane_area(
                bmz_core::lane::Lane::Scratch,
                bmz_core::lane::KeyMode::K7,
                &decoded.document.enabled_options(),
            )
            .expect("WMII scratch lane area should decode");

        assert!((area.x - 75.0 / 1920.0).abs() < 0.001);
        assert!(
            area.height > 0.65,
            "expected LR2 note.dst to define the full scroll lane height, got {area:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_maps_note_sources_by_lr2_lane_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let note = decoded.document.note.as_ref().expect("WMII notes should decode");
        let images = decoded.document.image_map();
        let scratch =
            images.get(note.note[7].as_str()).expect("WMII scratch note image should resolve");
        let key1 = images.get(note.note[0].as_str()).expect("WMII key1 note image should resolve");
        let key2 = images.get(note.note[1].as_str()).expect("WMII key2 note image should resolve");

        assert_eq!((scratch.x, scratch.w), (94, 90));
        assert_eq!((key1.x, key1.w), (187, 52));
        assert_eq!((key2.x, key2.w), (241, 40));
    }

    #[test]
    fn wmii_fhd_lr2skin_inserts_notes_marker_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        assert!(
            decoded
                .document
                .all_destinations(&decoded.document.enabled_options())
                .iter()
                .any(|destination| destination.id == "notes"),
            "LR2 play skins should insert the notes marker at the first DST_NOTE command"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_groove_gauge_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let gauge_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "19")
            .expect("WMII gauge source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for gauge_type in [
            bmz_core::clear::GaugeType::AssistEasy,
            bmz_core::clear::GaugeType::Normal,
            bmz_core::clear::GaugeType::Hard,
        ] {
            let state = bmz_render::skin::SkinDrawState {
                elapsed_ms: 2_000,
                play_timer_ms: Some(2_000),
                gauge: 80.0,
                gauge_max: 100.0,
                gauge_border: 80.0,
                gauge_type: gauge_type as i32,
                ..Default::default()
            };

            let items = decoded.document.static_render_items(
                &sources,
                &state,
                &bmz_render::skin::SkinTextState::default(),
            );
            assert!(
                items.iter().any(|item| matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                        if *texture == gauge_texture
                            && (rect.x - 54.0 / 1920.0).abs() < 0.001
                            && rect.width > 0.004
                            && tint.a > 0.5
                )),
                "expected WMII groove gauge item from source 19 for {gauge_type:?}; got {items:?}"
            );
        }
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_lift_cover_when_lifted() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        assert!(
            decoded.document.hidden_cover.iter().any(|cover| cover.id.contains("liftcover")
                && cover.disappear_line == 357
                && !cover.is_disappear_line_link_lift),
            "expected LR2 SRC_LIFT to decode as a liftcover hiddenCover"
        );
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let lift_cover = decoded
            .document
            .hidden_cover
            .iter()
            .find(|cover| cover.id.contains("liftcover"))
            .expect("WMII lift cover hiddenCover should decode");
        let lift_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == lift_cover.src)
            .map(|source| source.texture)
            .expect("WMII lift source should decode");
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            offset_lift_px: 0,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            !items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, tint, .. }
                    if *texture == lift_texture && tint.a > 0.5
            )),
            "expected WMII LIFT cover to stay hidden while lift offset is zero"
        );

        let lifted_items = decoded.document.static_render_items(
            &sources,
            &bmz_render::skin::SkinDrawState {
                elapsed_ms: 2_000,
                play_timer_ms: Some(2_000),
                offset_lift_px: 200,
                lift: 200.0 / 1080.0,
                lift_enabled: true,
                ..Default::default()
            },
            &bmz_render::skin::SkinTextState::default(),
        );
        assert!(
            lifted_items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == lift_texture && rect.height < 0.25 && tint.a > 0.5
            )),
            "expected WMII LIFT cover to render clipped once lift offset is active; got {lifted_items:?}"
        );
    }

    #[test]
    fn wmii_fhd_luaskin_renders_lift_cover_when_lifted() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/play7wide.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let lift_cover = decoded
            .document
            .lift_cover
            .iter()
            .find(|cover| cover.id.eq_ignore_ascii_case("lift"))
            .unwrap_or_else(|| {
                panic!(
                    "WMII Lua lift cover should decode; got {:?}",
                    decoded
                        .document
                        .lift_cover
                        .iter()
                        .map(|cover| (&cover.id, &cover.src))
                        .collect::<Vec<_>>()
                )
            });
        let lift_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == lift_cover.src)
            .map(|source| source.texture)
            .expect("WMII Lua lift source should decode");
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();

        let lifted_items = decoded.document.static_render_items(
            &sources,
            &bmz_render::skin::SkinDrawState {
                elapsed_ms: 2_000,
                play_timer_ms: Some(2_000),
                offset_lift_px: 200,
                lift: 200.0 / 1080.0,
                lift_enabled: true,
                ..Default::default()
            },
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            lifted_items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, tint, .. }
                    if *texture == lift_texture && tint.a > 0.5
            )),
            "expected WMII Lua LIFT cover to render once lift offset is active; got {lifted_items:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_moves_judge_line_with_lift_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let judge_line_ids = decoded
            .document
            .image
            .iter()
            .filter(|image| image.src == "1" && image.x == 1231 && image.y == 0)
            .map(|image| image.id.as_str())
            .collect::<Vec<_>>();
        assert!(!judge_line_ids.is_empty(), "expected WMII judge line source image");

        assert!(
            decoded
                .document
                .all_destinations(&decoded.document.enabled_options())
                .iter()
                .any(|destination| judge_line_ids.contains(&destination.id.as_str())
                    && destination.offsets.contains(&3)),
            "expected WMII DST_JUDGELINE to include beatoraja default OFFSET_LIFT"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_score_graph_bars_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            total_notes: 1_000,
            past_notes: 500,
            ex_score: 1_000,
            best_ex_score: Some(1_300),
            projected_best_ex_score: Some(650),
            target_ex_score: Some(1_500),
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { rect, tint, .. }
                    if (rect.x - 546.0 / 1920.0).abs() < 0.01
                        && (rect.width - 277.0 / 1920.0).abs() < 0.01
                        && (rect.height - 798.0 / 1080.0).abs() < 0.01
                        && tint.a > 0.5
            )),
            "expected WMII score graph frame/background to render on the left side"
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { rect, .. }
                    if (rect.x - 670.0 / 1920.0).abs() < 0.01
                        && rect.width > 0.0
                        && rect.height > 0.05
            )),
            "expected WMII score graph bars to render in the graph area"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_keeps_score_graph_and_extends_bga_on_autoplay_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([
            ("BGA Size".to_string(), "Extend".to_string()),
            ("Score Graph".to_string(), "On".to_string()),
        ]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();
        let frame_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "2")
            .expect("WMII AC frame source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            ready_timer_ms: Some(2_000),
            has_bga: true,
            bga_enabled: true,
            autoplay: true,
            skin_loaded: true,
            total_notes: 1_000,
            past_notes: 500,
            ex_score: 1_000,
            best_ex_score: Some(1_300),
            target_ex_score: Some(1_500),
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == frame_texture
                        && (rect.x - 726.0 / 1920.0).abs() < 0.01
                        && (rect.width - 1027.0 / 1920.0).abs() < 0.01
                        && tint.a > 0.5
            )),
            "expected WMII autoplay extended BGA frame to render; got {items:?}"
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { rect, tint, .. }
                    if (rect.x - 546.0 / 1920.0).abs() < 0.01
                        && (rect.width - 277.0 / 1920.0).abs() < 0.01
                        && (rect.height - 798.0 / 1080.0).abs() < 0.01
                        && tint.a > 0.5
            )),
            "expected WMII score graph frame to render during autoplay"
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { rect, tint, .. }
                    if (rect.x - 551.0 / 1920.0).abs() < 0.01
                        && (rect.width - 267.0 / 1920.0).abs() < 0.01
                        && tint.a > 0.5
            )),
            "expected WMII score graph target labels to render during autoplay"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_lane_cover_and_lift_numbers_when_adjusting() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let source1 = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "1")
            .expect("WMII number source should decode");
        let number_uv_y = 883.0 / source1.size.height;
        let number_uv_h = 20.0 / source1.size.height;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            lane_cover: 0.290,
            lift: 0.222,
            total_duration_ms: 517,
            offset_lift_px: (0.222_f32 * 723.0).round() as i32,
            offset_lanecover_px: -(723.0_f32 * 0.290).round() as i32,
            lane_cover_changing: true,
            lanecover_enabled: true,
            lift_enabled: true,
            now_bpm: 88.0,
            main_bpm: 88.0,
            min_bpm: 38.0,
            max_bpm: 156.0,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        let number_digits = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, uv, .. }
                        if *texture == source1.texture
                            && (uv.y - number_uv_y).abs() < 0.001
                            && (uv.height - number_uv_h).abs() < 0.001
                )
            })
            .collect::<Vec<_>>();
        let white_digits = number_digits
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { tint, .. }
                        if tint.r > 0.95 && tint.g > 0.95 && tint.b > 0.95 && tint.a > 0.5
                )
            })
            .count();
        let green_digits = number_digits
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { tint, .. }
                        if tint.r < 0.4 && tint.g > 0.75 && tint.b < 0.5 && tint.a > 0.5
                )
            })
            .count();
        let green_bpm_cover_digits = number_digits
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { tint, rect, .. }
                        if tint.r < 0.4
                            && tint.g > 0.75
                            && tint.b < 0.5
                            && tint.a > 0.5
                            && (rect.y * 1080.0 - 165.0).abs() < 2.0
                )
            })
            .count();
        let green_bpm_no_cover_digits = number_digits
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { tint, rect, .. }
                        if tint.r < 0.4
                            && tint.g > 0.75
                            && tint.b < 0.5
                            && tint.a > 0.5
                            && (rect.y * 1080.0 - 203.0).abs() < 2.0
                )
            })
            .count();
        let green_digit_ys = number_digits
            .iter()
            .filter_map(|item| {
                if let bmz_render::skin::SkinRenderItem::Image { tint, rect, .. } = item
                    && tint.r < 0.4
                    && tint.g > 0.75
                    && tint.b < 0.5
                    && tint.a > 0.5
                {
                    Some((rect.y * 1080.0).round() as i32)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        assert!(
            white_digits >= 6,
            "expected WMII SUDDEN and LIFT white number digits to render; got {white_digits}"
        );
        assert!(
            green_digits >= 6,
            "expected WMII upper and lower green number digits to render; got {green_digits}"
        );
        assert!(
            green_bpm_cover_digits >= 9,
            "expected WMII BPM green digits to use lanecover-on layout; got {green_bpm_cover_digits}; green ys {green_digit_ys:?}"
        );
        assert_eq!(
            green_bpm_no_cover_digits, 0,
            "expected WMII BPM green digits not to use lanecover-off layout when op271 is active"
        );

        let zero_lift_state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            lane_cover: 0.290,
            lift: 0.0,
            total_duration_ms: 517,
            offset_lift_px: 0,
            offset_lanecover_px: -(723.0_f32 * 0.290).round() as i32,
            lane_cover_changing: true,
            lanecover_enabled: true,
            lift_enabled: true,
            now_bpm: 88.0,
            main_bpm: 88.0,
            min_bpm: 38.0,
            max_bpm: 156.0,
            ..Default::default()
        };
        let zero_lift_items = decoded.document.static_render_items(
            &sources,
            &zero_lift_state,
            &bmz_render::skin::SkinTextState::default(),
        );
        let zero_lift_digits = zero_lift_items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, uv, rect, .. }
                        if *texture == source1.texture
                            && (uv.y - number_uv_y).abs() < 0.001
                            && (uv.height - number_uv_h).abs() < 0.001
                            && (rect.y * 1080.0 - 724.0).abs() < 2.0
                )
            })
            .collect::<Vec<_>>();
        let zero_lift_white_digits = zero_lift_digits
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { tint, .. }
                        if tint.r > 0.95 && tint.g > 0.95 && tint.b > 0.95 && tint.a > 0.5
                )
            })
            .count();
        let zero_lift_green_digits = zero_lift_digits
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { tint, .. }
                        if tint.r < 0.4 && tint.g > 0.75 && tint.b < 0.5 && tint.a > 0.5
                )
            })
            .count();
        assert!(
            zero_lift_white_digits > 0,
            "expected WMII LIFT white digits to render even when LIFT is zero"
        );
        assert!(
            zero_lift_green_digits > 0,
            "expected WMII LIFT green digits to render even when LIFT is zero"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_runtime_difficulty_badge_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            difficulty: 4,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { rect, tint, .. }
                    if (rect.x - 617.0 / 1920.0).abs() < 0.01
                        && (rect.width - 187.0 / 1920.0).abs() < 0.01
                        && tint.a > 0.1
            )),
            "expected WMII ANOTHER difficulty badge to render for difficulty op154"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_judge_and_combo_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Displayjudge".to_string(), "ON".to_string())]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();
        let judge_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "13")
            .expect("WMII judge source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut judge_ms = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
        judge_ms[0] = Some(100);
        let mut judge_index = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
        judge_index[0] = Some(0);
        let mut judge_combo = [0; bmz_render::skin::MAX_JUDGE_REGIONS];
        judge_combo[0] = 123;
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            judge_ms,
            judge_index,
            judge_combo,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );
        let judge_items = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                        if *texture == judge_texture
                            && rect.height > 0.01
                            && tint.a > 0.5
                )
            })
            .count();

        assert!(
            judge_items >= 2,
            "expected WMII judge text and combo digits from source 13; got {items:?}"
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, tint, .. }
                    if *texture == judge_texture
                        && rect.height > 0.05
                        && uv.y < 0.001
                        && tint.a > 0.5
            )),
            "expected PGREAT judge image to use the top WMII judge source row; got {items:?}"
        );

        for (judge_index, label) in ["PGREAT", "GREAT", "GOOD", "BAD", "POOR"].iter().enumerate() {
            let mut judge_ms = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
            judge_ms[0] = Some(100);
            let mut judge_indices = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
            judge_indices[0] = Some(judge_index);
            let mut judge_combo = [0; bmz_render::skin::MAX_JUDGE_REGIONS];
            judge_combo[0] = 123;
            let state = bmz_render::skin::SkinDrawState {
                elapsed_ms: 2_000,
                play_timer_ms: Some(2_000),
                judge_ms,
                judge_index: judge_indices,
                judge_combo,
                ..Default::default()
            };
            let items = decoded.document.static_render_items(
                &sources,
                &state,
                &bmz_render::skin::SkinTextState::default(),
            );
            assert!(
                items.iter().any(|item| matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                        if *texture == judge_texture
                            && rect.height > 0.05
                            && tint.a > 0.5
                )),
                "expected WMII {label} judge image to render; got {items:?}"
            );
        }
    }

    #[test]
    fn wmii_fhd_lr2skin_dp_renders_judge_detail_panel_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC_DP.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([
            ("Displayjudge".to_string(), "ON".to_string()),
            ("GRAPH SIDE".to_string(), "RIGHT".to_string()),
            ("Score Graph".to_string(), "On".to_string()),
        ]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(
            decoded.document.enabled_options().contains(&983),
            "expected WMII DP judge detail panel op983 to stay enabled"
        );

        let frame_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "1")
            .expect("WMII frame source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.size.width,
                            height: source.size.height,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            key_mode: bmz_core::lane::KeyMode::K14,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );

        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == frame_texture
                        && (rect.x - 71.0 / 1920.0).abs() < 0.01
                        && (rect.width - 247.0 / 1920.0).abs() < 0.02
                        && rect.height > 0.1
                        && tint.a > 0.1
            )),
            "expected WMII DP judge detail panel body to render; got {items:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_fast_slow_during_replay_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Display FAST/SLOW".to_string(), "ON-A".to_string())]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();
        let sources = decoded.sources.iter().map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: SkinImageSize { width: source.size.width, height: source.size.height },
        });
        let skin = SkinContext::from_manifest_and_document(
            SkinManifest::default(),
            decoded.document.clone(),
            sources,
        );
        let replay_snapshot = bmz_render::snapshot::RenderSnapshot {
            time: TimeUs(100_000),
            play_elapsed_time: TimeUs(100_000),
            replay_playback: true,
            key_mode: bmz_core::lane::KeyMode::K7,
            recent_judgements: vec![bmz_render::snapshot::DisplayJudgement {
                lane: bmz_core::lane::Lane::Key1,
                judge: bmz_core::judge::Judge::PGreat,
                side: Some(bmz_core::judge::TimingSide::Fast),
                text: "PGREAT FAST".to_string(),
                combo: 1,
                delta_us: -2_000,
                time: TimeUs(0),
                is_miss: false,
                timing_ms_suppressed: false,
            }],
            ..Default::default()
        };
        let has_wmii_fast_slow_image = |plan: &DrawPlan| {
            plan.commands.iter().any(|command| {
                matches!(
                    command,
                    DrawCommand::Image { rect, tint, .. }
                        if ((rect.x - 292.0 / 1920.0).abs() < 0.01
                            || (rect.x - 246.0 / 1920.0).abs() < 0.01)
                            && (rect.y - 502.0 / 1080.0).abs() < 0.01
                            && (rect.width - 82.0 / 1920.0).abs() < 0.01
                            && tint.a > 0.5
                )
            })
        };

        let mut snapshot = replay_snapshot.clone();
        crate::screens::play_snapshot::apply_fast_slow_display_filter(
            &mut snapshot,
            0,
            crate::config::profile_config::FastSlowDisplayScope::ThresholdMs,
        );

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut DynamicTimerRuntime::default(),
        );

        assert!(
            has_wmii_fast_slow_image(&plan),
            "expected WMII replay PGREAT FAST/SLOW image to render; got {:?}",
            plan.commands
        );

        let mut auto_snapshot = replay_snapshot;
        crate::screens::play_snapshot::apply_fast_slow_display_filter(
            &mut auto_snapshot,
            0,
            crate::config::profile_config::FastSlowDisplayScope::Auto,
        );
        let auto_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(auto_snapshot),
            &skin,
            &mut DynamicTimerRuntime::default(),
        );

        assert!(
            !has_wmii_fast_slow_image(&auto_plan),
            "expected WMII Auto scope to hide replay PGREAT FAST/SLOW; got {:?}",
            auto_plan.commands
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_applies_play_timing_headers_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert_eq!(decoded.document.loadstart, 0);
        assert_eq!(decoded.document.loadend, 3000);
        assert_eq!(decoded.document.playstart, 1500);
        assert_eq!(decoded.document.fadeout, 500);
        assert_eq!(decoded.document.close, 2500);
    }

    #[test]
    fn wmii_fhd_lr2skin_uses_lr2_bitmap_fonts_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert!(
            decoded.document.font.iter().any(|font| {
                font.id.starts_with("lr2font-")
                    && font.path.replace('\\', "/").ends_with("../font/songTitle/font.fnt")
            }),
            "expected LR2FONT font.lr2font to resolve to bundled font.fnt; got {:?}",
            decoded.document.font
        );
        assert!(
            decoded.document.text.iter().any(|text| {
                text.ref_id == 12 && text.font.starts_with("play:lr2font-") && text.size == 0
            }),
            "expected full-title text to keep its LR2 bitmap font id; got {:?}",
            decoded.document.text
        );
        assert!(
            decoded.document.text.iter().any(|text| {
                text.ref_id == 10 && text.font.starts_with("play:lr2font-") && text.size == 0
            }),
            "expected READY title text to use LR2 bitmap font index 0; got {:?}",
            decoded.document.text
        );
        assert!(
            decoded.document.text.iter().any(|text| {
                text.ref_id == 14 && text.font.starts_with("play:lr2font-") && text.size == 0
            }),
            "expected artist text to keep its LR2 bitmap font id; got {:?}",
            decoded.document.text
        );
        assert!(
            decoded.fonts.iter().any(|font| {
                font.stored_id.starts_with("play:lr2font-")
                    && matches!(font.data.as_ref(), Some(DecodedFontData::Bitmap(_)))
            }),
            "expected decoded LR2 bitmap font to be loaded"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_uses_dst_text_size_for_lr2_bitmap_fonts_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let title_id = decoded
            .document
            .text
            .iter()
            .find(|text| text.ref_id == 12)
            .map(|text| text.id.as_str())
            .expect("WMII full-title text should exist");
        let has_frame_height = |id: &str, height: i32| {
            decoded.document.destination.iter().any(|entry| match entry {
                bmz_render::skin::DestinationListEntry::Single(destination) => {
                    destination.id == id
                        && destination.dst.iter().any(|frame| match frame {
                            bmz_render::skin::SkinDstEntry::Frame(frame) => frame.h == Some(height),
                            bmz_render::skin::SkinDstEntry::Conditional { frames, .. } => {
                                frames.iter().any(|frame| frame.h == Some(height))
                            }
                        })
                }
                bmz_render::skin::DestinationListEntry::Conditional { destinations, .. } => {
                    destinations.iter().any(|destination| {
                        destination.id == id
                            && destination.dst.iter().any(|frame| match frame {
                                bmz_render::skin::SkinDstEntry::Frame(frame) => {
                                    frame.h == Some(height)
                                }
                                bmz_render::skin::SkinDstEntry::Conditional { frames, .. } => {
                                    frames.iter().any(|frame| frame.h == Some(height))
                                }
                            })
                    })
                }
            })
        };

        assert!(
            has_frame_height(title_id, 41),
            "expected WMII full-title bitmap font size to come from DST_TEXT h=41"
        );
        assert!(
            decoded.document.text.iter().any(|text| {
                text.ref_id == 14
                    && text.font.starts_with("play:lr2font-")
                    && has_frame_height(&text.id, 29)
            }),
            "expected WMII artist bitmap font size to come from DST_TEXT h=29"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_uses_lr2_bitmap_font_for_table_level_when_enabled() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Display Table Level".to_string(), "ON".to_string())]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(
            decoded.document.text.iter().any(|text| {
                text.ref_id == 1002 && text.font.starts_with("play:lr2font-") && text.size == 0
            }),
            "expected difficulty-table text to keep its LR2 bitmap font id; got {:?}",
            decoded.document.text
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_preserves_green_number_digit_width_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let green_numbers = decoded
            .document
            .value
            .iter()
            .filter(|value| matches!(value.ref_id, 313 | 1317 | 1321 | 1325))
            .collect::<Vec<_>>();

        assert!(!green_numbers.is_empty(), "expected WMII green-number value sprites");
        assert!(
            green_numbers.iter().all(|value| value.digit == 3),
            "LR2 keta field should remain 3 digits for WMII green numbers; got {green_numbers:?}"
        );

        assert!(
            decoded.document.value.iter().any(|value| value.ref_id == 310 && value.digit == 1),
            "expected WMII white high-speed integer digit to use LR2 keta=1"
        );
        assert!(
            decoded.document.value.iter().any(|value| value.ref_id == 311 && value.digit == 2),
            "expected WMII white high-speed decimal digits to use LR2 keta=2"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_keeps_runtime_difficulty_option_destinations_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        for op in 150..=155 {
            assert!(
                decoded.document.destination.iter().any(|entry| match entry {
                    bmz_render::skin::DestinationListEntry::Single(destination) =>
                        destination.op.contains(&op),
                    bmz_render::skin::DestinationListEntry::Conditional {
                        destinations, ..
                    } => destinations.iter().any(|destination| destination.op.contains(&op)),
                }),
                "expected runtime difficulty op {op} to survive LR2 #IF conversion"
            );
        }
    }

    #[test]
    fn wmii_fhd_lr2skin_uses_relative_combo_destination_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("Displayjudge".to_string(), "ON".to_string())]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(
            decoded.document.judge.iter().flat_map(|judge| &judge.numbers).any(|number| {
                number.dst.iter().any(|entry| match entry {
                    bmz_render::skin::SkinDstEntry::Frame(frame) => {
                        frame.x == Some(242) && frame.y == Some(0) && frame.h == Some(124)
                    }
                    bmz_render::skin::SkinDstEntry::Conditional { frames, .. } => {
                        frames.iter().any(|frame| {
                            frame.x == Some(242) && frame.y == Some(0) && frame.h == Some(124)
                        })
                    }
                })
            }),
            "expected WMII NOWCOMBO destination to stay relative to judge image"
        );
        assert!(
            decoded
                .document
                .judge
                .iter()
                .flat_map(|judge| &judge.images)
                .any(|image| { image.offsets.contains(&3) && image.offsets.contains(&32) }),
            "expected WMII NOWJUDGE destinations to include beatoraja LR2 judge and lift offsets"
        );
        assert!(
            decoded
                .document
                .judge
                .iter()
                .flat_map(|judge| &judge.numbers)
                .any(|number| { number.offsets.contains(&3) && number.offsets.contains(&32) }),
            "expected WMII NOWCOMBO destinations to include beatoraja LR2 judge and lift offsets"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_enables_score_graph_by_default_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert!(
            decoded.document.graph.iter().any(|graph| matches!(graph.graph_type, 110..=115)),
            "expected WMII score graph bar definitions to load"
        );
        assert!(
            decoded.document.destination.iter().any(|entry| match entry {
                bmz_render::skin::DestinationListEntry::Single(destination) =>
                    destination.op.contains(&39),
                bmz_render::skin::DestinationListEntry::Conditional { destinations, .. } =>
                    destinations.iter().any(|destination| destination.op.contains(&39)),
            }),
            "expected WMII score graph destinations to keep op39 enabled by default"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_2p_side_maps_single_play_notes_to_active_lanes_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let options = BTreeMap::from([("PLAY SIDE".to_string(), "2P".to_string())]);
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &options,
            &BTreeMap::new(),
        )
        .unwrap();
        let note = decoded.document.note.as_ref().expect("WMII note definition should load");

        assert!(
            note.dst.len() <= 8,
            "single-play 2P side should remap LR2 2P lanes into active lanes; got {} dst lanes",
            note.dst.len()
        );
        assert!(
            note.dst.iter().take(8).any(|entry| match entry {
                bmz_render::skin::SkinDstEntry::Frame(frame) =>
                    frame.w.unwrap_or_default() > 0 && frame.h.unwrap_or_default() > 0,
                bmz_render::skin::SkinDstEntry::Conditional { frames, .. } =>
                    frames.iter().any(|frame| {
                        frame.w.unwrap_or_default() > 0 && frame.h.unwrap_or_default() > 0
                    }),
            }),
            "expected remapped 2P note lanes to have visible destinations"
        );
    }

    #[test]
    fn wildcard_skin_source_prefers_filepath_default() {
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/default.png"), []).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
        )
        .unwrap();

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &BTreeMap::new())
                .unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
    }

    #[test]
    fn wildcard_skin_source_prefers_user_file_selection() {
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/default.png"), []).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
        )
        .unwrap();
        // ユーザ選択は `def` (blue) より優先される。
        let files = BTreeMap::from([("Parts".to_string(), "parts/default.png".to_string())]);

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
    }

    #[test]
    fn wildcard_skin_source_falls_back_when_user_selection_missing() {
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/default.png"), []).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
        )
        .unwrap();
        // 存在しないファイルを選択 → `def` (blue) へフォールバック。
        let files = BTreeMap::from([("Parts".to_string(), "parts/missing.png".to_string())]);

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
    }

    #[test]
    fn wildcard_skin_source_ignores_beatoraja_filter_suffix() {
        let root = unique_test_dir("bmz-json-source-filter");
        std::fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
        std::fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
        std::fs::write(root.join("parts/lanecover_lift/TYPE-M.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    {
                        "name": "レーンカバー",
                        "path": "parts/lanecover_lift/*.png|lanecover|",
                        "def": "default"
                    }
                ]
            }
            "#,
        )
        .unwrap();

        let resolved = resolve_json_skin_source_path(
            &root,
            "parts/lanecover_lift/*.png|lanecover|",
            &document,
            &BTreeMap::new(),
        )
        .unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
    }

    #[test]
    fn wildcard_skin_source_randomly_selects_match() {
        // beatoraja の SkinLoader.getPath 同様、ユーザ選択も def も無いワイルドカードは
        // ロードごとにランダムへ解決する。複数回呼んで両方の候補が選ばれることを確認。
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/a.png"), []).unwrap();
        std::fs::write(root.join("parts/b.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let resolved =
                resolve_json_skin_source_path(&root, "parts/*.png", &document, &BTreeMap::new())
                    .unwrap();
            let name =
                resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
            assert!(name == "a.png" || name == "b.png", "unexpected match {name}");
            seen.insert(name);
        }
        assert_eq!(seen.len(), 2, "both candidates should be selected over many loads");
    }

    #[test]
    fn wildcard_skin_source_explicit_random_overrides_def() {
        // ユーザが明示的に "Random" を選んだら、具体 def があってもランダムにする。
        let root = unique_test_dir("bmz-json-source-explicit-random");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        std::fs::write(root.join("parts/red.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
        )
        .unwrap();
        let files = BTreeMap::from([("Parts".to_string(), RANDOM_FILE_SELECTION.to_string())]);

        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let resolved =
                resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();
            let name =
                resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
            assert!(name == "blue.png" || name == "red.png", "unexpected match {name}");
            seen.insert(name);
        }
        assert_eq!(seen.len(), 2, "explicit Random should ignore def and pick randomly");
    }

    #[test]
    fn wildcard_skin_source_random_def_selects_match() {
        // filepath の def が "Random" の場合も具体ファイルとして解決せずランダムにする。
        let root = unique_test_dir("bmz-json-source-random-def");
        std::fs::create_dir_all(root.join("bg")).unwrap();
        std::fs::write(root.join("bg/one.mp4"), []).unwrap();
        std::fs::write(root.join("bg/two.mp4"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "BG", "path": "bg/*.mp4", "def": "Random" }
                ]
            }
            "#,
        )
        .unwrap();

        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let resolved =
                resolve_json_skin_source_path(&root, "bg/*.mp4", &document, &BTreeMap::new())
                    .unwrap();
            let name =
                resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
            assert!(name == "one.mp4" || name == "two.mp4", "unexpected match {name}");
            seen.insert(name);
        }
        assert_eq!(seen.len(), 2, "def=Random should pick randomly among matches");
    }

    #[test]
    fn wildcard_skin_font_resolves_nested_file() {
        let root = unique_test_dir("bmz-json-font");
        std::fs::create_dir_all(root.join("frame/SP/Default")).unwrap();
        std::fs::write(root.join("frame/SP/Default/song.fnt"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let resolved =
            resolve_json_skin_asset_path(&root, "frame/SP/*/song.fnt", &document, &BTreeMap::new())
                .unwrap();

        assert_eq!(resolved.strip_prefix(&root).unwrap(), Path::new("frame/SP/Default/song.fnt"));
    }

    #[test]
    fn skin_asset_path_resolves_case_insensitive_file_names() {
        let root = unique_test_dir("bmz-json-font-case");
        std::fs::create_dir_all(root.join("_font")).unwrap();
        std::fs::write(root.join("_font/Artist.fnt"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let resolved =
            resolve_json_skin_asset_path(&root, "_font/artist.fnt", &document, &BTreeMap::new())
                .unwrap();

        assert_eq!(resolved.strip_prefix(&root).unwrap(), Path::new("_font/Artist.fnt"));
    }

    #[test]
    fn lr2_document_cache_reuses_when_unused_option_changes() {
        let root = unique_test_dir("bmz-lr2-document-cache-option");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("play.lr2skin");
        std::fs::write(
            &skin_path,
            r#"
#INFORMATION,0,Cache Test,Author
#CUSTOMOPTION,Unused,930,Off,On
#CUSTOMOPTION,Branch,910,Off,On
#IF,911
#IMAGE,on.png
#ELSE
#IMAGE,off.png
#ENDIF
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "off.png");

        let unused_changed = BTreeMap::from([("Unused".to_string(), "On".to_string())]);
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &unused_changed,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Hit);
        assert_eq!(second.document.source[0].path, "off.png");
        assert!(second.document.enabled_options().contains(&931));

        let branch_changed = BTreeMap::from([("Branch".to_string(), "On".to_string())]);
        let third = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &branch_changed,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(third.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(third.document.source[0].path, "on.png");
    }

    #[test]
    fn lr2_document_cache_misses_when_play_side_remap_changes() {
        let root = unique_test_dir("bmz-lr2-document-cache-play-side");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("play.lr2skin");
        std::fs::write(
            &skin_path,
            r#"
#INFORMATION,0,Cache Test,Author
#CUSTOMOPTION,PLAY SIDE,900,1P,2P
#IMAGE,base.png
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "base.png");

        let play_side_2p = BTreeMap::from([("PLAY SIDE".to_string(), "2P".to_string())]);
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &play_side_2p,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(second.document.source[0].path, "base.png");
    }

    #[test]
    fn lr2_document_cache_misses_when_included_file_changes() {
        let root = unique_test_dir("bmz-lr2-document-cache-include");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("play.lr2skin");
        let include_path = root.join("parts.csv");
        std::fs::write(
            &skin_path,
            r#"
#INFORMATION,0,Cache Test,Author
#INCLUDE,parts.csv
"#,
        )
        .unwrap();
        std::fs::write(&include_path, "#IMAGE,off.png\n").unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "off.png");

        std::fs::write(&include_path, "#IMAGE,on-longer-name.png\n").unwrap();
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(second.document.source[0].path, "on-longer-name.png");
    }

    #[test]
    fn lr2_document_cache_misses_when_used_file_selection_changes() {
        let root = unique_test_dir("bmz-lr2-document-cache-file");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        std::fs::write(root.join("parts/red.png"), []).unwrap();
        let skin_path = root.join("play.lr2skin");
        std::fs::write(
            &skin_path,
            r#"
#INFORMATION,0,Cache Test,Author
#CUSTOMFILE,Parts,parts/*.png,blue
#IMAGE,parts/*.png
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "parts/blue.png");

        let selected = BTreeMap::from([("Parts".to_string(), "red.png".to_string())]);
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &selected,
            &LuaLoadRuntimeState::default(),
            Some(cache),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(second.document.source[0].path, "parts/red.png");
    }

    #[test]
    fn lua_document_cache_reuses_when_unused_option_changes() {
        let root = unique_test_dir("bmz-lua-document-cache-option");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("play.luaskin");
        std::fs::write(
            &skin_path,
            r#"
local branch = 910
if skin_config and skin_config.option then
    branch = skin_config.option["Branch"] or 910
end
return {
    type = 0,
    property = {
        { name = "Unused", item = {{ name = "Off", op = 900 }, { name = "On", op = 901 }}, def = "Off" },
        { name = "Branch", item = {{ name = "Off", op = 910 }, { name = "On", op = 911 }}, def = "Off" },
    },
    source = {
        { id = "bg", path = branch == 911 and "on.png" or "off.png" },
    },
}
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "off.png");

        let unused_changed = BTreeMap::from([("Unused".to_string(), "On".to_string())]);
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &unused_changed,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Hit);
        assert_eq!(second.document.source[0].path, "off.png");
        assert!(second.document.enabled_options().contains(&901));

        let branch_changed = BTreeMap::from([("Branch".to_string(), "On".to_string())]);
        let third = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &branch_changed,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache),
        )
        .unwrap();
        assert_eq!(third.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(third.document.source[0].path, "on.png");
    }

    #[test]
    fn lua_document_cache_misses_when_required_module_option_changes() {
        let root = unique_test_dir("bmz-lua-document-cache-required-option");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("play.luaskin");
        let module_path = root.join("parts.lua");
        std::fs::write(
            &skin_path,
            r#"
local parts = require("parts")
return parts.build()
"#,
        )
        .unwrap();
        std::fs::write(
            &module_path,
            r#"
local M = {}
function M.build()
    local branch = 910
    if skin_config and skin_config.option then
        branch = skin_config.option["Branch"] or 910
    end
    return {
        type = 0,
        property = {
            { name = "Unused", item = {{ name = "Off", op = 900 }, { name = "On", op = 901 }}, def = "Off" },
            { name = "Branch", item = {{ name = "Off", op = 910 }, { name = "On", op = 911 }}, def = "Off" },
        },
        source = {
            { id = "bg", path = branch == 911 and "on.png" or "off.png" },
        },
    }
end
return M
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "off.png");

        let unused_changed = BTreeMap::from([("Unused".to_string(), "On".to_string())]);
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &unused_changed,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Hit);
        assert_eq!(second.document.source[0].path, "off.png");

        let branch_changed = BTreeMap::from([("Branch".to_string(), "On".to_string())]);
        let third = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &branch_changed,
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache),
        )
        .unwrap();
        assert_eq!(third.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(third.document.source[0].path, "on.png");
    }

    #[test]
    fn lua_document_cache_misses_when_runtime_number_changes() {
        let root = unique_test_dir("bmz-lua-document-cache-number");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("result.luaskin");
        std::fs::write(
            &skin_path,
            r#"
local main_state = require("main_state")
local diff = main_state.number(178)
return {
    type = 7,
    source = {
        { id = "bg", path = diff == 0 and "zero.png" or "nonzero.png" },
    },
}
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let zero_state = LuaLoadRuntimeState {
            number_values: BTreeMap::from([(178, 0)]),
            text_values: BTreeMap::new(),
            option_values: BTreeMap::new(),
            ..LuaLoadRuntimeState::default()
        };
        let first = load_skin_document(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &zero_state,
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.source[0].path, "zero.png");

        let nonzero_state = LuaLoadRuntimeState {
            number_values: BTreeMap::from([(178, -1)]),
            text_values: BTreeMap::new(),
            option_values: BTreeMap::new(),
            ..LuaLoadRuntimeState::default()
        };
        let second = load_skin_document(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &nonzero_state,
            Some(cache),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(second.document.source[0].path, "nonzero.png");
    }

    #[test]
    fn lua_document_cache_misses_when_runtime_text_changes() {
        let root = unique_test_dir("bmz-lua-document-cache-text");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("select.luaskin");
        std::fs::write(
            &skin_path,
            r#"
local main_state = require("main_state")
return {
    type = 0,
    text = {
        { id = "player", constantText = main_state.text(2) },
    },
}
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                text_values: BTreeMap::from([(2, "Player One".to_string())]),
                ..LuaLoadRuntimeState::default()
            },
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(first.document.text[0].constant_text, "Player One");

        let second = load_skin_document(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState {
                text_values: BTreeMap::from([(2, "Player Two".to_string())]),
                ..LuaLoadRuntimeState::default()
            },
            Some(cache),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(second.document.text[0].constant_text, "Player Two");
    }

    #[test]
    fn lua_document_cache_misses_when_used_file_selection_changes() {
        let root = unique_test_dir("bmz-lua-document-cache-file");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        std::fs::write(root.join("parts/red.png"), []).unwrap();
        let skin_path = root.join("play.luaskin");
        std::fs::write(
            &skin_path,
            r#"
local path = "parts/blue.png"
if skin_config and skin_config.get_path then
    path = skin_config.get_path("parts/*.png")
end
return {
    type = 0,
    filepath = {
        { name = "Parts", path = "parts/*.png", def = "blue" },
    },
    source = {
        { id = "bg", path = path },
    },
}
"#,
        )
        .unwrap();
        let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

        let first = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(
            Path::new(&first.document.source[0].path).canonicalize().unwrap(),
            std::fs::canonicalize(root.join("parts/blue.png")).unwrap()
        );

        let selected = BTreeMap::from([("Parts".to_string(), "red.png".to_string())]);
        let second = load_skin_document(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &selected,
            &LuaLoadRuntimeState::default(),
            Some(cache),
        )
        .unwrap();
        assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
        assert_eq!(
            Path::new(&second.document.source[0].path).canonicalize().unwrap(),
            std::fs::canonicalize(root.join("parts/red.png")).unwrap()
        );
    }

    #[test]
    fn required_skin_sources_excludes_unused_images() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "source": [
                    { "id": 1, "path": "used.png" },
                    { "id": 2, "path": "unused.png" },
                    { "id": 3, "path": "lift.png" }
                ],
                "image": [
                    { "id": "used", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "unused", "src": 2, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "liftCover": [
                    { "id": "lift", "src": 3, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": "used", "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] },
                    { "id": "lift", "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let required = required_skin_source_ids(&document);

        assert!(required.contains("1"));
        assert!(!required.contains("2"));
        assert!(required.contains("3"));
    }

    #[test]
    fn supported_font_paths_include_vector_and_bitmap_fonts() {
        assert!(is_supported_font_path(Path::new("font.ttf")));
        assert!(is_supported_font_path(Path::new("font.OTF")));
        assert!(is_supported_font_path(Path::new("font.ttc")));
        assert!(is_supported_font_path(Path::new("font.fnt")));
        assert!(!is_supported_font_path(Path::new("font.png")));
        assert!(is_bitmap_font_path(Path::new("font.fnt")));
        assert!(!is_bitmap_font_path(Path::new("font.ttf")));
    }

    #[test]
    fn skin_font_cache_hit_skips_loader() {
        let root = unique_test_dir("bmz-font-cache-hit");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("font.ttf");
        std::fs::write(&path, b"not a real font").unwrap();
        let key = skin_font_cache_key(&path).unwrap();
        let expected = vec![1, 2, 3, 4];
        let cache = Arc::new(Mutex::new(SkinFontCache::default()));
        cache.lock().unwrap().insert(key.clone(), DecodedFontData::Vector(expected.clone()));

        let (actual, status, actual_key) = decode_font_with_cache(&path, Some(&cache)).unwrap();

        assert_eq!(status, FontCacheStatus::Hit);
        assert_eq!(actual_key, Some(key));
        match actual {
            DecodedFontData::Vector(bytes) => assert_eq!(bytes, expected),
            DecodedFontData::Bitmap(_) => panic!("expected cached vector font bytes"),
        }
    }

    #[test]
    fn skin_font_cache_evicts_least_recently_used_entry() {
        let mut cache = SkinFontCache::with_limit_bytes(8);
        let a = test_font_cache_key("a.ttf");
        let b = test_font_cache_key("b.ttf");
        let c = test_font_cache_key("c.ttf");

        cache.insert(a.clone(), DecodedFontData::Vector(vec![1, 1, 1, 1]));
        cache.insert(b.clone(), DecodedFontData::Vector(vec![2, 2, 2, 2]));
        assert!(cache.get(&a).is_some());
        cache.insert(c.clone(), DecodedFontData::Vector(vec![3, 3, 3, 3]));

        assert!(cache.get(&a).is_some());
        assert!(cache.get(&b).is_none());
        assert!(cache.get(&c).is_some());
    }

    #[test]
    fn skin_font_cache_skips_entries_larger_than_limit() {
        let mut cache = SkinFontCache::with_limit_bytes(4);
        let key = test_font_cache_key("too-large.ttf");

        cache.insert(key.clone(), DecodedFontData::Vector(vec![1, 2, 3, 4, 5]));

        assert!(cache.get(&key).is_none());
        assert_eq!(cache.total_bytes, 0);
    }

    #[test]
    fn installed_font_snapshot_skips_font_payload_decode() {
        let root = unique_test_dir("bmz-installed-font-skip");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("skin.json");
        let font_path = root.join("font.ttf");
        std::fs::write(&font_path, b"not a real font").unwrap();
        std::fs::write(
            &skin_path,
            r#"
            {
                "type": 0,
                "font": [
                    { "id": "font1", "path": "font.ttf" }
                ]
            }
            "#,
        )
        .unwrap();
        let key = skin_font_cache_key(&font_path).unwrap();
        let installed = HashMap::from([("play:font1".to_string(), key.clone())]);

        let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
            None,
            None,
            None,
            Some(installed),
        )
        .unwrap();

        assert_eq!(decoded.stats.font_count, 1);
        assert_eq!(decoded.stats.font_payload_skipped, 1);
        assert_eq!(decoded.stats.font_cache_hits, 0);
        assert_eq!(decoded.stats.font_cache_misses, 0);
        assert_eq!(decoded.fonts.len(), 1);
        assert_eq!(decoded.fonts[0].stored_id, "play:font1");
        assert_eq!(decoded.fonts[0].cache_key.as_ref(), Some(&key));
        assert!(decoded.fonts[0].data.is_none());
    }

    #[test]
    fn skin_source_asset_cache_hit_skips_loader() {
        let root = unique_test_dir("bmz-source-cache-hit");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.png");
        std::fs::write(&path, b"cached").unwrap();
        let key = skin_source_asset_cache_key(&path, false).unwrap();
        let expected = RgbaImageAsset { width: 1, height: 1, pixels: vec![1, 2, 3, 4] };
        let cache = Arc::new(Mutex::new(SkinSourceAssetCache::default()));
        cache.lock().unwrap().insert(key, expected.clone());

        let (actual, status) = load_source_asset_with_cache(&path, false, Some(&cache), || {
            panic!("cache hit must not call source loader")
        })
        .unwrap();

        assert_eq!(actual, expected);
        assert_eq!(status, SourceCacheStatus::Hit);
    }

    #[test]
    fn skin_source_asset_cache_misses_after_metadata_change() {
        let root = unique_test_dir("bmz-source-cache-metadata");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.png");
        std::fs::write(&path, b"old").unwrap();
        let key = skin_source_asset_cache_key(&path, false).unwrap();
        let stale = RgbaImageAsset { width: 1, height: 1, pixels: vec![1, 2, 3, 4] };
        let fresh = RgbaImageAsset { width: 1, height: 1, pixels: vec![5, 6, 7, 8] };
        let cache = Arc::new(Mutex::new(SkinSourceAssetCache::default()));
        cache.lock().unwrap().insert(key, stale);

        std::fs::write(&path, b"new and longer").unwrap();
        let (actual, status) =
            load_source_asset_with_cache(&path, false, Some(&cache), || Ok(fresh.clone())).unwrap();

        assert_eq!(actual, fresh);
        assert_eq!(status, SourceCacheStatus::Miss);
    }

    #[test]
    fn skin_gpu_texture_cache_reuses_inserted_source_textures() {
        let root = unique_test_dir("bmz-gpu-texture-cache");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.png");
        std::fs::write(&path, b"cached").unwrap();
        let key = skin_source_asset_cache_key(&path, false).unwrap();
        let size = SkinImageSize { width: 64.0, height: 32.0 };
        let mut cache = SkinGpuTextureCache::default();

        let allocated = cache.allocate_texture_id(SkinKind::Play);
        cache.insert(key.clone(), allocated, size);

        let cached = cache.get(&key).unwrap();
        assert_eq!(cached.texture, allocated);
        assert_eq!(cached.size, size);
        assert_ne!(cache.allocate_texture_id(SkinKind::Play), allocated);

        cache.clear();

        assert!(cache.get(&key).is_none());
        assert_eq!(cache.allocate_texture_id(SkinKind::Play), SkinTextureId(10_000));
    }

    #[test]
    fn decode_uses_gpu_texture_cache_to_skip_source_decode() {
        let root = unique_test_dir("bmz-source-texture-cache-hit");
        std::fs::create_dir_all(&root).unwrap();
        let skin_path = root.join("skin.json");
        let source_path = root.join("source.png");
        std::fs::write(&source_path, b"not a png").unwrap();
        std::fs::write(
            &skin_path,
            r#"
            {
                "type": 0,
                "source": [
                    { "id": 1, "path": "source.png" }
                ],
                "image": [
                    { "id": "img", "src": 1, "x": 0, "y": 0, "w": 64, "h": 32 }
                ],
                "destination": [
                    { "id": "img", "dst": [{ "x": 0, "y": 0, "w": 64, "h": 32 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let key = skin_source_asset_cache_key(&source_path, false).unwrap();
        let texture = SkinTextureId(12_345);
        let size = SkinImageSize { width: 64.0, height: 32.0 };
        let texture_cache = Arc::new(Mutex::new(SkinGpuTextureCache::default()));
        texture_cache.lock().unwrap().insert(key.clone(), texture, size);

        let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
            None,
            Some(texture_cache),
            None,
            None,
        )
        .unwrap();

        assert_eq!(decoded.stats.source_texture_cache_hits, 1);
        assert_eq!(decoded.stats.source_texture_cache_hit_bytes, 64 * 32 * 4);
        assert_eq!(decoded.stats.source_cache_hits, 0);
        assert_eq!(decoded.stats.source_cache_misses, 0);
        assert_eq!(decoded.stats.decoded_source_bytes, 0);
        assert_eq!(decoded.sources.len(), 1);
        assert_eq!(decoded.sources[0].texture, texture);
        assert_eq!(decoded.sources[0].size, size);
        assert_eq!(decoded.sources[0].cache_key.as_ref(), Some(&key));
        assert!(decoded.sources[0].asset.is_none());
    }

    #[test]
    fn skin_gpu_texture_cache_reuses_inserted_video_textures_separately() {
        let root = unique_test_dir("bmz-gpu-video-texture-cache");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("source.mp4");
        std::fs::write(&path, b"cached-video").unwrap();
        let image_key = skin_source_asset_cache_key(&path, false).unwrap();
        let video_key = skin_source_asset_cache_key(&path, true).unwrap();
        assert_ne!(image_key, video_key);

        let size = SkinImageSize { width: 320.0, height: 180.0 };
        let mut cache = SkinGpuTextureCache::default();
        let allocated = cache.allocate_texture_id(SkinKind::Play);
        cache.insert(video_key.clone(), allocated, size);

        assert!(cache.get(&image_key).is_none());
        let cached = cache.get(&video_key).unwrap();
        assert_eq!(cached.texture, allocated);
        assert_eq!(cached.size, size);
    }

    fn test_font_cache_key(path: &str) -> SkinFontCacheKey {
        SkinFontCacheKey { path: PathBuf::from(path), modified: None, len: 0, is_bitmap: false }
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        path
    }
}
