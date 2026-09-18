use super::*;

impl WinitApp {
    /// upload worker を起動する。surface 接続後に一度だけ呼ぶ。
    /// decode worker からの receiver (`skin_decode_rx`) と GPU uploader を worker へ
    /// move し、worker は decode 結果を受けて GPU アップロードし `skin_upload_tx` で
    /// main へ返す。
    pub(super) fn start_skin_upload_worker(&mut self) {
        if self.skin.skin_pipeline.upload_worker_started {
            return;
        }
        let Some(decode_rx) = self.skin.skin_pipeline.decode_rx.take() else {
            return;
        };
        let Some(uploader) = self.renderer.gpu_uploader() else {
            // surface 未接続。次回接続時に再試行できるよう receiver を戻す。
            self.skin.skin_pipeline.decode_rx = Some(decode_rx);
            return;
        };
        let upload_tx = self.skin.skin_pipeline.upload_tx.clone();
        let texture_cache = self.skin.skin_pipeline.gpu_texture_cache.clone();
        let event_proxy = self.event_proxy.clone();
        thread::Builder::new()
            .name("skin-upload".to_string())
            .spawn(move || {
                skin_upload_worker(decode_rx, upload_tx, uploader, texture_cache, event_proxy)
            })
            .expect("failed to spawn skin upload thread");
        self.skin.skin_pipeline.upload_worker_started = true;
    }

    /// 起動直後の Select が表示されている間は、非表示の Decide / Result skin が
    /// GPU queue と main-thread install を奪わないよう upload worker を保留する。
    /// Select skin の input 時刻を過ぎた後、または別 scene が必要になった時点で開始する。
    pub(super) fn start_deferred_skin_uploads_if_ready(&mut self) {
        if self.skin.skin_pipeline.upload_worker_started {
            return;
        }
        let select_startup_active = matches!(self.view_state(), AppViewState::Select);
        let select_input_ms =
            self.renderer.select_skin_document().map(|document| document.input.max(0)).unwrap_or(0);
        let select_elapsed_ms = (self.select_time().0 / 1_000).clamp(0, i32::MAX as i64) as i32;
        if should_defer_initial_skin_uploads(
            select_startup_active,
            self.first_frame_startup_completed,
            select_elapsed_ms,
            select_input_ms,
        ) {
            return;
        }
        self.start_skin_upload_worker();
    }

