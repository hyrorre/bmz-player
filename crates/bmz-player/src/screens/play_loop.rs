#[cfg(test)]
use std::sync::TryLockError;

use anyhow::{Result, anyhow};
use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::command::AudioEngineHandle;
use bmz_audio::queue::{AudioScheduler, ScheduledSoundQueue};
use bmz_core::{ids::SoundId, time::TimeUs};
use bmz_gameplay::session::{
    FrameOutput, GameSession, PlayState, SessionFrame, advance_session_frame,
    apply_auto_key_release, compute_frame_times, update_recent_inputs, update_recent_judgements,
};
use bmz_render::snapshot::RenderSnapshot;

use crate::audio::RunningPlaySession;
use crate::config::profile_config::{IrConfig, ReplayConfig};
use crate::paths::ProfilePaths;
use crate::screens::play_finish::{
    FinishResultMode, FinishSessionResultOnceRequest, FinishedPlaySession, finish_session_result,
    finish_session_result_once,
};
use crate::screens::play_session::AppliedArrange;
use crate::screens::play_snapshot::{
    BgaFrameCatalog, PlayRenderSnapshotCache, build_render_snapshot_with_target_and_bga_frames,
    build_render_snapshot_with_target_and_bga_frames_cached,
};
use crate::storage::network_db::NetworkDatabase;
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PlayAdvanceOutcome {
    Playing(FrameOutput<RenderSnapshot>),
    Finished { frame: FrameOutput<RenderSnapshot>, finished: FinishedPlaySession },
}

impl PlayAdvanceOutcome {
    pub fn frame(&self) -> &FrameOutput<RenderSnapshot> {
        match self {
            Self::Playing(frame) | Self::Finished { frame, .. } => frame,
        }
    }

