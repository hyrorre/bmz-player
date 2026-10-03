use super::*;

#[test]
fn sudden_slider_draws_above_disappear_line() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "cover.png" }],
                "slider": [
                    { "id": "lanecover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "angle": 2, "range": 723, "type": 4 }
                ],
                "hiddenCover": [
                    { "id": "hiddencover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "lanecover", "dst": [{ "x": 20, "y": 1080, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "12".to_string(),
        SkinDocumentTexture {
            source_id: "12".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 431.0, height: 723.0 },
        },
    )]);

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { lane_cover: 1.0, ..SkinDrawState::default() },
    );
    let SkinRenderItem::Image { rect, uv, .. } = &items[0] else {
        panic!("expected sudden+ lane cover image");
    };
    assert!(approx_eq(rect.height, 723.0 / 720.0));
    assert!(approx_eq(uv.height, 1.0));
}

#[test]
fn skin_state_number_maps_operating_time_refs() {
    let state = SkinDrawState { operating_time_ms: 90_061_234, ..SkinDrawState::default() };

    assert_eq!(skin_state_number(27, &state), Some(25));
    assert_eq!(skin_state_number(28, &state), Some(1));
    assert_eq!(skin_state_number(29, &state), Some(1));
}

#[test]
fn result_operating_time_refs_follow_application_time() {
    let crate::scene::AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene()
    else {
        panic!("expected result snapshot");
    };
    // Result entry resets scene time, but uptime must continue across scenes.
    snapshot.elapsed_time = bmz_core::time::TimeUs(0);
    for (operating_time_ms, expected) in
        [(3_599_999, [0, 59, 59]), (3_600_000, [1, 0, 0]), (90_061_234, [25, 1, 1])]
    {
        snapshot.operating_time_ms = operating_time_ms;
        let state = crate::plan::result_skin_draw_state(&snapshot, 0);
        for (id, value) in [27, 28, 29].into_iter().zip(expected) {
            assert_eq!(skin_state_number(id, &state), Some(value));
        }
    }
}

#[test]
fn skin_state_number_maps_beatoraja_point_score() {
    let state = SkinDrawState {
        key_mode: KeyMode::K7,
        max_combo: 45,
        total_notes: 100,
        judge_counts: DisplayJudgeCounts {
            pgreat: 30,
            great: 20,
            good: 10,
            bad: 4,
            poor: 3,
            empty_poor: 2,
        },
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(100, &state), Some(89_500));

    let five_key = SkinDrawState { key_mode: KeyMode::K5, ..state };
    assert_eq!(skin_state_number(100, &five_key), Some(55_000));
}

#[test]
fn skin_state_maps_level_failcount_and_float_properties() {
    let select = SkinDrawState {
        select_screen: true,
        select_play_level: 12,
        difficulty: 4,
        select_ex_score: Some(0),
        select_play_count: 9,
        select_clear_count: 4,
        ..SkinDrawState::default()
    };
    for ref_id in 45..=49 {
        assert_eq!(skin_state_number(ref_id, &select), Some(12));
    }
    assert_eq!(skin_state_number(79, &select), Some(5));
    assert!(approx_eq(skin_state_float_number(103, &select).unwrap(), 1.2));
    assert_eq!(skin_state_float_number(105, &select), Some(0.0));
    assert!(approx_eq(skin_state_float_number(108, &select).unwrap(), 1.2));
    assert_eq!(skin_state_float_number(109, &select), Some(0.0));

    let folder = SkinDrawState {
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        ..select.clone()
    };
    assert_eq!(skin_state_number(45, &folder), None);
    assert_eq!(skin_state_number(79, &folder), None);

    let state = SkinDrawState {
        current_fps: 237,
        play_timer_ms: Some(125_000),
        ex_score: 80,
        total_notes: 100,
        past_notes: 50,
        judge_counts: DisplayJudgeCounts {
            pgreat: 20,
            great: 15,
            good: 10,
            bad: 4,
            poor: 1,
            ..DisplayJudgeCounts::default()
        },
        best_ex_score: Some(120),
        target_ex_score: Some(150),
        hispeed: 1.75,
        gauge: 42.5,
        skin_loaded: false,
        resource_load_progress: 0.426,
        average_duration_us: Some(12_345),
        average_timing_ms: Some(-1.25),
        stddev_timing_ms: Some(4.5),
        select_chart_density: 8.25,
        select_chart_peak_density: 12.5,
        select_chart_end_density: 3.75,
        select_chart_total_gauge: 350.0,
        ..SkinDrawState::default()
    };
    assert!(approx_eq(skin_state_float_number(111, &state).unwrap(), 0.8));
    assert!(approx_eq(skin_state_float_number(113, &state).unwrap(), 0.6));
    assert_eq!(skin_state_float_number(101, &state), Some(0.0));
    assert!(approx_eq(skin_state_float_number(102, &state).unwrap(), 0.426));
    assert_eq!(skin_state_float_number(103, &state), Some(0.0));
    assert_eq!(skin_state_float_number(140, &state), Some(0.0));
    assert_eq!(skin_state_float_number(146, &state), None);
    assert_eq!(skin_state_float_number(1102, &state), None);
    assert_eq!(skin_state_float_number(372, &state), None);
    assert_eq!(skin_state_float_number(9_999, &state), None);
    assert_eq!(skin_state_number(161, &state), Some(2));
    assert_eq!(skin_state_number(162, &state), Some(5));
    assert_eq!(skin_state_number(20, &state), Some(237));
    assert_eq!(skin_state_number(368, &state), Some(350));
    assert_eq!(skin_state_number(165, &state), Some(42));
}

