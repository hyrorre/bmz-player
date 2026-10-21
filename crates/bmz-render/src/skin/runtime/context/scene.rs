use super::*;

impl Default for SkinContext {
    fn default() -> Self {
        Self {
            manifest: default_skin_manifest(),
            document: None,
            lua_draw_runtime: None,
            document_sources: HashMap::new(),
            runtime_document_sources: Arc::new(Mutex::new(HashMap::new())),
            select_render_cache: Arc::new(Mutex::new(SelectRenderCache::default())),
            select_settings_dest_index: Arc::new(
                crate::select_settings_dest::SelectSettingsDestIndex::default(),
            ),
            result_render_cache: Arc::new(Mutex::new(ResultRenderCache::default())),
            playfield_clip_candidate: false,
            lua_act_clicks: false,
            lua_act_frame_state: Arc::default(),
        }
    }
}

impl SkinContext {
    pub fn from_manifest(manifest: SkinManifest) -> Self {
        Self {
            manifest,
            document: None,
            lua_draw_runtime: None,
            document_sources: HashMap::new(),
            runtime_document_sources: Arc::new(Mutex::new(HashMap::new())),
            select_render_cache: Arc::new(Mutex::new(SelectRenderCache::default())),
            select_settings_dest_index: Arc::new(
                crate::select_settings_dest::SelectSettingsDestIndex::default(),
            ),
            result_render_cache: Arc::new(Mutex::new(ResultRenderCache::default())),
            playfield_clip_candidate: false,
            lua_act_clicks: false,
            lua_act_frame_state: Arc::default(),
        }
    }

    pub fn from_manifest_and_document(
        manifest: SkinManifest,
        document: SkinDocument,
        document_sources: impl IntoIterator<Item = SkinDocumentTexture>,
    ) -> Self {
        let select_settings_dest_index =
            Arc::new(crate::select_settings_dest::build_select_settings_dest_index(&document));
        let document_sources: HashMap<_, _> =
            document_sources.into_iter().map(|source| (source.source_id.clone(), source)).collect();
        let runtime_document_sources = Arc::new(Mutex::new(document_sources.clone()));
        let playfield_clip_candidate = super::play::notes_clip_candidate(&document);
        let lua_act_clicks = document_has_lua_acts(&document);
        Self {
            manifest,
            document: Some(document),
            lua_draw_runtime: None,
            document_sources,
            runtime_document_sources,
            select_render_cache: Arc::new(Mutex::new(SelectRenderCache::default())),
            select_settings_dest_index,
            result_render_cache: Arc::new(Mutex::new(ResultRenderCache::default())),
            playfield_clip_candidate,
            lua_act_clicks,
            lua_act_frame_state: Arc::default(),
        }
    }

    pub fn manifest(&self) -> &SkinManifest {
        &self.manifest
    }

    pub fn document(&self) -> Option<&SkinDocument> {
        self.document.as_ref()
    }

    pub fn set_lua_draw_runtime(&mut self, runtime: Option<Arc<dyn SkinLuaDrawRuntime>>) {
        self.lua_draw_runtime = runtime;
    }

    pub fn begin_frame(&self) {
        if let Some(runtime) = &self.lua_draw_runtime {
            runtime.begin_frame();
        }
    }

