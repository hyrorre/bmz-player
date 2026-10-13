use super::*;

fn document(lr2: bool) -> SkinDocument {
    let destinations: Vec<_> = (0..9)
        .map(|slot| {
            serde_json::json!({
                "id": "bars", "dst": [{ "x": 20 + slot * slot * 4,
            "y": 80 + slot * slot * 9, "w": 100 + slot * 2, "h": 20 + slot }]
            })
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "type": 5, "w": 1000, "h": 1000, "lr2": lr2,
        "source": [{"id": "source", "path": "bar.png"}],
        "image": [{"id": "bar", "src": "source", "w": 16, "h": 16},
                  {"id": "lamp", "src": "source", "w": 4, "h": 4}],
        "imageset": [{"id": "bars", "images": vec!["bar"; 12]}],
        "songlist": {"id": "list", "center": 4, "clickable": [0,1,2,3,4,5,6,7,8],
            "liston": destinations, "listoff": destinations,
            "lamp": [{"id": "lamp", "dst": [{"x": 3, "y": 5, "w": 4, "h": 4}]}],
            "flash": [{"id": "lamp", "dst": [{"x": 0, "y": 0, "w": 100, "h": 20}]}]},
        "destination": [{"id": "list"}]
    }))
    .unwrap()
}

fn rows(center: u32) -> Vec<SelectRowSnapshot> {
    (0..9)
        .map(|slot| {
            let index = (center + 31 - 4 + slot) % 31;
            SelectRowSnapshot {
                index,
                title: format!("row{index}"),
                play_level: index.to_string(),
                ..Default::default()
            }
        })
        .collect()
}

fn snapshot(selected: u32, remaining: f32) -> SelectSnapshot {
    SelectSnapshot {
        chart_count: 31,
        selected_index: selected,
        rows: rows(selected),
        bar_scroll_direction: remaining.signum() as i32,
        bar_scroll_progress: remaining.abs(),
        ..Default::default()
    }
}

fn row_rect(document: &SkinDocument, snapshot: &SelectSnapshot, index: u32) -> Rect {
    let sources = mock_source("source", 16.0, 16.0);
    let hits = document.select_click_hits(&sources, snapshot, &Default::default());
    let rect = hits
        .iter()
        .find_map(|hit| match hit.target {
            SkinClickTarget::SelectRow { row_index } if row_index == index => Some(hit.rect),
            _ => None,
        })
        .expect("visible clickable row");
    assert!(document.select_render_items(&sources, snapshot).iter().any(|item|
        matches!(item, SkinRenderItem::Image { rect: image_rect, .. } if image_rect == &rect)),
        "draw and click coordinates must agree");
    let lamp_x = rect.x + 0.003;
    let lamp_y = rect.y + rect.height - 0.009;
    assert!(
        document.select_render_items(&sources, snapshot).iter().any(
            |item| matches!(item, SkinRenderItem::Image { rect, .. }
            if (rect.x - lamp_x).abs() < 1e-6 && (rect.y - lamp_y).abs() < 1e-6
                && (rect.width - 0.004).abs() < 1e-6)
        ),
        "lamp must follow the same interpolated row"
    );
    rect
}

#[test]
fn select_scroll_center_flash_stays_fixed_across_whole_row_boundaries() {
    for lr2 in [false, true] {
        let document = document(lr2);
        let sources = mock_source("source", 16.0, 16.0);
        for remaining in [0.0, 0.4, 0.999_999, 1.0, 1.4, -0.6, -1.0, -1.4] {
            let items = document.select_render_items(&sources, &snapshot(5, remaining));
            let flash = items
                .iter()
                .find_map(|item| match item {
                    SkinRenderItem::Image { rect, uv, .. }
                        if (uv.width - 0.25).abs() < 1e-6 && (rect.width - 0.1).abs() < 1e-6 =>
                    {
                        Some(rect)
                    }
                    _ => None,
                })
                .expect("center flash");
            assert!((flash.x - 0.084).abs() < 1e-6);
            assert!((flash.y - 0.756).abs() < 1e-6);
        }
    }
}

#[test]
fn select_scroll_preserves_nonuniform_positions_on_repeat_reverse_and_wrap() {
    for lr2 in [false, true] {
        let document = document(lr2);
        // 0.4 remaining + next step = 1.4: the same physical rows cannot jump.
        let before = snapshot(4, 0.4);
        let after = snapshot(5, 1.4);
        for index in [2, 3, 4, 5, 6] {
            assert_eq!(row_rect(&document, &before, index), row_rect(&document, &after, index));
        }
        let before = snapshot(5, 0.4);
        let after = snapshot(4, -0.6);
        for index in [3, 4, 5, 6] {
            assert_eq!(row_rect(&document, &before, index), row_rect(&document, &after, index));
        }
        let before = snapshot(30, 0.4);
        let after = snapshot(0, 1.4);
        for index in [28, 29, 30, 0, 1] {
            assert_eq!(row_rect(&document, &before, index), row_rect(&document, &after, index));
        }
    }
}

#[test]
fn select_scroll_visual_window_keeps_large_backlogs_and_logical_selection_separate() {
    let document = document(false);
    let before = snapshot(4, 0.4);
    let mut after = snapshot(20, 16.4);
    after.bar_scroll_rows = Some(rows(4));
    for index in [2, 3, 4, 5, 6] {
        assert_eq!(row_rect(&document, &before, index), row_rect(&document, &after, index));
    }
    let (_, selected) = document.select_draw_state(&after, None);
    assert_eq!(selected.unwrap().index, 20);

    // Crossing a whole-row boundary switches windows without moving any row.
    let mut boundary = snapshot(5, 1.0);
    boundary.bar_scroll_rows = Some(rows(4));
    let rest = snapshot(4, 0.0);
    assert_eq!(row_rect(&document, &boundary, 4), row_rect(&document, &rest, 4));
    let near_boundary = snapshot(5, 0.999_999);
    assert_eq!(row_rect(&document, &boundary, 4), row_rect(&document, &near_boundary, 4));
}
