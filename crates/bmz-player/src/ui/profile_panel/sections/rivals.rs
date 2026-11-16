use super::*;
use crate::config::profile_config::{RivalEntry, RivalSourceConfig};

pub(in crate::ui::profile_panel) fn build_profile_rival_section(
    ui: &mut egui::Ui,
    paths: &AppPaths,
    section: &mut ProfileSectionContext<'_>,
) {
    let text = section.text;
    SettingsSection::new(SettingsPage::Select, tr!(text, "profile-rivals-title"))
        .scope(tr!(text, "settings-scope-profile"))
        .id_salt("profile_local_rivals")
        .show(ui, |ui| {
            if !section.unrestricted {
                ui.disable();
            }
            ui.small(tr!(text, "profile-rivals-help"));
            let profiles = match profile_cmd::profile_summaries(paths) {
                Ok(profiles) => profiles,
                Err(error) => {
                    ui.colored_label(egui::Color32::RED, format!("{error:#}"));
                    return;
                }
            };
            for summary in profiles.iter().filter(|summary| summary.id != section.profile.id) {
                let id = format!("local-profile-{}", summary.id);
                let mut registered =
                    section.profile.rival.entries.iter().any(|entry| entry.id == id);
                if ui
                    .checkbox(&mut registered, format!("{} ({})", summary.display_name, summary.id))
                    .changed()
                {
                    if registered {
                        section.profile.rival.entries.push(RivalEntry {
                            id,
                            display_name: summary.display_name.clone(),
                            source: RivalSourceConfig::LocalProfile,
                            profile_id: summary.id.clone(),
                            path: String::new(),
                            ir_service: String::new(),
                            ir_user_id: String::new(),
                        });
                    } else {
                        section.profile.rival.entries.retain(|entry| entry.id != id);
                        if section.profile.rival.active_rival == id {
                            section.profile.rival.active_rival.clear();
                        }
                    }
                    section.save_clicked = true;
                }
            }
            let ids = crate::screens::select_ir::selectable_rival_ids(section.profile);
            let mut selected = section.profile.rival.active_rival.clone();
            let label = section
                .profile
                .rival
                .entries
                .iter()
                .find(|entry| entry.id == selected)
                .map(|entry| entry.display_name.clone())
                .unwrap_or_else(|| tr!(text, "profile-rivals-none"));
            egui::ComboBox::new("active_local_or_ir_rival", tr!(text, "profile-rivals-active"))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut selected,
                        String::new(),
                        tr!(text, "profile-rivals-none"),
                    );
                    for entry in
                        section.profile.rival.entries.iter().filter(|entry| ids.contains(&entry.id))
                    {
                        ui.selectable_value(&mut selected, entry.id.clone(), &entry.display_name);
                    }
                });
            if selected != section.profile.rival.active_rival {
                section.profile.rival.active_rival = selected;
                section.save_clicked = true;
            }
        });
}
