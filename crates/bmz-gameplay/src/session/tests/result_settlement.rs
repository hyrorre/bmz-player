//! 採点の自然確定と、確定後に結果が変わらないことの回帰テスト。
#[path = "result_settlement/hold_timing.rs"]
mod hold_timing;
#[path = "result_settlement/negative_offset.rs"]
mod negative_offset;
use std::sync::Mutex;

use super::*;
use crate::input::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, InputBackend, PhysicalControl,
};
use crate::input::binding::{BindingEntry, LaneBinding};
use crate::judge::window::dx_judge_windows;

/// テストから後で入力を足せる backend。時刻は取得した更新の譜面時刻になる。
#[derive(Clone, Default)]
struct SharedBackend(Arc<Mutex<Vec<DeviceInputEvent>>>);

impl SharedBackend {
    fn press(&self) {
        self.push(InputKind::Press);
    }

    fn release(&self) {
        self.push(InputKind::Release);
    }

    fn push(&self, kind: InputKind) {
        self.0.lock().unwrap().push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind,
            timestamp: DeviceTimestamp::Unknown,
            bounce_policy: Default::default(),
        });
    }
}

impl InputBackend for SharedBackend {
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

fn tap_chart(time: TimeUs) -> PlayableChart {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()][0].time = time;
    chart.end_time = time;
    chart.metadata.key_mode = KeyMode::K7;
    chart
}

fn with_trailing_invisible(mut chart: PlayableChart) -> PlayableChart {
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(99),
        lane: Lane::Key1,
        kind: NoteKind::Invisible,
        tick: ChartTick(960),
        time: TimeUs(5_000_000),
        sound: Some(SoundId(8)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.sounds.push(SoundAssetRef { id: SoundId(8), path: "invisible.wav".into(), slice: None });
    chart.end_time = TimeUs(5_000_000);
    chart
}

/// DX 7KEY・等速の人間プレー。`W` は DX NOTE の BAD / 空POOR SLOW 側 200ms。
fn dx_human_session(chart: PlayableChart, input_offset_us: i64) -> (GameSession, SharedBackend) {
    let mut session = session_with_autoplay(chart);
    let windows = dx_judge_windows();
    session.rule_mode = RuleMode::Dx;
    session.primary_key_mode = KeyMode::K7;
    session.base_judge_windows = windows;
    session.base_judge_window = windows.note;
    session.judge = JudgeEngine::new_with_window_set(windows, RuleMode::Dx)
        .with_chart_index(Arc::clone(&session.chart));
    session.autoplay = None;
    session.offsets.input_offset_us = input_offset_us;
    let backend = SharedBackend::default();
    session.input_system = InputSystem {
        backend: Box::new(backend.clone()),
        translator: Box::new(DefaultInputTranslator {
            binding: LaneBinding {
                entries: vec![BindingEntry {
                    device: None,
                    control: PhysicalControl::KeyboardKey("Z".to_string()),
                    lane: Lane::Key1,
                    scratch_direction: None,
                }],
            },
        }),
        bounce_filter: Default::default(),
    };
    (session, backend)
}

fn advance_at(session: &mut GameSession, audio: &mut TestAudio, now: i64) -> SessionFrame {
    session.audio_clock.pause_at(TimeUs(now));
    advance_session_frame(session, audio)
}

fn empty_poor_count(session: &GameSession) -> u32 {
    session.score.judges.fast_empty_poor + session.score.judges.slow_empty_poor
}

fn hcn_regrab_session(rule: RuleMode, with_next_tap: bool) -> (GameSession, SharedBackend) {
    use crate::gauge::GaugeProperty;
    use crate::judge::window::judge_windows_for_keymode_and_rule_mode;
    use bmz_core::clear::GaugeType;

    let mut chart = chart_with_hcn_long_note();
    chart.long_notes[0].start_time = TimeUs(1_000_000);
    chart.long_notes[0].end_time = TimeUs(2_000_000);
    chart.end_time = TimeUs(2_000_000);
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes[0].time = TimeUs(1_000_000);
    notes[1].time = TimeUs(2_000_000);
    if with_next_tap {
        notes.push(NoteEvent {
            id: NoteId(3),
            kind: NoteKind::Tap,
            time: TimeUs(2_100_000),
            ..notes[0].clone()
        });
        chart.total_notes += 1;
        chart.end_time = TimeUs(2_100_000);
    }
    let (mut session, backend) = dx_human_session(chart, 0);
    session.rule_mode = rule;
    session.base_judge_windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K7, rule);
    session.judge = JudgeEngine::new_with_window_set(session.base_judge_windows, rule)
        .with_chart_index(Arc::clone(&session.chart));
    session.score = ScoreState::for_rule_mode(KeyMode::K7, rule);
    session.gauge = GaugeState::new_with_property_and_rule_mode(
        GaugeType::Class,
        260.0,
        session.scored_total_notes,
        GaugeProperty::for_rule_mode(KeyMode::K7, rule, None),
        rule,
    );
    session.gauge.set_initial_value(75.0);
    (session, backend)
}

