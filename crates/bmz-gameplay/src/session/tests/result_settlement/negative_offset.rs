use super::*;

fn compare_replay(live: &GameSession, recording: ReplayPlayer, now: i64) {
    let setup = || dx_human_session((*live.chart).clone(), -50_000).0;
    let mut own = setup();
    own.opponent.independent = Some(independent_replay(setup(), recording.clone()));
    own.replay_player = Some(recording);
    advance_at(&mut own, &mut TestAudio::default(), now);
    let rival = own.opponent.independent.as_ref().unwrap();
    for (score, gauge) in [(&own.score, &own.gauge), (&rival.score, &rival.gauge)] {
        assert_eq!(format!("{:?}", score.judges), format!("{:?}", live.score.judges));
        assert_eq!(score.ghost, live.score.ghost);
        assert_eq!(score.past_notes, live.score.past_notes);
        assert_eq!(score.bp(), live.score.bp());
        assert_eq!(score.ex_score(), live.score.ex_score());
        assert_eq!(score.max_combo, live.score.max_combo);
        assert_eq!(gauge.carry_values(), live.gauge.carry_values());
    }
    assert_eq!(own.judge.judged_notes, live.judge.judged_notes);
    assert_eq!(rival.judge.judged_notes, live.judge.judged_notes);
}

fn ordinary_recording(live: &GameSession, times: &[i64]) -> ReplayPlayer {
    let edges = &live.replay_recorder.events;
    assert_eq!(edges.iter().map(|event| event.time.0).collect::<Vec<_>>(), times);
    assert!(edges.iter().all(|event| event.applied_after.is_none()));
    ReplayPlayer::new(edges.clone())
}

#[test]
fn slow_tap_still_hits_bad_at_delivery() {
    let (mut live, keys) = dx_human_session(tap_chart(TimeUs(1_000_000)), -50_000);
    let mut audio = TestAudio::default();
    advance_at(&mut live, &mut audio, 1_229_999);
    assert_eq!(live.score.past_notes, 0);
    keys.press();
    let frame = advance_at(&mut live, &mut audio, 1_230_000);
    assert_eq!(frame.judgements.len(), 1);
    assert_eq!(
        (frame.judgements[0].judge, frame.judgements[0].time),
        (Judge::Bad, TimeUs(1_180_000))
    );
    assert_eq!(live.recent_display_judgements[0].display_time, TimeUs(1_230_000));
    advance_at(&mut live, &mut audio, 1_300_000);
    assert_eq!(empty_poor_count(&live), 0);
    compare_replay(&live, ordinary_recording(&live, &[1_180_000]), 1_300_000);
}

fn chart_with_late_mine() -> PlayableChart {
    let mut chart = tap_chart(TimeUs(0));
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes.push(NoteEvent {
        id: NoteId(2),
        kind: NoteKind::Mine,
        time: TimeUs(1_000_500),
        damage: Some(8.0),
        ..notes[0].clone()
    });
    chart.end_time = TimeUs(1_000_500);
    chart
}

#[test]
fn mine_uses_the_key_held_at_its_chart_time() {
    for (release, hit) in [(1_050_500, false), (1_050_501, true)] {
        let (mut live, keys) = dx_human_session(chart_with_late_mine(), -50_000);
        let mut audio = TestAudio::default();
        advance_at(&mut live, &mut audio, 1_048_999);
        keys.press();
        assert!(advance_at(&mut live, &mut audio, 1_049_000).mine_hits.is_empty());
        advance_at(&mut live, &mut audio, release - 1);
        keys.release();
        assert_eq!(advance_at(&mut live, &mut audio, release).mine_hits.len(), usize::from(hit));
        advance_at(&mut live, &mut audio, 1_300_000);
        compare_replay(&live, ordinary_recording(&live, &[999_000, release - 50_000]), 1_300_000);
    }
}

#[test]
fn hln_release_sweep_matches_replay_without_delivery_metadata() {
    let chart = super::super::conditional::imported("#HLNOBJ ZZ\n#00011:0001\n#00111:ZZ");
    assert_eq!(chart.long_notes[0].mode, Some(LongNoteMode::Hln));
    let mut values = std::collections::BTreeSet::new();
    for release in (1_300_000..=1_500_000).step_by(10_000) {
        let (mut live, keys) = dx_human_session(chart.clone(), -50_000);
        let mut audio = TestAudio::default();
        advance_at(&mut live, &mut audio, 1_049_000);
        keys.press();
        advance_at(&mut live, &mut audio, 1_050_000);
        advance_at(&mut live, &mut audio, release - 1);
        keys.release();
        advance_at(&mut live, &mut audio, release);
        advance_at(&mut live, &mut audio, 2_400_000);
        values.insert(live.gauge.current().value.to_bits());
        compare_replay(&live, ordinary_recording(&live, &[1_000_000, release - 50_000]), 2_400_000);
    }
    assert!(values.len() > 1);
}

