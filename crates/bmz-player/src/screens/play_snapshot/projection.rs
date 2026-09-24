//! Detached playfield data, projected at the consumer's audio-clock time.
//! No judgement engine, input queue, replay recorder or audio commands live here.
use super::*;
use bmz_core::ids::NoteId;
use bmz_gameplay::judge::model::HlnBodyVisualState;
use bmz_gameplay::judge::model::{JudgeWindows, LaneJudgeState};
use bmz_gameplay::session::HcnLaneTimer;
use std::borrow::Cow;

pub(super) struct PlayfieldJudgeView<'a> {
    pub hln_bodies: Cow<'a, [HlnBodyVisualState]>,
    pub lanes: &'a [LaneJudgeState; LANE_COUNT],
    pub judged_notes: &'a HashMap<NoteId, Judge>,
    pub window_set: JudgeWindows,
}

pub(super) struct PlayfieldView<'a> {
    pub chart: &'a PlayableChart,
    pub timing_map: &'a TimingMap,
    pub judge: PlayfieldJudgeView<'a>,
    pub lane_hcn_timer: &'a [Option<HcnLaneTimer>; LANE_COUNT],
    pub hispeed: f32,
    pub lift: f32,
    pub note_retention: bool,
    pub constant_enabled: bool,
    pub constant_fade_ms: i32,
    pub target_green_number: u32,
}

impl<'a> From<&'a GameSession> for PlayfieldView<'a> {
    fn from(session: &'a GameSession) -> Self {
        Self {
            chart: &session.chart,
            timing_map: &session.timing_map,
            judge: PlayfieldJudgeView {
                hln_bodies: Cow::Owned(session.judge.hln_body_visuals().collect()),
                lanes: &session.judge.lanes,
                judged_notes: &session.judge.judged_notes,
                window_set: session.judge.window_set,
            },
            lane_hcn_timer: &session.lane_hcn_timer,
            hispeed: session.hispeed,
            lift: session.lift,
            note_retention: session.note_retention,
            constant_enabled: session.constant_enabled,
            constant_fade_ms: session.constant_fade_ms,
            target_green_number: session.target_green_number,
        }
    }
}

/// Three instances are allocated at play start. The producer can update one
/// only when its Arc has no consumers; maps retain their chart-sized capacity.
pub(crate) struct PlayfieldProjection {
    chart: Arc<PlayableChart>,
    timing_map: Arc<TimingMap>,
    cache: PlayRenderSnapshotCache,
    judged_notes: HashMap<NoteId, Judge>,
    hln_bodies: Vec<HlnBodyVisualState>,
    lanes: [LaneJudgeState; LANE_COUNT],
    window_set: JudgeWindows,
    lane_hcn_timer: [Option<HcnLaneTimer>; LANE_COUNT],
    hispeed: f32,
    lift: f32,
    note_retention: bool,
    constant_enabled: bool,
    constant_fade_ms: i32,
    target_green_number: u32,
    visual_offset_us: i64,
    published_time: TimeUs,
    duration_lane_cover: f32,
    playback_rate_percent: u16,
}

impl PlayfieldProjection {
    pub(crate) fn pool(session: &GameSession, cache: &PlayRenderSnapshotCache) -> [Arc<Self>; 3] {
        let timing_map = Arc::new(session.timing_map.clone());
        let capacity = session.chart.lane_notes.iter().map(Vec::len).sum();
        std::array::from_fn(|_| {
            let mut projection = Self {
                chart: session.chart.clone(),
                timing_map: timing_map.clone(),
                cache: cache.clone(),
                judged_notes: HashMap::with_capacity(capacity),
                hln_bodies: Vec::new(),
                lanes: session.judge.lanes,
                window_set: session.judge.window_set,
                lane_hcn_timer: session.lane_hcn_timer,
                hispeed: session.hispeed,
                lift: session.lift,
                note_retention: session.note_retention,
                constant_enabled: session.constant_enabled,
                constant_fade_ms: session.constant_fade_ms,
                target_green_number: session.target_green_number,
                visual_offset_us: session.offsets.visual_offset_us,
                published_time: session.audio_clock.now(),
                duration_lane_cover: 0.0,
                playback_rate_percent: session.audio_clock.playback_rate_percent(),
            };
            projection.update(session, projection.published_time);
            Arc::new(projection)
        })
    }

