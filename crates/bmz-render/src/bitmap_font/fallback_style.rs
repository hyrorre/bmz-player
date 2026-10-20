//! .fnt のページ画像から、欠字をベクターフォントで描くときの見た目を推定する（BMZ 拡張）。
//!
//! Hiero 等で作った .fnt は、塗りのグラデーションや縁取りをページ画像に焼き込み、
//! 定義ファイルには色を持たない。代表的なグリフの画素から縁取りの色と幅、
//! 行内の高さごとの塗り色を求める。Hiero のグラデーションはグリフごとではなく
//! 行の位置に固定されるため、行の高さごとの色の表で再現できる。

use std::collections::HashMap;

use super::{BitmapFontGlyph, BitmapFontPage};

/// 欠字を補うベクターグリフの塗りと縁取り。座標・幅は .fnt のpx（`info size` が1em）。
#[derive(Debug, Clone, PartialEq)]
pub struct BitmapFallbackStyle {
    /// `fill_rows[0]` の行の位置。行上端（.fnt の `yoffset` の基準）からのpx。
    pub fill_top: i32,
    /// 1pxごとの塗り色（RGB）。範囲外は端の色を使う。
    pub fill_rows: Vec<[u8; 3]>,
    pub outline: Option<BitmapFallbackOutline>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BitmapFallbackOutline {
    /// 塗りの外側に見える縁取りの幅。
    pub width: f32,
    pub color: [u8; 3],
}

impl BitmapFallbackStyle {
    /// 行上端から `y` px の塗り色（0〜1）。画素の中心は `row + 0.5` にある。
    pub fn fill_at(&self, y: f32) -> [f32; 3] {
        let Some(last) = self.fill_rows.len().checked_sub(1) else {
            return [1.0; 3];
        };
        let position = (y - 0.5 - self.fill_top as f32).clamp(0.0, last as f32);
        let lower = position.floor() as usize;
        let upper = (lower + 1).min(last);
        let t = position - lower as f32;
        let (a, b) = (self.fill_rows[lower], self.fill_rows[upper]);
        std::array::from_fn(|channel| {
            (a[channel] as f32 + (b[channel] as f32 - a[channel] as f32) * t) / 255.0
        })
    }
}

/// 推定に使うグリフ。太い線を持つ文字を優先し、文字種の偏りを避ける。
const SAMPLE_CHARS: &str = "HMNEBDKRUWO0123456789AGSXZあかさなはまやらわ漢字日本東京国語音楽曲";
const MAX_SAMPLE_GLYPHS: usize = 48;
const MIN_SAMPLE_GLYPHS: usize = 8;
/// 推定に使う不透明な画素の数の下限。
const MIN_SAMPLE_PIXELS: usize = 64;
/// 縁の色と区別する塗りの色の差（チャンネルごとの最大差）。
const FILL_COLOR_DIFFERENCE: u8 = 48;
/// これ以上明るい塗りだけで縁取りも無ければ、スキンの文字色で塗る白い画像フォント。
const PLAIN_WHITE_LEVEL: u8 = 245;
/// 推定する縁取りの最大幅（px）。
const MAX_OUTLINE_WIDTH: usize = 16;
/// 塗りの画素と行の塗り色との差の平均の上限。手元のスキンでは、グラデーションのフォントが
/// 30前後、文字ごとに色が違うフォント（数字だけ赤い等）が90以上だった。
const MAX_FILL_SPREAD: f32 = 80.0;

/// 輪郭からの距離・色・行上端からの位置を持つ1画素。
struct SamplePixel {
    distance: f32,
    rgb: [u8; 3],
    line_y: i32,
}

/// 推定できない、またはスキンの文字色で塗る白いフォントなら None。
pub(super) fn estimate_fallback_style(
    glyphs: &HashMap<char, BitmapFontGlyph>,
    pages: &HashMap<i32, BitmapFontPage>,
) -> Option<BitmapFallbackStyle> {
    let pixels: Vec<SamplePixel> = sample_glyphs(glyphs, pages)
        .into_iter()
        .flat_map(|(glyph, page)| glyph_pixels(glyph, page))
        .collect();
    if pixels.len() < MIN_SAMPLE_PIXELS {
        return None;
    }
    let edge = edge_color(&pixels)?;
    let is_fill = |pixel: &SamplePixel| color_difference(pixel.rgb, edge) > FILL_COLOR_DIFFERENCE;
    let outline_end = outline_end(&pixels, is_fill);

    let (outline, fill_from) = match outline_end {
        Some(end) => {
            let outline = mean_color(
                pixels
                    .iter()
                    .filter(|pixel| !is_fill(pixel) && pixel.distance <= (end - 0.5).max(1.0)),
            )
            .map(|color| BitmapFallbackOutline { width: end - 0.5, color });
            // 縁取りから塗りへ変わる中間調（約1px）を避ける。
            (outline, end + 1.0)
        }
        // 縁取りが無ければ、輪郭の中間調を避けて内側の色を使う。
        None => (None, 1.5),
    };
    let fill_pixels = || {
        pixels
            .iter()
            .filter(|pixel| pixel.distance >= fill_from && (outline.is_none() || is_fill(pixel)))
    };
    let (fill_top, fill_rows) = fill_rows(fill_pixels())?;
    let style = BitmapFallbackStyle { fill_top, fill_rows, outline };
    // 文字ごとに色が違うフォントは、行の高さで色が決まらない。
    if fill_spread(fill_pixels(), &style) > MAX_FILL_SPREAD {
        return None;
    }
    let plain_white = style.outline.is_none()
        && style.fill_rows.iter().flatten().all(|channel| *channel >= PLAIN_WHITE_LEVEL);
    (!plain_white).then_some(style)
}

/// 塗りの画素と、同じ行の塗り色との差（チャンネルごとの最大差）の平均。
fn fill_spread<'a>(
    pixels: impl Iterator<Item = &'a SamplePixel>,
    style: &BitmapFallbackStyle,
) -> f32 {
    let (mut sum, mut count) = (0u64, 0u64);
    for pixel in pixels {
        let Some(row) = usize::try_from(pixel.line_y - style.fill_top)
            .ok()
            .and_then(|row| style.fill_rows.get(row))
        else {
            continue;
        };
        sum += u64::from(color_difference(pixel.rgb, *row));
        count += 1;
    }
    if count == 0 { 0.0 } else { sum as f32 / count as f32 }
}

