use super::*;

#[test]
fn default_skin_root_contains_json_documents() {
    let root = default_skin_root();
    for file_name in ["select.json", "decide.json", "result.json", "play7.json"] {
        assert!(root.join(file_name).is_file(), "missing bundled default {file_name}");
    }
}

#[test]
fn skin_audio_path_stays_inside_skin_root() {
    let root = unique_test_dir("bmz-skin-audio-path");
    fs::create_dir_all(root.join("parts")).unwrap();
    fs::write(root.join("parts/bgm.ogg"), []).unwrap();

    let resolved = resolve_skin_audio_path(&root, "parts/bgm.ogg").unwrap();
    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("bgm.ogg"));
    assert!(resolve_skin_audio_path(&root, "../outside.ogg").is_none());
    assert!(
        resolve_skin_audio_path(&root, root.join("parts/bgm.ogg").to_string_lossy().as_ref())
            .is_none()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn substitute_filepath_choice_replaces_wildcard_in_asset_path() {
    let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
    let mut files = BTreeMap::new();
    files.insert("レーザー".to_string(), "veryshort".to_string());

    let result = substitute_filepath_choice("custom/laser/*/main.png", &filepaths, &files);
    assert_eq!(result.as_deref(), Some("custom/laser/veryshort/main.png"));
}

#[test]
fn substitute_filepath_choice_strips_def_suffix_from_selection() {
    let filepaths = vec![filepath_def("icon", "icon-*.png", "")];
    let mut files = BTreeMap::new();
    files.insert("icon".to_string(), "icon-blue.png".to_string());

    let result = substitute_filepath_choice("icon-*.png", &filepaths, &files);
    assert_eq!(result.as_deref(), Some("icon-blue.png"));
}

#[test]
fn resolve_skin_source_accepts_beatoraja_filename_selection() {
    let root = unique_test_dir("bmz-json-source-filename");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/default.png"), []).unwrap();
    std::fs::write(root.join("parts/blue.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
    )
    .unwrap();
    let files = BTreeMap::from([("Parts".to_string(), "default.png".to_string())]);

    let resolved = resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
}

#[test]
fn wildcard_source_with_context_falls_back_to_default_file_stem() {
    let root = unique_test_dir("bmz-skin-wildcard-default-stem");
    fs::create_dir_all(root.join("system")).unwrap();
    fs::create_dir_all(root.join("customize")).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let entry = root.join("system/main.luaskin");
    fs::write(&entry, []).unwrap();
    fs::write(root.join("customize/default.png"), []).unwrap();
    fs::write(root.join("customize/blue.png"), []).unwrap();
    let context = SkinPathContext::new(&entry, [root.clone()]).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"{"filepath":[{"name":"Background","path":"../customize/*","def":"default"}]}"#,
    )
    .unwrap();
    for (choice, expected) in [("default", "default.png"), ("blue.png", "blue.png")] {
        let files = BTreeMap::from([("Background".to_string(), choice.to_string())]);
        let resolved = resolve_json_skin_source_path_with_context(
            entry.parent().unwrap(),
            Some(&context),
            "../customize/*",
            &document,
            &files,
        )
        .unwrap();
        assert_eq!(resolved, root.join("customize").join(expected));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn resolve_skin_source_still_accepts_legacy_relative_selection() {
    let root = unique_test_dir("bmz-json-source-relative");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/default.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png" }
                ]
            }
            "#,
    )
    .unwrap();
    let files = BTreeMap::from([("Parts".to_string(), "parts/default.png".to_string())]);

    let resolved = resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
}

#[test]
fn substitute_filepath_choice_returns_none_when_prefix_mismatch() {
    let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
    let mut files = BTreeMap::new();
    files.insert("レーザー".to_string(), "custom/laser/veryshort".to_string());

    // asset の prefix が定義と一致しない
    let result = substitute_filepath_choice("other/path/*.png", &filepaths, &files);
    assert_eq!(result, None);
}

#[test]
fn substitute_filepath_choice_returns_none_when_no_selection() {
    let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
    let files: BTreeMap<String, String> = BTreeMap::new();

    let result = substitute_filepath_choice("custom/laser/*/main.png", &filepaths, &files);
    assert_eq!(result, None);
}

#[test]
fn ecfn_select_lua_skin_decodes_movie_source_first_frame_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/select/select.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin_with_options(
        &skin_path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let mv = decoded.sources.iter().find(|source| source.source_id == "mv").unwrap();

    let mv_path = mv.path.to_string_lossy().replace('\\', "/");
    assert!(mv_path.ends_with("mv/default.mp4"));
    let asset = mv.asset.as_ref().expect("movie first frame should decode");
    assert!(asset.width > 0);
    assert!(asset.height > 0);
    assert_eq!(asset.pixels.len(), asset.width as usize * asset.height as usize * 4);
}

#[test]
fn ecfn_play7_uses_default_filepaths_when_defs_are_missing() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

    for (source_id, suffix) in
        [("6", "laser/default.png"), ("7", "notes/default.png"), ("12", "lanecover/default.png")]
    {
        let source = decoded
            .sources
            .iter()
            .find(|source| source.source_id == source_id)
            .unwrap_or_else(|| panic!("ECFN source {source_id} should decode"));
        let path = source.path.to_string_lossy().replace('\\', "/");
        assert!(
            path.ends_with(suffix),
            "ECFN source {source_id} should resolve to {suffix}, got {path}"
        );
    }
}

#[test]
fn starseeker_frame_filepath_selection_merges_frame_destinations_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Starseeker/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let mut files = BTreeMap::new();
    files.insert("フレーム".to_string(), "custom/frame/AC_SP/starseeker".to_string());

    let decoded =
        decode_beatoraja_skin_with_options(&skin_path, SkinKind::Play, &BTreeMap::new(), &files)
            .expect("decode starseeker frame skin");

    assert!(
        decoded.document.source.iter().any(|source| source.id == "main_frame"),
        "expected main_frame source from starseeker frameL.lua"
    );
    assert!(
        decoded
            .document
            .all_destinations(&[])
            .iter()
            .any(|destination| destination.id == "base_L" || destination.id == "base_R"),
        "expected frame panel destinations from starseeker frameL.lua"
    );
}

