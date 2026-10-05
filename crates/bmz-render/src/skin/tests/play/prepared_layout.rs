use super::*;

#[test]
fn lr2_horizontal_scroll_aligns_notes_long_bodies_lines_and_lift() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":1000,"h":500,
        "image":[{"id":"beam","src":"1","w":2,"h":500}],
        "note":{"lr2Horizontal":true,"dst2":-20,
            "dst":[{"x":100,"y":200,"w":20,"h":300}, {"x":100,"y":300,"w":20,"h":200}],
            "lr2Dst":[
                {"lr2Timing":true,"dst":[{"x":100,"y":200,"w":20,"h":10}]},
                {"lr2Timing":true,"dst":[{"x":100,"y":300,"w":20,"h":10}]}],
            "group":[{"id":"beam","offset":3,"dst":[{"x":100,"y":0,"w":2,"h":500}]}]
        }
    }))
    .unwrap();
    assert_eq!(document.primary_note_lane_height_px(), Some(900));
    let skin = SkinContext::from_manifest_and_document(
        default_skin_manifest(),
        document,
        mock_source("1", 2.0, 500.0).into_values(),
    );
    for lift in [0, 90] {
        let state =
            SkinDrawState { lr2_horizontal: true, offset_lift_px: lift, ..Default::default() };
        let notes = skin.prepare_note_layout(KeyMode::K7, &state);
        for lane in [Lane::Key1, Lane::Key2] {
            let height = notes.note_height(lane).unwrap();
            let head = notes.note_rect(lane, 0.0, height).unwrap();
            let tail = notes.note_rect(lane, 0.5, height).unwrap();
            let body = notes.body_rect(lane, 0.0, 0.5).unwrap();
            assert!(approx_eq(head.x, (100 + lift) as f32 / 1000.0));
            assert!(approx_eq(tail.x, (550.0 + lift as f32 / 2.0) / 1000.0));
            assert!(approx_eq(head.y, if lane == Lane::Key1 { 0.58 } else { 0.38 }));
            assert_eq!(head.y, tail.y);
            assert!(approx_eq(body.x, head.x + head.width));
            assert!(approx_eq(body.x + body.width, tail.x));
            assert_eq!(body.height, head.height);
            assert!(approx_eq(notes.note_rect(lane, 1.0, height).unwrap().x, 1.0));
            assert!(approx_eq(notes.missed_rect(lane, 1.0, height).unwrap().x, -0.02));
            assert_eq!(
                Some(tail),
                skin.note_rect_for_progress(lane, KeyMode::K7, 0.5, height, &state)
            );
        }
        let lines = skin.document_bar_line_items(0.5, KeyMode::K7, &state);
        let SkinRenderItem::Image { rect, .. } = lines[0] else { panic!("line") };
        assert!(approx_eq(rect.x, (550.0 + lift as f32 / 2.0) / 1000.0));
        assert!(approx_eq(rect.y, 0.0));
        assert!(approx_eq(rect.height, 1.0));
        let offset = effective_skin_offset(3, &state).unwrap();
        assert_eq!(offset.x, lift);
        assert_eq!(offset.y, 0);
    }
}

