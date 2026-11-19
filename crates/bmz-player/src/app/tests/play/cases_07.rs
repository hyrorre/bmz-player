use super::*;

// HS変更方式 Ratio: プレイ開始時のHS × 比率でE1+鍵盤の変更量を決める。

fn ratio_profile(base: BaseHispeedConfig, hispeed: f32, ratio: f32) -> ProfileConfig {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = base;
    profile.lane.floating_policy = FloatingPolicyConfig::Disabled;
    profile.lane.hispeed = hispeed;
    profile.lane.hispeed_change_type = HispeedChangeTypeConfig::Ratio;
    profile.lane.hispeed_change_ratio = ratio;
    profile
}

fn ratio_session(
    profile: &ProfileConfig,
    initial_bpm: f64,
    hs_fix: HsFixOption,
) -> bmz_gameplay::session::GameSession {
    let mut chart = app_test_chart();
    chart.metadata.initial_bpm = initial_bpm;
    crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix,
            ..crate::screens::play_session::PlaySessionOptions::default()
        },
    )
}

fn press_hispeed(
    session: &mut bmz_gameplay::session::GameSession,
    profile: &ProfileConfig,
    change: HispeedChange,
) {
    let steps =
        PlayHispeedSteps::for_profile(profile, session.hispeed_mode, session.hispeed_change_base);
    let mut lane_target = PlayLaneTarget::Lift;
    assert!(apply_play_lane_action_to_session(
        session,
        &mut lane_target,
        PlayLaneAction::Hispeed(change),
        false,
        steps,
    ));
}

#[test]
fn fixed_change_type_keeps_mode_steps_for_keys_and_covers() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.classic_hispeed_step = 0.3;
    profile.lane.floating_hispeed_step = 0.7;

    assert_eq!(profile.lane.hispeed_change_type, HispeedChangeTypeConfig::Fixed);
    assert_eq!(
        PlayHispeedSteps::for_profile(&profile, HispeedMode::Classic, 3.0),
        PlayHispeedSteps { key: 0.3, cover: 0.3 }
    );
    assert_eq!(
        PlayHispeedSteps::for_profile(&profile, HispeedMode::Floating, 3.0),
        PlayHispeedSteps { key: 0.7, cover: 0.7 }
    );
}

#[test]
fn ratio_change_type_scales_only_key_step_by_base_hispeed() {
    let mut profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 0.5);
    profile.lane.floating_hispeed_step = 0.25;

    assert_eq!(
        PlayHispeedSteps::for_profile(&profile, HispeedMode::Classic, 3.0),
        PlayHispeedSteps { key: 1.5, cover: 0.25 }
    );
    assert_eq!(
        PlayHispeedSteps::for_profile(&profile, HispeedMode::Floating, 3.0),
        PlayHispeedSteps { key: 1.5, cover: 0.25 }
    );
}

#[test]
fn ratio_step_is_quantized_and_clamped() {
    assert_eq!(ratio_hispeed_step(2.33, 0.25), 0.58);
    assert_eq!(ratio_hispeed_step(0.5, 0.01), 0.01);
    assert_eq!(ratio_hispeed_step(10.0, 10.0), 20.0);
    // 範囲外・非有限の比率は保存値の正規化と同じ規則で扱う。
    assert_eq!(ratio_hispeed_step(3.0, 0.0), 0.03);
    assert_eq!(ratio_hispeed_step(3.0, f32::NAN), 0.75);
}

#[test]
fn ratio_one_doubles_triples_base_hispeed() {
    let profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 1.0);
    let mut session = ratio_session(&profile, 150.0, HsFixOption::Off);
    assert_eq!(session.hispeed, 3.0);
    assert_eq!(session.hispeed_change_base, 3.0);

    press_hispeed(&mut session, &profile, HispeedChange::Up);
    assert_eq!(session.hispeed, 6.0);
    press_hispeed(&mut session, &profile, HispeedChange::Up);
    assert_eq!(session.hispeed, 9.0);
    // 基準はプレイ中に変わらない。
    assert_eq!(session.hispeed_change_base, 3.0);
    press_hispeed(&mut session, &profile, HispeedChange::Down);
    assert_eq!(session.hispeed, 6.0);
}

