use super::*;

#[test]
fn hln_skin_refs_expose_effective_mode_and_canonical_score_policy() {
    let state = SkinDrawState {
        ln_score_policy_index: Some(6),
        result_ln_mode_index: Some(3),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(308, &state), Some(3));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_LN_SCORE_POLICY, &state), Some(6));
    assert_eq!(
        skin_main_state_text(SKIN_REF_BMZ_LN_SCORE_POLICY, Some(&state), &SkinTextState::default()),
        "FORCE(HLN)"
    );
    assert!(test_skin_op(SKIN_OPTION_BMZ_LN_SCORE_POLICY_FORCE, &[], &state));
}

#[test]
fn skin_context_updates_user_selected_options() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "property": [
                    { "name": "Side", "def": "1P", "item": [
                        { "name": "1P", "op": 920 },
                        { "name": "2P", "op": 921 }
                    ]}
                ],
                "source": [{ "id": "src", "path": "option.png" }],
                "image": [
                    { "id": "one", "src": "src", "w": 10, "h": 10 },
                    { "id": "two", "src": "src", "w": 10, "h": 10 }
                ],
                "destination": [
                    { "if": 920, "values": [{
                        "id": "one", "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }]
                    }]},
                    { "if": 921, "values": [{
                        "id": "two", "dst": [{ "x": 50, "y": 0, "w": 10, "h": 10 }]
                    }]}
                ]
            }
            "#,
    )
    .unwrap();
    let source = SkinDocumentTexture {
        source_id: "src".to_string(),
        texture: SkinTextureId(42),
        source_size: SkinImageSize { width: 10.0, height: 10.0 },
    };
    let mut context =
        SkinContext::from_manifest_and_document(default_skin_manifest(), document, [source]);

    assert_eq!(context.document().unwrap().enabled_options(), [920]);
    let original = context.clone();
    let one = context.select_document_items(&SelectSnapshot::default());
    assert!(
        matches!(
            one.as_slice(),
            [SkinRenderItem::Image { rect, .. }] if approx_eq(rect.x, 0.1)
        ),
        "{one:?}"
    );

    assert!(context.set_user_selected_options(vec![921]));
    assert_eq!(context.document().unwrap().enabled_options(), [921]);
    let two = context.select_document_items(&SelectSnapshot::default());
    assert!(matches!(
        two.as_slice(),
        [SkinRenderItem::Image { rect, .. }] if approx_eq(rect.x, 0.5)
    ));

    let original_one = original.select_document_items(&SelectSnapshot::default());
    assert!(matches!(
        original_one.as_slice(),
        [SkinRenderItem::Image { rect, .. }] if approx_eq(rect.x, 0.1)
    ));
}

#[test]
fn hln_parts_fall_back_independently_to_hcn_then_ln() {
    let mut document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 0,
        "image": [
            {"id":"ln","src":1,"x":10,"y":0,"w":10,"h":1},
            {"id":"hcn","src":1,"x":20,"y":0,"w":10,"h":1},
            {"id":"hln","src":1,"x":30,"y":0,"w":10,"h":1}
        ],
        "note": {"id":"notes", "note":["ln"], "lnbodyActive":["ln"],
            "hcnbodyActive":["hcn"], "hlnbodyMiss":["hln"]}
    }))
    .unwrap();
    let sources = HashMap::from([(
        "1".into(),
        SkinDocumentTexture {
            source_id: "1".into(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 100.0, height: 50.0 },
        },
    )]);
    let render_x = |doc: &SkinDocument, state| {
        let item = doc
            .note_long_body_render_item(
                Lane::Key1,
                KeyMode::K7,
                Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                LongNoteMode::Hln,
                state,
                &SkinDrawState::default(),
                &sources,
            )
            .unwrap();
        match item {
            SkinRenderItem::Image { uv, .. } => uv.x,
            _ => panic!("image expected"),
        }
    };
    assert!(approx_eq(render_x(&document, LongBodyState::HcnDamage), 0.3));
    assert!(approx_eq(render_x(&document, LongBodyState::Processing), 0.2));
    document.note.as_mut().unwrap().hcnbody_active.clear();
    assert!(approx_eq(render_x(&document, LongBodyState::Processing), 0.1));
    document.note.as_mut().unwrap().hlnbody_active = vec![String::new()];
    assert!(approx_eq(render_x(&document, LongBodyState::Processing), 0.1));
}

