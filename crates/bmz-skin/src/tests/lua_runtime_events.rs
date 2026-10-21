//! Runtime `act`, `customEvents` and `main_state.event_exec` (beatoraja
//! `SkinLuaAccessor.loadEvent`, `CustomEvent`, `MainState.executeEvent`).

use super::*;
use bmz_skin_document::{
    SkinAudioActionDef, SkinAudioActionKind, SkinLuaEffect, SkinVolumeChannel, lua_act_callback_id,
};

fn load(source: &str, runtime_mode: LuaSkinRuntimeMode) -> (TestDir, LoadedSkinDocument) {
    let root = unique_test_dir("bmz-lua-runtime-events");
    fs::create_dir_all(&*root).unwrap();
    fs::write(root.join("se.wav"), b"").unwrap();
    let path = root.join("skin.luaskin");
    fs::write(&path, source).unwrap();
    let state = LuaLoadRuntimeState { runtime_mode, ..Default::default() };
    let loaded =
        load_lua_skin_with_runtime_state(&path, &BTreeMap::new(), &BTreeMap::new(), &state)
            .unwrap();
    (root, loaded)
}

fn image_act(document: &SkinDocument, id: &str) -> usize {
    let act = document.image.iter().find(|image| image.id == id).unwrap().act.unwrap();
    lua_act_callback_id(act).expect("a Lua act keeps its function")
}

fn callback(runtime: &LuaSkinRuntime, path: &str) -> usize {
    (0..runtime.callback_count()).find(|&id| runtime.callback_path(id) == Some(path)).unwrap()
}

fn frame(runtime: &mut LuaSkinRuntime, state: &TestLuaMainState) {
    runtime.begin_frame();
    runtime.advance_custom_events(state);
}

#[test]
fn act_function_runs_in_the_runtime_vm_with_click_arguments() {
    let (_root, mut loaded) = load(
        r#"
        local s = require('main_state')
        local opened = false
        local last = 'none'
        return {type=0,
            image={{id='btn', src=0, x=0, y=0, w=1, h=1,
                act=function(a, b) opened = not opened; last = a .. ':' .. b end}},
            destination={{id='btn', dst={{x=0, y=0, w=1, h=1}}},
                {id='panel', draw=function() return opened end, dst={{x=0, y=0, w=1, h=1}}}},
            text={{id='last', value=function() return last end}}}
        "#,
        LuaSkinRuntimeMode::Compat,
    );
    let act = image_act(&loaded.document, "btn");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let draw = callback(runtime, "$.destination[2].draw");
    let text = callback(runtime, "$.text[1].value");
    let state = TestLuaMainState::default();
    assert!(!runtime.evaluate_draw(draw, &state));
    assert!(runtime.execute_act(act, -1, 0, &state));
    // The click mutates the same closure state the draw callbacks read.
    assert!(runtime.evaluate_draw(draw, &state));
    assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("-1:0"));
    assert!(runtime.execute_act(act, 1, 0, &state));
    assert!(!runtime.evaluate_draw(draw, &state));
    // Only act callbacks run from a click.
    assert!(!runtime.execute_act(draw, 1, 0, &state));
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn custom_events_follow_condition_interval_and_event_exec() {
    let (_root, mut loaded) = load(
        r#"
        local s = require('main_state')
        local count = 0
        local nested = 0
        local log = {}
        return {type=0,
            customEvents={
                {id=1000, action=function() count = count + 1 end,
                    condition=function() return s.option(900) end, minInterval=100},
                {id=1001, action=function(a, b) table.insert(log, a .. ':' .. b) end},
                {id=1002, action=17},
                {id=1003, action=function() nested = nested + 1; s.event_exec(1003) end},
            },
            image={{id='btn', src=0, x=0, y=0, w=1, h=1, act=function(a)
                s.event_exec(1001, a, 7); s.event_exec(1002, 3, 4); s.event_exec(17); s.event_exec(1003)
            end}},
            text={{id='state', value=function()
                return count .. '/' .. table.concat(log, ',') .. '/' .. nested
            end}}}
        "#,
        LuaSkinRuntimeMode::Compat,
    );
    let act = image_act(&loaded.document, "btn");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let text = callback(runtime, "$.text[1].value");
    assert!(runtime.has_custom_events());
    let at = |now_us, enabled| TestLuaMainState {
        now_us,
        options: BTreeMap::from([(900, enabled)]),
        ..Default::default()
    };
    frame(runtime, &at(0, true));
    // Once per frame, then the 100 ms minimum interval.
    runtime.advance_custom_events(&at(0, true));
    frame(runtime, &at(50_000, true));
    assert_eq!(runtime.evaluate_text(text, &at(50_000, true)).as_deref(), Some("1//0"));
    frame(runtime, &at(100_000, true));
    frame(runtime, &at(300_000, false));
    assert_eq!(runtime.evaluate_text(text, &at(300_000, false)).as_deref(), Some("2//0"));

    assert!(runtime.execute_act(act, 5, 0, &at(400_000, false)));
    // event_exec runs custom events synchronously with its arguments, sends
    // built-in actions to the app and ignores other IDs. Recursion stops at the
    // nesting limit (the act itself is the first level).
    assert_eq!(runtime.evaluate_text(text, &at(400_000, false)).as_deref(), Some("2/5:7/15"));
    assert_eq!(runtime.take_effects(), vec![SkinLuaEffect::Event { id: 17, arg1: 3, arg2: 4 }]);
    assert!(runtime.take_effects().is_empty());
    assert_eq!(runtime.failure_log_count(), 1);
}

