use anyhow::Result;
use bmz_chart::model::LongNoteMode;
use bmz_core::clear::GaugeType;
use bmz_core::course::{
    CourseClassConstraint, CourseConstraints, CourseGaugeConstraint, CourseJudgeConstraint,
    CourseLnConstraint, CourseSpeedConstraint,
};
use bmz_core::lane::KeyMode;
use bmz_core::time::TimeUs;
use bmz_gameplay::gauge::{GaugeCarryValue, GaugeProperty};
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::replay::ReplayPlayer;

use crate::audio::{AudioRuntime, RunningPlaySession, open_prepared_play_audio};
use crate::config::app_config::AppConfig;
use crate::config::play::{
    bottom_shiftable_gauge_from_config, gauge_auto_shift_from_config, gauge_type_from_config,
};
use crate::config::profile_config::{
    AssistOptionConfig, GaugeAutoShiftConfig, GaugeTypeConfig, KeyModeConversionConfig,
    ProfileConfig, SevenToNinePattern, SevenToNineRuleMode, SevenToNineType,
};
use crate::input::gamepad::GamepadSlotMap;
use crate::input::shared::SharedInputBackend;
use crate::screens::play_session::{
    BattleOpponentOptions, PlaySessionOptions, PreloadedPlaySession, PreparedPlaySession,
    SRandomScheme, build_practice_prepared_from_preloaded,
    build_prepared_play_session_from_preloaded,
    load_prepared_play_session_for_chart_with_input_backend,
};
use crate::screens::practice::PracticeProperty;
use crate::select_options::{
    ArrangeOption, DoubleOption, HsFixOption, ResolvedTarget, SessionMode, TargetOption,
};
use crate::storage::library_db::LibraryDatabase;
use crate::storage::replay::ReplayFile;
use crate::storage::score_db::ScoreDatabase;

#[derive(Debug, Clone)]
pub struct BattleTarget {
    pub provider: String,
    pub score_id: String,
    pub player_id: String,
    pub player_name: String,
    pub rank: u32,
    pub ex_score: u32,
    pub gauge: Option<GaugeType>,
    pub playback: BattleTargetPlayback,
}

