use super::*;

#[test]
fn legacy_same_input_can_score_differently_when_an_idle_poll_precedes_it() {
    let chart = chart_with_two_taps(TimeUs(1_000_100), TimeUs(1_201_100));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut fine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let mut coarse = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let idle = fine.process_legacy_idle_lane(&chart, Lane::Key1, TimeUs(1_201_000), false);
    assert_eq!(idle.events[0].judge, Judge::Poor);
    // Same chart, input, judge engine and input time; only the idle poll differs.
    let input = press_at(TimeUs(1_202_000));
    let fine_result = fine.process_input(&chart, input);
    let coarse_result = coarse.process_input(&chart, input);
    assert_eq!(fine_result.events[0].note_id, Some(NoteId(2)));
    assert_eq!(fine_result.events[0].judge, Judge::PGreat);
    assert_eq!(coarse_result.events[0].note_id, Some(NoteId(1)));
    assert_eq!(coarse_result.events[0].judge, Judge::Poor);
}

#[test]
fn legacy_bad_recursion_scores_the_next_ln_as_a_single_note() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let mut tap = chart.lane_notes[Lane::Key1.index()][0].clone();
    tap.kind = NoteKind::Tap;
    tap.id = NoteId(3);
    tap.time = TimeUs(860_000);
    chart.lane_notes[Lane::Key1.index()].insert(0, tap);
    chart.total_notes = 2;
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let scored: Vec<_> =
        outcome.events.iter().filter(|e| e.affects_score).map(|e| e.judge).collect();
    assert_eq!(scored, [Judge::Bad, Judge::PGreat]);
    assert!(engine.lanes[Lane::Key1.index()].active_long.is_none());
    assert!(engine.process_input(&chart, release_at(TimeUs(1_100_000))).events.is_empty());
    assert!(
        engine
            .process_legacy_idle_lane(&chart, Lane::Key1, TimeUs(2_001_000), false)
            .events
            .is_empty()
    );
}

#[test]
fn legacy_recursive_ln_bad_uses_the_tap_window_and_continues_recursion() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let mut first = chart.lane_notes[Lane::Key1.index()][0].clone();
    first.kind = NoteKind::Tap;
    first.id = NoteId(3);
    first.time = TimeUs(950_000);
    let mut last = first.clone();
    last.id = NoteId(4);
    last.time = TimeUs(1_150_000);
    chart.lane_notes[Lane::Key1.index()].insert(0, first);
    chart.lane_notes[Lane::Key1.index()].insert(2, last);
    chart.total_notes = 3;
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));
    let scored: Vec<_> =
        outcome.events.iter().filter(|e| e.affects_score).map(|e| e.judge).collect();
    assert_eq!(scored, [Judge::Bad, Judge::Bad, Judge::PGreat]);
    assert!(engine.lanes[Lane::Key1.index()].active_long.is_none());
}

#[test]
fn chart_index_preserves_hcn_overlap_order_and_falls_back_after_chart_replacement() {
    let mut chart = chart_with_long_start(TimeUs(0), TimeUs(1000));
    chart.metadata.long_note_mode = LongNoteMode::Hcn;
    let mut second = chart.long_notes[0].clone();
    second.start_time = TimeUs(100);
    second.end_time = TimeUs(200);
    second.start_note_id = NoteId(3);
    chart.long_notes.push(second);
    let chart = std::sync::Arc::new(chart);
    let judge = JudgeEngine::new(windows()).with_chart_index(chart.clone());
    let passing = |now| {
        judge
            .hcn_pairs_at(&chart, TimeUs(now))
            .filter(|(_, pair)| pair.start_time.0 <= now && now < pair.end_time.0)
            .map(|(_, pair)| pair.start_note_id)
            .collect::<Vec<_>>()
    };
    assert_eq!(passing(99), vec![NoteId(1)]);
    assert_eq!(passing(100), vec![NoteId(1), NoteId(3)]);
    assert_eq!(passing(200), vec![NoteId(1)]);
    assert!(passing(1000).is_empty());
    for note in chart.lane_notes.iter().flatten() {
        assert_eq!(judge.note_by_id(&chart, note.id).unwrap().time, note.time);
    }
    let mut replaced = (*chart).clone();
    replaced.lane_notes[Lane::Key1.index()][0].id = NoteId(9999);
    assert!(judge.note_by_id(&replaced, NoteId(9999)).is_some());
    assert!(judge.note_by_id(&chart, NoteId(9999)).is_none());
}

#[test]
fn hold_index_returns_the_original_hln_pair_position() {
    let mut chart = chart_with_long_start(TimeUs(0), TimeUs(1000));
    chart.long_notes[0].mode = Some(LongNoteMode::Ln);
    let mut hln = chart.long_notes[0].clone();
    hln.mode = Some(LongNoteMode::Hln);
    chart.long_notes.push(hln);
    let chart = std::sync::Arc::new(chart);
    let judge = JudgeEngine::new(windows()).with_chart_index(chart.clone());
    assert!(!judge.has_hcn(&chart));
    let pairs: Vec<_> = judge.hcn_pairs_at(&chart, TimeUs(500)).map(|(i, _)| i).collect();
    assert_eq!(pairs, [1]);
}

#[test]
fn indexed_hcn_handles_unsorted_pairs_and_per_pair_mode_override() {
    let mut chart = chart_with_long_start(TimeUs(500), TimeUs(1000));
    chart.metadata.long_note_mode = LongNoteMode::Hcn;
    let mut earlier = chart.long_notes[0].clone();
    earlier.start_time = TimeUs(0);
    earlier.end_time = TimeUs(200);
    earlier.start_note_id = NoteId(3);
    chart.long_notes.push(earlier.clone());
    earlier.mode = Some(LongNoteMode::Ln);
    chart.long_notes.push(earlier);
    let chart = std::sync::Arc::new(chart);
    let judge = JudgeEngine::new(windows()).with_chart_index(chart.clone());
    let passing: Vec<_> = judge
        .hcn_pairs_at(&chart, TimeUs(100))
        .filter(|(_, pair)| pair.start_time <= TimeUs(100) && TimeUs(100) < pair.end_time)
        .map(|(_, pair)| pair.start_note_id)
        .collect();
    assert_eq!(passing, vec![NoteId(3)]);
}

