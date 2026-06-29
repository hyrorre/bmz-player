use std::collections::VecDeque;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI64, Ordering},
    mpsc::{Receiver, SyncSender, sync_channel},
};
use std::time::Duration;

use anyhow::Result;

const CLOCKED_FRAME_PUBLISH_LEAD_US: i64 = 8_000;
const CLOCKED_FRAME_CATCH_UP_TOLERANCE_US: i64 = 8_000;
const CLOCKED_FRAME_WAIT_MAX_SLEEP_US: i64 = 4_000;

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub pts_us: i64,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct VideoBgaDecoder {
    receiver: Option<Receiver<DecodedFrame>>,
    clocked_frames: Option<Arc<Mutex<ClockedFrameState>>>,
    pending: VecDeque<DecodedFrame>,
    current: Option<DecodedFrame>,
    finished: bool,
    playback_target_us: Option<Arc<AtomicI64>>,
    stop_decode: Arc<AtomicBool>,
}

struct SelectedVideoStream {
    index: usize,
    time_base_num: i64,
    time_base_den: i64,
    codec_params: ffmpeg_next::codec::Parameters,
}

#[derive(Default)]
struct ClockedFrameState {
    frame: Option<DecodedFrame>,
    finished: bool,
    recycled_rgba: Vec<Vec<u8>>,
}

impl VideoBgaDecoder {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_inner(path, false)
    }

    /// Open a decoder that follows the playback clock reported by `poll_frame`.
    ///
    /// This is intended for skin movie sources: the decode thread coalesces overdue
    /// frames instead of queueing every decoded frame, closer to beatoraja's
    /// SkinSourceMovie behavior.
    pub fn open_following_playback_time(path: &Path) -> Result<Self> {
        Self::open_inner(path, true)
    }

    fn open_inner(path: &Path, follow_playback_time: bool) -> Result<Self> {
        bmz_ffmpeg::ensure_init().map_err(|e| anyhow::anyhow!(e))?;

        let path = path.to_path_buf();
        let stop_decode = Arc::new(AtomicBool::new(false));
        if follow_playback_time {
            let playback_target_us = Arc::new(AtomicI64::new(0));
            let clocked_frames = Arc::new(Mutex::new(ClockedFrameState::default()));
            let thread_playback_target_us = Arc::clone(&playback_target_us);
            let thread_stop_decode = Arc::clone(&stop_decode);
            let thread_clocked_frames = Arc::clone(&clocked_frames);

            std::thread::Builder::new().name("bmz-video-decode".to_string()).spawn(move || {
                let result = decode_video_following_playback_time(
                    &path,
                    Arc::clone(&thread_clocked_frames),
                    thread_playback_target_us,
                    thread_stop_decode,
                );
                if let Err(e) = result {
                    mark_clocked_frames_finished(&thread_clocked_frames);
                    tracing::warn!(path = %path.display(), error = %e, "video decode thread error");
                }
            })?;

            return Ok(Self {
                receiver: None,
                clocked_frames: Some(clocked_frames),
                pending: VecDeque::new(),
                current: None,
                finished: false,
                playback_target_us: Some(playback_target_us),
                stop_decode,
            });
        }

        let (sender, receiver) = sync_channel(4);
        let thread_stop_decode = Arc::clone(&stop_decode);
        std::thread::Builder::new().name("bmz-video-decode".to_string()).spawn(move || {
            if !thread_stop_decode.load(Ordering::Acquire)
                && let Err(e) = decode_video(&path, sender)
            {
                tracing::warn!(path = %path.display(), error = %e, "video decode thread error");
            }
        })?;

        Ok(Self {
            receiver: Some(receiver),
            clocked_frames: None,
            pending: VecDeque::new(),
            current: None,
            finished: false,
            playback_target_us: None,
            stop_decode,
        })
    }

    /// チャンネルをdrainして `video_offset_us` 以下の最新フレームを返す。
    pub fn poll_frame(&mut self, video_offset_us: i64) -> Option<&DecodedFrame> {
        let follows_playback_time = self.playback_target_us.is_some();
        if let Some(target) = &self.playback_target_us {
            target.store(video_offset_us, Ordering::Release);
        }

        if follows_playback_time {
            return self.poll_clocked_frame();
        }

        let Some(receiver) = self.receiver.as_ref() else {
            return self.current.as_ref();
        };

        // チャンネルから利用可能なフレームをすべて受信する。
        // 受信時点ですでに表示期限を過ぎている frame は pending に積まず、
        // 最新候補だけへ畳み込む。decoder 出力は presentation order なので、
        // 新しく受信した due frame は既存 pending の due frame より新しい。
        let mut latest_received_due = None;
        loop {
            match receiver.try_recv() {
                Ok(frame) if frame.pts_us <= video_offset_us => {
                    latest_received_due = Some(frame);
                }
                Ok(frame) => self.pending.push_back(frame),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.finished = true;
                    break;
                }
            }
        }

        let latest_due = if latest_received_due.is_some() {
            while self.pending.front().is_some_and(|frame| frame.pts_us <= video_offset_us) {
                self.pending.pop_front();
            }
            latest_received_due
        } else {
            // video_offset_us 以下のフレームのうち最新のものだけを current に設定する。
            // decoder が先行している時に、古い大きな RGBA buffer を current へ何度も
            // 入れ替えず、表示されない pending frame はその場で捨てる。
            while self.pending.get(1).is_some_and(|frame| frame.pts_us <= video_offset_us) {
                self.pending.pop_front();
            }
            if self.pending.front().is_some_and(|frame| frame.pts_us <= video_offset_us) {
                self.pending.pop_front()
            } else {
                None
            }
        };
        if let Some(frame) = latest_due {
            self.current = Some(frame);
        }

        self.current.as_ref()
    }

    fn poll_clocked_frame(&mut self) -> Option<&DecodedFrame> {
        let Some(frames) = self.clocked_frames.as_ref() else {
            return self.current.as_ref();
        };
        let Ok(mut state) = frames.lock() else {
            self.finished = true;
            return self.current.as_ref();
        };
        if let Some(frame) = state.frame.take()
            && let Some(previous) = self.current.replace(frame)
        {
            recycle_clocked_rgba(&mut state, previous.rgba);
        }
        if state.finished && state.frame.is_none() {
            self.finished = true;
        }
        self.current.as_ref()
    }

    pub fn is_finished(&self) -> bool {
        self.finished && self.pending.is_empty()
    }
}