#[test]
fn ratio_change_stops_at_hispeed_limits() {
    let profile = ratio_profile(BaseHispeedConfig::Classic, 8.0, 1.0);
    let mut session = ratio_session(&profile, 150.0, HsFixOption::Off);

    press_hispeed(&mut session, &profile, HispeedChange::Up);
    press_hispeed(&mut session, &profile, HispeedChange::Up);
    assert_eq!(session.hispeed, crate::config::play::HISPEED_MAX);

    let profile = ratio_profile(BaseHispeedConfig::Classic, 1.0, 2.0);
    let mut session = ratio_session(&profile, 150.0, HsFixOption::Off);
    press_hispeed(&mut session, &profile, HispeedChange::Down);
    assert_eq!(session.hispeed, crate::config::play::HISPEED_MIN);
}

#[test]
fn ratio_change_ignores_hsfix_for_classic() {
    let profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 1.0);
    let mut session = ratio_session(&profile, 300.0, HsFixOption::StartBpm);
    assert_eq!(session.hispeed_change_base, 3.0);

    press_hispeed(&mut session, &profile, HispeedChange::Up);
    assert_eq!(session.hispeed, 6.0);
}

#[test]
fn ratio_change_uses_lr2_hsfix_display_hispeed() {
    let profile = ratio_profile(BaseHispeedConfig::Lr2, 3.0, 1.0);
    let mut session = ratio_session(&profile, 300.0, HsFixOption::StartBpm);
    // BPM300の譜面でLR2 HS 3.00は実効倍率1.50。基準は表示上の3.00にする。
    assert!((session.hispeed - 1.5).abs() < 0.000_01, "hispeed={}", session.hispeed);
    assert_eq!(session.hispeed_change_base, 3.0);

    press_hispeed(&mut session, &profile, HispeedChange::Up);
    assert!((session.hispeed - 3.0).abs() < 0.000_01, "hispeed={}", session.hispeed);
    assert_eq!(crate::screens::play_snapshot::display_hispeed_for_session(&session), 6.0);
}

#[test]
fn ratio_change_applies_to_floating_hispeed() {
    let mut profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 0.5);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    let mut session = ratio_session(&profile, 150.0, HsFixOption::Off);
    assert_eq!(session.hispeed_mode, HispeedMode::Floating);
    let base = session.hispeed_change_base;
    assert_eq!(base, session.hispeed);

    press_hispeed(&mut session, &profile, HispeedChange::Up);
    let expected = crate::config::play::quantize_hispeed(base + ratio_hispeed_step(base, 0.5));
    assert_eq!(session.hispeed, expected);
}

#[test]
fn ratio_change_keeps_fixed_step_for_cover_fallback() {
    let mut profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 1.0);
    profile.lane.classic_hispeed_step = 0.25;
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.lift_enabled = false;
    profile.lane.hispeed_auto_adjust = false;
    let mut session = ratio_session(&profile, 150.0, HsFixOption::Off);
    let steps =
        PlayHispeedSteps::for_profile(&profile, session.hispeed_mode, session.hispeed_change_base);
    let mut lane_target = PlayLaneTarget::Lift;

    assert!(apply_play_lane_action_to_session(
        &mut session,
        &mut lane_target,
        PlayLaneAction::LaneCoverDelta(-0.01),
        false,
        steps,
    ));
    assert_eq!(session.hispeed, 2.75);
}

#[test]
fn pending_ratio_change_uses_pending_base_and_replays_on_session() {
    let profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 1.0);
    let mut lane = PendingPlayLaneState {
        hispeed_change_base: 3.0,
        hispeed: 3.0,
        hispeed_mode: HispeedMode::Classic,
        base_hispeed_mode: HispeedMode::Classic,
        floating_policy: FloatingPolicy::Disabled,
        normal_hispeed_level: 18,
        target_green_number: 300,
        lane_cover: 0.0,
        lift: 0.0,
        hidden_cover: 0.0,
        sudden_enabled: false,
        lift_enabled: false,
        hidden_enabled: false,
        lane_cover_visible: false,
        lane_target: PlayLaneTarget::Lift,
        lane_cover_changing: false,
        hsfix_base_bpm: 150.0,
        lr2_hispeed: false,
        classic_hsfix_scale: 1.0,
        hispeed_auto_adjust: false,
        playback_rate_percent: 100,
    };
    let actions = [PlayLaneAction::Hispeed(HispeedChange::Up); 2];
    for action in actions {
        assert!(
            apply_pending_play_lane_action_to_state(&mut lane, action, &profile, 150.0, false,)
        );
    }
    assert_eq!(lane.hispeed, 9.0);
    assert_eq!(lane.hispeed_change_base, 3.0);

    let mut session = ratio_session(&profile, 150.0, HsFixOption::Off);
    let mut lane_target = PlayLaneTarget::Lift;
    replay_pending_play_lane_actions(&mut session, &mut lane_target, &actions, &profile, false);
    assert_eq!(session.hispeed, 9.0);
}
