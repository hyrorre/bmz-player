use super::*;

#[test]
fn select_click_hit_resolves_image_act_event() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "button.png" }],
                "image": [
                    { "id": "button_play", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "act": 15, "click": 2 }
                ],
                "destination": [
                    { "id": "button_play", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let snapshot = match crate::sample::sample_select_scene() {
        crate::scene::AppSceneSnapshot::Select(snapshot) => snapshot,
        _ => unreachable!(),
    };

    let hit = document
        .select_click_hit(
            &sources,
            &snapshot,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
            0.2,
            0.75,
        )
        .unwrap();

    assert_eq!(hit.target, SkinClickTarget::Event { event_id: 15, click: 2 });
    assert_eq!(hit.rect, Rect { x: 0.1, y: 0.7, width: 0.3, height: 0.1 });
}

#[test]
fn select_mouse_rect_gates_render_and_click_hits() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "button.png" }],
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
    let sources = mock_source("1", 100.0, 100.0);
    let inside = SelectSnapshot { mouse_position: Some((0.16, 0.75)), ..SelectSnapshot::default() };
    let outside =
        SelectSnapshot { mouse_position: Some((0.01, 0.01)), ..SelectSnapshot::default() };
    let unavailable = SelectSnapshot::default();

    assert!(document.select_render_items(&sources, &inside).iter().any(|item| {
        matches!(item, SkinRenderItem::Image { texture: SkinTextureId(9999), .. })
    }));
    assert!(!document.select_render_items(&sources, &outside).iter().any(|item| {
        matches!(item, SkinRenderItem::Image { texture: SkinTextureId(9999), .. })
    }));
    assert!(!document.select_render_items(&sources, &unavailable).iter().any(|item| {
        matches!(item, SkinRenderItem::Image { texture: SkinTextureId(9999), .. })
    }));

    assert!(
        document
            .select_click_hit(
                &sources,
                &inside,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.2,
                0.75,
            )
            .is_some()
    );
    assert!(
        document
            .select_click_hit(
                &sources,
                &outside,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.2,
                0.75,
            )
            .is_none()
    );
    assert!(
        document
            .select_click_hit(
                &sources,
                &unavailable,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.2,
                0.75,
            )
            .is_none()
    );
}

