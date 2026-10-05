use std::time::{Instant, SystemTime};

use bmz_gameplay::input::backend::{DeviceId, DeviceTimestamp, monotonic_timestamp_ns};
use gilrs::{Axis, Button, EventType};

use super::gamepad::{
    AnalogGamepadProcessor, ConnectedGamepad, GamepadButtonEvent, GamepadPollOutput,
    GamepadScratchConfig, RawControlCode, RawInputEvent, RawInputEventKind,
    current_device_timestamp, gamepad_device_id_from_backend_index,
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
    #[cfg(target_os = "linux")]
    pending: Option<gilrs::Event>,
    #[cfg(target_os = "linux")]
    early_returns: u32,
    #[cfg(target_os = "linux")]
    held_buttons: std::collections::HashMap<(DeviceId, String), GilrsButtonEvent>,
    #[cfg(target_os = "linux")]
    diagnostics: Option<LinuxDiagnostics>,
}

impl GilrsBackend {
    pub fn new(configs: [GamepadScratchConfig; 2]) -> Result<Self, Box<gilrs::Error>> {
        let gilrs =
            gilrs::GilrsBuilder::new().with_default_filters(false).build().map_err(Box::new)?;
        Ok(Self {
            gilrs,
            analog: AnalogGamepadProcessor::new(configs, GamepadSlotMap::default()),
            #[cfg(target_os = "linux")]
            pending: None,
            #[cfg(target_os = "linux")]
            early_returns: 0,
            #[cfg(target_os = "linux")]
            held_buttons: Default::default(),
            #[cfg(target_os = "linux")]
            diagnostics: bmz_core::latency::diagnostics_enabled().then(LinuxDiagnostics::default),
        })
    }

    pub fn set_analog_config(&mut self, configs: [GamepadScratchConfig; 2], slots: GamepadSlotMap) {
        self.analog.set_config(configs, slots);
    }

