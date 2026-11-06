use std::time::Duration;

use super::*;

#[derive(Debug, Default)]
struct OutputPlaybackTimeline {
    previous_playback: Option<::cpal::StreamInstant>,
    previous_observed: Option<Instant>,
    previous_frames: usize,
}

impl OutputPlaybackTimeline {
    fn catch_up_frames(
        &mut self,
        playback: ::cpal::StreamInstant,
        frames: usize,
        sample_rate: u32,
        observed: Instant,
    ) -> u64 {
        let catch_up = self.previous_observed.and_then(|previous_observed| {
            let callback_gap = observed.checked_duration_since(previous_observed)?;
            let previous_span = frames_to_duration(self.previous_frames, sample_rate);
            let span = frames_to_duration(frames, sample_rate);
            // A variable-size callback may refill PCM accumulated since the
            // last wake. Its larger span is coverage, not missing audio.
            let callback_loss = callback_gap.saturating_sub(previous_span.max(span));
            // Continuous buffers start one previous span apart, whatever this
            // refill holds. A larger output buffer/latency estimate is not
            // evidence of lost PCM, so a callback outage is required as well.
            // An API clock that went backwards cannot be compared at all.
            let loss = self
                .previous_playback
                .and_then(|previous| playback.checked_duration_since(previous))
                .map_or(callback_loss, |step| {
                    step.saturating_sub(previous_span).min(callback_loss)
                });
            (loss > output_gap_tolerance(previous_span.min(span)))
                .then(|| duration_to_frames(loss, sample_rate))
        });
        self.previous_playback = Some(playback);
        self.previous_observed = Some(observed);
        self.previous_frames = frames;
        catch_up.unwrap_or(0)
    }
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
    let mut buffers = OutputRenderBuffers::new();
    let sample_rate = config.sample_rate;
    let mut playback_timeline = OutputPlaybackTimeline::default();
    let mut frame_clock = OutputFrameClock::default();
    let timing_enabled = bmz_core::latency::diagnostics_enabled();
    // Query outside the callback. A failed host identity is unknown too.
    let prediction_available = device.id().ok().is_some_and(|id| id.host().name() != "PipeWire");
    let mut previous_callback = None;
    let mut previous_stream_callback: Option<::cpal::StreamInstant> = None;
    let mut warmup_until = Instant::now() + Duration::from_secs(2);
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let mut suspend = bmz_core::suspend::SuspendMonitor::default();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if timing_enabled {
        suspend.poll();
    }
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
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                let resumed = timing_enabled && suspend.poll();
                #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                let resumed = false;
                if timing_enabled
                    && (resumed
                        || previous_callback.is_some_and(|previous| {
                            callback_start.duration_since(previous) > Duration::from_secs(1)
                        })
                        || previous_stream_callback.is_some_and(|previous| {
                            info.timestamp().callback.checked_duration_since(previous).is_none()
                        }))
                {
                    diagnostics.timing.reset();
                    warmup_until = callback_start + Duration::from_secs(2);
                    previous_callback = None;
                }
                let measure = timing_enabled && callback_start >= warmup_until;
                if measure && !prediction_available {
                    diagnostics.timing.observe_unavailable(
                        frames,
                        callback_start,
                        previous_callback,
                    );
                } else if measure {
                    diagnostics.timing.observe_delay(
                        frames,
                        callback_start,
                        previous_callback,
                        info.timestamp().playback.checked_duration_since(info.timestamp().callback),
                    );
                }
                if timing_enabled {
                    previous_callback = Some(callback_start);
                    previous_stream_callback = Some(info.timestamp().callback);
                }
                let catch_up_frames = playback_timeline.catch_up_frames(
                    info.timestamp().playback,
                    frames,
                    sample_rate,
                    callback_start,
                );
                let start_frame =
                    current_frame.load(Ordering::Relaxed).saturating_add(catch_up_frames);
                if catch_up_frames != 0 {
                    diagnostics.timeline_catch_up_count.fetch_add(1, Ordering::Relaxed);
                    diagnostics
                        .timeline_catch_up_frames
                        .fetch_add(catch_up_frames, Ordering::Relaxed);
                }
                let span = frames_to_duration(frames, sample_rate);
                let buffer_time =
                    frame_clock.buffer_time(start_frame, callback_start, span, sample_rate);
                render_output(
                    data,
                    OutputRenderLayout {
                        channels,
                        channel_offset,
                        start_frame,
                        buffer_time: Some(buffer_time),
                    },
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
                if measure {
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

pub(super) fn device_name(device: &::cpal::Device) -> String {
    device
        .description()
        .map(|description| description.name().to_string())
        .unwrap_or_else(|_| device.to_string())
}

#[cfg(test)]
mod playback_timeline_tests {
    use super::*;

    const SAMPLE_RATE: u32 = 48_000;
    const BUFFER_FRAMES: usize = 480;

    #[test]
    fn a_shared_stall_is_measured_from_the_previous_buffer() {
        // cpal reports callback + padding. The 1056-frame endpoint held 576
        // frames (12 ms) at the last callback, then drained during the stall.
        let base = Instant::now();
        let api = ::cpal::StreamInstant::from_millis;
        for (stall_ms, silent_frames) in [(60, 1_824), (100, 3_744)] {
            let mut timeline = OutputPlaybackTimeline::default();
            timeline.catch_up_frames(api(1_012), BUFFER_FRAMES, SAMPLE_RATE, base);
            assert_eq!(
                timeline.catch_up_frames(
                    api(1_000 + stall_ms),
                    1_056,
                    SAMPLE_RATE,
                    base + Duration::from_millis(stall_ms),
                ),
                silent_frames
            );
        }
    }

    #[test]
    fn a_refill_is_not_an_outage_even_if_the_api_latency_ignores_it() {
        let base = Instant::now();
        let api = ::cpal::StreamInstant::from_millis;
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(api(1_010), BUFFER_FRAMES, SAMPLE_RATE, base);
        assert_eq!(
            timeline.catch_up_frames(
                api(1_090),
                3_840,
                SAMPLE_RATE,
                base + Duration::from_millis(80)
            ),
            0
        );
    }

    #[test]
    fn a_larger_refill_covers_callback_delay_even_when_the_api_clock_resets() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(::cpal::StreamInstant::from_millis(1_000), 480, SAMPLE_RATE, base);
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(5),
                3_840,
                SAMPLE_RATE,
                base + Duration::from_millis(80)
            ),
            0
        );
    }