#[test]
fn skin_value_evaluates_default_chart_total_count_expr() {
    let state = SkinDrawState {
        select_screen: true,
        select_total_notes: 2_000,
        select_chart_total_gauge: 500.0,
        ..SkinDrawState::default()
    };
    let value = SkinValueDef {
        value_expr: SKIN_EXPR_DEFAULT_CHART_TOTAL_COUNT.to_string(),
        ..SkinValueDef::default()
    };
    let expected = 7.605_f32 * 2_000.0 / (0.01 * 2_000.0 + 6.5) - 500.0;
    assert!(
        (skin_value_number(&value, &state).unwrap() as f32 - expected).abs() < 0.5,
        "expected ~{expected}, got {:?}",
        skin_value_number(&value, &state)
    );
}

#[test]
fn skin_image_act_uses_event_index_for_button_frame_row() {
    let image = SkinImageDef {
        lr2_panel: None,
        id: "auto-judge".to_string(),
        src: "1".to_string(),
        x: 0,
        y: 0,
        w: 68,
        h: 99,
        divx: 1,
        divy: 3,
        timer: None,
        cycle: 0,
        len: 0,
        ref_id: 0,
        click: 0,
        act: Some(75),
        clickable: None,
    };
    let source_size = SkinImageSize { width: 68.0, height: 99.0 };
    let off = skin_image_texture_region_for_state(
        &image,
        source_size,
        &SkinDrawState::default(),
        (0, 0, 68, 99),
    );
    let on = skin_image_texture_region_for_state(
        &image,
        source_size,
        &SkinDrawState { judge_timing_auto_adjust: true, ..SkinDrawState::default() },
        (0, 0, 68, 99),
    );

    assert!(approx_eq(off.y, 0.0));
    assert!(approx_eq(on.y, 1.0 / 3.0));
    assert!(approx_eq(on.height, 1.0 / 3.0));
}

#[test]
fn image_cycle_uses_its_own_clock_independently_of_destination_timer() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": "src", "path": "flash.png" }],
                "image": [
                    {
                        "id": "scene-cycle", "src": "src",
                        "x": 0, "y": 0, "w": 400, "h": 100,
                        "divx": 4, "cycle": 100
                    },
                    {
                        "id": "timer-cycle", "src": "src",
                        "x": 0, "y": 0, "w": 400, "h": 100,
                        "divx": 4, "cycle": 100, "timer": 11
                    }
                ],
                "destination": [
                    {
                        "id": "scene-cycle", "timer": 11,
                        "dst": [
                            { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                            { "time": 100, "x": 100, "y": 0, "w": 10, "h": 10 }
                        ]
                    },
                    {
                        "id": "timer-cycle",
                        "dst": [{ "time": 0, "x": 0, "y": 20, "w": 10, "h": 10 }]
                    }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = mock_source("src", 400.0, 100.0);
    let state =
        SkinDrawState { elapsed_ms: 75, select_bar_elapsed_ms: 25, ..SkinDrawState::default() };

    let items = document.static_image_render_items(&sources, &state);

    let SkinRenderItem::Image { rect: scene_rect, uv: scene_uv, .. } = &items[0] else {
        panic!("expected scene-cycle image");
    };
    let SkinRenderItem::Image { uv: timer_uv, .. } = &items[1] else {
        panic!("expected timer-cycle image");
    };
    assert!(approx_eq(scene_rect.x, 0.25));
    assert!(approx_eq(scene_uv.x, 0.75));
    assert!(approx_eq(timer_uv.x, 0.25));
}

#[test]
fn filter_nonzero_destination_returns_linear_filter_item() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "filter": 1, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(3),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 1);
    assert!(matches!(items[0], SkinRenderItem::Image { linear_filter: true, .. }));
}

