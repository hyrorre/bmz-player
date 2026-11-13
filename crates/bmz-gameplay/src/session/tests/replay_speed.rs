use super::*;

#[test]
fn freq_recorded_inputs_reproduce_score_and_gauge_for_player_and_opponent() {
    let new_session = || {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        session.base_judge_windows = crate::judge::window::judge_windows_for_keymode_and_rule_mode(
            bmz_core::lane::KeyMode::K7,
            RuleMode::Beatoraja,
        );
        session
    };
    let mut recorded = new_session();
    recorded.audio_clock.set_playback_rate_percent(200);
    sync_judge_windows(&mut recorded, TimeUs(30_000));
    let input = human_press(TimeUs(30_000));
    recorded.replay_recorder.record(input);
    let judgements = process_session_input(&mut recorded, input);
    assert_eq!(judgements[0].judge, Judge::PGreat);
    assert_eq!(recorded.score.ex_score(), 2);

    for opponent in [false, true] {
        for viewing_rate in [25, 100, 200, 300] {
            let mut session = new_session();
            let replay = ReplayPlayer {
                events: recorded.replay_recorder.events.clone(),
                ..Default::default()
            };
            if opponent {
                let mut rival =
                    replay_session(LongNoteMode::Ln, true).opponent.independent.take().unwrap();
                rival.chart = session.chart.clone();
                rival.judge = session.judge;
                rival.base_judge_windows = session.base_judge_windows;
                rival.gauge = session.gauge.clone();
                rival.replay_player = Some(replay);
                rival.replay_judge_rate_percent = 200;
                session = new_session();
                session.opponent.independent = Some(rival);
            } else {
                session.replay_player = Some(replay);
                session.replay_judge_rate_percent = 200;
            }
            session.audio_clock.set_playback_rate_percent(viewing_rate);
            session.audio_clock.pause_at(TimeUs(100_000));
            advance_session_frame(&mut session, &mut TestAudio::default());
            let (score, gauge) = if opponent {
                let rival = session.opponent.independent.as_ref().unwrap();
                (&rival.score, &rival.gauge)
            } else {
                (&session.score, &session.gauge)
            };
            assert_eq!(format!("{score:?}"), format!("{:?}", recorded.score));
            assert_eq!(score.ex_score(), recorded.score.ex_score());
            assert_eq!(gauge.current().value, recorded.gauge.current().value);
        }
    }
}

fn replay_session(mode: LongNoteMode, opponent: bool) -> GameSession {
    let mut chart = chart_with_hcn_long_note();
    chart.long_notes[0].mode = Some(mode);
    chart.metadata.long_note_mode = mode;
    for (id, time) in [(3, 1_500_000), (4, 2_000_000), (5, 2_500_000), (6, 3_000_000)] {
        let mut note = chart.lane_notes[Lane::Key1.index()][0].clone();
        note.id = NoteId(id);
        note.kind = NoteKind::Tap;
        note.time = TimeUs(time);
        chart.lane_notes[Lane::Key1.index()].push(note);
    }
    chart.total_notes = 5;
    chart.end_time = TimeUs(3_000_000);
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    let replay = ReplayPlayer {
        events: [
            (InputKind::Press, 16_001),
            (InputKind::Release, 310_001),
            (InputKind::Press, 610_001),
            (InputKind::Release, 960_001),
            (InputKind::Press, 1_540_001),
            (InputKind::Release, 1_550_000),
            (InputKind::Press, 2_120_001),
            (InputKind::Release, 2_130_000),
            // Note 5 is missed. This late input must not recover it after its deadline.
            (InputKind::Press, 3_016_000),
            (InputKind::Release, 3_030_000),
        ]
        .into_iter()
        .map(|(kind, time)| bmz_core::replay::ReplayEvent {
            lane: Lane::Key1,
            kind,
            time: TimeUs(time),
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        })
        .collect(),
        ..Default::default()
    };
    if opponent {
        session.opponent.independent = Some(BattleOpponentSession {
            conditional: Default::default(),
            chart: Arc::clone(&session.chart),
            key_mode: session.primary_key_mode,
            scored_total_notes: session.scored_total_notes,
            judge: JudgeEngine::new(session.base_judge_window),
            base_judge_windows: session.base_judge_windows,
            rule_mode: session.rule_mode,
            score: ScoreState::default(),
            gauge: session.gauge.clone(),
            replay_player: Some(replay),
            replay_judge_rate_percent: 100,
            autoplay: None,
            display_uses_primary_arrangement: false,
            publish_display_judgements: true,
            gauge_increase_started_at: None,
            gauge_max_started_at: None,
            full_combo_started_at: None,
            lane_keyon_started_at: Default::default(),
            lane_hcn_timer: Default::default(),
            last_hcn_gauge_at: None,
        });
    } else {
        session.replay_player = Some(replay);
    }
    session
}