#[test]
fn input_delivered_after_its_miss_replays_the_processed_boundary() {
    let mut chart = tap_chart(TimeUs(1_000_000));
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes.push(NoteEvent { id: NoteId(2), time: TimeUs(3_000_000), ..notes[0].clone() });
    chart.total_notes += 1;
    chart.end_time = TimeUs(3_000_000);
    let (mut live, _) = dx_human_session(chart, -50_000);
    let mut audio = TestAudio::default();
    advance_at(&mut live, &mut audio, 1_253_123);
    assert_eq!(live.score.judges.slow_poor, 1);
    // The backend supplies an old timestamp after the owner has already scored the miss.
    live.conditional.pending_inputs.push(human_press(TimeUs(1_180_000)));
    advance_at(&mut live, &mut audio, 1_254_000);
    assert_eq!(empty_poor_count(&live), 1);
    assert_eq!(live.replay_recorder.events[0].applied_after, Some(TimeUs(1_200_123)));
    compare_replay(&live, ReplayPlayer::new(live.replay_recorder.events.clone()), 1_300_000);
}

#[test]
fn backdated_release_and_press_keep_delivery_order_at_the_same_boundary() {
    let (mut live, keys) = dx_human_session(chart_with_late_mine(), -50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 1_050_000);
    live.conditional.pending_inputs.push(human_release(TimeUs(995_000)));
    advance_at(&mut live, &mut audio, 1_051_000);
    live.conditional.pending_inputs.push(human_press(TimeUs(990_000)));
    advance_at(&mut live, &mut audio, 1_052_000);
    assert_eq!(advance_at(&mut live, &mut audio, 1_054_000).mine_hits.len(), 1);
    let edges = &live.replay_recorder.events;
    assert_eq!(edges.len(), 3);
    assert!(edges[1..].iter().all(|e| e.applied_after == Some(TimeUs(1_000_000))));
    compare_replay(&live, ReplayPlayer::new(edges.clone()), 1_300_000);
}

#[test]
fn early_finish_preserves_a_mine_between_score_and_audio_clocks() {
    let (mut live, keys) = dx_human_session(chart_with_late_mine(), -50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 50_000);
    // An observation with a newer audio time must not publish a settled result
    // before the owner has processed the remaining mine.
    assert!(!result_is_settled(&live, TimeUs(1_500_000)));
    live.play_end.request(PlayEndRequest::Finish);
    let frame = advance_at(&mut live, &mut audio, 1_010_100);
    assert!(frame.mine_hits.is_empty());
    assert!(live.finished_early);
    let scored_through = live.replay_stop_scored_through();
    assert_eq!(scored_through, Some(TimeUs(957_100)));
    let recording = ReplayPlayer {
        kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
            ended_at: live.result_settled_at().unwrap(),
            scored_through,
            finished_early: true,
            ..Default::default()
        }),
        ..ordinary_recording(&live, &[0])
    };
    compare_replay(&live, recording, 1_100_000);
}

#[test]
fn early_finish_replays_a_press_judged_before_chart_zero() {
    let (mut live, keys) = dx_human_session(tap_chart(TimeUs(0)), -50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 20_000);
    live.play_end.request(PlayEndRequest::Finish);
    advance_at(&mut live, &mut audio, 30_000);
    assert!(live.score.ex_score() > 0);
    assert_eq!(live.replay_stop_scored_through(), Some(TimeUs(-23_000)));
    let recording = ReplayPlayer {
        kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
            ended_at: TimeUs(30_000),
            scored_through: live.replay_stop_scored_through(),
            finished_early: true,
            ..Default::default()
        }),
        ..ordinary_recording(&live, &[-30_000])
    };
    compare_replay(&live, recording, 100_000);
}

