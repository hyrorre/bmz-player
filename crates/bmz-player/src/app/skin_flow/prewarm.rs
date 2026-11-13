//! Scene skins decoded before their scene is entered.
//!
//! Result loads once the score is final and Select loads while Result is shown, so
//! the entry finds the decode done. Each entry still uses one decode of its own,
//! keeping load-time random choices per scene entry.
use super::*;

/// A scene skin decoded ahead of its entry. The upload is held until the entry so
/// installing it does not cost a frame of the scene still on screen.
pub(in crate::app) struct PrewarmedSceneSkin {
    /// Decode generation; any newer load of the same kind replaces this one.
    pub(in crate::app) generation: u64,
    /// Inputs of a Select decode, compared with the entry's own inputs.
    pub(in crate::app) load: Option<FrontendSkinLoad>,
    pub(in crate::app) upload: Option<PendingUploadResult>,
}

impl PrewarmedSceneSkin {
    fn new(generation: u64, load: Option<FrontendSkinLoad>) -> Box<Self> {
        Box::new(Self { generation, load, upload: None })
    }

    /// Stands in for the entry's load while it is the latest load of its kind, its
    /// upload arrived, and no Lua VM changed the session files it read.
    fn ready_for_entry(&self, current_generation: u64, file_store_revision: u64) -> bool {
        self.generation == current_generation
            && self.upload.as_ref().is_some_and(|upload| match &upload.uploaded {
                Ok(uploaded) => uploaded
                    .load_dependencies
                    .file_store_span
                    .is_none_or(|span| span.unchanged_at(file_store_revision)),
                // A failed decode has no dependency span. Retry at entry so
                // callbacks that completed shared files can repair the load.
                Err(_) => false,
            })
    }
}

impl WinitApp {
    fn skin_prewarm_slot(
        &mut self,
        kind: SkinKind,
    ) -> Option<&mut Option<Box<PrewarmedSceneSkin>>> {
        match kind {
            SkinKind::Result => Some(&mut self.skin.result_skin_prewarm),
            SkinKind::Select => Some(&mut self.skin.select_skin_prewarm),
            SkinKind::Decide | SkinKind::Play => None,
        }
    }

    /// Keeps a prewarmed upload until its scene is entered; returns every other
    /// upload for installation now.
    pub(super) fn hold_prewarmed_upload(
        &mut self,
        upload: PendingUploadResult,
    ) -> Option<PendingUploadResult> {
        let target = match upload.kind {
            SkinKind::Result => AppSceneKind::Result,
            SkinKind::Select => AppSceneKind::Select,
            SkinKind::Decide | SkinKind::Play => return Some(upload),
        };
        if self.current_scene_kind() == target {
            return Some(upload);
        }
        let generation = upload.generation;
        match self.skin_prewarm_slot(upload.kind) {
            Some(Some(prewarm)) if prewarm.generation == generation => {
                prewarm.upload = Some(upload);
                None
            }
            _ => Some(upload),
        }
    }

    /// The held upload of `kind`'s prewarm when it can stand in for this entry.
    fn take_prewarmed_upload(&mut self, kind: SkinKind) -> Option<PendingUploadResult> {
        let prewarm = self.skin_prewarm_slot(kind)?.take()?;
        let current_generation = self.skin.skin_pipeline.generation(kind);
        let revision = self.skin.skin_pipeline.lua_file_store.revision();
        if !prewarm.ready_for_entry(current_generation, revision) {
            tracing::info!(
                ?kind,
                arrived = prewarm.upload.is_some(),
                current = prewarm.generation == current_generation,
                "prewarmed skin not used; loading again at entry"
            );
            return None;
        }
        prewarm.upload
    }

    /// Drops a prewarm whose scene will not be entered next. A decode still in
    /// flight becomes stale, so it never installs into the scene on screen.
    pub(super) fn discard_skin_prewarm(&mut self, kind: SkinKind) {
        let Some(prewarm) = self.skin_prewarm_slot(kind).and_then(Option::take) else {
            return;
        };
        let pipeline = &mut self.skin.skin_pipeline;
        if prewarm.generation == pipeline.generation(kind) {
            pipeline.bump_generation(kind);
            pipeline.set_pending(kind, false);
        }
    }

    /// Starts the Result skin decode as soon as the score is final, so the Play
    /// ending covers it. Course plays keep loading at Result entry because their
    /// slot and course state are only settled when the stage advances.
    pub(super) fn prewarm_result_skin_for_finished_play(&mut self, finished: &FinishedPlaySession) {
        // Viewer and --skip-result plays never show a Result scene.
        if self.viewer_mode || self.skip_result || self.play.active_course.is_some() {
            return;
        }
        // The settled save and the Finished frame both deliver the same result.
        if self.skin.last_result_skin_signature.is_some()
            && self.skin.result_skin_prewarm.as_ref().is_some_and(|prewarm| {
                prewarm.generation == self.skin.skin_pipeline.generation(SkinKind::Result)
            })
        {
            return;
        }
        // The Result inputs read `finished_play`; publish it only while building the
        // request so Play keeps seeing no finished result until the scene changes.
        let previous = self.result.finished_play.replace(finished.clone());
        self.begin_result_skin_load_for_entry(ResultSkinSlot::Normal);
        self.refresh_result_favorites();
        self.spawn_result_skin_decode_for(ResultSkinSlot::Normal);
        self.result.finished_play = previous;
        self.skin.result_skin_prewarm = self.skin.last_result_skin_signature.is_some().then(|| {
            PrewarmedSceneSkin::new(self.skin.skin_pipeline.generation(SkinKind::Result), None)
        });
        tracing::info!(
            prewarmed = self.skin.result_skin_prewarm.is_some(),
            "result skin decode started at score finalization"
        );
    }

