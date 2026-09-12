use super::*;

#[test]
fn wmii_lr2_battle_bga_inherits_profile_stretch_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD_LR2/play/FHDPLAY_AC_Battle.lr2skin");
    if !path.is_file() {
        return;
    }
    let loaded = load_skin_document_uncached(
        &path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    let bga = loaded.document.bga.as_ref().expect("BATTLE skin has a BGA");
    let destinations = loaded
        .document
        .destination
        .iter()
        .filter_map(|entry| match entry {
            bmz_render::skin::DestinationListEntry::Single(destination)
                if destination.id == bga.id =>
            {
                Some(destination)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!destinations.is_empty());
    assert!(destinations.iter().all(|destination| destination.stretch == -1));
}

#[test]
fn wmii_lr2_battle_renders_live_gauges_hispeed_and_opponent_score_when_available() {
    use bmz_render::skin::{
        SKIN_REF_BMZ_LR2_GAUGE_2P, SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P, SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P,
        SKIN_REF_BMZ_LR2_HISPEED,
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD_LR2/play/FHDPLAY_AC_Battle.lr2skin");
    if !path.is_file() {
        return;
    }
    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let state = bmz_render::skin::SkinDrawState {
        elapsed_ms: 100_000,
        play_timer_ms: Some(100_000),
        ready_timer_ms: Some(100_000),
        play_screen: true,
        autoplay: true,
        hispeed: 2.5,
        gauge_type: 3,
        opponent_gauge_type: Some(1),
        opponent_gauge: Some(38.0),
        rival_ex_score: Some(789),
        target_ex_score: None,
        ..Default::default()
    };
    for (ref_id, expected_digits, expected_objects) in [
        (SKIN_REF_BMZ_LR2_HISPEED, 3, 2),
        (SKIN_REF_BMZ_LR2_GAUGE_2P, 2, 1),
        (271, 4, 1), // EX SCORE uses an eleven-cell font with blank padding.
    ] {
        let values = decoded
            .document
            .value
            .iter()
            .filter(|value| value.ref_id == ref_id)
            .collect::<Vec<_>>();
        assert_eq!(values.len(), expected_objects);
        for value in values {
            let mut document = decoded.document.clone();
            document.destination.retain(|entry| matches!(entry,
                bmz_render::skin::DestinationListEntry::Single(destination) if destination.id == value.id));
            assert_eq!(
                document.static_render_items(&sources, &state, &Default::default()).len(),
                expected_digits,
                "missing LR2 number {ref_id}"
            );
        }
    }
    for (ref_id, row) in
        [(SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P, 1.0), (SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P, 3.0)]
    {
        let image = decoded.document.image.iter().find(|image| image.ref_id == ref_id).unwrap();
        let mut document = decoded.document.clone();
        document.destination.retain(|entry| matches!(entry,
            bmz_render::skin::DestinationListEntry::Single(destination) if destination.id == image.id));
        let items = document.static_render_items(&sources, &state, &Default::default());
        assert_eq!(items.len(), 1);
        let bmz_render::skin::SkinRenderItem::Image { uv, .. } = &items[0] else {
            panic!("expected gauge image");
        };
        let height = sources[&image.src].source_size.height;
        assert!(
            (uv.y - (image.y as f32 + row * image.h as f32 / image.divy as f32) / height).abs()
                < 0.0001
        );
    }
}

#[test]
fn wmii_fhd_lr2skin_decodes_play_document_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    assert_eq!(decoded.document.name, "WMII FHD play AC");
    assert!(decoded.document.w >= 1920);
    assert!(decoded.document.source.len() >= 10);
    assert!(decoded.document.image.len() >= 100);
    assert!(
        decoded.document.source.iter().any(|source| source.id == "110")
            && decoded.document.source.iter().any(|source| source.id == "111"),
        "expected LR2 black/white reference sources"
    );
    let note = decoded.document.note.as_ref().expect("lr2 play skin should define notes");
    assert!(!note.group.is_empty());
    assert!(decoded.document.gauge.is_some());
    assert!(decoded.document.bga.is_some());
    assert!(
        decoded.sources.len() >= 10,
        "expected WMII sources to decode, got {}; source paths: {:?}; decoded: {:?}",
        decoded.sources.len(),
        decoded.document.source.iter().map(|source| source.path.as_str()).collect::<Vec<_>>(),
        decoded.sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>()
    );
    let black = decoded.sources.iter().find(|source| source.source_id == "110").unwrap();
    let white = decoded.sources.iter().find(|source| source.source_id == "111").unwrap();
    assert_eq!(black.asset.as_ref().unwrap().pixels, vec![0, 0, 0, 255]);
    assert_eq!(white.asset.as_ref().unwrap().pixels, vec![255, 255, 255, 255]);
}

#[test]
fn wmii_fhd_lr2skin_can_be_applied_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
}

#[test]
fn wmii_fhd_lr2skin_produces_static_play_items_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([
        ("GRAPH SIDE".to_string(), "LEFT".to_string()),
        ("Score Graph".to_string(), "On".to_string()),
    ]);
    let decoded =
        decode_beatoraja_skin_with_options(&skin_path, SkinKind::Play, &options, &BTreeMap::new())
            .unwrap();
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let state = bmz_render::skin::SkinDrawState {
        elapsed_ms: 2_000,
        play_timer_ms: Some(2_000),
        ready_timer_ms: Some(2_000),
        ..Default::default()
    };

    let items = decoded.document.static_render_items(
        &sources,
        &state,
        &bmz_render::skin::SkinTextState::default(),
    );
    assert!(!items.is_empty());
}