#[test]
fn live_hcn_regrab_does_not_penalize_the_following_tap_or_the_consumed_head() {
    use crate::result::PlayResult;
    use bmz_core::clear::ClearType;

    for (rule, with_tap, release, regrab) in [
        // Leave enough hold time to cancel the outstanding drain and exceed +200ms.
        (RuleMode::Beatoraja, true, 1_200_000, 1_690_000),
        (RuleMode::Lr2Oraja, true, 1_200_000, 1_690_000),
        (RuleMode::Dx, true, 1_200_000, 1_690_000),
        (RuleMode::Beatoraja, false, 1_050_000, 1_060_000),
    ] {
        let (mut session, keys) = hcn_regrab_session(rule, with_tap);
        let mut audio = TestAudio::default();
        keys.press();
        let start = advance_at(&mut session, &mut audio, 1_000_000);
        assert_eq!(start.judgements[0].judge, Judge::PGreat);
        keys.release();
        let tail = advance_at(&mut session, &mut audio, release);
        assert_eq!(tail.judgements[0].judge, Judge::Poor);
        assert_eq!(session.judge.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
        assert!(session.judge.lanes[Lane::Key1.index()].active_long.is_none());
        assert!(!session.lane_hcn_timer[Lane::Key1.index()].unwrap().inclease);
        let release_gauge = session.gauge.current().value;
        advance_at(&mut session, &mut audio, regrab - 1);
        let before_regrab = session.gauge.current().value;
        if with_tap {
            assert!(before_regrab < release_gauge);
        }
        let ghost = session.score.ghost.clone();
        let sounds = audio.scheduled.len();
        keys.press();
        let frame = advance_at(&mut session, &mut audio, regrab);
        assert!(frame.judgements.is_empty());
        assert_eq!(audio.scheduled.len(), sounds);
        assert_eq!(empty_poor_count(&session), 0);
        assert_eq!((session.score.bp(), session.score.past_notes), (1, 2));
        assert_eq!(session.score.ghost, ghost);
        assert!(session.lane_hcn_timer[Lane::Key1.index()].unwrap().inclease);
        assert_eq!(session.lane_hcn_keysound_muted[Lane::Key1.index()], Some(false));
        assert!(!session.judge.judged_notes.contains_key(&NoteId(3)));

        if with_tap {
            advance_at(&mut session, &mut audio, 1_999_999);
            assert!(session.gauge.current().value > before_regrab);
            let mut failed = PlayResult::from_states(
                &session.chart,
                &session.score,
                &session.gauge,
                PlayState::Failed,
                false,
            );
            assert_eq!(failed.record_bp(), 2);
            keys.release();
            advance_at(&mut session, &mut audio, 2_050_000);
            keys.press();
            let hit = advance_at(&mut session, &mut audio, 2_100_000);
            assert_eq!(hit.judgements[0].judge, Judge::PGreat);
            failed = PlayResult::from_states(
                &session.chart,
                &session.score,
                &session.gauge,
                session.state,
                false,
            );
            assert_ne!(failed.clear_type, ClearType::Failed);
            assert_eq!(failed.record_bp(), 1);
        }
    }
}

#[test]
fn hcn_regrab_is_the_same_for_own_and_opponent_replays_across_frame_sizes() {
    let mut recorder = ReplayRecorder::default();
    for event in [
        human_press(TimeUs(1_000_000)),
        human_release(TimeUs(1_200_000)),
        human_press(TimeUs(1_700_000)),
        human_release(TimeUs(2_050_000)),
        human_press(TimeUs(2_100_000)),
    ] {
        recorder.record(event);
    }
    for rule in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
        for rival in [false, true] {
            for batched in [false, true] {
                let (mut session, _) = hcn_regrab_session(rule, true);
                let replay = ReplayPlayer::new(recorder.events.clone());
                if rival {
                    let (opponent, _) = hcn_regrab_session(rule, true);
                    session.opponent.independent = Some(independent_replay(opponent, replay));
                } else {
                    session.replay_player = Some(replay);
                }
                let mut audio = TestAudio::default();
                if !batched {
                    advance_at(&mut session, &mut audio, 1_201_000);
                    let judge = session
                        .opponent
                        .independent
                        .as_ref()
                        .map_or(&session.judge, |opponent| &opponent.judge);
                    assert_eq!(judge.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
                    advance_at(&mut session, &mut audio, 1_700_000);
                }
                advance_at(&mut session, &mut audio, 2_100_000);
                let score = session
                    .opponent
                    .independent
                    .as_ref()
                    .map_or(&session.score, |opponent| &opponent.score);
                assert_eq!((score.bp(), score.past_notes), (1, 3), "{rule:?} {rival} {batched}");
                assert_eq!(score.ghost, [0, 4, 0]);
            }
        }
    }
}

#[test]
fn batched_live_hcn_regrab_suppresses_empty_poor_and_keeps_the_mine_hit() {
    for batched in [false, true] {
        let (mut session, _) = hcn_regrab_session(RuleMode::Beatoraja, true);
        let chart = Arc::make_mut(&mut session.chart);
        let notes = &mut chart.lane_notes[Lane::Key1.index()];
        notes.insert(
            1,
            NoteEvent {
                id: NoteId(5),
                kind: NoteKind::Mine,
                time: TimeUs(1_700_000),
                sound: None,
                damage: Some(8.0),
                ..notes[0].clone()
            },
        );
        let mut audio = TestAudio::default();
        session
            .conditional
            .pending_inputs
            .extend([human_press(TimeUs(1_000_000)), human_release(TimeUs(1_200_000))]);
        if !batched {
            advance_at(&mut session, &mut audio, 1_200_001);
            assert_eq!(session.judge.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
        }
        session.conditional.pending_inputs.push(human_press(TimeUs(1_700_000)));
        let frame = advance_at(&mut session, &mut audio, 1_700_000);
        assert_eq!(frame.mine_hits.len(), 1);
        assert_eq!(frame.mine_hits[0].damage, 8.0);
        assert_eq!(session.judge.judged_notes.get(&NoteId(2)), Some(&Judge::Poor));
        assert_eq!(empty_poor_count(&session), 0);
        assert_eq!((session.score.bp(), session.score.past_notes), (1, 2));
        assert_eq!(session.score.ghost, [0, 4]);
    }
}

#[test]
fn mine_passage_uses_timestamped_edges_in_live_play_and_replay() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Playback {
        Live,
        OwnReplay,
        OpponentReplay,
    }
    let mine_time = 1_000_500;
    for rule in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
        for (edges, expected) in [
            (vec![(InputKind::Press, 990_000), (InputKind::Release, 995_000)], 0),
            (vec![(InputKind::Press, 1_010_000)], 0),
            (vec![(InputKind::Press, 900_000)], 1),
            (vec![(InputKind::Press, 990_000)], 1),
            (vec![(InputKind::Press, mine_time - 1), (InputKind::Release, mine_time + 1)], 1),
            (vec![(InputKind::Press, mine_time), (InputKind::Release, mine_time + 1)], 1),
            (vec![(InputKind::Press, mine_time - 1), (InputKind::Release, mine_time)], 0),
            (vec![(InputKind::Press, mine_time + 1)], 0),
            (vec![(InputKind::Press, mine_time), (InputKind::Release, mine_time)], 0),
        ] {
            let mut chart = tap_chart(TimeUs(3_000_000));
            chart.lane_notes[Lane::Key2.index()].push(NoteEvent {
                id: NoteId(9),
                lane: Lane::Key2,
                kind: NoteKind::Mine,
                tick: ChartTick(960),
                time: TimeUs(mine_time),
                sound: None,
                layered_sounds: Vec::new(),
                damage: Some(8.0),
            });
            for source in [Playback::Live, Playback::OwnReplay, Playback::OpponentReplay] {
                for batched in [false, true] {
                    let (mut session, _) = dx_human_session(chart.clone(), 0);
                    session.rule_mode = rule;
                    let mut recorder = ReplayRecorder::default();
                    for &(kind, time) in &edges {
                        let input =
                            InputEvent { lane: Lane::Key2, kind, ..human_press(TimeUs(time)) };
                        if source != Playback::Live {
                            recorder.record(input);
                        } else {
                            session.conditional.pending_inputs.push(input);
                        }
                    }
                    match source {
                        Playback::Live => {}
                        Playback::OwnReplay => {
                            session.replay_player = Some(ReplayPlayer::new(recorder.events))
                        }
                        Playback::OpponentReplay => {
                            let (mut rival, _) = dx_human_session(chart.clone(), 0);
                            rival.rule_mode = rule;
                            session.opponent.independent =
                                Some(independent_replay(rival, ReplayPlayer::new(recorder.events)));
                        }
                    }
                    let initial = session.gauge.current().value;
                    let mut audio = TestAudio::default();
                    let times = if batched {
                        vec![1_020_000]
                    } else {
                        edges
                            .iter()
                            .map(|&(_, time)| time)
                            .chain([mine_time, 1_020_000])
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>()
                    };
                    let hits = times
                        .into_iter()
                        .map(|time| advance_at(&mut session, &mut audio, time).mine_hits.len())
                        .sum::<usize>();
                    let (gauge, score) = if let Some(rival) = &session.opponent.independent {
                        assert_eq!(hits, 0);
                        (&rival.gauge, &rival.score)
                    } else {
                        assert_eq!(
                            hits, expected,
                            "{rule:?} {edges:?} source={source:?} batched={batched}"
                        );
                        (&session.gauge, &session.score)
                    };
                    assert_eq!(gauge.current().value, initial - 8.0 * expected as f32);
                    assert_eq!((score.bp(), score.combo, score.past_notes), (0, 0, 0));
                }
            }
        }
    }
}

#[test]
fn mines_see_the_remaining_scratch_direction_in_own_and_opponent_replays() {
    let mut chart = tap_chart(TimeUs(3_000_000));
    let tap = chart.lane_notes[Lane::Key1.index()][0].clone();
    for (id, time) in [(2, 1_000_000), (3, 1_100_000)] {
        chart.lane_notes[Lane::Scratch.index()].push(NoteEvent {
            id: NoteId(id),
            lane: Lane::Scratch,
            kind: NoteKind::Mine,
            time: TimeUs(time),
            damage: Some(8.0),
            ..tap.clone()
        });
    }
    for released in [ScratchDirection::Up, ScratchDirection::Down] {
        let other = if released == ScratchDirection::Up {
            ScratchDirection::Down
        } else {
            ScratchDirection::Up
        };
        let mut recorder = ReplayRecorder::default();
        for (kind, direction, time) in [
            (InputKind::Press, ScratchDirection::Up, 900_000),
            (InputKind::Press, ScratchDirection::Down, 940_000),
            (InputKind::Release, released, 970_000),
            (InputKind::Release, other, 1_050_000),
        ] {
            recorder.record(InputEvent {
                lane: Lane::Scratch,
                kind,
                scratch_direction: Some(direction),
                ..human_press(TimeUs(time))
            });
        }
        for opponent in [false, true] {
            let (mut session, _) = dx_human_session(chart.clone(), 0);
            let replay = ReplayPlayer::new(recorder.events.clone());
            if opponent {
                let (rival, _) = dx_human_session(chart.clone(), 0);
                session.opponent.independent = Some(independent_replay(rival, replay));
            } else {
                session.replay_player = Some(replay);
            }
            let initial = session.gauge.current().value;
            advance_at(&mut session, &mut TestAudio::default(), 1_200_000);
            let gauge = session.opponent.independent.as_ref().map_or(&session.gauge, |r| &r.gauge);
            assert_eq!(gauge.current().value, initial - 8.0);
        }
    }
}

