use bmz_chart::model::ChartMetadata;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::JudgeCounts;
use bmz_gameplay::session::FrameOutput;
use bmz_render::snapshot::{
    RenderSnapshot, ResultGaugeGraphPoint, ResultGraphSnapshot, ResultTimingPoint,
};

use crate::storage::play_result::StoredPlayResult;

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSummary {
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub max_combo: u32,
    pub gauge_value: f32,
    pub gauge_type: GaugeType,
    pub total_notes: u32,
    pub judge_counts: ResultJudgeCounts,
    pub fast_slow_counts: ResultFastSlowJudgeCounts,
    pub replay_path: String,
    pub score_history_id: i64,
    pub best_ex_score: Option<u32>,
    pub best_clear_type: Option<ClearType>,
    pub best_max_combo: Option<u32>,
    pub best_misscount: Option<u32>,
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_misscount: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub target_misscount: Option<u32>,
    pub target_clear_type: Option<ClearType>,
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
        metadata: &ChartMetadata,
    ) -> Self {
        Self {
            clear_type: result.clear_type,
            ex_score: result.score.ex_score(),
            max_combo: result.score.max_combo,
            gauge_value: result.gauge_value,
            gauge_type: result.gauge_type,
            total_notes: result.total_notes,
            judge_counts: ResultJudgeCounts::from_judge_counts(&result.score.judges),
            fast_slow_counts: ResultFastSlowJudgeCounts::from_judge_counts(&result.score.judges),
            replay_path: stored.replay_path.clone(),
            score_history_id: stored.score_history_id,
            best_ex_score: None,
            best_clear_type: None,
            best_max_combo: None,
            best_misscount: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_misscount: None,
            target_ex_score: None,
            target_max_combo: None,
            target_misscount: None,
            target_clear_type: None,
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
        self.graph.judge_graph_density = snapshot.judge_graph_density.clone();
        self.graph.bpm_graph_segments = snapshot.bpm_graph_segments.clone();
        self.graph.hit_error_ring = snapshot.hit_error_ring;

        for event in &frame.judgements {
            self.graph.timing_points.push(ResultTimingPoint {
                time_ms: clamp_us_to_ms(event.time.0),
                delta_us: event.delta.0,
                judge: event.judge,
            });
        }
    }

    pub fn snapshot(&self) -> ResultGraphSnapshot {
        self.graph.clone()
    }

    fn record_gauge(&mut self, snapshot: &RenderSnapshot) {
        let time_ms = clamp_us_to_ms(snapshot.time.0.max(0));
        let point = ResultGaugeGraphPoint {
            time_ms,
            value: snapshot.gauge,
            border: snapshot.gauge_border,
            gauge_type: snapshot.gauge_type,
        };
        if let Some(last) = self.graph.gauge_points.last_mut()
            && last.time_ms == point.time_ms
        {
            *last = point;
            return;
        }
        self.graph.gauge_points.push(point);
    }
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
        let result = PlayResult {
            chart_sha256: [1; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 82.0,
            total_notes: 20,
            score,
            autoplay: false,
        };
        let stored = StoredPlayResult {
            score_history_id: 9,
            replay_path: "replay/test.toml".to_string(),
            slot_paths: [None, None, None, None],
        };
        let metadata = ChartMetadata { title: "Test".to_string(), ..ChartMetadata::default() };

        let summary = ResultSummary::from_play_result(&result, &stored, &metadata);

        assert_eq!(summary.title, "Test");
        assert_eq!(summary.clear_type, ClearType::Normal);
        assert_eq!(summary.gauge_type, GaugeType::Normal);
        assert_eq!(summary.max_combo, 12);
        assert_eq!(summary.gauge_value, 82.0);
        assert_eq!(summary.score_history_id, 9);
        assert_eq!(summary.replay_path, "replay/test.toml");
        assert_eq!(
            summary.judge_counts,
            ResultJudgeCounts { pgreat: 2, great: 3, good: 4, bad: 5, poor: 6, empty_poor: 7 }
        );
    }

    #[test]
    fn result_graph_collector_carries_frame_graph_data() {
        let hit_error_ring = HitErrorRingSnapshot { index: 7, ..HitErrorRingSnapshot::default() };
        let mut render_snapshot = RenderSnapshot {
            time: TimeUs(1_234_000),
            gauge: 72.5,
            gauge_type: GaugeType::Hard as i32,
            gauge_border: 30.0,
            judge_graph_density: vec![1, 3, 2],
            bpm_graph_segments: vec![BpmGraphSegment {
                start_ratio: 0.0,
                end_ratio: 1.0,
                bpm: 180.0,
                is_stop: false,
            }],
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
            }],
            mine_hits: Vec::new(),
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
            state: PlayState::Playing,
        });

        let graph = collector.snapshot();
        assert_eq!(graph.gauge_points.len(), 1);
        assert_eq!(graph.gauge_points[0].time_ms, 1234);
        assert_eq!(graph.gauge_points[0].value, 74.0);
        assert_eq!(graph.timing_points.len(), 1);
        assert_eq!(graph.timing_points[0].delta_us, -12_000);
        assert_eq!(graph.judge_graph_density, vec![1, 3, 2]);
        assert_eq!(graph.bpm_graph_segments.len(), 1);
        assert_eq!(graph.hit_error_ring.index, 7);
    }
}
