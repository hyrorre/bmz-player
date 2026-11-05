use std::time::Duration;

use super::*;

const MIN_PLAYBACK_DISCONTINUITY: Duration = Duration::from_millis(20);

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
        let catch_up = self.previous_playback.and_then(|previous| {
            let actual = playback.checked_duration_since(previous)?;
            let expected = frames_to_duration(self.previous_frames, sample_rate);
            let callback_gap = observed.checked_duration_since(self.previous_observed?)?;
            // A larger output buffer/latency estimate is not evidence of lost
            // PCM. Require a callback outage as well before skipping samples.
            let gap = actual.min(callback_gap).saturating_sub(expected);
            let tolerance = MIN_PLAYBACK_DISCONTINUITY.max(expected.saturating_mul(2));
            (gap > tolerance).then(|| duration_to_frames(gap, sample_rate))
        });
        self.previous_playback = Some(playback);
        self.previous_observed = Some(observed);
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

/// Convert only through an explicit API/Instant clock pair. Callback delivery
/// jitter does not move a valid pair; a reset/implausible timestamp rebases it.
fn map_playback_time(
    observed: Instant,
    callback: ::cpal::StreamInstant,
    playback: ::cpal::StreamInstant,
    initial: Option<(Instant, ::cpal::StreamInstant)>,
    rebased: &mut Option<(Instant, ::cpal::StreamInstant)>,
) -> Instant {
    let mapped = rebased.or(initial).and_then(|(system, api)| {
        if let Some(delta) = playback.checked_duration_since(api) {
            system.checked_add(delta)
        } else {
            system.checked_sub(api.checked_duration_since(playback)?)
        }
    });
    mapped
        .filter(|time| {
            time.checked_duration_since(observed).unwrap_or_default() < Duration::from_secs(1)
                && observed.checked_duration_since(*time).unwrap_or_default()
                    < Duration::from_millis(100)
        })
        .unwrap_or_else(|| {
            *rebased = Some((observed, callback));
            let delay = playback
                .checked_duration_since(callback)
                .filter(|delay| *delay < Duration::from_secs(1))
                .unwrap_or_default();
            observed + delay
        })
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
    // Pair the API clock with Instant before starting the stream. The callback
    // converts API timestamps through this pair, not by comparing clock epochs.
    let clock_pair = Arc::new(std::sync::OnceLock::new());
    let callback_clock_pair = Arc::clone(&clock_pair);
    let mut clock_pair_override: Option<(Instant, ::cpal::StreamInstant)> = None;
    let stream = device
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
                let timestamps = info.timestamp();
                let playback = if prediction_available {
                    map_playback_time(
                        callback_start,
                        timestamps.callback,
                        timestamps.playback,
                        callback_clock_pair.get().copied(),
                        &mut clock_pair_override,
                    )
                } else {
                    // No trustworthy output position: use the current callback,
                    // never a fixed song-start frame or a fabricated API latency.
                    callback_start
                };
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
                render_output(
                    data,
                    OutputRenderLayout {
                        channels,
                        channel_offset,
                        start_frame,
                        playback: Some(playback),
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
        .map_err(CpalBackendError::BuildStream)?;
    let before = Instant::now();
    let api = stream.now();
    let after = Instant::now();
    let _ = clock_pair.set((before + after.duration_since(before) / 2, api));
    Ok(stream)
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
    fn playback_mapping_uses_api_epoch_without_callback_delivery_jitter() {
        let start = Instant::now();
        let api = ::cpal::StreamInstant::from_millis;
        let mut rebased = None;
        let pair = Some((start, api(1_000)));
        for delivery_ms in [10, 15, 19] {
            assert_eq!(
                map_playback_time(
                    start + Duration::from_millis(delivery_ms),
                    api(1_010),
                    api(1_020),
                    pair,
                    &mut rebased
                ),
                start + Duration::from_millis(20)
            );
            assert!(rebased.is_none());
        }
    }

    #[test]
    fn playback_mapping_recovers_from_reset_and_rejects_broken_delay() {
        let start = Instant::now();
        let api = ::cpal::StreamInstant::from_millis;
        let mut rebased = None;
        let observed = start + Duration::from_secs(5);
        let pair = Some((start, api(10_000)));
        assert_eq!(
            map_playback_time(observed, api(0), api(10), pair, &mut rebased),
            observed + Duration::from_millis(10)
        );
        assert_eq!(
            map_playback_time(
                observed + Duration::from_millis(10),
                api(10),
                api(20),
                pair,
                &mut rebased
            ),
            observed + Duration::from_millis(20)
        );
        assert_eq!(map_playback_time(observed, api(0), api(30_000), pair, &mut rebased), observed);
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
