use super::*;

#[test]
fn result_destination_negative_image_id_renders_runtime_stagefile_source() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": "-100", "op": [191], "dst": [{ "x": 0, "y": 0, "w": 40, "h": 20 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let context = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let state = SkinDrawState {
        has_stagefile: true,
        stagefile_image_size: Some(SkinImageSize { width: 400.0, height: 200.0 }),
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    let items = context.static_document_items_for_result_state_and_text(
        &Arc::new(crate::snapshot::ResultGraphSnapshot::default()),
        &state,
        &SkinTextState::default(),
    );

    assert!(items.iter().any(|item| matches!(
        item,
        SkinRenderItem::Image {
            texture,
            source_size: Some(SkinImageSize { width: 400.0, height: 200.0 }),
            ..
        } if *texture == SkinTextureId(SELECT_STAGE_TEXTURE.0)
    )));
}

#[test]
fn result_judgegraphs_render_beatoraja_judge_and_early_late_series() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "judgegraph": [
                    { "id": "judge", "type": 1, "backTexOff": 1, "noGap": 1, "noGapX": 1 },
                    { "id": "fs", "type": 2, "backTexOff": 1, "noGap": 1, "noGapX": 1 }
                ],
                "destination": [
                    { "id": "judge", "dst": [{ "x": 0, "y": 0, "w": 50, "h": 20, "a": 255 }] },
                    { "id": "fs", "dst": [{ "x": 0, "y": 20, "w": 50, "h": 20, "a": 255 }] }
                ]
            }
            "#,
    )
    .unwrap();
    document.result_judge_graph_buckets =
        vec![crate::snapshot::ResultJudgeGraphBucket { values: [0, 0, 1, 0, 0, 0] }];
    document.result_early_late_graph_buckets = vec![crate::snapshot::ResultEarlyLateGraphBucket {
        values: [0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
    }];

    let items = document.static_image_render_items(&HashMap::new(), &SkinDrawState::default());

    assert!(items.iter().any(|item| {
        skin_render_item_has_rect_color(item, |Color { r, g, b, .. }| {
            approx_eq(*r, 0.0) && approx_eq(*g, 1.0) && approx_eq(*b, 0.53)
        })
    }));
    assert!(items.iter().any(|item| {
        skin_render_item_has_rect_color(item, |Color { r, g, b, .. }| {
            approx_eq(*r, 1.0) && approx_eq(*g, 0.53) && approx_eq(*b, 0.0)
        })
    }));
}

#[test]
fn result_notes_graph_renders_note_distribution_colors() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "judgegraph": [
                    { "id": "note_graph", "type": 0, "backTexOff": 1, "noGap": 1, "noGapX": 1 }
                ],
                "destination": [
                    { "id": "note_graph", "timer": 0, "dst": [{ "x": 0, "y": 0, "w": 50, "h": 20, "a": 255 }] }
                ]
            }
            "#,
    )
    .unwrap();
    document.result_note_graph_buckets =
        vec![crate::snapshot::ResultNoteGraphBucket { values: [1, 1, 1, 1, 1, 1, 1] }];

    let items = document.static_image_render_items(&HashMap::new(), &SkinDrawState::default());
    let colors = [
        (0.27, 1.0, 0.27),
        (0.13, 0.53, 0.13),
        (1.0, 0.27, 0.27),
        (0.27, 0.27, 1.0),
        (0.13, 0.13, 0.53),
        (0.80, 0.80, 0.80),
        (0.53, 0.0, 0.0),
    ];
    for (r, g, b) in colors {
        assert!(items.iter().any(|item| {
            skin_render_item_has_rect_color(
                item,
                |Color { r: actual_r, g: actual_g, b: actual_b, .. }| {
                    approx_eq(*actual_r, r) && approx_eq(*actual_g, g) && approx_eq(*actual_b, b)
                },
            )
        }));
    }
}

