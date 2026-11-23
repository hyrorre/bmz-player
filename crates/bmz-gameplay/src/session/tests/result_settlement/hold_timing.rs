use super::*;
use crate::result::PlayResult;
use bmz_core::clear::{ClearType, GaugeType};

#[test]
fn timed_human_input_records_latency_on_hcn_charts() {
    // Other test threads use the silent dispatcher. Register both interests so a
    // callsite first reached on another thread still consults our local subscriber.
    let _silent = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::sink)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let (mut play, keys) = normal_gauge_hcn_session();
        let mut audio = TestAudio::default();
        let (collected, enqueued) = latency::judgement_sample_counts();
        keys.press();
        advance_at(&mut play, &mut audio, 1_000_000);
        assert!(!audio.scheduled.is_empty());
        latency::audio_enqueued();
        assert_eq!(latency::judgement_sample_counts(), (collected + 1, enqueued + 1));
    });
}

#[test]
fn positive_offset_latency_includes_the_wait_across_empty_polls() {
    let _silent = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(std::io::sink)
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        let (mut play, keys) = dx_human_session(tap_chart(TimeUs(1_050_000)), 50_000);
        let mut audio = TestAudio::default();
        keys.press();
        assert!(advance_at(&mut play, &mut audio, 1_000_000).judgements.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(50));
        keys.release();
        assert!(advance_at(&mut play, &mut audio, 1_025_000).judgements.is_empty());
        assert!(advance_at(&mut play, &mut audio, 1_049_000).judgements.is_empty());
        let frame = advance_at(&mut play, &mut audio, 1_050_000);
        assert_eq!(frame.judgements[0].judge, Judge::PGreat);
        assert!(latency::max_collect_judge_us() >= 50_000);
    });
}

#[test]
fn negative_offset_hcn_uses_judgement_time_for_the_clear_and_all_gauges() {
    let setup = || {
        let (mut session, keys) = hcn_regrab_session(RuleMode::Beatoraja, false);
        session.gauge = GaugeState::new(GaugeType::Normal, 90.0, session.scored_total_notes);
        (session, keys)
    };
    let (mut live, keys) = setup();
    live.offsets.input_offset_us = -50_000;
    let mut audio = TestAudio::default();
    for now in (0..=2_400_000).step_by(1_000) {
        if now == 1_050_000 {
            keys.press();
        }
        if now == 1_430_000 {
            keys.release();
        }
        advance_at(&mut live, &mut audio, now);
    }
    let edges = &live.replay_recorder.events;
    assert!(edges.iter().all(|edge| edge.applied_after.is_none()));
    assert_eq!((edges[0].time, edges[1].time), (TimeUs(1_000_000), TimeUs(1_380_000)));
    let recording = ReplayPlayer::new(edges.clone());
    let (mut own, _) = setup();
    own.opponent.independent = Some(independent_replay(setup().0, recording.clone()));
    own.replay_player = Some(recording);
    advance_at(&mut live, &mut audio, 12_000_000);
    advance_at(&mut own, &mut audio, 12_000_000);
    for s in [&live, &own] {
        let result = PlayResult::from_states(&s.chart, &s.score, &s.gauge, s.state, false);
        assert_eq!(
            (result.score.ex_score(), result.gauge_value, result.clear_type),
            (2, 78.5, ClearType::Failed)
        );
    }
    assert_eq!(live.gauge.carry_values(), own.gauge.carry_values());
    assert_eq!(live.gauge.carry_values(), own.opponent.independent.unwrap().gauge.carry_values());
}

