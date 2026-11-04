//! Security regression tests for the Lua skin sandbox.
//!
//! One Lua check module runs at every execution stage (header, document,
//! `require`, runtime initialisation, runtime callback, `act` and
//! `customEvents`) and reports the checks that failed. Rust compares the
//! private temporary fixture on disk before and after, so a write or delete
//! that reached the host is caught even when Lua reports success.
//!
//! The tests never call a dangerous host function: they only check whether
//! one is reachable. Data outside the skin root is dummy data carrying the
//! `DUMMY-OUTSIDE-SECRET` marker, and nothing touches the network.
//!
//! Already covered elsewhere and not repeated here: disabled HTTP results in
//! every VM (`lua_disabled_http.rs`), `io.open` traversal and virtual-file size
//! limits (`cases_02.rs`), symlink escapes through `SkinPathContext`
//! (`cases_07.rs`), instruction ceilings for load-time loops (`cases_02.rs`,
//! `cases_05.rs`) and the act inference VM (`lua/tests/isolated_probe.rs`).

use super::*;
use bmz_skin_document::lua_act_callback_id;
use std::sync::mpsc;
use std::time::Duration;

const SECRET: &str = "DUMMY-OUTSIDE-SECRET";

/// The check module. `@BASE@` is the fixture's base directory with `/`
/// separators, and `@SYMLINKS@` is `true` when the symlink cases exist.
const CHECKS_MODULE: &str = r#"
local M = {}
local BASE = [==[@BASE@]==]
local OUTSIDE = BASE .. '/outside'
local SYMLINKS = @SYMLINKS@
local SECRET = 'DUMMY-OUTSIDE-SECRET'
local main_state = require('main_state')
local File = luajava.bindClass('java.io.File')

local function leaked(value)
    return type(value) == 'string' and value:find(SECRET, 1, true) ~= nil
end

