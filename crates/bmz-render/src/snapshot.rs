use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::skin_offset::SkinOffsetValues;

/// beatoraja の OPTION_COURSE_STAGE1..4 / OPTION_COURSE_STAGE_FINAL (280..283 / 289) に対応。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CourseStageMarker {
    Stage1,
    Stage2,
    Stage3,
    Stage4,
    Final,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlaySnapshot {
    /// 右下に常時表示する文字列。
    pub text: String,
    /// 右上に常時表示する FPS 文字列。
    pub fps_text: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderSnapshot {
    pub time: TimeUs,
    /// プレイ画面に遷移してからの経過時間。
    /// timer 未指定 destination の通常アニメーション時刻の基準に使う。
    pub play_elapsed_time: TimeUs,
    /// READY timer (TIMER_READY=40) elapsed time. None while READY is not active yet.
    pub ready_elapsed_time: Option<TimeUs>,
    pub duration: TimeUs,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub judge_rank: Option<i32>,
    pub play_level: String,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub gauge: f32,
    pub gauge_type: i32,
    /// 現在ゲージの上限 (beatoraja `GaugeElementProperty.max`)。
    pub gauge_max: f32,
    /// 現在ゲージの合格ライン (beatoraja `GaugeElementProperty.border`)。
    pub gauge_border: f32,
    pub hispeed: f32,
    pub lift: f32,
    pub lane_cover: f32,
    pub lane_cover_changing: bool,
    /// beatoraja `OPTION_LANECOVER1_ON` (271)。
    pub lanecover_enabled: bool,
    /// beatoraja `OPTION_LIFT1_ON` (272)。
    pub lift_enabled: bool,
    /// beatoraja `OPTION_HIDDEN1_ON` (273)。
    pub hidden_enabled: bool,
    pub note_display_duration_ms: i32,
    pub hidden_cover: f32,
    pub skin_offsets: SkinOffsetValues,
    pub now_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub has_bga: bool,
    pub bga_enabled: bool,
    pub bga_base: Option<DisplayBgaFrame>,
    pub bga_layer: Option<DisplayBgaFrame>,
    pub bga_poor: Option<DisplayBgaFrame>,
    pub bga_stretch: i32,
    pub best_ex_score: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub judge_timing_offset_ms: i32,
    /// OPTION_AUTOPLAYON (33) / OPTION_AUTOPLAYOFF (32) 用。
    pub autoplay: bool,
    /// OPTION_MODE_COURSE (290) とステージ別 op (280..283 / 289) 用。未対応時は None。
    pub course_stage: Option<CourseStageMarker>,
    pub key_mode: KeyMode,
    pub visible_notes: [Vec<VisibleNote>; LANE_COUNT],
    /// Mine ノーツ。スコア対象外で、専用のスプライト（赤系）で描く。
    pub visible_mines: [Vec<VisibleMine>; LANE_COUNT],
    pub visible_long_notes: Vec<VisibleLongNote>,
    pub recent_inputs: Vec<DisplayInput>,
    pub recent_judgements: Vec<DisplayJudgement>,
    /// Full combo timer elapsed ms (skin timer 48/49). None while inactive.
    pub full_combo_elapsed_ms: Option<i32>,
    /// Scene fadeout timer elapsed ms (skin timer 2). None while inactive.
    pub fadeout_elapsed_ms: Option<i32>,
    /// Failed/close timer elapsed ms (skin timer 3). None while inactive.
    pub failed_elapsed_ms: Option<i32>,
    /// Music end timer elapsed ms (skin timer 908). None while inactive.
    pub music_end_elapsed_ms: Option<i32>,
    pub bar_lines: Vec<VisibleBarLine>,
    /// 各レーンのキー押下開始からの経過 ms(押下中のみ Some)。skin timer 100..=107 に渡る。
    pub keyon_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのキー解放からの経過 ms(離した直後のみ Some)。skin timer 120..=127 に渡る。
    pub keyoff_ms: [Option<i32>; LANE_COUNT],
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// `#BACKBMP` テクスチャがロード済みなら true (BGA より下に描画)。
    pub backbmp_background: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayJudgeCounts {
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub empty_poor: u32,
}

/// リザルト画面の Fast/Slow 内訳。
/// beatoraja の result.json で `ref` 410-424（fast/slow split）を埋めるために使う。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FastSlowJudgeCounts {
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

impl FastSlowJudgeCounts {
    pub fn fast_total(self) -> u32 {
        self.fast_pgreat + self.fast_great + self.fast_good + self.fast_bad + self.fast_poor
    }

    pub fn slow_total(self) -> u32 {
        self.slow_pgreat + self.slow_great + self.slow_good + self.slow_bad + self.slow_poor
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisibleNote {
    pub lane: Lane,
    pub time: TimeUs,
    pub y: f32,
}

/// 画面上に見えている Mine ノーツ。座標系は [`VisibleNote`] と同じ。
#[derive(Debug, Clone, PartialEq)]
pub struct VisibleMine {
    pub lane: Lane,
    pub time: TimeUs,
    pub y: f32,
    pub damage: u16,
}

/// 画面上に見えているロングノートの胴体。
/// `head_y` は判定ライン側（手前）、`tail_y` は終端側（奥）。
/// どちらも `VisibleNote::y` と同じ正規化座標（0.0=判定ライン, 1.0=最奥）で、
/// 可視範囲 [0.0, 1.0] にクランプ済み。`head_y <= tail_y` が保証される。
#[derive(Debug, Clone, PartialEq)]
pub struct VisibleLongNote {
    pub lane: Lane,
    pub head_y: f32,
    pub tail_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayJudgement {
    pub lane: Lane,
    pub text: String,
    pub delta_us: i64,
    pub time: TimeUs,
    /// ノートを押さずに通過した見逃し判定（Poor）。
    /// このとき「打鍵」は発生していないのでキービームやボム演出は不要。
    pub is_miss: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInput {
    pub lane: Lane,
    pub time: TimeUs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisibleBarLine {
    pub time: TimeUs,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayBgaFrame {
    pub texture_id: u32,
    pub width: f32,
    pub height: f32,
}
