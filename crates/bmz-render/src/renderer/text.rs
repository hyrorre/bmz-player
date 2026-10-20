use super::*;

/// 未指定テキスト用の既定フォント列（CJK coverage 順）。
#[derive(Clone, Default)]
pub(super) struct FontFallbackChain {
    pub(super) faces: Vec<TextFace>,
}

impl FontFallbackChain {
    #[cfg(test)]
    pub(super) fn primary(&self) -> Option<&TextFace> {
        self.faces.first()
    }

    #[cfg(test)]
    pub(super) fn select(&self, ch: char) -> Option<&TextFace> {
        self.faces.iter().find(|face| face.has_glyph(ch))
    }

    #[cfg(test)]
    fn single(font: TextFace) -> Self {
        Self { faces: vec![font] }
    }
}

/// 1つのテキスト描画で使うベクターフォントの優先順。
///
/// `id` は解決結果・レイアウトのキャッシュキーで、フォント構成が変わると
/// atlas ごとキャッシュを作り直す。先頭 face の寸法を基準に他の face の em を揃える。
#[derive(Clone, Copy)]
pub(super) struct VectorFontSet<'a> {
    pub(super) id: &'a str,
    pub(super) faces: &'a [TextFace],
}

impl<'a> VectorFontSet<'a> {
    pub(super) fn primary(self) -> Option<&'a TextFace> {
        self.faces.first()
    }

    pub(super) fn face(self, index: u16) -> Option<&'a TextFace> {
        self.faces.get(usize::from(index))
    }

    pub(super) fn find_face(
        self,
        mut predicate: impl FnMut(&TextFace) -> bool,
    ) -> Option<(u16, &'a TextFace)> {
        self.faces
            .iter()
            .enumerate()
            .take(usize::from(u16::MAX))
            .find(|(_, face)| predicate(face))
            .map(|(index, face)| (index as u16, face))
    }

    /// face の1フォント単位あたりの設計単位。
    ///
    /// 先頭 face は ab_glyph の `PxScale`（ascent - descent が1）と一致させ、
    /// 従来の描画寸法を保つ。他の face は先頭 face と em の大きさを揃える。
    pub(super) fn units_scale(self, index: u16) -> f32 {
        let Some(primary) = self.primary() else {
            return 0.0;
        };
        let primary = primary.metrics();
        if index == 0 {
            return 1.0 / primary.height();
        }
        let face = self.face(index).map_or(primary, TextFace::metrics);
        primary.units_per_em / primary.height() / face.units_per_em
    }

    /// 先頭 face の1em（設計単位）。
    pub(super) fn em(self) -> f32 {
        self.primary()
            .map(|face| face.metrics().units_per_em / face.metrics().height())
            .unwrap_or(1.0)
    }
}

/// 1つのテキスト描画で使うビットマップフォント。
#[derive(Clone, Copy)]
pub(super) struct BitmapFontSet<'a> {
    pub(super) id: &'a str,
    pub(super) primary: &'a BitmapFont,
}

impl<'a> BitmapFontSet<'a> {
    pub(super) fn font(self, index: u16) -> Option<(&'a str, &'a BitmapFont)> {
        (index == 0).then_some((self.id, self.primary))
    }
}

