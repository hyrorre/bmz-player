use super::*;

#[test]
fn battle_skin_headers_receive_standard_play_offsets() {
    for skin_type in [12, 13] {
        let offsets =
            skin_offset_definitions_from_header(&serde_json::json!({ "type": skin_type }));

        assert!(offsets.iter().any(|(_, id)| *id == 10));
        assert!(offsets.iter().any(|(_, id)| *id == 30));
        assert!(offsets.iter().any(|(_, id)| *id == 32));
        assert!(offsets.iter().any(|(_, id)| *id == 33));
    }
}

#[test]
fn infers_select_score_availability_from_luxe_global_guard() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let draw = lua
        .load("flag_score = false; return function() return flag_score end")
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_select_score_available_draw_condition(&lua, &draw, &probe).as_deref(),
        Some("select_score_available()")
    );
    assert!(!lua.globals().get::<bool>("flag_score").unwrap());
}

#[test]
fn infers_select_score_availability_from_mz_select_local_guard() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let draw = lua
        .load("local flag_score = false; return function() return flag_score end")
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_select_score_available_draw_condition(&lua, &draw, &probe).as_deref(),
        Some("select_score_available()")
    );
    assert!(!draw.call::<bool>(()).unwrap());
}

#[test]
fn load_constant_number_fallback_rejects_runtime_state() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let draw = lua
        .load(
            r#"return function()
                    local ex = main_state.number(71)
                    local max = main_state.number(74) * 2
                    if max == 0 then return false end
                    local rate = ex / max
                    return rate >= 2 / 9 and rate < 3 / 9
                end"#,
        )
        .eval::<Function>()
        .unwrap();
    let value = lua
        .load(
            r#"return function()
                    local ex = main_state.number(71)
                    local max = main_state.number(74) * 2
                    if max == 0 then return 0 end
                    return math.abs(ex - math.ceil(max * 8 / 9))
                end"#,
        )
        .eval::<Function>()
        .unwrap();
    let timer_value =
        lua.load("return function() return main_state.time() end").eval::<Function>().unwrap();
    let constant = lua.load("return function() return 42 end").eval::<Function>().unwrap();

    assert!(infer_constant_draw_at_load(&lua, &draw, &probe).is_none());
    assert!(infer_constant_number_at_load(&lua, &value, &probe).is_none());
    assert!(infer_constant_number_at_load(&lua, &timer_value, &probe).is_none());
    assert_eq!(infer_constant_number_at_load(&lua, &constant, &probe).as_deref(), Some("42"));
}

