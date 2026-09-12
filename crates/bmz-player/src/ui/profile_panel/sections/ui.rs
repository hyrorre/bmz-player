use super::*;

pub(in crate::ui::profile_panel) fn build_profile_ui_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let show_fps = &mut *section.show_fps;
    let mut text = section.text;
    SettingsSection::new(SettingsPage::General, tr!(text, "profile-ui-title"))
        .scope(tr!(text, "settings-scope-profile"))
        .id_salt("profile_ui")
        .show(ui, |ui| {
            let current_locale = profile.ui.locale();
            let mut selected_locale = current_locale;
            let label = tr!(text, "profile-ui-language");
            let label =
                if text.locale() == AppLocale::En { label } else { format!("{label} (Language)") };
            egui::ComboBox::new("profile_ui_language", label)
                .selected_text(selected_locale.native_name())
                .show_ui(ui, |ui| {
                    for locale in AppLocale::SUPPORTED {
                        ui.selectable_value(&mut selected_locale, locale, locale.native_name());
                    }
                });
            if selected_locale != current_locale {
                profile.ui.set_locale(selected_locale);
                text = Localizer::new(selected_locale);
                section.save_clicked = true;
            }
            if ui.checkbox(show_fps, tr!(text, "settings-show-fps")).changed() {
                profile.ui.show_fps = *show_fps;
            }
        });
    section.text = text;
}
