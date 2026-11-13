use super::*;

#[test]
fn bundled_default_json_skin_documents_decode() {
    let app_paths = test_app_paths();
    for (kind, expected_type) in
        [(SkinKind::Select, 5), (SkinKind::Decide, 6), (SkinKind::Result, 7)]
    {
        let path = default_skin_document_path_from_paths(&app_paths, kind);
        let decoded = decode_beatoraja_skin(&path, kind)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));
        assert_eq!(decoded.document.skin_type, expected_type);
        assert!(!decoded.sources.is_empty(), "{} has no image sources", path.display());
    }

    for (key_mode, expected_type) in [
        (KeyMode::K4, 22),
        (KeyMode::K5, 1),
        (KeyMode::K6, 23),
        (KeyMode::K7, 0),
        (KeyMode::K8, 24),
        (KeyMode::K9, 4),
        (KeyMode::K10, 3),
        (KeyMode::K14, 2),
    ] {
        let path = default_play_skin_document_path_from_paths(&app_paths, key_mode);
        let decoded = decode_beatoraja_skin(&path, SkinKind::Play)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));
        assert_eq!(decoded.document.skin_type, expected_type);
        assert!(decoded.document.note.is_some(), "{} has no note definition", path.display());
        assert!(
            decoded.document.note.as_ref().is_some_and(|note| !note.group.is_empty()),
            "{} has no bar line group",
            path.display()
        );
        assert!(
            destination_ids(&decoded.document).contains("keybeam_img"),
            "{} has no keybeam destination",
            path.display()
        );
        assert!(!decoded.sources.is_empty(), "{} has no image sources", path.display());
    }
}

#[test]
fn bundled_default_select_displays_all_session_modes() {
    let app_paths = test_app_paths();
    let path = default_skin_document_path_from_paths(&app_paths, SkinKind::Select);
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select)
        .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));

    for (id, label) in [
        ("option_assist_0", "NORMAL"),
        ("option_assist_1", "PRACTICE"),
        ("option_assist_2", "AUTOPLAY"),
        ("option_assist_3", "AUTO BATTLE"),
        ("option_assist_4", "G-BATTLE"),
    ] {
        assert!(
            decoded.document.text.iter().any(|text| text.id == id && text.constant_text == label),
            "{} should decode {id} text",
            path.display()
        );
    }

    for index in 0..5 {
        let draw = format!("event_index({SKIN_REF_BMZ_SELECT_SESSION_MODE}) == {index}");
        assert!(decoded.document.destination.iter().any(|entry| matches!(
            entry,
            DestinationListEntry::Single(destination)
                if destination.id == "panel_img" && destination.draw == draw
        )));
    }
}

#[test]
fn bundled_default_select_labels_hs_fix_in_event_index_order() {
    let app_paths = test_app_paths();
    let path = default_skin_document_path_from_paths(&app_paths, SkinKind::Select);
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select)
        .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));

    for (id, label) in [
        ("option_hsfix_0", "OFF"),
        ("option_hsfix_1", "START BPM"),
        ("option_hsfix_2", "MAX BPM"),
        ("option_hsfix_3", "MAIN BPM"),
        ("option_hsfix_4", "MIN BPM"),
    ] {
        assert!(
            decoded.document.text.iter().any(|text| text.id == id && text.constant_text == label),
            "{} should decode {id} as {label}",
            path.display()
        );
    }
}

#[test]
fn bundled_default_play_and_result_display_extended_arrange_labels() {
    let app_paths = test_app_paths();
    for (path, kind, ids) in [
        (
            default_play_skin_document_path_from_paths(&app_paths, KeyMode::K7),
            SkinKind::Play,
            [
                ("play_arrange_1p_f", "1P F-RANDOM", "event_index(344) == 10"),
                ("play_arrange_1p_mf", "1P MF-RANDOM", "event_index(344) == 11"),
                ("play_arrange_2p_f", "2P F-RANDOM", "event_index(345) == 10"),
                ("play_arrange_2p_mf", "2P MF-RANDOM", "event_index(345) == 11"),
            ],
        ),
        (
            default_skin_document_path_from_paths(&app_paths, SkinKind::Result),
            SkinKind::Result,
            [
                ("result_arrange_1p_f", "1P F-RANDOM", "event_index(344) == 10"),
                ("result_arrange_1p_mf", "1P MF-RANDOM", "event_index(344) == 11"),
                ("result_arrange_2p_f", "2P F-RANDOM", "event_index(345) == 10"),
                ("result_arrange_2p_mf", "2P MF-RANDOM", "event_index(345) == 11"),
            ],
        ),
    ] {
        let decoded = decode_beatoraja_skin(&path, kind)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error:#}", path.display()));
        for (id, label, draw) in ids {
            assert!(
                decoded
                    .document
                    .text
                    .iter()
                    .any(|text| text.id == id && text.constant_text == label),
                "{} should decode {id} text",
                path.display()
            );
            assert!(decoded.document.destination.iter().any(|entry| matches!(
                entry,
                DestinationListEntry::Single(destination)
                    if destination.id == id && destination.draw == draw
            )));
        }
    }
}

