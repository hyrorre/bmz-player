use unicode_segmentation::UnicodeSegmentation;

use super::*;

/// beatoraja の `MISSING_GLYPH_CANDIDATES` と同じ欠字代替候補（□, ▢, ☐, ■, ?）。
pub(super) const MISSING_GLYPH_CANDIDATES: [char; 5] =
    ['\u{25A1}', '\u{25A2}', '\u{2610}', '\u{25A0}', '?'];

const IDEOGRAPHIC_SPACE: char = '\u{3000}';
const WAVE_DASH: char = '\u{301C}';
const FULLWIDTH_TILDE: char = '\u{FF5E}';

/// 代替グリフも無いときに生成する欠字矩形の高さ（em比）と縦横比。
pub(super) const TOFU_HEIGHT_EM: f32 = 0.8;
pub(super) const TOFU_WIDTH_RATIO: f32 = 0.75;
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
    /// `color` はフォント由来の色で描くカラーグリフ。
    Vector { face: u16, glyph: GlyphId, face_scale: f32, color: bool },
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
        // 点字の空白（U+2800）も、描画画素を持たない文字として空白と同じに扱う。
        _ if ch.is_whitespace() || ch == '\u{2800}' => CharClass::Space,
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

/// 1つの grapheme cluster の構成。
struct ClusterChars {
    chars: Vec<char>,
    /// 描画対象の文字（Default_Ignorable・制御文字を除く）。
    required: Vec<char>,
    /// 異体字セレクタ（U+FE00..FE0F, U+E0100..E01EF）。
    selector: Option<char>,
}

impl ClusterChars {
    fn new(cluster: &str) -> Self {
        let chars: Vec<char> = cluster.chars().collect();
        let required = chars
            .iter()
            .copied()
            .filter(|ch| matches!(char_class(*ch), CharClass::Visible | CharClass::Space))
            .collect();
        let selector = chars.iter().copied().find(|ch| is_variation_selector(*ch));
        Self { chars, required, selector }
    }

    fn has_newline(&self) -> bool {
        self.chars.contains(&'\n')
    }

    /// ZWJ・国旗・肌色・キーキャップ・絵文字異体字・タグを含む絵文字シーケンス。
    ///
    /// 1つの face で描けない場合は部品へ分解せず、cluster 単位で欠字表示する。
    fn is_emoji_sequence(&self) -> bool {
        self.chars.iter().any(|ch| {
            matches!(
                *ch as u32,
                0x200D | 0x20E3 | 0xFE0F | 0x1F1E6..=0x1F1FF | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F
            )
        })
    }
}

/// 複数文字の cluster を1つの face で描くときに、シェーピング結果へ求める条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SequenceKind {
    /// 結合文字など。各文字のグリフを並べた複数グリフのままでよい。
    Plain,
    /// キーキャップ。合字か、送り幅を持たない囲み記号（U+20E3）を数字に重ねて描く。
    Keycap,
    /// ZWJ・国旗・肌色・タグのシーケンス。部品が1つの合字へ置換された場合だけ描く。
    Ligature,
}

impl ClusterChars {
    fn sequence_kind(&self) -> SequenceKind {
        let count = |range: std::ops::RangeInclusive<u32>| {
            self.chars.iter().filter(|ch| range.contains(&(**ch as u32))).count()
        };
        let components = self.required.len();
        let ligature = count(0xE0020..=0xE007E) > 0
            || (components >= 2 && count(0x200D..=0x200D) > 0)
            || count(0x1F1E6..=0x1F1FF) >= 2
            || (components >= 2 && count(0x1F3FB..=0x1F3FF) > 0);
        if ligature {
            SequenceKind::Ligature
        } else if count(0x20E3..=0x20E3) > 0 {
            SequenceKind::Keycap
        } else {
            SequenceKind::Plain
        }
    }
}

