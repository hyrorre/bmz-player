use super::*;
use bmz_chart::model::{FreeZone, LongNoteMode, LongNotePair, LongNoteStyle, NoteEvent};
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::time::{ChartTick, TimeUs};

fn note(id: u32, lane: Lane, kind: NoteKind, at: i64, sounds: &[u32]) -> NoteEvent {
    NoteEvent {
        id: NoteId(id),
        lane,
        kind,
        tick: ChartTick(at as u64),
        time: TimeUs(at),
        sound: sounds.first().copied().map(SoundId),
        layered_sounds: sounds.iter().skip(1).copied().map(SoundId).collect(),
        damage: (kind == NoteKind::Mine).then_some(10.0),
    }
}

fn bgm(at: i64, sound: u32) -> SoundEvent {
    SoundEvent { tick: ChartTick(at as u64), time: TimeUs(at), sound: SoundId(sound) }
}

fn scratch_chart() -> PlayableChart {
    let mut chart = super::super::tests::chart();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    chart.lane_notes[Lane::Key1.index()] = vec![note(1, Lane::Key1, NoteKind::Tap, 100, &[1])];
    chart.lane_notes[Lane::Scratch.index()] = vec![
        note(10, Lane::Scratch, NoteKind::Tap, 100, &[5, 6]),
        note(11, Lane::Scratch, NoteKind::Invisible, 150, &[9]),
        note(12, Lane::Scratch, NoteKind::LongStart, 200, &[7]),
        note(13, Lane::Scratch, NoteKind::LongEnd, 300, &[8]),
        note(14, Lane::Scratch, NoteKind::Mine, 400, &[]),
    ];
    chart.lane_notes[Lane::Scratch2.index()] =
        vec![note(20, Lane::Scratch2, NoteKind::Tap, 250, &[4])];
    chart.long_notes = vec![LongNotePair {
        lane: Lane::Scratch,
        style: LongNoteStyle::ChannelPair,
        mode: Some(LongNoteMode::Cn),
        start_note_id: NoteId(12),
        end_note_id: NoteId(13),
        start_tick: ChartTick(200),
        end_tick: ChartTick(300),
        start_time: TimeUs(200),
        end_time: TimeUs(300),
        sound: Some(SoundId(7)),
    }];
    chart.bgm_events = vec![bgm(100, 2), bgm(500, 3)];
    chart.total_notes = 4;
    chart
}

fn kinds(chart: &PlayableChart, lane: Lane) -> Vec<NoteKind> {
    chart.notes_for_lane(lane).iter().map(|note| note.kind).collect()
}

#[test]
fn scratch_notes_leave_scoring_and_sound_as_bgm() {
    let mut chart = scratch_chart();

    assert!(move_auto_scratch_notes_to_background(&mut chart, &[Lane::Scratch, Lane::Scratch2]));

    assert_eq!(
        kinds(&chart, Lane::Scratch),
        [
            NoteKind::DummyTap,
            NoteKind::Invisible,
            NoteKind::DummyLongStart,
            NoteKind::DummyLongEnd,
            NoteKind::DummyMine,
        ]
    );
    assert_eq!(kinds(&chart, Lane::Scratch2), [NoteKind::DummyTap]);
    assert_eq!(kinds(&chart, Lane::Key1), [NoteKind::Tap]);
    assert_eq!(chart.notes_for_lane(Lane::Key1)[0].sound, Some(SoundId(1)));
    // beatoraja leaves hidden notes in the lane; only the visible notes become BGM.
    assert_eq!(chart.notes_for_lane(Lane::Scratch)[1].sound, Some(SoundId(9)));
    assert!(chart.notes_for_lane(Lane::Scratch)[0].sounds().next().is_none());
    let sounds: Vec<_> =
        chart.bgm_events.iter().map(|event| (event.time.0, event.sound.0)).collect();
    assert_eq!(sounds, [(100, 2), (100, 5), (100, 6), (200, 7), (250, 4), (300, 8), (500, 3)]);
    assert_eq!(chart.total_notes, 1);
    assert_eq!(bmz_gameplay::score::scored_note_count(&chart), 1);
    assert!(chart.is_dummy_long(&chart.long_notes[0]));
}

#[test]
fn converting_twice_changes_nothing() {
    let mut chart = scratch_chart();
    let lanes = [Lane::Scratch, Lane::Scratch2];
    assert!(has_auto_scratch_notes(&chart, &lanes));
    move_auto_scratch_notes_to_background(&mut chart, &lanes);
    let bgm_len = chart.bgm_events.len();

    assert!(!has_auto_scratch_notes(&chart, &lanes));
    assert!(!move_auto_scratch_notes_to_background(&mut chart, &lanes));
    assert_eq!(chart.bgm_events.len(), bgm_len);
}

#[test]
fn only_the_requested_scratch_lane_changes() {
    let mut chart = scratch_chart();

    assert!(move_auto_scratch_notes_to_background(&mut chart, &[Lane::Scratch]));

    assert_eq!(kinds(&chart, Lane::Scratch2), [NoteKind::Tap]);
    assert_eq!(chart.total_notes, 2);
}

#[test]
fn free_zones_on_auto_lanes_are_removed() {
    let mut chart = scratch_chart();
    chart.lane_notes[Lane::Scratch.index()].push(note(
        15,
        Lane::Scratch,
        NoteKind::FreeZoneEnd,
        450,
        &[],
    ));
    chart.metadata.free_zones.push(FreeZone {
        note_id: NoteId(15),
        lane: Lane::Scratch,
        start_tick: ChartTick(50),
        end_tick: ChartTick(450),
        start_time: TimeUs(50),
        end_time: TimeUs(450),
        inner_notes: vec![NoteId(10)],
    });

    assert!(move_auto_scratch_notes_to_background(&mut chart, &[Lane::Scratch]));

    assert!(chart.metadata.free_zones.is_empty());
    assert!(chart.note_by_id(NoteId(15)).is_none());
    assert_eq!(chart.notes_for_lane(Lane::Scratch)[0].kind, NoteKind::DummyTap);
}
