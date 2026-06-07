use std::rc::{Rc, Weak as RcWeak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ::cpal::{SampleFormat, StreamConfig};
use bmz_core::time::TimeUs;
use thiserror::Error;

use crate::clock::AudioClock;
use crate::engine::AudioEngine;

pub type SharedAudioEngine = Arc<Mutex<AudioEngine>>;
type SharedAudioSources = Arc<Mutex<Vec<AudioSource>>>;

#[derive(Debug, Default)]
pub struct CpalBackend;

#[derive(Debug, Clone, Default)]
pub struct CpalOutputConfig {
    pub host: Option<CpalHostId>,
    pub output_device_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpalHostId {
    Wasapi,
    Asio,
    CoreAudio,
    Alsa,
    Pulse,
}

pub struct CpalOutput {
    _shared: CpalSharedOutput,
    source: CpalOutputSource,
}

#[derive(Clone)]
pub struct CpalSharedOutput {
    inner: Rc<CpalSharedOutputInner>,
}

struct CpalSharedOutputInner {
    stream: ::cpal::Stream,
    sample_rate: u32,
    current_frame: Arc<AtomicU64>,
    sources: SharedAudioSources,
    next_source_id: AtomicU64,
}

pub struct CpalOutputSource {
    id: u64,
    inner: RcWeak<CpalSharedOutputInner>,
    pub engine: SharedAudioEngine,
    pub clock: AudioClock,
}

#[derive(Clone)]
struct AudioSource {
    id: u64,
    engine: SharedAudioEngine,
}

#[derive(Debug, Error)]
pub enum CpalBackendError {
    #[error("no default output device is available")]
    MissingDefaultOutputDevice,

    #[error("requested output device is not available: {0}")]
    MissingRequestedOutputDevice(String),

    #[error("requested cpal host is not available on this build or platform: {0:?}")]
    UnsupportedHost(CpalHostId),

    #[error("requested cpal host is unavailable")]
    HostUnavailable(#[from] ::cpal::HostUnavailable),

    #[error("failed to enumerate output devices")]
    OutputDevices(#[from] ::cpal::DevicesError),

    #[error("failed to query output device name")]
    OutputDeviceName(#[from] ::cpal::DeviceNameError),

    #[error("failed to query default output config")]
    DefaultOutputConfig(#[from] ::cpal::DefaultStreamConfigError),

    #[error("failed to build output stream")]
    BuildStream(#[from] ::cpal::BuildStreamError),

    #[error("failed to play output stream")]
    PlayStream(#[from] ::cpal::PlayStreamError),
}

impl CpalBackend {
    pub fn open_default(engine: SharedAudioEngine) -> Result<CpalOutput, CpalBackendError> {
        let mut shared = Self::open_shared_default()?;
        shared.play()?;
        let source = shared.add_source(engine);
        Ok(CpalOutput { _shared: shared, source })
    }

    pub fn open_shared_default() -> Result<CpalSharedOutput, CpalBackendError> {
        Self::open_shared(CpalOutputConfig::default())
    }

    pub fn open_shared(config: CpalOutputConfig) -> Result<CpalSharedOutput, CpalBackendError> {
        let host = match config.host {
            Some(host_id) => {
                let Some(cpal_host_id) = cpal_host_id(host_id) else {
                    return Err(CpalBackendError::UnsupportedHost(host_id));
                };
                ::cpal::host_from_id(cpal_host_id)?
            }
            None => ::cpal::default_host(),
        };
        let device = output_device(&host, config.output_device_name.as_deref())?;
        let supported_config = device.default_output_config()?;
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let sample_rate = config.sample_rate.0;
        let current_frame = Arc::new(AtomicU64::new(0));
        let sources = Arc::new(Mutex::new(Vec::new()));

        let stream = match sample_format {
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                &config,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
            )?,
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                &config,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
            )?,
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                &config,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
            )?,
            SampleFormat::I32 => build_output_stream::<i32>(
                &device,
                &config,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
            )?,
            _ => build_output_stream::<f32>(
                &device,
                &config,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
            )?,
        };

        Ok(CpalSharedOutput {
            inner: Rc::new(CpalSharedOutputInner {
                stream,
                sample_rate,
                current_frame,
                sources,
                next_source_id: AtomicU64::new(1),
            }),
        })
    }
}

fn cpal_host_id(host: CpalHostId) -> Option<::cpal::HostId> {
    match host {
        #[cfg(windows)]
        CpalHostId::Wasapi => Some(::cpal::HostId::Wasapi),
        #[cfg(not(windows))]
        CpalHostId::Wasapi => None,

        #[cfg(all(windows, feature = "asio"))]
        CpalHostId::Asio => Some(::cpal::HostId::Asio),
        #[cfg(not(all(windows, feature = "asio")))]
        CpalHostId::Asio => None,

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        CpalHostId::CoreAudio => Some(::cpal::HostId::CoreAudio),
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        CpalHostId::CoreAudio => None,

        #[cfg(target_os = "linux")]
        CpalHostId::Alsa => Some(::cpal::HostId::Alsa),
        #[cfg(not(target_os = "linux"))]
        CpalHostId::Alsa => None,

        #[cfg(target_os = "linux")]
        CpalHostId::Pulse => Some(::cpal::HostId::Pulse),
        #[cfg(not(target_os = "linux"))]
        CpalHostId::Pulse => None,
    }
}

pub fn is_host_supported(host: CpalHostId) -> bool {
    cpal_host_id(host).is_some()
}

/// 指定ホスト(`None` は既定ホスト)の出力デバイス名を列挙する。
///
/// UI のデバイス選択用。列挙に失敗した場合やホストが利用不可の場合は空 Vec を返す
/// (致命的エラーにはしない)。ASIO ホストではドライバ名が列挙される。
pub fn list_output_device_names(host: Option<CpalHostId>) -> Vec<String> {
    let host = match host {
        Some(host_id) => match cpal_host_id(host_id) {
            Some(cpal_host_id) => match ::cpal::host_from_id(cpal_host_id) {
                Ok(host) => host,
                Err(_) => return Vec::new(),
            },
            None => return Vec::new(),
        },
        None => ::cpal::default_host(),
    };

    let Ok(devices) = host.output_devices() else {
        return Vec::new();
    };
    devices.filter_map(|device| device.name().ok()).collect()
}

fn output_device(
    host: &::cpal::Host,
    requested_name: Option<&str>,
) -> Result<::cpal::Device, CpalBackendError> {
    let requested_name = requested_name.map(str::trim).filter(|name| !name.is_empty());
    let Some(requested_name) = requested_name else {
        return host.default_output_device().ok_or(CpalBackendError::MissingDefaultOutputDevice);
    };

    for device in host.output_devices()? {
        if device.name()? == requested_name {
            return Ok(device);
        }
    }

    Err(CpalBackendError::MissingRequestedOutputDevice(requested_name.to_string()))
}

impl CpalOutput {
    pub fn play(&mut self, chart_zero_time: TimeUs) -> Result<(), CpalBackendError> {
        self.source.play(chart_zero_time);
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), ::cpal::PauseStreamError> {
        self.source.pause();
        Ok(())
    }

    pub fn clock(&self) -> AudioClock {
        self.source.clock()
    }
}

impl CpalSharedOutput {
    pub fn play(&mut self) -> Result<(), CpalBackendError> {
        self.inner.stream.play()?;
        Ok(())
    }

    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate
    }

    pub fn add_source(&self, engine: SharedAudioEngine) -> CpalOutputSource {
        if let Ok(mut engine) = engine.lock() {
            engine.mixer.output_sample_rate = self.inner.sample_rate;
        }

        let id = self.inner.next_source_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut sources) = self.inner.sources.lock() {
            sources.push(AudioSource { id, engine: Arc::clone(&engine) });
        }

        let clock = AudioClock::with_position(
            self.inner.sample_rate,
            0,
            0,
            Arc::clone(&self.inner.current_frame),
            false,
        );
        CpalOutputSource { id, inner: Rc::downgrade(&self.inner), engine, clock }
    }
}

impl CpalOutputSource {
    pub fn play(&mut self, chart_zero_time: TimeUs) {
        self.clock.chart_zero_time_us = chart_zero_time.0;
        self.clock.start_output_frame = self.clock.current_frame.load(Ordering::Relaxed);
        self.clock.running = true;
    }

    pub fn pause(&mut self) {
        self.clock.running = false;
    }

    pub fn clock(&self) -> AudioClock {
        self.clock.clone()
    }
}

impl Drop for CpalOutputSource {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        if let Ok(mut sources) = inner.sources.lock() {
            sources.retain(|source| source.id != self.id);
        }
    }
}

fn build_output_stream<T>(
    device: &::cpal::Device,
    config: &StreamConfig,
    sources: SharedAudioSources,
    current_frame: Arc<AtomicU64>,
) -> Result<::cpal::Stream, ::cpal::BuildStreamError>
where
    T: ::cpal::SizedSample + OutputSample,
{
    let channels = config.channels as usize;
    let mut mix = Vec::new();
    let mut source_scratch = Vec::new();
    let mut source_engines = Vec::new();
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            if channels == 0 {
                data.fill(T::from_f32(0.0));
                return;
            }

            let start_frame = current_frame.load(Ordering::Relaxed);
            render_output(
                data,
                channels,
                start_frame,
                &sources,
                &mut mix,
                &mut source_scratch,
                &mut source_engines,
            );
            current_frame.fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
        },
        move |error| {
            tracing::warn!(%error, "cpal output stream error");
        },
        None,
    )
}

