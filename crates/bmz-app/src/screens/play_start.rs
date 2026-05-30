use anyhow::Result;
use bmz_core::time::TimeUs;
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::replay::ReplayPlayer;

use crate::audio::{RunningPlaySession, open_prepared_play_audio};
use crate::config::app_config::AppConfig;
use crate::config::play::gauge_type_from_config;
use crate::config::profile_config::{GaugeTypeConfig, ProfileConfig};
use crate::input::winit::WinitInputBackend;
use crate::screens::play_session::{
    PlaySessionOptions, PreloadedPlaySession, PreparedPlaySession,
    build_prepared_play_session_from_preloaded,
    load_prepared_play_session_for_chart_with_input_backend,
};
use crate::select_options::{ArrangeOption, TargetOption};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone, Default)]
pub struct PlayStartOptions {
    pub autoplay: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub chart_zero_time: TimeUs,
    /// Override profile gauge type. None means use the profile default.
    pub gauge: Option<GaugeTypeConfig>,
    pub arrange: ArrangeOption,
    pub target: TargetOption,
    pub arrange_seed: Option<i64>,
    pub arrange_pattern: Option<Vec<u8>>,
}

pub struct StartedWinitPlaySession {
    pub running: RunningPlaySession,
    pub input: WinitInputBackend,
}

pub struct PreparedWinitPlaySession {
    pub prepared: PreparedPlaySession,
    pub input: WinitInputBackend,
}

pub struct PreloadedWinitPlaySession {
    pub preloaded: PreloadedPlaySession,
    pub input: WinitInputBackend,
    pub session_options: PlaySessionOptions,
}

pub fn play_session_options_from_start(
    app_config: &AppConfig,
    start_options: PlayStartOptions,
) -> PlaySessionOptions {
    PlaySessionOptions {
        autoplay: start_options.autoplay,
        replay_player: start_options.replay_player,
        sample_rate: app_config.audio.sample_rate,
        gauge_override: start_options.gauge.map(gauge_type_from_config),
        arrange: start_options.arrange,
        target: start_options.target,
        arrange_seed: start_options.arrange_seed,
        arrange_pattern: start_options.arrange_pattern,
    }
}

pub fn start_running_play_session_for_chart(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
) -> Result<RunningPlaySession> {
    start_running_play_session_for_chart_with_input_backend(
        library_db,
        score_db,
        app_config,
        profile,
        chart_id,
        start_options,
        Box::new(NullInputBackend),
    )
}

pub fn start_running_play_session_for_chart_with_input_backend(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
    input_backend: Box<dyn InputBackend>,
) -> Result<RunningPlaySession> {
    let chart_zero_time = start_options.chart_zero_time;
    let session_options = play_session_options_from_start(app_config, start_options);
    let prepared = load_prepared_play_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        session_options,
        input_backend,
    )?;
    let chart_sha256 = prepared.session.chart.identity.file_sha256;
    let mut running = open_prepared_play_audio(&app_config.audio, prepared)?;
    running.best_ex_score = score_db.best_ex_score(chart_sha256).unwrap_or(None);
    running.start(chart_zero_time)?;
    Ok(running)
}

pub fn prepare_play_session_for_chart_with_winit_input(
    library_db: &LibraryDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
) -> Result<PreparedWinitPlaySession> {
    let input = WinitInputBackend::default();
    let session_options = play_session_options_from_start(app_config, start_options);
    let prepared = load_prepared_play_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        session_options,
        Box::new(input.clone()),
    )?;
    Ok(PreparedWinitPlaySession { prepared, input })
}

pub fn prepare_winit_play_session_from_preloaded(
    profile: &ProfileConfig,
    preloaded: PreloadedWinitPlaySession,
) -> PreparedWinitPlaySession {
    let prepared = build_prepared_play_session_from_preloaded(
        preloaded.preloaded,
        profile,
        preloaded.session_options,
        Box::new(preloaded.input.clone()),
    );
    PreparedWinitPlaySession { prepared, input: preloaded.input }
}

pub fn open_prepared_winit_play_session(
    score_db: &ScoreDatabase,
    app_config: &AppConfig,
    prepared: PreparedWinitPlaySession,
) -> Result<StartedWinitPlaySession> {
    let chart_sha256 = prepared.prepared.session.chart.identity.file_sha256;
    let mut running = open_prepared_play_audio(&app_config.audio, prepared.prepared)?;
    running.best_ex_score = score_db.best_ex_score(chart_sha256).unwrap_or(None);
    Ok(StartedWinitPlaySession { running, input: prepared.input })
}

pub fn start_running_play_session_for_chart_with_winit_input(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
) -> Result<StartedWinitPlaySession> {
    let input = WinitInputBackend::default();
    let running = start_running_play_session_for_chart_with_input_backend(
        library_db,
        score_db,
        app_config,
        profile,
        chart_id,
        start_options,
        Box::new(input.clone()),
    )?;
    Ok(StartedWinitPlaySession { running, input })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::AppConfig;
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};

    #[test]
    fn play_session_options_use_audio_sample_rate() {
        let mut app_config = AppConfig::default();
        app_config.audio.sample_rate = 96_000;

        let options = play_session_options_from_start(
            &app_config,
            PlayStartOptions { autoplay: true, ..Default::default() },
        );

        assert!(options.autoplay);
        assert_eq!(options.sample_rate, 96_000);
        assert!(options.replay_player.is_none());
    }

    #[test]
    fn winit_input_clone_can_feed_session_backend() {
        let event_source = WinitInputBackend::default();
        let mut session_backend = event_source.clone();

        event_source.handle_key_parts(
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Pressed,
            false,
        );

        assert_eq!(session_backend.drain_events().len(), 1);
    }
}
