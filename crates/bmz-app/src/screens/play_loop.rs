use anyhow::{Result, anyhow};
use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::queue::AudioScheduler;
use bmz_gameplay::session::{FrameOutput, GameSession, PlayState, advance_session_frame};
use bmz_render::snapshot::RenderSnapshot;

use crate::audio::RunningPlaySession;
use crate::config::profile_config::ReplayConfig;
use crate::paths::ProfilePaths;
use crate::screens::play_finish::{
    FinishedPlaySession, finish_session_result, finish_session_result_once,
};
use crate::screens::play_snapshot::build_render_snapshot;
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone)]
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
) -> FrameOutput<RenderSnapshot> {
    let frame = advance_session_frame(session, audio);
    let render_snapshot =
        build_render_snapshot(session, frame.times.render_now, &session.recent_judgements);
    FrameOutput { render_snapshot, state: frame.state }
}

pub fn advance_play_screen_until_result(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    played_at: i64,
) -> Result<PlayAdvanceOutcome> {
    let frame = advance_play_screen(session, audio);
    if matches!(frame.state, PlayState::Finished | PlayState::Failed) {
        let finished =
            finish_session_result(score_db, profile_paths, replay_config, session, played_at)?;
        return Ok(PlayAdvanceOutcome::Finished { frame, finished });
    }

    Ok(PlayAdvanceOutcome::Playing(frame))
}

pub fn advance_play_screen_with_shared_audio(
    session: &mut GameSession,
    audio: &SharedAudioEngine,
) -> Result<FrameOutput<RenderSnapshot>> {
    let mut audio = audio.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
    Ok(advance_play_screen(session, &mut *audio))
}

pub fn advance_running_play_session(
    running: &mut RunningPlaySession,
) -> Result<FrameOutput<RenderSnapshot>> {
    advance_play_screen_with_shared_audio(&mut running.session, &running.audio.engine)
}

pub fn advance_running_play_session_until_result(
    running: &mut RunningPlaySession,
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    played_at: i64,
) -> Result<PlayAdvanceOutcome> {
    if let Some(finished) = running.finished.clone() {
        pause_running_audio_after_finish(running);
        return Ok(PlayAdvanceOutcome::Finished {
            frame: current_play_frame(&running.session),
            finished,
        });
    }

    let frame = {
        let mut audio =
            running.audio.engine.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
        advance_play_screen(&mut running.session, &mut *audio)
    };
    if matches!(frame.state, PlayState::Finished | PlayState::Failed) {
        let finished = finish_session_result_once(
            &mut running.finished,
            score_db,
            profile_paths,
            replay_config,
            &running.session,
            played_at,
        )?;
        pause_running_audio_after_finish(running);
        return Ok(PlayAdvanceOutcome::Finished { frame, finished });
    }

    Ok(PlayAdvanceOutcome::Playing(frame))
}

fn pause_running_audio_after_finish(running: &mut RunningPlaySession) {
    if running.audio_paused_after_finish {
        return;
    }

    match running.audio.pause() {
        Ok(()) => {
            running.audio_paused_after_finish = true;
        }
        Err(error) => {
            tracing::warn!(%error, "failed to pause audio output after play finished");
        }
    }
}

fn current_play_frame(session: &GameSession) -> FrameOutput<RenderSnapshot> {
    let times = bmz_gameplay::session::compute_frame_times(session);
    let render_snapshot =
        build_render_snapshot(session, times.render_now, &session.recent_judgements);
    FrameOutput { render_snapshot, state: session.state }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use bmz_audio::backend::cpal::SharedAudioEngine;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::queue::{AudioScheduler, ScheduledSound};
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use crate::config::profile_config::ProfileConfig;
    use crate::config::profile_config::ReplayConfig;
    use crate::screens::play_session::{PlaySessionOptions, build_game_session};
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};
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

        let frame = advance_play_screen(&mut session, &mut audio);

        assert_eq!(frame.render_snapshot.time, TimeUs(0));
        assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    }

    #[test]
    fn advance_play_screen_with_shared_audio_locks_engine() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));

        let frame = advance_play_screen_with_shared_audio(&mut session, &audio).unwrap();

        assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
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
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: false,
            save_failed_runs: false,
            save_autoplay_runs: false,
            compress: false,
        };

        let outcome = advance_play_screen_until_result(
            &mut session,
            &mut audio,
            &mut score_db,
            &paths,
            &replay_config,
            1_700_000_200,
        )
        .unwrap();

        assert!(matches!(outcome, PlayAdvanceOutcome::Finished { .. }));
        assert!(outcome.is_finished());
        assert!(outcome.finished().is_some());
        assert_eq!(outcome.frame().state, PlayState::Finished);

        std::fs::remove_dir_all(root).unwrap();
    }

    fn chart() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
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
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
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
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(-1_000_000),
        }
    }

    fn make_temp_dir(label: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-app-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
