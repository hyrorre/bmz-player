use super::*;

impl WinitApp {
    pub(super) fn advance_play_ending(&mut self) {
        let Some(ending) = &self.play.play_ending else {
            return;
        };
        if ending.completion == PlayEndingCompletion::ViewerWait {
            self.finish_play_ending();
            return;
        }
        if ending.failed {
            if ending.started_at.elapsed() >= self.play_close_duration() {
                self.finish_play_ending();
            }
            return;
        }

        if ending.fadeout_started_at.is_none()
            && ending.started_at.elapsed() >= self.play_pre_fadeout_duration(ending)
        {
            if let Some(ending) = &mut self.play.play_ending {
                ending.fadeout_started_at = Some(Instant::now());
            }
            return;
        }

        let Some(fadeout_started_at) =
            self.play.play_ending.as_ref().and_then(|e| e.fadeout_started_at)
        else {
            return;
        };
        if fadeout_started_at.elapsed() >= self.play_fadeout_duration(ending.completion) {
            self.finish_play_ending();
        }
    }

    pub(super) fn finish_play_ending(&mut self) {
        self.poll_pending_finished_play();
        if self
            .play
            .active_play
            .as_ref()
            .is_some_and(|active| active.running.pending_finished.is_some())
        {
            return;
        }
        let Some(mut ending) = self.play.play_ending.take() else {
            return;
        };
        match ending.completion {
            PlayEndingCompletion::ViewerWait => {
                tracing::info!("viewer play finished; keeping Play scene for the next command");
                self.stop_viewer_playback();
                self.request_redraw();
                return;
            }
            PlayEndingCompletion::ViewerExit => {
                tracing::info!("viewer fadeout completed; exiting app");
                self.finish_viewer_exit("viewer fadeout completed");
                return;
            }
            PlayEndingCompletion::Select => {
                tracing::info!("play fadeout before chart start completed; returning to select");
                self.abort_pending_play_start();
                return;
            }
            PlayEndingCompletion::PracticeConfig => {
                tracing::info!("practice play transition completed; returning to configuration");
                self.finish_practice_round();
                return;
            }
            PlayEndingCompletion::PracticeLeave => {
                tracing::info!("practice fadeout completed; returning to select");
                self.leave_practice();
                return;
            }
            PlayEndingCompletion::Result => {}
        }
        let Some(mut started) = self.play.active_play.take() else {
            return;
        };
        let finished = match ending.finished.take() {
            Some(finished) => finished,
            None if started.running.finished.is_some() => {
                started.running.finished.clone().expect("checked above")
            }
            None => {
                let chart_length_ms = started.running.chart_length_ms;
                let play_duration_ms = started.running.finish_play_duration_ms();
                match crate::screens::play_finish::finish_session_result_once(
                    &mut started.running.finished,
                    &mut self.boot.score_db,
                    &mut self.boot.network_db,
                    crate::screens::play_finish::FinishSessionResultOnceRequest {
                        profile_paths: &self.boot.profile_paths,
                        replay_config: &self.boot.profile_config.replay,
                        ir_config: &self.boot.profile_config.ir,
                        session: &started.running.gameplay,
                        played_at: now_unix_seconds(),
                        applied_arrange: &started.running.applied_arrange,
                        source_ln_profile: started.running.source_ln_profile,
                        chart_length_ms: Some(chart_length_ms),
                        play_duration_ms: Some(play_duration_ms),
                        target_ex_score: started.running.target_ex_score,
                        target_name: &started.running.target_name,
                        target: started.running.target_option,
                        score_key: started.running.score_key,
                        practice_mode: started.running.practice_mode
                            || started.running.score_save_disabled,
                        finish_mode: if self.play.active_course.is_some() {
                            crate::screens::play_finish::FinishResultMode::CourseStage
                        } else {
                            crate::screens::play_finish::FinishResultMode::Normal
                        },
                    },
                ) {
                    Ok(mut finished) => {
                        finished.summary.skin_attempt = started.running.skin_attempt;
                        finished.summary.graph = Arc::new(
                            started
                                .running
                                .result_graph
                                .snapshot_for_source(&started.running.gameplay),
                        );
                        finished
                    }
                    Err(error) => {
                        tracing::error!(%error, "failed to finish play session");
                        if let Some(chart_id) = self.play.last_started_chart_id {
                            self.capture_play_media_cache_from_running(
                                chart_id,
                                &mut started.running,
                            );
                        }
                        let mut audio = started.running.audio;
                        audio.mark_draining();
                        self.audio.draining_audio = Some(audio);
                        self.refresh_player_stats_snapshot();
                        self.leave_result();
                        return;
                    }
                }
            }
        };
        match finished_play_action(
            self.viewer_mode,
            self.skip_result,
            self.play.active_course.is_some(),
        ) {
            FinishedPlayAction::ViewerWait => {
                tracing::info!("viewer play finished; waiting for the next command");
                drop(finished);
                self.play.active_play = Some(started);
                self.stop_viewer_playback();
                self.request_redraw();
                return;
            }
            FinishedPlayAction::Exit => {
                tracing::info!("result screen skipped by CLI option");
                drop(started);
                drop(finished);
                self.play.last_play_snapshot = None;
                self.clear_play_meta_image_state();
                self.shutdown_requested.store(true, Ordering::SeqCst);
                self.request_redraw();
                return;
            }
            FinishedPlayAction::ShowResult => {}
        }
        self.select.score_refresh.mark_score_data_changed(finished.score_data_changed);
        record_skin_session_play(
            &mut self.select.player_stats,
            &finished.result,
            finished.score_data_changed,
            finished.replay_playback,
        );
        if let Some(chart_id) = self.play.last_started_chart_id {
            self.capture_play_media_cache_from_running(chart_id, &mut started.running);
        }
        let mut audio = started.running.audio;
        audio.mark_draining();
        self.audio.draining_audio = Some(audio);
        self.refresh_player_stats_snapshot();
        if self.play.active_course.is_some() {
            if let Some(chart_id) = self.play.last_started_chart_id {
                self.prepare_terminal_course_finish(chart_id, &finished);
            }
            self.advance_course_after_finish(finished);
            return;
        }
        if finished.stored.slot_paths.iter().any(Option::is_some) {
            self.notify_obs_save_recording(crate::obs::ObsRecordingSaveReason::OnReplay);
        }
        self.result.finished_play = Some(finished);
        self.result.result_gauge_graph_type = self
            .result
            .finished_play
            .as_ref()
            .map(|finished| finished.summary.gauge_type as i32)
            .unwrap_or(GaugeType::Normal as i32);
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        self.result.result_scene_started_at = Instant::now();
        self.ensure_result_skin_ready_for_entry(ResultSkinSlot::Normal);
    }

