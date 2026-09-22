use super::*;

#[test]
fn cached_vector_glyph_uses_supersampled_atlas_pixels() {
    let Some(font) = load_default_font() else { return };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let Some(glyph) =
        atlas.cached_vector_glyph(DEFAULT_TEXT_FONT_ID, 'A', PxScale::from(24.0), &font)
    else {
        return;
    };

    assert!(glyph.width as f32 > glyph.display_width);
    assert!(glyph.height as f32 > glyph.display_height);
}

#[test]
fn cached_vector_glyph_distinguishes_horizontal_and_vertical_scale() {
    let Some(font) = load_default_font() else { return };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let wide = atlas
        .cached_vector_glyph(DEFAULT_TEXT_FONT_ID, 'A', PxScale { x: 12.0, y: 24.0 }, &font)
        .expect("glyph should render");
    let uniform = atlas
        .cached_vector_glyph(DEFAULT_TEXT_FONT_ID, 'A', PxScale::from(12.0), &font)
        .expect("glyph should render");

    assert!(wide.display_height > uniform.display_height);
    assert_eq!(atlas.glyphs.len(), 2);
}

#[test]
fn vector_text_shrink_preserves_height_by_default_and_supports_uniform_mode() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 100, height: 100 };
    let render = |overflow| {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "WWWW".to_string(),
                caret: Some(TextCaret { byte_index: 4, color: Color::rgb(1.0, 1.0, 1.0) }),
                post_scale: Point { x: 1.0, y: 1.0 },
                style: TextStyle {
                    font_id: None,
                    size: 0.2,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.2,
                    overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let frame = build_text_frame_with_cache(
            &plan,
            &font,
            &HashMap::new(),
            &HashMap::new(),
            surface,
            &mut atlas,
        );
        assert!(frame.instances.len() >= TEXT_INSTANCE_BYTES);
        let quad_y = f32::from_le_bytes(frame.instances[4..8].try_into().unwrap());
        let quad_width = f32::from_le_bytes(frame.instances[8..12].try_into().unwrap());
        let quad_height = f32::from_le_bytes(frame.instances[12..16].try_into().unwrap());
        let caret = frame.command_caret_rects[0].expect("caret should be laid out");
        (quad_y, quad_width, quad_height, caret.rect.y, caret.rect.height)
    };

    let natural = render(TextOverflow::Overflow);
    let horizontal = render(TextOverflow::Shrink);
    let uniform = render(TextOverflow::ShrinkUniform);

    assert!(horizontal.1 < natural.1);
    assert_approx(horizontal.0, natural.0);
    assert_approx(horizontal.2, natural.2);
    assert_approx(horizontal.3, natural.3);
    assert_approx(horizontal.4, natural.4);
    assert!(uniform.2 < horizontal.2);
    assert!(uniform.4 < horizontal.4);
    assert!(uniform.3 > horizontal.3);
}

#[test]
fn text_atlas_resets_when_height_reaches_limit() {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    // 上限を超える行を積み、アトラス高さを限界まで成長させる。
    let glyph_height = 64;
    while atlas.atlas_height() < TEXT_ATLAS_MAX_HEIGHT {
        for _ in 0..(TEXT_ATLAS_WIDTH / 32) {
            atlas.reserve(16, glyph_height);
        }
    }
    assert!(atlas.atlas_height() >= TEXT_ATLAS_MAX_HEIGHT);

    // フレーム境界でリセットされ、GPU テクスチャ上限を超えない高さに戻る。
    atlas.begin_frame();
    assert_eq!(atlas.pen_y, 0);
    assert_eq!(atlas.pen_x, 0);
    assert!(atlas.atlas_height() < TEXT_ATLAS_MAX_HEIGHT);
    assert!(atlas.glyphs.is_empty());
    assert_eq!(atlas.layouts.len(), 0);
}

#[test]
fn text_outline_emits_surrounding_text_instances() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 320, height: 240 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: None,
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: Some(crate::plan::TextOutline {
                    color: Color::rgba(0.0, 0.0, 0.0, 0.5),
                    width: 0.01,
                }),
                shadow: None,
            },
        }],
    };
    let frame = build_text_frame(&plan, &font, &HashMap::new(), &HashMap::new(), surface);

    assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES * 9);
}

#[test]
fn load_default_font_prefers_japanese_capable_font() {
    let Some(font) = load_default_font() else { return };
    // CJK 対応フォントが環境にあれば、必ずそれが採用されていなければならない。
    let cjk_available = bmz_font::resolve_system_font(true).is_some();
    if cjk_available {
        assert!(font_supports_japanese(&font));
    }
}

