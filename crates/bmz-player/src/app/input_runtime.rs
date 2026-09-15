use std::collections::HashSet;

use bmz_gameplay::input::backend::{DeviceId, DeviceInputEvent, PhysicalControl};
use bmz_gameplay::input::bounce::{InputBounceConfig, InputBounceFilter};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::PhysicalKey;

use crate::config::profile_config::InputActionConfig;
use crate::input::gamepad::GamepadPressedButton;
use crate::input::winit::{
    W_KEYBOARD_DEVICE_ID, physical_key_to_control, physical_key_to_device_input,
};

/// keyboard/gamepadに依存しないapp controlの押下・解放イベント。
pub(super) struct ControlInputEvent {
    pub(super) device: DeviceId,
    pub(super) name: Option<String>,
    pub(super) physical: Option<PhysicalControl>,
    pub(super) pressed: bool,
    pub(super) repeat: bool,
}

impl ControlInputEvent {
    pub(super) fn keyboard(event: &KeyEvent) -> Self {
        Self::keyboard_parts(event.physical_key, event.state, event.repeat)
    }

    pub(super) fn keyboard_parts(
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> Self {
        let physical = physical_key_to_control(physical_key);
        let name = match physical.as_ref() {
            Some(PhysicalControl::KeyboardKey(name)) => Some(name.clone()),
            _ => None,
        };
        Self {
            device: W_KEYBOARD_DEVICE_ID,
            name,
            physical,
            pressed: state == ElementState::Pressed,
            repeat,
        }
    }

    pub(super) fn gamepad(device: DeviceId, name: &str, pressed: bool) -> Self {
        Self {
            device,
            name: Some(name.to_string()),
            physical: Some(PhysicalControl::GamepadButton(name.to_string())),
            pressed,
            repeat: false,
        }
    }
}

/// app全体で共有する押下集合とkeyboard bounce状態。
#[derive(Default)]
pub(super) struct AppInputRuntime {
    pub(super) start_held: bool,
    pub(super) select_held: bool,
    pub(super) select_e_action_holds: HashSet<InputActionConfig>,
    pub(super) pressed_controls: HashSet<String>,
    pressed_control_sources: HashSet<(DeviceId, String)>,
    pub(super) pressed_play_inputs: HashSet<(DeviceId, PhysicalControl)>,
    raw_input_pressed_keys: HashSet<PhysicalKey>,
    window_input_pressed_keys: HashSet<PhysicalKey>,
    app_bounce_filter: InputBounceFilter,
    raw_bounce_filter: InputBounceFilter,
    discard_gamepad_output_until_resynced: bool,
}

pub(super) struct InputReleaseBatch {
    pub(super) raw_keyboard: Vec<DeviceInputEvent>,
    pub(super) window_keyboard: Vec<DeviceInputEvent>,
}

impl AppInputRuntime {
    /// Windows may mark the other Shift's first Press as a repeat because
    /// both keys share VK_SHIFT. Only an already held physical key can repeat.
    pub(super) fn keyboard_repeat(&self, key: PhysicalKey, repeat: bool) -> bool {
        repeat
            && physical_key_to_control(key).is_some_and(|control| {
                self.pressed_play_inputs.contains(&(W_KEYBOARD_DEVICE_ID, control))
            })
    }

    pub(super) fn reconcile_keyboard_releases(
        &mut self,
        released_keys: &[PhysicalKey],
    ) -> InputReleaseBatch {
        let mut releases =
            InputReleaseBatch { raw_keyboard: Vec::new(), window_keyboard: Vec::new() };
        for &key in released_keys {
            self.track_control(&ControlInputEvent::keyboard_parts(
                key,
                ElementState::Released,
                false,
            ));
            if self.raw_input_pressed_keys.remove(&key)
                && let Some(event) =
                    physical_key_to_device_input(key, ElementState::Released, false)
            {
                releases.raw_keyboard.push(event);
            }
            if self.window_input_pressed_keys.remove(&key)
                && let Some(event) =
                    physical_key_to_device_input(key, ElementState::Released, false)
            {
                releases.window_keyboard.push(event);
            }
        }
        releases
    }

