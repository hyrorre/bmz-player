use super::*;

#[test]
fn luxe_flat_select_score_and_chart_values_use_runtime_metadata() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_ex_score: Some(2_890),
        select_play_count: 2,
        select_total_notes: 1_498,
        select_chart_total_gauge: 290.0,
        select_length_ms: 152_000,
        ..SkinDrawState::default()
    };

    let diff = SkinValueDef {
        value_expr: "bmz:nearest_rank_diff_abs".to_string(),
        ..SkinValueDef::default()
    };
    let ratio_integer = SkinValueDef {
        value_expr: SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER.to_string(),
        ..SkinValueDef::default()
    };
    let ratio_fraction = SkinValueDef {
        value_expr: SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION.to_string(),
        ..SkinValueDef::default()
    };

    assert_eq!(skin_value_number(&diff, &state), Some(106));
    assert_eq!(skin_value_number(&ratio_integer, &state), Some(0));
    assert_eq!(skin_value_number(&ratio_fraction, &state), Some(193));
    assert_eq!(skin_state_number(1163, &state), Some(2));
    assert_eq!(skin_state_number(1164, &state), Some(32));
    assert!(eval_skin_draw_condition(
        "select_score_available() and nearest_rank(MAX,minus)",
        &state
    ));
    assert!(!eval_skin_draw_condition("select_score_available() and nearest_rank(F,plus)", &state));
    assert!(eval_skin_draw_condition(
        "select_score_available() and nearest_rank_label_width(3)",
        &state
    ));
    // 2,890 / 2,996 is 96.5%: Luxe Flat's two-digit score-rate layout.
    for (band, visible) in [("10,11", false), ("1,10", true), ("0,1", false)] {
        assert_eq!(
            eval_skin_draw_condition(
                &format!("select_score_available() and score_rate_tenths({band})"),
                &state
            ),
            visible,
            "{band}"
        );
    }
    let max = SkinDrawState { select_ex_score: Some(2_996), ..state.clone() };
    assert!(eval_skin_draw_condition("score_rate_tenths(10,11)", &max));
    assert!(!eval_skin_draw_condition("score_rate_tenths(1,10)", &max));
    let low = SkinDrawState { select_ex_score: Some(299), ..state.clone() };
    assert!(eval_skin_draw_condition("score_rate_tenths(0,1)", &low));
    let tenth = SkinDrawState { select_ex_score: Some(300), ..state };
    assert!(eval_skin_draw_condition("score_rate_tenths(1,10)", &tenth));
}

#[test]
fn select_course_keeps_score_totals_but_hides_per_chart_details() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Course,
        select_in_library: true,
        select_total_notes: 10_718,
        total_notes: 10_718,
        select_chart_normal_notes: 10_718,
        select_chart_total_gauge: 224.0,
        select_length_ms: 180_000,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(74, &state), Some(10_718));
    for ref_id in [90, 91, 92, 160, 350, 351, 352, 353, 360, 362, 364, 368, 1163, 1164] {
        assert_eq!(skin_state_number(ref_id, &state), None, "chart detail ref {ref_id}");
    }
}

