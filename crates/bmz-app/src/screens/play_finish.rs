use anyhow::{Result, bail};
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::session::{GameSession, PlayState};

use crate::config::profile_config::ReplayConfig;
use crate::paths::ProfilePaths;
use crate::screens::result_model::ResultSummary;
use crate::storage::play_result::{StorePlayResultRequest, StoredPlayResult, store_play_result};
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone)]
pub struct FinishedPlaySession {
    pub result: PlayResult,
    pub stored: StoredPlayResult,
    pub summary: ResultSummary,
}

pub fn play_result_from_session(session: &GameSession) -> PlayResult {
    PlayResult::from_states(
        &session.chart,
        &session.score,
        &session.gauge,
        session.state,
        session.autoplay.is_some(),
    )
}

pub fn store_session_result(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    session: &GameSession,
    played_at: i64,
) -> Result<StoredPlayResult> {
    Ok(finish_session_result(score_db, profile_paths, replay_config, session, played_at)?.stored)
}

pub fn finish_session_result(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    session: &GameSession,
    played_at: i64,
) -> Result<FinishedPlaySession> {
    ensure_storable_state(session.state)?;
    let result = play_result_from_session(session);
    let stored = store_play_result(
        score_db,
        profile_paths,
        replay_config,
        &result,
        StorePlayResultRequest {
            played_at,
            random_seed: None,
            gauge_option: String::new(),
            assist_mask: 0,
            replay_events: session.replay_recorder.events.clone(),
        },
    )?;
    let summary = ResultSummary::from_play_result(&result, &stored);

    Ok(FinishedPlaySession { result, stored, summary })
}

pub fn finish_session_result_once(
    cached: &mut Option<FinishedPlaySession>,
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    session: &GameSession,
    played_at: i64,
) -> Result<FinishedPlaySession> {
    if let Some(finished) = cached.clone() {
        return Ok(finished);
    }

    let finished =
        finish_session_result(score_db, profile_paths, replay_config, session, played_at)?;
    *cached = Some(finished.clone());
    Ok(finished)
}

fn ensure_storable_state(state: PlayState) -> Result<()> {
    match state {
        PlayState::Finished | PlayState::Failed => Ok(()),
        PlayState::Ready | PlayState::Playing => bail!("play session is not finished yet"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
    use bmz_core::clear::ClearType;
    use bmz_core::ids::NoteId;
    use bmz_core::input::{InputKind, InputSource};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};
    use bmz_gameplay::input::backend::NullInputBackend;
    use bmz_gameplay::input::binding::LaneBinding;
    use bmz_gameplay::input::system::InputSystem;
    use bmz_gameplay::input::translator::DefaultInputTranslator;
    use bmz_gameplay::judge::engine::JudgeEngine;
    use bmz_gameplay::replay::ReplayRecorder;
    use bmz_gameplay::session::{BgmScheduler, GameSession, PlayAudioMix, PlayOffsets, PlayState};
    use rusqlite::Connection;

    use super::*;
    use crate::config::play::DEFAULT_JUDGE_WINDOW;
    use crate::config::profile_config::ReplayConfig;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

    #[test]
    fn play_result_from_session_uses_session_state() {
        let session = session();

        let result = play_result_from_session(&session);

        assert_eq!(result.chart_sha256, session.chart.identity.file_sha256);
        assert_eq!(result.clear_type, ClearType::Failed);
        assert!(!result.autoplay);
    }

    #[test]
    fn store_session_result_writes_replay_events() {
        let root = make_temp_dir("finish-session");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            save_failed_runs: true,
            save_autoplay_runs: false,
            compress: false,
        };
        let mut session = session();
        session.replay_recorder.record(bmz_core::input::InputEvent {
            lane: Lane::Key1,
            kind: InputKind::Press,
            time: TimeUs(10),
            source: InputSource::Human,
        });

        let stored =
            store_session_result(&mut score_db, &paths, &replay_config, &session, 1_700_000_100)
                .unwrap();

        assert!(stored.score_history_id > 0);
        assert!(!stored.replay_path.is_empty());
        assert!(root.join(&stored.replay_path).exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_returns_summary() {
        let root = make_temp_dir("finish-summary");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            save_failed_runs: true,
            save_autoplay_runs: false,
            compress: false,
        };
        let session = session();

        let finished =
            finish_session_result(&mut score_db, &paths, &replay_config, &session, 1_700_000_102)
                .unwrap();

        assert_eq!(finished.summary.score_history_id, finished.stored.score_history_id);
        assert_eq!(finished.summary.clear_type, finished.result.clear_type);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finish_session_result_once_reuses_cached_result() {
        let root = make_temp_dir("finish-once");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            save_failed_runs: true,
            save_autoplay_runs: false,
            compress: false,
        };
        let session = session();
        let mut cached = None;

        let first = finish_session_result_once(
            &mut cached,
            &mut score_db,
            &paths,
            &replay_config,
            &session,
            1_700_000_103,
        )
        .unwrap();
        let second = finish_session_result_once(
            &mut cached,
            &mut score_db,
            &paths,
            &replay_config,
            &session,
            1_700_000_104,
        )
        .unwrap();

        assert_eq!(first.stored.score_history_id, second.stored.score_history_id);
        assert_eq!(score_db.recent_history(10, 0).unwrap().len(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_session_result_rejects_unfinished_session() {
        let root = make_temp_dir("unfinished-session");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let replay_config = ReplayConfig {
            auto_save: true,
            save_failed_runs: true,
            save_autoplay_runs: false,
            compress: false,
        };
        let mut session = session();
        session.state = PlayState::Playing;

        let result =
            store_session_result(&mut score_db, &paths, &replay_config, &session, 1_700_000_101);

        assert!(result.is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    fn session() -> GameSession {
        let chart = Arc::new(chart());
        GameSession {
            chart: Arc::clone(&chart),
            audio_clock: bmz_audio::clock::AudioClock::stopped(48_000),
            input_system: InputSystem {
                backend: Box::new(NullInputBackend),
                translator: Box::new(DefaultInputTranslator {
                    binding: LaneBinding { entries: Vec::new() },
                }),
            },
            judge: JudgeEngine::new(DEFAULT_JUDGE_WINDOW),
            score: Default::default(),
            gauge: bmz_gameplay::gauge::GaugeState::new(
                bmz_core::clear::GaugeType::Normal,
                160.0,
                chart.total_notes,
            ),
            replay_recorder: ReplayRecorder::default(),
            replay_player: None,
            autoplay: None,
            recent_inputs: Vec::new(),
            recent_judgements: Vec::new(),
            bgm_scheduler: BgmScheduler::default(),
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            audio_mix: PlayAudioMix { master_volume: 1.0, key_volume: 1.0, bgm_volume: 1.0 },
            hispeed: 2.0,
            lift: 0.0,
            lane_cover: 0.0,
            hidden_cover: 0.0,
            input_timestamp_anchor: None,
            state: PlayState::Finished,
        }
    }

    fn chart() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: compute_chart_identity(b"finish-session"),
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(0),
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
