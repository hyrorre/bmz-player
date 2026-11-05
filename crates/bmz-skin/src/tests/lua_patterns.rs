#[test]
fn metered_patterns_match_lua54_results() {
    let native = mlua::Lua::new();
    let sandbox = mlua::Lua::new();
    crate::lua::install_test_instruction_limit(&sandbox, 1_000_000_000, 1_000_000_000);
    let runner = r#"
        return function(op, s, p, init, replacement, maximum)
            local function encode(...)
                local values = table.pack(...)
                local result = {}
                for i=1,values.n do
                    local v = tostring(values[i])
                    result[#result+1] = type(values[i])..':'..#v..':'..v
                end
                return table.concat(result, '|')
            end
            local ok, result = pcall(function()
                if op == 'find' then return encode(string.find(s, p, init)) end
                if op == 'plain' then return encode(string.find(s, p, init, true)) end
                if op == 'match' then return encode(string.match(s, p, init)) end
                if op == 'gsub' then return encode(string.gsub(s, p, replacement, maximum)) end
                if op == 'function' then
                    return encode(string.gsub(s,p,function(...) return encode(...) end,maximum))
                end
                if op == 'table' then
                    return encode(string.gsub(s,p,setmetatable({a='A',b=false,[1]=42},
                        {__index=function() return nil end}),maximum))
                end
                local results = {}
                for a,b,c in string.gmatch(s,p,init) do
                    results[#results+1] = encode(a,b,c)
                    assert(#results < 30, 'iterator did not advance')
                end
                return table.concat(results, ';')
            end)
            return ok and 'ok:'..result or 'error'
        end
    "#;
    let native_run: mlua::Function = native.load(runner).eval().unwrap();
    let sandbox_run: mlua::Function = sandbox.load(runner).eval().unwrap();
    let mut patterns = vec![
        "",
        "abc",
        ")",
        "^a",
        "a$",
        "^$",
        "()",
        "(a)()",
        "((a)(b))",
        "(a*)b%1",
        "%0",
        "%1",
        "()%1",
        "(a%1)",
        "(a",
        "a)",
        "%",
        "a%",
        "%b()",
        "%bxx",
        "%b(",
        "%f[%a]%a+%f[%A]",
        "%f[%z]",
        "%f[^%z]",
        "%f.",
        "%f[",
        "[]]",
        "[^]]",
        "[a-z]",
        "[-a]",
        "[a-]",
        "[z-a]",
        "[a%-c]",
        "[",
        "[]",
        "[^]",
        "a*ab",
        "a-ab",
        ".*()$",
        "a?a?ab",
        "(.-)(.)",
        "(a?)b",
        "(a*)a",
        "(a?)a",
        "%a+",
        "[%d%a]+",
        "a\0b",
        "%z",
        "[\0a]",
        "^a*()",
        "a**",
        "a*?",
        "(a)+",
        "%b()*",
        "%f[%a]+",
        "%f[%a]a",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for class in "aAcCdDgGlLpPsSuUwWxXzZqQ".chars() {
        patterns.push(format!("%{class}"));
        patterns.push(format!("[%{class}]+"));
    }
    // Small exhaustive combinations exercise greedy/minimal backtracking and
    // capture rollback without allowing the native reference to run away.
    for first in ["a", "a*", "a-", "a?", "a+", ".*", "(a*)", "()", "[ab]*"] {
        for second in ["a", "b", "ab", "$", "(a)", "a*", "a-", "%1"] {
            patterns.push(format!("{first}{second}"));
        }
    }
    for s in
        ["", "a", "b", "aaaaab", "aba", "ababa", "(a(b)c)", "xx", "]-a", "a\0b", "a1 _\t\n", "éあ"]
    {
        for p in &patterns {
            for op in ["find", "plain", "match", "gmatch", "gsub", "function", "table"] {
                for init in [-20, -1, 1, 2, 20] {
                    let args = (op, s, p.as_str(), init, "[%0|%1]", 4);
                    let expected: mlua::String = native_run.call(args).unwrap();
                    let actual: mlua::String = sandbox_run.call(args).unwrap();
                    assert_eq!(
                        actual.as_bytes(),
                        expected.as_bytes(),
                        "op={op}, s={s:?}, p={p:?}, init={init}"
                    );
                }
            }
        }
    }
    for replacement in ["", "%%", "%0", "%1", "%2", "%9", "%a", "%", "%0%0", "日本語"] {
        for p in ["", "()", "(.)", "(a)(b)", ".", "a*", "a$"] {
            for maximum in [-1, 0, 1, 2, 20] {
                let args = ("gsub", "ababa", p, 1, replacement, maximum);
                let expected: String = native_run.call(args).unwrap();
                let actual: String = sandbox_run.call(args).unwrap();
                assert_eq!(actual, expected, "p={p:?}, repl={replacement:?}, maximum={maximum}");
            }
        }
    }
}

#[test]
fn every_pattern_api_stops_pathological_backtracking() {
    for call in [
        "string.find(s,p)",
        "s:match(p)",
        "string.gmatch(s,p)()",
        "string.gsub(s,p,'x')",
        "pcall(function() string.find(s,p) end)",
        "coroutine.wrap(function() string.find(s,p) end)()",
    ] {
        let (send, recv) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let lua = mlua::Lua::new();
            crate::lua::install_test_instruction_limit(&lua, 50_000, 50_000);
            let source =
                format!("local s=string.rep('a',64); local p=string.rep('a*',8)..'b'; {call}");
            let result = lua.load(&source).exec().map_err(|err| err.to_string());
            let _ = send.send(result);
        });
        let error = recv
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("pattern engine must be interruptible")
            .unwrap_err();
        assert!(error.contains("pattern work limit"), "{call}: {error}");
    }
}
