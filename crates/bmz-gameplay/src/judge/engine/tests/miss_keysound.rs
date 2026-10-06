use super::*;

#[test]
fn miss_keysound_plays_once_for_tap_and_each_long_head() {
    for mode in [None, Some(LongNoteMode::Ln), Some(LongNoteMode::Cn), Some(LongNoteMode::Hcn)] {
        let mut chart = if mode.is_some() {
            chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000))
        } else {
            chart_with_tap(TimeUs(1_000_000))
        };
        if let Some(mode) = mode {
            chart.metadata.long_note_mode = mode;
        }
        let mut on = JudgeEngine::new(windows());
        let mut off = JudgeEngine::new(windows());
        let missed = on.process_misses_with_keysounds(&chart, TimeUs(1_130_000), true);
        let silent = off.process_misses(&chart, TimeUs(1_130_000));
        assert_eq!(missed.events, silent.events, "{mode:?}");
        assert!(silent.keysounds.is_empty());
        assert_eq!(
            missed.keysounds,
            [KeySoundEvent {
                note_id: NoteId(1),
                time: TimeUs(1_130_000),
                trigger: KeySoundTrigger::Miss,
            }],
            "{mode:?}"
        );
        assert!(
            on.process_misses_with_keysounds(&chart, TimeUs(2_130_000), true).keysounds.is_empty()
        );
    }
}

#[test]
fn miss_keysound_plays_unreleased_charge_tail_but_not_early_release() {
    for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
        let mut chart = chart_with_long_start(TimeUs(1_000_000), TimeUs(2_000_000));
        chart.metadata.long_note_mode = mode;
        let mut held = JudgeEngine::new(windows());
        held.process_input(&chart, press_at(TimeUs(1_000_000)));
        let missed = held.process_misses_with_keysounds(&chart, TimeUs(2_130_000), true);
        assert_eq!(
            missed.keysounds,
            [KeySoundEvent {
                note_id: NoteId(2),
                time: TimeUs(2_130_000),
                trigger: KeySoundTrigger::Miss,
            }]
        );
        let mut released = JudgeEngine::new(windows());
        released.process_input(&chart, press_at(TimeUs(1_000_000)));
        released.process_input(
            &chart,
            InputEvent { kind: InputKind::Release, ..press_at(TimeUs(1_100_000)) },
        );
        assert!(
            released
                .process_misses_with_keysounds(&chart, TimeUs(2_130_000), true)
                .keysounds
                .is_empty()
        );
    }
}

#[test]
fn miss_keysound_does_not_repeat_nonvanishing_bad_or_play_invisible_or_mine() {
    let chart = chart_with_tap(TimeUs(1_000_000));
    let mut engine = JudgeEngine::new_with_window_set_algorithm_and_keymode(
        crate::judge::window::dx_pop_judge_windows(),
        RuleMode::Dx,
        JudgeAlgorithm::Combo,
        KeyMode::K9,
    );
    assert_eq!(engine.process_input(&chart, press_at(TimeUs(900_000))).events[0].judge, Judge::Bad);
    assert!(
        engine.process_misses_with_keysounds(&chart, TimeUs(2_000_000), true).keysounds.is_empty()
    );
    for kind in [NoteKind::Invisible, NoteKind::Mine] {
        let mut chart = chart_with_tap(TimeUs(1_000_000));
        chart.lane_notes[Lane::Key1.index()][0].kind = kind;
        assert!(
            JudgeEngine::new(windows())
                .process_misses_with_keysounds(&chart, TimeUs(2_000_000), true)
                .keysounds
                .is_empty()
        );
    }
}
