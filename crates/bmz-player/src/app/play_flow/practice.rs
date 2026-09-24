use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PracticeGamepadAction {
    Move(bool),
    Adjust(bool),
    Ignore,
}

pub(super) fn practice_gamepad_action(
    device: DeviceId,
    control: &PhysicalControl,
    synthesized_analog_axis: bool,
    input: Option<&PlayOptionInput>,
) -> PracticeGamepadAction {
    let Some(input) = input else { return PracticeGamepadAction::Ignore };
    let Some(resolution) = input.resolve_entry(device, control) else {
        return match control {
            PhysicalControl::GamepadButton(button)
                if matches!(button.as_str(), "DPadUp" | "DPadNorth") =>
            {
                PracticeGamepadAction::Move(false)
            }
            PhysicalControl::GamepadButton(button)
                if matches!(button.as_str(), "DPadDown" | "DPadSouth") =>
            {
                PracticeGamepadAction::Move(true)
            }
            _ => PracticeGamepadAction::Ignore,
        };
    };
    if matches!(resolution.lane, Lane::Scratch | Lane::Scratch2) {
        if synthesized_analog_axis {
            return PracticeGamepadAction::Ignore;
        }
        return match resolution.scratch_direction {
            Some(ScratchDirection::Up) => PracticeGamepadAction::Move(false),
            Some(ScratchDirection::Down) => PracticeGamepadAction::Move(true),
            None => PracticeGamepadAction::Ignore,
        };
    }

    let lane_index = resolution.lane.index();
    let key_index = if matches!(input.key_mode, KeyMode::K10 | KeyMode::K14) && lane_index >= 8 {
        lane_index - 7
    } else {
        lane_index
    };
    PracticeGamepadAction::Adjust(key_index % 2 == 1)
}

pub(super) fn practice_analog_cursor_delta(
    device: DeviceId,
    axis: &str,
    ticks: i32,
    input: Option<&PlayOptionInput>,
) -> Option<i32> {
    if ticks == 0 {
        return None;
    }
    let input = input?;
    let control =
        PhysicalControl::GamepadButton(format!("{}{}", axis, if ticks > 0 { "+" } else { "-" }));
    let resolution = input.resolve_entry(device, &control)?;
    if !matches!(resolution.lane, Lane::Scratch | Lane::Scratch2) {
        return None;
    }
    match resolution.scratch_direction {
        Some(ScratchDirection::Up) => Some(-ticks.abs()),
        Some(ScratchDirection::Down) => Some(ticks.abs()),
        None => None,
    }
}

pub(super) fn apply_session_mode_start_policy(options: &mut PlayStartOptions) {
    if options.session_mode == SessionMode::AutoplayBattle {
        options.autoplay = true;
    }
    if let Some(target) = options.battle_target.as_ref() {
        options.rival_name = Some(target.player_name.clone());
        options.resolved_target =
            Some(ResolvedTarget { name: target.player_name.clone(), ex_score: target.ex_score });
    }
}

impl WinitApp {
    pub(super) fn route_practice_gamepad_control(
        &mut self,
        device: DeviceId,
        button: &str,
        pressed: bool,
        synthesized_analog_axis: bool,
    ) -> bool {
        let is_config = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        if !is_config {
            return false;
        }
        if self.play.play_ending.is_some() {
            return true;
        }
        if pressed && self.play.play_e2_held && self.play.play_e3_held {
            self.stop_play_like_escape("E2+E3 pressed in practice configuration");
            return true;
        }
        if self.play.play_e1_held || self.play.play_e2_held {
            return false;
        }
        if !pressed {
            return true;
        }

        let control = PhysicalControl::GamepadButton(button.to_string());
        let action = practice_gamepad_action(
            device,
            &control,
            synthesized_analog_axis,
            self.play.play_option_input.as_ref(),
        );

        match action {
            PracticeGamepadAction::Move(forward) => {
                if let Some(practice) = &mut self.play.practice_session {
                    crate::screens::practice::move_practice_cursor(
                        &mut practice.cursor,
                        practice.is_double,
                        forward,
                    );
                }
            }
            PracticeGamepadAction::Adjust(increment) => {
                let cursor_action = self
                    .play
                    .practice_session
                    .as_mut()
                    .map(|practice| {
                        crate::screens::practice::apply_practice_cursor_horizontal(
                            &mut practice.property,
                            practice.cursor,
                            practice.is_double,
                            increment,
                            practice.max_end_time_ms,
                        )
                    })
                    .unwrap_or(crate::screens::practice::PracticeCursorAction::None);
                match cursor_action {
                    crate::screens::practice::PracticeCursorAction::None => {
                        self.refresh_practice_preview_snapshot();
                    }
                    crate::screens::practice::PracticeCursorAction::Start => {
                        self.start_practice_round();
                    }
                    crate::screens::practice::PracticeCursorAction::Leave => {
                        self.stop_play_like_escape("practice gamepad leave requested");
                    }
                }
            }
            PracticeGamepadAction::Ignore => {}
        }
        true
    }

