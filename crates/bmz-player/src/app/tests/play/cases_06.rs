//! CONSTANT と #SPEED の実効倍率が、描画・緑数字・HS 計算で一致することの回帰テスト。

use super::*;
use bmz_chart::model::{
    NoteEvent, NoteKind, ScrollEvent, SpeedEvent, TimingEvent, TimingEventKind,
};
use bmz_core::ids::NoteId;
use bmz_core::lane::Lane;
use bmz_core::time::ChartTick;

/// BPM 120。Key1 に 1s (1920 ticks) のノートを 1 つ置き、判定ラインから 4 拍の
/// 半分 (進捗 0.5) に見える譜面。
fn speed_chart(
    speed_events: Vec<SpeedEvent>,
    scroll_events: Vec<ScrollEvent>,
) -> bmz_chart::model::PlayableChart {
    let mut chart = app_test_chart();
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(1920),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.total_notes = 1;
    chart.end_time = TimeUs(3_000_000);
    chart.speed_events = speed_events;
    chart.scroll_events = scroll_events;
    chart
}

fn speed_2_chart() -> bmz_chart::model::PlayableChart {
    speed_chart(vec![SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 }], Vec::new())
}

fn lane_profile(
    base: BaseHispeedConfig,
    floating: FloatingPolicyConfig,
    constant: bool,
) -> ProfileConfig {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = base;
    profile.lane.floating_policy = floating;
    profile.lane.hispeed = 1.0;
    profile.lane.normal_hispeed_level = 18;
    profile.lane.target_green_number = 300;
    profile.lane.sudden = 0;
    profile.lane.lift = 0;
    profile.lane.lift_enabled = false;
    profile.lane.constant_enabled = constant;
    profile
}

fn session_at(
    chart: bmz_chart::model::PlayableChart,
    profile: &ProfileConfig,
    options: crate::screens::play_session::PlaySessionOptions,
    now_us: i64,
) -> bmz_gameplay::session::GameSession {
    let mut session = crate::screens::play_session::build_game_session(
        std::sync::Arc::new(chart),
        profile,
        options,
    );
    session.offsets.visual_offset_us = 0;
    let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(
        (now_us.max(0) as u64) * 48_000 / 1_000_000,
    ));
    session.audio_clock = bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);
    session
}

fn floating_session(
    chart: bmz_chart::model::PlayableChart,
    constant: bool,
) -> bmz_gameplay::session::GameSession {
    let profile = lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked, constant);
    session_at(chart, &profile, crate::screens::play_session::PlaySessionOptions::default(), 0)
}

fn note_y(snapshot: &bmz_render::snapshot::RenderSnapshot) -> f32 {
    snapshot.visible_notes[Lane::Key1.index()][0].y
}

fn assert_close(actual: f32, expected: f32, label: &str) {
    assert!((actual - expected).abs() < 1e-3, "{label}: expected {expected}, got {actual}");
}

