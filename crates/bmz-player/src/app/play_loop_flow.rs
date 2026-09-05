use super::*;

impl WinitApp {
    pub(super) fn advance_active_play(&mut self) {
        if self.viewer_mode && self.viewer_waiting {
            if self.stop_play_if_exit_hold_elapsed() {
                self.clear_play_control_holds();
            }
            if self.play.play_ending.is_some() {
                self.update_play_ending_snapshot();
            }
            return;
        }
        if self.viewer_mode && self.viewer_paused && self.play.play_ready_sound_started_at.is_some()
        {
            self.update_viewer_paused_snapshot();
            return;
        }
        self.sync_autoplay_replay_playback_rate();
        self.poll_pending_finished_play();
        if self.play.play_ending.is_some() {
            self.update_play_ending_snapshot();
            return;
        }
        if self.play.pending_play_start.is_some() {
            self.update_pending_play_snapshot_timers();
        }
        if self.stop_play_if_exit_hold_elapsed() {
            self.clear_play_control_holds();
            if self.play.play_ending.is_some() {
                return;
            }
        }
        if self.play.active_play.is_none() {
            return;
        }
        self.maybe_start_ready_phase();
        self.stop_decide_system_sound_after_chart_start();
        if self.play.play_ready_sound_started_at.is_none() {
            self.update_pre_ready_play_state();
            self.update_pending_play_snapshot_timers();
            return;
        }
        let course_titles = self.current_course_titles();
        let course_stage = self.current_course_stage_marker();
        let play_elapsed_time = self.play_elapsed_time();
        let ready_elapsed_time = self.play_ready_animation_elapsed_time();
        let seamless_play_entry = self.play.play_entry_presentation.is_seamless();
        let stagefile_background = self.play.play_stagefile_loaded;
        let stagefile_image_size = self.play.play_stagefile_size;
        let backbmp_background = self.play.play_backbmp_loaded;
        let Some(active_play) = &mut self.play.active_play else {
            return;
        };

        let state_before_advance = active_play.running.session.state;
        let advance_outcome = advance_running_play_session(&mut active_play.running);
        match advance_outcome {
            Ok(frame)
                if !matches!(
                    frame.state,
                    bmz_gameplay::session::PlayState::Finished
                        | bmz_gameplay::session::PlayState::Failed
                ) =>
            {
                let result_settled_at = frame.render_snapshot.time;
                let result_settled = bmz_gameplay::session::result_is_settled(
                    &active_play.running.session,
                    result_settled_at,
                );
                active_play.running.result_graph.record_frame(&frame);
                let guide_se_enabled = active_play.running.session.guide_se_enabled;
                let guide_judgements = frame.judgements.clone();
                let mine_hits = frame.mine_hits.clone();
                let audio_mix = active_play.running.session.audio_mix;
                let mut snapshot = frame.render_snapshot;
                // gameplayと同じ絶対時刻で動画を選び、前snapshot参照による
                // 常時1フレーム分のBGA表示遅延を発生させない。
                crate::video_bga::update_video_bga_frames(
                    &mut self.renderer,
                    &mut active_play.running,
                    snapshot.time,
                );
                self.apply_profile_fast_slow_filter(&mut snapshot);
                snapshot.play_elapsed_time = play_elapsed_time;
                snapshot.ready_elapsed_time = ready_elapsed_time;
                snapshot.seamless_play_entry = seamless_play_entry;
                snapshot.stagefile_background = stagefile_background;
                snapshot.stagefile_image_size = stagefile_image_size;
                snapshot.backbmp_background = backbmp_background;
                snapshot.course_stage = course_stage;
                snapshot.course_titles = course_titles.clone();
                self.apply_play_table_text(&mut snapshot);
                if let Some(active_play) = &self.play.active_play {
                    crate::screens::play_snapshot::refresh_play_skin_visuals(
                        &mut snapshot,
                        &active_play.running.session,
                    );
                }
                self.play.last_play_snapshot = Some(snapshot);
                self.play_guide_se_for_judgements(guide_se_enabled, &guide_judgements);
                self.play_landmine_se(&mine_hits, audio_mix);
                if result_settled {
                    self.finalize_settled_play_result_once(result_settled_at);
                }
            }
            Ok(frame) => {
                let should_play_retire_sound = should_play_retire_sound_for_failed_transition(
                    state_before_advance,
                    frame.state,
                );
                active_play.running.result_graph.record_frame(&frame);
                let guide_se_enabled = active_play.running.session.guide_se_enabled;
                let guide_judgements = frame.judgements.clone();
                if self
                    .play
                    .practice_session
                    .as_ref()
                    .is_some_and(|practice| practice.phase == PracticePhase::Playing)
                {
                    let failed = frame.state == bmz_gameplay::session::PlayState::Failed;
                    let mine_hits = frame.mine_hits.clone();
                    let audio_mix = active_play.running.session.audio_mix;
                    let mut snapshot = frame.render_snapshot;
                    snapshot.play_elapsed_time = play_elapsed_time;
                    snapshot.ready_elapsed_time = ready_elapsed_time;
                    snapshot.seamless_play_entry = seamless_play_entry;
                    snapshot.stagefile_background = stagefile_background;
                    snapshot.stagefile_image_size = stagefile_image_size;
                    snapshot.backbmp_background = backbmp_background;
                    snapshot.course_stage = course_stage;
                    snapshot.course_titles = course_titles.clone();
                    crate::screens::play_snapshot::refresh_play_skin_visuals(
                        &mut snapshot,
                        &active_play.running.session,
                    );
                    if let Err(error) = active_play.running.pause_audio() {
                        tracing::warn!(%error, "failed to stop practice audio at round end");
                    }
                    self.apply_profile_fast_slow_filter(&mut snapshot);
                    self.apply_play_table_text(&mut snapshot);
                    self.play.last_play_snapshot = Some(snapshot);
                    self.play_guide_se_for_judgements(guide_se_enabled, &guide_judgements);
                    if should_play_retire_sound {
                        self.play_system_sound(crate::system_sound::SoundType::PlayStop);
                    }
                    self.play_landmine_se(&mine_hits, audio_mix);
                    self.commit_active_play_lane_state_to_profile();
                    self.clear_play_control_holds();
                    self.notify_obs_play_ended();
                    let now = Instant::now();
                    self.play.play_ending = Some(if failed {
                        practice_failed_ending(now)
                    } else {
                        practice_natural_finish_ending(now)
                    });
                    self.update_play_ending_snapshot();
                    return;
                }
                let finish_mode = if self.play.active_course.is_some() {
                    crate::screens::play_finish::FinishResultMode::CourseStage
                } else {
                    crate::screens::play_finish::FinishResultMode::Normal
                };
                let chart_length_ms = active_play.running.chart_length_ms;
                let play_duration_ms = active_play.running.finish_play_duration_ms();
                let early_finished = if active_play.running.pending_finished.is_some() {
                    None
                } else if let Some(finished) = active_play.running.finished.clone() {
                    Some(finished)
                } else {
                    match crate::screens::play_finish::finish_session_result_once(
                        &mut active_play.running.finished,
                        &mut self.boot.score_db,
                        &mut self.boot.network_db,
                        crate::screens::play_finish::FinishSessionResultOnceRequest {
                            profile_paths: &self.boot.profile_paths,
                            replay_config: &self.boot.profile_config.replay,
                            ir_config: &self.boot.profile_config.ir,
                            session: &active_play.running.session,
                            played_at: now_unix_seconds(),
                            applied_arrange: &active_play.running.applied_arrange,
                            source_ln_profile: active_play.running.source_ln_profile,
                            chart_length_ms: Some(chart_length_ms),
                            play_duration_ms: Some(play_duration_ms),
                            target_ex_score: active_play.running.target_ex_score,
                            target_name: &active_play.running.target,
                            score_key: active_play.running.score_key,
                            practice_mode: active_play.running.practice_mode
                                || active_play.running.score_save_disabled,
                            finish_mode,
                        },
                    ) {
                        Ok(mut finished) => {
                            finished.summary.skin_attempt = active_play.running.skin_attempt;
                            finished.summary.graph = Arc::new(
                                active_play
                                    .running
                                    .result_graph
                                    .snapshot_for_session(&active_play.running.session),
                            );
                            Some(finished)
                        }
                        Err(error) => {
                            tracing::error!(%error, "failed to finish play session at play end");
                            None
                        }
                    }
                };
                let hispeed = Some(active_play.running.session.hispeed);
                let mine_hits = frame.mine_hits.clone();
                let audio_mix = active_play.running.session.audio_mix;
                let mut snapshot = frame.render_snapshot;
                snapshot.play_elapsed_time = play_elapsed_time;
                snapshot.ready_elapsed_time = ready_elapsed_time;
                snapshot.seamless_play_entry = seamless_play_entry;
                snapshot.stagefile_background = stagefile_background;
                snapshot.stagefile_image_size = stagefile_image_size;
                snapshot.backbmp_background = backbmp_background;
                snapshot.course_stage = course_stage;
                snapshot.course_titles = course_titles.clone();
                let full_combo_elapsed_at_finish_ms = snapshot.full_combo_elapsed_ms;
                crate::screens::play_snapshot::refresh_play_skin_visuals(
                    &mut snapshot,
                    &active_play.running.session,
                );
                self.apply_profile_fast_slow_filter(&mut snapshot);
                self.apply_play_table_text(&mut snapshot);
                self.play.last_play_snapshot = Some(snapshot);
                self.play_guide_se_for_judgements(guide_se_enabled, &guide_judgements);
                if should_play_retire_sound {
                    self.play_system_sound(crate::system_sound::SoundType::PlayStop);
                }
                self.play_landmine_se(&mine_hits, audio_mix);
                // active_play がまだ残っている内に hispeed/lane_cover/lift を profile に保存する。
                self.save_current_play_options(hispeed, "play finished");
                if let Some(finished) = &early_finished {
                    if let Some(chart_id) = self.play.last_started_chart_id {
                        self.prepare_terminal_course_finish(chart_id, finished);
                    }
                    self.start_result_ir_for_finished_play(finished);
                }
                self.notify_obs_play_ended();
                let now = Instant::now();
                let failed = frame.state == bmz_gameplay::session::PlayState::Failed;
                self.play.play_ending = Some(PlayEndingTransition {
                    started_at: now,
                    music_end_started_at: (!failed).then_some(now),
                    fadeout_started_at: None,
                    failed,
                    completion: crate::app::result_flow_ending::play_ending_completion(
                        self.viewer_mode,
                        self.skip_result,
                    ),
                    full_combo_elapsed_at_finish_ms,
                    finished: early_finished,
                });
                self.update_play_ending_snapshot();
            }
            Err(error) => {
                tracing::error!(%error, "failed to advance play session");
                self.play.active_play = None;
                self.clear_play_meta_image_state();
                self.play.last_play_snapshot = None;
            }
        }
        self.sync_profile_visual_offset_from_active_play();
    }

