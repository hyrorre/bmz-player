use super::*;
use crate::bitmap_font::BitmapFallbackStyle;

/// 画像フォント文中の欠字を補うベクターグリフ（BMZ 拡張）。
///
/// .fnt のページ画像から推定した塗り（行の高さごとの色）と縁取りで RGBA にし、
/// 画像フォントのグリフと同じく文字色を掛けて描く。
pub(super) struct StyledFallbackGlyph<'a> {
    /// ビットマップの主フォントの ID。face 番号はこのフォントセット内の番号。
    pub(super) set_id: &'a Arc<str>,
    pub(super) face_index: u16,
    pub(super) face: &'a TextFace,
    pub(super) glyph: GlyphId,
    /// ベクターグリフの `PxScale`。
    pub(super) scale: PxScale,
    /// .fnt の1pxあたりの表示px。
    pub(super) font_scale: PxScale,
    /// 基準線の位置。行上端（.fnt の `yoffset` の基準）からの .fnt のpx。
    pub(super) baseline: f32,
    pub(super) style: &'a BitmapFallbackStyle,
}

impl TextAtlasCache {
    pub(super) fn cached_styled_fallback_glyph(
        &mut self,
        request: StyledFallbackGlyph<'_>,
    ) -> Option<CachedGlyph> {
        let key = TextGlyphKey::styled_fallback(
            request.set_id.clone(),
            request.face_index,
            request.glyph,
            request.scale,
            request.baseline,
        );
        if let Some(glyph) = self.glyphs.get_mut(&key) {
            glyph.last_used_frame = self.frame;
            return Some(glyph.clone());
        }
        if self.failed.contains(&key) || self.is_deferred(&key) {
            return None;
        }
        let Some(raster) = self.raster(|max| rasterize_styled_fallback_glyph(&request, max)) else {
            self.mark_failed(key);
            return None;
        };
        self.insert_glyph_pixels(
            key,
            raster.width,
            raster.height,
            raster.width as f32 / raster.pixels_per_px,
            raster.height as f32 / raster.pixels_per_px,
            raster.offset_x,
            raster.offset_y,
            raster.pixels,
        )
    }
}

