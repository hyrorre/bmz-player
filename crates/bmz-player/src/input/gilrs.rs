use std::collections::HashMap;
use std::time::Instant;

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl};
use gilrs::{Axis, Button, EventType};

pub const GILRS_DEVICE_ID_BASE: u32 = 16;
// beatoraja 実機値 (INFINITAS / DAO / YuanCon / arcin board の実測値)
const BASE_TICK_MAX_SIZE: f32 = 0.009;

pub struct GilrsButtonEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub pressed: bool,
}

#[derive(Debug, Clone)]
pub struct GilrsRawCode {
    pub value: u32,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GilrsRawEventKind {
    Button,
    Axis,
}

impl GilrsRawEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Axis => "axis",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GilrsRawEvent {
    pub device_id: DeviceId,
    pub kind: GilrsRawEventKind,
    pub logical: String,
    pub raw_code: GilrsRawCode,
    pub mapped_control: Option<String>,
    pub pressed: Option<bool>,
    pub value: Option<f32>,
    pub ticks: Option<i32>,
}

/// アナログ軸の生 tick 差分。選曲画面の回転量比例スクロール用。
/// `name` は符号なしの raw 軸名 (`Axis1` 等)、`ticks` は符号付き tick 数。
pub struct GilrsAxisTickEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub ticks: i32,
}

#[derive(Default)]
pub struct GilrsPollOutput {
    pub buttons: Vec<GilrsButtonEvent>,
    pub axis_ticks: Vec<GilrsAxisTickEvent>,
    pub raw_events: Vec<GilrsRawEvent>,
}

#[derive(Default)]
struct ScratchState {
    active: bool,
    positive_direction: bool,
    control_name: Option<String>,
    last_movement: Option<Instant>,
}

struct GilrsConfig {
    tick_max_size: f32,
    scratch_timeout_ms: u64,
}

pub struct GilrsBackend {
    gilrs: gilrs::Gilrs,
    axis_prev: HashMap<(gilrs::GamepadId, u32), f32>,
    scratch_state: HashMap<(gilrs::GamepadId, u32), ScratchState>,
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
                EventType::ButtonPressed(button, code) => {
                    process_button_event(id, button, code, true, &mut output);
                }
                EventType::ButtonReleased(button, code) => {
                    process_button_event(id, button, code, false, &mut output);
                }
                EventType::AxisChanged(axis, value, code) => {
                    self.process_axis(id, axis, value, code, &mut output);
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
        code: gilrs::ev::Code,
        output: &mut GilrsPollOutput,
    ) {
        let raw_code = raw_code_from_gilrs(code);
        let axis_name = raw_control_name(GilrsRawEventKind::Axis, &raw_code);
        let axis_key = raw_code.value;
        let prev = self.axis_prev.entry((id, axis_key)).or_insert(value);
        let ticks = compute_analog_diff(*prev, value, self.config.tick_max_size);
        *prev = value;

        if ticks == 0 {
            return;
        }

        let positive = ticks > 0;
        let device_id = gilrs_gamepad_device_id(id);
        output.raw_events.push(GilrsRawEvent {
            device_id,
            kind: GilrsRawEventKind::Axis,
            logical: format!("{axis:?}"),
            raw_code,
            mapped_control: Some(axis_name.to_string()),
            pressed: None,
            value: Some(value),
            ticks: Some(ticks),
        });
        output.axis_ticks.push(GilrsAxisTickEvent { name: axis_name.clone(), device_id, ticks });
        let state = self.scratch_state.entry((id, axis_key)).or_default();

        if !state.active {
            let name = format!("{}{}", axis_name, if positive { "+" } else { "-" });
            output.buttons.push(GilrsButtonEvent { name, device_id, pressed: true });
            state.active = true;
            state.positive_direction = positive;
            state.control_name = Some(axis_name);
        } else if state.positive_direction != positive {
            let axis_name = state.control_name.as_deref().unwrap_or(&axis_name);
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
        for ((id, _axis), state) in &mut self.scratch_state {
            if !state.active {
                continue;
            }
            let timed_out =
                state.last_movement.is_none_or(|t| t.elapsed().as_millis() as u64 > timeout_ms);
            if timed_out {
                if let Some(axis_name) = state.control_name.as_deref() {
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
                state.control_name = None;
            }
        }
    }
}

fn process_button_event(
    id: gilrs::GamepadId,
    button: Button,
    code: gilrs::ev::Code,
    pressed: bool,
    output: &mut GilrsPollOutput,
) {
    let device_id = gilrs_gamepad_device_id(id);
    let raw_code = raw_code_from_gilrs(code);
    let mapped_control = raw_control_name(GilrsRawEventKind::Button, &raw_code);
    output.raw_events.push(GilrsRawEvent {
        device_id,
        kind: GilrsRawEventKind::Button,
        logical: format!("{button:?}"),
        raw_code,
        mapped_control: Some(mapped_control.clone()),
        pressed: Some(pressed),
        value: None,
        ticks: None,
    });

    output.buttons.push(GilrsButtonEvent { name: mapped_control, device_id, pressed });
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

pub fn gilrs_gamepad_device_id_from_player_index(index: u32) -> Option<DeviceId> {
    index.checked_sub(1).map(|offset| DeviceId(GILRS_DEVICE_ID_BASE + offset))
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
}
