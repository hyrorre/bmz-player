use super::*;

/// 既定テキストのフォント列と、全テキスト共通の補完用 OS フォント。
///
/// `faces[..default_len]` は CJK coverage 順の既定フォント、残りは記号・絵文字の
/// OS フォント。スキン指定フォントでは全体をスキンの fallback の後ろに続ける。
#[derive(Clone, Default)]
pub(super) struct FontFallbackChain {
    pub(super) faces: Vec<TextFace>,
    pub(super) default_len: usize,
}

impl FontFallbackChain {
    #[cfg(test)]
    pub(super) fn primary(&self) -> Option<&TextFace> {
        self.faces.first()
    }

    #[cfg(test)]
    pub(super) fn select(&self, ch: char) -> Option<&TextFace> {
        self.faces[..self.default_len].iter().find(|face| face.has_glyph(ch))
    }

    #[cfg(test)]
    pub(super) fn single(font: TextFace) -> Self {
        Self { faces: vec![font], default_len: 1 }
    }

    /// 未指定テキストの face 列。
    pub(super) fn default_faces(&self) -> FaceList<'_> {
        let default_len = self.default_len.min(self.faces.len());
        FaceList { preferred: &self.faces[..default_len], system: &self.faces[default_len..] }
    }

    /// スキン指定フォントの後ろに続ける face 列。
    pub(super) fn after<'a>(&'a self, preferred: &'a [TextFace]) -> FaceList<'a> {
        FaceList { preferred, system: &self.faces }
    }
}

/// 優先 face（スキン指定・既定フォント）とその後ろの OS fallback face。
///
/// 番号は `preferred` から通しで振る。cluster の解決結果はこの番号を保持する。
#[derive(Clone, Copy)]
pub(super) struct FaceList<'a> {
    pub(super) preferred: &'a [TextFace],
    pub(super) system: &'a [TextFace],
}

impl<'a> FaceList<'a> {
    pub(super) fn first(self) -> Option<&'a TextFace> {
        self.preferred.first().or_else(|| self.system.first())
    }

    pub(super) fn get(self, index: u16) -> Option<&'a TextFace> {
        let index = usize::from(index);
        self.preferred.get(index).or_else(|| self.system.get(index - self.preferred.len()))
    }

    pub(super) fn find(
        self,
        mut predicate: impl FnMut(&TextFace) -> bool,
    ) -> Option<(u16, &'a TextFace)> {
        self.find_map(|face| predicate(face).then_some(())).map(|(index, face, ())| (index, face))
    }

    pub(super) fn find_map<T>(
        self,
        mut f: impl FnMut(&TextFace) -> Option<T>,
    ) -> Option<(u16, &'a TextFace, T)> {
        self.preferred
            .iter()
            .chain(self.system)
            .enumerate()
            .take(usize::from(u16::MAX))
            .find_map(|(index, face)| f(face).map(|value| (index as u16, face, value)))
    }
}

/// 1つのテキスト描画で使うベクターフォントの優先順。
///
/// `id` は解決結果・レイアウトのキャッシュキーで、フォント構成が変わると
/// atlas ごとキャッシュを作り直す。先頭 face の寸法を基準に他の face の em を揃える。
#[derive(Clone, Copy)]
pub(super) struct VectorFontSet<'a> {
    pub(super) id: &'a str,
    pub(super) faces: FaceList<'a>,
}

impl<'a> VectorFontSet<'a> {
    pub(super) fn primary(self) -> Option<&'a TextFace> {
        self.faces.first()
    }

    pub(super) fn face(self, index: u16) -> Option<&'a TextFace> {
        self.faces.get(index)
    }

    pub(super) fn find_face(
        self,
        predicate: impl FnMut(&TextFace) -> bool,
    ) -> Option<(u16, &'a TextFace)> {
        self.faces.find(predicate)
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

/// スキンが登録したビットマップフォント。
#[derive(Clone)]
pub(super) struct BitmapFontEntry {
    pub(super) id: Arc<str>,
    pub(super) font: BitmapFont,
}

/// 1つのテキスト描画で使うビットマップフォントと、その後ろのベクター face。
///
/// `fonts[0]` が主フォント、以降は `font.fallback` の .fnt。
#[derive(Clone, Copy)]
pub(super) struct BitmapFontSet<'a> {
    pub(super) id: &'a str,
    pub(super) fonts: &'a [BitmapFontEntry],
    pub(super) faces: FaceList<'a>,
}

