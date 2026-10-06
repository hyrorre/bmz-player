use super::*;

pub(super) fn skin_timing_distribution_from_points(
    points: &[crate::snapshot::ResultTimingPoint],
) -> crate::snapshot::ResultTimingDistribution {
    let mut distribution = crate::snapshot::ResultTimingDistribution::default();
    for point in points {
        distribution.add((point.delta_us / 1_000) as i32);
    }
    distribution
}

pub(super) fn beatoraja_timing_distribution_max(
    distribution: &crate::snapshot::ResultTimingDistribution,
) -> u32 {
    let mut max = 10;
    for count in &distribution.counts {
        if max < *count {
            max = (count / 10) * 10 + 10;
        }
    }
    max
}

pub(super) fn timing_color(value: &str, frame_alpha: f32) -> Color {
    skin_hex_color(value)
        .or_else(|| skin_hex_color("FF0000FF"))
        .unwrap_or(Color::rgb(1.0, 0.0, 0.0))
        .with_alpha(frame_alpha)
}

pub(super) fn select_note_distribution_max_density(
    distribution: &[crate::scene::SelectChartDistributionSecond],
) -> u32 {
    let peak = distribution.iter().map(|second| second.total()).max().unwrap_or(0);
    if peak <= 20 { 20 } else { ((peak / 10) * 10 + 10).clamp(20, 100) }
}

pub(super) fn select_note_distribution_background_items(
    rect: Rect,
    seconds: usize,
    max_density: u32,
    frame_alpha: f32,
    blend: BlendMode,
    pixel_w: f32,
    pixel_h: f32,
) -> Vec<SkinRenderItem> {
    let mut items = vec![SkinRenderItem::Rect {
        rect,
        color: Color::rgba(0.0, 0.0, 0.0, 0.8 * frame_alpha),
        blend,
    }];

    for density in (10..max_density).step_by(10) {
        let y = rect.y + rect.height - rect.height * density as f32 / max_density.max(1) as f32;
        items.push(SkinRenderItem::Rect {
            rect: Rect { x: rect.x, y, width: rect.width, height: pixel_h },
            color: Color::rgba(0.007 * density as f32, 0.007 * density as f32, 0.0, frame_alpha),
            blend,
        });
    }

    for second in 0..seconds {
        let color = if second % 60 == 0 {
            Some(Color::rgba(0.25, 0.25, 0.25, frame_alpha))
        } else if second % 10 == 0 {
            Some(Color::rgba(0.125, 0.125, 0.125, frame_alpha))
        } else {
            None
        };
        if let Some(color) = color {
            let x = rect.x + rect.width * second as f32 / seconds.max(1) as f32;
            items.push(SkinRenderItem::Rect {
                rect: Rect { x, y: rect.y, width: pixel_w, height: rect.height },
                color,
                blend,
            });
        }
    }
    items
}

pub(super) fn note_distribution_colors(alpha: f32) -> [Color; 7] {
    [
        Color::rgba(0.27, 1.0, 0.27, alpha),
        Color::rgba(0.13, 0.53, 0.13, alpha),
        Color::rgba(1.0, 0.27, 0.27, alpha),
        Color::rgba(0.27, 0.27, 1.0, alpha),
        Color::rgba(0.13, 0.13, 0.53, alpha),
        Color::rgba(0.80, 0.80, 0.80, alpha),
        Color::rgba(0.53, 0.0, 0.0, alpha),
    ]
}

pub(super) fn result_judge_graph_colors(alpha: f32, pms: bool) -> [Color; 6] {
    if pms {
        return [
            Color::rgba(0.33, 0.33, 0.33, alpha),
            Color::rgba(1.0, 0.37, 0.69, alpha),
            Color::rgba(1.0, 0.75, 0.20, alpha),
            Color::rgba(0.86, 0.27, 0.24, alpha),
            Color::rgba(0.42, 0.78, 1.0, alpha),
            Color::rgba(0.42, 0.78, 1.0, alpha),
        ];
    }
    [
        Color::rgba(0.33, 0.33, 0.33, alpha),
        Color::rgba(0.0, 0.53, 1.0, alpha),
        Color::rgba(0.0, 1.0, 0.53, alpha),
        Color::rgba(1.0, 1.0, 0.0, alpha),
        Color::rgba(1.0, 0.53, 0.0, alpha),
        Color::rgba(1.0, 0.0, 0.0, alpha),
    ]
}

