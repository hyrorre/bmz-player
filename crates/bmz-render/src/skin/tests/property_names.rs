use super::*;
use serde_json::json;

#[test]
fn typed_integer_string_and_rate_names_follow_existing_id_evaluators() {
    let document: SkinDocument = serde_json::from_value(json!({
        "value": [{"value":"score"}, {"value":71}, {"value":"number(71) + 1"}, {"value":"unknown"}],
        "text": [{"value":"title"}, {"value":"unknown", "ref":10}],
        "slider": [{"value":"music_progress"}],
        "graph": [{"value":"music_progress"}, {"value":"score_rate"}]
    }))
    .unwrap();
    let state = SkinDrawState { ex_score: 123, play_progress: 0.25, ..Default::default() };
    assert_eq!(skin_value_number_for_destination(&document.value[0], &state), Some(123));
    assert_eq!(skin_value_number_for_destination(&document.value[1], &state), Some(123));
    assert_eq!(skin_value_number_for_destination(&document.value[2], &state), Some(124));
    assert_eq!(skin_value_number_for_destination(&document.value[3], &state), None);
    let text = SkinTextState { title: "named title", ..Default::default() };
    assert_eq!(skin_state_text(&document.text[0], &text), "named title");
    assert_eq!(skin_state_text(&document.text[1], &text), "");
    assert_eq!(skin_slider_progress(&document.slider[0], &state), Some(0.25));
    assert_eq!(graph_raw_value(&document.graph[0], &state), 0.25);
    assert_eq!(graph_raw_value(&document.graph[1], &state), 0.0);
}

#[test]
fn named_conditions_keep_negation_and_each_expression_and_boundary() {
    let state = SkinDrawState { bga_enabled: true, ex_score: 10, ..Default::default() };
    for (condition, expected) in [
        ("bgaon", true),
        ("!bgaon", false),
        ("!!bgaon", true),
        ("!!!bgaoff", true),
        ("unknown", false),
        ("!unknown", false),
        ("!!unknown", false),
    ] {
        assert_eq!(eval_skin_draw_condition(condition, &state), expected, "{condition}");
    }
    let mut destination: SkinDestinationDef = serde_json::from_value(json!({
        "op": ["bgaon", "option(41) or option(40)", "number(71) > 20"]
    }))
    .unwrap();
    assert!(!destination_ops_match(&destination, &[], &state));
    destination.op_expr[1] = "number(71) > 0".into();
    assert!(destination_ops_match(&destination, &[], &state));
    destination.op_expr[1] = "!unknown".into();
    assert!(!destination_ops_match(&destination, &[], &state));
}

#[test]
fn imageset_integer_value_is_distinct_from_index_ref_and_preserves_index_bounds() {
    let mut document: SkinDocument = serde_json::from_value(json!({
        "image": [
            {"id":"zero", "src":"source", "x":0, "w":1, "h":1},
            {"id":"one", "src":"source", "x":1, "w":1, "h":1}
        ],
        "imageset": [{"id":"set", "ref":330, "value":"rival_score", "images":["zero", "one"]}],
        "destination": [{"id":"set", "dst":[{"w":20, "h":20}]}]
    }))
    .unwrap();
    let sources = mock_source("source", 2.0, 1.0);
    for (score, expected_x) in [(-1, None), (0, Some(0.0)), (1, Some(0.5)), (2, Some(0.0))] {
        let state = SkinDrawState {
            rival_ex_score: Some(score),
            lanecover_enabled: false,
            ..Default::default()
        };
        let items = document.static_render_items(&sources, &state, &SkinTextState::default());
        match expected_x {
            None => assert!(items.is_empty()),
            Some(expected) => assert!(
                matches!(items.as_slice(), [SkinRenderItem::Image { uv, .. }] if uv.x == expected)
            ),
        }
    }
    document.imageset[0].value = None;
    let state = SkinDrawState { ex_score: 1, lanecover_enabled: false, ..Default::default() };
    let items = document.static_render_items(&sources, &state, &SkinTextState::default());
    assert!(matches!(items.as_slice(), [SkinRenderItem::Image { uv, .. }] if uv.x == 0.0));
}

#[test]
fn expression_conditions_gate_destinations_without_numeric_options() {
    let document: SkinDocument = serde_json::from_value(json!({
        "image": [{"id":"image", "src":"source", "w":1, "h":1}],
        "destination": [{"id":"image", "op":["number(71) > 0"], "dst":[{"w":20,"h":20}]}]
    }))
    .unwrap();
    let sources = mock_source("source", 1.0, 1.0);
    let visible = SkinDrawState { ex_score: 1, ..Default::default() };
    let hidden = SkinDrawState { ex_score: 0, ..Default::default() };
    assert_eq!(
        document.static_render_items(&sources, &visible, &SkinTextState::default()).len(),
        1
    );
    assert!(document.static_render_items(&sources, &hidden, &SkinTextState::default()).is_empty());
}
