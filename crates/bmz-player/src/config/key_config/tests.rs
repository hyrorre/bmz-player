use super::*;
use crate::config::profile_config::ProfileConfig;

fn key_target(lane: LaneConfig, slot: KeyBindingSlot) -> KeyBindingTarget {
    KeyBindingTarget::Key { lane, slot }
}

fn scratch_target(
    lane: LaneConfig,
    direction: ScratchDirection,
    slot: KeyBindingSlot,
) -> KeyBindingTarget {
    KeyBindingTarget::Scratch { lane, direction, slot }
}

fn action_target(action: InputActionConfig, slot: KeyBindingSlot) -> KeyBindingTarget {
    KeyBindingTarget::Action { action, slot }
}

#[test]
fn lane_label_names_double_play_keys_by_side() {
    assert_eq!(lane_label(LaneConfig::Key1), "KEY 1");
    assert_eq!(lane_label(LaneConfig::Key7), "KEY 7");
    assert_eq!(lane_label(LaneConfig::Key8), "2P KEY 1");
    assert_eq!(lane_label(LaneConfig::Key14), "2P KEY 7");
}

#[test]
fn lane_label_for_key_mode_names_pms_extra_keys() {
    assert_eq!(lane_label_for_key_mode(KeyMode::K8, LaneConfig::Key8), "KEY 8");
    assert_eq!(lane_label_for_key_mode(KeyMode::K9, LaneConfig::Key8), "KEY 8");
    assert_eq!(lane_label_for_key_mode(KeyMode::K9, LaneConfig::Key9), "KEY 9");
    assert_eq!(lane_label_for_key_mode(KeyMode::K14, LaneConfig::Key8), "2P KEY 1");
}

#[test]
fn lane_entries_follow_active_lanes() {
    assert_eq!(lane_entries_for_key_mode(KeyMode::K7).len(), 8);
    assert_eq!(scratch_lanes_for_key_mode(KeyMode::K7).len(), 1);
    assert_eq!(key_lanes_for_key_mode(KeyMode::K7).len(), 7);
    assert_eq!(scratch_lanes_for_key_mode(KeyMode::K14).len(), 2);
}

#[test]
fn shared_key_config_target_catalog_matches_mode_lanes_and_dp_devices() {
    assert_eq!(
        common_key_binding_targets(KeyBindingSlot::KeyboardPrimary).len(),
        COMMON_ACTIONS.len()
    );
    assert_eq!(key_mode_binding_targets(KeyMode::K7, KeyBindingSlot::KeyboardPrimary).len(), 9);

    let dp = key_mode_binding_targets(KeyMode::K14, KeyBindingSlot::Controller);
    assert_eq!(dp.len(), 18);
    assert!(dp.iter().any(|target| target.slot() == KeyBindingSlot::Controller1P));
    assert!(dp.iter().any(|target| target.slot() == KeyBindingSlot::Controller2P));
}

#[test]
fn action_groups_keep_screen_order_and_play_visual_shortcuts() {
    let select =
        key_binding_targets_for_group(KeyBindingGroup::Select, KeyBindingSlot::KeyboardPrimary);
    let actions: Vec<_> = select
        .into_iter()
        .map(|target| match target {
            KeyBindingTarget::Action { action, .. } => action,
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        actions,
        vec![
            InputActionConfig::SelectModeFilter,
            InputActionConfig::SelectSort,
            InputActionConfig::SelectLnMode,
            InputActionConfig::SelectReplayCycle,
            InputActionConfig::SelectReplayPlay,
            InputActionConfig::SelectOpenKeyConfig,
            InputActionConfig::SelectRivalCycle,
            InputActionConfig::SelectSameFolder,
            InputActionConfig::SelectOpenDocuments,
            InputActionConfig::SelectDifficultyFilter,
            InputActionConfig::SelectOpenFolder,
            InputActionConfig::SelectReload,
            InputActionConfig::SelectFavoriteSong,
            InputActionConfig::SelectFavoriteChart,
            InputActionConfig::SelectAutoplayFolder,
            InputActionConfig::SelectOpenIr,
            InputActionConfig::Screenshot,
        ]
    );
    assert_eq!(PLAY_ACTIONS.len(), 7);
    assert!(PLAY_ACTIONS.contains(&InputActionConfig::PlayVisualOffsetAutoAdjust));
    assert!(
        key_binding_targets_for_group(KeyBindingGroup::Result, KeyBindingSlot::KeyboardPrimary)
            .is_empty()
    );
}

#[test]
fn apply_play_binding_keeps_primary_and_secondary_separate() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        "Z",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
        "Q",
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        ),
        "Z"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
        ),
        "Q"
    );
}