pub(super) fn result_early_late_graph_colors(alpha: f32, pms: bool) -> [Color; 10] {
    if pms {
        return [
            Color::rgba(0.33, 0.33, 0.33, alpha),
            Color::rgba(1.0, 0.37, 0.69, alpha),
            Color::rgba(0.0, 0.53, 1.0, alpha),
            Color::rgba(0.0, 0.4, 0.8, alpha),
            Color::rgba(0.0, 0.27, 0.53, alpha),
            Color::rgba(0.0, 0.13, 0.27, alpha),
            Color::rgba(1.0, 0.53, 0.0, alpha),
            Color::rgba(0.8, 0.4, 0.0, alpha),
            Color::rgba(0.53, 0.27, 0.0, alpha),
            Color::rgba(0.27, 0.13, 0.0, alpha),
        ];
    }
    [
        Color::rgba(0.33, 0.33, 0.33, alpha),
        Color::rgba(0.27, 1.0, 0.27, alpha),
        Color::rgba(0.0, 0.53, 1.0, alpha),
        Color::rgba(0.0, 0.4, 0.8, alpha),
        Color::rgba(0.0, 0.27, 0.53, alpha),
        Color::rgba(0.0, 0.13, 0.27, alpha),
        Color::rgba(1.0, 0.53, 0.0, alpha),
        Color::rgba(0.8, 0.4, 0.0, alpha),
        Color::rgba(0.53, 0.27, 0.0, alpha),
        Color::rgba(0.27, 0.13, 0.0, alpha),
    ]
}

pub(super) trait ResultNoteGraphBucketValues<const N: usize> {
    fn values(&self) -> [u32; N];
}

impl<const N: usize> ResultNoteGraphBucketValues<N> for [u32; N] {
    fn values(&self) -> [u32; N] {
        *self
    }
}

impl ResultNoteGraphBucketValues<6> for crate::snapshot::ResultJudgeGraphBucket {
    fn values(&self) -> [u32; 6] {
        self.values
    }
}

impl ResultNoteGraphBucketValues<7> for crate::snapshot::ResultNoteGraphBucket {
    fn values(&self) -> [u32; 7] {
        self.values
    }
}

impl ResultNoteGraphBucketValues<10> for crate::snapshot::ResultEarlyLateGraphBucket {
    fn values(&self) -> [u32; 10] {
        self.values
    }
}

pub(super) fn stacked_result_note_graph_rect_batch<
    const N: usize,
    B: ResultNoteGraphBucketValues<N>,
