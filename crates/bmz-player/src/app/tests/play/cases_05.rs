use super::*;
use crate::app::play_flow_practice::{
    PracticeGamepadAction, apply_session_mode_start_policy, practice_analog_cursor_delta,
    practice_gamepad_action,
};
use crate::app::scene_state::playback_overlay_suffix;

#[test]
fn viewer_seek_skips_only_the_ready_presentation() {
    assert!(PlayEntryPresentation::Normal.shows_ready_presentation());
    assert!(!PlayEntryPresentation::Normal.is_seamless());
    assert!(!PlayEntryPresentation::ViewerSeek.shows_ready_presentation());
    assert!(PlayEntryPresentation::ViewerSeek.is_seamless());
    assert_eq!(
        play_entry_elapsed_time(PlayEntryPresentation::Normal, TimeUs(5_000_000)),
        TimeUs(0)
    );
    assert_eq!(
        play_entry_elapsed_time(PlayEntryPresentation::ViewerSeek, TimeUs(5_000_000)),
        TimeUs(5_000_000)
    );
}

#[test]
fn viewer_ready_timer_survives_repeated_seeks_without_restarting_intro() {
    let normal = PlayEntryPresentation::Normal;
    let viewer = PlayEntryPresentation::ViewerSeek;
    assert_eq!(play_ready_skin_elapsed_time(normal, None, TimeUs(0)), None);
    assert_eq!(play_ready_skin_elapsed_time(viewer, None, TimeUs(0)), None);
    assert_eq!(
        play_ready_skin_elapsed_time(normal, Some(TimeUs(250_000)), TimeUs(100_000_000)),
        Some(TimeUs(250_000))
    );
    let first = play_ready_skin_elapsed_time(viewer, Some(TimeUs(0)), TimeUs(500_000));
    assert_eq!(first, Some(TimeUs(100_000_000)));
    let later = play_ready_skin_elapsed_time(viewer, Some(TimeUs(2_000_000)), TimeUs(500_000));
    assert_eq!(later, Some(TimeUs(102_000_000)));
    assert_eq!(play_ready_skin_elapsed_time(viewer, Some(TimeUs(0)), later.unwrap()), later);
    assert_eq!(
        play_ready_skin_elapsed_time(viewer, Some(TimeUs(1_000_000)), TimeUs(150_000_000)),
        Some(TimeUs(151_000_000))
    );
}

#[test]
fn decide_launch_promotes_only_staged_practice_config() {
    assert!(DecideLaunch::Play.into_practice_session().is_none());

    let staged = PracticeSession {
        chart_id: 42,
        chart_title: "Practice".to_string(),
        chart_sha256: [7; 32],
        property: Default::default(),
        phase: PracticePhase::Config,
        max_end_time_ms: 120_000,
        last_graph: Arc::new(Default::default()),
        graph_start_time_ms: 0,
        is_double: false,
        cursor: 0,
        preview_time_ms: None,
        battle_target: None,
    };
    let promoted = DecideLaunch::practice(staged).into_practice_session().unwrap();

    assert_eq!(promoted.chart_id, 42);
    assert_eq!(promoted.phase, PracticePhase::Config);
}

#[test]
fn decide_launch_keeps_large_practice_state_out_of_line() {
    assert!(
        std::mem::size_of::<DecideLaunch>() <= std::mem::size_of::<usize>() * 2,
        "DecideLaunch should remain pointer-sized instead of embedding PracticeSession"
    );
}

#[test]
fn battle_target_start_policy_preserves_every_session_mode() {
    for session_mode in SessionMode::VALUES {
        let mut options = PlayStartOptions {
            session_mode,
            battle_target: Some(crate::screens::play_start::BattleTarget {
                provider: "test".to_string(),
                score_id: "score".to_string(),
                player_id: "player".to_string(),
                player_name: "RIVAL".to_string(),
                rank: 1,
                ex_score: 1234,
                gauge: None,
                playback: crate::screens::play_start::BattleTargetPlayback::Seed {
                    arrange: ArrangeOption::Normal,
                    arrange_2p: ArrangeOption::Normal,
                    double_option: DoubleOption::Off,
                    packed_seed: None,
                },
            }),
            ..Default::default()
        };

        apply_session_mode_start_policy(&mut options);

        assert_eq!(options.session_mode, session_mode);
        assert_eq!(options.autoplay, session_mode == SessionMode::AutoplayBattle);
        assert_eq!(
            options.resolved_target,
            Some(ResolvedTarget { name: "RIVAL".to_string(), ex_score: 1234 })
        );
    }
}

