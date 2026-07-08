use anyhow::Result;
use bmz_chart::model::LongNoteMode;
use bmz_core::clear::GaugeType;
use bmz_core::course::{
    CourseClassConstraint, CourseConstraints, CourseGaugeConstraint, CourseJudgeConstraint,
    CourseLnConstraint, CourseSpeedConstraint,
};
use bmz_core::time::TimeUs;
use bmz_gameplay::gauge::{GaugeCarryValue, GaugeProperty};
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::replay::ReplayPlayer;

use crate::audio::{AudioRuntime, RunningPlaySession, open_prepared_play_audio};
use crate::config::app_config::AppConfig;
use crate::config::play::{
    bottom_shiftable_gauge_from_config, gauge_auto_shift_from_config, gauge_type_from_config,
};
use crate::config::profile_config::{GaugeAutoShiftConfig, GaugeTypeConfig, ProfileConfig};
use crate::input::shared::SharedInputBackend;
use crate::screens::play_session::{
    PlaySessionOptions, PreloadedPlaySession, PreparedPlaySession,
    build_prepared_play_session_from_preloaded,
    load_prepared_play_session_for_chart_with_input_backend,
};
use crate::select_options::{ArrangeOption, DoubleOption, HsFixOption, TargetOption};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone, Default)]
pub struct PlayStartOptions {
    pub autoplay: bool,
    /// Practice mode: section play without result DB update (CLI entry only for now).
    pub practice_mode: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub chart_zero_time: TimeUs,
    /// Override profile gauge type. None means use the profile default.
    pub gauge: Option<GaugeTypeConfig>,
    pub gauge_auto_shift: GaugeAutoShiftConfig,
    pub bottom_shiftable_gauge: crate::config::profile_config::BottomShiftableGaugeConfig,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    pub hs_fix: HsFixOption,
    pub target: TargetOption,
    pub target_ex_score_override: Option<u32>,
    pub arrange_seed: Option<i64>,
    pub arrange_pattern: Option<Vec<u8>>,
    /// Override the starting gauge value (used to carry the gauge between
    /// charts in a course).  None means use the gauge's default `init`.
    pub initial_gauge_value: Option<f32>,
    /// Per-gauge starting values for course carry.  This takes priority over
    /// `initial_gauge_value` when present.
    pub initial_gauge_values: Option<Vec<GaugeCarryValue>>,
    /// Course-mode combo carried from the previous chart. None means this is
    /// not a course carry boundary.
    pub initial_course_combo: Option<u32>,
    /// Course judge constraint (e.g. NoGood / NoGreat).  Forwarded to the
    /// JudgeEngine via PlaySessionOptions::judge_constraint.
    pub judge_constraint: CourseJudgeConstraint,
    /// Override the LN mode for this chart (Ln/Cn/Hcn).  None preserves the
    /// chart's own declaration.  Used by course `ln`/`cn`/`hcn` constraints.
    pub ln_mode_override: Option<LongNoteMode>,
    /// Course-forced gauge override (CLASS / EXCLASS / EXHARDCLASS).
    /// `apply_course_constraints` populates this for course play so the user's
    /// selected gauge translates into a course-only class gauge; takes priority
    /// over `gauge`.
    pub course_gauge_override: Option<GaugeType>,
    /// 段位ゲージの `GaugeProperty` 上書き。`apply_course_constraints` で
    /// `CourseGaugeConstraint::Lr2/Keys5/Keys7/Keys9/Keys24` を解釈して設定。
    /// `None` なら `PlaySessionOptions` 側でチャート由来の値が使われる。
    pub course_gauge_property_override: Option<GaugeProperty>,
}

pub struct StartedInputPlaySession {
    pub running: RunningPlaySession,
    pub input: SharedInputBackend,
}

pub struct PreparedInputPlaySession {
    pub prepared: PreparedPlaySession,
    pub input: SharedInputBackend,
}

