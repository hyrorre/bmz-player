use anyhow::{Context, Result, bail};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::loader::{LoadedSampleReport, SampleLoader, WavSampleLoader, load_chart_samples};
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::PlayableChart;
use bmz_gameplay::autoplay::AutoplayController;
use bmz_gameplay::gauge::GaugeState;
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::DefaultInputTranslator;
use bmz_gameplay::judge::engine::JudgeEngine;
use bmz_gameplay::replay::{ReplayPlayer, ReplayRecorder};
use bmz_gameplay::score::ScoreState;
use bmz_gameplay::session::{BgmScheduler, GameSession, PlayState};
use std::sync::Arc;

use crate::config::play::{
    DEFAULT_JUDGE_WINDOW, audio_mix_from_profile, gauge_type_from_config,
    lane_binding_from_profile_input, play_offsets_from_profile,
};
use crate::config::profile_config::ProfileConfig;
use crate::storage::library_db::LibraryDatabase;

#[derive(Debug, Clone)]
pub struct PlaySessionOptions {
    pub autoplay: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub sample_rate: u32,
}

pub struct PreparedPlaySession {
    pub session: GameSession,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
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
    build_game_session_with_input_backend(chart, profile, options, Box::new(NullInputBackend))
}

pub fn build_game_session_with_input_backend(
    chart: Arc<PlayableChart>,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> GameSession {
    let gauge_type = gauge_type_from_config(profile.play.gauge);
    let autoplay = (profile.play.auto_play || options.autoplay).then(AutoplayController::default);
    let input_system = InputSystem {
        backend: input_backend,
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
        audio_clock: AudioClock::stopped(options.sample_rate),
        chart,
        input_system,
        score: ScoreState::default(),
        replay_recorder: ReplayRecorder::default(),
        replay_player: options.replay_player,
        autoplay,
        recent_inputs: Vec::new(),
        recent_judgements: Vec::new(),
        bgm_scheduler: BgmScheduler::default(),
        offsets: play_offsets_from_profile(profile),
        audio_mix: audio_mix_from_profile(profile),
        input_timestamp_anchor: None,
        state: PlayState::Ready,
    }
}

pub fn load_game_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> Result<GameSession> {
    load_game_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        options,
        Box::new(NullInputBackend),
    )
}

pub fn load_game_session_for_chart_with_input_backend(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> Result<GameSession> {
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file not found for chart id {chart_id}");
    };
    let import = import_bms_chart(std::path::Path::new(&path), None)
        .with_context(|| format!("failed to import chart file: {path}"))?;
    Ok(build_game_session_with_input_backend(
        Arc::new(import.chart),
        profile,
        options,
        input_backend,
    ))
}

pub fn build_audio_engine_for_chart(
    chart: &PlayableChart,
    sample_rate: u32,
    loader: &mut dyn SampleLoader,
) -> (AudioEngine, Vec<LoadedSampleReport>) {
    let mut audio = AudioEngine::new(sample_rate);
    let sample_report = load_chart_samples(&mut audio, chart, loader);
    (audio, sample_report)
}

pub fn load_prepared_play_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> Result<PreparedPlaySession> {
    load_prepared_play_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        options,
        Box::new(NullInputBackend),
    )
}

pub fn load_prepared_play_session_for_chart_with_input_backend(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> Result<PreparedPlaySession> {
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file not found for chart id {chart_id}");
    };
    let import = import_bms_chart(std::path::Path::new(&path), None)
        .with_context(|| format!("failed to import chart file: {path}"))?;
    let chart = Arc::new(import.chart);
    let mut loader = WavSampleLoader;
    let (audio, sample_report) =
        build_audio_engine_for_chart(&chart, options.sample_rate, &mut loader);
    let session = build_game_session_with_input_backend(chart, profile, options, input_backend);

    Ok(PreparedPlaySession { session, audio, sample_report })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bmz_audio::loader::LoadedSampleStatus;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart, SoundAssetRef};
    use bmz_core::clear::GaugeType;
    use bmz_core::ids::SoundId;
    use bmz_core::input::InputKind;
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use bmz_gameplay::input::translator::InputTimingContext;
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
        assert_eq!(session.audio_mix.master_volume, 1.0);
        assert_eq!(session.audio_clock.sample_rate, 48_000);
    }

    #[test]
    fn build_game_session_accepts_custom_input_backend() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        let chart = Arc::new(chart());
        let mut session = build_game_session_with_input_backend(
            chart,
            &profile,
            PlaySessionOptions::default(),
            Box::new(backend),
        );
        let ctx = InputTimingContext {
            audio_clock: &session.audio_clock,
            offsets: session.offsets,
            timestamp_anchor: None,
        };

        let inputs = session.input_system.collect_game_inputs(&ctx);

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].lane, Lane::Key1);
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

    #[test]
    fn load_prepared_play_session_for_chart_loads_audio_samples() {
        let (path, wav_path) = write_temp_bms_with_wav(
            "\
#TITLE Prepared
#BPM 120
#WAV01 test.wav
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

        let prepared = load_prepared_play_session_for_chart(
            &library_db,
            chart_id,
            &profile,
            PlaySessionOptions::default(),
        )
        .unwrap();

        assert_eq!(prepared.session.chart.metadata.title, "Prepared");
        assert_eq!(prepared.audio.mixer.output_sample_rate, 48_000);
        assert!(matches!(prepared.sample_report[0].status, LoadedSampleStatus::Loaded));
        assert!(prepared.audio.samples.get(SoundId(0)).is_some());

        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(wav_path).unwrap();
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
            sounds: Vec::<SoundAssetRef>::new(),
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

    fn write_temp_bms_with_wav(text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir();
        let bms_path = dir.join(format!("bmz-prepared-session-{}-{stamp}.bms", std::process::id()));
        let wav_name = format!("bmz-prepared-session-{}-{stamp}.wav", std::process::id());
        let wav_path = dir.join(&wav_name);
        std::fs::write(&bms_path, text.replace("test.wav", &wav_name)).unwrap();
        std::fs::write(
            &wav_path,
            [wav_header(1, 1, 48_000, 16, 2).as_slice(), &[0x00, 0x40]].concat(),
        )
        .unwrap();
        (bms_path, wav_path)
    }

    fn wav_header(
        format: u16,
        channels: u16,
        sample_rate: u32,
        bits: u16,
        data_len: u32,
    ) -> Vec<u8> {
        let byte_rate = sample_rate * channels as u32 * bits as u32 / 8;
        let block_align = channels * bits / 8;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out
    }
}
