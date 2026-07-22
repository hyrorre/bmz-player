use std::time::{Instant, SystemTime};

use bmz_gameplay::input::backend::{DeviceId, DeviceTimestamp, monotonic_timestamp_ns};
use gilrs::{Axis, Button, EventType};

use super::gamepad::{
    AnalogGamepadProcessor, ConnectedGamepad, GamepadButtonEvent, GamepadPollOutput,
    RawControlCode, RawInputEvent, RawInputEventKind, current_device_timestamp,
    gamepad_device_id_from_backend_index,
};

pub use super::gamepad::GamepadSlotMap;
pub type GilrsButtonEvent = GamepadButtonEvent;
pub type GilrsRawCode = RawControlCode;
pub type GilrsRawEventKind = RawInputEventKind;
pub type GilrsRawEvent = RawInputEvent;
pub type GilrsPollOutput = GamepadPollOutput;

pub struct GilrsBackend {
    gilrs: gilrs::Gilrs,
    analog: AnalogGamepadProcessor,
}

impl GilrsBackend {
    pub fn new(sensitivity: f32, scratch_threshold: u32) -> Result<Self, Box<gilrs::Error>> {
        let gilrs =
            gilrs::GilrsBuilder::new().with_default_filters(false).build().map_err(Box::new)?;
        Ok(Self { gilrs, analog: AnalogGamepadProcessor::new(sensitivity, scratch_threshold) })
    }

    pub fn poll(&mut self) -> GilrsPollOutput {
        let mut output = GilrsPollOutput::default();
        self.analog.check_timeouts(Instant::now(), &mut output.buttons);
        while let Some(gilrs::Event { id, event, time }) = self.gilrs.next_event() {
            let timestamp = device_timestamp_from_system_time(time);
            match event {
                EventType::ButtonPressed(button, code) => {
                    process_button_event(id, button, code, true, timestamp, &mut output);
                }
                EventType::ButtonReleased(button, code) => {
                    process_button_event(id, button, code, false, timestamp, &mut output);
                }
                EventType::AxisChanged(axis, value, code) => {
                    self.process_axis(id, axis, value, code, timestamp, &mut output);
                }
                EventType::Connected => {
                    tracing::info!(gamepad = ?id, "gamepad connected");
                }
                EventType::Disconnected => {
                    tracing::info!(gamepad = ?id, "gamepad disconnected");
                }
                _ => {}
            }
        }
        self.analog.check_timeouts(Instant::now(), &mut output.buttons);
        output
    }

    /// 接続中 (および gilrs が認識している) ゲームパッド一覧。
    pub fn connected_gamepads(&self) -> Vec<ConnectedGamepad> {
        self.gilrs
            .gamepads()
            .map(|(id, pad)| ConnectedGamepad {
                stable_id: format!("gilrs:{}", usize::from(id)),
                backend_id: usize::from(id) as u32,
                device_id: gilrs_gamepad_device_id(id),
                name: pad.name().to_string(),
                is_connected: pad.is_connected(),
            })
            .collect()
    }

    fn process_axis(
        &mut self,
        id: gilrs::GamepadId,
        axis: Axis,
        value: f32,
        code: gilrs::ev::Code,
        timestamp: DeviceTimestamp,
        output: &mut GilrsPollOutput,
    ) {
        let raw_code = raw_code_from_gilrs(code);
        let axis_name = raw_control_name(GilrsRawEventKind::Axis, &raw_code);
        let axis_key = raw_code.value;
        let device_id = gilrs_gamepad_device_id(id);
        self.analog.process_axis(
            device_id,
            axis_key,
            &axis_name,
            format!("{axis:?}"),
            raw_code,
            value,
            timestamp,
            output,
        );
    }
}

fn process_button_event(
    id: gilrs::GamepadId,
    button: Button,
    code: gilrs::ev::Code,
    pressed: bool,
    timestamp: DeviceTimestamp,
    output: &mut GilrsPollOutput,
) {
    let device_id = gilrs_gamepad_device_id(id);
    let raw_code = raw_code_from_gilrs(code);
    let mapped_control = raw_control_name(GilrsRawEventKind::Button, &raw_code);
    output.raw_events.push(RawInputEvent {
        device_id,
        kind: RawInputEventKind::Button,
        logical: format!("{button:?}"),
        raw_code,
        timestamp,
        mapped_control: Some(mapped_control.clone()),
        pressed: Some(pressed),
        value: None,
        ticks: None,
    });

    output.buttons.push(GilrsButtonEvent {
        name: mapped_control,
        device_id,
        pressed,
        timestamp,
        synthesized_analog_axis: false,
    });
}

