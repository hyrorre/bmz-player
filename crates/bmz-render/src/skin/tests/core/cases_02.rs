use super::*;

#[test]
fn decorated_text_destination_alpha_is_shared_without_accumulation() {
    let mut document: SkinDocument = serde_json::from_str(
        r#"{
        "w":100,"h":100,
        "text":[{"id":"label","constantText":"FAVORITE","outlineWidth":2,
            "outlineColor":"12345680","shadowColor":"abcdef40",
            "shadowOffsetX":3,"shadowOffsetY":4}],
        "destination":[{"id":"label","offset":42,"loop":-1,"dst":[
            {"time":0,"w":50,"h":10,"r":100,"g":150,"b":200,"a":255},
            {"time":100,"a":0}]}]
    }"#,
    )
    .unwrap();
    for (elapsed_ms, offset, search, expected) in [
        (0, 0, 1.0, 1.0),
        (50, 0, 1.0, 128.0 / 255.0),
        (100, 0, 1.0, 0.0),
        (0, 0, 1.0, 1.0),
        (0, -127, 1.0, 128.0 / 255.0),
        (0, 0, 0.5, 0.5),
    ] {
        document.text[0].ref_id = if search < 1.0 { 30 } else { 0 };
        let mut state = SkinDrawState { elapsed_ms, ..Default::default() };
        state.skin_offsets.set(42, SkinOffsetValue { a: offset, ..Default::default() });
        let items = document.static_render_items(
            &HashMap::new(),
            &state,
            &SkinTextState {
                search_word: "FAVORITE",
                search_word_alpha: search,
                search_caret_byte_index: Some(3),
                ..Default::default()
            },
        );
        let SkinRenderItem::Text { text, style, caret, .. } = &items[0] else { panic!("text") };
        assert_eq!(text, "FAVORITE");
        assert!(
            approx_eq(style.color.a, expected),
            "time={elapsed_ms} offset={offset}: {} != {expected}",
            style.color.a
        );
        assert!(approx_eq(style.color.r, 100.0 / 255.0));
        let outline = style.outline.unwrap();
        let shadow = style.shadow.unwrap();
        assert!(approx_eq(outline.color.a, 128.0 / 255.0 * expected));
        assert!(approx_eq(shadow.color.a, 64.0 / 255.0 * expected));
        assert!(approx_eq(outline.color.r, 18.0 / 255.0));
        assert!(approx_eq(shadow.color.r, 171.0 / 255.0));
        assert!(approx_eq(outline.width, 0.02));
        assert_eq!(shadow.offset, Point { x: 0.03, y: 0.04 });
        if search < 1.0 {
            assert!(approx_eq(caret.unwrap().color.a, expected));
        }
        let faded = style.clone().with_alpha(0.25);
        assert!(approx_eq(faded.color.a, expected * 0.25));
        assert!(approx_eq(faded.outline.unwrap().color.a, outline.color.a * 0.25));
        assert!(approx_eq(faded.shadow.unwrap().color.a, shadow.color.a * 0.25));
    }
}

#[test]
fn decorated_text_defaults_and_transparent_decoration_colors() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
        "text":[{"id":"label","constantText":"TEXT","outlineWidth":2,
            "outlineColor":"ff000000","shadowColor":"00ff0000"}],
        "destination":[{"id":"label","dst":[{"h":10}]}]
    }"#,
    )
    .unwrap();
    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );
    let SkinRenderItem::Text { style, .. } = &items[0] else { panic!("text") };
    assert_eq!(style.color.a, 1.0);
    assert!(style.outline.is_none());
    assert!(style.shadow.is_none());
}