    #[test]
    fn a_callback_stop_is_still_an_outage_when_the_api_clock_resets() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base,
        );
        // The new API epoch is not comparable. The Instant gap still is.
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(5),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(110),
            ),
            4_800
        );
    }

    #[test]
    fn an_api_clock_reset_with_regular_callbacks_is_not_an_outage() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base,
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(5),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(10),
            ),
            0
        );
    }

    #[test]
    fn output_latency_jump_alone_does_not_skip_playing_samples() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base,
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_110),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(10)
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_120),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(20)
            ),
            0
        );
    }

    #[test]
    fn callback_jitter_does_not_trigger_timeline_catch_up() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_000),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(1_000),
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_029),
                BUFFER_FRAMES * 2,
                SAMPLE_RATE,
                base + Duration::from_millis(1_029),
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_049),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(1_049),
            ),
            0
        );
    }

    #[test]
    fn confirmed_playback_gap_advances_the_logical_output_frame() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base + Duration::from_millis(1_000),
        );

        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(1_110),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(1_110),
            ),
            4_800
        );
    }

    #[test]
    fn invalid_backward_timestamp_resets_without_correction() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base + Duration::from_millis(1_000),
        );

        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(900),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(900),
            ),
            0
        );
        assert_eq!(
            timeline.catch_up_frames(
                ::cpal::StreamInstant::from_millis(910),
                BUFFER_FRAMES,
                SAMPLE_RATE,
                base + Duration::from_millis(910),
            ),
            0
        );
    }
}
