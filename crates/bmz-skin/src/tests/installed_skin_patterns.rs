//! Minimal reproductions of Lua patterns found in the installed skin survey.
//! They pin behavior that existing skins depend on across the API extension;
//! no third-party skin source is copied.

use super::*;
use bmz_skin_document::SKIN_EXPR_GAUGE_PERCENT_INTEGER;

fn load(name: &str, source: &str, mode: LuaSkinRuntimeMode) -> LoadedSkinDocument {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("sound.wav"), b"").unwrap();
    let path = root.join("skin.luaskin");
    fs::write(&path, source).unwrap();
    load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::from([("GHOST TARGET".to_string(), "MYBEST".to_string())]),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
    )
    .unwrap()
}

/// A BMZ-adapted WMII_FHD branch reads `main_state.float`, which only the
/// persistent runtime VM provides. It must stay a working runtime callback.
#[test]
fn bmz_adapted_float_alias_branch_stays_live() {
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load(
            "bmz-pattern-float-alias",
            r#"
            local header = {type=0, property={{name='GHOST TARGET',
                item={{name='MYBEST', op=990}, {name='TARGET', op=991}}}}}
            if skin_config == nil then return header end
            local main_state = require('main_state')
            local function best_ghost()
                if type(bmz) == 'table' then
                    return main_state.float(113) > 0 and skin_config.option['GHOST TARGET'] == 990
                end
                return main_state.number(150) > 0
            end
            return {type=0, destination={{id='ghost', draw=best_ghost, dst={{x=0, y=0, w=1, h=1}}}}}
            "#,
            mode,
        );
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let draw = only_destination_draw(&loaded).to_string();
        let id: usize = draw
            .strip_prefix("bmz:lua_draw_callback:")
            .expect("runtime-only alias")
            .parse()
            .unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        let mut state = TestLuaMainState::default();
        state.floats.insert(113, 0.5);
        assert!(runtime.evaluate_draw(id, &state));
        state.floats.insert(113, 0.0);
        assert!(!runtime.evaluate_draw(id, &state));
        assert_eq!(runtime.failure_log_count(), 0);
    }
}

/// PeacefulPlay derives gauge digits from `main_state.gauge() or 0`; the
/// object ID keeps selecting BMZ's built-in expression without a Lua callback.
#[test]
fn gauge_digit_values_keep_their_builtin_expression() {
    let loaded = load(
        "bmz-pattern-gauge-digits",
        r#"
        local main_state = require('main_state')
        local function percent() return math.floor((main_state.gauge() or 0) * 100 / 100) end
        return {type=0, value={{id='val-gauge-percent-integer', value=percent}}}
        "#,
        LuaSkinRuntimeMode::Auto,
    );
    assert_eq!(loaded.document.value[0].value_expr, SKIN_EXPR_GAUGE_PERCENT_INTEGER);
    assert!(loaded.lua_runtime.is_none());
}

/// Luxez-Flat and ModernChic play click sounds without a volume from `act`.
/// Full-volume defaults must not turn those UI sounds into scene audio.
#[test]
fn ui_actions_with_omitted_volume_do_not_become_scene_audio() {
    let loaded = load(
        "bmz-pattern-ui-sound",
        r#"
        local main_state = require('main_state')
        local function click()
            main_state.audio_play(skin_config and skin_config.get_path('sound.wav') or 'sound.wav')
        end
        return {type=5, destination={{id='button', act=click, dst={{x=0, y=0, w=10, h=10}}}}}
        "#,
        LuaSkinRuntimeMode::Auto,
    );
    assert!(loaded.document.scene_audio.is_empty());
    assert!(loaded.document.custom_events.is_empty());
}

/// mz-select and simple-result show `os.date()` on Result; BMZ keeps its
/// established default text instead of LuaJ's `%c`.
#[test]
fn default_os_date_text_is_unchanged() {
    let loaded = load(
        "bmz-pattern-default-date",
        r#"return {type=7, name=tostring(os.date():match('^%d%d%d%d%-%d%d%-%d%d %d%d:%d%d:%d%d$') ~= nil)}"#,
        LuaSkinRuntimeMode::Auto,
    );
    assert_eq!(loaded.document.name, "true");
}
