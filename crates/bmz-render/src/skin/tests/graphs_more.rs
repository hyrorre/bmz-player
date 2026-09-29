use super::*;

#[test]
fn integer_graphs_normalize_integer_properties() {
    let mut graph: SkinGraphDef =
        serde_json::from_str(r#"{"id":"bar","type":74,"isRefNum":1,"min":0,"max":1200}"#).unwrap();
    for (min, max, value, expected) in [
        (0, 1200, 100, 100.0 / 1200.0),
        (0, 1200, 71, 71.0 / 1200.0),
        (0, 100, 200, 1.0),
        (20, 100, 10, 0.0),
        (100, 0, 25, 0.75),
        (100, 0, 200, 0.0),
        (0, 0, 0, 0.0),
        (i32::MIN, i32::MAX, 0, 0.5),
    ] {
        graph.min = min;
        graph.max = max;
        let state = SkinDrawState { total_notes: value, ..Default::default() };
        let (fill, uv) = graph_fill_dimensions(&graph, &state);
        assert!((fill - expected).abs() < 0.00001, "{min}..{max}: {fill}");
        assert_eq!(fill, uv);
    }
}

#[test]
fn integer_fast_slow_graphs_use_judgement_counts() {
    let state = SkinDrawState {
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            fast_great: 100,
            slow_great: 71,
            ..Default::default()
        }),
        ..Default::default()
    };
    for (reference, count) in [(423, 100.0), (424, 71.0)] {
        let graph: SkinGraphDef = serde_json::from_value(serde_json::json!({
            "id": "bar", "type": reference, "isRefNum": 1, "min": 0, "max": 1200
        }))
        .unwrap();
        let (fill, uv) = graph_fill_dimensions(&graph, &state);
        assert!((fill - count / 1200.0).abs() < 0.00001);
        assert_eq!(fill, uv);
    }
}

#[test]
fn static_render_items_resolve_gauge_in_destination_order() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [
                    { "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "gauge-node", "src": 1, "x": 10, "y": 0, "w": 5, "h": 10 }
                ],
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 4, "type": 0 },
                "destination": [
                    { "id": "panel", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "gauge", "timer": 2, "dst": [{ "x": 80, "y": 10, "w": -40, "h": 10 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )]);

    let inactive = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            elapsed_ms: 500,
            gauge: 50.0,
            gauge_max: 100.0,
            fadeout_ms: None,
            ..Default::default()
        },
    );
    let active = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            elapsed_ms: 500,
            gauge: 50.0,
            gauge_max: 100.0,
            fadeout_ms: Some(250),
            ..Default::default()
        },
    );

    assert_eq!(inactive.len(), 1);
    // beatoraja は全 `parts` 分のセルを描画する (埋まり具合でスプライトだけ変える)。
    assert_eq!(active.len(), 5);
    assert!(active[1..].iter().all(|item| matches!(item, SkinRenderItem::Image { .. })));
}

#[test]
fn best_and_target_scores_follow_note_progress() {
    let state = SkinDrawState {
        play_screen: true,
        ex_score: 450,
        total_notes: 1000,
        past_notes: 250,
        best_ex_score: Some(1800),
        target_ex_score: Some(1600),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(150, &state), Some(1800));
    assert_eq!(skin_state_number(170, &state), Some(1800));
    assert_eq!(skin_state_number(121, &state), Some(400));
    assert_eq!(skin_state_number(151, &state), Some(400));
    assert_eq!(skin_state_number(152, &state), Some(0));
    assert_eq!(skin_state_number(172, &state), Some(0));
    assert_eq!(skin_state_number(153, &state), Some(50));
}

#[test]
fn target_score_timer_and_ops_follow_current_ex_score() {
    let below = SkinDrawState {
        elapsed_ms: 1234,
        ex_score: 1599,
        total_notes: 900,
        target_ex_score: Some(1600),
        ..SkinDrawState::default()
    };
    let reached = SkinDrawState { ex_score: 1600, ..below.clone() };
    let updated = SkinDrawState { ex_score: 1601, ..below.clone() };

    assert_eq!(skin_timer_elapsed_ms(Some(352), &below), None);
    assert_eq!(skin_timer_elapsed_ms(Some(352), &reached), Some(1234));
    assert!(test_skin_op(1336, &[], &reached));
    assert!(!test_skin_op(336, &[], &reached));
    assert!(test_skin_op(336, &[], &updated));
}