#[test]
fn skin_state_imageset_index_maps_select_options() {
    let state = SkinDrawState {
        select_screen: true,
        select_arrange_index: 2,
        select_arrange_2p_index: 5,
        select_double_option_index: 3,
        select_hs_fix_index: 4,
        select_gauge_index: 4,
        select_target_index: 3,
        select_bga_index: 1,
        judge_timing_auto_adjust: true,
        select_judge_algorithm_index: 2,
        assist_flags: [true, false, true, false, true, false, true],
        assist_extra_note_depth: 4,
        assist_mine_mode: 3,
        assist_scroll_mode: 2,
        assist_long_note_mode: 5,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_imageset_index(42, &state), Some(2));
    assert_eq!(skin_state_imageset_index(43, &state), Some(5));
    assert_eq!(skin_state_imageset_index(54, &state), Some(3));
    assert_eq!(skin_state_imageset_index(55, &state), Some(4));
    assert_eq!(skin_state_imageset_index(40, &state), Some(4));
    assert_eq!(skin_state_imageset_index(41, &state), Some(3));
    assert_eq!(skin_state_imageset_index(75, &state), Some(1));
    assert_eq!(skin_state_imageset_index(72, &state), Some(1));
    assert_eq!(skin_state_imageset_index(340, &state), Some(2));
    for (ref_id, enabled) in (301..=307).zip([true, false, true, false, true, false, true]) {
        assert_eq!(skin_state_imageset_index(ref_id, &state), Some(usize::from(enabled)));
    }
    assert_eq!(skin_state_imageset_index(350, &state), Some(4));
    assert_eq!(skin_state_imageset_index(351, &state), Some(3));
    assert_eq!(skin_state_imageset_index(352, &state), Some(2));
    assert_eq!(skin_state_imageset_index(353, &state), Some(5));
    assert_eq!(skin_state_imageset_index(500, &state), None);
}

#[test]
fn select_arrange_index_maps_beatoraja_random_options() {
    assert_eq!(select_arrange_index("NORMAL"), 0);
    assert_eq!(select_arrange_index("MIRROR"), 1);
    assert_eq!(select_arrange_index("RANDOM"), 2);
    assert_eq!(select_arrange_index("R-RANDOM"), 3);
    assert_eq!(select_arrange_index("S-RANDOM"), 4);
    assert_eq!(select_arrange_index("SPIRAL"), 5);
    assert_eq!(select_arrange_index("H-RANDOM"), 6);
    assert_eq!(select_arrange_index("ALL-SCR"), 7);
    assert_eq!(select_arrange_index("RANDOM-EX"), 8);
    assert_eq!(select_arrange_index("S-RANDOM-EX"), 9);
    assert_eq!(select_arrange_index("F-RANDOM"), 2);
    assert_eq!(select_arrange_index("MF-RANDOM"), 2);
    assert_eq!(extended_arrange_index("F-RANDOM"), 10);
    assert_eq!(extended_arrange_index("MF-RANDOM"), 11);
    assert_eq!(select_arrange_index("unknown"), 0);
}

#[test]
fn select_judge_algorithm_index_maps_beatoraja_order() {
    assert_eq!(select_judge_algorithm_index("Combo"), 0);
    assert_eq!(select_judge_algorithm_index("Duration"), 1);
    assert_eq!(select_judge_algorithm_index("Lowest"), 2);
    assert_eq!(select_judge_algorithm_index("unknown"), 0);
}

#[test]
fn select_hs_fix_index_maps_beatoraja_order() {
    assert_eq!(select_hs_fix_index("OFF"), 0);
    assert_eq!(select_hs_fix_index("START BPM"), 1);
    assert_eq!(select_hs_fix_index("MAX BPM"), 2);
    assert_eq!(select_hs_fix_index("MAIN BPM"), 3);
    assert_eq!(select_hs_fix_index("MIN BPM"), 4);
    assert_eq!(select_hs_fix_index("unknown"), 0);
}

#[test]
fn select_session_mode_keeps_legacy_assist_binary_and_exposes_exact_bmz_index() {
    let cases = [
        ("NORMAL", 0, 0),
        ("PRACTICE", 0, 1),
        ("AUTOPLAY", 1, 2),
        ("AUTOPLAY BATTLE", 1, 3),
        ("G-BATTLE", 0, 4),
        ("GHOST BATTLE", 0, 4),
    ];

    for (mode, assist_index, session_mode_index) in cases {
        assert_eq!(select_assist_index(mode), assist_index);
        assert_eq!(select_session_mode_index(mode), session_mode_index);
    }
}

