//! プレイモード別入力 binding の inherit 解決。

use std::collections::{BTreeMap, HashSet};

use std::fmt;

use bmz_core::input::ScratchDirection;
use bmz_core::lane::{KeyMode, Lane};
use bmz_gameplay::input::backend::{DeviceId, PhysicalControl};
use bmz_gameplay::input::binding::{BindingEntry, LaneBinding};

use super::play::lane_from_config;
use super::profile_config::{
    BindingConfigEntry, HispeedDirectionConfig, LaneConfig, PlayModeInputConfig,
    ProfileInputConfig, ScratchDirectionConfig,
};
use crate::input::gamepad::GamepadSlotMap;

mod defaults;
mod device;
mod inheritance;
mod normalize;

pub use defaults::*;
pub use device::{gamepad_player_index, is_gamepad_device};
pub use inheritance::*;
pub use normalize::*;

use device::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{
        GamepadScratchConfig, ProfileInputConfig, SelectInputModeConfig, UiInputConfig,
        default_ui_bindings,
    };

    fn sample_7k_input() -> ProfileInputConfig {
        let mut play = BTreeMap::new();
        play.insert(
            "7k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: default_play_7k_bindings(),
                ..Default::default()
            },
        );
        ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            select_input_mode: SelectInputModeConfig::Key7Key14,
            start_key: None,
            ui: UiInputConfig {
                version: crate::config::profile_config::UI_INPUT_BINDING_VERSION,
                bindings: default_ui_bindings(),
            },
            play,
            legacy_bindings: Vec::new(),
            legacy_analog_scratch_sensitivity: None,
            analog_scratch_timeout_ms: 500,
            legacy_analog_scratch_threshold: None,
            gamepad1: GamepadScratchConfig::default(),
            gamepad2: GamepadScratchConfig::default(),
            analog_ticks_per_scroll: 3,
            lane_cover_step: crate::config::profile_config::default_lane_cover_step(),
            lane_cover_hold_delay_ms:
                crate::config::profile_config::default_lane_cover_hold_delay_ms(),
            lane_cover_hold_step: crate::config::profile_config::default_lane_cover_hold_step(),
            lane_cover_wheel_step: crate::config::profile_config::default_lane_cover_wheel_step(),
            keyboard_release_bounce_ms: 0,
            controller_release_bounce_ms: 0,
        }
    }

    #[test]
    fn five_k_inherits_seven_k_without_config() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K5).unwrap();
        let lanes: HashSet<_> = bindings.iter().filter_map(|e| e.lane).collect();
        assert!(lanes.contains(&LaneConfig::Scratch));
        assert!(lanes.contains(&LaneConfig::Key5));
        assert!(!lanes.contains(&LaneConfig::Key6));
    }

    #[test]
    fn ten_k_inherits_fourteen_k_without_config() {
        let mut play = BTreeMap::new();
        play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: default_play_14k_bindings(),
                ..Default::default()
            },
        );
        let input = ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            select_input_mode: SelectInputModeConfig::Key7Key14,
            start_key: None,
            ui: UiInputConfig::default(),
            play,
            legacy_bindings: Vec::new(),
            legacy_analog_scratch_sensitivity: None,
            analog_scratch_timeout_ms: 500,
            legacy_analog_scratch_threshold: None,
            gamepad1: GamepadScratchConfig::default(),
            gamepad2: GamepadScratchConfig::default(),
            analog_ticks_per_scroll: 3,
            lane_cover_step: crate::config::profile_config::default_lane_cover_step(),
            lane_cover_hold_delay_ms:
                crate::config::profile_config::default_lane_cover_hold_delay_ms(),
            lane_cover_hold_step: crate::config::profile_config::default_lane_cover_hold_step(),
            lane_cover_wheel_step: crate::config::profile_config::default_lane_cover_wheel_step(),
            keyboard_release_bounce_ms: 0,
            controller_release_bounce_ms: 0,
        };
        let bindings = resolve_play_bindings(&input, KeyMode::K10).unwrap();
        assert!(bindings.iter().any(|e| e.lane == Some(LaneConfig::Key8)));
        assert!(!bindings.iter().any(|e| e.lane == Some(LaneConfig::Key6)));
    }

    #[test]
    fn four_k_remaps_parent_lanes() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K4).unwrap();
        let key = |lane: LaneConfig| {
            bindings
                .iter()
                .filter(|entry| entry.device == "keyboard")
                .find(|entry| entry.lane == Some(lane))
                .map(|entry| entry.control.as_str())
                .unwrap()
        };
        assert_eq!(key(LaneConfig::Key1), "Z");
        assert_eq!(key(LaneConfig::Key2), "S");
        assert_eq!(key(LaneConfig::Key3), "D");
        assert_eq!(key(LaneConfig::Key4), "C");
    }

    #[test]
    fn six_k_remaps_parent_lanes() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K6).unwrap();
        let key = |lane: LaneConfig| {
            bindings
                .iter()
                .filter(|entry| entry.device == "keyboard")
                .find(|entry| entry.lane == Some(lane))
                .map(|entry| entry.control.as_str())
                .unwrap()
        };
        assert_eq!(key(LaneConfig::Key4), "C");
        assert_eq!(key(LaneConfig::Key5), "F");
        assert_eq!(key(LaneConfig::Key6), "V");
    }

    #[test]
    fn eight_k_uses_scratchless_default_lanes() {
        let input = sample_7k_input();
        let bindings = resolve_play_bindings(&input, KeyMode::K8).unwrap();
        assert!(!bindings.iter().any(|e| e.lane == Some(LaneConfig::Scratch)));
        assert!(bindings.iter().any(|e| e.lane == Some(LaneConfig::Key1)));
        assert!(bindings.iter().any(|e| e.lane == Some(LaneConfig::Key8)));
    }

    #[test]
    fn nine_k_does_not_inherit_seven_k() {
        let input = sample_7k_input();
        assert!(resolve_play_bindings(&input, KeyMode::K9).is_ok());
        let mut play = input.play.clone();
        play.insert(
            "9k".to_string(),
            PlayModeInputConfig {
                inherit: Some("7k".into()),
                bindings: Vec::new(),
                ..Default::default()
            },
        );
        let input = ProfileInputConfig { play, ..input };
        assert_eq!(
            validate_play_inherit_config(&input),
            Err(InheritError::RootWithInherit { mode: KeyMode::K9 })
        );
    }

    #[test]
    fn four_k_inherit_five_k_allowed() {
        let input = sample_7k_input();
        let mut play = input.play.clone();
        play.insert(
            "4k".to_string(),
            PlayModeInputConfig {
                inherit: Some("5k".into()),
                bindings: Vec::new(),
                ..Default::default()
            },
        );
        let input = ProfileInputConfig { play, ..input };
        validate_play_inherit_config(&input).unwrap();
        let bindings = resolve_play_bindings(&input, KeyMode::K4).unwrap();
        assert_eq!(bindings.len(), 8);
    }

    #[test]
    fn remap_inherit_preserves_all_bindings_for_each_parent_lane() {
        for (key_mode, parent_lane, child_lane) in [
            (KeyMode::K4, LaneConfig::Key4, LaneConfig::Key3),
            (KeyMode::K6, LaneConfig::Key5, LaneConfig::Key4),
        ] {
            let mut input = sample_7k_input();
            let mut secondary = play_binding("Q", parent_lane);
            secondary.keyboard_slot =
                Some(crate::config::profile_config::KeyboardBindingSlotConfig::Secondary);
            input.play.get_mut("7k").unwrap().bindings.push(secondary);

            let remapped: Vec<_> = resolve_play_bindings(&input, key_mode)
                .unwrap()
                .into_iter()
                .filter(|entry| entry.lane == Some(child_lane))
                .collect();

            assert_eq!(remapped.len(), 3, "{}", key_mode.as_str());
            assert!(
                remapped
                    .iter()
                    .any(|entry| entry.device == "keyboard" && entry.keyboard_slot.is_none())
            );
            assert!(remapped.iter().any(|entry| {
                entry.device == "keyboard"
                    && entry.control == "Q"
                    && entry.keyboard_slot
                        == Some(crate::config::profile_config::KeyboardBindingSlotConfig::Secondary)
            }));
            assert!(
                remapped.iter().any(|entry| entry.device == "gamepad"),
                "{}",
                key_mode.as_str(),
            );
        }
    }

    #[test]
    fn lane_override_replaces_parent_with_all_override_bindings() {
        let mut input = sample_7k_input();
        let mut primary = play_binding("A", LaneConfig::Key1);
        primary.keyboard_slot =
            Some(crate::config::profile_config::KeyboardBindingSlotConfig::Primary);
        let mut secondary = play_binding("Q", LaneConfig::Key1);
        secondary.keyboard_slot =
            Some(crate::config::profile_config::KeyboardBindingSlotConfig::Secondary);
        input.play.insert(
            "5k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![primary, secondary],
                ..Default::default()
            },
        );

        let overridden: Vec<_> = resolve_play_bindings(&input, KeyMode::K5)
            .unwrap()
            .into_iter()
            .filter(|entry| entry.lane == Some(LaneConfig::Key1))
            .collect();

        assert_eq!(overridden.len(), 2);
        assert_eq!(overridden[0].control, "A");
        assert_eq!(overridden[1].control, "Q");
    }

    #[test]
    fn six_k_inherit_five_k_rejected() {
        let mut play = BTreeMap::new();
        play.insert(
            "6k".to_string(),
            PlayModeInputConfig {
                inherit: Some("5k".into()),
                bindings: Vec::new(),
                ..Default::default()
            },
        );
        let input = ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            select_input_mode: SelectInputModeConfig::Key7Key14,
            start_key: None,
            ui: UiInputConfig::default(),
            play,
            legacy_bindings: Vec::new(),
            legacy_analog_scratch_sensitivity: None,
            analog_scratch_timeout_ms: 500,
            legacy_analog_scratch_threshold: None,
            gamepad1: GamepadScratchConfig::default(),
            gamepad2: GamepadScratchConfig::default(),
            analog_ticks_per_scroll: 3,
            lane_cover_step: crate::config::profile_config::default_lane_cover_step(),
            lane_cover_hold_delay_ms:
                crate::config::profile_config::default_lane_cover_hold_delay_ms(),
            lane_cover_hold_step: crate::config::profile_config::default_lane_cover_hold_step(),
            lane_cover_wheel_step: crate::config::profile_config::default_lane_cover_wheel_step(),
            keyboard_release_bounce_ms: 0,
            controller_release_bounce_ms: 0,
        };
        assert_eq!(
            validate_play_inherit_config(&input),
            Err(InheritError::Disallowed { child: KeyMode::K6, parent: KeyMode::K5 })
        );
    }

    #[test]
    fn migrate_legacy_splits_ui_and_play() {
        let legacy = crate::config::profile_config::default_bindings();
        let (ui, play) = migrate_legacy_bindings(&legacy);
        assert!(ui.iter().any(|e| e.action.is_some()));
        assert!(play.contains_key("7k"));
    }

    #[test]
    fn gamepad_numbered_devices_resolve_to_specific_device_ids() {
        let mut input = sample_7k_input();
        input.play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![
                    gamepad_play_binding_for_device("gamepad1", "Button1", LaneConfig::Key1),
                    gamepad_play_binding_for_device("gamepad2", "Button1", LaneConfig::Key8),
                ],
                ..Default::default()
            },
        );

        let binding = lane_binding_for_key_mode(&input, KeyMode::K14).unwrap();

        assert_eq!(
            binding.resolve(DeviceId(16), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
        assert_eq!(
            binding.resolve(DeviceId(17), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key8)
        );
        assert_eq!(
            binding.resolve(DeviceId(18), &PhysicalControl::GamepadButton("Button1".into())),
            None
        );
    }

    #[test]
    fn gamepad_slot_map_remaps_logical_players_to_assigned_gilrs_ids() {
        let mut input = sample_7k_input();
        input.play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![
                    gamepad_play_binding_for_device("gamepad1", "Button1", LaneConfig::Key1),
                    gamepad_play_binding_for_device("gamepad2", "Button1", LaneConfig::Key8),
                ],
                ..Default::default()
            },
        );

        // Swap: logical 1P → gilrs id 1 (DeviceId 17), logical 2P → gilrs id 0 (DeviceId 16)
        let slots = GamepadSlotMap::from_slot_ids([Some(1), Some(0)]);
        let binding = lane_binding_for_key_mode_with_slots(&input, KeyMode::K14, slots).unwrap();

        assert_eq!(
            binding.resolve(DeviceId(17), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
        assert_eq!(
            binding.resolve(DeviceId(16), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key8)
        );
    }

    #[test]
    fn numbered_gamepads_above_two_remain_device_specific() {
        let mut input = sample_7k_input();
        input.play.insert(
            "14k".to_string(),
            PlayModeInputConfig {
                inherit: None,
                bindings: vec![gamepad_play_binding_for_device(
                    "gamepad3",
                    "Button1",
                    LaneConfig::Key1,
                )],
                ..Default::default()
            },
        );

        let binding = lane_binding_for_key_mode(&input, KeyMode::K14).unwrap();
        let control = PhysicalControl::GamepadButton("Button1".into());
        assert_eq!(binding.resolve(DeviceId(18), &control), Some(Lane::Key1));
        assert_eq!(binding.resolve(DeviceId(16), &control), None);
    }

    #[test]
    fn gamepad_wildcard_still_matches_any_gamepad_device() {
        let input = sample_7k_input();
        let binding = lane_binding_for_key_mode(&input, KeyMode::K7).unwrap();

        assert_eq!(
            binding.resolve(DeviceId(16), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
        assert_eq!(
            binding.resolve(DeviceId(17), &PhysicalControl::GamepadButton("Button1".into())),
            Some(Lane::Key1)
        );
    }

    #[test]
    fn default_fourteen_k_gamepad_uses_two_numbered_devices() {
        let bindings = default_play_14k_bindings();

        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad1"
                && entry.control == "Button1"
                && entry.lane == Some(LaneConfig::Key1)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad1"
                && entry.control == "Axis1+"
                && entry.lane == Some(LaneConfig::Scratch)
                && entry.scratch == Some(ScratchDirectionConfig::Up)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad1"
                && entry.control == "Axis1-"
                && entry.lane == Some(LaneConfig::Scratch)
                && entry.scratch == Some(ScratchDirectionConfig::Down)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad2"
                && entry.control == "Button1"
                && entry.lane == Some(LaneConfig::Key8)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad2"
                && entry.control == "Axis1-"
                && entry.lane == Some(LaneConfig::Scratch2)
                && entry.scratch == Some(ScratchDirectionConfig::Up)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad2"
                && entry.control == "Axis1+"
                && entry.lane == Some(LaneConfig::Scratch2)
                && entry.scratch == Some(ScratchDirectionConfig::Down)
        }));
        assert!(
            !bindings
                .iter()
                .any(|entry| { entry.device == "gamepad" && entry.control == "Button14" })
        );
    }

    #[test]
    fn scratchless_modes_resolve_seven_key_scratch_for_play_options_only() {
        let input = default_profile_input();
        let up = PhysicalControl::KeyboardKey("LShift".to_string());
        let down = PhysicalControl::KeyboardKey("LControl".to_string());

        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let gameplay = lane_binding_for_key_mode(&input, key_mode).unwrap();
            assert_eq!(gameplay.resolve(DeviceId(0), &up), None, "{}", key_mode.as_str());

            let options = lane_binding_for_play_option_scratch_with_slots(
                &input,
                key_mode,
                GamepadSlotMap::default(),
            )
            .unwrap();
            assert_eq!(options.resolve(DeviceId(0), &up), Some(Lane::Scratch));
            assert_eq!(options.resolve(DeviceId(0), &down), Some(Lane::Scratch));
            assert_eq!(
                options.resolve(DeviceId(42), &PhysicalControl::GamepadButton("Axis1+".into())),
                Some(Lane::Scratch),
            );
        }
    }

    #[test]
    fn mode_specific_scratch_binding_precedes_seven_key_option_fallback() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let mut input = default_profile_input();
            let mut bindings = default_play_bindings(key_mode);
            bindings.push(scratch_play_binding(
                "T",
                LaneConfig::Scratch,
                ScratchDirectionConfig::Up,
            ));
            input.play.insert(
                key_mode.play_map_key().to_string(),
                PlayModeInputConfig { inherit: None, bindings, ..Default::default() },
            );

            let options = lane_binding_for_play_option_scratch_with_slots(
                &input,
                key_mode,
                GamepadSlotMap::default(),
            )
            .unwrap();
            assert_eq!(
                options.resolve(DeviceId(0), &PhysicalControl::KeyboardKey("T".into())),
                Some(Lane::Scratch),
                "{}",
                key_mode.as_str(),
            );
            let gameplay = lane_binding_for_key_mode(&input, key_mode).unwrap();
            assert_eq!(
                gameplay.resolve(DeviceId(0), &PhysicalControl::KeyboardKey("T".into())),
                None,
                "{}",
                key_mode.as_str(),
            );
            assert_eq!(
                options.resolve(DeviceId(0), &PhysicalControl::KeyboardKey("LShift".into())),
                None,
                "{}",
                key_mode.as_str(),
            );
        }
    }

    #[test]
    fn hispeed_direction_table_matches_each_key_mode() {
        use HispeedDirectionConfig::{Down, Up};

        let cases = [
            (KeyMode::K4, vec![Lane::Key1, Lane::Key4], vec![Lane::Key2, Lane::Key3]),
            (KeyMode::K5, vec![Lane::Key1, Lane::Key3, Lane::Key5], vec![Lane::Key2, Lane::Key4]),
            (
                KeyMode::K6,
                vec![Lane::Key1, Lane::Key3, Lane::Key4, Lane::Key6],
                vec![Lane::Key2, Lane::Key5],
            ),
            (
                KeyMode::K7,
                vec![Lane::Key1, Lane::Key3, Lane::Key5, Lane::Key7],
                vec![Lane::Key2, Lane::Key4, Lane::Key6],
            ),
            (
                KeyMode::K8,
                vec![Lane::Key2, Lane::Key4, Lane::Key5, Lane::Key7],
                vec![Lane::Key1, Lane::Key3, Lane::Key6, Lane::Key8],
            ),
            (
                KeyMode::K9,
                vec![Lane::Key1, Lane::Key3, Lane::Key5, Lane::Key7, Lane::Key9],
                vec![Lane::Key2, Lane::Key4, Lane::Key6, Lane::Key8],
            ),
            (
                KeyMode::K10,
                vec![Lane::Key1, Lane::Key3, Lane::Key5, Lane::Key8, Lane::Key10, Lane::Key12],
                vec![Lane::Key2, Lane::Key4, Lane::Key9, Lane::Key11],
            ),
            (
                KeyMode::K14,
                vec![
                    Lane::Key1,
                    Lane::Key3,
                    Lane::Key5,
                    Lane::Key7,
                    Lane::Key8,
                    Lane::Key10,
                    Lane::Key12,
                    Lane::Key14,
                ],
                vec![Lane::Key2, Lane::Key4, Lane::Key6, Lane::Key9, Lane::Key11, Lane::Key13],
            ),
        ];

        for (key_mode, down, up) in cases {
            for lane in down {
                assert_eq!(
                    default_hispeed_direction_for_lane(key_mode, lane),
                    Some(Down),
                    "{} {lane:?}",
                    key_mode.as_str(),
                );
            }
            for lane in up {
                assert_eq!(
                    default_hispeed_direction_for_lane(key_mode, lane),
                    Some(Up),
                    "{} {lane:?}",
                    key_mode.as_str(),
                );
            }
            for &lane in key_mode.active_lanes() {
                if matches!(lane, Lane::Scratch | Lane::Scratch2) {
                    assert_eq!(default_hispeed_direction_for_lane(key_mode, lane), None);
                } else {
                    assert!(default_hispeed_direction_for_lane(key_mode, lane).is_some());
                }
            }
        }
    }

    #[test]
    fn eight_key_hispeed_override_only_persists_non_default_direction() {
        let mut input = default_profile_input();

        assert_eq!(
            hispeed_direction_for_lane(&input, KeyMode::K8, Lane::Key1),
            Some(HispeedDirectionConfig::Up),
        );
        assert!(set_eight_key_hispeed_direction(
            &mut input,
            LaneConfig::Key1,
            HispeedDirectionConfig::Down,
        ));
        assert_eq!(
            input.play[KeyMode::K8.play_map_key()].hispeed.get(&LaneConfig::Key1),
            Some(&HispeedDirectionConfig::Down),
        );
        assert!(set_eight_key_hispeed_direction(
            &mut input,
            LaneConfig::Key1,
            HispeedDirectionConfig::Up,
        ));
        assert!(input.play[KeyMode::K8.play_map_key()].hispeed.is_empty());
        assert!(!set_eight_key_hispeed_direction(
            &mut input,
            LaneConfig::Scratch,
            HispeedDirectionConfig::Down,
        ));
    }
}
