use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread::JoinHandle;
use std::time::Instant;

use bmz_chart::model::{BgaAssetId, BgaAssetKind, BgaAssetRef};
use bmz_render::assets::load_chart_bga_image;
use bmz_render::plan::TextureId;
use bmz_render::renderer::{GpuUploader, PreparedTexture};

use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id};

pub(super) const RESOURCE_LOAD_PROGRESS_SCALE: u32 = 1_000_000;

pub(super) struct PendingBgaImage {
    pub(super) generation: u64,
    pub(super) asset_id: BgaAssetId,
    pub(super) texture_id: TextureId,
    pub(super) path: PathBuf,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) file_bytes: u64,
    pub(super) rgba_bytes: u64,
    pub(super) decode_us: u128,
    pub(super) upload_us: u128,
    pub(super) prepared: PreparedTexture,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct BgaImageLoadStats {
    pub(super) chart_bga_assets: u32,
    pub(super) static_assets: u32,
    pub(super) skipped_non_static: u32,
    pub(super) loaded_assets: u32,
    pub(super) failed_assets: u32,
    pub(super) total_file_bytes: u64,
    pub(super) loaded_file_bytes: u64,
    pub(super) rgba_bytes: u64,
    pub(super) decode_us: u128,
    pub(super) upload_us: u128,
    pub(super) total_us: u128,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum BgaImageLoadStatus {
    #[default]
    Idle,
    Loading {
        generation: u64,
        chart_id: i64,
    },
    Ready {
        generation: u64,
        chart_id: i64,
    },
    Failed {
        generation: u64,
        chart_id: i64,
    },
}

impl BgaImageLoadStatus {
    pub(super) fn loading(generation: u64, chart_id: i64) -> Self {
        Self::Loading { generation, chart_id }
    }

    pub(super) fn ready(generation: u64, chart_id: i64) -> Self {
        Self::Ready { generation, chart_id }
    }

    pub(super) fn failed(generation: u64, chart_id: i64) -> Self {
        Self::Failed { generation, chart_id }
    }

    pub(super) fn is_ready_for(self, generation: u64, chart_id: i64) -> bool {
        matches!(
            self,
            Self::Ready { generation: ready_generation, chart_id: ready_chart_id }
                | Self::Failed { generation: ready_generation, chart_id: ready_chart_id }
                if ready_generation == generation && ready_chart_id == chart_id
        )
    }
}

pub(super) enum PendingBgaImageResult {
    Loaded(PendingBgaImage),
    Failed {
        generation: u64,
        asset_id: BgaAssetId,
        path: PathBuf,
        file_bytes: u64,
        decode_us: u128,
        error: String,
    },
    Finished {
        generation: u64,
        stats: BgaImageLoadStats,
    },
}

/// chart選択からプレイ開始までをまたぐ静止画BGAプリロード状態。
///
/// GPU upload結果のtexture挿入は `WinitApp` が担当し、この型は世代、対象chart、
/// manifest、進捗、frame catalogとworker receiverを所有する。
pub(super) struct BgaPreloadRuntime {
    pub(super) generation: u64,
    pub(super) chart_id: Option<i64>,
    pub(super) rx: Option<Receiver<PendingBgaImageResult>>,
    pub(super) status: BgaImageLoadStatus,
    pub(super) completed_assets: u32,
    pub(super) total_assets: u32,
    pub(super) frames: BgaFrameCatalog,
    pub(super) assets: Option<Vec<BgaAssetRef>>,
    workers: Vec<JoinHandle<()>>,
}

impl Default for BgaPreloadRuntime {
    fn default() -> Self {
        Self {
            generation: 0,
            chart_id: None,
            rx: None,
            status: BgaImageLoadStatus::Idle,
            completed_assets: 0,
            total_assets: 0,
            frames: BgaFrameCatalog::new(),
            assets: None,
            workers: Vec::new(),
        }
    }
}

impl BgaPreloadRuntime {
    pub(super) fn track_worker(&mut self, worker: JoinHandle<()>) {
        let mut index = 0;
        while index < self.workers.len() {
            if self.workers[index].is_finished() {
                if self.workers.swap_remove(index).join().is_err() {
                    tracing::warn!("BGA image load worker panicked");
                }
            } else {
                index += 1;
            }
        }
        self.workers.push(worker);
    }

    pub(super) fn shutdown_workers(&mut self) {
        // 現在の bounded send を解除してから、切替前の世代も含めて GPU 解放を待つ。
        self.rx = None;
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                tracing::warn!("BGA image load worker panicked during shutdown");
            }
        }
    }

    pub(super) fn begin_unresolved(&mut self, chart_id: i64) -> u64 {
        self.begin(chart_id, None)
    }

    pub(super) fn begin_chart(&mut self, chart_id: i64, assets: Vec<BgaAssetRef>) -> u64 {
        self.begin(chart_id, Some(assets))
    }

    fn begin(&mut self, chart_id: i64, assets: Option<Vec<BgaAssetRef>>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.chart_id = Some(chart_id);
        self.rx = None;
        self.frames.clear();
        self.total_assets = assets.as_deref().map(static_asset_count).unwrap_or_default();
        self.assets = assets;
        self.completed_assets = 0;
        self.status = BgaImageLoadStatus::loading(self.generation, chart_id);
        self.generation
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.chart_id = None;
        self.rx = None;
        self.status = BgaImageLoadStatus::Idle;
        self.completed_assets = 0;
        self.total_assets = 0;
        self.frames.clear();
        self.assets = None;
    }

    pub(super) fn apply_reused(
        &mut self,
        chart_id: i64,
        frames: BgaFrameCatalog,
        assets: Vec<BgaAssetRef>,
    ) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.chart_id = Some(chart_id);
        self.rx = None;
        self.status = BgaImageLoadStatus::ready(self.generation, chart_id);
        self.frames = frames;
        self.total_assets = static_asset_count(&assets);
        self.completed_assets = self.total_assets;
        self.assets = Some(assets);
        self.generation
    }

    pub(super) fn matches_chart(&self, chart_id: i64, active_assets: &[BgaAssetRef]) -> bool {
        self.chart_id == Some(chart_id)
            && self
                .assets
                .as_deref()
                .is_some_and(|preloaded| asset_manifests_match(preloaded, active_assets))
    }

    pub(super) fn ready_for(&self, chart_id: Option<i64>, bga_enabled: bool) -> bool {
        images_ready_for_ready_phase(self.status, self.generation, chart_id, bga_enabled)
    }

    pub(super) fn progress(&self, active_chart_id: Option<i64>) -> f32 {
        resource_load_progress(
            self.status,
            self.generation,
            self.chart_id,
            active_chart_id,
            self.completed_assets,
            self.total_assets,
        )
    }
}

