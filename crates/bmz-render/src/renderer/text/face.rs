use super::*;

/// スキン文字描画で使うベクターフォント face。
///
/// ab_glyph の `FontArc` に collection index と寸法を添えて保持する。
/// `FontArc::font_data` はTTC全体のバイト列を返すため、shaping や
/// カラーグリフの読み出しでも同じバイト列を共有し、フォントを複製しない。
#[derive(Clone)]
pub(crate) struct TextFace(Arc<TextFaceInner>);

struct TextFaceInner {
    cache_id: Arc<str>,
    font: FontArc,
    index: u32,
    metrics: FaceMetrics,
    /// glyf / CFF / CFF2 の輪郭を持つか。カラー画像だけのフォントは輪郭で描けない。
    outlines: bool,
    /// sbix / CBDT / EBDT のグリフ画像を持つか。空の glyf を併せ持つことがある。
    images: bool,
}

/// ab_glyph と同じ定義の未スケール寸法。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FaceMetrics {
    pub(super) units_per_em: f32,
    pub(super) ascent: f32,
    pub(super) descent: f32,
    pub(super) line_gap: f32,
}

impl FaceMetrics {
    /// ab_glyph の `PxScale` が対応する高さ（ascent - descent）。
    pub(super) fn height(self) -> f32 {
        (self.ascent - self.descent).max(1.0)
    }
}

impl std::fmt::Debug for TextFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextFace")
            .field("cache_id", &self.0.cache_id)
            .field("index", &self.0.index)
            .finish_non_exhaustive()
    }
}

impl TextFace {
    pub(crate) fn new(cache_id: impl Into<Arc<str>>, font: FontArc, index: u32) -> Self {
        let metrics = FaceMetrics {
            units_per_em: font.units_per_em().unwrap_or(1000.0).max(1.0),
            ascent: font.ascent_unscaled(),
            descent: font.descent_unscaled(),
            line_gap: font.line_gap_unscaled(),
        };
        let (outlines, images) = ttf_parser::Face::parse(font.font_data(), index)
            .map(|face| {
                let tables = face.tables();
                (
                    tables.glyf.is_some() || tables.cff.is_some() || tables.cff2.is_some(),
                    tables.sbix.is_some() || tables.cbdt.is_some() || tables.ebdt.is_some(),
                )
            })
            .unwrap_or((false, false));
        Self(Arc::new(TextFaceInner {
            cache_id: cache_id.into(),
            font,
            index,
            metrics,
            outlines,
            images,
        }))
    }

    pub(crate) fn from_vec(
        cache_id: impl Into<Arc<str>>,
        bytes: Vec<u8>,
        index: u32,
    ) -> Result<Self, ab_glyph::InvalidFont> {
        let font = FontVec::try_from_vec_and_index(bytes, index)?;
        Ok(Self::new(cache_id, FontArc::from(font), index))
    }

    pub(crate) fn cache_id(&self) -> &Arc<str> {
        &self.0.cache_id
    }

    pub(crate) fn font(&self) -> &FontArc {
        &self.0.font
    }

    /// TTC/OTC 内の face index。
    pub(crate) fn index(&self) -> u32 {
        self.0.index
    }

    /// フォントファイル全体のバイト列。
    pub(crate) fn data(&self) -> &[u8] {
        self.0.font.font_data()
    }

    pub(super) fn metrics(&self) -> FaceMetrics {
        self.0.metrics
    }

    pub(crate) fn glyph_id(&self, ch: char) -> GlyphId {
        self.0.font.glyph_id(ch)
    }

    /// 輪郭グリフとして描ける文字か。
    pub(crate) fn has_glyph(&self, ch: char) -> bool {
        self.renders_outline(self.glyph_id(ch))
    }

    /// 輪郭グリフとして描けるか。
    ///
    /// 画像フォントの空の輪郭（Apple Color Emoji の glyf 等）は描けないものとして扱う。
    /// 通常のフォントでは空の輪郭も有効なグリフ（空白類）とみなす。
    pub(super) fn renders_outline(&self, glyph: GlyphId) -> bool {
        glyph.0 != 0 && self.0.outlines && (!self.0.images || self.0.font.outline(glyph).is_some())
    }

    /// cmap format 14 の異体字シーケンス（例: U+2764 U+FE0F）のグリフ。
    pub(super) fn variation_glyph(&self, ch: char, selector: char) -> Option<GlyphId> {
        let face = ttf_parser::Face::parse(self.data(), self.index()).ok()?;
        face.glyph_variation_index(ch, selector).map(|glyph| GlyphId(glyph.0))
    }

    /// 複数コードポイントの cluster を GSUB / GPOS / morx でシェーピングする。
    ///
    /// ZWJ・異体字セレクタ等の Default_Ignorable は取り除く。face に無い文字が残る場合は None。
    pub(super) fn shape_cluster(&self, text: &str) -> Option<Vec<FaceShapedGlyph>> {
        let face = rustybuzz::Face::from_slice(self.data(), self.index())?;
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_flags(rustybuzz::BufferFlags::REMOVE_DEFAULT_IGNORABLES);
        buffer.guess_segment_properties();
        let output = rustybuzz::shape(&face, &[], buffer);
        let glyphs: Vec<_> = output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions())
            .map(|(info, position)| FaceShapedGlyph {
                glyph: GlyphId(info.glyph_id as u16),
                x_advance: position.x_advance as f32,
                x_offset: position.x_offset as f32,
                y_offset: position.y_offset as f32,
            })
            .collect();
        (!glyphs.is_empty() && glyphs.iter().all(|glyph| glyph.glyph.0 != 0)).then_some(glyphs)
    }

    pub(super) fn h_advance_unscaled(&self, glyph: GlyphId) -> f32 {
        self.0.font.h_advance_unscaled(glyph)
    }
}

/// シェーピング結果（フォント単位、y は上向き）。
#[derive(Debug, Clone, Copy)]
pub(super) struct FaceShapedGlyph {
    pub(super) glyph: GlyphId,
    pub(super) x_advance: f32,
    pub(super) x_offset: f32,
    pub(super) y_offset: f32,
}
