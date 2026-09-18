use super::*;

#[test]
fn select_state_exposes_best_judge_detail_counts() {
    let document: SkinDocument = serde_json::from_str(r#"{ "w": 1280, "h": 720 }"#).unwrap();
    let row = SelectRowSnapshot {
        index: 0,
        total_notes: 100,
        judge_counts: crate::snapshot::DisplayJudgeCounts {
            pgreat: 20,
            great: 30,
            good: 10,
            bad: 5,
            poor: 2,
            empty_poor: 1,
        },
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 2,
            slow_pgreat: 3,
            fast_great: 7,
            slow_good: 4,
            fast_bad: 3,
            slow_empty_poor: 2,
            ..Default::default()
        }),
        ..SelectRowSnapshot::default()
    };
    let snapshot =
        SelectSnapshot { selected_index: 0, rows: vec![row], ..SelectSnapshot::default() };

    let (state, _) = document.select_draw_state(&snapshot, None);

    assert_eq!(skin_state_number(110, &state), Some(20));
    assert_eq!(skin_state_number(111, &state), Some(30));
    assert_eq!(skin_state_number(112, &state), Some(10));
    assert_eq!(skin_state_number(113, &state), Some(5));
    assert_eq!(skin_state_number(426, &state), Some(3));
    assert_eq!(skin_state_number(412, &state), Some(7));
    assert_eq!(skin_state_number(422, &state), Some(2));
    assert!((graph_value(140, &state) - 0.2).abs() < 1e-5);
    assert!((graph_value(141, &state) - 0.3).abs() < 1e-5);
    assert!((graph_value(148, &state) - (12.0 / 21.0)).abs() < 1e-5);
    assert!((graph_value(149, &state) - (9.0 / 21.0)).abs() < 1e-5);
}

#[test]
fn select_state_starts_input_timer_after_document_delay() {
    let document: SkinDocument =
        serde_json::from_str(r#"{ "w": 1280, "h": 720, "input": 500 }"#).unwrap();
    let mut runtime = DynamicTimerRuntime::default();

    let (waiting, _) = document.select_draw_state(
        &SelectSnapshot { time: TimeUs(500_000), ..SelectSnapshot::default() },
        Some(&mut runtime),
    );
    let (active, _) = document.select_draw_state(
        &SelectSnapshot { time: TimeUs(725_000), ..SelectSnapshot::default() },
        Some(&mut runtime),
    );
    let (advanced, _) = document.select_draw_state(
        &SelectSnapshot { time: TimeUs(750_000), ..SelectSnapshot::default() },
        Some(&mut runtime),
    );

    assert_eq!(waiting.start_input_ms, None);
    assert_eq!(active.start_input_ms, Some(0));
    assert_eq!(advanced.start_input_ms, Some(25));
}

#[test]
fn select_render_items_passes_selected_row_genre_to_string_ref_13() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "text": [{ "id": "genre", "size": 6, "ref": 13 }],
                "destination": [{ "id": "genre", "dst": [{ "x": 10, "y": 40, "w": 40, "h": 6 }] }]
            }
            "#,
    )
    .unwrap();
    let snapshot = SelectSnapshot {
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            genre: "Techno".to_string(),
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&HashMap::new(), &snapshot);

    assert!(items.iter().any(|item| matches!(
        item,
        SkinRenderItem::Text { text, .. } if text == "Techno"
    )));
}

#[test]
fn select_render_items_show_editing_marker_in_setting_genre() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "text": [{ "id": "genre", "size": 6, "ref": 13 }],
                "destination": [{
                    "id": "genre",
                    "op": [2],
                    "dst": [{ "x": 10, "y": 40, "w": 40, "h": 6 }]
                }]
            }
            "#,
    )
    .unwrap();
    let snapshot = SelectSnapshot {
        in_settings: true,
        settings_editing: true,
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            kind: SelectRowKind::Config,
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };
    let settings_dest_index =
        crate::select_settings_dest::build_select_settings_dest_index(&document);

    let items = document.select_render_items_with_dynamic_timers(
        &HashMap::new(),
        &snapshot,
        None,
        &settings_dest_index,
        None,
    );

    assert!(items.iter().any(|item| matches!(
        item,
        SkinRenderItem::Text { text, .. } if text == "[編集中]"
    )));
}