#[test]
fn infers_wmii_result_score_runtime_expressions() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let functions = lua
        .load(
            r#"
                local ranks = {
                    {name="F", value=0/9}, {name="E", value=2/9},
                    {name="D", value=3/9}, {name="C", value=4/9},
                    {name="B", value=5/9}, {name="A", value=6/9},
                    {name="AA", value=7/9}, {name="AAA", value=8/9},
                    {name="MAX", value=1},
                }
                local function info()
                    local ex = main_state.number(71)
                    local max = main_state.number(74) * 2
                    if max == 0 then return nil end
                    if ex >= max then return {target="MAX", sign="+", diff=0} end
                    local current = 1
                    for i = 1, #ranks do
                        if ex / max >= ranks[i].value then current = i else break end
                    end
                    local cur, next = ranks[current], ranks[current + 1]
                    local lower = math.ceil(cur.value * max)
                    local upper = math.ceil(next.value * max)
                    local to_lower = math.max(0, ex - lower)
                    local to_upper = math.max(0, upper - ex)
                    if to_lower <= to_upper then
                        return {target=cur.name, sign="+", diff=to_lower}
                    end
                    return {target=next.name, sign="-", diff=to_upper}
                end
                return {
                    band = function()
                        local ex = main_state.number(71)
                        local max = main_state.number(74) * 2
                        if max == 0 then return false end
                        return ex / max >= 2/9 and ex / max < 3/9
                    end,
                    max = function()
                        local ex = main_state.number(71)
                        local max = main_state.number(74) * 2
                        if max == 0 then return false end
                        return ex / max == 1
                    end,
                    diff = function() local i=info(); return i and i.diff or 0 end,
                    luxe_diff = function()
                        local ex = main_state.number(71)
                        local max = main_state.number(74) * 2
                        local _best = main_state.number(170)
                        local _rival = main_state.number(271)
                        if max <= 0 or ex >= max then return 0 end
                        local boundaries = {0, 2, 3, 4, 5, 6, 7, 8, 9}
                        local current = 1
                        for i = 1, #boundaries do
                            if ex * 9 >= boundaries[i] * max then current = i else break end
                        end
                        local lower, upper = boundaries[current], boundaries[current + 1]
                        local lower_score = math.ceil(lower * max / 9)
                        local upper_score = math.ceil(upper * max / 9)
                        if ex * 18 < (lower + upper) * max then
                            return math.max(0, ex - lower_score)
                        end
                        return math.max(0, upper_score - ex)
                    end,
                    aaa_minus = function()
                        local i=info(); return i and i.target == "AAA" and i.sign == "-"
                    end,
                    plus = function() local i=info(); return i and i.sign == "+" end,
                    text = function() return main_state.text(1001).." "..main_state.text(1002) end,
                }
                "#,
        )
        .eval::<Table>()
        .unwrap();

    assert_eq!(
        infer_score_rate_band(&functions.get::<Function>("band").unwrap(), &probe).as_deref(),
        Some("score_rate_band(2,3)")
    );
    assert_eq!(
        infer_score_rate_band(&functions.get::<Function>("max").unwrap(), &probe).as_deref(),
        Some("score_rate_band(9,10)")
    );
    assert_eq!(
        infer_nearest_rank_diff_value_expr(
            &functions.get::<Function>("diff").unwrap(),
            Some("diff_rank"),
            &probe,
        )
        .as_deref(),
        Some("bmz:nearest_rank_diff_abs")
    );
    assert_eq!(
        infer_nearest_rank_diff_value_expr(
            &functions.get::<Function>("luxe_diff").unwrap(),
            Some("rank_diff_count"),
            &probe,
        )
        .as_deref(),
        Some("bmz:nearest_rank_diff_abs")
    );
    assert_eq!(
        infer_nearest_rank_diff_value_expr(
            &functions.get::<Function>("luxe_diff").unwrap(),
            Some("default_playerdata_diff_count"),
            &probe,
        )
        .as_deref(),
        Some("bmz:nearest_rank_diff_abs")
    );
    assert_eq!(
        infer_result_score_draw(
            &functions.get::<Function>("aaa_minus").unwrap(),
            Some("nextRankAAA"),
            &probe,
        )
        .as_deref(),
        Some("nearest_rank(AAA,minus)")
    );
    assert_eq!(
        infer_result_score_draw(
            &functions.get::<Function>("plus").unwrap(),
            Some("diff_plus"),
            &probe,
        )
        .as_deref(),
        Some("nearest_rank_sign(plus)")
    );
    assert_eq!(
        infer_result_score_draw(
            &functions.get::<Function>("plus").unwrap(),
            Some("rank_diff_aaa_plus"),
            &probe,
        )
        .as_deref(),
        Some("nearest_rank(AAA,plus)")
    );
    assert_eq!(
        infer_text_concat_expr(&functions.get::<Function>("text").unwrap(), &probe).as_deref(),
        Some("bmz:text_concat:1001:1002")
    );
}

