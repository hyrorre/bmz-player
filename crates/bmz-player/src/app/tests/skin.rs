use super::*;

#[test]
fn qwilight_catalog_registers_only_play_slots_without_running_lua() {
    let temp = tempfile::tempdir().unwrap();
    let yaml = "format: {title: Qwilight fixture}\nframe: {}\nlambda: {}\npoint: {}";
    std::fs::write(temp.path().join("play.yaml"), yaml).unwrap();
    std::fs::write(temp.path().join("@theme.yaml"), yaml).unwrap();
    std::fs::write(temp.path().join("play.lua"), "while true do end").unwrap();
    let mut catalog = SkinCatalog::default();
    scan_skin_catalog_dir(
        temp.path(),
        temp.path(),
        &[temp.path().to_path_buf()],
        SkinCandidateOrigin::User,
        &mut catalog,
    );
    for candidates in [
        &catalog.play4,
        &catalog.play5,
        &catalog.play6,
        &catalog.play7,
        &catalog.play8,
        &catalog.play9,
        &catalog.play10,
        &catalog.play14,
    ] {
        assert_eq!(candidates.len(), 1);
    }
    assert!(catalog.select.is_empty());
    assert!(catalog.result.is_empty());
}

/// A Qwilight skin declaring two adjustments, with an option hidden by another.
fn write_qwilight_customize_fixture(dir: &Path) -> PathBuf {
    use std::io::Write;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("skin.yaml");
    std::fs::write(&path, "format: {zip: skin}\nframe: {}\nlambda:\n  setMainPosition: true\n  setJudgmentMainPosition: true\n  configure-comment-0: Colors\n  configure-0: Hide, Show\n  configure-comment-1: Color\n  configure-comment-1-1042: 색\n  configure-1: Red, Blue\n  configure-1-1042: 빨강, 파랑\n  configure-available-1: Toggle()\n  drawing1: 1, 2, 3, 4, 5, 10\n  pipeline: 7\npoint:\n  mainPosition: Main()\n  noteLength: 20\n  judgmentMainPosition: Judge()\n").unwrap();
    std::fs::write(
        dir.join("skin.lua"),
        "function Main() return 10 + mainPosition end\nfunction Judge() return judgmentMainPosition - 100 end\nfunction Toggle() return configures[1] == 1 end",
    )
    .unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(dir.join("skin.zip")).unwrap());
    zip.start_file("Note/N 1 0 0.png", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(png.get_ref()).unwrap();
    zip.finish().unwrap();
    path
}

#[test]
fn qwilight_saved_adjustments_reload_and_move_layout_for_each_key_mode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let path = write_qwilight_customize_fixture(&root.join("Q"));
    let offset = |name: &str, x, y| SkinOffsetConfig {
        name: Some(name.into()),
        id: 0,
        x,
        y,
        ..Default::default()
    };
    let mut skin = SkinConfig {
        play7: path.to_string_lossy().into_owned(),
        play14: path.to_string_lossy().into_owned(),
        ..Default::default()
    };
    skin.play7_offsets = vec![
        offset("Qwilight lane position", 40, 0),
        // Up by 50 in BMZ: Qwilight's judgment line value is -50.
        offset("Qwilight judgment line position", 0, 50),
    ];
    // Saved per key mode, as Qwilight keeps the lane position per input mode.
    skin.play14_offsets = vec![offset("Qwilight lane position", -5, 0)];
    let saved: SkinConfig = toml::from_str(&toml::to_string(&skin).unwrap()).unwrap();
    let state_for = |mode: i32, offsets: &[SkinOffsetConfig]| {
        let mut state = bmz_skin::LuaLoadRuntimeState {
            number_values: BTreeMap::from([(bmz_render::skin::SKIN_REF_BMZ_KEY_MODE, mode)]),
            ..Default::default()
        };
        apply_skin_offsets_to_lua_runtime_state(&mut state, offsets);
        state
    };
    let defaults = state_for(7, &[]);
    let adjusted = state_for(7, &saved.play7_offsets);
    let signature = |state| {
        play_skin_signature(
            KeyMode::K7,
            SessionMode::Normal,
            &saved.play7,
            &BTreeMap::new(),
            &BTreeMap::new(),
            state,
            0,
        )
    };
    assert_ne!(signature(&defaults), signature(&adjusted), "an offset edit re-decodes Play");
    let lane_area = |state: &bmz_skin::LuaLoadRuntimeState| {
        let decoded =
            crate::skin_loader::decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
                pinned_sources: None,
                skin_path: &path,
                kind: SkinKind::Play,
                options: &BTreeMap::new(),
                files: &BTreeMap::new(),
                runtime_state: state,
                library_roots: std::slice::from_ref(&root),
                document_cache: None,
                source_cache: None,
                texture_cache: None,
                font_cache: None,
                installed_fonts: None,
            })
            .unwrap();
        let note = decoded.document.note.unwrap();
        // beatoraja note order puts the 1P scratch after the seven keys (7K and 14K).
        let bmz_render::skin::SkinDstEntry::Frame(frame) = &note.dst[7] else {
            panic!("scratch frame")
        };
        (frame.x.unwrap(), frame.h.unwrap())
    };
    assert_eq!(lane_area(&defaults), (10, 500));
    assert_eq!(lane_area(&adjusted), (50, 450));
    assert_eq!(lane_area(&state_for(14, &saved.play14_offsets)), (5, 500));
}

