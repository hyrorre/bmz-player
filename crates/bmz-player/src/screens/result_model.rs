use std::collections::HashMap;

use bmz_chart::model::{LongNoteMode, NoteEvent, NoteKind, PlayableChart};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::ids::NoteId;
use bmz_core::lane::KeyMode;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::JudgeCounts;
use bmz_gameplay::session::{FrameOutput, GameSession, ResultJudgementDetail};
use bmz_render::snapshot::{
    RenderSnapshot, ResultEarlyLateGraphBucket, ResultGaugeGraphPoint, ResultGraphSnapshot,
    ResultJudgeGraphBucket, ResultTimingDistribution, ResultTimingPoint,
};

use crate::storage::play_result::StoredPlayResult;

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSummary {
    pub clear_type: ClearType,
    pub arrange: String,
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
    pub key_mode: KeyMode,
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
    pub graph: ResultGraphSnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct ResultGraphCollector {
    graph: ResultGraphSnapshot,
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
            arrange: "NORMAL".to_string(),
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
            total_gauge: metadata.total.unwrap_or(0.0) as f32,
            judge_rank: metadata.judge_rank,
            key_mode: metadata.key_mode,
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
            graph: ResultGraphSnapshot::default(),
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
        self.graph.clone()
    }

    pub fn snapshot_for_session(&self, session: &GameSession) -> ResultGraphSnapshot {
        let mut graph = self.graph.clone();
        populate_result_note_graphs(&mut graph, &session.chart, &session.result_judgements);
        graph
    }

    fn record_gauge(&mut self, snapshot: &RenderSnapshot) {
        let time_ms = clamp_us_to_ms(snapshot.time.0.max(0));
        if snapshot.gauge_graph_points.is_empty() {
            self.record_gauge_point(ResultGaugeGraphPoint {
                time_ms,
                value: snapshot.gauge,
                max: snapshot.gauge_max,
                border: snapshot.gauge_border,
                gauge_type: snapshot.gauge_type,
            });
            return;
        }

        for point in &snapshot.gauge_graph_points {
            self.record_gauge_point(ResultGaugeGraphPoint { time_ms, ..*point });
        }
    }

    fn record_gauge_point(&mut self, point: ResultGaugeGraphPoint) {
        let Some(last_index) = self
            .graph
            .gauge_points
            .iter()
            .rposition(|candidate| candidate.gauge_type == point.gauge_type)
        else {
            self.graph.gauge_points.push(point);
            return;
        };

        let last = self.graph.gauge_points[last_index];
        if last.time_ms == point.time_ms {
            self.graph.gauge_points[last_index] = point;
            return;
        }
        if same_gauge_graph_state(last, point) {
            let previous = self.graph.gauge_points[..last_index]
                .iter()
                .rfind(|candidate| candidate.gauge_type == point.gauge_type)
                .copied();
            if let Some(previous) = previous {
                if same_gauge_graph_state(previous, last) {
                    self.graph.gauge_points[last_index] = point;
                } else {
                    self.graph.gauge_points.push(point);
                }
            } else {
                self.graph.gauge_points.push(point);
            }
            return;
        }
        self.graph.gauge_points.push(point);
    }
}

fn same_gauge_graph_state(a: ResultGaugeGraphPoint, b: ResultGaugeGraphPoint) -> bool {
    a.value.to_bits() == b.value.to_bits()
        && a.max.to_bits() == b.max.to_bits()
        && a.border.to_bits() == b.border.to_bits()
        && a.gauge_type == b.gauge_type
}

