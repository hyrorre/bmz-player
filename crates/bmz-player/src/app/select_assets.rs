use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_audio::loudness::analyze_preview_loudness;
use bmz_audio::sample::DecodedSample;
use bmz_render::assets::{AnimatedImage, RgbaImageAsset, load_chart_image};
use bmz_render::skin::SkinImageSize;

use crate::chart_preview::SelectChartPreview;
use crate::generated_preview::lower_current_thread_priority;
use crate::generated_preview::{
    parse_generated_preview_cache_key, render_generated_preview_for_chart,
};

mod stamp;

pub(super) const SELECT_PREVIEW_FADE_DURATION: Duration = Duration::from_millis(150);
const SELECT_PREVIEW_CACHE_LIMIT: usize = 8;
const FOLDER_PREVIEW_PREFIX: &str = "folder-preview|";

fn folder_preview_request(key: &str) -> Option<(u64, &str)> {
    let (selection, folder) = key.strip_prefix(FOLDER_PREVIEW_PREFIX)?.split_once('|')?;
    Some((selection.parse().ok()?, folder))
}

/// Decoded animation frames kept in the meta image cache; older animations keep their
/// first frame only.
const SELECT_META_ANIMATION_CACHE_LIMIT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectMetaImageSlot {
    Stage,
    Backbmp,
    Banner,
}

enum SelectMetaImageCacheEntry {
    Loading,
    Ready(CachedMetaImage),
    Missing,
}

/// A cached still image and the state of its animation frames.
struct CachedMetaImage {
    first: RgbaImageAsset,
    animation: MetaAnimationState,
}

enum MetaAnimationState {
    /// The file has no animation.
    Still,
    Loaded(Arc<AnimatedImage>),
    /// Frames were dropped to stay within the cache budget; decode again when shown.
    Evicted,
    /// A background decode is restoring the evicted frames.
    Reloading,
}

impl CachedMetaImage {
    fn image(&self) -> SelectMetaImage {
        let animation = match &self.animation {
            MetaAnimationState::Loaded(animation) => Some(animation.clone()),
            _ => None,
        };
        SelectMetaImage { first: self.first.clone(), animation }
    }
}

/// A STAGEFILE / BACKBMP / BANNER image: its still (first) frame and animation, if any.
#[derive(Clone)]
pub(super) struct SelectMetaImage {
    pub(super) first: RgbaImageAsset,
    pub(super) animation: Option<Arc<AnimatedImage>>,
}

struct SelectMetaImageResult {
    slot: SelectMetaImageSlot,
    key: String,
    path: Option<PathBuf>,
    result: std::result::Result<SelectMetaImage, String>,
}

enum SelectPreviewCacheEntry {
    Loading,
    Ready(PreparedSelectPreview),
    Missing,
}

#[derive(Clone)]
pub(super) struct PreparedSelectPreview {
    pub(super) sample: DecodedSample,
    pub(super) normalization_gain: f32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectPreviewFade {
    #[default]
    Silent,
    FadingIn {
        started_at: Instant,
    },
    Playing,
    FadingOut {
        started_at: Instant,
    },
}

struct SelectPreviewResult {
    key: String,
    path: Option<PathBuf>,
    result: std::result::Result<PreparedSelectPreview, String>,
}

/// 選曲プレビューの重いロード処理は一度に1件だけ実行する。
/// 選択が変わった間の要求は最新の1件だけ残し、古い要求を捨てる。
#[derive(Debug, Default)]
pub(super) struct SelectPreviewLoadQueue {
    active: bool,
    pending: Option<String>,
}

impl SelectPreviewLoadQueue {
    pub(super) fn request(&mut self, key: String) -> Option<String> {
        if self.active {
            self.pending = Some(key);
            None
        } else {
            self.active = true;
            Some(key)
        }
    }

    pub(super) fn finish(&mut self) -> Option<String> {
        if let Some(next) = self.pending.take() {
            Some(next)
        } else {
            self.active = false;
            None
        }
    }
}

pub(super) struct SelectMetaImageUpload {
    pub(super) slot: SelectMetaImageSlot,
    pub(super) image: SelectMetaImage,
}

#[derive(Debug, Default)]
struct SelectMetaImageState {
    source: Option<String>,
    loaded: bool,
    size: Option<SkinImageSize>,
}

/// 選曲画面固有の画像・試聴音源について、選択変更をまたいで維持するロード状態。
///
/// GPU texture upload、profile 音量の解釈、audio stream 起動は `WinitApp` に残す。
/// worker、CPU cache、ロードキュー、選択変更とfadeの状態遷移は本runtimeが所有する。
pub(super) struct SelectAssetRuntime {
    pub(super) row_banners: super::select_row_banners::RowBannerRuntime,
    source_stamp: stamp::SelectedArchiveStamp,
    pub(super) archive_preload: super::select_archive_preload::SelectArchivePreload,
    stage: SelectMetaImageState,
    backbmp: SelectMetaImageState,
    banner: SelectMetaImageState,
    preview_source: Option<String>,
    preview_selection: Option<(u64, Option<i64>)>,
    folder_preview_selection: u64,
    preview_played_once: bool,
    preview_fade: SelectPreviewFade,
    preview_normalization_gain: f32,
    preview: Option<SelectChartPreview>,
    library_db_path: PathBuf,
    cache_dir: PathBuf,
    meta_image_cache: HashMap<String, SelectMetaImageCacheEntry>,
    /// Cache keys holding loaded animations, least recently shown first, with their size.
    meta_image_animations: VecDeque<(String, u64)>,
    /// Sum of `meta_image_animations` sizes.
    meta_image_animation_bytes: u64,
    meta_image_animation_limit_bytes: u64,
    meta_image_tx: mpsc::Sender<SelectMetaImageResult>,
    meta_image_rx: Receiver<SelectMetaImageResult>,
    preview_cache: HashMap<String, SelectPreviewCacheEntry>,
    preview_tx: mpsc::Sender<SelectPreviewResult>,
    preview_rx: Receiver<SelectPreviewResult>,
    preview_load_queue: SelectPreviewLoadQueue,
    generated_preview_loading: bool,
}

impl SelectAssetRuntime {
    pub(super) fn asset_cache_key(&self, folder: &str, file: &str) -> Option<String> {
        Some(crate::chart_asset::asset_cache_key_with_stamp(
            folder,
            file,
            self.source_stamp.get(folder)?,
        ))
    }

