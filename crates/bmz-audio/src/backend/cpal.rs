use std::collections::VecDeque;
use std::rc::{Rc, Weak as RcWeak};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::Instant;

use ::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ::cpal::{SampleFormat, StreamConfig};
use bmz_core::time::TimeUs;
use thiserror::Error;

#[cfg(windows)]
use crate::backend::wasapi::{WasapiExclusiveOutput, WasapiSharedPeriodGuard};
use crate::clock::AudioClock;
use crate::command::{AudioEngineHandle, CommandedAudioEngine};
use crate::engine::AudioEngine;

pub(crate) mod callback;
#[cfg(target_os = "macos")]
mod coreaudio_compat;
mod device;
#[cfg(all(target_os = "macos", feature = "experimental-coreaudio-ioproc"))]
mod ioproc;
mod source;
mod telemetry;
pub use telemetry::OutputTimingSummary;

use callback::*;
pub use device::{is_host_supported, list_output_device_names};
#[cfg(test)]
use device::{resolve_buffer_size, resolve_channel_offset};

pub type SharedAudioEngine = Arc<Mutex<AudioEngine>>;
type SharedOutputCommands = Arc<Mutex<VecDeque<CpalOutputCommand>>>;
type RetiredOutputSources = Arc<Mutex<Vec<RenderAudioSource>>>;
const OUTPUT_COMMAND_QUEUE_CAPACITY: usize = 256;
const OUTPUT_SCRATCH_INITIAL_FRAMES: usize = 4096;
const OUTPUT_SOURCE_INITIAL_CAPACITY: usize = 8;

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
    /// Windows の WASAPI 共有モードで `IAudioClient3` の低遅延エンジン周期を要求する。
    /// 非対応 OS / ホスト / デバイスでは通常の CPAL 共有モードへフォールバックする。
    pub low_latency_shared: bool,
    /// Windows の WASAPI endpoint を event-driven 排他モードで開く。
    /// 排他取得に失敗した場合は共有モードへ暗黙にフォールバックしない。
    pub exclusive: bool,
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
    CoreAudioIoProc,
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
    pub timing: OutputTimingSummary,
    pub callback_count: u64,
    pub rendered_frames: u64,
    pub timeline_catch_up_count: u64,
    pub timeline_catch_up_frames: u64,
    pub stream_error_count: u64,
    pub processor_overload_count: u64,
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
    timing: telemetry::OutputTiming,
    callback_count: AtomicU64,
    rendered_frames: AtomicU64,
    timeline_catch_up_count: AtomicU64,
    timeline_catch_up_frames: AtomicU64,
    stream_error_count: AtomicU64,
    processor_overload_count: AtomicU64,
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

#[derive(Debug, Clone)]
pub struct CpalStreamInfo {
    pub stream_id: u64,
    pub requested_host: Option<CpalHostId>,
    pub requested_device: Option<String>,
    pub actual_host: String,
    pub actual_device: String,
    pub requested_rate: Option<u32>,
    pub actual_rate: u32,
    pub requested_frames: Option<u32>,
    pub supported_frames: String,
    pub cpal_buffer: String,
}

struct CpalSharedOutputInner {
    info: CpalStreamInfo,
    stream: CpalOutputStream,
    // Audible CPAL stream must be dropped before releasing the shared engine-period request.
    #[cfg(windows)]
    _low_latency_guard: Option<WasapiSharedPeriodGuard>,
    host_id: ::cpal::HostId,
    sample_rate: u32,
    current_frame: Arc<AtomicU64>,
    output_commands: SharedOutputCommands,
    retired_sources: RetiredOutputSources,
    diagnostics: Arc<CpalOutputDiagnosticsCounters>,
    next_source_id: AtomicU64,
}

enum CpalOutputStream {
    Cpal(::cpal::Stream),
    #[cfg(all(target_os = "macos", feature = "experimental-coreaudio-ioproc"))]
    CoreAudioIoProc(ioproc::IoProcOutput),
    #[cfg(windows)]
    WasapiExclusive(WasapiExclusiveOutput),
}

