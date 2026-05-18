use bmz_chart::model::ChartMetadata;
use bmz_core::clear::ClearType;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::JudgeCounts;

use crate::storage::play_result::StoredPlayResult;

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSummary {
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub max_combo: u32,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub judge_counts: ResultJudgeCounts,
    pub fast_slow_counts: ResultFastSlowJudgeCounts,
    pub replay_path: String,
    pub score_history_id: i64,
    pub best_ex_score: Option<u32>,
    pub best_max_combo: Option<u32>,
    pub best_misscount: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub target_misscount: Option<u32>,
    pub target_clear_type: Option<ClearType>,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
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
            total_notes: result.total_notes,
            judge_counts: ResultJudgeCounts::from_judge_counts(&result.score.judges),
            fast_slow_counts: ResultFastSlowJudgeCounts::from_judge_counts(&result.score.judges),
            replay_path: stored.replay_path.clone(),
            score_history_id: stored.score_history_id,
            best_ex_score: None,
            best_max_combo: None,
            best_misscount: None,
            target_ex_score: None,
            target_max_combo: None,
            target_misscount: None,
            target_clear_type: None,
            title: metadata.title.clone(),
            subtitle: metadata.subtitle.clone(),
            artist: metadata.artist.clone(),
            subartist: metadata.subartist.clone(),
            genre: metadata.genre.clone(),
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

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_gameplay::score::ScoreState;

    use super::*;

    #[test]
    fn result_summary_uses_play_and_storage_values() {
        let mut score = ScoreState::default();
        score.max_combo = 12;
        score.judges.fast_pgreat = 2;
        score.judges.slow_great = 3;
        score.judges.fast_good = 4;
        score.judges.slow_bad = 5;
        score.judges.fast_poor = 6;
        score.judges.slow_empty_poor = 7;
        let result = PlayResult {
            chart_sha256: [1; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 82.0,
            total_notes: 20,
            score,
            autoplay: false,
        };
        let stored =
            StoredPlayResult { score_history_id: 9, replay_path: "replay/test.toml".to_string() };
        let metadata = ChartMetadata { title: "Test".to_string(), ..ChartMetadata::default() };

        let summary = ResultSummary::from_play_result(&result, &stored, &metadata);

        assert_eq!(summary.title, "Test");
        assert_eq!(summary.clear_type, ClearType::Normal);
        assert_eq!(summary.max_combo, 12);
        assert_eq!(summary.gauge_value, 82.0);
        assert_eq!(summary.score_history_id, 9);
        assert_eq!(summary.replay_path, "replay/test.toml");
        assert_eq!(
            summary.judge_counts,
            ResultJudgeCounts { pgreat: 2, great: 3, good: 4, bad: 5, poor: 6, empty_poor: 7 }
        );
    }
}
