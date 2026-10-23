//! 文字本体のグラデーション（BMZ 拡張 `colorTop` / `colorBottom` / `gradientAngle`）。

use super::unicode_text::*;
use super::*;

const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
const BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 0.5 };
const ORIGIN: Point = Point { x: 0.1, y: 0.2 };
const UNIT: Point = Point { x: 1.0, y: 1.0 };

/// 1つのインスタンスの値。位置と寸法、始点の色、終点の色、t0 / du / dv。
#[derive(Debug, Clone, Copy)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    color_end: [f32; 4],
    gradient: [f32; 3],
}

fn instances(frame: &TextFrame) -> Vec<Instance> {
    frame
        .instances
        .chunks_exact(TEXT_INSTANCE_BYTES)
        .map(|bytes| {
            let value = |index: usize| {
                f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
            };
            Instance {
                rect: [value(0), value(1), value(2), value(3)],
                color: [value(8), value(9), value(10), value(11)],
                color_end: [value(12), value(13), value(14), value(15)],
                gradient: [value(16), value(17), value(18)],
            }
        })
        .collect()
}

fn rgba(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

fn style(gradient: Option<TextGradient>, decorated: bool) -> TextStyle {
    let mut style = text_style(None);
    style.color = RED;
    style.gradient = gradient;
    if decorated {
        style.outline =
            Some(crate::plan::TextOutline { color: Color::rgb(0.0, 1.0, 0.0), width: 0.01 });
        style.shadow = Some(crate::plan::TextShadow {
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
            offset: Point { x: 0.01, y: 0.01 },
        });
    }
    style
}

fn command(origin: Point, post_scale: Point, style: TextStyle) -> DrawCommand {
    DrawCommand::Text { origin, text: "AWAW".to_string(), style, caret: None, post_scale }
}

fn render(atlas: &mut TextAtlasCache, commands: Vec<DrawCommand>) -> TextFrame {
    let plan = DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
    let system = FontFallbackChain { faces: vec![latin(), color_font()], default_len: 2 };
    let font_sets = SkinFontSets::new(&HashMap::new(), &HashMap::new(), &HashMap::new());
    build_text_frame_with_font_sets(&plan, &system, &font_sets, SURFACE, atlas)
}

fn fresh(commands: Vec<DrawCommand>) -> TextFrame {
    render(&mut TextAtlasCache::new(TEXT_ATLAS_WIDTH), commands)
}

fn vertical() -> Option<TextGradient> {
    Some(TextGradient { color: BLUE, angle_deg: 0.0 })
}

fn assert_plain(instance: &Instance) {
    assert_eq!(instance.color_end, instance.color, "{instance:?}");
    assert_eq!(instance.gradient, [0.0; 3], "{instance:?}");
}

fn approx(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-4
}

#[test]
fn text_without_gradient_keeps_one_color_per_quad() {
    for decorated in [false, true] {
        let frame = fresh(vec![command(ORIGIN, UNIT, style(None, decorated))]);
        assert!(!frame.instances.is_empty());
        instances(&frame).iter().for_each(assert_plain);
    }
}

#[test]
fn vertical_gradient_runs_from_the_text_top_to_its_height() {
    let size = text_style(None).size;
    let frame = fresh(vec![command(ORIGIN, UNIT, style(vertical(), false))]);
    let quads = instances(&frame);
    assert!(quads.len() >= 4);
    for quad in &quads {
        let [_, y, _, height] = quad.rect;
        assert_eq!(quad.color, rgba(RED));
        assert_eq!(quad.color_end, rgba(BLUE));
        let [t0, du, dv] = quad.gradient;
        assert!(approx(t0, (y - ORIGIN.y) / size), "{quad:?}");
        assert!(approx(du, 0.0), "{quad:?}");
        assert!(approx(dv, height / size), "{quad:?}");
    }
}

#[test]
fn angled_gradient_spans_the_text_width_in_screen_pixels() {
    let frame = fresh(vec![command(
        ORIGIN,
        UNIT,
        style(Some(TextGradient { color: BLUE, angle_deg: 90.0 }), false),
    )]);
    let quads = instances(&frame);
    let left = quads.iter().map(|quad| quad.rect[0]).fold(f32::INFINITY, f32::min);
    let right =
        quads.iter().map(|quad| quad.rect[0] + quad.rect[2]).fold(f32::NEG_INFINITY, f32::max);
    for quad in &quads {
        let [x, _, width, _] = quad.rect;
        let [t0, du, dv] = quad.gradient;
        // 90 度で左から右。縦には変わらない。
        assert!(approx(t0, (x - left) / (right - left)), "{quad:?}");
        assert!(approx(du, width / (right - left)), "{quad:?}");
        assert!(dv.abs() < 1e-6, "{quad:?}");
    }
    let start = quads.iter().map(|quad| quad.gradient[0]).fold(f32::INFINITY, f32::min);
    let end = quads
        .iter()
        .map(|quad| quad.gradient[0] + quad.gradient[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(approx(start, 0.0) && approx(end, 1.0));

    // 45 度では、画面の画素で測った対角方向になる（正規化座標の縦横比に依存しない）。
    let frame = fresh(vec![command(
        ORIGIN,
        UNIT,
        style(Some(TextGradient { color: BLUE, angle_deg: 45.0 }), false),
    )]);
    for quad in instances(&frame) {
        let [_, _, width, height] = quad.rect;
        let [_, du, dv] = quad.gradient;
        let ratio = (du / (width * SURFACE.width as f32)) / (dv / (height * SURFACE.height as f32));
        assert!(approx(ratio, 1.0), "{quad:?}");
    }
}

#[test]
fn gradient_is_applied_only_to_the_text_body() {
    let plain = fresh(vec![command(ORIGIN, UNIT, style(None, true))]);
    let graded = fresh(vec![command(ORIGIN, UNIT, style(vertical(), true))]);
    let body = fresh(vec![command(ORIGIN, UNIT, style(vertical(), false))]);
    let (plain, graded, body) = (instances(&plain), instances(&graded), instances(&body));
    assert_eq!(graded.len(), plain.len());
    let decorations = graded.len() - body.len();
    // 影と縁取り（本体の9倍）は先に並び、指定色のまま。
    assert_eq!(decorations, body.len() * 9);
    for (graded, plain) in graded[..decorations].iter().zip(&plain[..decorations]) {
        assert_plain(graded);
        assert_eq!(graded.rect, plain.rect);
        assert_eq!(graded.color, plain.color);
    }
    for (graded, body) in graded[decorations..].iter().zip(&body) {
        assert_eq!(graded.rect, body.rect);
        assert_eq!(graded.color_end, rgba(BLUE));
        assert_eq!(graded.gradient, body.gradient);
    }
}

#[test]
fn color_glyphs_keep_their_own_colors() {
    let mut style = style(vertical(), false);
    style.size = 0.3;
    let frame = fresh(vec![DrawCommand::Text {
        origin: ORIGIN,
        text: "A😀".to_string(),
        style,
        caret: None,
        post_scale: UNIT,
    }]);
    let quads = instances(&frame);
    assert_eq!(quads.len(), 2);
    assert_eq!(quads[0].color_end, rgba(BLUE));
    // カラー絵文字は白で、透明度だけを掛ける。グラデーションは掛けない。
    assert_eq!(quads[1].color, [1.0, 1.0, 1.0, RED.a]);
    assert_plain(&quads[1]);
}

#[test]
fn gradient_moves_with_cached_layouts_and_post_scale() {
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let first =
        instances(&render(&mut atlas, vec![command(ORIGIN, UNIT, style(vertical(), true))]));
    let layouts = atlas.layouts.len();
    for origin in [Point { x: 0.4, y: 0.6 }, Point { x: -0.2, y: 0.05 }, ORIGIN] {
        let moved =
            instances(&render(&mut atlas, vec![command(origin, UNIT, style(vertical(), true))]));
        assert_eq!(atlas.layouts.len(), layouts, "{origin:?} built a new layout");
        assert_eq!(moved.len(), first.len());
        for (moved, first) in moved.iter().zip(&first) {
            for (moved, first) in moved.gradient.iter().zip(&first.gradient) {
                assert!(approx(*moved, *first), "{origin:?}");
            }
        }
    }
    // 位置からの拡大縮小では、文字と一緒にグラデーションも伸び縮みする。
    let scaled = instances(&fresh(vec![command(
        ORIGIN,
        Point { x: 1.5, y: 0.5 },
        style(vertical(), false),
    )]));
    let body = instances(&fresh(vec![command(ORIGIN, UNIT, style(vertical(), false))]));
    for (scaled, body) in scaled.iter().zip(&body) {
        for (scaled, body) in scaled.gradient.iter().zip(&body.gradient) {
            assert!(approx(*scaled, *body));
        }
    }
}

#[test]
fn bitmap_font_text_uses_the_same_gradient() {
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Text {
            origin: ORIGIN,
            text: "AA".to_string(),
            style: TextStyle { font_id: Some("bitmap".to_string()), ..style(vertical(), false) },
            caret: None,
            post_scale: UNIT,
        }],
    };
    let bitmap_fonts = HashMap::from([("bitmap".to_string(), test_bitmap_font())]);
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let frame = build_text_frame_with_cache(
        &plan,
        &latin(),
        &HashMap::new(),
        &bitmap_fonts,
        SURFACE,
        &mut atlas,
    );
    let quads = instances(&frame);
    assert_eq!(quads.len(), 2);
    for quad in quads {
        assert_eq!(quad.color_end, rgba(BLUE));
        assert!(quad.gradient[2] > 0.0, "{quad:?}");
    }
}

/// 実際の GPU で、上が始点の色、下が終点の色になること。グラデーションの無い文字は
/// 従来どおり1色で描かれること。
#[test]
#[ignore = "requires a GPU; run explicitly for text rendering changes"]
fn gradient_text_renders_on_gpu() {
    let surface = SurfaceSize { width: 128, height: 128 };
    let render = |gradient: Option<TextGradient>| {
        let mut renderer = Renderer::default();
        renderer.insert_vector_font("skin:main".to_string(), latin());
        renderer.attach_offscreen(surface).unwrap();
        let mut style = text_style(Some("skin:main"));
        style.size = 0.9;
        style.color = Color::rgb(1.0, 0.0, 0.0);
        style.gradient = gradient;
        renderer.last_plan = Some(DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.02, y: 0.0 },
                text: "W".to_string(),
                style,
                caret: None,
                post_scale: UNIT,
            }],
        });
        renderer.render_last_plan().unwrap();
        renderer.read_offscreen_rgba().unwrap()
    };
    // 上下それぞれの、文字が塗られた画素の赤と青の合計。
    let halves = |pixels: &[u8]| {
        let mut sums = [[0u64; 2]; 2];
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let half = usize::from(index / surface.width as usize >= surface.height as usize / 2);
            sums[half][0] += u64::from(pixel[0]);
            sums[half][1] += u64::from(pixel[2]);
        }
        sums
    };
    let plain = render(None);
    let [top, bottom] = halves(&plain);
    assert!(top[0] > 0 && bottom[0] > 0, "the glyph is drawn");
    assert_eq!(top[1] + bottom[1], 0, "plain text has no blue");

    let graded = render(Some(TextGradient { color: Color::rgb(0.0, 0.0, 1.0), angle_deg: 0.0 }));
    let [top, bottom] = halves(&graded);
    assert!(top[0] > top[1] * 2, "top is mostly red: {top:?}");
    assert!(bottom[1] > bottom[0] * 2, "bottom is mostly blue: {bottom:?}");
    // 文字の形（アルファ）は変わらない。
    let alpha = |pixels: &[u8]| {
        pixels.chunks_exact(4).map(|p| u64::from(p[0]) + u64::from(p[2])).sum::<u64>()
    };
    let (plain, graded) = (alpha(&plain), alpha(&graded));
    assert!(graded.abs_diff(plain) * 10 < plain, "{plain} / {graded}");
}
