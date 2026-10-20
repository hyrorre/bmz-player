//! カラーグリフ（COLR v0/v1、CBDT/CBLC、sbix）の RGBA ラスタライズ。
//!
//! COLR の paint graph は skrifa が解釈し、描画命令（transform・clip・layer・fill）を
//! ここで合成する。輪郭の被覆率は ab_glyph_rasterizer で求める。ビットマップは
//! skrifa で最も近い strike を選び、PNG / BGRA を目的の ppem へ縮小・拡大する。
//! OpenType SVG と単色の EBDT は扱わず、呼び出し側が輪郭グリフか欠字表示へ戻す。

use ab_glyph_rasterizer::{Rasterizer, point};
use skrifa::MetadataProvider;
use skrifa::color::{Brush, ColorPainter, ColorStop, CompositeMode, Extend, Transform};
use skrifa::raw::types::BoundingBox;

use super::*;

/// 1辺の上限。異常な clip box で巨大な画素を確保しない。
const COLOR_GLYPH_MAX_EXTENT: f32 = 2048.0;

/// 行基準からの位置付きのカラーグリフ画素（straight alpha の RGBA8）。
pub(super) struct ColorRaster {
    pub(super) width: u32,
    pub(super) height: u32,
    /// pen 位置からの左端（px）。
    pub(super) offset_x: f32,
    /// 基準線からの上端（px、下向き正）。
    pub(super) offset_y: f32,
    pub(super) pixels: Vec<u8>,
}

/// face が持つカラーグリフの形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColorGlyphKind {
    Colr,
    Bitmap,
}

/// グリフを描けるカラー形式。
///
/// ラスタライズと同じ条件で判定する。COLR は描画範囲が求まり paint graph を
/// 最後まで辿れること、ビットマップは PNG の寸法か BGRA の画素数が読めること。
/// 結果は `TextFace::paints` が face ごとにキャッシュする。
pub(super) fn color_glyph_kind(face: &TextFace, glyph: GlyphId) -> Option<ColorGlyphKind> {
    if glyph.0 == 0 || !face.has_color_tables() {
        return None;
    }
    let font = skrifa::FontRef::from_index(face.data(), face.index()).ok()?;
    let glyph_id = skrifa::GlyphId::new(u32::from(glyph.0));
    if let Some(color) = font.color_glyphs().get(glyph_id) {
        let bounds = colr_bounds(face, &color)?;
        let mut painter = BoundsPainter::new(face);
        color.paint(skrifa::instance::LocationRef::default(), &mut painter).ok()?;
        return (bounds.x_min < bounds.x_max && bounds.y_min < bounds.y_max)
            .then_some(ColorGlyphKind::Colr);
    }
    color_bitmap_for_size(&font, glyph_id, f32::MAX).map(|_| ColorGlyphKind::Bitmap)
}

/// COLR グリフの描画範囲（フォント単位）。v1 の ClipBox、無ければ clip の和集合。
fn colr_bounds(face: &TextFace, color: &skrifa::color::ColorGlyph<'_>) -> Option<BoundingBox<f32>> {
    let location = skrifa::instance::LocationRef::default();
    color.bounding_box(location, skrifa::instance::Size::unscaled()).or_else(|| {
        let mut painter = BoundsPainter::new(face);
        color.paint(location, &mut painter).ok()?;
        painter.bounds
    })
}

/// PNG / BGRA を読める strike のうち、`ppem` と同じか直上、無ければ直下のもの。
///
/// skrifa の `glyph_for_size` と同じ優先順だが、単色マスクや壊れた画像の strike は選ばない。
fn color_bitmap_for_size<'a>(
    font: &skrifa::FontRef<'a>,
    glyph_id: skrifa::GlyphId,
    ppem: f32,
) -> Option<skrifa::bitmap::BitmapGlyph<'a>> {
    skrifa::bitmap::BitmapStrikes::new(font)
        .iter()
        .filter_map(|strike| strike.get(glyph_id).filter(readable_color_bitmap))
        .fold(None, |best: Option<skrifa::bitmap::BitmapGlyph<'a>>, bitmap| match best {
            Some(best)
                if !((bitmap.ppem_y >= ppem && bitmap.ppem_y < best.ppem_y)
                    || (best.ppem_y < ppem && bitmap.ppem_y > best.ppem_y)) =>
            {
                Some(best)
            }
            _ => Some(bitmap),
        })
}

