use super::*;

pub(super) struct ObjectFunctionMetadata {
    object_id: Option<String>,
    gauge_lead_glow: Option<(String, bool, usize)>,
    keylogger: Option<KeyloggerMetadata>,
    keylogger_chattering_lane: Option<usize>,
}

struct KeyloggerMetadata {
    graph_kind: &'static str,
    lane: usize,
    kind: Option<String>,
    slot: Option<usize>,
}

impl ObjectFunctionMetadata {
    pub(super) fn from_entries(
        entries: &[(Value, Value)],
        path: &str,
        main_state_probe: &Arc<Mutex<MainStateProbe>>,
    ) -> Self {
        let object_id = lua_object_id(entries);
        let gauge_lead_glow = object_id
            .as_deref()
            .filter(|_| path.contains(".destination["))
            .and_then(peaceful_gauge_lead_glow_id)
            .filter(|_| is_peaceful_gauge_lead_glow_destination(entries))
            .and_then(|(group, below_border)| {
                let mut probe = main_state_probe.lock().ok()?;
                let occurrence =
                    probe.gauge_lead_glow_occurrences.entry(object_id.clone()?).or_default();
                let part = *occurrence + 1;
                *occurrence += 1;
                Some((group.to_string(), below_border, part))
            });
        let keylogger = object_id.as_deref().and_then(parse_keylogger_destination_id).map(
            |(graph_kind, lane, kind)| {
                let slot = if path.contains(".destination[") {
                    main_state_probe.lock().ok().and_then(|mut probe| {
                        let occurrence = probe
                            .keylogger_destination_occurrences
                            .entry(object_id.clone()?)
                            .or_default();
                        let slot = *occurrence % 16 + 1;
                        *occurrence += 1;
                        Some(slot)
                    })
                } else {
                    None
                };
                KeyloggerMetadata { graph_kind, lane, kind: kind.map(str::to_string), slot }
            },
        );
        let keylogger_chattering_lane =
            object_id.as_deref().and_then(parse_keylogger_chattering_destination_id);
        Self { object_id, gauge_lead_glow, keylogger, keylogger_chattering_lane }
    }
}

pub(super) fn handle_function_field(
    lua: &Lua,
    function: &Function,
    key: &str,
    path: &str,
    metadata: &ObjectFunctionMetadata,
    object: &mut JsonMap<String, JsonValue>,
    warnings: &mut Vec<String>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    instruction_budget: &LuaInstructionBudget,
) -> Result<bool> {
    // 推論は同じ callback を probe 値を変えて複数回呼ぶ。callback ごとに
    // 新しい上限を与えつつ、推論全体の上限は別に維持する。
    instruction_budget.begin_callback();
    match key {
        "value" => {
            let runtime_supported = runtime_value_field_supported(path);
            let compat = main_state_probe
                .lock()
                .map_err(|_| anyhow!("main_state probe lock poisoned"))?
                .runtime_mode
                == LuaSkinRuntimeMode::Compat;
            let inferred = if compat && runtime_supported {
                false
            } else {
                infer_value_field(
                    lua,
                    function,
                    path,
                    metadata.object_id.as_deref(),
                    object,
                    main_state_probe,
                )
            };
            if !inferred && runtime_supported {
                let field_path = format!("{path}.value");
                let callback_id = register_runtime_callback(
                    main_state_probe,
                    &field_path,
                    LuaRuntimeCallbackKind::Value,
                )?;
                insert_expr(
                    object,
                    "value_expr",
                    format!("{LUA_VALUE_CALLBACK_PREFIX}{callback_id}"),
                );
                tracing::debug!(
                    %field_path,
                    callback_id,
                    classification = "RUNTIME",
                    "classified Lua value property"
                );
            } else if !inferred {
                warnings.push(format!("skipping unsupported value function at {path}.value"));
            }
            Ok(true)
        }
        "act" => Ok(infer_act_field(lua, function, object, main_state_probe)),
        "action" if path.contains(".customEvents[") => {
            Ok(infer_action_field(lua, function, object, main_state_probe))
        }
        "condition" if path.contains(".customEvents[") => {
            Ok(infer_condition_field(function, object, main_state_probe))
        }
        "draw" => {
            infer_draw_field(lua, function, path, metadata, object, main_state_probe)?;
            Ok(true)
        }
        "timer" => {
            infer_timer_field(lua, function, path, metadata, object, warnings, main_state_probe)
        }
        _ => Ok(false),
    }
}

