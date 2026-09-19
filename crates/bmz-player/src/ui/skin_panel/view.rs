#[derive(Clone)]
pub(in crate::ui) struct SkinEditorState {
    pub(in crate::ui) slot: SkinSlot,
    pub(in crate::ui) search: String,
}

impl Default for SkinEditorState {
    fn default() -> Self {
        Self { slot: SkinSlot::Select, search: String::new() }
    }
}

impl SkinEditorState {
    pub(in crate::ui) fn load(ctx: &egui::Context) -> Self {
        ctx.data_mut(|data| data.get_temp(egui::Id::new("skin_editor")).unwrap_or_default())
    }
    fn store(&self, ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("skin_editor"), self.clone()));
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new(("skin_preferences", self.slot)), self.search.clone())
        });
    }
    pub(in crate::ui) fn matches(&self, name: &str) -> bool {
        name.to_lowercase().contains(&self.search.trim().to_lowercase())
    }
}

impl EguiLayer {
    pub(crate) fn take_skin_catalog_refresh(&self) -> bool {
        self.ctx
            .data_mut(|data| data.remove_temp::<bool>(egui::Id::new("skin_catalog_refresh")))
            .unwrap_or(false)
    }

    pub(crate) fn skin_settings_path<'a>(&self, skin: &'a SkinConfig) -> Option<&'a str> {
        if !self.visible
            || !self.show_settings
            || SettingsNavigation::load(&self.ctx).page != SettingsPage::Skin
        {
            return None;
        }
        Some(match SkinEditorState::load(&self.ctx).slot {
            SkinSlot::Select => &skin.select,
            SkinSlot::Decide => &skin.decide,
            SkinSlot::Play4 => &skin.play4,
            SkinSlot::Play5 => &skin.play5,
            SkinSlot::Play6 => &skin.play6,
            SkinSlot::Play7 => &skin.play7,
            SkinSlot::Play8 => &skin.play8,
            SkinSlot::Play9 => &skin.play9,
            SkinSlot::Play10 => &skin.play10,
            SkinSlot::Play14 => &skin.play14,
            SkinSlot::Battle5 => &skin.battle5,
            SkinSlot::Battle7 => &skin.battle7,
            SkinSlot::Result => &skin.result,
            SkinSlot::CourseResult => &skin.course_result,
        })
    }
}

