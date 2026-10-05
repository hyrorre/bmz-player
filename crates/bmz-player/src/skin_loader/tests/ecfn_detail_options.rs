use super::*;
use crate::i18n::AppLocale;
use crate::select_detail_options::CATALOG;
use bmz_render::scene::detail_options::{DetailOptionsClosingSnapshot, detail_options_column};
use bmz_render::skin::SkinClickTarget;

// ECFN is a separately installed skin, not a bundled submodule. These tests are
// explicitly opt-in and fail if the real ADFX02 assets are absent.
pub(super) fn skin_path() -> PathBuf {
    let path = test_app_paths().resource_dir.join("skins/ADFX02/ECFN/select/select.luaskin");
    assert!(path.exists(), "install the extended ADFX02/ECFN skin before running these tests");
    path
}

fn snapshot(locale: AppLocale, cursor: usize) -> SelectSnapshot {
    SelectSnapshot { time: TimeUs(2_000_000), ..detail_options::snapshot(locale, cursor) }
}

fn renderer() -> Renderer {
    let decoded = decode(true);
    assert!(decoded.document.uses_detail_options());
    assert!(decoded.document.bmz_detail_options_numbers);
    assert!(decoded.document.bmz_detail_options_close);
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
    renderer
}

pub(super) fn decode(experimental: bool) -> DecodedSkin {
    decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &skin_path(),
        kind: SkinKind::Select,
        options: &BTreeMap::from([(
            "bmz_detail_options".into(),
            if experimental { "1" } else { "0" }.into(),
        )]),
        files: &BTreeMap::new(),
        runtime_state: &LuaLoadRuntimeState::default(),
        library_roots: &test_app_paths().skin_library_roots(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
    })
    .unwrap()
}

pub(super) fn event_at(renderer: &Renderer, s: &SelectSnapshot, x: f32, y: f32) -> Option<i32> {
    renderer.select_skin_click_hit(s, x / 1920.0, y / 1080.0).and_then(|hit| match hit.target {
        SkinClickTarget::Event { event_id, .. } => Some(event_id),
        _ => None,
    })
}

