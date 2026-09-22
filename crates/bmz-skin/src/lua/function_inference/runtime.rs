use super::*;

/// `CUSTOMS.some_flag` のようなトップレベル bool 参照を宣言的 runtime flag へ写す。
///
/// 任意テーブルやネストした値は触らず、各 bool を一つずつ反転して callback の結果が
/// 反転・復元する単一依存だけを受理する。これにより描画中の Lua 実行を避けつつ、
/// MILLIONDOLLAR の表示切替 callback を Rust 側の状態へ移せる。
pub(in crate::lua) fn infer_runtime_boolean_field_observe(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let baseline = match function.call::<Value>(()).ok()? {
        Value::Boolean(value) => value,
        _ => return None,
    };
    let customs = lua.globals().get::<Table>("CUSTOMS").ok()?;
    let bool_fields = customs
        .clone()
        .pairs::<Value, Value>()
        .filter_map(|entry| {
            let (key, value) = entry.ok()?;
            let Value::String(key) = key else {
                return None;
            };
            let Value::Boolean(value) = value else {
                return None;
            };
            Some((key.to_str().ok()?.to_string(), value))
        })
        .collect::<Vec<_>>();

    let mut candidates = Vec::new();
    for (field, initial) in bool_fields {
        customs.set(field.as_str(), !initial).ok()?;
        let flipped = function.call::<Value>(()).ok();
        customs.set(field.as_str(), initial).ok()?;
        let restored = function.call::<Value>(()).ok();
        if matches!(flipped, Some(Value::Boolean(value)) if value != baseline)
            && matches!(restored, Some(Value::Boolean(value)) if value == baseline)
        {
            candidates.push((field, initial));
        }
    }
    let [(field, initial)] = candidates.as_slice() else {
        return None;
    };

    let flag_id = {
        let mut probe = main_state_probe.lock().ok()?;
        if let Some(flag) =
            probe.runtime_flags.iter().find(|flag| flag.table == "CUSTOMS" && flag.field == *field)
        {
            flag.id
        } else {
            let id = probe.next_runtime_flag_id;
            probe.next_runtime_flag_id += 1;
            probe.runtime_flags.push(LuaRuntimeFlagProbe {
                id,
                table: "CUSTOMS".to_string(),
                field: field.clone(),
                initial: *initial,
            });
            id
        }
    };
    Some(if baseline == *initial {
        format!("runtime_flag({flag_id})")
    } else {
        format!("not runtime_flag({flag_id})")
    })
}

pub(in crate::lua) fn lua_runtime_scalar(value: Value) -> Option<LuaRuntimeScalar> {
    match value {
        Value::Boolean(value) => Some(LuaRuntimeScalar::Boolean(value)),
        Value::Integer(value) => Some(LuaRuntimeScalar::Integer(value)),
        Value::Number(value) if value.is_finite() => Some(LuaRuntimeScalar::Number(value)),
        Value::String(value) => Some(LuaRuntimeScalar::String(value.as_bytes().to_vec())),
        _ => None,
    }
}

pub(in crate::lua) fn lua_audio_action_to_json(action: LuaAudioActionProbe) -> JsonValue {
    let action_name = match action.action {
        LuaAudioActionKindProbe::Play => "play",
        LuaAudioActionKindProbe::Loop => "loop",
        LuaAudioActionKindProbe::Stop => "stop",
    };
    let volume =
        JsonNumber::from_f64(action.volume.clamp(0.0, 1.0)).unwrap_or_else(|| JsonNumber::from(0));
    JsonValue::Object(JsonMap::from_iter([
        ("action".to_string(), JsonValue::String(action_name.to_string())),
        ("path".to_string(), JsonValue::String(action.path)),
        ("volume".to_string(), JsonValue::Number(volume)),
    ]))
}

/// timer が off のとき false、on のとき true になる単一 timer 条件だけを受理する。
pub(in crate::lua) fn infer_timer_on_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    let timers = collect_timer_refs(function, main_state_probe)?;
    let [timer_id] = timers.as_slice() else { return None };
    let off = call_draw_with_numbers_and_timers(
        function,
        main_state_probe,
        BTreeMap::new(),
        BTreeMap::from([(*timer_id, TIMER_OFF_VALUE)]),
    )?;
    let on = call_draw_with_numbers_and_timers(
        function,
        main_state_probe,
        BTreeMap::new(),
        BTreeMap::from([(*timer_id, 123_456)]),
    )?;
    (!off && on).then_some(*timer_id)
}