fn practice_gamepad_test_input(key_mode: KeyMode) -> PlayOptionInput {
    let entry =
        |control: &str, lane, scratch_direction| bmz_gameplay::input::binding::BindingEntry {
            device: None,
            control: PhysicalControl::GamepadButton(control.to_string()),
            lane,
            scratch_direction,
        };
    PlayOptionInput {
        key_mode,
        binding: LaneBinding {
            entries: vec![
                entry("Button1", Lane::Key1, None),
                entry("Button2", Lane::Key2, None),
                entry("Button3", Lane::Key3, None),
                entry("Button4", Lane::Key4, None),
                entry("Axis1+", Lane::Scratch, Some(ScratchDirection::Down)),
                entry("Axis1-", Lane::Scratch, Some(ScratchDirection::Up)),
            ],
        },
        scratch_binding: LaneBinding { entries: Vec::new() },
        action_bindings: Vec::new(),
    }
}

#[test]
fn practice_gamepad_uses_play_lanes_for_horizontal_controls() {
    let input = practice_gamepad_test_input(KeyMode::K7);
    let action = |button: &str| {
        practice_gamepad_action(
            DeviceId(16),
            &PhysicalControl::GamepadButton(button.to_string()),
            false,
            Some(&input),
        )
    };

    assert_eq!(action("Button1"), PracticeGamepadAction::Adjust(true));
    assert_eq!(action("Button2"), PracticeGamepadAction::Adjust(false));
    assert_eq!(action("Button3"), PracticeGamepadAction::Adjust(true));
    assert_eq!(action("Button4"), PracticeGamepadAction::Adjust(false));
    assert_eq!(action("Button9"), PracticeGamepadAction::Ignore);
}

#[test]
fn practice_gamepad_scratch_moves_cursor_without_double_counting_analog_press() {
    let input = practice_gamepad_test_input(KeyMode::K7);
    let down = PhysicalControl::GamepadButton("Axis1+".to_string());
    let up = PhysicalControl::GamepadButton("Axis1-".to_string());

    assert_eq!(
        practice_gamepad_action(DeviceId(16), &down, false, Some(&input)),
        PracticeGamepadAction::Move(true)
    );
    assert_eq!(
        practice_gamepad_action(DeviceId(16), &up, false, Some(&input)),
        PracticeGamepadAction::Move(false)
    );
    assert_eq!(
        practice_gamepad_action(DeviceId(16), &down, true, Some(&input)),
        PracticeGamepadAction::Ignore
    );
    assert_eq!(practice_analog_cursor_delta(DeviceId(16), "Axis1", 4, Some(&input)), Some(4));
    assert_eq!(practice_analog_cursor_delta(DeviceId(16), "Axis1", -3, Some(&input)), Some(-3));
}

#[test]
fn practice_gamepad_dp_normalizes_second_player_key_parity() {
    let input = PlayOptionInput {
        key_mode: KeyMode::K14,
        binding: LaneBinding {
            entries: vec![
                bmz_gameplay::input::binding::BindingEntry {
                    device: Some(DeviceId(17)),
                    control: PhysicalControl::GamepadButton("Button1".to_string()),
                    lane: Lane::Key8,
                    scratch_direction: None,
                },
                bmz_gameplay::input::binding::BindingEntry {
                    device: Some(DeviceId(17)),
                    control: PhysicalControl::GamepadButton("Button2".to_string()),
                    lane: Lane::Key9,
                    scratch_direction: None,
                },
            ],
        },
        scratch_binding: LaneBinding { entries: Vec::new() },
        action_bindings: Vec::new(),
    };

    assert_eq!(
        practice_gamepad_action(
            DeviceId(17),
            &PhysicalControl::GamepadButton("Button1".to_string()),
            false,
            Some(&input),
        ),
        PracticeGamepadAction::Adjust(true)
    );
    assert_eq!(
        practice_gamepad_action(
            DeviceId(17),
            &PhysicalControl::GamepadButton("Button2".to_string()),
            false,
            Some(&input),
        ),
        PracticeGamepadAction::Adjust(false)
    );
}

#[test]
fn playback_overlay_suffix_exposes_each_non_normal_session_mode() {
    assert_eq!(playback_overlay_suffix(SessionMode::Normal, false, false), None);
    assert_eq!(playback_overlay_suffix(SessionMode::Practice, false, false), Some("PRACTICE"));
    assert_eq!(playback_overlay_suffix(SessionMode::Autoplay, true, false), Some("AUTOPLAY"));
    assert_eq!(
        playback_overlay_suffix(SessionMode::AutoplayBattle, true, false),
        Some("AUTO BATTLE")
    );
    assert_eq!(playback_overlay_suffix(SessionMode::GBattle, false, false), Some("G-BATTLE"));
}

