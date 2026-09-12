use super::*;

impl WinitApp {
    pub(super) fn poll_play_preload(&mut self) {
        if self.play.play_ending.as_ref().is_some_and(|ending| {
            matches!(
                ending.completion,
                PlayEndingCompletion::Select | PlayEndingCompletion::ViewerExit
            )
        }) {
            return;
        }
        // 1) preload worker からの結果を受け取り (Decide 演出中でも受信して退避する)。
        if let Some(pending) = &self.play.pending_play_preload {
            match pending.rx.try_recv() {
                Ok(result) => {
                    self.play.pending_play_preload = None;
                    if result.generation != self.play.play_preload_generation {
                        tracing::debug!(
                            chart_id = result.chart_id,
                            generation = result.generation,
                            current_generation = self.play.play_preload_generation,
                            "discarding stale play preload result"
                        );
                        if self.play.pending_play_start.is_some() {
                            tracing::warn!(
                                chart_id = result.chart_id,
                                generation = result.generation,
                                current_generation = self.play.play_preload_generation,
                                "aborting pending play start after stale preload result"
                            );
                            self.abort_pending_play_start();
                            return;
                        }
                    } else {
                        match result.result {
                            Ok(prepared) => {
                                tracing::info!(
                                    chart_id = result.chart_id,
                                    generation = result.generation,
                                    "play preload ready (buffered)"
                                );
                                self.play.preloaded_play_session = Some(prepared);
                            }
                            Err(error) => {
                                // preload 全体の失敗は譜面パース不能など再生不能なケースのみ
                                // (個別音源の欠落は load_chart_samples が warning で続行する)。
                                // Play 画面へ入場済みなら選曲へ戻す。中間リザルト中の
                                // course 次曲先読みは失敗を保持し、退出時に安全に中断する。
                                tracing::error!(
                                    chart_id = result.chart_id,
                                    error = %error,
                                    "play preload failed"
                                );
                                if let Some(launch) =
                                    self.play.pending_course_stage_launch.as_mut().filter(
                                        |launch| {
                                            launch.chart_id == result.chart_id
                                                && launch.preload_generation == result.generation
                                        },
                                    )
                                {
                                    launch.preload_error = Some(error.clone());
                                }
                                if self.play.pending_play_start.is_some() {
                                    self.abort_pending_play_start();
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    let pending_chart_id = pending.chart_id;
                    let pending_generation = pending.generation;
                    tracing::warn!(
                        chart_id = pending_chart_id,
                        generation = pending_generation,
                        "play preload worker disconnected"
                    );
                    self.play.pending_play_preload = None;
                    if let Some(launch) =
                        self.play.pending_course_stage_launch.as_mut().filter(|launch| {
                            launch.chart_id == pending_chart_id
                                && launch.preload_generation == pending_generation
                        })
                    {
                        launch.preload_error = Some("play preload worker disconnected".to_string());
                    }
                    if self.play.pending_play_start.is_some() {
                        self.abort_pending_play_start();
                        return;
                    }
                }
            }
        }

        self.publish_prepared_play_chart();
        // Course開始時の全譜面metricsは、ここで公開された先頭譜面を再利用して
        // background計算を始める。preloaded sessionをactiveへmoveする前に行う。
        self.poll_course_metrics();

        // 2) Play 入場が確定 (pending_play_start) しており、バッファに preload があれば install。
        if self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config)
        {
            self.refresh_practice_preview_snapshot();
            return;
        }
        let Some(play_start) = self.play.pending_play_start.as_ref() else {
            return;
        };
        let Some(prepared) = self.play.preloaded_play_session.take() else {
            return;
        };
        let chart_id = play_start.chart_id;
        let start_options = play_start.options.clone();
        let preload_matches = preloaded_matches_start(&prepared, chart_id, &start_options);
        if !preload_matches
            && self.play.pending_course_stage_launch.as_ref().is_some_and(|launch| {
                launch.chart_id == chart_id
                    && launch.preload_generation == self.play.play_preload_generation
            })
        {
            tracing::error!(chart_id, "discarding mismatched course next-stage preload");
            self.abort_pending_play_start();
            return;
        }
        let opened = if preload_matches {
            let prepared =
                prepare_winit_play_session_from_preloaded(&self.boot.profile_config, prepared);
            self.open_prepared_winit_play_session(prepared)
        } else {
            tracing::warn!(chart_id, "discarding mismatched play preload");
            let app_config = self.play_session_app_config();
            prepare_play_session_for_chart_with_winit_input(
                &self.boot.library_db,
                &app_config,
                &self.boot.profile_config,
                chart_id,
                start_options,
            )
            .and_then(|prepared| self.open_prepared_winit_play_session(prepared))
        };
        match opened {
            Ok(active_play) => {
                tracing::info!(chart_id, "play preload installed");
                self.install_active_play(chart_id, active_play);
                if self.play.pending_course_stage_launch.as_ref().is_some_and(|launch| {
                    launch.chart_id == chart_id
                        && launch.preload_generation == self.play.play_preload_generation
                }) {
                    self.play.pending_course_stage_launch = None;
                }
                // スキン宣言のロード演出時間を既に超えていれば、同一フレーム内で
                // READY を開始して op 80→81 切り替えと timer 40 発火を揃える
                // (次フレームの consume_active_play まで待つと 1 フレーム
                // 曲名表示が途切れる)。
                self.maybe_start_ready_phase();
            }
            Err(error) => {
                tracing::error!(chart_id, %error, "failed to open preloaded play audio");
                self.abort_pending_play_start();
            }
        }
    }

    /// 変換済み譜面を WAV 完了前に Play skin と BGA loader へ公開する。
    fn publish_prepared_play_chart(&mut self) {
        let chart_id = self
            .play
            .pending_play_preload
            .as_ref()
            .map(|pending| pending.chart_id)
            .or_else(|| {
                self.play.preloaded_play_session.as_ref().map(|preloaded| preloaded.chart_id)
            })
            .or_else(|| self.play.pending_play_start.as_ref().map(|pending| pending.chart_id));
        let Some(chart_id) = chart_id else {
            return;
        };
        let Some(prepared) = self.play_preload_prepared_chart(chart_id) else {
            return;
        };

        // BMP/BGA は WAV worker と同じ変換済み chart の manifest から開始する。
        // assets=None は begin_unresolved 後、まだ worker を起動していない状態を表す。
        if self.play.bga_preload.chart_id == Some(chart_id)
            && self.play.bga_preload.assets.is_none()
        {
            let frames = self.start_chart_bga_texture_load_for_chart(chart_id, &prepared.chart);
            if !frames.is_empty() {
                self.play.bga_preload.frames = frames;
            }
        }

        let pending_options = self
            .play
            .pending_play_start
            .as_ref()
            .filter(|pending| pending.chart_id == chart_id && !pending.prepared_chart_applied)
            .map(|pending| pending.options.clone());
        let Some(options) = pending_options else {
            return;
        };
        let battle_presentation = uses_battle_presentation(
            self.key_mode_for_chart(chart_id),
            prepared.chart.metadata.key_mode,
            options.session_mode,
            options.battle_target.is_some(),
        );
        let Some(snapshot) = &mut self.play.last_play_snapshot else {
            return;
        };
        apply_prepared_chart_to_render_snapshot(
            snapshot,
            &prepared.chart,
            &prepared.render_snapshot_cache,
            battle_presentation,
        );
        snapshot.skin_attempt.merge_known(prepared.skin_attempt);
        apply_play_arrange_to_snapshot(snapshot, &prepared.applied_arrange);
        snapshot.target = options.target.as_string();
        snapshot.resolved_target_name = options
            .rival_name
            .clone()
            .or_else(|| options.resolved_target.as_ref().map(|target| target.name.clone()));
        snapshot.target_ex_score = options
            .resolved_target
            .as_ref()
            .map(|target| target.ex_score)
            .or_else(|| options.target.target_ex_score(snapshot.total_notes));

        if let Some(pending) =
            self.play.pending_play_start.as_mut().filter(|pending| pending.chart_id == chart_id)
        {
            pending.lane.sync_chart_bpm(snapshot, pending.options.hs_fix);
            pending.prepared_chart_applied = true;
        }
        tracing::info!(
            chart_id,
            total_notes = snapshot.total_notes,
            judge_graph_seconds = snapshot.judge_graph_density.len(),
            bpm_graph_segments = snapshot.bpm_graph_segments.len(),
            "published prepared chart to play skin before media completion"
        );
    }

    pub(super) fn abort_pending_play_start(&mut self) {
        if !self.commit_active_play_lane_state_to_profile() {
            self.commit_pending_play_lane_state_to_profile();
        }
        if let Some(active_play) = &mut self.play.active_play
            && let Err(error) = active_play.running.pause_audio()
        {
            tracing::warn!(%error, "failed to pause audio while aborting play start");
        }
        self.invalidate_play_preload();
        self.play.pending_play_start = None;
        self.play.active_play = None;
        self.play.play_ending = None;
        self.play.play_ready_sound_started_at = None;
        self.play.play_ready_last_control_hold_at = None;
        self.play.play_option_input = None;
        self.clear_play_control_holds();
        self.stop_system_sound(crate::system_sound::SoundType::PlayReady);
        self.play.decide_sound_stopped_for_chart_start = true;
        if !self.viewer_mode {
            self.clear_play_meta_image_state();
            self.play.last_play_snapshot = None;
        }
        // An audio-open / audio-start failure bounces the user back to the
        // select screen.  If they were in a course at the time, the course
        // session is no longer valid — otherwise the next chart they pick
        // would be treated as the next entry of a stale course (route
        // through advance_course_after_finish with mismatched chart_id).
        self.clear_active_course_state();
        self.select.autoplay_folder = None;
        self.play.play_media_cache = None;
        // Result から Retry した直後は、前回結果の score DB 更新を
        // SelectItem がまだ取り込んでいない。曲開始前退出でも必ず再取得する。
        if self.viewer_mode {
            self.viewer_waiting = true;
            self.stop_select_preview();
            if let Some(manager) = &self.audio.system_sound {
                manager.stop_all_bgm();
            }
            return;
        }
        self.discard_cli_play_on_select();
        self.reload_select_items();
        self.reload_skin_for_scene_entry(SkinKind::Select);
        self.restart_select_scene_timers();
    }

    /// Clears any active course session and the cached finished-course
    /// summary.  Call from any path that returns to the select screen
    /// without completing the course naturally.
    pub(super) fn clear_active_course_state(&mut self) {
        self.clear_pending_course_metrics();
        if self.play.pending_course_stage_launch.is_some() {
            self.invalidate_play_preload();
        }
        if self.play.active_course.is_some() || self.result.finished_course.is_some() {
            tracing::info!(
                had_active = self.play.active_course.is_some(),
                had_finished = self.result.finished_course.is_some(),
                "clearing course session state (abort or cancel)"
            );
        }
        self.play.active_course = None;
        self.clear_finished_course();
    }

    pub(super) fn play_start_options(&self) -> PlayStartOptions {
        // beatoraja assigns a 24-bit seed even to NORMAL/MIRROR. Generate both
        // sides here so preload, retry, replay and IR all observe one stable pair.
        let option_seeds = crate::random_option_seed::RandomOptionSeeds::fresh(true);
        let random_trainer_seed = self.select.random_trainer.arrange_seed(option_seeds.p1);
        let seed = self.boot.profile_config.cli_seed();
        PlayStartOptions {
            score_save_disabled: self.boot.profile_config.cli_auto_scratch(),
            session_mode: self.select.session_mode,
            autoplay: self.select.session_mode.primary_autoplay(),
            assist: self.boot.profile_config.play.assist,
            gauge: Some(self.select.gauge_option),
            gauge_auto_shift: self.select.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.select.bottom_shiftable_gauge_option,
            arrange: self.select.arrange_option,
            arrange_2p: self.select.arrange_option_2p,
            double_option: self.select.double_option,
            hs_fix: self.select.hs_fix_option,
            target: self.select.target_option,
            rival_name: self.select.select_ir.active_rival_display_name().map(str::to_string),
            arrange_seed: Some(
                seed.map_or(i64::from(option_seeds.p1.value()), |s| (s & 0xff_ffff) as i64),
            ),
            arrange_seed_2p: seed
                .map(|s| ((s >> 24) & 0xff_ffff) as i64)
                .or_else(|| option_seeds.p2.map(|seed| i64::from(seed.value()))),
            random_trainer_seed,
            bms_random_seed: Some(
                seed.unwrap_or_else(crate::random_option_seed::fresh_bms_random_seed),
            ),
            key_mode_conversion: self.boot.profile_config.play.key_mode_conversion,
            seven_to_nine_pattern: self.boot.profile_config.play.seven_to_nine_pattern,
            seven_to_nine_type: self.boot.profile_config.play.seven_to_nine_type,
            seven_to_nine_rule_mode: self.boot.profile_config.play.seven_to_nine_rule_mode,
            ..Default::default()
        }
    }

    pub(super) fn refresh_play_target_from_source(&mut self) {
        if self
            .play
            .active_play
            .as_ref()
            .map(|active| active.running.resolved_target.is_some())
            .unwrap_or_else(|| {
                self.play
                    .preloaded_play_session
                    .as_ref()
                    .is_some_and(|preloaded| preloaded.session_options.resolved_target.is_some())
            })
        {
            return;
        }
        let source = self
            .play
            .active_play
            .as_ref()
            .map(|active| {
                (
                    active.running.score_key,
                    active.running.target_option,
                    active.running.best_ex_score,
                )
            })
            .or_else(|| {
                self.play.preloaded_play_session.as_ref().map(|preloaded| {
                    (
                        preloaded.preloaded.score_key,
                        preloaded.session_options.target,
                        self.boot
                            .score_db
                            .best_ex_score(preloaded.preloaded.score_key)
                            .ok()
                            .flatten(),
                    )
                })
            });
        let Some((score_key, target, local_best_ex_score)) = source else {
            return;
        };
        if !target.uses_ir_ranking() {
            return;
        }
        let context = select_ir_cache_context(
            self.boot.profile_config.play.ln_mode_policy,
            score_key.ln_policy,
            score_key.double_option,
            score_key.rule_mode,
        );
        self.select.select_ir.update(
            &self.boot.profile_config.ir,
            &self.boot.profile_paths.root_dir,
            &context,
            score_key.ln_policy,
            score_key.double_option,
            score_key.rule_mode,
            Some(score_key.chart_sha256),
        );
        let resolved = self.select.select_ir.resolved_target_for(
            &self.boot.profile_config.ir,
            Some(score_key.chart_sha256),
            target,
            local_best_ex_score,
        );
        let Some(resolved) = resolved else {
            return;
        };
        if let Some(preloaded) = &mut self.play.preloaded_play_session
            && preloaded.preloaded.score_key == score_key
            && preloaded.session_options.target == target
        {
            preloaded.session_options.resolved_target = Some(resolved.clone());
        }
        if let Some(pending) = &mut self.play.pending_play_start
            && pending.options.target == target
        {
            pending.options.resolved_target = Some(resolved.clone());
        }
        if let Some(snapshot) = &mut self.play.last_play_snapshot {
            snapshot.target_ex_score = Some(resolved.ex_score);
            // 選択ライバル名はIRターゲットより優先する。
            let rival_name = self
                .play
                .active_play
                .as_ref()
                .and_then(|active| active.running.rival_name.clone())
                .or_else(|| {
                    self.play
                        .pending_play_start
                        .as_ref()
                        .and_then(|pending| pending.options.rival_name.clone())
                });
            snapshot.resolved_target_name = rival_name.or_else(|| Some(resolved.name.clone()));
        }
        if let Some(active) = &mut self.play.active_play
            && active.running.score_key == score_key
            && active.running.target_option == target
        {
            active.running.target_ex_score = Some(resolved.ex_score);
            active.running.target_name =
                active.running.rival_name.clone().unwrap_or_else(|| resolved.name.clone());
            active.running.resolved_target = Some(resolved);
        }
    }
}
