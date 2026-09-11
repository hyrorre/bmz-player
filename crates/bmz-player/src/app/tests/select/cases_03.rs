use super::*;
use crate::app::select_flow_mode_config::{
    play_config_key_mode_for_runtime, select_item_play_mode,
};

#[test]
fn take_analog_scroll_steps_keeps_remainder() {
    let mut buffer = 7;
    assert_eq!(take_analog_scroll_steps(&mut buffer, 3), 2);
    assert_eq!(buffer, 1);

    let mut buffer = -7;
    assert_eq!(take_analog_scroll_steps(&mut buffer, 3), -2);
    assert_eq!(buffer, -1);

    let mut buffer = 2;
    assert_eq!(take_analog_scroll_steps(&mut buffer, 3), 0);
    assert_eq!(buffer, 2);
}

#[test]
fn select_modifier_keys_are_handled_before_folder_back() {
    let keys = default_select_keys();
    assert!(!is_select_modifier_key(PhysicalKey::Code(KeyCode::ArrowLeft), &keys));
    assert!(is_select_modifier_key(PhysicalKey::Code(KeyCode::KeyW), &keys));
    assert!(!is_select_modifier_key(PhysicalKey::Code(KeyCode::KeyS), &keys));
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ArrowLeft), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyW), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyS), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
}

#[test]
fn select_start_key_uses_profile_start_binding() {
    let keys = default_select_keys();
    assert!(is_select_start_key(PhysicalKey::Code(KeyCode::KeyQ), &keys));
    assert!(!is_select_start_key(PhysicalKey::Code(KeyCode::KeyW), &keys));
    assert!(!is_select_start_key(PhysicalKey::Code(KeyCode::KeyS), &keys));
}

#[test]
fn select_key_bindings_map_e1_plus_key7_to_autoplay_option() {
    let keys = default_select_keys();

    assert!(keys.is_start("Q"));
    assert!(keys.is_ui_key7("V"));
    assert!(keys.is_enter("V"));
}

#[test]
fn select_key_bindings_include_e3_action() {
    let keys = default_select_keys();

    assert!(keys.is_e3_action("E"));
}

#[test]
fn select_key_bindings_expose_key2_for_gas_toggle() {
    let keys = default_select_keys();

    assert!(keys.is_start("Q"));
    assert!(keys.is_back("W"));
    assert!(keys.is_back("S"));
    assert!(keys.is_back("D"));
    assert!(keys.is_back("F"));
    assert!(keys.is_key2("S"));
}

#[test]
fn select_key_bindings_expose_2p_keys_for_random2() {
    let keys = default_select_keys();

    assert!(keys.is_key8("M"));
    assert!(keys.is_key9("K"));
    assert!(keys.is_key10("Comma"));
    assert!(keys.is_key11("L"));
    assert!(keys.is_key12("Period"));
    assert!(keys.is_key13("Semicolon"));
    assert!(keys.is_key14("Slash"));
}

#[test]
fn select_key_bindings_treat_2p_keys_as_ui_equivalents() {
    let keys = select_keys_with_full_2p_bindings();

    for control in ["M", "Comma", "Period", "Slash", "P2K7"] {
        assert!(keys.is_enter(control), "{control} should decide like odd 1P keys");
    }
    for control in ["K", "L", "Semicolon", "P2K6"] {
        assert!(keys.is_back(control), "{control} should go back like even 1P keys");
    }
    assert_eq!(keys.ui_lane_for_control("M"), Some(Lane::Key1));
    assert_eq!(keys.ui_lane_for_control("K"), Some(Lane::Key2));
    assert_eq!(keys.ui_lane_for_control("Comma"), Some(Lane::Key3));
    assert_eq!(keys.ui_lane_for_control("L"), Some(Lane::Key4));
    assert_eq!(keys.ui_lane_for_control("Period"), Some(Lane::Key5));
    assert_eq!(keys.ui_lane_for_control("Semicolon"), Some(Lane::Key6));
    assert_eq!(keys.ui_lane_for_control("Slash"), Some(Lane::Key7));
    assert_eq!(keys.ui_lane_for_control("P2K6"), Some(Lane::Key6));
    assert_eq!(keys.ui_lane_for_control("P2K7"), Some(Lane::Key7));
}

#[test]
fn select_gauge_auto_shift_toggle_requires_start_then_key2() {
    let keys = default_select_keys();

    assert!(should_toggle_select_gauge_auto_shift("S", true, true, &keys));
    assert!(should_toggle_select_gauge_auto_shift("K", true, true, &keys));
    assert!(!should_toggle_select_gauge_auto_shift("Q", false, true, &keys));
    assert!(!should_toggle_select_gauge_auto_shift("Q", true, true, &keys));
    assert!(!should_toggle_select_gauge_auto_shift("W", true, false, &keys));
}