fn playback_trace(
    mode: LongNoteMode,
    rates: &[u16],
    offset: i64,
    opponent: bool,
    end_time: Option<i64>,
) -> Vec<String> {
    let mut session = replay_session(mode, opponent);
    if let Some(end_time) = end_time {
        let replay = if opponent {
            session.opponent.independent.as_mut().unwrap().replay_player.as_mut().unwrap()
        } else {
            session.replay_player.as_mut().unwrap()
        };
        replay.events.drain(1..3);
        replay.events[1].time = TimeUs(end_time);
    }
    session.offsets.visual_offset_us = offset;
    let mut audio = TestAudio::default();
    let mut now = 0;
    let mut trace = Vec::new();
    let mut judgements = Vec::new();
    for checkpoint in (0..=40).map(|step| step * 100_000) {
        loop {
            let rate = rates[(now / 200_000) as usize % rates.len()];
            session.audio_clock.set_playback_rate_percent(rate);
            session.audio_clock.pause_at(TimeUs(now));
            judgements.extend(advance_session_frame(&mut session, &mut audio).judgements);
            if now == checkpoint {
                break;
            }
            now = (now + 6_944 * i64::from(rate) / 100).min(checkpoint);
        }
        if let Some(opponent) = &session.opponent.independent {
            let mut notes: Vec<_> = opponent.judge.judged_notes.iter().collect();
            notes.sort_by_key(|(id, _)| **id);
            trace.push(format!(
                "{:?}|{:?}|{}|{}|{}",
                notes,
                opponent.score,
                opponent.gauge.current().value,
                opponent.score.ex_score(),
                opponent.score.bp()
            ));
        } else {
            let mut notes: Vec<_> = session.result_judgements.iter().collect();
            notes.sort_by_key(|(id, _)| **id);
            trace.push(format!(
                "{:?}|{:?}|{}|{}|{}|{:?}|{:?}",
                notes,
                session.score,
                session.gauge.current().value,
                session.score.ex_score(),
                session.score.bp(),
                judgements,
                session.last_miss_judgement_time
            ));
        }
    }
    assert_eq!(
        if opponent {
            session.opponent.independent.as_ref().unwrap().score.past_notes
        } else {
            session.score.past_notes
        },
        session.scored_total_notes,
        "all notes must be settled, including missed notes: mode={mode:?}, opponent={opponent}"
    );
    trace
}

#[test]
fn replay_scoring_is_independent_of_speed_frame_cadence_and_visual_offset() {
    for mode in [LongNoteMode::Ln, LongNoteMode::Cn, LongNoteMode::Hcn, LongNoteMode::Hln] {
        for opponent in [false, true] {
            let baseline = playback_trace(mode, &[100], 0, opponent, None);
            for offset in [0, -20_000] {
                for rates in [&[25][..], &[50], &[100], &[200], &[300], &[25, 300, 50, 200, 100]] {
                    assert_eq!(
                        playback_trace(mode, rates, offset, opponent, None),
                        baseline,
                        "mode={mode:?}, opponent={opponent}, rates={rates:?}, offset={offset}"
                    );
                }
            }
        }
    }
}

#[test]
fn replay_reproduces_the_chart_miss_text_trigger() {
    let trace = playback_trace(LongNoteMode::Ln, &[100], 0, false, None);
    // Note 5 is missed at 2.621s; the cadence test above compares this field at every speed.
    assert!(trace.last().unwrap().ends_with("|Some(TimeUs(2621000))"));
    assert!(trace.first().unwrap().ends_with("|None"));
}

