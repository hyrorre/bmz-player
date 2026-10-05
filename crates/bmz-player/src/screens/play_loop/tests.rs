use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::queue::{AudioScheduler, RestartPolicy, ScheduledSound, ScheduledSoundQueue};
use bmz_chart::hash::compute_chart_identity;
use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart, SoundEvent};
use bmz_core::ids::{NoteId, SoundId};
use bmz_core::judge::Judge;
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};
use bmz_gameplay::judge::model::JudgementEvent;

use crate::config::profile_config::ProfileConfig;
use crate::config::profile_config::ReplayConfig;
use crate::screens::play_session::{PlaySessionOptions, build_game_session};
use crate::select_options::ArrangeOption;
use crate::storage::common::configure_connection;
use crate::storage::migration::{NETWORK_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
use crate::storage::score_db::ScoreDatabase;

use super::*;

#[derive(Default)]
struct TestAudio {
    scheduled: Vec<ScheduledSound>,
}

impl AudioScheduler for TestAudio {
    fn schedule(&mut self, sound: ScheduledSound) {
        self.scheduled.push(sound);
    }
}

#[test]
fn advance_play_screen_returns_snapshot() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    let mut audio = TestAudio::default();

    let frame = advance_play_screen(&mut session, &mut audio, None);

    assert_eq!(frame.render_snapshot.time, TimeUs(0));
    assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
}

#[test]
fn apply_play_arrange_to_snapshot_sets_skin_values() {
    let mut snapshot = RenderSnapshot::default();
    let pattern = vec![3, 1, 4, 2, 7, 5, 6];
    let applied = AppliedArrange {
        arrange: ArrangeOption::Mirror,
        arrange_2p: ArrangeOption::Random,
        pattern: Some(pattern.clone()),
        ..AppliedArrange::default()
    };

    apply_play_arrange_to_snapshot(&mut snapshot, &applied);

    assert_eq!(snapshot.arrange, "MIRROR");
    assert_eq!(snapshot.arrange_2p, "RANDOM");
    assert_eq!(snapshot.lane_shuffle_pattern, pattern);
}

#[test]
fn advance_play_screen_with_shared_audio_returns_snapshot() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));

    let frame = advance_play_screen_with_shared_audio(&mut session, &audio, None).unwrap();

    assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
}

#[test]
fn advance_play_screen_with_shared_audio_flushes_scheduled_sounds() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart_with_bgm()), &profile, PlaySessionOptions::default());
    let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));

    let frame = advance_play_screen_with_shared_audio(&mut session, &audio, None).unwrap();

    assert_eq!(frame.state, PlayState::Playing);
    let mut guard = audio.lock().unwrap();
    let scheduled = guard.queue.drain_until_frame(0);
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0].sound_id, SoundId(3));
}

#[test]
fn nonblocking_audio_flush_keeps_sounds_when_engine_is_busy() {
    let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
    let held = audio.lock().unwrap();
    let mut scheduled = ScheduledSoundQueue::new();
    scheduled.schedule(scheduled_sound(0, 3));

    flush_scheduled_audio_nonblocking(&audio, &mut scheduled).unwrap();

    assert_eq!(scheduled.len(), 1);
    drop(held);
    flush_scheduled_audio_nonblocking(&audio, &mut scheduled).unwrap();
    assert!(scheduled.is_empty());
    let mut guard = audio.lock().unwrap();
    assert_eq!(guard.queue.drain_until_frame(0)[0].sound_id, SoundId(3));
}

#[test]
fn command_audio_flush_keeps_sounds_when_queue_is_full() {
    let audio = AudioEngineHandle::with_capacity(AudioEngine::default(), 1);
    assert!(audio.set_master_gain(0.5));
    let mut processor = audio.processor();
    let mut scheduled = ScheduledSoundQueue::new();
    scheduled.schedule(scheduled_sound(0, 3));

    flush_scheduled_audio_commands(&audio, &mut scheduled).unwrap();

    assert_eq!(scheduled.len(), 1);
    processor.apply_pending_commands_for_tests();
    flush_scheduled_audio_commands(&audio, &mut scheduled).unwrap();
    assert!(scheduled.is_empty());
}

