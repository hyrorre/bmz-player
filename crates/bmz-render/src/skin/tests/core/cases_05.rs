use super::*;

#[test]
fn lr2_single_timestamp_starts_then_holds_including_judge_destinations() {
    for time in [0, 30, 1000] {
        for loop_time in [-1, 0, time, time + 1] {
            // Multiple frames at one timestamp must select the last one too.
            for frames in [
                serde_json::json!([{"time": time, "x": 20}]),
                serde_json::json!([{"time": time, "x": 10}, {"time": time, "x": 20}]),
            ] {
                let destination: SkinDestinationDef = serde_json::from_value(serde_json::json!({
                    "lr2Timing": true, "loop": loop_time, "dst": frames
                }))
                .unwrap();
                for elapsed in [time - 1, time, time + 1, time + 10000] {
                    let expected =
                        (elapsed >= time && (loop_time >= 0 || elapsed == time)).then_some(20);
                    for actual in [
                        resolve_destination_frame(
                            &destination,
                            elapsed,
                            &[],
                            &SkinDrawState::default(),
                        ),
                        resolve_destination_frame_until_end(
                            &destination,
                            elapsed,
                            &[],
                            &SkinDrawState::default(),
                        ),
                    ] {
                        assert_eq!(
                            actual.map(|frame| frame.x),
                            expected,
                            "time={time}, loop={loop_time}, elapsed={elapsed}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn lr2_loop_boundaries_and_activation_match_openlr2() {
    for (loop_time, elapsed, expected) in [
        (0, 99, None),
        (0, 100, Some(100)),
        (0, 999, Some(999)),
        (0, 1000, Some(1000)),
        (0, 1001, Some(100)),
        (0, 1100, Some(100)),
        (500, 1000, Some(1000)),
        (500, 1001, Some(501)),
        (500, 1500, Some(500)),
        (1000, 500, Some(500)),
        (1000, 1001, Some(1000)),
        (1001, 99, None),
        (1001, 100, Some(100)),
        (1001, 1000, Some(100)),
        (-1, 99, None),
        (-1, 1000, Some(1000)),
        (-1, 1001, None),
    ] {
        let destination: SkinDestinationDef = serde_json::from_value(serde_json::json!({
            "lr2Timing": true, "loop": loop_time, "dst": [
                {"time": 100, "x": 100}, {"time": 1000, "x": 1000}
            ]
        }))
        .unwrap();
        let frame =
            resolve_destination_frame(&destination, elapsed, &[], &SkinDrawState::default());
        assert_eq!(frame.map(|frame| frame.x), expected, "loop={loop_time}, elapsed={elapsed}");
    }
}

#[test]
fn lr2_loop_uses_active_conditional_frames_and_rejects_reversed_range() {
    let destination: SkinDestinationDef = serde_json::from_value(serde_json::json!({
        "lr2Timing": true, "dst": [
            {"if": [1], "value": {"time": 30, "x": 10}},
            {"if": [2], "value": {"time": 1000, "x": 20}}
        ]
    }))
    .unwrap();
    for (ops, elapsed, expected) in [
        (&[][..], 1000, None),
        (&[1], 30, Some(10)),
        (&[2], 999, None),
        (&[2], 1000, Some(20)),
        (&[1, 2], 1000, Some(20)),
    ] {
        assert_eq!(
            resolve_destination_frame(&destination, elapsed, ops, &SkinDrawState::default())
                .map(|frame| frame.x),
            expected
        );
    }
    let reversed: SkinDestinationDef = serde_json::from_value(serde_json::json!({
        "lr2Timing": true, "dst": [{"time": 1000}, {"time": 30}]
    }))
    .unwrap();
    assert!(resolve_destination_frame(&reversed, 1000, &[], &SkinDrawState::default()).is_none());
}

#[test]
fn negative_loop_destination_disappears_after_end() {
    // loop:-1 の destination はアニメーション終端を過ぎると描画されない（READY/ボム）。
    let destination: SkinDestinationDef = serde_json::from_str(
        r#"{ "id": "ready", "loop": -1, "dst": [
                { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10, "a": 0 },
                { "time": 1000, "a": 255 }
            ]}"#,
    )
    .unwrap();
    assert!(resolve_destination_frame(&destination, 500, &[], &SkinDrawState::default()).is_some());
    assert!(
        resolve_destination_frame(&destination, 1000, &[], &SkinDrawState::default()).is_some()
    );
    assert!(
        resolve_destination_frame(&destination, 1001, &[], &SkinDrawState::default()).is_none()
    );
}

#[test]
fn single_frame_destination_preserves_start_and_loop_semantics() {
    let destination: SkinDestinationDef = serde_json::from_str(
        r#"{ "id": "flash", "dst": [{ "time": 1000, "x": 2, "y": 3, "w": 10, "h": 20 }] }"#,
    )
    .unwrap();

    assert!(resolve_destination_frame(&destination, 999, &[], &SkinDrawState::default()).is_none());
    assert!(
        resolve_destination_frame(&destination, 1000, &[], &SkinDrawState::default()).is_none()
    );

    let held: SkinDestinationDef = serde_json::from_str(
        r#"{ "id": "flash", "loop": 1000, "dst": [{ "time": 1000, "x": 2, "y": 3, "w": 10, "h": 20 }] }"#,
    )
    .unwrap();
    let frame = resolve_destination_frame(&held, 1000, &[], &SkinDrawState::default())
        .expect("single frame starts at its keyframe time");
    assert_eq!((frame.x, frame.y, frame.w, frame.h), (2, 3, 10, 20));
    assert!(resolve_destination_frame(&held, 1001, &[], &SkinDrawState::default()).is_some());

    let disappearing: SkinDestinationDef = serde_json::from_str(
            r#"{ "id": "flash", "loop": -1, "dst": [{ "time": 1000, "x": 2, "y": 3, "w": 10, "h": 20 }] }"#,
        )
        .unwrap();
    assert!(
        resolve_destination_frame(&disappearing, 1001, &[], &SkinDrawState::default()).is_none()
    );
}

#[test]
fn omitted_loop_restarts_destination_animation_from_zero() {
    let destination: SkinDestinationDef = serde_json::from_str(
        r#"{ "id": "rhythm", "timer": 140, "dst": [
                { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "time": 1000, "x": 100 }
            ]}"#,
    )
    .unwrap();

    let before = resolve_destination_frame(&destination, 999, &[], &SkinDrawState::default())
        .expect("animation should reach its final keyframe");
    let restarted = resolve_destination_frame(&destination, 1000, &[], &SkinDrawState::default())
        .expect("animation should restart at its first keyframe");

    assert_eq!(before.x, 100);
    assert_eq!(restarted.x, 0);
}

#[test]
fn destination_frame_h_expr_resolves_fast_slow_breakdown_height() {
    let destination: SkinDestinationDef = serde_json::from_str(&format!(
        r#"{{
                "id": "graph_r",
                "loop": 1000,
                "dst": [
                    {{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 0 }},
                    {{ "time": 1000, "h_expr": "{}(422)" }}
                ]
            }}"#,
        SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT
    ))
    .unwrap();
    let state = SkinDrawState {
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            slow_empty_poor: 5,
            slow_poor: 10,
            ..crate::snapshot::FastSlowJudgeCounts::default()
        }),
        ..SkinDrawState::default()
    };

    let frame = resolve_destination_frame(&destination, 1000, &[], &state).unwrap();

    assert_eq!(frame.h, 50);
}

