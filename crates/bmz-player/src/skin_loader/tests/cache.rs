use super::*;

#[test]
fn lua_document_cache_key_includes_explicit_library_roots() {
    let base = unique_test_dir("bmz-lua-document-cache-library-roots");
    let library_root = base.join("skins");
    let entry_dir = library_root.join("GenericTheme/play");
    std::fs::create_dir_all(&entry_dir).unwrap();
    let skin_path = entry_dir.join("play.luaskin");
    std::fs::write(&skin_path, "return { type = 0 }").unwrap();
    let narrow = SkinPathContext::new(&skin_path, [library_root]).unwrap();
    let broad = SkinPathContext::new(&skin_path, [base]).unwrap();

    assert_ne!(
        skin_document_cache_key(&skin_path, SkinKind::Play, Some(&narrow)),
        skin_document_cache_key(&skin_path, SkinKind::Play, Some(&broad))
    );
}

#[test]
fn lua_document_cache_does_not_reuse_load_time_math_random() {
    let root = unique_test_dir("bmz-lua-document-cache-math-random");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("select.luaskin");
    std::fs::write(
        &skin_path,
        r#"
return {
    type = 5,
    name = tostring(math.random(1, 1000000)),
}
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    for _ in 0..2 {
        let loaded = load_skin_document(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(loaded.cache_status, DocumentCacheStatus::Miss);
    }
    assert!(cache.lock().unwrap().entries.is_empty());
}

#[test]
fn lua_document_cache_does_not_reuse_random_get_path_selection() {
    let root = unique_test_dir("bmz-lua-document-cache-random-get-path");
    std::fs::create_dir_all(root.join("bg")).unwrap();
    std::fs::write(root.join("bg/one.png"), []).unwrap();
    std::fs::write(root.join("bg/two.png"), []).unwrap();
    let skin_path = root.join("result.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local path = "bg/one.png"
if skin_config and skin_config.get_path then
    path = skin_config.get_path("bg/*.png")
end
return {
    type = 7,
    filepath = {
        { name = "Background", path = "bg/*.png", def = "one" },
    },
    source = {
        { id = "bg", path = path },
    },
}
"#,
    )
    .unwrap();
    let files = BTreeMap::from([("Background".to_string(), RANDOM_FILE_SELECTION.to_string())]);
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    for _ in 0..2 {
        let loaded = load_skin_document(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &files,
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
        )
        .unwrap();
        assert_eq!(loaded.cache_status, DocumentCacheStatus::Miss);
    }
    assert!(cache.lock().unwrap().entries.is_empty());
}

#[test]
fn lua_document_cache_invalidates_cross_package_module_changes() {
    let library_root = unique_test_dir("bmz-lua-document-cache-cross-package").join("skins");
    let entry_dir = library_root.join("GenericTheme-master/play");
    let hub_dir = library_root.join("Hub");
    std::fs::create_dir_all(&entry_dir).unwrap();
    std::fs::create_dir_all(&hub_dir).unwrap();
    let skin_path = entry_dir.join("Hub_play7.luaskin");
    let module_path = hub_dir.join("label.lua");
    std::fs::write(
        &skin_path,
        r#"
            package.path = "skin/Hub/?.lua"
            return { type = 0, name = require("label") }
        "#,
    )
    .unwrap();
    std::fs::write(&module_path, "return 'first'").unwrap();
    let path_context = SkinPathContext::new(&skin_path, [library_root]).unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let load = || {
        load_skin_document_with_path_context(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            Some(cache.clone()),
            Some(&path_context),
        )
        .unwrap()
    };
    let first = load();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.name, "first");
    let unchanged = load();
    assert_eq!(unchanged.cache_status, DocumentCacheStatus::Hit);

    std::fs::write(&module_path, "return 'second-value'").unwrap();
    let changed = load();
    assert_eq!(changed.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(changed.document.name, "second-value");
}

#[test]
fn lua_document_cache_reuses_when_unused_option_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-option");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("play.luaskin");
    std::fs::write(
            &skin_path,
            r#"
local branch = 910
if skin_config and skin_config.option then
    branch = skin_config.option["Branch"] or 910
end
return {
    type = 0,
    property = {
        { name = "Unused", item = {{ name = "Off", op = 900 }, { name = "On", op = 901 }}, def = "Off" },
        { name = "Branch", item = {{ name = "Off", op = 910 }, { name = "On", op = 911 }}, def = "Off" },
    },
    source = {
        { id = "bg", path = branch == 911 and "on.png" or "off.png" },
    },
}
"#,
        )
        .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let first = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.source[0].path, "off.png");

    let unused_changed = BTreeMap::from([("Unused".to_string(), "On".to_string())]);
    let second = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &unused_changed,
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(second.cache_status, DocumentCacheStatus::Hit);
    assert_eq!(second.document.source[0].path, "off.png");
    assert!(second.document.enabled_options().contains(&901));

    let branch_changed = BTreeMap::from([("Branch".to_string(), "On".to_string())]);
    let third = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &branch_changed,
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache),
    )
    .unwrap();
    assert_eq!(third.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(third.document.source[0].path, "on.png");
}

