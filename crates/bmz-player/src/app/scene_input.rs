use super::input_resolve::{ResolvedControl, ResolvedInput, StandardKey};
use crate::config::profile_config::InputActionConfig;

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
    /// 9K KEY1 / KEY9: five rows at once, animated like five quick presses.
    JumpPrevious,
    JumpNext,
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

/// 選曲画面の単発操作。解決済み入力だけから決める。
pub(super) fn select_action(event: &ResolvedInput) -> Option<SelectAction> {
    if !event.pressed() || event.repeat() || event.uses_analog_scroll() {
        return None;
    }
    let control = event.scene();
    if event.is_keyboard() {
        keyboard_select_action(control)
    } else if event.is_controller() {
        controller_select_action(control)
    } else {
        None
    }
}

fn keyboard_select_action(control: &ResolvedControl) -> Option<SelectAction> {
    if let Some(action) = configurable_select_shortcut_action(control) {
        return Some(action);
    }
    let standard = match control.standard() {
        Some(StandardKey::Enter | StandardKey::Space | StandardKey::ArrowRight) => {
            Some(SelectAction::EnterOrPlay)
        }
        Some(StandardKey::ArrowLeft) => Some(SelectAction::ExitFolder),
        Some(StandardKey::ArrowUp) => Some(SelectAction::Move(SelectMove::Previous)),
        Some(StandardKey::ArrowDown) => Some(SelectAction::Move(SelectMove::Next)),
        Some(StandardKey::PageUp) => Some(SelectAction::Move(SelectMove::PagePrevious)),
        Some(StandardKey::PageDown) => Some(SelectAction::Move(SelectMove::PageNext)),
        Some(StandardKey::Home) => Some(SelectAction::Move(SelectMove::First)),
        Some(StandardKey::End) => Some(SelectAction::Move(SelectMove::Last)),
        _ => None,
    };
    if standard.is_some() {
        return standard;
    }
    if control.is_enter() {
        Some(SelectAction::EnterOrPlay)
    } else if control.is_back() {
        Some(SelectAction::ExitFolder)
    } else if let Some(action) = select_list_shortcut_action(control) {
        Some(action)
    } else {
        select_move_action(control)
    }
}

fn controller_select_action(control: &ResolvedControl) -> Option<SelectAction> {
    // コントローラーのボタンに固定の意味はなく、すべてキーコンフィグに従う。
    if let Some(action) = configurable_select_shortcut_action(control) {
        return Some(action);
    }
    if control.is_scratch_down() {
        Some(SelectAction::Move(SelectMove::Next))
    } else if control.is_scratch_up() {
        Some(SelectAction::Move(SelectMove::Previous))
    } else if control.is_enter() {
        Some(SelectAction::EnterOrPlay)
    } else if control.is_back() {
        Some(SelectAction::ExitFolder)
    } else if let Some(action) = select_list_shortcut_action(control) {
        Some(action)
    } else {
        select_move_action(control)
    }
}

fn select_list_shortcut_action(control: &ResolvedControl) -> Option<SelectAction> {
    const ORDER: &[(InputActionConfig, SelectAction)] = &[
        (InputActionConfig::SelectFavoriteSong, SelectAction::FavoriteSong),
        (InputActionConfig::SelectFavoriteChart, SelectAction::FavoriteChart),
        (InputActionConfig::SelectSameFolder, SelectAction::SameFolder),
        (InputActionConfig::SelectModeFilter, SelectAction::ModeFilter),
        (InputActionConfig::SelectSort, SelectAction::Sort),
        (InputActionConfig::SelectLnMode, SelectAction::LnMode),
        (InputActionConfig::SelectDifficultyFilter, SelectAction::DifficultyFilter),
        (InputActionConfig::SelectReplayCycle, SelectAction::ReplayCycle),
        (InputActionConfig::SelectReplayPlay, SelectAction::ReplayPlay),
    ];
    ORDER.iter().find_map(|(config, action)| control.has_shortcut(*config).then_some(*action))
}

