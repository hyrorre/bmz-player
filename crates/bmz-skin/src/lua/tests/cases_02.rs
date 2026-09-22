use super::*;

#[test]
fn infers_event_index_and_dp_side_options_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let random = lua
        .load(
            r#"
                return function()
                    local rnd = main_state.event_index(43)
                    return (rnd == 2 or rnd == 3)
                        and (main_state.option(162) or main_state.option(163))
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();
    let normal = lua
        .load(
            r#"
                return function()
                    return main_state.event_index(43) == 0
                        and (main_state.option(162) or main_state.option(163))
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();
    let extended = lua
        .load(
            r#"
                return function()
                    return main_state.event_index(345) == 11
                        and (main_state.option(162) or main_state.option(163))
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
            infer_boolean_predicate(&lua, &random, &probe, None),
            Some(
                "event_index(43) == 2 and option(162) or event_index(43) == 2 and option(163) or event_index(43) == 3 and option(162) or event_index(43) == 3 and option(163)"
                    .to_string()
            )
        );
    assert_eq!(
        infer_boolean_predicate(&lua, &normal, &probe, None),
        Some(
            "event_index(43) == 0 and option(162) or event_index(43) == 0 and option(163)"
                .to_string()
        )
    );
    assert_eq!(
        infer_boolean_predicate(&lua, &extended, &probe, None),
        Some(
            "event_index(345) == 11 and option(162) or event_index(345) == 11 and option(163)"
                .to_string()
        )
    );
}

#[test]
fn infers_single_number_lane_color_membership_draw_conditions() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let white = lua
        .load(
            r#"
                return function()
                    local value = main_state.number(450)
                    return value == 1 or value == 3 or value == 5 or value == 7
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();
    let blue = lua
        .load(
            r#"
                return function()
                    local value = main_state.number(450)
                    return value == 2 or value == 4 or value == 6
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_boolean_predicate(&lua, &white, &probe, None),
        Some(
            "number(450) == 1 or number(450) == 3 or number(450) == 5 or number(450) == 7"
                .to_string()
        )
    );
    assert_eq!(
        infer_boolean_predicate(&lua, &blue, &probe, None),
        Some("number(450) == 2 or number(450) == 4 or number(450) == 6".to_string())
    );
}

#[test]
fn infers_loading_or_loaded_before_ready_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).expect("main_state probe");
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
        .load(
            r#"
                return function()
                    if main_state.option(80) then
                        return true
                    end
                    if not main_state.option(81) then
                        return false
                    end
                    return main_state.timer(40) == main_state.timer_off_value
                end
                "#,
        )
        .eval::<Function>()
        .expect("draw function");

    assert_eq!(
        infer_main_state_two_options_timer_draw_condition(&function, &probe),
        Some("option(80) or option(81) and timer(40) == timer_off".to_string())
    );
}

#[test]
fn infers_option_and_multiple_positive_numbers_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
        .load(
            r#"
                return function()
                    return main_state.option(2)
                        and (main_state.number(74) > 0
                            or main_state.number(92) > 0
                            or main_state.number(368) > 0)
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_boolean_predicate(&lua, &function, &probe, None),
        Some(
            "option(2) and number(74) > 0 or option(2) and number(92) > 0 or option(2) and number(368) > 0"
                .to_string()
        )
    );
}

