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
        Self(Arc::new(TextFaceInner { cache_id: cache_id.into(), font, index, metrics }))
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
    #[allow(dead_code)]
    pub(crate) fn index(&self) -> u32 {
        self.0.index
    }

    /// フォントファイル全体のバイト列。
    #[allow(dead_code)]
    pub(crate) fn data(&self) -> &[u8] {
        self.0.font.font_data()
    }

    pub(super) fn metrics(&self) -> FaceMetrics {
        self.0.metrics
    }

    pub(crate) fn glyph_id(&self, ch: char) -> GlyphId {
        self.0.font.glyph_id(ch)
    }

    pub(crate) fn has_glyph(&self, ch: char) -> bool {
        self.glyph_id(ch).0 != 0
    }

    pub(super) fn h_advance_unscaled(&self, glyph: GlyphId) -> f32 {
        self.0.font.h_advance_unscaled(glyph)
    }
}
