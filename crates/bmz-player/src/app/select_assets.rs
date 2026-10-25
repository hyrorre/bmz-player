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
use crate::generated_preview::{
    parse_generated_preview_cache_key, render_generated_preview_for_chart,
};

#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
};

pub(super) const SELECT_PREVIEW_FADE_DURATION: Duration = Duration::from_millis(150);
const SELECT_PREVIEW_CACHE_LIMIT: usize = 8;
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

pub(super) enum SelectPreviewSyncAction {
    None,
    Play(PreparedSelectPreview),
    ApplyMix,
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
    stage: SelectMetaImageState,
    backbmp: SelectMetaImageState,
    banner: SelectMetaImageState,
    preview_source: Option<String>,
    preview_playing: bool,
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
    pub(super) fn invalidate_library(&mut self) {
        self.stop_preview();
        // Drop old worker receivers too, so stale results cannot refill the caches.
        *self =
            Self::new(self.preview.take(), self.library_db_path.clone(), self.cache_dir.clone());
    }

    pub(super) fn new(
        preview: Option<SelectChartPreview>,
        library_db_path: PathBuf,
        cache_dir: PathBuf,
    ) -> Self {
        let (meta_image_tx, meta_image_rx) = mpsc::channel();
        let (preview_tx, preview_rx) = mpsc::channel();
        Self {
            stage: SelectMetaImageState::default(),
            backbmp: SelectMetaImageState::default(),
            banner: SelectMetaImageState::default(),
            preview_source: None,
            preview_playing: false,
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
        self.preview_playing
    }

    pub(super) fn preview_source(&self) -> Option<&str> {
        self.preview_source.as_deref()
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

    pub(super) fn explicit_preview_missing(&self, key: &str) -> bool {
        matches!(self.preview_cache.get(key), Some(SelectPreviewCacheEntry::Missing))
    }

    pub(super) fn poll_loads(
        &mut self,
    ) -> (Vec<SelectMetaImageUpload>, Vec<PreparedSelectPreview>) {
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

        let mut previews = Vec::new();
        while let Ok(result) = self.preview_rx.try_recv() {
            if parse_generated_preview_cache_key(&result.key).is_some() {
                self.generated_preview_loading = false;
            }
            let is_current = self.preview_source.as_deref() == Some(result.key.as_str());
            match result.result {
                Ok(prepared) => {
                    if is_current {
                        previews.push(prepared.clone());
                    }
                    self.insert_preview_cache(result.key, SelectPreviewCacheEntry::Ready(prepared));
                }
                Err(error) => {
                    if let Some(path) = result.path {
                        tracing::debug!(path = %path.display(), %error, "skipping chart preview audio");
                    } else {
                        tracing::debug!(%error, "skipping chart preview audio");
                    }
                    if is_current {
                        self.preview_playing = false;
                    }
                    self.insert_preview_cache(result.key, SelectPreviewCacheEntry::Missing);
                }
            }
            if let Some(next) = self.preview_load_queue.finish() {
                self.start_preview_load(next);
            }
        }
        (image_uploads, previews)
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
        now: Instant,
    ) -> SelectPreviewSyncAction {
        if cache_key.as_deref() == self.preview_source.as_deref() {
            if !self.preview_playing
                && let Some(key) = cache_key.as_deref()
                && let Some(SelectPreviewCacheEntry::Ready(prepared)) = self.preview_cache.get(key)
            {
                return SelectPreviewSyncAction::Play(prepared.clone());
            }
            return SelectPreviewSyncAction::None;
        }

        let had_preview = self.preview_playing;
        self.preview_source = cache_key.clone();
        let action = match cache_key.as_deref() {
            Some(_) if self.preview.is_none() => SelectPreviewSyncAction::None,
            Some(key) => match self.preview_cache.get(key) {
                Some(SelectPreviewCacheEntry::Ready(_)) if had_preview => {
                    SelectPreviewSyncAction::ApplyMix
                }
                Some(SelectPreviewCacheEntry::Ready(prepared)) => {
                    SelectPreviewSyncAction::Play(prepared.clone())
                }
                Some(SelectPreviewCacheEntry::Loading) | Some(SelectPreviewCacheEntry::Missing) => {
                    SelectPreviewSyncAction::None
                }
                None => {
                    self.request_preview_load(key.to_string());
                    SelectPreviewSyncAction::None
                }
            },
            None => SelectPreviewSyncAction::None,
        };

        if had_preview {
            self.preview_fade = SelectPreviewFade::FadingOut { started_at: now };
            self.preview_playing = true;
            SelectPreviewSyncAction::ApplyMix
        } else {
            if !matches!(action, SelectPreviewSyncAction::Play(_)) {
                self.stop_preview_voice();
            }
            self.preview_playing = false;
            action
        }
    }

    pub(super) fn play_preview(
        &mut self,
        prepared: PreparedSelectPreview,
        volume: f32,
        now: Instant,
    ) -> bool {
        let normalization_gain = prepared.normalization_gain;
        let loaded = self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.play_sample(prepared.sample, volume));
        self.preview_playing = loaded;
        if loaded {
            self.preview_normalization_gain = normalization_gain;
            self.preview_fade = SelectPreviewFade::FadingIn { started_at: now };
        }
        loaded
    }

    pub(super) fn advance_preview_fade(&mut self, now: Instant) {
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
                self.preview_playing = false;
                self.preview_fade = SelectPreviewFade::Silent;
            }
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
        self.preview_source = None;
        self.preview_playing = false;
        self.preview_fade = SelectPreviewFade::Silent;
    }

    fn stop_preview_voice(&self) {
        if let Some(preview) = &self.preview {
            preview.stop();
        }
    }

    fn insert_preview_cache(&mut self, key: String, entry: SelectPreviewCacheEntry) {
        self.preview_cache.insert(key, entry);
        while self.preview_cache.len() > SELECT_PREVIEW_CACHE_LIMIT {
            let current = self.preview_source.as_deref();
            let removable_key = self
                .preview_cache
                .iter()
                .find(|(candidate, entry)| {
                    Some(candidate.as_str()) != current
                        && !matches!(entry, SelectPreviewCacheEntry::Loading)
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
            let resolved = crate::chart_asset::resolve_cached_asset(&key, &cache_dir, true);
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

#[cfg(windows)]
fn lower_current_thread_priority() {
    // 生成中の FFmpeg decode が短い ASIO callback の実行期限を奪わないようにする。
    let updated = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
    if updated == 0 {
        tracing::debug!("failed to lower generated preview worker priority");
    }
}

#[cfg(not(windows))]
fn lower_current_thread_priority() {}

#[cfg(test)]
mod tests {
    use super::*;

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
            uploads.extend(runtime.poll_loads().0);
            if done(runtime, &uploads) {
                return uploads;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("select meta image worker did not finish");
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
        assert!(!runtime.preview_playing);
        assert_eq!(runtime.preview_fade, SelectPreviewFade::Silent);
        assert_eq!(runtime.preview_normalization_gain, 1.0);
        assert!(runtime.meta_image_cache.is_empty());
        assert!(runtime.preview_cache.is_empty());
        assert!(!runtime.generated_preview_loading);
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