#[test]
fn skin_image_ref_number_maps_extended_select_arrange() {
    let state = SkinDrawState {
        select_screen: true,
        select_arrange_index: 9,
        select_arrange_2p_index: 6,
        select_extended_arrange_index: 11,
        select_extended_arrange_2p_index: 10,
        select_gauge_index: 4,
        select_target_index: 10,
        select_double_option_index: 2,
        select_hs_fix_index: 3,
        select_bga_index: 2,
        select_assist_index: 1,
        select_session_mode_index: 3,
        judge_timing_auto_adjust: true,
        select_gauge_auto_shift_index: 3,
        select_ln_mode_index: 2,
        select_judge_algorithm_index: 3,
        select_bottom_shiftable_gauge_index: 2,
        duration_green_ms: Some(300),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_image_ref_number(40, &state), Some(4));
    assert_eq!(skin_image_ref_number(41, &state), Some(10));
    assert_eq!(skin_image_ref_number(42, &state), Some(9));
    assert_eq!(skin_image_ref_number(43, &state), Some(6));
    assert_eq!(skin_image_ref_number(344, &state), Some(11));
    assert_eq!(skin_image_ref_number(345, &state), Some(10));
    assert_eq!(skin_image_ref_number(54, &state), Some(2));
    assert_eq!(skin_image_ref_number(55, &state), Some(3));
    assert_eq!(skin_image_ref_number(72, &state), Some(2));
    assert_eq!(skin_image_ref_number(75, &state), Some(1));
    assert_eq!(skin_image_ref_number(78, &state), Some(3));
    assert_eq!(skin_image_ref_number(308, &state), Some(2));
    assert_eq!(skin_image_ref_number(340, &state), Some(3));
    assert_eq!(skin_image_ref_number(341, &state), Some(2));
    assert_eq!(skin_state_number(42, &state), Some(9));
    assert_eq!(skin_state_number(43, &state), Some(6));
    assert_eq!(skin_state_number(344, &state), Some(11));
    assert_eq!(skin_state_number(345, &state), Some(10));
    assert_eq!(skin_state_number(54, &state), Some(2));
    assert_eq!(skin_state_number(55, &state), Some(3));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SELECT_SESSION_MODE, &state), Some(3));
    assert_eq!(skin_state_number(340, &state), Some(3));
    assert_eq!(skin_state_event_index(40, &state), 4);
    assert_eq!(skin_state_event_index(41, &state), 10);
    assert_eq!(skin_state_event_index(42, &state), 9);
    assert_eq!(skin_state_event_index(43, &state), 6);
    assert_eq!(skin_state_event_index(344, &state), 11);
    assert_eq!(skin_state_event_index(345, &state), 10);
    assert_eq!(skin_state_event_index(54, &state), 2);
    assert_eq!(skin_state_event_index(55, &state), 3);
    assert_eq!(skin_state_event_index(72, &state), 2);
    assert_eq!(skin_state_event_index(73, &state), 1);
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_SELECT_SESSION_MODE, &state), 3);
    assert_eq!(skin_state_event_index(75, &state), 1);
    assert_eq!(skin_state_event_index(78, &state), 3);
    assert_eq!(skin_state_event_index(308, &state), 2);
    assert_eq!(skin_state_event_index(340, &state), 3);
    assert_eq!(skin_state_event_index(341, &state), 2);
}

#[test]
fn select_skin_imageset_uses_extended_arrange_index() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "arrange.png" }],
                "image": [
                    { "id": "normal", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "mirror", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 },
                    { "id": "random", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 },
                    { "id": "r-random", "src": 1, "x": 30, "y": 0, "w": 10, "h": 10 },
                    { "id": "s-random", "src": 1, "x": 40, "y": 0, "w": 10, "h": 10 },
                    { "id": "spiral", "src": 1, "x": 50, "y": 0, "w": 10, "h": 10 },
                    { "id": "h-random", "src": 1, "x": 60, "y": 0, "w": 10, "h": 10 },
                    { "id": "all-scr", "src": 1, "x": 70, "y": 0, "w": 10, "h": 10 },
                    { "id": "random-ex", "src": 1, "x": 80, "y": 0, "w": 10, "h": 10 },
                    { "id": "s-random-ex", "src": 1, "x": 90, "y": 0, "w": 10, "h": 10 }
                ],
                "imageset": [{
                    "id": "option-random",
                    "ref": 42,
                    "images": [
                        "normal", "mirror", "random", "r-random", "s-random",
                        "spiral", "h-random", "all-scr", "random-ex", "s-random-ex"
                    ]
                }],
                "destination": [{
                    "id": "option-random",
                    "dst": [{ "x": 10, "y": 20, "w": 20, "h": 10 }]
                }]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 100.0, height: 10.0 },
        },
    )]);
    let items = document.select_render_items(
        &sources,
        &crate::scene::SelectSnapshot { arrange: "S-RANDOM-EX".to_string(), ..Default::default() },
    );

    assert!(matches!(
        items.as_slice(),
        [SkinRenderItem::Image {
            texture: SkinTextureId(42),
            uv: TextureRegion { x, .. },
            ..
        }] if approx_eq(*x, 0.9)
    ));
}

