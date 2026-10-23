//! Shrink で横に縮めた文字の、刻んだ倍率でのラスタライズの表示品質の回帰テスト。
//!
//! 同じグリフを、要求どおりの横倍率でラスタライズした場合（変更前）と、
//! `shrunk_raster_scale` で刻んだ倍率でラスタライズして表示で縮めた場合（変更後）とで、
//! 表示 px の格子に描いた結果を比べる。描画は GPU と同じく atlas をバイリニアで読み、
//! 1表示 px を 4×4 の副標本で平均する。

use super::unicode_text::*;
use super::*;

/// 表示 px の格子に描いた RGBA（0..=1）。
struct Canvas {
    pixels: Vec<[f32; 4]>,
}

/// 表示 px で `offset` から `size` の範囲に描くグリフ。
struct Placed {
    glyph: CachedGlyph,
    offset: (f32, f32),
    size: (f32, f32),
}

fn place(glyph: CachedGlyph, stretch_x: f32) -> Placed {
    Placed {
        offset: (glyph.offset_x * stretch_x, glyph.offset_y),
        size: (glyph.display_width * stretch_x, glyph.display_height),
        glyph,
    }
}

fn texel(atlas: &[u8], atlas_width: u32, x: i32, y: i32) -> [f32; 4] {
    let index = ((y as u32 * atlas_width + x as u32) * 4) as usize;
    std::array::from_fn(|channel| atlas[index + channel] as f32 / 255.0)
}

/// テクスチャ座標（texel 単位）でのバイリニア標本。パディングの外は読まない。
fn bilinear(atlas: &[u8], atlas_width: u32, glyph: &CachedGlyph, u: f32, v: f32) -> [f32; 4] {
    let x = glyph.atlas_origin.0 as f32 + u - 0.5;
    let y = glyph.atlas_origin.1 as f32 + v - 0.5;
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let clamp_x = |value: f32| {
        (value as i32).clamp(
            (glyph.atlas_origin.0 - TEXT_ATLAS_PADDING) as i32,
            (glyph.atlas_origin.0 + glyph.width) as i32,
        )
    };
    let clamp_y = |value: f32| {
        (value as i32).clamp(
            (glyph.atlas_origin.1 - TEXT_ATLAS_PADDING) as i32,
            (glyph.atlas_origin.1 + glyph.height) as i32,
        )
    };
    let corners = [
        (texel(atlas, atlas_width, clamp_x(x0), clamp_y(y0)), (1.0 - fx) * (1.0 - fy)),
        (texel(atlas, atlas_width, clamp_x(x0 + 1.0), clamp_y(y0)), fx * (1.0 - fy)),
        (texel(atlas, atlas_width, clamp_x(x0), clamp_y(y0 + 1.0)), (1.0 - fx) * fy),
        (texel(atlas, atlas_width, clamp_x(x0 + 1.0), clamp_y(y0 + 1.0)), fx * fy),
    ];
    std::array::from_fn(|channel| corners.iter().map(|(t, w)| t[channel] * w).sum())
}

const SUBSAMPLES: usize = 4;

fn render(atlas: &TextAtlasCache, placed: &Placed, x0: i32, y0: i32, w: usize, h: usize) -> Canvas {
    let size = atlas.size();
    let pixels = atlas.pixels_for_size(size);
    let mut canvas = Canvas { pixels: vec![[0.0; 4]; w * h] };
    let glyph = &placed.glyph;
    for py in 0..h {
        for px in 0..w {
            let mut sum = [0.0; 4];
            for sy in 0..SUBSAMPLES {
                for sx in 0..SUBSAMPLES {
                    let x = (x0 + px as i32) as f32 + (sx as f32 + 0.5) / SUBSAMPLES as f32;
                    let y = (y0 + py as i32) as f32 + (sy as f32 + 0.5) / SUBSAMPLES as f32;
                    let qx = (x - placed.offset.0) / placed.size.0;
                    let qy = (y - placed.offset.1) / placed.size.1;
                    if !(0.0..1.0).contains(&qx) || !(0.0..1.0).contains(&qy) {
                        continue;
                    }
                    let sample = bilinear(
                        &pixels,
                        size.width,
                        glyph,
                        qx * glyph.width as f32,
                        qy * glyph.height as f32,
                    );
                    // 乗算済みアルファで平均する（GPU のアルファブレンドと同じ重み）。
                    for channel in 0..3 {
                        sum[channel] += sample[channel] * sample[3];
                    }
                    sum[3] += sample[3];
                }
            }
            let n = (SUBSAMPLES * SUBSAMPLES) as f32;
            canvas.pixels[py * w + px] = sum.map(|value| value / n);
        }
    }
    canvas
}