#[test]
fn select_judge_auto_adjust_toggle_requires_start_then_key3() {
    let keys = default_select_keys();

    assert!(should_toggle_select_judge_auto_adjust("X", true, true, &keys));
    assert!(should_toggle_select_judge_auto_adjust("Comma", true, true, &keys));
    assert!(!should_toggle_select_judge_auto_adjust("X", false, true, &keys));
    assert!(!should_toggle_select_judge_auto_adjust("S", true, true, &keys));
    assert!(!should_toggle_select_judge_auto_adjust("W", true, false, &keys));
}

#[test]
fn select_skin_cover_events_toggle_sudden_and_hidden_independently() {
    assert_eq!(toggled_select_sudden(LaneEffectConfig::Off), LaneEffectConfig::Sudden);
    assert_eq!(toggled_select_sudden(LaneEffectConfig::Hidden), LaneEffectConfig::HiddenSudden);
    assert_eq!(toggled_select_sudden(LaneEffectConfig::HiddenSudden), LaneEffectConfig::Hidden);

    assert_eq!(toggled_select_hidden(LaneEffectConfig::Off), LaneEffectConfig::Hidden);
    assert_eq!(toggled_select_hidden(LaneEffectConfig::Sudden), LaneEffectConfig::HiddenSudden);
    assert_eq!(toggled_select_hidden(LaneEffectConfig::HiddenSudden), LaneEffectConfig::Sudden);
}

#[test]
fn select_play_mode_uses_chart_mode_and_filter_fallback() {
    assert_eq!(select_item_play_mode(None, SelectModeFilter::All), Some(KeyMode::K7));
    assert_eq!(select_item_play_mode(None, SelectModeFilter::K14), Some(KeyMode::K14));

    let chart = chart_row_with_mode(1, "5K");
    assert_eq!(select_item_play_mode(Some(&chart), SelectModeFilter::All), Some(KeyMode::K5));
    assert_eq!(select_item_play_mode(Some(&chart), SelectModeFilter::K14), Some(KeyMode::K5));
}

#[test]
fn select_play_mode_requires_a_common_resolved_course_mode() {
    let mut same_mode = select_course_row(2, 2);
    same_mode.common_key_mode = Some(KeyMode::K14);
    let same_mode = SelectItem::Course(same_mode);
    assert_eq!(select_item_play_mode(Some(&same_mode), SelectModeFilter::All), Some(KeyMode::K14));

    let mixed = SelectItem::Course(select_course_row(2, 2));
    assert_eq!(select_item_play_mode(Some(&mixed), SelectModeFilter::K7), None);
}

#[test]
fn visual_offset_runtime_mode_prefers_active_then_pending_play_mode() {
    assert_eq!(
        play_config_key_mode_for_runtime(Some(KeyMode::K9), Some(KeyMode::K7)),
        Some(KeyMode::K9)
    );
    assert_eq!(play_config_key_mode_for_runtime(None, Some(KeyMode::K10)), Some(KeyMode::K10));
    assert_eq!(play_config_key_mode_for_runtime(None, None), None);
}

#[test]
fn visual_offset_runtime_mode_updates_only_actual_mode_and_roundtrips() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    profile.activate_play_mode(KeyMode::K7);
    profile.judge.visual_offset_us = 1_000;
    profile.sync_active_play_mode();
    profile.activate_play_mode(KeyMode::K9);
    profile.judge.visual_offset_us = 2_000;
    profile.sync_active_play_mode();

    let runtime_mode =
        play_config_key_mode_for_runtime(Some(KeyMode::K9), Some(KeyMode::K7)).unwrap();
    profile.activate_play_mode(runtime_mode);
    assert!(crate::config::settings_registry::adjust_settings_value(
        &mut profile,
        crate::config::settings_registry::SettingsEntryId::VisualOffsetMs,
        1,
    ));
    profile.sync_active_play_mode();

    let mut restored: ProfileConfig = toml::from_str(&toml::to_string(&profile).unwrap()).unwrap();
    restored.normalize_play_mode_configs();
    assert_eq!(restored.play_mode_config(KeyMode::K7).visual_offset_us, 1_000);
    assert_eq!(restored.play_mode_config(KeyMode::K9).visual_offset_us, 3_000);
}

#[test]
fn result_panel_direct_selection_matches_tab_availability() {
    assert_eq!(selected_result_panel(1, 2, true, true), Some(2));
    assert_eq!(selected_result_panel(2, 1, true, true), Some(1));
    assert_eq!(selected_result_panel(2, 1, true, false), None);
    assert_eq!(selected_result_panel(1, 2, true, false), Some(2));
    assert_eq!(selected_result_panel(2, 2, true, true), None);
    assert_eq!(selected_result_panel(1, 2, false, true), None);
}

