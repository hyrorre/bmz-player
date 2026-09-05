use super::*;

#[test]
fn process_mine_passes_ignores_autoplay_lane() {
    let mut session = session_with_autoplay(chart_with_mine(TimeUs(1_000_000), 8.0));
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(900_000));

    process_mine_passes(&mut session, TimeUs(1_000_000));

    assert!(session.pending_mine_hits.is_empty());
    assert!((session.gauge.current().value - 20.0).abs() < f32::EPSILON);
}

#[test]
fn advance_session_frame_schedules_bgm_with_mix_volume() {
    let mut session = session_with_autoplay(chart_with_bgm());
    session.audio_mix.master_volume = 0.5;
    session.audio_mix.bgm_volume = 0.75;
    session.audio_mix.chart_normalization_gain = 0.5;
    session.audio_mix.normalize_chart_volume = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(audio.scheduled[0].sound_id, SoundId(3));
    assert_eq!(audio.scheduled[0].volume, 0.1875);
    assert_eq!(audio.scheduled[0].restart_policy, RestartPolicy::StopSameSound);
}

#[test]
fn bgm_scheduler_starting_at_skips_past_events_and_keeps_boundary() {
    let mut chart = chart_with_bgm();
    chart.bgm_events = vec![
        SoundEvent { tick: ChartTick(192), time: TimeUs(1_000_000), sound: SoundId(3) },
        SoundEvent { tick: ChartTick(384), time: TimeUs(2_000_000), sound: SoundId(4) },
        SoundEvent { tick: ChartTick(576), time: TimeUs(3_000_000), sound: SoundId(5) },
    ];
    let mut scheduler = BgmScheduler::starting_at(&chart, TimeUs(2_000_000));
    let mut audio = TestAudio::default();

    scheduler.schedule_until(
        &chart,
        &AudioClock::stopped(48_000),
        TimeUs(3_000_000),
        1.0,
        &mut audio,
    );

    assert_eq!(
        audio.scheduled.iter().map(|sound| sound.sound_id).collect::<Vec<_>>(),
        vec![SoundId(4), SoundId(5)]
    );
}

#[test]
fn viewer_seek_skips_past_autoplay_judgements_and_keeps_boundary_note() {
    let mut chart = chart_with_keysound();
    let mut boundary = chart.lane_notes[Lane::Key1.index()][0].clone();
    boundary.id = NoteId(2);
    boundary.tick = ChartTick(384);
    boundary.time = TimeUs(2_000_000);
    chart.lane_notes[Lane::Key1.index()].push(boundary);
    chart.total_notes = 2;
    chart.end_time = TimeUs(2_000_000);
    let mut session = session_with_autoplay(chart);
    prepare_viewer_seek(&mut session, TimeUs(2_000_000));
    session.audio_clock =
        AudioClock::with_position(48_000, 0, 2_000_000, Arc::new(AtomicU64::new(0)), true);
    let mut audio = TestAudio::default();

    let frame = advance_session_frame(&mut session, &mut audio);

    assert_eq!(frame.judgements.len(), 1);
    assert_eq!(frame.judgements[0].note_id, Some(NoteId(2)));
    assert_eq!(session.score.past_notes, 2);
    assert_eq!(session.score.combo, 2);
    assert_eq!(session.score.ex_score(), 4);
}

#[test]
fn viewer_seek_prefills_display_only_battle_score_and_gauge() {
    let mut chart = chart_with_keysound();
    let mut opponent_note = chart.lane_notes[Lane::Key1.index()][0].clone();
    opponent_note.id = NoteId(2);
    opponent_note.lane = Lane::Key8;
    chart.lane_notes[Lane::Key8.index()].push(opponent_note);
    chart.total_notes = 2;
    let mut session = session_with_autoplay(chart);
    session.scored_total_notes = 1;
    session.display_only_lane_mask[Lane::Key8.index()] = true;
    session.opponent_score = Some(ScoreState::default());
    session.opponent_gauge = Some(session.gauge.clone());
    let opponent_gauge_before = session.opponent_gauge.as_ref().unwrap().current().value;

    prepare_viewer_seek(&mut session, TimeUs(500_000));

    assert_eq!(session.score.ex_score(), 2);
    assert_eq!(session.score.past_notes, 1);
    let opponent_score = session.opponent_score.as_ref().unwrap();
    assert_eq!(opponent_score.ex_score(), 2);
    assert_eq!(opponent_score.past_notes, 1);
    assert_eq!(opponent_score.combo, 1);
    assert!(session.opponent_gauge.as_ref().unwrap().current().value > opponent_gauge_before);
    assert_eq!(session.opponent_full_combo_started_at, Some(TimeUs(500_000)));
}

