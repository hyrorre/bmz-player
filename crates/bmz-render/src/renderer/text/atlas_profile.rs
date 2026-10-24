use super::*;
use std::collections::HashSet;
use std::time::Instant;

pub(super) const RECENT_TEXT_FRAMES: u64 = 3;

#[derive(Debug, Default)]
pub(super) struct TextAtlasProfile {
    pub(super) started: Option<Instant>,
    raster_us: u128,
    raster_count: usize,
    pub(super) layout_hits: usize,
    pub(super) layout_misses: usize,
    pub(super) capacity_misses: u64,
}

impl TextAtlasProfile {
    pub(super) fn begin(capacity_misses: u64) -> Self {
        Self {
            started: tracing::enabled!(target: "bmz_render::text_atlas", tracing::Level::DEBUG)
                .then(Instant::now),
            capacity_misses,
            ..Self::default()
        }
    }
}

impl TextAtlasCache {
    /// 時計を読むのは診断を有効にした場合だけ。通常のキャッシュhitでは呼ばない。
    pub(in crate::renderer) fn raster<T>(&mut self, raster: impl FnOnce(u32) -> T) -> T {
        let start = self.profile.started.map(|_| Instant::now());
        let result = raster(self.max_glyph_extent());
        if let Some(start) = start {
            self.profile.raster_us += start.elapsed().as_micros();
            self.profile.raster_count += 1;
        }
        result
    }

    /// レイアウトhitではグリフの検索を通らない。満杯時だけ、その参照先も合流する。
    pub(super) fn recent_origins(&self) -> HashSet<(u32, u32)> {
        let recent = |frame| self.frame.saturating_sub(frame) <= RECENT_TEXT_FRAMES;
        self.glyphs
            .values()
            .filter(|g| recent(g.last_used_frame))
            .map(|g| g.atlas_origin)
            .chain(
                self.layouts
                    .entries
                    .values()
                    .filter(|l| recent(l.last_used_frame))
                    .flat_map(|l| l.quads.iter().map(|q| q.atlas_origin)),
            )
            .collect()
    }

    pub(super) fn profile_inventory(&self) {
        if self.profile.started.is_none() {
            return;
        }
        let origins = self.recent_origins();
        let mut fonts = HashSet::new();
        let mut characters = HashSet::new();
        let mut scales = HashSet::new();
        let mut kinds = [0usize; 6];
        let mut occupied = 0u64;
        let mut live = 0u64;
        let mut live_count = 0usize;
        for (key, glyph) in &self.glyphs {
            fonts.insert(&key.font_id);
            // ベクターは文字ではなくfont内のglyph ID（合字を含む）。
            characters.insert((&key.font_id, key.glyph));
            scales.insert((key.scale_x_bits, key.scale_y_bits));
            let index = match key.kind {
                TextGlyphKind::Vector => 0,
                TextGlyphKind::Bitmap => 1,
                TextGlyphKind::Tofu => 2,
                TextGlyphKind::Color => 3,
                TextGlyphKind::ColorSilhouette => 4,
                TextGlyphKind::StyledFallback => 5,
            };
            kinds[index] += 1;
            let area = u64::from(glyph.width + 2 * TEXT_ATLAS_PADDING)
                * u64::from(glyph.height + 2 * TEXT_ATLAS_PADDING);
            occupied += area;
            if origins.contains(&glyph.atlas_origin) {
                live += area;
                live_count += 1;
            }
            tracing::trace!(target: "bmz_render::text_atlas", frame = self.frame,
                ?key, width = glyph.width, height = glyph.height,
                recent = origins.contains(&glyph.atlas_origin), "text atlas glyph");
        }
        let decorated_layouts = self.layouts.entries.keys().filter(|k| k.style.decoration).count();
        tracing::debug!(target: "bmz_render::text_atlas", frame = self.frame,
            glyphs = self.glyphs.len(), fonts = fonts.len(), characters = characters.len(),
            scales = scales.len(), ?kinds, decorated_layouts, occupied_pixels = occupied,
            recent_glyphs = live_count, recent_pixels = live,
            capacity_pixels = u64::from(self.width) * u64::from(self.max_height),
            "text atlas inventory");
    }

    pub(in crate::renderer) fn profile_frame(&self, rebuilt: bool, missing_glyphs: u64) {
        let Some(start) = self.profile.started else { return };
        let prepare_us = start.elapsed().as_micros();
        tracing::debug!(target: "bmz_render::text_atlas", frame = self.frame,
            prepare_us, raster_us = self.profile.raster_us, raster_count = self.profile.raster_count,
            layout_and_pack_us = prepare_us.saturating_sub(self.profile.raster_us),
            layout_hits = self.profile.layout_hits, layout_misses = self.profile.layout_misses,
            missing_glyphs, attempted_misses = self.capacity_misses - self.profile.capacity_misses,
            glyphs = self.glyphs.len(), rebuilt, overflowed = self.overflowed,
            recycles = self.recycles, full_clears = self.full_clears,
            cpu_capacity_bytes = self.pixels.capacity(),
            width = self.size().width, height = self.size().height,
            "text atlas frame");
    }
}