/// 輪郭の被覆率を塗り、外側へ縁取りの幅だけ広げた範囲を縁取り色で塗る。
fn rasterize_styled_fallback_glyph(
    request: &StyledFallbackGlyph<'_>,
    max_extent: u32,
) -> Option<OutlineRaster> {
    let outline = request.style.outline.filter(|outline| outline.width > 0.0);
    // 縁取りの外側の中間調の分（1px）も含めて広げる。
    let margins = |pixels_per_px: f32| {
        let Some(outline) = outline else { return (0, 0) };
        let margin = |scale: f32| ((outline.width + 1.0) * scale * pixels_per_px).ceil() as u32;
        (margin(request.font_scale.x), margin(request.font_scale.y))
    };
    // 倍率は atlas に収めるために下がることはあっても上がらないので、超解像の倍率で見積もる。
    let (margin_x, margin_y) = margins(VECTOR_TEXT_SUPERSAMPLE_SCALE);
    let glyph_extent = max_extent.saturating_sub(margin_x.max(margin_y) * 2).max(1);
    let glyph = rasterize_outline_glyph(request.face, request.glyph, request.scale, glyph_extent)?;
    let pixels_per_px = glyph.pixels_per_px;
    let (margin_x, margin_y) = margins(pixels_per_px);
    let width = glyph.width + margin_x * 2;
    let height = glyph.height + margin_y * 2;
    let (w, h) = (width as usize, height as usize);

    let mut coverage = vec![0.0f32; w * h];
    for y in 0..glyph.height as usize {
        for x in 0..glyph.width as usize {
            let alpha = glyph.pixels[(y * glyph.width as usize + x) * 4 + 3];
            coverage[(y + margin_y as usize) * w + x + margin_x as usize] = alpha as f32 / 255.0;
        }
    }
    // 縁取りの被覆率。輪郭の内側の画素からの距離（.fnt のpx）で決め、1画素ぶんぼかす。
    let outline_coverage = outline.map(|outline| {
        let step_x = 1.0 / (request.font_scale.x * pixels_per_px);
        let step_y = 1.0 / (request.font_scale.y * pixels_per_px);
        let inside: Vec<bool> = coverage.iter().map(|coverage| *coverage >= 0.5).collect();
        let soft = (step_x + step_y) / 2.0;
        squared_distances(&inside, w, h, step_x, step_y)
            .into_iter()
            .map(|distance| ((outline.width - distance.sqrt()) / soft + 1.0).clamp(0.0, 1.0))
            .collect::<Vec<_>>()
    });
    let outline_color = outline.map_or([0.0; 3], |outline| outline.color.map(|c| c as f32 / 255.0));

    let offset_y = glyph.offset_y - margin_y as f32 / pixels_per_px;
    let mut pixels = vec![0u8; w * h * 4];
    for y in 0..h {
        let display_y = offset_y + (y as f32 + 0.5) / pixels_per_px;
        let fill = request.style.fill_at(request.baseline + display_y / request.font_scale.y);
        for x in 0..w {
            let index = y * w + x;
            let fill_alpha = coverage[index];
            let outline_alpha = outline_coverage.as_ref().map_or(0.0, |coverage| coverage[index])
                * (1.0 - fill_alpha);
            let alpha = fill_alpha + outline_alpha;
            if alpha <= f32::EPSILON {
                continue;
            }
            let pixel = &mut pixels[index * 4..index * 4 + 4];
            for channel in 0..3 {
                let color =
                    (fill[channel] * fill_alpha + outline_color[channel] * outline_alpha) / alpha;
                pixel[channel] = (color * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            pixel[3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    Some(OutlineRaster {
        width,
        height,
        pixels_per_px,
        offset_x: glyph.offset_x - margin_x as f32 / pixels_per_px,
        offset_y,
        pixels,
    })
}

/// 内側の画素までの距離の2乗。`step_x` / `step_y` は1画素の長さ。
fn squared_distances(
    inside: &[bool],
    width: usize,
    height: usize,
    step_x: f32,
    step_y: f32,
) -> Vec<f32> {
    const FAR: f32 = 1.0e20;
    let mut grid: Vec<f32> = inside.iter().map(|inside| if *inside { 0.0 } else { FAR }).collect();
    let mut line = Vec::with_capacity(width.max(height));
    let mut out = Vec::with_capacity(width.max(height));
    // Envelope workspace is shared by all rows and columns of this glyph.
    // Each transform overwrites its active prefix; no per-line allocation is needed.
    let mut vertices = vec![0usize; width.max(height)];
    let mut bounds = vec![0.0f32; width.max(height) + 1];
    for x in 0..width {
        line.clear();
        line.extend((0..height).map(|y| grid[y * width + x]));
        distance_transform_1d(&line, step_y, &mut out, &mut vertices, &mut bounds);
        for (y, distance) in out.iter().enumerate() {
            grid[y * width + x] = *distance;
        }
    }
    for row in grid.chunks_exact_mut(width) {
        line.clear();
        line.extend_from_slice(row);
        distance_transform_1d(&line, step_x, &mut out, &mut vertices, &mut bounds);
        row.copy_from_slice(&out);
    }
    grid
}

/// Felzenszwalb & Huttenlocher の距離変換（放物線の下側包絡線）。
///
/// `min_q (step (p - q))^2 + f(q)` を、`f / step^2` の変換結果に `step^2` を掛けて求める。
fn distance_transform_1d(
    values: &[f32],
    step: f32,
    out: &mut Vec<f32>,
    vertices: &mut [usize],
    bounds: &mut [f32],
) {
    let n = values.len();
    out.clear();
    if n == 0 {
        return;
    }
    let step_squared = step * step;
    let f = |q: usize| values[q] / step_squared;
    let intersection = |q: usize, v: usize| {
        ((f(q) + (q * q) as f32) - (f(v) + (v * v) as f32)) / (2.0 * (q as f32 - v as f32))
    };
    let mut k = 0;
    vertices[0] = 0;
    bounds[0] = f32::NEG_INFINITY;
    bounds[1] = f32::INFINITY;
    for q in 1..n {
        let mut s = intersection(q, vertices[k]);
        while k > 0 && s <= bounds[k] {
            k -= 1;
            s = intersection(q, vertices[k]);
        }
        k += 1;
        vertices[k] = q;
        bounds[k] = s;
        bounds[k + 1] = f32::INFINITY;
    }
    k = 0;
    for q in 0..n {
        while bounds[k + 1] < q as f32 {
            k += 1;
        }
        let v = vertices[k];
        let offset = q as f32 - v as f32;
        out.push((offset * offset + f(v)) * step_squared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reused_distance_workspace_matches_nearest_pixel_distances() {
        for (width, height) in [(1, 1), (1, 17), (17, 1), (3, 11), (19, 4), (12, 12)] {
            for seed in 0..8 {
                let mut inside: Vec<_> = (0..width * height)
                    .map(|i| ((i % width) * 7 + (i / width) * 11 + seed) % 13 == 0)
                    .collect();
                inside[seed % (width * height)] = true;
                for (step_x, step_y) in [(1.0, 1.0), (0.5, 2.0), (2.5, 0.25)] {
                    let actual = squared_distances(&inside, width, height, step_x, step_y);
                    for (i, distance) in actual.iter().enumerate() {
                        let expected = inside
                            .iter()
                            .enumerate()
                            .filter(|(_, set)| **set)
                            .map(|(j, _)| {
                                let dx = ((i % width) as f32 - (j % width) as f32) * step_x;
                                let dy = ((i / width) as f32 - (j / width) as f32) * step_y;
                                dx * dx + dy * dy
                            })
                            .fold(f32::INFINITY, f32::min);
                        assert!(
                            (distance - expected).abs() < 1e-3,
                            "{width}x{height}, seed={seed}, steps={step_x}/{step_y}, pixel={i}: {distance} != {expected}"
                        );
                    }
                }
            }
        }
    }

    /// CPU rasterization only, excluding font loading, atlas upload and hashing.
    /// cargo test -p bmz-render --release profile_styled_fallback -- --ignored --nocapture
    #[test]
    #[ignore = "release CPU performance probe"]
    fn profile_styled_fallback_rasterization() {
        assert!(!std::hint::black_box(cfg!(debug_assertions)), "run with --release");
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fonts/bmz-test-symbols.ttf");
        let face = TextFace::from_vec("symbols", std::fs::read(path).unwrap(), 0).unwrap();
        let style = BitmapFallbackStyle {
            fill_top: 0,
            fill_rows: vec![[240, 220, 180]; 16],
            outline: Some(crate::bitmap_font::BitmapFallbackOutline {
                width: 2.0,
                color: [20, 30, 40],
            }),
        };
        let iterations = std::env::var("BMZ_TEXT_PROFILE_ITERATIONS")
            .ok()
            .map(|v| v.parse::<usize>().unwrap())
            .unwrap_or(200);
        assert!((1..=100_000).contains(&iterations));
        for size in [24.0, 72.0, 192.0] {
            let request = StyledFallbackGlyph {
                set_id: &Arc::from("bitmap"),
                face_index: 0,
                face: &face,
                glyph: face.font().glyph_id('✡'),
                scale: PxScale::from(size),
                font_scale: PxScale::from(size / 16.0),
                baseline: 12.0,
                style: &style,
            };
            let mut total = std::time::Duration::ZERO;
            let mut last = None;
            for i in 0..iterations + 20 {
                let start = std::time::Instant::now();
                let raster =
                    std::hint::black_box(rasterize_styled_fallback_glyph(&request, 2046).unwrap());
                let elapsed = start.elapsed();
                if i >= 20 {
                    total += elapsed;
                }
                last = Some(raster);
            }
            let raster = last.unwrap();
            let mut hash = 0xcbf2_9ce4_8422_2325u64;
            for byte in &raster.pixels {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
            }
            println!(
                "{}",
                serde_json::json!({"size":size,"iterations":iterations,
                "mean_ns":total.as_nanos() as f64 / iterations as f64,
                "width":raster.width,"height":raster.height,"pixel_hash":format!("{hash:016x}")})
            );
        }
    }

    #[test]
    fn distance_transform_scales_each_axis() {
        // 中央の1画素からの距離。横は1画素0.5、縦は1画素2の長さ。
        let (width, height) = (5, 3);
        let mut inside = vec![false; width * height];
        inside[width + 2] = true;
        let distances = squared_distances(&inside, width, height, 0.5, 2.0);
        assert_eq!(distances[width + 2], 0.0);
        assert!((distances[width] - 1.0).abs() < 1e-4, "two pixels to the left");
        assert!((distances[2] - 4.0).abs() < 1e-4, "one pixel above");
        assert!((distances[0] - 5.0).abs() < 1e-4, "diagonal");
    }
}
