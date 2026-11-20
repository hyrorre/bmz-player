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

#[test]
fn ratio_changed_hispeed_survives_profile_save_and_next_session() {
    for base in [BaseHispeedConfig::Classic, BaseHispeedConfig::Lr2] {
        let mut profile = ratio_profile(base, 3.0, 0.1);
        assert_eq!(profile.lane.classic_hispeed_step, 0.25);
        let mut session = ratio_session(&profile, 150.0, HsFixOption::StartBpm);
        press_hispeed(&mut session, &profile, HispeedChange::Up);
        let display = crate::screens::play_snapshot::display_hispeed_for_session(&session);
        assert_eq!(display, 3.3, "base={base:?}");

        apply_lane_state_to_profile(
            &mut profile,
            Some(display),
            Some(active_lane_state_for_session(&session)),
        );
        assert_eq!(profile.lane.hispeed, 3.3, "base={base:?}");
        assert_eq!(profile.lane.base_hispeed, base);

        let next = ratio_session(&profile, 150.0, HsFixOption::StartBpm);
        assert_eq!(
            crate::screens::play_snapshot::display_hispeed_for_session(&next),
            3.3,
            "base={base:?}"
        );
        assert_eq!(next.hispeed_change_base, 3.3, "base={base:?}");
    }
}

#[test]
fn fixed_change_type_still_rounds_saved_classic_hispeed_to_quarter_steps() {
    let mut profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 0.1);
    profile.lane.hispeed_change_type = HispeedChangeTypeConfig::Fixed;
    let session = ratio_session(&profile, 150.0, HsFixOption::Off);

    apply_lane_state_to_profile(
        &mut profile,
        Some(3.3),
        Some(active_lane_state_for_session(&session)),
    );
    assert_eq!(profile.lane.hispeed, 3.25);
}

/// 初期BPM150、1秒後からBPM300。ノーツはBPM300の区間に置くので、MAIN BPMも300になる。
fn bpm_change_chart(key_mode: KeyMode) -> bmz_chart::model::PlayableChart {
    use bmz_chart::model::{NoteEvent, NoteKind};
    let mut chart = app_test_chart();
    chart.metadata.key_mode = key_mode;
    chart.metadata.initial_bpm = 150.0;
    chart.timing_events.push(bmz_chart::model::TimingEvent {
        tick: bmz_core::time::ChartTick(48),
        time: TimeUs(1_000_000),
        kind: bmz_chart::model::TimingEventKind::BpmChange { bpm: 300.0 },
    });
    for (index, time) in [2_000_000, 2_500_000, 3_000_000].into_iter().enumerate() {
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: bmz_core::ids::NoteId(index as u32 + 1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: bmz_core::time::ChartTick(96 + index as u64 * 24),
            time: TimeUs(time),
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        });
    }
    chart.total_notes = 3;
    chart
}

/// 譜面の準備がPlay画面に入る前に済んだか、入った後に済んだか。
#[derive(Debug, Clone, Copy)]
enum PreparedTiming {
    BeforeEntry,
    AfterEntry,
}

struct PendingComparison {
    /// 補正前にplaceholderで使っていた比率の基準。
    placeholder_base: f32,
    pending_before_actions: f32,
    session_before_actions: f32,
}

