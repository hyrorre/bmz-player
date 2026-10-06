use super::*;

fn load(source: &str, state: &LuaLoadRuntimeState) -> LoadedSkinDocument {
    let root = unique_test_dir("bmz-read-helpers");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.lua");
    fs::write(&path, source).unwrap();
    let loaded =
        load_lua_skin_with_runtime_state(&path, &BTreeMap::new(), &BTreeMap::new(), state).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    fs::remove_dir_all(root).unwrap();
    loaded
}

fn callback(runtime: &LuaSkinRuntime, path: &str) -> usize {
    (0..runtime.callback_count()).find(|&id| runtime.callback_path(id) == Some(path)).unwrap()
}

#[test]
fn screen_dimensions_reach_header_load_and_captured_runtime_accessors() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local s = require('main_state')
            local width, height = s.screen_width, s.screen_height
            -- Header and document must see the same real size; division by a
            -- neutral header height must not produce infinity or an exception.
            assert(height() > 0)
            local aspect = width() / height()
            assert(aspect == 1920 / 1080)
            return {type=5, name=tostring(width()) .. ':' .. tostring(height()),
                value={{id='width',value=function() return width() end},
                       {id='height',value=function() return height() end}}}
            "#,
            &LuaLoadRuntimeState {
                runtime_mode: mode,
                screen_size: [1920, 1080],
                ..Default::default()
            },
        );
        assert_eq!(loaded.document.name, "1920:1080");
        assert_eq!(loaded.dependencies.screen_size, Some([1920, 1080]));
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let width = callback(runtime, "$.value[1].value");
        let height = callback(runtime, "$.value[2].value");
        for size in [[1920, 1080], [2560, 1440], [0, 0]] {
            let state = TestLuaMainState { screen_size: size, ..Default::default() };
            assert_eq!(runtime.evaluate_number(width, &state), Some(f64::from(size[0])));
            assert_eq!(runtime.evaluate_number(height, &state), Some(f64::from(size[1])));
        }
        assert_eq!(runtime.failure_log_count(), 0);
    }
}

#[test]
fn screen_dimensions_read_only_in_header_are_load_dependencies() {
    let loaded = load(
        r#"
        if skin_config == nil then
            local s = require('main_state')
            assert(s.screen_width() / s.screen_height() == 16 / 9)
            return {type=5, property={{name='size', item={{name='wide',op=900}}}}}
        end
        return {type=5, name=tostring(skin_config.option.size)}
        "#,
        &LuaLoadRuntimeState { screen_size: [1920, 1080], ..Default::default() },
    );
    assert_eq!(loaded.dependencies.screen_size, Some([1920, 1080]));
    assert!(loaded.lua_runtime.is_none());
}

#[test]
fn screen_dimensions_runtime_only_reads_do_not_add_load_dependencies() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local s = require('main_state')
            return {type=5, value={{id='width',value=function() return s.screen_width() end},
                                  {id='height',value=function() return s.screen_height() end}}}
            "#,
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        );
        assert_eq!(loaded.dependencies.screen_size, None);
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let width = callback(runtime, "$.value[1].value");
        let height = callback(runtime, "$.value[2].value");
        assert_eq!(runtime.evaluate_number(width, &TestLuaMainState::default()), Some(0.0));
        assert_eq!(runtime.evaluate_number(height, &TestLuaMainState::default()), Some(0.0));
    }
}

#[test]
fn numbers_preserve_multiple_returns_load_dependencies_and_live_captured_accessors() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local s = require('main_state')
            local numbers = s.numbers
            local loaded_score, loaded_best = numbers('score', 150)
            local loaded_direct = s.number(151)
            assert(type(loaded_score) == 'number' and type(loaded_best) == 'number')
            assert(type(loaded_direct) == 'number')
            assert(select('#', numbers()) == 0)
            local calls = 0
            return {type=5, text={{id='values', value=function()
                calls = calls + 1
                local score, numeric_string, missing, unknown = numbers('score', '71', 244, 'unknown')
                assert(select('#', numbers('score', '71', 244, 'unknown')) == 4)
                assert(select('#', numbers()) == 0)
                assert(numeric_string == score and unknown == 0)
                assert(numbers(99999) == 0)
                local values = {numbers('score', 244)}
                assert(#values == 2 and values[1] == score and values[2] == missing)
                return string.format('%d:%d:%d:%d', score, missing, calls, s.number(151))
            end}}}
            "#,
            &LuaLoadRuntimeState {
                runtime_mode: mode,
                number_values: BTreeMap::from([(71, 77), (150, 88), (151, 99)]),
                ..Default::default()
            },
        );
        assert_eq!(loaded.dependencies.number_values.get(&71), Some(&77));
        assert_eq!(loaded.dependencies.number_values.get(&150), Some(&88));
        assert_eq!(loaded.dependencies.number_values.get(&151), Some(&99));
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let text = callback(runtime, "$.text[1].value");
        let mut state = TestLuaMainState {
            numbers: BTreeMap::from([(71, 123), (151, 314), (244, i64::from(i32::MIN))]),
            ..Default::default()
        };
        assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("123:-2147483648:1:314"));
        state.numbers.extend([(71, 456), (151, 628), (244, 2026)]);
        assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("456:2026:2:628"));
        assert_eq!(runtime.failure_log_count(), 0);
    }
}