#[test]
fn branch_is_evaluated_at_audio_time_and_a_late_press_keeps_its_time() {
    // A tap at 1.96875s before a branch at 2s that checks the score. With −50ms the
    // press reaches the owner at 2.019s, after the audio has reached the branch.
    let body = format!(
        "#00011:{}01\n#CONDITIONAL 1\n#WHEN SCORE>=1\n#00101:02\n#DEFAULT\n#00101:01\n#ENDCONDITIONAL\n#00211:01",
        "00".repeat(63)
    );
    let chart = super::super::conditional::imported(&body);
    let (mut live, keys) = dx_human_session(chart.clone(), -50_000);
    let mut audio = TestAudio::default();
    for now in (1_900_000..=2_100_000).step_by(1_000) {
        if now == 2_019_000 {
            keys.press();
        }
        advance_at(&mut live, &mut audio, now);
        if now == 2_000_000 {
            assert_eq!(live.conditional.decisions.len(), 1, "evaluated with the audio");
        }
    }
    // The branch did not count the late press, which keeps its own time for judgement.
    assert_eq!(live.conditional.decisions[0].branch, 1);
    assert_eq!(live.score.ex_score(), 2);
    let edge = live.replay_recorder.events[0];
    assert_eq!((edge.time, edge.applied_after), (TimeUs(1_969_000), None));
    compare_branch_replay(&live, &chart, 2_100_000);
}

fn compare_branch_replay(live: &GameSession, initial: &PlayableChart, end: i64) {
    let recording = ReplayPlayer {
        branch_decisions: Some(live.conditional.decisions.clone()),
        kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
            evaluations: live.conditional.evaluations.clone(),
            reference_total: live.conditional.reference_total.unwrap(),
            actual_total: live.scored_total_notes,
            preplay_lamp: live.conditional.best_lamp,
            ended_at: live.result_settled_at().unwrap_or(TimeUs(end)),
            scored_through: live.replay_stop_scored_through(),
            manually_stopped: live.manually_stopped,
            finished_early: live.finished_early,
            ..Default::default()
        }),
        ..ReplayPlayer::new(live.replay_recorder.events.clone())
    };
    for step in [13_000, end] {
        let setup = || dx_human_session(initial.clone(), 0).0;
        let mut own = setup();
        own.conditional =
            crate::session::conditional::ConditionalRuntime::from_player(Some(&recording));
        let mut rival = independent_replay(setup(), recording.clone());
        rival.conditional =
            crate::session::conditional::ConditionalRuntime::from_player(Some(&recording));
        own.opponent.independent = Some(rival);
        own.replay_player = Some(recording.clone());
        let mut audio = TestAudio::default();
        for now in (0..end).step_by(step as usize).chain([end]) {
            advance_at(&mut own, &mut audio, now);
        }
        let rival = own.opponent.independent.as_ref().unwrap();
        for (runtime, chart, total, score, gauge) in [
            (&own.conditional, &own.chart, own.scored_total_notes, &own.score, &own.gauge),
            (
                &rival.conditional,
                &rival.chart,
                rival.scored_total_notes,
                &rival.score,
                &rival.gauge,
            ),
        ] {
            assert_eq!(runtime.decisions, live.conditional.decisions);
            assert_eq!(runtime.evaluations.len(), live.conditional.evaluations.len());
            for (actual, observed) in runtime.evaluations.iter().zip(&live.conditional.evaluations)
            {
                assert_eq!(actual.values, observed.values);
                assert_eq!(actual.score_position, observed.score_position);
            }
            assert!(chart.metadata.kaleid.diagnostics.is_empty());
            assert_eq!(total, live.scored_total_notes);
            assert_eq!(format!("{:?}", score.judges), format!("{:?}", live.score.judges));
            assert_eq!(score.ghost, live.score.ghost);
            assert_eq!(gauge.carry_values(), live.gauge.carry_values());
        }
    }
}

#[test]
fn branch_preserves_the_slow_bad_window_and_pending_mine() {
    let body = format!(
        "#00011:{}01{}\n#000D1:{}08\n#CONDITIONAL 1\n#WHEN POOR==0\n#00101:02\n#DEFAULT\n#00101:01\n#ENDCONDITIONAL\n#00211:01",
        "00".repeat(57),
        "00".repeat(6),
        "00".repeat(127)
    );
    let chart = super::super::conditional::imported(&body);
    let (mut live, keys) = dx_human_session(chart.clone(), -50_000);
    let mut audio = TestAudio::default();
    advance_at(&mut live, &mut audio, 2_000_000);
    assert_eq!(live.conditional.decisions[0].branch, 0);
    assert_eq!(live.score.past_notes, 0);
    keys.press();
    let hit = advance_at(&mut live, &mut audio, 2_020_000);
    assert_eq!(hit.judgements[0].judge, Judge::Bad);
    assert_eq!(empty_poor_count(&live), 0);
    assert_eq!(advance_at(&mut live, &mut audio, 2_100_000).mine_hits.len(), 1);
    assert_eq!(live.replay_recorder.events[0].applied_after, None);
    compare_branch_replay(&live, &chart, 2_100_000);
}