pub struct CpalOutputSource {
    id: u64,
    inner: RcWeak<CpalSharedOutputInner>,
    kind: CpalOutputSourceKind,
    pub engine: SharedAudioEngine,
    pub clock: AudioClock,
}

pub struct CpalCommandedOutputSource {
    id: u64,
    inner: RcWeak<CpalSharedOutputInner>,
    kind: CpalOutputSourceKind,
    handle: AudioEngineHandle,
    pub clock: AudioClock,
}

struct RenderAudioSource {
    id: u64,
    kind: CpalOutputSourceKind,
    active: bool,
    engine: RenderAudioEngine,
}

enum RenderAudioEngine {
    Legacy(SharedAudioEngine),
    Commanded(CommandedAudioEngine),
}

enum CpalOutputCommand {
    AddLegacySource { id: u64, kind: CpalOutputSourceKind, engine: SharedAudioEngine },
    AddCommandedSource { id: u64, kind: CpalOutputSourceKind, engine: CommandedAudioEngine },
    RemoveSource { id: u64 },
    SetSourceKind { id: u64, kind: CpalOutputSourceKind },
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

    #[error("WASAPI exclusive output requires the WASAPI host")]
    ExclusiveRequiresWasapi,

    #[error("failed to open WASAPI exclusive output: {0}")]
    WasapiExclusive(String),

    #[error("Core Audio IOProc output failed: {0}")]
    CoreAudioIoProc(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_boundary_removes_non_finite_and_clips_extreme_pcm() {
        let input = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 6405.997, -6405.997, 0.25];
        let mut output = [0.0f32; 6];
        write_interleaved_output(
            &mut output,
            2,
            0,
            &input,
            &CpalOutputDiagnosticsCounters::default(),
        );
        assert_eq!(output, [0.0, 0.0, 0.0, 1.0, -1.0, 0.25]);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires a Windows 10+ output device with IAudioClient3 low-period support"]
    fn opens_default_wasapi_low_latency_shared_stream() {
        let output = CpalBackend::open_shared(CpalOutputConfig {
            host: Some(CpalHostId::Wasapi),
            low_latency_shared: true,
            ..Default::default()
        })
        .unwrap();

        assert!(output.low_latency_shared_period_frames().is_some());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires an available Windows output device with exclusive-mode permission"]
    fn opens_and_starts_default_wasapi_exclusive_stream() {
        let output = CpalBackend::open_shared(CpalOutputConfig {
            host: Some(CpalHostId::Wasapi),
            exclusive: true,
            ..Default::default()
        })
        .unwrap();

        assert!(output.exclusive_buffer_frames().is_some());
        output.play().unwrap();

        for _ in 0..50 {
            if output.take_diagnostics().callback_count > 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        panic!("WASAPI exclusive stream did not start rendering");
    }

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
        let output_commands = test_output_commands([CpalOutputCommand::AddLegacySource {
            id: 1,
            kind: CpalOutputSourceKind::Play,
            engine,
        }]);
        let mut output = vec![1.0_f32; 4];
        let mut buffers = OutputRenderBuffers {
            mix: Vec::with_capacity(16),
            source_scratch: Vec::new(),
            render_sources: Vec::new(),
            source_command_scratch: Vec::new(),
        };
        let retired_sources = test_retired_sources(1);
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        render_output(
            &mut output,
            OutputRenderLayout { channels: 2, channel_offset: 0, start_frame: 0 },
            OutputRenderSources {
                output_commands: &output_commands,
                retired_sources: &retired_sources,
            },
            &mut buffers,
            &diagnostics,
        );

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0]);
        assert_eq!(buffers.mix.len(), 4);
        assert!(buffers.mix.capacity() >= 16);
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
        let output_commands = test_output_commands([
            CpalOutputCommand::AddLegacySource {
                id: 1,
                kind: CpalOutputSourceKind::System,
                engine: first,
            },
            CpalOutputCommand::AddLegacySource {
                id: 2,
                kind: CpalOutputSourceKind::Play,
                engine: second,
            },
        ]);
        let mut mix = Vec::new();
        let mut scratch = Vec::new();
        let mut sources = Vec::new();
        let mut commands = Vec::new();
        let retired_sources = test_retired_sources(2);
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        mix_sources_stereo(
            0,
            1,
            &output_commands,
            &retired_sources,
            &mut mix,
            &mut scratch,
            &mut sources,
            &mut commands,
            &diagnostics,
        );

        assert_eq!(mix, vec![0.75, 0.75]);
    }

