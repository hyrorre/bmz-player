use super::*;

#[test]
fn lua_skin_main_state_offset_exposes_zero_defaults_by_id() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local main_state = require("main_state")
            if skin_config then
                local offset = main_state.offset(4)
                return {
                    type = 0,
                    destination = {
                        { id = -110, dst = {{
                            x = offset.x,
                            y = 1080 + offset.y,
                            w = offset.w,
                            h = offset.h,
                            r = offset.r,
                            a = offset.a
                        }} }
                    }
                }
            end
            return { type = 0 }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["destination"][0]["dst"][0]["x"], 0);
    assert_eq!(loaded.value["destination"][0]["dst"][0]["y"], 1080);
    assert_eq!(loaded.value["destination"][0]["dst"][0]["w"], 0);
    assert_eq!(loaded.value["destination"][0]["dst"][0]["h"], 0);
    assert_eq!(loaded.value["destination"][0]["dst"][0]["r"], 0);
    assert_eq!(loaded.value["destination"][0]["dst"][0]["a"], 0);
}

#[test]
fn lua_skin_main_state_offset_exposes_load_time_values_by_id() {
    let root = unique_test_dir("bmz-skin-lua-offset-id");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local main_state = require("main_state")
            local skin = {
                type = 0,
                offset = {
                    { name = "Load-time offset", id = 4, x = true, y = true, w = true,
                      h = true, r = true, a = true }
                }
            }
            if skin_config then
                local offset = main_state.offset(4)
                skin.destination = {
                    { id = -110, dst = {{
                        x = offset.x,
                        y = offset.y,
                        w = offset.w,
                        h = offset.h,
                        r = offset.r,
                        a = offset.a
                    }} }
                }
            end
            return skin
            "#,
    )
    .unwrap();

    let offset = LuaSkinOffsetValue { x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 };
    let runtime_state = LuaLoadRuntimeState {
        offset_id_values: BTreeMap::from([(4, offset)]),
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
    assert_eq!(dst["y"], 2);
    assert_eq!(dst["w"], 3);
    assert_eq!(dst["h"], 4);
    assert_eq!(dst["r"], 5);
    assert_eq!(dst["a"], -6);
    assert_eq!(loaded.dependencies.offset_id_values.get(&4), Some(&offset));
}

#[test]
fn lua_skin_uses_explicit_play_mode_options() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            if skin_config then
                local graph = {}
                if main_state.option(32) then
                    table.insert(graph, { id = "score", src = 1, x = 0, y = 0, w = 1, h = 10, type = 110 })
                end
                return {
                    type = 0,
                    graph = graph,
                    image = main_state.option(33) and {{ id = "autoplay", src = 1, x = 0, y = 0, w = 1, h = 1 }} or {}
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

    let unresolved =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
    assert_eq!(unresolved.value["graph"].as_array().map(Vec::len), Some(0));
    assert_eq!(unresolved.value["image"].as_array().map(Vec::len), Some(0));

    let runtime_state = LuaLoadRuntimeState {
        option_values: BTreeMap::from([(32, true), (33, false)]),
        ..LuaLoadRuntimeState::default()
    };
    let loaded = load_lua_skin_value_with_runtime_state(
        &root.join("play7.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .unwrap();

    assert_eq!(loaded.value["graph"][0]["id"], "score");
    assert_eq!(loaded.value["image"].as_array().map(Vec::len), Some(0));
}

#[test]
fn lua_skin_os_clock_after_draw_becomes_elapsed_timer_condition() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local function after(ms)
                local start_time = nil
                return function()
                    start_time = start_time or os.clock()
                    return (os.clock() - start_time) * 1000 >= ms
                end
            end
            if skin_config then
                return {
                    type = 0,
                    image = {{ id = "keyflash", src = 1, x = 0, y = 0, w = 1, h = 1 }},
                    destination = {{
                        id = "keyflash",
                        timer = 101,
                        draw = after(1800),
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    }}
                }
            end
            return { type = 0 }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["destination"][0]["draw"], "timer(0) >= 1800");
}

