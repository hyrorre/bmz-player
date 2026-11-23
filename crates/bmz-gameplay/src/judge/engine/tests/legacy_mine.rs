//! OpenLR2 `ProcSinglenote` mine branch on the LEGACY polling clock.
use super::*;

/// RANK NORMAL: PGREAT 18 / GREAT 40 / GOOD 100 / BAD 200 ms.
fn legacy_engine() -> JudgeEngine {
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    engine.set_legacy_inline_mines(true);
    engine
}

fn tap(id: u32, time: TimeUs) -> NoteEvent {
    NoteEvent {
        id: NoteId(id),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: Default::default(),
        time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    }
}

/// Mine (id 7, damage 8) at `mine`, followed or preceded by the given taps.
fn chart_with_mine_and_taps(mine: TimeUs, taps: &[(u32, TimeUs)]) -> PlayableChart {
    let mut chart = chart_with_mine(mine, 8.0);
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes.extend(taps.iter().map(|&(id, time)| tap(id, time)));
    notes.sort_by_key(|note| note.time);
    chart.total_notes = taps.len() as u32;
    chart.end_time = notes.last().map_or(mine, |note| note.time);
    chart
}

#[test]
fn legacy_mine_is_hit_by_a_press_inside_the_early_great_width() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = legacy_engine();
    let outcome = engine.process_input(&chart, press_at(TimeUs(961_000)));
    assert_eq!(outcome.mine_hits.len(), 1);
    assert_eq!(outcome.mine_hits[0].damage, 8.0);
    assert!(outcome.events.is_empty());
    assert!(outcome.consumed_input);
}

#[test]
fn legacy_mine_early_great_boundary_is_exclusive() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = legacy_engine();
    // realTiming - timing < GREAT: exactly 40 ms early is not a hit.
    let early = engine.process_input(&chart, press_at(TimeUs(960_000)));
    assert!(early.mine_hits.is_empty());
    assert!(early.consumed_input, "the blocked press plays nothing");
    assert!(engine.process_input(&chart, release_at(TimeUs(970_000))).mine_hits.is_empty());
    // An idle poll after the mine time lets it go without damage.
    let passed =
        engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(1_001_000), false, false);
    assert!(passed.mine_hits.is_empty());
    assert!(engine.process_input(&chart, press_at(TimeUs(1_010_000))).mine_hits.is_empty());
}

#[test]
fn legacy_mine_is_hit_by_a_key_held_on_the_first_poll_after_it() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = legacy_engine();
    let at = engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(1_000_000), true, false);
    assert!(at.mine_hits.is_empty(), "the mine has not passed at its own time");
    let after = engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(1_001_000), true, false);
    assert_eq!(after.mine_hits.len(), 1);
    let again = engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(1_002_000), true, false);
    assert!(again.mine_hits.is_empty(), "a consumed mine does not fire twice");
}

#[test]
fn legacy_release_while_the_mine_passes_does_not_hit() {
    let chart = chart_with_mine(TimeUs(1_000_000), 8.0);
    let mut engine = legacy_engine();
    let release = engine.process_input(&chart, release_at(TimeUs(1_001_000)));
    assert!(release.mine_hits.is_empty());
    let held = engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(1_002_000), true, false);
    assert!(held.mine_hits.is_empty(), "the release already consumed the mine");
}

#[test]
fn legacy_mine_hit_takes_the_press_and_blocks_the_following_tap() {
    let chart = chart_with_mine_and_taps(TimeUs(1_000_000), &[(1, TimeUs(1_020_000))]);
    let mut engine = legacy_engine();
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    assert_eq!(outcome.mine_hits.len(), 1);
    assert!(outcome.events.is_empty(), "the same press does not judge the tap");
    // Holding through the tap does not judge it either; it is missed later.
    for ms in 1_001..=1_220 {
        let poll =
            engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(ms * 1_000), true, false);
        assert!(poll.events.is_empty() && poll.mine_hits.is_empty(), "poll at {ms} ms");
    }
    let missed =
        engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(1_221_000), true, false);
    assert_eq!(missed.events[0].judge, Judge::Poor);
}

#[test]
fn legacy_pending_mine_blocks_an_earlier_press_for_the_tap_behind_it() {
    let chart = chart_with_mine_and_taps(TimeUs(1_000_000), &[(1, TimeUs(1_010_000))]);
    let mut engine = legacy_engine();
    let outcome = engine.process_input(&chart, press_at(TimeUs(900_000)));
    assert!(outcome.events.is_empty(), "no EMPTY POOR for the tap behind the mine");
    assert!(outcome.mine_hits.is_empty());
    assert!(outcome.keysounds.is_empty());
    assert!(outcome.consumed_input);
}

#[test]
fn legacy_tap_bad_recursion_can_step_on_the_next_mine() {
    let chart = chart_with_mine_and_taps(TimeUs(1_000_000), &[(1, TimeUs(800_000))]);
    let mut engine = legacy_engine();
    let outcome = engine.process_input(&chart, press_at(TimeUs(990_000)));
    assert_eq!(outcome.events.iter().map(|e| e.judge).collect::<Vec<_>>(), [Judge::Bad]);
    assert_eq!(outcome.mine_hits.len(), 1);
}

#[test]
fn legacy_mine_before_its_tap_is_judged_in_lane_order() {
    let chart = chart_with_mine_and_taps(TimeUs(1_000_000), &[(1, TimeUs(990_000))]);
    let mut engine = legacy_engine();
    // The tap comes first in the lane, so the press scores it and leaves the mine.
    let first = engine.process_input(&chart, press_at(TimeUs(990_000)));
    assert_eq!(first.events[0].judge, Judge::PGreat);
    assert!(first.mine_hits.is_empty());
    let second = engine.process_input(&chart, press_at(TimeUs(995_000)));
    assert_eq!(second.mine_hits.len(), 1);
}

#[test]
fn shared_mine_sweep_is_unchanged_without_inline_mines() {
    let chart = chart_with_mine_and_taps(TimeUs(1_000_000), &[(1, TimeUs(1_020_000))]);
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    assert!(outcome.mine_hits.is_empty());
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
}

#[test]
fn legacy_ln_finishes_without_the_end_keysound() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let end_note_id = chart.long_notes[0].end_note_id;
    let mut engine = legacy_engine();
    engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let held = engine.process_legacy_lane_poll(&chart, Lane::Key1, TimeUs(2_001_000), true, false);
    assert_eq!(held.events.last().map(|e| e.judge), Some(Judge::PGreat));
    assert!(held.keysounds.iter().all(|sound| sound.note_id != end_note_id));

    let mut released = legacy_engine();
    released.process_input(&chart, press_at(TimeUs(1_000_000)));
    let outcome = released.process_input(&chart, release_at(TimeUs(1_950_000)));
    assert_eq!(outcome.events.last().map(|e| e.judge), Some(Judge::PGreat));
    assert!(outcome.keysounds.iter().all(|sound| sound.note_id != end_note_id));
}
