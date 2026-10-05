use super::*;
use crate::i18n::AppLocale;
use crate::select_detail_options::CATALOG;
use bmz_render::scene::detail_options::{DetailOptionsClosingSnapshot, detail_options_column};
use bmz_render::skin::SkinClickTarget;

fn skin_path() -> PathBuf {
    let path = test_app_paths().resource_dir.join("skins/Luxez-Flat/music_select.luaskin");
    assert!(path.exists(), "initialize the bundled Luxez-Flat submodule for these tests");
    path
}

fn snapshot(locale: AppLocale, cursor: usize) -> SelectSnapshot {
    SelectSnapshot { time: TimeUs(2_000_000), ..detail_options::snapshot(locale, cursor) }
}

fn renderer() -> Renderer {
    let decoded = detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap();
    assert!(decoded.document.uses_detail_options());
    assert!(decoded.document.bmz_detail_options_close);
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

fn close(s: &mut SelectSnapshot, next: u8, elapsed: i64) {
    s.detail_options_closing = Some(DetailOptionsClosingSnapshot {
        panel: s.detail_options.take().unwrap(),
        scroll: s.detail_options_scroll,
    });
    s.detail_options_scroll = 0.0;
    s.option_panel = next;
    s.option_panel_time = TimeUs(elapsed * 1000);
    s.option_panel_off_times[1] = Some(TimeUs(elapsed * 1000));
}

fn switching(from: u8, to: u8, elapsed: i64) -> SelectSnapshot {
    let mut s = snapshot(AppLocale::Ja, 0);
    if from == 2 {
        close(&mut s, to, elapsed);
    } else if to != 2 {
        s.detail_options = None;
    }
    s.option_panel = to;
    s.option_panel_time = TimeUs(elapsed * 1000);
    s.option_panel_off_times[usize::from(from - 1)] = Some(TimeUs(elapsed * 1000));
    s
}

#[test]
fn luxe_detail_options_all_items_choices_and_moving_hits() {
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
                assert!(!has_text(&panel.title) && !has_text(&panel.guide));
                assert!(!has_text("▼") && !has_text("●"));
                assert!(has_text(&panel.selected().unwrap().description));
                assert!(commands.iter().any(|cmd| matches!(cmd, DrawCommand::Rect { rect, color }
                    if rect.width == 1.0 && rect.height == 1.0 && color.a > 0.0 && color.a < 1.0)));
                for slot in 0..9 {
                    let x = 138.0 + (detail_options_column(slot) as f32 + scroll) * 274.0;
                    if !(0.0..1920.0).contains(&x) {
                        continue;
                    }
                    let row = panel.row(slot).unwrap();
                    assert!(has_text(&row.label), "missing {}", row.item_id);
                    assert_eq!(event_at(&renderer, &s, x, 220.0), Some(19310 + slot as i32));
                    for (index, choice) in row.choices.iter().enumerate() {
                        assert!(has_text(&choice.label));
                        assert_eq!(
                            event_at(&renderer, &s, x, 299.0 + index as f32 * 52.0),
                            Some(19500 + slot as i32 * 64 + index as i32 * 4)
                        );
                    }
                }
                for (x, y) in [(960.0, 60.0), (1474.0, 958.0)] {
                    assert_eq!(event_at(&renderer, &s, x, y), None);
                }
                // The old EXTRA MODE hover rectangle must never capture a click.
                assert!(
                    event_at(&renderer, &s, 1520.0, 558.0)
                        .is_none_or(bmz_render::skin::is_detail_options_event)
                );
                assert!(renderer.select_skin_slider_hit(&s, 0.5, 0.3).is_none());
            }
        }
    }
}