#[test]
fn practice_human_input_keeps_speed_scaled_judge_windows() {
    for (rate, expected) in [
        (25, Judge::Good),
        (50, Judge::Great),
        (100, Judge::PGreat),
        (200, Judge::PGreat),
        (300, Judge::PGreat),
    ] {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.session_mode_index = 1;
        session.autoplay = None;
        session.audio_clock.set_playback_rate_percent(rate);
        sync_judge_windows(&mut session, TimeUs(0));
        let events = process_session_input(&mut session, human_press(TimeUs(15_000)));
        assert_eq!(events[0].judge, expected, "rate={rate}");
    }
}

#[test]
fn replay_long_note_end_boundaries_keep_the_same_scoring_at_every_speed() {
    for mode in [LongNoteMode::Ln, LongNoteMode::Cn, LongNoteMode::Hcn, LongNoteMode::Hln] {
        for end_time in [983_999, 984_000, 1_000_000, 1_016_000, 1_016_001] {
            for opponent in [false, true] {
                let baseline = playback_trace(mode, &[100], 0, opponent, Some(end_time));
                for rates in [&[25][..], &[50], &[200], &[300], &[25, 300, 50, 200, 100]] {
                    assert_eq!(
                        playback_trace(mode, rates, 0, opponent, Some(end_time)),
                        baseline,
                        "mode={mode:?}, opponent={opponent}, rates={rates:?}, end_time={end_time}"
                    );
                }
            }
        }
    }
}

#[test]
fn hln_full_replay_settles_misses_and_finishes() {
    for no_inputs in [false, true] {
        let mut session = replay_session(LongNoteMode::Hln, false);
        if no_inputs {
            session.replay_player.as_mut().unwrap().events.clear();
        }
        let mut audio = TestAudio::default();
        for now in (0..=10_000_000).step_by(10_000) {
            session.audio_clock.pause_at(TimeUs(now));
            advance_session_frame(&mut session, &mut audio);
        }
        assert_eq!(session.result_judgements[&NoteId(5)].judge, Judge::Poor);
        assert_eq!(session.score.past_notes, 5);
        assert!(session.judge.is_exhausted(&session.chart));
        assert_eq!(session.state, PlayState::Finished);
        if no_inputs {
            assert_eq!(session.score.ex_score(), 0);
            assert_eq!(session.score.bp(), 5);
        }
    }
}

fn autoplay_battle_session(chart: PlayableChart) -> GameSession {
    let mut session = session_with_autoplay(chart);
    let mut opponent = replay_session(LongNoteMode::Hln, true).opponent.independent.unwrap();
    opponent.chart = session.chart.clone();
    opponent.scored_total_notes = session.scored_total_notes;
    opponent.replay_player = None;
    opponent.autoplay = Some(AutoplayController::default());
    session.opponent.independent = Some(opponent);
    session
}

#[test]
fn battle_viewer_seek_starts_the_opponent_clock_at_the_restored_prefix() {
    let mut chart = (*replay_session(LongNoteMode::Hln, false).chart).clone();
    for note in chart.lane_notes.iter_mut().flatten() {
        note.time.0 += 119_500_000;
    }
    for pair in &mut chart.long_notes {
        pair.start_time.0 += 119_500_000;
        pair.end_time.0 += 119_500_000;
    }
    chart.end_time.0 += 119_500_000;
    let mut session = autoplay_battle_session(chart);
    prepare_viewer_seek(&mut session, TimeUs(120_000_000));
    let opponent = session.opponent.independent.as_ref().unwrap();
    assert_eq!(opponent.conditional.processed_time, Some(TimeUs(119_999_999)));
    assert_eq!(
        super::super::frame::next_battle_opponent_time(opponent, TimeUs(120_000_000)),
        Some(TimeUs(120_000_000))
    );
    session.audio_clock.pause_at(TimeUs(130_000_000));
    advance_session_frame(&mut session, &mut TestAudio::default());
    let opponent = session.opponent.independent.as_ref().unwrap();
    assert_eq!(opponent.score.past_notes, 5);
    assert_eq!(opponent.score.ex_score(), 10);
}