    /// upload worker が GPU アップロードまで終えたスキンを非ブロッキングで取り込む。
    /// 毎フレーム呼ぶ。テクスチャ挿入 + フォント登録 + SkinContext 構築のみで軽量。
    pub(super) fn drain_pending_skins(&mut self) -> SkinDrainStats {
        let mut stats = SkinDrainStats::default();
        for _ in 0..MAX_SKIN_UPLOADS_PER_REDRAW {
            match self.skin.skin_pipeline.upload_rx.try_recv() {
                Ok(result) => {
                    stats.received_count += 1;
                    stats.max_upload_wait_us = stats
                        .max_upload_wait_us
                        .max(instant_elapsed_us_u64(result.upload_finished_at));
                    if self.apply_uploaded_skin(result) {
                        stats.applied_count += 1;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        stats
    }

    /// 指定された kind のスキンがアップロードされ取り込まれるまでブロックして待つ。
    /// scene 遷移直前 (特にプレイ開始) に呼ぶ。GPU アップロードは upload worker 上で
    /// 進むため、main は worker からの受信を待つだけで重い同期処理は無い。
    /// 先読みが間に合っていれば待ちはゼロ。
    pub(super) fn ensure_skin_ready(&mut self, kind: SkinKind) {
        // Select の開始演出中でも、scene 遷移が skin を要求した場合は待機中の
        // decode 結果を直ちに GPU upload へ流す。
        self.start_skin_upload_worker();
        while self.is_kind_pending_decode(kind) {
            match self.skin.skin_pipeline.upload_rx.recv() {
                Ok(result) => {
                    let _ = self.apply_uploaded_skin(result);
                }
                Err(_) => break,
            }
        }
    }

    pub(super) fn ensure_result_skin_ready(&mut self, slot: ResultSkinSlot) {
        self.refresh_result_favorite_chart();
        self.spawn_result_skin_decode_for(slot);
        self.ensure_skin_ready(SkinKind::Result);
        self.renderer.reset_result_skin_runtime();
        let (skin_bgm_volume, skin_se_volume) = self.result_skin_audio_volumes();
        let started = self.result.result_skin_audio.as_mut().is_some_and(|audio| {
            audio.reset();
            audio.start_scene(skin_bgm_volume, skin_se_volume)
        });
        if started {
            self.start_audio_output_stream();
        }
        self.result.result_panel = self
            .renderer
            .result_skin_document()
            .and_then(|document| document.result_panel_default)
            .filter(|panel| (0..=2).contains(panel))
            .unwrap_or(0);
        self.clear_result_ir_scroll_input();
    }

    pub(super) fn ensure_result_skin_ready_for_entry(&mut self, slot: ResultSkinSlot) {
        self.skin.last_result_skin_signature = None;
        self.ensure_result_skin_ready(slot);
    }

    pub(super) fn refresh_result_favorite_chart(&mut self) {
        let Some(sha256) =
            self.result.finished_play.as_ref().map(|finished| finished.result.chart_sha256)
        else {
            self.result.result_favorite_chart = false;
            return;
        };
        self.result.result_favorite_chart = match self.boot.collection_db.is_favorite_chart(sha256)
        {
            Ok(favorite) => favorite,
            Err(error) => {
                tracing::warn!(%error, "failed to load favorite chart state for result");
                false
            }
        };
    }

    pub(super) fn current_result_skin_slot(&self) -> ResultSkinSlot {
        if self.result.finished_course.is_some() {
            ResultSkinSlot::Course
        } else {
            ResultSkinSlot::Normal
        }
    }

    pub(super) fn spawn_result_skin_decode_for(&mut self, slot: ResultSkinSlot) {
        let skin = &self.boot.profile_config.skin;
        let table_song = !self.play.play_table_text_primary.is_empty();
        let ir_name = result_ir_skin_name(&self.boot.profile_config.ir);
        let runtime_state = self.result_lua_runtime_state(slot, table_song, ir_name);
        let signature = result_skin_signature_for_config(skin, slot, runtime_state);
        if !self.skin.skin_pipeline.is_pending(SkinKind::Result)
            && self.skin.last_result_skin_signature.as_ref() == Some(&signature)
        {
            tracing::debug!(?slot, "result skin reuse (signature unchanged)");
            return;
        }

        let (_, trimmed, options, files, runtime_state) = signature.clone();
        self.skin.last_result_skin_signature = Some(signature);
        self.skin.skin_pipeline.set_pending(SkinKind::Result, false);
        let generation = self.skin.skin_pipeline.bump_generation(SkinKind::Result);

        let (path, path_label, options, files) = if trimmed.is_empty() {
            (
                default_skin_document_path_from_paths(&self.boot.app_paths, SkinKind::Result),
                "default result skin".to_string(),
                BTreeMap::new(),
                BTreeMap::new(),
            )
        } else {
            let path = match self.boot.app_paths.resolve_path_ref(&trimmed) {
                Ok(path) => path,
                Err(error) => {
                    self.set_empty_result_skin_context();
                    tracing::warn!(
                        ?slot,
                        path = %trimmed,
                        error = %format_error_chain(&error),
                        "failed to resolve result skin path; using fallback result drawing"
                    );
                    return;
                }
            };
            (path, trimmed.clone(), options, files)
        };
        if !is_decodable_skin_path(&path) {
            self.set_empty_result_skin_context();
            tracing::warn!(
                ?slot,
                path = %path.display(),
                "result skin path is not a supported beatoraja skin file; using fallback result drawing"
            );
            return;
        }

        let request = SkinDecodeRequest::new(
            generation,
            path,
            SkinKind::Result,
            options,
            files,
            runtime_state,
        )
        .with_library_roots(self.boot.app_paths.skin_library_roots())
        .reuse_installed_fonts(&self.skin.skin_pipeline);
        spawn_skin_decode(&self.skin.skin_pipeline, request);
        self.skin.skin_pipeline.set_pending(SkinKind::Result, true);
        tracing::info!(?slot, path = %path_label, generation, "result skin decode queued");
    }

    pub(super) fn result_lua_runtime_state(
        &self,
        slot: ResultSkinSlot,
        table_song: bool,
        ir_name: Option<&str>,
    ) -> bmz_skin::LuaLoadRuntimeState {
        let summary = match slot {
            ResultSkinSlot::Course => self.result.finished_course_skin_summary.as_ref(),
            ResultSkinSlot::Normal => {
                self.result.finished_play.as_ref().map(|finished| &finished.summary)
            }
        };
        let key_mode = summary.map(|summary| summary.key_mode).unwrap_or_default();
        let number_values =
            summary.map(result_lua_runtime_number_values_for_summary).unwrap_or_default();
        let mut runtime_state = lua_runtime_state_for_result(
            table_song,
            ir_name,
            self.result_score_save_enabled_for_slot(slot),
            self.current_result_autoplay(),
            key_mode,
            number_values,
            &self.boot.profile_config.display_name,
        );
        if let Some(summary) = summary {
            apply_skin_attempt_lua_load_state(&mut runtime_state, summary.skin_attempt);
            apply_result_summary_lua_load_state(
                &mut runtime_state,
                summary,
                &self.play.play_table_text_primary,
                &self.play.play_table_text_secondary,
                &self.play.play_table_text_fallback,
            );
        }
        if summary.is_none() {
            runtime_state.event_index_values.insert(
                54,
                i32::try_from(bmz_render::skin::select_double_option_index(
                    self.result_double_option_for_slot(slot).as_str(),
                ))
                .unwrap_or_default(),
            );
        }
        if let Some(stage) = self.current_course_stage_marker() {
            apply_course_mode_lua_options(&mut runtime_state, Some(stage));
        }
        if let ResultSkinSlot::Course = slot
            && let Some(course) = &self.result.finished_course
        {
            apply_course_mode_lua_options(&mut runtime_state, None);
            apply_course_result_lua_load_state(&mut runtime_state, course);
        }
        runtime_state.runtime_mode = self.skin.lua_runtime_mode;
        runtime_state
    }

    pub(super) fn result_double_option_for_slot(&self, slot: ResultSkinSlot) -> DoubleOption {
        match slot {
            ResultSkinSlot::Normal => self
                .result
                .finished_play
                .as_ref()
                .map(|finished| finished.applied_arrange.double_option)
                .unwrap_or(DoubleOption::Off),
            ResultSkinSlot::Course => DoubleOption::Off,
        }
    }

    pub(super) fn current_result_score_save_enabled(&self) -> bool {
        self.result_score_save_enabled_for_slot(self.current_result_skin_slot())
    }

    pub(super) fn current_result_autoplay(&self) -> bool {
        self.result.finished_play.as_ref().is_some_and(|finished| finished.result.autoplay)
    }

    pub(super) fn result_score_save_enabled_for_slot(&self, slot: ResultSkinSlot) -> bool {
        match slot {
            ResultSkinSlot::Course => self
                .result
                .finished_course
                .as_ref()
                .is_some_and(|course| course.course_score_id.is_some()),
            ResultSkinSlot::Normal => self
                .result
                .finished_play
                .as_ref()
                .is_some_and(|finished| finished.stored.score_history_id > 0),
        }
    }

    pub(super) fn set_empty_result_skin_context(&mut self) {
        let context = self
            .skin
            .default_skin_manifest
            .clone()
            .map(SkinContext::from_manifest)
            .unwrap_or_default();
        self.renderer.set_result_skin_context(context);
        self.skin.skin_video_sources.remove(&SkinKind::Result);
    }

    pub(super) fn is_kind_pending_decode(&self, kind: SkinKind) -> bool {
        self.skin.skin_pipeline.is_pending(kind)
    }

    pub(super) fn has_pending_skin_reload(&self) -> bool {
        self.skin.skin_pipeline.has_pending()
    }

    /// upload worker から届いた `UploadedSkin` を Renderer へ取り込む。
    /// stale generation は破棄。GPU アップロードは worker で完了済みなので、
    /// ここではハンドル挿入・フォント登録・SkinContext 構築のみ (軽量)。
    pub(super) fn apply_uploaded_skin(&mut self, pending: PendingUploadResult) -> bool {
        let PendingUploadResult {
            generation,
            path,
            kind,
            queued_at,
            decode_started_at,
            decode_finished_at,
            upload_started_at,
            upload_finished_at,
            uploaded,
        } = pending;
        let apply_started_at = Instant::now();
        let current_generation = self.skin.skin_pipeline.generation(kind);
        if generation != current_generation {
            tracing::debug!(
                path = %path.display(),
                kind = ?kind,
                generation,
                current = current_generation,
                total_us = instant_elapsed_us_u64(queued_at),
                decode_us = instant_duration_us_u64(decode_started_at, decode_finished_at),
                upload_us = instant_duration_us_u64(upload_started_at, upload_finished_at),
                "discarding stale uploaded skin"
            );
            return false;
        }
        self.skin.skin_pipeline.set_pending(kind, false);
        self.skin
            .skin_pipeline
            .record_load_result(&path, uploaded.as_ref().err().map(|error| format!("{error:#}")));
        let uploaded = match uploaded {
            Ok(uploaded) => uploaded,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    kind = ?kind,
                    total_us = instant_elapsed_us_u64(queued_at),
                    decode_us = instant_duration_us_u64(decode_started_at, decode_finished_at),
                    upload_us = instant_duration_us_u64(upload_started_at, upload_finished_at),
                    error = %format_error_chain(&error),
                    "failed to decode/upload beatoraja skin in background"
                );
                return false;
            }
        };
        let Some(manifest) = self.skin.default_skin_manifest.clone() else {
            self.skin
                .skin_pipeline
                .record_load_result(&path, Some("default skin manifest is unavailable".into()));
            tracing::warn!(
                path = %path.display(),
                kind = ?kind,
                "skipping uploaded skin because default skin manifest is unavailable"
            );
            return false;
        };
        let UploadedSkin {
            kind,
            document,
            lua_runtime,
            fonts,
            prepared,
            audio_assets,
            decode_stats,
            upload_stats,
        } = uploaded;
        if kind == SkinKind::Result {
            self.result.result_skin_audio = self.audio.system_audio.as_ref().map(|audio| {
                crate::skin_audio::SkinAudioRuntime::install(
                    audio.engine(),
                    &document,
                    audio_assets,
                )
            });
        }
        let font_count = fonts.len();
        let font_install_start = Instant::now();
        let mut font_install_count = 0usize;
        let mut font_install_skip_count = 0usize;
        let mut font_install_failed_count = 0usize;
        // フォント登録。reload 間で同一 font key の場合は text atlas reset ごと避ける。
        for font in fonts {
            let stored_id = font.stored_id.clone();
            let cache_key = font.cache_key.clone();
            if let Some(cache_key) = cache_key.as_ref()
                && self.skin.skin_pipeline.installed_font_cache.get(&stored_id) == Some(cache_key)
            {
                font_install_skip_count += 1;
                continue;
            }
            if install_decoded_font(&mut self.renderer, font) {
                font_install_count += 1;
                if let Some(cache_key) = cache_key {
                    self.skin.skin_pipeline.installed_font_cache.insert(stored_id, cache_key);
                } else {
                    self.skin.skin_pipeline.installed_font_cache.remove(&stored_id);
                }
            } else {
                font_install_failed_count += 1;
                self.skin.skin_pipeline.installed_font_cache.remove(&stored_id);
            }
        }
        let font_install_us = instant_elapsed_us_u64(font_install_start);
        // アップロード済みテクスチャを差し込み、SkinDocumentTexture を組む。
        let mut document_textures = Vec::with_capacity(prepared.len());
        let mut video_sources = Vec::new();
        for source in prepared {
            let PreparedSource {
                source_id,
                path,
                texture,
                prepared,
                size,
                is_video,
                cache_key,
                texture_lease: _texture_lease,
            } = source;
            if let Some(prepared) = prepared {
                self.renderer.insert_prepared_texture(TextureId(texture.0), prepared);
                if let Some(cache_key) = cache_key
                    && let Ok(mut cache) = self.skin.skin_pipeline.gpu_texture_cache.lock()
                {
                    cache.insert(cache_key, texture, size);
                }
            }
            if is_video {
                let gating = skin_video_source_gating(&document, &source_id);
                video_sources.push(ActiveSkinVideoSource {
                    texture,
                    path,
                    decoder: None,
                    last_pts: None,
                    loop_start_us: 0,
                    active: gating.active,
                    gating_op_sets: gating.op_sets,
                    enabled_options: document.enabled_options(),
                    result_ranktime_ms: document.ranktime,
                    failed: false,
                });
            }
            document_textures.push(SkinDocumentTexture { source_id, texture, source_size: size });
        }
        if video_sources.is_empty() {
            self.skin.skin_video_sources.remove(&kind);
        } else {
            self.skin.skin_video_sources.insert(kind, video_sources);
        }
        let preserve_play_dynamic_timers =
            kind == SkinKind::Play && self.play.active_play.is_some();
        let installed_sources = document_textures.len();
        set_decoded_skin_context(
            &mut self.renderer,
            kind,
            manifest,
            document,
            lua_runtime,
            document_textures,
            preserve_play_dynamic_timers,
        );
        self.skin.pending_skin_render_probe =
            Some(PendingSkinRenderProbe { kind, generation, applied_at: Instant::now() });
        // Reclaim both cache metadata and GPU resources after replacing the scene.
        // Pending worker results hold leases, including cache hits without pixels.
        let active = self.renderer.active_skin_texture_ids();
        if let Ok(mut cache) = self.skin.skin_pipeline.gpu_texture_cache.lock() {
            for texture in cache.evict_unused(&active) {
                self.renderer.remove_image_texture(TextureId(texture.0));
            }
        }
        self.frame.request_immediate_frame();
        tracing::debug!(
            path = %path.display(),
            kind = ?kind,
            generation,
            total_us = instant_elapsed_us_u64(queued_at),
            decode_queue_us = instant_duration_us_u64(queued_at, decode_started_at),
            decode_thread_us = instant_duration_us_u64(decode_started_at, decode_finished_at),
            upload_queue_us = instant_duration_us_u64(decode_finished_at, upload_started_at),
            upload_thread_us = instant_duration_us_u64(upload_started_at, upload_finished_at),
            apply_queue_us = instant_duration_us_u64(upload_finished_at, apply_started_at),
            apply_us = instant_elapsed_us_u64(apply_started_at),
            document_us = decode_stats.document_us,
            document_cache_hits = decode_stats.document_cache_hits,
            document_cache_misses = decode_stats.document_cache_misses,
            document_cache_uncacheable = decode_stats.document_cache_uncacheable,
            document_cache_disabled = decode_stats.document_cache_disabled,
            font_count,
            font_decode_us = decode_stats.font_decode_us,
            font_payload_skipped = decode_stats.font_payload_skipped,
            font_cache_hits = decode_stats.font_cache_hits,
            font_cache_misses = decode_stats.font_cache_misses,
            font_cache_uncacheable = decode_stats.font_cache_uncacheable,
            font_cache_disabled = decode_stats.font_cache_disabled,
            font_install_us,
            font_installed = font_install_count,
            font_install_skipped = font_install_skip_count,
            font_install_failed = font_install_failed_count,
            source_task_count = decode_stats.source_task_count,
            source_decode_us = decode_stats.source_decode_us,
            decoded_sources = decode_stats.decoded_source_count,
            decoded_source_bytes = decode_stats.decoded_source_bytes,
            builtin_sources = decode_stats.builtin_source_count,
            image_sources = decode_stats.image_source_count,
            video_sources = decode_stats.video_source_count,
            source_cache_hits = decode_stats.source_cache_hits,
            source_cache_misses = decode_stats.source_cache_misses,
            source_cache_uncacheable = decode_stats.source_cache_uncacheable,
            source_cache_disabled = decode_stats.source_cache_disabled,
            video_source_cache_hits = decode_stats.video_source_cache_hits,
            video_source_cache_misses = decode_stats.video_source_cache_misses,
            video_source_cache_uncacheable = decode_stats.video_source_cache_uncacheable,
            video_source_cache_disabled = decode_stats.video_source_cache_disabled,
            source_texture_cache_hits = decode_stats.source_texture_cache_hits,
            source_texture_cache_hit_bytes = decode_stats.source_texture_cache_hit_bytes,
            video_source_texture_cache_hits = decode_stats.video_source_texture_cache_hits,
            video_source_texture_cache_hit_bytes =
                decode_stats.video_source_texture_cache_hit_bytes,
            uploaded_sources = upload_stats.uploaded_source_count,
            uploaded_source_bytes = upload_stats.uploaded_source_bytes,
            uploaded_video_sources = upload_stats.uploaded_video_source_count,
            uploaded_video_source_bytes = upload_stats.uploaded_video_source_bytes,
            texture_cache_hits = upload_stats.texture_cache_hits,
            texture_cache_misses = upload_stats.texture_cache_misses,
            texture_cache_uncacheable = upload_stats.texture_cache_uncacheable,
            texture_cache_disabled = upload_stats.texture_cache_disabled,
            video_texture_cache_hits = upload_stats.video_texture_cache_hits,
            video_texture_cache_misses = upload_stats.video_texture_cache_misses,
            video_texture_cache_uncacheable = upload_stats.video_texture_cache_uncacheable,
            video_texture_cache_disabled = upload_stats.video_texture_cache_disabled,
            installed_sources,
            "beatoraja skin reload timings"
        );
        if kind == SkinKind::Select && matches!(self.view_state(), AppViewState::Select) {
            self.restart_select_scene_timers();
        }
        true
    }
}

fn should_defer_initial_skin_uploads(
    select_view: bool,
    first_frame_startup_completed: bool,
    select_elapsed_ms: i32,
    select_input_ms: i32,
) -> bool {
    select_view && (!first_frame_startup_completed || select_elapsed_ms < select_input_ms.max(0))
}

#[cfg(test)]
mod deferred_upload_tests {
    use super::should_defer_initial_skin_uploads;

    #[test]
    fn initial_select_defers_background_skin_uploads_until_input_time() {
        assert!(should_defer_initial_skin_uploads(true, false, 0, 2_000));
        assert!(should_defer_initial_skin_uploads(true, true, 1_999, 2_000));
        assert!(!should_defer_initial_skin_uploads(true, true, 2_000, 2_000));
    }

    #[test]
    fn non_select_scene_does_not_defer_required_skin_uploads() {
        assert!(!should_defer_initial_skin_uploads(false, false, 0, 2_000));
    }
}