>(
    buckets: &[B],
    colors: &[Color; N],
    graph: &SkinJudgeGraphDef,
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    canvas_w: u32,
    canvas_h: u32,
    elapsed_ms: i32,
) -> Arc<[RectCommand]> {
    if buckets.is_empty() {
        return Arc::from([]);
    }
    let rect = normalize_skin_frame_rect(frame, canvas_w, canvas_h);
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Arc::from([]);
    }
    let frame_alpha = frame.a as f32 / 255.0;
    let blend = skin_blend_mode(destination.blend);
    let max_stack =
        buckets.iter().map(|bucket| bucket.values().into_iter().sum::<u32>()).max().unwrap_or(0);
    let graph_max = beatoraja_note_graph_max(max_stack);
    let visible_len = result_note_graph_visible_len(buckets.len(), graph, elapsed_ms);
    let background_items = if graph.back_tex_off == 0 {
        result_note_graph_background_item_count(buckets.len(), graph_max)
    } else {
        0
    };
    let chip_items = buckets
        .iter()
        .take(visible_len)
        .map(|bucket| bucket.values().into_iter().sum::<u32>().min(graph_max) as usize)
        .sum::<usize>();
    let mut rects = Vec::with_capacity(background_items.saturating_add(chip_items));
    if graph.back_tex_off == 0 {
        push_result_note_graph_background(
            &mut rects,
            rect,
            buckets.len(),
            graph_max,
            frame_alpha,
            blend,
        );
    }
    if visible_len == 0 {
        return Arc::from(rects);
    }
    let bucket_w = rect.width / buckets.len().max(1) as f32;
    let chip_w = bucket_w * if graph.no_gap_x != 0 { 1.0 } else { 0.8 };
    let unit_h = rect.height / graph_max.max(1) as f32;
    let chip_h = unit_h * if graph.no_gap != 0 { 1.0 } else { 0.8 };

    for (second, bucket) in buckets.iter().take(visible_len).enumerate() {
        let x = rect.x + second as f32 * bucket_w;
        let chip_layout = ResultNoteGraphChipLayout {
            rect,
            x,
            chip_width: chip_w,
            unit_height: unit_h,
            chip_height: chip_h,
            graph_max,
        };
        let mut drawn = 0_u32;
        let values = bucket.values();
        if graph.order_reverse != 0 {
            for (series, value) in values.into_iter().enumerate().rev() {
                push_result_note_graph_chips(
                    &mut rects,
                    chip_layout,
                    &mut drawn,
                    value,
                    colors[series],
                );
            }
        } else {
            for (series, value) in values.into_iter().enumerate() {
                push_result_note_graph_chips(
                    &mut rects,
                    chip_layout,
                    &mut drawn,
                    value,
                    colors[series],
                );
            }
        }
    }
    Arc::from(rects)
}

pub(super) fn rect_batch_render_items(
    rects: Arc<[RectCommand]>,
    cache: Option<RectBatchCache>,
) -> Vec<SkinRenderItem> {
    if rects.is_empty() { Vec::new() } else { vec![SkinRenderItem::RectBatch { rects, cache }] }
}

pub(super) fn result_note_graph_cache_key<const N: usize, B: ResultNoteGraphBucketValues<N>>(
    destination_index: usize,
    kind: ResultRectBatchKind,
    buckets: &[B],
    graph: &SkinJudgeGraphDef,
    frame: ResolvedSkinFrame,
    state: &SkinDrawState,
    elapsed_ms: i32,
) -> ResultRectBatchCacheKey {
    ResultRectBatchCacheKey {
        destination_index,
        kind,
        frame: frame.geometry_cache_frame(),
        key_mode: state.key_mode,
        judge_rank: state.judge_rank,
        visible_len: result_note_graph_visible_len(buckets.len(), graph, elapsed_ms),
        data_hash: result_note_graph_data_hash(buckets, graph),
    }
}