    pub fn poll(&mut self) -> GilrsPollOutput {
        #[cfg(target_os = "linux")]
        if let Some(diagnostics) = &mut self.diagnostics {
            diagnostics.poll_cycles += 1;
        }
        let mut output = GilrsPollOutput::default();
        self.analog.check_timeouts(Instant::now(), &mut output.buttons);
        #[cfg(target_os = "linux")]
        let mut first = self.pending.take();
        #[cfg(not(target_os = "linux"))]
        let mut first = None;
        while let Some(gilrs::Event { id, event, time }) =
            first.take().or_else(|| self.gilrs.next_event())
        {
            #[cfg(target_os = "linux")]
            if matches!(
                event,
                EventType::ButtonPressed(..)
                    | EventType::ButtonReleased(..)
                    | EventType::AxisChanged(..)
            ) && let Some(diagnostics) = &mut self.diagnostics
            {
                diagnostics.event(time);
            }
            let timestamp = device_timestamp_from_system_time(time);
            #[cfg(target_os = "linux")]
            let first_button = output.buttons.len();
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
                    #[cfg(target_os = "linux")]
                    {
                        let device = gilrs_gamepad_device_id(id);
                        self.analog.release_device(device, timestamp, &mut output.buttons);
                        release_disconnected_buttons(
                            &mut self.held_buttons,
                            device,
                            timestamp,
                            &mut output.buttons,
                        );
                    }
                    tracing::info!(gamepad = ?id, "gamepad disconnected");
                }
                _ => {}
            }
            #[cfg(target_os = "linux")]
            for button in &output.buttons[first_button..] {
                if button.synthesized_analog_axis || !button.name.starts_with("Button") {
                    continue;
                }
                let key = (button.device_id, button.name.clone());
                if button.pressed {
                    self.held_buttons.insert(key, button.clone());
                } else {
                    self.held_buttons.remove(&key);
                }
            }
        }
        self.analog.check_timeouts(Instant::now(), &mut output.buttons);
        output
    }

    /// gilrs owns its epoll fd; unpark cannot interrupt it. Bound control/exit
    /// latency and wake at the existing scratch release deadline.
    #[cfg(target_os = "linux")]
    pub(super) fn wait_for_input(&mut self) {
        let timeout = linux_wait_timeout(self.analog.next_timeout(Instant::now()));
        let start = Instant::now();
        self.pending = self.gilrs.next_event_blocking(Some(timeout));
        let elapsed = start.elapsed();
        if let Some(diagnostics) = &mut self.diagnostics {
            diagnostics.waits += 1;
            diagnostics.empty_wakes += u64::from(self.pending.is_none());
            diagnostics.early_empty_wakes +=
                u64::from(self.pending.is_none() && elapsed < Duration::from_millis(1));
            if diagnostics.logged.elapsed() >= Duration::from_secs(5) {
                diagnostics.log();
            }
        }
        self.early_returns = if self.pending.is_none() && elapsed < Duration::from_millis(1) {
            self.early_returns.saturating_add(1)
        } else {
            0
        };
        if let Some(backoff) =
            early_return_backoff(self.early_returns, timeout.saturating_sub(elapsed))
        {
            // Error/unknown-event loops must not spin. This is a recovery bound,
            // not a sleep after each event; real pending events are never delayed.
            std::thread::park_timeout(backoff);
        }
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

#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
struct LinuxDiagnostics {
    poll_cycles: u64,
    age: bmz_core::latency::LatencyHistogram,
    invalid: u64,
    waits: u64,
    empty_wakes: u64,
    early_empty_wakes: u64,
    logged: Instant,
    wall: SystemTime,
    mono: Instant,
    epoch_start: SystemTime,
    generation: u128,
}
#[cfg(target_os = "linux")]
impl Default for LinuxDiagnostics {
    fn default() -> Self {
        Self {
            poll_cycles: 0,
            age: Default::default(),
            invalid: 0,
            waits: 0,
            empty_wakes: 0,
            early_empty_wakes: 0,
            logged: Instant::now(),
            wall: SystemTime::now(),
            mono: Instant::now(),
            epoch_start: SystemTime::now(),
            generation: monotonic_timestamp_ns(),
        }
    }
}
#[cfg(target_os = "linux")]
impl LinuxDiagnostics {
    fn event(&mut self, time: SystemTime) {
        let (wall, mono) = (SystemTime::now(), Instant::now());
        let continuous = wall.duration_since(self.wall).ok().is_some_and(|elapsed| {
            elapsed.abs_diff(mono.duration_since(self.mono)) <= Duration::from_millis(2)
        });
        (self.wall, self.mono) = (wall, mono);
        if !continuous {
            self.log();
            self.age = Default::default();
            self.generation = monotonic_timestamp_ns();
            self.epoch_start = wall;
        }
        match wall.duration_since(time).ok().filter(|_| continuous && time >= self.epoch_start) {
            Some(age) => self.age.record(age.as_nanos().min(u64::MAX as u128) as u64),
            None => self.invalid += 1,
        }
    }
    fn log(&mut self) {
        let summary = serde_json::json!({"schema":1,"kind":"gilrs","generation":self.generation,
            "poll_cycles":self.poll_cycles,
            "os_to_receive_ns":self.age.summary(),"invalid_timestamps":self.invalid,
            "blocking_waits":self.waits,"empty_wakes":self.empty_wakes,"early_empty_wakes":self.early_empty_wakes,
            "clock":"gilrs SystemTime / Linux default EV_KEY CLOCK_REALTIME; discontinuity checked, not CLOCK_MONOTONIC",
            "physical_switch_time_ns":null});
        tracing::info!("BMZ_LATENCY_JSON {summary}");
        self.logged = Instant::now();
    }
}
#[cfg(target_os = "linux")]
impl Drop for LinuxDiagnostics {
    fn drop(&mut self) {
        self.log();
    }
}

#[cfg(target_os = "linux")]
fn release_disconnected_buttons(
    held: &mut std::collections::HashMap<(DeviceId, String), GilrsButtonEvent>,
    device: DeviceId,
    timestamp: DeviceTimestamp,
    output: &mut Vec<GilrsButtonEvent>,
) {
    held.retain(|(id, _), event| {
        if *id != device {
            return true;
        }
        let mut release = event.clone();
        release.pressed = false;
        release.timestamp = timestamp;
        output.push(release);
        false
    });
}

#[cfg(target_os = "linux")]
fn linux_wait_timeout(deadline: Option<Duration>) -> Duration {
    let bound = Duration::from_millis(50);
    let duration = deadline.unwrap_or(bound).min(bound);
    // gilrs-core converts Duration to integral epoll milliseconds (truncating).
    Duration::from_millis(duration.as_nanos().div_ceil(1_000_000).clamp(1, 50) as u64)
}

#[cfg(target_os = "linux")]
fn early_return_backoff(count: u32, remaining: Duration) -> Option<Duration> {
    (count >= 3 && !remaining.is_zero()).then_some(remaining.min(Duration::from_millis(10)))
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

    #[cfg(target_os = "linux")]
    #[test]
    fn gilrs_future_and_initial_timestamps_are_not_zero_age_samples() {
        let mut diagnostics = LinuxDiagnostics::default();
        diagnostics.event(SystemTime::UNIX_EPOCH);
        diagnostics.event(SystemTime::now() + Duration::from_secs(5));
        assert_eq!(diagnostics.age.summary().count, 0);
        assert_eq!(diagnostics.invalid, 2);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn disconnected_buttons_release_only_their_device() {
        let mut held = std::collections::HashMap::new();
        for id in [16, 17] {
            held.insert(
                (DeviceId(id), "Button1".into()),
                GilrsButtonEvent {
                    device_id: DeviceId(id),
                    name: "Button1".into(),
                    pressed: true,
                    timestamp: test_timestamp(10),
                    synthesized_analog_axis: false,
                },
            );
        }
        let mut output = Vec::new();
        release_disconnected_buttons(&mut held, DeviceId(16), test_timestamp(20), &mut output);
        assert_eq!(held.len(), 1);
        assert_eq!(output.len(), 1);
        assert!(!output[0].pressed);
        assert_eq!(output[0].timestamp, test_timestamp(20));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn blocking_wait_bounds_idle_exit_and_scratch_deadlines_without_spinning() {
        assert_eq!(linux_wait_timeout(None), Duration::from_millis(50));
        assert_eq!(linux_wait_timeout(Some(Duration::from_micros(1200))), Duration::from_millis(2));
        assert_eq!(linux_wait_timeout(Some(Duration::ZERO)), Duration::from_millis(1));
        assert_eq!(early_return_backoff(2, Duration::from_millis(50)), None);
        assert_eq!(
            early_return_backoff(3, Duration::from_millis(50)),
            Some(Duration::from_millis(10))
        );
        assert_eq!(
            early_return_backoff(100, Duration::from_millis(2)),
            Some(Duration::from_millis(2))
        );
    }

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