#[test]
fn numbers_single_return_still_infers_a_numeric_ref() {
    let loaded = load(
        r#"
        local numbers = require('main_state').numbers
        return {type=5, value={{id='score', value=function() return numbers('score') end}}}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    assert_eq!(loaded.document.value[0].ref_id, 71);
    assert!(loaded.lua_runtime.is_none());
}

#[test]
fn elapsed_helpers_keep_live_timers_and_do_not_fold_auto_values_to_constants() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local s = require('main_state')
            local elapsed, milliseconds, seconds = s.timer_elapsed, s.timer_elapsed_ms, s.timer_elapsed_seconds
            local is_on, is_off = s.timer_is_on, s.timer_is_off
            assert(is_off(41) and not is_on(41))
            assert(elapsed(41) == -1 and milliseconds(41) == -1 and seconds(41) == -1)
            return {type=5, value={
                {id='us', value=function() return elapsed(41) end},
                {id='ms', value=function() return milliseconds(41) end},
                {id='seconds', value=function() return seconds(41) end}
            }, text={{id='state', value=function()
                return tostring(is_on(41)) .. ':' .. tostring(is_off(41))
            end}}}
            "#,
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        );
        let runtime = loaded.lua_runtime.as_mut().expect("elapsed helpers need live state");
        let us = callback(runtime, "$.value[1].value");
        let ms = callback(runtime, "$.value[2].value");
        let seconds = callback(runtime, "$.value[3].value");
        let text = callback(runtime, "$.text[1].value");
        let mut state = TestLuaMainState { now_us: 5_000_000_000, ..Default::default() };
        for elapsed_ms in [None, Some(0), Some(1234), Some(-1234), None] {
            state.timers = elapsed_ms.into_iter().map(|elapsed| (41, elapsed)).collect();
            runtime.begin_frame();
            assert_eq!(
                runtime.evaluate_number(us, &state),
                Some(elapsed_ms.map_or(-1.0, |n| f64::from(n) * 1000.0))
            );
            assert_eq!(
                runtime.evaluate_number(ms, &state),
                Some(elapsed_ms.map_or(-1.0, f64::from))
            );
            assert_eq!(
                runtime.evaluate_number(seconds, &state),
                Some(elapsed_ms.map_or(-1.0, |n| f64::from(n) / 1000.0))
            );
            assert_eq!(
                runtime.evaluate_text(text, &state).as_deref(),
                Some(if elapsed_ms.is_some() { "true:false" } else { "false:true" })
            );
            state.now_us += 1_000_000;
        }
        assert_eq!(runtime.failure_log_count(), 0);
    }
}

#[test]
fn timer_helpers_see_same_frame_custom_writes_zero_start_and_submillisecond_precision() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local s = require('main_state')
            local is_on, is_off = s.timer_is_on, s.timer_is_off
            local elapsed, milliseconds, seconds = s.timer_elapsed, s.timer_elapsed_ms, s.timer_elapsed_seconds
            return {type=5, customTimers={
                {id=10001},
                {id=10002, timer=function()
                    s.set_timer(10001, s.option(2) and 0 or s.timer_off_value)
                    assert(is_on(10001) == s.option(2) and is_off(10001) ~= s.option(2))
                    return s.timer(10001)
                end},
                {id=10003, timer=function()
                    s.set_timer(10004, s.time() + 1501)
                    assert(is_on(10004) and not is_off(10004))
                    assert(elapsed(10004) == -1501 and milliseconds(10004) == -1)
                    assert(seconds(10004) == -0.001501)
                    assert(is_on(10002) == s.option(2))
                    if is_off(10002) then return s.timer_off_value end
                    return elapsed(10002)
                end},
                {id=10004}
            }, text={{id='elapsed', value=function()
                return string.format('%d:%d:%.6f', elapsed(10001), milliseconds(10001), seconds(10001))
            end}}}
            "#,
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        );
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let text = callback(runtime, "$.text[1].value");
        let mut state = TestLuaMainState { now_us: 5_000_001_501, ..Default::default() };
        state.options.insert(2, true);
        runtime.begin_frame();
        let values = runtime.advance_custom_timers(&state);
        assert_eq!(values[&10001], Some(0));
        assert_eq!(values[&10002], Some(0));
        assert_eq!(values[&10003], Some(state.now_us));
        assert_eq!(values[&10004], Some(state.now_us + 1501));
        assert_eq!(
            runtime.evaluate_text(text, &state).as_deref(),
            Some("5000001501:5000001:5000.001501")
        );
        state.options.insert(2, false);
        runtime.begin_frame();
        let values = runtime.advance_custom_timers(&state);
        for id in [10001, 10002, 10003] {
            assert_eq!(values[&id], None);
        }
        assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("-1:-1:-1.000000"));
        assert_eq!(runtime.failure_log_count(), 0);
    }
}