#[test]
fn lua_document_cache_misses_when_required_module_option_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-required-option");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("play.luaskin");
    let module_path = root.join("parts.lua");
    std::fs::write(
        &skin_path,
        r#"
local parts = require("parts")
return parts.build()
"#,
    )
    .unwrap();
    std::fs::write(
            &module_path,
            r#"
local M = {}
function M.build()
    local branch = 910
    if skin_config and skin_config.option then
        branch = skin_config.option["Branch"] or 910
    end
    return {
        type = 0,
        property = {
            { name = "Unused", item = {{ name = "Off", op = 900 }, { name = "On", op = 901 }}, def = "Off" },
            { name = "Branch", item = {{ name = "Off", op = 910 }, { name = "On", op = 911 }}, def = "Off" },
        },
        source = {
            { id = "bg", path = branch == 911 and "on.png" or "off.png" },
        },
    }
end
return M
"#,
        )
        .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let first = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.source[0].path, "off.png");

    let unused_changed = BTreeMap::from([("Unused".to_string(), "On".to_string())]);
    let second = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &unused_changed,
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(second.cache_status, DocumentCacheStatus::Hit);
    assert_eq!(second.document.source[0].path, "off.png");

    let branch_changed = BTreeMap::from([("Branch".to_string(), "On".to_string())]);
    let third = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &branch_changed,
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache),
    )
    .unwrap();
    assert_eq!(third.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(third.document.source[0].path, "on.png");
}

#[test]
fn lua_document_cache_misses_when_runtime_number_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-number");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("result.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local main_state = require("main_state")
local diff = main_state.number(178)
return {
    type = 7,
    source = {
        { id = "bg", path = diff == 0 and "zero.png" or "nonzero.png" },
    },
}
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let zero_state = LuaLoadRuntimeState {
        number_values: BTreeMap::from([(178, 0)]),
        text_values: BTreeMap::new(),
        option_values: BTreeMap::new(),
        ..LuaLoadRuntimeState::default()
    };
    let first = load_skin_document(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &zero_state,
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.source[0].path, "zero.png");

    let nonzero_state = LuaLoadRuntimeState {
        number_values: BTreeMap::from([(178, -1)]),
        text_values: BTreeMap::new(),
        option_values: BTreeMap::new(),
        ..LuaLoadRuntimeState::default()
    };
    let second = load_skin_document(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &nonzero_state,
        Some(cache),
    )
    .unwrap();
    assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(second.document.source[0].path, "nonzero.png");
}

