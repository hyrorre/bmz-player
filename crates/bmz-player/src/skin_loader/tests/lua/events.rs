use super::*;
use bmz_render::skin::{SkinClickTarget, SkinLuaEffect};

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
        let effects = context.take_lua_effects();
        let [SkinLuaEffect::Audio(audio), event] = effects.as_slice() else {
            panic!("{mode:?}: {effects:?}");
        };
        assert_eq!(audio.action, bmz_render::skin::SkinAudioActionKind::Play);
        assert_eq!(audio.volume, 1.0);
        // The app decodes the checked file without the skin's path context.
        let audio_path = Path::new(&audio.path);
        assert!(audio_path.is_absolute() && audio_path.ends_with("open.wav"), "{audio_path:?}");
        assert_eq!(*event, SkinLuaEffect::Event { id: 17, arg1: 2, arg2: 0 });
        assert!(context.take_lua_effects().is_empty());
        assert_eq!(texts(&context), ["PANEL", "TIMED", "ACTIVE", "-1:0"], "{mode:?}");
        assert!(context.queue_lua_act(act, 1, 0));
        assert_eq!(texts(&context), ["1:0"], "{mode:?}");
        assert_eq!(adapter.runtime.lock().unwrap().as_ref().unwrap().failure_log_count(), 0);
    }
}

#[test]
fn lua_string_act_and_destination_timer_render_and_keep_last_frame_values() {
    let root = unique_test_dir("bmz-lua-script-properties");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.luaskin");
    let (context, document) = install_lua_skin(
        &path,
        r#"
        local s = require('main_state')
        local opened = false
        function switchMenu() opened = not opened end
        function panelTimer() return opened and s.time() - 250000 or s.timer_off_value end
        return {type=5, w=100, h=100, image={{id='btn', act='switchMenu()'}},
            text={{id='panel', size=10, constantText='PANEL'}},
            destination={{id='panel', act='switchMenu()', timer='panelTimer', loop=1000,
                dst={{time=0,x=0,y=0,w=20,h=10},{time=1000,x=80}}}}}
    "#,
    );
    let snapshot =
        bmz_render::scene::SelectSnapshot { time: TimeUs(5_000_000), ..Default::default() };
    context.begin_frame();
    assert!(context.select_document_items(&snapshot).is_empty());
    assert!(context.queue_lua_act(document.image[0].act.unwrap(), 1, 0));
    context.begin_frame();
    let items = context.select_document_items(&snapshot);
    let panel = items
        .iter()
        .find_map(|item| {
            if let SkinRenderItem::Text { origin, text, .. } = item {
                (text == "PANEL").then_some(origin)
            } else {
                None
            }
        })
        .unwrap();
    assert!((panel.x - 0.2).abs() < 0.001);
    let hit = context.select_click_hit(&snapshot, 0.25, 0.95);
    assert!(hit.is_some(), "the timer-gated panel uses its last rendered start time");
}

#[test]
fn lua_slider_events_drag_the_selected_writer_with_last_frame_visibility() {
    let root = unique_test_dir("bmz-lua-slider-writer");
    fs::create_dir_all(&root).unwrap();
    let (context, document) = install_lua_skin(
        &root.join("skin.luaskin"),
        r#"
        local shown, a, b = false, 0, 0
        return {type=5, w=100, h=100,
            image={{id='open', act=function() shown = not shown end}},
            slider={
                {id='a', range=80, angle=1, value=function() return a end, event=function(v) a=v end},
                {id='b', range=80, angle=1, value=function() return b end, event=function(v) b=v end}},
            text={{id='values', size=10, value=function() return string.format('%.2f:%.2f',a,b) end}},
            destination={
                {id='a', draw=function() return shown end, dst={{x=10,y=0,w=10,h=10}}},
                {id='b', draw=function() return shown end, dst={{x=10,y=20,w=10,h=10}}},
                {id='values', dst={{x=0,y=40,w=100,h=10}}}}}
    "#,
    );
    let snapshot =
        bmz_render::scene::SelectSnapshot { time: TimeUs(5_000_000), ..Default::default() };
    let frame = || {
        context.begin_frame();
        context
            .select_document_items(&snapshot)
            .into_iter()
            .find_map(
                |item| if let SkinRenderItem::Text { text, .. } = item { Some(text) } else { None },
            )
            .unwrap()
    };
    assert_eq!(frame(), "0.00:0.00");
    assert!(context.select_slider_hit(&snapshot, 0.30, 0.95).is_none());
    assert!(context.queue_lua_act(document.image[0].act.unwrap(), 1, 0));
    frame();
    let first = context.select_slider_hit(&snapshot, 0.30, 0.95).unwrap();
    assert!((first.value - 0.25).abs() < 0.0001);
    assert!(context.queue_lua_float_writer(first.slider_type, first.value));
    assert_eq!(frame(), "0.25:0.00");
    assert!(
        context.select_slider_drag(&snapshot, first.slider_type, 0.70, 0.75).is_none(),
        "dragging one writer must not target another slider with the same type"
    );
    let second = context.select_slider_hit(&snapshot, 0.70, 0.75).unwrap();
    assert_ne!(first.slider_type, second.slider_type);
    assert!(context.queue_lua_float_writer(second.slider_type, second.value));
    assert_eq!(frame(), "0.25:0.75");
    assert!(context.queue_lua_act(document.image[0].act.unwrap(), 1, 0));
    frame();
    assert!(context.select_slider_hit(&snapshot, 0.70, 0.75).is_none());
}