    /// Play画面を `Playing` のまま維持し、判定確定後の保存・IRだけを先行する。
    /// 実際の `Finished` 遷移と退出演出は gameplay の従来の終了条件に任せる。
    fn finalize_settled_play_result_once(&mut self, settled_at: TimeUs) {
        let finish_mode = if self.play.active_course.is_some() {
            crate::screens::play_finish::FinishResultMode::CourseStage
        } else {
            crate::screens::play_finish::FinishResultMode::Normal
        };
        let Some(active_play) = &mut self.play.active_play else {
            return;
        };
        if active_play.running.finished.is_some()
            || active_play.running.pending_finished.is_some()
            || active_play.running.finish_error.is_some()
            || active_play.running.practice_mode
            || active_play.running.score_save_disabled
        {
            return;
        }
        let chart_length_ms = active_play.running.chart_length_ms;
        // 保存値は判定確定時刻を使うが、RunningPlaySession の terminal duration は
        // 従来どおり実際に Finished/Failed へ入った時点まで凍結しない。
        let play_duration_ms =
            (active_play.running.audio.clock().elapsed_since(TimeUs(0)).0.max(0) / 1_000) as u64;
        match crate::screens::play_finish::spawn_settled_session_result(
            crate::screens::play_finish::FinishSessionResultOnceRequest {
                profile_paths: &self.boot.profile_paths,
                replay_config: &self.boot.profile_config.replay,
                ir_config: &self.boot.profile_config.ir,
                session: &active_play.running.session,
                played_at: now_unix_seconds(),
                applied_arrange: &active_play.running.applied_arrange,
                source_ln_profile: active_play.running.source_ln_profile,
                chart_length_ms: Some(chart_length_ms),
                play_duration_ms: Some(play_duration_ms),
                target_ex_score: active_play.running.target_ex_score,
                target_name: &active_play.running.target,
                score_key: active_play.running.score_key,
                practice_mode: active_play.running.practice_mode
                    || active_play.running.score_save_disabled,
                finish_mode,
            },
            settled_at,
            active_play.running.result_graph.clone(),
        ) {
            Ok(pending) => active_play.running.pending_finished = Some(pending),
            Err(error) => {
                active_play.running.finish_error = Some(error.to_string());
                tracing::error!(%error, "failed to start settled play result save");
            }
        }
    }