#[test]
fn fully_transparent_destination_hides_opaque_decorations_and_clamps_alpha() {
    let document: SkinDocument = serde_json::from_str(r#"{"w":100,"h":100}"#).unwrap();
    let text = SkinTextDef {
        constant_text: "FAVORITE".into(),
        outline_width: 2.0,
        outline_color: "ff0000ff".into(),
        shadow_color: "00ff00ff".into(),
        ..Default::default()
    };
    for (a, expected) in [(-10, 0.0), (0, 0.0), (255, 1.0), (300, 1.0)] {
        let item = document
            .text_render_item(
                &text,
                ResolvedSkinFrame { a, h: 10, ..Default::default() },
                &SkinTextState::default(),
            )
            .unwrap();
        let SkinRenderItem::Text { style, .. } = item else { panic!("text") };
        assert_eq!(style.color.a, expected);
        assert_eq!(style.outline.unwrap().color.a, expected);
        assert_eq!(style.shadow.unwrap().color.a, expected);
    }
}

#[test]
fn static_image_destination_maps_lr2_multiply_and_subtract_blends() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w": 100,
            "h": 100,
            "source": [{ "id": 1, "path": "colors.png" }],
            "image": [{ "id": "judge-color", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1 }],
            "destination": [{
                "id": "judge-color",
                "blend": 4,
                "dst": [{ "x": 10, "y": 20, "w": 30, "h": 10 }]
            }]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("1", 6.0, 1.0);

    for (mode, expected) in [(4, BlendMode::Multiply), (3, BlendMode::Subtract)] {
        let mut document = document.clone();
        let DestinationListEntry::Single(destination) = &mut document.destination[0] else {
            panic!("destination")
        };
        destination.blend = mode;
        let items = document.static_image_render_items(&sources, &SkinDrawState::default());
        assert!(
            matches!(items.as_slice(), [SkinRenderItem::Image { blend, .. }] if *blend == expected)
        );
    }
}

#[test]
fn static_render_items_require_an_exact_destination_image_id() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "frame.png" }],
                "image": [{ "id": "groove_frame", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "groove_frame_iidx", "timer": 9001, "dst": [{ "x": 1, "y": 2, "w": 10, "h": 10 }] }
                ],
                "dynamicTimer": [{ "id": 9001, "observe": "gauge_type() == 4 or gauge_type() == 5" }]
            }
            "#,
        )
        .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(7),
            source_size: SkinImageSize { width: 100.0, height: 100.0 },
        },
    )]);
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState { gauge_type: 4, elapsed_ms: 100, ..Default::default() };
    runtime.advance(&document, &mut state, 100);
    let (behind, front, _) =
        document.static_render_items_split(&sources, &state, &SkinTextState::default());
    let items = behind.into_iter().chain(front).collect::<Vec<_>>();
    assert!(items.is_empty());
}

#[test]
fn skin_document_evaluates_destination_option_conditions() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "property": [
                    { "name": "Play Side", "item": [
                        { "name": "1P", "op": 920 },
                        { "name": "2P", "op": 921 }
                    ]},
                    { "name": "Score Graph", "def": "On", "item": [
                        { "name": "Off", "op": 900 },
                        { "name": "On", "op": 901 }
                    ]}
                ],
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "op": [920, 901], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "op": [921], "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "op": [-901], "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] }
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

    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(document.enabled_options(), [920, 901]);
    assert_eq!(items.len(), 1);
    assert!(
        matches!(items[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.0))
    );
}