impl SequenceKind {
    /// シェーピング結果が、このシーケンスの描き方として成立しているか。
    ///
    /// 合字は、全グリフが同じ cluster 番号（構成文字が統合された）で、かつ各文字の
    /// cmap のグリフをそのまま並べただけではないことで判定する。後者はタグのように
    /// 構成文字が1つで、タグ文字が Default_Ignorable として取り除かれる場合に必要。
    fn is_formed(self, face: &TextFace, required: &[char], glyphs: &[FaceShapedGlyph]) -> bool {
        let ligature = || {
            glyphs.iter().all(|glyph| glyph.cluster == glyphs[0].cluster)
                && !glyphs
                    .iter()
                    .map(|glyph| glyph.glyph)
                    .eq(required.iter().map(|ch| face.glyph_id(*ch)))
        };
        match self {
            Self::Plain => true,
            Self::Ligature => ligature(),
            Self::Keycap => {
                ligature()
                    || (glyphs.len() == required.len()
                        && glyphs[1..].iter().all(|glyph| glyph.x_advance == 0.0))
            }
        }
    }
}

pub(super) fn is_variation_selector(ch: char) -> bool {
    matches!(ch as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF)
}

const TEXT_PRESENTATION_SELECTOR: char = '\u{FE0E}';
const EMOJI_PRESENTATION_SELECTOR: char = '\u{FE0F}';

/// 文字・cluster の表示形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Presentation {
    Text,
    Emoji,
}

impl Presentation {
    /// VS15 / VS16 を優先し、無ければ絵文字シーケンスか Emoji_Presentation の既定値を使う。
    ///
    /// ✡（U+2721）や ❤（U+2764）のように既定がテキスト表示の記号は、VS16 が無い限り
    /// 通常の記号として扱う。
    pub(super) fn of(base: char, selector: Option<char>, emoji_sequence: bool) -> Self {
        use unicode_properties::{EmojiStatus, UnicodeEmoji};
        match selector {
            Some(EMOJI_PRESENTATION_SELECTOR) => return Self::Emoji,
            Some(TEXT_PRESENTATION_SELECTOR) => return Self::Text,
            _ => {}
        }
        let emoji_default = matches!(
            base.emoji_status(),
            EmojiStatus::EmojiPresentation
                | EmojiStatus::EmojiPresentationAndModifierBase
                | EmojiStatus::EmojiPresentationAndEmojiComponent
                | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
        );
        if emoji_sequence || emoji_default { Self::Emoji } else { Self::Text }
    }

