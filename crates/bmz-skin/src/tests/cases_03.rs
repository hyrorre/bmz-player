use super::*;

#[test]
fn lua_skin_preserves_text_shrink_mode() {
    let root = unique_test_dir("bmz-skin-text-shrink-mode");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.luaskin");
    fs::write(
        &path,
        r#"
            return {
                type = 0,
                text = {
                    { id = "title", size = 24, overflow = 1, shrinkMode = 1 }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();

    assert_eq!(loaded.document.text[0].overflow, 1);
    assert_eq!(loaded.document.text[0].shrink_mode, 1);
}

#[test]
fn lua_skin_luajava_stub_loads_legacy_sound_helper() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
        r#"
            local luajava = require("luajava")
            local gdx = luajava.bindClass("com.badlogic.gdx.Gdx")
            pcall(function() gdx.app:getApplicationListener():getAudioProcessor():play("x", 1) end)
            return {
                type = 0,
                text = {
                    { id = "loaded", font = 1, size = 16, constantText = "ok" }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "ok");
}

#[test]
fn lua_skin_global_luajava_exposes_gdx_graphics_dimensions() {
    let root = unique_test_dir("bmz-skin-lua-global-luajava-graphics");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            local Gdx = luajava.bindClass("com.badlogic.gdx.Gdx")
            local width = Gdx.graphics:getWidth()
            local height = Gdx.graphics:getHeight()
            return {
                type = 0,
                text = {
                    {
                        id = "dimensions",
                        font = 1,
                        size = 16,
                        constantText = tostring(width) .. "x" .. tostring(height)
                    }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "1920x1080");
}

#[test]
fn lua_skin_luajava_input_stubs_are_neutral_during_load() {
    let root = unique_test_dir("bmz-skin-lua-luajava-input");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            local luajava = require("luajava")
            local Gdx = luajava.bindClass("com.badlogic.gdx.Gdx")
            local Controllers = luajava.bindClass("com.badlogic.gdx.controllers.Controllers")
            local Expand_op = 2
            local function input_handler()
                local controllers = Controllers:getControllers()
                if Gdx.input:isKeyPressed(1)
                    or controllers.size > 0
                    or controllers:first() ~= nil
                then
                    Expand_op = 1
                end
            end
            input_handler()
            return {
                type = 7,
                text = {{ id = "panel", font = 1, size = 16, constantText = tostring(Expand_op) }}
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["text"][0]["constantText"], "2");
}

#[test]
fn lua_skin_non_finite_numbers_warn_and_convert_to_zero() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play7.luaskin"),
        r#"
            return {
                type = 0,
                destination = {
                    { id = -110, dst = {{ x = 0 / 0, y = 1 / 0, w = 1, h = 1 }} }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play7.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.value["destination"][0]["dst"][0]["x"], 0);
    assert_eq!(loaded.value["destination"][0]["dst"][0]["y"], 0);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|warning| warning.message.contains("non-finite lua number converted to 0"))
    );
}

#[test]
fn lua_skin_m_select_result_graph_heights_become_runtime_exprs() {
    let root = unique_test_dir("bmz-skin-lua-m-select-result");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            local destinations = {}
            for i = 1, 39 do
                destinations[i] = { id = "dummy", dst = {{ x = 0, y = 0, w = 1, h = 1 }} }
            end
            for i = 40, 51 do
                destinations[i] = {
                    id = "graph",
                    dst = {
                        { time = 0, x = 0, y = 0, w = 1, h = 0 },
                        { time = 500, h = 0 },
                        { time = 1000, h = 0 / 0 },
                    },
                }
            end
            return { type = 7, destination = destinations }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(
        loaded
            .warnings
            .iter()
            .all(|warning| !warning.message.contains("non-finite lua number converted to 0"))
    );
    assert_eq!(
        loaded.value["destination"][39]["dst"][2]["h_expr"],
        "bmz:fast_slow_breakdown_height(422)"
    );
    assert_eq!(
        loaded.value["destination"][50]["dst"][2]["h_expr"],
        "bmz:fast_slow_breakdown_height(421)"
    );
}

#[test]
fn lua_skin_value_functions_fall_back_to_load_time_constants() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("select.luaskin"),
            r#"
            return {
                type = 0,
                value = {
                    { id = "num", src = 1, x = 0, y = 0, w = 10, h = 10, value = function() return 42 end }
                },
                graph = {
                    { id = "graph", src = 1, x = 0, y = 0, w = 10, h = 10, value = function() return 0.25 end }
                },
                text = {
                    { id = "text", font = 1, size = 16, value = function() return "ready" end }
                }
            }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(loaded.warnings.is_empty());
    assert_eq!(loaded.value["value"][0]["value_expr"], "42");
    assert_eq!(loaded.value["graph"][0]["value_expr"], "0.25");
    assert_eq!(loaded.value["text"][0]["constantText"], "ready");
}

#[test]
fn lua_skin_volume_value_functions_map_to_number_refs() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 0,
                value = {
                    { id = "master", src = 1, x = 0, y = 0, w = 110, h = 10, divx = 11, digit = 3, value = function() return main_state.volume_sys() * 100 end },
                    { id = "key", src = 1, x = 0, y = 0, w = 110, h = 10, divx = 11, digit = 3, value = function() return main_state.volume_key() * 100 end },
                    { id = "bgm", src = 1, x = 0, y = 0, w = 110, h = 10, divx = 11, digit = 3, value = function() return main_state.volume_bg() * 100 end },
                }
            }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(loaded.warnings.is_empty());
    assert_eq!(loaded.value["value"][0]["ref"], 57);
    assert_eq!(loaded.value["value"][1]["ref"], 58);
    assert_eq!(loaded.value["value"][2]["ref"], 59);
}