#[test]
fn skin_document_applies_destination_acc_easing() {
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    for (acc, expected_x) in [(1, 0.25), (2, 0.75), (3, 0.0)] {
        let document: SkinDocument = serde_json::from_str(&format!(
            r#"
                {{
                    "type": 0,
                    "w": 100,
                    "h": 100,
                    "source": [{{ "id": 1, "path": "system.png" }}],
                    "image": [{{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }}],
                    "destination": [
                        {{ "id": "panel", "dst": [
                            {{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }},
                            {{ "time": 100, "x": 100, "acc": {acc} }}
                        ]}}
                    ]
                }}
                "#
        ))
        .unwrap();

        let items = document.static_image_render_items(
            &sources,
            &SkinDrawState { elapsed_ms: 50, ..SkinDrawState::default() },
        );

        assert!(matches!(items[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                    if approx_eq(x, expected_x)));
    }
}

#[test]
fn skin_document_prefers_lnbody_active_for_pressed_long_body_in_new_format() {
    // 新形式 (lnbodyActive 定義あり): 押下中=lnbodyActive、非押下=lnbody。
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "source": [{ "id": 1, "path": "notes.png" }],
                "image": [
                    { "id": "body", "src": 1, "x": 20, "y": 0, "w": 20, "h": 1 },
                    { "id": "body-a", "src": 1, "x": 50, "y": 0, "w": 30, "h": 1 }
                ],
                "note": {
                    "id": "notes",
                    "note": ["body", "body", "body", "body", "body", "body", "body", "body"],
                    "lnbody": ["body", "body", "body", "body", "body", "body", "body", "body"],
                    "lnbodyActive": ["body-a", "body-a", "body-a", "body-a", "body-a", "body-a", "body-a", "body-a"]
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

    let pressed = document
        .note_long_body_render_item(
            Lane::Scratch,
            KeyMode::K7,
            rect,
            LongNoteMode::Ln,
            LongBodyState::Processing,
            &SkinDrawState::default(),
            &sources,
        )
        .unwrap();
    let unpressed = document
        .note_long_body_render_item(
            Lane::Scratch,
            KeyMode::K7,
            rect,
            LongNoteMode::Ln,
            LongBodyState::Inactive,
            &SkinDrawState::default(),
            &sources,
        )
        .unwrap();

    // 押下中 → lnbodyActive (x=50/100)、非押下 → lnbody (x=20/100)
    assert!(matches!(
        pressed,
        SkinRenderItem::Image { uv: TextureRegion { x, .. }, .. } if approx_eq(x, 0.5)
    ));
    assert!(matches!(
        unpressed,
        SkinRenderItem::Image { uv: TextureRegion { x, .. }, .. } if approx_eq(x, 0.2)
    ));
}

#[test]
fn skin_document_animates_csv_ln_body_only_while_processing() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "source": [{ "id": 1, "path": "notes.png" }],
                "image": [
                    { "id": "active", "src": 1, "x": 0, "y": 0, "w": 20, "h": 10, "divx": 2, "cycle": 100, "timer": 70 },
                    { "id": "inactive", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 }
                ],
                "note": {
                    "id": "notes",
                    "lnbody": ["inactive", "inactive", "inactive", "inactive", "inactive", "inactive", "inactive", "inactive"],
                    "lnbodyActive": ["active", "active", "active", "active", "active", "active", "active", "active"]
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
            source_size: SkinImageSize { width: 100.0, height: 10.0 },
        },
    )]);
    let rect = Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 };
    let mut draw_state = SkinDrawState::default();
    draw_state.hold_ms[Lane::Scratch.index()] = Some(50);

    let pressed = document
        .note_long_body_render_item(
            Lane::Scratch,
            KeyMode::K7,
            rect,
            LongNoteMode::Ln,
            LongBodyState::Processing,
            &draw_state,
            &sources,
        )
        .unwrap();
    let unpressed = document
        .note_long_body_render_item(
            Lane::Scratch,
            KeyMode::K7,
            rect,
            LongNoteMode::Ln,
            LongBodyState::Inactive,
            &draw_state,
            &sources,
        )
        .unwrap();

    assert!(matches!(
        pressed,
        SkinRenderItem::Image { uv: TextureRegion { x, .. }, .. } if approx_eq(x, 0.1)
    ));
    assert!(matches!(
        unpressed,
        SkinRenderItem::Image { uv: TextureRegion { x, .. }, .. } if approx_eq(x, 0.2)
    ));
}

#[test]
fn panel_renders_fill_and_canvas_pixel_border() {
    let document: SkinDocument = serde_json::from_str(
        r##"
            {
                "w": 100,
                "h": 100,
                "panel": [{
                    "id": "option-panel",
                    "color": "#102030",
                    "borderColor": "#A0B0C0",
                    "borderWidth": 2
                }],
                "destination": [{
                    "id": "option-panel",
                    "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }]
                }]
            }
            "##,
    )
    .unwrap();

    let items = document.static_image_render_items(&HashMap::new(), &SkinDrawState::default());

    assert_eq!(items.len(), 5);
    let SkinRenderItem::Rect { rect, color, blend } = items[0] else {
        panic!("expected panel fill");
    };
    assert_eq!(rect, Rect { x: 0.1, y: 0.4, width: 0.3, height: 0.4 });
    assert!(approx_eq(color.r, 16.0 / 255.0));
    assert!(approx_eq(color.g, 32.0 / 255.0));
    assert!(approx_eq(color.b, 48.0 / 255.0));
    assert_eq!(blend, BlendMode::Normal);
    assert!(matches!(
        items[1],
        SkinRenderItem::Rect {
            rect: Rect { x, y, width, height },
            ..
        } if approx_eq(x, 0.1)
            && approx_eq(y, 0.4)
            && approx_eq(width, 0.3)
            && approx_eq(height, 0.02)
    ));
}

