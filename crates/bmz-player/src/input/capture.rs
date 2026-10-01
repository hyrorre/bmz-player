//! OS input is acquired here, before any window/render work. The UI consumes a
//! bounded copy for menus; gameplay receives timestamped events directly.
use super::{gamepad::*, rawinput::RawInputBridge, shared::SharedInputBackend};
use crate::config::app_config::GamepadBackendKind;
use bmz_gameplay::input::{backend::InputBouncePolicy, binding::LaneBinding};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct InputRoute {
    pub input: SharedInputBackend,
    pub binding: LaneBinding,
    pub focused: bool,
    pub keyboard_enabled: bool,
}

struct State {
    #[cfg(all(windows, feature = "experimental-gameinput"))]
    gameinput_diagnostics: Option<super::gameinput::GameInputPollDiagnostics>,
    configs: [GamepadScratchConfig; 2],
    slots: GamepadSlotMap,
    route: Option<Arc<InputRoute>>,
    owner_window: usize,
    output: GamepadPollOutput,
    connected: Vec<ConnectedGamepad>,
}

pub struct InputCapture {
    #[cfg(target_os = "macos")]
    gc_pads: Option<super::gamecontroller::Pads>,
    #[cfg(target_os = "macos")]
    mac_keyboard: Mutex<(Option<crate::config::app_config::InputBackendKind>, Option<MacKeyboard>)>,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    name: &'static str,
    #[cfg(not(target_os = "macos"))]
    native_keyboard: Arc<AtomicBool>,
}

