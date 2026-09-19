use std::collections::HashMap;
use std::path::Path;

use bmz_core::ids::{NoteId, SoundId};
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::{ChartTick, TimeUs};

use crate::bga_keybound::parse_swbga_pattern;
use crate::model::{
    BarLine, BgaArgbEvent, BgaAssetId, BgaAssetRef, BgaEvent, BgaEventKind, BgaKeyboundEvent,
    BgaOpacityEvent, ChartMetadata, ChartTextEvent, ChartVolumeEvent, JudgeRankEvent, LongNotePair,
    NoteEvent, NoteKind, PlayableChart, ScrollEvent, SoundAssetRef, SoundEvent, SpeedEvent,
    SwBgaDefinition, TimingEvent, TimingEventKind,
};
use crate::sound_asset::sound_asset_exists;
use crate::timing::{
    IMPORT_TICK_SCALE, TickTimingEvent, TickTimingEventKind, TimingMap,
    build_timing_map_with_tick_scale,
};

use super::error::{ImportError, ImportWarning};
use super::intermediate::{
    IntermediateBgaKind, IntermediateChart, IntermediateLayeredSound, IntermediateMetadata,
    IntermediateObject, IntermediateObjectKind, LaneObject, LaneObjectSource, MeasureInfo,
    ResolvedLaneEvent,
};
use super::long_note::normalize_lane_objects_with_markers;

#[derive(Debug, Clone)]
struct SoundTable {
    by_wav_key: HashMap<u16, SoundId>,
    assets: Vec<SoundAssetRef>,
}

#[derive(Debug, Clone)]
struct BgaTable {
    by_bmp_key: HashMap<u16, BgaAssetId>,
    assets: Vec<BgaAssetRef>,
}

#[derive(Debug, Clone)]
struct TickObject {
    tick: ChartTick,
    kind: TickObjectKind,
}

#[derive(Debug, Clone)]
enum TickObjectKind {
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
        mode: Option<crate::model::LongNoteMode>,
        explicit_end_sound: bool,
    },
    MineNote {
        lane: Lane,
        wav_key: Option<u16>,
        damage: f64,
    },
    Bgm {
        wav_key: u16,
    },
    Bga {
        bmp_key: u16,
        kind: BgaEventKind,
    },
}

#[derive(Debug, Clone)]
struct PlayableChartDraft {
    identity: bmz_core::chart::ChartIdentity,
    metadata: ChartMetadata,
    total_is_bmson_percent: bool,
    lane_notes: [Vec<NoteEvent>; LANE_COUNT],
    long_notes: Vec<LongNotePair>,
    bgm_events: Vec<SoundEvent>,
    bga_events: Vec<BgaEvent>,
    timing_events: Vec<TimingEvent>,
    scroll_events: Vec<ScrollEvent>,
    speed_events: Vec<SpeedEvent>,
    judge_rank_events: Vec<JudgeRankEvent>,
    bgm_volume_events: Vec<ChartVolumeEvent>,
    key_volume_events: Vec<ChartVolumeEvent>,
    text_events: Vec<ChartTextEvent>,
    bga_opacity_events: Vec<BgaOpacityEvent>,
    bga_argb_events: Vec<BgaArgbEvent>,
    swbga_definitions: Vec<SwBgaDefinition>,
    bga_keybound_events: Vec<BgaKeyboundEvent>,
    bga_asset_by_bmp_key: HashMap<u16, BgaAssetId>,
    bar_lines: Vec<BarLine>,
    sounds: Vec<SoundAssetRef>,
    bga_assets: Vec<BgaAssetRef>,
    total_notes: u32,
    end_time: TimeUs,
}

mod assets;
mod events;
mod finalize;
mod notes;
mod pipeline;

use assets::*;
use events::*;
use finalize::*;
use notes::*;
pub use pipeline::normalize_chart;

#[cfg(test)]
mod tests {
    use bmz_core::ids::SoundId;

    use crate::hash::compute_chart_identity;

    use super::*;

    fn draft() -> PlayableChartDraft {
        PlayableChartDraft::new(
            compute_chart_identity(b"end-time"),
            ChartMetadata::default(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn compute_end_time_ignores_distant_bgm_when_playable_notes_exist() {
        let mut draft = draft();
        draft.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(2_000_000),
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        });
        draft.bgm_events.push(SoundEvent {
            tick: ChartTick(0),
            time: TimeUs(60 * 60 * 1_000_000),
            sound: SoundId(1),
        });

        assert_eq!(compute_end_time(&draft), TimeUs(2_000_000));
    }

    #[test]
    fn compute_end_time_uses_bgm_for_empty_charts() {
        let mut draft = draft();
        draft.bgm_events.push(SoundEvent {
            tick: ChartTick(0),
            time: TimeUs(3_000_000),
            sound: SoundId(1),
        });

        assert_eq!(compute_end_time(&draft), TimeUs(3_000_000));
    }
}