fn static_asset_count(assets: &[BgaAssetRef]) -> u32 {
    assets.iter().filter(|asset| asset.kind == BgaAssetKind::Static).count().min(u32::MAX as usize)
        as u32
}

pub(super) fn images_ready_for_ready_phase(
    status: BgaImageLoadStatus,
    generation: u64,
    chart_id: Option<i64>,
    bga_enabled: bool,
) -> bool {
    if !bga_enabled {
        return true;
    }
    let Some(chart_id) = chart_id else {
        return true;
    };
    status.is_ready_for(generation, chart_id)
}

pub(super) fn resource_load_progress_units(loaded: usize, total: usize) -> u32 {
    if total == 0 {
        return RESOURCE_LOAD_PROGRESS_SCALE;
    }
    let loaded = loaded.min(total) as u64;
    ((loaded * u64::from(RESOURCE_LOAD_PROGRESS_SCALE)) / total as u64) as u32
}

pub(super) fn resource_load_progress(
    status: BgaImageLoadStatus,
    generation: u64,
    load_chart_id: Option<i64>,
    active_chart_id: Option<i64>,
    completed: u32,
    total: u32,
) -> f32 {
    if load_chart_id != active_chart_id || active_chart_id.is_none() {
        return 0.0;
    }
    match status {
        BgaImageLoadStatus::Loading { generation: load_generation, chart_id }
            if load_generation == generation && Some(chart_id) == active_chart_id =>
        {
            if total == 0 {
                0.0
            } else {
                completed.min(total) as f32 / total as f32
            }
        }
        status if status.is_ready_for(generation, active_chart_id.unwrap_or_default()) => 1.0,
        _ => 0.0,
    }
}

pub(super) fn combined_resource_load_progress(audio: f32, bga: f32, bga_enabled: bool) -> f32 {
    let audio = audio.clamp(0.0, 1.0);
    if bga_enabled { (audio + bga.clamp(0.0, 1.0)) / 2.0 } else { audio }
}