fn device_timestamp_from_system_time(event_time: SystemTime) -> DeviceTimestamp {
    let now_mono = monotonic_timestamp_ns();
    let now_system = SystemTime::now();
    if let Ok(age) = now_system.duration_since(event_time) {
        DeviceTimestamp::MonotonicNs(now_mono.saturating_sub(age.as_nanos()))
    } else if let Ok(future) = event_time.duration_since(now_system) {
        DeviceTimestamp::MonotonicNs(now_mono.saturating_add(future.as_nanos()))
    } else {
        current_device_timestamp()
    }
}

fn raw_code_from_gilrs(code: gilrs::ev::Code) -> GilrsRawCode {
    GilrsRawCode { value: code.into_u32(), label: code.to_string() }
}

fn raw_control_name(kind: GilrsRawEventKind, raw_code: &GilrsRawCode) -> String {
    raw_control_name_from_parts(kind, &raw_code.label, raw_code.value)
}

fn raw_control_name_from_parts(kind: GilrsRawEventKind, label: &str, value: u32) -> String {
    match kind {
        GilrsRawEventKind::Button => {
            let index = parse_raw_code_index(label, "Button").unwrap_or(value);
            format!("Button{}", index.saturating_add(1))
        }
        GilrsRawEventKind::Axis => {
            let index = parse_raw_code_index(label, "Axis")
                .or_else(|| parse_raw_code_index(label, "Switch"))
                .unwrap_or(value);
            format!("Axis{}", index.saturating_add(1))
        }
    }
}

fn parse_raw_code_index(label: &str, kind: &str) -> Option<u32> {
    label.strip_prefix(kind)?.strip_prefix('(')?.strip_suffix(')')?.parse().ok()
}

fn gilrs_gamepad_device_id(id: gilrs::GamepadId) -> DeviceId {
    gamepad_device_id_from_backend_index(usize::from(id) as u32)
}

pub fn gilrs_gamepad_device_id_from_player_index(index: u32) -> Option<DeviceId> {
    index.checked_sub(1).map(gamepad_device_id_from_backend_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::gamepad::to_device_input_event;

    fn test_timestamp(ns: u128) -> DeviceTimestamp {
        DeviceTimestamp::MonotonicNs(ns)
    }

    #[test]
    fn raw_control_name_uses_platform_code_indices() {
        assert_eq!(
            raw_control_name_from_parts(GilrsRawEventKind::Button, "Button(0)", 0),
            "Button1"
        );
        assert_eq!(
            raw_control_name_from_parts(GilrsRawEventKind::Button, "Button(6)", 6),
            "Button7"
        );
        assert_eq!(
            raw_control_name_from_parts(GilrsRawEventKind::Axis, "Axis(0)", 65_536),
            "Axis1"
        );
        assert_eq!(
            raw_control_name_from_parts(GilrsRawEventKind::Axis, "Switch(1)", 131_073),
            "Axis2"
        );
        assert_eq!(raw_control_name_from_parts(GilrsRawEventKind::Button, "12", 12), "Button13");
    }

    #[test]
    fn player_index_maps_to_gilrs_device_id() {
        assert_eq!(gilrs_gamepad_device_id_from_player_index(0), None);
        assert_eq!(gilrs_gamepad_device_id_from_player_index(1), Some(DeviceId(16)));
        assert_eq!(gilrs_gamepad_device_id_from_player_index(2), Some(DeviceId(17)));
    }

    #[test]
    fn players_above_two_keep_their_numbered_device_mapping() {
        let slots = GamepadSlotMap::default();
        assert_eq!(slots.device_id_for_player(3), Some(DeviceId(18)));
    }

    #[test]
    fn to_device_input_event_preserves_event_timestamp() {
        let event = GilrsButtonEvent {
            device_id: DeviceId(16),
            name: "South".to_string(),
            pressed: true,
            timestamp: test_timestamp(123),
            synthesized_analog_axis: false,
        };

        let input = to_device_input_event(&event);

        assert_eq!(input.timestamp, test_timestamp(123));
    }
}
