use std::sync::Arc;

use bmz_chart::model::LongNoteMode;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::session::SkinRuntimeEvent;

pub use crate::chart_graph::BpmGraphSegment;
use crate::skin_offset::SkinOffsetValues;

/// Play skin が fadeout 時間も timer=2 destination も持たない場合の黒フェード時間。
pub const DEFAULT_PLAY_FADEOUT_DURATION_MS: i32 = 500;

pub use bmz_skin_document::{
    ResultEarlyLateGraphBucket, ResultGaugeGraphPoint, ResultJudgeGraphBucket,
    ResultNoteGraphBucket, ResultTimingDistribution, ResultTimingPoint,
};

pub const SKIN_SOURCE_LN_UNDEFINED_BIT: u8 = 1 << 0;
pub const SKIN_SOURCE_LN_DEFINED_LN_BIT: u8 = 1 << 1;
pub const SKIN_SOURCE_LN_DEFINED_CN_BIT: u8 = 1 << 2;
pub const SKIN_SOURCE_LN_DEFINED_HCN_BIT: u8 = 1 << 3;
pub const SKIN_SOURCE_LN_DEFINED_HLN_BIT: u8 = 1 << 4;

/// Selectでは開始予定、Decide/Play/Resultでは試行開始時に固定されたskin公開状態。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinAttemptState {
    /// BATTLE / キーモード変換前の譜面キーモード。
    pub source_key_mode: Option<KeyMode>,
    /// skinが描画する変換後のキーモード。Selectでは開始予定値。
    pub effective_key_mode: Option<KeyMode>,
    pub seven_to_six: bool,
    /// beatoraja ref 360互換。7K TO 9K無効時は0、有効時は1..=6。
    pub seven_to_nine_pattern: u8,
    /// beatoraja ref 361互換。0=FIXED, 1=NO MASHING, 2=ALTERNATION。
    pub seven_to_nine_type: u8,
    /// [`SKIN_SOURCE_LN_UNDEFINED_BIT`] などのbit mask。Noneは譜面未確定。
    pub source_ln_profile_bits: Option<u8>,
    /// 0=NORMAL, 1=PRACTICE, 2=AUTOPLAY, 3=AUTO BATTLE, 4=G-BATTLE。
    pub session_mode_index: Option<usize>,
    pub double_option_index: Option<usize>,
    pub hsfix_index: Option<usize>,
    pub gauge_auto_shift_index: Option<usize>,
    pub bottom_shiftable_gauge_index: Option<usize>,
    pub judge_algorithm_index: Option<usize>,
    /// 0=LN, 1=CN, 2=HCN。Noneは譜面未確定。
    pub ln_mode_index: Option<usize>,
    /// SongDataBooleanProperty互換。Noneは譜面未選択・未確定。
    pub has_bga: Option<bool>,
    pub has_random_sequence: Option<bool>,
}

