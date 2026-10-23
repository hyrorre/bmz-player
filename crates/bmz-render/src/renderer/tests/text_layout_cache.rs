//! 位置に依存しないテキストレイアウトのキャッシュの回帰テスト。

use super::unicode_text::*;
use super::*;

#[derive(Clone, Copy)]
enum Font {
    Vector,
    Bitmap,
}

/// 1フレームで `origin` に "AA" を組んだ quad。`decorated` では縁取りと影を付ける。
fn draw(atlas: &mut TextAtlasCache, font: Font, origin: Point, decorated: bool) -> Vec<TextQuad> {
    let mut style = text_style(None);
    if decorated {
        style.outline =
            Some(crate::plan::TextOutline { color: Color::rgb(0.0, 0.0, 0.0), width: 0.01 });
        style.shadow = Some(crate::plan::TextShadow {
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
            offset: Point { x: 0.01, y: 0.01 },
        });
    }
    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(atlas);
    match font {
        Font::Vector => {
            let face = latin();
            builder.push_text(&origin, "AA", style, single_font_set("skin:cache", &face), SURFACE);
        }
        Font::Bitmap => {
            let bitmap = test_bitmap_font();
            let entries = bitmap_entries(&[&bitmap]);
            builder.push_bitmap_text(&origin, "AA", style, bitmap_set(&entries), SURFACE);
        }
    }
    builder.quads.clone()
}

/// キャッシュの無い atlas で組んだ quad。
fn fresh(font: Font, origin: Point, decorated: bool) -> Vec<TextQuad> {
    draw(&mut TextAtlasCache::new(TEXT_ATLAS_WIDTH), font, origin, decorated)
}

fn assert_same_layout(actual: &[TextQuad], expected: &[TextQuad]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(actual.is_finite(), "{actual:?}");
        assert!((actual.x - expected.x).abs() < 1e-6, "{actual:?} / {expected:?}");
        assert!((actual.y - expected.y).abs() < 1e-6, "{actual:?} / {expected:?}");
        assert!((actual.width - expected.width).abs() < 1e-6, "{actual:?} / {expected:?}");
        assert!((actual.height - expected.height).abs() < 1e-6, "{actual:?} / {expected:?}");
        assert_eq!(actual.color, expected.color);
    }
}

const NORMAL: Point = Point { x: 0.1, y: 0.2 };

fn non_finite_origins() -> [Point; 6] {
    [
        Point { x: f32::NAN, y: 0.2 },
        Point { x: 0.1, y: f32::NAN },
        Point { x: f32::INFINITY, y: 0.2 },
        Point { x: f32::NEG_INFINITY, y: 0.2 },
        Point { x: 0.1, y: f32::INFINITY },
        Point { x: 0.1, y: f32::NEG_INFINITY },
    ]
}

fn cases() -> impl Iterator<Item = (Font, bool)> {
    [(Font::Vector, false), (Font::Vector, true), (Font::Bitmap, false), (Font::Bitmap, true)]
        .into_iter()
}

#[test]
fn non_finite_origin_between_normal_frames_does_not_poison_the_layout() {
    for (font, decorated) in cases() {
        for bad in non_finite_origins() {
            let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
            let first = draw(&mut atlas, font, NORMAL, decorated);
            let layouts = atlas.layouts.len();
            assert!(layouts > 0);
            let _ = draw(&mut atlas, font, bad, decorated);
            assert_eq!(atlas.layouts.len(), layouts, "{bad:?} added a layout");
            let back = draw(&mut atlas, font, NORMAL, decorated);
            assert_same_layout(&back, &first);
        }
    }
}

#[test]
fn layout_built_at_a_non_finite_origin_is_not_reused_at_a_normal_origin() {
    for (font, decorated) in cases() {
        for bad in non_finite_origins() {
            let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
            let drawn = draw(&mut atlas, font, bad, decorated);
            // 非有限の位置の描画は変更前と同じく毎回組み、キャッシュには何も残さない。
            assert_eq!(drawn.len(), fresh(font, bad, decorated).len());
            assert_eq!(atlas.layouts.len(), 0, "{bad:?}");
            let normal = draw(&mut atlas, font, NORMAL, decorated);
            assert_same_layout(&normal, &fresh(font, NORMAL, decorated));
            // 正常な位置ではキャッシュし、次のフレームで使い回す。
            assert!(atlas.layouts.len() > 0);
            let again = draw(&mut atlas, font, NORMAL, decorated);
            assert_same_layout(&again, &normal);
        }
    }
}

#[test]
fn moving_text_still_shares_one_layout() {
    for (font, decorated) in cases() {
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let _ = draw(&mut atlas, font, NORMAL, decorated);
        let layouts = atlas.layouts.len();
        for step in 1..20 {
            let origin = Point { x: 0.1 + step as f32 * 0.013, y: 0.2 - step as f32 * 0.007 };
            let moved = draw(&mut atlas, font, origin, decorated);
            assert_eq!(atlas.layouts.len(), layouts, "step {step} built a new layout");
            assert_same_layout(&moved, &fresh(font, origin, decorated));
        }
    }
}

#[test]
fn far_away_origin_is_drawn_but_not_cached() {
    // f32 の精度が落ちる遠い位置では、相対座標がずれるのでキャッシュしない。
    let far = Point { x: 1.0e8, y: 0.2 };
    for (font, decorated) in cases() {
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let _ = draw(&mut atlas, font, far, decorated);
        assert_eq!(atlas.layouts.len(), 0);
        let normal = draw(&mut atlas, font, NORMAL, decorated);
        assert_same_layout(&normal, &fresh(font, NORMAL, decorated));
    }
}
