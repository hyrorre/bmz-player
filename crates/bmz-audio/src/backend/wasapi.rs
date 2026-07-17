//! WASAPI shared-mode engine-period control using `IAudioClient3`.
//!
//! CPAL owns the audible stream and mixer. This module keeps a silent stream open on the same
//! endpoint and processing mode with the requested engine period. WASAPI applies that period to
//! the shared audio engine, so CPAL's event-driven stream is woken at the lower period too.

use std::ffi::c_void;
use std::io;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use thiserror::Error;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, IAudioClient3,
    IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects};
use windows::core::PCWSTR;

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
}
