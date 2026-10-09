use super::*;

#[test]
fn antique_progress_slider_resolves_its_source_and_moves_with_play_progress_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let library_roots = test_app_paths().skin_library_roots();
    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &skin_path,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &BTreeMap::new(),
        runtime_state: &LuaLoadRuntimeState::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &library_roots,
    })
    .expect("decode Antique skin with the app skin-library root");
    let slider = decoded
        .document
        .slider
        .iter()
        .find(|slider| slider.id == "sld_progress_song")
        .expect("Antique song progress slider");
    assert_eq!(slider.src, "src_progress_song");
    assert_eq!((slider.x, slider.y, slider.w, slider.h), (0, 0, -1, -1));
    assert_eq!((slider.angle, slider.range, slider.slider_type), (2, 617, 6));

    let source = decoded
        .sources
        .iter()
        .find(|source| source.source_id == slider.src)
        .expect("Antique progress image source should resolve within the skin library");
    assert!(source.path.is_file(), "resolved progress image: {}", source.path.display());
    assert!(source.size.width > 0.0 && source.size.height > 0.0);
    let texture = source.texture;
    let document_sources = decoded
        .sources
        .iter()
        .map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: source.size,
        })
        .map(|source| (source.source_id.clone(), source))
        .collect::<HashMap<_, _>>();

    for (progress, expected_y) in [(0.0, 1014), (0.5, 705), (1.0, 397)] {
        let items = decoded.document.static_image_render_items(
            &document_sources,
            &SkinDrawState {
                play_progress: progress,
                rhythm_timer_ms: Some(0),
                ..SkinDrawState::default()
            },
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                SkinRenderItem::Image { texture: item_texture, rect, uv, .. }
                    if *item_texture == texture
                        && uv.width > 0.0 && uv.height > 0.0
                        && (rect.y - 1.0 + expected_y as f32 / 1080.0 + 42.0 / 1080.0).abs() < 0.001
            )),
            "Antique progress slider should have positive UVs and move to y={expected_y} at {progress}"
        );
    }
}