#[test]
fn skin_document_selects_hcn_body_by_state() {
    // 旧形式 HCN: [6]=hcnbody(processing) [7]=hcnactive(inactive)
    // [8]=hcndamage(回復中) [9]=hcnreactive(減衰中)
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "source": [{ "id": 1, "path": "notes.png" }],
                "image": [
                    { "id": "hb", "src": 1, "x": 10, "y": 0, "w": 10, "h": 1 },
                    { "id": "ha", "src": 1, "x": 20, "y": 0, "w": 10, "h": 1 },
                    { "id": "hd", "src": 1, "x": 30, "y": 0, "w": 10, "h": 1 },
                    { "id": "hr", "src": 1, "x": 40, "y": 0, "w": 10, "h": 1 }
                ],
                "note": {
                    "id": "notes",
                    "note": ["hb", "hb", "hb", "hb", "hb", "hb", "hb", "hb"],
                    "hcnbody": ["hb", "hb", "hb", "hb", "hb", "hb", "hb", "hb"],
                    "hcnactive": ["ha", "ha", "ha", "ha", "ha", "ha", "ha", "ha"],
                    "hcndamage": ["hd", "hd", "hd", "hd", "hd", "hd", "hd", "hd"],
                    "hcnreactive": ["hr", "hr", "hr", "hr", "hr", "hr", "hr", "hr"]
                }
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 100.0, height: 50.0 },
        },
    )]);
    let rect = Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 };
    let render_x = |state: LongBodyState| {
        let item = document
            .note_long_body_render_item(
                Lane::Scratch,
                KeyMode::K7,
                rect,
                LongNoteMode::Hcn,
                state,
                &SkinDrawState::default(),
                &sources,
            )
            .unwrap();
        match item {
            SkinRenderItem::Image { uv: TextureRegion { x, .. }, .. } => x,
            _ => panic!("expected image item"),
        }
    };

    assert!(approx_eq(render_x(LongBodyState::Processing), 0.1)); // hcnbody
    assert!(approx_eq(render_x(LongBodyState::Inactive), 0.2)); // hcnactive
    assert!(approx_eq(render_x(LongBodyState::HcnActive), 0.3)); // hcndamage
    assert!(approx_eq(render_x(LongBodyState::HcnDamage), 0.4)); // hcnreactive
}

#[test]
fn skin_gauge_sprite_selects_exhard_nodes_and_tip_frame() {
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
                elapsed_ms: 1_000,
                gauge: 75.0,
                gauge_max: 100.0,
                gauge_border: 1.0,
                gauge_type: 4,
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
    let tip_flicker = items.iter().find_map(|item| match item {
        SkinRenderItem::Image { uv, blend: BlendMode::Normal, .. } if uv.x > 0.7 => Some(uv.x),
        _ => None,
    });
    assert!(
        tip_flicker.is_some(),
        "EX-HARD flickering tip should use node index 28+ (normal blend overlay)"
    );
}

