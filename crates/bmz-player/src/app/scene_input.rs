use bmz_gameplay::input::backend::PhysicalControl;

use super::input_runtime::ControlInputEvent;
use super::select_key_bindings::SelectKeyBindings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectAction {
    EnterOrPlay,
    ExitFolder,
    OpenFolder,
    Reload,
    AutoplayFolder,
    OpenPrimaryIr,
    OpenKeyConfig,
    CycleRival,
    OpenDocuments,
    FavoriteSong,
    FavoriteChart,
    SameFolder,
    ModeFilter,
    Sort,
    LnMode,
    DifficultyFilter,
    ReplayCycle,
    ReplayPlay,
    Move(SelectMove),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectMove {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultAction {
    Retry,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecideAction {
    Confirm,
    Cancel,
}

pub(super) fn select_action(
    event: &ControlInputEvent,
    bindings: &SelectKeyBindings,
) -> Option<SelectAction> {
    if !event.pressed || event.repeat || event.uses_analog_scroll(bindings) {
        return None;
    }
    let control = event.name.as_deref()?;

    match event.physical.as_ref()? {
        PhysicalControl::KeyboardKey(_) => keyboard_select_action(control, bindings),
        PhysicalControl::GamepadButton(_) => gamepad_select_action(control, bindings),
        PhysicalControl::HidButton(_) => None,
    }
}

fn keyboard_select_action(control: &str, bindings: &SelectKeyBindings) -> Option<SelectAction> {
    if let Some(action) = configurable_select_shortcut_action(control, bindings) {
        return Some(action);
    }
    let fixed = match control {
        "Enter" | "Space" | "ArrowRight" => Some(SelectAction::EnterOrPlay),
        "ArrowLeft" => Some(SelectAction::ExitFolder),
        "ArrowUp" => Some(SelectAction::Move(SelectMove::Previous)),
        "ArrowDown" => Some(SelectAction::Move(SelectMove::Next)),
        "PageUp" => Some(SelectAction::Move(SelectMove::PagePrevious)),
        "PageDown" => Some(SelectAction::Move(SelectMove::PageNext)),
        "Home" => Some(SelectAction::Move(SelectMove::First)),
        "End" => Some(SelectAction::Move(SelectMove::Last)),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }

    if bindings.is_enter(control) {
        Some(SelectAction::EnterOrPlay)
    } else if bindings.is_back(control) {
        Some(SelectAction::ExitFolder)
    } else if bindings.is_favorite_song(control) {
        Some(SelectAction::FavoriteSong)
    } else if bindings.is_favorite_chart(control) {
        Some(SelectAction::FavoriteChart)
    } else if bindings.is_same_folder(control) {
        Some(SelectAction::SameFolder)
    } else if bindings.is_mode_filter(control) {
        Some(SelectAction::ModeFilter)
    } else if bindings.is_sort(control) {
        Some(SelectAction::Sort)
    } else if bindings.is_ln_mode(control) {
        Some(SelectAction::LnMode)
    } else if bindings.is_difficulty_filter(control) {
        Some(SelectAction::DifficultyFilter)
    } else if bindings.is_replay_cycle(control) {
        Some(SelectAction::ReplayCycle)
    } else if bindings.is_replay_play(control) {
        Some(SelectAction::ReplayPlay)
    } else if bindings.is_select_scratch_down(control) {
        Some(SelectAction::Move(SelectMove::Next))
    } else if bindings.is_select_scratch_up(control) {
        Some(SelectAction::Move(SelectMove::Previous))
    } else if bindings.is_select_previous(control) {
        if bindings.is_select_next(control) {
            Some(SelectAction::Move(SelectMove::Next))
        } else {
            Some(SelectAction::Move(SelectMove::Previous))
        }
    } else if bindings.is_select_next(control) {
        Some(SelectAction::Move(SelectMove::Next))
    } else {
        None
    }
}

fn gamepad_select_action(control: &str, bindings: &SelectKeyBindings) -> Option<SelectAction> {
    if let Some(action) = configurable_select_shortcut_action(control, bindings) {
        return Some(action);
    }
    let fixed = match control {
        "DPadUp" => Some(SelectAction::Move(SelectMove::Previous)),
        "DPadDown" => Some(SelectAction::Move(SelectMove::Next)),
        "Select" => Some(SelectAction::ExitFolder),
        "Button1" => Some(SelectAction::EnterOrPlay),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }

    if bindings.is_select_scratch_up(control) {
        if bindings.is_select_scratch_down(control) {
            Some(SelectAction::Move(SelectMove::Next))
        } else {
            Some(SelectAction::Move(SelectMove::Previous))
        }
    } else if bindings.is_select_scratch_down(control) {
        Some(SelectAction::Move(SelectMove::Next))
    } else if bindings.is_enter(control) {
        Some(SelectAction::EnterOrPlay)
    } else if bindings.is_back(control) {
        Some(SelectAction::ExitFolder)
    } else if bindings.is_favorite_song(control) {
        Some(SelectAction::FavoriteSong)
    } else if bindings.is_favorite_chart(control) {
        Some(SelectAction::FavoriteChart)
    } else if bindings.is_same_folder(control) {
        Some(SelectAction::SameFolder)
    } else if bindings.is_mode_filter(control) {
        Some(SelectAction::ModeFilter)
    } else if bindings.is_sort(control) {
        Some(SelectAction::Sort)
    } else if bindings.is_ln_mode(control) {
        Some(SelectAction::LnMode)
    } else if bindings.is_difficulty_filter(control) {
        Some(SelectAction::DifficultyFilter)
    } else if bindings.is_replay_cycle(control) {
        Some(SelectAction::ReplayCycle)
    } else if bindings.is_replay_play(control) {
        Some(SelectAction::ReplayPlay)
    } else {
        None
    }
}

pub(super) fn configurable_select_shortcut_action(
    control: &str,
    bindings: &SelectKeyBindings,
) -> Option<SelectAction> {
    if bindings.is_open_folder(control) {
        Some(SelectAction::OpenFolder)
    } else if bindings.is_reload(control) {
        Some(SelectAction::Reload)
    } else if bindings.is_autoplay_folder(control) {
        Some(SelectAction::AutoplayFolder)
    } else if bindings.is_open_ir(control) {
        Some(SelectAction::OpenPrimaryIr)
    } else if bindings.is_open_key_config(control) {
        Some(SelectAction::OpenKeyConfig)
    } else if bindings.is_rival_cycle(control) {
        Some(SelectAction::CycleRival)
    } else if bindings.is_open_documents(control) {
        Some(SelectAction::OpenDocuments)
    } else {
        None
    }
}

pub(super) fn decide_action(
    event: &ControlInputEvent,
    bindings: &SelectKeyBindings,
) -> Option<DecideAction> {
    if !event.pressed || event.repeat {
        return None;
    }
    let control = event.name.as_deref()?;
    match event.physical.as_ref()? {
        PhysicalControl::KeyboardKey(_) => match control {
            "Enter" | "Space" => Some(DecideAction::Confirm),
            "Escape" => Some(DecideAction::Cancel),
            _ => bindings.is_enter(control).then_some(DecideAction::Confirm),
        },
        PhysicalControl::GamepadButton(_) => {
            if control == "Button1" {
                Some(DecideAction::Confirm)
            } else {
                bindings.is_enter(control).then_some(DecideAction::Confirm)
            }
        }
        PhysicalControl::HidButton(_) => None,
    }
}

pub(super) fn result_action(event: &ControlInputEvent) -> Option<ResultAction> {
    if !event.pressed || event.repeat {
        return None;
    }
    let control = event.name.as_deref()?;
    match event.physical.as_ref()? {
        PhysicalControl::KeyboardKey(_) => match control {
            "R" => Some(ResultAction::Retry),
            "Enter" | "Escape" => Some(ResultAction::Leave),
            _ => None,
        },
        PhysicalControl::GamepadButton(_) => match control {
            "Button1" | "Start" => Some(ResultAction::Retry),
            "Button2" | "Select" => Some(ResultAction::Leave),
            _ => None,
        },
        PhysicalControl::HidButton(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use bmz_gameplay::input::backend::DeviceId;
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};

    use super::*;
    use crate::config::profile_config::ProfileConfig;

    fn keyboard(code: KeyCode) -> ControlInputEvent {
        ControlInputEvent::keyboard_parts(PhysicalKey::Code(code), ElementState::Pressed, false)
    }

    fn gamepad(control: &str) -> ControlInputEvent {
        ControlInputEvent::gamepad(DeviceId(1), control, true)
    }

    fn bindings() -> SelectKeyBindings {
        SelectKeyBindings::from_profile(&ProfileConfig::new_default("default", "Default", 1).input)
    }

    #[test]
    fn analog_scratch_moves_once_from_ticks_and_digital_axes_keep_button_navigation() {
        use crate::app::input_runtime::AppInputRuntime;
        use crate::app::select_key_bindings::{
            select_analog_scroll_delta, take_analog_scroll_steps,
        };
        use crate::input::gamepad::GamepadButtonEvent;
        use bmz_gameplay::input::backend::DeviceTimestamp;

        for axis in ["Axis1", "GCAxisLeftX", "FutureBackendAxis"] {
            let mut profile = ProfileConfig::new_default("test", "Test", 0);
            for binding in
                profile.input.ui.bindings.iter_mut().chain(
                    profile.input.play.values_mut().flat_map(|mode| mode.bindings.iter_mut()),
                )
            {
                if let Some(suffix) = binding.control.strip_prefix("Axis1") {
                    binding.control = format!("{axis}{suffix}");
                }
            }
            let keys = SelectKeyBindings::from_profile(&profile.input);
            let mut button = GamepadButtonEvent {
                name: format!("{axis}+"),
                device_id: DeviceId(16),
                pressed: true,
                timestamp: DeviceTimestamp::MonotonicNs(1),
                synthesized_analog_axis: true,
            };
            let event = ControlInputEvent::gamepad_button(&button);
            let mut runtime = AppInputRuntime::default();
            runtime.track_control(&event);
            assert!(runtime.pressed_controls.contains(&button.name));
            assert_eq!(select_action(&event, &keys), None, "{axis}: no extra button step");

            let mut buffer = select_analog_scroll_delta(axis, 2, &keys).unwrap();
            assert_eq!(take_analog_scroll_steps(&mut buffer, 3), 0);
            buffer += select_analog_scroll_delta(axis, 1, &keys).unwrap();
            assert_eq!(take_analog_scroll_steps(&mut buffer, 3), -1);
            assert_eq!(buffer, 0);

            button.pressed = false;
            let release = ControlInputEvent::gamepad_button(&button);
            runtime.track_control(&release);
            assert!(!runtime.pressed_controls.contains(&button.name));
            assert_eq!(select_action(&release, &keys), None);

            // Analog scratch OFF emits endpoint buttons without any axis ticks.
            button.pressed = true;
            button.synthesized_analog_axis = false;
            assert_eq!(
                select_action(&ControlInputEvent::gamepad_button(&button), &keys),
                Some(SelectAction::Move(SelectMove::Previous)),
                "{axis}: digital axis must remain usable"
            );
        }
    }

    #[test]
    fn select_actions_normalize_keyboard_and_gamepad_navigation() {
        let bindings = bindings();

        assert_eq!(
            select_action(&keyboard(KeyCode::ArrowUp), &bindings),
            Some(SelectAction::Move(SelectMove::Previous))
        );
        assert_eq!(
            select_action(&gamepad("DPadUp"), &bindings),
            Some(SelectAction::Move(SelectMove::Previous))
        );
        assert_eq!(
            select_action(&keyboard(KeyCode::Enter), &bindings),
            Some(SelectAction::EnterOrPlay)
        );
        assert_eq!(select_action(&gamepad("Button1"), &bindings), Some(SelectAction::EnterOrPlay));
        assert_eq!(select_action(&gamepad("ArrowUp"), &bindings), None);
    }

    #[test]
    fn decide_and_result_actions_normalize_keyboard_and_gamepad_controls() {
        let bindings = bindings();

        assert_eq!(
            decide_action(&keyboard(KeyCode::Enter), &bindings),
            Some(DecideAction::Confirm)
        );
        assert_eq!(decide_action(&gamepad("Button1"), &bindings), Some(DecideAction::Confirm));
        assert_eq!(result_action(&keyboard(KeyCode::KeyR)), Some(ResultAction::Retry));
        assert_eq!(result_action(&gamepad("Start")), Some(ResultAction::Retry));
        assert_eq!(result_action(&keyboard(KeyCode::Escape)), Some(ResultAction::Leave));
        assert_eq!(result_action(&gamepad("Select")), Some(ResultAction::Leave));
        assert_eq!(decide_action(&gamepad("Escape"), &bindings), None);
        assert_eq!(result_action(&gamepad("R")), None);
    }
}
