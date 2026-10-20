use super::*;

/// beatoraja の `MISSING_GLYPH_CANDIDATES` と同じ欠字代替候補（□, ▢, ☐, ■, ?）。
pub(super) const MISSING_GLYPH_CANDIDATES: [char; 5] =
    ['\u{25A1}', '\u{25A2}', '\u{2610}', '\u{25A0}', '?'];

const IDEOGRAPHIC_SPACE: char = '\u{3000}';
const WAVE_DASH: char = '\u{301C}';
const FULLWIDTH_TILDE: char = '\u{FF5E}';

/// 代替グリフも無いときに生成する欠字矩形の高さ（em比）と縦横比。
const TOFU_HEIGHT_EM: f32 = 0.8;
const TOFU_WIDTH_RATIO: f32 = 0.75;
/// beatoraja の生成矩形と同じく、線幅は高さの1/12とする。
const TOFU_STROKE_RATIO: f32 = 1.0 / 12.0;

/// フォントセットごとに一度だけ解決した文字列。
///
/// 幅計算・Truncate・Wrapping・caret・描画が同じ欠字解決結果を共有するため、
/// 文字単位の処理はこの cluster 列だけを参照する。寸法はスケール1の設計単位で、
/// 描画時に `PxScale` を掛けて px にする。
#[derive(Debug, Default)]
pub(super) struct ShapedText {
    pub(super) clusters: Vec<ShapedCluster>,
    glyphs: Vec<ShapedGlyph>,
}

/// 1つの表示単位。Truncate・Wrapping・caret はこの単位を分割しない。
#[derive(Debug, Clone)]
pub(super) struct ShapedCluster {
    /// 元文字列中のバイト範囲。
    pub(super) text: Range<usize>,
    glyphs: Range<usize>,
    /// 後続 cluster までの送り幅（設計単位、`scale.x` 倍で px）。
    pub(super) advance: f32,
    /// 改行。Wrapping では行を区切り、単一行描画では幅を持たない。
    pub(super) newline: bool,
}

/// cluster 内の1グリフ。`x` / `y` は cluster 先頭の pen 位置と行基準線からの設計単位。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShapedGlyph {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) source: GlyphSource,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum GlyphSource {
    /// フォントセット内 `face` 番目のグリフ。`face_scale` は face 固有の `PxScale` 倍率。
    Vector { face: u16, glyph: GlyphId, face_scale: f32 },
    /// ビットマップフォントセット内 `font` 番目のグリフ。
    Bitmap { font: u16, ch: char },
    /// 代替グリフも無いときに描画側で生成する欠字矩形（設計単位）。
    Tofu { width: f32, height: f32 },
}

impl ShapedText {
    pub(super) fn glyphs(&self, cluster: &ShapedCluster) -> &[ShapedGlyph] {
        &self.glyphs[cluster.glyphs.clone()]
    }

    fn push(&mut self, text: Range<usize>, advance: f32, glyphs: &[ShapedGlyph]) {
        let start = self.glyphs.len();
        self.glyphs.extend_from_slice(glyphs);
        self.clusters.push(ShapedCluster {
            text,
            glyphs: start..self.glyphs.len(),
            advance,
            newline: false,
        });
    }

