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
