use super::*;

impl WinitApp {
    pub(super) fn poll_select_asset_loads(&mut self) {
        let (image_uploads, previews) = self.select.select_assets.poll_loads();
        for upload in image_uploads {
            self.upload_select_meta_image(upload.slot, &upload.image);
        }
        for prepared in previews {
            if self.play_select_preview_sample(prepared, 0.0) {
                self.apply_select_preview_audio_mix();
            }
        }
    }

    pub(super) fn selected_chart_needs_generated_preview_distribution(&self) -> bool {
        match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().is_some_and(|chart| {
                let explicit_key = format!("{}|{}", chart.folder_path, chart.preview_file);
                let explicit_missing =
                    self.select.select_assets.explicit_preview_missing(&explicit_key);
                should_use_generated_preview(&chart.preview_file, explicit_missing)
            }),
            _ => false,
        }
    }

    pub(super) fn selected_select_preview_cache_key(&self) -> Option<String> {
        match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => {
                let chart = row.chart.as_ref()?;
                let explicit_key = format!("{}|{}", chart.folder_path, chart.preview_file);
                let explicit_missing =
                    self.select.select_assets.explicit_preview_missing(&explicit_key);
                if !should_use_generated_preview(&chart.preview_file, explicit_missing) {
                    return Some(explicit_key);
                }
                let distributions = self.select.select_distribution_cache.borrow();
                let distribution = distributions.get(&chart.chart_id)?;
                let start_ms = fallback_preview_start_ms(&distribution.notes, chart.length_ms)?;
                Some(generated_preview_cache_key(chart.chart_id, start_ms))
            }
            _ => None,
        }
    }

    pub(super) fn sync_select_preview_audio(&mut self) {
        if self.selected_chart_needs_generated_preview_distribution() {
            self.ensure_visible_select_chart_distributions(25);
        }
        let selected_cache_key = self.selected_select_preview_cache_key();
        let cache_key = select_preview_key_after_delay(
            selected_cache_key,
            self.select.select_assets.preview_source(),
            self.select.select_bar_started_at.elapsed(),
            SELECT_PREVIEW_START_DELAY,
        );
        match self.select.select_assets.sync_preview(cache_key, Instant::now()) {
            SelectPreviewSyncAction::None => {}
            SelectPreviewSyncAction::Play(prepared) => {
                if self.play_select_preview_sample(prepared, 0.0) {
                    self.apply_select_preview_audio_mix();
                }
            }
            SelectPreviewSyncAction::ApplyMix => self.apply_select_preview_audio_mix(),
        }
    }

    pub(super) fn stop_select_preview(&mut self) {
        self.select.select_assets.stop_preview();
        self.set_select_bgm_volume_factor(1.0);
    }

    pub(super) fn sync_select_banner_texture(&mut self) {
        self.sync_select_meta_image_texture(SelectMetaImageSlot::Banner);
    }

    pub(super) fn sync_select_stage_texture(&mut self) {
        self.sync_select_meta_image_texture(SelectMetaImageSlot::Stage);
    }

    pub(super) fn sync_select_backbmp_texture(&mut self) {
        self.sync_select_meta_image_texture(SelectMetaImageSlot::Backbmp);
    }

    pub(super) fn sync_select_meta_image_texture(&mut self, slot: SelectMetaImageSlot) {
        let cache_key = match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().and_then(|chart| {
                let file = match slot {
                    SelectMetaImageSlot::Stage => &chart.stage_file,
                    SelectMetaImageSlot::Backbmp => &chart.backbmp_file,
                    SelectMetaImageSlot::Banner => &chart.banner_file,
                };
                (!file.is_empty()).then(|| format!("{}|{}", chart.folder_path, file))
            }),
            _ => None,
        };
        if let Some(image) = self.select.select_assets.sync_meta_image(slot, cache_key) {
            self.upload_select_meta_image(slot, &image);
        }
    }

    pub(super) fn upload_select_meta_image(
        &mut self,
        slot: SelectMetaImageSlot,
        image: &RgbaImageAsset,
    ) -> bool {
        let texture_id = match slot {
            SelectMetaImageSlot::Stage => SELECT_STAGE_TEXTURE,
            SelectMetaImageSlot::Backbmp => PLAY_BACKBMP_TEXTURE,
            SelectMetaImageSlot::Banner => SELECT_BANNER_TEXTURE,
        };
        if let Err(error) = self.renderer.upsert_image_asset(texture_id, image) {
            tracing::warn!(%error, "failed to upload select meta image");
            self.select.select_assets.finish_meta_image_upload(slot, None);
            false
        } else {
            self.select.select_assets.finish_meta_image_upload(
                slot,
                Some(SkinImageSize { width: image.width as f32, height: image.height as f32 }),
            );
            true
        }
    }

    pub(super) fn select_preview_volume(&self) -> f32 {
        self.select_preview_volume_for_gain(self.select.select_assets.preview_normalization_gain())
    }

    pub(super) fn select_preview_volume_for_gain(&self, analyzed_gain: f32) -> f32 {
        let mix = &self.boot.profile_config.audio_mix;
        let volume = crate::config::play::volume_unit_to_f32(mix.master_volume)
            * crate::config::play::volume_unit_to_f32(mix.preview_volume)
            * select_preview_normalization_gain(mix.normalize_chart_volume, analyzed_gain);
        volume.clamp(0.0, 1.0)
    }

    pub(super) fn play_select_preview_sample(
        &mut self,
        prepared: PreparedSelectPreview,
        volume_factor: f32,
    ) -> bool {
        let volume = self.select_preview_volume_for_gain(prepared.normalization_gain)
            * volume_factor.clamp(0.0, 1.0);
        let loaded = self.select.select_assets.play_preview(prepared, volume, Instant::now());
        if loaded {
            self.start_audio_output_stream();
        }
        loaded
    }

    pub(super) fn update_select_preview_fade(&mut self) {
        let now = Instant::now();
        self.select.select_assets.advance_preview_fade(now);
        self.apply_select_preview_audio_mix();
    }

    pub(super) fn apply_select_preview_audio_mix(&self) {
        let preview_factor = self.select.select_assets.preview_fade_factor(Instant::now());
        self.select.select_assets.set_preview_volume(self.select_preview_volume() * preview_factor);
        self.set_select_bgm_volume_factor(1.0 - preview_factor);
    }

    pub(super) fn set_select_bgm_volume_factor(&self, factor: f32) {
        let Some(manager) = &self.audio.system_sound else {
            return;
        };
        let volume = system_sound_volume_from_mix(
            &self.boot.profile_config.audio_mix,
            crate::system_sound::SoundType::Select,
        ) * factor.clamp(0.0, 1.0);
        manager.set_volume(crate::system_sound::SoundType::Select, volume);
    }
}
