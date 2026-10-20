use super::*;

fn fixture_face(name: &str) -> TextFace {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fonts").join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    TextFace::from_vec(name, bytes, 0).expect("fixture font should parse")
}

fn latin() -> TextFace {
    fixture_face("bmz-test-latin.ttf")
}

fn symbols() -> TextFace {
    fixture_face("bmz-test-symbols.ttf")
}

fn text_style(font_id: Option<&str>) -> TextStyle {
    TextStyle {
        font_id: font_id.map(str::to_string),
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
    }
}

fn text_command(text: &str, style: TextStyle) -> DrawCommand {
    DrawCommand::Text {
        origin: Point { x: 0.1, y: 0.1 },
        text: text.to_string(),
        caret: None,
        post_scale: Point { x: 1.0, y: 1.0 },
        style,
    }
}

const SURFACE: SurfaceSize = SurfaceSize { width: 400, height: 200 };

/// 1つの明示フォントで組んだ quad。
fn vector_quads(text: &str, face: &TextFace, style: TextStyle) -> Vec<TextQuad> {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    builder.push_text(
        &Point { x: 0.1, y: 0.1 },
        text,
        style,
        single_font_set("skin:test", face),
        SURFACE,
    );
    builder.quads
}

fn tofu_glyphs(shaped: &ShapedText) -> usize {
    shaped
        .clusters
        .iter()
        .flat_map(|cluster| shaped.glyphs(cluster))
        .filter(|glyph| matches!(glyph.source, GlyphSource::Tofu { .. }))
        .count()
}

#[test]
fn missing_vector_glyph_draws_generated_placeholder_and_keeps_width() {
    let face = latin();
    let shaped = shape_vector_text("A✡B", single_font_set("skin:test", &face));

    assert_eq!(shaped.clusters.len(), 3);
    assert_eq!(tofu_glyphs(&shaped), 1);
    assert!(shaped.clusters[1].advance > 0.0, "missing glyph must keep an advance");

    let with_missing = vector_quads("A✡B", &face, text_style(None));
    let without = vector_quads("AB", &face, text_style(None));
    assert_eq!(with_missing.len(), 3, "A, placeholder and B must all be drawn");
    let placeholder = &with_missing[1];
    assert!(placeholder.width > 0.0 && placeholder.height > 0.0);
    assert!(placeholder.x > with_missing[0].x);
    // 後続文字は欠字ぶん右へずれ、欠字の右に置かれる。
    let shift = with_missing[2].x - without[1].x;
    let advance_px = shaped.clusters[1].advance * 20.0 / SURFACE.width as f32;
    assert_approx(shift, advance_px);
    assert!(with_missing[2].x >= placeholder.x + placeholder.width);
}

#[test]
fn solomon_table_symbol_never_disappears() {
    let face = latin();
    let shaped = shape_vector_text("Solomon ✡ 12", single_font_set("skin:test", &face));
    let star = shaped.clusters.iter().find(|cluster| cluster.text == (8..11)).unwrap();
    assert!(star.advance > 0.0);
    assert!(matches!(shaped.glyphs(star)[0].source, GlyphSource::Tofu { .. }));
    // 空白は欠字表示しない。
    assert_eq!(tofu_glyphs(&shaped), 1);
}

#[test]
fn missing_glyph_uses_question_mark_candidate_before_generated_box() {
    let face = fixture_face("bmz-test-question.ttf");
    let shaped = shape_vector_text("A✡B", single_font_set("skin:test", &face));
    let glyph = shaped.glyphs(&shaped.clusters[1])[0];
    assert_eq!(
        glyph.source,
        GlyphSource::Vector { face: 0, glyph: face.glyph_id('?'), face_scale: 1.0, color: false }
    );
}

