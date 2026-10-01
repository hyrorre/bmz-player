use super::super::{
    capture::{ButtonDelivery, InputRoute},
    gamepad::*,
    macos_clock::MachClock,
};
use bmz_gameplay::input::backend::{
    DeviceId, DeviceTimestamp, InputBouncePolicy, monotonic_timestamp_ns,
};
use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, c_char, c_void},
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn bmz_gc_pads_open(
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void, u64, u32, i32, u32, f32, f64, *const c_char),
    ) -> *mut c_void;
    fn bmz_gc_pads_generation(handle: *mut c_void, generation: u64);
    fn bmz_gc_pads_close(handle: *mut c_void);
    fn bmz_keyboard_ticks(numer: *mut u32, denom: *mut u32) -> u64;
}
static PAD_STATUS: AtomicU8 = AtomicU8::new(0);
pub fn pad_status() -> u8 {
    PAD_STATUS.load(Ordering::Acquire)
}

pub struct Pads {
    handle: usize,
    context: Box<Mutex<PadState>>,
}
impl Pads {
    pub fn start(configs: [GamepadScratchConfig; 2]) -> Option<Self> {
        let (ticks, numer, denom) = ticks();
        if ticks == 0 || numer == 0 || denom == 0 {
            return None;
        }
        PAD_STATUS.store(3, Ordering::Release);
        let mut context = Box::new(Mutex::new(PadState::new(configs, ticks, numer, denom)));
        let handle =
            unsafe { bmz_gc_pads_open((&mut *context as *mut Mutex<PadState>).cast(), pad_event) };
        if handle.is_null() {
            PAD_STATUS.store(0, Ordering::Release);
            tracing::warn!("GameController unavailable (requires macOS 11+); using gilrs");
            return None;
        }
        tracing::info!(
            "GameController opened on dedicated serial queue; macOS 14+ buffered host timestamps, older OS callback receipt timestamps"
        );
        Some(Self { handle: handle as usize, context })
    }
    pub fn set_route(&self, route: Option<InputRoute>) {
        let generation = {
            let mut state = self.context.lock().unwrap_or_else(|e| e.into_inner());
            if super::same_route(state.route.as_ref(), route.as_ref()) {
                return;
            }
            state.reset_all();
            state.generation = state.generation.wrapping_add(1);
            state.route_started = monotonic_timestamp_ns();
            state.route = route;
            state.generation
        };
        unsafe {
            bmz_gc_pads_generation(self.handle as *mut c_void, generation);
        }
    }
    pub fn set_analog_config(&self, configs: [GamepadScratchConfig; 2], slots: GamepadSlotMap) {
        let mut state = self.context.lock().unwrap_or_else(|e| e.into_inner());
        state.analog.set_config(configs, slots);
    }
    pub fn poll(&self) -> GamepadPollOutput {
        self.context
            .try_lock()
            .map(|mut state| {
                let mut output = std::mem::take(&mut state.output);
                output.pressed_buttons = Some(state.pressed_buttons());
                output
            })
            .unwrap_or_default()
    }
    pub fn connected_gamepads(&self) -> Vec<ConnectedGamepad> {
        let state = self.context.lock().unwrap_or_else(|e| e.into_inner());
        let mut pads: Vec<_> = state.devices.values().map(|pad| pad.info.clone()).collect();
        pads.sort_by_key(|pad| pad.backend_id);
        pads
    }
}
impl Drop for Pads {
    fn drop(&mut self) {
        unsafe {
            bmz_gc_pads_close(self.handle as *mut c_void);
        }
        let mut state = self.context.lock().unwrap_or_else(|e| e.into_inner());
        state.reset_all();
        if bmz_core::latency::diagnostics_enabled() {
            let summary = serde_json::json!({"schema":1,"kind":"gamecontroller",
                "os_to_receive_ns":state.age.summary(),"invalid_timestamps":state.invalid,
                "history_gaps":state.gaps,"ui_overflow":state.ui_overflow,"epoch":state.clock.epoch});
            tracing::info!("BMZ_LATENCY_JSON {summary}");
        }
        PAD_STATUS.store(0, Ordering::Release);
    }
}

