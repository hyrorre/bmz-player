use super::*;
use bmz_audio::{
    command::CommandedAudioEngine,
    queue::{AudioScheduler, ScheduledSound},
    sample::DecodedSample,
};
use bmz_core::{
    clear::{ClearType, GaugeType},
    judge::Judge,
};
use bmz_gameplay::gauge::GaugeState;

const SAMPLE_RATE: u32 = 1_000;

fn sample(value: f32) -> DecodedSample {
    // 45 seconds, independent of files, devices and wall-clock sleeps.
    DecodedSample { channels: 1, sample_rate: SAMPLE_RATE, frames: vec![value; 45_000] }
}

fn fixture(capacity: usize) -> (GameplayRuntime, AudioEngineHandle, SharedInputBackend) {
    let input = SharedInputBackend::default();
    let mut session = prepared(input.clone());
    session.audio_clock =
        AudioClock::with_position(SAMPLE_RATE, 0, 0, Arc::new(AtomicU64::new(0)), true);
    Arc::make_mut(&mut session.chart).bgm_events = vec![
        SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(2) },
        SoundEvent { tick: ChartTick(125), time: TimeUs(650_000), sound: SoundId(3) },
    ];
    let mut engine = AudioEngine::new(SAMPLE_RATE);
    for id in 1..=3 {
        engine.insert_sample(SoundId(id), sample(0.125));
    }
    (GameplayRuntime::new(session), AudioEngineHandle::with_capacity(engine, capacity), input)
}

#[test]
fn optional_play_behaviors_wait_for_events_not_pcm_and_preserve_terminal_audio_rules() {
    for enabled in [false, true] {
        for terminal in [PlayState::Playing, PlayState::Finished, PlayState::Failed] {
            let (mut runtime, audio, input) = fixture(64);
            runtime.session.result_wait_end_time = enabled.then_some(TimeUs(8_000_000));
            runtime.session.play_keysound_on_miss = enabled;
            let mut processor = audio.processor();
            start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
            runtime.session.audio_clock.current_frame.store(2_000, Ordering::Release);
            runtime.advance(&audio);
            assert!(runtime.session.judge.is_exhausted(&runtime.session.chart));
            assert!(bmz_gameplay::session::result_is_settled(&runtime.session, TimeUs(2_000_000)));
            if terminal != PlayState::Playing {
                runtime.session.state = terminal;
            }
            runtime.session.audio_clock.current_frame.store(7_000, Ordering::Release);
            let actual = runtime.advance(&audio).state;
            let expected = if terminal != PlayState::Playing {
                terminal
            } else if enabled {
                PlayState::Playing
            } else {
                PlayState::Finished
            };
            assert_eq!(actual, expected);
            assert_eq!(render(&mut processor, 7_000)[0] == 0.0, terminal == PlayState::Failed);
            if expected == PlayState::Playing {
                runtime.session.audio_clock.current_frame.store(13_001, Ordering::Release);
                assert_eq!(runtime.advance(&audio).state, PlayState::Finished);
                // The 45s PCM still has a tail: wait is an event-time deadline.
                assert!(render(&mut processor, 13_001)[0] > 0.0);
                let result =
                    crate::screens::play_finish::play_result_from_session(&runtime.session);
                assert_eq!(result.clear_type, ClearType::Failed);
            }
        }
    }
}

#[test]
fn optional_play_behaviors_missed_keysound_reaches_audio_in_normal_and_replay() {
    for replay in [false, true] {
        for enabled in [false, true] {
            let (mut runtime, audio, _) = fixture(64);
            // The shared fixture uses one sound ID for every lane. Isolate
            // the HCN voice so its existing miss mute does not silence taps.
            Arc::make_mut(&mut runtime.session.chart).lane_notes[Lane::Key4.index()][0].sound =
                Some(SoundId(3));
            runtime.session.play_keysound_on_miss = enabled;
            runtime.session.audio_mix.bgm_volume = 0.0;
            if replay {
                runtime.session.replay_player = Some(bmz_gameplay::replay::ReplayPlayer {
                    events: Vec::new(),
                    next_index: 0,
                    next_scoring_time: None,
                });
            }
            let mut processor = audio.processor();
            runtime.advance(&audio);
            assert_eq!(render(&mut processor, 0), [0.0; 2]);
            runtime.session.audio_clock.current_frame.store(600, Ordering::Release);
            let frame = runtime.advance(&audio);
            assert!(frame.judgements.iter().any(|event| event.judge == Judge::Poor));
            assert_eq!(render(&mut processor, 600)[0] > 0.0, enabled, "replay={replay}");
            runtime.session.state = PlayState::Failed;
            runtime.advance(&audio);
            assert_eq!(render(&mut processor, 601), [0.0; 2]);
        }
    }
}