#[test]
fn opponent_keeps_no_good_after_recalculating_replay_windows() {
    let chart = tap_chart(TimeUs(1_000_000));
    let (mut session, _) = dx_human_session(chart.clone(), 0);
    let (mut rival, _) = dx_human_session(chart, 0);
    rival.judge.window_rates.keys = [100, 100, 0];
    let mut recorder = ReplayRecorder::default();
    recorder.record(human_press(TimeUs(1_080_000)));
    session.opponent.independent =
        Some(independent_replay(rival, ReplayPlayer::new(recorder.events)));
    advance_at(&mut session, &mut TestAudio::default(), 1_080_000);
    assert_eq!(session.opponent.independent.unwrap().score.judges.slow_bad, 1);
}

#[test]
fn mine_and_tap_at_the_same_time_apply_damage_before_recovery() {
    let time = TimeUs(1_000_000);
    let mut chart = tap_chart(time);
    let tap = chart.lane_notes[Lane::Key1.index()][0].clone();
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(7),
        kind: NoteKind::Mine,
        damage: Some(8.0),
        sound: None,
        ..tap
    });
    let (mut session, keys) = dx_human_session(chart, 0);
    session.gauge.set_initial_value(100.0);
    keys.press();
    let frame = advance_at(&mut session, &mut TestAudio::default(), time.0);
    assert_eq!(frame.mine_hits.len(), 1);
    assert_eq!(session.gauge.current().value, 100.0);
    assert_eq!((session.score.bp(), session.score.combo, session.score.past_notes), (0, 1, 1));
    assert_eq!(session.score.judges.fast_pgreat, 1);
    assert_eq!(frame.judgements[0].side, TimingSide::Slow);
}

#[test]
fn duration_replay_batch_settles_earlier_miss_before_the_next_hit() {
    use bmz_chart::model::{JudgeRankKind, JudgeRankSpec};

    let mut chart = tap_chart(TimeUs(1_000_000));
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes.push(NoteEvent { id: NoteId(9), time: TimeUs(1_380_000), ..notes[0].clone() });
    chart.total_notes = 2;
    chart.end_time = TimeUs(1_380_000);
    chart.metadata.judge_rank_spec = Some(JudgeRankSpec { value: 2, kind: JudgeRankKind::BmsRank });
    let mut recorder = ReplayRecorder::default();
    recorder.record(human_press(TimeUs(1_200_000)));
    recorder.record(human_release(TimeUs(1_205_000)));
    recorder.record(human_press(TimeUs(1_380_000)));

    for updates in [vec![1_380_000], vec![1_200_000, 1_211_000, 1_380_000]] {
        let (mut session, _) = dx_human_session(chart.clone(), 0);
        session.rule_mode = RuleMode::Beatoraja;
        session.base_judge_windows =
            crate::judge::window::beatoraja_judge_windows_for_keymode(KeyMode::K7);
        session.judge = JudgeEngine::new_with_window_set_and_algorithm(
            session.base_judge_windows,
            session.rule_mode,
            crate::judge::model::JudgeAlgorithm::Duration,
        );
        session.replay_player = Some(ReplayPlayer::new(recorder.events.clone()));
        let mut audio = TestAudio::default();
        for now in updates {
            advance_at(&mut session, &mut audio, now);
        }
        assert_eq!((session.score.bp(), session.score.combo, session.score.past_notes), (2, 1, 2));
        assert_eq!(session.score.ghost, [4, 0]);
    }
}

#[test]
fn orphan_charge_end_counts_before_clear_without_failed_bp_compensation() {
    use crate::gauge::GaugeProperty;
    use crate::judge::window::judge_windows_for_keymode_and_rule_mode;
    use crate::result::PlayResult;
    use bmz_core::clear::{ClearType, GaugeType};

    for rule in [RuleMode::Lr2Oraja, RuleMode::Dx] {
        for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
            let mut chart = chart_with_hcn_long_note();
            let pair = &mut chart.long_notes[0];
            pair.mode = Some(mode);
            pair.start_time = TimeUs(1_180_000);
            pair.end_time = TimeUs(2_000_000);
            chart.end_time = pair.end_time;
            let notes = &mut chart.lane_notes[Lane::Key1.index()];
            notes[0].time = pair.start_time;
            notes[1].time = pair.end_time;
            notes.insert(
                0,
                NoteEvent {
                    id: NoteId(17),
                    kind: NoteKind::Tap,
                    time: TimeUs(1_150_000),
                    ..notes[0].clone()
                },
            );
            chart.total_notes = 2;
            let (mut session, _) = dx_human_session(chart, 0);
            session.rule_mode = rule;
            session.base_judge_windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K7, rule);
            session.judge = JudgeEngine::new_with_window_set(session.base_judge_windows, rule);
            let gauge_type =
                if rule == RuleMode::Lr2Oraja { GaugeType::ExClass } else { GaugeType::Hard };
            session.gauge = GaugeState::new_with_property_and_rule_mode(
                gauge_type,
                260.0,
                3,
                GaugeProperty::for_rule_mode(KeyMode::K7, rule, None),
                rule,
            );
            let mut audio = TestAudio::default();
            sync_judge_windows(&mut session, TimeUs(1_000_000));
            process_session_input(&mut session, human_press(TimeUs(1_000_000)));
            process_session_input(&mut session, human_release(TimeUs(1_001_000)));
            assert_eq!((session.score.bp(), session.score.past_notes), (2, 2));
            assert!(!session.judge.is_exhausted(&session.chart));
            advance_at(&mut session, &mut audio, 2_199_999);
            advance_at(&mut session, &mut audio, 2_200_000);
            assert_eq!(session.score.bp(), 2);
            let gauge_before = session.gauge.current().value;
            advance_at(&mut session, &mut audio, 2_200_001);
            assert_eq!((session.score.bp(), session.score.past_notes), (3, 3));
            let miss_damage = if rule == RuleMode::Lr2Oraja { 10.0 } else { 9.0 };
            assert_eq!(session.gauge.current().value, gauge_before - miss_damage);
            assert_eq!(
                session.judge.judged_notes[&session.chart.long_notes[0].end_note_id],
                Judge::Poor
            );
            assert!(session.judge.is_exhausted(&session.chart));
            assert!(session.scoring_closed_at.is_some());
            let result = PlayResult::from_states(
                &session.chart,
                &session.score,
                &session.gauge,
                session.state,
                false,
            );
            assert_eq!(result.clear_type, ClearType::Hard);
            assert_eq!(result.record_bp(), 3);
        }
    }
}