#[test]
fn secondary_binding_survives_without_primary_and_toml_roundtrip() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let primary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    let secondary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary);
    clear_play_binding(&mut profile.input, KeyMode::K7, primary).unwrap();
    apply_play_binding(&mut profile.input, KeyMode::K7, secondary, "Q").unwrap();

    assert_eq!(format_play_binding(&profile, KeyMode::K7, primary), "(none)");
    assert_eq!(format_play_binding(&profile, KeyMode::K7, secondary), "Q");
    assert!(profile.input.play["7k"].bindings.iter().any(|entry| {
        entry.device == "keyboard"
            && entry.control == "Q"
            && entry.lane == Some(LaneConfig::Key1)
            && entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Secondary)
    }));

    let serialized = toml::to_string(&profile).unwrap();
    let restored: ProfileConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(format_play_binding(&restored, KeyMode::K7, primary), "(none)");
    assert_eq!(format_play_binding(&restored, KeyMode::K7, secondary), "Q");

    let runtime =
        crate::config::play_input::lane_binding_for_key_mode(&restored.input, KeyMode::K7).unwrap();
    assert_eq!(
        runtime.resolve(
            bmz_gameplay::input::backend::DeviceId(0),
            &bmz_gameplay::input::backend::PhysicalControl::KeyboardKey("Q".to_string()),
        ),
        Some(bmz_core::lane::Lane::Key1),
    );
}

#[test]
fn explicit_keyboard_slots_ignore_binding_entry_order() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let primary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    let secondary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary);
    apply_play_binding(&mut profile.input, KeyMode::K7, primary, "A").unwrap();
    apply_play_binding(&mut profile.input, KeyMode::K7, secondary, "Q").unwrap();
    profile.input.play.get_mut("7k").unwrap().bindings.reverse();

    assert_eq!(format_play_binding(&profile, KeyMode::K7, primary), "A");
    assert_eq!(format_play_binding(&profile, KeyMode::K7, secondary), "Q");
}

#[test]
fn secondary_binding_is_preserved_in_every_key_config_mode() {
    for &key_mode in KEY_CONFIG_MODES {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let primary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
        let secondary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary);
        apply_play_binding(&mut profile.input, key_mode, secondary, "Q").unwrap();

        assert_eq!(
            format_play_binding(&profile, key_mode, primary),
            "Z",
            "{} primary",
            key_mode.as_str(),
        );
        assert_eq!(
            format_play_binding(&profile, key_mode, secondary),
            "Q",
            "{} secondary",
            key_mode.as_str(),
        );
        let controller =
            key_target(LaneConfig::Key1, controller_slot_for_lane(key_mode, LaneConfig::Key1));
        let expected_controller =
            if matches!(key_mode, KeyMode::K8 | KeyMode::K9) { "(none)" } else { "Button1" };
        assert_eq!(
            format_play_binding(&profile, key_mode, controller),
            expected_controller,
            "{} controller",
            key_mode.as_str(),
        );
    }
}

#[test]
fn action_secondary_survives_without_primary() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let primary = action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary);
    let secondary = action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary);
    clear_play_binding(&mut profile.input, KeyMode::K7, primary).unwrap();
    apply_play_binding(&mut profile.input, KeyMode::K7, secondary, "T").unwrap();

    assert_eq!(format_play_binding(&profile, KeyMode::K7, primary), "(none)");
    assert_eq!(format_play_binding(&profile, KeyMode::K7, secondary), "T");
    assert!(profile.input.ui.bindings.iter().any(|entry| {
        entry.control == "T"
            && entry.action == Some(InputActionConfig::E4)
            && entry.keyboard_slot == Some(KeyboardBindingSlotConfig::Secondary)
    }));
}

#[test]
fn scratch_secondary_directions_survive_without_primary() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    for direction in [ScratchDirection::Up, ScratchDirection::Down] {
        clear_play_binding(
            &mut profile.input,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, direction, KeyBindingSlot::KeyboardPrimary),
        )
        .unwrap();
    }
    let up_secondary = scratch_target(
        LaneConfig::Scratch,
        ScratchDirection::Up,
        KeyBindingSlot::KeyboardSecondary,
    );
    let down_secondary = scratch_target(
        LaneConfig::Scratch,
        ScratchDirection::Down,
        KeyBindingSlot::KeyboardSecondary,
    );
    apply_play_binding(&mut profile.input, KeyMode::K7, up_secondary, "Q").unwrap();
    assert_eq!(format_play_binding(&profile, KeyMode::K7, down_secondary), "(none)");
    apply_play_binding(&mut profile.input, KeyMode::K7, down_secondary, "W").unwrap();

    assert_eq!(format_play_binding(&profile, KeyMode::K7, up_secondary), "Q");
    assert_eq!(format_play_binding(&profile, KeyMode::K7, down_secondary), "W");
    for direction in [ScratchDirection::Up, ScratchDirection::Down] {
        assert_eq!(
            format_play_binding(
                &profile,
                KeyMode::K7,
                scratch_target(LaneConfig::Scratch, direction, KeyBindingSlot::KeyboardPrimary,),
            ),
            "(none)",
        );
    }
}

