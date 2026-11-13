use super::*;

const MAX_VIDEO_DIMENSION: u32 = 8192;
const MAX_VIDEO_PIXELS: u64 = 3840 * 2160 * 2;

pub(crate) fn validate_video_dimensions(width: u32, height: u32) -> Result<()> {
    anyhow::ensure!(
        width <= MAX_VIDEO_DIMENSION && height <= MAX_VIDEO_DIMENSION,
        "video dimensions {width}x{height} exceed maximum dimension ({MAX_VIDEO_DIMENSION})"
    );
    // A zero parameter is unknown until the first decoded frame.
    anyhow::ensure!(
        u64::from(width) * u64::from(height) <= MAX_VIDEO_PIXELS,
        "video dimensions {width}x{height} exceed maximum pixels ({MAX_VIDEO_PIXELS})"
    );
    Ok(())
}

pub(crate) fn select_video_stream(
    ictx: &ffmpeg_next::format::context::Input,
) -> Result<SelectedVideoStream> {
    let best = ictx
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or_else(|| anyhow::anyhow!("no video stream found"))?;
    let best_index = best.index();
    let mut candidates = Vec::new();
    for stream in ictx.streams() {
        let params = stream.parameters();
        if params.medium() != ffmpeg_next::media::Type::Video {
            continue;
        }
        candidates.push((stream.index(), video_stream_bit_rate(&params), params));
    }
    let selected_index = choose_beatoraja_video_stream(
        best_index,
        candidates.iter().map(|(index, bitrate, _)| (*index, *bitrate)),
    );
    let (stream_index, codec_params) = candidates
        .into_iter()
        .find_map(|(index, _, params)| (index == selected_index).then_some((index, params)))
        .ok_or_else(|| anyhow::anyhow!("selected video stream not found"))?;
    // SAFETY: codec_params owns the AVCodecParameters for this shared read.
    let params = unsafe { &*codec_params.as_ptr() };
    validate_video_dimensions(u32::try_from(params.width)?, u32::try_from(params.height)?)?;
    let stream = ictx
        .stream(stream_index)
        .ok_or_else(|| anyhow::anyhow!("selected video stream not available"))?;
    let tb = stream.time_base();
    let start_time_raw =
        (stream.start_time() != ffmpeg_next::ffi::AV_NOPTS_VALUE).then_some(stream.start_time());
    let duration_us = (stream.duration() > 0)
        .then(|| {
            timestamp_raw_to_us(stream.duration(), tb.numerator().into(), tb.denominator().into())
        })
        .filter(|duration| *duration > 0);
    let rate = [stream.avg_frame_rate(), stream.rate()]
        .into_iter()
        .find(|rate| rate.numerator() > 0 && rate.denominator() > 0);
    let frame_interval_us = rate.map_or(33_333, |rate| {
        (1_000_000 * i64::from(rate.denominator()) / i64::from(rate.numerator())).max(1)
    });
    tracing::debug!(stream_index, best_index, "selected video stream for BGA decode");
    Ok(SelectedVideoStream {
        index: stream_index,
        time_base_num: tb.numerator() as i64,
        time_base_den: tb.denominator() as i64,
        start_time_raw,
        duration_us,
        frame_interval_us,
        codec_params,
    })
}

pub(crate) fn open_video_decoder(
    selected: &SelectedVideoStream,
) -> Result<ffmpeg_next::decoder::Video> {
    let mut context =
        ffmpeg_next::codec::context::Context::from_parameters(selected.codec_params.clone())?;
    context.set_threading(ffmpeg_next::codec::threading::Config::kind(
        ffmpeg_next::codec::threading::Type::Frame,
    ));
    Ok(context.decoder().video()?)
}

pub(crate) fn rewind_video_decoder(
    ictx: &mut ffmpeg_next::format::context::Input,
    decoder: &mut ffmpeg_next::decoder::Video,
) -> Result<()> {
    ictx.seek(0, ..)?;
    decoder.flush();
    Ok(())
}

pub(crate) fn seek_video_decoder(
    ictx: &mut ffmpeg_next::format::context::Input,
    decoder: &mut ffmpeg_next::decoder::Video,
    selected: &SelectedVideoStream,
    video_offset_us: i64,
) -> Result<()> {
    let stream_start_us = selected.start_time_raw.map_or(0, |start_time_raw| {
        timestamp_raw_to_us(start_time_raw, selected.time_base_num, selected.time_base_den)
    });
    let target_us = stream_start_us.saturating_add(video_offset_us.max(0));
    ictx.seek(target_us, ..target_us)?;
    decoder.flush();
    Ok(())
}

