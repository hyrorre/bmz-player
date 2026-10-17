pub(in crate::ui) fn skin_path_combo(
    ui: &mut egui::Ui,
    skin: &mut SkinConfig,
    slot: SkinSlot,
    label: &str,
    candidates: &[SkinCandidate],
    show_bundled_origin: bool,
    text: Localizer,
) -> bool {
    ui.label(label);
    let current = skin_slot_path(skin, slot).to_string();
    let mut selected = current.clone();
    let selected_text = skin_candidate_label(candidates, &current, show_bundled_origin, text);
    egui::ComboBox::from_id_salt(("skin_path_combo", slot.path_combo_id()))
        .selected_text(selected_text)
        .width(ui.available_width().clamp(120.0, 520.0))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, String::new(), tr!(text, "skin-default"));
            for candidate in candidates {
                let response = ui.selectable_value(
                    &mut selected,
                    candidate.path.clone(),
                    skin_candidate_display(candidate, show_bundled_origin, text),
                );
                if show_bundled_origin && candidate.origin == SkinCandidateOrigin::Bundled {
                    response.on_hover_text(tr!(text, "skin-origin-bundled-help"));
                }
            }
        });
    let combo_changed = selected != current;
    if combo_changed {
        save_skin_slot_history(skin, slot);
        *skin_slot_path_mut(skin, slot) = selected;
        restore_skin_slot_history(skin, slot);
    }
    let mut edited_path = skin_slot_path(skin, slot).to_string();
    let text_changed =
        ui.add(egui::TextEdit::singleline(&mut edited_path).desired_width(f32::INFINITY)).changed();
    if text_changed {
        save_skin_slot_history(skin, slot);
        *skin_slot_path_mut(skin, slot) = edited_path;
        restore_skin_slot_history(skin, slot);
    }
    combo_changed || text_changed
}

pub(in crate::ui) fn skin_candidate_label(
    candidates: &[SkinCandidate],
    current: &str,
    show_bundled_origin: bool,
    text: Localizer,
) -> String {
    if current.is_empty() {
        return tr!(text, "skin-default");
    }
    candidates
        .iter()
        .find(|candidate| candidate.path == current)
        .map(|candidate| skin_candidate_display(candidate, show_bundled_origin, text))
        .unwrap_or_else(|| current.to_string())
}

pub(in crate::ui) fn skin_candidate_display(
    candidate: &SkinCandidate,
    show_bundled_origin: bool,
    text: Localizer,
) -> String {
    let label = skin_candidate_origin_label(candidate.origin, show_bundled_origin, text);
    let text = if candidate.name.is_empty() {
        candidate.path.clone()
    } else {
        format!("{} ({})", candidate.name, candidate.path)
    };
    if let Some(label) = label { format!("{label} {text}") } else { text }
}

pub(in crate::ui) fn skin_candidate_origin_label(
    origin: SkinCandidateOrigin,
    show_bundled_origin: bool,
    text: Localizer,
) -> Option<String> {
    match origin {
        SkinCandidateOrigin::Bundled if show_bundled_origin => {
            Some(tr!(text, "skin-origin-bundled"))
        }
        SkinCandidateOrigin::Bundled => None,
        SkinCandidateOrigin::User => Some(tr!(text, "skin-origin-user")),
        SkinCandidateOrigin::External => Some(tr!(text, "skin-origin-external")),
    }
}

pub(in crate::ui) fn show_bundled_skin_origin(
    app_paths: &AppPaths,
    skin_catalog: &SkinCatalog,
) -> bool {
    !app_paths.hides_bundled_skin_label() && skin_catalog_has_non_bundled_candidate(skin_catalog)
}

pub(in crate::ui) fn skin_catalog_has_non_bundled_candidate(skin_catalog: &SkinCatalog) -> bool {
    let groups: [&[SkinCandidate]; 14] = [
        &skin_catalog.select,
        &skin_catalog.decide,
        &skin_catalog.play4,
        &skin_catalog.play5,
        &skin_catalog.play6,
        &skin_catalog.play7,
        &skin_catalog.play8,
        &skin_catalog.play9,
        &skin_catalog.play10,
        &skin_catalog.play14,
        &skin_catalog.battle5,
        &skin_catalog.battle7,
        &skin_catalog.result,
        &skin_catalog.course_result,
    ];
    groups.iter().any(|candidates| {
        candidates.iter().any(|candidate| candidate.origin != SkinCandidateOrigin::Bundled)
    })
}

pub(in crate::ui) fn skin_slot_path(skin: &SkinConfig, slot: SkinSlot) -> &str {
    match slot {
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
    }
}

pub(in crate::ui) fn skin_slot_path_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut String {
    match slot {
        SkinSlot::Select => &mut skin.select,
        SkinSlot::Decide => &mut skin.decide,
        SkinSlot::Play4 => &mut skin.play4,
        SkinSlot::Play5 => &mut skin.play5,
        SkinSlot::Play6 => &mut skin.play6,
        SkinSlot::Play7 => &mut skin.play7,
        SkinSlot::Play8 => &mut skin.play8,
        SkinSlot::Play9 => &mut skin.play9,
        SkinSlot::Play10 => &mut skin.play10,
        SkinSlot::Play14 => &mut skin.play14,
        SkinSlot::Battle5 => &mut skin.battle5,
        SkinSlot::Battle7 => &mut skin.battle7,
        SkinSlot::Result => &mut skin.result,
        SkinSlot::CourseResult => &mut skin.course_result,
    }
}

