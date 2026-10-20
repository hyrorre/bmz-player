//! 輪郭・カラーグリフの描画可否と、描けない場合の fallback の回帰テスト。

use super::unicode_text::*;
use super::*;

fn empty_base_font() -> TextFace {
    fixture_face("bmz-test-colr-empty-base.ttf")
}

fn chain(faces: &[TextFace]) -> VectorFontSet<'_> {
    VectorFontSet { id: "chain", faces: faces_only(faces) }
}

/// 1行を組んだ quad。
fn chain_quads(text: &str, faces: &[TextFace]) -> Vec<TextQuad> {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    builder.push_text(&Point { x: 0.1, y: 0.1 }, text, text_style(None), chain(faces), SURFACE);
    builder.quads
}

/// 解決結果のグリフがすべて quad になり、どの quad も不透明な画素を持つこと。
fn assert_every_glyph_is_visible(text: &str, faces: &[TextFace]) {
    let shaped = shape_vector_text(text, chain(faces));
    let glyphs: usize = shaped.clusters.iter().map(|cluster| shaped.glyphs(cluster).len()).sum();
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    builder.push_text(&Point { x: 0.1, y: 0.1 }, text, text_style(None), chain(faces), SURFACE);
    let quads = builder.quads.clone();
    assert_eq!(quads.len(), glyphs, "{text}: every resolved glyph must produce a quad");
    let pixels = atlas.pixels_for_size(atlas.size());
    for quad in &quads {
        let (x0, y0) = quad.atlas_origin;
        let opaque = (y0..y0 + quad.glyph_height).any(|y| {
            (x0..x0 + quad.glyph_width)
                .any(|x| pixels[((y * TEXT_ATLAS_WIDTH + x) * 4 + 3) as usize] > 0)
        });
        assert!(opaque, "{text}: a drawn glyph must not be fully transparent");
    }
}

#[test]
fn colr_base_glyph_without_outline_is_drawn_in_color() {
    let faces = [latin(), empty_base_font()];
    let face = &faces[1];
    // glyf 表はあるが、✡ の基底グリフは空でカラーレイヤーだけを持つ。
    assert_ne!(face.glyph_id('✡').0, 0);
    assert!(face.font().outline(face.glyph_id('✡')).is_none());
    for text in ["✡", "✡\u{FE0E}", "✡\u{FE0F}", "❤", "❤\u{FE0F}", "😀"] {
        let shaped = shape_vector_text(text, chain(&faces));
        assert_eq!(glyph_paint(&shaped, 0), (1, true), "{text}");
        assert_eq!(chain_quads(text, &faces).len(), 1, "{text} must not vanish");
    }
}

#[test]
fn text_presentation_prefers_a_later_outline_over_an_earlier_color_only_glyph() {
    let faces = [latin(), empty_base_font(), symbols()];
    for (text, expected) in [("✡", (2, false)), ("✡\u{FE0E}", (2, false)), ("✡\u{FE0F}", (1, true))]
    {
        let shaped = shape_vector_text(text, chain(&faces));
        assert_eq!(glyph_paint(&shaped, 0), expected, "{text}");
    }
}

#[test]
fn glyph_with_neither_outline_nor_color_falls_through_to_placeholder() {
    let faces = [latin(), empty_base_font()];
    assert_ne!(faces[1].glyph_id('★').0, 0);
    let shaped = shape_vector_text("A★B", chain(&faces));
    assert_eq!(shaped.clusters.len(), 3);
    assert_eq!(tofu_glyphs(&shaped), 1);
    assert_eq!(chain_quads("A★B", &faces).len(), 3);
}

#[test]
fn outline_and_color_glyphs_switch_with_presentation() {
    let faces = [latin(), color_font()];
    for (text, color) in
        [("✡", false), ("✡\u{FE0E}", false), ("✡\u{FE0F}", true), ("❤", false), ("❤\u{FE0F}", true)]
    {
        let shaped = shape_vector_text(text, chain(&faces));
        assert_eq!(glyph_paint(&shaped, 0), (1, color), "{text}");
        assert_every_glyph_is_visible(text, &faces);
    }
}

#[test]
fn blank_glyphs_keep_their_width_without_placeholders() {
    // 空白は輪郭を持たないが、欠字ではなく送り幅だけを持つ。
    for faces in [vec![latin()], vec![latin(), empty_base_font()], vec![empty_base_font(), latin()]]
    {
        let fonts = chain(&faces);
        let shaped = shape_vector_text("A B", fonts);
        assert_eq!(tofu_glyphs(&shaped), 0);
        let space = &shaped.clusters[1];
        assert!(shaped.glyphs(space).is_empty());
        let face = fonts.primary().unwrap();
        assert_approx(
            space.advance,
            face.h_advance_unscaled(face.glyph_id(' ')) * fonts.units_scale(0),
        );
    }
    // 点字の空白（U+2800）も意図的に描画画素を持たない文字として扱う。
    let faces = [latin()];
    let shaped = shape_vector_text("A\u{2800}B", chain(&faces));
    assert_eq!(tofu_glyphs(&shaped), 0);
    assert!(shaped.clusters[1].advance > 0.0);
}

#[test]
fn resolved_glyphs_are_never_transparent() {
    let sets = [
        vec![latin(), color_font()],
        vec![latin(), empty_base_font()],
        vec![latin(), empty_base_font(), symbols()],
        vec![latin(), sbix_font()],
        vec![latin()],
    ];
    for faces in &sets {
        for text in ["A✡B", "A✡\u{FE0E}B", "A✡\u{FE0F}B", "A❤B", "A❤\u{FE0F}B", "A😀B", "A★B"]
        {
            assert_every_glyph_is_visible(text, faces);
        }
    }
}

#[test]
fn glyph_capabilities_are_cached_per_face() {
    let face = empty_base_font();
    let glyph = face.glyph_id('✡');
    assert!(!face.paints(glyph, GlyphPaint::Outline));
    assert!(face.paints(glyph, GlyphPaint::Color));
    let cached = face.cached_capabilities();
    assert!(face.paints(glyph, GlyphPaint::Color));
    assert_eq!(face.cached_capabilities(), cached, "a second query must hit the cache");
    // 同じ face の複製（Arc 共有）でも結果を共有する。
    assert_eq!(face.clone().cached_capabilities(), cached);
}

#[test]
fn failed_color_rasterization_falls_back_to_outline_then_placeholder() {
    let faces = [latin(), empty_base_font()];
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    let fonts = chain(&faces);
    // カラーグリフを持たない 'A' と、輪郭もカラーも持たない ★ をカラーとして描かせる。
    let a = faces[0].glyph_id('A');
    let star = faces[1].glyph_id('★');
    let (_, mut glyph_a) = fonts.vector_glyph(0, &faces[0], a, true);
    let (_, mut glyph_star) = fonts.vector_glyph(1, &faces[1], star, true);
    glyph_a.x = 0.0;
    glyph_star.x = 1.0;
    builder.push_resolved_glyphs(&[glyph_a, glyph_star], fonts, SURFACE);
    let quads = builder.quads.clone();
    assert_eq!(quads.len(), 2, "both glyphs must stay visible");
    assert_eq!(quads[0].color, Color::rgb(1.0, 1.0, 1.0));
    let kinds: Vec<_> = atlas.glyphs.keys().map(|key| key.kind).collect();
    assert!(kinds.contains(&TextGlyphKind::Vector), "{kinds:?}");
    assert!(kinds.contains(&TextGlyphKind::Tofu), "{kinds:?}");
}
