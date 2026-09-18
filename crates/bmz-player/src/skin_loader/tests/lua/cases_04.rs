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
            let expected = if relative.starts_with("mz-select/") {
                (1305.0, 990.0, 150.0, 50.0)
            } else {
                (977.0, 1034.0, 135.0, 35.0)
            };
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
                    assert!(
                        context.select_document_items(&snapshot).iter().any(
                            |item| matches!(item, SkinRenderItem::Text { text, .. } if text == mode)
                        ),
                        "{label} must render {mode}"
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