#[test]
fn viewer_seek_prefills_independent_battle_opponent() {
    let opponent_chart = Arc::new(chart_with_keysound());
    let window = JudgeWindow::symmetric(16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000);
    let mut session = session_with_autoplay(chart_with_keysound());
    session.battle_opponent = Some(BattleOpponentSession {
        chart: Arc::clone(&opponent_chart),
        key_mode: opponent_chart.metadata.key_mode,
        scored_total_notes: 1,
        judge: JudgeEngine::new(window),
        base_judge_windows: JudgeWindows::uniform(window),
        rule_mode: RuleMode::Beatoraja,
        score: ScoreState::default(),
        gauge: GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, 1),
        replay_player: None,
        autoplay: Some(AutoplayController::default()),
        display_uses_primary_arrangement: true,
        publish_display_judgements: true,
        gauge_increase_started_at: None,
        gauge_max_started_at: None,
        full_combo_started_at: None,
        lane_keyon_started_at: Default::default(),
    });
    let opponent_gauge_before = session.battle_opponent.as_ref().unwrap().gauge.current().value;

    prepare_viewer_seek(&mut session, TimeUs(500_000));

    let opponent = session.battle_opponent.as_ref().unwrap();
    assert_eq!(opponent.score.ex_score(), 2);
    assert_eq!(opponent.score.past_notes, 1);
    assert_eq!(opponent.score.combo, 1);
    assert!(opponent.gauge.current().value > opponent_gauge_before);
    assert_eq!(opponent.full_combo_started_at, Some(TimeUs(500_000)));
}

#[test]
fn viewer_seek_prefills_pgreats_and_restores_crossing_hcn() {
    let mut session = session_with_autoplay(chart_with_hcn_long_note());

    prepare_viewer_seek(&mut session, TimeUs(500_000));

    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.ex_score(), 2);
    assert_eq!(session.judge.judged_notes.get(&NoteId(1)), Some(&Judge::PGreat));
    assert!(session.judge.lanes[Lane::Key1.index()].active_long.is_some());
    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(500_000)));

    session.audio_clock =
        AudioClock::with_position(48_000, 0, 1_000_000, Arc::new(AtomicU64::new(0)), true);
    let mut audio = TestAudio::default();
    advance_session_frame(&mut session, &mut audio);

    assert_eq!(session.score.past_notes, 2);
    assert_eq!(session.score.combo, 2);
    assert_eq!(session.score.ex_score(), 4);
    assert!(session.judge.lanes[Lane::Key1.index()].active_long.is_none());
}

#[test]
fn viewer_seek_prefills_completed_ln_only_at_its_end() {
    let mut session = session_with_autoplay(ln_chart_with_start_sound_and_end_sound(None));

    prepare_viewer_seek(&mut session, TimeUs(500_000));
    assert_eq!(session.score.past_notes, 0);
    assert!(session.judge.lanes[Lane::Key1.index()].active_long.is_some());

    session.audio_clock =
        AudioClock::with_position(48_000, 0, 1_000_000, Arc::new(AtomicU64::new(0)), true);
    let mut audio = TestAudio::default();
    advance_session_frame(&mut session, &mut audio);
    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.ex_score(), 2);

    let mut session = session_with_autoplay(ln_chart_with_start_sound_and_end_sound(None));
    prepare_viewer_seek(&mut session, TimeUs(1_500_000));
    assert_eq!(session.score.past_notes, 1);
    assert_eq!(session.score.combo, 1);
    assert_eq!(session.score.ex_score(), 2);
}