#[test]
fn qwilight_settings_defs_follow_slot_options_and_keep_only_effective_offsets() {
    let temp = tempfile::tempdir().unwrap();
    let path = write_qwilight_customize_fixture(&temp.path().join("skins/Q"));
    let paths = crate::paths::AppPaths::from_dirs(
        temp.path().to_path_buf(),
        temp.path().to_path_buf(),
        temp.path().join("cache"),
        temp.path().join("logs"),
    );
    let path = path.to_string_lossy();
    let defs = |options: Option<&BTreeMap<String, String>>| {
        skin_defs_from_path(&paths, &SkinConfig::default(), &path, true, options)
    };
    let names = |defs: &SceneSkinDefs| {
        defs.property.iter().map(|property| property.name.clone()).collect::<Vec<_>>()
    };
    let hidden = BTreeMap::from([("Colors".to_string(), "Hide".to_string())]);
    let shown = BTreeMap::from([("Colors".to_string(), "Show".to_string())]);
    assert_eq!(names(&defs(Some(&hidden))), ["Colors", "Lane cover image"]);
    let visible = defs(Some(&shown));
    assert_eq!(names(&visible), ["Colors", "Color", "Lane cover image"]);
    // The catalog header lists every option without running Lua.
    assert_eq!(names(&defs(None)), ["Colors", "Color", "Lane cover image"]);
    assert!(visible.qwilight);
    assert_eq!(visible.property_label("Color", crate::i18n::AppLocale::Ko), "색");
    assert_eq!(visible.property_label("Color", crate::i18n::AppLocale::Ja), "Color");
    assert_eq!(visible.item_label("Color", "Blue", crate::i18n::AppLocale::Ko), "파랑");
    assert_eq!(visible.item_label("Color", "Blue", crate::i18n::AppLocale::ZhCn), "Blue");
    let offsets: Vec<_> = visible.offset.iter().map(|offset| offset.id).collect();
    // Declared Qwilight adjustments plus All / Notes / Judge offset. Judge Detail
    // and Bar Line offset have no converted element, so they are not shown.
    assert_eq!(offsets, [1001, 1005, 10, 30, 32]);
}

#[test]
fn invalid_continue_fallback_does_not_abort_result_skin_decode() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    std::fs::create_dir(&root).unwrap();
    let entry = root.join("result.lr2skin");
    std::fs::write(&entry, "#INFORMATION,7,Result,Test\n#RESOLUTION,0\n").unwrap();
    let outside = temp.path().join("outside.lr2skin");
    std::fs::write(&outside, "#INFORMATION,0,Outside,Test\n").unwrap();
    let deleted = root.join("deleted.lr2skin");
    std::fs::write(&deleted, "#INFORMATION,0,Deleted,Test\n").unwrap();
    std::fs::remove_file(&deleted).unwrap();
    for fallback in [deleted, outside] {
        let mut pipeline = SkinPipelineRuntime::new();
        let request = SkinDecodeRequest::new(
            0,
            entry.clone(),
            SkinKind::Result,
            BTreeMap::new(),
            BTreeMap::new(),
            Default::default(),
        )
        .with_library_roots(vec![root.clone()])
        .with_lr2_continue_fallback(Some(Lr2ContinueFallback {
            play_skin: fallback,
            runtime_state: Default::default(),
        }));
        spawn_skin_decode(&pipeline, request);
        let result =
            pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        pipeline.shutdown_workers();
        assert_eq!(result.result.unwrap().document.name, "Result");
    }
}

