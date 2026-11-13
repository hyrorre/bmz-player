use super::*;

impl WinitApp {
    /// 現在の profile config のスキンパスを renderer へ再適用する。
    ///
    /// 起動時と同じ `load_skin_textures` 経路を使い、JSON スキンは
    /// バックグラウンド decode + 段階 install パイプラインへ流す。
    pub(super) fn reload_skins(&mut self, mut request: SkinReloadRequest) {
        let skin = self.boot.profile_config.skin.clone();
        if self.skin.skin_pipeline.update_access(&skin.external_access) {
            self.skin.skin_defs_cache.clear();
            self.skin.skin_header_checks.clear();
            self.select.lr2_skin_settings.clear_cache();
            self.skin.skin_video_sources.clear();
            self.audio.skin_lua_audio = Default::default();
            self.result.result_skin_audio = None;
            // Revocation drops installed Lua VMs even if the replacement fails.
            self.renderer.set_select_skin_context(SkinContext::default());
            self.renderer.set_decide_skin_context(SkinContext::default());
            self.renderer.set_play_skin_context(SkinContext::default(), false);
            self.renderer.set_result_skin_context(SkinContext::default());
            request = SkinReloadRequest {
                select: true,
                decide: true,
                result: true,
                course_result: true,
                play4: true,
                play5: true,
                play6: true,
                play7: true,
                play8: true,
                play9: true,
                play10: true,
                play14: true,
                offsets: true,
            };
        }
        let select_last_played_at = if request.select || request.decide {
            self.select_snapshot().last_played_at
        } else {
            None
        };
        let texture_request = SkinReloadRequest { result: false, course_result: false, ..request };
        let frontend_chart = if request.decide {
            Some(self.decide_load_chart_snapshot())
        } else {
            request.select.then(|| self.select_load_chart_snapshot())
        };
        let (pending_select, pending_decide, _pending_result) = reload_skin_textures(
            &self.boot.app_paths,
            &mut self.skin.skin_pipeline,
            texture_request,
            FrontendLuaLoadInputs {
                player_name: &self.boot.profile_config.display_name,
                ir_name: result_ir_skin_name(&self.boot.profile_config.ir),
                player_stats: &self.select.player_stats,
            },
            &skin,
            self.skin.lua_runtime_mode,
            self.boot.app_config.select.experimental_detail_options,
            select_last_played_at,
            frontend_chart.as_ref(),
        );
        if request.select {
            self.skin.skin_pipeline.set_pending(SkinKind::Select, pending_select);
        }
        if request.decide {
            self.skin.skin_pipeline.set_pending(SkinKind::Decide, pending_decide);
        }
        if request.result || request.course_result {
            self.skin.last_result_skin_signature = None;
            if matches!(self.current_scene_kind(), AppSceneKind::Result) {
                let slot = self.current_result_skin_slot();
                if matches!(
                    (slot, request.result, request.course_result),
                    (ResultSkinSlot::Normal, true, _) | (ResultSkinSlot::Course, _, true)
                ) {
                    self.spawn_result_skin_decode_for(slot);
                }
            }
        }
        // 旧 generation 分の upload 結果は apply_uploaded_skin の generation
        // チェックで破棄されるため、ここでの明示的なキュー破棄は不要。
        if let Some((
            key_mode,
            session_mode,
            old_path,
            old_options,
            old_files,
            runtime_state,
            play_preload_generation,
        )) = self.skin.last_play_skin_signature.clone()
            && skin_reload_request_includes_key_mode(request, key_mode)
        {
            let selection = play_skin_selection_for_session(&skin, key_mode, session_mode);
            let mut offset_state = runtime_state.clone();
            offset_state.offset_values.clear();
            offset_state.offset_id_values.clear();
            apply_skin_offsets_to_lua_runtime_state(&mut offset_state, selection.offsets);
            if request.offsets
                && old_path == selection.path.trim()
                && old_options == *selection.options
                && old_files == *selection.files
                && bmz_skin::is_qwilight_play_skin_path(Path::new(selection.path))
                && qwilight_native_offsets_equal(&runtime_state, &offset_state)
                && (self.is_kind_pending_decode(SkinKind::Play)
                    || self.skin.skin_pipeline.has_current_install(SkinKind::Play))
            {
                // A pending replacement gets the newest settings at installation.
                // Do not invalidate that generation or start another asset decode.
                if !self.is_kind_pending_decode(SkinKind::Play)
                    && let Some(document) = self.renderer.play_skin_document()
                {
                    let values = qwilight_offset_values_from_config(selection.offsets, document);
                    self.renderer.set_play_qwilight_offsets(&values);
                }
                self.skin.last_play_skin_signature = Some((
                    key_mode,
                    session_mode,
                    old_path,
                    old_options,
                    old_files,
                    offset_state,
                    play_preload_generation,
                ));
                self.frame.request_immediate_frame();
                self.request_redraw();
                return;
            }
            let play_options_only = self.play.active_play.is_some()
                && old_path == selection.path.trim()
                && old_files == *selection.files
                && old_options != *selection.options;
            let play_options_need_full_reload = play_options_only
                && self.play_skin_options_need_full_reload(key_mode, selection.path.trim());
            if play_options_only
                && !play_options_need_full_reload
                && self.apply_active_play_skin_options_fast_path(key_mode, selection.options)
            {
                self.skin.last_play_skin_signature = Some((
                    key_mode,
                    session_mode,
                    selection.path.trim().to_string(),
                    selection.options.clone(),
                    selection.files.clone(),
                    runtime_state,
                    play_preload_generation,
                ));
                tracing::debug!(
                    ?key_mode,
                    "play skin option change applied without background reload"
                );
                tracing::info!(?request, "skin reload queued from egui skin panel");
                return;
            }
            self.skin.last_play_skin_signature = None;
            self.spawn_play_skin_decode_for(key_mode, session_mode, runtime_state);
        }
        let pending_after_reload = self.has_pending_skin_reload();
        tracing::info!(?request, "skin reload queued from egui skin panel");
        if pending_after_reload {
            self.frame.request_immediate_frame();
            let _ = self.drain_pending_skins();
            self.request_redraw();
        }
    }