pub struct PreloadedInputPlaySession {
    pub chart_id: i64,
    pub preloaded: PreloadedPlaySession,
    pub input: SharedInputBackend,
    pub session_options: PlaySessionOptions,
}

pub fn play_session_options_from_start(
    app_config: &AppConfig,
    start_options: PlayStartOptions,
) -> PlaySessionOptions {
    let gauge_override = start_options
        .course_gauge_override
        .or_else(|| start_options.gauge.map(gauge_type_from_config));
    let gauge_auto_shift = start_options
        .gauge
        .map(|gauge| gauge_auto_shift_from_config(gauge, start_options.gauge_auto_shift))
        .unwrap_or_default();

    PlaySessionOptions {
        autoplay: start_options.autoplay,
        practice_mode: start_options.practice_mode,
        replay_player: start_options.replay_player,
        sample_rate: app_config.audio.sample_rate,
        gauge_override,
        gauge_auto_shift,
        bottom_shiftable_gauge: bottom_shiftable_gauge_from_config(
            start_options.bottom_shiftable_gauge,
        ),
        arrange: start_options.arrange,
        arrange_2p: start_options.arrange_2p,
        double_option: start_options.double_option,
        hs_fix: start_options.hs_fix,
        target: start_options.target,
        target_ex_score_override: start_options.target_ex_score_override,
        arrange_seed: start_options.arrange_seed,
        arrange_pattern: start_options.arrange_pattern,
        initial_gauge_value: start_options.initial_gauge_value,
        initial_gauge_values: start_options.initial_gauge_values,
        initial_course_combo: start_options.initial_course_combo,
        judge_constraint: start_options.judge_constraint,
        ln_mode_override: start_options.ln_mode_override,
        ln_policy_setting: Default::default(),
        rule_mode: Default::default(),
        gauge_property: start_options.course_gauge_property_override,
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
    let runtime = AudioRuntime::open(&app_config.audio)?;
    start_running_play_session_for_chart_with_audio_runtime_and_input_backend(
        library_db,
        score_db,
        app_config,
        profile,
        chart_id,
        start_options,
        input_backend,
        &runtime,
    )
}

pub fn start_running_play_session_for_chart_with_audio_runtime_and_input_backend(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
    input_backend: Box<dyn InputBackend>,
    runtime: &AudioRuntime,
) -> Result<RunningPlaySession> {
    let chart_zero_time = start_options.chart_zero_time;
    let mut session_options = play_session_options_from_start(app_config, start_options);
    session_options.sample_rate = runtime.sample_rate();
    session_options.ln_policy_setting = profile.play.ln_mode_policy;
    let prepared = load_prepared_play_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        session_options,
        input_backend,
    )?;
    let score_key = prepared.score_key;
    let mut running = open_prepared_play_audio(runtime, prepared, score_key);
    running.best_ex_score = score_db.best_ex_score(score_key).unwrap_or(None);
    running.best_ghost =
        score_db.best_ghost(score_key, running.session.chart.total_notes).unwrap_or(None);
    running.start(chart_zero_time)?;
    Ok(running)
}

pub fn prepare_play_session_for_chart_with_winit_input(
    library_db: &LibraryDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
) -> Result<PreparedInputPlaySession> {
    let input = SharedInputBackend::default();
    let mut session_options = play_session_options_from_start(app_config, start_options);
    session_options.ln_policy_setting = profile.play.ln_mode_policy;
    let prepared = load_prepared_play_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        session_options,
        Box::new(input.clone()),
    )?;
    Ok(PreparedInputPlaySession { prepared, input })
}

pub fn prepare_winit_play_session_from_preloaded(
    profile: &ProfileConfig,
    preloaded: PreloadedInputPlaySession,
) -> PreparedInputPlaySession {
    let prepared = build_prepared_play_session_from_preloaded(
        preloaded.preloaded,
        profile,
        preloaded.session_options,
        Box::new(preloaded.input.clone()),
    );
    PreparedInputPlaySession { prepared, input: preloaded.input }
}