#[test]
fn lua_skin_os_clock_after_and_option_draw_becomes_elapsed_timer_and_option_condition() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            local function after_and_op(ms, ...)
                local start_time = nil
                local ops = {...}
                return function()
                    start_time = start_time or os.clock()
                    if (os.clock() - start_time) * 1000 < ms then
                        return false
                    end
                    for _, op in ipairs(ops) do
                        if not main_state.option(op) then
                            return false
                        end
                    end
                    return true
                end
            end
            if skin_config then
                return {
                    type = 0,
                    value = {{ id = "lanecover-value", src = 1, x = 0, y = 0, w = 10, h = 1, divx = 10, digit = 3, ref = 14 }},
                    destination = {{
                        id = "lanecover-value",
                        draw = after_and_op(1800, 270, 177),
                        dst = {{ x = 0, y = 0, w = 1, h = 1 }}
                    }}
                }
            end
            return { type = 0 }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(
        loaded.value["destination"][0]["draw"],
        "timer(0) >= 1800 and option(270) and option(177)"
    );
}

#[test]
fn lua_skin_load_time_table_level_text_ref_is_preserved() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local main_state = require("main_state")
            local table_text = main_state.text(1002)
            if skin_config then
                return {
                    type = 0,
                    text = {{
                        id = "tableLevel",
                        font = 3,
                        size = 18,
                        value = function()
                            return table_text
                        end
                    }}
                }
            end
            return { type = 0 }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["ref"], 1002);
    assert!(loaded.value["text"][0].get("constantText").is_none());
}

#[test]
fn lua_skin_text_returning_a_number_ref_becomes_number_ref() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
        r#"
            local main_state = require("main_state")
            local function notes() return main_state.number(1938) end
            return {
                type = 5,
                text = {{ id = "notes", font = 0, size = 24, value = notes }},
                value = {{ id = "score", src = 0, divx = 10, digit = 4, value = notes }},
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    // A text `ref` is a StringProperty; the number is kept through `numberRef`.
    assert_eq!(loaded.value["text"][0]["numberRef"], 1938);
    assert!(loaded.value["text"][0].get("ref").is_none());
    assert_eq!(loaded.value["value"][0]["ref"], 1938);
}