    /// 終了フェードアウトの経過を監視し、通常はスキンのフェードアウト時間を、
    /// スキップ時は timer=2 の実アニメーション終端と最終フレーム保持を過ぎたら
    /// 保留していた遷移を実行する。毎フレーム描画前に呼ぶ。
    pub(super) fn advance_result_exit(&mut self) {
        if self.result.finished_play.is_some()
            && self.result.result_exit.is_none()
            && let Some(auto_exit_duration) = self.result_auto_exit_duration()
            && self.result.result_scene_started_at.elapsed() >= auto_exit_duration
        {
            // 中間リザルトは scene 時間経過で次の曲へ、それ以外は選曲へ戻る。
            let action = if self.is_course_intermediate_result() {
                self.course_intermediate_exit_action()
            } else if self.autoplay_folder_has_next() {
                ResultExitAction::AdvanceAutoplayFolder
            } else {
                ResultExitAction::Leave
            };
            self.begin_result_exit(action);
        }
        let Some(exit) = self.result.result_exit.as_ref() else {
            return;
        };
        // 何らかの理由でリザルトを抜けていたら終了状態を破棄する。
        if self.result.finished_play.is_none() {
            self.stop_result_exit_system_sounds();
            if let Some(audio) = &self.result.result_skin_audio {
                audio.stop_all();
            }
            self.result.result_exit = None;
            return;
        }
        let started_at = exit.started_at;
        let action = exit.action.clone();
        let skip_requested = exit.skip_requested;
        let skip_final_frame_held = exit.skip_final_frame_held;
        let fadeout = Duration::from_millis(self.renderer.result_skin_fadeout_ms().max(0) as u64);
        let animation_duration = Duration::from_millis(
            self.renderer.result_skin_timer_animation_duration_ms(2).max(0) as u64,
        );
        let elapsed = started_at.elapsed();
        // スキンの終了アニメーション時間に合わせて、プレイ残響(draining_audio)を
        // 1.0 → 0.0 へ絞る。リザルトSEは begin_result_exit で callback 側の
        // fade-out を開始済みなので、ここでは毎フレーム command を投げない。
        self.fade_audio_for_result_exit(elapsed, fadeout);
        if skip_requested && elapsed >= animation_duration && !skip_final_frame_held {
            // この呼び出しでは遷移せず、次のフレームで最終状態を1フレーム描画する。
            if let Some(exit) = self.result.result_exit.as_mut() {
                exit.skip_final_frame_held = true;
            }
            return;
        }
        if !result_exit_transition_ready(
            elapsed,
            fadeout,
            animation_duration,
            skip_requested,
            skip_final_frame_held,
        ) {
            return;
        }
        self.stop_result_exit_system_sounds();
        self.result.result_exit = None;
        match action {
            ResultExitAction::Leave => self.leave_result(),
            ResultExitAction::Retry(mode) => self.retry_last_chart_with_mode(mode),
            ResultExitAction::HeldLanes => {
                match result_action_for_held_lanes(
                    self.result.result_key5_held,
                    self.result.result_key7_held,
                ) {
                    Some(mode) => self.retry_last_chart_with_mode(mode),
                    None => self.leave_result(),
                }
            }
            ResultExitAction::RetryCourseSameArrange => self.retry_course_same_arrange(),
            ResultExitAction::HeldCourseLanes => {
                match result_action_for_held_lanes(
                    self.result.result_key5_held,
                    self.result.result_key7_held,
                ) {
                    Some(ResultRetryMode::SameArrange) => self.retry_course_same_arrange(),
                    Some(ResultRetryMode::DifferentArrange) => {
                        self.retry_course_different_arrange()
                    }
                    None => self.leave_result(),
                }
            }
            ResultExitAction::AdvanceCourse => self.advance_to_next_course_chart(),
            ResultExitAction::FinishCourse => self.finish_course_after_intermediate_result(),
            ResultExitAction::AdvanceAutoplayFolder => self.advance_autoplay_folder(),
        }
    }

