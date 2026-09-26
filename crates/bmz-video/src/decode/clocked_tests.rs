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

impl ThreeFrameMovie {
    fn new() -> Self {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-video-loop-{}-{stamp}.y4m", std::process::id()));
        let mut bytes = b"YUV4MPEG2 W2 H2 F10:1 Ip A1:1 C420jpeg\n".to_vec();
        for luma in [32, 128, 224] {
            bytes.extend_from_slice(b"FRAME\n");
            bytes.extend_from_slice(&[luma, luma, luma, luma, 128, 128]);
        }
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for ThreeFrameMovie {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
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