#[test]
fn optional_play_behaviors_hcn_miss_plays_then_mutes_and_resumes_on_press() {
    for replay in [false, true] {
        for enabled in [false, true] {
            let (mut runtime, audio, input) = fixture(64);
            // Only the HCN head has a keysound in this fixture.
            for note in Arc::make_mut(&mut runtime.session.chart).lane_notes.iter_mut().flatten() {
                note.sound = (note.id == NoteId(9)).then_some(SoundId(1));
            }
            runtime.session.play_keysound_on_miss = enabled;
            runtime.session.audio_mix.bgm_volume = 0.0;
            if replay {
                runtime.session.replay_player = Some(bmz_gameplay::replay::ReplayPlayer {
                    events: vec![bmz_core::replay::ReplayEvent {
                        lane: Lane::Key4,
                        kind: InputKind::Press,
                        time: TimeUs(950_000),
                        device_kind: bmz_core::input::InputDeviceKind::Keyboard,
                        scratch_direction: None,
                    }],
                    next_index: 0,
                    next_scoring_time: None,
                });
            }
            let mut processor = audio.processor();
            let mut missed_at = None;
            // Replay advances in 1ms scoring steps; observe the update that
            // actually produces the head POOR, not an arbitrary later update.
            for at in 0..600 {
                runtime.session.audio_clock.current_frame.store(at, Ordering::Release);
                let frame = runtime.advance(&audio);
                let output = render(&mut processor, at);
                if frame
                    .judgements
                    .iter()
                    .any(|event| event.note_id == Some(NoteId(9)) && event.judge == Judge::Poor)
                {
                    assert_eq!(output[0] > 0.0, enabled, "first miss replay={replay}");
                    missed_at = Some(at);
                    break;
                }
                assert_eq!(output, [0.0; 2]);
            }
            let next = missed_at.expect("HCN head must become POOR") + 1;
            runtime.session.audio_clock.current_frame.store(next, Ordering::Release);
            runtime.advance(&audio);
            assert_eq!(render(&mut processor, next), [0.0; 2]);
            if !replay {
                for event in events(950) {
                    input.push_shared_event(event);
                }
            }
            runtime.session.audio_clock.current_frame.store(950, Ordering::Release);
            runtime.advance(&audio);
            assert_eq!(render(&mut processor, 950)[0] > 0.0, enabled, "resume replay={replay}");
        }
    }
}

fn render(processor: &mut CommandedAudioEngine, frame: u64) -> [f32; 2] {
    let mut output = [0.0; 2];
    assert!(processor.render_stereo(frame, &mut output));
    output
}

fn start_long_bgm_and_keys(
    runtime: &mut GameplayRuntime,
    audio: &AudioEngineHandle,
    input: &SharedInputBackend,
    processor: &mut CommandedAudioEngine,
) {
    runtime.advance(audio);
    let bgm = render(processor, 0)[0];
    assert!(bgm > 0.0);
    runtime.session.audio_clock.current_frame.store(100, Ordering::Release);
    for event in events(100) {
        input.push_shared_event(event);
    }
    let frame = runtime.advance(audio);
    assert!(!frame.judgements.is_empty());
    assert!(render(processor, 100)[0] > bgm, "BGM and multiple key voices must be active");
    assert!(audio.schedule_sound(ScheduledSound::one_shot(10_000, SoundId(3), 1.0, 0.0)));
    processor.apply_pending_commands_for_tests();
}

#[test]
fn manual_failed_command_stops_audio_without_render_poll_or_system_output() {
    let (mut runtime, audio, input) = fixture(64);
    let mut processor = audio.processor();
    start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
    let render_config = config(&runtime.session, Arc::new(RuntimeProbe::default()));
    assert!(render_config.effects.is_none());
    let mut client = GameplayClient::new(runtime.session);
    client.start(audio.clone(), render_config).unwrap();
    // Hold publication access: failure and audio stop must not wait for rendering.
    let latest = client.worker.as_ref().unwrap().latest.clone();
    let publication_lock = latest.lock().unwrap();
    // Same worker edit as stop_play_like_escape (Esc / E1+E2).
    assert!(client.edit(|session| session.state = PlayState::Failed));
    wait_until(|| render(&mut processor, 101) == [0.0; 2]);
    assert!(client.edit(|session| session.state = PlayState::Failed));
    assert!(!audio.play_now(SoundId(1), 1.0, false));
    assert_eq!(render(&mut processor, 10_000), [0.0; 2]);
    drop(publication_lock);
    wait_until(|| client.poll().is_some_and(|frame| frame.state == PlayState::Failed));
    client.shutdown();
    // RESULT draining changes source kind only; retirement cannot cancel stop.
    assert_eq!(render(&mut processor, 20_000), [0.0; 2]);

    let (rate, bank) = audio.clone_sample_bank().unwrap();
    let retry = AudioEngineHandle::new(AudioEngine::with_sample_bank(rate, bank));
    let (mut next, _, next_input) = fixture(64);
    let mut next_processor = retry.processor();
    start_long_bgm_and_keys(&mut next, &retry, &next_input, &mut next_processor);
    audio.stop_playback();
    assert!(render(&mut next_processor, 101)[0] > 0.0);
    assert_eq!(render(&mut processor, 20_001), [0.0; 2]);
}

