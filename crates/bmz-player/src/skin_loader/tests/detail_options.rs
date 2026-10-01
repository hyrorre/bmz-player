use super::*;
use crate::config::profile_config::{GaugeAutoShiftConfig, ProfileConfig};
use crate::i18n::{AppLocale, Localizer};
use crate::select_detail_options::{CATALOG, DetailContext};
use bmz_render::scene::detail_options::{DetailOptionsSnapshot, detail_options_viewport};

fn snapshot(locale: AppLocale, cursor: usize) -> SelectSnapshot {
    let p = ProfileConfig::new_default("test", "Test", 0);
    let text = Localizer::new(locale);
    SelectSnapshot {
        option_panel: 2,
        option_panel_time: TimeUs(500_000),
        detail_options: Some(std::sync::Arc::new(DetailOptionsSnapshot {
            cursor,
            viewport_start: detail_options_viewport(cursor, CATALOG.len()),
            items: CATALOG
                .iter()
                .map(|item| {
                    item.row(
                        &p,
                        DetailContext {
                            mode: Some(bmz_core::lane::KeyMode::K7),
                            gas: GaugeAutoShiftConfig::Off,
                            practice: false,
                            course: false,
                        },
                        &text,
                    )
                })
                .collect(),
            title: text.text("detail-options-title"),
            scope_label: if CATALOG[cursor].mode_scoped {
                "7K".into()
            } else {
                text.text("detail-options-scope-global")
            },
            guide: text.text("detail-options-guide"),
            position: format!("{} / {}", cursor + 1, CATALOG.len()),
        })),
        ..Default::default()
    }
}

#[test]
fn detail_default_skin_decodes_all_rows_and_routes_only_panel_clicks() {
    let path = default_skin_document_path_from_paths(&test_app_paths(), SkinKind::Select);
    let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
    assert_eq!(decoded.document.bmz_detail_options, 1);
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    for locale in [AppLocale::Ja, AppLocale::En] {
        for cursor in 0..CATALOG.len() {
            let s = snapshot(locale, cursor);
            renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
            let plan = renderer.last_plan().unwrap();
            let panel = s.detail_options.as_ref().unwrap();
            for slot in 0..7 {
                let row = panel.row(slot).unwrap();
                assert!(
                    plan.commands.iter().any(
                        |cmd| matches!(cmd,DrawCommand::Text { text, .. } if text == &row.label)
                    ),
                    "missing row {}",
                    row.item_id
                );
                for (choice, value) in row.choices.iter().enumerate() {
                    assert!(plan.commands.iter().any(|cmd| matches!(cmd, DrawCommand::Text { text, .. } if text == &value.label)), "missing choice {} for {}", choice, row.item_id);
                    let hit = renderer
                        .select_skin_click_hit(
                            &s,
                            0.10 + slot as f32 * 0.129,
                            0.29 + choice as f32 * 0.041,
                        )
                        .unwrap();
                    assert!(
                        matches!(hit.target, bmz_render::skin::SkinClickTarget::Event { event_id, .. } if event_id == 19500 + slot as i32 * 64 + choice as i32 * 4)
                    );
                }
            }
            assert!(
                plan.commands.iter().any(
                    |cmd| matches!(cmd,DrawCommand::Text { text, .. } if text == &panel.title)
                )
            );
            assert!(
                plan.commands.iter().any(
                    |cmd| matches!(cmd,DrawCommand::Text { text, .. } if text == &panel.guide)
                )
            );
            let hit = renderer.select_skin_click_hit(&s, 0.10, 0.20).unwrap();
            assert!(matches!(
                hit.target,
                bmz_render::skin::SkinClickTarget::Event { event_id: 19310, .. }
            ));
            let hit = renderer.select_skin_click_hit(&s, 0.85, 0.87).unwrap();
            assert!(matches!(
                hit.target,
                bmz_render::skin::SkinClickTarget::Event { event_id: 19303, .. }
            ));
            assert!(renderer.select_skin_click_hit(&s, 0.98, 0.98).is_none());
            assert!(renderer.select_skin_slider_hit(&s, 0.5, 0.3).is_none());
        }
    }
    let closed = SelectSnapshot::default();
    renderer.prepare_scene(AppSceneSnapshot::Select(closed.clone()));
    assert!(
        !renderer.last_plan().unwrap().commands.iter().any(
            |cmd| matches!(cmd,DrawCommand::Text { text,.. } if text.contains("DETAIL OPTIONS"))
        )
    );
    assert!(!renderer.select_skin_click_hit(&closed, 0.85, 0.87).is_some_and(|hit| matches!(
        hit.target,
        bmz_render::skin::SkinClickTarget::Event { event_id: 19303, .. }
    )));
}

