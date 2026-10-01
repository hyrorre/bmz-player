//! Permission-free, foreground GameController input. The native serial queue
//! never owns gameplay or UI state; it only delivers into the shared input sink.
mod pads;
use super::{capture::InputRoute, macos_keys, shared::SharedInputBackend};
use bmz_gameplay::input::backend::{
    DeviceInputEvent, DeviceTimestamp, PhysicalControl, monotonic_timestamp_ns,
};
pub use pads::{Pads, pad_status};
use std::{
    collections::HashSet,
    ffi::c_void,
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
};

unsafe extern "C" {
    fn bmz_gc_available() -> i32;
    fn bmz_gc_keyboard_open(
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void, u64, u32, i32),
        codes: *const u32,
        count: usize,
    ) -> *mut c_void;
    fn bmz_gc_keyboard_generation(handle: *mut c_void, generation: u64);
    fn bmz_gc_keyboard_close(handle: *mut c_void);
    fn bmz_keyboard_foreground() -> i32;
}

pub(crate) fn is_available() -> bool {
    unsafe { bmz_gc_available() != 0 }
}

// 0=off/unavailable, 1=connected and bound keys supported, 2=waiting,
// 3=callback failure, 4=bound key unavailable. None proves real-key latency.
static KEYBOARD_STATUS: AtomicU8 = AtomicU8::new(0);
pub fn keyboard_status() -> u8 {
    KEYBOARD_STATUS.load(Ordering::Acquire)
}

pub struct Keyboard {
    handle: usize,
    context: Box<Mutex<KeyboardState>>,
}
impl Keyboard {
    pub fn start() -> Option<Self> {
        KEYBOARD_STATUS.store(2, Ordering::Release);
        let mut context = Box::new(Mutex::new(KeyboardState::default()));
        let codes: Vec<u32> = (4..=231).filter(|code| macos_keys::key(*code).is_some()).collect();
        let handle = unsafe {
            bmz_gc_keyboard_open(
                (&mut *context as *mut Mutex<KeyboardState>).cast(),
                keyboard_event,
                codes.as_ptr(),
                codes.len(),
            )
        };
        if handle.is_null() {
            KEYBOARD_STATUS.store(0, Ordering::Release);
            tracing::warn!("GCKeyboard unavailable (requires macOS 11+); using winit");
            return None;
        }
        tracing::info!(
            "GCKeyboard opened on dedicated serial queue; timestamps are callback receipt times"
        );
        Some(Self { handle: handle as usize, context })
    }
    pub fn active(&self) -> bool {
        keyboard_status() == 1
    }
    pub fn window_event(&self, event: DeviceInputEvent, input: &SharedInputBackend) -> bool {
        self.context.lock().unwrap_or_else(|e| e.into_inner()).window_event(event, input)
    }
    pub fn set_route(&self, route: Option<InputRoute>) {
        let generation = {
            let mut state = self.context.lock().unwrap_or_else(|e| e.into_inner());
            if same_route(state.route.as_ref(), route.as_ref()) {
                return;
            }
            state.replace_route(route);
            state.generation
        };
        // Publish the generation before fencing the queue. Already scheduled
        // callbacks carry the old generation and cannot enter the new play.
        unsafe {
            bmz_gc_keyboard_generation(self.handle as *mut c_void, generation);
        }
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        unsafe {
            bmz_gc_keyboard_close(self.handle as *mut c_void);
        }
        self.context.lock().unwrap_or_else(|e| e.into_inner()).release();
        KEYBOARD_STATUS.store(0, Ordering::Release);
    }
}

pub(super) fn same_route(a: Option<&InputRoute>, b: Option<&InputRoute>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            a.input.same_source(&b.input)
                && a.focused == b.focused
                && a.keyboard_enabled == b.keyboard_enabled
                && a.binding.entries.len() == b.binding.entries.len()
                && a.binding.entries.iter().zip(&b.binding.entries).all(|(a, b)| {
                    a.device == b.device
                        && a.control == b.control
                        && a.lane == b.lane
                        && a.scratch_direction == b.scratch_direction
                })
        }
        (None, None) => true,
        _ => false,
    }
}

