#![allow(dead_code)]

use std::path::PathBuf;

use kira::sound::static_sound::StaticSoundData;

pub mod bms_loader;
pub mod db;
pub mod hasher;
pub mod player;
pub mod judge;

#[allow(non_camel_case_types)]
#[derive(Debug, Default, PartialEq, Eq)]
pub enum Judge {
    LN_PUSHING_PGREAT_EARLY = -20,
    LN_PUSHING_GREAT_EARLY = -19,
    LN_PUSHING_GOOD_EARLY = -18,
    LN_PUSHING_PGREAT_LATE = -10,
    LN_PUSHING_GREAT_LATE = -9,
    LN_PUSHING_GOOD_LATE = -8,
    #[default]
    JUDGE_YET = -1,
    PGREAT_EARLY = 0,
    GREAT_EARLY = 1,
    GOOD_EARLY = 2,
    BAD_EARLY = 3,
    POOR_EARLY = 4,
    KPOOR_EARLY = 5,
    PGREAT_LATE = 6,
    GREAT_LATE = 7,
    GOOD_LATE = 8,
    BAD_LATE = 9,
    POOR_LATE = 10,
    KPOOR_LATE = 11,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Default)]
pub enum Rank {
    #[default]
    VHARD = 0,
    HARD = 1,
    NORMAL = 2,
    EASY = 3,
    VEASY = 4,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Default, PartialEq, Eq, Ord, PartialOrd)]
pub enum Mode {
    #[default]
    BEAT_5K = 0,
    BEAT_7K = 1,
    BEAT_10K = 2,
    BEAT_14K = 3,
    POPN_5K = 4,
    POPN_9K = 5,
}

impl Mode {
    pub fn is_bms(&self) -> bool {
        *self <= Mode::BEAT_14K
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Default)]
pub enum LnType {
    #[default]
    LN = 0,
    CN = 1,
    HCN = 2,
}

pub const FEATURE_UNDEFINEDLN: i32 = 1;
pub const FEATURE_MINENOTE: i32 = 2;
pub const FEATURE_RANDOM: i32 = 4;
pub const FEATURE_LONGNOTE: i32 = 8;
pub const FEATURE_CHARGENOTE: i32 = 16;
pub const FEATURE_HELLCHARGENOTE: i32 = 32;
pub const FEATURE_STOPSEQUENCE: i32 = 64;
pub const FEATURE_SCROLL: i32 = 128;

pub type BpmEventType = i32;
pub const BPM_CHANGE: BpmEventType = 0;
pub const STOP: BpmEventType = 1;

pub const MAX_LANE: usize = 20;

#[derive(Debug, Default)]
pub struct Note {
    pub lane: i32,
    pub lane_origin: i32,
    pub pulse: u64,
    pub len: u64,
    pub num: u64,
    pub judge: Judge,
    pub empty_poor_count: i32,
    pub ms: u64,
    pub lnend_ms: u64,
    pub index: usize,
}

#[derive(Debug, Default)]
pub struct BarLine {
    pulse: u64,
}

impl BarLine {
    pub fn new(pulse: u64) -> BarLine {
        return BarLine { pulse };
    }
}

#[derive(Debug, Default)]
pub struct BpmEvent {
    pub event_type: BpmEventType,
    pub pulse: u64,
    pub bpm: f64,
    pub duration: u64,
    pub ms: u64,
}

impl BpmEvent {
    pub fn next_event_ms(&self, pulse: u64, resolution: u64) -> u64 {
        return ((self.ms + (pulse - self.pulse) * 60 * 1000) as f64
            / (self.bpm * resolution as f64)) as u64;
    }
}

#[derive(Debug, Default)]
pub struct BGAHeader {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct BGAEvent {
    pub pulse: u64,
    pub id: i32,
}

#[derive(Debug, Default)]
pub struct BGA {
    bga_header: Vec<BGAHeader>,
    bga_events: Vec<BGAEvent>,
    layer_events: Vec<BGAEvent>,
    poor_events: Vec<BGAEvent>,
}

#[derive(Debug)]
pub struct ChartInfo {
    pub path: PathBuf,
    pub folder: PathBuf,
    pub parent: PathBuf,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub mode: Mode,
    pub ln_type: LnType,
    pub difficulty: i32,
    pub level: i32,
    pub rank: Rank,
    pub total: f32,
    pub back_image: String,
    pub eyecatch_image: String,
    pub banner_image: String,
    pub preview_music: String,
    pub resolution: u64,
    pub end_pulse: u64,
    pub end_ms: u64,
    pub init_bpm: f64,
    pub max_bpm: f64,
    pub min_bpm: f64,
    pub base_bpm: f64,
    pub note_count: i32,
    pub feature: i32,
    pub md5: String,
    pub sha256: String,
}

impl Default for ChartInfo {
    fn default() -> ChartInfo {
        return ChartInfo {
            resolution: 240,
            init_bpm: 130.0,
            base_bpm: 130.0,
            max_bpm: 130.0,
            min_bpm: 130.0,

            path: Default::default(),
            folder: Default::default(),
            parent: Default::default(),
            title: Default::default(),
            subtitle: Default::default(),
            artist: Default::default(),
            subartist: Default::default(),
            genre: Default::default(),
            mode: Default::default(),
            ln_type: Default::default(),
            difficulty: Default::default(),
            level: Default::default(),
            rank: Default::default(),
            total: Default::default(),
            back_image: Default::default(),
            eyecatch_image: Default::default(),
            banner_image: Default::default(),
            preview_music: Default::default(),
            end_pulse: Default::default(),
            end_ms: Default::default(),
            note_count: Default::default(),
            feature: Default::default(),
            md5: Default::default(),
            sha256: Default::default(),
        };
    }
}

impl ChartInfo {
    pub fn get_feature(&self, feature: i32) -> bool {
        (self.feature & feature) != 0
    }

    pub fn add_feature(&mut self, feature: i32) {
        self.feature |= feature;
    }
}

#[derive(Debug, Default)]
pub struct Chart {
    pub version: String,
    pub info: ChartInfo,
    pub lines: Vec<BarLine>,
    pub bpm_events: Vec<BpmEvent>,
    pub bga: BGA,
    pub notes: Vec<Note>,
    pub sounds: Vec<Option<StaticSoundData>>,
}

impl Chart {
    pub fn ms_to_pulse(&self, mut ms: u64) -> u64 {
        if self.bpm_events.is_empty() {
            return 0;
        }
        let mut pulse = 0;
        for i in 1..self.bpm_events.len() {
            let duration = ((self.bpm_events[i].pulse - self.bpm_events[i - 1].pulse) as f64
                * 60_000.0
                / (self.info.resolution as f64 * self.bpm_events[i - 1].bpm))
                as u64;
            if duration < ms {
                ms -= duration;
                pulse +=
                    (self.bpm_events[i - 1].bpm * self.info.resolution as f64 * duration as f64
                        / 60_000.0) as u64;
            }
        }
        pulse += (self.bpm_events[self.bpm_events.len() - 1].bpm
            * (self.info.resolution * ms) as f64
            / 60_000.0) as u64;
        return pulse;
    }
}