fn readable_color_bitmap(bitmap: &skrifa::bitmap::BitmapGlyph<'_>) -> bool {
    match bitmap.data {
        skrifa::bitmap::BitmapData::Png(data) => {
            image::ImageReader::with_format(std::io::Cursor::new(data), image::ImageFormat::Png)
                .into_dimensions()
                .is_ok_and(|(width, height)| width > 0 && height > 0)
        }
        skrifa::bitmap::BitmapData::Bgra(data) => {
            bitmap.width > 0
                && bitmap.height > 0
                && data.len() as u64 >= u64::from(bitmap.width) * u64::from(bitmap.height) * 4
        }
        skrifa::bitmap::BitmapData::Mask(_) => false,
    }
}

/// `scale` は ab_glyph と同じ `PxScale`（ascent - descent を px で表した値）。
pub(super) fn rasterize_color_glyph(
    face: &TextFace,
    glyph: GlyphId,
    scale: PxScale,
) -> Option<ColorRaster> {
    let font = skrifa::FontRef::from_index(face.data(), face.index()).ok()?;
    let glyph_id = skrifa::GlyphId::new(u32::from(glyph.0));
    let metrics = face.metrics();
    let units = (scale.x / metrics.height(), scale.y / metrics.height());
    let mut raster = if let Some(color) = font.color_glyphs().get(glyph_id) {
        rasterize_colr(face, &font, &color, units)?
    } else {
        rasterize_bitmap(&font, glyph_id, metrics, units)?
    };
    bleed_transparent_edges(&mut raster);
    Some(raster)
}

fn rasterize_colr(
    face: &TextFace,
    font: &skrifa::FontRef<'_>,
    color: &skrifa::color::ColorGlyph<'_>,
    units: (f32, f32),
) -> Option<ColorRaster> {
    let location = skrifa::instance::LocationRef::default();
    let bounds = colr_bounds(face, color)?;
    let left = (bounds.x_min * units.0).floor();
    let top = (-bounds.y_max * units.1).floor();
    let width = ((bounds.x_max * units.0).ceil() - left).clamp(0.0, COLOR_GLYPH_MAX_EXTENT);
    let height = ((-bounds.y_min * units.1).ceil() - top).clamp(0.0, COLOR_GLYPH_MAX_EXTENT);
    if width < 1.0 || height < 1.0 {
        return None;
    }
    let palettes = font.color_palettes();
    let palette = palettes.get(0);
    let colors: Vec<[f32; 4]> = palette
        .map(|palette| {
            palette
                .colors()
                .iter()
                .map(|color| {
                    [color.red, color.green, color.blue, color.alpha].map(|v| v as f32 / 255.0)
                })
                .collect()
        })
        .unwrap_or_default();
    // フォント単位（y上向き）→ 画素（y下向き）。
    let base = Transform { xx: units.0, yx: 0.0, xy: 0.0, yy: -units.1, dx: -left, dy: -top };
    let mut painter = Compositor::new(face, width as usize, height as usize, base, colors);
    color.paint(location, &mut painter).ok()?;
    Some(ColorRaster {
        width: width as u32,
        height: height as u32,
        offset_x: left,
        offset_y: top,
        pixels: painter.into_rgba8(),
    })
}