#[test]
fn lua_document_cache_does_not_reuse_auto_document_for_compat_mode() {
    let root = unique_test_dir("bmz-lua-document-cache-runtime-mode");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("play.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local main_state = require("main_state")
return {
    type = 0,
    destination = {{
        id = "runtime",
        draw = function() return main_state.option(46) end,
        dst = {{ x = 0, y = 0, w = 1, h = 1 }},
    }},
}
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let auto = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(auto.cache_status, DocumentCacheStatus::Miss);
    assert!(auto.lua_runtime.is_none());

    let compat_state = LuaLoadRuntimeState {
        runtime_mode: bmz_skin::LuaSkinRuntimeMode::Compat,
        ..Default::default()
    };
    let compat = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &compat_state,
        Some(cache),
    )
    .unwrap();
    assert_eq!(compat.cache_status, DocumentCacheStatus::Miss);
    assert!(compat.lua_runtime.is_some());
}

#[test]
fn lua_document_cache_misses_when_runtime_offset_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-offset");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("play.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local skin = {
    type = 1,
    offset = {
        { name = "Panel", id = 42, x = true },
    },
}
if skin_config == nil then
    return skin
end
local panel_x = skin_config.offset["Panel"].x
skin.source = {
    { id = "bg", path = panel_x == 0 and "zero.png" or "nonzero.png" },
}
return skin
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));
    let offset = |x| LuaLoadRuntimeState {
        offset_values: BTreeMap::from([(
            "Panel".to_string(),
            bmz_skin::LuaSkinOffsetValue { x, ..Default::default() },
        )]),
        offset_id_values: BTreeMap::from([(
            42,
            bmz_skin::LuaSkinOffsetValue { x, ..Default::default() },
        )]),
        ..Default::default()
    };

    let first = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &offset(0),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.source[0].path, "zero.png");

    let same = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &offset(0),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(same.cache_status, DocumentCacheStatus::Hit);

    let changed = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &offset(12),
        Some(cache),
    )
    .unwrap();
    assert_eq!(changed.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(changed.document.source[0].path, "nonzero.png");
}

#[test]
fn lua_document_cache_misses_when_runtime_event_index_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-event-index");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("result.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local main_state = require("main_state")
local lnmode = main_state.event_index(308)
return {
    type = 7,
    source = {
        { id = "bg", path = lnmode == 0 and "ln.png" or "charge.png" },
    },
}
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let first = load_skin_document(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            event_index_values: BTreeMap::from([(308, 0)]),
            ..LuaLoadRuntimeState::default()
        },
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.source[0].path, "ln.png");

    let second = load_skin_document(
        &skin_path,
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            event_index_values: BTreeMap::from([(308, 2)]),
            ..LuaLoadRuntimeState::default()
        },
        Some(cache),
    )
    .unwrap();
    assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(second.document.source[0].path, "charge.png");
}

#[test]
fn lua_document_cache_misses_when_runtime_text_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-text");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("select.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local main_state = require("main_state")
return {
    type = 0,
    text = {
        { id = "player", constantText = main_state.text(2) },
    },
}
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let first = load_skin_document(
        &skin_path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            text_values: BTreeMap::from([(2, "Player One".to_string())]),
            ..LuaLoadRuntimeState::default()
        },
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(first.document.text[0].constant_text, "Player One");

    let second = load_skin_document(
        &skin_path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            text_values: BTreeMap::from([(2, "Player Two".to_string())]),
            ..LuaLoadRuntimeState::default()
        },
        Some(cache),
    )
    .unwrap();
    assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(second.document.text[0].constant_text, "Player Two");
}

