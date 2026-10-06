use super::*;

pub(super) const LUA_TEXT_REF_SENTINEL_PREFIX: &str = "__BMZ_TEXT_REF_";
pub(super) const LUA_TEXT_REF_SENTINEL_SUFFIX: &str = "__";

pub(super) fn lua_property_id(
    lua: &Lua,
    value: &Value,
    family: PropertyFamily,
) -> mlua::Result<Option<i32>> {
    // Preserve mlua's existing numeric and numeric-string argument conversions.
    match <i32 as mlua::FromLua>::from_lua(value.clone(), lua) {
        Ok(id) => Ok(Some(id)),
        Err(error) => match value {
            Value::String(name) => Ok(resolve_property_name(family, &name.to_str()?)),
            _ => Err(error),
        },
    }
}

pub(super) fn lua_boolean_property(
    lua: &Lua,
    value: &Value,
) -> mlua::Result<Option<ResolvedBooleanProperty>> {
    match <i32 as mlua::FromLua>::from_lua(value.clone(), lua) {
        Ok(id) => Ok(Some(ResolvedBooleanProperty { id, negated: false })),
        Err(error) => match value {
            Value::String(name) => Ok(resolve_boolean_property_name(&name.to_str()?)),
            _ => Err(error),
        },
    }
}

pub(super) fn lua_runtime_stub_number(ref_id: i32) -> i32 {
    let now = unix_seconds_to_utc_datetime(lua_os_now_seconds());
    match ref_id {
        // beatoraja IntegerProperty: currenttime_year/month/day
        21 => now.year,
        22 => now.month as i32,
        23 => now.day as i32,
        _ => 0,
    }
}

pub(super) fn create_main_state_stub(
    lua: &Lua,
    probe: Arc<Mutex<MainStateProbe>>,
) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set("timer_off_value", i32::MIN)?;
    table.set(
        "set_timer",
        lua.create_function(|_, _: (i32, i64)| {
            Err::<bool, _>(mlua::Error::runtime("timer writes require runtime evaluation"))
        })?,
    )?;
    for field in
        ["total_play_counts_in_session", "total_play_notes_in_session", "score_date_sec_time"]
    {
        let probe = probe.clone();
        table.set(
            field,
            lua.create_function(move |_, ()| {
                let probe = probe
                    .lock()
                    .map_err(|_| mlua::Error::runtime("main_state probe lock poisoned"))?;
                if probe.inferring {
                    return Err(mlua::Error::runtime(
                        "session/score state requires runtime evaluation",
                    ));
                }
                // No scene snapshot exists during initial Lua execution.
                mark_load_dependency_opaque(probe.load_dependencies.as_ref());
                Ok(0_i64)
            })?,
        )?;
    }
    let probe_for_number = probe.clone();
    table.set(
        "number",
        lua.create_function(move |lua, argument: Value| {
            let Some(ref_id) = lua_property_id(lua, &argument, PropertyFamily::Integer)? else {
                return Ok(0);
            };
            Ok(probe_for_number
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .number(ref_id))
        })?,
    )?;
    let probe_for_exscore = probe.clone();
    table.set(
        "exscore",
        lua.create_function(move |_, ()| {
            Ok(probe_for_exscore
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .number(71))
        })?,
    )?;
    let probe_for_judge = probe.clone();
    table.set(
        "judge",
        lua.create_function(move |_, index: i32| {
            Ok(probe_for_judge
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .judge(index))
        })?,
    )?;
    let probe_for_option = probe.clone();
    let probe_for_timer = probe.clone();
    table.set(
        "option",
        lua.create_function(move |lua, argument: Value| {
            let Some(property) = lua_boolean_property(lua, &argument)? else {
                return Ok(false);
            };
            Ok(probe_for_option
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .option(property.id)
                ^ property.negated)
        })?,
    )?;
    let probe_for_text = probe.clone();
    table.set(
        "text",
        lua.create_function(move |lua, argument: Value| {
            let Some(ref_id) = lua_property_id(lua, &argument, PropertyFamily::String)? else {
                return Ok(String::new());
            };
            Ok(probe_for_text
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .text(ref_id))
        })?,
    )?;
    let probe_for_offset = probe.clone();
    table.set(
        "offset",
        lua.create_function(move |lua, offset_id: i32| {
            let value = probe_for_offset
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .offset(offset_id);
            create_main_state_offset_table(lua, value)
        })?,
    )?;
    let probe_for_float_number = probe.clone();
    table.set(
        "float_number",
        lua.create_function(move |lua, argument: Value| {
            let Some(ref_id) = lua_property_id(lua, &argument, PropertyFamily::Float)? else {
                return Ok(0.0);
            };
            Ok(probe_for_float_number
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .float_number(ref_id))
        })?,
    )?;
    let probe_for_event_index = probe.clone();
    table.set(
        "event_index",
        lua.create_function(move |_, event_id: i32| {
            Ok(probe_for_event_index
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .event_index(event_id))
        })?,
    )?;
    table.set(
        "timer",
        lua.create_function(move |_, timer_id: i32| {
            Ok(probe_for_timer
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .timer(timer_id))
        })?,
    )?;
    let probe_for_time = probe.clone();
    table.set(
        "time",
        lua.create_function(move |_, ()| {
            Ok(probe_for_time
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .time())
        })?,
    )?;
    let probe_for_gauge_type = probe.clone();
    table.set(
        "gauge_type",
        lua.create_function(move |_, ()| {
            Ok(probe_for_gauge_type
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .gauge_type())
        })?,
    )?;
    let probe_for_volume_sys = probe.clone();
    table.set(
        "volume_sys",
        lua.create_function(move |_, ()| {
            Ok(probe_for_volume_sys
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .volume_number(57))
        })?,
    )?;
    let probe_for_volume_key = probe.clone();
    table.set(
        "volume_key",
        lua.create_function(move |_, ()| {
            Ok(probe_for_volume_key
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .volume_number(58))
        })?,
    )?;
    let probe_for_volume_bg = probe.clone();
    table.set(
        "volume_bg",
        lua.create_function(move |_, ()| {
            Ok(probe_for_volume_bg
                .lock()
                .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                .volume_number(59))
        })?,
    )?;
    table.set("set_volume_sys", lua.create_function(|_, _: Value| Ok(true))?)?;
    table.set("set_volume_key", lua.create_function(|_, _: Value| Ok(true))?)?;
    table.set("set_volume_bg", lua.create_function(|_, _: Value| Ok(true))?)?;
    let probe_for_audio_play = probe.clone();
    table.set(
        "audio_play",
        lua.create_function(move |_, (path, volume): (Value, Value)| {
            if let Some((path, volume)) = lua_audio_path_and_volume(path, volume) {
                probe_for_audio_play
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                    .record_audio_action(LuaAudioActionKindProbe::Play, path, volume);
            }
            Ok(true)
        })?,
    )?;
    let probe_for_audio_loop = probe.clone();
    table.set(
        "audio_loop",
        lua.create_function(move |_, (path, volume): (Value, Value)| {
            if let Some((path, volume)) = lua_audio_path_and_volume(path, volume) {
                probe_for_audio_loop
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                    .record_audio_action(LuaAudioActionKindProbe::Loop, path, volume);
            }
            Ok(true)
        })?,
    )?;
    table.set(
        "audio_stop",
        lua.create_function(move |_, path: Value| {
            if let Value::String(path) = path
                && let Ok(path) = path.to_str()
            {
                probe
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                    .record_audio_action(LuaAudioActionKindProbe::Stop, path.to_string(), 1.0);
            }
            Ok(true)
        })?,
    )?;
    install_main_state_read_helpers(lua, &table)?;
    Ok(Value::Table(table))
}