pub fn open_prepared_winit_play_session(
    score_db: &ScoreDatabase,
    runtime: &AudioRuntime,
    prepared: PreparedInputPlaySession,
) -> Result<StartedInputPlaySession> {
    let score_key = prepared.prepared.score_key;
    let mut running = open_prepared_play_audio(runtime, prepared.prepared, score_key);
    running.best_ex_score = score_db.best_ex_score(score_key).unwrap_or(None);
    running.best_ghost =
        score_db.best_ghost(score_key, running.session.chart.total_notes).unwrap_or(None);
    Ok(StartedInputPlaySession { running, input: prepared.input })
}

pub fn start_running_play_session_for_chart_with_winit_input(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    app_config: &AppConfig,
    profile: &ProfileConfig,
    chart_id: i64,
    start_options: PlayStartOptions,
) -> Result<StartedInputPlaySession> {
    let input = SharedInputBackend::default();
    let runtime = AudioRuntime::open(&app_config.audio)?;
    let running = start_running_play_session_for_chart_with_audio_runtime_and_input_backend(
        library_db,
        score_db,
        app_config,
        profile,
        chart_id,
        start_options,
        Box::new(input.clone()),
        &runtime,
    )?;
    Ok(StartedInputPlaySession { running, input })
}

/// Overrides `options` fields based on the course constraints.
///
/// - Gauge: course play always uses one of the class gauges (CLASS / EXCLASS /
///   EXHARDCLASS).  We pick which one based on the user's selected gauge type:
///   AssistEasy/Easy/Normal → CLASS, Hard → EXCLASS, ExHard/Hazard/AutoShift →
///   EXHARDCLASS (mirrors beatoraja `GrooveGauge.create`: `type<=2?6:type==3?7:8`).
///   `CourseGaugeConstraint` (gauge_lr2 / gauge_5k / gauge_7k / gauge_9k /
///   gauge_24k) はキーモード別 `GaugeProperty`（FIVEKEYS / SEVENKEYS / PMS /
///   KEYBOARD / LR2）を選び、段位ゲージ係数を決める。`Default` ならチャートの
///   キーモード由来で `PlaySessionOptions` 側が自動推定する。
/// - Arrange: class constraints restrict which arrange options are allowed.
///   If the user's current arrange is not in the allowed set, it falls back to Normal.
pub fn apply_course_constraints(options: &mut PlayStartOptions, constraints: &CourseConstraints) {
    let selected = options.gauge.unwrap_or(GaugeTypeConfig::Normal);
    options.course_gauge_override = Some(course_gauge_for(selected));
    // beatoraja `GrooveGauge.create` の `case GAUGE_X` 分岐に対応。`Default` は
    // チャートのキーモードから推定するため None のまま (play_session 側で導出)。
    options.course_gauge_property_override = match constraints.gauge {
        CourseGaugeConstraint::Default => None,
        CourseGaugeConstraint::Lr2 => Some(GaugeProperty::Lr2),
        CourseGaugeConstraint::Keys5 => Some(GaugeProperty::FiveKeys),
        CourseGaugeConstraint::Keys7 => Some(GaugeProperty::SevenKeys),
        CourseGaugeConstraint::Keys9 => Some(GaugeProperty::Pms),
        CourseGaugeConstraint::Keys24 => Some(GaugeProperty::Keyboard),
    };

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
        CourseClassConstraint::GradeRandomAllowed => &[
            ArrangeOption::Normal,
            ArrangeOption::Mirror,
            ArrangeOption::Random,
            ArrangeOption::RRandom,
            ArrangeOption::SRandom,
            ArrangeOption::Spiral,
        ],
    };
    if !allowed.contains(&options.arrange) {
        options.arrange = ArrangeOption::Normal;
        options.arrange_seed = None;
        options.arrange_pattern = None;
    }
}