#[derive(Default)]
struct KeyboardState {
    generation: u64,
    route: Option<InputRoute>,
    sink: Option<SharedInputBackend>,
    supported: HashSet<PhysicalControl>,
    held: HashSet<u32>,
    connected: bool,
    failed: bool,
    events: u64,
    fallback: super::capture::ButtonDelivery,
}
impl KeyboardState {
    fn status(&self) -> u8 {
        if self.failed {
            return 3;
        }
        if !self.connected {
            return 2;
        }
        if self.route.as_ref().is_some_and(|route| {
            route.binding.entries.iter().any(|entry| {
                matches!(entry.control, PhysicalControl::KeyboardKey(_))
                    && !self.supported.contains(&entry.control)
            })
        }) {
            return 4;
        }
        1
    }
    fn replace_route(&mut self, route: Option<InputRoute>) {
        self.release();
        self.generation = self.generation.wrapping_add(1);
        self.route = route;
    }
    fn release(&mut self) {
        self.fallback.set_route(None);
        let held = std::mem::take(&mut self.held);
        for usage in held {
            self.deliver(usage, false, monotonic_timestamp_ns());
        }
        self.sink = None;
    }
    fn window_event(&mut self, event: DeviceInputEvent, input: &SharedInputBackend) -> bool {
        // Window admission and native admission share this state's mutex. Never
        // decide on an atomic status and enqueue later across a backend change.
        if self.status() == 1 {
            self.fallback.set_route(None);
            return false;
        }
        let route = self
            .route
            .as_ref()
            .filter(|r| r.focused && r.keyboard_enabled && r.input.same_source(input));
        self.fallback.set_route(route);
        if !self.fallback.active() {
            return false;
        }
        self.fallback.push(event);
        true
    }
    fn deliver(&self, usage: u32, pressed: bool, timestamp: u128) {
        if let (Some(sink), Some(key)) = (&self.sink, macos_keys::physical_key(usage))
            && let Some(control) = super::winit::physical_key_to_control(key)
        {
            sink.push_shared_event(DeviceInputEvent {
                device: super::winit::W_KEYBOARD_DEVICE_ID,
                control,
                kind: if pressed {
                    bmz_core::input::InputKind::Press
                } else {
                    bmz_core::input::InputKind::Release
                },
                timestamp: DeviceTimestamp::MonotonicNs(timestamp),
                bounce_policy: Default::default(),
            });
        }
    }
    fn receive(
        &mut self,
        generation: u64,
        usage: u32,
        state: i32,
        received: u128,
        foreground: bool,
    ) {
        if generation != self.generation {
            return;
        }
        match state {
            -1 | -10 => {
                self.release();
                self.connected = false;
                self.supported.clear();
            }
            -11 => {
                if let Some(key) = macos_keys::physical_key(usage)
                    && let Some(control) = super::winit::physical_key_to_control(key)
                {
                    self.supported.insert(control);
                }
            }
            -12 => self.connected = true,
            _ => {}
        }
        let eligible = self.status() == 1
            && foreground
            && self.route.as_ref().is_some_and(|r| r.focused && r.keyboard_enabled);
        if !eligible {
            // An unavailable native device must not clear a still-active winit
            // fallback on every heartbeat. Focus/route loss releases both.
            if !foreground || !self.route.as_ref().is_some_and(|r| r.focused && r.keyboard_enabled)
            {
                self.release();
            }
            return;
        }
        self.fallback.set_route(None);
        if self.sink.is_none() {
            self.sink = self.route.as_ref().map(|r| r.input.clone());
        }
        if state == 0 || state == 1 {
            let changed =
                if state == 1 { self.held.insert(usage) } else { self.held.remove(&usage) };
            if changed {
                self.events += 1;
                self.deliver(usage, state == 1, received);
            }
        }
    }
}