    #[test]
    fn mix_sources_stereo_records_engine_lock_miss_by_source_kind() {
        let engine = Arc::new(Mutex::new(AudioEngine::default()));
        let _held = engine.lock().unwrap();
        let output_commands = test_output_commands([CpalOutputCommand::AddLegacySource {
            id: 1,
            kind: CpalOutputSourceKind::System,
            engine: Arc::clone(&engine),
        }]);
        let mut mix = Vec::new();
        let mut scratch = Vec::new();
        let mut sources = Vec::new();
        let mut commands = Vec::new();
        let retired_sources = test_retired_sources(1);
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        mix_sources_stereo(
            0,
            1,
            &output_commands,
            &retired_sources,
            &mut mix,
            &mut scratch,
            &mut sources,
            &mut commands,
            &diagnostics,
        );

        let snapshot = diagnostics.take_snapshot();
        assert_eq!(snapshot.engine_lock_miss_count, 1);
        assert_eq!(snapshot.engine_lock_miss_callback_count, 1);
        assert_eq!(snapshot.system_engine_lock_miss_count, 1);
        assert_eq!(snapshot.play_engine_lock_miss_count, 0);
        assert_eq!(snapshot.draining_engine_lock_miss_count, 0);
        assert_eq!(snapshot.other_engine_lock_miss_count, 0);
    }

    fn test_output_commands(
        commands: impl IntoIterator<Item = CpalOutputCommand>,
    ) -> SharedOutputCommands {
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut queue_guard = queue.lock().unwrap();
            queue_guard.extend(commands);
        }
        queue
    }

    fn test_retired_sources(capacity: usize) -> RetiredOutputSources {
        Arc::new(Mutex::new(Vec::with_capacity(capacity)))
    }

    #[test]
    fn remove_source_defers_drop_to_app_thread_reaper() {
        let handle = AudioEngineHandle::new(AudioEngine::default());
        let output_commands = test_output_commands([
            CpalOutputCommand::AddCommandedSource {
                id: 1,
                kind: CpalOutputSourceKind::Play,
                engine: handle.processor(),
            },
            CpalOutputCommand::RemoveSource { id: 1 },
        ]);
        let retired_sources = test_retired_sources(1);
        let mut sources = Vec::new();
        let mut scratch = Vec::new();
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        drain_output_commands(
            &output_commands,
            &retired_sources,
            &mut sources,
            &mut scratch,
            &diagnostics,
        );

        assert!(sources.is_empty(), "removed source should not remain in the callback mixer");
        assert_eq!(retired_sources.lock().unwrap().len(), 1);
    }

    #[test]
    fn remove_source_stays_silent_until_reaper_has_capacity() {
        let handle = AudioEngineHandle::new(AudioEngine::default());
        let output_commands = test_output_commands([
            CpalOutputCommand::AddCommandedSource {
                id: 1,
                kind: CpalOutputSourceKind::Play,
                engine: handle.processor(),
            },
            CpalOutputCommand::RemoveSource { id: 1 },
        ]);
        let retired_sources = test_retired_sources(0);
        let mut sources = Vec::new();
        let mut scratch = Vec::new();
        let diagnostics = CpalOutputDiagnosticsCounters::default();

        drain_output_commands(
            &output_commands,
            &retired_sources,
            &mut sources,
            &mut scratch,
            &diagnostics,
        );

        assert_eq!(sources.len(), 1);
        assert!(!sources[0].active);
        assert!(retired_sources.lock().unwrap().is_empty());
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