pub(super) fn build_text_frame_with_fallback_cache(
    plan: &DrawPlan,
    default_fonts: &FontFallbackChain,
    fonts: &HashMap<String, TextFace>,
    bitmap_fonts: &HashMap<String, BitmapFont>,
    surface: SurfaceSize,
    atlas: &mut TextAtlasCache,
) -> TextFrame {
    if !surface.is_drawable() {
        return TextFrame::default();
    }

    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(atlas);
    let mut command_quad_counts = Vec::new();
    let mut command_caret_rects = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Text { origin, text, style, caret, post_scale } = command else {
            continue;
        };
        let quads_before = builder.quads.len();
        if let Some(font_id) = style.font_id.as_deref()
            && let Some(bitmap_font) = bitmap_fonts.get(font_id)
        {
            let fonts = BitmapFontSet { id: font_id, primary: bitmap_font };
            builder.push_bitmap_text(origin, text, style.clone(), fonts, surface);
            scale_text_quads(&mut builder.quads[quads_before..], *origin, *post_scale);
            command_caret_rects.push(
                caret
                    .and_then(|caret| {
                        let shaped = builder.shaped_bitmap_text(text, fonts);
                        bitmap_text_caret_rect(origin, &shaped, style, fonts, surface, caret)
                    })
                    .map(|caret| scale_text_rect_command(caret, *origin, *post_scale)),
            );
        } else {
            let vector_fonts = style
                .font_id
                .as_deref()
                .and_then(|font_id| {
                    fonts
                        .get_key_value(font_id)
                        .map(|(id, face)| VectorFontSet { id, faces: std::slice::from_ref(face) })
                })
                .unwrap_or(VectorFontSet { id: DEFAULT_TEXT_FONT_ID, faces: &default_fonts.faces });
            builder.push_text(origin, text, style.clone(), vector_fonts, surface);
            scale_text_quads(&mut builder.quads[quads_before..], *origin, *post_scale);
            command_caret_rects.push(
                caret
                    .and_then(|caret| {
                        let shaped = builder.shaped_vector_text(text, vector_fonts);
                        vector_text_caret_rect(origin, &shaped, style, vector_fonts, surface, caret)
                    })
                    .map(|caret| scale_text_rect_command(caret, *origin, *post_scale)),
            );
        }
        command_quad_counts.push(builder.quads.len() - quads_before);
    }
    builder.finish(command_quad_counts, command_caret_rects)
}

pub(super) fn scale_text_quads(quads: &mut [TextQuad], origin: Point, scale: Point) {
    if scale == (Point { x: 1.0, y: 1.0 }) {
        return;
    }
    for quad in quads {
        quad.x = origin.x + (quad.x - origin.x) * scale.x;
        quad.y = origin.y + (quad.y - origin.y) * scale.y;
        quad.width *= scale.x;
        quad.height *= scale.y;
    }
}

pub(super) fn scale_text_rect_command(
    mut command: RectCommand,
    origin: Point,
    scale: Point,
) -> RectCommand {
    command.rect.x = origin.x + (command.rect.x - origin.x) * scale.x;
    command.rect.y = origin.y + (command.rect.y - origin.y) * scale.y;
    command.rect.width *= scale.x;
    command.rect.height *= scale.y;
    command
}

#[cfg(test)]
pub(super) fn build_text_frame_with_cache(
    plan: &DrawPlan,
    default_font: &TextFace,
    fonts: &HashMap<String, TextFace>,
    bitmap_fonts: &HashMap<String, BitmapFont>,
    surface: SurfaceSize,
    atlas: &mut TextAtlasCache,
) -> TextFrame {
    build_text_frame_with_fallback_cache(
        plan,
        &FontFallbackChain::single(default_font.clone()),
        fonts,
        bitmap_fonts,
        surface,
        atlas,
    )
}

#[derive(Debug, Clone)]
pub(super) struct TextQuad {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) atlas_origin: (u32, u32),
    pub(super) glyph_width: u32,
    pub(super) glyph_height: u32,
    pub(super) color: Color,
}

pub(super) const DEFAULT_TEXT_FONT_ID: &str = "<default>";