    pub(super) fn poll_pending_finished_play(&mut self) {
        let completed = {
            let Some(active_play) = &mut self.play.active_play else {
                return;
            };
            let Some(pending) = &active_play.running.pending_finished else {
                return;
            };
            let elapsed_ms = pending.elapsed().as_millis();
            match pending.try_recv() {
                Ok(Some(finished)) => Some((Ok(finished), elapsed_ms)),
                Ok(None) => None,
                Err(error) => Some((Err(error), elapsed_ms)),
            }
        };
        let Some((result, elapsed_ms)) = completed else {
            return;
        };
        let finished = match result {
            Ok(mut finished) => {
                let Some(active_play) = &mut self.play.active_play else {
                    return;
                };
                finished.summary.skin_attempt = active_play.running.skin_attempt;
                active_play.running.pending_finished = None;
                active_play.running.finished = Some(finished.clone());
                active_play.running.finish_error = None;
                finished
            }
            Err(error) => {
                if let Some(active_play) = &mut self.play.active_play {
                    active_play.running.pending_finished = None;
                    active_play.running.finish_error = Some(error.to_string());
                }
                tracing::error!(%error, elapsed_ms, "background play result save failed");
                return;
            }
        };
        if let Some(ending) = &mut self.play.play_ending
            && ending.finished.is_none()
        {
            ending.finished = Some(finished.clone());
        }
        if let Some(chart_id) = self.play.last_started_chart_id {
            self.prepare_terminal_course_finish(chart_id, &finished);
        }
        self.start_result_ir_for_finished_play(&finished);
        tracing::info!(elapsed_ms, "background play result save completed");
    }