/// 選択した1スロットのスキンを編集する。
pub(in crate::ui) fn build_skin_panel(
    ui: &mut egui::Ui,
    skin: &mut SkinConfig,
    skin_meta: &SkinConfigMeta,
    skin_catalog: &SkinCatalog,
    app_paths: &AppPaths,
    path_cache: &mut SkinUiPathCache,
    text: Localizer,
) -> SkinPanelActions {
    let save_clicked = false;
    let reset_clicked = false;
    let mut reload = SkinReloadRequest::default();
    let show_bundled_origin = show_bundled_skin_origin(app_paths, skin_catalog);
    if SettingsNavigation::load(ui.ctx()).page != SettingsPage::Skin {
        return SkinPanelActions { save: false, reset: false, reload };
    }
    let mut editor = SkinEditorState::load(ui.ctx());
    let user_root = app_paths.data_dir.join("skins");
    let user_root = std::path::absolute(&user_root).unwrap_or(user_root);
    ui.label(tr!(text, "skin-install-help"));
    ui.label(user_root.display().to_string());
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!(text, "skin-open-user-folder")).clicked() {
            let error = std::fs::create_dir_all(&user_root)
                .map_err(|error| error.to_string())
                .and_then(|()| open_directory(&user_root, text))
                .err();
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("skin_folder_error"), error);
            });
        }
        if ui.button(tr!(text, "skin-refresh-list")).clicked() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("skin_catalog_refresh"), true);
            });
            path_cache.clear();
        }
        if ui.button(tr!(text, "skin-retry")).clicked() {
            request_skin_reload(&mut reload, editor.slot, false);
            ui.ctx().data_mut(|data| {
                data.insert_temp(egui::Id::new("skin_catalog_refresh"), true);
            });
            path_cache.clear();
        }
    });
    if let Some(error) = ui.ctx().data_mut(|data| {
        data.get_temp::<Option<String>>(egui::Id::new("skin_folder_error")).flatten()
    }) {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
    ui.small(tr!(text, "skin-resource-help"));
    ui.separator();
    let previous_slot = editor.slot;
    let groups: &[(&str, &[SkinSlot])] = &[
        (
            "skin-target-common",
            &[SkinSlot::Select, SkinSlot::Decide, SkinSlot::Result, SkinSlot::CourseResult],
        ),
        (
            "skin-target-play",
            &[
                SkinSlot::Play4,
                SkinSlot::Play5,
                SkinSlot::Play6,
                SkinSlot::Play7,
                SkinSlot::Play8,
                SkinSlot::Play9,
                SkinSlot::Play10,
                SkinSlot::Play14,
            ],
        ),
        ("skin-target-battle", &[SkinSlot::Battle5, SkinSlot::Battle7]),
    ];
    egui::ComboBox::new("skin_target", tr!(text, "skin-target"))
        .selected_text(skin_scene_label(editor.slot, text))
        .show_ui(ui, |ui| {
            for (heading, slots) in groups {
                ui.strong(text.text(heading));
                for &slot in *slots {
                    ui.selectable_value(&mut editor.slot, slot, skin_scene_label(slot, text));
                }
                ui.separator();
            }
        });
    if editor.slot != previous_slot {
        let search = ui.ctx().data_mut(|data| {
            data.get_temp(egui::Id::new(("skin_preferences", editor.slot))).unwrap_or_default()
        });
        editor.search = search;
    }
    ui.push_id("skin_selection", |ui| {
        for (slot, candidates) in [
            (SkinSlot::Select, skin_catalog.select.as_slice()),
            (SkinSlot::Decide, skin_catalog.decide.as_slice()),
            (SkinSlot::Play4, skin_catalog.play4.as_slice()),
            (SkinSlot::Play5, skin_catalog.play5.as_slice()),
            (SkinSlot::Play6, skin_catalog.play6.as_slice()),
            (SkinSlot::Play7, skin_catalog.play7.as_slice()),
            (SkinSlot::Play8, skin_catalog.play8.as_slice()),
            (SkinSlot::Play9, skin_catalog.play9.as_slice()),
            (SkinSlot::Play10, skin_catalog.play10.as_slice()),
            (SkinSlot::Play14, skin_catalog.play14.as_slice()),
            (SkinSlot::Battle5, skin_catalog.battle5.as_slice()),
            (SkinSlot::Battle7, skin_catalog.battle7.as_slice()),
            (SkinSlot::Result, skin_catalog.result.as_slice()),
            (SkinSlot::CourseResult, skin_catalog.course_result.as_slice()),
        ] {
            if slot != editor.slot {
                continue;
            }
            if skin_path_combo(
                ui,
                skin,
                slot,
                &tr!(text, "skin-selected"),
                candidates,
                show_bundled_origin,
                text,
            ) {
                request_skin_reload(&mut reload, slot, true);
            }
        }
    });
    if editor.slot != previous_slot || reload.any() {
        // このフレームのメタデータは変更前の対象に対応する。古い定義で
        // options / offsets を正規化せず、次フレームのロードを待つ。
        editor.store(ui.ctx());
        ui.label(tr!(text, "skin-loading-settings"));
        ui.ctx().request_repaint();
        return SkinPanelActions { save: false, reset: false, reload };
    }
    let selected_path = skin_slot_path(skin, editor.slot);
    if !selected_path.trim().is_empty() {
        match app_paths.resolve_path_ref(selected_path) {
            Ok(path) => {
                if app_paths.skin_library_roots().len() > 1 {
                    if path.starts_with(&app_paths.data_dir) {
                        ui.label(tr!(text, "skin-origin-user"));
                    } else if path.starts_with(&app_paths.resource_dir) {
                        ui.label(tr!(text, "skin-origin-bundled"));
                    } else {
                        ui.label(tr!(text, "skin-origin-external"));
                    }
                }
                let path = std::path::absolute(&path).unwrap_or(path);
                ui.label(path.display().to_string());
            }
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            }
        }
    }
    if let Some((path, error)) = &skin_meta.load_error
        && path == selected_path
    {
        ui.colored_label(ui.visuals().error_fg_color, tr!(text, "skin-load-error"));
        ui.label(error);
        editor.store(ui.ctx());
        return SkinPanelActions { save: false, reset: false, reload };
    }
    ui.add(
        egui::TextEdit::singleline(&mut editor.search)
            .hint_text(tr!(text, "skin-search"))
            .desired_width(f32::INFINITY),
    );
    editor.store(ui.ctx());
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt(("skin_details", editor.slot, &editor.search))
        .auto_shrink([false, false])
        .max_height((ui.available_height() - 65.0).max(40.0))
        .show(ui, |ui| match editor.slot {
            SkinSlot::Select => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Select,
                    &skin_meta.select,
                    &skin.select,
                    app_paths,
                    path_cache,
                    &mut skin.select_options,
                    &mut skin.select_files,
                    &mut skin.select_offsets,
                    text,
                );
            }
            SkinSlot::Decide => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Decide,
                    &skin_meta.decide,
                    &skin.decide,
                    app_paths,
                    path_cache,
                    &mut skin.decide_options,
                    &mut skin.decide_files,
                    &mut skin.decide_offsets,
                    text,
                );
            }
            SkinSlot::Play4 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play4,
                    &skin_meta.play4,
                    &skin.play4,
                    app_paths,
                    path_cache,
                    &mut skin.play4_options,
                    &mut skin.play4_files,
                    &mut skin.play4_offsets,
                    text,
                );
            }
            SkinSlot::Play5 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play5,
                    &skin_meta.play5,
                    &skin.play5,
                    app_paths,
                    path_cache,
                    &mut skin.play5_options,
                    &mut skin.play5_files,
                    &mut skin.play5_offsets,
                    text,
                );
            }
            SkinSlot::Play6 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play6,
                    &skin_meta.play6,
                    &skin.play6,
                    app_paths,
                    path_cache,
                    &mut skin.play6_options,
                    &mut skin.play6_files,
                    &mut skin.play6_offsets,
                    text,
                );
            }
            SkinSlot::Play7 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play7,
                    &skin_meta.play7,
                    &skin.play7,
                    app_paths,
                    path_cache,
                    &mut skin.play7_options,
                    &mut skin.play7_files,
                    &mut skin.play7_offsets,
                    text,
                );
            }
            SkinSlot::Play8 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play8,
                    &skin_meta.play8,
                    &skin.play8,
                    app_paths,
                    path_cache,
                    &mut skin.play8_options,
                    &mut skin.play8_files,
                    &mut skin.play8_offsets,
                    text,
                );
            }
            SkinSlot::Play9 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play9,
                    &skin_meta.play9,
                    &skin.play9,
                    app_paths,
                    path_cache,
                    &mut skin.play9_options,
                    &mut skin.play9_files,
                    &mut skin.play9_offsets,
                    text,
                );
            }
            SkinSlot::Play10 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play10,
                    &skin_meta.play10,
                    &skin.play10,
                    app_paths,
                    path_cache,
                    &mut skin.play10_options,
                    &mut skin.play10_files,
                    &mut skin.play10_offsets,
                    text,
                );
            }
            SkinSlot::Play14 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Play14,
                    &skin_meta.play14,
                    &skin.play14,
                    app_paths,
                    path_cache,
                    &mut skin.play14_options,
                    &mut skin.play14_files,
                    &mut skin.play14_offsets,
                    text,
                );
            }
            SkinSlot::Battle5 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Battle5,
                    &skin_meta.battle5,
                    &skin.battle5,
                    app_paths,
                    path_cache,
                    &mut skin.battle5_options,
                    &mut skin.battle5_files,
                    &mut skin.battle5_offsets,
                    text,
                );
            }
            SkinSlot::Battle7 => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Battle7,
                    &skin_meta.battle7,
                    &skin.battle7,
                    app_paths,
                    path_cache,
                    &mut skin.battle7_options,
                    &mut skin.battle7_files,
                    &mut skin.battle7_offsets,
                    text,
                );
            }
            SkinSlot::Result => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::Result,
                    &skin_meta.result,
                    &skin.result,
                    app_paths,
                    path_cache,
                    &mut skin.result_options,
                    &mut skin.result_files,
                    &mut skin.result_offsets,
                    text,
                );
            }
            SkinSlot::CourseResult => {
                build_scene_skin_defs_with_reload(
                    &mut reload,
                    ui,
                    SkinSlot::CourseResult,
                    &skin_meta.course_result,
                    &skin.course_result,
                    app_paths,
                    path_cache,
                    &mut skin.course_result_options,
                    &mut skin.course_result_files,
                    &mut skin.course_result_offsets,
                    text,
                );
            }
        });
    ui.separator();
    ui.label(tr!(text, "settings-autosave-help"));
    SkinPanelActions { save: save_clicked, reset: reset_clicked, reload }
}

