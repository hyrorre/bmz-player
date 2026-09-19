use super::*;

fn hln_chart() -> PlayableChart {
    let mut chart = chart_with_hcn_long_note();
    chart.long_notes[0].mode = Some(LongNoteMode::Hln);
    chart
}

fn at(session: &mut GameSession, time: i64) {
    session.audio_clock =
        AudioClock::with_position(48_000, 0, time, Arc::new(AtomicU64::new(0)), true);
}

#[test]
fn hln_scores_once_and_body_ticks_only_affect_gauge() {
    let mut session = session_with_autoplay(hln_chart());
    session.autoplay = None;
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 200);
    session.gauge.set_initial_value(50.0);
    input::process_session_input(&mut session, human_press(TimeUs(0)));
    assert_eq!(session.score.past_notes, 0);
    assert_eq!(session.gauge.current().value, 50.0);
    process_misses(&mut session, TimeUs(900_000));
    let mut expected = GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 200);
    expected.set_initial_value(50.0);
    for _ in 0..4 {
        expected.apply_hcn_hold();
    }
    assert_eq!(session.gauge.current().value, expected.current().value);
    assert_eq!(session.score.combo, 0);
    assert_eq!(session.score.ex_score(), 0);
    let end = process_misses(&mut session, TimeUs(1_000_000));
    assert_eq!(end.len(), 1);
    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.score.ex_score(), 2);
    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.ghost.len(), 1);
}

#[test]
fn hln_autoplay_coarse_and_fine_frames_have_the_same_result() {
    fn run(step: i64) -> (u32, u32, f32) {
        let mut session = session_with_autoplay(hln_chart());
        session.state = PlayState::Playing;
        session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 200);
        session.gauge.set_initial_value(50.0);
        let mut audio = TestAudio::default();
        let mut now = 0;
        while now < 1_000_000 {
            at(&mut session, now);
            advance_session_frame(&mut session, &mut audio);
            if now < 1_000_000 {
                assert!(session.full_combo_started_at.is_none());
            }
            now += step;
        }
        at(&mut session, 1_000_000);
        advance_session_frame(&mut session, &mut audio);
        assert_eq!(session.full_combo_started_at, Some(TimeUs(1_000_000)));
        (session.score.ex_score(), session.score.past_notes, session.gauge.current().value)
    }
    assert_eq!(run(10_000), run(1_000_000));
    assert_eq!(run(16_667), run(1_000_000));
}

#[test]
fn hln_viewer_does_not_prefill_crossing_head() {
    let mut session = session_with_autoplay(hln_chart());
    prepare_viewer_seek(&mut session, TimeUs(500_000));
    assert_eq!(session.score.past_notes, 0);
    let mut audio = TestAudio::default();
    at(&mut session, 1_000_000);
    session.state = PlayState::Playing;
    advance_session_frame(&mut session, &mut audio);
    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.score.ex_score(), 2);
}

#[test]
fn hln_auto_keysound_ignores_hold_gates_and_manual_tail() {
    let mut chart = hln_chart();
    chart.lane_notes[Lane::Key1.index()][1].sound = Some(SoundId(8));
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    input::process_session_input(&mut session, human_press(TimeUs(0)));
    input::process_session_input(&mut session, human_release(TimeUs(300_000)));
    input::process_session_input(&mut session, human_press(TimeUs(700_000)));
    process_misses(&mut session, TimeUs(1_000_000));
    let mut audio = TestAudio::default();
    schedule_keysounds(&mut session, &mut audio);
    assert!(audio.scheduled.is_empty());
    assert!(session.pending_keysound_volumes.is_empty());
}

