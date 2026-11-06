//! Native WASAPI helpers for low-latency shared and event-driven exclusive output.
//!
//! Shared low-latency mode keeps a silent period-control stream beside CPAL. Exclusive mode owns
//! the endpoint stream and feeds the existing mixer from a dedicated event-driven worker.

use std::cell::Cell;
use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::slice;
use std::sync::{Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_SHAREMODE_EXCLUSIVE,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_NOPERSIST, IAudioClient, IAudioClient3,
    IAudioClock, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    WAVE_FORMAT_PCM, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::Media::KernelStreaming::{
    KSAUDIO_SPEAKER_DIRECTOUT, KSDATAFORMAT_SUBTYPE_PCM, SPEAKER_FRONT_CENTER, SPEAKER_FRONT_LEFT,
    SPEAKER_FRONT_RIGHT, WAVE_FORMAT_EXTENSIBLE,
};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW, INFINITE,
    SetEvent, WaitForMultipleObjects,
};
use windows::core::PCWSTR;

use crate::backend::output::{
    NativeOutputRenderer, OutputSample, duration_to_frames, frames_to_duration,
};

const EXCLUSIVE_RECOVERY_INITIAL_DELAY_MS: u32 = 100;
const EXCLUSIVE_RECOVERY_MAX_DELAY_MS: u32 = 5_000;

#[derive(Debug, Clone, Copy)]
pub(crate) struct WasapiSharedPeriodInfo {
    pub queried_engine_sample_rate: u32,
    pub current_engine_sample_rate: u32,
    pub default_period_frames: u32,
    pub fundamental_period_frames: u32,
    pub min_period_frames: u32,
    pub max_period_frames: u32,
    pub selected_period_frames: u32,
    pub current_period_frames: u32,
    pub client_period_frames: u32,
    pub buffer_frames: u32,
}

#[derive(Debug, Error)]
pub(crate) enum WasapiSharedPeriodError {
    #[error("failed to create the WASAPI low-latency worker: {0}")]
    Spawn(#[source] io::Error),
    #[error("WASAPI low-latency worker stopped during initialization")]
    WorkerStopped,
    #[error("WASAPI low-latency initialization failed: {0}")]
    Initialization(String),
}

pub(crate) struct WasapiSharedPeriodGuard {
    stop_event: OwnedEvent,
    worker: Option<JoinHandle<()>>,
    info: WasapiSharedPeriodInfo,
}

impl WasapiSharedPeriodGuard {
    pub(crate) fn open(
        endpoint_id: String,
        client_sample_rate: u32,
        requested_client_frames: Option<u32>,
    ) -> Result<Self, WasapiSharedPeriodError> {
        let stop_event = OwnedEvent::new().map_err(|error| {
            WasapiSharedPeriodError::Initialization(format!("CreateEventW(stop): {error}"))
        })?;
        // HANDLE contains a raw pointer and is not Send. Kernel handles are valid process-wide,
        // so pass its value to the worker and rebuild the transparent wrapper there.
        let stop_event_value = stop_event.handle().0 as usize;
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("bmz-wasapi-period".to_string())
            .spawn(move || {
                let stop_event = HANDLE(stop_event_value as *mut c_void);
                worker_main(
                    endpoint_id,
                    client_sample_rate,
                    requested_client_frames,
                    stop_event,
                    startup_sender,
                );
            })
            .map_err(WasapiSharedPeriodError::Spawn)?;

        match startup_receiver.recv() {
            Ok(Ok(info)) => Ok(Self { stop_event, worker: Some(worker), info }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(WasapiSharedPeriodError::Initialization(error))
            }
            Err(_) => {
                let _ = worker.join();
                Err(WasapiSharedPeriodError::WorkerStopped)
            }
        }
    }

    pub(crate) fn info(&self) -> WasapiSharedPeriodInfo {
        self.info
    }
}

impl Drop for WasapiSharedPeriodGuard {
    fn drop(&mut self) {
        if let Err(error) = unsafe { SetEvent(self.stop_event.handle()) } {
            tracing::warn!(%error, "failed to stop WASAPI low-latency worker");
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("WASAPI low-latency worker panicked while stopping");
        }
    }
}

struct OwnedEvent(HANDLE);

impl OwnedEvent {
    fn new() -> windows::core::Result<Self> {
        // Auto-reset events are required by event-driven WASAPI streams.
        unsafe { CreateEventW(None, false, false, None) }.map(Self)
    }

    fn handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        if let Err(error) = unsafe { CloseHandle(self.0) } {
            tracing::warn!(%error, "failed to close WASAPI event handle");
        }
    }
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| format!("CoInitializeEx: {error}"))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct TaskMemFormat(*mut WAVEFORMATEX);

impl TaskMemFormat {
    fn as_ptr(&self) -> *const WAVEFORMATEX {
        self.0
    }

    fn sample_rate(&self) -> Result<u32, String> {
        if self.0.is_null() {
            return Err("IAudioClient3::GetMixFormat returned a null format".to_string());
        }
        Ok(unsafe { (*self.0).nSamplesPerSec })
    }
}

impl Drop for TaskMemFormat {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
        }
    }
}

struct WorkerStream {
    client: IAudioClient3,
    render_client: IAudioRenderClient,
    audio_event: OwnedEvent,
    buffer_frames: u32,
}

impl WorkerStream {
    fn open(
        endpoint_id: &str,
        client_sample_rate: u32,
        requested_client_frames: Option<u32>,
    ) -> Result<(Self, WasapiSharedPeriodInfo), String> {
        let enumerator: IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|error| format!("CoCreateInstance(MMDeviceEnumerator): {error}"))?
        };
        let endpoint_id_wide =
            endpoint_id.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
        let endpoint = unsafe {
            enumerator
                .GetDevice(PCWSTR(endpoint_id_wide.as_ptr()))
                .map_err(|error| format!("IMMDeviceEnumerator::GetDevice: {error}"))?
        };
        let client: IAudioClient3 = unsafe {
            endpoint
                .Activate(CLSCTX_ALL, None)
                .map_err(|error| format!("IMMDevice::Activate<IAudioClient3>: {error}"))?
        };
        let format = TaskMemFormat(unsafe {
            client
                .GetMixFormat()
                .map_err(|error| format!("IAudioClient3::GetMixFormat: {error}"))?
        });
        let engine_sample_rate = format.sample_rate()?;
        if engine_sample_rate == 0 || client_sample_rate == 0 {
            return Err("WASAPI reported an invalid zero sample rate".to_string());
        }

        let mut default_period_frames = 0;
        let mut fundamental_period_frames = 0;
        let mut min_period_frames = 0;
        let mut max_period_frames = 0;
        unsafe {
            client
                .GetSharedModeEnginePeriod(
                    format.as_ptr(),
                    &mut default_period_frames,
                    &mut fundamental_period_frames,
                    &mut min_period_frames,
                    &mut max_period_frames,
                )
                .map_err(|error| format!("IAudioClient3::GetSharedModeEnginePeriod: {error}"))?;
        }

        let requested_engine_frames = requested_client_frames
            .map(|frames| scale_frames_ceil(frames, client_sample_rate, engine_sample_rate))
            .transpose()?;
        let selected_period_frames = select_period_frames(
            requested_engine_frames,
            fundamental_period_frames,
            min_period_frames,
            max_period_frames,
        )?;
        let audio_event =
            OwnedEvent::new().map_err(|error| format!("CreateEventW(audio): {error}"))?;
        unsafe {
            client
                .InitializeSharedAudioStream(
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                    selected_period_frames,
                    format.as_ptr(),
                    None,
                )
                .map_err(|error| format!("IAudioClient3::InitializeSharedAudioStream: {error}"))?;
            client
                .SetEventHandle(audio_event.handle())
                .map_err(|error| format!("IAudioClient3::SetEventHandle: {error}"))?;
        }
        let mut current_format_ptr = std::ptr::null_mut();
        let mut current_period_frames = 0;
        unsafe {
            client
                .GetCurrentSharedModeEnginePeriod(
                    &mut current_format_ptr,
                    &mut current_period_frames,
                )
                .map_err(|error| {
                    format!("IAudioClient3::GetCurrentSharedModeEnginePeriod: {error}")
                })?;
        }
        let current_format = TaskMemFormat(current_format_ptr);
        let current_engine_sample_rate = current_format.sample_rate()?;
        if current_period_frames == 0 {
            return Err("WASAPI reported an invalid zero current engine period".to_string());
        }
        let client_period_frames = scale_frames_ceil(
            current_period_frames,
            current_engine_sample_rate,
            client_sample_rate,
        )?;
        let buffer_frames = unsafe {
            client
                .GetBufferSize()
                .map_err(|error| format!("IAudioClient3::GetBufferSize: {error}"))?
        };
        let render_client: IAudioRenderClient = unsafe {
            client.GetService().map_err(|error| {
                format!("IAudioClient3::GetService<IAudioRenderClient>: {error}")
            })?
        };
        fill_silence(&render_client, buffer_frames)?;
        unsafe {
            client.Start().map_err(|error| format!("IAudioClient3::Start: {error}"))?;
        }

        let info = WasapiSharedPeriodInfo {
            queried_engine_sample_rate: engine_sample_rate,
            current_engine_sample_rate,
            default_period_frames,
            fundamental_period_frames,
            min_period_frames,
            max_period_frames,
            selected_period_frames,
            current_period_frames,
            client_period_frames,
            buffer_frames,
        };
        Ok((Self { client, render_client, audio_event, buffer_frames }, info))
    }

    fn run(&self, stop_event: HANDLE) -> Result<(), String> {
        let mut consecutive_refill_errors = 0u64;
        loop {
            let wait = unsafe {
                WaitForMultipleObjects(&[stop_event, self.audio_event.handle()], false, INFINITE)
            };
            if wait == WAIT_OBJECT_0 {
                return Ok(());
            }
            if wait == WAIT_FAILED {
                return Err(format!(
                    "WaitForMultipleObjects: {}",
                    windows::core::Error::from_thread()
                ));
            }
            if wait.0 != WAIT_OBJECT_0.0 + 1 {
                return Err(format!("WaitForMultipleObjects returned unexpected value {}", wait.0));
            }

            let refill_result = (|| {
                let padding = unsafe {
                    self.client
                        .GetCurrentPadding()
                        .map_err(|error| format!("IAudioClient3::GetCurrentPadding: {error}"))?
                };
                let available = self.buffer_frames.saturating_sub(padding);
                fill_silence(&self.render_client, available)
            })();
            match refill_result {
                Ok(()) => consecutive_refill_errors = 0,
                Err(error) => {
                    consecutive_refill_errors = consecutive_refill_errors.saturating_add(1);
                    // Keep the initialized period stream alive. A transient buffer error must not
                    // silently return the shared engine to its default period while CPAL is still
                    // running with the low-latency ring buffer. Log at exponentially increasing
                    // intervals to avoid flooding at millisecond engine periods.
                    if consecutive_refill_errors.is_power_of_two() {
                        tracing::warn!(
                            %error,
                            consecutive_refill_errors,
                            "failed to refill WASAPI low-latency period stream; keeping it active",
                        );
                    }
                }
            }
        }
    }
}