#[test]
fn normal_window_consumes_note() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(windows());

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_030_000)));

    assert!(outcome.consumed_input);
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].judge, Judge::Great);
    assert_eq!(outcome.events[0].side, TimingSide::Slow);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
}

#[test]
fn slow_empty_poor_does_not_consume_note() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(windows());

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

    assert!(!outcome.consumed_input);
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].judge, Judge::EmptyPoor);
    assert_eq!(outcome.events[0].side, TimingSide::Slow);
    assert_eq!(outcome.events[0].note_id, None);
    assert_eq!(
        outcome.keysounds,
        vec![KeySoundEvent {
            note_id: NoteId(1),
            time: TimeUs(1_150_000),
            trigger: KeySoundTrigger::NoteJudged,
        }]
    );
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
}

#[test]
fn fast_empty_poor_does_not_consume_note() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(windows());

    let outcome = engine.process_input(&chart, press_at(TimeUs(700_000)));

    assert!(!outcome.consumed_input);
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].judge, Judge::EmptyPoor);
    assert_eq!(outcome.events[0].side, TimingSide::Fast);
    assert_eq!(outcome.events[0].note_id, None);
    assert_eq!(
        outcome.keysounds,
        vec![KeySoundEvent {
            note_id: NoteId(1),
            time: TimeUs(700_000),
            trigger: KeySoundTrigger::NoteJudged,
        }]
    );
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
}

#[test]
fn outside_empty_poor_windows_is_unjudged() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(windows());

    let too_late = engine.process_input(&chart, press_at(TimeUs(1_250_000)));
    let too_early = engine.process_input(&chart, press_at(TimeUs(400_000)));

    assert!(too_late.events.is_empty());
    assert!(!too_late.consumed_input);
    assert!(too_early.events.is_empty());
    assert!(!too_early.consumed_input);
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
}

#[test]
fn double_press_after_normal_judge_is_slow_empty_poor() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(windows());

    let first = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let second = engine.process_input(&chart, press_at(TimeUs(1_005_000)));

    assert_eq!(first.events[0].judge, Judge::PGreat);
    assert_eq!(first.events[0].note_id, Some(NoteId(1)));
    assert!(!second.consumed_input);
    assert_eq!(second.events.len(), 1);
    assert_eq!(second.events[0].judge, Judge::EmptyPoor);
    assert_eq!(second.events[0].side, TimingSide::Slow);
    assert_eq!(second.events[0].note_id, None);
    assert_eq!(
        second.keysounds,
        vec![KeySoundEvent {
            note_id: NoteId(1),
            time: TimeUs(1_005_000),
            trigger: KeySoundTrigger::NoteJudged,
        }]
    );
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
}

#[test]
fn beatoraja_7k_double_press_after_slow_empty_poor_window_is_unjudged() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(
        crate::judge::window::beatoraja_note_judge_window_for_keymode(bmz_core::lane::KeyMode::K7),
    );

    let first = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let second = engine.process_input(&chart, press_at(TimeUs(1_151_000)));

    assert_eq!(first.events[0].judge, Judge::PGreat);
    assert!(second.events.is_empty());
    assert!(!second.consumed_input);
}

#[test]
fn beatoraja_7k_late_bad_is_not_missed_before_late_bad_end() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(
        crate::judge::window::beatoraja_note_judge_window_for_keymode(bmz_core::lane::KeyMode::K7),
    );

    let missed = engine.process_misses(&chart, TimeUs(1_260_000));
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_260_000)));

    assert!(missed.events.is_empty());
    assert!(outcome.consumed_input);
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert_eq!(outcome.events[0].side, TimingSide::Slow);
    assert_eq!(outcome.events[0].delta, TimeUs(260_000));
}

#[test]
fn beatoraja_7k_early_beyond_bad_is_fast_empty_poor() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new(
        crate::judge::window::beatoraja_note_judge_window_for_keymode(bmz_core::lane::KeyMode::K7),
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(740_000)));

    assert!(!outcome.consumed_input);
    assert_eq!(outcome.events[0].judge, Judge::EmptyPoor);
    assert_eq!(outcome.events[0].side, TimingSide::Fast);
    assert_eq!(outcome.events[0].delta, TimeUs(-260_000));
}

#[test]
fn beatoraja_pms_bad_does_not_consume_and_can_be_rejudged() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K9),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let bad = engine.process_input(&chart, press_at(TimeUs(820_000)));

    assert!(bad.consumed_input);
    assert_eq!(bad.events[0].judge, Judge::Bad);
    assert_eq!(bad.events[0].note_id, Some(NoteId(1)));
    assert!(engine.attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), None);

    let great = engine.process_input(&chart, press_at(TimeUs(1_050_000)));

    assert!(!engine.attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), Some(&Judge::Great));

    assert!(great.consumed_input);
    assert_eq!(great.events[0].judge, Judge::Great);
    assert_eq!(great.events[0].note_id, Some(NoteId(1)));
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
}

#[test]
fn beatoraja_pms_bad_attempt_miss_consumes_without_scoring_another_poor() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K9),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let bad = engine.process_input(&chart, press_at(TimeUs(820_000)));
    assert_eq!(bad.events[0].judge, Judge::Bad);
    assert!(engine.attempted_notes.contains(&NoteId(1)));

    let missed = engine.process_misses(&chart, TimeUs(1_184_000));

    assert!(!missed.events[0].affects_score);
    assert!(missed.events[0].advances_score);
    assert!(!engine.attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), Some(&Judge::Poor));
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
}

#[test]
fn dx_9key_bad_does_not_consume_and_can_be_rejudged() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let bad = engine.process_input(&chart, press_at(TimeUs(850_000)));
    assert_eq!(bad.events[0].judge, Judge::Bad);
    assert!(engine.attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), None);

    let pgreat = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    assert_eq!(pgreat.events[0].judge, Judge::PGreat);
    assert!(!engine.attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), Some(&Judge::PGreat));
}