fn rasterize_bitmap(
    font: &skrifa::FontRef<'_>,
    glyph_id: skrifa::GlyphId,
    metrics: FaceMetrics,
    units: (f32, f32),
) -> Option<ColorRaster> {
    let ppem = units.1 * metrics.units_per_em;
    let bitmap = color_bitmap_for_size(font, glyph_id, ppem)?;
    let image = match bitmap.data {
        skrifa::bitmap::BitmapData::Png(data) => {
            image::load_from_memory_with_format(data, image::ImageFormat::Png).ok()?.to_rgba8()
        }
        skrifa::bitmap::BitmapData::Bgra(data) => {
            let len = (bitmap.width * bitmap.height * 4) as usize;
            let mut rgba = data.get(..len)?.to_vec();
            for pixel in rgba.chunks_exact_mut(4) {
                // CBDT の BGRA は premultiplied。
                pixel.swap(0, 2);
                let alpha = pixel[3] as f32 / 255.0;
                if alpha > 0.0 {
                    for channel in &mut pixel[..3] {
                        *channel = (*channel as f32 / alpha).round().min(255.0) as u8;
                    }
                }
            }
            image::RgbaImage::from_raw(bitmap.width, bitmap.height, rgba)?
        }
        skrifa::bitmap::BitmapData::Mask(_) => return None,
    };
    let scale_y = ppem / bitmap.ppem_y.max(1.0);
    let scale_x = units.0 * metrics.units_per_em / bitmap.ppem_x.max(1.0);
    let width = (image.width() as f32 * scale_x).round().clamp(1.0, COLOR_GLYPH_MAX_EXTENT);
    let height = (image.height() as f32 * scale_y).round().clamp(1.0, COLOR_GLYPH_MAX_EXTENT);
    // Skia の fontations 実装と同じ配置。CBDT は左上、sbix は左下が原点。
    let (offset_x, offset_y) = match bitmap.placement_origin {
        skrifa::bitmap::Origin::TopLeft => {
            (bitmap.inner_bearing_x * scale_x, -bitmap.inner_bearing_y * scale_y)
        }
        skrifa::bitmap::Origin::BottomLeft => {
            let left = bitmap.bearing_x * units.0 + bitmap.inner_bearing_x * scale_x;
            let bottom = bitmap.bearing_y * units.1 + bitmap.inner_bearing_y * scale_y;
            (left, -bottom - height)
        }
    };
    let pixels = resize_rgba(&image, width as u32, height as u32);
    Some(ColorRaster { width: width as u32, height: height as u32, offset_x, offset_y, pixels })
}

/// premultiplied で縮小・拡大し、透明部分の色が縁へ混ざらないようにする。
fn resize_rgba(image: &image::RgbaImage, width: u32, height: u32) -> Vec<u8> {
    if image.width() == width && image.height() == height {
        return image.as_raw().clone();
    }
    let mut premultiplied = image.clone();
    for pixel in premultiplied.pixels_mut() {
        let alpha = pixel[3] as u32;
        for channel in 0..3 {
            pixel[channel] = ((pixel[channel] as u32 * alpha + 127) / 255) as u8;
        }
    }
    let mut resized = image::imageops::resize(
        &premultiplied,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    for pixel in resized.pixels_mut() {
        let alpha = pixel[3] as u32;
        for channel in 0..3 {
            if let Some(value) = (pixel[channel] as u32 * 255 + alpha / 2).checked_div(alpha) {
                pixel[channel] = value.min(255) as u8;
            }
        }
    }
    resized.into_raw()
}

/// 完全透明の画素へ隣接画素の色を写し、atlas の線形補間で縁が黒ずまないようにする。
fn bleed_transparent_edges(raster: &mut ColorRaster) {
    let (width, height) = (raster.width as usize, raster.height as usize);
    let source = raster.pixels.clone();
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 4;
            if source[index + 3] != 0 {
                continue;
            }
            let neighbor = [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().find_map(|(dx, dy)| {
                let nx = x.checked_add_signed(*dx).filter(|nx| *nx < width)?;
                let ny = y.checked_add_signed(*dy).filter(|ny| *ny < height)?;
                let neighbor = (ny * width + nx) * 4;
                (source[neighbor + 3] != 0).then_some(neighbor)
            });
            if let Some(neighbor) = neighbor {
                raster.pixels[index..index + 3].copy_from_slice(&source[neighbor..neighbor + 3]);
            }
        }
    }
}

fn apply(transform: &Transform, x: f32, y: f32) -> (f32, f32) {
    (
        transform.xx * x + transform.xy * y + transform.dx,
        transform.yx * x + transform.yy * y + transform.dy,
    )
}

fn invert(transform: &Transform) -> Option<Transform> {
    let det = transform.xx * transform.yy - transform.xy * transform.yx;
    if det.abs() <= f32::EPSILON {
        return None;
    }
    let inverse = 1.0 / det;
    let xx = transform.yy * inverse;
    let xy = -transform.xy * inverse;
    let yx = -transform.yx * inverse;
    let yy = transform.xx * inverse;
    Some(Transform {
        xx,
        xy,
        yx,
        yy,
        dx: -(xx * transform.dx + xy * transform.dy),
        dy: -(yx * transform.dx + yy * transform.dy),
    })
}