/// atlas 内のグリフ。ベクターはグリフID、ビットマップは文字、生成矩形は寸法で識別する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TextGlyphKey {
    pub(super) kind: TextGlyphKind,
    pub(super) font_id: Arc<str>,
    pub(super) glyph: u32,
    scale_x_bits: u32,
    scale_y_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TextGlyphKind {
    Vector,
    Bitmap,
    Tofu,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TextLayoutKey {
    kind: TextGlyphKind,
    pub(super) font_id: String,
    pub(super) text: String,
    origin_x_bits: u32,
    origin_y_bits: u32,
    surface_width: u32,
    surface_height: u32,
    style: TextLayoutStyleKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TextLayoutStyleKey {
    size_bits: u32,
    bitmap_size_bits: Option<u32>,
    color: ColorKey,
    align: u8,
    max_width_bits: u32,
    overflow: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ColorKey {
    r_bits: u32,
    g_bits: u32,
    b_bits: u32,
    a_bits: u32,
}

#[derive(Debug, Clone)]
pub(super) struct CachedGlyph {
    pub(super) atlas_origin: (u32, u32),
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) display_width: f32,
    pub(super) display_height: f32,
    pub(super) offset_x: f32,
    pub(super) offset_y: f32,
}

#[derive(Debug)]
pub(super) struct TextLayoutCache {
    pub(super) entries: HashMap<TextLayoutKey, Vec<TextQuad>>,
}

impl TextLayoutCache {
    fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn cached(&self, key: &TextLayoutKey) -> Option<&[TextQuad]> {
        self.entries.get(key).map(Vec::as_slice)
    }

    fn insert(&mut self, key: TextLayoutKey, quads: &[TextQuad]) {
        if self.entries.len() >= TEXT_LAYOUT_CACHE_MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, quads.to_vec());
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug)]
pub(super) struct TextAtlasCache {
    width: u32,
    pub(super) pen_x: u32,
    pub(super) pen_y: u32,
    row_height: u32,
    pixels: Vec<u8>,
    pub(super) glyphs: HashMap<TextGlyphKey, CachedGlyph>,
    pub(super) layouts: TextLayoutCache,
    /// 欠字・fallback の解決結果。atlas 画素とは独立なので高さ上限のリセットでは残す。
    pub(super) shapes: TextShapeCache,
    pub(super) dirty_regions: Vec<TextAtlasDirtyRegion>,
}

impl TextAtlasCache {
    pub(super) fn new(width: u32) -> Self {
        Self {
            width,
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            pixels: Vec::new(),
            glyphs: HashMap::new(),
            layouts: TextLayoutCache::new(),
            shapes: TextShapeCache::default(),
            dirty_regions: Vec::new(),
        }
    }

    pub(super) fn begin_frame(&mut self) {
        self.dirty_regions.clear();
        // アトラス高さが上限に達したらキャッシュを捨てて作り直す。フレーム描画前に
        // 行うので、このフレームのグリフは新しいアトラスへ再ラスタライズされる。
        if self.atlas_height() >= TEXT_ATLAS_MAX_HEIGHT {
            self.clear();
        }
    }

    fn clear(&mut self) {
        self.pen_x = 0;
        self.pen_y = 0;
        self.row_height = 0;
        self.pixels.clear();
        self.glyphs.clear();
        self.layouts.clear();
        self.dirty_regions.clear();
    }

    pub(super) fn atlas_height(&self) -> u32 {
        (self.pen_y + self.row_height).max(1)
    }

    pub(super) fn size(&self) -> AtlasSize {
        AtlasSize { width: self.width, height: self.atlas_height() }
    }

    pub(super) fn pixels_for_size(&self, size: AtlasSize) -> Vec<u8> {
        let mut pixels = self.pixels.clone();
        pixels.resize((size.width * size.height * 4) as usize, 0);
        pixels
    }

    pub(super) fn cached_layout(&self, key: &TextLayoutKey) -> Option<&[TextQuad]> {
        self.layouts.cached(key)
    }

    pub(super) fn insert_layout(&mut self, key: TextLayoutKey, quads: &[TextQuad]) {
        self.layouts.insert(key, quads);
    }

    pub(super) fn cached_vector_glyph(
        &mut self,
        face: &TextFace,
        glyph_id: GlyphId,
        scale: PxScale,
    ) -> Option<CachedGlyph> {
        let key = TextGlyphKey {
            kind: TextGlyphKind::Vector,
            font_id: face.cache_id().clone(),
            glyph: u32::from(glyph_id.0),
            scale_x_bits: scale.x.to_bits(),
            scale_y_bits: scale.y.to_bits(),
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }

        let font = face.font();
        let raster_scale = PxScale {
            x: scale.x * VECTOR_TEXT_SUPERSAMPLE_SCALE,
            y: scale.y * VECTOR_TEXT_SUPERSAMPLE_SCALE,
        };
        let scaled_font = font.as_scaled(raster_scale);
        let baseline_y = scaled_font.ascent();
        let glyph = Glyph { id: glyph_id, scale: raster_scale, position: point(0.0, baseline_y) };
        let outlined = font.outline_glyph(glyph)?;
        let bounds = outlined.px_bounds();
        let width = bounds.width().ceil().max(0.0) as u32;
        let height = bounds.height().ceil().max(0.0) as u32;
        if width == 0 || height == 0 {
            return None;
        }

        let mut pixels = vec![0; (width * height * 4) as usize];
        outlined.draw(|x, y, coverage| {
            let index = ((y * width + x) * 4) as usize;
            let coverage_u8 = (coverage * 255.0).clamp(0.0, 255.0) as u8;
            if let Some(pixel) = pixels.get_mut(index..index + 4)
                && coverage_u8 >= pixel[3]
            {
                pixel[0] = 255;
                pixel[1] = 255;
                pixel[2] = 255;
                pixel[3] = coverage_u8;
            }
        });

        Some(self.insert_glyph_pixels(
            key,
            width,
            height,
            width as f32 / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            height as f32 / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            bounds.min.x / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            (bounds.min.y - baseline_y) / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            pixels,
        ))
    }

    /// 代替グリフが無い欠字の枠。`width` / `height` は表示px。
    pub(super) fn cached_tofu_glyph(&mut self, width: f32, height: f32) -> Option<CachedGlyph> {
        if !(width > 0.0 && height > 0.0) {
            return None;
        }
        let key = TextGlyphKey {
            kind: TextGlyphKind::Tofu,
            font_id: Arc::from(""),
            glyph: 0,
            scale_x_bits: width.to_bits(),
            scale_y_bits: height.to_bits(),
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }
        let pixel_width = (width * VECTOR_TEXT_SUPERSAMPLE_SCALE).round().max(2.0) as u32;
        let pixel_height = (height * VECTOR_TEXT_SUPERSAMPLE_SCALE).round().max(2.0) as u32;
        let pixels = tofu_pixels(pixel_width, pixel_height);
        Some(self.insert_glyph_pixels(
            key,
            pixel_width,
            pixel_height,
            width,
            height,
            0.0,
            0.0,
            pixels,
        ))
    }

    pub(super) fn cached_bitmap_glyph(
        &mut self,
        font_id: Arc<str>,
        ch: char,
        glyph: crate::bitmap_font::BitmapFontGlyph,
        page: &crate::bitmap_font::BitmapFontPage,
        font: &BitmapFont,
        scale: PxScale,
    ) -> CachedGlyph {
        let key = TextGlyphKey {
            kind: TextGlyphKind::Bitmap,
            font_id,
            glyph: u32::from(ch),
            scale_x_bits: scale.x.to_bits(),
            scale_y_bits: scale.y.to_bits(),
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return glyph.clone();
        }

        let width = (glyph.width as f32 * scale.x).ceil().max(1.0) as u32;
        let height = (glyph.height as f32 * scale.y).ceil().max(1.0) as u32;
        let pixels = rasterized_bitmap_glyph_pixels(glyph, page, scale, width, height);

        self.insert_glyph_pixels(
            key,
            width,
            height,
            width as f32,
            height as f32,
            glyph.xoffset as f32 * scale.x,
            (glyph.yoffset as f32 - font.ascent) * scale.y,
            pixels,
        )
    }

    fn insert_glyph_pixels(
        &mut self,
        key: TextGlyphKey,
        width: u32,
        height: u32,
        display_width: f32,
        display_height: f32,
        offset_x: f32,
        offset_y: f32,
        pixels: Vec<u8>,
    ) -> CachedGlyph {
        let atlas_origin = self.reserve(width, height);
        self.blit_region(atlas_origin, width, height, &pixels);
        self.dirty_regions.push(TextAtlasDirtyRegion {
            origin: atlas_origin,
            size: AtlasSize { width, height },
            pixels,
        });
        let cached = CachedGlyph {
            atlas_origin,
            width,
            height,
            display_width,
            display_height,
            offset_x,
            offset_y,
        };
        self.glyphs.insert(key, cached.clone());
        cached
    }

    pub(super) fn reserve(&mut self, glyph_width: u32, glyph_height: u32) -> (u32, u32) {
        let padded_width = glyph_width + TEXT_ATLAS_PADDING * 2;
        let padded_height = glyph_height + TEXT_ATLAS_PADDING * 2;
        if self.pen_x + padded_width > self.width {
            self.pen_x = 0;
            self.pen_y += self.row_height;
            self.row_height = 0;
        }

        let origin = (self.pen_x + TEXT_ATLAS_PADDING, self.pen_y + TEXT_ATLAS_PADDING);
        self.pen_x += padded_width;
        self.row_height = self.row_height.max(padded_height);
        self.ensure_height(self.pen_y + self.row_height);
        origin
    }

    fn ensure_height(&mut self, height: u32) {
        let needed = (self.width * height * 4) as usize;
        if self.pixels.len() < needed {
            self.pixels.resize(needed, 0);
        }
    }

    fn blit_region(&mut self, atlas_origin: (u32, u32), width: u32, height: u32, pixels: &[u8]) {
        for y in 0..height {
            let dst_start = (((atlas_origin.1 + y) * self.width + atlas_origin.0) * 4) as usize;
            let src_start = (y * width * 4) as usize;
            let len = (width * 4) as usize;
            if let (Some(dst), Some(src)) = (
                self.pixels.get_mut(dst_start..dst_start + len),
                pixels.get(src_start..src_start + len),
            ) {
                dst.copy_from_slice(src);
            }
        }
    }
}

/// 線幅が高さの約1/12（表示上最低1px）の白い枠。文字色・透明度は描画時に乗算する。
fn tofu_pixels(width: u32, height: u32) -> Vec<u8> {
    let stroke = ((height as f32 / 12.0).round() as u32)
        .max(VECTOR_TEXT_SUPERSAMPLE_SCALE as u32)
        .min((width.min(height) / 2).max(1));
    let mut pixels = vec![0; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            if x < stroke || y < stroke || x + stroke >= width || y + stroke >= height {
                let index = ((y * width + x) * 4) as usize;
                pixels[index..index + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    pixels
}

pub(super) fn text_layout_key(
    kind: TextGlyphKind,
    font_id: &str,
    origin: &Point,
    text: &str,
    style: &TextStyle,
    surface: SurfaceSize,
) -> Option<TextLayoutKey> {
    if style.wrapping || style.outline.is_some() || style.shadow.is_some() {
        return None;
    }
    Some(TextLayoutKey {
        kind,
        font_id: font_id.to_string(),
        text: text.to_string(),
        origin_x_bits: origin.x.to_bits(),
        origin_y_bits: origin.y.to_bits(),
        surface_width: surface.width,
        surface_height: surface.height,
        style: TextLayoutStyleKey {
            size_bits: style.size.to_bits(),
            bitmap_size_bits: style.bitmap_size.map(f32::to_bits),
            color: ColorKey {
                r_bits: style.color.r.to_bits(),
                g_bits: style.color.g.to_bits(),
                b_bits: style.color.b.to_bits(),
                a_bits: style.color.a.to_bits(),
            },
            align: text_align_key(style.align),
            max_width_bits: style.max_width.to_bits(),
            overflow: text_overflow_key(style.overflow),
        },
    })
}

pub(super) fn text_align_key(align: TextAlign) -> u8 {
    match align {
        TextAlign::Left => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    }
}

pub(super) fn text_overflow_key(overflow: TextOverflow) -> u8 {
    match overflow {
        TextOverflow::Overflow => 0,
        TextOverflow::Shrink => 1,
        TextOverflow::Truncate => 2,
        TextOverflow::ShrinkUniform => 3,
    }
}
