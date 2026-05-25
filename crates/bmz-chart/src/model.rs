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
    pub bar_lines: Vec<BarLine>,
    pub sounds: Vec<SoundAssetRef>,
    pub bga_assets: Vec<BgaAssetRef>,
    pub total_notes: u32,
    pub end_time: TimeUs,
}

#[derive(Debug, Clone, Default)]
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
    pub has_bga: bool,
    pub key_mode: KeyMode,
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