    pub(super) fn start_chart(&mut self, chart_id: i64) {
        self.select.autoplay_folder = None;
        let mut options = self.play_start_options();
        if options.session_mode.is_practice() {
            self.enter_practice(chart_id, PracticeCliOverrides::default());
            return;
        }
        if !self.prepare_session_mode_or_show_error(chart_id, &mut options) {
            return;
        }
        self.begin_decide_for_chart(chart_id, options);
    }

    pub(super) fn prepare_session_mode_or_show_error(
        &mut self,
        chart_id: i64,
        options: &mut PlayStartOptions,
    ) -> bool {
        let has_battle_target = options.battle_target.is_some();
        if let Err(error) = self.prepare_session_mode_for_chart(chart_id, options) {
            let error_text = format_error_chain(&error);
            tracing::warn!(
                chart_id,
                session_mode = options.session_mode.as_str(),
                error = %error_text,
                "session mode is unavailable for selected chart"
            );
            if has_battle_target {
                self.show_ir_battle_error(&error_text);
            } else {
                let text = Localizer::new(self.boot.profile_config.ui.locale());
                self.show_left_overlay_toast(text.text("toast-session-mode-unavailable"));
            }
            false
        } else {
            true
        }
    }

    pub(super) fn prepare_session_mode_for_chart(
        &self,
        chart_id: i64,
        options: &mut PlayStartOptions,
    ) -> Result<()> {
        apply_session_mode_start_policy(options);
        let Some(target) = options.battle_target.as_mut() else {
            return Ok(());
        };
        let chart = crate::screens::play_session::load_source_chart_for_chart(
            &self.boot.library_db,
            chart_id,
            None,
        )?;
        if let crate::screens::play_start::BattleTargetPlayback::Replay(replay) =
            &mut target.playback
        {
            let ln_policy = crate::ln_policy::score_ln_policy_for_chart(
                self.boot.profile_config.play.ln_mode_policy,
                &chart,
            );
            if replay.chart_sha256_bytes()? != chart.identity.file_sha256 {
                anyhow::bail!("battle replay chart hash does not match selected chart");
            }
            if !replay.ln_policy.is_empty()
                && crate::ln_policy::LnScorePolicy::from_str_opt(&replay.ln_policy)
                    != Some(ln_policy)
            {
                anyhow::bail!("battle replay long note policy does not match selected chart");
            }
            crate::screens::play_start::normalize_battle_replay_for_key_mode(
                replay,
                chart.metadata.key_mode,
            )?;
        }
        Ok(())
    }

    pub(super) fn enter_practice(&mut self, chart_id: i64, cli: PracticeCliOverrides) {
        self.enter_practice_with_battle_target(chart_id, cli, None);
    }

    pub(super) fn enter_practice_with_battle_target(
        &mut self,
        chart_id: i64,
        cli: PracticeCliOverrides,
        battle_target: Option<crate::screens::play_start::BattleTarget>,
    ) {
        let defaults = match self.load_practice_defaults_for_chart(chart_id, &cli) {
            Ok(defaults) => defaults,
            Err(error) => {
                tracing::error!(%error, chart_id, "failed to load practice configuration");
                return;
            }
        };
        let practice_session = PracticeSession {
            chart_id,
            chart_title: defaults.title,
            chart_sha256: defaults.sha256,
            property: defaults.property,
            phase: PracticePhase::Config,
            max_end_time_ms: defaults.max_end_time_ms,
            last_graph: defaults.graph,
            graph_start_time_ms: 0,
            is_double: defaults.is_double,
            cursor: 0,
            preview_time_ms: None,
            battle_target: battle_target.clone(),
        };
        self.play.practice_session = None;
        self.result.finished_play = None;
        self.play.play_ending = None;
        self.result.result_exit = None;
        self.clear_active_course_state();

        let preload_options = PlayStartOptions {
            session_mode: SessionMode::Practice,
            autoplay: false,
            gauge: Some(self.select.gauge_option),
            gauge_auto_shift: self.select.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.select.bottom_shiftable_gauge_option,
            arrange: ArrangeOption::Normal,
            battle_target,
            ..Default::default()
        };
        let snapshot = self.decide_snapshot_for_chart(chart_id);
        self.begin_decide_for_chart_with_snapshot(
            chart_id,
            preload_options,
            snapshot,
            None,
            None,
            DecideLaunch::practice(practice_session),
        );
        tracing::info!(chart_id, "practice decide screen ready");
    }

