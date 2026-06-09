use bmz_chart::model::LongNoteMode;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

pub use crate::chart_graph::BpmGraphSegment;
use crate::skin_offset::SkinOffsetValues;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResultGaugeGraphPoint {
    pub time_ms: i32,
    pub value: f32,
    pub border: f32,
    pub gauge_type: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultTimingPoint {
    pub time_ms: i32,
    /// beatoraja `Note.getPlayTime()` 相当。正が FAST/EARLY、負が SLOW/LATE。
    pub delta_us: i64,
    pub judge: Judge,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultJudgeGraphBucket {
    /// beatoraja `SkinNoteDistributionGraph.TYPE_JUDGE` の state 0..5。
    /// 0=unjudged, 1=PG, 2=GR, 3=GD, 4=BD, 5=PR/MS。
    pub values: [u32; 6],
}

impl ResultJudgeGraphBucket {
    pub fn total(self) -> u32 {
        self.values.iter().copied().sum()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultEarlyLateGraphBucket {
    /// beatoraja `SkinNoteDistributionGraph.TYPE_EARLYLATE` の state 0..9。
    /// 0=unjudged, 1=PG, 2..5=FAST/EARLY GR..PR, 6..9=SLOW/LATE GR..PR。
    pub values: [u32; 10],
}

impl ResultEarlyLateGraphBucket {
    pub fn total(self) -> u32 {
        self.values.iter().copied().sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultTimingDistribution {
    pub range_ms: i32,
    pub counts: Vec<u32>,
}

impl Default for ResultTimingDistribution {
    fn default() -> Self {
        Self::new(150)
    }
}

impl ResultTimingDistribution {
    pub fn new(range_ms: i32) -> Self {
        let range_ms = range_ms.max(1);
        Self { range_ms, counts: vec![0; (range_ms * 2 + 1) as usize] }
    }

    pub fn add(&mut self, timing_ms: i32) {
        if (-self.range_ms..=self.range_ms).contains(&timing_ms) {
            let index = (timing_ms + self.range_ms) as usize;
            if let Some(count) = self.counts.get_mut(index) {
                *count = count.saturating_add(1);
            }
        }
    }

    pub fn total(&self) -> u32 {
        self.counts.iter().copied().sum()
    }

    pub fn stats(&self) -> Option<(f32, f32)> {
        let count = self.total();
        if count == 0 {
            return None;
        }
        let count_f = count as f32;
        let average_ms = self
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| (index as i32 - self.range_ms) as f32 * *count as f32)
            .sum::<f32>()
            / count_f;
        let variance = self
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                let diff = (index as i32 - self.range_ms) as f32 - average_ms;
                diff * diff * *count as f32
            })
            .sum::<f32>()
            / count_f;
        Some((average_ms, variance.sqrt()))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultGraphSnapshot {
    pub gauge_points: Vec<ResultGaugeGraphPoint>,
    pub timing_points: Vec<ResultTimingPoint>,
    pub timing_distribution: ResultTimingDistribution,
    pub judge_graph_buckets: Vec<ResultJudgeGraphBucket>,
    pub early_late_graph_buckets: Vec<ResultEarlyLateGraphBucket>,
    pub judge_graph_density: Vec<u8>,
    pub bpm_graph_segments: Vec<BpmGraphSegment>,
    pub hit_error_ring: HitErrorRingSnapshot,
}

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
    /// 左上に常時表示する文字列。
    pub left_text: String,
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
    /// Play option arrange label for skin ref 42/43.
    pub arrange: String,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub gauge: f32,
    pub gauge_type: i32,
    /// Gauge Auto Shift が有効なプレイかどうか。
    pub gauge_auto_shift: bool,
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
    pub has_bpm_stop: bool,
    pub bga_enabled: bool,
    pub bga_base: Option<DisplayBgaFrame>,
    pub bga_layer: Option<DisplayBgaFrame>,
    pub bga_layer2: Option<DisplayBgaFrame>,
    pub bga_poor: Option<DisplayBgaFrame>,
    pub bga_stretch: i32,
    pub best_ex_score: Option<u32>,
    pub projected_best_ex_score: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub judge_timing_offset_ms: i32,
    /// beatoraja `NUMBER_MAINBPM` (92) 用の代表 BPM。
    pub main_bpm: f32,
    /// beatoraja `event_index(BUTTON_HSFIX=55)`。
    pub hsfix_index: i32,
    /// Rm-skin F/S threshold 表示 (ms)。
    pub fs_threshold_ms: i32,
    /// HSFIX 連動の adjusted hidden cover (0..1)。
    pub adjusted_cover_progress: Option<f32>,
    /// HSFIX 連動の BPM 比率 (0..1)。
    pub adjusted_rate: Option<f32>,
    /// HSFIX 連動の BPM 比率 ×100 整数部。
    pub adjusted_rate_adot: Option<i32>,
    /// プレイ用 judgegraph (1 秒単位ノーツ密度)。
    pub judge_graph_density: Vec<u8>,
    /// プレイ用 bpmgraph 線分。
    pub bpm_graph_segments: Vec<BpmGraphSegment>,
    /// OPTION_AUTOPLAYON (33) / OPTION_AUTOPLAYOFF (32) 用。
    pub autoplay: bool,
    /// OPTION_MODE_COURSE (290) とステージ別 op (280..283 / 289) 用。未対応時は None。
    pub course_stage: Option<CourseStageMarker>,
    /// beatoraja STRING_COURSE1_TITLE..10_TITLE (150..159) 用。
    pub course_titles: [String; 10],
    /// beatoraja `TEXT_TABLE1` (1001): 難易度表名 (例: `[★] Insane`)。
    pub table_text_primary: String,
    /// beatoraja `TEXT_TABLE2` (1002): 表内レベル (例: `★12`)。
    pub table_text_secondary: String,
    /// beatoraja `TEXT_TABLE3` (1003): 表内レベル + 表名。
    pub table_text_fallback: String,
    pub key_mode: KeyMode,
    pub visible_notes: [Vec<VisibleNote>; LANE_COUNT],
    /// Mine ノーツ。スコア対象外で、専用のスプライト（赤系）で描く。
    pub visible_mines: [Vec<VisibleMine>; LANE_COUNT],
    pub visible_long_notes: Vec<VisibleLongNote>,
    pub recent_inputs: Vec<DisplayInput>,
    pub recent_judgements: Vec<DisplayJudgement>,
    /// HitErrorVisualizer 用の直近判定タイミング (ms)。
    pub hit_error_ring: HitErrorRingSnapshot,
    /// Full combo timer elapsed ms (skin timer 48/49). None while inactive.
    pub full_combo_elapsed_ms: Option<i32>,
    /// End-of-note timer elapsed ms (skin timer 143/144). None while inactive.
    pub end_of_note_elapsed_ms: Option<i32>,
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
    /// 各レーンの LN ホールド開始からの経過 ms(ホールド中のみ Some)。
    /// beatoraja の TIMER_HOLD (skin timer 70..=77 / 80..=87) に渡る。
    pub hold_ms: [Option<i32>; LANE_COUNT],
    /// LN モードでも終端 (tail) キャップを描画するか。既定 OFF (beatoraja 準拠)。
    pub show_ln_tail_cap: bool,
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// `#BACKBMP` テクスチャがロード済みなら true (BGA より下に描画)。
    pub backbmp_background: bool,
    /// BMS `#TEXT` / チャネル #99 で表示する譜面テキスト。
    pub chart_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitErrorRingSnapshot {
    pub values: [i64; bmz_gameplay::hit_error::HIT_ERROR_RING_LEN],
    pub index: usize,
}

impl Default for HitErrorRingSnapshot {
    fn default() -> Self {
        Self {
            values: [bmz_gameplay::hit_error::HIT_ERROR_EMPTY;
                bmz_gameplay::hit_error::HIT_ERROR_RING_LEN],
            index: 0,
        }
    }
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
        self.fast_great + self.fast_good + self.fast_bad + self.fast_poor
    }

    pub fn slow_total(self) -> u32 {
        self.slow_great + self.slow_good + self.slow_bad + self.slow_poor
    }
}

/// [`VisibleNote`] の種別。描画に使う画像を切り替えるために使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteVisualKind {
    Tap,
    LnStart,
    LnEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisibleNote {
    pub lane: Lane,
    pub time: TimeUs,
    pub y: f32,
    pub kind: NoteVisualKind,
    /// beatoraja の `Note.state` 相当。判定済みでも本来の時刻までは描画に残す。
    pub processed_judge: Option<Judge>,
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
    pub mode: LongNoteMode,
    pub head_y: f32,
    pub tail_y: f32,
    /// LN HEAD を判定済みでキーを押し続けている状態。胴体の画像を切り替えるために使う。
    pub is_pressing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayJudgement {
    pub lane: Lane,
    pub judge: Judge,
    /// `None` = FAST/SLOW 表示なし（閾値以内の JUST 判定）。
    pub side: Option<TimingSide>,
    pub text: String,
    pub combo: u32,
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
    pub tint_r: f32,
    pub tint_g: f32,
    pub tint_b: f32,
    pub tint_a: f32,
    /// 動画 BGA フレームかどうか。beatoraja は動画 Layer に対して
    /// `layer.frag` (黒クロマキー) ではなく `ffmpeg.frag` を使うため、
    /// Layer/Layer2 でも動画ならクロマキーを適用しない。
    pub is_video: bool,
}

impl DisplayBgaFrame {
    pub fn opaque(texture_id: u32, width: f32, height: f32) -> Self {
        Self {
            texture_id,
            width,
            height,
            tint_r: 1.0,
            tint_g: 1.0,
            tint_b: 1.0,
            tint_a: 1.0,
            is_video: false,
        }
    }

    pub fn opaque_video(texture_id: u32, width: f32, height: f32) -> Self {
        Self { is_video: true, ..Self::opaque(texture_id, width, height) }
    }
}
