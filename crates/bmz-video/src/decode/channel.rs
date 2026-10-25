use super::*;

pub(crate) fn decode_video_restartable(
    path: &Path,
    sender: SyncSender<QueuedDecodedFrame>,
    stop_decode: Arc<AtomicBool>,
    restart_decode: Arc<AtomicBool>,
    pass_finished: Arc<AtomicBool>,
    playback_target_us: Arc<AtomicI64>,
    decode_generation: Arc<AtomicU64>,
) -> Result<()> {
    let mut ictx = crate::open_input(path)?;
    let mut selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;
    let mut scaler = None;
    let mut decoded = ffmpeg_next::frame::Video::empty();
    let mut first_pass = true;

    loop {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(());
        }

        let restart_was_requested = restart_decode.swap(false, Ordering::AcqRel);
        let target_us = playback_target_us.load(Ordering::Acquire).max(0);
        let mut direct_seeked = false;
        if !first_pass || restart_was_requested {
            if target_us > 0 {
                match seek_video_decoder(&mut ictx, &mut decoder, &selected, target_us) {
                    Ok(()) => {
                        direct_seeked = true;
                        tracing::debug!(target_us, path = %path.display(), "direct-seeked video decoder");
                    }
                    Err(error) => {
                        tracing::warn!(
                            target_us,
                            path = %path.display(),
                            %error,
                            "video direct seek failed; falling back to decode from start"
                        );
                        // A failed avformat seek may leave the input context latched at EOF.
                        // Reopen both demuxer and decoder so the sequential fallback is reliable.
                        ictx = crate::open_input(path)?;
                        selected = select_video_stream(&ictx)?;
                        decoder = open_video_decoder(&selected)?;
                        scaler = None;
                    }
                }
            } else {
                rewind_video_decoder(&mut ictx, &mut decoder)?;
            }
        }
        first_pass = false;
        pass_finished.store(false, Ordering::Release);
        let generation = decode_generation.load(Ordering::Acquire);

        let pass_end = decode_video_channel_pass(
            &mut ictx,
            &mut decoder,
            &selected,
            &mut scaler,
            &mut decoded,
            &sender,
            &stop_decode,
            &restart_decode,
            &playback_target_us,
            ChannelDecodePassStart { generation, direct_seeked },
        )?;

        match pass_end {
            ChannelDecodePassEnd::Stop => return Ok(()),
            ChannelDecodePassEnd::Restart => continue,
            ChannelDecodePassEnd::Eof => {
                pass_finished.store(true, Ordering::Release);
                while !stop_decode.load(Ordering::Acquire) {
                    if restart_decode.swap(false, Ordering::AcqRel) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
                if stop_decode.load(Ordering::Acquire) {
                    return Ok(());
                }
            }
        }
    }
}