    /// 探す描画形式の順。絵文字はカラー → 輪郭、テキストは輪郭 → カラー。
    fn paints(self) -> [GlyphPaint; 2] {
        match self {
            Self::Emoji => [GlyphPaint::Color, GlyphPaint::Outline],
            Self::Text => [GlyphPaint::Outline, GlyphPaint::Color],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum GlyphPaint {
    Outline,
    Color,
}

/// 文字（＋異体字セレクタ）を描ける face・グリフ・形式を、表示形式の優先順で探す。
fn resolve_char<'a>(
    faces: FaceList<'a>,
    ch: char,
    selector: Option<char>,
    paints: &[GlyphPaint],
) -> Option<(u16, &'a TextFace, GlyphId, GlyphPaint)> {
    paints.iter().find_map(|paint| {
        faces
            .find_map(|face| {
                let variation = selector.and_then(|selector| face.variation_glyph(ch, selector));
                variation
                    .into_iter()
                    .chain([face.glyph_id(ch)])
                    .find(|glyph| face.paints(*glyph, *paint))
            })
            .map(|(index, face, glyph)| (index, face, glyph, *paint))
    })
}

/// 複数文字の cluster を1つの face でシェーピングできるか、表示形式の優先順で探す。
///
/// 構成文字のグリフがあっても、シーケンスに必要な合字・置換が無い face は使わない。
fn resolve_cluster<'a>(
    faces: FaceList<'a>,
    cluster: &str,
    parts: &ClusterChars,
    presentation: Presentation,
) -> Option<(u16, &'a TextFace, Vec<FaceShapedGlyph>, GlyphPaint)> {
    let kind = parts.sequence_kind();
    let required = parts.required.as_slice();
    presentation.paints().into_iter().find_map(|paint| {
        faces
            .find_map(|face| {
                if !required.iter().all(|ch| face.glyph_id(*ch).0 != 0) {
                    return None;
                }
                let glyphs = face.shape_cluster(cluster)?;
                (kind.is_formed(face, required, &glyphs)
                    && glyphs.iter().all(|glyph| face.paints(glyph.glyph, paint)))
                .then_some(glyphs)
            })
            .map(|(index, face, glyphs)| (index, face, glyphs, paint))
    })
}

pub(super) fn shape_vector_text(text: &str, fonts: VectorFontSet<'_>) -> ShapedText {
    let mut shaped = ShapedText::default();
    if fonts.primary().is_none() {
        return shaped;
    }
    for (start, cluster) in text.grapheme_indices(true) {
        let range = start..start + cluster.len();
        let mut chars = cluster.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            let presentation = Presentation::of(ch, None, false);
            push_vector_char(&mut shaped, range, ch, None, presentation, fonts);
            continue;
        }
        let parts = ClusterChars::new(cluster);
        match parts.required.as_slice() {
            _ if parts.has_newline() => shaped.push_empty(range, true),
            [] => shaped.push_empty(range, false),
            // タグシーケンスは見える文字が1つでも、cluster 全体をシェーピングする。
            [ch] if parts.sequence_kind() != SequenceKind::Ligature => {
                let presentation = Presentation::of(*ch, parts.selector, parts.is_emoji_sequence());
                push_vector_char(&mut shaped, range, *ch, parts.selector, presentation, fonts);
            }
            required => {
                let presentation =
                    Presentation::of(required[0], parts.selector, parts.is_emoji_sequence());
                let shaped_cluster = resolve_cluster(fonts.faces, cluster, &parts, presentation)
                    .map(|(index, face, glyphs, paint)| {
                        let units = fonts.units_scale(index);
                        place_face_glyphs(
                            index,
                            &glyphs,
                            units,
                            units * face.metrics().height(),
                            0.0,
                            paint == GlyphPaint::Color,
                        )
                    });
                if let Some((advance, glyphs)) = shaped_cluster {
                    shaped.push(range, advance, &glyphs);
                } else if parts.is_emoji_sequence() {
                    let (advance, glyph) = fonts.missing_glyph();
                    shaped.push(range, advance, &[glyph]);
                } else {
                    // 結合文字を描ける face が無ければ基底文字だけを描く。
                    push_vector_char(&mut shaped, range, required[0], None, presentation, fonts);
                }
            }
        }
    }
    shaped
}

fn push_vector_char(
    shaped: &mut ShapedText,
    range: Range<usize>,
    ch: char,
    selector: Option<char>,
    presentation: Presentation,
    fonts: VectorFontSet<'_>,
) {
    match char_class(ch) {
        CharClass::Newline => shaped.push_empty(range, true),
        CharClass::Invisible => shaped.push_empty(range, false),
        // 空白は描画画素を持たないので、送り幅だけを持つ。
        CharClass::Space => shaped.push(range, fonts.space_advance(ch), &[]),
        CharClass::Visible => {
            if let Some((index, face, glyph, paint)) =
                resolve_char(fonts.faces, ch, selector, &presentation.paints())
            {
                let (advance, glyph) =
                    fonts.vector_glyph(index, face, glyph, paint == GlyphPaint::Color);
                shaped.push(range, advance, &[glyph]);
            } else {
                let (advance, glyph) = fonts.missing_glyph();
                shaped.push(range, advance, &[glyph]);
            }
        }
    }
}