#[test]
fn wmii_fhd_lr2skin_renders_play_fadeout_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let black_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "110")
        .map(|source| source.texture)
        .expect("WMII black reference source should decode");
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let state = bmz_render::skin::SkinDrawState { fadeout_ms: Some(500), ..Default::default() };

    let items = decoded.document.static_render_items(
        &sources,
        &state,
        &bmz_render::skin::SkinTextState::default(),
    );

    assert!(
        items.iter().any(|item| matches!(
            item,
            bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                if *texture == black_texture
                    && rect.width >= 0.99
                    && rect.height >= 0.99
                    && tint.a > 0.99
        )),
        "expected WMII timer=2 fadeout to draw an opaque fullscreen black image"
    );
}

#[test]
fn wmii_fhd_lr2skin_decodes_auto_judge_button_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let options = BTreeMap::from([("Displayjudge".to_string(), "ON".to_string())]);
    let decoded = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &options,
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
    )
    .unwrap()
    .document;
    let candidates = decoded
        .image
        .iter()
        .filter(|image| image.divx == 1 && image.divy >= 2 && image.h > 0)
        .map(|image| {
            format!(
                "src={} x={} y={} w={} h={} divy={} ref={} act={:?}",
                image.src, image.x, image.y, image.w, image.h, image.divy, image.ref_id, image.act
            )
        })
        .collect::<Vec<_>>();
    let auto_judge = decoded
        .image
        .iter()
        .find(|image| image.act == Some(75) && image.divx == 1 && image.divy >= 2)
        .unwrap_or_else(|| {
            panic!("WMII auto judge button should decode; candidates: {}", candidates.join(" | "))
        });

    assert_eq!(auto_judge.ref_id, 0);
    assert_eq!(auto_judge.click, 2);
    assert_eq!(auto_judge.clickable, Some(false));
    assert!(
        auto_judge.h > 0,
        "WMII auto judge button should keep a positive source height: {auto_judge:?}"
    );
}