#[test]
fn lua_document_cache_misses_when_used_file_selection_changes() {
    let root = unique_test_dir("bmz-lua-document-cache-file");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    std::fs::write(root.join("parts/blue.png"), []).unwrap();
    std::fs::write(root.join("parts/red.png"), []).unwrap();
    let skin_path = root.join("play.luaskin");
    std::fs::write(
        &skin_path,
        r#"
local path = "parts/blue.png"
if skin_config and skin_config.get_path then
    path = skin_config.get_path("parts/*.png")
end
return {
    type = 0,
    filepath = {
        { name = "Parts", path = "parts/*.png", def = "blue" },
    },
    source = {
        { id = "bg", path = path },
    },
}
"#,
    )
    .unwrap();
    let cache = Arc::new(Mutex::new(SkinDocumentCache::default()));

    let first = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        Some(cache.clone()),
    )
    .unwrap();
    assert_eq!(first.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(
        Path::new(&first.document.source[0].path).canonicalize().unwrap(),
        std::fs::canonicalize(root.join("parts/blue.png")).unwrap()
    );

    let selected = BTreeMap::from([("Parts".to_string(), "red.png".to_string())]);
    let second = load_skin_document(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &selected,
        &LuaLoadRuntimeState::default(),
        Some(cache),
    )
    .unwrap();
    assert_eq!(second.cache_status, DocumentCacheStatus::Miss);
    assert_eq!(
        Path::new(&second.document.source[0].path).canonicalize().unwrap(),
        std::fs::canonicalize(root.join("parts/red.png")).unwrap()
    );
}

#[test]
fn required_skin_sources_excludes_unused_images() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "source": [
                    { "id": 1, "path": "used.png" },
                    { "id": 2, "path": "unused.png" },
                    { "id": 3, "path": "lift.png" }
                ],
                "image": [
                    { "id": "used", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "unused", "src": 2, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "liftCover": [
                    { "id": "lift", "src": 3, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": "used", "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] },
                    { "id": "lift", "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] }
                ]
            }
            "#,
    )
    .unwrap();

    let required = required_skin_source_ids(&document);

    assert!(required.contains("1"));
    assert!(!required.contains("2"));
    assert!(required.contains("3"));
}

#[test]
fn supported_font_paths_include_vector_and_bitmap_fonts() {
    assert!(is_supported_font_path(Path::new("font.ttf")));
    assert!(is_supported_font_path(Path::new("font.OTF")));
    assert!(is_supported_font_path(Path::new("font.ttc")));
    assert!(is_supported_font_path(Path::new("font.fnt")));
    assert!(is_supported_font_path(Path::new("font.LR2FONT")));
    assert!(!is_supported_font_path(Path::new("font.png")));
    assert!(is_bitmap_font_path(Path::new("font.fnt")));
    assert!(is_bitmap_font_path(Path::new("font.lr2font")));
    assert!(!is_bitmap_font_path(Path::new("font.ttf")));
}

#[test]
fn skin_font_cache_hit_skips_loader() {
    let root = unique_test_dir("bmz-font-cache-hit");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("font.ttf");
    std::fs::write(&path, b"not a real font").unwrap();
    let key = skin_font_cache_key(&path).unwrap();
    let expected = vec![1, 2, 3, 4];
    let cache = Arc::new(Mutex::new(SkinFontCache::default()));
    cache.lock().unwrap().insert(key.clone(), DecodedFontData::Vector(expected.clone()));

    let (actual, status, actual_key) = decode_font_with_cache(&path, Some(&cache)).unwrap();

    assert_eq!(status, FontCacheStatus::Hit);
    assert_eq!(actual_key, Some(key));
    match actual {
        DecodedFontData::Vector(bytes) => assert_eq!(bytes, expected),
        DecodedFontData::Bitmap(_) => panic!("expected cached vector font bytes"),
    }
}

#[test]
fn skin_font_cache_evicts_least_recently_used_entry() {
    let mut cache = SkinFontCache::with_limit_bytes(8);
    let a = test_font_cache_key("a.ttf");
    let b = test_font_cache_key("b.ttf");
    let c = test_font_cache_key("c.ttf");

    cache.insert(a.clone(), DecodedFontData::Vector(vec![1, 1, 1, 1]));
    cache.insert(b.clone(), DecodedFontData::Vector(vec![2, 2, 2, 2]));
    assert!(cache.get(&a).is_some());
    cache.insert(c.clone(), DecodedFontData::Vector(vec![3, 3, 3, 3]));

    assert!(cache.get(&a).is_some());
    assert!(cache.get(&b).is_none());
    assert!(cache.get(&c).is_some());
}

