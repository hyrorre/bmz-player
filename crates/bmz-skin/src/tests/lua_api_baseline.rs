//! Characterization fixtures for the Lua API that existed before the
//! beatoraja / LR2oraja Endless Dream API extension.
//!
//! Existing names must stay present with the same Lua type, and the recorded
//! return values of existing calls must stay identical. New APIs may only add
//! names; intentional behavior changes must update these fixtures explicitly.

use super::*;

/// Shared Lua helpers that render values with their Lua 5.4 subtype, so an
/// integer/float or nil/false change is visible in the fixture string.
const LUA_FIXTURE_HELPERS: &str = r#"
local function keys(t)
    local list = {}
    for k, v in pairs(t) do list[#list + 1] = tostring(k) .. ':' .. type(v) end
    table.sort(list)
    return table.concat(list, ',')
end
local function show(...)
    local out = {}
    for i = 1, select('#', ...) do
        local v = select(i, ...)
        out[#out + 1] = (math.type(v) or type(v)) .. '=' .. tostring(v)
    end
    return '[' .. table.concat(out, ';') .. ']'
end
local function try(f, ...)
    local r = table.pack(pcall(f, ...))
    if not r[1] then return 'error' end
    return show(table.unpack(r, 2, r.n))
end
local function offset_text(o)
    return string.format('%d,%d,%d,%d,%d,%d', o.x, o.y, o.w, o.h, o.r, o.a)
end
"#;

/// API names present before the extension, as `name:type`. They must remain.
const PRE_EXTENSION_MAIN_STATE_KEYS: &[&str] = &[
    "audio_loop:function",
    "audio_play:function",
    "audio_stop:function",
    "event_index:function",
    "exscore:function",
    "float_number:function",
    "gauge_type:function",
    "judge:function",
    "number:function",
    "numbers:function",
    "offset:function",
    "option:function",
    "score_date_sec_time:function",
    "screen_height:function",
    "screen_width:function",
    "set_timer:function",
    "set_volume_bg:function",
    "set_volume_key:function",
    "set_volume_sys:function",
    "text:function",
    "time:function",
    "timer:function",
    "timer_elapsed:function",
    "timer_elapsed_ms:function",
    "timer_elapsed_seconds:function",
    "timer_is_off:function",
    "timer_is_on:function",
    "timer_off_value:number",
    "total_play_counts_in_session:function",
    "total_play_notes_in_session:function",
    "volume_bg:function",
    "volume_key:function",
    "volume_sys:function",
];

/// The persistent runtime VM additionally exposes this BMZ alias.
const PRE_EXTENSION_RUNTIME_ONLY_MAIN_STATE_KEYS: &[&str] = &["float:function"];

const PRE_EXTENSION_TIMER_UTIL_KEYS: &[&str] = &[
    "is_timer_off:function",
    "is_timer_on:function",
    "new_passive_timer:function",
    "now_timer:function",
    "timer_function:function",
    "timer_observe_boolean:function",
];

const PRE_EXTENSION_EVENT_UTIL_KEYS: &[&str] = &[
    "event_min_interval:function",
    "event_observe_timer:function",
    "event_observe_timer_off:function",
    "event_observe_timer_on:function",
    "event_observe_turn_true:function",
];

const PRE_EXTENSION_SKIN_CONFIG_KEYS: &[&str] =
    &["file_path:table", "get_path:function", "offset:table", "option:table"];

const PRE_EXTENSION_GLOBAL_KEYS: &[&str] = &[
    "_G:table",
    "_VERSION:string",
    "assert:function",
    "bmz:table",
    "bmz_main_state:table",
    "bmz_timer_fn_map:table",
    "collectgarbage:function",
    "coroutine:table",
    "dofile:function",
    "error:function",
    "getmetatable:function",
    "io:table",
    "ipairs:function",
    "load:function",
    "loadfile:function",
    "luajava:table",
    "math:table",
    "next:function",
    "os:table",
    "package:table",
    "pairs:function",
    "pcall:function",
    "print:function",
    "rawequal:function",
    "rawget:function",
    "rawlen:function",
    "rawset:function",
    "require:function",
    "select:function",
    "setmetatable:function",
    "skin_config:table",
    "string:table",
    "table:table",
    "tonumber:function",
    "tostring:function",
    "type:function",
    "utf8:table",
    "warn:function",
    "xpcall:function",
];

const PRE_EXTENSION_OS_KEYS: &[&str] = &["clock:function", "date:function", "time:function"];
const PRE_EXTENSION_IO_KEYS: &[&str] = &["close:function", "lines:function", "open:function"];
const PRE_EXTENSION_LUAJAVA_KEYS: &[&str] =
    &["bindClass:function", "createProxy:function", "newInstance:function"];
const PRE_EXTENSION_BMZ_KEYS: &[&str] = &["get_option:function", "option:table"];

fn load_fixture(source: &str, runtime_state: &LuaLoadRuntimeState) -> LoadedSkinDocument {
    let root = unique_test_dir("bmz-lua-api-baseline");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(&path, format!("{LUA_FIXTURE_HELPERS}\n{source}")).unwrap();
    load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::from([("Mode".to_string(), "B".to_string())]),
        &BTreeMap::new(),
        runtime_state,
    )
    .unwrap()
}

fn fixture_sections(text: &str) -> BTreeMap<String, String> {
    text.split('\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (name, value) = line.split_once(' ').unwrap_or((line, ""));
            (name.to_string(), value.to_string())
        })
        .collect()
}

