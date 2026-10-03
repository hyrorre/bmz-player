use super::*;
use crate::i18n::AppLocale;
use crate::select_detail_options::CATALOG;
use bmz_render::skin::SkinClickTarget;

fn skin_path() -> PathBuf {
    let path = test_app_paths().resource_dir.join("skins/mz-select/music_select.luaskin");
    assert!(path.exists(), "initialize the bundled mz-select submodule to run these tests");
    path
}

fn snapshot(locale: AppLocale, cursor: usize) -> SelectSnapshot {
    SelectSnapshot { time: TimeUs(2_000_000), ..detail_options::snapshot(locale, cursor) }
}

fn renderer() -> Renderer {
    let decoded = decode_beatoraja_skin(&skin_path(), SkinKind::Select).unwrap();
    assert_eq!(decoded.document.bmz_detail_options, 1);
    assert!(decoded.document.bmz_detail_options_close);
    assert!(!decoded.document.destination.iter().any(|entry| matches!(entry,
        DestinationListEntry::Single(d) if d.id == "default_optionpanel_option_panel2"
    )));
    for timer in [21, 23, 31, 33] {
        assert!(
            decoded.document.destination.iter().any(|entry| matches!(entry,
                DestinationListEntry::Single(d) if d.timer == Some(timer)
            )),
            "legacy panel timer {timer} was lost"
        );
    }
    assert!(
        decoded
            .document
            .text
            .iter()
            .filter(|t| t.id.starts_with("mz_detail_"))
            .all(|t| { !t.constant_text.contains(['▼', '●']) && t.value_expr.is_empty() })
    );
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer
}

fn event_at(renderer: &Renderer, s: &SelectSnapshot, x: f32, y: f32) -> Option<i32> {
    renderer.select_skin_click_hit(s, x / 1920.0, y / 1080.0).and_then(|hit| match hit.target {
        SkinClickTarget::Event { event_id, .. } => Some(event_id),
        _ => None,
    })
}

fn close_panel(s: &mut SelectSnapshot, elapsed_ms: i64) {
    s.detail_options_closing =
        Some(bmz_render::scene::detail_options::DetailOptionsClosingSnapshot {
            panel: s.detail_options.take().unwrap(),
            scroll: s.detail_options_scroll,
        });
    s.detail_options_scroll = 0.0;
    s.option_panel = 0;
    s.option_panel_time = TimeUs(0);
    s.option_panel_off_times[1] = Some(TimeUs(elapsed_ms * 1000));
}

fn switching_panel(from: u8, to: u8, elapsed: i64) -> SelectSnapshot {
    let mut s = snapshot(AppLocale::Ja, 0);
    if from == 2 {
        close_panel(&mut s, elapsed);
    } else if to != 2 {
        s.detail_options = None;
    }
    s.option_panel = to;
    s.option_panel_time = TimeUs(elapsed * 1000);
    s.option_panel_off_times[usize::from(from - 1)] = Some(TimeUs(elapsed * 1000));
    s
}