pub(crate) fn timestamp_raw_to_us(
    timestamp_raw: i64,
    time_base_num: i64,
    time_base_den: i64,
) -> i64 {
    if time_base_den == 0 {
        return 0;
    }
    let timestamp_us = i128::from(timestamp_raw)
        .saturating_mul(i128::from(time_base_num))
        .saturating_mul(1_000_000)
        / i128::from(time_base_den);
    timestamp_us.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

pub(crate) fn video_stream_bit_rate(params: &ffmpeg_next::codec::Parameters) -> usize {
    // Stream selection must not open decoders before validating their dimensions.
    usize::try_from(params.bit_rate()).unwrap_or(0)
}

pub(crate) fn choose_beatoraja_video_stream(
    best_index: usize,
    candidates: impl IntoIterator<Item = (usize, usize)>,
) -> usize {
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort_by_key(|(index, _)| *index);
    let best_bitrate = candidates
        .iter()
        .find_map(|(index, bitrate)| (*index == best_index).then_some(*bitrate))
        .unwrap_or(0);
    if best_bitrate >= 10 {
        return best_index;
    }
    candidates
        .iter()
        .find_map(|(index, bitrate)| {
            (*index > best_index && *index <= 5 && *bitrate >= 10).then_some(*index)
        })
        .or_else(|| {
            candidates
                .iter()
                .find_map(|(index, bitrate)| (*index <= 5 && *bitrate >= 10).then_some(*index))
        })
        .unwrap_or(best_index)
}

pub(crate) fn rgba_frame_from_video(
    decoded: &ffmpeg_next::frame::Video,
    pts_us: i64,
) -> Result<DecodedFrame> {
    let mut scaler = None;
    rgba_frame_from_video_with_scaler(decoded, pts_us, &mut scaler, None, [0; 2])
}

fn rgba_output_size([width, height]: [u32; 2], [canvas_w, canvas_h]: [u32; 2]) -> [u32; 2] {
    if canvas_w == 0 || canvas_h == 0 || width <= canvas_w || height <= canvas_h {
        return [width, height];
    }

    // Cover the canvas so a wide or tall video can still fill it when cropped.
    let width_at_canvas_h = u64::from(width) * u64::from(canvas_h);
    let height_at_canvas_w = u64::from(height) * u64::from(canvas_w);
    if width_at_canvas_h >= height_at_canvas_w {
        [width_at_canvas_h.div_ceil(u64::from(height)) as u32, canvas_h]
    } else {
        [canvas_w, height_at_canvas_w.div_ceil(u64::from(width)) as u32]
    }
}

pub(crate) fn rgba_frame_from_video_with_scaler(
    decoded: &ffmpeg_next::frame::Video,
    pts_us: i64,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    rgba_buffer: Option<Vec<u8>>,
    canvas_size: [u32; 2],
) -> Result<DecodedFrame> {
    let w = decoded.width();
    let h = decoded.height();
    validate_video_dimensions(w, h)?;
    anyhow::ensure!(w > 0 && h > 0, "decoded video frame has zero dimensions");

    let input = ffmpeg_next::software::scaling::context::Definition {
        format: decoded.format(),
        width: w,
        height: h,
    };
    let [width, height] = rgba_output_size([w, h], canvas_size);
    if scaler.as_ref().is_none_or(|scaler| {
        scaler.input() != &input
            || scaler.output().width != width
            || scaler.output().height != height
    }) {
        *scaler = Some(ffmpeg_next::software::scaling::context::Context::get(
            decoded.format(),
            w,
            h,
            ffmpeg_next::format::Pixel::RGBA,
            width,
            height,
            ffmpeg_next::software::scaling::flag::Flags::FAST_BILINEAR,
        )?);
        tracing::debug!(
            source_width = w,
            source_height = h,
            width,
            height,
            "configured video RGBA scaler"
        );
    }

    let mut rgba_frame = ffmpeg_next::frame::Video::empty();
    scaler.as_mut().unwrap().run(decoded, &mut rgba_frame)?;

    let data = rgba_frame.data(0);
    let stride = rgba_frame.stride(0);
    let row_bytes = (width as usize) * 4;
    let rgba =
        copy_rgba_frame_data_with_buffer(data, stride, row_bytes, height as usize, rgba_buffer);

    Ok(DecodedFrame { pts_us, rgba, width, height, source_size: [w, h] })
}

#[cfg(test)]
pub(crate) fn copy_rgba_frame_data(
    data: &[u8],
    stride: usize,
    row_bytes: usize,
    rows: usize,
) -> Vec<u8> {
    copy_rgba_frame_data_with_buffer(data, stride, row_bytes, rows, None)
}

pub(crate) fn copy_rgba_frame_data_with_buffer(
    data: &[u8],
    stride: usize,
    row_bytes: usize,
    rows: usize,
    rgba_buffer: Option<Vec<u8>>,
) -> Vec<u8> {
    let total_bytes = row_bytes.saturating_mul(rows);
    let mut rgba = rgba_buffer.unwrap_or_default();
    rgba.clear();
    if stride == row_bytes
        && let Some(contiguous) = data.get(..total_bytes)
    {
        rgba.extend_from_slice(contiguous);
        return rgba;
    }

    rgba.resize(total_bytes, 0);
    for row in 0..rows {
        let src_start = row.saturating_mul(stride);
        let dst_start = row * row_bytes;
        let Some(src) = data.get(src_start..src_start + row_bytes) else {
            break;
        };
        let dst = &mut rgba[dst_start..dst_start + row_bytes];
        dst.copy_from_slice(src);
    }
    rgba
}
