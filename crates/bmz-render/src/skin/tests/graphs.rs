use super::*;

#[test]
fn play_judgegraph_density_uses_canvas_pixel_gap() {
    let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "judgegraph": [{ "id": "density" }],
                "destination": [{ "id": "density", "dst": [{ "x": 10, "y": 10, "w": 40, "h": 10 }] }]
            }
            "#,
        )
        .unwrap();
    document.play_judge_graph_density = vec![1, 2, 3];

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );
    let rects: Vec<Rect> = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();

    assert_eq!(rects.len(), 3);
    for rect in rects {
        assert!(rect.x >= 0.10);
        assert!(
            rect.x + rect.width <= 0.50 + 0.0001,
            "play judgegraph bar should stay inside the destination: {rect:?}",
        );
    }
}

#[test]
fn skin_document_evaluates_safe_gauge_draw_conditions() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "draw": "gauge() >= 75", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "gauge() >= 50 and gauge() < 75", "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "gauge() < 25", "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "unknown() > 0", "dst": [{ "x": 30, "y": 0, "w": 10, "h": 10 }] }
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
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    let high = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 0, gauge: 80.0, ..SkinDrawState::default() },
    );
    let middle = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 0, gauge: 60.0, ..SkinDrawState::default() },
    );
    let low = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 0, gauge: 10.0, ..SkinDrawState::default() },
    );

    assert_eq!(high.len(), 1);
    assert_eq!(middle.len(), 1);
    assert_eq!(low.len(), 1);
    assert!(
        matches!(high[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.0))
    );
    assert!(
        matches!(middle[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.1))
    );
    assert!(
        matches!(low[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.2))
    );
}

#[test]
fn skin_document_evaluates_gauge_type_draw_conditions() {
    assert!(eval_skin_draw_condition(
        "gauge_type() == 4 or gauge_type() == 5",
        &SkinDrawState { gauge_type: 4, ..Default::default() }
    ));
    assert!(eval_skin_draw_condition(
        "gauge_type() == 4 or gauge_type() == 5",
        &SkinDrawState { gauge_type: 5, ..Default::default() }
    ));
    assert!(!eval_skin_draw_condition(
        "gauge_type() == 4 or gauge_type() == 5",
        &SkinDrawState { gauge_type: 2, ..Default::default() }
    ));
}

#[test]
fn peaceful_gauge_lead_glow_uses_group_part_border_and_profile() {
    let pms = SkinDrawState { gauge: 60.0, gauge_max: 120.0, gauge_type: 2, ..Default::default() };
    assert!(eval_skin_draw_condition("gauge_lead_glow(groove,12,below)", &pms));
    assert!(!eval_skin_draw_condition("gauge_lead_glow(groove,12,above)", &pms));
    assert!(!eval_skin_draw_condition("gauge_lead_glow(easy,12,below)", &pms));

    let sevenkeys =
        SkinDrawState { gauge: 80.0, gauge_max: 100.0, gauge_type: 2, ..Default::default() };
    assert!(eval_skin_draw_condition("gauge_lead_glow(groove,19,below)", &sevenkeys));
    assert!(!eval_skin_draw_condition("gauge_lead_glow(groove,19,above)", &sevenkeys));

    let class =
        SkinDrawState { gauge: 50.0, gauge_max: 100.0, gauge_type: 6, ..Default::default() };
    assert!(eval_skin_draw_condition("gauge_lead_glow(hard,12,above)", &class));
}

#[test]
fn skin_document_evaluates_gauge_auto_shift_draw_conditions() {
    assert!(eval_skin_draw_condition(
        "gauge_auto_shift() == 1",
        &SkinDrawState { gauge_auto_shift: true, ..Default::default() }
    ));
    assert!(!eval_skin_draw_condition(
        "gauge_auto_shift() == 1",
        &SkinDrawState { gauge_auto_shift: false, ..Default::default() }
    ));
    assert_eq!(select_gauge_auto_shift_index("BEST CLEAR"), 3);
    assert_eq!(select_bottom_shiftable_gauge_index("NORMAL"), 2);
    assert_eq!(
        skin_state_imageset_index(
            78,
            &SkinDrawState { select_gauge_auto_shift_index: 3, ..Default::default() }
        ),
        Some(3)
    );
    assert_eq!(
        skin_state_imageset_index(
            341,
            &SkinDrawState { select_bottom_shiftable_gauge_index: 2, ..Default::default() }
        ),
        Some(2)
    );
}