    pub(super) fn update_source(&mut self, folder: Option<&str>, now: Instant) {
        self.source_stamp.update(folder, now);
    }

    pub(super) fn preview_cache_key(
        &self,
        chart_id: i64,
        folder: &str,
        file: &str,
        fallback_start_ms: impl FnOnce() -> Option<i64>,
    ) -> Option<String> {
        let stamp = self.source_stamp.get(folder)?;
        let key = if stamp.is_none() && file.trim().is_empty() {
            // A preview may have been added since the scan or the last selection.
            format!("{FOLDER_PREVIEW_PREFIX}{}|{folder}", self.folder_preview_selection)
        } else {
            crate::chart_asset::asset_cache_key_with_stamp(folder, file, stamp)
        };
        if !matches!(self.preview_cache.get(&key), Some(SelectPreviewCacheEntry::Missing)) {
            return Some(key);
        }
        Some(crate::generated_preview::generated_preview_cache_key_with_stamp(
            chart_id,
            fallback_start_ms()?,
            folder,
            stamp,
        ))
    }

    pub(super) fn invalidate_library(&mut self) {
        self.stop_preview();
        // Keep the bounded worker pool alive; old generations are discarded.
        let mut row_banners = std::mem::take(&mut self.row_banners);
        row_banners.invalidate();
        // Drop old worker receivers too, so stale results cannot refill the caches.
        *self =
            Self::new(self.preview.take(), self.library_db_path.clone(), self.cache_dir.clone());
        self.row_banners = row_banners;
    }

    pub(super) fn new(
        preview: Option<SelectChartPreview>,
        library_db_path: PathBuf,
        cache_dir: PathBuf,
    ) -> Self {
        let (meta_image_tx, meta_image_rx) = mpsc::channel();
        let (preview_tx, preview_rx) = mpsc::channel();
        Self {
            row_banners: Default::default(),
            source_stamp: Default::default(),
            archive_preload: Default::default(),
            stage: SelectMetaImageState::default(),
            backbmp: SelectMetaImageState::default(),
            banner: SelectMetaImageState::default(),
            preview_source: None,
            preview_selection: None,
            folder_preview_selection: 0,
            preview_played_once: false,
            preview_fade: SelectPreviewFade::Silent,
            preview_normalization_gain: 1.0,
            preview,
            library_db_path,
            cache_dir,
            meta_image_cache: HashMap::new(),
            meta_image_animations: VecDeque::new(),
            meta_image_animation_bytes: 0,
            meta_image_animation_limit_bytes: SELECT_META_ANIMATION_CACHE_LIMIT_BYTES,
            meta_image_tx,
            meta_image_rx,
            preview_cache: HashMap::new(),
            preview_tx,
            preview_rx,
            preview_load_queue: SelectPreviewLoadQueue::default(),
            generated_preview_loading: false,
        }
    }

    pub(super) fn preview_playing(&self) -> bool {
        self.preview_fade != SelectPreviewFade::Silent
    }

    pub(super) fn preview_source(&self) -> Option<&str> {
        self.preview_source.as_deref()
    }

    pub(super) fn update_preview_selection(
        &mut self,
        generation: u64,
        chart_id: Option<i64>,
    ) -> bool {
        let selection = Some((generation, chart_id));
        if self.preview_selection == selection {
            return false;
        }
        self.preview_selection = selection;
        self.folder_preview_selection = self.folder_preview_selection.wrapping_add(1);
        self.preview_cache.retain(|key, _| !key.starts_with(FOLDER_PREVIEW_PREFIX));
        self.preview_played_once = false;
        true
    }

    pub(super) fn preview_fade(&self) -> SelectPreviewFade {
        self.preview_fade
    }

    pub(super) fn preview_normalization_gain(&self) -> f32 {
        self.preview_normalization_gain
    }

    pub(super) fn generated_preview_loading(&self) -> bool {
        self.generated_preview_loading
    }

    pub(super) fn output_sample_rate(&self) -> Option<u32> {
        self.preview.as_ref().map(SelectChartPreview::output_sample_rate)
    }

    pub(super) fn has_preview(&self) -> bool {
        self.preview.is_some()
    }

    pub(super) fn install_preview(&mut self, preview: SelectChartPreview) {
        self.preview = Some(preview);
    }

    pub(super) fn poll_loads(&mut self) -> Vec<SelectMetaImageUpload> {
        let mut image_uploads = Vec::new();
        while let Ok(result) = self.meta_image_rx.try_recv() {
            let is_current =
                self.meta_image_source(result.slot).as_deref() == Some(result.key.as_str());
            match result.result {
                Ok(image) => {
                    if is_current {
                        image_uploads.push(SelectMetaImageUpload {
                            slot: result.slot,
                            image: image.clone(),
                        });
                    }
                    self.insert_meta_image_cache(result.key, image);
                }
                Err(error) => {
                    if let Some(path) = result.path {
                        tracing::debug!(path = %path.display(), %error, "skipping select meta image");
                    } else {
                        tracing::debug!(%error, "skipping select meta image");
                    }
                    // A failed animation reload keeps the cached still image.
                    if let Some(SelectMetaImageCacheEntry::Ready(cached)) =
                        self.meta_image_cache.get_mut(&result.key)
                    {
                        cached.animation = MetaAnimationState::Still;
                        continue;
                    }
                    if is_current {
                        self.set_meta_image_loaded(result.slot, false);
                    }
                    self.meta_image_cache.insert(result.key, SelectMetaImageCacheEntry::Missing);
                }
            }
        }

        while let Ok(result) = self.preview_rx.try_recv() {
            if parse_generated_preview_cache_key(&result.key).is_some() {
                self.generated_preview_loading = false;
            }
            match result.result {
                Ok(prepared) => {
                    self.insert_preview_cache(result.key, SelectPreviewCacheEntry::Ready(prepared));
                }
                Err(error) => {
                    if let Some(path) = result.path {
                        tracing::debug!(path = %path.display(), %error, "skipping chart preview audio");
                    } else {
                        tracing::debug!(%error, "skipping chart preview audio");
                    }
                    self.insert_preview_cache(result.key, SelectPreviewCacheEntry::Missing);
                }
            }
            if let Some(next) = self.preview_load_queue.finish() {
                self.start_preview_load(next);
            }
        }
        image_uploads
    }