#[test]
fn infers_keybeam_hold_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
            .load(
                r#"
                local off = main_state.timer_off_value
                local last_update_time = off
                local last_key_on_timer = {}
                local last_key_off_timer = {}
                local active = {}
                local fade_start_time = {}
                local suppress_until_key_off = {}
                local lanes = {
                    { display_lane = 1, key_on_timer = 101, key_off_timer = 121, hold_timer = 71 },
                    { display_lane = 2, key_on_timer = 102, key_off_timer = 122, hold_timer = 72 },
                }
                local function update()
                    local now = main_state.time()
                    if now == last_update_time then
                        return
                    end
                    last_update_time = now
                    for _, lane_info in ipairs(lanes) do
                        local lane = lane_info.display_lane
                        local key_on_time = main_state.timer(lane_info.key_on_timer)
                        local key_off_time = main_state.timer(lane_info.key_off_timer)
                        local hold_time = main_state.timer(lane_info.hold_timer)
                        local key_on_changed = key_on_time ~= off and key_on_time ~= last_key_on_timer[lane]
                        local key_off_changed = key_off_time ~= off and key_off_time ~= last_key_off_timer[lane]
                        if key_on_changed then
                            active[lane] = true
                            fade_start_time[lane] = nil
                            suppress_until_key_off[lane] = false
                        end
                        if hold_time ~= off and (active[lane] or key_off_changed) then
                            suppress_until_key_off[lane] = true
                            fade_start_time[lane] = nil
                        end
                        if key_off_changed then
                            active[lane] = true
                            fade_start_time[lane] = key_off_time
                        end
                        last_key_on_timer[lane] = key_on_time
                        last_key_off_timer[lane] = key_off_time
                    end
                end
                return function()
                    update()
                    if not active[1] then
                        return false
                    end
                    if suppress_until_key_off[1] then
                        return false
                    end
                    if fade_start_time[1] ~= nil and main_state.time() >= fade_start_time[1] then
                        return false
                    end
                    return main_state.event_index(501) == 2 or main_state.event_index(501) == 3
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

    assert_eq!(
            infer_boolean_predicate(&lua, &function, &probe, None),
            Some(
                "timer(101) != timer_off and timer(71) == timer_off and event_index(501) == 2 or timer(101) != timer_off and timer(71) == timer_off and event_index(501) == 3"
                    .to_string()
            )
        );
}

#[test]
fn infers_end_of_note_shadow_draw_condition() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
        .load(
            r#"
                local TIMER_OFF = main_state.timer_off_value
                local function getRemainNotes()
                    return main_state.number(106)
                        - main_state.number(110)
                        - main_state.number(111)
                        - main_state.number(112)
                        - main_state.number(113)
                        - main_state.number(114)
                end

                return function()
                    if main_state.timer(143) == TIMER_OFF and getRemainNotes() == 0 then
                        return true
                    end
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
            infer_boolean_predicate(&lua, &function, &probe, None),
            Some(
                "timer(143) == timer_off and number(106)-number(110)-number(111)-number(112)-number(113)-number(114) == 0"
                    .to_string()
            )
        );
}

#[test]
fn repairs_keybeam_hold_destination_draws_from_fade_pairs() {
    let mut root = JsonMap::from_iter([(
            "destination".to_string(),
            JsonValue::Array(vec![
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-pgreat".to_string())),
                    ("draw".to_string(), JsonValue::String("number(0) < 0".to_string())),
                ])),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-pgreat".to_string())),
                    ("timer".to_string(), JsonValue::Number(JsonNumber::from(122))),
                    ("loop".to_string(), JsonValue::Number(JsonNumber::from(-1))),
                    ("draw".to_string(), JsonValue::String("event_index(502) == 1".to_string())),
                ])),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-great".to_string())),
                    (
                        "draw".to_string(),
                        JsonValue::String(
                            "timer(103) != timer_off and timer(73) == timer_off and event_index(503) == 2"
                                .to_string(),
                        ),
                    ),
                ])),
                JsonValue::Object(JsonMap::from_iter([
                    ("id".to_string(), JsonValue::String("key-beam-thick-great".to_string())),
                    ("loop".to_string(), JsonValue::Number(JsonNumber::from(-1))),
                    (
                        "draw".to_string(),
                        JsonValue::String(
                            "event_index(503) == 2 or event_index(503) == 3".to_string(),
                        ),
                    ),
                ])),
            ]),
        )]);

    let mut warnings = vec![
        "skipping unsupported draw function at $.destination[3].draw".to_string(),
        "skipping unsupported field `timer` at $.destination[4]".to_string(),
    ];
    postprocess_lua_skin_json(&mut root, &mut warnings);

    let destinations = root.get("destination").and_then(JsonValue::as_array).unwrap();
    let draw = |index: usize| {
        destinations[index]
            .as_object()
            .and_then(|destination| destination.get("draw"))
            .and_then(JsonValue::as_str)
            .unwrap()
    };
    assert_eq!(draw(0), "keybeam_hold(102) != 0 and event_index(502) == 1");
    assert_eq!(
        draw(2),
        "keybeam_hold(103) != 0 and event_index(503) == 2 or keybeam_hold(103) != 0 and event_index(503) == 3"
    );
    assert_eq!(draw(1), "keybeam_fade(122) != 0 and event_index(502) == 1");
    assert_eq!(
        destinations[3].as_object().and_then(|destination| destination.get("timer")),
        Some(&JsonValue::Number(JsonNumber::from(123)))
    );
    assert!(warnings.is_empty());
}

