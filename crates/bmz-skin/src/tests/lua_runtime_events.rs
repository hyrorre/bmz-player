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
        return {type=0,
            customEvents={{id=1000, action=function() s.audio_play('se.wav', 0.5) end,
                condition=function() return timer_util.is_timer_on(s.timer(2)) end}},
            image={{id='btn', src=0, x=0, y=0, w=1, h=1, act=function()
                s.event_exec(1000); s.audio_loop('se.wav', 3); s.audio_stop('se.wav')
                s.audio_play('missing.wav'); s.set_volume_sys(0.25)
            end}},
            destination={{id='noisy', draw=function() s.audio_play('se.wav'); return true end,
                dst={{x=0, y=0, w=1, h=1}}}},
            text={{id='volume', value=function() return tostring(s.volume_sys()) end}}}
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
    let state = TestLuaMainState { numbers: BTreeMap::from([(57, 40)]), ..Default::default() };
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
    assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some("0.4"));
}

#[test]
fn runtime_vm_is_built_only_for_events_that_need_it() {
    let (_root, declarative) = load(
        r#"
        local s = require('main_state')
        local timer_util = require('timer_util')
        return {type=0, customEvents={{id=1000, action=function() s.audio_play('se.wav') end,
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