#[test]
fn wmii_fhd_lr2skin_renders_ac_bga_frame_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let frame_image = decoded
        .document
        .image
        .iter()
        .find(|image| image.src == "2" && image.x == 1016 && image.y == 1276 && image.w == 389)
        .expect("WMII AC frame image should decode");
    let mut destinations = Vec::new();
    for entry in &decoded.document.destination {
        match entry {
            bmz_render::skin::DestinationListEntry::Single(destination) => {
                destinations.push(destination);
            }
            bmz_render::skin::DestinationListEntry::Conditional {
                destinations: nested, ..
            } => {
                destinations.extend(nested.iter());
            }
        }
    }
    let frame_destination = destinations
        .into_iter()
        .find(|destination| {
            destination.id == frame_image.id
                && destination.op.contains(&33)
                && destination.op.contains(&41)
                && destination.op.contains(&30)
        })
        .expect("WMII AC frame destination should decode");
    assert!(
        frame_destination.dst.len() >= 2,
        "expected WMII AC frame destination keyframes, got {:?}",
        frame_destination.dst
    );
    let frame_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "2")
        .expect("WMII AC frame source should load")
        .texture;
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let state = bmz_render::skin::SkinDrawState {
        elapsed_ms: 2_000,
        play_timer_ms: Some(2_000),
        ready_timer_ms: Some(2_000),
        has_bga: true,
        bga_enabled: true,
        autoplay: true,
        skin_loaded: true,
        ..Default::default()
    };

    let items = decoded.document.static_render_items(
        &sources,
        &state,
        &bmz_render::skin::SkinTextState::default(),
    );
    assert!(
        items.iter().any(|item| matches!(
            item,
            bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                if *texture == frame_texture
                    && (rect.width - 389.0 / 1920.0).abs() < 0.001
                    && tint.a > 0.5
        )),
        "expected WMII AC BGA frame item from source 2; got {items:?}"
    );
}

#[test]
fn wmii_fhd_lr2skin_uses_full_note_lane_region_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let area = decoded
        .document
        .note_lane_area(
            bmz_core::lane::Lane::Scratch,
            bmz_core::lane::KeyMode::K7,
            &decoded.document.enabled_options(),
        )
        .expect("WMII scratch lane area should decode");

    assert!((area.x - 75.0 / 1920.0).abs() < 0.001);
    assert!(
        area.height > 0.65,
        "expected LR2 note.dst to define the full scroll lane height, got {area:?}"
    );
}

#[test]
fn wmii_fhd_lr2skin_maps_note_sources_by_lr2_lane_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let note = decoded.document.note.as_ref().expect("WMII notes should decode");
    let images = decoded.document.image_map();
    let scratch =
        images.get(note.note[7].as_str()).expect("WMII scratch note image should resolve");
    let key1 = images.get(note.note[0].as_str()).expect("WMII key1 note image should resolve");
    let key2 = images.get(note.note[1].as_str()).expect("WMII key2 note image should resolve");

    assert_eq!((scratch.x, scratch.w), (94, 90));
    assert_eq!((key1.x, key1.w), (187, 52));
    assert_eq!((key2.x, key2.w), (241, 40));
}

#[test]
fn wmii_fhd_lr2skin_inserts_notes_marker_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    assert!(
        decoded
            .document
            .all_destinations(&decoded.document.enabled_options())
            .iter()
            .any(|destination| destination.id == "notes"),
        "LR2 play skins should insert the notes marker at the first DST_NOTE command"
    );
}

#[test]
fn wmii_fhd_lr2skin_renders_groove_gauge_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let gauge_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "19")
        .expect("WMII gauge source should load")
        .texture;
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    for gauge_type in [
        bmz_core::clear::GaugeType::AssistEasy,
        bmz_core::clear::GaugeType::Normal,
        bmz_core::clear::GaugeType::Hard,
    ] {
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            gauge: 80.0,
            gauge_max: 100.0,
            gauge_border: 80.0,
            gauge_type: gauge_type as i32,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            &state,
            &bmz_render::skin::SkinTextState::default(),
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == gauge_texture
                        && (rect.x - 54.0 / 1920.0).abs() < 0.001
                        && rect.width > 0.004
                        && tint.a > 0.5
            )),
            "expected WMII groove gauge item from source 19 for {gauge_type:?}; got {items:?}"
        );
    }
}

