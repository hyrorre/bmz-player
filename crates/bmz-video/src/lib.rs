use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
    mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
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

/// Container/stream duration used as an exact loop period during offline export.
pub fn video_duration_us(path: &Path) -> Result<i64> {
    bmz_ffmpeg::ensure_init().map_err(|e| anyhow::anyhow!(e))?;
    let input = ffmpeg_next::format::input(path)?;
    let selected = select_video_stream(&input)?;
    let stream =
        input.stream(selected.index).ok_or_else(|| anyhow::anyhow!("video stream missing"))?;
    let duration = if stream.duration() > 0 && selected.time_base_den > 0 {
        (i128::from(stream.duration()) * i128::from(selected.time_base_num) * 1_000_000
            / i128::from(selected.time_base_den))
        .min(i128::from(i64::MAX)) as i64
    } else {
        input.duration()
    };
    anyhow::ensure!(
        duration > 0,
        "video duration unavailable for offline loop: {}",
        path.display()
    );
    Ok(duration)
}

pub struct VideoBgaDecoder {
    path: PathBuf,
    follow_playback_time: bool,
    receiver: Option<Receiver<QueuedDecodedFrame>>,
    clocked_frames: Option<Arc<Mutex<ClockedFrameState>>>,
    pending: VecDeque<DecodedFrame>,
    current: Option<DecodedFrame>,
    finished: bool,
    playback_target_us: Arc<AtomicI64>,
    decode_generation: Arc<AtomicU64>,
    stop_decode: Arc<AtomicBool>,
    /// Channel-mode: decode thread seeks to start and begins a new pass.
    restart_decode: Arc<AtomicBool>,
    /// Channel-mode: current pass reached EOF without disconnecting the receiver.
    pass_finished: Arc<AtomicBool>,
    decode_thread: Option<std::thread::JoinHandle<()>>,
}

struct QueuedDecodedFrame {
    generation: u64,
    frame: DecodedFrame,
}

struct SelectedVideoStream {
    index: usize,
    time_base_num: i64,
    time_base_den: i64,
    start_time_raw: Option<i64>,
    codec_params: ffmpeg_next::codec::Parameters,
}

#[derive(Debug, Default)]
struct VideoTimestampNormalizer {
    origin_raw: Option<i64>,
    last_us: i64,
}

impl VideoTimestampNormalizer {
    fn with_origin_raw(origin_raw: Option<i64>) -> Self {
        Self { origin_raw, last_us: 0 }
    }

    fn frame_pts_us(
        &mut self,
        decoded: &ffmpeg_next::frame::Video,
        time_base_num: i64,
        time_base_den: i64,
    ) -> i64 {
        self.timestamp_us(
            decoded.timestamp().or_else(|| decoded.pts()),
            time_base_num,
            time_base_den,
        )
    }

    fn timestamp_us(
        &mut self,
        timestamp_raw: Option<i64>,
        time_base_num: i64,
        time_base_den: i64,
    ) -> i64 {
        if time_base_den == 0 {
            return self.last_us;
        }
        let Some(timestamp_raw) = timestamp_raw else {
            return self.last_us;
        };
        let origin_raw = *self.origin_raw.get_or_insert(timestamp_raw);
        let elapsed_raw = i128::from(timestamp_raw) - i128::from(origin_raw);
        let elapsed_us =
            elapsed_raw.saturating_mul(i128::from(time_base_num)).saturating_mul(1_000_000)
                / i128::from(time_base_den);
        self.last_us = elapsed_us.clamp(0, i128::from(i64::MAX)) as i64;
        self.last_us
    }
}

#[derive(Default)]
struct VideoDecodeContext {
    scaler: Option<ffmpeg_next::software::scaling::context::Context>,
    timestamp_normalizer: VideoTimestampNormalizer,
}