#[test]
fn static_render_items_resolve_exhard_gauge_additive_overlay() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [{ "id": "gauge-node", "src": 1, "x": 0, "y": 0, "w": 5, "h": 10 }],
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 2 },
                "destination": [
                    {
                        "id": "gauge",
                        "loop": 1200,
                        "draw": "gauge_type() == 4 or gauge_type() == 5",
                        "blend": 2,
                        "offset": 11,
                        "dst": [
                            { "time": 1200, "x": 54, "y": 151, "w": 450, "h": 28, "a": 0 },
                            { "time": 1700, "a": 80 },
                            { "time": 2000, "a": 0 }
                        ]
                    }
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
    let mut skin_offsets = SkinOffsetValues::default();
    skin_offsets
        .set(11, crate::skin_offset::SkinOffsetValue { x: 10, y: 8, w: 4, h: 6, r: 0, a: 0 });
    let (behind, front, _) = document.static_render_items_split(
        &sources,
        &SkinDrawState { gauge_type: 4, elapsed_ms: 1700, skin_offsets, ..Default::default() },
        &SkinTextState::default(),
    );
    let items = behind.into_iter().chain(front).collect::<Vec<_>>();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| matches!(
        item,
        SkinRenderItem::Image {
            tint: Color { a, .. },
            blend: BlendMode::Add,
            ..
        } if (*a - 80.0 / 255.0).abs() < 0.01
    )));
    assert!(matches!(
        items[0],
        SkinRenderItem::Image {
            rect: Rect { x, y, width, height },
            ..
        } if approx_eq(x, 62.0 / 1920.0)
            && approx_eq(y, 890.0 / 1080.0)
            && approx_eq(width, 227.0 / 1920.0)
            && approx_eq(height, 34.0 / 1080.0)
    ));
}

#[test]
fn skin_document_resolves_gauge_nodes_into_parts() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [{ "id": "gauge-node", "src": 1, "x": 10, "y": 0, "w": 5, "h": 10 }],
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 4, "type": 0 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 80, "y": 10, "w": -40, "h": 10 }] }
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

    let items = document.gauge_render_items(50.0, 0, &sources).unwrap();

    assert_eq!(items.len(), 4);
    assert!(items.iter().all(|item| matches!(item, SkinRenderItem::Image { .. })));
    assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, 0.7)
                && approx_eq(y, 0.8)
                && approx_eq(width, 0.1)
                && approx_eq(height, 0.1)));
}

#[test]
fn skin_gauge_expands_compact_node_layouts_like_beatoraja() {
    let compact_to_logical: &[(usize, &[&[usize]])] = &[
        (
            4,
            &[
                &[0, 4, 6, 10, 12, 16, 18, 22, 24, 28, 30, 34],
                &[1, 5, 7, 11, 13, 17, 19, 23, 25, 29, 31, 35],
                &[2, 8, 14, 20, 26, 32],
                &[3, 9, 15, 21, 27, 33],
            ],
        ),
        (
            8,
            &[
                &[12, 16, 18, 22],
                &[13, 17, 19, 23],
                &[14, 20],
                &[15, 21],
                &[0, 4, 6, 10, 24, 28, 30, 34],
                &[1, 5, 7, 11, 25, 29, 31, 35],
                &[2, 8, 26, 32],
                &[3, 9, 27, 33],
            ],
        ),
        (
            12,
            &[
                &[12, 18],
                &[13, 19],
                &[14, 20],
                &[15, 21],
                &[0, 6, 24, 30],
                &[1, 7, 25, 31],
                &[2, 8, 26, 32],
                &[3, 9, 27, 33],
                &[16, 22],
                &[17, 23],
                &[4, 10, 28, 34],
                &[5, 11, 29, 35],
            ],
        ),
    ];

    for &(node_count, groups) in compact_to_logical {
        let mut expected = [usize::MAX; 36];
        for (source_index, logical_indices) in groups.iter().enumerate() {
            for &logical_index in *logical_indices {
                expected[logical_index] = source_index;
            }
        }
        for (logical_index, expected_source) in expected.into_iter().enumerate() {
            assert_eq!(
                skin_gauge_source_node_index(logical_index, node_count),
                expected_source,
                "node_count={node_count}, logical_index={logical_index}"
            );
        }
    }

    for logical_index in 0..36 {
        assert_eq!(skin_gauge_source_node_index(logical_index, 36), logical_index);
    }
}

#[test]
fn skin_gauge_flickering_draws_normal_tip_overlay() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [],
                "gauge": { "id": "gauge", "nodes": [], "parts": 4, "type": 3, "cycle": 33 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }
                ]
            }
            "#,
    )
    .unwrap();
    document.gauge.as_mut().unwrap().nodes = (0..36).map(|index| format!("node-{index}")).collect();
    document.image = (0..36)
        .map(|index| SkinImageDef {
            lr2_panel: None,
            id: format!("node-{index}"),
            src: "1".to_string(),
            x: index,
            y: 0,
            w: 1,
            h: 1,
            divx: 1,
            divy: 1,
            timer: None,
            cycle: 0,
            len: 0,
            ref_id: 0,
            click: 0,
            act: None,
            clickable: None,
        })
        .collect();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 36.0, height: 1.0 },
        },
    )]);
    let items = document
        .static_image_render_items(
            &sources,
            &SkinDrawState {
                elapsed_ms: 8,
                gauge: 75.0,
                gauge_max: 100.0,
                gauge_border: 1.0,
                gauge_type: 2,
                ..Default::default()
            },
        )
        .into_iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { .. } => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 5, "4 parts + flickering tip overlay");
    let flicker = items.iter().find(|item| {
        matches!(
            item,
            SkinRenderItem::Image {
                blend: BlendMode::Normal,
                tint: Color { a, .. },
                ..
            } if *a > 0.2
        )
    });
    assert!(flicker.is_some(), "expected normal-blend tip overlay with alpha fade");
}

