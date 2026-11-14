use super::*;

#[test]
#[ignore = "requires the local mrym2014 song files"]
fn mrym2014_fixed_branches() {
    use bmz_chart::import::{BmsRandomSource, import_bms_chart_with_random_source};
    use bmz_chart::model::TimingEventKind;
    use bmz_gameplay::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use bmz_gameplay::input::binding::{BindingEntry, LaneBinding};
    use bmz_gameplay::input::translator::{DefaultInputTranslator, InputTimestampAnchor};
    use bmz_gameplay::session::{process_human_inputs, process_misses, process_replay_inputs};

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/songs/OTHERS/mrym2014/foon_tournamix_2014.bms");
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut reflected_branches = Vec::new();
    for branch in 1..=133 {
        let result = import_bms_chart_with_random_source(
            &path,
            BmsRandomSource::Choices { random: vec![branch], switches: Vec::new() },
            false,
        )
        .unwrap();
        assert_eq!(result.bms_random_choices, [branch]);
        let mut chart = result.chart;
        let reflection_events: Vec<_> = chart
            .timing_events
            .iter()
            .filter(|event| event.kind.is_bpm_reflection())
            .copied()
            .collect();
        if !reflection_events.is_empty() {
            reflected_branches.push(branch);
        }
        let raw_map =
            TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
        assert!(
            raw_map.segments.iter().all(|segment| segment.bpm.is_finite() && segment.bpm > 0.0)
        );
        match branch {
            8 => assert!(reflection_events.is_empty()),
            26 => {
                assert_eq!(reflection_events[0].time, TimeUs(0));
                assert!(raw_map.bpm_reflections().is_empty());
                assert_eq!(raw_map.bpm_at_time(TimeUs(0)), 83.0);
                assert_eq!(chart.total_notes, 50);
                assert!((chart.end_time.0 - 693_976).abs() <= 1);
                bmz_chart::start_margin::apply_start_note_margin(&mut chart);
                let shifted = TimingMap::from_chart_timing_events(
                    chart.metadata.initial_bpm,
                    &chart.timing_events,
                );
                assert_eq!(shifted.bpm_reflections()[0].time, TimeUs(1_000_000));
            }
            72 => {
                assert_eq!(chart.long_notes.len(), 1);
                let pair = &chart.long_notes[0];
                assert_eq!(pair.start_time, reflection_events[0].time);
                assert!((pair.end_time.0 - 266_893_382).abs() <= 3);
            }
            81 => {
                let anchor = reflection_events[0].time;
                assert_eq!(raw_map.bpm_at_time(anchor), 45.0);
                let note = chart
                    .lane_notes
                    .iter()
                    .flatten()
                    .rev()
                    .filter(|note| note.kind == NoteKind::Tap && note.time < anchor)
                    .max_by_key(|note| note.time)
                    .unwrap()
                    .clone();
                let distance = anchor.0 - note.time.0;
                let later = TimeUs(anchor.0 + distance + 1_000);
                let mut session =
                    build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
                let snapshot = build_render_snapshot(&session, later, &[], None);
                // The input clock reflects in time while the playfield reflects
                // integrated distance. Check each against its own equation.
                let late_tick = session.timing_map.time_to_tick_f64(later);
                let folded_tick = 2.0 * reflection_events[0].tick.0 as f64 - late_tick;
                let earlier = session.timing_map.tick_to_time(ChartTick(folded_tick as u64));
                let before = build_render_snapshot(&session, earlier, &[], None);
                assert!(snapshot.visible_notes.iter().map(Vec::len).sum::<usize>() > 0);
                let mut compared = 0;
                for lane in Lane::ALL {
                    for note in &before.visible_notes[lane.index()] {
                        if let Some(projected) = snapshot.visible_notes[lane.index()]
                            .iter()
                            .find(|candidate| candidate.time == note.time)
                        {
                            assert!((projected.y - note.y).abs() < 0.002);
                            compared += 1;
                        }
                    }
                }
                assert!(compared > 0);
                let control = PhysicalControl::KeyboardKey("Z".to_string());
                let mut backend = BufferedInputBackend::default();
                backend.push(DeviceInputEvent {
                    device: DeviceId(1),
                    control: control.clone(),
                    kind: InputKind::Press,
                    timestamp: DeviceTimestamp::MonotonicNs(1_000_000_000),
                    bounce_policy: Default::default(),
                });
                session.input_system.backend = Box::new(backend);
                session.input_system.translator = Box::new(DefaultInputTranslator {
                    binding: LaneBinding {
                        entries: vec![BindingEntry {
                            device: None,
                            control,
                            lane: note.lane,
                            scratch_direction: None,
                        }],
                    },
                });
                session.input_timestamp_anchor = Some(InputTimestampAnchor {
                    monotonic_ns: 1_000_000_000,
                    audio_time: TimeUs(later.0 - 3_000),
                });
                session.offsets.input_offset_us = 3_000;
                session.audio_clock.pause_at(later);
                let events = process_human_inputs(&mut session);
                let judged = events.iter().find(|event| event.note_id == Some(note.id)).unwrap();
                assert_eq!(judged.delta, TimeUs(-1_000));
                assert_eq!(judged.side, TimingSide::Fast);
                assert_eq!(judged.time, later);
                let saved = session.replay_recorder.events.last().copied().unwrap();
                assert_eq!(
                    saved,
                    bmz_core::replay::ReplayEvent {
                        time: later,
                        lane: note.lane,
                        kind: InputKind::Press,
                        device_kind: InputDeviceKind::Keyboard,
                        scratch_direction: None,
                    }
                );
                let mut replay = build_game_session(
                    session.chart.clone(),
                    &profile,
                    PlaySessionOptions::default(),
                );
                replay.offsets.input_offset_us = 35_000;
                replay.replay_player = Some(bmz_gameplay::replay::ReplayPlayer::new(vec![saved]));
                let replayed = process_replay_inputs(&mut replay, later);
                assert_eq!(replayed[0].note_id, judged.note_id);
                assert_eq!(replayed[0].delta, judged.delta);
                assert_eq!(replayed[0].time, judged.time);

                assert!(process_misses(&mut session, TimeUs(later.0 + 500_000)).iter().all(
                    |event| {
                        event
                            .note_id
                            .is_some_and(|id| session.chart.note_by_id(id).unwrap().time < anchor)
                    }
                ));
            }
            128 => {
                assert_eq!(raw_map.bpm_at_time(reflection_events[0].time), 830.0);
                assert!(
                    chart
                        .timing_events
                        .iter()
                        .any(|event| matches!(event.kind, TimingEventKind::Stop { .. }))
                );
            }
            133 => {
                let anchor = reflection_events[0].time;
                assert_eq!(raw_map.bpm_at_time(anchor), 256.0);
                let after = TimeUs(anchor.0 + 100_000);
                assert_eq!(raw_map.bpm_at_time(after), 69.0);
                assert_eq!(raw_map.reflected_judge_time(after), TimeUs(anchor.0 - 100_000));
            }
            _ => {}
        }
    }
    assert_eq!(reflected_branches.len(), 36, "{reflected_branches:?}");
}
