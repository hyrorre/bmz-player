use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_audio::loudness::analyze_preview_loudness;
use bmz_audio::sample::DecodedSample;
use bmz_render::assets::{RgbaImageAsset, load_static_rgba_image};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectMetaImageSlot {
    Stage,
    Backbmp,
    Banner,
}

enum SelectMetaImageCacheEntry {
    Loading,
    Ready(RgbaImageAsset),
    Missing,
}

struct SelectMetaImageResult {
    slot: SelectMetaImageSlot,
    key: String,
    path: Option<PathBuf>,
    result: std::result::Result<RgbaImageAsset, String>,
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
    pub(super) image: RgbaImageAsset,
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
    meta_image_cache: HashMap<String, SelectMetaImageCacheEntry>,
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
        *self = Self::new(self.preview.take(), self.library_db_path.clone());
    }

    pub(super) fn new(preview: Option<SelectChartPreview>, library_db_path: PathBuf) -> Self {
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
            meta_image_cache: HashMap::new(),
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
                    self.meta_image_cache
                        .insert(result.key, SelectMetaImageCacheEntry::Ready(image));
                }
                Err(error) => {
                    if let Some(path) = result.path {
                        tracing::debug!(path = %path.display(), %error, "skipping select meta image");
                    } else {
                        tracing::debug!(%error, "skipping select meta image");
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
    ) -> Option<RgbaImageAsset> {
        if cache_key.as_deref() == self.meta_image_source(slot).as_deref() {
            if self.meta_image_loaded(slot) {
                return None;
            }
            return cache_key.as_deref().and_then(|key| match self.meta_image_cache.get(key) {
                Some(SelectMetaImageCacheEntry::Ready(image)) => Some(image.clone()),
                _ => None,
            });
        }

        self.set_meta_image_source(slot, cache_key.clone());
        self.set_meta_image_loaded(slot, false);
        self.set_meta_image_size(slot, None);
        let key = cache_key?;
        match self.meta_image_cache.get(&key) {
            Some(SelectMetaImageCacheEntry::Ready(image)) => Some(image.clone()),
            Some(SelectMetaImageCacheEntry::Loading) | Some(SelectMetaImageCacheEntry::Missing) => {
                None
            }
            None => {
                self.spawn_meta_image_load(slot, key);
                None
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

        thread::spawn(move || {
            let (folder, file) = key.split_once('|').unwrap_or(("", ""));
            let path = crate::chart_asset::resolve_preview_file(Path::new(folder), file);
            let result = match path.as_ref() {
                Some(path) => {
                    let mut loader = FfmpegSampleLoader::default();
                    loader.load(path).map(prepare_select_preview).map_err(|error| error.to_string())
                }
                None => Err("chart preview audio file not found".to_string()),
            };
            let _ = tx.send(SelectPreviewResult { key, path, result });
        });
    }

    fn spawn_meta_image_load(&mut self, slot: SelectMetaImageSlot, key: String) {
        self.meta_image_cache.insert(key.clone(), SelectMetaImageCacheEntry::Loading);
        let tx = self.meta_image_tx.clone();
        thread::spawn(move || {
            let (folder, file) = key.split_once('|').unwrap_or(("", ""));
            let path = crate::chart_asset::resolve_chart_asset_path(folder, file);
            let result = match path.as_ref() {
                Some(path) => load_static_rgba_image(path).map_err(|error| error.to_string()),
                None => Err("select meta image file not found".to_string()),
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

    #[test]
    fn runtime_initializes_preview_and_worker_state() {
        let runtime = SelectAssetRuntime::new(None, PathBuf::new());

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
        let mut runtime = SelectAssetRuntime::new(None, PathBuf::new());
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