#[test]
fn result_click_hit_uses_runtime_panel_visibility() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "image": [
                    { "id": "graph", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "act": -10002 },
                    { "id": "ir", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "act": -10001 },
                    { "id": "favorite", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "divy": 3, "ref": 90, "act": 90 }
                ],
                "destination": [
                    { "id": "graph", "draw": "result_panel(1)", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 10 }] },
                    { "id": "ir", "draw": "result_panel(2)", "dst": [{ "x": 50, "y": 20, "w": 30, "h": 10 }] },
                    { "id": "favorite", "dst": [{ "x": 10, "y": 40, "w": 30, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let ir_panel = SkinDrawState { result_panel: Some(1), ..SkinDrawState::default() };
    let graph_hit = document.result_click_hit(&ir_panel, 0.2, 0.75).unwrap();
    assert_eq!(
        graph_hit.target,
        SkinClickTarget::Event { event_id: SKIN_EVENT_RESULT_PANEL_GRAPH, click: 0 }
    );
    assert!(document.result_click_hit(&ir_panel, 0.65, 0.75).is_none());

    let graph_panel = SkinDrawState {
        result_panel: Some(2),
        result_favorite_chart: Some(false),
        ..SkinDrawState::default()
    };
    let ir_hit = document.result_click_hit(&graph_panel, 0.65, 0.75).unwrap();
    assert_eq!(
        ir_hit.target,
        SkinClickTarget::Event { event_id: SKIN_EVENT_RESULT_PANEL_IR, click: 0 }
    );
    assert!(document.result_click_hit(&graph_panel, 0.2, 0.75).is_none());

    let favorite_hit = document.result_click_hit(&graph_panel, 0.2, 0.55).unwrap();
    assert_eq!(favorite_hit.target, SkinClickTarget::Event { event_id: 90, click: 0 });
}

#[test]
fn result_mouse_rect_requires_an_available_pointer() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "image": [
                    { "id": "button", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "act": 15 }
                ],
                "destination": [
                    {
                        "id": "button",
                        "dst": [{ "x": 10, "y": 20, "w": 30, "h": 10 }],
                        "mouseRect": { "x": 5, "y": 2, "w": 10, "h": 4 }
                    }
                ]
            }
        "#,
    )
    .unwrap();

    assert!(document.result_click_hit(&SkinDrawState::default(), 0.2, 0.75).is_none());

    let hovered =
        SkinDrawState { mouse_x: Some(20.0), mouse_y: Some(24.0), ..SkinDrawState::default() };
    assert!(document.result_click_hit(&hovered, 0.2, 0.75).is_some());
}

