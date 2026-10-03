pub(super) fn normalize_lua_skin_audio_paths(
    path_context: &SkinPathContext,
    root: &mut JsonMap<String, JsonValue>,
    warnings: &mut Vec<String>,
) {
    if let Some(JsonValue::Array(actions)) = root.get_mut("sceneAudio") {
        normalize_lua_skin_audio_action_array(path_context, actions, warnings);
    }
    if let Some(JsonValue::Array(events)) = root.get_mut("customEvents") {
        for event in events {
            let JsonValue::Object(event) = event else { continue };
            if let Some(JsonValue::Array(actions)) = event.get_mut("audioActions") {
                normalize_lua_skin_audio_action_array(path_context, actions, warnings);
            }
        }
    }
}

pub(super) fn normalize_lua_skin_audio_action_array(
    path_context: &SkinPathContext,
    actions: &mut Vec<JsonValue>,
    warnings: &mut Vec<String>,
) {
    actions.retain_mut(|action| {
        let JsonValue::Object(action) = action else { return false };
        let Some(JsonValue::String(path)) = action.get_mut("path") else { return false };
        let requested = path.clone();
        let Ok(candidate) = path_context.resolve_file(&requested) else {
            warnings.push(format!("skipping missing skin audio path: {requested}"));
            return false;
        };
        *path = candidate
            .strip_prefix(path_context.entry_dir())
            .unwrap_or(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        true
    });
}

pub(super) fn lua_result_panel_value(value: Value) -> Option<i32> {
    match value {
        Value::Integer(value) => i32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() && value.fract() == 0.0 => Some(value as i32),
        _ => None,
    }
    .filter(|panel| (0..=2).contains(panel))
}

pub(super) fn result_panel_from_local_mode(mode: i32) -> Option<i32> {
    match mode {
        0 => Some(2),
        1 => Some(1),
        _ => None,
    }
}

pub(super) fn record_local_result_panel_default(
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    mode: i32,
) -> Option<()> {
    let panel = result_panel_from_local_mode(mode)?;
    let mut probe = main_state_probe.lock().ok()?;
    probe.result_panel_default.get_or_insert(panel);
    Some(())
}

/// Returns the index, integer value and identity of an upvalue named `result_mode`.
///
/// Lua 5.4 does not expose arbitrary upvalues through mlua's safe API. This
/// private C callback only inspects the function passed as argument 1 and never
/// installs the debug library into the skin sandbox.
unsafe extern "C-unwind" fn find_result_mode_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: mlua invokes this callback with a live Lua state. Every inspected
    // stack slot belongs to this call, and `lua_getupvalue` pushes exactly one
    // value whenever it returns a non-null name.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION {
            return 0;
        }
        for index in 1..=255 {
            let name = mlua::ffi::lua_getupvalue(state, 1, index);
            if name.is_null() {
                break;
            }
            let matches = CStr::from_ptr(name).to_bytes() == b"result_mode";
            if matches && mlua::ffi::lua_isinteger(state, -1) != 0 {
                let value = mlua::ffi::lua_tointeger(state, -1);
                mlua::ffi::lua_pop(state, 1);
                mlua::ffi::lua_pushinteger(state, i64::from(index));
                mlua::ffi::lua_pushinteger(state, value);
                mlua::ffi::lua_pushlightuserdata(state, mlua::ffi::lua_upvalueid(state, 1, index));
                return 3;
            }
            mlua::ffi::lua_pop(state, 1);
        }
        0
    }
}

/// Returns the index and boolean value of a closure upvalue named `flag_score`.
///
/// mz-select keeps its score-availability guard local to the player-data
/// module, while Luxe Flat exposes the same guard as a global. Inspecting the
/// closure lets both original skins produce the same runtime draw predicate.
unsafe extern "C-unwind" fn find_flag_score_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: see `find_result_mode_upvalue`; this callback only inspects the
    // function passed in stack slot 1 and balances every pushed upvalue.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION {
            return 0;
        }
        for index in 1..=255 {
            let name = mlua::ffi::lua_getupvalue(state, 1, index);
            if name.is_null() {
                break;
            }
            let matches = CStr::from_ptr(name).to_bytes() == b"flag_score";
            if matches && mlua::ffi::lua_type(state, -1) == mlua::ffi::LUA_TBOOLEAN {
                let value = mlua::ffi::lua_toboolean(state, -1);
                mlua::ffi::lua_pop(state, 1);
                mlua::ffi::lua_pushinteger(state, i64::from(index));
                mlua::ffi::lua_pushboolean(state, value);
                return 2;
            }
            mlua::ffi::lua_pop(state, 1);
        }
        0
    }
}