    /// Queue a clicked Lua `act`; false when the event is not one or no runtime exists.
    pub fn queue_lua_act(&self, event_id: i32, arg1: i32, arg2: i32) -> bool {
        let Some(callback_id) = lua_act_callback_id(event_id) else { return false };
        self.lua_draw_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.queue_act(callback_id, arg1, arg2))
    }

    /// Effects requested by this skin's Lua `act` / event actions.
    pub fn take_lua_effects(&self) -> Vec<SkinLuaEffect> {
        self.lua_draw_runtime.as_ref().map(|runtime| runtime.take_effects()).unwrap_or_default()
    }

    /// Size of the installed source `source_id`.
    pub fn source_size(&self, source_id: &str) -> Option<SkinImageSize> {
        self.document_sources.get(source_id).map(|source| source.source_size)
    }

    /// Corrects the size of every source drawn from `texture`, for example after a sprite
    /// sheet was laid out for a GPU that attached after the skin was installed.
    pub fn update_texture_source_size(&mut self, texture: SkinTextureId, size: SkinImageSize) {
        let mut changed = false;
        for source in self.document_sources.values_mut() {
            if source.texture == texture && source.source_size != size {
                source.source_size = size;
                changed = true;
            }
        }
        if let Ok(mut sources) = self.runtime_document_sources.lock() {
            for source in sources.values_mut() {
                if source.texture == texture && source.source_size != size {
                    source.source_size = size;
                    changed = true;
                }
            }
        }
        if changed {
            // Cached render items were laid out with the provisional size.
            self.select_render_cache = Arc::new(Mutex::new(SelectRenderCache::default()));
            self.result_render_cache = Arc::new(Mutex::new(ResultRenderCache::default()));
        }
    }

    pub fn texture_ids(&self) -> Vec<SkinTextureId> {
        let mut ids: Vec<_> = self.document_sources.values().map(|source| source.texture).collect();
        if let Ok(sources) = self.runtime_document_sources.lock() {
            ids.extend(sources.values().map(|source| source.texture));
        }
        ids
    }

    /// Whether the skin itself shows BMS chart text: a text `ref` / `value` names
    /// 20100 / 20103, or a Lua callback has read them while drawing.
    pub fn shows_chart_text(&self) -> bool {
        self.document.as_ref().is_some_and(|document| document.references_chart_text())
            || self.lua_draw_runtime.as_ref().is_some_and(|runtime| runtime.chart_text_read())
    }

    pub(super) fn state_with_lua_runtime(
        &self,
        state: &SkinDrawState,
        text: &SkinTextState<'_>,
    ) -> SkinDrawState {
        let mut state = state.clone();
        let Some(runtime) = self.lua_draw_runtime.as_ref() else {
            return state;
        };
        let enabled_options: Arc<[i32]> = self
            .document
            .as_ref()
            .map(|document| Arc::from(document.enabled_options()))
            .unwrap_or_else(|| Arc::from([]));
        let text_values = Arc::new(lua_main_state_text_values(&state, text));
        state.lua_custom_timer_us =
            runtime.advance_custom_timers(&state, &enabled_options, &text_values);
        state.lua_runtime = Some(SkinLuaRuntimeContext {
            runtime: Arc::clone(runtime),
            enabled_options,
            text_values,
        });
        if self.lua_act_clicks
            && let Ok(mut frame) = self.lua_act_frame_state.lock()
        {
            *frame = Some(state.clone());
        }
        state
    }

    /// The Lua `act` under a Play / Decide click, tested against the last drawn
    /// frame. These scenes have no built-in click events, so an object with a
    /// built-in `act` on top takes the click without an action.
    pub fn last_frame_lua_act_hit(&self, x: f32, y: f32) -> Option<SkinClickHit> {
        let runtime = self.lua_draw_runtime.as_ref()?;
        let mut state = self.lua_act_frame_state.lock().ok()?.clone()?;
        apply_last_frame_lua(&mut state, runtime);
        let hit = self.document.as_ref()?.result_click_hit(&state, x, y)?;
        let SkinClickTarget::Event { event_id, .. } = hit.target else { return None };
        lua_act_callback_id(event_id).map(|_| hit)
    }

    pub fn set_user_selected_options(&mut self, enabled_options: Vec<i32>) -> bool {
        let Some(document) = &mut self.document else {
            return false;
        };
        document.user_selected_options = Some(enabled_options);
        // A cloned context may still use the old document options, so detach
        // rather than clearing the shared caches in place.
        self.select_render_cache = Arc::new(Mutex::new(SelectRenderCache::default()));
        self.result_render_cache = Arc::new(Mutex::new(ResultRenderCache::default()));
        true
    }

    pub fn with_play_graphs(
        &self,
        judge_graph_density: Vec<u8>,
        bpm_graph_segments: Vec<crate::chart_graph::BpmGraphSegment>,
    ) -> Self {
        let mut cloned = self.clone();
        if let Some(document) = &mut cloned.document {
            document.play_judge_graph_density = judge_graph_density;
            document.play_bpm_graph_segments = bpm_graph_segments;
        }
        cloned
    }

    pub fn with_result_graphs(&self, graph: &crate::snapshot::ResultGraphSnapshot) -> Self {
        let mut cloned = self.clone();
        if let Some(document) = &mut cloned.document {
            document.play_judge_graph_density = graph.judge_graph_density.clone();
            document.play_bpm_graph_segments = graph.bpm_graph_segments.clone();
            document.result_gauge_graph_points = graph.gauge_points.clone();
            document.result_timing_points = graph.timing_points.clone();
            document.result_judge_graph_buckets = graph.judge_graph_buckets.clone();
            document.result_note_graph_buckets = graph.note_graph_buckets.clone();
            document.result_early_late_graph_buckets = graph.early_late_graph_buckets.clone();
            document.result_timing_distribution = graph.timing_distribution.clone();
        }
        cloned
    }

    pub fn static_document_items(&self) -> Vec<SkinRenderItem> {
        self.static_document_items_for_state(&SkinDrawState::default())
    }

    pub fn static_document_items_for_state(&self, state: &SkinDrawState) -> Vec<SkinRenderItem> {
        self.static_document_items_for_state_and_text(state, &SkinTextState::default())
    }

    pub fn static_document_items_for_state_and_text(
        &self,
        state: &SkinDrawState,
        text: &SkinTextState<'_>,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        let runtime_sources = static_runtime_document_sources(
            &self.document_sources,
            &self.runtime_document_sources,
            state,
        );
        let state = self.state_with_lua_runtime(state, text);
        if let Some(mut cache) = RenderCacheLease::take(&self.result_render_cache) {
            return document.static_render_items_with_graphs_cached(
                &runtime_sources,
                &state,
                text,
                SkinRuntimeGraphs::from_document(document),
                Some(&mut cache),
            );
        }
        document.static_render_items(&runtime_sources, &state, text)
    }

    pub fn static_document_items_for_result_state_and_text(
        &self,
        graph: &Arc<crate::snapshot::ResultGraphSnapshot>,
        state: &SkinDrawState,
        text: &SkinTextState<'_>,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        let runtime_sources = static_runtime_document_sources(
            &self.document_sources,
            &self.runtime_document_sources,
            state,
        );
        let state = self.state_with_lua_runtime(state, text);
        if let Some(mut cache) = RenderCacheLease::take(&self.result_render_cache) {
            cache.prepare_gauge_graph(graph);
            return document.static_render_items_with_graphs_cached(
                &runtime_sources,
                &state,
                text,
                SkinRuntimeGraphs::from_result_graph(graph.as_ref()),
                Some(&mut cache),
            );
        }
        document.static_render_items_with_graphs(
            &runtime_sources,
            &state,
            text,
            SkinRuntimeGraphs::from_result_graph(graph.as_ref()),
        )
    }

    pub fn select_document_items(&self, snapshot: &SelectSnapshot) -> Vec<SkinRenderItem> {
        self.select_document_items_with_dynamic_timers(snapshot, None)
    }

    pub fn select_document_items_with_dynamic_timers(
        &self,
        snapshot: &SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        let runtime_sources = select_runtime_document_sources(
            &self.document_sources,
            &self.runtime_document_sources,
            snapshot,
        );
        if let Some(mut cache) = RenderCacheLease::take(&self.select_render_cache) {
            return document.select_render_items_with_dynamic_timers_cached(
                &runtime_sources,
                snapshot,
                dynamic_timers,
                &self.select_settings_dest_index,
                self.lua_draw_runtime.clone(),
                Some(&mut cache),
            );
        }
        document.select_render_items_with_dynamic_timers(
            &runtime_sources,
            snapshot,
            dynamic_timers,
            &self.select_settings_dest_index,
            self.lua_draw_runtime.clone(),
        )
    }

    pub fn select_click_hit(
        &self,
        snapshot: &SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        let document = self.document.as_ref()?;
        document.select_click_hit_with_lua(
            &self.document_sources,
            snapshot,
            &self.select_settings_dest_index,
            self.lua_draw_runtime.as_ref(),
            x,
            y,
        )
    }

    pub fn select_search_input_rect(&self, snapshot: &SelectSnapshot) -> Option<Rect> {
        self.document.as_ref()?.select_search_input_rect(snapshot, &self.select_settings_dest_index)
    }

    pub fn result_click_hit(&self, state: &SkinDrawState, x: f32, y: f32) -> Option<SkinClickHit> {
        let document = self.document.as_ref()?;
        let Some(runtime) = &self.lua_draw_runtime else {
            return document.result_click_hit(state, x, y);
        };
        let mut state = state.clone();
        apply_last_frame_lua(&mut state, runtime);
        document.result_click_hit(&state, x, y)
    }

    pub fn result_slider_hit(
        &self,
        state: &SkinDrawState,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        self.document.as_ref()?.result_slider_hit(state, x, y)
    }

    pub fn select_slider_hit(
        &self,
        snapshot: &SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let document = self.document.as_ref()?;
        document.select_slider_hit(snapshot, &self.select_settings_dest_index, x, y)
    }

    pub fn select_slider_drag(
        &self,
        snapshot: &SelectSnapshot,
        slider_type: i32,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let document = self.document.as_ref()?;
        document.select_slider_drag(snapshot, &self.select_settings_dest_index, slider_type, x, y)
    }

    /// Whether the point is over the embedded skin settings of an LR2 select skin.
    pub fn select_lr2_skin_settings_contains(
        &self,
        snapshot: &SelectSnapshot,
        x: f32,
        y: f32,
    ) -> bool {
        self.document.as_ref().is_some_and(|document| {
            document.select_lr2_skin_settings_contains(
                snapshot,
                &self.select_settings_dest_index,
                x,
                y,
            )
        })
    }

    /// 静的 destination を `{"id":"notes"}` マーカーと `timer: 3` (FAILED) で分割して返す。
    /// `.0` はノーツ背面、`.1` はノーツ前面、`.2` は閉店/暗転オーバーレイ（最前面）。
    pub fn static_document_items_split_for_state_and_text(
        &self,
        state: &SkinDrawState,
        text: &SkinTextState<'_>,
    ) -> (Vec<SkinRenderItem>, Vec<SkinRenderItem>, Vec<SkinRenderItem>) {
        let Some(document) = &self.document else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let runtime_sources = static_runtime_document_sources(
            &self.document_sources,
            &self.runtime_document_sources,
            state,
        );
        let state = self.state_with_lua_runtime(state, text);
        if let Some(mut cache) = RenderCacheLease::take(&self.result_render_cache) {
            return document.static_render_items_split_with_graphs(
                &runtime_sources,
                &state,
                text,
                SkinRuntimeGraphs::from_document(document),
                Some(&mut cache),
            );
        }
        document.static_render_items_split(&runtime_sources, &state, text)
    }

    pub fn static_document_play_items_split_for_state_and_text(
        &self,
        state: &SkinDrawState,
        text: &SkinTextState<'_>,
        play_judge_graph_density: &[u8],
        play_bpm_graph_segments: &[crate::chart_graph::BpmGraphSegment],
    ) -> (Vec<SkinRenderItem>, Vec<SkinRenderItem>, Vec<SkinRenderItem>) {
        let Some(document) = &self.document else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        let runtime_sources = static_runtime_document_sources(
            &self.document_sources,
            &self.runtime_document_sources,
            state,
        );
        let state = self.state_with_lua_runtime(state, text);
        // The cache only retains document structure and static destinations.
        // draw/value callbacks still run in destination order on every frame.
        if let Some(mut cache) = RenderCacheLease::take(&self.result_render_cache) {
            return document.static_render_items_split_with_graphs(
                &runtime_sources,
                &state,
                text,
                SkinRuntimeGraphs::from_document_with_play_graphs(
                    document,
                    play_judge_graph_density,
                    play_bpm_graph_segments,
                ),
                Some(&mut cache),
            );
        }
        document.static_render_items_split_with_graphs(
            &runtime_sources,
            &state,
            text,
            SkinRuntimeGraphs::from_document_with_play_graphs(
                document,
                play_judge_graph_density,
                play_bpm_graph_segments,
            ),
            None,
        )
    }
}

