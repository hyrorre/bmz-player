use super::*;
use bmz_render::skin::BlendMode;
use ecfn_detail_options::{decode, event_at, skin_path, switching};

fn renderer(experimental: bool) -> Renderer {
    let decoded = decode(experimental);
    assert!(decoded.document.text.iter().any(|t| t.id == "bmz_select_mode"));
    assert_eq!(decoded.document.uses_detail_options(), experimental);
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer
}

fn snapshot() -> SelectSnapshot {
    SelectSnapshot { time: TimeUs(2_000_000), ..Default::default() }
}

// Identify the first strip of the original additive cursor by its source pixels.
// This checks the rendered selection, rather than merely the Lua definitions.
fn cursors(renderer: &Renderer) -> Vec<(f32, f32, f32)> {
    renderer
        .last_plan()
        .unwrap()
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { rect, uv, tint, blend: BlendMode::Add, .. }
                if (uv.x * 3993.0 - 933.0).abs() < 0.1
                    && (uv.y * 2318.0 - 1147.0).abs() < 0.1
                    && (rect.width * 1920.0 - 175.0).abs() < 0.1
                    && (rect.height * 1080.0 - 12.0).abs() < 0.1 =>
            {
                Some((rect.x * 1920.0, rect.y * 1080.0, tint.a))
            }
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_extensions_mode_and_force_preserve_button_hits() {
    for experimental in [false, true] {
        let mut renderer = renderer(experimental);
        for mode in ["ALL", "7K", "14K", "9K", "5K", "10K", "4K", "6K", "8K"] {
            for ln in 0..6 {
                let s = SelectSnapshot {
                    select_mode: mode.into(),
                    ln_policy_setting_index: ln,
                    // FORCE reflects the setting, independently of the score context.
                    ln_score_policy_index: Some((ln + 3) % 6),
                    ..snapshot()
                };
                renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
                let commands = &renderer.last_plan().unwrap().commands;
                assert_eq!(
                    commands
                        .iter()
                        .filter(|c| matches!(c, DrawCommand::Text { text, .. }
                        if text == mode))
                        .count(),
                    1,
                    "actual mode name must appear exactly once: {mode}"
                );
                assert_eq!(
                    commands
                        .iter()
                        .filter(|c| matches!(c, DrawCommand::Text { text, .. }
                        if text == "FORCE"))
                        .count(),
                    usize::from(ln >= 3)
                );
                for fraction in [0.01, 0.5, 0.99] {
                    for (left, event) in [(1364.0, 11), (1538.0, 12), (1711.0, 308)] {
                        assert_eq!(
                            event_at(&renderer, &s, left + 158.0 * fraction, 96.0),
                            Some(event)
                        );
                    }
                }
                assert_eq!(event_at(&renderer, &s, 1790.0, 83.0), Some(308));
            }
        }
    }
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_extensions_twelve_arrangements_and_panel_transitions() {
    let arrangements = [
        "OFF",
        "MIRROR",
        "RANDOM",
        "R-RANDOM",
        "S-RANDOM",
        "SPIRAL",
        "H-RANDOM",
        "ALL-SCR",
        "RANDOM-EX",
        "S-RANDOM-EX",
        "F-RANDOM",
        "MF-RANDOM",
    ];
    for experimental in [false, true] {
        let mut renderer = renderer(experimental);
        for index in 0..12 {
            let mut s = SelectSnapshot {
                option_panel: 1,
                option_panel_time: TimeUs(200_000),
                arrange: arrangements[index].into(),
                arrange_2p: arrangements[11 - index].into(),
                ..snapshot()
            };
            for (time, alpha) in [(200, 1.0), (1200, 170.0 / 255.0), (2200, 1.0)] {
                s.option_panel_time = TimeUs(time * 1000);
                renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
                let selected = cursors(&renderer);
                assert_eq!(selected.len(), 2, "{experimental}: {index}");
                for (side, (x, row)) in
                    [(534.0, index), (1197.0, 11 - index)].into_iter().enumerate()
                {
                    let actual = selected[side];
                    assert!((actual.0 - x).abs() < 0.1);
                    assert!((actual.1 - (511 + row * 35) as f32).abs() < 0.1);
                    assert!((actual.2 - alpha).abs() < 0.01);
                }
            }
            s.option_panel = 0;
            renderer.prepare_scene(AppSceneSnapshot::Select(s));
            assert!(cursors(&renderer).is_empty());
        }
        if experimental {
            for (from, to) in [(1, 2), (2, 1), (1, 0)] {
                for time in [0, 100, 200, 300] {
                    let mut s = switching(from, to, time);
                    s.arrange = "F-RANDOM".into();
                    s.arrange_2p = "MF-RANDOM".into();
                    renderer.prepare_scene(AppSceneSnapshot::Select(s));
                    let expected = if to == 1 && time > 200 {
                        (255.0 - 85.0 * (time - 200) as f32 / 1000.0) / 255.0
                    } else if to == 1 {
                        time as f32 / 200.0
                    } else {
                        1.0 - time.min(200) as f32 / 200.0
                    };
                    let selected = cursors(&renderer);
                    if expected == 0.0 {
                        assert!(selected.iter().all(|v| v.2 == 0.0));
                    } else {
                        assert_eq!(selected.len(), 2);
                        assert!(
                            selected.iter().all(|v| (v.2 - expected).abs() < 0.01),
                            "{from} -> {to} at {time}ms: {selected:?}, expected {expected}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_extensions_atomic_fallback_is_independent_of_detail_options() {
    let source = skin_path();
    let source = source.parent().unwrap();
    for experimental in [false, true] {
        for state in ["loaded", "missing", "failed", "non-bmz"] {
            let root = unique_test_dir("bmz-ecfn-extension-fallback");
            fs::create_dir_all(&root).unwrap();
            for name in ["select.lua", "bmz_detail_options.lua"] {
                fs::copy(source.join(name), root.join(name)).unwrap();
            }
            if state != "missing" {
                let code = fs::read_to_string(source.join("bmz_select_extensions.lua")).unwrap();
                let code = if state == "failed" {
                    code.replace(
                        "\treturn skin\n",
                        "\terror('construction failed')\n\treturn skin\n",
                    )
                } else {
                    code
                };
                fs::write(root.join("bmz_select_extensions.lua"), code).unwrap();
            }
            fs::write(
                root.join("select.luaskin"),
                format!(
                    "{} local t = require('select'); if skin_config then return t.main() else return t.header end",
                    if state == "non-bmz" { "bmz = nil;" } else { "" }
                ),
            )
            .unwrap();
            let loaded = bmz_skin::load_lua_skin(
                &root.join("select.luaskin"),
                bmz_skin::SkinKind::Select,
                &BTreeMap::from([(
                    "bmz_detail_options".into(),
                    if experimental { "1" } else { "0" }.into(),
                )]),
                &BTreeMap::new(),
            )
            .unwrap();
            let basic = state == "loaded";
            assert_eq!(loaded.document.text.iter().any(|t| t.id == "bmz_select_mode"), basic);
            assert_eq!(loaded.document.image.iter().any(|i| i.id.starts_with("ecfn_bmz_")), basic);
            assert_eq!(loaded.document.font.iter().any(|f| f.id == "ecfn_bmz_font"), basic);
            assert_eq!(loaded.document.uses_detail_options(), experimental && state != "non-bmz");
            for id in
                ["modeset", "option-detail1", "option-detail4", "option-random", "option-random2"]
            {
                assert_eq!(
                    loaded.document.destination.iter().any(|entry| matches!(entry,
                    DestinationListEntry::Single(d) if d.id == id)),
                    !basic
                );
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}

#[test]
#[ignore = "requires ADFX02/ECFN assets and a GPU; writes previews to a temporary directory"]
fn ecfn_extensions_gpu_previews() {
    let output = unique_test_dir("bmz-ecfn-extensions-preview");
    fs::create_dir_all(&output).unwrap();
    for (width, height) in [(1920, 1080), (1280, 720), (960, 540), (1024, 768), (2560, 1080)] {
        let mut renderer = renderer(true);
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        renderer.attach_offscreen(bmz_render::renderer::SurfaceSize { width, height }).unwrap();
        for (mode, ln, panel, first, second) in [
            ("4K", 3, 0, "OFF", "MIRROR"),
            ("6K", 4, 0, "RANDOM", "R-RANDOM"),
            ("8K", 5, 0, "F-RANDOM", "MF-RANDOM"),
            ("14K", 2, 1, "F-RANDOM", "MF-RANDOM"),
            ("7K", 0, 1, "MF-RANDOM", "F-RANDOM"),
        ] {
            let s = SelectSnapshot {
                select_mode: mode.into(),
                ln_policy_setting_index: ln,
                select_ln_mode: [
                    "AUTO(LN)",
                    "AUTO(CN)",
                    "AUTO(HCN)",
                    "FORCE(LN)",
                    "FORCE(CN)",
                    "FORCE(HCN)",
                ][ln]
                    .into(),
                option_panel: panel,
                option_panel_time: TimeUs(200_000),
                arrange: first.into(),
                arrange_2p: second.into(),
                player_name: "BMZ Player".into(),
                selected_title: "Sample song".into(),
                rows: vec![SelectRowSnapshot { title: "Sample song".into(), ..Default::default() }],
                ..snapshot()
            };
            renderer.render_scene(AppSceneSnapshot::Select(s)).unwrap();
            image::RgbaImage::from_raw(width, height, renderer.read_offscreen_rgba().unwrap())
                .unwrap()
                .save(output.join(format!("ecfn-{mode}-{width}x{height}.png")))
                .unwrap();
        }
    }
    println!("ECFN BMZ extension previews: {}", output.display());
}
