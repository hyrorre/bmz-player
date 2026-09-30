use super::*;

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
        session.battle_opponent = Some(BattleOpponentSession {
            chart: Arc::clone(&session.chart),
            key_mode: session.primary_key_mode,
            scored_total_notes: session.scored_total_notes,
            judge: JudgeEngine::new(session.base_judge_window),
            base_judge_windows: session.base_judge_windows,
            rule_mode: session.rule_mode,
            score: ScoreState::default(),
            gauge: session.gauge.clone(),
            replay_player: Some(replay),
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
            session.battle_opponent.as_mut().unwrap().replay_player.as_mut().unwrap()
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
        if let Some(opponent) = &session.battle_opponent {
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
                "{:?}|{:?}|{}|{}|{}|{:?}",
                notes,
                session.score,
                session.gauge.current().value,
                session.score.ex_score(),
                session.score.bp(),
                judgements
            ));
        }
    }
    assert!(
        if opponent {
            session.battle_opponent.as_ref().unwrap().score.past_notes
        } else {
            session.score.past_notes
        } > 0
    );
    trace
}

#[test]
fn replay_scoring_is_independent_of_speed_frame_cadence_and_visual_offset() {
    for mode in [LongNoteMode::Ln, LongNoteMode::Cn, LongNoteMode::Hcn] {
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
    for mode in [LongNoteMode::Ln, LongNoteMode::Cn, LongNoteMode::Hcn] {
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