#[derive(Default)]
struct ClockedFrameState {
    frame: Option<DecodedFrame>,
    finished: bool,
    recycled_rgba: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockedDrainStatus {
    Continue,
    Restart,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClockedFrameWait {
    Reached,
    Rewound,
    Stopped,
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

        let path_buf = path.to_path_buf();
        let stop_decode = Arc::new(AtomicBool::new(false));
        let restart_decode = Arc::new(AtomicBool::new(false));
        let pass_finished = Arc::new(AtomicBool::new(false));
        let playback_target_us = Arc::new(AtomicI64::new(0));
        let decode_generation = Arc::new(AtomicU64::new(0));
        if follow_playback_time {
            let clocked_frames = Arc::new(Mutex::new(ClockedFrameState::default()));
            let thread_playback_target_us = Arc::clone(&playback_target_us);
            let thread_stop_decode = Arc::clone(&stop_decode);
            let thread_restart_decode = Arc::clone(&restart_decode);
            let thread_clocked_frames = Arc::clone(&clocked_frames);
            let path = path_buf.clone();

            let decode_thread = std::thread::Builder::new()
                .name("bmz-video-decode".to_string())
                .spawn(move || {
                    let result = decode_video_following_playback_time(
                        &path,
                        Arc::clone(&thread_clocked_frames),
                        thread_playback_target_us,
                        thread_stop_decode,
                        thread_restart_decode,
                    );
                    if let Err(e) = result {
                        mark_clocked_frames_finished(&thread_clocked_frames);
                        tracing::warn!(path = %path.display(), error = %e, "video decode thread error");
                    }
                })?;

            return Ok(Self {
                path: path_buf,
                follow_playback_time: true,
                receiver: None,
                clocked_frames: Some(clocked_frames),
                pending: VecDeque::new(),
                current: None,
                finished: false,
                playback_target_us,
                decode_generation,
                stop_decode,
                restart_decode,
                pass_finished,
                decode_thread: Some(decode_thread),
            });
        }

        let (sender, receiver) = sync_channel(4);
        let thread_stop_decode = Arc::clone(&stop_decode);
        let thread_restart_decode = Arc::clone(&restart_decode);
        let thread_pass_finished = Arc::clone(&pass_finished);
        let thread_playback_target_us = Arc::clone(&playback_target_us);
        let thread_decode_generation = Arc::clone(&decode_generation);
        let path = path_buf.clone();
        let decode_thread =
            std::thread::Builder::new().name("bmz-video-decode".to_string()).spawn(move || {
                if let Err(e) = decode_video_restartable(
                    &path,
                    sender,
                    thread_stop_decode,
                    thread_restart_decode,
                    thread_pass_finished,
                    thread_playback_target_us,
                    thread_decode_generation,
                ) {
                    tracing::warn!(path = %path.display(), error = %e, "video decode thread error");
                }
            })?;

        Ok(Self {
            path: path_buf,
            follow_playback_time: false,
            receiver: Some(receiver),
            clocked_frames: None,
            pending: VecDeque::new(),
            current: None,
            finished: false,
            playback_target_us,
            decode_generation,
            stop_decode,
            restart_decode,
            pass_finished,
            decode_thread: Some(decode_thread),
        })
    }

    /// Seek to the start and begin decoding from the first frame again.
    ///
    /// Keeps the decode thread and ffmpeg input open (beatoraja `stop`/`play` style).
    /// Channel mode signals the worker to rewind; clocked mode rewinds via playback target.
    pub fn restart(&mut self) {
        self.restart_at(0);
    }

    /// Seek directly to the keyframe at or before `video_offset_us`, then decode forward.
    ///
    /// Channel mode uses FFmpeg seek on the existing input. Clocked mode keeps its existing
    /// rewind-and-rebase behavior because it is used by looping skin movie sources.
    pub fn restart_at(&mut self, video_offset_us: i64) {
        let video_offset_us = video_offset_us.max(0);
        if !self.follow_playback_time {
            // receiver drain と worker の blocked send は並行するため、古い pass の frame が
            // drain 直後に 1 枚だけ到着し得る。generation を先に進めて poll 側で捨てる。
            self.decode_generation.fetch_add(1, Ordering::AcqRel);
        }
        self.pending.clear();
        self.current = None;
        self.finished = false;
        self.pass_finished.store(false, Ordering::Release);
        self.playback_target_us.store(video_offset_us, Ordering::Release);

        if self.follow_playback_time {
            if let Some(frames) = self.clocked_frames.as_ref()
                && let Ok(mut state) = frames.lock()
            {
                if let Some(previous) = state.frame.take() {
                    recycle_clocked_rgba(&mut state, previous.rgba);
                }
                state.finished = false;
            }
            self.restart_decode.store(true, Ordering::Release);
            return;
        }

        if let Some(receiver) = self.receiver.as_ref() {
            while receiver.try_recv().is_ok() {}
        }
        self.restart_decode.store(true, Ordering::Release);
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Offline selection waits for the first future PTS or EOF, so decode speed
    /// cannot affect the selected picture. Use `open`, not the live clock mode.
    pub fn frame_at_blocking(&mut self, video_offset_us: i64) -> Result<Option<&DecodedFrame>> {
        anyhow::ensure!(!self.follow_playback_time, "offline selection requires channel mode");
        while self.pending.front().is_some_and(|f| f.pts_us <= video_offset_us) {
            self.current = self.pending.pop_front();
        }
        while self.pending.is_empty() && !self.finished {
            let receiver = self.receiver.as_ref().expect("channel decoder");
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(queued)
                    if queued.generation != self.decode_generation.load(Ordering::Acquire) => {}
                Ok(queued) if queued.frame.pts_us <= video_offset_us => {
                    self.current = Some(queued.frame)
                }
                Ok(queued) => self.pending.push_back(queued.frame),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.pass_finished.load(Ordering::Acquire) {
                        self.finished = true;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("video decoder failed: {}", self.path.display());
                }
            }
        }
        Ok(self.current.as_ref())
    }

    /// チャンネルをdrainして `video_offset_us` 以下の最新フレームを返す。
    pub fn poll_frame(&mut self, video_offset_us: i64) -> Option<&DecodedFrame> {
        self.playback_target_us.store(video_offset_us, Ordering::Release);

        if self.follow_playback_time {
            return self.poll_clocked_frame();
        }

        let Some(receiver) = self.receiver.as_ref() else {
            return self.current.as_ref();
        };

        // video_offset_us 以下の pending frame は最新候補だけへ畳み込む。
        // 通常は future frame を 1 枚だけ pending に置くが、旧状態や test helper が
        // 複数枚を持っていても presentation order のまま安全に compact する。
        let mut latest_due = None;
        while self.pending.front().is_some_and(|frame| frame.pts_us <= video_offset_us) {
            latest_due = self.pending.pop_front();
        }

        // future frame がすでに pending にある間は receiver を drain しない。
        // 最初の future frame を受信した時点でも止め、残りを bounded channel 側に
        // 留める。ここで全件を pending へ移すと sync_channel(4) の backpressure が
        // 実質解除され、高解像度動画の RGBA frame を動画末尾まで先読みしてしまう。
        if self.pending.is_empty() {
            let decode_generation = self.decode_generation.load(Ordering::Acquire);
            loop {
                match receiver.try_recv() {
                    Ok(queued) if queued.generation != decode_generation => continue,
                    Ok(QueuedDecodedFrame { frame, .. }) if frame.pts_us <= video_offset_us => {
                        latest_due = Some(frame);
                    }
                    Ok(QueuedDecodedFrame { frame, .. }) => {
                        self.pending.push_back(frame);
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.finished = true;
                        break;
                    }
                }
            }
        }

        if let Some(frame) = latest_due {
            self.current = Some(frame);
        }

        if self.pass_finished.load(Ordering::Acquire) && self.pending.is_empty() {
            self.finished = true;
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
        // channel モードは receiver を先に落とし、sync_channel の send block を
        // エラーで解いてから join する。clocked モードは stop_decode を数 ms
        // 間隔で確認するため、どちらも join はすぐ返る。join しないままだと
        // 譜面切り替えのたびに ffmpeg リソースを持った detached thread が残る。
        drop(self.receiver.take());
        if let Some(handle) = self.decode_thread.take()
            && handle.join().is_err()
        {
            tracing::warn!("video decode thread panicked before join");
        }
    }
}

pub fn decode_first_frame(path: &Path) -> Result<DecodedFrame> {
    bmz_ffmpeg::ensure_init().map_err(|e| anyhow::anyhow!(e))?;

    let mut ictx = ffmpeg_next::format::input(path)?;
    let selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;
    let mut decoded = ffmpeg_next::frame::Video::empty();
    let mut timestamp_normalizer = VideoTimestampNormalizer::default();

    for (stream, packet) in ictx.packets() {
        if stream.index() != selected.index {
            continue;
        }
        decoder.send_packet(&packet)?;
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                let pts_us = timestamp_normalizer.frame_pts_us(
                    &decoded,
                    selected.time_base_num,
                    selected.time_base_den,
                );
                return rgba_frame_from_video(&decoded, pts_us);
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
        Ok(()) => {
            let pts_us = timestamp_normalizer.frame_pts_us(
                &decoded,
                selected.time_base_num,
                selected.time_base_den,
            );
            rgba_frame_from_video(&decoded, pts_us)
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelDecodePassEnd {
    Stop,
    Restart,
    Eof,
}

#[derive(Debug, Clone, Copy)]
struct ChannelDecodePassStart {
    generation: u64,
    direct_seeked: bool,
}

struct ChannelFrameCatchUp<T> {
    published_any: bool,
    last_skipped: Option<T>,
}

impl<T> Default for ChannelFrameCatchUp<T> {
    fn default() -> Self {
        Self { published_any: false, last_skipped: None }
    }
}

impl<T> ChannelFrameCatchUp<T> {
    fn should_skip(&self, pts_us: i64, playback_target_us: i64) -> bool {
        self.published_any && should_skip_frame_conversion(pts_us, playback_target_us)
    }

    fn record_skipped(&mut self, frame: T) {
        self.last_skipped = Some(frame);
    }

    fn record_published(&mut self) {
        self.published_any = true;
        self.last_skipped = None;
    }

    fn take_last_skipped(&mut self) -> Option<T> {
        self.last_skipped.take()
    }
}

#[path = "decode/channel.rs"]
mod channel_decode;
#[path = "decode/clocked.rs"]
mod clocked_decode;
#[path = "decode/ffmpeg.rs"]
mod ffmpeg_decode;

use channel_decode::*;
use clocked_decode::*;
use ffmpeg_decode::*;

#[cfg(test)]
#[path = "decode/tests.rs"]
mod tests;
