use std::collections::HashMap;
use std::time::{Duration, Instant};

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl, monotonic_timestamp_ns,
};

pub const GAMEPAD_DEVICE_ID_BASE: u32 = 16;
const BASE_TICK_MAX_SIZE: f32 = 0.009;
const ANALOG_SCRATCH_THRESHOLD_MIN: u32 = 1;
const ANALOG_SCRATCH_THRESHOLD_MAX: u32 = 1_000;
const ANALOG_SCRATCH_CALLS_PER_AXIS_POLL: u32 = 2;

#[derive(Debug, Clone)]
pub struct ConnectedGamepad {
    pub stable_id: String,
    pub backend_id: u32,
    pub device_id: DeviceId,
    pub name: String,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadSlotMap {
    pub slot_device_ids: [Option<DeviceId>; 2],
}

impl GamepadSlotMap {
    pub fn from_device_ids(slot_device_ids: [Option<DeviceId>; 2]) -> Self {
        Self { slot_device_ids }
    }

    /// Legacy gilrs slot indexes used by existing configuration and tests.
    pub fn from_slot_ids(slot_ids: [Option<u32>; 2]) -> Self {
        Self { slot_device_ids: slot_ids.map(|id| id.map(gamepad_device_id_from_backend_index)) }
    }

    pub fn device_id_for_player(self, player_index: u32) -> Option<DeviceId> {
        let slot = player_index.checked_sub(1)? as usize;
        if slot >= self.slot_device_ids.len() {
            return Some(gamepad_device_id_from_backend_index(slot as u32));
        }
        self.slot_device_ids[slot]
            .or_else(|| Some(gamepad_device_id_from_backend_index(slot as u32)))
    }
}

pub fn gamepad_device_id_from_backend_index(index: u32) -> DeviceId {
    DeviceId(GAMEPAD_DEVICE_ID_BASE.saturating_add(index))
}

pub fn resolve_gamepad_slot_device_ids(
    mut configured: [Option<DeviceId>; 2],
    connected_device_ids: impl IntoIterator<Item = DeviceId>,
) -> [Option<DeviceId>; 2] {
    let connected: Vec<DeviceId> = connected_device_ids.into_iter().collect();
    for slot in 0..configured.len() {
        if configured[slot].is_some() {
            continue;
        }
        configured[slot] = connected
            .iter()
            .copied()
            .find(|id| !configured.iter().flatten().any(|assigned| assigned == id));
    }
    configured
}

pub fn resolve_gamepad_slot_backend_ids(
    mut configured: [Option<u32>; 2],
    connected_backend_ids: impl IntoIterator<Item = u32>,
) -> [Option<u32>; 2] {
    let connected: Vec<u32> = connected_backend_ids.into_iter().collect();
    for slot in 0..configured.len() {
        if configured[slot].is_some() {
            continue;
        }
        configured[slot] = connected
            .iter()
            .copied()
            .find(|id| !configured.iter().flatten().any(|assigned| assigned == id));
    }
    configured
}

#[derive(Debug, Clone)]
pub struct GamepadButtonEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub pressed: bool,
    pub timestamp: DeviceTimestamp,
}

#[derive(Debug, Clone)]
pub struct RawControlCode {
    pub value: u32,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawInputEventKind {
    Button,
    Axis,
}

impl RawInputEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::Axis => "axis",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawInputEvent {
    pub device_id: DeviceId,
    pub kind: RawInputEventKind,
    pub logical: String,
    pub raw_code: RawControlCode,
    pub timestamp: DeviceTimestamp,
    pub mapped_control: Option<String>,
    pub pressed: Option<bool>,
    pub value: Option<f32>,
    pub ticks: Option<i32>,
}

pub struct GamepadAxisTickEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub timestamp: DeviceTimestamp,
    pub ticks: i32,
}

#[derive(Default)]
pub struct GamepadPollOutput {
    pub buttons: Vec<GamepadButtonEvent>,
    pub axis_ticks: Vec<GamepadAxisTickEvent>,
    pub raw_events: Vec<RawInputEvent>,
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

pub struct AnalogGamepadProcessor {
    axis_prev: HashMap<(DeviceId, u32), f32>,
    scratch_state: HashMap<(DeviceId, u32), ScratchState>,
    tick_max_size: f32,
    scratch_threshold: u32,
}

impl AnalogGamepadProcessor {
    pub fn new(sensitivity: f32, scratch_threshold: u32) -> Self {
        Self {
            axis_prev: HashMap::new(),
            scratch_state: HashMap::new(),
            tick_max_size: BASE_TICK_MAX_SIZE / sensitivity.max(0.01),
            scratch_threshold: clamp_analog_scratch_threshold(scratch_threshold),
        }
    }

    pub fn process_axis(
        &mut self,
        device_id: DeviceId,
        axis_key: u32,
        axis_name: &str,
        logical: String,
        raw_code: RawControlCode,
        value: f32,
        timestamp: DeviceTimestamp,
        output: &mut GamepadPollOutput,
    ) {
        let prev = self.axis_prev.entry((device_id, axis_key)).or_insert(value);
        let ticks = compute_analog_diff(*prev, value, self.tick_max_size);
        *prev = value;
        if ticks == 0 {
            return;
        }

        output.raw_events.push(RawInputEvent {
            device_id,
            kind: RawInputEventKind::Axis,
            logical,
            raw_code,
            timestamp,
            mapped_control: Some(axis_name.to_string()),
            pressed: None,
            value: Some(value),
            ticks: Some(ticks),
        });
        output.axis_ticks.push(GamepadAxisTickEvent {
            name: axis_name.to_string(),
            device_id,
            timestamp,
            ticks,
        });

        let now = Instant::now();
        let state = self.scratch_state.entry((device_id, axis_key)).or_default();
        state.advance_to(now, self.scratch_threshold, device_id, &mut output.buttons);
        state.apply_movement(
            ticks,
            axis_name,
            device_id,
            timestamp,
            self.scratch_threshold,
            &mut output.buttons,
        );
    }