/// 2つの描画の差。`ink` は被覆率の合計の比（変更後 / 変更前）。
#[derive(Debug)]
struct Diff {
    max_alpha: f32,
    mean_alpha: f32,
    max_color: f32,
    ink: f32,
}

fn compare(before: &Canvas, after: &Canvas) -> Diff {
    let mut diff = Diff { max_alpha: 0.0, mean_alpha: 0.0, max_color: 0.0, ink: 1.0 };
    let (mut ink_before, mut ink_after, mut count) = (0.0, 0.0, 0);
    for (b, a) in before.pixels.iter().zip(&after.pixels) {
        if b[3] == 0.0 && a[3] == 0.0 {
            continue;
        }
        let alpha = (b[3] - a[3]).abs();
        diff.max_alpha = diff.max_alpha.max(alpha);
        diff.mean_alpha += alpha;
        for channel in 0..3 {
            diff.max_color = diff.max_color.max((b[channel] - a[channel]).abs());
        }
        ink_before += b[3];
        ink_after += a[3];
        count += 1;
    }
    diff.mean_alpha /= count.max(1) as f32;
    if ink_before > 0.0 {
        diff.ink = ink_after / ink_before;
    }
    diff
}

#[derive(Clone, Copy)]
enum Paint {
    Outline,
    Color,
}

/// 変更前（要求どおりの倍率）と変更後（刻んだ倍率）の描画の差。
fn shrink_diff(face: &TextFace, ch: char, px: f32, ratio: f32, paint: Paint) -> Option<Diff> {
    let glyph = face.glyph_id(ch);
    let scale = PxScale { x: px * ratio, y: px };
    let (raster, stretch) = shrunk_raster_scale(scale);
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let mut cached = |scale| match paint {
        Paint::Outline => atlas.cached_vector_glyph(face, glyph, scale),
        Paint::Color => atlas.cached_color_glyph(face, glyph, scale, false),
    };
    let before = place(cached(scale)?, 1.0);
    let after = place(cached(raster)?, stretch);
    let left = before.offset.0.min(after.offset.0).floor() as i32 - 1;
    let top = before.offset.1.min(after.offset.1).floor() as i32 - 1;
    let right =
        (before.offset.0 + before.size.0).max(after.offset.0 + after.size.0).ceil() as i32 + 1;
    let bottom =
        (before.offset.1 + before.size.1).max(after.offset.1 + after.size.1).ceil() as i32 + 1;
    let (width, height) = ((right - left) as usize, (bottom - top) as usize);
    let before = render(&atlas, &before, left, top, width, height);
    let after = render(&atlas, &after, left, top, width, height);
    Some(compare(&before, &after))
}

/// 1/16 刻みの境界の前後と、極端に縮めた倍率。
const RATIOS: [f32; 10] = [0.999, 0.97, 0.9376, 0.9374, 0.7, 0.5001, 0.4999, 0.3, 0.12, 0.07];
/// 小さい文字から高 DPI の大きな文字まで（表示 px、縦）。
const SIZES: [f32; 4] = [10.0, 24.0, 52.0, 120.0];

fn report(label: &str, face: &TextFace, text: &str, paint: Paint) -> Vec<(String, Diff)> {
    let mut out = Vec::new();
    for ch in text.chars().filter(|ch| face.has_glyph(*ch)) {
        for px in SIZES {
            for ratio in RATIOS {
                if let Some(diff) = shrink_diff(face, ch, px, ratio, paint) {
                    out.push((format!("{label} {ch} {px}px x{ratio}"), diff));
                }
            }
        }
    }
    out
}

fn noto_cjk() -> Option<TextFace> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/fonts/noto-cjk/NotoSansCJK-Regular.ttc");
    let bytes = std::fs::read(path).ok()?;
    TextFace::from_vec("noto-cjk", bytes, 0).ok()
}

