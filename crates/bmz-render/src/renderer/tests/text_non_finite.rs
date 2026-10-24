//! 非有限の位置を持つ文字を GPU のインスタンスバッファへ渡さないことの回帰テスト。

use super::unicode_text::*;
use super::*;

#[derive(Clone, Copy, Debug)]
enum Font {
    Vector,
    Bitmap,
}

const NORMAL: Point = Point { x: 0.1, y: 0.2 };
const UNIT: Point = Point { x: 1.0, y: 1.0 };
const CARET: TextCaret =
    TextCaret { byte_index: 1, color: Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } };

fn style(font: Font, decorated: bool) -> TextStyle {
    let mut style = text_style(match font {
        Font::Vector => None,
        Font::Bitmap => Some("bitmap"),
    });
    if decorated {
        style.outline =
            Some(crate::plan::TextOutline { color: Color::rgb(0.0, 0.0, 0.0), width: 0.01 });
        style.shadow = Some(crate::plan::TextShadow {
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
            offset: Point { x: 0.01, y: 0.01 },
        });
    }
    style
}

fn command(origin: Point, post_scale: Point, style: TextStyle) -> DrawCommand {
    DrawCommand::Text { origin, text: "AA".to_string(), style, caret: Some(CARET), post_scale }
}

fn render(atlas: &mut TextAtlasCache, commands: Vec<DrawCommand>) -> TextFrame {
    let plan = DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
    let bitmap_fonts = HashMap::from([("bitmap".to_string(), test_bitmap_font())]);
    build_text_frame_with_cache(&plan, &latin(), &HashMap::new(), &bitmap_fonts, SURFACE, atlas)
}

fn fresh(commands: Vec<DrawCommand>) -> TextFrame {
    render(&mut TextAtlasCache::new(TEXT_ATLAS_WIDTH), commands)
}

/// インスタンスの位置・寸法がすべて有限で、コマンドごとの quad 数と合うこと。
fn assert_valid(frame: &TextFrame) {
    let quads: usize = frame.command_quad_counts.iter().sum();
    assert_eq!(frame.instances.len(), quads * TEXT_INSTANCE_BYTES);
    assert_eq!(frame.command_quad_counts.len(), frame.command_caret_rects.len());
    for instance in frame.instances.as_chunks::<TEXT_INSTANCE_BYTES>().0 {
        for value in instance[..16].as_chunks::<4>().0 {
            let value = f32::from_le_bytes(*value);
            assert!(value.is_finite(), "{value}");
        }
    }
    for caret in frame.command_caret_rects.iter().flatten() {
        let rect = caret.rect;
        assert!([rect.x, rect.y, rect.width, rect.height].iter().all(|value| value.is_finite()));
    }
}

fn non_finite_positions() -> [(Point, Point); 8] {
    [
        (Point { x: f32::NAN, y: 0.2 }, UNIT),
        (Point { x: 0.1, y: f32::NAN }, UNIT),
        (Point { x: f32::INFINITY, y: 0.2 }, UNIT),
        (Point { x: f32::NEG_INFINITY, y: 0.2 }, UNIT),
        (Point { x: 0.1, y: f32::INFINITY }, UNIT),
        (Point { x: 0.1, y: f32::NEG_INFINITY }, UNIT),
        (Point { x: f32::INFINITY, y: f32::NEG_INFINITY }, UNIT),
        (NORMAL, Point { x: f32::NAN, y: 1.0 }),
    ]
}

fn cases() -> impl Iterator<Item = (Font, bool)> {
    [(Font::Vector, false), (Font::Vector, true), (Font::Bitmap, false), (Font::Bitmap, true)]
        .into_iter()
}

#[test]
fn non_finite_text_is_skipped_without_shifting_the_next_command() {
    for (font, decorated) in cases() {
        let expected = fresh(vec![command(NORMAL, UNIT, style(font, decorated))]);
        assert!(expected.command_quad_counts[0] > 0);
        assert!(expected.command_caret_rects[0].is_some());
        for (origin, post_scale) in non_finite_positions() {
            let frame = fresh(vec![
                command(origin, post_scale, style(font, decorated)),
                command(NORMAL, UNIT, style(font, decorated)),
            ]);
            assert_valid(&frame);
            let case = format!("{font:?} decorated={decorated} {origin:?} {post_scale:?}");
            assert_eq!(
                frame.command_quad_counts,
                vec![0, expected.command_quad_counts[0]],
                "{case}"
            );
            assert_eq!(frame.command_caret_rects[0], None, "{case}");
            assert_eq!(frame.command_caret_rects[1], expected.command_caret_rects[0], "{case}");
            assert_eq!(frame.instances, expected.instances, "{case}");
        }
    }
}

#[test]
fn non_finite_decoration_drops_only_the_broken_quads() {
    for font in [Font::Vector, Font::Bitmap] {
        let plain = fresh(vec![command(NORMAL, UNIT, style(font, false))]);
        let mut outline = style(font, true);
        outline.outline.as_mut().unwrap().width = f32::NAN;
        let mut shadow = style(font, true);
        shadow.shadow.as_mut().unwrap().offset = Point { x: f32::INFINITY, y: 0.01 };
        for broken in [outline, shadow] {
            let frame = fresh(vec![
                command(NORMAL, UNIT, broken.clone()),
                command(NORMAL, UNIT, style(font, false)),
            ]);
            assert_valid(&frame);
            // 本体の文字は残し、壊れた縁取りや影だけを描かない。
            assert!(frame.command_quad_counts[0] >= plain.command_quad_counts[0], "{font:?}");
            assert_eq!(frame.command_quad_counts[1], plain.command_quad_counts[0], "{font:?}");
            assert!(frame.command_caret_rects[0].is_some(), "{font:?}");
        }
    }
}

#[test]
fn text_recovers_after_a_non_finite_frame() {
    for (font, decorated) in cases() {
        for (origin, post_scale) in non_finite_positions() {
            let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
            let first = render(&mut atlas, vec![command(NORMAL, UNIT, style(font, decorated))]);
            let layouts = atlas.layouts.len();
            let bad = render(&mut atlas, vec![command(origin, post_scale, style(font, decorated))]);
            assert_valid(&bad);
            assert_eq!(bad.command_quad_counts, vec![0]);
            assert!(bad.instances.is_empty());
            assert_eq!(atlas.layouts.len(), layouts);
            let back = render(&mut atlas, vec![command(NORMAL, UNIT, style(font, decorated))]);
            assert_valid(&back);
            assert_eq!(back.command_quad_counts, first.command_quad_counts);
            assert_eq!(back.command_caret_rects, first.command_caret_rects);
            assert_eq!(back.instances, first.instances);
        }
    }
}

#[test]
fn far_finite_origin_is_still_drawn() {
    // キャッシュの対象外になる遠い位置でも、有限なら変更前と同じく描く。
    for (font, decorated) in cases() {
        let near = fresh(vec![command(NORMAL, UNIT, style(font, decorated))]);
        for far in [
            Point { x: 20.0, y: 0.2 },
            Point { x: -20.0, y: -20.0 },
            Point { x: 1.0e8, y: 0.2 },
            Point { x: 0.1, y: -1.0e8 },
        ] {
            let frame = fresh(vec![command(far, UNIT, style(font, decorated))]);
            assert_valid(&frame);
            assert_eq!(frame.command_quad_counts, near.command_quad_counts, "{far:?}");
            assert!(frame.command_caret_rects[0].is_some(), "{far:?}");
        }
    }
}