#[test]
fn enabled_options_includes_unselected_property_default_for_real_skin() {
    // 実際の Starseeker play7.luaskin で「スコアグラフ=On」のみ選択した時、
    // 未選択の「プレーサイド」のデフォルト (1P=920) と「スコアグラフ=On」(901)
    // の両方が enabled_options に入ることを確認する。
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Starseeker/play/play7.luaskin");
    if !skin_path.is_file() {
        eprintln!("skipping: skin not present at {}", skin_path.display());
        return;
    }
    let mut selections = BTreeMap::new();
    selections.insert("スコアグラフ".to_string(), "On".to_string());

    let loaded = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &selections,
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
    )
    .expect("load skin document");
    let ops = enabled_options_from_selections(&loaded.document, &selections);
    assert!(ops.contains(&901), "expected 901 in ops, got {ops:?}");
    assert!(ops.contains(&920), "expected 920 (1P default) in ops, got {ops:?}");
}

#[test]
fn enabled_options_rejects_stale_numeric_selection() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "property": [
                    {
                        "name": "Graph",
                        "def": "AC",
                        "item": [
                            { "name": "AC", "op": 922 },
                            { "name": "TYPE-M", "op": 923 }
                        ]
                    }
                ]
            }
            "#,
    )
    .unwrap();
    let selections = BTreeMap::from([("Graph".to_string(), "999".to_string())]);

    assert_eq!(enabled_options_from_selections(&document, &selections), vec![922]);
}

#[test]
fn default_skin_can_be_applied_to_renderer() {
    let mut renderer = Renderer::default();

    apply_default_skin_from_paths(&mut renderer, &test_app_paths()).unwrap();
    assert!(renderer.play_skin_document().is_some());
}

#[test]
fn beatoraja_default_json_skin_can_be_applied_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/beatoraja/skin/default/play7.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_json_skin_for_kind(
        &mut renderer,
        &test_app_paths(),
        &skin_path,
        SkinKind::Play,
    )
    .unwrap();
}

#[test]
fn beatoraja_default_select_json_skin_can_be_applied_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/beatoraja/skin/default/select.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_json_skin_for_kind(
        &mut renderer,
        &test_app_paths(),
        &skin_path,
        SkinKind::Select,
    )
    .unwrap();
}

#[test]
fn beatoraja_default_result_json_skin_can_be_applied_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/beatoraja/skin/default/result.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();

    apply_beatoraja_json_skin_for_kind(
        &mut renderer,
        &test_app_paths(),
        &skin_path,
        SkinKind::Result,
    )
    .unwrap();
}

#[test]
#[ignore = "manual select skin profiling helper"]
fn profile_rgba_frame_clone_cost() {
    let width = 1920_usize;
    let height = 1080_usize;
    let rgba = vec![127_u8; width * height * 4];
    let frames = 240;

    let clone_start = Instant::now();
    let mut cloned_len = 0_usize;
    for _ in 0..frames {
        let cloned = black_box(rgba.clone());
        cloned_len += black_box(cloned.len());
    }
    let clone_elapsed = clone_start.elapsed();

    let borrow_start = Instant::now();
    let mut borrowed_len = 0_usize;
    for _ in 0..frames {
        borrowed_len += black_box(rgba.as_slice()).len();
    }
    let borrow_elapsed = borrow_start.elapsed();

    assert_eq!(cloned_len, borrowed_len);
    println!(
        "profile_rgba_frame_clone_cost frames={frames} bytes={} avg_clone_ms={:.3} avg_borrow_ms={:.6}",
        rgba.len(),
        clone_elapsed.as_secs_f64() * 1000.0 / frames as f64,
        borrow_elapsed.as_secs_f64() * 1000.0 / frames as f64
    );
}

