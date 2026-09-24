use super::*;

fn chart() -> PlayableChart {
    let mut chart = chart_with_long_start(TimeUs(0), TimeUs(1_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Hln);
    chart
}

#[test]
fn hln_overlapping_bodies_keep_independent_pending_scores_and_counters() {
    let mut chart = chart();
    let mut second = chart.long_notes[0].clone();
    second.start_note_id = NoteId(3);
    second.end_note_id = NoteId(4);
    second.start_time = TimeUs(500_000);
    second.end_time = TimeUs(1_500_000);
    let mut head = chart.lane_notes[Lane::Key1.index()][0].clone();
    head.id = NoteId(3);
    head.time = second.start_time;
    let mut tail = chart.lane_notes[Lane::Key1.index()][1].clone();
    tail.id = NoteId(4);
    tail.time = second.end_time;
    chart.lane_notes[Lane::Key1.index()].extend([head, tail]);
    chart.lane_notes[Lane::Key1.index()].sort_by_key(|note| note.time);
    chart.long_notes.push(second);
    let mut engine = JudgeEngine::new(windows());
    engine.process_input(&chart, press_at(TimeUs(0)));
    let second_head = engine.process_input(&chart, press_at(TimeUs(500_000)));
    assert_eq!(second_head.events.len(), 1);
    assert!(!second_head.events[0].affects_score);
    let first_end = engine.process_misses(&chart, TimeUs(1_000_000));
    assert_eq!(first_end.events[0].note_id, Some(NoteId(1)));
    let released = engine.process_input(&chart, release_at(TimeUs(1_450_000)));
    assert_eq!(released.events.len(), 1);
    assert_eq!(released.events[0].note_id, Some(NoteId(3)));
    assert_eq!(released.events[0].judge, Judge::Good);
}

#[test]
fn hln_scratch_reverse_has_no_bss_judgement_or_suppression() {
    let mut chart = chart_with_lane_long_start(Lane::Scratch, TimeUs(0), TimeUs(1_000_000));
    chart.long_notes[0].mode = Some(LongNoteMode::Hln);
    let mut engine = JudgeEngine::new(windows());
    let mut down = press_lane_at(Lane::Scratch, TimeUs(0));
    down.scratch_direction = Some(ScratchDirection::Down);
    engine.process_input(&chart, down);
    let mut up = down;
    up.time = TimeUs(500_000);
    up.scratch_direction = Some(ScratchDirection::Up);
    assert!(engine.process_input(&chart, up).events.is_empty());
    let mut release = down;
    release.kind = InputKind::Release;
    release.time = TimeUs(600_000);
    let out = engine.process_input(&chart, release);
    assert_eq!(out.events[0].judge, Judge::Bad);
    assert!(engine.hln_body_state(0, TimeUs(600_000)).unwrap().0);
    assert!(engine.lanes[Lane::Scratch.index()].scratch_press_suppression.is_none());
}

#[test]
fn hln_defers_one_score_until_end_and_ticks_strictly_after_200ms() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    let head = engine.process_input(&chart, press_at(TimeUs(0)));
    assert_eq!(head.events.len(), 1);
    assert!(!head.events[0].affects_score);
    assert!(engine.process_misses(&chart, TimeUs(200_000)).hold_ticks.is_empty());
    let tick = engine.process_misses(&chart, TimeUs(200_001));
    assert_eq!(tick.hold_ticks.len(), 1);
    assert!(tick.hold_ticks[0].increase);
    let end = engine.process_misses(&chart, TimeUs(1_000_000));
    assert_eq!(end.hold_ticks.len(), 3);
    assert_eq!(end.events.len(), 1);
    assert_eq!(end.events[0].note_id, Some(NoteId(1)));
    assert_eq!(end.events[0].judge, Judge::PGreat);
    assert_eq!(end.events[0].time, TimeUs(1_000_000));
    assert!(engine.process_input(&chart, release_at(TimeUs(1_000_000))).events.is_empty());
    assert!(engine.is_exhausted(&chart));
}

