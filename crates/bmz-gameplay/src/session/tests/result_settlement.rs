//! 採点の自然確定と、確定後に結果が変わらないことの回帰テスト。
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
    advance_at(&mut session, &mut audio, end + 220_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(end + 220_001)));
    assert_eq!(session.natural_result_settled_at, session.scoring_closed_at);
    assert_eq!(session.state, PlayState::Playing);
    // Later offset changes and the terminal transition must not restart the effect.
    session.offsets.input_offset_us = -300_000;
    let finished_at = session.chart.end_time.0 + SESSION_END_MARGIN_US + 1;
    advance_at(&mut session, &mut audio, finished_at);
    assert_eq!(session.state, PlayState::Finished);
    assert_eq!(session.natural_result_settled_at, Some(TimeUs(end + 220_001)));
}

#[test]
fn full_replay_settles_only_after_the_processed_tick_passes_the_window() {
    // E=1100.5ms, W=200ms。audio_now=1300.6ms では 1300ms tick までしか処理していない。
    let end = 1_100_500;
    let (mut session, _keys) =
        dx_human_session(with_trailing_invisible(tap_chart(TimeUs(end))), -20_000);
    let event = |kind, time| bmz_core::replay::ReplayEvent {
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
    let mine_window = session.judge.window_set.note.mine_hit_us;
    keys.press();
    let frame = advance_at(&mut session, &mut audio, 1_000_000 + mine_window + 20_000);
    assert_eq!(frame.mine_hits.len(), 1);
    assert!(session.gauge.current().value < gauge);
    advance_at(&mut session, &mut audio, 1_220_000);
    assert_eq!(session.scoring_closed_at, None);
    advance_at(&mut session, &mut audio, 1_220_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(1_220_001)));
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

#[test]
fn independent_opponent_waits_for_its_scoring_tick_but_not_invisible_notes() {
    let (mut session, keys) = dx_human_session(with_trailing_invisible(tap_chart(TimeUs(0))), 0);
    let (rival, _) = dx_human_session(with_trailing_invisible(tap_chart(TimeUs(1_100_500))), 0);
    session.opponent.independent = Some(BattleOpponentSession {
        conditional: Default::default(),
        judge: rival.judge,
        chart: rival.chart,
        key_mode: rival.primary_key_mode,
        scored_total_notes: rival.scored_total_notes,
        base_judge_windows: rival.base_judge_windows,
        rule_mode: rival.rule_mode,
        score: rival.score,
        gauge: rival.gauge,
        replay_player: Some(ReplayPlayer::default()),
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
    advance_at(&mut session, &mut audio, 250_001);
    assert_eq!(session.scoring_closed_at, Some(TimeUs(250_001)));
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