#[test]
fn bgm_scheduler_viewer_seek_carries_only_live_latest_bgm_voices() {
    let mut chart = chart_with_bgm();
    chart.bgm_events = vec![
        SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(10) },
        SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(11) },
        SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(12) },
        SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(13) },
        SoundEvent { tick: ChartTick(288), time: TimeUs(1_500_000), sound: SoundId(12) },
        SoundEvent { tick: ChartTick(384), time: TimeUs(2_000_000), sound: SoundId(13) },
        SoundEvent { tick: ChartTick(576), time: TimeUs(3_000_000), sound: SoundId(14) },
    ];
    let clock =
        AudioClock::with_position(48_000, 512, 2_000_000, Arc::new(AtomicU64::new(512)), true);

    let (mut scheduler, carryover) = BgmScheduler::starting_at_with_carryover(
        &chart,
        TimeUs(2_000_000),
        &clock,
        0.5,
        |sound_id| match sound_id {
            SoundId(10) => Some(3_000_000),
            SoundId(11) => Some(1_000_000),
            SoundId(12) => Some(250_000),
            SoundId(13) => Some(3_000_000),
            _ => None,
        },
    );

    assert_eq!(carryover.len(), 1);
    assert_eq!(carryover[0].sound_id, SoundId(10));
    assert_eq!(carryover[0].start_frame, 512);
    assert_eq!(carryover[0].sample_offset_frames, 96_000);
    assert_eq!(carryover[0].volume, 0.5);
    assert_eq!(carryover[0].restart_policy, RestartPolicy::StopSameSound);

    let mut audio = TestAudio::default();
    scheduler.schedule_until(&chart, &clock, TimeUs(3_000_000), 1.0, &mut audio);
    assert_eq!(
        audio.scheduled.iter().map(|sound| sound.sound_id).collect::<Vec<_>>(),
        vec![SoundId(13), SoundId(14)]
    );
}

#[test]
fn auto_keysound_plays_note_sounds_without_input() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.autoplay = None;
    session.audio_mix.master_volume = 0.5;
    session.audio_mix.key_volume = 0.25;
    session.audio_mix.chart_normalization_gain = 0.5;
    session.audio_mix.normalize_chart_volume = true;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // 押鍵ゼロでも譜面の生タイミングでキー音が鳴る。音量は key_volume を使う。
    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
    assert_eq!(audio.scheduled[0].start_frame, 0);
    assert_eq!(audio.scheduled[0].volume, 0.0625);
    assert_eq!(audio.scheduled[0].restart_policy, RestartPolicy::StopSameSound);
}

#[test]
fn auto_keysound_suppresses_hit_keysounds() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    let frame = advance_session_frame(&mut session, &mut audio);

    // オートプレイでノーツは判定されるが、キー音は自動再生の 1 回だけ
    // (押鍵経路は抑制されるので二重再生しない)。
    assert_eq!(frame.judgements.len(), 1);
    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
}

