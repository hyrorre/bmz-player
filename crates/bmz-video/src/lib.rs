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
    finished: bool,
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

        Ok(Self { receiver, pending: VecDeque::new(), current: None, finished: false })
    }

    /// チャンネルをdrainして `video_offset_us` 以下の最新フレームを返す。
    pub fn poll_frame(&mut self, video_offset_us: i64) -> Option<&DecodedFrame> {
        // チャンネルから利用可能なフレームをすべて受信
        loop {
            match self.receiver.try_recv() {
                Ok(frame) => self.pending.push_back(frame),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finished = true;
                    break;
                }
            }
        }

        // video_offset_us 以下のフレームのうち最新のものだけを current に設定する。
        // decoder が先行している時に、古い大きな RGBA buffer を current へ何度も
        // 入れ替えず、表示されない pending frame はその場で捨てる。
        while self.pending.get(1).is_some_and(|frame| frame.pts_us <= video_offset_us) {
            self.pending.pop_front();
        }
        if self.pending.front().is_some_and(|frame| frame.pts_us <= video_offset_us) {
            self.current = self.pending.pop_front();
        }

        self.current.as_ref()
    }

    pub fn is_finished(&self) -> bool {
        self.finished && self.pending.is_empty()
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
    let rgba = copy_rgba_frame_data(data, stride, row_bytes, h as usize);

    Ok(DecodedFrame { pts_us, rgba, width: w, height: h })
}

fn copy_rgba_frame_data(data: &[u8], stride: usize, row_bytes: usize, rows: usize) -> Vec<u8> {
    let total_bytes = row_bytes.saturating_mul(rows);
    if stride == row_bytes
        && let Some(contiguous) = data.get(..total_bytes)
    {
        return contiguous.to_vec();
    }

    let mut rgba = vec![0u8; total_bytes];
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts_us: i64) -> DecodedFrame {
        DecodedFrame { pts_us, rgba: vec![pts_us as u8], width: 1, height: 1 }
    }

    fn decoder_with_pending(pending: impl IntoIterator<Item = i64>) -> VideoBgaDecoder {
        let (_sender, receiver) = sync_channel(1);
        VideoBgaDecoder {
            receiver,
            pending: pending.into_iter().map(frame).collect(),
            current: Some(frame(0)),
            finished: false,
        }
    }

    #[test]
    fn poll_frame_skips_overdue_intermediate_frames() {
        let mut decoder = decoder_with_pending([10, 20, 30]);

        let frame = decoder.poll_frame(25).unwrap();

        assert_eq!(frame.pts_us, 20);
        assert_eq!(decoder.pending.len(), 1);
        assert_eq!(decoder.pending.front().unwrap().pts_us, 30);
    }

    #[test]
    fn poll_frame_keeps_current_when_next_frame_is_future() {
        let mut decoder = decoder_with_pending([10, 20]);

        let frame = decoder.poll_frame(5).unwrap();

        assert_eq!(frame.pts_us, 0);
        assert_eq!(decoder.pending.len(), 2);
    }

    #[test]
    fn copy_rgba_frame_data_copies_contiguous_rows_at_once() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];

        let copied = copy_rgba_frame_data(&data, 4, 4, 2);

        assert_eq!(copied, data);
    }

    #[test]
    fn copy_rgba_frame_data_strips_padded_stride() {
        let data = [1, 2, 3, 4, 99, 99, 5, 6, 7, 8, 88, 88];

        let copied = copy_rgba_frame_data(&data, 6, 4, 2);

        assert_eq!(copied, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }
}