#[test]
fn wmii_fhd_lr2skin_renders_lift_cover_when_lifted() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    assert!(
        decoded.document.hidden_cover.iter().any(|cover| cover.id.contains("liftcover")
            && cover.disappear_line == 357
            && !cover.is_disappear_line_link_lift),
        "expected LR2 SRC_LIFT to decode as a liftcover hiddenCover"
    );
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let lift_cover = decoded
        .document
        .hidden_cover
        .iter()
        .find(|cover| cover.id.contains("liftcover"))
        .expect("WMII lift cover hiddenCover should decode");
    let lift_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == lift_cover.src)
        .map(|source| source.texture)
        .expect("WMII lift source should decode");
    let state = bmz_render::skin::SkinDrawState {
        elapsed_ms: 2_000,
        play_timer_ms: Some(2_000),
        offset_lift_px: 0,
        ..Default::default()
    };

    let items = decoded.document.static_render_items(
        &sources,
        &state,
        &bmz_render::skin::SkinTextState::default(),
    );

    assert!(
        !items.iter().any(|item| matches!(
            item,
            bmz_render::skin::SkinRenderItem::Image { texture, tint, .. }
                if *texture == lift_texture && tint.a > 0.5
        )),
        "expected WMII LIFT cover to stay hidden while lift offset is zero"
    );

    let lifted_items = decoded.document.static_render_items(
        &sources,
        &bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            offset_lift_px: 200,
            lift: 200.0 / 1080.0,
            lift_enabled: true,
            ..Default::default()
        },
        &bmz_render::skin::SkinTextState::default(),
    );
    assert!(
        lifted_items.iter().any(|item| matches!(
            item,
            bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                if *texture == lift_texture && rect.height < 0.25 && tint.a > 0.5
        )),
        "expected WMII LIFT cover to render clipped once lift offset is active; got {lifted_items:?}"
    );
}

#[test]
fn wmii_fhd_luaskin_renders_lift_cover_when_lifted() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/play7wide.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
    let lift_cover = decoded
        .document
        .lift_cover
        .iter()
        .find(|cover| cover.id.eq_ignore_ascii_case("lift"))
        .unwrap_or_else(|| {
            panic!(
                "WMII Lua lift cover should decode; got {:?}",
                decoded
                    .document
                    .lift_cover
                    .iter()
                    .map(|cover| (&cover.id, &cover.src))
                    .collect::<Vec<_>>()
            )
        });
    let lift_texture = decoded
        .sources
        .iter()
        .find(|source| source.source_id == lift_cover.src)
        .map(|source| source.texture)
        .expect("WMII Lua lift source should decode");
    let sources = decoded
        .sources
        .iter()
        .map(|source| {
            (
                source.source_id.clone(),
                SkinDocumentTexture {
                    source_id: source.source_id.clone(),
                    texture: source.texture,
                    source_size: SkinImageSize {
                        width: source.size.width,
                        height: source.size.height,
                    },
                },
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    let lifted_items = decoded.document.static_render_items(
        &sources,
        &bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            offset_lift_px: 200,
            lift: 200.0 / 1080.0,
            lift_enabled: true,
            ..Default::default()
        },
        &bmz_render::skin::SkinTextState::default(),
    );

    assert!(
        lifted_items.iter().any(|item| matches!(
            item,
            bmz_render::skin::SkinRenderItem::Image { texture, tint, .. }
                if *texture == lift_texture && tint.a > 0.5
        )),
        "expected WMII Lua LIFT cover to render once lift offset is active; got {lifted_items:?}"
    );
}
