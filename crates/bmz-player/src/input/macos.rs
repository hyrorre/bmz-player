//! The C manager and its callback context are created/destroyed on one thread.
//! The manager is unscheduled and closed before the boxed Rust context is freed.
use super::{
    capture::InputRoute,
    macos_clock::MachClock,
    macos_keys::{self, Holds},
};
use bmz_gameplay::input::backend::{DeviceInputEvent, DeviceTimestamp, monotonic_timestamp_ns};
use std::{
    ffi::c_void,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread::JoinHandle,
};

unsafe extern "C" {
    fn bmz_keyboard_access() -> i32;
    fn bmz_keyboard_request_access();
    fn bmz_keyboard_foreground() -> i32;
    fn bmz_keyboard_ticks(numer: *mut u32, denom: *mut u32) -> u64;
    fn bmz_keyboard_open(
        context: *mut c_void,
        callback: extern "C" fn(*mut c_void, usize, u32, u64, i32),
    ) -> *mut c_void;
    fn bmz_keyboard_wait();
    fn bmz_keyboard_close(manager: *mut c_void);
}

// Status is process-wide because BMZ owns one keyboard capture instance.
// 0=off, 1=running (not proof of device delivery), 2=permission, 3=initialization/error.
static STATUS: AtomicU8 = AtomicU8::new(0);
pub fn status() -> u8 {
    STATUS.load(Ordering::Acquire)
}
pub fn request_permission() {
    // Called only by an explicit settings button, never during startup.
    unsafe {
        bmz_keyboard_request_access();
    }
}