local function paths_outside(name)
    local paths = {
        '../../outside/' .. name,
        '..\\..\\outside\\' .. name,
        '../..\\outside/' .. name,
        'sub/../../../outside/' .. name,
        './sub/..\\../../outside/' .. name,
        'skin/../outside/' .. name,
        OUTSIDE .. '/' .. name,
        (OUTSIDE .. '/' .. name):gsub('/', '\\'),
        'C:\\outside\\' .. name,
        'C:/outside/' .. name,
        '//server/share/' .. name,
        '\\\\server\\share\\' .. name,
        'data.txt\0../../outside/' .. name,
        '../../player/bmz/ir_credentials.json',
        BASE .. '/player/bmz/ir_credentials.json',
    }
    if SYMLINKS then
        paths[#paths + 1] = (name == 'secret_mod.lua') and 'link_mod.lua' or 'link.txt'
        paths[#paths + 1] = 'link_dir/' .. name
    end
    return paths
end

local function check_standard_library(expect)
    -- A. Process and file operations are not reachable through any path.
    expect(_G.os == os and rawget(_G, 'os') == os and require('os') == os
        and package.loaded.os == os, 'os identity')
    expect(_G.io == io and rawget(_G, 'io') == io and require('io') == io
        and package.loaded.io == io, 'io identity')
    local allowed_os = { clock = true, date = true, time = true, difftime = true }
    for key in next, os do expect(allowed_os[key], 'os.' .. tostring(key) .. ' exposed') end
    local allowed_io = { open = true, lines = true, close = true }
    for key in next, io do expect(allowed_io[key], 'io.' .. tostring(key) .. ' exposed') end
    expect(debug == nil and rawget(_G, 'debug') == nil and package.loaded.debug == nil
        and not pcall(require, 'debug'), 'debug reachable')
    expect(package.loadlib == nil and package.cpath == '' and package.searchpath == nil,
        'native loader or searchpath reachable')
    expect(#package.searchers == 1, 'file searchers reachable')
    expect(package.loaded._G == _G and require('_G') == _G, '_G module')
    expect(require('string').dump == nil and getmetatable('').__index.dump == nil,
        'string.dump through module or metatable')
    for _, name in ipairs({ 'socket', 'socket.http', 'ssl', 'http', 'ltn12', 'ffi', 'jit', 'lfs' }) do
        expect(not pcall(require, name), 'module ' .. name .. ' loadable')
    end

    -- Walk everything reachable from _G and the string metatable without
    -- invoking metamethods. Names are host-dangerous functions, and secret-like
    -- keys must not appear in any table a skin can see.
    local forbidden = {
        execute = true, popen = true, getenv = true, tmpname = true, loadlib = true,
        searchpath = true, dump = true, sethook = true, gethook = true, getregistry = true,
        getupvalue = true, setupvalue = true, upvaluejoin = true, getlocal = true,
        setlocal = true, setcstacklimit = true, setuservalue = true, getuservalue = true,
    }
    local seen = {}
    local function walk(t, path, depth)
        if seen[t] or depth > 6 then return end
        seen[t] = true
        local key, value = next(t)
        while key ~= nil do
            local name = tostring(key)
            if type(key) == 'string' and forbidden[key] then
                expect(false, 'reachable ' .. path .. '.' .. name)
            end
            local lower = name:lower()
            if lower:find('password', 1, true) or lower:find('token', 1, true)
                or lower:find('secret', 1, true) or lower:find('credential', 1, true)
                or lower:find('apikey', 1, true) or lower:find('api_key', 1, true) then
                expect(false, 'secret-like key ' .. path .. '.' .. name)
            end
            if type(value) == 'userdata' or type(value) == 'thread' then
                expect(false, type(value) .. ' at ' .. path .. '.' .. name)
            end
            if type(value) == 'table' then walk(value, path .. '.' .. name, depth + 1) end
            key, value = next(t, key)
        end
        local metatable = getmetatable(t)
        if type(metatable) == 'table' then walk(metatable, path .. '<mt>', depth + 1) end
    end
    walk(_G, '_G', 0)
    walk(getmetatable(''), 'string<mt>', 0)

    -- Compatible APIs keep working.
    expect(type(os.clock()) == 'number' and type(os.time()) == 'number'
        and os.time({ year = 2020, month = 1, day = 1, hour = 0 }) ~= nil, 'os.clock/time')
    expect(type(os.date('%Y-%m-%d')) == 'string' and type(os.date('*t').year) == 'number'
        and os.difftime(10, 4) == 6, 'os.date/difftime')
    expect(load('return ...', 'args')(4) == 4 and load('return x', 'env', 't', { x = 5 })() == 5,
        'text load')
end

local function inert(value)
    if type(value) ~= 'table' or next(value) ~= nil then return false end
    local metatable = getmetatable(value)
    if type(metatable) ~= 'table' then return false end
    for key, field in next, metatable do
        if (key ~= '__index' and key ~= '__call') or type(field) ~= 'function' then return false end
    end
    return true
end

local function check_luajava(expect)
    -- B. Unknown classes give inert stubs: empty tables whose metatable only
    -- makes more inert stubs. No Java object, userdata or host function.
    expect(require('luajava') == luajava, 'luajava identity')
    local classes = {
        'java.lang.Runtime', 'java.lang.ProcessBuilder', 'java.lang.System', 'java.lang.Class',
        'java.lang.Thread', 'java.lang.reflect.Method', 'java.net.URL', 'java.net.Socket',
        'java.net.http.HttpClient', 'java.io.FileOutputStream', 'java.io.FileInputStream',
        'java.nio.file.Files', 'java.nio.file.Paths', 'org.luaj.vm2.lib.jse.JsePlatform',
        'bmz.sandbox.Nonexistent',
    }
    for _, name in ipairs(classes) do
        local class = luajava.bindClass(name)
        local ok = inert(class)
        expect(ok, 'bindClass ' .. name .. ' is not inert')
        expect(inert(luajava.newInstance(name, 'dummy')), 'newInstance ' .. name)
        if ok then
            expect(inert(luajava.new(class, 'dummy')), 'new ' .. name)
            -- Only called after the class proved to be an inert stub.
            expect(inert(class.getRuntime) and inert(class:getRuntime():exec('bmz-dummy'))
                and inert(class.getenv) and inert(class:forName('java.lang.Runtime')),
                'stub methods of ' .. name)
            local metatable = getmetatable(class)
            expect(inert(metatable.__index(class, 'anything')), 'stub __index of ' .. name)
        end
    end
    expect(inert(luajava.createProxy('java.lang.Runnable', { run = function() end })),
        'createProxy')
    -- Compatibility stubs stay.
    local gdx = luajava.bindClass('com.badlogic.gdx.Gdx')
    expect(type(gdx.graphics:getWidth()) == 'number' and gdx.input:isKeyPressed(0) == false,
        'Gdx stub')
    local controllers = luajava.bindClass('com.badlogic.gdx.controllers.Controllers')
    expect(controllers:getControllers().size == 0, 'Controllers stub')
    expect(type(File) == 'table' and File.bmz_java_class == 'java.io.File', 'java.io.File class')
end

local function check_file_boundaries(expect)
    -- C. Reads stay inside the skin roots for every file API.
    for _, path in ipairs(paths_outside('secret.txt')) do
        local shown = path:gsub('%z', '\\0')
        local file = io.open(path, 'r')
        expect(file == nil or not leaked(file:read('a')), 'io.open read ' .. shown)
        local ok, lines = pcall(io.lines, path)
        if ok and lines then
            for line in lines do expect(not leaked(line), 'io.lines ' .. shown) end
        end
        for _, line in ipairs(main_state.file_read_lines(path)) do
            expect(not leaked(line), 'file_read_lines ' .. shown)
        end
        expect(main_state.file_exists(path) == false, 'file_exists ' .. shown)
        expect(main_state.file_count_lines(path) == 0, 'file_count_lines ' .. shown)
        local java_file = luajava.new(File, path)
        expect(not java_file:exists() and not java_file:isFile(), 'java.io.File ' .. shown)
    end
    for _, path in ipairs(paths_outside('secret_mod.lua')) do
        local shown = path:gsub('%z', '\\0')
        local ok, value = pcall(dofile, path)
        expect(not ok and not leaked(value), 'dofile ' .. shown)
        local chunk = loadfile and select(2, pcall(loadfile, path))
        expect(type(chunk) ~= 'function', 'loadfile ' .. shown)
    end
    for _, directory in ipairs({ OUTSIDE, '../../outside', 'skin/../outside', SYMLINKS and 'link_dir' or OUTSIDE }) do
        local listed, count = main_state.file_list(directory)
        expect(count == 0 and not listed:find('secret', 1, true), 'file_list ' .. directory)
        expect(luajava.new(File, directory):listFiles() == nil, 'listFiles ' .. directory)
    end

    -- require cannot be pointed outside by package.path or preload.
    local saved_path = package.path
    for _, template in ipairs({ OUTSIDE .. '/?.lua', '../../outside/?.lua', 'skin/../outside/?.lua',
        '..\\..\\outside\\?.lua', 'C:\\outside\\?.lua', '//server/share/?.lua' }) do
        package.path = template
        local ok, value = pcall(require, 'secret_mod')
        expect(not ok and not leaked(value), 'require via package.path ' .. template)
        package.loaded.secret_mod = nil
    end
    package.path = saved_path
    if SYMLINKS then
        expect(not pcall(require, 'link_mod'), 'require through a symlink')
    end
    package.preload.preload_probe = function()
        local file = io.open(OUTSIDE .. '/secret.txt')
        return { leaked = file ~= nil and leaked(file:read('a')) }
    end
    expect(require('preload_probe').leaked == false, 'preload loader read outside')
    package.preload.preload_probe, package.loaded.preload_probe = nil, nil

    -- Legitimate paths inside the roots still work.
    expect(io.open('data.txt'):read('a') == 'inside-data', 'io.open inside')
    expect(io.lines('data.txt')() == 'inside-data', 'io.lines inside')
    expect(dofile('inside.lua') == 'inside-lua' and loadfile('inside.lua')() == 'inside-lua',
        'dofile/loadfile inside')
    expect(require('sub.mod') == 'sub-mod', 'require in the skin directory')
    expect(require('common.shared').value == 'shared', 'require shared module in library root')
    expect(main_state.file_read_lines('data.txt')[1] == 'inside-data'
        and main_state.file_exists('data.txt'), 'main_state.file_* inside')
    expect(luajava.new(File, 'data.txt'):isFile() and luajava.new(File, 'sub'):isDirectory(),
        'java.io.File inside')
    expect(io.open('missing.txt') == nil and not pcall(dofile, 'missing.lua')
        and main_state.file_exists('missing.txt') == false, 'missing files')
end

local function check_writes(expect, stage)
    -- D. Writes go to the in-memory overlay and read back from it. Rust checks
    -- that the disk is unchanged.
    local name = 'new_' .. stage .. '.txt'
    local writer = io.open(name, 'w')
    writer:write('w1')
    writer:close()
    expect(io.open(name):read('a') == 'w1', 'io.open w read back')
    io.open('keep.txt', 'w'):write('changed'):close()
    io.open('keep.txt', 'a'):write('+'):close()
    expect(io.open('keep.txt'):read('a') == 'changed+', 'io.open a read back')
    local ms = 'ms_' .. stage .. '.txt'
    expect(main_state.file_write(ms, 'm') and main_state.file_append(ms, 'n'), 'file_write/append')
    expect(main_state.file_read_lines(ms)[1] == 'mn', 'file_write read back')
    main_state.file_delete('del.txt')
    expect(not main_state.file_exists('del.txt') and io.open('del.txt') == nil, 'file_delete hides')
    expect(main_state.file_mkdir('dir_' .. stage) and main_state.file_exists('dir_' .. stage),
        'file_mkdir')
    local java_dir = luajava.new(File, 'jdir_' .. stage)
    expect(java_dir:mkdirs() and java_dir:isDirectory(), 'java.io.File mkdirs')

    for _, path in ipairs({ OUTSIDE .. '/written.txt', '../../outside/written.txt',
        'C:\\outside\\written.txt', 'skin/../outside/written.txt' }) do
        local outside_writer = io.open(path, 'w')
        if outside_writer then outside_writer:write('x') end
        expect(io.open(path) == nil, 'outside write read back ' .. path)
        expect(main_state.file_write(path, 'x') == false and main_state.file_append(path, 'x') == false,
            'file_write outside ' .. path)
        expect(main_state.file_mkdir(path) == false, 'file_mkdir outside ' .. path)
        expect(luajava.new(File, path):mkdirs() == false, 'mkdirs outside ' .. path)
    end
    expect(main_state.file_delete(OUTSIDE .. '/secret.txt') == false
        and main_state.file_delete('../../outside/secret.txt') == false, 'file_delete outside')
    if SYMLINKS then
        expect(main_state.file_write('link.txt', 'x') == false
            and main_state.file_delete('link.txt') == false, 'write through a symlink')
    end
end

function M.run(stage)
    local failed = {}
    local function expect(ok, name) if not ok then failed[#failed + 1] = name end end
    local steps = {
        check_standard_library,
        check_luajava,
        check_file_boundaries,
        function(e) check_writes(e, stage) end,
    }
    for index, step in ipairs(steps) do
        local ok, error = pcall(step, expect)
        if not ok then expect(false, 'step ' .. index .. ' raised: ' .. tostring(error)) end
    end
    if #failed == 0 then return 'ok' end
    return stage .. ': ' .. table.concat(failed, '; ')
end

M.at_require = M.run('require')
return M
"#;

const CHECK_SKIN: &str = r#"
local checks = require('checks')
local at_main = checks.run(skin_config and 'document' or 'header')
local act_result, event_result = 'not run', 'not run'
return {
    type = 5,
    name = checks.at_require .. '|' .. at_main,
    image = {{ id = 'btn', src = 0, x = 0, y = 0, w = 1, h = 1,
        act = function() act_result = checks.run('act') end }},
    customEvents = {{ id = 1000, action = function() event_result = checks.run('event') end,
        condition = function() return true end }},
    text = {
        { id = 'init', value = function() return checks.at_require .. '|' .. at_main end },
        { id = 'callback', value = function() return checks.run('callback') end },
        { id = 'act', value = function() return act_result end },
        { id = 'event', value = function() return event_result end },
    },
}
"#;

struct Fixture {
    _dir: TestDir,
    base: PathBuf,
    library_root: PathBuf,
    entry: PathBuf,
}

/// Creates a private library root with one skin and dummy data outside it.
/// Returns whether the symlink cases could be created.
fn fixture(name: &str) -> (Fixture, bool) {
    let dir = TestDir::created(name);
    let base = crate::path_context::canonicalize_skin_path(&dir).unwrap();
    let library_root = base.join("skins");
    let skin = library_root.join("Sec");
    let outside = base.join("outside");
    for directory in [&skin.join("sub"), &library_root.join("common"), &outside] {
        fs::create_dir_all(directory).unwrap();
    }
    fs::create_dir_all(base.join("player/bmz")).unwrap();
    fs::write(outside.join("secret.txt"), SECRET).unwrap();
    fs::write(outside.join("secret_mod.lua"), format!("return '{SECRET}'")).unwrap();
    fs::write(
        base.join("player/bmz/ir_credentials.json"),
        format!(r#"{{"token":"{SECRET}-TOKEN","password":"{SECRET}-PASSWORD"}}"#),
    )
    .unwrap();
    fs::write(library_root.join("common/shared.lua"), "return { value = 'shared' }").unwrap();
    fs::write(skin.join("sub/mod.lua"), "return 'sub-mod'").unwrap();
    fs::write(skin.join("data.txt"), "inside-data").unwrap();
    fs::write(skin.join("inside.lua"), "return 'inside-lua'").unwrap();
    fs::write(skin.join("keep.txt"), "original").unwrap();
    fs::write(skin.join("del.txt"), "to-delete").unwrap();

    let symlinks = create_symlinks(&skin, &outside);
    let base_text = base.to_string_lossy().replace('\\', "/");
    fs::write(
        skin.join("checks.lua"),
        CHECKS_MODULE
            .replace("@BASE@", &base_text)
            .replace("@SYMLINKS@", if symlinks { "true" } else { "false" }),
    )
    .unwrap();
    let entry = skin.join("skin.luaskin");
    fs::write(&entry, CHECK_SKIN).unwrap();
    (Fixture { _dir: dir, base, library_root, entry }, symlinks)
}

fn create_symlinks(skin: &Path, outside: &Path) -> bool {
    #[cfg(unix)]
    let result = (|| {
        use std::os::unix::fs::symlink;
        symlink(outside.join("secret.txt"), skin.join("link.txt"))?;
        symlink(outside.join("secret_mod.lua"), skin.join("link_mod.lua"))?;
        symlink(outside, skin.join("link_dir"))
    })();
    #[cfg(windows)]
    let result = (|| {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        symlink_file(outside.join("secret.txt"), skin.join("link.txt"))?;
        symlink_file(outside.join("secret_mod.lua"), skin.join("link_mod.lua"))?;
        symlink_dir(outside, skin.join("link_dir"))
    })();
    #[cfg(not(any(unix, windows)))]
    let result: std::io::Result<()> = Err(std::io::Error::other("no symlink support"));
    match result {
        Ok(()) => true,
        Err(error) => {
            // Windows needs Developer Mode or an elevated shell for symlinks.
            eprintln!("NOT VERIFIED: symlink cases skipped, cannot create symlinks: {error}");
            false
        }
    }
}

/// Every file and directory below `root`, with file contents. Symlinks are
/// recorded by their target and not followed.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path).unwrap();
                entries.insert(path, Some(target.to_string_lossy().into_owned().into_bytes()));
            } else if metadata.is_dir() {
                entries.insert(path.clone(), None);
                pending.push(path);
            } else {
                entries.insert(path.clone(), Some(fs::read(&path).unwrap()));
            }
        }
    }
    entries
}

/// Runs `test` on another thread and fails instead of hanging when a limit
/// regresses. A stuck thread ends with the test process.
fn with_deadline<T: Send + 'static>(seconds: u64, test: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(test());
    });
    receiver
        .recv_timeout(Duration::from_secs(seconds))
        .expect("Lua sandbox test did not finish; an execution limit may be missing")
}

