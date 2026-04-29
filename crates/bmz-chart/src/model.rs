use std::path::PathBuf;

use bmz_core::chart::ChartIdentity;
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::{ChartTick, TimeUs};

#[derive(Debug, Clone)]
pub struct PlayableChart {
    pub identity: ChartIdentity,
    pub metadata: ChartMetadata,
    pub lane_notes: [Vec<NoteEvent>; LANE_COUNT],
    pub long_notes: Vec<LongNotePair>,
    pub bgm_events: Vec<SoundEvent>,
    pub timing_events: Vec<TimingEvent>,
    pub bar_lines: Vec<BarLine>,
    pub sounds: Vec<SoundAssetRef>,
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
    pub play_level: String,
    pub initial_bpm: f64,
    pub total: Option<f64>,
    pub stage_file: String,
    pub preview_file: String,
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

impl PlayableChart {
    pub fn notes_for_lane(&self, lane: Lane) -> &[NoteEvent] {
        &self.lane_notes[lane.index()]
    }
}