    pub(super) fn track_control(&mut self, event: &ControlInputEvent) {
        if let Some(name) = event.name.as_deref() {
            if event.pressed {
                self.pressed_control_sources.insert((event.device, name.to_string()));
                self.pressed_controls.insert(name.to_string());
            } else {
                self.pressed_control_sources.remove(&(event.device, name.to_string()));
                if !self.pressed_control_sources.iter().any(|(_, pressed)| pressed == name) {
                    self.pressed_controls.remove(name);
                }
            }
        }
        if let Some(physical) = event.physical.as_ref() {
            let input = (event.device, physical.clone());
            if event.pressed {
                self.pressed_play_inputs.insert(input);
            } else {
                self.pressed_play_inputs.remove(&input);
            }
        }
    }

    pub(super) fn replace_gamepad_pressed_controls(
        &mut self,
        pressed_buttons: &[GamepadPressedButton],
    ) {
        self.pressed_control_sources.retain(|(device, _)| *device == W_KEYBOARD_DEVICE_ID);
        self.pressed_play_inputs
            .retain(|(_, control)| matches!(control, PhysicalControl::KeyboardKey(_)));
        for button in pressed_buttons {
            self.pressed_control_sources.insert((button.device_id, button.name.clone()));
            self.pressed_play_inputs
                .insert((button.device_id, PhysicalControl::GamepadButton(button.name.clone())));
        }
        self.pressed_controls =
            self.pressed_control_sources.iter().map(|(_, name)| name.clone()).collect();
    }

    pub(super) fn select_e_action_held(&self) -> bool {
        !self.select_e_action_holds.is_empty()
    }

    pub(super) fn accept_app_event(
        &mut self,
        config: InputBounceConfig,
        event: DeviceInputEvent,
    ) -> Option<DeviceInputEvent> {
        self.app_bounce_filter.set_config(config);
        self.app_bounce_filter.accept(event)
    }

    /// UI がゲーム入力をブロックしていても、受理済みキーのReleaseは通す。
    /// バウンスとして抑制したPressは押下集合へ追加しない。
    pub(super) fn raw_keyboard_transition(
        &mut self,
        config: InputBounceConfig,
        physical_key: PhysicalKey,
        state: ElementState,
        gameplay_blocked: bool,
    ) -> Option<DeviceInputEvent> {
        let tracked = self.raw_input_pressed_keys.contains(&physical_key);
        if matches!(state, ElementState::Pressed) && (gameplay_blocked || tracked) {
            return None;
        }
        if matches!(state, ElementState::Released) && !tracked {
            return None;
        }
        let event = physical_key_to_device_input(physical_key, state, false)?;
        self.raw_bounce_filter.set_config(config);
        let event = self.raw_bounce_filter.accept(event)?;
        match state {
            ElementState::Pressed => {
                self.raw_input_pressed_keys.insert(physical_key);
            }
            ElementState::Released => {
                self.raw_input_pressed_keys.remove(&physical_key);
            }
        }
        Some(event)
    }

    pub(super) fn discard_raw_keyboard_transition(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
    ) {
        if state == ElementState::Released {
            self.raw_input_pressed_keys.remove(&physical_key);
        }
        self.raw_bounce_filter.clear();
    }

    pub(super) fn track_window_keyboard(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
        gameplay_enabled: bool,
        has_play_context: bool,
    ) {
        if !gameplay_enabled || repeat {
            return;
        }
        match state {
            ElementState::Pressed if has_play_context => {
                self.window_input_pressed_keys.insert(physical_key);
            }
            ElementState::Released => {
                self.window_input_pressed_keys.remove(&physical_key);
            }
            ElementState::Pressed => {}
        }
    }

    pub(super) fn handle_focus_lost(&mut self) -> InputReleaseBatch {
        self.discard_gamepad_output_until_resynced = true;
        self.pressed_controls.clear();
        self.pressed_control_sources.clear();
        self.pressed_play_inputs.clear();
        self.release_keyboard_inputs()
    }

