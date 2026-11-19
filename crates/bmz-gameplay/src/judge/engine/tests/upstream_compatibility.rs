use super::*;
use crate::judge::window::{
    JudgeWindowRates, judge_percent_at_time_for_keymode, judge_windows_for_keymode_and_rule_mode,
    judge_windows_for_rule_mode_and_keymode,
};
use crate::score::ScoreState;

#[test]
fn hcn_empty_poor_uses_the_effective_kind_lane_and_half_open_judge_time() {
    for (time, suppressed) in [
        (999_999, false),
        (1_000_000, true),
        (1_000_001, true),
        (1_999_999, true),
        (2_000_000, false),
        (2_000_001, false),
    ] {
        for (mode, lane) in [
            (LongNoteMode::Hcn, Lane::Key1),
            (LongNoteMode::Hcn, Lane::Key2),
            (LongNoteMode::Cn, Lane::Key1),
            (LongNoteMode::Ln, Lane::Key1),
        ] {
            let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
            // Resolve an undefined pair through the effective chart mode.
            chart.metadata.long_note_mode = mode;
            let mut tap =
                chart_with_lane_tap(lane, TimeUs(2_350_000)).lane_notes[lane.index()][0].clone();
            tap.id = NoteId(3);
            chart.lane_notes[lane.index()].push(tap.clone());
            if lane != Lane::Key1 {
                tap.id = NoteId(4);
                tap.time = TimeUs(1_350_000);
                chart.lane_notes[lane.index()].insert(0, tap);
            }
            let mut judge = engine(RuleMode::Beatoraja, KeyMode::K7, 100, JudgeAlgorithm::Combo);
            judge.process_input(&chart, press_at(TimeUs(950_000)));
            judge.process_input(&chart, release_at(TimeUs(960_000)));
            assert!(judge.lanes[Lane::Key1.index()].active_long.is_none());

            // The dispatch time intentionally sits outside the HCN. Only the
            // corrected judgement time should determine suppression.
            let result = judge.process_input_at(
                &chart,
                press_lane_at(lane, TimeUs(3_000_000)),
                TimeUs(time),
            );
            if suppressed && mode == LongNoteMode::Hcn && lane == Lane::Key1 {
                assert!(result.events.is_empty() && result.keysounds.is_empty());
                assert!(result.consumed_input);
            } else {
                assert_eq!(result.events[0].judge, Judge::EmptyPoor, "{time} {mode:?} {lane:?}");
            }
        }
    }
}

#[test]
fn hcn_missed_head_allows_regrab_without_recording_empty_poor_history_in_nine_key() {
    for rule in [RuleMode::Beatoraja, RuleMode::Dx] {
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(LongNoteMode::Hcn);
        let mut tap = chart_with_tap(TimeUs(2_100_000)).lane_notes[Lane::Key1.index()][0].clone();
        tap.id = NoteId(3);
        chart.lane_notes[Lane::Key1.index()].push(tap);
        let mut judge = engine(rule, KeyMode::K9, 100, JudgeAlgorithm::Combo);
        let missed = judge.process_misses(&chart, TimeUs(1_190_000));
        assert_eq!(missed.events.len(), 2);
        assert!(
            missed.events.iter().all(|event| event.judge == Judge::Poor && event.affects_score)
        );
        assert!(judge.lanes[Lane::Key1.index()].active_long.is_none());
        let regrab = judge.process_input(&chart, press_at(TimeUs(1_700_000)));
        assert!(regrab.events.is_empty() && regrab.keysounds.is_empty());
        assert!(!judge.attempted_notes.contains(&NoteId(3)));
        let missed_tap = judge.process_misses(&chart, TimeUs(2_300_000));
        assert!(missed_tap.events[0].affects_score);
        assert_eq!(missed_tap.events[0].note_id, Some(NoteId(3)));
    }
}

#[test]
fn hcn_suppression_does_not_reselect_a_farther_bad() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Hcn);
    let mut taps = chart_with_two_taps(TimeUs(1_500_000), TimeUs(1_880_000));
    for (index, mut tap) in taps.lane_notes[Lane::Key1.index()].drain(..).enumerate() {
        tap.id = NoteId(index as u32 + 3);
        chart.lane_notes[Lane::Key1.index()].insert(index + 1, tap);
    }
    let mut judge = engine(RuleMode::Beatoraja, KeyMode::K7, 75, JudgeAlgorithm::Duration);
    judge.process_input(&chart, press_at(TimeUs(1_000_000)));
    let release = judge.process_input(&chart, release_at(TimeUs(1_200_000)));
    assert_eq!(release.events[0].judge, Judge::Poor);
    let regrab = judge.process_input(&chart, press_at(TimeUs(1_700_000)));
    assert!(regrab.events.is_empty());
    assert!(!judge.judged_notes.contains_key(&NoteId(3)));
    assert!(!judge.judged_notes.contains_key(&NoteId(4)));
}

