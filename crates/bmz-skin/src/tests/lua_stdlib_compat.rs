//! LuaJ 3.0 standard-library behavior that beatoraja / LR2oraja Endless Dream
//! skins can rely on. Expected strings were produced by running the same
//! expressions on beatoraja's `luaj-jse-3.0.2-custom.jar` (UTC time zone).

use super::*;

fn evaluate(expressions: &str) -> String {
    let root = unique_test_dir("bmz-lua-stdlib-compat");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        format!(
            r#"
            local function out(...)
                local parts = {{}}
                for i = 1, select('#', ...) do parts[#parts + 1] = tostring(select(i, ...)) end
                return table.concat(parts, ',')
            end
            return {{type=5, name=table.concat({{ {expressions} }}, '\n')}}
            "#
        ),
    )
    .unwrap();
    load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new())
        .unwrap()
        .document
        .name
}

#[test]
fn bit32_module_is_shared_by_global_require_and_runtime_access() {
    assert_eq!(evaluate("out(require('bit32') == bit32, require('bit32').band(7, 3))"), "true,3");
    let root = unique_test_dir("bmz-bit32-module");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        "local b = require('bit32'); return {type=5, text={{id='bits', value=function() \
         return tostring(b == bit32 and require('bit32') == b) .. ':' .. b.band(7, 3) end}}}",
    )
    .unwrap();
    let mut loaded = load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
    )
    .unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let id = (0..runtime.callback_count())
        .find(|&id| runtime.callback_path(id) == Some("$.text[1].value"))
        .unwrap();
    assert_eq!(runtime.evaluate_text(id, &TestLuaMainState::default()).as_deref(), Some("true:3"));
}

#[test]
fn bit32_matches_luaj_results() {
    let lines = evaluate(
        r#"
        out(bit32.band(0xF0F0, 0x0FF0), bit32.band(), bit32.band(-1, 0xFF)),
        out(bit32.bor(1, 2, 4), bit32.bor()),
        out(bit32.bxor(5, 3), bit32.bxor()),
        out(bit32.bnot(0), bit32.bnot(5)),
        out(bit32.btest(1, 2), bit32.btest(3, 2)),
        out(bit32.lshift(1, 31), bit32.lshift(1, 32), bit32.lshift(8, -2), bit32.lshift(3, 4)),
        out(bit32.rshift(256, 4), bit32.rshift(-1, 28), bit32.rshift(1, -4), bit32.rshift(1, 33)),
        out(bit32.arshift(0x80000000, 4), bit32.arshift(-1, 40), bit32.arshift(256, 4), bit32.arshift(1, -2)),
        out(bit32.extract(0xF0, 4, 4), bit32.extract(0xF0, 4), bit32.extract(0x80000000, 31)),
        out(bit32.replace(0, 7, 4, 3), bit32.replace(0xFF, 0, 0, 4), bit32.replace(0, 1, 31)),
        out(bit32.lrotate(0x80000001, 1), bit32.rrotate(1, 1), bit32.lrotate(1, 33), bit32.rrotate(1, -1)),
        out(bit32.band(5.9, 7), bit32.bor(2.5, 0)),
        out(pcall(bit32.extract, 1, 30, 4) == false, pcall(bit32.band, 'x') == false)
        "#,
    );
    assert_eq!(
        lines.lines().collect::<Vec<_>>(),
        [
            "240,4294967295,255",
            "7,0",
            "6,0",
            "4294967295,4294967290",
            "false,true",
            "2147483648,0,2,48",
            "16,15,16,0",
            "4160749568,4294967295,16,4",
            "15,1,1",
            "112,240,2147483648",
            "3,2147483648,2,2",
            "5,2",
            "true,true",
        ]
    );
}