#[test]
fn audio_and_volume_effects_come_from_events_and_volume_reads_are_live() {
    let (root, mut loaded) = load(
        r#"
        local s = require('main_state')
        local timer_util = require('timer_util')
        return {type=7,
            customEvents={{id=1000, action=function() s.audio_play('se.wav', 0.5) end,
                condition=function() return timer_util.is_timer_on(s.timer(2)) end}},
            image={{id='btn', src=0, x=0, y=0, w=1, h=1, act=function()
                s.event_exec(1000); s.audio_loop('se.wav', 3); s.audio_stop('se.wav')
                s.audio_play('missing.wav'); s.set_volume_sys(0.25)
            end}},
            destination={{id='noisy', draw=function() s.audio_play('se.wav'); return true end,
                dst={{x=0, y=0, w=1, h=1}}}},
            text={{id='volume', value=function() return tostring(s.volume_sys()) end},
                {id='key', value=function() return tostring(s.volume_key()) end}}}
        "#,
        LuaSkinRuntimeMode::Compat,
    );
    // The timer-triggered sound stays declarative and is not updated at runtime.
    assert_eq!(loaded.document.custom_events.len(), 1);
    assert_eq!(loaded.document.custom_events[0].timer, 2);
    let act = image_act(&loaded.document, "btn");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    assert!(!runtime.has_custom_events());
    let draw = callback(runtime, "$.destination[1].draw");
    let text = callback(runtime, "$.text[1].value");
    let state =
        TestLuaMainState { numbers: BTreeMap::from([(57, 40), (58, 58)]), ..Default::default() };
    assert!(runtime.evaluate_draw(draw, &state));
    assert!(runtime.take_effects().is_empty(), "draw callbacks never play sounds");
    assert!(runtime.execute_act(act, 1, 0, &state));
    // The app gets the checked file, not the skin-relative name.
    let path = crate::path_context::canonicalize_skin_path(&root.join("se.wav")).unwrap();
    let path = path.to_string_lossy().into_owned();
    let audio = |action, volume| {
        SkinLuaEffect::Audio(SkinAudioActionDef { action, path: path.clone(), volume })
    };
    assert_eq!(
        runtime.take_effects(),
        vec![
            audio(SkinAudioActionKind::Play, 0.5),
            // beatoraja clamps the volume to 0..=2.
            audio(SkinAudioActionKind::Loop, 2.0),
            audio(SkinAudioActionKind::Stop, 1.0),
            SkinLuaEffect::SetVolume { channel: SkinVolumeChannel::System, value: 0.25 },
        ]
    );
    // The missing file is reported once and skipped.
    assert_eq!(runtime.failure_log_count(), 1);
    // A volume set this frame reads back at once, like beatoraja's config.
    let key = callback(runtime, "$.text[2].value");
    assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("0.25"));
    assert_eq!(runtime.evaluate_text(key, &state).as_deref(), Some("0.58"));
    // From the next frame on, the state carries what the app applied.
    runtime.begin_frame();
    assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("0.4"));
}

#[test]
fn runtime_vm_is_built_only_for_events_that_need_it() {
    let (_root, declarative) = load(
        r#"
        local s = require('main_state')
        local timer_util = require('timer_util')
        return {type=7, customEvents={{id=1000, action=function() s.audio_play('se.wav') end,
            condition=function() return timer_util.is_timer_on(s.timer(2)) end}}}
        "#,
        LuaSkinRuntimeMode::Auto,
    );
    assert!(declarative.lua_runtime.is_none());
    assert!(!declarative.dependencies.opaque);

    let (_root, observed) = load(
        r#"
        local s = require('main_state')
        local fired = false
        return {type=0, customEvents={{id=1000, action=function() fired = true end,
            condition=function() return s.option(900) end}}}
        "#,
        LuaSkinRuntimeMode::Auto,
    );
    let runtime = observed.lua_runtime.expect("observed customEvents need the runtime VM");
    assert!(runtime.has_custom_events());
    assert!(observed.dependencies.opaque);
}

