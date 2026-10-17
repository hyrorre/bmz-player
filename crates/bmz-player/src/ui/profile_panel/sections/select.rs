use super::*;

pub(in crate::ui::profile_panel) fn build_profile_select_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let unrestricted = section.unrestricted;
    let text = section.text;
    SettingsSection::new(SettingsPage::Select, tr!(text, "profile-select-title"))
        .scope(tr!(text, "settings-scope-profile"))
        .id_salt("profile_select")
        .show(ui, |ui| {
            if !unrestricted {
                ui.disable();
            }
            ui.group(|ui| {
                ui.checkbox(&mut profile.select.random_select, "RANDOM SELECT");
                ui.checkbox(&mut profile.select.random_select_no_play, "NO PLAY RANDOM SELECT");
                ui.checkbox(&mut profile.select.random_select_failed, "FAILED RANDOM SELECT");
                ui.checkbox(&mut profile.select.random_select_not_easy, "NOT EASY RANDOM SELECT");
                ui.checkbox(&mut profile.select.random_select_not_clear, "NOT CLEAR RANDOM SELECT");
                ui.checkbox(&mut profile.select.random_select_not_hard, "NOT HARD RANDOM SELECT");
                ui.checkbox(
                    &mut profile.select.random_select_not_ex_hard,
                    "NOT EX-HARD RANDOM SELECT",
                );
                ui.checkbox(
                    &mut profile.select.random_select_not_full_combo,
                    "NOT FULL COMBO RANDOM SELECT",
                );
            });
            ui.small(tr!(text, "settings-entry-description-random-select"));
            ui.add_space(8.0);
            egui::ComboBox::new(
                "profile_select_difficulty_table_level_display",
                tr!(text, "profile-select-difficulty-table-level-display"),
            )
            .selected_text(difficulty_table_level_display_label(
                text,
                profile.select.difficulty_table_level_display,
            ))
            .show_ui(ui, |ui| {
                for value in
                    [DifficultyTableLevelDisplay::Table, DifficultyTableLevelDisplay::Chart]
                {
                    ui.selectable_value(
                        &mut profile.select.difficulty_table_level_display,
                        value,
                        difficulty_table_level_display_label(text, value),
                    );
                }
            });
        });
}

pub(in crate::ui) fn difficulty_table_level_display_label(
    text: Localizer,
    value: DifficultyTableLevelDisplay,
) -> String {
    match value {
        DifficultyTableLevelDisplay::Table => text.text("profile-select-level-table"),
        DifficultyTableLevelDisplay::Chart => text.text("profile-select-level-chart"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_table_level_display_labels_are_localized() {
        assert_eq!(
            difficulty_table_level_display_label(
                Localizer::new(AppLocale::Ja),
                DifficultyTableLevelDisplay::Table,
            ),
            "難易度表レベル"
        );
        assert_eq!(
            difficulty_table_level_display_label(
                Localizer::new(AppLocale::En),
                DifficultyTableLevelDisplay::Chart,
            ),
            "Chart's original level"
        );
    }
}
