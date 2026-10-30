#[derive(Debug, Clone, Copy)]
pub(in crate::app) struct ActiveLaneState {
    pub(in crate::app) lane_cover: f32,
    pub(in crate::app) lift: f32,
    pub(in crate::app) hidden_cover: f32,
    pub(in crate::app) sudden_enabled: bool,
    pub(in crate::app) lift_enabled: bool,
    pub(in crate::app) hidden_enabled: bool,
    pub(in crate::app) hispeed_mode: HispeedMode,
    pub(in crate::app) base_hispeed_mode: HispeedMode,
    pub(in crate::app) floating_policy: FloatingPolicy,
    pub(in crate::app) normal_hispeed_level: u8,
    pub(in crate::app) target_green_number: u32,
}

pub(in crate::app) fn profile_lane_settings_changed(
    before: &LaneViewConfig,
    after: &LaneViewConfig,
) -> bool {
    before.hispeed != after.hispeed
        || before.base_hispeed != after.base_hispeed
        || before.floating_policy != after.floating_policy
        || before.normal_hispeed_level != after.normal_hispeed_level
        || before.sudden != after.sudden
        || before.lift != after.lift
        || before.lift_enabled != after.lift_enabled
        || before.hispeed_auto_adjust != after.hispeed_auto_adjust
        || before.hidden != after.hidden
        || before.target_green_number != after.target_green_number
        || before.constant_enabled != after.constant_enabled
        || before.constant_fade_ms != after.constant_fade_ms
}

