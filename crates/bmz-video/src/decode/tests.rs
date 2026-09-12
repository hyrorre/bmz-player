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
fn direct_seek_catch_up_skips_the_landing_keyframe() {
    let mut catch_up = ChannelFrameCatchUp { published_any: true, last_skipped: None };

    for pts_us in [700_000, 900_000] {
        assert!(catch_up.should_skip(pts_us, 1_000_000));
        catch_up.record_skipped(pts_us);
    }
    assert!(!catch_up.should_skip(995_000, 1_000_000));
    assert_eq!(catch_up.take_last_skipped(), Some(900_000));
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
fn video_timestamp_normalizer_preserves_seeked_stream_timeline() {
    let mut normalizer = VideoTimestampNormalizer::with_origin_raw(Some(48_003));

    assert_eq!(normalizer.timestamp_us(Some(48_003), 1, 90_000), 0);
    assert_eq!(normalizer.timestamp_us(Some(138_003), 1, 90_000), 1_000_000);
}

#[test]
fn timestamp_raw_to_us_uses_stream_time_base_and_saturates() {
    assert_eq!(timestamp_raw_to_us(48_003, 1, 90_000), 533_366);
    assert_eq!(timestamp_raw_to_us(10, 1, 0), 0);
    assert_eq!(timestamp_raw_to_us(i64::MAX, i64::MAX, 1), i64::MAX);
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
fn offline_selection_waits_for_a_future_frame_instead_of_reusing_stale_data() {
    let (sender, mut decoder) = decoder_with_channel([]);
    let producer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(25));
        sender.send(queued_frame(0, 10)).unwrap();
        sender.send(queued_frame(0, 20)).unwrap();
        sender.send(queued_frame(0, 30)).unwrap();
    });
    assert_eq!(decoder.frame_at_blocking(25).unwrap().unwrap().pts_us, 20);
    producer.join().unwrap();
    assert!(
        decoder.frame_at_blocking(35).is_err(),
        "disconnect is a failure, not a successful EOF"
    );
}

#[test]
fn offline_selection_holds_last_frame_at_verified_eof() {
    let (_sender, mut decoder) = decoder_with_channel([10, 20]);
    decoder.pass_finished.store(true, Ordering::Release);
    assert_eq!(decoder.frame_at_blocking(100).unwrap().unwrap().pts_us, 20);
    assert_eq!(decoder.frame_at_blocking(200).unwrap().unwrap().pts_us, 20);
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
fn restart_at_sets_seek_target_and_invalidates_channel_generation() {
    let (_sender, mut decoder) = decoder_with_channel([10, 20]);

    decoder.restart_at(1_234_567);

    assert_eq!(decoder.playback_target_us.load(Ordering::Acquire), 1_234_567);
    assert_eq!(decoder.decode_generation.load(Ordering::Acquire), 1);
    assert!(decoder.restart_decode.load(Ordering::Acquire));
    assert!(decoder.current.is_none());
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