    /// Select / Decide の論理シーン進入ごとに、現在の設定でskinを再decodeする。
    ///
    /// 決定的なLua documentと画像は共有cacheで再利用される一方、ロード時乱数や
    /// wildcardのRandom指定はこのdecode要求を単位として再抽選される。
    pub(super) fn reload_skin_for_scene_entry(&mut self, kind: SkinKind) {
        if kind == SkinKind::Select && self.enter_select_with_prewarmed_skin() {
            return;
        }
        self.discard_skin_prewarm(kind);
        self.reload_skin_for_scene_entry_with_chart(kind, None);
        self.request_scene_skin(kind);
    }

    /// The Select decode inputs a scene entry would use now.
    pub(super) fn select_skin_load(&self) -> Option<FrontendSkinLoad> {
        let frontend_chart = self.select_load_chart_snapshot();
        frontend_skin_load(
            &self.boot.app_paths,
            SkinKind::Select,
            FrontendLuaLoadInputs {
                player_name: &self.boot.profile_config.display_name,
                ir_name: result_ir_skin_name(&self.boot.profile_config.ir),
                player_stats: &self.select.player_stats,
            },
            &self.boot.profile_config.skin,
            self.skin.lua_runtime_mode,
            self.boot.app_config.select.experimental_detail_options,
            self.select_snapshot().last_played_at,
            Some(&frontend_chart),
        )
    }

    /// Decide進入時の再decodeへ、表示するsnapshotの選択譜面の値を渡す。
    ///
    /// beatorajaは選択曲のある状態でDecide skinを読むため、ロード時に難易度色などを
    /// 決めるskinがある。起動時のdecodeは選択曲を持たず、難易度未定義（150）として読む。
    /// Queue only: the caller starts Play decoding and prepares presentation
    /// images before waiting for this Decide skin.
    pub(super) fn reload_decide_skin_for_chart(&mut self, snapshot: &RenderSnapshot) {
        self.reload_skin_for_scene_entry_with_chart(SkinKind::Decide, Some(snapshot));
    }

