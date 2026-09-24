use super::*;
use crate::input::shared::SharedInputBackend;
use crate::screens::play_session::{PlaySessionOptions, build_game_session_with_input_backend};
use bmz_audio::{clock::AudioClock, engine::AudioEngine};
use bmz_chart::model::*;
use bmz_core::{
    ids::{NoteId, SoundId},
    input::{InputEvent, InputKind},
    lane::{KeyMode, Lane},
    time::ChartTick,
};
use bmz_gameplay::input::{backend::*, binding::*, translator::*};
use std::sync::atomic::AtomicI64;

#[derive(Default)]
pub struct RuntimeProbe {
    time: AtomicI64,
    fingerprint: Mutex<String>,
    scheduled: AtomicU64,
}

impl RuntimeProbe {
    pub fn record(&self, session: &GameSession, audio: &AudioEngineHandle) {
        let time = session.audio_clock.now().0;
        if time <= self.time.load(Ordering::Acquire) {
            return;
        }
        let mut details = session.result_judgements.iter().collect::<Vec<_>>();
        details.sort_by_key(|(id, _)| id.0);
        *self.fingerprint.lock().unwrap() = format!(
            "{:?}",
            (
                crate::screens::play_finish::play_result_from_session(session),
                &session.replay_recorder.events,
                &session.gauge,
                &session.judge.lanes,
                details,
                session.offsets,
                &session.hit_error_ring,
            )
        );
        self.scheduled.store(audio.diagnostics().submitted, Ordering::Release);
        self.time.store(time, Ordering::Release);
    }
}

// The driver supplies a virtual monotonic timestamp and uses the production
// translator with a deterministic clock anchor. OS timestamp conversion itself
// is covered by translator and native/backend tests.
struct VirtualTimestampTranslator(DefaultInputTranslator);
impl InputTranslator for VirtualTimestampTranslator {
    fn translate(
        &mut self,
        event: DeviceInputEvent,
        ctx: &InputTimingContext<'_>,
    ) -> Option<InputEvent> {
        self.0.translate(
            event,
            &InputTimingContext {
                audio_clock: ctx.audio_clock,
                offsets: ctx.offsets,
                timestamp_anchor: Some(InputTimestampAnchor {
                    monotonic_ns: 0,
                    audio_time: TimeUs(0),
                }),
            },
        )
    }
}