#[test]
fn skin_gauge_defaults_to_random_when_type_omitted() {
    let document: SkinDocument =
        serde_json::from_str(r#"{"type":0,"w":100,"h":100,"gauge":{"id":"g","nodes":[]}}"#)
            .unwrap();
    assert_eq!(document.gauge.as_ref().unwrap().gauge_type, 0);
}

#[test]
fn skin_gauge_random_animation_uses_latched_runtime_value() {
    let gauge = SkinGaugeDef {
        id: "g".to_string(),
        nodes: Vec::new(),
        parts: 4,
        gauge_type: 0,
        range: 3,
        cycle: 33,
        starttime: 0,
        endtime: 500,
    };
    let mut state = SkinDrawState::default();
    state.gauge_random_indices.insert("g".into(), 3);
    assert_eq!(skin_gauge_animation_index(&gauge, &state), 3);
    state.elapsed_ms = 100;
    assert_eq!(skin_gauge_animation_index(&gauge, &state), 3);
    state.gauge_random_indices.insert("g".into(), 1);
    assert_eq!(skin_gauge_animation_index(&gauge, &state), 1);
}

#[test]
fn skin_gauge_random_runtime_populates_draw_state_and_resets_with_scene() {
    let document: SkinDocument =
        serde_json::from_str(r#"{"gauge":{"id":"g","nodes":[],"type":0,"range":3,"cycle":33}}"#)
            .unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();
    runtime.advance(&document, &mut state, 0);
    let initial = state.gauge_random_indices["g"];
    assert!((0..=3).contains(&initial));
    assert_eq!(skin_gauge_animation_index(document.gauge.as_ref().unwrap(), &state), initial);
    state.gauge_random_indices.clear();
    state.elapsed_ms = 20;
    runtime.advance(&document, &mut state, 20);
    assert_eq!(state.gauge_random_indices["g"], initial);

    runtime.reset_for_document(None);
    let empty: SkinDocument = serde_json::from_str("{}").unwrap();
    runtime.advance(&empty, &mut state, 20);
    assert!(state.gauge_random_indices.is_empty());
}

#[test]
fn skin_gauge_decrease_animation_advances_forward() {
    let gauge = SkinGaugeDef {
        id: "g".to_string(),
        nodes: Vec::new(),
        parts: 4,
        gauge_type: 2,
        range: 3,
        cycle: 33,
        starttime: 0,
        endtime: 500,
    };

    assert_eq!(
        skin_gauge_animation_index(&gauge, &SkinDrawState { elapsed_ms: 33, ..Default::default() }),
        1
    );
    assert_eq!(
        skin_gauge_animation_index(&gauge, &SkinDrawState { elapsed_ms: 66, ..Default::default() }),
        2
    );
}

#[test]
fn skin_gauge_notes_count_truncates_toward_zero() {
    assert_eq!(skin_gauge_notes_count(74.9, 4, 100.0), 2);
    assert_eq!(skin_gauge_notes_count(75.0, 4, 100.0), 3);
    assert_eq!(skin_gauge_notes_count(0.0, 4, 100.0), 0);
}

#[test]
fn skin_gauge_omitted_type_has_no_flickering_overlay() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [],
                "gauge": { "id": "gauge", "nodes": [], "parts": 4 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }
                ]
            }
            "#,
    )
    .unwrap();
    document.gauge.as_mut().unwrap().nodes = (0..36).map(|index| format!("node-{index}")).collect();
    document.image = (0..36)
        .map(|index| SkinImageDef {
            lr2_panel: None,
            id: format!("node-{index}"),
            src: "1".to_string(),
            x: index,
            y: 0,
            w: 1,
            h: 1,
            divx: 1,
            divy: 1,
            timer: None,
            cycle: 0,
            len: 0,
            ref_id: 0,
            click: 0,
            act: None,
            clickable: None,
        })
        .collect();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 36.0, height: 1.0 },
        },
    )]);
    let items = document
        .static_image_render_items(
            &sources,
            &SkinDrawState {
                elapsed_ms: 8,
                gauge: 75.0,
                gauge_max: 100.0,
                gauge_border: 1.0,
                gauge_type: 2,
                ..Default::default()
            },
        )
        .into_iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { .. } => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 4, "type=0 should not add flickering tip overlay");
}
