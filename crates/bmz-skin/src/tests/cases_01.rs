use super::*;

#[test]
fn lua_skin_loads_main_state_draw_and_value_functions() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                value = {
                    { id = "score", src = 1, x = 0, y = 0, w = 10, h = 10, value = function()
                        return main_state.number(71)
                    end }
                },
                destination = {
                    { id = "panel", draw = function() return main_state.option(1) end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} }
                }
            }
            "#,
        )
        .unwrap();

    let loaded = load_lua_skin(
        &root.join("play7.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    assert!(loaded.warnings.is_empty());
    assert_eq!(loaded.document.value[0].ref_id, 71);
    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("destination should be single");
    };
    assert_eq!(destination.draw, "option(1)");
}

#[test]
fn lua_skin_preserves_destination_angle_for_shared_renderer_conversion() {
    let root = unique_test_dir("bmz-skin-lua-rotation");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("play7.luaskin"),
            r#"
            return {
                type = 0,
                destination = {
                    { id = "turntable", offset = 1, dst = {{ x = 1, y = 2, w = 3, h = 4, angle = -90 }} }
                }
            }
            "#,
        )
        .unwrap();

    let loaded = load_lua_skin(
        &root.join("play7.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("destination should be single");
    };
    let bmz_skin_document::SkinDstEntry::Frame(frame) = &destination.dst[0] else {
        panic!("destination frame should be static");
    };

    assert_eq!(frame.angle, Some(-90));
    assert_eq!(destination.offset, 1);
}

#[test]
fn lua_skin_runtime_option_is_available_during_load() {
    let root = unique_test_dir("bmz-skin-lua-runtime-option");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            local main_state = require("main_state")
            local y = 18
            if main_state.option(1008) then
                y = 45
            end
            return {
                type = 7,
                destination = {
                    { id = "panel", dst = {{ x = 1, y = y, w = 3, h = 4 }} }
                }
            }
            "#,
    )
    .unwrap();

    let loaded = load_lua_skin_with_runtime_state(
        &root.join("result.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            number_values: BTreeMap::new(),
            text_values: BTreeMap::new(),
            option_values: BTreeMap::from([(1008, true)]),
            ..LuaLoadRuntimeState::default()
        },
    )
    .unwrap();

    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("destination should be single");
    };
    let bmz_skin_document::SkinDstEntry::Frame(frame) = &destination.dst[0] else {
        panic!("destination frame should be static");
    };
    assert_eq!(frame.y, Some(45));
}

#[test]
fn lua_skin_runtime_event_index_is_available_during_load() {
    let root = unique_test_dir("bmz-skin-lua-runtime-event-index");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            local main_state = require("main_state")
            local row = main_state.event_index(308)
            return {
                type = 7,
                image = {
                    { id = "ln-type", src = 1, x = 0, y = 10 + row * 19, w = 50, h = 19 }
                }
            }
            "#,
    )
    .unwrap();

    let loaded = load_lua_skin_with_runtime_state(
        &root.join("result.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState {
            event_index_values: BTreeMap::from([(308, 2)]),
            ..LuaLoadRuntimeState::default()
        },
    )
    .unwrap();

    assert_eq!(loaded.document.image[0].y, 48);
    assert_eq!(loaded.dependencies.event_index_values.get(&308), Some(&2));
}