#[test]
fn apply_play_binding_moves_duplicate_keyboard_key() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        "Q",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key2, KeyBindingSlot::KeyboardPrimary),
        "Q",
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        ),
        "Q"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key2, KeyBindingSlot::KeyboardPrimary),
        ),
        "Q"
    );
}

#[test]
fn conflicting_bindings_are_kept_and_reported_with_priority() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let select_target =
        action_target(InputActionConfig::SelectSort, KeyBindingSlot::KeyboardPrimary);
    apply_play_binding(&mut profile.input, KeyMode::K7, select_target, "Q").unwrap();
    let common_target = action_target(InputActionConfig::E1, KeyBindingSlot::KeyboardPrimary);

    let common_warning = key_binding_conflict_description(&profile, KeyMode::K7, common_target)
        .expect("common binding should report the select conflict");
    assert!(common_warning.contains("Select") || common_warning.contains("SORT"));
    let select_warning = key_binding_conflict_description(&profile, KeyMode::K7, select_target)
        .expect("select binding should report the common conflict");
    assert!(select_warning.contains("E1"));
}

#[test]
fn key_mode_binding_wins_over_common_binding() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let target = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    apply_play_binding(&mut profile.input, KeyMode::K7, target, "Q").unwrap();
    let common_target = action_target(InputActionConfig::E1, KeyBindingSlot::KeyboardPrimary);

    let warning = key_binding_conflict_description(&profile, KeyMode::K7, common_target)
        .expect("common binding should report the lane conflict");
    assert!(warning.contains("KEY 1"));
}

#[test]
fn restoring_action_group_defaults_does_not_change_other_groups() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E1, KeyBindingSlot::KeyboardPrimary),
        "T",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::SelectSort, KeyBindingSlot::KeyboardPrimary),
        "Y",
    )
    .unwrap();

    restore_action_group_defaults(
        &mut profile.input,
        KeyBindingGroup::Common,
        KeyBindingSlot::KeyboardPrimary,
    );

    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::E1, KeyBindingSlot::KeyboardPrimary),
        ),
        "Q"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::SelectSort, KeyBindingSlot::KeyboardPrimary),
        ),
        "Y"
    );
}

#[test]
fn restoring_key_mode_defaults_restores_all_visible_lanes() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let key1 = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    let key2 = key_target(LaneConfig::Key2, KeyBindingSlot::KeyboardPrimary);
    apply_play_binding(&mut profile.input, KeyMode::K7, key1, "Q").unwrap();
    apply_play_binding(&mut profile.input, KeyMode::K7, key2, "A").unwrap();

    restore_key_mode_defaults(&mut profile.input, KeyMode::K7, KeyBindingSlot::KeyboardPrimary)
        .unwrap();

    assert_eq!(format_play_binding(&profile, KeyMode::K7, key1), "Z");
    assert_eq!(format_play_binding(&profile, KeyMode::K7, key2), "S");
}

#[test]
fn restoring_key_mode_defaults_keeps_other_modes_and_slots() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let k7_primary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    let k7_secondary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary);
    let k5_primary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    apply_play_binding(&mut profile.input, KeyMode::K7, k7_primary, "A").unwrap();
    apply_play_binding(&mut profile.input, KeyMode::K7, k7_secondary, "B").unwrap();
    apply_play_binding(&mut profile.input, KeyMode::K5, k5_primary, "C").unwrap();

    restore_key_mode_defaults(&mut profile.input, KeyMode::K7, KeyBindingSlot::KeyboardPrimary)
        .unwrap();

    assert_eq!(format_play_binding(&profile, KeyMode::K7, k7_primary), "Z");
    assert_eq!(format_play_binding(&profile, KeyMode::K7, k7_secondary), "B");
    assert_eq!(format_play_binding(&profile, KeyMode::K5, k5_primary), "C");
}