#[test]
fn starseeker_default_frame_uses_same_directory_for_lua_parts_and_sources_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Starseeker/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play)
        .expect("decode starseeker default frame skin");
    let main_frame = decoded
        .sources
        .iter()
        .find(|source| source.source_id == "main_frame")
        .expect("main_frame source should be decoded from selected frame");

    assert!(
        main_frame.path.components().any(|component| component.as_os_str() == "TM_default"),
        "expected default frame source under TM_default, got {}",
        main_frame.path.display()
    );
}

#[test]
fn apply_skin_from_config_empty_path_uses_default_skin() {
    let mut renderer = Renderer::default();
    let app_paths = test_app_paths();

    apply_skin_from_config(&mut renderer, &app_paths, "").unwrap();
}

#[test]
fn apply_skin_from_config_json_path_loads_beatoraja_skin_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/beatoraja/skin/default/play7.json");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();
    let app_paths = test_app_paths();

    apply_skin_from_config(&mut renderer, &app_paths, skin_path.to_str().unwrap()).unwrap();
}

#[test]
fn apply_skin_from_config_lua_path_loads_beatoraja_skin_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
    if !skin_path.is_file() {
        return;
    }
    let mut renderer = Renderer::default();
    let app_paths = test_app_paths();

    apply_skin_from_config(&mut renderer, &app_paths, skin_path.to_str().unwrap()).unwrap();
}

