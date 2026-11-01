use super::*;

#[test]
fn clean_runtime_probe_never_accumulates_audio_actions() {
    let mut inference = MainStateProbe::default();
    inference.record_audio_action(LuaAudioActionKindProbe::Play, "se.wav".to_string(), 1.0);
    assert_eq!(inference.take_audio_actions().len(), 1);

    // The persistent runtime VM has nobody draining this list.
    let mut runtime = MainStateProbe { clean_runtime: true, ..Default::default() };
    for _ in 0..1_000 {
        runtime.record_audio_action(LuaAudioActionKindProbe::Play, "se.wav".to_string(), 1.0);
    }
    assert!(runtime.audio_actions.is_empty());
}

struct RepeatState {
    frame: i64,
}

impl LuaMainState for RepeatState {
    fn option(&self, id: i32) -> bool {
        (i64::from(id) + self.frame) % 2 == 0
    }
    fn number(&self, id: i32) -> i64 {
        i64::from(id) + self.frame
    }
    fn float(&self, id: i32) -> f64 {
        f64::from(id) / 1000.0
    }
    fn text(&self, id: i32) -> String {
        format!("テキスト{id}")
    }
    fn timer(&self, _id: i32) -> Option<i32> {
        Some(16)
    }
    fn event_index(&self, id: i32) -> i32 {
        id % 3
    }
    fn gauge_type(&self) -> i32 {
        2
    }
    fn gauge(&self) -> f64 {
        50.0 + self.frame as f64 % 10.0
    }
    fn time_us(&self) -> i32 {
        (self.frame * 16_667) as i32
    }
}