fn render_output<T: OutputSample>(
    data: &mut [T],
    channels: usize,
    output_start_frame: u64,
    sources: &SharedAudioSources,
    mix: &mut Vec<f32>,
    source_scratch: &mut Vec<f32>,
    source_engines: &mut Vec<SharedAudioEngine>,
) {
    if channels == 0 {
        return;
    }

    let frames = data.len() / channels;
    mix_sources_stereo(output_start_frame, frames, sources, mix, source_scratch, source_engines);

    write_interleaved_output(data, channels, mix);
}

fn mix_sources_stereo(
    output_start_frame: u64,
    frames: usize,
    sources: &SharedAudioSources,
    mix: &mut Vec<f32>,
    source_scratch: &mut Vec<f32>,
    source_engines: &mut Vec<SharedAudioEngine>,
) {
    mix.resize(frames * 2, 0.0);
    mix.fill(0.0);

    // オーディオ(ASIO)コールバックはハードリアルタイム。ここで `lock()` すると
    // ゲームスレッドがロックを保持している間ブロックし、小バッファでは締切を超えて
    // xrun(全体のプツプツ)を起こす。`try_lock` で「決してブロックしない」を保証し、
    // 競合したバッファだけスキップ(その音源は 1 バッファ無音)に留める。
    source_engines.clear();
    if let Ok(sources) = sources.try_lock() {
        source_engines.extend(sources.iter().map(|source| Arc::clone(&source.engine)));
    }

    source_scratch.resize(frames * 2, 0.0);
    for engine in source_engines.iter() {
        if let Ok(mut engine) = engine.try_lock() {
            engine.render_stereo(output_start_frame, source_scratch);
            for (dst, src) in mix.iter_mut().zip(source_scratch.iter()) {
                *dst += *src;
            }
        }
    }
}