impl Drop for WorkerStream {
    fn drop(&mut self) {
        if let Err(error) = unsafe { self.client.Stop() } {
            tracing::warn!(%error, "failed to stop WASAPI low-latency period stream");
        }
    }
}

fn worker_main(
    endpoint_id: String,
    client_sample_rate: u32,
    requested_client_frames: Option<u32>,
    stop_event: HANDLE,
    startup_sender: mpsc::SyncSender<Result<WasapiSharedPeriodInfo, String>>,
) {
    let apartment = match ComApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = startup_sender.send(Err(error));
            return;
        }
    };
    let _apartment = apartment;
    let (stream, info) =
        match WorkerStream::open(&endpoint_id, client_sample_rate, requested_client_frames) {
            Ok(value) => value,
            Err(error) => {
                let _ = startup_sender.send(Err(error));
                return;
            }
        };
    if startup_sender.send(Ok(info)).is_err() {
        return;
    }
    if let Err(error) = stream.run(stop_event) {
        tracing::warn!(%error, "WASAPI low-latency period stream stopped unexpectedly");
    }
}

fn fill_silence(render_client: &IAudioRenderClient, frames: u32) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    unsafe {
        render_client
            .GetBuffer(frames)
            .map_err(|error| format!("IAudioRenderClient::GetBuffer: {error}"))?;
        render_client
            .ReleaseBuffer(frames, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)
            .map_err(|error| format!("IAudioRenderClient::ReleaseBuffer: {error}"))?;
    }
    Ok(())
}

fn scale_frames_ceil(
    frames: u32,
    from_sample_rate: u32,
    to_sample_rate: u32,
) -> Result<u32, String> {
    if frames == 0 || from_sample_rate == 0 || to_sample_rate == 0 {
        return Err("audio frame and sample-rate values must be non-zero".to_string());
    }
    let scaled = (frames as u64 * to_sample_rate as u64).div_ceil(from_sample_rate as u64);
    u32::try_from(scaled).map_err(|_| "converted audio period exceeds u32".to_string())
}