#[test]
fn combo_candidate_prefers_later_combo_note_over_slow_bad() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_100_000));
    let mut engine = JudgeEngine::new(windows());

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));
    let missed = engine.process_misses(&chart, TimeUs(1_130_000));

    assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
    assert_eq!(outcome.events[0].judge, Judge::PGreat);
    assert_eq!(missed.events[0].note_id, Some(NoteId(1)));
    assert_eq!(missed.events[0].judge, Judge::Poor);
}

#[test]
fn duration_candidate_prefers_closest_note() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_040_000));
    let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
        JudgeWindows::uniform(windows()),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Duration,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_030_000)));

    assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
    assert_eq!(outcome.events[0].judge, Judge::PGreat);
    assert_eq!(outcome.events[0].delta, TimeUs(-10_000));
}

#[test]
fn duration_exact_hit_keeps_tie_order_and_falls_back_after_exact_notes() {
    let time = TimeUs(1_040_000);
    let mut chart = chart_with_two_taps(time, time);
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes.insert(0, NoteEvent { id: NoteId(3), time: TimeUs(1_010_000), ..notes[0].clone() });
    let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
        JudgeWindows::uniform(windows()),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Duration,
    );
    engine.attempted_notes.insert(NoteId(1));

    for (id, delta, judge) in [
        (NoteId(1), 0, Judge::PGreat),
        (NoteId(2), 0, Judge::PGreat),
        (NoteId(3), 30_000, Judge::Great),
    ] {
        let outcome = engine.process_input(&chart, press_at(time));
        let event = &outcome.events[0];
        assert_eq!((event.note_id, event.delta, event.judge), (Some(id), TimeUs(delta), judge));
    }
}

#[test]
fn lowest_candidate_keeps_first_note() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_040_000));
    let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
        JudgeWindows::uniform(windows()),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Lowest,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_030_000)));

    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Great);
    assert_eq!(outcome.events[0].delta, TimeUs(30_000));
}

#[test]
fn score_candidate_uses_great_threshold_instead_of_duration() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_150_000));
    let mut engine = JudgeEngine::new_with_window_set_and_algorithm(
        JudgeWindows::uniform(windows()),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Score,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));

    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert_eq!(outcome.events[0].delta, TimeUs(100_000));
}

#[test]
fn lr2oraja_multi_bad_adds_preceding_bad_before_selected_note() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_090_000));
    let mut engine = JudgeEngine::new_with_rule_mode(
        crate::judge::window::lr2oraja_note_judge_window(),
        RuleMode::Lr2Oraja,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

    assert!(outcome.consumed_input);
    assert_eq!(outcome.events.len(), 2);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert_eq!(outcome.events[0].delta, TimeUs(150_000));
    assert_eq!(outcome.events[1].note_id, Some(NoteId(2)));
    assert_eq!(outcome.events[1].judge, Judge::Great);
    assert_eq!(outcome.events[1].delta, TimeUs(60_000));
    assert_eq!(
        outcome.keysounds,
        vec![KeySoundEvent {
            note_id: NoteId(2),
            time: TimeUs(1_150_000),
            trigger: KeySoundTrigger::NoteJudged,
        }]
    );
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 2);
}

#[test]
fn dx_mode_adds_lr2oraja_multi_bad() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_090_000));
    let mut engine =
        JudgeEngine::new_with_rule_mode(crate::judge::window::dx_note_judge_window(), RuleMode::Dx);

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

    assert!(outcome.consumed_input);
    assert_eq!(outcome.events.len(), 2);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert_eq!(outcome.events[1].note_id, Some(NoteId(2)));
    assert_eq!(outcome.events[1].judge, Judge::Good);
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 2);
}

#[test]
fn dx_mode_fast_empty_poor_window_ends_at_500ms() {
    for lane in [Lane::Key1, Lane::Scratch] {
        let chart = chart_with_lane_tap(lane, TimeUs(1_000_000));

        let mut inside = JudgeEngine::new_with_window_set(
            crate::judge::window::dx_judge_windows(),
            RuleMode::Dx,
        );
        let outcome = inside.process_input(&chart, press_lane_at(lane, TimeUs(500_001)));
        assert_eq!(outcome.events.len(), 1, "{lane:?}");
        assert_eq!(outcome.events[0].judge, Judge::EmptyPoor);
        assert_eq!(outcome.events[0].side, TimingSide::Fast);
        assert!(!outcome.consumed_input);

        let mut outside = JudgeEngine::new_with_window_set(
            crate::judge::window::dx_judge_windows(),
            RuleMode::Dx,
        );
        let outcome = outside.process_input(&chart, press_lane_at(lane, TimeUs(500_000)));
        assert!(outcome.events.is_empty(), "{lane:?}");
        assert!(!outcome.consumed_input);
    }
}

#[test]
fn dx_9key_multi_bad_does_not_consume_the_bad_note() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_110_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

    assert_eq!(outcome.events.len(), 2);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert!(engine.attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), None);
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Great));

    let missed = engine.process_misses(&chart, TimeUs(1_183_334));
    assert!(!missed.events[0].affects_score);
    assert!(missed.events[0].advances_score);
    assert_eq!(engine.judged_notes.get(&NoteId(1)), Some(&Judge::Poor));
}

#[test]
fn beatoraja_mode_does_not_add_lr2oraja_multi_bad() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_090_000));
    let mut engine = JudgeEngine::new_with_rule_mode(
        crate::judge::window::lr2oraja_note_judge_window(),
        RuleMode::Beatoraja,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_150_000)));

    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
    assert_eq!(outcome.events[0].judge, Judge::Great);
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 0);
}

#[test]
fn lr2oraja_multi_bad_keeps_following_bad_when_selected_note_is_bad() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_260_000));
    let mut engine = JudgeEngine::new_with_rule_mode(
        crate::judge::window::lr2oraja_note_judge_window(),
        RuleMode::Lr2Oraja,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_130_000)));

    assert!(outcome.consumed_input);
    assert_eq!(outcome.events.len(), 2);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(2)));
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert_eq!(outcome.events[0].delta, TimeUs(-130_000));
    assert_eq!(outcome.events[1].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[1].judge, Judge::Bad);
    assert_eq!(outcome.events[1].delta, TimeUs(130_000));
    assert_eq!(
        outcome.keysounds,
        vec![KeySoundEvent {
            note_id: NoteId(1),
            time: TimeUs(1_130_000),
            trigger: KeySoundTrigger::NoteJudged,
        }]
    );
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 2);
}

