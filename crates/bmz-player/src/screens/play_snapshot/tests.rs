use std::collections::HashMap;
use std::sync::Arc;

use bmz_chart::hash::compute_chart_identity;
use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
use bmz_core::ids::NoteId;
use bmz_core::input::{InputDeviceKind, InputEvent, InputKind, InputSource};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::judge::model::JudgementEvent;
use bmz_render::skin::{
    SkinDocument, SkinDocumentRenderExt, SkinDocumentTexture, SkinDrawState, SkinImageSize,
    SkinRenderItem, SkinTextureId,
};

use crate::config::profile_config::ProfileConfig;
use crate::screens::play_session::{
    BattleOpponentOptions, PlaySessionOptions, SRandomScheme, build_game_session,
};
use crate::select_options::{ArrangeOption, DoubleOption, SessionMode};

use super::*;

fn approx_eq(left: f32, right: f32) -> bool {
    (left - right).abs() < 0.0001
}

fn chart_with_bpm_changes() -> PlayableChart {
    use bmz_chart::model::{TimingEvent, TimingEventKind};
    PlayableChart {
        identity: compute_chart_identity(b"bpm-test"),
        metadata: ChartMetadata { initial_bpm: 120.0, ..Default::default() },
        lane_notes: std::array::from_fn(|_| Vec::new()),
        long_notes: Vec::new(),
        bgm_events: Vec::new(),
        bga_events: Vec::new(),
        timing_events: vec![
            TimingEvent {
                tick: ChartTick(0),
                time: TimeUs(500_000),
                kind: TimingEventKind::BpmChange { bpm: 180.0 },
            },
            TimingEvent {
                tick: ChartTick(0),
                time: TimeUs(1_000_000),
                kind: TimingEventKind::BpmChange { bpm: 90.0 },
            },
        ],
        scroll_events: Vec::new(),
        speed_events: Vec::new(),
        judge_rank_events: Vec::new(),
        bgm_volume_events: Vec::new(),
        key_volume_events: Vec::new(),
        text_events: Vec::new(),
        bga_opacity_events: Vec::new(),
        bga_argb_events: Vec::new(),
        swbga_definitions: Vec::new(),
        bga_keybound_events: Vec::new(),
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 0,
        end_time: TimeUs(2_000_000),
    }
}

pub(super) fn chart() -> PlayableChart {
    let note = tap_note(1, Lane::Key1, 0, 1_000_000);
    let mut lane_notes = std::array::from_fn(|_| Vec::new());
    lane_notes[Lane::Key1.index()].push(note);

    PlayableChart {
        identity: compute_chart_identity(b"snapshot"),
        metadata: ChartMetadata {
            title: "snapshot".to_string(),
            initial_bpm: 120.0,
            total: Some(160.0),
            ..Default::default()
        },
        lane_notes,
        long_notes: Vec::new(),
        bgm_events: Vec::new(),
        bga_events: Vec::new(),
        timing_events: Vec::new(),

        scroll_events: Vec::new(),

        speed_events: Vec::new(),
        judge_rank_events: Vec::new(),
        bgm_volume_events: Vec::new(),
        key_volume_events: Vec::new(),
        text_events: Vec::new(),
        bga_opacity_events: Vec::new(),
        bga_argb_events: Vec::new(),
        swbga_definitions: Vec::new(),
        bga_keybound_events: Vec::new(),
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 1,
        end_time: TimeUs(1_000_000),
    }
}

fn tap_note(id: u32, lane: Lane, tick: u64, time_us: i64) -> NoteEvent {
    NoteEvent {
        id: NoteId(id),
        lane,
        kind: NoteKind::Tap,
        tick: ChartTick(tick),
        time: TimeUs(time_us),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    }
}

/// Key1 に start=500ms, end=1500ms のロングノートを1本持つ譜面。
fn chart_with_long_note() -> PlayableChart {
    use bmz_chart::model::{LongNotePair, LongNoteStyle};

    let start = NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::LongStart,
        tick: ChartTick(0),
        time: TimeUs(500_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let end = NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        kind: NoteKind::LongEnd,
        tick: ChartTick(0),
        time: TimeUs(1_500_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let mut lane_notes = std::array::from_fn(|_| Vec::new());
    lane_notes[Lane::Key1.index()].push(start);
    lane_notes[Lane::Key1.index()].push(end);

    PlayableChart {
        identity: compute_chart_identity(b"long-note"),
        metadata: ChartMetadata { initial_bpm: 120.0, ..Default::default() },
        lane_notes,
        long_notes: vec![LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(0),
            start_time: TimeUs(500_000),
            end_time: TimeUs(1_500_000),
            sound: None,
        }],
        bgm_events: Vec::new(),
        bga_events: Vec::new(),
        timing_events: Vec::new(),

        scroll_events: Vec::new(),

        speed_events: Vec::new(),
        judge_rank_events: Vec::new(),
        bgm_volume_events: Vec::new(),
        key_volume_events: Vec::new(),
        text_events: Vec::new(),
        bga_opacity_events: Vec::new(),
        bga_argb_events: Vec::new(),
        swbga_definitions: Vec::new(),
        bga_keybound_events: Vec::new(),
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 1,
        end_time: TimeUs(1_500_000),
    }
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
#[path = "tests/cases_05.rs"]
mod cases_05;
#[path = "tests/input_offset.rs"]
mod input_offset;