#[test]
fn missing_glyph_candidates_follow_beatoraja_order_across_faces() {
    let faces = [latin(), symbols()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    // ☀ はどの face にも無いため、□（U+25A1）を後続 face から探す。
    let shaped = shape_vector_text("A☀", fonts);
    let GlyphSource::Vector { face, glyph, .. } = shaped.glyphs(&shaped.clusters[1])[0].source
    else {
        panic!("candidate glyph expected");
    };
    assert_eq!((face, glyph), (1, faces[1].glyph_id('\u{25A1}')));
}

#[test]
fn actual_symbol_glyph_wins_over_placeholder() {
    let faces = [latin(), symbols()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    let shaped = shape_vector_text("A✡B\u{FFFD}", fonts);
    assert_eq!(tofu_glyphs(&shaped), 0);
    for (index, ch) in [(1, '✡'), (3, '\u{FFFD}')] {
        let GlyphSource::Vector { face, glyph, .. } =
            shaped.glyphs(&shaped.clusters[index])[0].source
        else {
            panic!("real glyph expected");
        };
        assert_eq!((face, glyph), (1, faces[1].glyph_id(ch)), "{ch}");
    }
}

#[test]
fn fallback_face_glyphs_use_primary_em_size() {
    let faces = [latin(), symbols()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    let shaped = shape_vector_text("✡", fonts);
    // symbols は em 2048、✡ の送り幅は1em。latin の em（1000/1000）と揃う。
    assert_approx(shaped.clusters[0].advance, fonts.em());
    let GlyphSource::Vector { face_scale, .. } = shaped.glyphs(&shaped.clusters[0])[0].source
    else {
        panic!("vector glyph expected");
    };
    let symbols_metrics = faces[1].metrics();
    assert_approx(face_scale, symbols_metrics.height() / symbols_metrics.units_per_em);
}

#[test]
fn zero_width_controls_and_spaces_are_not_drawn_as_placeholders() {
    let face = latin();
    let text = "A\u{200D}\u{FE0F}\u{200B}\u{00AD}\r\u{E0041}\tB \u{3000}B";
    let shaped = shape_vector_text(text, single_font_set("skin:test", &face));
    assert_eq!(tofu_glyphs(&shaped), 0);
    let width = |range: std::ops::Range<usize>| -> f32 {
        shaped.clusters.iter().filter(|c| range.contains(&c.text.start)).map(|c| c.advance).sum()
    };
    // ZWJ・異体字セレクタ・ZWSP・SHY・CR・タグ文字は幅0。
    assert_eq!(width(1..text.find('\t').unwrap()), 0.0);
    let space = face.h_advance_unscaled(face.glyph_id(' ')) / face.metrics().height();
    let tab = shaped.clusters.iter().find(|c| &text[c.text.clone()] == "\t").unwrap();
    assert_approx(tab.advance, space);
    // フォントに無い全角空白は1em。
    let ideographic = shaped.clusters.iter().find(|c| &text[c.text.clone()] == "\u{3000}").unwrap();
    assert_approx(ideographic.advance, single_font_set("skin:test", &face).em());
}

#[test]
fn replacement_character_is_an_ordinary_character() {
    assert_eq!(char_class('\u{FFFD}'), CharClass::Visible);
    let face = latin();
    let shaped = shape_vector_text("\u{FFFD}", single_font_set("skin:test", &face));
    // フォントに無ければ他の欠字と同じく代替表示する。
    assert_eq!(tofu_glyphs(&shaped), 1);
}

#[test]
fn placeholder_keeps_alignment_truncate_shrink_and_wrapping() {
    let face = latin();
    let quads = |text: &str, edit: &dyn Fn(&mut TextStyle)| {
        let mut style = text_style(None);
        style.max_width = 0.5;
        edit(&mut style);
        vector_quads(text, &face, style)
    };
    let right = |style: &mut TextStyle| style.align = TextAlign::Right;
    let center = |style: &mut TextStyle| style.align = TextAlign::Center;
    let shaped = shape_vector_text("A✡", single_font_set("skin:test", &face));
    let missing_px = shaped.clusters[1].advance * 20.0 / SURFACE.width as f32;
    assert_approx(quads("A", &right)[0].x - quads("A✡", &right)[0].x, missing_px);
    assert_approx(quads("A", &center)[0].x - quads("A✡", &center)[0].x, missing_px / 2.0);

    // 幅0.15（60px）には A(12px)×2 と欠字1つ分までしか入らない。
    let truncate = |style: &mut TextStyle| {
        style.max_width = 0.15;
        style.overflow = TextOverflow::Truncate;
    };
    let truncated = quads("AA✡✡✡✡", &truncate);
    let fits = 2 + ((60.0 - 24.0) / (shaped.clusters[1].advance * 20.0)) as usize;
    assert_eq!(truncated.len(), fits);
    let last = truncated.last().unwrap();
    assert!((last.x + last.width) * SURFACE.width as f32 <= 0.1 * 400.0 + 60.0 + 0.01);

    let shrink = |style: &mut TextStyle| {
        style.max_width = 0.05;
        style.overflow = TextOverflow::Shrink;
    };
    let shrunk = quads("A✡A", &shrink);
    assert_eq!(shrunk.len(), 3);
    let right_edge = shrunk.last().unwrap().x + shrunk.last().unwrap().width;
    assert!(right_edge <= 0.1 + 0.05 + 0.001, "shrunk text must fit: {right_edge}");

    let wrap = |style: &mut TextStyle| {
        style.max_width = 0.07;
        style.wrapping = true;
    };
    let wrapped = quads("A✡A✡", &wrap);
    assert_eq!(wrapped.len(), 4);
    assert!(wrapped[2].y > wrapped[0].y, "third cluster should move to the second line");
    assert_approx(wrapped[2].x, wrapped[0].x);
}

#[test]
fn placeholder_is_decorated_like_other_glyphs() {
    let face = latin();
    let mut style = text_style(None);
    style.outline =
        Some(crate::plan::TextOutline { color: Color::rgba(0.0, 0.0, 0.0, 1.0), width: 0.01 });
    style.shadow = Some(crate::plan::TextShadow {
        color: Color::rgba(0.0, 0.0, 0.0, 1.0),
        offset: Point { x: 0.01, y: 0.01 },
    });
    assert_eq!(vector_quads("✡", &face, style.clone()).len(), 10);
    assert_eq!(vector_quads("A", &face, style).len(), 10);
}

#[test]
fn placeholder_scales_with_surface_and_is_cached_once() {
    let face = latin();
    let mut fonts = HashMap::new();
    fonts.insert("skin:test".to_string(), face.clone());
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![text_command("✡✡✡", text_style(Some("skin:test")))],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let small =
        build_text_frame_with_cache(&plan, &face, &fonts, &HashMap::new(), SURFACE, &mut atlas);
    assert_eq!(small.command_quad_counts, vec![3]);
    let tofu_keys = |atlas: &TextAtlasCache| {
        atlas.glyphs.keys().filter(|key| key.kind == TextGlyphKind::Tofu).count()
    };
    assert_eq!(tofu_keys(&atlas), 1, "identical placeholders share one atlas entry");
    assert_eq!(atlas.shapes.len(), 1);

    let large_surface = SurfaceSize { width: 800, height: 400 };
    let large = build_text_frame_with_cache(
        &plan,
        &face,
        &fonts,
        &HashMap::new(),
        large_surface,
        &mut atlas,
    );
    assert_eq!(large.command_quad_counts, vec![3]);
    assert_eq!(tofu_keys(&atlas), 2);
    // 解像度が変わっても解決結果は再利用する。
    assert_eq!(atlas.shapes.len(), 1);
    let quad_size = |frame: &TextFrame| {
        let floats: Vec<f32> = frame.instances[..TEXT_INSTANCE_BYTES]
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        (floats[2], floats[3])
    };
    let (small_w, small_h) = quad_size(&small);
    let (large_w, large_h) = quad_size(&large);
    assert_approx(small_w, large_w);
    assert_approx(small_h, large_h);
}

#[test]
fn generated_placeholder_pixels_are_an_opaque_frame() {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let glyph = atlas.cached_tofu_glyph(12.0, 16.0).unwrap();
    let pixels = atlas.pixels_for_size(atlas.size());
    let pixel = |x: u32, y: u32| {
        let index = (((glyph.atlas_origin.1 + y) * TEXT_ATLAS_WIDTH + glyph.atlas_origin.0 + x) * 4)
            as usize;
        pixels[index + 3]
    };
    assert_eq!(pixel(0, 0), 255);
    assert_eq!(pixel(glyph.width - 1, glyph.height / 2), 255);
    assert_eq!(pixel(glyph.width / 2, glyph.height / 2), 0, "the box must stay hollow");
}

fn bitmap_font_with(chars: &[(char, i32)]) -> BitmapFont {
    let mut font = test_bitmap_font();
    let mut glyphs = (*font.glyphs).clone();
    for (ch, advance) in chars {
        glyphs.insert(
            *ch,
            crate::bitmap_font::BitmapFontGlyph {
                id: *ch,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                xoffset: 0,
                yoffset: 0,
                xadvance: *advance,
                page: 0,
            },
        );
    }
    font.glyphs = glyphs.into();
    font
}

fn bitmap_quads(text: &str, font: &BitmapFont) -> Vec<TextQuad> {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    let mut style = text_style(Some("bitmap"));
    style.size = 0.05;
    builder.push_bitmap_text(
        &Point { x: 0.1, y: 0.1 },
        text,
        style,
        bitmap_set(&bitmap_entries(&[font])),
        SURFACE,
    );
    builder.quads
}

#[test]
fn missing_bitmap_glyph_is_not_skipped_silently() {
    let font = test_bitmap_font();
    let shaped = shape_bitmap_text("A✡A", bitmap_set(&bitmap_entries(&[&font])));
    assert_eq!(tofu_glyphs(&shaped), 1);
    assert!(shaped.clusters[1].advance > 0.0);

    let quads = bitmap_quads("A✡A", &font);
    let plain = bitmap_quads("AA", &font);
    assert_eq!(quads.len(), 3);
    let scale = 0.05 * SURFACE.height as f32 / font.size as f32;
    assert_approx(
        quads[2].x - plain[1].x,
        shaped.clusters[1].advance * scale / SURFACE.width as f32,
    );
    // 生成矩形は大文字上端から基準線までの高さ。
    let cap_height = (font.base as f32 - font.ascent) * scale / SURFACE.height as f32;
    assert_approx(quads[1].height, cap_height);
    assert_approx(quads[1].y, 0.1);
}

#[test]
fn bitmap_missing_glyph_uses_box_candidate_from_font() {
    let font = bitmap_font_with(&[('\u{25A2}', 3), ('?', 2)]);
    let shaped = shape_bitmap_text("A✡", bitmap_set(&bitmap_entries(&[&font])));
    assert_eq!(
        shaped.glyphs(&shaped.clusters[1])[0].source,
        GlyphSource::Bitmap { font: 0, ch: '\u{25A2}' }
    );
    assert_eq!(shaped.clusters[1].advance, 3.0);
}

#[test]
fn bitmap_wave_dash_and_fullwidth_tilde_substitute_each_other() {
    let font = bitmap_font_with(&[('\u{FF5E}', 4)]);
    let shaped = shape_bitmap_text("\u{301C}", bitmap_set(&bitmap_entries(&[&font])));
    assert_eq!(
        shaped.glyphs(&shaped.clusters[0])[0].source,
        GlyphSource::Bitmap { font: 0, ch: '\u{FF5E}' }
    );
}

#[test]
fn bitmap_spaces_and_controls_are_not_placeholders() {
    let font = test_bitmap_font();
    let shaped =
        shape_bitmap_text("A \u{3000}\u{200D}\u{FE0F}A", bitmap_set(&bitmap_entries(&[&font])));
    assert_eq!(tofu_glyphs(&shaped), 0);
    assert_eq!(bitmap_quads("A \u{3000}\u{200D}A", &font).len(), 2);
}

#[test]
fn bitmap_truncate_keeps_placeholder_clusters_whole() {
    let font = test_bitmap_font();
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    let mut style = text_style(Some("bitmap"));
    style.size = 0.05;
    // A の送り幅は1px（scale 1）。2.5px に A A は入るが、続く欠字は入らない。
    style.max_width = 0.00625;
    style.overflow = TextOverflow::Truncate;
    builder.push_bitmap_text(
        &Point { x: 0.1, y: 0.1 },
        "AA✡A",
        style,
        bitmap_set(&bitmap_entries(&[&font])),
        SURFACE,
    );
    assert_eq!(builder.quads.len(), 2);
}

/// JSON の TTF / .fnt 指定と LR2 system font（既定フォント）の各経路で、
/// 難易度表の ✡ を含む文字列から文字が消えないこと。
#[test]
fn skin_texts_keep_every_visible_character_across_font_kinds() {
    use crate::skin::{SkinDocumentRenderExt, SkinDrawState, SkinRenderItem, SkinTextState};
    let latin = latin();
    let mut fonts = HashMap::new();
    fonts.insert("skin:latin".to_string(), latin.clone());
    let mut bitmap_fonts = HashMap::new();
    bitmap_fonts.insert("skin:bitmap".to_string(), test_bitmap_font());
    let default_fonts = load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[]);
    for (font, lr2, text, expected) in [
        (Some("skin:latin"), false, "Solomon ✡ 12", 10),
        (Some("skin:bitmap"), false, "A✡A", 3),
        (None, true, "Solomon ✡ 12", 10),
    ] {
        if font.is_none() && default_fonts.primary().is_none() {
            continue;
        }
        let mut text_def = serde_json::json!({"id":"label", "constantText": text});
        if let Some(font) = font {
            text_def["font"] = font.into();
        }
        if lr2 {
            text_def["lr2SystemFont"] = serde_json::json!({"size":24, "thickness":0, "type":0});
        }
        let document: bmz_skin_document::SkinDocument = serde_json::from_value(serde_json::json!({
            "w":400, "h":200, "lr2": lr2,
            "text":[text_def],
            "destination":[{"id":"label","dst":[{"x":10,"y":10,"w":380,"h":24}]}]
        }))
        .unwrap();
        let items = document.static_render_items(
            &HashMap::new(),
            &SkinDrawState::default(),
            &SkinTextState::default(),
        );
        let SkinRenderItem::Text { origin, text, style, .. } = &items[0] else { panic!("text") };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: *origin,
                text: text.clone(),
                caret: None,
                post_scale: Point { x: 1.0, y: 1.0 },
                style: style.clone(),
            }],
        };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let frame = build_text_frame_with_fallback_cache(
            &plan,
            &default_fonts,
            &fonts,
            &bitmap_fonts,
            SURFACE,
            &mut atlas,
        );
        assert_eq!(frame.command_quad_counts, vec![expected], "{font:?} lr2={lr2}");
    }
}

