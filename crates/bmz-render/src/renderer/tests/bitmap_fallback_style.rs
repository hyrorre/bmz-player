//! 画像フォント文中の欠字を、.fnt から推定した塗り・縁取りで描くテスト（BMZ 拡張）。

use super::unicode_text::*;
use super::*;
use crate::bitmap_font::{BitmapFallbackOutline, BitmapFallbackStyle};

const OUTLINE: [u8; 3] = [20, 30, 40];

/// 行上端の白から下端の灰へ変わる塗りと、1px の縁取り。
fn styled_bitmap_font() -> BitmapFont {
    let mut font = test_bitmap_font();
    font.fallback_style = Some(Arc::new(BitmapFallbackStyle {
        fill_top: 0,
        fill_rows: (0..10)
            .map(|row| {
                let level = 250 - row * 10;
                [level, level, level]
            })
            .collect(),
        outline: Some(BitmapFallbackOutline { width: 1.0, color: OUTLINE }),
    }));
    font
}

struct StyledText {
    quads: Vec<TextQuad>,
    atlas: TextAtlasCache,
}

impl StyledText {
    fn glyph(&self, kind: TextGlyphKind) -> Option<CachedGlyph> {
        self.atlas.glyphs.iter().find(|(key, _)| key.kind == kind).map(|(_, glyph)| glyph.clone())
    }

    fn pixels(&self, glyph: &CachedGlyph) -> Vec<[u8; 4]> {
        let atlas = self.atlas.pixels_for_size(self.atlas.size());
        let width = self.atlas.size().width;
        let mut pixels = Vec::new();
        for y in 0..glyph.height {
            for x in 0..glyph.width {
                let index =
                    (((glyph.atlas_origin.1 + y) * width + glyph.atlas_origin.0 + x) * 4) as usize;
                pixels.push([atlas[index], atlas[index + 1], atlas[index + 2], atlas[index + 3]]);
            }
        }
        pixels
    }
}

fn styled_text(text: &str, font: &BitmapFont, faces: &[TextFace], color: Color) -> StyledText {
    let entries = bitmap_entries(&[font]);
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(faces) };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(&mut atlas);
    let mut style = text_style(Some("bitmap"));
    // .fnt の1pxが表示4px。
    style.size = 0.2;
    style.color = color;
    builder.push_bitmap_text(&Point { x: 0.1, y: 0.1 }, text, style, fonts, SURFACE);
    let quads = builder.quads;
    StyledText { quads, atlas }
}

#[test]
fn missing_bitmap_glyphs_use_the_estimated_fill_and_outline() {
    let font = styled_bitmap_font();
    let color = Color::rgba(1.0, 0.5, 0.25, 0.75);
    let text = styled_text("A✡", &font, &[symbols()], color);
    assert_eq!(text.quads.len(), 2);
    // 画像フォントのグリフと同じく文字色を掛ける。
    assert_eq!(text.quads[1].color, color);
    assert!(text.glyph(TextGlyphKind::Vector).is_none());
    let glyph = text.glyph(TextGlyphKind::StyledFallback).expect("styled star");
    let pixels = text.pixels(&glyph);

    // 外側の不透明な画素は縁取り色。
    let row = glyph.height as usize / 2;
    let row_pixels = &pixels[row * glyph.width as usize..(row + 1) * glyph.width as usize];
    let first_opaque = row_pixels.iter().find(|pixel| pixel[3] == 255).expect("opaque row");
    assert_eq!(&first_opaque[..3], &OUTLINE);

    // 塗りの色は行の位置で決まる。基準線は行上端から `base` px。
    // 縁取りの無い同じグリフで完全に覆われる画素を、余白の分ずらして調べる。
    let plain = styled_text("A✡", &test_bitmap_font(), &[symbols()], color);
    let plain_glyph = plain.glyph(TextGlyphKind::Vector).unwrap();
    let pixels_per_px = glyph.width as f32 / glyph.display_width;
    let margin = ((glyph.width - plain_glyph.width) / 2) as usize;
    let scale = 0.2 * SURFACE.height as f32 / font.size as f32;
    let style = font.fallback_style.as_deref().unwrap();
    let mut checked = 0;
    for (index, plain_pixel) in plain.pixels(&plain_glyph).iter().enumerate() {
        if plain_pixel[3] != 255 {
            continue;
        }
        let y = index / plain_glyph.width as usize + margin;
        let x = index % plain_glyph.width as usize + margin;
        let pixel = pixels[y * glyph.width as usize + x];
        assert_eq!(pixel[3], 255);
        let display_y = glyph.offset_y + (y as f32 + 0.5) / pixels_per_px;
        let expected = style.fill_at(font.base as f32 + display_y / scale)[0] * 255.0;
        assert!(
            (pixel[0] as f32 - expected).abs() <= 1.0,
            "row {y}: expected {expected}, got {}",
            pixel[0]
        );
        checked += 1;
    }
    assert!(checked > 20, "the star must have filled pixels");
}

#[test]
fn outline_widens_the_glyph_by_the_estimated_width() {
    let font = styled_bitmap_font();
    let styled = styled_text("✡", &font, &[symbols()], Color::rgb(1.0, 1.0, 1.0));
    let plain = styled_text("✡", &test_bitmap_font(), &[symbols()], Color::rgb(1.0, 1.0, 1.0));
    let styled = styled.glyph(TextGlyphKind::StyledFallback).unwrap();
    let plain = plain.glyph(TextGlyphKind::Vector).unwrap();
    // 1px の縁取りが表示4px。外側の中間調の1pxを含め、左右に (1 + 1) * 4 px 広げる。
    assert_approx(styled.display_width - plain.display_width, 16.0);
    assert_approx(styled.display_height - plain.display_height, 16.0);
    assert_approx(styled.offset_x, plain.offset_x - 8.0);
    assert_approx(styled.offset_y, plain.offset_y - 8.0);
}

#[test]
fn bitmap_fonts_without_a_style_keep_plain_vector_fallbacks() {
    let text = styled_text("A✡", &test_bitmap_font(), &[symbols()], Color::rgb(1.0, 1.0, 1.0));
    assert!(text.glyph(TextGlyphKind::Vector).is_some());
    assert!(text.glyph(TextGlyphKind::StyledFallback).is_none());
}

#[test]
fn color_glyphs_in_bitmap_text_keep_their_font_colors() {
    let text =
        styled_text("A😀", &styled_bitmap_font(), &[color_font()], Color::rgb(1.0, 1.0, 1.0));
    assert!(text.glyph(TextGlyphKind::Color).is_some());
    assert!(text.glyph(TextGlyphKind::StyledFallback).is_none());
}

#[test]
fn styled_fallback_glyphs_are_rasterized_once() {
    let font = styled_bitmap_font();
    let entries = bitmap_entries(&[&font]);
    let faces = [symbols()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&faces) };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    for _ in 0..2 {
        atlas.begin_frame();
        let mut builder = CachedTextFrameBuilder::new(&mut atlas);
        let style = text_style(Some("bitmap"));
        builder.push_bitmap_text(&Point { x: 0.1, y: 0.1 }, "✡A✡", style, fonts, SURFACE);
    }
    let styled = atlas.glyphs.keys().filter(|key| key.kind == TextGlyphKind::StyledFallback);
    assert_eq!(styled.count(), 1);
}
