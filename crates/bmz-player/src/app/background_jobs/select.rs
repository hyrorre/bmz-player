use super::*;

impl WinitApp {
    pub(in crate::app) fn reload_select_items(&mut self) {
        let started_at = Instant::now();
        // Song scans and table/course updates all converge here. Keep the editor
        // cache until an actual library refresh instead of querying it per frame.
        self.course_editor_cache.invalidate();
        if self.select.score_refresh.take_dirty() {
            self.invalidate_select_folder_summaries();
        }
        self.select.select_folder_summaries.sync_view(&self.select.folder_stack);
        let previous_selected_key =
            self.select.select_items.get(self.select.selected_index).map(select_item_key);
        let history: Vec<String> = self.select.search.history().iter().cloned().collect();
        let (items, resolved_mode_filter) = load_items_for_stack(
            &self.boot,
            &mut self.select.collection_cache,
            &self.select.folder_stack,
            &history,
            self.select.select_mode_filter,
            self.select.select_difficulty_filter,
            self.select.select_sort,
        );
        // beatoraja 準拠の自動送りで mode filter が変わることがあるので、
        // 表示状態と永続化用 profile config を実際に適用したモードへ揃える。
        self.select.select_mode_filter = resolved_mode_filter;
        self.boot.profile_config.select.mode_filter = resolved_mode_filter.as_str().to_string();
        self.select.select_items = items;
        // Table levels already resolve sources before score/analysis enrichment.
        let table_level = matches!(
            self.select.folder_stack.last().and_then(|path| parse_table_path(path)),
            Some(TablePath::Level { .. })
        );
        let charts: Vec<_> = self
            .select
            .select_items
            .iter()
            .filter_map(|item| match item {
                SelectItem::Chart(row) if !table_level => row.chart.as_ref(),
                _ => None,
            })
            .collect();
        let sources =
            self.boot.library_db.registered_chart_sources(&charts).unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to resolve select chart sources");
                HashMap::new()
            });
        let replacement_ids: Vec<_> = sources
            .iter()
            .filter_map(|(id, source)| {
                (*id != source.chart.chart_id).then_some(source.chart.chart_id)
            })
            .collect();
        let analyses = self
            .boot
            .library_db
            .chart_analysis_summaries_by_chart_ids(&replacement_ids)
            .unwrap_or_default();
        for item in &mut self.select.select_items {
            if let SelectItem::Chart(row) = item
                && let Some(chart) = &row.chart
            {
                match sources.get(&chart.chart_id) {
                    Some(source) if source.chart.chart_id != chart.chart_id => {
                        row.has_document = source.chart.has_document;
                        row.chart_analysis = analyses.get(&source.chart.chart_id).cloned();
                        row.chart = Some(source.chart.clone());
                    }
                    _ => {}
                }
            }
        }
        if self.select.folder_stack.last().and_then(|path| parse_search_query(path)).is_some() {
            let count = self
                .select
                .select_items
                .iter()
                .filter(|item| matches!(item, SelectItem::Chart(_)))
                .count();
            let mut args = FluentArgs::new();
            args.set("count", count as i64);
            self.select.search.set_message(
                Localizer::new(self.boot.profile_config.ui.locale())
                    .format("select-search-results", &args),
            );
        }
        self.select.replay_slot_cache.replace(None);
        self.select.selected_index = restored_select_index(
            &self.select.select_items,
            previous_selected_key.as_ref(),
            self.select.selected_index,
        );
        self.sync_selected_play_mode();
        self.normalize_selected_replay_slot();
        tracing::debug!(target: "bmz_player::select_profile",
            elapsed_us = started_at.elapsed().as_micros(),
            items = self.select.select_items.len(), "select list loaded");
    }

    pub(in crate::app) fn invalidate_select_folder_summaries(&mut self) {
        self.select.select_folder_summaries.invalidate_data();
        self.invalidate_select_distributions();
    }

    pub(in crate::app) fn invalidate_select_distributions(&mut self) {
        self.select
            .select_distributions
            .borrow_mut()
            .invalidate(&mut self.select.select_distribution_cache.borrow_mut());
    }

    pub(in crate::app) fn load_songs_and_reload(&mut self) {
        self.spawn_song_scan_with_scope(
            Vec::new(),
            true,
            "library rescan".to_string(),
            SongScanScope::Library,
        );
    }

    pub(in crate::app) fn refresh_visible_select_folder_summaries(&mut self) {
        let visible_indices = select_visible_item_indices(
            self.select.select_items.len(),
            self.select.selected_index,
            25,
        );
        let ln_policy = self.boot.profile_config.play.ln_mode_policy;
        let rule_mode = self.boot.profile_config.play.rule_mode;
        self.select.select_folder_summaries.refresh(
            &mut self.select.select_items,
            &visible_indices,
            ln_policy,
            rule_mode,
        );
    }
}