/// 本番と同じ `PendingPlayStart::rebase_lane_to_prepared_chart` を通したロード中の状態が、
/// 実sessionへ同じ操作を再適用した結果と一致することを確かめる。
fn assert_pending_matches_session(
    profile: &ProfileConfig,
    chart_key_mode: KeyMode,
    play_config_key_mode: KeyMode,
    hs_fix: HsFixOption,
    timing: PreparedTiming,
) -> PendingComparison {
    let label = format!("{hs_fix:?} {timing:?} {chart_key_mode:?}/{play_config_key_mode:?}");
    let chart = bpm_change_chart(chart_key_mode);
    let start_options = PlayStartOptions { hs_fix, ..Default::default() };
    let app_config = AppConfig::default();
    // preloadと同じく、実sessionには設定対象キーモードを明示して渡す。
    let mut session_options = crate::screens::play_start::play_session_options_from_start(
        &app_config,
        start_options.clone(),
    );
    session_options.play_config_key_mode = Some(play_config_key_mode);

    let mut snapshot = bmz_render::snapshot::RenderSnapshot {
        now_bpm: 150.0,
        main_bpm: 150.0,
        min_bpm: 150.0,
        max_bpm: 150.0,
        ..Default::default()
    };
    crate::screens::play_session::apply_placeholder_session_visuals(
        &mut snapshot,
        profile,
        play_config_key_mode,
        &session_options,
    );
    let cache = crate::screens::play_snapshot::PlayRenderSnapshotCache::from_chart(&chart);
    if matches!(timing, PreparedTiming::BeforeEntry) {
        crate::screens::play_snapshot::apply_prepared_chart_to_render_snapshot(
            &mut snapshot,
            &chart,
            &cache,
            false,
        );
    }
    let mut pending = PendingPlayStart::from_snapshot(
        1,
        start_options,
        &snapshot,
        profile,
        play_config_key_mode,
        play_config_key_mode,
        Default::default(),
    );
    let placeholder_base = pending.lane.hispeed_change_base;
    let actions = [PlayLaneAction::Hispeed(HispeedChange::Up)];
    let push_actions =
        |pending: &mut PendingPlayStart, snapshot: &mut bmz_render::snapshot::RenderSnapshot| {
            for action in actions {
                assert!(apply_pending_play_lane_action_to_state(
                    &mut pending.lane,
                    action,
                    profile,
                    snapshot.now_bpm,
                    false,
                ));
                pending.lane_actions.push(action);
                pending.lane.apply_to_snapshot(snapshot);
            }
        };
    let pending_before_actions;
    match timing {
        PreparedTiming::BeforeEntry => {
            pending.rebase_lane_to_prepared_chart(
                &chart,
                &mut snapshot,
                profile,
                &app_config,
                false,
            );
            pending_before_actions = pending.lane.display_hispeed();
            push_actions(&mut pending, &mut snapshot);
        }
        PreparedTiming::AfterEntry => {
            push_actions(&mut pending, &mut snapshot);
            crate::screens::play_snapshot::apply_prepared_chart_to_render_snapshot(
                &mut snapshot,
                &chart,
                &cache,
                false,
            );
            pending.rebase_lane_to_prepared_chart(
                &chart,
                &mut snapshot,
                profile,
                &app_config,
                false,
            );
            let mut without_actions = PendingPlayStart::from_snapshot(
                1,
                pending.options.clone(),
                &snapshot,
                profile,
                play_config_key_mode,
                play_config_key_mode,
                Default::default(),
            );
            without_actions.rebase_lane_to_prepared_chart(
                &chart,
                &mut snapshot.clone(),
                profile,
                &app_config,
                false,
            );
            pending_before_actions = without_actions.lane.display_hispeed();
        }
    }
    assert!(pending.prepared_chart_applied, "{label}");

    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        profile,
        session_options,
    );
    let session_before_actions =
        crate::screens::play_snapshot::display_hispeed_for_session(&session);
    assert_eq!(pending_before_actions, session_before_actions, "{label}");
    let mut lane_target = PlayLaneTarget::Lift;
    replay_pending_play_lane_actions(&mut session, &mut lane_target, &actions, profile, false);

    assert_eq!(pending.lane.hispeed_change_base, session.hispeed_change_base, "{label}");
    assert!(
        (pending.lane.hispeed - session.hispeed).abs() < 0.000_01,
        "{label} pending={} session={}",
        pending.lane.hispeed,
        session.hispeed
    );
    assert_eq!(
        pending.lane.display_hispeed(),
        crate::screens::play_snapshot::display_hispeed_for_session(&session),
        "{label}"
    );
    assert_eq!(snapshot.hispeed_display, Some(pending.lane.display_hispeed()), "{label}");
    PendingComparison { placeholder_base, pending_before_actions, session_before_actions }
}

