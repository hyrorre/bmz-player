use std::collections::VecDeque;
use std::path::Path;
use std::sync::mpsc::{SyncSender, sync_channel};

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub pts_us: i64,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct VideoBgaDecoder {
    receiver: std::sync::mpsc::Receiver<DecodedFrame>,
    pending: VecDeque<DecodedFrame>,
    current: Option<DecodedFrame>,
}

impl VideoBgaDecoder {
    pub fn open(path: &Path) -> Result<Self> {
        bmz_ffmpeg::ensure_init().map_err(|e| anyhow::anyhow!(e))?;

        let path = path.to_path_buf();
        let (sender, receiver) = sync_channel(4);

        std::thread::Builder::new().name("bmz-video-decode".to_string()).spawn(move || {
            if let Err(e) = decode_video(&path, sender) {
                tracing::warn!(path = %path.display(), error = %e, "video decode thread error");
            }
        })?;

        Ok(Self { receiver, pending: VecDeque::new(), current: None })
    }

    /// チャンネルをdrainして `video_offset_us` 以下の最新フレームを返す。
    pub fn poll_frame(&mut self, video_offset_us: i64) -> Option<&DecodedFrame> {
        // チャンネルから利用可能なフレームをすべて受信
        while let Ok(frame) = self.receiver.try_recv() {
            self.pending.push_back(frame);
        }

        // video_offset_us 以下のフレームのうち最新のものを current に設定
        while let Some(front) = self.pending.front() {
            if front.pts_us <= video_offset_us {
                self.current = self.pending.pop_front();
            } else {
                break;
            }
        }

        self.current.as_ref()
    }
}

pub fn decode_first_frame(path: &Path) -> Result<DecodedFrame> {
    bmz_ffmpeg::ensure_init().map_err(|e| anyhow::anyhow!(e))?;

    let mut ictx = ffmpeg_next::format::input(path)?;
    let stream = ictx
        .streams()
        .best(ffmpeg_next::media::Type::Video)
        .ok_or_else(|| anyhow::anyhow!("no video stream found"))?;

    let stream_index = stream.index();
    let time_base_num = stream.time_base().numerator() as i64;
    let time_base_den = stream.time_base().denominator() as i64;
    let context = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())?;
    let mut decoder = context.decoder().video()?;
    let mut decoded = ffmpeg_next::frame::Video::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet)?;
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => return rgba_frame_from_video(&decoded, time_base_num, time_base_den),
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN }) => {}
            Err(ffmpeg_next::Error::Eof) => {
                return Err(anyhow::anyhow!("video ended before first frame"));
            }
            Err(e) => return Err(e.into()),
        }
    }

    decoder.send_eof()?;
    match decoder.receive_frame(&mut decoded) {
        Ok(()) => rgba_frame_from_video(&decoded, time_base_num, time_base_den),
        Err(e) => Err(e.into()),
    }
}

fn decode_video(path: &Path, sender: SyncSender<DecodedFrame>) -> Result<()> {
    let mut ictx = ffmpeg_next::format::input(path)?;

    // ベストビデオストリームを見つける
    let stream_index;
    let time_base_num;
    let time_base_den;
    let codec_params;

    {
        let stream = ictx
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| anyhow::anyhow!("no video stream found"))?;

        stream_index = stream.index();
        let tb = stream.time_base();
        time_base_num = tb.numerator() as i64;
        time_base_den = tb.denominator() as i64;
        codec_params = stream.parameters();
    }

    let context = ffmpeg_next::codec::context::Context::from_parameters(codec_params)?;
    let mut decoder = context.decoder().video()?;

    // スケーラは最初のフレーム受信後に lazily 作成
    let mut scaler: Option<ffmpeg_next::software::scaling::context::Context> = None;

    let mut decoded = ffmpeg_next::frame::Video::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != stream_index {
            continue;
        }

        decoder.send_packet(&packet)?;

        loop {
            match decoder.receive_frame(&mut decoded) {
                Ok(()) => {}
                Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN }) => break,
                Err(ffmpeg_next::Error::Eof) => return Ok(()),
                Err(e) => return Err(e.into()),
            }

            let frame = rgba_frame_from_video_with_scaler(
                &decoded,
                time_base_num,
                time_base_den,
                &mut scaler,
            )?;

            if sender.send(frame).is_err() {
                // receiver が drop された
                return Ok(());
            }
        }
    }

    // フラッシュ
    decoder.send_eof()?;
    loop {
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {}
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN })
            | Err(ffmpeg_next::Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }

        let frame =
            rgba_frame_from_video_with_scaler(&decoded, time_base_num, time_base_den, &mut scaler)?;
        if sender.send(frame).is_err() {
            return Ok(());
        }
    }

    Ok(())
}

fn rgba_frame_from_video(
    decoded: &ffmpeg_next::frame::Video,
    time_base_num: i64,
    time_base_den: i64,
) -> Result<DecodedFrame> {
    let mut scaler = None;
    rgba_frame_from_video_with_scaler(decoded, time_base_num, time_base_den, &mut scaler)
}

fn rgba_frame_from_video_with_scaler(
    decoded: &ffmpeg_next::frame::Video,
    time_base_num: i64,
    time_base_den: i64,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
) -> Result<DecodedFrame> {
    let w = decoded.width();
    let h = decoded.height();

    if scaler.is_none() {
        *scaler = Some(ffmpeg_next::software::scaling::context::Context::get(
            decoded.format(),
            w,
            h,
            ffmpeg_next::format::Pixel::RGBA,
            w,
            h,
            ffmpeg_next::software::scaling::flag::Flags::BILINEAR,
        )?);
    }

    let mut rgba_frame = ffmpeg_next::frame::Video::empty();
    scaler.as_mut().unwrap().run(decoded, &mut rgba_frame)?;

    let pts_raw = decoded.pts().unwrap_or(0);
    let pts_us =
        if time_base_den != 0 { pts_raw * time_base_num * 1_000_000 / time_base_den } else { 0 };

    let data = rgba_frame.data(0);
    let stride = rgba_frame.stride(0);
    let row_bytes = (w as usize) * 4;
    let mut rgba = vec![0u8; row_bytes * h as usize];
    for row in 0..h as usize {
        let src = &data[row * stride..row * stride + row_bytes];
        let dst = &mut rgba[row * row_bytes..(row + 1) * row_bytes];
        dst.copy_from_slice(src);
    }

    Ok(DecodedFrame { pts_us, rgba, width: w, height: h })
}
