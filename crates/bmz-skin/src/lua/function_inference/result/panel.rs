use super::*;

pub(in crate::lua) fn infer_result_panel_draw_condition(
    lua: &Lua,
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    const ALWAYS_TRUE: &str = "number(0) >= 0";
    const ALWAYS_FALSE: &str = "number(0) < 0";

    let globals = lua.globals();
    let global_original = globals
        .raw_get::<Value>("Expand_op")
        .ok()
        .filter(|value| lua_result_panel_value(value.clone()).is_some());
    let local_original = if global_original.is_none() {
        let (index, mode) = lua_result_mode_upvalue(lua, function)?;
        record_local_result_panel_default(main_state_probe, mode)?;
        Some((index, mode))
    } else {
        None
    };

    let mut conditions = Vec::with_capacity(3);
    for panel in 0..=2 {
        let state_updated = if global_original.is_some() {
            globals.raw_set("Expand_op", panel).is_ok()
        } else if let Some((index, _)) = local_original {
            // Luxe Flat: result_mode 0=GRAPH, 1=IR. Use 2 for the inactive
            // BMZ panel state so neither equality branch is selected.
            let mode = match panel {
                1 => 1,
                2 => 0,
                _ => 2,
            };
            set_lua_integer_upvalue(lua, function, index, mode)
        } else {
            false
        };
        if !state_updated {
            restore_result_panel_probe_state(
                lua,
                function,
                global_original.as_ref(),
                local_original,
            );
            return None;
        }
        let specialized = infer_result_score_draw(function, object_id, main_state_probe);
        conditions.push(if result_score_draw_object(object_id) {
            specialized.or_else(|| infer_config_or_panel_constant_draw(lua, function, true))
        } else {
            specialized
                .or_else(|| infer_boolean_predicate(lua, function, main_state_probe, object_id))
                .or_else(|| infer_config_or_panel_constant_draw(lua, function, true))
        });
    }
    restore_result_panel_probe_state(lua, function, global_original.as_ref(), local_original);

    if conditions.windows(2).all(|pair| pair[0] == pair[1]) {
        return None;
    }

    let branches = conditions
        .into_iter()
        .enumerate()
        .flat_map(|(panel, condition)| match condition.as_deref() {
            None | Some(ALWAYS_FALSE) => Vec::new(),
            Some(ALWAYS_TRUE) => vec![format!("result_panel({panel})")],
            Some(condition) => condition
                .split(" or ")
                .map(|branch| format!("result_panel({panel}) and {branch}"))
                .collect(),
        })
        .collect::<Vec<_>>();
    (!branches.is_empty()).then(|| branches.join(" or "))
}

pub(in crate::lua) fn restore_result_panel_probe_state(
    lua: &Lua,
    function: &Function,
    global_original: Option<&Value>,
    local_original: Option<(i32, i32)>,
) {
    if let Some(original) = global_original {
        let _ = lua.globals().raw_set("Expand_op", original.clone());
    } else if let Some((index, mode)) = local_original {
        let _ = set_lua_integer_upvalue(lua, function, index, mode);
    }
}

pub(in crate::lua) fn result_score_draw_object(object_id: Option<&str>) -> bool {
    object_id.is_some_and(|id| {
        id == "scoreGraph"
            || id.starts_with("ir_scoreGraph")
            || id == "irYouFrame"
            || id.starts_with("nextRank")
            || matches!(id, "diff_plus" | "diff_minus" | "diff_rank")
    })
}

pub(in crate::lua) fn ir_ranking_slot_from_id(id: &str, prefix: &str) -> Option<i32> {
    let slot = id.strip_prefix(prefix)?.parse::<i32>().ok()?;
    (1..=10).contains(&slot).then_some(slot)
}

pub(in crate::lua) fn modern_chic_ir_ranking_graph(id: &str) -> Option<(i32, &'static str)> {
    let suffix = id.strip_prefix("s_rankingGraph")?;
    let digit_start = suffix.find(|character: char| character.is_ascii_digit())?;
    let (rank, slot) = suffix.split_at(digit_start);
    let rank = match rank {
        "AAA" => "AAA",
        "AA" => "AA",
        "A" => "A",
        "B" => "B",
        "C" => "C",
        "D" => "D",
        "E" => "E",
        "F" => "F",
        _ => return None,
    };
    let slot = slot.parse::<i32>().ok()?;
    (1..=10).contains(&slot).then_some((slot, rank))
}
