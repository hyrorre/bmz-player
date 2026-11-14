use bmz_chart::model::{TimingEvent, TimingEventKind};
use bmz_core::replay::ReplayEvent;

use super::*;

fn reflect_at(chart: &mut PlayableChart, time: i64) {
    chart.timing_events.push(TimingEvent {
        tick: ChartTick(1_920),
        time: TimeUs(time),
        kind: TimingEventKind::BpmReflection,
    });
}

fn tap_before_reflection() -> PlayableChart {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()][0].time = TimeUs(980_000);
    chart.end_time = TimeUs(980_000);
    reflect_at(&mut chart, 1_000_000);
    chart
}

#[test]
fn reflected_input_uses_the_mirrored_delta_and_forward_effect_time() {
    let mut manual = session_with_autoplay(tap_before_reflection());
    manual.autoplay = None;
    let actual = TimeUs(1_025_000);
    let input = human_press(actual);
    manual.replay_recorder.record(input);
    let events = process_session_input(&mut manual, input);
    assert_eq!(events[0].judge, Judge::PGreat);
    assert_eq!(events[0].side, TimingSide::Fast);
    assert_eq!(events[0].delta, TimeUs(-5_000));
    assert_eq!(events[0].time, actual);
    assert_eq!(manual.pending_keysounds[0].time, actual);
    assert_eq!(manual.replay_recorder.events[0].time, actual);

    let mut replay = session_with_autoplay(tap_before_reflection());
    replay.autoplay = None;
    replay.replay_player = Some(ReplayPlayer::new(vec![ReplayEvent {
        time: actual,
        lane: input.lane,
        kind: input.kind,
        device_kind: input.device_kind,
        scratch_direction: None,
    }]));
    assert!(process_replay_inputs(&mut replay, TimeUs(actual.0 - 1)).is_empty());
    let replayed = process_replay_inputs(&mut replay, actual);
    assert_eq!(replayed[0].delta, events[0].delta);
    assert_eq!(replayed[0].time, actual);
    assert_eq!(replay.score.ex_score(), manual.score.ex_score());

    let mut chart = tap_before_reflection();
    chart.lane_notes[Lane::Key1.index()][0].time = TimeUs(1_200_000);
    let mut auto = session_with_autoplay(chart);
    let auto_input = InputEvent { time: TimeUs(1_200_000), source: InputSource::Auto, ..input };
    let events = process_session_input(&mut auto, auto_input);
    assert_eq!(events[0].delta, TimeUs(0));
}

#[test]
fn reflection_keeps_pending_notes_and_revisits_the_last_forward_deadline() {
    let mut chart = tap_before_reflection();
    chart.lane_notes[Lane::Key1.index()][0].time = TimeUs(700_000);
    let mut other = chart.lane_notes[Lane::Key1.index()][0].clone();
    other.id = NoteId(2);
    other.time = TimeUs(1_200_000);
    chart.lane_notes[Lane::Key1.index()].push(other);
    chart.total_notes = 2;
    chart.end_time = TimeUs(1_200_000);

    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.state = PlayState::Playing;
    let mut audio = TestAudio::default();
    session.audio_clock.pause_at(TimeUs(1_600_000));
    let frame = advance_session_frame(&mut session, &mut audio);
    assert_eq!(frame.judgements.len(), 1);
    assert_eq!(frame.judgements[0].note_id, Some(NoteId(1)));
    assert_eq!(frame.judgements[0].time, TimeUs(999_999));
    assert!(!session.judge.judged_notes.contains_key(&NoteId(2)));
    session.audio_clock.pause_at(TimeUs(6_300_000));
    let frame = advance_session_frame(&mut session, &mut audio);
    assert_eq!(frame.state, PlayState::Failed);
    assert!(frame.judgements.is_empty());
}

#[test]
fn reflection_applies_to_long_release_and_mine_passes() {
    let mut chart = chart_with_hcn_long_note();
    chart.long_notes[0].mode = Some(LongNoteMode::Ln);
    chart.long_notes[0].end_time = TimeUs(1_005_000);
    chart.lane_notes[Lane::Key1.index()][1].time = TimeUs(1_005_000);
    reflect_at(&mut chart, 1_000_000);
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    process_session_input(&mut session, human_press(TimeUs(0)));
    assert!(process_misses(&mut session, TimeUs(1_025_000)).is_empty());
    let release = process_session_input(&mut session, human_release(TimeUs(1_025_000)));
    assert_eq!(release[0].judge, Judge::Great);
    assert_eq!(release[0].delta, TimeUs(-30_000));
    assert_eq!(release[0].time, TimeUs(1_025_000));

    let mut chart = chart_with_mine(TimeUs(1_100_000), 10.0);
    reflect_at(&mut chart, 1_000_000);
    let mut mine = session_with_autoplay(chart);
    mine.autoplay = None;
    mine.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));
    process_mine_passes(&mut mine, TimeUs(1_100_000));
    assert!(mine.pending_mine_hits.is_empty());
    assert_eq!(mine.judge.lanes[Lane::Key1.index()].next_mine_index, 0);
}

#[test]
fn seek_uses_the_chart_reflection_at_the_destination() {
    let mut session = session_with_autoplay(tap_before_reflection());
    prepare_viewer_seek(&mut session, TimeUs(1_020_000));
    assert_eq!(session.timing_map.reflected_judge_time(TimeUs(1_030_000)), TimeUs(970_000));
    assert_eq!(session.score.past_notes, 1);
    assert!(session.pending_keysounds.is_empty());
}