#[test]
fn scratch_press_uses_scratch_window() {
    let chart = chart_with_lane_tap(Lane::Scratch, TimeUs(1_000_000));
    let mut engine = JudgeEngine::new_with_window_set(
        crate::judge::window::beatoraja_judge_windows_for_keymode(bmz_core::lane::KeyMode::K7),
        RuleMode::Beatoraja,
    );

    let outcome = engine.process_input(&chart, press_lane_at(Lane::Scratch, TimeUs(1_065_000)));

    assert_eq!(outcome.events[0].judge, Judge::Great);
    assert_eq!(outcome.events[0].side, TimingSide::Slow);
}

#[test]
fn cn_release_uses_long_note_end_window() {
    let mut window_set = JudgeWindows::uniform(windows());
    window_set.long_note_end = JudgeWindow::symmetric(120_000, 160_000, 200_000, 220_000, 0, 0);
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new_with_window_set(window_set, RuleMode::Beatoraja);

    let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let release = engine.process_input(&chart, release_at(TimeUs(2_150_000)));

    assert_eq!(press.events[0].judge, Judge::PGreat);
    assert_eq!(release.events[0].judge, Judge::Great);
}

#[test]
fn opposite_scratch_direction_finishes_bss() {
    let mut chart = chart_with_lane_long_start(Lane::Scratch, TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new_with_window_set(
        crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K7),
        RuleMode::Beatoraja,
    );
    let mut start = press_lane_at(Lane::Scratch, TimeUs(1_000_000));
    start.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
    let mut end = press_lane_at(Lane::Scratch, TimeUs(2_000_000));
    end.scratch_direction = Some(bmz_core::input::ScratchDirection::Up);

    let start_outcome = engine.process_input(&chart, start);
    let end_outcome = engine.process_input(&chart, end);

    assert_eq!(start_outcome.events[0].judge, Judge::PGreat);
    assert_eq!(end_outcome.events[0].note_id, Some(NoteId(2)));
    assert_eq!(end_outcome.events[0].judge, Judge::PGreat);
    assert!(engine.lanes[Lane::Scratch.index()].active_long.is_none());
}

#[test]
fn bss_release_suppresses_immediate_reverse_before_following_pgreat_note() {
    for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
        let mut chart =
            chart_with_lane_long_start(Lane::Scratch, TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(mode);
        chart.lane_notes[Lane::Scratch.index()].push(NoteEvent {
            id: NoteId(3),
            lane: Lane::Scratch,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time: TimeUs(2_020_000),
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        });
        chart.total_notes = 3;
        chart.end_time = TimeUs(2_020_000);
        let mut engine = JudgeEngine::new_with_window_set(
            crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K7),
            RuleMode::Beatoraja,
        );
        let mut start = press_lane_at(Lane::Scratch, TimeUs(1_000_000));
        start.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
        let mut release = release_lane_at(Lane::Scratch, TimeUs(2_000_000));
        release.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
        let mut reverse = press_lane_at(Lane::Scratch, TimeUs(2_001_000));
        reverse.scratch_direction = Some(bmz_core::input::ScratchDirection::Up);
        let mut next = press_lane_at(Lane::Scratch, TimeUs(2_020_000));
        next.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);

        engine.process_input(&chart, start);
        let release_outcome = engine.process_input(&chart, release);
        assert_eq!(release_outcome.events.len(), 1);
        assert_eq!(release_outcome.events[0].note_id, Some(NoteId(2)));
        assert_eq!(release_outcome.events[0].judge, Judge::PGreat);
        assert!(engine.lanes[Lane::Scratch.index()].active_long.is_none());

        let reverse_outcome = engine.process_input(&chart, reverse);
        assert!(reverse_outcome.events.is_empty());
        assert!(reverse_outcome.consumed_input);
        assert_eq!(engine.judged_notes.get(&NoteId(3)), None);

        let next_outcome = engine.process_input(&chart, next);
        assert_eq!(next_outcome.events.len(), 1);
        assert_eq!(next_outcome.events[0].note_id, Some(NoteId(3)));
        assert_eq!(next_outcome.events[0].judge, Judge::PGreat);
    }
}

#[test]
fn bss_reverse_press_suppression_expires_after_thirty_milliseconds() {
    for (delay_us, suppressed) in [(30_000, true), (30_001, false)] {
        let reverse_time = TimeUs(2_000_000 + delay_us);
        let mut chart =
            chart_with_lane_long_start(Lane::Scratch, TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(LongNoteMode::Cn);
        chart.lane_notes[Lane::Scratch.index()].push(NoteEvent {
            id: NoteId(3),
            lane: Lane::Scratch,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time: reverse_time,
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        });
        let mut engine = JudgeEngine::new_with_window_set(
            crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K7),
            RuleMode::Beatoraja,
        );
        let mut start = press_lane_at(Lane::Scratch, TimeUs(1_000_000));
        start.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
        let mut release = release_lane_at(Lane::Scratch, TimeUs(2_000_000));
        release.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
        let mut reverse = press_lane_at(Lane::Scratch, reverse_time);
        reverse.scratch_direction = Some(bmz_core::input::ScratchDirection::Up);

        engine.process_input(&chart, start);
        engine.process_input(&chart, release);
        let reverse_outcome = engine.process_input(&chart, reverse);

        assert!(reverse_outcome.consumed_input);
        assert_eq!(reverse_outcome.events.is_empty(), suppressed);
        assert_eq!(!engine.judged_notes.contains_key(&NoteId(3)), suppressed);
        if suppressed {
            let after = engine
                .process_input(&chart, InputEvent { time: TimeUs(reverse_time.0 + 1), ..reverse });
            assert_eq!(after.events[0].note_id, Some(NoteId(3)));
        }
    }
}