const TIMINGS: [PreparedTiming; 2] = [PreparedTiming::BeforeEntry, PreparedTiming::AfterEntry];

#[test]
fn pending_floating_ratio_change_matches_session_after_chart_is_prepared() {
    let mut profile = ratio_profile(BaseHispeedConfig::Classic, 3.0, 1.0);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    for timing in TIMINGS {
        for hs_fix in
            [HsFixOption::StartBpm, HsFixOption::MaxBpm, HsFixOption::MinBpm, HsFixOption::MainBpm]
        {
            let result =
                assert_pending_matches_session(&profile, KeyMode::K7, KeyMode::K7, hs_fix, timing);
            if hs_fix == HsFixOption::MaxBpm && matches!(timing, PreparedTiming::AfterEntry) {
                // placeholderは初期BPM150、実sessionは最大BPM300から求めるため基準が異なる。
                assert!(
                    (result.placeholder_base - 3.2).abs() < 0.01,
                    "placeholder={}",
                    result.placeholder_base
                );
            }
        }
    }
}

#[test]
fn pending_lr2_ratio_change_matches_session_after_chart_is_prepared() {
    let profile = ratio_profile(BaseHispeedConfig::Lr2, 3.0, 1.0);
    for timing in TIMINGS {
        for hs_fix in
            [HsFixOption::Off, HsFixOption::MaxBpm, HsFixOption::MinBpm, HsFixOption::MainBpm]
        {
            let result =
                assert_pending_matches_session(&profile, KeyMode::K7, KeyMode::K7, hs_fix, timing);
            // 操作前の表示HSは、どの基準BPMでも保存値の3.00のまま。
            assert_eq!(result.pending_before_actions, 3.0, "{hs_fix:?} {timing:?}");
            assert_eq!(result.session_before_actions, 3.0, "{hs_fix:?} {timing:?}");
        }
    }
}

#[test]
fn pending_lr2_main_bpm_uses_chart_main_bpm_for_display_scale() {
    let chart = bpm_change_chart(KeyMode::K7);
    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        chart.metadata.initial_bpm,
        &chart.timing_events,
    );
    assert_eq!(crate::screens::play_session::main_bpm_for_chart(&chart, &timing_map, false), 300.0);
    let profile = ratio_profile(BaseHispeedConfig::Lr2, 3.0, 1.0);
    for timing in TIMINGS {
        let _ = assert_pending_matches_session(
            &profile,
            KeyMode::K7,
            KeyMode::K7,
            HsFixOption::MainBpm,
            timing,
        );
    }
}

/// G-BATTLEでは譜面が14K / 10Kに展開されても、設定は元の7K / 5Kを使う。
#[test]
fn pending_uses_play_config_key_mode_for_expanded_battle_charts() {
    for (single, double) in [(KeyMode::K7, KeyMode::K14), (KeyMode::K5, KeyMode::K10)] {
        for change_type in HispeedChangeTypeConfig::ORDER {
            let mut profile = ratio_profile(BaseHispeedConfig::Classic, 1.0, 1.0);
            profile.lane.hispeed_change_type = change_type;
            profile.activate_play_mode(double);
            profile.lane.hispeed = 1.0;
            profile.activate_play_mode(single);
            profile.lane.hispeed = 3.0;
            profile.sync_active_play_mode();
            assert_eq!(profile.play_mode_config(double).hispeed, 1.0);
            assert_eq!(profile.play_mode_config(single).hispeed, 3.0);

            for timing in TIMINGS {
                let result = assert_pending_matches_session(
                    &profile,
                    double,
                    single,
                    HsFixOption::Off,
                    timing,
                );
                assert_eq!(result.pending_before_actions, 3.0, "{single:?} {change_type:?}");
            }
        }
    }
}