#[test]
fn numeric_custom_event_actions_chain_synchronously_and_bound_cycles() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let (_root, mut loaded) = load(
            r#"
            local s = require('main_state')
            local count, last = 0, ''
            return {type=5, customEvents={
                {id=1000, action=1001},
                {id=1001, action=function(a,b)
                    count=count+1; last=a..':'..b; s.audio_play('se.wav')
                end},
                {id=1002, action=1003}, {id=1003, action=1002},
                {id=1004, action=17}},
                image={{id='btn', src=0, x=0, y=0, w=1, h=1, act=function(a,b)
                    s.event_exec(1000,a,b); s.event_exec(1002); s.event_exec(1004,a,b)
                end}},
                text={{id='log', value=function() return count..':'..last end}}}
            "#,
            mode,
        );
        let act = image_act(&loaded.document, "btn");
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let text = callback(runtime, "$.text[1].value");
        let state = TestLuaMainState::default();
        for (count, arg1, arg2) in [(1, -3, 7), (2, 5, 9)] {
            runtime.begin_frame();
            assert!(runtime.execute_act(act, arg1, arg2, &state));
            assert_eq!(
                runtime.evaluate_text(text, &state).unwrap(),
                format!("{count}:{arg1}:{arg2}"),
                "{mode:?}"
            );
            let effects = runtime.take_effects();
            assert!(matches!(
                effects.as_slice(),
                [SkinLuaEffect::Audio(_), SkinLuaEffect::Event { id: 17, .. }]
            ));
            assert_eq!(effects[1], SkinLuaEffect::Event { id: 17, arg1, arg2 });
            assert_eq!(
                runtime.failure_log_count(),
                1,
                "cycles fail once and leave the depth restored"
            );
        }
    }
}

#[test]
fn timer_audio_events_use_runtime_without_a_declarative_scene_trigger() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        for skin_type in [0, 5, 6, 7, 15] {
            for timer in [1, 2, 10001] {
                if matches!(skin_type, 7 | 15) && timer == 2 {
                    continue;
                }
                let (_root, mut loaded) = load(
                    &format!(
                        r#"
                        local s=require('main_state')
                        local t=require('timer_util')
                        return {{type={skin_type}, customEvents={{{{id=1000,
                            condition=function() return t.is_timer_on(s.timer({timer})) end,
                            action=function() s.audio_play('se.wav') end}}}}}}
                    "#
                    ),
                    mode,
                );
                assert_eq!(loaded.document.custom_events[0].timer, timer);
                let runtime = loaded.lua_runtime.as_mut().unwrap_or_else(|| {
                    panic!("type={skin_type}, timer={timer}, mode={mode:?} needs a runtime")
                });
                let state =
                    TestLuaMainState { timers: BTreeMap::from([(timer, 0)]), ..Default::default() };
                runtime.begin_frame();
                runtime.advance_custom_timers(&state);
                runtime.advance_custom_events(&state);
                assert!(matches!(runtime.take_effects().as_slice(), [SkinLuaEffect::Audio(_)]));
                runtime.advance_custom_events(&state);
                assert!(runtime.take_effects().is_empty(), "update only once in a frame");
                assert_eq!(runtime.failure_log_count(), 0);
            }
        }
    }
}

#[test]
fn result_fadeout_audio_keeps_its_single_declarative_trigger() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        for skin_type in [7, 15] {
            let (_root, loaded) = load(
                &format!(
                    r#"
                    local s=require('main_state')
                    local t=require('timer_util')
                    return {{type={skin_type}, customEvents={{{{id=1000,
                        condition=function() return t.is_timer_on(s.timer(2)) end,
                        action=function() s.audio_play('se.wav') end}}}}}}
                "#
                ),
                mode,
            );
            assert!(loaded.lua_runtime.is_none());
            assert!(!loaded.dependencies.opaque);
            assert_eq!(loaded.document.custom_events[0].timer, 2);
            assert_eq!(loaded.document.custom_events[0].audio_actions.len(), 1);
        }
    }
}

#[test]
fn custom_event_conditions_run_during_intervals_and_recheck_nested_execution() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let (_root, mut loaded) = load(
            r#"
            local s=require('main_state')
            local checks, fires=0,0
            return {type=5,customEvents={{id=1000,minInterval=100,
                condition=function()
                    checks=checks+1
                    if s.option(901) then s.event_exec(1000) end
                    return s.option(900)
                end,
                action=function() fires=fires+1 end}},
                text={{id='counts',value=function() return checks..':'..fires end}}}
            "#,
            mode,
        );
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let text = callback(runtime, "$.text[1].value");
        for (now_us, nested, expected) in [
            (0, false, "1:1"),
            (50_000, false, "2:1"),
            (100_000, false, "3:2"),
            (150_000, true, "4:3"),
            (250_000, true, "5:4"),
        ] {
            let state = TestLuaMainState {
                now_us,
                options: BTreeMap::from([(900, true), (901, nested)]),
                ..Default::default()
            };
            frame(runtime, &state);
            assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some(expected), "{mode:?}");
        }
        assert_eq!(runtime.failure_log_count(), 0);
    }
}