impl<'a> BitmapFontSet<'a> {
    pub(super) fn primary(self) -> &'a BitmapFont {
        &self.fonts[0].font
    }
}

/// スキンフォントごとに解決した優先順（主フォント＋`font.fallback`）。
///
/// フォントの登録・fallback 指定が変わるたびに作り直し、描画時は参照するだけにする。
#[derive(Clone, Default)]
pub(super) struct SkinFontSets {
    pub(super) vector: HashMap<String, Vec<TextFace>>,
    pub(super) bitmap: HashMap<String, BitmapFontChain>,
}

#[derive(Clone)]
pub(super) struct BitmapFontChain {
    pub(super) fonts: Vec<BitmapFontEntry>,
    pub(super) faces: Vec<TextFace>,
}

impl SkinFontSets {
    /// beatoraja と同じく、ベクター主フォントはベクターの fallback だけを使う。
    /// ビットマップ主フォントは .fnt の fallback を先に、ベクターの fallback を後に探す。
    pub(super) fn new(
        fonts: &HashMap<String, TextFace>,
        bitmap_fonts: &HashMap<String, BitmapFont>,
        fallbacks: &HashMap<String, Vec<String>>,
    ) -> Self {
        let fallback_ids = |id: &str| {
            let mut ids: Vec<&str> = Vec::new();
            for fallback in fallbacks.get(id).into_iter().flatten() {
                if fallback != id && !ids.contains(&fallback.as_str()) {
                    ids.push(fallback);
                }
            }
            ids
        };
        let vector = fonts
            .iter()
            .map(|(id, face)| {
                let faces = std::iter::once(face.clone())
                    .chain(fallback_ids(id).into_iter().filter_map(|id| fonts.get(id).cloned()))
                    .collect();
                (id.clone(), faces)
            })
            .collect();
        let bitmap = bitmap_fonts
            .iter()
            .map(|(id, font)| {
                let ids = fallback_ids(id);
                let entries = std::iter::once((id.as_str(), font))
                    .chain(ids.iter().filter_map(|id| {
                        bitmap_fonts.get_key_value(*id).map(|(id, font)| (id.as_str(), font))
                    }))
                    .map(|(id, font)| BitmapFontEntry { id: Arc::from(id), font: font.clone() })
                    .collect();
                let faces = ids.iter().filter_map(|id| fonts.get(*id).cloned()).collect();
                (id.clone(), BitmapFontChain { fonts: entries, faces })
            })
            .collect();
        Self { vector, bitmap }
    }
}

pub(super) fn build_text_frame_with_font_sets(
    plan: &DrawPlan,
    system_fonts: &FontFallbackChain,
    font_sets: &SkinFontSets,
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
        // 位置が不正な文字は組まずに飛ばす。コマンドごとの quad 数とキャレットの対応は保つ。
        if !point_is_finite(origin) || !point_is_finite(post_scale) {
            command_quad_counts.push(0);
            command_caret_rects.push(None);
            continue;
        }
        if let Some(font_id) = style.font_id.as_deref()
            && let Some(chain) = font_sets.bitmap.get(font_id)
        {
            let fonts = BitmapFontSet {
                id: font_id,
                fonts: &chain.fonts,
                faces: system_fonts.after(&chain.faces),
            };
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
                    font_sets
                        .vector
                        .get_key_value(font_id)
                        .map(|(id, faces)| VectorFontSet { id, faces: system_fonts.after(faces) })
                })
                .unwrap_or(VectorFontSet {
                    id: DEFAULT_TEXT_FONT_ID,
                    faces: system_fonts.default_faces(),
                });
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
        drop_non_finite_quads(&mut builder.quads, quads_before);
        if let Some(caret) = command_caret_rects.last_mut()
            && caret.is_some_and(|caret| !rect_is_finite(caret.rect))
        {
            *caret = None;
        }
        command_quad_counts.push(builder.quads.len() - quads_before);
    }
    builder.finish(command_quad_counts, command_caret_rects)
}