    /// Enters Result with the prewarmed decode when it still matches.
    pub(super) fn enter_result_with_prewarmed_skin(&mut self) -> bool {
        // A skin setting change during the ending drops the signature.
        if self.skin.last_result_skin_signature.is_none() {
            self.skin.result_skin_prewarm = None;
            return false;
        }
        let Some(upload) = self.take_prewarmed_upload(SkinKind::Result) else {
            return false;
        };
        tracing::info!("result skin entry reuses the decode started at score finalization");
        self.apply_uploaded_skin(upload);
        self.refresh_result_favorites();
        self.request_scene_skin(SkinKind::Result);
        if !self.scene_skin_entry_waiting(SkinKind::Result) {
            self.finish_result_skin_entry();
        }
        true
    }

    /// Starts the Select skin decode while Result is shown. Course stage results
    /// lead to the next chart, not to Select.
    pub(super) fn prewarm_select_skin_for_result_exit(&mut self) {
        self.discard_skin_prewarm(SkinKind::Select);
        if self.is_course_intermediate_result() || self.jobs.profile_change.is_some() {
            return;
        }
        let Some(load) = self.select_skin_load() else {
            return;
        };
        let pipeline = &mut self.skin.skin_pipeline;
        let generation = pipeline.bump_generation(SkinKind::Select);
        spawn_frontend_skin_decode(
            &self.boot.app_paths,
            pipeline,
            &self.boot.profile_config.skin,
            generation,
            SkinKind::Select,
            load.clone(),
        );
        pipeline.set_pending(SkinKind::Select, true);
        self.skin.select_skin_prewarm = Some(PrewarmedSceneSkin::new(generation, Some(load)));
        tracing::info!(generation, "select skin decode started while result is shown");
    }

    /// Enters Select with the decode started during Result when its inputs, and the
    /// session files it read, are still the ones this entry would load with.
    pub(super) fn enter_select_with_prewarmed_skin(&mut self) -> bool {
        let Some(prewarm_load) =
            self.skin.select_skin_prewarm.as_ref().and_then(|prewarm| prewarm.load.clone())
        else {
            return false;
        };
        if self.select_skin_load().as_ref() != Some(&prewarm_load) {
            self.skin.select_skin_prewarm = None;
            tracing::info!("select skin inputs changed during result; loading again at entry");
            return false;
        }
        let Some(upload) = self.take_prewarmed_upload(SkinKind::Select) else {
            return false;
        };
        tracing::info!("select skin entry reuses the decode started during result");
        self.discard_cli_play_on_select();
        self.apply_uploaded_skin(upload);
        self.request_scene_skin(SkinKind::Select);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin_loader::UploadedSkin;

    fn upload(generation: u64, span: Option<bmz_skin::LuaFileStoreSpan>) -> PendingUploadResult {
        let now = Instant::now();
        PendingUploadResult {
            generation,
            path: "select.luaskin".into(),
            kind: SkinKind::Select,
            queued_at: now,
            decode_started_at: now,
            decode_finished_at: now,
            upload_started_at: now,
            upload_finished_at: now,
            uploaded: Ok(UploadedSkin {
                load_dependencies: bmz_skin::SkinLoadDependencies {
                    file_store_span: span,
                    ..Default::default()
                },
                kind: SkinKind::Select,
                document: serde_json::from_value(serde_json::json!({"type": 5})).unwrap(),
                lua_runtime: None,
                fonts: Vec::new(),
                prepared: Vec::new(),
                audio_assets: Vec::new(),
                decode_stats: Default::default(),
                upload_stats: Default::default(),
            }),
        }
    }

    #[test]
    fn prewarm_stands_in_only_for_the_latest_load_of_unchanged_files() {
        let mut prewarm = PrewarmedSceneSkin::new(3, None);
        assert!(!prewarm.ready_for_entry(3, 0), "an upload still in flight is not used");
        prewarm.upload = Some(upload(3, None));
        assert!(prewarm.ready_for_entry(3, 0));
        assert!(!prewarm.ready_for_entry(4, 0), "a newer load replaced it");

        // The load's own commit moved the store from 5 to 6.
        let own = bmz_skin::LuaFileStoreSpan { seen: 5, committed_from: 5, left: 6 };
        prewarm.upload = Some(upload(3, Some(own)));
        assert!(prewarm.ready_for_entry(3, 6));
        assert!(!prewarm.ready_for_entry(3, 7), "a runtime callback wrote afterwards");
        let raced = bmz_skin::LuaFileStoreSpan { seen: 5, committed_from: 6, left: 7 };
        prewarm.upload = Some(upload(3, Some(raced)));
        assert!(!prewarm.ready_for_entry(3, 7), "another VM wrote while it loaded");
    }

    #[test]
    fn failed_prewarm_always_retries_at_entry_even_after_a_callback_writes() {
        let mut prewarm = PrewarmedSceneSkin::new(3, None);
        let mut failed = upload(3, None);
        failed.uploaded = Err(anyhow::anyhow!("shared history was temporarily empty"));
        prewarm.upload = Some(failed);
        assert!(!prewarm.ready_for_entry(3, 5));
        assert!(!prewarm.ready_for_entry(3, 6));
    }
}