#[test]
fn auto_keysound_skips_display_only_lanes() {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key2.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key2,
        kind: NoteKind::Tap,
        tick: ChartTick(0),
        time: TimeUs(0),
        sound: Some(SoundId(9)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.display_only_lane_mask[Lane::Key2.index()] = true;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // 主プレイヤーレーン (Key1) は鳴り、display-only レーン (Key2) は鳴らない。
    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
}

#[test]
fn auto_keysound_does_not_schedule_invisible_notes() {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        kind: NoteKind::Invisible,
        tick: ChartTick(0),
        time: TimeUs(50_000),
        sound: Some(SoundId(8)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // Invisible は押鍵時の fallback キー音候補であり、譜面タイミングでの自動再生対象外。
    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
}

#[test]
fn practice_auto_keysound_does_not_catch_up_notes_before_section() {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(2),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(0),
        time: TimeUs(50_000),
        sound: Some(SoundId(8)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    // 区間 [40ms, 100ms) を練習: time 0 のノーツは Invisible へ退避される。
    bmz_chart::practice::apply_practice_section(&mut chart, TimeUs(40_000), TimeUs(100_000));
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // 区間前 (time 0) のキー音は catch-up されず、区間内 (time 50ms) のみ鳴る。
    assert_eq!(audio.scheduled.len(), 1);
    assert_eq!(audio.scheduled[0].sound_id, SoundId(8));
}

#[test]
fn auto_keysound_schedules_layered_note_sounds() {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()][0].layered_sounds = vec![SoundId(8)];
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    let mut sounds: Vec<_> = audio.scheduled.iter().map(|s| s.sound_id).collect();
    sounds.sort_by_key(|s| s.0);
    assert_eq!(sounds, vec![SoundId(7), SoundId(8)]);
}

#[test]
fn auto_keysound_schedules_long_start_and_long_end() {
    let mut chart = ln_chart_with_start_sound_and_end_sound(Some(SoundId(8)));
    // LongEnd を先読み窓 (100ms) 内へ寄せる。
    chart.lane_notes[Lane::Key1.index()][1].time = TimeUs(50_000);
    chart.long_notes[0].end_time = TimeUs(50_000);
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    let mut sounds: Vec<_> = audio.scheduled.iter().map(|s| s.sound_id).collect();
    sounds.sort_by_key(|s| s.0);
    // LongStart (SoundId 7) と LongEnd (SoundId 8) がそれぞれ 1 回ずつ。
    assert_eq!(sounds, vec![SoundId(7), SoundId(8)]);
}

#[test]
fn auto_keysound_applies_key_volume_events() {
    let mut chart = chart_with_keysound();
    chart.key_volume_events.push(bmz_chart::model::ChartVolumeEvent {
        tick: ChartTick(0),
        time: TimeUs(0),
        value: 128,
    });
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    session.audio_mix.master_volume = 1.0;
    session.audio_mix.key_volume = 1.0;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // 譜面のチャンネル音量 (#xxx07 相当) が自動再生にも反映される。
    let expected = 128.0 / 255.0;
    assert_eq!(audio.scheduled.len(), 1);
    assert!((audio.scheduled[0].volume - expected).abs() < 0.001);
}

#[test]
fn auto_keysound_restart_resets_lane_cursors() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);
    assert_eq!(audio.scheduled.len(), 1);
    assert!(session.auto_keysound_scheduler.next_note_index[Lane::Key1.index()] > 0);

    // リトライ等でセッションを作り直すのと同じく、スケジューラも Default に戻す。
    // カーソルが 0 に戻り、同じノーツを先頭から再スケジュールできることを確認する。
    session.auto_keysound_scheduler = AutoKeysoundScheduler::default();
    let mut audio_after_restart = TestAudio::default();

    advance_session_frame(&mut session, &mut audio_after_restart);

    assert_eq!(audio_after_restart.scheduled.len(), 1);
    assert_eq!(audio_after_restart.scheduled[0].sound_id, SoundId(7));
}

#[test]
fn advance_session_frame_applies_chart_volume_channels() {
    let mut chart = chart_with_keysound();
    chart.key_volume_events.push(bmz_chart::model::ChartVolumeEvent {
        tick: ChartTick(0),
        time: TimeUs(0),
        value: 128,
    });
    let mut session = session_with_autoplay(chart);
    session.audio_mix.master_volume = 1.0;
    session.audio_mix.key_volume = 1.0;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    let expected = 128.0 / 255.0;
    assert!((audio.scheduled[0].volume - expected).abs() < 0.001);

    let mut bgm_chart = chart_with_bgm();
    bgm_chart.bgm_volume_events.push(bmz_chart::model::ChartVolumeEvent {
        tick: ChartTick(0),
        time: TimeUs(0),
        value: 64,
    });
    let mut bgm_session = session_with_autoplay(bgm_chart);
    bgm_session.audio_mix.master_volume = 1.0;
    bgm_session.audio_mix.bgm_volume = 1.0;
    let mut bgm_audio = TestAudio::default();

    advance_session_frame(&mut bgm_session, &mut bgm_audio);

    let expected_bgm = 64.0 / 255.0;
    assert!((bgm_audio.scheduled[0].volume - expected_bgm).abs() < 0.001);
}

