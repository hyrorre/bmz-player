use std::time::Duration;

use super::*;

const MIN_PLAYBACK_DISCONTINUITY: Duration = Duration::from_millis(20);

#[derive(Debug, Default)]
struct OutputPlaybackTimeline {
    previous_playback: Option<::cpal::StreamInstant>,
    previous_frames: usize,
}

impl OutputPlaybackTimeline {
    fn catch_up_frames(
        &mut self,
        playback: ::cpal::StreamInstant,
        frames: usize,
        sample_rate: u32,
    ) -> u64 {
        let catch_up = self.previous_playback.and_then(|previous| {
            let actual = playback.checked_duration_since(previous)?;
            let expected = frames_to_duration(self.previous_frames, sample_rate);
            let gap = actual.saturating_sub(expected);
            let tolerance = MIN_PLAYBACK_DISCONTINUITY.max(expected.saturating_mul(2));
            (gap > tolerance).then(|| duration_to_frames(gap, sample_rate))
        });
        self.previous_playback = Some(playback);
        self.previous_frames = frames;
        catch_up.unwrap_or(0)
    }
}

fn frames_to_duration(frames: usize, sample_rate: u32) -> Duration {
    if sample_rate == 0 {
        return Duration::ZERO;
    }
    let nanos = (frames as u128)
        .saturating_mul(1_000_000_000)
        .checked_div(u128::from(sample_rate))
        .unwrap_or(0)
        .min(u128::from(u64::MAX));
    Duration::from_nanos(nanos as u64)
}

fn duration_to_frames(duration: Duration, sample_rate: u32) -> u64 {
    duration
        .as_nanos()
        .saturating_mul(u128::from(sample_rate))
        .checked_div(1_000_000_000)
        .unwrap_or(0)
        .min(u128::from(u64::MAX)) as u64
}

pub(super) fn push_output_command(
    queue: &SharedOutputCommands,
    command: CpalOutputCommand,
) -> bool {
    let Ok(mut queue) = queue.lock() else {
        tracing::warn!("failed to lock cpal output command queue");
        return false;
    };
    if queue.len() >= OUTPUT_COMMAND_QUEUE_CAPACITY {
        tracing::warn!("cpal output command queue is full; dropping source command");
        return false;
    }
    queue.push_back(command);
    true
}

pub(super) fn build_output_stream<T>(
    device: &::cpal::Device,
    config: &StreamConfig,
    channel_offset: usize,
    output_commands: SharedOutputCommands,
    retired_sources: RetiredOutputSources,
    current_frame: Arc<AtomicU64>,
    diagnostics: Arc<CpalOutputDiagnosticsCounters>,
) -> Result<::cpal::Stream, CpalBackendError>
where
    T: ::cpal::SizedSample + OutputSample,
{
    let channels = config.channels as usize;
    // 最短周期の最初の callback で割り当てやゼロ初期化を発生させない。4096 frames
    // あれば設定 UI の最大 fixed buffer と 384 kHz / 約 10 ms までを覆える。
    let mut buffers = OutputRenderBuffers {
        mix: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
        source_scratch: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
        render_sources: Vec::with_capacity(OUTPUT_SOURCE_INITIAL_CAPACITY),
        source_command_scratch: Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY),
    };
    let sample_rate = config.sample_rate;
    let mut playback_timeline = OutputPlaybackTimeline::default();
    let timing_enabled = bmz_core::latency::diagnostics_enabled();
    let mut previous_callback = None;
    let error_diagnostics = Arc::clone(&diagnostics);
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], info| {
                let callback_start = Instant::now();
                diagnostics.callback_count.fetch_add(1, Ordering::Relaxed);
                if channels == 0 {
                    data.fill(T::from_f32(0.0));
                    diagnostics.observe_callback_duration(callback_start);
                    return;
                }

                let frames = data.len() / channels;
                if timing_enabled {
                    diagnostics.timing.observe(
                        frames,
                        callback_start,
                        previous_callback,
                        info.timestamp(),
                    );
                    previous_callback = Some(callback_start);
                }
                let catch_up_frames = playback_timeline.catch_up_frames(
                    info.timestamp().playback,
                    frames,
                    sample_rate,
                );
                let start_frame =
                    current_frame.load(Ordering::Relaxed).saturating_add(catch_up_frames);
                if catch_up_frames != 0 {
                    diagnostics.timeline_catch_up_count.fetch_add(1, Ordering::Relaxed);
                    diagnostics
                        .timeline_catch_up_frames
                        .fetch_add(catch_up_frames, Ordering::Relaxed);
                }
                render_output(
                    data,
                    OutputRenderLayout { channels, channel_offset, start_frame },
                    OutputRenderSources {
                        output_commands: &output_commands,
                        retired_sources: &retired_sources,
                    },
                    &mut buffers,
                    &diagnostics,
                );
                diagnostics.rendered_frames.fetch_add(frames as u64, Ordering::Relaxed);
                current_frame.store(start_frame.saturating_add(frames as u64), Ordering::Relaxed);
                diagnostics.observe_callback_duration(callback_start);
                if timing_enabled {
                    diagnostics
                        .timing
                        .duration_ns
                        .record(callback_start.elapsed().as_nanos().min(u64::MAX as u128) as u64);
                }
            },
            move |error| {
                error_diagnostics.stream_error_count.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(%error, "cpal output stream error");
            },
            None,
        )
        .map_err(CpalBackendError::BuildStream)
}

