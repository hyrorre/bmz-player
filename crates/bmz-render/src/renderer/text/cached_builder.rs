use super::*;

pub(super) struct CachedTextFrameBuilder<'a> {
    atlas: &'a mut TextAtlasCache,
    pub(super) quads: Vec<TextQuad>,
}

/// 1行ぶんの描画位置。`line_y` はベクターでは基準線、ビットマップでは文字上端。
#[derive(Clone, Copy)]
struct GlyphLine {
    pen_x: f32,
    line_y: f32,
    scale: PxScale,
    color: Color,
    surface: SurfaceSize,
    /// 影・縁取りの描画。カラーグリフも文字色で塗ったシルエットにする。
    decoration: bool,
}

/// cluster のグリフ番号が指すフォント。
#[derive(Clone, Copy)]
struct GlyphFonts<'a> {
    faces: FaceList<'a>,
    bitmaps: &'a [BitmapFontEntry],
}

impl<'a> From<VectorFontSet<'a>> for GlyphFonts<'a> {
    fn from(fonts: VectorFontSet<'a>) -> Self {
        Self { faces: fonts.faces, bitmaps: &[] }
    }
}

impl<'a> From<BitmapFontSet<'a>> for GlyphFonts<'a> {
    fn from(fonts: BitmapFontSet<'a>) -> Self {
        Self { faces: fonts.faces, bitmaps: fonts.fonts }
    }
}

impl<'a> CachedTextFrameBuilder<'a> {
    pub(super) fn new(atlas: &'a mut TextAtlasCache) -> Self {
        Self { atlas, quads: Vec::new() }
    }

    pub(super) fn shaped_vector_text(
        &mut self,
        text: &str,
        fonts: VectorFontSet<'_>,
    ) -> Arc<ShapedText> {
        self.atlas.shapes.vector(text, fonts)
    }

    pub(super) fn shaped_bitmap_text(
        &mut self,
        text: &str,
        fonts: BitmapFontSet<'_>,
    ) -> Arc<ShapedText> {
        self.atlas.shapes.bitmap(text, fonts)
    }