#[test]
fn restoring_inherited_key_mode_defaults_restores_visible_lanes_and_roundtrips() {
    for key_mode in [KeyMode::K4, KeyMode::K5, KeyMode::K6, KeyMode::K10] {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let primary_targets = key_mode_binding_targets(key_mode, KeyBindingSlot::KeyboardPrimary);
        let secondary_targets =
            key_mode_binding_targets(key_mode, KeyBindingSlot::KeyboardSecondary);

        for (index, target) in primary_targets.iter().copied().enumerate() {
            apply_play_binding(&mut profile.input, key_mode, target, &format!("Primary{index}"))
                .unwrap();
        }
        for (index, target) in secondary_targets.iter().copied().enumerate() {
            apply_play_binding(&mut profile.input, key_mode, target, &format!("Secondary{index}"))
                .unwrap();
        }

        restore_key_mode_defaults(&mut profile.input, key_mode, KeyBindingSlot::KeyboardPrimary)
            .unwrap();

        let expected = ProfileConfig::new_default("expected", "Expected", 0);
        for target in primary_targets {
            assert_eq!(
                format_play_binding(&profile, key_mode, target),
                format_play_binding(&expected, key_mode, target),
                "{} primary {:?}",
                key_mode.as_str(),
                target,
            );
        }
        for target in secondary_targets {
            assert!(
                format_play_binding(&profile, key_mode, target).starts_with("Secondary"),
                "{} secondary {:?} was changed",
                key_mode.as_str(),
                target,
            );
        }

        let serialized = toml::to_string(&profile).unwrap();
        let restored: ProfileConfig = toml::from_str(&serialized).unwrap();
        for target in key_mode_binding_targets(key_mode, KeyBindingSlot::KeyboardPrimary) {
            assert_eq!(
                format_play_binding(&restored, key_mode, target),
                format_play_binding(&expected, key_mode, target),
                "{} restored primary {:?}",
                key_mode.as_str(),
                target,
            );
        }
    }
}

#[test]
fn restoring_key_mode_secondary_defaults_does_not_copy_legacy_primary_defaults() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let primary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary);
    let secondary = key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary);
    apply_play_binding(&mut profile.input, KeyMode::K5, secondary, "Q").unwrap();

    restore_key_mode_defaults(&mut profile.input, KeyMode::K5, KeyBindingSlot::KeyboardSecondary)
        .unwrap();

    assert_eq!(format_play_binding(&profile, KeyMode::K5, primary), "Z");
    assert_eq!(format_play_binding(&profile, KeyMode::K5, secondary), "(none)");
}

#[test]
fn apply_play_binding_sets_controller_without_touching_keyboard() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::Controller),
        "Button9",
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::Controller),
        ),
        "Button9"
    );
    assert_ne!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        ),
        "(none)"
    );
}

#[test]
fn apply_action_binding_keeps_primary_secondary_and_controller_separate() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);

    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
        "R",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
        "T",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E4, KeyBindingSlot::Controller),
        "Button10",
    )
    .unwrap();

    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
        ),
        "R"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
        ),
        "T"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::Controller),
        ),
        "Button10"
    );
}

#[test]
fn clear_action_binding_removes_selected_slot_only() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
        "R",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
        "T",
    )
    .unwrap();

    clear_play_binding(
        &mut profile.input,
        KeyMode::K7,
        action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
    )
    .unwrap();

    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardPrimary),
        ),
        "R"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            action_target(InputActionConfig::E4, KeyBindingSlot::KeyboardSecondary),
        ),
        "(none)"
    );
}

#[test]
fn clear_play_binding_removes_selected_slot_only() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        "Z",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
        "Q",
    )
    .unwrap();
    clear_play_binding(
        &mut profile.input,
        KeyMode::K7,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        ),
        "Z"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardSecondary),
        ),
        "(none)"
    );
}

#[test]
fn scratch_keyboard_up_and_down_are_independent() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::KeyboardPrimary),
        "Q",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        scratch_target(
            LaneConfig::Scratch,
            ScratchDirection::Down,
            KeyBindingSlot::KeyboardPrimary,
        ),
        "W",
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Up,
                KeyBindingSlot::KeyboardPrimary,
            ),
        ),
        "Q"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Down,
                KeyBindingSlot::KeyboardPrimary,
            ),
        ),
        "W"
    );
    let bindings = resolve_play_bindings(&profile.input, KeyMode::K7).unwrap();
    assert!(bindings.iter().any(|entry| {
        entry.control == "Q" && entry.scratch == Some(ScratchDirectionConfig::Up)
    }));
    assert!(bindings.iter().any(|entry| {
        entry.control == "W" && entry.scratch == Some(ScratchDirectionConfig::Down)
    }));
}

