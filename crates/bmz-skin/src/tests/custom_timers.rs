use super::*;

fn load(source: &str, mode: LuaSkinRuntimeMode) -> LoadedSkinDocument {
    let root = unique_test_dir("bmz-custom-timers");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lua");
    fs::write(&path, source).unwrap();
    let loaded = load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
    )
    .unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    loaded
}

#[test]
fn custom_timers_share_clean_state_and_update_passive_aliases_once_per_frame() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local s = require('main_state')
            local count = 0
            return {type=5, customTimers={
                {id=10001},
                {id=10002, timer=function()
                    count = count + 1
                    if s.option(2) then
                        if s.timer(10001) == s.timer_off_value then
                            s.set_timer(10001, s.time())
                        end
                    else s.set_timer(10001, s.timer_off_value) end
                    return s.timer(10001)
                end},
                {id=10003, timer=function() return s.timer(10002) end}
            }, value={{id='count', value=function() return count end}}}
        "#,
            mode,
        );
        assert!(loaded.dependencies.opaque);
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let counter = (0..runtime.callback_count())
            .find(|&id| runtime.callback_path(id) == Some("$.value[1].value"))
            .unwrap();
        let mut state = TestLuaMainState { now_us: 5_000_000_000, ..Default::default() };
        state.options.insert(2, true);
        runtime.begin_frame();
        let first = runtime.advance_custom_timers(&state);
        for id in [10001, 10002, 10003] {
            assert_eq!(first[&id], Some(state.now_us), "mode={mode:?} id={id}");
        }
        assert_eq!(runtime.evaluate_number(counter, &state), Some(1.0));
        assert_eq!(runtime.advance_custom_timers(&state), first);
        assert_eq!(runtime.evaluate_number(counter, &state), Some(1.0));
        // Equal scene timestamps still represent different render frames.
        runtime.begin_frame();
        assert_eq!(runtime.advance_custom_timers(&state), first);
        assert_eq!(runtime.evaluate_number(counter, &state), Some(2.0));
        state.options.insert(2, false);
        runtime.begin_frame();
        assert!(runtime.advance_custom_timers(&state).values().all(Option::is_none));
        state.now_us += 700_000;
        state.options.insert(2, true);
        runtime.begin_frame();
        assert_eq!(runtime.advance_custom_timers(&state)[&10003], Some(state.now_us));
        assert_eq!(runtime.failure_log_count(), 0);
    }
}

#[test]
fn custom_timers_use_live_observers_passive_helpers_and_microsecond_clock() {
    let mut loaded = load(
        r#"
        local s = require('main_state')
        local t = require('timer_util')
        local passive = t.new_passive_timer()
        local observed = t.timer_observe_boolean(function() return s.option(2) end)
        return {type=5, customTimers={
            {id=10001, timer=function()
                if s.option(3) then passive.turn_on() else passive.turn_off() end
                return observed()
            end},
            {id=10002, timer=passive.timer},
            {id=10003, timer=t.timer_function(10001)}
        }}
    "#,
        LuaSkinRuntimeMode::Compat,
    );
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let mut state = TestLuaMainState { now_us: 4_000_000_000, ..Default::default() };
    state.options.extend([(2, true), (3, true)]);
    runtime.begin_frame();
    let first = runtime.advance_custom_timers(&state);
    assert!(first.values().all(|value| *value == Some(state.now_us)));
    state.now_us += 1_000_000;
    runtime.begin_frame();
    assert_eq!(runtime.advance_custom_timers(&state), first);
    state.options.clear();
    runtime.begin_frame();
    assert!(runtime.advance_custom_timers(&state).values().all(Option::is_none));
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn custom_timer_errors_are_off_and_cannot_write_engine_or_active_timers() {
    let mut loaded = load(
        r#"
        local s = require('main_state')
        return {type=5, customTimers={
            {id=10001, timer=function() error('bad timer') end},
            {id=10002, timer=function() return 0/0 end},
            {id=10003, timer=function() s.set_timer(41, 0) end},
            {id=10004, timer=function() return 200000 end},
            {id=10005, timer=function() s.set_timer(10004, 999999) end},
            {id=10006, timer=function() return s.timer(10004) end},
            {id=10007, timer=function() return -9223372036854775807 - 1 end}
        }}
    "#,
        LuaSkinRuntimeMode::Compat,
    );
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    for _ in 0..2 {
        runtime.begin_frame();
        let values = runtime.advance_custom_timers(&TestLuaMainState::default());
        for id in [10001, 10002, 10003, 10007] {
            assert_eq!(values[&id], None);
        }
        assert_eq!(values[&10004], Some(200000));
        assert_eq!(values[&10005], Some(0)); // LuaJ nil -> long is zero.
        assert_eq!(values[&10006], Some(200000));
        assert!(!values.contains_key(&41));
    }
    assert_eq!(runtime.failure_log_count(), 3);
}

#[test]
fn custom_timer_instruction_budget_and_scene_instances_are_isolated() {
    let source = r#"
        local count = 0
        return {type=5, customTimers={
            {id=10001, timer=function() while true do end end},
            {id=10002, timer=function() count=count+1; return count end}
        }}
    "#;
    for _ in 0..2 {
        let mut loaded = load(source, LuaSkinRuntimeMode::Compat);
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        runtime.begin_frame();
        let values = runtime.advance_custom_timers(&TestLuaMainState::default());
        assert_eq!(values[&10001], None);
        assert_eq!(values[&10002], Some(1));
        assert_eq!(runtime.failure_log_count(), 1);
    }
}
