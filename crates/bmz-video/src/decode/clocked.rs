use super::*;

#[derive(Default)]
pub(crate) struct ClockedLoopTiming {
    last_pts_us: Option<i64>,
    last_interval_us: Option<i64>,
    last_duration_us: Option<i64>,
}

impl ClockedLoopTiming {
    fn record_frame(&mut self, pts_us: i64, duration_us: i64) {
        if let Some(previous) = self.last_pts_us {
            let interval = pts_us.saturating_sub(previous);
            if interval > 0 {
                self.last_interval_us = Some(interval);
            }
        }
        self.last_pts_us = Some(pts_us);
        self.last_duration_us = (duration_us > 0).then_some(duration_us);
    }

    fn end_us(&self, base_us: i64, stream_duration_us: Option<i64>, frame_interval_us: i64) -> i64 {
        let duration =
            self.last_duration_us.or(self.last_interval_us).unwrap_or(frame_interval_us).max(1);
        let frame_end = self.last_pts_us.unwrap_or(base_us).saturating_add(duration);
        // Stream duration also preserves sub-microsecond rounding at the final
        // frame. Do not use container duration, which may include longer audio.
        stream_duration_us
            .map_or(frame_end, |duration| frame_end.max(base_us.saturating_add(duration)))
    }
}

pub(crate) fn decode_video_following_playback_time(
    path: &Path,
    clocked_frames: Arc<Mutex<ClockedFrameState>>,
    playback_target_us: Arc<AtomicI64>,
    stop_decode: Arc<AtomicBool>,
    restart_decode: Arc<AtomicBool>,
) -> Result<()> {
    let mut ictx = ffmpeg_next::format::input(path)?;
    let selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;
    let mut decoded = ffmpeg_next::frame::Video::empty();
    let mut decode_context = VideoDecodeContext::default();
    let mut loop_base_us = 0;
    let mut highest_target_us = playback_target_us.load(Ordering::Acquire);
    while !stop_decode.load(Ordering::Acquire) {
        if restart_decode.swap(false, Ordering::AcqRel) {
            loop_base_us = playback_target_us.load(Ordering::Acquire);
            highest_target_us = loop_base_us;
            decode_context.timestamp_normalizer = VideoTimestampNormalizer::default();
            rewind_video_decoder(&mut ictx, &mut decoder)?;
        }
        let mut target_us = playback_target_us.load(Ordering::Acquire);
        // The next loop's base can still be in the future while its final frame
        // is displayed. Only observed playback time can indicate a rewind.
        if clocked_playback_target_rewound(target_us, highest_target_us) {
            loop_base_us = target_us;
            highest_target_us = target_us;
            decode_context.timestamp_normalizer = VideoTimestampNormalizer::default();
            rewind_video_decoder(&mut ictx, &mut decoder)?;
        }
        highest_target_us = highest_target_us.max(target_us);
        let mut loop_timing = ClockedLoopTiming::default();
        let mut drain_status = ClockedDrainStatus::Continue;

        for (stream, packet) in ictx.packets() {
            if stop_decode.load(Ordering::Acquire) {
                drain_status = ClockedDrainStatus::Stop;
                break;
            }
            if stream.index() != selected.index {
                continue;
            }
            decoder.send_packet(&packet)?;
            drain_status = drain_clocked_decoder_frames(
                &mut decoder,
                &mut decoded,
                &selected,
                loop_base_us,
                &mut decode_context,
                &clocked_frames,
                &playback_target_us,
                &stop_decode,
                &mut loop_timing,
            )?;
            if drain_status != ClockedDrainStatus::Continue {
                break;
            }
        }

        if drain_status == ClockedDrainStatus::Continue {
            decoder.send_eof()?;
            drain_status = drain_clocked_decoder_frames(
                &mut decoder,
                &mut decoded,
                &selected,
                loop_base_us,
                &mut decode_context,
                &clocked_frames,
                &playback_target_us,
                &stop_decode,
                &mut loop_timing,
            )?;
        }

        if drain_status == ClockedDrainStatus::Restart {
            target_us = playback_target_us.load(Ordering::Acquire);
            loop_base_us = target_us;
            highest_target_us = target_us;
            decode_context.timestamp_normalizer = VideoTimestampNormalizer::default();
            rewind_video_decoder(&mut ictx, &mut decoder)?;
            continue;
        }
        if loop_timing.last_pts_us.is_none() {
            break;
        }
        if drain_status == ClockedDrainStatus::Stop {
            break;
        }
        target_us = playback_target_us.load(Ordering::Acquire);
        highest_target_us = highest_target_us.max(target_us);
        loop_base_us = loop_timing
            .end_us(loop_base_us, selected.duration_us, selected.frame_interval_us)
            .max(target_us);
        rewind_video_decoder(&mut ictx, &mut decoder)?;
    }
    mark_clocked_frames_finished(&clocked_frames);
    Ok(())
}