/// 描画・緑数字・HS 初期化・HS 逆算・Normal 段階計算が、同じ実効倍率 `multiplier`
/// を使っていることを確かめる。BPM 120 / 緑数字 300 / カバーなしなら
/// HS = 4.0 / multiplier。
fn assert_paths_use_multiplier(
    chart: bmz_chart::model::PlayableChart,
    constant: bool,
    multiplier: f32,
    base_progress: f32,
) {
    let mut session = floating_session(chart, constant);
    let now = session.audio_clock.now();
    let expected_hs = 4.0 / multiplier;

    assert_close(
        crate::screens::play_snapshot::current_scroll_multiplier_for_session(&session, now),
        multiplier,
        "multiplier",
    );
    // A. セッション初期化 (Floating)。
    assert_close(session.hispeed, expected_hs, "initial floating hispeed");
    // B. プレイ中の HS 計算。
    assert_close(hispeed_for_green_number(&session, 0.0, now), expected_hs, "floating hispeed");
    assert_close(hispeed_for_normal_level(&session, 18, now), expected_hs, "normal hispeed");
    assert_eq!(current_green_number(&session, now), 300);
    assert_eq!(current_full_lane_green_number(&session, now), 300);

    // 描画と緑数字の表示時間。HS 1.0 で比較する。CONSTANT の表示境界は目標緑数字から
    // 決まるため、比較対象のノートが境界の外へ出ないよう最大値にする。
    session.hispeed = 1.0;
    session.target_green_number = TARGET_GREEN_NUMBER_MAX;
    let snapshot = crate::screens::play_snapshot::build_render_snapshot(&session, now, &[], None);
    assert_close(note_y(&snapshot), base_progress * multiplier, "note y");
    let expected_duration = (2000.0 / multiplier).round() as i32;
    assert_eq!(snapshot.note_display_duration_ms, expected_duration);
    // C. プレイオプション更新でも同じ表示時間になる。
    let mut updated = snapshot.clone();
    updated.note_display_duration_ms = 0;
    crate::screens::play_snapshot::update_render_snapshot_play_options(&mut updated, &session, now);
    assert_eq!(updated.note_display_duration_ms, expected_duration);
}

#[test]
fn constant_off_applies_speed_to_render_green_number_and_hispeed() {
    assert_paths_use_multiplier(speed_2_chart(), false, 2.0, 0.5);
}

#[test]
fn constant_on_ignores_speed_for_render_green_number_and_hispeed() {
    // SPEED 2.0 を無視し、緑数字 300 の Floating HS は理論値 4.0 になる。
    assert_paths_use_multiplier(speed_2_chart(), true, 1.0, 0.5);
}

#[test]
fn constant_disables_only_speed_when_scroll_is_also_used() {
    let chart = || {
        speed_chart(
            vec![SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 }],
            vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 0.5 }],
        )
    };
    // SCROLL 0.5 は区間積分で距離を縮め、SPEED は一律に掛かる。
    // CONSTANT OFF: 0.5 × 2.0、ON: SCROLL 0.5 のみ。
    assert_paths_use_multiplier(chart(), false, 1.0, 0.5);
    assert_paths_use_multiplier(chart(), true, 0.5, 0.5);
}

#[test]
fn practice_session_initial_hispeed_uses_disabled_constant() {
    let profile = lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked, true);
    let session = session_at(
        speed_2_chart(),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            session_mode: crate::select_options::SessionMode::Practice,
            ..crate::screens::play_session::PlaySessionOptions::default()
        },
        0,
    );
    assert!(!session.constant_enabled);
    // Practice は CONSTANT を無効にするので SPEED 2.0 を含めた HS 2.0 になる。
    assert_close(session.hispeed, 2.0, "practice floating hispeed");
}

fn toggle_constant(
    session: &mut bmz_gameplay::session::GameSession,
    profile: &mut ProfileConfig,
    constant: bool,
) {
    let before = profile.lane.clone();
    profile.lane.constant_enabled = constant;
    assert!(apply_profile_lane_settings_to_session(
        session,
        &before,
        profile.play.lane_effect,
        &profile.lane,
        profile.play.lane_effect,
        false,
        false,
    ));
    assert_eq!(session.constant_enabled, constant);
}

#[test]
fn toggling_constant_refreshes_floating_and_normal_hispeed() {
    for (base, floating, label) in [
        (BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked, "floating"),
        (BaseHispeedConfig::Normal, FloatingPolicyConfig::Disabled, "normal"),
    ] {
        let mut profile = lane_profile(base, floating, false);
        let mut session = session_at(
            speed_2_chart(),
            &profile,
            crate::screens::play_session::PlaySessionOptions::default(),
            0,
        );
        let now = session.audio_clock.now();
        for (constant, expected_hs) in [(false, 2.0), (true, 4.0), (false, 2.0)] {
            if session.constant_enabled != constant {
                toggle_constant(&mut session, &mut profile, constant);
            }
            assert_close(session.hispeed, expected_hs, label);
            // 緑数字は目標値 (段階 18 = 300) のまま。
            assert_eq!(current_green_number(&session, now), 300, "{label}");
            let snapshot =
                crate::screens::play_snapshot::build_render_snapshot(&session, now, &[], None);
            // HS × SPEED が一定なので、表示時間 (緑数字) も変わらない。
            assert_eq!(snapshot.note_display_duration_ms, 500, "{label}");
        }
    }
}