#[test]
fn bit32_rejects_extreme_fields_and_widths_as_lua_errors() {
    assert_eq!(
        evaluate(
            r#"(function()
                for _, args in ipairs({{math.maxinteger, 1}, {1, math.maxinteger},
                    {math.maxinteger, math.maxinteger}, {32, 1}, {31, 2}}) do
                    local ok, err = pcall(bit32.extract, 1, args[1], args[2])
                    assert(not ok and tostring(err):find('non-existent bits', 1, true))
                    ok, err = pcall(bit32.replace, 0, 1, args[1], args[2])
                    assert(not ok and tostring(err):find('non-existent bits', 1, true))
                end
                return out(bit32.extract(-1, 0, 32), bit32.extract(-1, 31, 1),
                    bit32.replace(-1, 0, 0, 32), bit32.replace(0, 1, 31, 1))
            end)()"#,
        ),
        "4294967295,1,0,2147483648"
    );
}

#[test]
fn os_date_specifiers_match_luaj() {
    let format = "%a|%A|%b|%B|%c|%d|%H|%I|%j|%m|%M|%p|%S|%U|%w|%W|%x|%X|%y|%Y|%%";
    let timestamps = [0, 86_400 * 3, 1_700_000_000, 1_735_689_600, 1_761_004_800, 1_767_225_599];
    let expressions = timestamps
        .iter()
        .map(|time| format!("os.date('!{format}', {time})"))
        .chain(["out(os.date('!%I %p', 43200), os.date('!%I %p', 46800))".to_string()])
        .collect::<Vec<_>>()
        .join(",");
    let lines = evaluate(&expressions);
    assert_eq!(
        lines.lines().collect::<Vec<_>>(),
        [
            "Thu|Thursday|Jan|January|Thu Jan 01 00:00:00 1970|01|00|00|001|01|00|AM|00|53|4|53|01/01/70|00:00:00|70|1970|%",
            "Sun|Sunday|Jan|January|Sun Jan 04 00:00:00 1970|04|00|00|004|01|00|AM|00|1|0|53|01/04/70|00:00:00|70|1970|%",
            "Tue|Tuesday|Nov|November|Tue Nov 14 22:13:20 2023|14|22|10|318|11|13|PM|20|46|2|46|11/14/23|22:13:20|23|2023|%",
            "Wed|Wednesday|Jan|January|Wed Jan 01 00:00:00 2025|01|00|00|001|01|00|AM|00|53|3|53|01/01/25|00:00:00|25|2025|%",
            "Tue|Tuesday|Oct|October|Tue Oct 21 00:00:00 2025|21|00|00|294|10|00|AM|00|42|2|42|10/21/25|00:00:00|25|2025|%",
            "Wed|Wednesday|Dec|December|Wed Dec 31 23:59:59 2025|31|23|11|365|12|59|PM|59|52|3|52|12/31/25|23:59:59|25|2025|%",
            "00 PM,01 PM",
        ]
    );
}

/// ModernChic stores daily history under `os.date("%y%m%d")`.
#[test]
fn os_date_two_digit_year_and_existing_bmz_formats() {
    let lines = evaluate(
        r#"
        os.date('!%y%m%d', 1761004800),
        os.date('!%Y-%m-%d %H:%M:%S', 1761004800),
        -- Unknown specifiers stay literal instead of raising.
        os.date('!%Q|%e', 1761004800),
        tostring(os.date():match('^%d%d%d%d%-%d%d%-%d%d %d%d:%d%d:%d%d$') ~= nil),
        tostring(os.date('%z'):match('^[+-]%d%d%d%d$') ~= nil),
        tostring(os.date('!*t', 1761004800).yday)
        "#,
    );
    assert_eq!(
        lines.lines().collect::<Vec<_>>(),
        ["251021", "2025-10-21 00:00:00", "%Q|%e", "true", "true", "294"]
    );
}

#[test]
fn os_difftime_returns_float_seconds() {
    let lines = evaluate(
        r#"
        out(os.difftime(10, 4), os.difftime(1.5, 0.25), math.type(os.difftime(10, 4))),
        out(pcall(os.difftime, 10) == false)
        "#,
    );
    assert_eq!(lines.lines().collect::<Vec<_>>(), ["6.0,1.25,float", "true"]);
}