#[test]
fn queue_keysound_volumes_keeps_latest_volume_per_sound() {
    let mut pending = Vec::new();

    queue_keysound_volumes(&mut pending, &[(SoundId(1), 0.5), (SoundId(1), 0.25)]);
    queue_keysound_volumes(&mut pending, &[(SoundId(2), 0.75)]);

    assert_eq!(pending, vec![(SoundId(1), 0.25), (SoundId(2), 0.75)]);
}

#[test]
fn nonblocking_keysound_volume_flush_retries_when_engine_is_busy() {
    let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
    let held = audio.lock().unwrap();
    let mut pending = vec![(SoundId(1), 0.25)];

    flush_keysound_volumes_nonblocking(&audio, &mut pending).unwrap();

    assert_eq!(pending, vec![(SoundId(1), 0.25)]);
    drop(held);
    flush_keysound_volumes_nonblocking(&audio, &mut pending).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn command_keysound_volume_flush_retries_when_queue_is_full() {
    let audio = AudioEngineHandle::with_capacity(AudioEngine::default(), 1);
    assert!(audio.set_master_gain(0.5));
    let mut processor = audio.processor();
    let mut pending = vec![(SoundId(1), 0.25)];

    flush_keysound_volumes_commands(&audio, &mut pending).unwrap();

    assert_eq!(pending, vec![(SoundId(1), 0.25)]);
    processor.apply_pending_commands_for_tests();
    flush_keysound_volumes_commands(&audio, &mut pending).unwrap();
    assert!(pending.is_empty());
}

#[test]
fn advance_play_screen_until_result_returns_finished_outcome() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(finished_chart()), &profile, PlaySessionOptions::default());
    let mut audio = TestAudio::default();
    let root = make_temp_dir("advance-finished");
    let paths = crate::paths::ProfilePaths {
        root_dir: root.clone(),
        profile_toml: root.join("profile.toml"),
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
        replay_dir: root.join("replay"),
    };
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut score_db = ScoreDatabase::from_connection(conn);
    let mut network_conn = rusqlite::Connection::open_in_memory().unwrap();
    configure_connection(&network_conn).unwrap();
    run_migrations(&mut network_conn, NETWORK_MIGRATIONS).unwrap();
    let mut network_db = NetworkDatabase::from_connection(network_conn);
    let replay_config = ReplayConfig {
        auto_save: false,
        compress: false,
        slot_rules: crate::config::profile_config::default_slot_rules(),
    };

    let outcome = advance_play_screen_until_result(
        &mut session,
        &mut audio,
        &mut score_db,
        &mut network_db,
        &paths,
        &replay_config,
        &crate::config::profile_config::IrConfig::default(),
        1_700_000_200,
        &AppliedArrange::default(),
    )
    .unwrap();

    assert!(matches!(outcome, PlayAdvanceOutcome::Finished { .. }));
    assert!(outcome.is_finished());
    assert!(outcome.finished().is_some());
    assert_eq!(outcome.frame().state, PlayState::Finished);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn refresh_play_ending_snapshot_advances_note_scroll_with_audio_clock() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.hispeed = 1.0;
    session.audio_clock = test_running_audio_clock(0);

    let timers = PlayEndingSkinTimers {
        play_elapsed_time: TimeUs(0),
        ready_elapsed_time: None,
        backbmp_background: false,
        failed_elapsed_ms: None,
        music_end_elapsed_ms: None,
        fadeout_elapsed_ms: None,
    };
    let early = refresh_play_ending_snapshot_with_session(
        &mut session,
        None,
        None,
        None,
        &BgaFrameCatalog::new(),
        timers,
    );
    assert_eq!(early.visible_notes[Lane::Key1.index()][0].y, 0.5);

    advance_test_audio_clock(&mut session, 750_000);
    let later = refresh_play_ending_snapshot_with_session(
        &mut session,
        None,
        None,
        None,
        &BgaFrameCatalog::new(),
        timers,
    );
    assert_eq!(later.time, TimeUs(750_000));
    assert_eq!(later.visible_notes[Lane::Key1.index()][0].y, 0.125);
}

