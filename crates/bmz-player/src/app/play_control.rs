use crate::config::profile_config::{InputActionConfig, ProfileInputConfig};
use bmz_gameplay::input::backend::PhysicalControl;

use super::input_runtime::ControlInputEvent;
use super::{LANE_COVER_REPEAT_STEP, LANE_COVER_STEP};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HispeedChange {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum PlayLaneAction {
    ToggleHispeedMode,
    Hispeed(HispeedChange),
    LaneCoverDelta(f32),
    AnalogLaneCoverDelta(f32),
    GreenNumberDelta(i32),
    ToggleLaneCoverVisibility,
    VisualOffsetDelta(i32),
    ToggleVisualOffsetAutoAdjust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayLaneTarget {
    Sudden,
    Lift,
    Hidden,
}

impl PlayLaneTarget {
    pub(super) const fn toggled_lift_hidden(self) -> Self {
        match self {
            Self::Hidden => Self::Lift,
            Self::Sudden | Self::Lift => Self::Hidden,
        }
    }
}

pub(super) fn resolved_play_lane_target(
    sudden_enabled: bool,
    lane_cover_visible: bool,
    lift_enabled: bool,
    hidden_enabled: bool,
    preferred: PlayLaneTarget,
) -> Option<PlayLaneTarget> {
    if sudden_enabled && lane_cover_visible {
        return Some(PlayLaneTarget::Sudden);
    }
    match (lift_enabled, hidden_enabled) {
        (true, true) => Some(match preferred {
            PlayLaneTarget::Hidden => PlayLaneTarget::Hidden,
            PlayLaneTarget::Sudden | PlayLaneTarget::Lift => PlayLaneTarget::Lift,
        }),
        (true, false) => Some(PlayLaneTarget::Lift),
        (false, true) => Some(PlayLaneTarget::Hidden),
        (false, false) => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayOptionControl {
    ToggleHispeedMode,
    Hispeed(HispeedChange),
    LaneCover(LaneCoverChange),
    GreenNumber(GreenNumberChange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayAnalogOptionMode {
    LaneCover,
    GreenNumber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaneCoverChange {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GreenNumberChange {
    Up,
    Down,
}

pub(super) fn keyboard_lane_action(
    event: &ControlInputEvent,
    input: &ProfileInputConfig,
) -> Option<PlayLaneAction> {
    if !event.pressed {
        return None;
    }
    let PhysicalControl::KeyboardKey(control) = event.physical.as_ref()? else {
        return None;
    };
    let lane_cover_step = if event.repeat { LANE_COVER_REPEAT_STEP } else { LANE_COVER_STEP };
    // 設定ファイル内の行順に依存させず、プレイ操作の固定順で同一キーを
    // 解決する。共通 E1〜E4 は呼び出し側で先に判定されるため、ここでは
    // プレイ用ショートカット同士の決定順だけを定義する。
    let action = crate::config::profile_config::PLAY_KEYBOARD_SHORTCUT_ACTIONS
        .iter()
        .copied()
        .find(|action| {
            input.ui.bindings.iter().any(|entry| {
                entry.device == "keyboard"
                    && entry.action == Some(*action)
                    && keyboard_controls_match(&entry.control, control)
            })
        })?;
    match action {
        InputActionConfig::PlayHispeedDown => Some(PlayLaneAction::Hispeed(HispeedChange::Down)),
        InputActionConfig::PlayHispeedUp => Some(PlayLaneAction::Hispeed(HispeedChange::Up)),
        InputActionConfig::PlayLaneCoverUp => Some(PlayLaneAction::LaneCoverDelta(lane_cover_step)),
        InputActionConfig::PlayLaneCoverDown => {
            Some(PlayLaneAction::LaneCoverDelta(-lane_cover_step))
        }
        InputActionConfig::PlayVisualOffsetUp => Some(PlayLaneAction::VisualOffsetDelta(1)),
        InputActionConfig::PlayVisualOffsetDown => Some(PlayLaneAction::VisualOffsetDelta(-1)),
        InputActionConfig::PlayVisualOffsetAutoAdjust => {
            (!event.repeat).then_some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
        }
        _ => None,
    }
}

fn keyboard_controls_match(configured: &str, pressed: &str) -> bool {
    configured == pressed
        || numeric_keypad_alias(configured) == Some(pressed)
        || numeric_keypad_alias(pressed) == Some(configured)
}

fn numeric_keypad_alias(control: &str) -> Option<&'static str> {
    match control {
        "0" => Some("Numpad0"),
        "1" => Some("Numpad1"),
        "2" => Some("Numpad2"),
        "3" => Some("Numpad3"),
        "4" => Some("Numpad4"),
        "5" => Some("Numpad5"),
        "6" => Some("Numpad6"),
        "7" => Some("Numpad7"),
        "8" => Some("Numpad8"),
        "9" => Some("Numpad9"),
        "Numpad0" => Some("0"),
        "Numpad1" => Some("1"),
        "Numpad2" => Some("2"),
        "Numpad3" => Some("3"),
        "Numpad4" => Some("4"),
        "Numpad5" => Some("5"),
        "Numpad6" => Some("6"),
        "Numpad7" => Some("7"),
        "Numpad8" => Some("8"),
        "Numpad9" => Some("9"),
        _ => None,
    }
}

pub(super) fn lane_action_from_option(
    action: PlayOptionControl,
    is_axis: bool,
) -> Option<PlayLaneAction> {
    match action {
        PlayOptionControl::ToggleHispeedMode => Some(PlayLaneAction::ToggleHispeedMode),
        PlayOptionControl::Hispeed(change) => Some(PlayLaneAction::Hispeed(change)),
        PlayOptionControl::LaneCover(_) if is_axis => None,
        PlayOptionControl::LaneCover(LaneCoverChange::Up) => {
            Some(PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP))
        }
        PlayOptionControl::LaneCover(LaneCoverChange::Down) => {
            Some(PlayLaneAction::LaneCoverDelta(-LANE_COVER_STEP))
        }
        PlayOptionControl::GreenNumber(_) if is_axis => None,
        PlayOptionControl::GreenNumber(GreenNumberChange::Up) => {
            Some(PlayLaneAction::GreenNumberDelta(1))
        }
        PlayOptionControl::GreenNumber(GreenNumberChange::Down) => {
            Some(PlayLaneAction::GreenNumberDelta(-1))
        }
    }
}

#[cfg(test)]
mod tests {
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};

    use super::*;

    fn keyboard(code: KeyCode, repeat: bool) -> ControlInputEvent {
        ControlInputEvent::keyboard_parts(PhysicalKey::Code(code), ElementState::Pressed, repeat)
    }

    #[test]
    fn keyboard_arrows_map_to_shared_lane_actions() {
        let input = crate::config::play_input::default_profile_input();
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::ArrowLeft, false), &input),
            Some(PlayLaneAction::Hispeed(HispeedChange::Down))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::ArrowUp, false), &input),
            Some(PlayLaneAction::LaneCoverDelta(LANE_COVER_STEP))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::ArrowDown, true), &input),
            Some(PlayLaneAction::LaneCoverDelta(-LANE_COVER_REPEAT_STEP))
        );
    }

    #[test]
    fn remapped_and_cleared_shortcuts_survive_profile_reload() {
        use crate::config::key_config::{
            KeyBindingSlot, KeyBindingTarget, apply_play_binding, clear_play_binding,
        };
        use bmz_core::lane::KeyMode;
        let mut input = crate::config::play_input::default_profile_input();
        for (action, old_key, new_key, control, expected) in [
            (
                InputActionConfig::PlayHispeedDown,
                KeyCode::ArrowLeft,
                KeyCode::KeyH,
                "H",
                PlayLaneAction::Hispeed(HispeedChange::Down),
            ),
            (
                InputActionConfig::PlayHispeedUp,
                KeyCode::ArrowRight,
                KeyCode::KeyJ,
                "J",
                PlayLaneAction::Hispeed(HispeedChange::Up),
            ),
            (
                InputActionConfig::PlayLaneCoverUp,
                KeyCode::ArrowUp,
                KeyCode::KeyK,
                "K",
                PlayLaneAction::LaneCoverDelta(LANE_COVER_REPEAT_STEP),
            ),
            (
                InputActionConfig::PlayLaneCoverDown,
                KeyCode::ArrowDown,
                KeyCode::KeyL,
                "L",
                PlayLaneAction::LaneCoverDelta(-LANE_COVER_REPEAT_STEP),
            ),
        ] {
            let target = KeyBindingTarget::Action { action, slot: KeyBindingSlot::KeyboardPrimary };
            apply_play_binding(&mut input, KeyMode::K7, target, control).unwrap();
            input = toml::from_str(&toml::to_string(&input).unwrap()).unwrap();
            crate::config::play_input::normalize_profile_input(&mut input);
            assert_eq!(keyboard_lane_action(&keyboard(old_key, false), &input), None);
            assert_eq!(keyboard_lane_action(&keyboard(new_key, true), &input), Some(expected));
            let mut release = keyboard(new_key, false);
            release.pressed = false;
            assert_eq!(keyboard_lane_action(&release, &input), None);
            clear_play_binding(&mut input, KeyMode::K7, target).unwrap();
            input = toml::from_str(&toml::to_string(&input).unwrap()).unwrap();
            crate::config::play_input::normalize_profile_input(&mut input);
            assert_eq!(keyboard_lane_action(&keyboard(new_key, false), &input), None);
            assert_eq!(keyboard_lane_action(&keyboard(old_key, false), &input), None);
        }
    }

    #[test]
    fn visual_offset_shortcuts_accept_numeric_keypad_aliases() {
        let input = crate::config::play_input::default_profile_input();
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Digit3, false), &input),
            Some(PlayLaneAction::VisualOffsetDelta(1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad3, false), &input),
            Some(PlayLaneAction::VisualOffsetDelta(1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad9, false), &input),
            Some(PlayLaneAction::VisualOffsetDelta(-1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad0, false), &input),
            Some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
        );
    }

    #[test]
    fn visual_offset_auto_adjust_toggles_only_on_press_edges() {
        let input = crate::config::play_input::default_profile_input();
        for code in [KeyCode::Digit0, KeyCode::Numpad0] {
            assert_eq!(
                keyboard_lane_action(&keyboard(code, false), &input),
                Some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
            );
            assert_eq!(keyboard_lane_action(&keyboard(code, true), &input), None);
            let mut release = keyboard(code, false);
            release.pressed = false;
            assert_eq!(keyboard_lane_action(&release, &input), None);
        }

        use crate::config::key_config::{KeyBindingSlot, KeyBindingTarget, apply_play_binding};
        use bmz_core::lane::KeyMode;
        let mut remapped = input.clone();
        apply_play_binding(
            &mut remapped,
            KeyMode::K7,
            KeyBindingTarget::Action {
                action: InputActionConfig::PlayVisualOffsetAutoAdjust,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
            "H",
        )
        .unwrap();
        crate::config::play_input::normalize_profile_input(&mut remapped);
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::KeyH, false), &remapped),
            Some(PlayLaneAction::ToggleVisualOffsetAutoAdjust)
        );
        assert_eq!(keyboard_lane_action(&keyboard(KeyCode::KeyH, true), &remapped), None);
        let mut release = keyboard(KeyCode::KeyH, false);
        release.pressed = false;
        assert_eq!(keyboard_lane_action(&release, &remapped), None);

        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Digit3, true), &remapped),
            Some(PlayLaneAction::VisualOffsetDelta(1))
        );
        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::Numpad9, true), &remapped),
            Some(PlayLaneAction::VisualOffsetDelta(-1))
        );
    }

    #[test]
    fn duplicate_play_shortcuts_use_fixed_action_order() {
        use crate::config::key_config::{KeyBindingSlot, KeyBindingTarget, apply_play_binding};
        use bmz_core::lane::KeyMode;

        let mut input = crate::config::play_input::default_profile_input();
        apply_play_binding(
            &mut input,
            KeyMode::K7,
            KeyBindingTarget::Action {
                action: InputActionConfig::PlayVisualOffsetUp,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
            "H",
        )
        .unwrap();
        apply_play_binding(
            &mut input,
            KeyMode::K7,
            KeyBindingTarget::Action {
                action: InputActionConfig::PlayHispeedUp,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
            "H",
        )
        .unwrap();

        assert_eq!(
            keyboard_lane_action(&keyboard(KeyCode::KeyH, false), &input),
            Some(PlayLaneAction::Hispeed(HispeedChange::Up))
        );
    }

    #[test]
    fn axis_button_events_do_not_duplicate_analog_lane_changes() {
        assert_eq!(
            lane_action_from_option(PlayOptionControl::LaneCover(LaneCoverChange::Up), true,),
            None
        );
        assert_eq!(
            lane_action_from_option(PlayOptionControl::GreenNumber(GreenNumberChange::Down), true,),
            None
        );
        assert_eq!(
            lane_action_from_option(PlayOptionControl::Hispeed(HispeedChange::Up), true,),
            Some(PlayLaneAction::Hispeed(HispeedChange::Up))
        );
    }
}