#[test]
fn text_destination_rect_for_ref_returns_normalized_first_frame() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w": 1280,
        "h": 720,
        "text": [
            { "id": "searchword", "ref": 30, "font": "f" },
            { "id": "title", "ref": 10, "font": "f" }
        ],
        "destination": [
            {
                "id": "title",
                "dst": [{ "x": 0, "y": 0, "w": 100, "h": 30 }]
            },
            {
                "id": "searchword",
                "dst": [{ "x": 640, "y": 360, "w": 320, "h": 36 }]
            }
        ]
    }))
    .unwrap();

    let rect = document.text_destination_rect_for_ref(30).unwrap();
    assert!(approx_eq(rect.0, 0.5));
    // skin y=360, h=36 → flipped: (720 - 396) / 720 = 0.45
    assert!(approx_eq(rect.1, 0.45));
    assert!(approx_eq(rect.2, 0.25));
    assert!(approx_eq(rect.3, 0.05));

    assert!(document.text_destination_rect_for_ref(999).is_none());
}

#[test]
fn destination_offsets_deduplicate_ids_and_accept_only_beatoraja_range() {
    let destination: SkinDestinationDef = serde_json::from_str(
        r#"{
            "id": "image",
            "offset": 42,
            "offsets": [42, 42, 0, -1, 200, 34],
            "dst": [{ "x": 100, "y": 200, "w": 20, "h": 40 }]
        }"#,
    )
    .unwrap();
    let mut offsets = SkinOffsetValues::default();
    offsets.set(42, SkinOffsetValue { x: 10, y: 20, w: 4, h: 6, r: 7, a: -25 });
    offsets.set(34, SkinOffsetValue { x: 3, y: -2, ..Default::default() });
    offsets.set(
        SKIN_OFFSET_BAR_LINE,
        SkinOffsetValue { x: 1000, y: 1000, w: 1000, h: 1000, r: 1000, a: 1000 },
    );
    let state = SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() };
    let mut frame =
        ResolvedSkinFrame { x: 100, y: 200, w: 20, h: 40, a: 200, angle: 1, ..Default::default() };

    apply_skin_offset_to_frame(&destination, &mut frame, &state, false);

    assert_eq!((frame.x, frame.y, frame.w, frame.h), (111, 215, 24, 46));
    assert_eq!(frame.angle, 8);
    assert_eq!(frame.a, 175);
}

