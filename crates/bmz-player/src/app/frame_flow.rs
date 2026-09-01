use super::*;
use crate::screens::select_model::SelectCourseRow;

mod render;

struct EguiProfileBefore {
    app_input: GlobalInputConfig,
    locale: crate::i18n::AppLocale,
    play: PlayDefaultsConfig,
    lane: LaneViewConfig,
    input: ProfileInputConfig,
}

fn egui_scene_name(scene_kind: AppSceneKind) -> &'static str {
    match scene_kind {
        AppSceneKind::Select => "Select",
        AppSceneKind::Decide => "Decide",
        AppSceneKind::Play => "Play",
        AppSceneKind::Result => "Result",
    }
}

fn practice_panel_context(
    practice: Option<&mut PracticeSession>,
    media_ready: bool,
    input_enabled: bool,
    default_position: Option<(f32, f32)>,
) -> Option<PracticePanelContext<'_>> {
    let practice = practice.filter(|practice| practice.phase == PracticePhase::Config)?;
    Some(PracticePanelContext {
        property: &mut practice.property,
        graph: &practice.last_graph,
        graph_start_time_ms: practice.graph_start_time_ms,
        is_double: practice.is_double,
        cursor: &mut practice.cursor,
        chart_title: &practice.chart_title,
        media_ready,
        input_enabled,
        max_end_time_ms: practice.max_end_time_ms,
        default_position,
    })
}

impl WinitApp {
    pub(super) fn restart_select_scene_timers(&mut self) {
        let now = Instant::now();
        self.select.select_scene_timer_armed = false;
        self.select.select_scene_started_at = now;
        self.restart_select_bar_timer_without_scroll(now);
        self.select.option_panel_started_at = now;
        self.select.option_panel_off_started_at = [None; 6];
    }

    pub(super) fn arm_select_scene_timers_after_render(
        &mut self,
        select_view: bool,
        render_status: Option<RenderSurfaceStatus>,
    ) {
        if !should_arm_select_scene_timers(
            select_view,
            self.select.select_scene_timer_armed,
            render_status,
        ) {
            return;
        }
        let now = Instant::now();
        self.select.select_scene_timer_armed = true;
        self.select.select_scene_started_at = now;
        self.restart_select_bar_timer_without_scroll(now);
        self.select.option_panel_started_at = now;
        self.select.option_panel_off_started_at = [None; 6];
    }

    pub(super) fn current_frame_limit(&self) -> u32 {
        if self.ui.focused {
            self.boot.app_config.video.target_fps
        } else {
            self.boot.app_config.video.frame_limit_in_background
        }
    }

    fn current_frame_pacing_state(&self) -> FramePacingState {
        let window_mode = match &self.ui.applied_window_mode {
            WindowMode::Windowed => FrameWindowMode::Windowed,
            WindowMode::BorderlessFullscreen => FrameWindowMode::BorderlessFullscreen,
            WindowMode::ExclusiveFullscreen => FrameWindowMode::ExclusiveFullscreen,
        };
        FramePacingState {
            focused: self.ui.focused,
            effective_frame_limit: self.current_frame_limit(),
            present_mode: config_present_mode(&self.boot.app_config.video),
            window_mode,
        }
    }