    pub(super) fn load_practice_defaults_for_chart(
        &self,
        chart_id: i64,
        cli: &PracticeCliOverrides,
    ) -> Result<PracticeChartDefaults> {
        let Some(path) = self.boot.library_db.primary_chart_file_path(chart_id)? else {
            anyhow::bail!("chart file not found for chart id {chart_id}");
        };
        let import = bmz_chart::import::import_bms_chart(Path::new(&path), None, true)
            .with_context(|| format!("import chart for practice defaults: {path}"))?;
        let property = load_practice_property(
            &self.boot.profile_paths,
            &import.chart.identity.file_sha256,
            &import.chart,
            self.select.gauge_option,
            self.boot.profile_config.play.rule_mode,
            cli,
        )?;
        let title = if import.chart.metadata.title.is_empty() {
            format!("chart {chart_id}")
        } else {
            import.chart.metadata.title.clone()
        };
        let graph = crate::screens::result_model::ResultGraphCollector::default()
            .snapshot_for_result_parts(&import.chart, &Default::default(), None);
        let max_end_time_ms = crate::screens::practice::default_end_time_ms(&import.chart);
        let is_double = matches!(import.chart.metadata.key_mode, KeyMode::K10 | KeyMode::K14);
        Ok(PracticeChartDefaults {
            property,
            title,
            sha256: import.chart.identity.file_sha256,
            graph: std::sync::Arc::new(graph),
            max_end_time_ms,
            is_double,
        })
    }

    pub(super) fn practice_media_ready(&self) -> bool {
        self.play.practice_session.is_some()
            && self.play.preloaded_play_session.is_some()
            && self.play.pending_play_preload.is_none()
            && self.play.play_ending.is_none()
    }

    pub(super) fn begin_practice_leave_transition(&mut self, reason: &'static str) {
        if self.play.play_ending.is_some() {
            return;
        }
        if let Some(practice) = &self.play.practice_session
            && let Err(error) = save_practice_property(
                &self.boot.profile_paths,
                &practice.chart_sha256,
                &practice.property,
            )
        {
            tracing::warn!(%error, "failed to save practice property before exit");
        }
        if let Some(active_play) = &mut self.play.active_play
            && let Err(error) = active_play.running.pause_audio()
        {
            tracing::warn!(%error, "failed to pause practice audio during exit");
        }
        self.clear_play_control_holds();
        self.stop_system_sound(crate::system_sound::SoundType::PlayReady);
        self.notify_obs_play_ended();
        let now = Instant::now();
        self.play.play_ending = Some(practice_leave_ending(now));
        self.update_play_ending_snapshot();
        tracing::info!(reason, "started practice fadeout to select");
    }

    pub(super) fn leave_practice(&mut self) {
        if let Some(practice) = &self.play.practice_session {
            let _ = save_practice_property(
                &self.boot.profile_paths,
                &practice.chart_sha256,
                &practice.property,
            );
        }
        if !self.commit_active_play_lane_state_to_profile() {
            self.commit_pending_play_lane_state_to_profile();
        }
        self.play.practice_session = None;
        self.select.autoplay_folder = None;
        self.play.practice_chart_zero_time = None;
        self.play.active_play = None;
        self.play.pending_play_start = None;
        self.play.preloaded_play_session = None;
        self.invalidate_play_preload();
        self.play.play_option_input = None;
        self.clear_play_control_holds();
        self.play.play_media_cache = None;
        self.play.play_ending = None;
        self.result.finished_play = None;
        self.play.play_ready_sound_started_at = None;
        self.play.play_ready_last_control_hold_at = None;
        self.audio.draining_audio = None;
        self.clear_play_meta_image_state();
        self.play.last_play_snapshot = None;
        self.discard_cli_play_on_select();
        self.reload_select_items();
        self.reload_skin_for_scene_entry(SkinKind::Select);
        self.restart_select_scene_timers();
        tracing::info!("left practice mode");
    }

