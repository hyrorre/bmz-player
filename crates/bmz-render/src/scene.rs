use bmz_core::clear::ClearType;
use bmz_core::time::TimeUs;

use crate::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts, RenderSnapshot};

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum AppSceneSnapshot {
    Select(SelectSnapshot),
    Play(RenderSnapshot),
    Result(ResultSnapshot),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectSnapshot {
    pub time: TimeUs,
    pub selection_time: TimeUs,
    pub option_panel_time: TimeUs,
    pub option_panel: u8,
    pub chart_count: u32,
    pub selected_index: u32,
    pub selected_chart_id: Option<i64>,
    pub selected_title: String,
    pub rows: Vec<SelectRowSnapshot>,
    pub arrange: String,
    pub gauge: String,
    pub assist: String,
    pub bga: String,
    pub current_folder: String,
    pub key_hint: String,
    pub option_hint: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectRowSnapshot {
    pub index: u32,
    pub title: String,
    pub artist: String,
    pub play_level: String,
    pub table_level: String,
    pub total_notes: u32,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub length_ms: i64,
    pub clear_type: String,
    pub ex_score: Option<u32>,
    pub replay_slots: [bool; 4],
    pub is_folder: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSnapshot {
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub ex_score_rate: f32,
    pub max_combo: u32,
    pub gauge_value: f32,
    pub total_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub score_history_id: i64,
    pub replay_saved: bool,
    pub best_ex_score: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub best_max_combo: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub best_misscount: Option<u32>,
    pub target_misscount: Option<u32>,
    pub target_clear_type: Option<ClearType>,
    /// リザルト画面を開いてからの経過時間。
    /// destination の timer/loop/keyframe アニメーション、image cycle に使われる。
    pub elapsed_time: TimeUs,
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
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 1,
            replay_saved: true,
            best_ex_score: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
            target_clear_type: None,
            elapsed_time: TimeUs(0),
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
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 1,
            replay_saved: true,
            best_ex_score: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
            target_clear_type: None,
            elapsed_time: TimeUs(0),
        };

        assert!(!snapshot.is_full_combo());
    }
}