#[test]
fn lr2_prepared_notes_animate_and_use_auto_sprites_only_for_assisted_lanes() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,
        "image":[{"id":"normal","src":"1","x":0,"w":20,"h":10,"divx":2,"cycle":100},
            {"id":"auto","src":"1","x":20,"w":20,"h":10,"divx":2,"cycle":100}],
        "note":{"note":["normal"],"lnstart":["normal"],"lnend":["normal"],"lnbody":["normal"],"mine":["normal"],
            "lr2Auto":{"note":["auto"],"lnstart":["auto"],"lnend":["auto"],"lnbody":["auto"],"mine":["auto"]},
            "dst":[{"x":10,"y":20,"w":10,"h":80}],
            "lr2Dst":[{"lr2Timing":true,"loop":200,"dst":[
                {"time":100,"x":10,"y":20,"w":10,"h":10,"r":128},
                {"time":200,"x":30,"y":10,"w":20,"h":20}]}]}
    })).unwrap();
    let skin = SkinContext::from_manifest_and_document(
        default_skin_manifest(),
        document,
        mock_source("1", 40.0, 10.0).into_values(),
    );
    let rect = Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 };
    for (time, assisted, autoplay, uv_x) in
        [(100, false, false, 0.0), (150, false, true, 0.25), (150, true, false, 0.75)]
    {
        let mut state = SkinDrawState { elapsed_ms: time, autoplay, ..Default::default() };
        state.auto_note_lanes[Lane::Key1.index()] = assisted;
        state.hold_ms[Lane::Key1.index()] = Some(50);
        let prepared = skin.prepare_note_layout(KeyMode::K7, &state);
        for item in [
            prepared.tap_item(Lane::Key1, rect, false),
            prepared.cap_item(Lane::Key1, rect, LongNoteMode::Ln, false),
            prepared.cap_item(Lane::Key1, rect, LongNoteMode::Ln, true),
            prepared.body_item(Lane::Key1, rect, LongNoteMode::Ln, LongBodyState::Processing),
            prepared.mine_item(Lane::Key1, rect),
        ] {
            let Some(SkinRenderItem::Image { uv, tint, .. }) = item else { panic!("note") };
            assert_eq!(uv.x, uv_x);
            assert!(approx_eq(tint.r, 128.0 / 255.0));
        }
        if time == 150 {
            let rect = prepared
                .note_rect(Lane::Key1, 0.0, prepared.note_height(Lane::Key1).unwrap())
                .unwrap();
            assert!(approx_eq(rect.x, 0.2));
            assert!(approx_eq(rect.y, 0.7));
            assert!(approx_eq(rect.width, 0.15));
            assert!(approx_eq(rect.height, 0.15));
        }
    }
    let state = SkinDrawState { elapsed_ms: 99, ..Default::default() };
    assert!(
        skin.prepare_note_layout(KeyMode::K7, &state).note_rect(Lane::Key1, 0.0, 0.1).is_none()
    );
}

