use std::rc::{Rc, Weak as RcWeak};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ::cpal::{SampleFormat, StreamConfig};
use bmz_core::time::TimeUs;
use thiserror::Error;

use crate::clock::AudioClock;
use crate::engine::AudioEngine;

pub type SharedAudioEngine = Arc<Mutex<AudioEngine>>;
type SharedAudioSources = Arc<Mutex<Vec<AudioSource>>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CpalOutputSourceKind {
    #[default]
    Other,
    System,
    Play,
    Draining,
}

impl CpalOutputSourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::System => "system",
            Self::Play => "play",
            Self::Draining => "draining",
        }
    }
}

#[derive(Debug, Default)]
pub struct CpalBackend;

#[derive(Debug, Clone, Default)]
pub struct CpalOutputConfig {
    pub host: Option<CpalHostId>,
    pub output_device_name: Option<String>,
    /// 出力サンプルレート(Hz)。`None` はデバイス既定。デバイスが対応しない値は
    /// 既定レートへフォールバックする。
    pub sample_rate: Option<u32>,
    /// 1 コールバックあたりのバッファフレーム数。`None` はデバイス既定(自動)。
    /// `Some(n)` でも端末がサポートする範囲にクランプされる。
    pub buffer_size: Option<u32>,
    /// ステレオを書き込む先頭チャンネル(0 始まりのインターリーブ位置)。
    /// 0 = 1-2ch, 2 = 3-4ch, 4 = 5-6ch …。デバイスのチャンネル数を超える場合は
    /// ストリーム生成時に有効な範囲へクランプされる。
    pub channel_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpalHostId {
    Wasapi,
    Asio,
    CoreAudio,
    Alsa,
    Pulse,
    PipeWire,
}

pub struct CpalOutput {
    _shared: CpalSharedOutput,
    source: CpalOutputSource,
}

/// Snapshot of the shared output stream diagnostics.
///
/// Counts are cumulative since stream creation. `peak_abs` and `max_callback_ns`
/// are interval maxima since the previous snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CpalOutputDiagnostics {
    pub callback_count: u64,
    pub rendered_frames: u64,
    pub stream_error_count: u64,
    pub source_lock_miss_count: u64,
    pub engine_lock_miss_count: u64,
    pub engine_lock_miss_callback_count: u64,
    pub system_engine_lock_miss_count: u64,
    pub play_engine_lock_miss_count: u64,
    pub draining_engine_lock_miss_count: u64,
    pub other_engine_lock_miss_count: u64,
    pub clipped_sample_count: u64,
    pub peak_abs: f32,
    pub max_callback_ns: u64,
}

#[derive(Debug, Default)]
struct CpalOutputDiagnosticsCounters {
    callback_count: AtomicU64,
    rendered_frames: AtomicU64,
    stream_error_count: AtomicU64,
    source_lock_miss_count: AtomicU64,
    engine_lock_miss_count: AtomicU64,
    engine_lock_miss_callback_count: AtomicU64,
    system_engine_lock_miss_count: AtomicU64,
    play_engine_lock_miss_count: AtomicU64,
    draining_engine_lock_miss_count: AtomicU64,
    other_engine_lock_miss_count: AtomicU64,
    clipped_sample_count: AtomicU64,
    peak_abs_bits: AtomicU32,
    max_callback_ns: AtomicU64,
}

#[derive(Clone)]
pub struct CpalSharedOutput {
    inner: Rc<CpalSharedOutputInner>,
}

struct CpalSharedOutputInner {
    stream: ::cpal::Stream,
    host_id: ::cpal::HostId,
    sample_rate: u32,
    current_frame: Arc<AtomicU64>,
    sources: SharedAudioSources,
    diagnostics: Arc<CpalOutputDiagnosticsCounters>,
    next_source_id: AtomicU64,
}

pub struct CpalOutputSource {
    id: u64,
    inner: RcWeak<CpalSharedOutputInner>,
    kind: CpalOutputSourceKind,
    pub engine: SharedAudioEngine,
    pub clock: AudioClock,
}

