//! 欠字表示・fallback・カラー絵文字・atlas の修正をまたいだ回帰テスト。
//!
//! 依頼の検証文字列を、スキンフォント＋`font.fallback`・ビットマップフォント・
//! OS フォントの各構成で描き、どの文字も消えないことを確かめる。

use super::unicode_text::*;
use super::*;

const REGRESSION_LINES: [&str; 12] = [
    "Solomon ✡ 12",
    "A✡B",
    "A✡\u{FE0E}B",
    "A✡\u{FE0F}B",
    "A❤B",
    "A❤\u{FE0F}B",
    "A😀B",
    "A🎵B",
    "A👍🏻B",
    "A🇯🇵B",
    "A👨\u{200D}👩\u{200D}👧\u{200D}👦B",
    "日本語✡한글😀中文",
];

fn system_fonts() -> FontFallbackChain {
    load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[])
}

/// OS の既定フォントが日本語とハングルを描けるか（描けない環境では欠字を許す）。
fn system_covers_cjk(system: &FontFallbackChain) -> bool {
    ['日', '한', '中'].iter().all(|ch| system.faces.iter().any(|face| face.has_glyph(*ch)))
}

/// スキンフォント `skin:main` と `font.fallback` を登録した Renderer。
fn renderer_with_skin_chain(faces: Vec<TextFace>) -> Renderer {
    let mut renderer = Renderer::default();
    let mut ids = Vec::new();
    for (index, face) in faces.into_iter().enumerate() {
        let id = if index == 0 { "skin:main".to_string() } else { format!("skin:main#{index}") };
        renderer.insert_vector_font(id.clone(), face);
        if index > 0 {
            ids.push(id);
        }
    }
    renderer.set_font_fallbacks("skin:main", ids);
    renderer
}

fn skin_set<'a>(renderer: &'a Renderer, system: &'a FontFallbackChain) -> VectorFontSet<'a> {
    let (id, faces) = renderer.font_sets.vector.get_key_value("skin:main").unwrap();
    VectorFontSet { id, faces: system.after(faces) }
}

/// 空白以外の cluster が必ずグリフを持ち、描画結果の quad も同じ数だけあること。
fn assert_line_fully_drawn(line: &str, shaped: &ShapedText, quads: usize) {
    let mut glyphs = 0;
    for cluster in &shaped.clusters {
        let text = &line[cluster.text.clone()];
        if text.chars().all(char::is_whitespace) {
            continue;
        }
        let count = shaped.glyphs(cluster).len();
        assert!(count > 0, "{line}: {text:?} vanished");
        glyphs += count;
    }
    assert_eq!(quads, glyphs, "{line}: every glyph must produce a quad");
}

fn line_commands(style: &TextStyle, surface: SurfaceSize, line_height: f32) -> Vec<DrawCommand> {
    REGRESSION_LINES
        .iter()
        .enumerate()
        .map(|(index, line)| DrawCommand::Text {
            origin: Point {
                x: 0.02,
                y: (index as f32 * line_height + 4.0) / surface.height as f32,
            },
            text: line.to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: style.clone(),
        })
        .collect()
}