/// シェーピング結果を設計単位へ変換する。`baseline_y` は行基準から基準線までの距離。
fn place_face_glyphs(
    face: u16,
    glyphs: &[FaceShapedGlyph],
    units: f32,
    face_scale: f32,
    baseline_y: f32,
    color: bool,
) -> (f32, Vec<ShapedGlyph>) {
    let mut pen = 0.0;
    let placed = glyphs
        .iter()
        .map(|glyph| {
            let placed = ShapedGlyph {
                x: (pen + glyph.x_offset) * units,
                y: baseline_y - glyph.y_offset * units,
                source: GlyphSource::Vector { face, glyph: glyph.glyph, face_scale, color },
            };
            pen += glyph.x_advance;
            placed
        })
        .collect();
    (pen * units, placed)
}

impl<'a> VectorFontSet<'a> {
    /// 設計単位でのグリフ送り幅と描画グリフ。
    pub(super) fn vector_glyph(
        self,
        index: u16,
        face: &TextFace,
        glyph: GlyphId,
        color: bool,
    ) -> (f32, ShapedGlyph) {
        let units = self.units_scale(index);
        let face_scale = units * face.metrics().height();
        (
            face.h_advance_unscaled(glyph) * units,
            ShapedGlyph {
                x: 0.0,
                y: 0.0,
                source: GlyphSource::Vector { face: index, glyph, face_scale, color },
            },
        )
    }

    /// 欠字の代替表示。□ 等の候補をセット全体から探し、無ければ矩形を生成する。
    pub(super) fn missing_glyph(self) -> (f32, ShapedGlyph) {
        for candidate in MISSING_GLYPH_CANDIDATES {
            if let Some((index, face)) = self.find_face(|face| face.has_glyph(candidate)) {
                return self.vector_glyph(index, face, face.glyph_id(candidate), false);
            }
        }
        generated_tofu(self.em() * TOFU_HEIGHT_EM, -self.em() * TOFU_HEIGHT_EM)
    }

    /// 空白の送り幅。文字を持つ最初の face の幅、無ければ半角空白か em から決める。
    fn space_advance(self, ch: char) -> f32 {
        if let Some((index, face)) = self.find_face(|face| face.has_spacing_glyph(ch)) {
            return self.vector_glyph(index, face, face.glyph_id(ch), false).0;
        }
        if ch == IDEOGRAPHIC_SPACE {
            return self.em();
        }
        self.find_face(|face| face.has_spacing_glyph(' '))
            .map(|(index, face)| self.vector_glyph(index, face, face.glyph_id(' '), false).0)
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
    for (start, cluster) in text.grapheme_indices(true) {
        let range = start..start + cluster.len();
        let mut chars = cluster.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            push_bitmap_char(&mut shaped, range, ch, None, fonts);
            continue;
        }
        let parts = ClusterChars::new(cluster);
        match parts.required.as_slice() {
            _ if parts.has_newline() => shaped.push_empty(range, true),
            [] => shaped.push_empty(range, false),
            [ch] if parts.sequence_kind() != SequenceKind::Ligature => {
                push_bitmap_char(&mut shaped, range, *ch, parts.selector, fonts)
            }
            required => {
                if let Some((advance, glyphs)) = fonts.cluster_glyphs(cluster, &parts) {
                    shaped.push(range, advance, &glyphs);
                } else if parts.is_emoji_sequence() {
                    let (advance, glyph) = fonts.missing_glyph();
                    shaped.push(range, advance, &[glyph]);
                } else {
                    push_bitmap_char(&mut shaped, range, required[0], None, fonts);
                }
            }
        }
    }
    shaped
}