#[test]
fn infers_luxe_flat_select_rank_and_diff_layout_from_score_rate_state() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let functions = lua
        .load(
            r#"
                flag_score = false
                local rank_plus = false
                local scorerate = 0
                local rivalscore = 0
                local rival_scorerate = 0
                return {
                    max_minus = function()
                        return rank_plus and flag_score
                            and scorerate < 18/18 and scorerate >= 17/18
                    end,
                    f_plus = function()
                        return rank_plus and flag_score
                            and scorerate < 2/18 and scorerate >= 0/18
                    end,
                    wide = function()
                        return rank_plus and flag_score and scorerate >= 15/18
                    end,
                    rival = function()
                        return rank_plus and rivalscore > 0
                            and rival_scorerate < 18/18 and rival_scorerate >= 17/18
                    end,
                }
            "#,
        )
        .eval::<Table>()
        .unwrap();

    assert_eq!(
        infer_luxe_flat_select_score_draw(
            &lua,
            &functions.get::<Function>("max_minus").unwrap(),
            Some("diff_rank_max"),
            &probe,
        )
        .as_deref(),
        Some("select_score_available() and nearest_rank(MAX,minus)")
    );
    assert_eq!(
        infer_luxe_flat_select_score_draw(
            &lua,
            &functions.get::<Function>("f_plus").unwrap(),
            Some("diff_rank_f_plus"),
            &probe,
        )
        .as_deref(),
        Some("select_score_available() and nearest_rank(F,plus)")
    );
    assert_eq!(
        infer_luxe_flat_select_score_draw(
            &lua,
            &functions.get::<Function>("wide").unwrap(),
            Some("default_playerdata_diff_count"),
            &probe,
        )
        .as_deref(),
        Some("select_score_available() and nearest_rank_label_width(3)")
    );
    assert_eq!(
        infer_luxe_flat_select_score_draw(
            &lua,
            &functions.get::<Function>("rival").unwrap(),
            Some("diff_rank_max"),
            &probe,
        ),
        None
    );
    assert!(!lua.globals().get::<bool>("flag_score").unwrap());
}

#[test]
fn infers_luxe_flat_total_notes_ratio_values() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let ratio = lua
        .load(
            r#"
                return function()
                    local total = main_state.number(368)
                    local notes = main_state.number(74)
                    if notes > 0 then return total / notes end
                    return 0
                end
            "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_bmz_builtin_value_expr(&ratio, Some("tn_count"), &probe).as_deref(),
        Some(SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER)
    );
    assert_eq!(
        infer_bmz_builtin_value_expr(&ratio, Some("tn_dot_count"), &probe).as_deref(),
        Some(SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION)
    );
}

#[test]
fn infers_wmii_result_ir_ranking_runtime_expressions() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    lua.globals().set("Expand_op", 1).unwrap();
    let functions = lua
        .load(
            r#"
                return {
                    graph = function()
                        return main_state.number(382) / (main_state.number(74) * 2)
                    end,
                    rate_integer = function()
                        local score = main_state.number(382)
                        local max = main_state.number(74) * 2
                        if score > 0 and max > 0 then return math.floor(score / max * 100) end
                        return 0
                    end,
                    rate_fraction = function()
                        local score = main_state.number(382)
                        local max = main_state.number(74) * 2
                        if score > 0 and max > 0 then return (score / max * 10000) % 100 end
                        return 0
                    end,
                    diff = function()
                        return math.max(main_state.number(170), main_state.number(171))
                            - main_state.number(382)
                    end,
                    band = function()
                        local rate = main_state.number(382) / (main_state.number(74) * 2)
                        return rate >= 7/9 and rate < 8/9 and Expand_op == 1
                    end,
                    name = function()
                        local current = main_state.text(122)
                        local own = main_state.text(1021)
                        if current == own then return own end
                        return main_state.text(122)
                    end,
                    own = function()
                        return main_state.text(122) == main_state.text(1021) and Expand_op == 1
                    end,
                }
                "#,
        )
        .eval::<Table>()
        .unwrap();

    assert_eq!(
        infer_ir_ranking_score_value_expr(
            &functions.get::<Function>("graph").unwrap(),
            Some("ir_scoreGraph3"),
            &probe,
        )
        .as_deref(),
        Some("bmz:ir_score_rate:3")
    );
    assert_eq!(
        infer_ir_ranking_score_rate_value_expr(
            &functions.get::<Function>("rate_integer").unwrap(),
            Some("ir_scorerate3"),
            &probe,
        )
        .as_deref(),
        Some("bmz:ir_score_rate_integer:3")
    );
    assert_eq!(
        infer_ir_ranking_score_rate_value_expr(
            &functions.get::<Function>("rate_fraction").unwrap(),
            Some("ir_scorerate_dot3"),
            &probe,
        )
        .as_deref(),
        Some("bmz:ir_score_rate_fraction:3")
    );
    assert_eq!(
        infer_ir_ranking_score_diff_value_expr(
            &functions.get::<Function>("diff").unwrap(),
            Some("ir_diff_score3"),
            &probe,
        )
        .as_deref(),
        Some("bmz:ir_score_diff:3")
    );
    assert_eq!(
        infer_result_score_draw(
            &functions.get::<Function>("band").unwrap(),
            Some("ir_scoreGraph3"),
            &probe,
        )
        .as_deref(),
        Some("ir_score_rate_band(3,7,8)")
    );
    assert_eq!(
        infer_ir_ranking_name_ref(
            &functions.get::<Function>("name").unwrap(),
            Some("ir_username3"),
            &probe,
        ),
        Some(122)
    );
    assert_eq!(
        infer_result_score_draw(
            &functions.get::<Function>("own").unwrap(),
            Some("irYouFrame"),
            &probe,
        )
        .as_deref(),
        Some("ir_ranking_user(3)")
    );
}