/// Returns the index and numeric value of a closure upvalue named `scorerate`.
unsafe extern "C-unwind" fn find_scorerate_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: see `find_result_mode_upvalue`; this callback only inspects the
    // function passed in stack slot 1 and balances every pushed upvalue.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION {
            return 0;
        }
        for index in 1..=255 {
            let name = mlua::ffi::lua_getupvalue(state, 1, index);
            if name.is_null() {
                break;
            }
            let matches = CStr::from_ptr(name).to_bytes() == b"scorerate";
            if matches && mlua::ffi::lua_type(state, -1) == mlua::ffi::LUA_TNUMBER {
                let value = mlua::ffi::lua_tonumber(state, -1);
                mlua::ffi::lua_pop(state, 1);
                mlua::ffi::lua_pushinteger(state, i64::from(index));
                mlua::ffi::lua_pushnumber(state, value);
                return 2;
            }
            mlua::ffi::lua_pop(state, 1);
        }
        0
    }
}

/// Returns the index and value of a closure upvalue named `rank_plus`.
unsafe extern "C-unwind" fn find_rank_plus_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: see `find_result_mode_upvalue`; this callback only inspects the
    // function passed in stack slot 1 and balances every pushed upvalue.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION {
            return 0;
        }
        for index in 1..=255 {
            let name = mlua::ffi::lua_getupvalue(state, 1, index);
            if name.is_null() {
                break;
            }
            let matches = CStr::from_ptr(name).to_bytes() == b"rank_plus";
            if matches && mlua::ffi::lua_type(state, -1) == mlua::ffi::LUA_TBOOLEAN {
                let value = mlua::ffi::lua_toboolean(state, -1);
                mlua::ffi::lua_pop(state, 1);
                mlua::ffi::lua_pushinteger(state, i64::from(index));
                mlua::ffi::lua_pushboolean(state, value);
                return 2;
            }
            mlua::ffi::lua_pop(state, 1);
        }
        0
    }
}

/// Replaces one integer closure upvalue and reports whether the index existed.
unsafe extern "C-unwind" fn set_integer_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: arguments are validated before touching the stack. `lua_setupvalue`
    // consumes the pushed value and only mutates the function passed to this call.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION
            || mlua::ffi::lua_isinteger(state, 2) == 0
            || mlua::ffi::lua_isinteger(state, 3) == 0
        {
            mlua::ffi::lua_pushboolean(state, 0);
            return 1;
        }
        let index = mlua::ffi::lua_tointeger(state, 2);
        let value = mlua::ffi::lua_tointeger(state, 3);
        let Ok(index) = c_int::try_from(index) else {
            mlua::ffi::lua_pushboolean(state, 0);
            return 1;
        };
        mlua::ffi::lua_pushinteger(state, value);
        let name = mlua::ffi::lua_setupvalue(state, 1, index);
        mlua::ffi::lua_pushboolean(state, if name.is_null() { 0 } else { 1 });
        1
    }
}

/// Replaces one boolean closure upvalue and reports whether the index existed.
unsafe extern "C-unwind" fn set_boolean_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: arguments are validated before touching the stack. `lua_setupvalue`
    // consumes the pushed value and only mutates the supplied function.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION
            || mlua::ffi::lua_isinteger(state, 2) == 0
            || mlua::ffi::lua_type(state, 3) != mlua::ffi::LUA_TBOOLEAN
        {
            mlua::ffi::lua_pushboolean(state, 0);
            return 1;
        }
        let index = mlua::ffi::lua_tointeger(state, 2);
        let value = mlua::ffi::lua_toboolean(state, 3);
        let Ok(index) = c_int::try_from(index) else {
            mlua::ffi::lua_pushboolean(state, 0);
            return 1;
        };
        mlua::ffi::lua_pushboolean(state, value);
        let name = mlua::ffi::lua_setupvalue(state, 1, index);
        mlua::ffi::lua_pushboolean(state, if name.is_null() { 0 } else { 1 });
        1
    }
}