fn glyph_face(shaped: &ShapedText, cluster: usize) -> Option<u16> {
    match shaped.glyphs(&shaped.clusters[cluster]).first()?.source {
        GlyphSource::Vector { face, .. } => Some(face),
        _ => None,
    }
}

#[test]
fn skin_font_fallbacks_are_searched_before_system_fonts() {
    let mut renderer = Renderer::default();
    renderer.insert_vector_font("skin:main".into(), latin());
    renderer.insert_vector_font("skin:main#fallback0".into(), symbols());
    renderer.set_font_fallbacks("skin:main", vec!["skin:main#fallback0".into()]);
    // OS 側にも ✡ を持つ face があっても、スキンの fallback を先に使う。
    let system = FontFallbackChain {
        faces: vec![latin(), fixture_face("bmz-test-color.ttf")],
        default_len: 1,
    };
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![text_command("A✡", text_style(Some("skin:main")))],
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let frame =
        build_text_frame_with_font_sets(&plan, &system, &renderer.font_sets, SURFACE, &mut atlas);
    assert_eq!(frame.command_quad_counts, vec![2]);
    let ids: Vec<_> = atlas.glyphs.keys().map(|key| key.font_id.to_string()).collect();
    assert!(ids.contains(&"bmz-test-latin.ttf".to_string()));
    assert!(ids.contains(&"bmz-test-symbols.ttf".to_string()), "{ids:?}");
    assert!(!ids.contains(&"bmz-test-color.ttf".to_string()));

    // fallback 指定を外すと OS 側の face を使う。同じ指定の再設定は何もしない。
    renderer.set_font_fallbacks("skin:main", vec!["skin:main#fallback0".into()]);
    renderer.set_font_fallbacks("skin:main", Vec::new());
    assert!(renderer.font_fallbacks.is_empty());
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    build_text_frame_with_font_sets(&plan, &system, &renderer.font_sets, SURFACE, &mut atlas);
    let ids: Vec<_> = atlas.glyphs.keys().map(|key| key.font_id.to_string()).collect();
    assert!(ids.contains(&"bmz-test-color.ttf".to_string()), "{ids:?}");
}

