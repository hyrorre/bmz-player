use super::*;

pub(in crate::lua) fn infer_constant_number_at_load(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    // A numeric result at load time does not prove that a callback is constant.
    // Captured helpers, mutable locals and _ENV can all supply runtime state,
    // including dependencies hidden behind a branch not visited by the probes.
    // Only fold Lua functions with no upvalues; leave the rest to runtime.
    if !is_closed_lua_function(lua, function) {
        return None;
    }
    main_state_probe.lock().ok()?.end_recording();
    match function.call::<Value>(()).ok()? {
        Value::Integer(value) => Some(value.to_string()),
        Value::Number(value) if value.is_finite() => Some(value.to_string()),
        _ => None,
    }
}

pub(in crate::lua) fn is_closed_lua_function(lua: &Lua, function: &Function) -> bool {
    if function.info().what != "Lua" {
        return false;
    }
    // SAFETY: the helper only inspects its argument and balances the Lua stack.
    unsafe { lua.create_c_function(has_function_upvalue) }
        .and_then(|helper| helper.call::<bool>(function.clone()))
        .is_ok_and(|has_upvalue| !has_upvalue)
}

unsafe extern "C-unwind" fn has_function_upvalue(state: *mut mlua::lua_State) -> std::ffi::c_int {
    // SAFETY: mlua supplies a live state and the caller passes a Function in
    // slot 1. lua_getupvalue pushes one value only when an upvalue exists.
    unsafe {
        let has_upvalue = !mlua::ffi::lua_getupvalue(state, 1, 1).is_null();
        if has_upvalue {
            mlua::ffi::lua_pop(state, 1);
        }
        mlua::ffi::lua_pushboolean(state, i32::from(has_upvalue));
        1
    }
}

/// Evaluate a copy against only immutable config, or the panel currently being
/// probed. Any other global/upvalue access must fail rather than read a stub.
pub(in crate::lua) fn infer_config_or_panel_constant_draw(
    lua: &Lua,
    function: &Function,
    panel: bool,
) -> Option<String> {
    if function.info().what != "Lua" {
        return None;
    }
    let values = lua.create_table().ok()?;
    if let Ok(config) = lua.globals().get::<Table>("skin_config") {
        let options = config.get::<Table>("option").ok()?;
        let copied = lua.create_table().ok()?;
        for pair in options.pairs::<String, i32>() {
            let (key, value) = pair.ok()?;
            copied.raw_set(key, value).ok()?;
        }
        let config = lua.create_table().ok()?;
        config.raw_set("option", readonly_probe_table(lua, copied, false)?).ok()?;
        values.raw_set("skin_config", readonly_probe_table(lua, config, true)?).ok()?;
    }
    if panel {
        values.raw_set("Expand_op", lua.globals().raw_get::<Value>("Expand_op").ok()?).ok()?;
    }
    let copied = clone_constant_probe_function(
        lua,
        function,
        &readonly_probe_table(lua, values, true)?,
        panel,
        0,
    )?;
    match copied.call::<Value>(()).ok()? {
        Value::Boolean(true) => Some("number(0) >= 0".into()),
        Value::Boolean(false) => Some("number(0) < 0".into()),
        _ => None,
    }
}

fn readonly_probe_table(lua: &Lua, values: Table, reject_missing: bool) -> Option<Table> {
    let table = lua.create_table().ok()?;
    let meta = lua.create_table().ok()?;
    meta.set(
        "__index",
        lua.create_function(move |_, (_, key): (Value, Value)| {
            let value = values.raw_get::<Value>(key)?;
            if reject_missing && matches!(value, Value::Nil) {
                Err(mlua::Error::runtime("dynamic state in constant probe"))
            } else {
                Ok(value)
            }
        })
        .ok()?,
    )
    .ok()?;
    meta.set(
        "__newindex",
        lua.create_function(|_, _: mlua::MultiValue| -> mlua::Result<()> {
            Err(mlua::Error::runtime("state mutation in constant probe"))
        })
        .ok()?,
    )
    .ok()?;
    table.set_metatable(Some(meta));
    Some(table)
}

