use bmz_core::clear::ClearType;
use bmz_core::lane::KeyMode;
use bmz_core::time::TimeUs;

use crate::chart_graph::BpmGraphSegment;
use crate::skin::SkinImageSize;
use crate::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ResultGradeDiffDisplay {
    #[default]
    Nearest,
    Next,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum AppSceneSnapshot {
    Select(SelectSnapshot),
    Decide(RenderSnapshot),
    Play(RenderSnapshot),
    Result(ResultSnapshot),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayerStatsSnapshot {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectSnapshot {
    pub time: TimeUs,
    /// beatoraja STRING_PLAYER (2) に渡す現在プロフィール名。
    pub player_name: String,
    /// beatoraja NUMBER_CURRENT_FPS (20)。
    pub current_fps: u32,
    /// アプリ起動後の経過時間 ms。
    /// beatoraja の NUMBER_OPERATING_TIME_HOUR/MINUTE/SECOND (27..29) に使う。
    pub operating_time_ms: i32,
    pub selection_time: TimeUs,
    pub option_panel_time: TimeUs,
    /// TIMER_PANEL1_OFF..6_OFF (31..36) の経過時間。None は対応タイマーOFF。
    pub option_panel_off_times: [Option<TimeUs>; 6],
    pub option_panel: u8,
    pub chart_count: u32,
    pub selected_index: u32,
    /// beatoraja-style song bar movement direction. `1` means the new bars start
    /// from the next slot, `-1` from the previous slot, `0` disables movement.
    pub bar_scroll_direction: i32,
    /// Remaining song bar movement progress (1.0 at movement start, 0.0 at rest).
    pub bar_scroll_progress: f32,
    pub selected_chart_id: Option<i64>,
    pub selected_title: String,
    /// Current profile hispeed shown to select skins (NUMBER_HISPEED=310/311).
    pub hispeed: f32,
    /// Effective note display duration for the selected chart. `None` for folders/settings.
    pub note_display_duration_ms: Option<i32>,
    pub rows: Vec<SelectRowSnapshot>,
    pub arrange: String,
    pub arrange_2p: String,
    pub target: String,
    pub gauge: String,
    pub gauge_auto_shift: String,
    pub bottom_shiftable_gauge: String,
    pub double_option: String,
    pub hs_fix: String,
    pub assist: String,
    pub select_mode: String,
    pub select_sort: String,
    pub select_ln_mode: String,
    pub judge_algorithm: String,
    pub bga: String,
    pub grade_diff_display: ResultGradeDiffDisplay,
    /// Select detail option panelで表示する判定表示オフセット(ms)。
    pub judge_timing_offset_ms: i32,
    pub judge_timing_auto_adjust: bool,
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
    /// ロード済み `#STAGEFILE` の画像サイズ。
    pub stage_image_size: Option<SkinImageSize>,
    /// `#BACKBMP` テクスチャがロード済みなら true。
    pub backbmp_image: bool,
    /// ロード済み `#BACKBMP` の画像サイズ。
    pub backbmp_image_size: Option<SkinImageSize>,
    /// `#BANNER` テクスチャがロード済みなら true。
    pub banner_image: bool,
    /// ロード済み `#BANNER` の画像サイズ。
    pub banner_image_size: Option<SkinImageSize>,
    /// 設定フォルダ内にいるとき true。
    pub in_settings: bool,
    /// 設定項目の編集モード中。
    pub settings_editing: bool,
    /// 楽曲検索バー (beatoraja `STRING_SEARCHWORD`, ref=30) に表示する文字列。
    /// 検索モード中は入力中クエリ、非モード中は空 or 直前のメッセージ
    /// ("no song found" 等)。
    pub search_word: String,
    /// `search_word` に乗せる不透明度倍率 (0.0..=1.0)。placeholder /
    /// メッセージ表示時は薄く (< 1.0)、実入力中は 1.0。
    pub search_word_alpha: f32,
    /// `search_word` 内に重ねる検索 caret の UTF-8 byte index。
    pub search_caret_byte_index: Option<usize>,
    /// Select skin mouse position in normalized screen coordinates.
    pub mouse_position: Option<(f32, f32)>,
    /// 選曲カーソル譜面の IR ランキング状態 (NUMBER_IR_* / OPTION_IR_*)。
    pub ir: ResultIrSnapshot,
    /// 選曲カーソル譜面の IR ライバルベスト
    /// (STRING_RIVAL=1 / NUMBER_RIVAL_*=271,275,276 / OPTION_COMPARE_RIVAL=624,625)。
    pub rival: Option<SelectRivalSnapshot>,
    /// beatoraja IndexType autosave_replay1..4 (321..324) image row indices.
    pub replay_slot_rule_indices: [i64; 4],
    pub player_stats: PlayerStatsSnapshot,
}

/// 選曲カーソル譜面に対する IR ライバル (最上位 1 名) のベストスコア。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectRivalSnapshot {
    pub display_name: String,
    pub ex_score: u32,
    pub max_combo: u32,
    pub bp: u32,
}

impl Default for SelectSnapshot {
    fn default() -> Self {
        Self {
            time: TimeUs::default(),
            player_name: String::new(),
            current_fps: 0,
            operating_time_ms: 0,
            selection_time: TimeUs::default(),
            option_panel_time: TimeUs::default(),
            option_panel_off_times: [None; 6],
            option_panel: 0,
            chart_count: 0,
            selected_index: 0,
            bar_scroll_direction: 0,
            bar_scroll_progress: 0.0,
            selected_chart_id: None,
            selected_title: String::new(),
            hispeed: 0.0,
            note_display_duration_ms: None,
            rows: Vec::new(),
            arrange: String::new(),
            arrange_2p: String::new(),
            target: String::new(),
            gauge: String::new(),
            gauge_auto_shift: String::new(),
            bottom_shiftable_gauge: String::new(),
            double_option: String::new(),
            hs_fix: String::new(),
            assist: String::new(),
            select_mode: String::new(),
            select_sort: String::new(),
            select_ln_mode: String::new(),
            judge_algorithm: String::new(),
            bga: String::new(),
            grade_diff_display: ResultGradeDiffDisplay::default(),
            judge_timing_offset_ms: 0,
            judge_timing_auto_adjust: false,
            master_volume: 0.0,
            key_volume: 0.0,
            bgm_volume: 0.0,
            current_folder: String::new(),
            key_hint: String::new(),
            option_hint: String::new(),
            exit_hold_progress: 0.0,
            overlay: OverlaySnapshot::default(),
            stage_background: false,
            stage_image_size: None,
            backbmp_image: false,
            backbmp_image_size: None,
            banner_image: false,
            banner_image_size: None,
            in_settings: false,
            settings_editing: false,
            search_word: String::new(),
            search_word_alpha: 1.0,
            search_caret_byte_index: None,
            mouse_position: None,
            ir: ResultIrSnapshot::default(),
            rival: None,
            replay_slot_rule_indices: [0; 4],
            player_stats: PlayerStatsSnapshot::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectRowSnapshot {
    pub index: u32,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub play_level: String,
    pub table_level: String,
    pub table_text_primary: String,
    pub table_text_secondary: String,
    pub table_text_fallback: String,
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
    pub bp: Option<u32>,
    pub cb: Option<u32>,
    pub judge_counts: crate::snapshot::DisplayJudgeCounts,
    pub fast_slow_counts: Option<crate::snapshot::FastSlowJudgeCounts>,
    pub play_count: u32,
    pub clear_count: u32,
    pub replay_slots: [bool; 4],
    pub favorite_chart: bool,
    pub favorite_song: bool,
    pub has_long_notes: bool,
    pub has_mines: bool,
    pub has_random: bool,
    /// beatoraja SongInformation-derived chart details for selected song rows.
    pub chart_normal_notes: u32,
    pub chart_long_notes: u32,
    pub chart_scratch_notes: u32,
    pub chart_long_scratch_notes: u32,
    pub chart_mine_notes: u32,
    pub chart_density: f32,
    pub chart_peak_density: f32,
    pub chart_end_density: f32,
    pub chart_total_gauge: f32,
    pub chart_main_bpm: f32,
    pub chart_distribution: Vec<SelectChartDistributionSecond>,
    pub chart_bpm_graph_segments: Vec<BpmGraphSegment>,
    /// beatoraja DirectoryBar-style lamp distribution for folder rows.
    /// Indexes match SkinBar BARLAMP IDs: 0 no play, 1 failed, ... 10 max.
    pub folder_lamp_counts: [u32; 11],
    pub is_folder: bool,
    pub kind: SelectRowKind,
    /// library.db に登録済みかどうか。未登録の難易度表エントリは false。
    pub in_library: bool,
    /// コース行の場合のみ、これまでに達成したトロフィー名のリスト
    /// （`course_trophy_achievements` の DISTINCT、アルファ順）。
    /// それ以外の行 (Song / Folder / TableFolder) では常に空。
    ///
    /// `songlist.trophy` の描画判定で `SelectRowSnapshot` から直接参照する。
    /// `SkinDrawState` には載せない (Copy であるため Vec を抱えられない)。
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
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            table_level: String::new(),
            table_text_primary: String::new(),
            table_text_secondary: String::new(),
            table_text_fallback: String::new(),
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
            bp: None,
            cb: None,
            judge_counts: crate::snapshot::DisplayJudgeCounts::default(),
            fast_slow_counts: None,
            play_count: 0,
            clear_count: 0,
            replay_slots: [false; 4],
            favorite_chart: false,
            favorite_song: false,
            has_long_notes: false,
            has_mines: false,
            has_random: false,
            chart_normal_notes: 0,
            chart_long_notes: 0,
            chart_scratch_notes: 0,
            chart_long_scratch_notes: 0,
            chart_mine_notes: 0,
            chart_density: 0.0,
            chart_peak_density: 0.0,
            chart_end_density: 0.0,
            chart_total_gauge: 0.0,
            chart_main_bpm: 0.0,
            chart_distribution: Vec::new(),
            chart_bpm_graph_segments: Vec::new(),
            folder_lamp_counts: [0; 11],
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
    SearchFolder,
    Course,
    Executable,
    RandomCourse,
    Command,
    Container,
    NoSong,
    SettingsFolder,
    Config,
}

pub const IR_RANKING_ENTRY_SLOTS: usize = 10;
pub const IR_RANKING_NAME_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultIrRankingName {
    bytes: [u8; IR_RANKING_NAME_BYTES],
    len: u8,
}

impl Default for ResultIrRankingName {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl ResultIrRankingName {
    pub const EMPTY: Self = Self { bytes: [0; IR_RANKING_NAME_BYTES], len: 0 };

    pub fn from_display_name(name: &str) -> Self {
        let mut len = name.len().min(IR_RANKING_NAME_BYTES);
        while !name.is_char_boundary(len) {
            len -= 1;
        }
        let mut bytes = [0; IR_RANKING_NAME_BYTES];
        bytes[..len].copy_from_slice(&name.as_bytes()[..len]);
        Self { bytes, len: len as u8 }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultIrRankingEntrySnapshot {
    pub rank: Option<i64>,
    pub ex_score: Option<i64>,
    /// image/index property 390..399 で使う beatoraja clear type index。
    pub clear_index: Option<i64>,
    pub player_name: ResultIrRankingName,
}

impl ResultIrRankingEntrySnapshot {
    pub const EMPTY: Self = Self {
        rank: None,
        ex_score: None,
        clear_index: None,
        player_name: ResultIrRankingName::EMPTY,
    };
}

/// リザルト画面の IR ランキング表示状態。
///
/// beatoraja の `NUMBER_IR_*` / `OPTION_IR_*` skin property に対応する。
/// IR 未設定なら `Offline` (beatoraja の STATE_OFFLINE と同じく値は非表示)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultIrSnapshot {
    pub state: ResultIrState,
    /// STRING_IR_USER_NAME=1021。自分のランキング行判定にも使う。
    pub user_name: ResultIrRankingName,
    /// IR connect/send/access begin timer elapsed ms (TIMER_IR_CONNECT_BEGIN=172).
    pub connect_begin_ms: Option<i32>,
    /// IR connect/send/access success timer elapsed ms (TIMER_IR_CONNECT_SUCCESS=173).
    pub connect_success_ms: Option<i32>,
    /// IR connect/send/access fail timer elapsed ms (TIMER_IR_CONNECT_FAIL=174).
    pub connect_fail_ms: Option<i32>,
    /// 全体ランキングでの自分の順位 (NUMBER_IR_RANK=179)。
    pub rank: Option<i64>,
    /// ランキング対象の総プレイヤー数 (NUMBER_IR_TOTALPLAYER=180/200)。
    pub total_player: Option<i64>,
    /// 全プレイヤー中のクリア率 % (NUMBER_IR_CLEARRATE=181)。
    pub clear_rate: Option<i64>,
    /// 更新前の順位 (NUMBER_IR_PREVRANK=182)。未対応なら None。
    pub previous_rank: Option<i64>,
    /// 上位ランキング行 (STRING_RANKINGNAME1..10 / NUMBER_RANKING*_EXSCORE/INDEX)。
    pub entries: [ResultIrRankingEntrySnapshot; IR_RANKING_ENTRY_SLOTS],
}

impl ResultIrSnapshot {
    pub const EMPTY: Self = Self {
        state: ResultIrState::Offline,
        user_name: ResultIrRankingName::EMPTY,
        connect_begin_ms: None,
        connect_success_ms: None,
        connect_fail_ms: None,
        rank: None,
        total_player: None,
        clear_rate: None,
        previous_rank: None,
        entries: [ResultIrRankingEntrySnapshot::EMPTY; IR_RANKING_ENTRY_SLOTS],
    };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResultIrState {
    /// IR 未設定 / 未接続。
    #[default]
    Offline,
    /// 送信・ランキング取得中 (OPTION_IR_LOADING=601)。
    Loading,
    /// ランキング取得済み (OPTION_IR_LOADED=602)。
    Loaded,
    /// 取得失敗 (OPTION_IR_FAILED=604)。
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultSnapshot {
    /// beatoraja STRING_PLAYER (2) に渡す現在プロフィール名。
    pub player_name: String,
    /// beatoraja NUMBER_CURRENT_FPS (20)。
    pub current_fps: u32,
    pub clear_type: ClearType,
    /// OPTION_RESULT_CLEAR/FAILED (90/91) に渡す実際の成否。
    ///
    /// コース曲間リザルトでは clear lamp 表示用の `clear_type` を NoPlay に丸める一方、
    /// 背景や CLEAR/FAILED 演出は実プレイ結果に合わせるため分けて持つ。
    pub result_failed: bool,
    pub arrange: String,
    pub arrange_2p: String,
    pub lane_shuffle_pattern: Vec<u8>,
    pub ex_score: u32,
    pub ex_score_rate: f32,
    pub max_combo: u32,
    pub bp: u32,
    pub cb: u32,
    pub gauge_value: f32,
    pub gauge_type: i32,
    pub total_notes: u32,
    pub grade_diff_display: ResultGradeDiffDisplay,
    pub duration_ms: i32,
    /// NUMBER_DURATION/NUMBER_DURATION_GREEN に渡す緑数字 ms。
    pub note_display_duration_ms: Option<i32>,
    pub initial_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub main_bpm: f32,
    pub total_gauge: f32,
    pub judge_rank: Option<i32>,
    pub key_mode: KeyMode,
    /// 実効譜面にLNが含まれるか (OPTION_NO_LN/LN=172/173)。
    pub has_long_notes: bool,
    /// 実効LN種別のimageset index (0=LN, 1=CN, 2=HCN)。
    pub ln_mode_index: usize,
    pub result_gauge_graph_type: i32,
    /// Lua Result スキンの展開パネル (0=非表示、1=IR、2=グラフ)。
    pub result_panel: i32,
    /// 現在の譜面が favorite chart か。BMZ は invisible を持たないため2状態。
    pub favorite_chart: bool,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    /// 今回のリザルトがスコア保存対象か。
    pub score_save_enabled: bool,
    pub score_history_id: i64,
    pub replay_saved: bool,
    pub replay_slots: [bool; 4],
    pub saved_replay_slots: [bool; 4],
    pub best_ex_score: Option<u32>,
    pub best_clear_type: Option<ClearType>,
    pub target_ex_score: Option<u32>,
    pub best_max_combo: Option<u32>,
    pub target_max_combo: Option<u32>,
    pub best_bp: Option<u32>,
    pub target_bp: Option<u32>,
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_clear_type: Option<ClearType>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_bp: Option<u32>,
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
    pub table_text_primary: String,
    pub table_text_secondary: String,
    pub table_text_fallback: String,
    /// beatoraja STRING_COURSE1_TITLE..10_TITLE (150..159) for course results.
    pub course_titles: [String; 10],
    /// Result 画面の graph 系 skin object に渡すプレイ中の推移データ。
    pub graph: crate::snapshot::ResultGraphSnapshot,
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// IR ランキング表示状態 (NUMBER_IR_* / OPTION_IR_*)。
    pub ir: ResultIrSnapshot,
    pub player_stats: PlayerStatsSnapshot,
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
            player_name: String::new(),
            current_fps: 0,
            clear_type: ClearType::Normal,
            result_failed: false,
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 20,
            ex_score_rate: 1.0,
            max_combo: 10,
            bp: 0,
            cb: 0,
            gauge_value: 100.0,
            gauge_type: 2,
            total_notes: 10,
            grade_diff_display: ResultGradeDiffDisplay::default(),
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
            course_titles: Default::default(),
            graph: crate::snapshot::ResultGraphSnapshot::default(),
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
            current_fps: 0,
            clear_type: ClearType::Normal,
            result_failed: false,
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 0,
            ex_score_rate: 1.0,
            max_combo: 0,
            bp: 0,
            cb: 0,
            gauge_value: 100.0,
            gauge_type: 2,
            total_notes: 0,
            grade_diff_display: ResultGradeDiffDisplay::default(),
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
            course_titles: Default::default(),
            graph: crate::snapshot::ResultGraphSnapshot::default(),
            overlay: OverlaySnapshot::default(),
            ir: ResultIrSnapshot::default(),
            player_stats: PlayerStatsSnapshot::default(),
        };

        assert!(!snapshot.is_full_combo());
    }
}
