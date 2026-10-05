use super::*;

pub(super) struct LibrarySettingsActions<'a> {
    pub(super) save_clicked: &'a mut bool,
    pub(super) rescan_clicked: &'a mut bool,
    pub(super) song_scan_requests: &'a mut Vec<SongScanRequest>,
    pub(super) table_fetch_urls: &'a mut Vec<String>,
    pub(super) score_import_request: &'a mut Option<ScoreImportRequest>,
    pub(super) replay_import_request: &'a mut Option<ImportBeatorajaReplaysRequest>,
    pub(super) cancel_replay_import: &'a mut bool,
}

pub(super) fn build_library_settings_sections(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    difficulty_tables: &[DifficultyTableRecord],
    text: Localizer,
    state: &mut SettingsPanelState<'_>,
    actions: LibrarySettingsActions<'_>,
) {
    let LibrarySettingsActions {
        save_clicked,
        rescan_clicked,
        song_scan_requests,
        table_fetch_urls,
        score_import_request,
        replay_import_request,
        cancel_replay_import,
    } = actions;
    SettingsSection::new(SettingsPage::Library, tr!(text, "settings-song-folders"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_song_folders")
        .show(ui, |ui| {
            let mut root_action = None;
            let root_len = config.songs.roots.len();
            for (index, root) in config.songs.roots.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    let folder_name = song_folder_display_name(&root.path);
                    let label_width = (settings_list_label_width(ui)
                        - SETTINGS_LIST_DRAG_HANDLE_WIDTH)
                        .max(SETTINGS_LIST_MIN_LABEL_WIDTH);
                    let (_, dropped) =
                        ui.dnd_drop_zone::<SettingsDragPayload, _>(egui::Frame::NONE, |ui| {
                            let payload =
                                SettingsDragPayload { list: SettingsDragList::SongRoots, index };
                            ui.horizontal(|ui| {
                                settings_drag_handle(ui, payload, text);
                                settings_list_label_with_tooltip(
                                    ui,
                                    &folder_name,
                                    label_width,
                                    &root.path,
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(tr!(text, "common-delete")).clicked() {
                                            root_action = Some(SettingsListAction::Remove(index));
                                        }
                                        if ui
                                            .add_enabled(
                                                root.enabled,
                                                egui::Button::new(tr!(text, "common-reload")),
                                            )
                                            .clicked()
                                        {
                                            song_scan_requests.push(SongScanRequest {
                                                roots: vec![root.clone()],
                                                force: true,
                                                label: "egui song reload".to_string(),
                                            });
                                        }
                                        if ui
                                            .add_enabled(
                                                index + 1 < root_len,
                                                egui::Button::new(tr!(text, "common-move-down")),
                                            )
                                            .clicked()
                                        {
                                            root_action = Some(SettingsListAction::MoveDown(index));
                                        }
                                        if ui
                                            .add_enabled(
                                                index > 0,
                                                egui::Button::new(tr!(text, "common-move-up")),
                                            )
                                            .clicked()
                                        {
                                            root_action = Some(SettingsListAction::MoveUp(index));
                                        }
                                    },
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut root.enabled, tr!(text, "common-enabled"));
                                ui.checkbox(
                                    &mut root.recursive,
                                    tr!(text, "settings-song-recursive"),
                                );
                            });
                        });
                    if egui::DragAndDrop::payload::<SettingsDragPayload>(ui.ctx()).is_some_and(
                        |payload| {
                            payload.list == SettingsDragList::SongRoots && payload.index == index
                        },
                    ) {
                        settings_drag_ghost(
                            ui.ctx(),
                            egui::Id::new(("settings_song_root_ghost", index)),
                            &folder_name,
                            label_width,
                            true,
                            text,
                        );
                    }
                    if let Some(payload) = dropped
                        && payload.list == SettingsDragList::SongRoots
                    {
                        root_action =
                            Some(SettingsListAction::MoveTo { from: payload.index, to: index });
                    }
                    ui.separator();
                });
            }
            if let Some(action) = root_action {
                apply_settings_list_action(&mut config.songs.roots, action);
            }
            if config.songs.roots.is_empty() {
                ui.label(tr!(text, "settings-song-folders-empty"));
            }
            ui.horizontal(|ui| {
                ui.label(tr!(text, "common-path"));
                ui.add(
                    egui::TextEdit::singleline(state.new_root_path)
                        .desired_width(240.0)
                        .hint_text("/path/to/bms"),
                );
            });
            ui.horizontal(|ui| {
                if ui.button(tr!(text, "common-choose-folder")).clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    *state.new_root_path = folder.to_string_lossy().into_owned();
                    state.add_root_error.clear();
                }
                if ui.button(tr!(text, "common-add")).clicked() {
                    let path = state.new_root_path.trim().to_string();
                    if path.is_empty() {
                        *state.add_root_error = tr!(text, "settings-song-path-required");
                    } else {
                        match add_song_root_entry(&mut config.songs.roots, &path, true, true) {
                            Ok(()) => {
                                song_scan_requests.push(SongScanRequest {
                                    roots: vec![PathEntry { path, enabled: true, recursive: true }],
                                    force: false,
                                    label: "egui song load".to_string(),
                                });
                                *save_clicked = true;
                                state.new_root_path.clear();
                                state.add_root_error.clear();
                            }
                            Err(error) => *state.add_root_error = error.to_string(),
                        }
                    }
                }
            });
            if !state.add_root_error.is_empty() {
                ui.colored_label(egui::Color32::RED, state.add_root_error.as_str());
            }
            if ui.button(tr!(text, "settings-library-rescan")).clicked() {
                *rescan_clicked = true;
            }
            ui.label(tr!(text, "settings-song-scan-help"));
        });

    SettingsSection::new(SettingsPage::Library, tr!(text, "settings-scan-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_scan")
        .show(ui, |ui| {
            ui.checkbox(
                &mut config.scan.follow_symlinks,
                tr!(text, "settings-scan-follow-symlinks"),
            );
            ui.checkbox(&mut config.scan.skip_hidden, tr!(text, "settings-scan-skip-hidden"));
            #[cfg(windows)]
            {
                ui.checkbox(
                    &mut config.scan.use_everything,
                    tr!(text, "settings-scan-use-everything"),
                );
                ui.small(tr!(text, "settings-scan-use-everything-help"));
            }
            ui.checkbox(
                &mut config.scan.auto_rescan_on_startup,
                tr!(text, "settings-scan-on-startup"),
            );
            ui.checkbox(
                &mut config.scan.rescan_missing_files,
                tr!(text, "settings-scan-remove-missing"),
            );
        });

    SettingsSection::new(SettingsPage::Select, tr!(text, "settings-select-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_select")
        .show(ui, |ui| {
            ui.checkbox(
                &mut config.select.experimental_detail_options,
                tr!(text, "settings-select-experimental-detail-options"),
            );
            ui.small(tr!(text, "settings-select-experimental-detail-options-help"));
            ui.add(
                egui::Slider::new(&mut config.select.scroll_duration_low_ms, 2..=1000)
                    .text(tr!(text, "settings-select-scroll-initial")),
            );
            ui.add(
                egui::Slider::new(&mut config.select.scroll_duration_high_ms, 1..=1000)
                    .text(tr!(text, "settings-select-scroll-repeat")),
            );
            ui.label(tr!(text, "settings-select-scroll-help"));
        });

    SettingsSection::new(SettingsPage::Tables, tr!(text, "settings-tables-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_tables")
        .show(ui, |ui| {
            ui.checkbox(
                &mut config.tables.auto_fetch_on_startup,
                tr!(text, "settings-tables-fetch-on-startup"),
            );
            let mut table_action = None;
            let table_len = config.tables.sources.len();
            for (index, source) in config.tables.sources.iter_mut().enumerate() {
                ui.push_id(("table_source", index), |ui| {
                    let source_label =
                        difficulty_table_source_label(&source.url, difficulty_tables);
                    let label_width = (ui.available_width()
                        - SETTINGS_TABLE_LIST_BUTTONS_WIDTH
                        - SETTINGS_TABLE_ENABLED_WIDTH
                        - SETTINGS_LIST_DRAG_HANDLE_WIDTH)
                        .max(64.0);
                    let (_, dropped) =
                        ui.dnd_drop_zone::<SettingsDragPayload, _>(egui::Frame::NONE, |ui| {
                            let payload =
                                SettingsDragPayload { list: SettingsDragList::TableSources, index };
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [SETTINGS_TABLE_ENABLED_WIDTH, ui.spacing().interact_size.y],
                                    egui::Checkbox::new(
                                        &mut source.enabled,
                                        tr!(text, "common-enabled"),
                                    ),
                                );
                                settings_drag_handle(ui, payload, text);
                                settings_list_label_with_tooltip(
                                    ui,
                                    &source_label,
                                    label_width,
                                    &source.url,
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(tr!(text, "common-delete")).clicked() {
                                            table_action = Some(SettingsListAction::Remove(index));
                                        }
                                        if ui.button(tr!(text, "common-fetch")).clicked() {
                                            table_fetch_urls.push(source.url.clone());
                                        }
                                        if ui
                                            .add_enabled(
                                                index + 1 < table_len,
                                                egui::Button::new(tr!(text, "common-move-down")),
                                            )
                                            .clicked()
                                        {
                                            table_action =
                                                Some(SettingsListAction::MoveDown(index));
                                        }
                                        if ui
                                            .add_enabled(
                                                index > 0,
                                                egui::Button::new(tr!(text, "common-move-up")),
                                            )
                                            .clicked()
                                        {
                                            table_action = Some(SettingsListAction::MoveUp(index));
                                        }
                                    },
                                );
                            });
                        });
                    if egui::DragAndDrop::payload::<SettingsDragPayload>(ui.ctx()).is_some_and(
                        |payload| {
                            payload.list == SettingsDragList::TableSources && payload.index == index
                        },
                    ) {
                        settings_drag_ghost(
                            ui.ctx(),
                            egui::Id::new(("settings_table_source_ghost", index)),
                            &source_label,
                            label_width,
                            false,
                            text,
                        );
                    }
                    if let Some(payload) = dropped
                        && payload.list == SettingsDragList::TableSources
                    {
                        table_action =
                            Some(SettingsListAction::MoveTo { from: payload.index, to: index });
                    }
                });
            }
            if let Some(action) = table_action {
                apply_settings_list_action(&mut config.tables.sources, action);
            }
            if config.tables.sources.is_empty() {
                ui.label(tr!(text, "settings-tables-empty"));
            }
            let enabled_table_urls: Vec<String> = config
                .tables
                .sources
                .iter()
                .filter(|source| source.enabled)
                .map(|source| source.url.clone())
                .collect();
            if ui
                .add_enabled(
                    !enabled_table_urls.is_empty(),
                    egui::Button::new(tr!(text, "settings-tables-fetch-enabled")),
                )
                .clicked()
            {
                table_fetch_urls.extend(enabled_table_urls);
            }
            ui.horizontal(|ui| {
                ui.label("URL");
                ui.add(
                    egui::TextEdit::singleline(state.new_table_url)
                        .desired_width(300.0)
                        .hint_text("https://.../header.json"),
                );
            });
            if ui.button(tr!(text, "common-add")).clicked() {
                let url = state.new_table_url.trim().to_string();
                match add_difficulty_table_source(&mut config.tables.sources, &url, text) {
                    Ok(()) => {
                        table_fetch_urls.push(url);
                        *save_clicked = true;
                        state.new_table_url.clear();
                        state.add_table_error.clear();
                    }
                    Err(error) => *state.add_table_error = error,
                }
            }
            if !state.add_table_error.is_empty() {
                ui.colored_label(egui::Color32::RED, state.add_table_error.as_str());
            }
            ui.label(tr!(text, "settings-tables-help"));
        });

    SettingsSection::new(SettingsPage::Library, tr!(text, "settings-downloads-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_downloads")
        .show(ui, |ui| {
            ui.label(tr!(text, "settings-downloads-disclaimer"));
            ui.separator();
            ui.checkbox(
                &mut config.downloads.ipfs_enabled,
                tr!(text, "settings-downloads-ipfs-enable"),
            );
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-downloads-ipfs-api-url"));
                ui.add(
                    egui::TextEdit::singleline(&mut config.downloads.ipfs_api_url)
                        .desired_width(360.0)
                        .hint_text("http://127.0.0.1:5001/"),
                );
            });
            ui.label(tr!(
                text,
                "settings-downloads-ipfs-help",
                "cid" => "{cid}"
            ));
            ui.separator();
            ui.checkbox(
                &mut config.downloads.http_enabled,
                tr!(text, "settings-downloads-http-enable"),
            );
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-downloads-http-api-url"));
                ui.add(
                    egui::TextEdit::singleline(&mut config.downloads.http_api_url)
                        .desired_width(360.0)
                        .hint_text("https://example.com/package/{md5}"),
                );
            });
            ui.label(tr!(
                text,
                "settings-downloads-http-help",
                "md5" => "{md5}",
                "sha256" => "{sha256}"
            ));
            ui.label(tr!(text, "settings-downloads-save-path"));
        });

    build_score_import_section(
        ui,
        state.score_import_path,
        state.score_import_kind,
        state.score_import_device_type,
        state.score_import_status,
        state.score_import_error,
        score_import_request,
        text,
    );
    build_replay_import_section(
        ui,
        state.replay_import_path,
        state.replay_import_device_type,
        state.replay_import_overwrite,
        state.replay_import_status,
        state.replay_import_error,
        state.replay_import_progress,
        replay_import_request,
        cancel_replay_import,
        text,
    );
}