    pub(super) fn sync_meta_image(
        &mut self,
        slot: SelectMetaImageSlot,
        cache_key: Option<String>,
    ) -> Option<SelectMetaImage> {
        if cache_key.as_deref() == self.meta_image_source(slot).as_deref() {
            // The texture keeps playing the animation it was given; nothing to restore.
            if self.meta_image_loaded(slot) {
                return None;
            }
            return cache_key.and_then(|key| self.cached_meta_image_for_display(slot, key));
        }

        self.set_meta_image_source(slot, cache_key.clone());
        self.set_meta_image_loaded(slot, false);
        self.set_meta_image_size(slot, None);
        let key = cache_key?;
        match self.meta_image_cache.get(&key) {
            Some(SelectMetaImageCacheEntry::Ready(_)) => {
                self.cached_meta_image_for_display(slot, key)
            }
            Some(SelectMetaImageCacheEntry::Loading) | Some(SelectMetaImageCacheEntry::Missing) => {
                None
            }
            None => {
                self.spawn_meta_image_load(slot, key);
                None
            }
        }
    }

    /// The cached image to upload now. Evicted animations are decoded again in the
    /// background while the still frame is shown.
    fn cached_meta_image_for_display(
        &mut self,
        slot: SelectMetaImageSlot,
        key: String,
    ) -> Option<SelectMetaImage> {
        let Some(SelectMetaImageCacheEntry::Ready(cached)) = self.meta_image_cache.get_mut(&key)
        else {
            return None;
        };
        let image = cached.image();
        match cached.animation {
            MetaAnimationState::Evicted => {
                cached.animation = MetaAnimationState::Reloading;
                self.spawn_meta_image_decode(slot, key);
            }
            MetaAnimationState::Loaded(_) => self.touch_meta_animation(&key),
            MetaAnimationState::Still | MetaAnimationState::Reloading => {}
        }
        Some(image)
    }

    /// Uploads the cached image again after another scene overwrote the shared texture.
    pub(super) fn invalidate_meta_image_upload(&mut self, slot: SelectMetaImageSlot) {
        self.set_meta_image_loaded(slot, false);
    }

    fn insert_meta_image_cache(&mut self, key: String, image: SelectMetaImage) {
        // A key registered again replaces its old frames and their accounted size.
        self.forget_meta_animation(&key);
        let animation = match image.animation {
            Some(animation) => {
                let bytes = animation.rgba_bytes();
                self.meta_image_animations.push_back((key.clone(), bytes));
                self.meta_image_animation_bytes =
                    self.meta_image_animation_bytes.saturating_add(bytes);
                MetaAnimationState::Loaded(animation)
            }
            None => MetaAnimationState::Still,
        };
        self.meta_image_cache.insert(
            key.clone(),
            SelectMetaImageCacheEntry::Ready(CachedMetaImage { first: image.first, animation }),
        );
        self.evict_meta_animations(&key);
    }

    /// Moves a loaded animation to the most recently shown end.
    fn touch_meta_animation(&mut self, key: &str) {
        if let Some(index) = self.meta_image_animations.iter().position(|(cached, _)| cached == key)
            && let Some(entry) = self.meta_image_animations.remove(index)
        {
            self.meta_image_animations.push_back(entry);
        }
    }

    fn forget_meta_animation(&mut self, key: &str) {
        if let Some(index) = self.meta_image_animations.iter().position(|(cached, _)| cached == key)
            && let Some((_, bytes)) = self.meta_image_animations.remove(index)
        {
            self.meta_image_animation_bytes = self.meta_image_animation_bytes.saturating_sub(bytes);
        }
    }

    /// Drops the least recently shown animations beyond the budget, except `keep`.
    fn evict_meta_animations(&mut self, keep: &str) {
        while self.meta_image_animation_bytes > self.meta_image_animation_limit_bytes {
            let Some(index) = self.meta_image_animations.iter().position(|(key, _)| key != keep)
            else {
                break;
            };
            let Some((key, bytes)) = self.meta_image_animations.remove(index) else {
                break;
            };
            self.meta_image_animation_bytes = self.meta_image_animation_bytes.saturating_sub(bytes);
            if let Some(SelectMetaImageCacheEntry::Ready(cached)) =
                self.meta_image_cache.get_mut(&key)
            {
                cached.animation = MetaAnimationState::Evicted;
            }
        }
    }

    pub(super) fn finish_meta_image_upload(
        &mut self,
        slot: SelectMetaImageSlot,
        size: Option<SkinImageSize>,
    ) {
        self.set_meta_image_loaded(slot, size.is_some());
        self.set_meta_image_size(slot, size);
    }