#[test]
fn play_skin_selection_for_returns_per_mode_fields() {
    let mut skin = SkinConfig {
        play4: "skin4.json".to_string(),
        play5: "skin5.json".to_string(),
        play6: "skin6.json".to_string(),
        play7: "skin7.json".to_string(),
        play8: "skin8.json".to_string(),
        play9: "skin9.json".to_string(),
        play10: "skin10.json".to_string(),
        play14: "skin14.json".to_string(),
        battle5: "battle5.json".to_string(),
        battle7: "battle7.json".to_string(),
        ..SkinConfig::default()
    };
    skin.play4_options.insert("g".to_string(), "r".to_string());
    skin.play5_options.insert("a".to_string(), "x".to_string());
    skin.play6_options.insert("f".to_string(), "q".to_string());
    skin.play7_options.insert("b".to_string(), "y".to_string());
    skin.play8_options.insert("h".to_string(), "n".to_string());
    skin.play9_options.insert("e".to_string(), "p".to_string());
    skin.play10_files.insert("c".to_string(), "z.png".to_string());
    skin.play14_files.insert("d".to_string(), "w.png".to_string());
    skin.play7_offsets.push(SkinOffsetConfig { id: 30, h: 7, ..Default::default() });
    skin.play14_offsets.push(SkinOffsetConfig { id: 30, h: 14, ..Default::default() });

    let s4 = play_skin_selection_for(&skin, KeyMode::K4);
    assert_eq!(s4.path, "skin4.json");
    assert!(s4.options.contains_key("g"));

    let s5 = play_skin_selection_for(&skin, KeyMode::K5);
    assert_eq!(s5.path, "skin5.json");
    assert!(s5.options.contains_key("a"));

    let s6 = play_skin_selection_for(&skin, KeyMode::K6);
    assert_eq!(s6.path, "skin6.json");
    assert!(s6.options.contains_key("f"));

    let s7 = play_skin_selection_for(&skin, KeyMode::K7);
    assert_eq!(s7.path, "skin7.json");
    assert!(s7.options.contains_key("b"));
    assert_eq!(s7.offsets[0].h, 7);

    let s8 = play_skin_selection_for(&skin, KeyMode::K8);
    assert_eq!(s8.path, "skin8.json");
    assert!(s8.options.contains_key("h"));

    let s9 = play_skin_selection_for(&skin, KeyMode::K9);
    assert_eq!(s9.path, "skin9.json");
    assert!(s9.options.contains_key("e"));

    let s10 = play_skin_selection_for(&skin, KeyMode::K10);
    assert_eq!(s10.path, "skin10.json");
    assert!(s10.files.contains_key("c"));

    let s14 = play_skin_selection_for(&skin, KeyMode::K14);
    assert_eq!(s14.path, "skin14.json");
    assert!(s14.files.contains_key("d"));
    assert_eq!(s14.offsets[0].h, 14);

    let battle5 = play_skin_selection_for_session(&skin, KeyMode::K5, SessionMode::AutoplayBattle);
    assert_eq!(battle5.path, "battle5.json");
    assert_eq!(battle5.key_mode, KeyMode::K5);
    let battle7 = play_skin_selection_for_session(&skin, KeyMode::K7, SessionMode::AutoplayBattle);
    assert_eq!(battle7.path, "battle7.json");
    assert_eq!(battle7.key_mode, KeyMode::K7);
    let g_battle7 = play_skin_selection_for_session(&skin, KeyMode::K7, SessionMode::GBattle);
    assert_eq!(g_battle7.path, "battle7.json");
    assert_eq!(g_battle7.key_mode, KeyMode::K7);

    skin.battle5.clear();
    skin.battle7.clear();
    let fallback5 =
        play_skin_selection_for_session(&skin, KeyMode::K5, SessionMode::AutoplayBattle);
    assert_eq!(fallback5.path, "skin5.json");
    assert_eq!(fallback5.key_mode, KeyMode::K5);
    assert!(fallback5.options.contains_key("a"));
    let fallback7 = play_skin_selection_for_session(&skin, KeyMode::K7, SessionMode::GBattle);
    assert_eq!(fallback7.path, "skin7.json");
    assert_eq!(fallback7.key_mode, KeyMode::K7);
    assert!(fallback7.options.contains_key("b"));

    let practice7 = play_skin_selection_for_session(&skin, KeyMode::K7, SessionMode::Practice);
    assert_eq!(practice7.path, "skin7.json");
    assert!(practice7.options.contains_key("b"));
    assert_eq!(
        play_skin_selection_for_session(&skin, KeyMode::K14, SessionMode::Normal).path,
        "skin14.json"
    );
}

#[test]
fn apply_skin_from_config_rejects_toml_skin_directory() {
    let mut renderer = Renderer::default();
    let app_paths = test_app_paths();
    let path = app_paths.default_skin_root();

    let error = apply_skin_from_config(&mut renderer, &app_paths, path.to_str().unwrap())
        .unwrap_err()
        .to_string();

    assert!(error.contains("BMZ TOML skin directories are no longer supported"), "{error}");
}