    pub(super) fn maybe_start_ready_phase(&mut self) {
        if self.play.play_ready_sound_started_at.is_some() {
            return;
        }
        let shows_ready_presentation = self.play.play_entry_presentation.shows_ready_presentation();
        let seamless_play_entry = !shows_ready_presentation;
        let now = Instant::now();
        if shows_ready_presentation {
            self.sync_play_control_holds_from_pressed_controls();
            if play_ready_blocked_by_control_holds(self.play.play_e1_held, self.play.play_e2_held) {
                self.play.play_ready_last_control_hold_at = Some(now);
                self.update_pending_play_snapshot_timers();
                return;
            }
            if play_ready_blocked_by_recent_control_hold(
                self.play.play_ready_last_control_hold_at,
                now,
            ) {
                self.update_pending_play_snapshot_timers();
                return;
            }
            if self.play_elapsed_time().0 < self.play_skin_ready_delay().as_micros() as i64 {
                return;
            }
        }
        let chart_id = self
            .play
            .pending_play_start
            .as_ref()
            .map(|start| start.chart_id)
            .or(self.play.last_started_chart_id);
        let Some(active_play) = &self.play.active_play else {
            return;
        };
        if !self.play.bga_preload.ready_for(chart_id, active_play.running.session.bga_enabled) {
            return;
        }
        let chart_zero_time = self
            .play
            .practice_chart_zero_time
            .take()
            .unwrap_or_else(|| self.play_skin_playstart_offset());
        let play_elapsed_time = self.play_elapsed_time();
        let Some(active_play) = &mut self.play.active_play else {
            return;
        };
        bmz_gameplay::session::drain_pre_ready_visual_inputs(
            &mut active_play.running.session,
            play_elapsed_time,
        );
        let start_result = if self.viewer_mode {
            active_play.running.start_viewer_seek(chart_zero_time, self.viewer_paused).map(
                |carryover_count| {
                    tracing::info!(
                        chart_time_us = chart_zero_time.0,
                        carryover_count,
                        paused = self.viewer_paused,
                        "started viewer audio from requested chart position"
                    );
                },
            )
        } else {
            active_play.running.start(chart_zero_time)
        };
        if let Err(error) = start_result {
            tracing::error!(%error, "failed to start preloaded play audio");
            self.abort_pending_play_start();
            return;
        }
        let decide_fade_out_frames =
            decide_bgm_fade_out_frames(chart_zero_time, self.play_output_sample_rate());
        self.stop_system_sound_with_fade_out(
            crate::system_sound::SoundType::Decide,
            decide_fade_out_frames,
        );
        self.play.play_ready_sound_started_at = Some(Instant::now());
        self.play.pending_play_start = None;
        if shows_ready_presentation {
            self.play_system_sound(crate::system_sound::SoundType::PlayReady);
        }
        if let Some(snapshot) = &mut self.play.last_play_snapshot {
            snapshot.play_elapsed_time = play_elapsed_time;
            snapshot.ready_elapsed_time = (!seamless_play_entry).then_some(TimeUs(0));
            snapshot.seamless_play_entry = seamless_play_entry;
            snapshot.time = chart_zero_time;
            if let Some(active_play) = &self.play.active_play {
                crate::screens::play_snapshot::refresh_play_skin_visuals_with_input_elapsed(
                    snapshot,
                    &active_play.running.session,
                    play_elapsed_time,
                );
            }
        }
    }

    pub(super) fn stop_decide_system_sound_after_chart_start(&mut self) {
        if self.play.decide_sound_stopped_for_chart_start {
            return;
        }
        let Some(active_play) = &self.play.active_play else {
            return;
        };
        if !chart_play_has_started(&active_play.running.session) {
            return;
        }
        self.stop_system_sound(crate::system_sound::SoundType::Decide);
        self.play.decide_sound_stopped_for_chart_start = true;
    }