#[test]
fn select_songlist_uses_player_and_rival_lamps_while_rival_is_selected() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 5,
            "w": 100,
            "h": 100,
            "source": [{ "id": 1, "path": "lamp.png" }],
            "image": [
                { "id": "bar", "src": 1, "x": 0, "y": 0, "w": 4, "h": 4 },
                { "id": "lamp-none", "src": 1, "x": 0, "y": 0, "w": 4, "h": 4 },
                { "id": "lamp-failed", "src": 1, "x": 4, "y": 0, "w": 4, "h": 4 },
                { "id": "lamp-assist", "src": 1, "x": 8, "y": 0, "w": 4, "h": 4 },
                { "id": "lamp-light-assist", "src": 1, "x": 12, "y": 0, "w": 4, "h": 4 },
                { "id": "lamp-easy", "src": 1, "x": 16, "y": 0, "w": 4, "h": 4 },
                { "id": "lamp-normal", "src": 1, "x": 20, "y": 0, "w": 4, "h": 4 }
            ],
            "songlist": {
                "id": "songlist",
                "center": 0,
                "liston": [{ "id": "bar", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 10 }] }],
                "lamp": [
                    { "id": "lamp-none", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-failed", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-light-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-easy", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-normal", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] }
                ],
                "playerlamp": [
                    { "id": "lamp-none", "dst": [{ "x": 10, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-failed", "dst": [{ "x": 10, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-assist", "dst": [{ "x": 10, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-light-assist", "dst": [{ "x": 10, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-easy", "dst": [{ "x": 10, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-normal", "dst": [{ "x": 10, "y": 1, "w": 4, "h": 4 }] }
                ],
                "rivallamp": [
                    { "id": "lamp-none", "dst": [{ "x": 20, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-failed", "dst": [{ "x": 20, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-assist", "dst": [{ "x": 20, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-light-assist", "dst": [{ "x": 20, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-easy", "dst": [{ "x": 20, "y": 1, "w": 4, "h": 4 }] },
                    { "id": "lamp-normal", "dst": [{ "x": 20, "y": 1, "w": 4, "h": 4 }] }
                ]
            },
            "destination": [{ "id": "songlist" }]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("1", 24.0, 4.0);
    let row = SelectRowSnapshot {
        clear_type: "Normal".to_string(),
        rival_clear_index: 4,
        kind: SelectRowKind::Song,
        ..SelectRowSnapshot::default()
    };

    let rival_items = document.select_render_items(
        &sources,
        &SelectSnapshot {
            rows: vec![row.clone()],
            rival_selected: true,
            ..SelectSnapshot::default()
        },
    );
    assert!(rival_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
        rect: Rect { x, .. },
        uv: TextureRegion { x: u, .. },
        ..
    } if approx_eq(*x, 0.2) && approx_eq(*u, 20.0 / 24.0))));
    assert!(rival_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
        rect: Rect { x, .. },
        uv: TextureRegion { x: u, .. },
        ..
    } if approx_eq(*x, 0.3) && approx_eq(*u, 16.0 / 24.0))));
    assert!(!rival_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
        rect: Rect { x, .. },
        ..
    } if approx_eq(*x, 0.11))));

    let solo_items = document.select_render_items(
        &sources,
        &SelectSnapshot { rows: vec![row], ..SelectSnapshot::default() },
    );
    assert!(solo_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
        rect: Rect { x, .. },
        uv: TextureRegion { x: u, .. },
        ..
    } if approx_eq(*x, 0.11) && approx_eq(*u, 20.0 / 24.0))));
}

