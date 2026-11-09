use super::*;

impl WinitApp {
    /// Queue an interactive scene dependency without waiting for disk, Lua or GPU upload.
    pub(super) fn request_scene_skin(&mut self, kind: SkinKind) {
        self.start_skin_upload_worker();
        if kind == SkinKind::Play {
            self.publish_prepared_play_chart();
        }
        if self.is_kind_pending_decode(kind) {
            self.skin.scene_entry_waits.insert(kind);
        } else {
            self.skin.scene_entry_waits.remove(&kind);
            if kind == SkinKind::Play {
                self.skin.skin_pipeline.lr2_assets.previous_scene = Some(SkinKind::Play);
            }
        }
    }

    pub(super) fn scene_skin_entry_waiting(&self, kind: SkinKind) -> bool {
        self.skin.scene_entry_waits.contains(&kind)
    }

    pub(super) fn scene_skin_entry_loading(&self) -> bool {
        match self.current_scene_kind() {
            AppSceneKind::Select => self.scene_skin_entry_waiting(SkinKind::Select),
            AppSceneKind::Decide => self.scene_skin_entry_waiting(SkinKind::Decide),
            AppSceneKind::Play => {
                self.scene_skin_entry_waiting(SkinKind::Play)
                    || self.scene_skin_entry_waiting(SkinKind::Decide)
            }
            AppSceneKind::Result => self.scene_skin_entry_waiting(SkinKind::Result),
        }
    }

    pub(super) fn poll_scene_skin_entries(&mut self) {
        let was_loading = self.scene_skin_entry_loading();
        self.skin.scene_entry_waits.retain(|kind| self.skin.skin_pipeline.is_pending(*kind));
        if !was_loading || self.scene_skin_entry_loading() {
            return;
        }
        let now = Instant::now();
        match self.current_scene_kind() {
            AppSceneKind::Select => self.restart_select_scene_timers(),
            AppSceneKind::Decide => {
                let lr2 = self.renderer.decide_skin_document().is_some_and(|doc| doc.lr2);
                if let Some(decide) = &mut self.play.pending_decide {
                    decide.started_at = now;
                    // A skip requested during loading starts its animation with the new skin.
                    if decide.fadeout_started_at.is_some() {
                        decide.fadeout_started_at = Some(now);
                        if !decide.cancel {
                            decide.exit =
                                if lr2 { DecideExit::Lr2Close } else { DecideExit::Fadeout };
                        }
                    }
                }
            }
            AppSceneKind::Play => {
                if !self.play.play_entry_presentation.is_seamless() {
                    self.play.play_scene_started_at = now;
                }
                self.skin.skin_pipeline.lr2_assets.previous_scene = Some(SkinKind::Play);
            }
            AppSceneKind::Result => {
                self.result.result_scene_started_at = now;
                self.finish_result_skin_entry();
            }
        }
    }
}