#[test]
fn mz_detail_options_switch_plays_both_exit_and_entrance_and_routes_current_panel_clicks() {
    let mut decoded = decode_beatoraja_skin(&skin_path(), SkinKind::Select).unwrap();
    let panel_texture = |id: &str| {
        let src = &decoded.document.image.iter().find(|image| image.id == id).unwrap().src;
        bmz_render::plan::TextureId(
            decoded.sources.iter().find(|s| &s.source_id == src).unwrap().texture.0,
        )
    };
    let panel1 = panel_texture("default_optionpanel_option_panel1");
    let panel3 = panel_texture("default_optionpanel_option_panel3");
    // A current-panel control must remain clickable while E2 is exiting.
    decoded.document.image.push(
        serde_json::from_value(serde_json::json!({
            "id":"test-current-panel-control","src":-1,"w":1,"h":1,"act":78,"clickable":true
        }))
        .unwrap(),
    );
    decoded.document.destination.push(
        serde_json::from_value(serde_json::json!({
            "id":"test-current-panel-control","op":[23],"dst":[{"x":1200,"y":180,"w":100,"h":60}]
        }))
        .unwrap(),
    );
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    for (from, to) in [(1, 3), (3, 1), (2, 3), (3, 2), (2, 1)] {
        for elapsed in [0, 75, 150, 225, 299, 300] {
            let s = switching_panel(from, to, elapsed);
            renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
            let commands = &renderer.last_plan().unwrap().commands;
            let rate = elapsed as f32 / 300.0;
            let ease = 1.0 - (1.0 - rate).powi(2);
            for (panel, entering) in [(from, false), (to, true)] {
                let (actual, expected) = if panel == 2 {
                    let x = commands.iter().find_map(|cmd| match cmd {
                        DrawCommand::Text { text, origin, .. } if text == "SUDDEN+" => {
                            Some(origin.x * 1920.0)
                        }
                        _ => None,
                    });
                    if !entering && elapsed == 300 {
                        assert!(x.is_none());
                        continue;
                    }
                    (
                        x,
                        841.0
                            + if entering {
                                (-1920.0 * (1.0 - ease)).round()
                            } else {
                                (-1920.0 * ease).round()
                            },
                    )
                } else {
                    let texture_id = if panel == 1 { panel1 } else { panel3 };
                    let x = commands.iter().find_map(|cmd| match cmd {
                        DrawCommand::Image { texture, rect, .. } if *texture == texture_id => {
                            Some(rect.x * 1920.0)
                        }
                        _ => None,
                    });
                    (
                        x,
                        if entering {
                            -1024.0 * (1.0 - ease)
                        } else {
                            -(if panel == 1 { 1315.0 } else { 1024.0 }) * ease
                        }
                        .round(),
                    )
                };
                assert!(
                    (actual.expect("both panels must be drawn") - expected).abs() < 1.1,
                    "{from}->{to}, panel={panel}, elapsed={elapsed}: {actual:?} vs {expected}"
                );
            }
            if to == 3 {
                assert_eq!(event_at(&renderer, &s, 1250.0, 870.0), Some(78));
            }
            if to != 2 {
                for x in [100.0, 960.0, 1800.0] {
                    assert!(
                        !event_at(&renderer, &s, x, 299.0)
                            .is_some_and(bmz_render::skin::is_detail_options_event)
                    );
                }
            }
        }
    }
}

#[test]
fn mz_detail_options_exit_keeps_labels_and_scroll_but_blocks_all_clicks() {
    let mut renderer = renderer();
    for locale in [AppLocale::Ja, AppLocale::En] {
        for scroll in [-0.5, 0.0, 0.5] {
            let mut s = snapshot(locale, 0);
            s.detail_options_scroll = scroll;
            close_panel(&mut s, 0);
            for elapsed in [0, 75, 150, 225, 299, 300, 500] {
                s.option_panel_off_times[1] = Some(TimeUs(elapsed * 1000));
                renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
                let commands = &renderer.last_plan().unwrap().commands;
                let x = commands.iter().find_map(|cmd| match cmd {
                    DrawCommand::Text { text, origin, .. } if text == "SUDDEN+" => {
                        Some(origin.x * 1920.0)
                    }
                    _ => None,
                });
                if elapsed < 300 {
                    let rate = elapsed as f32 / 300.0;
                    let offset = (-1920.0 * (1.0 - (1.0 - rate).powi(2))).round();
                    assert!((x.unwrap() - 841.0 - offset - scroll * 274.0).abs() < 1.0);
                } else {
                    assert!(x.is_none(), "expired close snapshot still drawn");
                }
                for x in [80.0, 450.0, 960.0, 1840.0] {
                    for y in [60.0, 299.0, 700.0, 960.0] {
                        if elapsed < 300 {
                            assert_eq!(event_at(&renderer, &s, x, y), None);
                            assert!(
                                renderer
                                    .select_skin_slider_hit(&s, x / 1920.0, y / 1080.0)
                                    .is_none()
                            );
                        }
                    }
                }
            }
            // Switching to another panel preserves the E2 exit alongside its entrance.
            s.option_panel_off_times[1] = Some(TimeUs(100_000));
            for panel in [1, 3] {
                s.option_panel = panel;
                renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
                assert!(renderer.last_plan().unwrap().commands.iter().any(|cmd| matches!(cmd,
                    DrawCommand::Text { text, .. } if text == "SUDDEN+")));
            }
        }
    }
}

