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
        // 未表示の動画は可視になってから起動する。実行時 op を持つソースが
        // 無い場合は state 構築自体を避ける。
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
            let texture = TextureId(source.texture.0);
            update_skin_video_source(
                source,
                kind,
                elapsed_us,
                visible,
                profiling,
                &mut profile,
                |frame| {
                    self.renderer.upsert_rgba_texture_ref(
                        texture,
                        frame.width,
                        frame.height,
                        &frame.rgba,
                    )
                },
            );
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
