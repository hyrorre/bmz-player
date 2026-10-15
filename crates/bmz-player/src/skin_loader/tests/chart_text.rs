//! BMS `#TEXT` refs through the real JSON / Lua / LR2 CSV skin loaders.
use super::*;
use bmz_render::snapshot::RenderSnapshot;

fn play_scene(chart_text: &str) -> AppSceneSnapshot {
    AppSceneSnapshot::Play(RenderSnapshot {
        chart_text: chart_text.into(),
        ready_elapsed_time: Some(TimeUs(0)),
        ..Default::default()
    })
}

fn plan_texts(renderer: &mut Renderer, chart_text: &str) -> Vec<String> {
    renderer.prepare_scene(play_scene(chart_text));
    renderer
        .last_plan()
        .unwrap()
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn install(path: &Path, mode: bmz_skin::LuaSkinRuntimeMode) -> Renderer {
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        path,
        SkinKind::Play,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState { runtime_mode: mode, ..Default::default() },
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer
}

#[test]
fn lua_main_state_text_reads_chart_text_by_id_and_name_every_frame() {
    let root = unique_test_dir("chart-text-lua");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lua");
    fs::write(
        &path,
        r#"
            local main_state = require('main_state')
            return {type=0,w=320,h=180,
                text={
                    {id='by_id',size=12,value=function() return 'ID:' .. main_state.text(20100) end},
                    {id='by_name',size=12,value=function()
                        return 'NAME:' .. main_state.text('bmz_chart_text')
                    end},
                    {id='visible',size=12,constantText='VISIBLE'},
                },
                destination={
                    {id='by_id',dst={{x=0,y=0,w=300,h=20}}},
                    {id='by_name',dst={{x=0,y=20,w=300,h=20}}},
                    {id='visible',draw=function() return main_state.option(20101) end,
                        dst={{x=0,y=40,w=300,h=20}}},
                }
            }
        "#,
    )
    .unwrap();

    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = install(&path, mode);
        assert_eq!(plan_texts(&mut renderer, ""), ["ID:", "NAME:"], "{mode:?}");
        assert_eq!(
            plan_texts(&mut renderer, "Hello World"),
            ["ID:Hello World", "NAME:Hello World", "VISIBLE"],
            "{mode:?}"
        );
        assert_eq!(
            plan_texts(&mut renderer, "日本語"),
            ["ID:日本語", "NAME:日本語", "VISIBLE"],
            "{mode:?}"
        );
        assert_eq!(plan_texts(&mut renderer, ""), ["ID:", "NAME:"], "{mode:?}");
    }
}

#[test]
fn json_skin_file_reads_chart_text_value_name_and_option() {
    let root = unique_test_dir("chart-text-json");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.json");
    fs::write(
        &path,
        r#"{
            "type": 0, "w": 320, "h": 180,
            "text": [
                { "id": "chart_message", "size": 28, "value": "bmz_chart_text" },
                { "id": "visible", "size": 12, "constantText": "VISIBLE" }
            ],
            "destination": [
                { "id": "chart_message", "dst": [{ "x": 0, "y": 0, "w": 300, "h": 30 }] },
                { "id": "visible", "op": [20101], "dst": [{ "x": 0, "y": 40, "w": 300, "h": 20 }] }
            ]
        }"#,
    )
    .unwrap();
    let mut renderer = install(&path, bmz_skin::LuaSkinRuntimeMode::Auto);
    assert!(plan_texts(&mut renderer, "").is_empty());
    assert_eq!(plan_texts(&mut renderer, "JSON"), ["JSON", "VISIBLE"]);
}

