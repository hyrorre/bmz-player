use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle, SoundAssetRef, SoundEvent};
use bmz_core::chart::ChartIdentity;
use bmz_core::input::InputSource;

use super::*;

fn windows() -> JudgeWindow {
    JudgeWindow::symmetric(16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000)
}

fn chart_with_tap(time: TimeUs) -> PlayableChart {
    chart_with_lane_tap(Lane::Key1, time)
}

fn chart_with_lane_tap(lane: Lane, time: TimeUs) -> PlayableChart {
    let note = NoteEvent {
        id: NoteId(1),
        lane,
        kind: NoteKind::Tap,
        tick: Default::default(),
        time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let mut lane_notes = std::array::from_fn(|_| Vec::new());
    lane_notes[lane.index()].push(note);

    PlayableChart {
        identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata: ChartMetadata::default(),
        lane_notes,
        long_notes: Vec::new(),
        bgm_events: Vec::<SoundEvent>::new(),
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
        sounds: Vec::<SoundAssetRef>::new(),
        bga_assets: Vec::new(),
        total_notes: 1,
        end_time: time,
    }
}

fn chart_with_two_taps(first_time: TimeUs, second_time: TimeUs) -> PlayableChart {
    let lane = Lane::Key1;
    let first = NoteEvent {
        id: NoteId(1),
        lane,
        kind: NoteKind::Tap,
        tick: Default::default(),
        time: first_time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let second = NoteEvent {
        id: NoteId(2),
        lane,
        kind: NoteKind::Tap,
        tick: Default::default(),
        time: second_time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let mut chart = chart_with_tap(first_time);
    chart.lane_notes[lane.index()] = vec![first, second];
    chart.total_notes = 2;
    chart.end_time = second_time;
    chart
}

fn chart_with_long_start(time: TimeUs, end_time: TimeUs) -> PlayableChart {
    chart_with_lane_long_start(Lane::Key1, time, end_time)
}

fn chart_with_lane_long_start(lane: Lane, time: TimeUs, end_time: TimeUs) -> PlayableChart {
    let start = NoteEvent {
        id: NoteId(1),
        lane,
        kind: NoteKind::LongStart,
        tick: Default::default(),
        time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let end = NoteEvent {
        id: NoteId(2),
        lane,
        kind: NoteKind::LongEnd,
        tick: Default::default(),
        time: end_time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let mut chart = chart_with_tap(time);
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.lane_notes[lane.index()] = vec![start, end];
    chart.long_notes = vec![LongNotePair {
        lane,
        style: LongNoteStyle::ChannelPair,
        mode: None,
        start_note_id: NoteId(1),
        end_note_id: NoteId(2),
        start_tick: Default::default(),
        end_tick: Default::default(),
        start_time: time,
        end_time,
        sound: None,
    }];
    chart
}

fn press_at(time: TimeUs) -> InputEvent {
    press_lane_at(Lane::Key1, time)
}

fn press_lane_at(lane: Lane, time: TimeUs) -> InputEvent {
    InputEvent {
        source: InputSource::Human,
        lane,
        kind: InputKind::Press,
        time,
        device_kind: bmz_core::input::InputDeviceKind::Keyboard,
        scratch_direction: None,
    }
}

fn release_at(time: TimeUs) -> InputEvent {
    release_lane_at(Lane::Key1, time)
}

fn release_lane_at(lane: Lane, time: TimeUs) -> InputEvent {
    InputEvent {
        source: InputSource::Human,
        lane,
        kind: InputKind::Release,
        time,
        device_kind: bmz_core::input::InputDeviceKind::Keyboard,
        scratch_direction: None,
    }
}

fn chart_with_mine(time: TimeUs, damage: f64) -> PlayableChart {
    let lane = Lane::Key1;
    let note = NoteEvent {
        id: NoteId(7),
        lane,
        kind: NoteKind::Mine,
        tick: Default::default(),
        time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: Some(damage),
    };
    let mut lane_notes = std::array::from_fn(|_| Vec::new());
    lane_notes[lane.index()].push(note);
    PlayableChart {
        identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
        metadata: ChartMetadata::default(),
        lane_notes,
        long_notes: Vec::new(),
        bgm_events: Vec::<SoundEvent>::new(),
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
        sounds: Vec::<SoundAssetRef>::new(),
        bga_assets: Vec::new(),
        total_notes: 0,
        end_time: time,
    }
}

#[path = "tests/hln.rs"]
mod hln_tests;
#[path = "tests/judgement.rs"]
mod judgement;
#[path = "tests/mine.rs"]
mod mine;