#[test]
fn skin_document_resolves_static_value_destinations() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "number.png" }],
                "value": [
                    { "id": "combo", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 104 },
                    { "id": "remain", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "expr": "number(106) - number(110) - number(111)" },
                    { "id": "unknown", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 9999 }
                ],
                "destination": [
                    { "id": "combo", "dst": [{ "x": 10, "y": 20, "w": 5, "h": 10 }] },
                    { "id": "remain", "dst": [{ "x": 10, "y": 30, "w": 5, "h": 10 }] },
                    { "id": "unknown", "dst": [{ "x": 10, "y": 40, "w": 5, "h": 10 }] }
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

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            elapsed_ms: 0,
            combo: 45,
            total_notes: 100,
            judge_counts: DisplayJudgeCounts { pgreat: 30, great: 20, ..Default::default() },
            ..SkinDrawState::default()
        },
    );

    // combo=45 (2 digits), digit=3 → shiftbase=1, align=0 (right-aligned, default)
    // digit_step = 5/100 = 0.05, origin_x = 10/100 = 0.1
    // digit "4": x = 0.1 + 0.05 * (1+0) - 0 = 0.15
    // digit "5": x = 0.1 + 0.05 * (1+1) - 0 = 0.20
    assert_eq!(items.len(), 4);
    assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.15) && approx_eq(y, 0.7) && approx_eq(u, 0.4)));
    assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.20) && approx_eq(u, 0.5)));
    assert!(matches!(items[2], SkinRenderItem::Image {
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.15) && approx_eq(y, 0.6) && approx_eq(u, 0.5)));
    assert!(matches!(items[3], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.20) && approx_eq(u, 0.0)));
}

#[test]
fn skin_document_resolves_static_text_destinations() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "text": [
                    { "id": "title", "font": "main", "size": 8, "align": 1, "wrapping": true, "outlineColor": "ff000080", "outlineWidth": 1, "shadowColor": "00000080", "shadowOffsetX": 2, "shadowOffsetY": 3, "ref": 12 },
                    { "id": "genre", "size": 6, "align": 2, "overflow": 1, "ref": 13 },
                    { "id": "uniform", "size": 6, "overflow": 1, "shrinkMode": 1, "ref": 13 },
                    { "id": "constant", "size": 5, "constantText": "READY" },
                    { "id": "numeric-constant", "size": 5, "constantText": 1 }
                ],
                "destination": [
                    { "id": "title", "dst": [{ "x": 10, "y": 20, "w": 50, "h": 10, "r": 128, "g": 200, "b": 255 }] },
                    { "id": "genre", "dst": [{ "x": 10, "y": 40, "w": 40, "h": 6 }] },
                    { "id": "uniform", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 6 }] },
                    { "id": "constant", "dst": [{ "x": 10, "y": 60, "h": 5, "a": 128 }] },
                    { "id": "numeric-constant", "dst": [{ "x": 10, "y": 70, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let items = document.static_render_items(
        &HashMap::new(),
        &SkinDrawState::default(),
        &SkinTextState {
            title: "Song",
            subtitle: "Another",
            genre: "Techno",
            ..SkinTextState::default()
        },
    );

    assert_eq!(items.len(), 5);
    assert!(matches!(&items[0], SkinRenderItem::Text {
                origin: Point { x, y },
                text,
                style,
                ..
            } if approx_eq(*x, -0.15)
                && approx_eq(*y, 0.7)
                && text == "Song Another"
                && style.font_id.as_deref() == Some("main")
                && approx_eq(style.size, 0.1)
                && style.align == TextAlign::Center
                && style.wrapping
                && matches!(style.outline, Some(TextOutline { color, width })
                    if color == Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0)
                        && approx_eq(width, 0.01))
                && matches!(style.shadow, Some(TextShadow { color, offset })
                    if color == Color::rgba(0.0, 0.0, 0.0, 128.0 / 255.0)
                        && approx_eq(offset.x, 0.02)
                        && approx_eq(offset.y, 0.03))
                && approx_eq(style.max_width, 0.5)
                && style.color == Color::rgba(128.0 / 255.0, 200.0 / 255.0, 1.0, 1.0)));
    assert!(matches!(&items[1], SkinRenderItem::Text { text, style, .. }
                if text == "Techno"
                    && style.align == TextAlign::Right
                    && style.overflow == TextOverflow::Shrink
                    && approx_eq(style.max_width, 0.4)));
    assert!(matches!(&items[2], SkinRenderItem::Text { text, style, .. }
                if text == "Techno"
                    && style.overflow == TextOverflow::ShrinkUniform
                    && approx_eq(style.max_width, 0.4)));
    assert!(
        matches!(&items[3], SkinRenderItem::Text { text, style, .. } if text == "READY" && approx_eq(style.color.a, 128.0 / 255.0))
    );
    assert!(matches!(&items[4], SkinRenderItem::Text { text, .. } if text == "1"));
}

#[test]
fn skin_document_resolves_music_progress_slider() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "slider": [
                    { "id": "progress", "src": 1, "x": 10, "y": 20, "w": 5, "h": 6, "angle": 2, "range": 40, "type": 6 },
                    { "id": "lane-cover", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "angle": 2, "range": 20, "type": 4 },
                    { "id": "lane-cover-modern", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "angle": 2, "range": 20, "type": 5 },
                    { "id": "song-scroll", "src": 1, "x": 20, "y": 20, "w": 5, "h": 6, "angle": 2, "range": 40, "type": 1 },
                    { "id": "master", "src": 1, "x": 30, "y": 20, "w": 5, "h": 6, "angle": 1, "range": 40, "type": 17 },
                    { "id": "unknown", "src": 1, "x": 10, "y": 20, "w": 5, "h": 6, "angle": 0, "range": 40, "type": 999 }
                ],
                "destination": [
                    { "id": "progress", "blend": 2, "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
                    { "id": "lane-cover", "dst": [{ "x": 10, "y": 50, "w": 10, "h": 10 }] },
                    { "id": "lane-cover-modern", "dst": [{ "x": 20, "y": 50, "w": 10, "h": 10 }] },
                    { "id": "song-scroll", "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
                    { "id": "master", "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
                    { "id": "unknown", "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] }
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

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            play_progress: 0.25,
            select_scroll_progress: 0.5,
            select_master_volume: 0.75,
            ..SkinDrawState::default()
        },
    );

    assert_eq!(items.len(), 3);
    assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uw, height: uh },
                blend,
                ..
            } if approx_eq(x, 0.3)
                && approx_eq(y, 0.24)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.06)
                && approx_eq(u, 0.1)
                && approx_eq(v, 0.2)
                && approx_eq(uw, 0.05)
                && approx_eq(uh, 0.06)
                && blend == BlendMode::Add));
    assert!(matches!(
        items[1],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.3) && approx_eq(y, 0.34)
    ));
    assert!(matches!(
        items[2],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.6) && approx_eq(y, 0.14)
    ));

    let no_lane_cover = document.static_image_render_items(
        &sources,
        &SkinDrawState { lane_cover: 0.0, ..SkinDrawState::default() },
    );
    assert_eq!(no_lane_cover.len(), 3);

    let lane_cover = document.static_image_render_items(
        &sources,
        &SkinDrawState { lane_cover: 0.5, ..SkinDrawState::default() },
    );
    assert_eq!(lane_cover.len(), 5);
    assert!(matches!(
        lane_cover[1],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.1) && approx_eq(y, 0.5)
    ));
    assert!(matches!(
        lane_cover[2],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.2) && approx_eq(y, 0.5)
    ));
}