fn chart() -> Arc<PlayableChart> {
    let mut chart = PlayableChart {
        identity: bmz_chart::hash::compute_chart_identity(b"runtime-stall-seed-12345"),
        metadata: ChartMetadata {
            initial_bpm: 120.0,
            key_mode: KeyMode::K7,
            total: Some(160.0),
            ..Default::default()
        },
        lane_notes: std::array::from_fn(|_| Vec::new()),
        long_notes: Vec::new(),
        bgm_events: Vec::new(),
        bga_events: Vec::new(),
        timing_events: Vec::new(),
        scroll_events: Vec::new(),
        speed_events: Vec::new(),
        judge_rank_events: Vec::new(),
        bgm_volume_events: Vec::new(),
        key_volume_events: Vec::new(),
        text_events: Vec::new(),
        bga_opacity_events: Vec::new(),
        bga_argb_events: Vec::new(),
        swbga_definitions: Vec::new(),
        bga_keybound_events: Vec::new(),
        bga_asset_by_bmp_key: Default::default(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 8,
        end_time: TimeUs(1_200_000),
    };
    for (id, lane, kind, time) in [
        (1, Lane::Key1, NoteKind::Tap, 100_000),
        (2, Lane::Key1, NoteKind::Tap, 200_000),
        (3, Lane::Key1, NoteKind::Tap, 300_000), // intentionally missed
        (4, Lane::Key1, NoteKind::Mine, 400_000),
        (5, Lane::Key2, NoteKind::LongStart, 100_000),
        (6, Lane::Key2, NoteKind::LongEnd, 600_000),
        (7, Lane::Key3, NoteKind::LongStart, 100_000),
        (8, Lane::Key3, NoteKind::LongEnd, 600_000),
        (9, Lane::Key4, NoteKind::LongStart, 100_000),
        (10, Lane::Key4, NoteKind::LongEnd, 1_200_000),
    ] {
        chart.lane_notes[lane.index()].push(NoteEvent {
            id: NoteId(id),
            lane,
            kind,
            tick: ChartTick(time * 192 / 1_000_000),
            time: TimeUs(time as i64),
            sound: Some(SoundId(1)),
            layered_sounds: Vec::new(),
            damage: (kind == NoteKind::Mine).then_some(4.0),
        });
    }
    for (lane, start, mode) in [
        (Lane::Key2, 5, LongNoteMode::Ln),
        (Lane::Key3, 7, LongNoteMode::Cn),
        (Lane::Key4, 9, LongNoteMode::Hcn),
    ] {
        chart.long_notes.push(LongNotePair {
            lane,
            style: LongNoteStyle::ChannelPair,
            mode: Some(mode),
            start_note_id: NoteId(start),
            end_note_id: NoteId(start + 1),
            start_tick: ChartTick(19),
            end_tick: ChartTick(if mode == LongNoteMode::Hcn { 230 } else { 115 }),
            start_time: TimeUs(100_000),
            end_time: TimeUs(if mode == LongNoteMode::Hcn { 1_200_000 } else { 600_000 }),
            sound: Some(SoundId(1)),
        });
    }
    Arc::new(chart)
}

fn events(tick: i64) -> Vec<DeviceInputEvent> {
    let controls: &[(&str, InputKind)] = match tick {
        100 => &[
            ("Z", InputKind::Press),
            ("S", InputKind::Press),
            ("X", InputKind::Press),
            ("D", InputKind::Press),
        ],
        110 => &[("Z", InputKind::Release)],
        200 => &[("Z", InputKind::Press)],
        210 => &[("Z", InputKind::Release)],
        350 => &[("D", InputKind::Release)], // HCN recovery pulse, then damage pulses
        400 => &[("Z", InputKind::Press)],
        410 => &[("Z", InputKind::Release)],
        600 => &[("S", InputKind::Release), ("X", InputKind::Release)],
        950 => &[("D", InputKind::Press)],
        1200 => &[("D", InputKind::Release)],
        _ => &[],
    };
    controls
        .iter()
        .map(|(key, kind)| DeviceInputEvent {
            device: DeviceId(0),
            control: keyboard_control(*key),
            kind: *kind,
            timestamp: DeviceTimestamp::MonotonicNs((tick as u128) * 1_000_000),
            bounce_policy: InputBouncePolicy::Bypass,
        })
        .collect()
}

fn prepared(input: SharedInputBackend) -> GameSession {
    let profile = crate::config::profile_config::ProfileConfig::new_default("test", "Test", 1);
    let mut session = build_game_session_with_input_backend(
        chart(),
        &profile,
        PlaySessionOptions { arrange_seed: Some(12345), ..Default::default() },
        Box::new(input),
    );
    session.audio_clock =
        AudioClock::with_position(1_000_000, 0, 0, Arc::new(AtomicU64::new(0)), true);
    session.input_system.translator =
        Box::new(VirtualTimestampTranslator(DefaultInputTranslator {
            binding: LaneBinding {
                entries: [
                    ("Z", Lane::Key1),
                    ("S", Lane::Key2),
                    ("X", Lane::Key3),
                    ("D", Lane::Key4),
                ]
                .into_iter()
                .map(|(key, lane)| BindingEntry {
                    device: None,
                    control: keyboard_control(key),
                    lane,
                    scratch_direction: None,
                })
                .collect(),
            },
        }));
    session
}

fn config(session: &GameSession, probe: Arc<RuntimeProbe>) -> RuntimeRenderConfig {
    RuntimeRenderConfig {
        probe: Some(probe),
        effects: None,
        best_ex_score: None,
        best_ghost: None,
        target_ex_score: None,
        target: String::new(),
        resolved_target_name: None,
        applied_arrange: AppliedArrange::default(),
        source_ln_profile: crate::ln_policy::ChartLnProfile::from_chart(&session.chart),
        skin_attempt: Default::default(),
        score_key: crate::storage::score_db::ScoreKey::new(
            session.chart.identity.file_sha256,
            crate::ln_policy::LnScorePolicy::AutoLn,
        ),
        practice_mode: false,
        score_save_disabled: false,
        bga_frames: BgaFrameCatalog::new(),
        cache: PlayRenderSnapshotCache::from_chart(&session.chart),
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "gameplay worker failed to progress");
        thread::sleep(Duration::from_micros(100));
    }
}