#[test]
#[cfg(unix)]
fn shared_resource_and_data_root_aliases_remain_authorized() {
    let temp = tempfile::tempdir().unwrap();
    let resource = temp.path().join("resource");
    let data = temp.path().join("data");
    std::fs::create_dir_all(data.join("skins")).unwrap();
    std::fs::create_dir_all(&resource).unwrap();
    std::os::unix::fs::symlink(data.join("skins"), resource.join("skins")).unwrap();
    let paths = crate::paths::AppPaths::from_dirs(
        resource.clone(),
        data.clone(),
        temp.path().join("cache"),
        temp.path().join("logs"),
    );
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]))
        .save(data.join("skins/page.png"))
        .unwrap();
    let entry = resource.join("skins/select.json");
    std::fs::write(
        &entry,
        serde_json::json!({
            "type":5,"name":"Shared roots",
            "source":[{"id":"page","path":resource.join("skins/page.png")}]
        })
        .to_string(),
    )
    .unwrap();
    let catalog = scan_skin_catalog(&paths);
    assert_eq!(catalog.select.len(), 1);
    assert_eq!(catalog.select[0].path, "resource:skins/select.json");
    let skin = SkinConfig::default();
    for path in [&entry, &data.join("skins/select.json")] {
        assert!(paths.skin_path_context(&skin, path).is_ok());
    }
    let mut pipeline = SkinPipelineRuntime::new();
    spawn_skin_decode(
        &pipeline,
        SkinDecodeRequest::new(
            0,
            entry,
            SkinKind::Select,
            BTreeMap::new(),
            BTreeMap::new(),
            Default::default(),
        )
        .with_skin_access(&paths, &skin),
    );
    let result =
        pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(result.result.unwrap().sources.len(), 1);
    pipeline.shutdown_workers();
}

#[test]
#[cfg(unix)]
fn explicitly_authorized_alias_keeps_its_logical_request_identity() {
    let root = tempfile::tempdir().unwrap();
    let paths = crate::paths::AppPaths::from_dirs(
        root.path().join("resource"),
        root.path().join("data"),
        root.path().join("cache"),
        root.path().join("logs"),
    );
    let external = root.path().join("external");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(external.join("select.lua"), "return {type=5,name='Authorized'}").unwrap();
    let alias = root.path().join("selected.lua");
    std::os::unix::fs::symlink(external.join("select.lua"), &alias).unwrap();
    let mut skin = SkinConfig::default();
    paths.approve_skin_path(&mut skin, &alias).unwrap();
    let request = SkinDecodeRequest::new(
        0,
        alias.clone(),
        SkinKind::Select,
        BTreeMap::new(),
        BTreeMap::new(),
        Default::default(),
    )
    .with_skin_access(&paths, &skin);
    assert_eq!(request.path, alias);
    let mut pipeline = SkinPipelineRuntime::new();
    spawn_skin_decode(&pipeline, request);
    let result =
        pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(result.path, alias);
    assert_eq!(result.result.unwrap().document.name, "Authorized");
    pipeline.shutdown_workers();
}

#[test]
#[cfg(unix)]
fn skin_catalog_rejects_outside_entries_before_header_evaluation() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("skins");
    let outside = root.path().join("outside");
    std::fs::create_dir(&library).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(library.join("inside.json"), r#"{"type":5,"name":"Inside"}"#).unwrap();
    for (name, content) in [
        ("outside.luaskin", "error('OUTSIDE EVALUATED')"),
        ("outside.json", "invalid json"),
        ("outside.lr2skin", "#INFORMATION,5,Outside,Test\n"),
    ] {
        std::fs::write(outside.join(name), content).unwrap();
        let link = library.join(name);
        std::os::unix::fs::symlink(outside.join(name), &link).unwrap();
        let error = read_skin_header_document(&link, std::slice::from_ref(&library)).unwrap_err();
        assert!(error.to_string().contains("link escapes skin root"), "{error:#}");
        assert!(!format!("{error:#}").contains("OUTSIDE EVALUATED"));
    }
    std::os::unix::fs::symlink(&outside, library.join("outside-dir")).unwrap();
    std::os::unix::fs::symlink(library.join("inside.json"), library.join("alias.json")).unwrap();
    let paths = crate::paths::AppPaths::from_dirs(
        root.path().to_path_buf(),
        root.path().to_path_buf(),
        root.path().join("cache"),
        root.path().join("logs"),
    );
    let catalog = scan_skin_catalog(&paths);
    assert_eq!(catalog.select.len(), 1);
    assert_eq!(catalog.select[0].path, "resource:skins/inside.json");
    assert_eq!(catalog.select[0].name, "Inside");
    std::fs::write(library.join("module.lua"), "return {type=5,name='Alias'}").unwrap();
    std::os::unix::fs::symlink(library.join("module.lua"), library.join("select.luaskin")).unwrap();
    let catalog = scan_skin_catalog(&paths);
    assert_eq!(catalog.select.len(), 2);
    assert!(
        catalog.select.iter().any(|candidate| candidate.path == "resource:skins/select.luaskin")
    );
}