#[test]
fn regression_lines_keep_every_cluster_across_skin_font_chains() {
    let system = system_fonts();
    let chains = [
        vec![latin(), symbols(), color_font()],
        vec![latin(), fixture_face("bmz-test-colr-empty-base.ttf"), color_font()],
        vec![latin(), fixture_face("bmz-test-emoji-parts.ttf"), symbols(), color_font()],
    ];
    for faces in chains {
        let renderer = renderer_with_skin_chain(faces);
        let fonts = skin_set(&renderer, &system);
        let surface = SurfaceSize { width: 640, height: 480 };
        let mut style = text_style(Some("skin:main"));
        style.size = 24.0 / surface.height as f32;
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: line_commands(&style, surface, 40.0),
        };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let frame = build_text_frame_with_font_sets(
            &plan,
            &system,
            &renderer.font_sets,
            surface,
            &mut atlas,
        );
        for (index, line) in REGRESSION_LINES.iter().enumerate() {
            let shaped = shape_vector_text(line, fonts);
            assert_line_fully_drawn(line, &shaped, frame.command_quad_counts[index]);
            if system_covers_cjk(&system) || index + 1 < REGRESSION_LINES.len() {
                assert_eq!(tofu_glyphs(&shaped), 0, "{line}");
            }
        }
        // 表示形式: ✡ / ✡︎ / ❤ は記号の輪郭、VS16 と絵文字はカラー。
        let paint = |text: &str, cluster: usize| {
            let shaped = shape_vector_text(text, fonts);
            match shaped.glyphs(&shaped.clusters[cluster])[0].source {
                GlyphSource::Vector { color, .. } => color,
                other => panic!("{text}: {other:?}"),
            }
        };
        for (line, color) in [
            ("A✡B", false),
            ("A✡\u{FE0E}B", false),
            ("A✡\u{FE0F}B", true),
            ("A❤B", false),
            ("A❤\u{FE0F}B", true),
            ("A😀B", true),
            ("A🎵B", true),
            ("A👍🏻B", true),
            ("A🇯🇵B", true),
            ("A👨\u{200D}👩\u{200D}👧\u{200D}👦B", true),
        ] {
            assert_eq!(paint(line, 1), color, "{line}");
        }
        assert!(!paint("Solomon ✡ 12", 8), "Solomon ✡ must stay a monochrome symbol");
    }
}

#[test]
fn regression_lines_with_os_fonts_only_never_vanish() {
    let system = system_fonts();
    if system.faces.is_empty() {
        return;
    }
    let fonts = VectorFontSet { id: DEFAULT_TEXT_FONT_ID, faces: system.default_faces() };
    let surface = SurfaceSize { width: 640, height: 480 };
    let mut style = text_style(None);
    style.size = 24.0 / surface.height as f32;
    let plan = DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: line_commands(&style, surface, 40.0),
    };
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    let frame = build_text_frame_with_font_sets(
        &plan,
        &system,
        &SkinFontSets::default(),
        surface,
        &mut atlas,
    );
    for (index, line) in REGRESSION_LINES.iter().enumerate() {
        let shaped = shape_vector_text(line, fonts);
        assert_line_fully_drawn(line, &shaped, frame.command_quad_counts[index]);
    }
}

#[test]
fn regression_lines_through_bitmap_fonts_use_vector_fallbacks() {
    let chars: Vec<(char, i32)> = "ABSolmn12".chars().map(|ch| (ch, 8)).collect();
    let primary = bitmap_font_with(&chars);
    let entries = bitmap_entries(&[&primary]);
    let system = system_fonts();
    let faces = [symbols(), color_font()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: system.after(&faces) };
    for line in REGRESSION_LINES {
        let shaped = shape_bitmap_text(line, fonts);
        assert_line_fully_drawn(line, &shaped, {
            let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
            atlas.begin_frame();
            let mut builder = CachedTextFrameBuilder::new(&mut atlas);
            builder.push_bitmap_text(
                &Point { x: 0.1, y: 0.1 },
                line,
                text_style(Some("bitmap")),
                fonts,
                SURFACE,
            );
            builder.quads.len()
        });
        if system_covers_cjk(&system) || !line.starts_with('日') {
            assert_eq!(tofu_glyphs(&shaped), 0, "{line}");
        }
    }
}