    pub(super) fn update_pending_play_snapshot_timers(&mut self) {
        let play_elapsed_time = self.play_elapsed_time();
        let ready_elapsed_time = self.play_ready_animation_elapsed_time();
        let seamless_play_entry = self.play.play_entry_presentation.is_seamless();
        let resource_load_progress = self.current_play_resource_load_progress();
        let chart_id = self
            .play
            .pending_play_start
            .as_ref()
            .map(|start| start.chart_id)
            .or(self.play.last_started_chart_id);
        let applied_arrange =
            chart_id.and_then(|chart_id| self.play_preload_applied_arrange(chart_id));
        if let Some(snapshot) = &mut self.play.last_play_snapshot {
            snapshot.play_elapsed_time = play_elapsed_time;
            snapshot.ready_elapsed_time = ready_elapsed_time;
            snapshot.seamless_play_entry = seamless_play_entry;
            snapshot.resource_load_progress = resource_load_progress;
            if let Some(applied_arrange) = &applied_arrange {
                apply_play_arrange_to_snapshot(snapshot, applied_arrange);
            }
        }
        self.refresh_pending_play_visual_snapshot(play_elapsed_time);
    }

    pub(super) fn play_preload_prepared_chart(&self, chart_id: i64) -> Option<PreparedPlayChart> {
        self.play
            .preloaded_play_session
            .as_ref()
            .filter(|preloaded| preloaded.chart_id == chart_id)
            .map(|preloaded| preloaded.preloaded.prepared_chart())
            .or_else(|| {
                self.play
                    .pending_play_preload
                    .as_ref()
                    .filter(|pending| pending.chart_id == chart_id)
                    .and_then(|pending| pending.prepared_chart.get().cloned())
            })
    }

    pub(super) fn play_preload_applied_arrange(&self, chart_id: i64) -> Option<AppliedArrange> {
        self.play
            .preloaded_play_session
            .as_ref()
            .filter(|preloaded| preloaded.chart_id == chart_id)
            .map(|preloaded| preloaded.preloaded.applied_arrange.clone())
            .or_else(|| {
                self.play
                    .pending_play_preload
                    .as_ref()
                    .filter(|pending| pending.chart_id == chart_id)
                    .and_then(|pending| {
                        pending.prepared_chart.get().map(|chart| chart.applied_arrange.clone())
                    })
            })
    }

    pub(super) fn current_play_resource_load_progress(&self) -> f32 {
        let chart_id = self
            .play
            .pending_play_start
            .as_ref()
            .map(|start| start.chart_id)
            .or(self.play.last_started_chart_id);
        let audio_progress = self
            .play
            .pending_play_preload
            .as_ref()
            .filter(|pending| Some(pending.chart_id) == chart_id)
            .map(|pending| {
                pending.audio_progress.load(Ordering::Relaxed) as f32
                    / RESOURCE_LOAD_PROGRESS_SCALE as f32
            })
            .unwrap_or_else(|| {
                if self.play.preloaded_play_session.is_some() || self.play.active_play.is_some() {
                    1.0
                } else {
                    0.0
                }
            });
        let bga_progress = self.play.bga_preload.progress(chart_id);
        let bga_enabled =
            self.play.last_play_snapshot.as_ref().is_some_and(|snapshot| snapshot.bga_enabled);
        combined_resource_load_progress(audio_progress, bga_progress, bga_enabled)
    }

    pub(super) fn update_pre_ready_play_state(&mut self) {
        let play_elapsed_time = self.play_elapsed_time();
        let Some(active_play) = &mut self.play.active_play else {
            return;
        };
        bmz_gameplay::session::drain_pre_ready_visual_inputs(
            &mut active_play.running.session,
            play_elapsed_time,
        );
        let Some(snapshot) = &mut self.play.last_play_snapshot else {
            return;
        };
        snapshot.play_elapsed_time = play_elapsed_time;
        crate::screens::play_snapshot::refresh_play_skin_visuals_with_input_elapsed(
            snapshot,
            &active_play.running.session,
            play_elapsed_time,
        );
    }

    pub(super) fn apply_play_lane_action(&mut self, action: PlayLaneAction) -> bool {
        if self.play.active_play.is_none() {
            return self.apply_pending_play_lane_action(action);
        }
        let speed_locked = active_course_speed_locked(self.play.active_course.as_ref());
        let lane_target = &mut self.play.play_lane_target;
        let Some(active_play) = &mut self.play.active_play else {
            return false;
        };
        let hispeed_step = hispeed_step_for_profile(
            &self.boot.profile_config,
            active_play.running.session.hispeed_mode,
        );
        if !apply_play_lane_action_to_session(
            &mut active_play.running.session,
            lane_target,
            action,
            speed_locked,
            hispeed_step,
        ) {
            tracing::debug!("play lane change ignored: course NoSpeed constraint");
            return false;
        }
        tracing::info!(
            hispeed = active_play.running.session.hispeed,
            hispeed_mode = ?active_play.running.session.hispeed_mode,
            target_green_number = active_play.running.session.target_green_number,
            lane_cover = active_play.running.session.lane_cover,
            lift = active_play.running.session.lift,
            hidden = active_play.running.session.hidden_cover,
            lane_target = ?lane_target,
            lane_cover_visible = active_play.running.session.lane_cover_visible,
            "adjusted play lane settings"
        );
        update_pre_ready_play_snapshot_options_for_session(
            self.play.play_ready_sound_started_at,
            &mut self.play.last_play_snapshot,
            &active_play.running.session,
            &active_play.running.applied_arrange,
        );
        true
    }