pub(in crate::ui) fn skin_slot_options_mut(
    skin: &mut SkinConfig,
    slot: SkinSlot,
) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_options,
        SkinSlot::Decide => &mut skin.decide_options,
        SkinSlot::Play4 => &mut skin.play4_options,
        SkinSlot::Play5 => &mut skin.play5_options,
        SkinSlot::Play6 => &mut skin.play6_options,
        SkinSlot::Play7 => &mut skin.play7_options,
        SkinSlot::Play8 => &mut skin.play8_options,
        SkinSlot::Play9 => &mut skin.play9_options,
        SkinSlot::Play10 => &mut skin.play10_options,
        SkinSlot::Play14 => &mut skin.play14_options,
        SkinSlot::Battle5 => &mut skin.battle5_options,
        SkinSlot::Battle7 => &mut skin.battle7_options,
        SkinSlot::Result => &mut skin.result_options,
        SkinSlot::CourseResult => &mut skin.course_result_options,
    }
}

pub(in crate::ui) fn skin_slot_files_mut(
    skin: &mut SkinConfig,
    slot: SkinSlot,
) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_files,
        SkinSlot::Decide => &mut skin.decide_files,
        SkinSlot::Play4 => &mut skin.play4_files,
        SkinSlot::Play5 => &mut skin.play5_files,
        SkinSlot::Play6 => &mut skin.play6_files,
        SkinSlot::Play7 => &mut skin.play7_files,
        SkinSlot::Play8 => &mut skin.play8_files,
        SkinSlot::Play9 => &mut skin.play9_files,
        SkinSlot::Play10 => &mut skin.play10_files,
        SkinSlot::Play14 => &mut skin.play14_files,
        SkinSlot::Battle5 => &mut skin.battle5_files,
        SkinSlot::Battle7 => &mut skin.battle7_files,
        SkinSlot::Result => &mut skin.result_files,
        SkinSlot::CourseResult => &mut skin.course_result_files,
    }
}

pub(in crate::ui) fn skin_slot_offsets_mut(
    skin: &mut SkinConfig,
    slot: SkinSlot,
) -> &mut Vec<SkinOffsetConfig> {
    match slot {
        SkinSlot::Select => &mut skin.select_offsets,
        SkinSlot::Decide => &mut skin.decide_offsets,
        SkinSlot::Play4 => &mut skin.play4_offsets,
        SkinSlot::Play5 => &mut skin.play5_offsets,
        SkinSlot::Play6 => &mut skin.play6_offsets,
        SkinSlot::Play7 => &mut skin.play7_offsets,
        SkinSlot::Play8 => &mut skin.play8_offsets,
        SkinSlot::Play9 => &mut skin.play9_offsets,
        SkinSlot::Play10 => &mut skin.play10_offsets,
        SkinSlot::Play14 => &mut skin.play14_offsets,
        SkinSlot::Battle5 => &mut skin.battle5_offsets,
        SkinSlot::Battle7 => &mut skin.battle7_offsets,
        SkinSlot::Result => &mut skin.result_offsets,
        SkinSlot::CourseResult => &mut skin.course_result_offsets,
    }
}

pub(in crate::ui) fn skin_slot_history_key(slot: SkinSlot, path: &str) -> String {
    let slot_name = match slot {
        SkinSlot::Select => "select",
        SkinSlot::Decide => "decide",
        SkinSlot::Play4 => "play4",
        SkinSlot::Play5 => "play5",
        SkinSlot::Play6 => "play6",
        SkinSlot::Play7 => "play7",
        SkinSlot::Play8 => "play8",
        SkinSlot::Play9 => "play9",
        SkinSlot::Play10 => "play10",
        SkinSlot::Play14 => "play14",
        SkinSlot::Battle5 => "battle5",
        SkinSlot::Battle7 => "battle7",
        SkinSlot::Result => "result",
        SkinSlot::CourseResult => "course_result",
    };
    format!("{slot_name}::{path}")
}

pub(in crate::ui) fn save_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot) {
    let path = skin_slot_path(skin, slot).trim().to_string();
    if path.is_empty() {
        return;
    }
    let options = skin_slot_options_mut(skin, slot).clone();
    let files = skin_slot_files_mut(skin, slot).clone();
    let offsets = skin_slot_offsets_mut(skin, slot).clone();
    skin.history.insert(
        skin_slot_history_key(slot, &path),
        SkinHistoryEntryConfig { options, files, offsets },
    );
}

pub(in crate::ui) fn restore_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot) {
    let path = skin_slot_path(skin, slot).trim().to_string();
    let history_key = skin_slot_history_key(slot, &path);
    let Some(entry) =
        skin.history.get(&history_key).cloned().or_else(|| skin.history.get(&path).cloned())
    else {
        skin_slot_options_mut(skin, slot).clear();
        skin_slot_files_mut(skin, slot).clear();
        skin_slot_offsets_mut(skin, slot).clear();
        return;
    };
    skin.history.entry(history_key).or_insert_with(|| entry.clone());
    *skin_slot_options_mut(skin, slot) = entry.options;
    *skin_slot_files_mut(skin, slot) = entry.files;
    *skin_slot_offsets_mut(skin, slot) = entry.offsets;
}
use super::*;
