use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use crate::skin_loader::{
    SharedSkinDocumentCache, SharedSkinFontCache, SharedSkinGpuTextureCache,
    SharedSkinSourceAssetCache, SkinDocumentCache, SkinFontCache, SkinFontCacheKey,
    SkinGpuTextureCache, SkinKind, SkinSourceAssetCache,
};

use super::{PendingSkinResult, PendingUploadResult};

pub(super) const MAX_PENDING_SKIN_UPLOADS: usize = 1;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SkinReloadGenerations {
    select: u64,
    decide: u64,
    play: u64,
    result: u64,
}

impl SkinReloadGenerations {
    pub(super) fn current(self, kind: SkinKind) -> u64 {
        match kind {
            SkinKind::Select => self.select,
            SkinKind::Decide => self.decide,
            SkinKind::Play => self.play,
            SkinKind::Result => self.result,
        }
    }

    pub(super) fn bump(&mut self, kind: SkinKind) -> u64 {
        let generation = match kind {
            SkinKind::Select => &mut self.select,
            SkinKind::Decide => &mut self.decide,
            SkinKind::Play => &mut self.play,
            SkinKind::Result => &mut self.result,
        };
        *generation = generation.wrapping_add(1);
        *generation
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PendingSkinKinds {
    select: bool,
    decide: bool,
    play: bool,
    result: bool,
}

/// skin decode (CPU) と upload (GPU) workerをつなぐchannel、共有cache、世代状態。
///
/// Rendererへのinstallとscene固有のskin選択は `WinitApp` に残し、この型は
/// pipelineのライフサイクルとstale結果判定に必要な状態だけを所有する。
pub(super) struct SkinPipelineRuntime {
    load_errors: Mutex<HashMap<std::path::PathBuf, String>>,
    pub(super) decode_tx: mpsc::Sender<PendingSkinResult>,
    pub(super) decode_rx: Option<Receiver<PendingSkinResult>>,
    pub(super) upload_tx: mpsc::SyncSender<PendingUploadResult>,
    pub(super) upload_rx: Receiver<PendingUploadResult>,
    pub(super) upload_worker_started: bool,
    pub(super) source_asset_cache: SharedSkinSourceAssetCache,
    pub(super) document_cache: SharedSkinDocumentCache,
    pub(super) font_cache: SharedSkinFontCache,
    pub(super) installed_font_cache: HashMap<String, SkinFontCacheKey>,
    pub(super) gpu_texture_cache: SharedSkinGpuTextureCache,
    pending: PendingSkinKinds,
    pub(super) generations: SkinReloadGenerations,
}

impl SkinPipelineRuntime {
    pub(super) fn new() -> Self {
        let (decode_tx, decode_rx) = mpsc::channel();
        let (upload_tx, upload_rx) = mpsc::sync_channel(MAX_PENDING_SKIN_UPLOADS);
        Self {
            load_errors: Mutex::new(HashMap::new()),
            decode_tx,
            decode_rx: Some(decode_rx),
            upload_tx,
            upload_rx,
            upload_worker_started: false,
            source_asset_cache: Arc::new(Mutex::new(SkinSourceAssetCache::default())),
            document_cache: Arc::new(Mutex::new(SkinDocumentCache::default())),
            font_cache: Arc::new(Mutex::new(SkinFontCache::default())),
            installed_font_cache: HashMap::new(),
            gpu_texture_cache: Arc::new(Mutex::new(SkinGpuTextureCache::default())),
            pending: PendingSkinKinds::default(),
            generations: SkinReloadGenerations::default(),
        }
    }

    pub(super) fn is_pending(&self, kind: SkinKind) -> bool {
        match kind {
            SkinKind::Select => self.pending.select,
            SkinKind::Decide => self.pending.decide,
            SkinKind::Play => self.pending.play,
            SkinKind::Result => self.pending.result,
        }
    }

    pub(super) fn record_load_result(&self, path: &std::path::Path, error: Option<String>) {
        if let Ok(mut errors) = self.load_errors.lock() {
            if let Some(error) = error {
                errors.insert(path.to_path_buf(), error);
            } else {
                errors.remove(path);
            }
        }
    }

    pub(super) fn load_error(&self, path: &std::path::Path) -> Option<String> {
        self.load_errors.lock().ok()?.get(path).cloned()
    }

    pub(super) fn set_pending(&mut self, kind: SkinKind, pending: bool) {
        match kind {
            SkinKind::Select => self.pending.select = pending,
            SkinKind::Decide => self.pending.decide = pending,
            SkinKind::Play => self.pending.play = pending,
            SkinKind::Result => self.pending.result = pending,
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.select || self.pending.decide || self.pending.play || self.pending.result
    }

    pub(super) fn generation(&self, kind: SkinKind) -> u64 {
        self.generations.current(kind)
    }

    pub(super) fn bump_generation(&mut self, kind: SkinKind) -> u64 {
        self.generations.bump(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_failure_is_retained_until_that_path_loads_successfully() {
        let runtime = SkinPipelineRuntime::new();
        let path = std::path::Path::new("custom/play7.json");
        runtime.record_load_result(path, Some("decode failed".into()));
        runtime.record_load_result(std::path::Path::new("default/play7.json"), None);
        assert_eq!(runtime.load_error(path).as_deref(), Some("decode failed"));
        runtime.record_load_result(path, None);
        assert_eq!(runtime.load_error(path), None);
    }

    #[test]
    fn pending_kinds_and_generations_are_isolated() {
        let mut runtime = SkinPipelineRuntime::new();

        runtime.set_pending(SkinKind::Select, true);
        runtime.set_pending(SkinKind::Play, true);
        assert!(runtime.is_pending(SkinKind::Select));
        assert!(!runtime.is_pending(SkinKind::Decide));
        assert!(runtime.is_pending(SkinKind::Play));
        assert!(runtime.has_pending());

        assert_eq!(runtime.bump_generation(SkinKind::Play), 1);
        assert_eq!(runtime.bump_generation(SkinKind::Play), 2);
        assert_eq!(runtime.generation(SkinKind::Play), 2);
        assert_eq!(runtime.generation(SkinKind::Result), 0);

        runtime.set_pending(SkinKind::Select, false);
        runtime.set_pending(SkinKind::Play, false);
        assert!(!runtime.has_pending());
    }
}