#[test]
fn infers_modern_chic_select_graph_runtime_expressions() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let functions = lua
        .load(
            r#"
                return {
                    fast = function()
                        local slow = main_state.number(424)
                        local fast = main_state.number(423)
                        return fast / (slow + fast)
                    end,
                    slow = function()
                        local slow = main_state.number(424)
                        local fast = main_state.number(423)
                        return slow / (slow + fast)
                    end,
                    graph = function()
                        local score = main_state.number(380)
                        if score == -2147483648 then return 0 end
                        return score / (main_state.number(74) * 2)
                    end,
                    band = function()
                        local score = main_state.number(380)
                        local rate = (score / (main_state.number(74) * 2)) * 100
                        return main_state.option(51) and rate <= 88.8 and rate > 77.7
                    end,
                }
                "#,
        )
        .eval::<Table>()
        .unwrap();

    assert_eq!(
        infer_value_float_expr(&functions.get::<Function>("fast").unwrap(), &probe).as_deref(),
        Some("(number(423))/(number(423)+number(424))")
    );
    assert_eq!(
        infer_value_float_expr(&functions.get::<Function>("slow").unwrap(), &probe).as_deref(),
        Some("(number(424))/(number(423)+number(424))")
    );
    assert_eq!(
        infer_ir_ranking_score_value_expr(
            &functions.get::<Function>("graph").unwrap(),
            Some("s_rankingGraphAA1"),
            &probe,
        )
        .as_deref(),
        Some("bmz:ir_score_rate:1")
    );
    assert_eq!(
        infer_result_score_draw(
            &functions.get::<Function>("band").unwrap(),
            Some("s_rankingGraphAA1"),
            &probe,
        )
        .as_deref(),
        Some("option(51) and ir_score_rate_range(1,777,888)")
    );
    assert_eq!(modern_chic_ir_ranking_graph("s_rankingGraphAAA10"), Some((10, "AAA")));
}