#[test]
fn select_slider_hit_resolves_changeable_volume_slider() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "slider": [
                    { "id": "master", "src": 1, "x": 0, "y": 0, "w": 10, "h": 5, "angle": 1, "range": 50, "type": 17 }
                ],
                "destination": [
                    { "id": "master", "dst": [{ "x": 10, "y": 20, "w": 10, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let snapshot = SelectSnapshot::default();

    // angle=1 destination x=10 range=50 → value 0.5 at skin x=35 (norm x=0.35)
    let hit = document
        .select_slider_hit(
            &snapshot,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
            0.35,
            0.775,
        )
        .unwrap();

    assert_eq!(hit.slider_type, 17);
    assert!(approx_eq(hit.value, 0.5));
    assert!(
        document
            .select_slider_hit(
                &snapshot,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.70,
                0.775,
            )
            .is_none()
    );
}

#[test]
fn select_slider_hit_resolves_song_scroll_slider() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "slider": [
                    { "id": "song-scroll", "src": 1, "x": 0, "y": 0, "w": 10, "h": 5, "angle": 2, "range": 50, "type": 1 }
                ],
                "destination": [
                    { "id": "song-scroll", "dst": [{ "x": 10, "y": 70, "w": 10, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let snapshot = SelectSnapshot::default();

    // beatoraja: value=(region.y - mouse_y)/range. Mid = skin y 45 → norm 0.55.
    let hit = document
        .select_slider_hit(
            &snapshot,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
            0.15,
            0.55,
        )
        .unwrap();

    assert_eq!(hit.slider_type, 1);
    assert!(approx_eq(hit.value, 0.5));
    // Top of track (value 0) is destination y itself → skin y 70 → norm 0.30.
    let top_hit = document
        .select_slider_hit(
            &snapshot,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
            0.15,
            0.30,
        )
        .unwrap();
    assert_eq!(top_hit.slider_type, 1);
    assert!(approx_eq(top_hit.value, 0.0));
}

#[test]
fn select_slider_hit_matches_mz_select_songlist_scroll_collision() {
    // mz-select default_songlistscroll2 collision:
    // parts_position=(1888,270), dst x=1864 y=790 w=64 h=64, angle=2 range=500
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 1920,
                "h": 1080,
                "slider": [
                    {
                        "id": "default_songlistscroll2_collision",
                        "src": 1,
                        "x": 80,
                        "y": 0,
                        "w": 64,
                        "h": 64,
                        "angle": 2,
                        "range": 500,
                        "type": 1
                    }
                ],
                "destination": [
                    {
                        "id": "default_songlistscroll2_collision",
                        "dst": [{ "x": 1864, "y": 790, "w": 64, "h": 64 }]
                    }
                ]
            }
            "#,
    )
    .unwrap();
    let snapshot = SelectSnapshot::default();
    let settings = crate::select_settings_dest::SelectSettingsDestIndex::default();
    let x = (1864.0 + 32.0) / 1920.0;

    let top = document.select_slider_hit(&snapshot, &settings, x, 1.0 - 790.0 / 1080.0).unwrap();
    assert_eq!(top.slider_type, 1);
    assert!(approx_eq(top.value, 0.0));

    let mid = document.select_slider_hit(&snapshot, &settings, x, 1.0 - 540.0 / 1080.0).unwrap();
    assert_eq!(mid.slider_type, 1);
    assert!(approx_eq(mid.value, 0.5));

    let bottom = document.select_slider_hit(&snapshot, &settings, x, 1.0 - 290.0 / 1080.0).unwrap();
    assert_eq!(bottom.slider_type, 1);
    assert!(approx_eq(bottom.value, 1.0));

    // Clicks above destination y must miss (beatoraja uses region.y as the upper edge).
    assert!(document.select_slider_hit(&snapshot, &settings, x, 1.0 - 822.0 / 1080.0).is_none());
}

#[test]
fn select_click_hit_resolves_clickable_songlist_row() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "clickable": [0],
                    "liston": [
                        { "id": "bar", "dst": [{ "x": 0, "y": 0, "w": 50, "h": 10 }] }
                    ],
                    "listoff": [
                        { "id": "bar", "dst": [{ "x": 50, "y": 0, "w": 50, "h": 10 }] }
                    ]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
    )
    .unwrap();
    let snapshot = match crate::sample::sample_select_scene() {
        crate::scene::AppSceneSnapshot::Select(snapshot) => snapshot,
        _ => unreachable!(),
    };

    let hit = document
        .select_click_hit(
            &HashMap::new(),
            &snapshot,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
            0.25,
            0.95,
        )
        .unwrap();

    assert_eq!(hit.target, SkinClickTarget::SelectRow { row_index: 0 });
    assert_eq!(hit.rect, Rect { x: 0.0, y: 0.9, width: 0.5, height: 0.1 });
    assert!(
        document
            .select_click_hit(
                &HashMap::new(),
                &snapshot,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.75,
                0.95,
            )
            .is_none()
    );
}

#[test]
fn select_skin_document_advances_dynamic_timers() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "marker.png" }],
                "image": [{ "id": "marker", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "marker", "timer": 9001, "dst": [{ "x": 10, "y": 10, "w": 10, "h": 10 }] }
                ],
                "dynamicTimer": [{ "id": 9001, "observe": "number(300) > 0" }]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let snapshot = SelectSnapshot {
        time: TimeUs(100_000),
        chart_count: 1,
        rows: vec![SelectRowSnapshot {
            index: 0,
            is_folder: true,
            kind: SelectRowKind::Folder,
            folder_lamp_counts: [1; 11],
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    assert!(document.select_render_items(&sources, &snapshot).is_empty());

    let mut runtime = DynamicTimerRuntime::default();
    let items = document.select_render_items_with_dynamic_timers(
        &sources,
        &snapshot,
        Some(&mut runtime),
        &crate::select_settings_dest::SelectSettingsDestIndex::default(),
        None,
    );

    assert_eq!(items.len(), 1);
    assert!(matches!(items[0], SkinRenderItem::Image { .. }));
}

#[test]
fn select_skin_document_renders_unowned_song_with_nograde_bar() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "bar.png" }],
                "image": [
                    { "id": "bar-song", "src": 1, "x": 0, "y": 0, "w": 40, "h": 10 },
                    { "id": "bar-nograde", "src": 1, "x": 0, "y": 40, "w": 40, "h": 10 }
                ],
                "imageset": [{
                    "id": "bar",
                    "images": ["bar-song", "bar-song", "bar-song", "bar-song", "bar-nograde"]
                }],
                "text": [
                    { "id": "bartext-owned", "font": "main", "size": 10 },
                    { "id": "bartext-owned2", "font": "main", "size": 10 },
                    { "id": "bartext-owned3", "font": "main", "size": 10 },
                    { "id": "bartext-owned4", "font": "main", "size": 10 },
                    { "id": "bartext-owned5", "font": "main", "size": 10 },
                    { "id": "bartext-owned6", "font": "main", "size": 10 },
                    { "id": "bartext-owned7", "font": "main", "size": 10 },
                    { "id": "bartext-owned8", "font": "main", "size": 10 },
                    { "id": "bartext-unowned", "font": "unowned", "size": 10 }
                ],
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "listoff": [{ "id": "bar", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 10 }] }],
                    "liston": [{ "id": "bar", "dst": [{ "x": 12, "y": 50, "w": 40, "h": 10 }] }],
                    "text": [
                        { "id": "bartext-owned", "dst": [{ "x": 1, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned2", "dst": [{ "x": 2, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned3", "dst": [{ "x": 3, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned4", "dst": [{ "x": 4, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned5", "dst": [{ "x": 5, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned6", "dst": [{ "x": 6, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned7", "dst": [{ "x": 7, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned8", "dst": [{ "x": 8, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-unowned", "dst": [{ "x": 9, "y": 2, "w": 20, "h": 8 }] }
                    ]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
    )
    .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let snapshot = SelectSnapshot {
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            title: "Missing Song".to_string(),
            in_library: false,
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&sources, &snapshot);

    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                uv: TextureRegion { y: v, .. },
                ..
            } if approx_eq(*v, 40.0 / 100.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Text {
                text,
                style,
                ..
            } if text == "Missing Song" && style.font_id.as_deref() == Some("unowned"))));
}

#[test]
fn select_skin_uses_snapshot_time_and_bar_type_ops() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "panel.png" }],
                "image": [
                    { "id": "song-panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "folder-panel", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "song-panel", "timer": 11, "loop": 200, "op": [2], "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                        { "time": 200, "x": 20 }
                    ] },
                    { "id": "folder-panel", "op": [1], "dst": [
                        { "x": 50, "y": 0, "w": 10, "h": 10 }
                    ] },
                    { "id": "song-panel", "timer": 21, "op": [21], "dst": [
                        { "time": 0, "x": 30, "y": 0, "w": 10, "h": 10 },
                        { "time": 200, "x": 50 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = mock_source("1", 100.0, 100.0);
    let snapshot = SelectSnapshot {
        time: bmz_core::time::TimeUs(100_000),
        selection_time: bmz_core::time::TimeUs(100_000),
        option_panel_time: bmz_core::time::TimeUs(100_000),
        option_panel: 1,
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            title: "Song".to_string(),
            is_folder: false,
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&sources, &snapshot);

    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.1) && approx_eq(*u, 0.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(*x, 0.4))));
}