/// Every frame calls the full accessor set from a runtime callback. Neither the
/// Lua heap nor per-call tables may grow with the number of evaluations.
#[test]
fn repeated_runtime_api_calls_keep_lua_memory_bounded() {
    let root = unique_skin_test_dir("repeated-api-calls");
    fs::write(root.join("se.wav"), b"").unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local header = {type=0, property={{name='Mode', item={{name='A', op=901}}}}}
        if skin_config == nil then return header end
        local s = require('main_state')
        local tu = require('timer_util')
        return {type=0, text={{id='all', value=function()
            s.audio_play('se.wav')
            local offset = s.offset(1)
            return table.concat({s.number(71), s.number('score'), tostring(s.option(1)),
                s.text(12), s.float_number(113), s.timer(41), s.time(), s.event_index(42),
                s.gauge_type(), s.gauge(), s.rate(), s.rate_best(), s.rate_rival(),
                s.exscore(), s.exscore_best(), s.exscore_rival(), s.judge(0), offset.x,
                tostring(s.timer_is_on(41)), s.timer_elapsed_ms(41), tu.now_timer(s.timer(41)),
                skin_config.enabled_options[1], #skin_config.enabled_options,
                bit32.band(s.number(71), 0xFF), os.date('!%y', 0), select('#', s.numbers(1, 2, 3))}, '|')
        end}}}
        "#,
    )
    .unwrap();
    let mut loaded = load_lua_skin_value(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
        &BTreeMap::new(),
    )
    .unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let callback = (0..runtime.callback_count())
        .find(|&id| runtime.callback_path(id) == Some("$.text[1].value"))
        .unwrap();
    let mut measure = |frames: std::ops::Range<i64>| {
        for frame in frames {
            runtime.begin_frame();
            assert!(runtime.evaluate_text(callback, &RepeatState { frame }).is_some());
        }
        runtime.lua.gc_collect().unwrap();
        runtime.lua.gc_collect().unwrap();
        runtime.lua.used_memory()
    };
    let warmed = measure(0..1_000);
    let after = measure(1_000..21_000);
    assert!(after <= warmed + 64 * 1024, "Lua memory grew from {warmed} to {after} bytes");
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn custom_frame_lua_allocations_do_not_scale_with_callback_count() {
    fn advance(runtime: &mut LuaSkinRuntime, frame: i64, batched: bool) {
        runtime.begin_frame();
        let state = RepeatState { frame };
        if batched {
            runtime.advance_frame(&state, &[]);
        } else {
            runtime.advance_custom_timers(&state);
            runtime.advance_custom_events(&state);
        }
    }
    fn measure(count: usize, batched: bool) -> usize {
        let root = unique_skin_test_dir("custom-frame-allocation");
        let path = root.join("skin.luaskin");
        fs::write(&path, format!(r#"
            local s = require('main_state')
            local timers, events = {{}}, {{}}
            for i = 1, {count} do
                timers[i] = {{id=10000+i, timer=function() return s.time() end}}
                events[i] = {{id=1000+i, condition=function() return true end, action=function() end}}
            end
            return {{type=0, customTimers=timers, customEvents=events}}
        "#)).unwrap();
        let mut loaded = load_lua_skin_value(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
            &BTreeMap::new(),
        )
        .unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        for frame in 0..10 {
            advance(runtime, frame, batched);
        }
        runtime.lua.gc_collect().unwrap();
        runtime.lua.gc_collect().unwrap();
        runtime.lua.gc_stop();
        let before = runtime.lua.used_memory();
        let started = std::time::Instant::now();
        for frame in 10..210 {
            advance(runtime, frame, batched);
        }
        let growth = runtime.lua.used_memory().saturating_sub(before);
        runtime.lua.gc_restart();
        assert_eq!(runtime.failure_log_count(), 0);
        eprintln!(
            "batched={batched}, custom callbacks={count}+{count}, frames=200, Lua growth={growth} bytes, CPU={:?}",
            started.elapsed()
        );
        fs::remove_dir_all(root).unwrap();
        growth
    }
    for batched in [false, true] {
        let one = measure(1, batched);
        let many = measure(40, batched);
        assert!(
            many <= one + 128 * 1024,
            "per-callback Lua garbage: batched={batched}, one={one} bytes, forty={many} bytes"
        );
    }
}

#[test]
fn custom_frame_orders_inputs_timers_events_once_and_restores_the_outer_provider() {
    let root = unique_skin_test_dir("custom-frame-order");
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local s = require('main_state')
        local n = 0
        return {type=0,
            destination={{id='button', act=function(a,b) n=n+a+b end}},
            customTimers={{id=10001, timer=function() n=n+1; return s.time() end}},
            customEvents={{id=1000,
                condition=function() return s.timer(10001)==s.time() end,
                action=function() n=n*10; s.set_timer(10002, s.time()) end}},
            value={{id='count', value=function() return n end},
                {id='clock', value=function() return s.time() end}}
        }
    "#,
    )
    .unwrap();
    let mut loaded = load_lua_skin_value(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
        &BTreeMap::new(),
    )
    .unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let callback = |path| {
        (0..runtime.callback_count()).find(|&id| runtime.callback_path(id) == Some(path)).unwrap()
    };
    let act = callback("$.destination[1].act");
    let count = callback("$.value[1].value");
    let clock = callback("$.value[2].value");
    let state = RepeatState { frame: 1 };
    let outer = RepeatState { frame: 500 };
    runtime.begin_frame();
    runtime
        .state_scope()
        .with_state(&outer, || {
            let timers = runtime.advance_frame(&state, &[LuaRuntimeInput::Act(act, 2, 3)]);
            assert_eq!(timers[&10001], Some(16_667));
            assert_eq!(timers[&10002], Some(16_667));
            assert_eq!(runtime.evaluate_number_in_scope(count), Some(60.0));
            assert_eq!(runtime.evaluate_number_in_scope(clock), Some(500.0 * 16_667.0));
        })
        .unwrap();
    runtime.advance_frame(&state, &[]);
    assert_eq!(runtime.evaluate_number(count, &state), Some(60.0));
    runtime.begin_frame();
    runtime.advance_frame(&RepeatState { frame: 2 }, &[]);
    assert_eq!(runtime.evaluate_number(count, &state), Some(610.0));
    assert_eq!(runtime.evaluate_number(clock, &state), Some(16_667.0));
    assert_eq!(runtime.failure_log_count(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bit32_never_replaces_an_existing_global() {
    let lua = Lua::new();
    let existing = lua.create_table().unwrap();
    existing.set("custom", true).unwrap();
    lua.globals().set("bit32", existing).unwrap();
    install_bit32_library(&lua).unwrap();
    let bit32: Table = lua.globals().get("bit32").unwrap();
    assert!(bit32.get::<bool>("custom").unwrap());
    assert!(matches!(bit32.get::<Value>("band").unwrap(), Value::Nil));
}