#[derive(Clone)]
struct AudioSource {
    id: u64,
    kind: CpalOutputSourceKind,
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
    HostUnavailable(::cpal::Error),

    #[error("failed to enumerate output devices")]
    OutputDevices(::cpal::Error),

    #[error("failed to query default output config")]
    DefaultOutputConfig(::cpal::Error),

    #[error("failed to build output stream")]
    BuildStream(::cpal::Error),

    #[error("failed to play output stream")]
    PlayStream(::cpal::Error),
}

impl CpalBackend {
    pub fn open_default(engine: SharedAudioEngine) -> Result<CpalOutput, CpalBackendError> {
        let shared = Self::open_shared_default()?;
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
                ::cpal::host_from_id(cpal_host_id).map_err(CpalBackendError::HostUnavailable)?
            }
            None => ::cpal::default_host(),
        };
        let device = output_device(&host, config.output_device_name.as_deref())?;
        let requested_sample_rate = config.sample_rate;
        let requested_buffer_size = config.buffer_size;
        let requested_channel_offset = config.channel_offset;
        let supported_config =
            device.default_output_config().map_err(CpalBackendError::DefaultOutputConfig)?;
        let sample_format = supported_config.sample_format();
        let default_sample_rate = supported_config.sample_rate();
        let supported_buffer_size = *supported_config.buffer_size();
        let mut config = supported_config.config();
        let sample_rate =
            resolve_sample_rate(&device, requested_sample_rate, default_sample_rate, sample_format);
        config.sample_rate = sample_rate;
        config.buffer_size = resolve_buffer_size(requested_buffer_size, &supported_buffer_size);
        let channel_offset =
            resolve_channel_offset(requested_channel_offset, config.channels as usize);

        // ASIO のバッファ問い合わせ結果を可視化する。ドライバが報告する
        // サポート範囲(`supported_buffer_size`)と、要求値・実際にストリームへ
        // 渡す値をログに残し、RME / ASIO4ALL などのレイテンシ調整を切り分けやすくする。
        let device_name = device_name(&device);
        tracing::info!(
            host = ?host.id(),
            device = %device_name,
            sample_format = ?sample_format,
            requested_sample_rate = ?requested_sample_rate,
            sample_rate,
            channels = config.channels,
            supported_buffer_size = ?supported_buffer_size,
            requested_buffer_size = ?requested_buffer_size,
            resolved_buffer_size = ?config.buffer_size,
            requested_channel_offset,
            channel_offset,
            "opening cpal output stream",
        );

        let current_frame = Arc::new(AtomicU64::new(0));
        let sources = Arc::new(Mutex::new(Vec::new()));
        let diagnostics = Arc::new(CpalOutputDiagnosticsCounters::default());

        let stream = match sample_format {
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                &config,
                channel_offset,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            )?,
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                &config,
                channel_offset,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            )?,
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                &config,
                channel_offset,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            )?,
            SampleFormat::I32 => build_output_stream::<i32>(
                &device,
                &config,
                channel_offset,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            )?,
            _ => build_output_stream::<f32>(
                &device,
                &config,
                channel_offset,
                Arc::clone(&sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            )?,
        };

        Ok(CpalSharedOutput {
            inner: Rc::new(CpalSharedOutputInner {
                stream,
                host_id: host.id(),
                sample_rate,
                current_frame,
                sources,
                diagnostics,
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

        #[cfg(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pulseaudio"
        ))]
        CpalHostId::Pulse => Some(::cpal::HostId::PulseAudio),
        #[cfg(not(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pulseaudio"
        )))]
        CpalHostId::Pulse => None,

        #[cfg(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pipewire"
        ))]
        CpalHostId::PipeWire => Some(::cpal::HostId::PipeWire),
        #[cfg(not(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pipewire"
        )))]
        CpalHostId::PipeWire => None,
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
    devices.map(|device| device_name(&device)).collect()
}

