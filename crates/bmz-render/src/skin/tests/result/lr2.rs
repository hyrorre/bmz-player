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

#[test]
fn lr2_select_option_buttons_show_cycled_profile_values() {
    let select = SkinDrawState { select_screen: true, ..Default::default() };
    let index = |button: i32, state: &SkinDrawState| {
        skin_state_event_index(LR2_BUTTON_BASE + button, state)
    };
    // RANDOM: H-RANDOM / ALL-SCR are LR2 values 4 / 5; R-RANDOM has no LR2 frame.
    for (arrange, expected) in [(6, 4), (7, 5), (3, -1)] {
        let state = SkinDrawState { select_extended_arrange_index: arrange, ..select.clone() };
        assert_eq!(index(42, &state), expected);
    }
    // EFFECT: OFF / HIDDEN / SUDDEN / HID+SUD.
    for (sudden, hidden, expected) in
        [(false, false, 0), (false, true, 1), (true, false, 2), (true, true, 3)]
    {
        let state =
            SkinDrawState { lanecover_enabled: sudden, hidden_enabled: hidden, ..select.clone() };
        assert_eq!(index(50, &state), expected);
    }
    let state = SkinDrawState { judge_timing_auto_adjust: true, ..select.clone() };
    assert_eq!((index(75, &select), index(75, &state)), (0, 1));
    let state = SkinDrawState { lr2_select_target_index: 6, ..select.clone() };
    assert_eq!(index(77, &state), 6);
    // Result screens keep these select-only buttons hidden.
    let result = SkinDrawState { select_screen: false, ..state };
    assert_eq!((index(50, &result), index(75, &result), index(77, &result)), (-1, -1, -1));
}

#[test]
fn lr2_numbers_follow_lr2_definitions() {
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    let mut state = SkinDrawState {
        play_screen: true,
        key_mode: KeyMode::K7,
        total_notes: 100,
        past_notes: 50,
        ex_score: 120,
        gauge: 77.9,
        target_ex_score: Some(160),
        judge_counts: crate::snapshot::DisplayJudgeCounts {
            pgreat: 50,
            great: 20,
            good: 5,
            bad: 3,
            poor: 4,
            empty_poor: 6,
        },
        ..Default::default()
    };
    // POOR includes empty POOR, unlike beatoraja's NUMBER_POOR (114).
    assert_eq!(lr2(114, &state), Some(10));
    assert_eq!(skin_state_number(114, &state), Some(4));
    assert_eq!(lr2(100, &state), Some((50 * 4 + 20 * 2 + 5) * 50_000 / 100));
    assert_eq!(lr2(107, &state), Some(76));
    // Single play compares with the target ghost at the current progress.
    assert_eq!(lr2(108, &state), Some(120 - 80));
    // 120 / 200 is rank 5 (B); A starts at 200 * 6 / 9 = 133.
    assert_eq!(lr2(109, &state), Some(120 - 133));
    // Result 1P POOR goes through the flip namespace to the same definition.
    state.play_screen = false;
    state.result_failed = Some(false);
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 14, &state), Some(10));
    state.lr2_result_flip = true;
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 34, &state), Some(10));
}

#[test]
fn lr2_select_ir_numbers_are_not_the_main_bpm() {
    let mut state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_chart_main_bpm: 150.0,
        select_ex_score: Some(900),
        rival_ex_score: Some(850),
        ..Default::default()
    };
    state.ir_ranking.rank = Some(12);
    state.ir_ranking.total_player = Some(345);
    let mut counts = [0; 11];
    counts[1] = 10; // FAILED
    counts[4] = 20; // EASY
    counts[6] = 10; // HARD
    state.ir_ranking.clear_counts = Some(counts);
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    assert_eq!(lr2(92, &state), Some(12));
    assert_eq!(skin_state_number(92, &state), Some(150));
    assert_eq!(lr2(93, &state), Some(345));
    assert_eq!(lr2(94, &state), Some(75));
    assert_eq!(lr2(95, &state), Some(50));
    // Folders have no per-song IR values.
    state.select_is_folder = true;
    assert_eq!((lr2(92, &state), lr2(95, &state)), (None, None));
}

#[test]
fn lr2_options_follow_lr2_definitions() {
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    let mut select = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_total_notes: 500,
        select_ex_score: Some(780), // 780 * 9 / 1000 = 7 (AA)
        select_clear_index: 7,      // EX-HARD is LR2's HARD lamp
        ..Default::default()
    };
    assert!(op(104, &select) && !op(103, &select) && !op(105, &select));
    assert!(op(111, &select) && !op(110, &select) && !op(112, &select));
    assert!(test_skin_ops(&[-(LR2_OPTION_BASE + 110)], &[], &select));
    // Ranks are hidden until the song has been played.
    select.select_clear_index = 0;
    assert!(op(100, &select) && !op(111, &select));
    select.select_is_folder = true;
    assert!(!op(100, &select));
    // Fixed values for settings BMZ does not have.
    for (id, expected) in [(52, true), (53, false), (54, true), (620, true), (621, false)] {
        assert_eq!(op(id, &select), expected, "option {id}");
    }
    // Result new rank 340..347 from the score just played.
    let result = SkinDrawState {
        result_failed: Some(false),
        total_notes: 500,
        ex_score: 900,
        ..Default::default()
    };
    assert!(op(340, &result) && !op(341, &result));
    let result = SkinDrawState { ex_score: 250, ..result };
    assert!(op(346, &result) && !op(347, &result));
}