impl Drop for VideoBgaDecoder {
    fn drop(&mut self) {
        self.stop_decode.store(true, Ordering::Release);
    }
}

pub fn decode_first_frame(path: &Path) -> Result<DecodedFrame> {
    bmz_ffmpeg::ensure_init().map_err(|e| anyhow::anyhow!(e))?;

    let mut ictx = ffmpeg_next::format::input(path)?;
    let selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;
    let mut decoded = ffmpeg_next::frame::Video::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != selected.index {
            continue;
        }
        decoder.send_packet(&packet)?;
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                return rgba_frame_from_video(
                    &decoded,
                    selected.time_base_num,
                    selected.time_base_den,
                );
            }
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN }) => {}
            Err(ffmpeg_next::Error::Eof) => {
                return Err(anyhow::anyhow!("video ended before first frame"));
            }
            Err(e) => return Err(e.into()),
        }
    }

    decoder.send_eof()?;
    match decoder.receive_frame(&mut decoded) {
        Ok(()) => rgba_frame_from_video(&decoded, selected.time_base_num, selected.time_base_den),
        Err(e) => Err(e.into()),
    }
}

fn decode_video(path: &Path, sender: SyncSender<DecodedFrame>) -> Result<()> {
    decode_video_frames(path, |frame| {
        if sender.send(frame).is_err() {
            // receiver が drop された
            return Ok(DecodeFrameControl::Stop);
        }
        Ok(DecodeFrameControl::Continue)
    })
}