#[test]
fn skin_font_cache_skips_entries_larger_than_limit() {
    let mut cache = SkinFontCache::with_limit_bytes(4);
    let key = test_font_cache_key("too-large.ttf");

    cache.insert(key.clone(), DecodedFontData::Vector(vec![1, 2, 3, 4, 5]));

    assert!(cache.get(&key).is_none());
    assert_eq!(cache.total_bytes, 0);
}

#[test]
fn installed_font_snapshot_skips_font_payload_decode() {
    let root = unique_test_dir("bmz-installed-font-skip");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("skin.json");
    let font_path = root.join("font.ttf");
    std::fs::write(&font_path, b"not a real font").unwrap();
    std::fs::write(
        &skin_path,
        r#"
            {
                "type": 0,
                "font": [
                    { "id": "font1", "path": "font.ttf" }
                ]
            }
            "#,
    )
    .unwrap();
    let key = skin_font_cache_key(&font_path).unwrap();
    let installed = HashMap::from([("play:font1".to_string(), key.clone())]);

    let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
        None,
        None,
        None,
        Some(installed),
    )
    .unwrap();

    assert_eq!(decoded.stats.font_count, 1);
    assert_eq!(decoded.stats.font_payload_skipped, 1);
    assert_eq!(decoded.stats.font_cache_hits, 0);
    assert_eq!(decoded.stats.font_cache_misses, 0);
    assert_eq!(decoded.fonts.len(), 1);
    assert_eq!(decoded.fonts[0].stored_id, "play:font1");
    assert_eq!(decoded.fonts[0].cache_key.as_ref(), Some(&key));
    assert!(decoded.fonts[0].data.is_none());
}

#[test]
fn bitmap_page_changes_invalidate_cached_and_installed_fonts() {
    for (extension, definition) in [
        (
            "fnt",
            "info face=Test size=1\ncommon lineHeight=1 base=1 scaleW=1 scaleH=1\npage id=0 file=\"page.png\"\nchar id=65 x=0 y=0 width=1 height=1 xoffset=0 yoffset=0 xadvance=1 page=0\n",
        ),
        ("lr2font", "#S,1\n#T,0,page.png\n#R,65,0,0,0,1,1\n"),
    ] {
        let root = unique_test_dir("bmz-bitmap-page-change");
        fs::create_dir_all(&root).unwrap();
        let font_path = root.join(format!("font.{extension}"));
        let page_path = root.join("page.png");
        fs::write(&font_path, definition).unwrap();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])).save(&page_path).unwrap();
        let cache = Arc::new(Mutex::new(SkinFontCache::default()));
        let (first, _, old_key) = decode_font_with_cache(&font_path, Some(&cache)).unwrap();
        let (second, status, _) = decode_font_with_cache(&font_path, Some(&cache)).unwrap();
        assert_eq!(status, FontCacheStatus::Hit);
        let (DecodedFontData::Bitmap(first), DecodedFontData::Bitmap(second)) = (first, second)
        else {
            panic!("bitmap font payloads")
        };
        assert!(Arc::ptr_eq(&first.pages, &second.pages));
        assert!(Arc::ptr_eq(&first.glyphs, &second.glyphs));
        let old_key = old_key.unwrap();
        image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 255, 0, 255])).save(&page_path).unwrap();
        fs::File::options()
            .write(true)
            .open(&page_path)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
            .unwrap();
        assert_ne!(skin_font_cache_key(&font_path).unwrap(), old_key);
        assert_eq!(fs::read_to_string(&font_path).unwrap(), definition);
        let skin_path = root.join("skin.json");
        fs::write(
            &skin_path,
            format!(r#"{{"type":0,"font":[{{"id":"page-test","path":"font.{extension}"}}]}}"#),
        )
        .unwrap();
        let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            None,
            None,
            None,
            Some(cache),
            Some(HashMap::from([("play:page-test".into(), old_key)])),
        )
        .unwrap();
        assert_eq!(decoded.stats.font_payload_skipped, 0);
        assert_eq!(decoded.stats.font_cache_misses, 1);
        let Some(DecodedFontData::Bitmap(font)) = &decoded.fonts[0].data else {
            panic!("bitmap font payload")
        };
        assert_eq!(font.pages[&0].image.width, 2);
        assert_eq!(&font.pages[&0].image.pixels[..4], &[0, 255, 0, 255]);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn skin_source_asset_cache_hit_skips_loader() {
    let root = unique_test_dir("bmz-source-cache-hit");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.png");
    std::fs::write(&path, b"cached").unwrap();
    let key = skin_source_asset_cache_key(&path, false).unwrap();
    let expected = RgbaImageAsset { width: 1, height: 1, pixels: vec![1, 2, 3, 4] };
    let cache = Arc::new(Mutex::new(SkinSourceAssetCache::default()));
    cache.lock().unwrap().insert(key, expected.clone());

    let (actual, status) = load_source_asset_with_cache(&path, false, Some(&cache), || {
        panic!("cache hit must not call source loader")
    })
    .unwrap();

    assert_eq!(actual, expected);
    assert_eq!(status, SourceCacheStatus::Hit);
}