#[test]
fn select_target_index_maps_fixed_targets() {
    let index = |target| select_target_index_for_name(target).unwrap_or(0);
    assert_eq!(index("NONE"), 0);
    assert_eq!(index("RANK_A"), 1);
    assert_eq!(index("RANK_AA-"), 2);
    assert_eq!(index("RANK_AA"), 3);
    assert_eq!(index("RANK_AAA-"), 4);
    assert_eq!(index("RANK_AAA"), 5);
    assert_eq!(index("RANK_MAX-"), 6);
    assert_eq!(index("MAX"), 7);
    assert_eq!(index("RANK_NEXT"), 8);
    assert_eq!(index("IR_TOP"), 9);
    assert_eq!(index("IR_NEXT"), 10);
    assert_eq!(index("RIVAL TOP"), 11);
    assert_eq!(index("RIVAL NEXT"), 12);
    assert_eq!(index("RIVAL"), 11);
    assert_eq!(index("AAA"), 5);
    assert_eq!(index("AA"), 3);
    assert_eq!(index("A"), 1);
    assert_eq!(index("B"), 1);
}

#[test]
fn bundled_beatoraja_default_select_json_loads_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/beatoraja/skin/default/select.json");
    if !path.is_file() {
        return;
    }

    let document = SkinDocument::load_beatoraja_json(&path).unwrap();

    assert_eq!(document.name, "beatoraja default");
    assert_eq!(document.skin_type, 5);
    assert!(document.songlist.is_some());
    assert!(!document.destination.is_empty());
}

#[test]
fn local_ecfn_converted_select_json_loads_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/ECFN/select/select-converted.json");
    if !path.is_file() {
        return;
    }

    let document = SkinDocument::load_beatoraja_json(&path).unwrap();

    assert_eq!(document.skin_type, 5);
    assert!(document.songlist.is_some());
    assert!(!document.destination.is_empty());
}

