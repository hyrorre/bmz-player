//! Select key filter image indices: beatoraja `ref=11` and BMZ `ref=19250`.
use super::*;

const ALL_FILTER_MODES: [(&str, usize, usize); 14] = [
    // (filter, ref=11, ref=19250)
    ("ALL", 0, 0),
    ("5K", 1, 1),
    ("7K", 2, 2),
    ("10K", 3, 3),
    ("14K", 4, 4),
    ("9K", 5, 5),
    ("24K", 6, 6),
    ("24K_DOUBLE", 7, 7),
    ("4K", 0, 10),
    ("6K", 0, 11),
    ("8K", 0, 12),
    ("", 0, 0),
    ("unknown", 0, 0),
    ("SINGLE", 0, 0),
];

fn select_state(mode: &str) -> SkinDrawState {
    SkinDrawState {
        select_screen: true,
        select_mode_index: select_mode_index(mode),
        select_mode_extended_index: select_mode_index_extended(mode),
        ..SkinDrawState::default()
    }
}

fn select_snapshot(mode: &str) -> SelectSnapshot {
    SelectSnapshot { select_mode: mode.to_string(), ..SelectSnapshot::default() }
}

/// 13 cells of 10x10 in a 130x130 source: imageset images `m0..m12` are laid out
/// horizontally and the `row` image splits the first column into 13 rows.
fn mode_document(element: &str) -> SkinDocument {
    let images = (0..13)
        .map(|index| {
            format!(r#"{{"id":"m{index}","src":"s","x":{},"y":0,"w":10,"h":10}}"#, index * 10)
        })
        .collect::<Vec<_>>()
        .join(",");
    serde_json::from_str(&format!(
        r#"{{
            "type": 5, "w": 100, "h": 100,
            "source": [{{"id": "s", "path": "mode.png"}}],
            "image": [{images}, {element}],
            "imageset": [
                {{"id": "set19250", "ref": 19250, "images": ["m0","m1","m2","m3","m4","m5","m6","m7","m8","m9","m10","m11","m12"]}},
                {{"id": "set11", "ref": 11, "images": ["m0","m1","m2","m3","m4","m5","m6","m7"]}},
                {{"id": "set19250_short", "ref": 19250, "images": ["m0","m1","m2","m3","m4","m5","m6","m7"]}},
                {{"id": "set_value", "value": "bmz_select_mode_index", "images": ["m0","m1","m2","m3","m4","m5","m6","m7","m8","m9","m10","m11","m12"]}},
                {{"id": "set_value_short", "value": "bmz_select_mode_index", "images": ["m0","m1","m2","m3","m4","m5","m6","m7"]}}
            ],
            "destination": []
        }}"#
    ))
    .unwrap()
}

fn with_destination(mut document: SkinDocument, id: &str) -> SkinDocument {
    document.destination = vec![
        serde_json::from_value(serde_json::json!({
            "id": id, "dst": [{"x": 0, "y": 0, "w": 10, "h": 10}]
        }))
        .unwrap(),
    ];
    document
}

/// Returns the 10px source cell index (`x` for imagesets, `y` for the row image).
fn rendered_cell(document: &SkinDocument, mode: &str, vertical: bool) -> Option<usize> {
    let sources = mock_source("s", 130.0, 130.0);
    let items = document.select_render_items(&sources, &select_snapshot(mode));
    let uvs = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { uv, .. } => Some(*uv),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(uvs.len() <= 1, "{uvs:?}");
    uvs.first().map(|uv| ((if vertical { uv.y } else { uv.x }) * 13.0).round() as usize)
}

#[test]
fn legacy_ref_11_keeps_beatoraja_indices_and_maps_4k_6k_8k_to_all() {
    for (mode, legacy, _) in ALL_FILTER_MODES {
        assert_eq!(select_mode_index(mode), legacy, "{mode}");
        let state = select_state(mode);
        assert_eq!(skin_state_number(11, &state), Some(legacy as i64), "{mode}");
        assert_eq!(skin_image_ref_number(11, &state), Some(legacy as i64), "{mode}");
    }
}