/// 揃え・Truncate・Shrink・影・縁取りで、cluster が欠けたり分割されたりしないこと。
#[test]
fn regression_lines_keep_alignment_truncation_and_decorations() {
    let system = FontFallbackChain::default();
    let renderer = renderer_with_skin_chain(vec![latin(), symbols(), color_font()]);
    let fonts = skin_set(&renderer, &system);
    let quads = |line: &str, style: TextStyle| {
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        atlas.begin_frame();
        let mut builder = CachedTextFrameBuilder::new(&mut atlas);
        builder.push_text(&Point { x: 0.1, y: 0.1 }, line, style, fonts, SURFACE);
        builder.quads
    };
    for line in &REGRESSION_LINES[..11] {
        let shaped = shape_vector_text(line, fonts);
        let style = text_style(None);
        let px = style.size * SURFACE.height as f32;
        let width = clusters_width_px(&shaped.clusters, px);
        let max_width = (width + 60.0) / SURFACE.width as f32;
        let aligned = |align| {
            let mut style = text_style(None);
            style.max_width = max_width;
            style.align = align;
            quads(line, style)
        };
        let left = aligned(TextAlign::Left);
        for (align, ratio) in [(TextAlign::Center, 0.5), (TextAlign::Right, 1.0)] {
            let moved = aligned(align);
            assert_eq!(moved.len(), left.len(), "{line}");
            for (a, b) in left.iter().zip(&moved) {
                assert_approx((b.x - a.x) * SURFACE.width as f32, 60.0 * ratio);
            }
        }

        // 幅の半分で打ち切ると、収まる cluster のグリフだけが残る。
        let mut truncate = text_style(None);
        truncate.max_width = width * 0.5 / SURFACE.width as f32;
        truncate.overflow = TextOverflow::Truncate;
        let kept = truncated_cluster_count(&shaped.clusters, px, width * 0.5);
        let expected: usize = shaped.clusters[..kept]
            .iter()
            .filter(|cluster| !line[cluster.text.clone()].chars().all(char::is_whitespace))
            .map(|cluster| shaped.glyphs(cluster).len())
            .sum();
        assert_eq!(quads(line, truncate).len(), expected, "{line}");

        // 縮小しても全グリフを描く。
        let mut shrink = text_style(None);
        shrink.max_width = width * 0.5 / SURFACE.width as f32;
        shrink.overflow = TextOverflow::Shrink;
        assert_eq!(quads(line, shrink).len(), left.len(), "{line}");

        // 影1回＋縁取り8方向＋本体。
        let mut decorated = text_style(None);
        decorated.shadow = Some(crate::plan::TextShadow {
            color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            offset: Point { x: 0.004, y: 0.004 },
        });
        decorated.outline =
            Some(crate::plan::TextOutline { color: Color::rgb(0.0, 0.0, 1.0), width: 0.002 });
        assert_eq!(quads(line, decorated).len(), left.len() * 10, "{line}");
    }
}

/// `regression_lines_render_on_gpu_including_oversized_emoji` の大きな絵文字の 1em（px）。
///
/// fallback face の em は主フォント（latin）の em に揃う。
fn huge_em_px(primary: &TextFace, surface: SurfaceSize) -> f32 {
    let metrics = primary.metrics();
    0.6 * surface.height as f32 * metrics.units_per_em / metrics.height()
}