fn asset_manifests_match(preloaded: &[BgaAssetRef], active: &[BgaAssetRef]) -> bool {
    if preloaded.len() != active.len() {
        return false;
    }
    let mut preloaded = preloaded.iter().collect::<Vec<_>>();
    let mut active = active.iter().collect::<Vec<_>>();
    preloaded.sort_by_key(|asset| asset.id);
    active.sort_by_key(|asset| asset.id);
    preloaded.iter().zip(active).all(|(preloaded, active)| {
        preloaded.id == active.id && preloaded.path == active.path && preloaded.kind == active.kind
    })
}

pub(super) fn load_worker(
    generation: u64,
    assets: Vec<BgaAssetRef>,
    tx: SyncSender<PendingBgaImageResult>,
    uploader: GpuUploader,
) {
    let total_start = Instant::now();
    let mut stats = BgaImageLoadStats::default();
    for asset in assets {
        stats.chart_bga_assets += 1;
        let path = asset.path;
        let file_bytes = std::fs::metadata(&path).map(|metadata| metadata.len()).unwrap_or(0);
        stats.total_file_bytes = stats.total_file_bytes.saturating_add(file_bytes);
        if asset.kind != BgaAssetKind::Static {
            stats.skipped_non_static += 1;
            continue;
        }
        stats.static_assets += 1;

        let decode_start = Instant::now();
        match load_chart_bga_image(&path) {
            Ok(image) => {
                let image_decode_us = decode_start.elapsed().as_micros();
                stats.decode_us += image_decode_us;
                let texture_id = TextureId(bga_texture_id(asset.id));
                let image_rgba_bytes = image.pixels.len() as u64;
                let upload_start = Instant::now();
                let prepared = match uploader.upload(image.width, image.height, &image.pixels) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        stats.failed_assets += 1;
                        if tx
                            .send(PendingBgaImageResult::Failed {
                                generation,
                                asset_id: asset.id,
                                path,
                                file_bytes,
                                decode_us: image_decode_us,
                                error: error.to_string(),
                            })
                            .is_err()
                        {
                            return;
                        }
                        continue;
                    }
                };
                let image_upload_us = upload_start.elapsed().as_micros();
                stats.upload_us += image_upload_us;
                stats.loaded_assets += 1;
                stats.loaded_file_bytes = stats.loaded_file_bytes.saturating_add(file_bytes);
                stats.rgba_bytes = stats.rgba_bytes.saturating_add(image_rgba_bytes);
                let result = PendingBgaImageResult::Loaded(PendingBgaImage {
                    generation,
                    asset_id: asset.id,
                    texture_id,
                    path,
                    width: image.width,
                    height: image.height,
                    file_bytes,
                    rgba_bytes: image_rgba_bytes,
                    decode_us: image_decode_us,
                    upload_us: image_upload_us,
                    prepared,
                });
                if tx.send(result).is_err() {
                    return;
                }
            }
            Err(error) => {
                let image_decode_us = decode_start.elapsed().as_micros();
                stats.decode_us += image_decode_us;
                stats.failed_assets += 1;
                if tx
                    .send(PendingBgaImageResult::Failed {
                        generation,
                        asset_id: asset.id,
                        path,
                        file_bytes,
                        decode_us: image_decode_us,
                        error: error.to_string(),
                    })
                    .is_err()
                {
                    return;
                }
            }
        }
    }
    stats.total_us = total_start.elapsed().as_micros();
    let _ = tx.send(PendingBgaImageResult::Finished { generation, stats });
}

impl Drop for BgaPreloadRuntime {
    fn drop(&mut self) {
        self.shutdown_workers();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_disconnects_uploads_and_joins_previous_generations() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };
        use std::thread;
        use std::time::Duration;

        let mut runtime = BgaPreloadRuntime::default();
        let finished = Arc::new(AtomicUsize::new(0));
        let (release_tx, release_rx) = mpsc::channel();
        let previous_finished = Arc::clone(&finished);
        runtime.track_worker(thread::spawn(move || {
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            previous_finished.fetch_add(1, Ordering::SeqCst);
        }));
        runtime.invalidate();
        runtime.begin_chart(2, Vec::new());
        let (tx, rx) = mpsc::sync_channel(0);
        runtime.rx = Some(rx);
        let current_finished = Arc::clone(&finished);
        runtime.track_worker(thread::spawn(move || {
            assert!(
                tx.send(PendingBgaImageResult::Finished {
                    generation: 2,
                    stats: BgaImageLoadStats::default(),
                })
                .is_err()
            );
            release_tx.send(()).unwrap();
            current_finished.fetch_add(1, Ordering::SeqCst);
        }));