#[test]
fn playback_overlay_suffix_uses_effective_playback_flags() {
    assert_eq!(playback_overlay_suffix(SessionMode::Normal, true, false), Some("AUTOPLAY"));
    assert_eq!(playback_overlay_suffix(SessionMode::Normal, false, true), Some("REPLAY"));
    assert_eq!(playback_overlay_suffix(SessionMode::Autoplay, true, true), Some("REPLAY"));
}

#[test]
fn session_mode_profile_migrates_legacy_autoplay_and_persists_autoplay() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.session_mode = None;
    profile.play.auto_play = true;
    assert_eq!(session_mode_from_profile(&profile.play), SessionMode::Autoplay);

    let mut options = select_play_options_from_profile(&profile.play);
    options.session_mode = SessionMode::Autoplay;
    apply_current_play_options_to_profile(&mut profile, None, None, options, 2);

    assert_eq!(profile.play.session_mode, Some(SessionMode::Autoplay));
    assert!(profile.play.auto_play);
    let serialized = toml::to_string(&profile).unwrap();
    assert!(serialized.contains(r#"session_mode = "Autoplay""#));
}

#[test]
fn session_mode_profile_persists_practice_without_legacy_autoplay() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    let mut options = select_play_options_from_profile(&profile.play);
    options.session_mode = SessionMode::Practice;

    apply_current_play_options_to_profile(&mut profile, None, None, options, 2);

    assert_eq!(profile.play.session_mode, Some(SessionMode::Practice));
    assert!(!profile.play.auto_play);
    let serialized = toml::to_string(&profile).unwrap();
    assert!(serialized.contains(r#"session_mode = "Practice""#));
}

#[test]
fn session_mode_profile_persists_g_battle_without_legacy_autoplay() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    let mut options = select_play_options_from_profile(&profile.play);
    options.session_mode = SessionMode::GBattle;

    apply_current_play_options_to_profile(&mut profile, None, None, options, 2);

    assert_eq!(profile.play.session_mode, Some(SessionMode::GBattle));
    assert!(!profile.play.auto_play);
    let serialized = toml::to_string(&profile).unwrap();
    assert!(serialized.contains(r#"session_mode = "GBattle""#));
}

#[test]
fn profile_random_change_preserves_cli_autoplay_runtime_option() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let before = profile.play.clone();
    let mut current = select_play_options_from_profile(&before);
    current.session_mode = SessionMode::Autoplay;

    let mut after = before.clone();
    after.random = RandomOptionConfig::Mirror;
    let synced = merge_changed_select_play_options_from_profile(current, &before, &after);

    assert_eq!(synced.arrange, ArrangeOption::Mirror);
    assert_eq!(synced.session_mode, SessionMode::Autoplay);
}

#[test]
fn apply_lane_state_preserves_lift_amount_while_lift_is_disabled() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.lift = 240;
    profile.lane.lift_enabled = false;

    apply_lane_state_to_profile(
        &mut profile,
        None,
        Some(ActiveLaneState {
            lane_cover: 0.3,
            lift: 0.0,
            hidden_cover: 0.0,
            sudden_enabled: true,
            lift_enabled: false,
            hidden_enabled: false,
            hispeed_mode: HispeedMode::Normal,
            base_hispeed_mode: HispeedMode::Normal,
            floating_policy: FloatingPolicy::Toggle,
            normal_hispeed_level: 18,
            target_green_number: 300,
        }),
    );

    assert_eq!(profile.lane.lift, 240);
    assert!(!profile.lane.lift_enabled);
}

#[test]
fn arrange_option_maps_profile_random_defaults() {
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::Off), ArrangeOption::Normal);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::Mirror), ArrangeOption::Mirror);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::Random), ArrangeOption::Random);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::RRandom), ArrangeOption::RRandom);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::SRandom), ArrangeOption::SRandom);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::Spiral), ArrangeOption::Spiral);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::HRandom), ArrangeOption::HRandom);
    assert_eq!(
        arrange_option_from_profile(RandomOptionConfig::AllScratch),
        ArrangeOption::AllScratch
    );
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::RandomEx), ArrangeOption::RandomEx);
    assert_eq!(
        arrange_option_from_profile(RandomOptionConfig::SRandomEx),
        ArrangeOption::SRandomEx
    );
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::FRandom), ArrangeOption::FRandom);
    assert_eq!(arrange_option_from_profile(RandomOptionConfig::MFRandom), ArrangeOption::MFRandom);
    assert!(matches!(random_config_from_arrange(ArrangeOption::Normal), RandomOptionConfig::Off));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::Mirror),
        RandomOptionConfig::Mirror
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::Random),
        RandomOptionConfig::Random
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::RRandom),
        RandomOptionConfig::RRandom
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::SRandom),
        RandomOptionConfig::SRandom
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::Spiral),
        RandomOptionConfig::Spiral
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::HRandom),
        RandomOptionConfig::HRandom
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::AllScratch),
        RandomOptionConfig::AllScratch
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::RandomEx),
        RandomOptionConfig::RandomEx
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::SRandomEx),
        RandomOptionConfig::SRandomEx
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::FRandom),
        RandomOptionConfig::FRandom
    ));
    assert!(matches!(
        random_config_from_arrange(ArrangeOption::MFRandom),
        RandomOptionConfig::MFRandom
    ));
}