fn assert_keys_preserved(sections: &BTreeMap<String, String>, section: &str, expected: &[&str]) {
    let actual = sections
        .get(section)
        .unwrap_or_else(|| panic!("missing fixture section {section}"))
        .split(',')
        .collect::<BTreeSet<_>>();
    let missing = expected.iter().filter(|key| !actual.contains(*key)).collect::<Vec<_>>();
    assert!(missing.is_empty(), "{section} lost or changed: {missing:?}; actual={actual:?}");
}

fn assert_values(sections: &BTreeMap<String, String>, expected: &[(&str, &str)]) {
    let mut mismatches = Vec::new();
    for (name, value) in expected {
        match sections.get(*name) {
            Some(actual) if actual == value => {}
            actual => mismatches.push(format!("{name}: expected {value:?}, actual {actual:?}")),
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

/// Load-time API surface and neutral values observed by top-level skin code.
const LOAD_TIME_FIXTURE: &str = r#"
local s = require('main_state')
local tu = require('timer_util')
local eu = require('event_util')
local lines = {}
local function put(name, value) lines[#lines + 1] = name .. ' ' .. value end
local header = { type = 5, name = 'header', property = {
    { name = 'Mode', item = { { name = 'A', op = 901 }, { name = 'B', op = 902 } } },
} }
if skin_config == nil then return header end

put('keys.main_state', keys(s))
put('keys.timer_util', keys(tu))
put('keys.event_util', keys(eu))
put('keys.skin_config', keys(skin_config))
put('keys.globals', keys(_G))
put('keys.os', keys(os))
put('keys.io', keys(io))
put('keys.luajava', keys(luajava))
put('keys.bmz', keys(bmz))
put('same.main_state_global', tostring(rawequal(s, bmz_main_state)))
put('same.luajava_module', tostring(rawequal(require('luajava'), luajava)))

put('number.id', try(s.number, 71))
put('number.numeric_string', try(s.number, '71'))
put('number.name', try(s.number, 'score'))
put('number.unknown_name', try(s.number, 'no_such_property'))
put('number.unknown_id', try(s.number, 99999))
put('number.zero', try(s.number, 0))
put('number.negative', try(s.number, -1))
put('number.fraction', try(s.number, 1.5))
put('number.nil', try(s.number, nil))
put('number.false', try(s.number, false))
put('number.date_year_type', math.type(s.number(21)))
put('number.lastplay_missing', try(s.number, 244))
put('numbers.none', try(s.numbers))
put('numbers.mixed', try(s.numbers, 71, 'score', 'no_such_property'))
put('option.id', try(s.option, 1))
put('option.unknown_id', try(s.option, 99999))
put('option.unknown_name', try(s.option, 'no_such_property'))
put('option.negated_unknown', try(s.option, '!no_such_property'))
put('option.zero', try(s.option, 0))
put('option.nil', try(s.option, nil))
put('text.id', try(s.text, 12))
put('text.name', try(s.text, 'title'))
put('text.unknown', try(s.text, 99999))
put('text.nil', try(s.text, nil))
put('text.version_prefix', tostring(s.text(1010):sub(1, 11) == 'bmz-player '))
put('text.sentinel_1001', try(s.text, 1001))
put('float.id', try(s.float_number, 113))
put('float.unknown_name', try(s.float_number, 'no_such_property'))
put('float.nil', try(s.float_number, nil))
put('offset.unknown', offset_text(s.offset(99999)))
put('offset.keys', keys(s.offset(1)))
put('offset.nil', try(s.offset, nil))
put('timer.id', try(s.timer, 41))
put('timer.unknown', try(s.timer, 99999))
put('timer.off_value', show(s.timer_off_value))
put('timer.nil', try(s.timer, nil))
put('time.type', math.type(s.time()))
put('event_index.id', try(s.event_index, 42))
put('event_index.unknown', try(s.event_index, 99999))
put('judge.0', try(s.judge, 0))
put('judge.5', try(s.judge, 5))
put('judge.6', try(s.judge, 6))
put('judge.-1', try(s.judge, -1))
put('exscore', try(s.exscore))
put('gauge_type', try(s.gauge_type))
put('volume', try(s.volume_sys) .. try(s.volume_key) .. try(s.volume_bg))
put('set_volume', try(s.set_volume_sys, 0.5) .. try(s.set_volume_key, 0) .. try(s.set_volume_bg, nil))
put('audio.play', try(s.audio_play, 'se.wav', 1))
put('audio.play_no_volume', try(s.audio_play, 'se.wav'))
put('audio.loop', try(s.audio_loop, 'bgm.ogg', 0.5))
put('audio.stop', try(s.audio_stop, 'bgm.ogg'))
put('audio.play_nil_path', try(s.audio_play, nil, 1))
put('set_timer.load', try(s.set_timer, 10001, 0))
put('timer_helpers', try(s.timer_is_on, 41) .. try(s.timer_is_off, 41) .. try(s.timer_elapsed, 41)
    .. try(s.timer_elapsed_ms, 41) .. try(s.timer_elapsed_seconds, 41))
put('screen', try(s.screen_width) .. try(s.screen_height))
put('session', try(s.total_play_counts_in_session) .. try(s.total_play_notes_in_session)
    .. try(s.score_date_sec_time))

put('timer_util.now_timer_off', try(tu.now_timer, s.timer_off_value))
put('timer_util.now_timer_long_off', try(tu.now_timer, math.mininteger))
put('timer_util.is_timer', try(tu.is_timer_on, s.timer_off_value) .. try(tu.is_timer_off, s.timer_off_value)
    .. try(tu.is_timer_on, 0) .. try(tu.is_timer_off, math.mininteger))
put('timer_util.timer_function', try(tu.timer_function(41)))
local passive = tu.new_passive_timer()
put('timer_util.passive_keys', keys(passive))
put('timer_util.passive_initial', try(passive.timer))
put('timer_util.passive_returns', try(passive.turn_on) .. try(passive.turn_on_reset) .. try(passive.turn_off))
put('timer_util.passive_after_off', try(passive.timer))
local observed = tu.timer_observe_boolean(function() return false end)
put('timer_util.observe_false', try(observed))
local fired = 0
local turn_true = eu.event_observe_turn_true(function() return true end, function() fired = fired + 1 end)
put('event_util.turn_true', try(turn_true) .. try(turn_true) .. tostring(fired))
local timer_on = 0
local on_event = eu.event_observe_timer_on(function() return 5 end, function() timer_on = timer_on + 1 end)
put('event_util.timer_on', try(on_event) .. try(on_event) .. tostring(timer_on))
local timer_off = 0
local off_event = eu.event_observe_timer_off(function() return s.timer_off_value end,
    function() timer_off = timer_off + 1 end)
put('event_util.timer_off', try(off_event) .. try(off_event) .. tostring(timer_off))
local timer_changes = 0
local timer_event = eu.event_observe_timer(function() return 7 end, function() timer_changes = timer_changes + 1 end)
put('event_util.timer', try(timer_event) .. try(timer_event) .. tostring(timer_changes))

put('skin_config.option', show(skin_config.option.Mode, skin_config.option.Missing))
put('skin_config.option_raw', show(rawget(skin_config.option, 'Mode'), next(skin_config.option)))
local option_pairs = {}
for k, v in pairs(skin_config.option) do option_pairs[#option_pairs + 1] = k .. '=' .. v end
put('skin_config.option_pairs', table.concat(option_pairs, ','))
put('skin_config.enabled_options', type(skin_config.enabled_options))
put('bmz.option', show(bmz.option.Mode, bmz.get_option('Mode'), bmz.get_option('Missing', 'fallback'),
    bmz.get_option('Missing')))
put('package', show(type(package.path), package.cpath, package.loadlib, debug))
put('lua_version', _VERSION)
put('lua5_2_names', show(type(bit32), type(unpack), type(loadstring), type(math.pow), type(math.ldexp),
    type(math.atan2), type(os.difftime)))
put('integer_float', show(10 / 2, 7 // 2, math.floor(2.5), tostring(1e15)))
return { type = 5, name = table.concat(lines, '\n') }
"#;

#[test]
fn pre_extension_load_time_lua_api_surface_and_values_are_preserved() {
    let loaded = load_fixture(LOAD_TIME_FIXTURE, &LuaLoadRuntimeState::default());
    let sections = fixture_sections(&loaded.document.name);
    assert_keys_preserved(&sections, "keys.main_state", PRE_EXTENSION_MAIN_STATE_KEYS);
    assert_keys_preserved(&sections, "keys.timer_util", PRE_EXTENSION_TIMER_UTIL_KEYS);
    assert_keys_preserved(&sections, "keys.event_util", PRE_EXTENSION_EVENT_UTIL_KEYS);
    assert_keys_preserved(&sections, "keys.skin_config", PRE_EXTENSION_SKIN_CONFIG_KEYS);
    assert_keys_preserved(&sections, "keys.globals", PRE_EXTENSION_GLOBAL_KEYS);
    assert_keys_preserved(&sections, "keys.os", PRE_EXTENSION_OS_KEYS);
    assert_keys_preserved(&sections, "keys.io", PRE_EXTENSION_IO_KEYS);
    assert_keys_preserved(&sections, "keys.luajava", PRE_EXTENSION_LUAJAVA_KEYS);
    assert_keys_preserved(&sections, "keys.bmz", PRE_EXTENSION_BMZ_KEYS);
    assert_values(
        &sections,
        &[
            ("same.main_state_global", "true"),
            ("same.luajava_module", "true"),
            ("number.id", "[integer=0]"),
            ("number.numeric_string", "[integer=0]"),
            ("number.name", "[integer=0]"),
            ("number.unknown_name", "[integer=0]"),
            ("number.unknown_id", "[integer=0]"),
            ("number.zero", "[integer=0]"),
            ("number.negative", "[integer=0]"),
            ("number.fraction", "[integer=0]"),
            ("number.nil", "error"),
            ("number.false", "error"),
            ("number.date_year_type", "integer"),
            ("number.lastplay_missing", "[integer=-2147483648]"),
            ("numbers.none", "[]"),
            ("numbers.mixed", "[integer=0;integer=0;integer=0]"),
            ("option.id", "[boolean=false]"),
            ("option.unknown_id", "[boolean=false]"),
            ("option.unknown_name", "[boolean=false]"),
            ("option.negated_unknown", "[boolean=false]"),
            ("option.zero", "[boolean=false]"),
            ("option.nil", "error"),
            ("text.id", "[string=]"),
            ("text.name", "[string=]"),
            ("text.unknown", "[string=]"),
            ("text.nil", "error"),
            ("text.version_prefix", "true"),
            ("text.sentinel_1001", "[string=__BMZ_TEXT_REF_1001__]"),
            ("float.id", "[float=0.0]"),
            ("float.unknown_name", "[float=0.0]"),
            ("float.nil", "error"),
            ("offset.unknown", "0,0,0,0,0,0"),
            ("offset.keys", "a:number,h:number,r:number,w:number,x:number,y:number"),
            ("offset.nil", "error"),
            ("timer.id", "[integer=-2147483648]"),
            ("timer.unknown", "[integer=-2147483648]"),
            ("timer.off_value", "[integer=-2147483648]"),
            ("timer.nil", "error"),
            ("time.type", "integer"),
            ("event_index.id", "[integer=0]"),
            ("event_index.unknown", "[integer=0]"),
            ("judge.0", "[integer=0]"),
            ("judge.5", "[integer=0]"),
            ("judge.6", "[integer=0]"),
            ("judge.-1", "[integer=0]"),
            ("exscore", "[integer=0]"),
            ("gauge_type", "[integer=0]"),
            ("volume", "[float=1.0][float=1.0][float=1.0]"),
            ("set_volume", "[boolean=true][boolean=true][boolean=true]"),
            ("audio.play", "[boolean=true]"),
            ("audio.play_no_volume", "[boolean=true]"),
            ("audio.loop", "[boolean=true]"),
            ("audio.stop", "[boolean=true]"),
            ("audio.play_nil_path", "[boolean=true]"),
            ("set_timer.load", "error"),
            ("timer_helpers", "[boolean=false][boolean=true][integer=-1][integer=-1][integer=-1]"),
            ("screen", "[integer=0][integer=0]"),
            ("session", "[integer=0][integer=0][integer=0]"),
            ("timer_util.now_timer_off", "[integer=0]"),
            ("timer_util.now_timer_long_off", "[integer=0]"),
            ("timer_util.is_timer", "[boolean=false][boolean=true][boolean=true][boolean=true]"),
            ("timer_util.timer_function", "[integer=-2147483648]"),
            (
                "timer_util.passive_keys",
                "timer:function,turn_off:function,turn_on:function,turn_on_reset:function",
            ),
            ("timer_util.passive_initial", "[integer=-2147483648]"),
            ("timer_util.passive_returns", "[][][]"),
            ("timer_util.passive_after_off", "[integer=-2147483648]"),
            ("timer_util.observe_false", "[integer=-2147483648]"),
            ("event_util.turn_true", "[boolean=true][boolean=true]1"),
            ("event_util.timer_on", "[boolean=true][boolean=true]1"),
            ("event_util.timer_off", "[boolean=true][boolean=true]0"),
            ("event_util.timer", "[boolean=true][boolean=true]1"),
            ("skin_config.option", "[integer=902;nil=nil]"),
            ("skin_config.option_raw", "[nil=nil;nil=nil]"),
            ("skin_config.option_pairs", "Mode=902"),
            ("skin_config.enabled_options", "nil"),
            ("bmz.option", "[string=B;string=B;string=fallback;string=]"),
            ("package", "[string=string;string=;nil=nil;nil=nil]"),
            ("lua_version", "Lua 5.4"),
            (
                "lua5_2_names",
                "[string=nil;string=nil;string=nil;string=function;string=function;string=function;string=nil]",
            ),
            ("integer_float", "[float=5.0;integer=3;integer=2;string=1e+15]"),
        ],
    );
}

/// Runtime VM values read through the live `LuaMainState` provider.
const RUNTIME_FIXTURE: &str = r#"
local s = require('main_state')
local tu = require('timer_util')
local float_alias = s.float
return { type = 5, text = { { id = 'runtime', value = function()
    local lines = {}
    local function put(name, value) lines[#lines + 1] = name .. ' ' .. value end
    put('keys.main_state', keys(s))
    put('number', try(s.number, 71) .. try(s.number, 'score') .. try(s.number, 'no_such_property')
        .. try(s.number, nil))
    put('numbers', try(s.numbers, 71, 150))
    put('option', try(s.option, 1) .. try(s.option, '!no_such_property') .. try(s.option, nil))
    put('text', try(s.text, 12) .. try(s.text, 'fulltitle') .. try(s.text, 99999))
    put('float', try(s.float_number, 113) .. try(s.float_number, 'no_such_property')
        .. try(s.float, 113) .. tostring(float_alias ~= nil))
    put('timer', try(s.timer, 41) .. try(s.timer, 99999))
    put('time', try(s.time))
    put('event_index', try(s.event_index, 42))
    put('judge', try(s.judge, 0) .. try(s.judge, 5) .. try(s.judge, 6))
    put('exscore', try(s.exscore))
    put('gauge_type', try(s.gauge_type))
    put('offset', offset_text(s.offset(7)))
    put('screen', try(s.screen_width) .. try(s.screen_height))
    put('session', try(s.total_play_counts_in_session) .. try(s.total_play_notes_in_session)
        .. try(s.score_date_sec_time))
    put('set_timer', try(s.set_timer, 10001, 1000) .. try(s.set_timer, 41, 1000))
    put('timer_helpers', try(s.timer_is_on, 41) .. try(s.timer_elapsed_ms, 41))
    put('timer_util.now_timer', try(tu.now_timer, s.timer(41)))
    return table.concat(lines, '\n')
end } } }
"#;

#[test]
fn pre_extension_runtime_lua_api_surface_and_values_are_preserved() {
    let mut loaded = load_fixture(
        RUNTIME_FIXTURE,
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
    );
    let runtime = loaded.lua_runtime.as_mut().expect("compat text callback uses the runtime VM");
    let callback = (0..runtime.callback_count())
        .find(|&id| runtime.callback_path(id) == Some("$.text[1].value"))
        .unwrap();
    let state = TestLuaMainState {
        screen_size: [1280, 720],
        options: BTreeMap::from([(1, true)]),
        numbers: BTreeMap::from([(71, 1234), (150, 1500), (110, 3), (420, 9)]),
        floats: BTreeMap::from([(113, 0.25)]),
        texts: BTreeMap::from([(12, "タイトル".to_string())]),
        timers: BTreeMap::from([(41, 250)]),
        offsets: BTreeMap::from([(7, LuaSkinOffsetValue { x: 1, y: -2, w: 3, h: 4, r: 5, a: 6 })]),
        session_counts: (2, 3000),
        score_date: 1_700_000_000,
        now_us: 9_000_000,
        result_panel: None,
    };
    runtime.begin_frame();
    let text = runtime.evaluate_text(callback, &state).unwrap();
    let sections = fixture_sections(&text);
    let mut runtime_keys = PRE_EXTENSION_MAIN_STATE_KEYS.to_vec();
    runtime_keys.extend_from_slice(PRE_EXTENSION_RUNTIME_ONLY_MAIN_STATE_KEYS);
    assert_keys_preserved(&sections, "keys.main_state", &runtime_keys);
    assert_values(
        &sections,
        &[
            ("number", "[integer=1234][integer=1234][integer=0]error"),
            ("numbers", "[integer=1234;integer=1500]"),
            ("option", "[boolean=true][boolean=false]error"),
            ("text", "[string=タイトル][string=タイトル][string=]"),
            ("float", "[float=0.25][float=0.0][float=0.25]true"),
            ("timer", "[integer=8750000][integer=-2147483648]"),
            ("time", "[integer=9000000]"),
            ("event_index", "[integer=0]"),
            ("judge", "[integer=3][integer=9][integer=0]"),
            ("exscore", "[integer=1234]"),
            ("gauge_type", "[integer=0]"),
            ("offset", "1,-2,3,4,5,6"),
            ("screen", "[integer=1280][integer=720]"),
            ("session", "[integer=2][integer=3000][integer=1700000000]"),
            ("set_timer", "[boolean=true]error"),
            ("timer_helpers", "[boolean=true][integer=250]"),
            ("timer_util.now_timer", "[integer=250000]"),
        ],
    );
    assert_eq!(runtime.failure_log_count(), 0);
}
