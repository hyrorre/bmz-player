use super::*;

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
    assert!(engine.bad_attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), None);

    let great = engine.process_input(&chart, press_at(TimeUs(1_050_000)));

    assert!(!engine.bad_attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), Some(&Judge::Great));

    assert!(great.consumed_input);
    assert_eq!(great.events[0].judge, Judge::Great);
    assert_eq!(great.events[0].note_id, Some(NoteId(1)));
    assert_eq!(engine.lanes[Lane::Key1.index()].next_note_index, 1);
}

#[test]
fn beatoraja_pms_bad_attempt_miss_consumes_without_extra_poor_event() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K9),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let bad = engine.process_input(&chart, press_at(TimeUs(820_000)));
    assert_eq!(bad.events[0].judge, Judge::Bad);
    assert!(engine.bad_attempted_notes.contains(&NoteId(1)));

    let missed = engine.process_misses(&chart, TimeUs(1_184_000));

    assert!(missed.events.is_empty());
    assert!(!engine.bad_attempted_notes.contains(&NoteId(1)));
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

    let bad = engine.process_input(&chart, press_at(TimeUs(900_000)));
    assert_eq!(bad.events[0].judge, Judge::Bad);
    assert!(engine.bad_attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), None);

    let pgreat = engine.process_input(&chart, press_at(TimeUs(1_000_000)));
    assert_eq!(pgreat.events[0].judge, Judge::PGreat);
    assert!(!engine.bad_attempted_notes.contains(&NoteId(1)));
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
fn dx_9key_multi_bad_does_not_consume_the_bad_note() {
    let chart = chart_with_two_taps(TimeUs(1_000_000), TimeUs(1_050_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );

    let outcome = engine.process_input(&chart, press_at(TimeUs(1_100_000)));

    assert_eq!(outcome.events.len(), 2);
    assert_eq!(outcome.events[0].note_id, Some(NoteId(1)));
    assert_eq!(outcome.events[0].judge, Judge::Bad);
    assert!(engine.bad_attempted_notes.contains(&NoteId(1)));
    assert_eq!(engine.judged_notes.get(&NoteId(1)), None);
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Great));

    let missed = engine.process_misses(&chart, TimeUs(1_100_001));
    assert!(missed.events.is_empty());
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
    window_set.long_note_end =
        JudgeWindow::symmetric(120_000, 160_000, 200_000, 220_000, 0, 0, 16_000);
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

    let before_margin = engine.process_misses(&chart, TimeUs(1_900_000));
    assert!(before_margin.events.is_empty());
    let repress = engine.process_input(&chart, press_at(TimeUs(1_940_000)));
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

    let finalized = engine.process_misses(&chart, TimeUs(1_950_000));
    assert_eq!(finalized.events.len(), 1);
    assert_eq!(finalized.events[0].note_id, Some(NoteId(2)));
    assert_eq!(finalized.events[0].judge, Judge::Bad);
    assert_eq!(engine.judged_notes.get(&NoteId(2)), Some(&Judge::Bad));
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
    window_set.long_note_end =
        JudgeWindow::symmetric(120_000, 160_000, 200_000, 220_000, 0, 0, 16_000);
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