#[test]
fn bmz_ref_19250_extends_select_mode_index_for_4k_6k_8k() {
    for (mode, _, extended) in ALL_FILTER_MODES {
        assert_eq!(select_mode_index_extended(mode), extended, "{mode}");
    }
    for (mode, expected) in [("ALL", 0), ("4K", 10), ("6K", 11), ("8K", 12), ("7K", 2), ("14K", 4)]
    {
        assert_eq!(select_mode_index_extended(mode), expected, "{mode}");
    }
}

#[test]
fn bmz_ref_19250_number_event_index_and_expressions_follow_the_select_filter() {
    assert_eq!(SKIN_REF_BMZ_SELECT_MODE_INDEX, 19_250);
    for (mode, legacy, extended) in ALL_FILTER_MODES {
        let state = select_state(mode);
        let id = SKIN_REF_BMZ_SELECT_MODE_INDEX;
        assert_eq!(skin_state_number(id, &state), Some(extended as i64), "{mode}");
        assert_eq!(skin_image_ref_number(id, &state), Some(extended as i64), "{mode}");
        assert_eq!(skin_state_event_index(id, &state), extended as i32, "{mode}");
        assert_eq!(lua_main_state_number(id, &state), extended as i64, "{mode}");
        assert_eq!(lua_main_state_event_index(id, &state), extended as i32, "{mode}");
        assert_eq!(eval_skin_draw_operand("number(19250)", &state), Some(extended as f32));
        assert_eq!(eval_skin_draw_operand("event_index(19250)", &state), Some(extended as f32));
        assert_eq!(
            skin_integer_property(
                &SkinPropertyValue::Expression("bmz_select_mode_index".to_string()),
                &state
            ),
            Some(extended as i64),
            "{mode}"
        );
        // ref=11 and the chart key mode ref stay independent of the extended index.
        assert_eq!(skin_state_number(11, &state), Some(legacy as i64), "{mode}");
    }
}

#[test]
fn bmz_ref_19250_is_select_only() {
    let mut state = select_state("6K");
    state.select_screen = false;
    let id = SKIN_REF_BMZ_SELECT_MODE_INDEX;
    assert_eq!(skin_state_number(id, &state), None);
    assert_eq!(skin_image_ref_number(id, &state), None);
    assert_eq!(skin_state_event_index(id, &state), -1);
    assert_eq!(lua_main_state_event_index(id, &state), -1);
    assert_eq!(
        skin_integer_property(
            &SkinPropertyValue::Expression("bmz_select_mode_index".to_string()),
            &state
        ),
        None
    );
    // Same unavailable handling as the legacy ref=11 outside Select.
    assert_eq!(skin_state_number(11, &state), None);
    assert_eq!(skin_image_ref_number(11, &state), None);
}

#[test]
fn bmz_ref_19250_is_independent_of_chart_key_mode() {
    // The selected chart's key mode (ref=1903) and the key filter do not follow each other.
    for (filter, chart, filter_index, chart_number) in
        [("4K", KeyMode::K7, 10, 7), ("7K", KeyMode::K4, 2, 4), ("8K", KeyMode::K14, 12, 14)]
    {
        let state = SkinDrawState {
            select_row_kind: SelectRowKind::Song,
            select_chart_key_mode: Some(chart),
            ..select_state(filter)
        };
        let id = SKIN_REF_BMZ_SELECT_MODE_INDEX;
        assert_eq!(skin_state_number(id, &state), Some(filter_index), "{filter}");
        assert_eq!(skin_state_number(SKIN_REF_BMZ_KEY_MODE, &state), Some(chart_number));
    }
}

