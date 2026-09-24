use super::*;

/// Renderer-facing interface for Lua runtime sidecars. Implementations own the
/// VM outside `SkinDocument`; the renderer only supplies a read-only frame state.
pub trait SkinLuaDrawRuntime: std::fmt::Debug + Send + Sync {
    fn begin_frame(&self) {}

    /// Run once, synchronously, with a read-only state shared by the enclosed
    /// callbacks. Callbacks using another state must still use that state.
    fn with_state(
        &self,
        _state: &SkinDrawState,
        _enabled_options: &[i32],
        _text_values: &BTreeMap<i32, String>,
        run: &mut dyn FnMut(),
    ) {
        run();
    }

    fn evaluate_draw(
        &self,
        callback_id: usize,
        state: &SkinDrawState,
        enabled_options: &[i32],
        text_values: &BTreeMap<i32, String>,
    ) -> bool;

    fn evaluate_number(
        &self,
        _callback_id: usize,
        _state: &SkinDrawState,
        _enabled_options: &[i32],
        _text_values: &BTreeMap<i32, String>,
    ) -> Option<f64> {
        None
    }

    fn evaluate_text(
        &self,
        _callback_id: usize,
        _state: &SkinDrawState,
        _enabled_options: &[i32],
        _text_values: &BTreeMap<i32, String>,
    ) -> Option<String> {
        None
    }
}

pub(in crate::skin) fn with_lua_render_state<R>(
    state: &SkinDrawState,
    run: impl FnOnce() -> R,
) -> R {
    let Some(context) = &state.lua_runtime else {
        return run();
    };
    let mut run = Some(run);
    let mut result = None;
    context.runtime.with_state(state, &context.enabled_options, &context.text_values, &mut || {
        result = Some(run.take().expect("Lua state scope must run once")());
    });
    result.expect("Lua state scope must run synchronously")
}

#[derive(Clone)]
pub struct SkinLuaRuntimeContext {
    pub(in crate::skin) runtime: Arc<dyn SkinLuaDrawRuntime>,
    pub(in crate::skin) enabled_options: Arc<[i32]>,
    pub(in crate::skin) text_values: Arc<BTreeMap<i32, String>>,
}