#[test]
fn manual_failure_adds_unprocessed_notes_after_non_consuming_bad_and_retry() {
    use crate::judge::window::judge_windows_for_keymode_and_rule_mode;
    use crate::result::PlayResult;
    use bmz_core::clear::ClearType;

    for rule in [RuleMode::Beatoraja, RuleMode::Dx] {
        let mut chart = tap_chart(TimeUs(1_000_000));
        chart.metadata.key_mode = KeyMode::K9;
        let notes = &mut chart.lane_notes[Lane::Key1.index()];
        for (id, time) in [(22, 2_000_000), (23, 3_000_000)] {
            notes.push(NoteEvent { id: NoteId(id), time: TimeUs(time), ..notes[0].clone() });
        }
        chart.total_notes = 3;
        chart.end_time = TimeUs(3_000_000);
        let (mut session, _) = dx_human_session(chart, 0);
        session.rule_mode = rule;
        session.primary_key_mode = KeyMode::K9;
        session.base_judge_windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K9, rule);
        session.judge = JudgeEngine::new_with_window_set_algorithm_and_keymode(
            session.base_judge_windows,
            rule,
            Default::default(),
            KeyMode::K9,
        );
        session.score = ScoreState::for_rule_mode(KeyMode::K9, rule);
        let initial = session.gauge.current().value;
        let bad_damage = session.gauge.current().definition.values[3];
        process_session_input(&mut session, human_press(TimeUs(850_000)));
        assert_eq!(session.gauge.current().value, initial + bad_damage);
        for time in [1_000_000, 2_000_000] {
            process_session_input(&mut session, human_release(TimeUs(time - 1)));
            process_session_input(&mut session, human_press(TimeUs(time)));
        }
        assert_eq!((session.score.bp(), session.score.past_notes), (1, 2));
        assert_eq!(session.score.ghost, [0, 0]);
        assert!(session.request_manual_stop());
        let result = PlayResult::from_states(
            &session.chart,
            &session.score,
            &session.gauge,
            session.state,
            false,
        );
        assert_eq!(result.clear_type, ClearType::Failed);
        assert_eq!(result.record_bp(), 2);
    }
}

#[test]
fn dx_negative_input_offset_scores_late_empty_poor_before_settling() {
    let end = 1_000_000;
    let (mut session, keys) =
        dx_human_session(with_trailing_invisible(tap_chart(TimeUs(end))), -20_000);
    let mut audio = TestAudio::default();
    assert_eq!(session.judge.window_set.result_settle_margin_us(), 200_000);

    // 取得時刻 E+20ms の押鍵は -20ms 補正で E ちょうどの PGREAT。
    keys.press();
    advance_at(&mut session, &mut audio, end + 20_000);
    keys.release();
    advance_at(&mut session, &mut audio, end + 30_000);
    assert!(session.judge.is_exhausted(&session.chart));
    assert_eq!(session.score.judges.slow_pgreat + session.score.judges.fast_pgreat, 1);

    // E+W は過ぎたが、取得時刻 E+W+20ms までは補正後に判定窓へ入り得る。
    advance_at(&mut session, &mut audio, end + 205_000);
    assert_eq!(session.scoring_closed_at, None);
    assert!(!result_is_settled(&session, TimeUs(end + 205_000)));

    // 最終ノーツから +210ms の押鍵は判定上 +190ms の空POORになる。
    keys.press();
    let frame = advance_at(&mut session, &mut audio, end + 210_000);
    assert_eq!(empty_poor_count(&session), 1);
    assert!(
        frame.judgements.iter().any(|event| {
            event.judge == Judge::EmptyPoor && event.time == TimeUs(end + 190_000)
        })
    );
    assert_eq!(session.scoring_closed_at, None);

    // 境界 E+W-O では、同時刻に取得した入力がまだ境界上の空POORになり得る。
    advance_at(&mut session, &mut audio, end + 220_000);
    assert_eq!(session.scoring_closed_at, None);
    assert_eq!(session.natural_result_settled_at, None);
    advance_at(&mut session, &mut audio, end + 223_000);
    assert_eq!(session.scoring_closed_at, None);
    advance_at(&mut session, &mut audio, end + 223_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(end + 223_001)));
    assert_eq!(session.natural_result_settled_at, session.scoring_closed_at);
    assert_eq!(session.state, PlayState::Playing);
    // Later offset changes and the terminal transition must not restart the effect.
    session.offsets.input_offset_us = -300_000;
    let finished_at = session.chart.end_time.0 + SESSION_END_MARGIN_US + 1;
    advance_at(&mut session, &mut audio, finished_at);
    assert_eq!(session.state, PlayState::Finished);
    assert_eq!(session.natural_result_settled_at, Some(TimeUs(end + 223_001)));
}

#[test]
fn full_replay_settles_only_after_the_processed_tick_passes_the_window() {
    // E=1100.5ms, W=200ms。audio_now=1300.6ms では 1300ms tick までしか処理していない。
    let end = 1_100_500;
    let (mut session, _keys) =
        dx_human_session(with_trailing_invisible(tap_chart(TimeUs(end))), -20_000);
    let event = |kind, time| bmz_core::replay::ReplayEvent {
        applied_after: None,
        lane: Lane::Key1,
        kind,
        time: TimeUs(time),
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    };
    session.replay_player = Some(ReplayPlayer {
        events: vec![
            event(InputKind::Press, end),
            event(InputKind::Release, end + 10_000),
            // 未処理のまま残る、記録時刻 1300.4ms の有効な空POOR入力。
            event(InputKind::Press, 1_300_400),
        ],
        ..Default::default()
    });
    let mut audio = TestAudio::default();

    advance_at(&mut session, &mut audio, 1_300_600);
    assert!(session.judge.is_exhausted(&session.chart));
    assert_eq!(session.replay_player.as_ref().unwrap().scored_until(), Some(TimeUs(1_300_000)));
    assert_eq!(empty_poor_count(&session), 0);
    // 外側の時計は E+W を超えているが、処理済み tick はまだ超えていない。
    assert_eq!(session.scoring_closed_at, None);
    assert_eq!(session.natural_result_settled_at, None);

    advance_at(&mut session, &mut audio, 1_301_000);
    assert_eq!(empty_poor_count(&session), 1);
    // 記録済み入力に現在の -20ms を再適用しないため、負オフセット分は待たない。
    assert_eq!(session.scoring_closed_at, Some(TimeUs(1_301_000)));
    assert_eq!(session.natural_result_settled_at, session.scoring_closed_at);
}

#[test]
fn settled_scoring_ignores_old_inputs_offset_changes_and_late_stop_requests() {
    let (mut session, keys) = dx_human_session(with_trailing_invisible(tap_chart(TimeUs(0))), 0);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut session, &mut audio, 0);
    keys.release();
    advance_at(&mut session, &mut audio, 10_000);
    advance_at(&mut session, &mut audio, 200_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(200_001)));

    let score = session.score.clone();
    let gauge = session.gauge.clone();
    let judgements = session.result_judgements.clone();
    let recorded = session.replay_recorder.events.clone();
    let carry = session.gauge.carry_values();
    let combo = (session.display_combo(), session.display_max_combo());

    // 確定後のオフセット変更で、取得時刻 250ms の入力が判定上 150ms の空POOR相当になる。
    session.offsets.input_offset_us = -100_000;
    audio.scheduled.clear();
    keys.press();
    let frame = advance_at(&mut session, &mut audio, 250_000);
    assert!(frame.judgements.is_empty());
    // 採点とは分けて、キービーム・skin入力・fallbackキー音は従来どおり進める。
    assert!(session.lane_keyon_started_at[Lane::Key1.index()].is_some());
    assert!(session.recent_inputs.iter().any(|input| input.source == InputSource::Human));
    assert!(frame.skin_events.iter().any(|event| matches!(
        event.kind,
        SkinRuntimeEventKind::Input(InputEvent { kind: InputKind::Press, .. })
    )));
    assert!(audio.scheduled.iter().any(|sound| sound.sound_id == SoundId(7)));
    assert!(result_is_settled(&session, TimeUs(250_000)));

    // 古い観測から送られた手動中断は、所有スレッドで再確認して無視する。
    assert!(!session.request_manual_stop());
    assert_eq!(session.state, PlayState::Playing);
    assert!(!session.manually_stopped);

    keys.release();
    advance_at(&mut session, &mut audio, 5_000_000);
    audio.scheduled.clear();
    keys.press();
    let frame = advance_at(&mut session, &mut audio, 5_200_000);
    assert!(frame.judgements.is_empty());
    assert!(audio.scheduled.iter().any(|sound| sound.sound_id == SoundId(8)));
    assert_eq!(session.state, PlayState::Playing);

    // 従来の5秒待機による Finished 遷移でも、確定時刻と採点結果を変えない。
    let finished_at = session.chart.end_time.0 + SESSION_END_MARGIN_US + 1;
    advance_at(&mut session, &mut audio, finished_at);
    assert_eq!(session.state, PlayState::Finished);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(200_001)));
    assert_eq!(format!("{:?}", session.score), format!("{score:?}"));
    assert_eq!(session.gauge.carry_values(), carry);
    assert_eq!(session.gauge.current().value, gauge.current().value);
    assert_eq!(session.result_judgements, judgements);
    assert_eq!(session.replay_recorder.events, recorded);
    assert_eq!((session.display_combo(), session.display_max_combo()), combo);
}