#[test]
fn default_font_fallback_chain_prefers_requested_coverage() {
    for coverage in bmz_font::ALL_FONT_COVERAGES {
        if bmz_font::resolve_system_font_for_coverage(coverage).is_none() {
            continue;
        }
        let fonts = load_default_font_fallbacks(coverage, &[]);
        let Some(primary) = fonts.primary() else {
            panic!("resolved {coverage:?} font should be loadable");
        };
        assert!(
            coverage.glyph_probes().iter().all(|ch| primary.font.glyph_id(*ch).0 != 0),
            "primary face should match requested {coverage:?} coverage"
        );
    }
}

#[test]
fn bundled_noto_cjk_supplies_ui_fallbacks() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/fonts/noto-cjk");
    let font_roots = vec![root];
    let fallbacks = load_cjk_font_fallback_data(bmz_font::FontCoverage::Japanese, &font_roots);

    assert!(fallbacks.iter().any(|(coverage, data)| {
        *coverage == bmz_font::FontCoverage::Japanese
            && bmz_font::font_supports_coverage(
                &data.bytes,
                data.font_index,
                bmz_font::FontCoverage::Japanese,
            )
    }));
}

#[test]
fn default_font_fallback_uses_selected_face_in_glyph_and_layout_cache_keys() {
    let fonts = load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[]);
    let Some(primary) = fonts.primary() else { return };
    let fallback_char = bmz_font::ALL_FONT_COVERAGES
        .iter()
        .flat_map(|coverage| coverage.glyph_probes())
        .copied()
        .find(|ch| {
            fonts.select(*ch).is_some_and(|selected| {
                selected.cache_id != primary.cache_id && selected.font.glyph_id(*ch).0 != 0
            })
        });
    let Some(fallback_char) = fallback_char else {
        return;
    };
    let selected_id = fonts.select(fallback_char).unwrap().cache_id.clone();
    let surface = SurfaceSize { width: 320, height: 240 };
    let text = format!("A{fallback_char}");
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: text.clone(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: None,
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

    let frame = build_text_frame_with_fallback_cache(
        &plan,
        &fonts,
        &HashMap::new(),
        &HashMap::new(),
        surface,
        &mut atlas,
    );

    assert!(!frame.instances.is_empty());
    assert!(
        atlas.glyphs.keys().any(|key| { key.ch == fallback_char && key.font_id == selected_id })
    );
    assert!(
        !atlas.glyphs.keys().any(|key| { key.ch == fallback_char && key.font_id != selected_id })
    );
    assert!(
        atlas
            .layouts
            .entries
            .keys()
            .any(|key| { key.text == text && key.font_id.contains(&selected_id) })
    );
}

#[test]
fn explicit_vector_font_does_not_use_default_fallback_faces() {
    let default_fonts = load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[]);
    let Some(primary) = default_fonts.primary() else { return };
    let fallback_char = bmz_font::ALL_FONT_COVERAGES
        .iter()
        .flat_map(|coverage| coverage.glyph_probes())
        .copied()
        .find(|ch| {
            primary.font.glyph_id(*ch).0 == 0
                && default_fonts
                    .select(*ch)
                    .is_some_and(|selected| selected.font.glyph_id(*ch).0 != 0)
        });
    let Some(fallback_char) = fallback_char else {
        return;
    };
    let custom_id = "skin:custom";
    let mut custom_fonts = HashMap::new();
    custom_fonts.insert(custom_id.to_string(), primary.font.clone());
    let surface = SurfaceSize { width: 320, height: 240 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: fallback_char.to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: Some(custom_id.to_string()),
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

    build_text_frame_with_fallback_cache(
        &plan,
        &default_fonts,
        &custom_fonts,
        &HashMap::new(),
        surface,
        &mut atlas,
    );

    assert!(atlas.glyphs.keys().all(|key| key.font_id == custom_id));
    assert!(atlas.layouts.entries.keys().all(|key| key.font_id == custom_id));
}

#[test]
fn explicit_vector_font_renders_without_default_fallback() {
    let Some(font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 320, height: 240 };
    let mut fonts = HashMap::new();
    fonts.insert("skin:custom".to_string(), font);
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: Some("skin:custom".to_string()),
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

    let frame = build_text_frame_with_fallback_cache(
        &plan,
        &FontFallbackChain::default(),
        &fonts,
        &HashMap::new(),
        surface,
        &mut atlas,
    );

    assert!(!frame.instances.is_empty());
}

