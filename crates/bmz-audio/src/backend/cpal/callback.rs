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
            // A variable-size callback may refill PCM accumulated since the
            // last wake. Its larger span is coverage, not missing audio.
            let expected = frames_to_duration(self.previous_frames.max(frames), sample_rate);
            let actual = match self.previous_playback {
                Some(previous) => playback.checked_duration_since(previous)?,
                None => callback_gap,
            };
            // A larger output buffer/latency estimate is not evidence of lost
            // PCM. Require a callback outage as well before skipping samples.
            let gap = actual.min(callback_gap).saturating_sub(expected);
            (gap > output_gap_tolerance(expected)).then(|| duration_to_frames(gap, sample_rate))
        });
        self.previous_playback = Some(playback);
        self.previous_observed = Some(observed);
        self.previous_frames = frames;
        catch_up.unwrap_or(0)
    }

    /// API times are not comparable across a clock rebase. The callback gap is
    /// measured by Instant, so a real stop is still detected from it alone.
    fn rebase(&mut self) {
        self.previous_playback = None;
    }
}

/// Convert only through an explicit API/Instant clock pair. Callback delivery
/// jitter does not move a valid pair; a reset/implausible timestamp rebases it.
fn map_playback_time(
    observed: Instant,
    callback: ::cpal::StreamInstant,
    playback: ::cpal::StreamInstant,
    initial: Option<(Instant, ::cpal::StreamInstant)>,
    rebased: &mut Option<(Instant, ::cpal::StreamInstant)>,
) -> (Option<Instant>, bool) {
    let mapped = rebased.or(initial).and_then(|(system, api)| {
        if let Some(delta) = playback.checked_duration_since(api) {
            system.checked_add(delta)
        } else {
            system.checked_sub(api.checked_duration_since(playback)?)
        }
    });
    if let Some(time) = mapped.filter(|time| {
        time.checked_duration_since(observed).unwrap_or_default() < Duration::from_secs(1)
            && observed.checked_duration_since(*time).unwrap_or_default()
                < Duration::from_millis(100)
    }) {
        (Some(time), false)
    } else {
        *rebased = Some((observed, callback));
        let prediction = playback
            .checked_duration_since(callback)
            .filter(|delay| *delay < Duration::from_secs(1))
            .and_then(|delay| observed.checked_add(delay));
        (prediction, true)
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
    let mut output_phase = ScheduledOutputPhase::default();
    let mut schedule_timeline = OutputScheduleTimeline::default();
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
                let (playback, rebased) = if prediction_available {
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
                    (None, false)
                };
                let span = frames_to_duration(frames, sample_rate);
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
                if rebased {
                    playback_timeline.rebase();
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
                    schedule_timeline.mark_outage();
                    diagnostics.timeline_catch_up_count.fetch_add(1, Ordering::Relaxed);
                    diagnostics
                        .timeline_catch_up_frames
                        .fetch_add(catch_up_frames, Ordering::Relaxed);
                }
                let schedule_stopped = schedule_timeline.observe(callback_start, span);
                if rebased || catch_up_frames != 0 || schedule_stopped {
                    output_phase.reset();
                }
                let buffer_time = output_phase.buffer_time(playback, callback_start, span);
                render_output(
                    data,
                    OutputRenderLayout {
                        channels,
                        channel_offset,
                        start_frame,
                        buffer_time: Some(schedule_timeline.timing(buffer_time)),
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
    fn a_larger_refill_covers_callback_delay_even_during_a_clock_rebase() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(::cpal::StreamInstant::from_millis(1_000), 480, SAMPLE_RATE, base);
        timeline.rebase();
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
    fn a_callback_stop_is_still_an_outage_across_a_clock_rebase() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base,
        );
        timeline.rebase();
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
    fn a_clock_rebase_with_regular_callbacks_is_not_an_outage() {
        let base = Instant::now();
        let mut timeline = OutputPlaybackTimeline::default();
        timeline.catch_up_frames(
            ::cpal::StreamInstant::from_millis(1_000),
            BUFFER_FRAMES,
            SAMPLE_RATE,
            base,
        );
        timeline.rebase();
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
                (Some(start + Duration::from_millis(20)), false)
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
            (Some(observed + Duration::from_millis(10)), true)
        );
        assert_eq!(
            map_playback_time(
                observed + Duration::from_millis(10),
                api(10),
                api(20),
                pair,
                &mut rebased
            ),
            (Some(observed + Duration::from_millis(20)), false)
        );
        assert_eq!(
            map_playback_time(observed, api(0), api(30_000), pair, &mut rebased),
            (None, true)
        );
    }

    #[test]
    fn signed_api_drift_and_epoch_rebases_keep_scheduling_bounded() {
        let start = Instant::now();
        let span = Duration::from_millis(10);
        for ppm in [-200_i64, -40, 40, 200] {
            let mut phase = ScheduledOutputPhase::default();
            let mut rebased = None;
            let pair = Some((start, ::cpal::StreamInstant::from_nanos(0)));
            for buffer in 0..=60_000_u32 {
                let elapsed = u64::from(buffer) * 10_000_000;
                let api_elapsed =
                    (i128::from(elapsed) * i128::from(1_000_000 + ppm) / 1_000_000) as u64;
                let observed = start + Duration::from_nanos(elapsed);
                let (prediction, reset) = map_playback_time(
                    observed,
                    ::cpal::StreamInstant::from_nanos(api_elapsed),
                    ::cpal::StreamInstant::from_nanos(api_elapsed + 10_000_000),
                    pair,
                    &mut rebased,
                );
                if reset {
                    phase.reset();
                }
                let time = phase.buffer_time(prediction, observed, span);
                let expected = observed - span / 2;
                assert!(
                    time.max(expected).duration_since(time.min(expected))
                        < Duration::from_micros(500),
                    "{ppm} ppm at {buffer}"
                );
            }
            if ppm == -200 {
                // The old pair becomes >100 ms late and is explicitly rebased.
                assert!(rebased.is_some());
            }
        }
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
