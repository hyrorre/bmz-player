use super::*;

impl WinitApp {
    pub(super) fn invalidate_chart_bga_texture_preload(&mut self) {
        self.play.bga_preload.invalidate();
    }

    pub(super) fn start_chart_bga_texture_load_for_chart(
        &mut self,
        chart_id: i64,
        chart: &PlayableChart,
    ) -> BgaFrameCatalog {
        let generation = self.play.bga_preload.begin_chart(chart_id, chart.bga_assets.clone());
        let static_asset_count = chart
            .bga_assets
            .iter()
            .filter(|asset| asset.kind == bmz_chart::model::BgaAssetKind::Static)
            .count();
        if static_asset_count == 0 {
            self.play.bga_preload.status = BgaImageLoadStatus::ready(generation, chart_id);
            return BgaFrameCatalog::new();
        }
        let Some(uploader) = self.renderer.gpu_uploader() else {
            tracing::warn!("loading BGA images synchronously because GPU uploader is unavailable");
            let frames = load_chart_bga_textures(&mut self.renderer, chart);
            self.play.bga_preload.completed_assets = self.play.bga_preload.total_assets;
            self.play.bga_preload.status = BgaImageLoadStatus::ready(generation, chart_id);
            return frames;
        };

        let assets = chart.bga_assets.clone();
        let (tx, rx) = bounded_gpu_upload_channel(MAX_PENDING_BGA_TEXTURE_UPLOADS);
        let worker = thread::Builder::new()
            .name("bga-image-load".to_string())
            .spawn(move || chart_bga_texture_load_worker(generation, assets, tx, uploader))
            .expect("failed to spawn BGA image load thread");
        self.play.bga_preload.track_worker(worker);
        self.play.bga_preload.rx = Some(rx);
        tracing::info!(chart_id, generation, "BGA image preload started");
        BgaFrameCatalog::new()
    }

    pub(super) fn poll_chart_bga_texture_load(&mut self) {
        let Some(rx) = self.play.bga_preload.rx.take() else {
            return;
        };
        let mut keep_rx = true;
        for _ in 0..MAX_BGA_TEXTURE_RESULTS_PER_REDRAW {
            match rx.try_recv() {
                Ok(PendingBgaImageResult::Loaded(image)) => {
                    if image.generation != self.play.bga_preload.generation {
                        continue;
                    }
                    self.play.bga_preload.completed_assets =
                        self.play.bga_preload.completed_assets.saturating_add(1);
                    self.renderer.insert_prepared_texture(image.texture_id, image.prepared);
                    self.play.bga_preload.frames.insert(
                        image.asset_id,
                        display_bga_frame(image.asset_id, image.width, image.height),
                    );
                    if let Some(active_play) = &mut self.play.active_play {
                        active_play.running.bga_frames.insert(
                            image.asset_id,
                            display_bga_frame(image.asset_id, image.width, image.height),
                        );
                    }
                    tracing::info!(
                        asset_id = image.asset_id.0,
                        texture_id = image.texture_id.0,
                        width = image.width,
                        height = image.height,
                        file_bytes = image.file_bytes,
                        rgba_bytes = image.rgba_bytes,
                        decode_us = image.decode_us,
                        upload_us = image.upload_us,
                        async_load = true,
                        path = %image.path.display(),
                        "loaded BGA image"
                    );
                }
                Ok(PendingBgaImageResult::Failed {
                    generation,
                    asset_id,
                    path,
                    file_bytes,
                    decode_us,
                    error,
                }) => {
                    if generation != self.play.bga_preload.generation {
                        continue;
                    }
                    self.play.bga_preload.completed_assets =
                        self.play.bga_preload.completed_assets.saturating_add(1);
                    tracing::warn!(
                        asset_id = asset_id.0,
                        file_bytes,
                        decode_us,
                        async_load = true,
                        path = %path.display(),
                        error,
                        "skipping unreadable BGA image"
                    );
                }
                Ok(PendingBgaImageResult::Finished { generation, stats }) => {
                    if generation == self.play.bga_preload.generation {
                        self.play.bga_preload.completed_assets = self.play.bga_preload.total_assets;
                        if let Some(chart_id) = self.play.bga_preload.chart_id {
                            self.play.bga_preload.status =
                                BgaImageLoadStatus::ready(generation, chart_id);
                        }
                        tracing::info!(
                            chart_bga_assets = stats.chart_bga_assets,
                            static_assets = stats.static_assets,
                            skipped_non_static = stats.skipped_non_static,
                            loaded_assets = stats.loaded_assets,
                            failed_assets = stats.failed_assets,
                            total_file_bytes = stats.total_file_bytes,
                            loaded_file_bytes = stats.loaded_file_bytes,
                            rgba_bytes = stats.rgba_bytes,
                            decode_us = stats.decode_us,
                            upload_us = stats.upload_us,
                            total_us = stats.total_us,
                            async_load = true,
                            "chart BGA image load timing"
                        );
                    }
                    keep_rx = false;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(chart_id) = self.play.bga_preload.chart_id {
                        self.play.bga_preload.status =
                            BgaImageLoadStatus::failed(self.play.bga_preload.generation, chart_id);
                    }
                    keep_rx = false;
                    break;
                }
            }
        }
        if keep_rx {
            self.play.bga_preload.rx = Some(rx);
        }
    }
}