fn clone_constant_probe_function(
    lua: &Lua,
    function: &Function,
    environment: &Table,
    panel: bool,
    depth: usize,
) -> Option<Function> {
    if depth >= 8 || function.info().what != "Lua" {
        return None;
    }
    let copied = lua.load(function.dump(false)).into_function().ok()?;
    // SAFETY: helpers only read/set the specified upvalue in their call frame.
    let get = unsafe { lua.create_c_function(get_probe_upvalue).ok()? };
    let set = unsafe { lua.create_c_function(set_probe_upvalue).ok()? };
    for index in 1..=255 {
        let (name, value) = get.call::<(Option<String>, Value)>((function.clone(), index)).ok()?;
        let Some(name) = name else {
            break;
        };
        let value = match (name.as_str(), value) {
            ("_ENV", _) => Value::Table(environment.clone()),
            ("result_mode", Value::Integer(value)) if panel => Value::Integer(value),
            (_, Value::Function(helper)) => Value::Function(clone_constant_probe_function(
                lua,
                &helper,
                environment,
                panel,
                depth + 1,
            )?),
            _ => return None,
        };
        if !set.call::<bool>((copied.clone(), index, value)).ok()? {
            return None;
        }
    }
    Some(copied)
}

unsafe extern "C-unwind" fn get_probe_upvalue(state: *mut mlua::lua_State) -> std::ffi::c_int {
    // SAFETY: caller supplies (Function, bounded integer). Return (name, value).
    unsafe {
        let index = mlua::ffi::lua_tointeger(state, 2) as std::ffi::c_int;
        let name = mlua::ffi::lua_getupvalue(state, 1, index);
        if name.is_null() {
            mlua::ffi::lua_pushnil(state);
            mlua::ffi::lua_pushnil(state);
        } else {
            mlua::ffi::lua_pushstring(state, name);
            mlua::ffi::lua_insert(state, -2);
        }
        2
    }
}

unsafe extern "C-unwind" fn set_probe_upvalue(state: *mut mlua::lua_State) -> std::ffi::c_int {
    // SAFETY: caller supplies (Function, bounded integer, Value).
    unsafe {
        let index = mlua::ffi::lua_tointeger(state, 2) as std::ffi::c_int;
        mlua::ffi::lua_pushvalue(state, 3);
        let name = mlua::ffi::lua_setupvalue(state, 1, index);
        mlua::ffi::lua_pushboolean(state, i32::from(!name.is_null()));
        1
    }
}

