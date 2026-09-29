use super::*;

#[test]
fn custom_timers_reach_rendering_before_draw_and_are_shared_by_render_passes() {
    let root = unique_test_dir("bmz-render-custom-timers");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.lua");
    fs::write(
        &path,
        r#"
        local s = require('main_state')
        local frames = 0
        return {type=7, w=1280, h=720,
            customTimers={
                {id=10001},
                {id=10002, timer=function()
                    frames=frames+1
                    if s.number(71)>0 then
                        if s.timer(10001)==s.timer_off_value then s.set_timer(10001,s.time()) end
                    else s.set_timer(10001,s.timer_off_value) end
                end}
            },
            text={
                {id='timer',size=24,constantText='ON'},
                {id='counter',size=24,value=function() return tostring(frames) end},
                {id='native',size=24,value=function() return tostring(s.timer(41)) end}
            },
            destination={
                {id='timer',timer=10001,dst={{x=0,y=0,w=100,h=24},{time=1000,x=100}}},
                {id='counter',dst={{x=0,y=40,w=100,h=24}}},
                {id='native',dst={{x=0,y=80,w=100,h=24}}}
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
        let adapter = Arc::new(LuaSkinDrawRuntimeAdapter::new(loaded.lua_runtime.unwrap()));
        context.set_lua_draw_runtime(Some(adapter.clone()));
        for (frame, (ms, score, on)) in
            [(5000, 0, false), (5000, 1, true), (5500, 1, true), (6000, 0, false), (7000, 1, true)]
                .into_iter()
                .enumerate()
        {
            let state = SkinDrawState {
                elapsed_ms: ms,
                ex_score: score,
                play_timer_ms: Some(ms - 1000),
                ..Default::default()
            };
            context.begin_frame();
            for _ in 0..2 {
                let texts = context
                    .static_document_items_for_state(&state)
                    .into_iter()
                    .filter_map(|item| {
                        if let SkinRenderItem::Text { text, .. } = item { Some(text) } else { None }
                    })
                    .collect::<Vec<_>>();
                let mut expected = Vec::new();
                if on {
                    expected.push("ON".to_string());
                }
                expected.push((frame + 1).to_string());
                expected.push("1000000".to_string());
                assert_eq!(texts, expected, "{mode:?} frame={frame}");
            }
        }
        assert_eq!(adapter.runtime.lock().unwrap().as_ref().unwrap().failure_log_count(), 0);
    }
}

#[test]
fn luxe_flat_custom_timer_switches_option_warning_on_current_frame_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/Luxez-Flat/music_select.luaskin");
    if !path.is_file() {
        return;
    }
    let loaded = bmz_skin::load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    assert!(
        !loaded.warnings.iter().any(|warning| warning.message.contains("unsupported custom timer"))
    );
    let timer = loaded
        .document
        .destination
        .iter()
        .find_map(|entry| match entry {
            bmz_render::skin::DestinationListEntry::Single(dst) if dst.id == "warning_dp" => {
                dst.timer
            }
            _ => None,
        })
        .unwrap();
    let mut runtime = loaded.lua_runtime.unwrap();
    for (ms, double, expected) in
        [(100, 0, None), (250, 2, Some(250000)), (300, 2, Some(250000)), (400, 0, None)]
    {
        let state = SkinDrawState {
            elapsed_ms: ms,
            select_double_option_index: double,
            ..Default::default()
        };
        let provider = RenderLuaMainState {
            state: &state,
            enabled_options: &[],
            text_values: &BTreeMap::new(),
        };
        runtime.begin_frame();
        assert_eq!(runtime.advance_custom_timers(&provider)[&timer], expected);
    }
    assert_eq!(runtime.failure_log_count(), 0);
}

#[test]
fn mz_select_custom_timers_run_without_missing_callback_failures_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/mz-select/music_select.luaskin");
    if !path.is_file() {
        return;
    }
    let loaded = bmz_skin::load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    assert!(
        !loaded.warnings.iter().any(|warning| warning.message.contains("unsupported custom timer"))
    );
    let mut runtime = loaded.lua_runtime.unwrap();
    for ms in [0, 100, 200] {
        let state = SkinDrawState { elapsed_ms: ms, ..Default::default() };
        let provider = RenderLuaMainState {
            state: &state,
            enabled_options: &[],
            text_values: &BTreeMap::new(),
        };
        runtime.begin_frame();
        assert!(!runtime.advance_custom_timers(&provider).is_empty());
    }
    assert_eq!(runtime.failure_log_count(), 0);
}
