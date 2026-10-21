//! beatoraja / LR2oraja Endless Dream Lua APIs added on top of the
//! pre-extension surface fixed by `lua_api_baseline`.

use super::*;
use bmz_skin_document::SkinAudioActionKind;

fn load(source: &str, state: &LuaLoadRuntimeState) -> LoadedSkinDocument {
    load_with_options(source, state, &BTreeMap::new())
}

fn load_with_options(
    source: &str,
    state: &LuaLoadRuntimeState,
    options: &BTreeMap<String, String>,
) -> LoadedSkinDocument {
    let root = unique_test_dir("bmz-lua-api-extensions");
    fs::create_dir_all(&root).unwrap();
    for asset in ["se.wav", "bgm.ogg"] {
        fs::write(root.join(asset), b"").unwrap();
    }
    let path = root.join("skin.luaskin");
    fs::write(&path, source).unwrap();
    load_lua_skin_with_runtime_state(&path, options, &BTreeMap::new(), state).unwrap()
}

fn callback(runtime: &LuaSkinRuntime, path: &str) -> usize {
    (0..runtime.callback_count()).find(|&id| runtime.callback_path(id) == Some(path)).unwrap()
}

#[test]
fn score_accessors_follow_their_equivalent_property_refs() {
    let state = LuaLoadRuntimeState {
        runtime_mode: LuaSkinRuntimeMode::Compat,
        number_values: BTreeMap::from([(150, 1400), (121, 1450)]),
        ..Default::default()
    };
    let mut loaded = load(
        r#"
        local s = require('main_state')
        -- Load time: the same neutral values and dependencies as number / float_number.
        local loaded = string.format('%s:%s:%s:%d:%d', math.type(s.rate()), s.rate_best(),
            s.rate_rival(), s.exscore_best(), s.exscore_rival())
        return {type=0, name=loaded, text={{id='live', value=function()
            return table.concat({
                tostring(s.rate() == s.float_number(111)), tostring(s.rate_best() == s.float_number(112)),
                tostring(s.rate_rival() == s.float_number(115)), s.rate(), s.rate_best(), s.rate_rival(),
                math.type(s.exscore_best()), s.exscore_best(), s.exscore_rival()}, ':')
        end}}}
        "#,
        &state,
    );
    assert_eq!(loaded.document.name, "float:0.0:0.0:1400:1450");
    assert_eq!(loaded.dependencies.number_values.get(&150), Some(&1400));
    assert_eq!(loaded.dependencies.number_values.get(&121), Some(&1450));
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let text = callback(runtime, "$.text[1].value");
    let live = TestLuaMainState {
        floats: BTreeMap::from([(111, 0.5), (112, 0.25), (115, 0.75)]),
        numbers: BTreeMap::from([(150, 1234), (121, 1500)]),
        ..Default::default()
    };
    assert_eq!(
        runtime.evaluate_text(text, &live).as_deref(),
        Some("true:true:true:0.5:0.25:0.75:integer:1234:1500")
    );
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn score_accessors_reuse_numeric_ref_inference_in_auto_mode() {
    let loaded = load(
        r#"
        local s = require('main_state')
        return {type=0, value={
            {id='best', value=function() return s.exscore_best() end},
            {id='target', value=function() return s.exscore_rival() end}
        }}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    assert_eq!(loaded.document.value[0].ref_id, 150);
    assert_eq!(loaded.document.value[1].ref_id, 121);
    assert!(loaded.lua_runtime.is_none());
}

/// Starseeker's frame digits and PeacefulPlay's gauge helpers call
/// `main_state.gauge()` from value callbacks.
#[test]
fn gauge_is_neutral_at_load_and_live_in_runtime_callbacks() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            r#"
            local main_state = require('main_state')
            local at_load = main_state.gauge()
            return {type=0, name=math.type(at_load) .. ':' .. tostring(at_load), value={
                {id='gaugevalue', value=function()
                    if main_state.gauge() >= 2 then
                        return math.floor(main_state.gauge() / 2) * 2
                    elseif main_state.gauge() < 0 then
                        return 2
                    end
                end},
                {id='percent', value=function() return math.floor((main_state.gauge() or 0) * 100) end}
            }}
            "#,
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        );
        assert_eq!(loaded.document.name, "float:0.0");
        let runtime = loaded.lua_runtime.as_mut().expect("gauge needs the runtime VM");
        let digits = callback(runtime, "$.value[1].value");
        let percent = callback(runtime, "$.value[2].value");
        for (gauge, expected_digits, expected_percent) in
            [(85.5, Some(84.0), 8550.0), (2.0, Some(2.0), 200.0), (-1.0, Some(2.0), -100.0)]
        {
            let state = TestLuaMainState { gauge, ..Default::default() };
            runtime.begin_frame();
            assert_eq!(runtime.evaluate_number(digits, &state), expected_digits);
            assert_eq!(runtime.evaluate_number(percent, &state), Some(expected_percent));
        }
        // beatoraja returns nothing for 0..2; the number stays unavailable.
        let state = TestLuaMainState { gauge: 1.0, ..Default::default() };
        assert_eq!(runtime.evaluate_number(digits, &state), None);
    }
}

