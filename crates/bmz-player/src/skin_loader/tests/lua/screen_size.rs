use super::*;

fn install_skins(renderer: &mut Renderer, mode: bmz_skin::LuaSkinRuntimeMode) {
    let root = unique_test_dir("screen-dimensions-scenes");
    fs::create_dir_all(&root).unwrap();
    for (skin_type, kind) in
        [(5, SkinKind::Select), (6, SkinKind::Decide), (0, SkinKind::Play), (7, SkinKind::Result)]
    {
        let path = root.join(format!("{skin_type}.lua"));
        fs::write(&path, format!(r#"
            local s = require('main_state')
            local width, height = s.screen_width, s.screen_height
            return {{type={skin_type},w=320,h=180,
                text={{{{id='size',size=12,value=function() return width() .. 'x' .. height() end}}}},
                destination={{{{id='size',dst={{{{x=0,y=0,w=300,h=20}}}}}}}}
            }}
        "#)).unwrap();
        let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &path,
            kind,
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
        assert_eq!(decoded.load_dependencies.screen_size, None);
        install_decoded_skin(renderer, decoded, bmz_render::skin::default_skin_manifest()).unwrap();
    }
    fs::remove_dir_all(root).unwrap();
}

fn assert_all_scenes(renderer: &mut Renderer, size: [u32; 2]) {
    let AppSceneSnapshot::Play(play) = bmz_render::sample::sample_play_scene() else {
        unreachable!()
    };
    for scene in [
        bmz_render::sample::sample_select_scene(),
        AppSceneSnapshot::Decide(play.clone()),
        AppSceneSnapshot::Play(play),
        bmz_render::sample::sample_result_scene(),
    ] {
        renderer.prepare_scene(scene);
        assert!(
            renderer.last_plan().unwrap().commands.iter().any(|command| matches!(
                command, DrawCommand::Text { text, .. } if text == &format!("{}x{}",size[0],size[1])
            )),
            "scene must receive current physical dimensions: {:?}",
            renderer.last_plan()
        );
    }
}

#[test]
fn screen_dimensions_are_zero_without_an_attached_target_in_all_scenes() {
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = Renderer::default();
        install_skins(&mut renderer, mode);
        assert_eq!(renderer.render_target_size(), [0, 0]);
        assert_all_scenes(&mut renderer, [0, 0]);
    }
}

#[test]
#[ignore = "requires a GPU; verifies physical output dimensions in every Lua scene"]
fn screen_dimensions_follow_offscreen_output_and_target_replacement_in_all_scenes() {
    use bmz_render::renderer::{InternalResolutionMode, SurfaceSize};
    for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        let mut renderer = Renderer::default();
        renderer.set_internal_resolution_mode(InternalResolutionMode::Skin);
        install_skins(&mut renderer, mode);
        for [width, height] in [[640, 360], [800, 600]] {
            renderer.attach_offscreen(SurfaceSize { width, height }).unwrap();
            assert_eq!(renderer.render_target_size(), [width, height]);
            assert_all_scenes(&mut renderer, [width, height]);
            renderer.render_last_plan().unwrap();
            assert_eq!(
                renderer.read_offscreen_rgba().unwrap().len(),
                (width * height * 4) as usize
            );
        }
        renderer.detach_surface();
        assert_all_scenes(&mut renderer, [0, 0]);
    }
}