fn sample_glyphs<'a>(
    glyphs: &'a HashMap<char, BitmapFontGlyph>,
    pages: &'a HashMap<i32, BitmapFontPage>,
) -> Vec<(&'a BitmapFontGlyph, &'a BitmapFontPage)> {
    let usable = |glyph: &&'a BitmapFontGlyph| glyph.width >= 4 && glyph.height >= 4;
    let with_page = |glyph: &'a BitmapFontGlyph| Some((glyph, pages.get(&glyph.page)?));
    let mut samples: Vec<_> = SAMPLE_CHARS
        .chars()
        .filter_map(|ch| glyphs.get(&ch))
        .filter(usable)
        .filter_map(with_page)
        .take(MAX_SAMPLE_GLYPHS)
        .collect();
    if samples.len() < MIN_SAMPLE_GLYPHS {
        // 記号だけのフォント等。HashMap の順序に依存しないよう文字順に選ぶ。
        let mut others: Vec<_> = glyphs.values().filter(usable).collect();
        others.sort_by_key(|glyph| glyph.id);
        samples = others.into_iter().filter_map(with_page).take(MAX_SAMPLE_GLYPHS).collect();
    }
    samples
}

/// 不透明な画素と、透明な画素（グリフの矩形の外を含む）までの距離。
fn glyph_pixels(glyph: &BitmapFontGlyph, page: &BitmapFontPage) -> Vec<SamplePixel> {
    let (width, height) = (glyph.width as usize, glyph.height as usize);
    let pixel = |x: usize, y: usize| {
        let (page_x, page_y) = (glyph.x as usize + x, glyph.y as usize + y);
        if page_x >= page.image.width as usize || page_y >= page.image.height as usize {
            return [0; 4];
        }
        let index = (page_y * page.image.width as usize + page_x) * 4;
        page.image.pixels.get(index..index + 4).map_or([0; 4], |p| [p[0], p[1], p[2], p[3]])
    };
    let inside: Vec<bool> =
        (0..width * height).map(|index| pixel(index % width, index / width)[3] >= 128).collect();
    let distances = chamfer_distances(&inside, width, height);
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let [r, g, b, a] = pixel(x, y);
            // 輪郭の中間調は背景色と混ざるので使わない。
            if a < 200 {
                continue;
            }
            pixels.push(SamplePixel {
                distance: distances[y * width + x],
                rgb: [r, g, b],
                line_y: glyph.yoffset + y as i32,
            });
        }
    }
    pixels
}