/// clip glyph / clip box の和集合から描画範囲を求める（v1 の ClipBox が無い場合）。
struct BoundsPainter<'a> {
    face: &'a TextFace,
    transforms: Vec<Transform>,
    bounds: Option<BoundingBox<f32>>,
}

impl<'a> BoundsPainter<'a> {
    fn new(face: &'a TextFace) -> Self {
        Self { face, transforms: vec![Transform::default()], bounds: None }
    }

    fn include(&mut self, corners: [(f32, f32); 4]) {
        let transform = *self.transforms.last().unwrap();
        for (x, y) in corners {
            let (x, y) = apply(&transform, x, y);
            let bounds =
                self.bounds.get_or_insert(BoundingBox { x_min: x, y_min: y, x_max: x, y_max: y });
            bounds.x_min = bounds.x_min.min(x);
            bounds.y_min = bounds.y_min.min(y);
            bounds.x_max = bounds.x_max.max(x);
            bounds.y_max = bounds.y_max.max(y);
        }
    }
}

impl ColorPainter for BoundsPainter<'_> {
    fn push_transform(&mut self, transform: Transform) {
        let current = *self.transforms.last().unwrap();
        self.transforms.push(current * transform);
    }

    fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: skrifa::GlyphId) {
        let Some(outline) = glyph_outline(self.face, glyph_id) else { return };
        let (x0, x1) = (outline.bounds.min.x, outline.bounds.max.x);
        let (y0, y1) = (outline.bounds.max.y, outline.bounds.min.y);
        self.include([(x0, y0), (x1, y0), (x0, y1), (x1, y1)]);
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>) {
        self.include([
            (clip_box.x_min, clip_box.y_min),
            (clip_box.x_max, clip_box.y_min),
            (clip_box.x_min, clip_box.y_max),
            (clip_box.x_max, clip_box.y_max),
        ]);
    }

    fn pop_clip(&mut self) {}

    fn fill(&mut self, _brush: Brush<'_>) {}

    fn push_layer(&mut self, _composite_mode: CompositeMode) {}
}

fn glyph_outline(face: &TextFace, glyph_id: skrifa::GlyphId) -> Option<ab_glyph::Outline> {
    let glyph = u16::try_from(glyph_id.to_u32()).ok()?;
    face.font().outline(GlyphId(glyph))
}

/// premultiplied RGBA（f32）の layer stack に描画命令を合成する。
struct Compositor<'a> {
    face: &'a TextFace,
    width: usize,
    height: usize,
    transforms: Vec<Transform>,
    clips: Vec<Vec<f32>>,
    layers: Vec<Vec<[f32; 4]>>,
    palette: Vec<[f32; 4]>,
}

impl<'a> Compositor<'a> {
    fn new(
        face: &'a TextFace,
        width: usize,
        height: usize,
        base: Transform,
        palette: Vec<[f32; 4]>,
    ) -> Self {
        Self {
            face,
            width,
            height,
            transforms: vec![base],
            clips: Vec::new(),
            layers: vec![vec![[0.0; 4]; width * height]],
            palette,
        }
    }

    fn transform(&self) -> Transform {
        *self.transforms.last().unwrap()
    }

    fn coverage(&self, index: usize) -> f32 {
        self.clips.last().map_or(1.0, |clip| clip[index])
    }

    /// 現在の transform で多角形・曲線を塗りつぶした被覆率と、既存 clip の積を積む。
    fn push_clip_mask(&mut self, draw: impl FnOnce(&mut Rasterizer, &Transform)) {
        let mut rasterizer = Rasterizer::new(self.width, self.height);
        draw(&mut rasterizer, &self.transform());
        let mut mask = vec![0.0; self.width * self.height];
        rasterizer.for_each_pixel(|index, coverage| {
            if let Some(value) = mask.get_mut(index) {
                *value = coverage.clamp(0.0, 1.0);
            }
        });
        if let Some(previous) = self.clips.last() {
            for (value, previous) in mask.iter_mut().zip(previous) {
                *value *= previous;
            }
        }
        self.clips.push(mask);
    }

    fn palette_color(&self, palette_index: u16, alpha: f32) -> [f32; 4] {
        // 0xFFFF は前景色。カラーグリフは白で描き、透明度だけを文字色から受け取る。
        let [r, g, b, a] =
            self.palette.get(usize::from(palette_index)).copied().unwrap_or([1.0; 4]);
        let a = (a * alpha).clamp(0.0, 1.0);
        [r * a, g * a, b * a, a]
    }

