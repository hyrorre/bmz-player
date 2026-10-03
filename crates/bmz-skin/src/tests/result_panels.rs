use super::*;
use std::panic;

#[test]
fn rank_value_inference_preserves_shared_state_even_when_the_return_matches_a_ref() {
    let root = unique_test_dir("bmz-shared-rank-value");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local main_state = require('main_state')
        local rank_plus = false
        local scorerate = 0
        return {
            type = 7,
            value = {{id = 'rank_diff_count', value = function()
                rank_plus = true
                scorerate = main_state.number(71) / (main_state.number(74) * 2)
                return main_state.number(71)
            end}},
            text = {{id = 'state', value = function()
                return tostring(rank_plus) .. ':' .. tostring(scorerate)
            end}},
            destination = {{id = 'rank_diff_max_plus', draw = function()
                return rank_plus and scorerate == 1
            end}}
        }
    "#,
    )
    .unwrap();
    for mode in [LuaSkinRuntimeMode::Auto, LuaSkinRuntimeMode::Compat] {
        let mut loaded = load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        )
        .unwrap();
        let draw = only_destination_draw(&loaded);
        assert!(draw.starts_with("bmz:lua_draw_callback:"), "{mode:?}: {draw}");
        let draw_id = draw.rsplit(':').next().unwrap().parse().unwrap();
        let value = &loaded.document.value[0].value_expr;
        assert!(value.starts_with("bmz:lua_value_callback:"), "{mode:?}: {value}");
        let number = value.rsplit(':').next().unwrap().parse().unwrap();
        let text = loaded.document.text[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
        let runtime = loaded.lua_runtime.as_mut().unwrap();
        for (score, expected) in [(200, "true:1.0"), (100, "true:0.5"), (200, "true:1.0")] {
            let state = TestLuaMainState {
                numbers: BTreeMap::from([(71, score), (74, 100)]),
                ..Default::default()
            };
            runtime.begin_frame();
            assert_eq!(runtime.evaluate_number(number, &state), Some(score as f64));
            assert_eq!(runtime.evaluate_text(text, &state).as_deref(), Some(expected));
            assert_eq!(runtime.evaluate_draw(draw_id, &state), score == 200);
        }
        assert_eq!(runtime.failure_log_count(), 0);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn result_panel_runtime_preserves_closures_and_restores_nested_scopes() {
    let mut loaded = load_runtime_value_fixture(
        "bmz-result-panel-runtime",
        LuaSkinRuntimeMode::Compat,
        r#"
        Expand_op = 2
        local result_mode = 0
        local calls = 0
        local number_value = function()
            calls = calls + 1
            return result_mode * 100 + Expand_op * 10 + calls
        end
        -- A separate upvalue must also follow the panel, then retain its own initial value.
        local function make_text()
            local result_mode = 1
            return function() return result_mode .. ':' .. Expand_op .. ':' .. calls end
        end
        local text_value = make_text()
        "#,
    );
    let number = loaded.document.value[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
    let text = loaded.document.text[0].value_expr.rsplit(':').next().unwrap().parse().unwrap();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    let graph = TestLuaMainState { result_panel: Some(2), ..Default::default() };
    let ir = TestLuaMainState { result_panel: Some(1), ..Default::default() };
    let inactive = TestLuaMainState { result_panel: Some(0), ..Default::default() };
    let unspecified = TestLuaMainState::default();
    let scope = runtime.state_scope();

    assert_eq!(runtime.evaluate_number(number, &graph), Some(21.0));
    scope
        .with_state(&ir, || {
            assert_eq!(runtime.evaluate_number_in_scope(number), Some(112.0));
            assert_eq!(runtime.evaluate_text_in_scope(text).as_deref(), Some("1:1:2"));
            assert_eq!(runtime.evaluate_number(number, &graph), Some(23.0));
            assert_eq!(runtime.evaluate_text_in_scope(text).as_deref(), Some("1:1:3"));
            let panic = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                scope
                    .with_state(&inactive, || {
                        assert_eq!(runtime.evaluate_text_in_scope(text).as_deref(), Some("2:0:3"));
                        panic!("scope unwind");
                    })
                    .unwrap();
            }));
            assert!(panic.is_err());
            assert_eq!(runtime.evaluate_text_in_scope(text).as_deref(), Some("1:1:3"));
        })
        .unwrap();
    assert_eq!(runtime.evaluate_number(number, &unspecified), Some(24.0));
    assert_eq!(runtime.evaluate_text(text, &unspecified).as_deref(), Some("1:2:4"));
    assert_eq!(runtime.evaluate_number(number, &ir), Some(115.0));
    assert_eq!(runtime.evaluate_number(number, &graph), Some(26.0));
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn result_panel_compat_detects_local_default_without_tab_actions() {
    let root = unique_test_dir("bmz-result-panel-default");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local result_mode = 0
        return { type=7, destination={
            {id='graph',draw=function() return result_mode == 0 end},
            {id='ir',draw=function() return result_mode == 1 end}
        }}
    "#,
    )
    .unwrap();
    let mut loaded = load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: LuaSkinRuntimeMode::Compat, ..Default::default() },
    )
    .unwrap();
    assert_eq!(loaded.document.result_panel_default, Some(2));
    let callbacks = loaded
        .document
        .destination
        .iter()
        .map(|entry| {
            let bmz_skin_document::DestinationListEntry::Single(dst) = entry else { panic!() };
            dst.draw.rsplit(':').next().unwrap().parse().unwrap()
        })
        .collect::<Vec<_>>();
    let runtime = loaded.lua_runtime.as_mut().unwrap();
    for (panel, expected) in
        [(2, [true, false]), (1, [false, true]), (0, [false, false]), (2, [true, false])]
    {
        runtime.begin_frame();
        let state = TestLuaMainState { result_panel: Some(panel), ..Default::default() };
        for (&callback, expected) in callbacks.iter().zip(expected) {
            assert_eq!(runtime.evaluate_draw(callback, &state), expected, "panel={panel}");
        }
        // These callbacks share one upvalue. Restoring duplicate bindings in
        // sequence would leave it at the scoped value instead of its default.
        let unspecified = TestLuaMainState::default();
        assert!(runtime.evaluate_draw(callbacks[0], &unspecified));
        assert!(!runtime.evaluate_draw(callbacks[1], &unspecified));
    }
    assert_eq!(runtime.failure_log_count(), 0);
    fs::remove_dir_all(root).unwrap();
}