#[test]
fn update_recent_judgements_expires_old_events() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let event = JudgementEvent {
        note_id: Some(NoteId(1)),
        lane: Lane::Key1,
        judge: Judge::PGreat,
        side: TimingSide::Slow,
        delta: TimeUs(0),
        time: TimeUs(0),
        affects_score: true,
    };

    update_recent_judgements(&mut session, &[event], TimeUs(0));
    update_recent_judgements(&mut session, &[], TimeUs(JUDGEMENT_DISPLAY_US + 1));

    assert!(session.recent_judgements.is_empty());
}

#[test]
fn advance_session_frame_skips_human_inputs_when_replay_active() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let chart = chart_with_keysound();
    let mut session = session_with_autoplay(chart);
    // 入力バインディングを設定して Z キーを Key1 にマップ
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("Z".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
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
    session.replay_player = Some(crate::replay::ReplayPlayer::default());
    session.autoplay = None;
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // 人間入力は judge にも recorder にも渡らない
    assert_eq!(session.score.judges.fast_pgreat + session.score.judges.slow_pgreat, 0);
    assert_eq!(session.score.judges.fast_great + session.score.judges.slow_great, 0);
    assert!(session.replay_recorder.events.is_empty());
    // recent_inputs だけは Press が反映される (視覚エフェクト用)
    assert_eq!(session.recent_inputs.len(), 1);
}

#[test]
fn advance_session_frame_skips_human_inputs_when_autoplay_active() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let chart = chart_with_keysound();
    let mut session = session_with_autoplay(chart);
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("Z".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
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
    let mut audio = TestAudio::default();

    advance_session_frame(&mut session, &mut audio);

    // オートプレイ中は人間入力を recorder に渡さない。
    assert!(session.replay_recorder.events.is_empty());
    // 人間のキー入力は recent_inputs(キービーム)にも反映されない。
    // recent_inputs に乗るのは autoplay のノーツ処理入力のみ。
    assert!(session.recent_inputs.iter().all(|i| i.source == InputSource::Auto));
    assert!(
        session.recent_inputs.iter().all(|i| i.source != InputSource::Human),
        "human key press must not produce a keybeam during autoplay",
    );
}

#[test]
fn process_autoplay_inputs_flashes_keybeam_on_note_processing() {
    let mut session = session_with_autoplay(chart_with_keysound());

    // chart_with_keysound のノーツは time=0 / Key1。audio_now=0 で処理される。
    let judgements = process_autoplay_inputs(&mut session, TimeUs(0));

    assert!(!judgements.is_empty(), "autoplay should judge the note");
    // ノーツ処理に伴って autoplay 入力が recent_inputs に積まれる(キービーム発火)。
    assert_eq!(session.recent_inputs.len(), 1);
    assert_eq!(session.recent_inputs[0].lane, Lane::Key1);
    assert_eq!(session.recent_inputs[0].source, InputSource::Auto);
}

#[test]
fn update_lane_key_states_press_sets_keyon_clears_keyoff() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.lane_keyoff_started_at[Lane::Key1.index()] = Some(TimeUs(1_000));

    let inputs = [InputEvent {
        lane: Lane::Key1,
        kind: InputKind::Press,
        time: TimeUs(5_000),
        source: InputSource::Human,
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    }];
    update_lane_key_states(&mut session, &inputs);

    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(5_000)));
    assert_eq!(session.lane_keyoff_started_at[Lane::Key1.index()], None);
    // Human source の Press は自動 release を予約しない (押し続け対応)。
    assert_eq!(session.lane_auto_release_at[Lane::Key1.index()], None);
}

#[test]
fn update_lane_key_states_tracks_scratch_direction_until_release() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let press = [InputEvent {
        lane: Lane::Scratch,
        kind: InputKind::Press,
        time: TimeUs(5_000),
        source: InputSource::Human,
        device_kind: InputDeviceKind::Controller,
        scratch_direction: Some(ScratchDirection::Up),
    }];

    update_lane_key_states(&mut session, &press);

    assert_eq!(session.lane_scratch_direction[Lane::Scratch.index()], Some(ScratchDirection::Up));

    let release = [InputEvent {
        lane: Lane::Scratch,
        kind: InputKind::Release,
        time: TimeUs(10_000),
        source: InputSource::Human,
        device_kind: InputDeviceKind::Controller,
        scratch_direction: Some(ScratchDirection::Up),
    }];
    update_lane_key_states(&mut session, &release);

    assert_eq!(session.lane_scratch_direction[Lane::Scratch.index()], None);
}