    fn push_empty(&mut self, text: Range<usize>, newline: bool) {
        let start = self.glyphs.len();
        self.clusters.push(ShapedCluster { text, glyphs: start..start, advance: 0.0, newline });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CharClass {
    Newline,
    /// 制御文字・Default_Ignorable。グリフも欠字表示も持たない。
    Invisible,
    /// 空白。フォントに無くても欠字表示せず、送り幅だけを持つ。
    Space,
    Visible,
}

pub(super) fn char_class(ch: char) -> CharClass {
    match ch {
        '\n' => CharClass::Newline,
        '\t' => CharClass::Space,
        _ if ch.is_control() || is_default_ignorable(ch) => CharClass::Invisible,
        _ if ch.is_whitespace() => CharClass::Space,
        _ => CharClass::Visible,
    }
}

/// Unicode の Default_Ignorable_Code_Point（DerivedCoreProperties.txt）。
///
/// ZWJ・異体字セレクタ・書式制御などで、フォントに無くても欠字として扱わない。
pub(super) fn is_default_ignorable(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

pub(super) fn shape_vector_text(text: &str, fonts: VectorFontSet<'_>) -> ShapedText {
    let mut shaped = ShapedText::default();
    if fonts.primary().is_none() {
        return shaped;
    }
    for (start, ch) in text.char_indices() {
        let range = start..start + ch.len_utf8();
        match char_class(ch) {
            CharClass::Newline => shaped.push_empty(range, true),
            CharClass::Invisible => shaped.push_empty(range, false),
            class => {
                if let Some((index, face)) = fonts.find_face(|face| face.has_glyph(ch)) {
                    let (advance, glyph) = fonts.vector_glyph(index, face, face.glyph_id(ch));
                    shaped.push(range, advance, &[glyph]);
                } else if class == CharClass::Space {
                    shaped.push(range, fonts.missing_space_advance(ch), &[]);
                } else {
                    let (advance, glyph) = fonts.missing_glyph();
                    shaped.push(range, advance, &[glyph]);
                }
            }
        }
    }
    shaped
}

impl<'a> VectorFontSet<'a> {
    /// 設計単位でのグリフ送り幅と描画グリフ。
    pub(super) fn vector_glyph(
        self,
        index: u16,
        face: &TextFace,
        glyph: GlyphId,
    ) -> (f32, ShapedGlyph) {
        let units = self.units_scale(index);
        let face_scale = units * face.metrics().height();
        (
            face.h_advance_unscaled(glyph) * units,
            ShapedGlyph {
                x: 0.0,
                y: 0.0,
                source: GlyphSource::Vector { face: index, glyph, face_scale },
            },
        )
    }

    /// 欠字の代替表示。□ 等の候補をセット全体から探し、無ければ矩形を生成する。
    pub(super) fn missing_glyph(self) -> (f32, ShapedGlyph) {
        for candidate in MISSING_GLYPH_CANDIDATES {
            if let Some((index, face)) = self.find_face(|face| face.has_glyph(candidate)) {
                return self.vector_glyph(index, face, face.glyph_id(candidate));
            }
        }
        generated_tofu(self.em() * TOFU_HEIGHT_EM, -self.em() * TOFU_HEIGHT_EM)
    }

    fn missing_space_advance(self, ch: char) -> f32 {
        if ch == IDEOGRAPHIC_SPACE {
            return self.em();
        }
        self.find_face(|face| face.has_glyph(' '))
            .map(|(index, face)| self.vector_glyph(index, face, face.glyph_id(' ')).0)
            .unwrap_or(self.em() * 0.25)
    }
}

/// 設計単位の生成矩形。`top` は行基準からの上端。
fn generated_tofu(height: f32, top: f32) -> (f32, ShapedGlyph) {
    let width = height * TOFU_WIDTH_RATIO;
    let advance = width + height * TOFU_STROKE_RATIO;
    (advance, ShapedGlyph { x: 0.0, y: top, source: GlyphSource::Tofu { width, height } })
}

pub(super) fn shape_bitmap_text(text: &str, fonts: BitmapFontSet<'_>) -> ShapedText {
    let mut shaped = ShapedText::default();
    for (start, ch) in text.char_indices() {
        let range = start..start + ch.len_utf8();
        let class = char_class(ch);
        match class {
            CharClass::Newline => shaped.push_empty(range, true),
            CharClass::Invisible => shaped.push_empty(range, false),
            _ => {
                if let Some((advance, glyph)) = fonts.bitmap_glyph(ch) {
                    shaped.push(range, advance, &[glyph]);
                } else if class == CharClass::Space {
                    // 空白を持たない.fntは従来どおり幅0で詰める（欠字表示はしない）。
                    shaped.push_empty(range, false);
                } else {
                    let (advance, glyph) = fonts.missing_glyph();
                    shaped.push(range, advance, &[glyph]);
                }
            }
        }
    }
    shaped
}

impl<'a> BitmapFontSet<'a> {
    /// beatoraja と同じく主フォント → fallback .fnt の順に探し、無ければ
    /// 〜(U+301C) と ～(U+FF5E) を相互に代用する。それも無ければベクターの
    /// fallback・OS フォントの実グリフを使う（BMZ 拡張）。
    fn bitmap_glyph(self, ch: char) -> Option<(f32, ShapedGlyph)> {
        let equivalent = match ch {
            WAVE_DASH => Some(FULLWIDTH_TILDE),
            FULLWIDTH_TILDE => Some(WAVE_DASH),
            _ => None,
        };
        std::iter::once(ch).chain(equivalent).find_map(|key| self.glyph_of(key)).or_else(|| {
            let (index, face) = self.faces.find(|face| face.has_glyph(ch))?;
            Some(self.vector_glyph(index, face, face.glyph_id(ch)))
        })
    }

    fn glyph_of(self, ch: char) -> Option<(f32, ShapedGlyph)> {
        self.fonts.iter().enumerate().find_map(|(index, entry)| {
            let glyph = entry.font.glyphs.get(&ch)?;
            Some((
                glyph.xadvance as f32,
                ShapedGlyph {
                    x: 0.0,
                    y: 0.0,
                    source: GlyphSource::Bitmap { font: index as u16, ch },
                },
            ))
        })
    }

    /// ベクター face のグリフを、ビットマップの設計サイズを em として基準線へ置く。
    fn vector_glyph(self, index: u16, face: &TextFace, glyph: GlyphId) -> (f32, ShapedGlyph) {
        let metrics = face.metrics();
        let units = self.design_size() / metrics.units_per_em;
        (
            face.h_advance_unscaled(glyph) * units,
            ShapedGlyph {
                x: 0.0,
                y: self.cap_height(),
                source: GlyphSource::Vector {
                    face: index,
                    glyph,
                    face_scale: units * metrics.height(),
                },
            },
        )
    }

    fn missing_glyph(self) -> (f32, ShapedGlyph) {
        if let Some(found) = MISSING_GLYPH_CANDIDATES.iter().find_map(|ch| self.glyph_of(*ch)) {
            return found;
        }
        // 文字上端（大文字の高さ）から基準線までの矩形にする。
        generated_tofu(self.cap_height(), 0.0)
    }

    /// 描画原点（大文字上端）から基準線までの設計単位。
    fn cap_height(self) -> f32 {
        let font = self.primary();
        let cap_height = font.base as f32 - font.ascent;
        if cap_height > 0.0 { cap_height } else { self.design_size() * 0.7 }
    }

    pub(super) fn design_size(self) -> f32 {
        let font = self.primary();
        (if font.size > 0 { font.size } else { font.line_height.max(1) }) as f32
    }
}

/// 送り幅の合計（px）。旧実装と同じく cluster ごとの px 送り幅を加算する。
pub(super) fn clusters_width_px(clusters: &[ShapedCluster], scale_x: f32) -> f32 {
    clusters.iter().map(|cluster| cluster.advance * scale_x).sum()
}

/// `max_width` に収まる先頭 cluster 数。
pub(super) fn truncated_cluster_count(
    clusters: &[ShapedCluster],
    scale_x: f32,
    max_width: f32,
) -> usize {
    let mut width = 0.0;
    for (index, cluster) in clusters.iter().enumerate() {
        let advance = cluster.advance * scale_x;
        if width + advance > max_width {
            return index;
        }
        width += advance;
    }
    clusters.len()
}

/// 改行と最大幅で cluster 列を行へ分割する。改行 cluster 自体はどの行にも含めない。
pub(super) fn wrap_clusters(
    clusters: &[ShapedCluster],
    scale_x: f32,
    max_width: f32,
) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut width = 0.0;
    for (index, cluster) in clusters.iter().enumerate() {
        if cluster.newline {
            lines.push(start..index);
            start = index + 1;
            width = 0.0;
            continue;
        }
        let advance = cluster.advance * scale_x;
        if index > start && width + advance > max_width {
            lines.push(start..index);
            start = index;
            width = 0.0;
        }
        width += advance;
    }
    lines.push(start..clusters.len());
    lines
}

/// caret のバイト位置より前で終わる cluster の幅（px）。
pub(super) fn prefix_width_px(clusters: &[ShapedCluster], byte_index: usize, scale_x: f32) -> f32 {
    clusters
        .iter()
        .take_while(|cluster| cluster.text.end <= byte_index)
        .map(|cluster| cluster.advance * scale_x)
        .sum()
}

/// フォントセットと文字列ごとの解決結果。フォント変更時は atlas ごと作り直す。
#[derive(Debug, Default)]
pub(super) struct TextShapeCache {
    vector: HashMap<Box<str>, HashMap<Box<str>, Arc<ShapedText>>>,
    bitmap: HashMap<Box<str>, HashMap<Box<str>, Arc<ShapedText>>>,
    entries: usize,
}

impl TextShapeCache {
    pub(super) fn vector(&mut self, text: &str, fonts: VectorFontSet<'_>) -> Arc<ShapedText> {
        if let Some(shaped) = self.vector.get(fonts.id).and_then(|texts| texts.get(text)) {
            return shaped.clone();
        }
        let shaped = Arc::new(shape_vector_text(text, fonts));
        self.reserve_entry();
        self.vector.entry(fonts.id.into()).or_default().insert(text.into(), shaped.clone());
        shaped
    }

    pub(super) fn bitmap(&mut self, text: &str, fonts: BitmapFontSet<'_>) -> Arc<ShapedText> {
        if let Some(shaped) = self.bitmap.get(fonts.id).and_then(|texts| texts.get(text)) {
            return shaped.clone();
        }
        let shaped = Arc::new(shape_bitmap_text(text, fonts));
        self.reserve_entry();
        self.bitmap.entry(fonts.id.into()).or_default().insert(text.into(), shaped.clone());
        shaped
    }

    fn reserve_entry(&mut self) {
        if self.entries >= TEXT_SHAPE_CACHE_MAX_ENTRIES {
            self.vector.clear();
            self.bitmap.clear();
            self.entries = 0;
        }
        self.entries += 1;
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries
    }
}