#[test]
fn play_scene_keeps_decide_bgm_for_ready_fade() {
    use crate::system_sound::SoundType;

    let sounds = system_bgm_stop_targets_on_scene_enter(AppSceneKind::Play);

    assert!(sounds.contains(&SoundType::Select));
    assert!(!sounds.contains(&SoundType::Decide));
}

#[test]
fn decide_bgm_fade_out_spans_ready_to_chart_start() {
    assert_eq!(decide_bgm_fade_out_frames(TimeUs(-1_500_000), 48_000), 72_000);
    assert_eq!(decide_bgm_fade_out_frames(TimeUs(-6_500_000), 48_000), 312_000);
}

#[test]
fn decide_bgm_fade_out_is_immediate_without_ready_lead() {
    assert_eq!(decide_bgm_fade_out_frames(TimeUs(0), 48_000), 0);
    assert_eq!(decide_bgm_fade_out_frames(TimeUs(500_000), 48_000), 0);
    assert_eq!(decide_bgm_fade_out_frames(TimeUs(-1_500_000), 0), 0);
}

#[test]
fn non_play_scene_stops_all_transition_bgms() {
    use crate::system_sound::SoundType;

    for scene in [AppSceneKind::Select, AppSceneKind::Decide, AppSceneKind::Result] {
        let sounds = system_bgm_stop_targets_on_scene_enter(scene);
        assert!(sounds.contains(&SoundType::Select), "scene={scene:?}");
        assert!(sounds.contains(&SoundType::Decide), "scene={scene:?}");
    }
}

#[test]
fn returning_to_select_reshuffles_system_sound_sets() {
    assert!(!should_shuffle_system_sound_sets_on_scene_enter(None, AppSceneKind::Select));
    assert!(!should_shuffle_system_sound_sets_on_scene_enter(
        Some(AppSceneKind::Select),
        AppSceneKind::Select,
    ));
    for previous in [AppSceneKind::Decide, AppSceneKind::Play, AppSceneKind::Result] {
        assert!(
            should_shuffle_system_sound_sets_on_scene_enter(Some(previous), AppSceneKind::Select),
            "previous={previous:?}"
        );
    }
    for next in [AppSceneKind::Decide, AppSceneKind::Play, AppSceneKind::Result] {
        assert!(
            !should_shuffle_system_sound_sets_on_scene_enter(Some(AppSceneKind::Select), next,)
        );
    }
}

#[test]
fn left_overlay_hides_toast_while_screenshot_pending() {
    let toast = Some(("スクリーンショットを保存しました", Duration::from_millis(100)));
    assert_eq!(resolve_left_overlay_text(true, toast, "SCAN 1 / 2"), "SCAN 1 / 2");
    assert_eq!(
        resolve_left_overlay_text(false, toast, "SCAN 1 / 2"),
        "スクリーンショットを保存しました"
    );
}

#[test]
fn clear_rank_separates_unowned_from_noplay() {
    // 所持済み・スコア無し → NoPlay = 0。
    let noplay = select_chart_row(1);
    assert!(noplay.in_library());
    assert_eq!(clear_rank(&noplay), 0);

    // 難易度表エントリだがローカル未所持 → NoPlay より下位の -1。
    let mut unowned = select_chart_row(2);
    unowned.chart = None;
    unowned.entry_sha256 = Some([2u8; 32]);
    assert!(!unowned.in_library());
    assert_eq!(clear_rank(&unowned), -1);

    assert!(clear_rank(&unowned) < clear_rank(&noplay));
}