#[test]
fn infers_wmii_result_panel_gates_without_mutating_default() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    lua.globals().set("Expand_op", 2).unwrap();
    let functions = lua
        .load(
            r#"
                return {
                    ir = function() return Expand_op == 1 end,
                    not_ir = function() return Expand_op ~= 1 end,
                    band = function()
                        local rate = main_state.number(382) / (main_state.number(74) * 2)
                        return rate >= 7/9 and rate < 8/9 and Expand_op == 1
                    end,
                    own = function()
                        return main_state.text(122) == main_state.text(1021) and Expand_op == 1
                    end,
                    timing_negative = function()
                        return (main_state.number(374) + main_state.number(375) * 0.01) < 0
                            and Expand_op == 2
                    end,
                    timing_non_negative = function()
                        return (main_state.number(374) + main_state.number(375) * 0.01) >= 0
                            and Expand_op == 2
                    end,
                }
                "#,
        )
        .eval::<Table>()
        .unwrap();

    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("ir").unwrap(),
            None,
            &probe,
        )
        .as_deref(),
        Some("result_panel(1)")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("not_ir").unwrap(),
            None,
            &probe,
        )
        .as_deref(),
        Some("result_panel(0) or result_panel(2)")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("band").unwrap(),
            Some("ir_scoreGraph3"),
            &probe,
        )
        .as_deref(),
        Some("result_panel(1) and ir_score_rate_band(3,7,8)")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("own").unwrap(),
            Some("irYouFrame"),
            &probe,
        )
        .as_deref(),
        Some("result_panel(1) and ir_ranking_user(3)")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("timing_negative").unwrap(),
            Some("timingAvg"),
            &probe,
        )
        .as_deref(),
        Some("result_panel(2) and number(374) < 0 or result_panel(2) and number(375) < 0")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("timing_non_negative").unwrap(),
            Some("timingAvg"),
            &probe,
        )
        .as_deref(),
        Some("result_panel(2) and number(374) >= 0 and number(375) >= 0")
    );
    assert_eq!(lua.globals().get::<i32>("Expand_op").unwrap(), 2);
}

#[test]
fn infers_luxe_flat_local_result_panel_state_without_mutating_default() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let functions = lua
        .load(
            r#"
                local result_mode = 0
                return {
                    graph_act = function() result_mode = 0 end,
                    ir_act = function() result_mode = 1 end,
                    graph = function() return result_mode == 0 end,
                    ir = function() return result_mode == 1 end,
                    graph_score = function()
                        return result_mode == 0 and main_state.number(71) >= 0
                    end,
                }
                "#,
        )
        .eval::<Table>()
        .unwrap();

    assert_eq!(
        infer_result_panel_act_at_load(
            &lua,
            &functions.get::<Function>("graph_act").unwrap(),
            &probe,
        ),
        Some(i64::from(SKIN_EVENT_RESULT_PANEL_GRAPH))
    );
    assert_eq!(
        infer_result_panel_act_at_load(&lua, &functions.get::<Function>("ir_act").unwrap(), &probe,),
        Some(i64::from(SKIN_EVENT_RESULT_PANEL_IR))
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("graph").unwrap(),
            None,
            &probe,
        )
        .as_deref(),
        Some("result_panel(2)")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("ir").unwrap(),
            None,
            &probe,
        )
        .as_deref(),
        Some("result_panel(1)")
    );
    assert_eq!(
        infer_result_panel_draw_condition(
            &lua,
            &functions.get::<Function>("graph_score").unwrap(),
            None,
            &probe,
        )
        .as_deref(),
        Some("result_panel(2) and number(71) >= 0")
    );
    assert_eq!(probe.lock().unwrap().result_panel_default, Some(2));
    assert_eq!(
        lua_result_mode_upvalue(&lua, &functions.get::<Function>("graph").unwrap())
            .map(|(_, mode)| mode),
        Some(0)
    );
}

#[test]
fn maps_peacefulplay_keylogger_graph_ids_to_builtin_expressions() {
    assert_eq!(
        keylogger_graph_value_expr_from_id("keylogger-graph-judge-3-good").as_deref(),
        Some("bmz:keylogger_graph:judge:3:good")
    );
    assert_eq!(
        keylogger_graph_value_expr_from_id("keylogger-graph-fastslow-9-fast").as_deref(),
        Some("bmz:keylogger_graph:fastslow:9:fast")
    );
    assert!(keylogger_graph_value_expr_from_id("graph-now").is_none());
}