#[test]
fn infers_keybeam_keyoff_timer_function() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
            .load(
                r#"
                local off = main_state.timer_off_value
                local fade_us = 50000
                local last_update_time = off
                local last_key_on_timer = {}
                local last_key_off_timer = {}
                local active = {}
                local fade_start_time = {}
                local lanes = {
                    { display_lane = 1, key_on_timer = 101, key_off_timer = 121, hold_timer = 71 },
                    { display_lane = 2, key_on_timer = 102, key_off_timer = 122, hold_timer = 72 },
                }
                local function update()
                    local now = main_state.time()
                    if now == last_update_time then
                        return
                    end
                    last_update_time = now
                    for _, lane_info in ipairs(lanes) do
                        local lane = lane_info.display_lane
                        local key_on_time = main_state.timer(lane_info.key_on_timer)
                        local key_off_time = main_state.timer(lane_info.key_off_timer)
                        local key_off_changed = key_off_time ~= off and key_off_time ~= last_key_off_timer[lane]
                        if key_on_time ~= off and key_on_time ~= last_key_on_timer[lane] then
                            active[lane] = true
                            fade_start_time[lane] = nil
                        end
                        if key_off_changed then
                            active[lane] = true
                            fade_start_time[lane] = key_off_time
                        end
                        if fade_start_time[lane] and now >= fade_start_time[lane] + fade_us then
                            active[lane] = false
                        end
                        last_key_on_timer[lane] = key_on_time
                        last_key_off_timer[lane] = key_off_time
                    end
                end
                return function()
                    update()
                    local fade_start = fade_start_time[1]
                    if active[1] and fade_start and main_state.time() >= fade_start then
                        return fade_start
                    end
                    return off
                end
                "#,
            )
            .eval::<Function>()
            .unwrap();

    assert_eq!(infer_timer_function_ref(&function, &probe), Some(121));
}

#[test]
fn infers_main_state_judge_as_beatoraja_number_ref() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let value = lua
        .load(
            r#"
                return function()
                    return main_state.judge(1) or 0
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();
    let draw = lua
        .load(
            r#"
                return function()
                    return (main_state.judge(2) or 0) > 0
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(infer_main_state_number_ref(&value, &probe), Some(111));
    assert_eq!(
        infer_boolean_predicate(&lua, &draw, &probe, None),
        Some("number(112) > 0".to_string())
    );
}

#[test]
fn infers_weighted_pscore_value_expr_from_judge_counts() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let main_state = create_main_state_stub(&lua, probe.clone()).unwrap();
    lua.globals().set("main_state", main_state).unwrap();
    let function = lua
        .load(
            r#"
                local function clamp(value, min_value, max_value)
                    if value < min_value then
                        return min_value
                    end
                    if value > max_value then
                        return max_value
                    end
                    return value
                end

                return function()
                    local total_notes = main_state.number(74)
                    if not total_notes or total_notes <= 0 then
                        return 0
                    end

                    local cool = main_state.judge(0)
                    local great = main_state.judge(1)
                    local good = main_state.judge(2)
                    local raw = 100000 * ((cool * 1.0) + (great * 0.7) + (good * 0.4)) / total_notes
                    return clamp(math.floor(raw), 0, 100000)
                end
                "#,
        )
        .eval::<Function>()
        .unwrap();

    assert_eq!(
        infer_value_float_expr(&function, &probe),
        Some(
            "floor((100000*number(110)+70000*number(111)+40000*number(112))/number(74))"
                .to_string()
        )
    );
}