    pub(super) fn apply_pending_play_lane_action(&mut self, action: PlayLaneAction) -> bool {
        let speed_locked = active_course_speed_locked(self.play.active_course.as_ref());
        let now_bpm =
            self.play.last_play_snapshot.as_ref().map_or(120.0, |snapshot| snapshot.now_bpm);
        let Some(pending) = &mut self.play.pending_play_start else {
            return false;
        };
        let lane = &mut pending.lane;
        if !apply_pending_play_lane_action_to_state(
            lane,
            action,
            &self.boot.profile_config,
            now_bpm,
            speed_locked,
        ) {
            tracing::debug!("pending play lane change ignored: course NoSpeed constraint");
            return false;
        }
        pending.lane_actions.push(action);
        if let Some(snapshot) = &mut self.play.last_play_snapshot {
            lane.apply_to_snapshot(snapshot);
        }
        tracing::info!(
            hispeed = lane.hispeed,
            hispeed_mode = ?lane.hispeed_mode,
            target_green_number = lane.target_green_number,
            lane_cover = lane.active_lane_cover(),
            lift = lane.lift,
            "adjusted pending play lane settings"
        );
        true
    }

    pub(super) fn stop_play_like_escape(&mut self, reason: &'static str) -> bool {
        if self.viewer_mode {
            return self.begin_viewer_exit_transition(reason);
        }
        let practice_phase = self.play.practice_session.as_ref().map(|practice| practice.phase);
        if play_exit_should_leave_practice(practice_phase) {
            self.begin_practice_leave_transition(reason);
            return true;
        }
        if self.play.play_ending.is_some() {
            return true;
        }
        if self.play.active_play.is_none() && self.play.pending_play_start.is_none() {
            return false;
        }
        let chart_started = self
            .play
            .active_play
            .as_ref()
            .is_some_and(|active_play| chart_play_has_started(&active_play.running.session));
        if !chart_started {
            if practice_phase == Some(PracticePhase::Playing) {
                self.begin_practice_leave_transition(reason);
                return true;
            }
            tracing::info!(reason, "fading out play before chart start");
            if let Some(active_play) = &mut self.play.active_play
                && let Err(error) = active_play.running.pause_audio()
            {
                tracing::warn!(%error, "failed to pause pre-play audio during exit");
            }
            self.invalidate_play_preload();
            self.clear_play_control_holds();
            self.stop_system_sound(crate::system_sound::SoundType::PlayReady);
            self.notify_obs_play_ended();
            self.play.play_ending = Some(pre_play_abort_ending(Instant::now()));
            self.update_play_ending_snapshot();
            return true;
        }

        let stopped = {
            let Some(active_play) = &mut self.play.active_play else {
                return false;
            };
            let session = &mut active_play.running.session;
            if session.judge.is_exhausted(&session.chart)
                || matches!(
                    session.state,
                    bmz_gameplay::session::PlayState::Failed
                        | bmz_gameplay::session::PlayState::Finished
                )
            {
                return false;
            }
            tracing::info!(reason, "stopping active play");
            session.state = bmz_gameplay::session::PlayState::Failed;
            true
        };
        self.clear_play_control_holds();
        self.play_system_sound(crate::system_sound::SoundType::PlayStop);
        if practice_phase == Some(PracticePhase::Playing) {
            if let Some(active_play) = &mut self.play.active_play
                && let Err(error) = active_play.running.pause_audio()
            {
                tracing::warn!(%error, "failed to stop practice audio after abort");
            }
            self.notify_obs_play_ended();
            self.play.play_ending = Some(practice_failed_ending(Instant::now()));
            self.update_play_ending_snapshot();
        }
        stopped
    }

    pub(super) fn update_play_exit_hold_timer(&mut self) {
        update_play_exit_hold_started_at(
            &mut self.play.play_exit_hold_started_at,
            self.play.play_e1_held,
            self.play.play_e2_held,
            Instant::now(),
        );
    }

    pub(super) fn clear_play_control_holds(&mut self) {
        self.play.last_play_start_press_at = None;
        self.play.decide_e1_held = false;
        self.play.play_e1_held = false;
        self.play.play_e2_held = false;
        self.play.play_e3_held = false;
        self.play.play_ready_last_control_hold_at = None;
        self.play.play_exit_hold_started_at = None;
        self.reset_play_analog_scroll();
        self.refresh_play_lane_value_changing();
    }

