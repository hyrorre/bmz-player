//! OpenLR2's `note_current < totalnotes` cap, the free buffer slots after a
//! lane's objects and its double note times, on the LEGACY polling clock.
use super::*;

/// RANK NORMAL: PGREAT 18 / GREAT 40 / GOOD 100 / BAD 200 ms.
fn legacy_engine() -> JudgeEngine {
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    engine.set_legacy_inline_mines(true);
    engine
}

fn tap(id: u32, lane: Lane, ms: i64) -> NoteEvent {
    NoteEvent {
        id: NoteId(id),
        lane,
        kind: NoteKind::Tap,
        tick: Default::default(),
        time: TimeUs(ms * 1_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    }
}

/// Polls `lane` every millisecond in `from..=to` and returns (ms, judge).
fn poll(
    engine: &mut JudgeEngine,
    chart: &PlayableChart,
    lane: Lane,
    from: i64,
    to: i64,
) -> Vec<(i64, Judge)> {
    let mut judged = Vec::new();
    for ms in from..=to {
        let outcome =
            engine.process_legacy_lane_poll(chart, lane, TimeUs(ms * 1_000), false, false);
        judged.extend(outcome.events.iter().filter(|e| e.affects_score).map(|e| (ms, e.judge)));
    }
    judged
}

#[test]
fn a_passed_single_note_is_not_judged_once_the_note_count_is_reached() {
    // LR2's totalnotes can be lower than the judgeable notes.
    let mut chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_100_000));
    chart.total_notes = 1;
    let mut engine = legacy_engine();
    assert_eq!(poll(&mut engine, &chart, Lane::Key1, 0, 2_000), [(1_201, Judge::Poor)]);
    // The second note stays at the cursor and a press there judges nothing.
    let press = engine.process_input(&chart, press_at(TimeUs(1_100_000)));
    assert!(press.events.is_empty());
}

#[test]
fn a_lane_of_only_mines_judges_free_slots_until_the_note_count() {
    let mut chart = chart_with_mine(TimeUs(500_000), 8.0);
    chart.lane_notes[Lane::Key2.index()] =
        vec![tap(1, Lane::Key2, 3_000), tap(2, Lane::Key2, 3_100), tap(3, Lane::Key2, 3_200)];
    chart.total_notes = 3;
    let mut engine = legacy_engine();
    // The idle poll after the mine lets it go; each later poll judges a slot.
    assert_eq!(
        poll(&mut engine, &chart, Lane::Key1, 0, 600),
        [(502, Judge::Poor), (503, Judge::Poor), (504, Judge::Poor)]
    );
    assert!(poll(&mut engine, &chart, Lane::Key2, 0, 4_000).is_empty());
    assert!(!engine.legacy_read_past_lane_buffer());
}

#[test]
fn free_slots_end_at_the_buffer_size() {
    // One mine leaves 99 free slots in LR2's 100-object buffer.
    let mut chart = chart_with_mine(TimeUs(500_000), 8.0);
    chart.total_notes = 1_000;
    let mut engine = legacy_engine();
    // POOR from 502 to 600 ms; the call after the last slot reads past the buffer.
    assert_eq!(poll(&mut engine, &chart, Lane::Key1, 0, 600).len(), 99);
    assert!(!engine.legacy_read_past_lane_buffer());
    assert!(poll(&mut engine, &chart, Lane::Key1, 601, 601).is_empty());
    assert!(engine.legacy_read_past_lane_buffer());
}

#[test]
fn a_bad_on_the_last_note_recurses_into_a_free_slot_near_the_start() {
    let mut chart = chart_with_lane_tap(Lane::Key1, TimeUs(150_000));
    chart.total_notes = 5;
    let mut engine = legacy_engine();
    // BAD on the note, then the free slot at -1 ms is 1 ms away: PGREAT.
    let press = engine.process_input(&chart, press_at(TimeUs(0)));
    let judged: Vec<_> = press.events.iter().map(|e| (e.note_id, e.judge)).collect();
    assert_eq!(judged, [(Some(NoteId(1)), Judge::Bad), (None, Judge::PGreat)]);
    // The next free slot passes after 200 ms.
    assert_eq!(
        poll(&mut engine, &chart, Lane::Key1, 1, 201),
        [(200, Judge::Poor), (201, Judge::Poor)]
    );
}

#[test]
fn a_note_just_below_a_whole_millisecond_passes_with_the_double_rounding() {
    // OpenLR2: 202 - 1.9999999999999996 rounds to 200.0, which is not > BAD.
    // The µs-floored time (1999 µs) alone would pass the note at 202 ms.
    let mut chart = chart_with_lane_tap(Lane::Key1, TimeUs(1_999));
    chart.metadata.legacy_note_ms =
        Some(std::sync::Arc::new([(NoteId(1), 1.999_999_999_999_999_6)].into_iter().collect()));
    let mut engine = legacy_engine();
    assert_eq!(poll(&mut engine, &chart, Lane::Key1, 0, 203), [(203, Judge::Poor)]);
}
