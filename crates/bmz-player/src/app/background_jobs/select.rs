use super::*;

impl WinitApp {
    pub(in crate::app) fn reload_select_items(&mut self) {
        self.change_select_list(SelectListAction::Refresh);
    }

    pub(in crate::app) fn reload_select_items_for_difficulty_filter(&mut self) {
        if self.select.select_difficulty_filter == SelectDifficultyFilter::All {
            self.reload_select_items();
            return;
        }

        let selected_folder =
            self.select.select_items.get(self.select.selected_index).and_then(|item| match item {
                SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.folder_path.clone()),
                _ => None,
            });
        if let Some(folder_path) = selected_folder {
            self.change_select_list(SelectListAction::RefreshSelectedChartFolder(folder_path));
        } else {
            self.reload_select_items();
        }
    }

    pub(in crate::app) fn change_select_list(&mut self, action: SelectListAction) -> bool {
        if !self.select_list_load_allowed() {
            self.select.list_loader.defer(action);
            return true;
        }
        self.select.list_loader.request(super::super::select_list_loader::SelectListRequest {
            action,
            app_config: self.boot.app_config.clone(),
            profile_config: self.boot.profile_config.clone(),
            library_path: self.boot.app_paths.library_db.clone(),
            profile_paths: self.boot.profile_paths.clone(),
            folder_stack: self.select.folder_stack.clone(),
            selected_index_stack: self.select.selected_index_stack.clone(),
            selected_index: self.select.selected_index,
            selected_key: self
                .select
                .select_items
                .get(self.select.selected_index)
                .map(select_item_key),
            history: self.select.search.history().iter().cloned().collect(),
            mode: self.select.select_mode_filter,
            difficulty: self.select.select_difficulty_filter,
            sort: self.select.select_sort,
        });
        true // Accepted; navigation feedback is emitted only after successful installation.
    }

    fn select_list_load_allowed(&self) -> bool {
        self.view_state() == AppViewState::Select && self.jobs.profile_change.is_none()
    }

    pub(in crate::app) fn poll_select_list(&mut self) {
        if !self.select_list_load_allowed() {
            self.select.list_loader.suspend();
        } else if let Some(action) = self.select.list_loader.take_deferred() {
            self.change_select_list(action);
        }
        if let Some((action, result)) = self.select.list_loader.poll() {
            self.apply_loaded_select_list(action, result);
        }
    }

    fn apply_loaded_select_list(
        &mut self,
        action: SelectListAction,
        result: Result<super::super::select_list_loader::LoadedSelectList>,
    ) {
        let started_at = Instant::now();
        let loaded = match result {
            Ok(loaded) => loaded,
            Err(error) => {
                tracing::error!(%error, ?action, "failed to load select list");
                self.show_left_overlay_toast(
                    Localizer::new(self.boot.profile_config.ui.locale())
                        .text("toast-select-folder-load-failed"),
                );
                return;
            }
        };
        let mut prepared = match loaded.prepared {
            Some(prepared) => prepared,
            None => {
                if let SelectListAction::Enter(path) = &action {
                    let text = Localizer::new(self.boot.profile_config.ui.locale());
                    if parse_search_query(path).is_some() && self.select.search.is_active() {
                        self.select.search.set_no_results(text.text("select-search-no-results"));
                    } else {
                        self.show_left_overlay_toast(text.text("toast-select-folder-empty"));
                    }
                }
                return;
            }
        };
        let folder_changed = self.select.folder_stack != prepared.folder_stack;
        if !folder_changed
            && matches!(action, SelectListAction::Refresh | SelectListAction::RefreshDownloads)
        {
            // The cursor can move while the query runs. Preserve that newer selection.
            prepared.selected_index = self.select.selected_index;
            prepared.selected_key =
                self.select.select_items.get(self.select.selected_index).map(select_item_key);
        }
        if !matches!(action, SelectListAction::Refresh | SelectListAction::RefreshDownloads) {
            self.select.selection_generation = self.select.selection_generation.wrapping_add(1);
        }
        if matches!(&action, SelectListAction::Enter(path) if parse_search_query(path).is_some())
            && self.select.search.is_active()
        {
            self.set_search_mode(false);
        }
        if let SelectListAction::Enter(path) = &action
            && let Some(query) = parse_search_query(path)
        {
            self.select.search.record_successful_query(query.to_owned());
        }
        self.select.folder_stack = prepared.folder_stack;
        self.select.virtual_folders = loaded.virtual_folders;
        self.select.selected_index_stack = prepared.selected_index_stack;
        self.select.selected_index = prepared.selected_index;
        // Song scans and table/course updates all converge here. Keep the editor
        // cache until an actual library refresh instead of querying it per frame.
        self.course_editor_cache.invalidate();
        if self.select.score_refresh.take_dirty() {
            self.invalidate_select_folder_summaries();
        }
        self.select.select_folder_summaries.sync_view(&self.select.folder_stack);
        // beatoraja 準拠の自動送りで mode filter が変わることがあるので、
        // 表示状態と永続化用 profile config を実際に適用したモードへ揃える。
        self.select.select_mode_filter = prepared.mode_filter;
        self.boot.profile_config.select.mode_filter = prepared.mode_filter.as_str().to_string();
        let old_items = std::mem::replace(&mut self.select.select_items, prepared.items);
        self.select.list_loader.retire(old_items);
        *self.select.lr2_select_cache.borrow_mut() = Default::default();
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
        } else if folder_changed {
            self.select.search.clear_message();
        }
        self.select.replay_slot_cache.replace(None);
        self.select.selected_index = restored_select_index(
            &self.select.select_items,
            prepared.selected_key.as_ref(),
            self.select.selected_index,
        );
        self.sync_selected_play_mode();
        self.normalize_selected_replay_slot();
        if folder_changed {
            self.clear_select_hold();
            self.reset_selected_replay_slot();
            self.restart_select_bar_timer_without_scroll(Instant::now());
        }
        if prepared.left_empty_folder {
            self.show_left_overlay_toast(
                Localizer::new(self.boot.profile_config.ui.locale())
                    .text("toast-select-folder-empty-returned"),
            );
        }
        match action {
            SelectListAction::Enter(_) => {
                self.play_system_sound(crate::system_sound::SoundType::FolderOpen)
            }
            SelectListAction::Exit => {
                self.play_system_sound(crate::system_sound::SoundType::FolderClose)
            }
            _ => {}
        }
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

    /// Synchronize every configured root. Unless `reparse_all`, unchanged charts are
    /// skipped like beatoraja's library update; the full re-parse is a separate action.
    pub(in crate::app) fn load_songs_and_reload(&mut self, reparse_all: bool) {
        let label = if reparse_all { "library re-parse" } else { "library rescan" };
        self.spawn_song_scan_with_scope(
            Vec::new(),
            reparse_all,
            label.to_string(),
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
        let active_song_roots = enabled_root_paths(&self.boot.app_config);
        self.select.select_folder_summaries.refresh(
            &mut self.select.select_items,
            &visible_indices,
            ln_policy,
            rule_mode,
            &active_song_roots,
        );
    }
}