#[test]
fn maps_peacefulplay_chattering_destination_ids_to_lanes() {
    assert_eq!(parse_keylogger_chattering_destination_id("keylogger-chattering-alert-1"), Some(1));
    assert_eq!(parse_keylogger_chattering_destination_id("keylogger-chattering-alert-9"), Some(9));
    assert!(parse_keylogger_chattering_destination_id("keylogger-chattering-alert-0").is_none());
    assert!(parse_keylogger_chattering_destination_id("keylogger-chattering-alert-10").is_none());
    assert!(parse_keylogger_chattering_destination_id("keylogger-note-1").is_none());
}

#[test]
fn maps_milliondollar_fast_slow_graph_ids_to_runtime_expressions() {
    assert_eq!(
        milliondollar_fast_slow_graph_value_expr_from_id("Graph_Totalfastslow_Fast").as_deref(),
        Some(
            "(option(928)*number(423)+(1-option(928))*(number(423)+number(410)))/(number(110)+number(111)+number(112)+number(113)+number(114)+number(420))"
        )
    );
    assert_eq!(
        milliondollar_fast_slow_graph_value_expr_from_id("Graph_Totalfastslow_Slow").as_deref(),
        Some(
            "(option(928)*number(424)+(1-option(928))*(number(424)+number(411)))/(number(110)+number(111)+number(112)+number(113)+number(114)+number(420))"
        )
    );
    assert!(milliondollar_fast_slow_graph_value_expr_from_id("graph-now").is_none());
}

#[test]
fn milliondollar_result_fast_slow_graphs_convert_when_available() {
    let skin_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/MILLIONDOLLAR/result.luaskin");
    if !skin_path.is_file() {
        return;
    }

    let loaded = load_lua_skin_value(
        &skin_path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        &BTreeMap::new(),
    )
    .expect("MILLIONDOLLAR result should convert");
    let messages: Vec<_> = loaded.warnings.iter().map(|warning| warning.message.as_str()).collect();
    assert!(
        !messages.iter().any(|message| {
            message.contains("Graph_Totalfastslow_Fast")
                || message.contains("Graph_Totalfastslow_Slow")
                || (message.contains("graph[") && message.contains("unsupported value"))
        }),
        "MILLIONDOLLAR fast/slow graph values should convert: {messages:?}"
    );
    let document = loaded.value.to_string();
    assert!(document.contains("Graph_Totalfastslow_Fast"));
    assert!(document.contains("option(928)*number(423)"));
    assert!(document.contains("Graph_Totalfastslow_Slow"));
    assert!(document.contains("option(928)*number(424)"));
}

#[test]
fn infers_fixed_delay_timer_function() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let function = lua
        .load(
            r#"return function()
                    local off = main_state.timer_off_value
                    local source = main_state.timer(143)
                    if source == off then return off end
                    local start = source + 1000000
                    if main_state.time() < start then return off end
                    return start
                end"#,
        )
        .eval::<Function>()
        .unwrap();
    assert_eq!(infer_fixed_delay_timer(&function, &probe), Some((143, 1000)));
}

#[test]
fn infers_custom_timer_alias_function() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    lua.globals().set("main_state", create_main_state_stub(&lua, probe.clone()).unwrap()).unwrap();
    let function =
        lua.load("return function() return main_state.timer(150) end").eval::<Function>().unwrap();

    assert_eq!(infer_custom_timer_alias(&function, &probe), Some(150));
}

#[test]
fn infers_event_index_or_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
        .load(
            r#"
                return function()
                    return main_state.event_index(42) == 2 or main_state.event_index(42) == 3
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_main_state_event_index_draw_condition(&function, &probe),
        Some("event_index(42) == 2 or event_index(42) == 3".to_string())
    );
}

#[test]
fn infers_extended_arrange_event_index_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
        .load(
            r#"
                return function()
                    return main_state.event_index(344) == 10
                        or main_state.event_index(344) == 11
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_main_state_event_index_draw_condition(&function, &probe),
        Some("event_index(344) == 10 or event_index(344) == 11".to_string())
    );
}