fn decode_video_following_playback_time(
    path: &Path,
    clocked_frames: Arc<Mutex<ClockedFrameState>>,
    playback_target_us: Arc<AtomicI64>,
    stop_decode: Arc<AtomicBool>,
) -> Result<()> {
    let mut loop_base_us = 0;
    while !stop_decode.load(Ordering::Acquire) {
        let mut ictx = ffmpeg_next::format::input(path)?;
        let selected = select_video_stream(&ictx)?;
        let mut decoder = open_video_decoder(&selected)?;
        let mut scaler: Option<ffmpeg_next::software::scaling::context::Context> = None;
        let mut decoded = ffmpeg_next::frame::Video::empty();
        let mut decoded_any = false;
        let mut last_pts_us = None;
        let mut stopped = false;

        for (stream, packet) in ictx.packets() {
            if stop_decode.load(Ordering::Acquire) {
                stopped = true;
                break;
            }
            if stream.index() != selected.index {
                continue;
            }
            decoder.send_packet(&packet)?;
            stopped = drain_clocked_decoder_frames(
                &mut decoder,
                &mut decoded,
                &selected,
                loop_base_us,
                &mut scaler,
                &clocked_frames,
                &playback_target_us,
                &stop_decode,
                &mut decoded_any,
                &mut last_pts_us,
            )?;
            if stopped {
                break;
            }
        }

        if !stopped {
            decoder.send_eof()?;
            stopped = drain_clocked_decoder_frames(
                &mut decoder,
                &mut decoded,
                &selected,
                loop_base_us,
                &mut scaler,
                &clocked_frames,
                &playback_target_us,
                &stop_decode,
                &mut decoded_any,
                &mut last_pts_us,
            )?;
        }

        if !decoded_any {
            break;
        }
        if stopped {
            break;
        }
        let target_us = playback_target_us.load(Ordering::Acquire);
        loop_base_us = last_pts_us.unwrap_or(loop_base_us).saturating_add(1).max(target_us);
    }
    mark_clocked_frames_finished(&clocked_frames);
    Ok(())
}

enum DecodeFrameControl {
    Continue,
    Stop,
}