fn point_is_finite(point: &Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn rect_is_finite(rect: Rect) -> bool {
    [rect.x, rect.y, rect.width, rect.height].into_iter().all(f32::is_finite)
}

/// `start` 以降の quad から、縁取りの幅や影の位置などで非有限になったものを除く。
/// GPU のインスタンスバッファへ不正な座標を送らない。
fn drop_non_finite_quads(quads: &mut Vec<TextQuad>, start: usize) {
    if quads[start..].iter().all(TextQuad::is_finite) {
        return;
    }
    let kept: Vec<_> = quads.drain(start..).filter(TextQuad::is_finite).collect();
    quads.extend(kept);
}

/// fallback 指定なしのフォント表でフレームを組む（テスト用）。
#[cfg(test)]
pub(super) fn build_text_frame_with_fallback_cache(
    plan: &DrawPlan,
    default_fonts: &FontFallbackChain,
    fonts: &HashMap<String, TextFace>,
    bitmap_fonts: &HashMap<String, BitmapFont>,
    surface: SurfaceSize,
    atlas: &mut TextAtlasCache,
) -> TextFrame {
    let font_sets = SkinFontSets::new(fonts, bitmap_fonts, &HashMap::new());
    build_text_frame_with_font_sets(plan, default_fonts, &font_sets, surface, atlas)
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

impl TextQuad {
    /// 正規化座標で `x` / `y` だけ動かした quad。
    pub(super) fn translated(&self, x: f32, y: f32) -> Self {
        Self { x: self.x + x, y: self.y + y, ..self.clone() }
    }

    pub(super) fn is_finite(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
    }
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
    /// 種類ごとの追加の識別値。`StyledFallback` では基準線の位置。
    detail_bits: u32,
}

impl TextGlyphKey {
    /// 画像フォント文中の欠字を補うベクターグリフ。`set_id` はビットマップの主フォント。
    pub(super) fn styled_fallback(
        set_id: Arc<str>,
        face_index: u16,
        glyph_id: GlyphId,
        scale: PxScale,
        baseline: f32,
    ) -> Self {
        Self {
            kind: TextGlyphKind::StyledFallback,
            font_id: set_id,
            glyph: (u32::from(face_index) << 16) | u32::from(glyph_id.0),
            scale_x_bits: scale.x.to_bits(),
            scale_y_bits: scale.y.to_bits(),
            detail_bits: baseline.to_bits(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum TextGlyphKind {
    Vector,
    Bitmap,
    Tofu,
    Color,
    /// カラーグリフの alpha だけを持つ白いシルエット（影・縁取り用）。
    ColorSilhouette,
    /// .fnt から推定した塗り・縁取りで描いた、画像フォント文中のベクターグリフ。
    StyledFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TextLayoutKey {
    kind: TextGlyphKind,
    pub(super) font_id: String,
    pub(super) text: String,
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
    decoration: bool,
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

    fn insert(&mut self, key: TextLayoutKey, quads: Vec<TextQuad>) {
        if self.entries.len() >= TEXT_LAYOUT_CACHE_MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, quads);
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug)]
pub(super) struct TextAtlasCache {
    width: u32,
    /// GPU テクスチャの上限と `TEXT_ATLAS_MAX_HEIGHT` の小さい方。
    max_height: u32,
    pub(super) pen_x: u32,
    pub(super) pen_y: u32,
    row_height: u32,
    pixels: Vec<u8>,
    pub(super) glyphs: HashMap<TextGlyphKey, CachedGlyph>,
    /// ラスタライズできなかったグリフ。毎フレーム同じ失敗を繰り返さない。
    pub(super) failed: std::collections::HashSet<TextGlyphKey>,
    /// atlas の空きが足りず格納を見送ったグリフ。作り直した atlas で再試行する。
    pub(super) deferred: std::collections::HashSet<TextGlyphKey>,
    /// 空き不足でグリフを返せなかった回数の累計。欠けたレイアウトをキャッシュしない判定に使う。
    capacity_misses: u64,
    pub(super) layouts: TextLayoutCache,
    /// 欠字・fallback の解決結果。atlas 画素とは独立なので作り直しでは残す。
    pub(super) shapes: TextShapeCache,
    pub(super) dirty_regions: Vec<TextAtlasDirtyRegion>,
    /// GPU テクスチャの高さ。格納済みの高さを超えたら倍に広げ、作り直しでは縮めない。
    /// 伸びるたびにテクスチャを作り直して全体を転送しないよう、余裕を持って確保する。
    texture_height: u32,
    /// 作り直した。フレームの統計に出す。
    rebuilt: bool,
    /// 空きが足りずに格納できなかったグリフがある。作り直すまで立てたまま。
    overflowed: bool,
    /// 作り直してから始まったフレーム数。まだ作り直していなければ `u32::MAX`。
    frames_since_clear: u32,
    /// 作り直しを見送る残りフレーム数（1フレームの文字が atlas に収まらない場合）。
    clear_delay: u32,
}

impl TextAtlasCache {
    #[cfg(test)]
    pub(super) fn new(width: u32) -> Self {
        Self::with_limits(width, TEXT_ATLAS_MAX_HEIGHT)
    }

    /// GPU の `max_texture_dimension_2d` に収まる atlas。上限を超えて広げることはしない。
    pub(super) fn for_texture_limit(max_texture_dimension: u32) -> Self {
        let limit = max_texture_dimension.max(TEXT_ATLAS_PADDING * 2 + 1);
        Self::with_limits(TEXT_ATLAS_WIDTH.min(limit), TEXT_ATLAS_MAX_HEIGHT.min(limit))
    }

    pub(super) fn with_limits(width: u32, max_height: u32) -> Self {
        Self {
            width,
            max_height,
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            pixels: Vec::new(),
            glyphs: HashMap::new(),
            failed: Default::default(),
            deferred: Default::default(),
            capacity_misses: 0,
            layouts: TextLayoutCache::new(),
            shapes: TextShapeCache::default(),
            dirty_regions: Vec::new(),
            texture_height: 0,
            rebuilt: false,
            overflowed: false,
            frames_since_clear: u32::MAX,
            clear_delay: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn width(&self) -> u32 {
        self.width
    }

    #[cfg(test)]
    pub(super) fn max_height(&self) -> u32 {
        self.max_height
    }

    /// ラスタライズ・格納できなかったグリフを記録する。拡大縮小し続ける文字で
    /// 際限なく増えないよう、上限を超えたら記録をやり直す。
    pub(super) fn mark_failed(&mut self, key: TextGlyphKey) {
        if self.failed.len() >= TEXT_LAYOUT_CACHE_MAX_ENTRIES {
            self.failed.clear();
        }
        self.failed.insert(key);
    }

    /// 空き不足で見送ったグリフなら、作り直すまで再試行せずに欠けたことだけを記録する。
    pub(super) fn is_deferred(&mut self, key: &TextGlyphKey) -> bool {
        if !self.deferred.contains(key) {
            return false;
        }
        self.capacity_misses += 1;
        true
    }

    fn defer(&mut self, key: TextGlyphKey) {
        if self.deferred.len() >= TEXT_LAYOUT_CACHE_MAX_ENTRIES {
            self.deferred.clear();
        }
        self.deferred.insert(key);
        self.capacity_misses += 1;
    }

    pub(super) fn capacity_misses(&self) -> u64 {
        self.capacity_misses
    }

    /// 作り直した後の最初の取り出しで true。
    pub(super) fn take_rebuilt(&mut self) -> bool {
        std::mem::take(&mut self.rebuilt)
    }

    /// 1グリフの画素の上限。これを超えるグリフは表示寸法を保ったまま解像度を下げる。
    pub(super) fn max_glyph_extent(&self) -> u32 {
        self.width.min(self.max_height).saturating_sub(TEXT_ATLAS_PADDING * 2).max(1)
    }

    pub(super) fn begin_frame(&mut self) {
        self.dirty_regions.clear();
        // 空きが足りずに格納できないグリフがあった場合だけ、キャッシュを捨てて作り直す。
        // 高さが上限に達しただけなら登録済みのグリフは描けるので、そのまま使い続ける。
        // フレーム描画前に行うので、このフレームのグリフは新しい atlas へ格納し直される。
        if self.overflowed {
            // 作り直した直後のフレームでも溢れた場合は、1フレームの文字が atlas に
            // 収まらない。すぐ作り直しても同じグリフしか入らないので、間を空ける。
            if self.frames_since_clear == 1 && self.clear_delay == 0 {
                self.clear_delay = TEXT_ATLAS_OVERFLOW_RETRY_FRAMES;
            }
            if self.clear_delay > 0 {
                self.clear_delay -= 1;
            } else {
                self.clear();
            }
        }
        self.frames_since_clear = self.frames_since_clear.saturating_add(1);
    }

    fn clear(&mut self) {
        self.pen_x = 0;
        self.pen_y = 0;
        self.row_height = 0;
        self.pixels.clear();
        self.glyphs.clear();
        self.failed.clear();
        self.deferred.clear();
        self.layouts.clear();
        self.dirty_regions.clear();
        self.rebuilt = true;
        self.overflowed = false;
        self.frames_since_clear = 0;
    }

    pub(super) fn atlas_height(&self) -> u32 {
        (self.pen_y + self.row_height).max(1)
    }

    /// GPU テクスチャの寸法。UV もこの寸法で正規化する。
    pub(super) fn size(&self) -> AtlasSize {
        AtlasSize { width: self.width, height: self.texture_height.max(self.atlas_height()) }
    }

    pub(super) fn pixels_for_size(&self, size: AtlasSize) -> Vec<u8> {
        let mut pixels = self.pixels.clone();
        pixels.resize((size.width * size.height * 4) as usize, 0);
        pixels
    }

    pub(super) fn cached_layout(&self, key: &TextLayoutKey) -> Option<&[TextQuad]> {
        self.layouts.cached(key)
    }

    pub(super) fn insert_layout(&mut self, key: TextLayoutKey, quads: Vec<TextQuad>) {
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
            detail_bits: 0,
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }
        if self.failed.contains(&key) || self.is_deferred(&key) {
            return None;
        }
        let Some(raster) = rasterize_outline_glyph(face, glyph_id, scale, self.max_glyph_extent())
        else {
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

    /// カラーグリフの RGBA。`silhouette` では色を白にして alpha だけを残す。
    pub(super) fn cached_color_glyph(
        &mut self,
        face: &TextFace,
        glyph_id: GlyphId,
        scale: PxScale,
        silhouette: bool,
    ) -> Option<CachedGlyph> {
        let key = TextGlyphKey {
            kind: if silhouette { TextGlyphKind::ColorSilhouette } else { TextGlyphKind::Color },
            font_id: face.cache_id().clone(),
            glyph: u32::from(glyph_id.0),
            scale_x_bits: scale.x.to_bits(),
            scale_y_bits: scale.y.to_bits(),
            detail_bits: 0,
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }
        if self.failed.contains(&key) || self.is_deferred(&key) {
            return None;
        }
        let raster_scale = PxScale {
            x: scale.x * VECTOR_TEXT_SUPERSAMPLE_SCALE,
            y: scale.y * VECTOR_TEXT_SUPERSAMPLE_SCALE,
        };
        let Some(mut raster) =
            rasterize_color_glyph(face, glyph_id, raster_scale, self.max_glyph_extent())
        else {
            self.mark_failed(key);
            return None;
        };
        if silhouette {
            for pixel in raster.pixels.as_chunks_mut::<4>().0 {
                pixel[..3].fill(255);
            }
        }
        // atlas に収めるため縮小した場合も、表示 px は要求した大きさのまま。
        let pixels_per_px = VECTOR_TEXT_SUPERSAMPLE_SCALE * raster.downscale;
        self.insert_glyph_pixels(
            key,
            raster.width,
            raster.height,
            raster.width as f32 / pixels_per_px,
            raster.height as f32 / pixels_per_px,
            raster.offset_x / pixels_per_px,
            raster.offset_y / pixels_per_px,
            raster.pixels,
        )
    }
}

/// 輪郭グリフの白い被覆率画像。
pub(super) struct OutlineRaster {
    pub(super) width: u32,
    pub(super) height: u32,
    /// 表示 1px あたりの画素数。通常は超解像の倍率で、大きなグリフでは小さくなる。
    pub(super) pixels_per_px: f32,
    /// pen 位置・基準線からの左上（表示 px）。
    pub(super) offset_x: f32,
    pub(super) offset_y: f32,
    pub(super) pixels: Vec<u8>,
}

/// 超解像の倍率でラスタライズする。1辺が `max_extent` を超える場合は倍率を下げる。
pub(super) fn rasterize_outline_glyph(
    face: &TextFace,
    glyph_id: GlyphId,
    scale: PxScale,
    max_extent: u32,
) -> Option<OutlineRaster> {
    let font = face.font();
    let mut pixels_per_px = VECTOR_TEXT_SUPERSAMPLE_SCALE;
    // 縮小後の丸めで上限を1px超える場合に備え、最大2回まで測り直す。
    for _ in 0..3 {
        let raster_scale = PxScale { x: scale.x * pixels_per_px, y: scale.y * pixels_per_px };
        let baseline_y = font.as_scaled(raster_scale).ascent();
        let glyph = Glyph { id: glyph_id, scale: raster_scale, position: point(0.0, baseline_y) };
        let outlined = font.outline_glyph(glyph)?;
        let bounds = outlined.px_bounds();
        let width = bounds.width().ceil().max(0.0) as u32;
        let height = bounds.height().ceil().max(0.0) as u32;
        if width == 0 || height == 0 {
            return None;
        }
        if width.max(height) > max_extent {
            let extent = bounds.width().max(bounds.height());
            pixels_per_px *= (max_extent as f32 - 1.0).max(1.0) / extent;
            continue;
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
        return Some(OutlineRaster {
            width,
            height,
            pixels_per_px,
            offset_x: bounds.min.x / pixels_per_px,
            offset_y: (bounds.min.y - baseline_y) / pixels_per_px,
            pixels,
        });
    }
    None
}

impl TextAtlasCache {
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
            detail_bits: 0,
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }
        if self.is_deferred(&key) {
            return None;
        }
        let max = self.max_glyph_extent() as f32;
        let pixels_per_px = VECTOR_TEXT_SUPERSAMPLE_SCALE.min(max / width.max(height));
        let pixel_width = (width * pixels_per_px).round().clamp(2.0, max.max(2.0)) as u32;
        let pixel_height = (height * pixels_per_px).round().clamp(2.0, max.max(2.0)) as u32;
        let pixels = tofu_pixels(pixel_width, pixel_height);
        self.insert_glyph_pixels(key, pixel_width, pixel_height, width, height, 0.0, 0.0, pixels)
    }

    pub(super) fn cached_bitmap_glyph(
        &mut self,
        font_id: Arc<str>,
        ch: char,
        glyph: crate::bitmap_font::BitmapFontGlyph,
        page: &crate::bitmap_font::BitmapFontPage,
        font: &BitmapFont,
        scale: PxScale,
    ) -> Option<CachedGlyph> {
        let key = TextGlyphKey {
            kind: TextGlyphKind::Bitmap,
            font_id,
            glyph: u32::from(ch),
            scale_x_bits: scale.x.to_bits(),
            scale_y_bits: scale.y.to_bits(),
            detail_bits: 0,
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }
        if self.failed.contains(&key) || self.is_deferred(&key) {
            return None;
        }

        let display_width = (glyph.width as f32 * scale.x).ceil().max(1.0);
        let display_height = (glyph.height as f32 * scale.y).ceil().max(1.0);
        // atlas に収まらない倍率では、表示寸法を保ったまま標本化の解像度を下げる。
        let max = self.max_glyph_extent() as f32;
        let fit = (max / display_width.max(display_height)).min(1.0);
        let raster_scale = PxScale { x: scale.x * fit, y: scale.y * fit };
        let width = ((glyph.width as f32 * raster_scale.x).ceil().clamp(1.0, max)) as u32;
        let height = ((glyph.height as f32 * raster_scale.y).ceil().clamp(1.0, max)) as u32;
        let pixels = rasterized_bitmap_glyph_pixels(glyph, page, raster_scale, width, height);

        self.insert_glyph_pixels(
            key,
            width,
            height,
            display_width,
            display_height,
            glyph.xoffset as f32 * scale.x,
            (glyph.yoffset as f32 - font.ascent) * scale.y,
            pixels,
        )
    }

    pub(super) fn insert_glyph_pixels(
        &mut self,
        key: TextGlyphKey,
        width: u32,
        height: u32,
        display_width: f32,
        display_height: f32,
        offset_x: f32,
        offset_y: f32,
        pixels: Vec<u8>,
    ) -> Option<CachedGlyph> {
        let Some(atlas_origin) = self.reserve(width, height) else {
            // 空きが無いだけなら、作り直した atlas で再試行する。どちらの場合も、
            // atlas を作り直すまでは同じグリフを再試行しない。
            if self.fits_empty_atlas(width, height) {
                self.defer(key);
            } else {
                self.mark_failed(key);
            }
            return None;
        };
        self.blit_region(atlas_origin, width, height, &pixels);
        self.dirty_regions.push(padded_dirty_region(atlas_origin, width, height, &pixels));
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
        Some(cached)
    }

    /// パディングを含めて atlas の幅と高さの上限に収まる位置。収まらなければ None。
    pub(super) fn reserve(&mut self, glyph_width: u32, glyph_height: u32) -> Option<(u32, u32)> {
        if !self.fits_empty_atlas(glyph_width, glyph_height) {
            return None;
        }
        let padded_width = glyph_width + TEXT_ATLAS_PADDING * 2;
        let padded_height = glyph_height + TEXT_ATLAS_PADDING * 2;
        let (mut pen_x, mut pen_y, mut row_height) = (self.pen_x, self.pen_y, self.row_height);
        if pen_x + padded_width > self.width {
            pen_x = 0;
            pen_y += row_height;
            row_height = 0;
        }
        row_height = row_height.max(padded_height);
        if pen_y + row_height > self.max_height {
            self.overflowed = true;
            return None;
        }

        let origin = (pen_x + TEXT_ATLAS_PADDING, pen_y + TEXT_ATLAS_PADDING);
        self.pen_x = pen_x + padded_width;
        self.pen_y = pen_y;
        self.row_height = row_height;
        self.ensure_height(self.pen_y + self.row_height);
        self.grow_texture(self.pen_y + self.row_height);
        Some(origin)
    }

    /// パディングを含めて空の atlas に収まる寸法か。
    fn fits_empty_atlas(&self, glyph_width: u32, glyph_height: u32) -> bool {
        let fits = |extent: u32, limit: u32| {
            extent.checked_add(TEXT_ATLAS_PADDING * 2).is_some_and(|padded| padded <= limit)
        };
        fits(glyph_width, self.width) && fits(glyph_height, self.max_height)
    }

    /// 格納済みの高さが GPU テクスチャを超えたら、上限まで倍々に広げる。
    fn grow_texture(&mut self, height: u32) {
        if height <= self.texture_height {
            return;
        }
        let doubled = height.max(TEXT_ATLAS_MIN_TEXTURE_HEIGHT).next_power_of_two();
        self.texture_height = doubled.min(self.max_height).max(height);
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

/// パディングを含めた転送領域。作り直した atlas では GPU テクスチャに古いグリフが残るので、
/// 線形補間で読む周囲1pxも透明で書き直し、テクスチャ全体を転送せずに済ませる。
fn padded_dirty_region(
    atlas_origin: (u32, u32),
    width: u32,
    height: u32,
    pixels: &[u8],
) -> TextAtlasDirtyRegion {
    let padded = AtlasSize {
        width: width + TEXT_ATLAS_PADDING * 2,
        height: height + TEXT_ATLAS_PADDING * 2,
    };
    let mut region = vec![0; (padded.width * padded.height * 4) as usize];
    let row = (width * 4) as usize;
    for y in 0..height {
        let src = (y * width * 4) as usize;
        let dst = (((y + TEXT_ATLAS_PADDING) * padded.width + TEXT_ATLAS_PADDING) * 4) as usize;
        if let (Some(dst), Some(src)) = (region.get_mut(dst..dst + row), pixels.get(src..src + row))
        {
            dst.copy_from_slice(src);
        }
    }
    TextAtlasDirtyRegion {
        origin: (atlas_origin.0 - TEXT_ATLAS_PADDING, atlas_origin.1 - TEXT_ATLAS_PADDING),
        size: padded,
        pixels: region,
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

/// 位置を含まないレイアウトのキー。quad は描画位置からの相対座標でキャッシュする。
/// 縁取り・影の重ね描きは本体と別の色・装飾フラグで、それぞれのキーを持つ。
pub(super) fn text_layout_key(
    kind: TextGlyphKind,
    font_id: &str,
    text: &str,
    style: &TextStyle,
    surface: SurfaceSize,
    decoration: bool,
) -> Option<TextLayoutKey> {
    if style.wrapping {
        return None;
    }
    Some(TextLayoutKey {
        kind,
        font_id: font_id.to_string(),
        text: text.to_string(),
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
            decoration,
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