fn run_with_render_stall(stall_ms: u64) -> (String, u64) {
    run_mode_with_render_stall(stall_ms, 0)
}

#[test]
fn runtime_publishes_target_id_and_resolved_name_from_the_first_frame() {
    let session = prepared(SharedInputBackend::default());
    let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
    let mut render_config = config(&session, Arc::new(RuntimeProbe::default()));
    render_config.target = "RANK_AAA".to_string();
    render_config.resolved_target_name = Some("ライバル_AAA".to_string());
    let mut client = GameplayClient::new(session);
    client.start(audio, render_config).unwrap();

    let initial = &client.latest_frame.as_ref().unwrap().render_snapshot;
    assert_eq!(initial.target, "RANK_AAA");
    assert_eq!(initial.resolved_target_name.as_deref(), Some("ライバル_AAA"));

    let published = client.worker.as_ref().unwrap().latest.clone();
    wait_until(|| published.lock().unwrap().is_some());
    let frame = client.poll().unwrap();
    assert_eq!(frame.render_snapshot.target, "RANK_AAA");
    assert_eq!(frame.render_snapshot.resolved_target_name.as_deref(), Some("ライバル_AAA"));
    client.shutdown();
}

#[test]
fn final_note_state_reaches_input_path_without_render_poll() {
    let session = prepared(SharedInputBackend::default());
    let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
    let config = config(&session, Arc::new(RuntimeProbe::default()));
    let mut client = GameplayClient::new(session);
    client.start(audio, config).unwrap();

    let commands = client.worker.as_ref().unwrap().commands.clone();
    let wake = client.worker.as_ref().unwrap().thread.thread().clone();
    commands
        .send(Box::new(|session| {
            session.audio_clock.current_frame.store(1_300_000, Ordering::Release);
        }))
        .unwrap();
    wake.unpark();

    wait_until(|| client.final_notes_processed());
    assert!(!client.session.exhausted, "detached render observation must not be required");
    client.shutdown();
}

fn run_mode_with_render_stall(stall_ms: u64, mode: u8) -> (String, u64) {
    let input = SharedInputBackend::default();
    let mut session = prepared(input.clone());
    if mode == 1 {
        session.autoplay = Some(Default::default());
    }
    if mode == 2 {
        let ctx_clock = session.audio_clock.clone();
        let replay = (10..=1200)
            .step_by(10)
            .flat_map(events)
            .filter_map(|event| {
                session.input_system.translator.translate(
                    event,
                    &InputTimingContext {
                        audio_clock: &ctx_clock,
                        offsets: session.offsets,
                        timestamp_anchor: None,
                    },
                )
            })
            .map(|event| bmz_core::replay::ReplayEvent {
                lane: event.lane,
                kind: event.kind,
                time: event.time,
                device_kind: event.device_kind,
                scratch_direction: event.scratch_direction,
            })
            .collect();
        session.replay_player = Some(bmz_gameplay::replay::ReplayPlayer::new(replay));
    }
    let probe = Arc::new(RuntimeProbe::default());
    let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
    let config = config(&session, probe.clone());
    let mut client = GameplayClient::new(session);
    let mut processor = audio.processor();
    client.start(audio.clone(), config).unwrap();
    let commands = client.worker.as_ref().unwrap().commands.clone();
    let wake = client.worker.as_ref().unwrap().thread.thread().clone();
    let driver_probe = probe.clone();
    let driver = thread::spawn(move || {
        for tick in (10..=3_000).step_by(10) {
            let input = input.clone();
            commands
                .send(Box::new(move |session| {
                    session.audio_clock.current_frame.store(tick as u64 * 1_000, Ordering::Release);
                    for event in events(tick) {
                        input.push_shared_event(event);
                    }
                }))
                .unwrap();
            wake.unpark();
            wait_until(|| driver_probe.time.load(Ordering::Acquire) >= tick * 1_000);
            let mut output = [0.0; 2];
            assert!(processor.render_stereo(tick as u64 * 1_000, &mut output));
            thread::sleep(Duration::from_millis(1));
        }
    });
    let mut stalled = false;
    let mut last = TimeUs(i64::MIN);
    while !driver.is_finished() {
        if !stalled && probe.time.load(Ordering::Acquire) >= 50_000 {
            let before_time = probe.time.load(Ordering::Acquire);
            let before_audio = probe.scheduled.load(Ordering::Acquire);
            // Hold the exchange mutex as well: even a hostile consumer cannot
            // block gameplay or enqueue while inside acquire/present.
            let slot = client.worker.as_ref().unwrap().latest.clone();
            let guard = slot.lock().unwrap();
            thread::sleep(Duration::from_millis(stall_ms));
            if stall_ms >= 100 {
                assert!(probe.time.load(Ordering::Acquire) > before_time);
                assert!(probe.scheduled.load(Ordering::Acquire) > before_audio);
            }
            drop(guard);
            stalled = true;
        }
        if let Some(frame) = client.poll() {
            assert!(frame.render_snapshot.time >= last);
            last = frame.render_snapshot.time;
        }
        thread::sleep(Duration::from_micros(4_167)); // 240fps consumer
    }
    driver.join().unwrap();
    wait_until(|| {
        client.poll();
        client.result.is_some()
    });
    let diagnostics = audio.diagnostics();
    assert_eq!(diagnostics.scheduling_late_frames, 0, "render stall delayed audio commands");
    let answer = (probe.fingerprint.lock().unwrap().clone(), diagnostics.scheduled_sound_count);
    client.shutdown();
    answer
}