#[test]
fn destination_angle_and_center_emit_rotated_image() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "center": 1, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 30, "h": 40, "angle": 90 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();

    let sources = mock_source("src", 10.0, 10.0);
    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 1);
    assert!(matches!(
        items[0],
        SkinRenderItem::RotatedImage { angle_deg, center, .. }
            if approx_eq(angle_deg, -90.0) && approx_eq(center.x, 0.0) && approx_eq(center.y, 1.0)
    ));
}

#[test]
fn negative_static_image_width_matches_beatoraja_horizontal_mirroring() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 1920, "h": 1080,
                "source": [{ "id": "frame-src", "path": "frame.png" }],
                "image": [{
                    "id": "table-level-frame", "src": "frame-src",
                    "x": 0, "y": 0, "w": 101, "h": 53
                }],
                "destination": [{
                    "id": "table-level-frame",
                    "dst": [{ "x": 1193, "y": 100, "w": -101, "h": 53 }]
                }]
            }
            "#,
    )
    .unwrap();

    let sources = mock_source("frame-src", 101.0, 53.0);
    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
    assert!(approx_eq(rect.x, (1193.0 - 101.0) / 1920.0));
    assert!(approx_eq(rect.width, 101.0 / 1920.0));
    assert!(approx_eq(uv.x, 1.0));
    assert!(approx_eq(uv.width, -1.0));
}