pub(in crate::app) fn apply_profile_lane_settings_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    before: &LaneViewConfig,
    before_lane_effect: LaneEffectConfig,
    profile: &LaneViewConfig,
    lane_effect: LaneEffectConfig,
    speed_locked: bool,
    practice_mode: bool,
) -> bool {
    if !profile_lane_settings_changed(before, profile) && before_lane_effect == lane_effect {
        return false;
    }
    if speed_locked {
        return false;
    }

    let config_changed = before.base_hispeed != profile.base_hispeed
        || before.floating_policy != profile.floating_policy;
    let normal_level_changed = before.normal_hispeed_level != profile.normal_hispeed_level;
    let hispeed_changed = before.hispeed != profile.hispeed;
    let sudden_changed = before.sudden != profile.sudden;
    let sudden_enabled_changed =
        before_lane_effect.sudden_enabled() != lane_effect.sudden_enabled();
    let lift_changed = before.lift != profile.lift;
    let lift_enabled_changed = before.lift_enabled != profile.lift_enabled;
    let auto_adjust_changed = before.hispeed_auto_adjust != profile.hispeed_auto_adjust;
    let hidden_changed = before.hidden != profile.hidden;
    let hidden_enabled_changed =
        before_lane_effect.hidden_enabled() != lane_effect.hidden_enabled();
    let target_green_changed = before.target_green_number != profile.target_green_number;
    let constant_changed = before.constant_enabled != profile.constant_enabled;
    let constant_fade_changed = before.constant_fade_ms != profile.constant_fade_ms;
    let cover_changed =
        sudden_changed || sudden_enabled_changed || lift_changed || lift_enabled_changed;

    if cover_changed {
        session.lanecover_enabled = lane_effect.sudden_enabled();
        if sudden_enabled_changed {
            session.lane_cover_visible = session.lanecover_enabled;
        }
        session.lift_enabled = profile.lift_enabled;
        session.lift = if profile.lift_enabled {
            crate::config::play::lane_unit_to_f32(profile.lift)
        } else {
            0.0
        };
        session.lane_cover = crate::config::play::clamp_lane_cover_for_lift(
            crate::config::play::lane_unit_to_f32(profile.sudden),
            session.lift,
        );
    }
    if hidden_changed || hidden_enabled_changed {
        session.hidden_enabled = lane_effect.hidden_enabled();
        session.hidden_cover = if session.hidden_enabled {
            crate::config::play::lane_unit_to_f32(profile.hidden)
        } else {
            0.0
        };
    }
    if auto_adjust_changed {
        session.hispeed_auto_adjust = profile.hispeed_auto_adjust;
    }
    // CONSTANT は実効 SPEED を変えるため、緑数字・HS を求める前に反映する。
    if constant_changed {
        let display_hispeed = crate::screens::play_snapshot::display_hispeed_for_session(session);
        session.constant_enabled = profile.constant_enabled && !practice_mode;
        if session.hsfix_index == crate::skin_extension::hsfix_index(HsFixOption::MainBpm) as i32 {
            session.hsfix_base_bpm = crate::screens::play_session::main_bpm_for_chart(
                &session.chart,
                &session.timing_map,
                session.constant_enabled,
            );
            if session.lr2_hispeed && session.hispeed_mode == HispeedMode::Classic {
                session.hispeed = crate::config::play::classic_effective_hispeed(
                    display_hispeed,
                    crate::screens::play_snapshot::classic_hsfix_scale_for_session(session),
                );
            }
        }
    }

    let now = session.audio_clock.now();
    let lr2_hispeed = profile.base_hispeed == BaseHispeedConfig::Lr2;
    if config_changed && session.lr2_hispeed != lr2_hispeed {
        // CLASSICとLR2の切替では、表示中のHS値を保ったまま実効倍率を求め直す。
        let display_hispeed = crate::screens::play_snapshot::display_hispeed_for_session(session);
        session.lr2_hispeed = lr2_hispeed;
        if session.hispeed_mode == HispeedMode::Classic {
            session.hispeed = crate::config::play::classic_effective_hispeed(
                display_hispeed,
                crate::screens::play_snapshot::classic_hsfix_scale_for_session(session),
            );
        }
    }
    if config_changed {
        session.base_hispeed_mode = base_hispeed_mode_from_config(profile.base_hispeed);
        session.floating_policy = floating_policy_from_config(profile.floating_policy);
        let previous_mode = session.hispeed_mode;
        session.hispeed_mode = match session.floating_policy {
            FloatingPolicy::Locked => HispeedMode::Floating,
            FloatingPolicy::Disabled => session.base_hispeed_mode,
            FloatingPolicy::Toggle if previous_mode == HispeedMode::Floating => {
                HispeedMode::Floating
            }
            FloatingPolicy::Toggle => session.base_hispeed_mode,
        };
        if session.hispeed_mode == HispeedMode::Floating
            && previous_mode != HispeedMode::Floating
            && !target_green_changed
            && !hispeed_changed
        {
            session.target_green_number = current_green_number(session, now);
        } else if session.hispeed_mode == HispeedMode::Normal
            && previous_mode != HispeedMode::Normal
        {
            session.normal_hispeed_level =
                crate::config::play::normal_hispeed_level_for_green_number(
                    current_full_lane_green_number(session, now),
                );
        }
    }
    if normal_level_changed {
        session.normal_hispeed_level =
            crate::config::play::normalize_normal_hispeed_level(profile.normal_hispeed_level);
    }

    if target_green_changed {
        session.target_green_number = profile.target_green_number.max(1);
    }
    if constant_fade_changed {
        session.constant_fade_ms = profile.constant_fade_ms.clamp(
            crate::config::play::CONSTANT_FADE_MIN_MS,
            crate::config::play::CONSTANT_FADE_MAX_MS,
        );
    }

    let direct_hispeed_change = hispeed_changed;
    if direct_hispeed_change && session.hispeed_mode == HispeedMode::Classic {
        session.hispeed = crate::config::play::classic_effective_hispeed(
            profile.hispeed,
            crate::screens::play_snapshot::classic_hsfix_scale_for_session(session),
        );
    }

    if session.hispeed_mode == HispeedMode::Normal {
        // CONSTANT 切替で実効 SPEED が変わるため、段階の緑数字に合わせ直す。
        if normal_level_changed || config_changed || constant_changed {
            session.hispeed = hispeed_for_normal_level(session, session.normal_hispeed_level, now);
        }
    } else if session.hispeed_mode == HispeedMode::Floating {
        if direct_hispeed_change && !target_green_changed {
            // Floating中に設定UIで倍率を変更した場合は、現在の見た目を目標値にする。
            session.target_green_number = current_green_number(session, now);
        } else if target_green_changed
            || cover_changed
            || auto_adjust_changed
            || (constant_changed && !direct_hispeed_change)
            || (config_changed && !direct_hispeed_change)
        {
            session.hispeed =
                hispeed_for_green_number(session, active_lane_cover_for_hispeed(session), now);
        }
    }

    true
}

pub(in crate::app) fn lane_state_for_profile_save(
    speed_locked: bool,
    hispeed: Option<f32>,
    lane_state: Option<ActiveLaneState>,
) -> (Option<f32>, Option<ActiveLaneState>) {
    if speed_locked { (None, None) } else { (hispeed, lane_state) }
}