#[test]
fn bss_early_same_direction_release_still_fails() {
    for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
        let mut chart =
            chart_with_lane_long_start(Lane::Scratch, TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(mode);
        let mut engine = JudgeEngine::new_with_window_set(
            crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K7),
            RuleMode::Beatoraja,
        );
        let mut start = press_lane_at(Lane::Scratch, TimeUs(1_000_000));
        start.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
        let mut release = release_lane_at(Lane::Scratch, TimeUs(1_700_000));
        release.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);

        engine.process_input(&chart, start);
        let release_outcome = engine.process_input(&chart, release);

        assert_eq!(release_outcome.events.len(), 1);
        assert_eq!(release_outcome.events[0].note_id, Some(NoteId(2)));
        assert_eq!(release_outcome.events[0].judge, Judge::Poor);
        assert!(engine.lanes[Lane::Scratch.index()].active_long.is_none());
        assert!(engine.lanes[Lane::Scratch.index()].scratch_press_suppression.is_none());
    }
}

#[test]
fn releasing_inactive_scratch_direction_keeps_bss_held() {
    let mut chart = chart_with_lane_long_start(Lane::Scratch, TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new(windows());
    let mut start = press_lane_at(Lane::Scratch, TimeUs(1_000_000));
    start.scratch_direction = Some(bmz_core::input::ScratchDirection::Down);
    let mut wrong_release = release_lane_at(Lane::Scratch, TimeUs(1_500_000));
    wrong_release.scratch_direction = Some(bmz_core::input::ScratchDirection::Up);

    engine.process_input(&chart, start);
    let outcome = engine.process_input(&chart, wrong_release);

    assert!(outcome.events.is_empty());
    assert!(engine.lanes[Lane::Scratch.index()].active_long.is_some());
}

#[test]
fn dx_9key_ln_early_bad_release_can_be_cancelled_during_margin() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    assert_eq!(press.events[0].judge, Judge::PGreat);

    let release = engine.process_input(&chart, release_at(TimeUs(1_750_000)));
    assert!(release.events.is_empty());
    assert!(
        engine.lanes[Lane::Key1.index()]
            .active_long
            .is_some_and(|active| active.pending_release.is_some())
    );

    let before_margin = engine.process_misses(&chart, TimeUs(1_866_666));
    assert!(before_margin.events.is_empty());
    let repress = engine.process_input(&chart, press_at(TimeUs(1_866_667)));
    assert!(repress.events.is_empty());
    assert!(repress.consumed_input);

    let end = engine.process_misses(&chart, TimeUs(2_000_001));
    assert_eq!(end.events.len(), 1);
    assert_eq!(end.events[0].note_id, Some(NoteId(1)));
    assert_eq!(end.events[0].judge, Judge::PGreat);
}

#[test]
fn dx_9key_cn_early_bad_release_finalizes_after_margin() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let release = engine.process_input(&chart, release_at(TimeUs(1_750_000)));
    assert!(release.events.is_empty());

    let finalized = engine.process_misses(&chart, TimeUs(1_866_667));
    assert_eq!(finalized.events.len(), 1);
    assert_eq!(finalized.events[0].note_id, Some(NoteId(2)));
    assert_eq!(finalized.events[0].judge, Judge::Bad);
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Bad));
}

fn dx_pop_engine() -> JudgeEngine {
    JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    )
}

#[test]
fn dx_pop_cn_release_boundaries_widen_only_the_early_side() {
    // 参照側 JudgeProperty.POP の LN 終端は PG/GR/GD が {-116667, 233333}、BD が
    // {-183333, 366667} ({LATE, EARLY})。BMZ の差分は「離し時刻 - 終端」なので EARLY が負。
    // BD 以下の早離しは longnoteMargin (116667) の間だけ保留され、その後に確定する。
    for (offset_us, expected, pending) in [
        (-233_333, Judge::PGreat, false),
        (-233_334, Judge::Bad, true),
        (-366_667, Judge::Bad, true),
        (-366_668, Judge::Poor, true),
        (116_667, Judge::PGreat, false),
        (116_668, Judge::Bad, false),
        (183_333, Judge::Bad, false),
        (183_334, Judge::Poor, false),
    ] {
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.long_notes[0].mode = Some(LongNoteMode::Cn);
        let mut engine = dx_pop_engine();
        engine.process_input(&chart, press_at(TimeUs(1_000_000)));

        let released_at = 2_000_000 + offset_us;
        let release = engine.process_input(&chart, release_at(TimeUs(released_at)));
        let events = if pending {
            assert!(release.events.is_empty(), "{offset_us}");
            let before = engine.process_misses(&chart, TimeUs(released_at + 116_666));
            assert!(before.events.is_empty(), "{offset_us}");
            engine.process_misses(&chart, TimeUs(released_at + 116_667)).events
        } else {
            release.events
        };

        assert_eq!(events.len(), 1, "{offset_us}");
        assert_eq!(events[0].note_id, Some(NoteId(2)), "{offset_us}");
        assert_eq!(events[0].judge, expected, "{offset_us}");
    }
}

#[test]
fn dx_pop_ln_early_release_boundary_uses_widened_window() {
    for (offset_us, pending) in [(-233_333, false), (-233_334, true)] {
        let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        let mut engine = dx_pop_engine();
        engine.process_input(&chart, press_at(TimeUs(1_000_000)));

        let release = engine.process_input(&chart, release_at(TimeUs(2_000_000 + offset_us)));

        let active = engine.lanes[Lane::Key1.index()].active_long;
        if pending {
            assert!(release.events.is_empty());
            assert!(active.is_some_and(|active| active.pending_release.is_some()));
        } else {
            assert!(active.is_none());
            assert_eq!(release.events.len(), 1);
            assert_eq!(release.events[0].note_id, Some(NoteId(1)));
            assert_eq!(release.events[0].judge, Judge::PGreat);
        }
    }
}