#[test]
fn select_folder_hides_song_score_numbers() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        select_in_library: true,
        ex_score: 1234,
        total_notes: 1000,
        select_total_notes: 1000,
        select_play_count: 7,
        select_clear_count: 3,
        select_bp: Some(12),
        select_cb: Some(8),
        judge_counts: DisplayJudgeCounts {
            pgreat: 20,
            great: 30,
            good: 10,
            bad: 5,
            poor: 2,
            empty_poor: 1,
        },
        fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 7,
            slow_empty_poor: 2,
            ..Default::default()
        }),
        ..SkinDrawState::default()
    };

    for ref_id in [71, 74, 76, 77, 78, 80, 85, 102, 110, 154, 410, 420, 426] {
        assert_eq!(skin_state_number(ref_id, &state), None, "ref {ref_id}");
    }
    assert_eq!(skin_state_number(30, &state), Some(0));
    assert_eq!(skin_state_number(33, &state), Some(0));
}

#[test]
fn select_course_exposes_score_numbers() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Course,
        select_in_library: true,
        ex_score: 1234,
        max_combo: 345,
        total_notes: 1000,
        select_total_notes: 1000,
        select_play_count: 42,
        select_clear_count: 31,
        select_bp: Some(12),
        select_cb: Some(8),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(71, &state), Some(1234));
    assert_eq!(skin_state_number(74, &state), Some(1000));
    assert_eq!(skin_state_number(75, &state), Some(345));
    assert_eq!(skin_state_number(76, &state), Some(12));
    assert_eq!(skin_state_number(77, &state), Some(42));
    assert_eq!(skin_state_number(78, &state), Some(31));
    assert_eq!(skin_state_number(425, &state), Some(8));
    assert_eq!(skin_state_number(427, &state), Some(8));
}