    fn stop_color(&self, stops: &[ColorStop], t: f32) -> [f32; 4] {
        let Some(first) = stops.first() else { return [0.0; 4] };
        if stops.len() == 1 || t <= first.offset {
            return self.palette_color(first.palette_index, first.alpha);
        }
        for pair in stops.windows(2) {
            let (start, end) = (&pair[0], &pair[1]);
            if t <= end.offset {
                let span = end.offset - start.offset;
                let f = if span > f32::EPSILON { (t - start.offset) / span } else { 1.0 };
                let a = self.palette_color(start.palette_index, start.alpha);
                let b = self.palette_color(end.palette_index, end.alpha);
                return [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * f);
            }
        }
        let last = stops.last().unwrap();
        self.palette_color(last.palette_index, last.alpha)
    }

    /// paint 空間の点 `(x, y)` での brush の色（premultiplied）。
    fn brush_color(&self, brush: &Brush<'_>, x: f32, y: f32) -> [f32; 4] {
        match brush {
            Brush::Solid { palette_index, alpha } => self.palette_color(*palette_index, *alpha),
            Brush::LinearGradient { p0, p1, color_stops, extend } => {
                let (dx, dy) = (p1.x - p0.x, p1.y - p0.y);
                let length = dx * dx + dy * dy;
                if length <= f32::EPSILON {
                    return self.stop_color(color_stops, 0.0);
                }
                let t = ((x - p0.x) * dx + (y - p0.y) * dy) / length;
                self.stop_color(color_stops, extend_t(t, *extend))
            }
            Brush::RadialGradient { c0, r0, c1, r1, color_stops, extend } => {
                let Some(t) = radial_t((x, y), (c0.x, c0.y), *r0, (c1.x, c1.y), *r1) else {
                    return [0.0; 4];
                };
                self.stop_color(color_stops, extend_t(t, *extend))
            }
            Brush::SweepGradient { c0, start_angle, end_angle, color_stops, extend } => {
                // フォント座標（y上向き）の反時計回りは、画面（y下向き）の時計回り。
                let angle = (y - c0.y).atan2(x - c0.x).to_degrees().rem_euclid(360.0);
                let span = end_angle - start_angle;
                if span.abs() <= f32::EPSILON {
                    return self.stop_color(color_stops, 0.0);
                }
                self.stop_color(color_stops, extend_t((angle - start_angle) / span, *extend))
            }
        }
    }
}

fn extend_t(t: f32, extend: Extend) -> f32 {
    match extend {
        Extend::Repeat => t - t.floor(),
        Extend::Reflect => {
            let t = t.rem_euclid(2.0);
            if t > 1.0 { 2.0 - t } else { t }
        }
        _ => t.clamp(0.0, 1.0),
    }
}

/// 2点間の円錐グラデーションで、点 `p` を通る円のうち大きい方の `t`。
fn radial_t(p: (f32, f32), c0: (f32, f32), r0: f32, c1: (f32, f32), r1: f32) -> Option<f32> {
    let (cdx, cdy, dr) = (c1.0 - c0.0, c1.1 - c0.1, r1 - r0);
    let (pdx, pdy) = (p.0 - c0.0, p.1 - c0.1);
    let a = cdx * cdx + cdy * cdy - dr * dr;
    let b = pdx * cdx + pdy * cdy + r0 * dr;
    let c = pdx * pdx + pdy * pdy - r0 * r0;
    let valid = |t: f32| r0 + t * dr >= 0.0;
    if a.abs() <= f32::EPSILON {
        if b.abs() <= f32::EPSILON {
            return None;
        }
        let t = c / (2.0 * b);
        return valid(t).then_some(t);
    }
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let (t1, t2) = ((b + root) / a, (b - root) / a);
    let (high, low) = if t1 > t2 { (t1, t2) } else { (t2, t1) };
    if valid(high) {
        Some(high)
    } else if valid(low) {
        Some(low)
    } else {
        None
    }
}

