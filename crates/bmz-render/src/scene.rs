use bmz_core::clear::ClearType;
use bmz_core::time::TimeUs;

use crate::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot};

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum AppSceneSnapshot {
    Select(SelectSnapshot),
    Decide(RenderSnapshot),
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
    pub target: String,
    pub gauge: String,
    pub assist: String,
    pub bga: String,
    pub master_volume: f32,
    pub key_volume: f32,
    pub bgm_volume: f32,
    pub current_folder: String,
    pub key_hint: String,
    pub option_hint: String,
    /// ESC 長押しによるアプリ終了の進捗 (0.0..=1.0)。0.0 のときは未押下。
    pub exit_hold_progress: f32,
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// `#STAGEFILE` テクスチャがロード済みなら true。
    pub stage_background: bool,
    /// `#BANNER` テクスチャがロード済みなら true。
    pub banner_image: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectRowSnapshot {
    pub index: u32,
    pub title: String,
    pub artist: String,
    pub difficulty_name: String,
    pub play_level: String,
    pub table_level: String,
    pub total_notes: u32,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub length_ms: i64,
    pub clear_type: String,
    pub ex_score: Option<u32>,
    pub max_combo: Option<u32>,
    pub gauge_value: Option<f32>,
    pub replay_slots: [bool; 4],
    pub is_folder: bool,
    pub kind: SelectRowKind,
    /// library.db に登録済みかどうか。未登録の難易度表エントリは false。
    pub in_library: bool,
}

impl Default for SelectRowSnapshot {
    fn default() -> Self {
        Self {
            index: 0,
            title: String::new(),
            artist: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            table_level: String::new(),
            total_notes: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            length_ms: 0,
            clear_type: String::new(),
            ex_score: None,
            max_combo: None,
            gauge_value: None,
            replay_slots: [false; 4],
            is_folder: false,
            kind: SelectRowKind::default(),
            in_library: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectRowKind {
    #[default]
    Song,
    Folder,
    TableFolder,
    Course,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSnapshot {
    pub clear_type: ClearType,
    pub ex_score: u32,
    pub ex_score_rate: f32,
    pub max_combo: u32,
    pub gauge_value: f32,
    pub gauge_type: i32,
    pub total_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub score_history_id: i64,
    pub replay_saved: bool,
    pub best_ex_score: Option<u32>,
    pub best_clear_type: Option<ClearType>,
    pub target_ex_score: Option<u32>,
    pub best_max_combo: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub best_misscount: Option<u32>,
    pub target_misscount: Option<u32>,
    pub target_clear_type: Option<ClearType>,
    /// リザルト画面を開いてからの経過時間。
    /// destination の timer/loop/keyframe アニメーション、image cycle に使われる。
    pub elapsed_time: TimeUs,
    /// リザルト画面終了フェードアウトの経過時間 (TIMER_FADEOUT=2)。
    /// None なら終了処理に入っていない。Some のあいだは `timer: 2` の
    /// destination が描画され、終了アニメーションが進行する。
    pub fadeout_elapsed: Option<TimeUs>,
    /// 曲名 (text ref 10/12 で表示)。
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub play_level: String,
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
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
            gauge_type: 2,
            total_notes: 10,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 1,
            replay_saved: true,
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
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
            overlay: OverlaySnapshot::default(),
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
            gauge_type: 2,
            total_notes: 0,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 1,
            replay_saved: true,
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
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
            overlay: OverlaySnapshot::default(),
        };

        assert!(!snapshot.is_full_combo());
    }
}
