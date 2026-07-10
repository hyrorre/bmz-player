use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl, monotonic_timestamp_ns,
};
use gilrs::{Axis, Button, EventType};

pub const GILRS_DEVICE_ID_BASE: u32 = 16;
// beatoraja 実機値 (INFINITAS / DAO / YuanCon / arcin board の実測値)
const BASE_TICK_MAX_SIZE: f32 = 0.009;
const ANALOG_SCRATCH_THRESHOLD_MIN: u32 = 1;
const ANALOG_SCRATCH_THRESHOLD_MAX: u32 = 1_000;
const ANALOG_SCRATCH_CALLS_PER_AXIS_POLL: u32 = 2;

/// 接続中ゲームパッドの表示用スナップショット。
#[derive(Debug, Clone)]
pub struct ConnectedGamepad {
    /// gilrs の `GamepadId` (0-based)。
    pub gilrs_id: u32,
    pub device_id: DeviceId,
    pub name: String,
    pub is_connected: bool,
}

/// 論理スロット `gamepad1` / `gamepad2` → 物理 gilrs id の対応。
///
/// `slot_gilrs_ids[0]` が 1P、`[1]` が 2P。`None` は接続順フォールバック
/// (`gamepadN` → gilrs id `N-1`) を使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadSlotMap {
    pub slot_gilrs_ids: [Option<u32>; 2],
}

impl GamepadSlotMap {
    pub fn from_slot_ids(slot_gilrs_ids: [Option<u32>; 2]) -> Self {
        Self { slot_gilrs_ids }
    }

    /// プレイヤー番号 (1 or 2) に対応する `DeviceId` を返す。
    pub fn device_id_for_player(self, player_index: u32) -> Option<DeviceId> {
        let slot = player_index.checked_sub(1)? as usize;
        if slot >= 2 {
            return None;
        }
        match self.slot_gilrs_ids[slot] {
            Some(gilrs_id) => Some(DeviceId(GILRS_DEVICE_ID_BASE.saturating_add(gilrs_id))),
            None => gilrs_gamepad_device_id_from_player_index(player_index),
        }
    }
}

pub struct GilrsButtonEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub pressed: bool,
    pub timestamp: DeviceTimestamp,
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
    pub timestamp: DeviceTimestamp,
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
    pub timestamp: DeviceTimestamp,
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
    counter: u32,
    tick_counter: u32,
    last_counter_update: Option<Instant>,
}

struct GilrsConfig {
    tick_max_size: f32,
    scratch_threshold: u32,
}

pub struct GilrsBackend {
    gilrs: gilrs::Gilrs,
    axis_prev: HashMap<(gilrs::GamepadId, u32), f32>,
    scratch_state: HashMap<(gilrs::GamepadId, u32), ScratchState>,
    config: GilrsConfig,
}

impl GilrsBackend {
    pub fn new(sensitivity: f32, scratch_threshold: u32) -> Result<Self, Box<gilrs::Error>> {
        let gilrs =
            gilrs::GilrsBuilder::new().with_default_filters(false).build().map_err(Box::new)?;
        Ok(Self {
            gilrs,
            axis_prev: HashMap::new(),
            scratch_state: HashMap::new(),
            config: GilrsConfig {
                tick_max_size: BASE_TICK_MAX_SIZE / sensitivity.max(0.01),
                scratch_threshold: clamp_analog_scratch_threshold(scratch_threshold),
            },
        })
    }

    pub fn poll(&mut self) -> GilrsPollOutput {
        let mut output = GilrsPollOutput::default();
        self.check_scratch_timeouts(Instant::now(), &mut output.buttons);
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
        self.check_scratch_timeouts(Instant::now(), &mut output.buttons);
        output
    }