#[test]
fn gauge_failure_discards_failure_frame_schedule_and_all_existing_voices() {
    for gauge in [GaugeType::Hard, GaugeType::ExHard, GaugeType::Hazard, GaugeType::Class] {
        let (mut runtime, audio, input) = fixture(64);
        let mut processor = audio.processor();
        start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
        runtime.session.gauge = GaugeState::new(gauge, 160.0, 8);
        runtime.session.gauge.set_initial_value(0.1);
        // Missing the next notes depletes the gauge in the same advance that
        // schedules the BGM at 650ms. Class also covers course-stage failure.
        runtime.session.audio_clock.current_frame.store(600, Ordering::Release);
        let frame = runtime.advance(&audio);
        assert_eq!(frame.state, PlayState::Failed, "{gauge:?}");
        assert!(frame.judgements.iter().any(|event| event.judge == Judge::Poor));
        assert!(runtime.session.bgm_scheduler.is_done(&runtime.session.chart));
        assert!(runtime.pending_audio.is_empty());
        assert!(runtime.pending_keysound_volumes.is_empty());
        for at in [600, 650, 10_000, 20_000] {
            assert_eq!(render(&mut processor, at), [0.0; 2], "{gauge:?} at {at}");
        }
    }
}

#[test]
fn failed_paused_practice_discards_pending_requests_when_command_queue_is_full() {
    let (mut runtime, audio, input) = fixture(8);
    let mut processor = audio.processor();
    start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
    for _ in 0..8 {
        assert!(audio.play_now(SoundId(1), 0.125, false));
    }
    runtime.pending_audio.schedule(ScheduledSound::one_shot(650, SoundId(3), 1.0, 0.0));
    runtime.pending_keysound_volumes.push((SoundId(1), 0.5));
    runtime.flush_audio(&audio);
    assert!(!runtime.pending_audio.is_empty());
    assert!(!runtime.pending_keysound_volumes.is_empty());
    // Practice abort can queue FAILED and pause before the next worker wake.
    runtime.session.state = PlayState::Failed;
    runtime.session.audio_clock.pause_at(TimeUs(100_000));
    runtime.advance(&audio);
    assert!(runtime.pending_audio.is_empty());
    assert!(runtime.pending_keysound_volumes.is_empty());
    assert_eq!(render(&mut processor, 101), [0.0; 2]);
    runtime.flush_audio(&audio);
    runtime.advance(&audio);
    assert_eq!(render(&mut processor, 10_000), [0.0; 2]);
}

#[test]
fn completed_failed_lamp_and_requested_finish_keep_audio_tails() {
    for requested_finish in [false, true] {
        let (mut runtime, audio, input) = fixture(64);
        let mut processor = audio.processor();
        start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
        runtime.session.gauge = GaugeState::new(GaugeType::Normal, 160.0, 8);
        runtime.session.gauge.set_initial_value(0.0);
        let finish_at = if requested_finish { 2_000 } else { 7_000 };
        runtime.session.audio_clock.current_frame.store(finish_at, Ordering::Release);
        // Process all notes before applying the final-notes exit command.
        runtime.advance(&audio);
        assert!(runtime.session.judge.is_exhausted(&runtime.session.chart));
        if requested_finish {
            assert_eq!(runtime.session.state, PlayState::Playing);
            runtime.session.state = PlayState::Finished;
        }
        assert_eq!(runtime.advance(&audio).state, PlayState::Finished);
        let result = crate::screens::play_finish::play_result_from_session(&runtime.session);
        assert_eq!(result.clear_type, ClearType::Failed);
        assert!(render(&mut processor, finish_at)[0] > 0.0);
        assert!(render(&mut processor, 10_000)[0] > 0.0);
        // The ordinary RESULT-exit gain fade still works.
        assert!(audio.set_master_gain(0.0));
        assert_eq!(render(&mut processor, 10_001), [0.0; 2]);
    }
}