#[test]
fn hln_body_holds_until_all_sources_in_a_lane_release() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    engine.process_input(&chart, press_at(TimeUs(0)));
    engine.process_input(&chart, press_at(TimeUs(50_000)));
    let first_release = engine.process_input(&chart, release_at(TimeUs(100_000)));
    assert!(first_release.hold_sounds.is_empty());
    assert!(engine.hln_body_state(0, TimeUs(100_000)).unwrap().0);
    let last_release = engine.process_input(&chart, release_at(TimeUs(150_000)));
    assert_eq!(last_release.hold_sounds.len(), 1);
    assert!(!last_release.hold_sounds[0].audible);
    assert!(!engine.hln_body_state(0, TimeUs(150_000)).unwrap().0);
}

#[test]
fn hln_counts_the_full_duration_when_a_tick_coincides_with_tail() {
    let mut chart = chart_with_long_start(TimeUs(0), TimeUs(200_001));
    chart.long_notes[0].mode = Some(LongNoteMode::Hln);
    let mut engine = JudgeEngine::new(windows());
    engine.process_input(&chart, press_at(TimeUs(0)));
    let end = engine.process_misses(&chart, TimeUs(200_001));
    assert_eq!(end.hold_ticks.len(), 1);
    assert_eq!(end.events.len(), 1);
}

#[test]
fn hln_combines_head_and_release_as_ln() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    engine.process_input(&chart, press_at(TimeUs(30_000)));
    let release = engine.process_input(&chart, release_at(TimeUs(940_000)));
    assert_eq!(release.events.len(), 1);
    assert_eq!(release.events[0].judge, Judge::Good);
    assert_eq!(release.events[0].delta, TimeUs(-60_000));
    assert!(release.keysounds.is_empty());
    assert_eq!(release.hold_sounds.len(), 1);
    assert!(!release.hold_sounds[0].audible);
    assert!(!engine.is_exhausted(&chart));
    let tail = engine.process_misses(&chart, TimeUs(1_000_000));
    assert!(tail.events.is_empty());
    assert_eq!(tail.keysounds[0].note_id, NoteId(2));
}

#[test]
fn hln_regrab_changes_body_without_rescoring_or_empty_poor() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    engine.process_input(&chart, press_at(TimeUs(0)));
    let release = engine.process_input(&chart, release_at(TimeUs(100_000)));
    assert_eq!(release.events[0].judge, Judge::Bad);
    let regrab = engine.process_input(&chart, press_at(TimeUs(150_000)));
    assert!(regrab.events.is_empty());
    assert!(regrab.consumed_input);
    assert!(regrab.hold_sounds[0].audible);
    assert!(regrab.hold_sounds[0].start_at.is_none());
    // +100ms -50ms +150ms = +200ms; strict boundary, no tick yet.
    assert!(engine.process_misses(&chart, TimeUs(300_000)).hold_ticks.is_empty());
    let tick = engine.process_misses(&chart, TimeUs(300_001));
    assert_eq!(tick.hold_ticks.len(), 1);
    assert!(tick.hold_ticks[0].increase);
    assert!(engine.process_misses(&chart, TimeUs(1_100_000)).events.is_empty());
}

#[test]
fn hln_head_miss_is_single_poor_and_recovery_uses_original_sample_position() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    let missed = engine.process_misses(&chart, TimeUs(500_000));
    assert_eq!(missed.events.len(), 1);
    assert_eq!(missed.events[0].judge, Judge::Poor);
    assert_eq!(missed.events[0].time, TimeUs(120_001));
    assert_eq!(missed.hold_ticks.len(), 1);
    assert!(!missed.hold_ticks[0].increase);
    let regrab = engine.process_input(&chart, press_at(TimeUs(600_000)));
    assert!(regrab.events.is_empty());
    assert_eq!(regrab.hold_sounds[0].start_at, Some(TimeUs(0)));
    assert!(engine.process_misses(&chart, TimeUs(1_000_000)).events.is_empty());
}