#[test]
fn judge_timer_elapsed_ms_selects_animation_frame() {
    // timer=46 → TIMER_JUDGE_1P; two dst frames at time=0 and time=200
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "timer": 46, "loop": 200, "dst": [
                        { "time": 0,   "x": 0,   "y": 0, "w": 10, "h": 10 },
                        { "time": 200, "x": 50,  "y": 0, "w": 10, "h": 10 }
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
            texture: SkinTextureId(2),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    // judge_ms=Some(100) → between frame 0 and frame 200 → x should be 0.25 (interpolated)
    let state_early = SkinDrawState {
        judge_ms: judge_region_state(0, 100, 0).judge_ms,
        ..SkinDrawState::default()
    };
    let items_early = document.static_image_render_items(&sources, &state_early);
    assert_eq!(items_early.len(), 1);
    assert!(
        matches!(items_early[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.25)),
        "at judge_ms=100, x should interpolate to 0.25 (halfway between 0 and 0.5)"
    );

    // judge_ms=Some(300) → past last frame → last frame x=0.5
    let state_late = SkinDrawState {
        judge_ms: judge_region_state(0, 300, 0).judge_ms,
        ..SkinDrawState::default()
    };
    let items_late = document.static_image_render_items(&sources, &state_late);
    assert_eq!(items_late.len(), 1);
    assert!(
        matches!(items_late[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.5)),
        "at judge_ms=300 (past last frame), x should be at last frame x=0.5"
    );

    // judge_ms=None → no items (timer inactive)
    let state_inactive =
        SkinDrawState { judge_ms: [None; MAX_JUDGE_REGIONS], ..SkinDrawState::default() };
    let items_inactive = document.static_image_render_items(&sources, &state_inactive);
    assert_eq!(items_inactive.len(), 0, "judge_ms=None should produce no items");
}

#[test]
fn dst_if_value_selects_frame_by_enabled_option() {
    // property: option 920 enabled (1P)
    // destination dst has two conditional frames: one for 920, one for 921
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "property": [
                    { "name": "Side", "def": "1P", "item": [
                        { "name": "1P", "op": 920 },
                        { "name": "2P", "op": 921 }
                    ]}
                ],
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "dst": [
                        { "if": [920], "value": { "time": 0, "x": 100, "y": 200, "w": 50, "h": 50 } },
                        { "if": [921], "value": { "time": 0, "x": 900, "y": 200, "w": 50, "h": 50 } },
                        { "time": 500 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("src", 10.0, 10.0);
    let state = SkinDrawState::default();
    let items = document.static_image_render_items(&sources, &state);

    // With option 920 (1P) enabled, x should be 100/1280
    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    assert!(approx_eq(rect.x, 100.0 / 1280.0), "expected 1P x position, got {}", rect.x);
}

#[test]
fn skin_state_number_maps_folder_lamp_counts_on_folder_rows() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        select_folder_lamp_counts: [12, 3, 0, 1, 4, 5, 6, 7, 8, 9, 10],
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(320, &state), Some(12));
    assert_eq!(skin_state_number(321, &state), Some(3));
    assert_eq!(skin_state_number(324, &state), Some(4));
    assert_eq!(skin_state_number(330, &state), Some(10));

    let song = SkinDrawState {
        select_row_kind: SelectRowKind::Song,
        select_is_folder: false,
        select_in_library: true,
        select_chart_normal_notes: 900,
        ..state
    };
    assert_eq!(skin_state_number(320, &song), None);
    assert_eq!(skin_state_number(300, &song), None);
    assert_eq!(skin_state_number(350, &song), Some(900));
}

#[test]
fn select_folder_song_count_uses_cursor_folder_row() {
    let row = SelectRowSnapshot {
        is_folder: true,
        kind: SelectRowKind::Folder,
        folder_lamp_counts: [2, 3, 0, 1, 0, 0, 0, 0, 0, 0, 0],
        ..SelectRowSnapshot::default()
    };
    assert_eq!(select_row_folder_song_count(&row), Some(6));

    let song = SelectRowSnapshot { kind: SelectRowKind::Song, ..row };
    assert_eq!(select_row_folder_song_count(&song), None);
}

#[test]
fn skin_image_index_number_select_favorite_refs() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_favorite_song: true,
        select_favorite_chart: false,
        select_max_bpm: 200.0,
        ..SkinDrawState::default()
    };
    assert_eq!(skin_image_index_number(89, &state), Some(1));
    assert_eq!(skin_image_index_number(90, &state), Some(0));
    assert_eq!(skin_state_number(90, &state), Some(200));

    let chart_favorite = SkinDrawState { select_favorite_chart: true, ..state };
    assert_eq!(skin_image_index_number(90, &chart_favorite), Some(1));
    assert_eq!(skin_state_number(90, &chart_favorite), Some(200));
}

#[test]
fn graph_value_select_rate_exscore_uses_selected_total_notes() {
    // ECFN select uses BARGRAPH_RATE_EXSCORE (147) for the score rate bar.
    // Select has no play-progress past_notes, so it should use the selected chart total.
    let state = SkinDrawState {
        select_screen: true,
        ex_score: 418,
        total_notes: 594,
        select_total_notes: 594,
        past_notes: 0,
        ..SkinDrawState::default()
    };
    let v = graph_value(147, &state);
    assert!((v - (418.0 / 1188.0)).abs() < 1e-5, "select ex rate: got {v}");
}