    pub fn check_timeouts(&mut self, now: Instant, events: &mut Vec<GamepadButtonEvent>) {
        for ((device_id, _axis), state) in &mut self.scratch_state {
            state.advance_to(now, self.scratch_threshold, *device_id, events);
        }
    }

    pub fn release_device(
        &mut self,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        for ((state_device_id, _axis), state) in &mut self.scratch_state {
            if *state_device_id == device_id {
                state.release_if_active_at(device_id, timestamp, events);
            }
        }
        self.axis_prev.retain(|(state_device_id, _), _| *state_device_id != device_id);
    }
}

impl ScratchState {
    fn advance_to(
        &mut self,
        now: Instant,
        threshold: u32,
        device_id: DeviceId,
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        let elapsed = self
            .last_counter_update
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or_default();
        self.last_counter_update = Some(now);

        let elapsed_ticks =
            duration_millis_u32(elapsed).saturating_mul(ANALOG_SCRATCH_CALLS_PER_AXIS_POLL);
        if elapsed_ticks > 0 {
            self.counter = self.counter.saturating_add(elapsed_ticks);
        }

        if self.counter > threshold.saturating_mul(2) {
            self.release_if_active_at(device_id, current_device_timestamp(), events);
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
        events: &mut Vec<GamepadButtonEvent>,
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

    fn release_if_active_at(
        &mut self,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GamepadButtonEvent>,
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
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        if let Some(axis_name) = self.control_name.as_deref() {
            let name = format!("{}{}", axis_name, if self.positive_direction { "+" } else { "-" });
            events.push(GamepadButtonEvent { name, device_id, pressed, timestamp });
        }
    }
}

pub fn to_device_input_event(event: &GamepadButtonEvent) -> DeviceInputEvent {
    DeviceInputEvent {
        device: event.device_id,
        control: PhysicalControl::GamepadButton(event.name.clone()),
        kind: if event.pressed { InputKind::Press } else { InputKind::Release },
        timestamp: event.timestamp,
    }
}

pub fn current_device_timestamp() -> DeviceTimestamp {
    DeviceTimestamp::MonotonicNs(monotonic_timestamp_ns())
}

fn clamp_analog_scratch_threshold(value: u32) -> u32 {
    value.clamp(ANALOG_SCRATCH_THRESHOLD_MIN, ANALOG_SCRATCH_THRESHOLD_MAX)
}

fn duration_millis_u32(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn button_events(events: &[GamepadButtonEvent]) -> Vec<(String, bool)> {
        events.iter().map(|event| (event.name.clone(), event.pressed)).collect()
    }

    fn event_timestamp(ns: u128) -> DeviceTimestamp {
        DeviceTimestamp::MonotonicNs(ns)
    }

    #[test]
    fn slot_resolution_uses_connected_devices_without_duplicates() {
        assert_eq!(
            resolve_gamepad_slot_device_ids(
                [None, Some(DeviceId(18))],
                [DeviceId(16), DeviceId(18)]
            ),
            [Some(DeviceId(16)), Some(DeviceId(18))]
        );
    }

    #[test]
    fn analog_diff_wraps_at_axis_range_edges() {
        assert_eq!(compute_analog_diff(0.99, -0.99, BASE_TICK_MAX_SIZE), 3);
        assert_eq!(compute_analog_diff(-0.99, 0.99, BASE_TICK_MAX_SIZE), -3);
    }

    #[test]
    fn scratch_requires_two_ticks_to_press() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(1, "Axis1", device_id, event_timestamp(1), 100, &mut events);
        assert!(events.is_empty());
        state.apply_movement(1, "Axis1", device_id, event_timestamp(2), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), true)]);
    }

    #[test]
    fn scratch_releases_after_beatoraja_dual_axis_calls() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(2, "Axis1", device_id, event_timestamp(10), 100, &mut events);
        events.clear();
        state.advance_to(now + Duration::from_millis(101), 100, device_id, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), false)]);
    }

    #[test]
    fn scratch_direction_change_releases_before_opposite_press() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(2, "Axis1", device_id, event_timestamp(30), 100, &mut events);
        events.clear();
        state.apply_movement(-2, "Axis1", device_id, event_timestamp(31), 100, &mut events);
        assert_eq!(button_events(&events), vec![("Axis1+".to_string(), false)]);
        state.apply_movement(-2, "Axis1", device_id, event_timestamp(32), 100, &mut events);
        assert_eq!(button_events(&events).last(), Some(&("Axis1-".to_string(), true)));
    }

    #[test]
    fn scratch_threshold_is_clamped_to_beatoraja_range() {
        assert_eq!(clamp_analog_scratch_threshold(0), 1);
        assert_eq!(clamp_analog_scratch_threshold(100), 100);
        assert_eq!(clamp_analog_scratch_threshold(5_000), 1_000);
    }
}
