//! Regression tests for access routes that used to bypass the Lua sandbox:
//! the real `os` / `io` in `package.loaded`, the stock `package.searchers` /
//! `package.searchpath`, and binary chunks via `load` / `string.dump`.
//!
//! The checks only inspect values. No dangerous function is ever called, and
//! files live in private temporary directories.

use super::*;

/// Lua that returns `ok`, or the names of the failed checks. The same body is
/// evaluated in the header pass, the document pass and a runtime callback.
const SANDBOX_CHECKS: &str = r#"
local function sandbox_checks()
    local failed = {}
    local function expect(ok, name) if not ok then failed[#failed + 1] = name end end

    local required_os, required_io = require('os'), require('io')
    expect(required_os == os and package.loaded.os == os, 'os module identity')
    expect(required_io == io and package.loaded.io == io, 'io module identity')
    for _, candidate in ipairs({ os, required_os, package.loaded.os }) do
        expect(candidate.execute == nil and candidate.exit == nil and candidate.remove == nil
            and candidate.rename == nil and candidate.getenv == nil and candidate.tmpname == nil,
            'os exposes process or file operations')
    end
    for _, candidate in ipairs({ io, required_io, package.loaded.io }) do
        expect(candidate.popen == nil and candidate.tmpfile == nil, 'io exposes popen/tmpfile')
    end
    expect(package.loaded.debug == nil and debug == nil, 'debug reachable')
    expect(package.loadlib == nil and package.cpath == '', 'native loader reachable')

    expect(package.searchpath == nil, 'package.searchpath reachable')
    expect(#package.searchers == 1 and package.searchers[2] == nil, 'file searchers reachable')

    expect(string.dump == nil and ('').dump == nil, 'string.dump reachable')
    local chunk, err = load('\27Lua\84\0garbage')
    expect(chunk == nil and tostring(err):find("mode is 't'", 1, true) ~= nil, 'binary load accepted')
    chunk, err = load('\27Lua\84\0garbage', 'binary', 'b')
    expect(chunk == nil and tostring(err):find("mode is 't'", 1, true) ~= nil, 'mode b accepted')

    -- Compatible behavior that must keep working.
    expect(type(os.clock()) == 'number' and type(os.time()) == 'number', 'os time')
    expect(type(os.date('%Y')) == 'string' and os.difftime(5, 2) == 3, 'os date/difftime')
    expect(load('return 1 + 1')() == 2, 'text load')
    expect(load('return x', 'env', 't', { x = 5 })() == 5, 'text load with env')
    expect(pcall(load('return print', 'nil env', 't', nil)) == false, 'explicit nil env')
    local parts = { 'return ', '3' }
    expect(load(function() return table.remove(parts, 1) end)() == 3, 'reader load')
    package.preload.hardening_preloaded = function() return 'preloaded' end
    expect(type(package.searchers[1]('hardening_preloaded')) == 'function', 'preload searcher')

    if #failed == 0 then return 'ok' end
    return table.concat(failed, '; ')
end
"#;

fn write_check_skin(root: &Path) -> PathBuf {
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        format!(
            "{SANDBOX_CHECKS}\nreturn {{type=5, name=sandbox_checks(), \
             text={{{{id='checks', value=sandbox_checks}}}}}}"
        ),
    )
    .unwrap();
    path
}

#[test]
fn standard_library_bypass_routes_are_closed_in_header_document_and_runtime() {
    let root = TestDir::created("bmz-lua-sandbox-hardening");
    let path = write_check_skin(&root);

    let header = crate::lua::load_lua_skin_header_value(&path).unwrap();
    assert_eq!(header.value["name"], "ok", "header pass");

    let mut loaded = load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
    )
    .unwrap();
    assert_eq!(loaded.document.name, "ok", "document pass");
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let id = (0..runtime.callback_count())
        .find(|&id| runtime.callback_path(id) == Some("$.text[1].value"))
        .unwrap();
    assert_eq!(
        runtime.evaluate_text(id, &TestLuaMainState::default()).as_deref(),
        Some("ok"),
        "runtime callback"
    );
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn stock_searchers_cannot_read_modules_outside_the_skin_root() {
    let base = TestDir::created("bmz-lua-searcher-escape");
    let outside = base.join("outside");
    let root = base.join("skin");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::write(outside.join("secret_module.lua"), "return 'DUMMY-SECRET'").unwrap();
    fs::write(root.join("lib/shared.lua"), "return { value = 'shared' }").unwrap();
    let outside_template = outside.join("?.lua").to_string_lossy().replace('\\', "/");
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        format!(
            r#"
            local leaked = {{}}
            local saved = package.path
            package.path = '{outside_template};' .. saved
            for index = 1, 4 do
                local searcher = package.searchers[index]
                if searcher ~= nil then
                    local loader = searcher('secret_module')
                    if type(loader) == 'function' then leaked[#leaked + 1] = tostring(loader()) end
                end
            end
            local ok = pcall(require, 'secret_module')
            if ok then leaked[#leaked + 1] = 'require' end
            package.path = saved
            local shared = require('lib.shared')
            return {{type=5, name=table.concat(leaked, ',') .. '|' .. shared.value}}
            "#
        ),
    )
    .unwrap();

    let loaded = load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new())
        .expect("shared module inside the skin root still loads");
    assert_eq!(loaded.document.name, "|shared");
    assert_eq!(
        fs::read_to_string(outside.join("secret_module.lua")).unwrap(),
        "return 'DUMMY-SECRET'"
    );
}

#[test]
fn dofile_and_loadfile_load_skin_files_as_text_only() {
    let root = TestDir::created("bmz-lua-binary-files");
    // A UTF-8 file that starts with the binary chunk signature.
    fs::write(root.join("binary.lua"), "\x1bLua garbage").unwrap();
    fs::write(root.join("text.lua"), "return 'text'").unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local function rejected(loader)
            local ok, result, err = pcall(loader, 'binary.lua')
            local message = tostring(ok and err or result)
            return (not ok or result == nil) and message:find("mode is 't'", 1, true) ~= nil
        end
        local results = {
            tostring(rejected(dofile)),
            tostring(rejected(loadfile)),
            tostring(dofile('text.lua') == 'text' and loadfile('text.lua')() == 'text'),
        }
        return {type=5, name=table.concat(results, ',')}
        "#,
    )
    .unwrap();
    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.document.name, "true,true,true");
}