#[test]
fn text_without_any_font_is_skipped_without_default_fallback() {
    let surface = SurfaceSize { width: 320, height: 240 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: None,
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

    let frame = build_text_frame_with_fallback_cache(
        &plan,
        &FontFallbackChain::default(),
        &HashMap::new(),
        &HashMap::new(),
        surface,
        &mut atlas,
    );

    assert!(frame.instances.is_empty());
    assert_eq!(frame.command_quad_counts, vec![0]);
}

#[test]
fn japanese_text_emits_glyph_quads_with_default_font() {
    let Some(font) = load_default_font() else { return };
    if !font_supports_japanese(&font) {
        return;
    }
    let surface = SurfaceSize { width: 320, height: 240 };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "日本語と記号★♪".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: None,
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };
    let frame = build_text_frame(&plan, &font, &HashMap::new(), &HashMap::new(), surface);

    assert!(!frame.instances.is_empty());
    assert!(frame.pixels.contains(&255));
}

#[test]
fn bitmap_font_text_uses_registered_font() {
    let surface = SurfaceSize { width: 320, height: 240 };
    let mut pages = HashMap::new();
    pages.insert(
        0,
        crate::bitmap_font::BitmapFontPage {
            id: 0,
            path: std::path::PathBuf::from("page.png"),
            image: crate::assets::RgbaImageAsset {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
            },
        },
    );
    let mut glyphs = HashMap::new();
    glyphs.insert(
        'A',
        crate::bitmap_font::BitmapFontGlyph {
            id: 'A',
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            xoffset: 0,
            yoffset: 0,
            xadvance: 1,
            page: 0,
        },
    );
    let mut bitmap_fonts = HashMap::new();
    bitmap_fonts.insert(
        "bitmap".to_string(),
        BitmapFont {
            size: 10,
            line_height: 10,
            base: 8,
            ascent: 7.0,
            scale_width: 1,
            scale_height: 1,
            pages: pages.into(),
            glyphs: glyphs.into(),
        },
    );
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: Some("bitmap".to_string()),
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };

    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let frame = build_text_frame_with_fallback_cache(
        &plan,
        &FontFallbackChain::default(),
        &HashMap::new(),
        &bitmap_fonts,
        surface,
        &mut atlas,
    );

    assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES);
    assert!(!frame.dirty_regions.is_empty());
}

#[test]
fn bitmap_glyph_non_integer_scale_uses_interpolated_alpha() {
    let page = crate::bitmap_font::BitmapFontPage {
        id: 0,
        path: std::path::PathBuf::from("page.png"),
        image: crate::assets::RgbaImageAsset {
            width: 2,
            height: 1,
            pixels: vec![255, 255, 255, 0, 255, 255, 255, 255],
        },
    };
    let glyph = crate::bitmap_font::BitmapFontGlyph {
        id: 'A',
        x: 0,
        y: 0,
        width: 2,
        height: 1,
        xoffset: 0,
        yoffset: 0,
        xadvance: 2,
        page: 0,
    };

    let pixels = rasterized_bitmap_glyph_pixels(glyph, &page, PxScale::from(1.5), 3, 1);
    let middle_alpha = pixels[7];

    assert!(middle_alpha > 0 && middle_alpha < 255);
}

#[test]
fn bitmap_font_text_positions_glyphs_from_destination_baseline() {
    let Some(default_font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 100, height: 100 };
    let mut pages = HashMap::new();
    pages.insert(
        0,
        crate::bitmap_font::BitmapFontPage {
            id: 0,
            path: std::path::PathBuf::from("page.png"),
            image: crate::assets::RgbaImageAsset {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
            },
        },
    );
    let mut glyphs = HashMap::new();
    glyphs.insert(
        'A',
        crate::bitmap_font::BitmapFontGlyph {
            id: 'A',
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            xoffset: 0,
            yoffset: 7,
            xadvance: 1,
            page: 0,
        },
    );
    let mut bitmap_fonts = HashMap::new();
    bitmap_fonts.insert(
        "bitmap".to_string(),
        BitmapFont {
            size: 30,
            line_height: 45,
            base: 34,
            ascent: 12.0,
            scale_width: 1,
            scale_height: 1,
            pages: pages.into(),
            glyphs: glyphs.into(),
        },
    );
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: Some("bitmap".to_string()),
                size: 0.3,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };

    let frame = build_text_frame(&plan, &default_font, &HashMap::new(), &bitmap_fonts, surface);
    let y = f32::from_le_bytes(frame.instances[4..8].try_into().unwrap());

    assert!((y - 0.05).abs() < f32::EPSILON);
}

