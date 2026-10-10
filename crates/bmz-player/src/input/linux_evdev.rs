//! Opt-in, read-only evdev keyboard capture. Native X11 + logind only.
//! Never grabs devices or requests ownership of the desktop seat.
use super::{capture::InputRoute, linux_clock::Clock, shared::SharedInputBackend};
use bmz_core::{input::InputKind, latency::LatencyHistogram};
use bmz_gameplay::input::backend::{
    DeviceInputEvent, DeviceTimestamp, PhysicalControl, monotonic_timestamp_ns,
};
use evdev::{EventType, raw_stream::RawDevice};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::OpenOptions,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{fs::OpenOptionsExt, net::UnixStream},
    },
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

mod focus;

// 0 disabled, 1 ready, 2 unsupported display/session, 3 no selection,
// 4 permission, 5 missing node, 6 clock/setup/read failure, 7 focus lease expired.
static STATUS: AtomicU8 = AtomicU8::new(0);
static UNSUPPORTED_KEY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn unsupported_key_seen() -> bool {
    UNSUPPORTED_KEY.load(Ordering::Relaxed)
}
pub fn status() -> u8 {
    STATUS.load(Ordering::Acquire)
}
/// How long window-side UI suppression state stays trusted. A lapsed lease only stops
/// new presses: gameplay runs on its own thread, so a redraw stall must not cut holds.
const LEASE: Duration = Duration::from_millis(250);

/// Hard conditions (native, focus, clock) reset holds when they fail; a lapsed lease only
/// keeps new presses out, so releases of keys already delivered still arrive.
fn admits(kind: InputKind, permitted: bool, leased: bool) -> bool {
    permitted && (leased || kind == InputKind::Release)
}

#[derive(Default)]
struct Routing {
    route: Option<InputRoute>,
    generation: u64,
    lease: Option<Instant>,
    native: bool,
    held: HashMap<PhysicalControl, DeviceInputEvent>,
    window_held: HashSet<PhysicalControl>,
    window_blocked: HashSet<PhysicalControl>,
}

impl Routing {
    fn release(&mut self) {
        if let Some(route) = &self.route {
            for (_, mut event) in self.held.drain() {
                event.kind = InputKind::Release;
                event.timestamp = DeviceTimestamp::MonotonicNs(monotonic_timestamp_ns());
                route.input.push_shared_event(event);
            }
        } else {
            self.held.clear();
        }
        self.window_blocked.extend(self.window_held.iter().cloned());
    }
    fn set_native(&mut self, native: bool) {
        if self.native != native {
            self.release();
            self.native = native;
            self.generation = self.generation.wrapping_add(1);
        }
    }
    fn unavailable(&mut self, error: &io::Error) -> u8 {
        self.set_native(false);
        match error.kind() {
            io::ErrorKind::PermissionDenied => 4,
            io::ErrorKind::NotFound => 5,
            _ => 6,
        }
    }
    fn set_route(&mut self, route: Option<InputRoute>) {
        let route = route.filter(|r| r.focused && r.keyboard_enabled);
        let same = match (&self.route, &route) {
            (Some(a), Some(b)) => a.input.same_source(&b.input),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.release();
            self.generation = self.generation.wrapping_add(1);
            self.route = route;
        }
        self.lease = self.route.as_ref().map(|_| Instant::now());
    }
    fn leased(&self, now: Instant) -> bool {
        self.route.is_some()
            && self.lease.is_some_and(|t| now.saturating_duration_since(t) <= LEASE)
    }
    fn push(&mut self, event: DeviceInputEvent) {
        match event.kind {
            InputKind::Press => {
                self.held.insert(event.control.clone(), event.clone());
            }
            InputKind::Release => {
                self.held.remove(&event.control);
            }
        }
        if let Some(route) = &self.route {
            route.input.push_shared_event(event);
        }
    }
    fn window_event(&mut self, event: DeviceInputEvent, input: &SharedInputBackend) -> bool {
        match event.kind {
            InputKind::Press => {
                self.window_held.insert(event.control.clone());
            }
            InputKind::Release => {
                self.window_held.remove(&event.control);
            }
        }
        let blocked = self.window_blocked.contains(&event.control);
        if event.kind == InputKind::Release {
            self.window_blocked.remove(&event.control);
        }
        if self.native
            || blocked
            || !self.route.as_ref().is_some_and(|r| r.input.same_source(input))
        {
            return false;
        }
        self.push(event);
        true
    }
}