#[cfg(test)]
pub(in crate::app) fn active_lane_state_for_session(
    session: &bmz_gameplay::session::GameSession,
) -> ActiveLaneState {
    ActiveLaneState {
        lane_cover: session.lane_cover,
        lift: session.lift,
        hidden_cover: session.hidden_cover,
        sudden_enabled: session.lanecover_enabled,
        lift_enabled: session.lift_enabled,
        hidden_enabled: session.hidden_enabled,
        hispeed_mode: session.hispeed_mode,
        base_hispeed_mode: session.base_hispeed_mode,
        floating_policy: session.floating_policy,
        normal_hispeed_level: session.normal_hispeed_level,
        // 基準方式の現在表示は曲終了時に変動するため保存しない。target は
        // Floatingへの明示切替時にsession側で更新された値を引き継ぐ。
        target_green_number: session.target_green_number,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct CurrentPlayOptions {
    pub(in crate::app) arrange: ArrangeOption,
    pub(in crate::app) arrange_2p: ArrangeOption,
    pub(in crate::app) target: TargetOption,
    pub(in crate::app) gauge: GaugeTypeConfig,
    pub(in crate::app) gauge_auto_shift: GaugeAutoShiftConfig,
    pub(in crate::app) bottom_shiftable_gauge: BottomShiftableGaugeConfig,
    pub(in crate::app) double_option: DoubleOption,
    pub(in crate::app) hs_fix: HsFixOption,
    pub(in crate::app) session_mode: SessionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct SelectScoreContext {
    pub(in crate::app) rule_mode: RuleMode,
    pub(in crate::app) ln_mode_policy: LnPolicySetting,
}

impl SelectScoreContext {
    pub(in crate::app) fn from_profile(profile: &ProfileConfig) -> Self {
        Self::from_play(&profile.play)
    }

    pub(in crate::app) fn from_play(play: &PlayDefaultsConfig) -> Self {
        Self { rule_mode: play.rule_mode, ln_mode_policy: play.ln_mode_policy }
    }
}

pub(in crate::app) fn session_mode_from_profile(play: &PlayDefaultsConfig) -> SessionMode {
    play.session_mode.unwrap_or(if play.auto_play {
        SessionMode::Autoplay
    } else {
        SessionMode::Normal
    })
}

pub(in crate::app) fn normalize_session_mode_for_course(options: &mut PlayStartOptions) {
    options.session_mode = match options.session_mode {
        SessionMode::Autoplay | SessionMode::AutoplayBattle => SessionMode::Autoplay,
        SessionMode::Normal | SessionMode::Practice | SessionMode::GBattle => SessionMode::Normal,
    };
    options.autoplay = options.session_mode.primary_autoplay();
    options.replay_player = None;
    options.battle_target = None;
}

pub(in crate::app) fn select_play_options_from_profile(
    play: &PlayDefaultsConfig,
) -> CurrentPlayOptions {
    let (gauge, gauge_auto_shift) = if play.gauge == GaugeTypeConfig::AutoShift {
        (GaugeTypeConfig::ExHard, GaugeAutoShiftConfig::BestClear)
    } else {
        (play.gauge, play.gauge_auto_shift)
    };
    CurrentPlayOptions {
        arrange: arrange_option_from_profile(play.random),
        arrange_2p: arrange_option_from_profile(play.random2),
        target: target_option_from_profile(play.target),
        gauge,
        gauge_auto_shift,
        bottom_shiftable_gauge: play.bottom_shiftable_gauge,
        double_option: double_option_from_profile(play.double_option),
        hs_fix: hs_fix_option_from_profile(play.hs_fix),
        session_mode: session_mode_from_profile(play),
    }
}

/// `profile.toml` の保存済みスキン設定だけを現在のprofileへ戻す。
///
/// スキンUIのリセットで、同じファイルに保存されているプレイ・入力・UI設定まで
/// 巻き戻さないため、`ProfileConfig` 全体は差し替えない。
pub(in crate::app) fn replace_skin_config_from_loaded_profile(
    current: &mut ProfileConfig,
    loaded: ProfileConfig,
) {
    current.skin = loaded.skin;
}

pub(in crate::app) fn merge_changed_select_play_options_from_profile(
    mut current: CurrentPlayOptions,
    before: &PlayDefaultsConfig,
    after: &PlayDefaultsConfig,
) -> CurrentPlayOptions {
    let profile = select_play_options_from_profile(after);
    if before.random != after.random {
        current.arrange = profile.arrange;
    }
    if before.random2 != after.random2 {
        current.arrange_2p = profile.arrange_2p;
    }
    if before.target != after.target {
        current.target = profile.target;
    }
    if before.gauge != after.gauge || before.gauge_auto_shift != after.gauge_auto_shift {
        current.gauge = profile.gauge;
        current.gauge_auto_shift = profile.gauge_auto_shift;
    }
    if before.bottom_shiftable_gauge != after.bottom_shiftable_gauge {
        current.bottom_shiftable_gauge = profile.bottom_shiftable_gauge;
    }
    if before.double_option != after.double_option {
        current.double_option = profile.double_option;
    }
    if before.hs_fix != after.hs_fix {
        current.hs_fix = profile.hs_fix;
    }
    if before.session_mode != after.session_mode {
        current.session_mode = profile.session_mode;
    } else if before.auto_play != after.auto_play {
        current.session_mode =
            if after.auto_play { SessionMode::Autoplay } else { SessionMode::Normal };
    }
    current
}

pub(in crate::app) fn apply_current_play_options_to_profile(
    profile: &mut ProfileConfig,
    hispeed: Option<f32>,
    lane_state: Option<ActiveLaneState>,
    options: CurrentPlayOptions,
    updated_at: i64,
) {
    apply_lane_state_to_profile(profile, hispeed, lane_state);
    profile.play.random = random_config_from_arrange(options.arrange);
    profile.play.random2 = random_config_from_arrange(options.arrange_2p);
    profile.play.target = target_config_from_option(options.target);
    profile.play.gauge = options.gauge;
    profile.play.gauge_auto_shift = options.gauge_auto_shift;
    profile.play.bottom_shiftable_gauge = options.bottom_shiftable_gauge;
    profile.play.double_option = double_config_from_option(options.double_option);
    profile.play.hs_fix = hs_fix_config_from_option(options.hs_fix);
    profile.play.session_mode = Some(options.session_mode);
    profile.play.auto_play = options.session_mode.primary_autoplay();
    profile.updated_at = updated_at;
}

pub(in crate::app) fn apply_lane_state_to_profile(
    profile: &mut ProfileConfig,
    hispeed: Option<f32>,
    lane_state: Option<ActiveLaneState>,
) {
    let active_mode = lane_state.map(|state| state.hispeed_mode).unwrap_or(HispeedMode::Classic);
    let base_mode = lane_state
        .map(|state| state.base_hispeed_mode)
        .unwrap_or_else(|| base_hispeed_mode_from_config(profile.lane.base_hispeed));
    if let Some(hispeed) = hispeed
        && base_mode == HispeedMode::Classic
    {
        let step = match active_mode {
            HispeedMode::Floating => profile.lane.floating_hispeed_step,
            HispeedMode::Normal | HispeedMode::Classic => profile.lane.classic_hispeed_step,
        };
        profile.lane.hispeed = clamp_hispeed_for_profile(hispeed, active_mode, step);
    }
    if let Some(state) = lane_state {
        if state.sudden_enabled {
            profile.lane.sudden = crate::config::play::lane_f32_to_unit(state.lane_cover);
        }
        if state.lift_enabled {
            profile.lane.lift = crate::config::play::lane_f32_to_unit(state.lift);
        }
        if state.hidden_enabled {
            profile.lane.hidden = crate::config::play::lane_f32_to_unit(state.hidden_cover);
        }
        let base_hispeed = base_hispeed_config(state.base_hispeed_mode);
        // LR2はプレイ中の基準方式ではClassicとして扱うため、Classicで上書きしない。
        if !(base_hispeed == BaseHispeedConfig::Classic
            && profile.lane.base_hispeed == BaseHispeedConfig::Lr2)
        {
            profile.lane.base_hispeed = base_hispeed;
        }
        profile.lane.floating_policy = floating_policy_config(state.floating_policy);
        profile.lane.normal_hispeed_level =
            crate::config::play::normalize_normal_hispeed_level(state.normal_hispeed_level);
        profile.lane.target_green_number = state.target_green_number.max(1);
    }
}

pub(in crate::app) fn clamp_hispeed_for_profile(hispeed: f32, mode: HispeedMode, step: f32) -> f32 {
    let clamped = clamp_hispeed(hispeed);
    if mode == HispeedMode::Classic
        && (normalize_classic_hispeed_step(step) - 0.25).abs() < f32::EPSILON
    {
        clamp_hispeed((clamped * 4.0).round() / 4.0)
    } else {
        clamped
    }
}

pub(in crate::app) const fn base_hispeed_mode_from_config(mode: BaseHispeedConfig) -> HispeedMode {
    match mode {
        BaseHispeedConfig::Normal => HispeedMode::Normal,
        BaseHispeedConfig::Classic | BaseHispeedConfig::Lr2 => HispeedMode::Classic,
    }
}

pub(in crate::app) const fn base_hispeed_config(mode: HispeedMode) -> BaseHispeedConfig {
    match mode {
        HispeedMode::Normal => BaseHispeedConfig::Normal,
        HispeedMode::Classic | HispeedMode::Floating => BaseHispeedConfig::Classic,
    }
}

pub(in crate::app) const fn floating_policy_from_config(
    policy: FloatingPolicyConfig,
) -> FloatingPolicy {
    match policy {
        FloatingPolicyConfig::Disabled => FloatingPolicy::Disabled,
        FloatingPolicyConfig::Toggle => FloatingPolicy::Toggle,
        FloatingPolicyConfig::Locked => FloatingPolicy::Locked,
    }
}

pub(in crate::app) const fn floating_policy_config(policy: FloatingPolicy) -> FloatingPolicyConfig {
    match policy {
        FloatingPolicy::Disabled => FloatingPolicyConfig::Disabled,
        FloatingPolicy::Toggle => FloatingPolicyConfig::Toggle,
        FloatingPolicy::Locked => FloatingPolicyConfig::Locked,
    }
}

pub(in crate::app) fn update_pre_ready_play_snapshot_options_for_runtime(
    last_play_snapshot: &mut Option<RenderSnapshot>,
    runtime: &crate::gameplay_runtime::GameplayClient,
    applied_arrange: &AppliedArrange,
) {
    // A prepared session has not started gameplay, including during an extended READY.
    if let Some(session) = runtime.prepared_session() {
        update_pre_ready_play_snapshot_options_for_session(
            None,
            last_play_snapshot,
            session,
            applied_arrange,
        );
    }
}

pub(in crate::app) fn update_pre_ready_play_snapshot_options_for_session(
    ready_sound_started_at: Option<Instant>,
    last_play_snapshot: &mut Option<RenderSnapshot>,
    session: &bmz_gameplay::session::GameSession,
    applied_arrange: &AppliedArrange,
) {
    if ready_sound_started_at.is_some() {
        return;
    }
    let Some(snapshot) = last_play_snapshot else {
        return;
    };
    crate::screens::play_snapshot::update_render_snapshot_play_options(
        snapshot,
        session,
        snapshot.time,
    );
    apply_play_arrange_to_snapshot(snapshot, applied_arrange);
}

pub(in crate::app) fn update_play_exit_hold_started_at(
    started_at: &mut Option<Instant>,
    e1_held: bool,
    e2_held: bool,
    now: Instant,
) {
    if e1_held && e2_held {
        started_at.get_or_insert(now);
    } else {
        *started_at = None;
    }
}

pub(in crate::app) fn play_exit_hold_elapsed(
    started_at: Option<Instant>,
    now: Instant,
    duration: Duration,
) -> bool {
    started_at.is_some_and(|started_at| now.duration_since(started_at) >= duration)
}
use super::*;

pub(in crate::app) fn active_lane_state_for_observation(
    session: &crate::gameplay_runtime::PlaySessionObservation,
) -> ActiveLaneState {
    ActiveLaneState {
        lane_cover: session.lane_cover,
        lift: session.lift,
        hidden_cover: session.hidden_cover,
        sudden_enabled: session.lanecover_enabled,
        lift_enabled: session.lift_enabled,
        hidden_enabled: session.hidden_enabled,
        hispeed_mode: session.hispeed_mode,
        base_hispeed_mode: session.base_hispeed_mode,
        floating_policy: session.floating_policy,
        normal_hispeed_level: session.normal_hispeed_level,
        // 基準方式の現在表示は曲終了時に変動するため保存しない。target は
        // Floatingへの明示切替時にsession側で更新された値を引き継ぐ。
        target_green_number: session.target_green_number,
    }
}
