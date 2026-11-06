//! Shared real-time output state for CPAL callbacks and native audio backends.
pub(super) use crate::clock::{OutputScheduleTiming, duration_to_frames, output_gap_tolerance};
use crate::command::CommandedAudioEngine;
use crate::engine::AudioEngine;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

mod telemetry;
pub use telemetry::OutputTimingSummary;

pub type SharedAudioEngine = Arc<Mutex<AudioEngine>>;
pub(super) type SharedOutputCommands = Arc<Mutex<VecDeque<CpalOutputCommand>>>;
pub(super) type RetiredOutputSources = Arc<Mutex<Vec<RenderAudioSource>>>;
pub(super) const OUTPUT_COMMAND_QUEUE_CAPACITY: usize = 256;
pub(super) const OUTPUT_SCRATCH_INITIAL_FRAMES: usize = 4096;
pub(super) const OUTPUT_SOURCE_INITIAL_CAPACITY: usize = 8;

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
    /// WASAPI fills using the callback fallback after rejecting the position epoch.
    pub untrusted_position_count: u64,
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
pub(super) struct CpalOutputDiagnosticsCounters {
    pub(super) timing: telemetry::OutputTiming,
    pub(super) callback_count: AtomicU64,
    pub(super) rendered_frames: AtomicU64,
    pub(super) timeline_catch_up_count: AtomicU64,
    pub(super) timeline_catch_up_frames: AtomicU64,
    pub(super) untrusted_position_count: AtomicU64,
    pub(super) stream_error_count: AtomicU64,
    pub(super) processor_overload_count: AtomicU64,
    pub(super) source_lock_miss_count: AtomicU64,
    pub(super) engine_lock_miss_count: AtomicU64,
    pub(super) engine_lock_miss_callback_count: AtomicU64,
    pub(super) system_engine_lock_miss_count: AtomicU64,
    pub(super) play_engine_lock_miss_count: AtomicU64,
    pub(super) draining_engine_lock_miss_count: AtomicU64,
    pub(super) other_engine_lock_miss_count: AtomicU64,
    pub(super) clipped_sample_count: AtomicU64,
    pub(super) peak_abs_bits: AtomicU32,
    pub(super) max_callback_ns: AtomicU64,
}

pub(super) struct RenderAudioSource {
    pub(super) id: u64,
    pub(super) kind: CpalOutputSourceKind,
    pub(super) active: bool,
    pub(super) engine: RenderAudioEngine,
}

pub(super) enum RenderAudioEngine {
    Legacy(SharedAudioEngine),
    Commanded(CommandedAudioEngine),
}

pub(super) enum CpalOutputCommand {
    AddLegacySource { id: u64, kind: CpalOutputSourceKind, engine: SharedAudioEngine },
    AddCommandedSource { id: u64, kind: CpalOutputSourceKind, engine: CommandedAudioEngine },
    RemoveSource { id: u64 },
    SetSourceKind { id: u64, kind: CpalOutputSourceKind },
}