#[test]
fn mz_detail_options_closing_data_requires_explicit_opt_in() {
    let mut s = snapshot(AppLocale::Ja, 0);
    close_panel(&mut s, 100);
    let mut decoded = decode_beatoraja_skin(&skin_path(), SkinKind::Select).unwrap();
    decoded.document.bmz_detail_options_close = false;
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer.prepare_scene(AppSceneSnapshot::Select(s));
    assert!(!renderer.last_plan().unwrap().commands.iter().any(|cmd| matches!(cmd,
        DrawCommand::Text { text, .. } if text == "SUDDEN+")));
}

#[test]
fn mz_detail_options_reuses_option_artwork_and_version_font() {
    let path = skin_path();
    let root = path.parent().unwrap();
    let original = bmz_render::bitmap_font::load_bitmap_font(
        &root.join("customize/advanced/default_commonparts/font/m_select_system.fnt"),
    )
    .unwrap();
    let choices = bmz_render::bitmap_font::load_bitmap_font(
        &root.join("customize/advanced/default_detailoptions/choices.fnt"),
    )
    .unwrap();
    for (character, glyph) in original.glyphs.iter() {
        assert_eq!(choices.glyphs.get(character), Some(glyph));
    }
    assert_eq!(
        choices.pages[&0].path.canonicalize().unwrap(),
        original.pages[&0].path.canonicalize().unwrap()
    );
    // Keep the exact snapshot strings, including LN MODE parentheses that the
    // version atlas lacks. Only these two glyphs use the existing profile atlas.
    for locale in [AppLocale::Ja, AppLocale::En] {
        let s = snapshot(locale, 0);
        for row in s.detail_options.unwrap().items.iter() {
            for choice in row.choices.iter() {
                for c in choice.label.chars() {
                    assert!(choices.glyphs.contains_key(&c), "missing {c} in {}", choice.label);
                }
            }
        }
    }
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
    for (id, source) in
        [("mz_detail_button", "mz_detail_panel"), ("mz_detail_value_selected", "mz_detail_cursor")]
    {
        assert!(decoded.document.image.iter().any(|i| i.id == id && i.src == source));
    }
    assert!(
        decoded
            .document
            .text
            .iter()
            .filter(|t| t.id.starts_with("mz_detail_") && t.id.contains("_choice_"))
            .all(|t| t.font == "select:mz_detail_choices"),
        "choice font: {:?}",
        decoded.document.text.iter().find(|t| t.id == "mz_detail_row_3_choice_0")
    );
    assert!(
        !decoded
            .document
            .text
            .iter()
            .any(|t| t.id.starts_with("mz_detail_") && (19306..=19309).contains(&t.ref_id))
    );
}

#[test]
fn mz_detail_options_entrance_uses_legacy_easing_and_moving_hit_regions() {
    let mut renderer = renderer();
    let mut s = snapshot(AppLocale::Ja, 0);
    for elapsed in [0, 75, 150, 225, 300, 500] {
        s.option_panel_time = TimeUs(elapsed * 1000);
        renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
        let rate = (elapsed as f32 / 300.0).min(1.0);
        let offset = (-1920.0 * (1.0 - rate).powi(2)).round();
        let commands = &renderer.last_plan().unwrap().commands;
        let x = commands
            .iter()
            .find_map(|cmd| match cmd {
                DrawCommand::Text { text, origin, .. } if text == "SUDDEN+" => {
                    Some(origin.x * 1920.0)
                }
                _ => None,
            })
            .unwrap();
        // Center alignment stores the left edge of the 238 px text box.
        assert!((x - 841.0 - offset).abs() < 1.0, "elapsed={elapsed}, x={x}, offset={offset}");
        for slot in 0..7 {
            let x = 138.0 + slot as f32 * 274.0 + offset;
            if (0.0..1920.0).contains(&x) {
                assert_eq!(event_at(&renderer, &s, x, 299.0), Some(19500 + slot * 64));
            }
        }
        // No right overscan column flashes into view while the panel enters.
        if elapsed < 300 {
            assert_eq!(event_at(&renderer, &s, 1900.0, 299.0), None);
        }
        assert_eq!(event_at(&renderer, &s, 960.0, 60.0), None);
        assert_eq!(event_at(&renderer, &s, 960.0, 960.0), None);
        if elapsed >= 300 {
            assert!(commands.iter().any(|cmd| matches!(cmd, DrawCommand::Rect { rect, color }
                if rect.width == 1.0 && rect.height == 1.0 && color.a > 0.0 && color.a < 1.0)));
        }
    }
}