#[test]
fn toggling_constant_keeps_classic_hispeed_and_updates_green_number() {
    let mut profile =
        lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Disabled, false);
    profile.lane.hispeed = 1.0;
    let mut session = session_at(
        speed_2_chart(),
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
        0,
    );
    // CONSTANT の表示境界にノートが掛からないようにする。Classic の HS には影響しない。
    session.target_green_number = TARGET_GREEN_NUMBER_MAX;
    let now = session.audio_clock.now();
    for (constant, multiplier) in [(false, 2.0_f32), (true, 1.0), (false, 2.0)] {
        if session.constant_enabled != constant {
            toggle_constant(&mut session, &mut profile, constant);
        }
        assert_close(session.hispeed, 1.0, "classic hispeed");
        let duration = 2000.0_f32 / multiplier;
        assert_eq!(current_green_number(&session, now), (duration * 0.6).round() as u32);
        let snapshot =
            crate::screens::play_snapshot::build_render_snapshot(&session, now, &[], None);
        assert_eq!(snapshot.note_display_duration_ms, duration as i32);
        assert_close(note_y(&snapshot), 0.5 * multiplier, "classic note y");
    }
}

#[test]
fn constant_ignores_interpolated_speed_and_restores_it_when_disabled() {
    let chart = speed_chart(
        vec![
            SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 1.0 },
            SpeedEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: 3.0 },
        ],
        Vec::new(),
    );
    let profile = lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Disabled, false);
    let mut session = session_at(
        chart,
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
        500_000,
    );
    session.target_green_number = TARGET_GREEN_NUMBER_MAX;
    let now = TimeUs(500_000);
    for (constant, multiplier) in [(false, 1.5_f32), (true, 1.0), (false, 1.5)] {
        session.constant_enabled = constant;
        assert_close(
            crate::screens::play_snapshot::current_scroll_multiplier_for_session(&session, now),
            multiplier,
            "interpolated multiplier",
        );
        let snapshot =
            crate::screens::play_snapshot::build_render_snapshot(&session, now, &[], None);
        assert_eq!(snapshot.note_display_duration_ms, (2000.0 / multiplier).round() as i32);
        // ノートまで残り 0.5s = 進捗 0.25。
        assert_close(note_y(&snapshot), 0.25 * multiplier, "interpolated note y");
    }
}

#[test]
fn constant_and_time_based_speed_across_bpm_change_and_stop() {
    let mut chart = speed_chart(
        vec![
            SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: 1.0 },
            // 1s で BPM 240 + STOP 0.5s。その後 1920 ticks (0.5s) で 2.0s。
            SpeedEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: 3.0 },
        ],
        Vec::new(),
    );
    chart.timing_events = vec![
        TimingEvent {
            tick: ChartTick(1920),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::BpmChange { bpm: 240.0 },
        },
        TimingEvent {
            tick: ChartTick(1920),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::Stop { duration_us: 500_000 },
        },
    ];
    let profile = lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Disabled, false);
    let mut session = session_at(
        chart,
        &profile,
        crate::screens::play_session::PlaySessionOptions::default(),
        1_250_000,
    );
    // STOP 中の 1.25s。時間比 0.625 で 2.25 (tick 比 0.5 の 2.0 ではない)。
    let now = TimeUs(1_250_000);
    assert_close(
        crate::screens::play_snapshot::current_scroll_multiplier_for_session(&session, now),
        2.25,
        "time based speed",
    );
    session.constant_enabled = true;
    assert_close(
        crate::screens::play_snapshot::current_scroll_multiplier_for_session(&session, now),
        1.0,
        "constant speed",
    );
}