#[test]
fn hcn_regrab_still_judges_normal_notes_and_multi_bad() {
    for rule in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(LongNoteMode::Hcn);
        let mut prototype = chart_with_tap(TimeUs(0)).lane_notes[Lane::Key1.index()][0].clone();
        for (window_band, expected) in
            [(0, Judge::PGreat), (1, Judge::Great), (2, Judge::Good), (3, Judge::Bad)]
        {
            let mut judge = engine(rule, KeyMode::K7, 75, JudgeAlgorithm::Combo);
            let mut chart = chart.clone();
            let [pg, gr, gd] = judge.windows.bands(true);
            let delta = [pg, gr, gd, gd + 1][window_band];
            prototype.time = TimeUs(1_500_000 + delta);
            prototype.id = NoteId(3);
            chart.lane_notes[Lane::Key1.index()].insert(1, prototype.clone());
            if expected == Judge::Bad {
                chart.lane_notes[Lane::Key1.index()].insert(
                    2,
                    NoteEvent {
                        id: NoteId(4),
                        time: TimeUs(prototype.time.0 + 1),
                        ..prototype.clone()
                    },
                );
            }
            judge.process_input(&chart, press_at(TimeUs(1_000_000)));
            let release = judge.process_input(&chart, release_at(TimeUs(1_200_000)));
            assert_eq!(release.events[0].judge, Judge::Poor);
            let hit = judge.process_input(&chart, press_at(TimeUs(1_500_000)));
            let expected_count =
                if expected == Judge::Bad && rule != RuleMode::Beatoraja { 2 } else { 1 };
            assert_eq!(hit.events.len(), expected_count);
            assert!(hit.events.iter().all(|event| event.judge == expected));
        }
    }
}

fn engine(rule: RuleMode, mode: KeyMode, rank: i32, algorithm: JudgeAlgorithm) -> JudgeEngine {
    let base = judge_windows_for_keymode_and_rule_mode(mode, rule);
    let windows = judge_windows_for_rule_mode_and_keymode(base, rank, rule, mode);
    JudgeEngine::new_with_window_set_algorithm_and_keymode(windows, rule, algorithm, mode)
}

#[test]
fn asymmetric_custom_good_window_controls_actual_press_judgements() {
    let rule = RuleMode::Beatoraja;
    let chart = chart_with_tap(TimeUs(1_000_000));
    for (offset, expected) in [
        (-165_001, Some(Judge::EmptyPoor)),
        (-165_000, Some(Judge::Good)),
        (-164_999, Some(Judge::Good)),
        (180_000, Some(Judge::Good)),
        (209_999, Some(Judge::Good)),
        (210_000, Some(Judge::Good)),
        (210_001, None),
    ] {
        let mut judge = engine(rule, KeyMode::K7, 75, JudgeAlgorithm::Combo);
        let rates = JudgeWindowRates { keys: [100, 100, 200], ..Default::default() };
        judge.set_window_set(rates.apply(judge.window_set));
        let event = judge.process_input(&chart, press_at(TimeUs(1_000_000 + offset)));
        assert_eq!(event.events.first().map(|event| event.judge), expected, "{offset}");
    }
}

#[test]
fn dx_no_good_keeps_both_close_pgreats() {
    let mut judge = engine(RuleMode::Dx, KeyMode::K7, 100, JudgeAlgorithm::Combo);
    let rates = JudgeWindowRates { keys: [100, 100, 0], ..Default::default() };
    judge.set_window_set(rates.apply(judge.window_set));
    let chart = chart_with_two_taps(TimeUs(990_000), TimeUs(1_000_000));
    for (id, time) in [(NoteId(1), 1_000_000), (NoteId(2), 1_001_000)] {
        let outcome = judge.process_input(&chart, press_at(TimeUs(time)));
        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].note_id, Some(id));
        assert_eq!(outcome.events[0].judge, Judge::PGreat);
        judge.process_input(&chart, release_at(TimeUs(time)));
    }
}

#[test]
fn bms_rank_source_controls_out_of_range_fallback() {
    use bmz_chart::import::{BmsRandomSource, import_chart_bytes_with_random_source};
    use std::path::Path;
    for (header, expected) in [
        ("", [100, 75, 100]),
        ("#RANK 0", [25, 25, 100]),
        ("#RANK 4", [125, 75, 100]),
        ("#RANK 100", [75, 75, 100]),
        ("#DEFEXRANK 100", [75, 75, 100]),
    ] {
        let bytes = format!("#BPM 120\n{header}\n#00111:01\n");
        let chart = import_chart_bytes_with_random_source(
            Path::new("rank.bms"),
            bytes.as_bytes(),
            BmsRandomSource::Seed(Some(1)),
            false,
        )
        .unwrap()
        .chart;
        for (rule, percent) in
            [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx].into_iter().zip(expected)
        {
            assert_eq!(
                judge_percent_at_time_for_keymode(
                    chart.metadata.judge_rank_spec,
                    &chart.judge_rank_events,
                    TimeUs(0),
                    KeyMode::K7,
                    rule,
                ),
                percent,
                "{header} {rule:?}"
            );
        }
    }
}

