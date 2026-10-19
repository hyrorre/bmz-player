use super::detail_options::cursor_of;
use super::*;
use crate::i18n::AppLocale;
use crate::select_detail_options::CATALOG;
use bmz_render::scene::detail_options::{DetailOptionsClosingSnapshot, detail_options_column};
use bmz_render::skin::SkinClickTarget;

// First choice of the normal-card layout: 1:1 38 px buttons every 41 px from y = 244.
const CHOICE_Y: f32 = 263.0;
const CHOICE_PITCH: f32 = 41.0;
// The column at the centre (drawing column 3) is raised by this much, scaled
// down linearly to 0 one column away while the carousel scrolls.
const SELECTED_RAISE: f32 = 24.0;

fn raise_at(column: f32) -> f32 {
    (SELECTED_RAISE * (1.0 - (column - 3.0).abs()).max(0.0)).round()
}

fn choice_y(centre: bool) -> f32 {
    choice_y_at(if centre { 3.0 } else { 0.0 })
}

fn choice_y_at(column: f32) -> f32 {
    CHOICE_Y - raise_at(column)
}

/// A point in a column's header, clear of its first choice.
fn header_y(centre: bool) -> f32 {
    header_y_at(if centre { 3.0 } else { 0.0 })
}

fn header_y_at(column: f32) -> f32 {
    220.0 - raise_at(column)
}
const RAISE: u32 = SELECTED_RAISE as u32;

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
    let mut s = snapshot(AppLocale::Ja, cursor_of(104));
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
                    let column = detail_options_column(slot) as f32 + scroll;
                    let x = 138.0 + column * 274.0;
                    if !(0.0..1920.0).contains(&x) {
                        continue;
                    }
                    let row = panel.row(slot).unwrap();
                    assert!(has_text(&row.label), "missing {}", row.item_id);
                    assert_eq!(
                        event_at(&renderer, &s, x, header_y_at(column)),
                        Some(19310 + slot as i32)
                    );
                    for (index, choice) in row.choices.iter().enumerate() {
                        assert!(has_text(&choice.label));
                        assert_eq!(
                            event_at(
                                &renderer,
                                &s,
                                x,
                                choice_y_at(column) + index as f32 * CHOICE_PITCH
                            ),
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
            let s = snapshot(locale, cursor_of(104));
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
        ("luxe_detail_button_m", "luxe_detail_panel"),
        ("luxe_detail_value_selected_l", "luxe_detail_cursor"),
        ("luxe_detail_value_selected_m", "luxe_detail_cursor"),
        ("luxe_detail_value_selected_r", "luxe_detail_cursor"),
    ] {
        assert!(decoded.document.image.iter().any(|i| i.id == id && i.src == source), "{id}");
    }
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
    // Buttons and the cursor keep the normal panel's 1:1 height; only the plain
    // middle stretches, and the cursor glows 19 px beyond the button.
    for (id, w, h) in [
        ("luxe_detail_button_l_0", 15, 38),
        ("luxe_detail_button_m", 196, 38),
        ("luxe_detail_button_r_0", 15, 38),
        ("luxe_detail_value_selected_l", 37, 76),
        ("luxe_detail_value_selected_m", 190, 76),
        ("luxe_detail_value_selected_r", 37, 76),
    ] {
        let frame = image_frame(id);
        assert_eq!((frame.w.unwrap(), frame.h.unwrap()), (w, h), "{id}");
    }
    let assets: std::collections::HashMap<_, _> = decoded
        .sources
        .iter()
        .filter_map(|s| s.asset.as_ref().map(|asset| (s.source_id.as_str(), asset)))
        .collect();
    let pixel = |source: &str, x: i32, y: i32| {
        let asset = assets[source];
        let offset = ((y as u32 * asset.width + x as u32) * 4) as usize;
        asset.pixels[offset..offset + 4].to_vec()
    };
    // The plain middle never samples the rounded caps.
    let middle = decoded.document.image.iter().find(|i| i.id == "luxe_detail_button_m").unwrap();
    assert!(middle.x >= 623 + 15 && middle.x + middle.w <= 623 + 160);
    // Each cap comes from a button on the same-colour card: outside its rounding it
    // shows the card's own fill, identical to the row above the button, which the
    // column fill reproduces at the same offsets.
    let caps: Vec<_> = decoded
        .document
        .image
        .iter()
        .filter(|i| {
            i.id.starts_with("luxe_detail_button_l_") || i.id.starts_with("luxe_detail_button_r_")
        })
        .collect();
    assert_eq!(caps.len(), 10);
    for cap in caps {
        let outer = if cap.id.contains("_l_") { cap.x } else { cap.x + cap.w - 1 };
        for y in [cap.y, cap.y + cap.h - 1] {
            assert_eq!(pixel(&cap.src, outer, y), pixel(&cap.src, outer, cap.y - 1), "{}", cap.id);
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
    let card_sources: Vec<_> =
        decoded.sources.iter().map(|s| (s.source_id.clone(), s.texture.0)).collect();
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer.prepare_scene(AppSceneSnapshot::Select(snapshot(AppLocale::Ja, cursor_of(104))));
    // Both rounded caps of the centre column's first button are really drawn.
    for x in [831.0 + 16.0, 831.0 + 16.0 + 226.0 - 15.0] {
        assert!(
            renderer.last_plan().unwrap().commands.iter().any(|command| matches!(
                command,
                DrawCommand::Image { rect, .. }
                    if (rect.x * 1920.0 - x).abs() < 0.5
                        && (rect.y * 1080.0 - (244.0 - SELECTED_RAISE)).abs() < 0.5
                        && (rect.width * 1920.0 - 15.0).abs() < 0.5
                        && (rect.height * 1080.0 - 38.0).abs() < 0.5
            )),
            "missing button cap at {x}"
        );
    }
    // The opaque fill follows the rounded rim: nothing from the card textures is
    // drawn just outside the four corners of the selected column.
    let textures: Vec<_> = ["luxe_detail_panel", "luxe_detail_panel1", "luxe_detail_panel3"]
        .iter()
        .map(|id| {
            bmz_render::plan::TextureId(
                card_sources.iter().find(|(source, _)| source == id).unwrap().1,
            )
        })
        .collect();
    for x in [831.0 + 3.0, 831.0 + 258.0 - 3.0] {
        for y in [180.0 + 3.0 - SELECTED_RAISE, 730.0 - 3.0 - SELECTED_RAISE] {
            assert!(!renderer.last_plan().unwrap().commands.iter().any(|command| {
                matches!(command, DrawCommand::Image { texture, rect, .. }
                    if textures.contains(texture)
                        && x / 1920.0 >= rect.x && x / 1920.0 < rect.x + rect.width
                        && y / 1080.0 >= rect.y && y / 1080.0 < rect.y + rect.height)
            }));
        }
    }
}

#[test]
fn luxe_detail_options_number_values_use_an_unselected_button() {
    let decoded = detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap();
    detail_options::assert_number_values_unselected(
        decoded,
        "luxe_detail_cursor",
        (0.0, 500.0, 213.0, 576.0),
    );
}

#[test]
fn luxe_detail_options_draws_all_buttons_before_the_choice_glow_and_labels() {
    let decoded = detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap();
    let texture = |id| {
        bmz_render::plan::TextureId(
            decoded.sources.iter().find(|s| s.source_id == id).unwrap().texture.0,
        )
    };
    let cursor_texture = texture("luxe_detail_cursor");
    let card_textures = [
        texture("luxe_detail_panel"),
        texture("luxe_detail_panel1"),
        texture("luxe_detail_panel3"),
    ];
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    for choice in 0..3 {
        let mut s = snapshot(AppLocale::Ja, cursor_of(703));
        let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
        let row = &mut std::sync::Arc::make_mut(&mut panel.items)[cursor_of(703)];
        row.value_index = choice;
        row.value = row.choices[choice as usize].value;
        let labels: Vec<_> = row.choices.iter().map(|c| c.label.clone()).collect();
        renderer.prepare_scene(AppSceneSnapshot::Select(s));
        let commands = &renderer.last_plan().unwrap().commands;
        let mut buttons = Vec::new();
        let mut cursors = Vec::new();
        let mut texts = Vec::new();
        for (index, command) in commands.iter().enumerate() {
            match command {
                DrawCommand::Image { texture, rect, .. }
                    if rect.x >= 828.0 / 1920.0 && rect.x < 1092.0 / 1920.0 =>
                {
                    if *texture == cursor_texture {
                        cursors.push(index);
                    }
                    if card_textures.contains(texture) && (rect.height * 1080.0 - 38.0).abs() < 0.1
                    {
                        buttons.push(index);
                    }
                }
                DrawCommand::Text { origin, text, .. }
                    if origin.x > 830.0 / 1920.0
                        && origin.x < 1090.0 / 1920.0
                        && labels.contains(text) =>
                {
                    texts.push(index)
                }
                _ => {}
            }
        }
        assert_eq!(buttons.len(), 9);
        assert_eq!(cursors.len(), 3);
        assert_eq!(texts.len(), 3);
        assert!(
            buttons.iter().max().unwrap() < cursors.iter().min().unwrap(),
            "choice {choice}: a neighboring button cuts off the cursor glow"
        );
        assert!(
            cursors.iter().max().unwrap() < texts.iter().min().unwrap(),
            "choice {choice}: the cursor overlays a choice label"
        );
    }
}

#[test]
fn luxe_detail_options_orange_header_contains_no_baked_label_pixels() {
    let decoded = detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap();
    for id in ["grad", "grad_l", "grad_r"] {
        let image = decoded
            .document
            .image
            .iter()
            .find(|i| i.id == format!("luxe_detail_{id}_orange"))
            .unwrap();
        let source = decoded
            .sources
            .iter()
            .find(|s| s.source_id == image.src)
            .unwrap()
            .asset
            .as_ref()
            .unwrap();
        for x in image.x..image.x + image.w {
            let mut previous_red = 255;
            for y in image.y..image.y + image.h {
                let offset = ((y as u32 * source.width + x as u32) * 4) as usize;
                let pixel = &source.pixels[offset..offset + 4];
                assert!(
                    pixel[0] > pixel[1] && pixel[1] > 0 && pixel[2] == 0 && pixel[3] == 255,
                    "{id} ({x}, {y}) contains a baked glyph instead of the orange gradient: {pixel:?}"
                );
                assert!(
                    pixel[0] <= previous_red,
                    "{id} ({x}, {y}) breaks the smooth header gradient"
                );
                previous_red = pixel[0];
            }
        }
    }
}

#[test]
fn luxe_detail_options_frames_follow_row_categories() {
    // Left edge of each rounded option card with the purple card's corners:
    // blue LANE / HI-SPEED and JUDGE, green GAUGE, orange LONG NOTE, red BGA and
    // the original purple ASSIST / MODIFIER.
    let card_x = |category| match category {
        1 | 7 => 483.0,
        2 => 749.0,
        4 => 414.0,
        6 => 713.0,
        5 => 607.0,
        _ => panic!("unexpected category {category}"),
    };
    let mut cases: Vec<_> = [104, 201, 601, 501]
        .map(|id| (id, snapshot(AppLocale::Ja, cursor_of(id))))
        .into_iter()
        .collect();
    // The exit starts from the settled columns with the rows it froze.
    let mut closing = snapshot(AppLocale::Ja, cursor_of(601));
    close(&mut closing, 0, 0);
    cases.push((0, closing));
    detail_options::assert_frames_follow_categories(&mut renderer(), cases, (7.0, 480.0), card_x);
}

#[test]
fn luxe_detail_options_handles_empty_locked_inactive_and_external_rows() {
    let mut renderer = renderer();
    for count in [0, 1, 3, 6] {
        let mut s = snapshot(AppLocale::Ja, cursor_of(104));
        let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
        panel.items = panel.items[..count].to_vec().into();
        renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
        for slot in 0..7 {
            let valid = s.detail_options.as_ref().unwrap().row(slot).is_some();
            assert_eq!(
                event_at(&renderer, &s, 138.0 + slot as f32 * 274.0, header_y(slot == 3)),
                valid.then_some(19310 + slot as i32)
            );
        }
    }
    let mut s = snapshot(AppLocale::Ja, cursor_of(304));
    let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
    let row = &mut std::sync::Arc::make_mut(&mut panel.items)[cursor_of(304)];
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
    assert_eq!(event_at(&renderer, &s, 960.0, choice_y(true)), Some(19313));
    let s = snapshot(AppLocale::Ja, cursor_of(201));
    assert!(!s.detail_options.as_ref().unwrap().selected().unwrap().effective);
    assert_eq!(event_at(&renderer, &s, 960.0, choice_y(true)), Some(19692));
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
                        DrawCommand::Text { text, origin, .. } if text == "LANE COVER" => {
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
                        // Number rows have no choice cell; their value box selects the column.
                        let number = matches!(
                            s.detail_options.as_ref().unwrap().row(slot as usize).unwrap().kind,
                            bmz_render::scene::detail_options::DetailValueKind::Number { .. }
                        );
                        let expected = if number { 19310 + slot } else { 19500 + slot * 64 };
                        assert_eq!(event_at(&renderer, &s, x, choice_y(slot == 3)), Some(expected));
                    }
                }
            } else {
                assert!(
                    !event_at(&renderer, &s, 960.0, CHOICE_Y)
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
    }
}

#[test]
#[ignore = "requires a GPU adapter; writes Luxez Flat panel previews to a temporary directory"]
fn luxe_detail_options_gpu_previews() {
    let temporary = unique_test_dir("bmz-luxe-detail-preview");
    let output = std::env::var_os("BMZ_SKIN_PREVIEW_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.to_path_buf());
    std::fs::create_dir_all(&output).unwrap();
    for (width, height) in
        [(1920, 1080), (3840, 2160), (1280, 720), (960, 540), (1024, 768), (2560, 1080)]
    {
        let mut renderer = renderer();
        renderer.set_default_font_search_paths(vec![test_app_paths().resource_dir.join("fonts")]);
        renderer.attach_offscreen(bmz_render::renderer::SurfaceSize { width, height }).unwrap();
        let mut scenes = Vec::new();
        for (locale, cursor, scroll) in [
            (AppLocale::Ja, cursor_of(104), 0.0),
            (AppLocale::En, cursor_of(303), 0.0),
            (AppLocale::Ja, cursor_of(201), 0.0),
            (AppLocale::Ja, cursor_of(401), 0.0),
            (AppLocale::En, cursor_of(401), 0.0),
            (AppLocale::Ja, cursor_of(304), 0.0),
            (AppLocale::Ja, cursor_of(702), 0.0),
            (AppLocale::En, cursor_of(702), 0.0),
            (AppLocale::En, cursor_of(501), -0.5),
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

#[test]
#[ignore = "requires a GPU adapter; verifies rounded cards and button matte at native and doubled size"]
fn luxe_detail_options_gpu_corners_match_the_original_card() {
    let decoded = detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap();
    let cards = [
        (1, "luxe_detail_panel3", 483, 712, 690, 957),
        (2, "luxe_detail_panel1", 749, 334, 956, 702),
        (4, "luxe_detail_panel3", 414, 133, 671, 460),
        (5, "luxe_detail_panel", 607, 172, 814, 355),
        (6, "luxe_detail_panel3", 713, 713, 920, 917),
    ];
    let assets: Vec<_> = cards
        .iter()
        .map(|(_, id, ..)| {
            decoded.sources.iter().find(|s| s.source_id == *id).unwrap().asset.clone().unwrap()
        })
        .collect();
    for scale in [1, 2] {
        let mut renderer = Renderer::default();
        install_decoded_skin(
            &mut renderer,
            detail_options::decode_experimental(&skin_path(), SkinKind::Select).unwrap(),
            bmz_render::skin::default_skin_manifest(),
        )
        .unwrap();
        renderer
            .attach_offscreen(bmz_render::renderer::SurfaceSize {
                width: 1920 * scale,
                height: 1080 * scale,
            })
            .unwrap();
        for ((category, _, l, t, r, b), asset) in cards.iter().zip(&assets) {
            let mut s = snapshot(AppLocale::Ja, cursor_of(702));
            let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
            std::sync::Arc::make_mut(&mut panel.items)[cursor_of(702)].category_id = *category;
            renderer.render_scene(AppSceneSnapshot::Select(s)).unwrap();
            let rgba = renderer.read_offscreen_rgba().unwrap();
            let pixel = |x: u32, y: u32| {
                let offset = ((y * 1920 * scale + x) * 4) as usize;
                &rgba[offset..offset + 4]
            };
            // No cursor glows over this numeric field. Its matte must reproduce
            // the body underneath at both ends and on both rows of a 2x texel.
            for x in [847, 1072] {
                for dx in 0..scale {
                    for y in [(244 - RAISE) * scale, (282 - RAISE) * scale - 1] {
                        assert_eq!(
                            pixel(x * scale + dx, y),
                            pixel(x * scale + dx, 400 * scale),
                            "button matte: category {category}, scale {scale}, x {x}, y {y}"
                        );
                    }
                }
            }
            // Each coloured corner pixel must retain the source's bevel and
            // curved inner glow, including the bottom's own shading.
            for right in [false, true] {
                for bottom in [false, true] {
                    for y in 0..24 {
                        for x in 0..24 {
                            let sx = if right { r - 1 - x } else { l + x };
                            let sy = if bottom { b - 2 - y } else { t + 1 + y };
                            let offset = ((sy * asset.width + sx) * 4) as usize;
                            let expected = &asset.pixels[offset..offset + 4];
                            if expected[3] != 255 || expected[..3].iter().all(|v| *v < 6) {
                                continue;
                            }
                            let dx = if right { 1088 - x } else { 831 + x };
                            let dy = if bottom { 729 - RAISE - y } else { 180 - RAISE + y };
                            assert_eq!(
                                pixel(dx * scale, dy * scale),
                                expected,
                                "corner: category {category}, scale {scale}, right {right}, bottom {bottom}, ({x}, {y})"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU adapter; verifies the selected glow across neighboring button corners"]
fn luxe_detail_options_gpu_glow_reaches_neighboring_button_corners() {
    for scale in [1, 2] {
        let mut renderer = renderer();
        renderer
            .attach_offscreen(bmz_render::renderer::SurfaceSize {
                width: 1920 * scale,
                height: 1080 * scale,
            })
            .unwrap();
        let scene = |index: i64| {
            let mut s = snapshot(AppLocale::Ja, cursor_of(104));
            let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
            let row = &mut std::sync::Arc::make_mut(&mut panel.items)[cursor_of(104)];
            row.value_index = index;
            row.value = if index < 0 { -999 } else { row.choices[index as usize].value };
            s
        };
        renderer.render_scene(AppSceneSnapshot::Select(scene(-1))).unwrap();
        let baseline = renderer.read_offscreen_rgba().unwrap();
        for (selected, neighbor_y) in [(0, 285 - RAISE), (1, 281 - RAISE)] {
            renderer.render_scene(AppSceneSnapshot::Select(scene(selected))).unwrap();
            let highlighted = renderer.read_offscreen_rgba().unwrap();
            for dx in 0..scale {
                let offset = ((neighbor_y * scale * 1920 * scale + 847 * scale + dx) * 4) as usize;
                assert_eq!(baseline[offset], 0, "the baseline corner is cyan");
                assert!(
                    highlighted[offset] > baseline[offset],
                    "choice {selected}, scale {scale}: the neighboring cap cuts off the orange halo"
                );
            }
        }
    }
}