    pub(super) fn push_bitmap_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        fonts: BitmapFontSet<'_>,
        surface: SurfaceSize,
    ) {
        self.push_bitmap_text_pass(origin, text, style, fonts, surface, false);
    }

    fn push_bitmap_text_pass(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        fonts: BitmapFontSet<'_>,
        surface: SurfaceSize,
        decoration: bool,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style.clone();
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_bitmap_text_pass(&shadow_origin, text, shadow_style, fonts, surface, true);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style.clone();
            outline_style.color = outline.color;
            outline_style.outline = None;
            outline_style.shadow = None;
            let outline_y = outline.width;
            let outline_x = outline.width * surface.height as f32 / surface.width as f32;
            for (offset_x, offset_y) in [
                (-outline_x, -outline_y),
                (0.0, -outline_y),
                (outline_x, -outline_y),
                (-outline_x, 0.0),
                (outline_x, 0.0),
                (-outline_x, outline_y),
                (0.0, outline_y),
                (outline_x, outline_y),
            ] {
                let outline_origin = Point { x: origin.x + offset_x, y: origin.y + offset_y };
                self.push_bitmap_text_pass(
                    &outline_origin,
                    text,
                    outline_style.clone(),
                    fonts,
                    surface,
                    true,
                );
            }
        }

        let layout_key = text_layout_key(
            TextGlyphKind::Bitmap,
            fonts.id,
            origin,
            text,
            &style,
            surface,
            decoration,
        );
        if let Some(cached) = layout_key.as_ref().and_then(|key| self.atlas.cached_layout(key)) {
            self.quads.extend_from_slice(cached);
            return;
        }
        let quads_before = self.quads.len();

        let shaped = self.shaped_bitmap_text(text, fonts);
        let design_size = fonts.design_size();
        let bitmap_size = style.bitmap_size.unwrap_or(style.size);
        let mut scale =
            PxScale::from((bitmap_size * surface.height as f32 / design_size).max(0.01));
        let original_scale = scale;
        let mut clusters = &shaped.clusters[..];
        let text_width = clusters_width_px(clusters, scale.x);
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        if max_width > 0.0 && text_width > max_width {
            match style.overflow {
                TextOverflow::Overflow => {}
                TextOverflow::Shrink => {
                    scale.x = (scale.x * max_width / text_width).max(0.01);
                }
                TextOverflow::ShrinkUniform => {
                    scale = PxScale::from((scale.x * max_width / text_width).max(0.01));
                }
                TextOverflow::Truncate => {
                    clusters = &clusters[..truncated_cluster_count(clusters, scale.x, max_width)];
                }
            }
        }
        let text_width = clusters_width_px(clusters, scale.x);
        let align_offset = text_align_offset_px(style.align, max_width, text_width);
        let shrink_offset_y = if matches!(style.overflow, TextOverflow::ShrinkUniform)
            && scale.y < original_scale.y
        {
            (design_size * (original_scale.y - scale.y)) / 2.0
        } else {
            0.0
        };
        let line = GlyphLine {
            pen_x: origin.x * surface.width as f32 + align_offset,
            line_y: origin.y * surface.height as f32 + shrink_offset_y,
            scale,
            color: style.color,
            surface,
            decoration,
        };
        self.push_clusters(&shaped, clusters, line, fonts.into());

        if let Some(key) = layout_key {
            self.atlas.insert_layout(key, &self.quads[quads_before..]);
        }
    }

    pub(super) fn push_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        fonts: VectorFontSet<'_>,
        surface: SurfaceSize,
    ) {
        self.push_text_pass(origin, text, style, fonts, surface, false);
    }

    fn push_text_pass(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        fonts: VectorFontSet<'_>,
        surface: SurfaceSize,
        decoration: bool,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style.clone();
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_text_pass(&shadow_origin, text, shadow_style, fonts, surface, true);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style.clone();
            outline_style.color = outline.color;
            outline_style.outline = None;
            outline_style.shadow = None;
            let outline_y = outline.width;
            let outline_x = outline.width * surface.height as f32 / surface.width as f32;
            for (offset_x, offset_y) in [
                (-outline_x, -outline_y),
                (0.0, -outline_y),
                (outline_x, -outline_y),
                (-outline_x, 0.0),
                (outline_x, 0.0),
                (-outline_x, outline_y),
                (0.0, outline_y),
                (outline_x, outline_y),
            ] {
                let outline_origin = Point { x: origin.x + offset_x, y: origin.y + offset_y };
                self.push_text_pass(
                    &outline_origin,
                    text,
                    outline_style.clone(),
                    fonts,
                    surface,
                    true,
                );
            }
        }

        let layout_key = text_layout_key(
            TextGlyphKind::Vector,
            fonts.id,
            origin,
            text,
            &style,
            surface,
            decoration,
        );
        if let Some(cached) = layout_key.as_ref().and_then(|key| self.atlas.cached_layout(key)) {
            self.quads.extend_from_slice(cached);
            return;
        }
        let quads_before = self.quads.len();

        let Some(primary) = fonts.primary() else {
            return;
        };
        let shaped = self.shaped_vector_text(text, fonts);
        let px_size = (style.size * surface.height as f32).max(1.0);
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let mut scale = PxScale::from(px_size);
        let original_scale = scale;
        let mut scaled_primary = primary.font().as_scaled(scale);
        let mut clusters = &shaped.clusters[..];
        let mut text_width = clusters_width_px(clusters, scale.x);
        if style.wrapping && max_width > 0.0 {
            let line_height = (scaled_primary.ascent() - scaled_primary.descent()
                + scaled_primary.line_gap())
            .max(px_size);
            let origin_x = origin.x * surface.width as f32;
            let first_baseline_y = origin.y * surface.height as f32 + scaled_primary.ascent();
            for (index, line) in wrap_clusters(clusters, scale.x, max_width).into_iter().enumerate()
            {
                let clusters = &shaped.clusters[line];
                let line_width = clusters_width_px(clusters, scale.x);
                let align_offset = text_align_offset_px(style.align, max_width, line_width);
                let line = GlyphLine {
                    pen_x: origin_x + align_offset,
                    line_y: first_baseline_y + line_height * index as f32,
                    scale,
                    color: style.color,
                    surface,
                    decoration,
                };
                self.push_clusters(&shaped, clusters, line, fonts.into());
            }
            return;
        }
        if max_width > 0.0 && text_width > max_width {
            match style.overflow {
                TextOverflow::Overflow => {}
                TextOverflow::Shrink => {
                    scale.x = (scale.x * max_width / text_width).max(0.01);
                    scaled_primary = primary.font().as_scaled(scale);
                    text_width = clusters_width_px(clusters, scale.x);
                }
                TextOverflow::ShrinkUniform => {
                    scale = PxScale::from((scale.x * max_width / text_width).max(1.0));
                    scaled_primary = primary.font().as_scaled(scale);
                    text_width = clusters_width_px(clusters, scale.x);
                }
                TextOverflow::Truncate => {
                    clusters = &clusters[..truncated_cluster_count(clusters, scale.x, max_width)];
                    text_width = clusters_width_px(clusters, scale.x);
                }
            }
        }
        let shrink_offset_y = if matches!(style.overflow, TextOverflow::ShrinkUniform)
            && scale.y < original_scale.y
        {
            (original_scale.y - scale.y) / 2.0
        } else {
            0.0
        };
        let align_offset = text_align_offset_px(style.align, max_width, text_width);
        let line = GlyphLine {
            pen_x: origin.x * surface.width as f32 + align_offset,
            line_y: origin.y * surface.height as f32 + shrink_offset_y + scaled_primary.ascent(),
            scale,
            color: style.color,
            surface,
            decoration,
        };
        self.push_clusters(&shaped, clusters, line, fonts.into());

        if let Some(key) = layout_key {
            self.atlas.insert_layout(key, &self.quads[quads_before..]);
        }
    }

    fn push_clusters(
        &mut self,
        shaped: &ShapedText,
        clusters: &[ShapedCluster],
        mut line: GlyphLine,
        fonts: GlyphFonts<'_>,
    ) {
        for cluster in clusters {
            for glyph in shaped.glyphs(cluster) {
                self.push_glyph(glyph, line, fonts);
            }
            line.pen_x += cluster.advance * line.scale.x;
        }
    }

    fn push_glyph(&mut self, glyph: &ShapedGlyph, line: GlyphLine, fonts: GlyphFonts<'_>) {
        let x = line.pen_x + glyph.x * line.scale.x;
        let mut y = line.line_y + glyph.y * line.scale.y;
        let mut quad_color = line.color;
        let cached = match glyph.source {
            GlyphSource::Vector { face, glyph, face_scale, color } => {
                let Some(face) = fonts.faces.get(face) else { return };
                let scale = PxScale { x: line.scale.x * face_scale, y: line.scale.y * face_scale };
                let color_glyph = || {
                    // カラーグリフはフォントの色を保ち、スキンの透明度だけを掛ける。
                    // 影・縁取りでは輪郭と同じく指定色のシルエットにする。
                    let tint = (!line.decoration).then(|| Color::rgba(1.0, 1.0, 1.0, line.color.a));
                    (GlyphPaint::Color, tint)
                };
                let paints = if color {
                    [color_glyph(), (GlyphPaint::Outline, None)]
                } else {
                    [(GlyphPaint::Outline, None), color_glyph()]
                };
                // 解決時に選んだ形式で描けなければ、もう一方の形式、最後に欠字枠で描く。
                let drawn = paints.into_iter().find_map(|(paint, tint)| {
                    let cached = match paint {
                        GlyphPaint::Color => {
                            self.atlas.cached_color_glyph(face, glyph, scale, line.decoration)
                        }
                        GlyphPaint::Outline => self.atlas.cached_vector_glyph(face, glyph, scale),
                    }?;
                    Some((cached, tint))
                });
                match drawn {
                    Some((cached, tint)) => {
                        quad_color = tint.unwrap_or(quad_color);
                        Some(cached)
                    }
                    None => {
                        let metrics = face.metrics();
                        let em = scale.y * metrics.units_per_em / metrics.height();
                        let height = em * TOFU_HEIGHT_EM;
                        y -= height;
                        self.atlas.cached_tofu_glyph(height * TOFU_WIDTH_RATIO, height)
                    }
                }
            }
            GlyphSource::Bitmap { font, ch } => {
                let Some(entry) = fonts.bitmaps.get(usize::from(font)) else { return };
                let Some(bitmap_glyph) = entry.font.glyphs.get(&ch) else { return };
                if bitmap_glyph.width == 0 || bitmap_glyph.height == 0 {
                    return;
                }
                let Some(page) = entry.font.pages.get(&bitmap_glyph.page) else { return };
                // fallback .fnt も自身の ascent で大文字上端を揃え、主フォントと同じ倍率で描く。
                Some(self.atlas.cached_bitmap_glyph(
                    entry.id.clone(),
                    ch,
                    *bitmap_glyph,
                    page,
                    &entry.font,
                    line.scale,
                ))
            }
            GlyphSource::Tofu { width, height } => {
                self.atlas.cached_tofu_glyph(width * line.scale.x, height * line.scale.y)
            }
        };
        let Some(cached) = cached else { return };
        let surface = line.surface;
        self.quads.push(TextQuad {
            x: (x + cached.offset_x) / surface.width as f32,
            y: (y + cached.offset_y) / surface.height as f32,
            width: cached.display_width / surface.width as f32,
            height: cached.display_height / surface.height as f32,
            atlas_origin: cached.atlas_origin,
            glyph_width: cached.width,
            glyph_height: cached.height,
            color: quad_color,
        });
    }

    /// 解決済みグリフを1行に並べる（描画 fallback のテスト用）。
    #[cfg(test)]
    pub(super) fn push_resolved_glyphs(
        &mut self,
        glyphs: &[ShapedGlyph],
        fonts: VectorFontSet<'_>,
        surface: SurfaceSize,
    ) {
        let line = GlyphLine {
            pen_x: 10.0,
            line_y: 50.0,
            scale: PxScale::from(20.0),
            color: Color::rgb(1.0, 1.0, 1.0),
            surface,
            decoration: false,
        };
        for glyph in glyphs {
            self.push_glyph(glyph, line, fonts.into());
        }
    }

    pub(super) fn finish(
        self,
        command_quad_counts: Vec<usize>,
        command_caret_rects: Vec<Option<RectCommand>>,
    ) -> TextFrame {
        let size = self.atlas.size();
        let instances = encode_text_quads(&self.quads, size.width, size.height);
        TextFrame {
            size,
            #[cfg(test)]
            pixels: Vec::new(),
            dirty_regions: std::mem::take(&mut self.atlas.dirty_regions),
            instances,
            command_quad_counts,
            command_caret_rects,
        }
    }
}
