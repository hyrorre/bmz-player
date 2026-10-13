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

#[test]
fn lr2_select_numbers_follow_lr2_definitions() {
    let mut state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_total_notes: 500,
        select_ex_score: Some(780),
        select_play_level: 12,
        difficulty: 4,
        judge_timing_offset_ms: -15,
        select_sort_index: 2,
        select_double_option_index: 1,
        key_mode: KeyMode::K7,
        judge_counts: crate::snapshot::DisplayJudgeCounts {
            pgreat: 300,
            great: 180,
            good: 15,
            ..Default::default()
        },
        ..Default::default()
    };
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    // Judge timing instead of the sort; beatoraja's 12 stays the sort on select.
    assert_eq!((lr2(12, &state), skin_state_number(12, &state)), (Some(-15), Some(2)));
    // Only the selected chart's difficulty slot (ANOTHER) has a level.
    assert_eq!(lr2(48, &state), Some(12));
    assert_eq!((lr2(45, &state), lr2(49, &state)), (None, None));
    // EQ4 is not BMZ's DP option index.
    assert_eq!((lr2(54, &state), skin_state_number(54, &state)), (None, Some(1)));
    assert_eq!(lr2(70, &state), Some((300 * 4 + 180 * 2 + 15) * 50_000 / 500));
    assert_eq!(lr2(73, &state), Some(78));
    // IR clear classes: FAILED / EASY (assist + easy) / CLEAR / HARD (+EX-HARD) / FULLCOMBO.
    let mut counts = [0; 11];
    (counts[1], counts[2], counts[4], counts[5], counts[7], counts[10]) = (5, 1, 4, 6, 2, 2);
    state.ir_ranking.state = crate::scene::ResultIrState::Loaded;
    state.ir_ranking.clear_counts = Some(counts);
    let classes = (210..=219).map(|id| lr2(id, &state)).collect::<Vec<_>>();
    assert_eq!(classes, [5, 25, 5, 25, 6, 30, 2, 10, 2, 10].map(Some).to_vec(),);
}

#[test]
fn lr2_select_panel_option_and_selected_score_rate_graphs() {
    let mut state = SkinDrawState {
        select_screen: true,
        select_total_notes: 400,
        judge_counts: crate::snapshot::DisplayJudgeCounts { pgreat: 100, ..Default::default() },
        ..Default::default()
    };
    assert!(test_skin_ops(&[LR2_OPTION_BASE + 20], &[], &state));
    state.select_option_panel = 1;
    assert!(!test_skin_ops(&[LR2_OPTION_BASE + 20], &[], &state));
    let graph: SkinGraphDef =
        serde_json::from_value(serde_json::json!({"id": "pg", "type": 140})).unwrap();
    assert!((graph_raw_value(&graph, &state) - 0.25).abs() < 1e-6);
}

#[test]
fn lr2_ghost_rival_and_lamp_challenge_values() {
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    // Play: best / target ghosts at half progress of a 100-note chart.
    let play = SkinDrawState {
        play_screen: true,
        total_notes: 100,
        past_notes: 50,
        ex_score: 90,
        best_ex_score: Some(150),
        target_ex_score: Some(180),
        ..Default::default()
    };
    assert_eq!((lr2(150, &play), lr2(151, &play)), (Some(75), Some(90)));
    assert_eq!((lr2(155, &play), lr2(156, &play)), (Some(37), Some(50)));
    assert_eq!((lr2(157, &play), lr2(158, &play)), (Some(45), Some(0)));
    // Single play 2P numbers follow the target ghost.
    let two_p = |id: i32| skin_state_number(SKIN_REF_BMZ_LR2_2P_BASE + id - 120, &play);
    assert_eq!((two_p(121), two_p(122), two_p(128)), (Some(90), Some(90), Some(0)));
    assert!(op(354, &play) && !op(352, &play) && !op(353, &play));

    // Select: rival lamp / rank / max EX and the lamp the current settings can earn.
    let mut select = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_total_notes: 500,
        rival_ex_score: Some(820),
        rival_clear_index: Some(6),
        select_gauge_index: 5,
        ..Default::default()
    };
    assert!(op(644, &select) && !op(643, &select));
    assert!(op(651, &select) && !op(650, &select));
    assert_eq!(
        (lr2(272, &select), lr2(273, &select), lr2(274, &select)),
        (Some(1000), Some(82), Some(500))
    );
    assert!(op(66, &select));
    select.select_extended_arrange_index = 6; // H-RANDOM
    assert!(op(63, &select));
    select.select_session_mode_index = 2; // AUTOPLAY
    assert!(op(62, &select));
}

#[test]
fn lr2_level_bar_uses_same_folder_difficulties() {
    use crate::scene::SelectSameFolderDifficulty as Slot;
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    let empty = Slot::default();
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_chart_key_mode: Some(KeyMode::K7),
        select_same_folder: Some([
            empty,
            Slot { charts: 1, level: Some(5), clear_index: 4 },
            Slot { charts: 1, level: Some(9), clear_index: 0 },
            Slot { charts: 2, level: Some(12), clear_index: 7 },
            Slot { charts: 1, level: Some(13), clear_index: 10 },
        ]),
        ..Default::default()
    };
    // Levels per difficulty; a missing difficulty has no level.
    assert_eq!(lr2(45, &state), None);
    assert_eq!((lr2(46, &state), lr2(48, &state), lr2(49, &state)), (Some(5), Some(12), Some(13)));
    // Missing / present / one chart / several charts.
    assert!(op(500, &state) && !op(505, &state));
    assert!(op(506, &state) && op(511, &state) && op(518, &state) && !op(513, &state));
    // Lamps per difficulty: EASY is LR2 lamp 2, EX-HARD folds into HARD (4), MAX into FC (5).
    assert!(op(532, &state) && op(540, &state) && op(554, &state) && op(565, &state));
    assert!(op(520, &state));
    // 7 keys flash above level 12.
    assert!(op(73, &state) && !op(78, &state) && op(79, &state) && !op(74, &state));
    // Bar length is the level over the same limit.
    let graph = |kind: i32| {
        let def: SkinGraphDef =
            serde_json::from_value(serde_json::json!({"id": "g", "type": kind})).unwrap();
        graph_raw_value(&def, &state)
    };
    assert!((graph(106) - 5.0 / 12.0).abs() < 1e-6);
    assert_eq!((graph(105), graph(109)), (0.0, 1.0));
}