/// プレイヤー選択の Gauge から段位ゲージ (CLASS / EXCLASS / EXHARDCLASS) を決める。
/// beatoraja `GrooveGauge.create`: `type<=2?CLASS:type==3?EXCLASS:EXHARDCLASS` 準拠。
/// `AutoShift` は beatoraja に存在しないため EXHARDCLASS にマップする。
fn course_gauge_for(gauge: GaugeTypeConfig) -> GaugeType {
    match gauge {
        GaugeTypeConfig::AssistEasy | GaugeTypeConfig::Easy | GaugeTypeConfig::Normal => {
            GaugeType::Class
        }
        GaugeTypeConfig::Hard => GaugeType::ExClass,
        GaugeTypeConfig::ExHard | GaugeTypeConfig::Hazard | GaugeTypeConfig::AutoShift => {
            GaugeType::ExHardClass
        }
    }
}

/// Attach a queued course replay to `PlayStartOptions`.
///
/// Sets the replay player and copies the recorded arrange / arrange_seed /
/// lane_shuffle_pattern from the replay file so the chart unfolds exactly as
/// it did at record time.  Must be called *after* `apply_course_constraints`
/// so that constraints don't overwrite the replay's arrange.
/// Reproduce a recorded arrange (option / seed / lane shuffle pattern) on a
/// fresh PLAY start.  Unlike [`apply_queued_replay`] this attaches no replay
/// player, so the chart is actually played, not played back.  Must be called
/// *after* `apply_course_constraints` so constraints don't overwrite the
/// arrange.
pub fn apply_arrange_override(
    options: &mut PlayStartOptions,
    arrange: &crate::screens::play_session::AppliedArrange,
) {
    options.arrange = arrange.arrange;
    options.arrange_2p = arrange.arrange_2p;
    options.double_option = arrange.double_option;
    options.arrange_seed = arrange.seed;
    options.arrange_pattern = arrange.pattern.clone();
}

