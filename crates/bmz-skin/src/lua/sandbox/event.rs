use super::*;

const LUA_LOAD_GRAPHICS_WIDTH: i32 = 1920;
const LUA_LOAD_GRAPHICS_HEIGHT: i32 = 1080;

#[derive(Debug)]
pub(super) struct EventObserveBoolState {
    is_on: bool,
}

#[derive(Debug)]
pub(super) struct EventObserveTimerState {
    value: i32,
}

#[derive(Debug)]
pub(super) struct EventMinIntervalState {
    last_execution_ms: Option<i32>,
}

/// beatoraja の `EventUtility` 相当。CustomEvent 用 callback 生成器を提供する。
pub(super) fn create_event_util_module(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;

    table.set(
        "event_observe_turn_true",
        lua.create_function(|lua, (observed, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveBoolState { is_on: false }));
            lua.create_function(move |_, ()| {
                let on = observed.call::<bool>(())?;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event observe lock poisoned"))?;
                if state.is_on != on {
                    state.is_on = on;
                    if state.is_on {
                        action.call::<()>(())?;
                    }
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_observe_timer",
        lua.create_function(|lua, (timer, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveTimerState { value: TIMER_OFF_VALUE }));
            lua.create_function(move |_, ()| {
                let value = timer.call::<i32>(())?;
                let mut state =
                    state.lock().map_err(|_| mlua::Error::external("event timer lock poisoned"))?;
                if value != state.value && value != TIMER_OFF_VALUE {
                    state.value = value;
                    action.call::<()>(())?;
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_observe_timer_on",
        lua.create_function(|lua, (timer, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveBoolState { is_on: false }));
            lua.create_function(move |_, ()| {
                let on = timer.call::<i32>(())? != TIMER_OFF_VALUE;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event timer-on lock poisoned"))?;
                if state.is_on != on {
                    state.is_on = on;
                    if state.is_on {
                        action.call::<()>(())?;
                    }
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_observe_timer_off",
        lua.create_function(|lua, (timer, action): (Function, Function)| {
            let state = Arc::new(Mutex::new(EventObserveBoolState { is_on: true }));
            lua.create_function(move |_, ()| {
                let off = timer.call::<i32>(())? == TIMER_OFF_VALUE;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event timer-off lock poisoned"))?;
                if state.is_on != off {
                    state.is_on = off;
                    if state.is_on {
                        action.call::<()>(())?;
                    }
                }
                Ok(true)
            })
        })?,
    )?;

    table.set(
        "event_min_interval",
        lua.create_function(|lua, (min_interval_ms, action): (i32, Function)| {
            let state = Arc::new(Mutex::new(EventMinIntervalState { last_execution_ms: None }));
            lua.create_function(move |_, ()| {
                let now = lua_load_now_ms();
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("event interval lock poisoned"))?;
                let should_run = state
                    .last_execution_ms
                    .is_none_or(|last| now.saturating_sub(last) >= min_interval_ms);
                if should_run {
                    state.last_execution_ms = Some(now);
                    action.call::<()>(())?;
                }
                Ok(true)
            })
        })?,
    )?;

    Ok(Value::Table(table))
}

pub(super) fn create_luajava_stub(lua: &Lua) -> mlua::Result<Value> {
    let table = lua.create_table()?;
    table.set(
        "bindClass",
        lua.create_function(|lua, class_name: String| match class_name.as_str() {
            "com.badlogic.gdx.Gdx" => create_luajava_gdx_stub(lua),
            "com.badlogic.gdx.controllers.Controllers" => create_luajava_controllers_stub(lua),
            _ => create_luajava_object_stub(lua),
        })?,
    )?;
    table.set(
        "newInstance",
        lua.create_function(|lua, (_class_name, _args): (String, Variadic<Value>)| {
            create_luajava_object_stub(lua)
        })?,
    )?;
    table.set(
        "createProxy",
        lua.create_function(|lua, _: Variadic<Value>| create_luajava_object_stub(lua))?,
    )?;
    Ok(Value::Table(table))
}

pub(super) fn create_luajava_gdx_stub(lua: &Lua) -> mlua::Result<Value> {
    let gdx = lua.create_table()?;
    let input = lua.create_table()?;
    input
        .set("isKeyPressed", lua.create_function(|_, (_self, _key): (Value, Value)| Ok(false))?)?;
    gdx.set("input", input)?;
    let graphics = lua.create_table()?;
    graphics
        .set("getWidth", lua.create_function(|_, _self: Value| Ok(LUA_LOAD_GRAPHICS_WIDTH))?)?;
    graphics
        .set("getHeight", lua.create_function(|_, _self: Value| Ok(LUA_LOAD_GRAPHICS_HEIGHT))?)?;
    gdx.set("graphics", graphics)?;
    Ok(Value::Table(gdx))
}

pub(super) fn create_luajava_controllers_stub(lua: &Lua) -> mlua::Result<Value> {
    let controllers = lua.create_table()?;
    controllers.set(
        "getControllers",
        lua.create_function(|lua, _self: Value| {
            let list = lua.create_table()?;
            // libGDX Array exposes `size` as a numeric field. Returning an
            // empty list is the neutral load-time value and prevents skins
            // from treating the generic truthy object stub as live input.
            list.set("size", 0)?;
            list.set(
                "get",
                lua.create_function(|_, (_self, _index): (Value, Value)| Ok(Value::Nil))?,
            )?;
            list.set("first", lua.create_function(|_, _self: Value| Ok(Value::Nil))?)?;
            Ok(Value::Table(list))
        })?,
    )?;
    Ok(Value::Table(controllers))
}

pub(super) fn create_luajava_object_stub(lua: &Lua) -> mlua::Result<Value> {
    let object = lua.create_table()?;
    let metatable = lua.create_table()?;
    metatable.set(
        "__index",
        lua.create_function(|lua, (_table, _key): (Value, Value)| create_luajava_object_stub(lua))?,
    )?;
    metatable.set(
        "__call",
        lua.create_function(|lua, (_self, _args): (Value, Variadic<Value>)| {
            create_luajava_object_stub(lua)
        })?,
    )?;
    object.set_metatable(Some(metatable));
    Ok(Value::Table(object))
}

/// beatoraja の `TimerUtility` 相当。Lua スキンが `require("timer_util")` できるようにする。
pub(super) fn create_timer_util_module(
    lua: &Lua,
    probe: Arc<Mutex<MainStateProbe>>,
) -> mlua::Result<Value> {
    let table = lua.create_table()?;

    table.set(
        "now_timer",
        lua.create_function(|_, timer_value: i32| {
            Ok(if timer_value != TIMER_OFF_VALUE {
                lua_load_now_micros().saturating_sub(timer_value.max(0))
            } else {
                0
            })
        })?,
    )?;
    table.set(
        "is_timer_on",
        lua.create_function(|_, timer_value: i32| Ok(timer_value != TIMER_OFF_VALUE))?,
    )?;
    table.set(
        "is_timer_off",
        lua.create_function(|_, timer_value: i32| Ok(timer_value == TIMER_OFF_VALUE))?,
    )?;

    let probe_for_timer_function = probe.clone();
    table.set(
        "timer_function",
        lua.create_function(move |lua, timer_id: i32| {
            let probe = probe_for_timer_function.clone();
            lua.create_function(move |_, _: Value| {
                Ok(probe
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?
                    .timer(timer_id))
            })
        })?,
    )?;

    let probe_for_observe = probe.clone();
    table.set(
        "timer_observe_boolean",
        lua.create_function(move |lua, observed: Function| {
            let specialized = infer_is_gauge_iidx_global_observe(lua, &observed);
            let observe = specialized
                .clone()
                .or_else(|| infer_runtime_boolean_field_observe(lua, &observed, &probe_for_observe))
                .or_else(|| infer_boolean_predicate(lua, &observed, &probe_for_observe, None));
            let unsupported = observe.is_none();
            let load_time_constant = specialized.is_none()
                && observe.as_deref().is_some_and(is_constant_boolean_condition);
            let observe = observe.unwrap_or_else(|| "number(0) < 0".to_string());
            let timer_id = {
                let mut probe = probe_for_observe
                    .lock()
                    .map_err(|_| mlua::Error::external("main_state probe lock poisoned"))?;
                if !unsupported
                    && let Some(timer_id) =
                        probe.dynamic_timer_ids_by_observe.get(&observe).copied()
                {
                    timer_id
                } else {
                    let timer_id = probe.next_dynamic_timer_id;
                    probe.next_dynamic_timer_id += 1;
                    probe.dynamic_timers.push((timer_id, observe.clone()));
                    if !unsupported {
                        probe.dynamic_timer_ids_by_observe.insert(observe, timer_id);
                    }
                    if unsupported {
                        probe.unsupported_dynamic_timers.push(timer_id);
                    }
                    if load_time_constant {
                        probe.load_time_constant_dynamic_timers.push(timer_id);
                    }
                    timer_id
                }
            };
            let state = Arc::new(Mutex::new(TimerObserveState { timer_value: TIMER_OFF_VALUE }));
            let observed_for_timer = observed.clone();
            let inner = lua.create_function(move |_, ()| {
                let on = observed_for_timer.call::<bool>(())?;
                let mut state = state
                    .lock()
                    .map_err(|_| mlua::Error::external("timer observe lock poisoned"))?;
                if on && state.timer_value == TIMER_OFF_VALUE {
                    state.timer_value = lua_load_now_ms();
                } else if !on && state.timer_value != TIMER_OFF_VALUE {
                    state.timer_value = TIMER_OFF_VALUE;
                }
                Ok(state.timer_value)
            })?;
            let map: Table = lua.globals().get("bmz_timer_fn_map")?;
            map.set(inner.clone(), timer_id)?;
            Ok(inner)
        })?,
    )?;

    table.set(
        "new_passive_timer",
        lua.create_function(|lua, ()| {
            let state = Arc::new(Mutex::new(TimerObserveState { timer_value: TIMER_OFF_VALUE }));
            let passive = lua.create_table()?;
            let state_for_timer = state.clone();
            passive.set(
                "timer",
                lua.create_function(move |_, ()| {
                    Ok(state_for_timer
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?
                        .timer_value)
                })?,
            )?;
            let state_for_turn_on = state.clone();
            passive.set(
                "turn_on",
                lua.create_function(move |_, ()| {
                    let mut state = state_for_turn_on
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?;
                    if state.timer_value == TIMER_OFF_VALUE {
                        state.timer_value = lua_load_now_micros();
                    }
                    Ok(())
                })?,
            )?;
            let state_for_turn_on_reset = state.clone();
            passive.set(
                "turn_on_reset",
                lua.create_function(move |_, ()| {
                    state_for_turn_on_reset
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?
                        .timer_value = lua_load_now_micros();
                    Ok(())
                })?,
            )?;
            passive.set(
                "turn_off",
                lua.create_function(move |_, ()| {
                    state
                        .lock()
                        .map_err(|_| mlua::Error::external("passive timer lock poisoned"))?
                        .timer_value = TIMER_OFF_VALUE;
                    Ok(())
                })?,
            )?;
            Ok(passive)
        })?,
    )?;

    Ok(Value::Table(table))
}
