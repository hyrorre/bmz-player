use super::*;

#[test]
fn ambient_bga_groups_layers_and_follows_poor_and_disabled_state() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
        "type":0,"w":100,"h":100,"bga":{"id":"bga"},
        "destination":[
            {"id":"bga","bmzAmbient":true,"dst":[{"x":0,"y":0,"w":100,"h":100}]},
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
        "destination":[{"id":"fallback","bmzAmbient":true,"dst":[{"x":20,"y":30,"w":40,"h":50,"a":128}]}]
    }"#).unwrap();
    let items = document.static_render_items(
        &mock_source("image", 100.0, 100.0),
        &SkinDrawState::default(),
        &SkinTextState::default(),
    );
    let SkinRenderItem::Ambient { rect, layers } = &items[0] else {
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
