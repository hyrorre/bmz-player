/// 1 シーン分のスキン設定可能項目を表示・編集する。
///
/// - property: ComboBox で選択肢を選び `options` へ書き込む。
/// - filepath: `path` グロブにマッチするファイルを ComboBox で選び `files` へ書き込む。
/// - offset: 宣言された要素ごとに x/y/w/h/r/a を編集し `offsets` (名前単位) へ反映。
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::ui) struct SceneSkinEdit {
    pub(in crate::ui) changed: bool,
    pub(in crate::ui) offsets_changed: bool,
}

/// ComboBox の選択肢のうち最も長い文字列が収まる幅を計算する。
///
/// `ComboBox` は選択中の文字列に合わせて幅が変わるため、選択肢全体を計測して
/// ボタン幅を固定する。アイコンとボタンの余白も含め、通常の既定幅を下限にする。
fn combo_width_for_labels<'a>(ui: &egui::Ui, labels: impl IntoIterator<Item = &'a str>) -> f32 {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let max_text_width = labels
        .into_iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(label.to_owned(), font_id.clone(), ui.visuals().text_color())
                .size()
                .x
        })
        .fold(0.0, f32::max);
    let spacing = ui.spacing();
    (max_text_width + spacing.icon_spacing + spacing.icon_width + spacing.button_padding.x * 2.0)
        .max(spacing.combo_width)
}