#[test]
#[ignore = "prints the shrink rasterization differences; run with --nocapture"]
fn print_shrink_raster_differences() {
    let mut all = report("latin", &latin(), "AWMmil1|-", Paint::Outline);
    all.extend(report("symbols", &symbols(), "♪★✡☀♠", Paint::Outline));
    all.extend(report("color", &color_font(), "😀", Paint::Color));
    if let Some(face) = noto_cjk() {
        all.extend(report("ja", &face, "あいうカタ漢字書鬱一川", Paint::Outline));
    }
    let worst = |key: fn(&Diff) -> f32| {
        all.iter()
            .max_by(|a, b| key(&a.1).total_cmp(&key(&b.1)))
            .map(|(l, d)| format!("{l}: {d:?}"))
    };
    println!("cases: {}", all.len());
    println!("max alpha: {:?}", worst(|d| d.max_alpha));
    println!("mean alpha: {:?}", worst(|d| d.mean_alpha));
    println!("max color: {:?}", worst(|d| d.max_color));
    println!("ink low: {:?}", worst(|d| -d.ink));
    println!("ink high: {:?}", worst(|d| d.ink));
    for label in ["latin", "symbols", "color", "ja"] {
        let cases: Vec<_> = all.iter().filter(|(l, _)| l.starts_with(label)).collect();
        let max =
            |key: fn(&Diff) -> f32| cases.iter().map(|(_, d)| key(d)).fold(f32::MIN, f32::max);
        println!(
            "{label}: cases {} max_alpha {:.3} mean_alpha {:.4} ink {:.3}..{:.3}",
            cases.len(),
            max(|d| d.max_alpha),
            max(|d| d.mean_alpha),
            -max(|d| -d.ink),
            max(|d| d.ink)
        );
    }
    let mean = all.iter().map(|(_, d)| d.mean_alpha).sum::<f32>() / all.len() as f32;
    println!("average mean alpha: {mean}");
}

/// 変更前後の差の上限。描画の差は副画素の位置ずれによるもので、
/// 1段（約4.4%）より大きく縮めると細い線が薄くなる（被覆率の合計が大きく減る）。
fn assert_close(label: &str, face: &TextFace, text: &str, paint: Paint, sizes: &[f32]) {
    for ch in text.chars() {
        assert!(face.has_glyph(ch), "{label}: {ch}");
        for px in sizes {
            for ratio in RATIOS {
                let diff = shrink_diff(face, ch, *px, ratio, paint).expect("rasterized");
                let case = format!("{label} {ch} {px}px x{ratio}: {diff:?}");
                assert!((0.85..=1.15).contains(&diff.ink), "{case}");
                assert!(diff.mean_alpha <= 0.11, "{case}");
                assert!(diff.max_alpha <= 0.2, "{case}");
                if let Paint::Color = paint {
                    // カラー絵文字の色と透明度を保つ。
                    assert!((0.97..=1.03).contains(&diff.ink), "{case}");
                    assert!(diff.max_color <= 0.12, "{case}");
                }
            }
        }
    }
}

#[test]
fn shrunk_latin_and_symbol_glyphs_match_the_exact_scale() {
    assert_close("latin", &latin(), "AWm1l", Paint::Outline, &[10.0, 52.0]);
    assert_close("symbols", &symbols(), "✡♠", Paint::Outline, &[10.0, 52.0]);
}

#[test]
fn shrunk_color_emoji_keeps_its_colors_and_alpha() {
    assert_close("color", &color_font(), "😀", Paint::Color, &[24.0, 120.0]);
}

#[test]
fn shrunk_japanese_keeps_thin_strokes() {
    let Some(face) = noto_cjk() else { return };
    // 横線・縦線の細い字と画数の多い字。極端に縮めても線が薄くならない。
    assert_close("ja", &face, "一川書鬱あカ", Paint::Outline, &[10.0, 52.0]);
}

#[test]
fn display_width_is_continuous_across_raster_steps() {
    // 倍率が刻みの境界をまたいでも、表示の幅は連続して変わる（ちらつかない）。
    let face = latin();
    let glyph = face.glyph_id('W');
    let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
    atlas.begin_frame();
    let width = |atlas: &mut TextAtlasCache, x: f32| {
        let (raster, stretch) = shrunk_raster_scale(PxScale { x, y: 52.0 });
        let cached = atlas.cached_vector_glyph(&face, glyph, raster).unwrap();
        (cached.offset_x * stretch, (cached.offset_x + cached.display_width) * stretch)
    };
    let boundary = 52.0 * 2f32.powf(-3.0 / 16.0);
    let below = width(&mut atlas, boundary - 0.01);
    let above = width(&mut atlas, boundary + 0.01);
    assert!((below.0 - above.0).abs() < 0.1, "{below:?} / {above:?}");
    assert!((below.1 - above.1).abs() < 0.1, "{below:?} / {above:?}");
}
