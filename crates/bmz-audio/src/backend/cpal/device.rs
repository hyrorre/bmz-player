use super::*;

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
        static NEXT_STREAM: AtomicU64 = AtomicU64::new(1);
        let stream_id = NEXT_STREAM.fetch_add(1, Ordering::Relaxed);
        let requested_host = config.host;
        let requested_device = config.output_device_name.clone();
        #[cfg(all(target_os = "macos", feature = "experimental-coreaudio-ioproc"))]
        if requested_host == Some(CpalHostId::CoreAudioIoProc) {
            return ioproc::open_shared(config, stream_id);
        }
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
        let requested_low_latency_shared = config.low_latency_shared;
        let requested_exclusive = config.exclusive;
        let requested_channel_offset = config.channel_offset;

        #[cfg(not(windows))]
        if requested_exclusive {
            return Err(CpalBackendError::ExclusiveRequiresWasapi);
        }

        #[cfg(windows)]
        if requested_exclusive {
            if host.id() != ::cpal::HostId::Wasapi {
                return Err(CpalBackendError::ExclusiveRequiresWasapi);
            }
            let endpoint_id = device
                .id()
                .map_err(|error| CpalBackendError::WasapiExclusive(error.to_string()))?
                .id()
                .to_owned();
            let current_frame = Arc::new(AtomicU64::new(0));
            let output_commands = Arc::new(Mutex::new(VecDeque::new()));
            let retired_sources =
                Arc::new(Mutex::new(Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY)));
            let diagnostics = Arc::new(CpalOutputDiagnosticsCounters::default());
            let renderer = NativeOutputRenderer::new(
                requested_channel_offset as usize,
                Arc::clone(&output_commands),
                Arc::clone(&retired_sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            );
            let stream = WasapiExclusiveOutput::open(
                endpoint_id.clone(),
                requested_sample_rate,
                requested_buffer_size,
                renderer,
            )
            .map_err(|error| CpalBackendError::WasapiExclusive(error.to_string()))?;
            let info = stream.info();
            tracing::info!(
                endpoint_id = %endpoint_id,
                device = %device_name(&device),
                requested_sample_rate = ?requested_sample_rate,
                sample_rate = info.sample_rate,
                sample_format = info.sample_format,
                channels = info.channels,
                requested_buffer_size = ?requested_buffer_size,
                buffer_frames = info.buffer_frames,
                default_period_100ns = info.default_period_100ns,
                minimum_period_100ns = info.minimum_period_100ns,
                period_100ns = info.period_100ns,
                "opened event-driven WASAPI exclusive output",
            );
            return Ok(CpalSharedOutput {
                inner: Rc::new(CpalSharedOutputInner {
                    info: CpalStreamInfo {
                        stream_id,
                        requested_host,
                        requested_device,
                        actual_host: format!("{:?}", host.id()),
                        actual_device: device_name(&device),
                        requested_rate: requested_sample_rate,
                        actual_rate: info.sample_rate,
                        requested_frames: requested_buffer_size,
                        supported_frames: "native WASAPI".into(),
                        cpal_buffer: format!("native {}", info.buffer_frames),
                    },
                    stream: CpalOutputStream::WasapiExclusive(stream),
                    _low_latency_guard: None,
                    host_id: host.id(),
                    sample_rate: info.sample_rate,
                    current_frame,
                    output_commands,
                    retired_sources,
                    diagnostics,
                    next_source_id: AtomicU64::new(1),
                }),
            });
        }

        let supported_config =
            device.default_output_config().map_err(CpalBackendError::DefaultOutputConfig)?;
        let sample_format = supported_config.sample_format();
        let default_sample_rate = supported_config.sample_rate();
        let supported_buffer_size = *supported_config.buffer_size();
        let mut config = supported_config.config();
        let sample_rate =
            resolve_sample_rate(&device, requested_sample_rate, default_sample_rate, sample_format);
        config.sample_rate = sample_rate;
        #[cfg(windows)]
        let mut resolved_buffer_size =
            resolve_buffer_size(requested_buffer_size, &supported_buffer_size);
        #[cfg(not(windows))]
        let resolved_buffer_size =
            resolve_buffer_size(requested_buffer_size, &supported_buffer_size);
        #[cfg(windows)]
        let mut low_latency_guard = if requested_low_latency_shared {
            open_wasapi_shared_period_guard(&host, &device, sample_rate, requested_buffer_size)
        } else {
            None
        };
        #[cfg(windows)]
        if let Some(guard) = low_latency_guard.as_ref() {
            resolved_buffer_size = ::cpal::BufferSize::Fixed(guard.info().client_period_frames);
        }
        #[cfg(not(windows))]
        if requested_low_latency_shared {
            tracing::warn!(
                "WASAPI low-latency shared mode is only available on Windows; using standard shared output"
            );
        }
        config.buffer_size = resolved_buffer_size;
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
            requested_low_latency_shared,
            resolved_buffer_size = ?config.buffer_size,
            requested_channel_offset,
            channel_offset,
            "opening cpal output stream",
        );

        let current_frame = Arc::new(AtomicU64::new(0));
        let output_commands = Arc::new(Mutex::new(VecDeque::new()));
        // callback 側で source を除去すると最後の Arc になった AudioEngine の
        // SampleBank を解放し得る。音声 callback でそのような重い drop をしない
        // よう、容量を先に確保した退避先へ移し、app thread 側で回収する。
        let retired_sources =
            Arc::new(Mutex::new(Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY)));
        let diagnostics = Arc::new(CpalOutputDiagnosticsCounters::default());

        let build_stream = |config: &StreamConfig| match sample_format {
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                config,
                channel_offset,
                Arc::clone(&output_commands),
                Arc::clone(&retired_sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            ),
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                config,
                channel_offset,
                Arc::clone(&output_commands),
                Arc::clone(&retired_sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            ),
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                config,
                channel_offset,
                Arc::clone(&output_commands),
                Arc::clone(&retired_sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            ),
            SampleFormat::I32 => build_output_stream::<i32>(
                &device,
                config,
                channel_offset,
                Arc::clone(&output_commands),
                Arc::clone(&retired_sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            ),
            _ => build_output_stream::<f32>(
                &device,
                config,
                channel_offset,
                Arc::clone(&output_commands),
                Arc::clone(&retired_sources),
                Arc::clone(&current_frame),
                Arc::clone(&diagnostics),
            ),
        };
        let stream_result = build_stream(&config);
        #[cfg(windows)]
        let stream = match stream_result {
            Ok(stream) => stream,
            Err(error) if low_latency_guard.is_some() => {
                // Some drivers accept the IAudioClient3 period stream but reject the matching
                // legacy IAudioClient ring-buffer duration used by CPAL. Release the period
                // request before retrying the exact standard shared-mode configuration.
                drop(low_latency_guard.take());
                config.buffer_size =
                    resolve_buffer_size(requested_buffer_size, &supported_buffer_size);
                tracing::warn!(
                    %error,
                    fallback_buffer_size = ?config.buffer_size,
                    "failed to build the CPAL stream at the low-latency period; retrying standard shared output",
                );
                build_stream(&config)?
            }
            Err(error) => return Err(error),
        };
        #[cfg(not(windows))]
        let stream = stream_result?;

        Ok(CpalSharedOutput {
            inner: Rc::new(CpalSharedOutputInner {
                info: CpalStreamInfo {
                    stream_id,
                    requested_host,
                    requested_device,
                    actual_host: format!("{:?}", host.id()),
                    actual_device: device_name,
                    requested_rate: requested_sample_rate,
                    actual_rate: sample_rate,
                    requested_frames: requested_buffer_size,
                    supported_frames: format!("{supported_buffer_size:?}"),
                    cpal_buffer: format!("{:?}", config.buffer_size),
                },
                stream: CpalOutputStream::Cpal(stream),
                #[cfg(windows)]
                _low_latency_guard: low_latency_guard,
                host_id: host.id(),
                sample_rate,
                current_frame,
                output_commands,
                retired_sources,
                diagnostics,
                next_source_id: AtomicU64::new(1),
            }),
        })
    }
}