/// 実際の GPU（Vulkan / DX12 / Metal、ソフトウェア実装を含む）で検証文字列と
/// atlas 幅を超える大きな絵文字を描き、読み出した画素で確かめる。
/// `BMZ_GPU_TEXT_PREVIEW=<dir>` を指定すると読み出した画像を保存する。
#[test]
#[ignore = "requires a GPU; run explicitly for text and emoji rendering changes"]
fn regression_lines_render_on_gpu_including_oversized_emoji() {
    for surface in
        [SurfaceSize { width: 1280, height: 720 }, SurfaceSize { width: 3840, height: 2160 }]
    {
        let mut renderer = renderer_with_skin_chain(vec![latin(), symbols(), color_font()]);
        renderer.attach_offscreen(surface).unwrap();
        let line_height = surface.height as f32 / 16.0;
        let mut style = text_style(Some("skin:main"));
        style.size = 0.6 / 16.0;
        style.color = Color::rgb(0.2, 1.0, 0.2);
        let mut commands = line_commands(&style, surface, line_height);
        // atlas の幅（1024px）を超える大きさの絵文字（4K では em が1300px 超）。
        let mut huge = style.clone();
        huge.size = 0.6;
        commands.push(DrawCommand::Text {
            origin: Point { x: 0.6, y: 0.2 },
            text: "😀".to_string(),
            caret: None,
            post_scale: Point { x: 1.0, y: 1.0 },
            style: huge,
        });
        renderer.last_plan = Some(DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands });
        renderer.render_last_plan().unwrap();
        let pixels = renderer.read_offscreen_rgba().unwrap();
        if let Ok(dir) = std::env::var("BMZ_GPU_TEXT_PREVIEW") {
            let path = std::path::Path::new(&dir)
                .join(format!("text-{}x{}.png", surface.width, surface.height));
            image::RgbaImage::from_raw(surface.width, surface.height, pixels.clone())
                .unwrap()
                .save(path)
                .unwrap();
        }
        let pixel = |x: u32, y: u32| {
            let index = ((y * surface.width + x) * 4) as usize;
            [pixels[index], pixels[index + 1], pixels[index + 2]]
        };
        let text_right = (surface.width as f32 * 0.55) as u32;
        for (index, line) in REGRESSION_LINES.iter().enumerate() {
            let top = (index as f32 * line_height) as u32;
            let rows = top..(top + line_height as u32).min(surface.height);
            let lit: Vec<[u8; 3]> = rows
                .flat_map(|y| (0..text_right).map(move |x| (x, y)))
                .map(|(x, y)| pixel(x, y))
                .filter(|p| p.iter().map(|v| u32::from(*v)).sum::<u32>() > 60)
                .collect();
            assert!(lit.len() > 50, "{line} ({surface:?}) must leave visible pixels");
            // 文字色は緑。赤・青が勝る画素はフォント由来の色。
            let font_colored = lit.iter().any(|p| p[0] > p[1] || p[2] > p[1]);
            let emoji = !matches!(index, 0 | 1 | 2 | 4) && index + 1 < REGRESSION_LINES.len()
                || index + 1 == REGRESSION_LINES.len();
            assert_eq!(font_colored, emoji, "{line} ({surface:?})");
        }
        // 大きな絵文字は縮小して格納しても、表示寸法どおりの八角形の顔と左右の目で描かれる。
        let is_yellow = |[r, g, b]: [u8; 3]| r > 200 && g > 150 && b < 60;
        let is_dark = |[r, g, b]: [u8; 3]| r < 60 && g < 60 && b < 60;
        let yellow: Vec<(u32, u32)> = (0..surface.height)
            .flat_map(|y| (text_right..surface.width).map(move |x| (x, y)))
            .filter(|(x, y)| is_yellow(pixel(*x, *y)))
            .collect();
        assert!(!yellow.is_empty(), "huge emoji ({surface:?}) must be drawn");
        let (min_x, max_x) =
            yellow.iter().fold((u32::MAX, 0), |(a, b), (x, _)| (a.min(*x), b.max(*x)));
        let (min_y, max_y) =
            yellow.iter().fold((u32::MAX, 0), |(a, b), (_, y)| (a.min(*y), b.max(*y)));
        let em = huge_em_px(&latin(), surface);
        // 顔は 0.9em 四方の八角形。
        for extent in [max_x - min_x, max_y - min_y] {
            assert!(
                (extent as f32 - em * 0.9).abs() < em * 0.05,
                "huge emoji ({surface:?}) extent {extent} for em {em}"
            );
        }
        // 八角形なので四隅は欠けている。
        for (x, y) in [
            (min_x + 2, min_y + 2),
            (max_x - 2, min_y + 2),
            (min_x + 2, max_y - 2),
            (max_x - 2, max_y - 2),
        ] {
            assert!(!is_yellow(pixel(x, y)), "corner ({x}, {y}) must be cut ({surface:?})");
        }
        let center_x = (min_x + max_x) / 2;
        let eyes = |range: std::ops::Range<u32>| {
            (min_y..max_y)
                .flat_map(|y| range.clone().map(move |x| (x, y)))
                .filter(|(x, y)| is_dark(pixel(*x, *y)))
                .count() as f32
        };
        let (left_eye, right_eye) = (eyes(min_x..center_x), eyes(center_x..max_x));
        assert!(
            left_eye > 0.0 && (right_eye / left_eye - 1.0).abs() < 0.2,
            "eyes {left_eye} / {right_eye} ({surface:?})"
        );

        // 2フレーム目はキャッシュを使い、同じ画素になる。
        renderer.render_last_plan().unwrap();
        assert_eq!(renderer.read_offscreen_rgba().unwrap(), pixels, "{surface:?}");
    }
}