#[test]
fn select_skin_document_renders_songlist_rows() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [
                    { "id": 1, "path": "bar.png" },
                    { "id": 2, "path": "num.png" },
                    { "id": 3, "path": "lamp.png" },
                    { "id": 4, "path": "graph.png" }
                ],
                "image": [
                    { "id": "bar-song", "src": 1, "x": 0, "y": 0, "w": 40, "h": 10 },
                    { "id": "bar-folder", "src": 1, "x": 0, "y": 10, "w": 40, "h": 10 },
                    { "id": "bar-table", "src": 1, "x": 0, "y": 30, "w": 40, "h": 10 },
                    { "id": "song-op-marker", "src": 1, "x": 0, "y": 20, "w": 4, "h": 4 },
                    { "id": "folder-op-marker", "src": 1, "x": 4, "y": 20, "w": 4, "h": 4 },
                    { "id": "trophy-bronze", "src": 3, "x": 0, "y": 0, "w": 4, "h": 4 },
                    { "id": "trophy-silver", "src": 3, "x": 4, "y": 0, "w": 4, "h": 4 },
                    { "id": "trophy-gold", "src": 3, "x": 8, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-none", "src": 3, "x": 0, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-failed", "src": 3, "x": 4, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-assist", "src": 3, "x": 8, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-light-assist", "src": 3, "x": 12, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-easy", "src": 3, "x": 16, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-normal", "src": 3, "x": 20, "y": 0, "w": 4, "h": 4 },
                    { "id": "label-ln", "src": 1, "x": 0, "y": 40, "w": 4, "h": 4 },
                    { "id": "label-random", "src": 1, "x": 4, "y": 40, "w": 4, "h": 4 },
                    { "id": "label-mine", "src": 1, "x": 8, "y": 40, "w": 4, "h": 4 }
                ],
                "imageset": [{ "id": "bar", "images": ["bar-song", "bar-folder", "bar-table"] }],
                "text": [
                    { "id": "bartext", "font": "main", "size": 10 },
                    { "id": "bartext1", "font": "folder", "size": 10 },
                    { "id": "bartext2", "font": "table", "size": 10 },
                    { "id": "bartext3", "font": "main", "size": 10 },
                    { "id": "bartext4", "font": "folder", "size": 10 }
                ],
                "value": [
                    { "id": "level-other", "src": 2, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 2 },
                    { "id": "level-beginner", "src": 2, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 2 },
                    { "id": "level-normal", "src": 2, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 2 }
                ],
                "graph": [{ "id": "graph-lamp", "src": 4, "x": 0, "y": 0, "w": 44, "h": 4, "divx": 11, "angle": 0, "type": -1 }],
                "songlist": {
                    "id": "songlist",
                    "center": 1,
                    "listoff": [
                        { "id": "bar", "dst": [{ "x": 10, "y": 70, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 10, "y": 30, "w": 40, "h": 10 }] }
                    ],
                    "liston": [
                        { "id": "bar", "dst": [{ "x": 12, "y": 70, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 12, "y": 50, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 12, "y": 30, "w": 40, "h": 10 }] }
                    ],
                    "text": [
                        { "id": "bartext", "dst": [{ "x": 1, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext", "dst": [{ "x": 2, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext", "dst": [{ "x": 5, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext", "dst": [{ "x": 6, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext4", "dst": [{ "x": 7, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext4", "dst": [{ "x": 8, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext2", "dst": [{ "x": 9, "y": 2, "w": 20, "h": 8 }] }
                    ],
                    "judgegraph": [
                        { "id": "song-op-marker", "op": [2], "dst": [{ "x": 8, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "folder-op-marker", "op": [1], "dst": [{ "x": 12, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "level": [
                        { "id": "level-other", "dst": [{ "x": 30, "y": 2, "w": 5, "h": 8 }] },
                        { "id": "level-beginner", "dst": [{ "x": 30, "y": 2, "w": 5, "h": 8 }] },
                        { "id": "level-normal", "dst": [{ "x": 30, "y": 2, "w": 5, "h": 8 }] }
                    ],
                    "trophy": [
                        { "id": "trophy-bronze", "dst": [{ "x": 35, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "trophy-silver", "dst": [{ "x": 35, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "trophy-gold", "dst": [{ "x": 35, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "label": [
                        { "id": "label-ln", "dst": [{ "x": 40, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "label-random", "dst": [{ "x": 44, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "label-mine", "dst": [{ "x": 48, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "graph": { "id": "graph-lamp", "dst": [{ "x": 5, "y": 1, "w": 20, "h": 2 }] },
                    "lamp": [
                        { "id": "lamp-none", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-failed", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-light-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-easy", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-normal", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "playerlamp": [
                        { "id": "lamp-none", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-failed", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-assist", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-light-assist", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-easy", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-normal", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] }
                    ]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
    let mut sources = mock_source("1", 100.0, 100.0);
    sources.extend(mock_source("2", 100.0, 100.0));
    sources.extend(mock_source("3", 24.0, 4.0));
    sources.extend(mock_source("4", 44.0, 4.0));
    let snapshot = SelectSnapshot {
        selected_index: 2,
        rows: vec![
            SelectRowSnapshot {
                index: 1,
                title: "Folder".to_string(),
                play_level: "0".to_string(),
                clear_type: "Normal".to_string(),
                folder_lamp_counts: {
                    let mut counts = [0; 11];
                    counts[5] = 1;
                    counts[6] = 1;
                    counts
                },
                is_folder: true,
                kind: SelectRowKind::Folder,
                ..SelectRowSnapshot::default()
            },
            SelectRowSnapshot {
                index: 2,
                title: "Song".to_string(),
                difficulty_name: "2".to_string(),
                play_level: "12".to_string(),
                clear_type: "Normal".to_string(),
                total_notes: 100,
                ex_score: Some(180),
                has_long_notes: true,
                has_mines: true,
                ..SelectRowSnapshot::default()
            },
            SelectRowSnapshot {
                index: 3,
                title: "Table".to_string(),
                play_level: "0".to_string(),
                is_folder: true,
                kind: SelectRowKind::TableFolder,
                ..SelectRowSnapshot::default()
            },
        ],
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&sources, &snapshot);

    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image { .. })));
    assert!(
        items
            .iter()
            .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Song"))
    );
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Text {
                origin: Point { x, y },
                text,
                style,
                ..
            } if text == "Folder"
                && style.font_id.as_deref() == Some("folder")
                && approx_eq(*x, 0.17)
                && approx_eq(*y, 0.2))));
    assert_eq!(
        items
            .iter()
            .filter(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Folder"))
            .count(),
        1
    );
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Text {
                text,
                style,
                ..
            } if text == "Table"
                && style.font_id.as_deref() == Some("table"))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                uv: TextureRegion { y: v, .. },
                ..
            } if approx_eq(*v, 30.0 / 100.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.13)
                && approx_eq(*y, 0.45)
                && approx_eq(*width, 0.04)
                && approx_eq(*height, 0.04)
                && approx_eq(*u, 20.0 / 24.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.11)
                && approx_eq(*y, 0.25)
                && approx_eq(*width, 0.04)
                && approx_eq(*height, 0.04)
                && approx_eq(*u, 20.0 / 24.0))));
    assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(*x, 0.72)
                && approx_eq(*y, 0.45)
                && approx_eq(*width, 0.04)
                && approx_eq(*height, 0.04))));
    assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 24.0))));
    let course_snapshot = SelectSnapshot {
        selected_index: 4,
        rows: vec![SelectRowSnapshot {
            index: 4,
            title: "Course".to_string(),
            kind: SelectRowKind::Course,
            difficulty_name: "2".to_string(),
            play_level: "12".to_string(),
            total_notes: 100,
            ex_score: Some(200),
            achieved_trophy_names: vec!["goldmedal".to_string()],
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };
    let course_items = document.select_render_items(&sources, &course_snapshot);
    assert!(course_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 24.0))));
    assert!(!course_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.2)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 20.0 / 100.0))));
    assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, .. },
                uv: TextureRegion { width: u_width, .. },
                ..
            } if approx_eq(*x, 0.17)
                && approx_eq(*y, 0.47)
                && approx_eq(*width, 0.1)
                && approx_eq(*u_width, 0.5))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, .. },
                uv: TextureRegion { x: u, width: u_width, .. },
                ..
            } if approx_eq(*x, 0.15)
                && approx_eq(*y, 0.27)
                && approx_eq(*width, 0.1)
                && approx_eq(*u, 24.0 / 44.0)
                && approx_eq(*u_width, 4.0 / 44.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, .. },
                uv: TextureRegion { x: u, width: u_width, .. },
                ..
            } if approx_eq(*x, 0.25)
                && approx_eq(*y, 0.27)
                && approx_eq(*width, 0.1)
                && approx_eq(*u, 20.0 / 44.0)
                && approx_eq(*u_width, 4.0 / 44.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { y: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.4)
                && approx_eq(*u, 0.2))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.2)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 20.0 / 100.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.52)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 40.0 / 100.0))));
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.60)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 100.0)
                && approx_eq(*v, 40.0 / 100.0))));
    let scrolling_snapshot =
        SelectSnapshot { bar_scroll_direction: 1, bar_scroll_progress: 0.5, ..snapshot.clone() };
    let scrolling_items = document.select_render_items(&sources, &scrolling_snapshot);
    assert!(scrolling_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.11)
                && approx_eq(*y, 0.5)
                && approx_eq(*width, 0.4)
                && approx_eq(*height, 0.1)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 0.0))));
    assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.22)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 4.0 / 100.0)
                && approx_eq(*v, 20.0 / 100.0))));

    let folder_selected = SelectSnapshot { selected_index: 1, ..snapshot };
    let items = document.select_render_items(&sources, &folder_selected);
    assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.18)
                && approx_eq(*y, 0.65)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 20.0 / 100.0))));
    assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.22)
                && approx_eq(*y, 0.65)
                && approx_eq(*u, 4.0 / 100.0)
                && approx_eq(*v, 20.0 / 100.0))));

    let wrapped_snapshot = SelectSnapshot {
        selected_index: 0,
        rows: vec![
            SelectRowSnapshot {
                index: 2,
                title: "Last".to_string(),
                play_level: "2".to_string(),
                ..SelectRowSnapshot::default()
            },
            SelectRowSnapshot {
                index: 0,
                title: "First".to_string(),
                play_level: "1".to_string(),
                ..SelectRowSnapshot::default()
            },
            SelectRowSnapshot {
                index: 1,
                title: "Second".to_string(),
                play_level: "2".to_string(),
                ..SelectRowSnapshot::default()
            },
        ],
        ..SelectSnapshot::default()
    };
    let items = document.select_render_items(&sources, &wrapped_snapshot);
    assert!(
        items
            .iter()
            .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Last"))
    );
    assert!(
        items
            .iter()
            .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "First"))
    );
    assert!(
        items
            .iter()
            .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Second"))
    );
}