    pub(super) fn sync_preview(
        &mut self,
        cache_key: Option<String>,
        restart: bool,
        now: Instant,
    ) -> Option<PreparedSelectPreview> {
        if self.preview.is_none() || self.preview_played_once {
            return None;
        }
        if restart || cache_key != self.preview_source {
            if cache_key != self.preview_load_queue.pending {
                self.discard_pending_preview_load();
            }
            self.preview_source.clone_from(&cache_key);
            if matches!(
                self.preview_fade,
                SelectPreviewFade::FadingIn { .. } | SelectPreviewFade::Playing
            ) {
                self.preview_fade = SelectPreviewFade::FadingOut { started_at: now };
            }
        }
        let key = cache_key?;
        match self.preview_cache.get(&key) {
            Some(SelectPreviewCacheEntry::Ready(prepared)) if !self.preview_playing() => {
                Some(prepared.clone())
            }
            Some(_) => None,
            None => {
                self.request_preview_load(key);
                None
            }
        }
    }

    pub(super) fn play_preview(
        &mut self,
        prepared: PreparedSelectPreview,
        volume: f32,
        loop_playback: bool,
        now: Instant,
    ) -> bool {
        let normalization_gain = prepared.normalization_gain;
        let loaded = self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.play_sample(prepared.sample, volume, loop_playback));
        if loaded {
            self.preview_played_once = !loop_playback;
            self.preview_normalization_gain = normalization_gain;
            self.preview_fade = SelectPreviewFade::FadingIn { started_at: now };
        }
        loaded
    }

    pub(super) fn advance_preview_fade(&mut self, now: Instant) {
        if self.preview.as_ref().is_some_and(SelectChartPreview::is_finished) {
            self.preview_fade = SelectPreviewFade::Silent;
        }
        match self.preview_fade {
            SelectPreviewFade::FadingIn { started_at }
                if now.duration_since(started_at) >= SELECT_PREVIEW_FADE_DURATION =>
            {
                self.preview_fade = SelectPreviewFade::Playing;
            }
            SelectPreviewFade::FadingOut { started_at }
                if now.duration_since(started_at) >= SELECT_PREVIEW_FADE_DURATION =>
            {
                self.stop_preview_voice();
                self.preview_fade = SelectPreviewFade::Silent;
            }
            SelectPreviewFade::Silent => self.stop_preview_voice(),
            _ => {}
        }
    }

    pub(super) fn preview_fade_factor(&self, now: Instant) -> f32 {
        select_preview_fade_factor(self.preview_fade, now)
    }

    pub(super) fn set_preview_volume(&self, volume: f32) {
        if let Some(preview) = &self.preview {
            preview.set_volume(volume);
        }
    }

    pub(super) fn stop_preview(&mut self) {
        self.stop_preview_voice();
        self.discard_pending_preview_load();
        self.preview_source = None;
        self.preview_selection = None;
        self.preview_played_once = false;
        self.preview_fade = SelectPreviewFade::Silent;
    }

    fn discard_pending_preview_load(&mut self) {
        if let Some(key) = self.preview_load_queue.pending.take() {
            self.preview_cache.remove(&key);
        }
    }

    fn stop_preview_voice(&self) {
        if let Some(preview) = &self.preview {
            preview.stop();
        }
    }

    fn insert_preview_cache(&mut self, key: String, entry: SelectPreviewCacheEntry) {
        if let Some((selection, _)) = folder_preview_request(&key)
            && (self.preview_selection.is_none() || selection != self.folder_preview_selection)
        {
            return;
        }
        self.preview_cache.insert(key, entry);
        while self.preview_cache.len() > SELECT_PREVIEW_CACHE_LIMIT {
            let current = self.preview_source.as_deref();
            let removable_key = self
                .preview_cache
                .iter()
                .find(|(candidate, entry)| {
                    Some(candidate.as_str()) != current
                        && !matches!(entry, SelectPreviewCacheEntry::Loading)
                        // Keep the folder miss while its generated preview is selected.
                        && !candidate.starts_with(FOLDER_PREVIEW_PREFIX)
                })
                .map(|(candidate, _)| candidate.clone());
            let Some(removable_key) = removable_key else {
                break;
            };
            self.preview_cache.remove(&removable_key);
        }
    }

    fn request_preview_load(&mut self, key: String) {
        self.insert_preview_cache(key.clone(), SelectPreviewCacheEntry::Loading);
        let Some(key) = self.preview_load_queue.request(key) else {
            return;
        };
        self.start_preview_load(key);
    }

    fn start_preview_load(&mut self, key: String) {
        self.generated_preview_loading = false;
        let tx = self.preview_tx.clone();
        if let Some(generated) = parse_generated_preview_cache_key(&key) {
            self.generated_preview_loading = true;
            let library_db_path = self.library_db_path.clone();
            let sample_rate = self.output_sample_rate().unwrap_or(48_000);
            let result_key = key.clone();
            if let Err(error) = thread::Builder::new()
                .name(format!("select-preview-{}", generated.chart_id))
                .spawn(move || {
                    lower_current_thread_priority();
                    let result = render_generated_preview_for_chart(
                        &library_db_path,
                        generated.chart_id,
                        generated.start_ms,
                        sample_rate,
                        Some(&result_key),
                    )
                    .map(prepare_select_preview)
                    .map_err(|error| format!("{error:#}"));
                    let _ = tx.send(SelectPreviewResult { key: result_key, path: None, result });
                })
            {
                tracing::warn!(%error, "failed to spawn generated chart preview loader");
                self.generated_preview_loading = false;
                self.insert_preview_cache(key, SelectPreviewCacheEntry::Missing);
                if let Some(next) = self.preview_load_queue.finish() {
                    self.start_preview_load(next);
                }
            }
            return;
        }

        let cache_dir = self.cache_dir.clone();
        thread::spawn(move || {
            let resolved = match folder_preview_request(&key) {
                Some((_, folder)) => Ok(crate::chart_asset::find_preview_prefix_audio_file(
                    folder.as_ref(),
                    "preview.",
                )),
                None => crate::chart_asset::resolve_cached_asset(&key, &cache_dir, true),
            };
            let path = resolved.as_ref().ok().and_then(Clone::clone);
            let result = match path.as_ref() {
                Some(path) => {
                    let mut loader = FfmpegSampleLoader::default();
                    loader.load(path).map(prepare_select_preview).map_err(|error| error.to_string())
                }
                None => Err(resolved.err().map_or_else(
                    || "chart preview audio file not found".to_string(),
                    |e| format!("{e:#}"),
                )),
            };
            let _ = tx.send(SelectPreviewResult { key, path, result });
        });
    }