#[test]
fn access_change_detaches_caches_and_invalidates_inflight_generations() {
    let mut pipeline = SkinPipelineRuntime::new();
    let access = BTreeMap::from([(
        "external.lua".into(),
        crate::config::profile_config::SkinAccessGrant {
            entry: "external.lua".into(),
            roots: vec!["external".into()],
        },
    )]);
    assert!(pipeline.update_access(&access));
    let document = pipeline.document_cache.clone();
    let source = pipeline.source_asset_cache.clone();
    let font = pipeline.font_cache.clone();
    let texture = pipeline.gpu_texture_cache.clone();
    let before: Vec<_> = [SkinKind::Select, SkinKind::Decide, SkinKind::Play, SkinKind::Result]
        .into_iter()
        .map(|kind| (kind, pipeline.generation(kind)))
        .collect();
    assert!(!pipeline.update_access(&access));
    assert!(pipeline.update_access(&BTreeMap::new()));
    assert!(!Arc::ptr_eq(&document, &pipeline.document_cache));
    assert!(!Arc::ptr_eq(&source, &pipeline.source_asset_cache));
    assert!(!Arc::ptr_eq(&font, &pipeline.font_cache));
    assert!(!Arc::ptr_eq(&texture, &pipeline.gpu_texture_cache));
    for (kind, generation) in before {
        assert_ne!(pipeline.generation(kind), generation);
    }
}