#[test]
fn lr2_csv_text_ref_reads_chart_text() {
    let root = unique_test_dir("chart-text-csv");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lr2skin");
    fs::write(
        &path,
        "#INFORMATION,0,Chart Text,Author\n#RESOLUTION,100,100\n\
         #SRC_TEXT,0,0,20100,0,0\n\
         #DST_TEXT,0,0,10,20,80,30,0,255,255,255,255,0,0,0,0,0,0,0,0,0,0\n\
         #SRC_TEXT,0,0,10,0,0\n\
         #DST_TEXT,0,0,10,60,80,30,0,255,255,255,255,0,0,0,0,0,0,20101,0,0\n",
    )
    .unwrap();
    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();
    assert_eq!(decoded.document.text[0].ref_id, 20_100);
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();

    let mut scene = |chart_text: &str| {
        renderer.prepare_scene(AppSceneSnapshot::Play(RenderSnapshot {
            title: "TITLE".to_string(),
            chart_text: chart_text.into(),
            ready_elapsed_time: Some(TimeUs(0)),
            ..Default::default()
        }));
        renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(scene("").is_empty());
    assert_eq!(scene("CSV"), ["CSV", "TITLE"]);
}

fn timer_scene(elapsed: Option<i32>) -> AppSceneSnapshot {
    AppSceneSnapshot::Play(RenderSnapshot {
        chart_text: "TIMER".into(),
        chart_text_event_elapsed_ms: elapsed,
        time: TimeUs(5_000_000),
        play_elapsed_time: TimeUs(5_000_000),
        ready_elapsed_time: Some(TimeUs(0)),
        ..Default::default()
    })
}

fn scene_texts(renderer: &mut Renderer, scene: AppSceneSnapshot) -> Vec<String> {
    renderer.prepare_scene(scene);
    renderer
        .last_plan()
        .unwrap()
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn lua_main_state_reads_chart_text_event_timer() {
    let root = unique_test_dir("chart-text-lua-timer");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lua");
    fs::write(
        &path,
        r#"
            local main_state = require('main_state')
            return {type=0,w=320,h=180,
                text={{id='timer',size=12,value=function()
                    return 'T:' .. main_state.timer_elapsed_ms(20102)
                        .. (main_state.timer_is_off(20102) and ':OFF' or ':ON')
                end}},
                destination={{id='timer',dst={{x=0,y=0,w=300,h=20}}}}
            }
        "#,
    )
    .unwrap();
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = install(&path, mode);
        assert_eq!(scene_texts(&mut renderer, timer_scene(None)), ["T:-1:OFF"], "{mode:?}");
        assert_eq!(scene_texts(&mut renderer, timer_scene(Some(250))), ["T:250:ON"], "{mode:?}");
        // A repeated event restarts the timer from zero.
        assert_eq!(scene_texts(&mut renderer, timer_scene(Some(0))), ["T:0:ON"], "{mode:?}");
    }
}

#[test]
fn lr2_csv_destination_uses_chart_text_event_timer() {
    let root = unique_test_dir("chart-text-csv-timer");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lr2skin");
    fs::write(
        &path,
        "#INFORMATION,0,Chart Text Timer,Author\n#RESOLUTION,100,100\n\
         #SRC_TEXT,0,0,20100,0,0\n\
         #DST_TEXT,0,0,10,20,80,30,0,255,255,255,255,0,0,0,0,0,20102,0,0,0\n",
    )
    .unwrap();
    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    assert!(scene_texts(&mut renderer, timer_scene(None)).is_empty());
    assert_eq!(scene_texts(&mut renderer, timer_scene(Some(100))), ["TIMER"]);
}

#[test]
fn lua_main_state_reads_miss_text_definition_option_and_timer() {
    let root = unique_test_dir("chart-text-lua-miss");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lua");
    fs::write(
        &path,
        r#"
            local s = require('main_state')
            return {type=0,w=320,h=180,
                text={{id='miss',size=12,value=function()
                    return s.text(20100) .. '/' .. s.text('bmz_chart_miss_text') .. '/'
                        .. tostring(s.option(20104)) .. '/'
                        .. tostring(s.option('bmz_chart_miss_text_active')) .. '/'
                        .. s.timer_elapsed_ms(20105)
                end}},
                destination={{id='miss',dst={{x=0,y=0,w=300,h=20}}}}
            }
        "#,
    )
    .unwrap();
    let scene = |shown: &str, elapsed: Option<i32>| {
        AppSceneSnapshot::Play(RenderSnapshot {
            chart_text: shown.into(),
            chart_miss_text: "MISS!!".into(),
            chart_miss_text_elapsed_ms: elapsed,
            time: TimeUs(5_000_000),
            play_elapsed_time: TimeUs(5_000_000),
            ready_elapsed_time: Some(TimeUs(0)),
            ..Default::default()
        })
    };
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = install(&path, mode);
        assert_eq!(
            scene_texts(&mut renderer, scene("Normal", None)),
            ["Normal/MISS!!/false/false/-1"],
            "{mode:?}"
        );
        assert_eq!(
            scene_texts(&mut renderer, scene("MISS!!", Some(120))),
            ["MISS!!/MISS!!/true/true/120"],
            "{mode:?}"
        );
    }
}

