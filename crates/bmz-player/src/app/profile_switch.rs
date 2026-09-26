use super::*;
use crate::bootstrap::PreparedProfile;
use crate::ui::ProfileManagerAction;

pub(super) struct PreparedProfileSwitch {
    profile: PreparedProfile,
    folder_summaries: SelectFolderSummaryRuntime,
}

enum ProfileChangeStage {
    Preparing(Receiver<Result<Option<PreparedProfileSwitch>>>),
    WaitingForSkin {
        prepared: Box<PreparedProfileSwitch>,
        generation: u64,
        uploaded: Option<Box<PendingUploadResult>>,
    },
    WaitingForIr {
        prepared: Box<PreparedProfileSwitch>,
        select_skin: Box<PendingUploadResult>,
    },
}

pub(super) struct PendingProfileChange {
    action: ProfileManagerAction,
    stage: ProfileChangeStage,
}

impl PendingProfileChange {
    /// 切替先の結果はまだrendererへ適用しない。旧profileの描画とLua状態を保つ。
    pub(super) fn stage_uploaded_skin(
        &mut self,
        result: PendingUploadResult,
    ) -> Option<PendingUploadResult> {
        if let ProfileChangeStage::WaitingForSkin { generation, uploaded, .. } = &mut self.stage
            && result.kind == SkinKind::Select
            && result.generation == *generation
            && uploaded.is_none()
        {
            *uploaded = Some(Box::new(result));
            None
        } else {
            Some(result)
        }
    }
}

fn queue_profile_select_skin(
    paths: &AppPaths,
    profile: &ProfileConfig,
    pipeline: &mut SkinPipelineRuntime,
    runtime_mode: bmz_skin::LuaSkinRuntimeMode,
) -> Result<u64> {
    let skin = &profile.skin;
    let trimmed = skin.select.trim();
    let path = if trimmed.is_empty() {
        default_skin_document_path_from_paths(paths, SkinKind::Select)
    } else {
        paths.resolve_path_ref(trimmed)?
    };
    if !is_decodable_skin_path(&path) {
        bail!("unsupported select skin: {}", path.display());
    }
    let generation = pipeline.bump_generation(SkinKind::Select);
    spawn_skin_decode(
        pipeline,
        SkinDecodeRequest::new(
            generation,
            path,
            SkinKind::Select,
            if trimmed.is_empty() { BTreeMap::new() } else { skin.select_options.clone() },
            if trimmed.is_empty() { BTreeMap::new() } else { skin.select_files.clone() },
            lua_runtime_state_with_mode(
                lua_runtime_state_with_skin_offsets(
                    lua_runtime_state_for_frontend(
                        &profile.display_name,
                        result_ir_skin_name(&profile.ir),
                    ),
                    &skin.select_offsets,
                ),
                runtime_mode,
            ),
        )
        .with_library_roots(paths.skin_library_roots()),
    );
    pipeline.set_pending(SkinKind::Select, true);
    Ok(generation)
}

fn validate_profile_select_skin(
    result: &PendingUploadResult,
    generation: u64,
    manifest: Option<&SkinManifest>,
) -> Result<()> {
    if result.kind != SkinKind::Select || result.generation != generation {
        bail!("profile select skin was superseded");
    }
    let uploaded = result.uploaded.as_ref().map_err(|error| {
        anyhow::anyhow!("failed to prepare select skin {}: {error:#}", result.path.display())
    })?;
    if uploaded.kind != SkinKind::Select || uploaded.document.skin_type != 5 {
        bail!("not a select skin: {}", result.path.display());
    }
    if manifest.is_none() {
        bail!("default skin manifest is unavailable");
    }
    Ok(())
}

fn prepare_profile_action(
    paths: &AppPaths,
    action: &ProfileManagerAction,
) -> Result<Option<PreparedProfileSwitch>> {
    let (id, activate) = match action {
        ProfileManagerAction::Switch(id) => (id, true),
        ProfileManagerAction::Create { id, display_name, activate } => {
            crate::profile_cmd::create_profile(paths, id, display_name.as_deref(), false)?;
            (id, *activate)
        }
        ProfileManagerAction::Copy { source_id, id, display_name, activate } => {
            crate::profile_cmd::copy_profile(paths, source_id, id, display_name.as_deref(), false)?;
            (id, *activate)
        }
    };
    if !activate {
        return Ok(None);
    }
    let mut profile = PreparedProfile::load(paths, id)?;
    profile.config.play.session_mode = Some(SessionMode::Normal);
    profile.config.play.auto_play = false;
    let folder_summaries = SelectFolderSummaryRuntime::new(
        paths.library_db.clone(),
        profile.paths.score_db.clone(),
        &[],
        profile.config.play.ln_mode_policy,
        profile.config.play.rule_mode,
    )?;
    Ok(Some(PreparedProfileSwitch { profile, folder_summaries }))
}