#[test]
fn result_ir_slider_hit_and_rate_use_ranking_scroll_position() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "slider": [
                    { "id": "ir-scroll", "src": 1, "x": 0, "y": 0, "w": 10, "h": 5, "angle": 2, "range": 50, "type": 8, "changeable": true }
                ],
                "destination": [
                    { "id": "ir-scroll", "draw": "result_panel(1)", "dst": [{ "x": 10, "y": 70, "w": 10, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let state = SkinDrawState {
        result_panel: Some(1),
        ir_ranking: crate::scene::ResultIrSnapshot {
            scroll_offset: 5,
            scroll_max: 10,
            ..Default::default()
        },
        ..Default::default()
    };

    assert!(approx_eq(skin_state_float_number(8, &state).unwrap(), 0.5));
    assert!(approx_eq(skin_slider_progress_by_type(8, &state).unwrap(), 0.5));
    // angle=2 destination y=70 range=50 → value 0.5 at skin y=45 (norm y=0.55)
    let hit = document.result_slider_hit(&state, 0.15, 0.55).unwrap();
    assert_eq!(hit.slider_type, 8);
    assert!(approx_eq(hit.value, 0.5));
}

#[test]
fn skin_state_number_maps_player_statistics_refs() {
    let state = SkinDrawState {
        total_notes: 99,
        select_total_notes: 100,
        select_screen: true,
        select_play_count: 42,
        select_clear_count: 31,
        player_stats: PlayerStatsSnapshot {
            session_play_count: 0,
            session_play_notes: 0,
            play_count: 10,
            clear_count: 7,
            playtime_seconds: 3_661,
            max_combo: 999,
            fast_pgreat: 2,
            slow_pgreat: 3,
            fast_great: 4,
            slow_great: 5,
            fast_good: 6,
            slow_good: 7,
            fast_bad: 8,
            slow_bad: 9,
            fast_poor: 10,
            slow_poor: 11,
            fast_empty_poor: 12,
            slow_empty_poor: 13,
            daily: Default::default(),
        },
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(17, &state), Some(1));
    assert_eq!(skin_state_number(18, &state), Some(1));
    assert_eq!(skin_state_number(19, &state), Some(1));
    assert_eq!(skin_state_number(30, &state), Some(10));
    assert_eq!(skin_state_number(31, &state), Some(7));
    assert_eq!(skin_state_number(32, &state), Some(3));
    assert_eq!(skin_state_number(33, &state), Some(5));
    assert_eq!(skin_state_number(34, &state), Some(9));
    assert_eq!(skin_state_number(35, &state), Some(13));
    assert_eq!(skin_state_number(36, &state), Some(17));
    assert_eq!(skin_state_number(37, &state), Some(21));
    assert_eq!(skin_state_number(333, &state), Some(44));
    assert_eq!(skin_state_number(77, &state), Some(42));
    assert_eq!(skin_state_number(78, &state), Some(31));
}

#[test]
fn compatible_daily_statistics_use_skin_specific_note_definitions() {
    let daily = DailyPlayerStatsSnapshot {
        play_count: 5,
        clear_count: 4,
        pgreat: 50,
        great: 25,
        good: 10,
        bad: 5,
        poor: 3,
        empty_poor: 2,
        score_update_count: 3,
        clear_update_count: 2,
        miss_count_update_count: 1,
        recent_titles: std::array::from_fn(|index| format!("Recent {}", index + 1)),
    };
    let state = SkinDrawState {
        player_stats: PlayerStatsSnapshot { daily, ..PlayerStatsSnapshot::default() },
        ..SkinDrawState::default()
    };

    let million_value = SkinValueDef {
        id: "Number_Todayplayednotes".to_string(),
        value_expr: "1".to_string(),
        ..SkinValueDef::default()
    };
    assert_eq!(skin_value_number(&million_value, &state), Some(95));
    assert_eq!(skin_state_number(1930, &state), Some(5));
    assert_eq!(skin_state_number(1938, &state), Some(90));
    assert_eq!(skin_state_number(1939, &state), Some(95));
    assert_eq!(skin_state_number(1940, &state), Some(125));
    assert_eq!(skin_state_number(1941, &state), Some(180));
    assert_eq!(skin_state_number(1942, &state), Some(6944));
    assert_eq!(skin_state_number(1943, &state), Some(2));
    assert_eq!(skin_state_number(1944, &state), Some(3));
    assert_eq!(skin_state_number(1945, &state), Some(2));
    assert_eq!(skin_state_number(1946, &state), Some(1));

    let text = |id: &str| SkinTextDef { id: id.to_string(), ..SkinTextDef::default() };
    let text_state = SkinTextState::default();
    assert_eq!(
        skin_state_text_with_draw_state(
            &text("defaultNotesProcessingCounter_notes"),
            Some(&state),
            &text_state,
        ),
        "90"
    );
    assert_eq!(
        skin_state_text_with_draw_state(
            &text("defaultNotesProcessingCounter_pg"),
            Some(&state),
            &text_state,
        ),
        "50  (55.56%)"
    );
    assert_eq!(
        skin_state_text_with_draw_state(
            &text("defaultNotesProcessingCounter_cp"),
            Some(&state),
            &text_state,
        ),
        "4/5"
    );
    assert_eq!(
        skin_state_text_with_draw_state(
            &text("defaultNotesProcessingCounter_rank"),
            Some(&state),
            &text_state,
        ),
        "A"
    );
    assert_eq!(
        skin_state_text_with_draw_state(
            &text("defaultNotesProcessingCounter_rate"),
            Some(&state),
            &text_state,
        ),
        "69.44"
    );
    let generic_rank = SkinTextDef { ref_id: 1943, ..Default::default() };
    let generic_recent = SkinTextDef { ref_id: 1950, ..Default::default() };
    assert_eq!(skin_state_text_with_draw_state(&generic_rank, Some(&state), &text_state), "A");
    assert_eq!(
        skin_state_text_with_draw_state(&generic_recent, Some(&state), &text_state),
        "Recent 1"
    );
}

#[test]
fn course_stage_result_refs_map_fixed_slots() {
    let mut course_result = CourseResultSkinSnapshot { stage_count: 2, ..Default::default() };
    course_result.stages[1] = crate::scene::CourseStageResultSkinSnapshot {
        ex_score: 200,
        gauge: 42.9,
        bp: 17,
        rate_basis_points: 8_333,
    };
    let state = SkinDrawState { course_result, ..Default::default() };

    assert_eq!(skin_state_number(SKIN_REF_BMZ_COURSE_STAGE_COUNT, &state), Some(2));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_COURSE_STAGE_EX_BASE + 1, &state), Some(200));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE + 1, &state), Some(42));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_COURSE_STAGE_BP_BASE + 1, &state), Some(17));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE + 1, &state), Some(8_333));
}