#[test]
fn update_scratch_angle_phase_accumulates_directionless_scratch_until_release() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.scratch_angle_last_render_at = Some(TimeUs(0));
    session.lane_keyon_started_at[Lane::Scratch.index()] = Some(TimeUs(0));

    update_scratch_angle_phase(&mut session, TimeUs(1_000_000));

    assert_eq!(session.lane_scratch_angle_delta_ms[Lane::Scratch.index()], 160);

    update_lane_key_states(
        &mut session,
        &[InputEvent {
            lane: Lane::Scratch,
            kind: InputKind::Release,
            time: TimeUs(1_000_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }],
    );
    update_scratch_angle_phase(&mut session, TimeUs(1_500_000));

    assert_eq!(session.lane_scratch_angle_delta_ms[Lane::Scratch.index()], 160);
}

#[test]
fn rebase_pre_ready_visual_times_keeps_timer_elapsed_across_clock_reset() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.scratch_angle_last_render_at = Some(TimeUs(5_000_000));
    session.lane_keyon_started_at[Lane::Key1.index()] = Some(TimeUs(2_000_000));
    session.lane_keyoff_started_at[Lane::Key2.index()] = Some(TimeUs(3_000_000));
    session.recent_inputs.push(InputEvent {
        lane: Lane::Key1,
        kind: InputKind::Press,
        time: TimeUs(4_900_000),
        source: InputSource::Human,
        device_kind: InputDeviceKind::Keyboard,
        scratch_direction: None,
    });

    rebase_pre_ready_visual_times(&mut session, TimeUs(-1_000_000));

    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(-4_000_000)));
    assert_eq!(session.lane_keyoff_started_at[Lane::Key2.index()], Some(TimeUs(-3_000_000)));
    assert_eq!(session.recent_inputs[0].time, TimeUs(-1_100_000));
    assert_eq!(session.scratch_angle_last_render_at, Some(TimeUs(-1_000_000)));

    // READY/PLAYに移行しても押下状態は保たれ、実際のReleaseでのみ解除される。
    update_lane_key_states(
        &mut session,
        &[InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Release,
            time: TimeUs(-500_000),
            source: InputSource::Human,
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        }],
    );
    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], None);
    assert_eq!(session.lane_keyoff_started_at[Lane::Key1.index()], Some(TimeUs(-500_000)));
}

#[test]
fn sync_input_timestamp_anchor_tracks_running_audio_clock() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.input_timestamp_anchor =
        Some(InputTimestampAnchor { monotonic_ns: 123, audio_time: TimeUs(456) });

    sync_input_timestamp_anchor(&mut session, TimeUs(1_234_567));

    assert!(session.input_timestamp_anchor.is_none());

    session.audio_clock.running = true;
    sync_input_timestamp_anchor(&mut session, TimeUs(1_234_567));

    let anchor = session.input_timestamp_anchor.unwrap();
    assert_eq!(anchor.audio_time, TimeUs(1_234_567));
    assert!(anchor.monotonic_ns <= monotonic_timestamp_ns());
}

#[test]
fn process_human_inputs_uses_monotonic_event_time() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let mut session = session_with_autoplay(chart_with_bgm());
    session.autoplay = None;
    session.audio_clock.running = true;
    session.audio_clock.current_frame = Arc::new(AtomicU64::new(48_000));
    session.input_timestamp_anchor =
        Some(InputTimestampAnchor { monotonic_ns: 2_000_000, audio_time: TimeUs(1_000_000) });
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("Z".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::MonotonicNs(1_500_000),
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
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

    let judgements = process_human_inputs(&mut session);

    assert!(judgements.is_empty());
    assert_eq!(session.replay_recorder.events[0].time, TimeUs(999_500));
    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(999_500)));
}