#[test]
fn play_option_control_uses_chart_mode_instead_of_select_input_mode() {
    let input = crate::config::play_input::default_profile_input();
    assert_eq!(input.select_input_mode, SelectInputModeConfig::Key7Key14);
    let keys = SelectKeyBindings::from_profile(&input);
    let play_input = play_option_input_for(&input, KeyMode::K9);

    assert_eq!(
        keyboard_play_option("B", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Down))
    );
    assert_eq!(
        keyboard_play_option("G", true, false, &keys, &play_input, &input),
        Some(PlayOptionControl::Hispeed(HispeedChange::Up))
    );
}

#[test]
fn select_skin_duration_is_derived_from_green_number_for_nhs() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hispeed = 2.0;
    profile.lane.base_hispeed = BaseHispeedConfig::Normal;
    profile.lane.floating_policy = FloatingPolicyConfig::Disabled;
    profile.lane.normal_hispeed_level = 18;

    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            false,
        ),
        500
    );
}

#[test]
fn select_skin_duration_is_derived_from_green_number_for_fhs() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 280;

    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            true,
        ),
        467
    );
}

#[test]
fn select_skin_floating_duration_does_not_reapply_sudden_cover() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    profile.lane.sudden = 200;

    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            true,
        ),
        500
    );
}

#[test]
fn select_skin_floating_duration_applies_hidden_to_remaining_lane() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.floating_policy = FloatingPolicyConfig::Locked;
    profile.lane.target_green_number = 300;
    profile.play.lane_effect = LaneEffectConfig::HiddenSudden;
    profile.lane.sudden = 200;
    profile.lane.hidden = 300;

    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            true,
        ),
        313
    );

    profile.lane.lift_enabled = true;
    profile.lane.lift = 200;
    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            true,
        ),
        300
    );
}

#[test]
fn select_skin_duration_accounts_for_enabled_sudden_and_hidden_covers() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = BaseHispeedConfig::Normal;
    profile.lane.floating_policy = FloatingPolicyConfig::Disabled;
    profile.lane.normal_hispeed_level = 18;
    profile.play.lane_effect = LaneEffectConfig::HiddenSudden;
    profile.lane.sudden = 200;
    profile.lane.hidden = 300;

    let duration_ms = WinitApp::select_note_display_duration_ms_for_config(
        &profile.play_mode_config(profile.active_play_mode),
        false,
    );
    assert_eq!(duration_ms, 250);
    assert_eq!(crate::config::play::green_number_from_duration_ms(duration_ms as u32), 150);
}

#[test]
fn select_skin_duration_ignores_disabled_cover_values_and_clamps_overlap() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.base_hispeed = BaseHispeedConfig::Normal;
    profile.lane.floating_policy = FloatingPolicyConfig::Disabled;
    profile.lane.normal_hispeed_level = 18;
    profile.lane.sudden = 700;
    profile.lane.hidden = 600;

    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            false,
        ),
        500
    );

    profile.play.lane_effect = LaneEffectConfig::HiddenSudden;
    assert_eq!(
        WinitApp::select_note_display_duration_ms_for_config(
            &profile.play_mode_config(profile.active_play_mode),
            false,
        ),
        0
    );
}

#[test]
fn profile_play_option_changes_sync_all_select_runtime_options() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let before = profile.play.clone();
    let current = select_play_options_from_profile(&before);
    let mut after = before.clone();
    after.gauge = GaugeTypeConfig::AutoShift;
    after.gauge_auto_shift = GaugeAutoShiftConfig::Continue;
    after.bottom_shiftable_gauge = BottomShiftableGaugeConfig::Normal;
    after.random = RandomOptionConfig::SRandom;
    after.random2 = RandomOptionConfig::RRandom;
    after.double_option = DoubleOptionConfig::Flip;
    after.hs_fix = HsFixConfig::MainBpm;
    after.target = TargetOptionConfig::RankAaa;
    after.auto_play = true;

    let synced = merge_changed_select_play_options_from_profile(current, &before, &after);

    assert_eq!(synced.gauge, GaugeTypeConfig::ExHard);
    assert_eq!(synced.gauge_auto_shift, GaugeAutoShiftConfig::BestClear);
    assert_eq!(synced.bottom_shiftable_gauge, BottomShiftableGaugeConfig::Normal);
    assert_eq!(synced.arrange, ArrangeOption::SRandom);
    assert_eq!(synced.arrange_2p, ArrangeOption::RRandom);
    assert_eq!(synced.double_option, DoubleOption::Flip);
    assert_eq!(synced.hs_fix, HsFixOption::MainBpm);
    assert_eq!(synced.target, TargetOption::RankAaa);
    assert_eq!(synced.session_mode, SessionMode::Autoplay);
}
