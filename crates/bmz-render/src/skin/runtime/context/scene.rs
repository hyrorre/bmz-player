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
        Self {
            manifest,
            document: Some(document),
            lua_draw_runtime: None,
            document_sources,
            runtime_document_sources,
            select_render_cache: Arc::new(Mutex::new(SelectRenderCache::default())),
            select_settings_dest_index,
            result_render_cache: Arc::new(Mutex::new(ResultRenderCache::default())),
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

    pub fn texture_ids(&self) -> Vec<SkinTextureId> {
        let mut ids: Vec<_> = self.document_sources.values().map(|source| source.texture).collect();
        if let Ok(sources) = self.runtime_document_sources.lock() {
            ids.extend(sources.values().map(|source| source.texture));
        }
        ids
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
        state.lua_runtime = Some(SkinLuaRuntimeContext {
            runtime: Arc::clone(runtime),
            enabled_options,
            text_values: Arc::new(lua_main_state_text_values(&state, text)),
        });
        state
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
        document.select_click_hit(
            &self.document_sources,
            snapshot,
            &self.select_settings_dest_index,
            x,
            y,
        )
    }

    pub fn select_search_input_rect(&self, snapshot: &SelectSnapshot) -> Option<Rect> {
        self.document.as_ref()?.select_search_input_rect(snapshot, &self.select_settings_dest_index)
    }

    pub fn result_click_hit(&self, state: &SkinDrawState, x: f32, y: f32) -> Option<SkinClickHit> {
        self.document.as_ref()?.result_click_hit(state, x, y)
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