#[test]
fn select_course_rows_only_expose_course_stage_title_refs() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "text": [
                    { "id": "title", "size": 6, "ref": 12 },
                    { "id": "genre", "size": 6, "ref": 13 },
                    { "id": "artist", "size": 6, "ref": 16 },
                    { "id": "stage", "size": 6, "ref": 150 }
                ],
                "destination": [
                    { "id": "title", "dst": [{ "x": 10, "y": 70, "w": 40, "h": 6 }] },
                    { "id": "genre", "dst": [{ "x": 10, "y": 60, "w": 40, "h": 6 }] },
                    { "id": "artist", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 6 }] },
                    { "id": "stage", "dst": [{ "x": 10, "y": 40, "w": 40, "h": 6 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let snapshot = SelectSnapshot {
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            title: "Course title".to_string(),
            genre: "Course genre".to_string(),
            artist: "Course artist".to_string(),
            kind: SelectRowKind::Course,
            course_titles: std::array::from_fn(|index| {
                if index == 0 { "Stage title".to_string() } else { String::new() }
            }),
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&HashMap::new(), &snapshot);
    let texts = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(texts, vec!["Stage title"]);
}

#[test]
fn skin_state_text_formats_select_option_fields() {
    let state = SkinTextState {
        target: "AAA",
        select_arrange: "RANDOM",
        select_arrange_2p: "MIRROR",
        select_gauge: "HARD",
        select_gauge_auto_shift: "BEST CLEAR",
        select_bottom_shiftable_gauge: "NORMAL",
        select_double_option: "FLIP",
        select_hs_fix: "MAIN BPM",
        select_assist: "AUTOPLAY",
        select_mode: "7K",
        select_sort: "LEVEL",
        select_ln_mode: "AUTO(LN)",
        select_chart_replication: "RIVALOPTION",
        select_bga: "AUTO",
        select_judge_timing_auto_adjust: "ON",
        ..SkinTextState::default()
    };
    let make_text = |id: &str| SkinTextDef { id: id.to_string(), ..SkinTextDef::default() };

    assert_eq!(skin_state_text(&make_text("bmz_select_target"), &state), "RANK AAA");
    assert_eq!(skin_state_text(&make_text("bmz_select_arrange"), &state), "RANDOM");
    assert_eq!(skin_state_text(&make_text("bmz_select_arrange_2p"), &state), "MIRROR");
    assert_eq!(skin_state_text(&make_text("bmz_select_gauge"), &state), "HARD");
    assert_eq!(skin_state_text(&make_text("bmz_select_gauge_auto_shift"), &state), "BEST CLEAR");
    assert_eq!(skin_state_text(&make_text("bmz_select_bottom_shiftable_gauge"), &state), "NORMAL");
    assert_eq!(skin_state_text(&make_text("bmz_select_double_option"), &state), "FLIP");
    assert_eq!(skin_state_text(&make_text("bmz_select_hs_fix"), &state), "MAIN BPM");
    assert_eq!(skin_state_text(&make_text("bmz_select_assist"), &state), "AUTOPLAY");
    assert_eq!(skin_state_text(&make_text("bmz_select_mode"), &state), "7K");
    for mode in ["ALL", "4K", "6K", "8K", "14K"] {
        let mode_state = SkinTextState { select_mode: mode, ..SkinTextState::default() };
        assert_eq!(skin_state_text(&make_text("bmz_select_mode"), &mode_state), mode);
    }
    assert_eq!(skin_state_text(&make_text("bmz_select_sort"), &state), "LEVEL");
    assert_eq!(skin_state_text(&make_text("bmz_select_ln_mode"), &state), "AUTO(LN)");
    assert_eq!(skin_state_text(&make_text("bmz_select_chart_replication"), &state), "RIVALOPTION");
    assert_eq!(skin_state_text(&make_text("bmz_select_bga"), &state), "AUTO");
    assert_eq!(skin_state_text(&make_text("bmz_select_judge_timing_auto_adjust"), &state), "ON");
}

#[test]
fn skin_state_text_formats_lr2_random_mix_refs() {
    let draw_state =
        SkinDrawState { random_mix_options: [0, 12, 3, 7, 180, 90, 0], ..SkinDrawState::default() };
    let text_state = SkinTextState::default();
    let expected = ["OFF", "LEVEL 12", "LEVEL 3", "+- 7BPM", "180 BPM", "90 BPM", "RANDOM"];

    for (offset, expected) in expected.into_iter().enumerate() {
        assert_eq!(
            skin_main_state_text(190 + offset as i32, Some(&draw_state), &text_state),
            expected
        );
    }
}

#[test]
fn select_search_input_overlay_does_not_repeat_the_skin_text_fade() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "font": [{ "id": "skin-font", "path": "search.fnt" }],
                "text": [
                    { "id": "search", "font": "skin-font", "size": 24, "ref": 30 },
                    { "id": "blink", "constantText": "blink", "size": 10 },
                    { "id": "after", "constantText": "after", "size": 10 }
                ],
                "destination": [
                    { "id": "search", "dst": [
                        { "time": 400, "x": 10, "y": 20, "w": 50, "h": 20,
                          "a": 0, "r": 200, "g": 210, "b": 220 },
                        { "time": 550, "a": 255 }
                    ]},
                    { "id": "blink", "dst": [
                        { "time": 400, "x": 0, "y": 0, "w": 10, "h": 10, "a": 0 },
                        { "time": 550, "a": 255 }
                    ]},
                    { "id": "after", "dst": [
                        { "x": 0, "y": 0, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();
    let snapshot = SelectSnapshot {
        time: TimeUs(600_000),
        search_word: "query".to_string(),
        search_word_alpha: 0.8,
        search_caret_byte_index: Some(2),
        search_input_active: true,
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&HashMap::new(), &snapshot);
    let texts = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    // The ordinary omitted-loop text has restarted before its first frame,
    // while the search input is held at the settled destination and appended
    // after all skin objects.
    assert_eq!(texts, vec!["after", "query"]);
    let search = items.last().expect("search input overlay");
    match search {
        SkinRenderItem::Text { style, caret: Some(caret), .. } => {
            assert_eq!(style.font_id, None);
            assert_eq!(style.bitmap_size, None);
            assert_eq!(style.align, TextAlign::Left);
            assert!(approx_eq(style.size, 0.2));
            assert!(approx_eq(style.color.a, 0.8));
            assert_eq!(caret.byte_index, 2);
            assert_eq!(caret.color, Color::rgb(1.0, 1.0, 1.0));
        }
        other => panic!("expected search input text with caret, got {other:?}"),
    }

    let rect = document
        .select_search_input_rect(
            &snapshot,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
        )
        .expect("search input rect");
    assert!(approx_eq(rect.x, 0.1));
    assert!(approx_eq(rect.y, 0.6));
    assert!(approx_eq(rect.width, 0.5));
    assert!(approx_eq(rect.height, 0.2));
}

#[test]
fn select_search_placeholder_keeps_its_destination_z_order() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
            "type": 5,
            "w": 100,
            "h": 100,
            "text": [
                { "id": "before", "constantText": "before", "size": 10 },
                { "id": "search", "font": "skin-font", "size": 10, "ref": 30 },
                { "id": "panel", "constantText": "panel", "size": 10 }
            ],
            "destination": [
                { "id": "before", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }]},
                { "id": "search", "dst": [{ "x": 10, "y": 20, "w": 50, "h": 10 }]},
                { "id": "panel", "dst": [{ "x": 0, "y": 0, "w": 100, "h": 100 }]}
            ]
        }"#,
    )
    .unwrap();
    let snapshot = SelectSnapshot {
        search_word: "placeholder".to_string(),
        search_word_alpha: 0.45,
        search_input_active: false,
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&HashMap::new(), &snapshot);
    let texts = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(texts, vec!["before", "placeholder", "panel"]);
    assert!(matches!(
        &items[1],
        SkinRenderItem::Text { style, .. }
            if style.font_id.is_none()
                && style.bitmap_size.is_none()
                && approx_eq(style.color.a, 0.45)
    ));
}

#[test]
fn select_search_input_overlay_keeps_empty_text_when_the_caret_is_visible() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
            "type": 5,
            "w": 100,
            "h": 100,
            "text": [{ "id": "search", "size": 10, "ref": 30 }],
            "destination": [{ "id": "search", "dst": [
                { "x": 10, "y": 20, "w": 50, "h": 10 }
            ]}]
        }"#,
    )
    .unwrap();
    let snapshot = SelectSnapshot {
        search_word: String::new(),
        search_caret_byte_index: Some(0),
        search_input_active: true,
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&HashMap::new(), &snapshot);

    assert!(matches!(
        items.as_slice(),
        [SkinRenderItem::Text { text, caret: Some(TextCaret { byte_index: 0, .. }), .. }]
            if text.is_empty()
    ));
}
