use super::*;

#[test]
fn ambient_bga_groups_layers_and_follows_poor_and_disabled_state() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
        "type":0,"w":100,"h":100,"bga":{"id":"bga"},
        "destination":[
            {"id":"bga","ambient":true,"dst":[{"x":0,"y":0,"w":100,"h":100}]},
            {"id":"bga","dst":[{"x":50,"y":50,"w":50,"h":50}]}
        ]
    }"#,
    )
    .unwrap();
    let frame =
        |id| SkinBgaFrame::opaque(SkinTextureId(id), SkinImageSize { width: 100.0, height: 100.0 });
    let mut state = SkinDrawState {
        has_bga: true,
        bga_base: Some(frame(10)),
        bga_layer: Some(frame(11)),
        bga_layer2: Some(frame(12)),
        ..Default::default()
    };
    let render = |state: &SkinDrawState| {
        document.static_render_items(&HashMap::new(), state, &SkinTextState::default())
    };
    let items = render(&state);
    let SkinRenderItem::Ambient { layers, .. } = &items[0] else {
        panic!("ambient group");
    };
    assert_eq!(layers.len(), 3);
    assert!(matches!(&layers[1], SkinRenderItem::Image { blend: BlendMode::LayerMask, .. }));
    assert!(matches!(&items[1], SkinRenderItem::Image { texture: SkinTextureId(10), .. }));
    state.bga_poor = Some(frame(13));
    let items = render(&state);
    let SkinRenderItem::Ambient { layers, .. } = &items[0] else {
        panic!("poor group");
    };
    assert!(matches!(
        layers.as_slice(),
        [SkinRenderItem::Image { texture: SkinTextureId(13), .. }]
    ));
    state.bga_enabled = false;
    assert!(render(&state).is_empty());
}

#[test]
fn ambient_image_preserves_alpha_and_destination_geometry() {
    let document: SkinDocument = serde_json::from_str(r#"{
        "type":0,"w":100,"h":100,
        "image":[{"id":"fallback","src":"image","w":100,"h":100}],
        "destination":[{"id":"fallback","ambient":true,"dst":[{"x":20,"y":30,"w":40,"h":50,"a":128}]}]
    }"#).unwrap();
    let items = document.static_render_items(
        &mock_source("image", 100.0, 100.0),
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );
    let SkinRenderItem::Ambient { rect, layers, .. } = &items[0] else {
        panic!("ambient fallback");
    };
    assert!(approx_eq(rect.x, 0.2));
    assert!(
        matches!(&layers[0], SkinRenderItem::Image { tint, .. } if approx_eq(tint.a,128.0/255.0))
    );
    let mut commands = Vec::new();
    append_skin_render_items(&mut commands, &items);
    assert!(matches!(&commands[0], DrawCommand::Ambient { layers, .. } if layers.len() == 1));
}

#[test]
fn ambient_spread_uses_fitted_video_bounds_and_keeps_its_center() {
    for (width, height, fitted_width, fitted_height) in
        [(160.0, 90.0, 0.4, 0.45), (90.0, 160.0, 0.16875, 0.6), (100.0, 100.0, 0.3, 0.6)]
    {
        for spread in [0.0, 20.0, 200.0] {
            let document: SkinDocument = serde_json::from_value(serde_json::json!({
                "type":0,"w":200,"h":100,"bga":{"id":"bga"},
                "destination":[{"id":"bga","ambient":true,"ambientMode":"spread",
                    "ambientSpread":spread,"stretch":1,
                    "dst":[{"x":80,"y":20,"w":80,"h":60}]}]
            }))
            .unwrap();
            let state = SkinDrawState {
                has_bga: true,
                bga_base: Some(SkinBgaFrame::opaque(
                    SkinTextureId(10),
                    SkinImageSize { width, height },
                )),
                ..Default::default()
            };
            let items =
                document.static_render_items(&HashMap::new(), &state, &SkinTextState::default());
            let SkinRenderItem::Ambient { rect, blur, fade_edges, layers } = &items[0] else {
                panic!("spread")
            };
            assert!(*fade_edges);
            assert_eq!(*blur, 50.0);
            let scale = 1.0 + spread / 100.0;
            assert!(approx_eq(rect.width, fitted_width * scale));
            assert!(approx_eq(rect.height, fitted_height * scale));
            assert!(approx_eq(rect.x + rect.width / 2.0, 0.6));
            assert!(approx_eq(rect.y + rect.height / 2.0, 0.5));
            assert!(
                matches!(&layers[0], SkinRenderItem::Image { rect: layer, .. } if layer == rect)
            );
        }
    }
}