#[test]
fn skin_source_asset_cache_misses_after_metadata_change() {
    let root = unique_test_dir("bmz-source-cache-metadata");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.png");
    std::fs::write(&path, b"old").unwrap();
    let key = skin_source_asset_cache_key(&path, false).unwrap();
    let stale = RgbaImageAsset { width: 1, height: 1, pixels: vec![1, 2, 3, 4] };
    let fresh = RgbaImageAsset { width: 1, height: 1, pixels: vec![5, 6, 7, 8] };
    let cache = Arc::new(Mutex::new(SkinSourceAssetCache::default()));
    cache.lock().unwrap().insert(key, stale);

    std::fs::write(&path, b"new and longer").unwrap();
    let (actual, status) =
        load_source_asset_with_cache(&path, false, Some(&cache), || Ok(fresh.clone())).unwrap();

    assert_eq!(actual, fresh);
    assert_eq!(status, SourceCacheStatus::Miss);
}

#[test]
fn skin_gpu_texture_cache_reuses_inserted_source_textures() {
    let root = unique_test_dir("bmz-gpu-texture-cache");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.png");
    std::fs::write(&path, b"cached").unwrap();
    let key = skin_source_asset_cache_key(&path, false).unwrap();
    let size = SkinImageSize { width: 64.0, height: 32.0 };
    let mut cache = SkinGpuTextureCache::default();

    let allocated = cache.allocate_texture_id(SkinKind::Play);
    cache.insert(key.clone(), allocated, size);

    let cached = cache.get(&key).unwrap();
    assert_eq!(cached.texture, allocated);
    assert_eq!(cached.size, size);
    assert_ne!(cache.allocate_texture_id(SkinKind::Play), allocated);

    cache.clear();

    assert!(cache.get(&key).is_none());
    assert_eq!(cache.allocate_texture_id(SkinKind::Play), SkinTextureId(10_000));
}