/// Device-local physical state, plus inhibition until release after a snapshot.
#[derive(Default)]
struct Keys {
    held: HashSet<u16>,
    blocked: HashSet<u16>,
    dropped: bool,
}
impl Keys {
    fn baseline(&mut self, keys: impl IntoIterator<Item = u16>) {
        self.held = keys.into_iter().collect();
        self.blocked = self.held.clone();
    }
    fn change(&mut self, code: u16, value: i32) -> Option<InputKind> {
        if self.dropped {
            return None;
        }
        match value {
            1 if self.held.insert(code) && !self.blocked.contains(&code) => Some(InputKind::Press),
            0 => {
                let held = self.held.remove(&code);
                let blocked = self.blocked.remove(&code);
                (held && !blocked).then_some(InputKind::Release)
            }
            _ => None, // includes autorepeat 2
        }
    }
}

fn other_key_held<'a>(
    mut keys: impl Iterator<Item = (usize, &'a Keys)>,
    index: usize,
    code: u16,
) -> bool {
    keys.any(|(other, keys)| {
        other != index && keys.held.contains(&code) && !keys.blocked.contains(&code)
    })
}

struct Device {
    raw: RawDevice,
    keys: Keys,
}

// Merge ready devices by kernel time; preserve each fd's order (including SYN
// markers) and use selection order to break ties reproducibly.
fn pop_earliest(batches: &mut [VecDeque<evdev::InputEvent>]) -> Option<(usize, evdev::InputEvent)> {
    let index = batches
        .iter()
        .enumerate()
        .filter_map(|(index, batch)| {
            let event = batch.front()?.as_ref();
            Some(((event.time.tv_sec, event.time.tv_usec, index), index))
        })
        .min_by_key(|(key, _)| *key)?
        .1;
    batches[index].pop_front().map(|event| (index, event))
}

pub struct Keyboard {
    routing: Arc<Mutex<Routing>>,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Keyboard {
    pub fn start(paths: Vec<String>, window: Option<u32>) -> io::Result<Self> {
        UNSUPPORTED_KEY.store(false, Ordering::Relaxed);
        let (wake, receive) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        receive.set_nonblocking(true)?;
        let routing = Arc::new(Mutex::new(Routing::default()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (worker, stopped) = (routing.clone(), stop.clone());
        let thread = thread::Builder::new().name("bmz-evdev".into()).spawn(move || {
            run(paths, window, receive, &worker, &stopped);
            worker.lock().unwrap_or_else(|e| e.into_inner()).set_native(false);
        })?;
        Ok(Self { routing, wake, thread: Some(thread), stop })
    }
    pub fn set_route(&self, route: Option<InputRoute>) {
        let mut routing = self.routing.lock().unwrap_or_else(|e| e.into_inner());
        let previous = routing.generation;
        routing.set_route(route);
        let changed = previous != routing.generation;
        drop(routing);
        if changed {
            let _ = (&self.wake).write(&[1]);
        }
    }
    pub fn active(&self) -> bool {
        self.routing.lock().unwrap_or_else(|e| e.into_inner()).native
    }
    pub fn window_event(&self, event: DeviceInputEvent, input: &SharedInputBackend) -> bool {
        self.routing.lock().unwrap_or_else(|e| e.into_inner()).window_event(event, input)
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = (&self.wake).write(&[1]);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        STATUS.store(0, Ordering::Release);
    }
}

/// Explicit stable aliases, never eventN or a product-name match. by-path
/// identifies a physical port, by-id usually includes a serial. Custom symlinks
/// allow virtual remappers without automatically reading the physical device too.
fn valid_path(path: &Path) -> bool {
    path.starts_with("/dev/input")
        && path.is_symlink()
        && path.file_name().is_some_and(|name| !name.to_string_lossy().starts_with("event"))
}

pub fn device_paths() -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for directory in ["/dev/input/by-id", "/dev/input/by-path"] {
        if let Ok(entries) = std::fs::read_dir(directory) {
            let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            for path in entries {
                if path.to_string_lossy().ends_with("-event-kbd")
                    && let Ok(target) = path.canonicalize()
                    && seen.insert(target)
                {
                    paths.push(path.to_string_lossy().into_owned());
                }
            }
        }
    }
    paths
}

fn open_devices(paths: &[String]) -> io::Result<Vec<Device>> {
    let mut seen = HashSet::new();
    let mut devices = Vec::new();
    for path in paths {
        let path = Path::new(path);
        // exists() turns EACCES into false, hiding a permission failure as a
        // missing node. Preserve the OS error for the settings status.
        std::fs::metadata(path)?;
        if !valid_path(path) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "select a stable /dev/input symlink",
            ));
        }
        let target = path.canonicalize()?;
        if !target.starts_with("/dev/input") || !seen.insert(target) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "duplicate or invalid input device",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        let clock: libc::c_int = libc::CLOCK_MONOTONIC;
        // Linux UAPI _IOW('E', 0xa0, int); pointer to initialized int is required.
        let result = unsafe {
            libc::ioctl(file.as_raw_fd(), libc::_IOW::<libc::c_int>(b'E' as u32, 0xa0), &clock)
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        let raw = RawDevice::from_fd(file.into())?;
        if !raw.supported_keys().is_some_and(|keys| keys.iter().any(|key| key.0 < 0x100)) {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "not a keyboard EV_KEY device"));
        }
        let mut device = Device { raw, keys: Keys::default() };
        resync(&mut device)?;
        devices.push(device);
    }
    Ok(devices)
}

