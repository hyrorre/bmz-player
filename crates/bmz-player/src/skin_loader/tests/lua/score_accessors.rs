use super::*;

fn scene_text(renderer: &mut Renderer, scene: AppSceneSnapshot) -> String {
    renderer.prepare_scene(scene);
    renderer
        .last_plan()
        .unwrap()
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { text, .. } if text.starts_with("gauge=") => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing accessor text: {:?}", renderer.last_plan()))
}

/// `main_state.gauge()` reads the live play gauge and is zero elsewhere, like
/// beatoraja's `BMSPlayer` check. The score accessors match their refs.
#[test]
fn gauge_and_score_accessors_follow_the_rendered_scene() {
    let root = unique_test_dir("lua-score-accessors");
    fs::create_dir_all(&root).unwrap();
    for (skin_type, kind) in [(0, SkinKind::Play), (7, SkinKind::Result)] {
        let path = root.join(format!("{skin_type}.lua"));
        fs::write(
            &path,
            format!(
                r#"
            local s = require('main_state')
            return {{type={skin_type},w=320,h=180,
                text={{{{id='accessors',size=12,value=function()
                    return 'gauge=' .. string.format('%.1f', s.gauge())
                        .. ':' .. tostring(s.rate() == s.float_number(111))
                        .. ':' .. tostring(s.rate_best() == s.float_number(112))
                        .. ':' .. tostring(s.rate_rival() == s.float_number(115))
                        .. ':' .. tostring(s.exscore_best() == s.number(150))
                        .. ':' .. tostring(s.exscore_rival() == s.number(121))
                end}}}},
                destination={{{{id='accessors',dst={{{{x=0,y=0,w=300,h=20}}}}}}}}
            }}
        "#
            ),
        )
        .unwrap();
        let decoded = decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
            &path,
            kind,
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
        let (scene, expected) = if skin_type == 0 {
            (bmz_render::sample::sample_play_scene(), "gauge=82.0:true:true:true:true:true")
        } else {
            (bmz_render::sample::sample_result_scene(), "gauge=0.0:true:true:true:true:true")
        };
        assert_eq!(scene_text(&mut renderer, scene), expected);
    }
}