    pub(super) fn start_practice_round(&mut self) {
        if !self.practice_media_ready() {
            tracing::debug!("practice start ignored: media not ready");
            return;
        }
        self.play.presentation.enter();
        let (chart_id, property, chart_sha256) = {
            let Some(practice) = &mut self.play.practice_session else {
                return;
            };
            if let Some(preloaded) = &self.play.preloaded_play_session {
                clamp_practice_property(&mut practice.property, &preloaded.preloaded.chart);
                practice.max_end_time_ms =
                    crate::screens::practice::default_end_time_ms(&preloaded.preloaded.chart);
            }
            (practice.chart_id, practice.property.clone(), practice.chart_sha256)
        };
        if let Err(error) =
            save_practice_property(&self.boot.profile_paths, &chart_sha256, &property)
        {
            tracing::warn!(%error, "failed to save practice property");
        }
        self.play.practice_chart_zero_time =
            Some(practice_chart_zero_time(&property, self.play_skin_playstart_offset()));
        if let Some(practice) = &mut self.play.practice_session {
            practice.phase = PracticePhase::Playing;
        }

        let preloaded = match self.play.preloaded_play_session.as_ref() {
            Some(preloaded) => preloaded.clone_loaded_resources(),
            None => {
                tracing::error!(chart_id, "practice start without preloaded session");
                self.play.practice_chart_zero_time = None;
                if let Some(practice) = &mut self.play.practice_session {
                    practice.phase = PracticePhase::Config;
                }
                return;
            }
        };

        let prepared_winit = prepare_practice_winit_play_session_from_preloaded(
            &self.boot.profile_config,
            &property,
            preloaded,
        );
        // Exclusive output を含め、前ラウンドの出力streamを新しいstreamより先に解放する。
        self.audio.draining_audio = None;
        match self.open_prepared_winit_play_session(prepared_winit) {
            Ok(active_play) => {
                self.install_active_play(chart_id, active_play);
                self.play.pending_play_start = None;
                tracing::info!(chart_id, "practice round started");
            }
            Err(error) => {
                tracing::error!(%error, chart_id, "failed to open practice play session");
                self.play.practice_chart_zero_time = None;
                if let Some(practice) = &mut self.play.practice_session {
                    practice.phase = PracticePhase::Config;
                }
            }
        }
    }

