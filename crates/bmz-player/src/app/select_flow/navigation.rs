use super::*;

pub(in crate::app) fn select_explorer_path(item: &SelectItem) -> Option<PathBuf> {
    match item {
        SelectItem::Chart(row) => row.chart.as_ref().map(|chart| PathBuf::from(&chart.folder_path)),
        SelectItem::Folder { path, kind: bmz_render::scene::SelectRowKind::Folder, .. } => {
            Some(PathBuf::from(path))
        }
        _ => None,
    }
}

impl WinitApp {
    pub(super) fn move_selection(&mut self, select_move: SelectMove) {
        self.move_selection_with_duration(select_move, self.select_scroll_duration_low());
    }

    pub(super) fn move_selection_with_duration(
        &mut self,
        select_move: SelectMove,
        duration: Duration,
    ) {
        if self.select.ir_battle.active {
            self.move_select_ir_battle(select_move, duration);
            return;
        }
        if self.select.select_items.is_empty() {
            self.reload_select_items();
        }
        if self.select.select_items.is_empty() {
            return;
        }
        let previous_index = self.select.selected_index;
        self.select.selected_index = moved_select_index(
            self.select.selected_index,
            self.select.select_items.len(),
            select_move,
        );
        if self.select.selected_index != previous_index {
            self.sync_selected_play_mode();
            self.reset_selected_replay_slot();
            self.select.select_bar_started_at = Instant::now();
            self.select.select_bar_scroll_direction = select_move_scroll_direction(select_move);
            self.select.select_bar_scroll_duration = duration;
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
    }

    pub(super) fn advance_select_hold_move(&mut self) {
        if !self.ui.focused {
            self.clear_select_hold();
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            self.clear_select_hold();
            return;
        }
        let (Some(select_move), Some(started_at), Some(last_trigger_at)) = (
            self.select.select_hold_move,
            self.select.select_hold_started_at,
            self.select.select_hold_last_trigger_at,
        ) else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(started_at);
        if elapsed < self.select_scroll_duration_low() {
            return;
        }
        let since_last = now.duration_since(last_trigger_at);
        if since_last >= self.select_scroll_duration_high() {
            self.select.select_hold_last_trigger_at = Some(now);
            self.move_selection_with_duration(select_move, self.select_scroll_duration_high());
        }
    }

    pub(super) fn start_select_hold_move(&mut self, select_move: SelectMove, control: String) {
        self.select.select_hold_move = Some(select_move);
        self.select.select_hold_started_at = Some(Instant::now());
        self.select.select_hold_last_trigger_at = Some(Instant::now());
        self.select.select_hold_control = Some(control);
    }

    pub(super) fn clear_select_hold_control(&mut self, control: &str) {
        if self.select.select_hold_control.as_deref() == Some(control) {
            self.clear_select_hold();
        }
    }

    pub(super) fn clear_select_hold(&mut self) {
        self.select.select_hold_move = None;
        self.select.select_hold_started_at = None;
        self.select.select_hold_last_trigger_at = None;
        self.select.select_hold_control = None;
    }

    pub(super) fn open_advanced_settings_from_select(&mut self) {
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.open_advanced_settings();
        }
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!("opened egui advanced settings from select");
    }