impl InputCapture {
    pub fn new(
        kind: Option<GamepadBackendKind>,
        configs: [GamepadScratchConfig; 2],
        bridge: Option<RawInputBridge>,
    ) -> anyhow::Result<Self> {
        #[cfg(target_os = "macos")]
        let gc_pads = (kind == Some(GamepadBackendKind::GameController))
            .then(|| super::gamecontroller::Pads::start(configs))
            .flatten();
        #[cfg(target_os = "macos")]
        let kind = if gc_pads.is_some() { None } else { kind };
        let state = Arc::new(Mutex::new(State {
            #[cfg(all(windows, feature = "experimental-gameinput"))]
            gameinput_diagnostics: None,
            configs,
            slots: GamepadSlotMap::default(),
            route: None,
            owner_window: 0,
            output: GamepadPollOutput::default(),
            connected: Vec::new(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let native_keyboard = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (worker_state, worker_stop, native_status) =
            (state.clone(), stop.clone(), native_keyboard.clone());
        let thread = thread::Builder::new().name("bmz-input-capture".into()).spawn(move || {
            // Construct and destroy GameInput on its owner thread, including its COM pointers.
            let mut backend = create_backend(kind, configs, bridge.clone());
            let name = backend.as_ref().map_or("keyboard only", GamepadBackend::name);
            let _ = ready_tx.send(name);
            #[cfg(windows)]
            let mut native = super::native_capture::NativeCapture::new(
                bridge,
                matches!(kind, Some(GamepadBackendKind::RawInput)),
            )
            .map_err(|error| tracing::error!(%error, "independent keyboard capture unavailable"))
            .ok();
            #[cfg(not(windows))]
            let _ = (bridge, native_status);
            let mut last_devices = Instant::now() - Duration::from_secs(1);
            let mut delivery = ButtonDelivery::default();
            while !worker_stop.load(Ordering::Acquire) {
                let (configs, slots, route, owner) = {
                    let state = worker_state.lock().unwrap_or_else(|e| e.into_inner());
                    (state.configs, state.slots, state.route.clone(), state.owner_window)
                };
                #[cfg(windows)]
                if let Some(native) = &mut native {
                    native_status.store(native.attach(owner).is_ok(), Ordering::Release);
                    native.poll(route.as_deref());
                }
                #[cfg(not(windows))]
                let _ = owner;
                let mut output = if let Some(backend) = &mut backend {
                    backend.set_analog_config(configs, slots);
                    backend.poll()
                } else {
                    GamepadPollOutput::default()
                };
                let active_route =
                    route.as_deref().filter(|route| route.focused && foreground_matches(owner));
                delivery.set_route(active_route);
                if let Some(route) = active_route {
                    for button in &output.buttons {
                        let mut event = to_device_input_event(button);
                        if button.synthesized_analog_axis
                            && route
                                .binding
                                .resolve_entry(event.device, &event.control)
                                .is_some_and(|entry| {
                                    matches!(
                                        entry.lane,
                                        bmz_core::lane::Lane::Scratch
                                            | bmz_core::lane::Lane::Scratch2
                                    )
                                })
                        {
                            event.bounce_policy = InputBouncePolicy::Bypass;
                        }
                        delivery.push(event);
                    }
                }
                let connected = if last_devices.elapsed() >= Duration::from_millis(250) {
                    last_devices = Instant::now();
                    Some(backend.as_ref().map_or_else(Vec::new, GamepadBackend::connected_gamepads))
                } else {
                    None
                };
                {
                    let mut state = worker_state.lock().unwrap_or_else(|e| e.into_inner());
                    #[cfg(all(windows, feature = "experimental-gameinput"))]
                    {
                        state.gameinput_diagnostics =
                            backend.as_ref().and_then(GamepadBackend::gameinput_diagnostics);
                    }
                    // This queue is UI-only. Gameplay has already consumed its
                    // independent copy, so UI stalls cannot lose judged inputs.
                    const LIMIT: usize = 4096;
                    append_bounded(&mut state.output.buttons, &mut output.buttons, LIMIT);
                    append_bounded(&mut state.output.axis_ticks, &mut output.axis_ticks, LIMIT);
                    append_bounded(&mut state.output.raw_events, &mut output.raw_events, LIMIT);
                    if output.pressed_buttons.is_some() {
                        state.output.pressed_buttons = output.pressed_buttons;
                    }
                    if let Some(connected) = connected {
                        state.connected = connected;
                    }
                }
                #[cfg(windows)]
                super::native_capture::wait_for_input();
                #[cfg(target_os = "macos")]
                thread::park_timeout(if backend.is_some() {
                    Duration::from_millis(1)
                } else {
                    Duration::from_millis(250)
                });
                #[cfg(not(any(windows, target_os = "macos")))]
                thread::park_timeout(Duration::from_millis(1));
            }
        })?;
        let name = ready_rx.recv_timeout(Duration::from_secs(5))?;
        #[cfg(target_os = "macos")]
        let name = if gc_pads.is_some() { "GameController" } else { name };
        tracing::info!(backend = name, "input backend: dedicated capture thread");
        Ok(Self {
            #[cfg(target_os = "macos")]
            gc_pads,
            #[cfg(target_os = "macos")]
            mac_keyboard: Mutex::new((None, None)),
            state,
            stop,
            thread: Some(thread),
            name,
            #[cfg(not(target_os = "macos"))]
            native_keyboard,
        })
    }

    pub fn set_route(&self, route: Option<InputRoute>) {
        #[cfg(target_os = "macos")]
        if let Some(pads) = &self.gc_pads {
            pads.set_route(route.clone());
        }
        #[cfg(target_os = "macos")]
        if let Some(keyboard) = &self.mac_keyboard.lock().unwrap_or_else(|e| e.into_inner()).1 {
            keyboard.set_route(route.clone());
        }
        let route = route.map(Arc::new);
        let old = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::replace(&mut state.route, route)
        };
        drop(old);
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }
    pub fn native_keyboard_enabled(&self) -> bool {
        #[cfg(target_os = "macos")]
        return self
            .mac_keyboard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .1
            .as_ref()
            .is_some_and(MacKeyboard::active);
        #[cfg(not(target_os = "macos"))]
        self.native_keyboard.load(Ordering::Acquire)
    }
    #[cfg(target_os = "macos")]
    pub fn configure_mac_keyboard(
        &self,
        requested: Option<crate::config::app_config::InputBackendKind>,
    ) {
        let mut keyboard = self.mac_keyboard.lock().unwrap_or_else(|e| e.into_inner());
        if keyboard.0 == requested {
            return;
        }
        keyboard.0 = requested.clone();
        keyboard.1 = None;
        keyboard.1 = match requested {
            #[cfg(feature = "macos-iohid")]
            Some(crate::config::app_config::InputBackendKind::MacOsHid) => {
                super::macos::MacKeyboard::start().map(MacKeyboard::Hid)
            }
            Some(crate::config::app_config::InputBackendKind::MacOsGameController) => {
                super::gamecontroller::Keyboard::start().map(MacKeyboard::GameController)
            }
            _ => None,
        };
    }
    pub fn set_analog_config(&mut self, configs: [GamepadScratchConfig; 2], slots: GamepadSlotMap) {
        #[cfg(target_os = "macos")]
        if let Some(pads) = &self.gc_pads {
            pads.set_analog_config(configs, slots);
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.configs = configs;
        state.slots = slots;
    }
    pub fn poll(&mut self) -> GamepadPollOutput {
        #[cfg(target_os = "macos")]
        if let Some(pads) = &self.gc_pads {
            return pads.poll();
        }
        self.state.try_lock().map(|mut state| std::mem::take(&mut state.output)).unwrap_or_default()
    }
    pub fn connected_gamepads(&self) -> Vec<ConnectedGamepad> {
        #[cfg(target_os = "macos")]
        if let Some(pads) = &self.gc_pads {
            return pads.connected_gamepads();
        }
        self.state.lock().unwrap_or_else(|e| e.into_inner()).connected.clone()
    }
    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn is_gilrs(&self) -> bool {
        self.name == "gilrs"
    }
    pub fn attach_window(&mut self, window: &winit::window::Window) -> anyhow::Result<()> {
        #[cfg(windows)]
        {
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let RawWindowHandle::Win32(handle) = window.window_handle()?.as_raw() {
                self.state.lock().unwrap_or_else(|e| e.into_inner()).owner_window =
                    handle.hwnd.get() as usize;
            }
        }
        #[cfg(not(windows))]
        let _ = window;
        Ok(())
    }
    #[cfg(all(windows, feature = "experimental-gameinput"))]
    pub fn gameinput_diagnostics(&self) -> Option<super::gameinput::GameInputPollDiagnostics> {
        self.state.try_lock().ok().and_then(|state| state.gameinput_diagnostics)
    }
}

#[cfg(target_os = "macos")]
enum MacKeyboard {
    #[cfg(feature = "macos-iohid")]
    Hid(super::macos::MacKeyboard),
    GameController(super::gamecontroller::Keyboard),
}

#[cfg(target_os = "macos")]
impl MacKeyboard {
    fn active(&self) -> bool {
        match self {
            #[cfg(feature = "macos-iohid")]
            Self::Hid(keyboard) => keyboard.active(),
            Self::GameController(keyboard) => keyboard.active(),
        }
    }
    fn set_route(&self, route: Option<InputRoute>) {
        match self {
            #[cfg(feature = "macos-iohid")]
            Self::Hid(keyboard) => keyboard.set_route(route),
            Self::GameController(keyboard) => keyboard.set_route(route),
        }
    }
}

#[derive(Default)]
pub(super) struct ButtonDelivery {
    input: Option<SharedInputBackend>,
    pressed: std::collections::HashMap<
        (bmz_gameplay::input::backend::DeviceId, bmz_gameplay::input::backend::PhysicalControl),
        bmz_gameplay::input::backend::DeviceInputEvent,
    >,
}

impl ButtonDelivery {
    #[cfg(target_os = "macos")]
    pub(super) fn active(&self) -> bool {
        self.input.is_some()
    }
    pub(super) fn set_route(&mut self, route: Option<&InputRoute>) {
        let same = match (&self.input, route) {
            (Some(input), Some(route)) => input.same_source(&route.input),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some(input) = &self.input {
            for (_, mut event) in self.pressed.drain() {
                event.kind = bmz_core::input::InputKind::Release;
                event.timestamp = bmz_gameplay::input::backend::DeviceTimestamp::MonotonicNs(
                    bmz_gameplay::input::backend::monotonic_timestamp_ns(),
                );
                input.push_shared_event(event);
            }
        }
        self.input = route.map(|route| route.input.clone());
    }
    pub(super) fn push(&mut self, event: bmz_gameplay::input::backend::DeviceInputEvent) {
        let key = (event.device, event.control.clone());
        match event.kind {
            bmz_core::input::InputKind::Press => {
                self.pressed.insert(key, event.clone());
            }
            bmz_core::input::InputKind::Release => {
                self.pressed.remove(&key);
            }
        }
        if let Some(input) = &self.input {
            input.push_shared_event(event);
        }
    }
}

impl Drop for InputCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            // Input backend replacement happens outside active play. Complete
            // unregister on its owner thread before registering a replacement.
            let _ = thread.join();
        }
    }
}

fn append_bounded<T>(destination: &mut Vec<T>, source: &mut Vec<T>, capacity: usize) {
    let accepted = source.len().min(capacity.saturating_sub(destination.len()));
    destination.extend(source.drain(..accepted));
}

pub(super) fn foreground_matches(owner: usize) -> bool {
    #[cfg(windows)]
    {
        owner != 0
            && unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() as usize == owner
            }
    }
    #[cfg(not(windows))]
    {
        let _ = owner;
        true
    }
}

fn create_backend(
    kind: Option<GamepadBackendKind>,
    configs: [GamepadScratchConfig; 2],
    bridge: Option<RawInputBridge>,
) -> Option<GamepadBackend> {
    let kind = kind?;
    #[cfg(windows)]
    if kind == GamepadBackendKind::RawInput
        && let Some(bridge) = bridge
    {
        return Some(GamepadBackend::RawInput(Box::new(super::rawinput::RawInputBackend::new(
            bridge, configs,
        ))));
    }
    #[cfg(not(windows))]
    let _ = (kind, bridge);
    #[cfg(all(windows, feature = "experimental-gameinput"))]
    if kind == GamepadBackendKind::GameInput
        && let Ok(backend) = super::gameinput::GameInputBackend::new(configs)
    {
        return Some(GamepadBackend::GameInput(Box::new(backend)));
    }
    match super::gilrs::GilrsBackend::new(configs) {
        Ok(backend) => Some(GamepadBackend::Gilrs(Box::new(backend))),
        Err(error) => {
            tracing::warn!(%error, "gamepad capture initialization failed");
            None
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use bmz_gameplay::input::backend::*;
    #[test]
    fn changing_capture_route_releases_old_sink_without_leaking_into_retry() {
        let mut delivery = ButtonDelivery::default();
        let mut old = SharedInputBackend::default();
        let mut new = SharedInputBackend::default();
        let route = |input| InputRoute {
            input,
            binding: LaneBinding { entries: Vec::new() },
            focused: true,
            keyboard_enabled: true,
        };
        delivery.set_route(Some(&route(old.clone())));
        delivery.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::HidButton(1),
            kind: bmz_core::input::InputKind::Press,
            timestamp: DeviceTimestamp::MonotonicNs(123),
            bounce_policy: Default::default(),
        });
        delivery.set_route(Some(&route(new.clone())));
        let events = old.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].timestamp, DeviceTimestamp::MonotonicNs(123));
        assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
        assert!(new.drain_events().is_empty());
    }
}