    pub(super) fn finish_practice_round(&mut self) {
        let (chart_id, chart_sha256, property) = {
            let Some(practice) = &self.play.practice_session else {
                return;
            };
            (practice.chart_id, practice.chart_sha256, practice.property.clone())
        };
        if let Err(error) =
            save_practice_property(&self.boot.profile_paths, &chart_sha256, &property)
        {
            tracing::warn!(%error, "failed to save practice property after round");
        }
        self.commit_active_play_lane_state_to_profile();
        if let Some(mut started) = self.play.active_play.take() {
            let graph = started.running.result_graph.snapshot_for_source(&started.running.gameplay);
            if let Some(practice) = &mut self.play.practice_session {
                practice.last_graph = std::sync::Arc::new(graph);
                practice.graph_start_time_ms = property.start_time_ms;
                practice.preview_time_ms = None;
            }
            self.capture_play_media_cache_from_running(chart_id, &mut started.running);
            let mut audio = started.running.audio;
            audio.mark_draining();
            self.audio.draining_audio = Some(audio);
        }
        self.clear_play_control_holds();
        self.play.play_ending = None;
        self.result.finished_play = None;
        self.play.play_ready_sound_started_at = None;
        self.play.play_ready_last_control_hold_at = None;
        self.play.practice_chart_zero_time = None;
        if let Some(practice) = &mut self.play.practice_session {
            practice.phase = PracticePhase::Config;
        }

        let battle_target =
            self.play.practice_session.as_ref().and_then(|practice| practice.battle_target.clone());
        let mut preload_options = PlayStartOptions {
            session_mode: SessionMode::Practice,
            autoplay: false,
            gauge: Some(self.select.gauge_option),
            gauge_auto_shift: self.select.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.select.bottom_shiftable_gauge_option,
            arrange: ArrangeOption::Normal,
            battle_target,
            ..Default::default()
        };
        let key_mode = self.play_skin_key_mode_for_chart(chart_id, &preload_options);
        let play_config_key_mode = self.key_mode_for_chart(chart_id);
        self.boot.profile_config.activate_play_mode(play_config_key_mode);
        self.select.hs_fix_option =
            hs_fix_option_from_profile(self.boot.profile_config.play.hs_fix);
        preload_options.hs_fix = self.select.hs_fix_option;
        let mut session_options = play_session_options_from_start(
            &self.play_session_app_config(),
            preload_options.clone(),
        );
        session_options.play_config_key_mode = Some(play_config_key_mode);
        let mut snapshot = self
            .play
            .last_play_snapshot
            .clone()
            .unwrap_or_else(|| self.decide_snapshot_for_chart(chart_id));
        crate::screens::play_session::apply_placeholder_session_visuals(
            &mut snapshot,
            &self.boot.profile_config,
            key_mode,
            &session_options,
        );
        let mut pending_play_start = PendingPlayStart::from_snapshot(
            chart_id,
            preload_options,
            &snapshot,
            &self.boot.profile_config,
            key_mode,
            play_config_key_mode,
            session_options.gamepad_slots,
        );
        pending_play_start.lane.lane_cover_changing = self.play_lane_value_changing();
        pending_play_start.lane.apply_to_snapshot(&mut snapshot);
        self.play.play_option_input = Some(PlayOptionInput::new(
            key_mode,
            pending_play_start.visual_input.binding.clone(),
            &self.boot.profile_config.input,
            session_options.gamepad_slots,
        ));
        self.play.last_play_snapshot = Some(snapshot);
        self.play.pending_play_start = Some(pending_play_start);
        self.refresh_practice_preview_snapshot();
        tracing::info!(chart_id, "practice round finished; reused resources for configuration");
    }

    /// 設定中の開始位置へ Play skin の譜面表示を移動する。
    ///
    /// preload が保持する変換済み譜面を参照するだけで、音源・画像・動画の
    /// reload は行わない。beatoraja の Practice `LaneRenderer` と同じく
    /// 設定中だけ hispeed 1.0 と秒線/BPMガイドを使う。
    pub(super) fn refresh_practice_preview_snapshot(&mut self) {
        if self.play.play_ending.is_some() {
            return;
        }
        let (chart_id, start_time_ms) = {
            let Some(practice) = self.play.practice_session.as_ref() else {
                return;
            };
            if practice.phase != PracticePhase::Config
                || practice.preview_time_ms == Some(practice.property.start_time_ms)
            {
                return;
            }
            (practice.chart_id, practice.property.start_time_ms)
        };
        let Some(preloaded) = self
            .play
            .preloaded_play_session
            .as_ref()
            .filter(|preloaded| preloaded.chart_id == chart_id)
        else {
            return;
        };

        let mut preview_session = crate::screens::play_session::build_game_session(
            Arc::clone(&preloaded.preloaded.chart),
            &self.boot.profile_config,
            preloaded.session_options.clone(),
        );
        preview_session.hispeed = 1.0;
        let chart_now = TimeUs(i64::from(start_time_ms) * 1_000);
        let mut snapshot = build_render_snapshot_with_target_and_bga_frames_cached(
            &preview_session,
            chart_now,
            &[],
            None,
            None,
            None,
            &self.play.bga_preload.frames,
            &preloaded.preloaded.render_snapshot_cache,
        );
        snapshot.practice_mode = true;
        snapshot.practice_preview = true;
        snapshot.skin_attempt.merge_known(preloaded.preloaded.skin_attempt);
        apply_play_arrange_to_snapshot(&mut snapshot, &preloaded.preloaded.applied_arrange);
        snapshot.stagefile_background = self.play.play_stagefile_loaded;
        snapshot.stagefile_image_size = self.play.play_stagefile_size;
        snapshot.backbmp_background = self.play.play_backbmp_loaded;
        self.apply_profile_fast_slow_filter(&mut snapshot);
        self.apply_play_table_text(&mut snapshot);
        self.play.last_play_snapshot = Some(snapshot);
        if let Some(practice) = self.play.practice_session.as_mut()
            && practice.chart_id == chart_id
            && practice.phase == PracticePhase::Config
        {
            practice.preview_time_ms = Some(start_time_ms);
        }
    }
}