    pub(super) fn stop_result_exit_system_sounds(&self) {
        for sound_type in result_exit_system_sounds() {
            self.stop_system_sound(*sound_type);
        }
    }

    /// リザルト終了アニメ中、プレイ残響(draining_audio)のマスターゲインを
    /// 1.0 → 0.0 へランプする。毎フレーム呼ぶ。
    /// フェード時間は `RESULT_EXIT_AUDIO_FADE` を上限とし、スキンの終了アニメ時間
    /// (`fadeout`) がそれより短ければ遷移前に絞り切れるよう短い方を採用する。
    /// 見た目の遷移タイミング自体は `fadeout` のまま変えない。
    pub(super) fn fade_audio_for_result_exit(&mut self, elapsed: Duration, fadeout: Duration) {
        let gain = result_exit_audio_gain(elapsed, fadeout);
        if let Some(audio) = &self.audio.draining_audio {
            audio.engine.set_master_gain(gain);
        }
    }

    pub(super) fn result_exit_audio_fade_frames(&self, fadeout: Duration) -> u32 {
        duration_to_frames(
            result_exit_audio_fade_duration(fadeout),
            self.system_audio_sample_rate(),
        )
    }

    pub(super) fn system_audio_sample_rate(&self) -> u32 {
        self.audio.audio_runtime.as_ref().map(AudioRuntime::sample_rate).unwrap_or(48_000).max(1)
    }

    pub(super) fn fade_result_entry_system_sounds(&self, fade_out_frames: u32) {
        use crate::system_sound::SoundType;
        let Some(manager) = &self.audio.system_sound else {
            return;
        };
        for sound_type in [
            SoundType::ResultClear,
            SoundType::ResultFail,
            SoundType::CourseClear,
            SoundType::CourseFail,
        ] {
            manager.stop_with_fade_out(sound_type, fade_out_frames);
        }
    }

    pub(super) fn play_result_close_sound_with_fade_out(&self, fade_out_frames: u32) {
        let Some(manager) = &self.audio.system_sound else {
            return;
        };
        let sound_type = result_exit_sound_for_context(
            self.play.active_course.is_some() || self.result.finished_course.is_some(),
            manager.has_sound(crate::system_sound::SoundType::CourseClose),
        );
        manager.play_with_master_gain_and_fade_out(
            sound_type,
            system_sound_volume_from_mix(&self.boot.profile_config.audio_mix, sound_type),
            1.0,
            fade_out_frames,
        );
        self.start_audio_output_stream();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FinishedPlayAction {
    ShowResult,
    ViewerWait,
    Exit,
}

pub(super) const fn finished_play_action(
    viewer_mode: bool,
    skip_result: bool,
    has_active_course: bool,
) -> FinishedPlayAction {
    if has_active_course {
        FinishedPlayAction::ShowResult
    } else if viewer_mode && !skip_result {
        FinishedPlayAction::ViewerWait
    } else if skip_result {
        FinishedPlayAction::Exit
    } else {
        FinishedPlayAction::ShowResult
    }
}

pub(super) const fn play_ending_completion(
    viewer_mode: bool,
    skip_result: bool,
) -> PlayEndingCompletion {
    if viewer_mode && !skip_result {
        PlayEndingCompletion::ViewerWait
    } else {
        PlayEndingCompletion::Result
    }
}
