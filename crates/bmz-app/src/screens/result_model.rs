use bmz_core::clear::ClearType;
use bmz_gameplay::result::PlayResult;

use crate::storage::play_result::StoredPlayResult;

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSummary {
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub max_combo: u32,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub replay_path: String,
    pub score_history_id: i64,
}

impl ResultSummary {
    pub fn from_play_result(result: &PlayResult, stored: &StoredPlayResult) -> Self {
        Self {
            clear_type: result.clear_type,
            ex_score: result.score.ex_score(),
            max_combo: result.score.max_combo,
            gauge_value: result.gauge_value,
            total_notes: result.total_notes,
            replay_path: stored.replay_path.clone(),
            score_history_id: stored.score_history_id,
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

        let summary = ResultSummary::from_play_result(&result, &stored);

        assert_eq!(summary.clear_type, ClearType::Normal);
        assert_eq!(summary.max_combo, 12);
        assert_eq!(summary.gauge_value, 82.0);
        assert_eq!(summary.score_history_id, 9);
        assert_eq!(summary.replay_path, "replay/test.toml");
    }
}
