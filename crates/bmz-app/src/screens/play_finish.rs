use anyhow::Result;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::session::GameSession;

use crate::config::profile_config::ReplayConfig;
use crate::paths::ProfilePaths;
use crate::storage::play_result::{StorePlayResultRequest, StoredPlayResult, store_play_result};
use crate::storage::score_db::ScoreDatabase;

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
    let result = play_result_from_session(session);
    store_play_result(
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
    )
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
            recent_judgements: Vec::new(),
            bgm_scheduler: BgmScheduler::default(),
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            audio_mix: PlayAudioMix { master_volume: 1.0, key_volume: 1.0, bgm_volume: 1.0 },
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