pub fn apply_queued_replay(
    options: &mut PlayStartOptions,
    replay: &crate::storage::replay::QueuedCourseReplay,
) {
    let player =
        bmz_gameplay::replay::ReplayPlayer { events: replay.replay.events.clone(), next_index: 0 };
    options.replay_player = Some(player);
    options.arrange = replay.replay.arrange_option();
    options.arrange_2p = replay.replay.arrange_2p_option();
    options.double_option = replay.replay.double_option();
    options.arrange_seed = replay.replay.arrange_seed;
    options.arrange_pattern = replay.replay.lane_shuffle_pattern.clone();
    // Replays of past plays were recorded by a human; never autoplay them.
    options.autoplay = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::AppConfig;
    use bmz_core::course::CourseGaugeConstraint;
    use bmz_gameplay::gauge::GaugeAutoShiftMode;
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};

    #[test]
    fn apply_arrange_override_copies_arrange_without_replay() {
        use crate::screens::play_session::AppliedArrange;
        use crate::select_options::ArrangeOption;

        let mut options = PlayStartOptions::default();
        let arrange = AppliedArrange {
            arrange: ArrangeOption::Random,
            arrange_2p: ArrangeOption::Mirror,
            double_option: crate::select_options::DoubleOption::Flip,
            seed: Some(42),
            pattern: Some(vec![3, 1, 2, 0]),
        };
        apply_arrange_override(&mut options, &arrange);

        assert_eq!(options.arrange, ArrangeOption::Random);
        assert_eq!(options.arrange_2p, ArrangeOption::Mirror);
        assert_eq!(options.double_option, crate::select_options::DoubleOption::Flip);
        assert_eq!(options.arrange_seed, Some(42));
        assert_eq!(options.arrange_pattern, Some(vec![3, 1, 2, 0]));
        // Unlike a replay, no playback player is attached: the chart is played.
        assert!(options.replay_player.is_none());
    }

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

    fn default_constraints() -> CourseConstraints {
        CourseConstraints {
            gauge: CourseGaugeConstraint::Default,
            judge: CourseJudgeConstraint::Normal,
            ln: CourseLnConstraint::Default,
            speed: CourseSpeedConstraint::Free,
            class: CourseClassConstraint::None,
            source_constraints: Vec::new(),
        }
    }

    #[test]
    fn course_constraints_pick_class_gauge_for_groove_selections() {
        for (selected, expected) in [
            (GaugeTypeConfig::AssistEasy, GaugeType::Class),
            (GaugeTypeConfig::Easy, GaugeType::Class),
            (GaugeTypeConfig::Normal, GaugeType::Class),
            (GaugeTypeConfig::Hard, GaugeType::ExClass),
            (GaugeTypeConfig::ExHard, GaugeType::ExHardClass),
            (GaugeTypeConfig::Hazard, GaugeType::ExHardClass),
            (GaugeTypeConfig::AutoShift, GaugeType::ExHardClass),
        ] {
            let mut options = PlayStartOptions { gauge: Some(selected), ..Default::default() };
            apply_course_constraints(&mut options, &default_constraints());
            assert_eq!(
                options.course_gauge_override,
                Some(expected),
                "selected {selected:?} should map to {expected:?}"
            );
        }
    }

    #[test]
    fn course_gauge_override_keeps_auto_shift_in_session_options() {
        let app_config = AppConfig::default();
        let mut options =
            PlayStartOptions { gauge: Some(GaugeTypeConfig::AutoShift), ..Default::default() };
        apply_course_constraints(&mut options, &default_constraints());

        let session = play_session_options_from_start(&app_config, options);

        assert_eq!(session.gauge_override, Some(GaugeType::ExHardClass));
        assert_eq!(session.gauge_auto_shift, GaugeAutoShiftMode::BestClear);
    }

    #[test]
    fn course_gauge_constraint_maps_to_gauge_property() {
        let cases = [
            (CourseGaugeConstraint::Default, None),
            (CourseGaugeConstraint::Lr2, Some(GaugeProperty::Lr2)),
            (CourseGaugeConstraint::Keys5, Some(GaugeProperty::FiveKeys)),
            (CourseGaugeConstraint::Keys7, Some(GaugeProperty::SevenKeys)),
            (CourseGaugeConstraint::Keys9, Some(GaugeProperty::Pms)),
            (CourseGaugeConstraint::Keys24, Some(GaugeProperty::Keyboard)),
        ];
        for (constraint, expected_property) in cases {
            let mut options =
                PlayStartOptions { gauge: Some(GaugeTypeConfig::Hard), ..Default::default() };
            let mut constraints = default_constraints();
            constraints.gauge = constraint;
            apply_course_constraints(&mut options, &constraints);
            // 段位ゲージ自体は CourseGaugeConstraint に依存しない（プレイヤー選択ゲージから決定）。
            assert_eq!(options.course_gauge_override, Some(GaugeType::ExClass));
            // CourseGaugeConstraint からは GaugeProperty が決まる。
            assert_eq!(options.course_gauge_property_override, expected_property, "{constraint:?}",);
        }
    }

    #[test]
    fn course_gauge_property_override_reaches_session_options() {
        let app_config = AppConfig::default();
        let mut options =
            PlayStartOptions { gauge: Some(GaugeTypeConfig::Hard), ..Default::default() };
        let mut constraints = default_constraints();
        constraints.gauge = CourseGaugeConstraint::Lr2;
        apply_course_constraints(&mut options, &constraints);

        let session = play_session_options_from_start(&app_config, options);

        assert_eq!(session.gauge_property, Some(GaugeProperty::Lr2));
    }

    #[test]
    fn winit_input_clone_can_feed_session_backend() {
        let event_source = SharedInputBackend::default();
        let mut session_backend = event_source.clone();

        crate::input::winit::handle_key_parts(
            &event_source,
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Pressed,
            false,
        );

        assert_eq!(session_backend.drain_events().len(), 1);
    }
}