/// Transport-neutral owner for the real-time mixer state used by native output backends.
///
/// CPAL normally owns this state inside its data callback. WASAPI exclusive mode has to service
/// endpoint buffers itself, so it moves the same state onto its dedicated worker thread.
#[cfg(windows)]
pub(crate) struct NativeOutputRenderer {
    channel_offset: usize,
    output_commands: SharedOutputCommands,
    retired_sources: RetiredOutputSources,
    current_frame: Arc<AtomicU64>,
    diagnostics: Arc<CpalOutputDiagnosticsCounters>,
    buffers: OutputRenderBuffers,
}

#[cfg(windows)]
impl NativeOutputRenderer {
    pub(super) fn new(
        channel_offset: usize,
        output_commands: SharedOutputCommands,
        retired_sources: RetiredOutputSources,
        current_frame: Arc<AtomicU64>,
        diagnostics: Arc<CpalOutputDiagnosticsCounters>,
    ) -> Self {
        Self {
            channel_offset,
            output_commands,
            retired_sources,
            current_frame,
            diagnostics,
            buffers: OutputRenderBuffers {
                mix: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
                source_scratch: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
                render_sources: Vec::with_capacity(OUTPUT_SOURCE_INITIAL_CAPACITY),
                source_command_scratch: Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY),
            },
        }
    }

    pub(crate) fn render<T: OutputSample>(&mut self, data: &mut [T], channels: usize) {
        let callback_start = Instant::now();
        self.diagnostics.callback_count.fetch_add(1, Ordering::Relaxed);
        if channels == 0 {
            data.fill(T::from_f32(0.0));
            self.diagnostics.observe_callback_duration(callback_start);
            return;
        }

        let frames = data.len() / channels;
        let start_frame = self.current_frame.load(Ordering::Relaxed);
        render_output(
            data,
            OutputRenderLayout { channels, channel_offset: self.channel_offset, start_frame },
            OutputRenderSources {
                output_commands: &self.output_commands,
                retired_sources: &self.retired_sources,
            },
            &mut self.buffers,
            &self.diagnostics,
        );
        self.diagnostics.rendered_frames.fetch_add(frames as u64, Ordering::Relaxed);
        self.current_frame.store(start_frame.saturating_add(frames as u64), Ordering::Relaxed);
        self.diagnostics.observe_callback_duration(callback_start);
    }

    pub(crate) fn record_stream_error(&self) {
        self.diagnostics.stream_error_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Advance the mixer timeline across a native-stream outage without rendering the missed
    /// interval. Gameplay keeps using its monotonic clock while the endpoint is unavailable, so
    /// resuming from the old frame would permanently delay every subsequently scheduled sound.
    pub(crate) fn catch_up_after_stream_outage(&self, outage: Duration, sample_rate: u32) -> u64 {
        let frames = duration_to_frames(outage, sample_rate);
        if frames == 0 {
            return 0;
        }
        self.current_frame.fetch_add(frames, Ordering::Relaxed);
        self.diagnostics.timeline_catch_up_count.fetch_add(1, Ordering::Relaxed);
        self.diagnostics.timeline_catch_up_frames.fetch_add(frames, Ordering::Relaxed);
        frames
    }
}

