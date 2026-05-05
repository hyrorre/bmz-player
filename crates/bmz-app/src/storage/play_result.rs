use anyhow::Result;
use bmz_core::clear::ClearType;
use bmz_core::replay::ReplayEvent;
use bmz_gameplay::result::PlayResult;

use crate::config::profile_config::ReplayConfig;
use crate::paths::ProfilePaths;

use super::replay::{ReplayFile, replay_file_name, save_replay};
use super::score_db::{ScoreDatabase, ScoreRecord};

#[derive(Debug, Clone)]
pub struct StoredPlayResult {
    pub score_history_id: i64,
    pub replay_path: String,
}

#[derive(Debug, Clone)]
pub struct StorePlayResultRequest {
    pub played_at: i64,
    pub random_seed: Option<i64>,
    pub gauge_option: String,
    pub assist_mask: u32,
    pub replay_events: Vec<ReplayEvent>,
}

pub fn store_play_result(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    result: &PlayResult,
    request: StorePlayResultRequest,
) -> Result<StoredPlayResult> {
    let replay_path = if should_save_replay(replay_config, result) {
        let file_name = replay_file_name(result.chart_sha256, request.played_at);
        let path = profile_paths.replay_dir.join(&file_name);
        let replay = ReplayFile::new(
            result.chart_sha256,
            request.played_at,
            request.random_seed,
            request.replay_events,
        );
        save_replay(&path, &replay)?;
        format!("replay/{file_name}")
    } else {
        String::new()
    };

    let record = ScoreRecord::from_play_result(
        result,
        request.played_at,
        request.random_seed,
        request.gauge_option,
        request.assist_mask,
        replay_path.clone(),
    );
    let score_history_id = score_db.insert_score(&record)?;

    Ok(StoredPlayResult { score_history_id, replay_path })
}

fn should_save_replay(config: &ReplayConfig, result: &PlayResult) -> bool {
    config.auto_save
        && (!result.autoplay || config.save_autoplay_runs)
        && (result.clear_type != ClearType::Failed || config.save_failed_runs)
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::input::InputKind;
    use bmz_core::lane::Lane;
    use bmz_core::replay::ReplayEvent;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::ScoreState;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

    #[test]
    fn store_play_result_writes_replay_and_score() {
        let root = make_temp_dir("store-result");
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
        let config = ReplayConfig {
            auto_save: true,
            save_failed_runs: false,
            save_autoplay_runs: false,
            compress: false,
        };
        let result = play_result(false);

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                played_at: 1_700_000_060,
                random_seed: Some(77),
                gauge_option: String::new(),
                assist_mask: 0,
                replay_events: vec![ReplayEvent {
                    lane: Lane::Key1,
                    kind: InputKind::Press,
                    time: TimeUs(10),
                }],
            },
        )
        .unwrap();

        assert!(stored.score_history_id > 0);
        assert!(!stored.replay_path.is_empty());
        assert!(root.join(&stored.replay_path).exists());
        assert_eq!(score_db.best_ex_score([4; 32]).unwrap(), Some(0));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_play_result_skips_autoplay_replay_by_default() {
        let root = make_temp_dir("store-autoplay-result");
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
        let config = ReplayConfig {
            auto_save: true,
            save_failed_runs: false,
            save_autoplay_runs: false,
            compress: false,
        };
        let result = play_result(true);

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                played_at: 1_700_000_061,
                random_seed: None,
                gauge_option: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(stored.replay_path, "");
        assert!(!paths.replay_dir.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_play_result_skips_failed_replay_when_disabled() {
        let root = make_temp_dir("store-failed-result");
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
        let config = ReplayConfig {
            auto_save: true,
            save_failed_runs: false,
            save_autoplay_runs: false,
            compress: false,
        };
        let mut result = play_result(false);
        result.clear_type = ClearType::Failed;

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                played_at: 1_700_000_062,
                random_seed: None,
                gauge_option: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(stored.replay_path, "");
        assert!(!paths.replay_dir.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    fn play_result(autoplay: bool) -> PlayResult {
        PlayResult {
            chart_sha256: [4; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 80.0,
            total_notes: 1,
            score: ScoreState::default(),
            autoplay,
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