#[test]
fn lua_skin_infers_option_and_number_draw_conditions() {
    let root = unique_test_dir("bmz-skin-lua-option-number-draw");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play.luaskin"),
        r#"
            local main_state = require("main_state")
            local function nonzero(ref)
                return main_state.number(ref) ~= 0
            end
            return {
                type = 0,
                destination = {
                    { id = "fast", draw = function()
                        return main_state.option(1242) and nonzero(525)
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} },
                    { id = "ms", draw = function()
                        return not main_state.option(241) and nonzero(525)
                    end, dst = {{ x = 1, y = 2, w = 3, h = 4 }} },
                }
            }
            "#,
    )
    .unwrap();

    let loaded = load_lua_skin(
        &root.join("play.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    let bmz_skin_document::DestinationListEntry::Single(fast) = &loaded.document.destination[0]
    else {
        panic!("expected fast destination");
    };
    let bmz_skin_document::DestinationListEntry::Single(ms) = &loaded.document.destination[1]
    else {
        panic!("expected ms destination");
    };
    assert_eq!(fast.draw, "option(1242) && number(525) != 0");
    assert_eq!(ms.draw, "!option(241) && number(525) != 0");
}

#[test]
fn lua_skin_records_required_module_skin_config_option_dependency() {
    let root = unique_test_dir("bmz-skin-lua-required-option-dependency");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("play.luaskin"), "local parts = require('parts')\nreturn parts.build()")
        .unwrap();
    fs::write(
            root.join("parts.lua"),
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

    let loaded = load_lua_skin_with_runtime_state(
        &root.join("play.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();

    assert_eq!(loaded.document.source[0].path, "off.png");
    assert!(loaded.dependencies.option_values.contains_key(&910));
}

#[test]
fn lua_skin_rejects_paths_outside_root() {
    // The escape target must exist beside the root, so own the parent as well: the
    // root of a shared temp directory would be written by every concurrent test run.
    let parent = tempfile::Builder::new().prefix("bmz-skin-lua-").tempdir().unwrap();
    let root = parent.path().join("skin");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("play7.luaskin"), "return dofile('../outside.lua')").unwrap();
    fs::write(parent.path().join("outside.lua"), "return {}").unwrap();

    let err = load_lua_skin(
        &root.join("play7.luaskin"),
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("escapes skin root"));
}

#[test]
fn lua_skin_config_get_path_ignores_beatoraja_filter_suffix() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
    fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local cover_path = "parts/lanecover_lift/*.png|lanecover|"
            if skin_config then
                cover_path = skin_config.get_path(cover_path)
            end
            return {
                type = 0,
                source = {
                    {
                        id = "cover",
                        path = cover_path
                    }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(
        loaded.value["source"][0]["path"].as_str().and_then(|path| {
            std::path::Path::new(path).file_name().and_then(|name| name.to_str())
        }),
        Some("default.png")
    );
}

#[test]
fn lua_skin_header_load_skips_skin_config_body() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(root.join("parts")).unwrap();
    fs::write(root.join("parts/frame.lua"), "return {}").unwrap();
    fs::write(
        root.join("play5.luaskin"),
        r#"
            if skin_config then
                dofile(skin_config.get_path("parts/*") .. "/frame.lua")
            end
            return {
                name = "Header Only",
                type = 1
            }
            "#,
    )
    .unwrap();

    let header = load_lua_skin_header_value(&root.join("play5.luaskin")).unwrap();

    assert_eq!(header.value["name"], "Header Only");
    assert_eq!(header.value["type"], 1);
}

#[test]
fn lua_skin_config_get_path_applies_user_file_selection() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(root.join("parts")).unwrap();
    fs::write(root.join("parts/a.png"), []).unwrap();
    fs::write(root.join("parts/z.png"), []).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local cover_path = "parts/*.png"
            if skin_config then
                cover_path = skin_config.get_path(cover_path)
            end
            return {
                type = 0,
                filepath = {
                    { name = "Cover", path = "parts/*.png", def = "a" }
                },
                source = {
                    { id = "cover", path = cover_path }
                }
            }
            "#,
    )
    .unwrap();

    let files = BTreeMap::from([("Cover".to_string(), "parts/z.png".to_string())]);
    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &files).unwrap();

    assert_eq!(
        loaded.value["source"][0]["path"].as_str().and_then(|path| {
            std::path::Path::new(path).file_name().and_then(|name| name.to_str())
        }),
        // ユーザ選択 (z.png) を採用する。ソート先頭候補は a.png。
        Some("z.png")
    );
}

#[test]
fn lua_skin_config_get_path_applies_directory_selection_to_child_wildcard() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(root.join("Theme/a/_lua")).unwrap();
    fs::create_dir_all(root.join("Theme/z/_lua")).unwrap();
    fs::write(
        root.join("Theme/a/_lua/frame.lua"),
        r#"return { source = { { id = "frame", path = "Theme/a/frame.png" } } }"#,
    )
    .unwrap();
    fs::write(
        root.join("Theme/z/_lua/frame.lua"),
        r#"return { source = { { id = "frame", path = "Theme/z/frame.png" } } }"#,
    )
    .unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            if skin_config then
                local parts = dofile(skin_config.get_path("Theme/*/_lua") .. "/frame.lua")
                return {
                    type = 7,
                    filepath = {
                        { name = "Theme", path = "Theme/*", def = "a" }
                    },
                    source = parts.source
                }
            end
            return {
                type = 7,
                filepath = {
                    { name = "Theme", path = "Theme/*", def = "a" }
                }
            }
            "#,
    )
    .unwrap();

    let files = BTreeMap::from([("Theme".to_string(), "Theme/z".to_string())]);
    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &files).unwrap();

    assert_eq!(loaded.value["source"][0]["path"], "Theme/z/frame.png");
}

