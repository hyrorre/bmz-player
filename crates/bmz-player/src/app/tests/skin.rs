use super::*;

#[test]
fn frontend_lua_load_exposes_ir_connection_before_building_objects() {
    let path = std::env::temp_dir().join(format!("bmz-frontend-ir-{}.lua", std::process::id()));
    std::fs::write(
        &path,
        r#"
        local state = require("main_state")
        local skin = { type = 5, imageset = {} }
        if state.option(51) then
            skin.imageset[1] = { id = "ir_lamp", ref = 390, images = {"lamp"} }
        end
        if state.option(50) then
            skin.imageset[1] = { id = "offline_lamp", ref = 390, images = {"lamp"} }
        end
        return skin
    "#,
    )
    .unwrap();
    for (ir_name, expected) in [(Some("Test IR"), "ir_lamp"), (None, "offline_lamp")] {
        let state = lua_runtime_state_for_frontend("Player", ir_name);
        assert_eq!(state.text_values[&2], "Player");
        assert_eq!(state.text_values[&1020], ir_name.unwrap_or_default());
        let loaded = bmz_skin::load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &state,
        )
        .unwrap();
        assert_eq!(loaded.document.imageset.len(), 1);
        assert_eq!(loaded.document.imageset[0].id, expected);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn litone_select_builds_online_ranker_lamps_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    let path = root.join("LITONE9/Select/select.luaskin");
    if !path.is_file() {
        return;
    }
    let context = bmz_skin::SkinPathContext::new(&path, [root]).unwrap();
    let loaded = bmz_skin::load_lua_skin_with_path_context(
        &context,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &lua_runtime_state_for_frontend("Player", Some("Test IR")),
        &BTreeMap::new(),
    )
    .unwrap();
    for index in 1..=10 {
        let lamp = loaded
            .document
            .imageset
            .iter()
            .find(|set| set.id == format!("ir_cleartype{index}"))
            .expect("online LITONE ranker lamps must survive load-time Lua branches");
        assert_eq!(lamp.ref_id, 389 + index);
    }
}