#[test]
fn system_symbol_faces_follow_default_and_skin_faces() {
    let system = FontFallbackChain { faces: vec![latin(), symbols()], default_len: 1 };
    let default_set = VectorFontSet { id: DEFAULT_TEXT_FONT_ID, faces: system.default_faces() };
    let shaped = shape_vector_text("A✡☀", default_set);
    assert_eq!(glyph_face(&shaped, 0), Some(0));
    assert_eq!(glyph_face(&shaped, 1), Some(1));
    // どこにも無い ☀ は □ の候補（記号 face）で表示する。
    assert_eq!(tofu_glyphs(&shaped), 0);
    assert_eq!(glyph_face(&shaped, 2), Some(1));

    let skin = [fixture_face("bmz-test-question.ttf")];
    let skin_set = VectorFontSet { id: "skin:q", faces: system.after(&skin) };
    let shaped = shape_vector_text("A✡", skin_set);
    // スキン → 既定（latin） → 記号の順。A はスキン、✡ は記号 face（通し番号2）。
    assert_eq!(glyph_face(&shaped, 0), Some(0));
    assert_eq!(glyph_face(&shaped, 1), Some(2));
}

#[test]
fn font_sets_ignore_self_references_duplicates_and_unknown_ids() {
    let mut fonts = HashMap::new();
    fonts.insert("a".to_string(), latin());
    fonts.insert("b".to_string(), symbols());
    let mut bitmap_fonts = HashMap::new();
    bitmap_fonts.insert("fnt".to_string(), test_bitmap_font());
    bitmap_fonts.insert("fnt2".to_string(), bitmap_font_with(&[('✡', 2)]));
    let fallbacks = HashMap::from([
        ("a".to_string(), vec!["a".into(), "b".into(), "b".into(), "missing".into(), "fnt".into()]),
        ("fnt".to_string(), vec!["fnt2".into(), "b".into()]),
    ]);
    let sets = SkinFontSets::new(&fonts, &bitmap_fonts, &fallbacks);
    let vector: Vec<_> = sets.vector["a"].iter().map(|face| face.cache_id().to_string()).collect();
    assert_eq!(vector, ["bmz-test-latin.ttf", "bmz-test-symbols.ttf"]);
    let bitmap = &sets.bitmap["fnt"];
    let ids: Vec<_> = bitmap.fonts.iter().map(|entry| entry.id.to_string()).collect();
    assert_eq!(ids, ["fnt", "fnt2"]);
    assert_eq!(bitmap.faces.len(), 1);
}