#[test]
fn lua_skin_config_offset_exposes_zero_defaults_by_name() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local alpha = 255
            if skin_config then
                alpha = skin_config.offset["Panel alpha"].a
            end
            return {
                type = 0,
                offset = {
                    { name = "Panel alpha", id = 42, a = true }
                },
                destination = {
                    { id = -110, dst = {{ x = 1, y = 2, w = 3, h = 4, a = alpha }} }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["destination"][0]["dst"][0]["a"], 0);
}

#[test]
fn lua_offset_definitions_use_first_saved_name_and_last_runtime_id() {
    let first = LuaSkinOffsetValue { x: 1, ..Default::default() };
    let duplicate_saved = LuaSkinOffsetValue { x: 2, ..Default::default() };
    let later_id = LuaSkinOffsetValue { x: 3, ..Default::default() };
    let mut runtime_state = LuaLoadRuntimeState::default();

    runtime_state.set_offset_definitions(
        [
            ("Same name".to_string(), 7),
            ("Same name".to_string(), 8),
            ("Later ID".to_string(), 7),
            ("Missing".to_string(), 9),
        ],
        [
            ("Same name".to_string(), first),
            ("Same name".to_string(), duplicate_saved),
            ("Later ID".to_string(), later_id),
        ],
    );

    assert_eq!(runtime_state.offset_values.get("Same name"), Some(&first));
    assert_eq!(runtime_state.offset_values.get("Later ID"), Some(&later_id));
    assert_eq!(runtime_state.offset_values.get("Missing"), Some(&LuaSkinOffsetValue::default()));
    assert_eq!(runtime_state.offset_id_values.get(&7), Some(&later_id));
    assert_eq!(runtime_state.offset_id_values.get(&8), Some(&first));
    assert_eq!(runtime_state.offset_id_values.get(&9), Some(&LuaSkinOffsetValue::default()));
}

