use bmz_core::clear::ClearType;

use crate::snapshot::RenderSnapshot;

#[derive(Debug, Clone, PartialEq)]
pub enum AppSceneSnapshot {
    Select(SelectSnapshot),
    Play(RenderSnapshot),
    Result(ResultSnapshot),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectSnapshot {
    pub chart_count: u32,
    pub selected_index: u32,
    pub selected_chart_id: Option<i64>,
    pub selected_title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSnapshot {
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub ex_score_rate: f32,
    pub max_combo: u32,
    pub gauge_value: f32,
    pub total_notes: u32,
}

impl ResultSnapshot {
    pub fn is_full_combo(&self) -> bool {
        self.total_notes > 0 && self.max_combo >= self.total_notes
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::ClearType;

    use super::*;

    #[test]
    fn result_snapshot_detects_full_combo() {
        let snapshot = ResultSnapshot {
            clear_type: ClearType::Normal,
            ex_score: 20,
            ex_score_rate: 1.0,
            max_combo: 10,
            gauge_value: 100.0,
            total_notes: 10,
        };

        assert!(snapshot.is_full_combo());
    }

    #[test]
    fn zero_note_result_is_not_full_combo() {
        let snapshot = ResultSnapshot {
            clear_type: ClearType::Normal,
            ex_score: 0,
            ex_score_rate: 1.0,
            max_combo: 0,
            gauge_value: 100.0,
            total_notes: 0,
        };

        assert!(!snapshot.is_full_combo());
    }
}