#[test]
fn a_branch_inside_a_hold_preserves_the_release_and_gauge_ticks() {
    let branch = "#CONDITIONAL 1\n#WHEN GAUGE>=0\n#00101:02\n#DEFAULT\n#00101:01\n#ENDCONDITIONAL";
    for directive in ["HCNOBJ", "HLNOBJ"] {
        let body = format!("#{directive} ZZ\n#00011:0001\n#00111:00ZZ\n#00211:01");
        let run = |body: &str| {
            let chart = super::super::conditional::imported(body);
            let (mut live, keys) = dx_human_session(chart.clone(), -50_000);
            let mut audio = TestAudio::default();
            for now in (0..=3_400_000).step_by(1_000) {
                if now == 1_050_000 {
                    keys.press();
                }
                if now == 2_020_000 {
                    keys.release();
                }
                advance_at(&mut live, &mut audio, now);
            }
            (chart, live)
        };
        let (_, reference) = run(&body);
        let (chart, live) = run(&format!("{body}\n{branch}"));
        assert_eq!(live.score.ghost, reference.score.ghost);
        assert_eq!(live.gauge.carry_values(), reference.gauge.carry_values());
        assert!(live.replay_recorder.events.iter().all(|edge| edge.applied_after.is_none()));
        compare_branch_replay(&live, &chart, 3_400_000);
    }
}

#[test]
fn a_branch_splits_inputs_delivered_at_the_same_scoring_position() {
    let chart = super::super::conditional::imported(&format!(
        "#00011:{}01\n#CONDITIONAL 1\n#WHEN SCORE>=1\n#00112:02\n#DEFAULT\n#00113:01\n#ENDCONDITIONAL\n#00211:01",
        "00".repeat(31)
    ));
    let (mut live, keys) = dx_human_session(chart.clone(), -50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 1_999_000);
    advance_at(&mut live, &mut audio, 2_000_000);
    assert_eq!(live.conditional.decisions[0].branch, 0);
    live.conditional
        .pending_inputs
        .push(InputEvent { lane: Lane::Key2, ..human_press(TimeUs(1_948_000)) });
    advance_at(&mut live, &mut audio, 2_001_000);
    assert_eq!(live.replay_recorder.events[1].applied_after, Some(TimeUs(1_949_000)));
    assert_eq!(live.score.past_notes, 2);
    compare_branch_replay(&live, &chart, 2_100_000);
}

#[test]
fn stop_after_a_branch_keeps_the_selected_chart_ahead_of_scoring() {
    let chart = super::super::conditional::imported(
        "#CONDITIONAL 1\n#WHEN AUTO==0\n#00101:02\n#00211:0101\n#DEFAULT\n#00211:01\n#ENDCONDITIONAL\n#CONDITIONAL 3\n#WHEN SCORE==0\n#00411:01\n#DEFAULT\n#ENDCONDITIONAL",
    );
    for stop in [PlayEndRequest::ManualStop, PlayEndRequest::Finish] {
        let (mut live, _) = dx_human_session(chart.clone(), -50_000);
        let mut audio = TestAudio::default();
        advance_at(&mut live, &mut audio, 2_000_000);
        live.play_end.request(stop);
        advance_at(&mut live, &mut audio, 2_010_000);
        assert_eq!(live.scored_total_notes, 2);
        assert!(live.replay_stop_scored_through().unwrap() < TimeUs(2_000_000));
        compare_branch_replay(&live, &chart, 2_100_000);
    }
}

#[test]
fn partial_autoplay_sounds_at_note_time_while_scoring_waits() {
    let mut chart = tap_chart(TimeUs(1_000_000));
    let note = chart.lane_notes[Lane::Key1.index()].pop().unwrap();
    chart.lane_notes[Lane::Scratch.index()].push(NoteEvent { lane: Lane::Scratch, ..note });
    let (mut live, _) = dx_human_session(chart, -50_000);
    live.autoplay = Some(AutoplayController::for_lanes(&[Lane::Scratch]));
    let mut audio = TestAudio::default();
    let mut queued_at = None;
    for now in (900_000..=1_100_000).step_by(1_000) {
        advance_at(&mut live, &mut audio, now);
        if queued_at.is_none() && !audio.scheduled.is_empty() {
            queued_at = Some(now);
            assert_eq!(live.score.past_notes, 0, "queued before its delayed judgement");
        }
    }
    // Queued by the note time like an auto keysound, not 53ms later with the judgement.
    assert!(queued_at.is_some_and(|at| at <= 1_000_000), "queued at {queued_at:?}");
    // The delayed judgement scores the note without sounding it a second time.
    assert_eq!(live.score.ex_score(), 2);
    assert_eq!(audio.scheduled.len(), 1);
}