extern "C" fn keyboard_event(context: *mut c_void, generation: u64, usage: u32, state: i32) {
    let received = monotonic_timestamp_ns();
    let foreground = !matches!(state, 0 | 1 | -3) || unsafe { bmz_keyboard_foreground() } != 0;
    // Storage is allocated before native open, and freed only after close fence.
    let context = unsafe { &*context.cast::<Mutex<KeyboardState>>() };
    let mut context = context.lock().unwrap_or_else(|e| e.into_inner());
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        context.receive(generation, usage, state, received, foreground)
    }))
    .is_err()
    {
        context.failed = true;
        context.release();
    }
    KEYBOARD_STATUS.store(context.status(), Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_gameplay::input::{backend::InputBackend, binding::LaneBinding};
    fn route(input: SharedInputBackend) -> InputRoute {
        InputRoute {
            input,
            binding: LaneBinding { entries: vec![] },
            focused: true,
            keyboard_enabled: true,
        }
    }
    fn window_key(pressed: bool) -> DeviceInputEvent {
        super::super::winit::physical_key_to_device_input(
            macos_keys::physical_key(4).unwrap(),
            if pressed {
                winit::event::ElementState::Pressed
            } else {
                winit::event::ElementState::Released
            },
            false,
        )
        .unwrap()
    }
    #[test]
    fn fallback_hold_is_released_before_native_reconnect_and_never_duplicated() {
        let mut input = SharedInputBackend::default();
        let mut state = KeyboardState { route: Some(route(input.clone())), ..Default::default() };
        assert!(state.window_event(window_key(true), &input));
        state.receive(0, 0, -3, 101, true);
        assert_eq!(input.drain_events().len(), 1); // Waiting heartbeat preserves fallback hold.
        state.receive(0, 0, -12, 102, true);
        let releases = input.drain_events();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].kind, bmz_core::input::InputKind::Release);
        assert!(!state.window_event(window_key(false), &input));
        state.receive(0, 4, 0, 103, true); // Held-at-install key was not a native press.
        assert!(input.drain_events().is_empty());
        state.receive(0, 4, 1, 104, true);
        assert!(!state.window_event(window_key(true), &input));
        state.receive(0, 4, 0, 105, true);
        assert!(!state.window_event(window_key(false), &input));
        assert_eq!(input.drain_events().len(), 2);
        state.receive(0, 0, -1, 106, true);
        assert!(state.window_event(window_key(true), &input));
        state.replace_route(None);
        let events = input.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    }
    #[test]
    fn short_press_release_preserves_edges_and_receipt_timestamps() {
        let mut input = SharedInputBackend::default();
        let mut state = KeyboardState {
            route: Some(route(input.clone())),
            connected: true,
            ..Default::default()
        };
        state.receive(0, 4, 1, 100, true);
        state.receive(0, 4, 0, 101, true);
        let events = input.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].timestamp, DeviceTimestamp::MonotonicNs(100));
        assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    }
    #[test]
    fn route_generation_rejects_queued_events_and_releases_old_sink() {
        let mut old = SharedInputBackend::default();
        let mut new = SharedInputBackend::default();
        let mut state = KeyboardState {
            route: Some(route(old.clone())),
            connected: true,
            ..Default::default()
        };
        state.receive(0, 4, 1, 100, true);
        state.replace_route(Some(route(new.clone())));
        state.receive(0, 4, 1, 101, true);
        assert!(new.drain_events().is_empty());
        assert_eq!(old.drain_events().len(), 2);
        state.receive(1, 5, 1, 102, true);
        state.receive(1, 0, -3, 103, false);
        let events = new.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    }
    #[test]
    fn unsupported_bound_key_falls_back_and_disconnect_releases() {
        let mut input = SharedInputBackend::default();
        let mut binding = route(input.clone());
        binding.binding.entries.push(bmz_gameplay::input::binding::BindingEntry {
            device: None,
            control: PhysicalControl::KeyboardKey("A".into()),
            lane: bmz_core::lane::Lane::Key1,
            scratch_direction: None,
        });
        let mut state =
            KeyboardState { route: Some(binding), connected: true, ..Default::default() };
        assert_eq!(state.status(), 4);
        state.receive(0, 4, 1, 100, true);
        assert!(input.drain_events().is_empty());
        state.receive(0, 4, -11, 101, true);
        assert_eq!(state.status(), 1);
        state.receive(0, 4, 1, 102, true);
        state.receive(0, 0, -1, 103, true);
        assert_eq!(state.status(), 2);
        let events = input.drain_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    }
    #[test]
    fn native_keyboard_open_close_fences_callback_storage() {
        if let Some(keyboard) = Keyboard::start() {
            drop(keyboard);
        }
        assert_eq!(keyboard_status(), 0);
    }
}