    pub fn finished(&self) -> Option<&FinishedPlaySession> {
        match self {
            Self::Playing(_) => None,
            Self::Finished { finished, .. } => Some(finished),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished().is_some()
    }
}

pub fn advance_play_screen(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    best_ex_score: Option<u32>,
) -> FrameOutput<RenderSnapshot> {
    advance_play_screen_with_bga_frames(
        session,
        audio,
        best_ex_score,
        None,
        None,
        &BgaFrameCatalog::new(),
    )
}

pub fn advance_play_screen_with_bga_frames(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> FrameOutput<RenderSnapshot> {
    let frame = advance_session_frame(session, audio);
    let mut render_snapshot = build_render_snapshot_with_target_and_bga_frames(
        session,
        frame.times.render_now,
        &session.recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
    );
    render_snapshot.skin_events = frame.skin_events.clone();
    FrameOutput {
        render_snapshot,
        judgements: frame.judgements,
        mine_hits: frame.mine_hits,
        keysound_volumes: frame.keysound_volumes,
        skin_events: frame.skin_events,
        state: frame.state,
    }
}

pub fn advance_play_screen_until_result(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    ir_config: &IrConfig,
    played_at: i64,
    applied_arrange: &AppliedArrange,
) -> Result<PlayAdvanceOutcome> {
    let frame = advance_play_screen(session, audio, None);
    if matches!(frame.state, PlayState::Finished | PlayState::Failed) {
        let mut finished = finish_session_result(
            score_db,
            network_db,
            profile_paths,
            replay_config,
            ir_config,
            session,
            played_at,
            applied_arrange,
            None,
            crate::storage::score_db::ScoreKey::new(
                session.chart.identity.file_sha256,
                crate::ln_policy::score_ln_policy(
                    crate::ln_policy::LnPolicySetting::AutoLn,
                    crate::ln_policy::ChartLnProfile::from_chart(&session.chart),
                ),
            )
            .with_rule_mode(session.rule_mode),
            false,
            FinishResultMode::Normal,
        )?;
        let mut result_graph = crate::screens::result_model::ResultGraphCollector::default();
        result_graph.record_frame(&frame);
        finished.summary.graph = result_graph.snapshot_for_session(session);
        return Ok(PlayAdvanceOutcome::Finished { frame, finished });
    }

    Ok(PlayAdvanceOutcome::Playing(frame))
}

pub fn advance_play_screen_with_shared_audio(
    session: &mut GameSession,
    audio: &SharedAudioEngine,
    best_ex_score: Option<u32>,
) -> Result<FrameOutput<RenderSnapshot>> {
    let mut scheduled = ScheduledSoundQueue::new();
    let frame = advance_session_frame(session, &mut scheduled);
    flush_scheduled_audio_blocking(audio, &mut scheduled)?;
    Ok(frame_output_from_session_frame(
        session,
        frame,
        best_ex_score,
        None,
        None,
        &BgaFrameCatalog::new(),
    ))
}

/// `SessionFrame`(audio スケジューリング結果)から、ロック不要な render
/// snapshot を構築して `FrameOutput` を組み立てる。重い処理はここに集約し、
/// audio エンジンロックの外で実行する。
fn frame_output_from_session_frame(
    session: &GameSession,
    frame: SessionFrame,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> FrameOutput<RenderSnapshot> {
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    frame_output_from_session_frame_cached(
        session,
        frame,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
        &cache,
    )
}

fn frame_output_from_session_frame_cached(
    session: &GameSession,
    frame: SessionFrame,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
    cache: &PlayRenderSnapshotCache,
) -> FrameOutput<RenderSnapshot> {
    let mut render_snapshot = build_render_snapshot_with_target_and_bga_frames_cached(
        session,
        frame.times.render_now,
        &session.recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
        cache,
    );
    render_snapshot.play_elapsed_time = TimeUs(frame.times.audio_now.0.max(0));
    render_snapshot.skin_events = frame.skin_events.clone();
    FrameOutput {
        render_snapshot,
        judgements: frame.judgements,
        mine_hits: frame.mine_hits,
        keysound_volumes: frame.keysound_volumes,
        skin_events: frame.skin_events,
        state: frame.state,
    }
}

fn flush_scheduled_audio_blocking(
    audio: &SharedAudioEngine,
    scheduled: &mut ScheduledSoundQueue,
) -> Result<()> {
    if scheduled.is_empty() {
        return Ok(());
    }
    let mut audio = audio.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
    audio.schedule_all(scheduled.drain_all());
    Ok(())
}

#[cfg(test)]
fn flush_scheduled_audio_nonblocking(
    audio: &SharedAudioEngine,
    scheduled: &mut ScheduledSoundQueue,
) -> Result<()> {
    if scheduled.is_empty() {
        return Ok(());
    }
    match audio.try_lock() {
        Ok(mut audio) => {
            audio.schedule_all(scheduled.drain_all());
            Ok(())
        }
        Err(TryLockError::WouldBlock) => Ok(()),
        Err(TryLockError::Poisoned(_)) => Err(anyhow!("audio engine lock poisoned")),
    }
}

fn flush_scheduled_audio_commands(
    audio: &AudioEngineHandle,
    scheduled: &mut ScheduledSoundQueue,
) -> Result<()> {
    if scheduled.is_empty() {
        return Ok(());
    }
    let sounds = scheduled.drain_all().collect::<Vec<_>>();
    match audio.try_schedule_all(sounds) {
        Ok(()) => Ok(()),
        Err(sounds) => {
            for sound in sounds {
                scheduled.schedule(sound);
            }
            Ok(())
        }
    }
}

fn queue_keysound_volumes(pending: &mut Vec<(SoundId, f32)>, volumes: &[(SoundId, f32)]) {
    for &(sound_id, volume) in volumes {
        if let Some((_, pending_volume)) =
            pending.iter_mut().find(|(pending_sound_id, _)| *pending_sound_id == sound_id)
        {
            *pending_volume = volume;
        } else {
            pending.push((sound_id, volume));
        }
    }
}

/// HCN 早離し時のミュート/復帰など、フレームで発生したキー音音量変更を
/// audio engine に反映する。audio callback との競合時は次フレームへ retry する。
#[cfg(test)]
fn flush_keysound_volumes_nonblocking(
    audio: &SharedAudioEngine,
    pending: &mut Vec<(SoundId, f32)>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    match audio.try_lock() {
        Ok(mut audio) => {
            for (sound_id, volume) in pending.drain(..) {
                audio.set_sound_volume(sound_id, volume);
            }
            Ok(())
        }
        Err(TryLockError::WouldBlock) => Ok(()),
        Err(TryLockError::Poisoned(_)) => Err(anyhow!("audio engine lock poisoned")),
    }
}

fn flush_keysound_volumes_commands(
    audio: &AudioEngineHandle,
    pending: &mut Vec<(SoundId, f32)>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let mut remaining = Vec::new();
    for (sound_id, volume) in pending.drain(..) {
        if !audio.set_sound_volume(sound_id, volume) {
            remaining.push((sound_id, volume));
        }
    }
    *pending = remaining;
    Ok(())
}

pub fn advance_running_play_session(
    running: &mut RunningPlaySession,
) -> Result<FrameOutput<RenderSnapshot>> {
    let frame = advance_session_frame(&mut running.session, &mut running.pending_audio);
    flush_scheduled_audio_commands(&running.audio.engine, &mut running.pending_audio)?;
    queue_keysound_volumes(&mut running.pending_keysound_volumes, &frame.keysound_volumes);
    flush_keysound_volumes_commands(&running.audio.engine, &mut running.pending_keysound_volumes)?;
    let mut output = frame_output_from_session_frame_cached(
        &running.session,
        frame,
        running.best_ex_score,
        running.best_ghost.as_deref(),
        running.target_ex_score,
        &running.bga_frames,
        &running.render_snapshot_cache,
    );
    apply_play_arrange_to_snapshot(
        &mut output.render_snapshot,
        running.applied_arrange.arrange,
        running.applied_arrange.arrange_2p,
    );
    apply_running_play_target_to_snapshot(&mut output.render_snapshot, running);
    apply_running_play_mode_to_snapshot(&mut output.render_snapshot, running);
    Ok(output)
}

pub fn advance_running_play_session_until_result(
    running: &mut RunningPlaySession,
    score_db: &mut ScoreDatabase,
    network_db: &mut NetworkDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    ir_config: &IrConfig,
    played_at: i64,
) -> Result<PlayAdvanceOutcome> {
    let session_frame = advance_session_frame(&mut running.session, &mut running.pending_audio);
    flush_scheduled_audio_commands(&running.audio.engine, &mut running.pending_audio)?;
    queue_keysound_volumes(&mut running.pending_keysound_volumes, &session_frame.keysound_volumes);
    flush_keysound_volumes_commands(&running.audio.engine, &mut running.pending_keysound_volumes)?;
    let mut frame = frame_output_from_session_frame_cached(
        &running.session,
        session_frame,
        running.best_ex_score,
        running.best_ghost.as_deref(),
        running.target_ex_score,
        &running.bga_frames,
        &running.render_snapshot_cache,
    );
    apply_play_arrange_to_snapshot(
        &mut frame.render_snapshot,
        running.applied_arrange.arrange,
        running.applied_arrange.arrange_2p,
    );
    apply_running_play_target_to_snapshot(&mut frame.render_snapshot, running);
    apply_running_play_mode_to_snapshot(&mut frame.render_snapshot, running);
    running.result_graph.record_frame(&frame);
    if matches!(frame.state, PlayState::Finished | PlayState::Failed) {
        let mut finished = finish_session_result_once(
            &mut running.finished,
            score_db,
            network_db,
            FinishSessionResultOnceRequest {
                profile_paths,
                replay_config,
                ir_config,
                session: &running.session,
                played_at,
                applied_arrange: &running.applied_arrange,
                target_ex_score: running.target_ex_score,
                score_key: running.score_key,
                practice_mode: running.practice_mode,
                finish_mode: FinishResultMode::Normal,
            },
        )?;
        finished.summary.graph = running.result_graph.snapshot_for_session(&running.session);
        running.finished = Some(finished.clone());
        // ここでは音声を止めない。スケジュール済みの BGM/キー音は
        // オーディオ出力スレッド側で曲の最後まで鳴り切る。出力の解放は
        // リザルト画面側 (advance_draining_audio) がドレイン完了後に行う。
        return Ok(PlayAdvanceOutcome::Finished { frame, finished });
    }

    Ok(PlayAdvanceOutcome::Playing(frame))
}

fn apply_running_play_target_to_snapshot(
    snapshot: &mut RenderSnapshot,
    running: &RunningPlaySession,
) {
    snapshot.target = running.target.clone();
}

fn apply_running_play_mode_to_snapshot(
    snapshot: &mut RenderSnapshot,
    running: &RunningPlaySession,
) {
    snapshot.practice_mode = running.practice_mode;
    snapshot.score_save_enabled =
        !snapshot.autoplay && !snapshot.replay_playback && !running.practice_mode;
}

/// `play_ending` 中に skin 側へ渡す壁時計ベースの timer 値。
#[derive(Debug, Clone, Copy)]
pub struct PlayEndingSkinTimers {
    pub play_elapsed_time: TimeUs,
    pub ready_elapsed_time: Option<TimeUs>,
    pub backbmp_background: bool,
    pub failed_elapsed_ms: Option<i32>,
    pub music_end_elapsed_ms: Option<i32>,
    pub fadeout_elapsed_ms: Option<i32>,
}

/// 終了演出中に gameplay を止めたまま、オーディオクロックに追従して描画 snapshot を更新する。
pub fn refresh_play_ending_snapshot(
    running: &mut RunningPlaySession,
    timers: PlayEndingSkinTimers,
) -> RenderSnapshot {
    let _ = flush_scheduled_audio_commands(&running.audio.engine, &mut running.pending_audio);
    let _ = flush_keysound_volumes_commands(
        &running.audio.engine,
        &mut running.pending_keysound_volumes,
    );
    let mut snapshot = refresh_play_ending_snapshot_with_session_cached(
        &mut running.session,
        running.best_ex_score,
        running.best_ghost.as_deref(),
        running.target_ex_score,
        &running.bga_frames,
        timers,
        &running.render_snapshot_cache,
    );
    apply_play_arrange_to_snapshot(
        &mut snapshot,
        running.applied_arrange.arrange,
        running.applied_arrange.arrange_2p,
    );
    apply_running_play_target_to_snapshot(&mut snapshot, running);
    apply_running_play_mode_to_snapshot(&mut snapshot, running);
    snapshot
}

fn apply_play_arrange_to_snapshot(
    snapshot: &mut RenderSnapshot,
    arrange: crate::select_options::ArrangeOption,
    arrange_2p: crate::select_options::ArrangeOption,
) {
    snapshot.arrange = arrange.as_str().to_string();
    snapshot.arrange_2p = arrange_2p.as_str().to_string();
}

pub fn refresh_play_ending_snapshot_with_session(
    session: &mut GameSession,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
    timers: PlayEndingSkinTimers,
) -> RenderSnapshot {
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    refresh_play_ending_snapshot_with_session_cached(
        session,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
        timers,
        &cache,
    )
}

pub fn refresh_play_ending_snapshot_with_session_cached(
    session: &mut GameSession,
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
    timers: PlayEndingSkinTimers,
    cache: &PlayRenderSnapshotCache,
) -> RenderSnapshot {
    let times = compute_frame_times(session);
    apply_auto_key_release(session, times.audio_now);
    update_recent_judgements(session, &[], times.render_now);
    update_recent_inputs(session, &[], times.render_now);

    let mut snapshot = build_render_snapshot_with_target_and_bga_frames_cached(
        session,
        times.render_now,
        &session.recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
        cache,
    );
    snapshot.play_elapsed_time = timers.play_elapsed_time;
    snapshot.ready_elapsed_time = timers.ready_elapsed_time;
    snapshot.backbmp_background = timers.backbmp_background;
    snapshot.failed_elapsed_ms = timers.failed_elapsed_ms;
    snapshot.music_end_elapsed_ms = timers.music_end_elapsed_ms;
    snapshot.fadeout_elapsed_ms = timers.fadeout_elapsed_ms;
    crate::screens::play_snapshot::refresh_play_skin_visuals(&mut snapshot, session);
    snapshot
}

#[cfg(test)]
mod tests {
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
    fn apply_play_arrange_to_snapshot_sets_skin_label() {
        let mut snapshot = RenderSnapshot::default();

        apply_play_arrange_to_snapshot(&mut snapshot, ArrangeOption::Mirror, ArrangeOption::Random);

        assert_eq!(snapshot.arrange, "MIRROR");
        assert_eq!(snapshot.arrange_2p, "RANDOM");
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
        chart.bgm_events.push(SoundEvent {
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: SoundId(3),
        });
        chart
    }

    fn scheduled_sound(start_frame: u64, sound_id: u32) -> ScheduledSound {
        ScheduledSound {
            start_frame,
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
}