#[test]
fn select_panel_on_and_off_timers_follow_each_panel_state() {
    let state = SkinDrawState {
        select_option_panel: 2,
        select_option_panel_elapsed_ms: 75,
        select_option_panel_off_elapsed_ms: [Some(120), None, Some(340), None, None, None],
        ..SkinDrawState::default()
    };

    assert_eq!(skin_timer_elapsed_ms(Some(21), &state), None);
    assert_eq!(skin_timer_elapsed_ms(Some(22), &state), Some(75));
    assert_eq!(skin_timer_elapsed_ms(Some(23), &state), None);
    assert_eq!(skin_timer_elapsed_ms(Some(31), &state), Some(120));
    assert_eq!(skin_timer_elapsed_ms(Some(32), &state), None);
    assert_eq!(skin_timer_elapsed_ms(Some(33), &state), Some(340));
}

#[test]
fn rival_skin_properties_map_select_rival_best() {
    let state = SkinDrawState {
        rival_selected: true,
        rival_ex_score: Some(1500),
        rival_max_combo: Some(700),
        rival_bp: Some(12),
        rival_judge_counts: Some([900, 50, 7, 3, 3]),
        select_total_notes: 1000,
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(271, &state), Some(1500));
    assert_eq!(skin_state_number(275, &state), Some(700));
    assert_eq!(skin_state_number(276, &state), Some(12));
    assert_eq!(skin_state_number(280, &state), Some(900));
    assert_eq!(skin_state_number(281, &state), Some(50));
    assert_eq!(skin_state_number(282, &state), Some(7));
    assert_eq!(skin_state_number(283, &state), Some(3));
    assert_eq!(skin_state_number(284, &state), Some(3));
    assert_eq!(skin_state_number(285, &state), Some(90));
    assert_eq!(skin_state_number(286, &state), Some(5));
    assert_eq!(skin_state_number(287, &state), Some(0));
    assert!((skin_state_float_number(285, &state).unwrap() - 0.9).abs() < f32::EPSILON);
    assert!((skin_state_float_number(286, &state).unwrap() - 0.05).abs() < f32::EPSILON);
    assert!(!test_skin_op(624, &[], &state));
    assert!(test_skin_op(625, &[], &state));

    let total_only = SkinDrawState {
        rival_bp: Some(12),
        target_clear_index: Some(6),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(283, &total_only), Some(0));
    assert_eq!(skin_state_number(284, &total_only), Some(12));
    assert_eq!(skin_state_number(371, &total_only), Some(6));

    let selected_without_score = SkinDrawState { rival_selected: true, ..SkinDrawState::default() };
    assert!(!test_skin_op(624, &[], &selected_without_score));
    assert!(test_skin_op(625, &[], &selected_without_score));

    let score_without_selected =
        SkinDrawState { rival_ex_score: Some(1500), ..SkinDrawState::default() };
    assert!(test_skin_op(624, &[], &score_without_selected));
    assert!(!test_skin_op(625, &[], &score_without_selected));

    let no_rival = SkinDrawState::default();
    assert_eq!(skin_state_number(271, &no_rival), None);
    assert_eq!(skin_state_number(280, &no_rival), None);
    assert_eq!(skin_state_number(285, &no_rival), None);
    assert_eq!(skin_state_float_number(285, &no_rival), None);
    assert!(test_skin_op(624, &[], &no_rival));
    assert!(!test_skin_op(625, &[], &no_rival));
}