pub struct MacKeyboard {
    route: Arc<Mutex<RouteState>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MacKeyboard {
    pub fn start() -> Option<Self> {
        if unsafe { bmz_keyboard_access() } != 0 {
            STATUS.store(2, Ordering::Release);
            tracing::warn!("IOHID permission unavailable; using winit");
            return None;
        }
        let route = Arc::new(Mutex::new(RouteState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (worker_route, worker_stop) = (route.clone(), stop.clone());
        let worker = std::thread::Builder::new().name("bmz-iohid-keyboard".into()).spawn(move || {
            let (ticks, numer, denom) = ticks();
            let mut context = Box::new(Context {
                route: worker_route, generation: 0, active: None, holds: Holds::default(),
                clock: MachClock::new(ticks, monotonic_timestamp_ns(), numer, denom),
                failed: false, diagnostics: bmz_core::latency::diagnostics_enabled(),
                age: Default::default(), invalid: 0, unsupported: 0,
            });
            // Box address remains stable until close completes; callbacks only
            // borrow it synchronously on this owning run-loop thread.
            let manager = unsafe { bmz_keyboard_open((&mut *context as *mut Context).cast(), event) };
            if !manager.is_null() { STATUS.store(1, Ordering::Release); }
            let _ = tx.send(!manager.is_null());
            if manager.is_null() { return; }
            while !worker_stop.load(Ordering::Acquire) && !context.failed {
                context.sync_route();
                unsafe { bmz_keyboard_wait(); }
                if unsafe { bmz_keyboard_access() } != 0 {
                    context.failed = true;
                }
            }
            context.release();
            unsafe { bmz_keyboard_close(manager); }
            if context.diagnostics {
                let summary = serde_json::json!({"schema":1,"kind":"iohid", "os_to_receive_ns":context.age.summary(),
                    "invalid_timestamps":context.invalid,"unsupported_usages":context.unsupported});
                tracing::info!("BMZ_LATENCY_JSON {summary}");
            }
            if context.failed && status() == 1 { STATUS.store(3, Ordering::Release); }
        }).ok()?;
        if !rx.recv().unwrap_or(false) {
            let _ = worker.join();
            STATUS.store(3, Ordering::Release);
            tracing::warn!("IOHID initialization failed; using winit");
            return None;
        }
        tracing::info!("IOHID keyboard manager opened on dedicated run loop");
        Some(Self { route, stop, worker: Some(worker) })
    }
    pub fn set_route(&self, route: Option<InputRoute>) {
        let mut state = self.route.lock().unwrap_or_else(|e| e.into_inner());
        state.set(route);
    }
    pub fn active(&self) -> bool {
        status() == 1
    }
}
impl Drop for MacKeyboard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        STATUS.store(0, Ordering::Release);
    }
}

fn ticks() -> (u64, u32, u32) {
    let (mut numer, mut denom) = (0, 0);
    let now = unsafe { bmz_keyboard_ticks(&mut numer, &mut denom) };
    (now, numer, denom)
}
#[derive(Default)]
struct RouteState {
    route: Option<InputRoute>,
    generation: u64,
}
impl RouteState {
    fn set(&mut self, next: Option<InputRoute>) {
        let same = match (&self.route, &next) {
            (Some(a), Some(b)) => {
                a.input.same_source(&b.input)
                    && a.focused == b.focused
                    && a.keyboard_enabled == b.keyboard_enabled
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.generation = self.generation.wrapping_add(1);
        }
        self.route = next;
    }
}
struct Context {
    route: Arc<Mutex<RouteState>>,
    generation: u64,
    active: Option<InputRoute>,
    holds: Holds,
    clock: MachClock,
    failed: bool,
    diagnostics: bool,
    age: bmz_core::latency::LatencyHistogram,
    invalid: u64,
    unsupported: u64,
}
impl Context {
    fn release(&mut self) {
        for usage in self.holds.clear() {
            self.deliver(usage, false, monotonic_timestamp_ns());
        }
        self.active = None;
    }
    fn sync_route(&mut self) {
        let (next, generation) = {
            let state = self.route.lock().unwrap_or_else(|e| e.into_inner());
            (state.route.clone(), state.generation)
        };
        let next = next.filter(|r| {
            r.focused && r.keyboard_enabled && unsafe { bmz_keyboard_foreground() } != 0
        });
        let same = match (&self.active, &next) {
            (Some(a), Some(b)) => a.input.same_source(&b.input),
            (None, None) => true,
            _ => false,
        };
        if !same || self.generation != generation {
            self.release();
            self.active = next;
            self.generation = generation;
        }
    }
    fn deliver(&self, usage: u32, down: bool, ns: u128) {
        if let (Some(route), Some(key)) = (&self.active, macos_keys::physical_key(usage)) {
            let control = super::winit::physical_key_to_control(key).unwrap();
            route.input.push_shared_event(DeviceInputEvent {
                device: super::winit::W_KEYBOARD_DEVICE_ID,
                control,
                kind: if down {
                    bmz_core::input::InputKind::Press
                } else {
                    bmz_core::input::InputKind::Release
                },
                timestamp: DeviceTimestamp::MonotonicNs(ns),
                bounce_policy: Default::default(),
            });
        }
    }
    fn receive(&mut self, device: usize, usage: u32, timestamp: u64, state: i32) {
        self.sync_route();
        // Serialize the final route check and enqueue with route replacement.
        // A stale callback cannot enqueue into a new play after set_route returns.
        let route = self.route.clone();
        let route_guard = route.lock().unwrap_or_else(|e| e.into_inner());
        if route_guard.generation != self.generation {
            return;
        }
        if state == -2 {
            self.failed = true;
            return;
        }
        if state == -1 {
            for usage in self.holds.remove(device) {
                self.deliver(usage, false, monotonic_timestamp_ns());
            }
            return;
        }
        if self.active.is_none() {
            return;
        }
        if macos_keys::key(usage).is_none() {
            // A keyboard outside the supported physical mapping fails the route
            // closed, rather than silently losing a bound key or mixing sources.
            if usage > 3 {
                self.unsupported += 1;
                self.failed = true;
            }
            return;
        }
        let (now_ticks, _, _) = ticks();
        let received = monotonic_timestamp_ns();
        let timestamp = match self.clock.convert(timestamp, now_ticks, received) {
            Some(timestamp) => {
                if self.diagnostics {
                    self.age.record((received - timestamp).min(u64::MAX as u128) as u64);
                }
                timestamp
            }
            None => {
                self.invalid += 1;
                received
            }
        };
        if self.holds.update(device, usage, state != 0) {
            self.deliver(usage, state != 0, timestamp);
        }
    }
}
extern "C" fn event(context: *mut c_void, device: usize, usage: u32, timestamp: u64, state: i32) {
    // Never unwind over the C ABI. The callback context is alive on this thread
    // until IOHID callbacks are unregistered and the manager is closed.
    let context = unsafe { &mut *context.cast::<Context>() };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        context.receive(device, usage, timestamp, state)
    }))
    .is_err()
    {
        context.failed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_generation_remembers_suppression_even_between_callbacks() {
        let input = super::super::shared::SharedInputBackend::default();
        let route = InputRoute {
            input,
            focused: true,
            keyboard_enabled: true,
            binding: bmz_gameplay::input::binding::LaneBinding { entries: vec![] },
        };
        let mut state = RouteState::default();
        state.set(Some(route.clone()));
        let first = state.generation;
        state.set(Some(route.clone()));
        assert_eq!(state.generation, first);
        state.set(None);
        state.set(Some(route.clone()));
        assert_ne!(state.generation, first);
        let previous = state.generation;
        state.set(Some(InputRoute { focused: false, ..route }));
        assert_ne!(state.generation, previous);
    }
}
