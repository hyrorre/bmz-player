use super::*;
use bmz_audio::queue::ScheduledSoundQueue;
use bmz_core::replay::ReplayEvent;
use std::io::Write;

pub(super) fn imported(body: &str) -> PlayableChart {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(file, "#BPM 120\n#WAV01 a.wav\n#WAV02 b.wav\n{body}\n").unwrap();
    bmz_chart::import::import_chart(file.path(), Some(1), false).unwrap().chart
}

pub(super) fn advance(session: &mut GameSession, time: i64, audio: &mut ScheduledSoundQueue) {
    session.audio_clock.start_output_frame = 0;
    session.audio_clock.running = true;
    session
        .audio_clock
        .current_frame
        .store((time * 48 / 1000) as u64, std::sync::atomic::Ordering::Relaxed);
    advance_session_frame(session, audio);
}

#[test]
fn empty_default_does_not_close_scoring_before_a_later_branch_and_replay_finishes() {
    let chart = imported("#CONDITIONAL 4\n#WHEN RATE>=100\n#00511:01\n#DEFAULT\n#ENDCONDITIONAL");
    for replay in [false, true] {
        let mut session = session_with_autoplay(chart.clone());
        if replay {
            session.autoplay = None;
            session.replay_player = Some(ReplayPlayer {
                branch_decisions: Some(vec![bmz_core::replay::BranchDecision {
                    block: 0,
                    branch: 0,
                    time: TimeUs(8_000_000),
                }]),
                ..Default::default()
            });
            session.conditional = super::super::conditional::ConditionalRuntime::from_player(
                session.replay_player.as_ref(),
            );
        }
        let mut audio = ScheduledSoundQueue::new();
        advance(&mut session, 1_000_000, &mut audio);
        assert_eq!(session.scoring_closed_at, None);
        assert_eq!(session.state, PlayState::Playing);
        advance(&mut session, 12_000_000, &mut audio);
        assert_eq!(session.conditional.decisions[0].branch, 0);
        assert_eq!(session.score.past_notes, 1);
        assert!(session.scoring_closed_at.is_some());
        // Scoring closes before the existing audio tail; wait through that tail.
        let finished_at = session.chart.end_time.0 + SESSION_END_MARGIN_US + 1_000;
        advance(&mut session, finished_at, &mut audio);
        assert_eq!(session.state, PlayState::Finished);
    }
}

#[test]
fn same_time_snapshot_and_actual_count_are_frame_rate_independent() {
    let body = "#00011:01\n#00111:01\n#CONDITIONAL 1\n#WHEN COMBO>=2\n#00211:0101\n#DEFAULT\n#00211:01\n#ENDCONDITIONAL\n#CONDITIONAL 1\n#WHEN SCORE==2\n#00312:01\n#DEFAULT\n#00312:0101\n#ENDCONDITIONAL";
    for step in [1_000, 16_000, 9_000_000] {
        let mut s = session_with_autoplay(imported(body));
        let mut audio = ScheduledSoundQueue::new();
        let reference = s.scored_total_notes;
        let mut time = 0;
        while time < 9_000_000 {
            advance(&mut s, time, &mut audio);
            time += step;
        }
        advance(&mut s, 9_000_000, &mut audio);
        assert_eq!(
            s.conditional.decisions.iter().map(|d| d.branch).collect::<Vec<_>>(),
            vec![1, 0]
        );
        assert_eq!(reference, 5);
        assert_eq!(s.scored_total_notes, 4);
        assert_eq!(s.score.ex_score(), 8);
        assert_eq!(s.score.bp(), 0);
    }
}

#[test]
fn recorded_choice_overrides_current_score_and_lamp() {
    let body = "#CONDITIONAL 1\n#WHEN LAMP>=5\n#00211:0101\n#DEFAULT\n#00211:01\n#ENDCONDITIONAL";
    let mut s = session_with_autoplay(imported(body));
    s.conditional.best_lamp = 10;
    s.conditional.replay_decisions = Some(vec![bmz_core::replay::BranchDecision {
        block: 0,
        branch: 1,
        time: TimeUs(2_000_000),
    }]);
    advance(&mut s, 8_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(s.scored_total_notes, 1);
    assert_eq!(s.score.ex_score(), 2);
    assert_eq!(s.conditional.evaluations[0].calculated_branch, 0);
    assert_eq!(s.conditional.evaluations[0].decision.branch, 1);
    assert!(s.conditional.evaluations[0].from_replay);
}

#[test]
fn stopped_replay_does_not_evaluate_or_score_the_future_suffix() {
    let mut s = session_with_autoplay(imported(
        "#00011:01\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00211:0101\n#DEFAULT\n#00211:01\n#ENDCONDITIONAL",
    ));
    s.autoplay = None;
    s.replay_player = Some(ReplayPlayer {
        kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
            reference_total: 2,
            actual_total: 2,
            preplay_lamp: 5,
            ended_at: TimeUs(1_000_000),
            manually_stopped: true,
            ..Default::default()
        }),
        branch_decisions: Some(vec![]),
        events: vec![ReplayEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(0),
            device_kind: Default::default(),
            scratch_direction: None,
        }],
        ..Default::default()
    });
    s.conditional =
        super::super::conditional::ConditionalRuntime::from_player(s.replay_player.as_ref());
    advance(&mut s, 9_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(s.state, PlayState::Failed);
    assert_eq!(s.score.ex_score(), 2);
    assert_eq!(s.conditional.best_lamp, 5);
    assert!(s.conditional.evaluations.is_empty());
    assert_eq!(s.conditional.decisions[0].branch, 1);
}