struct Device {
    info: ConnectedGamepad,
    modern: bool,
}
struct PadState {
    generation: u64,
    route_started: u128,
    route: Option<InputRoute>,
    nonce: String,
    devices: HashMap<u32, Device>,
    buttons: HashMap<(u32, u32), bool>,
    axes: HashMap<(u32, u32), f32>,
    suppressed: HashSet<(u32, u32)>,
    suppressed_axes: HashSet<(u32, u32)>,
    analog: AnalogGamepadProcessor,
    delivery: ButtonDelivery,
    output: GamepadPollOutput,
    clock: MachClock,
    suspend: bmz_core::suspend::SuspendMonitor,
    age: bmz_core::latency::LatencyHistogram,
    invalid: u64,
    gaps: u64,
    ui_overflow: u64,
    failed: bool,
    foreground: bool,
    last_foreground_check: u128,
}
impl PadState {
    fn new(configs: [GamepadScratchConfig; 2], ticks: u64, numer: u32, denom: u32) -> Self {
        let now = monotonic_timestamp_ns();
        Self {
            generation: 0,
            route_started: now,
            route: None,
            nonce: format!("{}:{now}", std::process::id()),
            devices: HashMap::new(),
            buttons: HashMap::new(),
            axes: HashMap::new(),
            suppressed: HashSet::new(),
            suppressed_axes: HashSet::new(),
            analog: AnalogGamepadProcessor::new(configs, GamepadSlotMap::default()),
            delivery: ButtonDelivery::default(),
            output: GamepadPollOutput::default(),
            clock: MachClock::new(ticks, now, numer, denom),
            suspend: Default::default(),
            age: Default::default(),
            invalid: 0,
            gaps: 0,
            ui_overflow: 0,
            failed: false,
            foreground: false,
            last_foreground_check: 0,
        }
    }
    fn status(&self) -> u8 {
        if self.failed {
            4
        } else if self.devices.is_empty() {
            3
        } else if self.devices.values().all(|d| d.modern) {
            1
        } else {
            2
        }
    }
    fn reset_all(&mut self) {
        self.delivery.set_route(None);
        let ids: Vec<_> = self.devices.keys().copied().collect();
        for id in ids {
            self.reset_device(id);
        }
    }
    fn reset_device(&mut self, id: u32) {
        if let Some(device) = self.devices.get(&id) {
            let mut output = GamepadPollOutput::default();
            self.analog.release_device(
                device.info.device_id,
                current_device_timestamp(),
                &mut output.buttons,
            );
            for (&(pad, code), &pressed) in &self.buttons {
                if pad == id && pressed {
                    output.buttons.push(button_event(
                        device.info.device_id,
                        code,
                        false,
                        current_device_timestamp(),
                    ));
                    self.suppressed.insert((pad, code));
                }
            }
            self.publish(output);
            self.axes.retain(|(pad, _), _| *pad != id);
        }
    }
    fn pressed_buttons(&self) -> Vec<GamepadPressedButton> {
        let mut buttons = self.analog.pressed_buttons();
        buttons.extend(self.buttons.iter().filter_map(|(&(pad, code), &pressed)| {
            pressed
                .then(|| {
                    self.devices.get(&pad).map(|device| GamepadPressedButton {
                        name: button_name(code).to_owned(),
                        device_id: device.info.device_id,
                    })
                })
                .flatten()
        }));
        buttons
    }
    fn publish(&mut self, mut output: GamepadPollOutput) {
        if self.delivery.active() && !self.failed {
            for button in &output.buttons {
                let mut event = to_device_input_event(button);
                if button.synthesized_analog_axis
                    && self.route.as_ref().is_some_and(|route| {
                        route.binding.resolve_entry(event.device, &event.control).is_some_and(
                            |entry| {
                                matches!(
                                    entry.lane,
                                    bmz_core::lane::Lane::Scratch | bmz_core::lane::Lane::Scratch2
                                )
                            },
                        )
                    })
                {
                    event.bounce_policy = InputBouncePolicy::Bypass;
                }
                self.delivery.push(event);
            }
        }
        fn append<T>(dst: &mut Vec<T>, src: &mut Vec<T>) -> u64 {
            let accepted = src.len().min(4096usize.saturating_sub(dst.len()));
            let lost = src.len() - accepted;
            dst.extend(src.drain(..accepted));
            lost as u64
        }
        self.ui_overflow += append(&mut self.output.buttons, &mut output.buttons)
            + append(&mut self.output.axis_ticks, &mut output.axis_ticks)
            + append(&mut self.output.raw_events, &mut output.raw_events);
    }
    fn receive(
        &mut self,
        event: Event,
        received: u128,
        now: Instant,
        now_ticks: u64,
        foreground: bool,
    ) {
        // Topology belongs to this native owner, not to a gameplay route.
        // A hotplug queued between the Rust generation update and the native
        // fence must still be registered before the new baseline arrives.
        if self.failed || (event.generation != self.generation && !matches!(event.kind, 1 | 2)) {
            return;
        }
        if self.suspend.poll() {
            self.reset_all();
            self.clock.rebase(now_ticks, received);
            self.route_started = received;
            self.age = Default::default();
        }
        let active = self.route.as_ref().filter(|r| r.focused && foreground);
        self.delivery.set_route(active);
        match event.kind {
            1 => {
                self.devices.insert(
                    event.device,
                    Device {
                        modern: event.control == 1,
                        info: ConnectedGamepad {
                            stable_id: format!("gc-session:{}:{}", self.nonce, event.device),
                            backend_id: event.device,
                            device_id: DeviceId(0x4000_0000 + event.device),
                            name: event.name.unwrap_or_else(|| "GameController".into()),
                            is_connected: true,
                        },
                    },
                );
                return;
            }
            2 => {
                self.reset_device(event.device);
                self.devices.remove(&event.device);
                self.buttons.retain(|(pad, _), _| *pad != event.device);
                self.suppressed.retain(|(pad, _)| *pad != event.device);
                self.suppressed_axes.retain(|(pad, _)| *pad != event.device);
                return;
            }
            5 => {
                self.gaps += 1;
                self.reset_device(event.device);
                return;
            }
            6 => {
                let mut output = GamepadPollOutput::default();
                self.analog.check_timeouts(now, &mut output.buttons);
                self.publish(output);
                return;
            }
            _ => {}
        }
        let Some(device) = self.devices.get(&event.device) else {
            return;
        };
        let id = device.info.device_id;
        let modern = device.modern;
        let key = (event.device, event.control);
        if event.kind == 8 {
            self.buttons.insert(key, event.value != 0.0);
            if event.value != 0.0 {
                self.suppressed.insert(key);
            } else {
                self.suppressed.remove(&key);
            }
            return;
        }
        if event.kind == 9 {
            self.axes.insert(key, event.value);
            self.analog.seed_axis(id, event.control, axis_name(event.control), event.value);
            if event.value != 0.0 && !self.analog.analog_scratch_enabled(id) {
                self.suppressed_axes.insert(key);
            } else {
                self.suppressed_axes.remove(&key);
            }
            return;
        }
        if !event.value.is_finite() {
            return;
        }
        if event.kind == 3 {
            let pressed = event.value != 0.0;
            if self.buttons.get(&key) == Some(&pressed) {
                return;
            }
            if self.suppressed.contains(&key) {
                self.buttons.insert(key, pressed);
                if !pressed {
                    self.suppressed.remove(&key);
                }
                return;
            }
        } else if event.kind == 4 {
            if self.axes.get(&key).is_some_and(|value| value.to_bits() == event.value.to_bits()) {
                return;
            }
            if self.suppressed_axes.contains(&key) {
                self.axes.insert(key, event.value);
                self.analog.seed_axis(id, event.control, axis_name(event.control), event.value);
                if event.value == 0.0 {
                    self.suppressed_axes.remove(&key);
                }
                return;
            }
        } else {
            return;
        }
        let timestamp = if modern {
            let epoch = self.clock.epoch;
            let time = self.clock.convert_seconds(event.timestamp, now_ticks, received);
            if self.clock.epoch != epoch {
                self.reset_all();
                self.age = Default::default();
            }
            let Some(time) = time.filter(|time| *time >= self.route_started) else {
                self.invalid += 1;
                self.reset_device(event.device);
                return;
            };
            if bmz_core::latency::diagnostics_enabled() {
                self.age.record((received - time).min(u64::MAX as u128) as u64);
            }
            time
        } else {
            received
        };
        let stamp = DeviceTimestamp::MonotonicNs(timestamp);
        let mut output = GamepadPollOutput::default();
        if event.kind == 3 {
            let pressed = event.value != 0.0;
            // Keep the last valid physical state until timestamp validation.
            // Otherwise an invalid release could erase the held state before
            // reset_device has a chance to release it from the gameplay sink.
            self.buttons.insert(key, pressed);
            let name = button_name(event.control);
            output.buttons.push(button_event(id, event.control, pressed, stamp));
            output.raw_events.push(RawInputEvent {
                device_id: id,
                kind: RawInputEventKind::Button,
                logical: name.into(),
                raw_code: RawControlCode { value: event.control, label: name.into() },
                timestamp: stamp,
                mapped_control: Some(name.into()),
                pressed: Some(pressed),
                value: None,
                ticks: None,
            });
        } else {
            self.axes.insert(key, event.value);
            let name = axis_name(event.control);
            let at = now
                .checked_sub(Duration::from_nanos(
                    (received - timestamp).min(u64::MAX as u128) as u64
                ))
                .unwrap_or(now);
            self.analog.process_axis_at(
                id,
                event.control,
                name,
                name.into(),
                RawControlCode { value: event.control, label: name.into() },
                event.value,
                stamp,
                at,
                &mut output,
            );
        }
        self.publish(output);
    }
}
struct Event {
    generation: u64,
    device: u32,
    kind: i32,
    control: u32,
    value: f32,
    timestamp: f64,
    name: Option<String>,
}
fn button_name(code: u32) -> &'static str {
    const NAMES: [&str; 17] = [
        "GCButtonA",
        "GCButtonB",
        "GCButtonX",
        "GCButtonY",
        "GCLeftShoulder",
        "GCRightShoulder",
        "GCLeftTrigger",
        "GCRightTrigger",
        "GCLeftStickButton",
        "GCRightStickButton",
        "GCMenu",
        "GCOptions",
        "GCHome",
        "GCDpadUp",
        "GCDpadDown",
        "GCDpadLeft",
        "GCDpadRight",
    ];
    NAMES.get(code as usize).copied().unwrap_or("GCUnknownButton")
}
fn axis_name(code: u32) -> &'static str {
    const NAMES: [&str; 6] = [
        "GCAxisLeftX",
        "GCAxisLeftY",
        "GCAxisRightX",
        "GCAxisRightY",
        "GCAxisLeftTrigger",
        "GCAxisRightTrigger",
    ];
    NAMES.get(code as usize).copied().unwrap_or("GCUnknownAxis")
}
fn button_event(
    device: DeviceId,
    code: u32,
    pressed: bool,
    timestamp: DeviceTimestamp,
) -> GamepadButtonEvent {
    GamepadButtonEvent {
        name: button_name(code).into(),
        device_id: device,
        pressed,
        timestamp,
        synthesized_analog_axis: false,
    }
}
fn ticks() -> (u64, u32, u32) {
    let (mut numer, mut denom) = (0, 0);
    let ticks = unsafe { bmz_keyboard_ticks(&mut numer, &mut denom) };
    (ticks, numer, denom)
}