fn decode_video_frames<F>(path: &Path, mut on_frame: F) -> Result<()>
where
    F: FnMut(DecodedFrame) -> Result<DecodeFrameControl>,
{
    let mut ictx = ffmpeg_next::format::input(path)?;

    let selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;

    // スケーラは最初のフレーム受信後に lazily 作成
    let mut scaler: Option<ffmpeg_next::software::scaling::context::Context> = None;

    let mut decoded = ffmpeg_next::frame::Video::empty();

    for (stream, packet) in ictx.packets() {
        if stream.index() != selected.index {
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
                selected.time_base_num,
                selected.time_base_den,
                &mut scaler,
                None,
            )?;

            if matches!(on_frame(frame)?, DecodeFrameControl::Stop) {
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

        let frame = rgba_frame_from_video_with_scaler(
            &decoded,
            selected.time_base_num,
            selected.time_base_den,
            &mut scaler,
            None,
        )?;
        if matches!(on_frame(frame)?, DecodeFrameControl::Stop) {
            return Ok(());
        }
    }

    Ok(())
}

fn publish_clocked_frame(
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

fn mark_clocked_frames_finished(clocked_frames: &Mutex<ClockedFrameState>) {
    if let Ok(mut state) = clocked_frames.lock() {
        state.finished = true;
    }
}

fn drain_clocked_decoder_frames(
    decoder: &mut ffmpeg_next::decoder::Video,
    decoded: &mut ffmpeg_next::frame::Video,
    selected: &SelectedVideoStream,
    loop_base_us: i64,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    clocked_frames: &Mutex<ClockedFrameState>,
    playback_target_us: &AtomicI64,
    stop_decode: &AtomicBool,
    decoded_any: &mut bool,
    last_pts_us: &mut Option<i64>,
) -> Result<bool> {
    loop {
        match decoder.receive_frame(decoded) {
            Ok(()) => {}
            Err(ffmpeg_next::Error::Other { errno: ffmpeg_next::error::EAGAIN })
            | Err(ffmpeg_next::Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }

        if stop_decode.load(Ordering::Acquire) {
            return Ok(true);
        }

        *decoded_any = true;
        let pts_us = video_frame_pts_us(decoded, selected.time_base_num, selected.time_base_den)
            .saturating_add(loop_base_us);
        *last_pts_us = Some(pts_us);
        if should_skip_clocked_frame_conversion(pts_us, playback_target_us.load(Ordering::Acquire))
        {
            continue;
        }

        let publish_after_us = pts_us.saturating_sub(CLOCKED_FRAME_PUBLISH_LEAD_US);
        if publish_after_us > playback_target_us.load(Ordering::Acquire)
            && !wait_until_playback_reaches_frame(playback_target_us, stop_decode, publish_after_us)
        {
            return Ok(true);
        }

        let mut frame = rgba_frame_from_video_with_scaler(
            decoded,
            selected.time_base_num,
            selected.time_base_den,
            scaler,
            take_clocked_recycled_rgba(clocked_frames),
        )?;
        frame.pts_us = pts_us;
        publish_clocked_frame(clocked_frames, frame)?;
    }
    Ok(false)
}

fn take_clocked_recycled_rgba(clocked_frames: &Mutex<ClockedFrameState>) -> Option<Vec<u8>> {
    clocked_frames.lock().ok().and_then(|mut state| state.recycled_rgba.pop())
}

fn recycle_clocked_rgba(state: &mut ClockedFrameState, mut rgba: Vec<u8>) {
    const MAX_RECYCLED_RGBA_BUFFERS: usize = 2;
    if state.recycled_rgba.len() < MAX_RECYCLED_RGBA_BUFFERS {
        rgba.clear();
        state.recycled_rgba.push(rgba);
    }
}

fn should_skip_clocked_frame_conversion(frame_pts_us: i64, playback_target_us: i64) -> bool {
    frame_pts_us.saturating_add(CLOCKED_FRAME_CATCH_UP_TOLERANCE_US) < playback_target_us
}

fn wait_until_playback_reaches_frame(
    playback_target_us: &AtomicI64,
    stop_decode: &AtomicBool,
    frame_pts_us: i64,
) -> bool {
    loop {
        if stop_decode.load(Ordering::Acquire) {
            return false;
        }
        let target_us = playback_target_us.load(Ordering::Acquire);
        if target_us >= frame_pts_us {
            return true;
        }
        let sleep_us = (frame_pts_us - target_us).clamp(1_000, CLOCKED_FRAME_WAIT_MAX_SLEEP_US);
        std::thread::sleep(Duration::from_micros(sleep_us as u64));
    }
}

fn select_video_stream(ictx: &ffmpeg_next::format::context::Input) -> Result<SelectedVideoStream> {
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
    let stream = ictx
        .stream(stream_index)
        .ok_or_else(|| anyhow::anyhow!("selected video stream not available"))?;
    let tb = stream.time_base();
    tracing::debug!(stream_index, best_index, "selected video stream for BGA decode");
    Ok(SelectedVideoStream {
        index: stream_index,
        time_base_num: tb.numerator() as i64,
        time_base_den: tb.denominator() as i64,
        codec_params,
    })
}

fn open_video_decoder(selected: &SelectedVideoStream) -> Result<ffmpeg_next::decoder::Video> {
    let mut context =
        ffmpeg_next::codec::context::Context::from_parameters(selected.codec_params.clone())?;
    context.set_threading(ffmpeg_next::codec::threading::Config::kind(
        ffmpeg_next::codec::threading::Type::Frame,
    ));
    Ok(context.decoder().video()?)
}

fn video_stream_bit_rate(params: &ffmpeg_next::codec::Parameters) -> usize {
    ffmpeg_next::codec::context::Context::from_parameters(params.clone())
        .and_then(|context| context.decoder().video())
        .map(|decoder| decoder.bit_rate())
        .unwrap_or(0)
}

fn choose_beatoraja_video_stream(
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

fn rgba_frame_from_video(
    decoded: &ffmpeg_next::frame::Video,
    time_base_num: i64,
    time_base_den: i64,
) -> Result<DecodedFrame> {
    let mut scaler = None;
    rgba_frame_from_video_with_scaler(decoded, time_base_num, time_base_den, &mut scaler, None)
}

fn rgba_frame_from_video_with_scaler(
    decoded: &ffmpeg_next::frame::Video,
    time_base_num: i64,
    time_base_den: i64,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    rgba_buffer: Option<Vec<u8>>,
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
            ffmpeg_next::software::scaling::flag::Flags::FAST_BILINEAR,
        )?);
    }

    let mut rgba_frame = ffmpeg_next::frame::Video::empty();
    scaler.as_mut().unwrap().run(decoded, &mut rgba_frame)?;

    let pts_us = video_frame_pts_us(decoded, time_base_num, time_base_den);

    let data = rgba_frame.data(0);
    let stride = rgba_frame.stride(0);
    let row_bytes = (w as usize) * 4;
    let rgba = copy_rgba_frame_data_with_buffer(data, stride, row_bytes, h as usize, rgba_buffer);

    Ok(DecodedFrame { pts_us, rgba, width: w, height: h })
}

fn video_frame_pts_us(
    decoded: &ffmpeg_next::frame::Video,
    time_base_num: i64,
    time_base_den: i64,
) -> i64 {
    let pts_raw = decoded.pts().unwrap_or(0);
    if time_base_den != 0 { pts_raw * time_base_num * 1_000_000 / time_base_den } else { 0 }
}

#[cfg(test)]
fn copy_rgba_frame_data(data: &[u8], stride: usize, row_bytes: usize, rows: usize) -> Vec<u8> {
    copy_rgba_frame_data_with_buffer(data, stride, row_bytes, rows, None)
}

fn copy_rgba_frame_data_with_buffer(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts_us: i64) -> DecodedFrame {
        DecodedFrame { pts_us, rgba: vec![pts_us as u8], width: 1, height: 1 }
    }

    fn decoder_with_pending(pending: impl IntoIterator<Item = i64>) -> VideoBgaDecoder {
        let (_sender, receiver) = sync_channel(1);
        VideoBgaDecoder {
            receiver: Some(receiver),
            clocked_frames: None,
            pending: pending.into_iter().map(frame).collect(),
            current: Some(frame(0)),
            finished: false,
            playback_target_us: None,
            stop_decode: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn choose_beatoraja_video_stream_keeps_best_when_bitrate_is_valid() {
        let selected = choose_beatoraja_video_stream(1, [(0, 100), (1, 10), (2, 100), (6, 100)]);

        assert_eq!(selected, 1);
    }

    #[test]
    fn choose_beatoraja_video_stream_advances_from_low_bitrate_best() {
        let selected = choose_beatoraja_video_stream(0, [(0, 0), (1, 0), (2, 100), (6, 100)]);

        assert_eq!(selected, 2);
    }

    #[test]
    fn choose_beatoraja_video_stream_falls_back_to_best_when_no_valid_retry_exists() {
        let selected = choose_beatoraja_video_stream(0, [(0, 0), (1, 0), (6, 100)]);

        assert_eq!(selected, 0);
    }

    #[test]
    fn decode_first_frame_reads_data_song_video_fixture() {
        let frame = decode_first_frame(&repo_root().join("data/songs/bga-compat/movie.webm"))
            .expect("fixture movie must decode");

        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 2);
        assert_eq!(frame.rgba.len(), 2 * 2 * 4);
    }

    fn decoder_with_channel(
        pending: impl IntoIterator<Item = i64>,
    ) -> (SyncSender<DecodedFrame>, VideoBgaDecoder) {
        let (sender, receiver) = sync_channel(4);
        let decoder = VideoBgaDecoder {
            receiver: Some(receiver),
            clocked_frames: None,
            pending: pending.into_iter().map(frame).collect(),
            current: Some(frame(0)),
            finished: false,
            playback_target_us: None,
            stop_decode: Arc::new(AtomicBool::new(false)),
        };
        (sender, decoder)
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
    fn poll_frame_compacts_received_overdue_frames_before_pending_queue() {
        let (sender, mut decoder) = decoder_with_channel([]);
        sender.send(frame(10)).unwrap();
        sender.send(frame(20)).unwrap();
        sender.send(frame(30)).unwrap();

        let frame = decoder.poll_frame(25).unwrap();

        assert_eq!(frame.pts_us, 20);
        assert_eq!(decoder.pending.len(), 1);
        assert_eq!(decoder.pending.front().unwrap().pts_us, 30);
    }

    #[test]
    fn poll_frame_prefers_newer_received_due_frame_over_pending_due_frames() {
        let (sender, mut decoder) = decoder_with_channel([10, 20, 30]);
        sender.send(frame(40)).unwrap();
        sender.send(frame(50)).unwrap();

        let frame = decoder.poll_frame(45).unwrap();

        assert_eq!(frame.pts_us, 40);
        assert_eq!(decoder.pending.len(), 1);
        assert_eq!(decoder.pending.front().unwrap().pts_us, 50);
    }

    #[test]
    fn poll_frame_updates_playback_target_for_clocked_decoder() {
        let target = Arc::new(AtomicI64::new(0));
        let mut decoder = VideoBgaDecoder {
            receiver: None,
            clocked_frames: Some(Arc::new(Mutex::new(ClockedFrameState::default()))),
            pending: VecDeque::new(),
            current: None,
            finished: false,
            playback_target_us: Some(Arc::clone(&target)),
            stop_decode: Arc::new(AtomicBool::new(false)),
        };

        assert!(decoder.poll_frame(123_456).is_none());

        assert_eq!(target.load(Ordering::Acquire), 123_456);
    }

    #[test]
    fn clocked_poll_accepts_received_frame_without_pts_gate() {
        let frames = Arc::new(Mutex::new(ClockedFrameState::default()));
        let target = Arc::new(AtomicI64::new(0));
        let mut decoder = VideoBgaDecoder {
            receiver: None,
            clocked_frames: Some(Arc::clone(&frames)),
            pending: VecDeque::new(),
            current: None,
            finished: false,
            playback_target_us: Some(Arc::clone(&target)),
            stop_decode: Arc::new(AtomicBool::new(false)),
        };
        publish_clocked_frame(&frames, frame(50_000)).unwrap();

        let frame = decoder.poll_frame(10_000).unwrap();

        assert_eq!(frame.pts_us, 50_000);
        assert_eq!(target.load(Ordering::Acquire), 10_000);
    }

    #[test]
    fn clocked_publish_keeps_latest_frame_when_consumer_lags() {
        let frames = Arc::new(Mutex::new(ClockedFrameState::default()));

        publish_clocked_frame(&frames, frame(10)).unwrap();
        publish_clocked_frame(&frames, frame(20)).unwrap();

        let mut state = frames.lock().unwrap();
        assert_eq!(state.frame.take().unwrap().pts_us, 20);
        assert_eq!(state.recycled_rgba.len(), 1);
    }

    #[test]
    fn mark_clocked_frames_finished_preserves_last_frame() {
        let frames = Arc::new(Mutex::new(ClockedFrameState::default()));
        publish_clocked_frame(&frames, frame(10)).unwrap();

        mark_clocked_frames_finished(&frames);

        let state = frames.lock().unwrap();
        assert!(state.finished);
        assert_eq!(state.frame.as_ref().unwrap().pts_us, 10);
    }

    #[test]
    fn clocked_conversion_skip_only_drops_stale_frames() {
        assert!(should_skip_clocked_frame_conversion(10_000, 20_000));
        assert!(!should_skip_clocked_frame_conversion(12_000, 20_000));
        assert!(!should_skip_clocked_frame_conversion(25_000, 20_000));
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

    #[test]
    fn copy_rgba_frame_data_reuses_supplied_buffer() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let buffer = Vec::with_capacity(64);

        let copied = copy_rgba_frame_data_with_buffer(&data, 4, 4, 2, Some(buffer));

        assert_eq!(copied, data);
        assert!(copied.capacity() >= 64);
    }

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }
}
