use super::*;

fn header(source: &str, file_bytes: Option<usize>) -> Result<LoadedLuaSkinValue> {
    let root = TestDir::created("lua-resource-budget");
    let entry = root.join("entry.luaskin");
    fs::write(&entry, source).unwrap();
    if let Some(size) = file_bytes {
        fs::write(root.join("data.txt"), vec![b'x'; size]).unwrap();
    }
    load_lua_skin_header_value(&entry)
}

#[test]
fn protected_calls_and_coroutines_cannot_swallow_instruction_exhaustion() {
    // A regression must fail the test rather than block the test runner.
    for script in [
        "while true do pcall(function() while true do end end) end",
        "while true do xpcall(function() while true do end end, function() return 'caught' end) end",
        "xpcall(function() error('start') end, function() while true do end end)",
        "coroutine.resume(coroutine.create(function() while true do end end))",
        "coroutine.wrap(function() while true do end end)()",
        "coroutine.wrap(function() return 1 end)(); while true do end",
        "local co=coroutine.create(function() while true do coroutine.yield() end end); while true do coroutine.resume(co) end",
        "while true do coroutine.wrap(function() return 1 end)() end",
        "pcall(function() coroutine.wrap(function() while true do end end)() end)",
        "coroutine.wrap(function() coroutine.wrap(function() while true do end end)() end)()",
        "local co=coroutine.create(function() local x <close> = setmetatable({}, {__close=function() while true do end end}); coroutine.yield() end); coroutine.resume(co); pcall(coroutine.close, co)",
    ] {
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let error = header(script, None).err().map(|error| format!("{error:#}"));
            let _ = send.send(error);
        });
        let error = recv
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("Lua resource test exceeded deadline: {script}"))
            .expect("runaway Lua should fail");
        assert!(error.contains("instruction limit"), "{script}: {error}");
    }
}

#[test]
fn skins_cannot_register_unhooked_gc_finalizers() {
    for source in [
        "setmetatable({}, {__gc=function() while true do end end})",
        "local mt={__gc=false}; local t=setmetatable({},mt); mt.__gc=function() while true do end end",
        "local mt={}; local t=setmetatable({},mt); mt.__gc=function() while true do end end; setmetatable(t,mt)",
    ] {
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let error = header(source, None).err().map(|error| format!("{error:#}"));
            let _ = send.send(error);
        });
        let error = recv
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("GC finalizer must not block VM teardown")
            .expect("finalizer must be rejected");
        assert!(error.contains("finalizers are disabled"), "{error}");
    }
}

#[test]
fn coroutine_yields_nested_resumes_and_protected_errors_keep_their_results() {
    let loaded = header(r#"
        local ok, a, b, c = pcall(function() return 1, nil, 3 end)
        assert(ok and a == 1 and b == nil and c == 3)
        local marker = {}
        ok, a = pcall(function() error(marker) end)
        assert(not ok and a == marker)
        ok, a = xpcall(function() error('test') end, function() return marker end)
        assert(not ok and a == marker)
        local co = coroutine.create(function()
            local inner = coroutine.wrap(function() return 7, nil, 9 end)
            local x,y,z = inner()
            assert(x == 7 and y == nil and z == 9)
            local value = coroutine.yield(4, nil, 6)
            return value
        end)
        ok, a, b, c = coroutine.resume(co)
        assert(ok and a == 4 and b == nil and c == 6)
        ok, a = coroutine.resume(co, 42)
        assert(ok and a == 42 and coroutine.status(co) == 'dead')
        assert(coroutine.close(co))
        for _, protected in ipairs({pcall, function(f) return xpcall(f, function(e) return e end) end}) do
            local wrapped = coroutine.wrap(function()
                local success, value = protected(function()
                    coroutine.yield('paused')
                    return 123
                end)
                assert(success and value == 123)
                return 'done'
            end)
            assert(wrapped() == 'paused')
            assert(wrapped() == 'done')
        end
        return {type=5, name='ok'}
    "#, None).unwrap();
    assert_eq!(loaded.value["name"], "ok");
}

#[test]
fn retained_file_and_iterator_buffers_have_an_aggregate_limit() {
    for open in ["io.open('data.txt', 'rb')", "io.lines('data.txt')"] {
        let source =
            format!("local files={{}}; for i=1,40 do files[i]={open} end; return {{type=5}}");
        let error = header(&source, Some(1024 * 1024)).unwrap_err();
        assert!(format!("{error:#}").contains("retained buffer limit"));
    }
    let loaded = header(
        r#"
        local files = {}
        for i=1,80 do
            local f = assert(io.open('data.txt', 'rb'))
            assert(f:read(1) == 'x')
            assert(f:close())
            files[i] = f -- closed handles may stay reachable
        end
        for i=1,80 do
            local lines = io.lines('data.txt')
            lines = nil
            collectgarbage('collect')
        end
        return {type=5, name='ok'}
    "#,
        Some(1024 * 1024),
    )
    .unwrap();
    assert_eq!(loaded.value["name"], "ok");
}

#[test]
fn shared_lua_strings_cannot_amplify_the_converted_document_without_bound() {
    let error = header(
        r#"
        local text = string.rep('x', 1024 * 1024)
        local values = {}
        for i=1,80 do values[i] = text end
        return {type=5, values=values}
    "#,
        None,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("string byte limit"));
}

#[test]
fn oversized_writes_fail_before_copying_shared_arguments_and_preserve_contents() {
    let loaded = header(
        r#"
        local s = require('main_state')
        local f = assert(io.open('output.txt', 'w'))
        assert(f:write('seed'))
        local text = string.rep('x', 4*1024*1024)
        local args = {}
        for i=1,48 do args[i] = text end
        local ok, err = f:write(table.unpack(args))
        assert(ok == nil and type(err) == 'string')
        assert(io.open('output.txt'):read('*a') == 'seed')
        assert(s.file_write('large.txt', text..text))
        assert(not s.file_append('large.txt', 'x'))
        assert(not s.file_write('large.txt', text..text..'x'))
        assert(#io.open('large.txt'):read('*a') == 8*1024*1024)
        assert(not s.file_clear(string.rep('x', 1024*1024)))
        assert(not s.file_mkdir(string.rep('x', 1024*1024)))
        return {type=5, name='ok'}
    "#,
        None,
    )
    .unwrap();
    assert_eq!(loaded.value["name"], "ok");
}