#[test]
fn mz_detail_options_all_items_choices_and_animated_clicks() {
    let mut renderer = renderer();
    for locale in [AppLocale::Ja, AppLocale::En] {
        for cursor in 0..CATALOG.len() {
            let mut s = snapshot(locale, cursor);
            for scroll in [-1.0, -0.5, 0.0, 0.5, 1.0] {
                s.detail_options_scroll = scroll;
                renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
                let panel = s.detail_options.as_ref().unwrap();
                let commands = &renderer.last_plan().unwrap().commands;
                let has_text = |label: &str| {
                    commands.iter().any(|cmd| {
                        matches!(cmd,
                    DrawCommand::Text { text, .. } if text == label)
                    })
                };
                assert!(!has_text(&panel.title));
                assert!(!has_text(&panel.guide));
                assert!(has_text(&panel.selected().unwrap().description));
                assert!(!has_text("▼") && !has_text("●"));
                for slot in 0..9 {
                    let row = panel.row(slot).unwrap();
                    let column = bmz_render::scene::detail_options::detail_options_column(slot);
                    let x = 138.0 + (column as f32 + scroll) * 274.0;
                    if !(0.0..1920.0).contains(&x) {
                        continue;
                    }
                    assert!(has_text(&row.label), "missing item {}", row.item_id);
                    assert_eq!(event_at(&renderer, &s, x, 220.0), Some(19310 + slot as i32));
                    for (index, choice) in row.choices.iter().enumerate() {
                        assert!(has_text(&choice.label));
                        assert_eq!(
                            event_at(&renderer, &s, x, 299.0 + index as f32 * 52.0),
                            Some(19500 + slot as i32 * 64 + index as i32 * 4)
                        );
                    }
                }
                // Transparent space still consumes clicks; no navigation buttons remain.
                assert_eq!(event_at(&renderer, &s, 1474.0, 958.0), None);
                assert!(renderer.select_skin_slider_hit(&s, 0.5, 0.3).is_none());
                assert_eq!(event_at(&renderer, &s, 960.0, 30.0), None);
            }
        }
    }
    for option_panel in [0, 1, 3] {
        let s = SelectSnapshot {
            option_panel,
            time: TimeUs(2_000_000),
            option_panel_time: TimeUs(500_000),
            option_panel_off_times: [None, Some(TimeUs(100_000)), None, None, None, None],
            ..Default::default()
        };
        renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
        assert!(!renderer.last_plan().unwrap().commands.iter().any(|cmd| matches!(cmd,
            DrawCommand::Text { text, .. } if text.contains("DETAIL OPTIONS"))));
        assert!(
            !event_at(&renderer, &s, 1474.0, 958.0)
                .is_some_and(bmz_render::skin::is_detail_options_event)
        );
    }
}

