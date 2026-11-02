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
fn lua_skin_preserves_text_gradient() {
    let root = unique_test_dir("bmz-skin-text-gradient");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.luaskin");
    fs::write(
        &path,
        r#"
            return {
                type = 0,
                text = {
                    { id = "title", size = 24, colorTop = "ffcc00", colorBottom = "ff3300ff", gradientAngle = 15.5 }
                }
            }
            "#,
    )
    .unwrap();

    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();

    assert_eq!(loaded.document.text[0].color_top, "ffcc00");
    assert_eq!(loaded.document.text[0].color_bottom, "ff3300ff");
    assert_eq!(loaded.document.text[0].gradient_angle, 15.5);
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
                    { id = "local", constantText = os.date("*t").year .. "/" .. os.date("*t").month .. "/" .. os.date("*t").day },
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
    // The local year can differ from the UTC year around New Year.
    let year = date.split('/').next().and_then(|year| year.parse::<i32>().ok());
    assert!(
        year.is_some_and(|year| (current_year - 1..=current_year + 1).contains(&year)),
        "unexpected date: {date}"
    );
    // beatoraja reads the local Calendar, like `os.date` without `!`.
    assert_eq!(date, loaded.value["text"][1]["constantText"].as_str().unwrap_or_default());
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
    // Other act functions run in the runtime VM when clicked.
    let act = loaded.value["image"][2]["act"].as_i64().unwrap();
    let callback_id = bmz_skin_document::lua_act_callback_id(act as i32).unwrap();
    assert_eq!(loaded.runtime_callback_paths[callback_id], "$.image[3].act");
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

    let act = loaded.value["image"][0]["act"].as_i64().unwrap();
    assert!(bmz_skin_document::lua_act_callback_id(act as i32).is_some());
    let mut loaded = load_lua_skin(
        &root.join("result.luaskin"),
        SkinKind::Result,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    // Callback IDs follow the randomized `pairs` order of each VM.
    let act = loaded.document.image[0].act.and_then(bmz_skin_document::lua_act_callback_id);
    let act = act.unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let draw = |path: &str| {
        (0..runtime.callback_count()).find(|&id| runtime.callback_path(id) == Some(path)).unwrap()
    };
    let (graph, ir) = (draw("$.destination[1].draw"), draw("$.destination[2].draw"));
    let state = TestLuaMainState::default();
    assert!(runtime.evaluate_draw(graph, &state));
    assert!(!runtime.evaluate_draw(ir, &state));
    // The click runs the act in the runtime VM.
    assert!(runtime.execute_act(act, 0, 0, &state));
    assert!(!runtime.evaluate_draw(graph, &state));
    assert!(runtime.evaluate_draw(ir, &state));
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
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert!(loaded.lua_runtime.is_some());
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

/// ModernChic's Result builds its menus with `timer_observe_boolean` before it
/// checks `main_state.option(51)` (online) for the IR menu. Inferring the
/// predicate must not leave probe values in the load state.
#[test]
fn observe_timer_inference_keeps_the_load_time_state_for_the_rest_of_main() {
    let root = unique_test_dir("bmz-skin-lua");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("result.luaskin");
    fs::write(
        &path,
        r#"
        if skin_config == nil then return {type = 7} end
        local s = require("main_state")
        local timer_util = require("timer_util")
        local open = false
        local menu = timer_util.timer_observe_boolean(function() return open end)
        local graph = timer_util.timer_observe_boolean(function() return s.option(2) and open end)
        return {type = 7, name = tostring(s.option(51)) .. "," .. s.number(71) .. "," .. s.text(2),
            destination = {
                {id = "menu", timer = menu, dst = {{x = 0, y = 0, w = 1, h = 1}}},
                {id = "graph", timer = graph, dst = {{x = 0, y = 0, w = 1, h = 1}}},
            }}
        "#,
    )
    .unwrap();
    let state = LuaLoadRuntimeState {
        option_values: BTreeMap::from([(2, true), (50, false), (51, true)]),
        number_values: BTreeMap::from([(71, 1234)]),
        text_values: BTreeMap::from([(2, "Player".to_string())]),
        ..Default::default()
    };
    let loaded =
        load_lua_skin_with_runtime_state(&path, &BTreeMap::new(), &BTreeMap::new(), &state)
            .unwrap();
    assert_eq!(loaded.document.name, "true,1234,Player");
    // Probe reads during inference are not load dependencies.
    assert_eq!(loaded.dependencies.option_values.get(&51), Some(&true));
    assert_eq!(loaded.dependencies.number_values.get(&71), Some(&1234));
}

/// ModernChic's side menu opens panels from `act` and shows each part with
/// `timer = timer_observe_boolean(function() return isLampMenuOpen end)`.
#[test]
fn destination_observe_timer_over_lua_state_runs_in_the_runtime_vm() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let root = unique_test_dir("bmz-skin-lua");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("select.luaskin");
        fs::write(
            &path,
            r#"
            local timer_util = require("timer_util")
            local menu_open = false
            local open_timer = timer_util.timer_observe_boolean(function() return menu_open end)
            local image_timer = timer_util.timer_observe_boolean(function() return menu_open end)
            return {
                type = 5,
                image = {
                    { id = "btn", src = 0, x = 0, y = 0, w = 1, h = 1,
                      act = function() menu_open = not menu_open end },
                    { id = "anim", src = 0, x = 0, y = 0, w = 2, h = 1, divx = 2,
                      cycle = 100, timer = image_timer },
                },
                destination = {
                    { id = "btn", dst = { { x = 0, y = 0, w = 1, h = 1 } } },
                    { id = "panel", timer = open_timer, loop = -1,
                      dst = { { x = 0, y = 0, w = 1, h = 1 } } },
                    { id = "closed", timer = timer_util.timer_observe_boolean(function()
                        return not menu_open end), dst = { { x = 0, y = 0, w = 1, h = 1 } } },
                }
            }
            "#,
        )
        .unwrap();
        let state = LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() };
        let mut loaded =
            load_lua_skin_with_runtime_state(&path, &BTreeMap::new(), &BTreeMap::new(), &state)
                .unwrap();
        let destination = |id: &str| {
            loaded
                .document
                .destination
                .iter()
                .find_map(|entry| match entry {
                    bmz_skin_document::DestinationListEntry::Single(destination)
                        if destination.id == id =>
                    {
                        Some(destination.clone())
                    }
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(destination("panel").timer, None);
        assert!(destination("panel").timer_expr.starts_with("bmz:lua_timer_callback:"));
        assert!(destination("closed").timer_expr.starts_with("bmz:lua_timer_callback:"));
        // An image animation timer has no runtime callback and stays inactive.
        let messages =
            loaded.warnings.iter().map(|warning| warning.message.as_str()).collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                "timer_util.timer_observe_boolean callback for generated timer 9001 could not be inferred; timer remains inactive"
            ],
            "{mode:?}"
        );
        let dynamic_timers = loaded
            .document
            .dynamic_timers
            .iter()
            .map(|timer| (timer.id, timer.observe.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(dynamic_timers, [(9001, "number(0) < 0")]);
        assert_eq!(
            loaded.document.image.iter().find(|image| image.id == "anim").unwrap().timer,
            Some(9001)
        );

        let act = bmz_skin_document::lua_act_callback_id(
            loaded.document.image.iter().find(|image| image.id == "btn").unwrap().act.unwrap(),
        )
        .unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let callback = |runtime: &LuaSkinRuntime, path: &str| {
            (0..runtime.callback_count())
                .find(|&id| runtime.callback_path(id) == Some(path))
                .unwrap()
        };
        let panel = callback(runtime, "$.destination[2].timer");
        let closed = callback(runtime, "$.destination[3].timer");
        let mut state = TestLuaMainState { now_us: 1_000_000, ..Default::default() };
        assert_eq!(runtime.evaluate_timer(panel, &state), None);
        assert_eq!(runtime.evaluate_timer(closed, &state), Some(1_000_000));
        state.now_us = 2_000_000;
        assert!(runtime.execute_act(act, 1, 0, &state));
        state.now_us = 3_000_000;
        // The timer starts when the observed state turns on, as in beatoraja.
        assert_eq!(runtime.evaluate_timer(panel, &state), Some(3_000_000));
        assert_eq!(runtime.evaluate_timer(closed, &state), None);
        state.now_us = 4_000_000;
        assert_eq!(runtime.evaluate_timer(panel, &state), Some(3_000_000));
        assert_eq!(runtime.failure_log_count(), 0);
    }
}