pub(super) fn result_note_graph_rect_batch_cache(
    key: ResultRectBatchCacheKey,
    graph: &SkinJudgeGraphDef,
    frame: ResolvedSkinFrame,
    canvas_w: u32,
    canvas_h: u32,
) -> Option<RectBatchCache> {
    if graph.back_tex_off == 0 {
        return None;
    }
    let bounds = normalize_skin_frame_rect(frame, canvas_w, canvas_h);
    if bounds.width <= f32::EPSILON || bounds.height <= f32::EPSILON {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    "result-note-graph-rect-batch".hash(&mut hasher);
    key.hash(&mut hasher);
    Some(RectBatchCache { key: RectBatchCacheKey(hasher.finish()), bounds })
}

pub(super) fn result_gauge_graph_rect_batch_cache(
    key: ResultGaugeGraphRectBatchCacheKey,
    rects: &[RectCommand],
) -> Option<RectBatchCache> {
    let first = rects.first()?.rect;
    let bounds = rects.iter().skip(1).fold(first, |bounds, command| {
        let left = bounds.x.min(command.rect.x);
        let top = bounds.y.min(command.rect.y);
        let right = (bounds.x + bounds.width).max(command.rect.x + command.rect.width);
        let bottom = (bounds.y + bounds.height).max(command.rect.y + command.rect.height);
        Rect { x: left, y: top, width: right - left, height: bottom - top }
    });
    if bounds.width <= f32::EPSILON || bounds.height <= f32::EPSILON {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    "result-gauge-graph-rect-batch".hash(&mut hasher);
    key.hash(&mut hasher);
    Some(RectBatchCache { key: RectBatchCacheKey(hasher.finish()), bounds })
}

pub(super) fn result_note_graph_data_hash<const N: usize, B: ResultNoteGraphBucketValues<N>>(
    buckets: &[B],
    graph: &SkinJudgeGraphDef,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    graph.graph_type().hash(&mut hasher);
    graph.back_tex_off.hash(&mut hasher);
    graph.delay.hash(&mut hasher);
    graph.order_reverse.hash(&mut hasher);
    graph.no_gap.hash(&mut hasher);
    graph.no_gap_x.hash(&mut hasher);
    buckets.len().hash(&mut hasher);
    for bucket in buckets {
        bucket.values().hash(&mut hasher);
    }
    hasher.finish()
}

pub(super) fn result_note_graph_visible_len(
    bucket_count: usize,
    graph: &SkinJudgeGraphDef,
    elapsed_ms: i32,
) -> usize {
    let render_ratio = if graph.delay > 0 {
        (elapsed_ms as f32 / graph.delay as f32).clamp(0.0, 1.0)
    } else {
        1.0
    };
    ((bucket_count as f32) * render_ratio).ceil() as usize
}

pub(super) fn beatoraja_note_graph_max(max_stack: u32) -> u32 {
    if max_stack <= 20 { 20 } else { ((max_stack / 10) * 10 + 10).min(100) }
}

#[derive(Debug, Clone, Copy)]
struct ResultNoteGraphChipLayout {
    rect: Rect,
    x: f32,
    chip_width: f32,
    unit_height: f32,
    chip_height: f32,
    graph_max: u32,
}

fn push_result_note_graph_chips(
    rects: &mut Vec<RectCommand>,
    layout: ResultNoteGraphChipLayout,
    drawn: &mut u32,
    value: u32,
    color: Color,
) {
    for _ in 0..value {
        if *drawn >= layout.graph_max {
            break;
        }
        let y = layout.rect.y + layout.rect.height - (*drawn as f32 + 1.0) * layout.unit_height;
        rects.push(RectCommand {
            rect: Rect { x: layout.x, y, width: layout.chip_width, height: layout.chip_height },
            color,
        });
        *drawn = (*drawn).saturating_add(1);
    }
}

pub(super) fn push_result_note_graph_background(
    rects: &mut Vec<RectCommand>,
    rect: Rect,
    bucket_count: usize,
    graph_max: u32,
    frame_alpha: f32,
    _blend: BlendMode,
) {
    rects.push(RectCommand { rect, color: Color::rgba(0.0, 0.0, 0.0, 0.8 * frame_alpha) });
    for count in (10..graph_max).step_by(10) {
        let band_y =
            rect.y + rect.height * (1.0 - (count + 10).min(graph_max) as f32 / graph_max as f32);
        let band_h = rect.height * 10.0 / graph_max as f32;
        rects.push(RectCommand {
            rect: Rect { x: rect.x, y: band_y, width: rect.width, height: band_h },
            color: Color::rgba(0.007 * count as f32, 0.007 * count as f32, 0.0, frame_alpha),
        });
    }
    let line_w = (rect.width / (bucket_count.max(1) * 5) as f32).max(0.0005);
    for second in 0..bucket_count {
        let color = if second % 60 == 0 {
            Some(Color::rgba(0.25, 0.25, 0.25, frame_alpha))
        } else if second % 10 == 0 {
            Some(Color::rgba(0.125, 0.125, 0.125, frame_alpha))
        } else {
            None
        };
        if let Some(color) = color {
            rects.push(RectCommand {
                rect: Rect {
                    x: rect.x + second as f32 * rect.width / bucket_count.max(1) as f32,
                    y: rect.y,
                    width: line_w,
                    height: rect.height,
                },
                color,
            });
        }
    }
}

pub(super) fn result_note_graph_background_item_count(
    bucket_count: usize,
    graph_max: u32,
) -> usize {
    let band_count = (10..graph_max).step_by(10).count();
    let line_count = (0..bucket_count).filter(|second| second % 10 == 0).count();
    1 + band_count + line_count
}