#[test]
fn wildcard_skin_source_prefers_filepath_default() {
    let root = unique_test_dir("bmz-json-source");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/default.png"), []).unwrap();
    std::fs::write(root.join("parts/blue.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
    )
    .unwrap();

    let resolved =
        resolve_json_skin_source_path(&root, "parts/*.png", &document, &BTreeMap::new()).unwrap();

    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
}

#[test]
fn wildcard_skin_source_prefers_user_file_selection() {
    let root = unique_test_dir("bmz-json-source");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/default.png"), []).unwrap();
    std::fs::write(root.join("parts/blue.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
    )
    .unwrap();
    // ユーザ選択は `def` (blue) より優先される。
    let files = BTreeMap::from([("Parts".to_string(), "parts/default.png".to_string())]);

    let resolved = resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
}

#[test]
fn wildcard_skin_source_falls_back_when_user_selection_missing() {
    let root = unique_test_dir("bmz-json-source");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/default.png"), []).unwrap();
    std::fs::write(root.join("parts/blue.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
    )
    .unwrap();
    // 存在しないファイルを選択 → `def` (blue) へフォールバック。
    let files = BTreeMap::from([("Parts".to_string(), "parts/missing.png".to_string())]);

    let resolved = resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
}

#[test]
fn wildcard_skin_source_ignores_beatoraja_filter_suffix() {
    let root = unique_test_dir("bmz-json-source-filter");
    std::fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
    std::fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
    std::fs::write(root.join("parts/lanecover_lift/TYPE-M.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    {
                        "name": "レーンカバー",
                        "path": "parts/lanecover_lift/*.png|lanecover|",
                        "def": "default"
                    }
                ]
            }
            "#,
    )
    .unwrap();

    let resolved = resolve_json_skin_source_path(
        &root,
        "parts/lanecover_lift/*.png|lanecover|",
        &document,
        &BTreeMap::new(),
    )
    .unwrap();

    assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
}

#[test]
fn wildcard_skin_source_randomly_selects_match() {
    // beatoraja の SkinLoader.getPath 同様、ユーザ選択も def も無いワイルドカードは
    // ロードごとにランダムへ解決する。複数回呼んで両方の候補が選ばれることを確認。
    let root = unique_test_dir("bmz-json-source");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/a.png"), []).unwrap();
    std::fs::write(root.join("parts/b.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str("{}").unwrap();

    let mut seen = std::collections::HashSet::new();
    for _ in 0..200 {
        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &BTreeMap::new())
                .unwrap();
        let name =
            resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
        assert!(name == "a.png" || name == "b.png", "unexpected match {name}");
        seen.insert(name);
    }
    assert_eq!(seen.len(), 2, "both candidates should be selected over many loads");
}

#[test]
fn wildcard_skin_source_explicit_random_overrides_def() {
    // ユーザが明示的に "Random" を選んだら、具体 def があってもランダムにする。
    let root = unique_test_dir("bmz-json-source-explicit-random");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/blue.png"), []).unwrap();
    std::fs::write(root.join("parts/red.png"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
    )
    .unwrap();
    let files = BTreeMap::from([("Parts".to_string(), RANDOM_FILE_SELECTION.to_string())]);

    let mut seen = std::collections::HashSet::new();
    for _ in 0..200 {
        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();
        let name =
            resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
        assert!(name == "blue.png" || name == "red.png", "unexpected match {name}");
        seen.insert(name);
    }
    assert_eq!(seen.len(), 2, "explicit Random should ignore def and pick randomly");
}

#[test]
fn wildcard_skin_source_random_def_selects_match() {
    // filepath の def が "Random" の場合も具体ファイルとして解決せずランダムにする。
    let root = unique_test_dir("bmz-json-source-random-def");
    std::fs::create_dir_all(root.join("bg")).unwrap();
    std::fs::write(root.join("bg/one.mp4"), []).unwrap();
    std::fs::write(root.join("bg/two.mp4"), []).unwrap();
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "filepath": [
                    { "name": "BG", "path": "bg/*.mp4", "def": "Random" }
                ]
            }
            "#,
    )
    .unwrap();

    let mut seen = std::collections::HashSet::new();
    for _ in 0..200 {
        let resolved =
            resolve_json_skin_source_path(&root, "bg/*.mp4", &document, &BTreeMap::new()).unwrap();
        let name =
            resolved.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
        assert!(name == "one.mp4" || name == "two.mp4", "unexpected match {name}");
        seen.insert(name);
    }
    assert_eq!(seen.len(), 2, "def=Random should pick randomly among matches");
}

#[test]
fn wildcard_skin_font_resolves_nested_file() {
    let root = unique_test_dir("bmz-json-font");
    std::fs::create_dir_all(root.join("frame/SP/Default")).unwrap();
    std::fs::write(root.join("frame/SP/Default/song.fnt"), []).unwrap();
    let document: SkinDocument = serde_json::from_str("{}").unwrap();

    let resolved =
        resolve_json_skin_asset_path(&root, "frame/SP/*/song.fnt", &document, &BTreeMap::new())
            .unwrap();

    assert_eq!(resolved.strip_prefix(&root).unwrap(), Path::new("frame/SP/Default/song.fnt"));
}

#[test]
fn skin_asset_path_resolves_case_insensitive_file_names() {
    let root = unique_test_dir("bmz-json-font-case");
    std::fs::create_dir_all(root.join("_font")).unwrap();
    std::fs::write(root.join("_font/Artist.fnt"), []).unwrap();
    let document: SkinDocument = serde_json::from_str("{}").unwrap();

    let resolved =
        resolve_json_skin_asset_path(&root, "_font/artist.fnt", &document, &BTreeMap::new())
            .unwrap();

    assert_eq!(resolved.strip_prefix(&root).unwrap(), Path::new("_font/Artist.fnt"));
}