/// 要求サンプルレートがデバイスでサポートされていれば採用し、そうでなければ
/// デバイス既定レートへフォールバックする。`None` は既定レート。
fn resolve_sample_rate(
    device: &::cpal::Device,
    requested: Option<u32>,
    default_rate: u32,
    sample_format: SampleFormat,
) -> u32 {
    let Some(requested) = requested else {
        return default_rate;
    };
    if requested == default_rate {
        return requested;
    }

    let supported = match device.supported_output_configs() {
        Ok(configs) => configs.into_iter().any(|range| {
            range.sample_format() == sample_format
                && range.min_sample_rate() <= requested
                && requested <= range.max_sample_rate()
        }),
        Err(error) => {
            tracing::warn!(%error, "failed to query supported output configs for sample rate");
            false
        }
    };

    if supported {
        requested
    } else {
        tracing::warn!(
            requested,
            fallback = default_rate,
            "requested sample rate is not supported; using device default",
        );
        default_rate
    }
}

/// 要求バッファサイズをデバイスのサポート範囲にクランプして `BufferSize` を決める。
/// `None` はデバイス既定。範囲不明なら要求値をそのまま Fixed で渡す。
fn resolve_buffer_size(
    requested: Option<u32>,
    supported: &::cpal::SupportedBufferSize,
) -> ::cpal::BufferSize {
    match requested {
        None => ::cpal::BufferSize::Default,
        Some(frames) => {
            let frames = match supported {
                ::cpal::SupportedBufferSize::Range { min, max } => frames.clamp(*min, *max),
                ::cpal::SupportedBufferSize::Unknown => frames,
            };
            ::cpal::BufferSize::Fixed(frames)
        }
    }
}

/// ステレオを書き込む先頭チャンネル位置を、デバイスのチャンネル数に収まるよう
/// クランプする。ステレオ(2ch)が収まらない場合は 0(先頭ペア)へフォールバック。
fn resolve_channel_offset(requested: u32, channels: usize) -> usize {
    if channels < 2 {
        return 0;
    }
    // ステレオペアが収まる最大の先頭インデックス。
    let max_offset = channels - 2;
    (requested as usize).min(max_offset)
}

fn output_device(
    host: &::cpal::Host,
    requested_name: Option<&str>,
) -> Result<::cpal::Device, CpalBackendError> {
    let requested_name = requested_name.map(str::trim).filter(|name| !name.is_empty());
    let Some(requested_name) = requested_name else {
        return host.default_output_device().ok_or(CpalBackendError::MissingDefaultOutputDevice);
    };

    for device in host.output_devices().map_err(CpalBackendError::OutputDevices)? {
        if device_name(&device) == requested_name {
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

    pub fn pause(&mut self) -> Result<(), ::cpal::Error> {
        self.source.pause();
        Ok(())
    }

    pub fn clock(&self) -> AudioClock {
        self.source.clock()
    }
}

impl CpalSharedOutput {
    pub fn play(&self) -> Result<(), CpalBackendError> {
        self.inner.stream.play().map_err(CpalBackendError::PlayStream)?;
        Ok(())
    }

    pub fn uses_pulseaudio_host(&self) -> bool {
        #[cfg(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pulseaudio"
        ))]
        {
            matches!(self.inner.host_id, ::cpal::HostId::PulseAudio)
        }
        #[cfg(not(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pulseaudio"
        )))]
        {
            let _ = self.inner.host_id;
            false
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate
    }

    pub fn take_diagnostics(&self) -> CpalOutputDiagnostics {
        self.inner.diagnostics.take_snapshot()
    }

    pub fn add_source(&self, engine: SharedAudioEngine) -> CpalOutputSource {
        self.add_source_with_kind(engine, CpalOutputSourceKind::Other)
    }

    pub fn add_source_with_kind(
        &self,
        engine: SharedAudioEngine,
        kind: CpalOutputSourceKind,
    ) -> CpalOutputSource {
        if let Ok(mut engine) = engine.lock() {
            // 実ストリームレートへ揃える。既に読込済みのサンプルもここで再変換され、
            // ミキサーは等倍(補間なし)で再生できる。
            engine.set_output_sample_rate(self.inner.sample_rate);
        }

        let id = self.inner.next_source_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut sources) = self.inner.sources.lock() {
            sources.push(AudioSource { id, kind, engine: Arc::clone(&engine) });
        }

        let clock = AudioClock::with_position(
            self.inner.sample_rate,
            0,
            0,
            Arc::clone(&self.inner.current_frame),
            false,
        );
        CpalOutputSource { id, inner: Rc::downgrade(&self.inner), kind, engine, clock }
    }
}