#[test]
fn select_folder_distribution_graph_uses_cycle_animation_row() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "graph.png" }],
                "graph": [
                    { "id": "graph-lamp", "src": 1, "x": 0, "y": 0, "w": 44, "h": 8, "divx": 11, "divy": 2, "cycle": 100, "type": -1 }
                ],
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "liston": [{ "id": "row", "dst": [{ "x": 10, "y": 40, "w": 80, "h": 20 }] }],
                    "graph": { "id": "graph-lamp", "dst": [{ "x": 0, "y": 0, "w": 44, "h": 4 }] }
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 44.0, 8.0);
    let snapshot = SelectSnapshot {
        time: TimeUs(50_000),
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            is_folder: true,
            kind: SelectRowKind::Folder,
            folder_lamp_counts: {
                let mut counts = [0; 11];
                counts[5] = 1;
                counts[6] = 1;
                counts
            },
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let items = document.select_render_items(&sources, &snapshot);
    let graph_items: Vec<&SkinRenderItem> = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                SkinRenderItem::Image {
                    texture: SkinTextureId(9999),
                    rect: Rect { y, height, .. },
                    ..
                } if approx_eq(*y, 0.56) && approx_eq(*height, 0.04)
            )
        })
        .collect();

    assert_eq!(graph_items.len(), 2);
    assert!(graph_items.iter().all(|item| matches!(
        item,
        SkinRenderItem::Image {
            uv: TextureRegion { y, height, .. },
            ..
        } if approx_eq(*y, 0.5) && approx_eq(*height, 0.5)
    )));
    assert!(matches!(
        graph_items[0],
        SkinRenderItem::Image {
            rect: Rect { x, width, .. },
            uv: TextureRegion { x: uv_x, .. },
            ..
        } if approx_eq(*x, 0.10) && approx_eq(*width, 0.22) && approx_eq(*uv_x, 24.0 / 44.0)
    ));
    assert!(matches!(
        graph_items[1],
        SkinRenderItem::Image {
            rect: Rect { x, width, .. },
            uv: TextureRegion { x: uv_x, .. },
            ..
        } if approx_eq(*x, 0.32) && approx_eq(*width, 0.22) && approx_eq(*uv_x, 20.0 / 44.0)
    ));
}