#[test]
fn select_compare_rival_option_stays_visible_without_chart_score() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 5,
            "w": 100,
            "h": 100,
            "source": [{ "id": 1, "path": "rival.png" }],
            "image": [
                { "id": "rival", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "rival-lose", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                { "id": "rival-clear", "src": 1, "x": 0, "y": 0, "w": 10, "h": 110, "divy": 11, "ref": 371 }
            ],
            "destination": [
                { "id": "rival", "op": [625], "dst": [{ "x": 10, "y": 20, "w": 10, "h": 10 }] },
                { "id": "rival-lose", "op": [353], "dst": [{ "x": 30, "y": 20, "w": 10, "h": 10 }] },
                { "id": "rival-clear", "op": [625], "dst": [{ "x": 50, "y": 20, "w": 10, "h": 10 }] }
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("1", 10.0, 110.0);

    let selected_without_score =
        SelectSnapshot { rival_selected: true, ..SelectSnapshot::default() };
    assert!(
        document
            .select_render_items(&sources, &selected_without_score)
            .iter()
            .any(|item| matches!(item, SkinRenderItem::Image { texture: SkinTextureId(9999), .. }))
    );

    let selected_with_score = SelectSnapshot {
        rows: vec![SelectRowSnapshot { ex_score: Some(1400), ..SelectRowSnapshot::default() }],
        rival: Some(crate::scene::SelectRivalSnapshot {
            display_name: "Rival".to_string(),
            numeric_score_available: true,
            ex_score: 1500,
            clear_index: 6,
            max_combo: 700,
            bp: 12,
            judge_counts: None,
        }),
        rival_selected: true,
        ..SelectSnapshot::default()
    };
    let score_items = document.select_render_items(&sources, &selected_with_score);
    assert_eq!(score_items.len(), 3);
    assert!(score_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
        rect: Rect { x, .. },
        uv: TextureRegion { y: v, .. },
        ..
    } if approx_eq(*x, 0.5) && approx_eq(*v, 60.0 / 110.0))));

    assert!(document.select_render_items(&sources, &SelectSnapshot::default()).is_empty());
}

#[test]
fn skin_state_number_maps_select_refs() {
    let state = SkinDrawState {
        select_folder_song_count: Some(42),
        select_screen: true,
        select_play_level: 12,
        select_clear_index: 5,
        select_total_notes: 1200,
        select_bpm: 148.0,
        select_chart_normal_notes: 900,
        select_chart_long_notes: 180,
        select_chart_scratch_notes: 100,
        select_chart_long_scratch_notes: 20,
        select_chart_density: 4.56,
        select_chart_peak_density: 12.34,
        select_chart_end_density: 7.89,
        select_chart_total_gauge: 200.0,
        select_chart_main_bpm: 150.0,
        select_min_bpm: 120.0,
        select_max_bpm: 180.0,
        select_length_ms: 183_000,
        hispeed: 2.75,
        total_duration_ms: 500,
        duration_green_ms: Some(300),
        select_master_volume: 0.575,
        select_key_volume: 0.59,
        select_bgm_volume: 0.28,
        select_mode_index: 4,
        select_sort_index: 6,
        select_ln_mode_index: 2,
        select_bp: Some(12),
        select_cb: Some(8),
        ex_score: 1234,
        max_combo: 345,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(11, &state), Some(4));
    assert_eq!(skin_state_number(12, &state), Some(6));
    assert_eq!(skin_state_number(300, &state), Some(42));
    assert_eq!(skin_state_number(96, &state), Some(12));
    assert_eq!(
        skin_state_number(
            96,
            &SkinDrawState { select_play_level: 12, play_level: 9, ..SkinDrawState::default() }
        ),
        Some(9)
    );
    assert_eq!(skin_state_number(370, &state), Some(5));
    assert_eq!(skin_state_number(74, &state), Some(1200));
    assert_eq!(skin_state_number(75, &state), Some(345));
    assert_eq!(skin_state_number(105, &state), Some(345));
    assert_eq!(skin_state_number(76, &state), Some(12));
    assert_eq!(skin_state_number(425, &state), Some(8));
    assert_eq!(skin_state_number(90, &state), Some(180));
    assert_eq!(skin_state_number(91, &state), Some(120));
    assert_eq!(skin_state_number(92, &state), Some(150));
    assert_eq!(skin_state_number(160, &state), Some(148));
    assert_eq!(skin_state_number(350, &state), Some(900));
    assert_eq!(skin_state_number(351, &state), Some(180));
    assert_eq!(skin_state_number(352, &state), Some(100));
    assert_eq!(skin_state_number(353, &state), Some(20));
    assert_eq!(skin_state_number(360, &state), Some(12));
    assert_eq!(skin_state_number(361, &state), Some(34));
    assert_eq!(skin_state_number(362, &state), Some(7));
    assert_eq!(skin_state_number(363, &state), Some(89));
    assert_eq!(skin_state_number(364, &state), Some(4));
    assert_eq!(skin_state_number(365, &state), Some(56));
    assert_eq!(skin_state_number(368, &state), Some(200));
    assert_eq!(skin_state_number(71, &state), Some(1234));
    assert_eq!(skin_state_number(1163, &state), Some(3));
    assert_eq!(skin_state_number(1164, &state), Some(3));
    assert_eq!(skin_state_number(310, &state), Some(2));
    assert_eq!(skin_state_number(311, &state), Some(75));
    assert_eq!(skin_state_number(312, &state), Some(500));
    assert_eq!(skin_state_number(313, &state), Some(300));
    assert_eq!(skin_state_number(57, &state), Some(57));
    assert_eq!(skin_state_number(58, &state), Some(59));
    assert_eq!(skin_state_number(59, &state), Some(28));
    assert_eq!(skin_state_number(308, &state), Some(2));

    assert!(skin_state_number(21, &state).is_some_and(|value| value >= 2026));
    assert!(skin_state_number(22, &state).is_some_and(|value| (1..=12).contains(&value)));
    assert!(skin_state_number(23, &state).is_some_and(|value| (1..=31).contains(&value)));
    assert!(skin_state_number(24, &state).is_some_and(|value| (0..=23).contains(&value)));
    assert!(skin_state_number(25, &state).is_some_and(|value| (0..=59).contains(&value)));
    assert!(skin_state_number(26, &state).is_some_and(|value| (0..=59).contains(&value)));
}