#[test]
fn bitmap_text_uses_fallback_fnt_then_vector_faces_before_placeholders() {
    let primary = test_bitmap_font();
    let fallback = bitmap_font_with(&[('♠', 3)]);
    let entries = bitmap_entries(&[&primary, &fallback]);
    let faces = [symbols()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&faces) };
    let shaped = shape_bitmap_text("A♠✡☀", fonts);
    assert_eq!(
        shaped.glyphs(&shaped.clusters[1])[0].source,
        GlyphSource::Bitmap { font: 1, ch: '♠' }
    );
    // .fnt に無い ✡ はベクター face の実グリフで、基準線（大文字上端+cap高）に置く。
    let star = shaped.glyphs(&shaped.clusters[2])[0];
    assert!(matches!(star.source, GlyphSource::Vector { face: 0, .. }));
    assert_approx(star.y, primary.base as f32 - primary.ascent);
    // ✡ の送り幅は .fnt の設計サイズを1emとした値。
    assert_approx(shaped.clusters[2].advance, primary.size as f32);
    // どこにも無い ☀ は .fnt の候補が無いため生成矩形。
    assert!(matches!(shaped.glyphs(&shaped.clusters[3])[0].source, GlyphSource::Tofu { .. }));

    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    let mut style = text_style(Some("bitmap"));
    style.size = 0.05;
    builder.push_bitmap_text(&Point { x: 0.1, y: 0.1 }, "A♠✡☀", style, fonts, SURFACE);
    assert_eq!(builder.quads.len(), 4);
}

/// OS に ✡ を持つ記号フォントがある環境では、スキンフォントに無くても実グリフで描く。
#[test]
fn os_symbol_fonts_render_solomon_star_when_installed() {
    let system = load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[]);
    if !system.faces.iter().any(|face| face.has_glyph('✡')) {
        return;
    }
    let skin = [latin()];
    let shaped =
        shape_vector_text("Solomon ✡ 12", VectorFontSet { id: "skin", faces: system.after(&skin) });
    assert_eq!(tofu_glyphs(&shaped), 0);
    let star = shaped.clusters.iter().position(|cluster| cluster.text == (8..11)).unwrap();
    let face = glyph_face(&shaped, star).unwrap();
    assert!(face > 0, "the star must come from a system face");
}

