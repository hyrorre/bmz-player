use std::path::Path;

use bmz_core::course::{CourseConstraints, CourseDefinition, CourseEntry, CourseKind};

use super::{
    CourseEditorAction, CourseEditorData, CourseEditorUiState, FluentArgs, Localizer,
    course_form::{build_course_constraints_editor, build_course_trophy_editor},
};

const LOCAL_COURSE_SOURCE: &str = "bmz:local";

pub(super) fn build_course_editor_page(
    ui: &mut egui::Ui,
    state: &mut CourseEditorUiState,
    data: &CourseEditorData,
    profile_root: &Path,
    editable: bool,
    text: Localizer,
) -> Option<CourseEditorAction> {
    if !editable {
        ui.colored_label(egui::Color32::LIGHT_YELLOW, text.text("course-editor-select-only"));
        return None;
    }

    build_course_picker(ui, state, data, text);
    let draft = state.draft.as_mut()?;
    let mut action = None;
    let mut reset_draft = false;

    egui::ScrollArea::vertical().show(ui, |ui| {
        build_identity_editor(ui, draft, text);
        ui.separator();
        build_course_constraints_editor(ui, draft, text, "course_editor");
        ui.separator();
        build_entry_editor(ui, draft, &mut state.search_query, data, text);
        ui.separator();
        build_course_trophy_editor(ui, draft, text, "course_editor");
        ui.separator();

        ui.horizontal_wrapped(|ui| {
            if ui.button(text.text("course-editor-save")).clicked() {
                action = Some(CourseEditorAction::Save(normalize_definition(
                    draft.clone(),
                    text.text("select-new-course"),
                )));
            }
            let local_selected = state.selected_course_id.is_some_and(|id| {
                data.courses
                    .iter()
                    .any(|course| course.id == id && course.source == LOCAL_COURSE_SOURCE)
            });
            if ui
                .add_enabled(local_selected, egui::Button::new(text.text("course-editor-delete")))
                .clicked()
                && let Some(id) = state.selected_course_id
            {
                action = Some(CourseEditorAction::Delete(id));
                reset_draft = true;
            }
        });

        ui.horizontal_wrapped(|ui| {
            if ui.button(text.text("course-editor-export")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("beatoraja course JSON", &["json"])
                    .set_directory(profile_root)
                    .set_file_name("courses.json")
                    .save_file()
            {
                action = Some(CourseEditorAction::Export {
                    path,
                    definition: normalize_definition(draft.clone(), text.text("select-new-course")),
                });
            }
            if ui.button(text.text("course-editor-import")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("beatoraja course JSON", &["json"])
                    .set_directory(profile_root)
                    .pick_file()
            {
                action = Some(CourseEditorAction::Import { path });
            }
        });
        if !state.status.is_empty() {
            let color =
                if state.error { egui::Color32::LIGHT_RED } else { egui::Color32::LIGHT_GREEN };
            ui.colored_label(color, &state.status);
        }
    });
    if reset_draft {
        state.selected_course_id = None;
        state.draft = None;
    }
    action
}

fn build_course_picker(
    ui: &mut egui::Ui,
    state: &mut CourseEditorUiState,
    data: &CourseEditorData,
    text: Localizer,
) {
    let selected_title = state
        .selected_course_id
        .and_then(|id| data.courses.iter().find(|course| course.id == id))
        .map(|course| course.definition.title.clone())
        .unwrap_or_else(|| text.text("course-editor-new"));
    let mut selected = None;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("course_editor_course")
            .selected_text(&selected_title)
            .width(360.0)
            .show_ui(ui, |ui| {
                for course in local_first_courses(&data.courses) {
                    let source =
                        if course.source == LOCAL_COURSE_SOURCE { "LOCAL" } else { "IMPORT" };
                    if ui
                        .selectable_label(
                            state.selected_course_id == Some(course.id),
                            format!("[{}] {}", source, course.definition.title),
                        )
                        .clicked()
                    {
                        selected = Some(course.id);
                    }
                }
            });
        if ui.button(text.text("course-editor-new")).clicked() {
            state.selected_course_id = None;
            state.draft = Some(new_definition(&data.courses, text.text("select-new-course")));
            state.status.clear();
        }
    });
    if let Some(id) = selected
        && let Some(course) = data.courses.iter().find(|course| course.id == id)
    {
        state.selected_course_id = Some(id);
        state.draft = Some(course.definition.clone());
        state.status.clear();
    }
    if state.draft.is_none() {
        state.draft = Some(new_definition(&data.courses, text.text("select-new-course")));
    }
}

fn local_first_courses(
    courses: &[crate::storage::library_db::StoredCourse],
) -> impl Iterator<Item = &crate::storage::library_db::StoredCourse> {
    courses
        .iter()
        .filter(|course| course.source == LOCAL_COURSE_SOURCE)
        .chain(courses.iter().filter(|course| course.source != LOCAL_COURSE_SOURCE))
}