#[test]
fn trailing_invisible_does_not_skip_a_late_mine_or_its_input_window() {
    let mut chart = tap_chart(TimeUs(0));
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        kind: NoteKind::Mine,
        tick: ChartTick(192),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: Some(8.0),
    });
    let (mut session, keys) = dx_human_session(with_trailing_invisible(chart), -20_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut session, &mut audio, 20_000);
    keys.release();
    advance_at(&mut session, &mut audio, 30_000);
    advance_at(&mut session, &mut audio, 220_001);
    assert!(session.judge.is_exhausted(&session.chart));
    assert_eq!(session.scoring_closed_at, None);
    let gauge = session.gauge.current().value;
    keys.press();
    let frame = advance_at(&mut session, &mut audio, 1_020_000);
    assert_eq!(frame.mine_hits.len(), 1);
    assert!(session.gauge.current().value < gauge);
    advance_at(&mut session, &mut audio, 1_220_000);
    assert_eq!(session.scoring_closed_at, None);
    advance_at(&mut session, &mut audio, 1_223_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(1_223_001)));
}

#[test]
fn early_finish_replay_preserves_result_before_a_later_mine() {
    use crate::result::PlayResult;
    use bmz_core::clear::GaugeType;

    let mine_time = TimeUs(1_000_000);
    let mut chart = tap_chart(TimeUs(0));
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        kind: NoteKind::Mine,
        time: mine_time,
        damage: Some(200.0),
        ..chart.lane_notes[Lane::Key1.index()][0].clone()
    });
    chart.end_time = mine_time;

    let (mut live, keys) = dx_human_session(chart.clone(), 0);
    live.gauge = GaugeState::new(GaugeType::Hard, 160.0, chart.total_notes);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 0);
    assert!(live.judge.is_exhausted(&live.chart));
    live.play_end.request(PlayEndRequest::Finish);
    let ended = advance_at(&mut live, &mut audio, 250_500);
    assert_eq!(ended.state, PlayState::Finished);
    assert!(ended.mine_hits.is_empty());
    let saved = PlayResult::from_states(&chart, &live.score, &live.gauge, live.state, false);

    let (mut replay, _) = dx_human_session(chart.clone(), 0);
    replay.gauge = GaugeState::new(GaugeType::Hard, 160.0, chart.total_notes);
    replay.replay_player = Some(ReplayPlayer {
        kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
            ended_at: live.result_settled_at().unwrap(),
            manually_stopped: live.manually_stopped,
            finished_early: live.finished_early,
            ..Default::default()
        }),
        events: live.replay_recorder.events,
        ..Default::default()
    });
    advance_at(&mut replay, &mut audio, mine_time.0 + 20_000);
    let played = PlayResult::from_states(&chart, &replay.score, &replay.gauge, replay.state, false);
    assert_eq!((played.gauge_value, played.clear_type), (saved.gauge_value, saved.clear_type));
    assert_eq!(replay.state, PlayState::Finished);
    assert_eq!(replay.scoring_closed_at, live.scoring_closed_at);
}

fn normal_gauge_hcn_session() -> (GameSession, SharedBackend) {
    use bmz_core::clear::GaugeType;
    let (mut play, keys) = hcn_regrab_session(RuleMode::Beatoraja, false);
    play.gauge = GaugeState::new(GaugeType::Normal, 120.0, play.scored_total_notes);
    (play, keys)
}

#[test]
fn positive_offset_input_waits_for_its_judgement_time() {
    let (mut live, keys) = dx_human_session(tap_chart(TimeUs(1_000_000)), 50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 950_000);
    // The press is judged at 1.000s. Nothing scores, sounds or records before then.
    assert!(!live.judge.is_exhausted(&live.chart));
    assert!(audio.scheduled.is_empty());
    assert!(live.replay_recorder.events.is_empty());
    assert_eq!(live.lane_keyon_started_at[Lane::Key1.index()], None);
    let frame = advance_at(&mut live, &mut audio, 999_999);
    assert!(frame.judgements.is_empty());
    assert!(audio.scheduled.is_empty());

    let frame = advance_at(&mut live, &mut audio, 1_000_000);
    assert_eq!(frame.judgements[0].judge, Judge::PGreat);
    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(frame.judgements[0].time, TimeUs(1_000_000));
    assert_eq!(live.replay_recorder.events[0].time, TimeUs(1_000_000));
    assert_eq!(live.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(1_000_000)));
    // The judgement display starts with the judgement, not 50ms into its animation.
    assert_eq!(live.recent_display_judgements[0].display_time, TimeUs(1_000_000));
}

#[test]
fn overdue_input_wakes_the_owner_immediately() {
    use std::time::Duration;
    let safety = Duration::from_micros(1_800);
    let (mut session, keys) = dx_human_session(tap_chart(TimeUs(1_000_000)), 50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut session, &mut audio, 950_000);
    advance_at(&mut session, &mut audio, 999_900);
    let mut owner = crate::runtime::GameplayRuntime::new(session);
    owner.session.audio_clock.running = true;
    assert_eq!(owner.next_wake_after(safety), Duration::from_micros(100));
    owner.session.audio_clock.pause_at(TimeUs(1_000_100));
    assert_eq!(owner.next_wake_after(safety), safety);
    owner.session.audio_clock.running = true;
    assert_eq!(owner.next_wake_after(safety), Duration::ZERO);
    owner.session.state = PlayState::Failed;
    assert_eq!(owner.next_wake_after(safety), safety);
    owner.session.state = PlayState::Playing;
    advance_at(&mut owner.session, &mut audio, 1_000_100);
    owner.session.audio_clock.running = true;
    assert_eq!(owner.next_wake_after(safety), safety);
    assert_eq!(audio.scheduled.len(), 1);
}

#[test]
fn viewer_seek_discards_inputs_waiting_for_their_judgement_time() {
    let (mut live, keys) = dx_human_session(tap_chart(TimeUs(1_000_000)), 50_000);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 980_000);
    assert_eq!(live.conditional.pending_inputs.len(), 1);
    prepare_viewer_seek(&mut live, TimeUs(500_000));
    assert!(live.conditional.pending_inputs.is_empty());
    // The press due at 1.030s before the seek is not judged when playback reaches it again.
    advance_at(&mut live, &mut audio, 1_100_000);
    assert!(live.replay_recorder.events.is_empty());
    assert!(!live.judge.is_exhausted(&live.chart));
}

#[test]
fn ordinary_hcn_replay_matches_a_positive_offset_release() {
    use crate::result::PlayResult;
    // Before inputs waited for their judgement time, the live release left the HCN
    // 50ms before its replay did, and some of these releases ended 1.5 apart.
    for (release, step) in (1_300_000..=1_500_000)
        .step_by(20_000)
        .map(|t| (t, 1_000))
        .chain([(1_352_000, 2_000), (1_353_000, 3_100)])
    {
        let (mut live, keys) = normal_gauge_hcn_session();
        live.offsets.input_offset_us = 50_000;
        let mut audio = TestAudio::default();
        let mut now = 950_000;
        while now <= 2_400_000 {
            if now == 950_000 {
                keys.press();
            }
            if now == release {
                keys.release();
            }
            advance_at(&mut live, &mut audio, now);
            now += step;
        }
        advance_at(&mut live, &mut audio, 12_000_000);
        assert_eq!(live.state, PlayState::Finished);
        assert_eq!(live.replay_recorder.events[1].time, TimeUs(release + 50_000));
        let saved =
            PlayResult::from_states(&live.chart, &live.score, &live.gauge, live.state, false);
        let (mut own, _) = normal_gauge_hcn_session();
        let recording = ReplayPlayer::new(live.replay_recorder.events);
        own.opponent.independent =
            Some(independent_replay(normal_gauge_hcn_session().0, recording.clone()));
        own.replay_player = Some(recording);
        advance_at(&mut own, &mut audio, 12_000_000);
        let played = PlayResult::from_states(&own.chart, &own.score, &own.gauge, own.state, false);
        assert_eq!(
            (played.gauge_value, played.clear_type, played.record_bp()),
            (saved.gauge_value, saved.clear_type, saved.record_bp()),
            "release={release}"
        );
        let rival = own.opponent.independent.as_ref().unwrap();
        assert_eq!(rival.gauge.carry_values(), live.gauge.carry_values(), "release={release}");
        assert_eq!(own.gauge.carry_values(), live.gauge.carry_values(), "release={release}");
    }
}

