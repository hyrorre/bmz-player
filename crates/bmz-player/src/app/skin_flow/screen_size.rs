use super::*;

impl WinitApp {
    /// Runtime-only size readers update on the next frame. Rebuild only documents
    /// that read the old size during construction, or failed before attachment.
    pub(super) fn refresh_skin_screen_size(&mut self) {
        let size = self.renderer.render_target_size();
        if self.skin.skin_pipeline.screen_size == size {
            return;
        }
        self.skin.skin_pipeline.screen_size = size;
        for kind in [SkinKind::Select, SkinKind::Decide, SkinKind::Play, SkinKind::Result] {
            // Pending results expose their actual dependencies when they arrive.
            // Keep profile-change requests separate from the installed old profile.
            if self.skin.skin_pipeline.is_pending(kind)
                || (kind == SkinKind::Select && self.jobs.profile_change.is_some())
            {
                continue;
            }
            if self.skin.skin_pipeline.screen_load_needs_refresh(kind) {
                self.retry_skin_for_screen_size(kind);
            }
        }
    }

    pub(super) fn retry_skin_for_screen_size(&mut self, kind: SkinKind) -> Option<u64> {
        let pipeline = &mut self.skin.skin_pipeline;
        let mut request = pipeline.screen_request(kind)?;
        let old_generation = request.generation;
        let installed_dependencies = pipeline.screen_dependencies(kind);
        let was_pending = pipeline.is_pending(kind);
        let result_refresh = kind == SkinKind::Result
            && (!was_pending || pipeline.result_refresh_generation == Some(old_generation));
        request.generation = if kind == SkinKind::Result {
            pipeline.begin_result_load(result_refresh)
        } else {
            pipeline.bump_generation(kind)
        };
        request.runtime_state.screen_size = pipeline.screen_size;
        if let Some(dependencies) = installed_dependencies {
            request.runtime_state.pinned_random_file_paths = dependencies.random_file_paths;
        }
        if result_refresh {
            request.pinned_sources.clone_from(&pipeline.result_source_selections);
        }
        let generation = request.generation;
        spawn_skin_decode(pipeline, request.reuse_installed_fonts(pipeline));
        pipeline.set_pending(kind, true);
        if let Some(signature) = self.skin.last_play_skin_signature.as_mut()
            && kind == SkinKind::Play
        {
            signature.5.screen_size = pipeline.screen_size;
        }
        if let Some(signature) = self.skin.last_result_skin_signature.as_mut()
            && kind == SkinKind::Result
        {
            signature.4.screen_size = pipeline.screen_size;
        }
        if kind == SkinKind::Select
            && let Some(change) = self.jobs.profile_change.as_mut()
        {
            change.replace_skin_generation(old_generation, generation);
        }
        // First Select needs this worker before its stale startup animation can finish.
        self.start_skin_upload_worker();
        self.frame.request_immediate_frame();
        self.request_redraw();
        Some(generation)
    }
}
