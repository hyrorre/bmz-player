//! ESC hold-to-exit overrides through the real JSON / Lua select skin loaders.
use super::*;
use bmz_render::scene::SelectSnapshot;
use bmz_render::snapshot::ExitHoldSnapshot;

const BUILTIN_LABEL: &str = "HOLD ESC TO EXIT";

fn plan_texts(renderer: &mut Renderer, elapsed_ms: Option<i32>) -> Vec<String> {
    renderer.prepare_scene(AppSceneSnapshot::Select(SelectSnapshot {
        exit_hold: ExitHoldSnapshot { elapsed_ms, duration_ms: 1_200 },
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
        .collect()
}

fn install(path: &Path) -> Renderer {
    let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
        path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
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
fn lua_select_skin_replaces_the_builtin_exit_hold_indicator() {
    let root = unique_test_dir("exit-hold-lua");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lua");
    fs::write(
        &path,
        r#"
            local main_state = require('main_state')
            return {type=5,w=320,h=180,
                bmzExitHold={builtin=false},
                text={
                    {id='exit',size=12,value=function()
                        return 'EXIT ' .. main_state.number(19070)
                    end},
                },
                destination={
                    {id='exit',draw=function() return main_state.option(19073) end,
                        dst={{x=0,y=0,w=300,h=20}}},
                }
            }
        "#,
    )
    .unwrap();

    let mut renderer = install(&path);
    assert_eq!(plan_texts(&mut renderer, None), Vec::<String>::new());
    assert_eq!(plan_texts(&mut renderer, Some(600)), ["EXIT 50"]);
    assert_eq!(plan_texts(&mut renderer, Some(900)), ["EXIT 75"]);
}

#[test]
fn json_select_skin_without_declaration_keeps_the_builtin_indicator() {
    let root = unique_test_dir("exit-hold-json");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.json");
    fs::write(&path, r#"{ "type": 5, "w": 320, "h": 180 }"#).unwrap();

    let mut renderer = install(&path);
    assert!(plan_texts(&mut renderer, None).is_empty());
    assert_eq!(plan_texts(&mut renderer, Some(600)), [BUILTIN_LABEL]);
}