#[test]
fn gameplay_results_replay_and_audio_are_invariant_to_render_stalls() {
    let expected = run_with_render_stall(0);
    assert!(expected.1 > 0);
    for stall in [16, 33, 100, 250] {
        assert_eq!(
            run_with_render_stall(stall),
            expected,
            "render stall {stall}ms changed gameplay"
        );
    }
}

#[test]
fn dedicated_runtime_matches_original_session_advance() {
    let input = SharedInputBackend::default();
    let mut session = prepared(input.clone());
    let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
    let mut queue = bmz_audio::queue::ScheduledSoundQueue::new();
    let probe = RuntimeProbe::default();
    for tick in (10..=3_000).step_by(10) {
        session.audio_clock.current_frame.store(tick as u64 * 1_000, Ordering::Release);
        for event in events(tick) {
            input.push_shared_event(event);
        }
        bmz_gameplay::session::advance_session_frame(&mut session, &mut queue);
    }
    probe.record(&session, &audio);
    assert_eq!(run_with_render_stall(100).0, *probe.fingerprint.lock().unwrap());
}
#[test]
fn autoplay_and_replay_are_invariant_to_render_stalls() {
    for mode in [1, 2] {
        let expected = run_mode_with_render_stall(0, mode);
        for stall in [16, 33, 100, 250] {
            assert_eq!(
                run_mode_with_render_stall(stall, mode),
                expected,
                "mode {mode}, stall {stall}"
            );
        }
    }
}

#[test]
fn renderer_projects_cached_publication_when_exchange_is_unavailable() {
    for fps in [120, 240] {
        let mut session = prepared(SharedInputBackend::default());
        session.hispeed = 1.0;
        session.offsets.visual_offset_us = 0;
        let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
        let config = config(&session, Arc::new(RuntimeProbe::default()));
        let mut client = GameplayClient::new(session);
        client.start(audio, config).unwrap();
        let slot = client.worker.as_ref().unwrap().latest.clone();
        // Keep the producer from delivering anything: poll must project its
        // cached logical state even if it never receives another publication.
        let guard = slot.lock().unwrap();
        for frame in 0..=fps / 20 {
            let now = frame as u64 * 1_000_000 / fps as u64;
            client.session.audio_clock.current_frame.store(now, Ordering::Release);
            let rendered = client.poll().unwrap().render_snapshot;
            assert_eq!(rendered.time, TimeUs(now as i64));
            let note = rendered.visible_notes[Lane::Key1.index()]
                .iter()
                .find(|note| note.time == TimeUs(300_000))
                .unwrap();
            assert!((note.y - (300_000 - now) as f32 / 2_000_000.0).abs() < 1e-6);
        }
        for now in [150_000, 300_000] {
            client.session.audio_clock.current_frame.store(now, Ordering::Release);
            assert_eq!(client.poll().unwrap().render_snapshot.time, TimeUs(now as i64));
        }
        drop(guard);
        client.shutdown();
    }
}
