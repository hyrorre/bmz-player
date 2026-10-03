use super::*;

#[test]
fn lr2_songlist_conditional_variants_keep_fixed_slots() {
    let entries: Vec<DestinationListEntry> = serde_json::from_value(serde_json::json!([
        {"if": [], "values": [{"id":"closed","op":[-21]}, {"id":"open","op":[21]}]},
        {"id":"next"}
    ]))
    .unwrap();
    let state = SkinDrawState { select_option_panel: 1, ..Default::default() };
    assert_eq!(songlist_destination_at(&entries, 0, &[], &state, true).unwrap().id, "open");
    assert_eq!(songlist_destination_at(&entries, 1, &[], &state, true).unwrap().id, "next");
}

#[test]
fn lr2_result_flip_uses_player_values_without_fabricating_an_opponent() {
    let mut state = SkinDrawState {
        ex_score: 1234,
        target_ex_score: Some(2345),
        result_failed: Some(false),
        ..Default::default()
    };
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 1, &state), Some(1234));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 21, &state), None);
    state.lr2_result_flip = true;
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 1, &state), None);
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 21, &state), Some(1234));
    state.total_notes = 1000;
    assert!(test_skin_ops(&[LR2_RESULT_RANK_BASE + 13], &[], &state));
    assert!(!test_skin_ops(&[LR2_RESULT_RANK_BASE + 3], &[], &state));
    state.lr2_result_flip = false;
    assert!(test_skin_ops(&[LR2_RESULT_RANK_BASE + 3], &[], &state));
    assert!(!test_skin_ops(&[LR2_RESULT_RANK_BASE + 13], &[], &state));
}

#[test]
fn lr2_charts_animate_with_cache_and_filter_the_selected_gauge() {
    let mut doc: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,"lr2":true,
        "image":[{"id":"gauge","src":"src","w":2,"h":2}],
        "lr2Charts":[{"id":"gauge","score":false,"player":0,"index":0,"width":10,"height":80,"start":500,"end":1500}],
        "destination":[{"id":"gauge","dst":[{"x":5,"y":10,"w":2,"h":2,"a":0,"lr2Style":{"blend":0,"filter":1,"center":0}}]}]
    })).unwrap();
    doc.result_gauge_graph_points = vec![
        crate::snapshot::ResultGaugeGraphPoint {
            time_ms: 0,
            value: 20.0,
            max: 100.0,
            border: 80.0,
            gauge_type: 2,
            course_section_start: false,
        },
        crate::snapshot::ResultGaugeGraphPoint {
            time_ms: 0,
            value: 100.0,
            max: 100.0,
            border: 0.0,
            gauge_type: 3,
            course_section_start: false,
        },
    ];
    let sources = mock_source("src", 2.0, 2.0);
    let mut cache = ResultRenderCache::default();
    for (elapsed, count) in [(499, 0), (1000, 3), (1500, 5), (2000, 5)] {
        let state = SkinDrawState {
            elapsed_ms: elapsed,
            gauge_type: 2,
            result_failed: Some(false),
            ..Default::default()
        };
        let items = doc.static_render_items_with_graphs_cached(
            &sources,
            &state,
            &SkinTextState::default(),
            SkinRuntimeGraphs::from_document(&doc),
            Some(&mut cache),
        );
        assert_eq!(items.len(), count, "at {elapsed}");
        for item in items {
            let SkinRenderItem::Image { rect, tint, .. } = item else { panic!("image") };
            assert_eq!(tint.a, 1.0, "LR2 blend 0 ignores DST alpha");
            assert!((rect.y - 0.72).abs() < 0.001, "wrong gauge series: {rect:?}");
        }
    }
    let state = SkinDrawState {
        elapsed_ms: 700,
        gauge_type: 2,
        result_failed: Some(false),
        result_graph_end_ms: Some(0),
        ..Default::default()
    };
    assert_eq!(doc.static_image_render_items(&sources, &state).len(), 5);
    doc.lr2_result = Some(SkinLr2ResultDef { flip: true, ..Default::default() });
    assert!(doc.static_image_render_items(&sources, &state).is_empty());
    doc.lr2_charts[0].player = 1;
    assert_eq!(doc.static_image_render_items(&sources, &state).len(), 5);
    let mut late = doc.result_gauge_graph_points[0];
    late.time_ms = 900;
    late.value = 40.0;
    doc.result_gauge_graph_points.push(late);
    let state = SkinDrawState { result_duration_ms: 1000, ..state };
    let items = doc.static_image_render_items(&sources, &state);
    for item in items.iter().take(4) {
        let SkinRenderItem::Image { rect, .. } = item else { panic!("image") };
        assert!((rect.y - 0.72).abs() < 0.001, "time distribution: {rect:?}");
    }
    let SkinRenderItem::Image { rect, .. } = items.last().unwrap() else { panic!("image") };
    assert!((rect.y - 0.56).abs() < 0.001, "last sample: {rect:?}");
}

#[test]
fn lr2_button_indices_and_click_panel_restrictions_are_separate_from_bmz() {
    let state = SkinDrawState {
        select_screen: true,
        select_gauge_index: 2,
        select_extended_arrange_index: 4,
        select_hs_fix_index: 4,
        ..Default::default()
    };
    for (button, expected) in [(40, 0), (42, 3), (55, 2)] {
        assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + button, &state), expected);
    }
    assert_eq!(skin_state_event_index(40, &state), 2);
    assert_eq!(
        skin_state_event_index(
            LR2_BUTTON_BASE + 55,
            &SkinDrawState { select_hs_fix_index: 1, ..state.clone() }
        ),
        -1
    );
    let doc: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,"lr2":true,
        "image":[{"id":"button","src":"src","w":10,"h":10,"act":90040,"clickable":true,"lr2Panel":1}],
        "destination":[{"id":"button","dst":[{"x":10,"y":10,"w":10,"h":10}]}]
    })).unwrap();
    assert!(doc.result_click_hit(&state, 0.15, 0.85).is_none());
    let state = SkinDrawState { select_option_panel: 1, ..state };
    assert!(doc.result_click_hit(&state, 0.15, 0.85).is_some());
}