#[test]
fn nine_key_ln_bad_counts_without_advancing_ghost_then_ln_can_be_recovered() {
    for rule in [RuleMode::Beatoraja, RuleMode::Dx] {
        let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        let mut judge = engine(rule, KeyMode::K9, 100, JudgeAlgorithm::Combo);
        let mut score = ScoreState::for_rule_mode(KeyMode::K9, rule);
        let bad = judge.process_input(&chart, press_at(TimeUs(850_000)));
        assert_eq!(bad.events[0].judge, Judge::Bad);
        assert!(!judge.judged_notes.contains_key(&NoteId(1)));
        score.combo = 2;
        score.apply(&bad.events[0]);
        assert_eq!((score.bp(), score.combo, score.past_notes), (1, 0, 0));
        assert!(score.ghost.is_empty());
        judge.process_input(&chart, release_at(TimeUs(860_000)));
        let head = judge.process_input(&chart, press_at(TimeUs(1_000_000)));
        score.apply(&head.events[0]);
        assert_eq!(score.past_notes, 0);
        let end = judge.process_misses(&chart, TimeUs(2_000_001));
        score.apply(&end.events[0]);
        assert_eq!((score.bp(), score.combo, score.past_notes), (1, 1, 1));
        assert_eq!(score.judges.fast_pgreat, 1);
        assert_eq!(score.ghost, [0]);
    }
}

#[test]
fn miss_once_remembers_empty_poor_but_consumes_the_later_miss() {
    for rule in [RuleMode::Beatoraja, RuleMode::Dx] {
        let chart = chart_with_tap(TimeUs(1_000_000));
        let mut judge = engine(rule, KeyMode::K9, 100, JudgeAlgorithm::Combo);
        let mut score = ScoreState::for_rule_mode(KeyMode::K9, rule);
        let first = judge.process_input(&chart, press_at(TimeUs(700_000)));
        assert_eq!(first.events[0].judge, Judge::EmptyPoor);
        score.apply(&first.events[0]);
        assert!(judge.process_input(&chart, press_at(TimeUs(750_000))).events.is_empty());
        let miss = judge.process_misses(&chart, TimeUs(1_300_000));
        assert!(!miss.events[0].affects_score);
        score.apply(&miss.events[0]);
        assert_eq!((score.bp(), score.past_notes), (1, 1));
        assert_eq!(score.ghost, [4]);
        assert!(judge.is_exhausted(&chart));

        let mut judge = engine(rule, KeyMode::K9, 100, JudgeAlgorithm::Combo);
        judge.process_input(&chart, press_at(TimeUs(1_000_000)));
        assert!(judge.process_input(&chart, press_at(TimeUs(1_010_000))).events.is_empty());
    }
}

#[test]
fn duration_can_prefer_an_empty_poor_to_a_farther_bad() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_380_000));
    for (algorithm, first_judge, bp) in [
        (JudgeAlgorithm::Duration, Judge::EmptyPoor, 2),
        (JudgeAlgorithm::Combo, Judge::Bad, 1),
        (JudgeAlgorithm::Lowest, Judge::Bad, 1),
    ] {
        let mut judge = engine(RuleMode::Beatoraja, KeyMode::K7, 75, algorithm);
        let mut score = ScoreState::default();
        let first = judge.process_input(&chart, press_at(TimeUs(1_200_000)));
        assert_eq!(first.events[0].judge, first_judge);
        score.apply(&first.events[0]);
        for event in judge.process_misses(&chart, TimeUs(1_211_000)).events {
            score.apply(&event);
        }
        judge.process_input(&chart, release_at(TimeUs(1_220_000)));
        let second = judge.process_input(&chart, press_at(TimeUs(1_380_000)));
        assert_eq!(second.events[0].judge, Judge::PGreat);
        score.apply(&second.events[0]);
        assert_eq!(score.bp(), bp);
        assert_eq!(score.past_notes, 2);
    }
}

#[test]
fn early_scan_limit_is_exclusive_without_changing_internal_band_boundaries() {
    for rule in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
        for lane in [Lane::Key1, Lane::Scratch] {
            let chart = chart_with_lane_tap(lane, TimeUs(2_000_000));
            let mut judge = engine(rule, KeyMode::K7, 75, JudgeAlgorithm::Combo);
            let (_, limit) = judge.window_set.press_scan_limits();
            for (offset, count) in [(-1, 0), (0, 0), (1, 1)] {
                let time = TimeUs(2_000_000 - limit + offset);
                assert_eq!(
                    judge.process_input(&chart, press_lane_at(lane, time)).events.len(),
                    count
                );
            }
            let width = judge.window_set.press_window(lane).bands(true)[0];
            let event = judge.process_input(&chart, press_lane_at(lane, TimeUs(2_000_000 - width)));
            assert_eq!(event.events[0].judge, Judge::PGreat);
        }
    }
}