#[test]
fn slider_image_rectangle_resolves_remaining_and_positive_source_bounds() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w":100,"h":100,
            "slider":[
                {"id":"full","src":"src","x":0,"y":0,"w":-1,"h":-1,"type":6},
                {"id":"offset","src":"src","x":10,"y":5,"w":-1,"h":-1,"type":6},
                {"id":"positive","src":"src","x":10,"y":20,"w":30,"h":20,"type":6}
            ],
            "destination":[
                {"id":"full","dst":[{"x":0,"y":0,"w":10,"h":10}]},
                {"id":"offset","dst":[{"x":20,"y":0,"w":10,"h":10}]},
                {"id":"positive","dst":[{"x":40,"y":0,"w":10,"h":10}]}
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("src", 100.0, 80.0);
    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 3);
    assert!(matches!(
        items[0],
        SkinRenderItem::Image { uv: TextureRegion { x, y, width, height }, .. }
            if approx_eq(x, 0.0) && approx_eq(y, 0.0)
                && approx_eq(width, 1.0) && approx_eq(height, 1.0)
    ));
    assert!(matches!(
        items[1],
        SkinRenderItem::Image { uv: TextureRegion { x, y, width, height }, .. }
            if approx_eq(x, 0.1) && approx_eq(y, 5.0 / 80.0)
                && approx_eq(width, 0.9) && approx_eq(height, 75.0 / 80.0)
    ));
    assert!(matches!(
        items[2],
        SkinRenderItem::Image { uv: TextureRegion { x, y, width, height }, .. }
            if approx_eq(x, 0.1) && approx_eq(y, 0.25)
                && approx_eq(width, 0.3) && approx_eq(height, 0.25)
    ));
}