fn build_identity_editor(ui: &mut egui::Ui, draft: &mut CourseDefinition, text: Localizer) {
    egui::Grid::new("course_editor_identity").num_columns(2).show(ui, |ui| {
        ui.label(text.text("course-editor-name"));
        ui.text_edit_singleline(&mut draft.title);
        ui.end_row();
        ui.label(text.text("course-editor-ir-submit"));
        ui.checkbox(&mut draft.release, "");
        ui.end_row();
    });
}

fn build_entry_editor(
    ui: &mut egui::Ui,
    draft: &mut CourseDefinition,
    search_query: &mut String,
    data: &CourseEditorData,
    text: Localizer,
) {
    let mut args = FluentArgs::new();
    args.set("count", draft.entries.len() as i64);
    args.set("max", crate::course::LOCAL_COURSE_MAX_ENTRIES as i64);
    ui.heading(text.format("course-editor-entries", &args));
    let mut move_entry = None;
    let mut remove_entry = None;
    for (index, entry) in draft.entries.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}.", index + 1));
            ui.label(&entry.title_hint);
            if ui.small_button("↑").clicked() && index > 0 {
                move_entry = Some((index, index - 1));
            }
            if ui.small_button("↓").clicked() && index + 1 < draft.entries.len() {
                move_entry = Some((index, index + 1));
            }
            if ui.small_button("×").clicked() {
                remove_entry = Some(index);
            }
        });
    }
    if let Some((from, to)) = move_entry {
        draft.entries.swap(from, to);
    }
    if let Some(index) = remove_entry {
        draft.entries.remove(index);
    }

    ui.horizontal(|ui| {
        ui.label(text.text("course-editor-chart-search"));
        ui.text_edit_singleline(search_query);
    });
    let row_height = ui.spacing().interact_size.y;
    egui::ScrollArea::vertical().max_height(180.0).show_rows(
        ui,
        row_height,
        data.charts.len(),
        |ui, visible_rows| {
            for index in visible_rows {
                let chart = &data.charts[index];
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            draft.entries.len() < crate::course::LOCAL_COURSE_MAX_ENTRIES,
                            egui::Button::new("+"),
                        )
                        .clicked()
                    {
                        draft.entries.push(CourseEntry {
                            title_hint: chart.title.clone(),
                            md5: Some(chart.md5.clone()),
                            sha256: Some(chart.sha256.clone()),
                            chart_id: Some(chart.chart_id),
                        });
                    }
                    ui.label(format!(
                        "{} / {}  ☆{}  [{}]",
                        chart.title, chart.artist, chart.play_level, chart.mode
                    ));
                });
            }
        },
    );
}

fn new_definition(
    courses: &[crate::storage::library_db::StoredCourse],
    default_title: String,
) -> CourseDefinition {
    let key = crate::course::next_local_course_key(
        courses.iter().map(|course| course.definition.key.as_str()),
    );
    CourseDefinition {
        key,
        title: default_title,
        kind: CourseKind::Course,
        entries: Vec::new(),
        constraints: CourseConstraints::default(),
        trophies: Vec::new(),
        release: false,
    }
}

fn normalize_definition(
    mut definition: CourseDefinition,
    default_title: String,
) -> CourseDefinition {
    definition.title = definition.title.trim().to_string();
    if definition.title.is_empty() {
        definition.title = default_title;
    }
    crate::course::normalize_course_definition(&mut definition);
    definition
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_course(
        id: i64,
        source: &str,
        title: &str,
    ) -> crate::storage::library_db::StoredCourse {
        crate::storage::library_db::StoredCourse {
            id,
            source: source.to_string(),
            definition: CourseDefinition {
                key: format!("course-{id}"),
                title: title.to_string(),
                kind: CourseKind::Course,
                entries: Vec::new(),
                constraints: CourseConstraints::default(),
                trophies: Vec::new(),
                release: false,
            },
        }
    }

    #[test]
    fn new_course_uses_localized_title_and_automatic_key() {
        let definition = new_definition(&[], "新規コース".to_string());

        assert_eq!(definition.key, "local-course-1");
        assert_eq!(definition.title, "新規コース");
        assert!(definition.trophies.is_empty());
        assert!(!definition.release);
    }

    #[test]
    fn empty_course_title_uses_localized_default_without_changing_key() {
        let definition = normalize_definition(
            CourseDefinition {
                key: "local-course-7".to_string(),
                title: "  ".to_string(),
                kind: CourseKind::Course,
                entries: Vec::new(),
                constraints: CourseConstraints::default(),
                trophies: Vec::new(),
                release: false,
            },
            "新規コース".to_string(),
        );

        assert_eq!(definition.key, "local-course-7");
        assert_eq!(definition.title, "新規コース");
    }

    #[test]
    fn course_picker_orders_local_courses_before_imports_stably() {
        let courses = vec![
            stored_course(1, "table:a", "Alpha"),
            stored_course(2, LOCAL_COURSE_SOURCE, "Beta"),
            stored_course(3, "table:b", "Charlie"),
            stored_course(4, LOCAL_COURSE_SOURCE, "Delta"),
        ];

        let ids = local_first_courses(&courses).map(|course| course.id).collect::<Vec<_>>();

        assert_eq!(ids, [2, 4, 1, 3]);
    }
}