#[test]
fn lua_skin_config_offset_prefers_names_and_keeps_duplicate_ids_distinct() {
    let root = unique_test_dir("bmz-skin-lua-offset-names");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local first = { x = 0, y = 0, w = 0, h = 0, r = 0, a = 0 }
            local second = first
            local fallback = first
            local missing = first
            if skin_config then
                first = skin_config.offset["First"]
                second = skin_config.offset["Second"]
                fallback = skin_config.offset["ID fallback"]
                missing = skin_config.offset["Missing"]
            end
            return {
                type = 0,
                offset = {
                    { name = "First", id = 7, x = true, r = true },
                    { name = "Second", id = 7, y = true, a = true },
                    { name = "ID fallback", id = 8, w = true },
                    { name = "Missing", id = 9, h = true }
                },
                destination = {
                    { id = -110, dst = {{
                        x = first.x,
                        y = second.y,
                        w = fallback.w,
                        h = missing.h,
                        r = first.r,
                        a = second.a
                    }} }
                }
            }
            "#,
    )
    .unwrap();

    let first = LuaSkinOffsetValue { x: 11, r: 12, ..Default::default() };
    let second = LuaSkinOffsetValue { y: 21, a: -22, ..Default::default() };
    let fallback = LuaSkinOffsetValue { w: 31, ..Default::default() };
    let ignored_id_value = LuaSkinOffsetValue { x: 99, y: 99, w: 99, h: 99, r: 99, a: 99 };
    let runtime_state = LuaLoadRuntimeState {
        offset_values: BTreeMap::from([
            ("First".to_string(), first),
            ("Second".to_string(), second),
        ]),
        offset_id_values: BTreeMap::from([(7, ignored_id_value), (8, fallback)]),
        ..Default::default()
    };
    let loaded = load_lua_skin_value_with_runtime_state(
        &root.join("play7.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .unwrap();

    let dst = &loaded.value["destination"][0]["dst"][0];
    assert_eq!(dst["x"], 11);
    assert_eq!(dst["y"], 21);
    assert_eq!(dst["w"], 31);
    assert_eq!(dst["h"], 0);
    assert_eq!(dst["r"], 12);
    assert_eq!(dst["a"], -22);
    assert_eq!(loaded.dependencies.offset_values.get("First"), Some(&first));
    assert_eq!(loaded.dependencies.offset_values.get("Second"), Some(&second));
    assert_eq!(loaded.dependencies.offset_values.get("ID fallback"), Some(&fallback));
    assert_eq!(
        loaded.dependencies.offset_values.get("Missing"),
        Some(&LuaSkinOffsetValue::default())
    );
}

#[test]
fn lua_play_skin_config_appends_beatoraja_common_offsets() {
    let root = unique_test_dir("bmz-skin-lua-common-offsets");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local main_state = require("main_state")
            local zero = { x = 0, y = 0, w = 0, h = 0, r = 0, a = 0 }
            local all = zero
            local notes = zero
            local judge = zero
            local detail = zero
            local custom = zero
            local runtime_notes = zero
            local barline_is_absent = false
            if skin_config then
                all = skin_config.offset["All offset(%)"]
                notes = skin_config.offset["Notes offset"]
                judge = skin_config.offset["Judge offset"]
                detail = skin_config.offset["Judge Detail offset"]
                custom = skin_config.offset["Custom notes"]
                runtime_notes = main_state.offset(30)
                barline_is_absent = skin_config.offset["Bar Line offset"] == nil
            end
            return {
                type = 0,
                offset = {
                    { name = "Custom notes", id = 30, r = true }
                },
                destination = {
                    { id = -110, dst = {{
                        x = all.x,
                        y = judge.y,
                        w = detail.w,
                        h = notes.h + runtime_notes.h,
                        r = custom.r,
                        a = barline_is_absent and judge.a or 0
                    }} }
                }
            }
            "#,
    )
    .unwrap();

    let all = LuaSkinOffsetValue { x: 1, ..Default::default() };
    let notes = LuaSkinOffsetValue { h: 2, ..Default::default() };
    let judge = LuaSkinOffsetValue { y: 3, a: -6, ..Default::default() };
    let detail = LuaSkinOffsetValue { w: 4, ..Default::default() };
    let custom = LuaSkinOffsetValue { r: 5, ..Default::default() };
    let ignored_notes_id = LuaSkinOffsetValue { h: 99, r: 99, ..Default::default() };
    let runtime_state = LuaLoadRuntimeState {
        offset_values: BTreeMap::from([
            ("Notes offset".to_string(), notes),
            ("Judge offset".to_string(), judge),
            ("Judge Detail offset".to_string(), detail),
            ("Custom notes".to_string(), custom),
        ]),
        offset_id_values: BTreeMap::from([(10, all), (30, ignored_notes_id)]),
        ..Default::default()
    };
    let loaded = load_lua_skin_value_with_runtime_state(
        &root.join("play7.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .unwrap();

    let dst = &loaded.value["destination"][0]["dst"][0];
    assert_eq!(dst["x"], 1);
    assert_eq!(dst["y"], 3);
    assert_eq!(dst["w"], 4);
    assert_eq!(dst["h"], 4);
    assert_eq!(dst["r"], 5);
    assert_eq!(dst["a"], -6);
    assert_eq!(loaded.dependencies.offset_values.get("All offset(%)"), Some(&all));
    assert_eq!(loaded.dependencies.offset_values.get("Notes offset"), Some(&notes));
    assert_eq!(loaded.dependencies.offset_values.get("Judge offset"), Some(&judge));
    assert_eq!(loaded.dependencies.offset_values.get("Judge Detail offset"), Some(&detail));
    assert!(!loaded.dependencies.offset_values.contains_key("Bar Line offset"));
}

#[test]
fn antique_play_skin_bakes_named_note_size_offset_when_available() {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/play/antique/system/play7main.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let runtime_state = LuaLoadRuntimeState {
        offset_values: BTreeMap::from([(
            "ノーツの大きさ".to_string(),
            LuaSkinOffsetValue { h: 9, ..Default::default() },
        )]),
        ..Default::default()
    };
    let loaded = load_lua_skin_with_runtime_state(
        &skin_path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .expect("Antique play skin should decode with a named note-size offset");

    let note = loaded.document.note.as_ref().expect("Antique note definition");
    assert_eq!(note.size, vec![45; 8]);
    assert_eq!(
        loaded.dependencies.offset_values.get("ノーツの大きさ"),
        runtime_state.offset_values.get("ノーツの大きさ")
    );
}