        drop(runtime);

        assert_eq!(finished.load(Ordering::SeqCst), 2);
    }

    fn asset(id: u32, path: &str, kind: BgaAssetKind) -> BgaAssetRef {
        BgaAssetRef { id: BgaAssetId(id), path: path.into(), kind }
    }

    #[test]
    fn manifest_matches_id_path_and_kind_regardless_of_order() {
        use BgaAssetKind::{Static, Video};

        let preloaded = vec![asset(0, "base.png", Static), asset(1, "layer.mp4", Video)];
        let active = vec![asset(1, "layer.mp4", Video), asset(0, "base.png", Static)];

        assert!(asset_manifests_match(&preloaded, &active));
        assert!(!asset_manifests_match(
            &[asset(0, "base.png", Static)],
            &[asset(1, "base.png", Static)]
        ));
        assert!(!asset_manifests_match(
            &[asset(0, "base.png", Static)],
            &[asset(0, "poor.png", Static)]
        ));
        assert!(!asset_manifests_match(
            &[asset(0, "base.png", Static)],
            &[asset(0, "base.png", Video)]
        ));
    }

    #[test]
    fn ready_gate_waits_for_current_terminal_load() {
        assert!(!images_ready_for_ready_phase(
            BgaImageLoadStatus::loading(7, 42),
            7,
            Some(42),
            true,
        ));
        for status in [BgaImageLoadStatus::ready(7, 42), BgaImageLoadStatus::failed(7, 42)] {
            assert!(images_ready_for_ready_phase(status, 7, Some(42), true));
        }
        assert!(images_ready_for_ready_phase(
            BgaImageLoadStatus::loading(7, 42),
            7,
            Some(42),
            false,
        ));
        assert!(images_ready_for_ready_phase(BgaImageLoadStatus::loading(7, 42), 7, None, true,));
        assert!(
            !images_ready_for_ready_phase(BgaImageLoadStatus::ready(6, 42), 7, Some(42), true,)
        );
        assert!(
            !images_ready_for_ready_phase(BgaImageLoadStatus::ready(7, 41), 7, Some(42), true,)
        );
    }

    #[test]
    fn resource_progress_combines_audio_and_enabled_bga() {
        assert_eq!(resource_load_progress_units(0, 4), 0);
        assert_eq!(resource_load_progress_units(1, 4), 250_000);
        assert_eq!(resource_load_progress_units(4, 4), RESOURCE_LOAD_PROGRESS_SCALE);
        assert_eq!(resource_load_progress_units(0, 0), RESOURCE_LOAD_PROGRESS_SCALE);

        assert!((combined_resource_load_progress(0.25, 0.75, true) - 0.5).abs() < f32::EPSILON);
        assert!((combined_resource_load_progress(0.25, 0.75, false) - 0.25).abs() < f32::EPSILON);

        assert_eq!(
            resource_load_progress(BgaImageLoadStatus::loading(7, 42), 7, Some(42), Some(42), 1, 4,),
            0.25
        );
        assert_eq!(
            resource_load_progress(BgaImageLoadStatus::ready(7, 42), 7, Some(42), Some(42), 0, 0,),
            1.0
        );
        assert_eq!(
            resource_load_progress(BgaImageLoadStatus::ready(6, 42), 7, Some(42), Some(42), 4, 4,),
            0.0
        );
    }

    #[test]
    fn lifecycle_resets_and_reuses_generation_scoped_state() {
        let mut runtime = BgaPreloadRuntime::default();
        let assets = vec![asset(0, "base.png", BgaAssetKind::Static)];
        assert_eq!(runtime.begin_chart(42, assets.clone()), 1);
        assert_eq!(runtime.total_assets, 1);
        assert!(runtime.matches_chart(42, &assets));

        assert_eq!(runtime.apply_reused(42, BgaFrameCatalog::new(), assets), 2);
        assert!(runtime.ready_for(Some(42), true));
        assert_eq!(runtime.progress(Some(42)), 1.0);

        runtime.invalidate();
        assert_eq!(runtime.generation, 3);
        assert_eq!(runtime.chart_id, None);
        assert_eq!(runtime.status, BgaImageLoadStatus::Idle);
        assert!(runtime.assets.is_none());
    }
}