#[test]
fn runtime_offsets_preserve_configured_components_of_special_ids() {
    let mut offsets = SkinOffsetValues::default();
    offsets.set(3, SkinOffsetValue { x: 3, y: 999, w: 2, h: 4, r: 1, a: 5 });
    offsets.set(4, SkinOffsetValue { x: 4, y: 999, w: 6, h: 8, r: 2, a: 6 });
    offsets.set(5, SkinOffsetValue { x: 5, y: 999, w: 10, h: 12, r: 3, a: 123 });
    let disabled = SkinDrawState {
        skin_offsets: offsets,
        offset_lift_px: 10,
        offset_lanecover_px: -20,
        offset_hidden_cover_px: 30,
        hidden_enabled: false,
        ..SkinDrawState::default()
    };

    assert_eq!(
        effective_skin_offset(3, &disabled),
        Some(SkinOffsetValue { x: 3, y: 10, w: 2, h: 4, r: 1, a: 5 })
    );
    assert_eq!(
        effective_skin_offset(4, &disabled),
        Some(SkinOffsetValue { x: 4, y: -20, w: 6, h: 8, r: 2, a: 6 })
    );
    assert_eq!(
        effective_skin_offset(5, &disabled),
        Some(SkinOffsetValue { x: 5, y: 30, w: 10, h: 12, r: 3, a: -255 })
    );

    let enabled = SkinDrawState { hidden_enabled: true, ..disabled };
    assert_eq!(effective_skin_offset(5, &enabled).unwrap().a, 0);
}

#[test]
fn judge_detail_offset_requires_an_explicit_destination_id() {
    let implicit: SkinDestinationDef = serde_json::from_str(
        r#"{ "id": "judge-early", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }"#,
    )
    .unwrap();
    let explicit: SkinDestinationDef = serde_json::from_str(
        r#"{ "id": "judge-early", "offset": 33, "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }"#,
    )
    .unwrap();
    let mut offsets = SkinOffsetValues::default();
    offsets.set(33, SkinOffsetValue { x: 8, y: 10, w: 4, h: 6, r: 1, a: -20 });
    let state = SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() };
    let original = ResolvedSkinFrame { x: 10, y: 20, w: 30, h: 40, a: 200, ..Default::default() };

    let mut implicit_frame = original;
    apply_skin_offset_to_frame(&implicit, &mut implicit_frame, &state, false);
    assert_eq!(implicit_frame.x, original.x);
    assert_eq!(implicit_frame.a, original.a);

    let mut explicit_frame = original;
    apply_skin_offset_to_frame(&explicit, &mut explicit_frame, &state, false);
    assert_eq!((explicit_frame.x, explicit_frame.y), (16, 27));
    assert_eq!((explicit_frame.w, explicit_frame.h), (34, 46));
    assert_eq!(explicit_frame.angle, 1);
    assert_eq!(explicit_frame.a, 180);
}

#[test]
fn animated_color_applies_offset_alpha_only_at_keyframes() {
    let varying: SkinDestinationDef = serde_json::from_str(
        r#"{
            "id": "image", "offset": 42,
            "dst": [
                { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10, "a": 100 },
                { "time": 1000, "a": 200 }
            ]
        }"#,
    )
    .unwrap();
    let fixed: SkinDestinationDef = serde_json::from_str(
        r#"{
            "id": "image", "offset": 42,
            "dst": [
                { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10, "a": 100 },
                { "time": 1000, "x": 100, "a": 100 }
            ]
        }"#,
    )
    .unwrap();
    let mut offsets = SkinOffsetValues::default();
    offsets.set(42, SkinOffsetValue { a: 40, ..Default::default() });
    let state = SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() };

    let mut keyframe = resolve_destination_frame(&varying, 0, &[], &state).unwrap();
    apply_skin_offset_to_frame(&varying, &mut keyframe, &state, false);
    assert_eq!(keyframe.a, 140);

    let mut interpolated = resolve_destination_frame(&varying, 500, &[], &state).unwrap();
    apply_skin_offset_to_frame(&varying, &mut interpolated, &state, false);
    assert_eq!(interpolated.a, 150);

    let mut fixed_color = resolve_destination_frame(&fixed, 500, &[], &state).unwrap();
    apply_skin_offset_to_frame(&fixed, &mut fixed_color, &state, false);
    assert_eq!(fixed_color.a, 140);
}