pub(in crate::ui) fn build_scene_skin_defs(
    ui: &mut egui::Ui,
    slot: SkinSlot,
    defs: &SceneSkinDefs,
    skin_path: &str,
    access: SkinUiAccess<'_>,
    path_cache: &mut SkinUiPathCache,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
    text: Localizer,
) -> SceneSkinEdit {
    let mut changed = false;
    let mut offsets_changed = false;
    let editor = SkinEditorState::load(ui.ctx());
    ui.push_id(slot.defs_header_id(), |ui| {
        ui.strong(skin_scene_defs_label(slot, text));
        if defs.is_empty() {
            ui.label(tr!(text, "skin-no-settings"));
            return;
        }
        // 選択中のスロットだけ正規化・path 解決する。
        let synced = sync_skin_offsets_with_defs(&defs.offset, offsets);
        changed |= synced;
        offsets_changed |= synced;
        let path_context = if defs.filepath.is_empty() {
            None
        } else {
            path_cache.get_or_resolve(slot, access.paths, skin_path, access.grants)
        };
        changed |= fill_missing_skin_defaults_with_context(defs, path_context, options, files);
        let locale = text.locale();
        let property_matches = |prop: &SkinPropertyDef| {
            editor.matches(&prop.name) || editor.matches(defs.property_label(&prop.name, locale))
        };
        let any_match = defs.property.iter().any(property_matches)
            || defs.filepath.iter().any(|file| editor.matches(&file.name))
            || defs
                .offset
                .iter()
                .any(|offset| editor.matches(&format!("{} {}", offset.name, offset.category)));
        if !any_match {
            ui.label(tr!(text, "skin-no-matches"));
        }
        if !defs.property.is_empty() {
            ui.add_space(8.0);
            ui.strong(tr!(text, "skin-options"));
            ui.separator();
            // property / filepath は同名 (例: "シャッター") を持ちうるので、egui の
            // ComboBox ID 衝突を防ぐためにカテゴリで名前空間を切る。
            ui.push_id("property", |ui| {
                let row_height = ui.text_style_height(&egui::TextStyle::Body)
                    + ui.spacing().item_spacing.y
                    + ui.spacing().interact_size.y;
                for (index, prop) in defs.property.iter().enumerate() {
                    if !property_matches(prop) {
                        continue;
                    }
                    let item_labels: Vec<&str> = prop
                        .item
                        .iter()
                        .map(|item| defs.item_label(&prop.name, &item.name, locale))
                        .collect();
                    let combo_width = combo_width_for_labels(ui, item_labels.iter().copied());
                    show_culled_skin_row(ui, (index, prop.name.as_str()), row_height, |ui| {
                        let mut selected = options
                            .get(&prop.name)
                            .cloned()
                            .unwrap_or_else(|| property_default(prop));
                        let before = selected.clone();
                        let label = defs.property_label(&prop.name, locale);
                        ui.add(egui::Label::new(label).truncate()).on_hover_text(label);
                        ui.horizontal(|ui| {
                            let previous = previous_property_selection(prop, &selected);
                            if ui
                                .add_enabled(previous.is_some(), egui::Button::new("◀"))
                                .on_hover_text(tr!(text, "skin-option-previous"))
                                .clicked()
                                && let Some(previous) = previous
                            {
                                selected = previous.to_string();
                            }
                            let selected_label =
                                defs.item_label(&prop.name, &selected, locale).to_string();
                            egui::ComboBox::from_id_salt("selection")
                                .selected_text(selected_label)
                                .width(combo_width)
                                .truncate()
                                .show_ui(ui, |ui| {
                                    for (item, label) in prop.item.iter().zip(&item_labels) {
                                        ui.selectable_value(
                                            &mut selected,
                                            item.name.clone(),
                                            *label,
                                        );
                                    }
                                });
                            let next = next_property_selection(prop, &selected);
                            if ui
                                .add_enabled(next.is_some(), egui::Button::new("▶"))
                                .on_hover_text(tr!(text, "skin-option-next"))
                                .clicked()
                                && let Some(next) = next
                            {
                                selected = next.to_string();
                            }
                        });
                        if selected != before {
                            options.insert(prop.name.clone(), selected);
                            changed = true;
                        }
                    });
                }
            });
        }
        if defs.qwilight {
            ui.add_space(4.0);
            changed |= build_qwilight_lane_cover_tools(ui, slot, access.paths, options, text);
        }
        if !defs.filepath.is_empty() {
            ui.add_space(8.0);
            ui.strong(tr!(text, "skin-file-selection"));
            ui.separator();
            ui.push_id("filepath", |ui| {
                let row_height = ui.text_style_height(&egui::TextStyle::Body)
                    + ui.spacing().item_spacing.y
                    + ui.spacing().interact_size.y;
                for (index, filepath) in defs.filepath.iter().enumerate() {
                    if !editor.matches(&filepath.name) {
                        continue;
                    }
                    show_culled_skin_row(ui, (index, filepath.name.as_str()), row_height, |ui| {
                        let candidates = path_context
                            .map(|context| glob_candidates_for_skin(context, &filepath.path))
                            .unwrap_or_default();
                        let mut selected = files.get(&filepath.name).cloned().unwrap_or_default();
                        let before = selected.clone();
                        ui.add(egui::Label::new(&filepath.name).truncate())
                            .on_hover_text(&filepath.name);
                        ui.horizontal(|ui| {
                            let can_step = path_context.is_some() && !selected.is_empty();
                            if ui
                                .add_enabled(
                                    can_step && selected != RANDOM_FILE_SELECTION,
                                    egui::Button::new("◀"),
                                )
                                .on_hover_text(tr!(text, "skin-option-previous"))
                                .clicked()
                                && let Some(context) = path_context
                            {
                                let candidates = glob_candidates_for_skin(context, &filepath.path);
                                if let Some(previous) =
                                    previous_filepath_selection(&selected, &candidates)
                                {
                                    selected = previous;
                                }
                            }
                            let display = if selected.is_empty() {
                                tr!(text, "skin-file-none")
                            } else if selected == RANDOM_FILE_SELECTION {
                                tr!(text, "skin-file-random")
                            } else {
                                filepath_selection_label(&selected).to_string()
                            };
                            let mut combo_labels = vec![display.clone()];
                            combo_labels.push(tr!(text, "skin-file-random"));
                            combo_labels.extend(
                                candidates.iter().map(|candidate| {
                                    filepath_selection_label(candidate).to_string()
                                }),
                            );
                            let combo_width =
                                combo_width_for_labels(ui, combo_labels.iter().map(String::as_str));
                            egui::ComboBox::from_id_salt("selection")
                                .selected_text(display)
                                .width(combo_width)
                                .truncate()
                                .show_ui(ui, |ui| {
                                    // beatoraja 同様、具体ファイルに加えて「ランダム」を選べる。
                                    // ランダム選択時は毎ロードで候補からランダムに解決する。
                                    ui.selectable_value(
                                        &mut selected,
                                        RANDOM_FILE_SELECTION.to_string(),
                                        tr!(text, "skin-file-random"),
                                    );
                                    if let Some(normalized) =
                                        normalize_filepath_selection(&selected, &candidates)
                                    {
                                        selected = normalized;
                                    }
                                    if candidates.is_empty() {
                                        ui.label(tr!(text, "skin-file-no-candidates"));
                                    }
                                    for candidate in candidates {
                                        let label = filepath_selection_label(&candidate);
                                        ui.selectable_value(
                                            &mut selected,
                                            candidate.clone(),
                                            label,
                                        );
                                    }
                                });
                            if ui
                                .add_enabled(can_step, egui::Button::new("▶"))
                                .on_hover_text(tr!(text, "skin-option-next"))
                                .clicked()
                                && let Some(context) = path_context
                            {
                                let candidates = glob_candidates_for_skin(context, &filepath.path);
                                if let Some(next) = next_filepath_selection(&selected, &candidates)
                                {
                                    selected = next;
                                }
                            }
                        });
                        if selected != before {
                            files.insert(filepath.name.clone(), selected);
                            changed = true;
                        }
                    });
                }
            });
        }
        if !defs.offset.is_empty() {
            ui.add_space(8.0);
            ui.strong(tr!(text, "skin-section-offsets"));
            ui.separator();
            let row_height = ui.text_style_height(&egui::TextStyle::Body)
                + ui.spacing().item_spacing.y
                + ui.spacing().interact_size.y;
            for (offset_index, offset_def) in defs.offset.iter().enumerate() {
                if !editor.matches(&format!("{} {}", offset_def.name, offset_def.category)) {
                    continue;
                }
                show_culled_skin_row(
                    ui,
                    (offset_index, offset_def.id, offset_def.name.as_str()),
                    row_height,
                    |ui| {
                        ui.add(
                            egui::Label::new(&offset_def.name)
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        let existing = offsets
                            .iter()
                            .find(|offset| {
                                offset.name.as_deref() == Some(offset_def.name.as_str())
                                    && offset.id == offset_def.id
                            })
                            .or_else(|| {
                                offsets.iter().find(|offset| {
                                    offset.name.as_deref() == Some(offset_def.name.as_str())
                                })
                            })
                            .or_else(|| {
                                offsets.iter().find(|offset| {
                                    offset.name.is_none() && offset.id == offset_def.id
                                })
                            })
                            .cloned();
                        let mut value = existing.unwrap_or(SkinOffsetConfig {
                            name: Some(offset_def.name.clone()),
                            id: offset_def.id,
                            ..Default::default()
                        });
                        value.name = Some(offset_def.name.clone());
                        value.id = offset_def.id;
                        let before = value.clone();
                        ui.horizontal(|ui| {
                            let _ = add_offset_drag_values(ui, offset_def, &mut value, text);
                        });
                        if value != before {
                            changed |= update_skin_offset_value(offsets, offset_def, value);
                            offsets_changed = true;
                        }
                    },
                );
            }
        }
        if !defs.is_empty() && ui.button(tr!(text, "skin-reset-defaults")).clicked() {
            let previous_offsets = offsets.clone();
            changed |= reset_scene_skin_to_defaults_with_context(
                defs,
                path_context,
                options,
                files,
                offsets,
            );
            offsets_changed |= *offsets != previous_offsets;
        }
    });
    SceneSkinEdit { changed, offsets_changed }
}

/// 外側の ScrollArea の clip rect に入る行だけ widget tree を構築する。
///
/// `allocate_space` で全体のスクロール量は維持しつつ、Rmz-skin のような数百行の
/// ComboBox / label / DragValue を画面外まで毎フレーム生成しない。
pub(in crate::ui) fn show_culled_skin_row(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    row_height: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let (_, rect) = ui.allocate_space(egui::vec2(ui.available_width(), row_height));
    if !ui.is_rect_visible(rect) {
        return;
    }
    let mut row_ui = ui.new_child(egui::UiBuilder::new().id_salt(id_salt).max_rect(rect));
    add_contents(&mut row_ui);
}

/// 現在のスキン定義に合わせ、offset 設定を名前優先で正規化する。
///
/// 旧設定は名前を持たないため ID で移行する。同じ旧 ID を複数の異なる名前が
/// 使用する場合は値をそれぞれへ複製し、以後は独立して編集できるようにする。
/// 同名定義が複数 ID にある場合は、beatoraja と同様に最初の同名設定を共有する。
pub(in crate::ui) fn sync_skin_offsets_with_defs(
    defs: &[SkinOffsetDef],
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    if defs.is_empty() || offsets.is_empty() {
        return false;
    }

    let previous = offsets.clone();
    let mut named_sources = std::collections::HashMap::with_capacity(previous.len());
    let mut legacy_sources = std::collections::HashMap::with_capacity(previous.len());
    for offset in &previous {
        if let Some(name) = offset.name.as_deref() {
            named_sources.entry(name).or_insert(offset);
        } else {
            legacy_sources.entry(offset.id).or_insert(offset);
        }
    }
    let mut synced = Vec::with_capacity(previous.len().max(defs.len()));
    for offset_def in defs {
        let source = named_sources
            .get(offset_def.name.as_str())
            .copied()
            .or_else(|| legacy_sources.get(&offset_def.id).copied());
        if let Some(source) = source {
            let mut value = source.clone();
            value.name = Some(offset_def.name.clone());
            value.id = offset_def.id;
            synced.push(value);
        }
    }

    let declared_names: std::collections::HashSet<&str> =
        defs.iter().map(|offset| offset.name.as_str()).collect();
    let declared_ids: std::collections::HashSet<i32> =
        defs.iter().map(|offset| offset.id).collect();
    synced.extend(
        previous
            .iter()
            .filter(|offset| match offset.name.as_deref() {
                Some(name) => !declared_names.contains(name),
                None => !declared_ids.contains(&offset.id),
            })
            .cloned(),
    );

    if synced == previous {
        false
    } else {
        *offsets = synced;
        true
    }
}

/// 同名 offset の値を全定義へ反映する。ID は各定義のものを維持する。
pub(in crate::ui) fn update_skin_offset_value(
    offsets: &mut Vec<SkinOffsetConfig>,
    offset_def: &SkinOffsetDef,
    value: SkinOffsetConfig,
) -> bool {
    let mut found_named = false;
    for entry in
        offsets.iter_mut().filter(|offset| offset.name.as_deref() == Some(offset_def.name.as_str()))
    {
        let id = entry.id;
        *entry = value.clone();
        entry.name = Some(offset_def.name.clone());
        entry.id = id;
        found_named = true;
    }
    if found_named {
        return true;
    }

    if let Some(entry) =
        offsets.iter_mut().find(|offset| offset.name.is_none() && offset.id == offset_def.id)
    {
        *entry = value;
        entry.name = Some(offset_def.name.clone());
        entry.id = offset_def.id;
    } else {
        offsets.push(value);
    }
    true
}

/// 1 シーン分の options / files / 当該 offset 名をスキン定義の factory default へ戻す。
#[cfg(test)]
pub(in crate::ui) fn reset_scene_skin_to_defaults(
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    let path_context = skin_root.map(SkinUiPathContext::legacy);
    reset_scene_skin_to_defaults_with_context(defs, path_context.as_ref(), options, files, offsets)
}

pub(in crate::ui) fn reset_scene_skin_to_defaults_with_context(
    defs: &SceneSkinDefs,
    path_context: Option<&SkinUiPathContext>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    if defs.is_empty() {
        return false;
    }
    let previous_options = options.clone();
    let previous_files = files.clone();
    let previous_offsets = offsets.clone();
    options.clear();
    files.clear();
    let scene_offset_names: std::collections::HashSet<&str> =
        defs.offset.iter().map(|offset| offset.name.as_str()).collect();
    let scene_offset_ids: std::collections::HashSet<i32> =
        defs.offset.iter().map(|offset| offset.id).collect();
    offsets.retain(|offset| match offset.name.as_deref() {
        Some(name) => !scene_offset_names.contains(name),
        None => !scene_offset_ids.contains(&offset.id),
    });
    let _ = fill_missing_skin_defaults_with_context(defs, path_context, options, files);
    *options != previous_options || *files != previous_files || *offsets != previous_offsets
}

#[cfg(test)]
pub(in crate::ui) fn fill_missing_skin_defaults(
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
) -> bool {
    let path_context = skin_root.map(SkinUiPathContext::legacy);
    fill_missing_skin_defaults_with_context(defs, path_context.as_ref(), options, files)
}

pub(in crate::ui) fn fill_missing_skin_defaults_with_context(
    defs: &SceneSkinDefs,
    path_context: Option<&SkinUiPathContext>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
) -> bool {
    let mut changed = false;
    for prop in &defs.property {
        let current = options.get(&prop.name).map(String::as_str);
        if current.is_none() || !property_selection_is_valid(prop, current.unwrap_or_default()) {
            let default = property_default(prop);
            if current != Some(default.as_str()) {
                options.insert(prop.name.clone(), default);
                changed = true;
            }
        }
    }
    let Some(path_context) = path_context else {
        return changed;
    };
    for filepath in &defs.filepath {
        // beatoraja は保存済み filepath を候補内に存在するか検証せず尊重する。
        // BMZ 旧版の相対パス保存も含め、空でなければここでは置き換えない。
        let current = files.get(&filepath.name);
        if current.is_some_and(|selected| !selected.is_empty()) {
            continue;
        }
        let current = current.map(|value| value.replace('\\', "/"));
        let candidates = glob_candidates_for_skin(path_context, &filepath.path);
        if let Some(default) = filepath_default(filepath, &candidates) {
            if current.as_deref() != Some(default.as_str()) {
                files.insert(filepath.name.clone(), default);
                changed = true;
            }
        } else if current.as_deref() != Some("") {
            files.insert(filepath.name.clone(), String::new());
            changed = true;
        }
    }
    changed
}

pub(in crate::ui) fn add_offset_drag_values(
    ui: &mut egui::Ui,
    def: &SkinOffsetDef,
    value: &mut SkinOffsetConfig,
    text: Localizer,
) -> bool {
    let mut changed = false;
    let mut any = false;
    if def.x {
        changed |= ui.add(egui::DragValue::new(&mut value.x).prefix("x:")).changed();
        any = true;
    }
    if def.y {
        changed |= ui.add(egui::DragValue::new(&mut value.y).prefix("y:")).changed();
        any = true;
    }
    if def.w {
        changed |= ui.add(egui::DragValue::new(&mut value.w).prefix("w:")).changed();
        any = true;
    }
    if def.h {
        changed |= ui.add(egui::DragValue::new(&mut value.h).prefix("h:")).changed();
        any = true;
    }
    if def.r {
        changed |= ui.add(egui::DragValue::new(&mut value.r).prefix("r:")).changed();
        any = true;
    }
    if def.a {
        changed |= ui.add(egui::DragValue::new(&mut value.a).prefix("a:")).changed();
        any = true;
    }
    if !any {
        ui.label(tr!(text, "skin-offset-no-adjustable-values"));
    }
    changed
}
use super::*;
