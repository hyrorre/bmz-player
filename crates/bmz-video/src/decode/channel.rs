use super::*;

#[cfg(test)]
pub(crate) static CHANNEL_WAIT_COUNT: AtomicU64 = AtomicU64::new(0);

fn wait_for_channel_progress() {
    #[cfg(test)]
    CHANNEL_WAIT_COUNT.fetch_add(1, Ordering::Relaxed);
    std::thread::park();
}

pub(crate) struct ChannelDecodeState {
    restart_target_us: Mutex<i64>,
    generation: AtomicU64,
    finished_generation: AtomicU64,
}

impl Default for ChannelDecodeState {
    fn default() -> Self {
        Self {
            restart_target_us: Mutex::new(0),
            generation: AtomicU64::new(0),
            finished_generation: AtomicU64::new(u64::MAX),
        }
    }
}

impl ChannelDecodeState {
    pub(crate) fn restart_at(&self, target_us: i64) {
        let mut restart_target_us = self.restart_target_us.lock().unwrap();
        *restart_target_us = target_us;
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub(crate) fn restart_position(&self) -> (u64, i64) {
        let restart_target_us = self.restart_target_us.lock().unwrap();
        (self.generation(), *restart_target_us)
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn mark_finished(&self, generation: u64) {
        self.finished_generation.store(generation, Ordering::Release);
    }

    pub(crate) fn finished(&self) -> bool {
        self.finished_generation.load(Ordering::Acquire) == self.generation()
    }
}

pub(crate) fn decode_video_restartable(
    path: &Path,
    sender: SyncSender<QueuedDecodedFrame>,
    stop_decode: Arc<AtomicBool>,
    channel: Arc<ChannelDecodeState>,
    playback_target_us: Arc<AtomicI64>,
) -> Result<()> {
    let mut ictx = crate::open_input(path)?;
    let mut selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;
    let mut scaler = None;
    let mut decoded = ffmpeg_next::frame::Video::empty();
    loop {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(());
        }

        let (generation, target_us) = channel.restart_position();
        let mut direct_seeked = false;
        if generation != 0 {
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
        let pass_end = decode_video_channel_pass(
            &mut ictx,
            &mut decoder,
            &selected,
            &mut scaler,
            &mut decoded,
            &sender,
            &stop_decode,
            &channel,
            &playback_target_us,
            ChannelDecodePassStart { generation, direct_seeked, path },
        )?;

        match pass_end {
            ChannelDecodePassEnd::Stop => return Ok(()),
            ChannelDecodePassEnd::Restart => continue,
            ChannelDecodePassEnd::Eof => {
                channel.mark_finished(generation);
                while !stop_decode.load(Ordering::Acquire) && channel.generation() == generation {
                    wait_for_channel_progress();
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
    channel: &ChannelDecodeState,
    playback_target_us: &AtomicI64,
    start: ChannelDecodePassStart<'_>,
) -> Result<ChannelDecodePassEnd> {
    let ChannelDecodePassStart { generation, direct_seeked, path } = start;
    let timestamp_origin_raw = if direct_seeked {
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
        published_any: direct_seeked,
        last_skipped: None,
    };
    let codec = selected.codec_params.id();
    let mut warned_about_skipped_packet = false;

    for (stream, packet) in ictx.packets() {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Stop);
        }
        if channel.generation() != generation {
            return Ok(ChannelDecodePassEnd::Restart);
        }
        if stream.index() != selected.index {
            continue;
        }

        if let Err(error) = decoder.send_packet(&packet) {
            if !is_skippable_video_packet_error(codec, error) {
                return Err(error.into());
            }
            if !warned_about_skipped_packet {
                tracing::warn!(
                    path = %path.display(),
                    packet_dts = ?packet.dts(),
                    %error,
                    "skipping VC-1 video packet after a decoder error"
                );
                warned_about_skipped_packet = true;
            }
            continue;
        }
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
                        channel,
                    );
                }
                Err(e) => return Err(e.into()),
            }

            if stop_decode.load(Ordering::Acquire) {
                return Ok(ChannelDecodePassEnd::Stop);
            }
            if channel.generation() != generation {
                return Ok(ChannelDecodePassEnd::Restart);
            }

            validate_video_dimensions(decoded.width(), decoded.height())?;
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
            match send_decoded_frame(sender, frame, generation, stop_decode, channel)? {
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
        if channel.generation() != generation {
            return Ok(ChannelDecodePassEnd::Restart);
        }

        validate_video_dimensions(decoded.width(), decoded.height())?;
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
        match send_decoded_frame(sender, frame, generation, stop_decode, channel)? {
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
        channel,
    )
}

pub(crate) fn is_skippable_video_packet_error(
    codec: ffmpeg_next::codec::Id,
    error: ffmpeg_next::Error,
) -> bool {
    // The VC-1 decoder can return -1 for a packet and still decode subsequent packets.
    // At send_packet this appears as EPERM, after the demuxer already read the packet.
    codec == ffmpeg_next::codec::Id::VC1
        && error == (ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EPERM })
}

pub(crate) fn publish_last_skipped_channel_frame(
    catch_up: &mut ChannelFrameCatchUp<(ffmpeg_next::frame::Video, i64)>,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    sender: &SyncSender<QueuedDecodedFrame>,
    generation: u64,
    stop_decode: &AtomicBool,
    channel: &ChannelDecodeState,
) -> Result<ChannelDecodePassEnd> {
    if stop_decode.load(Ordering::Acquire) {
        return Ok(ChannelDecodePassEnd::Stop);
    }
    if channel.generation() != generation {
        return Ok(ChannelDecodePassEnd::Restart);
    }
    let Some((decoded, pts_us)) = catch_up.take_last_skipped() else {
        return Ok(ChannelDecodePassEnd::Eof);
    };
    let frame = rgba_frame_from_video_with_scaler(&decoded, pts_us, scaler, None)?;
    send_decoded_frame(sender, frame, generation, stop_decode, channel)
}

pub(crate) fn send_decoded_frame(
    sender: &SyncSender<QueuedDecodedFrame>,
    frame: DecodedFrame,
    generation: u64,
    stop_decode: &AtomicBool,
    channel: &ChannelDecodeState,
) -> Result<ChannelDecodePassEnd> {
    let mut queued = QueuedDecodedFrame { generation, frame };
    loop {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(ChannelDecodePassEnd::Stop);
        }
        if channel.generation() != generation {
            return Ok(ChannelDecodePassEnd::Restart);
        }
        match sender.try_send(queued) {
            Ok(()) => return Ok(ChannelDecodePassEnd::Eof),
            Err(TrySendError::Full(returned)) => {
                queued = returned;
                wait_for_channel_progress();
            }
            Err(TrySendError::Disconnected(_)) => return Ok(ChannelDecodePassEnd::Stop),
        }
    }
}