impl SkinAttemptState {
    /// 譜面preloadで確定した値を反映し、まだ取得できないprofile由来値は保持する。
    pub fn merge_known(&mut self, newer: Self) {
        self.source_key_mode = newer.source_key_mode.or(self.source_key_mode);
        self.effective_key_mode = newer.effective_key_mode.or(self.effective_key_mode);
        self.seven_to_six = newer.seven_to_six;
        self.seven_to_nine_pattern = newer.seven_to_nine_pattern;
        self.seven_to_nine_type = newer.seven_to_nine_type;
        self.source_ln_profile_bits = newer.source_ln_profile_bits.or(self.source_ln_profile_bits);
        self.session_mode_index = newer.session_mode_index.or(self.session_mode_index);
        self.double_option_index = newer.double_option_index.or(self.double_option_index);
        self.hsfix_index = newer.hsfix_index.or(self.hsfix_index);
        self.gauge_auto_shift_index = newer.gauge_auto_shift_index.or(self.gauge_auto_shift_index);
        self.bottom_shiftable_gauge_index =
            newer.bottom_shiftable_gauge_index.or(self.bottom_shiftable_gauge_index);
        self.judge_algorithm_index = newer.judge_algorithm_index.or(self.judge_algorithm_index);
        self.ln_mode_index = newer.ln_mode_index.or(self.ln_mode_index);
        self.has_bga = newer.has_bga.or(self.has_bga);
        self.has_random_sequence = newer.has_random_sequence.or(self.has_random_sequence);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResultTimingMetrics {
    pub initialized: bool,
    pub stats: Option<(f32, f32)>,
    pub judged_notes: u32,
    pub absolute_duration_us: u128,
}

impl ResultTimingMetrics {
    pub fn average_duration_us(self, total_notes: u32) -> Option<i64> {
        if total_notes == 0 || self.judged_notes > total_notes {
            return None;
        }
        const UNJUDGED_DURATION_US: u128 = 1_000_000;
        let unjudged_notes = total_notes - self.judged_notes;
        let total_duration = self
            .absolute_duration_us
            .saturating_add(u128::from(unjudged_notes).saturating_mul(UNJUDGED_DURATION_US));
        Some((total_duration / u128::from(total_notes)).min(i64::MAX as u128) as i64)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultGraphSnapshot {
    pub gauge_points: Vec<ResultGaugeGraphPoint>,
    pub timing_points: Vec<ResultTimingPoint>,
    pub timing_distribution: ResultTimingDistribution,
    pub timing_metrics: ResultTimingMetrics,
    pub judge_graph_buckets: Vec<ResultJudgeGraphBucket>,
    pub note_graph_buckets: Vec<ResultNoteGraphBucket>,
    pub early_late_graph_buckets: Vec<ResultEarlyLateGraphBucket>,
    pub judge_graph_density: Vec<u8>,
    pub bpm_graph_segments: Vec<BpmGraphSegment>,
    pub hit_error_ring: HitErrorRingSnapshot,
}

impl ResultGraphSnapshot {
    pub fn refresh_timing_metrics(&mut self) {
        let stats =
            self.timing_distribution.stats().or_else(|| timing_point_stats(&self.timing_points));
        self.timing_metrics = ResultTimingMetrics {
            initialized: true,
            stats,
            judged_notes: self.timing_points.len().min(u32::MAX as usize) as u32,
            absolute_duration_us: self
                .timing_points
                .iter()
                .map(|point| u128::from(point.delta_us.unsigned_abs()))
                .sum(),
        };
    }
}

fn timing_point_stats(points: &[ResultTimingPoint]) -> Option<(f32, f32)> {
    if points.is_empty() {
        return None;
    }
    let count = points.len() as f32;
    let average_ms =
        points.iter().map(|point| point.delta_us as f32 / 1_000.0).sum::<f32>() / count;
    let variance = points
        .iter()
        .map(|point| {
            let diff = point.delta_us as f32 / 1_000.0 - average_ms;
            diff * diff
        })
        .sum::<f32>()
        / count;
    Some((average_ms, variance.sqrt()))
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

/// BMZ skin extension logical inputs in E1/E2/E3/E4/UI Left/Right/Up/Down order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinLogicalInputSnapshot {
    pub held: [bool; bmz_skin_document::SKIN_BMZ_INPUT_COUNT],
}

/// Battle session の表示専用2P状態。
///
/// 主プレイヤーの保存スコアとは分離し、beatoraja の rival ref と2P timerへ公開する。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpponentRenderSnapshot {
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub gauge: f32,
    pub gauge_type: i32,
    pub gauge_max: f32,
    pub gauge_border: f32,
    pub full_combo_elapsed_ms: Option<i32>,
    pub end_of_note_elapsed_ms: Option<i32>,
    pub gauge_increase_elapsed_ms: Option<i32>,
    pub gauge_max_elapsed_ms: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderSnapshot {
    /// 表示オフセットを含まない譜面時刻。判定演出・BGA・skin timer の基準に使う。
    /// ノート等のレーン描画位置は app 側で表示オフセットを別途適用して構築する。
    pub time: TimeUs,
    /// beatoraja STRING_PLAYER (2) に渡す現在プロフィール名。
    pub player_name: String,
    /// beatoraja NUMBER_CURRENT_FPS (20)。
    pub current_fps: u32,
    /// プレイ画面に遷移してからの経過時間。
    /// timer 未指定 destination の通常アニメーション時刻の基準に使う。
    pub play_elapsed_time: TimeUs,
    /// アプリ起動後の経過時間 ms。
    /// beatoraja の NUMBER_OPERATING_TIME_HOUR/MINUTE/SECOND (27..29) に使う。
    pub operating_time_ms: i32,
    pub skin_input: SkinLogicalInputSnapshot,
    pub skin_attempt: SkinAttemptState,
    /// READY timer (TIMER_READY=40) elapsed time. None while READY is not active yet.
    pub ready_elapsed_time: Option<TimeUs>,
    /// Play sceneの時計を継続し、ロード表示とREADY timerを再始動しない入場。
    /// 常駐viewerの後続Playなど、見た目だけをseek相当にする経路で使う。
    pub seamless_play_entry: bool,
    /// 直近の小節線からの60 BPM換算拍時間 (TIMER_RHYTHM=140)。
    pub rhythm_timer_elapsed_ms: Option<i32>,
    /// 直近の4分拍からの実時間。PMS ノート拡縮に使い、TIMER_RHYTHM とは分けて扱う。
    pub quarter_note_elapsed_ms: Option<i32>,
    /// BMS リソース (WAV 等) のバックグラウンドロードが完了しているか。
    /// READY 遷移可否の判定に使う。op 80/81 の表現状態とは独立して保持する。
    /// preload 完了前の placeholder snapshot では false。
    pub resources_loaded: bool,
    /// Audio/BGA resource load progress in the beatoraja RateType 102 range (0.0..=1.0).
    pub resource_load_progress: f32,
    pub duration: TimeUs,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub judge_rank: Option<i32>,
    pub play_level: String,
    /// 1P play option arrange label for skin ref 42/344.
    pub arrange: String,
    /// 2P play option arrange label for skin ref 43/345.
    pub arrange_2p: String,
    /// プレイ中に実際に適用されている固定レーン配置。
    /// `pattern[表示先レーン] = 元レーン`。動的ノート配置では空。
    pub lane_shuffle_pattern: Vec<u8>,
    /// Play target option id for skin string refs 1 / 3 / 200..=219.
    pub target: String,
    /// Selected rival or resolved target display name for skin string ref 1.
    /// None falls back to fixed target labels; unresolved IR targets stay empty.
    pub resolved_target_name: Option<String>,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    /// beatoraja NUMBER_SONGGAUGE_TOTAL / FLOAT_CHART_TOTALGAUGE (368)。
    pub chart_total_gauge: f32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub fast_slow_counts: FastSlowJudgeCounts,
    pub gauge: f32,
    pub gauge_type: i32,
    /// Per-gauge values captured from the running play state for result gaugegraph switching.
    pub gauge_graph_points: Vec<ResultGaugeGraphPoint>,
    /// Gauge Auto Shift が有効なプレイかどうか。
    pub gauge_auto_shift: bool,
    /// 現在ゲージの上限 (beatoraja `GaugeElementProperty.max`)。
    pub gauge_max: f32,
    /// 現在ゲージの合格ライン (beatoraja `GaugeElementProperty.border`)。
    pub gauge_border: f32,
    pub opponent: Option<OpponentRenderSnapshot>,
    pub hispeed: f32,
    /// BMZ extension: current hispeed mode for play skin refs. 0=base, 1=Floating.
    pub hispeed_mode_index: i32,
    /// BMZ extension: configured base hispeed. 0=Classic, 1=Normal.
    pub base_hispeed_index: i32,
    /// BMZ extension: selected Normal hispeed level (1..=20).
    pub normal_hispeed_level: u8,
    /// BMZ extension: five-choice hispeed configuration index.
    pub hispeed_config_index: i32,
    /// BMZ extension: target green number used by Floating.
    pub target_green_number: u32,
    pub lift: f32,
    pub lane_cover: f32,
    pub lane_cover_changing: bool,
    /// beatoraja `OPTION_LANECOVER1_ON` (271)。
    pub lanecover_enabled: bool,
    /// beatoraja `OPTION_LIFT1_ON` (272)。
    pub lift_enabled: bool,
    /// beatoraja `OPTION_HIDDEN1_ON` (273)。
    pub hidden_enabled: bool,
    /// beatoraja image/index ref 342。
    pub hispeed_auto_adjust: bool,
    pub note_display_duration_ms: i32,
    pub hidden_cover: f32,
    pub skin_offsets: SkinOffsetValues,
    pub now_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub has_bga: bool,
    /// beatoraja OPTION_NO_LN / OPTION_LN (172/173) 用。
    /// None は開始譜面をまだ特定できない placeholder。
    pub has_long_notes: Option<bool>,
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
    pub judge_timing_auto_adjust: bool,
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
    pub judge_graph_density: Arc<[u8]>,
    /// プレイ用 bpmgraph 線分。
    pub bpm_graph_segments: Arc<[BpmGraphSegment]>,
    /// OPTION_AUTOPLAYON (33) / OPTION_AUTOPLAYOFF (32) 用。
    pub autoplay: bool,
    /// リプレイ再生中かどうか。プレイ中 FAST/SLOW 表示など、入力由来の表示制御に使う。
    pub replay_playback: bool,
    /// BMZ extension: frozen scoring rule mode index for Decide/Play.
    pub rule_mode_index: usize,
    /// BMZ extension: normalized LN policy index stored in the score key.
    pub ln_score_policy_index: Option<usize>,
    /// プラクティス再生中かどうか。beatoraja OPTION_PRACTICE (1080) 用。
    pub practice_mode: bool,
    /// プラクティスの設定中プレビューか。秒線/BPMガイドはこの間だけ表示する。
    pub practice_preview: bool,
    /// beatoraja assist button 301..307 の選択状態。
    pub assist_flags: [bool; 7],
    pub assist_extra_note_depth: u8,
    pub assist_mine_mode: i64,
    pub assist_scroll_mode: i64,
    pub assist_long_note_mode: i64,
    /// beatoraja event/index 343。
    pub guide_se_enabled: bool,
    /// beatoraja event/index/option 400。
    pub constant_enabled: bool,
    pub constant_fade_ms: i32,
    /// 描画専用アシスト。判定・譜面変換とは独立して renderer が参照する。
    pub judge_area: bool,
    pub mark_processed_note: bool,
    pub bpm_guide: bool,
    /// JUDGE AREA のPGREAT/GREAT/GOOD/BAD/POOR外縁（判定ラインからの進捗）。
    pub judge_area_key_y: [f32; 5],
    pub judge_area_scratch_y: [f32; 5],
    /// このプレイがスコア保存対象か。beatoraja OPTION_SCORE_SAVE_ENABLED (61) 用。
    pub score_save_enabled: bool,
    /// OPTION_MODE_COURSE (290) とステージ別 op (280..283 / 289) 用。未対応時は None。
    pub course_stage: Option<CourseStageMarker>,
    /// beatoraja STRING_COURSE1_TITLE..10_TITLE (150..159) 用。
    pub course_titles: [String; 10],
    /// beatoraja `TEXT_TABLE1` (1001): 難易度表名 (例: `Insane`)。
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
    /// このフレームで発生した key logger 等の skin runtime 向けイベント。
    pub skin_events: Vec<SkinRuntimeEvent>,
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
    /// Gauge increase timer elapsed ms (skin timer 42/43). None while inactive.
    pub gauge_increase_elapsed_ms: Option<i32>,
    /// Gauge max timer elapsed ms (skin timer 44/45). None while inactive.
    pub gauge_max_elapsed_ms: Option<i32>,
    pub bar_lines: Vec<VisibleBarLine>,
    pub bpm_lines: Vec<VisibleBarLine>,
    pub stop_lines: Vec<VisibleBarLine>,
    pub time_lines: Vec<VisibleBarLine>,
    /// 各レーンのキー押下開始からの経過 ms(押下中のみ Some)。skin timer 100..=107 に渡る。
    pub keyon_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのキー解放からの経過 ms(離した直後のみ Some)。skin timer 120..=127 に渡る。
    pub keyoff_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの LN ホールド開始からの経過 ms(ホールド中のみ Some)。
    /// beatoraja の TIMER_HOLD (skin timer 70..=77 / 80..=87) に渡る。
    pub hold_ms: [Option<i32>; LANE_COUNT],
    /// LN モードでも終端 (tail) キャップを描画するか。既定 OFF (beatoraja 準拠)。
    pub show_ln_tail_cap: bool,
    /// 各レーンの HCN ACTIVE(回復中) タイマー経過 ms。
    /// beatoraja の TIMER_HCN_ACTIVE (skin timer 250..=257 / 260..=267) に渡る。
    pub hcn_active_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの HCN DAMAGE(減衰中) タイマー経過 ms。
    /// beatoraja の TIMER_HCN_DAMAGE (skin timer 270..=277 / 280..=287) に渡る。
    pub hcn_damage_ms: [Option<i32>; LANE_COUNT],
    /// 右下に常時表示するオーバーレイ文字列。
    pub overlay: OverlaySnapshot,
    /// `#STAGEFILE` テクスチャがロード済みなら true。
    pub stagefile_background: bool,
    /// ロード済み `#STAGEFILE` の画像サイズ。
    pub stagefile_image_size: Option<crate::skin::SkinImageSize>,
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
        self.fast_great + self.fast_good + self.fast_bad + self.fast_poor + self.fast_empty_poor
    }

    pub fn slow_total(self) -> u32 {
        self.slow_great + self.slow_good + self.slow_bad + self.slow_poor + self.slow_empty_poor
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
    pub alpha: f32,
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
    pub alpha: f32,
    pub damage: f64,
}

/// ロングノート胴体の表示状態。beatoraja `drawLongNote` の longImage 選択に対応。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongBodyState {
    /// 非アクティブ（接近中など）。longImage\[3\] (LN/CN) / \[7\] (HCN)。
    Inactive,
    /// HEAD 判定済みで処理中 (`processing == pair`)。longImage\[2\] / \[6\]。
    Processing,
    /// HCN passing 中で inclease（押下していて回復中）。longImage\[8\]。
    HcnActive,
    /// HCN passing 中で離している（減衰中）。longImage\[9\]。
    HcnDamage,
}

impl LongBodyState {
    /// LN/CN 用の 2 状態（押下中か否か）に縮退させる。
    pub fn is_processing(self) -> bool {
        self == Self::Processing
    }
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
    pub alpha: f32,
    /// 胴体の表示状態。胴体画像の切り替えに使う。
    pub body_state: LongBodyState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayJudgement {
    pub lane: Lane,
    pub judge: Judge,
    /// `None` = FAST/SLOW 表示なし（Auto または閾値以内の PGREAT）。
    pub side: Option<TimingSide>,
    pub text: String,
    pub combo: u32,
    pub delta_us: i64,
    pub time: TimeUs,
    /// ノートを押さずに通過した見逃し判定（Poor）。
    /// このとき「打鍵」は発生していないのでキービームやボム演出は不要。
    pub is_miss: bool,
    /// PGREAT の閾値 ms フィルタ（bmz 独自拡張）で ±ms 表示 (ref 525) も非表示にする。
    /// Auto (beatoraja 準拠) では常に false（beatoraja は 525 を常に供給する）。
    pub timing_ms_suppressed: bool,
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
    pub alpha: f32,
    pub label: String,
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