#[test]
fn mixed_hcn_recovery_and_hln_drain_use_chart_order() {
    let chart = super::super::conditional::imported(
        "#HCNOBJ YY\n#HLNOBJ ZZ\n#00011:0001\n#00012:00000000000100000000000000000000\n#00111:YY\n#00112:ZZ",
    );
    for (lane, mode, start) in
        [(Lane::Key1, LongNoteMode::Hcn, 1_000_000), (Lane::Key2, LongNoteMode::Hln, 625_000)]
    {
        let pair = chart.long_notes.iter().find(|pair| pair.lane == lane).unwrap();
        assert_eq!(
            (pair.mode, pair.start_time, pair.end_time),
            (Some(mode), TimeUs(start), TimeUs(2_000_000))
        );
    }
    let setup = || {
        let (mut play, keys) = dx_human_session(chart.clone(), 0);
        play.gauge = GaugeState::new(GaugeType::Normal, 300.0, play.scored_total_notes);
        (play, keys)
    };
    let mut expected = None;
    for step in [1_000, 350_000] {
        let (mut play, keys) = setup();
        let mut audio = TestAudio::default();
        keys.press();
        advance_at(&mut play, &mut audio, 1_000_000);
        let mut now = 1_000_000;
        while now < 2_400_000 {
            now = (now + step).min(2_400_000);
            advance_at(&mut play, &mut audio, now);
        }
        let gauges = play.gauge.carry_values();
        assert_eq!(&gauges, expected.get_or_insert_with(|| gauges.clone()));
        let (mut replay, _) = setup();
        let recording = ReplayPlayer::new(play.replay_recorder.events);
        replay.opponent.independent = Some(independent_replay(setup().0, recording.clone()));
        replay.replay_player = Some(recording);
        advance_at(&mut replay, &mut audio, 2_400_000);
        assert_eq!(gauges, replay.gauge.carry_values());
        assert_eq!(gauges, replay.opponent.independent.unwrap().gauge.carry_values());
    }
}

fn fatal_hcn_session(gauge: GaugeType) -> (GameSession, SharedBackend) {
    let (mut play, keys) = hcn_regrab_session(RuleMode::Beatoraja, false);
    let chart = Arc::make_mut(&mut play.chart);
    chart.end_time = TimeUs(60_000_000);
    chart.long_notes[0].end_time = chart.end_time;
    chart.lane_notes[Lane::Key1.index()][1].time = chart.end_time;
    let tap = NoteEvent {
        id: NoteId(3),
        lane: Lane::Key2,
        kind: NoteKind::Tap,
        time: TimeUs(11_091_999),
        ..chart.lane_notes[Lane::Key1.index()][0].clone()
    };
    chart.lane_notes[Lane::Key2.index()].push(tap);
    chart.total_notes += 1;
    play.scored_total_notes = scored_note_count(chart);
    play.judge = play.judge.with_chart_index(play.chart.clone());
    play.gauge = GaugeState::new(gauge, 160.0, play.scored_total_notes);
    (play, keys)
}

#[test]
fn fatal_hcn_tick_stops_before_the_following_miss() {
    for (step, gauge) in
        [(1_000, GaugeType::Hard), (10_000, GaugeType::Hard), (10_000, GaugeType::Normal)]
    {
        let (mut play, keys) = fatal_hcn_session(gauge);
        let (rival, _) = fatal_hcn_session(GaugeType::Hard);
        let mut recorder = ReplayRecorder::default();
        recorder.record(human_press(TimeUs(1_000_000)));
        recorder.record(human_release(TimeUs(1_050_000)));
        play.opponent.independent =
            Some(independent_replay(rival, ReplayPlayer::new(recorder.events)));
        let mut audio = TestAudio::default();
        keys.press();
        advance_at(&mut play, &mut audio, 1_000_000);
        keys.release();
        advance_at(&mut play, &mut audio, 1_050_000);
        advance_at(&mut play, &mut audio, 11_299_000);
        assert_eq!(play.state, PlayState::Playing);
        for now in (11_299_000 + step..=11_309_000).step_by(step as usize) {
            advance_at(&mut play, &mut audio, now);
        }
        if gauge == GaugeType::Hard {
            assert_eq!(play.state, PlayState::Failed);
            assert_eq!(play.scoring_closed_at, Some(TimeUs(11_300_001)));
            assert_eq!((play.score.past_notes, play.score.bp()), (2, 1));
            assert!(!play.judge.judged_notes.contains_key(&NoteId(3)));
        } else {
            assert_eq!(play.state, PlayState::Playing);
            assert!(play.judge.judged_notes.contains_key(&NoteId(3)));
        }
        let rival = play.opponent.independent.unwrap();
        assert!(rival.gauge.current_closes_play_on_zero());
        assert_eq!((rival.score.past_notes, rival.score.bp()), (2, 1));
        assert!(!rival.judge.judged_notes.contains_key(&NoteId(3)));
    }
}