pub(super) fn frames_to_duration(frames: usize, sample_rate: u32) -> Duration {
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

/// Long enough to ignore callback delivery jitter, short enough to follow a real change.
const OUTPUT_LEAD_SMOOTHING_SECS: f64 = 2.0;

/// Detect scheduling stalls from callback observations, never from corrected
/// buffer times. A larger delivered buffer can cover a longer callback interval.
/// This is a deadline resync signal, not evidence of missing physical PCM.
#[derive(Debug, Default)]
pub(super) struct OutputScheduleTimeline {
    previous: Option<(Instant, Duration)>,
    epoch: u64,
}

impl OutputScheduleTimeline {
    pub(super) fn observe(&mut self, callback: Instant, span: Duration) -> bool {
        let stopped = self.previous.is_some_and(|(previous, previous_span)| {
            let coverage = previous_span.max(span);
            callback.saturating_duration_since(previous)
                > coverage.saturating_add(output_gap_tolerance(coverage))
        });
        self.previous = Some((callback, span));
        if stopped {
            self.mark_outage();
        }
        stopped
    }

    pub(super) fn mark_outage(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub(super) fn timing(&self, time: Instant) -> OutputScheduleTiming {
        OutputScheduleTiming { time, outage_epoch: self.epoch }
    }
}

/// Gives scheduled sounds the average output delay of immediately requested ones.
///
/// An immediate request waits about half a buffer for the next callback, then
/// for the output lead. Aiming at the predicted output time alone would put
/// BGM ahead of key sounds and guide SEs by that whole delay.
#[derive(Debug, Default)]
pub(super) struct ScheduledOutputPhase {
    // This is a clock correspondence, not a physical latency. It may be negative.
    offset_ns: Option<f64>,
    samples: u32,
    previous: Option<(Instant, Instant, Duration)>,
}

impl ScheduledOutputPhase {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns the time a buffer's first frame stands for on the deadline timeline.
    ///
    /// The prediction supplies the low jitter and callback arrival the long-term
    /// reference, so an offset or a device-rate API clock cannot accumulate.
    pub(super) fn buffer_time(
        &mut self,
        playback: Option<Instant>,
        callback: Instant,
        span: Duration,
    ) -> Instant {
        let Some(playback) = playback else {
            // Unavailable predictions must not reuse an old offset. A valid zero
            // or negative difference, on the other hand, participates in the filter.
            self.reset();
            return callback.checked_sub(span / 2).unwrap_or(callback);
        };
        if let Some((previous_playback, previous_callback, previous_span)) = self.previous {
            let prediction_step = signed_duration_ns(playback, previous_playback);
            let observed_step = signed_duration_ns(callback, previous_callback);
            let tolerance = output_gap_tolerance(previous_span);
            // A prediction epoch/latency jump must not consume a whole window
            // of queued events. Callback delivery jitter alone leaves the API
            // increment continuous and must not trigger this rebase.
            if (prediction_step - previous_span.as_nanos() as f64).abs()
                > tolerance.as_nanos() as f64
                && (prediction_step - observed_step).abs() > tolerance.as_nanos() as f64
            {
                self.reset();
            }
        }
        let sample = signed_duration_ns(playback, callback);
        // Average the first samples, so that the delivery jitter of the callback
        // after a reset does not stay in the offset for a whole smoothing period.
        self.samples = self.samples.saturating_add(1);
        let weight = (span.as_secs_f64() / OUTPUT_LEAD_SMOOTHING_SECS)
            .max(1.0 / f64::from(self.samples))
            .min(1.0);
        let offset = self.offset_ns.map_or(sample, |offset| offset + (sample - offset) * weight);
        self.offset_ns = Some(offset);
        self.previous = Some((playback, callback, span));
        // Express the signed correction relative to the observed Instant, without
        // clamping a negative offset to zero or constructing a negative Duration.
        shift_instant_ns(callback, sample - offset - (span / 2).as_nanos() as f64)
    }
}

fn signed_duration_ns(a: Instant, b: Instant) -> f64 {
    if a >= b {
        a.duration_since(b).as_nanos() as f64
    } else {
        -(b.duration_since(a).as_nanos() as f64)
    }
}

fn shift_instant_ns(time: Instant, offset: f64) -> Instant {
    let distance = Duration::from_nanos(offset.abs().round() as u64);
    if offset < 0.0 { time.checked_sub(distance) } else { time.checked_add(distance) }
        .unwrap_or(time)
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

/// Transport-neutral owner for the real-time mixer state used by native output backends.
///
/// CPAL normally owns this state inside its data callback. WASAPI exclusive mode has to service
/// endpoint buffers itself, so it moves the same state onto its dedicated worker thread.
#[cfg(any(windows, all(target_os = "macos", feature = "experimental-coreaudio-ioproc")))]
pub(crate) struct NativeOutputRenderer {
    channel_offset: usize,
    output_commands: SharedOutputCommands,
    retired_sources: RetiredOutputSources,
    current_frame: Arc<AtomicU64>,
    pub(super) diagnostics: Arc<CpalOutputDiagnosticsCounters>,
    buffers: OutputRenderBuffers,
    output_phase: ScheduledOutputPhase,
    schedule_timeline: OutputScheduleTimeline,
}

#[cfg(any(windows, all(target_os = "macos", feature = "experimental-coreaudio-ioproc")))]
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
            buffers: OutputRenderBuffers::new(),
            output_phase: ScheduledOutputPhase::default(),
            schedule_timeline: OutputScheduleTimeline::default(),
        }
    }

    pub(crate) fn render_at<T: OutputSample>(
        &mut self,
        data: &mut [T],
        channels: usize,
        playback: Option<Instant>,
        callback_start: Instant,
        sample_rate: u32,
    ) {
        self.diagnostics.callback_count.fetch_add(1, Ordering::Relaxed);
        if channels == 0 {
            data.fill(T::from_f32(0.0));
            self.diagnostics.observe_callback_duration(callback_start);
            return;
        }

        let frames = data.len() / channels;
        let start_frame = self.current_frame.load(Ordering::Relaxed);
        let span = frames_to_duration(frames, sample_rate);
        if self.schedule_timeline.observe(callback_start, span) {
            self.output_phase.reset();
        }
        let buffer_time = self.output_phase.buffer_time(playback, callback_start, span);
        render_output(
            data,
            OutputRenderLayout {
                channels,
                channel_offset: self.channel_offset,
                start_frame,
                buffer_time: Some(self.schedule_timeline.timing(buffer_time)),
            },
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

    #[cfg(windows)]
    pub(crate) fn record_stream_error(&self) {
        self.diagnostics.stream_error_count.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(windows)]
    pub(crate) fn record_untrusted_position(&self) {
        self.diagnostics.untrusted_position_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Advance the mixer timeline across a native-stream outage without rendering the missed
    /// interval. Gameplay keeps using its monotonic clock while the endpoint is unavailable, so
    /// resuming from the old frame would permanently delay every subsequently scheduled sound.
    pub(crate) fn catch_up_after_stream_outage(
        &mut self,
        outage: Duration,
        sample_rate: u32,
    ) -> u64 {
        let frames = duration_to_frames(outage, sample_rate);
        self.catch_up_output_frames(frames);
        frames
    }

    pub(crate) fn catch_up_output_frames(&mut self, frames: u64) {
        self.reset_prediction();
        if frames == 0 {
            return;
        }
        self.current_frame.fetch_add(frames, Ordering::Relaxed);
        self.schedule_timeline.mark_outage();
        self.diagnostics.timeline_catch_up_count.fetch_add(1, Ordering::Relaxed);
        self.diagnostics.timeline_catch_up_frames.fetch_add(frames, Ordering::Relaxed);
    }

    pub(crate) fn reset_prediction(&mut self) {
        self.output_phase.reset();
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct OutputRenderLayout {
    pub(super) buffer_time: Option<OutputScheduleTiming>,
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

impl OutputRenderBuffers {
    pub(super) fn new() -> Self {
        Self {
            mix: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
            source_scratch: vec![0.0; OUTPUT_SCRATCH_INITIAL_FRAMES * 2],
            render_sources: Vec::with_capacity(OUTPUT_SOURCE_INITIAL_CAPACITY),
            source_command_scratch: Vec::with_capacity(OUTPUT_COMMAND_QUEUE_CAPACITY),
        }
    }
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
        (layout.start_frame, layout.buffer_time),
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
    position: (u64, Option<OutputScheduleTiming>),
    frames: usize,
    output_commands: &SharedOutputCommands,
    retired_sources: &RetiredOutputSources,
    mix: &mut Vec<f32>,
    source_scratch: &mut Vec<f32>,
    render_sources: &mut Vec<RenderAudioSource>,
    source_command_scratch: &mut Vec<CpalOutputCommand>,
    diagnostics: &CpalOutputDiagnosticsCounters,
) {
    let (output_start_frame, buffer_time) = position;
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
                    engine.render_stereo_with_timing(
                        output_start_frame,
                        source_scratch,
                        buffer_time,
                    );
                    true
                }
                Err(_) => false,
            },
            RenderAudioEngine::Commanded(engine) => {
                engine.render_stereo_with_timing(output_start_frame, source_scratch, buffer_time)
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
            untrusted_position_count: self.untrusted_position_count.load(Ordering::Relaxed),
            stream_error_count: self.stream_error_count.load(Ordering::Relaxed),
            processor_overload_count: self.processor_overload_count.load(Ordering::Relaxed),
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

    pub(super) fn observe_callback_duration(&self, callback_start: Instant) {
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
mod tests {
    use super::*;
    use crate::queue::{AudioScheduler, ScheduledSound};
    use crate::sample::DecodedSample;
    use bmz_core::ids::SoundId;

    const SAMPLE_RATE: u32 = 48_000;
    const SPAN: Duration = Duration::from_millis(10);
    const LEAD: Duration = Duration::from_millis(10);

    fn distance(a: Instant, b: Instant) -> Duration {
        a.max(b).duration_since(a.min(b))
    }

    /// A filter past its first average, and the next callback time.
    fn settled_phase(start: Instant) -> (ScheduledOutputPhase, Instant) {
        let mut phase = ScheduledOutputPhase::default();
        for buffer in 0..500 {
            let callback = start + SPAN * buffer;
            phase.buffer_time(Some(callback + LEAD), callback, SPAN);
        }
        (phase, start + SPAN * 500)
    }

    #[test]
    fn shared_padding_changes_preserve_short_bgm_and_auto_attacks() {
        let start = Instant::now();
        let base = start + SPAN * 500;
        for catch_up in [true, false] {
            let mut phase = ScheduledOutputPhase::default();
            let mut timeline = OutputScheduleTimeline::default();
            for tick in 0..500 {
                let callback = start + SPAN * tick;
                timeline.observe(callback, SPAN);
                phase.buffer_time(Some(callback + Duration::from_millis(70)), callback, SPAN);
            }
            let mut engine = AudioEngine::new(SAMPLE_RATE);
            engine.insert_sample(
                SoundId(1),
                DecodedSample { channels: 1, sample_rate: SAMPLE_RATE, frames: vec![0.5] },
            );
            let mut sound = ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0);
            sound.start_time = Some(base + Duration::from_millis(60));
            sound.catch_up = catch_up;
            engine.schedule(sound);
            // Endpoint retains 80 ms of PCM: the delayed callback consumes that
            // padding, fills 80 ms, then the next callback needs only 10 ms.
            let mut frame = 100_000;
            let mut onsets = 0;
            for (callback_ms, playback_ms, frames) in [(0, 70, 480), (80, 80, 3840), (90, 160, 480)]
            {
                let callback = base + Duration::from_millis(callback_ms);
                let span = frames_to_duration(frames, SAMPLE_RATE);
                assert!(!timeline.observe(callback, span));
                let time = phase.buffer_time(
                    Some(base + Duration::from_millis(playback_ms)),
                    callback,
                    span,
                );
                let mut pcm = vec![0.0; frames * 2];
                engine.render_stereo_with_timing(frame, &mut pcm, Some(timeline.timing(time)));
                onsets += pcm.as_chunks::<2>().0.iter().filter(|pcm| pcm[0] != 0.0).count();
                frame += frames as u64;
            }
            assert_eq!(onsets, 1, "catch_up={catch_up}");
            assert!(engine.is_idle());
        }
    }

    #[test]
    fn callback_stop_is_distinct_from_a_prediction_or_buffer_change() {
        let base = Instant::now();
        let mut timeline = OutputScheduleTimeline::default();
        assert!(!timeline.observe(base, SPAN));
        assert!(!timeline.observe(base + SPAN, SPAN));
        assert_eq!(timeline.timing(base).outage_epoch, 0);
        assert!(timeline.observe(base + Duration::from_millis(60), SPAN));
        assert_eq!(timeline.timing(base).outage_epoch, 1);
        assert!(!timeline.observe(base + Duration::from_millis(70), SPAN));
        assert_eq!(timeline.timing(base).outage_epoch, 1);
    }

    #[test]
    fn scheduled_and_immediate_sounds_have_the_same_average_output_delay() {
        // With 4096 frames, lead plus period exceeds the 100 ms lookahead.
        for (frames, lead_us) in [(256, 6_700), (480, 10_000), (4_096, 85_300)] {
            let span = frames_to_duration(frames, SAMPLE_RATE);
            let lead = Duration::from_micros(lead_us);
            let start = Instant::now();
            let mut engine = AudioEngine::new(SAMPLE_RATE);
            engine.insert_sample(
                SoundId(1),
                DecodedSample { channels: 1, sample_rate: SAMPLE_RATE, frames: vec![0.5] },
            );
            let mut phase = ScheduledOutputPhase::default();
            let mut pending = VecDeque::new();
            let mut next_deadline = start + Duration::from_millis(200);
            let (mut scheduled, mut immediate, mut count) = (0.0, 0.0, 0_u32);
            let mut pcm = vec![0.0; frames * 2];
            for buffer in 0..(30_000_000 / span.as_micros()) as u32 {
                let callback = start + span * buffer;
                let playback = callback + lead;
                while next_deadline < callback + Duration::from_millis(100) {
                    let mut sound = ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0);
                    sound.start_time = Some(next_deadline);
                    engine.schedule(sound);
                    pending.push_back(next_deadline);
                    next_deadline += Duration::from_micros(13_370);
                }
                let time = phase.buffer_time(Some(playback), callback, span);
                engine.render_stereo_at(u64::from(buffer) * frames as u64, &mut pcm, Some(time));
                let onsets = pcm.as_chunks::<2>().0.iter().enumerate();
                for (frame, _) in onsets.filter(|(_, pcm)| pcm[0] != 0.0) {
                    let deadline = pending.pop_front().unwrap();
                    let output = playback + frames_to_duration(frame, SAMPLE_RATE);
                    scheduled += output.saturating_duration_since(deadline).as_secs_f64();
                    // An immediate request at the deadline starts the next buffer.
                    let elapsed = deadline.duration_since(start).as_secs_f64();
                    let next_callback = start + span.mul_f64((elapsed / span.as_secs_f64()).ceil());
                    immediate += (next_callback + lead).duration_since(deadline).as_secs_f64();
                    count += 1;
                }
            }
            // A one-sample BGM caught up past its start would never be rendered.
            assert!(count > 2_000 && pending.len() < 16, "{frames}: {count}, {}", pending.len());
            let difference = (scheduled - immediate).abs() / f64::from(count);
            assert!(difference < span.as_secs_f64() / 50.0, "{frames}: {difference} s");
        }
    }

    #[test]
    fn output_without_a_prediction_gets_the_same_buffer_time() {
        for lead in [None, Some(Duration::ZERO), Some(LEAD)] {
            let callback = Instant::now();
            let mut phase = ScheduledOutputPhase::default();
            let time = phase.buffer_time(lead.map(|lead| callback + lead), callback, SPAN);
            assert!(distance(time, callback - SPAN / 2) < Duration::from_micros(1));
        }
    }

    #[test]
    fn prefill_before_the_stream_starts_does_not_seed_the_output_lead() {
        let callback = Instant::now();
        let mut phase = ScheduledOutputPhase::default();
        phase.buffer_time(None, callback, SPAN);
        let next = callback + SPAN;
        let time = phase.buffer_time(Some(next + LEAD), next, SPAN);
        assert!(distance(time, next - SPAN / 2) < Duration::from_micros(1));
    }

    #[test]
    fn late_callback_delivery_does_not_move_the_buffer_time() {
        let (mut phase, next) = settled_phase(Instant::now());
        let time = phase.buffer_time(Some(next + LEAD), next + Duration::from_millis(3), SPAN);
        assert!(distance(time, next - SPAN / 2) < Duration::from_micros(20));
    }

    #[test]
    fn one_late_callback_after_a_reset_does_not_bias_the_next_seconds() {
        let start = Instant::now();
        let mut phase = ScheduledOutputPhase::default();
        // The seeding callback arrives 1 ms late.
        phase.buffer_time(Some(start + LEAD), start + Duration::from_millis(1), SPAN);
        let (mut time, mut callback) = (start, start);
        for buffer in 1..=20 {
            callback = start + SPAN * buffer;
            time = phase.buffer_time(Some(callback + LEAD), callback, SPAN);
        }
        assert!(distance(time, callback - SPAN / 2) < Duration::from_micros(100));
    }

    #[test]
    fn signed_prediction_drift_stays_bounded_for_ten_minutes() {
        let start = Instant::now();
        for rate in [44_100, 48_000, 96_000] {
            for frames in [256, 480, 4096] {
                let span = frames_to_duration(frames, rate);
                for ppm in [-200, -40, 0, 40, 200] {
                    let mut phase = ScheduledOutputPhase::default();
                    let count = (600.0 / span.as_secs_f64()).ceil() as u32;
                    for buffer in 0..=count {
                        let elapsed = span * buffer;
                        let callback = start + elapsed;
                        let drift_ns = elapsed.as_nanos() as f64 * f64::from(ppm) / 1e6;
                        let playback = shift_instant_ns(callback + LEAD, drift_ns);
                        let time = phase.buffer_time(Some(playback), callback, span);
                        assert!(
                            distance(time, callback - span / 2) < Duration::from_micros(500),
                            "{rate} Hz, {frames} frames, {ppm} ppm, {buffer}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unavailable_prediction_discards_the_old_offset_and_reseeds_on_recovery() {
        let start = Instant::now();
        let mut phase = ScheduledOutputPhase::default();
        phase.buffer_time(Some(start + LEAD), start, SPAN);
        for buffer in 1..5 {
            let callback = start + SPAN * buffer;
            assert_eq!(phase.buffer_time(None, callback, SPAN), callback - SPAN / 2);
        }
        let callback = start + SPAN * 5;
        assert_eq!(phase.buffer_time(Some(callback - LEAD), callback, SPAN), callback - SPAN / 2);
    }

    #[test]
    fn a_valid_zero_prediction_continues_the_filter() {
        let (mut phase, start) = settled_phase(Instant::now());
        let first = phase.buffer_time(Some(start), start, SPAN);
        for buffer in 1..2_000 {
            let callback = start + SPAN * buffer;
            phase.buffer_time(Some(callback), callback, SPAN);
        }
        let callback = start + SPAN * 2_000;
        let last = phase.buffer_time(Some(callback), callback, SPAN);
        assert!(distance(first, start - SPAN / 2) > Duration::from_millis(9));
        assert!(distance(last, callback - SPAN / 2) < Duration::from_micros(1));
    }

    #[test]
    fn prediction_jumps_do_not_skip_attacks_or_consume_future_events() {
        let start = Instant::now();
        for jump_ms in [-100, -40, 40, 100] {
            let mut engine = AudioEngine::new(SAMPLE_RATE);
            engine.insert_sample(
                SoundId(1),
                DecodedSample { channels: 1, sample_rate: SAMPLE_RATE, frames: vec![0.5] },
            );
            let mut phase = ScheduledOutputPhase::default();
            for ms in [8, 18, 28, 38] {
                let mut sound = ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0);
                sound.start_time = Some(start + Duration::from_millis(ms));
                engine.schedule(sound);
            }
            let mut pcm = [0.0; 960];
            for buffer in 0..5 {
                let callback = start + SPAN * buffer;
                let playback = shift_instant_ns(
                    callback + LEAD,
                    if buffer == 0 { 0.0 } else { f64::from(jump_ms) * 1e6 },
                );
                let time = phase.buffer_time(Some(playback), callback, SPAN);
                engine.render_stereo_at(100_000 + u64::from(buffer) * 480, &mut pcm, Some(time));
                let onsets: Vec<_> = pcm
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .enumerate()
                    .filter(|(_, pcm)| pcm[0] != 0.0)
                    .map(|(frame, _)| frame)
                    .collect();
                assert_eq!(
                    onsets,
                    if buffer == 0 { vec![] } else { vec![144] },
                    "jump {jump_ms}, buffer {buffer}"
                );
            }
            assert!(engine.is_idle());
        }
    }

    #[test]
    fn callback_delivery_jitter_past_the_prediction_keeps_the_api_progression() {
        let (mut phase, callback) = settled_phase(Instant::now());
        let time =
            phase.buffer_time(Some(callback + LEAD), callback + Duration::from_millis(30), SPAN);
        // No rebase to the late callback, even though the signed offset crosses zero.
        assert!(distance(time, callback - SPAN / 2) < Duration::from_micros(200));
    }

    #[test]
    fn device_frame_rate_error_does_not_accumulate_in_the_rendered_onset() {
        let start = Instant::now();
        for rate in [44_100, 48_000, 96_000] {
            let frames = rate as usize / 100;
            let span = frames_to_duration(frames, rate);
            for ppm in [-200, -40, 40, 200] {
                let mut engine = AudioEngine::new(rate);
                engine.insert_sample(
                    SoundId(1),
                    DecodedSample { channels: 1, sample_rate: rate, frames: vec![0.5] },
                );
                let mut phase = ScheduledOutputPhase::default();
                let mut pcm = vec![0.0; frames * 2];
                for seconds in [0, 60, 180, 300, 600] {
                    // Device rate changes the wall time of each frame, independently
                    // of the API clock drift tested above. Predictions here are exact.
                    let frame = u64::from(rate) * seconds;
                    let elapsed = frame as f64 / (f64::from(rate) * (1.0 + f64::from(ppm) / 1e6));
                    let callback = start + Duration::from_secs_f64(elapsed);
                    let deadline = callback + Duration::from_millis(3);
                    let mut sound = ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0);
                    sound.start_time = Some(deadline);
                    engine.schedule(sound);
                    let time = phase.buffer_time(Some(callback + LEAD), callback, span);
                    engine.render_stereo_at(frame, &mut pcm, Some(time));
                    let onset =
                        pcm.as_chunks::<2>().0.iter().position(|pcm| pcm[0] != 0.0).unwrap();
                    let output = callback
                        + LEAD
                        + Duration::from_secs_f64(
                            onset as f64 / (f64::from(rate) * (1.0 + f64::from(ppm) / 1e6)),
                        );
                    assert!(
                        distance(output, deadline + LEAD + span / 2) < Duration::from_micros(30)
                    );
                }
            }
        }
    }

    #[cfg(any(windows, all(target_os = "macos", feature = "experimental-coreaudio-ioproc")))]
    #[test]
    fn native_outage_advances_active_and_pending_sounds_only_once() {
        let start = Instant::now();
        let mut engine = AudioEngine::new(SAMPLE_RATE);
        for id in [1, 2] {
            engine.insert_sample(
                SoundId(id),
                DecodedSample {
                    channels: 1,
                    sample_rate: SAMPLE_RATE,
                    frames: (0..10_000).map(|frame| frame as f32 / 10_000.0).collect(),
                },
            );
        }
        engine.schedule(ScheduledSound::one_shot(100_000, SoundId(1), 1.0, -1.0));
        let mut sound = ScheduledSound::one_shot(0, SoundId(2), 1.0, 1.0);
        sound.start_time = Some(start + Duration::from_millis(25));
        engine.schedule(sound);
        let frame = Arc::new(AtomicU64::new(100_000));
        let diagnostics = Arc::new(CpalOutputDiagnosticsCounters::default());
        let commands = Arc::new(Mutex::new(VecDeque::from([CpalOutputCommand::AddLegacySource {
            id: 1,
            kind: CpalOutputSourceKind::Play,
            engine: Arc::new(Mutex::new(engine)),
        }])));
        let mut renderer = NativeOutputRenderer::new(
            0,
            commands,
            Arc::new(Mutex::new(Vec::new())),
            frame.clone(),
            diagnostics.clone(),
        );
        let mut pcm = [0.0_f32; 960];
        renderer.render_at(&mut pcm, 2, Some(start + LEAD), start, SAMPLE_RATE);
        renderer.catch_up_output_frames(2_400);
        let callback = start + Duration::from_millis(60);
        renderer.render_at(&mut pcm, 2, Some(callback + LEAD), callback, SAMPLE_RATE);
        assert_eq!(&pcm[..2], &[0.288, 0.144]);
        let callback = callback + SPAN;
        renderer.render_at(&mut pcm, 2, Some(callback + LEAD), callback, SAMPLE_RATE);
        assert_eq!(&pcm[..2], &[0.336, 0.192]);
        assert_eq!(frame.load(Ordering::Relaxed), 100_000 + 2_400 + 3 * 480);
        let diagnostics = diagnostics.take_snapshot();
        assert_eq!(diagnostics.timeline_catch_up_count, 1);
        assert_eq!(diagnostics.timeline_catch_up_frames, 2_400);
    }
}
