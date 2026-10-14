use super::*;

fn scroll_document(lr2_style: Option<i32>) -> SkinDocument {
    let mut frame = serde_json::json!({
        "time":1000,"x":1221,"y":501,"w":66,"h":72,"a":0
    });
    if let Some(blend) = lr2_style {
        frame["lr2Style"] = serde_json::json!({"blend":blend,"filter":0,"center":0});
    }
    serde_json::from_value(serde_json::json!({
        "type":5,"lr2":true,"w":1280,"h":720,
        "slider":[{"id":"scroll","src":"src","w":66,"h":72,
            "angle":2,"range":350,"type":1}],
        "destination":[{"id":"scroll","loop":0,"lr2Timing":true,"dst":[
            frame,{"time":1100,"a":255}
        ]}]
    }))
    .unwrap()
}

#[test]
fn lr2_scroll_slider_blend_zero_stays_visible_and_tracks_clicks_after_looping() {
    let doc = scroll_document(Some(0));
    let sources = mock_source("src", 66.0, 72.0);
    let settings = crate::select_settings_dest::SelectSettingsDestIndex::default();
    for elapsed_ms in [1000, 1050, 1100, 1500, 5000] {
        for index in [0, 50, 100] {
            let snapshot = SelectSnapshot {
                time: TimeUs(i64::from(elapsed_ms) * 1000),
                chart_count: 101,
                selected_index: index,
                ..Default::default()
            };
            let items = doc.select_render_items(&sources, &snapshot);
            let [SkinRenderItem::Image { rect, tint, .. }] = items.as_slice() else {
                panic!("one scroll thumb at {elapsed_ms} ms");
            };
            assert_eq!(tint.a, 1.0, "blend=0 must ignore DST alpha at {elapsed_ms} ms");
            let progress = index as f32 / 100.0;
            assert!(approx_eq(rect.y, (147.0 + 350.0 * progress) / 720.0));
            let hit = doc
                .select_slider_hit(
                    &snapshot,
                    &settings,
                    rect.x + rect.width * 0.5,
                    rect.y + rect.height * 0.5,
                )
                .expect("visible thumb is draggable");
            assert_eq!(hit.slider_type, 1);
            assert!(approx_eq(hit.value, progress));
            for (mouse_y, expected) in [(-1.0, 0.0), (2.0, 1.0)] {
                let dragged =
                    doc.select_slider_drag(&snapshot, &settings, 1, 0.0, mouse_y).unwrap();
                assert_eq!(dragged.value, expected);
            }
        }
    }
    assert!(
        doc.select_render_items(
            &sources,
            &SelectSnapshot { time: TimeUs(999_000), ..Default::default() }
        )
        .is_empty(),
        "blend=0 does not bypass the activation time"
    );
}

#[test]
fn slider_dst_alpha_is_preserved_without_lr2_blend_zero() {
    let sources = mock_source("src", 66.0, 72.0);
    for style in [None, Some(1)] {
        let doc = scroll_document(style);
        for (elapsed_ms, alpha) in [(1000, 0.0), (1050, 0.5), (1100, 1.0), (1500, 0.0)] {
            let snapshot =
                SelectSnapshot { time: TimeUs(i64::from(elapsed_ms) * 1000), ..Default::default() };
            let items = doc.select_render_items(&sources, &snapshot);
            let [SkinRenderItem::Image { tint, .. }] = items.as_slice() else {
                panic!("one thumb");
            };
            assert!((tint.a - alpha).abs() < 0.01, "style={style:?}, time={elapsed_ms}");
        }
    }
}