/// Replaces one numeric closure upvalue and reports whether the index existed.
unsafe extern "C-unwind" fn set_number_upvalue(state: *mut mlua::lua_State) -> c_int {
    // SAFETY: arguments are validated before touching the stack. `lua_setupvalue`
    // consumes the pushed value and only mutates the supplied function.
    unsafe {
        if mlua::ffi::lua_type(state, 1) != mlua::ffi::LUA_TFUNCTION
            || mlua::ffi::lua_isinteger(state, 2) == 0
            || mlua::ffi::lua_type(state, 3) != mlua::ffi::LUA_TNUMBER
        {
            mlua::ffi::lua_pushboolean(state, 0);
            return 1;
        }
        let index = mlua::ffi::lua_tointeger(state, 2);
        let value = mlua::ffi::lua_tonumber(state, 3);
        let Ok(index) = c_int::try_from(index) else {
            mlua::ffi::lua_pushboolean(state, 0);
            return 1;
        };
        mlua::ffi::lua_pushnumber(state, value);
        let name = mlua::ffi::lua_setupvalue(state, 1, index);
        mlua::ffi::lua_pushboolean(state, if name.is_null() { 0 } else { 1 });
        1
    }
}

pub(super) fn lua_result_mode_upvalue(lua: &Lua, function: &Function) -> Option<(i32, i32)> {
    lua_result_mode_binding(lua, function).map(|(index, value, _)| (index, value))
}

pub(super) fn lua_result_mode_binding(lua: &Lua, function: &Function) -> Option<(i32, i32, usize)> {
    // SAFETY: both callbacks obey Lua's C function ABI and access only their
    // call frame. They are retained by mlua for the duration of `call`.
    let helper = unsafe { lua.create_c_function(find_result_mode_upvalue).ok()? };
    let (index, value, identity) =
        helper.call::<(i64, i64, mlua::LightUserData)>(function.clone()).ok()?;
    Some((i32::try_from(index).ok()?, i32::try_from(value).ok()?, identity.0 as usize))
}

pub(super) fn set_lua_integer_upvalue(
    lua: &Lua,
    function: &Function,
    index: i32,
    value: i32,
) -> bool {
    // SAFETY: see `lua_result_mode_upvalue`; Rust-side argument conversion also
    // guarantees the C callback receives a function and two integers.
    let Ok(helper) = (unsafe { lua.create_c_function(set_integer_upvalue) }) else {
        return false;
    };
    helper.call::<bool>((function.clone(), index, value)).unwrap_or(false)
}

pub(super) fn lua_flag_score_upvalue(lua: &Lua, function: &Function) -> Option<(i32, bool)> {
    // SAFETY: the callback obeys Lua's C function ABI and accesses only its
    // call frame. It is retained by mlua for the duration of `call`.
    let helper = unsafe { lua.create_c_function(find_flag_score_upvalue).ok()? };
    let (index, value) = helper.call::<(i64, bool)>(function.clone()).ok()?;
    Some((i32::try_from(index).ok()?, value))
}

pub(super) fn lua_scorerate_upvalue(lua: &Lua, function: &Function) -> Option<(i32, f64)> {
    // SAFETY: the callback obeys Lua's C function ABI and accesses only its
    // call frame. It is retained by mlua for the duration of `call`.
    let helper = unsafe { lua.create_c_function(find_scorerate_upvalue).ok()? };
    let (index, value) = helper.call::<(i64, f64)>(function.clone()).ok()?;
    Some((i32::try_from(index).ok()?, value))
}

pub(super) fn lua_rank_plus_upvalue(lua: &Lua, function: &Function) -> Option<(i32, bool)> {
    // SAFETY: the callback obeys Lua's C function ABI and accesses only its
    // call frame. It is retained by mlua for the duration of `call`.
    let helper = unsafe { lua.create_c_function(find_rank_plus_upvalue).ok()? };
    let (index, value) = helper.call::<(i64, bool)>(function.clone()).ok()?;
    Some((i32::try_from(index).ok()?, value))
}

pub(super) fn set_lua_boolean_upvalue(
    lua: &Lua,
    function: &Function,
    index: i32,
    value: bool,
) -> bool {
    // SAFETY: see `lua_flag_score_upvalue`; Rust-side argument conversion
    // guarantees the callback receives a function, integer, and boolean.
    let Ok(helper) = (unsafe { lua.create_c_function(set_boolean_upvalue) }) else {
        return false;
    };
    helper.call::<bool>((function.clone(), index, value)).unwrap_or(false)
}

pub(super) fn set_lua_number_upvalue(
    lua: &Lua,
    function: &Function,
    index: i32,
    value: f64,
) -> bool {
    // SAFETY: see `lua_scorerate_upvalue`; Rust-side argument conversion
    // guarantees the callback receives a function, integer, and number.
    let Ok(helper) = (unsafe { lua.create_c_function(set_number_upvalue) }) else {
        return false;
    };
    helper.call::<bool>((function.clone(), index, value)).unwrap_or(false)
}