#[test]
fn gas_fallback_continues_chart_audio() {
    let (mut runtime, audio, input) = fixture(64);
    let mut processor = audio.processor();
    start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
    runtime.session.gauge = GaugeState::new_auto_shift(160.0, 8);
    runtime.session.audio_clock.current_frame.store(600, Ordering::Release);
    assert_eq!(runtime.advance(&audio).state, PlayState::Playing);
    assert_ne!(runtime.session.gauge.selected, GaugeType::Hazard);
    assert!(render(&mut processor, 600)[0] > 0.0);
    assert!(render(&mut processor, 650)[0] > 0.0);
}

#[test]
fn replay_gauge_failure_stops_audio_and_autoplay_completion_keeps_tails() {
    for replay in [true, false] {
        let (mut runtime, audio, _) = fixture(64);
        let mut processor = audio.processor();
        if replay {
            runtime.session.replay_player = Some(bmz_gameplay::replay::ReplayPlayer {
                events: Vec::new(),
                next_index: 0,
                next_scoring_time: None,
            });
            runtime.session.gauge = GaugeState::new(GaugeType::Hard, 160.0, 8);
            runtime.session.gauge.set_initial_value(0.1);
        } else {
            runtime.session.autoplay = Some(Default::default());
        }
        runtime.advance(&audio);
        assert!(render(&mut processor, 0)[0] > 0.0);
        let at = if replay { 600 } else { 7_000 };
        runtime.session.audio_clock.current_frame.store(at, Ordering::Release);
        let frame = runtime.advance(&audio);
        if replay {
            assert_eq!(frame.state, PlayState::Failed);
            assert_eq!(render(&mut processor, at), [0.0; 2]);
        } else {
            assert_eq!(frame.state, PlayState::Finished);
            assert!(render(&mut processor, at)[0] > 0.0);
        }
    }
}

#[test]
fn viewer_pause_and_seek_keep_source_reusable_without_failed_transition() {
    let (mut runtime, audio, _) = fixture(64);
    runtime.session.autoplay = Some(Default::default());
    let mut processor = audio.processor();
    runtime.advance(&audio);
    assert!(render(&mut processor, 0)[0] > 0.0);
    runtime.session.audio_clock.pause_at(TimeUs(0));
    assert_eq!(runtime.advance(&audio).state, PlayState::Playing);
    assert!(audio.set_playback_paused(true));
    assert_eq!(render(&mut processor, 1), [0.0; 2]);
    assert!(audio.replace_playback_paused(vec![ScheduledSound::one_shot(2, SoundId(2), 1.0, 0.0)]));
    assert_eq!(render(&mut processor, 2), [0.0; 2]);
    assert!(audio.set_playback_paused(false));
    assert_eq!(render(&mut processor, 3), [0.125; 2]);
}

#[test]
fn failed_worker_keeps_play_stop_result_and_skin_audio_available() {
    use crate::system_sound::SoundType;
    use crate::system_sound_manager::SystemSoundManager;
    let (mut runtime, audio, input) = fixture(64);
    let mut processor = audio.processor();
    start_long_bgm_and_keys(&mut runtime, &audio, &input, &mut processor);
    let mut system_engine = AudioEngine::new(SAMPLE_RATE);
    system_engine.insert_sample(SoundId(1), sample(0.25));
    system_engine.insert_sample(SoundId(2), sample(0.5));
    let system = AudioEngineHandle::new(system_engine);
    let mut system_processor = system.processor();
    let manager = SystemSoundManager::with_id_map(
        system.clone(),
        std::collections::HashMap::from([
            (SoundType::PlayStop, SoundId(1)),
            (SoundType::ResultFail, SoundId(2)),
        ]),
    );
    let mut render_config = config(&runtime.session, Arc::new(RuntimeProbe::default()));
    render_config.effects = Some(manager.gameplay_output(1.0));
    let mut client = GameplayClient::new(runtime.session);
    client.start(audio.clone(), render_config).unwrap();
    assert!(client.edit(|session| session.state = PlayState::Failed));
    wait_until(|| render(&mut system_processor, 0) == [0.25; 2]);
    assert_eq!(render(&mut processor, 101), [0.0; 2]);
    client.shutdown();
    manager.stop(SoundType::PlayStop);
    manager.play(SoundType::ResultFail, 1.0);
    assert_eq!(render(&mut system_processor, 1), [0.5; 2]);
    manager.stop(SoundType::ResultFail);
    let document =
        serde_json::from_str(r#"{"sceneAudio":[{"action":"loop","path":"result.ogg"}]}"#).unwrap();
    let skin = crate::skin_audio::SkinAudioRuntime::install(
        system,
        &document,
        vec![crate::skin_loader::DecodedSkinAudio {
            path: "result.ogg".into(),
            sample: sample(0.125),
        }],
    );
    assert!(skin.start_scene(1.0, 1.0));
    assert_eq!(render(&mut system_processor, 2), [0.125; 2]);
}