#[test]
fn bitmap_font_shrink_selects_horizontal_or_uniform_scaling() {
    let Some(default_font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 100, height: 100 };
    let mut pages = HashMap::new();
    pages.insert(
        0,
        crate::bitmap_font::BitmapFontPage {
            id: 0,
            path: std::path::PathBuf::from("page.png"),
            image: crate::assets::RgbaImageAsset {
                width: 10,
                height: 10,
                pixels: vec![255; 10 * 10 * 4],
            },
        },
    );
    let mut glyphs = HashMap::new();
    glyphs.insert(
        'A',
        crate::bitmap_font::BitmapFontGlyph {
            id: 'A',
            x: 0,
            y: 0,
            width: 10,
            height: 10,
            xoffset: 0,
            yoffset: 7,
            xadvance: 10,
            page: 0,
        },
    );
    let mut bitmap_fonts = HashMap::new();
    bitmap_fonts.insert(
        "bitmap".to_string(),
        BitmapFont {
            size: 10,
            line_height: 10,
            base: 7,
            ascent: 7.0,
            scale_width: 10,
            scale_height: 10,
            pages: pages.into(),
            glyphs: glyphs.into(),
        },
    );
    let render = |overflow| {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "AAAA".to_string(),
                caret: Some(TextCaret { byte_index: 4, color: Color::rgb(1.0, 1.0, 1.0) }),
                post_scale: Point { x: 1.0, y: 1.0 },
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.2,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.4,
                    overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let frame = build_text_frame_with_cache(
            &plan,
            &default_font,
            &HashMap::new(),
            &bitmap_fonts,
            surface,
            &mut atlas,
        );
        let y = f32::from_le_bytes(frame.instances[4..8].try_into().unwrap());
        let width = f32::from_le_bytes(frame.instances[8..12].try_into().unwrap());
        let height = f32::from_le_bytes(frame.instances[12..16].try_into().unwrap());
        let caret = frame.command_caret_rects[0].expect("caret should be laid out");
        (y, width, height, caret.rect.y, caret.rect.height)
    };

    let natural = render(TextOverflow::Overflow);
    let horizontal = render(TextOverflow::Shrink);
    let uniform = render(TextOverflow::ShrinkUniform);

    assert!(horizontal.1 < natural.1);
    assert_approx(horizontal.0, natural.0);
    assert_approx(horizontal.2, natural.2);
    assert_approx(horizontal.3, natural.3);
    assert_approx(horizontal.4, natural.4);
    assert_approx(uniform.0, 0.15);
    assert!(uniform.2 < horizontal.2);
    assert!(uniform.4 < horizontal.4);
}

#[test]
fn bitmap_font_text_uses_bitmap_size_for_scale() {
    let Some(default_font) = load_default_font() else { return };
    let surface = SurfaceSize { width: 100, height: 100 };
    let mut pages = HashMap::new();
    pages.insert(
        0,
        crate::bitmap_font::BitmapFontPage {
            id: 0,
            path: std::path::PathBuf::from("page.png"),
            image: crate::assets::RgbaImageAsset {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
            },
        },
    );
    let mut glyphs = HashMap::new();
    glyphs.insert(
        'A',
        crate::bitmap_font::BitmapFontGlyph {
            id: 'A',
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            xoffset: 0,
            yoffset: 0,
            xadvance: 1,
            page: 0,
        },
    );
    let mut bitmap_fonts = HashMap::new();
    bitmap_fonts.insert(
        "bitmap".to_string(),
        BitmapFont {
            size: 10,
            line_height: 10,
            base: 8,
            ascent: 7.0,
            scale_width: 1,
            scale_height: 1,
            pages: pages.into(),
            glyphs: glyphs.into(),
        },
    );
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "A".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: TextStyle {
                font_id: Some("bitmap".to_string()),
                size: 0.3,
                bitmap_size: Some(0.1),
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }],
    };

    let frame = build_text_frame(&plan, &default_font, &HashMap::new(), &bitmap_fonts, surface);
    let width = f32::from_le_bytes(frame.instances[8..12].try_into().unwrap());

    assert!((width - 0.01).abs() < f32::EPSILON);
}