    pub(super) fn active_play_uses_playback_rate_keys(&self) -> bool {
        self.play.active_play.as_ref().is_some_and(|active_play| {
            active_play.running.session.autoplay.is_some()
                || active_play.running.session.replay_player.is_some()
        })
    }

    pub(super) fn sync_autoplay_replay_playback_rate(&mut self) {
        let rate = autoplay_replay_playback_rate_from_pressed_inputs(
            &self.input.pressed_play_inputs,
            self.play.play_option_input.as_ref(),
        );
        let Some(active_play) = &mut self.play.active_play else {
            return;
        };
        if active_play.running.session.autoplay.is_none()
            && active_play.running.session.replay_player.is_none()
        {
            return;
        }
        if active_play.running.playback_rate_percent != rate {
            active_play.running.set_playback_rate_percent(rate);
        }
    }

    pub(super) fn sync_play_control_holds_from_pressed_controls(&mut self) {
        let (e1_held, e2_held, e3_held) = self
            .play
            .play_option_input
            .as_ref()
            .map(|input| {
                play_control_hold_state_from_pressed_inputs(&self.input.pressed_play_inputs, input)
            })
            .unwrap_or((false, false, false));
        let was_ready_blocked =
            play_ready_blocked_by_control_holds(self.play.play_e1_held, self.play.play_e2_held);
        if self.play.play_e1_held == e1_held
            && self.play.play_e2_held == e2_held
            && self.play.play_e3_held == e3_held
        {
            if was_ready_blocked {
                self.play.play_ready_last_control_hold_at = Some(Instant::now());
            }
            return;
        }
        if self.play.play_e1_held != e1_held || self.play.play_e2_held != e2_held {
            self.reset_play_analog_scroll();
        }
        self.play.play_e1_held = e1_held;
        self.play.play_e2_held = e2_held;
        self.play.play_e3_held = e3_held;
        if was_ready_blocked || play_ready_blocked_by_control_holds(e1_held, e2_held) {
            self.play.play_ready_last_control_hold_at = Some(Instant::now());
        }
        self.refresh_play_lane_value_changing();
        self.update_play_exit_hold_timer();
    }

    pub(super) fn play_lane_value_changing(&self) -> bool {
        self.play.play_e1_held || self.play.play_e2_held
    }

    pub(super) fn refresh_play_lane_value_changing(&mut self) {
        let changing = self.play_lane_value_changing();
        if let Some(active_play) = &mut self.play.active_play {
            active_play.running.session.lane_cover_changing = changing;
            update_pre_ready_play_snapshot_options_for_session(
                self.play.play_ready_sound_started_at,
                &mut self.play.last_play_snapshot,
                &active_play.running.session,
                &active_play.running.applied_arrange,
            );
        } else if self.play.play_ready_sound_started_at.is_none()
            && let Some(pending) = &mut self.play.pending_play_start
        {
            pending.lane.lane_cover_changing = changing;
            if let Some(snapshot) = &mut self.play.last_play_snapshot {
                pending.lane.apply_to_snapshot(snapshot);
            }
        }
    }

    pub(super) fn update_play_e1_control_state(
        &mut self,
        device: DeviceId,
        control: &PhysicalControl,
        pressed: bool,
    ) -> bool {
        let is_e1 = self.play.play_option_input.as_ref().is_some_and(|input| {
            !input.resolves_lane(device, control)
                && input.is_action(device, control, InputActionConfig::E1)
        });
        if !is_e1 {
            return false;
        }
        let was_ready_blocked =
            play_ready_blocked_by_control_holds(self.play.play_e1_held, self.play.play_e2_held);
        if self.play.play_e1_held != pressed {
            self.reset_play_analog_scroll();
        }
        self.play.play_e1_held = pressed;
        if was_ready_blocked
            || play_ready_blocked_by_control_holds(self.play.play_e1_held, self.play.play_e2_held)
        {
            self.play.play_ready_last_control_hold_at = Some(Instant::now());
        }
        self.refresh_play_lane_value_changing();
        self.update_play_exit_hold_timer();
        true
    }

    /// Start / E1 の2回連続押しでレーンカバー (SUDDEN+) 表示を切り替える。
    /// キーボード・ゲームパッド共通。トグルした場合は true。
    pub(super) fn handle_play_start_double_press(&mut self) -> bool {
        let sudden_enabled = self
            .play
            .active_play
            .as_ref()
            .is_some_and(|active| active.running.session.lanecover_enabled)
            || self
                .play
                .pending_play_start
                .as_ref()
                .is_some_and(|pending| pending.lane.sudden_enabled);
        if !sudden_enabled {
            self.play.last_play_start_press_at = None;
            return false;
        }
        let now = Instant::now();
        if !register_play_start_double_press(&mut self.play.last_play_start_press_at, now) {
            return false;
        }
        self.apply_play_lane_action(PlayLaneAction::ToggleLaneCoverVisibility)
    }

