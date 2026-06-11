use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use bmz_core::chart::ChartIdentity;
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::{ChartTick, TimeUs};

#[derive(Debug, Clone)]
pub struct PlayableChart {
    pub identity: ChartIdentity,
    pub metadata: ChartMetadata,
    pub lane_notes: [Vec<NoteEvent>; LANE_COUNT],
    pub long_notes: Vec<LongNotePair>,
    pub bgm_events: Vec<SoundEvent>,
    pub bga_events: Vec<BgaEvent>,
    pub timing_events: Vec<TimingEvent>,
    pub scroll_events: Vec<ScrollEvent>,
    pub speed_events: Vec<SpeedEvent>,
    pub judge_rank_events: Vec<JudgeRankEvent>,
    pub bgm_volume_events: Vec<ChartVolumeEvent>,
    pub key_volume_events: Vec<ChartVolumeEvent>,
    pub text_events: Vec<ChartTextEvent>,
    pub bga_opacity_events: Vec<BgaOpacityEvent>,
    pub bga_argb_events: Vec<BgaArgbEvent>,
    /// `#SWBGA` 定義。
    pub swbga_definitions: Vec<SwBgaDefinition>,
    /// チャネル A5 による keybound BGA 切替。
    pub bga_keybound_events: Vec<BgaKeyboundEvent>,
    /// BMP キー → BGA アセット ID。
    pub bga_asset_by_bmp_key: HashMap<u16, BgaAssetId>,
    pub bar_lines: Vec<BarLine>,
    pub sounds: Vec<SoundAssetRef>,
    pub bga_assets: Vec<BgaAssetRef>,
    pub total_notes: u32,
    pub end_time: TimeUs,
}

#[derive(Debug, Clone)]
pub struct ChartMetadata {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub difficulty_name: String,
    pub judge_rank: Option<i32>,
    pub play_level: String,
    pub initial_bpm: f64,
    pub total: Option<f64>,
    pub stage_file: String,
    pub banner_file: String,
    pub backbmp_file: String,
    pub preview_file: String,
    /// `#VOLWAV` ヘッダ (百分率、100 = 原音)。
    pub volwav_percent: u8,
    pub has_bga: bool,
    /// Source chart defines BMS `#RANDOM` sections. Distinct from player arrange options.
    pub has_bms_random: bool,
    /// `#URL` / `%URL` distribution URL.
    pub source_url: String,
    /// `#URL-WAV` and similar append URLs.
    pub append_url: String,
    /// Raw BMS header commands keyed by uppercased command name.
    pub bms_headers: BTreeMap<String, String>,
    pub key_mode: KeyMode,
    /// `#LNMODE` / BMSON `ln_type`。未指定時は LR2 互換の LN (終点判定なし)。
    pub long_note_mode: LongNoteMode,
    /// True when the source chart explicitly declared its long-note mode.
    /// False means long notes should be treated as undefined until player
    /// policy resolves them.
    pub long_note_mode_defined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LongNoteMode {
    /// 始点のみ判定。終点まで押し続ければ離さなくてよい。
    #[default]
    Ln,
    /// 始点と終点の両方で判定 (IIDX CN 相当)。
    Cn,
    /// CN + 押下中のゲージ増加 / 早離しペナルティ (IIDX HCN 相当)。
    Hcn,
}

impl Default for ChartMetadata {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            judge_rank: None,
            play_level: String::new(),
            initial_bpm: 0.0,
            total: None,
            stage_file: String::new(),
            banner_file: String::new(),
            backbmp_file: String::new(),
            preview_file: String::new(),
            volwav_percent: 100,
            has_bga: false,
            has_bms_random: false,
            source_url: String::new(),
            append_url: String::new(),
            bms_headers: BTreeMap::new(),
            key_mode: KeyMode::default(),
            long_note_mode: LongNoteMode::default(),
            long_note_mode_defined: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Tap,
    LongStart,
    LongEnd,
    Invisible,
    Mine,
}

#[derive(Debug, Clone)]
pub struct NoteEvent {
    pub id: NoteId,
    pub lane: Lane,
    pub kind: NoteKind,
    pub tick: ChartTick,
    pub time: TimeUs,
    pub sound: Option<SoundId>,
    /// Mine 専用のダメージ値（チャネル D系列に置かれた base36 値そのもの）。
    /// Mine 以外は常に None。
    pub damage: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LongNoteStyle {
    ChannelPair,
    LnObj,
}

#[derive(Debug, Clone)]
pub struct LongNotePair {
    pub lane: Lane,
    pub style: LongNoteStyle,
    pub mode: Option<LongNoteMode>,
    pub start_note_id: NoteId,
    pub end_note_id: NoteId,
    pub start_tick: ChartTick,
    pub end_tick: ChartTick,
    pub start_time: TimeUs,
    pub end_time: TimeUs,
    pub sound: Option<SoundId>,
}

#[derive(Debug, Clone)]
pub struct SoundEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub sound: SoundId,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimingEventKind {
    BpmChange { bpm: f64 },
    Stop { duration_us: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimingEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub kind: TimingEventKind,
}

/// SCROLL チャネルで指定されたスクロール速度倍率の変化点。
/// 判定時刻には影響せず、譜面の見た目だけを変える（factor>1.0 で速く流れる、
/// factor<0 で逆スクロール等）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub factor: f64,
}

/// SPEED チャネルで指定された間隔倍率の変化点。SCROLL とは別系統で、
/// beatoraja 拡張の `#SPEEDxx` 系をサポートする譜面で使われる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub factor: f64,
}

/// `#EXRANK` / chA0 による判定ランク変更イベント。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeRankEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    /// beatoraja 準拠の判定窓倍率 (%) 。25=VERYHARD, 100=EASY 等。
    pub rank_percent: i32,
}