fn resync(device: &mut Device) -> io::Result<()> {
    device.keys.baseline(device.raw.get_key_state()?.iter().map(|key| key.0));
    Ok(())
}

#[derive(Default)]
struct Diagnostics {
    generation: u128,
    age: LatencyHistogram,
    invalid: u64,
    syn_dropped: u64,
    resyncs: u64,
    queue_recoveries: u64,
    focus_suppressed: u64,
    unsupported: u64,
    epoch: u64,
}
impl Diagnostics {
    fn log(&self) {
        if bmz_core::latency::diagnostics_enabled() {
            let summary = serde_json::json!({"schema":1,"kind":"evdev","epoch":self.epoch,
                "generation":self.generation,
                "os_to_receive_ns":self.age.summary(),"invalid_timestamps":self.invalid,
                "syn_dropped":self.syn_dropped,"resyncs":self.resyncs,
                "queue_recoveries":self.queue_recoveries,"focus_suppressed":self.focus_suppressed,
                "unsupported_keys":self.unsupported,"clock":"CLOCK_MONOTONIC anchored to BMZ Instant",
                "status":status(),"focus_lease_ms":LEASE.as_millis()});
            tracing::info!("BMZ_LATENCY_JSON {summary}");
        }
    }
}

fn run(
    paths: Vec<String>,
    window: Option<u32>,
    mut wake: UnixStream,
    routing: &Mutex<Routing>,
    stop: &std::sync::atomic::AtomicBool,
) {
    let Some(mut focus) = window.and_then(|w| focus::Guard::new(w).ok()) else {
        STATUS.store(2, Ordering::Release);
        return;
    };
    if paths.is_empty() {
        STATUS.store(3, Ordering::Release);
        return;
    }
    let enabled = bmz_core::latency::diagnostics_enabled();
    let mut diagnostics =
        Diagnostics { generation: monotonic_timestamp_ns(), ..Default::default() };
    let mut devices = Vec::new();
    let mut retry_at = Instant::now();
    let mut clock = Clock::sample().map(Clock::new);
    let mut generation = u64::MAX;
    let mut overflow = 0;
    let mut active = false;
    let mut admit_after = monotonic_timestamp_ns();
    let mut last_log = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if devices.is_empty() && Instant::now() >= retry_at {
            retry_at = Instant::now() + Duration::from_secs(1);
            match open_devices(&paths) {
                Ok(opened) => {
                    devices = opened;
                    clock = Clock::sample().map(Clock::new);
                    diagnostics.log();
                    diagnostics.epoch += 1;
                    diagnostics.age = LatencyHistogram::default();
                    routing.lock().unwrap_or_else(|e| e.into_inner()).set_native(clock.is_some());
                    STATUS.store(if clock.is_some() { 1 } else { 6 }, Ordering::Release);
                    if clock.is_none() {
                        devices.clear();
                    }
                }
                Err(error) => {
                    let status =
                        routing.lock().unwrap_or_else(|e| e.into_inner()).unavailable(&error);
                    STATUS.store(status, Ordering::Release);
                }
            }
        }
        let mut polls = Vec::with_capacity(devices.len() + 1);
        polls.push(libc::pollfd { fd: wake.as_raw_fd(), events: libc::POLLIN, revents: 0 });
        polls.extend(devices.iter().map(|d| libc::pollfd {
            fd: d.raw.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        }));
        // Notification interrupts this wait. 10ms bounds focus/session checks;
        // unavailable devices retry once per second, with no high-frequency spin.
        let timeout = if devices.is_empty() { 1000 } else { 10 };
        let result =
            unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, timeout) };
        if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            STATUS.store(6, Ordering::Release);
            break;
        }
        let mut bytes = [0u8; 128];
        while wake.read(&mut bytes).is_ok_and(|n| n > 0) {}
        if stop.load(Ordering::Acquire) {
            break;
        }
        let sample = Clock::sample();
        // A preempted sample proves nothing about the clocks; keep the current mapping.
        let continuous = match sample {
            Some(sample) => clock.as_ref().is_some_and(|c| c.continuous(sample)),
            None => clock.is_some(),
        };
        // Do not query X while holding the routing lock; replies have a bounded wait.
        let focused = !devices.is_empty() && focus.focused();
        let mut route = routing.lock().unwrap_or_else(|e| e.into_inner());
        let input_overflow = route.route.as_ref().map_or(0, |r| r.input.overflow_count());
        let permitted = route.native && focused && continuous;
        let leased = route.leased(Instant::now());
        let reset = generation != route.generation
            || active != permitted
            || !continuous
            || input_overflow != overflow;
        let mut failed = false;
        if reset {
            admit_after = monotonic_timestamp_ns();
            route.release();
            for device in &mut devices {
                failed |= resync(device).is_err();
                diagnostics.resyncs += 1;
            }
            if !continuous {
                diagnostics.log();
                diagnostics.epoch += 1;
                diagnostics.age = LatencyHistogram::default();
                clock = sample.map(Clock::new);
            }
            if input_overflow != overflow {
                diagnostics.queue_recoveries += 1;
            }
            generation = route.generation;
            overflow = input_overflow;
            active = permitted;
        }
        if route.native {
            STATUS.store(if permitted && leased { 1 } else { 7 }, Ordering::Release);
        }
        let mut batches: Vec<VecDeque<evdev::InputEvent>> = vec![VecDeque::new(); devices.len()];
        for index in 0..devices.len() {
            if failed {
                break;
            }
            if polls[index + 1].revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                failed = true;
                break;
            }
            if polls[index + 1].revents & libc::POLLIN == 0 {
                continue;
            }
            batches[index] = match devices[index].raw.fetch_events() {
                Ok(events) => events.collect(),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                Err(_) => {
                    failed = true;
                    break;
                }
            };
        }
        let received = monotonic_timestamp_ns();
        let now_os = super::linux_clock::clock_ns(libc::CLOCK_MONOTONIC);
        while !failed {
            let Some((index, event)) = pop_earliest(&mut batches) else {
                break;
            };
            if event.event_type() == EventType::SYNCHRONIZATION {
                if event.code() == 3 {
                    // SYN_DROPPED: discard through next SYN_REPORT.
                    devices[index].keys.dropped = true;
                    diagnostics.syn_dropped += 1;
                    route.release();
                    for device in &mut devices {
                        device.keys.blocked.extend(device.keys.held.iter().copied());
                    }
                } else if event.code() == 0 && devices[index].keys.dropped {
                    admit_after = monotonic_timestamp_ns();
                    failed |= resync(&mut devices[index]).is_err();
                    devices[index].keys.dropped = false;
                    diagnostics.resyncs += 1;
                }
                continue;
            }
            if devices[index].keys.dropped || event.event_type() != EventType::KEY {
                continue;
            }
            let code = event.code();
            let Some(kind) = devices[index].keys.change(code, event.value()) else {
                continue;
            };
            let Some(control) = super::linux_keys::control(code) else {
                diagnostics.unsupported += 1;
                if !UNSUPPORTED_KEY.swap(true, Ordering::Relaxed) {
                    tracing::warn!(
                        "evdev received an unsupported key; select winit to restore window input"
                    );
                }
                continue;
            };
            if reset || !admits(kind, permitted, leased) {
                if kind == InputKind::Press {
                    devices[index].keys.blocked.insert(code);
                }
                diagnostics.focus_suppressed += 1;
                continue;
            }
            // Aggregate identical physical controls across explicitly selected keyboards.
            let other_held =
                other_key_held(devices.iter().enumerate().map(|(i, d)| (i, &d.keys)), index, code);
            if other_held {
                continue;
            }
            let raw = event.as_ref();
            let timestamp = clock
                .as_ref()
                .and_then(|clock| clock.convert(raw.time.tv_sec, raw.time.tv_usec, now_os?));
            let Some(timestamp) = timestamp else {
                diagnostics.invalid += 1;
                admit_after = received;
                route.release();
                for device in &mut devices {
                    failed |= resync(device).is_err();
                }
                continue;
            };
            if timestamp < admit_after {
                if kind == InputKind::Press {
                    devices[index].keys.blocked.insert(code);
                }
                diagnostics.focus_suppressed += 1;
                continue;
            }
            if enabled {
                diagnostics
                    .age
                    .record(received.saturating_sub(timestamp).min(u64::MAX as u128) as u64);
            }
            route.push(DeviceInputEvent {
                device: super::winit::W_KEYBOARD_DEVICE_ID,
                control,
                kind,
                timestamp: DeviceTimestamp::MonotonicNs(timestamp),
                bounce_policy: Default::default(),
            });
        }
        if failed {
            route.set_native(false);
            devices.clear();
            active = false;
            STATUS.store(6, Ordering::Release);
        }
        drop(route);
        if last_log.elapsed() >= Duration::from_secs(5) {
            diagnostics.log();
            last_log = Instant::now();
        }
    }
    diagnostics.log();
}

#[cfg(test)]
mod tests;