fn populate_result_note_graphs(
    graph: &mut ResultGraphSnapshot,
    chart: &PlayableChart,
    judgements: &HashMap<NoteId, ResultJudgementDetail>,
) {
    let seconds = result_graph_seconds(chart).max(graph.judge_graph_density.len()).max(1);
    let mut judge_buckets = vec![ResultJudgeGraphBucket::default(); seconds];
    let mut early_late_buckets = vec![ResultEarlyLateGraphBucket::default(); seconds];
    let mut timing_points = Vec::new();

    let mut notes: Vec<&NoteEvent> = chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| result_graph_includes_note(chart, note))
        .collect();
    notes.sort_by_key(|note| (note.time.0, note.lane.index(), note.id.0));

    for note in notes {
        let second = clamp_note_second(note, seconds);
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

fn result_graph_seconds(chart: &PlayableChart) -> usize {
    (chart.end_time.0 / 1_000_000).max(0) as usize + 1
}

fn result_graph_includes_note(chart: &PlayableChart, note: &NoteEvent) -> bool {
    match note.kind {
        NoteKind::Tap | NoteKind::LongStart => true,
        NoteKind::LongEnd => !is_ignored_long_end(chart, note.id),
        NoteKind::Invisible | NoteKind::Mine => false,
    }
}

fn is_ignored_long_end(chart: &PlayableChart, note_id: NoteId) -> bool {
    let mode = chart
        .long_notes
        .iter()
        .find(|pair| pair.end_note_id == note_id)
        .and_then(|pair| pair.mode)
        .unwrap_or(chart.metadata.long_note_mode);
    mode == LongNoteMode::Ln
}

fn clamp_note_second(note: &NoteEvent, seconds: usize) -> usize {
    let second = (note.time.0 / 1_000_000).max(0) as usize;
    second.min(seconds.max(1) - 1)
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
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::judge::model::JudgementEvent;
    use bmz_gameplay::score::ScoreState;
    use bmz_gameplay::session::PlayState;
    use bmz_render::chart_graph::BpmGraphSegment;
    use bmz_render::snapshot::HitErrorRingSnapshot;

    use super::*;

    #[test]
    fn result_summary_uses_play_and_storage_values() {
        let result = play_result();
        let stored = stored_result();
        let chart = chart();

        let summary = ResultSummary::from_play_result(&result, &stored, &chart);

        assert_eq!(summary.title, "Test");
        assert_eq!(summary.duration_ms, 90_000);
        assert_eq!(summary.initial_bpm, 128.0);
        assert_eq!(summary.clear_type, ClearType::Normal);
        assert_eq!(summary.gauge_type, GaugeType::Normal);
        assert_eq!(summary.max_combo, 12);
        assert_eq!(summary.bp, 18);
        assert_eq!(summary.cb, 11);
        assert_eq!(summary.gauge_value, 82.0);
        assert_eq!(summary.score_history_id, 9);
        assert_eq!(summary.replay_path, "replay/test.toml");
        assert_eq!(
            summary.judge_counts,
            ResultJudgeCounts { pgreat: 2, great: 3, good: 4, bad: 5, poor: 6, empty_poor: 7 }
        );
    }

    #[test]
    fn failed_result_summary_uses_record_bp_and_cb() {
        let mut result = play_result();
        result.clear_type = ClearType::Failed;
        result.score = bmz_gameplay::score::ScoreState::default();
        let stored = stored_result();
        let chart = chart();

        let summary = ResultSummary::from_play_result(&result, &stored, &chart);

        assert_eq!(summary.bp, chart.total_notes);
        assert_eq!(summary.cb, chart.total_notes);
    }

    fn play_result() -> PlayResult {
        let score = ScoreState {
            max_combo: 12,
            judges: bmz_gameplay::score::JudgeCounts {
                fast_pgreat: 2,
                slow_great: 3,
                fast_good: 4,
                slow_bad: 5,
                fast_poor: 6,
                slow_empty_poor: 7,
                ..Default::default()
            },
            ..Default::default()
        };
        PlayResult {
            chart_sha256: [1; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 82.0,
            total_notes: 20,
            score,
            autoplay: false,
        }
    }

    fn stored_result() -> StoredPlayResult {
        StoredPlayResult {
            score_history_id: 9,
            played_at: 0,
            replay_path: "replay/test.toml".to_string(),
            replay_sha256: None,
            slot_paths: [None, None, None, None],
            device_type: bmz_core::input::InputDeviceKind::Keyboard,
        }
    }

    fn chart() -> bmz_chart::model::PlayableChart {
        bmz_chart::model::PlayableChart {
            identity: bmz_core::chart::ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: bmz_chart::model::ChartMetadata {
                title: "Test".to_string(),
                initial_bpm: 128.0,
                ..bmz_chart::model::ChartMetadata::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 20,
            end_time: TimeUs(90_000_000),
        }
    }

    #[test]
    fn result_graph_collector_carries_frame_graph_data() {
        let hit_error_ring = HitErrorRingSnapshot { index: 7, ..HitErrorRingSnapshot::default() };
        let mut render_snapshot = RenderSnapshot {
            time: TimeUs(1_234_000),
            gauge: 72.5,
            gauge_type: GaugeType::Hard as i32,
            gauge_border: 30.0,
            judge_graph_density: vec![1, 3, 2].into(),
            bpm_graph_segments: vec![BpmGraphSegment {
                start_ratio: 0.0,
                end_ratio: 1.0,
                bpm: 180.0,
                is_stop: false,
            }]
            .into(),
            hit_error_ring,
            ..RenderSnapshot::default()
        };
        let frame = FrameOutput {
            render_snapshot: render_snapshot.clone(),
            judgements: vec![JudgementEvent {
                note_id: None,
                lane: Lane::Key1,
                judge: Judge::Great,
                side: TimingSide::Fast,
                delta: TimeUs(-12_000),
                time: TimeUs(1_200_000),
                affects_score: true,
            }],
            mine_hits: Vec::new(),
            keysound_volumes: Vec::new(),
            state: PlayState::Playing,
        };
        let mut collector = ResultGraphCollector::default();
        collector.record_frame(&frame);

        render_snapshot.time = TimeUs(1_234_500);
        render_snapshot.gauge = 74.0;
        collector.record_frame(&FrameOutput {
            render_snapshot,
            judgements: Vec::new(),
            mine_hits: Vec::new(),
            keysound_volumes: Vec::new(),
            state: PlayState::Playing,
        });

        let graph = collector.snapshot();
        assert_eq!(graph.gauge_points.len(), 1);
        assert_eq!(graph.gauge_points[0].time_ms, 1234);
        assert_eq!(graph.gauge_points[0].value, 74.0);
        assert_eq!(graph.timing_points.len(), 1);
        assert_eq!(graph.timing_points[0].delta_us, 12_000);
        assert_eq!(graph.timing_distribution.counts[(150 + 12) as usize], 1);
        assert_eq!(graph.judge_graph_density, vec![1, 3, 2]);
        assert_eq!(graph.bpm_graph_segments.len(), 1);
        assert_eq!(graph.hit_error_ring.index, 7);
    }

    #[test]
    fn result_graph_collector_compresses_unchanged_gauge_points() {
        fn record(collector: &mut ResultGraphCollector, time_us: i64, gauge: f32) {
            collector.record_frame(&FrameOutput {
                render_snapshot: RenderSnapshot {
                    time: TimeUs(time_us),
                    gauge,
                    gauge_type: GaugeType::Normal as i32,
                    gauge_border: 80.0,
                    ..RenderSnapshot::default()
                },
                judgements: Vec::new(),
                mine_hits: Vec::new(),
                keysound_volumes: Vec::new(),
                state: PlayState::Playing,
            });
        }

        let mut collector = ResultGraphCollector::default();
        record(&mut collector, 0, 20.0);
        record(&mut collector, 16_000, 20.0);
        record(&mut collector, 32_000, 35.0);
        record(&mut collector, 48_000, 35.0);
        record(&mut collector, 64_000, 35.0);
        record(&mut collector, 80_000, 42.0);

        let graph = collector.snapshot();
        assert_eq!(
            graph.gauge_points.iter().map(|point| (point.time_ms, point.value)).collect::<Vec<_>>(),
            vec![(0, 20.0), (16, 20.0), (32, 35.0), (64, 35.0), (80, 42.0)]
        );
    }

    #[test]
    fn result_graph_collector_records_each_gauge_type() {
        fn record(collector: &mut ResultGraphCollector, time_us: i64, normal: f32, easy: f32) {
            collector.record_frame(&FrameOutput {
                render_snapshot: RenderSnapshot {
                    time: TimeUs(time_us),
                    gauge_graph_points: vec![
                        ResultGaugeGraphPoint {
                            time_ms: 0,
                            value: normal,
                            max: 100.0,
                            border: 80.0,
                            gauge_type: GaugeType::Normal as i32,
                        },
                        ResultGaugeGraphPoint {
                            time_ms: 0,
                            value: easy,
                            max: 100.0,
                            border: 80.0,
                            gauge_type: GaugeType::Easy as i32,
                        },
                    ],
                    ..RenderSnapshot::default()
                },
                judgements: Vec::new(),
                mine_hits: Vec::new(),
                keysound_volumes: Vec::new(),
                state: PlayState::Playing,
            });
        }

        let mut collector = ResultGraphCollector::default();
        record(&mut collector, 0, 20.0, 20.0);
        record(&mut collector, 1_000_000, 70.0, 90.0);

        let graph = collector.snapshot();
        let normal = graph
            .gauge_points
            .iter()
            .filter(|point| point.gauge_type == GaugeType::Normal as i32)
            .map(|point| (point.time_ms, point.value))
            .collect::<Vec<_>>();
        let easy = graph
            .gauge_points
            .iter()
            .filter(|point| point.gauge_type == GaugeType::Easy as i32)
            .map(|point| (point.time_ms, point.value))
            .collect::<Vec<_>>();

        assert_eq!(normal, vec![(0, 20.0), (1000, 70.0)]);
        assert_eq!(easy, vec![(0, 20.0), (1000, 90.0)]);
    }

    #[test]
    fn result_graph_collector_builds_beatoraja_result_buckets_from_session_judgements() {
        let mut chart = chart();
        chart.end_time = TimeUs(2_000_000);
        chart.total_notes = 3;
        chart.lane_notes[Lane::Key1.index()] =
            vec![note(1, 0), note(2, 1_000_000), note(3, 1_000_000)];
        let mut judgements = std::collections::HashMap::new();
        judgements.insert(
            bmz_core::ids::NoteId(1),
            bmz_gameplay::session::ResultJudgementDetail {
                judge: Judge::Great,
                side: TimingSide::Fast,
                delta: TimeUs(-12_000),
                time: TimeUs(12_000),
            },
        );
        judgements.insert(
            bmz_core::ids::NoteId(2),
            bmz_gameplay::session::ResultJudgementDetail {
                judge: Judge::Bad,
                side: TimingSide::Slow,
                delta: TimeUs(45_000),
                time: TimeUs(1_045_000),
            },
        );

        let mut graph = ResultGraphSnapshot::default();
        populate_result_note_graphs(&mut graph, &chart, &judgements);

        assert_eq!(graph.judge_graph_buckets[0].values[2], 1);
        assert_eq!(graph.early_late_graph_buckets[0].values[2], 1);
        assert_eq!(graph.judge_graph_buckets[1].values[4], 1);
        assert_eq!(graph.judge_graph_buckets[1].values[0], 1);
        assert_eq!(graph.early_late_graph_buckets[1].values[8], 1);
        assert_eq!(graph.early_late_graph_buckets[1].values[0], 1);
        assert_eq!(
            graph.timing_points.iter().map(|point| point.delta_us).collect::<Vec<_>>(),
            vec![12_000, -45_000]
        );
        assert_eq!(graph.timing_distribution.counts[(150 + 12) as usize], 1);
        assert_eq!(graph.timing_distribution.counts[(150 - 45) as usize], 1);
    }

    fn note(id: u32, time_us: i64) -> bmz_chart::model::NoteEvent {
        bmz_chart::model::NoteEvent {
            id: bmz_core::ids::NoteId(id),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Tap,
            tick: bmz_core::time::ChartTick(0),
            time: TimeUs(time_us),
            sound: None,
            damage: None,
        }
    }
}