/// BMS チャネル #97 (BGM) / #98 (KEY) による音量変更イベント。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartVolumeEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    /// 0x01..=0xFF (255 = 原音)。
    pub value: u8,
}

/// BMS `#TEXT` / チャネル #99 によるテキスト表示イベント。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChartTextEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub text: String,
}

/// BMS チャネル 0B–0E による BGA レイヤ不透明度変更。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BgaOpacityEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub layer: BgaEventKind,
    /// 0x01..=0xFF (255 = 不透明)。
    pub opacity: u8,
}

/// BMS チャネル A1–A4 / `#ARGB` による BGA レイヤ色変更。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BgaArgbEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub layer: BgaEventKind,
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Debug, Clone)]
pub struct SwBgaDefinition {
    pub id: u16,
    pub frame_rate_ms: u32,
    pub total_time_ms: u32,
    /// 対象キー通道 (11–18 / 21–28)。
    pub line: u8,
    pub loop_mode: bool,
    pub chroma_alpha: u8,
    pub chroma_red: u8,
    pub chroma_green: u8,
    pub chroma_blue: u8,
    pub pattern_bmp_keys: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BgaKeyboundEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub swbga_id: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarLine {
    pub measure: u32,
    pub tick: ChartTick,
    pub time: TimeUs,
}

#[derive(Debug, Clone)]
pub struct SoundAssetRef {
    pub id: SoundId,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BgaAssetId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgaEventKind {
    Base,
    Poor,
    Layer,
    /// BMS チャネル 0A (Overlay2)。
    Layer2,
}

#[derive(Debug, Clone)]
pub struct BgaEvent {
    pub tick: ChartTick,
    pub time: TimeUs,
    pub asset: BgaAssetId,
    pub kind: BgaEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgaAssetKind {
    Static,
    Video,
}

#[derive(Debug, Clone)]
pub struct BgaAssetRef {
    pub id: BgaAssetId,
    pub path: PathBuf,
    pub kind: BgaAssetKind,
}

impl PlayableChart {
    pub fn notes_for_lane(&self, lane: Lane) -> &[NoteEvent] {
        &self.lane_notes[lane.index()]
    }

    pub fn note_by_id(&self, id: NoteId) -> Option<&NoteEvent> {
        self.lane_notes.iter().flatten().find(|note| note.id == id)
    }
}
