use super::select_row_banners::SourceRef;
use super::*;

const BANNER_PREFETCH_ROWS: usize = 2;

fn row_banner_indices(
    item_len: usize,
    selected: usize,
    center: usize,
) -> impl Iterator<Item = (usize, bool)> + Clone {
    let selected_visible =
        select_visible_indices_iter(item_len, center, SELECT_VISIBLE_ROWS).any(|i| i == selected);
    std::iter::once((selected, selected_visible)).chain(
        select_visible_indices_iter(
            item_len,
            center,
            SELECT_VISIBLE_ROWS + BANNER_PREFETCH_ROWS * 2,
        )
        .enumerate()
        .map(|(position, index)| {
            (
                index,
                (BANNER_PREFETCH_ROWS..BANNER_PREFETCH_ROWS + SELECT_VISIBLE_ROWS)
                    .contains(&position),
            )
        }),
    )
}

impl WinitApp {
    pub(super) fn sync_select_row_banners(&mut self, enabled: bool) {
        let enabled = enabled
            && !self.select.ir_battle.active
            && !in_settings_stack(&self.select.folder_stack)
            && self
                .renderer
                .select_skin_document()
                .and_then(|document| document.songlist.as_ref())
                .is_some_and(|songlist| !songlist.banner.is_empty());
        if !enabled {
            self.select.select_assets.row_banners.deactivate();
            return;
        }
        let count = self.select.select_items.len();
        let selected = self.select.selected_index;
        let center =
            select_scroll_center_index(selected, count, self.select_bar_scroll_remaining());
        // Match the physically displayed window during long scroll animations.
        // Warm two neighbours on either side without uploading them yet.
        let indices = row_banner_indices(count, selected, center);
        let items = &self.select.select_items;
        let sources = indices.filter_map(|(index, visible)| {
            let SelectItem::Chart(row) = items.get(index)? else {
                return None;
            };
            let chart = row.chart.as_ref()?;
            (!chart.banner_file.is_empty()).then_some((
                SourceRef { folder: &chart.folder_path, file: &chart.banner_file },
                visible,
            ))
        });
        let rows = &mut self.select.select_assets.row_banners;
        rows.reconcile(sources);
        let now = Instant::now();
        rows.tick(now);
        self.collect_select_row_banner_textures();
        if !self.select.select_assets.row_banners.has_pending_upload(now) {
            return;
        }
        let Ok(mut cache) = self.skin.skin_pipeline.gpu_texture_cache.lock() else {
            return;
        };
        let renderer = &mut self.renderer;
        self.select.select_assets.row_banners.upload_ready(now, |image| {
            let (texture, lease) = cache.reserve_chart_texture();
            if let Err(error) = renderer.upsert_image_asset(TextureId(texture.0), image) {
                tracing::debug!(%error, "failed to upload row banner");
                drop(lease);
                if cache.release_chart_texture(texture) {
                    renderer.remove_image_texture(TextureId(texture.0));
                }
                return None;
            }
            Some(bmz_render::scene::SelectRowBannerSnapshot {
                texture,
                size: SkinImageSize { width: image.width as f32, height: image.height as f32 },
                lease,
            })
        });
    }

    pub(super) fn collect_select_row_banner_textures(&mut self) {
        if !self.select.select_assets.row_banners.has_retired() {
            return;
        }
        let Ok(mut cache) = self.skin.skin_pipeline.gpu_texture_cache.lock() else {
            return;
        };
        self.select.select_assets.row_banners.collect_retired(|texture| {
            if cache.release_chart_texture(texture) {
                // Also removes the renderer's filtered/unfiltered bind groups.
                self.renderer.remove_image_texture(TextureId(texture.0));
            }
        });
    }

    pub(super) fn apply_select_row_banners(&self, rows: &mut [SelectRowSnapshot]) {
        if self.select.ir_battle.active || !self.select.select_assets.row_banners.has_demand() {
            return;
        }
        for row in rows {
            let Some(SelectItem::Chart(chart_row)) =
                self.select.select_items.get(row.index as usize)
            else {
                continue;
            };
            let Some(chart) = &chart_row.chart else {
                continue;
            };
            row.banner = self
                .select
                .select_assets
                .row_banners
                .image(SourceRef { folder: &chart.folder_path, file: &chart.banner_file });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_banner_window_matches_snapshot_rows_while_scrolling_and_wrapping() {
        for count in [1, 7, 31, 100] {
            for selected in [0, count / 2, count - 1] {
                for remaining in [0.0, 0.4, 16.4, -16.4] {
                    let center = select_scroll_center_index(selected, count, remaining);
                    let snapshot_rows =
                        select_visible_item_indices(count, center, SELECT_VISIBLE_ROWS);
                    let indices: Vec<_> = row_banner_indices(count, selected, center).collect();
                    assert_eq!(indices[0], (selected, snapshot_rows.contains(&selected)));
                    let displayed: Vec<_> = indices[1..]
                        .iter()
                        .filter_map(|&(index, visible)| visible.then_some(index))
                        .collect();
                    assert_eq!(displayed, snapshot_rows);
                    let warmed: Vec<_> = indices[1..].iter().map(|&(index, _)| index).collect();
                    assert_eq!(
                        warmed,
                        select_visible_item_indices(
                            count,
                            center,
                            SELECT_VISIBLE_ROWS + BANNER_PREFETCH_ROWS * 2
                        )
                    );
                }
            }
        }
    }
}
