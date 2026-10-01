use std::collections::HashMap;
use std::time::{Duration, Instant};

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, InputBouncePolicy, PhysicalControl,
    monotonic_timestamp_ns,
};

pub const GAMEPAD_DEVICE_ID_BASE: u32 = 16;
const STABLE_GAMEPAD_DEVICE_ID_MASK: u32 = 0x8000_0000;
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

    pub fn from_runtime_or_legacy(
        runtime_device_ids: [Option<u32>; 2],
        legacy_slot_ids: [Option<u32>; 2],
    ) -> Self {
        if runtime_device_ids.iter().any(Option::is_some) {
            Self::from_device_ids(runtime_device_ids.map(|id| id.map(DeviceId)))
        } else {
            Self::from_slot_ids(legacy_slot_ids)
        }
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GamepadScratchConfig {
    pub analog_scratch: bool,
    pub sensitivity: f32,
    pub threshold: u32,
}

impl Default for GamepadScratchConfig {
    fn default() -> Self {
        Self { analog_scratch: true, sensitivity: 1.0, threshold: 100 }
    }
}

pub fn gamepad_device_id_from_backend_index(index: u32) -> DeviceId {
    DeviceId(GAMEPAD_DEVICE_ID_BASE.saturating_add(index))
}

pub fn gamepad_device_id_from_stable_id(stable_id: &str) -> DeviceId {
    let hash = stable_id
        .bytes()
        .fold(2_166_136_261u32, |hash, byte| hash.wrapping_mul(16_777_619) ^ u32::from(byte));
    DeviceId(STABLE_GAMEPAD_DEVICE_ID_MASK | (hash & !STABLE_GAMEPAD_DEVICE_ID_MASK))
}

fn resolve_gamepad_device_id_from_stable_id(
    stable_id: &str,
    use_gilrs_backend_ids: bool,
    known_gamepads: &[ConnectedGamepad],
) -> DeviceId {
    if let Some(gamepad) = known_gamepads.iter().find(|gamepad| gamepad.stable_id == stable_id) {
        return gamepad.device_id;
    }
    if use_gilrs_backend_ids
        && let Some(index) =
            stable_id.strip_prefix("gilrs:").and_then(|value| value.parse::<u32>().ok())
    {
        return gamepad_device_id_from_backend_index(index);
    }
    gamepad_device_id_from_stable_id(stable_id)
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

pub fn resolve_gamepad_slot_assignments(
    stable_ids: [Option<&str>; 2],
    legacy_backend_ids: [Option<u32>; 2],
    using_gilrs: bool,
    include_known_disconnected: bool,
    connected: &[ConnectedGamepad],
) -> [Option<DeviceId>; 2] {
    let configured = std::array::from_fn(|slot| {
        stable_ids[slot]
            .map(|stable_id| {
                resolve_gamepad_device_id_from_stable_id(stable_id, using_gilrs, connected)
            })
            .or_else(|| {
                using_gilrs
                    .then_some(legacy_backend_ids[slot])
                    .flatten()
                    .map(gamepad_device_id_from_backend_index)
            })
    });
    // GCController has no public persistent per-device ID. Multiple devices
    // require explicit runtime assignment, rather than guessing by name/order.
    if connected.iter().any(|pad| pad.stable_id.starts_with("gc-session:"))
        && connected.iter().filter(|pad| pad.is_connected).count() > 1
    {
        return configured;
    }
    resolve_gamepad_slot_device_ids(
        configured,
        connected
            .iter()
            .filter(|device| device.is_connected || include_known_disconnected)
            .map(|device| device.device_id),
    )
}

#[derive(Debug, Clone)]
pub struct GamepadButtonEvent {
    pub name: String,
    pub device_id: DeviceId,
    pub pressed: bool,
    pub timestamp: DeviceTimestamp,
    pub synthesized_analog_axis: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamepadPressedButton {
    pub name: String,
    pub device_id: DeviceId,
}

#[derive(Default)]
pub struct GamepadPollOutput {
    pub buttons: Vec<GamepadButtonEvent>,
    pub axis_ticks: Vec<GamepadAxisTickEvent>,
    pub raw_events: Vec<RawInputEvent>,
    /// バックエンドが把握している現在の物理押下状態。
    ///
    /// `None` はスナップショット非対応、`Some([])` は全ボタン解放を表す。
    pub pressed_buttons: Option<Vec<GamepadPressedButton>>,
}

pub enum GamepadBackend {
    Gilrs(Box<super::gilrs::GilrsBackend>),
    #[cfg(windows)]
    RawInput(Box<super::rawinput::RawInputBackend>),
    #[cfg(all(windows, feature = "experimental-gameinput"))]
    GameInput(Box<super::gameinput::GameInputBackend>),
}

impl GamepadBackend {
    pub fn set_analog_config(&mut self, configs: [GamepadScratchConfig; 2], slots: GamepadSlotMap) {
        match self {
            Self::Gilrs(backend) => backend.set_analog_config(configs, slots),
            #[cfg(windows)]
            Self::RawInput(backend) => {
                backend.set_analog_config(configs, slots);
            }
            #[cfg(all(windows, feature = "experimental-gameinput"))]
            Self::GameInput(backend) => {
                backend.set_analog_config(configs, slots);
            }
        }
    }

    pub fn poll(&mut self) -> GamepadPollOutput {
        match self {
            Self::Gilrs(backend) => backend.poll(),
            #[cfg(windows)]
            Self::RawInput(backend) => backend.poll(),
            #[cfg(all(windows, feature = "experimental-gameinput"))]
            Self::GameInput(backend) => backend.poll(),
        }
    }

    pub fn connected_gamepads(&self) -> Vec<ConnectedGamepad> {
        match self {
            Self::Gilrs(backend) => backend.connected_gamepads(),
            #[cfg(windows)]
            Self::RawInput(backend) => backend.connected_gamepads(),
            #[cfg(all(windows, feature = "experimental-gameinput"))]
            Self::GameInput(backend) => backend.connected_gamepads(),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Gilrs(_) => "gilrs",
            #[cfg(windows)]
            Self::RawInput(_) => "Raw Input",
            #[cfg(all(windows, feature = "experimental-gameinput"))]
            Self::GameInput(_) => "GameInput",
        }
    }

    pub fn is_gilrs(&self) -> bool {
        matches!(self, Self::Gilrs(_))
    }

    pub fn attach_window(&mut self, window: &winit::window::Window) -> anyhow::Result<()> {
        #[cfg(not(windows))]
        let _ = window;

        match self {
            Self::Gilrs(_) => Ok(()),
            #[cfg(windows)]
            Self::RawInput(backend) => backend.attach_window(window),
            #[cfg(all(windows, feature = "experimental-gameinput"))]
            Self::GameInput(_) => Ok(()),
        }
    }

    #[cfg(all(windows, feature = "experimental-gameinput"))]
    pub fn gameinput_diagnostics(&self) -> Option<super::gameinput::GameInputPollDiagnostics> {
        match self {
            Self::Gilrs(_) => None,
            Self::RawInput(_) => None,
            Self::GameInput(backend) => Some(backend.diagnostics()),
        }
    }
}

#[derive(Default)]
struct ScratchState {
    active: bool,
    positive_direction: bool,
    control_name: Option<String>,
    counter: u32,
    counter_elapsed_remainder: Duration,
    tick_counter: u32,
    last_counter_update: Option<Instant>,
}

#[derive(Default)]
struct DigitalAxisState {
    positive_pressed: bool,
    negative_pressed: bool,
    control_name: Option<String>,
}

pub struct AnalogGamepadProcessor {
    axis_prev: HashMap<(DeviceId, u32), f32>,
    axis_names: HashMap<(DeviceId, u32), String>,
    scratch_state: HashMap<(DeviceId, u32), ScratchState>,
    digital_axis_state: HashMap<(DeviceId, u32), DigitalAxisState>,
    configs: [GamepadScratchConfig; 2],
    slots: GamepadSlotMap,
    pending_buttons: Vec<GamepadButtonEvent>,
}

impl AnalogGamepadProcessor {
    pub fn new(configs: [GamepadScratchConfig; 2], slots: GamepadSlotMap) -> Self {
        Self {
            axis_prev: HashMap::new(),
            axis_names: HashMap::new(),
            scratch_state: HashMap::new(),
            digital_axis_state: HashMap::new(),
            configs,
            slots,
            pending_buttons: Vec::new(),
        }
    }

    pub fn set_config(&mut self, configs: [GamepadScratchConfig; 2], slots: GamepadSlotMap) {
        if self.configs == configs && self.slots == slots {
            return;
        }

        let old_configs = self.configs;
        let old_slots = self.slots;
        let timestamp = current_device_timestamp();
        let axes = self.axis_prev.keys().copied().collect::<Vec<_>>();
        for (device_id, axis_key) in axes {
            let old_analog = config_for_device(old_configs, old_slots, device_id).analog_scratch;
            let new_analog = config_for_device(configs, slots, device_id).analog_scratch;
            if old_analog == new_analog {
                continue;
            }
            if let Some(state) = self.scratch_state.get_mut(&(device_id, axis_key)) {
                state.release_if_active_at(device_id, timestamp, &mut self.pending_buttons);
            }
            if let Some(state) = self.digital_axis_state.get_mut(&(device_id, axis_key)) {
                state.release_all(device_id, timestamp, &mut self.pending_buttons);
            }
            self.scratch_state.remove(&(device_id, axis_key));
            self.digital_axis_state.remove(&(device_id, axis_key));

            if !new_analog
                && let (Some(&value), Some(axis_name)) = (
                    self.axis_prev.get(&(device_id, axis_key)),
                    self.axis_names.get(&(device_id, axis_key)),
                )
            {
                self.digital_axis_state.entry((device_id, axis_key)).or_default().apply_value(
                    value,
                    axis_name,
                    device_id,
                    timestamp,
                    &mut self.pending_buttons,
                );
            }
        }
        self.configs = configs;
        self.slots = slots;
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
        self.process_axis_at(
            device_id,
            axis_key,
            axis_name,
            logical,
            raw_code,
            value,
            timestamp,
            Instant::now(),
            output,
        );
    }

    /// Route baseline without synthesizing a press for an already held axis.
    #[cfg(target_os = "macos")]
    pub(super) fn analog_scratch_enabled(&self, device: DeviceId) -> bool {
        config_for_device(self.configs, self.slots, device).analog_scratch
    }

    #[cfg(target_os = "macos")]
    pub(super) fn seed_axis(&mut self, device: DeviceId, axis: u32, name: &str, value: f32) {
        self.axis_prev.insert((device, axis), value);
        self.axis_names.insert((device, axis), name.to_owned());
    }

    pub(super) fn process_axis_at(
        &mut self,
        device_id: DeviceId,
        axis_key: u32,
        axis_name: &str,
        logical: String,
        raw_code: RawControlCode,
        value: f32,
        timestamp: DeviceTimestamp,
        now: Instant,
        output: &mut GamepadPollOutput,
    ) {
        let key = (device_id, axis_key);
        let config = config_for_device(self.configs, self.slots, device_id);
        let tick_max_size = BASE_TICK_MAX_SIZE / config.sensitivity.max(0.01);
        let previous = self.axis_prev.insert(key, value);
        let changed = previous.is_none_or(|prev| prev.to_bits() != value.to_bits());
        let ticks = previous.map_or(0, |prev| compute_analog_diff(prev, value, tick_max_size));
        self.axis_names.entry(key).or_insert_with(|| axis_name.to_string());
        if !changed && ticks == 0 {
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
            ticks: config.analog_scratch.then_some(ticks),
        });
        if config.analog_scratch {
            if ticks == 0 {
                return;
            }
            output.axis_ticks.push(GamepadAxisTickEvent {
                name: axis_name.to_string(),
                device_id,
                timestamp,
                ticks,
            });

            let threshold = clamp_analog_scratch_threshold(config.threshold);
            let state = self.scratch_state.entry(key).or_default();
            state.advance_to_at(now, threshold, device_id, timestamp, &mut output.buttons);
            state.apply_movement(
                ticks,
                axis_name,
                device_id,
                timestamp,
                threshold,
                &mut output.buttons,
            );
        } else {
            self.digital_axis_state.entry(key).or_default().apply_value(
                value,
                axis_name,
                device_id,
                timestamp,
                &mut output.buttons,
            );
        }
    }

    pub fn check_timeouts(&mut self, now: Instant, events: &mut Vec<GamepadButtonEvent>) {
        events.append(&mut self.pending_buttons);
        let configs = self.configs;
        let slots = self.slots;
        for ((device_id, _axis), state) in &mut self.scratch_state {
            let config = config_for_device(configs, slots, *device_id);
            if config.analog_scratch {
                state.advance_to(
                    now,
                    clamp_analog_scratch_threshold(config.threshold),
                    *device_id,
                    events,
                );
            }
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
        for ((state_device_id, _axis), state) in &mut self.digital_axis_state {
            if *state_device_id == device_id {
                state.release_all(device_id, timestamp, events);
            }
        }
        self.axis_prev.retain(|(state_device_id, _), _| *state_device_id != device_id);
        self.axis_names.retain(|(state_device_id, _), _| *state_device_id != device_id);
        self.scratch_state.retain(|(state_device_id, _), _| *state_device_id != device_id);
        self.digital_axis_state.retain(|(state_device_id, _), _| *state_device_id != device_id);
    }

    pub fn pressed_buttons(&self) -> Vec<GamepadPressedButton> {
        let mut pressed = self
            .scratch_state
            .iter()
            .filter_map(|((device_id, _axis), state)| {
                if !state.active {
                    return None;
                }
                let axis_name = state.control_name.as_deref()?;
                Some(GamepadPressedButton {
                    name: format!(
                        "{}{}",
                        axis_name,
                        if state.positive_direction { "+" } else { "-" }
                    ),
                    device_id: *device_id,
                })
            })
            .collect::<Vec<_>>();
        for ((device_id, _axis), state) in &self.digital_axis_state {
            let Some(axis_name) = state.control_name.as_deref() else { continue };
            if state.positive_pressed {
                pressed.push(GamepadPressedButton {
                    name: format!("{axis_name}+"),
                    device_id: *device_id,
                });
            }
            if state.negative_pressed {
                pressed.push(GamepadPressedButton {
                    name: format!("{axis_name}-"),
                    device_id: *device_id,
                });
            }
        }
        pressed
    }
}

fn config_for_device(
    configs: [GamepadScratchConfig; 2],
    slots: GamepadSlotMap,
    device_id: DeviceId,
) -> GamepadScratchConfig {
    if slots.slot_device_ids[0] == Some(device_id) {
        configs[0]
    } else if slots.slot_device_ids[1] == Some(device_id) {
        configs[1]
    } else {
        // SPのgamepadワイルドカードと未割当デバイスは1P設定を使う。
        configs[0]
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
        self.advance_to_at(now, threshold, device_id, current_device_timestamp(), events);
    }

    fn advance_to_at(
        &mut self,
        now: Instant,
        threshold: u32,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        let elapsed = self
            .last_counter_update
            .map(|last| now.saturating_duration_since(last))
            .unwrap_or_default();
        self.last_counter_update = Some(now);

        let accumulated_elapsed = self.counter_elapsed_remainder.saturating_add(elapsed);
        let elapsed_millis = duration_millis_u32(accumulated_elapsed);
        self.counter_elapsed_remainder =
            accumulated_elapsed.saturating_sub(Duration::from_millis(u64::from(elapsed_millis)));
        let elapsed_ticks = elapsed_millis.saturating_mul(ANALOG_SCRATCH_CALLS_PER_AXIS_POLL);
        if elapsed_ticks > 0 {
            self.counter = self.counter.saturating_add(elapsed_ticks);
        }

        if self.counter > threshold.saturating_mul(2) {
            self.release_if_active_at(device_id, timestamp, events);
            self.tick_counter = 0;
            self.counter = 0;
            self.counter_elapsed_remainder = Duration::ZERO;
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
        self.counter_elapsed_remainder = Duration::ZERO;
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
            events.push(GamepadButtonEvent {
                name,
                device_id,
                pressed,
                timestamp,
                synthesized_analog_axis: true,
            });
        }
    }
}

impl DigitalAxisState {
    fn apply_value(
        &mut self,
        value: f32,
        axis_name: &str,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        self.control_name.get_or_insert_with(|| axis_name.to_string());
        let positive = value > 0.9;
        let negative = value < -0.9;

        if self.positive_pressed && !positive {
            self.push_button_event(device_id, false, true, timestamp, events);
        }
        if self.negative_pressed && !negative {
            self.push_button_event(device_id, false, false, timestamp, events);
        }
        if !self.positive_pressed && positive {
            self.push_button_event(device_id, true, true, timestamp, events);
        }
        if !self.negative_pressed && negative {
            self.push_button_event(device_id, true, false, timestamp, events);
        }
        self.positive_pressed = positive;
        self.negative_pressed = negative;
    }

    fn release_all(
        &mut self,
        device_id: DeviceId,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        if self.positive_pressed {
            self.push_button_event(device_id, false, true, timestamp, events);
            self.positive_pressed = false;
        }
        if self.negative_pressed {
            self.push_button_event(device_id, false, false, timestamp, events);
            self.negative_pressed = false;
        }
    }

    fn push_button_event(
        &self,
        device_id: DeviceId,
        pressed: bool,
        positive: bool,
        timestamp: DeviceTimestamp,
        events: &mut Vec<GamepadButtonEvent>,
    ) {
        if let Some(axis_name) = self.control_name.as_deref() {
            events.push(GamepadButtonEvent {
                name: format!("{}{}", axis_name, if positive { "+" } else { "-" }),
                device_id,
                pressed,
                timestamp,
                // OFFは端点を通常のコントローラーボタンとして扱う。
                synthesized_analog_axis: false,
            });
        }
    }
}

pub fn to_device_input_event(event: &GamepadButtonEvent) -> DeviceInputEvent {
    DeviceInputEvent {
        device: event.device_id,
        control: PhysicalControl::GamepadButton(event.name.clone()),
        kind: if event.pressed { InputKind::Press } else { InputKind::Release },
        timestamp: event.timestamp,
        bounce_policy: InputBouncePolicy::Apply,
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

    fn process_axis(
        processor: &mut AnalogGamepadProcessor,
        device_id: DeviceId,
        value: f32,
        timestamp: u128,
        output: &mut GamepadPollOutput,
    ) {
        processor.process_axis(
            device_id,
            0,
            "Axis1",
            "Axis1".to_string(),
            RawControlCode { value: 0, label: "Axis(0)".to_string() },
            value,
            event_timestamp(timestamp),
            output,
        );
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
    fn multiple_gamecontroller_devices_require_explicit_runtime_assignment() {
        let pads: Vec<_> = (1..=2)
            .map(|i| ConnectedGamepad {
                stable_id: format!("gc-session:test:{i}"),
                backend_id: i,
                device_id: DeviceId(0x4000_0000 + i),
                name: "Identical pad".into(),
                is_connected: true,
            })
            .collect();
        assert_eq!(
            resolve_gamepad_slot_assignments([None, None], [None, None], false, false, &pads),
            [None, None]
        );
        assert_eq!(
            resolve_gamepad_slot_assignments(
                [Some(&pads[1].stable_id), None],
                [None, None],
                false,
                false,
                &pads
            ),
            [Some(pads[1].device_id), None]
        );
    }
    #[test]
    fn buffered_axis_timeout_uses_sample_time_instead_of_drain_time() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device = DeviceId(16);
        let sample = Instant::now() - Duration::from_secs(1);
        state.advance_to_at(sample, 100, device, event_timestamp(1), &mut events);
        state.apply_movement(2, "GCAxisLeftX", device, event_timestamp(2), 100, &mut events);
        state.advance_to_at(
            sample + Duration::from_millis(101),
            100,
            device,
            event_timestamp(3),
            &mut events,
        );
        assert_eq!(
            button_events(&events),
            vec![("GCAxisLeftX+".into(), true), ("GCAxisLeftX+".into(), false)]
        );
        assert_eq!(events[1].timestamp, event_timestamp(3));
    }

    #[test]
    fn stable_gamepad_device_id_is_deterministic_and_separate_from_gilrs_range() {
        let first = gamepad_device_id_from_stable_id("gameinput:controller-a");
        let second = gamepad_device_id_from_stable_id("gameinput:controller-a");
        assert_eq!(first, second);
        assert_ne!(first, gamepad_device_id_from_backend_index(0));
        assert_ne!(first, gamepad_device_id_from_stable_id("gameinput:controller-b"));
    }

    #[test]
    fn gilrs_stable_slots_use_connected_device_ids() {
        let connected = [
            ConnectedGamepad {
                stable_id: "gilrs:0".to_string(),
                backend_id: 0,
                device_id: DeviceId(16),
                name: "1P controller".to_string(),
                is_connected: true,
            },
            ConnectedGamepad {
                stable_id: "gilrs:1".to_string(),
                backend_id: 1,
                device_id: DeviceId(17),
                name: "2P controller".to_string(),
                is_connected: true,
            },
        ];

        let slots = resolve_gamepad_slot_assignments(
            [Some("gilrs:0"), Some("gilrs:1")],
            [None, None],
            true,
            false,
            &connected,
        );

        assert_eq!(slots, [Some(DeviceId(16)), Some(DeviceId(17))]);
    }

    #[test]
    fn gilrs_stable_slots_support_swapped_assignments() {
        let connected = [
            ConnectedGamepad {
                stable_id: "gilrs:0".to_string(),
                backend_id: 0,
                device_id: DeviceId(16),
                name: "1P controller".to_string(),
                is_connected: true,
            },
            ConnectedGamepad {
                stable_id: "gilrs:1".to_string(),
                backend_id: 1,
                device_id: DeviceId(17),
                name: "2P controller".to_string(),
                is_connected: true,
            },
        ];

        let slots = resolve_gamepad_slot_assignments(
            [Some("gilrs:1"), Some("gilrs:0")],
            [None, None],
            true,
            false,
            &connected,
        );

        assert_eq!(slots, [Some(DeviceId(17)), Some(DeviceId(16))]);
    }

    #[test]
    fn gilrs_stable_slots_fall_back_to_backend_indexes_when_not_discovered() {
        let slots = resolve_gamepad_slot_assignments(
            [Some("gilrs:0"), Some("gilrs:1")],
            [None, None],
            true,
            false,
            &[],
        );

        assert_eq!(slots, [Some(DeviceId(16)), Some(DeviceId(17))]);
    }

    #[test]
    fn stable_slot_is_kept_when_device_is_disconnected() {
        let slots = resolve_gamepad_slot_assignments(
            [Some("gameinput:controller-a"), None],
            [None, None],
            false,
            false,
            &[],
        );
        assert_eq!(slots[0], Some(gamepad_device_id_from_stable_id("gameinput:controller-a")));
    }

    #[test]
    fn gameinput_ignores_legacy_gilrs_slot_and_uses_connected_device() {
        let connected = [ConnectedGamepad {
            stable_id: "gameinput:controller-a".to_string(),
            backend_id: 4,
            device_id: gamepad_device_id_from_stable_id("gameinput:controller-a"),
            name: "controller".to_string(),
            is_connected: true,
        }];
        let slots = resolve_gamepad_slot_assignments(
            [None, None],
            [Some(1), None],
            false,
            true,
            &connected,
        );
        assert_eq!(slots[0], Some(connected[0].device_id));
    }

    #[test]
    fn gameinput_keeps_known_device_slot_during_transient_disconnect() {
        let known = [ConnectedGamepad {
            stable_id: "gameinput:controller-a".to_string(),
            backend_id: 0,
            device_id: gamepad_device_id_from_stable_id("gameinput:controller-a"),
            name: "controller".to_string(),
            is_connected: false,
        }];

        let slots =
            resolve_gamepad_slot_assignments([None, None], [None, None], false, true, &known);

        assert_eq!(slots[0], Some(known[0].device_id));
    }

    #[test]
    fn analog_diff_wraps_at_axis_range_edges() {
        assert_eq!(compute_analog_diff(0.99, -0.99, BASE_TICK_MAX_SIZE), 3);
        assert_eq!(compute_analog_diff(-0.99, 0.99, BASE_TICK_MAX_SIZE), -3);
    }

    #[test]
    fn analog_scratch_off_uses_axis_endpoints_without_tick_scroll() {
        let off = GamepadScratchConfig { analog_scratch: false, ..Default::default() };
        let mut processor = AnalogGamepadProcessor::new(
            [off, GamepadScratchConfig::default()],
            GamepadSlotMap::default(),
        );
        let mut output = GamepadPollOutput::default();

        process_axis(&mut processor, DeviceId(16), 0.0, 1, &mut output);
        process_axis(&mut processor, DeviceId(16), 0.91, 2, &mut output);
        process_axis(&mut processor, DeviceId(16), 0.5, 3, &mut output);
        process_axis(&mut processor, DeviceId(16), -0.91, 4, &mut output);

        assert_eq!(
            button_events(&output.buttons),
            vec![
                ("Axis1+".to_string(), true),
                ("Axis1+".to_string(), false),
                ("Axis1-".to_string(), true),
            ]
        );
        assert!(output.axis_ticks.is_empty());
        assert!(output.buttons.iter().all(|event| !event.synthesized_analog_axis));
        assert_eq!(
            processor.pressed_buttons(),
            vec![GamepadPressedButton { name: "Axis1-".to_string(), device_id: DeviceId(16) }]
        );
    }

    #[test]
    fn per_player_analog_modes_follow_logical_slots() {
        let configs = [
            GamepadScratchConfig { analog_scratch: false, ..Default::default() },
            GamepadScratchConfig::default(),
        ];
        let slots = GamepadSlotMap::from_device_ids([Some(DeviceId(30)), Some(DeviceId(20))]);
        let mut processor = AnalogGamepadProcessor::new(configs, slots);
        let mut output = GamepadPollOutput::default();

        process_axis(&mut processor, DeviceId(30), 0.0, 1, &mut output);
        process_axis(&mut processor, DeviceId(30), 0.02, 2, &mut output);
        assert!(output.axis_ticks.is_empty());
        assert!(output.buttons.is_empty());

        process_axis(&mut processor, DeviceId(20), 0.0, 3, &mut output);
        process_axis(&mut processor, DeviceId(20), 0.02, 4, &mut output);
        assert_eq!(output.axis_ticks.len(), 1);
        assert_eq!(button_events(&output.buttons), vec![("Axis1+".to_string(), true)]);
    }

    #[test]
    fn changing_analog_mode_releases_old_state_and_applies_current_endpoint() {
        let analog = GamepadScratchConfig::default();
        let off = GamepadScratchConfig { analog_scratch: false, ..Default::default() };
        let mut processor = AnalogGamepadProcessor::new([off, analog], GamepadSlotMap::default());
        let mut output = GamepadPollOutput::default();

        process_axis(&mut processor, DeviceId(16), 0.95, 1, &mut output);
        output.buttons.clear();
        processor.set_config([analog, analog], GamepadSlotMap::default());
        processor.check_timeouts(Instant::now(), &mut output.buttons);
        assert_eq!(button_events(&output.buttons), vec![("Axis1+".to_string(), false)]);

        output.buttons.clear();
        processor.set_config([off, analog], GamepadSlotMap::default());
        processor.check_timeouts(Instant::now(), &mut output.buttons);
        assert_eq!(button_events(&output.buttons), vec![("Axis1+".to_string(), true)]);
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
        assert!(events.iter().all(|event| event.synthesized_analog_axis));
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
    fn scratch_release_accumulates_sub_millisecond_poll_intervals() {
        let mut state = ScratchState::default();
        let mut events = Vec::new();
        let device_id = DeviceId(16);
        let now = Instant::now();

        state.advance_to(now, 100, device_id, &mut events);
        state.apply_movement(2, "Axis1", device_id, event_timestamp(20), 100, &mut events);
        events.clear();

        for elapsed_us in (100_u64..=101_000).step_by(100) {
            state.advance_to(now + Duration::from_micros(elapsed_us), 100, device_id, &mut events);
        }

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
    fn analog_config_updates_without_resetting_axis_state() {
        let mut processor = AnalogGamepadProcessor::new(
            [GamepadScratchConfig::default(); 2],
            GamepadSlotMap::default(),
        );
        processor.axis_prev.insert((DeviceId(16), 1), 0.5);

        let configs = [
            GamepadScratchConfig { sensitivity: 2.0, threshold: 250, ..Default::default() },
            GamepadScratchConfig::default(),
        ];
        processor.set_config(configs, GamepadSlotMap::default());

        assert_eq!(processor.configs[0], configs[0]);
        assert_eq!(processor.axis_prev.get(&(DeviceId(16), 1)), Some(&0.5));
    }

    #[test]
    fn scratch_threshold_is_clamped_to_beatoraja_range() {
        assert_eq!(clamp_analog_scratch_threshold(0), 1);
        assert_eq!(clamp_analog_scratch_threshold(100), 100);
        assert_eq!(clamp_analog_scratch_threshold(5_000), 1_000);
    }
}
