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
    codec_params: ffmpeg_next::codec::Parameters,
}

#[derive(Debug, Default)]
struct VideoTimestampNormalizer {
    origin_raw: Option<i64>,
    last_us: i64,
}

impl VideoTimestampNormalizer {
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
        if !self.follow_playback_time {
            // receiver drain と worker の blocked send は並行するため、古い pass の frame が
            // drain 直後に 1 枚だけ到着し得る。generation を先に進めて poll 側で捨てる。
            self.decode_generation.fetch_add(1, Ordering::AcqRel);
        }
        self.pending.clear();
        self.current = None;
        self.finished = false;
        self.pass_finished.store(false, Ordering::Release);
        self.playback_target_us.store(0, Ordering::Release);

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

fn decode_video_restartable(
    path: &Path,
    sender: SyncSender<QueuedDecodedFrame>,
    stop_decode: Arc<AtomicBool>,
    restart_decode: Arc<AtomicBool>,
    pass_finished: Arc<AtomicBool>,
    playback_target_us: Arc<AtomicI64>,
    decode_generation: Arc<AtomicU64>,
) -> Result<()> {
    let mut ictx = ffmpeg_next::format::input(path)?;
    let selected = select_video_stream(&ictx)?;
    let mut decoder = open_video_decoder(&selected)?;
    let mut scaler = None;
    let mut decoded = ffmpeg_next::frame::Video::empty();
    let mut first_pass = true;

    loop {
        if stop_decode.load(Ordering::Acquire) {
            return Ok(());
        }

        if !first_pass {
            rewind_video_decoder(&mut ictx, &mut decoder)?;
        }
        first_pass = false;
        restart_decode.store(false, Ordering::Release);
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
            generation,
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

fn decode_video_channel_pass(
    ictx: &mut ffmpeg_next::format::context::Input,
    decoder: &mut ffmpeg_next::decoder::Video,
    selected: &SelectedVideoStream,
    scaler: &mut Option<ffmpeg_next::software::scaling::context::Context>,
    decoded: &mut ffmpeg_next::frame::Video,
    sender: &SyncSender<QueuedDecodedFrame>,
    stop_decode: &AtomicBool,
    restart_decode: &AtomicBool,
    playback_target_us: &AtomicI64,
    generation: u64,
) -> Result<ChannelDecodePassEnd> {
    let mut timestamp_normalizer = VideoTimestampNormalizer::default();
    let mut catch_up = ChannelFrameCatchUp::default();

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

fn publish_last_skipped_channel_frame(
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

fn send_decoded_frame(
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

fn decode_video_following_playback_time(
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
    while !stop_decode.load(Ordering::Acquire) {
        if restart_decode.swap(false, Ordering::AcqRel) {
            loop_base_us = playback_target_us.load(Ordering::Acquire);
            decode_context.timestamp_normalizer = VideoTimestampNormalizer::default();
            rewind_video_decoder(&mut ictx, &mut decoder)?;
        }
        let mut target_us = playback_target_us.load(Ordering::Acquire);
        if clocked_playback_target_rewound(target_us, loop_base_us) {
            loop_base_us = target_us;
            decode_context.timestamp_normalizer = VideoTimestampNormalizer::default();
            rewind_video_decoder(&mut ictx, &mut decoder)?;
        }
        let mut decoded_any = false;
        let mut last_pts_us = None;
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
                &mut decoded_any,
                &mut last_pts_us,
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
                &mut decoded_any,
                &mut last_pts_us,
            )?;
        }

        if drain_status == ClockedDrainStatus::Restart {
            target_us = playback_target_us.load(Ordering::Acquire);
            loop_base_us = target_us;
            rewind_video_decoder(&mut ictx, &mut decoder)?;
            continue;
        }
        if !decoded_any {
            break;
        }
        if drain_status == ClockedDrainStatus::Stop {
            break;
        }
        target_us = playback_target_us.load(Ordering::Acquire);
        loop_base_us = last_pts_us.unwrap_or(loop_base_us).saturating_add(1).max(target_us);
        rewind_video_decoder(&mut ictx, &mut decoder)?;
    }
    mark_clocked_frames_finished(&clocked_frames);
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
    decode_context: &mut VideoDecodeContext,
    clocked_frames: &Mutex<ClockedFrameState>,
    playback_target_us: &AtomicI64,
    stop_decode: &AtomicBool,
    decoded_any: &mut bool,
    last_pts_us: &mut Option<i64>,
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

        *decoded_any = true;
        let pts_us = decode_context
            .timestamp_normalizer
            .frame_pts_us(decoded, selected.time_base_num, selected.time_base_den)
            .saturating_add(loop_base_us);
        *last_pts_us = Some(pts_us);
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

fn should_skip_frame_conversion(frame_pts_us: i64, playback_target_us: i64) -> bool {
    frame_pts_us.saturating_add(CLOCKED_FRAME_CATCH_UP_TOLERANCE_US) < playback_target_us
}

fn clocked_playback_target_rewound(target_us: i64, highest_target_us: i64) -> bool {
    target_us.saturating_add(CLOCKED_FRAME_CATCH_UP_TOLERANCE_US) < highest_target_us
}

fn wait_until_playback_reaches_frame(
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

fn rewind_video_decoder(
    ictx: &mut ffmpeg_next::format::context::Input,
    decoder: &mut ffmpeg_next::decoder::Video,
) -> Result<()> {
    ictx.seek(0, ..)?;
    decoder.flush();
    Ok(())
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

fn rgba_frame_from_video(decoded: &ffmpeg_next::frame::Video, pts_us: i64) -> Result<DecodedFrame> {
    let mut scaler = None;
    rgba_frame_from_video_with_scaler(decoded, pts_us, &mut scaler, None)
}

fn rgba_frame_from_video_with_scaler(
    decoded: &ffmpeg_next::frame::Video,
    pts_us: i64,
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

    let data = rgba_frame.data(0);
    let stride = rgba_frame.stride(0);
    let row_bytes = (w as usize) * 4;
    let rgba = copy_rgba_frame_data_with_buffer(data, stride, row_bytes, h as usize, rgba_buffer);

    Ok(DecodedFrame { pts_us, rgba, width: w, height: h })
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

    fn queued_frame(generation: u64, pts_us: i64) -> QueuedDecodedFrame {
        QueuedDecodedFrame { generation, frame: frame(pts_us) }
    }

    fn decoder_with_pending(pending: impl IntoIterator<Item = i64>) -> VideoBgaDecoder {
        let (_sender, receiver) = sync_channel(1);
        VideoBgaDecoder {
            path: PathBuf::new(),
            follow_playback_time: false,
            receiver: Some(receiver),
            clocked_frames: None,
            pending: pending.into_iter().map(frame).collect(),
            current: Some(frame(0)),
            finished: false,
            playback_target_us: Arc::new(AtomicI64::new(0)),
            decode_generation: Arc::new(AtomicU64::new(0)),
            stop_decode: Arc::new(AtomicBool::new(false)),
            restart_decode: Arc::new(AtomicBool::new(false)),
            pass_finished: Arc::new(AtomicBool::new(false)),
            decode_thread: None,
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
    fn restart_rewinds_channel_decoder_to_first_frame() {
        let path = repo_root().join("data/songs/bga-compat/movie.webm");
        let mut decoder = VideoBgaDecoder::open(&path).expect("fixture movie must open");

        let mut first_pts = None;
        for _ in 0..200 {
            if let Some(frame) = decoder.poll_frame(1_000_000) {
                first_pts = Some(frame.pts_us);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(first_pts, Some(0), "decoder should produce the first frame");

        // Consume any buffered later frames so restart has work to do.
        for _ in 0..50 {
            let _ = decoder.poll_frame(1_000_000);
            std::thread::sleep(Duration::from_millis(2));
        }

        decoder.restart();

        let mut first_after_restart = None;
        for _ in 0..200 {
            if let Some(frame) = decoder.poll_frame(0) {
                first_after_restart = Some(frame.pts_us);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(first_after_restart, Some(0));
    }

    #[test]
    fn channel_catch_up_preserves_latest_skipped_frame_for_eof() {
        let mut catch_up = ChannelFrameCatchUp::default();

        assert!(!catch_up.should_skip(0, 1_000_000));
        catch_up.record_published();
        for pts_us in [100_000, 200_000, 300_000] {
            assert!(catch_up.should_skip(pts_us, 1_000_000));
            catch_up.record_skipped(pts_us);
        }

        assert_eq!(catch_up.take_last_skipped(), Some(300_000));
    }

    #[test]
    fn channel_send_stops_while_queue_is_full() {
        let (sender, _receiver) = sync_channel(1);
        sender.try_send(queued_frame(0, 0)).unwrap();
        let stop_decode = Arc::new(AtomicBool::new(false));
        let restart_decode = Arc::new(AtomicBool::new(false));
        let thread_stop_decode = Arc::clone(&stop_decode);
        let thread_restart_decode = Arc::clone(&restart_decode);
        let handle = std::thread::spawn(move || {
            send_decoded_frame(&sender, frame(1), 0, &thread_stop_decode, &thread_restart_decode)
                .unwrap()
        });

        std::thread::sleep(Duration::from_millis(5));
        stop_decode.store(true, Ordering::Release);

        assert_eq!(handle.join().unwrap(), ChannelDecodePassEnd::Stop);
    }

    #[test]
    fn channel_send_restarts_while_queue_is_full() {
        let (sender, _receiver) = sync_channel(1);
        sender.try_send(queued_frame(0, 0)).unwrap();
        let stop_decode = Arc::new(AtomicBool::new(false));
        let restart_decode = Arc::new(AtomicBool::new(false));
        let thread_stop_decode = Arc::clone(&stop_decode);
        let thread_restart_decode = Arc::clone(&restart_decode);
        let handle = std::thread::spawn(move || {
            send_decoded_frame(&sender, frame(1), 0, &thread_stop_decode, &thread_restart_decode)
                .unwrap()
        });

        std::thread::sleep(Duration::from_millis(5));
        restart_decode.store(true, Ordering::Release);

        assert_eq!(handle.join().unwrap(), ChannelDecodePassEnd::Restart);
    }

    #[test]
    fn decode_first_frame_reads_data_song_video_fixture() {
        let frame = decode_first_frame(&repo_root().join("data/songs/bga-compat/movie.webm"))
            .expect("fixture movie must decode");

        assert_eq!(frame.pts_us, 0);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 2);
        assert_eq!(frame.rgba.len(), 2 * 2 * 4);
    }

    #[test]
    fn video_timestamp_normalizer_starts_nonzero_timestamps_at_zero() {
        let mut normalizer = VideoTimestampNormalizer::default();

        assert_eq!(normalizer.timestamp_us(Some(48_003), 1, 90_000), 0);
        assert_eq!(normalizer.timestamp_us(Some(51_006), 1, 90_000), 33_366);
        assert_eq!(normalizer.timestamp_us(None, 1, 90_000), 33_366);
        assert_eq!(normalizer.timestamp_us(Some(48_003), 1, 90_000), 0);
    }

    #[test]
    fn video_timestamp_normalizer_handles_missing_and_invalid_timestamps() {
        let mut normalizer = VideoTimestampNormalizer::default();

        assert_eq!(normalizer.timestamp_us(None, 1, 90_000), 0);
        assert_eq!(normalizer.timestamp_us(Some(10), 1, 0), 0);
        assert_eq!(normalizer.timestamp_us(Some(10), 1, 90_000), 0);
        assert_eq!(normalizer.timestamp_us(Some(5), 1, 90_000), 0);
    }

    #[test]
    fn video_timestamp_normalizer_saturates_extreme_values() {
        let mut normalizer = VideoTimestampNormalizer::default();

        assert_eq!(normalizer.timestamp_us(Some(i64::MIN), 1, 1), 0);
        assert_eq!(normalizer.timestamp_us(Some(i64::MAX), i64::MAX, 1), i64::MAX);
    }

    fn decoder_with_channel(
        pending: impl IntoIterator<Item = i64>,
    ) -> (SyncSender<QueuedDecodedFrame>, VideoBgaDecoder) {
        let (sender, receiver) = sync_channel(4);
        let decoder = VideoBgaDecoder {
            path: PathBuf::new(),
            follow_playback_time: false,
            receiver: Some(receiver),
            clocked_frames: None,
            pending: pending.into_iter().map(frame).collect(),
            current: Some(frame(0)),
            finished: false,
            playback_target_us: Arc::new(AtomicI64::new(0)),
            decode_generation: Arc::new(AtomicU64::new(0)),
            stop_decode: Arc::new(AtomicBool::new(false)),
            restart_decode: Arc::new(AtomicBool::new(false)),
            pass_finished: Arc::new(AtomicBool::new(false)),
            decode_thread: None,
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
        sender.send(queued_frame(0, 10)).unwrap();
        sender.send(queued_frame(0, 20)).unwrap();
        sender.send(queued_frame(0, 30)).unwrap();

        let frame = decoder.poll_frame(25).unwrap();

        assert_eq!(frame.pts_us, 20);
        assert_eq!(decoder.pending.len(), 1);
        assert_eq!(decoder.pending.front().unwrap().pts_us, 30);
    }

    #[test]
    fn poll_frame_prefers_newer_received_due_frame_over_pending_due_frames() {
        let (sender, mut decoder) = decoder_with_channel([10, 20, 30]);
        sender.send(queued_frame(0, 40)).unwrap();
        sender.send(queued_frame(0, 50)).unwrap();

        let frame = decoder.poll_frame(45).unwrap();

        assert_eq!(frame.pts_us, 40);
        assert_eq!(decoder.pending.len(), 1);
        assert_eq!(decoder.pending.front().unwrap().pts_us, 50);
    }

    #[test]
    fn poll_frame_keeps_future_frames_in_bounded_channel() {
        let (sender, mut decoder) = decoder_with_channel([]);
        for pts_us in [10, 20, 30, 40] {
            sender.try_send(queued_frame(0, pts_us)).unwrap();
        }

        let frame = decoder.poll_frame(0).unwrap();

        assert_eq!(frame.pts_us, 0);
        assert_eq!(decoder.pending.len(), 1);
        assert_eq!(decoder.pending.front().unwrap().pts_us, 10);
        sender.try_send(queued_frame(0, 50)).expect("poll should release exactly one channel slot");
        assert!(matches!(
            sender.try_send(queued_frame(0, 60)),
            Err(std::sync::mpsc::TrySendError::Full(_))
        ));

        for _ in 0..100 {
            let frame = decoder.poll_frame(0).unwrap();
            assert_eq!(frame.pts_us, 0);
        }

        assert_eq!(decoder.pending.len(), 1);
        assert!(matches!(
            sender.try_send(queued_frame(0, 60)),
            Err(std::sync::mpsc::TrySendError::Full(_))
        ));
    }

    #[test]
    fn poll_frame_drops_frames_from_before_restart() {
        let (sender, mut decoder) = decoder_with_channel([]);
        decoder.restart();

        sender.send(queued_frame(0, 100)).unwrap();
        sender.send(queued_frame(1, 0)).unwrap();

        let pts_us = decoder.poll_frame(0).unwrap().pts_us;

        assert_eq!(decoder.decode_generation.load(Ordering::Acquire), 1);
        assert_eq!(pts_us, 0);
        assert!(decoder.pending.is_empty());
    }

    #[test]
    fn poll_frame_updates_playback_target_for_clocked_decoder() {
        let target = Arc::new(AtomicI64::new(0));
        let mut decoder = VideoBgaDecoder {
            path: PathBuf::new(),
            follow_playback_time: true,
            receiver: None,
            clocked_frames: Some(Arc::new(Mutex::new(ClockedFrameState::default()))),
            pending: VecDeque::new(),
            current: None,
            finished: false,
            playback_target_us: Arc::clone(&target),
            decode_generation: Arc::new(AtomicU64::new(0)),
            stop_decode: Arc::new(AtomicBool::new(false)),
            restart_decode: Arc::new(AtomicBool::new(false)),
            pass_finished: Arc::new(AtomicBool::new(false)),
            decode_thread: None,
        };

        assert!(decoder.poll_frame(123_456).is_none());

        assert_eq!(target.load(Ordering::Acquire), 123_456);
    }

    #[test]
    fn clocked_poll_accepts_received_frame_without_pts_gate() {
        let frames = Arc::new(Mutex::new(ClockedFrameState::default()));
        let target = Arc::new(AtomicI64::new(0));
        let mut decoder = VideoBgaDecoder {
            path: PathBuf::new(),
            follow_playback_time: true,
            receiver: None,
            clocked_frames: Some(Arc::clone(&frames)),
            pending: VecDeque::new(),
            current: None,
            finished: false,
            playback_target_us: Arc::clone(&target),
            decode_generation: Arc::new(AtomicU64::new(0)),
            stop_decode: Arc::new(AtomicBool::new(false)),
            restart_decode: Arc::new(AtomicBool::new(false)),
            pass_finished: Arc::new(AtomicBool::new(false)),
            decode_thread: None,
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
        assert!(should_skip_frame_conversion(10_000, 20_000));
        assert!(!should_skip_frame_conversion(12_000, 20_000));
        assert!(!should_skip_frame_conversion(25_000, 20_000));
    }

    #[test]
    fn clocked_rewind_detection_ignores_future_frame_waits() {
        assert!(!clocked_playback_target_rewound(10_000, 10_000));
        assert!(!clocked_playback_target_rewound(10_000, 18_000));
        assert!(clocked_playback_target_rewound(10_000, 18_001));
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