#[allow(clippy::too_many_arguments)]
fn build_scene_skin_defs_with_reload(
    reload: &mut SkinReloadRequest,
    ui: &mut egui::Ui,
    slot: SkinSlot,
    defs: &SceneSkinDefs,
    skin_path: &str,
    app_paths: &AppPaths,
    path_cache: &mut SkinUiPathCache,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
    text: Localizer,
) {
    let edit = build_scene_skin_defs(
        ui, slot, defs, skin_path, app_paths, path_cache, options, files, offsets, text,
    );
    if edit.changed {
        request_skin_reload(reload, slot, edit.offsets_changed);
    }
}
use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_option_edits_keep_settings_visible_across_frames() {
        let ctx = egui::Context::default();
        SettingsNavigation::select(&ctx, SettingsPage::Skin);
        SkinEditorState { search: "Layout".into(), ..Default::default() }.store(&ctx);
        let paths =
            AppPaths::from_dirs("resources".into(), "data".into(), "cache".into(), "logs".into());
        let mut skin = SkinConfig::default();
        let meta = SkinConfigMeta {
            select: SceneSkinDefs {
                property: vec![SkinPropertyDef {
                    name: "Layout".into(),
                    def: "On".into(),
                    category: String::new(),
                    item: vec![
                        bmz_render::skin::SkinPropertyItemDef { name: "On".into(), op: 1 },
                        bmz_render::skin::SkinPropertyItemDef { name: "Off".into(), op: 2 },
                    ],
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        for selected in ["On", "Off", "On"] {
            skin.select_options.insert("Layout".into(), selected.into());
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 1600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let actions = build_skin_panel(
                        ui,
                        &mut skin,
                        &meta,
                        &SkinCatalog::default(),
                        &paths,
                        &mut SkinUiPathCache::default(),
                        Localizer::new(AppLocale::En),
                    );
                    assert!(!actions.reload.any());
                },
            );
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.job.text == "Layout")));
            assert_eq!(skin.select_options["Layout"], selected);
            assert_eq!(SkinEditorState::load(&ctx).search, "Layout");
        }
    }

    #[test]
    fn failed_skin_keeps_selected_path_and_all_customization() {
        let ctx = egui::Context::default();
        SettingsNavigation::select(&ctx, SettingsPage::Skin);
        let mut skin =
            SkinConfig { select: "data:skins/missing/select.json".into(), ..Default::default() };
        skin.select_options.insert("Custom".into(), "Saved".into());
        skin.select_files.insert("Font".into(), "custom.ttf".into());
        skin.select_offsets
            .push(SkinOffsetConfig { name: Some("Saved offset".into()), ..Default::default() });
        save_skin_slot_history(&mut skin, SkinSlot::Select);
        let before = skin.clone();
        // An installed fallback's definitions must not normalize the failed skin's options.
        let meta = SkinConfigMeta {
            load_error: Some((skin.select.clone(), "missing file".into())),
            select: SceneSkinDefs::from_play_document(None),
            ..Default::default()
        };
        let paths =
            AppPaths::from_dirs("resources".into(), "data".into(), "cache".into(), "logs".into());
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                let actions = build_skin_panel(
                    ui,
                    &mut skin,
                    &meta,
                    &SkinCatalog::default(),
                    &paths,
                    &mut SkinUiPathCache::default(),
                    Localizer::new(AppLocale::En),
                );
                assert!(!actions.reload.any());
            });
            assert_eq!(
                serde_json::to_value(&skin).unwrap(),
                serde_json::to_value(&before).unwrap()
            );
        }
    }

    #[test]
    fn editing_one_skin_does_not_initialize_other_slots() {
        let ctx = egui::Context::default();
        SettingsNavigation::select(&ctx, SettingsPage::Skin);
        SkinEditorState { slot: SkinSlot::Play7, ..Default::default() }.store(&ctx);
        let defs = SceneSkinDefs {
            property: vec![SkinPropertyDef {
                category: String::new(),
                name: "Lane".into(),
                def: "On".into(),
                item: vec![bmz_render::skin::SkinPropertyItemDef { name: "On".into(), op: 1 }],
            }],
            ..Default::default()
        };
        let meta = SkinConfigMeta { play7: defs.clone(), play14: defs, ..Default::default() };
        let mut skin = SkinConfig::default();
        let paths =
            AppPaths::from_dirs("resources".into(), "data".into(), "cache".into(), "logs".into());
        let mut reload = SkinReloadRequest::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            reload.union(
                build_skin_panel(
                    ui,
                    &mut skin,
                    &meta,
                    &SkinCatalog::default(),
                    &paths,
                    &mut SkinUiPathCache::default(),
                    Localizer::new(AppLocale::En),
                )
                .reload,
            );
        });
        assert_eq!(skin.play7_options.get("Lane").map(String::as_str), Some("On"));
        assert!(skin.play14_options.is_empty());
        assert!(reload.play7);
        assert!(!reload.play14);
    }

    #[test]
    fn skin_search_is_case_insensitive_and_supports_japanese() {
        let mut editor = SkinEditorState { search: " lane ".into(), ..Default::default() };
        assert!(editor.matches("LANE Cover"));
        assert!(!editor.matches("Background"));
        editor.search = "判定".into();
        assert!(editor.matches("判定文字"));
        assert!(!editor.matches("背景"));
    }

    #[test]
    fn skin_editor_displays_options_files_and_offsets_together() {
        let ctx = egui::Context::default();
        SettingsNavigation::select(&ctx, SettingsPage::Skin);
        let paths =
            AppPaths::from_dirs("resources".into(), "data".into(), "cache".into(), "logs".into());
        let defs = SceneSkinDefs {
            property: vec![SkinPropertyDef {
                category: String::new(),
                name: "Lane".into(),
                def: "On".into(),
                item: vec![bmz_render::skin::SkinPropertyItemDef { name: "On".into(), op: 1 }],
            }],
            filepath: vec![SkinFilepathDef {
                category: String::new(),
                name: "Notes".into(),
                path: "notes/*.png".into(),
                def: String::new(),
            }],
            offset: vec![SkinOffsetDef {
                category: String::new(),
                name: "Judge".into(),
                id: 32,
                x: true,
                y: false,
                w: false,
                h: false,
                r: false,
                a: false,
            }],
        };
        let meta = SkinConfigMeta { select: defs, ..Default::default() };
        let text = Localizer::new(AppLocale::En);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| {
                build_skin_panel(
                    ui,
                    &mut SkinConfig::default(),
                    &meta,
                    &SkinCatalog::default(),
                    &paths,
                    &mut SkinUiPathCache::default(),
                    text,
                );
            },
        );
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        for key in ["skin-options", "skin-file-selection", "skin-section-offsets"] {
            assert!(labels.contains(&text.text(key).as_str()), "missing skin section: {key}");
        }
    }

    #[test]
    fn compact_skin_editor_keeps_global_save_visible() {
        let ctx = egui::Context::default();
        SettingsNavigation::select(&ctx, SettingsPage::Skin);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 360.0));
        let paths =
            AppPaths::from_dirs("resources".into(), "data".into(), "cache".into(), "logs".into());
        let mut skin = SkinConfig::default();
        let mut saw_save = false;
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput { screen_rect: Some(viewport), ..Default::default() },
                |ui| {
                    build_settings_window(
                        ui.ctx(),
                        &mut true,
                        "Default",
                        Localizer::new(AppLocale::En),
                        |ui| {
                            build_skin_panel(
                                ui,
                                &mut skin,
                                &SkinConfigMeta::default(),
                                &SkinCatalog::default(),
                                &paths,
                                &mut SkinUiPathCache::default(),
                                Localizer::new(AppLocale::En),
                            );
                        },
                    );
                },
            );
            for shape in output.shapes {
                if let egui::Shape::Text(text) = shape.shape
                    && text.galley.job.text == "Changes are saved automatically."
                {
                    saw_save = true;
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(viewport.contains_rect(rect), "save outside viewport: {rect:?}");
                    assert!(shape.clip_rect.contains_rect(rect), "save clipped: {rect:?}");
                }
            }
        }
        assert!(saw_save);
    }
}
