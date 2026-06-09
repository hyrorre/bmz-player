use std::collections::HashMap;
use std::time::Instant;

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl};
use gilrs::{Axis, Button, EventType};

const GILRS_DEVICE_ID_BASE: u32 = 16;
// beatoraja 実機値 (INFINITAS / DAO / YuanCon / arcin board の実測値)
const BASE_TICK_MAX_SIZE: f32 = 0.009;

pub struct GilrsButtonEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub pressed: bool,
}

/// アナログ軸の生 tick 差分。選曲画面の回転量比例スクロール用。
/// `name` は符号なしの軸名 (`AxisLeftX` 等)、`ticks` は符号付き tick 数。
pub struct GilrsAxisTickEvent {
    pub name: &'static str,
    pub device_id: DeviceId,
    pub ticks: i32,
}

#[derive(Default)]
pub struct GilrsPollOutput {
    pub buttons: Vec<GilrsButtonEvent>,
    pub axis_ticks: Vec<GilrsAxisTickEvent>,
}

#[derive(Default)]
struct ScratchState {
    active: bool,
    positive_direction: bool,
    last_movement: Option<Instant>,
}

struct GilrsConfig {
    tick_max_size: f32,
    scratch_timeout_ms: u64,
}

pub struct GilrsBackend {
    gilrs: gilrs::Gilrs,
    axis_prev: HashMap<(gilrs::GamepadId, Axis), f32>,
    scratch_state: HashMap<(gilrs::GamepadId, Axis), ScratchState>,
    config: GilrsConfig,
}

impl GilrsBackend {
    pub fn new(sensitivity: f32, scratch_timeout_ms: u32) -> Result<Self, Box<gilrs::Error>> {
        let gilrs =
            gilrs::GilrsBuilder::new().with_default_filters(false).build().map_err(Box::new)?;
        Ok(Self {
            gilrs,
            axis_prev: HashMap::new(),
            scratch_state: HashMap::new(),
            config: GilrsConfig {
                tick_max_size: BASE_TICK_MAX_SIZE / sensitivity.max(0.01),
                scratch_timeout_ms: scratch_timeout_ms as u64,
            },
        })
    }

