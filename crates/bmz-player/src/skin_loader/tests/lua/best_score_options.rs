use super::*;

#[test]
fn best_score_options_lua_auto_and_compat_follow_snapshot_changes() {
    let root = unique_test_dir("bmz-best-score-options");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local state = require('main_state')
        return { type = 7, w = 1280, h = 720,
            text = {
                {id='label', size=24, value=function() return state.text(19201) end},
                {id='details', size=24, value=function()
                    return tostring(state.number(19201)) .. ':' ..
                        tostring(state.event_index(19202)) .. ':' .. state.text(19203)
                end},
                {id='random', size=24, constantText='RANDOM BADGE'}
            },
            destination = {
                {id='label', op={19200}, dst={{x=0,y=0,w=320,h=24}}},
                {id='details', draw=function() return state.option(19200) end,
                    dst={{x=0,y=30,w=320,h=24}}},
                {id='random', draw=function()
                    return state.option(19200) and state.number(19201) == 2
                end, dst={{x=0,y=60,w=320,h=24}}}
            }
        }
    "#,
    )
    .unwrap();
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let loaded = bmz_skin::load_lua_skin_with_runtime_state(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        )
        .unwrap();
        let mut context = SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            loaded.document,
            [],
        );
        context.set_lua_draw_runtime(loaded.lua_runtime.map(|runtime| {
            Arc::new(LuaSkinDrawRuntimeAdapter::new(runtime))
                as Arc<dyn bmz_render::skin::SkinLuaDrawRuntime>
        }));
        let graph = Arc::new(bmz_render::snapshot::ResultGraphSnapshot::default());
        for (options, expected) in [
            (None, vec![]),
            (Some((2, 1, 1)), vec!["RANDOM", "2:1:FLIP", "RANDOM BADGE"]),
            (Some((11, 10, 0)), vec!["MF-RANDOM", "11:10:OFF"]),
            (Some((0, 0, 0)), vec!["NORMAL", "0:0:OFF"]),
            (None, vec![]),
        ] {
            let state = SkinDrawState {
                skin_attempt: bmz_render::snapshot::SkinAttemptState {
                    best_score_options: options.map(|(arrange_1p, arrange_2p, double_option)| {
                        bmz_render::snapshot::SkinBestScoreOptions {
                            arrange_1p,
                            arrange_2p,
                            double_option,
                        }
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            context.begin_frame();
            let texts = context
                .static_document_items_for_result_state_and_text(
                    &graph,
                    &state,
                    &SkinTextState::default(),
                )
                .into_iter()
                .filter_map(|item| match item {
                    SkinRenderItem::Text { text, .. } => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(texts, expected, "{mode:?}: {options:?}");
        }
    }
    fs::remove_dir_all(root).unwrap();
}
