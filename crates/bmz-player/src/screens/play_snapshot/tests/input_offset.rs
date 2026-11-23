use super::*;
use bmz_gameplay::input::backend::{
    BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
};
use bmz_gameplay::input::binding::{BindingEntry, LaneBinding};
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::{DefaultInputTranslator, InputTimestampAnchor};
use bmz_gameplay::session::{process_human_inputs, update_recent_judgements};
use bmz_render::plan::{DrawCommand, DrawPlan, TextureId};
use bmz_render::scene::AppSceneSnapshot;
use bmz_render::skin::{DynamicTimerRuntime, SkinContext, SkinManifest};

/// Press at `time` and deliver it after `delivery_delay`. Returns when the press is
/// judged: a positive offset holds it until its judgement time.
fn press_at(session: &mut GameSession, time: TimeUs, delivery_delay: i64) -> TimeUs {
    let control = PhysicalControl::KeyboardKey("Z".to_string());
    let mut backend = BufferedInputBackend::default();
    for (kind, timestamp) in [(InputKind::Release, 999_000_000), (InputKind::Press, 1_000_000_000)]
    {
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: control.clone(),
            kind,
            timestamp: DeviceTimestamp::MonotonicNs(timestamp),
            bounce_policy: Default::default(),
        });
    }
    session.input_system = InputSystem {
        backend: Box::new(backend),
        translator: Box::new(DefaultInputTranslator {
            binding: LaneBinding {
                entries: vec![BindingEntry {
                    device: None,
                    control,
                    lane: Lane::Key1,
                    scratch_direction: None,
                }],
            },
        }),
        bounce_filter: Default::default(),
    };
    session.input_timestamp_anchor =
        Some(InputTimestampAnchor { monotonic_ns: 1_000_000_000, audio_time: time });
    let delivered_at = TimeUs(time.0 + delivery_delay);
    let judged_at = TimeUs(time.0 + session.offsets.input_offset_us);
    let processed_at = delivered_at.max(judged_at);
    session.audio_clock.pause_at(delivered_at);
    let mut judgements = process_human_inputs(session);
    if processed_at > delivered_at {
        assert!(judgements.is_empty());
        session.audio_clock.pause_at(processed_at);
        judgements = process_human_inputs(session);
    }
    assert_eq!(judgements.len(), 1);
    assert_eq!(judgements[0].judge, Judge::PGreat);
    assert_eq!(judgements[0].delta, TimeUs(10_000));
    assert_eq!(judgements[0].time, judged_at);
    assert_eq!(session.replay_recorder.events.last().unwrap().time, judgements[0].time);
    update_recent_judgements(session, &judgements, processed_at);
    processed_at
}

fn judgement_skin() -> SkinContext {
    let document = serde_json::from_str(
        r#"{
            "type":0,"w":100,"h":100,
            "image":[
                {"id":"pg","src":1,"x":0,"y":0,"w":10,"h":10},
                {"id":"bomb","src":3,"x":0,"y":0,"w":10,"h":10}
            ],
            "value":[{"id":"combo","src":2,"x":0,"y":0,"w":100,"h":10,"divx":10,"digit":3}],
            "judge":[{"id":"judge","index":0,
                "images":[{"id":"pg","dst":[
                    {"time":0,"x":0,"y":20,"w":20,"h":10},
                    {"time":100,"x":100},{"time":1000}
                ]}],
                "numbers":[{"id":"combo","dst":[
                    {"time":0,"x":40,"y":20,"w":5,"h":10},{"time":1000}
                ]}]
            }],
            "destination":[{"id":"judge"},{"id":"bomb","timer":51,"loop":-1,
                "dst":[{"time":0,"x":10,"y":0,"w":10,"h":10},{"time":800}]
            }]
        }"#,
    )
    .unwrap();
    SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        document,
        (1..=3).map(|id| SkinDocumentTexture {
            source_id: id.to_string(),
            texture: SkinTextureId(200 + id),
            source_size: SkinImageSize { width: 100.0, height: 10.0 },
        }),
    )
}

#[test]
fn input_offset_keeps_judge_combo_and_bomb_visible_and_animating_on_every_press() {
    let skin = judgement_skin();
    for offset in [-20_000, 0, 20_000] {
        for delivery_delay in [0, 50_000] {
            let mut profile = ProfileConfig::new_default("default", "Default", 1);
            profile.judge.input_offset_us = offset;
            let mut chart = chart();
            chart.lane_notes[Lane::Key1.index()].push(tap_note(2, Lane::Key1, 192, 1_200_000));
            chart.total_notes = 2;
            chart.end_time = TimeUs(1_200_000);
            let mut session =
                build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());
            let mut timers = DynamicTimerRuntime::default();
            for (index, note_time) in [1_000_000, 1_200_000].into_iter().enumerate() {
                let physical_time = TimeUs(note_time + 10_000 - offset);
                let processed_at = press_at(&mut session, physical_time, delivery_delay);
                // The display starts at the later of the press and its judgement time.
                let shown_from = TimeUs(physical_time.0 + offset.max(0));
                let mut previous_x = None;
                for elapsed in [0, 5_000, 19_000, 20_000, 30_000] {
                    let now = TimeUs(processed_at.0 + elapsed);
                    let snapshot =
                        build_render_snapshot(&session, now, &session.recent_judgements, None);
                    let judgement = snapshot.recent_judgements.last().unwrap();
                    assert_eq!(judgement.time, shown_from);
                    assert_eq!(judgement.delta_us, 10_000);
                    assert_eq!(judgement.combo, index as u32 + 1);
                    let plan = DrawPlan::from_scene_with_skin(
                        &AppSceneSnapshot::Play(snapshot),
                        &skin,
                        &mut timers,
                    );
                    for texture in [201, 202, 203] {
                        assert!(plan.commands.iter().any(|command| matches!(command,
                            DrawCommand::Image { texture: actual, .. } if *actual == TextureId(texture)
                        )), "offset={offset}, delivery_delay={delivery_delay}, elapsed={elapsed}, texture={texture}");
                    }
                    let x = plan
                        .commands
                        .iter()
                        .find_map(|command| match command {
                            DrawCommand::Image { texture: TextureId(201), rect, .. } => {
                                Some(rect.x)
                            }
                            _ => None,
                        })
                        .unwrap();
                    if let Some(previous_x) = previous_x {
                        assert!(
                            x > previous_x,
                            "animation froze: offset={offset}, elapsed={elapsed}, x={x}, previous={previous_x}"
                        );
                    }
                    previous_x = Some(x);
                }
            }
            assert_eq!(session.score.ex_score(), 4);
        }
    }
}

#[test]
fn input_offset_display_history_expires_800ms_after_the_display_starts() {
    for offset in [-500_000, 500_000] {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.judge.input_offset_us = offset;
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let physical_time = TimeUs(1_010_000 - offset);
        press_at(&mut session, physical_time, 0);
        // A negative offset shows from the press, a positive one from its judgement time.
        let shown_from = TimeUs(physical_time.0 + offset.max(0));
        for age in [799_000, 801_000] {
            let now = TimeUs(shown_from.0 + age);
            update_recent_judgements(&mut session, &[], now);
            let snapshot = build_render_snapshot(&session, now, &session.recent_judgements, None);
            if age < 800_000 {
                assert_eq!(snapshot.recent_judgements.len(), 1);
                assert_eq!(snapshot.recent_judgements[0].time, shown_from);
            } else {
                assert!(snapshot.recent_judgements.is_empty());
            }
        }
    }
}