pub(super) fn device_name(device: &::cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| device.to_string())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct OutputRenderLayout {
    pub(super) channels: usize,
    pub(super) channel_offset: usize,
    pub(super) start_frame: u64,
}

pub(super) struct OutputRenderSources<'a> {
    pub(super) output_commands: &'a SharedOutputCommands,
    pub(super) retired_sources: &'a RetiredOutputSources,
}

pub(super) struct OutputRenderBuffers {
    pub(super) mix: Vec<f32>,
    pub(super) source_scratch: Vec<f32>,
    pub(super) render_sources: Vec<RenderAudioSource>,
    pub(super) source_command_scratch: Vec<CpalOutputCommand>,
}

pub(super) fn render_output<T: OutputSample>(
    data: &mut [T],
    layout: OutputRenderLayout,
    sources: OutputRenderSources<'_>,
    buffers: &mut OutputRenderBuffers,
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    if layout.channels == 0 {
        return;
    }

    let frames = data.len() / layout.channels;
    mix_sources_stereo(
        layout.start_frame,
        frames,
        sources.output_commands,
        sources.retired_sources,
        &mut buffers.mix,
        &mut buffers.source_scratch,
        &mut buffers.render_sources,
        &mut buffers.source_command_scratch,
        diagnostics,
    );

    write_interleaved_output(
        data,
        layout.channels,
        layout.channel_offset,
        &buffers.mix,
        diagnostics,
    );
}