    /// Decide進入以外（skin設定パネル、profile切替等）のDecide再decodeに渡す選択譜面。
    ///
    /// 表示中のDecide、なければSelectのカーソル位置の譜面を使う。フォルダ等で譜面が
    /// ない場合は空のsnapshot（難易度未定義の150）になる。
    fn decide_load_chart_snapshot(&self) -> RenderSnapshot {
        if let Some(decide) = &self.play.pending_decide {
            return decide.snapshot_for_render();
        }
        self.select_load_chart_snapshot()
    }

    fn select_load_chart_snapshot(&self) -> RenderSnapshot {
        match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => {
                let mut snapshot = row
                    .chart
                    .as_ref()
                    .map(|chart| self.decide_snapshot_for_chart(chart.chart_id))
                    .unwrap_or_default();
                snapshot.chart_md5 = row.score_md5();
                snapshot.chart_sha256 = row.score_sha256();
                snapshot
            }
            _ => RenderSnapshot::default(),
        }
    }

    fn reload_skin_for_scene_entry_with_chart(
        &mut self,
        kind: SkinKind,
        decide_chart: Option<&RenderSnapshot>,
    ) {
        if matches!(kind, SkinKind::Select) {
            self.discard_cli_play_on_select();
        }
        let request = match kind {
            SkinKind::Select => SkinReloadRequest { select: true, ..Default::default() },
            SkinKind::Decide => SkinReloadRequest { decide: true, ..Default::default() },
            SkinKind::Play | SkinKind::Result => {
                unreachable!("play/result skins have scene-specific runtime state")
            }
        };
        let skin = self.boot.profile_config.skin.clone();
        let select_last_played_at = self.select_snapshot().last_played_at;
        let frontend_chart = decide_chart.cloned().unwrap_or_else(|| match kind {
            SkinKind::Select => self.select_load_chart_snapshot(),
            _ => self.decide_load_chart_snapshot(),
        });
        let (pending_select, pending_decide, _) = reload_skin_textures(
            &self.boot.app_paths,
            &mut self.skin.skin_pipeline,
            request,
            FrontendLuaLoadInputs {
                player_name: &self.boot.profile_config.display_name,
                ir_name: result_ir_skin_name(&self.boot.profile_config.ir),
                player_stats: &self.select.player_stats,
            },
            &skin,
            self.skin.lua_runtime_mode,
            self.boot.app_config.select.experimental_detail_options,
            select_last_played_at,
            Some(&frontend_chart),
        );
        let pending = match kind {
            SkinKind::Select => pending_select,
            SkinKind::Decide => pending_decide,
            SkinKind::Play | SkinKind::Result => unreachable!(),
        };
        self.skin.skin_pipeline.set_pending(kind, pending);
    }

    pub(super) fn apply_active_play_skin_options_fast_path(
        &mut self,
        key_mode: KeyMode,
        options: &BTreeMap<String, String>,
    ) -> bool {
        let Some((enabled_options, property_ops)) =
            self.renderer.play_skin_document().map(|document| {
                (
                    enabled_options_from_selections(document, options),
                    skin_document_property_ops(document),
                )
            })
        else {
            return false;
        };
        let applied_options = enabled_options.clone();
        if self.renderer.set_play_skin_user_selected_options(enabled_options) {
            if let Some(sources) = self.skin.skin_video_sources.get_mut(&SkinKind::Play) {
                apply_skin_video_source_enabled_options(sources, &applied_options, &property_ops);
            }
            tracing::debug!(?key_mode, "applied play skin option change before background reload");
            return true;
        }
        false
    }

    pub(super) fn play_skin_options_need_full_reload(
        &self,
        key_mode: KeyMode,
        trimmed_path: &str,
    ) -> bool {
        let path = if trimmed_path.is_empty() {
            default_play_skin_document_path_from_paths(&self.boot.app_paths, key_mode)
        } else {
            match self.boot.app_paths.resolve_path_ref(trimmed_path) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(
                        ?key_mode,
                        path = %trimmed_path,
                        error = %format_error_chain(&error),
                        "keeping play skin background reload because skin path could not be resolved"
                    );
                    return true;
                }
            }
        };
        match skin_path_options_need_full_reload(&path) {
            Ok(needed) => needed,
            Err(error) => {
                tracing::warn!(
                    ?key_mode,
                    path = %path.display(),
                    error = %format_error_chain(&error),
                    "keeping play skin background reload because skin option dependencies could not be inspected"
                );
                true
            }
        }
    }

    /// 決定対象チャートの key_mode に対応するプレイスキンを background decode に投入する。
    /// 直前と同じ mode かつ path/options/files が同じなら何もしない。
    pub(super) fn spawn_play_skin_decode_for(
        &mut self,
        key_mode: KeyMode,
        session_mode: SessionMode,
        mut runtime_state: bmz_skin::LuaLoadRuntimeState,
    ) {
        runtime_state.runtime_mode = self.skin.lua_runtime_mode;
        runtime_state.screen_size = self.renderer.render_target_size();
        let selection =
            play_skin_selection_for_session(&self.boot.profile_config.skin, key_mode, session_mode);
        runtime_state.offset_values.clear();
        runtime_state.offset_id_values.clear();
        apply_skin_offsets_to_lua_runtime_state(&mut runtime_state, selection.offsets);
        let trimmed = selection.path.trim();
        let signature = play_skin_signature(
            key_mode,
            session_mode,
            trimmed,
            selection.options,
            selection.files,
            &runtime_state,
            self.play.play_preload_generation,
        );

        if self.skin.last_play_skin_signature.as_ref() == Some(&signature) {
            tracing::debug!(?key_mode, "play skin reuse (signature unchanged)");
            return;
        }
        self.skin.last_play_skin_signature = Some(signature);
        self.skin.skin_pipeline.set_pending(SkinKind::Play, false);
        let generation = self.skin.skin_pipeline.bump_generation(SkinKind::Play);

        let (path, path_label, options, files) = if trimmed.is_empty() {
            (
                default_play_skin_document_path_from_paths(&self.boot.app_paths, key_mode),
                format!("default play skin for {key_mode:?}"),
                BTreeMap::new(),
                BTreeMap::new(),
            )
        } else {
            let path = match self.boot.app_paths.resolve_path_ref(trimmed) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(
                        ?key_mode,
                        path = trimmed,
                        error = %format_error_chain(&error),
                        "failed to resolve play skin path; using existing textures"
                    );
                    return;
                }
            };
            (path, trimmed.to_string(), selection.options.clone(), selection.files.clone())
        };
        if !is_decodable_skin_path(&path) {
            tracing::warn!(
                ?key_mode,
                path = %path.display(),
                "play skin path is not a supported beatoraja skin file; using existing textures"
            );
            return;
        }

        let chart_state = self
            .play
            .pending_play_preload
            .as_ref()
            .filter(|pending| pending.generation == self.play.play_preload_generation)
            .map(|pending| pending.skin_chart_state.clone())
            .or_else(|| {
                let facts = self
                    .play
                    .preloaded_play_session
                    .as_ref()
                    .map(|play| &play.preloaded.render_snapshot_cache.skin_chart)
                    .or_else(|| {
                        let play = self.play.active_play.as_ref()?;
                        Some(&play.running.render_snapshot_cache.skin_chart)
                    })?;
                let state = PlaySkinChartState::default();
                state.finish(Ok(Arc::clone(facts)));
                Some(state)
            });
        let request =
            SkinDecodeRequest::new(generation, path, SkinKind::Play, options, files, runtime_state)
                .with_play_chart_state(chart_state)
                .with_skin_access(&self.boot.app_paths, &self.boot.profile_config.skin)
                .reuse_installed_fonts(&self.skin.skin_pipeline);
        spawn_skin_decode(&self.skin.skin_pipeline, request);
        self.skin.skin_pipeline.set_pending(SkinKind::Play, true);
        tracing::info!(?key_mode, path = %path_label, generation, "play skin decode queued");
    }
}