impl CpalOutputSource {
    pub fn kind(&self) -> CpalOutputSourceKind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: CpalOutputSourceKind) {
        self.kind = kind;
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        if let Ok(mut sources) = inner.sources.lock()
            && let Some(source) = sources.iter_mut().find(|source| source.id == self.id)
        {
            source.kind = kind;
        }
    }

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
    channel_offset: usize,
    sources: SharedAudioSources,
    current_frame: Arc<AtomicU64>,
    diagnostics: Arc<CpalOutputDiagnosticsCounters>,
) -> Result<::cpal::Stream, CpalBackendError>
where
    T: ::cpal::SizedSample + OutputSample,
{
    let channels = config.channels as usize;
    let mut mix = Vec::new();
    let mut source_scratch = Vec::new();
    let mut source_engines = Vec::new();
    let error_diagnostics = Arc::clone(&diagnostics);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _| {
                let callback_start = Instant::now();
                diagnostics.callback_count.fetch_add(1, Ordering::Relaxed);
                if channels == 0 {
                    data.fill(T::from_f32(0.0));
                    diagnostics.observe_callback_duration(callback_start);
                    return;
                }

                let start_frame = current_frame.load(Ordering::Relaxed);
                let frames = data.len() / channels;
                render_output(
                    data,
                    channels,
                    channel_offset,
                    start_frame,
                    &sources,
                    &mut mix,
                    &mut source_scratch,
                    &mut source_engines,
                    &diagnostics,
                );
                diagnostics.rendered_frames.fetch_add(frames as u64, Ordering::Relaxed);
                current_frame.fetch_add(frames as u64, Ordering::Relaxed);
                diagnostics.observe_callback_duration(callback_start);
            },
            move |error| {
                error_diagnostics.stream_error_count.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, "cpal output stream error");
            },
            None,
        )
        .map_err(CpalBackendError::BuildStream)
}

fn device_name(device: &::cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| device.to_string())
}

fn render_output<T: OutputSample>(
    data: &mut [T],
    channels: usize,
    channel_offset: usize,
    output_start_frame: u64,
    sources: &SharedAudioSources,
    mix: &mut Vec<f32>,
    source_scratch: &mut Vec<f32>,
    source_engines: &mut Vec<AudioSource>,
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    if channels == 0 {
        return;
    }

    let frames = data.len() / channels;
    mix_sources_stereo(
        output_start_frame,
        frames,
        sources,
        mix,
        source_scratch,
        source_engines,
        diagnostics,
    );

    write_interleaved_output(data, channels, channel_offset, mix, diagnostics);
}