#[test]
fn slider_source_bounds_are_clipped_and_empty_regions_are_skipped() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w":100,"h":100,
            "slider":[
                {"id":"clipped","src":"src","x":90,"y":70,"w":50,"h":50,"type":6},
                {"id":"negative-origin","src":"src","x":-10,"y":-5,"w":30,"h":20,"type":6},
                {"id":"outside","src":"src","x":101,"y":80,"w":-1,"h":-1,"type":6},
                {"id":"empty","src":"src","x":0,"y":0,"w":0,"h":10,"type":6}
            ],
            "destination":[
                {"id":"clipped","dst":[{"x":0,"y":0,"w":10,"h":10}]},
                {"id":"negative-origin","dst":[{"x":20,"y":0,"w":10,"h":10}]},
                {"id":"outside","dst":[{"x":20,"y":0,"w":10,"h":10}]},
                {"id":"empty","dst":[{"x":40,"y":0,"w":10,"h":10}]}
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("src", 100.0, 80.0);
    let items = document.static_image_render_items(&sources, &SkinDrawState::default());

    assert_eq!(items.len(), 2);
    assert!(matches!(
        items[0],
        SkinRenderItem::Image { uv: TextureRegion { x, y, width, height }, .. }
            if approx_eq(x, 0.9) && approx_eq(y, 70.0 / 80.0)
                && approx_eq(width, 0.1) && approx_eq(height, 10.0 / 80.0)
    ));
    assert!(matches!(
        items[1],
        SkinRenderItem::Image { uv: TextureRegion { x, y, width, height }, .. }
            if approx_eq(x, 0.0) && approx_eq(y, 0.0)
                && approx_eq(width, 0.2) && approx_eq(height, 15.0 / 80.0)
    ));
}

#[test]
fn music_progress_slider_uses_clamped_rate_direction_range_and_destination_style() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w":1920,"h":1080,
            "slider":[
                {"id":"progress","src":"src","x":0,"y":0,"w":-1,"h":-1,
                    "angle":2,"range":617,"type":6}
            ],
            "destination":[
                {"id":"progress","blend":2,"filter":1,"dst":[
                    {"x":25,"y":1014,"w":36,"h":42,"r":128,"g":64,"b":32,"a":128}
                ]}
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("src", 100.0, 80.0);

    for (progress, expected_y) in [(-0.1, 1014), (0.0, 1014), (0.5, 705), (1.0, 397), (1.1, 397)] {
        let items = document.static_image_render_items(
            &sources,
            &SkinDrawState { play_progress: progress, ..SkinDrawState::default() },
        );
        assert_eq!(items.len(), 1);
        assert!(
            matches!(
                &items[0],
                SkinRenderItem::Image {
                    rect: Rect { x, y, width, height },
                    uv: TextureRegion { width: uv_width, height: uv_height, .. },
                    tint: Color { r, g, b, a },
                    blend,
                    linear_filter,
                    ..
                } if approx_eq(*x, 25.0 / 1920.0)
                    && approx_eq(*y, 1.0 - expected_y as f32 / 1080.0 - 42.0 / 1080.0)
                    && approx_eq(*width, 36.0 / 1920.0)
                    && approx_eq(*height, 42.0 / 1080.0)
                    && approx_eq(*uv_width, 1.0) && approx_eq(*uv_height, 1.0)
                    && approx_eq(*r, 128.0 / 255.0) && approx_eq(*g, 64.0 / 255.0)
                    && approx_eq(*b, 32.0 / 255.0) && approx_eq(*a, 128.0 / 255.0)
                    && *blend == BlendMode::Add && *linear_filter
            ),
            "unexpected slider item for progress {progress}: {:?}",
            items[0]
        );
    }
}