#[cfg(windows)]
pub(super) fn open_wasapi_shared_period_guard(
    host: &::cpal::Host,
    device: &::cpal::Device,
    sample_rate: u32,
    requested_buffer_size: Option<u32>,
) -> Option<WasapiSharedPeriodGuard> {
    if host.id() != ::cpal::HostId::Wasapi {
        tracing::warn!(
            host = ?host.id(),
            "WASAPI low-latency shared mode was requested for another host; using standard shared output",
        );
        return None;
    }

    let endpoint_id = match device.id() {
        Ok(id) => id.id().to_owned(),
        Err(error) => {
            tracing::warn!(
                %error,
                "failed to identify WASAPI endpoint for low-latency shared mode; using standard shared output",
            );
            return None;
        }
    };
    match WasapiSharedPeriodGuard::open(endpoint_id.clone(), sample_rate, requested_buffer_size) {
        Ok(guard) => {
            let info = guard.info();
            tracing::info!(
                endpoint_id = %endpoint_id,
                client_sample_rate = sample_rate,
                queried_engine_sample_rate = info.queried_engine_sample_rate,
                current_engine_sample_rate = info.current_engine_sample_rate,
                default_period_frames = info.default_period_frames,
                fundamental_period_frames = info.fundamental_period_frames,
                min_period_frames = info.min_period_frames,
                max_period_frames = info.max_period_frames,
                selected_period_frames = info.selected_period_frames,
                current_period_frames = info.current_period_frames,
                client_period_frames = info.client_period_frames,
                period_stream_buffer_frames = info.buffer_frames,
                "enabled IAudioClient3 low-latency shared mode",
            );
            Some(guard)
        }
        Err(error) => {
            tracing::warn!(
                %error,
                endpoint_id = %endpoint_id,
                "failed to enable IAudioClient3 low-latency shared mode; using standard shared output",
            );
            None
        }
    }
}

pub(super) fn cpal_host_id(host: CpalHostId) -> Option<::cpal::HostId> {
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

        #[cfg(all(target_os = "macos", feature = "experimental-coreaudio-ioproc"))]
        CpalHostId::CoreAudioIoProc => Some(::cpal::HostId::CoreAudio),
        #[cfg(not(all(target_os = "macos", feature = "experimental-coreaudio-ioproc")))]
        CpalHostId::CoreAudioIoProc => None,

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
pub(super) fn resolve_sample_rate(
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
pub(super) fn resolve_buffer_size(
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
pub(super) fn resolve_channel_offset(requested: u32, channels: usize) -> usize {
    if channels < 2 {
        return 0;
    }
    // ステレオペアが収まる最大の先頭インデックス。
    let max_offset = channels - 2;
    (requested as usize).min(max_offset)
}

pub(super) fn output_device(
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