#[test]
fn skin_state_number_maps_result_value_refs() {
    let fast_slow = crate::snapshot::FastSlowJudgeCounts {
        fast_pgreat: 350,
        slow_pgreat: 427,
        fast_great: 180,
        slow_great: 154,
        fast_good: 12,
        slow_good: 10,
        fast_bad: 2,
        slow_bad: 1,
        fast_poor: 3,
        slow_poor: 2,
        fast_empty_poor: 5,
        slow_empty_poor: 4,
    };
    let state = SkinDrawState {
        ex_score: 1888,
        max_combo: 777,
        total_notes: 1000,
        past_notes: 1000,
        judge_counts: DisplayJudgeCounts {
            pgreat: 777,
            great: 334,
            good: 22,
            bad: 3,
            poor: 5,
            empty_poor: 9,
        },
        fast_slow_counts: Some(fast_slow),
        best_ex_score: Some(1700),
        best_clear_index: Some(6),
        target_ex_score: Some(1900),
        best_max_combo: Some(800),
        target_max_combo: Some(1000),
        best_bp: Some(20),
        previous_best_ex_score: Some(1800),
        previous_best_clear_index: Some(4),
        previous_best_max_combo: Some(700),
        previous_best_bp: Some(10),
        target_bp: Some(0),
        target_clear_index: Some(8),
        select_clear_index: 5,
        result_failed: Some(false),
        result_arrange_index: 9,
        result_arrange_2p_index: 1,
        average_timing_ms: Some(-12.34),
        average_duration_us: Some(345_670),
        stddev_timing_ms: Some(56.78),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(42, &state), Some(9));
    assert_eq!(skin_state_number(43, &state), Some(1));
    // 符号付き差分
    assert_eq!(skin_state_number(170, &state), Some(1800));
    assert_eq!(skin_state_number(121, &state), Some(1900));
    assert_eq!(skin_state_number(151, &state), Some(1900));
    assert_eq!(skin_state_number(122, &state), Some(95));
    assert_eq!(skin_state_number(123, &state), Some(0));
    assert_eq!(skin_state_number(135, &state), Some(95));
    assert_eq!(skin_state_number(136, &state), Some(0));
    assert_eq!(skin_state_number(157, &state), Some(95));
    assert_eq!(skin_state_number(158, &state), Some(0));
    assert_eq!(skin_state_number(183, &state), Some(90));
    assert_eq!(skin_state_number(184, &state), Some(0));
    assert_eq!(skin_state_number(152, &state), Some(1888 - 1800));
    assert_eq!(skin_state_number(172, &state), Some(1888 - 1800));
    assert_eq!(skin_state_number(153, &state), Some(1888 - 1900));
    assert_eq!(skin_state_number(173, &state), Some(700));
    assert_eq!(skin_state_number(175, &state), Some(777 - 700));
    assert_eq!(skin_state_number(176, &state), Some(10));
    assert_eq!(skin_state_number(177, &state), Some(8));
    // 現在 bp = bad+poor = 8、MYBEST = 更新前の 10 → diff = -2
    assert_eq!(skin_state_number(178, &state), Some(-2));
    assert_eq!(skin_state_number(370, &state), Some(5));
    assert_eq!(skin_state_number(371, &state), Some(4));
    assert_eq!(skin_state_number(372, &state), Some(345));
    assert_eq!(skin_state_number(373, &state), Some(67));
    assert_eq!(skin_state_number(374, &state), Some(-12));
    assert_eq!(skin_state_number(375, &state), Some(-34));
    assert_eq!(skin_image_index_number(370, &state), Some(5));
    assert_eq!(skin_image_index_number(371, &state), Some(4));
    assert!(test_skin_op(320, &[], &state));
    assert!(!test_skin_op(321, &[], &state));
    assert!(test_skin_op(330, &[], &state));
    assert!(!test_skin_op(1330, &[], &state));
    assert!(test_skin_op(331, &[], &state));
    assert!(!test_skin_op(1331, &[], &state));
    assert!(test_skin_op(332, &[], &state));
    assert!(!test_skin_op(1332, &[], &state));
    assert!(test_skin_op(335, &[], &state));
    assert!(!test_skin_op(1335, &[], &state));
    assert!(test_skin_op(300, &[], &state));
    assert!(test_skin_op(310, &[], &state));
    assert!(!test_skin_op(301, &[], &state));
    assert!(!test_skin_op(308, &[], &state));
    assert!(test_skin_op(350, &[], &state));
    assert!(!test_skin_op(351, &[], &state));
    assert!(!test_skin_op(352, &[], &state));
    assert!(test_skin_op(353, &[], &state));
    assert!(!test_skin_op(354, &[], &state));

    let draw_state = SkinDrawState {
        ex_score: 1800,
        max_combo: 700,
        total_notes: 1000,
        judge_counts: DisplayJudgeCounts { bad: 5, poor: 5, ..DisplayJudgeCounts::default() },
        previous_best_ex_score: Some(1800),
        previous_best_max_combo: Some(700),
        previous_best_bp: Some(10),
        target_ex_score: Some(1800),
        result_failed: Some(false),
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(1330, &[], &draw_state));
    assert!(test_skin_op(1331, &[], &draw_state));
    assert!(test_skin_op(1332, &[], &draw_state));
    assert!(test_skin_op(1335, &[], &draw_state));
    assert!(test_skin_op(354, &[], &draw_state));

    let failed_record_bp_state = SkinDrawState {
        judge_counts: DisplayJudgeCounts { bad: 1, poor: 2, ..DisplayJudgeCounts::default() },
        previous_best_bp: Some(10),
        result_bp: Some(100),
        result_cb: Some(80),
        result_failed: Some(true),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(76, &failed_record_bp_state), Some(100));
    assert_eq!(skin_state_number(177, &failed_record_bp_state), Some(100));
    assert_eq!(skin_state_number(178, &failed_record_bp_state), Some(90));
    assert_eq!(skin_state_number(425, &failed_record_bp_state), Some(80));
    assert_eq!(skin_state_number(427, &failed_record_bp_state), Some(80));
    assert!(!test_skin_op(332, &[], &failed_record_bp_state));
    assert!(!test_skin_op(1332, &[], &failed_record_bp_state));

    let updated_result_state = SkinDrawState {
        ex_score: 1900,
        total_notes: 1000,
        past_notes: 1000,
        best_ex_score: Some(1900),
        previous_best_ex_score: Some(1700),
        result_failed: Some(false),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(150, &updated_result_state), Some(1700));
    assert_eq!(skin_state_number(170, &updated_result_state), Some(1700));
    assert_eq!(skin_state_number(152, &updated_result_state), Some(200));
    assert_eq!(skin_state_number(183, &updated_result_state), Some(85));
    assert!(test_skin_op(321, &[], &updated_result_state));
    assert!(!test_skin_op(320, &[], &updated_result_state));
    assert!((graph_value(113, &updated_result_state) - 0.85).abs() < 1e-5);

    let first_play_result_state = SkinDrawState {
        ex_score: 1888,
        max_combo: 777,
        total_notes: 1000,
        past_notes: 1000,
        judge_counts: DisplayJudgeCounts { bad: 3, poor: 5, ..DisplayJudgeCounts::default() },
        best_ex_score: Some(1888),
        best_clear_index: Some(6),
        best_bp: Some(8),
        previous_best_ex_score: None,
        previous_best_clear_index: None,
        previous_best_bp: None,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(150, &first_play_result_state), Some(0));
    assert_eq!(skin_state_number(170, &first_play_result_state), Some(0));
    assert_eq!(skin_state_number(152, &first_play_result_state), Some(1888));
    assert_eq!(skin_state_number(176, &first_play_result_state), None);
    assert_eq!(skin_state_number(178, &first_play_result_state), None);
    assert_eq!(lua_main_state_number(176, &first_play_result_state), i64::from(i32::MIN));
    assert_eq!(lua_main_state_number(178, &first_play_result_state), i64::from(i32::MIN));
    assert!(eval_skin_draw_condition("number(176)<0", &first_play_result_state));
    assert!(!test_skin_op(332, &[], &first_play_result_state));
    assert!(!test_skin_op(1332, &[], &first_play_result_state));
    assert_eq!(skin_state_number(183, &first_play_result_state), Some(0));
    assert_eq!(skin_state_number(184, &first_play_result_state), Some(0));
    assert_eq!(skin_state_number(371, &first_play_result_state), Some(0));
    assert_eq!(graph_value(113, &first_play_result_state), 0.0);
    assert!(!test_skin_op(320, &[], &first_play_result_state));
    assert!(test_skin_op(327, &[], &first_play_result_state));
    assert!(test_skin_op(SKIN_OPTION_BMZ_FIRST_PLAY, &[], &first_play_result_state));

    let zero_rank_state = SkinDrawState {
        ex_score: 0,
        total_notes: 1000,
        result_failed: Some(true),
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(308, &[], &zero_rank_state));
    assert!(test_skin_op(318, &[], &zero_rank_state));

    // Fast/Slow 内訳
    assert_eq!(skin_state_number(410, &state), Some(350));
    assert_eq!(skin_state_number(411, &state), Some(427));
    assert_eq!(skin_state_number(412, &state), Some(180));
    assert_eq!(skin_state_number(413, &state), Some(154));
    assert_eq!(skin_state_number(414, &state), Some(12));
    assert_eq!(skin_state_number(415, &state), Some(10));
    assert_eq!(skin_state_number(416, &state), Some(2));
    assert_eq!(skin_state_number(417, &state), Some(1));
    assert_eq!(skin_state_number(418, &state), Some(3));
    assert_eq!(skin_state_number(419, &state), Some(2));
    assert_eq!(skin_state_number(421, &state), Some(5));
    assert_eq!(skin_state_number(422, &state), Some(4));
    // TOTAL_EARLY = fast 合計 (PGREAT 除外) = 180+12+2+3+5 = 202
    assert_eq!(skin_state_number(423, &state), Some(202));
    // TOTAL_LATE = slow 合計 (PGREAT 除外) = 154+10+1+2+4 = 171
    assert_eq!(skin_state_number(424, &state), Some(171));

    // Result timing distribution
    assert_eq!(skin_state_number(374, &state), Some(-12));
    assert_eq!(skin_state_number(375, &state), Some(-34));
    assert_eq!(skin_state_number(376, &state), Some(56));
    assert_eq!(skin_state_number(377, &state), Some(78));

    // best/target が None のとき None を返す
    let bare = SkinDrawState::default();
    assert_eq!(skin_state_number(152, &bare), None);
    assert_eq!(skin_state_number(173, &bare), None);
    assert_eq!(skin_state_number(410, &bare), None);
    assert_eq!(skin_state_number(374, &bare), None);
}
