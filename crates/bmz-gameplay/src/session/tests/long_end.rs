use super::*;

fn chart(mode: LongNoteMode) -> PlayableChart {
    let mut chart = ln_chart_with_start_sound_and_end_sound(Some(SoundId(8)));
    chart.long_notes[0].mode = Some(mode);
    chart.lane_notes[Lane::Key1.index()][1].layered_sounds.push(SoundId(9));
    chart
}

fn tail_sounds(audio: &TestAudio) -> Vec<SoundId> {
    audio.scheduled.iter().map(|sound| sound.sound_id).filter(|id| *id != SoundId(7)).collect()
}

#[test]
fn charge_end_plays_all_layers_once_on_release_including_early_bad() {
    for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
        for time in [100_000, 940_000, 1_000_000] {
            let mut session = session_with_autoplay(chart(mode));
            session.autoplay = None;
            let mut audio = TestAudio::default();
            process_session_input(&mut session, human_press(TimeUs(0)));
            schedule_keysounds(&mut session, &mut audio);
            audio.scheduled.clear();
            let events = process_session_input(&mut session, human_release(TimeUs(time)));
            assert_eq!(events.iter().filter(|event| event.affects_score).count(), 1);
            schedule_keysounds(&mut session, &mut audio);
            assert_eq!(tail_sounds(&audio), [SoundId(8), SoundId(9)]);
            audio.scheduled.clear();
            process_session_input(&mut session, human_release(TimeUs(time + 1)));
            process_misses(&mut session, TimeUs(1_500_000));
            schedule_keysounds(&mut session, &mut audio);
            assert!(tail_sounds(&audio).is_empty());
        }
    }
}

#[test]
fn charge_end_miss_is_silent_with_or_without_a_captured_head() {
    for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
        for press in [false, true] {
            let mut session = session_with_autoplay(chart(mode));
            session.autoplay = None;
            if press {
                process_session_input(&mut session, human_press(TimeUs(0)));
            }
            process_misses(&mut session, TimeUs(1_500_000));
            let mut audio = TestAudio::default();
            schedule_keysounds(&mut session, &mut audio);
            assert!(tail_sounds(&audio).is_empty());
        }
    }
}

#[test]
fn charge_end_autoplay_and_auto_keysound_play_each_layer_once() {
    for mode in [LongNoteMode::Cn, LongNoteMode::Hcn] {
        for auto_keysound in [false, true] {
            let mut chart = chart(mode);
            chart.lane_notes[Lane::Key1.index()][1].time = TimeUs(50_000);
            chart.long_notes[0].end_time = TimeUs(50_000);
            let mut session = session_with_autoplay(chart);
            session.audio_mix.auto_keysound = auto_keysound;
            let mut audio = TestAudio::default();
            advance_session_frame(&mut session, &mut audio);
            process_autoplay_inputs(&mut session, TimeUs(50_000));
            schedule_keysounds(&mut session, &mut audio);
            assert_eq!(tail_sounds(&audio), [SoundId(8), SoundId(9)]);
        }
    }
}