#[test]
fn documented_chart_text_skin_sample_renders() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples/chart-text/play-chart-text.json");
    let mut renderer = install(&path, bmz_skin::LuaSkinRuntimeMode::Auto);
    let scene = |shown: &str, miss: Option<i32>| {
        AppSceneSnapshot::Play(RenderSnapshot {
            chart_text: shown.into(),
            chart_text_event_elapsed_ms: Some(400),
            chart_miss_text: "MISS!!".into(),
            chart_miss_text_elapsed_ms: miss,
            ready_elapsed_time: Some(TimeUs(0)),
            ..Default::default()
        })
    };
    assert_eq!(scene_texts(&mut renderer, scene("Hello World", None)), ["Hello World"]);
    assert_eq!(scene_texts(&mut renderer, scene("MISS!!", Some(50))), ["MISS!!"]);
    assert!(scene_texts(&mut renderer, scene("", None)).is_empty());
}

fn overlay_scene(chart_text: &str) -> AppSceneSnapshot {
    AppSceneSnapshot::Play(RenderSnapshot {
        chart_text: chart_text.into(),
        chart_text_overlay: true,
        ready_elapsed_time: Some(TimeUs(0)),
        ..Default::default()
    })
}

fn write_lua(name: &str, body: &str) -> (crate::test_dir::TestDir, PathBuf) {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("play.lua");
    fs::write(&path, body).unwrap();
    (root, path)
}

#[test]
fn lua_functions_that_read_chart_text_suppress_the_overlay_from_the_first_frame() {
    // A concatenating, option-gated function stays a runtime callback in both modes,
    // so only the runtime read can tell that the skin shows the chart text.
    let (_root, path) = write_lua(
        "chart-text-lua-overlay",
        r#"
            local s = require('main_state')
            return {type=0,w=320,h=180,
                text={{id='m',size=12,value=function() return 'M:' .. s.text(20100) end}},
                destination={{id='m',draw=function() return s.option(20101) end,
                    dst={{x=0,y=0,w=300,h=20}}}}
            }
        "#,
    );
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = install(&path, mode);
        assert!(scene_texts(&mut renderer, overlay_scene("")).is_empty(), "{mode:?}");
        // The callback runs before the overlay is decided in the same frame.
        assert_eq!(scene_texts(&mut renderer, overlay_scene("Hello")), ["M:Hello"], "{mode:?}");
        assert_eq!(scene_texts(&mut renderer, overlay_scene("")), Vec::<String>::new());
        assert_eq!(scene_texts(&mut renderer, overlay_scene("Next")), ["M:Next"], "{mode:?}");
    }
}

#[test]
fn lua_skins_without_chart_text_use_the_overlay() {
    let (_root, path) = write_lua(
        "chart-text-lua-no-ref",
        r#"
            local s = require('main_state')
            return {type=0,w=320,h=180,
                text={{id='t',size=12,value=function() return 'T:' .. s.text(10) end}},
                destination={{id='t',dst={{x=0,y=0,w=300,h=20}}}}
            }
        "#,
    );
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = install(&path, mode);
        assert_eq!(scene_texts(&mut renderer, overlay_scene("Hello")), ["T:", "Hello"], "{mode:?}");
    }
}