#[test]
fn missed_cn_head_marks_both_head_and_tail_poor() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new(windows());

    let missed = engine.process_misses(&chart, TimeUs(1_120_001));

    assert_eq!(missed.events.len(), 2);
    assert_eq!(missed.events[0].note_id, Some(NoteId(1)));
    assert_eq!(missed.events[1].note_id, Some(NoteId(2)));
    assert!(missed.events.iter().all(|event| event.judge == Judge::Poor));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), Some(&Judge::Poor));
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
}

#[test]
fn held_cn_tail_miss_uses_normal_note_bad_window() {
    let mut window_set = JudgeWindows::uniform(windows());
    window_set.long_note_end = JudgeWindow::symmetric(120_000, 160_000, 200_000, 220_000, 0, 0);
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new_with_window_set(window_set, RuleMode::Beatoraja);

    engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let inside_note_bad = engine.process_misses(&chart, TimeUs(2_120_000));
    assert!(inside_note_bad.events.is_empty());
    let missed = engine.process_misses(&chart, TimeUs(2_120_001));
    assert_eq!(missed.events.len(), 1);
    assert_eq!(missed.events[0].note_id, Some(NoteId(2)));
    assert_eq!(missed.events[0].judge, Judge::Poor);
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
}

#[test]
fn lr2oraja_derived_modes_suppress_late_bad_on_long_note_start() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let input = press_at(TimeUs(1_100_000));

    let mut beatoraja = JudgeEngine::new(windows());
    let beatoraja_outcome = beatoraja.process_input(&chart, input);
    assert_eq!(beatoraja_outcome.events[0].judge, Judge::Bad);
    assert_eq!(beatoraja.lanes[Lane::Key1.index()].next_note_index, 2);

    let mut lr2oraja = JudgeEngine::new_with_rule_mode(windows(), RuleMode::Lr2Oraja);
    let lr2oraja_outcome = lr2oraja.process_input(&chart, input);
    assert!(lr2oraja_outcome.events.is_empty());
    assert!(!lr2oraja_outcome.consumed_input);
    assert_eq!(lr2oraja.lanes[Lane::Key1.index()].next_note_index, 0);

    let mut dx = JudgeEngine::new_with_rule_mode(windows(), RuleMode::Dx);
    let dx_outcome = dx.process_input(&chart, input);
    assert!(dx_outcome.events.is_empty());
    assert!(!dx_outcome.consumed_input);
    assert_eq!(dx.lanes[Lane::Key1.index()].next_note_index, 0);
}

#[test]
fn defined_cn_pair_judges_release_even_when_chart_default_is_ln() {
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.long_notes[0].mode = Some(LongNoteMode::Cn);
    let mut engine = JudgeEngine::new(windows());

    let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let release = engine.process_input(&chart, release_at(TimeUs(2_000_000)));

    assert_eq!(press.events[0].judge, Judge::PGreat);
    assert_eq!(release.events.len(), 1);
    assert_eq!(release.events[0].note_id, Some(NoteId(2)));
    assert_eq!(release.events[0].judge, Judge::PGreat);
}

#[test]
fn ln_start_defers_scoring_until_end() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let mut engine = JudgeEngine::new(windows());

    let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let end = engine.process_misses(&chart, TimeUs(2_000_001));

    assert_eq!(press.events[0].note_id, Some(NoteId(1)));
    assert_eq!(press.events[0].judge, Judge::PGreat);
    assert!(!press.events[0].affects_score);
    assert_eq!(end.events[0].note_id, Some(NoteId(1)));
    assert_eq!(end.events[0].judge, Judge::PGreat);
    assert!(end.events[0].affects_score);
}

#[test]
fn ln_early_release_scores_once_with_combined_judge() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let mut engine = JudgeEngine::new(windows());

    let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let release = engine.process_input(&chart, release_at(TimeUs(1_900_000)));

    assert!(!press.events[0].affects_score);
    assert_eq!(release.events[0].note_id, Some(NoteId(1)));
    assert_eq!(release.events[0].judge, Judge::Bad);
    assert_eq!(release.events[0].side, TimingSide::Fast);
    assert_eq!(release.events[0].delta, TimeUs(-100_000));
    assert!(release.events[0].affects_score);
}

#[test]
fn defined_hcn_pair_judges_early_release_even_when_chart_default_is_ln() {
    // 早離し後の減衰は judge engine ではなく session 側の passing ベース
    // (update_hcn_lane_timers / apply_hcn_gauge) で処理される。
    let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.long_notes[0].mode = Some(LongNoteMode::Hcn);
    let mut engine = JudgeEngine::new(windows());

    let press = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let release = engine.process_input(&chart, release_at(TimeUs(1_500_000)));

    assert_eq!(press.events[0].judge, Judge::PGreat);
    assert_eq!(release.events[0].note_id, Some(NoteId(2)));
    assert_eq!(release.events[0].judge, Judge::Poor);
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
}

#[test]
fn legacy_tap_bad_consumes_the_note_and_continues_with_the_same_press() {
    let chart = chart_with_two_taps(TimeUs(1_000_900), TimeUs(1_161_900));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_179_001)));
    let grades: Vec<_> = outcome.events.iter().map(|event| (event.note_id, event.judge)).collect();
    assert_eq!(grades, vec![(Some(NoteId(1)), Judge::Bad), (Some(NoteId(2)), Judge::PGreat)]);
    assert_eq!(outcome.events[1].delta, TimeUs(18_000));
}

#[test]
fn legacy_empty_poor_does_not_consume_and_excludes_exact_early_boundary() {
    let chart = chart_with_tap(TimeUs(2_000_000));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    assert!(engine.process_input(&chart, press_at(TimeUs(1_000_000))).events.is_empty());
    let early = engine.process_input(&chart, press_at(TimeUs(1_001_000)));
    assert_eq!(early.events[0].note_id, None);
    assert_eq!(early.events[0].judge, Judge::EmptyPoor);
    let on_time = engine.process_input(&chart, press_at(TimeUs(2_000_000)));
    assert_eq!(on_time.events[0].judge, Judge::PGreat);
}