extern "C" fn pad_event(
    context: *mut c_void,
    generation: u64,
    device: u32,
    kind: i32,
    control: u32,
    value: f32,
    timestamp: f64,
    name: *const c_char,
) {
    let received = monotonic_timestamp_ns();
    let now = Instant::now();
    let (ticks, _, _) = ticks();
    let context = unsafe { &*context.cast::<Mutex<PadState>>() };
    let mut context = context.lock().unwrap_or_else(|e| e.into_inner());
    // Scratch deadlines run at 1ms, but idle deadlines need not query the
    // WindowServer at 1kHz. Actual input always performs the foreground check.
    if kind != 6 || received.saturating_sub(context.last_foreground_check) >= 10_000_000 {
        context.foreground = unsafe { super::bmz_keyboard_foreground() } != 0;
        context.last_foreground_check = received;
    }
    let foreground = context.foreground;
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let name = (!name.is_null())
            .then(|| unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned());
        context.receive(
            Event { generation, device, kind, control, value, timestamp, name },
            received,
            now,
            ticks,
            foreground,
        );
    }))
    .is_err()
    {
        context.failed = true;
        context.delivery.set_route(None);
    }
    PAD_STATUS.store(context.status(), Ordering::Release);
}

#[cfg(test)]
#[path = "pads_tests.rs"]
mod tests;