impl ColorPainter for Compositor<'_> {
    fn push_transform(&mut self, transform: Transform) {
        let current = self.transform();
        self.transforms.push(current * transform);
    }

    fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: skrifa::GlyphId) {
        let outline = glyph_outline(self.face, glyph_id);
        self.push_clip_mask(|rasterizer, transform| {
            let Some(outline) = outline else { return };
            let map = |p: ab_glyph::Point| {
                let (x, y) = apply(transform, p.x, p.y);
                point(x, y)
            };
            for curve in &outline.curves {
                match *curve {
                    ab_glyph::OutlineCurve::Line(p0, p1) => rasterizer.draw_line(map(p0), map(p1)),
                    ab_glyph::OutlineCurve::Quad(p0, p1, p2) => {
                        rasterizer.draw_quad(map(p0), map(p1), map(p2))
                    }
                    ab_glyph::OutlineCurve::Cubic(p0, p1, p2, p3) => {
                        rasterizer.draw_cubic(map(p0), map(p1), map(p2), map(p3))
                    }
                }
            }
        });
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>) {
        self.push_clip_mask(|rasterizer, transform| {
            let corners = [
                (clip_box.x_min, clip_box.y_min),
                (clip_box.x_max, clip_box.y_min),
                (clip_box.x_max, clip_box.y_max),
                (clip_box.x_min, clip_box.y_max),
            ]
            .map(|(x, y)| {
                let (x, y) = apply(transform, x, y);
                point(x, y)
            });
            for index in 0..4 {
                rasterizer.draw_line(corners[index], corners[(index + 1) % 4]);
            }
        });
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn fill(&mut self, brush: Brush<'_>) {
        let inverse = invert(&self.transform());
        let solid =
            matches!(brush, Brush::Solid { .. }).then(|| self.brush_color(&brush, 0.0, 0.0));
        let width = self.width;
        for index in 0..width * self.height {
            let coverage = self.coverage(index);
            if coverage <= 0.0 {
                continue;
            }
            let color = match (solid, &inverse) {
                (Some(color), _) => color,
                (None, Some(inverse)) => {
                    let (x, y) =
                        apply(inverse, (index % width) as f32 + 0.5, (index / width) as f32 + 0.5);
                    self.brush_color(&brush, x, y)
                }
                (None, None) => continue,
            };
            let source = color.map(|channel| channel * coverage);
            let layer = self.layers.last_mut().unwrap();
            layer[index] = composite(layer[index], source, CompositeMode::SrcOver);
        }
    }

    fn push_layer(&mut self, _composite_mode: CompositeMode) {
        self.layers.push(vec![[0.0; 4]; self.width * self.height]);
    }

    fn pop_layer_with_mode(&mut self, composite_mode: CompositeMode) {
        if self.layers.len() < 2 {
            return;
        }
        let source = self.layers.pop().unwrap();
        let backdrop = self.layers.last_mut().unwrap();
        for (destination, source) in backdrop.iter_mut().zip(source) {
            *destination = composite(*destination, source, composite_mode);
        }
    }
}

impl Compositor<'_> {
    fn into_rgba8(mut self) -> Vec<u8> {
        let layer = self.layers.swap_remove(0);
        layer
            .into_iter()
            .flat_map(|[r, g, b, a]| {
                let alpha = a.clamp(0.0, 1.0);
                let channel = |value: f32| {
                    if alpha <= 0.0 {
                        0
                    } else {
                        ((value / alpha).clamp(0.0, 1.0) * 255.0).round() as u8
                    }
                };
                [channel(r), channel(g), channel(b), (alpha * 255.0).round() as u8]
            })
            .collect()
    }
}

