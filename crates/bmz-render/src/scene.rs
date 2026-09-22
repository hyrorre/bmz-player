use std::sync::Arc;

use bmz_core::clear::ClearType;
use bmz_core::lane::KeyMode;
use bmz_core::time::TimeUs;

use crate::chart_graph::BpmGraphSegment;
use crate::skin::SkinImageSize;
use crate::skin_offset::SkinOffsetValues;
use crate::snapshot::{
    DisplayJudgeCounts, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot, SkinAttemptState,
    SkinLogicalInputSnapshot,
};

#[derive(Debug, Clone, PartialEq)]
// シーン snapshot は毎フレーム構築されるため、variant の Box 化による
// フレーム単位のヒープ割当を避け、値のまま renderer へ受け渡す。
pub enum AppSceneSnapshot {
    Select(SelectSnapshot),
    Decide(RenderSnapshot),
    Play(RenderSnapshot),
    Result(ResultSnapshot),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DailyPlayerStatsSnapshot {
    pub play_count: u64,
    pub clear_count: u64,
    pub pgreat: u64,
    pub great: u64,
    pub good: u64,
    pub bad: u64,
    pub poor: u64,
    pub empty_poor: u64,
    pub score_update_count: u64,
    pub clear_update_count: u64,
    pub miss_count_update_count: u64,
    /// Most recent locally played titles in the current statistics day.
    pub recent_titles: [String; 10],
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerStatsSnapshot {
    /// Current process only: locally saved plays, excluding replay/autoplay.
    pub session_play_count: u64,
    pub session_play_notes: u64,
    pub play_count: u64,
    pub clear_count: u64,
    pub playtime_seconds: u64,
    pub max_combo: u32,
    pub fast_pgreat: u64,
    pub slow_pgreat: u64,
    pub fast_great: u64,
    pub slow_great: u64,
    pub fast_good: u64,
    pub slow_good: u64,
    pub fast_bad: u64,
    pub slow_bad: u64,
    pub fast_poor: u64,
    pub slow_poor: u64,
    pub fast_empty_poor: u64,
    pub slow_empty_poor: u64,
    pub daily: DailyPlayerStatsSnapshot,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CourseStageResultSkinSnapshot {
    pub ex_score: u32,
    pub gauge: f32,
    pub bp: u32,
    pub rate_basis_points: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CourseResultSkinSnapshot {
    pub stage_count: u32,
    pub stages: [CourseStageResultSkinSnapshot; bmz_skin_document::SKIN_BMZ_COURSE_STAGE_COUNT],
}

#[path = "scene/result.rs"]
mod scene_result;
#[path = "scene/select.rs"]
mod scene_select;

pub use scene_result::*;
pub use scene_select::*;

#[cfg(test)]
mod tests {
    use bmz_core::clear::ClearType;

    use super::*;

    #[test]
    fn result_snapshot_detects_full_combo() {
        let snapshot = ResultSnapshot {
            player_name: String::new(),
            target_name: String::new(),
            target: Default::default(),
            current_fps: 0,
            skin_input: SkinLogicalInputSnapshot::default(),
            skin_attempt: SkinAttemptState::default(),
            skin_offsets: SkinOffsetValues::default(),
            mouse_position: None,
            hispeed_auto_adjust: false,
            assist_flags: [false; 7],
            assist_extra_note_depth: 0,
            assist_mine_mode: 0,
            assist_scroll_mode: 0,
            assist_long_note_mode: 0,
            clear_type: ClearType::Normal,
            result_failed: false,
            autoplay: false,
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            double_option: "OFF".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 20,
            ex_score_rate: 1.0,
            max_combo: 10,
            bp: 0,
            cb: 0,
            gauge_value: 100.0,
            gauge_type: 2,
            total_notes: 10,
            duration_ms: 0,
            note_display_duration_ms: None,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            main_bpm: 0.0,
            total_gauge: 0.0,
            judge_rank: None,
            key_mode: KeyMode::default(),
            has_long_notes: false,
            ln_mode_index: 0,
            rule_mode_index: 0,
            ln_score_policy_index: Some(0),
            result_gauge_graph_type: 2,
            result_panel: 0,
            favorite_chart: false,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_save_enabled: true,
            score_history_id: 1,
            replay_saved: true,
            replay_slots: [true, false, false, false],
            saved_replay_slots: [true, false, false, false],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            target_bp: None,
            previous_best_ex_score: None,
            previous_best_clear_type: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_clear_type: None,
            elapsed_time: TimeUs(0),
            fadeout_elapsed: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            table_text_primary: String::new(),
            table_text_secondary: String::new(),
            table_text_fallback: String::new(),
            stagefile_background: false,
            stagefile_image_size: None,
            course_titles: Default::default(),
            course_result: CourseResultSkinSnapshot::default(),
            graph: Arc::new(crate::snapshot::ResultGraphSnapshot::default()),
            overlay: OverlaySnapshot::default(),
            ir: ResultIrSnapshot::default(),
            player_stats: PlayerStatsSnapshot::default(),
        };

        assert!(snapshot.is_full_combo());
    }

    #[test]
    fn zero_note_result_is_not_full_combo() {
        let snapshot = ResultSnapshot {
            player_name: String::new(),
            target_name: String::new(),
            target: Default::default(),
            current_fps: 0,
            skin_input: SkinLogicalInputSnapshot::default(),
            skin_attempt: SkinAttemptState::default(),
            skin_offsets: SkinOffsetValues::default(),
            mouse_position: None,
            hispeed_auto_adjust: false,
            assist_flags: [false; 7],
            assist_extra_note_depth: 0,
            assist_mine_mode: 0,
            assist_scroll_mode: 0,
            assist_long_note_mode: 0,
            clear_type: ClearType::Normal,
            result_failed: false,
            autoplay: false,
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            double_option: "OFF".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 0,
            ex_score_rate: 1.0,
            max_combo: 0,
            bp: 0,
            cb: 0,
            gauge_value: 100.0,
            gauge_type: 2,
            total_notes: 0,
            duration_ms: 0,
            note_display_duration_ms: None,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            main_bpm: 0.0,
            total_gauge: 0.0,
            judge_rank: None,
            key_mode: KeyMode::default(),
            has_long_notes: false,
            ln_mode_index: 0,
            rule_mode_index: 0,
            ln_score_policy_index: Some(0),
            result_gauge_graph_type: 2,
            result_panel: 0,
            favorite_chart: false,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_save_enabled: true,
            score_history_id: 1,
            replay_saved: true,
            replay_slots: [true, false, false, false],
            saved_replay_slots: [true, false, false, false],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            target_bp: None,
            previous_best_ex_score: None,
            previous_best_clear_type: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_clear_type: None,
            elapsed_time: TimeUs(0),
            fadeout_elapsed: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            table_text_primary: String::new(),
            table_text_secondary: String::new(),
            table_text_fallback: String::new(),
            stagefile_background: false,
            stagefile_image_size: None,
            course_titles: Default::default(),
            course_result: CourseResultSkinSnapshot::default(),
            graph: Arc::new(crate::snapshot::ResultGraphSnapshot::default()),
            overlay: OverlaySnapshot::default(),
            ir: ResultIrSnapshot::default(),
            player_stats: PlayerStatsSnapshot::default(),
        };

        assert!(!snapshot.is_full_combo());
    }
}