#[test]
fn process_human_inputs_does_not_record_display_only_opponent_lanes() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let mut session = session_with_autoplay(chart_with_bgm());
    session.autoplay = None;
    session.display_only_lane_mask[Lane::Key8.index()] = true;
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("M".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
        translator: Box::new(DefaultInputTranslator {
            binding: LaneBinding {
                entries: vec![BindingEntry {
                    device: None,
                    control: PhysicalControl::KeyboardKey("M".to_string()),
                    lane: Lane::Key8,
                    scratch_direction: None,
                }],
            },
        }),
        bounce_filter: Default::default(),
    };

    process_human_inputs(&mut session);

    assert!(session.replay_recorder.events.is_empty());
    assert_eq!(session.recent_inputs[0].lane, Lane::Key8);
}

#[test]
fn process_human_inputs_records_projected_source_lane() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let mut session = session_with_autoplay(chart_with_bgm());
    session.autoplay = None;
    let mut record_to_source = [None; LANE_COUNT];
    record_to_source[Lane::Key9.index()] = Some(Lane::Scratch);
    session.replay_lane_projection = Some(ReplayLaneProjection {
        record_to_source,
        playback_to_chart: std::array::from_fn(|index| Lane::ALL[index]),
        playback_scratch_lane_mask: [false; LANE_COUNT],
        active_playback_scratch_lane: None,
    });
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("M".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
        translator: Box::new(DefaultInputTranslator {
            binding: LaneBinding {
                entries: vec![BindingEntry {
                    device: None,
                    control: PhysicalControl::KeyboardKey("M".to_string()),
                    lane: Lane::Key9,
                    scratch_direction: None,
                }],
            },
        }),
        bounce_filter: Default::default(),
    };

    process_human_inputs(&mut session);

    assert_eq!(session.recent_inputs[0].lane, Lane::Key9);
    assert_eq!(session.replay_recorder.events[0].lane, Lane::Scratch);
}

#[test]
fn drain_pre_ready_visual_inputs_updates_lane_key_states_without_judging() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let mut session = session_with_autoplay(chart_with_keysound());
    session.autoplay = None;
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("Z".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
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

    drain_pre_ready_visual_inputs(&mut session, TimeUs(2_000_000));

    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], Some(TimeUs(2_000_000)));
    assert_eq!(session.recent_inputs.len(), 1);
    assert_eq!(session.score.past_notes, 0);
}

#[test]
fn drain_pre_ready_visual_inputs_advances_scratch_phase() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let mut session = session_with_autoplay(chart_with_keysound());
    session.autoplay = None;
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::GamepadButton("scratch-up".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
        translator: Box::new(DefaultInputTranslator {
            binding: LaneBinding {
                entries: vec![BindingEntry {
                    device: None,
                    control: PhysicalControl::GamepadButton("scratch-up".to_string()),
                    lane: Lane::Scratch,
                    scratch_direction: Some(ScratchDirection::Up),
                }],
            },
        }),
        bounce_filter: Default::default(),
    };

    drain_pre_ready_visual_inputs(&mut session, TimeUs(2_000_000));
    drain_pre_ready_visual_inputs(&mut session, TimeUs(2_500_000));

    assert_eq!(session.lane_scratch_angle_delta_ms[Lane::Scratch.index()], 1_000);
    assert_eq!(session.scratch_angle_last_render_at, Some(TimeUs(2_500_000)));
    assert_eq!(session.score.past_notes, 0);
    assert!(session.replay_recorder.events.is_empty());
}

#[test]
fn drain_pre_ready_visual_inputs_discards_human_inputs_during_full_autoplay() {
    use crate::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use crate::input::binding::{BindingEntry, LaneBinding};

    let mut session = session_with_autoplay(chart_with_keysound());
    let mut backend = BufferedInputBackend::default();
    backend.push(DeviceInputEvent {
        device: DeviceId(1),
        control: PhysicalControl::KeyboardKey("Z".to_string()),
        kind: InputKind::Press,
        timestamp: DeviceTimestamp::Unknown,
        bounce_policy: Default::default(),
    });
    session.input_system = InputSystem {
        backend: Box::new(backend),
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

    drain_pre_ready_visual_inputs(&mut session, TimeUs(2_000_000));

    assert_eq!(session.lane_keyon_started_at[Lane::Key1.index()], None);
    assert!(session.recent_inputs.is_empty());
    assert_eq!(session.score.past_notes, 0);
    assert!(session.replay_recorder.events.is_empty());
}
