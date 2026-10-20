//! BMZ `ref=19250` key filter index through the real JSON / Lua select skin loaders.
use super::*;

const IMAGE_IDS: [&str; 13] =
    ["all", "5k", "7k", "10k", "14k", "9k", "24k", "48k", "single", "double", "4k", "6k", "8k"];

fn install(path: &Path) -> (Renderer, SkinDocument) {
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
    let document = decoded.document.clone();
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    (renderer, document)
}

fn prepare(renderer: &mut Renderer, mode: &str) -> Vec<DrawCommand> {
    renderer.prepare_scene(AppSceneSnapshot::Select(SelectSnapshot {
        select_mode: mode.to_string(),
        ..Default::default()
    }));
    renderer.last_plan().unwrap().commands.clone()
}

/// Index of the 10px cell drawn from the 130x10 `mode.png` strip.
fn drawn_cells(commands: &[DrawCommand]) -> Vec<usize> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { uv, .. } if (uv.width * 13.0 - 1.0).abs() < 0.001 => {
                Some((uv.x * 13.0).round() as usize)
            }
            _ => None,
        })
        .collect()
}

fn texts(commands: &[DrawCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn write_mode_strip(root: &Path) {
    image::RgbaImage::from_pixel(130, 10, image::Rgba([255, 255, 255, 255]))
        .save(root.join("mode.png"))
        .unwrap();
}

fn json_images() -> String {
    IMAGE_IDS
        .iter()
        .enumerate()
        .map(|(index, id)| {
            format!(r#"{{"id":"{id}","src":"mode","x":{},"y":0,"w":10,"h":10}}"#, index * 10)
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn quoted_ids() -> String {
    IMAGE_IDS.iter().map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(",")
}

#[test]
fn json_select_skin_selects_4k_6k_8k_images_with_ref_19250_and_named_value() {
    let root = unique_test_dir("select-mode-index-json");
    fs::create_dir_all(&root).unwrap();
    write_mode_strip(&root);
    for (file, imageset) in [
        ("ref.json", r#""ref": 19250"#.to_string()),
        ("value.json", r#""value": "bmz_select_mode_index""#.to_string()),
    ] {
        let path = root.join(file);
        fs::write(
            &path,
            format!(
                r#"{{
                    "type": 5, "w": 320, "h": 180,
                    "source": [{{"id": "mode", "path": "mode.png"}}],
                    "image": [{}],
                    "imageset": [{{"id": "key_mode_filter", {imageset}, "images": [{}]}}],
                    "destination": [
                        {{"id": "key_mode_filter", "dst": [{{"x": 0, "y": 0, "w": 10, "h": 10}}]}}
                    ]
                }}"#,
                json_images(),
                quoted_ids()
            ),
        )
        .unwrap();

        let (mut renderer, document) = install(&path);
        assert_eq!(document.imageset[0].images.len(), 13, "{file}");
        for (mode, expected) in
            [("ALL", 0), ("7K", 2), ("14K", 4), ("4K", 10), ("6K", 11), ("8K", 12), ("4K", 10)]
        {
            assert_eq!(drawn_cells(&prepare(&mut renderer, mode)), [expected], "{file}: {mode}");
        }
    }
}

#[test]
fn json_select_skin_ref_11_keeps_mapping_4k_6k_8k_to_all() {
    let root = unique_test_dir("select-mode-index-json-ref11");
    fs::create_dir_all(&root).unwrap();
    write_mode_strip(&root);
    let path = root.join("select.json");
    fs::write(
        &path,
        format!(
            r#"{{
                "type": 5, "w": 320, "h": 180,
                "source": [{{"id": "mode", "path": "mode.png"}}],
                "image": [{}],
                "imageset": [{{"id": "legacy", "ref": 11,
                    "images": ["all","5k","7k","10k","14k","9k","24k","48k"]}}],
                "destination": [{{"id": "legacy", "dst": [{{"x": 0, "y": 0, "w": 10, "h": 10}}]}}]
            }}"#,
            json_images()
        ),
    )
    .unwrap();

    let (mut renderer, _) = install(&path);
    for (mode, expected) in [
        ("ALL", 0),
        ("5K", 1),
        ("7K", 2),
        ("10K", 3),
        ("14K", 4),
        ("9K", 5),
        ("4K", 0),
        ("6K", 0),
        ("8K", 0),
    ] {
        assert_eq!(drawn_cells(&prepare(&mut renderer, mode)), [expected], "{mode}");
    }
}

#[test]
fn lua_select_skin_reads_ref_19250_from_imageset_number_and_event_index() {
    let root = unique_test_dir("select-mode-index-lua");
    fs::create_dir_all(&root).unwrap();
    write_mode_strip(&root);
    let path = root.join("select.lua");
    let images = IMAGE_IDS
        .iter()
        .enumerate()
        .map(|(index, id)| format!("{{id='{id}',src='mode',x={},y=0,w=10,h=10}}", index * 10))
        .collect::<Vec<_>>()
        .join(",");
    let ids = IMAGE_IDS.iter().map(|id| format!("'{id}'")).collect::<Vec<_>>().join(",");
    fs::write(
        &path,
        format!(
            r#"
                local main_state = require('main_state')
                local skin = {{type=5,w=320,h=180}}
                skin.source = {{{{id='mode',path='mode.png'}}}}
                skin.image = {{{images}}}
                skin.imageset = {{
                    {{id='key_mode_filter',ref=19250,images={{{ids}}}}},
                }}
                skin.text = {{
                    {{id='mode_values',size=12,value=function()
                        return main_state.number(19250) .. ',' .. main_state.event_index(19250)
                            .. ',' .. main_state.number(11)
                    end}},
                }}
                skin.destination = {{
                    {{id='key_mode_filter',dst={{{{x=0,y=0,w=10,h=10}}}}}},
                    {{id='mode_values',dst={{{{x=0,y=20,w=300,h=20}}}}}},
                }}
                return skin
            "#
        ),
    )
    .unwrap();

    let (mut renderer, document) = install(&path);
    let imageset = document.imageset.iter().find(|set| set.id == "key_mode_filter").unwrap();
    assert_eq!(imageset.ref_id, 19_250);
    assert_eq!(imageset.images, IMAGE_IDS);
    for (mode, index, legacy) in
        [("ALL", 0, 0), ("7K", 2, 2), ("4K", 10, 0), ("6K", 11, 0), ("8K", 12, 0), ("14K", 4, 4)]
    {
        let commands = prepare(&mut renderer, mode);
        assert_eq!(drawn_cells(&commands), [index], "{mode}");
        assert_eq!(texts(&commands), [format!("{index},{index},{legacy}")], "{mode}");
    }
}