#[test]
fn luxe_detail_options_uses_existing_artwork_and_fonts_with_complete_labels() {
    let path = skin_path();
    let root = path.parent().unwrap();
    for font in ["font_sub/sub.fnt", "font_sub_small/sub_small.fnt"] {
        let font = bmz_render::bitmap_font::load_bitmap_font(
            &root.join("select_skinparts/default_commonparts/font_ver1.2.0").join(font),
        )
        .unwrap();
        for locale in [AppLocale::Ja, AppLocale::En] {
            let s = snapshot(locale, 0);
            for row in s.detail_options.unwrap().items.iter() {
                for label in [
                    &row.label,
                    &row.description,
                    &row.category,
                    &row.reason,
                    &row.auxiliary,
                    &row.status,
                ] {
                    for c in label.chars().filter(|c| !c.is_whitespace()) {
                        assert!(font.glyphs.contains_key(&c), "missing glyph {c} in {label}");
                    }
                }
            }
        }
    }
    let decoded = detail_options::decode_experimental(&path, SkinKind::Select).unwrap();
    assert!(decoded.fonts.iter().any(|font| {
        font.stored_id == "select:luxe_detail_choices"
            && font.path.ends_with("NotoSansCJKjp-Medium.otf")
            && matches!(&font.data, Some(DecodedFontData::Vector(bytes)) if !bytes.is_empty())
    }));
    for (id, source) in [
        ("luxe_detail_button_1", "luxe_detail_panel"),
        ("luxe_detail_value_selected", "luxe_detail_cursor"),
    ] {
        assert!(decoded.document.image.iter().any(|i| i.id == id && i.src == source));
    }
    // The glow padding belongs outside the button. Scaling the whole cursor to
    // the button's bounds shrinks its bright center and reproduces the mismatch.
    let image_frame = |id: &str| {
        decoded
            .document
            .destination
            .iter()
            .find_map(|entry| match entry {
                DestinationListEntry::Single(d) if d.id == id && d.timer == Some(22) => {
                    match d.dst.first() {
                        Some(bmz_render::skin::SkinDstEntry::Frame(frame)) => Some(frame),
                        _ => None,
                    }
                }
                _ => None,
            })
            .unwrap()
    };
    let hit = image_frame("luxe_detail_row_0_choice_0_hit");
    let button_x = hit.x.unwrap() as f32 + 6.0;
    let button_y = (1080 - hit.y.unwrap() - hit.h.unwrap()) as f32;
    let cursor = image_frame("luxe_detail_value_selected");
    let scale = 226.0 / 176.0;
    for (actual, expected) in [
        (cursor.w.unwrap() as f32, 213.0 * scale),
        (cursor.h.unwrap() as f32, 76.0 * scale),
        (cursor.x.unwrap() as f32, button_x - 19.0 * scale),
        ((1080 - cursor.y.unwrap() - cursor.h.unwrap()) as f32, button_y - 19.0 * scale),
    ] {
        // Skin frames are quantized to whole canvas pixels by the Lua decoder.
        assert!((actual - expected).abs() <= 1.0);
    }
    let bands: Vec<_> = decoded
        .document
        .image
        .iter()
        .filter(|image| image.id.starts_with("luxe_detail_button_"))
        .collect();
    let panel = decoded.sources.iter().find(|s| s.source_id == "luxe_detail_panel").unwrap();
    let asset = panel.asset.as_ref().unwrap();
    // Keep every neutral button pixel exactly once and exclude the purple matte,
    // including the right/bottom padding, rather than covering it with a color.
    for y in 277..316 {
        for x in 623..799 {
            let offset = ((y as u32 * asset.width + x as u32) * 4) as usize;
            let pixel = &asset.pixels[offset..offset + 4];
            let neutral = pixel[0] == pixel[1] && pixel[1] == pixel[2];
            let count = bands
                .iter()
                .filter(|b| x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h)
                .count();
            assert_eq!(count, usize::from(neutral), "button pixel ({x}, {y})");
        }
    }
    let mut previous_bottom = button_y;
    for band in bands {
        let frame = image_frame(&band.id);
        let top = (1080 - frame.y.unwrap() - frame.h.unwrap()) as f32;
        assert_eq!(top, previous_bottom, "button bands must not overlap or leave seams");
        previous_bottom = top + frame.h.unwrap() as f32;
        for (actual, expected) in [
            (frame.x.unwrap() as f32, button_x + (band.x - 623) as f32 * scale),
            (top, button_y + (band.y - 277) as f32 * scale),
            (frame.w.unwrap() as f32, band.w as f32 * scale),
            (frame.h.unwrap() as f32, band.h as f32 * scale),
        ] {
            assert!((actual - expected).abs() <= 1.0);
        }
    }
    assert!(
        decoded
            .document
            .text
            .iter()
            .filter(|t| t.id.starts_with("luxe_detail_")
                && (t.id.contains("_choice_")
                    || t.id.ends_with("_external")
                    || t.id == "luxe_detail_value"))
            .all(|t| t.font == "select:luxe_detail_choices"
                && t.outline_width == 0.0
                && t.shadow_color.is_empty())
    );
    assert!(
        !decoded
            .document
            .text
            .iter()
            .any(|t| t.id.starts_with("luxe_detail_") && (19306..=19309).contains(&t.ref_id))
    );
    let panel_texture = bmz_render::plan::TextureId(
        decoded.sources.iter().find(|s| s.source_id == "luxe_detail_panel").unwrap().texture.0,
    );
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer.prepare_scene(AppSceneSnapshot::Select(snapshot(AppLocale::Ja, 0)));
    // These points are inside the four rounded corners of the selected column.
    // Opaque fill from the original 24x24 corner tiles must not cover them.
    for x in [831.0 + 20.0, 831.0 + 258.0 - 20.0] {
        for y in [180.0 + 20.0, 730.0 - 20.0] {
            assert!(!renderer.last_plan().unwrap().commands.iter().any(|command| {
                matches!(command, DrawCommand::Image { texture, rect, .. }
                    if *texture == panel_texture
                        && x / 1920.0 >= rect.x && x / 1920.0 < rect.x + rect.width
                        && y / 1080.0 >= rect.y && y / 1080.0 < rect.y + rect.height)
            }));
        }
    }
}

