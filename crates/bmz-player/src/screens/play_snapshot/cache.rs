use std::hash::{Hash, Hasher};

use bmz_render::snapshot::{PlayNoteGraphs, ResultEarlyLateGraphBucket, ResultJudgeGraphBucket};

use super::*;

/// Chart values a Play skin reads while loading. beatoraja loads the Play skin
/// with the parsed chart, so skins pick texts, colors and parts at load time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaySkinChartFacts {
    /// SongData.length after Play transforms, retained before Practice changes.
    /// Independent of note display duration, range end and result waiting time.
    pub(crate) length_ms: i64,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) artist: String,
    pub(crate) subartist: String,
    pub(crate) genre: String,
    pub(crate) difficulty_name: String,
    pub(crate) play_level: String,
    pub(crate) judge_rank: Option<i32>,
    pub(crate) has_long_notes: bool,
    pub(crate) min_bpm: f32,
    pub(crate) max_bpm: f32,
    pub(crate) main_bpm: f32,
}

#[derive(Debug, Clone)]
pub struct PlayRenderSnapshotCache {
    pub(crate) skin_chart: Arc<PlaySkinChartFacts>,
    pub(super) conditional_revision: usize,
    pub(super) judge_graph_density: Arc<[u8]>,
    pub(super) bpm_graph_segments: Arc<[bmz_render::chart_graph::BpmGraphSegment]>,
    /// All notes unjudged; the gameplay thread publishes later judge states.
    pub(crate) play_note_graphs: PlayNoteGraphs,
    pub(super) min_bpm: f32,
    pub(super) max_bpm: f32,
    pub(super) has_bpm_stop: bool,
    pub(super) bpm_changes: Arc<[BpmChangePoint]>,
    pub(super) end_of_note_time: TimeUs,
    pub(super) scroll_integral: ScrollIntegralCache,
    pub(super) missed_notes: super::missed_notes::MissedNoteFallCache,
    /// SPEED イベントの `(time_us, factor)`。beatoraja と同じく時間で補間する。
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
    strictly_forward: bool,
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
    pub(crate) fn bga_event_at(&self, kind: BgaEventKind, time: TimeUs) -> Option<&BgaEvent> {
        let events = self.bga_events.events(kind);
        let end = events.partition_point(|event| event.time <= time);
        events[..end].last()
    }
    pub fn from_chart(chart: &PlayableChart) -> Self {
        let judge_graph_density = Arc::from(build_judge_graph_density(chart).into_boxed_slice());
        let bpm_graph_segments = Arc::from(build_bpm_graph_segments(chart).into_boxed_slice());
        let play_note_graphs = play_note_graphs_for_chart(chart);
        let min_bpm = chart_min_bpm(chart) as f32;
        let max_bpm = chart_max_bpm(chart) as f32;
        let has_bpm_stop = chart
            .timing_events
            .iter()
            .any(|event| matches!(event.kind, TimingEventKind::Stop { .. }));
        let bpm_changes = Arc::from(build_bpm_change_points(chart).into_boxed_slice());
        let end_of_note_time = end_of_note_time(chart);
        let scroll_integral = ScrollIntegralCache::from_segments(
            chart.scroll_events.iter().map(|event| (event.tick.0 as f64, event.factor)),
        );
        let speed_segments = Arc::from(
            chart
                .speed_events
                .iter()
                .map(|event| (event.time.0 as f64, event.factor))
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
        let timing =
            TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
        let missed_notes = super::missed_notes::MissedNoteFallCache::new(chart, &timing);
        Self {
            skin_chart: Arc::new(PlaySkinChartFacts {
                length_ms: crate::screens::play_session::beatoraja_chart_last_time(chart).0.max(0)
                    / 1_000,
                title: chart.metadata.title.clone(),
                subtitle: chart.metadata.subtitle.clone(),
                artist: chart.metadata.artist.clone(),
                subartist: chart.metadata.subartist.clone(),
                genre: chart.metadata.genre.clone(),
                difficulty_name: chart.metadata.difficulty_name.clone(),
                play_level: chart.metadata.play_level.clone(),
                judge_rank: chart.metadata.judge_rank,
                has_long_notes: !chart.long_notes.is_empty(),
                min_bpm,
                max_bpm,
                main_bpm: chart.metadata.initial_bpm as f32,
            }),
            conditional_revision: chart.metadata.conditional_revision,
            judge_graph_density,
            bpm_graph_segments,
            play_note_graphs,
            min_bpm,
            max_bpm,
            has_bpm_stop,
            bpm_changes,
            end_of_note_time,
            scroll_integral,
            missed_notes,
            speed_segments,
            long_note_prefix_max_end_times,
            bga_events,
        }
    }
}

fn play_note_graphs_for_chart(chart: &PlayableChart) -> PlayNoteGraphs {
    let seconds = bmz_render::chart_graph::note_graph_seconds(chart);
    let notes = bmz_render::chart_graph::build_note_graph_buckets(chart, seconds);
    let mut judge = vec![ResultJudgeGraphBucket::default(); seconds];
    let mut early_late = vec![ResultEarlyLateGraphBucket::default(); seconds];
    for (_, second) in bmz_render::chart_graph::judge_graph_notes(chart, seconds) {
        judge[second].values[0] = judge[second].values[0].saturating_add(1);
        early_late[second].values[0] = early_late[second].values[0].saturating_add(1);
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    notes.iter().for_each(|bucket| bucket.values.hash(&mut hasher));
    judge.iter().for_each(|bucket| bucket.values.hash(&mut hasher));
    let revision = hasher.finish();
    PlayNoteGraphs {
        notes: notes.into(),
        judge: judge.into(),
        early_late: early_late.into(),
        notes_revision: revision,
        judge_revision: revision,
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

        let strictly_forward = segments
            .iter()
            .all(|&(tick, factor)| tick.is_finite() && factor.is_finite() && factor > 0.0)
            && integral_at_event.iter().all(|value| value.is_finite());
        Self {
            segments: Arc::from(segments.into_boxed_slice()),
            integral_at_event: Arc::from(integral_at_event.into_boxed_slice()),
            strictly_forward,
        }
    }

    /// Invert only a finite, strictly increasing scroll timeline. Zero/reverse
    /// segments keep the unrestricted path, including when they are far away.
    pub(super) fn tick_after_distance(&self, from: f64, distance: f64) -> Option<f64> {
        if !self.strictly_forward || !from.is_finite() || !distance.is_finite() || distance <= 0.0 {
            return None;
        }
        let origin = self.primitive(from);
        // A huge earlier SCROLL can dwarf the visible distance. Preserve notes
        // whose primitive difference rounds to zero, instead of inverting an
        // origin + distance that rounded back to origin and culling them.
        let target = origin + distance + origin.abs() * (4.0 * f64::EPSILON);
        self.tick_at_position(target)
    }

    /// Inverse of the integrated scroll position on a strictly forward chart.
    pub(super) fn tick_at_position(&self, target: f64) -> Option<f64> {
        if !self.strictly_forward || !target.is_finite() {
            return None;
        }
        let tick = self
            .integral_at_event
            .partition_point(|&value| value <= target)
            .checked_sub(1)
            .map_or(target, |index| {
                self.segments[index].0
                    + (target - self.integral_at_event[index]) / self.segments[index].1
            });
        tick.is_finite().then_some(tick)
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