/// 3-4 chamfer で近似したユークリッド距離（px）。外側の画素は0、輪郭に接する画素は1。
fn chamfer_distances(inside: &[bool], width: usize, height: usize) -> Vec<f32> {
    const UNREACHED: u32 = u32::MAX / 2;
    let mut distance: Vec<u32> =
        inside.iter().map(|inside| if *inside { UNREACHED } else { 0 }).collect();
    let at = |distance: &[u32], x: isize, y: isize| {
        if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
            0
        } else {
            distance[y as usize * width + x as usize]
        }
    };
    let neighbours = [(-1, 0, 3), (0, -1, 3), (-1, -1, 4), (1, -1, 4)];
    for y in 0..height as isize {
        for x in 0..width as isize {
            let index = y as usize * width + x as usize;
            if distance[index] == 0 {
                continue;
            }
            let nearest = neighbours
                .iter()
                .map(|(dx, dy, cost)| at(&distance, x + dx, y + dy) + cost)
                .min()
                .unwrap_or(UNREACHED);
            distance[index] = distance[index].min(nearest);
        }
    }
    for y in (0..height as isize).rev() {
        for x in (0..width as isize).rev() {
            let index = y as usize * width + x as usize;
            if distance[index] == 0 {
                continue;
            }
            let nearest = neighbours
                .iter()
                .map(|(dx, dy, cost)| at(&distance, x - dx, y - dy) + cost)
                .min()
                .unwrap_or(UNREACHED);
            distance[index] = distance[index].min(nearest);
        }
    }
    distance.into_iter().map(|distance| distance as f32 / 3.0).collect()
}

/// 輪郭に接する画素の色（チャンネルごとの中央値）。
fn edge_color(pixels: &[SamplePixel]) -> Option<[u8; 3]> {
    let mut channels: [Vec<u8>; 3] = Default::default();
    for pixel in pixels.iter().filter(|pixel| pixel.distance <= 1.0) {
        for (channel, value) in channels.iter_mut().zip(pixel.rgb) {
            channel.push(value);
        }
    }
    if channels[0].len() < MIN_SAMPLE_PIXELS / 4 {
        return None;
    }
    Some(channels.map(|mut values| {
        values.sort_unstable();
        values[values.len() / 2]
    }))
}

fn color_difference(a: [u8; 3], b: [u8; 3]) -> u8 {
    (0..3).map(|channel| a[channel].abs_diff(b[channel])).max().unwrap_or(0)
}

/// 縁の色から塗りの色へ変わる距離。縁取りが無ければ None。
///
/// 距離ごとに塗りの色の画素の割合を求め、半分を超える位置を線形補間する。
/// 距離 d の画素の中心は輪郭から d - 0.5 px にあるため、縁取りの幅はこの値から 0.5 引く。
fn outline_end(pixels: &[SamplePixel], is_fill: impl Fn(&SamplePixel) -> bool) -> Option<f32> {
    let mut fill = [0usize; MAX_OUTLINE_WIDTH + 2];
    let mut total = [0usize; MAX_OUTLINE_WIDTH + 2];
    for pixel in pixels {
        let bin = pixel.distance.round() as usize;
        if (1..total.len()).contains(&bin) {
            total[bin] += 1;
            fill[bin] += usize::from(is_fill(pixel));
        }
    }
    let ratio = |bin: usize| (total[bin] >= 8).then(|| fill[bin] as f32 / total[bin] as f32);
    let filled = pixels.iter().filter(|pixel| is_fill(pixel)).count();
    // 縁で塗りの色が始まる、または塗りの色がほとんど無い（単色のフォント）。
    if ratio(1).is_none_or(|ratio| ratio >= 0.5) || filled * 20 < pixels.len() {
        return None;
    }
    let mut previous = (1, ratio(1)?);
    for bin in 2..total.len() {
        let Some(current) = ratio(bin) else { continue };
        if current >= 0.5 {
            let (previous_bin, previous_ratio) = previous;
            let t = (0.5 - previous_ratio) / (current - previous_ratio).max(f32::EPSILON);
            let end = previous_bin as f32 + (bin - previous_bin) as f32 * t;
            // 1px に満たない縁は、輪郭の中間調と区別できない。
            return (end - 0.5 >= 0.75).then_some(end);
        }
        previous = (bin, current);
    }
    None
}

