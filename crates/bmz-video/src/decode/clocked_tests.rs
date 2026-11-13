use super::*;

#[test]
fn clocked_loop_keeps_full_period_across_repeated_sixty_fps_passes() {
    let mut base = 0;
    for _ in 0..100 {
        let mut timing = ClockedLoopTiming::default();
        for frame in 0..3600 {
            timing.record_frame(base + frame * 1_000_000 / 60, 1_000_000 / 60);
        }
        base = timing.end_us(base, Some(60_000_000), 1_000_000 / 60);
    }
    assert_eq!(base, 6_000_000_000);
}

#[test]
fn clocked_loop_uses_variable_final_frame_duration_without_stream_duration() {
    let mut timing = ClockedLoopTiming::default();
    timing.record_frame(0, 50_000);
    timing.record_frame(50_000, 100_000);
    timing.record_frame(150_000, 200_000);
    assert_eq!(timing.end_us(0, None, 33_333), 350_000);
}

#[test]
fn clocked_loop_falls_back_to_observed_interval_or_nominal_frame_rate() {
    let mut timing = ClockedLoopTiming::default();
    timing.record_frame(0, 0);
    assert_eq!(timing.end_us(0, None, 40_000), 40_000);
    assert_eq!(timing.end_us(0, Some(1_000_000), 40_000), 1_000_000);
    timing.record_frame(50_000, 0);
    timing.record_frame(100_000, 0);
    assert_eq!(timing.end_us(0, None, 40_000), 150_000);
}

#[test]
fn clocked_decoder_holds_final_picture_until_the_next_full_loop() {
    let fixture = ThreeFrameMovie::new();
    let path = &fixture.0;
    let duration = video_duration_us(path).unwrap();
    let mut reference = VideoBgaDecoder::open(path).unwrap();
    let first = reference.frame_at_blocking(0).unwrap().unwrap().clone();
    let last = reference.frame_at_blocking(duration).unwrap().unwrap().clone();
    assert_ne!(first.rgba, last.rgba, "the fixture must detect an early restart");
    assert!(duration - last.pts_us > CLOCKED_FRAME_PUBLISH_LEAD_US);
    let mut decoder = VideoBgaDecoder::open_following_playback_time(path).unwrap();
    for pass in 0..3 {
        let base = pass * duration;
        let target = base + last.pts_us;
        wait_for_frame(&mut decoder, target, target);
        std::thread::sleep(Duration::from_millis(30));
        let frame = decoder.poll_frame(target).unwrap();
        assert_eq!(frame.pts_us, target, "last frame must keep its display interval");
        assert_eq!(frame.rgba, last.rgba);
        let next = base + duration;
        wait_for_frame(&mut decoder, next, next);
        assert_eq!(decoder.poll_frame(next).unwrap().rgba, first.rgba);
    }
    decoder.restart();
    wait_for_frame(&mut decoder, 0, 0);
    assert_eq!(decoder.poll_frame(0).unwrap().rgba, first.rgba);
}

struct ThreeFrameMovie(PathBuf);

#[test]
fn channel_decoder_wakes_from_eof_for_repeated_restart() {
    let fixture = ThreeFrameMovie::new();
    let mut decoder = VideoBgaDecoder::open(&fixture.0).unwrap();
    let first = decoder.frame_at_blocking(0).unwrap().unwrap().rgba.clone();
    for _ in 0..3 {
        let last = decoder.frame_at_blocking(1_000_000).unwrap().unwrap().rgba.clone();
        assert_ne!(last, first);
        assert!(decoder.is_finished());
        decoder.restart();
        assert_eq!(decoder.frame_at_blocking(0).unwrap().unwrap().rgba, first);
    }
}

#[test]
fn playback_decoder_applies_canvas_size_at_open_and_after_restart() {
    let fixture = ThreeFrameMovie::new();
    let mut decoder = VideoBgaDecoder::open_for_playback(&fixture.0, [1, 1]).unwrap();
    let first = decoder.frame_at_blocking(0).unwrap().unwrap();
    assert_eq!(first.source_size, [2, 2]);
    assert_eq!([first.width, first.height], [1, 1]);
    let last = decoder.frame_at_blocking(1_000_000).unwrap().unwrap();
    assert_eq!([last.width, last.height], [1, 1]);
    assert_eq!(last.pts_us, 200_000);

    decoder.set_playback_canvas_size([4, 4]);
    decoder.restart();
    let first = decoder.frame_at_blocking(0).unwrap().unwrap();
    assert_eq!(first.source_size, [2, 2]);
    assert_eq!([first.width, first.height], [2, 2]);
    assert_eq!(first.pts_us, 0);
}

#[test]
fn playback_resize_uses_new_size_for_unqueued_frames_without_seeking() {
    let fixture = ThreeFrameMovie::with_luma_levels(0..12);
    let mut decoder = VideoBgaDecoder::open_for_playback(&fixture.0, [1, 1]).unwrap();
    assert_eq!(decoder.frame_at_blocking(0).unwrap().unwrap().width, 1);
    decoder.set_playback_canvas_size([2, 2]);
    let last = decoder.frame_at_blocking(1_100_000).unwrap().unwrap();
    assert_eq!(last.pts_us, 1_100_000);
    assert_eq!([last.width, last.height], [2, 2]);
}

impl ThreeFrameMovie {
    fn new() -> Self {
        Self::with_luma_levels([32, 128, 224])
    }

    fn with_luma_levels(levels: impl IntoIterator<Item = u8>) -> Self {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-video-loop-{}-{stamp}.y4m", std::process::id()));
        let mut bytes = b"YUV4MPEG2 W2 H2 F10:1 Ip A1:1 C420jpeg\n".to_vec();
        for luma in levels {
            bytes.extend_from_slice(b"FRAME\n");
            bytes.extend_from_slice(&[luma, luma, luma, luma, 128, 128]);
        }
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for ThreeFrameMovie {
    fn drop(&mut self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::remove_file(&self.0) {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    assert!(std::time::Instant::now() < deadline, "{}: {error}", self.0.display());
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}

fn wait_for_frame(decoder: &mut VideoBgaDecoder, target: i64, expected: i64) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(frame) = decoder.poll_frame(target) {
            assert!(
                frame.pts_us <= expected,
                "frame advanced early: {} > {expected}",
                frame.pts_us
            );
            if frame.pts_us == expected {
                return;
            }
        }
        assert!(std::time::Instant::now() < deadline, "decoder did not reach {expected}");
        std::thread::sleep(Duration::from_millis(2));
    }
}