pub(crate) fn publish_clocked_frame(
    clocked_frames: &Mutex<ClockedFrameState>,
    frame: DecodedFrame,
) -> Result<()> {
    let mut state = clocked_frames
        .lock()
        .map_err(|_| anyhow::anyhow!("clocked video frame state lock poisoned"))?;
    if let Some(previous) = state.frame.replace(frame) {
        recycle_clocked_rgba(&mut state, previous.rgba);
    }
    state.finished = false;
    Ok(())
}

pub(crate) fn mark_clocked_frames_finished(clocked_frames: &Mutex<ClockedFrameState>) {
    if let Ok(mut state) = clocked_frames.lock() {
        state.finished = true;
    }
}

pub(crate) fn drain_clocked_decoder_frames(
    decoder: &mut ffmpeg_next::decoder::Video,
    decoded: &mut ffmpeg_next::frame::Video,
    selected: &SelectedVideoStream,
    loop_base_us: i64,
    decode_context: &mut VideoDecodeContext,
    clocked_frames: &Mutex<ClockedFrameState>,
    playback_target_us: &AtomicI64,
    stop_decode: &AtomicBool,
    loop_timing: &mut ClockedLoopTiming,
) -> Result<ClockedDrainStatus> {
    loop {
        match decoder.receive_frame(decoded) {
            Ok(()) => {}
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN })
            | Err(ffmpeg_next::Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }

        if stop_decode.load(Ordering::Acquire) {
            return Ok(ClockedDrainStatus::Stop);
        }

        let pts_us = decode_context
            .timestamp_normalizer
            .frame_pts_us(decoded, selected.time_base_num, selected.time_base_den)
            .saturating_add(loop_base_us);
        loop_timing.record_frame(
            pts_us,
            timestamp_raw_to_us(
                decoded.packet().duration,
                selected.time_base_num,
                selected.time_base_den,
            ),
        );
        if should_skip_frame_conversion(pts_us, playback_target_us.load(Ordering::Acquire)) {
            continue;
        }

        let publish_after_us = pts_us.saturating_sub(CLOCKED_FRAME_PUBLISH_LEAD_US);
        let target_us = playback_target_us.load(Ordering::Acquire);
        if publish_after_us > target_us {
            match wait_until_playback_reaches_frame(
                playback_target_us,
                stop_decode,
                publish_after_us,
                target_us,
            ) {
                ClockedFrameWait::Reached => {}
                ClockedFrameWait::Rewound => return Ok(ClockedDrainStatus::Restart),
                ClockedFrameWait::Stopped => return Ok(ClockedDrainStatus::Stop),
            }
        }

        let frame = rgba_frame_from_video_with_scaler(
            decoded,
            pts_us,
            &mut decode_context.scaler,
            take_clocked_recycled_rgba(clocked_frames),
        )?;
        publish_clocked_frame(clocked_frames, frame)?;
    }
    Ok(ClockedDrainStatus::Continue)
}

pub(crate) fn take_clocked_recycled_rgba(
    clocked_frames: &Mutex<ClockedFrameState>,
) -> Option<Vec<u8>> {
    clocked_frames.lock().ok().and_then(|mut state| state.recycled_rgba.pop())
}

pub(crate) fn recycle_clocked_rgba(state: &mut ClockedFrameState, mut rgba: Vec<u8>) {
    const MAX_RECYCLED_RGBA_BUFFERS: usize = 2;
    if state.recycled_rgba.len() < MAX_RECYCLED_RGBA_BUFFERS {
        rgba.clear();
        state.recycled_rgba.push(rgba);
    }
}

pub(crate) fn should_skip_frame_conversion(frame_pts_us: i64, playback_target_us: i64) -> bool {
    frame_pts_us.saturating_add(CLOCKED_FRAME_CATCH_UP_TOLERANCE_US) < playback_target_us
}

pub(crate) fn clocked_playback_target_rewound(target_us: i64, highest_target_us: i64) -> bool {
    target_us.saturating_add(CLOCKED_FRAME_CATCH_UP_TOLERANCE_US) < highest_target_us
}

pub(crate) fn wait_until_playback_reaches_frame(
    playback_target_us: &AtomicI64,
    stop_decode: &AtomicBool,
    frame_pts_us: i64,
    observed_target_us: i64,
) -> ClockedFrameWait {
    let mut highest_target_us = observed_target_us;
    loop {
        if stop_decode.load(Ordering::Acquire) {
            return ClockedFrameWait::Stopped;
        }
        let target_us = playback_target_us.load(Ordering::Acquire);
        if target_us >= frame_pts_us {
            return ClockedFrameWait::Reached;
        }
        // playback clock が巻き戻ったときだけ現在の decode loop をやり直す。
        // 未来フレームを待っている通常ケースでは抜けない。ここで抜けると
        // decoder が先のフレームを publish し続け、skin movie が高速再生に見える。
        if clocked_playback_target_rewound(target_us, highest_target_us) {
            return ClockedFrameWait::Rewound;
        }
        highest_target_us = highest_target_us.max(target_us);
        let sleep_us = (frame_pts_us - target_us).clamp(1_000, CLOCKED_FRAME_WAIT_MAX_SLEEP_US);
        std::thread::sleep(Duration::from_micros(sleep_us as u64));
    }
}

#[cfg(test)]
#[path = "clocked_tests.rs"]
mod tests;
