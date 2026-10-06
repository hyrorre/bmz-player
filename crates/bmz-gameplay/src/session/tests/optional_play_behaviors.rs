use super::*;

#[test]
fn miss_keysound_setting_preserves_score_and_uses_key_mix_without_auto_duplicates() {
    for enabled in [false, true] {
        for auto in [false, true] {
            let mut chart = chart_with_keysound();
            chart.lane_notes[Lane::Key1.index()][0].layered_sounds.push(SoundId(8));
            let mut session = session_with_autoplay(chart);
            session.play_keysound_on_miss = enabled;
            session.audio_mix.auto_keysound = auto;
            session.audio_mix.master_volume = 0.8;
            session.audio_mix.key_volume = 0.5;
            session.audio_mix.chart_normalization_gain = 0.5;
            let events = process_misses(&mut session, TimeUs(130_000));
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].judge, Judge::Poor);
            assert_eq!(session.score.past_notes, 1);
            let mut audio = TestAudio::default();
            schedule_keysounds(&mut session, &mut audio);
            assert_eq!(audio.scheduled.len(), if enabled && !auto { 2 } else { 0 });
            for sound in audio.scheduled {
                assert_eq!(
                    sound.start_frame,
                    session.audio_clock.time_to_output_frame(TimeUs(130_000))
                );
                assert!((sound.volume - 0.2).abs() < f32::EPSILON);
                assert_eq!(sound.restart_policy, RestartPolicy::StopSameSound);
            }
            assert!(session.pending_keysounds.is_empty());
        }
    }
}

#[test]
fn result_wait_extends_normal_completion_but_not_result_settlement() {
    let mut session = session_with_autoplay(chart_with_keysound());
    session.judge.process_input(&session.chart, human_press(TimeUs(0)));
    assert!(should_finish(&session, TimeUs(5_000_001)));
    session.result_wait_end_time = Some(TimeUs(8_000_000));
    assert!(result_is_settled(&session, TimeUs(200_001)));
    assert!(!should_finish(&session, TimeUs(5_000_001)));
    assert!(!should_finish(&session, TimeUs(13_000_000)));
    assert!(should_finish(&session, TimeUs(13_000_001)));
    assert_eq!(session.chart.end_time, TimeUs(0));
}

#[test]
fn miss_keysound_does_not_escape_display_only_or_viewer_seek_boundaries() {
    for seek in [false, true] {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.play_keysound_on_miss = true;
        if seek {
            prepare_viewer_seek(&mut session, TimeUs(500_000));
        } else {
            session.display_only_lane_mask[Lane::Key1.index()] = true;
        }
        process_misses(&mut session, TimeUs(500_000));
        let mut audio = TestAudio::default();
        schedule_keysounds(&mut session, &mut audio);
        assert!(audio.scheduled.is_empty());
        assert!(session.pending_keysounds.is_empty());
    }
}

#[test]
fn missed_hcn_head_resets_prior_lane_mute_before_muting_new_voice() {
    for previous in [None, Some(false), Some(true)] {
        let mut session = session_with_autoplay(chart_with_hcn_long_note());
        session.play_keysound_on_miss = true;
        process_misses(&mut session, TimeUs(130_000));
        // A frame jump can retain the mute state of the preceding HCN.
        session.lane_hcn_keysound_muted[Lane::Key1.index()] = previous;
        update_hcn_lane_timers(&mut session, TimeUs(130_000));
        assert_eq!(session.lane_hcn_keysound_muted[Lane::Key1.index()], None);
        assert!(session.pending_keysound_volumes.is_empty());
        let mut audio = TestAudio::default();
        schedule_keysounds(&mut session, &mut audio);
        assert_eq!(audio.scheduled.len(), 1);
        update_hcn_lane_timers(&mut session, TimeUs(131_000));
        assert_eq!(session.lane_hcn_keysound_muted[Lane::Key1.index()], Some(true));
        assert_eq!(session.pending_keysound_volumes, [(SoundId(7), 0.0)]);
    }
}