/// Capture the current accessors, rather than a snapshot of their values. The
/// runtime VM installs these again after its stable dispatchers are available.
pub(super) fn install_main_state_read_helpers(lua: &Lua, table: &Table) -> mlua::Result<()> {
    let number: Function = table.get("number")?;
    table.set(
        "numbers",
        lua.create_function(move |_, arguments: Variadic<Value>| {
            arguments
                .into_iter()
                .map(|argument| number.call::<Value>(argument))
                .collect::<mlua::Result<Variadic<Value>>>()
        })?,
    )?;
    let timer: Function = table.get("timer")?;
    let time: Function = table.get("time")?;
    for (field, expected_on) in [("timer_is_on", true), ("timer_is_off", false)] {
        let timer = timer.clone();
        table.set(
            field,
            lua.create_function(move |_, id: i32| {
                let start = timer.call::<i64>(id)?;
                Ok((start != i64::from(TIMER_OFF_VALUE)) == expected_on)
            })?,
        )?;
    }
    for (field, divisor) in [
        ("timer_elapsed", Some(1)),
        ("timer_elapsed_ms", Some(1_000)),
        ("timer_elapsed_seconds", None),
    ] {
        let timer = timer.clone();
        let time = time.clone();
        table.set(
            field,
            lua.create_function(move |_, id: i32| {
                let start = timer.call::<i64>(id)?;
                if start == i64::from(TIMER_OFF_VALUE) {
                    return Ok(Value::Integer(-1));
                }
                // Match Java long subtraction and division, including future
                // start times; do not clamp negative elapsed values to zero.
                let elapsed = time.call::<i64>(())?.wrapping_sub(start);
                Ok(match divisor {
                    Some(divisor) => Value::Integer(elapsed / divisor),
                    None => Value::Number(elapsed as f64 / 1_000_000.0),
                })
            })?,
        )?;
    }
    Ok(())
}

pub(super) fn lua_audio_path_and_volume(path: Value, volume: Value) -> Option<(String, f64)> {
    let Value::String(path) = path else { return None };
    let volume = match volume {
        Value::Integer(volume) => volume as f64,
        Value::Number(volume) if volume.is_finite() => volume,
        _ => return None,
    };
    Some((path.to_str().ok()?.to_string(), volume))
}

pub(super) fn create_main_state_offset_table(
    lua: &Lua,
    offset: LuaSkinOffsetValue,
) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set("x", offset.x)?;
    table.set("y", offset.y)?;
    table.set("w", offset.w)?;
    table.set("h", offset.h)?;
    table.set("r", offset.r)?;
    table.set("a", offset.a)?;
    Ok(Value::Table(table))
}