    fn spawn_meta_image_load(&mut self, slot: SelectMetaImageSlot, key: String) {
        self.meta_image_cache.insert(key.clone(), SelectMetaImageCacheEntry::Loading);
        self.spawn_meta_image_decode(slot, key);
    }

    /// Decodes `key` off the select thread; `poll_loads` installs the result.
    fn spawn_meta_image_decode(&self, slot: SelectMetaImageSlot, key: String) {
        let tx = self.meta_image_tx.clone();
        let cache_dir = self.cache_dir.clone();
        thread::spawn(move || {
            let resolved = crate::chart_asset::resolve_cached_asset(&key, &cache_dir, false);
            let path = resolved.as_ref().ok().and_then(Clone::clone);
            let result = match path.as_ref() {
                Some(path) => load_chart_image(
                    path,
                    &super::meta_image_animation::META_IMAGE_ANIMATION_LIMITS,
                )
                .map(|decoded| SelectMetaImage {
                    first: decoded.first_frame().clone(),
                    animation: decoded.animation().cloned(),
                })
                .map_err(|error| error.to_string()),
                None => Err(resolved.err().map_or_else(
                    || "select meta image file not found".to_string(),
                    |e| format!("{e:#}"),
                )),
            };
            let _ = tx.send(SelectMetaImageResult { slot, key, path, result });
        });
    }

    fn meta_image(&self, slot: SelectMetaImageSlot) -> &SelectMetaImageState {
        match slot {
            SelectMetaImageSlot::Stage => &self.stage,
            SelectMetaImageSlot::Backbmp => &self.backbmp,
            SelectMetaImageSlot::Banner => &self.banner,
        }
    }

    fn meta_image_mut(&mut self, slot: SelectMetaImageSlot) -> &mut SelectMetaImageState {
        match slot {
            SelectMetaImageSlot::Stage => &mut self.stage,
            SelectMetaImageSlot::Backbmp => &mut self.backbmp,
            SelectMetaImageSlot::Banner => &mut self.banner,
        }
    }

    pub(super) fn meta_image_source(&self, slot: SelectMetaImageSlot) -> &Option<String> {
        &self.meta_image(slot).source
    }

    pub(super) fn set_meta_image_source(
        &mut self,
        slot: SelectMetaImageSlot,
        source: Option<String>,
    ) {
        self.meta_image_mut(slot).source = source;
    }

    pub(super) fn meta_image_loaded(&self, slot: SelectMetaImageSlot) -> bool {
        self.meta_image(slot).loaded
    }

    pub(super) fn set_meta_image_loaded(&mut self, slot: SelectMetaImageSlot, loaded: bool) {
        self.meta_image_mut(slot).loaded = loaded;
    }

    pub(super) fn meta_image_size(&self, slot: SelectMetaImageSlot) -> Option<SkinImageSize> {
        self.meta_image(slot).size
    }

    pub(super) fn set_meta_image_size(
        &mut self,
        slot: SelectMetaImageSlot,
        size: Option<SkinImageSize>,
    ) {
        self.meta_image_mut(slot).size = size;
    }
}

pub(super) fn prepare_select_preview(sample: DecodedSample) -> PreparedSelectPreview {
    let normalization_gain = analyze_preview_loudness(&sample)
        .map(|analysis| {
            tracing::debug!(
                loudness_lufs = analysis.loudness_lufs,
                short_term_lufs = analysis.short_term_lufs,
                sample_peak = analysis.peak_abs,
                normalization_gain = analysis.normalization_gain,
                "analyzed select preview loudness"
            );
            analysis.normalization_gain
        })
        .unwrap_or(1.0);
    PreparedSelectPreview { sample, normalization_gain }
}

fn fade_progress(started_at: Instant, now: Instant, duration: Duration) -> f32 {
    if duration == Duration::ZERO {
        return 1.0;
    }
    now.saturating_duration_since(started_at).as_secs_f32() / duration.as_secs_f32()
}