    pub(super) fn open_skin_settings_from_select(&mut self) {
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.open_skin_settings();
        }
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!("opened egui skin settings from select");
    }

    pub(super) fn open_key_config_from_select(&mut self) {
        self.set_search_mode(false);
        if !push_key_config_folder_history(
            &mut self.select.folder_stack,
            &mut self.select.selected_index_stack,
            self.select.selected_index,
        ) {
            return;
        }
        self.reload_select_items();
        self.select.selected_index = 0;
        self.reset_selected_replay_slot();
        self.restart_select_bar_timer_without_scroll(Instant::now());
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!("opened key config from select");
    }

    pub(super) fn selected_chart_row(
        &self,
    ) -> Option<&crate::screens::select_model::SelectChartRow> {
        match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => Some(row),
            _ => None,
        }
    }

    pub(super) fn toggle_favorite_chart_selected(&mut self) {
        let Some(row) = self.selected_chart_row().cloned() else {
            return;
        };
        let Some(sha256) = row.score_sha256() else {
            return;
        };
        let hints = favorite_hints_for_row(&row);
        match self.boot.collection_db.toggle_favorite_chart(sha256, &hints, now_unix_seconds()) {
            Ok(enabled) => {
                self.reload_select_items();
                self.restart_select_bar_timer_without_scroll(Instant::now());
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                let text = Localizer::new(self.boot.profile_config.ui.locale());
                self.show_left_overlay_toast(text.text(if enabled {
                    "toast-favorite-chart-added"
                } else {
                    "toast-favorite-chart-removed"
                }));
                tracing::info!(enabled, title = row.display_title(), "favorite chart toggled");
            }
            Err(error) => tracing::error!(%error, "failed to toggle favorite chart"),
        }
    }

    pub(super) fn toggle_favorite_chart_result(&mut self) {
        let Some((sha256, title, artist)) = self.result.finished_play.as_ref().map(|finished| {
            (
                finished.result.chart_sha256,
                finished.summary.title.clone(),
                finished.summary.artist.clone(),
            )
        }) else {
            return;
        };
        let hints = FavoriteHints::new(title.clone(), artist, "");
        match self.boot.collection_db.toggle_favorite_chart(sha256, &hints, now_unix_seconds()) {
            Ok(enabled) => {
                self.result.result_favorite_chart = enabled;
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                let text = Localizer::new(self.boot.profile_config.ui.locale());
                self.show_left_overlay_toast(text.text(if enabled {
                    "toast-favorite-chart-added"
                } else {
                    "toast-favorite-chart-removed"
                }));
                tracing::info!(enabled, %title, "favorite chart toggled from result");
            }
            Err(error) => tracing::error!(%error, "failed to toggle favorite chart from result"),
        }
    }

    pub(super) fn handle_select_open_folder_action(&mut self) {
        let e1_held = self.input.select_e_action_holds.contains(&InputActionConfig::E1);
        let e2_held = self.input.select_e_action_holds.contains(&InputActionConfig::E2);
        let ctrl_held = self.input.pressed_controls.iter().any(|control| {
            matches!(control.as_str(), "LControl" | "RControl" | "ControlLeft" | "ControlRight")
        });
        let shift_held = self.input.pressed_controls.iter().any(|control| {
            matches!(control.as_str(), "LShift" | "RShift" | "ShiftLeft" | "ShiftRight")
        });

        if e1_held {
            self.copy_selected_hash(false);
        } else if e2_held || (ctrl_held && shift_held) {
            self.copy_selected_hash(true);
        } else if ctrl_held {
            self.copy_selected_hash(false);
        } else {
            self.open_selected_chart_folder();
        }
    }

    pub(super) fn copy_selected_hash(&mut self, sha256: bool) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some(row) = self.selected_chart_row().cloned() else {
            return;
        };
        let Some(value) = (if sha256 {
            row.score_sha256().map(|hash| hash_to_hex(&hash))
        } else {
            row.chart.as_ref().map(|chart| hash_to_hex(&chart.md5))
        }) else {
            self.show_left_overlay_toast(text.text(if sha256 {
                "toast-chart-hash-unavailable-sha256"
            } else {
                "toast-chart-hash-md5-local-only"
            }));
            return;
        };
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(value.clone()))
        {
            Ok(()) => {
                self.show_left_overlay_toast(text.text(if sha256 {
                    "toast-chart-hash-copied-sha256"
                } else {
                    "toast-chart-hash-copied-md5"
                }));
                tracing::info!(sha256, hash = %value, "copied chart hash to clipboard");
            }
            Err(error) => {
                tracing::warn!(%error, sha256, "failed to copy chart hash to clipboard");
                self.show_left_overlay_toast(text.text("toast-clipboard-copy-failed"));
            }
        }
    }

    pub(super) fn open_selected_chart_folder(&mut self) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some(path) =
            self.select.select_items.get(self.select.selected_index).and_then(select_explorer_path)
        else {
            return;
        };
        if let Err(error) = open_file_browser_path(&path) {
            tracing::warn!(path = %path.display(), %error, "failed to open selected chart folder");
            self.show_left_overlay_toast(text.text("toast-chart-folder-open-failed"));
        } else {
            tracing::info!(path = %path.display(), "opened selected chart folder");
        }
    }

    pub(super) fn open_selected_chart_documents(&mut self) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some(chart) = self.selected_chart_row().and_then(|row| row.chart.clone()) else {
            return;
        };
        let folder = PathBuf::from(&chart.folder_path);
        let mut opened = 0usize;
        match std::fs::read_dir(&folder) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let is_text = path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"));
                    if is_text && open_file_with_default_app(&path).is_ok() {
                        opened += 1;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(path = %folder.display(), %error, "failed to read chart documents");
            }
        }
        if opened == 0 {
            self.show_left_overlay_toast(text.text("toast-chart-text-not-found"));
        } else {
            let mut args = FluentArgs::new();
            args.set("count", opened as i64);
            self.show_left_overlay_toast(text.format("toast-chart-text-opened", &args));
        }
    }

    pub(super) fn open_primary_ir_for_selected(&mut self) {
        let identity = match self.select.select_items.get(self.select.selected_index) {
            Some(SelectItem::Chart(row)) => {
                let Some(sha256) = row.score_sha256() else {
                    let text = Localizer::new(self.boot.profile_config.ui.locale());
                    self.show_left_overlay_toast(text.text("toast-ir-chart-hash-missing"));
                    return;
                };
                PrimaryIrPageIdentity::Chart { sha256: hash_to_hex(&sha256) }
            }
            Some(SelectItem::Course(row)) => PrimaryIrPageIdentity::Course {
                canonical_hash: row.course_hash.clone(),
                rian_hash_v1: row.rian_course_hash_v1.clone(),
                bms_ir_course_key: row.bms_ir_course_key.clone(),
            },
            _ => return,
        };
        self.open_primary_ir_page(identity);
    }

    pub(super) fn open_primary_ir_page(&mut self, identity: PrimaryIrPageIdentity) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some(provider) = primary_ir_provider_for_profile(&self.boot.profile_config) else {
            self.show_left_overlay_toast(text.text("toast-primary-ir-not-configured"));
            return;
        };
        let url = match primary_ir_page_url(provider, &identity) {
            Ok(url) => url,
            Err(error) => {
                tracing::warn!(%error, "failed to build primary IR page URL");
                self.show_left_overlay_toast(text.text("toast-primary-ir-open-failed"));
                return;
            }
        };
        match open_external_url(&url) {
            Ok(()) => {
                self.show_left_overlay_toast(text.text("toast-primary-ir-opened"));
                tracing::info!(%url, "opened primary IR page");
            }
            Err(error) => {
                tracing::warn!(%error, %url, "failed to open primary IR page");
                self.show_left_overlay_toast(text.text("toast-primary-ir-open-failed"));
            }
        }
    }

    pub(super) fn open_selected_chart_download_sites(&mut self) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some(row) = self.selected_chart_row() else {
            return;
        };
        let urls = crate::song_download::validated_browser_urls([
            &row.download_metadata.url,
            &row.download_metadata.append_url,
        ]);
        if urls.is_empty() {
            self.show_left_overlay_toast(text.text("toast-chart-source-unavailable"));
            return;
        }
        match open_browser_urls(&urls) {
            Ok(count) => {
                let mut args = FluentArgs::new();
                args.set("count", count as i64);
                self.show_left_overlay_toast(text.format("toast-chart-sources-opened", &args));
            }
            Err(error) => {
                tracing::error!(%error, "failed to open selected chart URLs");
                self.show_left_overlay_toast(text.text("toast-chart-sources-open-failed"));
            }
        }
    }

    pub(super) fn start_autoplay_folder_selected(&mut self) {
        if self.select.course_builder.is_some() {
            self.show_select_course_builder_chart_required();
            return;
        }
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some((path, kind)) =
            self.select.select_items.get(self.select.selected_index).and_then(|item| match item {
                SelectItem::Folder { path, kind, .. } => Some((path.clone(), *kind)),
                _ => None,
            })
        else {
            return;
        };
        if kind != bmz_render::scene::SelectRowKind::Folder {
            self.show_left_overlay_toast(text.text("toast-folder-autoplay-only-normal-folder"));
            return;
        }
        let mut folder_paths = vec![path.clone()];
        match self.boot.library_db.list_descendant_folder_paths(&path) {
            Ok(descendants) => folder_paths.extend(descendants),
            Err(error) => {
                tracing::warn!(folder = %path, %error, "failed to list autoplay folder descendants");
            }
        }
        let folder_refs: Vec<&str> = folder_paths.iter().map(String::as_str).collect();
        let charts = match self.boot.library_db.list_charts_in_folders(&folder_refs) {
            Ok(charts) => charts,
            Err(error) => {
                tracing::warn!(folder = %path, %error, "failed to list autoplay folder charts");
                self.show_left_overlay_toast(text.text("toast-folder-autoplay-charts-load-failed"));
                return;
            }
        };
        let mut chart_ids = Vec::with_capacity(charts.len());
        let mut seen = HashSet::new();
        for chart in charts {
            if seen.insert(chart.chart_id) {
                chart_ids.push(chart.chart_id);
            }
        }
        let Some(&first_chart_id) = chart_ids.first() else {
            self.show_left_overlay_toast(text.text("toast-folder-autoplay-empty"));
            return;
        };
        self.clear_active_course_state();
        self.select.autoplay_folder = Some(AutoplayFolderSession { chart_ids, next_index: 1 });
        let mut options = self.play_start_options();
        options.session_mode = SessionMode::Autoplay;
        options.autoplay = true;
        self.begin_decide_for_chart(first_chart_id, options);
        self.show_left_overlay_toast(text.text("toast-folder-autoplay-started"));
        tracing::info!(folder = %path, first_chart_id, "started folder autoplay");
    }

    pub(super) fn toggle_favorite_song_selected(&mut self) {
        let Some(row) = self.selected_chart_row().cloned() else {
            return;
        };
        let Some(sha256) = row.score_sha256() else {
            return;
        };
        let representatives = match row.chart.as_ref() {
            Some(chart) => favorite_song_representatives_for_folder(
                &self.boot.library_db,
                &self.boot.collection_db,
                &chart.folder_path,
            )
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to resolve favorite song folders");
                Vec::new()
            }),
            None => Vec::new(),
        };
        let hints = favorite_hints_for_row(&row);
        let result = if representatives.is_empty() {
            self.boot.collection_db.toggle_favorite_song(sha256, &hints, now_unix_seconds())
        } else {
            self.boot.collection_db.remove_favorite_songs(&representatives).map(|_| false)
        };
        match result {
            Ok(enabled) => {
                self.reload_select_items();
                self.restart_select_bar_timer_without_scroll(Instant::now());
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                let text = Localizer::new(self.boot.profile_config.ui.locale());
                self.show_left_overlay_toast(text.text(if enabled {
                    "toast-favorite-song-added"
                } else {
                    "toast-favorite-song-removed"
                }));
                tracing::info!(enabled, title = row.display_title(), "favorite song toggled");
            }
            Err(error) => tracing::error!(%error, "failed to toggle favorite song"),
        }
    }

    pub(super) fn open_same_folder_for_selected(&mut self) {
        let selected = self.select.select_items.get(self.select.selected_index).cloned();
        let (path, description) = match selected {
            Some(SelectItem::Chart(row)) => {
                let Some(chart) = row.chart else {
                    return;
                };
                let folder_path = chart.folder_path;
                (same_folder_path(&folder_path), folder_path)
            }
            Some(SelectItem::Course(row)) => {
                (course_contents_path(row.course_id), row.title.clone())
            }
            _ => return,
        };
        self.select.selected_index_stack.push(self.select.selected_index);
        self.select.folder_stack.push(path);
        self.reload_select_items();
        self.select.selected_index = 0;
        self.reset_selected_replay_slot();
        self.restart_select_bar_timer_without_scroll(Instant::now());
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!(target = %description, "entered related chart view");
    }

    pub(super) fn start_random_select(&mut self, chart_ids: &[i64]) {
        if chart_ids.is_empty() {
            return;
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let index = (nanos % chart_ids.len() as u128) as usize;
        self.start_chart(chart_ids[index]);
    }

    pub(super) fn enter_or_play_selected(&mut self) {
        if self.select.select_items.is_empty() {
            self.reload_select_items();
        }
        match self.select.select_items.get(self.select.selected_index).cloned() {
            Some(SelectItem::Folder { path, .. }) => {
                // 入る直前のカーソル位置を覚えておき、出た時に復元できるようにする。
                self.select.selected_index_stack.push(self.select.selected_index);
                self.select.folder_stack.push(path);
                self.reload_select_items();
                self.select.selected_index = 0;
                self.reset_selected_replay_slot();
                self.restart_select_bar_timer_without_scroll(Instant::now());
                self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
                tracing::info!(folder = ?self.select.folder_stack.last(), "entered folder");
            }
            Some(SelectItem::Chart(row)) => {
                if self.select.course_builder.is_some() {
                    self.add_chart_to_select_course(&row);
                } else if row.in_library() {
                    self.start_chart(
                        row.chart.as_ref().expect("in_library row has chart").chart_id,
                    );
                } else {
                    self.acquire_missing_chart(&row);
                }
            }
            Some(SelectItem::Course(row)) => {
                if self.select.course_builder.is_some() {
                    self.show_select_course_builder_chart_required();
                } else if row.exists_all_songs() {
                    self.start_course(row.course_id);
                } else {
                    self.acquire_missing_course(&row);
                }
            }
            Some(SelectItem::Executable(row)) => match row.kind {
                SelectExecutableKind::RandomSelect if self.select.course_builder.is_none() => {
                    self.start_random_select(&row.chart_ids)
                }
                SelectExecutableKind::RandomMix if self.select.course_builder.is_none() => {
                    self.start_random_mix()
                }
                SelectExecutableKind::NewCourse if self.select.course_builder.is_none() => {
                    self.begin_select_course_builder()
                }
                SelectExecutableKind::RandomSelect
                | SelectExecutableKind::RandomMix
                | SelectExecutableKind::NewCourse => {
                    self.show_select_course_builder_chart_required()
                }
            },
            Some(SelectItem::Config(_)) if self.select.course_builder.is_some() => {
                self.show_select_course_builder_chart_required();
            }
            Some(SelectItem::Config(_)) => {}
            Some(SelectItem::AppConfig(_)) if self.select.course_builder.is_some() => {
                self.show_select_course_builder_chart_required();
            }
            Some(SelectItem::AppConfig(_)) => {}
            Some(SelectItem::KeyBinding(row)) => {
                if self.select.course_builder.is_some() {
                    self.show_select_course_builder_chart_required();
                } else {
                    self.begin_key_config_edit(row.key_mode, row.target);
                }
            }
            Some(SelectItem::SettingsBack | SelectItem::SettingsClose) => {
                self.exit_folder();
            }
            Some(SelectItem::AdvancedSettings) => {
                if self.select.course_builder.is_some() {
                    self.show_select_course_builder_chart_required();
                } else {
                    self.open_advanced_settings_from_select();
                }
            }
            Some(SelectItem::ApplyAudioSettings) => {
                if self.select.course_builder.is_some() {
                    self.show_select_course_builder_chart_required();
                } else {
                    self.apply_select_audio_settings();
                }
            }
            None => {
                tracing::warn!("no item is available to select");
            }
        }
    }

    pub(super) fn acquire_missing_chart(&mut self, row: &SelectChartRow) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let action =
            choose_missing_chart_action(&self.boot.app_config.downloads, &row.download_metadata);
        match action {
            MissingChartAction::Browser(urls) => match open_browser_urls(&urls) {
                Ok(count) => {
                    let mut args = FluentArgs::new();
                    args.set("count", count as i64);
                    self.show_left_overlay_toast(text.format("toast-chart-sources-opened", &args));
                    tracing::info!(title = row.display_title(), count, "opened missing chart URLs");
                }
                Err(error) => {
                    self.show_left_overlay_toast(text.text("toast-chart-sources-open-failed"));
                    tracing::error!(%error, title = row.display_title(), "failed to open chart URLs");
                }
            },
            MissingChartAction::Unavailable => {
                self.show_left_overlay_toast(text.text("toast-chart-source-unavailable"));
                tracing::info!(
                    title = row.display_title(),
                    "missing chart has no available acquisition source"
                );
            }
            action @ (MissingChartAction::Ipfs { .. } | MissingChartAction::Http { .. }) => {
                self.spawn_chart_download(action, row.display_title().to_string());
            }
        }
    }

    pub(super) fn acquire_missing_course(
        &mut self,
        row: &crate::screens::select_model::SelectCourseRow,
    ) {
        let items = match load_select_items_for_course_contents(
            &self.boot.library_db,
            &self.boot.score_db,
            row.course_id,
            self.boot.profile_config.play.ln_mode_policy,
            self.boot.profile_config.play.rule_mode,
        ) {
            Ok(items) => items,
            Err(error) => {
                self.show_left_overlay_toast(
                    Localizer::new(self.boot.profile_config.ui.locale())
                        .text("toast-chart-source-unavailable"),
                );
                tracing::error!(%error, course_id = row.course_id, "failed to load missing course stages");
                return;
            }
        };
        let mut downloads = Vec::new();
        let mut browser_urls = Vec::new();
        let mut unavailable = 0usize;
        for chart in items.into_iter().filter_map(|item| match item {
            SelectItem::Chart(chart) if !chart.in_library() => Some(chart),
            _ => None,
        }) {
            match choose_missing_chart_action(
                &self.boot.app_config.downloads,
                &chart.download_metadata,
            ) {
                action @ (MissingChartAction::Ipfs { .. } | MissingChartAction::Http { .. }) => {
                    downloads.push((action, chart.display_title().to_string()));
                }
                MissingChartAction::Browser(urls) => browser_urls.extend(urls),
                MissingChartAction::Unavailable => unavailable += 1,
            }
        }

        let text = Localizer::new(self.boot.profile_config.ui.locale());
        if !browser_urls.is_empty() {
            match open_browser_urls(&browser_urls) {
                Ok(count) => {
                    let mut args = FluentArgs::new();
                    args.set("count", count as i64);
                    self.show_left_overlay_toast(text.format("toast-chart-sources-opened", &args));
                }
                Err(error) => {
                    self.show_left_overlay_toast(text.text("toast-chart-sources-open-failed"));
                    tracing::error!(%error, course_id = row.course_id, "failed to open course chart URLs");
                }
            }
        }
        if downloads.is_empty() {
            if browser_urls.is_empty() {
                self.show_left_overlay_toast(text.text("toast-chart-source-unavailable"));
            }
            tracing::info!(
                course_id = row.course_id,
                browser_sources = browser_urls.len(),
                unavailable,
                "handled missing course stage acquisition"
            );
            return;
        }
        tracing::info!(
            course_id = row.course_id,
            downloads = downloads.len(),
            browser_sources = browser_urls.len(),
            unavailable,
            "starting missing course stage acquisition"
        );
        self.spawn_chart_download_batch(downloads);
    }

    pub(super) fn spawn_chart_download(&mut self, action: MissingChartAction, title: String) {
        self.spawn_chart_download_batch(vec![(action, title)]);
    }

    pub(super) fn spawn_chart_download_batch(
        &mut self,
        downloads: Vec<(MissingChartAction, String)>,
    ) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        if self.jobs.pending_chart_download.is_some() {
            self.show_left_overlay_toast(text.text("toast-chart-download-in-progress"));
            return;
        }
        let mut source_names = HashSet::new();
        let requests: Vec<ChartDownloadRequest> = downloads
            .into_iter()
            .filter_map(|(action, title)| {
                match &action {
                    MissingChartAction::Ipfs { .. } => {
                        source_names.insert("IPFS");
                    }
                    MissingChartAction::Http { .. } => {
                        source_names.insert("HTTP");
                    }
                    MissingChartAction::Browser(_) | MissingChartAction::Unavailable => {
                        return None;
                    }
                }
                Some(ChartDownloadRequest {
                    action,
                    title,
                    data_dir: self.boot.app_paths.data_dir.clone(),
                })
            })
            .collect();
        if requests.is_empty() {
            return;
        }
        let request_count = requests.len();
        let source_name = if source_names.len() == 1 {
            source_names.into_iter().next().unwrap_or("HTTP").to_string()
        } else {
            "IPFS / HTTP".to_string()
        };
        let (tx, rx) = mpsc::channel();
        thread::Builder::new()
            .name("chart-download".to_string())
            .spawn(move || {
                let result = (|| -> Result<ChartDownloadBatchResult> {
                    let runtime =
                        tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                    Ok(runtime.block_on(download_charts(requests)))
                })();
                let _ = tx.send(result);
            })
            .expect("failed to spawn chart download thread");
        self.jobs.pending_chart_download = Some(rx);
        if request_count == 1 {
            let mut args = FluentArgs::new();
            args.set("source", source_name.as_str());
            self.show_left_overlay_toast(text.format("toast-chart-download-started", &args));
        } else {
            let mut args = FluentArgs::new();
            args.set("count", request_count as i64);
            self.show_left_overlay_toast(text.format("toast-course-download-started", &args));
        }
        tracing::info!(source = source_name, count = request_count, "started chart downloads");
    }

    pub(super) fn poll_pending_chart_download(&mut self) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let Some(rx) = &self.jobs.pending_chart_download else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(result)) => {
                self.jobs.pending_chart_download = None;
                let completed_count = result.completed.len();
                let failed_count = result.failures.len();
                for failure in result.failures {
                    tracing::error!(
                        title = failure.title,
                        error = %format_error_chain(&failure.error),
                        "chart download failed"
                    );
                }
                if completed_count == 0 {
                    if failed_count == 1 {
                        self.show_left_overlay_toast(text.text("toast-chart-download-failed"));
                    } else {
                        let mut args = FluentArgs::new();
                        args.set("count", failed_count as i64);
                        self.show_left_overlay_toast(
                            text.format("toast-course-download-failed", &args),
                        );
                    }
                    return;
                }

                let mut roots = HashSet::new();
                for completed in &result.completed {
                    roots.insert(completed.root_dir.clone());
                    tracing::info!(
                        source = completed.source.display_name(),
                        path = %completed.chart_dir.display(),
                        "chart download complete"
                    );
                }
                if completed_count == 1 && failed_count == 0 {
                    let source_name = result.completed[0].source.display_name();
                    let mut args = FluentArgs::new();
                    args.set("source", source_name);
                    self.show_left_overlay_toast(
                        text.format("toast-chart-download-complete-registering", &args),
                    );
                } else {
                    let mut args = FluentArgs::new();
                    args.set("completed", completed_count as i64);
                    args.set("failed", failed_count as i64);
                    self.show_left_overlay_toast(
                        text.format("toast-course-download-complete-registering", &args),
                    );
                }
                let scan_roots = roots
                    .into_iter()
                    .map(|root| PathEntry {
                        path: root.to_string_lossy().into_owned(),
                        enabled: true,
                        recursive: true,
                    })
                    .collect();
                self.spawn_song_scan(scan_roots, true, "course chart download scan".to_string());
            }
            Ok(Err(error)) => {
                self.jobs.pending_chart_download = None;
                self.show_left_overlay_toast(text.text("toast-chart-download-failed"));
                tracing::error!(error = %format_error_chain(&error), "chart download failed");
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.jobs.pending_chart_download = None;
                self.show_left_overlay_toast(text.text("toast-chart-download-worker-ended"));
                tracing::warn!("chart download worker disconnected");
            }
        }
    }

    /// Returns true when the event was consumed by the search input layer
    /// (either because the user is in search mode or pressed the search-toggle
    /// hotkey), which suppresses normal m-select navigation for this event.
    /// Applies a winit IME event (Preedit / Commit / Enabled / Disabled) to the
    /// search query state. Only acts while the user is in search mode on the
    /// select screen — IME events received otherwise are ignored.
    pub(super) fn route_ime_event(&mut self, ime: &winit::event::Ime) {
        if !matches!(self.view_state(), AppViewState::Select) || !self.select.search.is_active() {
            return;
        }
        self.select.search.apply_ime(ime);
    }

    /// Toggles search mode and synchronizes IME enablement on the window.
    /// IME is only enabled while search mode is active to avoid macOS / Linux
    /// IMEs swallowing gameplay keypresses.
    pub(super) fn set_search_mode(&mut self, enabled: bool) {
        if enabled && in_settings_stack(&self.select.folder_stack) {
            return;
        }
        self.select.search.set_active(enabled);
        if let Some(window) = self.window.as_ref() {
            window.set_ime_allowed(enabled);
        }
        if enabled {
            self.update_search_ime_cursor_area();
        }
    }

    /// Positions the OS IME candidate window over the search input region of
    /// the active select skin (beatoraja `STRING_SEARCHWORD`, ref=30). No-op
    /// when not in search mode or when the skin does not define such a text
    /// element. Pixel coords are derived from the current window size and the
    /// skin canvas; letterboxing is approximated by direct proportional scale,
    /// which is close enough for IME candidate positioning.
    pub(super) fn update_search_ime_cursor_area(&self) {
        if !self.select.search.is_active() {
            return;
        }
        let Some(window) = self.window.as_ref() else { return };
        let snapshot = self.select_snapshot();
        let Some(rect) = self.renderer.select_skin_search_input_rect(&snapshot) else {
            return;
        };
        // egui_winit と同じ規約で物理ピクセル top-left を渡す。winit 側で各
        // バックエンドの座標系 (macOS は内部で `to_logical`) に変換される。
        let size = window.inner_size();
        let width = size.width as f32;
        let height = size.height as f32;
        let x = (rect.x * width).round() as i32;
        let y = (rect.y * height).round() as i32;
        let w = (rect.width * width).round().max(1.0) as u32;
        let h = (rect.height * height).round().max(1.0) as u32;
        window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(x, y),
            winit::dpi::PhysicalSize::new(w, h),
        );
    }

    pub(super) fn handle_search_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        match self.select.search.handle_key(
            event,
            self.select_e_action_held(),
            in_settings_stack(&self.select.folder_stack),
        ) {
            SearchInputAction::Ignored => false,
            SearchInputAction::Consumed => true,
            SearchInputAction::CursorMoved => {
                self.update_search_ime_cursor_area();
                true
            }
            SearchInputAction::EnterMode => {
                self.set_search_mode(true);
                tracing::info!("entered song search mode");
                true
            }
            SearchInputAction::ExitMode => {
                self.set_search_mode(false);
                tracing::info!("exited song search mode");
                true
            }
            SearchInputAction::Execute => {
                self.execute_song_search();
                true
            }
        }
    }

    /// Runs the current `search_query` against the library DB. On hit: appends
    /// to history (dedupe + bounded), pushes a virtual folder onto the stack,
    /// and exits search mode. On miss: leaves the query intact and updates the
    /// feedback message.
    pub(super) fn execute_song_search(&mut self) {
        let query = self.select.search.trimmed_query();
        if query.is_empty() {
            return;
        }
        let hit_count = match self.boot.library_db.search_charts(&query) {
            Ok(charts) => charts.len(),
            Err(error) => {
                tracing::error!(%error, %query, "song search failed");
                0
            }
        };
        if hit_count == 0 {
            // クエリをクリアして次入力を待つ。display_search_word はクエリ空 +
            // メッセージ有りの組み合わせで "no song found" を流す。
            self.select.search.set_no_results(
                Localizer::new(self.boot.profile_config.ui.locale())
                    .text("select-search-no-results"),
            );
            tracing::info!(%query, "song search returned no results");
            return;
        }

        self.select.search.record_successful_query(query.clone());

        self.set_search_mode(false);
        let mut args = FluentArgs::new();
        args.set("count", hit_count as i64);
        self.select.search.set_message(
            Localizer::new(self.boot.profile_config.ui.locale())
                .format("select-search-results", &args),
        );

        // 検索結果フォルダへ入る。`enter_or_play_selected` と同じ流儀でカーソル
        // 位置を退避してから push する。
        self.select.selected_index_stack.push(self.select.selected_index);
        self.select.folder_stack.push(format!("{SEARCH_PATH_PREFIX}{query}"));
        self.reload_select_items();
        self.select.selected_index = 0;
        self.reset_selected_replay_slot();
        self.restart_select_bar_timer_without_scroll(Instant::now());
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!(%query, hit_count, "entered search result folder");
    }

    pub(super) fn exit_folder(&mut self) {
        if self.select.key_config_edit.is_some() {
            self.cancel_key_config_edit();
        }
        if self.select.settings_edit.is_some() {
            self.cancel_settings_edit();
        }
        if self.select.folder_stack.pop().is_some() {
            let restored = self.select.selected_index_stack.pop().unwrap_or(0);
            self.reload_select_items();
            // 復元先がリスト範囲外なら末尾にクランプする。
            self.select.selected_index =
                restored.min(self.select.select_items.len().saturating_sub(1));
            self.reset_selected_replay_slot();
            self.restart_select_bar_timer_without_scroll(Instant::now());
            self.play_system_sound(crate::system_sound::SoundType::FolderClose);
            tracing::info!(depth = self.select.folder_stack.len(), "exited folder");
        } else if self.select.course_builder.is_some() {
            self.cancel_select_course_builder();
        }
    }
}

pub(in crate::app) fn push_key_config_folder_history(
    folder_stack: &mut Vec<String>,
    selected_index_stack: &mut Vec<usize>,
    selected_index: usize,
) -> bool {
    if folder_stack.last().is_some_and(|path| path == CONFIG_KEYS_PATH) {
        return false;
    }
    selected_index_stack.push(selected_index);
    folder_stack.push(CONFIG_KEYS_PATH.to_string());
    true
}