#[test]
fn refresh_play_ending_snapshot_expires_old_judgements() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut session =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
    session.recent_judgements.push(JudgementEvent {
        lane: Lane::Key1,
        judge: Judge::PGreat,
        side: bmz_core::judge::TimingSide::Fast,
        delta: TimeUs(0),
        time: TimeUs(0),
        note_id: Some(NoteId(1)),
        affects_score: true,
    });
    session.recent_display_judgements.push(bmz_gameplay::session::DisplayJudgementEvent {
        judgement: session.recent_judgements[0].clone(),
        combo: 1,
        display_time: TimeUs(0),
    });
    session.audio_clock = test_running_audio_clock(0);

    let timers = PlayEndingSkinTimers {
        play_elapsed_time: TimeUs(0),
        ready_elapsed_time: None,
        backbmp_background: false,
        failed_elapsed_ms: None,
        music_end_elapsed_ms: None,
        fadeout_elapsed_ms: None,
    };
    let visible = refresh_play_ending_snapshot_with_session(
        &mut session,
        None,
        None,
        None,
        &BgaFrameCatalog::new(),
        timers,
    );
    assert_eq!(visible.recent_judgements.len(), 1);

    advance_test_audio_clock(&mut session, 900_000);
    let expired = refresh_play_ending_snapshot_with_session(
        &mut session,
        None,
        None,
        None,
        &BgaFrameCatalog::new(),
        timers,
    );
    assert!(expired.recent_judgements.is_empty());
}

fn test_running_audio_clock(elapsed_us: i64) -> AudioClock {
    let sample_rate = 48_000;
    let current_frame = Arc::new(AtomicU64::new(us_to_frames(elapsed_us, sample_rate)));
    AudioClock::with_position(sample_rate, 0, 0, current_frame, true)
}

fn advance_test_audio_clock(session: &mut GameSession, elapsed_us: i64) {
    let sample_rate = session.audio_clock.sample_rate;
    session
        .audio_clock
        .current_frame
        .store(us_to_frames(elapsed_us, sample_rate), Ordering::Relaxed);
}

fn us_to_frames(elapsed_us: i64, sample_rate: u32) -> u64 {
    ((elapsed_us.max(0) as u128 * sample_rate as u128) / 1_000_000u128) as u64
}

fn chart() -> PlayableChart {
    let note = NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(0),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    };
    let mut lane_notes = std::array::from_fn(|_| Vec::new());
    lane_notes[Lane::Key1.index()].push(note);

    PlayableChart {
        identity: compute_chart_identity(b"play-loop"),
        metadata: ChartMetadata {
            title: "play-loop".to_string(),
            initial_bpm: 120.0,
            total: Some(160.0),
            ..Default::default()
        },
        lane_notes,
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
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 1,
        end_time: TimeUs(1_000_000),
    }
}

fn finished_chart() -> PlayableChart {
    PlayableChart {
        identity: compute_chart_identity(b"finished-play-loop"),
        metadata: ChartMetadata {
            title: "finished-play-loop".to_string(),
            initial_bpm: 120.0,
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
        bga_asset_by_bmp_key: std::collections::HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 0,
        end_time: TimeUs(-6_000_000),
    }
}

fn chart_with_bgm() -> PlayableChart {
    let mut chart = chart();
    chart.bgm_events.push(SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(3) });
    chart
}

fn scheduled_sound(start_frame: u64, sound_id: u32) -> ScheduledSound {
    ScheduledSound {
        start_frame,
        sample_offset_frames: 0,
        sound_id: SoundId(sound_id),
        volume: 1.0,
        pan: 0.0,
        loop_playback: false,
        fade_in_frames: 0,
        catch_up: true,
        restart_policy: RestartPolicy::Overlap,
    }
}

fn make_temp_dir(label: &str) -> std::path::PathBuf {
    let stamp =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path =
        std::env::temp_dir().join(format!("bmz-player-{label}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}
