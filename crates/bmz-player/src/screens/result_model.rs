use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bmz_chart::model::{LongNoteMode, PlayableChart};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::ids::NoteId;
use bmz_core::lane::KeyMode;
use bmz_gameplay::gauge::gauge_total_for_chart;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::JudgeCounts;
use bmz_gameplay::session::{FrameOutput, GameSession, ResultJudgementDetail};
use bmz_render::snapshot::{
    PlayNoteGraphs, RenderSnapshot, ResultEarlyLateGraphBucket, ResultGaugeGraphPoint,
    ResultGraphSnapshot, ResultJudgeGraphBucket, ResultTimingDistribution, ResultTimingPoint,
};

use crate::storage::play_result::StoredPlayResult;

const RESULT_GAUGE_GRAPH_SAMPLE_MS: i32 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSummary {
    pub chart_md5: Option<[u8; 16]>,
    pub chart_sha256: Option<[u8; 32]>,
    /// This saved attempt's time, not the previous best. None for unsaved modes.
    pub last_played_at: Option<i64>,
    pub clear_type: ClearType,
    pub skin_attempt: bmz_render::snapshot::SkinAttemptState,
    /// text ref 1 に渡すプレイ時ライバル/ターゲット名。
    pub target_name: String,
    pub target: crate::select_options::TargetOption,
    pub arrange: String,
    pub arrange_2p: String,
    pub lane_shuffle_pattern: Vec<u8>,
    pub ex_score: u32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
    pub gauge_value: f32,
    pub gauge_type: GaugeType,
    pub total_notes: u32,
    pub duration_ms: i32,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub main_bpm: f32,
    pub total_gauge: f32,
    pub judge_rank: Option<i32>,
    /// Final effective note judge window in chart time, frozen at session finish.
    pub note_judge_window: Option<bmz_gameplay::judge::model::JudgeWindow>,
    pub key_mode: KeyMode,
    /// LN policy / course constraint適用後の実効譜面にLNが含まれるか。
    pub has_long_notes: bool,
    /// LN policy / course constraint適用後の実効LN種別。
    pub long_note_mode: LongNoteMode,
    pub judge_counts: ResultJudgeCounts,
    pub fast_slow_counts: ResultFastSlowJudgeCounts,
    pub replay_path: String,
    pub replay_slots: [bool; 4],
    pub saved_replay_slots: [bool; 4],
    pub score_history_id: i64,
    pub best_ex_score: Option<u32>,
    pub best_clear_type: Option<ClearType>,
    pub best_max_combo: Option<u32>,
    pub best_bp: Option<u32>,
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_clear_type: Option<ClearType>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_bp: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub target_bp: Option<u32>,
    pub target_clear_type: Option<ClearType>,
    pub ir_queued_jobs: u32,
    pub ir_last_error: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub play_level: String,
    /// The battle opponent's final score (AUTO BATTLE / G-BATTLE), shown as LR2's 2P.
    pub battle_opponent: Option<bmz_render::snapshot::OpponentRenderSnapshot>,
    pub graph: Arc<ResultGraphSnapshot>,
}