fn select_move_action(control: &ResolvedControl) -> Option<SelectAction> {
    if control.is_scratch_down() {
        return Some(SelectAction::Move(SelectMove::Next));
    }
    if control.is_scratch_up() {
        return Some(SelectAction::Move(SelectMove::Previous));
    }
    match control.nine_key_move() {
        Some(-1) => return Some(SelectAction::Move(SelectMove::Previous)),
        Some(_) => return Some(SelectAction::Move(SelectMove::Next)),
        None => {}
    }
    match control.nine_key_jump()? {
        -1 => Some(SelectAction::Move(SelectMove::JumpPrevious)),
        _ => Some(SelectAction::Move(SelectMove::JumpNext)),
    }
}

pub(super) fn configurable_select_shortcut_action(
    control: &ResolvedControl,
) -> Option<SelectAction> {
    const ORDER: &[(InputActionConfig, SelectAction)] = &[
        (InputActionConfig::SelectOpenFolder, SelectAction::OpenFolder),
        (InputActionConfig::SelectReload, SelectAction::Reload),
        (InputActionConfig::SelectAutoplayFolder, SelectAction::AutoplayFolder),
        (InputActionConfig::SelectOpenIr, SelectAction::OpenPrimaryIr),
        (InputActionConfig::SelectOpenKeyConfig, SelectAction::OpenKeyConfig),
        (InputActionConfig::SelectRivalCycle, SelectAction::CycleRival),
        (InputActionConfig::SelectOpenDocuments, SelectAction::OpenDocuments),
    ];
    ORDER.iter().find_map(|(config, action)| control.has_shortcut(*config).then_some(*action))
}

pub(super) fn decide_action(event: &ResolvedInput) -> Option<DecideAction> {
    if !event.pressed() || event.repeat() {
        return None;
    }
    let control = event.scene();
    if event.is_keyboard() {
        match control.standard() {
            Some(StandardKey::Enter | StandardKey::Space) => return Some(DecideAction::Confirm),
            Some(StandardKey::Escape) => return Some(DecideAction::Cancel),
            _ => {}
        }
    } else if !event.is_controller() {
        return None;
    }
    control.is_enter().then_some(DecideAction::Confirm)
}

