use super::*;

impl WinitApp {
    pub(super) fn leave_result(&mut self) {
        if let Some(finished) = &self.result.finished_play {
            self.select.score_refresh.mark_score_data_changed(finished.score_data_changed);
        }
        if let Some(audio) = &self.result.result_skin_audio {
            audio.stop_all();
        }
        self.result.finished_play = None;
        self.select.autoplay_folder = None;
        self.result.result_favorite_chart = false;
        self.clear_active_course_state();
        self.result.result_exit = None;
        self.result.result_key5_held = false;
        self.result.result_key7_held = false;
        self.clear_result_ir_scroll_input();
        self.clear_play_meta_image_state();
        // リザルト画面を抜けたら、まだ鳴っていても余韻再生を止める。
        self.audio.draining_audio = None;
        self.play.play_media_cache = None;
        self.play.last_play_snapshot = None;
        self.discard_cli_play_on_select();
        self.reload_select_items();
        self.sync_select_holds_from_pressed_controls();
        self.reload_skin_for_scene_entry(SkinKind::Select);
        self.restart_select_scene_timers();
    }

    pub(super) fn decide_scene_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.scene).unwrap_or(0))
    }

    pub(super) fn decide_fadeout_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.fadeout).unwrap_or(0))
    }

    pub(super) fn decide_fadeout_scene_timing(&self) -> DecideFadeoutSceneTiming {
        decide_fadeout_scene_timing(self.renderer.decide_skin_document())
    }

    pub(super) fn play_finishmargin_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.play_skin_document().map(|d| d.finishmargin).unwrap_or(0))
    }

    pub(super) fn play_pre_fadeout_duration(&self, ending: &PlayEndingTransition) -> Duration {
        let finishmargin = self.play_finishmargin_duration();
        let Some(elapsed_ms) = ending.full_combo_elapsed_at_finish_ms else {
            return finishmargin;
        };
        let full_combo_ms = self
            .renderer
            .play_skin_timer_animation_duration_ms(48)
            .max(self.renderer.play_skin_timer_animation_duration_ms(49));
        let remaining_ms = full_combo_ms.saturating_sub(elapsed_ms.max(0));
        finishmargin.max(skin_duration_ms(remaining_ms))
    }

    pub(super) fn play_fadeout_duration(&self, completion: PlayEndingCompletion) -> Duration {
        let declared_ms =
            self.renderer.play_skin_document().map(|document| document.fadeout).unwrap_or(0).max(0);
        if matches!(
            completion,
            PlayEndingCompletion::PracticeConfig | PlayEndingCompletion::PracticeLeave
        ) {
            return practice_play_fadeout_duration_for_skin(declared_ms);
        }
        play_fadeout_duration_for_skin(
            declared_ms,
            self.renderer.play_skin_timer_animation_duration_ms(2),
        )
    }

    pub(super) fn play_close_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.play_skin_document().map(|d| d.close).unwrap_or(0))
    }

    pub(super) fn result_input_ready(&self) -> bool {
        self.result.result_scene_started_at.elapsed() >= self.result_input_duration()
    }

    pub(super) fn result_input_duration(&self) -> Duration {
        result_input_duration_for_document(self.renderer.result_skin_document())
    }

    pub(super) fn result_auto_exit_duration(&self) -> Option<Duration> {
        let duration = result_auto_exit_duration_for_document(
            self.renderer.result_skin_document(),
            self.is_course_intermediate_result(),
            self.course_intermediate_auto_advance_enabled(),
        );
        if duration.is_none() && self.autoplay_folder_has_next() {
            Some(FALLBACK_RESULT_SCENE_DURATION)
        } else {
            duration
        }
    }
}

/// beatoraja の Practice は timer 2 の destination 長ではなく、skin header の
/// `fadeout` 宣言だけを状態遷移の待ち時間に使う。
pub(super) fn practice_play_fadeout_duration_for_skin(declared_ms: i32) -> Duration {
    skin_duration_ms(declared_ms.max(0))
}

pub(super) fn play_fadeout_duration_for_skin(
    declared_ms: i32,
    timer_animation_ms: i32,
) -> Duration {
    let duration_ms = declared_ms.max(timer_animation_ms).max(0);
    skin_duration_ms(if duration_ms == 0 {
        bmz_render::snapshot::DEFAULT_PLAY_FADEOUT_DURATION_MS
    } else {
        duration_ms
    })
}