#[test]
fn value_number_right_aligns_by_default() {
    // 3-digit number "42" in a 5-digit area (align=0, default right-aligned)
    // shiftbase=3 → first digit at position 3, second at 4
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "val", "src": "src", "x": 0, "y": 0, "w": 100, "h": 20, "divx": 10, "digit": 5, "ref": 104 }],
                "destination": [
                    { "id": "val", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 20, "h": 20 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("src", 100.0, 20.0);
    // combo=42, total_notes=100 → ref 104 = combo = 42 → 2 digits
    let state =
        SkinDrawState { elapsed_ms: 0, combo: 42, total_notes: 100, ..SkinDrawState::default() };
    let items = document.static_image_render_items(&sources, &state);

    // 2 digits in a 5-digit space, right-aligned: shiftbase=3
    // digit_width = 20/1280, digit_step = digit_width (space=0)
    // digit 0 ("4"): x = 0 + step * (3 + 0) - 0 = 3 * step
    // digit 1 ("2"): x = 0 + step * (3 + 1) - 0 = 4 * step
    assert_eq!(items.len(), 2);
    let digit_width = 20.0 / 1280.0;
    let SkinRenderItem::Image { rect: r0, .. } = &items[0] else { panic!() };
    let SkinRenderItem::Image { rect: r1, .. } = &items[1] else { panic!() };
    assert!(
        approx_eq(r0.x, 3.0 * digit_width),
        "first digit x={} expected {}",
        r0.x,
        3.0 * digit_width
    );
    assert!(
        approx_eq(r1.x, 4.0 * digit_width),
        "second digit x={} expected {}",
        r1.x,
        4.0 * digit_width
    );
}

#[test]
fn volume_number_uses_blank_padding_and_digit_cell_width() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1920, "h": 1080,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "volume", "src": "src", "x": 2401, "y": 510, "w": 242, "h": 15, "divx": 11, "digit": 3, "ref": 57 }],
                "destination": [
                    { "id": "volume", "dst": [{ "time": 0, "x": 1717, "y": 360, "w": 22, "h": 15 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("src", 3200.0, 3200.0);
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { select_master_volume: 0.37, ..SkinDrawState::default() },
    );

    assert_eq!(items.len(), 3);
    let SkinRenderItem::Image { rect: r0, uv: uv0, .. } = &items[0] else { panic!() };
    let SkinRenderItem::Image { rect: r1, uv: uv1, .. } = &items[1] else { panic!() };
    let SkinRenderItem::Image { rect: r2, uv: uv2, .. } = &items[2] else { panic!() };
    let digit_width = 22.0 / 1920.0;
    assert!(approx_eq(r0.width, digit_width));
    assert!(approx_eq(r1.width, digit_width));
    assert!(approx_eq(r2.width, digit_width));
    assert!(approx_eq(r1.x - r0.x, digit_width));
    assert!(approx_eq(r2.x - r1.x, digit_width));
    assert!(approx_eq(uv0.width, 22.0 / 3200.0));
    assert!(approx_eq(uv1.width, 22.0 / 3200.0));
    assert!(approx_eq(uv2.width, 22.0 / 3200.0));
    assert!(approx_eq(uv0.x, (2401.0 + 10.0 * 22.0) / 3200.0));
    assert!(approx_eq(uv1.x, (2401.0 + 3.0 * 22.0) / 3200.0));
    assert!(approx_eq(uv2.x, (2401.0 + 7.0 * 22.0) / 3200.0));
    assert!(
        approx_eq(uv0.width, 242.0 / 11.0 / 3200.0),
        "value sprite must be sliced into 11 cells, got uv.width={}",
        uv0.width
    );
}

#[test]
fn value_number_slices_source_with_beatoraja_integer_division() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "volume", "src": "src", "x": 3114, "y": 0, "w": 99, "h": 12, "divx": 10, "digit": 3, "ref": 57, "align": 2 }],
                "destination": [
                    { "id": "volume", "dst": [{ "time": 0, "x": 560, "y": 480, "w": 12, "h": 12 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let source_width = 3224.0;
    let sources = mock_source("src", source_width, 1024.0);
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { select_master_volume: 0.37, ..SkinDrawState::default() },
    );

    assert_eq!(items.len(), 2);
    let SkinRenderItem::Image { uv: uv0, .. } = &items[0] else { panic!() };
    let SkinRenderItem::Image { uv: uv1, .. } = &items[1] else { panic!() };
    assert!(
        approx_eq(uv0.width, 9.0 / source_width),
        "beatoraja slices 99px / 10 as 9px cells, got {}",
        uv0.width * source_width
    );
    assert!(approx_eq(uv0.x, (3114.0 + 3.0 * 9.0) / source_width));
    assert!(approx_eq(uv1.x, (3114.0 + 7.0 * 9.0) / source_width));
}

#[test]
fn value_number_left_aligns_when_align_1() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "val", "src": "src", "x": 0, "y": 0, "w": 100, "h": 20, "divx": 10, "digit": 5, "align": 1, "ref": 104 }],
                "destination": [
                    { "id": "val", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 20, "h": 20 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("src", 100.0, 20.0);
    let state =
        SkinDrawState { elapsed_ms: 0, combo: 42, total_notes: 100, ..SkinDrawState::default() };
    let items = document.static_image_render_items(&sources, &state);

    // left-aligned: shift = 3 * step, digit 0 at 0, digit 1 at step
    assert_eq!(items.len(), 2);
    let digit_width = 20.0 / 1280.0;
    let SkinRenderItem::Image { rect: r0, .. } = &items[0] else { panic!() };
    let SkinRenderItem::Image { rect: r1, .. } = &items[1] else { panic!() };
    assert!(approx_eq(r0.x, 0.0), "first digit x={} expected 0", r0.x);
    assert!(approx_eq(r1.x, digit_width), "second digit x={} expected {}", r1.x, digit_width);
}

#[test]
fn skin_state_number_hispeed_and_timeleft() {
    let state = SkinDrawState { hispeed: 1.5, timeleft_ms: 90_500, ..SkinDrawState::default() };
    // NUMBER_HISPEED (310) = integer part = 1
    assert_eq!(skin_state_number(310, &state), Some(1));
    // NUMBER_HISPEED_AFTERDOT (311) = decimal part × 100 = 50
    assert_eq!(skin_state_number(311, &state), Some(50));
    // NUMBER_TIMELEFT_MINUTE (163) = 90500 / 60000 = 1
    assert_eq!(skin_state_number(163, &state), Some(1));
    // NUMBER_TIMELEFT_SECOND (164) = (90500 / 1000) % 60 = 90 % 60 = 30
    assert_eq!(skin_state_number(164, &state), Some(30));
    let result_state = SkinDrawState {
        result_failed: Some(false),
        total_duration_ms: 183_000,
        ..SkinDrawState::default()
    };
    // Starseeker 系の Result BMS DATA は選曲詳細の曲長 ref を流用する。
    assert_eq!(skin_state_number(1163, &result_state), Some(3));
    assert_eq!(skin_state_number(1164, &result_state), Some(3));
}

#[test]
fn skin_state_number_maps_bmz_hispeed_mode_refs() {
    let normal = SkinDrawState {
        hispeed_mode_index: 0,
        base_hispeed_index: 1,
        normal_hispeed_level: 13,
        hispeed_config_index: 3,
        total_duration_ms: 500,
        duration_green_ms: Some(300),
        ..SkinDrawState::default()
    };
    let floating = SkinDrawState {
        hispeed_mode_index: 1,
        target_green_number: 280,
        total_duration_ms: 500,
        duration_green_ms: Some(300),
        ..SkinDrawState::default()
    };
    let clamped = SkinDrawState { hispeed_mode_index: 9, ..floating.clone() };
    let mode_text = SkinTextDef { ref_id: 1900, ..SkinTextDef::default() };
    let base_text = SkinTextDef { ref_id: SKIN_REF_BMZ_BASE_HISPEED, ..SkinTextDef::default() };
    let config_text = SkinTextDef { ref_id: SKIN_REF_BMZ_HISPEED_CONFIG, ..SkinTextDef::default() };

    assert_eq!(skin_state_number(1900, &normal), Some(0));
    assert_eq!(skin_state_number(1901, &normal), Some(0));
    assert_eq!(skin_state_number(1902, &normal), Some(300));
    assert_eq!(skin_state_event_index(1900, &normal), 0);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_BASE_HISPEED, &normal), Some(1));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_NORMAL_HISPEED_LEVEL, &normal), Some(13));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_HISPEED_CONFIG, &normal), Some(3));
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_BASE_HISPEED, &normal), 1);
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_NORMAL_HISPEED_LEVEL, &normal), 13);
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_HISPEED_CONFIG, &normal), 3);
    assert!(!test_skin_op(1901, &[], &normal));
    assert_eq!(
        skin_state_text_with_draw_state(&mode_text, Some(&normal), &SkinTextState::default()),
        "NHS"
    );
    assert_eq!(
        skin_state_text_with_draw_state(&base_text, Some(&normal), &SkinTextState::default()),
        "NHS"
    );
    assert_eq!(
        skin_state_text_with_draw_state(&config_text, Some(&normal), &SkinTextState::default()),
        "NORMAL+FLOATING"
    );

    assert_eq!(skin_state_number(1900, &floating), Some(1));
    assert_eq!(skin_state_number(1901, &floating), Some(1));
    assert_eq!(skin_state_number(1902, &floating), Some(280));
    assert_eq!(skin_state_event_index(1900, &floating), 1);
    assert!(test_skin_op(1901, &[], &floating));
    assert_eq!(
        skin_state_text_with_draw_state(&mode_text, Some(&floating), &SkinTextState::default()),
        "FHS"
    );

    assert_eq!(skin_state_number(1900, &clamped), Some(1));
}

#[test]
fn skin_image_index_number_separates_colliding_value_refs() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_clear_count: 99,
        select_gauge_auto_shift_index: 2,
        select_sort_index: 5,
        select_option_panel: 3,
        judge_timing_offset_ms: 42,
        select_chart_normal_notes: 900,
        select_max_bpm: 180.0,
        judge_rank: Some(3),
        duration_green_ms: Some(300),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_image_index_number(78, &state), Some(2));
    assert_eq!(skin_state_number(78, &state), Some(99));

    assert_eq!(skin_image_index_number(12, &state), Some(5));
    assert_eq!(skin_state_number(12, &state), Some(42));

    assert_eq!(skin_image_index_number(350, &state), Some(0));
    assert_eq!(skin_state_number(350, &state), Some(900));

    assert_eq!(skin_image_index_number(400, &state), Some(0));
    assert_eq!(skin_state_number(400, &state), Some(3));
}