/// `customEvents.action` を一度だけ sandbox 内で呼び、音声命令以外に
/// `CUSTOMS` の scalar 状態を変えない callback を宣言データへ落とす。
pub(in crate::lua) fn infer_custom_audio_event_action(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<LuaAudioActionProbe>> {
    let customs = lua.globals().get::<Table>("CUSTOMS").ok();
    let before = customs.as_ref().and_then(customs_scalar_snapshot);
    {
        let mut probe = main_state_probe.lock().ok()?;
        probe.audio_actions.clear();
        probe.capture_audio_actions = true;
    }
    let result = function.call::<Value>(()).ok();
    let actions = {
        let mut probe = main_state_probe.lock().ok()?;
        probe.capture_audio_actions = false;
        probe.take_audio_actions()
    };
    let after = customs.as_ref().and_then(customs_scalar_snapshot);
    if !matches!(result, Some(Value::Nil))
        || actions.is_empty()
        || before.is_some() && before != after
    {
        return None;
    }
    Some(actions)
}

pub(in crate::lua) fn customs_scalar_snapshot(
    customs: &Table,
) -> Option<BTreeMap<String, LuaRuntimeScalar>> {
    let mut snapshot = BTreeMap::new();
    for entry in customs.clone().pairs::<Value, Value>() {
        let (key, value) = entry.ok()?;
        let Value::String(key) = key else {
            continue;
        };
        let Some(value) = lua_runtime_scalar(value) else {
            continue;
        };
        snapshot.insert(key.to_str().ok()?.to_string(), value);
    }
    Some(snapshot)
}

pub(in crate::lua) fn restore_customs_scalar_snapshot(
    lua: &Lua,
    customs: &Table,
    snapshot: &BTreeMap<String, LuaRuntimeScalar>,
) -> mlua::Result<()> {
    for (field, value) in snapshot {
        match value {
            LuaRuntimeScalar::Boolean(value) => customs.set(field.as_str(), *value)?,
            LuaRuntimeScalar::Integer(value) => customs.set(field.as_str(), *value)?,
            LuaRuntimeScalar::Number(value) => customs.set(field.as_str(), *value)?,
            LuaRuntimeScalar::String(value) => {
                customs.set(field.as_str(), lua.create_string(value.as_slice())?)?
            }
        }
    }
    Ok(())
}

/// 登録済み `CUSTOMS` bool だけを反転し、二回呼ぶと全 scalar が復元する act を
/// `runtimeEvent` へ変換する。外部副作用を持つ任意 callback は対象にしない。
pub(in crate::lua) fn infer_runtime_toggle_act(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i64> {
    let registered = main_state_probe.lock().ok()?.runtime_flags.clone();
    if registered.is_empty() || registered.iter().any(|flag| flag.table != "CUSTOMS") {
        return None;
    }
    let customs = lua.globals().get::<Table>("CUSTOMS").ok()?;
    let before = customs_scalar_snapshot(&customs)?;
    let first_result = function.call::<Value>(()).ok();
    let after_first = customs_scalar_snapshot(&customs)?;

    let mut changed_flag_ids = Vec::new();
    let mut safe = matches!(first_result, Some(Value::Nil));
    for (field, before_value) in &before {
        let after_value = after_first.get(field);
        if after_value == Some(before_value) {
            continue;
        }
        let (LuaRuntimeScalar::Boolean(before_bool), Some(LuaRuntimeScalar::Boolean(after_bool))) =
            (before_value, after_value)
        else {
            safe = false;
            continue;
        };
        if *after_bool == *before_bool {
            safe = false;
            continue;
        }
        let Some(flag) = registered.iter().find(|flag| flag.field == *field) else {
            safe = false;
            continue;
        };
        changed_flag_ids.push(flag.id);
    }
    if before.len() != after_first.len() || changed_flag_ids.is_empty() {
        safe = false;
    }

    let second_result = function.call::<Value>(()).ok();
    let after_second = customs_scalar_snapshot(&customs);
    let _ = restore_customs_scalar_snapshot(lua, &customs, &before);
    if !safe || !matches!(second_result, Some(Value::Nil)) || after_second.as_ref() != Some(&before)
    {
        return None;
    }

    changed_flag_ids.sort_unstable();
    changed_flag_ids.dedup();
    let event_id = {
        let mut probe = main_state_probe.lock().ok()?;
        if let Some(event_id) = probe.runtime_event_ids_by_flags.get(&changed_flag_ids) {
            *event_id
        } else {
            let event_id = probe.next_runtime_event_id;
            probe.next_runtime_event_id -= 1;
            probe.runtime_event_ids_by_flags.insert(changed_flag_ids.clone(), event_id);
            probe.runtime_events.push((event_id, changed_flag_ids));
            event_id
        }
    };
    Some(i64::from(event_id))
}

/// Starseeker 等が `return is_gauge_iidx` / `return not is_gauge_iidx` と書くが
/// グローバルを定義しないスキン向け。ロード時に真偽を切り替えて EX-HARD/HAZARD 相当へ写す。
pub(in crate::lua) fn infer_is_gauge_iidx_global_observe(
    lua: &Lua,
    function: &Function,
) -> Option<String> {
    let globals = lua.globals();
    let previous = globals.get::<Value>("is_gauge_iidx").ok();
    let selected_gauge_display = globals
        .get::<Table>("skin_config")
        .ok()
        .and_then(|skin_config| skin_config.get::<Table>("option").ok())
        .and_then(|option| option.get::<i64>("グルーヴゲージ表示").ok());

    fn observe_truth(function: &Function) -> Option<bool> {
        match function.call::<Value>(()).ok()? {
            Value::Boolean(value) => Some(value),
            Value::Nil => Some(false),
            _ => None,
        }
    }

    globals.set("is_gauge_iidx", false).ok()?;
    let when_false = observe_truth(function)?;
    globals.set("is_gauge_iidx", true).ok()?;
    let when_true = observe_truth(function)?;

    if let Some(value) = previous {
        globals.set("is_gauge_iidx", value).ok()?;
    } else {
        globals.raw_remove("is_gauge_iidx").ok()?;
    }

    match (when_false, when_true) {
        (false, true) if selected_gauge_display == Some(930) => Some("number(0) < 0".to_string()),
        (true, false) if selected_gauge_display == Some(930) => Some("number(0) >= 0".to_string()),
        (false, true) => Some("gauge_type() == 4 or gauge_type() == 5".to_string()),
        (true, false) => Some("gauge_type() != 4 and gauge_type() != 5".to_string()),
        _ => None,
    }
}

pub(in crate::lua) fn infer_boolean_predicate(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    object_id: Option<&str>,
) -> Option<String> {
    // PeacefulPlay のキービーム関数は closure 内に前フレーム状態を持つ。
    // 汎用probeを先に走らせるとその状態が変化し、特に最後のLane9が
    // 定数falseへ畳み込まれるため、対象objectは専用推論を最初に行う。
    if object_id.is_some_and(|id| id.starts_with("key-beam-"))
        && let Some(predicate) =
            infer_keybeam_timer_event_draw_condition(function, main_state_probe)
    {
        return Some(predicate);
    }
    if let Some(predicate) = infer_all_timers_off_draw_condition(function, main_state_probe) {
        return Some(predicate);
    }
    // Probe short-circuit option/timer predicates before simpler single-option
    // inference can collapse them to the first branch alone.
    if let Some(predicate) =
        infer_main_state_two_options_timer_draw_condition(function, main_state_probe)
    {
        return Some(predicate);
    }
    let refs = collect_number_refs(function, main_state_probe).unwrap_or_default();
    infer_result_average_timing_sign_draw_condition(function, main_state_probe)
        .or_else(|| {
            if refs.len() >= 2 {
                infer_or_of_number_gt_zero(function, main_state_probe)
                    .or_else(|| infer_or_of_number_eq_zero(function, main_state_probe))
                    .or_else(|| infer_or_of_number_lt_zero(function, main_state_probe))
                    .or_else(|| infer_two_number_compare_and(function, main_state_probe))
            } else {
                None
            }
        })
        .or_else(|| infer_float_number_and_number_and_draw(function, main_state_probe))
        .or_else(|| infer_main_state_event_index_options_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_option_number_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_event_index_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_gauge_type_draw_condition(function, main_state_probe))
        .or_else(|| infer_keybeam_timer_event_draw_condition(function, main_state_probe))
        .or_else(|| infer_main_state_timer_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_end_of_note_shadow_draw_condition(function, main_state_probe))
        .or_else(|| infer_os_clock_after_draw_condition(function, main_state_probe))
        .or_else(|| infer_os_clock_after_option_draw_condition(function, main_state_probe))
        .or_else(|| infer_judge_fast_slow_draw_condition(function, main_state_probe, object_id))
        .or_else(|| infer_or_of_number_gt_zero(function, main_state_probe))
        .or_else(|| infer_or_of_number_eq_zero(function, main_state_probe))
        .or_else(|| infer_or_of_number_lt_zero(function, main_state_probe))
        .or_else(|| infer_two_number_compare_and(function, main_state_probe))
        .or_else(|| infer_number_eq_zero_with_constant_tail(function, main_state_probe))
        .or_else(|| infer_constant_draw_at_load(lua, function, main_state_probe))
}

/// Fold only callbacks that cannot read globals or captured mutable state.
pub(in crate::lua) fn infer_constant_draw_at_load(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    if !is_closed_lua_function(lua, function) {
        return infer_config_or_panel_constant_draw(lua, function, false);
    }
    main_state_probe.lock().ok()?.end_recording();
    let first = match function.call::<Value>(()).ok()? {
        Value::Boolean(value) => value,
        _ => return None,
    };
    if first { Some("number(0) >= 0".to_string()) } else { Some("number(0) < 0".to_string()) }
}

pub(in crate::lua) fn infer_constant_text_at_load(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    if !is_closed_lua_function(lua, function) {
        return None;
    }
    main_state_probe.lock().ok()?.end_recording();
    match function.call::<Value>(()).ok()? {
        Value::String(value) => Some(value.to_string_lossy()),
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) if value.is_finite() => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(in crate::lua) fn infer_constant_text_ref_at_load(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    // This recognizer is deliberately separate from constant folding: a whole
    // sentinel represents a dynamic text ref, never a constant string.
    main_state_probe.lock().ok()?.end_recording();
    let Value::String(text) = function.call::<Value>(()).ok()? else {
        return None;
    };
    let text = text.to_string_lossy();
    let ref_id = text
        .strip_prefix(LUA_TEXT_REF_SENTINEL_PREFIX)?
        .strip_suffix(LUA_TEXT_REF_SENTINEL_SUFFIX)?
        .parse::<i32>()
        .ok()?;
    (1001..=1003).contains(&ref_id).then_some(ref_id)
}

pub(in crate::lua) fn repair_keybeam_destination_draws(
    root: &mut JsonMap<String, JsonValue>,
) -> BTreeSet<usize> {
    let mut repaired = BTreeSet::new();
    let Some(destinations) = root.get_mut("destination").and_then(JsonValue::as_array_mut) else {
        return repaired;
    };
    for index in 0..destinations.len().saturating_sub(1) {
        let Some((hold_draw, fade_draw, fade_timer)) =
            keybeam_draw_replacements(&destinations[index], &destinations[index + 1])
        else {
            continue;
        };
        if let JsonValue::Object(destination) = &mut destinations[index] {
            destination.insert("draw".to_string(), JsonValue::String(hold_draw));
        }
        if let JsonValue::Object(destination) = &mut destinations[index + 1] {
            destination
                .insert("timer".to_string(), JsonValue::Number(JsonNumber::from(fade_timer)));
            destination.insert("draw".to_string(), JsonValue::String(fade_draw));
        }
        // Lua table path is 1-based, while the converted JSON array is 0-based.
        repaired.insert(index + 1);
        repaired.insert(index + 2);
    }
    repaired
}

pub(in crate::lua) fn keybeam_draw_replacements(
    hold: &JsonValue,
    fade: &JsonValue,
) -> Option<(String, String, i32)> {
    let hold = hold.as_object()?;
    let fade = fade.as_object()?;
    let hold_id = json_string_field(hold, "id")?;
    if !hold_id.starts_with("key-beam-") || hold_id != json_string_field(fade, "id")? {
        return None;
    }
    if json_i32_field(hold, "timer").is_some() || json_i32_field(hold, "loop") == Some(-1) {
        return None;
    }
    if json_i32_field(fade, "loop") != Some(-1) {
        return None;
    }
    let inferred_fade_draw = json_string_field(fade, "draw")?;
    let fade_timer = json_i32_field(fade, "timer")
        .filter(|timer| is_keybeam_keyoff_timer(*timer))
        .or_else(|| keybeam_keyoff_timer_from_draw(inferred_fade_draw))?;
    let fallback_draw;
    let fade_draw = if inferred_fade_draw.contains("event_index(") {
        inferred_fade_draw
    } else {
        fallback_draw = keybeam_judge_draw_from_id(hold_id, fade_timer)?;
        &fallback_draw
    };
    let keyon_timer = keybeam_keyon_timer_for_keyoff_timer(fade_timer)?;
    let hold_timer = keybeam_hold_timer_for_keyon_timer(keyon_timer)?;
    let hold_draw = keybeam_hold_draw_from_fade_draw(fade_draw, keyon_timer, hold_timer)?;
    let fade_draw = fade_draw
        .split(" or ")
        .map(str::trim)
        .map(|branch| format!("keybeam_fade({fade_timer}) != 0 and {branch}"))
        .collect::<Vec<_>>()
        .join(" or ");
    Some((hold_draw, fade_draw, fade_timer))
}

pub(in crate::lua) fn keybeam_judge_draw_from_id(id: &str, keyoff_timer: i32) -> Option<String> {
    let event_id = keyoff_timer.checked_add(380)?;
    let values: &[i32] = if id.ends_with("-pgreat") {
        &[1]
    } else if id.ends_with("-great") {
        &[2, 3]
    } else if id.ends_with("-good") {
        &[4, 5]
    } else if id.ends_with("-other") {
        &[0, 6, 7, 8, 9]
    } else {
        return None;
    };
    Some(
        values
            .iter()
            .map(|value| format!("event_index({event_id}) == {value}"))
            .collect::<Vec<_>>()
            .join(" or "),
    )
}

pub(in crate::lua) fn json_string_field<'a>(
    object: &'a JsonMap<String, JsonValue>,
    key: &str,
) -> Option<&'a str> {
    object.get(key)?.as_str()
}

pub(in crate::lua) fn json_i32_field(
    object: &JsonMap<String, JsonValue>,
    key: &str,
) -> Option<i32> {
    i32::try_from(object.get(key)?.as_i64()?).ok()
}

pub(in crate::lua) fn keybeam_keyoff_timer_from_draw(draw: &str) -> Option<i32> {
    let event_ids = draw
        .split("event_index(")
        .skip(1)
        .filter_map(|tail| tail.split_once(')')?.0.trim().parse::<i32>().ok())
        .collect::<BTreeSet<_>>();
    let event_id = (event_ids.len() == 1).then(|| *event_ids.first().unwrap())?;
    (500..=517).contains(&event_id).then_some(event_id - 380)
}

pub(in crate::lua) fn keybeam_keyon_timer_for_keyoff_timer(timer_id: i32) -> Option<i32> {
    match timer_id {
        120..=137 => Some(timer_id - 20),
        _ => None,
    }
}

pub(in crate::lua) fn keybeam_hold_draw_from_fade_draw(
    fade_draw: &str,
    keyon_timer: i32,
    _hold_timer: i32,
) -> Option<String> {
    let prefix = format!("keybeam_hold({keyon_timer}) != 0 and ");
    let branches = fade_draw
        .split(" or ")
        .map(str::trim)
        .filter(|branch| branch.contains("event_index("))
        .map(|branch| format!("{prefix}{branch}"))
        .collect::<Vec<_>>();
    (!branches.is_empty()).then(|| branches.join(" or "))
}
