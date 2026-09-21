use super::*;

#[test]
fn lua_cross_package_source_decodes_with_explicit_library_root() {
    let root = unique_test_dir("lua-cross-package-source").join("skins");
    let entry_dir = root.join("GenericTheme-master/play");
    let hub_parts = root.join("Hub/parts");
    fs::create_dir_all(&entry_dir).unwrap();
    fs::create_dir_all(&hub_parts).unwrap();
    let entry = entry_dir.join("Hub_play7.luaskin");
    fs::write(
        &entry,
        r#"
            return {
                type = 0,
                source = {{ id = "hub-test", path = "../../Hub/parts/sample.png" }},
                image = {{ id = "hub-image", src = "hub-test", x = 0, y = 0, w = 1, h = 1 }},
                destination = {{
                    id = "hub-image",
                    dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                }}
            }
        "#,
    )
    .unwrap();
    let bundled_png =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default/note.png");
    fs::copy(&bundled_png, hub_parts.join("sample.png")).unwrap();

    let options = BTreeMap::new();
    let files = BTreeMap::new();
    let runtime_state = LuaLoadRuntimeState::default();
    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        skin_path: &entry,
        kind: SkinKind::Play,
        options: &options,
        files: &files,
        runtime_state: &runtime_state,
        library_roots: std::slice::from_ref(&root),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
    })
    .unwrap();

    let context = SkinPathContext::new(&entry, [root]).unwrap();
    let source = decoded.sources.iter().find(|source| source.source_id == "hub-test").unwrap();
    assert_eq!(source.path, context.resolve_file("../../Hub/parts/sample.png").unwrap());
    assert!(source.asset.is_some());

    assert_eq!(
        resolve_skin_audio_path_with_context(
            context.entry_dir(),
            Some(&context),
            "../../Hub/parts/sample.png",
        )
        .unwrap(),
        source.path
    );
}

#[test]
fn select_lua_skins_decode_with_explicit_library_root_when_available() {
    let skin_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    let cases = [
        ("mz-select/music_select.luaskin", "m-select"),
        ("Luxez-Flat/music_select.luaskin", "Luxez-Flat"),
        ("ModernChic/musicselect.luaskin", "ModernChic"),
    ];
    let options = BTreeMap::new();
    let files = BTreeMap::new();
    let runtime_state = LuaLoadRuntimeState::default();

    for (relative, label) in cases {
        let skin_path = skin_root.join(relative);
        if !skin_path.is_file() {
            continue;
        }
        let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
            skin_path: &skin_path,
            kind: SkinKind::Select,
            options: &options,
            files: &files,
            runtime_state: &runtime_state,
            library_roots: std::slice::from_ref(&skin_root),
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        })
        .unwrap_or_else(|error| panic!("{label} should decode with app path context: {error:#}"));

        assert!(
            !decoded.document.destination.is_empty(),
            "{label} should not decode into an empty select skin"
        );
        if relative.starts_with("mz-select/") || relative.starts_with("Luxez-Flat/") {
            let text = decoded
                .document
                .text
                .iter()
                .find(|text| text.id == "bmz_select_mode")
                .expect("bundled filter must display the actual BMZ mode name");
            assert!(
                decoded.fonts.iter().any(|font| font.stored_id == text.font && font.data.is_some())
            );
            assert!(
                !decoded
                    .document
                    .image
                    .iter()
                    .any(|image| image.id == "default_modechange_modeset")
            );
            assert!(
                !decoded
                    .document
                    .imageset
                    .iter()
                    .any(|image| image.id == "default_modechange_modeset")
            );
            let frame_texture = decoded
                .sources
                .iter()
                .find(|source| source.source_id == "src-default_modechange_frame")
                .map(|source| source.texture)
                .expect("bundled filter must decode a frame image source");
            let mode_parts_texture = decoded
                .sources
                .iter()
                .find(|source| source.source_id == "src-default_modechange_parts")
                .map(|source| source.texture)
                .expect("bundled filter must decode its hover image source");
            let expected = if relative.starts_with("mz-select/") {
                (1305.0, 990.0, 150.0, 50.0)
            } else {
                (977.0, 1034.0, 135.0, 35.0)
            };
            let expected_mode_font_size =
                if relative.starts_with("mz-select/") { 26.0 } else { 25.0 };
            let textures = decoded.sources.iter().map(|source| SkinDocumentTexture {
                source_id: source.source_id.clone(),
                texture: source.texture,
                source_size: SkinImageSize { width: source.size.width, height: source.size.height },
            });
            let context = SkinContext::from_manifest_and_document(
                bmz_render::skin::default_skin_manifest(),
                decoded.document,
                textures,
            );
            for mode in ["ALL", "7K", "14K", "9K", "5K", "10K", "4K", "6K", "8K"] {
                for fraction in [0.1, 0.9] {
                    let x = (expected.0 + expected.2 * fraction) / 1920.0;
                    let y = 1.0 - (expected.1 + expected.3 / 2.0) / 1080.0;
                    let snapshot = SelectSnapshot {
                        select_mode: mode.to_string(),
                        mouse_position: Some((x, y)),
                        ..SelectSnapshot::default()
                    };
                    let items = context.select_document_items(&snapshot);
                    let mode_style = items.iter().find_map(|item| match item {
                        SkinRenderItem::Text { text, style, .. } if text == mode => Some(style),
                        _ => None,
                    });
                    assert!(mode_style.is_some(), "{label} must render {mode}");
                    let mode_style = mode_style.expect("mode text style must be available");
                    assert!(
                        (mode_style.size - expected_mode_font_size / 1080.0).abs() < 0.0001,
                        "{label} must use the matched mode font size for {mode}: got {}",
                        mode_style.size * 1080.0
                    );
                    let mode_rect = |item: &&SkinRenderItem| {
                        matches!(
                            item,
                            SkinRenderItem::Image { texture, rect, .. }
                                if *texture == frame_texture
                                    && (rect.x - expected.0 / 1920.0).abs() < 0.0001
                                    && (rect.width - expected.2 / 1920.0).abs() < 0.0001
                        )
                    };
                    assert_eq!(
                        items.iter().filter(mode_rect).count(),
                        1,
                        "{label} must render the filter frame"
                    );
                    let outside = context.select_document_items(&SelectSnapshot {
                        select_mode: mode.to_string(),
                        mouse_position: Some((0.0, 0.0)),
                        ..SelectSnapshot::default()
                    });
                    assert_eq!(
                        outside.iter().filter(mode_rect).count(),
                        1,
                        "{label} must keep the filter frame outside hover"
                    );
                    let hover_image_count = |render_items: &[SkinRenderItem]| {
                        render_items
                            .iter()
                            .filter(|item| {
                                matches!(
                                    item,
                                    SkinRenderItem::Image { texture, rect, .. }
                                        if *texture == mode_parts_texture
                                            && (rect.x - expected.0 / 1920.0).abs() < 0.0001
                                            && (rect.width - expected.2 / 1920.0).abs() < 0.0001
                                )
                            })
                            .count()
                    };
                    assert!(
                        hover_image_count(&items) > hover_image_count(&outside),
                        "{label} must add the filter hover image while hovered"
                    );
                    let hit = context
                        .select_click_hit(&snapshot, x, y)
                        .expect("filter must remain clickable across its full width");
                    assert_eq!(
                        hit.target,
                        bmz_render::skin::SkinClickTarget::Event { event_id: 11, click: 2 }
                    );
                    assert!((hit.rect.x - expected.0 / 1920.0).abs() < 0.0001);
                    assert!((hit.rect.width - expected.2 / 1920.0).abs() < 0.0001);
                }
            }
        }
    }
}