/// 実際の OpenLR2 (RianMod) が記録した判定との照合 (`Air -GOD-`)。
///
/// `air7god.bme` (title `Air -GOD-`, `#RANK 3`, BPM 180, LN なし, 3414 notes) を
/// OpenLR2_RianMod_x64.exe で RANDOM を掛けてプレイしたときの
/// `LR2files/Replay/lightning_23k/751738dea1169c5c39db935adfc9e85f.lr2rep` には、
/// 確定した判定が op 210 (P1) の value として 3418 件残っている
/// (`Scene04_Play.cpp` の `ApplyJudgeNote` が `AddReplayData` で書き出す)。
/// 同じプレイのスコア DB 集計は perfect 3106 / great 261 / good 22 / bad 10 / poor 19
/// (totalnotes 3414, clear 4) で、リプレイの 3418 件 (poor 19 = POOR 15 + 空 POOR 4) と一致する。
///
/// ここでは OpenLR2 が実際に使った譜面時刻と押下時刻をそのまま LEGACY エンジンへ与え、
/// 3418 件の判定値が記録された順序まで含めて一致することを確認する。
/// フィクスチャは `tests/data/air_god_*.csv`:
///
/// - `air_god_notes.csv`: `譜面レーン,ノート時刻ms`。BMS の `#xxx16` `#xxx11`..`#xxx19` を
///   小節位置に直し、OpenLR2 の `realTiming` 逐次加算
///   (`240.0 / BPM * meaLength * (bmsTiming - prev) * 1000.0`, `meaLength = 1.0`) で求めた値を
///   `(int)note.realTiming` と同じく切り捨てたもの。
/// - `air_god_presses.csv`: `入力レーン,押下時刻ms`。リプレイの op 0-7 の value=1。
/// - `air_god_judgements.csv`: `時刻ms,判定`。リプレイの op 210。5=PGREAT 4=GREAT
///   3=GOOD 2=BAD 1=POOR 0=空 POOR (`JudgeToScore` の引数)。
///
/// 3414 notes / 3400 押下 / 3418 判定と量が多いので、テスト本体ではなく CSV に分けている。
///
/// このプレイは RANDOM なので、リプレイの入力レーン (op) と譜面レーンは一致しない。
/// 押下回数と譜面ノート数の対応、および最近傍ノートとの差が ±21ms 以内になる割合から
/// `op 0..7 -> 譜面レーン {0, 4, 1, 5, 2, 6, 7, 3}` と確定できる
/// (例: op 2 の押下 564 回のうち 520 回が譜面 Key1 のノートから ±21ms 以内)。
#[test]
fn legacy_matches_openlr2_recorded_judgement_of_air_god() {
    /// 譜面レーン番号 (BMS の scratch, key1..key7) -> `Lane`。
    const LANE_BY_INDEX: [Lane; 8] = [
        Lane::Scratch,
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
    ];
    /// リプレイの入力レーン (op 0..7) -> 譜面レーン。RANDOM で決まった対応。
    const RANDOM_LANE: [Lane; 8] = [
        Lane::Scratch,
        Lane::Key4,
        Lane::Key1,
        Lane::Key5,
        Lane::Key2,
        Lane::Key6,
        Lane::Key7,
        Lane::Key3,
    ];

    const NOTES_CSV: &str = include_str!("data/air_god_notes.csv");
    const PRESSES_CSV: &str = include_str!("data/air_god_presses.csv");
    const JUDGEMENTS_CSV: &str = include_str!("data/air_god_judgements.csv");

    fn parse_rows(csv: &str) -> Vec<(usize, i64)> {
        csv.lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .map(|line| {
                let (first, second) = line.split_once(',').expect("csv row");
                (
                    first.trim().parse().expect("first column"),
                    second.trim().parse().expect("second column"),
                )
            })
            .collect()
    }

    let notes = parse_rows(NOTES_CSV);
    let presses = parse_rows(PRESSES_CSV);
    let expected = parse_rows(JUDGEMENTS_CSV);

    assert_eq!(notes.len(), 3414);
    assert_eq!(presses.len(), 3400);
    assert_eq!(expected.len(), 3418);

    // 譜面。CSV は譜面レーン順に並んでいるので、ノート ID もレーンごとに昇順になる。
    let mut chart = chart_with_lane_tap(Lane::Scratch, TimeUs(0));
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    let mut end_time = 0;
    for (index, &(lane_index, ms)) in notes.iter().enumerate() {
        let lane = LANE_BY_INDEX[lane_index];
        chart.lane_notes[lane.index()].push(NoteEvent {
            id: NoteId(index as u32 + 1),
            lane,
            kind: NoteKind::Tap,
            tick: Default::default(),
            time: TimeUs(ms * 1_000),
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        });
        end_time = end_time.max(ms * 1_000);
    }
    chart.total_notes = notes.len() as u32;
    chart.end_time = TimeUs(end_time);

    let window = crate::judge::window::judge_window_for_rule_mode(
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy),
        100, // #RANK 3 -> 21 / 60 / 120ms, BAD 200ms, 空 POOR 1000ms
        RuleMode::Legacy,
    );
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);

    // 実プレイと同じく、そのフレームの入力を先に処理し、その後で全レーンの
    // 取りこぼしを確定する。入力レーン内の期限切れは process_input 側が
    // 先に確定する (OpenLR2 の ProcSinglenote と同じ順序)。
    let mut timeline: Vec<i64> = presses.iter().map(|&(_, ms)| ms).collect();
    timeline.extend(notes.iter().map(|&(_, ms)| ms + 201));
    timeline.sort_unstable();
    timeline.dedup();

    let mut press_cursor = 0;
    let mut judged = Vec::new();
    for &ms in &timeline {
        let now = TimeUs(ms * 1_000);
        while let Some(&(op, press_ms)) = presses.get(press_cursor) {
            if press_ms != ms {
                break;
            }
            // 同じフレームの押下は OpenLR2 と同じく入力レーン順に処理する。
            assert!(op < RANDOM_LANE.len(), "op {op} is not a P1 input lane");
            let outcome = engine.process_input(&chart, press_lane_at(RANDOM_LANE[op], now));
            judged.extend(outcome.events.iter().map(|e| e.judge));
            press_cursor += 1;
        }
        judged.extend(engine.process_misses(&chart, now).events.iter().map(|e| e.judge));
    }
    assert_eq!(press_cursor, presses.len());

    // 3414 notes がすべて 1 回ずつ消費され、取りこぼしも残らない。
    assert!(engine.process_misses(&chart, TimeUs(1_000_000_000)).events.is_empty());

    let judge_value = |judge: Judge| match judge {
        Judge::PGreat => 5,
        Judge::Great => 4,
        Judge::Good => 3,
        Judge::Bad => 2,
        Judge::Poor => 1,
        Judge::EmptyPoor => 0,
    };
    let judged_values: Vec<i64> = judged.iter().map(|&judge| judge_value(judge)).collect();
    let expected_values: Vec<i64> = expected.iter().map(|&(_, value)| value).collect();
    assert_eq!(judged_values.len(), expected_values.len());
    for (index, (got, want)) in judged_values.iter().zip(&expected_values).enumerate() {
        assert_eq!(got, want, "judgement #{index} of the recorded play");
    }

    let count = |want: Judge| judged.iter().filter(|&&judge| judge == want).count();
    assert_eq!(
        (
            count(Judge::PGreat),
            count(Judge::Great),
            count(Judge::Good),
            count(Judge::Bad),
            count(Judge::Poor),
            count(Judge::EmptyPoor),
        ),
        (3106, 261, 22, 10, 15, 4)
    );
}