fn callback_id(runtime: &LuaSkinRuntime, path: &str) -> usize {
    (0..runtime.callback_count()).find(|&id| runtime.callback_path(id) == Some(path)).unwrap()
}

#[test]
fn sandbox_policy_holds_at_every_lua_execution_stage() {
    let (fixture, _symlinks) = fixture("bmz-lua-sandbox-security");
    let before = snapshot(&fixture.base);
    let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();

    let header = load_lua_skin_header_value_with_path_context(&context).unwrap();
    assert_eq!(header.value["name"], "ok|ok", "require and header stages");
    assert_eq!(snapshot(&fixture.base), before, "header stage changed the disk");

    let mut loaded = load_lua_skin_with_path_context(
        &context,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(loaded.document.name, "ok|ok", "require and document stages");
    let act = lua_act_callback_id(loaded.document.image[0].act.unwrap()).unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let state = TestLuaMainState::default();
    let text = |runtime: &mut LuaSkinRuntime, path: &str| {
        let id = callback_id(runtime, path);
        runtime.begin_frame();
        runtime.evaluate_text(id, &state)
    };
    assert_eq!(text(runtime, "$.text[1].value").as_deref(), Some("ok|ok"), "runtime init");
    assert_eq!(text(runtime, "$.text[2].value").as_deref(), Some("ok"), "runtime callback");
    runtime.begin_frame();
    assert!(runtime.execute_act(act, 1, 0, &state));
    assert_eq!(text(runtime, "$.text[3].value").as_deref(), Some("ok"), "act");
    runtime.begin_frame();
    runtime.advance_custom_events(&state);
    assert_eq!(text(runtime, "$.text[4].value").as_deref(), Some("ok"), "customEvents");
    assert_eq!(runtime.failure_log_count(), 0);
    drop(loaded);
    assert_eq!(snapshot(&fixture.base), before, "document or runtime stage changed the disk");
}

#[test]
fn lua_that_is_not_a_skin_has_no_side_effects_during_discovery() {
    let (fixture, _symlinks) = fixture("bmz-lua-sandbox-non-skin");
    let outside = fixture.base.join("outside").to_string_lossy().replace('\\', "/");
    // Discovery may evaluate any `.luaskin`. These return no header, raise, or
    // never finish, and all of them try to write first.
    let attempts = format!(
        r#"
        local s = require('main_state')
        pcall(function() io.open('keep.txt', 'w'):write('x') end)
        pcall(function() io.open([==[{outside}/dropped.txt]==], 'w'):write('x') end)
        pcall(s.file_write, 'keep.txt', 'x')
        pcall(s.file_delete, 'del.txt')
        pcall(s.file_mkdir, 'made')
        pcall(function() luajava.new(luajava.bindClass('java.io.File'), 'jmade'):mkdirs() end)
        pcall(s.http_get_async, 'https://example.invalid/', 1000, 1024)
        "#
    );
    for tail in ["return nil", "return 42", "error('not a skin')", "while true do end"] {
        fs::write(&fixture.entry, format!("{attempts}\n{tail}")).unwrap();
        let before = snapshot(&fixture.base);
        let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();
        let _ = with_deadline(60, move || {
            load_lua_skin_header_value_with_path_context(&context).map(|value| value.value)
        });
        assert_eq!(snapshot(&fixture.base), before, "`{tail}` changed the disk");
    }
}

#[test]
fn package_preload_works_through_require_without_reopening_file_search() {
    let (fixture, _symlinks) = fixture("bmz-lua-preload");
    fs::write(
        &fixture.entry,
        r#"
        local results = {}
        local function put(value) results[#results + 1] = tostring(value) end

        package.preload.test_module = function() return { value = 123 } end
        put(require('test_module').value == 123)
        -- Cached in package.loaded: the loader runs once.
        local calls = 0
        package.preload.counted = function(name, extra)
            calls = calls + 1
            assert(name == 'counted' and extra == ':preload:')
            return {}
        end
        put(require('counted') == require('counted') and calls == 1)
        -- A nil result is recorded as true, as in Lua 5.4.
        package.preload.nothing = function() end
        put(require('nothing') == true and package.loaded.nothing == true)
        -- A loader may fill package.loaded itself.
        package.preload.self_set = function(name) package.loaded[name] = 'set' end
        put(require('self_set') == 'set')
        -- Errors propagate and nothing is cached.
        package.preload.broken = function() error('broken loader') end
        local ok, err = pcall(require, 'broken')
        put(not ok and tostring(err):find('broken loader', 1, true) ~= nil
            and package.loaded.broken == nil)
        -- Preload wins over a file module, built-ins win over preload.
        package.preload['sub.mod'] = function() return 'from-preload' end
        put(require('sub.mod') == 'from-preload')
        package.preload.main_state = function() return 'fake' end
        put(type(require('main_state')) == 'table')
        package.preload.luajava = function() return 'fake' end
        put(require('luajava') == luajava)
        -- Without a preload entry the sandboxed file search still applies.
        put(require('inside') == 'inside-lua')
        put(not pcall(require, 'secret_mod'))
        put(#package.searchers == 1 and package.searchpath == nil)
        return {type=5, name=table.concat(results, ',')}
        "#,
    )
    .unwrap();
    let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();
    let header = load_lua_skin_header_value_with_path_context(&context).unwrap();
    let expected = ["true"; 11].join(",");
    assert_eq!(header.value["name"], expected.as_str(), "header");
    let loaded = load_lua_skin_with_path_context(
        &context,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(loaded.document.name, expected, "document");
}

/// Harmless data that carries the Lua 5.4 binary chunk signature.
const BINARY_DUMMY: &str = "\x1bLua\x54\x00dummy";

#[test]
fn binary_chunks_are_rejected_everywhere() {
    let (fixture, _symlinks) = fixture("bmz-lua-binary-chunks");
    let skin = fixture.entry.parent().unwrap().to_path_buf();
    fs::write(skin.join("binary_mod.lua"), BINARY_DUMMY).unwrap();
    let checks = r#"
        local function binary_check()
            local failed = {}
            local function rejected(chunk, err)
                return chunk == nil and tostring(err):find("mode is 't'", 1, true) ~= nil
            end
            local dummy = '\27Lua\84\0dummy'
            for _, mode in ipairs({ 'b', 'bt', 't' }) do
                if not rejected(load(dummy, 'string', mode)) then failed[#failed + 1] = 'string ' .. mode end
                local pieces = { '\27', 'Lua', '\84\0dummy' }
                local reader = function() return table.remove(pieces, 1) end
                if not rejected(load(reader, 'reader', mode)) then failed[#failed + 1] = 'reader ' .. mode end
                if not rejected(load(dummy, 'env', mode, {})) then failed[#failed + 1] = 'env ' .. mode end
            end
            if not rejected(load(dummy)) then failed[#failed + 1] = 'default mode' end
            local ok, err = pcall(require, 'binary_mod')
            if ok or not tostring(err):find("mode is 't'", 1, true) then failed[#failed + 1] = 'require' end
            package.loaded.binary_mod = nil
            -- Text chunks still work.
            local pieces = { 'return ', '1 + 2' }
            if load(function() return table.remove(pieces, 1) end, 'reader', 'bt')() ~= 3
                or load('return y', 'env', 'bt', { y = 9 })() ~= 9
                or require('sub.mod') ~= 'sub-mod' then
                failed[#failed + 1] = 'text chunks'
            end
            return #failed == 0 and 'ok' or table.concat(failed, ',')
        end
    "#;
    fs::write(
        &fixture.entry,
        format!(
            "{checks}\nreturn {{type=5, name=binary_check(), \
             text={{{{id='b', value=binary_check}}}}}}"
        ),
    )
    .unwrap();
    let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();
    let header = load_lua_skin_header_value_with_path_context(&context).unwrap();
    assert_eq!(header.value["name"], "ok", "header");
    let mut loaded = load_lua_skin_with_path_context(
        &context,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(loaded.document.name, "ok", "document");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let id = callback_id(runtime, "$.text[1].value");
    assert_eq!(
        runtime.evaluate_text(id, &TestLuaMainState::default()).as_deref(),
        Some("ok"),
        "runtime callback"
    );

    // The skin entry itself is compiled as text.
    fs::write(&fixture.entry, BINARY_DUMMY).unwrap();
    let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();
    for error in [
        load_lua_skin_header_value_with_path_context(&context).err(),
        load_lua_skin_with_path_context(
            &context,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            &BTreeMap::new(),
        )
        .err(),
    ] {
        let message = error.map(|error| format!("{error:#}"));
        assert!(
            message.as_deref().is_some_and(|message| message.contains("mode is 't'")),
            "binary .luaskin must be rejected as a binary chunk: {message:?}"
        );
    }
}

fn load_error(fixture: &Fixture, source: &str) -> String {
    fs::write(&fixture.entry, source).unwrap();
    let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();
    with_deadline(120, move || {
        let header = load_lua_skin_header_value_with_path_context(&context)
            .err()
            .map(|error| format!("{error:#}"));
        let document = load_lua_skin_with_path_context(
            &context,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            &BTreeMap::new(),
        )
        .err()
        .map(|error| format!("{error:#}"));
        format!("{header:?} / {document:?}")
    })
}

#[test]
fn load_time_resource_limits_fail_safely() {
    let (fixture, _symlinks) = fixture("bmz-lua-resource-limits");
    let skin = fixture.entry.parent().unwrap().to_path_buf();

    // Memory: the allocator refuses before the string exists.
    let error =
        load_error(&fixture, "local s = string.rep('x', 320 * 1024 * 1024) return {type=5}");
    assert_eq!(error.matches("not enough memory").count(), 2, "{error}");

    // Instructions in a required module.
    fs::write(skin.join("spin.lua"), "while true do end").unwrap();
    let error = load_error(&fixture, "require('spin') return {type=5}");
    assert_eq!(error.matches("instruction limit").count(), 2, "{error}");

    // Table depth and entry count of the returned document.
    let error = load_error(
        &fixture,
        "local t = {type=5} local n = t for i = 1, 80 do n.next = {} n = n.next end return t",
    );
    assert_eq!(error.matches("nesting is too deep").count(), 2, "{error}");
    let error = load_error(
        &fixture,
        "local t = {type=5, list={}} for i = 1, 200001 do t.list[i] = i end return t",
    );
    assert_eq!(error.matches("entry limit exceeded").count(), 2, "{error}");
}

#[test]
fn runtime_limits_stop_callbacks_act_and_custom_events_without_stopping_the_skin() {
    let (fixture, _symlinks) = fixture("bmz-lua-runtime-limits");
    // `heavy` stays false during load, so only runtime calls do the work.
    fs::write(
        &fixture.entry,
        r#"
        heavy, spin = false, false
        local function work()
            if not heavy then return 'light' end
            local x = 0
            for i = 1, 600000 do x = x + i end
            return 'done'
        end
        local text = { { id = 'cheap', value = function() return 'alive' end },
            { id = 'spin', value = function() if spin then while true do end end return 'idle' end } }
        for i = 1, 8 do text[#text + 1] = { id = 'work' .. i, value = work } end
        return {
            type = 5,
            image = {
                { id = 'enable', src = 0, x = 0, y = 0, w = 1, h = 1,
                    act = function() heavy = true end },
                { id = 'spin', src = 0, x = 0, y = 0, w = 1, h = 1,
                    act = function() spin = true while true do end end },
            },
            customEvents = { { id = 1000, action = function() if spin then while true do end end end,
                condition = function() return spin end } },
            text = text,
        }
        "#,
    )
    .unwrap();
    let context = SkinPathContext::new(&fixture.entry, [fixture.library_root.clone()]).unwrap();
    let (failures, results) = with_deadline(120, move || {
        let mut loaded = load_lua_skin_with_path_context(
            &context,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
            &BTreeMap::new(),
        )
        .unwrap();
        let enable = lua_act_callback_id(loaded.document.image[0].act.unwrap()).unwrap();
        let spin = lua_act_callback_id(loaded.document.image[1].act.unwrap()).unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let state = TestLuaMainState::default();
        let cheap = callback_id(runtime, "$.text[1].value");
        let spin_text = callback_id(runtime, "$.text[2].value");
        let work = (3..=10)
            .map(|index| callback_id(runtime, &format!("$.text[{index}].value")))
            .collect::<Vec<_>>();
        let mut results = Vec::new();

        runtime.begin_frame();
        assert!(runtime.execute_act(enable, 1, 0, &state));
        // Each callback fits its own ceiling, but 8 of them exceed one frame.
        runtime.begin_frame();
        let first_frame =
            work.iter().map(|&id| runtime.evaluate_text(id, &state)).collect::<Vec<_>>();
        results.push(first_frame.iter().filter(|value| value.as_deref() == Some("done")).count());
        // The next frame has a fresh budget.
        runtime.begin_frame();
        results
            .push(usize::from(runtime.evaluate_text(work[0], &state).as_deref() == Some("done")));

        // An endless act, callback and custom event each stop at the limit.
        // Each one logs its own failure.
        let mut failures = vec![runtime.failure_log_count()];
        runtime.begin_frame();
        assert!(runtime.execute_act(spin, 1, 0, &state));
        failures.push(runtime.failure_log_count());
        runtime.begin_frame();
        results.push(usize::from(runtime.evaluate_text(spin_text, &state).is_none()));
        failures.push(runtime.failure_log_count());
        runtime.begin_frame();
        runtime.advance_custom_events(&state);
        failures.push(runtime.failure_log_count());
        runtime.begin_frame();
        results.push(usize::from(runtime.evaluate_text(cheap, &state).as_deref() == Some("alive")));
        (failures, results)
    });
    let [done_in_one_frame, next_frame, spin_stopped, alive] = results[..] else {
        panic!("unexpected results {results:?}");
    };
    assert!((1..8).contains(&done_in_one_frame), "frame budget: {done_in_one_frame} of 8 done");
    assert_eq!(next_frame, 1, "a new frame resets the budget");
    assert_eq!(spin_stopped, 1, "endless callback falls back");
    assert_eq!(alive, 1, "the skin keeps running after limit errors");
    assert!(
        failures.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "act, callback and custom event each log a limit error: {failures:?}"
    );
}