pub(crate) fn decode_video_channel_pass(
    ictx: &mut ffmpeg_next::format::context::Input,
    decoder: &mut ffmpeg_next::decoder::Video,
    selected: &SelectedVideoStream,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    decoded: &mut ffmpeg_next::frame::Video,
    sender: &SyncSender<QueuedDecodedFrame>,
    stop_decode: &AtomicBool,
    restart_decode: &AtomicBool,
    playback_target_us: &AtomicI64,
    start: ChannelDecodePassStart,
) -> Result<ChannelDecodePassEnd> {
    let generation = start.generation;
    let timestamp_origin_raw = if start.direct_seeked {
        // When stream start_time is unavailable, raw timestamps in seeked packets are still
        // absolute on the stream timeline. Using zero avoids rebasing the keyframe to video 0.
        selected.start_time_raw.or(Some(0))
    } else {
        selected.start_time_raw
    };
    let mut timestamp_normalizer = VideoTimestampNormalizer::with_origin_raw(timestamp_origin_raw);
    let mut catch_up = ChannelFrameCatchUp {
        // A direct seek lands on an earlier keyframe. Do not publish it as the target frame;
        // decode forward and keep only the final frame at/before the requested position.
        published_any: start.direct_seeked,
        last_skipped: None,
    };

    for (stream, packet) in ictx.packets() {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Stop);
        }
        if restart_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Restart);
        }
        if stream.index() != selected.index {
            continue;
        }

        decoder.send_packet(&packet)?;
        loop {
            match decoder.receive_frame(decoded) {
                Ok(()) => {}
                Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN }) => break,
                Err(ffmpeg_next::Error::Eof) => {
                    return publish_last_skipped_channel_frame(
                        &mut catch_up,
                        scaler,
                        sender,
                        generation,
                        stop_decode,
                        restart_decode,
                    );
                }
                Err(e) => return Err(e.into()),
            }

            if stop_decode.load(Ordering::Acquire) {
                return Ok(ChannelDecodePassEnd::Stop);
            }
            if restart_decode.load(Ordering::Acquire) {
                return Ok(ChannelDecodePassEnd::Restart);
            }

            let pts_us = timestamp_normalizer.frame_pts_us(
                decoded,
                selected.time_base_num,
                selected.time_base_den,
            );
            if catch_up.should_skip(pts_us, playback_target_us.load(Ordering::Acquire)) {
                catch_up.record_skipped((decoded.clone(), pts_us));
                continue;
            }
            let frame = rgba_frame_from_video_with_scaler(decoded, pts_us, scaler, None)?;
            match send_decoded_frame(sender, frame, generation, stop_decode, restart_decode)? {
                ChannelDecodePassEnd::Stop => return Ok(ChannelDecodePassEnd::Stop),
                ChannelDecodePassEnd::Restart => return Ok(ChannelDecodePassEnd::Restart),
                ChannelDecodePassEnd::Eof => catch_up.record_published(),
            }
        }
    }

    decoder.send_eof()?;
    loop {
        match decoder.receive_frame(decoded) {
            Ok(()) => {}
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN })
            | Err(ffmpeg_next::Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }

        if stop_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Stop);
        }
        if restart_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Restart);
        }

        let pts_us = timestamp_normalizer.frame_pts_us(
            decoded,
            selected.time_base_num,
            selected.time_base_den,
        );
        if catch_up.should_skip(pts_us, playback_target_us.load(Ordering::Acquire)) {
            catch_up.record_skipped((decoded.clone(), pts_us));
            continue;
        }
        let frame = rgba_frame_from_video_with_scaler(decoded, pts_us, scaler, None)?;
        match send_decoded_frame(sender, frame, generation, stop_decode, restart_decode)? {
            ChannelDecodePassEnd::Stop => return Ok(ChannelDecodePassEnd::Stop),
            ChannelDecodePassEnd::Restart => return Ok(ChannelDecodePassEnd::Restart),
            ChannelDecodePassEnd::Eof => catch_up.record_published(),
        }
    }

    publish_last_skipped_channel_frame(
        &mut catch_up,
        scaler,
        sender,
        generation,
        stop_decode,
        restart_decode,
    )
}

pub(crate) fn publish_last_skipped_channel_frame(
    catch_up: &mut ChannelFrameCatchUp<(ffmpeg_next::frame::Video, i64)>,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    sender: &SyncSender<QueuedDecodedFrame>,
    generation: u64,
    stop_decode: &AtomicBool,
    restart_decode: &AtomicBool,
) -> Result<ChannelDecodePassEnd> {
    if stop_decode.load(Ordering::Acquire) {
        return Ok(ChannelDecodePassEnd::Stop);
    }
    if restart_decode.load(Ordering::Acquire) {
        return Ok(ChannelDecodePassEnd::Restart);
    }
    let Some((decoded, pts_us)) = catch_up.take_last_skipped() else {
        return Ok(ChannelDecodePassEnd::Eof);
    };
    let frame = rgba_frame_from_video_with_scaler(&decoded, pts_us, scaler, None)?;
    send_decoded_frame(sender, frame, generation, stop_decode, restart_decode)
}

pub(crate) fn send_decoded_frame(
    sender: &SyncSender<QueuedDecodedFrame>,
    frame: DecodedFrame,
    generation: u64,
    stop_decode: &AtomicBool,
    restart_decode: &AtomicBool,
) -> Result<ChannelDecodePassEnd> {
    let mut queued = QueuedDecodedFrame { generation, frame };
    loop {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Stop);
        }
        if restart_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Restart);
        }
        match sender.try_send(queued) {
            Ok(()) => return Ok(ChannelDecodePassEnd::Eof),
            Err(TrySendError::Full(returned)) => {
                queued = returned;
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(TrySendError::Disconnected(_)) => return Ok(ChannelDecodePassEnd::Stop),
        }
    }
}
