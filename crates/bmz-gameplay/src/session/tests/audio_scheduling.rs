use super::*;
use std::time::{Duration, Instant};

#[test]
fn both_modes_use_system_time_for_bgm_and_auto_keysounds_at_every_speed() {
    for ahead in [false, true] {
        for rate in [25, 100, 300] {
            for full_auto in [false, true] {
                let mut chart = chart_with_keysound();
                chart.bgm_events = chart_with_bgm().bgm_events;
                let mut session = session_with_autoplay(chart);
                if !full_auto {
                    session.autoplay = None;
                }
                session.audio_mix.auto_keysound = !full_auto;
                let base = Instant::now();
                session.audio_clock.set_playback_rate_percent(rate);
                session.audio_clock.schedule_ahead = ahead;
                // ON needs chart zero inside the 100 ms window. OFF must stay before
                // it however long this thread is descheduled.
                let lead_ms = if ahead { 50 } else { 60_000 };
                let start = TimeUs(-10 * lead_ms * i64::from(rate));
                session.audio_clock.start_at(start, base);
                // Deliberately wrong device progress must not move the chart.
                session
                    .audio_clock
                    .current_frame
                    .store(48_000_000, std::sync::atomic::Ordering::Relaxed);
                assert_eq!(session.audio_clock.now_at(base), start);
                let mut audio = TestAudio::default();
                advance_session_frame(&mut session, &mut audio);
                if ahead {
                    assert_eq!(audio.scheduled.len(), 2);
                    for sound in &audio.scheduled {
                        assert_eq!(sound.start_time, Some(base + Duration::from_millis(50)));
                        assert_eq!(sound.catch_up, sound.sound_id == SoundId(3));
                    }
                } else {
                    assert!(audio.scheduled.is_empty());
                    // No sleeping: move the system anchor past chart zero.
                    session.audio_clock.start_at(TimeUs(0), base - Duration::from_millis(1));
                    advance_session_frame(&mut session, &mut audio);
                    assert_eq!(audio.scheduled.len(), 2);
                    assert!(
                        audio
                            .scheduled
                            .iter()
                            .all(|s| s.start_time.is_none() && s.start_frame == 0 && !s.catch_up)
                    );
                }
                let count = audio.scheduled.len();
                advance_session_frame(&mut session, &mut audio);
                assert_eq!(audio.scheduled.len(), count, "events must not be requested twice");
            }
        }
    }
}

#[test]
fn off_mode_wakes_for_an_auto_keysound_the_judge_has_moved_past() {
    let mut chart = chart_with_keysound();
    chart.lane_notes[Lane::Key1.index()][0].time = TimeUs(60_000_000);
    let mut session = session_with_autoplay(chart);
    session.autoplay = None;
    session.audio_mix.auto_keysound = true;
    session.audio_clock.schedule_ahead = false;
    session.audio_clock.start_at(TimeUs(0), Instant::now());
    // An early hit moves judgement on while the auto keysound is still due.
    session.judge.lanes[Lane::Key1.index()].next_note_index = 1;
    let runtime = crate::runtime::GameplayRuntime::new(session);
    assert!(runtime.next_wake_after(Duration::from_secs(3_600)) <= Duration::from_secs(60));
}

#[test]
fn pressed_keysounds_never_wait_for_old_output_frame_or_input_offset() {
    for ahead in [false, true] {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.autoplay = None;
        session.audio_clock.schedule_ahead = ahead;
        session.audio_clock.start_output_frame = 48_000_000;
        let mut audio = TestAudio::default();
        process_session_input(&mut session, human_press(TimeUs(10_000)));
        schedule_keysounds(&mut session, &mut audio);
        assert_eq!(audio.scheduled.len(), 1);
        let sound = audio.scheduled[0];
        assert!(sound.start_time.is_none());
        assert_eq!(sound.start_frame, 0);
        assert!(!sound.catch_up);
        assert_eq!(sound.sample_offset_frames, 0);
    }
}

#[test]
fn live_input_anchor_pairs_one_observation_even_if_frame_time_is_stale() {
    let mut session = session_with_autoplay(chart_with_keysound());
    let (base, base_ns) = crate::input::backend::monotonic_observation();
    session.audio_clock.start_at(TimeUs(123_000), base);
    sync_input_timestamp_anchor(&mut session);
    let anchor = session.input_timestamp_anchor.unwrap();
    assert_eq!(anchor.audio_time.0, 123_000 + ((anchor.monotonic_ns - base_ns) / 1000) as i64);
}