#[test]
fn imageset_ref_19250_selects_4k_6k_8k_images_and_updates_with_the_filter() {
    let document = with_destination(mode_document(r#"{"id":"unused","src":"s"}"#), "set19250");
    for (mode, _, extended) in ALL_FILTER_MODES {
        assert_eq!(rendered_cell(&document, mode, false), Some(extended), "{mode}");
    }
    // Switching the filter between frames changes the image immediately.
    assert_eq!(rendered_cell(&document, "4K", false), Some(10));
    assert_eq!(rendered_cell(&document, "8K", false), Some(12));
    assert_eq!(rendered_cell(&document, "6K", false), Some(11));
}

#[test]
fn imageset_ref_11_keeps_the_existing_eight_image_selection() {
    let document = with_destination(mode_document(r#"{"id":"unused","src":"s"}"#), "set11");
    for (mode, legacy, _) in ALL_FILTER_MODES {
        assert_eq!(rendered_cell(&document, mode, false), Some(legacy), "{mode}");
    }
}

#[test]
fn imageset_value_bmz_select_mode_index_uses_the_integer_property_path() {
    let document = with_destination(mode_document(r#"{"id":"unused","src":"s"}"#), "set_value");
    for (mode, _, extended) in ALL_FILTER_MODES {
        assert_eq!(rendered_cell(&document, mode, false), Some(extended), "{mode}");
    }
}

#[test]
fn short_imagesets_keep_the_existing_out_of_range_fallbacks() {
    // imageset.ref clamps to the last image; imageset.value falls back to the first.
    let by_ref = with_destination(mode_document(r#"{"id":"unused","src":"s"}"#), "set19250_short");
    let by_value =
        with_destination(mode_document(r#"{"id":"unused","src":"s"}"#), "set_value_short");
    for mode in ["4K", "6K", "8K"] {
        assert_eq!(rendered_cell(&by_ref, mode, false), Some(7), "{mode}");
        assert_eq!(rendered_cell(&by_value, mode, false), Some(0), "{mode}");
    }
    assert_eq!(rendered_cell(&by_ref, "24K_DOUBLE", false), Some(7));
    assert_eq!(rendered_cell(&by_value, "9K", false), Some(5));
}

#[test]
fn image_ref_19250_selects_the_row_and_clamps_to_divy() {
    let rows = with_destination(
        mode_document(r#"{"id":"row","src":"s","x":0,"y":0,"w":10,"h":130,"divy":13,"ref":19250}"#),
        "row",
    );
    for (mode, _, extended) in ALL_FILTER_MODES {
        assert_eq!(rendered_cell(&rows, mode, true), Some(extended), "{mode}");
    }

    // Existing 8-row sheets clamp to their last row instead of reading past it.
    let short = with_destination(
        mode_document(r#"{"id":"row","src":"s","x":0,"y":0,"w":10,"h":80,"divy":8,"ref":19250}"#),
        "row",
    );
    let sources = mock_source("s", 130.0, 130.0);
    let uv = |mode: &str| {
        short.select_render_items(&sources, &select_snapshot(mode)).iter().find_map(|item| {
            match item {
                SkinRenderItem::Image { uv, .. } => Some(*uv),
                _ => None,
            }
        })
    };
    assert!(approx_eq(uv("8K").unwrap().y, 70.0 / 130.0));
    assert!(approx_eq(uv("7K").unwrap().y, 20.0 / 130.0));

    let legacy = with_destination(
        mode_document(r#"{"id":"row","src":"s","x":0,"y":0,"w":10,"h":80,"divy":8,"ref":11}"#),
        "row",
    );
    for (mode, legacy_index, _) in ALL_FILTER_MODES {
        let uv = legacy
            .select_render_items(&sources, &select_snapshot(mode))
            .iter()
            .find_map(|item| match item {
                SkinRenderItem::Image { uv, .. } => Some(*uv),
                _ => None,
            })
            .unwrap();
        assert!(approx_eq(uv.y, legacy_index as f32 * 10.0 / 130.0), "{mode}");
    }
}