#[test]
fn infers_peaceful_play_gauge_value_builtins() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let function = lua.load("return function() return 0 end").eval::<Function>().unwrap();

    for (id, expected) in [
        ("val-gauge-percent-integer", SKIN_EXPR_GAUGE_PERCENT_INTEGER),
        ("val-gauge-percent-fraction", SKIN_EXPR_GAUGE_PERCENT_FRACTION),
        ("val-gauge-amount-integer", SKIN_EXPR_GAUGE_AMOUNT_INTEGER),
        ("val-gauge-amount-fraction", SKIN_EXPR_GAUGE_AMOUNT_FRACTION),
    ] {
        assert_eq!(
            infer_bmz_builtin_value_expr(&function, Some(id), &probe),
            Some(expected.to_string())
        );
    }
}

#[test]
fn beatoraja_skin_alias_accepts_renamed_skin_root() {
    let root = unique_skin_test_dir("renamed-root").join("mz-select");
    fs::create_dir_all(root.join("customize/advanced")).unwrap();
    fs::write(root.join("customize/advanced/enable.txt"), "parts.lua\n").unwrap();

    let resolved =
        resolve_skin_io_path(&root, "skin/m_select/customize/advanced/enable.txt").unwrap();

    assert_eq!(
        resolved,
        canonicalize_skin_path(&root.join("customize/advanced/enable.txt")).unwrap()
    );
}

#[test]
fn default_skin_file_uses_random_sentinel_for_random_def() {
    let root = unique_skin_test_dir("random-def");
    fs::create_dir_all(root.join("bg")).unwrap();
    fs::write(root.join("bg/one.mp4"), []).unwrap();
    fs::write(root.join("bg/two.mp4"), []).unwrap();
    let filepath: JsonValue =
        serde_json::from_str(r#"{ "name": "BG", "path": "bg/*.mp4", "def": "Random" }"#).unwrap();
    let path_context = test_skin_path_context(&root);

    assert_eq!(
        default_skin_file_from_filepath(&path_context, "bg/*.mp4", &filepath).as_deref(),
        Some(RANDOM_FILE_SELECTION)
    );
}

#[test]
fn default_skin_file_returns_beatoraja_filename_selection() {
    let root = unique_skin_test_dir("filename-default");
    fs::create_dir_all(root.join("bg")).unwrap();
    fs::write(root.join("bg/one.mp4"), []).unwrap();
    fs::write(root.join("bg/two.mp4"), []).unwrap();
    let filepath: JsonValue =
        serde_json::from_str(r#"{ "name": "BG", "path": "bg/*.mp4", "def": "two" }"#).unwrap();
    let path_context = test_skin_path_context(&root);

    assert_eq!(
        default_skin_file_from_filepath(&path_context, "bg/*.mp4", &filepath).as_deref(),
        Some("two.mp4")
    );
}

#[test]
fn default_skin_file_prefers_default_stem_when_def_missing() {
    let root = unique_skin_test_dir("default-stem");
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes/pastel.png"), []).unwrap();
    fs::write(root.join("notes/default.png"), []).unwrap();
    let filepath: JsonValue =
        serde_json::from_str(r#"{ "name": "Note", "path": "notes/*.png" }"#).unwrap();
    let path_context = test_skin_path_context(&root);

    assert_eq!(
        default_skin_file_from_filepath(&path_context, "notes/*.png", &filepath).as_deref(),
        Some("default.png")
    );
}

#[test]
fn property_default_matches_item_name_not_numeric_op_string() {
    let property: JsonValue = serde_json::from_str(
        r#"
            {
                "name": "Graph",
                "def": "923",
                "item": [
                    { "name": "AC", "op": 922 },
                    { "name": "TYPE-M", "op": 923 }
                ]
            }
            "#,
    )
    .unwrap();
    let items = property.get("item").and_then(JsonValue::as_array).unwrap();

    assert_eq!(default_property_op(&property, items), Some(922));
}

#[test]
fn selected_numeric_option_must_exist_in_items() {
    let items: Vec<JsonValue> = serde_json::from_str(
        r#"
            [
                { "name": "AC", "op": 922 },
                { "name": "TYPE-M", "op": 923 }
            ]
            "#,
    )
    .unwrap();

    assert_eq!(option_value_to_op(&items, "923"), Some(923));
    assert_eq!(option_value_to_op(&items, "999"), None);
}