fn infer_value_field(
    lua: &Lua,
    function: &Function,
    path: &str,
    object_id: Option<&str>,
    object: &mut JsonMap<String, JsonValue>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    let is_graph = path.contains(".graph[");
    if matches!(object_id, Some("val-hits-per-sec")) {
        insert_expr(object, "value_expr", "bmz:keylogger_nps");
        return true;
    }
    if is_graph && let Some(value_expr) = object_id.and_then(keylogger_graph_value_expr_from_id) {
        insert_expr(object, "value_expr", value_expr);
        return true;
    }
    if is_graph
        && let Some(value_expr) =
            object_id.and_then(milliondollar_fast_slow_graph_value_expr_from_id)
    {
        insert_expr(object, "value_expr", value_expr);
        return true;
    }
    if !is_graph
        && path.contains(".imageset[")
        && let Some(ref_id) = infer_gauge_type_imageset_ref(function, main_state_probe)
    {
        insert_number(object, "ref", ref_id);
        return true;
    }
    if !is_graph && path.contains(".text[") {
        if let Some(ref_id) = infer_ir_ranking_name_ref(function, object_id, main_state_probe) {
            insert_number(object, "ref", ref_id);
            return true;
        }
        if let Some(value_expr) =
            infer_course_table_text_expr(function, object_id, main_state_probe)
        {
            insert_expr(object, "value_expr", value_expr);
            return true;
        }
        if let Some(value_expr) = infer_text_concat_expr(function, main_state_probe) {
            insert_expr(object, "value_expr", value_expr);
            return true;
        }
        if let Some(ref_id) = infer_main_state_text_ref(function, main_state_probe) {
            insert_number(object, "ref", ref_id);
            return true;
        }
    }
    if !is_graph
        && path.contains(".slider[")
        && let Some(value_expr) = infer_slider_value_expr(function, object_id, main_state_probe)
    {
        insert_expr(object, "value_expr", value_expr);
        return true;
    }
    if !is_graph && infer_non_graph_value_field(function, object_id, object, main_state_probe) {
        return true;
    }
    if is_graph
        && let Some(value_expr) =
            infer_ir_ranking_score_value_expr(function, object_id, main_state_probe)
    {
        insert_expr(object, "value_expr", value_expr);
        true
    } else if is_graph
        && let Some(graph_type) = infer_fast_slow_ratio_graph_type(function, main_state_probe)
    {
        insert_number(object, "type", graph_type);
        true
    } else if !is_graph && let Some(expr) = infer_main_state_number_expr(function, main_state_probe)
    {
        insert_expr(object, "expr", expr);
        true
    } else if is_graph && matches!(object_id, Some("default_chart_gauge")) {
        insert_expr(object, "value_expr", "bmz:default_chart_gauge");
        true
    } else if !is_graph && matches!(object_id, Some("default_chart_total_count")) {
        insert_expr(object, "value_expr", "bmz:default_chart_total_count");
        true
    } else if let Some(value_expr) = infer_value_float_expr(function, main_state_probe) {
        insert_expr(object, "value_expr", value_expr);
        true
    } else if path.contains(".text[")
        && let Some(ref_id) = infer_constant_text_ref_at_load(function, main_state_probe)
    {
        insert_number(object, "ref", ref_id);
        true
    } else if path.contains(".text[")
        && let Some(text) = infer_constant_text_at_load(lua, function, main_state_probe)
    {
        insert_expr(object, "constantText", text);
        true
    } else if let Some(value_expr) = infer_constant_number_at_load(lua, function, main_state_probe)
    {
        insert_expr(object, "value_expr", value_expr);
        true
    } else if matches!(object_id, Some("Number_Info_Level_Insane")) {
        // 空の難易度表レベルから nil を返す場合は、非表示 destination 用の不活性値にする。
        insert_expr(object, "value_expr", "0");
        true
    } else {
        false
    }
}

fn runtime_value_field_supported(path: &str) -> bool {
    [".value[", ".text[", ".graph[", ".slider["].into_iter().any(|segment| path.contains(segment))
}

fn infer_non_graph_value_field(
    function: &Function,
    object_id: Option<&str>,
    object: &mut JsonMap<String, JsonValue>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    let value_expr = infer_nearest_rank_diff_value_expr(function, object_id, main_state_probe)
        .or_else(|| infer_ir_ranking_score_diff_value_expr(function, object_id, main_state_probe))
        .or_else(|| infer_ir_ranking_score_rate_value_expr(function, object_id, main_state_probe))
        .or_else(|| infer_bmz_builtin_value_expr(function, object_id, main_state_probe));
    if let Some(value_expr) = value_expr {
        insert_expr(object, "value_expr", value_expr);
        return true;
    }
    if let Some(ref_id) = infer_gated_number_ref(function, main_state_probe)
        .or_else(|| infer_main_state_number_ref(function, main_state_probe))
    {
        insert_number(object, "ref", ref_id);
        return true;
    }
    false
}

fn infer_act_field(
    lua: &Lua,
    function: &Function,
    object: &mut JsonMap<String, JsonValue>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    let event_id = infer_constant_integer_at_load(function, main_state_probe)
        .or_else(|| infer_runtime_toggle_act(lua, function, main_state_probe))
        .or_else(|| infer_result_panel_act_at_load(lua, function, main_state_probe));
    if let Some(event_id) = event_id {
        insert_number(object, "act", event_id);
        true
    } else {
        false
    }
}

fn infer_action_field(
    lua: &Lua,
    function: &Function,
    object: &mut JsonMap<String, JsonValue>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    let Some(actions) = infer_custom_audio_event_action(lua, function, main_state_probe) else {
        return false;
    };
    object.insert(
        "audioActions".to_string(),
        JsonValue::Array(actions.into_iter().map(lua_audio_action_to_json).collect()),
    );
    object.insert("once".to_string(), JsonValue::Bool(true));
    true
}