fn mean_color<'a>(pixels: impl Iterator<Item = &'a SamplePixel>) -> Option<[u8; 3]> {
    let mut sum = [0u64; 3];
    let mut count = 0u64;
    for pixel in pixels {
        for (sum, value) in sum.iter_mut().zip(pixel.rgb) {
            *sum += u64::from(value);
        }
        count += 1;
    }
    (count > 0).then(|| sum.map(|sum| (sum as f64 / count as f64).round() as u8))
}

/// 行の位置ごとの平均色。前後1行と平均し、画素が少ない行は前後の行から補間する。
///
/// 上端・下端の行はグリフの先端の画素だけで、縁取りとの中間調が混ざりやすい。
/// 最も多い行の1/8に満たない行は使わず、範囲外として端の色で描く。
fn fill_rows<'a>(pixels: impl Iterator<Item = &'a SamplePixel>) -> Option<(i32, Vec<[u8; 3]>)> {
    const MIN_ROW_PIXELS: u64 = 3;
    const ROW_PEAK_DIVISOR: u64 = 8;
    let mut rows: HashMap<i32, ([u64; 3], u64)> = HashMap::new();
    for pixel in pixels {
        let (sum, count) = rows.entry(pixel.line_y).or_default();
        for (sum, value) in sum.iter_mut().zip(pixel.rgb) {
            *sum += u64::from(value);
        }
        *count += 1;
    }
    let peak = rows.values().map(|(_, count)| *count).max()?;
    let min_pixels = MIN_ROW_PIXELS.max(peak / ROW_PEAK_DIVISOR);
    rows.retain(|_, (_, count)| *count >= min_pixels);
    let top = *rows.keys().min()?;
    let bottom = *rows.keys().max()?;
    let known: Vec<(i32, [f32; 3])> = (top..=bottom)
        .filter(|y| rows.contains_key(y))
        .map(|y| {
            let (sum, count) = (y - 1..=y + 1).filter_map(|y| rows.get(&y)).fold(
                ([0u64; 3], 0u64),
                |(mut sum, count), (row_sum, row_count)| {
                    for (sum, value) in sum.iter_mut().zip(row_sum) {
                        *sum += value;
                    }
                    (sum, count + row_count)
                },
            );
            (y, sum.map(|sum| sum as f32 / count as f32))
        })
        .collect();
    let mut next = 0;
    let colors = (top..=bottom)
        .map(|y| {
            while next + 1 < known.len() && known[next + 1].0 <= y {
                next += 1;
            }
            let (y0, c0) = known[next];
            let (y1, c1) = known.get(next + 1).copied().unwrap_or(known[next]);
            let t =
                if y1 > y0 { ((y - y0) as f32 / (y1 - y0) as f32).clamp(0.0, 1.0) } else { 0.0 };
            std::array::from_fn(|channel| {
                (c0[channel] + (c1[channel] - c0[channel]) * t).round().clamp(0.0, 255.0) as u8
            })
        })
        .collect();
    Some((top, colors))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::RgbaImageAsset;

    const OUTLINE: [u8; 3] = [30, 40, 50];

    /// 縁取り3px・行の位置で白→灰に変わる塗りの、四角いグリフを並べたページ。
    fn styled_font(
        outline: Option<[u8; 3]>,
        fill: impl Fn(i32) -> [u8; 3],
    ) -> BitmapFallbackFixture {
        let (glyph_width, glyph_height) = (20u32, 24u32);
        let yoffsets = [2, 6, 0, 4, 1, 5, 3, 7, 2, 0];
        let page_width = glyph_width * yoffsets.len() as u32;
        let mut pixels = vec![0u8; (page_width * glyph_height * 4) as usize];
        let mut glyphs = HashMap::new();
        for (index, (ch, yoffset)) in "HMNEBDKRUW".chars().zip(yoffsets).enumerate() {
            let x0 = index as u32 * glyph_width;
            for y in 0..glyph_height {
                for x in 0..glyph_width {
                    // 1px の透明な余白の内側を塗る。
                    if x == 0 || y == 0 || x == glyph_width - 1 || y == glyph_height - 1 {
                        continue;
                    }
                    let ring = x.min(y).min(glyph_width - 1 - x).min(glyph_height - 1 - y);
                    let rgb = match outline {
                        Some(color) if ring <= 3 => color,
                        _ => fill(yoffset + y as i32),
                    };
                    let index = ((y * page_width + x0 + x) * 4) as usize;
                    pixels[index..index + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                }
            }
            glyphs.insert(
                ch,
                BitmapFontGlyph {
                    id: ch,
                    x: x0,
                    y: 0,
                    width: glyph_width,
                    height: glyph_height,
                    xoffset: 0,
                    yoffset,
                    xadvance: glyph_width as i32,
                    page: 0,
                },
            );
        }
        let page = BitmapFontPage {
            id: 0,
            path: "page.png".into(),
            image: RgbaImageAsset { width: page_width, height: glyph_height, pixels },
        };
        BitmapFallbackFixture { glyphs, pages: HashMap::from([(0, page)]) }
    }

    struct BitmapFallbackFixture {
        glyphs: HashMap<char, BitmapFontGlyph>,
        pages: HashMap<i32, BitmapFontPage>,
    }

    impl BitmapFallbackFixture {
        fn estimate(&self) -> Option<BitmapFallbackStyle> {
            estimate_fallback_style(&self.glyphs, &self.pages)
        }
    }

    fn gradient(y: i32) -> [u8; 3] {
        let level = (240 - y * 3).clamp(0, 255) as u8;
        [level, level, level]
    }

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!((actual - expected).abs() <= tolerance, "expected {expected}, got {actual}");
    }

    #[test]
    fn outline_width_color_and_line_gradient_are_estimated() {
        let style = styled_font(Some(OUTLINE), gradient).estimate().expect("styled font");
        let outline = style.outline.expect("outline");
        assert_eq!(outline.color, OUTLINE);
        // 輪郭に接する画素から3px（ring 1〜3）が縁取り。
        assert_close(outline.width, 3.0, 0.5);
        // グリフごとに yoffset が違っても、行の位置の色として揃う。
        for y in [6, 12, 20] {
            let expected = gradient(y)[0] as f32 / 255.0;
            assert_close(style.fill_at(y as f32 + 0.5)[0], expected, 2.0 / 255.0);
        }
        // 推定した範囲の外は端の色。
        assert_eq!(style.fill_at(-100.0), style.fill_at(style.fill_top as f32 + 0.5));
    }

    #[test]
    fn plain_white_fonts_keep_the_skin_text_color() {
        assert_eq!(styled_font(None, |_| [255, 255, 255]).estimate(), None);
    }

    #[test]
    fn single_color_fonts_without_outline_keep_their_fill() {
        let style = styled_font(None, |_| [250, 200, 40]).estimate().expect("colored font");
        assert_eq!(style.outline, None);
        assert!(style.fill_rows.iter().all(|row| *row == [250, 200, 40]));
    }

    #[test]
    fn white_fill_with_dark_outline_is_styled() {
        let style = styled_font(Some([0, 0, 0]), |_| [255, 255, 255]).estimate().unwrap();
        assert_eq!(style.outline.map(|outline| outline.color), Some([0, 0, 0]));
        assert_eq!(style.fill_at(10.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn fonts_with_different_colors_per_glyph_are_not_estimated() {
        let mut fixture = styled_font(None, |_| [250, 250, 250]);
        // 半分のグリフだけ赤くする（LR2 の数字だけ色の違うフォント等）。
        let page = fixture.pages.get_mut(&0).unwrap();
        let page_width = page.image.width;
        for ch in "HNBKU".chars() {
            let glyph = fixture.glyphs[&ch];
            for y in glyph.y..glyph.y + glyph.height {
                for x in glyph.x..glyph.x + glyph.width {
                    let pixel = ((y * page_width + x) * 4) as usize;
                    if page.image.pixels[pixel + 3] > 0 {
                        page.image.pixels[pixel..pixel + 3].copy_from_slice(&[220, 20, 20]);
                    }
                }
            }
        }
        assert_eq!(fixture.estimate(), None);
    }

    #[test]
    fn too_few_pixels_are_not_estimated() {
        let fixture = styled_font(Some(OUTLINE), gradient);
        let glyphs: HashMap<_, _> = fixture
            .glyphs
            .iter()
            .map(|(ch, glyph)| (*ch, BitmapFontGlyph { width: 3, height: 3, ..*glyph }))
            .collect();
        assert_eq!(estimate_fallback_style(&glyphs, &fixture.pages), None);
    }
}
