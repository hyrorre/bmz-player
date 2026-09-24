use super::*;

#[test]
fn ir_ranking_missing_scores_match_auto_and_compat_through_updates() {
    let root = unique_test_dir("bmz-ir-missing-score");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    fs::write(
        &path,
        r#"
        local state = require('main_state')
        return { type=5, w=1920, h=1080,
            value={
                {id='a',src='digits',w=100,h=10,divx=10,digit=6,
                    value=function() return state.number(380) end},
                {id='b',src='digits',w=100,h=10,divx=10,digit=6,
                    value=function() return state.number(381) end},
                {id='c',src='digits',w=100,h=10,divx=10,digit=6,
                    value=function() return state.number(382) end}
            },
            destination={
                {id='a',dst={{x=0,y=0,w=12,h=20}}},
                {id='b',dst={{x=0,y=30,w=12,h=20}}},
                {id='c',dst={{x=0,y=60,w=12,h=20}}}
            }
        }
        "#,
    )
    .unwrap();
    let contexts =
        [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat].map(|mode| {
            let loaded = bmz_skin::load_lua_skin_with_runtime_state(
                &path,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
            )
            .unwrap();
            assert_eq!(loaded.lua_runtime.is_some(), mode == bmz_skin::LuaSkinRuntimeMode::Compat);
            let mut context = SkinContext::from_manifest_and_document(
                bmz_render::skin::default_skin_manifest(),
                loaded.document,
                [SkinDocumentTexture {
                    source_id: "digits".into(),
                    texture: bmz_render::skin::SkinTextureId(700),
                    source_size: SkinImageSize { width: 100.0, height: 10.0 },
                }],
            );
            context.set_lua_draw_runtime(loaded.lua_runtime.map(|runtime| {
                Arc::new(LuaSkinDrawRuntimeAdapter::new(runtime))
                    as Arc<dyn bmz_render::skin::SkinLuaDrawRuntime>
            }));
            context
        });
    // Reuse the same contexts to catch stale cached digits on arrival/removal.
    for (scores, expected_digits) in [
        (None, 0),
        (Some([0, 42, 9999]), 7),
        (Some([i64::from(i32::MIN), i64::from(i32::MAX), 0]), 1),
        (None, 0),
    ] {
        let mut state = SkinDrawState { select_screen: true, ..Default::default() };
        if let Some(scores) = scores {
            for (entry, score) in state.ir_ranking.entries.iter_mut().zip(scores) {
                entry.ex_score = Some(score);
            }
        }
        let [auto, compat] = contexts.each_ref().map(|context| {
            context.begin_frame();
            context.static_document_items_for_state(&state)
        });
        assert_eq!(auto, compat, "scores={scores:?}");
        assert_eq!(auto.len(), expected_digits, "scores={scores:?}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ir_ranking_missing_values_remain_sentinels_in_lua_api() {
    let mut state = SkinDrawState::default();
    let text = BTreeMap::new();
    let provider = RenderLuaMainState { state: &state, enabled_options: &[], text_values: &text };
    for reference in 380..=399 {
        assert_eq!(provider.number(reference), i64::from(i32::MIN), "ref={reference}");
    }
    // Missing unrelated properties retain their previous zero fallback.
    assert_eq!(provider.number(-999), 0);
    state.ir_ranking.entries[0].ex_score = Some(0);
    state.ir_ranking.entries[0].rank = Some(1);
    let provider = RenderLuaMainState { state: &state, enabled_options: &[], text_values: &text };
    assert_eq!(provider.number(380), 0);
    assert_eq!(provider.number(390), 1);
    assert_eq!(provider.number(381), i64::from(i32::MIN));
}