#[test]
fn ambient_spread_composes_mixed_aspects_and_blur_zero_bypasses_downsampling() {
    let mut document: SkinDocument = serde_json::from_str(r#"{
        "type":0,"w":200,"h":100,"bga":{"id":"bga"},
        "destination":[{"id":"bga","ambient":true,"ambientMode":"spread","ambientSpread":20,"stretch":1,
            "dst":[{"x":80,"y":20,"w":80,"h":60}]}]
    }"#).unwrap();
    let frame = |id, width, height| {
        SkinBgaFrame::opaque(SkinTextureId(id), SkinImageSize { width, height })
    };
    let mut state = SkinDrawState {
        has_bga: true,
        bga_base: Some(frame(10, 160.0, 90.0)),
        bga_layer: Some(frame(11, 100.0, 100.0)),
        ..Default::default()
    };
    let render = |document: &SkinDocument, state: &SkinDrawState| {
        document.static_render_items(&HashMap::new(), state, &SkinTextState::default())
    };
    let items = render(&document, &state);
    let SkinRenderItem::Ambient { rect, layers, .. } = &items[0] else { panic!("mixed group") };
    assert!(approx_eq(rect.width, 0.48) && approx_eq(rect.height, 0.72));
    assert_eq!(layers.len(), 2);
    // Changing POOR to a portrait replaces the mixed stack and its bounds.
    state.bga_poor = Some(frame(12, 90.0, 160.0));
    let items = render(&document, &state);
    assert!(matches!(&items[0], SkinRenderItem::Ambient { rect, layers, .. }
        if approx_eq(rect.width, 0.2025) && layers.len() == 1));
    // Change the JSON rather than hand-building destination internals.
    document = serde_json::from_str(r#"{
        "type":0,"w":200,"h":100,"bga":{"id":"bga"},
        "destination":[{"id":"bga","ambient":true,"ambientMode":"spread","ambientSpread":20,"ambientBlur":0,"stretch":1,
            "dst":[{"x":80,"y":20,"w":80,"h":60}]}]
    }"#).unwrap();
    let items = render(&document, &state);
    assert!(
        matches!(&items[0], SkinRenderItem::Image { rect, .. } if approx_eq(rect.width, 0.2025))
    );
}

#[test]
fn ambient_defaults_legacy_alias_and_percentage_limits() {
    let legacy: SkinDestinationDef = serde_json::from_str(r#"{"bmzAmbient":true}"#).unwrap();
    assert!(legacy.ambient);
    assert_eq!(legacy.ambient_mode, SkinAmbientMode::Full);
    assert_eq!((legacy.ambient_spread, legacy.ambient_blur), (20.0, 50.0));
    let frame = ResolvedSkinFrame { x: 0, y: 0, w: 100, h: 100, ..Default::default() };
    for (spread, blur, expected_width, expected_blur) in
        [(-1.0, -1.0, 1.0, 0.0), (500.0, 500.0, 3.0, 100.0), (f32::NAN, f32::NAN, 1.2, 50.0)]
    {
        let mut destination = legacy.clone();
        destination.ambient_mode = SkinAmbientMode::Spread;
        destination.ambient_spread = spread;
        destination.ambient_blur = blur;
        let items = wrap_ambient_destination(
            &destination,
            frame,
            100,
            100,
            vec![SkinRenderItem::Rect {
                rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                color: Color::rgb(1.0, 0.0, 0.0),
                blend: BlendMode::Normal,
            }],
        );
        if expected_blur == 0.0 {
            assert!(
                matches!(&items[0], SkinRenderItem::Rect { rect, .. } if approx_eq(rect.width, expected_width))
            );
        } else {
            assert!(matches!(&items[0], SkinRenderItem::Ambient { rect, blur, .. }
                if approx_eq(rect.width, expected_width) && *blur == expected_blur));
        }
    }
}
