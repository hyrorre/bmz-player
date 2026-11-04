//! The VM that runs copies of `act` callbacks during load-time inference.
//! Its globals become the copied callback's `_ENV`, so it must expose no more
//! than the skin sandbox and must keep the instruction and memory ceilings.

use super::*;

fn skin_function(lua: &Lua, source: &str) -> Function {
    lua.load(source).eval::<Function>().unwrap()
}

#[test]
fn act_inference_vm_has_no_host_libraries_or_chunk_loaders() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    // 7 only when nothing that reaches the host is visible; never calls any of it.
    let act = skin_function(
        &lua,
        r#"return function()
            if os == nil and io == nil and package == nil and require == nil and debug == nil
                and load == nil and loadfile == nil and dofile == nil and string.dump == nil
                and luajava == nil then
                return 7
            end
            return 8
        end"#,
    );
    assert_eq!(infer_constant_integer_at_load(&act, &probe), Some(7));

    let constant = skin_function(&lua, "return function() return math.floor(42.0) end");
    assert_eq!(infer_constant_integer_at_load(&constant, &probe), Some(42));
}

#[test]
fn act_inference_vm_stops_runaway_callbacks() {
    let lua = Lua::new();
    let probe = Arc::new(Mutex::new(MainStateProbe::default()));
    let endless = skin_function(&lua, "return function() while true do end end");
    assert_eq!(infer_constant_integer_at_load(&endless, &probe), None);

    // The allocator refuses this before any large allocation happens.
    let huge =
        skin_function(&lua, "return function() return #string.rep('x', 512 * 1024 * 1024) end");
    assert_eq!(infer_constant_integer_at_load(&huge, &probe), None);
}

#[test]
fn act_and_event_scripts_are_compiled_as_text_only() {
    let lua = Lua::new();
    let script = |source: &str| Value::String(lua.create_string(source).unwrap());
    for field in ["act", "event"] {
        let error = lua_script_field_function(&lua, script("\x1bLua garbage"), field)
            .err()
            .map(|error| error.to_string());
        assert!(
            error.as_deref().is_some_and(|message| message.contains("mode is 't'")),
            "{field}: {error:?}"
        );
    }
    let act = lua_script_field_function(&lua, script("return 3"), "act").unwrap().unwrap();
    assert_eq!(act.call::<i64>(()).unwrap(), 3);
}
