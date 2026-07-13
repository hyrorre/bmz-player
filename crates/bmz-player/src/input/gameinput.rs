use std::collections::HashMap;
use std::ffi::{CStr, c_void};
use std::ptr;
use std::thread::ThreadId;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use bmz_gameplay::input::backend::{DeviceId, DeviceTimestamp, monotonic_timestamp_ns};
use windows_sys::Win32::Foundation::{APP_LOCAL_DEVICE_ID, FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use super::gamepad::{
    AnalogGamepadProcessor, ConnectedGamepad, GamepadButtonEvent, GamepadPollOutput,
    RawControlCode, RawInputEvent, RawInputEventKind, current_device_timestamp,
    gamepad_device_id_from_backend_index,
};

const GAME_INPUT_KIND_CONTROLLER: u32 = 0x0000_000e;
const GAME_INPUT_DEVICE_CONNECTED: u32 = 0x0000_0001;
const MAX_CONTROLLER_AXES: usize = 64;
const MAX_CONTROLLER_BUTTONS: usize = 256;

type HResult = i32;
type GameInputCreate = unsafe extern "system" fn(*mut *mut IGameInput) -> HResult;

#[repr(C)]
struct IGameInput {
    vtable: *const IGameInputVTable,
}

#[repr(C)]
struct IGameInputVTable {
    query_interface:
        unsafe extern "system" fn(*mut IGameInput, *const c_void, *mut *mut c_void) -> HResult,
    add_ref: unsafe extern "system" fn(*mut IGameInput) -> u32,
    release: unsafe extern "system" fn(*mut IGameInput) -> u32,
    get_current_timestamp: unsafe extern "system" fn(*mut IGameInput) -> u64,
    get_current_reading: unsafe extern "system" fn(
        *mut IGameInput,
        u32,
        *mut IGameInputDevice,
        *mut *mut IGameInputReading,
    ) -> HResult,
    get_next_reading: unsafe extern "system" fn(
        *mut IGameInput,
        *mut IGameInputReading,
        u32,
        *mut IGameInputDevice,
        *mut *mut IGameInputReading,
    ) -> HResult,
    get_previous_reading: unsafe extern "system" fn(
        *mut IGameInput,
        *mut IGameInputReading,
        u32,
        *mut IGameInputDevice,
        *mut *mut IGameInputReading,
    ) -> HResult,
}

#[repr(C)]
struct IGameInputReading {
    vtable: *const IGameInputReadingVTable,
}

#[repr(C)]
struct IGameInputReadingVTable {
    query_interface: unsafe extern "system" fn(
        *mut IGameInputReading,
        *const c_void,
        *mut *mut c_void,
    ) -> HResult,
    add_ref: unsafe extern "system" fn(*mut IGameInputReading) -> u32,
    release: unsafe extern "system" fn(*mut IGameInputReading) -> u32,
    get_input_kind: unsafe extern "system" fn(*mut IGameInputReading) -> u32,
    get_sequence_number: unsafe extern "system" fn(*mut IGameInputReading, u32) -> u64,
    get_timestamp: unsafe extern "system" fn(*mut IGameInputReading) -> u64,
    get_device: unsafe extern "system" fn(*mut IGameInputReading, *mut *mut IGameInputDevice),
    get_raw_report: unsafe extern "system" fn(*mut IGameInputReading, *mut *mut c_void) -> u8,
    get_controller_axis_count: unsafe extern "system" fn(*mut IGameInputReading) -> u32,
    get_controller_axis_state:
        unsafe extern "system" fn(*mut IGameInputReading, u32, *mut f32) -> u32,
    get_controller_button_count: unsafe extern "system" fn(*mut IGameInputReading) -> u32,
    get_controller_button_state:
        unsafe extern "system" fn(*mut IGameInputReading, u32, *mut u8) -> u32,
}

#[repr(C)]
struct IGameInputDevice {
    vtable: *const IGameInputDeviceVTable,
}

#[repr(C)]
struct IGameInputDeviceVTable {
    query_interface: unsafe extern "system" fn(
        *mut IGameInputDevice,
        *const c_void,
        *mut *mut c_void,
    ) -> HResult,
    add_ref: unsafe extern "system" fn(*mut IGameInputDevice) -> u32,
    release: unsafe extern "system" fn(*mut IGameInputDevice) -> u32,
    get_device_info:
        unsafe extern "system" fn(*mut IGameInputDevice) -> *const GameInputDeviceInfoPrefix,
    get_device_status: unsafe extern "system" fn(*mut IGameInputDevice) -> u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GameInputUsage {
    page: u16,
    id: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GameInputVersion {
    major: u16,
    minor: u16,
    build: u16,
    revision: u16,
}

// deviceIdまでがGameInputDeviceInfoの固定prefix。後続の可変情報は参照しない。
#[repr(C)]
struct GameInputDeviceInfoPrefix {
    info_size: u32,
    vendor_id: u16,
    product_id: u16,
    revision_number: u16,
    interface_number: u8,
    collection_number: u8,
    usage: GameInputUsage,
    hardware_version: GameInputVersion,
    firmware_version: GameInputVersion,
    device_id: APP_LOCAL_DEVICE_ID,
}

struct DeviceState {
    stable_id: String,
    backend_id: u32,
    device_id: DeviceId,
    name: String,
    device: *mut IGameInputDevice,
    connected: bool,
    buttons: Vec<bool>,
    axes: Vec<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameInputPollDiagnostics {
    pub reading_count: u32,
    pub oldest_reading_age_us: u64,
}

pub struct GameInputBackend {
    module: HMODULE,
    game_input: *mut IGameInput,
    owner_thread: ThreadId,
    startup_timestamp_us: u64,
    last_reading: *mut IGameInputReading,
    devices: HashMap<String, DeviceState>,
    next_backend_id: u32,
    analog: AnalogGamepadProcessor,
    diagnostics: GameInputPollDiagnostics,
}

impl GameInputBackend {
    pub fn new(sensitivity: f32, scratch_threshold: u32) -> Result<Self> {
        let library_name: Vec<u16> = "GameInput.dll".encode_utf16().chain(Some(0)).collect();
        // SAFETY: library_name is nul-terminated and remains alive for the call.
        let module = unsafe { LoadLibraryW(library_name.as_ptr()) };
        if module.is_null() {
            bail!("GameInput.dll is not available");
        }

        let create = match load_create_function(module) {
            Ok(create) => create,
            Err(error) => {
                // SAFETY: module came from LoadLibraryW and is not used after this point.
                unsafe { FreeLibrary(module) };
                return Err(error);
            }
        };
        let mut game_input = ptr::null_mut();
        // SAFETY: create is the GameInputCreate export and receives a valid output pointer.
        let result = unsafe { create(&mut game_input) };
        if result < 0 || game_input.is_null() {
            // SAFETY: module came from LoadLibraryW and GameInputCreate did not return an object.
            unsafe { FreeLibrary(module) };
            bail!("GameInputCreate failed with HRESULT 0x{:08x}", result as u32);
        }

        // SAFETY: GameInputCreate returned a live object with the documented vtable.
        let startup_timestamp_us =
            unsafe { ((*(*game_input).vtable).get_current_timestamp)(game_input) };
        Ok(Self {
            module,
            game_input,
            owner_thread: std::thread::current().id(),
            startup_timestamp_us,
            last_reading: ptr::null_mut(),
            devices: HashMap::new(),
            next_backend_id: 0,
            analog: AnalogGamepadProcessor::new(sensitivity, scratch_threshold),
            diagnostics: GameInputPollDiagnostics::default(),
        })
    }

    pub fn poll(&mut self) -> GamepadPollOutput {
        debug_assert_eq!(self.owner_thread, std::thread::current().id());
        let mut output = GamepadPollOutput::default();
        self.diagnostics = GameInputPollDiagnostics::default();
        self.analog.check_timeouts(Instant::now(), &mut output.buttons);

        let backend_now_us = self.current_timestamp_us();
        let monotonic_now_ns = monotonic_timestamp_ns();
        if self.last_reading.is_null() {
            self.bootstrap_readings(backend_now_us, monotonic_now_ns, &mut output);
        } else {
            self.drain_next_readings(backend_now_us, monotonic_now_ns, &mut output);
        }
        self.release_disconnected_devices(&mut output);
        self.analog.check_timeouts(Instant::now(), &mut output.buttons);
        output
    }

    pub fn connected_gamepads(&self) -> Vec<ConnectedGamepad> {
        let mut devices: Vec<_> = self
            .devices
            .values()
            .map(|device| ConnectedGamepad {
                stable_id: device.stable_id.clone(),
                backend_id: device.backend_id,
                device_id: device.device_id,
                name: device.name.clone(),
                is_connected: device.connected,
            })
            .collect();
        devices.sort_by_key(|device| device.backend_id);
        devices
    }

    pub fn diagnostics(&self) -> GameInputPollDiagnostics {
        self.diagnostics
    }

    fn bootstrap_readings(
        &mut self,
        backend_now_us: u64,
        monotonic_now_ns: u128,
        output: &mut GamepadPollOutput,
    ) {
        let mut current = ptr::null_mut();
        // SAFETY: pointers belong to the live GameInput object and output is writable.
        let result = unsafe {
            ((*(*self.game_input).vtable).get_current_reading)(
                self.game_input,
                GAME_INPUT_KIND_CONTROLLER,
                ptr::null_mut(),
                &mut current,
            )
        };
        if result < 0 || current.is_null() {
            return;
        }

        let mut history = vec![current];
        loop {
            let reference = *history.last().expect("history contains current reading");
            let mut previous = ptr::null_mut();
            // SAFETY: reference is a live reading and previous is a valid output pointer.
            let result = unsafe {
                ((*(*self.game_input).vtable).get_previous_reading)(
                    self.game_input,
                    reference,
                    GAME_INPUT_KIND_CONTROLLER,
                    ptr::null_mut(),
                    &mut previous,
                )
            };
            if result < 0 || previous.is_null() {
                break;
            }
            if reading_timestamp_us(previous) < self.startup_timestamp_us {
                release_reading(previous);
                break;
            }
            history.push(previous);
        }

        for &reading in history.iter().rev() {
            self.process_reading(reading, backend_now_us, monotonic_now_ns, output);
        }
        for reading in history.into_iter().skip(1) {
            release_reading(reading);
        }
        self.last_reading = current;
    }

    fn drain_next_readings(
        &mut self,
        backend_now_us: u64,
        monotonic_now_ns: u128,
        output: &mut GamepadPollOutput,
    ) {
        loop {
            let mut next = ptr::null_mut();
            // SAFETY: last_reading and game_input stay live for the call.
            let result = unsafe {
                ((*(*self.game_input).vtable).get_next_reading)(
                    self.game_input,
                    self.last_reading,
                    GAME_INPUT_KIND_CONTROLLER,
                    ptr::null_mut(),
                    &mut next,
                )
            };
            if result < 0 || next.is_null() {
                break;
            }
            self.process_reading(next, backend_now_us, monotonic_now_ns, output);
            release_reading(self.last_reading);
            self.last_reading = next;
        }
    }

    fn process_reading(
        &mut self,
        reading: *mut IGameInputReading,
        backend_now_us: u64,
        monotonic_now_ns: u128,
        output: &mut GamepadPollOutput,
    ) {
        let reading_us = reading_timestamp_us(reading);
        let timestamp = map_gameinput_timestamp(reading_us, backend_now_us, monotonic_now_ns);
        self.diagnostics.reading_count = self.diagnostics.reading_count.saturating_add(1);
        self.diagnostics.oldest_reading_age_us =
            self.diagnostics.oldest_reading_age_us.max(backend_now_us.saturating_sub(reading_us));

        let mut device = ptr::null_mut();
        // SAFETY: reading is live and device is a valid output pointer.
        unsafe { ((*(*reading).vtable).get_device)(reading, &mut device) };
        if device.is_null() {
            return;
        }
        let Some(identity) = device_identity(device) else {
            release_device(device);
            return;
        };

        if !self.devices.contains_key(&identity.stable_id) {
            let backend_id = self.next_backend_id;
            self.next_backend_id = self.next_backend_id.saturating_add(1);
            self.devices.insert(
                identity.stable_id.clone(),
                DeviceState {
                    stable_id: identity.stable_id.clone(),
                    backend_id,
                    device_id: gamepad_device_id_from_backend_index(backend_id),
                    name: identity.name,
                    device,
                    connected: true,
                    buttons: Vec::new(),
                    axes: Vec::new(),
                },
            );
            tracing::info!(device = %identity.stable_id, "GameInput controller connected");
        } else {
            release_device(device);
        }

        let Some(state) = self.devices.get_mut(&identity.stable_id) else { return };
        state.connected = true;
        let button_count = controller_button_count(reading).min(MAX_CONTROLLER_BUTTONS);
        let buttons = controller_button_state(reading, button_count);
        for (index, &pressed) in buttons.iter().enumerate() {
            let previous = state.buttons.get(index).copied().unwrap_or(false);
            if pressed == previous {
                continue;
            }
            let name = format!("Button{}", index + 1);
            output.raw_events.push(RawInputEvent {
                device_id: state.device_id,
                kind: RawInputEventKind::Button,
                logical: name.clone(),
                raw_code: RawControlCode { value: index as u32, label: format!("Button({index})") },
                timestamp,
                mapped_control: Some(name.clone()),
                pressed: Some(pressed),
                value: None,
                ticks: None,
            });
            output.buttons.push(GamepadButtonEvent {
                name,
                device_id: state.device_id,
                pressed,
                timestamp,
            });
        }
        state.buttons = buttons;

        let axis_count = controller_axis_count(reading).min(MAX_CONTROLLER_AXES);
        let axes = controller_axis_state(reading, axis_count);
        for (index, &value) in axes.iter().enumerate() {
            let normalized = normalize_controller_axis(value);
            let previous = state.axes.get(index).copied().unwrap_or(normalized);
            if normalized.to_bits() == previous.to_bits() {
                continue;
            }
            let axis_name = format!("Axis{}", index + 1);
            self.analog.process_axis(
                state.device_id,
                index as u32,
                &axis_name,
                axis_name.clone(),
                RawControlCode { value: index as u32, label: format!("Axis({index})") },
                normalized,
                timestamp,
                output,
            );
        }
        state.axes = axes.into_iter().map(normalize_controller_axis).collect();
    }

    fn release_disconnected_devices(&mut self, output: &mut GamepadPollOutput) {
        let timestamp = current_device_timestamp();
        for state in self.devices.values_mut() {
            // SAFETY: device is retained by DeviceState until backend drop.
            let status = unsafe { ((*(*state.device).vtable).get_device_status)(state.device) };
            let connected = status & GAME_INPUT_DEVICE_CONNECTED != 0;
            if state.connected && !connected {
                for (index, pressed) in state.buttons.iter_mut().enumerate() {
                    if *pressed {
                        *pressed = false;
                        output.buttons.push(GamepadButtonEvent {
                            name: format!("Button{}", index + 1),
                            device_id: state.device_id,
                            pressed: false,
                            timestamp,
                        });
                    }
                }
                self.analog.release_device(state.device_id, timestamp, &mut output.buttons);
                tracing::info!(device = %state.stable_id, "GameInput controller disconnected");
            }
            state.connected = connected;
        }
    }

    fn current_timestamp_us(&self) -> u64 {
        // SAFETY: game_input stays live until backend drop.
        unsafe { ((*(*self.game_input).vtable).get_current_timestamp)(self.game_input) }
    }
}

impl Drop for GameInputBackend {
    fn drop(&mut self) {
        debug_assert_eq!(self.owner_thread, std::thread::current().id());
        for state in self.devices.values() {
            release_device(state.device);
        }
        if !self.last_reading.is_null() {
            release_reading(self.last_reading);
        }
        if !self.game_input.is_null() {
            // SAFETY: game_input is the retained GameInputCreate result.
            unsafe { ((*(*self.game_input).vtable).release)(self.game_input) };
        }
        if !self.module.is_null() {
            // SAFETY: all interfaces from the module have been released above.
            unsafe { FreeLibrary(self.module) };
        }
    }
}

struct DeviceIdentity {
    stable_id: String,
    name: String,
}

fn load_create_function(module: HMODULE) -> Result<GameInputCreate> {
    let name = CStr::from_bytes_with_nul(b"GameInputCreate\0").expect("static export name");
    // SAFETY: module is live and name is nul-terminated.
    let function = unsafe { GetProcAddress(module, name.as_ptr().cast::<u8>()) }
        .context("GameInputCreate export is not available")?;
    // SAFETY: the export has the GameInputCreate signature documented by GameInput.h.
    Ok(unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, GameInputCreate>(function)
    })
}

fn device_identity(device: *mut IGameInputDevice) -> Option<DeviceIdentity> {
    // SAFETY: device is a live GameInput device pointer.
    let info = unsafe { ((*(*device).vtable).get_device_info)(device) };
    if info.is_null() {
        return None;
    }
    // SAFETY: GameInput guarantees the info pointer for the lifetime of the device.
    let info = unsafe { &*info };
    let stable_id = info.device_id.value.iter().map(|byte| format!("{byte:02x}")).collect();
    let name = format!("GameInput {:04X}:{:04X}", info.vendor_id, info.product_id);
    Some(DeviceIdentity { stable_id, name })
}

fn controller_button_count(reading: *mut IGameInputReading) -> usize {
    // SAFETY: reading is live for the duration of processing.
    unsafe { ((*(*reading).vtable).get_controller_button_count)(reading) as usize }
}

fn controller_button_state(reading: *mut IGameInputReading, count: usize) -> Vec<bool> {
    let mut bytes = vec![0u8; count];
    // SAFETY: bytes has space for count one-byte C bool values.
    let written = unsafe {
        ((*(*reading).vtable).get_controller_button_state)(
            reading,
            count as u32,
            bytes.as_mut_ptr(),
        )
    } as usize;
    bytes.truncate(written.min(count));
    bytes.into_iter().map(|value| value != 0).collect()
}

fn controller_axis_count(reading: *mut IGameInputReading) -> usize {
    // SAFETY: reading is live for the duration of processing.
    unsafe { ((*(*reading).vtable).get_controller_axis_count)(reading) as usize }
}

fn controller_axis_state(reading: *mut IGameInputReading, count: usize) -> Vec<f32> {
    let mut values = vec![0.0; count];
    // SAFETY: values has space for count f32 values.
    let written = unsafe {
        ((*(*reading).vtable).get_controller_axis_state)(reading, count as u32, values.as_mut_ptr())
    } as usize;
    values.truncate(written.min(count));
    values
}

fn reading_timestamp_us(reading: *mut IGameInputReading) -> u64 {
    // SAFETY: reading is live for the duration of processing.
    unsafe { ((*(*reading).vtable).get_timestamp)(reading) }
}

fn release_reading(reading: *mut IGameInputReading) {
    // SAFETY: all callers pass one retained reading reference exactly once.
    unsafe { ((*(*reading).vtable).release)(reading) };
}

fn release_device(device: *mut IGameInputDevice) {
    // SAFETY: all callers pass one retained device reference exactly once.
    unsafe { ((*(*device).vtable).release)(device) };
}

fn normalize_controller_axis(value: f32) -> f32 {
    (value.clamp(0.0, 1.0) * 2.0) - 1.0
}

fn map_gameinput_timestamp(
    reading_us: u64,
    backend_now_us: u64,
    monotonic_now_ns: u128,
) -> DeviceTimestamp {
    let timestamp_ns = if reading_us <= backend_now_us {
        monotonic_now_ns.saturating_sub(u128::from(backend_now_us - reading_us) * 1_000)
    } else {
        monotonic_now_ns.saturating_add(u128::from(reading_us - backend_now_us) * 1_000)
    };
    DeviceTimestamp::MonotonicNs(timestamp_ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gameinput_microseconds_to_monotonic_nanoseconds() {
        assert_eq!(
            map_gameinput_timestamp(9_750, 10_000, 2_000_000),
            DeviceTimestamp::MonotonicNs(1_750_000)
        );
        assert_eq!(
            map_gameinput_timestamp(10_250, 10_000, 2_000_000),
            DeviceTimestamp::MonotonicNs(2_250_000)
        );
    }

    #[test]
    fn normalizes_generic_controller_axis_range() {
        assert_eq!(normalize_controller_axis(0.0), -1.0);
        assert_eq!(normalize_controller_axis(0.5), 0.0);
        assert_eq!(normalize_controller_axis(1.0), 1.0);
    }

    #[test]
    fn creates_and_polls_installed_gameinput_runtime() {
        let Ok(mut backend) = GameInputBackend::new(1.0, 100) else { return };
        let _ = backend.poll();
        assert_eq!(backend.owner_thread, std::thread::current().id());
    }
}