    pub(crate) fn update(&mut self, session: &GameSession, time: TimeUs) {
        if !Arc::ptr_eq(&self.chart, &session.chart) {
            self.chart = session.chart.clone();
            self.timing_map = Arc::new(session.timing_map.clone());
            self.cache = self.cache.for_updated_chart(&session.chart);
        }
        self.hln_bodies.clear();
        self.hln_bodies.extend(session.judge.hln_body_visuals());
        self.judged_notes.clear();
        self.judged_notes
            .extend(session.judge.judged_notes.iter().map(|(&id, &judge)| (id, judge)));
        self.lanes = session.judge.lanes;
        self.window_set = session.judge.window_set;
        self.lane_hcn_timer = session.lane_hcn_timer;
        self.hispeed = session.hispeed;
        self.lift = session.lift;
        self.note_retention = session.note_retention;
        self.constant_enabled = session.constant_enabled;
        self.constant_fade_ms = session.constant_fade_ms;
        self.target_green_number = session.target_green_number;
        self.visual_offset_us = session.offsets.visual_offset_us;
        self.published_time = time;
        self.duration_lane_cover = if session.lane_cover_visible {
            crate::config::play::clamp_lane_cover_for_lift(session.lane_cover, session.lift)
        } else {
            0.0
        };
        self.playback_rate_percent = session.audio_clock.playback_rate_percent();
    }

    pub(crate) fn project(
        &self,
        snapshot: &mut RenderSnapshot,
        now: TimeUs,
        clock: &mut ProjectionClock,
    ) {
        let (chart_now, lane_now) =
            clock.advance(now.max(self.published_time), self.visual_offset_us);
        snapshot.time = chart_now;
        snapshot.play_elapsed_time = TimeUs(chart_now.0.max(0));
        let view = PlayfieldView {
            chart: &self.chart,
            timing_map: &self.timing_map,
            judge: PlayfieldJudgeView {
                hln_bodies: Cow::Borrowed(&self.hln_bodies),
                lanes: &self.lanes,
                judged_notes: &self.judged_notes,
                window_set: self.window_set,
            },
            lane_hcn_timer: &self.lane_hcn_timer,
            hispeed: self.hispeed,
            lift: self.lift,
            note_retention: self.note_retention,
            constant_enabled: self.constant_enabled,
            constant_fade_ms: self.constant_fade_ms,
            target_green_number: self.target_green_number,
        };
        for notes in &mut snapshot.visible_notes {
            notes.clear();
        }
        for mines in &mut snapshot.visible_mines {
            mines.clear();
        }
        snapshot.visible_long_notes.clear();
        snapshot.bar_lines.clear();
        snapshot.bpm_lines.clear();
        snapshot.stop_lines.clear();
        snapshot.time_lines.clear();
        snapshot.now_bpm = self.timing_map.bpm_at_time(lane_now) as f32;
        let cursor_tick = self.timing_map.time_to_tick_f64(scroll_render_time(lane_now));
        let multiplier = current_scroll_multiplier_from_segments(
            &self.cache.scroll_integral,
            &self.cache.speed_segments,
            cursor_tick,
        );
        snapshot.note_display_duration_ms = display_duration_ms_for_bpm_hispeed(
            effective_bpm_for_playback_rate(f64::from(snapshot.now_bpm), self.playback_rate_percent)
                as f32,
            self.hispeed,
            self.duration_lane_cover,
            self.lift,
            multiplier,
        )
        .round()
        .clamp(0.0, i32::MAX as f32) as i32;
        snapshot.adjusted_cover_progress = compute_adjusted_cover_progress(
            snapshot.hidden_enabled,
            snapshot.lane_cover,
            self.lift,
            snapshot.hsfix_index,
            snapshot.now_bpm,
            self.cache.max_bpm,
            self.chart.metadata.initial_bpm as f32,
        );
        snapshot.adjusted_rate = compute_adjusted_rate(
            snapshot.hidden_enabled,
            snapshot.lanecover_enabled,
            snapshot.hsfix_index,
            snapshot.now_bpm,
            self.cache.max_bpm,
            self.chart.metadata.initial_bpm as f32,
        );
        snapshot.adjusted_rate_adot =
            snapshot.adjusted_rate.map(|rate| (rate * 100.0).floor() as i32);
        super::build::visible::populate_visible_playfield(
            snapshot,
            &view,
            chart_now,
            &self.cache,
            lane_now,
        );
    }
}

/// Consumer-local clock, reset by constructing a new client for seek/retry.
/// Auto adjustment may change the visual offset; clamp only the projection,
/// never the gameplay clock or the saved offset.
#[derive(Default)]
pub(crate) struct ProjectionClock {
    chart: Option<TimeUs>,
    lane: Option<TimeUs>,
}

impl ProjectionClock {
    fn advance(&mut self, now: TimeUs, offset: i64) -> (TimeUs, TimeUs) {
        let chart = self.chart.map_or(now, |previous| previous.max(now));
        let lane = TimeUs(chart.0.saturating_add(offset));
        let lane = self.lane.map_or(lane, |previous| previous.max(lane));
        self.chart = Some(chart);
        self.lane = Some(lane);
        (chart, lane)
    }
}

#[cfg(test)]
mod tests;