#[test]
fn early_finish_replays_hcn_released_with_a_positive_offset() {
    use crate::result::PlayResult;

    // The release at 1.190s is judged at 1.240s. Exit after that judgement, and
    // once more after a 200ms HCN gauge boundary.
    for end in [1_241_000, 1_260_000] {
        let (mut live, keys) = normal_gauge_hcn_session();
        live.offsets.input_offset_us = 50_000;
        let mut audio = TestAudio::default();
        for now in (950_000..=end).step_by(1_000) {
            if now == 950_000 {
                keys.press();
            }
            if now == 1_190_000 {
                keys.release();
            }
            if now == end {
                assert!(live.judge.is_exhausted(&live.chart));
                live.play_end.request(PlayEndRequest::Finish);
            }
            advance_at(&mut live, &mut audio, now);
        }
        assert_eq!(live.state, PlayState::Finished);
        assert!(live.finished_early);
        let saved =
            PlayResult::from_states(&live.chart, &live.score, &live.gauge, live.state, false);
        assert_eq!(live.replay_recorder.events[1].time, TimeUs(1_240_000));
        let recording = ReplayPlayer {
            kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
                ended_at: live.result_settled_at().unwrap(),
                finished_early: live.finished_early,
                ..Default::default()
            }),
            ..ReplayPlayer::new(live.replay_recorder.events)
        };
        let (mut own, _) = normal_gauge_hcn_session();
        own.replay_player = Some(recording.clone());
        advance_at(&mut own, &mut audio, 2_100_000);
        let played = PlayResult::from_states(&own.chart, &own.score, &own.gauge, own.state, false);
        assert_eq!(
            (played.gauge_value, played.clear_type, played.record_bp()),
            (saved.gauge_value, saved.clear_type, saved.record_bp())
        );
        assert_eq!(own.scoring_closed_at, Some(TimeUs(end)));

        let (mut battle, _) = normal_gauge_hcn_session();
        battle.opponent.independent =
            Some(independent_replay(normal_gauge_hcn_session().0, recording));
        advance_at(&mut battle, &mut audio, 2_100_000);
        let rival = battle.opponent.independent.as_ref().unwrap();
        assert_eq!(rival.gauge.current().value, saved.gauge_value);
        assert_eq!(rival.score.bp(), saved.score.bp());
    }
}

#[test]
fn early_finish_replay_queues_the_same_bgm_tail_as_live_play() {
    let mut chart = tap_chart(TimeUs(0));
    chart.bgm_events = [150_000, 200_000, 200_001]
        .map(|time| SoundEvent { tick: ChartTick(0), time: TimeUs(time), sound: SoundId(3) })
        .into();
    let (mut live, keys) = dx_human_session(chart.clone(), 0);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut live, &mut audio, 0);
    live.play_end.request(PlayEndRequest::Finish);
    advance_at(&mut live, &mut audio, 100_000);
    assert_eq!(live.bgm_scheduler.next_index, 2);

    for (manually_stopped, rate) in [(false, 25), (false, 100), (false, 300), (true, 100)] {
        let (mut replay, _) = dx_human_session(chart.clone(), 0);
        replay.audio_clock.set_playback_rate_percent(rate);
        replay.replay_player = Some(ReplayPlayer {
            kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
                ended_at: live.result_settled_at().unwrap(),
                finished_early: !manually_stopped,
                manually_stopped,
                ..Default::default()
            }),
            ..ReplayPlayer::new(live.replay_recorder.events.clone())
        });
        advance_at(&mut replay, &mut audio, 99_000);
        advance_at(&mut replay, &mut audio, 400_000);
        assert_eq!(replay.scoring_closed_at, live.scoring_closed_at);
        assert_eq!(replay.bgm_scheduler.next_index, if manually_stopped { 0 } else { 2 });
    }
}

#[test]
fn recorded_replay_end_bounds_scoring_but_not_the_ending_clock() {
    let end = TimeUs(250_500);
    for manually_stopped in [false, true] {
        let (mut replay, _) = dx_human_session(tap_chart(TimeUs(0)), 0);
        replay.replay_player = Some(ReplayPlayer {
            kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
                ended_at: end,
                manually_stopped,
                finished_early: !manually_stopped,
                ..Default::default()
            }),
            ..Default::default()
        });
        let mut audio = TestAudio::default();
        // The update crossing the recorded stop scores and ends there.
        let ended = advance_at(&mut replay, &mut audio, 400_000);
        assert_eq!(ended.times.audio_now, end);
        let state = if manually_stopped { PlayState::Failed } else { PlayState::Finished };
        assert_eq!(ended.state, state);
        assert_eq!(replay.scoring_closed_at, Some(end));

        // The ending keeps following the clock like the recorded play.
        let ending = advance_at(&mut replay, &mut audio, 600_000);
        assert_eq!(ending.times.audio_now, TimeUs(600_000));
        assert_eq!(compute_frame_times(&replay).audio_now, TimeUs(600_000));
        assert_eq!(replay.state, state);
        assert_eq!(replay.scoring_closed_at, Some(end));
    }
}

#[test]
fn manual_stop_before_the_first_update_replays_without_scoring() {
    for end in [TimeUs(-50_000), TimeUs(250_500)] {
        let (mut live, _) = dx_human_session(tap_chart(TimeUs(0)), 0);
        let mut audio = TestAudio::default();
        live.play_end.request(PlayEndRequest::ManualStop);
        advance_at(&mut live, &mut audio, end.0);
        assert_eq!(live.replay_stop_scored_through(), Some(TimeUs(-1)));
        let (mut replay, _) = dx_human_session(tap_chart(TimeUs(0)), 0);
        replay.replay_player = Some(ReplayPlayer {
            kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
                manually_stopped: true,
                ended_at: end,
                scored_through: live.replay_stop_scored_through(),
                ..Default::default()
            }),
            ..Default::default()
        });
        advance_at(&mut replay, &mut audio, 500_000);
        assert_eq!(replay.state, PlayState::Failed);
        assert_eq!(replay.scoring_closed_at, Some(end));
        assert_eq!(replay.score.past_notes, 0);
        assert_eq!(replay.replay_player.unwrap().scored_until(), None);
    }
}

#[test]
fn manual_stop_replay_scores_input_judged_in_the_last_partial_tick() {
    // The live update at 250.3ms scored a press judged then. The stop closed
    // scoring at 250.5ms, between two 1ms replay ticks, with a note left.
    let end = TimeUs(250_500);
    let mut chart = tap_chart(TimeUs(260_000));
    let notes = &mut chart.lane_notes[Lane::Key1.index()];
    notes.push(NoteEvent { id: NoteId(2), time: TimeUs(1_000_000), ..notes[0].clone() });
    chart.total_notes += 1;
    chart.end_time = TimeUs(1_000_000);
    let (mut replay, _) = dx_human_session(chart, 0);
    replay.replay_player = Some(ReplayPlayer {
        kaleid: Some(bmz_chart::kaleid::KaleidPlayEvidence {
            ended_at: end,
            manually_stopped: true,
            ..Default::default()
        }),
        ..ReplayPlayer::new(vec![bmz_core::replay::ReplayEvent {
            applied_after: None,
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(250_300),
            device_kind: Default::default(),
            scratch_direction: None,
        }])
    });
    let mut audio = TestAudio::default();
    advance_at(&mut replay, &mut audio, 400_000);
    assert_eq!(replay.state, PlayState::Failed);
    assert!(replay.manually_stopped);
    assert_eq!(replay.scoring_closed_at, Some(end));
    assert!(replay.score.ex_score() > 0);
    assert_eq!(replay.replay_player.as_ref().unwrap().scored_until(), Some(end));
}