#[test]
fn detail_legacy_and_unskinned_paths_use_opaque_native_overlay() {
    for declaration in [None, Some(0), Some(2)] {
        let mut document =
            serde_json::json!({"type":5,"text":[{"id":"partial","ref":19300}],"destination":[]});
        if let Some(version) = declaration {
            document["bmzDetailOptions"] = version.into();
        }
        let document = serde_json::from_value(document).unwrap();
        let mut renderer = Renderer::default();
        set_decoded_skin_context(
            &mut renderer,
            SkinKind::Select,
            bmz_render::skin::default_skin_manifest(),
            document,
            None,
            vec![],
            false,
        );
        assert_native(&mut renderer);
    }
    assert_native(&mut Renderer::default());
}

fn assert_native(renderer: &mut Renderer) {
    let s = snapshot(AppLocale::Ja, 14);
    renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
    let commands = &renderer.last_plan().unwrap().commands;
    assert!(commands.iter().any(|cmd| matches!(cmd,DrawCommand::Rect {rect,color} if rect.x==0.0 && rect.y==0.0 && rect.width==1.0 && rect.height==1.0 && color.a==1.0)));
    assert!(
        commands.iter().any(
            |cmd| matches!(cmd,DrawCommand::Text {text,..} if text.contains("DETAIL OPTIONS"))
        )
    );
    assert!(renderer.select_skin_click_hit(&s, 0.5, 0.7).is_none());
    assert!(matches!(
        renderer.select_skin_click_hit(&s, 0.85, 0.87).unwrap().target,
        bmz_render::skin::SkinClickTarget::Event { event_id: 19303, .. }
    ));
    for slot in 0..7 {
        let row = s.detail_options.as_ref().unwrap().row(slot).unwrap();
        for (choice, value) in row.choices.iter().enumerate() {
            assert!(
                commands.iter().any(
                    |cmd| matches!(cmd, DrawCommand::Text { text, .. } if text == &value.label)
                )
            );
            let hit = renderer
                .select_skin_click_hit(&s, 0.10 + slot as f32 * 0.129, 0.29 + choice as f32 * 0.041)
                .unwrap();
            assert!(
                matches!(hit.target, bmz_render::skin::SkinClickTarget::Event { event_id, .. } if event_id == 19500 + slot as i32 * 64 + choice as i32 * 4)
            );
        }
    }
}

/// Explicit opt-in visual QA; no window, profile, DB, audio, or input device.
#[test]
#[ignore = "requires an available GPU adapter; writes preview PNGs to a temporary directory"]
fn detail_options_gpu_previews() {
    let output = unique_test_dir("bmz-detail-options-preview");
    std::fs::create_dir_all(&output).unwrap();
    let path = default_skin_document_path_from_paths(&test_app_paths(), SkinKind::Select);
    for native in [false, true] {
        for (width, height) in [(1280, 720), (960, 540), (1024, 768), (1920, 1080)] {
            let mut renderer = Renderer::default();
            if !native {
                let decoded = decode_beatoraja_skin(&path, SkinKind::Select).unwrap();
                install_decoded_skin(
                    &mut renderer,
                    decoded,
                    bmz_render::skin::default_skin_manifest(),
                )
                .unwrap();
            }
            renderer
                .set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
            renderer.attach_offscreen(bmz_render::renderer::SurfaceSize { width, height }).unwrap();
            for (locale, cursor) in [(AppLocale::Ja, 3), (AppLocale::En, 8)] {
                renderer.render_scene(AppSceneSnapshot::Select(snapshot(locale, cursor))).unwrap();
                let rgba = renderer.read_offscreen_rgba().unwrap();
                let image = image::RgbaImage::from_raw(width, height, rgba).unwrap();
                image
                    .save(output.join(format!(
                        "{}-{}-{width}x{height}.png",
                        if native { "native" } else { "default" },
                        locale.code()
                    )))
                    .unwrap();
            }
        }
    }
    println!("DETAIL OPTIONS previews: {}", output.display());
}