    /// 接続中 (および gilrs が認識している) ゲームパッド一覧。
    pub fn connected_gamepads(&self) -> Vec<ConnectedGamepad> {
        self.gilrs
            .gamepads()
            .map(|(id, pad)| ConnectedGamepad {
                gilrs_id: usize::from(id) as u32,
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
        let prev = self.axis_prev.entry((id, axis_key)).or_insert(value);
        let ticks = compute_analog_diff(*prev, value, self.config.tick_max_size);
        *prev = value;

        if ticks == 0 {
            return;
        }

        let now = Instant::now();
        let device_id = gilrs_gamepad_device_id(id);
        output.raw_events.push(GilrsRawEvent {
            device_id,
            kind: GilrsRawEventKind::Axis,
            logical: format!("{axis:?}"),
            raw_code,
            timestamp,
            mapped_control: Some(axis_name.to_string()),
            pressed: None,
            value: Some(value),
            ticks: Some(ticks),
        });
        output.axis_ticks.push(GilrsAxisTickEvent {
            name: axis_name.clone(),
            device_id,
            timestamp,
            ticks,
        });
        let state = self.scratch_state.entry((id, axis_key)).or_default();
        state.advance_to(now, self.config.scratch_threshold, device_id, &mut output.buttons);
        state.apply_movement(
            ticks,
            &axis_name,
            device_id,
            timestamp,
            self.config.scratch_threshold,
            &mut output.buttons,
        );
    }

    fn check_scratch_timeouts(&mut self, now: Instant, events: &mut Vec<GilrsButtonEvent>) {
        let threshold = self.config.scratch_threshold;
        for ((id, _axis), state) in &mut self.scratch_state {
            state.advance_to(now, threshold, gilrs_gamepad_device_id(*id), events);
        }
    }
}

impl ScratchState {
    fn advance_to(
        &mut self,
        now: Instant,
        threshold: u32,
        device_id: DeviceId,
        events: &mut Vec<GilrsButtonEvent>,
    ) {
        let elapsed = self
            .last_counter_update
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or_default();
        self.last_counter_update = Some(now);

        // beatoraja evaluates each analog axis once for AXIS+ and once for AXIS- on
        // its ~1ms polling thread, so its counter advances in calls rather than ms.
        let elapsed_ticks =
            duration_millis_u32(elapsed).saturating_mul(ANALOG_SCRATCH_CALLS_PER_AXIS_POLL);
        if elapsed_ticks > 0 {
            self.counter = self.counter.saturating_add(elapsed_ticks);
        }

        if self.counter > threshold.saturating_mul(2) {
            self.release_if_active(device_id, events);
            self.tick_counter = 0;
            self.counter = 0;
        }
    }

    fn apply_movement(
        &mut self,
        ticks: i32,
        axis_name: &str,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        threshold: u32,
        events: &mut Vec<GilrsButtonEvent>,
    ) {
        let positive = ticks > 0;
        self.control_name.get_or_insert_with(|| axis_name.to_string());

        if self.active && self.positive_direction != positive {
            self.release_if_active_at(device_id, timestamp, events);
            self.positive_direction = positive;
            self.tick_counter = 0;
        } else if !self.active {
            if self.tick_counter == 0 || self.counter <= threshold {
                self.tick_counter = self.tick_counter.saturating_add(ticks.unsigned_abs());
            }
            if self.tick_counter >= 2 {
                self.active = true;
                self.positive_direction = positive;
                self.push_button_event(device_id, true, timestamp, events);
            }
        }

        self.counter = 0;
    }

    fn release_if_active(&mut self, device_id: DeviceId, events: &mut Vec<GilrsButtonEvent>) {
        self.release_if_active_at(device_id, timeout_device_timestamp(), events);
    }

    fn release_if_active_at(
        &mut self,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GilrsButtonEvent>,
    ) {
        if self.active {
            self.push_button_event(device_id, false, timestamp, events);
            self.active = false;
        }
    }

    fn push_button_event(
        &self,
        device_id: DeviceId,
        pressed: bool,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GilrsButtonEvent>,
    ) {
        if let Some(axis_name) = self.control_name.as_deref() {
            let name = format!("{}{}", axis_name, if self.positive_direction { "+" } else { "-" });
            events.push(GilrsButtonEvent { name, device_id, pressed, timestamp });
        }
    }
}

fn clamp_analog_scratch_threshold(value: u32) -> u32 {
    value.clamp(ANALOG_SCRATCH_THRESHOLD_MIN, ANALOG_SCRATCH_THRESHOLD_MAX)
}

fn duration_millis_u32(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
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
    output.raw_events.push(GilrsRawEvent {
        device_id,
        kind: GilrsRawEventKind::Button,
        logical: format!("{button:?}"),
        raw_code,
        timestamp,
        mapped_control: Some(mapped_control.clone()),
        pressed: Some(pressed),
        value: None,
        ticks: None,
    });

    output.buttons.push(GilrsButtonEvent { name: mapped_control, device_id, pressed, timestamp });
}

fn device_timestamp_from_system_time(event_time: SystemTime) -> DeviceTimestamp {
    let now_mono = monotonic_timestamp_ns();
    let now_system = SystemTime::now();
    if let Ok(age) = now_system.duration_since(event_time) {
        DeviceTimestamp::MonotonicNs(now_mono.saturating_sub(age.as_nanos()))
    } else if let Ok(future) = event_time.duration_since(now_system) {
        DeviceTimestamp::MonotonicNs(now_mono.saturating_add(future.as_nanos()))
    } else {
        timeout_device_timestamp()
    }
}

fn timeout_device_timestamp() -> DeviceTimestamp {
    DeviceTimestamp::MonotonicNs(monotonic_timestamp_ns())
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
        timestamp: event.timestamp,
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

    fn button_events(events: &[GilrsButtonEvent]) -> Vec<(String, bool)> {
        events.iter().map(|event| (event.name.clone(), event.pressed)).collect()
    }

    fn button_event_timestamps(events: &[GilrsButtonEvent]) -> Vec<DeviceTimestamp> {
        events.iter().map(|event| event.timestamp).collect()
    }

    fn test_timestamp(ns: u128) -> DeviceTimestamp {
        DeviceTimestamp::MonotonicNs(ns)
    }

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
    fn scratch_v2_requires_two_ticks_to_press() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(1, "Axis1", device_id, test_timestamp(1), 100, &mut events);
        assert!(events.is_empty());

        state.apply_movement(1, "Axis1", device_id, test_timestamp(2), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), true)]);
    }

    #[test]
    fn scratch_v2_releases_after_beatoraja_dual_axis_calls_and_represses_same_direction() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(2, "Axis1", device_id, test_timestamp(10), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), true)]);

        events.clear();
        state.advance_to(now + Duration::from_millis(101), 100, device_id, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), false)]);

        events.clear();
        state.advance_to(now + Duration::from_millis(102), 100, device_id, &mut events);
        state.apply_movement(2, "Axis1", device_id, test_timestamp(12), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), true)]);
    }

    #[test]
    fn scratch_v2_does_not_accumulate_partial_tick_after_beatoraja_threshold_window() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(1, "Axis1", device_id, test_timestamp(20), 100, &mut events);
        assert!(events.is_empty());

        state.advance_to(now + Duration::from_millis(51), 100, device_id, &mut events);
        state.apply_movement(1, "Axis1", device_id, test_timestamp(21), 100, &mut events);
        assert!(events.is_empty());

        state.apply_movement(1, "Axis1", device_id, test_timestamp(22), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), true)]);
    }

    #[test]
    fn scratch_v2_direction_change_releases_before_opposite_press() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(2, "Axis1", device_id, test_timestamp(30), 100, &mut events);
        events.clear();

        state.apply_movement(-2, "Axis1", device_id, test_timestamp(31), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), false)]);
        assert_eq!(button_event_timestamps(&events), vec![test_timestamp(31)]);

        events.clear();
        state.apply_movement(-2, "Axis1", device_id, test_timestamp(32), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1-".to_string(), true)]);
        assert_eq!(button_event_timestamps(&events), vec![test_timestamp(32)]);
    }

    #[test]
    fn scratch_threshold_is_clamped_to_beatoraja_range() {
        assert_eq!(clamp_analog_scratch_threshold(0), 1);
        assert_eq!(clamp_analog_scratch_threshold(100), 100);
        assert_eq!(clamp_analog_scratch_threshold(5_000), 1_000);
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
    fn to_device_input_event_preserves_event_timestamp() {
        let event = GilrsButtonEvent {
            device_id: DeviceId(16),
            name: "South".to_string(),
            pressed: true,
            timestamp: test_timestamp(123),
        };

        let input = to_device_input_event(&event);

        assert_eq!(input.timestamp, test_timestamp(123));
    }
}
