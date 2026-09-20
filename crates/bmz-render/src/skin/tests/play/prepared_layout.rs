use super::*;

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