    pub(super) fn release_keyboard_inputs(&mut self) -> InputReleaseBatch {
        InputReleaseBatch {
            raw_keyboard: self.take_raw_keyboard_releases(),
            window_keyboard: self.take_window_keyboard_releases(),
        }
    }

    pub(super) fn should_discard_gamepad_output(&mut self, focused: bool) -> bool {
        if !focused {
            return true;
        }
        std::mem::take(&mut self.discard_gamepad_output_until_resynced)
    }

    fn take_raw_keyboard_releases(&mut self) -> Vec<DeviceInputEvent> {
        let releases = release_events(&mut self.raw_input_pressed_keys);
        self.raw_bounce_filter.clear();
        releases
    }

    fn take_window_keyboard_releases(&mut self) -> Vec<DeviceInputEvent> {
        let releases = release_events(&mut self.window_input_pressed_keys);
        self.app_bounce_filter.clear();
        releases
    }
}

pub(super) fn should_route_gamepad_event_while_discarding(pressed: bool) -> bool {
    !pressed
}

fn release_events(pressed_keys: &mut HashSet<PhysicalKey>) -> Vec<DeviceInputEvent> {
    std::mem::take(pressed_keys)
        .into_iter()
        .filter_map(|physical_key| {
            physical_key_to_device_input(physical_key, ElementState::Released, false)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bmz_core::input::InputKind;
    use bmz_gameplay::input::backend::DeviceTimestamp;
    use winit::keyboard::KeyCode;

    use super::*;

    fn gamepad_button(
        device_id: DeviceId,
        name: &str,
        pressed: bool,
        timestamp_ns: u128,
    ) -> crate::input::gamepad::GamepadButtonEvent {
        crate::input::gamepad::GamepadButtonEvent {
            name: name.to_string(),
            device_id,
            pressed,
            timestamp: DeviceTimestamp::MonotonicNs(timestamp_ns),
            synthesized_analog_axis: false,
        }
    }

    fn track_and_filter_gamepad(
        runtime: &mut AppInputRuntime,
        config: InputBounceConfig,
        event: &crate::input::gamepad::GamepadButtonEvent,
    ) -> Option<DeviceInputEvent> {
        runtime.track_control(&ControlInputEvent::gamepad(
            event.device_id,
            &event.name,
            event.pressed,
        ));
        runtime.accept_app_event(config, crate::input::gamepad::to_device_input_event(event))
    }

    #[test]
    fn both_shifts_survive_repeat_marked_press_and_other_key_input() {
        for (first, second) in
            [(KeyCode::ShiftLeft, KeyCode::ShiftRight), (KeyCode::ShiftRight, KeyCode::ShiftLeft)]
        {
            let mut runtime = AppInputRuntime::default();
            let first = PhysicalKey::Code(first);
            let second = PhysicalKey::Code(second);
            runtime.track_control(&ControlInputEvent::keyboard_parts(
                first,
                ElementState::Pressed,
                false,
            ));
            assert!(!runtime.keyboard_repeat(second, true));
            runtime.track_control(&ControlInputEvent::keyboard_parts(
                second,
                ElementState::Pressed,
                true,
            ));
            runtime.track_control(&ControlInputEvent::keyboard_parts(
                PhysicalKey::Code(KeyCode::KeyD),
                ElementState::Pressed,
                false,
            ));
            assert!(runtime.pressed_controls.contains("LShift"));
            assert!(runtime.pressed_controls.contains("RShift"));
            assert!(runtime.keyboard_repeat(first, true));
            assert!(runtime.keyboard_repeat(second, true));
        }
    }

    #[test]
    fn missing_shift_release_is_repaired_once_without_releasing_other_inputs() {
        for key in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
            let mut runtime = AppInputRuntime::default();
            let key = PhysicalKey::Code(key);
            let other = PhysicalKey::Code(KeyCode::KeyZ);
            for held in [key, other] {
                runtime.track_control(&ControlInputEvent::keyboard_parts(
                    held,
                    ElementState::Pressed,
                    false,
                ));
                runtime.track_window_keyboard(held, ElementState::Pressed, false, true, true);
            }
            runtime.track_control(&ControlInputEvent::gamepad(DeviceId(2), "Button9", true));
            let release = runtime.reconcile_keyboard_releases(&[key]);
            assert_eq!(release.window_keyboard.len(), 1);
            assert_eq!(release.window_keyboard[0].kind, InputKind::Release);
            assert!(runtime.reconcile_keyboard_releases(&[key]).window_keyboard.is_empty());
            assert!(runtime.pressed_controls.contains("Z"));
            assert!(runtime.pressed_controls.contains("Button9"));
            assert_eq!(runtime.pressed_play_inputs.len(), 2);
            assert_eq!(runtime.release_keyboard_inputs().window_keyboard.len(), 1);
        }
    }

    #[test]
    fn tracks_control_press_and_release_in_shared_state() {
        let mut runtime = AppInputRuntime::default();
        let gamepad = ControlInputEvent::gamepad(DeviceId(2), "ButtonSouth", true);
        runtime.track_control(&gamepad);
        assert!(runtime.pressed_controls.contains("ButtonSouth"));
        assert!(
            runtime.pressed_play_inputs.contains(&(
                DeviceId(2),
                PhysicalControl::GamepadButton("ButtonSouth".to_string())
            ))
        );

        let released = ControlInputEvent::gamepad(DeviceId(2), "ButtonSouth", false);
        runtime.track_control(&released);
        assert!(!runtime.pressed_controls.contains("ButtonSouth"));
        assert!(runtime.pressed_play_inputs.is_empty());
    }

    #[test]
    fn physical_gamepad_hold_survives_suppressed_bounce_press() {
        let config =
            InputBounceConfig { keyboard_threshold_us: 0, controller_threshold_us: 17_000 };
        for control in ["Button9", "Button10"] {
            let mut runtime = AppInputRuntime::default();
            let device = DeviceId(2);
            let sequence = [
                gamepad_button(device, control, true, 0),
                gamepad_button(device, control, false, 1_000_000),
                gamepad_button(device, control, true, 2_000_000),
            ];
            let accepted_press_count = sequence
                .iter()
                .filter_map(|event| track_and_filter_gamepad(&mut runtime, config, event))
                .filter(|event| event.kind == InputKind::Press)
                .count();

            assert_eq!(accepted_press_count, 1, "{control}の再Pressは単発操作へ渡さない");
            assert!(runtime.pressed_controls.contains(control));
            assert!(
                runtime
                    .pressed_play_inputs
                    .contains(&(device, PhysicalControl::GamepadButton(control.to_string())))
            );
        }
    }

    #[test]
    fn releasing_one_gamepad_keeps_same_named_button_held_by_another() {
        let mut runtime = AppInputRuntime::default();
        runtime.track_control(&ControlInputEvent::gamepad(DeviceId(2), "Button9", true));
        runtime.track_control(&ControlInputEvent::gamepad(DeviceId(3), "Button9", true));

        runtime.track_control(&ControlInputEvent::gamepad(DeviceId(2), "Button9", false));

        assert!(runtime.pressed_controls.contains("Button9"));
        assert!(
            runtime
                .pressed_play_inputs
                .contains(&(DeviceId(3), PhysicalControl::GamepadButton("Button9".to_string())))
        );
    }

    #[test]
    fn gamepad_snapshot_resync_preserves_keyboard_and_replaces_gamepad_state() {
        let mut runtime = AppInputRuntime::default();
        runtime.track_control(&ControlInputEvent::keyboard_parts(
            PhysicalKey::Code(KeyCode::KeyQ),
            ElementState::Pressed,
            false,
        ));
        runtime.track_control(&ControlInputEvent::gamepad(DeviceId(2), "Button9", true));

        runtime.replace_gamepad_pressed_controls(&[GamepadPressedButton {
            name: "Button10".to_string(),
            device_id: DeviceId(3),
        }]);

        assert!(runtime.pressed_controls.contains("Q"));
        assert!(!runtime.pressed_controls.contains("Button9"));
        assert!(runtime.pressed_controls.contains("Button10"));
        assert!(
            runtime
                .pressed_play_inputs
                .contains(&(DeviceId(3), PhysicalControl::GamepadButton("Button10".to_string())))
        );
    }

    #[test]
    fn raw_input_blocking_drops_new_presses_but_keeps_tracked_release() {
        let key = PhysicalKey::Code(KeyCode::KeyZ);
        let mut runtime = AppInputRuntime::default();

        let press = runtime
            .raw_keyboard_transition(
                InputBounceConfig::default(),
                key,
                ElementState::Pressed,
                false,
            )
            .unwrap();
        assert_eq!(press.kind, InputKind::Press);
        assert!(
            runtime
                .raw_keyboard_transition(
                    InputBounceConfig::default(),
                    key,
                    ElementState::Pressed,
                    true,
                )
                .is_none()
        );
        let release = runtime
            .raw_keyboard_transition(
                InputBounceConfig::default(),
                key,
                ElementState::Released,
                true,
            )
            .unwrap();
        assert_eq!(release.kind, InputKind::Release);
        assert!(runtime.raw_input_pressed_keys.is_empty());
    }

    #[test]
    fn bounced_raw_press_does_not_restore_pressed_state() {
        let key = PhysicalKey::Code(KeyCode::KeyZ);
        let config =
            InputBounceConfig { keyboard_threshold_us: 1_000_000, controller_threshold_us: 0 };
        let mut runtime = AppInputRuntime::default();

        assert!(
            runtime.raw_keyboard_transition(config, key, ElementState::Pressed, false).is_some()
        );
        assert!(
            runtime.raw_keyboard_transition(config, key, ElementState::Released, false).is_some()
        );
        assert!(
            runtime.raw_keyboard_transition(config, key, ElementState::Pressed, false).is_none()
        );
        assert!(runtime.raw_input_pressed_keys.is_empty());
        assert!(
            runtime.raw_keyboard_transition(config, key, ElementState::Released, false).is_none()
        );
    }

    #[test]
    fn focus_loss_releases_keyboard_state_and_resyncs_gamepad_once() {
        let raw_key = PhysicalKey::Code(KeyCode::KeyZ);
        let window_key = PhysicalKey::Code(KeyCode::KeyX);
        let mut runtime = AppInputRuntime::default();
        runtime
            .raw_keyboard_transition(
                InputBounceConfig::default(),
                raw_key,
                ElementState::Pressed,
                false,
            )
            .unwrap();
        runtime.track_window_keyboard(window_key, ElementState::Pressed, false, true, true);
        runtime.track_control(&ControlInputEvent::gamepad(DeviceId(2), "Button9", true));

        let releases = runtime.handle_focus_lost();

        assert_eq!(releases.raw_keyboard.len(), 1);
        assert_eq!(releases.raw_keyboard[0].kind, InputKind::Release);
        assert_eq!(releases.window_keyboard.len(), 1);
        assert_eq!(releases.window_keyboard[0].kind, InputKind::Release);
        assert!(runtime.should_discard_gamepad_output(false));
        assert!(runtime.should_discard_gamepad_output(true));
        assert!(!runtime.should_discard_gamepad_output(true));
        assert!(!should_route_gamepad_event_while_discarding(true));
        assert!(should_route_gamepad_event_while_discarding(false));

        runtime.replace_gamepad_pressed_controls(&[GamepadPressedButton {
            name: "Button9".to_string(),
            device_id: DeviceId(2),
        }]);
        assert!(runtime.pressed_controls.contains("Button9"));
        assert!(
            runtime
                .pressed_play_inputs
                .contains(&(DeviceId(2), PhysicalControl::GamepadButton("Button9".to_string())))
        );
    }
}