/// 輪郭を持たないカラー画像専用フォント（sbix / CBDT）は輪郭描画の候補にせず、
/// カラーグリフとして描く。
#[test]
fn image_only_faces_are_not_used_for_outline_glyphs() {
    let sbix = fixture_face("bmz-test-sbix.ttf");
    assert_ne!(sbix.glyph_id('😀').0, 0);
    assert!(!sbix.has_glyph('😀'));
    let faces = [latin(), sbix];
    let shaped = shape_vector_text("😀", VectorFontSet { id: "chain", faces: faces_only(&faces) });
    assert_eq!(tofu_glyphs(&shaped), 0);
    assert_eq!(glyph_paint(&shaped, 0), (1, true));
}

fn color_font() -> TextFace {
    fixture_face("bmz-test-color.ttf")
}

fn cluster_texts<'a>(text: &'a str, shaped: &ShapedText) -> Vec<&'a str> {
    shaped.clusters.iter().map(|cluster| &text[cluster.text.clone()]).collect()
}

#[test]
fn emoji_sequences_shape_as_single_clusters_with_ligatures() {
    let faces = [latin(), color_font()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    let color = &faces[1];
    for (text, ligature) in [
        ("A👨\u{200D}👩\u{200D}👧\u{200D}👦B", "family"),
        ("A🇯🇵B", "flag_jp"),
        ("A👍🏻B", "thumbs_light"),
    ] {
        let shaped = shape_vector_text(text, fonts);
        assert_eq!(shaped.clusters.len(), 3, "{text}: {:?}", cluster_texts(text, &shaped));
        let glyphs = shaped.glyphs(&shaped.clusters[1]);
        assert_eq!(glyphs.len(), 1, "{text} should become the {ligature} ligature");
        let GlyphSource::Vector { face, glyph, .. } = glyphs[0].source else { panic!("vector") };
        assert_eq!(face, 1);
        assert!(
            !color.font().codepoint_ids().any(|(id, _)| id == glyph),
            "{ligature} is not mapped"
        );
        assert_approx(shaped.clusters[1].advance, fonts.em());
    }
}

#[test]
fn variation_selectors_and_zwj_are_never_separate_placeholders() {
    let faces = [latin(), color_font()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    for text in ["A✡\u{FE0E}B", "A✡\u{FE0F}B", "A❤\u{FE0F}B", "A\u{200D}B", "A\u{FE0F}B"] {
        let shaped = shape_vector_text(text, fonts);
        assert_eq!(tofu_glyphs(&shaped), 0, "{text}");
        let glyph_count: usize =
            shaped.clusters.iter().map(|cluster| shaped.glyphs(cluster).len()).sum();
        let visible = text.chars().filter(|ch| char_class(*ch) == CharClass::Visible).count();
        assert_eq!(glyph_count, visible, "{text}: {:?}", cluster_texts(text, &shaped));
    }
}

#[test]
fn unsupported_emoji_sequence_is_one_cluster_placeholder() {
    let face = latin();
    for text in ["A👨\u{200D}👩\u{200D}👧\u{200D}👦B", "A🇯🇵B", "A👍🏻B", "A1\u{FE0F}\u{20E3}B"]
    {
        let shaped = shape_vector_text(text, single_font_set("latin", &face));
        assert_eq!(shaped.clusters.len(), 3, "{text}");
        assert_eq!(tofu_glyphs(&shaped), 1, "{text} must not explode into per-codepoint boxes");
        assert_eq!(shaped.glyphs(&shaped.clusters[1]).len(), 1);
    }
}

#[test]
fn combining_marks_fall_back_to_the_base_character() {
    let face = latin();
    let shaped = shape_vector_text("A\u{0301}B", single_font_set("latin", &face));
    assert_eq!(cluster_texts("A\u{0301}B", &shaped), ["A\u{0301}", "B"]);
    assert_eq!(tofu_glyphs(&shaped), 0);
    let plain = shape_vector_text("A", single_font_set("latin", &face));
    assert_approx(shaped.clusters[0].advance, plain.clusters[0].advance);
}

#[test]
fn truncate_and_caret_keep_emoji_clusters_whole() {
    let faces = [latin(), color_font()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    let text = "A👨\u{200D}👩\u{200D}👧\u{200D}👦B";
    let shaped = shape_vector_text(text, fonts);
    let a_width = shaped.clusters[0].advance;
    // A + 家族の途中までの幅では家族全体を落とす。
    assert_eq!(truncated_cluster_count(&shaped.clusters, 1.0, a_width + fonts.em() * 0.5), 1);
    // cluster の途中を指す caret は cluster の前に置く。
    let inside = "A👨\u{200D}".len();
    assert_approx(prefix_width_px(&shaped.clusters, inside, 1.0), a_width);
    let after = text.len() - 1;
    assert_approx(prefix_width_px(&shaped.clusters, after, 1.0), a_width + fonts.em());
}

#[test]
fn bitmap_text_shapes_sequences_with_vector_faces_or_one_placeholder() {
    let primary = test_bitmap_font();
    let entries = bitmap_entries(&[&primary]);
    let text = "A🇯🇵A";
    let shaped = shape_bitmap_text(text, bitmap_set(&entries));
    assert_eq!(shaped.clusters.len(), 3);
    assert_eq!(tofu_glyphs(&shaped), 1);

    let faces = [color_font()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&faces) };
    let shaped = shape_bitmap_text(text, fonts);
    let glyphs = shaped.glyphs(&shaped.clusters[1]);
    assert_eq!(glyphs.len(), 1);
    assert!(matches!(glyphs[0].source, GlyphSource::Vector { face: 0, .. }));
    assert_approx(shaped.clusters[1].advance, primary.size as f32);
}

#[test]
fn mixed_script_and_emoji_text_never_loses_a_cluster() {
    let mut system = load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[]);
    if system.faces.is_empty() {
        return;
    }
    system.faces.push(color_font());
    let text = "日本語✡한글😀中文";
    let shaped = shape_vector_text(
        text,
        VectorFontSet { id: DEFAULT_TEXT_FONT_ID, faces: system.default_faces() },
    );
    assert_eq!(shaped.clusters.len(), text.chars().count());
    for cluster in &shaped.clusters {
        assert!(cluster.advance > 0.0);
        assert_eq!(shaped.glyphs(cluster).len(), 1, "{}", &text[cluster.text.clone()]);
    }
}

fn sbix_font() -> TextFace {
    fixture_face("bmz-test-sbix.ttf")
}

/// cluster の最初のグリフ（face, color）。
fn glyph_paint(shaped: &ShapedText, cluster: usize) -> (u16, bool) {
    match shaped.glyphs(&shaped.clusters[cluster])[0].source {
        GlyphSource::Vector { face, color, .. } => (face, color),
        other => panic!("vector glyph expected, got {other:?}"),
    }
}

/// RGBA の不透明画素を、色ごとに粗く量子化して数える。
fn opaque_colors(pixels: &[u8]) -> std::collections::BTreeSet<[u8; 3]> {
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] >= 250)
        .map(|pixel| [pixel[0] / 32, pixel[1] / 32, pixel[2] / 32])
        .collect()
}