#[test]
fn hln_body_failure_prevents_later_score_and_tail_sound_in_the_same_frame() {
    let mut chart = hln_chart();
    chart.lane_notes[Lane::Key1.index()][1].sound = Some(SoundId(8));
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.state = PlayState::Playing;
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 200);
    input::process_session_input(&mut session, human_press(TimeUs(0)));
    input::process_session_input(&mut session, human_release(TimeUs(100_000)));
    session.gauge.set_initial_value(1.0);
    let score_before = session.score.ex_score();
    session.pending_keysounds.clear();
    process_misses(&mut session, TimeUs(1_000_000));
    assert_eq!(session.state, PlayState::Failed);
    assert_eq!(session.score.ex_score(), score_before);
    assert!(session.pending_keysounds.is_empty());
    input::process_session_input(&mut session, human_press(TimeUs(1_100_000)));
    assert_eq!(session.state, PlayState::Failed);
}

#[test]
fn hln_failure_is_not_overwritten_by_finish_after_a_long_frame() {
    let mut session = session_with_autoplay(hln_chart());
    session.autoplay = None;
    session.state = PlayState::Playing;
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 200);
    input::process_session_input(&mut session, human_press(TimeUs(0)));
    input::process_session_input(&mut session, human_release(TimeUs(100_000)));
    session.gauge.set_initial_value(1.0);
    at(&mut session, 10_000_000);
    advance_session_frame(&mut session, &mut TestAudio::default());
    assert_eq!(session.state, PlayState::Failed);
}

#[test]
fn hln_regrab_keeps_another_lanes_automatic_tail_sound() {
    let mut chart = hln_chart();
    let mut other = chart.long_notes[0].clone();
    other.lane = Lane::Key2;
    other.start_note_id = NoteId(3);
    other.end_note_id = NoteId(4);
    other.end_time = TimeUs(500_000);
    let mut notes = chart.lane_notes[Lane::Key1.index()].clone();
    for note in &mut notes {
        note.lane = Lane::Key2;
        note.id = NoteId(note.id.0 + 2);
    }
    notes[1].time = other.end_time;
    notes[1].sound = Some(SoundId(8));
    chart.lane_notes[Lane::Key2.index()] = notes;
    chart.long_notes.push(other);
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    input::process_session_input(&mut session, human_press(TimeUs(0)));
    input::process_session_input(&mut session, human_release(TimeUs(100_000)));
    process_misses(&mut session, TimeUs(200_000));
    session.pending_keysounds.clear();
    input::process_session_input(&mut session, human_press(TimeUs(600_000)));
    assert_eq!(session.pending_keysounds.len(), 1);
    assert_eq!(session.pending_keysounds[0].note_id, NoteId(4));
    assert_eq!(session.pending_keysounds[0].time, TimeUs(500_000));
}

#[test]
fn hln_independent_battle_autoplay_matches_primary() {
    let chart = Arc::new(hln_chart());
    let mut session = session_with_autoplay((*chart).clone());
    session.state = PlayState::Playing;
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 100);
    session.gauge.set_initial_value(50.0);
    session.battle_opponent = Some(BattleOpponentSession {
        chart: Arc::clone(&chart),
        key_mode: chart.metadata.key_mode,
        scored_total_notes: 1,
        judge: JudgeEngine::new(session.base_judge_window),
        base_judge_windows: session.base_judge_windows,
        rule_mode: RuleMode::Beatoraja,
        score: ScoreState::default(),
        gauge: session.gauge.clone(),
        autoplay: Some(AutoplayController::default()),
        replay_player: None,
        display_uses_primary_arrangement: false,
        publish_display_judgements: false,
        gauge_increase_started_at: None,
        gauge_max_started_at: None,
        full_combo_started_at: None,
        lane_keyon_started_at: Default::default(),
        lane_hcn_timer: Default::default(),
        last_hcn_gauge_at: None,
    });
    let mut audio = TestAudio::default();
    at(&mut session, 1_000_000);
    advance_session_frame(&mut session, &mut audio);
    let opponent = session.battle_opponent.as_ref().unwrap();
    assert_eq!(opponent.score.ex_score(), 2);
    assert_eq!(opponent.score.ex_score(), session.score.ex_score());
    assert_eq!(opponent.gauge.current().value, session.gauge.current().value);
    assert_eq!(opponent.full_combo_started_at, Some(TimeUs(1_000_000)));
}