#[test]
fn luxe_detail_options_handles_empty_locked_inactive_and_external_rows() {
    let mut renderer = renderer();
    for count in [0, 1, 3, 6] {
        let mut s = snapshot(AppLocale::Ja, 0);
        let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
        panel.items = panel.items[..count].to_vec().into();
        renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
        for slot in 0..7 {
            let valid = s.detail_options.as_ref().unwrap().row(slot).is_some();
            assert_eq!(
                event_at(&renderer, &s, 138.0 + slot as f32 * 274.0, 220.0),
                valid.then_some(19310 + slot as i32)
            );
        }
    }
    let mut s = snapshot(AppLocale::Ja, 8);
    let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
    let row = &mut std::sync::Arc::make_mut(&mut panel.items)[8];
    row.value = 2;
    row.value_index = -1;
    row.value_label = "ADD".into();
    row.editable = false;
    row.status = "[編集不可]".into();
    row.reason = "テスト用の編集不可理由".into();
    renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
    for label in ["ADD", "[編集不可]", "テスト用の編集不可理由"] {
        assert!(renderer.last_plan().unwrap().commands.iter().any(|cmd| matches!(cmd,
            DrawCommand::Text { text, .. } if text == label)));
    }
    assert_eq!(event_at(&renderer, &s, 960.0, 299.0), Some(19313));
    let s = snapshot(AppLocale::Ja, 3);
    assert!(!s.detail_options.as_ref().unwrap().selected().unwrap().effective);
    assert_eq!(event_at(&renderer, &s, 960.0, 299.0), Some(19692));
}