#[test]
fn lua_skin_main_state_version_text_is_available_during_load() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
        r#"
            local main_state = require("main_state")
            local version = main_state.text(1010)
            version = string.sub(version, (string.find(version, " ") + 1))
            return {
                type = 0,
                text = {
                    { id = "version", constantText = version },
                },
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(loaded.warnings.is_empty());
    assert_eq!(loaded.value["text"][0]["constantText"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn lua_skin_main_state_player_name_is_available_during_load() {
    let root = unique_test_dir("bmz-skin-lua-player-name");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
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
    let runtime_state = LuaLoadRuntimeState {
        text_values: BTreeMap::from([(2, "Player One".to_string())]),
        ..LuaLoadRuntimeState::default()
    };

    let loaded = load_lua_skin_with_runtime_state(
        &root.join("select.luaskin"),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &runtime_state,
    )
    .unwrap();

    assert_eq!(loaded.document.text[0].constant_text, "Player One");
    assert_eq!(loaded.dependencies.text_values.get(&2).map(String::as_str), Some("Player One"));
}

#[test]
fn lua_skin_main_state_current_date_numbers_are_available_during_load() {
    let root = unique_test_dir("bmz-skin-lua-date");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("select.luaskin"),
            r#"
            local main_state = require("main_state")
            return {
                type = 5,
                text = {
                    { id = "date", constantText = main_state.number(21) .. "/" .. main_state.number(22) .. "/" .. main_state.number(23) },
                },
            }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
    let date = loaded.value["text"][0]["constantText"].as_str().unwrap_or_default();
    let current_year = unix_epoch_year_for_test();

    assert!(loaded.warnings.is_empty());
    assert!(date.starts_with(&format!("{current_year}/")), "unexpected date: {date}");
}

#[test]
fn lua_skin_nil_integer_keys_do_not_warn_as_mixed_table() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
        r#"
            local skin = { type = 0, image = {} }
            skin[1] = nil
            return skin
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(
        loaded.warnings.iter().all(|warning| !warning.message.contains("mixed lua table")),
        "warnings: {:?}",
        loaded.warnings
    );
}

#[test]
fn lua_skin_header_pass_mixed_table_warning_is_suppressed() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
        r#"
            if skin_config then
                return { type = 0, image = {} }
            end
            return {
                type = 0,
                { image = {} },
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(
        loaded.warnings.iter().all(|warning| !warning.message.contains("mixed lua table")),
        "warnings: {:?}",
        loaded.warnings
    );
}

#[test]
fn lua_skin_preserves_constant_act_and_skips_loader_callback_fields() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
            root.join("select.luaskin"),
            r#"
            return {
                type = 0,
                image = {
                    { id = "button", src = "src", x = 0, y = 0, w = 10, h = 10, act = 15 },
                    { id = "sort", src = "src", x = 0, y = 0, w = 10, h = 10, act = function() return 12 end },
                    { id = "callback", src = "src", x = 0, y = 0, w = 10, h = 10, act = function() return true end }
                },
                customTimers = {
                    { id = 9001, timer = function() return 0 end }
                }
            }
            "#,
        )
        .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(loaded.warnings.len(), 1);
    assert_eq!(
        loaded.warnings[0].message,
        "skipping unsupported custom timer function id 9001 at $.customTimers[1].timer"
    );
    assert_eq!(loaded.value["image"][0]["act"], serde_json::json!(15));
    assert_eq!(loaded.value["image"][1]["act"], serde_json::json!(12));
    assert!(loaded.value["image"][2].get("act").is_none());
    assert!(loaded.value["customTimers"][0].get("timer").is_none());
}