impl std::fmt::Debug for SkinLuaRuntimeContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SkinLuaRuntimeContext")
            .field("enabled_options", &self.enabled_options)
            .field("text_value_count", &self.text_values.len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for SkinLuaRuntimeContext {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.runtime, &other.runtime)
            && self.enabled_options == other.enabled_options
            && self.text_values == other.text_values
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeRegionState {
    pub judge_ms: [Option<i32>; MAX_JUDGE_REGIONS],
    pub judge_index: [Option<usize>; MAX_JUDGE_REGIONS],
    pub judge_combo: [u32; MAX_JUDGE_REGIONS],
    pub judge_timing_sign: [Option<i8>; MAX_JUDGE_REGIONS],
    /// 領域別の最新判定タイミングずれ ms (VALUE_JUDGE_1P/2P/3P_DURATION=525/526/527 に使用)。
    /// 符号は 押下時刻 - note時刻 (FAST=負)。None なら非表示。
    pub judge_timing_ms: [Option<i32>; MAX_JUDGE_REGIONS],
}

/// レーン index から判定領域 index へ (beatoraja `JudgeManager.updateMicro` 同式)。
pub fn lane_judge_region(lane_index: usize, lane_count: usize, region_count: usize) -> usize {
    if lane_count == 0 || region_count == 0 {
        return 0;
    }
    let region = lane_index * region_count / lane_count;
    region.min(region_count.saturating_sub(1))
}

/// `recent_judgements` から領域別の判定 timer / 画像 index を構築する。
pub fn build_judge_region_state(
    recent_judgements: &[crate::snapshot::DisplayJudgement],
    render_now_us: i64,
    region_count: usize,
) -> JudgeRegionState {
    let mut judge_ms = [None; MAX_JUDGE_REGIONS];
    let mut judge_index = [None; MAX_JUDGE_REGIONS];
    let mut judge_combo = [0; MAX_JUDGE_REGIONS];
    let mut judge_timing_sign = [None; MAX_JUDGE_REGIONS];
    let mut judge_timing_ms = [None; MAX_JUDGE_REGIONS];
    let region_count = region_count.min(MAX_JUDGE_REGIONS);
    for judgement in recent_judgements.iter().rev() {
        let region = lane_judge_region(judgement.lane.index(), LANE_COUNT, region_count);
        if judge_ms[region].is_some() {
            continue;
        }
        judge_ms[region] = Some(
            ((render_now_us - judgement.time.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
        );
        judge_index[region] = Some(judge_image_index_for_judge(judgement.judge));
        judge_combo[region] = judgement.combo;
        judge_timing_sign[region] = judgement.side.map(|side| match side {
            TimingSide::Fast => 1,
            TimingSide::Slow => -1,
        });
        if !judgement.timing_ms_suppressed {
            judge_timing_ms[region] =
                Some((judgement.delta_us / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32);
        }
    }
    JudgeRegionState { judge_ms, judge_index, judge_combo, judge_timing_sign, judge_timing_ms }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinClickTarget {
    Event { event_id: i32, click: i32 },
    SelectRow { row_index: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinClickHit {
    pub target: SkinClickTarget,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinSliderHit {
    pub slider_type: i32,
    pub value: f32,
}

#[derive(Debug, Clone)]
pub struct SkinContext {
    pub(super) manifest: SkinManifest,
    pub(super) document: Option<SkinDocument>,
    pub(super) lua_draw_runtime: Option<Arc<dyn SkinLuaDrawRuntime>>,
    pub(super) document_sources: HashMap<String, SkinDocumentTexture>,
    pub(super) runtime_document_sources: Arc<Mutex<HashMap<String, SkinDocumentTexture>>>,
    pub(super) select_render_cache: Arc<Mutex<SelectRenderCache>>,
    pub(super) select_settings_dest_index:
        Arc<crate::select_settings_dest::SelectSettingsDestIndex>,
    pub(super) result_render_cache: Arc<Mutex<ResultRenderCache>>,
}

impl PartialEq for SkinContext {
    fn eq(&self, other: &Self) -> bool {
        self.manifest == other.manifest
            && self.document == other.document
            && match (&self.lua_draw_runtime, &other.lua_draw_runtime) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
            && self.document_sources == other.document_sources
            && self.select_settings_dest_index == other.select_settings_dest_index
    }
}

pub(in crate::skin) const RESULT_RENDER_CACHE_MAX_ENTRIES: usize = 64;
static NEXT_RESULT_GAUGE_GRAPH_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub(in crate::skin) struct SelectRenderCache {
    planning: Option<DocumentPlanningCache>,
    static_image_items: HashMap<usize, Arc<[SkinRenderItem]>>,
}

impl SelectRenderCache {
    pub(in crate::skin) fn cached_planning(
        &mut self,
        document: &SkinDocument,
    ) -> DocumentPlanningCache {
        cached_document_planning(&mut self.planning, document)
    }

    pub(in crate::skin) fn cached_static_image_items(
        &mut self,
        destination_index: usize,
        build: impl FnOnce() -> Arc<[SkinRenderItem]>,
    ) -> Arc<[SkinRenderItem]> {
        cached_static_image_items(&mut self.static_image_items, destination_index, build)
    }
}

#[derive(Debug, Default)]
pub(in crate::skin) struct ResultRenderCache {
    planning: Option<DocumentPlanningCache>,
    static_image_items: HashMap<usize, Arc<[SkinRenderItem]>>,
    rect_batches: HashMap<ResultRectBatchCacheKey, Arc<[RectCommand]>>,
    gauge_graph: Option<ResultGaugeGraphCache>,
    gauge_rect_batches: HashMap<ResultGaugeGraphRectBatchCacheKey, Arc<[RectCommand]>>,
    judge_pie_key: Option<(DisplayJudgeCounts, Option<SkinDocumentTexture>)>,
    judge_pie_items: HashMap<usize, Option<SkinRenderItem>>,
}

impl ResultRenderCache {
    pub(in crate::skin) fn prepare_judge_pie(
        &mut self,
        counts: DisplayJudgeCounts,
        source: Option<&SkinDocumentTexture>,
    ) {
        if let Some((cached_counts, cached_source)) = &self.judge_pie_key
            && *cached_counts == counts
            && cached_source.as_ref() == source
        {
            return;
        }
        self.judge_pie_key = Some((counts, source.cloned()));
        self.judge_pie_items.clear();
    }

    pub(in crate::skin) fn judge_pie_item(&self, index: usize) -> Option<&Option<SkinRenderItem>> {
        self.judge_pie_items.get(&index)
    }

    pub(in crate::skin) fn cache_judge_pie_item(
        &mut self,
        index: usize,
        item: Option<SkinRenderItem>,
    ) {
        self.judge_pie_items.insert(index, item);
    }

    pub(in crate::skin) fn cached_planning(
        &mut self,
        document: &SkinDocument,
    ) -> DocumentPlanningCache {
        cached_document_planning(&mut self.planning, document)
    }

    pub(in crate::skin) fn cached_static_image_items(
        &mut self,
        destination_index: usize,
        build: impl FnOnce() -> Arc<[SkinRenderItem]>,
    ) -> Arc<[SkinRenderItem]> {
        cached_static_image_items(&mut self.static_image_items, destination_index, build)
    }

    pub(in crate::skin) fn cached_rect_batch(
        &mut self,
        key: ResultRectBatchCacheKey,
        build: impl FnOnce() -> Arc<[RectCommand]>,
    ) -> Arc<[RectCommand]> {
        if let Some(rects) = self.rect_batches.get(&key) {
            return Arc::clone(rects);
        }
        let rects = build();
        if self.rect_batches.len() >= RESULT_RENDER_CACHE_MAX_ENTRIES {
            self.rect_batches.clear();
        }
        self.rect_batches.insert(key, Arc::clone(&rects));
        rects
    }

    pub(in crate::skin) fn prepare_gauge_graph(
        &mut self,
        graph: &Arc<crate::snapshot::ResultGraphSnapshot>,
    ) {
        if self.gauge_graph.as_ref().is_some_and(|cached| Arc::ptr_eq(&cached.graph, graph)) {
            return;
        }
        let revision = NEXT_RESULT_GAUGE_GRAPH_REVISION.fetch_add(1, Ordering::Relaxed);
        self.gauge_graph = Some(ResultGaugeGraphCache {
            graph: Arc::clone(graph),
            revision,
            points_by_type: HashMap::new(),
        });
        if self.gauge_rect_batches.len() >= RESULT_RENDER_CACHE_MAX_ENTRIES {
            self.gauge_rect_batches.clear();
        }
    }

    pub(in crate::skin) fn cached_gauge_points(
        &mut self,
        gauge_type: i32,
    ) -> Option<(u64, Arc<[crate::snapshot::ResultGaugeGraphPoint]>)> {
        let cached = self.gauge_graph.as_mut()?;
        let points = cached
            .points_by_type
            .entry(gauge_type)
            .or_insert_with(|| {
                let filtered = cached
                    .graph
                    .gauge_points
                    .iter()
                    .copied()
                    .filter(|point| point.gauge_type == gauge_type)
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    Arc::from(cached.graph.gauge_points.as_slice())
                } else {
                    Arc::from(filtered)
                }
            })
            .clone();
        Some((cached.revision, points))
    }

    pub(in crate::skin) fn gauge_graph_revision(&self) -> Option<u64> {
        self.gauge_graph.as_ref().map(|cached| cached.revision)
    }

    pub(in crate::skin) fn cached_gauge_rect_batch(
        &mut self,
        key: ResultGaugeGraphRectBatchCacheKey,
        build: impl FnOnce() -> Arc<[RectCommand]>,
    ) -> Arc<[RectCommand]> {
        if let Some(rects) = self.gauge_rect_batches.get(&key) {
            return Arc::clone(rects);
        }
        let rects = build();
        if self.gauge_rect_batches.len() >= RESULT_RENDER_CACHE_MAX_ENTRIES {
            self.gauge_rect_batches.clear();
        }
        self.gauge_rect_batches.insert(key, Arc::clone(&rects));
        rects
    }
}

fn cached_static_image_items(
    cache: &mut HashMap<usize, Arc<[SkinRenderItem]>>,
    destination_index: usize,
    build: impl FnOnce() -> Arc<[SkinRenderItem]>,
) -> Arc<[SkinRenderItem]> {
    if let Some(items) = cache.get(&destination_index) {
        return Arc::clone(items);
    }
    let items = build();
    cache.insert(destination_index, Arc::clone(&items));
    items
}

fn cached_document_planning(
    cached: &mut Option<DocumentPlanningCache>,
    document: &SkinDocument,
) -> DocumentPlanningCache {
    if let Some(planning) = cached {
        return planning.clone();
    }
    let enabled_options = Arc::<[i32]>::from(document.enabled_options());
    let mut destinations = Vec::new();
    for (entry_index, entry) in document.destination.iter().enumerate() {
        match entry {
            DestinationListEntry::Single(_) => {
                destinations.push(ResultDestinationRef::Single { entry_index });
            }
            DestinationListEntry::Conditional { if_ops, destinations: entries } => {
                if test_skin_dst_if(if_ops, &enabled_options) {
                    destinations.extend(entries.iter().enumerate().map(
                        |(destination_index, _)| ResultDestinationRef::Conditional {
                            entry_index,
                            destination_index,
                        },
                    ));
                }
            }
        }
    }
    let objects = Arc::new(DocumentObjectIndices {
        // Collect preserves the existing image/value maps' last-definition wins.
        images: document.image.iter().enumerate().map(|(i, image)| (image.id.clone(), i)).collect(),
        values: document.value.iter().enumerate().map(|(i, value)| (value.id.clone(), i)).collect(),
    });
    let planning =
        DocumentPlanningCache { enabled_options, destinations: Arc::from(destinations), objects };
    *cached = Some(planning.clone());
    planning
}

#[derive(Debug)]
pub(in crate::skin) struct ResultGaugeGraphCache {
    graph: Arc<crate::snapshot::ResultGraphSnapshot>,
    revision: u64,
    points_by_type: HashMap<i32, Arc<[crate::snapshot::ResultGaugeGraphPoint]>>,
}

#[derive(Debug, Clone)]
pub(in crate::skin) struct DocumentPlanningCache {
    pub(in crate::skin) enabled_options: Arc<[i32]>,
    pub(in crate::skin) destinations: Arc<[ResultDestinationRef]>,
    pub(in crate::skin) objects: Arc<DocumentObjectIndices>,
}

#[derive(Debug)]
pub(in crate::skin) struct DocumentObjectIndices {
    pub(in crate::skin) images: HashMap<String, usize>,
    pub(in crate::skin) values: HashMap<String, usize>,
}

pub(in crate::skin) type SkinImageLookup<'a> = SkinObjectLookup<'a, SkinImageDef>;
pub(in crate::skin) type SkinValueLookup<'a> = SkinObjectLookup<'a, SkinValueDef>;

/// A view into the current document, backed by stable indices when a planning
/// cache exists. Sources and frame-dependent values are still resolved live.
pub(in crate::skin) enum SkinObjectLookup<'a, T> {
    Direct(HashMap<&'a str, &'a T>),
    Indexed { items: &'a [T], indices: &'a HashMap<String, usize> },
}

impl<'a, T> SkinObjectLookup<'a, T> {
    pub(in crate::skin) fn get(&self, id: &str) -> Option<&'a T> {
        match self {
            Self::Direct(items) => items.get(id).copied(),
            Self::Indexed { items, indices } => indices.get(id).and_then(|&i| items.get(i)),
        }
    }
}

impl<'a, T> From<HashMap<&'a str, &'a T>> for SkinObjectLookup<'a, T> {
    fn from(items: HashMap<&'a str, &'a T>) -> Self {
        Self::Direct(items)
    }
}

#[derive(Debug, Clone, Copy)]
pub(in crate::skin) enum ResultDestinationRef {
    Single { entry_index: usize },
    Conditional { entry_index: usize, destination_index: usize },
}

impl ResultDestinationRef {
    pub(in crate::skin) fn resolve(self, document: &SkinDocument) -> Option<&SkinDestinationDef> {
        match (self, document.destination.get(self.entry_index())) {
            (
                ResultDestinationRef::Single { .. },
                Some(DestinationListEntry::Single(destination)),
            ) => Some(destination),
            (
                ResultDestinationRef::Conditional { destination_index, .. },
                Some(DestinationListEntry::Conditional { destinations, .. }),
            ) => destinations.get(destination_index),
            _ => None,
        }
    }

    fn entry_index(self) -> usize {
        match self {
            ResultDestinationRef::Single { entry_index }
            | ResultDestinationRef::Conditional { entry_index, .. } => entry_index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::skin) struct ResultRectBatchCacheKey {
    pub(in crate::skin) destination_index: usize,
    pub(in crate::skin) kind: ResultRectBatchKind,
    pub(in crate::skin) frame: ResolvedSkinFrame,
    pub(in crate::skin) key_mode: KeyMode,
    pub(in crate::skin) judge_rank: Option<i32>,
    pub(in crate::skin) visible_len: usize,
    pub(in crate::skin) data_hash: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::skin) enum ResultRectBatchKind {
    Notes,
    Judge,
    EarlyLate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::skin) struct ResultGaugeGraphRectBatchCacheKey {
    pub(in crate::skin) destination_index: usize,
    pub(in crate::skin) frame: ResolvedSkinFrame,
    pub(in crate::skin) graph_revision: u64,
    pub(in crate::skin) display_gauge_type: i32,
    pub(in crate::skin) gauge_max_bits: u32,
    pub(in crate::skin) gauge_border_bits: u32,
}