/// W3C Compositing and Blending の Porter-Duff 合成と blend mode（premultiplied）。
fn composite(backdrop: [f32; 4], source: [f32; 4], mode: CompositeMode) -> [f32; 4] {
    let (ab, as_) = (backdrop[3], source[3]);
    let porter_duff = |fa: f32, fb: f32| [0, 1, 2, 3].map(|i| source[i] * fa + backdrop[i] * fb);
    match mode {
        CompositeMode::Clear => [0.0; 4],
        CompositeMode::Src => source,
        CompositeMode::Dest => backdrop,
        CompositeMode::DestOver => porter_duff(1.0 - ab, 1.0),
        CompositeMode::SrcIn => porter_duff(ab, 0.0),
        CompositeMode::DestIn => porter_duff(0.0, as_),
        CompositeMode::SrcOut => porter_duff(1.0 - ab, 0.0),
        CompositeMode::DestOut => porter_duff(0.0, 1.0 - as_),
        CompositeMode::SrcAtop => porter_duff(ab, 1.0 - as_),
        CompositeMode::DestAtop => porter_duff(1.0 - ab, as_),
        CompositeMode::Xor => porter_duff(1.0 - ab, 1.0 - as_),
        CompositeMode::Plus => [0, 1, 2, 3].map(|i| (source[i] + backdrop[i]).min(1.0)),
        CompositeMode::Screen
        | CompositeMode::Overlay
        | CompositeMode::Darken
        | CompositeMode::Lighten
        | CompositeMode::ColorDodge
        | CompositeMode::ColorBurn
        | CompositeMode::HardLight
        | CompositeMode::SoftLight
        | CompositeMode::Difference
        | CompositeMode::Exclusion
        | CompositeMode::Multiply
        | CompositeMode::HslHue
        | CompositeMode::HslSaturation
        | CompositeMode::HslColor
        | CompositeMode::HslLuminosity => blend(backdrop, source, mode),
        _ => porter_duff(1.0, 1.0 - as_),
    }
}

fn blend(backdrop: [f32; 4], source: [f32; 4], mode: CompositeMode) -> [f32; 4] {
    let (ab, as_) = (backdrop[3], source[3]);
    let unpremultiply = |color: [f32; 4]| {
        let alpha = color[3];
        if alpha <= 0.0 { [0.0; 3] } else { [color[0] / alpha, color[1] / alpha, color[2] / alpha] }
    };
    let (cb, cs) = (unpremultiply(backdrop), unpremultiply(source));
    let mixed = match mode {
        CompositeMode::HslHue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        CompositeMode::HslSaturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        CompositeMode::HslColor => set_lum(cs, lum(cb)),
        CompositeMode::HslLuminosity => set_lum(cb, lum(cs)),
        _ => [0, 1, 2].map(|i| separable_blend(cb[i], cs[i], mode)),
    };
    let alpha = as_ + ab - as_ * ab;
    let mut result = [0.0; 4];
    for i in 0..3 {
        result[i] = source[i] * (1.0 - ab) + backdrop[i] * (1.0 - as_) + as_ * ab * mixed[i];
    }
    result[3] = alpha;
    result
}

fn separable_blend(cb: f32, cs: f32, mode: CompositeMode) -> f32 {
    match mode {
        CompositeMode::Multiply => cb * cs,
        CompositeMode::Screen => cb + cs - cb * cs,
        CompositeMode::Overlay => separable_blend(cs, cb, CompositeMode::HardLight),
        CompositeMode::Darken => cb.min(cs),
        CompositeMode::Lighten => cb.max(cs),
        CompositeMode::ColorDodge => {
            if cb <= 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        CompositeMode::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        CompositeMode::HardLight => {
            if cs <= 0.5 {
                cb * 2.0 * cs
            } else {
                separable_blend(cb, 2.0 * cs - 1.0, CompositeMode::Screen)
            }
        }
        CompositeMode::SoftLight => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        CompositeMode::Difference => (cb - cs).abs(),
        CompositeMode::Exclusion => cb + cs - 2.0 * cb * cs,
        _ => cs,
    }
}

fn lum(color: [f32; 3]) -> f32 {
    0.3 * color[0] + 0.59 * color[1] + 0.11 * color[2]
}

fn clip_color(color: [f32; 3]) -> [f32; 3] {
    let l = lum(color);
    let n = color[0].min(color[1]).min(color[2]);
    let x = color[0].max(color[1]).max(color[2]);
    color.map(|c| {
        let mut c = c;
        if n < 0.0 {
            c = l + (c - l) * l / (l - n);
        }
        if x > 1.0 {
            c = l + (c - l) * (1.0 - l) / (x - l);
        }
        c
    })
}

fn set_lum(color: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(color);
    clip_color(color.map(|c| c + d))
}

fn sat(color: [f32; 3]) -> f32 {
    color[0].max(color[1]).max(color[2]) - color[0].min(color[1]).min(color[2])
}

fn set_sat(color: [f32; 3], s: f32) -> [f32; 3] {
    let max = color[0].max(color[1]).max(color[2]);
    let min = color[0].min(color[1]).min(color[2]);
    if max - min <= f32::EPSILON {
        return [0.0; 3];
    }
    color.map(|c| (c - min) * s / (max - min))
}