#[test]
fn prepared_layout_matches_uncached_geometry_across_modes_and_frame_changes() {
    let frames: Vec<_> = (0..16)
        .map(|i| serde_json::json!({"x": i * 40, "y": 100 + i, "w": 38, "h": 500}))
        .collect();
    let mut document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w": 1280, "h": 720,
        "image": [{"id": "note", "src": "1", "w": 32, "h": 24, "divy": 2}],
        "note": {
            "id": "notes", "note": vec!["note"; 16], "size": [18], "dst2": 40,
            "dst": []
        },
        "destination": [{"id": "notes", "offset": 30, "offsets": [30, 31]}]
    }))
    .unwrap();
    // Keep conditional entries unexpanded to cover option changes between frames.
    let frames: Vec<SkinAnimationDef> =
        frames.into_iter().map(|frame| serde_json::from_value(frame).unwrap()).collect();
    document.note.as_mut().unwrap().dst =
        vec![SkinDstEntry::Conditional { if_ops: vec![900], frames }];
    let mut skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    for key_mode in [
        KeyMode::K4,
        KeyMode::K5,
        KeyMode::K6,
        KeyMode::K7,
        KeyMode::K8,
        KeyMode::K9,
        KeyMode::K10,
        KeyMode::K14,
    ] {
        for options in [vec![900], vec![], vec![900]] {
            skin.set_user_selected_options(options);
            for lift in [0, 72, 800] {
                let mut state = SkinDrawState { offset_lift_px: lift, ..Default::default() };
                state.skin_offsets.set(
                    30,
                    SkinOffsetValue { x: 3, y: -7, w: 4, h: 6, a: -40, ..Default::default() },
                );
                state
                    .skin_offsets
                    .set(31, SkinOffsetValue { x: -2, y: 4, w: -1, h: -10, ..Default::default() });
                let prepared = skin.prepare_note_layout(key_mode, &state);
                assert_eq!(prepared.alpha_offset(), skin.document_notes_offset_alpha(&state));
                for &lane in key_mode.active_lanes() {
                    let height = skin.document_note_height(lane, key_mode);
                    assert_eq!(prepared.note_height(lane), height);
                    let height = height.unwrap_or(0.02);
                    for progress in [-0.5, 0.0, 0.25, 1.0, 1.5] {
                        assert_eq!(
                            prepared.note_rect(lane, progress, height),
                            skin.note_rect_for_progress(lane, key_mode, progress, height, &state)
                        );
                        assert_eq!(
                            prepared.missed_rect(lane, progress, height),
                            skin.missed_note_rect_for_fall(
                                lane, key_mode, progress, height, &state
                            )
                        );
                        assert_eq!(
                            prepared.body_rect(lane, progress, 0.75),
                            skin.note_body_rect(lane, key_mode, progress, 0.75, &state)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn prepared_layout_preserves_missing_geometry_and_disabled_missed_notes() {
    for document in [
        serde_json::from_str("{}").unwrap(),
        serde_json::from_str(
            r#"{
        "w": 100, "h": 100,
        "note": {"id": "notes", "dst": [{"x": 10, "y": 20, "w": 30, "h": 60}]}
    }"#,
        )
        .unwrap(),
    ] {
        let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
        let state = SkinDrawState::default();
        let prepared = skin.prepare_note_layout(KeyMode::K7, &state);
        for lane in Lane::ALL {
            assert_eq!(
                prepared.note_rect(lane, 0.5, 0.02),
                skin.note_rect_for_progress(lane, KeyMode::K7, 0.5, 0.02, &state)
            );
            assert_eq!(prepared.body_rect(lane, 0.0, 0.5), None);
            assert_eq!(prepared.missed_rect(lane, 0.5, 0.02), None);
        }
    }
}
#[test]
fn prepared_tap_sprites_preserve_lane_mapping_missing_assets_and_duplicate_ids() {
    let mut images: Vec<_> = (0..16)
        .map(|i| {
            serde_json::json!({
                "id": format!("tap{i}"), "src": "atlas", "x": i * 8, "y": 4,
                "w": 16, "h": 8, "divx": 2, "cycle": 100
            })
        })
        .collect();
    // Notes use the first duplicate image definition, unlike the general image map.
    images.push(serde_json::json!({"id": "tap0", "src": "missing", "w": 10, "h": 10}));
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "image": images,
        "note": {
            "id": "notes",
            "note": (0..16).map(|i| format!("tap{i}")).collect::<Vec<_>>(),
            "processed": ["tap0", "missing"]
        }
    }))
    .unwrap();
    let mut rendered = 0;
    for size in [None, Some((128.0, 64.0)), Some((256.0, 128.0))] {
        let sources = size.map(|(width, height)| SkinDocumentTexture {
            source_id: "atlas".into(),
            texture: SkinTextureId(123),
            source_size: SkinImageSize { width, height },
        });
        let skin = SkinContext::from_manifest_and_document(
            default_skin_manifest(),
            document.clone(),
            sources,
        );
        for key_mode in [
            KeyMode::K4,
            KeyMode::K5,
            KeyMode::K6,
            KeyMode::K7,
            KeyMode::K8,
            KeyMode::K9,
            KeyMode::K10,
            KeyMode::K14,
        ] {
            let state = SkinDrawState::default();
            let prepared = skin.prepare_note_layout(key_mode, &state);
            for &lane in key_mode.active_lanes() {
                for processed in [false, true] {
                    for x in [0.1, 0.7] {
                        let rect = Rect { x, y: 0.3, width: 0.1, height: 0.02 };
                        let expected = if processed {
                            skin.document_processed_note_item(lane, key_mode, rect)
                        } else {
                            skin.document_note_item(lane, key_mode, rect)
                        };
                        let actual = prepared.tap_item(lane, rect, processed);
                        assert_eq!(actual, expected, "{key_mode:?} {lane:?} processed={processed}");
                        if let Some(SkinRenderItem::Image { texture, rect: actual_rect, .. }) =
                            actual
                        {
                            assert_eq!(texture, SkinTextureId(123));
                            assert_eq!(actual_rect, rect);
                            rendered += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(rendered > 0);
}