pub(super) fn switching(from: u8, to: u8, elapsed: i64) -> SelectSnapshot {
    let mut s = snapshot(AppLocale::Ja, 0);
    if from == 2 {
        s.detail_options_closing = Some(DetailOptionsClosingSnapshot {
            panel: s.detail_options.take().unwrap(),
            scroll: s.detail_options_scroll,
        });
    } else if to != 2 {
        s.detail_options = None;
    }
    s.option_panel = to;
    s.option_panel_time = TimeUs(elapsed * 1000);
    if from != 0 {
        s.option_panel_off_times[usize::from(from - 1)] = Some(TimeUs(elapsed * 1000));
    }
    s
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_detail_options_all_items_and_scrolling_hits() {
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
                for absent in ["▼", "●", "+", "−"] {
                    assert!(!has_text(absent));
                }
                assert!(has_text(&panel.selected().unwrap().description));
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
                            event_at(&renderer, &s, x, 296.0 + index as f32 * 52.0),
                            Some(19500 + slot as i32 * 64 + index as i32 * 4)
                        );
                    }
                    if row.choices.is_empty() {
                        assert!(has_text(&row.value_label));
                        for dx in [-61.0, 61.0] {
                            assert_eq!(
                                event_at(&renderer, &s, x + dx, 355.0),
                                Some(19310 + slot as i32)
                            );
                        }
                    }
                }
                for (x, y) in [(960.0, 60.0), (130.0, 60.0), (1800.0, 800.0)] {
                    assert_eq!(event_at(&renderer, &s, x, y), None);
                }
                assert!(renderer.select_skin_slider_hit(&s, 0.5, 0.3).is_none());
            }
        }
    }
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_detail_options_empty_locked_external_and_inactive_values() {
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
    let mut s = snapshot(AppLocale::Ja, 14);
    let panel = std::sync::Arc::make_mut(s.detail_options.as_mut().unwrap());
    let row = &mut std::sync::Arc::make_mut(&mut panel.items)[14];
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
    assert_eq!(event_at(&renderer, &s, 960.0, 296.0), Some(19313));
    let s = snapshot(AppLocale::Ja, 3);
    assert!(!s.detail_options.as_ref().unwrap().selected().unwrap().effective);
    assert_eq!(event_at(&renderer, &s, 960.0, 296.0), Some(19692));
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_detail_options_atomic_activation_and_legacy_fallback() {
    let path = skin_path();
    let source = path.parent().unwrap();
    for state in ["loaded", "off", "missing", "failed", "non-bmz"] {
        let root = unique_test_dir("bmz-ecfn-detail-fallback");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(source.join("select.lua"), root.join("select.lua")).unwrap();
        std::fs::copy(
            source.join("bmz_select_extensions.lua"),
            root.join("bmz_select_extensions.lua"),
        )
        .unwrap();
        if state != "missing" {
            std::fs::copy(
                source.join("bmz_detail_options.lua"),
                root.join("bmz_detail_options.lua"),
            )
            .unwrap();
        }
        if state == "failed" {
            // Fail after constructing the entire extension, before publication.
            let file = root.join("bmz_detail_options.lua");
            let text = std::fs::read_to_string(&file).unwrap();
            std::fs::write(
                file,
                text.replace(
                    "skin.bmzDetailOptions = 1",
                    "error('test failure')\n skin.bmzDetailOptions = 1",
                ),
            )
            .unwrap();
        }
        std::fs::write(
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
                if state == "off" { "0" } else { "1" }.into(),
            )]),
            &BTreeMap::new(),
        )
        .unwrap();
        let active = state == "loaded";
        assert_eq!(
            loaded.document.text.iter().any(|t| t.id == "bmz_select_mode"),
            state != "non-bmz",
            "basic extensions survive independent detail failure: {state}"
        );
        assert_eq!(loaded.document.uses_detail_options(), active, "{state}");
        assert_eq!(loaded.document.bmz_detail_options_numbers, active, "{state}");
        assert_eq!(loaded.document.bmz_detail_options_close, active, "{state}");
        let has = |id: &str, timer| {
            loaded.document.destination.iter().any(|entry| {
                matches!(entry,
            DestinationListEntry::Single(d) if d.id == id && d.timer == Some(timer))
            })
        };
        assert_eq!(has("assist-panel1", 22), !active, "{state}");
        assert!(has("option-panel1", 21));
        assert_eq!(has("option-panel1", 31), active);
        assert!(has("subop-panel1", 23));
        assert_eq!(loaded.document.text.iter().any(|t| t.id.starts_with("ecfn_detail_")), active);
        std::fs::remove_dir_all(root).unwrap();
    }
    let old = decode(false);
    let new = decode(true);
    assert_eq!(old.sources.len(), new.sources.len(), "reuse the original assets");
    assert!(new.fonts.iter().any(|font| font.stored_id == "select:ecfn_detail_font"
        && font.path.ends_with("GenEiMGothic2-Heavy.ttf")
        && matches!(&font.data, Some(DecodedFontData::Vector(bytes)) if !bytes.is_empty())));
    assert!(
        new.document
            .text
            .iter()
            .filter(|t| t.id.starts_with("ecfn_detail_"))
            .all(|t| t.outline_width == 0.0 && t.shadow_color.is_empty())
    );
    // The original normal/suboption definitions (including fixed events) stay intact.
    for entry in &old.document.destination {
        if let DestinationListEntry::Single(d) = entry
            && matches!(d.timer, Some(21 | 23))
            && d.id != "-110"
        {
            assert!(
                new.document
                    .destination
                    .iter()
                    .any(|other| format!("{entry:?}") == format!("{other:?}")),
                "unchanged {}",
                d.id
            );
        }
    }
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_detail_options_parallel_fades_and_single_shade() {
    let mut renderer = renderer();
    for (from, to) in [(0, 1), (0, 2), (1, 0), (2, 0), (1, 2), (2, 1)] {
        for elapsed in [0, 50, 100, 150, 199, 200, 300] {
            let s = switching(from, to, elapsed);
            renderer.prepare_scene(AppSceneSnapshot::Select(s.clone()));
            let commands = &renderer.last_plan().unwrap().commands;
            for (panel, entering) in [(from, false), (to, true)] {
                if panel == 0 {
                    continue;
                }
                let expected = if entering {
                    elapsed.min(200) as f32 / 200.0
                } else {
                    1.0 - elapsed.min(200) as f32 / 200.0
                };
                let alpha = commands
                    .iter()
                    .find_map(|command| match command {
                        DrawCommand::Image { rect, tint, .. }
                            if panel == 1
                                && (rect.width * 1920.0 - 932.0).abs() < 0.1
                                && (rect.height * 1080.0 - 903.0).abs() < 0.1 =>
                        {
                            Some(tint.a)
                        }
                        DrawCommand::Text { text, style, .. }
                            if panel == 2 && text == "SUDDEN+" =>
                        {
                            Some(style.color.a)
                        }
                        _ => None,
                    })
                    .unwrap_or(0.0);
                assert!(
                    (alpha - expected).abs() < 0.01,
                    "{from}->{to} {elapsed}ms panel {panel}: {alpha} vs {expected}"
                );
            }
            let shades: Vec<_> = commands
                .iter()
                .filter_map(|cmd| match cmd {
                    DrawCommand::Rect { rect, color }
                        if rect.width == 1.0
                            && rect.height == 1.0
                            && rect.x == 0.0
                            && rect.y == 0.0
                            && color.r == 0.0
                            && color.a > 0.0 =>
                    {
                        Some(color.a)
                    }
                    _ => None,
                })
                .collect();
            assert!(shades.len() <= 1, "{from}->{to}, {elapsed}: {shades:?}");
            let expected = if from == 0 {
                elapsed.min(200) as f32 / 200.0
            } else if to == 0 {
                1.0 - elapsed.min(200) as f32 / 200.0
            } else {
                (elapsed.min(200) as f32 / 200.0).max(1.0 - elapsed.min(200) as f32 / 200.0)
            };
            assert!(
                (shades.first().copied().unwrap_or(0.0) - expected * 160.0 / 255.0).abs() < 0.01,
                "{from}->{to} {elapsed}ms shade {shades:?}, expected {expected}"
            );
            if to != 2 {
                assert!(
                    !event_at(&renderer, &s, 960.0, 296.0)
                        .is_some_and(bmz_render::skin::is_detail_options_event)
                );
            }
            if to == 0 && from == 2 && elapsed < 300 {
                assert_eq!(event_at(&renderer, &s, 130.0, 60.0), None);
                assert!(renderer.select_skin_slider_hit(&s, 0.5, 0.3).is_none());
            }
        }
    }
    // Reopening suppresses the same panel's stale exit; reversing the other
    // panel during its entrance still cannot double the dark overlay.
    let mut s = switching(1, 2, 50);
    s.option_panel_off_times[1] = Some(TimeUs(100_000));
    renderer.prepare_scene(AppSceneSnapshot::Select(s));
    assert_eq!(
        renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter(|cmd| matches!(cmd,
        DrawCommand::Text { text, .. } if text == "SUDDEN+"))
            .count(),
        1
    );
}

#[test]
#[ignore = "requires ADFX02/ECFN assets and a GPU; writes previews to a temporary directory"]
fn ecfn_detail_options_gpu_previews() {
    let output = unique_test_dir("bmz-ecfn-detail-preview");
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
        if width == 1920 {
            for (from, to) in [(2, 0), (2, 1), (1, 2)] {
                for elapsed in [0, 50, 100, 150, 200, 300] {
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
            image.save(output.join(format!("ecfn-{label}-{width}x{height}.png"))).unwrap();
        }
    }
    println!("ECFN DETAIL OPTIONS previews: {}", output.display());
}