#[test]
fn hln_early_capture_never_accumulates_before_chart_start() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    engine.process_input(&chart, press_at(TimeUs(-100_000)));
    assert!(engine.process_misses(&chart, TimeUs(200_000)).hold_ticks.is_empty());
    assert_eq!(engine.process_misses(&chart, TimeUs(200_001)).hold_ticks.len(), 1);
}

#[test]
fn hln_release_grace_uses_fixed_deadline_even_without_intermediate_frames() {
    for (regrab, expected) in [(299_999, Judge::PGreat), (300_000, Judge::Bad)] {
        let chart = chart();
        let mut windows = JudgeWindows::uniform(windows());
        windows.long_note_release_margin_us = 200_000;
        let mut engine = JudgeEngine::new_with_window_set(windows, RuleMode::Beatoraja);
        engine.process_input(&chart, press_at(TimeUs(0)));
        assert!(engine.process_input(&chart, release_at(TimeUs(100_000))).events.is_empty());
        let mut events = engine.process_input(&chart, press_at(TimeUs(regrab))).events;
        events.extend(engine.process_misses(&chart, TimeUs(1_000_000)).events);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].judge, expected);
    }
}

#[test]
fn hln_bad_consumes_head_even_in_9k() {
    let chart = chart();
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        JudgeWindows::uniform(windows()),
        RuleMode::Beatoraja,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );
    let first = engine.process_input(&chart, press_at(TimeUs(-100_000)));
    assert!(!first.events[0].affects_score);
    assert!(engine.process_input(&chart, press_at(TimeUs(0))).events.is_empty());
    let end = engine.process_misses(&chart, TimeUs(1_000_000));
    assert_eq!(end.events.len(), 1);
    assert_eq!(end.events[0].judge, Judge::Bad);
}

#[test]
fn hln_viewer_seek_restores_pending_head_without_retroactive_ticks() {
    let chart = chart();
    let mut engine = JudgeEngine::new(windows());
    engine.skip_before(&chart, TimeUs(600_000));
    assert!(engine.process_misses(&chart, TimeUs(800_000)).hold_ticks.is_empty());
    let end = engine.process_misses(&chart, TimeUs(1_000_000));
    assert_eq!(end.events.len(), 1);
    assert_eq!(end.events[0].judge, Judge::PGreat);
    assert_eq!(end.hold_ticks.len(), 1);
}

#[test]
fn hln_body_and_score_do_not_depend_on_frame_partition() {
    fn run(step: i64) -> (Vec<JudgementEvent>, Vec<crate::judge::model::HoldGaugeEvent>) {
        let chart = chart();
        let mut engine = JudgeEngine::new(windows());
        let inputs = [press_at(TimeUs(0)), release_at(TimeUs(350_000)), press_at(TimeUs(700_000))];
        let mut events = Vec::new();
        let mut ticks = Vec::new();
        let mut next_frame = step;
        for input in inputs {
            while next_frame < input.time.0 {
                let out = engine.process_misses(&chart, TimeUs(next_frame));
                events.extend(out.events);
                ticks.extend(out.hold_ticks);
                next_frame += step;
            }
            let out = engine.process_input(&chart, input);
            events.extend(out.events);
            ticks.extend(out.hold_ticks);
        }
        while next_frame < 1_100_000 {
            let out = engine.process_misses(&chart, TimeUs(next_frame));
            events.extend(out.events);
            ticks.extend(out.hold_ticks);
            next_frame += step;
        }
        let out = engine.process_misses(&chart, TimeUs(1_100_000));
        events.extend(out.events);
        ticks.extend(out.hold_ticks);
        (events, ticks)
    }
    assert_eq!(run(1_000), run(1_100_000));
    assert_eq!(run(16_667), run(1_100_000));
}
