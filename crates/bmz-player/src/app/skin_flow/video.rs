use super::*;

impl WinitApp {
    pub(super) fn update_current_skin_video_sources(
        &mut self,
        profiling: bool,
    ) -> SkinVideoFrameProfile {
        let mut profile = SkinVideoFrameProfile::default();
        let Some((kind, elapsed_us)) = self.current_skin_video_context() else {
            return profile;
        };
        let needs_runtime_state = self.renderer.last_plan().is_none()
            && self
                .skin
                .skin_video_sources
                .get(&kind)
                .is_some_and(|sources| skin_video_sources_need_runtime_state(sources));
        // 実行時 op 条件 (例: リザルトのランク別 BG) で実際に表示されるソースだけを
        // デコードする。実行時 op を持つソースが無い場合は state 構築自体を避ける。
        let runtime_state = if needs_runtime_state {
            self.renderer
                .last_scene()
                .and_then(|scene| self.current_skin_video_draw_state_for_scene(kind, scene))
        } else {
            None
        };
        let planned_visibility = self.renderer.last_plan().and_then(|plan| {
            self.skin.skin_video_sources.get(&kind).map(|sources| {
                sources
                    .iter()
                    .map(|source| skin_video_texture_visible_in_plan(plan, source.texture))
                    .collect::<Vec<_>>()
            })
        });
        let Some(sources) = self.skin.skin_video_sources.get_mut(&kind) else {
            return profile;
        };
        for (index, source) in sources.iter_mut().enumerate() {
            if source.failed {
                continue;
            }
            // Use the already evaluated plan as the authority, including
            // songlist/imageset indirection and dynamic Lua draw/timer gates.
            // Re-evaluating draw here would mutate stateful closures twice.
            let visible = planned_visibility.as_ref().map_or_else(
                || {
                    source.active
                        && runtime_state
                            .as_ref()
                            .is_none_or(|state| skin_video_source_runtime_visible(source, state))
                },
                |visible| visible[index],
            );
            if !visible {
                // 現在のシーン状態では非表示。デコード中なら止めて開放する。
                if source.decoder.is_some() {
                    source.decoder = None;
                    source.last_pts = None;
                }
                continue;
            }
            profile.active_sources += 1;
            profile.visible_sources += 1;
            if source.decoder.is_none() {
                match VideoBgaDecoder::open_following_playback_time(&source.path) {
                    Ok(decoder) => {
                        // 非同期 skin decode 完了後など、リザルト開始から時間が経ってから
                        // decoder を開くと video_offset が大きくなり、clocked decode が
                        // 1 周デコードして loop_base を追いつかせるまでフレームを出せない。
                        // 開いた時点の elapsed を loop 原点に合わせ、常に offset ≈ 0 から始める。
                        source.loop_start_us = elapsed_us;
                        source.last_pts = None;
                        tracing::info!(
                            kind = ?kind,
                            texture_id = source.texture.0,
                            path = %source.path.display(),
                            "opened skin video source decoder"
                        );
                        source.decoder = Some(decoder);
                        profile.opened += 1;
                    }
                    Err(error) => {
                        tracing::warn!(
                            kind = ?kind,
                            texture_id = source.texture.0,
                            path = %source.path.display(),
                            %error,
                            "failed to open skin video source"
                        );
                        source.failed = true;
                        continue;
                    }
                }
            }

            let Some(decoder) = source.decoder.as_mut() else {
                continue;
            };
            let video_offset_us = elapsed_us.saturating_sub(source.loop_start_us);
            let poll_start = profiling.then(Instant::now);
            let frame = decoder.poll_frame(video_offset_us);
            if let Some(start) = poll_start {
                profile.poll_us += start.elapsed().as_micros();
            }
            if let Some(frame) = frame
                && source.last_pts != Some(frame.pts_us)
            {
                let pts = frame.pts_us;
                let upload_start = profiling.then(Instant::now);
                match self.renderer.upsert_rgba_texture_ref(
                    TextureId(source.texture.0),
                    frame.width,
                    frame.height,
                    &frame.rgba,
                ) {
                    Ok(()) => {
                        source.last_pts = Some(pts);
                        profile.uploaded_frames += 1;
                    }
                    Err(error) => {
                        tracing::warn!(
                            kind = ?kind,
                            texture_id = source.texture.0,
                            path = %source.path.display(),
                            %error,
                            "failed to upload skin video source frame"
                        );
                    }
                }
                if let Some(start) = upload_start {
                    profile.upload_us += start.elapsed().as_micros();
                }
            }
            if source.decoder.as_ref().is_some_and(VideoBgaDecoder::is_finished) {
                source.decoder = None;
                source.last_pts = None;
                source.loop_start_us = elapsed_us;
            }
        }
        profile
    }

    pub(super) fn current_skin_video_context(&self) -> Option<(SkinKind, i64)> {
        match self.view_state() {
            AppViewState::Select => Some((SkinKind::Select, self.select_time().0)),
            AppViewState::Decide => self
                .play
                .pending_decide
                .as_ref()
                .map(|decide| (SkinKind::Decide, elapsed_since(decide.started_at).0)),
            AppViewState::Play => Some((SkinKind::Play, self.play_elapsed_time().0)),
            AppViewState::Result => {
                Some((SkinKind::Result, elapsed_since(self.result.result_scene_started_at).0))
            }
        }
    }

    /// 動画ソースの実行時可視判定に使う `SkinDrawState` を、現在のシーン用に構築する。
    pub(super) fn current_skin_video_draw_state_for_scene(
        &self,
        kind: SkinKind,
        scene: &AppSceneSnapshot,
    ) -> Option<bmz_render::skin::SkinDrawState> {
        match kind {
            SkinKind::Play => {
                let AppSceneSnapshot::Play(snapshot) = scene else {
                    return None;
                };
                let play_skin_document = self.renderer.play_skin_document();
                Some(play_skin_video_draw_state(
                    snapshot,
                    play_skin_document.map(|document| document.h),
                    play_skin_document.and_then(|document| document.primary_note_lane_height_px()),
                    play_skin_document.map_or(0, |document| document.input),
                ))
            }
            SkinKind::Result => {
                let AppSceneSnapshot::Result(snapshot) = scene else {
                    return None;
                };
                let ranktime = self
                    .skin
                    .skin_video_sources
                    .get(&SkinKind::Result)
                    .and_then(|sources| sources.first())
                    .map_or(0, |source| source.result_ranktime_ms);
                Some(bmz_render::plan::result_skin_draw_state(snapshot, ranktime))
            }
            _ => None,
        }
    }
}
