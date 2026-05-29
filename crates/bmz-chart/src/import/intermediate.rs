use std::path::PathBuf;

use bmz_core::chart::ChartIdentity;
use bmz_core::lane::{KeyMode, Lane};
use bmz_core::time::{ChartTick, TimeUs};

use crate::model::LongNoteStyle;

#[derive(Debug, Clone)]
pub struct IntermediateChart {
    pub identity: ChartIdentity,
    pub metadata: IntermediateMetadata,
    pub resources: IntermediateResources,
    pub measures: Vec<MeasureInfo>,
    pub objects: Vec<IntermediateObject>,
    pub lnobj_wav_key: Option<u16>,
}

#[derive(Debug, Clone, Default)]
pub struct IntermediateMetadata {
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub play_level: String,
    pub difficulty_name: String,
    pub judge_rank: Option<i32>,
    pub initial_bpm: f64,
    pub total: Option<f64>,
    pub stage_file: String,
    pub banner_file: String,
    pub backbmp_file: String,
    pub preview_file: String,
    pub volwav_percent: u8,
    pub has_bga: bool,
    pub key_mode: KeyMode,
}

#[derive(Debug, Clone, Default)]
pub struct IntermediateResources {
    pub wavs: Vec<WavDef>,
    pub bmps: Vec<BmpDef>,
    pub bpm_table: Vec<BpmDef>,
    pub stop_table: Vec<StopDef>,
}

#[derive(Debug, Clone)]
pub struct WavDef {
    pub key: u16,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BmpDef {
    pub key: u16,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct BpmDef {
    pub key: u16,
    pub bpm: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct StopDef {
    pub key: u16,
    pub value: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct MeasureInfo {
    pub index: u32,
    pub length_ratio_num: u32,
    pub length_ratio_den: u32,
    pub start_tick: ChartTick,
    pub tick_len: u64,
}

#[derive(Debug, Clone)]
pub struct IntermediateObject {
    pub measure: u32,
    pub position_num: u32,
    pub position_den: u32,
    pub kind: IntermediateObjectKind,
}

#[derive(Debug, Clone)]
pub enum IntermediateObjectKind {
    VisibleNote {
        lane: Lane,
        wav_key: Option<u16>,
    },
    InvisibleNote {
        lane: Lane,
        wav_key: Option<u16>,
    },
    LongChannelNote {
        lane: Lane,
        wav_key: Option<u16>,
    },
    MineNote {
        lane: Lane,
        wav_key: Option<u16>,
        damage: u16,
    },
    Bgm {
        wav_key: u16,
    },
    Bga {
        bmp_key: u16,
        kind: IntermediateBgaKind,
    },
    SetBpm {
        bpm: f64,
    },
    SetExtendedBpm {
        bpm_key: u16,
    },
    Stop {
        stop_key: u16,
    },
    /// SCROLL チャネル: スクロール速度倍率の変化点。
    SetScroll {
        factor: f64,
    },
    /// SPEED チャネル: 間隔倍率の変化点 (beatoraja 拡張)。
    SetSpeed {
        factor: f64,
    },
    /// `#EXRANK` / chA0: 判定ランク変更。
    SetJudgeRank {
        rank_percent: i32,
    },
    /// チャネル #97: BGM 音量変更。
    SetBgmVolume {
        volume: u8,
    },
    /// チャネル #98: KEY 音量変更。
    SetKeyVolume {
        volume: u8,
    },
    /// チャネル #99: テキスト表示。
    SetText {
        text: String,
    },
    /// チャネル 0B–0E: BGA レイヤ不透明度。
    SetBgaOpacity {
        kind: IntermediateBgaKind,
        opacity: u8,
    },
    /// チャネル A1–A4: BGA レイヤ ARGB。
    SetBgaArgb {
        kind: IntermediateBgaKind,
        alpha: u8,
        red: u8,
        green: u8,
        blue: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntermediateBgaKind {
    Base,
    Poor,
    Layer,
}

#[derive(Debug, Clone)]
pub struct LaneObject {
    pub lane: Lane,
    pub tick: ChartTick,
    pub time: TimeUs,
    pub wav_key: Option<u16>,
    pub source: LaneObjectSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneObjectSource {
    Visible,
    Invisible,
    LongChannel,
    Mine { damage: u16 },
}

#[derive(Debug, Clone)]
pub enum ResolvedLaneEvent {
    Tap { lane: Lane, tick: ChartTick, time: TimeUs, wav_key: Option<u16> },
    Long { pair: LongNotePairDraft },
    Invisible { lane: Lane, tick: ChartTick, time: TimeUs, wav_key: Option<u16> },
    Mine { lane: Lane, tick: ChartTick, time: TimeUs, wav_key: Option<u16>, damage: u16 },
}

#[derive(Debug, Clone)]
pub struct LongNotePairDraft {
    pub lane: Lane,
    pub style: LongNoteStyle,
    pub start_tick: ChartTick,
    pub end_tick: ChartTick,
    pub start_time: TimeUs,
    pub end_time: TimeUs,
    pub wav_key: Option<u16>,
}