fn infer_condition_field(
    function: &Function,
    object: &mut JsonMap<String, JsonValue>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    let Some(timer_id) = infer_timer_on_condition(function, main_state_probe) else {
        return false;
    };
    insert_number(object, "timer", timer_id);
    true
}

fn infer_draw_field(
    lua: &Lua,
    function: &Function,
    path: &str,
    metadata: &ObjectFunctionMetadata,
    object: &mut JsonMap<String, JsonValue>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Result<()> {
    let object_id = metadata.object_id.as_deref();
    let compat = main_state_probe
        .lock()
        .map_err(|_| anyhow!("main_state probe lock poisoned"))?
        .runtime_mode
        == LuaSkinRuntimeMode::Compat;
    let draw = if compat {
        None
    } else {
        infer_result_panel_draw_condition(lua, function, object_id, main_state_probe)
            .or_else(|| {
                infer_luxe_flat_select_score_draw(lua, function, object_id, main_state_probe)
            })
            .or_else(|| infer_result_score_draw(function, object_id, main_state_probe))
            .or_else(|| {
                metadata.gauge_lead_glow.as_ref().map(|(group, below_border, part)| {
                    format!(
                        "gauge_lead_glow({group},{part},{})",
                        if *below_border { "below" } else { "above" }
                    )
                })
            })
            .or_else(|| {
                let keylogger = metadata.keylogger.as_ref()?;
                Some(format!(
                    "keylogger_{}({},{},{})",
                    keylogger.graph_kind,
                    keylogger.lane,
                    keylogger.slot?,
                    keylogger.kind.as_deref()?
                ))
            })
            .or_else(|| {
                infer_gauge_value_digit_draw_condition(function, object_id, main_state_probe)
            })
            .or_else(|| {
                infer_select_score_available_draw_condition(lua, function, main_state_probe)
            })
            .or_else(|| infer_boolean_predicate(lua, function, main_state_probe, object_id))
    };

    let field_path = format!("{path}.draw");
    if let Some(draw) = draw {
        insert_expr(object, "draw", draw);
        tracing::debug!(%field_path, classification = "COMPILED", "classified Lua draw condition");
    } else {
        let callback_id =
            register_runtime_callback(main_state_probe, &field_path, LuaRuntimeCallbackKind::Draw)?;
        insert_expr(object, "draw", format!("{LUA_DRAW_CALLBACK_PREFIX}{callback_id}"));
        tracing::debug!(%field_path, callback_id, classification = "RUNTIME", "classified Lua draw condition");
    }
    Ok(())
}

fn infer_timer_field(
    lua: &Lua,
    function: &Function,
    path: &str,
    metadata: &ObjectFunctionMetadata,
    object: &mut JsonMap<String, JsonValue>,
    warnings: &mut Vec<String>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Result<bool> {
    if let Some(lane) = metadata.keylogger_chattering_lane {
        insert_expr(object, "timer_expr", format!("bmz:keylogger_chattering:{lane}"));
        return Ok(true);
    }
    if let Some(keylogger) = &metadata.keylogger
        && let Some(slot) = keylogger.slot
    {
        insert_expr(object, "timer_expr", format!("bmz:keylogger_event:{}:{slot}", keylogger.lane));
        return Ok(true);
    }

    let custom_timer = path.contains(".customTimers[");
    if custom_timer
        && let Some(id) = metadata.object_id.as_deref().and_then(|id| id.parse::<i32>().ok())
        && let Some((source_timer, delay_ms)) = infer_fixed_delay_timer(function, main_state_probe)
            .or_else(|| {
                infer_custom_timer_alias(function, main_state_probe)
                    .map(|source_timer| (source_timer, 0))
            })
    {
        if let Ok(mut probe) = main_state_probe.lock()
            && !probe.fixed_delay_timers.iter().any(|(existing, _, _)| *existing == id)
        {
            probe.fixed_delay_timers.push((id, source_timer, delay_ms));
        }
        return Ok(true);
    }

    let map: Table = lua.globals().get("bmz_timer_fn_map")?;
    if let Ok(timer_id) = map.get::<i32>(function.clone()) {
        insert_number(object, "timer", timer_id);
        return Ok(true);
    }
    if let Some(timer_id) = infer_timer_function_ref(function, main_state_probe) {
        insert_number(object, "timer", timer_id);
        return Ok(true);
    }
    if custom_timer {
        let id = metadata.object_id.as_deref().unwrap_or("unknown");
        warnings
            .push(format!("skipping unsupported custom timer function id {id} at {path}.timer"));
        return Ok(true);
    }
    Ok(false)
}

fn insert_number(object: &mut JsonMap<String, JsonValue>, key: &str, value: impl Into<JsonNumber>) {
    object.insert(key.to_string(), JsonValue::Number(value.into()));
}

fn insert_expr(object: &mut JsonMap<String, JsonValue>, key: &str, value: impl Into<String>) {
    object.insert(key.to_string(), JsonValue::String(value.into()));
}
