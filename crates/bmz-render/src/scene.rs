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

#[derive(Debug, Clone, PartialEq)]
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
    pub gauge_auto_shift: String,
    pub assist: String,
    pub select_mode: String,
    pub select_sort: String,
    pub select_ln_mode: String,
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
    /// 設定フォルダ内にいるとき true。
    pub in_settings: bool,
    /// 設定項目の編集モード中。
    pub settings_editing: bool,
    /// 楽曲検索バー (beatoraja `STRING_SEARCHWORD`, ref=30) に表示する文字列。
    /// 検索モード中は入力中クエリ (+ カーソル記号)、非モード中は空 or 直前の
    /// メッセージ ("no song found" 等)。
    pub search_word: String,
    /// `search_word` に乗せる不透明度倍率 (0.0..=1.0)。placeholder /
    /// メッセージ表示時は薄く (< 1.0)、実入力中は 1.0。
    pub search_word_alpha: f32,
    /// Select skin mouse position in normalized screen coordinates.
    pub mouse_position: Option<(f32, f32)>,
}

impl Default for SelectSnapshot {
    fn default() -> Self {
        Self {
            time: TimeUs::default(),
            selection_time: TimeUs::default(),
            option_panel_time: TimeUs::default(),
            option_panel: 0,
            chart_count: 0,
            selected_index: 0,
            selected_chart_id: None,
            selected_title: String::new(),
            rows: Vec::new(),
            arrange: String::new(),
            target: String::new(),
            gauge: String::new(),
            gauge_auto_shift: String::new(),
            assist: String::new(),
            select_mode: String::new(),
            select_sort: String::new(),
            select_ln_mode: String::new(),
            bga: String::new(),
            master_volume: 0.0,
            key_volume: 0.0,
            bgm_volume: 0.0,
            current_folder: String::new(),
            key_hint: String::new(),
            option_hint: String::new(),
            exit_hold_progress: 0.0,
            overlay: OverlaySnapshot::default(),
            stage_background: false,
            banner_image: false,
            in_settings: false,
            settings_editing: false,
            search_word: String::new(),
            search_word_alpha: 1.0,
            mouse_position: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectRowSnapshot {
    pub index: u32,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub difficulty_name: String,
    pub play_level: String,
    pub table_level: String,
    /// 現在の曲の #RANK / 判定ランク。0..4 は VERYHARD..VERYEASY、10 以上は直接倍率。
    pub judge_rank: Option<i32>,
    pub total_notes: u32,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub length_ms: i64,
    pub clear_type: String,
    pub ex_score: Option<u32>,
    pub max_combo: Option<u32>,
    pub gauge_value: Option<f32>,
    pub miss_count: Option<u32>,
    pub play_count: u32,
    pub clear_count: u32,
    pub replay_slots: [bool; 4],
    pub has_long_notes: bool,
    pub has_mines: bool,
    pub has_random: bool,
    /// beatoraja SongInformation-derived chart details for selected song rows.
    pub chart_normal_notes: u32,
    pub chart_long_notes: u32,
    pub chart_scratch_notes: u32,
    pub chart_long_scratch_notes: u32,
    pub chart_density: f32,
    pub chart_peak_density: f32,
    pub chart_end_density: f32,
    pub chart_total_gauge: f32,
    pub chart_main_bpm: f32,
    pub chart_distribution: Vec<SelectChartDistributionSecond>,
    pub is_folder: bool,
    pub kind: SelectRowKind,
    /// library.db に登録済みかどうか。未登録の難易度表エントリは false。
    pub in_library: bool,
    /// コース行の場合のみ、これまでに達成したトロフィー名のリスト
    /// （`course_trophy_achievements` の DISTINCT、アルファ順）。
    /// それ以外の行 (Song / Folder / TableFolder) では常に空。
    ///
    /// 現状このフィールドを直接参照するスキン要素は無く、`SelectRowSnapshot`
    /// までの流路だけが整っている。`SkinDrawState` には載せない (Copy で
    /// あるため Vec を抱えられない) — 対応する skin op を実装するときは
    /// `select_skin_items` のループから row を直接参照して描画判定する。
    pub achieved_trophy_names: Vec<String>,
    /// beatoraja STRING_COURSE1_TITLE..10_TITLE (150..159) for course rows.
    /// Empty for non-course rows.
    pub course_titles: [String; 10],
    /// beatoraja OPTION_GRADEBAR_* (1002..1017) for course rows.
    pub course_constraints: CourseConstraintFlags,
    /// 曲行のみ。beatoraja OPTION_MODE_* (160..164, 1160..1161) 用。
    pub chart_key_mode: Option<bmz_core::lane::KeyMode>,
}

impl Default for SelectRowSnapshot {
    fn default() -> Self {
        Self {
            index: 0,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            table_level: String::new(),
            judge_rank: None,
            total_notes: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            length_ms: 0,
            clear_type: String::new(),
            ex_score: None,
            max_combo: None,
            gauge_value: None,
            miss_count: None,
            play_count: 0,
            clear_count: 0,
            replay_slots: [false; 4],
            has_long_notes: false,
            has_mines: false,
            has_random: false,
            chart_normal_notes: 0,
            chart_long_notes: 0,
            chart_scratch_notes: 0,
            chart_long_scratch_notes: 0,
            chart_density: 0.0,
            chart_peak_density: 0.0,
            chart_end_density: 0.0,
            chart_total_gauge: 0.0,
            chart_main_bpm: 0.0,
            chart_distribution: Vec::new(),
            is_folder: false,
            kind: SelectRowKind::default(),
            in_library: true,
            achieved_trophy_names: Vec::new(),
            course_titles: Default::default(),
            course_constraints: CourseConstraintFlags::default(),
            chart_key_mode: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectChartDistributionSecond {
    pub scratch_long_heads: u16,
    pub scratch_long_bodies: u16,
    pub scratch_taps: u16,
    pub key_long_heads: u16,
    pub key_long_bodies: u16,
    pub key_taps: u16,
    pub mines: u16,
}

impl SelectChartDistributionSecond {
    pub fn total(self) -> u32 {
        u32::from(self.scratch_long_heads)
            + u32::from(self.scratch_long_bodies)
            + u32::from(self.scratch_taps)
            + u32::from(self.key_long_heads)
            + u32::from(self.key_long_bodies)
            + u32::from(self.key_taps)
            + u32::from(self.mines)
    }

    pub fn values(self) -> [u16; 7] {
        [
            self.scratch_long_heads,
            self.scratch_long_bodies,
            self.scratch_taps,
            self.key_long_heads,
            self.key_long_bodies,
            self.key_taps,
            self.mines,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CourseConstraintFlags {
    pub class: bool,
    pub mirror: bool,
    pub random: bool,
    pub no_speed: bool,
    pub no_good: bool,
    pub no_great: bool,
    pub gauge_lr2: bool,
    pub gauge_5k: bool,
    pub gauge_7k: bool,
    pub gauge_9k: bool,
    pub gauge_24k: bool,
    pub ln: bool,
    pub cn: bool,
    pub hcn: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectRowKind {
    #[default]
    Song,
    Folder,
    TableFolder,
    Course,
    SettingsFolder,
    Config,
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
    pub replay_slots: [bool; 4],
    pub saved_replay_slots: [bool; 4],
    pub best_ex_score: Option<u32>,
    pub best_clear_type: Option<ClearType>,
    pub target_ex_score: Option<u32>,
    pub best_max_combo: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub best_misscount: Option<u32>,
    pub target_misscount: Option<u32>,
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_misscount: Option<u32>,
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
    /// Result 画面の graph 系 skin object に渡すプレイ中の推移データ。
    pub graph: crate::snapshot::ResultGraphSnapshot,
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
            replay_slots: [true, false, false, false],
            saved_replay_slots: [true, false, false, false],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_misscount: None,
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
            graph: crate::snapshot::ResultGraphSnapshot::default(),
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
            replay_slots: [true, false, false, false],
            saved_replay_slots: [true, false, false, false],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_misscount: None,
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
            graph: crate::snapshot::ResultGraphSnapshot::default(),
            overlay: OverlaySnapshot::default(),
        };

        assert!(!snapshot.is_full_combo());
    }
}