#[test]
fn slider_source_animation_uses_its_timer_and_cycle_separately_from_destination() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w":100,"h":100,
            "slider":[
                {"id":"animated","src":"src","x":10,"y":5,"w":40,"h":20,
                    "divx":2,"divy":2,"timer":41,"cycle":100,"type":6}
            ],
            "destination":[
                {"id":"animated","timer":40,"loop":100,"dst":[
                    {"time":0,"x":20,"y":60,"w":10,"h":10},
                    {"time":100,"x":40}
                ]}
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("src", 80.0, 40.0);

    for (source_elapsed, expected_u, expected_v) in [
        (None, 10.0 / 80.0, 5.0 / 40.0),
        (Some(25), 30.0 / 80.0, 5.0 / 40.0),
        (Some(50), 10.0 / 80.0, 15.0 / 40.0),
        (Some(75), 30.0 / 80.0, 15.0 / 40.0),
    ] {
        let items = document.static_image_render_items(
            &sources,
            &SkinDrawState {
                elapsed_ms: 80,
                ready_timer_ms: Some(50),
                play_timer_ms: source_elapsed,
                ..SkinDrawState::default()
            },
        );
        assert_eq!(items.len(), 1);
        assert!(
            matches!(
                &items[0],
                SkinRenderItem::Image {
                    rect: Rect { x, y, .. },
                    uv: TextureRegion { x: u, y: v, width, height },
                    ..
                } if approx_eq(*x, 0.3) && approx_eq(*y, 0.3)
                    && approx_eq(*u, expected_u) && approx_eq(*v, expected_v)
                    && approx_eq(*width, 0.25) && approx_eq(*height, 0.25)
            ),
            "source timer elapsed {source_elapsed:?} did not select its expected frame: {:?}",
            items[0]
        );
    }
}

#[test]
fn non_play_slider_types_keep_scroll_and_volume_progress() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w":100,"h":100,
            "slider":[
                {"id":"scroll","src":"src","w":10,"h":10,"angle":1,"range":20,"type":1},
                {"id":"master","src":"src","w":10,"h":10,"angle":1,"range":20,"type":17},
                {"id":"key","src":"src","w":10,"h":10,"angle":1,"range":20,"type":18},
                {"id":"bgm","src":"src","w":10,"h":10,"angle":1,"range":20,"type":19}
            ],
            "destination":[
                {"id":"scroll","dst":[{"x":0,"y":0,"w":10,"h":10}]},
                {"id":"master","dst":[{"x":20,"y":0,"w":10,"h":10}]},
                {"id":"key","dst":[{"x":40,"y":0,"w":10,"h":10}]},
                {"id":"bgm","dst":[{"x":60,"y":0,"w":10,"h":10}]}
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("src", 100.0, 100.0);
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            select_screen: true,
            select_scroll_progress: 0.25,
            select_master_volume: 0.5,
            select_key_volume: 0.75,
            select_bgm_volume: 1.0,
            ..SkinDrawState::default()
        },
    );

    assert_eq!(items.len(), 4);
    for (item, expected_x) in items.iter().zip([0.05, 0.3, 0.55, 0.8]) {
        assert!(matches!(
            item,
            SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(*x, expected_x)
        ));
    }
}

#[test]
fn ir_ranking_slider_keeps_ranking_scroll_progress() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "w":100,"h":100,
            "slider":[
                {"id":"ranking","src":"src","w":10,"h":10,"angle":1,"range":20,"type":8}
            ],
            "destination":[
                {"id":"ranking","draw":"result_panel(1)","dst":[{"x":0,"y":0,"w":10,"h":10}]}
            ]
        }
        "#,
    )
    .unwrap();
    let sources = mock_source("src", 100.0, 100.0);
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState {
            result_panel: Some(1),
            ir_ranking: crate::scene::ResultIrSnapshot {
                scroll_offset: 5,
                scroll_max: 10,
                ..Default::default()
            },
            ..SkinDrawState::default()
        },
    );

    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(*x, 0.1)
    ));
}

#[test]
fn skin_document_moves_sliders_in_beatoraja_directions() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "slider": [
                    { "id": "up", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 0, "range": 20, "type": 17 },
                    { "id": "right", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 1, "range": 20, "type": 17 },
                    { "id": "down", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 2, "range": 20, "type": 17 },
                    { "id": "left", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 3, "range": 20, "type": 17 }
                ],
                "destination": [
                    { "id": "up", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] },
                    { "id": "right", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] },
                    { "id": "down", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] },
                    { "id": "left", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] }
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

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { select_master_volume: 0.5, ..SkinDrawState::default() },
    );

    assert_eq!(items.len(), 4);
    assert!(matches!(
        items[0],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.5) && approx_eq(y, 0.35)
    ));
    assert!(matches!(
        items[1],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.6) && approx_eq(y, 0.45)
    ));
    assert!(matches!(
        items[2],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.5) && approx_eq(y, 0.55)
    ));
    assert!(matches!(
        items[3],
        SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
            if approx_eq(x, 0.4) && approx_eq(y, 0.45)
    ));
}