fn select_period_frames(
    requested: Option<u32>,
    fundamental: u32,
    min: u32,
    max: u32,
) -> Result<u32, String> {
    if fundamental == 0 || min == 0 || max == 0 || min > max {
        return Err(format!(
            "invalid WASAPI period range: fundamental={fundamental}, min={min}, max={max}"
        ));
    }

    let target = requested.unwrap_or(min).clamp(min, max);
    let aligned_up = (target as u64).div_ceil(fundamental as u64) * fundamental as u64;
    if aligned_up <= max as u64 {
        return Ok(aligned_up as u32);
    }

    let aligned_down = max / fundamental * fundamental;
    if aligned_down >= min {
        Ok(aligned_down)
    } else {
        Err(format!(
            "WASAPI period range contains no fundamental-aligned value: fundamental={fundamental}, min={min}, max={max}"
        ))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WasapiExclusiveInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: &'static str,
    pub buffer_frames: u32,
    pub default_period_100ns: i64,
    pub minimum_period_100ns: i64,
    pub period_100ns: i64,
}

#[derive(Debug, Error)]
pub(crate) enum WasapiExclusiveError {
    #[error("failed to create the WASAPI exclusive worker: {0}")]
    Spawn(#[source] io::Error),
    #[error("WASAPI exclusive worker stopped during initialization")]
    WorkerStopped,
    #[error("WASAPI exclusive initialization failed: {0}")]
    Initialization(String),
    #[error("failed to start WASAPI exclusive output: {0}")]
    Start(String),
}

pub(crate) struct WasapiExclusiveOutput {
    start_event: OwnedEvent,
    stop_event: OwnedEvent,
    start_result: Mutex<Option<mpsc::Receiver<Result<(), String>>>>,
    worker: Option<JoinHandle<()>>,
    info: WasapiExclusiveInfo,
}

impl WasapiExclusiveOutput {
    pub(crate) fn open(
        endpoint_id: String,
        requested_sample_rate: Option<u32>,
        requested_buffer_frames: Option<u32>,
        renderer: NativeOutputRenderer,
    ) -> Result<Self, WasapiExclusiveError> {
        let start_event = OwnedEvent::new().map_err(|error| {
            WasapiExclusiveError::Initialization(format!("CreateEventW(start): {error}"))
        })?;
        let stop_event = OwnedEvent::new().map_err(|error| {
            WasapiExclusiveError::Initialization(format!("CreateEventW(stop): {error}"))
        })?;
        let start_event_value = start_event.handle().0 as usize;
        let stop_event_value = stop_event.handle().0 as usize;
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let (start_result_sender, start_result_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("bmz-wasapi-exclusive".to_string())
            .spawn(move || {
                exclusive_worker_main(
                    endpoint_id,
                    requested_sample_rate,
                    requested_buffer_frames,
                    HANDLE(start_event_value as *mut c_void),
                    HANDLE(stop_event_value as *mut c_void),
                    renderer,
                    startup_sender,
                    start_result_sender,
                );
            })
            .map_err(WasapiExclusiveError::Spawn)?;

        match startup_receiver.recv() {
            Ok(Ok(info)) => Ok(Self {
                start_event,
                stop_event,
                start_result: Mutex::new(Some(start_result_receiver)),
                worker: Some(worker),
                info,
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(WasapiExclusiveError::Initialization(error))
            }
            Err(_) => {
                let _ = worker.join();
                Err(WasapiExclusiveError::WorkerStopped)
            }
        }
    }

    pub(crate) fn play(&self) -> Result<(), WasapiExclusiveError> {
        let receiver = self
            .start_result
            .lock()
            .map_err(|_| WasapiExclusiveError::Start("start result lock was poisoned".to_string()))?
            .take();
        let Some(receiver) = receiver else {
            return Ok(());
        };
        unsafe { SetEvent(self.start_event.handle()) }
            .map_err(|error| WasapiExclusiveError::Start(format!("SetEvent(start): {error}")))?;
        receiver
            .recv()
            .map_err(|_| {
                WasapiExclusiveError::Start(
                    "worker stopped before reporting the start result".to_string(),
                )
            })?
            .map_err(WasapiExclusiveError::Start)
    }

    pub(crate) fn info(&self) -> WasapiExclusiveInfo {
        self.info
    }
}

impl Drop for WasapiExclusiveOutput {
    fn drop(&mut self) {
        if let Err(error) = unsafe { SetEvent(self.stop_event.handle()) } {
            tracing::warn!(%error, "failed to stop WASAPI exclusive worker");
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            tracing::warn!("WASAPI exclusive worker panicked while stopping");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExclusiveSampleFormat {
    F32,
    I32,
    I24In32,
    I24Packed,
    I16,
}

impl ExclusiveSampleFormat {
    const fn label(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::I32 => "i32",
            Self::I24In32 => "i24-in-i32",
            Self::I24Packed => "i24-packed",
            Self::I16 => "i16",
        }
    }

    const fn container_bytes(self) -> u16 {
        match self {
            Self::F32 | Self::I32 | Self::I24In32 => 4,
            Self::I24Packed => 3,
            Self::I16 => 2,
        }
    }

    const fn valid_bits(self) -> u16 {
        match self {
            Self::F32 | Self::I32 => 32,
            Self::I24In32 | Self::I24Packed => 24,
            Self::I16 => 16,
        }
    }

    const fn is_float(self) -> bool {
        matches!(self, Self::F32)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PackedI24([u8; 3]);

impl OutputSample for PackedI24 {
    fn from_f32(value: f32) -> Self {
        let value = (value.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        let bytes = value.to_le_bytes();
        Self([bytes[0], bytes[1], bytes[2]])
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PcmI24In32(i32);

impl OutputSample for PcmI24In32 {
    fn from_f32(value: f32) -> Self {
        let value = (value.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        Self(value << 8)
    }
}

#[derive(Clone, Copy)]
struct ExclusiveFormatCandidate {
    wave: WAVEFORMATEXTENSIBLE,
    sample_format: ExclusiveSampleFormat,
}

/// One reading of the endpoint clock and the QPC counter taken beside it.
#[derive(Debug, Clone, Copy)]
struct ExclusiveClockReading {
    position: u64,
    frequency: u64,
    qpc_100ns: u64,
    counter: i64,
    qpc_frequency: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExclusiveOutputState {
    /// Output time of the next buffer relative to the counter reading.
    next_buffer_delta_ns: Option<i128>,
    missed_frames: u64,
}

/// A rejected position must keep progressing smoothly this long before it
/// replaces the accepted correspondence.
const EXCLUSIVE_POSITION_REANCHOR: Duration = Duration::from_secs(1);

/// The position is trusted while it follows the accepted QPC correspondence.
/// After a rejection it either returns to that correspondence, or a long smooth
/// run replaces it. `written_bias` then absorbs the difference, so that a new
/// position epoch restores the prediction without becoming a voice skip.
#[derive(Debug, Default, Clone, Copy)]
struct ExclusiveClockTracker {
    accepted: Option<ExclusiveClockReading>,
    /// PCM queued past the position, from the last reading that had any.
    queued_frames: Option<u64>,
    written_bias: i128,
    recovering: bool,
    recovery_candidate: Option<ExclusiveClockReading>,
    reanchor_start: Option<ExclusiveClockReading>,
}

impl ExclusiveClockTracker {
    fn observe(
        &mut self,
        reading: ExclusiveClockReading,
        written_frames: u64,
        sample_rate: u32,
        max_missed_frames: u64,
        period: Duration,
    ) -> ExclusiveOutputState {
        let unavailable = ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 0 };
        let tolerance = crate::clock::output_gap_tolerance(period).as_nanos() as i128;
        if !exclusive_reading_is_usable(reading, sample_rate, tolerance) {
            self.reject();
            self.reanchor_start = None;
            return unavailable;
        }
        if !self.follows_accepted(
            reading,
            written_frames,
            sample_rate,
            max_missed_frames,
            tolerance,
        ) {
            self.reject();
            if !self.reanchor(reading, written_frames, sample_rate, period, tolerance) {
                return unavailable;
            }
            return self.accept(reading, written_frames, sample_rate, 0);
        }
        if self.recovering {
            let Some(candidate) = self.recovery_candidate else {
                self.recovery_candidate = Some(reading);
                return unavailable;
            };
            if reading.qpc_100ns <= candidate.qpc_100ns
                || reading.counter <= candidate.counter
                || reading.position < candidate.position
            {
                return unavailable;
            }
            self.finish_recovery();
        }
        self.accept(reading, written_frames, sample_rate, max_missed_frames)
    }

    fn reject(&mut self) {
        self.recovering = true;
        self.recovery_candidate = None;
    }

    fn finish_recovery(&mut self) {
        self.recovering = false;
        self.recovery_candidate = None;
        self.reanchor_start = None;
    }

    fn biased_written(&self, written_frames: u64) -> u64 {
        (i128::from(written_frames) + self.written_bias).clamp(0, i128::from(u64::MAX)) as u64
    }

    fn accept(
        &mut self,
        reading: ExclusiveClockReading,
        written_frames: u64,
        sample_rate: u32,
        max_missed_frames: u64,
    ) -> ExclusiveOutputState {
        let written = self.biased_written(written_frames);
        let queued = written.saturating_sub(exclusive_consumed_frames(reading, sample_rate));
        if queued != 0 {
            self.queued_frames = Some(queued);
        }
        self.accepted = Some(reading);
        exclusive_output_state(reading, written, sample_rate, max_missed_frames)
    }

    fn follows_accepted(
        &self,
        reading: ExclusiveClockReading,
        written_frames: u64,
        sample_rate: u32,
        max_missed_frames: u64,
        tolerance: i128,
    ) -> bool {
        let consumed = u128::from(exclusive_consumed_frames(reading, sample_rate));
        let written = u128::from(self.biased_written(written_frames));
        // Do not clip a huge untrusted deficit into many small voice skips.
        // Two frames allow integer conversion/position quantization at the edge.
        consumed <= written + u128::from(max_missed_frames) + 2
            && self
                .accepted
                .is_none_or(|previous| exclusive_progress_matches(previous, reading, tolerance))
    }

    /// Keeps the last queued PCM depth, so that a later real underrun is still
    /// measured from the same margin.
    fn reanchor(
        &mut self,
        reading: ExclusiveClockReading,
        written_frames: u64,
        sample_rate: u32,
        period: Duration,
        tolerance: i128,
    ) -> bool {
        let Some(start) = self
            .reanchor_start
            .filter(|start| exclusive_progress_matches(*start, reading, tolerance))
        else {
            self.reanchor_start = Some(reading);
            return false;
        };
        let elapsed = Duration::from_nanos((reading.qpc_100ns - start.qpc_100ns) * 100);
        if elapsed < EXCLUSIVE_POSITION_REANCHOR {
            return false;
        }
        let queued = self.queued_frames.unwrap_or(duration_to_frames(period, sample_rate));
        self.written_bias = i128::from(exclusive_consumed_frames(reading, sample_rate))
            + i128::from(queued)
            - i128::from(written_frames);
        self.finish_recovery();
        true
    }
}

fn exclusive_reading_is_usable(
    reading: ExclusiveClockReading,
    sample_rate: u32,
    tolerance: i128,
) -> bool {
    if reading.frequency == 0
        || reading.qpc_frequency == 0
        || sample_rate == 0
        || reading.counter < 0
    {
        return false;
    }
    let counter_ns =
        i128::from(reading.counter) * 1_000_000_000 / i128::from(reading.qpc_frequency);
    let age_ns = counter_ns - i128::from(reading.qpc_100ns) * 100;
    // GetPosition runs after QueryPerformanceCounter; a small future value
    // can be its acquisition cost. Old/future epochs are not output gaps.
    (-tolerance..=tolerance).contains(&age_ns)
}

fn exclusive_progress_matches(
    previous: ExclusiveClockReading,
    reading: ExclusiveClockReading,
    tolerance: i128,
) -> bool {
    if reading.frequency != previous.frequency
        || reading.qpc_frequency != previous.qpc_frequency
        || reading.position < previous.position
        || reading.qpc_100ns <= previous.qpc_100ns
        || reading.counter <= previous.counter
    {
        return false;
    }
    let elapsed_ns = i128::from(reading.qpc_100ns - previous.qpc_100ns) * 100;
    let advance_ns = i128::from(reading.position - previous.position) * 1_000_000_000
        / i128::from(reading.frequency);
    // Allow ordinary device drift across missing observations. A larger shift
    // is not this correspondence, and can only replace it through a re-anchor.
    let drift_slack = (elapsed_ns / 1_000).min(20_000_000);
    (advance_ns - elapsed_ns).abs() <= tolerance + drift_slack
}

fn exclusive_consumed_frames(reading: ExclusiveClockReading, sample_rate: u32) -> u64 {
    (u128::from(reading.position) * u128::from(sample_rate) / u128::from(reading.frequency))
        .min(u128::from(u64::MAX)) as u64
}

/// `max_missed_frames` is how far the callback ran past its period. A buffer is
/// queued ahead of every callback, so PCM cannot run out by more than that. A
/// larger position is a clock glitch, which must not skip the playing voices.
fn exclusive_output_state(
    reading: ExclusiveClockReading,
    written_frames: u64,
    sample_rate: u32,
    max_missed_frames: u64,
) -> ExclusiveOutputState {
    let counter_ns =
        i128::from(reading.counter) * 1_000_000_000 / i128::from(reading.qpc_frequency);
    let position_ns = i128::from(reading.position) * 1_000_000_000 / i128::from(reading.frequency);
    // An underrun can consume silence beyond the last submitted PCM. Do not
    // retain that lost interval as a permanent scheduling offset.
    let consumed = exclusive_consumed_frames(reading, sample_rate);
    let missed_frames = consumed.saturating_sub(written_frames).min(max_missed_frames);
    let queued_ns = i128::from(written_frames.saturating_add(missed_frames)) * 1_000_000_000
        / i128::from(sample_rate);
    let delta_ns = i128::from(reading.qpc_100ns) * 100 - counter_ns + queued_ns - position_ns;
    ExclusiveOutputState {
        next_buffer_delta_ns: (-100_000_000..1_000_000_000).contains(&delta_ns).then_some(delta_ns),
        missed_frames,
    }
}

struct ExclusiveWorkerStream {
    client: IAudioClient,
    render_client: IAudioRenderClient,
    clock: Option<(IAudioClock, u64)>,
    qpc_frequency: u64,
    sample_rate: u32,
    submitted_frames: Cell<u64>,
    recovered_frames: Cell<u64>,
    clock_tracker: Cell<ExclusiveClockTracker>,
    previous_fill: Cell<Option<Instant>>,
    audio_event: OwnedEvent,
    buffer_frames: u32,
    channels: usize,
    sample_format: ExclusiveSampleFormat,
    started: bool,
}

impl ExclusiveWorkerStream {
    fn open(
        endpoint_id: &str,
        requested_sample_rate: Option<u32>,
        requested_buffer_frames: Option<u32>,
    ) -> Result<(Self, WasapiExclusiveInfo), String> {
        let endpoint = endpoint_from_id(endpoint_id)?;
        let initial_client = activate_audio_client(&endpoint)?;
        let mix_format = TaskMemFormat(unsafe {
            initial_client
                .GetMixFormat()
                .map_err(|error| format!("IAudioClient::GetMixFormat: {error}"))?
        });
        let candidate =
            choose_exclusive_format(&initial_client, mix_format.as_ptr(), requested_sample_rate)?;
        let sample_rate = candidate.wave.Format.nSamplesPerSec;
        let channels = candidate.wave.Format.nChannels;

        let mut default_period_100ns = 0;
        let mut minimum_period_100ns = 0;
        unsafe {
            initial_client
                .GetDevicePeriod(Some(&mut default_period_100ns), Some(&mut minimum_period_100ns))
                .map_err(|error| format!("IAudioClient::GetDevicePeriod: {error}"))?;
        }
        if default_period_100ns <= 0 || minimum_period_100ns <= 0 {
            return Err(format!(
                "WASAPI reported invalid device periods: default={default_period_100ns}, minimum={minimum_period_100ns}"
            ));
        }
        let requested_period = requested_buffer_frames
            .map(|frames| frames_to_period_100ns(frames, sample_rate))
            .transpose()?
            .unwrap_or(default_period_100ns);
        // Match stock BMZ: prefer the user's Fixed period (clamped only to the device
        // minimum) so key-to-sound latency stays as low as the setting allows. Default /
        // 2×default are Initialize fallbacks only, never preferred over a successful low
        // period.
        let preferred_periods = exclusive_period_candidates(
            requested_period,
            minimum_period_100ns,
            default_period_100ns,
        );

        let (client, buffer_frames, period_100ns) = initialize_exclusive_client_with_fallbacks(
            &endpoint,
            initial_client,
            &candidate.wave.Format,
            &preferred_periods,
            sample_rate,
        )?;
        let audio_event =
            OwnedEvent::new().map_err(|error| format!("CreateEventW(audio): {error}"))?;
        unsafe {
            client
                .SetEventHandle(audio_event.handle())
                .map_err(|error| format!("IAudioClient::SetEventHandle: {error}"))?;
        }
        let render_client: IAudioRenderClient = unsafe {
            client
                .GetService()
                .map_err(|error| format!("IAudioClient::GetService<IAudioRenderClient>: {error}"))?
        };

        let info = WasapiExclusiveInfo {
            sample_rate,
            channels,
            sample_format: candidate.sample_format.label(),
            buffer_frames,
            default_period_100ns,
            minimum_period_100ns,
            period_100ns,
        };
        // Position uses the device's clock frequency; QPC is returned in 100 ns
        // units. Neither value has Instant's epoch.
        let clock = unsafe { client.GetService::<IAudioClock>() }.ok().and_then(|clock| {
            let frequency = unsafe { clock.GetFrequency() }.ok()?;
            (frequency != 0).then_some((clock, frequency))
        });
        let mut qpc_frequency = 0;
        let _ = unsafe { QueryPerformanceFrequency(&mut qpc_frequency) };
        Ok((
            Self {
                client,
                render_client,
                clock,
                qpc_frequency: qpc_frequency.max(0) as u64,
                sample_rate,
                submitted_frames: Cell::new(0),
                recovered_frames: Cell::new(0),
                clock_tracker: Cell::new(ExclusiveClockTracker::default()),
                previous_fill: Cell::new(None),
                audio_event,
                buffer_frames,
                channels: channels as usize,
                sample_format: candidate.sample_format,
                started: false,
            },
            info,
        ))
    }

    fn wait_for_start(start_event: HANDLE, stop_event: HANDLE) -> Result<bool, String> {
        let wait = unsafe { WaitForMultipleObjects(&[stop_event, start_event], false, INFINITE) };
        if wait == WAIT_OBJECT_0 {
            return Ok(false);
        }
        if wait == WAIT_FAILED {
            return Err(format!(
                "WaitForMultipleObjects(start): {}",
                windows::core::Error::from_thread()
            ));
        }
        if wait.0 != WAIT_OBJECT_0.0 + 1 {
            return Err(format!(
                "WaitForMultipleObjects(start) returned unexpected value {}",
                wait.0
            ));
        }
        Ok(true)
    }

    fn start(&mut self, renderer: &mut NativeOutputRenderer) -> Result<(), String> {
        self.fill_next_buffer(renderer)?;
        unsafe {
            self.client.Start().map_err(|error| format!("IAudioClient::Start: {error}"))?;
        }
        self.started = true;
        Ok(())
    }

    fn render_until_stopped(
        &self,
        stop_event: HANDLE,
        renderer: &mut NativeOutputRenderer,
    ) -> Result<(), String> {
        loop {
            let wait = unsafe {
                WaitForMultipleObjects(&[stop_event, self.audio_event.handle()], false, INFINITE)
            };
            if wait == WAIT_OBJECT_0 {
                return Ok(());
            }
            if wait == WAIT_FAILED {
                return Err(format!(
                    "WaitForMultipleObjects(audio): {}",
                    windows::core::Error::from_thread()
                ));
            }
            if wait.0 != WAIT_OBJECT_0.0 + 1 {
                return Err(format!(
                    "WaitForMultipleObjects(audio) returned unexpected value {}",
                    wait.0
                ));
            }
            self.fill_next_buffer(renderer)?;
        }
    }

    fn next_playback_time(&self, max_missed_frames: u64) -> (Option<Instant>, u64) {
        let before = Instant::now();
        let Some((clock, frequency)) = &self.clock else {
            return (None, 0);
        };
        if !self.started || self.qpc_frequency == 0 {
            return (None, 0);
        }
        let mut counter = 0;
        if unsafe { QueryPerformanceCounter(&mut counter) }.is_err() {
            return (None, 0);
        }
        let after = Instant::now();
        let paired = before + after.duration_since(before) / 2;
        let mut position = 0;
        let mut qpc_100ns = 0;
        if unsafe { clock.GetPosition(&mut position, Some(&mut qpc_100ns)) }.is_err() {
            return (None, 0);
        }
        let reading = ExclusiveClockReading {
            position,
            frequency: *frequency,
            qpc_100ns,
            counter,
            qpc_frequency: self.qpc_frequency,
        };
        let written_frames =
            self.submitted_frames.get().saturating_add(self.recovered_frames.get());
        let mut tracker = self.clock_tracker.get();
        let state = tracker.observe(
            reading,
            written_frames,
            self.sample_rate,
            max_missed_frames,
            frames_to_duration(self.buffer_frames as usize, self.sample_rate),
        );
        self.clock_tracker.set(tracker);
        self.recovered_frames.set(self.recovered_frames.get().saturating_add(state.missed_frames));
        let prediction = state.next_buffer_delta_ns.and_then(|delta_ns| {
            let distance = Duration::from_nanos(delta_ns.unsigned_abs() as u64);
            if delta_ns < 0 { paired.checked_sub(distance) } else { paired.checked_add(distance) }
        });
        (prediction, state.missed_frames)
    }

    fn fill_next_buffer(&self, renderer: &mut NativeOutputRenderer) -> Result<(), String> {
        let callback_start = Instant::now();
        let data = unsafe {
            self.render_client
                .GetBuffer(self.buffer_frames)
                .map_err(|error| format!("IAudioRenderClient::GetBuffer: {error}"))?
        };
        let sample_count = self.buffer_frames as usize * self.channels;
        let period = frames_to_duration(self.buffer_frames as usize, self.sample_rate);
        let late =
            self.previous_fill.replace(Some(callback_start)).map_or(Duration::ZERO, |previous| {
                callback_start.saturating_duration_since(previous).saturating_sub(period)
            });
        let (playback, missed_frames) =
            self.next_playback_time(duration_to_frames(late, self.sample_rate));
        if self.clock_tracker.get().recovering {
            renderer.record_untrusted_position();
        }
        if missed_frames != 0 {
            // Publish the same underrun to both active voices and timed events,
            // exactly once. Prediction changes alone never advance this frame.
            renderer.catch_up_output_frames(missed_frames);
        }
        unsafe {
            match self.sample_format {
                ExclusiveSampleFormat::F32 => renderer.render_at(
                    slice::from_raw_parts_mut(data.cast::<f32>(), sample_count),
                    self.channels,
                    playback,
                    callback_start,
                    self.sample_rate,
                ),
                ExclusiveSampleFormat::I32 => renderer.render_at(
                    slice::from_raw_parts_mut(data.cast::<i32>(), sample_count),
                    self.channels,
                    playback,
                    callback_start,
                    self.sample_rate,
                ),
                ExclusiveSampleFormat::I24In32 => renderer.render_at(
                    slice::from_raw_parts_mut(data.cast::<PcmI24In32>(), sample_count),
                    self.channels,
                    playback,
                    callback_start,
                    self.sample_rate,
                ),
                ExclusiveSampleFormat::I24Packed => renderer.render_at(
                    slice::from_raw_parts_mut(data.cast::<PackedI24>(), sample_count),
                    self.channels,
                    playback,
                    callback_start,
                    self.sample_rate,
                ),
                ExclusiveSampleFormat::I16 => renderer.render_at(
                    slice::from_raw_parts_mut(data.cast::<i16>(), sample_count),
                    self.channels,
                    playback,
                    callback_start,
                    self.sample_rate,
                ),
            }
            self.render_client
                .ReleaseBuffer(self.buffer_frames, 0)
                .map_err(|error| format!("IAudioRenderClient::ReleaseBuffer: {error}"))?;
        }
        self.submitted_frames
            .set(self.submitted_frames.get().saturating_add(u64::from(self.buffer_frames)));
        Ok(())
    }
}

impl Drop for ExclusiveWorkerStream {
    fn drop(&mut self) {
        if self.started
            && let Err(error) = unsafe { self.client.Stop() }
        {
            tracing::warn!(%error, "failed to stop WASAPI exclusive stream");
        }
    }
}

struct MmcssRegistration(Option<HANDLE>);

impl MmcssRegistration {
    fn pro_audio() -> Self {
        let task_name = "Pro Audio\0".encode_utf16().collect::<Vec<_>>();
        let mut task_index = 0;
        match unsafe { AvSetMmThreadCharacteristicsW(PCWSTR(task_name.as_ptr()), &mut task_index) }
        {
            Ok(handle) => Self(Some(handle)),
            Err(error) => {
                tracing::warn!(%error, "failed to register WASAPI exclusive worker with MMCSS");
                Self(None)
            }
        }
    }
}

impl Drop for MmcssRegistration {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take()
            && let Err(error) = unsafe { AvRevertMmThreadCharacteristics(handle) }
        {
            tracing::warn!(%error, "failed to revert WASAPI exclusive MMCSS registration");
        }
    }
}

fn exclusive_worker_main(
    endpoint_id: String,
    requested_sample_rate: Option<u32>,
    requested_buffer_frames: Option<u32>,
    start_event: HANDLE,
    stop_event: HANDLE,
    mut renderer: NativeOutputRenderer,
    startup_sender: mpsc::SyncSender<Result<WasapiExclusiveInfo, String>>,
    start_result_sender: mpsc::SyncSender<Result<(), String>>,
) {
    let apartment = match ComApartment::initialize() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = startup_sender.send(Err(error));
            return;
        }
    };
    let _apartment = apartment;
    let (mut stream, initial_info) = match ExclusiveWorkerStream::open(
        &endpoint_id,
        requested_sample_rate,
        requested_buffer_frames,
    ) {
        Ok(value) => value,
        Err(error) => {
            let _ = startup_sender.send(Err(error));
            return;
        }
    };
    if startup_sender.send(Ok(initial_info)).is_err() {
        return;
    }

    let should_start = match ExclusiveWorkerStream::wait_for_start(start_event, stop_event) {
        Ok(should_start) => should_start,
        Err(error) => {
            let _ = start_result_sender.send(Err(error));
            return;
        }
    };
    if !should_start {
        let _ = start_result_sender
            .send(Err("WASAPI exclusive output stopped before it was started".to_string()));
        return;
    }

    let _mmcss = MmcssRegistration::pro_audio();
    if let Err(error) = stream.start(&mut renderer) {
        let _ = start_result_sender.send(Err(error));
        return;
    }
    if start_result_sender.send(Ok(())).is_err() {
        return;
    }

    loop {
        let error = match stream.render_until_stopped(stop_event, &mut renderer) {
            Ok(()) => return,
            Err(error) => error,
        };
        renderer.record_stream_error();
        tracing::warn!(%error, "WASAPI exclusive stream stopped unexpectedly; reopening endpoint");
        drop(stream);

        let outage_started_at = Instant::now();
        let mut last_catch_up_at = outage_started_at;
        let mut retry_delay_ms = EXCLUSIVE_RECOVERY_INITIAL_DELAY_MS;
        loop {
            match wait_for_exclusive_recovery(stop_event, retry_delay_ms) {
                Ok(false) => return,
                Ok(true) => {}
                Err(error) => {
                    renderer.record_stream_error();
                    tracing::warn!(%error, "failed while waiting to reopen WASAPI exclusive output");
                    return;
                }
            }

            match ExclusiveWorkerStream::open(
                &endpoint_id,
                Some(initial_info.sample_rate),
                requested_buffer_frames,
            ) {
                Ok((mut recovered, recovered_info))
                    if recovered_info.sample_rate == initial_info.sample_rate =>
                {
                    let catch_up_at = Instant::now();
                    let catch_up_frames = renderer.catch_up_after_stream_outage(
                        catch_up_at.duration_since(last_catch_up_at),
                        initial_info.sample_rate,
                    );
                    last_catch_up_at = catch_up_at;
                    match recovered.start(&mut renderer) {
                        Ok(()) => {
                            tracing::info!(
                                outage_ms = outage_started_at.elapsed().as_millis(),
                                catch_up_frames,
                                "reopened WASAPI exclusive output",
                            );
                            stream = recovered;
                            break;
                        }
                        Err(error) => {
                            renderer.record_stream_error();
                            tracing::warn!(
                                %error,
                                "failed to restart reopened WASAPI exclusive output",
                            );
                        }
                    }
                }
                Ok((_recovered, recovered_info)) => {
                    renderer.record_stream_error();
                    tracing::warn!(
                        expected_sample_rate = initial_info.sample_rate,
                        recovered_sample_rate = recovered_info.sample_rate,
                        "reopened WASAPI endpoint changed sample rate; retrying",
                    );
                }
                Err(error) => {
                    renderer.record_stream_error();
                    tracing::warn!(
                        %error,
                        retry_delay_ms,
                        "failed to reopen WASAPI exclusive output",
                    );
                }
            }
            retry_delay_ms = next_exclusive_recovery_delay_ms(retry_delay_ms);
        }
    }
}

fn wait_for_exclusive_recovery(stop_event: HANDLE, timeout_ms: u32) -> Result<bool, String> {
    let wait = unsafe { WaitForMultipleObjects(&[stop_event], false, timeout_ms) };
    if wait == WAIT_OBJECT_0 {
        return Ok(false);
    }
    if wait == WAIT_TIMEOUT {
        return Ok(true);
    }
    if wait == WAIT_FAILED {
        return Err(format!(
            "WaitForMultipleObjects(recovery): {}",
            windows::core::Error::from_thread()
        ));
    }
    Err(format!("WaitForMultipleObjects(recovery) returned unexpected value {}", wait.0))
}

fn next_exclusive_recovery_delay_ms(current: u32) -> u32 {
    current.saturating_mul(2).min(EXCLUSIVE_RECOVERY_MAX_DELAY_MS)
}

fn endpoint_from_id(endpoint_id: &str) -> Result<IMMDevice, String> {
    let enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|error| format!("CoCreateInstance(MMDeviceEnumerator): {error}"))?
    };
    let endpoint_id_wide = endpoint_id.encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>();
    unsafe {
        enumerator
            .GetDevice(PCWSTR(endpoint_id_wide.as_ptr()))
            .map_err(|error| format!("IMMDeviceEnumerator::GetDevice: {error}"))
    }
}

fn activate_audio_client(endpoint: &IMMDevice) -> Result<IAudioClient, String> {
    unsafe {
        endpoint
            .Activate(CLSCTX_ALL, None)
            .map_err(|error| format!("IMMDevice::Activate<IAudioClient>: {error}"))
    }
}

fn initialize_exclusive_client(
    endpoint: &IMMDevice,
    initial_client: IAudioClient,
    format: &WAVEFORMATEX,
    requested_period_100ns: i64,
    sample_rate: u32,
) -> Result<(IAudioClient, u32, i64), String> {
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_NOPERSIST;
    let first = unsafe {
        initial_client.Initialize(
            AUDCLNT_SHAREMODE_EXCLUSIVE,
            flags,
            requested_period_100ns,
            requested_period_100ns,
            format,
            None,
        )
    };
    match first {
        Ok(()) => {
            let buffer_frames = unsafe {
                initial_client
                    .GetBufferSize()
                    .map_err(|error| format!("IAudioClient::GetBufferSize: {error}"))?
            };
            Ok((initial_client, buffer_frames, requested_period_100ns))
        }
        Err(error) if error.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
            let aligned_frames = unsafe {
                initial_client.GetBufferSize().map_err(|buffer_error| {
                    format!(
                        "IAudioClient::Initialize returned BUFFER_SIZE_NOT_ALIGNED, then GetBufferSize failed: {buffer_error}"
                    )
                })?
            };
            let aligned_period_100ns = frames_to_period_100ns(aligned_frames, sample_rate)?;
            drop(initial_client);
            let client = activate_audio_client(endpoint)?;
            unsafe {
                client
                    .Initialize(
                        AUDCLNT_SHAREMODE_EXCLUSIVE,
                        flags,
                        aligned_period_100ns,
                        aligned_period_100ns,
                        format,
                        None,
                    )
                    .map_err(|retry_error| {
                        format!("IAudioClient::Initialize(aligned {aligned_frames} frames): {retry_error}")
                    })?;
            }
            let buffer_frames = unsafe {
                client
                    .GetBufferSize()
                    .map_err(|error| format!("IAudioClient::GetBufferSize(aligned): {error}"))?
            };
            Ok((client, buffer_frames, aligned_period_100ns))
        }
        Err(error) => Err(format!("IAudioClient::Initialize(exclusive): {error}")),
    }
}

fn choose_exclusive_format(
    client: &IAudioClient,
    mix_format: *const WAVEFORMATEX,
    requested_sample_rate: Option<u32>,
) -> Result<ExclusiveFormatCandidate, String> {
    if mix_format.is_null() {
        return Err("IAudioClient::GetMixFormat returned a null format".to_string());
    }
    let mix = unsafe { *mix_format };
    let mix_sample_rate = mix.nSamplesPerSec;
    let mix_channels = mix.nChannels;
    if mix_sample_rate == 0 || mix_channels == 0 {
        return Err("WASAPI mix format contains a zero sample rate or channel count".to_string());
    }
    let (mix_sample_format, mix_channel_mask) = parse_mix_format(mix_format);

    let mut rates = Vec::new();
    push_unique(&mut rates, requested_sample_rate.unwrap_or(mix_sample_rate));
    push_unique(&mut rates, mix_sample_rate);
    for rate in [48_000, 44_100, 96_000, 192_000] {
        push_unique(&mut rates, rate);
    }

    let mut channels = Vec::new();
    channels.push((mix_channels, mix_channel_mask));
    if mix_channels != 2 {
        channels.push((2, SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT));
    }

    let mut sample_formats = Vec::new();
    if let Some(format) = mix_sample_format {
        sample_formats.push(format);
    }
    for format in [
        ExclusiveSampleFormat::F32,
        ExclusiveSampleFormat::I24In32,
        ExclusiveSampleFormat::I32,
        ExclusiveSampleFormat::I24Packed,
        ExclusiveSampleFormat::I16,
    ] {
        if !sample_formats.contains(&format) {
            sample_formats.push(format);
        }
    }

    for rate in rates {
        for &(channel_count, channel_mask) in &channels {
            for &sample_format in &sample_formats {
                let supports_legacy_container =
                    channel_count <= 2 && sample_format != ExclusiveSampleFormat::I24In32;
                let representation_count = if supports_legacy_container { 2 } else { 1 };
                for representation in 0..representation_count {
                    let force_extensible = !supports_legacy_container || representation == 1;
                    let candidate = make_wave_format(
                        channel_count,
                        channel_mask,
                        rate,
                        sample_format,
                        force_extensible,
                    )?;
                    let result = unsafe {
                        client.IsFormatSupported(
                            AUDCLNT_SHAREMODE_EXCLUSIVE,
                            &candidate.wave.Format,
                            None,
                        )
                    };
                    if result.0 == 0 {
                        return Ok(candidate);
                    }
                }
            }
        }
    }
    Err(format!(
        "endpoint supports none of the requested exclusive formats (requested_rate={requested_sample_rate:?}, mix_rate={}, mix_channels={})",
        mix_sample_rate, mix_channels
    ))
}

fn parse_mix_format(format: *const WAVEFORMATEX) -> (Option<ExclusiveSampleFormat>, u32) {
    let base = unsafe { *format };
    let mut channel_mask = default_channel_mask(base.nChannels);
    let mut valid_bits = base.wBitsPerSample;
    let mut subtype = None;
    if u32::from(base.wFormatTag) == WAVE_FORMAT_EXTENSIBLE
        && usize::from(base.cbSize) + size_of::<WAVEFORMATEX>() >= size_of::<WAVEFORMATEXTENSIBLE>()
    {
        let extended = unsafe { *format.cast::<WAVEFORMATEXTENSIBLE>() };
        channel_mask = extended.dwChannelMask;
        valid_bits = unsafe { extended.Samples.wValidBitsPerSample };
        subtype = Some(extended.SubFormat);
    }

    let format = if subtype == Some(KSDATAFORMAT_SUBTYPE_IEEE_FLOAT)
        || u32::from(base.wFormatTag) == WAVE_FORMAT_IEEE_FLOAT
    {
        (base.wBitsPerSample == 32).then_some(ExclusiveSampleFormat::F32)
    } else if subtype == Some(KSDATAFORMAT_SUBTYPE_PCM)
        || u32::from(base.wFormatTag) == WAVE_FORMAT_PCM
    {
        match (base.wBitsPerSample, valid_bits) {
            (16, _) => Some(ExclusiveSampleFormat::I16),
            (24, _) => Some(ExclusiveSampleFormat::I24Packed),
            (32, 24) => Some(ExclusiveSampleFormat::I24In32),
            (32, _) => Some(ExclusiveSampleFormat::I32),
            _ => None,
        }
    } else {
        None
    };
    (format, channel_mask)
}

fn make_wave_format(
    channels: u16,
    channel_mask: u32,
    sample_rate: u32,
    sample_format: ExclusiveSampleFormat,
    force_extensible: bool,
) -> Result<ExclusiveFormatCandidate, String> {
    if channels == 0 || sample_rate == 0 {
        return Err("exclusive format channels and sample rate must be non-zero".to_string());
    }
    let sample_bytes = sample_format.container_bytes();
    let block_align = channels
        .checked_mul(sample_bytes)
        .ok_or_else(|| "exclusive format block alignment overflow".to_string())?;
    let avg_bytes_per_sec = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| "exclusive format byte rate overflow".to_string())?;
    let use_legacy =
        !force_extensible && channels <= 2 && sample_format != ExclusiveSampleFormat::I24In32;
    let format_tag = if use_legacy {
        if sample_format.is_float() { WAVE_FORMAT_IEEE_FLOAT } else { WAVE_FORMAT_PCM }
    } else {
        WAVE_FORMAT_EXTENSIBLE
    };
    let extension_size = (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16;
    Ok(ExclusiveFormatCandidate {
        wave: WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: format_tag as u16,
                nChannels: channels,
                nSamplesPerSec: sample_rate,
                nAvgBytesPerSec: avg_bytes_per_sec,
                nBlockAlign: block_align,
                wBitsPerSample: sample_bytes * 8,
                cbSize: if format_tag == WAVE_FORMAT_EXTENSIBLE { extension_size } else { 0 },
            },
            Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: sample_format.valid_bits() },
            dwChannelMask: channel_mask,
            SubFormat: if sample_format.is_float() {
                KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
            } else {
                KSDATAFORMAT_SUBTYPE_PCM
            },
        },
        sample_format,
    })
}

fn default_channel_mask(channels: u16) -> u32 {
    match channels {
        1 => SPEAKER_FRONT_CENTER,
        2 => SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
        _ => KSAUDIO_SPEAKER_DIRECTOUT,
    }
}

fn push_unique(values: &mut Vec<u32>, value: u32) {
    if value != 0 && !values.contains(&value) {
        values.push(value);
    }
}

fn frames_to_period_100ns(frames: u32, sample_rate: u32) -> Result<i64, String> {
    if frames == 0 || sample_rate == 0 {
        return Err("audio frame and sample-rate values must be non-zero".to_string());
    }
    let numerator = u128::from(frames) * 10_000_000;
    let period = (numerator + u128::from(sample_rate) / 2) / u128::from(sample_rate);
    i64::try_from(period).map_err(|_| "WASAPI period exceeds i64".to_string())
}

/// Exclusive event-driven period candidates, preferred first.
///
/// Same priority as stock BMZ Player: use the requested Fixed period clamped to the
/// device minimum. Larger periods are only tried if Initialize rejects the low one.
fn exclusive_period_candidates(requested: i64, minimum: i64, default: i64) -> Vec<i64> {
    let minimum = minimum.max(1);
    let default = default.max(minimum);
    let requested = requested.max(minimum);
    let double_default = default.saturating_mul(2);

    let mut periods = Vec::with_capacity(3);
    let push_unique = |periods: &mut Vec<i64>, value: i64| {
        if !periods.contains(&value) {
            periods.push(value);
        }
    };

    push_unique(&mut periods, requested);
    push_unique(&mut periods, default);
    if double_default > default {
        push_unique(&mut periods, double_default);
    }
    periods
}

fn initialize_exclusive_client_with_fallbacks(
    endpoint: &IMMDevice,
    initial_client: IAudioClient,
    format: &WAVEFORMATEX,
    periods_100ns: &[i64],
    sample_rate: u32,
) -> Result<(IAudioClient, u32, i64), String> {
    let mut client = initial_client;
    // Keep every rejected period: the last HRESULT alone does not show which
    // candidates were tried or why the preferred low period failed.
    let mut failed_attempts = Vec::new();
    for (index, &period_100ns) in periods_100ns.iter().enumerate() {
        match initialize_exclusive_client(endpoint, client, format, period_100ns, sample_rate) {
            Ok(value) => {
                if !failed_attempts.is_empty() {
                    tracing::warn!(
                        selected = %describe_exclusive_period(value.2, sample_rate),
                        failed = %failed_attempts.join("; "),
                        "WASAPI exclusive fell back to a larger period"
                    );
                }
                return Ok(value);
            }
            Err(error) => {
                failed_attempts.push(format!(
                    "{}: {error}",
                    describe_exclusive_period(period_100ns, sample_rate)
                ));
                if index + 1 >= periods_100ns.len() {
                    break;
                }
                // Initialize consumes the client on failure paths that don't
                // return it; open a fresh one for the next candidate period.
                client = match activate_audio_client(endpoint) {
                    Ok(client) => client,
                    Err(error) => {
                        failed_attempts.push(error);
                        break;
                    }
                };
            }
        }
    }
    Err(exclusive_initialize_failure(&failed_attempts))
}

/// Period with its approximate millisecond and frame sizes for diagnostics.
fn describe_exclusive_period(period_100ns: i64, sample_rate: u32) -> String {
    let frames = (i128::from(period_100ns) * i128::from(sample_rate) + 5_000_000) / 10_000_000;
    format!(
        "period {period_100ns} x100ns ({:.2} ms, ~{frames} frames @ {sample_rate} Hz)",
        period_100ns as f64 / 10_000.0
    )
}

fn exclusive_initialize_failure(failed_attempts: &[String]) -> String {
    if failed_attempts.is_empty() {
        return "WASAPI exclusive initialize failed: no period candidates".to_string();
    }
    format!(
        "WASAPI exclusive initialize failed for {} attempt(s): {}",
        failed_attempts.len(),
        failed_attempts.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_period_uses_minimum_supported_period() {
        assert_eq!(select_period_frames(None, 16, 48, 480).unwrap(), 48);
    }

    #[test]
    fn fixed_period_is_clamped_and_aligned_up() {
        assert_eq!(select_period_frames(Some(64), 16, 48, 480).unwrap(), 64);
        assert_eq!(select_period_frames(Some(65), 16, 48, 480).unwrap(), 80);
        assert_eq!(select_period_frames(Some(8), 16, 48, 480).unwrap(), 48);
        assert_eq!(select_period_frames(Some(900), 16, 48, 480).unwrap(), 480);
    }

    #[test]
    fn invalid_period_capabilities_are_rejected() {
        assert!(select_period_frames(None, 0, 48, 480).is_err());
        assert!(select_period_frames(None, 16, 480, 48).is_err());
    }

    #[test]
    fn period_frames_are_scaled_with_ceiling() {
        assert_eq!(scale_frames_ceil(64, 48_000, 44_100).unwrap(), 59);
        assert_eq!(scale_frames_ceil(59, 44_100, 48_000).unwrap(), 65);
    }

    #[test]
    fn exclusive_period_converts_frames_to_nearest_100ns_unit() {
        assert_eq!(frames_to_period_100ns(48, 48_000).unwrap(), 10_000);
        assert_eq!(frames_to_period_100ns(64, 44_100).unwrap(), 14_512);
        assert!(frames_to_period_100ns(0, 48_000).is_err());
    }

    #[test]
    fn exclusive_period_prefers_requested_then_falls_back() {
        let minimum = 30_000; // ~3ms
        let default = 100_000; // ~10ms
        // Below device minimum: clamp to minimum first (stock behavior).
        assert_eq!(
            exclusive_period_candidates(6_667, minimum, default),
            vec![30_000, 100_000, 200_000]
        );
        assert_eq!(
            exclusive_period_candidates(20_000, minimum, default),
            vec![30_000, 100_000, 200_000]
        );
        // Fixed 256 @ 44.1k ≈ 5.8 ms: keep request first.
        assert_eq!(
            exclusive_period_candidates(58_050, minimum, default),
            vec![58_050, 100_000, 200_000]
        );
        assert_eq!(
            exclusive_period_candidates(80_000, minimum, default),
            vec![80_000, 100_000, 200_000]
        );
        // When default equals minimum, still offer 2× as last resort.
        assert_eq!(
            exclusive_period_candidates(1_000, minimum, minimum),
            vec![minimum, minimum.saturating_mul(2)]
        );
    }

    #[test]
    fn exclusive_initialize_failure_lists_every_period_attempt() {
        // Fixed 256 @ 44.1 kHz.
        assert_eq!(
            describe_exclusive_period(58_050, 44_100),
            "period 58050 x100ns (5.80 ms, ~256 frames @ 44100 Hz)"
        );
        let attempts = [
            format!("{}: E_FIRST", describe_exclusive_period(58_050, 44_100)),
            format!("{}: E_SECOND", describe_exclusive_period(100_000, 44_100)),
            "IMMDevice::Activate<IAudioClient>: E_ACTIVATE".to_string(),
        ];
        let message = exclusive_initialize_failure(&attempts);
        assert!(message.starts_with("WASAPI exclusive initialize failed for 3 attempt(s): "));
        for expected in [
            "period 58050 x100ns (5.80 ms, ~256 frames @ 44100 Hz): E_FIRST",
            "period 100000 x100ns (10.00 ms, ~441 frames @ 44100 Hz): E_SECOND",
            "E_ACTIVATE",
        ] {
            assert!(message.contains(expected), "{message}");
        }
        assert!(exclusive_initialize_failure(&[]).contains("no period candidates"));
    }

    #[test]
    fn exclusive_recovery_delay_uses_bounded_exponential_backoff() {
        assert_eq!(next_exclusive_recovery_delay_ms(100), 200);
        assert_eq!(next_exclusive_recovery_delay_ms(3_200), 5_000);
        assert_eq!(next_exclusive_recovery_delay_ms(5_000), 5_000);
    }

    #[test]
    fn exclusive_wave_format_describes_stereo_float() {
        let candidate = make_wave_format(
            2,
            SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            48_000,
            ExclusiveSampleFormat::F32,
            false,
        )
        .unwrap();
        let format = candidate.wave.Format;
        let format_tag = format.wFormatTag;
        let channels = format.nChannels;
        let sample_rate = format.nSamplesPerSec;
        let block_align = format.nBlockAlign;
        let bits_per_sample = format.wBitsPerSample;

        assert_eq!(u32::from(format_tag), WAVE_FORMAT_IEEE_FLOAT);
        assert_eq!(channels, 2);
        assert_eq!(sample_rate, 48_000);
        assert_eq!(block_align, 8);
        assert_eq!(bits_per_sample, 32);
    }

    #[test]
    fn exclusive_wave_format_uses_extensible_for_24_in_32() {
        let candidate = make_wave_format(
            2,
            SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            96_000,
            ExclusiveSampleFormat::I24In32,
            false,
        )
        .unwrap();
        let format = candidate.wave.Format;
        let valid_bits = unsafe { candidate.wave.Samples.wValidBitsPerSample };
        let format_tag = format.wFormatTag;
        let block_align = format.nBlockAlign;
        let bits_per_sample = format.wBitsPerSample;
        let sub_format = candidate.wave.SubFormat;

        assert_eq!(u32::from(format_tag), WAVE_FORMAT_EXTENSIBLE);
        assert_eq!(block_align, 8);
        assert_eq!(bits_per_sample, 32);
        assert_eq!(valid_bits, 24);
        assert_eq!(sub_format, KSDATAFORMAT_SUBTYPE_PCM);
    }

    #[test]
    fn exclusive_wave_format_can_force_extensible_stereo_representation() {
        let candidate = make_wave_format(
            2,
            SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT,
            48_000,
            ExclusiveSampleFormat::F32,
            true,
        )
        .unwrap();
        let format = candidate.wave.Format;
        let format_tag = format.wFormatTag;
        let extension_size = format.cbSize;
        let sub_format = candidate.wave.SubFormat;

        assert_eq!(u32::from(format_tag), WAVE_FORMAT_EXTENSIBLE);
        assert_eq!(usize::from(extension_size), 22);
        assert_eq!(sub_format, KSDATAFORMAT_SUBTYPE_IEEE_FLOAT);
    }

    #[test]
    fn packed_i24_encodes_signed_little_endian_samples() {
        assert_eq!(PackedI24::from_f32(0.0), PackedI24([0, 0, 0]));
        assert_eq!(PackedI24::from_f32(1.0), PackedI24([0xff, 0xff, 0x7f]));
        assert_eq!(PackedI24::from_f32(-1.0), PackedI24([0x01, 0x00, 0x80]));
    }

    #[test]
    fn i24_in_i32_left_aligns_valid_sample_bits() {
        assert_eq!(PcmI24In32::from_f32(0.0), PcmI24In32(0));
        assert_eq!(PcmI24In32::from_f32(1.0), PcmI24In32(0x7f_ff_ff_00));
        assert_eq!(PcmI24In32::from_f32(-1.0), PcmI24In32(-0x7f_ff_ff_00));
    }

    fn clock_reading(position: u64) -> ExclusiveClockReading {
        ExclusiveClockReading {
            position,
            frequency: 48_000,
            qpc_100ns: 1_000_000,
            counter: 1_000_000,
            qpc_frequency: 10_000_000,
        }
    }

    fn reading_at(position: u64, elapsed_us: u64) -> ExclusiveClockReading {
        ExclusiveClockReading {
            position,
            frequency: 48_000,
            qpc_100ns: elapsed_us * 10,
            counter: (elapsed_us * 10) as i64,
            qpc_frequency: 10_000_000,
        }
    }

    #[test]
    fn persistent_position_jump_never_turns_callback_jitter_into_voice_skips() {
        for jump in [48_000_i64, -4_800] {
            let mut tracker = ExclusiveClockTracker::default();
            let period = Duration::from_millis(10);
            assert_eq!(
                tracker
                    .observe(reading_at(9_600, 200_000), 10_080, 48_000, 0, period)
                    .missed_frames,
                0
            );
            for tick in 1..=1_000_u64 {
                let jitter = if tick % 2 == 0 { 100 } else { 0 };
                let position = (9_600 + tick as i64 * 480 + jump) as u64;
                let state = tracker.observe(
                    reading_at(position, 200_000 + tick * 10_000 + jitter),
                    10_080 + tick * 480,
                    48_000,
                    if jitter != 0 { 4 } else { 0 },
                    period,
                );
                // Only the prediction returns: the jump itself never skips voices.
                assert_eq!(
                    state,
                    ExclusiveOutputState {
                        next_buffer_delta_ns: (tick > 100).then_some(10_000_000),
                        missed_frames: 0
                    },
                    "{jump} at {tick}"
                );
            }
        }
    }

    #[test]
    fn a_position_glitch_returns_on_two_distinct_readings() {
        let period = Duration::from_millis(10);
        let mut tracker = ExclusiveClockTracker::default();
        tracker.observe(reading_at(9_600, 200_000), 10_080, 48_000, 0, period);
        assert_eq!(
            tracker.observe(reading_at(58_080, 210_000), 10_560, 48_000, 0, period),
            ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 0 }
        );
        let reading = reading_at(10_560, 220_000);
        for _ in 0..2 {
            assert_eq!(
                tracker.observe(reading, 11_040, 48_000, 0, period).next_buffer_delta_ns,
                None
            );
        }
        assert_eq!(
            tracker.observe(reading_at(11_040, 230_000), 11_520, 48_000, 0, period),
            ExclusiveOutputState { next_buffer_delta_ns: Some(10_000_000), missed_frames: 0 }
        );
    }

    #[test]
    fn an_unstable_position_never_replaces_the_accepted_correspondence() {
        let period = Duration::from_millis(10);
        let mut tracker = ExclusiveClockTracker::default();
        tracker.observe(reading_at(9_600, 200_000), 10_080, 48_000, 0, period);
        for tick in 1..=1_000_u64 {
            let jump = if tick % 2 == 0 { 48_000 } else { 96_000 };
            let state = tracker.observe(
                reading_at(9_600 + tick * 480 + jump, 200_000 + tick * 10_000),
                10_080 + tick * 480,
                48_000,
                0,
                period,
            );
            assert_eq!(
                state,
                ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 0 }
            );
        }
    }

    #[test]
    fn a_position_lag_regains_prediction_and_underrun_correction() {
        for lag_ms in [25_u64, 100] {
            let period = Duration::from_millis(10);
            let mut tracker = ExclusiveClockTracker::default();
            let (mut position, mut elapsed_us, mut written) = (9_600_u64, 200_000_u64, 10_080_u64);
            tracker.observe(reading_at(position, elapsed_us), written, 48_000, 0, period);
            // The device stopped with the callback: time passed, the PCM did not.
            position += 480;
            written += 480;
            elapsed_us += 10_000 + lag_ms * 1_000;
            let state = tracker.observe(
                reading_at(position, elapsed_us),
                written,
                48_000,
                lag_ms * 48,
                period,
            );
            assert_eq!(
                state,
                ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 0 }
            );
            for tick in 1..=100 {
                position += 480;
                written += 480;
                elapsed_us += 10_000;
                let state =
                    tracker.observe(reading_at(position, elapsed_us), written, 48_000, 0, period);
                assert_eq!(state.missed_frames, 0);
                assert_eq!(state.next_buffer_delta_ns.is_some(), tick == 100, "{lag_ms} ms {tick}");
            }
            assert_eq!(
                tracker
                    .observe(
                        reading_at(position + 480, elapsed_us + 10_000),
                        written + 480,
                        48_000,
                        0,
                        period,
                    )
                    .next_buffer_delta_ns,
                Some(10_000_000)
            );
            // A real underrun is corrected again.
            let state = tracker.observe(
                reading_at(position + 1_920, elapsed_us + 40_000),
                written + 960,
                48_000,
                960,
                period,
            );
            assert_eq!(state.missed_frames, 480);
        }
    }

    #[test]
    fn validated_underrun_is_accounted_once() {
        let mut tracker = ExclusiveClockTracker::default();
        let period = Duration::from_millis(10);
        tracker.observe(reading_at(9_600, 200_000), 10_080, 48_000, 0, period);
        let state = tracker.observe(reading_at(11_040, 230_000), 10_560, 48_000, 960, period);
        assert_eq!(state.missed_frames, 480);
        // Keep submitted PCM and the recovered interval separate, as the worker does.
        let submitted = 10_560 + 480;
        let recovered = state.missed_frames;
        let state =
            tracker.observe(reading_at(11_520, 240_000), submitted + recovered, 48_000, 0, period);
        assert_eq!(state.missed_frames, 0);
        assert_eq!(state.next_buffer_delta_ns, Some(0));
    }

    #[test]
    fn stale_or_malformed_clock_readings_cannot_advance_voices() {
        for invalid in [
            ExclusiveClockReading { frequency: 0, ..reading_at(11_040, 230_000) },
            ExclusiveClockReading { qpc_frequency: 0, ..reading_at(11_040, 230_000) },
            ExclusiveClockReading { qpc_100ns: 10, ..reading_at(11_040, 230_000) },
            ExclusiveClockReading { counter: -1, ..reading_at(11_040, 230_000) },
        ] {
            let mut tracker = ExclusiveClockTracker::default();
            let period = Duration::from_millis(10);
            tracker.observe(reading_at(9_600, 200_000), 10_080, 48_000, 0, period);
            assert_eq!(
                tracker.observe(invalid, 10_560, 48_000, 960, period),
                ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 0 }
            );
        }
    }

    #[test]
    fn ordinary_device_rate_error_remains_valid_for_ten_minutes() {
        for ppm in [-200_i64, -40, 0, 40, 200] {
            let mut tracker = ExclusiveClockTracker::default();
            for tick in 1..=60_000_u64 {
                let position = (tick as i64 * 480 * (1_000_000 + ppm) / 1_000_000) as u64;
                let state = tracker.observe(
                    reading_at(position, tick * 10_000),
                    position + 480,
                    48_000,
                    0,
                    Duration::from_millis(10),
                );
                assert_eq!(state.next_buffer_delta_ns, Some(10_000_000), "{ppm} ppm at {tick}");
                assert_eq!(state.missed_frames, 0);
            }
        }
    }

    #[test]
    fn next_buffer_follows_the_frames_queued_past_the_position() {
        assert_eq!(
            exclusive_output_state(clock_reading(9_600), 10_080, 48_000, 0),
            ExclusiveOutputState { next_buffer_delta_ns: Some(10_000_000), missed_frames: 0 }
        );
    }

    #[test]
    fn position_may_count_bytes_instead_of_frames() {
        let mut reading = clock_reading(9_600 * 8);
        reading.frequency = 48_000 * 8;
        let state = exclusive_output_state(reading, 10_080, 48_000, 0);
        assert_eq!(state.next_buffer_delta_ns, Some(10_000_000));
    }

    #[test]
    fn an_older_position_reading_moves_the_prediction_back() {
        let mut reading = clock_reading(9_600);
        reading.qpc_100ns -= 20_000;
        let state = exclusive_output_state(reading, 10_080, 48_000, 0);
        assert_eq!(state.next_buffer_delta_ns, Some(8_000_000));
    }

    #[test]
    fn an_underrun_within_the_late_callback_is_reported_in_full() {
        assert_eq!(
            exclusive_output_state(clock_reading(10_320), 10_080, 48_000, 480),
            ExclusiveOutputState { next_buffer_delta_ns: Some(0), missed_frames: 240 }
        );
    }

    #[test]
    fn a_position_jump_with_regular_callbacks_does_not_skip_audio() {
        assert_eq!(
            exclusive_output_state(clock_reading(58_080), 10_080, 48_000, 0),
            ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 0 }
        );
    }

    #[test]
    fn a_position_jump_is_limited_to_how_late_the_callback_was() {
        let state = exclusive_output_state(clock_reading(58_080), 10_080, 48_000, 480);
        assert_eq!(state.missed_frames, 480);
    }

    #[test]
    fn a_real_underrun_survives_an_unusable_prediction() {
        let mut reading = clock_reading(10_320);
        reading.counter += 2_000_000;
        assert_eq!(
            exclusive_output_state(reading, 10_080, 48_000, 480),
            ExclusiveOutputState { next_buffer_delta_ns: None, missed_frames: 240 }
        );
    }
}