#[test]
fn luxe_detail_options_enter_exit_and_other_panels_animate_together() {
    let decoded = detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap();
    let panel_texture = |id: &str| {
        let src = &decoded.document.image.iter().find(|i| i.id == id).unwrap().src;
        bmz_render::plan::TextureId(
            decoded.sources.iter().find(|s| &s.source_id == src).unwrap().texture.0,
        )
    };
    let legacy = [
        panel_texture("default_optionpanel_option_panel1"),
        panel_texture("default_optionpanel_option_panel3"),
    ];
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    for (from, to) in [(2, 0), (1, 3), (3, 1), (2, 3), (3, 2), (2, 1), (1, 2)] {
        for elapsed in [0, 75, 150, 225, 299, 300] {
            let s = switching(from, to, elapsed);
            renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
            let commands = &renderer.last_plan().unwrap().commands;
            let ease = 1.0 - (1.0 - elapsed as f32 / 300.0).powi(2);
            for (panel, entering) in [(from, false), (to, true)] {
                if panel == 0 {
                    continue;
                }
                let offset = if entering {
                    (-1920.0 * (1.0 - ease)).round()
                } else {
                    (-1920.0 * ease).round()
                };
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
                    (x, 843.0 + offset)
                } else {
                    let texture_id = legacy[usize::from(panel == 3)];
                    let x = commands.iter().find_map(|cmd| match cmd {
                        DrawCommand::Image { texture, rect, .. } if *texture == texture_id => {
                            Some(rect.x * 1920.0)
                        }
                        _ => None,
                    });
                    (x, offset)
                };
                assert!(
                    (actual.expect("both panels must be drawn") - expected).abs() < 1.1,
                    "{from}->{to}, panel {panel}, {elapsed}ms: {actual:?} vs {expected}"
                );
            }
            if to == 2 {
                let offset = (-1920.0 * (1.0 - ease)).round();
                for slot in 0..7 {
                    let x = 138.0 + slot as f32 * 274.0 + offset;
                    if (0.0..1920.0).contains(&x) {
                        assert_eq!(event_at(&renderer, &s, x, 299.0), Some(19500 + slot * 64));
                    }
                }
            } else {
                assert!(
                    !event_at(&renderer, &s, 960.0, 299.0)
                        .is_some_and(bmz_render::skin::is_detail_options_event)
                );
                if to == 0 && elapsed < 300 {
                    assert!(renderer.select_skin_slider_hit(&s, 0.5, 0.3).is_none());
                    assert_eq!(event_at(&renderer, &s, 1520.0, 558.0), None);
                }
            }
            if to == 3 && elapsed == 300 {
                assert_eq!(event_at(&renderer, &s, 1487.0, 257.0), Some(330));
            }
        }
    }
}