#[test]
fn mz_detail_options_small_catalogue_inactive_locked_and_external_values() {
    let mut renderer = renderer();
    for count in [0, 1, 3, 6] {
        let mut s = snapshot(AppLocale::Ja, 0);
        let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
        panel.items = panel.items[..count].to_vec().into();
        renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
        for slot in 0..9 {
            let column = bmz_render::scene::detail_options::detail_options_column(slot);
            let x = 138.0 + column as f32 * 274.0;
            let valid = s.detail_options.as_ref().unwrap().row(slot).is_some();
            assert_eq!(event_at(&renderer, &s, x, 220.0), valid.then_some(19310 + slot as i32));
        }
    }
    let mut s = snapshot(AppLocale::Ja, 8);
    let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
    let row = &mut std::sync::Arc::make_mut(&mut panel.items)[8];
    row.value = 2;
    row.value_index = -1;
    row.value_label = "ADD".into();
    row.editable = false;
    row.effective = false;
    row.status = "[編集不可]".into();
    row.reason = "テスト用の編集不可理由".into();
    renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
    for label in ["ADD", "[編集不可]", "テスト用の編集不可理由"] {
        assert!(renderer.last_plan().unwrap().commands.iter().any(|cmd| matches!(cmd,
            DrawCommand::Text { text, .. } if text == label)));
    }
    // A disabled value cannot fire its choice event, but its column stays selectable.
    assert_eq!(event_at(&renderer, &s, 960.0, 299.0), Some(19313));
    let s = snapshot(AppLocale::Ja, 3);
    assert!(!s.detail_options.as_ref().unwrap().selected().unwrap().effective);
    assert_eq!(event_at(&renderer, &s, 960.0, 299.0), Some(19692));
}