#[test]
fn sudden_slider_progress_is_capped_by_lift() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "cover.png" }],
                "slider": [
                    { "id": "lanecover", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "angle": 2, "range": 100, "type": 4 }
                ],
                "destination": [
                    { "id": "lanecover", "dst": [{ "x": 0, "y": 100, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let sources = mock_source("1", 100.0, 100.0);

    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { lane_cover: 0.9, lift: 0.2, ..SkinDrawState::default() },
    );

    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    assert!(approx_eq(rect.y, 0.7), "expected capped SUDDEN slider y, got {}", rect.y);
}

#[test]
fn skin_document_resolves_special_black_fade_rect() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 6,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -110, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 200, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();

    let mid = document.static_image_render_items(
        &HashMap::new(),
        &SkinDrawState { fadeout_ms: Some(100), ..SkinDrawState::default() },
    );

    assert_eq!(mid.len(), 1);
    assert!(matches!(mid[0], SkinRenderItem::Rect {
                rect: Rect { width, height, .. },
                color: Color { r, g, b, a },
                ..
            } if approx_eq(width, 1.0)
                && approx_eq(height, 1.0)
                && approx_eq(r, 0.0)
                && approx_eq(g, 0.0)
                && approx_eq(b, 0.0)
                && approx_eq(a, 128.0 / 255.0)));
}

#[test]
fn src_zero_image_keeps_its_explicit_pixel_crop() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": "system", "path": "system.png" }],
                "image": [
                    { "id": 7, "src": 0, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "black", "src": "bg", "x": 391, "y": 1080, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": 7, "timer": 3, "dst": [{ "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 200 }] }
                ]
            }
            "#,
        )
        .unwrap();
    let images = document.image_map();
    let image = images.get("7").unwrap();
    let rect = skin_image_pixel_rect(image);
    assert_eq!(rect, (0, 0, 8, 8));
}

#[test]
fn src_zero_with_explicit_crop_keeps_pixel_rect() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": "system", "path": "system.png" }],
                "image": [
                    { "id": "black", "src": "bg", "x": 391, "y": 1080, "w": 8, "h": 8 },
                    { "id": 15, "src": 0, "x": 16, "y": 0, "w": 8, "h": 8 }
                ]
            }
            "#,
    )
    .unwrap();
    let images = document.image_map();
    let image = images.get("15").unwrap();
    let rect = skin_image_pixel_rect(image);
    assert_eq!(rect, (16, 0, 8, 8));
}

#[test]
fn image_negative_crop_size_uses_remaining_source_extent() {
    let image = SkinImageDef {
        lr2_panel: None,
        id: "frame".to_string(),
        src: "src".to_string(),
        x: 10,
        y: 20,
        w: -1,
        h: -1,
        divx: 1,
        divy: 1,
        timer: None,
        cycle: 0,
        len: 0,
        ref_id: 0,
        click: 0,
        act: None,
        clickable: None,
    };

    let uv = skin_image_texture_region(&image, SkinImageSize { width: 110.0, height: 220.0 }, 0);

    assert!(approx_eq(uv.x, 10.0 / 110.0));
    assert!(approx_eq(uv.y, 20.0 / 220.0));
    assert!(approx_eq(uv.width, 100.0 / 110.0));
    assert!(approx_eq(uv.height, 200.0 / 220.0));
}

#[test]
fn failed_close_black_fades_in_over_fullscreen() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": "system", "path": "system.png" }],
                "image": [{ "id": "black", "src": "bg", "x": 391, "y": 1080, "w": 8, "h": 8 }],
                "destination": [
                    { "id": "black", "loop": 1000, "timer": 3, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 1000, "a": 255 }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = mock_source("bg", 1920.0, 1080.0);

    let inactive = document.static_image_render_items(
        &sources,
        &SkinDrawState { failed_ms: None, ..SkinDrawState::default() },
    );
    let mid = document.static_image_render_items(
        &sources,
        &SkinDrawState { failed_ms: Some(500), ..SkinDrawState::default() },
    );
    let (_, _, failed_overlay) = document.static_render_items_split(
        &sources,
        &SkinDrawState { failed_ms: Some(500), ..SkinDrawState::default() },
        &SkinTextState::default(),
    );

    assert!(inactive.is_empty());
    assert_eq!(mid.len(), 1);
    assert_eq!(failed_overlay.len(), 1);
    assert!(matches!(mid[0], SkinRenderItem::Image {
                rect: Rect { width, height, .. },
                tint: Color { a, .. },
                ..
            } if approx_eq(width, 1.0)
                && approx_eq(height, 1.0)
                && approx_eq(a, 128.0 / 255.0)));
}