    pub(super) fn update_play_exit_control_state(
        &mut self,
        device: DeviceId,
        control: &PhysicalControl,
        pressed: bool,
    ) -> bool {
        let was_ready_blocked =
            play_ready_blocked_by_control_holds(self.play.play_e1_held, self.play.play_e2_held);
        let (is_e2, is_e3) = self
            .play
            .play_option_input
            .as_ref()
            .map(|input| {
                if input.resolves_lane(device, control) {
                    (false, false)
                } else {
                    (
                        input.is_action(device, control, InputActionConfig::E2),
                        input.is_action(device, control, InputActionConfig::E3),
                    )
                }
            })
            .unwrap_or((false, false));
        let mut changed = false;
        if is_e2 {
            if self.play.play_e2_held != pressed {
                self.reset_play_analog_scroll();
            }
            self.play.play_e2_held = pressed;
            changed = true;
        }
        if is_e3 {
            self.play.play_e3_held = pressed;
            changed = true;
        }
        if !changed {
            return false;
        }
        if was_ready_blocked
            || play_ready_blocked_by_control_holds(self.play.play_e1_held, self.play.play_e2_held)
        {
            self.play.play_ready_last_control_hold_at = Some(Instant::now());
        }
        self.refresh_play_lane_value_changing();
        self.update_play_exit_hold_timer();
        if play_exit_chord_pressed(self.play.play_e2_held, self.play.play_e3_held) {
            return self.stop_play_like_escape("E2+E3 pressed during play");
        }
        false
    }

    pub(super) fn stop_play_if_exit_hold_elapsed(&mut self) -> bool {
        let hold_duration =
            Duration::from_millis(self.boot.profile_config.play.play_exit_hold_ms as u64);
        if play_exit_hold_elapsed(
            self.play.play_exit_hold_started_at,
            Instant::now(),
            hold_duration,
        ) {
            self.play.play_exit_hold_started_at = None;
            return self.stop_play_like_escape("E1+E2 held during play");
        }
        false
    }

    pub(super) fn update_play_ending_snapshot(&mut self) {
        let Some(ending) = &self.play.play_ending else {
            return;
        };
        let play_elapsed_time = self.play_elapsed_time();
        let ready_elapsed_time = self.play_ready_animation_elapsed_time();
        let seamless_play_entry = self.play.play_entry_presentation.is_seamless();
        let stagefile_background = self.play.play_stagefile_loaded;
        let stagefile_image_size = self.play.play_stagefile_size;
        let timers = PlayEndingSkinTimers {
            play_elapsed_time,
            ready_elapsed_time,
            backbmp_background: self.play.play_backbmp_loaded,
            failed_elapsed_ms: ending.failed.then_some(elapsed_since_ms(ending.started_at)),
            music_end_elapsed_ms: ending.music_end_started_at.map(elapsed_since_ms),
            fadeout_elapsed_ms: ending.fadeout_started_at.map(elapsed_since_ms),
        };

        let Some(active_play) = &mut self.play.active_play else {
            let Some(snapshot) = &mut self.play.last_play_snapshot else {
                return;
            };
            snapshot.play_elapsed_time = timers.play_elapsed_time;
            snapshot.ready_elapsed_time = timers.ready_elapsed_time;
            snapshot.seamless_play_entry = seamless_play_entry;
            snapshot.stagefile_background = stagefile_background;
            snapshot.stagefile_image_size = stagefile_image_size;
            snapshot.failed_elapsed_ms = timers.failed_elapsed_ms;
            snapshot.music_end_elapsed_ms = timers.music_end_elapsed_ms;
            snapshot.fadeout_elapsed_ms = timers.fadeout_elapsed_ms;
            return;
        };

        let video_update_time = compute_frame_times(&active_play.running.session).audio_now;
        crate::video_bga::update_video_bga_frames(
            &mut self.renderer,
            &mut active_play.running,
            video_update_time,
        );

        let mut snapshot = refresh_play_ending_snapshot(&mut active_play.running, timers);
        snapshot.seamless_play_entry = seamless_play_entry;
        snapshot.stagefile_background = stagefile_background;
        snapshot.stagefile_image_size = stagefile_image_size;
        self.apply_profile_fast_slow_filter(&mut snapshot);
        self.apply_course_skin_context(&mut snapshot);
        self.apply_play_table_text(&mut snapshot);
        self.play.last_play_snapshot = Some(snapshot);
    }
}