fn push_bitmap_char(
    shaped: &mut ShapedText,
    range: Range<usize>,
    ch: char,
    selector: Option<char>,
    fonts: BitmapFontSet,
) {
    let class = char_class(ch);
    let presentation = Presentation::of(ch, selector, false);
    match class {
        CharClass::Newline => shaped.push_empty(range, true),
        CharClass::Invisible => shaped.push_empty(range, false),
        CharClass::Space => {
            if let Some((advance, glyph)) = fonts.glyph_of(ch) {
                shaped.push(range, advance, &[glyph]);
            } else if let Some((index, face)) = fonts.faces.find(|face| face.has_spacing_glyph(ch))
            {
                let (advance, _) = fonts.vector_glyph(index, face, face.glyph_id(ch), false);
                shaped.push(range, advance, &[]);
            } else {
                // 空白を持たない.fntは従来どおり幅0で詰める（欠字表示はしない）。
                shaped.push_empty(range, false);
            }
        }
        CharClass::Visible => {
            if let Some((advance, glyph)) = fonts.bitmap_glyph(ch, selector, presentation) {
                shaped.push(range, advance, &[glyph]);
            } else {
                let (advance, glyph) = fonts.missing_glyph();
                shaped.push(range, advance, &[glyph]);
            }
        }
    }
}

impl<'a> BitmapFontSet<'a> {
    /// beatoraja と同じく主フォント → fallback .fnt の順に探し、無ければ
    /// 〜(U+301C) と ～(U+FF5E) を相互に代用する。それも無ければベクターの
    /// fallback・OS フォントの実グリフを使う（BMZ 拡張）。
    ///
    /// 絵文字表示の文字は、カラーグリフを持つベクター face を .fnt より先に探す。
    fn bitmap_glyph(
        self,
        ch: char,
        selector: Option<char>,
        presentation: Presentation,
    ) -> Option<(f32, ShapedGlyph)> {
        let vector = |paints: &[GlyphPaint]| {
            let (index, face, glyph, paint) = resolve_char(self.faces, ch, selector, paints)?;
            Some(self.vector_glyph(index, face, glyph, paint == GlyphPaint::Color))
        };
        if presentation == Presentation::Emoji
            && let Some(found) = vector(&[GlyphPaint::Color])
        {
            return Some(found);
        }
        let equivalent = match ch {
            WAVE_DASH => Some(FULLWIDTH_TILDE),
            FULLWIDTH_TILDE => Some(WAVE_DASH),
            _ => None,
        };
        std::iter::once(ch)
            .chain(equivalent)
            .find_map(|key| self.glyph_of(key))
            .or_else(|| vector(&presentation.paints()))
    }

    /// 複数文字の cluster。.fnt が全文字を持てば順に並べ（結合文字等）、
    /// 無ければ全文字を持つベクター face でシェーピングする。
    fn cluster_glyphs(
        self,
        cluster: &str,
        parts: &ClusterChars,
    ) -> Option<(f32, Vec<ShapedGlyph>)> {
        if !parts.is_emoji_sequence() {
            let glyphs: Option<Vec<_>> =
                parts.required.iter().map(|ch| self.glyph_of(*ch)).collect();
            if let Some(glyphs) = glyphs {
                let mut pen = 0.0;
                let placed = glyphs
                    .into_iter()
                    .map(|(advance, glyph)| {
                        let placed = ShapedGlyph { x: pen, ..glyph };
                        pen += advance;
                        placed
                    })
                    .collect();
                return Some((pen, placed));
            }
        }
        let presentation =
            Presentation::of(parts.required[0], parts.selector, parts.is_emoji_sequence());
        let (index, face, glyphs, paint) =
            resolve_cluster(self.faces, cluster, parts, presentation)?;
        let units = self.design_size() / face.metrics().units_per_em;
        Some(place_face_glyphs(
            index,
            &glyphs,
            units,
            units * face.metrics().height(),
            self.cap_height(),
            paint == GlyphPaint::Color,
        ))
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
    fn vector_glyph(
        self,
        index: u16,
        face: &TextFace,
        glyph: GlyphId,
        color: bool,
    ) -> (f32, ShapedGlyph) {
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
                    color,
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