#[test]
fn scratch_controller_up_and_down_are_independent() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::Controller),
        "Axis1-",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller),
        "Axis1+",
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::Controller,),
        ),
        "Axis1-"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller,),
        ),
        "Axis1+"
    );
}

#[test]
fn scratch_controller_keeps_both_directions_with_reversed_axis_polarity() {
    // 軸極性が逆のデバイス: UP に '+'、DOWN に '-' を割り当てても
    // 名前推測で再分類されず、両方向が保持される。
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::Controller),
        "Axis5+",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K7,
        scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller),
        "Axis5-",
    )
    .unwrap();
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Up, KeyBindingSlot::Controller,),
        ),
        "Axis5+"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(LaneConfig::Scratch, ScratchDirection::Down, KeyBindingSlot::Controller,),
        ),
        "Axis5-"
    );
}

#[test]
fn default_scratch_keyboard_shows_separate_keys_for_up_and_down() {
    let profile = ProfileConfig::new_default("default", "Default", 0);
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Up,
                KeyBindingSlot::KeyboardPrimary,
            ),
        ),
        "LShift"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Down,
                KeyBindingSlot::KeyboardPrimary,
            ),
        ),
        "LControl"
    );
}

#[test]
fn scratch_conflict_check_ignores_the_other_direction() {
    let profile = ProfileConfig::new_default("default", "Default", 0);

    assert!(
        key_binding_conflict_description(
            &profile,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Up,
                KeyBindingSlot::KeyboardPrimary,
            ),
        )
        .is_none()
    );
    assert!(
        key_binding_conflict_description(
            &profile,
            KeyMode::K7,
            scratch_target(
                LaneConfig::Scratch,
                ScratchDirection::Down,
                KeyBindingSlot::KeyboardPrimary,
            ),
        )
        .is_none()
    );
}

#[test]
fn fourteen_k_controller_slots_preserve_numbered_devices() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    apply_play_binding(
        &mut profile.input,
        KeyMode::K14,
        key_target(LaneConfig::Key1, KeyBindingSlot::Controller1P),
        "Button1",
    )
    .unwrap();
    apply_play_binding(
        &mut profile.input,
        KeyMode::K14,
        key_target(LaneConfig::Key8, KeyBindingSlot::Controller2P),
        "Button1",
    )
    .unwrap();

    // Editing keyboard must not collapse gamepad1/2 into wildcard.
    apply_play_binding(
        &mut profile.input,
        KeyMode::K14,
        key_target(LaneConfig::Key1, KeyBindingSlot::KeyboardPrimary),
        "Z",
    )
    .unwrap();

    let bindings =
        crate::config::play_input::resolve_play_bindings(&profile.input, KeyMode::K14).unwrap();
    assert!(bindings.iter().any(|entry| {
        entry.device == "gamepad1"
            && entry.control == "Button1"
            && entry.lane == Some(LaneConfig::Key1)
    }));
    assert!(bindings.iter().any(|entry| {
        entry.device == "gamepad2"
            && entry.control == "Button1"
            && entry.lane == Some(LaneConfig::Key8)
    }));
    assert!(!bindings.iter().any(|entry| {
        entry.device == "gamepad"
            && entry.lane == Some(LaneConfig::Key1)
            && entry.control == "Button1"
    }));
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K14,
            key_target(LaneConfig::Key1, KeyBindingSlot::Controller1P),
        ),
        "Button1"
    );
    assert_eq!(
        format_play_binding(
            &profile,
            KeyMode::K14,
            key_target(LaneConfig::Key8, KeyBindingSlot::Controller2P),
        ),
        "Button1"
    );
}

#[test]
fn controller_slot_for_lane_splits_double_play_sides() {
    assert_eq!(
        controller_slot_for_lane(KeyMode::K14, LaneConfig::Key1),
        KeyBindingSlot::Controller1P
    );
    assert_eq!(
        controller_slot_for_lane(KeyMode::K14, LaneConfig::Scratch2),
        KeyBindingSlot::Controller2P
    );
    assert_eq!(
        controller_slot_for_lane(KeyMode::K10, LaneConfig::Key8),
        KeyBindingSlot::Controller2P
    );
    assert_eq!(controller_slot_for_lane(KeyMode::K7, LaneConfig::Key1), KeyBindingSlot::Controller);
    assert_eq!(controller_slot_for_lane(KeyMode::K9, LaneConfig::Key8), KeyBindingSlot::Controller);
}
