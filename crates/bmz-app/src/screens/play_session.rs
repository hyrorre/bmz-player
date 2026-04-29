use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use anyhow::{Context, Result, bail};
use bmz_audio::clock::AudioClock;
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::PlayableChart;
use bmz_gameplay::autoplay::AutoplayController;
use bmz_gameplay::gauge::GaugeState;
use bmz_gameplay::input::backend::NullInputBackend;
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::DefaultInputTranslator;
use bmz_gameplay::judge::engine::JudgeEngine;
use bmz_gameplay::replay::{ReplayPlayer, ReplayRecorder};
use bmz_gameplay::score::ScoreState;
use bmz_gameplay::session::{BgmScheduler, GameSession, PlayState};

use crate::config::play::{
    DEFAULT_JUDGE_WINDOW, gauge_type_from_config, lane_binding_from_profile_input,
    play_offsets_from_profile,
};
use crate::config::profile_config::ProfileConfig;
use crate::storage::library_db::LibraryDatabase;

#[derive(Debug, Clone)]
pub struct PlaySessionOptions {
    pub autoplay: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub sample_rate: u32,
}

impl Default for PlaySessionOptions {
    fn default() -> Self {
        Self { autoplay: false, replay_player: None, sample_rate: 48_000 }
    }
}

pub fn build_game_session(
    chart: Arc<PlayableChart>,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> GameSession {
    let gauge_type = gauge_type_from_config(profile.play.gauge);
    let autoplay = (profile.play.auto_play || options.autoplay).then(AutoplayController::default);
    let input_system = InputSystem {
        backend: Box::new(NullInputBackend),
        translator: Box::new(DefaultInputTranslator {
            binding: lane_binding_from_profile_input(&profile.input),
        }),
    };

    GameSession {
        gauge: GaugeState::new(
            gauge_type,
            chart.metadata.total.unwrap_or(160.0),
            chart.total_notes,
        ),
        judge: JudgeEngine::new(DEFAULT_JUDGE_WINDOW),
        audio_clock: AudioClock {
            sample_rate: options.sample_rate,
            start_output_frame: 0,
            chart_zero_time_us: 0,
            current_frame: Arc::new(AtomicU64::new(0)),
            running: false,
        },
        chart,
        input_system,
        score: ScoreState::default(),
        replay_recorder: ReplayRecorder::default(),
        replay_player: options.replay_player,
        autoplay,
        bgm_scheduler: BgmScheduler::default(),
        offsets: play_offsets_from_profile(profile),
        state: PlayState::Ready,
    }
}

pub fn load_game_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> Result<GameSession> {
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file not found for chart id {chart_id}");
    };
    let import = import_bms_chart(std::path::Path::new(&path), None)
        .with_context(|| format!("failed to import chart file: {path}"))?;
    Ok(build_game_session(Arc::new(import.chart), profile, options))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::clear::GaugeType;
    use bmz_core::time::TimeUs;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    #[test]
    fn build_game_session_uses_profile_play_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.auto_play = true;
        profile.judge.input_offset_us = 123;
        let chart = Arc::new(chart());

        let session = build_game_session(chart, &profile, PlaySessionOptions::default());

        assert_eq!(session.state, PlayState::Ready);
        assert_eq!(session.gauge.selected, GaugeType::Normal);
        assert!(session.autoplay.is_some());
        assert_eq!(session.offsets.input_offset_us, 123);
        assert_eq!(session.audio_clock.sample_rate, 48_000);
    }

    #[test]
    fn load_game_session_for_chart_imports_linked_file() {
        let path = write_temp_bms(
            "\
#TITLE Linked
#BPM 120
#00011:01
",
        );
        let imported = import_bms_chart(&path, None).unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(conn);
        let chart_id = library_db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: &path,
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &imported.chart,
            })
            .unwrap();
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let session = load_game_session_for_chart(
            &library_db,
            chart_id,
            &profile,
            PlaySessionOptions::default(),
        )
        .unwrap();

        assert_eq!(session.chart.metadata.title, "Linked");

        std::fs::remove_file(path).unwrap();
    }

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"session"),
            metadata: ChartMetadata {
                title: "session".to_string(),
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
            total_notes: 1,
            end_time: TimeUs(0),
        }
    }

    fn write_temp_bms(text: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-play-session-{}-{stamp}.bms", std::process::id()));
        std::fs::write(&path, text).unwrap();
        path
    }
}
