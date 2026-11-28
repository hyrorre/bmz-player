//! The CustomIR submission fields built from LEGACY judgement events.
use super::*;
use crate::lr2_score::{Lr2Judgements, Lr2Score, Lr2ScoreRecorder};

/// RANK NORMAL: PGREAT 18 / GREAT 40 / GOOD 100 / BAD 200 ms.
fn legacy_engine() -> JudgeEngine {
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    engine.set_legacy_inline_mines(true);
    engine
}

/// Polls Key1 every millisecond in `0..=end` with the given presses
/// (ms, pressed), feeding every event to the recorder and logging the graphs.
fn play(chart: &PlayableChart, edges: &[(i64, bool)], end: i64) -> Lr2Score {
    let mut engine = legacy_engine();
    let mut recorder = Lr2ScoreRecorder::new(chart, 0, 160.0);
    let mut held = false;
    for ms in 0..=end {
        let now = TimeUs(ms * 1_000);
        let outcome = match edges.iter().find(|(at, _)| *at == ms) {
            Some(&(_, press)) => {
                held = press;
                let input = if press { press_at(now) } else { release_at(now) };
                engine.process_input(chart, input)
            }
            None => engine.process_legacy_lane_poll(chart, Lane::Key1, now, held, false),
        };
        for event in &outcome.events {
            recorder.apply_event(event);
        }
        for hit in &outcome.mine_hits {
            recorder.apply_mine(hit.damage);
        }
        recorder.log_graph(ms);
    }
    recorder.finish()
}

#[test]
fn taps_count_early_late_and_fast_slow_like_proc_singlenote() {
    let mut chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_100_000));
    chart.total_notes = 2;
    // 10 ms early: PGREAT, never FAST/SLOW. 30 ms late: GREAT and SLOW.
    let score = play(&chart, &[(990, true), (991, false), (1_130, true), (1_131, false)], 1_500);
    let total = score.judgements_total;
    assert_eq!((total.epg, total.lgr, total.fast, total.slow), (1, 1, 0, 1));
    assert_eq!((total.notes_played, total.notes_total), (2, 2));
    assert_eq!(score.judgements_column[1], Lr2Judgements { notes_total: 2, ..total });
    assert_eq!((score.exscore, score.moneyscore, score.max_combo), (3, 150_000, 2));
    assert_eq!(score.rate, 75.0);
    // EASY ... FULL COMBO: two notes, two combo.
    assert_eq!(score.clear_type, 5);
    assert_eq!(score.ghost, "EDZ");
}

#[test]
fn a_long_note_counts_fast_slow_at_the_head_and_its_early_late_at_the_end() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.total_notes = 1;
    // Head 30 ms early (GREAT): FAST now; held through the end: an early GREAT.
    let score = play(&chart, &[(970, true)], 2_500);
    let total = score.judgements_total;
    assert_eq!((total.egr, total.lgr, total.fast, total.slow, total.notes_played), (1, 0, 1, 0, 1));
}

#[test]
fn notes_left_at_the_end_zero_every_gauge_and_fail() {
    let mut chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_100_000));
    chart.total_notes = 2;
    // The first note passes at 1201 ms; the play ends before the second.
    let score = play(&chart, &[], 1_250);
    assert_eq!((score.judgements_total.lpr, score.judgements_total.cb), (1, 1));
    assert_eq!(score.hp, [0.0; 6]);
    assert_eq!(score.clear_type, 1);
    assert_eq!(score.ghost, "AZ");
}

#[test]
fn graphs_sample_the_state_over_the_song_runtime() {
    let mut chart = chart_with_tap(TimeUs(1_000_000));
    chart.total_notes = 1;
    chart.metadata.legacy_song_runtime_ms = Some(2_000.0);
    let score = play(&chart, &[(1_000, true), (1_001, false)], 1_500);
    // Sample i is written by the first poll with ms * 1000 / 2000 >= i, then
    // the rest is filled with the final state.
    assert_eq!(score.graphs.exscore[499], 0);
    assert_eq!(score.graphs.exscore[500], 2);
    assert_eq!(score.graphs.exscore[999], 2);
    assert_eq!(score.graphs.combo[500], 1);
    // GROOVE starts at 20 and gains TOTAL / notes (160) on the PGREAT.
    assert_eq!(score.graphs.hp[0][0], 20);
    assert_eq!(score.graphs.hp[0][500], 100);
    // P-ATTACK gains 0.1 on a PGREAT from its cap of 100.
    assert_eq!(score.hp[crate::lr2_score::LR2_GAUGE_PATTACK], 100.0);
}

#[test]
fn a_long_note_judged_by_the_bad_recursion_counts_as_a_tap() {
    let mut chart = chart_with_long_start(TimeUs(1_180_000), TimeUs(2_000_000));
    let mut tap = chart.lane_notes[Lane::Key1.index()][0].clone();
    tap.id = NoteId(3);
    tap.kind = NoteKind::Tap;
    tap.time = TimeUs(1_000_000);
    chart.lane_notes[Lane::Key1.index()].insert(0, tap);
    chart.total_notes = 2;
    // 150 ms late on the tap: BAD. ProcSinglenote then judges the long note
    // 30 ms early as a single note: GREAT, scored now and counted FAST.
    let score = play(&chart, &[(1_150, true), (1_151, false)], 1_500);
    let total = score.judgements_total;
    assert_eq!((total.lbd, total.egr, total.fast, total.slow, total.cb), (1, 1, 1, 1, 1));
    assert_eq!(total.notes_played, 2);
}

#[test]
fn an_infinite_song_runtime_keeps_the_starting_graph() {
    // (int)inf is INT_MIN with MSVC, so OpenLR2 logs nothing and repeats the
    // first sample.
    let mut chart = chart_with_tap(TimeUs(1_000_000));
    chart.total_notes = 1;
    chart.metadata.legacy_song_runtime_ms = Some(f64::INFINITY);
    let score = play(&chart, &[(1_000, true), (1_001, false)], 1_500);
    assert_eq!(score.exscore, 2);
    assert!(score.graphs.exscore.iter().all(|&v| v == 0));
    assert!(score.graphs.hp[0].iter().all(|&v| v == 20));
}