#[test]
fn lua_skin_mz_select_result_title_becomes_runtime_expr() {
    let root = unique_test_dir("bmz-skin-lua-mz-select-result-title");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            local main_state = require("main_state")
            local title = main_state.text(1002) .. " " .. main_state.text(1001)
            if title then title = title .. " " end
            title = title .. main_state.text(12)
            return {
                type = 7,
                text = {{ id = "title", font = 0, size = 24, constantText = title }},
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(
        loaded.value["text"][0]["value_expr"],
        bmz_skin_document::SKIN_EXPR_RESULT_TABLE_TITLE
    );
    assert!(loaded.value["text"][0].get("constantText").is_none());
}

#[test]
fn lua_skin_event_util_module_loads_custom_event_helpers() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local event_util = require("event_util")
            local count = 0
            local action = event_util.event_observe_turn_true(
                function() return true end,
                function() count = count + 1 end
            )
            action()
            action()
            return {
                type = 0,
                text = {
                    { id = "event-count", font = 1, size = 16, constantText = tostring(count) }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "1");
}

#[test]
fn lua_skin_os_stub_supports_date_clock_and_time() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local utc = os.date("!*t", 0)
            local elapsed = os.clock()
            local round_trip_input = { year = 2000, month = 1, day = 1, hour = 0 }
            local round_trip_epoch = os.time(round_trip_input)
            assert(round_trip_epoch > 0)
            local epoch_round_trip = os.date("*t", round_trip_epoch)
            local default_hour_input = { year = 2000, month = 1, day = 1 }
            os.time(default_hour_input)
            local normalized = { year = 2024, month = 13, day = 1, hour = 25 }
            local normalized_epoch = os.time(normalized)
            local normalized_round_trip = os.date("*t", normalized_epoch)
            local extreme_ok = pcall(os.time, {
                year = math.mininteger, month = 1, day = 1
            })
            local now = os.time()
            return {
                type = 0,
                text = {
                    {
                        id = "timestamp",
                        font = 1,
                        size = 16,
                        constantText = os.date("!%Y-%m-%d %H:%M:%S", 0)
                            .. "|" .. utc.year
                            .. "|" .. tostring(elapsed >= 0)
                            .. "|" .. round_trip_input.year .. "-" .. round_trip_input.month
                            .. "-" .. round_trip_input.day .. " " .. round_trip_input.hour
                            .. "|" .. epoch_round_trip.year .. "-" .. epoch_round_trip.month
                            .. "-" .. epoch_round_trip.day .. " " .. epoch_round_trip.hour
                            .. "|" .. default_hour_input.hour
                            .. "|" .. normalized.year .. "-" .. normalized.month
                            .. "-" .. normalized.day .. " " .. normalized.hour
                            .. "|" .. normalized_round_trip.year .. "-" .. normalized_round_trip.month
                            .. "-" .. normalized_round_trip.day .. " " .. normalized_round_trip.hour
                            .. "|" .. tostring(type(normalized.isdst) == "boolean")
                            .. "|" .. tostring(extreme_ok)
                            .. "|" .. tostring(now >= 0)
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
        loaded.value["text"][0]["constantText"],
        "1970-01-01 00:00:00|1970|true|2000-1-1 0|2000-1-1 0|12|2025-1-2 1|2025-1-2 1|true|false|true"
    );
}

#[test]
fn lua_skin_load_budget_accepts_large_fixture_and_stops_runaway_code() {
    const FINITE_FIXTURE: &str = r#"
        local checksum = 0
        for i = 1, 1000000 do
            checksum = checksum + (i % 7)
        end
        return checksum
    "#;

    let old_limit_lua = mlua::Lua::new();
    crate::lua::install_test_instruction_limit(&old_limit_lua, 2_000_000, 2_000_000);
    assert!(old_limit_lua.load(FINITE_FIXTURE).eval::<i64>().is_err());

    let load_limit_lua = mlua::Lua::new();
    crate::lua::install_test_instruction_limit(&load_limit_lua, 16_000_000, 16_000_000);
    assert!(load_limit_lua.load(FINITE_FIXTURE).eval::<i64>().is_ok());

    let runaway_lua = mlua::Lua::new();
    crate::lua::install_test_instruction_limit(&runaway_lua, 16_000_000, 16_000_000);
    let error = runaway_lua.load("while true do end").exec().unwrap_err();
    assert!(error.to_string().contains("instruction limit"));
}

#[test]
fn lua_skin_litone_result_fixtures_cover_compatibility_apis() {
    let root = unique_test_dir("bmz-skin-litone-result-fixture");
    fs::create_dir_all(&root).unwrap();

    for (file_name, skin_type) in [("result.luaskin", 7), ("course.luaskin", 15)] {
        let source = format!(
            r#"
                local Controllers = luajava.bindClass(
                    "com.badlogic.gdx.controllers.Controllers"
                )
                local controllers = Controllers:getControllers()
                local date = {{ year = 2024, month = 13, day = 1, hour = 25 }}
                local timestamp = os.time(date)
                return {{
                    type = {skin_type},
                    name = "LITONE compatibility fixture",
                    slider = {{
                        {{ id = "history", isRefNum = 1, value = 1 }}
                    }},
                    graph = {{
                        {{ id = "timing", isRefNum = 0, value = 2 }}
                    }},
                    text = {{
                        {{
                            id = "compat",
                            font = 1,
                            size = 16,
                            constantText = tostring(controllers:first() == nil)
                                .. "|" .. date.year .. "-" .. date.month
                                .. "-" .. date.day .. " " .. date.hour
                                .. "|" .. tostring(timestamp > 0)
                        }}
                    }}
                }}
            "#
        );
        let path = root.join(file_name);
        fs::write(&path, source).unwrap();

        let loaded = load_lua_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        let document: bmz_skin_document::SkinDocument =
            serde_json::from_value(loaded.value).unwrap();
        assert_eq!(document.skin_type, skin_type);
        assert!(document.slider[0].is_ref_num);
        assert!(!document.graph[0].is_ref_num);
        assert_eq!(document.text[0].constant_text, "true|2025-1-2 1|true");
    }
}

#[test]
fn lua_skin_io_stub_reads_skin_alias_from_renamed_root_and_ignores_writes() {
    let parent = unique_test_dir("bmz-skin-lua");
    let root = parent.join("mz-select");
    fs::create_dir_all(root.join("customize/advanced")).unwrap();
    fs::write(root.join("customize/advanced/enable.txt"), "parts.lua\n").unwrap();
    fs::write(
        root.join("customize/advanced/parts.lua"),
        r#"
            return {
                load = function()
                    return "loaded"
                end
            }
            "#,
    )
    .unwrap();
    fs::write(
        root.join("music_select.luaskin"),
        r#"
            local f = io.open("skin/m_select/customize/advanced/enable.txt", "r")
            local out = io.open("skin/m_select/customize/advanced/load_log.txt", "w")
            local count = 0
            for line in f:lines() do
                count = count + 1
                out:write(line)
                local parts = dofile("skin/m_select/customize/advanced/" .. line)
                if parts.load() == "loaded" then
                    count = count + 1
                end
            end
            for _ in io.lines("skin/m_select/customize/advanced/enable.txt") do
                count = count + 1
            end
            io.close(f)
            out:close()
            return {
                type = 0,
                text = {
                    { id = "line-count", font = 1, size = 16, constantText = tostring(count) }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("music_select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "3");
    assert!(!root.join("customize/advanced/load_log.txt").exists());
}

#[test]
fn lua_skin_io_read_all_lines_and_close_share_a_read_only_cursor() {
    let root = unique_test_dir("bmz-skin-lua-io-read");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("config.txt"), "alpha\r\nbeta\n").unwrap();
    fs::write(
            root.join("result.luaskin"),
            r#"
            local f = io.open("config.txt", "r")
            local all = f:read("*a")
            local eof = f:read("*all")
            f:close()
            local read_after_close = pcall(function() f:read("*a") end)
            local lines = {}
            for line in io.lines("config.txt") do
                table.insert(lines, line)
            end
            return {
                type = 7,
                text = {{
                    id = "io",
                    font = 1,
                    size = 16,
                    constantText = all .. "|" .. eof .. "|" .. tostring(read_after_close) .. "|" .. table.concat(lines, ",")
                }}
            }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "alpha\r\nbeta\n||false|alpha,beta");
    assert!(
        loaded
            .dependencies
            .loaded_files
            .contains_key(&root.join("config.txt").canonicalize().unwrap())
    );
}

#[test]
fn lua_skin_virtual_io_loads_wmii_style_player_config_without_host_access() {
    let root = unique_test_dir("bmz-skin-lua-virtual-io");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("result.luaskin"),
            r#"
            local sys = io.open("config_sys.json", "r")
            local player = sys:read("*a"):match('"playername"%s*:%s*"([^"]+)"')
            sys:close()
            local path = "player/" .. player .. "/config_player.json"
            local config = io.open(path, "r")
            local contents = config:read("*all")
            config:close()
            return {
                type = 7,
                text = {{ id = "config", font = 1, size = 16, constantText = path .. "|" .. contents }}
            }
            "#,
        )
        .unwrap();
    let virtual_files = BTreeMap::from([
        ("config_sys.json".to_string(), r#"{"playername":"bmz"}"#.to_string()),
        (
            "player\\bmz\\config_player.json".to_string(),
            r#"{"mode7":{"keyboard":{},"controller":[],"midi":{}}}"#.to_string(),
        ),
    ]);

    let loaded = load_lua_skin_value_with_runtime_state_and_virtual_io_files(
        &root.join("result.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        &virtual_files,
    )
    .unwrap();

    assert_eq!(
        loaded.value["text"][0]["constantText"],
        r#"player/bmz/config_player.json|{"mode7":{"keyboard":{},"controller":[],"midi":{}}}"#
    );
    assert_eq!(
        loaded.dependencies.virtual_io_files,
        BTreeMap::from([
            ("config_sys.json".to_string(), Some(r#"{"playername":"bmz"}"#.to_string())),
            (
                "player/bmz/config_player.json".to_string(),
                Some(r#"{"mode7":{"keyboard":{},"controller":[],"midi":{}}}"#.to_string())
            ),
        ])
    );
    assert!(!loaded.dependencies.opaque);
}

#[test]
fn lua_skin_virtual_io_dependency_snapshot_changes_with_contents() {
    let root = unique_test_dir("bmz-skin-lua-virtual-io-dependency");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("result.luaskin"),
            r#"
            local f = io.open("config_sys.json", "r")
            local contents = f:read("*a")
            f:close()
            return { type = 7, text = {{ id = "config", font = 1, size = 16, constantText = contents }} }
            "#,
        )
        .unwrap();
    let load = |contents: &str| {
        load_lua_skin_value_with_runtime_state_and_virtual_io_files(
            &root.join("result.luaskin"),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            &BTreeMap::from([("config_sys.json".to_string(), contents.to_string())]),
        )
        .unwrap()
    };

    let first = load("first");
    let second = load("second");
    assert_ne!(first.dependencies.virtual_io_files, second.dependencies.virtual_io_files);
    assert_eq!(second.dependencies.virtual_io_files["config_sys.json"], Some("second".to_string()));
}

#[test]
fn lua_skin_io_rejects_traversal_and_oversized_virtual_files() {
    let parent = unique_test_dir("bmz-skin-lua-io-security");
    let root = parent.join("skin");
    fs::create_dir_all(&root).unwrap();
    fs::write(parent.join("secret.txt"), "secret").unwrap();
    fs::write(
            root.join("result.luaskin"),
            r#"
            local paths = { "../secret.txt", "C:\\secret.txt", "//server/share/secret.txt" }
            local opened = 0
            for _, path in ipairs(paths) do
                if io.open(path, "r") then opened = opened + 1 end
            end
            return { type = 7, text = {{ id = "opened", font = 1, size = 16, constantText = tostring(opened) }} }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
    assert_eq!(loaded.value["text"][0]["constantText"], "0");

    let error = load_lua_skin_value_with_runtime_state_and_virtual_io_files(
        &root.join("result.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        &BTreeMap::from([("config_sys.json".to_string(), "x".repeat(8 * 1024 * 1024 + 1))]),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("exceeds 8388608 byte limit"));
}

#[test]
fn lua_skin_main_state_stubs_audio_volume_helpers() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("sound.wav"), []).unwrap();
    fs::write(
            root.join("play7.luaskin"),
            r#"
            local main_state = require("main_state")
            local ok = main_state.audio_play("sound.wav", main_state.volume_sys())
            return {
                type = 0,
                text = {
                    { id = "volume", font = 1, size = 16, constantText = tostring(main_state.volume_key() + main_state.volume_bg()) .. "|" .. tostring(ok) }
                }
            }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "2.0|true");
    assert_eq!(
        loaded.value["sceneAudio"],
        serde_json::json!([
            { "action": "play", "path": "sound.wav", "volume": 1.0 }
        ])
    );
}

#[test]
fn lua_skin_converts_timer_custom_event_audio_actions() {
    let root = unique_test_dir("bmz-skin-lua-audio-event");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("bgm.ogg"), []).unwrap();
    fs::write(root.join("close.ogg"), []).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            local main_state = require("main_state")
            local timer_util = require("timer_util")
            main_state.audio_loop("bgm.ogg", 0.8)
            local called = false
            return {
                type = 7,
                customEvents = {{
                    id = 1001,
                    action = function()
                        if not called then
                            called = true
                            main_state.audio_stop("bgm.ogg")
                            main_state.audio_play("close.ogg", 0.5)
                        end
                    end,
                    condition = function()
                        return timer_util.is_timer_on(main_state.timer(2))
                    end,
                }},
            }
            "#,
    )
    .unwrap();

    let loaded = load_lua_skin(
        &root.join("result.luaskin"),
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();

    assert_eq!(loaded.document.scene_audio.len(), 1);
    assert_eq!(loaded.document.scene_audio[0].path, "bgm.ogg");
    assert_eq!(loaded.document.custom_events.len(), 1);
    let event = &loaded.document.custom_events[0];
    assert_eq!(event.id, 1001);
    assert_eq!(event.timer, 2);
    assert!(event.once);
    assert_eq!(event.audio_actions.len(), 2);
    assert!(loaded.warnings.is_empty(), "warnings: {:?}", loaded.warnings);
}