impl WinitApp {
    pub(super) fn profile_change_allowed(&self) -> bool {
        self.select_maintenance_allowed()
            && !self.viewer_mode
            && self.play.active_course.is_none()
            && self.play.practice_session.is_none()
            && self.select.course_builder.is_none()
            && self.select.autoplay_folder.is_none()
            && self.select.ir_battle.pending.is_none()
            && self.select.settings_edit.is_none()
            && self.select.key_config_edit.is_none()
            && self.jobs.pending_replay_import.is_none()
            && self.jobs.pending_update_handoff.is_none()
            && !self.ui.egui.as_ref().is_some_and(EguiLayer::profile_operations_busy)
    }

    pub(super) fn restart_profile_ir_sync(&mut self) {
        self.jobs.ir_sync = None;
        if !self.viewer_mode && tokio::runtime::Handle::try_current().is_ok() {
            self.jobs.ir_sync =
                spawn_ir_sync_worker(&self.boot, self.jobs.maintenance_select_tx.subscribe());
        }
    }

    pub(super) fn request_profile_change(&mut self, action: ProfileManagerAction) {
        if self.jobs.profile_change.is_some() {
            return;
        }
        if !self.profile_change_allowed() {
            let message = Localizer::new(self.boot.profile_config.ui.locale())
                .text("profile-manager-unavailable");
            self.finish_profile_change(&action, Err(anyhow::anyhow!(message)));
            return;
        }
        if matches!(&action, ProfileManagerAction::Switch(id) if id == &self.boot.profile_config.id)
        {
            return;
        }
        // debounce中の編集も旧profileへ保存する。CLIの一時上書きは既存save APIが除く。
        let saved =
            save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
                .and_then(|_| {
                    save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config)
                });
        if let Err(error) = saved {
            self.finish_profile_change(&action, Err(error));
            return;
        }
        let (tx, rx) = mpsc::channel();
        let paths = self.boot.app_paths.clone();
        let worker_action = action.clone();
        let proxy = self.event_proxy.clone();
        let spawned = thread::Builder::new().name("profile-change".into()).spawn(move || {
            let result = prepare_profile_action(&paths, &worker_action);
            let _ = tx.send(result);
            let _ = proxy.send_event(AppUserEvent::ProfileChangeReady);
        });
        if let Err(error) = spawned {
            self.finish_profile_change(&action, Err(error.into()));
            return;
        }
        self.jobs.profile_change =
            Some(PendingProfileChange { action, stage: ProfileChangeStage::Preparing(rx) });
        self.clear_profile_input();
        self.sync_select_maintenance_gate();
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.settings_save_finished(false, Ok(()));
            egui.settings_save_finished(true, Ok(()));
            egui.profile_change_started();
        }
        self.request_redraw();
    }

    pub(super) fn poll_profile_change(&mut self) {
        let Some(mut pending) = self.jobs.profile_change.take() else { return };
        if let ProfileChangeStage::Preparing(rx) = &pending.stage {
            match rx.try_recv() {
                Ok(Ok(Some(prepared))) => {
                    let generation = self.queue_profile_select_skin(&prepared.profile.config);
                    match generation {
                        Ok(generation) => {
                            pending.stage = ProfileChangeStage::WaitingForSkin {
                                prepared: Box::new(prepared),
                                generation,
                                uploaded: None,
                            };
                        }
                        Err(error) => {
                            self.finish_profile_change(&pending.action, Err(error));
                            return;
                        }
                    }
                }
                Ok(Ok(None)) => {
                    self.finish_profile_change(&pending.action, Ok(()));
                    return;
                }
                Ok(Err(error)) => {
                    self.finish_profile_change(&pending.action, Err(error));
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.finish_profile_change(
                        &pending.action,
                        Err(anyhow::anyhow!("profile loader disconnected")),
                    );
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if matches!(pending.stage, ProfileChangeStage::WaitingForSkin { uploaded: Some(_), .. }) {
            let ProfileChangeStage::WaitingForSkin {
                prepared, uploaded: Some(select_skin), ..
            } = pending.stage
            else {
                unreachable!()
            };
            self.skin.skin_pipeline.set_pending(SkinKind::Select, false);
            let result = validate_profile_select_skin(
                &select_skin,
                self.skin.skin_pipeline.generation(SkinKind::Select),
                self.skin.default_skin_manifest.as_ref(),
            );
            self.skin.skin_pipeline.record_load_result(
                &select_skin.path,
                result.as_ref().err().map(|error| format!("{error:#}")),
            );
            if let Err(error) = result {
                self.finish_profile_change(&pending.action, Err(error));
                return;
            }
            // スキン準備失敗時は旧IR workerも維持する。成功後、送信済みジョブのDB更新を待つ。
            if let Some(worker) = &self.jobs.ir_sync {
                worker.stop();
            }
            pending.stage = ProfileChangeStage::WaitingForIr { prepared, select_skin };
        }
        if matches!(pending.stage, ProfileChangeStage::WaitingForIr { .. })
            && self.jobs.ir_sync.as_ref().is_none_or(|worker| worker.task.is_finished())
        {
            let ProfileChangeStage::WaitingForIr { prepared, select_skin } = pending.stage else {
                unreachable!()
            };
            let result = self.install_profile(*prepared, *select_skin);
            self.restart_profile_ir_sync();
            self.finish_profile_change(&pending.action, result);
        } else {
            self.jobs.profile_change = Some(pending);
        }
    }

    fn queue_profile_select_skin(&mut self, profile: &ProfileConfig) -> Result<u64> {
        if self.skin.default_skin_manifest.is_none() {
            bail!("default skin manifest is unavailable");
        }
        self.start_skin_upload_worker();
        if self.skin.skin_pipeline.upload_worker.is_none() {
            bail!("skin upload worker is unavailable");
        }
        queue_profile_select_skin(
            &self.boot.app_paths,
            profile,
            &mut self.skin.skin_pipeline,
            self.skin.lua_runtime_mode,
        )
    }

    fn finish_profile_change(&mut self, action: &ProfileManagerAction, result: Result<()>) {
        if let Err(error) = &result {
            tracing::warn!(error = %format_error_chain(error), "profile operation failed");
        }
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.profile_change_finished(
                action,
                result.map_err(|e| format!("{e:#}")),
                self.boot.profile_config.ui.locale(),
            );
        }
        self.sync_select_maintenance_gate();
        self.request_redraw();
    }

    fn clear_profile_input(&mut self) {
        let _ = self.input.handle_focus_lost();
        self.sync_select_holds_from_pressed_controls();
        self.clear_select_hold();
        self.reset_select_analog_scroll();
        self.reset_play_analog_scroll();
        self.clear_result_ir_scroll_input();
        self.clear_play_control_holds();
        self.select.select_exit_hold_started_at = None;
        self.select.select_slider_dragging_type = None;
        self.select.ir_battle.hold_started_at = None;
        self.select.ir_battle.hold_control = None;
        self.select.ir_battle.hold_short_action = None;
    }

    fn install_profile(
        &mut self,
        prepared: PreparedProfileSwitch,
        select_skin: PendingUploadResult,
    ) -> Result<()> {
        validate_profile_select_skin(
            &select_skin,
            self.skin.skin_pipeline.generation(SkinKind::Select),
            self.skin.default_skin_manifest.as_ref(),
        )?;
        self.boot.activate_prepared_profile(prepared.profile)?;
        // ここから先は失敗しないruntimeの入れ替え。非同期結果はreceiverごと破棄する。
        self.clear_profile_input();
        self.stop_select_preview();
        self.invalidate_play_preload();
        self.play.play_media_cache = None;
        self.play.last_started_chart_id = None;
        self.play.last_battle_target = None;
        self.play.last_play_snapshot = None;
        self.result.result_ir = None;
        self.result.result_skin_audio = None;
        self.result.last_play_session_mode = SessionMode::Normal;
        self.select.autoplay_folder = None;
        self.select.random_trainer = RandomTrainerState::default();
        self.select.select_ir = Default::default();
        self.select.ir_battle = Default::default();
        self.select.search = SelectSearchRuntime::new(Instant::now());
        self.select.folder_stack.clear();
        self.select.selected_index_stack.clear();
        self.select.selected_index = 0;
        self.select.select_items.clear();
        self.select.selected_replay_slot = None;
        *self.select.replay_slot_cache.borrow_mut() = None;
        self.select.score_refresh = Default::default();
        self.select.select_folder_summaries = prepared.folder_summaries;
        self.invalidate_select_distributions();
        self.select.select_option_panel = 0;
        self.select.select_mode_filter =
            SelectModeFilter::from_str_or_default(&self.boot.profile_config.select.mode_filter);
        self.select.select_difficulty_filter = SelectDifficultyFilter::from_str_or_default(
            &self.boot.profile_config.select.difficulty_filter,
        );
        self.select.select_sort =
            SelectSort::from_str_or_default(&self.boot.profile_config.select.sort);
        self.select.select_keys = SelectKeyBindings::from_profile(&self.boot.profile_config.input);
        self.reload_select_items();
        self.sync_selected_play_mode();
        self.sync_select_play_options_from_profile();
        self.apply_gamepad_analog_config();
        self.refresh_player_stats_snapshot();
        self.restart_select_scene_timers();
        self.jobs.pending_locale_refresh = false;
        self.jobs.pending_rival_sync = None;
        self.jobs.startup_rival_sync =
            RianRivalSyncRequest::from_profile(&self.boot.profile_config);
        // 同じIRアカウントでもprofileの境界では古い取得結果を引き継がない。
        self.jobs.table_fetch.pending_rian = None;
        self.jobs.table_fetch.rian_generation =
            self.jobs.table_fetch.rian_generation.wrapping_add(1);
        self.jobs.table_fetch.rian_next_refresh_at = None;
        self.jobs.table_fetch.rian_last_started_at = None;
        self.jobs.table_fetch.rian_refresh_queued = true;
        self.jobs.table_fetch.rian_refresh_manual = false;
        self.reconcile_rian_table_identity();
        // Selectは準備済みの世代をそのまま適用する。他sceneの旧profileの結果は破棄する。
        for kind in [SkinKind::Decide, SkinKind::Play, SkinKind::Result] {
            self.skin.skin_pipeline.bump_generation(kind);
            self.skin.skin_pipeline.set_pending(kind, false);
        }
        self.skin.last_play_skin_signature = None;
        self.skin.last_result_skin_signature = None;
        self.skin.pending_skin_render_probe = None;
        self.skin.skin_video_sources.clear();
        // 読み込み失敗時も旧profileのLua closureや表示名を残さない。
        let context = self
            .skin
            .default_skin_manifest
            .clone()
            .map(SkinContext::from_manifest)
            .unwrap_or_default();
        self.renderer.set_decide_skin_context(context.clone());
        self.renderer.set_play_skin_context(context.clone(), false);
        self.renderer.set_result_skin_context(context);
        self.renderer
            .set_default_font_coverage(self.boot.profile_config.ui.locale().font_coverage());
        // 検証・永続化後、次の描画より前にSelectを直接置き換える。
        let applied = self.apply_uploaded_skin(select_skin);
        debug_assert!(applied, "validated profile select skin must install");
        self.reload_skins(SkinReloadRequest {
            decide: true,
            result: true,
            course_result: true,
            ..Default::default()
        });
        if let Some(manager) = &self.audio.system_sound {
            manager.stop_all_bgm();
        }
        self.audio.system_sound = None;
        self.audio.pending_system_sound = None;
        self.audio.system_sound_generation = self.audio.system_sound_generation.wrapping_add(1);
        self.audio.draining_audio = None;
        self.audio.system_sound_catalog = system_sound_catalog_from_boot(&self.boot);
        self.start_system_sound_load();
        self.sync_realtime_profile_settings();
        self.sync_discord_presence_config();
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.reset_for_profile(&self.boot.profile_config);
        }
        tracing::info!(profile = %self.boot.profile_config.id, "profile switched without restart");
        Ok(())
    }
}