    /// `RedrawRequested` が現在の deadline に到達していればフレームを開始する。
    ///
    /// deadline より早い redraw は描画せず `WaitUntil` へ戻す。event loop thread を
    /// sleep させないため、待機中も keyboard/device event を遅延なく受け取れる。
    pub(super) fn begin_scheduled_frame(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let pacing_state = self.current_frame_pacing_state();
        let now = Instant::now();
        match self.frame.begin_scheduled_frame(now, pacing_state) {
            FrameSchedule::Start => true,
            FrameSchedule::WaitUntil(deadline) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                false
            }
        }
    }

    /// 次の frame deadline まで winit に待機させ、到達時に redraw を要求する。
    /// FPS が 0、設定変更直後、明示的な skip 時は即座に次フレームを要求する。
    pub(super) fn schedule_next_frame(&self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let fps = self.current_frame_limit();
        if let Some(deadline) = self.frame.next_deadline(now, fps) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            return;
        }

        if fps == 0 {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
        self.request_redraw();
    }

    /// egui の 1 フレームを構築し、renderer へ描画データを渡す。
    /// `render_current_scene` の前に呼ぶこと。
    pub(super) fn run_egui_frame(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let scene_kind = self.current_scene_kind();
        if scene_kind == AppSceneKind::Select {
            self.sync_selected_play_mode();
        }
        let scene = egui_scene_name(scene_kind);
        let course_result_active =
            matches!(scene_kind, AppSceneKind::Result) && self.result.finished_course.is_some();

        // Result IR is consumed by both the skin snapshot and the optional egui panel.
        // Keep its async state moving even while the hidden menu uses an idle egui frame.
        self.update_result_ir_for_frame(scene_kind, course_result_active);
        // Select IR is consumed by the select skin snapshot. Keep its debounce,
        // request, and completion handling moving while the hidden menu uses an idle frame.
        self.update_egui_select_ir(scene_kind);
        self.update_egui_course_editor_data(scene_kind);
        if self.run_idle_egui_frame_if_available(&window, scene_kind, scene) {
            return;
        }
        let info = self.egui_debug_info(&window, scene);
        let skin_meta = self.egui_skin_meta();

        // コース graph は egui を Option から取り出した後、clone せず参照で渡す。
        let course_result = self.result.finished_course.as_ref();
        let course_preview = self.egui_course_preview(scene_kind);
        let course_editor = &self.course_editor_cache.data;
        let practice_media_ready = self.practice_media_ready();
        let practice_input_enabled = self.play.play_ending.is_none();
        let practice_default_position = self.renderer.play_skin_practice_position();
        let mut practice_panel_ctx = practice_panel_context(
            self.play.practice_session.as_mut(),
            practice_media_ready,
            practice_input_enabled,
            practice_default_position,
        );
        let result_ir_panel = self.result.result_ir.as_mut();
        let update_dialog = self.jobs.update_prompt.as_ref().map(UpdatePrompt::as_dialog);
        let obs_connection_status = self
            .integrations
            .obs_controller
            .as_ref()
            .map(crate::obs::ObsController::status)
            .unwrap_or_else(crate::obs::ObsConnectionStatus::disabled);
        let connected_gamepads =
            self.gamepad.as_ref().map(|gamepad| gamepad.connected_gamepads()).unwrap_or_default();
        let Some(mut egui) = self.ui.egui.take() else {
            return;
        };
        let profile_before = EguiProfileBefore {
            app_input: self.boot.app_config.input.clone(),
            locale: self.boot.profile_config.ui.locale(),
            play: self.boot.profile_config.play.clone(),
            lane: self.boot.profile_config.lane.clone(),
            input: self.boot.profile_config.input.clone(),
        };
        let output = egui.run(
            &window,
            EguiRunContext {
                info: &info,
                app_config: &mut self.boot.app_config,
                profile_config: &mut self.boot.profile_config,
                random_trainer: &mut self.select.random_trainer,
                skin_meta: &skin_meta,
                skin_catalog: &self.skin.skin_catalog,
                course_result,
                course_preview: course_preview.as_ref(),
                course_editor,
                select_course_builder: self.select.course_builder.as_mut().map(|builder| {
                    SelectCourseBuilderData {
                        definition: &mut builder.definition,
                        max_entries: crate::course::LOCAL_COURSE_MAX_ENTRIES,
                    }
                }),
                practice: practice_panel_ctx.as_mut(),
                result_ir: result_ir_panel,
                profile_root: &self.boot.profile_paths.root_dir,
                app_paths: &self.boot.app_paths,
                difficulty_tables: &self.select.difficulty_tables,
                log_buffer: &self.ui.log_buffer,
                update_dialog,
                obs_connection_status: &obs_connection_status,
                connected_gamepads: &connected_gamepads,
            },
        );
        self.ui.egui = Some(egui);
        self.apply_egui_output(&window, output, profile_before);
    }

    fn run_idle_egui_frame_if_available(
        &mut self,
        window: &Window,
        scene_kind: AppSceneKind,
        scene: &'static str,
    ) -> bool {
        let practice_overlay = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        let scene_allows_idle = scene_kind != AppSceneKind::Play || self.play.play_ending.is_none();
        let select_course_builder = self.select.course_builder.is_some();
        let use_idle_frame = scene_allows_idle
            && self.ui.egui.as_ref().is_some_and(|egui| {
                !egui.needs_full_frame(
                    scene,
                    practice_overlay,
                    select_course_builder,
                    self.jobs.update_prompt.is_some(),
                )
            });
        if !use_idle_frame {
            return false;
        }

        let Some(mut egui) = self.ui.egui.take() else {
            return true;
        };
        let frame =
            egui.run_idle_frame(window, self.boot.profile_config.ui.locale().font_coverage());
        self.ui.egui = Some(egui);
        self.renderer.set_egui_frame(frame);
        true
    }

    fn egui_debug_info(&self, window: &Window, scene: &'static str) -> DebugInfo {
        let size = window.inner_size();
        let presentation = self.renderer.surface_presentation_status();
        DebugInfo {
            scene,
            current_fps: self.frame.current_fps(),
            width: size.width,
            height: size.height,
            effective_present_mode: presentation.map(|status| status.effective_mode),
            maximum_frame_latency: presentation.map(|status| status.maximum_frame_latency),
        }
    }

    fn egui_skin_meta(&mut self) -> SkinConfigMeta {
        let skin = &self.boot.profile_config.skin;
        let paths = [
            skin.play4.clone(),
            skin.play5.clone(),
            skin.play6.clone(),
            skin.play7.clone(),
            skin.play8.clone(),
            skin.play9.clone(),
            skin.play10.clone(),
            skin.play14.clone(),
            skin.battle5.clone(),
            skin.battle7.clone(),
            skin.course_result.clone(),
        ];
        let [
            play4,
            play5,
            play6,
            play7,
            play8,
            play9,
            play10,
            play14,
            battle5,
            battle7,
            course_result,
        ] = paths.map(|path| self.play_skin_defs_for_path(&path));
        SkinConfigMeta {
            select: SceneSkinDefs::from_document(self.renderer.select_skin_document()),
            decide: SceneSkinDefs::from_document(self.renderer.decide_skin_document()),
            play4,
            play5,
            play6,
            play7,
            play8,
            play9,
            play10,
            play14,
            battle5,
            battle7,
            result: SceneSkinDefs::from_document(self.renderer.result_skin_document()),
            course_result,
        }
    }

    fn update_result_ir_for_frame(&mut self, scene_kind: AppSceneKind, course_result_active: bool) {
        if scene_kind == AppSceneKind::Result
            && self
                .result
                .result_ir
                .as_ref()
                .is_some_and(|state| state.is_course() != course_result_active)
        {
            self.result.result_ir = None;
        }
        if course_result_active {
            self.spawn_course_result_ir_if_needed();
        } else if scene_kind == AppSceneKind::Result {
            self.spawn_play_result_ir_if_needed();
        }

        if scene_kind == AppSceneKind::Result || self.play.play_ending.is_some() {
            self.poll_result_ir_into_select_cache();
        } else {
            self.result.result_ir = None;
        }
    }

    fn spawn_course_result_ir_if_needed(&mut self) {
        if self.result.result_ir.is_some() || self.result.finished_course_ir_attempted {
            return;
        }
        // 無効設定や未解決 identity でも、この Result 滞在中の判定は一度にする。
        self.result.finished_course_ir_attempted = true;
        let Some((
            course_hash,
            rian_course_hash_v1,
            bms_ir_course_key,
            gauge,
            ln_policy,
            rule_mode,
        )) = self.course_result_ir_target()
        else {
            return;
        };
        let score_id = self
            .result
            .finished_course
            .as_ref()
            .and_then(|course| course.course_score_id)
            .unwrap_or_default();
        self.result.result_ir = crate::screens::result_ir::spawn_course_result_ir_task(
            self.boot.profile_paths.root_dir.clone(),
            self.boot.profile_paths.score_db.clone(),
            self.boot.profile_paths.network_db.clone(),
            self.boot.app_paths.logs_dir.clone(),
            &self.boot.profile_config.ir,
            score_id,
            crate::screens::result_ir::ResultIrCourseHashes {
                local: course_hash,
                rian_v1: rian_course_hash_v1,
                bms_ir: bms_ir_course_key,
            },
            gauge,
            ln_policy,
            rule_mode,
        );
    }

    fn spawn_play_result_ir_if_needed(&mut self) {
        if self.result.result_ir.is_some() {
            return;
        }
        let Some(finished) = &self.result.finished_play else {
            return;
        };
        if finished.stored.score_history_id <= 0 {
            return;
        }
        self.result.result_ir = crate::screens::result_ir::spawn_result_ir_task(
            self.boot.profile_paths.root_dir.clone(),
            self.boot.profile_paths.score_db.clone(),
            self.boot.profile_paths.network_db.clone(),
            self.boot.app_paths.logs_dir.clone(),
            &self.boot.profile_config.ir,
            finished.stored.score_history_id,
            crate::storage::common::hash_to_hex(&finished.result.chart_sha256),
            finished.ln_policy,
            finished.double_option,
            finished.rule_mode,
        );
    }

    fn poll_result_ir_into_select_cache(&mut self) {
        let rankings = self.result.result_ir.as_mut().map(|state| state.poll()).unwrap_or_default();
        for ranking in rankings {
            self.select
                .select_ir
                .cache_result_global_ranking(&ranking.chart_sha256_hex, &ranking.ranking);
        }
    }

    fn egui_course_preview(&self, scene_kind: AppSceneKind) -> Option<SelectCourseRow> {
        if scene_kind != AppSceneKind::Select {
            return None;
        }
        match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Course(row)) => Some(row.clone()),
            _ => None,
        }
    }

    fn update_egui_course_editor_data(&mut self, scene_kind: AppSceneKind) {
        let query = self
            .ui
            .egui
            .as_ref()
            .filter(|egui| scene_kind == AppSceneKind::Select && egui.course_editor_visible())
            .map(|egui| egui.course_editor_search_query().trim().to_string());
        let visible = query.is_some();
        let query = query.unwrap_or_default();
        let (reload_courses, reload_charts) =
            self.course_editor_cache.reload_requirements(visible, &query);

        if reload_courses {
            let courses = self
                .boot
                .library_db
                .list_courses()
                .unwrap_or_else(|error| {
                    tracing::error!(%error, "failed to load courses for editor");
                    Vec::new()
                })
                .into_iter()
                .filter(|course| {
                    course.definition.constraints.gauge
                        != bmz_core::course::CourseGaugeConstraint::Keys24
                })
                .collect();
            self.course_editor_cache.set_courses(courses);
        }

        if reload_charts {
            let charts = if query.is_empty() {
                self.boot.library_db.list_charts(200, 0)
            } else {
                self.boot.library_db.search_charts_limited(&query, 200)
            }
            .unwrap_or_else(|error| {
                tracing::error!(%error, query, "failed to search charts for course editor");
                Vec::new()
            })
            .into_iter()
            .filter(|chart| !chart.mode.contains("24") && !chart.mode.contains("48"))
            .map(|chart| CourseEditorChart {
                chart_id: chart.chart_id,
                title: chart.title,
                artist: chart.artist,
                play_level: chart.play_level,
                mode: chart.mode,
                md5: crate::storage::common::hash_to_hex(&chart.md5),
                sha256: crate::storage::common::hash_to_hex(&chart.sha256),
            })
            .collect();
            self.course_editor_cache.set_charts(query, charts);
        }
    }

    fn update_egui_select_ir(&mut self, scene_kind: AppSceneKind) {
        if scene_kind != AppSceneKind::Select {
            return;
        }
        let rival_target = crate::screens::select_ir::SelectRivalFetchTarget::from_profile(
            &self.boot.profile_config,
        );
        self.select.select_ir.update_rival(rival_target, &self.boot.profile_paths.root_dir);
        let selected_course = self.selected_course_ir_target();
        let ir_config = self.boot.profile_config.ir.clone();
        if let Some(course) = selected_course {
            let context = format!(
                "course:{}:{}:{}:{}:{}",
                course.course_hash,
                course.rian_course_hash_v1,
                course.gauge,
                course.ln_policy,
                course.rule_mode.as_str()
            );
            self.select.select_ir.update_course(
                &ir_config,
                &self.boot.profile_paths.root_dir,
                &context,
                Some(course),
            );
            return;
        }

        let (selected, ln_profile, key_mode) =
            match self.select.select_items.get(self.select.selected_index) {
                Some(SelectItem::Chart(row)) => (
                    row.score_sha256(),
                    row.chart.as_ref().map(|chart| chart.ln_profile).unwrap_or_default(),
                    row.chart
                        .as_ref()
                        .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
                        .unwrap_or_default(),
                ),
                _ => (None, crate::ln_policy::ChartLnProfile::default(), KeyMode::default()),
            };
        let configured_ln_policy = self.boot.profile_config.play.ln_mode_policy;
        let ln_policy = crate::ln_policy::score_ln_policy(configured_ln_policy, ln_profile);
        let double_option =
            self.select.double_option.normalize_for_key_mode(key_mode).score_bucket();
        let rule_mode = self.boot.profile_config.play.rule_mode;
        let context =
            select_ir_cache_context(configured_ln_policy, ln_policy, double_option, rule_mode);
        self.select.select_ir.update(
            &ir_config,
            &self.boot.profile_paths.root_dir,
            &context,
            ln_policy,
            double_option,
            rule_mode,
            selected,
        );
    }

    fn apply_egui_output(
        &mut self,
        window: &Window,
        output: crate::ui::EguiOutput,
        profile_before: EguiProfileBefore,
    ) {
        self.reconcile_rian_table_identity();
        self.apply_egui_profile_changes(&profile_before);
        self.apply_egui_input_config(window, &profile_before.app_input);
        self.renderer.set_egui_frame(output.frame);
        if let Some(action) = output.key_config_action {
            self.apply_egui_key_config_action(action);
        }
        if output.practice_leave {
            self.stop_play_like_escape("practice egui leave requested");
            return;
        }
        if output.practice_start {
            self.start_practice_round();
        } else {
            self.refresh_practice_preview_snapshot();
        }
        if let Some(action) = output.course_editor_action {
            self.apply_course_editor_action(action);
        }
        if let Some(action) = output.select_course_builder_action {
            self.apply_select_course_builder_action(action);
        }
        self.apply_egui_video_config(window);

        let mut apply_obs_config = output.obs_enabled_changed;
        if output.save_app_config {
            match save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config) {
                Ok(()) => {
                    tracing::info!("app config saved from egui settings panel");
                    apply_obs_config = true;
                }
                Err(error) => tracing::error!(%error, "failed to save app config"),
            }
        }
        if apply_obs_config {
            self.sync_obs_controller();
        }
        if output.check_for_update {
            self.spawn_update_check("manual update check", true);
        }
        if let Some(action) = output.update_dialog_action {
            self.handle_update_dialog_action(action);
        }
        if output.apply_audio_output {
            self.reopen_audio_output();
        }
        if !output.table_fetch_urls.is_empty() {
            self.spawn_table_fetches(output.table_fetch_urls, "egui table fetch".to_string());
        }
        for request in output.song_scan_requests {
            self.spawn_song_scan_request(request);
        }
        if output.trigger_song_rescan {
            self.load_songs_and_reload();
        }
        if let Some(request) = output.score_import_request {
            self.import_external_scores(request);
        }
        if let Some(request) = output.replay_import_request {
            self.spawn_beatoraja_replay_import(request);
        }
        if output.cancel_replay_import {
            self.cancel_beatoraja_replay_import();
        }
        if output.save_profile_config {
            match save_profile_config(
                &self.boot.profile_paths.profile_toml,
                &self.boot.profile_config,
            ) {
                Ok(()) => tracing::info!("profile config saved from egui skin panel"),
                Err(error) => tracing::error!(%error, "failed to save profile config"),
            }
        }
        if output.reset_skin_config {
            self.reset_skin_config_from_disk();
        } else if output.skin_reload_request.any() {
            if output.skin_reload_request.offsets {
                self.apply_profile_skin_offsets_to_active_play();
            }
            if output.skin_reload_request.any_reload() {
                self.reload_skins(output.skin_reload_request);
            }
        }
    }

    fn apply_egui_profile_changes(&mut self, before: &EguiProfileBefore) {
        let locale = self.boot.profile_config.ui.locale();
        self.renderer.set_default_font_coverage(locale.font_coverage());
        if locale != before.locale {
            self.select.search.clear_message();
            self.reload_select_items();
        }
        self.sync_changed_select_play_options_from_profile(&before.play);
        self.sync_changed_select_score_context(SelectScoreContext::from_play(&before.play));
        if before.play.key_mode_conversion != self.boot.profile_config.play.key_mode_conversion
            || before.play.seven_to_nine_pattern
                != self.boot.profile_config.play.seven_to_nine_pattern
            || before.play.seven_to_nine_type != self.boot.profile_config.play.seven_to_nine_type
            || before.play.seven_to_nine_rule_mode
                != self.boot.profile_config.play.seven_to_nine_rule_mode
        {
            self.invalidate_play_preload();
            self.play.play_media_cache = None;
        }
        self.sync_changed_gamepad_analog_config_from_profile(&before.input);
        if profile_lane_settings_changed(&before.lane, &self.boot.profile_config.lane)
            || before.play.lane_effect != self.boot.profile_config.play.lane_effect
        {
            self.sync_active_play_lane_settings_from_profile(&before.lane, before.play.lane_effect);
        }
        self.sync_realtime_profile_settings();
        self.sync_discord_presence_config();
    }

    pub(super) fn apply_egui_video_config(&mut self, window: &Window) {
        if let Err(error) =
            self.renderer.set_present_mode(config_present_mode(&self.boot.app_config.video))
        {
            tracing::error!(
                error = %format_error_chain(&error),
                "failed to reconfigure renderer present mode"
            );
        }
        if let Err(error) = self
            .renderer
            .set_frame_latency_mode(config_frame_latency_mode(&self.boot.app_config.video))
        {
            tracing::error!(
                error = %format_error_chain(&error),
                "failed to reconfigure renderer frame latency"
            );
        }
        self.renderer.set_internal_resolution_mode(config_internal_resolution_mode(
            &self.boot.app_config.video,
        ));
        let configured_mode = self.boot.app_config.video.mode.clone();
        if self.ui.exclusive_fullscreen_fallback_active
            && configured_mode != WindowMode::ExclusiveFullscreen
        {
            self.ui.exclusive_fullscreen_fallback_active = false;
        }
        let desired_mode = if self.ui.exclusive_fullscreen_fallback_active {
            WindowMode::BorderlessFullscreen
        } else {
            configured_mode.clone()
        };
        if desired_mode == self.ui.applied_window_mode {
            return;
        }
        let monitor = select_monitor(
            &self.boot.app_config.video.monitor_name,
            window.available_monitors(),
            window.primary_monitor(),
        );
        let mut effective_video = self.boot.app_config.video.clone();
        effective_video.mode = desired_mode.clone();
        window.set_fullscreen(fullscreen_from_config(&effective_video, monitor));
        tracing::info!(
            requested_window_mode = ?configured_mode,
            effective_window_mode = ?desired_mode,
            "window mode updated"
        );
        self.ui.applied_window_mode = desired_mode;
    }

    /// リザルト遷移後も鳴らし続けている音声出力を監視し、スケジュール済みの
    /// BGM/キー音がすべて鳴り切ったら出力を解放する。
    pub(super) fn advance_draining_audio(&mut self) {
        let Some(audio) = &self.audio.draining_audio else {
            return;
        };
        if audio.engine.is_idle() {
            tracing::info!("play audio drained after result; releasing output");
            self.audio.draining_audio = None;
        }
    }

    pub(super) fn request_manual_screenshot(&mut self) {
        let path = next_screenshot_path(
            &self.boot.app_config.screenshot.dir,
            &self.boot.app_paths.data_dir,
        );
        let toast_message = if self.boot.app_config.screenshot.copy_to_clipboard {
            self.renderer.request_screenshot_with_clipboard(path.clone());
            tracing::info!(
                path = %path.display(),
                "manual screenshot requested with clipboard copy"
            );
            Localizer::new(self.boot.profile_config.ui.locale()).text("screenshot-saved-clipboard")
        } else {
            self.renderer.request_screenshot(path.clone());
            tracing::info!(path = %path.display(), "manual screenshot requested");
            Localizer::new(self.boot.profile_config.ui.locale()).text("screenshot-saved")
        };
        // トーストは次フレーム以降に出す。撮影フレームでは has_pending_screenshot で隠す。
        self.show_left_overlay_toast(toast_message);
        self.notify_obs_save_recording(crate::obs::ObsRecordingSaveReason::OnScreenshot);
    }

    pub(super) fn show_left_overlay_toast(&mut self, message: impl Into<String>) {
        self.smoke.left_overlay_toast =
            Some(LeftOverlayToast { message: message.into(), shown_at: Instant::now() });
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn flush_pending_screenshots(&mut self, reason: &'static str) {
        if let Err(error) = self.renderer.flush_pending_screenshots() {
            tracing::warn!(%error, reason, "failed to flush pending screenshots");
        }
    }

    pub(super) fn handle_smoke_exit_after_redraw(&mut self, event_loop: &ActiveEventLoop) {
        if self.smoke.smoke_exit_on_result && self.result.finished_play.is_some() {
            self.smoke.smoke_exit_on_result = false;
            tracing::info!("smoke result reached; leaving event loop");
            self.save_configs_for_exit(None, "game exit");
            self.flush_pending_screenshots("smoke result exit");
            event_loop.exit();
            return;
        }

        if let Some(exit_after_result_frames) = self.smoke.smoke_exit_after_result_frames
            && self.result.finished_play.is_some()
        {
            self.smoke.rendered_result_frames = self.smoke.rendered_result_frames.saturating_add(1);
            if self.smoke.rendered_result_frames >= exit_after_result_frames {
                self.smoke.smoke_exit_after_result_frames = None;
                tracing::info!(
                    frames = self.smoke.rendered_result_frames,
                    "smoke result frame count reached; leaving event loop"
                );
                self.save_configs_for_exit(None, "game exit");
                self.flush_pending_screenshots("smoke result frame exit");
                event_loop.exit();
                return;
            }
        }

        if let Some(exit_after_play_frames) = self.smoke.smoke_exit_after_play_frames
            && self.current_scene_kind() == AppSceneKind::Play
        {
            let (frames, should_exit) =
                count_smoke_play_frame(self.smoke.rendered_play_frames, exit_after_play_frames);
            self.smoke.rendered_play_frames = frames;
            if should_exit {
                self.smoke.smoke_exit_after_play_frames = None;
                tracing::info!(
                    frames = self.smoke.rendered_play_frames,
                    "smoke play frame count reached; leaving event loop"
                );
                self.save_configs_for_exit(self.active_hispeed(), "smoke play frame exit");
                self.flush_pending_screenshots("smoke play frame exit");
                event_loop.exit();
                return;
            }
        }

        let Some(exit_after_frames) = self.smoke.smoke_exit_after_frames else {
            return;
        };

        self.smoke.rendered_frames = self.smoke.rendered_frames.saturating_add(1);
        if self.smoke.rendered_frames >= exit_after_frames {
            self.smoke.smoke_exit_after_frames = None;
            tracing::info!(
                frames = self.smoke.rendered_frames,
                "smoke exit frame count reached; leaving event loop"
            );
            self.save_configs_for_exit(self.active_hispeed(), "game exit");
            self.flush_pending_screenshots("smoke frame exit");
            event_loop.exit();
        }
    }

    pub(super) fn active_hispeed(&self) -> Option<f32> {
        self.play
            .active_play
            .as_ref()
            .map(|active| active.running.session.hispeed)
            .or_else(|| self.play.pending_play_start.as_ref().map(|pending| pending.lane.hispeed))
    }

    pub(super) fn start_scene_timers_before_snapshot(
        &mut self,
        select_view: bool,
        result_view: bool,
    ) {
        match self.integrations.last_scene_kind {
            Some(AppSceneKind::Select) if select_view => {}
            _ if select_view => self.restart_select_scene_timers(),
            Some(AppSceneKind::Result) if result_view => {}
            _ if result_view => {
                self.result.result_scene_started_at = Instant::now();
            }
            _ => {}
        }
    }

    pub(super) fn active_lane_state(&self) -> Option<ActiveLaneState> {
        self.play
            .active_play
            .as_ref()
            .map(|active| active_lane_state_for_session(&active.running.session))
            .or_else(|| {
                self.play.pending_play_start.as_ref().map(|pending| ActiveLaneState {
                    lane_cover: pending.lane.lane_cover,
                    lift: pending.lane.lift,
                    hidden_cover: pending.lane.hidden_cover,
                    sudden_enabled: pending.lane.sudden_enabled,
                    lift_enabled: pending.lane.lift_enabled,
                    hidden_enabled: pending.lane.hidden_enabled,
                    hispeed_mode: pending.lane.hispeed_mode,
                    base_hispeed_mode: pending.lane.base_hispeed_mode,
                    floating_policy: pending.lane.floating_policy,
                    normal_hispeed_level: pending.lane.normal_hispeed_level,
                    target_green_number: pending.lane.target_green_number,
                })
            })
    }

    pub(super) fn commit_pending_play_lane_state_to_profile(&mut self) {
        let Some(pending) = &self.play.pending_play_start else {
            return;
        };
        if pending.options.speed_constraint == bmz_core::course::CourseSpeedConstraint::NoSpeed {
            return;
        }
        if pending.lane_actions.is_empty() {
            return;
        }
        self.boot.profile_config.activate_play_mode(pending.play_config_key_mode);
        apply_lane_state_to_profile(
            &mut self.boot.profile_config,
            Some(pending.lane.hispeed),
            Some(ActiveLaneState {
                lane_cover: pending.lane.lane_cover,
                lift: pending.lane.lift,
                hidden_cover: pending.lane.hidden_cover,
                sudden_enabled: pending.lane.sudden_enabled,
                lift_enabled: pending.lane.lift_enabled,
                hidden_enabled: pending.lane.hidden_enabled,
                hispeed_mode: pending.lane.hispeed_mode,
                base_hispeed_mode: pending.lane.base_hispeed_mode,
                floating_policy: pending.lane.floating_policy,
                normal_hispeed_level: pending.lane.normal_hispeed_level,
                target_green_number: pending.lane.target_green_number,
            }),
        );
        self.boot.profile_config.updated_at = now_unix_seconds();
    }

    pub(super) fn commit_active_play_lane_state_to_profile(&mut self) -> bool {
        if active_course_speed_locked(self.play.active_course.as_ref()) {
            return true;
        }
        let Some(active_play) = &self.play.active_play else {
            return false;
        };
        let session = &active_play.running.session;
        self.boot.profile_config.activate_play_mode(session.play_config_key_mode);
        apply_lane_state_to_profile(
            &mut self.boot.profile_config,
            Some(session.hispeed),
            Some(active_lane_state_for_session(session)),
        );
        self.boot.profile_config.updated_at = now_unix_seconds();
        true
    }

    pub(super) fn save_current_play_options(&mut self, hispeed: Option<f32>, reason: &'static str) {
        let key_mode = self
            .play
            .active_play
            .as_ref()
            .map(|active| active.running.session.play_config_key_mode)
            .or_else(|| {
                self.play.pending_play_start.as_ref().map(|pending| pending.play_config_key_mode)
            })
            .or_else(|| self.selected_play_mode())
            .unwrap_or(KeyMode::K7);
        self.boot.profile_config.activate_play_mode(key_mode);
        let (hispeed, lane_state) = lane_state_for_profile_save(
            active_course_speed_locked(self.play.active_course.as_ref()),
            hispeed,
            self.active_lane_state(),
        );
        let options = self.current_select_play_options();
        self.sync_profile_visual_offset_from_active_play();
        apply_current_play_options_to_profile(
            &mut self.boot.profile_config,
            hispeed,
            lane_state,
            options,
            now_unix_seconds(),
        );
        self.boot.profile_config.sync_active_play_mode();
        if let Err(error) =
            save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
        {
            tracing::error!(%error, reason, "failed to save profile play options");
        } else {
            tracing::info!(reason, "saved profile play options");
        }
    }

    pub(super) fn save_configs_for_exit(&mut self, hispeed: Option<f32>, reason: &'static str) {
        if self.integrations.exit_configs_saved {
            return;
        }
        self.save_current_play_options(hispeed, reason);
        if let Err(error) = save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config)
        {
            tracing::error!(%error, reason, "failed to save app config on exit");
        } else {
            tracing::info!(reason, "saved app config on exit");
        }
        self.integrations.exit_configs_saved = true;
    }
}

fn should_arm_select_scene_timers(
    select_view: bool,
    timer_armed: bool,
    render_status: Option<RenderSurfaceStatus>,
) -> bool {
    select_view && !timer_armed && render_status == Some(RenderSurfaceStatus::Rendered)
}

#[cfg(test)]
mod select_scene_timer_tests {
    use super::{RenderSurfaceStatus, should_arm_select_scene_timers};

    #[test]
    fn select_timer_arms_only_after_first_rendered_surface() {
        assert!(!should_arm_select_scene_timers(
            true,
            false,
            Some(RenderSurfaceStatus::Reconfigured)
        ));
        assert!(!should_arm_select_scene_timers(true, false, Some(RenderSurfaceStatus::TimedOut)));
        assert!(should_arm_select_scene_timers(true, false, Some(RenderSurfaceStatus::Rendered)));
        assert!(!should_arm_select_scene_timers(true, true, Some(RenderSurfaceStatus::Rendered)));
        assert!(!should_arm_select_scene_timers(false, false, Some(RenderSurfaceStatus::Rendered)));
    }
}
