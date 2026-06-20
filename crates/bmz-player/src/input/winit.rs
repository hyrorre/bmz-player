use std::sync::{Arc, Mutex};

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{
    BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, InputBackend,
    InputEventSink, PhysicalControl, monotonic_timestamp_ns,
};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};

pub const W_KEYBOARD_DEVICE_ID: DeviceId = DeviceId(0);

#[derive(Debug, Clone, Default)]
pub struct WinitInputBackend {
    buffer: Arc<Mutex<BufferedInputBackend>>,
}

impl WinitInputBackend {
    pub fn handle_key_parts(&self, physical_key: PhysicalKey, state: ElementState, repeat: bool) {
        if let Some(event) = physical_key_to_device_input(physical_key, state, repeat) {
            self.push_shared_event(event);
        }
    }

    pub fn handle_key_event(&self, event: &KeyEvent) {
        if let Some(event) = key_event_to_device_input(event) {
            self.push_shared_event(event);
        }
    }

    pub(crate) fn push_shared_event(&self, event: DeviceInputEvent) {
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.push_event(event);
        }
    }
}

impl InputBackend for WinitInputBackend {
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        self.buffer.lock().map(|mut buffer| buffer.drain_events()).unwrap_or_default()
    }
}

impl InputEventSink for WinitInputBackend {
    fn push_event(&mut self, event: DeviceInputEvent) {
        self.push_shared_event(event);
    }
}

pub fn key_event_to_device_input(event: &KeyEvent) -> Option<DeviceInputEvent> {
    physical_key_to_device_input(event.physical_key, event.state, event.repeat)
}

pub fn physical_key_to_device_input(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<DeviceInputEvent> {
    if repeat {
        return None;
    }

    Some(DeviceInputEvent {
        device: W_KEYBOARD_DEVICE_ID,
        control: physical_key_to_control(physical_key)?,
        kind: input_kind_from_element_state(state),
        timestamp: DeviceTimestamp::MonotonicNs(monotonic_timestamp_ns()),
    })
}

pub fn physical_key_to_control(physical_key: PhysicalKey) -> Option<PhysicalControl> {
    match physical_key {
        PhysicalKey::Code(code) => Some(PhysicalControl::KeyboardKey(key_code_name(code))),
        PhysicalKey::Unidentified(NativeKeyCode::Unidentified) => None,
        PhysicalKey::Unidentified(native) => {
            Some(PhysicalControl::KeyboardKey(native_key_code_name(native)))
        }
    }
}

fn input_kind_from_element_state(state: ElementState) -> InputKind {
    match state {
        ElementState::Pressed => InputKind::Press,
        ElementState::Released => InputKind::Release,
    }
}

fn key_code_name(code: KeyCode) -> String {
    match code {
        KeyCode::KeyA => "A",
        KeyCode::KeyB => "B",
        KeyCode::KeyC => "C",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyM => "M",
        KeyCode::KeyN => "N",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyU => "U",
        KeyCode::KeyV => "V",
        KeyCode::KeyW => "W",
        KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y",
        KeyCode::KeyZ => "Z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        KeyCode::ShiftLeft => "LShift",
        KeyCode::ShiftRight => "RShift",
        KeyCode::ControlLeft => "LControl",
        KeyCode::ControlRight => "RControl",
        KeyCode::AltLeft => "LAlt",
        KeyCode::AltRight => "RAlt",
        KeyCode::Space => "Space",
        KeyCode::Enter => "Enter",
        KeyCode::Escape => "Escape",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::Tab => "Tab",
        KeyCode::Numpad0 => "Numpad0",
        KeyCode::Numpad1 => "Numpad1",
        KeyCode::Numpad2 => "Numpad2",
        KeyCode::Numpad3 => "Numpad3",
        KeyCode::Numpad4 => "Numpad4",
        KeyCode::Numpad5 => "Numpad5",
        KeyCode::Numpad6 => "Numpad6",
        KeyCode::Numpad7 => "Numpad7",
        KeyCode::Numpad8 => "Numpad8",
        KeyCode::Numpad9 => "Numpad9",
        _ => return format!("{code:?}"),
    }
    .to_string()
}

fn native_key_code_name(code: NativeKeyCode) -> String {
    match code {
        NativeKeyCode::Unidentified => "Native:Unidentified".to_string(),
        NativeKeyCode::Android(code) => format!("Native:Android:{code}"),
        NativeKeyCode::MacOS(code) => format!("Native:MacOS:{code}"),
        NativeKeyCode::Windows(code) => format!("Native:Windows:{code}"),
        NativeKeyCode::Xkb(code) => format!("Native:Xkb:{code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_key_to_control_matches_profile_binding_names() {
        assert_eq!(
            physical_key_to_control(PhysicalKey::Code(KeyCode::KeyZ)),
            Some(PhysicalControl::KeyboardKey("Z".to_string()))
        );
        assert_eq!(
            physical_key_to_control(PhysicalKey::Code(KeyCode::ShiftLeft)),
            Some(PhysicalControl::KeyboardKey("LShift".to_string()))
        );
    }

    #[test]
    fn physical_key_to_device_input_maps_press_and_release() {
        let press = physical_key_to_device_input(
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Pressed,
            false,
        )
        .unwrap();
        let release = physical_key_to_device_input(
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Released,
            false,
        )
        .unwrap();

        assert_eq!(press.kind, InputKind::Press);
        assert_eq!(release.kind, InputKind::Release);
        assert_eq!(press.device, W_KEYBOARD_DEVICE_ID);
        assert!(matches!(press.timestamp, DeviceTimestamp::MonotonicNs(_)));
    }

    #[test]
    fn repeated_key_events_are_ignored() {
        let event = physical_key_to_device_input(
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Pressed,
            true,
        );

        assert!(event.is_none());
    }

    #[test]
    fn unidentified_native_key_keeps_platform_code() {
        assert_eq!(
            physical_key_to_control(PhysicalKey::Unidentified(NativeKeyCode::Windows(30))),
            Some(PhysicalControl::KeyboardKey("Native:Windows:30".to_string()))
        );
        assert_eq!(
            physical_key_to_control(PhysicalKey::Unidentified(NativeKeyCode::Unidentified)),
            None
        );
    }

    #[test]
    fn winit_input_backend_buffers_translated_events() {
        let mut backend = WinitInputBackend::default();

        backend.handle_key_parts(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Pressed, false);
        backend.handle_key_parts(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Pressed, true);
        backend.handle_key_parts(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Released, false);

        let events = backend.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, InputKind::Press);
        assert_eq!(events[1].kind, InputKind::Release);
    }

    #[test]
    fn cloned_winit_input_backend_shares_events() {
        let event_source = WinitInputBackend::default();
        let mut game_backend = event_source.clone();

        event_source.handle_key_parts(
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Pressed,
            false,
        );

        let events = game_backend.drain_events();
        assert_eq!(events.len(), 1);
        assert!(game_backend.drain_events().is_empty());
    }
}
