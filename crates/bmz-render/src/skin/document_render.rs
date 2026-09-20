use super::*;

/// `SkinDocument` (bmz-skin-document) に対する描画評価の拡張 trait。
///
/// document スキーマ本体は `bmz-skin-document` crate へ移動したため、
/// `SkinDrawState` / `SkinRenderItem` 等の描画型に依存する評価メソッドは
/// foreign type への inherent impl ができず、この拡張 trait で提供する。
/// 実装は `impl SkinDocumentRenderExt for SkinDocument` の 1 つだけを想定する。
///
/// 旧 inherent impl の private ヘルパーメソッドも trait へ機械的に移した
/// ため、`SkinRuntimeGraphs` 等の crate 内 private 型がシグネチャに現れる。
/// これらは外部から呼べない (引数型を名指しできない) ので lint を許可する。
#[allow(private_interfaces)]
pub trait SkinDocumentRenderExt {
    fn static_image_render_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem>;

    fn static_render_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
        text_state: &SkinTextState<'_>,
    ) -> Vec<SkinRenderItem>;

    fn static_render_items_with_graphs(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
        text_state: &SkinTextState<'_>,
        runtime_graphs: SkinRuntimeGraphs<'_>,
    ) -> Vec<SkinRenderItem>;

    fn static_render_items_with_graphs_cached(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
        text_state: &SkinTextState<'_>,
        runtime_graphs: SkinRuntimeGraphs<'_>,
        cache: Option<&mut ResultRenderCache>,
    ) -> Vec<SkinRenderItem>;

    fn static_render_items_split(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
        text_state: &SkinTextState<'_>,
    ) -> (Vec<SkinRenderItem>, Vec<SkinRenderItem>, Vec<SkinRenderItem>);

    fn static_render_items_split_with_graphs(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
        text_state: &SkinTextState<'_>,
        runtime_graphs: SkinRuntimeGraphs<'_>,
        cache: Option<&mut ResultRenderCache>,
    ) -> (Vec<SkinRenderItem>, Vec<SkinRenderItem>, Vec<SkinRenderItem>);

    fn result_judge_pie_destination_item(
        &self,
        destination: &SkinDestinationDef,
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn destination_looks_like_pre_notes_judge_line(
        &self,
        destination: &SkinDestinationDef,
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        next_destination: Option<&SkinDestinationDef>,
    ) -> bool;

    fn disappear_line_for_lane_cover_clip(&self) -> Option<(i32, bool)>;

    fn should_clip_image_at_disappear_line(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
    ) -> bool;

    fn should_skip_lift_lane_cover_render(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
    ) -> bool;

    fn link_lift_for_lane_cover_clip(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
        link_lift: bool,
    ) -> bool;

    fn resolve_destination_items(
        &self,
        destination_index: usize,
        destination: &SkinDestinationDef,
        context: DestinationResolveContext<'_, '_>,
    ) -> Option<Vec<SkinRenderItem>>;

    #[doc(hidden)]
    fn resolve_image_destination_items(
        &self,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        images: &SkinImageLookup<'_>,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Option<Vec<SkinRenderItem>>>;

    #[doc(hidden)]
    fn resolve_bga_destination_items(
        &self,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
    ) -> Option<Option<Vec<SkinRenderItem>>>;

    #[doc(hidden)]
    fn resolve_imageset_destination_items(
        &self,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        images: &SkinImageLookup<'_>,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Option<Vec<SkinRenderItem>>>;

    fn resolve_offset_destination_items(
        &self,
        destination: &SkinDestinationDef,
        offset: (i32, i32),
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        text_state: &SkinTextState<'_>,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>>;

    fn select_render_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
    ) -> Vec<SkinRenderItem>;

    fn select_render_items_with_dynamic_timers(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        lua_draw_runtime: Option<Arc<dyn SkinLuaDrawRuntime>>,
    ) -> Vec<SkinRenderItem>;

    fn select_render_items_with_dynamic_timers_cached(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        lua_draw_runtime: Option<Arc<dyn SkinLuaDrawRuntime>>,
        cache: Option<&mut SelectRenderCache>,
    ) -> Vec<SkinRenderItem>;

    fn select_draw_state<'a>(
        &self,
        snapshot: &'a SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
    ) -> (SkinDrawState, Option<&'a SelectRowSnapshot>);

    fn select_search_input_rect(
        &self,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
    ) -> Option<Rect>;

    fn select_click_hit(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit>;

    fn result_click_hit(&self, state: &SkinDrawState, x: f32, y: f32) -> Option<SkinClickHit>;

    fn result_slider_hit(&self, state: &SkinDrawState, x: f32, y: f32) -> Option<SkinSliderHit>;

    fn select_slider_hit(
        &self,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit>;

    fn select_click_hits(
        &self,
        _sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
    ) -> Vec<SkinClickHit>;

    fn select_songlist_click_hits(
        &self,
        snapshot: &SelectSnapshot,
        enabled_options: &[i32],
        state: &SkinDrawState,
    ) -> Vec<SkinClickHit>;

    fn apply_select_songlist_render_row_state(
        state: &mut SkinDrawState,
        row: &SelectRowSnapshot,
        selected_replay_slot: Option<u8>,
    );

    fn apply_select_songlist_click_row_state(
        state: &mut SkinDrawState,
        row: &SelectRowSnapshot,
        selected_replay_slot: Option<u8>,
    );

    fn click_target_for_destination(
        &self,
        destination: &SkinDestinationDef,
        images: &SkinImageLookup<'_>,
    ) -> Option<SkinClickTarget>;

    fn destination_click_rect(
        &self,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: &SkinDrawState,
    ) -> Option<Rect>;

    fn destination_slider_hit(
        &self,
        slider: &SkinSliderDef,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: &SkinDrawState,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit>;

    fn select_songlist_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem>;

    fn apply_select_songlist_scroll_to_frame(
        &self,
        frame: &mut ResolvedSkinFrame,
        songlist: &SkinSongListDef,
        slot: i32,
        enabled_options: &[i32],
        state: &SkinDrawState,
        direction: i32,
        progress: f32,
    );

    fn select_songlist_all_child_items(
        &self,
        entries: &[DestinationListEntry],
        row: &SelectRowSnapshot,
        row_origin: (i32, i32),
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn select_folder_distribution_graph_render_items(
        &self,
        row: &SelectRowSnapshot,
        graph: &SkinGraphDef,
        destination: &SkinDestinationDef,
        row_origin: (i32, i32),
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn select_songlist_level_items(
        &self,
        entries: &[DestinationListEntry],
        row: &SelectRowSnapshot,
        row_origin: (i32, i32),
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &mut SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn select_songlist_child_items_by_index(
        &self,
        entries: &[DestinationListEntry],
        index: usize,
        row_origin: (i32, i32),
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn select_songlist_text_items(
        &self,
        row: &SelectRowSnapshot,
        row_origin: (i32, i32),
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn select_bar_item(
        &self,
        row: &SelectRowSnapshot,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_image_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_processed_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_ln_start_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        mode: LongNoteMode,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_ln_end_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        mode: LongNoteMode,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn ln_body_image_id<'a>(
        &self,
        note: &'a SkinNoteSetDef,
        index: usize,
        pressing: bool,
    ) -> Option<&'a String>;

    fn hcn_body_image_id<'a>(
        &self,
        note: &'a SkinNoteSetDef,
        index: usize,
        state: LongBodyState,
    ) -> Option<&'a String>;

    fn note_long_body_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        mode: LongNoteMode,
        state: LongBodyState,
        draw_state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_mine_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_height_for_lane(&self, lane: Lane, key_mode: KeyMode) -> Option<f32>;

    fn note_part_sprite(
        &self,
        image_id: &str,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<NoteSprite>;

    fn note_part_render_item(
        &self,
        image_id: &str,
        rect: Rect,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn note_group_render_items(
        &self,
        note_y: f32,
        key_mode: KeyMode,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn note_line_render_items(
        &self,
        destinations: &[SkinDestinationDef],
        note_y: f32,
        key_mode: KeyMode,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem>;

    fn note_lane_area(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        enabled_options: &[i32],
    ) -> Option<Rect>;

    fn primary_note_lane_height_px(&self) -> Option<i32>;

    fn notes_destination_offset(&self, state: &SkinDrawState) -> SkinOffsetValue;

    fn apply_notes_offset_to_rect(&self, rect: Rect, state: &SkinDrawState) -> Rect;

    fn apply_notes_offset_to_long_body_rect(&self, rect: Rect, state: &SkinDrawState) -> Rect;

    fn gauge_render_items(
        &self,
        gauge: f32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>>;

    fn destination_uses_skin_gauge_bar_render(&self, destination: &SkinDestinationDef) -> bool;

    fn destination_uses_skin_gauge_overlay_render(&self, destination: &SkinDestinationDef) -> bool;

    fn skin_gauge_for_destination(&self, destination: &SkinDestinationDef)
    -> Option<&SkinGaugeDef>;

    fn resolve_gauge_destination_items(
        &self,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>>;

    fn judge_render_items(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>>;

    fn judge_render_items_with_offsets(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
        skin_offsets: &SkinOffsetValues,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>>;

    fn judge_render_items_for_def(
        &self,
        judge: &SkinJudgeDef,
        judge_index: usize,
        combo: u32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: &SkinDrawState,
    ) -> Option<Vec<SkinRenderItem>>;

    fn beatoraja_judge_number_dst_x(dst_w: i32, digit: i32) -> i32;

    fn apply_beatoraja_judge_number_dst_x(frame: &mut ResolvedSkinFrame, digit: i32);

    fn value_number_length(&self, value_id: &str, number: i64, frame: ResolvedSkinFrame) -> i32;

    fn judge_image_render_item(
        &self,
        judge: &str,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn value_number_render_items(
        &self,
        value_id: &str,
        number: i64,
        base_frame: ResolvedSkinFrame,
        frame: ResolvedSkinFrame,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        compact_digits: bool,
        align_override: Option<i32>,
        signed_render: SignedNumberRender,
    ) -> Vec<SkinRenderItem>;

    fn value_digit_texture_region(
        value: &SkinValueDef,
        digit: u32,
        elapsed_ms: i32,
        source_size: SkinImageSize,
        cell_width_px: f32,
        cell_height_px: f32,
        divx: i32,
        divy: i32,
    ) -> TextureRegion;

    fn gauge_image_render_item(
        &self,
        image_id: &str,
        rect: Rect,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        tint: Color,
        blend: BlendMode,
        linear_filter: bool,
    ) -> Option<SkinRenderItem>;

    #[cfg(test)]
    fn text_render_item(
        &self,
        text: &SkinTextDef,
        frame: ResolvedSkinFrame,
        state: &SkinTextState<'_>,
    ) -> Option<SkinRenderItem>;

    fn text_render_item_with_draw_state(
        &self,
        text: &SkinTextDef,
        frame: ResolvedSkinFrame,
        draw_state: Option<&SkinDrawState>,
        state: &SkinTextState<'_>,
    ) -> Option<SkinRenderItem>;

    fn hiterror_visualizer_render_items(
        &self,
        visualizer: &SkinHitErrorVisualizerDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem>;

    fn gaugegraph_render_items(
        &self,
        destination_index: usize,
        graph: &SkinGaugeGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        points: &[crate::snapshot::ResultGaugeGraphPoint],
        cache: Option<&mut ResultRenderCache>,
    ) -> Vec<SkinRenderItem>;

    fn timing_visualizer_render_items(
        &self,
        visualizer: &SkinTimingVisualizerDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        timing_points: &[crate::snapshot::ResultTimingPoint],
    ) -> Vec<SkinRenderItem>;

    fn timing_distribution_graph_render_items(
        &self,
        graph: &SkinTimingDistributionGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        timing_points: &[crate::snapshot::ResultTimingPoint],
        timing_distribution: &crate::snapshot::ResultTimingDistribution,
    ) -> Vec<SkinRenderItem>;

    fn judgegraph_render_items(
        &self,
        destination_index: usize,
        graph: &SkinJudgeGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        elapsed_ms: i32,
        state: &SkinDrawState,
        runtime_graphs: SkinRuntimeGraphs<'_>,
        cache: Option<&mut ResultRenderCache>,
    ) -> Vec<SkinRenderItem>;

    fn density_judgegraph_render_items(
        &self,
        graph: &SkinJudgeGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        density: &[u8],
    ) -> Vec<SkinRenderItem>;

    fn select_note_distribution_graph_render_items(
        &self,
        row: &SelectRowSnapshot,
        graph: &SkinJudgeGraphDef,
        destination: &SkinDestinationDef,
        row_origin: (i32, i32),
        enabled_options: &[i32],
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem>;

    fn select_bpmgraph_row_render_items(
        &self,
        row: &SelectRowSnapshot,
        graph: &SkinBpmGraphDef,
        destination: &SkinDestinationDef,
        row_origin: (i32, i32),
        enabled_options: &[i32],
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem>;

    fn bpmgraph_render_items_with_segments(
        &self,
        graph: &SkinBpmGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        segments: &[crate::chart_graph::BpmGraphSegment],
    ) -> Vec<SkinRenderItem>;

    fn direct_source_image_render_item(
        &self,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn slider_render_item(
        &self,
        slider: &SkinSliderDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn hidden_cover_render_item(
        &self,
        cover: &SkinHiddenCoverDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;

    fn graph_render_item(
        &self,
        graph: &SkinGraphDef,
        frame: ResolvedSkinFrame,
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem>;
}

mod core;
mod graph;
mod play;
mod select;

// Rust requires one coherent impl block for a trait. The method groups live in
// scene-oriented macros so the public extension trait and its implementation
// remain behaviorally identical while the source is physically separated.
#[allow(private_interfaces)]
impl SkinDocumentRenderExt for SkinDocument {
    core::skin_document_render_core_static_methods!();
    core::skin_document_render_core_clip_methods!();
    core::skin_document_render_core_resolve_methods!();
    select::skin_document_render_select_render_methods!();
    select::skin_document_render_select_interaction_methods!();
    select::skin_document_render_select_songlist_methods!();
    select::skin_document_render_select_graph_methods!();
    select::skin_document_render_select_bar_methods!();
    play::skin_document_render_play_note_methods!();
    play::skin_document_render_play_lane_methods!();
    play::skin_document_render_play_gauge_methods!();
    play::skin_document_render_play_judge_methods!();
    play::skin_document_render_play_value_methods!();
    graph::skin_document_render_graph_text_methods!();
    graph::skin_document_render_graph_visualizer_methods!();
    graph::skin_document_render_graph_judge_methods!();
    graph::skin_document_render_graph_select_methods!();
    graph::skin_document_render_graph_image_methods!();
}