#[test]
fn legacy_idle_lane_consumes_only_one_expired_note_per_poll() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_001_000));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let first = engine.process_legacy_idle_lane(&chart, Lane::Key1, TimeUs(1_500_000), false);
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].note_id, Some(NoteId(1)));
    let second = engine.process_legacy_idle_lane(&chart, Lane::Key1, TimeUs(1_501_000), false);
    assert_eq!(second.events.len(), 1);
    assert_eq!(second.events[0].note_id, Some(NoteId(2)));
}

#[test]
fn legacy_active_ln_repress_overwrites_the_provisional_head_grade() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let head = engine.process_input(&chart, press_at(TimeUs(1_040_000)));
    assert!(head.events.iter().all(|event| !event.affects_score));
    let tail = engine.process_input(&chart, release_at(TimeUs(2_000_000)));
    assert_eq!(tail.events.len(), 1);
    assert_eq!(tail.events[0].judge, Judge::Great);
}

#[test]
fn legacy_late_repress_of_an_active_ln_scores_empty_poor() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    let extra = engine.process_input(&chart, press_at(TimeUs(1_500_000)));
    assert_eq!(extra.events[0].judge, Judge::EmptyPoor);
    let tail = engine.process_input(&chart, release_at(TimeUs(2_000_000)));
    assert_eq!(tail.events[0].judge, Judge::PGreat);
}

#[test]
fn legacy_release_poll_settles_an_expired_head_before_handling_the_edge() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let outcome = engine.process_input(&chart, release_at(TimeUs(1_201_000)));
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].judge, Judge::Poor);
    assert!(outcome.consumed_input);
}

#[test]
fn legacy_ln_release_preserves_head_grade_inside_the_good_window() {
    let chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    for (release_ms, expected) in [(1_900, Judge::PGreat), (1_899, Judge::Bad)] {
        let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
        engine.process_input(&chart, press_at(TimeUs(1_000_000)));
        let outcome = engine.process_input(&chart, release_at(TimeUs(release_ms * 1_000)));
        assert_eq!(outcome.events[0].judge, expected);
    }
}

#[test]
fn legacy_press_after_an_expired_head_consumes_only_one_poor() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_201_100));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_201_500)));
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Poor);
    assert!(outcome.consumed_input);

    // The following note remains available for a separate press.
    let next = engine.process_input(&chart, press_at(TimeUs(1_202_500)));
    assert_eq!(next.events.len(), 1);
    assert_eq!(next.events[0].note_id, Some(NoteId(2)));
    assert_eq!(next.events[0].judge, Judge::PGreat);

    // Several expired notes still consume only one note per press invocation.
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_001_000));
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);
    let outcome = engine.process_input(&chart, press_at(TimeUs(1_300_000)));
    assert_eq!(outcome.events.len(), 1);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    let next = engine.process_input(&chart, press_at(TimeUs(1_301_000)));
    assert_eq!(next.events.len(), 1);
    assert_eq!(next.events[0].note_id, Some(NoteId(2)));
    assert_eq!(next.events[0].judge, Judge::Poor);
}

#[test]
fn legacy_ln_early_bad_mutes_the_start_keysound_for_any_head_offset() {
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    for head_ms in [980_i64, 1_000, 1_020] {
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.lane_notes[Lane::Key1.index()][0].sound = Some(bmz_core::ids::SoundId(7));
        let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);

        engine.process_input(&chart, press_at(TimeUs(head_ms * 1_000)));
        // #RANK 2 の GOOD 窓 (100ms) を超えて早く離すと最終判定は BAD になる。
        let outcome = engine.process_input(&chart, release_at(TimeUs(1_899_000)));

        assert_eq!(outcome.events[0].judge, Judge::Bad, "head {head_ms}ms");
        assert_eq!(outcome.events[0].delta, TimeUs((head_ms - 1_000) * 1_000), "head {head_ms}ms");
        assert_eq!(
            outcome.keysound_volumes,
            vec![(bmz_core::ids::SoundId(7), 0.0)],
            "head {head_ms}ms"
        );
    }
}

#[test]
fn legacy_press_that_settles_an_expired_head_plays_its_miss_keysound_when_enabled() {
    let mut chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_201_100));
    chart.lane_notes[Lane::Key1.index()][0].sound = Some(bmz_core::ids::SoundId(7));
    let window =
        crate::judge::window::note_judge_window_for_rule_mode(KeyMode::K7, RuleMode::Legacy);
    let mut engine = JudgeEngine::new_with_rule_mode(window, RuleMode::Legacy);

    let outcome =
        engine.process_input_at(&chart, press_at(TimeUs(1_201_500)), TimeUs(1_201_500), true);

    assert_eq!(outcome.keysounds[0].note_id, NoteId(1));
    assert_eq!(outcome.keysounds[0].trigger, KeySoundTrigger::Miss);
}