#[test]
fn clear_only_rival_hides_numeric_refs_but_preserves_the_lamp_and_real_zero_scores() {
    let document: SkinDocument = serde_json::from_str(r#"{ "type": 5 }"#).unwrap();
    for available in [false, true] {
        let snapshot = SelectSnapshot {
            rival: Some(crate::scene::SelectRivalSnapshot {
                display_name: "Past self".into(),
                numeric_score_available: available,
                ex_score: 0,
                clear_index: 2,
                max_combo: 0,
                bp: 0,
                judge_counts: Some(crate::scene::SelectRivalJudgeCounts {
                    pgreat: 0,
                    great: 0,
                    good: 0,
                    bad: 0,
                    poor: 0,
                }),
            }),
            rival_selected: true,
            ..Default::default()
        };
        let (state, _) = document.select_draw_state(&snapshot, None);
        for reference in [271, 275, 276] {
            assert_eq!(skin_state_number(reference, &state), available.then_some(0));
        }
        assert_eq!(state.target_ex_score, available.then_some(0));
        assert_eq!(state.rival_judge_counts, available.then_some([0; 5]));
        assert_eq!(state.rival_clear_index, Some(2));
        assert_eq!(state.target_clear_index, Some(2));
        assert!(state.rival_selected);
    }
}

#[test]
fn select_mode_index_matches_beatoraja_skin_ref_order() {
    let cases = [
        ("ALL", 0),
        ("5K", 1),
        ("7K", 2),
        ("10K", 3),
        ("14K", 4),
        ("9K", 5),
        ("24K", 6),
        ("24K_DOUBLE", 7),
        ("unknown", 0),
    ];

    for (mode, expected) in cases {
        assert_eq!(select_mode_index(mode), expected, "select mode {mode}");
    }
}

#[test]
fn select_difficulty_filter_exposes_beatoraja_event_and_number() {
    let state = SkinDrawState {
        select_screen: true,
        select_difficulty_filter_index: 4,
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_event_index(309, &state), 4);
    assert_eq!(skin_state_number(221, &state), Some(4));
}

#[test]
fn select_folder_hides_chart_bpm_and_judge_rank() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        select_in_library: true,
        select_bpm: 0.0,
        select_min_bpm: 0.0,
        select_max_bpm: 0.0,
        judge_rank: None,
        total_duration_ms: 500,
        duration_green_ms: Some(300),
        ..SkinDrawState::default()
    };

    assert_eq!(skin_state_number(90, &state), None);
    assert_eq!(skin_state_number(91, &state), None);
    assert_eq!(skin_state_number(92, &state), None);
    assert_eq!(skin_state_number(160, &state), None);
    for ref_id in [350, 351, 352, 353, 360, 362, 364, 368, 1163, 1164] {
        assert_eq!(skin_state_number(ref_id, &state), None, "chart detail ref {ref_id}");
    }
    assert_eq!(skin_state_number(312, &state), Some(500));
    assert_eq!(skin_state_number(313, &state), Some(300));
    for op in 180..=184 {
        assert!(!test_skin_op(op, &[], &state), "judge rank option {op}");
    }
}