pub(super) fn select_preview_fade_factor(fade: SelectPreviewFade, now: Instant) -> f32 {
    match fade {
        SelectPreviewFade::Silent => 0.0,
        SelectPreviewFade::Playing => 1.0,
        SelectPreviewFade::FadingIn { started_at } => {
            fade_progress(started_at, now, SELECT_PREVIEW_FADE_DURATION)
        }
        SelectPreviewFade::FadingOut { started_at } => {
            1.0 - fade_progress(started_at, now, SELECT_PREVIEW_FADE_DURATION)
        }
    }
    .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "CPU benchmark; run alone with --release --ignored --nocapture"]
    fn profile_select_archive_cache_keys() -> anyhow::Result<()> {
        anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("songs.zip");
        std::fs::write(&archive, b"metadata probe").unwrap();
        let folder = format!("{}!/song", archive.display());
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            runtime.update_source(Some(&folder), Instant::now());
            if runtime.asset_cache_key(&folder, "stage.png").is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        for cached in [false, true] {
            let mut timings = Vec::new();
            for iteration in 0..1100 {
                let start = Instant::now();
                if cached {
                    runtime.update_source(Some(&folder), Instant::now());
                }
                for file in ["stage.png", "back.png", "banner.png", "", "", "", ""] {
                    std::hint::black_box(if cached {
                        runtime.asset_cache_key(&folder, file).unwrap()
                    } else {
                        crate::chart_asset::asset_cache_key(&folder, file)
                    });
                }
                if iteration >= 100 {
                    timings.push(start.elapsed().as_secs_f64() * 1_000_000.0);
                }
            }
            timings.sort_by(f64::total_cmp);
            eprintln!(
                "archive_keys cached={cached} files=7 n={} median_us={:.3} p95_us={:.3} p99_us={:.3} max_us={:.3}",
                timings.len(),
                timings[500],
                timings[949],
                timings[989],
                timings[999]
            );
        }
        Ok(())
    }

    #[test]
    fn archive_metadata_refresh_updates_image_and_generated_preview_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("songs.zip");
        std::fs::write(&path, b"first").unwrap();
        let folder = format!("{}!/song", path.display());
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());
        let start = Instant::now();
        let mut wait_for = |now: Instant, expected: &str| {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                runtime.update_source(Some(&folder), now);
                if runtime.asset_cache_key(&folder, "stage.png").as_deref() == Some(expected) {
                    break;
                }
                assert!(Instant::now() < deadline, "metadata worker did not complete");
                std::thread::sleep(Duration::from_millis(1));
            }
            let file_key = crate::chart_asset::asset_cache_key(&folder, "");
            assert_eq!(
                runtime.preview_cache_key(1, &folder, "", || None).as_ref(),
                Some(&file_key)
            );
            runtime.insert_preview_cache(file_key, SelectPreviewCacheEntry::Missing);
            assert_eq!(
                runtime.preview_cache_key(1, &folder, "", || Some(0)),
                Some(crate::generated_preview::generated_preview_cache_key_for_source(
                    1, 0, &folder
                ))
            );
        };
        let before = crate::chart_asset::asset_cache_key(&folder, "stage.png");
        wait_for(start, &before);
        std::fs::write(&path, b"second with different size").unwrap();
        let after = crate::chart_asset::asset_cache_key(&folder, "stage.png");
        assert_ne!(before, after);
        wait_for(start + Duration::from_secs(2), &after);
    }

    fn animated_image(color: u8) -> SelectMetaImage {
        let frames = [color, 255 - color]
            .map(|color| {
                let frame = RgbaImageAsset { width: 1, height: 1, pixels: vec![color; 4] };
                (frame, Duration::from_millis(100))
            })
            .into();
        SelectMetaImage {
            first: RgbaImageAsset { width: 1, height: 1, pixels: vec![color; 4] },
            animation: Some(Arc::new(AnimatedImage::new(frames, None).unwrap())),
        }
    }

    fn animation_state(runtime: &SelectAssetRuntime, key: &str) -> &'static str {
        match runtime.meta_image_cache.get(key) {
            Some(SelectMetaImageCacheEntry::Ready(cached)) => match cached.animation {
                MetaAnimationState::Still => "still",
                MetaAnimationState::Loaded(_) => "loaded",
                MetaAnimationState::Evicted => "evicted",
                MetaAnimationState::Reloading => "reloading",
            },
            Some(SelectMetaImageCacheEntry::Loading) => "loading",
            Some(SelectMetaImageCacheEntry::Missing) => "missing",
            None => "absent",
        }
    }

    /// Copies a GIF of `count` `size` x `size` frames and returns its cache key.
    fn write_gif(dir: &std::path::Path, name: &str, size: u32, count: usize) -> String {
        let path = dir.join(name);
        let mut encoder =
            image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
        for index in 0..count {
            let color = if index % 2 == 0 { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(size, size, image::Rgba(color)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
        crate::chart_asset::asset_cache_key(&dir.to_string_lossy(), name)
    }

    /// Polls worker results until `done` holds or 10 s pass, collecting uploads.
    fn poll_until(
        runtime: &mut SelectAssetRuntime,
        mut done: impl FnMut(&SelectAssetRuntime, &[SelectMetaImageUpload]) -> bool,
    ) -> Vec<SelectMetaImageUpload> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut uploads = Vec::new();
        while Instant::now() < deadline {
            uploads.extend(runtime.poll_loads());
            if done(runtime, &uploads) {
                return uploads;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("select asset worker did not finish");
    }

    /// Selects `key` in the STAGEFILE slot and returns the image uploaded immediately.
    fn select(runtime: &mut SelectAssetRuntime, key: &str) -> Option<SelectMetaImage> {
        let image = runtime.sync_meta_image(SelectMetaImageSlot::Stage, Some(key.to_string()));
        runtime.finish_meta_image_upload(
            SelectMetaImageSlot::Stage,
            image.as_ref().map(|_| SkinImageSize { width: 1.0, height: 1.0 }),
        );
        image
    }

    #[test]
    fn meta_image_cache_evicts_least_recently_shown_animations_beyond_the_budget() {
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());
        // Each animation holds two 1x1 RGBA frames (8 bytes); the budget keeps two.
        runtime.meta_image_animation_limit_bytes = 16;
        runtime.insert_meta_image_cache("a".to_string(), animated_image(10));
        runtime.insert_meta_image_cache("b".to_string(), animated_image(20));
        // Showing "a" again makes "b" the least recently shown.
        assert!(select(&mut runtime, "a").unwrap().animation.is_some());
        runtime.insert_meta_image_cache("c".to_string(), animated_image(30));

        assert_eq!(animation_state(&runtime, "a"), "loaded");
        assert_eq!(animation_state(&runtime, "b"), "evicted");
        assert_eq!(animation_state(&runtime, "c"), "loaded");
        assert_eq!(runtime.meta_image_animation_bytes, 16);
    }

    #[test]
    fn registering_a_key_again_replaces_its_accounted_size() {
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());
        for _ in 0..3 {
            runtime.insert_meta_image_cache("a".to_string(), animated_image(10));
        }
        assert_eq!(runtime.meta_image_animations.len(), 1);
        assert_eq!(runtime.meta_image_animation_bytes, 8);

        let still = SelectMetaImage { animation: None, ..animated_image(10) };
        runtime.insert_meta_image_cache("a".to_string(), still);
        assert!(runtime.meta_image_animations.is_empty());
        assert_eq!(runtime.meta_image_animation_bytes, 0);
        assert_eq!(animation_state(&runtime, "a"), "still");
    }

    #[test]
    fn still_images_never_decode_their_animation_again() {
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());
        let still = SelectMetaImage { animation: None, ..animated_image(10) };
        runtime.insert_meta_image_cache("still".to_string(), still);

        for key in ["still", "other", "still"] {
            runtime.sync_meta_image(SelectMetaImageSlot::Stage, Some(key.to_string()));
        }

        assert_eq!(animation_state(&runtime, "still"), "still");
    }

    #[test]
    fn evicted_animation_is_decoded_again_when_shown() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_gif(dir.path(), "a.gif", 2, 2);
        let b = write_gif(dir.path(), "b.gif", 3, 2);
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), dir.path().into());
        runtime.meta_image_animation_limit_bytes = 2 * 2 * 4 * 2;

        assert!(select(&mut runtime, &a).is_none());
        let uploads = poll_until(&mut runtime, |_, uploads| !uploads.is_empty());
        assert!(uploads[0].image.animation.is_some());
        select(&mut runtime, &b);
        poll_until(&mut runtime, |runtime, _| animation_state(runtime, &b) == "loaded");
        assert_eq!(animation_state(&runtime, &a), "evicted");

        // Showing "a" again uploads its still frame now and restores the frames later.
        let still = select(&mut runtime, &a).unwrap();
        assert!(still.animation.is_none());
        assert_eq!(animation_state(&runtime, &a), "reloading");
        // Selecting it again while reloading does not start a second decode.
        select(&mut runtime, &b);
        assert_eq!(animation_state(&runtime, &b), "loaded");
        select(&mut runtime, &a);
        let uploads =
            poll_until(&mut runtime, |runtime, _| animation_state(runtime, &a) == "loaded");
        let restored = uploads.iter().find(|upload| upload.image.first.width == 2).unwrap();
        assert_eq!(restored.image.animation.as_ref().unwrap().frames().len(), 2);
        assert_eq!(animation_state(&runtime, &b), "evicted");
    }

    #[test]
    fn reload_finishing_after_selection_moved_does_not_replace_the_current_image() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_gif(dir.path(), "a.gif", 2, 2);
        let b = write_gif(dir.path(), "b.gif", 3, 2);
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), dir.path().into());
        runtime.meta_image_animation_limit_bytes = 2 * 2 * 4 * 2;
        select(&mut runtime, &a);
        poll_until(&mut runtime, |runtime, _| animation_state(runtime, &a) == "loaded");
        select(&mut runtime, &b);
        poll_until(&mut runtime, |runtime, _| animation_state(runtime, &b) == "loaded");

        select(&mut runtime, &a);
        assert_eq!(animation_state(&runtime, &a), "reloading");
        select(&mut runtime, &b);
        let uploads =
            poll_until(&mut runtime, |runtime, _| animation_state(runtime, &a) == "loaded");

        assert!(uploads.iter().all(|upload| upload.image.first.width != 2), "stale upload");
        assert_eq!(runtime.meta_image_source(SelectMetaImageSlot::Stage).as_deref(), Some(&*b));
    }

    #[test]
    fn runtime_initializes_preview_and_worker_state() {
        let runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());

        assert!(runtime.preview.is_none());
        assert_eq!(runtime.preview_source, None);
        assert!(!runtime.preview_playing());
        assert_eq!(runtime.preview_fade, SelectPreviewFade::Silent);
        assert_eq!(runtime.preview_normalization_gain, 1.0);
        assert!(runtime.meta_image_cache.is_empty());
        assert!(runtime.preview_cache.is_empty());
        assert!(!runtime.generated_preview_loading);
    }

    #[test]
    fn blank_folder_preview_is_checked_once_and_picks_up_audio_on_reselection() {
        use bmz_audio::{command::AudioEngineHandle, engine::AudioEngine};

        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_str().unwrap();
        let wav = include_bytes!("../../../../data/songs/sample-playable/key.wav");
        std::fs::write(dir.path().join("preview-other.wav"), wav).unwrap();
        let preview = SelectChartPreview::new(AudioEngineHandle::new(AudioEngine::new(48_000)));
        let mut runtime = SelectAssetRuntime::new(Some(preview), PathBuf::new(), dir.path().into());
        runtime.update_preview_selection(1, Some(10));
        let preview_key = |runtime: &SelectAssetRuntime| {
            runtime.preview_cache_key(10, folder, "", || Some(0)).unwrap()
        };
        let generated = crate::generated_preview::generated_preview_cache_key(10, 0);
        let missing = preview_key(&runtime);
        assert_ne!(missing, generated);
        assert!(runtime.sync_preview(Some(missing), false, Instant::now()).is_none());
        assert_eq!(
            runtime.preview_cache_key(10, folder, "", || panic!("lookup pending")),
            runtime.preview_source.clone()
        );
        poll_until(&mut runtime, |runtime, _| preview_key(runtime) == generated);

        std::fs::write(dir.path().join("preview.wav"), wav).unwrap();
        runtime.preview_source = Some(generated.clone());
        for other in 0..SELECT_PREVIEW_CACHE_LIMIT {
            runtime
                .insert_preview_cache(format!("other-{other}"), SelectPreviewCacheEntry::Missing);
        }
        assert!(!runtime.update_preview_selection(1, Some(10)));
        assert_eq!(preview_key(&runtime), generated);

        runtime.update_preview_selection(2, None);
        runtime.update_preview_selection(3, Some(10));
        let found = preview_key(&runtime);
        assert_ne!(found, generated);
        assert!(runtime.sync_preview(Some(found.clone()), false, Instant::now()).is_none());
        poll_until(&mut runtime, |runtime, _| !runtime.preview_load_queue.active);
        let prepared =
            runtime.sync_preview(Some(found), false, Instant::now()).expect("preview.wav");
        assert!(!prepared.sample.frames.is_empty());
        assert_eq!(
            runtime.preview_cache_key(10, folder, "named.wav", || None),
            runtime.asset_cache_key(folder, "named.wav")
        );
    }

    #[test]
    fn stale_folder_preview_miss_does_not_skip_the_current_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_str().unwrap();
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), dir.path().into());
        runtime.update_preview_selection(1, Some(10));
        let old = runtime.preview_cache_key(10, folder, "", || None).unwrap();
        runtime.request_preview_load(old.clone());
        runtime.stop_preview();
        runtime.update_preview_selection(1, Some(10));
        let current = runtime.preview_cache_key(10, folder, "", || None).unwrap();
        poll_until(&mut runtime, |runtime, _| !runtime.preview_load_queue.active);
        assert!(!runtime.preview_cache.contains_key(&old));
        assert_eq!(runtime.preview_cache_key(10, folder, "", || None), Some(current));
    }

    #[test]
    fn once_preview_restores_bgm_at_end_and_rearms_on_selection_change() {
        use bmz_audio::command::AudioEngineHandle;
        use bmz_audio::engine::AudioEngine;
        use bmz_core::ids::SoundId;

        let mut engine = AudioEngine::new(1_000);
        let bgm_id = SoundId(1);
        let sample = DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![0.25; 256] };
        engine.insert_sample(bgm_id, sample.clone());
        let handle = AudioEngineHandle::new(engine);
        let mut callback = handle.processor();
        assert!(handle.play_now(bgm_id, 0.5, true));
        let mut runtime = SelectAssetRuntime::new(
            Some(SelectChartPreview::new(handle.clone())),
            PathBuf::new(),
            PathBuf::new(),
        );
        let key = Some("shared-preview".to_owned());
        runtime.insert_preview_cache(
            key.clone().unwrap(),
            SelectPreviewCacheEntry::Ready(PreparedSelectPreview {
                sample,
                normalization_gain: 1.0,
            }),
        );
        let now = Instant::now();
        assert!(runtime.update_preview_selection(1, Some(20)));
        let prepared = runtime.sync_preview(key.clone(), true, now).expect("first preview");
        assert!(runtime.play_preview(prepared, 1.0, false, now));
        let faded_in_at = now + SELECT_PREVIEW_FADE_DURATION;
        runtime.advance_preview_fade(faded_in_at);
        assert_eq!(runtime.preview_fade_factor(faded_in_at), 1.0);

        let mut first = [0.0; 128 * 2];
        assert!(callback.render_stereo(0, &mut first));
        runtime.advance_preview_fade(faded_in_at);
        assert!(runtime.preview_playing());
        assert_eq!(first[20], 0.375);

        assert!(callback.render_stereo(128, &mut first));
        runtime.advance_preview_fade(faded_in_at);
        assert!(!handle.is_idle(), "the independent select BGM still plays");
        assert_eq!(runtime.preview_fade_factor(faded_in_at), 0.0);
        assert!(!runtime.update_preview_selection(1, Some(20)));
        assert!(runtime.sync_preview(key.clone(), false, faded_in_at).is_none());

        let mut tail = [0.0; 2];
        assert!(callback.render_stereo(256, &mut tail));
        assert_eq!(tail, [0.125; 2]);
        assert!(runtime.update_preview_selection(3, Some(20)));
        assert!(runtime.sync_preview(key, true, faded_in_at).is_some());
    }

    #[test]
    fn stopping_preview_drops_queued_load_and_caches_in_flight_result() {
        use bmz_audio::command::AudioEngineHandle;
        use bmz_audio::engine::AudioEngine;

        let handle = AudioEngineHandle::new(AudioEngine::new(1_000));
        let mut runtime = SelectAssetRuntime::new(
            Some(SelectChartPreview::new(handle.clone())),
            PathBuf::new(),
            PathBuf::new(),
        );
        let active_key = "active".to_owned();
        let pending_key = "pending".to_owned();
        assert_eq!(
            runtime.preview_load_queue.request(active_key.clone()),
            Some(active_key.clone())
        );
        runtime.request_preview_load(pending_key.clone());
        runtime.stop_preview();
        assert!(!runtime.preview_cache.contains_key(&pending_key));
        assert!(runtime.preview_load_queue.pending.is_none());

        runtime
            .preview_tx
            .send(SelectPreviewResult {
                key: active_key.clone(),
                path: None,
                result: Ok(PreparedSelectPreview {
                    sample: DecodedSample {
                        channels: 1,
                        sample_rate: 1_000,
                        frames: vec![0.25; 256],
                    },
                    normalization_gain: 1.0,
                }),
            })
            .unwrap();
        let submitted = handle.diagnostics().submitted;
        assert!(runtime.poll_loads().is_empty());
        assert_eq!(handle.diagnostics().submitted, submitted);
        assert!(!runtime.preview_playing());
        assert!(!runtime.preview_load_queue.active);
        assert!(runtime.sync_preview(Some(active_key), true, Instant::now()).is_some());
    }

    #[test]
    fn meta_image_slots_keep_independent_state() {
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new(), PathBuf::new());
        let stage_size = SkinImageSize { width: 640.0, height: 480.0 };

        runtime.set_meta_image_source(SelectMetaImageSlot::Stage, Some("stage".to_string()));
        runtime.set_meta_image_loaded(SelectMetaImageSlot::Stage, true);
        runtime.set_meta_image_size(SelectMetaImageSlot::Stage, Some(stage_size));

        assert_eq!(runtime.meta_image_source(SelectMetaImageSlot::Stage).as_deref(), Some("stage"));
        assert!(runtime.meta_image_loaded(SelectMetaImageSlot::Stage));
        assert_eq!(runtime.meta_image_size(SelectMetaImageSlot::Stage), Some(stage_size));
        assert_eq!(runtime.meta_image_source(SelectMetaImageSlot::Banner), &None);
        assert!(!runtime.meta_image_loaded(SelectMetaImageSlot::Banner));
        assert_eq!(runtime.meta_image_size(SelectMetaImageSlot::Banner), None);
    }
}