#[test]
fn bundled_select_ln_force_badge_tracks_setting_without_changing_clicks() {
    let skin_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    for (relative, x, y, width, height) in [
        ("mz-select/music_select.luaskin", 1665.0, 990.0, 150.0, 50.0),
        ("Luxez-Flat/music_select.luaskin", 1151.0, 1004.0, 115.0, 35.0),
    ] {
        let path = skin_root.join(relative);
        if !path.is_file() {
            continue;
        }
        let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
        let text =
            decoded.document.text.iter().find(|text| text.id == "bmz_ln_force_badge").unwrap();
        assert!(
            decoded.fonts.iter().any(|font| font.stored_id == text.font && font.data.is_some())
        );
        let textures = decoded.sources.iter().map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: SkinImageSize { width: source.size.width, height: source.size.height },
        });
        let context = SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            decoded.document,
            textures,
        );
        for (index, mode) in
            ["AUTO(LN)", "AUTO(CN)", "AUTO(HCN)", "FORCE(LN)", "FORCE(CN)", "FORCE(HCN)"]
                .into_iter()
                .enumerate()
        {
            let snapshot = SelectSnapshot {
                ln_policy_setting_index: index,
                select_ln_mode: mode.to_string(),
                ..SelectSnapshot::default()
            };
            let items = context.select_document_items(&snapshot);
            let badges = items
                .iter()
                .filter(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "FORCE"))
                .count();
            assert_eq!(badges, usize::from(index >= 3), "{relative}: {mode}");
            for fraction in [0.1, 0.9] {
                let hit = context
                    .select_click_hit(
                        &snapshot,
                        (x + width * fraction) / 1920.0,
                        1.0 - (y + height / 2.0) / 1080.0,
                    )
                    .unwrap();
                assert_eq!(
                    hit.target,
                    bmz_render::skin::SkinClickTarget::Event { event_id: 308, click: 2 }
                );
            }
        }
    }
}

#[test]
fn wmii_fhd_lua_visual_offset_preserves_json_digit_and_blank_padding_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/play7wide.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::from([("Display Judge Panel".to_string(), "On".to_string())]),
        &BTreeMap::new(),
    )
    .unwrap();
    let visual_offset = decoded
        .document
        .value
        .iter()
        .find(|value| value.id == "judgetiming")
        .expect("expected WMII Lua visual-offset number");

    assert_eq!(visual_offset.ref_id, 12);
    assert_eq!((visual_offset.divx, visual_offset.divy), (12, 2));
    assert_eq!(visual_offset.digit, 3, "Lua/JSON digit must not gain a sign cell");
    assert_eq!(visual_offset.zeropadding, 2);
    assert_eq!(visual_offset.padding, 0);
}