fn install_lua_skin(path: &Path, source: &str) -> (SkinContext, SkinDocument) {
    fs::write(path, source).unwrap();
    let loaded = bmz_skin::load_lua_skin_with_runtime_state(
        path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    let document = loaded.document.clone();
    let mut context = SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        loaded.document,
        [],
    );
    context.set_lua_draw_runtime(Some(Arc::new(LuaSkinDrawRuntimeAdapter::new(
        loaded.lua_runtime.unwrap(),
    ))));
    (context, document)
}

fn image_act(document: &SkinDocument, id: &str) -> i32 {
    document.image.iter().find(|image| image.id == id).and_then(|image| image.act).unwrap()
}

#[test]
fn select_clicks_use_the_lua_draws_and_timers_of_the_last_drawn_frame() {
    let root = unique_test_dir("bmz-select-lua-act-hit");
    fs::create_dir_all(&root).unwrap();
    let (context, document) = install_lua_skin(
        &root.join("skin.lua"),
        r#"
        local s = require('main_state')
        local open = false
        local draws = 0
        return {type=5, w=100, h=100,
            customTimers={{id=10001}},
            image={
                {id='open', src=0, x=0, y=0, w=1, h=1, act=function()
                    open = true; s.set_timer(10001, s.time())
                end},
                {id='close', src=0, x=0, y=0, w=1, h=1, act=function() open = false end}},
            text={{id='draws', size=10, value=function() return tostring(draws) end}},
            destination={
                {id='open', dst={{x=0, y=0, w=10, h=10}}},
                {id='close', timer=10001,
                    draw=function() draws = draws + 1; return open end,
                    dst={{x=50, y=0, w=10, h=10}}},
                {id='draws', dst={{x=0, y=50, w=10, h=10}}}}}
        "#,
    );
    let (open, close) = (image_act(&document, "open"), image_act(&document, "close"));
    let snapshot =
        bmz_render::scene::SelectSnapshot { time: TimeUs(5_000_000), ..Default::default() };
    let frame = || {
        context.begin_frame();
        context
            .select_document_items(&snapshot)
            .into_iter()
            .filter_map(|item| match item {
                SkinRenderItem::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let hit = |x, y| {
        context.select_click_hit(&snapshot, x, y).map(|hit| match hit.target {
            SkinClickTarget::Event { event_id, .. } => event_id,
            other => panic!("{other:?}"),
        })
    };
    assert_eq!(frame(), ["1"]);
    assert_eq!(hit(0.05, 0.95), Some(open));
    // The close button was not drawn, so it takes no click.
    assert_eq!(hit(0.55, 0.95), None);
    assert!(context.queue_lua_act(open, 1, 0));
    assert_eq!(frame(), ["2"]);
    // Drawn with its custom timer on in the last frame: now it is clickable.
    assert_eq!(hit(0.55, 0.95), Some(close));
    // Hit tests read the last frame and never run the draw callback again.
    assert_eq!(frame(), ["3"]);
}

#[test]
fn decide_and_play_clicks_hit_only_lua_acts_of_the_last_drawn_frame() {
    let root = unique_test_dir("bmz-decide-lua-act-hit");
    fs::create_dir_all(&root).unwrap();
    for skin_type in [6, 0] {
        let (context, document) = install_lua_skin(
            &root.join(format!("skin{skin_type}.lua")),
            &format!(
                r#"
                local shown = true
                return {{type={skin_type}, w=100, h=100,
                    image={{
                        {{id='lua', src=0, x=0, y=0, w=1, h=1,
                            act=function() shown = not shown end}},
                        {{id='builtin', src=0, x=0, y=0, w=1, h=1, act=15}}}},
                    destination={{
                        {{id='lua', draw=function() return shown end,
                            dst={{{{x=0, y=0, w=10, h=10}}}}}},
                        {{id='lua', dst={{{{x=50, y=0, w=10, h=10}}}}}},
                        {{id='builtin', dst={{{{x=50, y=0, w=10, h=10}}}}}}}}}}
                "#
            ),
        );
        let act = image_act(&document, "lua");
        let state = SkinDrawState { elapsed_ms: 1_000, ..SkinDrawState::default() };
        let frame = || {
            context.begin_frame();
            context.static_document_items_split_for_state_and_text(
                &state,
                &bmz_render::skin::SkinTextState::default(),
            );
        };
        let hit = |x, y| {
            context.last_frame_lua_act_hit(x, y).map(|hit| match hit.target {
                SkinClickTarget::Event { event_id, .. } => event_id,
                other => panic!("{other:?}"),
            })
        };
        assert_eq!(hit(0.05, 0.95), None, "type {skin_type}: nothing drawn yet");
        frame();
        assert_eq!(hit(0.05, 0.95), Some(act), "type {skin_type}");
        // A built-in act drawn on top takes the click; these scenes have no action for it.
        assert_eq!(hit(0.55, 0.95), None, "type {skin_type}");
        assert!(context.queue_lua_act(act, 1, 0));
        frame();
        assert_eq!(hit(0.05, 0.95), None, "type {skin_type}: hidden by the act");
    }
}

#[test]
fn timer_audio_effects_reach_the_app_adapter_in_every_scene() {
    let root = unique_test_dir("bmz-lua-timer-audio-scenes");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("se.wav"), b"").unwrap();
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        for (skin_type, kind, scene) in [
            (5, SkinKind::Select, bmz_render::sample::sample_select_scene()),
            (
                6,
                SkinKind::Decide,
                match bmz_render::sample::sample_play_scene() {
                    AppSceneSnapshot::Play(snapshot) => AppSceneSnapshot::Decide(snapshot),
                    _ => unreachable!(),
                },
            ),
            (0, SkinKind::Play, bmz_render::sample::sample_play_scene()),
            (7, SkinKind::Result, bmz_render::sample::sample_result_scene()),
            (15, SkinKind::Result, bmz_render::sample::sample_result_scene()),
        ] {
            // The sample Play/Result scenes start at scene time zero, where
            // STARTINPUT is still off. Exercise an entered scene instead.
            let mut scene = scene;
            match &mut scene {
                AppSceneSnapshot::Play(snapshot) | AppSceneSnapshot::Decide(snapshot) => {
                    snapshot.play_elapsed_time = TimeUs(1_000_000);
                }
                AppSceneSnapshot::Result(snapshot) => snapshot.elapsed_time = TimeUs(1_000_000),
                AppSceneSnapshot::Select(_) => {}
            }
            let path = root.join(format!("skin{skin_type}.lua"));
            fs::write(
                &path,
                format!(
                    r#"
                local s=require('main_state')
                local t=require('timer_util')
                return {{type={skin_type},w=320,h=180,
                    customEvents={{{{id=1000,
                        condition=function() return t.is_timer_on(s.timer(1)) end,
                        action=function() s.audio_play('se.wav') end}}}},
                    text={{{{id='label',size=12,constantText='AUDIO'}}}},
                    destination={{{{id='label',dst={{{{x=0,y=0,w=100,h=20}}}}}}}}}}
            "#
                ),
            )
            .unwrap();
            let loaded = bmz_skin::load_lua_skin_with_runtime_state(
                &path,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
            )
            .unwrap();
            let mut renderer = Renderer::default();
            set_decoded_skin_context(
                &mut renderer,
                kind,
                bmz_render::skin::default_skin_manifest(),
                loaded.document,
                loaded.lua_runtime,
                vec![],
                false,
            );
            let runtime_scene = match kind {
                SkinKind::Select => bmz_render::renderer::SkinRuntimeScene::Select,
                SkinKind::Decide => bmz_render::renderer::SkinRuntimeScene::Decide,
                SkinKind::Play => bmz_render::renderer::SkinRuntimeScene::Play,
                SkinKind::Result => bmz_render::renderer::SkinRuntimeScene::Result,
            };
            for _ in 0..2 {
                renderer.prepare_scene(scene.clone());
                let effects = renderer.take_skin_lua_effects(runtime_scene);
                assert!(
                    matches!(effects.as_slice(), [SkinLuaEffect::Audio(_)]),
                    "type={skin_type} mode={mode:?}: {effects:?}"
                );
                assert!(renderer.take_skin_lua_effects(runtime_scene).is_empty());
            }
        }
    }
}