#[test]
fn battle_clock_jumps_over_idle_time_without_losing_later_notes() {
    let mut chart = (*replay_session(LongNoteMode::Hln, false).chart).clone();
    let mut note = chart.lane_notes[Lane::Key1.index()].last().unwrap().clone();
    note.id = NoteId(7);
    note.time = TimeUs(120_000_000);
    chart.lane_notes[Lane::Key1.index()].push(note);
    chart.total_notes += 1;
    chart.end_time = TimeUs(120_000_000);
    let mut session = autoplay_battle_session(chart);
    let mut audio = TestAudio::default();
    session.audio_clock.pause_at(TimeUs(4_000_000));
    advance_session_frame(&mut session, &mut audio);
    let opponent = session.opponent.independent.as_ref().unwrap();
    assert_eq!(opponent.score.past_notes, 5);
    // A 116-second idle gap requires one update, not 116,000 empty judge passes.
    assert_eq!(
        super::super::frame::next_battle_opponent_time(opponent, TimeUs(120_000_000)),
        Some(TimeUs(120_000_000))
    );
    session.audio_clock.pause_at(TimeUs(120_000_000));
    advance_session_frame(&mut session, &mut audio);
    assert_eq!(session.opponent.independent.as_ref().unwrap().score.past_notes, 6);
}

#[test]
fn battle_first_update_sets_auto_before_input_settles_an_empty_zone() {
    let chart = super::conditional::imported("#000SF:01\n#00011:00010000");
    let mut session = autoplay_battle_session(chart);
    session.audio_clock.pause_at(TimeUs(1_000_000));
    advance_session_frame(&mut session, &mut TestAudio::default());
    let opponent = session.opponent.independent.as_ref().unwrap();
    assert_eq!(opponent.score.past_notes, 2);
    assert_eq!(opponent.score.ex_score(), 4);
}

#[test]
fn battle_autoplay_long_stall_keeps_holds_and_branch_evaluation() {
    let chart = super::conditional::imported(
        "#HLNOBJ ZZ\n#HCNOBJ YY\n#00011:01ZZ\n#00012:01YY\n#CONDITIONAL 60\n#WHEN SCORE>=6\n#06111:0101\n#DEFAULT\n#06111:01\n#ENDCONDITIONAL",
    );
    let mut baseline = None;
    for step in [16_000, 125_000_000] {
        let mut session = autoplay_battle_session(chart.clone());
        let mut audio = TestAudio::default();
        for now in (0..125_000_000).step_by(step) {
            session.audio_clock.pause_at(TimeUs(now));
            advance_session_frame(&mut session, &mut audio);
        }
        session.audio_clock.pause_at(TimeUs(125_000_000));
        advance_session_frame(&mut session, &mut audio);
        let opponent = session.opponent.independent.as_ref().unwrap();
        assert_eq!(opponent.conditional.decisions[0].branch, 0);
        assert_eq!(opponent.score.past_notes, 5);
        let state = format!("{:?}|{}", opponent.score, opponent.gauge.current().value);
        if let Some(baseline) = &baseline {
            assert_eq!(&state, baseline);
        } else {
            baseline = Some(state);
        }
    }
}

#[test]
fn replay_failure_stops_scoring_at_the_same_tick_at_every_speed() {
    let mut baseline = None;
    for rate in [100, 25, 50, 200, 300] {
        let mut session = replay_session(LongNoteMode::Ln, false);
        session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 5);
        session.gauge.set_initial_value(1.0);
        session.audio_clock.set_playback_rate_percent(rate);
        let mut audio = TestAudio::default();
        for now in (0..4_000_000).step_by(6_944 * usize::from(rate) / 100) {
            session.audio_clock.pause_at(TimeUs(now));
            advance_session_frame(&mut session, &mut audio);
        }
        assert_eq!(session.state, PlayState::Failed);
        assert_eq!(session.score.past_notes, 1);
        let result = format!("{:?}|{}", session.score, session.gauge.current().value);
        if let Some(baseline) = &baseline {
            assert_eq!(&result, baseline, "rate={rate}");
        } else {
            baseline = Some(result);
        }
    }
}