#[test]
fn luxe_detail_options_replaces_only_e2_after_successful_part_load() {
    let source = skin_path().parent().unwrap().to_path_buf();
    for state in ["loaded", "experimental-off", "disabled", "missing", "failed", "non-bmz"] {
        let root = unique_test_dir("bmz-luxe-detail-fallback");
        let parts = root.join("select_skinparts");
        std::fs::create_dir_all(parts.join("default_optionpanel")).unwrap();
        std::fs::create_dir_all(parts.join("default_detailoptions")).unwrap();
        std::fs::copy(source.join("load.lua"), root.join("load.lua")).unwrap();
        std::fs::copy(
            source.join("select_skinparts/default_optionpanel/parts.lua"),
            parts.join("default_optionpanel/parts.lua"),
        )
        .unwrap();
        if state != "missing" {
            std::fs::copy(
                source.join("select_skinparts/default_detailoptions/parts.lua"),
                parts.join("default_detailoptions/parts.lua"),
            )
            .unwrap();
        }
        if state == "failed" {
            std::fs::write(
                parts.join("default_detailoptions/parts.lua"),
                "return {load = function() error('test load failure') end}",
            )
            .unwrap();
        }
        std::fs::write(
            parts.join("enable.txt"),
            if state == "disabled" {
                "default_optionpanel/parts.lua\n"
            } else {
                "default_optionpanel/parts.lua\ndefault_detailoptions/parts.lua\n"
            },
        )
        .unwrap();
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
            &BTreeMap::from([(
                "bmz_detail_options".into(),
                if state == "experimental-off" { "0" } else { "1" }.into(),
            )]),
            &BTreeMap::new(),
        )
        .unwrap();
        let active = state == "loaded";
        assert_eq!(loaded.document.uses_detail_options(), active, "{state}");
        assert_eq!(loaded.document.bmz_detail_options_close, active, "{state}");
        for timer in [22, 32] {
            assert_eq!(loaded.document.destination.iter().any(|entry| matches!(entry,
                DestinationListEntry::Single(d) if d.timer == Some(timer) && !d.id.starts_with("luxe_detail_"))), !active, "{state}: timer {timer}");
        }
        for timer in [21, 23, 31, 33] {
            assert!(
                loaded.document.destination.iter().any(|entry| matches!(entry,
                DestinationListEntry::Single(d) if d.timer == Some(timer))),
                "{state}: timer {timer}"
            );
        }
        // The same mouseRect id is reused by E2 and E1+E2; do not remove by id.
        let hover_count = loaded
            .document
            .destination
            .iter()
            .filter(|entry| {
                matches!(entry,
            DestinationListEntry::Single(d) if d.id == "gas_low_limit_rect")
            })
            .count();
        assert_eq!(hover_count, if active { 1 } else { 3 }, "{state}");
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
#[ignore = "requires a GPU adapter; writes Luxez Flat panel previews to a temporary directory"]
fn luxe_detail_options_gpu_previews() {
    let output = unique_test_dir("bmz-luxe-detail-preview");
    std::fs::create_dir_all(&output).unwrap();
    for (width, height) in [(1920, 1080), (1280, 720), (960, 540), (1024, 768), (2560, 1080)] {
        let mut renderer = renderer();
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        renderer.attach_offscreen(bmz_render::renderer::SurfaceSize { width, height }).unwrap();
        let mut scenes = Vec::new();
        for (locale, cursor, scroll) in [
            (AppLocale::Ja, 0, 0.0),
            (AppLocale::En, 7, 0.0),
            (AppLocale::Ja, 3, 0.0),
            (AppLocale::Ja, 8, 0.0),
            (AppLocale::Ja, 12, 0.0),
            (AppLocale::En, 12, 0.0),
            (AppLocale::En, 14, -0.5),
        ] {
            let mut s = snapshot(locale, cursor);
            s.detail_options_scroll = scroll;
            scenes.push((format!("{}-{cursor}-{scroll}", locale.code()), s));
        }
        if (1280..=1920).contains(&width) {
            for (from, to) in [(2, 0), (2, 1), (1, 2)] {
                for elapsed in [0, 75, 150, 225, 300] {
                    scenes
                        .push((format!("{from}-to-{to}-{elapsed}"), switching(from, to, elapsed)));
                }
            }
        }
        for (label, mut s) in scenes {
            s.player_name = "BMZ Player".into();
            s.selected_title = "Sample song".into();
            s.current_folder = "Sample folder".into();
            s.rows = vec![SelectRowSnapshot { title: "Sample song".into(), ..Default::default() }];
            s.chart_count = 1;
            renderer.render_scene(AppSceneSnapshot::Select(s)).unwrap();
            let image =
                image::RgbaImage::from_raw(width, height, renderer.read_offscreen_rgba().unwrap())
                    .unwrap();
            if width == 2560 {
                for x in [160, 2400] {
                    assert_eq!(image.get_pixel(x, 400).0, [0, 0, 0, 255]);
                }
            }
            image.save(output.join(format!("luxe-{label}-{width}x{height}.png"))).unwrap();
        }
    }
    println!("Luxez Flat DETAIL OPTIONS previews: {}", output.display());
}