pub(super) fn result_action(event: &ResolvedInput) -> Option<ResultAction> {
    if !event.pressed() || event.repeat() || !event.is_keyboard() {
        // Controllers leave or retry through their lane bindings only.
        return None;
    }
    match event.scene().standard()? {
        StandardKey::Retry => Some(ResultAction::Retry),
        StandardKey::Enter | StandardKey::Escape => Some(ResultAction::Leave),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bmz_gameplay::input::backend::DeviceId;
    use winit::event::ElementState;
    use winit::keyboard::{KeyCode, PhysicalKey};

    use super::*;
    use crate::app::input_resolve::{
        SceneInputResolver, resolve_axis_tick, resolve_scene_for_test,
    };
    use crate::app::input_runtime::ControlInputEvent;
    use crate::config::profile_config::ProfileConfig;
    use crate::input::gamepad::GamepadSlotMap;

    fn keyboard(code: KeyCode) -> ControlInputEvent {
        ControlInputEvent::keyboard_parts(PhysicalKey::Code(code), ElementState::Pressed, false)
    }

    fn gamepad(control: &str) -> ControlInputEvent {
        ControlInputEvent::gamepad(DeviceId(1), control, true)
    }

    fn bindings() -> SceneInputResolver {
        SceneInputResolver::from_profile(&ProfileConfig::new_default("default", "Default", 1).input)
    }

    fn select(event: ControlInputEvent, keys: &SceneInputResolver) -> Option<SelectAction> {
        select_action(&resolve_scene_for_test(keys, event))
    }

    fn decide(event: ControlInputEvent, keys: &SceneInputResolver) -> Option<DecideAction> {
        decide_action(&resolve_scene_for_test(keys, event))
    }

    fn result(event: ControlInputEvent) -> Option<ResultAction> {
        result_action(&resolve_scene_for_test(&bindings(), event))
    }

    #[test]
    fn analog_scratch_moves_once_from_ticks_and_digital_axes_keep_button_navigation() {
        use crate::app::input_runtime::AppInputRuntime;
        use crate::app::select_key_bindings::take_analog_scroll_steps;
        use crate::input::gamepad::GamepadButtonEvent;
        use bmz_gameplay::input::backend::{DeviceTimestamp, PhysicalControl};

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
            let keys = SceneInputResolver::from_profile(&profile.input);
            let device = DeviceId(16);
            let mut button = GamepadButtonEvent {
                name: format!("{axis}+"),
                device_id: device,
                pressed: true,
                timestamp: DeviceTimestamp::MonotonicNs(1),
                synthesized_analog_axis: true,
            };
            let control = PhysicalControl::GamepadButton(button.name.clone());
            let event = ControlInputEvent::gamepad_button(&button);
            let mut runtime = AppInputRuntime::default();
            runtime.track_control(&event);
            assert!(runtime.source_held(device, &control));
            assert_eq!(select(event, &keys), None, "{axis}: no extra button step");

            let tick = |ticks| {
                resolve_axis_tick(&keys, None, GamepadSlotMap::default(), device, axis, ticks)
                    .scene_scroll
                    .unwrap()
            };
            let mut buffer = tick(2);
            assert_eq!(take_analog_scroll_steps(&mut buffer, 3), 0);
            buffer += tick(1);
            assert_eq!(take_analog_scroll_steps(&mut buffer, 3), -1);
            assert_eq!(buffer, 0);

            button.pressed = false;
            let release = ControlInputEvent::gamepad_button(&button);
            runtime.track_control(&release);
            assert!(!runtime.source_held(device, &control));
            assert_eq!(select(release, &keys), None);

            // Analog scratch OFF emits endpoint buttons without any axis ticks.
            button.pressed = true;
            button.synthesized_analog_axis = false;
            assert_eq!(
                select(ControlInputEvent::gamepad_button(&button), &keys),
                Some(SelectAction::Move(SelectMove::Previous)),
                "{axis}: digital axis must remain usable"
            );
        }
    }

    #[test]
    fn select_actions_normalize_keyboard_and_gamepad_navigation() {
        let bindings = bindings();

        assert_eq!(
            select(keyboard(KeyCode::ArrowUp), &bindings),
            Some(SelectAction::Move(SelectMove::Previous))
        );
        // No backend names a controller button like this; it has no fixed meaning.
        assert_eq!(select(gamepad("DPadUp"), &bindings), None);
        assert_eq!(select(keyboard(KeyCode::Enter), &bindings), Some(SelectAction::EnterOrPlay));
        // Button1 confirms through its default KEY1 binding, not a fixed button name.
        assert_eq!(select(gamepad("Button1"), &bindings), Some(SelectAction::EnterOrPlay));
        assert_eq!(select(gamepad("ArrowUp"), &bindings), None);
    }

    #[test]
    fn gamepad_button1_has_no_fixed_select_decide_or_result_action() {
        // Default 9K select bindings are keyboard-only, so Button1 is unbound.
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.input.select_input_mode =
            crate::config::profile_config::SelectInputModeConfig::Key9;
        let keys = SceneInputResolver::from_profile(&profile.input);

        assert_eq!(select(gamepad("Button1"), &keys), None);
        assert_eq!(decide(gamepad("Button1"), &keys), None);
        assert_eq!(result(gamepad("Button1")), None);
    }

    #[test]
    fn decide_and_result_actions_normalize_keyboard_and_gamepad_controls() {
        let bindings = bindings();

        assert_eq!(decide(keyboard(KeyCode::Enter), &bindings), Some(DecideAction::Confirm));
        assert_eq!(decide(gamepad("Button1"), &bindings), Some(DecideAction::Confirm));
        assert_eq!(result(keyboard(KeyCode::KeyR)), Some(ResultAction::Retry));
        assert_eq!(result(gamepad("Start")), None);
        assert_eq!(result(keyboard(KeyCode::Escape)), Some(ResultAction::Leave));
        assert_eq!(result(gamepad("Select")), None);
        assert_eq!(result(gamepad("Button2")), None);
        assert_eq!(decide(gamepad("Escape"), &bindings), None);
        assert_eq!(result(gamepad("R")), None);
    }
}