#[test]
fn frontend_lua_load_exposes_ir_connection_before_building_objects() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("frontend-ir.lua");
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
fn skin_catalog_accepts_lua_property_scalar_coercions() {
    let root = crate::test_dir::TestDir::new("bmz-catalog-lua-coercions");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("select.luaskin");
    std::fs::write(
        &path,
        r#"return {type=5, name='Compatible Lua', property={
            {name='Direction', item={{name='Reverse', op='縦'}}},
            {name='Label', item={{name='Bottom', op=955}}, def=955}
        }}"#,
    )
    .unwrap();
    let (skin_type, candidate) =
        load_skin_candidate(&root, &path, SkinCandidateOrigin::User).unwrap();
    assert_eq!(skin_type, 5);
    assert_eq!(candidate.name, "Compatible Lua");
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
#[cfg(any(unix, windows))]
fn skin_catalog_scans_directory_links_once_and_accepts_linked_root() {
    let fixture = crate::directory_scan::test_support::LinkedDirectories::new();
    std::fs::write(fixture.root.join("root.json"), r#"{"type":5,"name":"Zulu"}"#).unwrap();
    std::fs::write(fixture.nested.join("select.JSON"), r#"{"type":5,"name":"Alpha"}"#).unwrap();
    std::fs::write(fixture.nested.join("parts.lua"), "return {type=5, name='Parts'}").unwrap();

    for root in [&fixture.root, &fixture.linked_root] {
        for (origin, prefix) in [
            (SkinCandidateOrigin::Bundled, "resource:skins/"),
            (SkinCandidateOrigin::User, "data:skins/"),
        ] {
            let mut catalog = SkinCatalog::default();
            scan_skin_catalog_dir(root, root, std::slice::from_ref(root), origin, &mut catalog);
            sort_skin_catalog(&mut catalog);
            assert_eq!(catalog.select.len(), 2);
            assert_eq!(catalog.select[0].name, "Alpha");
            assert_eq!(catalog.select[1].name, "Zulu");
            assert_eq!(catalog.select[1].path, format!("{prefix}root.json"));
            for (candidate, expected) in catalog
                .select
                .iter()
                .zip([fixture.nested.join("select.JSON"), fixture.root.join("root.json")])
            {
                assert_eq!(candidate.origin, origin);
                let relative = candidate.path.strip_prefix(prefix).unwrap();
                assert!(!Path::new(relative).is_absolute());
                assert_eq!(
                    root.join(relative).canonicalize().unwrap(),
                    expected.canonicalize().unwrap()
                );
            }
        }
    }
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
    let temp = tempfile::Builder::new().prefix("bmz-player-skin-catalog-json-").tempdir().unwrap();
    let root = temp.path().to_path_buf();
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
fn skin_catalog_lists_bundled_decide_json_skins() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let skin_root = repo_root.join("data/skins");

    let default_path = skin_root.join("default/decide.json");
    let (skin_type, candidate) =
        load_skin_candidate(&skin_root, &default_path, SkinCandidateOrigin::Bundled)
            .expect("load default decide candidate");
    assert_eq!(skin_type, 6);
    assert_eq!(candidate.path, "resource:skins/default/decide.json");

    // Luxez-Flat's decide.json is CP932, uses `//` comments, omits commas and writes
    // `"wrapping": "false"` like beatoraja/libGDX accepts (issue #87).
    let luxez_path = skin_root.join("Luxez-Flat/decide/decide.json");
    if !luxez_path.is_file() {
        return;
    }
    let (skin_type, candidate) =
        load_skin_candidate(&skin_root, &luxez_path, SkinCandidateOrigin::Bundled)
            .expect("load Luxez-Flat decide candidate");
    assert_eq!(skin_type, 6);
    assert_eq!(candidate.path, "resource:skins/Luxez-Flat/decide/decide.json");
    assert!(!candidate.name.trim().is_empty(), "candidate name should not be empty");

    let document = SkinDocument::load_beatoraja_json(&luxez_path).expect("load Luxez-Flat decide");
    assert_eq!(document.skin_type, 6);
    assert!(document.text.iter().any(|text| text.id == "title" && !text.wrapping));
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

#[test]
fn lr2_continue_fallback_uses_a_same_theme_play_skin_unless_that_theme_was_played() {
    let root = std::env::temp_dir().join("bmz-continue-fallback/skins");
    let roots = [root.clone()];
    let result = root.join("ECSE/SE-Result/result.lr2skin");
    let ecse_play = root.join("ECSE/Play/play_a.lr2skin");
    let wmix_play = root.join("WMIX/play/play.lr2skin");
    let lua_play = root.join("ECFN/play/play7.luaskin");
    let candidates = [wmix_play.as_path(), ecse_play.as_path()];
    let fallback = |played: Option<&Path>, lr2: bool| {
        lr2_continue_fallback_play_skin(&result, played, lr2, candidates, &roots)
    };
    // A Lua / JSON Play skin hands nothing over, so the Result's theme stands in.
    assert_eq!(fallback(Some(&lua_play), false), Some(ecse_play.clone()));
    assert_eq!(fallback(None, false), Some(ecse_play.clone()));
    // An LR2 Play skin of another theme is also replaced.
    assert_eq!(fallback(Some(&wmix_play), true), Some(ecse_play.clone()));
    // The Result's own theme keeps LR2's CONTINUE.
    assert_eq!(fallback(Some(&ecse_play), true), None);
    // No LR2 Play skin of that theme, or a Result without a theme folder.
    let lone = root.join("LONE/Result/result.lr2skin");
    assert_eq!(lr2_continue_fallback_play_skin(&lone, None, false, candidates, &roots), None);
    let flat = root.join("result.lr2skin");
    assert_eq!(lr2_continue_fallback_play_skin(&flat, None, false, candidates, &roots), None);
}

#[test]
fn lr2_play_skin_candidates_prefer_the_played_key_mode_and_skip_lua() {
    let candidate = |path: &str| SkinCandidate {
        name: path.into(),
        path: path.into(),
        origin: SkinCandidateOrigin::User,
    };
    let catalog = SkinCatalog {
        play7: vec![candidate("data:skins/A/b7.lr2skin"), candidate("data:skins/A/a7.lr2skin")],
        play14: vec![candidate("data:skins/A/dp.lr2skin"), candidate("data:skins/B/dp.luaskin")],
        battle7: vec![candidate("data:skins/A/battle.lr2skin")],
        ..Default::default()
    };
    assert_eq!(
        lr2_play_skin_candidates(&catalog, KeyMode::K14),
        [
            "data:skins/A/dp.lr2skin",
            "data:skins/A/a7.lr2skin",
            "data:skins/A/b7.lr2skin",
            "data:skins/A/battle.lr2skin"
        ]
    );
    assert_eq!(lr2_play_skin_candidates(&catalog, KeyMode::K7)[0], "data:skins/A/a7.lr2skin");
}

#[test]
fn lr2_continue_fallback_assets_win_but_keep_inherited_empty_slots() {
    let source = |path: &str| bmz_render::skin::SkinSourceDef {
        id: String::new(),
        path: path.into(),
        lr2_color_key: None,
    };
    let inherited = bmz_skin::Lr2ContinueAssets {
        sources: BTreeMap::from([
            ("23".into(), source("other/clear.png")),
            ("27".into(), source("other/decide.png")),
        ]),
        ..Default::default()
    };
    let fallback = bmz_skin::Lr2ContinueAssets {
        sources: BTreeMap::from([
            ("23".into(), source("theme/HARDCLEAR.png")),
            ("27".into(), source("bmz://lr2/empty")),
        ]),
        ..Default::default()
    };
    let merged = merge_lr2_continue_assets(fallback, &inherited);
    assert_eq!(merged.sources["23"].path, "theme/HARDCLEAR.png");
    assert_eq!(merged.sources["27"].path, "other/decide.png");
}

#[test]
fn installed_lr2_play_skins_resolve_result_continue_assets_when_available() {
    let skins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    let context = |path: &Path| bmz_skin::SkinPathContext::new(path, [skins.clone()]).unwrap();
    for (play, clear, slot, expected) in [
        ("EndlessCirculation/Play/play_Maniac.lr2skin", ClearType::Hard, "23", "HARDCLEAR.png"),
        ("EndlessCirculation/Play/play_Maniac.lr2skin", ClearType::NoPlay, "23", "NOPLAY.png"),
        ("WMIX_HD/play/HDPLAY_AC.lr2skin", ClearType::FullCombo, "23", "FULLCOMBO.png"),
    ] {
        let path = skins.join(play);
        if !path.is_file() {
            continue;
        }
        let mut state = bmz_skin::LuaLoadRuntimeState::default();
        apply_lr2_best_clear_load_options(&mut state, Some(clear));
        let assets = crate::skin_loader::resolve_lr2_continue_assets_with_context(
            &context(&path),
            &state,
            None,
        )
        .unwrap_or_else(|error| panic!("{play}: {error:#}"));
        let source = Path::new(&assets.sources[slot].path);
        assert!(source.is_absolute(), "{play}");
        assert!(source.ends_with(expected), "{play}: {}", source.display());
        // The Result's LR2FONT CONTINUE slots come from the same Play skin. W-MIX keeps
        // its fonts in DXA archives, so the path may name a file inside one.
        let font = Path::new(&assets.fonts["lr2font-0"].path);
        assert!(font.is_absolute() && font.ends_with("font.lr2font"), "{play}");
    }
    // ECSE Result draws the pre-update best lamp from the fallback's image 23.
    let (play, result) = (
        skins.join("EndlessCirculation/Play/play_Maniac.lr2skin"),
        skins.join("EndlessCirculation/SE-Result/SelfEvolution-Result.lr2skin"),
    );
    if play.is_file() && result.is_file() {
        let mut play_state = bmz_skin::LuaLoadRuntimeState::default();
        apply_lr2_best_clear_load_options(&mut play_state, Some(ClearType::Easy));
        let fallback = crate::skin_loader::resolve_lr2_continue_assets_with_context(
            &context(&play),
            &play_state,
            None,
        )
        .unwrap();
        let result_state = bmz_skin::LuaLoadRuntimeState {
            lr2_continue_assets: merge_lr2_continue_assets(fallback, &Default::default()).into(),
            ..Default::default()
        };
        let loaded = bmz_skin::load_lr2_csv_skin_with_path_context(
            &context(&result),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &result_state,
        )
        .unwrap();
        let source = loaded.document.source.iter().find(|source| source.id == "23").unwrap();
        assert!(source.path.ends_with("EASYCLEAR.png"), "{}", source.path);
    }
    let wmix = skins.join("WMIX_HD/play/HDPLAY_AC.lr2skin");
    if wmix.is_file() {
        let assets = crate::skin_loader::resolve_lr2_continue_assets_with_context(
            &context(&wmix),
            &bmz_skin::LuaLoadRuntimeState::default(),
            None,
        )
        .unwrap();
        // W-MIX Result draws the GROOVE / HARD gauge from the Play skin's GAUGE COLOR file.
        assert!(Path::new(&assets.sources["24"].path).is_file());
    }
}

#[test]
fn lr2_continue_theme_fill_keeps_inherited_and_skips_unusable_provider_slots() {
    let temp = tempfile::tempdir().unwrap();
    let shutter = temp.path().join("shutter.png");
    std::fs::write(&shutter, b"png").unwrap();
    let source = |path: &str| bmz_render::skin::SkinSourceDef {
        id: String::new(),
        path: path.into(),
        lr2_color_key: None,
    };
    let current = bmz_skin::Lr2ContinueAssets {
        sources: BTreeMap::from([
            ("23".into(), source("played/clear.png")),
            ("28".into(), source("bmz://lr2/empty")),
        ]),
        ..Default::default()
    };
    let provider = bmz_skin::Lr2ContinueAssets {
        sources: BTreeMap::from([
            ("23".into(), source("theme/other.png")),
            ("27".into(), source(&temp.path().join(".dummy").to_string_lossy())),
            ("28".into(), source(&shutter.to_string_lossy())),
            ("29".into(), source("bmz://lr2/empty")),
        ]),
        ..Default::default()
    };
    let filled = fill_empty_lr2_continue_assets(provider, &current);
    assert_eq!(filled.sources["23"].path, "played/clear.png");
    assert_eq!(Path::new(&filled.sources["28"].path), shutter);
    assert!(!filled.sources.contains_key("27") && !filled.sources.contains_key("29"));
}

#[test]
fn same_theme_lr2_skins_fill_continue_slots_left_by_other_scenes_when_available() {
    let skins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    let roots = [skins.clone()];
    let context = |path: &Path| bmz_skin::SkinPathContext::new(path, roots.clone()).unwrap();
    let load_with_theme_fill = |relative: &str| {
        let path = skins.join(relative);
        let mut state = bmz_skin::LuaLoadRuntimeState::default();
        for provider in lr2_continue_theme_providers(&path, &roots) {
            let assets = crate::skin_loader::resolve_lr2_continue_assets_with_context(
                &context(&provider),
                &bmz_skin::LuaLoadRuntimeState::default(),
                None,
            )
            .unwrap_or_else(|error| panic!("{}: {error:#}", provider.display()));
            state.lr2_continue_assets =
                fill_empty_lr2_continue_assets(assets, &state.lr2_continue_assets).into();
        }
        bmz_skin::load_lr2_csv_skin_with_path_context(
            &context(&path),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &state,
        )
        .unwrap_or_else(|error| panic!("{relative}: {error:#}"))
        .document
    };
    // Image slots each skin draws from CONTINUE and the file LR2 would still hold there.
    for (skin, slot, expected) in [
        // W-MIX Course Result / Play shutters come from the W-MIX Result.
        ("WMIX_HD/courseresult/courseresult.lr2skin", "28", "select/shutter"),
        ("WMIX_HD/play/HDPLAY_W.lr2skin", "28", "select/shutter"),
        // ECSE Play fades with the shutter its Result loads.
        ("EndlessCirculation/Play/play_SESP.lr2skin", "27", "Shutter"),
        // RED_BELT Result clear marks come from its Decide (Play in between).
        ("RED_BELT/Result/result.lr2skin", "24", "RED_BELT"),
        // ECSE / 3R Decide level numbers come from their Select.
        ("EndlessCirculation/SE-Decide/decide.lr2skin", "0", "SE-Select"),
        ("3R/Decide/decide_simple.lr2skin", "0", "Select"),
    ] {
        if !skins.join(skin).is_file() {
            eprintln!("SKIP missing {skin}");
            continue;
        }
        let document = load_with_theme_fill(skin);
        let source = document.source.iter().find(|source| source.id == slot).unwrap();
        let path = source.path.replace('\\', "/");
        assert!(path.contains(expected) && Path::new(&source.path).is_file(), "{skin}: {path}");
    }
}