#[test]
fn skin_catalog_refresh_finds_restored_skin_and_reports_invalid_headers() {
    let root = std::env::temp_dir().join(format!(
        "bmz-skin-refresh-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let paths = crate::paths::AppPaths::from_dirs(
        root.join("resources"),
        root.join("data"),
        root.join("cache"),
        root.join("logs"),
    );
    paths.ensure_dirs().unwrap();
    let path = paths.data_dir.join("skins/restored.json");
    assert!(read_skin_header_document(&path, &paths.skin_library_roots()).is_err());
    assert!(scan_skin_catalog(&paths).select.is_empty());
    std::fs::write(&path, "{broken json").unwrap();
    assert!(read_skin_header_document(&path, &paths.skin_library_roots()).is_err());
    std::fs::write(&path, r#"{"type":5,"name":"Restored"}"#).unwrap();
    assert!(read_skin_header_document(&path, &paths.skin_library_roots()).is_ok());
    let catalog = scan_skin_catalog(&paths);
    assert_eq!(catalog.select.len(), 1);
    assert_eq!(catalog.select[0].path, "data:skins/restored.json");
    assert_eq!(catalog.select[0].origin, SkinCandidateOrigin::User);
    let lua = paths.data_dir.join("skins/restored.lua");
    std::fs::write(&lua, "return {type=5, name='Lua'}").unwrap();
    assert!(read_skin_header_document(&lua, &paths.skin_library_roots()).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lua_runtime_offsets_keep_names_distinct_and_runtime_ids_last_wins() {
    let offsets = vec![
        SkinOffsetConfig { name: Some("First".to_string()), id: 42, x: 10, ..Default::default() },
        SkinOffsetConfig { name: Some("Second".to_string()), id: 42, x: 20, ..Default::default() },
    ];
    let state =
        lua_runtime_state_with_skin_offsets(bmz_skin::LuaLoadRuntimeState::default(), &offsets);

    assert_eq!(state.offset_values["First"].x, 10);
    assert_eq!(state.offset_values["Second"].x, 20);
    assert_eq!(state.offset_id_values[&42].x, 20);
}

#[test]
fn play_skin_signature_changes_for_each_preload_generation() {
    let options = BTreeMap::new();
    let files = BTreeMap::new();
    let runtime_state = bmz_skin::LuaLoadRuntimeState::default();
    let first = play_skin_signature(
        KeyMode::K7,
        SessionMode::Normal,
        "play.luaskin",
        &options,
        &files,
        &runtime_state,
        10,
    );
    let same_entry = play_skin_signature(
        KeyMode::K7,
        SessionMode::Normal,
        "play.luaskin",
        &options,
        &files,
        &runtime_state,
        10,
    );
    let next_entry = play_skin_signature(
        KeyMode::K7,
        SessionMode::Normal,
        "play.luaskin",
        &options,
        &files,
        &runtime_state,
        11,
    );

    assert_eq!(first, same_entry);
    assert_ne!(first, next_entry);
}

#[test]
fn skin_video_play_level_number_extracts_digits_without_allocating_label_shapes() {
    assert_eq!(skin_video_play_level_number("12"), 12);
    assert_eq!(skin_video_play_level_number("LV 10+"), 10);
    assert_eq!(skin_video_play_level_number("no level"), 0);
}

#[test]
fn skin_video_difficulty_code_matches_numeric_and_case_insensitive_names() {
    assert_eq!(skin_video_difficulty_code("1"), 1);
    assert_eq!(skin_video_difficulty_code(" normal "), 2);
    assert_eq!(skin_video_difficulty_code("INSANE"), 5);
    assert_eq!(skin_video_difficulty_code("unknown"), 0);
}

#[test]
fn default_skin_note_texture_exists() {
    assert!(default_skin_root().join("note.png").is_file());
    assert!(default_skin_root().join("note-blue.png").is_file());
    assert!(default_skin_root().join("note-red.png").is_file());
    assert!(default_skin_root().join("receptor.png").is_file());
    assert!(default_skin_root().join("receptor-blue.png").is_file());
    assert!(default_skin_root().join("receptor-red.png").is_file());
    assert!(default_skin_root().join("judge-line.png").is_file());
    assert!(default_skin_root().join("gauge-frame.png").is_file());
    assert!(default_skin_root().join("gauge-fill.png").is_file());
    assert!(default_skin_root().join("combo-panel.png").is_file());
    assert!(default_skin_root().join("combo-panel-inactive.png").is_file());
}

#[test]
fn default_skin_texture_catalog_defines_expected_assets() {
    let manifest = default_skin_manifest();

    assert!(manifest.textures.iter().any(|texture| texture.id == 1 && texture.path == "note.png"));
    assert!(
        manifest.textures.iter().any(|texture| texture.id == 2 && texture.path == "note-blue.png")
    );
    assert!(
        manifest.textures.iter().any(|texture| texture.id == 3 && texture.path == "note-red.png")
    );
    assert!(
        manifest.textures.iter().any(|texture| texture.id == 4 && texture.path == "receptor.png")
    );
    assert!(
        manifest
            .textures
            .iter()
            .any(|texture| texture.id == 5 && texture.path == "receptor-blue.png")
    );
    assert!(
        manifest
            .textures
            .iter()
            .any(|texture| texture.id == 6 && texture.path == "receptor-red.png")
    );
    assert!(
        manifest.textures.iter().any(|texture| texture.id == 7 && texture.path == "judge-line.png")
    );
    assert!(
        manifest
            .textures
            .iter()
            .any(|texture| texture.id == 8 && texture.path == "gauge-frame.png")
    );
    assert!(
        manifest.textures.iter().any(|texture| texture.id == 9 && texture.path == "gauge-fill.png")
    );
    assert!(
        manifest
            .textures
            .iter()
            .any(|texture| texture.id == 10 && texture.path == "combo-panel.png")
    );
    assert!(
        manifest
            .textures
            .iter()
            .any(|texture| texture.id == 11 && texture.path == "combo-panel-inactive.png")
    );
    assert!(
        manifest.textures.iter().any(|texture| texture.id == 12 && texture.path == "note-mine.png")
    );
}

#[test]
fn skin_catalog_scan_ignores_lua_parts_files() {
    assert!(is_skin_candidate_file(Path::new("data/skins/ECFN/play/play7.luaskin")));
    assert!(is_skin_candidate_file(Path::new("data/skins/ECFN/play/play7-1p.json")));
    assert!(is_skin_candidate_file(Path::new("data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin")));
    assert!(!is_skin_candidate_file(Path::new("data/skins/ECFN/play/play_parts.lua")));
}

#[test]
fn skin_catalog_rejects_json_without_explicit_skin_type() {
    let root = std::env::temp_dir().join(format!(
        "bmz-player-skin-catalog-json-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let player_data = root.join("songs.json");
    std::fs::write(&player_data, r#"{"20260829":{"songs":[]}}"#).unwrap();
    let valid_skin = root.join("result.json");
    std::fs::write(&valid_skin, r#"{"type":7,"name":"Result"}"#).unwrap();
    let included_skin = root.join("included-result.json");
    let included_document = root.join("result-main.json");
    std::fs::write(&included_skin, r#"{"include":"result-main.json"}"#).unwrap();
    std::fs::write(&included_document, r#"{"type":7.0,"name":"Included Result"}"#).unwrap();
    let untyped_include = root.join("untyped-include.json");
    let untyped_document = root.join("untyped-main.json");
    std::fs::write(&untyped_include, r#"{"include":"untyped-main.json"}"#).unwrap();
    std::fs::write(&untyped_document, r#"{"name":"Not a skin"}"#).unwrap();

    assert!(load_skin_candidate(&root, &player_data, SkinCandidateOrigin::User).is_none());
    let (skin_type, candidate) =
        load_skin_candidate(&root, &valid_skin, SkinCandidateOrigin::User).unwrap();
    assert_eq!(skin_type, 7);
    assert_eq!(candidate.name, "Result");
    let (skin_type, candidate) =
        load_skin_candidate(&root, &included_skin, SkinCandidateOrigin::User).unwrap();
    assert_eq!(skin_type, 7);
    assert_eq!(candidate.name, "Included Result");
    assert!(load_skin_candidate(&root, &untyped_include, SkinCandidateOrigin::User).is_none());
}

#[test]
fn skin_catalog_loads_litone11_result_headers_when_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let skin_root = repo_root.join("data/skins");
    let result_root = skin_root.join("LITONE11/Result");
    let cases = [("result.luaskin", 7), ("course.luaskin", 15)];

    for (file_name, expected_type) in cases {
        let path = result_root.join(file_name);
        if !path.is_file() {
            continue;
        }
        let (skin_type, candidate) =
            load_skin_candidate(&skin_root, &path, SkinCandidateOrigin::Bundled)
                .unwrap_or_else(|| panic!("load LITONE11 catalog candidate: {}", path.display()));
        assert_eq!(skin_type, expected_type, "{}", path.display());
        assert!(candidate.name.contains("LITONE11"), "candidate name: {}", candidate.name);
    }
}

#[test]
fn lr2skin_header_document_exposes_skin_config_defs_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
    if !path.is_file() {
        return;
    }

    let document = load_skin_header_document(&path).expect("load lr2 skin header");

    assert!(document.property.iter().any(|property| property.name == "Displayjudge"));
    assert!(document.filepath.iter().any(|filepath| filepath.name == "GAUGE COLOR"));
    assert!(document.offset.iter().any(|offset| offset.id == 1));
}

#[test]
fn skin_catalog_loads_rm_skin_lua_headers_when_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let skin_root = repo_root.join("data/skins");
    let root = skin_root.join("Rmz-skin");
    let cases = [
        ("play4main.luaskin", BMZ_SKIN_TYPE_PLAY_4KEYS),
        ("play5main.luaskin", 1),
        ("play6main.luaskin", BMZ_SKIN_TYPE_PLAY_6KEYS),
        ("play7main.luaskin", 0),
        ("play8main.luaskin", BMZ_SKIN_TYPE_PLAY_8KEYS),
        ("play9main.luaskin", 4),
    ];

    for (file_name, expected_type) in cases {
        let path = root.join(file_name);
        if !path.is_file() {
            continue;
        }

        let (skin_type, candidate) =
            load_skin_candidate(&skin_root, &path, SkinCandidateOrigin::Bundled)
                .expect("load Rm-skin catalog candidate");

        assert_eq!(skin_type, expected_type, "{}", path.display());
        assert_eq!(candidate.path, format!("resource:skins/Rmz-skin/{file_name}"));
        assert_eq!(candidate.origin, SkinCandidateOrigin::Bundled);
        assert!(candidate.name.contains("Rm-skin"), "candidate name: {}", candidate.name);
    }
}

#[test]
fn skin_catalog_maps_play_key_modes_by_exact_skin_type() {
    let mut catalog = SkinCatalog::default();
    push_skin_candidate(
        &mut catalog,
        0,
        SkinCandidate {
            name: "Seven".to_string(),
            path: "data/skins/example/play7.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        1,
        SkinCandidate {
            name: "Five".to_string(),
            path: "data/skins/example/play5.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        BMZ_SKIN_TYPE_PLAY_4KEYS,
        SkinCandidate {
            name: "Four".to_string(),
            path: "data/skins/example/play4.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        BMZ_SKIN_TYPE_PLAY_6KEYS,
        SkinCandidate {
            name: "Six".to_string(),
            path: "data/skins/example/play6.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        BMZ_SKIN_TYPE_PLAY_8KEYS,
        SkinCandidate {
            name: "Eight".to_string(),
            path: "data/skins/example/play8.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        2,
        SkinCandidate {
            name: "Fourteen".to_string(),
            path: "data/skins/example/play14.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        3,
        SkinCandidate {
            name: "Ten".to_string(),
            path: "data/skins/example/play10.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        4,
        SkinCandidate {
            name: "Nine".to_string(),
            path: "data/skins/example/play9.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        12,
        SkinCandidate {
            name: "Battle Seven".to_string(),
            path: "data/skins/example/battle7.lr2skin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        13,
        SkinCandidate {
            name: "Battle Five".to_string(),
            path: "data/skins/example/battle5.lr2skin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );
    push_skin_candidate(
        &mut catalog,
        15,
        SkinCandidate {
            name: "Course Result".to_string(),
            path: "data/skins/example/course-result.luaskin".to_string(),
            origin: SkinCandidateOrigin::User,
        },
    );

    assert_eq!(catalog.play4.len(), 1);
    assert_eq!(catalog.play5.len(), 1);
    assert_eq!(catalog.play6.len(), 1);
    assert_eq!(catalog.play7.len(), 1);
    assert_eq!(catalog.play8.len(), 1);
    assert_eq!(catalog.play9.len(), 1);
    assert_eq!(catalog.play10.len(), 1);
    assert_eq!(catalog.play14.len(), 1);
    assert_eq!(catalog.battle5.len(), 1);
    assert_eq!(catalog.battle7.len(), 1);
    assert_eq!(catalog.result.len(), 0);
    assert_eq!(catalog.course_result.len(), 1);
    assert_eq!(catalog.play4[0].path, "data/skins/example/play4.luaskin");
    assert_eq!(catalog.play5[0].path, "data/skins/example/play5.luaskin");
    assert_eq!(catalog.play6[0].path, "data/skins/example/play6.luaskin");
    assert_eq!(catalog.play7[0].path, "data/skins/example/play7.luaskin");
    assert_eq!(catalog.play8[0].path, "data/skins/example/play8.luaskin");
    assert_eq!(catalog.play9[0].path, "data/skins/example/play9.luaskin");
    assert_eq!(catalog.play10[0].path, "data/skins/example/play10.luaskin");
    assert_eq!(catalog.play14[0].path, "data/skins/example/play14.luaskin");
    assert_eq!(catalog.battle5[0].path, "data/skins/example/battle5.lr2skin");
    assert_eq!(catalog.battle7[0].path, "data/skins/example/battle7.lr2skin");
    assert_eq!(catalog.course_result[0].path, "data/skins/example/course-result.luaskin");
}

#[test]
fn skin_catalog_loads_modern_chic_headers_when_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let skin_root = repo_root.join("data/skins");
    let root = skin_root.join("ModernChic");
    if !root.is_dir() {
        return;
    }
    let cases = [
        ("musicselect.luaskin", 5),
        ("decide.luaskin", 6),
        ("play5_hw.luaskin", 1),
        ("play7_hw.luaskin", 0),
        ("play10_hw.luaskin", 3),
        ("play14_hw.luaskin", 2),
        ("result.luaskin", 7),
        ("course.luaskin", 15),
    ];

    for (file_name, expected_type) in cases {
        let path = root.join(file_name);
        let loaded = bmz_skin::load_lua_skin_header_value(&path)
            .unwrap_or_else(|error| panic!("load {} header: {error:#}", path.display()));
        let document: SkinDocument = serde_json::from_value(loaded.value)
            .unwrap_or_else(|error| panic!("decode {} header: {error:#}", path.display()));
        assert_eq!(document.skin_type, expected_type, "{}", path.display());

        let (skin_type, candidate) =
            load_skin_candidate(&skin_root, &path, SkinCandidateOrigin::Bundled)
                .unwrap_or_else(|| panic!("load {} catalog candidate", path.display()));
        assert_eq!(skin_type, expected_type, "{}", path.display());
        assert!(candidate.name.contains("ModernChic"), "candidate name: {}", candidate.name);
    }
}
