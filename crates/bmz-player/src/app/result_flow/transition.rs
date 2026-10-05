use super::*;

impl WinitApp {
    /// リザルト画面の終了アニメーションを開始する。
    /// 通常はスキンが宣言するフェードアウト時間が経過したら、スキップ要求時は
    /// timer=2 の実アニメーションが終わって最終フレームを保持したら、
    /// `advance_result_exit` が実際の遷移 (選曲へ戻る / リトライ) を実行する。
    pub(super) fn begin_result_exit(&mut self, action: ResultExitAction) {
        if self.result.result_exit.is_some() || self.result.finished_play.is_none() {
            return;
        }
        let score_save_enabled = self.current_result_score_save_enabled();
        if let Some(spec) = self.renderer.result_skin_document().and_then(|d| d.lr2_result.as_ref())
        {
            if self.result.lr2_result_epoch != Some(self.result.result_scene_started_at) {
                self.result.lr2_result_epoch = Some(self.result.result_scene_started_at);
                self.result.lr2_result_progress = Default::default();
            }
            let elapsed =
                self.result.result_scene_started_at.elapsed().as_millis().min(i32::MAX as u128)
                    as i32;
            if !self.result.lr2_result_progress.press(spec, elapsed, score_save_enabled) {
                return;
            }
        }
        tracing::info!(?action, "result screen exit animation started");
        self.result.result_exit = Some(ResultExit {
            started_at: Instant::now(),
            action,
            skip_requested: false,
            skip_final_frame_held: false,
        });
        let (skin_bgm_volume, skin_se_volume) = self.result_skin_audio_volumes();
        let dispatched = self
            .result
            .result_skin_audio
            .as_mut()
            .is_some_and(|audio| audio.trigger_timer(2, skin_bgm_volume, skin_se_volume));
        if dispatched {
            self.start_audio_output_stream();
        }
        // HeldLanes の遷移判定はフェードアウト終了時に Key5/Key7 の
        // 押下状態を読むため、ここでは held フラグをリセットしない。
        // Result SE は毎フレームの master-gain command ではなく callback 側で
        // fade-out させ、ASIO の小さい buffer でも段差が出にくいようにする。
        let fadeout = Duration::from_millis(self.renderer.result_skin_fadeout_ms().max(0) as u64);
        let fade_out_frames = self.result_exit_audio_fade_frames(fadeout);
        self.fade_result_entry_system_sounds(fade_out_frames);
        self.play_result_close_sound_with_fade_out(fade_out_frames);
    }

    pub(super) fn request_result_exit_skip_for_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        if result_exit_skip_key(physical_key, state, repeat) {
            return self.request_result_exit_skip();
        }
        false
    }

    pub(super) fn request_result_exit_skip_for_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        if pressed && !repeat && self.result_exit_skip_control(control) {
            return self.request_result_exit_skip();
        }
        false
    }

    pub(super) fn result_exit_skip_control(&self, control: &PhysicalControl) -> bool {
        self.result_lane_for_control(control).is_some_and(lane_skips_result_exit)
    }

    pub(super) fn request_result_exit_skip(&mut self) -> bool {
        let Some(exit) = self.result.result_exit.as_mut() else {
            return false;
        };
        if !exit.skip_requested {
            tracing::info!("result screen exit animation skip requested");
        }
        exit.skip_requested = true;
        true
    }

    pub(super) fn begin_decide_fadeout(&mut self, cancel: bool) {
        if self.play.pending_decide.is_none() {
            return;
        }
        self.clear_play_control_holds();
        let Some(decide) = &mut self.play.pending_decide else {
            return;
        };
        if decide.fadeout_started_at.is_some() {
            return;
        }
        decide.cancel = cancel;
        decide.fadeout_started_at = Some(Instant::now());
    }

    pub(super) fn advance_decide_transition(&mut self) {
        let Some(fadeout_started) =
            self.play.pending_decide.as_ref().map(|decide| decide.fadeout_started_at.is_some())
        else {
            return;
        };
        if !fadeout_started && self.cancel_decide_if_exit_hold_elapsed() {
            return;
        }
        let Some(decide) = &self.play.pending_decide else {
            return;
        };
        if decide.fadeout_started_at.is_none()
            && decide.started_at.elapsed() >= self.decide_scene_duration()
        {
            self.begin_decide_fadeout(false);
            return;
        }

        let Some(fadeout_started_at) =
            self.play.pending_decide.as_ref().and_then(|d| d.fadeout_started_at)
        else {
            return;
        };
        if fadeout_started_at.elapsed() < self.decide_fadeout_duration() {
            return;
        }

        if !decide.cancel && !self.decide_play_start_ready() {
            return;
        }

        let Some(decide) = self.play.pending_decide.take() else {
            return;
        };
        if decide.cancel {
            self.invalidate_play_preload();
            // Decide screen cancel (Escape) returns to select.  If a course
            // was being started, drop the course session — the user opted
            // out before the first chart actually began.
            self.clear_active_course_state();
            self.select.autoplay_folder = None;
            self.reload_skin_for_scene_entry(SkinKind::Select);
            self.restart_select_scene_timers();
        } else {
            let practice = decide.launch.into_practice_session();
            let entering_practice = practice.is_some();
            self.play.practice_session = practice;
            self.enter_play_scene(decide.chart_id, decide.options, decide.snapshot);
            // Decide 中に WAV preload が完了済みなら同一フレームで active session を
            // install し、不要な placeholder 1 フレームを挟まない。
            // Practice 設定画面では practice_session を先に有効化しているため、
            // preload は active session にせず start_practice_round まで保持する。
            self.poll_play_preload();
            if entering_practice {
                tracing::info!(chart_id = decide.chart_id, "practice configuration screen ready");
            }
        }
    }

    pub(super) fn decide_play_start_ready(&self) -> bool {
        // preload (WAV ロード等) の完了は待たない。Play 画面へ先に入場し、
        // ロード完了後に poll_play_preload が active_play を install して
        // READY タイマーが始まる。
        !self.skin.skin_pipeline.is_pending(SkinKind::Play)
    }

    pub(super) fn update_decide_cancel_control_state(
        &mut self,
        control: &str,
        pressed: bool,
    ) -> bool {
        let mut handled = false;
        if self.select.select_keys.is_start(control) {
            self.play.decide_e1_held = pressed;
            handled = true;
        }
        if self.select.select_keys.is_e2_action(control) {
            self.play.play_e2_held = pressed;
            handled = true;
        }
        if self.select.select_keys.is_e3_action(control) {
            self.play.play_e3_held = pressed;
            handled = true;
        }
        if !handled {
            return false;
        }
        update_play_exit_hold_started_at(
            &mut self.play.play_exit_hold_started_at,
            self.play.decide_e1_held,
            self.play.play_e2_held,
            Instant::now(),
        );
        if pressed
            && decide_cancel_chord_pressed(
                self.play.decide_e1_held,
                self.play.play_e2_held,
                self.play.play_e3_held,
            )
        {
            self.begin_decide_fadeout(true);
            return true;
        }
        true
    }

    pub(super) fn cancel_decide_if_exit_hold_elapsed(&mut self) -> bool {
        let hold_duration =
            Duration::from_millis(self.boot.profile_config.play.play_exit_hold_ms as u64);
        if play_exit_hold_elapsed(
            self.play.play_exit_hold_started_at,
            Instant::now(),
            hold_duration,
        ) {
            self.begin_decide_fadeout(true);
            return true;
        }
        false
    }
}