#[test]
fn charts_without_speed_keep_hispeed_for_constant_on_and_off() {
    use crate::select_options::HsFixOption;
    for hs_fix in [HsFixOption::Off, HsFixOption::StartBpm, HsFixOption::MaxBpm] {
        for (base, floating) in [
            (BaseHispeedConfig::Normal, FloatingPolicyConfig::Disabled),
            (BaseHispeedConfig::Classic, FloatingPolicyConfig::Disabled),
            (BaseHispeedConfig::Lr2, FloatingPolicyConfig::Disabled),
            (BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked),
        ] {
            let results = [false, true].map(|constant| {
                let mut profile = lane_profile(base, floating, constant);
                profile.lane.hispeed = 2.5;
                let session = session_at(
                    speed_chart(Vec::new(), Vec::new()),
                    &profile,
                    crate::screens::play_session::PlaySessionOptions {
                        hs_fix,
                        ..crate::screens::play_session::PlaySessionOptions::default()
                    },
                    0,
                );
                let now = session.audio_clock.now();
                let snapshot =
                    crate::screens::play_snapshot::build_render_snapshot(&session, now, &[], None);
                (
                    session.hispeed,
                    current_green_number(&session, now),
                    hispeed_for_green_number(&session, 0.0, now),
                    snapshot.note_display_duration_ms,
                )
            });
            assert_eq!(results[0], results[1], "{hs_fix:?} {base:?} {floating:?}");
        }
    }
}

#[test]
fn main_bpm_initial_hispeed_applies_scroll_and_effective_speed_once() {
    for (scroll, speed) in [(2.0, 1.0), (1.0, 2.0), (2.0, 2.0)] {
        for constant in [false, true] {
            for (base, floating) in [
                (BaseHispeedConfig::Normal, FloatingPolicyConfig::Disabled),
                (BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked),
            ] {
                let chart = speed_chart(
                    vec![SpeedEvent { tick: ChartTick(0), time: TimeUs(0), factor: speed }],
                    vec![ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: scroll }],
                );
                let profile = lane_profile(base, floating, constant);
                let mut session = crate::screens::play_session::build_game_session(
                    std::sync::Arc::new(chart),
                    &profile,
                    crate::screens::play_session::PlaySessionOptions {
                        hs_fix: HsFixOption::MainBpm,
                        ..Default::default()
                    },
                );
                session.offsets.visual_offset_us = 0;
                let multiplier = scroll * if constant { 1.0 } else { speed };
                let expected_hispeed = (4.0 / multiplier) as f32;
                assert_eq!(session.hsfix_base_bpm, 120.0 * multiplier);
                assert_close(session.hispeed, expected_hispeed, "initial MAIN hispeed");
                assert_close(
                    hispeed_for_normal_level(&session, 18, TimeUs(0)),
                    expected_hispeed,
                    "MAIN normal level before chart start",
                );
                reset_floating_hispeed_if_enabled(&mut session, false);
                assert_close(session.hispeed, expected_hispeed, "MAIN reset before chart start");
                assert_eq!(current_green_number(&session, TimeUs(0)), 300);
                assert_eq!(current_full_lane_green_number(&session, TimeUs(0)), 300);
                let snapshot = crate::screens::play_snapshot::build_render_snapshot(
                    &session,
                    TimeUs(0),
                    &[],
                    None,
                );
                assert_eq!(snapshot.note_display_duration_ms, 500);
            }
        }
    }
}