#[derive(Debug, Clone, Default)]
pub struct ResultGraphCollector {
    graph: ResultGraphSnapshot,
    next_gauge_sample_ms: i32,
    /// 採用した結果の確定時刻 (ms)。全ゲージの終端点は一度だけ記録する。
    terminal_gauge_ms: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultJudgeCounts {
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub empty_poor: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultFastSlowJudgeCounts {
    pub fast_pgreat: u32,
    pub slow_pgreat: u32,
    pub fast_great: u32,
    pub slow_great: u32,
    pub fast_good: u32,
    pub slow_good: u32,
    pub fast_bad: u32,
    pub slow_bad: u32,
    pub fast_poor: u32,
    pub slow_poor: u32,
    pub fast_empty_poor: u32,
    pub slow_empty_poor: u32,
}

impl ResultJudgeCounts {
    fn from_judge_counts(judges: &JudgeCounts) -> Self {
        Self {
            pgreat: judges.fast_pgreat + judges.slow_pgreat,
            great: judges.fast_great + judges.slow_great,
            good: judges.fast_good + judges.slow_good,
            bad: judges.fast_bad + judges.slow_bad,
            poor: judges.fast_poor + judges.slow_poor,
            empty_poor: judges.fast_empty_poor + judges.slow_empty_poor,
        }
    }
}

impl ResultFastSlowJudgeCounts {
    fn from_judge_counts(judges: &JudgeCounts) -> Self {
        Self {
            fast_pgreat: judges.fast_pgreat,
            slow_pgreat: judges.slow_pgreat,
            fast_great: judges.fast_great,
            slow_great: judges.slow_great,
            fast_good: judges.fast_good,
            slow_good: judges.slow_good,
            fast_bad: judges.fast_bad,
            slow_bad: judges.slow_bad,
            fast_poor: judges.fast_poor,
            slow_poor: judges.slow_poor,
            fast_empty_poor: judges.fast_empty_poor,
            slow_empty_poor: judges.slow_empty_poor,
        }
    }
}

impl ResultSummary {
    pub fn from_play_result(
        result: &PlayResult,
        stored: &StoredPlayResult,
        chart: &bmz_chart::model::PlayableChart,
    ) -> Self {
        let metadata = &chart.metadata;
        let duration_ms = (chart.end_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let initial_bpm = metadata.initial_bpm as f32;
        Self {
            clear_type: result.clear_type,
            chart_md5: Some(chart.identity.file_md5),
            chart_sha256: Some(chart.identity.file_sha256),
            last_played_at: None,
            skin_attempt: bmz_render::snapshot::SkinAttemptState {
                effective_key_mode: Some(metadata.key_mode),
                ln_mode_index: Some(crate::skin_extension::long_note_mode_index(
                    metadata.long_note_mode,
                )),
                has_bga: Some(metadata.has_bga),
                has_random_sequence: Some(metadata.has_bms_random),
                ..Default::default()
            },
            target_name: String::new(),
            target: Default::default(),
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: result.score.ex_score(),
            max_combo: result.score.max_combo,
            bp: result.record_bp(),
            cb: result.record_cb(),
            gauge_value: result.gauge_value,
            gauge_type: result.gauge_type,
            total_notes: result.total_notes,
            duration_ms,
            initial_bpm,
            min_bpm: initial_bpm,
            max_bpm: initial_bpm,
            main_bpm: initial_bpm,
            total_gauge: gauge_total_for_chart(metadata.total, result.total_notes) as f32,
            judge_rank: metadata.judge_rank,
            note_judge_window: None,
            key_mode: metadata.key_mode,
            has_long_notes: !chart.long_notes.is_empty(),
            long_note_mode: metadata.long_note_mode,
            judge_counts: ResultJudgeCounts::from_judge_counts(&result.score.judges),
            fast_slow_counts: ResultFastSlowJudgeCounts::from_judge_counts(&result.score.judges),
            replay_path: stored.replay_path.clone(),
            replay_slots: stored.slot_paths.each_ref().map(Option::is_some),
            saved_replay_slots: stored.slot_paths.each_ref().map(Option::is_some),
            score_history_id: stored.score_history_id,
            best_ex_score: None,
            best_clear_type: None,
            best_max_combo: None,
            best_bp: None,
            previous_best_ex_score: None,
            previous_best_clear_type: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_ex_score: None,
            target_max_combo: None,
            target_bp: None,
            target_clear_type: None,
            ir_queued_jobs: 0,
            ir_last_error: None,
            title: metadata.title.clone(),
            subtitle: metadata.subtitle.clone(),
            artist: metadata.artist.clone(),
            subartist: metadata.subartist.clone(),
            genre: metadata.genre.clone(),
            difficulty_name: metadata.difficulty_name.clone(),
            play_level: metadata.play_level.clone(),
            battle_opponent: None,
            graph: Arc::new(ResultGraphSnapshot::default()),
        }
    }

    pub fn ex_score_rate(&self) -> f32 {
        if self.total_notes == 0 {
            1.0
        } else {
            self.ex_score as f32 / (self.total_notes * 2) as f32
        }
    }
}

impl ResultGraphCollector {
    pub(crate) fn for_runtime(chart: &PlayableChart) -> Self {
        let mut collector = Self::default();
        collector.graph.judge_graph_density =
            bmz_render::chart_graph::build_judge_graph_density(chart);
        collector.graph.bpm_graph_segments =
            bmz_render::chart_graph::build_bpm_graph_segments(chart);
        collector
            .graph
            .timing_points
            .reserve(bmz_gameplay::score::scored_note_count(chart) as usize);
        collector
    }
    /// Result data is sampled by gameplay, independently of snapshot publication.
    pub(crate) fn record_runtime_frame(
        &mut self,
        session: &GameSession,
        frame: &bmz_gameplay::session::SessionFrame,
    ) {
        let time_ms = clamp_us_to_ms(frame.times.audio_now.0.max(0));
        if self.next_gauge_sample_ms <= time_ms {
            self.graph.gauge_points.extend(session.gauge.gauges.iter().map(|gauge| {
                ResultGaugeGraphPoint {
                    time_ms: self.next_gauge_sample_ms,
                    value: gauge.value,
                    max: gauge.definition.max,
                    border: gauge.definition.border,
                    gauge_type: gauge.definition.gauge_type as i32,
                    course_section_start: false,
                }
            }));
            self.next_gauge_sample_ms =
                self.next_gauge_sample_ms.saturating_add(RESULT_GAUGE_GRAPH_SAMPLE_MS);
        }
        self.graph.hit_error_ring = bmz_render::snapshot::HitErrorRingSnapshot {
            values: session.hit_error_ring.values,
            index: session.hit_error_ring.index,
        };
        for event in frame.judgements.iter().filter(|event| event.affects_score) {
            let delta_us = -event.delta.0;
            self.graph.timing_points.push(ResultTimingPoint {
                time_ms: clamp_us_to_ms(event.time.0 - event.delta.0),
                delta_us,
                judge: event.judge,
            });
            self.graph.timing_distribution.add(clamp_us_to_ms(delta_us));
        }
    }
    pub fn snapshot_for_source(
        &self,
        source: &dyn crate::screens::play_finish::FinishSessionSource,
    ) -> ResultGraphSnapshot {
        source.result_graph(self)
    }
    pub fn record_frame(&mut self, frame: &FrameOutput<RenderSnapshot>) {
        let snapshot = &frame.render_snapshot;
        self.record_gauge(snapshot);
        if self.graph.judge_graph_density.is_empty() {
            self.graph.judge_graph_density = snapshot.judge_graph_density.to_vec();
        }
        if self.graph.bpm_graph_segments.is_empty() {
            self.graph.bpm_graph_segments = snapshot.bpm_graph_segments.to_vec();
        }
        self.graph.hit_error_ring = snapshot.hit_error_ring;

        for event in &frame.judgements {
            if !event.affects_score {
                continue;
            }
            let delta_us = -event.delta.0;
            self.graph.timing_points.push(ResultTimingPoint {
                time_ms: clamp_us_to_ms(event.time.0 - event.delta.0),
                delta_us,
                judge: event.judge,
            });
            self.graph.timing_distribution.add(clamp_us_to_ms(delta_us));
        }
    }

    pub fn snapshot(&self) -> ResultGraphSnapshot {
        let mut graph = self.graph.clone();
        graph.refresh_timing_metrics();
        graph
    }

    pub fn snapshot_for_session(&self, session: &GameSession) -> ResultGraphSnapshot {
        self.captured_for_session(session).snapshot_for_result_parts(
            &session.chart,
            &session.result_judgements,
            (session.state == bmz_gameplay::session::PlayState::Failed).then_some(&session.gauge),
        )
    }

    /// `session` から結果を取得するときと同じ状態で、全ゲージの終端点を加えた履歴。
    /// 結果が未確定の `Playing` セッションには終端点を加えない。
    pub(crate) fn captured_for_session(&self, session: &GameSession) -> Self {
        let mut collector = self.clone();
        if let Some(settled_at) = session.result_settled_at() {
            collector.record_terminal_gauges(&session.gauge, settled_at);
        }
        collector
    }

    /// 結果を確定した譜面時刻に、Result画面で切り替えられる全ゲージの値を終端点として記録する。
    ///
    /// 500ms周期の定期記録とは独立に、確定時の `value / max / border / gauge_type` を残す。
    /// 同じゲージ・同じ `time_ms` の末尾点があれば最新値へ更新し、時刻順を保つ。
    /// 確定は一度だけなので、2回目以降の呼び出しは何もしない。
    pub(crate) fn record_terminal_gauges(
        &mut self,
        gauge_state: &bmz_gameplay::gauge::GaugeState,
        settled_at: bmz_core::time::TimeUs,
    ) {
        if self.terminal_gauge_ms.is_some() {
            return;
        }
        let last_ms = self.graph.gauge_points.last().map_or(0, |point| point.time_ms);
        let time_ms = clamp_us_to_ms(settled_at.0.max(0)).max(last_ms);
        for gauge in &gauge_state.gauges {
            let point = ResultGaugeGraphPoint {
                time_ms,
                value: gauge.value,
                max: gauge.definition.max,
                border: gauge.definition.border,
                gauge_type: gauge.definition.gauge_type as i32,
                course_section_start: false,
            };
            match self
                .graph
                .gauge_points
                .iter_mut()
                .rev()
                .find(|existing| existing.gauge_type == point.gauge_type)
            {
                Some(existing) if existing.time_ms == time_ms => {
                    existing.value = point.value;
                    existing.max = point.max;
                    existing.border = point.border;
                }
                _ => self.graph.gauge_points.push(point),
            }
        }
        self.terminal_gauge_ms = Some(time_ms);
    }

    pub(crate) fn snapshot_for_result_parts(
        &self,
        chart: &PlayableChart,
        result_judgements: &HashMap<NoteId, ResultJudgementDetail>,
        failed_gauge: Option<&bmz_gameplay::gauge::GaugeState>,
    ) -> ResultGraphSnapshot {
        let mut graph = self.graph.clone();
        if let Some(gauge) = failed_gauge {
            fill_failed_gauge_tail(
                &mut graph,
                gauge,
                self.failed_tail_start_ms(),
                clamp_us_to_ms(chart.end_time.0).saturating_add(RESULT_GAUGE_GRAPH_SAMPLE_MS),
            );
        }
        populate_result_note_graphs(&mut graph, chart, result_judgements);
        graph.refresh_timing_metrics();
        graph
    }

    /// 途中FAILEDの0埋めは、停止境界の終端点より後の定期サンプル位置から始める。
    fn failed_tail_start_ms(&self) -> i32 {
        let Some(terminal_ms) = self.terminal_gauge_ms else {
            return self.next_gauge_sample_ms;
        };
        let after_terminal = terminal_ms
            .div_euclid(RESULT_GAUGE_GRAPH_SAMPLE_MS)
            .saturating_add(1)
            .saturating_mul(RESULT_GAUGE_GRAPH_SAMPLE_MS);
        self.next_gauge_sample_ms.max(after_terminal)
    }

    fn record_gauge(&mut self, snapshot: &RenderSnapshot) {
        let time_ms = clamp_us_to_ms(snapshot.play_elapsed_time.0.max(0));
        if self.next_gauge_sample_ms > time_ms {
            return;
        }

        let sample_time_ms = self.next_gauge_sample_ms;
        if snapshot.gauge_graph_points.is_empty() {
            self.graph.gauge_points.push(ResultGaugeGraphPoint {
                time_ms: sample_time_ms,
                value: snapshot.gauge,
                max: snapshot.gauge_max,
                border: snapshot.gauge_border,
                gauge_type: snapshot.gauge_type,
                course_section_start: false,
            });
        } else {
            self.graph.gauge_points.extend(
                snapshot
                    .gauge_graph_points
                    .iter()
                    .map(|point| ResultGaugeGraphPoint { time_ms: sample_time_ms, ..*point }),
            );
        }
        self.next_gauge_sample_ms =
            self.next_gauge_sample_ms.saturating_add(RESULT_GAUGE_GRAPH_SAMPLE_MS);
    }
}

fn fill_failed_gauge_tail(
    graph: &mut ResultGraphSnapshot,
    gauge_state: &bmz_gameplay::gauge::GaugeState,
    mut next_sample_ms: i32,
    end_ms: i32,
) {
    while next_sample_ms < end_ms {
        graph.gauge_points.extend(gauge_state.gauges.iter().map(|gauge| ResultGaugeGraphPoint {
            time_ms: next_sample_ms,
            value: 0.0,
            max: gauge.definition.max,
            border: gauge.definition.border,
            gauge_type: gauge.definition.gauge_type as i32,
            course_section_start: false,
        }));
        let next = next_sample_ms.saturating_add(RESULT_GAUGE_GRAPH_SAMPLE_MS);
        if next == next_sample_ms {
            break;
        }
        next_sample_ms = next;
    }
}

fn populate_result_note_graphs(
    graph: &mut ResultGraphSnapshot,
    chart: &PlayableChart,
    judgements: &HashMap<NoteId, ResultJudgementDetail>,
) {
    let seconds = bmz_render::chart_graph::note_graph_seconds(chart)
        .max(graph.judge_graph_density.len())
        .max(1);
    let mut judge_buckets = vec![ResultJudgeGraphBucket::default(); seconds];
    let mut early_late_buckets = vec![ResultEarlyLateGraphBucket::default(); seconds];
    let mut timing_points = Vec::new();

    for (note, second) in bmz_render::chart_graph::judge_graph_notes(chart, seconds) {
        let Some(detail) = judgements.get(&note.id) else {
            judge_buckets[second].values[0] = judge_buckets[second].values[0].saturating_add(1);
            early_late_buckets[second].values[0] =
                early_late_buckets[second].values[0].saturating_add(1);
            continue;
        };

        let state = beatoraja_note_state(detail.judge);
        judge_buckets[second].values[state] = judge_buckets[second].values[state].saturating_add(1);
        let early_late = beatoraja_early_late_state(state, detail);
        early_late_buckets[second].values[early_late] =
            early_late_buckets[second].values[early_late].saturating_add(1);

        let delta_us = -detail.delta.0;
        timing_points.push(ResultTimingPoint {
            time_ms: clamp_us_to_ms(note.time.0),
            delta_us,
            judge: detail.judge,
        });
    }

    graph.judge_graph_buckets = judge_buckets;
    graph.note_graph_buckets = bmz_render::chart_graph::build_note_graph_buckets(chart, seconds);
    graph.early_late_graph_buckets = early_late_buckets;
    graph.timing_points = timing_points;
    graph.timing_distribution = timing_distribution_from_points(&graph.timing_points);
    if graph.judge_graph_density.is_empty() {
        graph.judge_graph_density = graph
            .judge_graph_buckets
            .iter()
            .map(|bucket| bucket.total().min(u8::MAX as u32) as u8)
            .collect();
    }
}

/// How often beatoraja redraws a judge graph while playing.
const PLAY_JUDGE_GRAPH_UPDATE_INTERVAL: Duration = Duration::from_millis(750);

#[derive(Debug, Clone, Copy)]
struct TrackedGraphNote {
    second: usize,
    state: usize,
    early_late: usize,
}

/// Live judge and early/late buckets for Play skins. The gameplay thread updates
/// them from the judgement events, as `populate_result_note_graphs` does at the end.
#[derive(Debug)]
pub(crate) struct PlayJudgeGraphTracker {
    notes: HashMap<NoteId, TrackedGraphNote>,
    judge: Vec<ResultJudgeGraphBucket>,
    early_late: Vec<ResultEarlyLateGraphBucket>,
    changed: bool,
    published_at: Option<Duration>,
}

impl PlayJudgeGraphTracker {
    pub(crate) fn new(
        chart: &PlayableChart,
        judgements: &HashMap<NoteId, ResultJudgementDetail>,
        graphs: &PlayNoteGraphs,
    ) -> Self {
        let seconds = graphs.judge.len();
        let notes = bmz_render::chart_graph::judge_graph_notes(chart, seconds)
            .into_iter()
            .map(|(note, second)| (note.id, TrackedGraphNote { second, state: 0, early_late: 0 }))
            .collect();
        let mut tracker = Self {
            notes,
            judge: graphs.judge.to_vec(),
            early_late: graphs.early_late.to_vec(),
            changed: false,
            published_at: None,
        };
        // A viewer start position judges the notes before it up front.
        for (note_id, detail) in judgements {
            tracker.apply(*note_id, detail);
        }
        tracker
    }

    /// `events` name the notes whose entry in `judgements` a frame changed.
    pub(crate) fn record(
        &mut self,
        judgements: &HashMap<NoteId, ResultJudgementDetail>,
        events: &[bmz_gameplay::judge::model::JudgementEvent],
    ) {
        for note_id in events.iter().filter_map(|event| event.note_id) {
            if let Some(detail) = judgements.get(&note_id) {
                self.apply(note_id, detail);
            }
        }
    }

    fn apply(&mut self, note_id: NoteId, detail: &ResultJudgementDetail) {
        let Some(note) = self.notes.get_mut(&note_id) else {
            return;
        };
        let state = beatoraja_note_state(detail.judge);
        let early_late = beatoraja_early_late_state(state, detail);
        if (note.state, note.early_late) == (state, early_late) {
            return;
        }
        let judge = &mut self.judge[note.second].values;
        judge[note.state] = judge[note.state].saturating_sub(1);
        judge[state] = judge[state].saturating_add(1);
        let values = &mut self.early_late[note.second].values;
        values[note.early_late] = values[note.early_late].saturating_sub(1);
        values[early_late] = values[early_late].saturating_add(1);
        note.state = state;
        note.early_late = early_late;
        self.changed = true;
    }

    /// Copies the buckets into `graphs` after a change, at most once per interval.
    /// `now` is any monotonic clock, which keeps offline exports reproducible.
    pub(crate) fn publish(&mut self, now: Duration, graphs: &mut PlayNoteGraphs) {
        let waiting = self.published_at.is_some_and(|published_at| {
            now.saturating_sub(published_at) < PLAY_JUDGE_GRAPH_UPDATE_INTERVAL
        });
        if !self.changed || waiting {
            return;
        }
        graphs.judge = Arc::from(self.judge.as_slice());
        graphs.early_late = Arc::from(self.early_late.as_slice());
        graphs.judge_revision = graphs.judge_revision.wrapping_add(1);
        self.changed = false;
        self.published_at = Some(now);
    }
}

fn beatoraja_note_state(judge: bmz_core::judge::Judge) -> usize {
    match judge {
        bmz_core::judge::Judge::PGreat => 1,
        bmz_core::judge::Judge::Great => 2,
        bmz_core::judge::Judge::Good => 3,
        bmz_core::judge::Judge::Bad => 4,
        bmz_core::judge::Judge::Poor | bmz_core::judge::Judge::EmptyPoor => 5,
    }
}

fn beatoraja_early_late_state(state: usize, detail: &ResultJudgementDetail) -> usize {
    if state <= 1 {
        return state;
    }
    if detail.delta.0 <= 0 { state } else { state + 4 }
}

fn timing_distribution_from_points(points: &[ResultTimingPoint]) -> ResultTimingDistribution {
    let mut distribution = ResultTimingDistribution::default();
    for point in points {
        distribution.add(clamp_us_to_ms(point.delta_us));
    }
    distribution
}

fn clamp_us_to_ms(us: i64) -> i32 {
    (us / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[cfg(test)]
#[path = "result_model/tests.rs"]
mod tests;