#[test]
fn text_symbols_stay_monochrome_unless_emoji_presentation_is_requested() {
    let faces = [latin(), color_font()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    for (text, color) in [
        ("✡", false),
        ("✡\u{FE0E}", false),
        ("✡\u{FE0F}", true),
        ("❤", false),
        ("❤\u{FE0F}", true),
        ("😀", true),
        ("🎵", true),
    ] {
        let shaped = shape_vector_text(text, fonts);
        assert_eq!(shaped.clusters.len(), 1, "{text}");
        assert_eq!(glyph_paint(&shaped, 0), (1, color), "{text}");
    }
}

#[test]
fn emoji_presentation_falls_back_to_outline_and_text_to_color_only_glyphs() {
    // ♠ はカラーグリフを持たないため、VS16 でも記号 face の輪郭を使う。
    let faces = [latin(), symbols()];
    let shaped =
        shape_vector_text("♠\u{FE0F}", VectorFontSet { id: "a", faces: faces_only(&faces) });
    assert_eq!(glyph_paint(&shaped, 0), (1, false));
    // 輪郭を持たない sbix の 😀 は、VS15 でも欠字ではなくカラーで描く。
    let faces = [latin(), sbix_font()];
    let shaped =
        shape_vector_text("😀\u{FE0E}", VectorFontSet { id: "b", faces: faces_only(&faces) });
    assert_eq!(glyph_paint(&shaped, 0), (1, true));
}

#[test]
fn emoji_sequences_prefer_color_ligatures() {
    let faces = [latin(), color_font()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    for text in ["👨\u{200D}👩\u{200D}👧\u{200D}👦", "🇯🇵", "👍🏻"] {
        let shaped = shape_vector_text(text, fonts);
        assert_eq!(shaped.glyphs(&shaped.clusters[0]).len(), 1, "{text}");
        assert_eq!(glyph_paint(&shaped, 0), (1, true), "{text}");
    }
}

fn raster(face: &TextFace, ch: char, px: f32) -> ColorRaster {
    rasterize_color_glyph(face, face.glyph_id(ch), PxScale::from(px)).expect("color glyph")
}

#[test]
fn colr_v0_layers_keep_their_palette_colors() {
    let face = color_font();
    assert_eq!(color_glyph_kind(&face, face.glyph_id('😀')), Some(ColorGlyphKind::Colr));
    let raster = raster(&face, '😀', 64.0);
    let colors = opaque_colors(&raster.pixels);
    // 黄色の顔（1.0, 0.8, 0.0）と暗い目（0.1, 0.1, 0.1）。
    assert!(colors.contains(&[7, 6, 0]), "{colors:?}");
    assert!(colors.contains(&[0, 0, 0]), "{colors:?}");
    // 基準線より上に置かれ、em の範囲に収まる。
    assert!(raster.offset_y < 0.0);
    assert!(raster.width as f32 <= 64.0 && raster.height as f32 <= 64.0);
}

#[test]
fn colr_v1_linear_gradient_blends_between_stops() {
    let face = color_font();
    let raster = raster(&face, '🎵', 96.0);
    let colors = opaque_colors(&raster.pixels);
    // 赤（0.9, 0, 0.1）から青（0, 0.3, 1）へ変化する。
    assert!(colors.iter().any(|[r, _, b]| *r >= 6 && *b <= 1), "{colors:?}");
    assert!(colors.iter().any(|[r, _, b]| *r <= 1 && *b >= 6), "{colors:?}");
    assert!(colors.len() >= 4, "gradient should produce intermediate colors: {colors:?}");
}

#[test]
fn sbix_png_strike_is_scaled_and_placed() {
    let face = sbix_font();
    assert_eq!(color_glyph_kind(&face, face.glyph_id('😀')), Some(ColorGlyphKind::Bitmap));
    // 32ppem の strike を 2倍へ拡大する（em = PxScale × upem / height）。
    let em_px = 64.0;
    let metrics = face.metrics();
    let raster = raster(&face, '😀', em_px * metrics.height() / metrics.units_per_em);
    assert_eq!((raster.width, raster.height), (64, 64));
    let pixel = |x: u32, y: u32| {
        let index = ((y * raster.width + x) * 4) as usize;
        [
            raster.pixels[index],
            raster.pixels[index + 1],
            raster.pixels[index + 2],
            raster.pixels[index + 3],
        ]
    };
    assert_eq!(pixel(16, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(48, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(2, 2)[3], 0);
    // sbix は左下原点。glyf の yMin = 0 なので画像の下端が基準線。
    assert_approx(raster.offset_y, -64.0);
}

/// OS の CBDT カラー絵文字（Linux の Noto Color Emoji）が入っていれば実際の色で描く。
#[test]
fn os_cbdt_color_emoji_renders_multiple_colors_when_installed() {
    let Some(symbol) =
        bmz_font::symbol_font_fallbacks().iter().find(|font| font.family() == "Noto Color Emoji")
    else {
        return;
    };
    let faces = load_symbol_faces(std::slice::from_ref(symbol));
    let face = &faces[0];
    if color_glyph_kind(face, face.glyph_id('😀')) != Some(ColorGlyphKind::Bitmap) {
        return;
    }
    let raster = raster(face, '😀', 48.0);
    assert!(opaque_colors(&raster.pixels).len() >= 3);
}

#[test]
fn color_glyphs_keep_font_colors_with_skin_alpha_and_silhouette_decorations() {
    let faces = [latin(), color_font()];
    let fonts = VectorFontSet { id: "chain", faces: faces_only(&faces) };
    let mut style = text_style(None);
    style.color = Color::rgba(1.0, 0.0, 0.0, 0.5);
    style.shadow = Some(crate::plan::TextShadow {
        color: Color::rgba(0.0, 0.0, 1.0, 0.25),
        offset: Point { x: 0.01, y: 0.01 },
    });
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    builder.push_text(&Point { x: 0.1, y: 0.1 }, "A😀", style, fonts, SURFACE);
    let quads = builder.quads.clone();
    assert_eq!(quads.len(), 4);
    // 影は文字色ではなく影色。カラーグリフの影もシルエット。
    for quad in &quads[..2] {
        assert_eq!(quad.color, Color::rgba(0.0, 0.0, 1.0, 0.25));
    }
    assert_eq!(quads[2].color, Color::rgba(1.0, 0.0, 0.0, 0.5));
    assert_eq!(quads[3].color, Color::rgba(1.0, 1.0, 1.0, 0.5));
    let kinds: Vec<_> = atlas.glyphs.keys().map(|key| key.kind).collect();
    assert!(kinds.contains(&TextGlyphKind::Color));
    assert!(kinds.contains(&TextGlyphKind::ColorSilhouette));
    let silhouette =
        atlas.glyphs.iter().find(|(key, _)| key.kind == TextGlyphKind::ColorSilhouette);
    let (_, silhouette) = silhouette.unwrap();
    let pixels = atlas.pixels_for_size(atlas.size());
    let row = (silhouette.atlas_origin.1 + silhouette.height / 2) * TEXT_ATLAS_WIDTH;
    let index = ((row + silhouette.atlas_origin.0 + silhouette.width / 2) * 4) as usize;
    assert_eq!(&pixels[index..index + 4], &[255, 255, 255, 255]);
}

#[test]
fn color_glyphs_are_rasterized_once_and_reused() {
    let face = color_font();
    let mut fonts = HashMap::new();
    fonts.insert("skin:color".to_string(), face.clone());
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![
            text_command("😀😀🎵", text_style(Some("skin:color"))),
            text_command("A😀", text_style(Some("skin:color"))),
        ],
    };
    let system = FontFallbackChain::single(latin());
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let first = build_text_frame_with_fallback_cache(
        &plan,
        &system,
        &fonts,
        &HashMap::new(),
        SURFACE,
        &mut atlas,
    );
    assert_eq!(first.command_quad_counts, vec![3, 2]);
    let color_keys = atlas.glyphs.keys().filter(|key| key.kind == TextGlyphKind::Color).count();
    assert_eq!(color_keys, 2);
    let second = build_text_frame_with_fallback_cache(
        &plan,
        &system,
        &fonts,
        &HashMap::new(),
        SURFACE,
        &mut atlas,
    );
    assert!(second.dirty_regions.is_empty(), "cached color glyphs must not be uploaded again");
}