#[test]
fn main_bpm_cover_changes_distinguish_fixed_and_current_scroll() {
    for auto_adjust in [false, true] {
        let chart = speed_chart(
            Vec::new(),
            vec![
                ScrollEvent { tick: ChartTick(0), time: TimeUs(0), factor: 2.0 },
                ScrollEvent { tick: ChartTick(3840), time: TimeUs(2_000_000), factor: 3.0 },
            ],
        );
        let mut profile =
            lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked, false);
        profile.play.lane_effect = LaneEffectConfig::Sudden;
        profile.lane.hispeed_auto_adjust = auto_adjust;
        let mut session = crate::screens::play_session::build_game_session(
            std::sync::Arc::new(chart),
            &profile,
            crate::screens::play_session::PlaySessionOptions {
                hs_fix: HsFixOption::MainBpm,
                ..Default::default()
            },
        );
        assert_eq!(session.hsfix_base_bpm, 240.0);
        let mut lane_target = PlayLaneTarget::Sudden;
        apply_lane_cover_step_to_session(&mut session, &mut lane_target, -0.25, false);
        assert_close(session.hispeed, 1.5, "MAIN cover before chart start");
        assert_eq!(current_green_number(&session, TimeUs(0)), 300);

        // ノーツのある SCROLL 2 区間から、SCROLL 3 区間へ移動する。
        let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(96_000));
        session.audio_clock =
            bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);
        let now = session.audio_clock.now();
        assert_eq!(now, TimeUs(2_000_000));
        assert_close(
            hispeed_for_normal_level(&session, 18, now),
            4.0 / 3.0,
            "normal level uses current scroll after chart start",
        );
        reset_floating_hispeed_if_enabled(&mut session, false);
        assert_close(
            session.hispeed,
            if auto_adjust { 1.0 } else { 1.5 },
            "MAIN reset after scroll change",
        );
        apply_lane_cover_step_to_session(&mut session, &mut lane_target, -0.25, false);
        assert_close(
            session.hispeed,
            if auto_adjust { 2.0 / 3.0 } else { 1.0 },
            "MAIN cover after scroll change",
        );
    }
}

#[test]
fn main_bpm_constant_toggle_updates_basis_and_preserves_lr2_display_hispeed() {
    for (base, floating) in [
        (BaseHispeedConfig::Normal, FloatingPolicyConfig::Disabled),
        (BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked),
        (BaseHispeedConfig::Lr2, FloatingPolicyConfig::Disabled),
    ] {
        for running in [false, true] {
            let mut profile = lane_profile(base, floating, false);
            let mut session = crate::screens::play_session::build_game_session(
                std::sync::Arc::new(speed_2_chart()),
                &profile,
                crate::screens::play_session::PlaySessionOptions {
                    hs_fix: HsFixOption::MainBpm,
                    ..Default::default()
                },
            );
            if running {
                let frame = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                session.audio_clock =
                    bmz_audio::clock::AudioClock::with_position(48_000, 0, 0, frame, true);
            }
            for (constant, expected_bpm) in [(false, 240.0), (true, 120.0), (false, 240.0)] {
                if session.constant_enabled != constant {
                    toggle_constant(&mut session, &mut profile, constant);
                }
                assert_eq!(session.hsfix_base_bpm, expected_bpm);
                if base == BaseHispeedConfig::Lr2 {
                    assert_close(session.hispeed, (150.0 / expected_bpm) as f32, "LR2 MAIN");
                    assert_close(
                        crate::screens::play_snapshot::display_hispeed_for_session(&session),
                        1.0,
                        "LR2 display hispeed across CONSTANT toggle",
                    );
                } else {
                    assert_close(
                        session.hispeed,
                        if constant { 4.0 } else { 2.0 },
                        "MAIN CONSTANT hispeed",
                    );
                    assert_eq!(current_green_number(&session, TimeUs(0)), 300);
                }
            }
        }
    }
}

#[test]
fn practice_main_bpm_counts_speed_when_constant_is_requested() {
    let profile = lane_profile(BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked, true);
    let session = session_at(
        speed_2_chart(),
        &profile,
        crate::screens::play_session::PlaySessionOptions {
            hs_fix: HsFixOption::MainBpm,
            session_mode: SessionMode::Practice,
            ..Default::default()
        },
        0,
    );
    assert!(!session.constant_enabled);
    assert_eq!(session.hsfix_base_bpm, 240.0);
    assert_close(session.hispeed, 2.0, "practice MAIN hispeed");
}