pub(super) fn postprocess_lua_skin_json(
    root: &mut JsonMap<String, JsonValue>,
    warnings: &mut Vec<String>,
) {
    repair_malformed_destination_ops(root, warnings);
    let repaired = repair_keybeam_destination_draws(root);
    warnings.retain(|warning| {
        !repaired.iter().any(|index| {
            warning == &format!("skipping unsupported draw function at $.destination[{index}].draw")
                || warning
                    == &format!("skipping unsupported field `timer` at $.destination[{index}]")
        })
    });
}

/// Repairs two malformed `op` table shapes accepted by Lua/beatoraja skins but
/// rejected by the strict document schema. Keep the predicates narrow so an
/// unrelated object or intentionally nested array is not silently flattened.
pub(super) fn repair_malformed_destination_ops(
    root: &mut JsonMap<String, JsonValue>,
    warnings: &mut Vec<String>,
) {
    let Some(destinations) = root.get_mut("destination").and_then(JsonValue::as_array_mut) else {
        return;
    };
    const DESTINATION_FIELDS: &[&str] = &[
        "blend",
        "filter",
        "timer",
        "timer_expr",
        "loop",
        "center",
        "offset",
        "offsets",
        "stretch",
        "draw",
        "dst",
        "mouseRect",
    ];
    let mut repaired_count = 0;

    for (index, destination) in destinations.iter_mut().enumerate() {
        let Some(destination) = destination.as_object_mut() else {
            continue;
        };
        let Some(op) = destination.remove("op") else {
            continue;
        };

        let repaired = match op {
            JsonValue::Object(mut mixed) => {
                let has_destination_marker = mixed.get("dst").is_some_and(JsonValue::is_array);
                let named_fields_are_known = mixed
                    .keys()
                    .filter(|key| key.parse::<usize>().is_err())
                    .all(|key| DESTINATION_FIELDS.contains(&key.as_str()));
                let named_fields_do_not_conflict = mixed
                    .keys()
                    .filter(|key| key.parse::<usize>().is_err())
                    .all(|key| !destination.contains_key(key));

                let mut numbered = mixed
                    .iter()
                    .filter_map(|(key, value)| {
                        key.parse::<usize>().ok().map(|position| (position, value.clone()))
                    })
                    .collect::<Vec<_>>();
                numbered.sort_by_key(|(position, _)| *position);
                let numbered_are_contiguous_i32 = !numbered.is_empty()
                    && numbered.iter().enumerate().all(|(offset, (position, value))| {
                        *position == offset + 1
                            && value.as_i64().and_then(|value| i32::try_from(value).ok()).is_some()
                    });

                if has_destination_marker
                    && named_fields_are_known
                    && named_fields_do_not_conflict
                    && numbered_are_contiguous_i32
                {
                    for key in mixed
                        .keys()
                        .filter(|key| key.parse::<usize>().is_err())
                        .cloned()
                        .collect::<Vec<_>>()
                    {
                        if let Some(value) = mixed.remove(&key) {
                            destination.insert(key, value);
                        }
                    }
                    destination.insert(
                        "op".to_string(),
                        JsonValue::Array(numbered.into_iter().map(|(_, value)| value).collect()),
                    );
                    warnings.retain(|warning| {
                        warning
                            != &format!(
                                "mixed lua table converted to object at $.destination[{}].op",
                                index + 1
                            )
                    });
                    true
                } else {
                    destination.insert("op".to_string(), JsonValue::Object(mixed));
                    false
                }
            }
            JsonValue::Array(mut outer) if outer.len() == 2 => {
                let head = outer.first().and_then(JsonValue::as_i64);
                let nested = outer.get(1).and_then(JsonValue::as_array);
                let nested_is_i32 = nested.is_some_and(|values| {
                    !values.is_empty()
                        && values.iter().all(|value| {
                            value.as_i64().and_then(|value| i32::try_from(value).ok()).is_some()
                        })
                });
                let redundant_prefix = head.is_some()
                    && nested.and_then(|values| values.first()).and_then(JsonValue::as_i64) == head;
                if nested_is_i32 && redundant_prefix {
                    destination.insert("op".to_string(), outer.swap_remove(1));
                    true
                } else {
                    destination.insert("op".to_string(), JsonValue::Array(outer));
                    false
                }
            }
            op => {
                destination.insert("op".to_string(), op);
                false
            }
        };

        if repaired {
            repaired_count += 1;
        }
    }
    if repaired_count > 0 {
        warnings.push(format!("repaired {repaired_count} malformed destination op tables"));
    }
}
use super::*;