    pub fn poll(&mut self) -> GilrsPollOutput {
        let mut output = GilrsPollOutput::default();
        while let Some(gilrs::Event { id, event, .. }) = self.gilrs.next_event() {
            match event {
                EventType::ButtonPressed(button, _) => {
                    if let Some(name) = gilrs_button_name(button) {
                        output.buttons.push(GilrsButtonEvent {
                            name: name.to_string(),
                            device_id: gilrs_gamepad_device_id(id),
                            pressed: true,
                        });
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if let Some(name) = gilrs_button_name(button) {
                        output.buttons.push(GilrsButtonEvent {
                            name: name.to_string(),
                            device_id: gilrs_gamepad_device_id(id),
                            pressed: false,
                        });
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    self.process_axis(id, axis, value, &mut output);
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
        self.check_scratch_timeouts(&mut output.buttons);
        output
    }

    fn process_axis(
        &mut self,
        id: gilrs::GamepadId,
        axis: Axis,
        value: f32,
        output: &mut GilrsPollOutput,
    ) {
        let Some(axis_name) = gilrs_axis_name(axis) else { return };
        let prev = self.axis_prev.entry((id, axis)).or_insert(value);
        let ticks = compute_analog_diff(*prev, value, self.config.tick_max_size);
        *prev = value;

        if ticks == 0 {
            return;
        }

        let positive = ticks > 0;
        let device_id = gilrs_gamepad_device_id(id);
        output.axis_ticks.push(GilrsAxisTickEvent { name: axis_name, device_id, ticks });
        let state = self.scratch_state.entry((id, axis)).or_default();

        if !state.active {
            let name = format!("{}{}", axis_name, if positive { "+" } else { "-" });
            output.buttons.push(GilrsButtonEvent { name, device_id, pressed: true });
            state.active = true;
            state.positive_direction = positive;
        } else if state.positive_direction != positive {
            let old_name =
                format!("{}{}", axis_name, if state.positive_direction { "+" } else { "-" });
            output.buttons.push(GilrsButtonEvent { name: old_name, device_id, pressed: false });
            let new_name = format!("{}{}", axis_name, if positive { "+" } else { "-" });
            output.buttons.push(GilrsButtonEvent { name: new_name, device_id, pressed: true });
            state.positive_direction = positive;
        }
        state.last_movement = Some(Instant::now());
    }

    fn check_scratch_timeouts(&mut self, events: &mut Vec<GilrsButtonEvent>) {
        let timeout_ms = self.config.scratch_timeout_ms;
        for ((id, axis), state) in &mut self.scratch_state {
            if !state.active {
                continue;
            }
            let timed_out =
                state.last_movement.is_none_or(|t| t.elapsed().as_millis() as u64 > timeout_ms);
            if timed_out {
                if let Some(axis_name) = gilrs_axis_name(*axis) {
                    let name = format!(
                        "{}{}",
                        axis_name,
                        if state.positive_direction { "+" } else { "-" }
                    );
                    events.push(GilrsButtonEvent {
                        name,
                        device_id: gilrs_gamepad_device_id(*id),
                        pressed: false,
                    });
                }
                state.active = false;
            }
        }
    }
}

pub fn to_device_input_event(event: &GilrsButtonEvent) -> DeviceInputEvent {
    DeviceInputEvent {
        device: event.device_id,
        control: PhysicalControl::GamepadButton(event.name.clone()),
        kind: if event.pressed { InputKind::Press } else { InputKind::Release },
        timestamp: DeviceTimestamp::Unknown,
    }
}

// beatoraja computeAnalogDiff 移植。軸の折り返し（±1.0）を考慮した整数ティック数を返す。
fn compute_analog_diff(old_value: f32, new_value: f32, tick_max_size: f32) -> i32 {
    let mut diff = new_value - old_value;
    let wraparound = 2.0 + tick_max_size / 2.0;
    if diff > 1.0 {
        diff -= wraparound;
    } else if diff < -1.0 {
        diff += wraparound;
    }
    diff /= tick_max_size;
    if diff > 0.0 { diff.ceil() as i32 } else { diff.floor() as i32 }
}

fn gilrs_gamepad_device_id(id: gilrs::GamepadId) -> DeviceId {
    DeviceId(GILRS_DEVICE_ID_BASE + usize::from(id) as u32)
}

fn gilrs_button_name(button: Button) -> Option<&'static str> {
    match button {
        Button::South => Some("Button1"),
        Button::East => Some("Button2"),
        Button::West => Some("Button3"),
        Button::North => Some("Button4"),
        Button::LeftTrigger => Some("Button5"),
        Button::RightTrigger => Some("Button6"),
        Button::LeftTrigger2 => Some("Button7"),
        Button::RightTrigger2 => Some("Button8"),
        Button::LeftThumb => Some("Button9"),
        Button::RightThumb => Some("Button10"),
        Button::Select | Button::C => Some("Select"),
        Button::Start => Some("Start"),
        Button::Mode => Some("Mode"),
        Button::DPadUp => Some("DPadUp"),
        Button::DPadDown => Some("DPadDown"),
        Button::DPadLeft => Some("DPadLeft"),
        Button::DPadRight => Some("DPadRight"),
        Button::Z | Button::Unknown => None,
    }
}

fn gilrs_axis_name(axis: Axis) -> Option<&'static str> {
    match axis {
        Axis::LeftStickX => Some("AxisLeftX"),
        Axis::LeftStickY => Some("AxisLeftY"),
        Axis::RightStickX => Some("AxisRightX"),
        Axis::RightStickY => Some("AxisRightY"),
        Axis::LeftZ => Some("AxisLeftZ"),
        Axis::RightZ => Some("AxisRightZ"),
        Axis::DPadX => Some("AxisDPadX"),
        Axis::DPadY => Some("AxisDPadY"),
        Axis::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_analog_diff_basic_movement() {
        let tick = BASE_TICK_MAX_SIZE;
        // 1 tick 分の移動
        assert_eq!(compute_analog_diff(0.0, tick, tick), 1);
        assert_eq!(compute_analog_diff(0.0, -tick, tick), -1);
    }

    #[test]
    fn compute_analog_diff_wraparound() {
        let tick = BASE_TICK_MAX_SIZE;
        // 0.99 → -0.99: 正方向の折り返し（-0.99 - 0.99 = -1.98 → wrapped）
        let diff = compute_analog_diff(0.99, -0.99, tick);
        // 折り返し補正で正方向に解釈される
        assert!(diff > 0, "wraparound should be positive: {diff}");
    }

    #[test]
    fn compute_analog_diff_no_movement() {
        assert_eq!(compute_analog_diff(0.5, 0.5, BASE_TICK_MAX_SIZE), 0);
    }

    #[test]
    fn gilrs_button_name_numbered() {
        assert_eq!(gilrs_button_name(Button::South), Some("Button1"));
        assert_eq!(gilrs_button_name(Button::East), Some("Button2"));
        assert_eq!(gilrs_button_name(Button::LeftTrigger2), Some("Button7"));
        assert_eq!(gilrs_button_name(Button::Start), Some("Start"));
        assert_eq!(gilrs_button_name(Button::Unknown), None);
    }
}