pub(in crate::lua) fn infer_constant_integer_at_load(
    function: &Function,
    _main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i64> {
    // `act` is an input callback. Calling it in the skin's live Lua environment
    // can mutate globals used by later draw conversion (WMII switches Expand_op
    // from GRAPH to IR this way). Evaluate serializable constant callbacks in an
    // isolated Lua state so conversion has no observable side effects.
    let isolated = Lua::new();
    let dumped = function.dump(true);
    let isolated_function = isolated.load(&dumped).into_function().ok()?;
    match isolated_function.call::<Value>(()).ok()? {
        Value::Integer(value) => Some(value),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => Some(value as i64),
        _ => None,
    }
}

pub(in crate::lua) fn infer_result_panel_act_at_load(
    lua: &Lua,
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i64> {
    if let Some(current) =
        lua.globals().raw_get::<Value>("Expand_op").ok().and_then(lua_result_panel_value)
    {
        // WMII の tab callback は `Expand_op = 1/2` だけを行う。元の Lua state を
        // 実行時まで保持せず、隔離 state で代入先を観測して BMZ 内部 event に変換する。
        let isolated = Lua::new();
        isolated.globals().raw_set("Expand_op", current).ok()?;
        let dumped = function.dump(true);
        let isolated_function = isolated.load(&dumped).into_function().ok()?;
        if !matches!(isolated_function.call::<Value>(()).ok()?, Value::Nil) {
            return None;
        }
        let panel = isolated.globals().raw_get::<Value>("Expand_op").ok()?;
        return result_panel_event(lua_result_panel_value(panel)?);
    }

    // Luxe Flat keeps the active tab in a local closure upvalue instead of the
    // global used by WMII. Preserve upvalue names in the dumped callback, seed
    // its isolated copy, and observe only the resulting `result_mode` value.
    let (upvalue_index, current_mode) = lua_result_mode_upvalue(lua, function)?;
    record_local_result_panel_default(main_state_probe, current_mode)?;
    let isolated = Lua::new();
    let dumped = function.dump(false);
    let isolated_function = isolated.load(&dumped).into_function().ok()?;
    if !set_lua_integer_upvalue(&isolated, &isolated_function, upvalue_index, current_mode)
        || !matches!(isolated_function.call::<Value>(()).ok()?, Value::Nil)
    {
        return None;
    }
    let (_, mode) = lua_result_mode_upvalue(&isolated, &isolated_function)?;
    result_panel_event(result_panel_from_local_mode(mode)?)
}

pub(in crate::lua) fn result_panel_event(panel: i32) -> Option<i64> {
    match panel {
        1 => Some(i64::from(SKIN_EVENT_RESULT_PANEL_IR)),
        2 => Some(i64::from(SKIN_EVENT_RESULT_PANEL_GRAPH)),
        _ => None,
    }
}

pub(in crate::lua) fn collect_number_refs(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<i32>> {
    let mut calls = Vec::new();
    // Lua の `or` / `and` 短絡評価で片方の number() だけ呼ばれることがあるため、
    // 複数の probe 値で実行して ref を集める。
    for default_value in [5, 0, -1] {
        {
            main_state_probe.lock().ok()?.begin_number_call_recording(default_value);
        }
        let _ = function.call::<Value>(()).ok();
        {
            let mut probe = main_state_probe.lock().ok()?;
            calls.extend(probe.number_calls.iter().copied());
            probe.end_recording();
        }
    }
    calls.sort_unstable();
    calls.dedup();
    Some(calls)
}

pub(in crate::lua) fn collect_number_refs_with_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    option_id: i32,
) -> Option<Vec<i32>> {
    collect_number_refs_with_option_value(function, main_state_probe, option_id, true)
}

pub(in crate::lua) fn collect_number_refs_with_option_value(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    option_id: i32,
    option_value: bool,
) -> Option<Vec<i32>> {
    let mut calls = Vec::new();
    for default_value in [5, 0, -1] {
        {
            main_state_probe.lock().ok()?.begin_number_call_recording_with_option_value(
                default_value,
                option_id,
                option_value,
            );
        }
        let _ = function.call::<Value>(()).ok();
        {
            let mut probe = main_state_probe.lock().ok()?;
            calls.extend(probe.number_calls.iter().copied());
            probe.end_recording();
        }
    }
    calls.sort_unstable();
    calls.dedup();
    Some(calls)
}

pub(in crate::lua) fn call_draw_with_numbers(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values(values);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

pub(in crate::lua) fn call_draw_with_numbers_and_timers(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
    timers: BTreeMap<i32, i32>,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_timer_recording_with_values(values, timers);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        Value::Nil => Some(false),
        _ => None,
    }
}

pub(in crate::lua) fn call_draw_with_number_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    number_ref: i32,
    number_value: i32,
    option_id: i32,
    option_value: bool,
) -> Option<bool> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values_and_options(
            BTreeMap::from([(number_ref, number_value)]),
            BTreeMap::from([(option_id, option_value)]),
        );
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

pub(in crate::lua) fn call_number_float_with_values(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
) -> Option<f64> {
    call_number_float_raw_with_values(function, main_state_probe, values)
        .filter(|value| value.is_finite())
}

pub(in crate::lua) fn call_number_float_raw_with_values(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
) -> Option<f64> {
    {
        main_state_probe.lock().ok()?.begin_number_recording_with_values(values);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Integer(value) => Some(value as f64),
        Value::Number(value) => Some(value),
        _ => None,
    }
}

pub(in crate::lua) fn call_number_float_with_values_and_options(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
    options: BTreeMap<i32, bool>,
) -> Option<f64> {
    {
        main_state_probe
            .lock()
            .ok()?
            .begin_number_recording_with_values_and_options(values, options);
    }
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Integer(value) => Some(value as f64),
        Value::Number(value) if value.is_finite() => Some(value),
        _ => None,
    }
}

pub(in crate::lua) fn call_draw_with_numbers_and_options(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    values: BTreeMap<i32, i32>,
    options: BTreeMap<i32, bool>,
) -> Option<bool> {
    main_state_probe.lock().ok()?.begin_number_recording_with_values_and_options(values, options);
    let result = function.call::<Value>(()).ok();
    main_state_probe.lock().ok()?.end_recording();
    match result? {
        Value::Boolean(value) => Some(value),
        Value::Nil => Some(false),
        _ => None,
    }
}

pub(in crate::lua) fn verify_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    refs: &[i32],
    expected: impl Fn(&BTreeMap<i32, i32>) -> bool,
) -> bool {
    // Keep consecutive values through one past the largest threshold inferred
    // by `infer_two_number_compare_and`. Without 4/6, an always-false draw can
    // spuriously match `left > right and right >= 4/5` because the verifier has
    // no sampled pair that can satisfy those predicates.
    let samples = [-1, 0, 1, 2, 3, 4, 5, 6];
    for &left in &samples {
        for &right in &samples {
            let mut values = BTreeMap::new();
            if refs.len() == 1 {
                values.insert(refs[0], left);
            } else if refs.len() >= 2 {
                values.insert(refs[0], left);
                values.insert(refs[1], right);
                for extra in refs.iter().skip(2) {
                    values.insert(*extra, 0);
                }
            }
            let Some(got) = call_draw_with_numbers(function, main_state_probe, values.clone())
            else {
                return false;
            };
            if got != expected(&values) {
                return false;
            }
        }
    }
    true
}