#[test]
fn select_songlist_judgegraph_renders_chart_distribution() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "judgegraph": [{ "id": "density", "delay": 0, "noGap": 1, "noGapX": 1 }],
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "liston": [{ "id": "row", "dst": [{ "x": 10, "y": 40, "w": 80, "h": 20 }] }],
                    "listoff": [{ "id": "row", "dst": [{ "x": 10, "y": 40, "w": 80, "h": 20 }] }],
                    "judgegraph": [{ "id": "density", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
    let snapshot = SelectSnapshot {
        selected_index: 0,
        rows: vec![SelectRowSnapshot {
            index: 0,
            kind: SelectRowKind::Song,
            in_library: true,
            chart_distribution: vec![
                crate::scene::SelectChartDistributionSecond {
                    key_taps: 4,
                    mines: 1,
                    ..Default::default()
                },
                crate::scene::SelectChartDistributionSecond {
                    scratch_taps: 2,
                    key_long_bodies: 3,
                    ..Default::default()
                },
            ],
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let sources = HashMap::new();
    let items = document.select_render_items(&sources, &snapshot);
    let rect_count =
        items.iter().filter(|item| matches!(item, SkinRenderItem::Rect { .. })).count();

    assert_eq!(rect_count, 7);
}
#[test]
fn select_level_uses_chart_level_independently_of_table_memberships() {
    for (play_level, table_level, expected) in
        [("5", "5/5", 5), ("7", "6/6", 7), ("11", "2/4", 11), ("", "12", 0)]
    {
        let row = SelectRowSnapshot {
            play_level: play_level.into(),
            table_level: table_level.into(),
            ..SelectRowSnapshot::default()
        };
        assert_eq!(select_row_level_number(&row), expected);
    }
}

#[test]
fn table_level_override_applies_to_songlist_numbers_without_leaking_to_other_values() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"{
        "type":5,"w":100,"h":100,
        "value":[{"id":"level","src":"digits","w":100,"h":10,"divx":10,"digit":1,"ref":160}],
        "songlist":{
            "id":"list","center":0,
            "liston":[{"id":"bar","dst":[{"x":0,"y":50,"w":100,"h":10}]}],
            "listoff":[{}, {"id":"bar","dst":[{"x":0,"y":30,"w":100,"h":10}]}],
            "level":[{"id":"level","dst":[{"x":0,"y":0,"w":10,"h":10}]}]
        },
        "destination":[{"id":"list"},{"id":"level","dst":[{"x":80,"y":0,"w":10,"h":10}]}]
    }"#,
    )
    .unwrap();
    let sources = mock_source("digits", 100.0, 10.0);
    let mut snapshot = SelectSnapshot {
        rows: vec![
            SelectRowSnapshot {
                play_level: "5".into(),
                initial_bpm: 6.0,
                level_display_override: Some("2".into()),
                in_library: true,
                ..Default::default()
            },
            SelectRowSnapshot {
                index: 1,
                play_level: "9".into(),
                initial_bpm: 8.0,
                level_display_override: Some("7".into()),
                in_library: true,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let digits = |snapshot: &SelectSnapshot, document: &SkinDocument| {
        document
            .select_render_items(&sources, snapshot)
            .into_iter()
            .filter_map(|item| {
                if let SkinRenderItem::Image { uv, .. } = item {
                    Some((uv.x * 10.0).round() as i32)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    // Declared level objects use each row's override even with an explicit BPM ref.
    // The same numeric object outside songlist.level still shows selected BPM.
    assert_eq!(digits(&snapshot, &document), [2, 7, 6]);
    snapshot.rows[0].level_display_override = Some("???".into());
    assert_eq!(digits(&snapshot, &document), [7, 6]);
    snapshot.rows[0].level_display_override = None;
    snapshot.rows[1].level_display_override = None;
    assert_eq!(digits(&snapshot, &document), [6, 8, 6]);
    // Standard level refs follow the chosen display level outside the bar too.
    document.value[0].ref_id = 96;
    snapshot.rows[0].level_display_override = Some("2".into());
    snapshot.rows[1].level_display_override = Some("7".into());
    assert_eq!(digits(&snapshot, &document), [2, 7, 2]);
}