/// Own the cache while evaluating a frame so Lua never runs under its mutex.
/// A concurrent/reentrant frame can use the empty replacement cache. Restoring
/// either cache only affects reuse, not the live state or callback ordering.
struct RenderCacheLease<'a, T: Default> {
    slot: &'a Mutex<T>,
    value: T,
}

impl<'a, T: Default> RenderCacheLease<'a, T> {
    fn take(slot: &'a Mutex<T>) -> Option<Self> {
        let value = std::mem::take(&mut *slot.lock().ok()?);
        Some(Self { slot, value })
    }
}

impl<T: Default> std::ops::Deref for RenderCacheLease<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: Default> std::ops::DerefMut for RenderCacheLease<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: Default> Drop for RenderCacheLease<'_, T> {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.slot.lock() {
            *slot = std::mem::take(&mut self.value);
        }
    }
}

fn document_has_lua_acts(document: &SkinDocument) -> bool {
    let lua_act = |act: Option<i32>| act.is_some_and(|id| lua_act_callback_id(id).is_some());
    document.image.iter().any(|image| lua_act(image.act))
        || document.imageset.iter().any(|imageset| lua_act(imageset.act))
        || document.destination.iter().any(|entry| match entry {
            DestinationListEntry::Single(destination) => lua_act(destination.act),
            DestinationListEntry::Conditional { destinations, .. } => {
                destinations.iter().any(|destination| lua_act(destination.act))
            }
        })
}

#[cfg(test)]
mod cache_lease_tests {
    use super::*;

    #[test]
    fn render_cache_is_unlocked_during_reentry_and_restored_on_unwind() {
        let slot = Mutex::new(vec![1]);
        let result = std::panic::catch_unwind(|| {
            let mut outer = RenderCacheLease::take(&slot).unwrap();
            assert!(slot.try_lock().is_ok());
            {
                let mut nested = RenderCacheLease::take(&slot).unwrap();
                assert!(nested.is_empty());
                nested.push(2);
            }
            outer.push(3);
            panic!("callback failed");
        });
        assert!(result.is_err());
        assert_eq!(*slot.lock().unwrap(), [1, 3]);
    }
}
