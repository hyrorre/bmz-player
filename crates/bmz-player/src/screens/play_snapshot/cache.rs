use super::*;

#[derive(Debug, Clone)]
pub struct PlayRenderSnapshotCache {
    pub(super) conditional_revision: usize,
    pub(super) judge_graph_density: Arc<[u8]>,
    pub(super) bpm_graph_segments: Arc<[bmz_render::chart_graph::BpmGraphSegment]>,
    pub(super) min_bpm: f32,
    pub(super) max_bpm: f32,
    pub(super) has_bpm_stop: bool,
    pub(super) end_of_note_time: TimeUs,
    pub(super) scroll_integral: ScrollIntegralCache,
    pub(super) speed_segments: Arc<[(f64, f64)]>,
    /// `long_notes` は start tick 順であることを前提にしている。各位置までの
    /// end time 最大値を保持し、画面より前に完全に抜けた LN 群を二分探索で飛ばす。
    /// start tick の上限と組み合わせることで、simple scroll 中の LN 走査を
    /// 可視候補だけに絞る。
    pub(super) long_note_prefix_max_end_times: Arc<[i64]>,
    pub(super) bga_events: BgaEventCache,
}

#[derive(Debug, Clone)]
pub(super) struct ScrollIntegralCache {
    pub(super) segments: Arc<[(f64, f64)]>,
    pub(super) integral_at_event: Arc<[f64]>,
}

#[derive(Debug, Clone)]
pub(super) struct BgaEventCache {
    pub(super) events_by_kind: [Arc<[BgaEvent]>; BGA_EVENT_KIND_COUNT],
    pub(super) opacity_by_kind: [Arc<[BgaOpacityEvent]>; BGA_EVENT_KIND_COUNT],
    pub(super) argb_by_kind: [Arc<[BgaArgbEvent]>; BGA_EVENT_KIND_COUNT],
}

impl PlayRenderSnapshotCache {
    pub fn for_updated_chart(&self, chart: &PlayableChart) -> Self {
        let mut cache = Self::from_chart(chart);
        cache.judge_graph_density = self.judge_graph_density.clone();
        cache
    }
    pub fn from_chart(chart: &PlayableChart) -> Self {
        let judge_graph_density = Arc::from(build_judge_graph_density(chart).into_boxed_slice());
        let bpm_graph_segments = Arc::from(build_bpm_graph_segments(chart).into_boxed_slice());
        let min_bpm = chart_min_bpm(chart) as f32;
        let max_bpm = chart_max_bpm(chart) as f32;
        let has_bpm_stop = chart
            .timing_events
            .iter()
            .any(|event| matches!(event.kind, TimingEventKind::Stop { .. }));
        let end_of_note_time = end_of_note_time(chart);
        let scroll_integral = ScrollIntegralCache::from_segments(
            chart.scroll_events.iter().map(|event| (event.tick.0 as f64, event.factor)),
        );
        let speed_segments = Arc::from(
            chart
                .speed_events
                .iter()
                .map(|event| (event.tick.0 as f64, event.factor))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let mut max_end_time = i64::MIN;
        let long_note_prefix_max_end_times = Arc::from(
            chart
                .long_notes
                .iter()
                .map(|long| {
                    max_end_time = max_end_time.max(long.end_time.0);
                    max_end_time
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let bga_events = BgaEventCache::from_chart(chart);
        Self {
            conditional_revision: chart.metadata.conditional_revision,
            judge_graph_density,
            bpm_graph_segments,
            min_bpm,
            max_bpm,
            has_bpm_stop,
            end_of_note_time,
            scroll_integral,
            speed_segments,
            long_note_prefix_max_end_times,
            bga_events,
        }
    }
}

impl ScrollIntegralCache {
    pub(super) fn from_segments(segments: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let segments = segments.into_iter().collect::<Vec<_>>();
        debug_assert!(segments.windows(2).all(|pair| pair[0].0 <= pair[1].0));

        let mut integral_at_event = Vec::with_capacity(segments.len());
        let mut previous_tick = 0.0;
        let mut previous_factor = 1.0;
        let mut integral = 0.0;
        for &(tick, next_factor) in &segments {
            integral += (tick - previous_tick) * previous_factor;
            integral_at_event.push(integral);
            previous_tick = tick;
            previous_factor = next_factor;
        }

        Self {
            segments: Arc::from(segments.into_boxed_slice()),
            integral_at_event: Arc::from(integral_at_event.into_boxed_slice()),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub(super) fn factor_at(&self, tick: f64) -> f64 {
        self.segments
            .partition_point(|(event_tick, _)| *event_tick <= tick)
            .checked_sub(1)
            .map_or(1.0, |index| self.segments[index].1)
    }

    pub(super) fn primitive(&self, tick: f64) -> f64 {
        let Some(index) =
            self.segments.partition_point(|(event_tick, _)| *event_tick <= tick).checked_sub(1)
        else {
            return tick;
        };
        let (event_tick, factor) = self.segments[index];
        self.integral_at_event[index] + (tick - event_tick) * factor
    }

    pub(super) fn delta(&self, from_tick: f64, to_tick: f64) -> f64 {
        if (from_tick - to_tick).abs() < f64::EPSILON {
            return 0.0;
        }
        self.primitive(to_tick) - self.primitive(from_tick)
    }
}

impl BgaEventCache {
    pub(super) fn from_chart(chart: &PlayableChart) -> Self {
        let mut events_by_kind: [Vec<BgaEvent>; BGA_EVENT_KIND_COUNT] =
            std::array::from_fn(|_| Vec::new());
        for event in &chart.bga_events {
            events_by_kind[bga_event_kind_index(event.kind)].push(event.clone());
        }
        let mut opacity_by_kind: [Vec<BgaOpacityEvent>; BGA_EVENT_KIND_COUNT] =
            std::array::from_fn(|_| Vec::new());
        for event in &chart.bga_opacity_events {
            opacity_by_kind[bga_event_kind_index(event.layer)].push(*event);
        }
        let mut argb_by_kind: [Vec<BgaArgbEvent>; BGA_EVENT_KIND_COUNT] =
            std::array::from_fn(|_| Vec::new());
        for event in &chart.bga_argb_events {
            argb_by_kind[bga_event_kind_index(event.layer)].push(*event);
        }
        Self {
            events_by_kind: events_by_kind.map(|events| Arc::from(events.into_boxed_slice())),
            opacity_by_kind: opacity_by_kind.map(|events| Arc::from(events.into_boxed_slice())),
            argb_by_kind: argb_by_kind.map(|events| Arc::from(events.into_boxed_slice())),
        }
    }

    pub(super) fn events(&self, kind: BgaEventKind) -> &[BgaEvent] {
        &self.events_by_kind[bga_event_kind_index(kind)]
    }

    pub(super) fn opacity_events(&self, kind: BgaEventKind) -> &[BgaOpacityEvent] {
        &self.opacity_by_kind[bga_event_kind_index(kind)]
    }

    pub(super) fn argb_events(&self, kind: BgaEventKind) -> &[BgaArgbEvent] {
        &self.argb_by_kind[bga_event_kind_index(kind)]
    }
}

pub(super) fn bga_event_kind_index(kind: BgaEventKind) -> usize {
    match kind {
        BgaEventKind::Base => 0,
        BgaEventKind::Poor => 1,
        BgaEventKind::Layer => 2,
        BgaEventKind::Layer2 => 3,
    }
}
