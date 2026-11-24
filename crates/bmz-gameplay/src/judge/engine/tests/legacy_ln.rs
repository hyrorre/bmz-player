//! LEGACY standard LN release, decided on integer milliseconds as in OpenLR2.
use super::*;
use bmz_core::ids::SoundId;

fn legacy_engine() -> JudgeEngine {
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy)
}

/// Head at 1000 ms with a keysound, tail at 2000.5 ms.
fn chart() -> PlayableChart {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_500));
    chart.lane_notes[Lane::Key1.index()][0].sound = Some(SoundId(1));
    chart.long_notes[0].sound = Some(SoundId(1));
    chart
}

/// Presses 150 ms early (a BAD head) and releases at `release_ms`.
fn release(release_ms: i64) -> JudgeOutcome {
    let chart = chart();
    let mut engine = legacy_engine();
    engine.process_input(&chart, press_at(TimeUs(850_000)));
    engine.process_input(&chart, release_at(TimeUs(release_ms * 1_000)))
}

#[test]
fn a_release_within_the_good_width_in_integer_ms_keeps_the_head_sound() {
    // 1900 - (int)2000.5 = -100 ms is not earlier than the GOOD width, so
    // OpenLR2 scores the head's BAD without stopping its sound. The µs
    // difference (-100.5 ms) must not decide the mute.
    let outcome = release(1_900);
    assert_eq!(outcome.events.iter().map(|e| e.judge).collect::<Vec<_>>(), [Judge::Bad]);
    assert!(outcome.keysound_volumes.is_empty(), "{:?}", outcome.keysound_volumes);
}

#[test]
fn an_early_release_beyond_the_good_width_stops_the_head_sound() {
    let outcome = release(1_899);
    assert_eq!(outcome.events.iter().map(|e| e.judge).collect::<Vec<_>>(), [Judge::Bad]);
    assert_eq!(outcome.keysound_volumes, [(SoundId(1), 0.0)]);
}
