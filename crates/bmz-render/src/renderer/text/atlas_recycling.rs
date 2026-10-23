use super::*;

#[derive(Debug)]
pub(super) struct TextAtlasShelf {
    pub(super) y: u32,
    pub(super) height: u32,
    pub(super) pen_x: u32,
}

impl TextAtlasCache {
    /// GPUへ渡す前だけ行う。最近使った行の座標・画素は変えず、古い行だけを空ける。
    pub(in crate::renderer) fn recycle_inactive_shelves(&mut self) -> bool {
        #[cfg(test)]
        if !self.recycling_enabled {
            return false;
        }
        self.profile_inventory();
        let started = self.profile.started.map(|_| std::time::Instant::now());
        let recent_rows: std::collections::HashSet<_> =
            self.recent_origins().into_iter().map(|(_, y)| y - TEXT_ATLAS_PADDING).collect();
        // 既存の行境界を復元する。再利用済みの空行も残し、行の高さを縮めない。
        let mut rows: HashMap<u32, (u32, u32)> = HashMap::new();
        rows.insert(self.pen_y, (self.row_height, 0));
        for shelf in &self.shelves {
            rows.insert(shelf.y, (shelf.height, 0));
        }
        for glyph in self.glyphs.values() {
            let row = rows.entry(glyph.atlas_origin.1 - TEXT_ATLAS_PADDING).or_default();
            row.0 = row.0.max(glyph.height + TEXT_ATLAS_PADDING * 2);
            row.1 = row.1.max(glyph.atlas_origin.0 + glyph.width + TEXT_ATLAS_PADDING);
        }
        let before = self.glyphs.len();
        // 空きができない場合は従来の全消去・再試行待ちへ戻す。
        if !self
            .glyphs
            .values()
            .any(|g| !recent_rows.contains(&(g.atlas_origin.1 - TEXT_ATLAS_PADDING)))
        {
            return false;
        }
        let mut rows: Vec<_> = rows.into_iter().collect();
        rows.sort_unstable_by_key(|(y, _)| *y);
        self.shelves = rows
            .iter()
            .enumerate()
            .map(|(index, &(y, (height, pen_x)))| TextAtlasShelf {
                y,
                height: rows.get(index + 1).map_or(height, |next| next.0 - y),
                pen_x: if recent_rows.contains(&y) { pen_x } else { 0 },
            })
            .collect();
        // 小さい高さの行から使い、背の高い文字用の空きを残す。
        self.shelves.sort_unstable_by_key(|s| (s.height, s.y));
        self.glyphs
            .retain(|_, glyph| recent_rows.contains(&(glyph.atlas_origin.1 - TEXT_ATLAS_PADDING)));
        // atlas座標は不変。追い出した行を1つでも参照するレイアウトだけを無効化する。
        self.layouts.entries.retain(|_, layout| {
            layout
                .quads
                .iter()
                .all(|q| recent_rows.contains(&(q.atlas_origin.1 - TEXT_ATLAS_PADDING)))
        });
        self.failed.clear();
        self.deferred.clear();
        self.recycling = true;
        self.rebuilt = true;
        self.overflowed = false;
        self.recycles += 1;
        if let Some(started) = started {
            tracing::debug!(target: "bmz_render::text_atlas", frame = self.frame,
                recycle_us = started.elapsed().as_micros(), evicted_glyphs = before - self.glyphs.len(),
                retained_glyphs = self.glyphs.len(), shelves = self.shelves.len(),
                recent_shelves = recent_rows.len(), "text atlas recycling");
        }
        true
    }
}