fn mix_sources_stereo(
    output_start_frame: u64,
    frames: usize,
    sources: &SharedAudioSources,
    mix: &mut Vec<f32>,
    source_scratch: &mut Vec<f32>,
    source_engines: &mut Vec<AudioSource>,
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    mix.resize(frames * 2, 0.0);
    mix.fill(0.0);

    // オーディオ(ASIO)コールバックはハードリアルタイム。ここで `lock()` すると
    // ゲームスレッドがロックを保持している間ブロックし、小バッファでは締切を超えて
    // xrun(全体のプツプツ)を起こす。`try_lock` で「決してブロックしない」を保証し、
    // 競合したバッファだけスキップ(その音源は 1 バッファ無音)に留める。
    source_engines.clear();
    match sources.try_lock() {
        Ok(sources) => {
            source_engines.extend(sources.iter().cloned());
        }
        Err(_) => {
            diagnostics.source_lock_miss_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    source_scratch.resize(frames * 2, 0.0);
    let mut missed_engine_lock = false;
    for source in source_engines.iter() {
        if let Ok(mut engine) = source.engine.try_lock() {
            engine.render_stereo(output_start_frame, source_scratch);
            for (dst, src) in mix.iter_mut().zip(source_scratch.iter()) {
                *dst += *src;
            }
        } else {
            missed_engine_lock = true;
            diagnostics.record_engine_lock_miss(source.kind);
        }
    }
    if missed_engine_lock {
        diagnostics.engine_lock_miss_callback_count.fetch_add(1, Ordering::Relaxed);
    }
}

fn write_interleaved_output<T: OutputSample>(
    data: &mut [T],
    channels: usize,
    channel_offset: usize,
    stereo: &[f32],
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    if channels == 0 {
        return;
    }

    // ステレオを書き込む先頭チャンネル。ペア(offset, offset+1)が収まらない場合は 0 へ。
    let left_channel =
        if channels >= 2 && channel_offset + 1 < channels { channel_offset } else { 0 };
    let silence = T::from_f32(0.0);
    let mut clipped = 0u64;
    let mut peak_abs = 0.0f32;

    for (frame_index, frame) in data.chunks_mut(channels).enumerate() {
        let left = stereo.get(frame_index * 2).copied().unwrap_or(0.0);
        let right = stereo.get(frame_index * 2 + 1).copied().unwrap_or(0.0);
        if channels == 1 {
            let mono = (left + right) * 0.5;
            observe_output_sample(mono, &mut clipped, &mut peak_abs);
            frame[0] = T::from_f32(mono);
            continue;
        }
        // 対象ペア以外は無音にして、選択チャンネルへ L/R を書く。
        for sample in frame.iter_mut() {
            *sample = silence;
        }
        observe_output_sample(left, &mut clipped, &mut peak_abs);
        observe_output_sample(right, &mut clipped, &mut peak_abs);
        frame[left_channel] = T::from_f32(left);
        frame[left_channel + 1] = T::from_f32(right);
    }
    diagnostics.observe_output_peak(peak_abs);
    if clipped != 0 {
        diagnostics.clipped_sample_count.fetch_add(clipped, Ordering::Relaxed);
    }
}

fn observe_output_sample(value: f32, clipped: &mut u64, peak_abs: &mut f32) {
    if !value.is_finite() {
        return;
    }
    let abs = value.abs();
    *peak_abs = (*peak_abs).max(abs);
    if abs > 1.0 {
        *clipped = clipped.saturating_add(1);
    }
}

impl CpalOutputDiagnosticsCounters {
    fn take_snapshot(&self) -> CpalOutputDiagnostics {
        CpalOutputDiagnostics {
            callback_count: self.callback_count.load(Ordering::Relaxed),
            rendered_frames: self.rendered_frames.load(Ordering::Relaxed),
            stream_error_count: self.stream_error_count.load(Ordering::Relaxed),
            source_lock_miss_count: self.source_lock_miss_count.load(Ordering::Relaxed),
            engine_lock_miss_count: self.engine_lock_miss_count.load(Ordering::Relaxed),
            engine_lock_miss_callback_count: self
                .engine_lock_miss_callback_count
                .load(Ordering::Relaxed),
            system_engine_lock_miss_count: self
                .system_engine_lock_miss_count
                .load(Ordering::Relaxed),
            play_engine_lock_miss_count: self.play_engine_lock_miss_count.load(Ordering::Relaxed),
            draining_engine_lock_miss_count: self
                .draining_engine_lock_miss_count
                .load(Ordering::Relaxed),
            other_engine_lock_miss_count: self.other_engine_lock_miss_count.load(Ordering::Relaxed),
            clipped_sample_count: self.clipped_sample_count.load(Ordering::Relaxed),
            peak_abs: f32::from_bits(self.peak_abs_bits.swap(0, Ordering::Relaxed)),
            max_callback_ns: self.max_callback_ns.swap(0, Ordering::Relaxed),
        }
    }

    fn observe_callback_duration(&self, callback_start: Instant) {
        let elapsed_ns = callback_start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        update_atomic_max(&self.max_callback_ns, elapsed_ns);
    }

    fn observe_output_peak(&self, peak_abs: f32) {
        if peak_abs <= 0.0 || !peak_abs.is_finite() {
            return;
        }
        update_atomic_max(&self.peak_abs_bits, u64::from(peak_abs.to_bits()));
    }

    fn record_engine_lock_miss(&self, source_kind: CpalOutputSourceKind) {
        self.engine_lock_miss_count.fetch_add(1, Ordering::Relaxed);
        match source_kind {
            CpalOutputSourceKind::Other => {
                self.other_engine_lock_miss_count.fetch_add(1, Ordering::Relaxed);
            }
            CpalOutputSourceKind::System => {
                self.system_engine_lock_miss_count.fetch_add(1, Ordering::Relaxed);
            }
            CpalOutputSourceKind::Play => {
                self.play_engine_lock_miss_count.fetch_add(1, Ordering::Relaxed);
            }
            CpalOutputSourceKind::Draining => {
                self.draining_engine_lock_miss_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

fn update_atomic_max<T>(atomic: &T, value: u64)
where
    T: AtomicMaxU64,
{
    let mut current = atomic.load_relaxed();
    while value > current {
        match atomic.compare_exchange_relaxed(current, value) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

trait AtomicMaxU64 {
    fn load_relaxed(&self) -> u64;
    fn compare_exchange_relaxed(&self, current: u64, value: u64) -> Result<u64, u64>;
}

impl AtomicMaxU64 for AtomicU64 {
    fn load_relaxed(&self) -> u64 {
        self.load(Ordering::Relaxed)
    }

    fn compare_exchange_relaxed(&self, current: u64, value: u64) -> Result<u64, u64> {
        self.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed)
    }
}

impl AtomicMaxU64 for AtomicU32 {
    fn load_relaxed(&self) -> u64 {
        u64::from(self.load(Ordering::Relaxed))
    }

    fn compare_exchange_relaxed(&self, current: u64, value: u64) -> Result<u64, u64> {
        let current = current as u32;
        let value = value as u32;
        self.compare_exchange(current, value, Ordering::Relaxed, Ordering::Relaxed)
            .map(u64::from)
            .map_err(u64::from)
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
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        write_interleaved_output(&mut output, 1, 0, &[0.25, 0.75, -0.5, 0.25], &diagnostics);

        assert_eq!(output, vec![0.5, -0.125]);
    }

    #[test]
    fn write_interleaved_output_fills_extra_channels_with_silence() {
        let mut output = vec![1.0_f32; 6];
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        write_interleaved_output(&mut output, 3, 0, &[0.25, 0.75, -0.5, 0.25], &diagnostics);

        assert_eq!(output, vec![0.25, 0.75, 0.0, -0.5, 0.25, 0.0]);
    }

    #[test]
    fn write_interleaved_output_routes_to_selected_channel_pair() {
        // 4ch 出力で 3-4ch(offset 2)へルーティングする。
        let mut output = vec![1.0_f32; 8];
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        write_interleaved_output(&mut output, 4, 2, &[0.25, 0.75, -0.5, 0.25], &diagnostics);

        assert_eq!(output, vec![0.0, 0.0, 0.25, 0.75, 0.0, 0.0, -0.5, 0.25]);
    }

    #[test]
    fn write_interleaved_output_falls_back_when_pair_does_not_fit() {
        // offset がデバイスチャンネル数に収まらない場合は先頭ペアへ。
        let mut output = vec![1.0_f32; 4];
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        write_interleaved_output(&mut output, 2, 5, &[0.25, 0.75, -0.5, 0.25], &diagnostics);

        assert_eq!(output, vec![0.25, 0.75, -0.5, 0.25]);
    }

    #[test]
    fn write_interleaved_output_records_peak_and_clipping() {
        let mut output = vec![0.0_f32; 4];
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        write_interleaved_output(&mut output, 2, 0, &[1.25, -0.5, 0.25, -1.5], &diagnostics);

        let snapshot = diagnostics.take_snapshot();
        assert_eq!(snapshot.clipped_sample_count, 2);
        assert_eq!(snapshot.peak_abs, 1.5);
    }

    #[test]
    fn resolve_channel_offset_clamps_to_last_pair() {
        assert_eq!(resolve_channel_offset(0, 12), 0);
        assert_eq!(resolve_channel_offset(2, 12), 2);
        assert_eq!(resolve_channel_offset(10, 12), 10);
        assert_eq!(resolve_channel_offset(11, 12), 10);
        assert_eq!(resolve_channel_offset(99, 12), 10);
        assert_eq!(resolve_channel_offset(4, 2), 0);
        assert_eq!(resolve_channel_offset(4, 1), 0);
    }

    #[test]
    fn render_output_reuses_scratch_buffer() {
        let engine = Arc::new(Mutex::new(AudioEngine::default()));
        let sources = Arc::new(Mutex::new(vec![AudioSource {
            id: 1,
            kind: CpalOutputSourceKind::Play,
            engine,
        }]));
        let mut output = vec![1.0_f32; 4];
        let mut mix = Vec::with_capacity(16);
        let mut source_scratch = Vec::new();
        let mut source_engines = Vec::new();
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        render_output(
            &mut output,
            2,
            0,
            0,
            &sources,
            &mut mix,
            &mut source_scratch,
            &mut source_engines,
            &diagnostics,
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
            AudioSource { id: 1, kind: CpalOutputSourceKind::System, engine: first },
            AudioSource { id: 2, kind: CpalOutputSourceKind::Play, engine: second },
        ]));
        let mut mix = Vec::new();
        let mut scratch = Vec::new();
        let mut engines = Vec::new();
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        mix_sources_stereo(0, 1, &sources, &mut mix, &mut scratch, &mut engines, &diagnostics);

        assert_eq!(mix, vec![0.75, 0.75]);
    }

    #[test]
    fn mix_sources_stereo_records_engine_lock_miss_by_source_kind() {
        let engine = Arc::new(Mutex::new(AudioEngine::default()));
        let _held = engine.lock().unwrap();
        let sources = Arc::new(Mutex::new(vec![AudioSource {
            id: 1,
            kind: CpalOutputSourceKind::System,
            engine: Arc::clone(&engine),
        }]));
        let mut mix = Vec::new();
        let mut scratch = Vec::new();
        let mut engines = Vec::new();
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        mix_sources_stereo(0, 1, &sources, &mut mix, &mut scratch, &mut engines, &diagnostics);

        let snapshot = diagnostics.take_snapshot();
        assert_eq!(snapshot.engine_lock_miss_count, 1);
        assert_eq!(snapshot.engine_lock_miss_callback_count, 1);
        assert_eq!(snapshot.system_engine_lock_miss_count, 1);
        assert_eq!(snapshot.play_engine_lock_miss_count, 0);
        assert_eq!(snapshot.draining_engine_lock_miss_count, 0);
        assert_eq!(snapshot.other_engine_lock_miss_count, 0);
    }

    #[test]
    fn resolve_buffer_size_uses_default_when_unset() {
        let resolved = resolve_buffer_size(None, &::cpal::SupportedBufferSize::Unknown);
        assert!(matches!(resolved, ::cpal::BufferSize::Default));
    }

    #[test]
    fn resolve_buffer_size_clamps_to_supported_range() {
        let range = ::cpal::SupportedBufferSize::Range { min: 64, max: 1024 };

        assert!(matches!(resolve_buffer_size(Some(32), &range), ::cpal::BufferSize::Fixed(64)));
        assert!(matches!(resolve_buffer_size(Some(256), &range), ::cpal::BufferSize::Fixed(256)));
        assert!(matches!(resolve_buffer_size(Some(4096), &range), ::cpal::BufferSize::Fixed(1024)));
    }

    #[test]
    fn resolve_buffer_size_passes_through_when_range_unknown() {
        assert!(matches!(
            resolve_buffer_size(Some(96), &::cpal::SupportedBufferSize::Unknown),
            ::cpal::BufferSize::Fixed(96)
        ));
    }
}