#[test]
fn finish_request_reaching_an_ended_session_is_discarded() {
    for ended in [PlayState::Finished, PlayState::Failed] {
        let (mut session, _) = dx_human_session(tap_chart(TimeUs(0)), 0);
        session.state = ended;
        // A stale UI observation sends Finish after the owner already ended.
        session.play_end.request(PlayEndRequest::Finish);
        let mut audio = TestAudio::default();
        advance_at(&mut session, &mut audio, 300_000);
        assert!(!session.play_end.is_pending());
        assert_eq!(session.state, ended);
        assert!(!session.finished_early);
    }
}

#[test]
fn missed_long_notes_keep_settlement_past_the_tail_in_every_mode() {
    for mode in [LongNoteMode::Ln, LongNoteMode::Cn, LongNoteMode::Hcn] {
        let mut chart = chart_with_hcn_long_note();
        chart.long_notes[0].mode = Some(mode);
        let (mut session, _keys) = dx_human_session(with_trailing_invisible(chart), 0);
        let mut audio = TestAudio::default();
        advance_at(&mut session, &mut audio, 200_001);
        let gauge = session.gauge.current().value;
        advance_at(&mut session, &mut audio, 900_000);
        assert_eq!(session.scoring_closed_at, None, "{mode:?}");
        if mode == LongNoteMode::Hcn {
            assert!(session.gauge.current().value < gauge);
        }
        advance_at(&mut session, &mut audio, 1_200_000);
        assert_eq!(session.scoring_closed_at, None, "{mode:?}");
        advance_at(&mut session, &mut audio, 1_200_001);
        assert_eq!(session.scoring_closed_at, Some(TimeUs(1_200_001)), "{mode:?}");
        let carry = session.gauge.carry_values();
        advance_at(&mut session, &mut audio, 5_000_001);
        assert_eq!(session.gauge.carry_values(), carry, "{mode:?}");
    }
}

fn independent_replay(rival: GameSession, replay: ReplayPlayer) -> BattleOpponentSession {
    BattleOpponentSession {
        conditional: Default::default(),
        judge: rival.judge,
        chart: rival.chart,
        key_mode: rival.primary_key_mode,
        scored_total_notes: rival.scored_total_notes,
        base_judge_windows: rival.base_judge_windows,
        rule_mode: rival.rule_mode,
        score: rival.score,
        gauge: rival.gauge,
        replay_player: Some(replay),
        replay_judge_rate_percent: 100,
        autoplay: None,
        display_uses_primary_arrangement: false,
        publish_display_judgements: true,
        gauge_increase_started_at: None,
        gauge_max_started_at: None,
        full_combo_started_at: None,
        lane_keyon_started_at: Default::default(),
        lane_key_hold: Default::default(),
        lane_hcn_timer: Default::default(),
        last_hcn_gauge_at: None,
    }
}

#[test]
fn independent_opponent_waits_for_its_scoring_tick_but_not_invisible_notes() {
    let (mut session, keys) = dx_human_session(with_trailing_invisible(tap_chart(TimeUs(0))), 0);
    let (rival, _) = dx_human_session(with_trailing_invisible(tap_chart(TimeUs(1_100_500))), 0);
    session.opponent.independent = Some(independent_replay(rival, ReplayPlayer::default()));
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut session, &mut audio, 0);
    advance_at(&mut session, &mut audio, 200_001);
    assert_eq!(session.scoring_closed_at, None);
    advance_at(&mut session, &mut audio, 1_300_600);
    assert_eq!(session.scoring_closed_at, None);
    advance_at(&mut session, &mut audio, 1_301_000);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(1_301_000)));
}

#[test]
fn stop_boundaries_close_scoring_without_waiting_for_the_natural_window() {
    // 手動中断はノーツが残る間だけ受け付け、その停止境界で確定する。
    let (mut session, _keys) = dx_human_session(tap_chart(TimeUs(1_000_000)), -20_000);
    let mut audio = TestAudio::default();
    advance_at(&mut session, &mut audio, 300_000);
    assert!(session.request_manual_stop());
    advance_at(&mut session, &mut audio, 310_000);
    assert_eq!(session.state, PlayState::Failed);
    assert!(session.manually_stopped);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(310_000)));
    // 遅れて届いた退出要求は、途中FAILEDを完走へ再分類しない。
    assert!(!session.request_finish());
    assert_eq!(session.state, PlayState::Failed);

    // 全ノーツ処理後の明示的な退出は、自然確定の判定窓を待たずにその境界で確定する。
    let (mut session, keys) = dx_human_session(tap_chart(TimeUs(0)), 0);
    keys.press();
    advance_at(&mut session, &mut audio, 0);
    advance_at(&mut session, &mut audio, 50_000);
    assert!(session.judge.is_exhausted(&session.chart));
    assert_eq!(session.scoring_closed_at, None);
    assert!(session.request_finish());
    advance_at(&mut session, &mut audio, 60_000);
    assert_eq!(session.state, PlayState::Finished);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(60_000)));
}

#[test]
fn full_autoplay_does_not_wait_for_the_human_input_offset() {
    let mut session = session_with_autoplay(tap_chart(TimeUs(0)));
    session.offsets.input_offset_us = -50_000;
    let mut audio = TestAudio::default();
    advance_at(&mut session, &mut audio, 0);
    assert!(session.judge.is_exhausted(&session.chart));
    advance_at(&mut session, &mut audio, 200_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(200_001)));

    // 部分オートでは、人間が担当するレーンの負オフセット分を待つ。
    let mut session = session_with_autoplay(tap_chart(TimeUs(0)));
    session.autoplay = Some(AutoplayController::for_lanes(&[Lane::Key1]));
    session.offsets.input_offset_us = -50_000;
    advance_at(&mut session, &mut audio, 0);
    advance_at(&mut session, &mut audio, 200_001);
    assert_eq!(session.scoring_closed_at, None);
    advance_at(&mut session, &mut audio, 253_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(253_001)));
}

#[test]
fn completed_failed_lamp_settles_naturally_but_a_mid_play_drop_stops_at_once() {
    // A normal completion below the border is FAILED as a lamp, not as a stop.
    let (mut session, _keys) = dx_human_session(tap_chart(TimeUs(0)), 0);
    let mut audio = TestAudio::default();
    advance_at(&mut session, &mut audio, 0);
    advance_at(&mut session, &mut audio, 200_001);
    assert!(session.judge.is_exhausted(&session.chart));
    assert_eq!(session.scoring_closed_at, Some(TimeUs(200_001)));
    assert_eq!(session.state, PlayState::Playing);
    advance_at(&mut session, &mut audio, SESSION_END_MARGIN_US + 1);
    assert_eq!(session.state, PlayState::Finished);
    let result = crate::result::PlayResult::from_states(
        &session.chart,
        &session.score,
        &session.gauge,
        session.state,
        false,
    );
    assert_eq!(result.clear_type, bmz_core::clear::ClearType::Failed);

    // A HARD gauge reaching zero stops at that update, before E + W.
    let (mut session, _keys) = dx_human_session(tap_chart(TimeUs(0)), 0);
    session.gauge = GaugeState::new(bmz_core::clear::GaugeType::Hard, 160.0, 1);
    let selected = session.gauge.selected;
    for gauge in &mut session.gauge.gauges {
        if gauge.definition.gauge_type == selected {
            gauge.value = 1.0;
        }
    }
    advance_at(&mut session, &mut audio, 0);
    advance_at(&mut session, &mut audio, 200_001);
    assert_eq!(session.state, PlayState::Failed);
    assert!(!session.manually_stopped);
    assert!(session.scoring_closed_at.is_some_and(|at| at <= TimeUs(200_001)));
}