#[test]
fn viewer_seek_uses_the_selected_prefix_and_keeps_boundary_inputs_pending() {
    let mut s = session_with_autoplay(imported(
        "#00011:01\n#CONDITIONAL 1\n#WHEN SCORE>=2\n#00211:0101\n#DEFAULT\n#00211:01\n#ENDCONDITIONAL",
    ));
    prepare_viewer_seek(&mut s, TimeUs(4_000_000));
    assert_eq!(s.conditional.decisions[0].branch, 0);
    assert_eq!(s.score.ex_score(), 2);
    assert!(s.pending_keysounds.is_empty());
    advance(&mut s, 4_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(s.score.ex_score(), 4);
}

#[test]
fn selected_media_extends_the_optional_result_wait() {
    let mut session = session_with_autoplay(imported(
        "#00011:01\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00501:01\n#DEFAULT\n#ENDCONDITIONAL",
    ));
    session.result_wait_end_time = Some(TimeUs(0));
    advance(&mut session, 3_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(session.result_wait_end_time, Some(TimeUs(10_000_000)));
    assert!(!should_finish(&session, TimeUs(3_000_000)));
}

#[test]
fn pending_branch_prevents_full_combo_and_result_settlement() {
    let mut s = session_with_autoplay(imported(
        "#00011:01\n#CONDITIONAL 3\n#WHEN AUTO==1\n#00411:01\n#DEFAULT\n#ENDCONDITIONAL",
    ));
    let mut audio = ScheduledSoundQueue::new();
    advance(&mut s, 0, &mut audio);
    assert!(s.full_combo_started_at.is_none());
    assert!(!result_is_settled(&s, TimeUs(100_000_000)));
    assert!(!should_finish(&s, TimeUs(100_000_000)));
    advance(&mut s, 9_000_000, &mut audio);
    assert_eq!(s.scored_total_notes, 2);
    assert_eq!(s.score.ex_score(), 4);
    assert!(s.full_combo_started_at.is_some());
}

#[test]
fn started_ln_keeps_original_endpoint() {
    let body = "#LNOBJ ZZ\n#00011:01\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00211:ZZ\n#DEFAULT\n#00311:ZZ\n#ENDCONDITIONAL";
    let mut s = session_with_autoplay(imported(body));
    advance(&mut s, 8_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(s.chart.long_notes[0].end_time, TimeUs(6_000_000));
    assert_eq!(s.score.ex_score(), 2);
    assert_eq!(s.score.bp(), 0);
}

#[test]
fn undecided_branch_blocks_audio_lookahead_and_common_bgm_is_not_repeated() {
    let body = "#00101:01\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00101:02\n#00211:01\n#DEFAULT\n#00101:01\n#00211:01\n#ENDCONDITIONAL";
    let mut s = session_with_autoplay(imported(body));
    let old_sound = s.chart.sounds.iter().find(|a| a.path.ends_with("a.wav")).unwrap().id;
    let new_sound = s.chart.sounds.iter().find(|a| a.path.ends_with("b.wav")).unwrap().id;
    let mut audio = ScheduledSoundQueue::new();
    advance(&mut s, 1_950_000, &mut audio);
    assert_eq!(audio.drain_all().count(), 0);
    advance(&mut s, 2_000_000, &mut audio);
    advance(&mut s, 2_020_000, &mut audio);
    let sounds: Vec<_> = audio.drain_all().collect();
    assert_eq!(sounds.iter().filter(|event| event.sound_id == old_sound).count(), 1);
    assert_eq!(sounds.iter().filter(|event| event.sound_id == new_sound).count(), 1);
}

#[test]
fn multiple_tempo_branches_in_one_update_match_small_updates() {
    let body = "#BPM01 240\n#STOP01 192\n#CONDITIONAL 1\n#WHEN AUTO==1\n#00108:01\n#00109:01\n#00202:0.5\n#DEFAULT\n#ENDCONDITIONAL\n#CONDITIONAL 3\n#WHEN AUTO==1\n#00411:0101\n#DEFAULT\n#00411:01\n#ENDCONDITIONAL";
    let mut s = session_with_autoplay(imported(body));
    advance(&mut s, 8_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(s.conditional.decisions[0].time, TimeUs(2_000_000));
    assert_eq!(s.conditional.decisions[1].time, TimeUs(4_500_000));
    assert_eq!(s.scored_total_notes, 2);
    assert_eq!(s.score.ex_score(), 4);
}

#[test]
fn early_judged_future_note_is_kept_when_branch_removes_it() {
    let mut s = session_with_autoplay(imported(
        "#CONDITIONAL 1\n#WHEN AUTO==0\n#DEFAULT\n#00111:01\n#ENDCONDITIONAL",
    ));
    s.autoplay = None;
    s.conditional.pending_inputs.push(InputEvent {
        lane: Lane::Key1,
        kind: InputKind::Press,
        time: TimeUs(1_990_000),
        source: InputSource::Human,
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    });
    advance(&mut s, 2_010_000, &mut ScheduledSoundQueue::new());
    assert_eq!(s.conditional.decisions[0].branch, 0);
    assert_eq!(s.scored_total_notes, 1);
    assert_eq!(s.score.ex_score(), 2);
    assert_eq!(s.score.bp(), 0);
}

#[test]
fn hcn_gauge_before_branch_matches_fine_updates() {
    for measure in [1, 4] {
        let body = format!(
            "#LNMODE 3\n#00051:01\n#00351:01\n#CONDITIONAL {measure}\n#WHEN GAUGE>=20\n#00512:01\n#DEFAULT\n#00512:0101\n#ENDCONDITIONAL"
        );
        let evaluation = measure * 2_000_000;
        let mut values = Vec::new();
        for step in [1_000, evaluation] {
            let mut s = session_with_autoplay(imported(&body));
            s.autoplay = None;
            s.gauge =
                GaugeState::new(bmz_core::clear::GaugeType::Normal, 1.0, s.scored_total_notes);
            s.replay_player = Some(ReplayPlayer::new(vec![
                bmz_core::replay::ReplayEvent {
                    lane: Lane::Key1,
                    kind: InputKind::Press,
                    time: TimeUs(0),
                    device_kind: InputDeviceKind::Keyboard,
                    scratch_direction: None,
                },
                bmz_core::replay::ReplayEvent {
                    lane: Lane::Key1,
                    kind: InputKind::Release,
                    time: TimeUs(1_000_000),
                    device_kind: InputDeviceKind::Keyboard,
                    scratch_direction: None,
                },
            ]));
            let mut t = 0;
            let mut audio = ScheduledSoundQueue::new();
            while t < evaluation {
                advance(&mut s, t, &mut audio);
                t += step;
            }
            advance(&mut s, evaluation, &mut audio);
            values.push((s.gauge.current().value, s.conditional.decisions[0].branch));
        }
        assert_eq!(values[0], values[1]);
    }
}

#[test]
fn zero_default_denominator_rate_is_one_hundred() {
    let mut session = session_with_autoplay(imported(
        "#CONDITIONAL 0\n#WHEN RATE==100\n#00111:01\n#DEFAULT\n#ENDCONDITIONAL",
    ));
    advance(&mut session, 4_000_000, &mut ScheduledSoundQueue::new());
    assert_eq!(session.conditional.reference_total, Some(0));
    assert_eq!(session.conditional.decisions[0].branch, 0);
    assert_eq!(session.score.ex_score(), 2);
}

#[test]
fn battle_players_select_independently_before_same_time_inputs() {
    for step in [1_000, 9_000_000] {
        let body = "#00011:01\n#00111:01\n#CONDITIONAL 1\n#WHEN COMBO>=1\n#00212:0101\n#DEFAULT\n#00212:01\n#ENDCONDITIONAL";
        let mut session = session_with_autoplay(imported(body));
        let opponent = session_with_autoplay(imported(body));
        session.opponent.independent = Some(BattleOpponentSession {
            replay_judge_rate_percent: 100,
            conditional: Default::default(),
            chart: opponent.chart,
            key_mode: opponent.primary_key_mode,
            scored_total_notes: opponent.scored_total_notes,
            judge: opponent.judge,
            base_judge_windows: opponent.base_judge_windows,
            rule_mode: opponent.rule_mode,
            score: opponent.score,
            gauge: opponent.gauge,
            replay_player: Some(ReplayPlayer::new(Vec::new())),
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
        let mut audio = ScheduledSoundQueue::new();
        let mut time = 0;
        while time < 9_000_000 {
            advance(&mut session, time, &mut audio);
            time += step;
        }
        advance(&mut session, 9_000_000, &mut audio);
        let opponent = session.opponent.independent.as_ref().unwrap();
        assert_eq!(session.conditional.decisions[0].branch, 0);
        assert_eq!(opponent.conditional.decisions[0].branch, 1);
        assert_eq!(session.scored_total_notes, 4);
        assert_eq!(opponent.scored_total_notes, 3);
        assert_eq!(session.score.ex_score(), 8);
        assert_eq!(opponent.score.bp(), 3);
    }
}
