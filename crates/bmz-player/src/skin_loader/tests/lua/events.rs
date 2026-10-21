use super::*;
use bmz_render::skin::SkinLuaEffect;

#[test]
fn clicked_lua_act_runs_before_the_next_frame_draws_and_reports_effects() {
    let root = unique_test_dir("bmz-select-lua-act");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("open.wav"), b"").unwrap();
    let path = root.join("skin.lua");
    fs::write(
        &path,
        r#"
        local s = require('main_state')
        local open = false
        local last = ''
        return {type=5, w=1280, h=720,
            customTimers={{id=10001},
                {id=10002, timer=function() return open and 0 or s.timer_off_value end}},
            customEvents={{id=1000, action=17}},
            image={{id='button', src=0, x=0, y=0, w=1, h=1, act=function(a, b)
                open = not open
                last = a .. ':' .. b
                s.set_timer(10001, open and s.time() or s.timer_off_value)
                s.audio_play('open.wav')
                s.event_exec(1000, 2, 0)
            end}},
            text={
                {id='panel', size=24, constantText='PANEL'},
                {id='timed', size=24, constantText='TIMED'},
                {id='active', size=24, constantText='ACTIVE'},
                {id='last', size=24, value=function() return last end}
            },
            destination={
                {id='panel', draw=function() return open end, dst={{x=0, y=0, w=100, h=24}}},
                {id='timed', timer=10001, dst={{x=0, y=40, w=100, h=24}}},
                {id='active', timer=10002, dst={{x=0, y=60, w=100, h=24}}},
                {id='last', dst={{x=0, y=80, w=100, h=24}}}
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
        let act = loaded.document.image[0].act.unwrap();
        let mut context = SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            loaded.document,
            [],
        );
        let adapter = Arc::new(LuaSkinDrawRuntimeAdapter::new(loaded.lua_runtime.unwrap()));
        context.set_lua_draw_runtime(Some(adapter.clone()));
        let texts = |context: &SkinContext| {
            let snapshot =
                bmz_render::scene::SelectSnapshot { time: TimeUs(5_000_000), ..Default::default() };
            context.begin_frame();
            context
                .select_document_items(&snapshot)
                .into_iter()
                .filter_map(|item| {
                    if let SkinRenderItem::Text { text, .. } = item { Some(text) } else { None }
                })
                .collect::<Vec<_>>()
        };
        assert!(texts(&context).is_empty(), "{mode:?}");
        assert!(!context.queue_lua_act(17, 1, 0), "built-in events are not Lua acts");
        assert!(context.queue_lua_act(act, -1, 0));
        assert!(context.take_lua_effects().is_empty(), "the act waits for the next frame");
        // The act runs before this frame's custom timers and draw.
        assert_eq!(texts(&context), ["PANEL", "TIMED", "ACTIVE", "-1:0"], "{mode:?}");
        assert_eq!(
            context.take_lua_effects(),
            vec![
                SkinLuaEffect::Audio(bmz_render::skin::SkinAudioActionDef {
                    action: bmz_render::skin::SkinAudioActionKind::Play,
                    path: "open.wav".to_string(),
                    volume: 1.0,
                }),
                SkinLuaEffect::Event { id: 17, arg1: 2, arg2: 0 },
            ],
            "{mode:?}"
        );
        assert!(context.take_lua_effects().is_empty());
        assert_eq!(texts(&context), ["PANEL", "TIMED", "ACTIVE", "-1:0"], "{mode:?}");
        assert!(context.queue_lua_act(act, 1, 0));
        assert_eq!(texts(&context), ["1:0"], "{mode:?}");
        assert_eq!(adapter.runtime.lock().unwrap().as_ref().unwrap().failure_log_count(), 0);
    }
}
