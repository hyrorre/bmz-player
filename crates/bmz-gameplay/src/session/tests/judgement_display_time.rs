use super::*;

#[test]
fn human_judgement_display_time_does_not_change_scoring_or_result_time() {
    for offset in [-500_000, -20_000, 0, 20_000, 500_000] {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        session.offsets.input_offset_us = offset;
        let events = process_session_input(&mut session, human_press(TimeUs(10_000)));
        let event = &events[0];
        assert_eq!(event.judge, Judge::PGreat);
        assert_eq!(event.side, TimingSide::Slow);
        assert_eq!(event.delta, TimeUs(10_000));
        assert_eq!(event.time, TimeUs(10_000));
        assert_eq!(session.score.ex_score(), 2);
        assert_eq!(session.result_judgements[&NoteId(1)].time, event.time);
        let display = &session.recent_display_judgements[0];
        assert_eq!(display.judgement, *event);
        assert_eq!(display.display_time, TimeUs(10_000 - offset));
        assert_eq!(display.combo, 1);

        // 表示履歴は正負どちらの offset でも打鍵から800ms保持する。
        let expires = TimeUs(display.display_time.0 + JUDGEMENT_DISPLAY_US);
        update_recent_judgements(&mut session, &events, expires);
        assert_eq!(session.recent_display_judgements.len(), 1);
        update_recent_judgements(&mut session, &[], TimeUs(expires.0 + 1));
        assert!(session.recent_display_judgements.is_empty());
    }
}

#[test]
fn long_note_release_display_time_uses_uncorrected_human_input() {
    for mode in [LongNoteMode::Ln, LongNoteMode::Cn, LongNoteMode::Hcn] {
        for offset in [-20_000, 20_000] {
            let mut chart = chart_with_hcn_long_note();
            chart.metadata.long_note_mode = mode;
            chart.long_notes[0].mode = Some(mode);
            let mut session = session_with_autoplay(chart);
            session.autoplay = None;
            session.offsets.input_offset_us = offset;
            process_session_input(&mut session, human_press(TimeUs(0)));
            let events = process_session_input(&mut session, human_release(TimeUs(1_000_000)));
            assert_eq!(events.len(), 1, "mode={mode:?}");
            assert_eq!(events[0].judge, Judge::PGreat);
            assert_eq!(events[0].time, TimeUs(1_000_000));
            let display = session.recent_display_judgements.last().unwrap();
            assert_eq!(display.judgement, events[0]);
            assert_eq!(display.display_time, TimeUs(1_000_000 - offset));
        }
    }
}

#[test]
fn autoplay_replay_and_misses_keep_their_own_display_times() {
    for offset in [-500_000, 500_000] {
        let mut auto = session_with_autoplay(chart_with_keysound());
        auto.offsets.input_offset_us = offset;
        let events = process_autoplay_inputs(&mut auto, TimeUs(50_000));
        assert_eq!(events.len(), 1);
        assert_eq!(auto.recent_display_judgements[0].display_time, events[0].time);

        let mut replay = session_with_autoplay(chart_with_keysound());
        replay.autoplay = None;
        replay.offsets.input_offset_us = offset;
        let mut recorder = ReplayRecorder::default();
        recorder.record(human_press(TimeUs(10_000)));
        replay.replay_player = Some(ReplayPlayer { events: recorder.events, ..Default::default() });
        let events = process_replay_inputs(&mut replay, TimeUs(50_000));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].time, TimeUs(10_000));
        assert_eq!(replay.recent_display_judgements[0].display_time, events[0].time);

        let mut missed = session_with_autoplay(chart_with_keysound());
        missed.autoplay = None;
        missed.offsets.input_offset_us = offset;
        let events = process_misses(&mut missed, TimeUs(600_000));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].judge, Judge::Poor);
        assert_eq!(missed.recent_display_judgements[0].display_time, events[0].time);
    }
}