#[test]
fn gauge_dependent_draw_conditions_are_not_folded_at_load() {
    let mut loaded = load(
        r#"
        local s = require('main_state')
        return {type=0, destination={{id='danger', draw=function() return s.gauge() < 30 end,
            dst={{x=0, y=0, w=1, h=1}}}}}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    let draw = only_destination_draw(&loaded).to_string();
    let id = draw.strip_prefix("bmz:lua_draw_callback:").expect("gauge draw stays live");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let id = id.parse().unwrap();
    assert!(runtime.evaluate_draw(id, &TestLuaMainState { gauge: 10.0, ..Default::default() }));
    assert!(!runtime.evaluate_draw(id, &TestLuaMainState { gauge: 80.0, ..Default::default() }));
}

/// beatoraja `key_pressed`: libGDX keycodes or `Input.Keys` names, false at load.
#[test]
fn key_pressed_reads_the_live_keyboard_by_gdx_code_or_name() {
    let mut loaded = load(
        r#"
        local s = require('main_state')
        local at_load = tostring(s.key_pressed('A'))
        return {type=0, name=at_load, text={{id='keys', value=function()
            return table.concat({
                tostring(s.key_pressed(29)), tostring(s.key_pressed('A')),
                tostring(s.key_pressed('29')), tostring(s.key_pressed(29.9)),
                tostring(s.key_pressed('L-Shift')), tostring(s.key_pressed('Escape')),
                tostring(s.key_pressed('LShift')), tostring(s.key_pressed(-1)),
                tostring(s.key_pressed(nil)), tostring(s.key_pressed(4096))}, ':')
        end}}, destination={{id='held', draw=function() return s.key_pressed('Space') end,
            dst={{x=0, y=0, w=1, h=1}}}}}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    assert_eq!(loaded.document.name, "false");
    let runtime = loaded.lua_runtime.as_mut().expect("key_pressed keeps callbacks live");
    let text = callback(runtime, "$.text[1].value");
    let draw = callback(runtime, "$.destination[1].draw");
    let released = TestLuaMainState::default();
    assert_eq!(
        runtime.evaluate_text(text, &released).as_deref(),
        Some("false:false:false:false:false:false:false:false:false:false")
    );
    assert!(!runtime.evaluate_draw(draw, &released));
    let held = TestLuaMainState {
        keys: crate::lua_keyboard_state(["A", "LShift", "Space"]),
        ..Default::default()
    };
    assert_eq!(
        runtime.evaluate_text(text, &held).as_deref(),
        Some("true:true:true:true:true:false:false:false:false:false")
    );
    assert!(runtime.evaluate_draw(draw, &held));
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn omitted_audio_volume_plays_at_full_volume_like_beatoraja() {
    let loaded = load(
        r#"
        local s = require('main_state')
        s.audio_play('se.wav')
        s.audio_loop('bgm.ogg', nil)
        s.audio_play('se.wav', 0.5)
        s.audio_play('se.wav', 'loud')
        return {type=7, customEvents={
            {id=1001, condition=function() return s.timer(2) ~= s.timer_off_value end,
             action=function() s.audio_stop('bgm.ogg'); s.audio_play('se.wav') end}
        }}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    let scene = loaded
        .document
        .scene_audio
        .iter()
        .map(|action| (action.action, action.path.as_str(), action.volume))
        .collect::<Vec<_>>();
    assert_eq!(
        scene,
        vec![
            (SkinAudioActionKind::Play, "se.wav", 1.0),
            (SkinAudioActionKind::Loop, "bgm.ogg", 1.0),
            (SkinAudioActionKind::Play, "se.wav", 0.5),
        ]
    );
    let event = &loaded.document.custom_events[0];
    assert_eq!(event.timer, 2);
    assert_eq!(event.audio_actions.len(), 2);
    assert_eq!(event.audio_actions[1].volume, 1.0);
}

#[test]
fn audio_preload_and_dispose_are_accepted_without_scene_actions() {
    let loaded = load(
        r#"
        local s = require('main_state')
        local results = tostring(s.audio_preload('se.wav')) .. ':' .. tostring(s.audio_dispose('se.wav'))
        return {type=7, name=results}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    assert_eq!(loaded.document.name, "true:true");
    assert!(loaded.document.scene_audio.is_empty());
}

#[test]
fn enabled_options_list_selected_ops_in_header_order() {
    let source = r#"
        local header = {type=5, property={
            {name='Color', item={{name='Red', op=901}, {name='Blue', op=902}}},
            {name='Side', item={{name='1P', op=910}, {name='2P', op=911}}, def='2P'},
            {name='Unused', item={{name='Off', op=920}, {name='On', op=921}}},
        }}
        if skin_config == nil then return header end
        local options = skin_config.enabled_options
        local listed = {}
        for i, op in ipairs(options) do listed[#listed + 1] = i .. '=' .. op end
        local paired = 0
        for _ in pairs(options) do paired = paired + 1 end
        local s = require('main_state')
        return {type=5, name=table.concat({#options, table.concat(listed, ','), table.concat(options, '+'),
            tostring(options[0]), tostring(options[4]), tostring(rawget(options, 1)), paired}, '|'),
            text={{id='live', value=function() return table.concat(skin_config.enabled_options, '+') end}}}
    "#;
    let loaded = load_with_options(
        source,
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
        &BTreeMap::from([("Color".to_string(), "Blue".to_string())]),
    );
    assert_eq!(loaded.document.name, "3|1=902,2=911,3=920|902+911+920|nil|nil|nil|3");
    let mut loaded = loaded;
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let text = callback(runtime, "$.text[1].value");
    assert_eq!(
        runtime.evaluate_text(text, &TestLuaMainState::default()).as_deref(),
        Some("902+911+920")
    );
}

#[test]
fn enabled_options_record_only_the_ops_a_skin_reads() {
    let loaded = load_with_options(
        r#"
        local header = {type=5, property={
            {name='Color', item={{name='Red', op=901}, {name='Blue', op=902}}},
            {name='Side', item={{name='1P', op=910}, {name='2P', op=911}}},
        }}
        if skin_config == nil then return header end
        return {type=5, name=tostring(skin_config.enabled_options[2]) .. ':' .. #skin_config.enabled_options}
        "#,
        &LuaLoadRuntimeState::default(),
        &BTreeMap::new(),
    );
    assert_eq!(loaded.document.name, "910:2");
    assert_eq!(loaded.dependencies.option_values.get(&910), Some(&true));
    assert_eq!(loaded.dependencies.option_values.get(&901), None);
}

#[test]
fn passive_timer_controls_return_true_in_custom_timers() {
    let mut loaded = load(
        r#"
        local s = require('main_state')
        local passive = require('timer_util').new_passive_timer()
        return {type=0, customTimers={{id=10001, timer=function()
            if s.option(1) then
                assert(passive.turn_on() == true)
            else
                assert(passive.turn_off() == true)
            end
            return passive.timer()
        end}}}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let mut state = TestLuaMainState { now_us: 4_000_000, ..Default::default() };
    state.options.insert(1, true);
    runtime.begin_frame();
    assert_eq!(runtime.advance_custom_timers(&state)[&10001], Some(4_000_000));
    state.options.insert(1, false);
    runtime.begin_frame();
    assert_eq!(runtime.advance_custom_timers(&state)[&10001], None);
    assert_eq!(runtime.failure_log_count(), 0);
}

/// ModernChic `customfunction.lua` creates and lists folders through
/// `luajava.new(File, path)` without checking for errors.
#[test]
fn luajava_new_returns_the_inert_object_stub() {
    let loaded = load(
        r#"
        local File = luajava.bindClass('java.io.File')
        local dir = luajava.new(File, 'History/261021/')
        local created = dir:mkdir()
        local count = 0
        local files = dir:listFiles()
        for i = 1, #files do count = count + 1 end
        return {type=7, name=type(dir) .. ':' .. tostring(created ~= nil) .. ':' .. count
            .. ':' .. type(require('luajava').new)}
        "#,
        &LuaLoadRuntimeState::default(),
    );
    assert_eq!(loaded.document.name, "table:true:0:function");
}