#[derive(Debug, Clone)]
pub enum BattleTargetPlayback {
    Replay(Box<ReplayFile>),
    Seed {
        arrange: ArrangeOption,
        arrange_2p: ArrangeOption,
        double_option: DoubleOption,
        packed_seed: Option<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BattleTargetArrangement {
    pub(crate) arrange: ArrangeOption,
    pub(crate) arrange_2p: ArrangeOption,
    pub(crate) double_option: DoubleOption,
    pub(crate) arrange_seed: Option<i64>,
    pub(crate) arrange_seed_2p: Option<i64>,
    pub(crate) packed_seed: Option<i64>,
    pub(crate) arrange_pattern: Option<Vec<u8>>,
    pub(crate) legacy_arrange_seed: bool,
    pub(crate) s_random_scheme: SRandomScheme,
    pub(crate) s_random_scheme_2p: Option<SRandomScheme>,
    pub(crate) h_random_threshold_ms: Option<u32>,
}

impl BattleTargetPlayback {
    pub(crate) fn arrangement(&self) -> BattleTargetArrangement {
        match self {
            Self::Replay(replay) => BattleTargetArrangement {
                arrange: replay.arrange_option(),
                arrange_2p: replay.arrange_2p_option(),
                double_option: replay.double_option(),
                arrange_seed: replay.arrange_seed,
                arrange_seed_2p: replay.arrange_seed_2p,
                packed_seed: None,
                arrange_pattern: replay.lane_shuffle_pattern.clone(),
                legacy_arrange_seed: replay.uses_legacy_seed_scheme(),
                s_random_scheme: replay.effective_s_random_scheme().unwrap_or_default(),
                s_random_scheme_2p: replay.effective_s_random_scheme_2p().ok(),
                h_random_threshold_ms: replay.h_random_threshold_ms,
            },
            Self::Seed { arrange, arrange_2p, double_option, packed_seed } => {
                BattleTargetArrangement {
                    arrange: *arrange,
                    arrange_2p: *arrange_2p,
                    double_option: *double_option,
                    arrange_seed: None,
                    arrange_seed_2p: None,
                    packed_seed: *packed_seed,
                    arrange_pattern: None,
                    legacy_arrange_seed: false,
                    s_random_scheme: SRandomScheme::default(),
                    s_random_scheme_2p: None,
                    h_random_threshold_ms: None,
                }
            }
        }
    }
}

/// G-BATTLEの相手は元譜面のプレイヤー側入力だけを再生する。
///
/// 旧BMZでは5K/7Kのバトル表示中に2P側キーバインドが有効だったため、表示用入力が
/// リプレイへ混入することがあった。また、同じpollで得た複数入力のtimestampが数usだけ
/// 前後する場合がある。再生対象を1P側へ限定し、安定sortして既存リプレイを救済する。
pub(crate) fn normalize_battle_replay_for_key_mode(
    replay: &mut ReplayFile,
    key_mode: KeyMode,
) -> Result<()> {
    if replay.events.is_empty() {
        anyhow::bail!("battle score has no full input replay");
    }
    let has_arrangement_pattern =
        replay.lane_shuffle_pattern.as_ref().is_some_and(|pattern| !pattern.is_empty());
    let missing_1p_arrangement =
        !matches!(replay.arrange_option(), ArrangeOption::Normal | ArrangeOption::Mirror)
            && replay.arrange_seed.is_none()
            && !has_arrangement_pattern;
    let missing_2p_arrangement =
        !matches!(replay.arrange_2p_option(), ArrangeOption::Normal | ArrangeOption::Mirror)
            && replay.arrange_seed_2p.is_none()
            && !has_arrangement_pattern;
    if missing_1p_arrangement || missing_2p_arrangement {
        anyhow::bail!("battle replay has no seed or pattern for its recorded arrangement");
    }
    let active_lanes = key_mode.active_lanes();
    replay.events.retain(|event| active_lanes.contains(&event.lane));
    if replay.events.is_empty() {
        anyhow::bail!("battle replay has no playable input for the selected key mode");
    }
    replay.events.sort_by_key(|event| event.time);
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct PlayStartOptions {
    pub session_mode: SessionMode,
    pub autoplay: bool,
    pub key_mode_conversion: KeyModeConversionConfig,
    pub seven_to_nine_pattern: SevenToNinePattern,
    pub seven_to_nine_type: SevenToNineType,
    pub seven_to_nine_rule_mode: SevenToNineRuleMode,
    pub score_save_disabled: bool,
    pub playback_rate_percent: u16,
    pub assist: AssistOptionConfig,
    pub replay_player: Option<ReplayPlayer>,
    /// Target required by `SessionMode::GBattle`.
    pub battle_target: Option<BattleTarget>,
    pub chart_zero_time: TimeUs,
    /// Override profile gauge type. None means use the profile default.
    pub gauge: Option<GaugeTypeConfig>,
    /// Gauge stored in a replay, which takes priority over the current profile.
    pub replay_gauge_override: Option<GaugeType>,
    pub gauge_auto_shift: GaugeAutoShiftConfig,
    pub bottom_shiftable_gauge: crate::config::profile_config::BottomShiftableGaugeConfig,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    pub hs_fix: HsFixOption,
    pub target: TargetOption,
    pub resolved_target: Option<ResolvedTarget>,
    /// 選択ライバルの表示名。譜面のスコア有無とは独立。
    pub rival_name: Option<String>,
    pub arrange_seed: Option<i64>,
    pub arrange_seed_2p: Option<i64>,
    /// Fresh play 用 Random Trainer seed。7K の通常 RANDOM かつ記録済み pattern が
    /// 無い場合だけ `PlaySessionOptions` 側で 1P seed より優先する。
    pub random_trainer_seed: Option<i64>,
    pub legacy_arrange_seed: bool,
    pub s_random_scheme: SRandomScheme,
    pub s_random_scheme_2p: Option<SRandomScheme>,
    pub h_random_threshold_ms: Option<u32>,
    pub bms_random_seed: Option<u64>,
    pub bms_random_choices: Option<Vec<i32>>,
    pub bms_switch_choices: Option<Vec<u64>>,
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
    /// Course speed constraint. `NoSpeed` forces HS 1.0 with no lane covers
    /// for the duration of the course without changing the saved profile.
    pub speed_constraint: CourseSpeedConstraint,
    /// Course fallback for undefined long notes (Ln/Cn/Hcn). AUTO settings use
    /// this instead of their configured fallback while preserving explicitly
    /// typed notes. FORCE settings ignore it and convert every long note.
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

impl PreloadedInputPlaySession {
    pub fn clone_loaded_resources(&self) -> Self {
        Self {
            chart_id: self.chart_id,
            preloaded: self.preloaded.clone_loaded_resources(),
            input: SharedInputBackend::default(),
            session_options: self.session_options.clone(),
        }
    }
}

pub fn play_session_options_from_start(
    app_config: &AppConfig,
    start_options: PlayStartOptions,
) -> PlaySessionOptions {
    let gauge_override = start_options
        .course_gauge_override
        .or(start_options.replay_gauge_override)
        .or_else(|| start_options.gauge.map(gauge_type_from_config));
    let gauge_auto_shift = start_options
        .gauge
        .map(|gauge| gauge_auto_shift_from_config(gauge, start_options.gauge_auto_shift))
        .unwrap_or_default();
    let battle_opponent = start_options.battle_target.as_ref().map(|target| {
        let arrangement = target.playback.arrangement();
        let (replay_player, bms_random_choices, bms_switch_choices) = match &target.playback {
            BattleTargetPlayback::Replay(replay) => (
                Some(replay.player()),
                replay.bms_random_choices.clone(),
                replay.bms_switch_choices.clone(),
            ),
            BattleTargetPlayback::Seed { .. } => (None, None, None),
        };
        BattleOpponentOptions {
            replay_player,
            gauge: target.gauge,
            arrange: arrangement.arrange,
            arrange_2p: arrangement.arrange_2p,
            double_option: arrangement.double_option,
            arrange_seed: arrangement.arrange_seed,
            arrange_seed_2p: arrangement.arrange_seed_2p,
            legacy_arrange_seed: arrangement.legacy_arrange_seed,
            packed_seed: arrangement.packed_seed,
            bms_random_choices,
            bms_switch_choices,
            arrange_pattern: arrangement.arrange_pattern,
            s_random_scheme: arrangement.s_random_scheme,
            s_random_scheme_2p: arrangement.s_random_scheme_2p,
            h_random_threshold_ms: arrangement.h_random_threshold_ms,
        }
    });

    PlaySessionOptions {
        play_config_key_mode: None,
        session_mode: start_options.session_mode,
        autoplay: start_options.autoplay,
        key_mode_conversion: start_options.key_mode_conversion,
        seven_to_nine_pattern: start_options.seven_to_nine_pattern,
        seven_to_nine_type: start_options.seven_to_nine_type,
        seven_to_nine_rule_mode: start_options.seven_to_nine_rule_mode,
        score_save_disabled: start_options.score_save_disabled,
        playback_rate_percent: bmz_audio::clock::clamp_playback_rate_percent(
            if start_options.playback_rate_percent == 0 {
                100
            } else {
                start_options.playback_rate_percent
            },
        ),
        assist: start_options.assist,
        assist_runtime: Default::default(),
        replay_player: start_options.replay_player,
        battle_opponent,
        opponent_chart: None,
        sample_rate: app_config.audio.sample_rate,
        gauge_override,
        opponent_gauge_override: start_options
            .battle_target
            .as_ref()
            .and_then(|target| target.gauge),
        gauge_auto_shift,
        bottom_shiftable_gauge: bottom_shiftable_gauge_from_config(
            start_options.bottom_shiftable_gauge,
        ),
        arrange: start_options.arrange,
        arrange_2p: start_options.arrange_2p,
        double_option: start_options.double_option,
        hs_fix: start_options.hs_fix,
        target: start_options.target,
        resolved_target: start_options.resolved_target,
        rival_name: start_options.rival_name,
        arrange_seed: start_options.arrange_seed,
        arrange_seed_2p: start_options.arrange_seed_2p,
        random_trainer_seed: start_options.random_trainer_seed,
        legacy_arrange_seed: start_options.legacy_arrange_seed,
        s_random_scheme: start_options.s_random_scheme,
        s_random_scheme_2p: start_options.s_random_scheme_2p,
        h_random_threshold_ms: start_options.h_random_threshold_ms,
        bms_random_seed: start_options.bms_random_seed,
        bms_random_choices: start_options.bms_random_choices,
        bms_switch_choices: start_options.bms_switch_choices,
        arrange_pattern: start_options.arrange_pattern,
        initial_gauge_value: start_options.initial_gauge_value,
        initial_gauge_values: start_options.initial_gauge_values,
        initial_course_combo: start_options.initial_course_combo,
        judge_constraint: start_options.judge_constraint,
        speed_constraint: start_options.speed_constraint,
        ln_mode_override: start_options.ln_mode_override,
        ln_policy_setting: Default::default(),
        rule_mode: Default::default(),
        gauge_property: start_options.course_gauge_property_override,
        gamepad_slots: GamepadSlotMap::from_runtime_or_legacy(
            app_config.input.gamepad_slot_runtime_device_ids,
            app_config.input.gamepad_slot_gilrs_ids,
        ),
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
    // 表示値とFIRST_PLAY判定にはscore保存可否にかかわらず既存履歴が必要。
    running.best_ex_score = score_db.best_ex_score(score_key).unwrap_or(None);
    let lamp = score_db
        .best_scores_for_charts(&[score_key])
        .ok()
        .and_then(|scores| {
            scores
                .first()
                .map(|score| bmz_core::clear::ClearType::rank_from_label(&score.clear_type))
        })
        .unwrap_or(0);
    running.gameplay.configure_prepared(|session| session.conditional.best_lamp = lamp);
    if !running.score_save_disabled {
        running.best_ghost =
            score_db.best_ghost(score_key, running.session.scored_total_notes).unwrap_or(None);
    }
    resolve_local_target_ex_score(&mut running);
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

pub fn prepare_practice_winit_play_session_from_preloaded(
    profile: &ProfileConfig,
    property: &PracticeProperty,
    preloaded: PreloadedInputPlaySession,
) -> PreparedInputPlaySession {
    let prepared = build_practice_prepared_from_preloaded(
        preloaded.preloaded,
        profile,
        property,
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
    // 表示値とFIRST_PLAY判定にはscore保存可否にかかわらず既存履歴が必要。
    running.best_ex_score = score_db.best_ex_score(score_key).unwrap_or(None);
    let lamp = score_db
        .best_scores_for_charts(&[score_key])
        .ok()
        .and_then(|scores| {
            scores
                .first()
                .map(|score| bmz_core::clear::ClearType::rank_from_label(&score.clear_type))
        })
        .unwrap_or(0);
    running.gameplay.configure_prepared(|session| session.conditional.best_lamp = lamp);
    if !running.score_save_disabled {
        running.best_ghost =
            score_db.best_ghost(score_key, running.session.scored_total_notes).unwrap_or(None);
    }
    resolve_local_target_ex_score(&mut running);
    Ok(StartedInputPlaySession { running, input: prepared.input })
}

fn resolve_local_target_ex_score(running: &mut RunningPlaySession) {
    if running.resolved_target.is_some() {
        return;
    }
    // 既存挙動どおり、保存無効プレイではMyBestターゲットの解決にDB値を使わない。
    // best_ex_score自体はskin表示とFIRST_PLAY判定のため保持する。
    let target_best = if running.score_save_disabled { None } else { running.best_ex_score };
    running.target_ex_score = running
        .target_option
        .target_ex_score_with_best(running.session.scored_total_notes, target_best);
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

    // NoSpeed is applied while constructing both the placeholder and real
    // session, then kept locked by the app-side input handling.
    options.speed_constraint = constraints.speed;

    // Judge constraints are applied at GameSession construction by narrowing
    // the judge window inside play_session_options_from_start.
    options.judge_constraint = constraints.judge;

    // In beatoraja, LN constraints replace PlayerConfig.lnmode for the course.
    // BMZ applies that as the undefined-LN fallback for AUTO; FORCE remains the
    // stronger explicit player choice and ignores this value.
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
        options.arrange_pattern = None;
    }
}

/// プレイヤー選択の Gauge から段位ゲージ (CLASS / EXCLASS / EXHARDCLASS) を決める。
/// beatoraja `GrooveGauge.create`: `type<=2?CLASS:type==3?EXCLASS:EXHARDCLASS` 準拠。
/// `AutoShift` は beatoraja に存在しないため EXHARDCLASS にマップする。
pub(crate) fn course_gauge_for(gauge: GaugeTypeConfig) -> GaugeType {
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
    options.arrange_seed_2p = arrange.seed_2p;
    options.legacy_arrange_seed = arrange.legacy_seed;
    options.s_random_scheme = arrange.s_random_scheme;
    options.s_random_scheme_2p = arrange.s_random_scheme_2p;
    options.h_random_threshold_ms = arrange.h_random_threshold_ms;
    options.bms_random_choices = Some(arrange.bms_random_choices.clone());
    options.bms_switch_choices = Some(arrange.bms_switch_choices.clone());
    options.arrange_pattern = arrange.pattern.clone();
    options.key_mode_conversion = arrange.key_mode_conversion;
    options.seven_to_nine_pattern = arrange.seven_to_nine_pattern;
    options.seven_to_nine_type = arrange.seven_to_nine_type;
    options.seven_to_nine_rule_mode = arrange.seven_to_nine_rule_mode;
    options.score_save_disabled |= arrange.score_persistence_disabled();
}

pub fn apply_queued_replay(
    options: &mut PlayStartOptions,
    replay: &crate::storage::replay::QueuedCourseReplay,
) -> Result<()> {
    let player = replay.replay.player();
    options.replay_player = Some(player);
    options.arrange = replay.replay.arrange_option();
    options.arrange_2p = replay.replay.arrange_2p_option();
    options.double_option = replay.replay.double_option();
    options.arrange_seed = replay.replay.arrange_seed;
    options.arrange_seed_2p = replay.replay.arrange_seed_2p;
    options.legacy_arrange_seed = replay.replay.uses_legacy_seed_scheme();
    options.s_random_scheme = replay.replay.effective_s_random_scheme()?;
    options.s_random_scheme_2p = Some(replay.replay.effective_s_random_scheme_2p()?);
    options.h_random_threshold_ms = replay.replay.h_random_threshold_ms;
    options.replay_gauge_override = replay.replay.recorded_gauge_type();
    options.bms_random_choices = replay.replay.bms_random_choices.clone();
    options.bms_switch_choices = replay.replay.bms_switch_choices.clone();
    options.arrange_pattern = replay.replay.lane_shuffle_pattern.clone();
    // Replays of past plays were recorded by a human; never autoplay them.
    options.autoplay = false;
    Ok(())
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
            seed_2p: Some(24),
            legacy_seed: false,
            s_random_scheme: SRandomScheme::Legacy40MsV1,
            s_random_scheme_2p: Some(SRandomScheme::Lm120HzV1),
            h_random_threshold_ms: Some(125),
            bms_random_choices: vec![2],
            bms_switch_choices: vec![2_000_000_000_000],
            pattern: Some(vec![3, 1, 2, 0]),
            key_mode_conversion: KeyModeConversionConfig::SevenToSix,
            seven_to_nine_pattern: SevenToNinePattern::default(),
            seven_to_nine_type: SevenToNineType::default(),
            seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
        };
        apply_arrange_override(&mut options, &arrange);

        assert_eq!(options.arrange, ArrangeOption::Random);
        assert_eq!(options.arrange_2p, ArrangeOption::Mirror);
        assert_eq!(options.double_option, crate::select_options::DoubleOption::Flip);
        assert_eq!(options.arrange_seed, Some(42));
        assert_eq!(options.s_random_scheme, SRandomScheme::Legacy40MsV1);
        assert_eq!(options.s_random_scheme_2p, Some(SRandomScheme::Lm120HzV1));
        assert_eq!(options.arrange_pattern, Some(vec![3, 1, 2, 0]));
        assert_eq!(options.key_mode_conversion, KeyModeConversionConfig::SevenToSix);
        assert!(options.score_save_disabled);
        // Unlike a replay, no playback player is attached: the chart is played.
        assert!(options.replay_player.is_none());
    }

    #[test]
    fn battle_target_arrangement_reads_replay_randomization() {
        let replay = ReplayFile::new(
            [1; 32],
            1,
            Some(42),
            ArrangeOption::SRandom,
            Some(42),
            Some(vec![2, 0, 1]),
            Vec::new(),
        )
        .with_randomization(Some(24), Vec::new(), Vec::new())
        .with_seed_scheme(crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3)
        .with_s_random_schemes(SRandomScheme::Legacy40MsV1, Some(SRandomScheme::Lm120HzV1));

        let arrangement = BattleTargetPlayback::Replay(Box::new(replay)).arrangement();

        assert_eq!(arrangement.arrange, ArrangeOption::SRandom);
        assert_eq!(arrangement.arrange_seed, Some(42));
        assert_eq!(arrangement.arrange_seed_2p, Some(24));
        assert_eq!(arrangement.arrange_pattern, Some(vec![2, 0, 1]));
        assert!(arrangement.legacy_arrange_seed);
        assert_eq!(arrangement.s_random_scheme, SRandomScheme::Legacy40MsV1);
        assert_eq!(arrangement.s_random_scheme_2p, Some(SRandomScheme::Lm120HzV1));
    }

    #[test]
    fn battle_replay_normalization_keeps_source_lanes_and_stably_orders_them() {
        use bmz_core::input::{InputDeviceKind, InputKind};
        use bmz_core::lane::Lane;
        use bmz_core::replay::ReplayEvent;

        let event = |lane, time, kind| ReplayEvent {
            lane,
            kind,
            time: TimeUs(time),
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        };
        let mut replay =
            ReplayFile::new([1; 32], 1, None, ArrangeOption::Normal, None, None, Vec::new())
                .with_seed_scheme(crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3);
        // Assign directly to model a replay loaded from an older, unsorted file.
        replay.events = vec![
            event(Lane::Key1, 30, InputKind::Release),
            event(Lane::Key8, 10, InputKind::Press),
            event(Lane::Key1, 20, InputKind::Press),
            event(Lane::Key2, 20, InputKind::Release),
        ];

        normalize_battle_replay_for_key_mode(&mut replay, KeyMode::K7).unwrap();

        assert_eq!(replay.events.len(), 3);
        assert!(replay.events.iter().all(|event| event.lane != Lane::Key8));
        assert_eq!(
            replay.events.iter().map(|event| event.time.0).collect::<Vec<_>>(),
            vec![20, 20, 30]
        );
        assert_eq!(replay.events[0].lane, Lane::Key1);
        assert_eq!(replay.events[1].lane, Lane::Key2);
    }

    #[test]
    fn play_session_options_carry_battle_legacy_seed_scheme() {
        let replay =
            ReplayFile::new([1; 32], 1, None, ArrangeOption::Random, Some(42), None, Vec::new())
                .with_seed_scheme(crate::storage::replay::SEED_SCHEME_LEGACY_SHARED_V3);
        let target = BattleTarget {
            provider: "local".to_string(),
            score_id: "1".to_string(),
            player_id: "self".to_string(),
            player_name: "SELF".to_string(),
            rank: 0,
            ex_score: 0,
            gauge: None,
            playback: BattleTargetPlayback::Replay(Box::new(replay)),
        };

        let options = play_session_options_from_start(
            &AppConfig::default(),
            PlayStartOptions { battle_target: Some(target), ..Default::default() },
        );

        assert!(options.battle_opponent.expect("battle opponent").legacy_arrange_seed);
    }

    #[test]
    fn play_session_options_use_audio_sample_rate() {
        let mut app_config = AppConfig::default();
        app_config.audio.sample_rate = 96_000;

        let options = play_session_options_from_start(
            &app_config,
            PlayStartOptions {
                autoplay: true,
                random_trainer_seed: Some(322),
                ..Default::default()
            },
        );

        assert!(options.autoplay);
        assert_eq!(options.sample_rate, 96_000);
        assert_eq!(options.random_trainer_seed, Some(322));
        assert_eq!(options.s_random_scheme, SRandomScheme::Lm120HzV1);
        assert!(options.replay_player.is_none());
    }

    #[test]
    fn apply_queued_replay_carries_legacy_s_random_scheme() {
        let mut replay = crate::storage::replay::ReplayFile::new(
            [1; 32],
            1,
            Some(42),
            ArrangeOption::SRandom,
            Some(42),
            None,
            Vec::new(),
        );
        replay.version = 4;
        replay.s_random_scheme.clear();
        let queued = crate::storage::replay::QueuedCourseReplay {
            position: 0,
            chart_id: 1,
            chart_sha256: [1; 32],
            replay,
        };
        let mut options = PlayStartOptions {
            key_mode_conversion: KeyModeConversionConfig::SevenToNine,
            seven_to_nine_rule_mode: SevenToNineRuleMode::Keys7,
            ..Default::default()
        };

        apply_queued_replay(&mut options, &queued).unwrap();

        assert_eq!(options.arrange, ArrangeOption::SRandom);
        assert_eq!(options.s_random_scheme, SRandomScheme::Legacy40MsV1);
        assert_eq!(options.s_random_scheme_2p, Some(SRandomScheme::Legacy40MsV1));
        assert!(options.replay_player.is_some());
        assert_eq!(options.key_mode_conversion, KeyModeConversionConfig::SevenToNine);
        assert_eq!(options.seven_to_nine_rule_mode, SevenToNineRuleMode::Keys7);
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
    fn course_speed_constraint_reaches_session_options() {
        let app_config = AppConfig::default();
        let mut options = PlayStartOptions::default();
        let mut constraints = default_constraints();
        constraints.speed = CourseSpeedConstraint::NoSpeed;

        apply_course_constraints(&mut options, &constraints);
        assert_eq!(options.speed_constraint, CourseSpeedConstraint::NoSpeed);

        let session = play_session_options_from_start(&app_config, options);
        assert_eq!(session.speed_constraint, CourseSpeedConstraint::NoSpeed);
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