#[test]
fn gauge_timers_use_state_elapsed_values() {
    let inactive = SkinDrawState::default();
    assert_eq!(skin_timer_elapsed_ms(Some(42), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(43), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(44), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(45), &inactive), None);

    let active = SkinDrawState {
        gauge_increase_ms: Some(75),
        gauge_increase_2p_ms: Some(125),
        gauge_max_ms: Some(1_700),
        gauge_max_2p_ms: Some(1_900),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_timer_elapsed_ms(Some(42), &active), Some(75));
    assert_eq!(skin_timer_elapsed_ms(Some(43), &active), Some(125));
    assert_eq!(skin_timer_elapsed_ms(Some(44), &active), Some(1_700));
    assert_eq!(skin_timer_elapsed_ms(Some(45), &active), Some(1_900));
}

#[test]
fn graph_renders_vertical_bar_proportional_to_score() {
    // BARGRAPH_SCORERATE (110): ex_score / (total_notes * 2)
    // total_notes=100, ex_score=100 → value=0.5
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "score-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 200, "type": 110 }],
                "destination": [
                    { "id": "score-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 100, "h": 480 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("bar-src", 100.0, 200.0);
    let state = SkinDrawState { ex_score: 100, total_notes: 100, ..SkinDrawState::default() };
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1, "expected one graph bar");
    let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
    // value=0.5 → height = 480/720 * 0.5; destination bottom is y=0 in beatoraja space.
    let dst_h = 480.0 / 720.0;
    assert!(approx_eq(rect.height, dst_h * 0.5), "bar height should be half: got {}", rect.height);
    assert!(
        approx_eq(rect.y, 1.0 - dst_h * 0.5),
        "bar y should start at half-height: got {}",
        rect.y
    );
    // UV should also be clipped to bottom half
    assert!(approx_eq(uv.height, 0.5), "uv height should be 0.5, got {}", uv.height);
    assert!(approx_eq(uv.y, 0.5), "uv y should be 0.5, got {}", uv.y);
}

#[test]
fn graph_renders_current_score_rate_against_past_notes() {
    // BARGRAPH_SCORERATE_FINAL (111): ex_score / (past_notes * 2)
    // total_notes=1000, past_notes=9, ex_score=18 → current rate is 100%.
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "score-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 200, "type": 111 }],
                "destination": [
                    { "id": "score-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 100, "h": 480 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("bar-src", 100.0, 200.0);
    let state = SkinDrawState {
        ex_score: 18,
        total_notes: 1000,
        past_notes: 9,
        ..SkinDrawState::default()
    };
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1, "expected one graph bar");
    let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
    let dst_h = 480.0 / 720.0;
    assert!(approx_eq(rect.height, dst_h), "bar height should be full: got {}", rect.height);
    assert!(approx_eq(rect.y, 1.0 - dst_h), "bar y should start at top: got {}", rect.y);
    assert!(approx_eq(uv.height, 1.0), "uv height should be full, got {}", uv.height);
    assert!(approx_eq(uv.y, 0.0), "uv y should start at top, got {}", uv.y);
}

#[test]
fn graph_renders_horizontal_bar_for_load_progress() {
    // BARGRAPH_LOAD_PROGRESS (102): always 1.0
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "load-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 8, "angle": 0, "type": 102 }],
                "destination": [
                    { "id": "load-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 640, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("bar-src", 100.0, 8.0);
    let state = SkinDrawState::default();
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1, "expected one load bar");
    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    // value=1.0 → full width = 640/1280 = 0.5
    assert!(approx_eq(rect.width, 640.0 / 1280.0), "full load bar width: got {}", rect.width);
}

#[test]
fn lua_graph_with_negative_width_fills_leftwards_from_destination_x() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{
                    "id": "pg_fast", "src": "bar-src", "x": 0, "y": 0, "w": 1, "h": 1, "angle": 0,
                    "value_expr": "(number(410))/(number(410)+number(411))"
                }],
                "destination": [
                    { "id": "pg_fast", "dst": [{ "time": 0, "x": 640, "y": 0, "w": -640, "h": 8 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let sources = mock_source("bar-src", 1.0, 1.0);
    let state = SkinDrawState {
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 1,
            slow_pgreat: 3,
            ..crate::snapshot::FastSlowJudgeCounts::default()
        }),
        ..SkinDrawState::default()
    };
    assert!(
        approx_eq(graph_raw_value(&document.graph[0], &state), 0.25),
        "WMII graph expression must preserve the FAST ratio"
    );
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
    assert!(approx_eq(rect.width, 0.125), "25% of half-canvas width: got rect {rect:?}, uv {uv:?}");
    assert!(
        approx_eq(rect.x, 0.375),
        "negative width must remain anchored at destination x: got {}",
        rect.x
    );
    assert!(approx_eq(uv.width, 0.25), "source UV should be clipped to 25%: got {}", uv.width);
}

#[test]
fn graph_music_progress_uses_play_progress() {
    // BARGRAPH_MUSIC_PROGRESS (101): play_progress=0.75 → bar is 75% full
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "music-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 8, "angle": 0, "type": 101 }],
                "destination": [
                    { "id": "music-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1280, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("bar-src", 100.0, 8.0);
    let state = SkinDrawState { play_progress: 0.75, ..SkinDrawState::default() };
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1, "expected one music bar");
    let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
    // value=0.75 → width = 1280/1280 * 0.75 = 0.75
    assert!(approx_eq(rect.width, 0.75), "music bar width should be 0.75, got {}", rect.width);
    assert!(approx_eq(uv.width, 0.75), "music bar uv.width should be 0.75, got {}", uv.width);
}

#[test]
fn graph_rate_pgreat_uses_judge_count_over_past_notes() {
    // BARGRAPH_RATE_PGREAT (140): pgreat / past_notes
    // pgreat=60, past_notes=100 → 0.6
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "pg-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 8, "angle": 0, "type": 140 }],
                "destination": [
                    { "id": "pg-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1000, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("bar-src", 100.0, 8.0);
    let state = SkinDrawState {
        judge_counts: DisplayJudgeCounts { pgreat: 60, great: 30, ..Default::default() },
        past_notes: 100,
        total_notes: 200,
        ..SkinDrawState::default()
    };
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    // value=0.6, dst_width = 1000/1280
    assert!(approx_eq(rect.width, 1000.0 / 1280.0 * 0.6), "pg bar width: got {}", rect.width);
}

#[test]
fn skin_value_number_evaluates_peaceful_play_gauge_values() {
    let state = SkinDrawState { gauge: 78.75, gauge_max: 120.0, ..Default::default() };
    let value = |expr: &str| SkinValueDef { value_expr: expr.to_string(), ..Default::default() };

    assert_eq!(skin_value_number(&value(SKIN_EXPR_GAUGE_PERCENT_INTEGER), &state), Some(65));
    assert_eq!(skin_value_number(&value(SKIN_EXPR_GAUGE_PERCENT_FRACTION), &state), Some(62));
    assert_eq!(skin_value_number(&value(SKIN_EXPR_GAUGE_AMOUNT_INTEGER), &state), Some(78));
    assert_eq!(skin_value_number(&value(SKIN_EXPR_GAUGE_AMOUNT_FRACTION), &state), Some(75));
}

#[test]
fn score_rate_parts_matches_beatoraja_score_data_property() {
    let (integer, afterdot) = score_rate_parts(3948, 2006);
    assert_eq!(integer, 98);
    assert_eq!(afterdot, 40);
}

#[test]
fn current_score_rate_refs_use_past_notes() {
    let state = SkinDrawState {
        ex_score: 18,
        total_notes: 1000,
        past_notes: 9,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(102, &state), Some(100));
    assert_eq!(skin_state_number(103, &state), Some(0));
    assert_eq!(skin_state_number(115, &state), Some(0));
    assert_eq!(skin_state_number(116, &state), Some(90));
}

#[test]
fn current_score_rate_starts_at_full_rate_before_first_note() {
    let state = SkinDrawState { total_notes: 1000, ..SkinDrawState::default() };

    assert_eq!(skin_state_number(102, &state), Some(100));
    assert_eq!(skin_state_number(103, &state), Some(0));
    assert!((graph_value(111, &state) - 1.0).abs() < 1e-5);
}

#[test]
fn graph_fill_dimensions_scales_lua_chart_graph_by_dst_multiplier() {
    let graph = SkinGraphDef {
        id: "default_chart_peak".to_string(),
        src: "graph".to_string(),
        value_expr: "4.800000000000001*number(360)".to_string(),
        min: 0,
        max: 320,
        x: 0,
        y: 0,
        w: 1,
        h: 14,
        divx: 1,
        divy: 1,
        timer: None,
        cycle: 0,
        angle: 0,
        graph_type: 0,
        is_ref_num: false,
    };
    let state = SkinDrawState {
        select_screen: true,
        select_chart_peak_density: 12.5,
        ..SkinDrawState::default()
    };
    let (fill, uv) = graph_fill_dimensions(&graph, &state);
    assert!((fill - 57.6).abs() < 0.01);
    assert!((uv - 57.6 / 320.0).abs() < 1e-5);
}

#[test]
fn skin_state_number_best_and_target_score() {
    let state = SkinDrawState {
        best_ex_score: Some(1500),
        target_ex_score: Some(800),
        ..SkinDrawState::default()
    };
    // NUMBER_HIGHSCORE (150)
    assert_eq!(skin_state_number(150, &state), Some(1500));
    // NUMBER_TARGET_SCORE (121)
    assert_eq!(skin_state_number(121, &state), Some(800));
    let ghost_projected = SkinDrawState {
        play_screen: true,
        best_ex_score: Some(1500),
        projected_best_ex_score: Some(321),
        ex_score: 400,
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(150, &ghost_projected), Some(1500));
    assert_eq!(skin_state_number(152, &ghost_projected), Some(79));
    // When None → None
    let no_scores = SkinDrawState::default();
    assert_eq!(skin_state_number(150, &no_scores), None);
    assert_eq!(skin_state_number(121, &no_scores), None);

    let first_play = SkinDrawState { play_screen: true, ex_score: 400, ..SkinDrawState::default() };
    assert_eq!(skin_state_number(150, &first_play), Some(0));
    assert_eq!(skin_state_number(152, &first_play), Some(400));
    assert!(test_skin_op(SKIN_OPTION_BMZ_FIRST_PLAY, &[], &first_play));

    let played_zero =
        SkinDrawState { play_screen: true, best_ex_score: Some(0), ..SkinDrawState::default() };
    assert_eq!(skin_state_number(150, &played_zero), Some(0));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_FIRST_PLAY, &[], &played_zero));

    let practice = SkinDrawState {
        play_screen: true,
        practice_mode: true,
        best_ex_score: Some(1500),
        ex_score: 400,
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(150, &practice), Some(0));
    assert_eq!(skin_state_number(152, &practice), Some(400));
    assert_eq!(graph_value(113, &practice), 0.0);
    assert!(!test_skin_op(SKIN_OPTION_BMZ_FIRST_PLAY, &[], &practice));
}

#[test]
fn graph_value_bestscorerate_fills_bar_proportionally() {
    // BARGRAPH_BESTSCORERATE (113): best / (total_notes * 2)
    // best=800, total=500 → 800/1000 = 0.8
    let state =
        SkinDrawState { best_ex_score: Some(800), total_notes: 500, ..SkinDrawState::default() };
    let v = graph_value(113, &state);
    assert!((v - 0.8).abs() < 1e-5, "best score rate: expected 0.8, got {v}");
}

#[test]
fn graph_value_targetscorerate_fills_bar_proportionally() {
    // BARGRAPH_TARGETSCORERATE (115): target / (total_notes * 2)
    // target=600, total=600 → 600/1200 = 0.5
    let state =
        SkinDrawState { target_ex_score: Some(600), total_notes: 600, ..SkinDrawState::default() };
    let v = graph_value(115, &state);
    assert!((v - 0.5).abs() < 1e-5, "target score rate: expected 0.5, got {v}");
}

#[test]
fn graph_value_bestscorerate_now_scales_with_past_notes() {
    // BARGRAPH_BESTSCORERATE_NOW (112): best * past / (total^2 * 2)
    // best=160 (80% of max 200), past=50, total=100
    // → 160 * 50 / (100^2 * 2) = 8000 / 20000 = 0.4
    // = best_rate(0.8) × play_fraction(0.5) = 0.4
    let state = SkinDrawState {
        best_ex_score: Some(160),
        past_notes: 50,
        total_notes: 100,
        ..SkinDrawState::default()
    };
    let v = graph_value(112, &state);
    assert!((v - 0.4).abs() < 1e-4, "best now rate: expected 0.4, got {v}");
}

#[test]
fn graph_value_bestscorerate_now_uses_projected_best_score() {
    let state = SkinDrawState {
        best_ex_score: Some(160),
        projected_best_ex_score: Some(100),
        past_notes: 50,
        total_notes: 100,
        ..SkinDrawState::default()
    };

    let v = graph_value(112, &state);

    assert!((v - 0.5).abs() < 1e-4, "best ghost now rate: expected 0.5, got {v}");
}

#[test]
fn graph_value_returns_zero_when_no_best_score() {
    let state = SkinDrawState { total_notes: 100, ..SkinDrawState::default() };
    assert_eq!(graph_value(113, &state), 0.0);
    assert_eq!(graph_value(115, &state), 0.0);
}