#[test]
fn mz_detail_options_capability_requires_a_successfully_loaded_part() {
    let source = skin_path().parent().unwrap().to_path_buf();
    for version in [3, 4] {
        for state in ["loaded", "disabled", "missing", "failed", "non-bmz"] {
            let root = unique_test_dir("bmz-mz-detail-fallback");
            let advanced = root.join("customize/advanced");
            let legacy = format!("default_optionpanel{version}");
            std::fs::create_dir_all(advanced.join(&legacy)).unwrap();
            std::fs::create_dir_all(advanced.join("default_detailoptions")).unwrap();
            std::fs::create_dir_all(root.join("system")).unwrap();
            std::fs::copy(source.join("load.lua"), root.join("load.lua")).unwrap();
            std::fs::copy(
                source.join(format!("customize/advanced/{legacy}/parts.lua")),
                advanced.join(&legacy).join("parts.lua"),
            )
            .unwrap();
            std::fs::write(root.join("system/sound.lua"), "return {}").unwrap();
            if state != "missing" {
                std::fs::copy(
                    source.join("customize/advanced/default_detailoptions/parts.lua"),
                    advanced.join("default_detailoptions/parts.lua"),
                )
                .unwrap();
            }
            if state == "failed" {
                std::fs::write(
                    advanced.join("default_detailoptions/parts.lua"),
                    "return {load = function() error('test load failure') end}",
                )
                .unwrap();
            }
            let enable = if state == "disabled" {
                format!("{legacy}/parts.lua\n")
            } else {
                format!("{legacy}/parts.lua\ndefault_detailoptions/parts.lua\n")
            };
            std::fs::write(advanced.join("enable.txt"), enable).unwrap();
            std::fs::write(
                root.join("music_select.luaskin"),
                format!(
                    "{} return require('load').load_parts()",
                    if state == "non-bmz" { "bmz = nil;" } else { "" }
                ),
            )
            .unwrap();
            let loaded = bmz_skin::load_lua_skin(
                &root.join("music_select.luaskin"),
                bmz_skin::SkinKind::Select,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
            .unwrap();
            let active = state == "loaded";
            assert_eq!(loaded.document.uses_detail_options(), active, "{version}: {state}");
            assert_eq!(loaded.document.bmz_detail_options, u32::from(active), "{version}: {state}");
            assert_eq!(loaded.document.bmz_detail_options_close, active, "{version}: {state}");
            for timer in [22, 32] {
                assert_eq!(
                    loaded.document.destination.iter().any(|entry| matches!(entry,
                        DestinationListEntry::Single(d) if d.timer == Some(timer) && !d.id.starts_with("mz_detail_")
                    )),
                    !active,
                    "{version}: {state}, timer {timer}"
                );
            }
            for timer in [21, 23, 31, 33] {
                assert!(
                    loaded.document.destination.iter().any(|entry| matches!(entry,
                        DestinationListEntry::Single(d) if d.timer == Some(timer)
                    )),
                    "{version}: {state}, timer {timer}"
                );
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter; writes mz-select preview PNGs to a temporary directory"]
fn mz_detail_options_gpu_previews() {
    let output = unique_test_dir("bmz-mz-detail-options-preview");
    std::fs::create_dir_all(&output).unwrap();
    for (width, height) in [(1920, 1080), (1280, 720), (960, 540), (1024, 768), (2560, 1080)] {
        let mut renderer = renderer();
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        renderer.attach_offscreen(bmz_render::renderer::SurfaceSize { width, height }).unwrap();
        for (locale, cursor, scroll, elapsed) in [
            (AppLocale::Ja, 0, 0.0, 500),
            (AppLocale::En, 7, 0.0, 500),
            (AppLocale::Ja, 3, 0.0, 500),
            (AppLocale::En, 14, -0.5, 500),
            (AppLocale::Ja, 0, 0.5, 500),
            (AppLocale::Ja, 0, 0.0, 0),
            (AppLocale::Ja, 0, 0.0, 75),
            (AppLocale::Ja, 0, 0.0, 150),
            (AppLocale::Ja, 0, 0.0, 225),
            (AppLocale::Ja, 0, 0.0, -1),
            (AppLocale::Ja, 0, 0.0, -76),
            (AppLocale::En, 7, 0.0, -151),
            (AppLocale::Ja, 0, 0.5, -226),
            (AppLocale::Ja, 0, 0.0, -301),
        ] {
            let mut s = snapshot(locale, cursor);
            s.detail_options_scroll = scroll;
            s.option_panel_time = TimeUs(elapsed * 1000);
            if elapsed < 0 {
                close_panel(&mut s, -elapsed - 1);
            }
            s.player_name = "BMZ Player".into();
            s.selected_title = "Sample song".into();
            s.current_folder = "Sample folder".into();
            s.rows = (0..12)
                .map(|index| SelectRowSnapshot {
                    index,
                    title: format!("Sample song {:02}", index + 1),
                    play_level: "10".into(),
                    difficulty_name: "ANOTHER".into(),
                    ..Default::default()
                })
                .collect();
            s.chart_count = s.rows.len() as u32;
            renderer.render_scene(AppSceneSnapshot::Select(s)).unwrap();
            let image =
                image::RgbaImage::from_raw(width, height, renderer.read_offscreen_rgba().unwrap())
                    .unwrap();
            if width == 2560 {
                for x in [160, 2400] {
                    assert_eq!(
                        image.get_pixel(x, 400).0,
                        [0, 0, 0, 255],
                        "overscan leaked into the pillarbox"
                    );
                }
            }
            image
                .save(output.join(format!(
                    "mz-{}-{cursor}-{scroll}-{elapsed}-{width}x{height}.png",
                    locale.code()
                )))
                .unwrap();
        }
    }
    println!("mz-select DETAIL OPTIONS previews: {}", output.display());
}

#[test]
#[ignore = "requires a GPU adapter; writes panel transition PNGs to a temporary directory"]
fn mz_detail_options_switch_gpu_previews() {
    let output = unique_test_dir("bmz-mz-detail-options-switch");
    std::fs::create_dir_all(&output).unwrap();
    for (width, height) in [(1920, 1080), (1280, 720)] {
        let mut renderer = renderer();
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        renderer.attach_offscreen(bmz_render::renderer::SurfaceSize { width, height }).unwrap();
        for (from, to) in [(2, 3), (3, 2)] {
            for elapsed in [0, 75, 150, 225, 300] {
                let mut s = switching_panel(from, to, elapsed);
                s.player_name = "BMZ Player".into();
                s.selected_title = "Sample song".into();
                s.rows =
                    vec![SelectRowSnapshot { title: "Sample song".into(), ..Default::default() }];
                s.chart_count = 1;
                renderer.render_scene(AppSceneSnapshot::Select(s)).unwrap();
                image::RgbaImage::from_raw(width, height, renderer.read_offscreen_rgba().unwrap())
                    .unwrap()
                    .save(
                        output.join(format!(
                            "mz-switch-{from}-to-{to}-{elapsed}-{width}x{height}.png"
                        )),
                    )
                    .unwrap();
            }
        }
    }
    println!("mz-select panel switch previews: {}", output.display());
}