pub(super) fn mix_sources_stereo(
    output_start_frame: u64,
    frames: usize,
    output_commands: &SharedOutputCommands,
    retired_sources: &RetiredOutputSources,
    mix: &mut Vec<f32>,
    source_scratch: &mut Vec<f32>,
    render_sources: &mut Vec<RenderAudioSource>,
    source_command_scratch: &mut Vec<CpalOutputCommand>,
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    mix.resize(frames * 2, 0.0);
    mix.fill(0.0);

    drain_output_commands(
        output_commands,
        retired_sources,
        render_sources,
        source_command_scratch,
        diagnostics,
    );

    source_scratch.resize(frames * 2, 0.0);
    let mut missed_engine_lock = false;
    for source in render_sources.iter_mut().filter(|source| source.active) {
        let rendered = match &mut source.engine {
            RenderAudioEngine::Legacy(engine) => match engine.try_lock() {
                Ok(mut engine) => {
                    engine.render_stereo(output_start_frame, source_scratch);
                    true
                }
                Err(_) => false,
            },
            RenderAudioEngine::Commanded(engine) => {
                engine.render_stereo(output_start_frame, source_scratch)
            }
        };
        if rendered {
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

pub(super) fn drain_output_commands(
    output_commands: &SharedOutputCommands,
    retired_sources: &RetiredOutputSources,
    render_sources: &mut Vec<RenderAudioSource>,
    scratch: &mut Vec<CpalOutputCommand>,
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    scratch.clear();
    match output_commands.try_lock() {
        Ok(mut commands) => {
            scratch.reserve(commands.len());
            while let Some(command) = commands.pop_front() {
                scratch.push(command);
            }
        }
        Err(TryLockError::WouldBlock) => {
            diagnostics.source_lock_miss_count.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(TryLockError::Poisoned(_)) => {
            diagnostics.source_lock_miss_count.fetch_add(1, Ordering::Relaxed);
            return;
        }
    }

    for command in scratch.drain(..) {
        match command {
            CpalOutputCommand::AddLegacySource { id, kind, engine } => {
                render_sources.push(RenderAudioSource {
                    id,
                    kind,
                    active: true,
                    engine: RenderAudioEngine::Legacy(engine),
                });
            }
            CpalOutputCommand::AddCommandedSource { id, kind, engine } => {
                render_sources.push(RenderAudioSource {
                    id,
                    kind,
                    active: true,
                    engine: RenderAudioEngine::Commanded(engine),
                });
            }
            CpalOutputCommand::RemoveSource { id } => {
                if let Some(source) = render_sources.iter_mut().find(|source| source.id == id) {
                    // `retain` でここから drop すると、最後の参照になった chart
                    // sample bank の解放が callback の実行期限を超え得る。無音化だけ
                    // 先に行い、app thread 側へ移すまでは source を保持する。
                    source.active = false;
                }
            }
            CpalOutputCommand::SetSourceKind { id, kind } => {
                if let Some(source) = render_sources.iter_mut().find(|source| source.id == id) {
                    source.kind = kind;
                }
            }
        }
    }

    retire_inactive_sources(render_sources, retired_sources);
}

/// callback 中には lock 待ちも追加確保もせず、無音化済み source を退避する。
/// 退避先が一杯または app thread が回収中なら、次 callback まで source を保持する。
pub(super) fn retire_inactive_sources(
    render_sources: &mut Vec<RenderAudioSource>,
    retired_sources: &RetiredOutputSources,
) {
    let Ok(mut retired_sources) = retired_sources.try_lock() else {
        return;
    };
    if retired_sources.len() >= retired_sources.capacity() {
        return;
    }

    let mut index = 0;
    while index < render_sources.len() && retired_sources.len() < retired_sources.capacity() {
        if render_sources[index].active {
            index += 1;
        } else {
            // capacity は確認済みなので `push` は allocate しない。`swap_remove` の
            // 所有権を app thread の回収用 Vec へ move し、callback では drop しない。
            retired_sources.push(render_sources.swap_remove(index));
        }
    }
}

pub(super) fn write_interleaved_output<T: OutputSample>(
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
            frame[0] = T::from_f32(safe_output_sample(mono));
            continue;
        }
        // 対象ペア以外は無音にして、選択チャンネルへ L/R を書く。
        for sample in frame.iter_mut() {
            *sample = silence;
        }
        observe_output_sample(left, &mut clipped, &mut peak_abs);
        observe_output_sample(right, &mut clipped, &mut peak_abs);
        frame[left_channel] = T::from_f32(safe_output_sample(left));
        frame[left_channel + 1] = T::from_f32(safe_output_sample(right));
    }
    diagnostics.observe_output_peak(peak_abs);
    if clipped != 0 {
        diagnostics.clipped_sample_count.fetch_add(clipped, Ordering::Relaxed);
    }
}

fn safe_output_sample(value: f32) -> f32 {
    // Device boundary only: preserve float headroom throughout decode/analysis/mix.
    if value.is_finite() { value.clamp(-1.0, 1.0) } else { 0.0 }
}

pub(super) fn observe_output_sample(value: f32, clipped: &mut u64, peak_abs: &mut f32) {
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
    pub(super) fn take_snapshot(&self) -> CpalOutputDiagnostics {
        CpalOutputDiagnostics {
            timing: self.timing.summary(),
            callback_count: self.callback_count.load(Ordering::Relaxed),
            rendered_frames: self.rendered_frames.load(Ordering::Relaxed),
            timeline_catch_up_count: self.timeline_catch_up_count.load(Ordering::Relaxed),
            timeline_catch_up_frames: self.timeline_catch_up_frames.load(Ordering::Relaxed),
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

pub(super) fn update_atomic_max<T>(atomic: &T, value: u64)
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

pub(super) trait AtomicMaxU64 {
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

pub(crate) trait OutputSample: Copy {
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
mod playback_timeline_tests {
    use super::*;

    const SAMPLE_RATE: u32 = 48_000;
    const BUFFER_FRAMES: usize = 480;

    #[test]
    fn callback_jitter_does_not_trigger_timeline_catch_up() {
        let mut timeline = OutputPlaybackTimeline::default();
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_000),
                BUFFER_FRAMES,
                SAMPLE_RATE,
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_029),
                BUFFER_FRAMES * 2,
                SAMPLE_RATE,
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_049),
                BUFFER_FRAMES,
                SAMPLE_RATE,
            ),
            0
        );
    }

    #[test]
    fn confirmed_playback_gap_advances_the_logical_output_frame() {
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
        );

        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_110),
                BUFFER_FRAMES,
                SAMPLE_RATE,
            ),
            4_800
        );
    }

    #[test]
    fn invalid_backward_timestamp_resets_without_correction() {
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
        );

        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(900),
                BUFFER_FRAMES,
                SAMPLE_RATE,
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(910),
                BUFFER_FRAMES,
                SAMPLE_RATE,
            ),
            0
        );
    }
}