/// 音声付きの通常スクラッチ Tap(0ms) と、終端 `E` のスクラッチ CN(BSS) を持つ DX 7KEY 譜面。
fn bss_end_chart(end: TimeUs) -> PlayableChart {
    let mut chart = tap_chart(TimeUs(0));
    let mut tap = chart.lane_notes[Lane::Key1.index()].remove(0);
    tap.lane = Lane::Scratch;
    let start = TimeUs(end.0 - 500_000);
    let long_note = |id, kind, time, tick| NoteEvent {
        id: NoteId(id),
        lane: Lane::Scratch,
        kind,
        tick: ChartTick(tick),
        time,
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    chart.lane_notes[Lane::Scratch.index()] = vec![
        tap,
        long_note(2, NoteKind::LongStart, start, 96),
        long_note(3, NoteKind::LongEnd, end, 192),
    ];
    chart.long_notes.push(bmz_chart::model::LongNotePair {
        lane: Lane::Scratch,
        style: bmz_chart::model::LongNoteStyle::ChannelPair,
        mode: Some(LongNoteMode::Cn),
        start_note_id: NoteId(2),
        end_note_id: NoteId(3),
        start_tick: ChartTick(96),
        end_tick: ChartTick(192),
        start_time: start,
        end_time: end,
        sound: None,
    });
    chart.total_notes = 2;
    chart.end_time = end;
    chart
}

fn push_scratch(keys: &SharedBackend, control: &str, kind: InputKind) {
    keys.0.lock().unwrap().push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey(control.to_string()),
        kind,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
}

#[test]
fn bss_reverse_press_stays_silent_after_the_release_settles_scoring() {
    let end = 1_000_000;
    let (mut session, keys) = dx_human_session(bss_end_chart(TimeUs(end)), 0);
    session.input_system.translator = Box::new(DefaultInputTranslator {
        binding: LaneBinding {
            entries: [("Up", ScratchDirection::Up), ("Down", ScratchDirection::Down)]
                .into_iter()
                .map(|(control, direction)| BindingEntry {
                    device: None,
                    control: PhysicalControl::KeyboardKey(control.to_string()),
                    lane: Lane::Scratch,
                    scratch_direction: Some(direction),
                })
                .collect(),
        },
    });
    let mut audio = TestAudio::default();
    assert_eq!(session.judge.window_set.result_settle_margin_us(), 200_000);

    advance_at(&mut session, &mut audio, 0);
    push_scratch(&keys, "Down", InputKind::Press);
    advance_at(&mut session, &mut audio, end - 500_000);
    // E+190ms: 始端と同方向の Release で BSS 終端を判定し、逆方向 Press の抑止を設定する。
    push_scratch(&keys, "Down", InputKind::Release);
    let frame = advance_at(&mut session, &mut audio, end + 190_000);
    assert!(frame.judgements.iter().any(|event| event.note_id == Some(NoteId(3))));
    assert!(session.judge.lanes[Lane::Scratch.index()].scratch_press_suppression.is_some());
    // E+201ms: 最大SLOW窓 200ms を超えて採点を閉じる。
    advance_at(&mut session, &mut audio, end + 201_000);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(end + 201_000)));

    // E+205ms: 抑止対象の逆方向 Press は、確定後も先行 Tap の fallback キー音を鳴らさない。
    audio.scheduled.clear();
    push_scratch(&keys, "Up", InputKind::Press);
    let frame = advance_at(&mut session, &mut audio, end + 205_000);
    assert!(frame.judgements.is_empty());
    assert!(audio.scheduled.is_empty());
    assert!(frame.skin_events.iter().any(|event| matches!(
        event.kind,
        SkinRuntimeEventKind::Input(InputEvent { kind: InputKind::Press, .. })
    )));

    // 抑止は一度きりで、次の Press は従来どおり fallback キー音を鳴らす。
    push_scratch(&keys, "Up", InputKind::Release);
    advance_at(&mut session, &mut audio, end + 230_000);
    push_scratch(&keys, "Up", InputKind::Press);
    advance_at(&mut session, &mut audio, end + 240_000);
    assert!(audio.scheduled.iter().any(|sound| sound.sound_id == SoundId(7)));
}

#[test]
fn practice_round_end_finishes_a_run_whose_notes_were_all_processed() {
    // 全ノーツ処理後・自然確定前に届いた Practice の中断要求は、完走として終える。
    let (mut session, keys) = dx_human_session(tap_chart(TimeUs(0)), 0);
    let mut audio = TestAudio::default();
    keys.press();
    advance_at(&mut session, &mut audio, 0);
    advance_at(&mut session, &mut audio, 50_000);
    assert!(session.judge.is_exhausted(&session.chart));
    assert_eq!(session.scoring_closed_at, None);
    assert!(!session.request_manual_stop());
    assert!(session.request_practice_round_end());
    assert_eq!(session.state, PlayState::Finished);
    assert!(!session.manually_stopped);
    // Practice は直後に時計を止めるため、更新を経ずに停止境界で結果を取得できる。
    assert_eq!(session.result_settled_at(), Some(TimeUs(50_000)));
    advance_at(&mut session, &mut audio, 50_000);
    assert_eq!(session.natural_result_settled_at, None);

    // ノーツが残る間は従来どおり途中中断になる。
    let (mut session, _keys) = dx_human_session(tap_chart(TimeUs(1_000_000)), 0);
    advance_at(&mut session, &mut audio, 300_000);
    assert!(session.request_practice_round_end());
    assert_eq!(session.state, PlayState::Failed);
    assert!(session.manually_stopped);
    advance_at(&mut session, &mut audio, 300_000);
    assert_eq!(session.natural_result_settled_at, None);
}

#[test]
#[ignore = "requires BMZ_RESULT_SETTLEMENT_CHART pointing to a chart with trailing invisible notes"]
fn installed_chart_settles_before_trailing_invisible_notes() {
    let path = std::env::var_os("BMZ_RESULT_SETTLEMENT_CHART").expect("chart path is required");
    let chart =
        bmz_chart::import::import_chart(std::path::Path::new(&path), None, false).unwrap().chart;
    let mut session = session_with_autoplay(chart);
    session.judge = session.judge.with_chart_index(Arc::clone(&session.chart));
    let scoring_end = session.judge.result_end_time(&session.chart);
    assert!(session.chart.end_time.0 - scoring_end.0 > 1_000_000);
    let first = session
        .chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| note.kind == NoteKind::Tap)
        .map(|note| note.time)
        .min()
        .unwrap();
    let mut audio = TestAudio::default();
    let autoplay = session.autoplay.take();
    let first_miss = first.0 + session.judge.window_set.result_settle_margin_us() + 1;
    advance_at(&mut session, &mut audio, first_miss);
    session.autoplay = autoplay;
    for now in (first_miss + 1000..=session.chart.end_time.0).step_by(1000) {
        advance_at(&mut session, &mut audio, now);
        if session.scoring_closed_at.is_some() {
            break;
        }
    }
    let settled =
        session.scoring_closed_at.expect("natural result must settle before the invisible notes");
    let margin = session.judge.window_set.result_settle_margin_us();
    assert!(settled.0 > scoring_end.0 + margin);
    assert!(settled.0 <= scoring_end.0 + margin + 1000);
    assert!(settled < session.chart.end_time);
    assert_eq!(session.natural_result_settled_at, Some(settled));
    assert_eq!(session.full_combo_started_at, None);
    let result = crate::result::PlayResult::from_states(
        &session.chart,
        &session.score,
        &session.gauge,
        session.state,
        false,
    );
    println!(
        "title={}; scoring_end_us={}; presentation_end_us={}; settled_us={}; result={:?}",
        session.chart.metadata.title,
        scoring_end.0,
        session.chart.end_time.0,
        settled.0,
        result.clear_type
    );
}