#[test]
fn lua_skin_does_not_execute_mutating_act_during_conversion() {
    let root = unique_test_dir("bmz-skin-lua-mutating-act");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            Panel = 2
            return {
                type = 7,
                image = {
                    {
                        id = "switch",
                        src = "src",
                        x = 0, y = 0, w = 10, h = 10,
                        act = function() Panel = 1 end,
                    },
                    { id = "graph", src = "src", x = 0, y = 0, w = 10, h = 10 },
                    { id = "ir", src = "src", x = 0, y = 0, w = 10, h = 10 },
                },
                destination = {
                    {
                        id = "graph",
                        draw = function() return Panel == 2 end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                    {
                        id = "ir",
                        draw = function() return Panel == 1 end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                },
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert!(loaded.value["image"][0].get("act").is_none());
    let mut loaded = load_lua_skin(
        &root.join("result.luaskin"),
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    assert!(runtime.evaluate_draw(0, &TestLuaMainState::default()));
    assert!(!runtime.evaluate_draw(1, &TestLuaMainState::default()));
}

#[test]
fn lua_skin_maps_result_panel_act_without_mutating_default() {
    let root = unique_test_dir("bmz-skin-lua-result-panel-act");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("result.luaskin"),
        r#"
            Expand_op = 2
            return {
                type = 7,
                image = {
                    {
                        id = "BtnGraphData", src = "src", x = 0, y = 0, w = 10, h = 10,
                        act = function() Expand_op = 2 end,
                    },
                    {
                        id = "BtnIrData", src = "src", x = 0, y = 0, w = 10, h = 10,
                        act = function() Expand_op = 1 end,
                    },
                },
                destination = {
                    {
                        id = "BtnGraphData",
                        draw = function() return Expand_op == 1 end,
                        dst = {{ x = 0, y = 0, w = 10, h = 10 }},
                    },
                    {
                        id = "BtnIrData",
                        draw = function() return Expand_op == 2 end,
                        dst = {{ x = 10, y = 0, w = 10, h = 10 }},
                    },
                },
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("result.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(
        loaded.value["image"][0]["act"],
        serde_json::json!(bmz_skin_document::SKIN_EVENT_RESULT_PANEL_GRAPH)
    );
    assert_eq!(
        loaded.value["image"][1]["act"],
        serde_json::json!(bmz_skin_document::SKIN_EVENT_RESULT_PANEL_IR)
    );
    assert_eq!(loaded.value["resultPanelDefault"], serde_json::json!(2));
    assert_eq!(loaded.value["destination"][0]["draw"], "result_panel(1)");
    assert_eq!(loaded.value["destination"][1]["draw"], "result_panel(2)");
}

#[test]
fn lua_skin_infers_fixed_delay_custom_timer() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("play.luaskin"),
        r#"
            local main_state = require("main_state")
            return {
                type = 0,
                customTimers = {
                    { id = 11900, timer = function()
                        local off = main_state.timer_off_value
                        local source = main_state.timer(143)
                        if source == off then return off end
                        local start = source + 1000000
                        if main_state.time() < start then return off end
                        return start
                    end },
                    { id = 11901, timer = function() return main_state.timer(150) end },
                    { id = 11902, timer = function() return main_state.timer(150) + 1 end }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("play.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(
        loaded.value["fixedDelayTimer"],
        serde_json::json!([
            { "id": 11900, "sourceTimer": 143, "delayMs": 1000 },
            { "id": 11901, "sourceTimer": 150, "delayMs": 0 }
        ])
    );
    assert!(loaded.value["customTimers"][0].get("timer").is_none());
    assert!(loaded.value["customTimers"][1].get("timer").is_none());
    assert!(loaded.value["customTimers"][2].get("timer").is_none());
    assert!(loaded.warnings.iter().any(|warning| {
        warning.message
            == "skipping unsupported custom timer function id 11902 at $.customTimers[3].timer"
    }));
}

#[test]
fn lua_skin_warns_when_timer_observe_callback_needs_runtime_lua() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("select.luaskin"),
        r#"
            local timer_util = require("timer_util")
            local menu_open = false
            local menu_timer = timer_util.timer_observe_boolean(function()
                return menu_open
            end)
            return {
                type = 0,
                destination = {
                    { id = "menu", dst = { { timer = menu_timer } } }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin_value(&root.join("select.luaskin"), &BTreeMap::new(), &BTreeMap::new())
            .unwrap();

    assert_eq!(
        loaded.value["dynamicTimer"],
        serde_json::json!([{ "id": 9000, "observe": "number(0) < 0" }])
    );
    assert!(loaded.warnings.iter().any(|warning| {
            warning.message
                == "timer_util.timer_observe_boolean callback for generated timer 9000 could not be inferred; timer remains inactive"
        }));
}
