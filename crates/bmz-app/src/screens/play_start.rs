use anyhow::Result;
use bmz_chart::model::LongNoteMode;
use bmz_core::course::{
    CourseClassConstraint, CourseConstraints, CourseGaugeConstraint, CourseJudgeConstraint,
    CourseLnConstraint, CourseSpeedConstraint,
};
use bmz_core::time::TimeUs;
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::replay::ReplayPlayer;

use crate::audio::{RunningPlaySession, open_prepared_play_audio};
use crate::config::app_config::AppConfig;
use crate::config::play::{gauge_auto_shift_from_config, gauge_type_from_config};
use crate::config::profile_config::{GaugeAutoShiftConfig, GaugeTypeConfig, ProfileConfig};
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
    pub gauge_auto_shift: GaugeAutoShiftConfig,
    pub arrange: ArrangeOption,
    pub target: TargetOption,
    pub arrange_seed: Option<i64>,
    pub arrange_pattern: Option<Vec<u8>>,
    /// Override the starting gauge value (used to carry the gauge between
    /// charts in a course).  None means use the gauge's default `init`.
    pub initial_gauge_value: Option<f32>,
    /// Course judge constraint (e.g. NoGood / NoGreat).  Forwarded to the
    /// JudgeEngine via PlaySessionOptions::judge_constraint.
    pub judge_constraint: CourseJudgeConstraint,
    /// Override the LN mode for this chart (Ln/Cn/Hcn).  None preserves the
    /// chart's own declaration.  Used by course `ln`/`cn`/`hcn` constraints.
    pub ln_mode_override: Option<LongNoteMode>,
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
        gauge_auto_shift: start_options
            .gauge
            .map(|gauge| gauge_auto_shift_from_config(gauge, start_options.gauge_auto_shift))
            .unwrap_or_default(),
        arrange: start_options.arrange,
        target: start_options.target,
        arrange_seed: start_options.arrange_seed,
        arrange_pattern: start_options.arrange_pattern,
        initial_gauge_value: start_options.initial_gauge_value,
        judge_constraint: start_options.judge_constraint,
        ln_mode_override: start_options.ln_mode_override,
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
    running.best_ghost =
        score_db.best_ghost(chart_sha256, running.session.chart.total_notes).unwrap_or(None);
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
    running.best_ghost =
        score_db.best_ghost(chart_sha256, running.session.chart.total_notes).unwrap_or(None);
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

/// Overrides `options` fields based on the course constraints.
///
/// - Gauge: non-Default constraints override the user's gauge choice with `Normal`.
/// - Arrange: class constraints restrict which arrange options are allowed.
///   If the user's current arrange is not in the allowed set, it falls back to Normal.
pub fn apply_course_constraints(options: &mut PlayStartOptions, constraints: &CourseConstraints) {
    match constraints.gauge {
        CourseGaugeConstraint::Default => {}
        CourseGaugeConstraint::Lr2
        | CourseGaugeConstraint::Keys5
        | CourseGaugeConstraint::Keys7
        | CourseGaugeConstraint::Keys9
        | CourseGaugeConstraint::Keys24 => {
            options.gauge = Some(GaugeTypeConfig::Normal);
        }
    }

    // NoSpeed: enforced at the input-handling layer in WinitApp::route_keyboard_input
    // by reading active_course.definition.constraints.speed.
    let _ = constraints.speed == CourseSpeedConstraint::NoSpeed;

    // Judge constraints are applied at GameSession construction by narrowing
    // the judge window inside play_session_options_from_start.
    options.judge_constraint = constraints.judge;

    // LN constraints force the chart's long-note mode regardless of the chart's
    // own declaration; applied in preload_play_session_for_chart.
    options.ln_mode_override = match constraints.ln {
        CourseLnConstraint::Default => None,
        CourseLnConstraint::Ln => Some(LongNoteMode::Ln),
        CourseLnConstraint::Cn => Some(LongNoteMode::Cn),
        CourseLnConstraint::Hcn => Some(LongNoteMode::Hcn),
    };

    let allowed: &[ArrangeOption] = match constraints.class {
        CourseClassConstraint::None => return,
        CourseClassConstraint::Grade => &[ArrangeOption::Normal],
        CourseClassConstraint::GradeMirrorAllowed => {
            &[ArrangeOption::Normal, ArrangeOption::Mirror]
        }
        CourseClassConstraint::GradeRandomAllowed => {
            &[ArrangeOption::Normal, ArrangeOption::Random]
        }
    };
    if !allowed.contains(&options.arrange) {
        options.arrange = ArrangeOption::Normal;
        options.arrange_seed = None;
        options.arrange_pattern = None;
    }
}

/// Attach a queued course replay to `PlayStartOptions`.
///
/// Sets the replay player and copies the recorded arrange / arrange_seed /
/// lane_shuffle_pattern from the replay file so the chart unfolds exactly as
/// it did at record time.  Must be called *after* `apply_course_constraints`
/// so that constraints don't overwrite the replay's arrange.
pub fn apply_queued_replay(
    options: &mut PlayStartOptions,
    replay: &crate::storage::replay::QueuedCourseReplay,
) {
    let player = bmz_gameplay::replay::ReplayPlayer {
        events: replay.replay.events.clone(),
        next_index: 0,
    };
    options.replay_player = Some(player);
    options.arrange = replay.replay.arrange_option();
    options.arrange_seed = replay.replay.arrange_seed;
    options.arrange_pattern = replay.replay.lane_shuffle_pattern.clone();
    // Replays of past plays were recorded by a human; never autoplay them.
    options.autoplay = false;
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