#[test]
fn skin_gpu_cache_evicts_lru_but_pins_active_and_pending_textures() {
    let mut cache = SkinGpuTextureCache::default();
    cache.limit_bytes = 8;
    let keys: Vec<_> = (0..3)
        .map(|i| SkinSourceAssetCacheKey {
            path: PathBuf::from(format!("image-{i}.png")),
            modified: None,
            len: 4,
            is_video: false,
        })
        .collect();
    let ids: Vec<_> = keys
        .iter()
        .map(|key| {
            let id = cache.allocate_texture_id(SkinKind::Play);
            cache.insert(key.clone(), id, SkinImageSize { width: 1.0, height: 1.0 });
            id
        })
        .collect();
    drop(cache.get(&keys[0])); // newest access; entry 1 is the least recently used
    assert_eq!(cache.evict_unused(&HashSet::new()), vec![ids[1]]);
    assert_eq!(cache.allocate_texture_id(SkinKind::Play), ids[1]);
    let pending = cache.get(&keys[2]).unwrap();
    cache.limit_bytes = 0;
    let active = HashSet::from([ids[0]]);
    assert!(cache.evict_unused(&active).is_empty());
    drop(pending);
    assert_eq!(cache.evict_unused(&active), vec![ids[2]]);
    assert!(cache.get(&keys[2]).is_none());
    assert!(cache.get(&keys[0]).is_some());
    assert_eq!(cache.evict_unused(&HashSet::new()), vec![ids[0]]);
    assert!(cache.entries.is_empty());
}

#[test]
fn decode_uses_gpu_texture_cache_to_skip_source_decode() {
    let root = unique_test_dir("bmz-source-texture-cache-hit");
    std::fs::create_dir_all(&root).unwrap();
    let skin_path = root.join("skin.json");
    let source_path = root.join("source.png");
    std::fs::write(&source_path, b"not a png").unwrap();
    std::fs::write(
        &skin_path,
        r#"
            {
                "type": 0,
                "source": [
                    { "id": 1, "path": "source.png" }
                ],
                "image": [
                    { "id": "img", "src": 1, "x": 0, "y": 0, "w": 64, "h": 32 }
                ],
                "destination": [
                    { "id": "img", "dst": [{ "x": 0, "y": 0, "w": 64, "h": 32 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let key = skin_source_asset_cache_key(&source_path, false).unwrap();
    let texture = SkinTextureId(12_345);
    let size = SkinImageSize { width: 64.0, height: 32.0 };
    let texture_cache = Arc::new(Mutex::new(SkinGpuTextureCache::default()));
    texture_cache.lock().unwrap().insert(key.clone(), texture, size);

    let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        &skin_path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        None,
        None,
        Some(texture_cache),
        None,
        None,
    )
    .unwrap();

    assert_eq!(decoded.stats.source_texture_cache_hits, 1);
    assert_eq!(decoded.stats.source_texture_cache_hit_bytes, 64 * 32 * 4);
    assert_eq!(decoded.stats.source_cache_hits, 0);
    assert_eq!(decoded.stats.source_cache_misses, 0);
    assert_eq!(decoded.stats.decoded_source_bytes, 0);
    assert_eq!(decoded.sources.len(), 1);
    assert_eq!(decoded.sources[0].texture, texture);
    assert_eq!(decoded.sources[0].size, size);
    assert_eq!(decoded.sources[0].cache_key.as_ref(), Some(&key));
    assert!(decoded.sources[0].asset.is_none());
}

#[test]
fn skin_gpu_texture_cache_reuses_inserted_video_textures_separately() {
    let root = unique_test_dir("bmz-gpu-video-texture-cache");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.mp4");
    std::fs::write(&path, b"cached-video").unwrap();
    let image_key = skin_source_asset_cache_key(&path, false).unwrap();
    let video_key = skin_source_asset_cache_key(&path, true).unwrap();
    assert_ne!(image_key, video_key);

    let size = SkinImageSize { width: 320.0, height: 180.0 };
    let mut cache = SkinGpuTextureCache::default();
    let allocated = cache.allocate_texture_id(SkinKind::Play);
    cache.insert(video_key.clone(), allocated, size);

    assert!(cache.get(&image_key).is_none());
    let cached = cache.get(&video_key).unwrap();
    assert_eq!(cached.texture, allocated);
    assert_eq!(cached.size, size);
}