#[cfg(test)]
mod skin_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::profile_tests::ProfileTestDir;

    #[test]
    fn profile_action_creation_and_copy_only_activate_when_requested() {
        let data = ProfileTestDir::new();
        let mut boot = data.boot();
        let create = ProfileManagerAction::Create {
            id: "new".into(),
            display_name: Some("New".into()),
            activate: false,
        };
        assert!(prepare_profile_action(&data.paths, &create).unwrap().is_none());
        assert_eq!(
            crate::config::load::load_app_config(&data.paths.config_toml).unwrap().active_profile,
            "default"
        );
        boot.profile_config.audio_mix.master_volume = 35;
        boot.profile_config.play.session_mode = Some(SessionMode::Autoplay);
        boot.profile_config.play.auto_play = true;
        save_profile_config(&boot.profile_paths.profile_toml, &boot.profile_config).unwrap();
        let copy = ProfileManagerAction::Copy {
            source_id: "default".into(),
            id: "copy".into(),
            display_name: None,
            activate: true,
        };
        let prepared = prepare_profile_action(&data.paths, &copy).unwrap().unwrap();
        assert_eq!(prepared.profile.config.id, "copy");
        assert_eq!(prepared.profile.config.audio_mix.master_volume, 35);
        assert_eq!(prepared.profile.config.play.session_mode, Some(SessionMode::Normal));
        assert!(!prepared.profile.config.play.auto_play);
        assert_eq!(
            crate::config::load::load_app_config(&data.paths.config_toml).unwrap().active_profile,
            "default"
        );
    }

    #[test]
    fn stopped_ir_worker_exits_while_select_is_paused_without_network_access() {
        let data = ProfileTestDir::new();
        let mut boot = data.boot();
        let provider = &mut boot.profile_config.ir.providers[0];
        provider.enabled = true;
        provider.base_url = "http://127.0.0.1:1".into();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (_select_tx, select_rx) = tokio::sync::watch::channel(false);
            let worker = spawn_ir_sync_worker(&boot, select_rx).unwrap();
            worker.stop();
            tokio::time::timeout(Duration::from_secs(2), async {
                while !worker.task.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
        });
    }
}