fn write_interleaved_output<T: OutputSample>(data: &mut [T], channels: usize, stereo: &[f32]) {
    if channels == 0 {
        return;
    }

    for (frame_index, frame) in data.chunks_mut(channels).enumerate() {
        let left = stereo.get(frame_index * 2).copied().unwrap_or(0.0);
        let right = stereo.get(frame_index * 2 + 1).copied().unwrap_or(0.0);
        match frame {
            [mono] => *mono = T::from_f32((left + right) * 0.5),
            [left_out, right_out, rest @ ..] => {
                *left_out = T::from_f32(left);
                *right_out = T::from_f32(right);
                for sample in rest {
                    *sample = T::from_f32(0.0);
                }
            }
            [] => {}
        }
    }
}

trait OutputSample: Copy {
    fn from_f32(value: f32) -> Self;
}

impl OutputSample for f32 {
    fn from_f32(value: f32) -> Self {
        value.clamp(-1.0, 1.0)
    }
}

impl OutputSample for i16 {
    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
}

impl OutputSample for u16 {
    fn from_f32(value: f32) -> Self {
        ((value.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
    }
}

impl OutputSample for i32 {
    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_interleaved_output_downmixes_mono() {
        let mut output = vec![0.0_f32; 2];

        write_interleaved_output(&mut output, 1, &[0.25, 0.75, -0.5, 0.25]);

        assert_eq!(output, vec![0.5, -0.125]);
    }

    #[test]
    fn write_interleaved_output_fills_extra_channels_with_silence() {
        let mut output = vec![1.0_f32; 6];

        write_interleaved_output(&mut output, 3, &[0.25, 0.75, -0.5, 0.25]);

        assert_eq!(output, vec![0.25, 0.75, 0.0, -0.5, 0.25, 0.0]);
    }

    #[test]
    fn render_output_reuses_scratch_buffer() {
        let engine = Arc::new(Mutex::new(AudioEngine::default()));
        let sources = Arc::new(Mutex::new(vec![AudioSource { id: 1, engine }]));
        let mut output = vec![1.0_f32; 4];
        let mut mix = Vec::with_capacity(16);
        let mut source_scratch = Vec::new();
        let mut source_engines = Vec::new();

        render_output(
            &mut output,
            2,
            0,
            &sources,
            &mut mix,
            &mut source_scratch,
            &mut source_engines,
        );

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0]);
        assert_eq!(mix.len(), 4);
        assert!(mix.capacity() >= 16);
    }

    #[test]
    fn mix_sources_stereo_adds_registered_engines() {
        use bmz_core::ids::SoundId;

        let first = Arc::new(Mutex::new(AudioEngine::default()));
        let second = Arc::new(Mutex::new(AudioEngine::default()));
        {
            let mut first = first.lock().unwrap();
            first.insert_sample(
                SoundId(1),
                crate::sample::DecodedSample {
                    channels: 1,
                    sample_rate: 48_000,
                    frames: vec![0.25],
                },
            );
            first.play_now(SoundId(1), 1.0, false);
        }
        {
            let mut second = second.lock().unwrap();
            second.insert_sample(
                SoundId(1),
                crate::sample::DecodedSample {
                    channels: 1,
                    sample_rate: 48_000,
                    frames: vec![0.5],
                },
            );
            second.play_now(SoundId(1), 1.0, false);
        }
        let sources = Arc::new(Mutex::new(vec![
            AudioSource { id: 1, engine: first },
            AudioSource { id: 2, engine: second },
        ]));
        let mut mix = Vec::new();
        let mut scratch = Vec::new();
        let mut engines = Vec::new();

        mix_sources_stereo(0, 1, &sources, &mut mix, &mut scratch, &mut engines);

        assert_eq!(mix, vec![0.75, 0.75]);
    }
}
