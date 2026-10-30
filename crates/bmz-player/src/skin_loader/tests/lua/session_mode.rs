use super::*;
use bmz_render::skin::{SKIN_EVENT_SELECT_SESSION_MODE, SkinClickTarget, SkinLuaEffect};

#[test]
fn lua_session_mode_refs_and_negative_acts_match_json_and_keep_custom_event_1970() {
    let root = unique_test_dir("bmz-session-mode-events");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.lua");
    fs::write(
        &path,
        r#"
        local s = require('main_state')
        local last = ''
        return {type=5, w=100, h=100,
            source={{id=1,path='session.png'}},
            image={
                {id='mode',src=1,w=210,h=250,divy=5,len=5,ref=1970,act=-10006,click=2},
                {id='custom-numeric',act=1970},
                {id='custom',act=function(a,b) s.event_exec(1970,a,b) end},
                {id='builtin',act=function(a,b) s.event_exec(1000,a,b) end}},
            imageset={{id='set',images={'mode'},ref=1970,act=-10006,click=0}},
            customEvents={
                {id=1970,action=function(a,b) last = a .. ':' .. b end},
                {id=1000,action=-10006}},
            text={{id='last',size=10,value=function() return last end}},
            destination={
                {id='mode',dst={{x=0,y=0,w=20,h=20}}},
                {id='set',dst={{x=20,y=0,w=20,h=20}}},
                {id='last',dst={{x=0,y=50,w=20,h=10}}}}
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
        let doc = loaded.document;
        assert_eq!(doc.image[0].ref_id, 1970);
        assert_eq!(doc.image[0].act, Some(SKIN_EVENT_SELECT_SESSION_MODE));
        assert_eq!(doc.imageset[0].act, Some(SKIN_EVENT_SELECT_SESSION_MODE));
        assert_eq!(doc.image[1].act, Some(1970));
        let act = doc.image[2].act.unwrap();
        let builtin_act = doc.image[3].act.unwrap();
        assert!(bmz_render::skin::lua_act_callback_id(act).is_some());
        let json: SkinDocument = serde_json::from_value(serde_json::json!({
            "type": 5, "w": 100, "h": 100,
            "image": [{"id": "mode", "ref": 1970, "act": -10006, "click": 2}],
            "imageset": [{"id": "set", "images": ["mode"], "ref": 1970, "act": -10006, "click": 0}],
            "destination": [
                {"id": "mode", "dst": [{"x": 0, "y": 0, "w": 20, "h": 20}]},
                {"id": "set", "dst": [{"x": 20, "y": 0, "w": 20, "h": 20}]}
            ]
        }))
        .unwrap();
        let mut context = SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            doc,
            [SkinDocumentTexture {
                source_id: "1".into(),
                texture: bmz_render::skin::SkinTextureId(42),
                source_size: SkinImageSize { width: 210.0, height: 250.0 },
            }],
        );
        context.set_lua_draw_runtime(Some(Arc::new(LuaSkinDrawRuntimeAdapter::new(
            loaded.lua_runtime.unwrap(),
        ))));
        for (index, session) in ["NORMAL", "PRACTICE", "AUTOPLAY", "AUTOPLAY BATTLE", "G-BATTLE"]
            .into_iter()
            .enumerate()
        {
            let snapshot = SelectSnapshot { assist: session.into(), ..Default::default() };
            context.begin_frame();
            let items = context.select_document_items(&snapshot);
            assert!(matches!(items.first(), Some(SkinRenderItem::Image {uv, ..})
                if (uv.y - index as f32 / 5.0).abs() < 0.0001));
            for (x, click) in [(0.1, 2), (0.3, 0)] {
                let hit = context.select_click_hit(&snapshot, x, 0.9).unwrap();
                assert_eq!(
                    hit.target,
                    SkinClickTarget::Event { event_id: SKIN_EVENT_SELECT_SESSION_MODE, click }
                );
                let json_hit = json
                    .select_click_hit(&Default::default(), &snapshot, &Default::default(), x, 0.9)
                    .unwrap();
                assert_eq!(json_hit.target, hit.target);
                assert_eq!(json_hit.rect, hit.rect);
            }
        }
        for id in [-10001, -10002, -10003, -10004, -10005, -10006, -10100, 1970] {
            assert!(!context.queue_lua_act(id, 1, 0), "{id} must not become a function act");
        }
        assert!(context.queue_lua_act(act, -1, 7));
        context.begin_frame();
        assert!(
            context
                .select_document_items(&SelectSnapshot::default())
                .iter()
                .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "-1:7"))
        );
        assert!(context.take_lua_effects().is_empty(), "custom 1970 stays inside the Lua VM");
        assert!(context.queue_lua_act(builtin_act, 1, 0));
        context.begin_frame();
        context.select_document_items(&SelectSnapshot::default());
        assert_eq!(
            context.take_lua_effects(),
            [SkinLuaEffect::Event { id: SKIN_EVENT_SELECT_SESSION_MODE, arg1: 1, arg2: 0 }]
        );
    }
}
