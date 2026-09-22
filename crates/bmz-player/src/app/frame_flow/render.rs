use super::*;

impl WinitApp {
    pub(in crate::app) fn render_current_scene(&mut self) -> Option<SceneFrameProfileSample> {
        let select_view = matches!(self.view_state(), AppViewState::Select);
        let decide_view = matches!(self.view_state(), AppViewState::Decide);
        let play_view = matches!(self.view_state(), AppViewState::Play);
        let result_view = matches!(self.view_state(), AppViewState::Result);
        let profiling_select = select_view
            && tracing::enabled!(target: "bmz_player::select_profile", tracing::Level::DEBUG);
        let profiling_decide = decide_view
            && tracing::enabled!(target: "bmz_player::decide_profile", tracing::Level::DEBUG);
        let profiling_play = play_view
            && tracing::enabled!(target: "bmz_player::play_profile", tracing::Level::DEBUG);
        let profiling_result = result_view
            && tracing::enabled!(target: "bmz_player::result_profile", tracing::Level::DEBUG);
        if select_view && !self.viewer_waiting {
            self.refresh_visible_select_folder_summaries();
            self.poll_select_asset_loads();
            self.sync_select_stage_texture();
            self.sync_select_backbmp_texture();
            self.sync_select_banner_texture();
            self.sync_select_preview_audio();
            self.update_select_preview_fade();
        }
        self.start_scene_timers_before_snapshot(select_view, result_view);
        let snapshot_start = Instant::now();
        let scene = self.scene_snapshot();
        let snapshot_us = snapshot_start.elapsed().as_micros();
        let scene_kind = scene_kind(&scene);
        let plan_start = Instant::now();
        self.renderer.prepare_scene(scene);
        let plan_us = plan_start.elapsed().as_micros();
        let video_start = Instant::now();
        let video_profile = self.update_current_skin_video_sources(
            profiling_select || profiling_decide || profiling_play || profiling_result,
        );
        let video_us = video_start.elapsed().as_micros();
        self.update_window_title_for_scene(scene_kind);
        if let (Some(path), Some(exit_after_frames)) =
            (&self.smoke.smoke_screenshot_path, self.smoke.smoke_exit_after_frames)
            && self.smoke.rendered_frames.saturating_add(1) >= exit_after_frames
        {
            self.renderer.request_screenshot(path.clone());
        }
        let render_start = Instant::now();
        let render_status = self.renderer.render_last_plan();
        let render_us = plan_us + render_start.elapsed().as_micros();
        let frame_timings = self.renderer.last_frame_timings();
        let surface_status = render_status.as_ref().ok().copied();
        self.frame.record_surface_status(Instant::now(), surface_status);
        if surface_status == Some(RenderSurfaceStatus::Rendered) && !self.smoke.first_present_logged
        {
            self.smoke.first_present_logged = true;
            tracing::info!(
                startup_to_first_present_ms = self.smoke.startup_started_at.elapsed().as_millis(),
                "first surface frame presented"
            );
        }
        self.arm_select_scene_timers_after_render(select_view, surface_status);
        self.log_pending_skin_render_probe(
            scene_kind,
            surface_status,
            snapshot_us,
            video_us,
            render_us,
            frame_timings,
        );
        log_render_status(render_status);
        let profile_kind = if profiling_select {
            Some(FrameProfileKind::Select)
        } else if profiling_decide {
            Some(FrameProfileKind::Decide)
        } else if profiling_play {
            Some(FrameProfileKind::Play)
        } else if profiling_result {
            Some(FrameProfileKind::Result)
        } else {
            None
        };
        frame_profile_sample(
            profile_kind,
            video_us,
            video_profile,
            snapshot_us,
            render_us,
            frame_timings,
        )
    }

    fn log_pending_skin_render_probe(
        &mut self,
        scene_kind: AppSceneKind,
        render_status: Option<RenderSurfaceStatus>,
        snapshot_us: u128,
        video_us: u128,
        render_us: u128,
        frame_timings: Option<bmz_render::renderer::RenderFrameTimings>,
    ) {
        let Some(probe) = self.skin.pending_skin_render_probe.take() else {
            return;
        };
        let expected_scene = match probe.kind {
            SkinKind::Select => AppSceneKind::Select,
            SkinKind::Decide => AppSceneKind::Decide,
            SkinKind::Play => AppSceneKind::Play,
            SkinKind::Result => AppSceneKind::Result,
        };
        if expected_scene != scene_kind {
            self.skin.pending_skin_render_probe = Some(probe);
            return;
        }
        let timings = frame_timings.unwrap_or_default();
        tracing::debug!(
            kind = ?probe.kind,
            generation = probe.generation,
            scene = ?scene_kind,
            status = ?render_status,
            since_apply_us = instant_elapsed_us_u64(probe.applied_at),
            snapshot_us,
            video_us,
            render_us,
            plan_us = timings.plan_us,
            draw_us = timings.draw_us,
            text_us = timings.text_us,
            geometry_us = timings.geometry_us,
            upload_us = timings.upload_us,
            submit_us = timings.submit_us,
            surface_us = timings.surface_us,
            bind_us = timings.bind_us,
            encode_us = timings.encode_us,
            queue_us = timings.queue_us,
            present_us = timings.present_us,
            commands = timings.commands,
            steps = timings.steps,
            rect_steps = timings.rect_steps,
            image_steps = timings.image_steps,
            text_steps = timings.text_steps,
            rect_instances = timings.rect_instances,
            image_instances = timings.image_instances,
            text_instances = timings.text_instances,
            "skin reload first render timings"
        );
    }
}

fn log_render_status(render_status: Result<RenderSurfaceStatus>) {
    match render_status {
        Ok(RenderSurfaceStatus::Rendered)
        | Ok(RenderSurfaceStatus::SkippedNoSurface)
        | Ok(RenderSurfaceStatus::SkippedZeroSize) => {}
        Ok(RenderSurfaceStatus::Reconfigured) => {
            tracing::debug!("renderer surface reconfigured");
        }
        Ok(RenderSurfaceStatus::TimedOut) => {
            tracing::debug!("renderer surface acquisition timed out");
        }
        Err(error) => {
            tracing::error!(%error, "failed to present render scene");
        }
    }
}

fn frame_profile_sample(
    kind: Option<FrameProfileKind>,
    video_us: u128,
    video_profile: SkinVideoFrameProfile,
    snapshot_us: u128,
    render_us: u128,
    render_timings: Option<bmz_render::renderer::RenderFrameTimings>,
) -> Option<SceneFrameProfileSample> {
    let kind = kind?;
    Some(SceneFrameProfileSample {
        kind,
        video_us,
        video_profile,
        snapshot_us,
        render_us,
        render_timings,
    })
}
