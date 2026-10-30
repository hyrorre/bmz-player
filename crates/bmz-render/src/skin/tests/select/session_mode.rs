use super::*;

#[test]
fn session_mode_image_refs_match_numbers_and_events_in_every_scene() {
    let image: SkinImageDef = serde_json::from_value(serde_json::json!({
        "id": "mode", "w": 210, "h": 250, "divy": 5, "len": 5, "ref": 1970
    }))
    .unwrap();
    for select_screen in [true, false] {
        for selected in 0..5 {
            for frozen in [None, Some(0), Some(1), Some(2), Some(3), Some(4)] {
                let state = SkinDrawState {
                    select_screen,
                    select_session_mode_index: selected,
                    skin_attempt: SkinAttemptState {
                        session_mode_index: frozen,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let index = frozen.unwrap_or(selected);
                let ref_id = SKIN_REF_BMZ_SELECT_SESSION_MODE;
                assert_eq!(skin_state_number(ref_id, &state), Some(index as i64));
                assert_eq!(skin_state_event_index(ref_id, &state), index as i32);
                assert_eq!(skin_image_ref_number(ref_id, &state), Some(index as i64));
                assert_eq!(skin_state_imageset_index(ref_id, &state), Some(index));
                let uv = skin_image_texture_region_for_state(
                    &image,
                    SkinImageSize { width: 210.0, height: 250.0 },
                    &state,
                    (0, 0, 210, 250),
                );
                assert!(approx_eq(uv.y, index as f32 / 5.0));
                assert!(approx_eq(uv.height, 0.2));
            }
        }
    }
}

#[test]
fn select_session_mode_renders_sprite_sheet_and_imageset_in_the_same_order() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 5, "w": 100, "h": 100,
        "source": [{"id": 1, "path": "session.png"}],
        "image": [
            {"id": "sheet", "src": 1, "w": 210, "h": 250,
             "divy": 5, "len": 5, "ref": 1970},
            {"id": "normal", "src": 1, "y": 0, "w": 210, "h": 50},
            {"id": "practice", "src": 1, "y": 50, "w": 210, "h": 50},
            {"id": "autoplay", "src": 1, "y": 100, "w": 210, "h": 50},
            {"id": "auto-battle", "src": 1, "y": 150, "w": 210, "h": 50},
            {"id": "g-battle", "src": 1, "y": 200, "w": 210, "h": 50}
        ],
        "imageset": [{"id": "set", "ref": 1970,
            "images": ["normal", "practice", "autoplay", "auto-battle", "g-battle"]}],
        "destination": [
            {"id": "sheet", "dst": [{"x": 0, "y": 0, "w": 20, "h": 10}]},
            {"id": "set", "dst": [{"x": 20, "y": 0, "w": 20, "h": 10}]}
        ]
    }))
    .unwrap();
    let sources = mock_source("1", 210.0, 250.0);
    for (index, mode) in
        ["NORMAL", "PRACTICE", "AUTOPLAY", "AUTOPLAY BATTLE", "G-BATTLE"].into_iter().enumerate()
    {
        let snapshot = SelectSnapshot { assist: mode.to_string(), ..Default::default() };
        let items = document.select_render_items(&sources, &snapshot);
        assert_eq!(items.len(), 2, "{mode}");
        for item in items {
            assert!(
                matches!(item, SkinRenderItem::Image { uv, .. }
                if approx_eq(uv.y, index as f32 / 5.0) && approx_eq(uv.height, 0.2)),
                "{mode}"
            );
        }
    }
}
