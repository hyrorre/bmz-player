//! Lua-generated skin definitions that the API extension must not reinterpret:
//! `font.fallback` forms and Unicode strings.

use super::*;

#[test]
fn lua_destination_options_ignore_nested_tables_like_beatoraja() {
    let root = TestDir::created("bmz-lua-nested-destination-options");
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"return {type=5, destination={
            {id='rank', op={{2, 3}, 200}, dst={{x=0, y=0, w=10, h=10}}},
            {id='cover', op={80, {270}, {271}, false}, dst={{x=0, y=0, w=10, h=10}}},
            {id='mask', op={{190, 191}, 2, 975}, dst={{x=0, y=0, w=10, h=10}}}
        }}"#,
    )
    .unwrap();
    let loaded =
        load_lua_skin(&path, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let options = loaded.document.all_destinations(&[200, 80, 2, 975]);
    assert_eq!(options[0].op, [200]);
    assert_eq!(options[1].op, [80]);
    assert_eq!(options[2].op, [2, 975]);
    // The Lua compatibility rule must not make ordinary JSON accept bad ops.
    assert!(
        serde_json::from_value::<SkinDocument>(serde_json::json!({
            "destination": [{"id":"bad", "op":[[2, 3], 200]}]
        }))
        .is_err()
    );
}

#[test]
fn installed_social_play_document_decodes_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    for name in ["play_7"] {
        let entry = root.join(format!("SocialSkin/lua/{name}.luaskin"));
        if !entry.is_file() {
            continue;
        }
        let context = SkinPathContext::new(&entry, [root.clone()]).unwrap();
        let loaded = load_lua_skin_with_path_context(
            &context,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(!loaded.document.destination.is_empty());
    }
}
use bmz_skin_document::SkinFontFallbackDef;

#[test]
fn lua_font_fallback_forms_and_unicode_strings_reach_the_document() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let root = unique_test_dir("bmz-lua-font-fallback");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("skin.luaskin");
        fs::write(
            &path,
            r#"
            local s = require('main_state')
            return {type=5, font={
                {id='list', path='font/main.ttf', fallback={'font/記号.ttf', {path='font/絵文字.ttc', type=1}}},
                {id='single', path='font/main.ttf', fallback='font/b.ttf'},
                {id='object', path='font/main.ttf', fallback={path='font/c.ttf', type=2}},
                {id='empty', path='font/main.ttf', fallback={}},
                {id='invalid', path='font/main.ttf', fallback={{type=1}, 42}},
            }, text={
                {id='constant', font='list', size=16, constantText='日本語 ♪ 🎵 é'},
                {id='live', font='list', size=16, value=function() return s.text(12) .. '／' .. '★' end},
            }}
            "#,
        )
        .unwrap();
        let mut loaded = load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        )
        .unwrap();
        let fallback = |id: &str| {
            loaded.document.font.iter().find(|font| font.id == id).unwrap().fallback.clone()
        };
        let entry = |path: &str, font_type| SkinFontFallbackDef { path: path.into(), font_type };
        assert_eq!(fallback("list"), [entry("font/記号.ttf", 0), entry("font/絵文字.ttc", 1)]);
        assert_eq!(fallback("single"), [entry("font/b.ttf", 0)]);
        assert_eq!(fallback("object"), [entry("font/c.ttf", 2)]);
        assert!(fallback("empty").is_empty());
        assert!(fallback("invalid").is_empty());
        assert_eq!(loaded.document.text[0].constant_text, "日本語 ♪ 🎵 é");

        let state = TestLuaMainState {
            texts: BTreeMap::from([(12, "曲名".to_string())]),
            ..Default::default()
        };
        let live = &loaded.document.text[1];
        if let Some(id) = live.value_expr.strip_prefix(lua::LUA_VALUE_CALLBACK_PREFIX) {
            let runtime = loaded.lua_runtime.as_mut().unwrap();
            assert_eq!(
                runtime.evaluate_text(id.parse().unwrap(), &state).as_deref(),
                Some("曲名／★")
            );
        } else {
            assert_eq!(mode, LuaSkinRuntimeMode::Auto, "compat keeps text values in Lua");
        }
    }
}
