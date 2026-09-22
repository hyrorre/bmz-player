pub(in crate::ui) fn build_profile_manager_section(
    ui: &mut egui::Ui,
    app_paths: &AppPaths,
    profile: &ProfileConfig,
    state: &mut ProfileManagerUiState,
    editable: bool,
    text: Localizer,
) -> Option<ProfileManagerAction> {
    let mut action = None;
    SettingsSection::new(SettingsPage::Profile, tr!(text, "profile-manager-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("profile_manager")
        .show(ui, |ui| {
            if !editable || !state.available || state.busy {
                ui.label(tr!(text, "profile-manager-unavailable"));
                ui.disable();
            }
            let profiles = match profile_cmd::profile_summaries(app_paths) {
                Ok(profiles) => profiles,
                Err(error) => {
                    ui.colored_label(egui::Color32::RED, format!("{error:#}"));
                    return;
                }
            };

            if state.copy_source_id.is_empty() {
                state.copy_source_id = profile.id.clone();
            }
            if state.selected_id.is_empty() {
                state.selected_id = profile.id.clone();
            }

            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-manager-current"));
                ui.monospace(&profile.id);
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-manager-target"));
                egui::ComboBox::from_id_salt("profile_switch_target")
                    .selected_text(profile_selection_label(&profiles, &state.selected_id))
                    .show_ui(ui, |ui| {
                        for summary in &profiles {
                            let label = profile_selection_label(&profiles, &summary.id);
                            ui.selectable_value(&mut state.selected_id, summary.id.clone(), label);
                        }
                    });
                if ui
                    .add_enabled(
                        state.selected_id != profile.id,
                        egui::Button::new(tr!(text, "profile-manager-switch")),
                    )
                    .clicked()
                {
                    action = Some(ProfileManagerAction::Switch(state.selected_id.clone()));
                }
            });

            ui.separator();
            ui.label(tr!(text, "profile-manager-create-title"));
            ui.horizontal(|ui| {
                ui.label("ID");
                profile_id_text_edit(ui, &mut state.create_id);
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-display-name"));
                ui.text_edit_singleline(&mut state.create_display_name);
            });
            ui.checkbox(&mut state.create_activate, tr!(text, "profile-manager-activate"));
            if ui.button(tr!(text, "profile-manager-create")).clicked() {
                let id = state.create_id.trim().to_string();
                let display_name =
                    trimmed_non_empty(&state.create_display_name).map(str::to_string);
                action = Some(ProfileManagerAction::Create {
                    id,
                    display_name,
                    activate: state.create_activate,
                });
            }

            ui.separator();
            ui.label(tr!(text, "profile-manager-copy-title"));
            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-manager-copy-source"));
                egui::ComboBox::from_id_salt("profile_copy_source")
                    .selected_text(profile_selection_label(&profiles, &state.copy_source_id))
                    .show_ui(ui, |ui| {
                        for summary in &profiles {
                            let selected = summary.id == state.copy_source_id;
                            let label = profile_selection_label(&profiles, &summary.id);
                            if ui.selectable_label(selected, label).clicked() {
                                state.copy_source_id = summary.id.clone();
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-manager-new-id"));
                profile_id_text_edit(ui, &mut state.copy_target_id);
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-display-name"));
                ui.text_edit_singleline(&mut state.copy_display_name);
            });
            ui.checkbox(&mut state.copy_activate, tr!(text, "profile-manager-activate"));
            if ui.button(tr!(text, "profile-manager-copy")).clicked() {
                let source_id = state.copy_source_id.trim().to_string();
                let target_id = state.copy_target_id.trim().to_string();
                let display_name = trimmed_non_empty(&state.copy_display_name).map(str::to_string);
                action = Some(ProfileManagerAction::Copy {
                    source_id,
                    id: target_id,
                    display_name,
                    activate: state.copy_activate,
                });
            }

            if !state.message.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_GREEN, state.message.as_str());
            }
            if !state.error.is_empty() {
                ui.colored_label(egui::Color32::RED, state.error.as_str());
            }
        });
    action
}
use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_manager_requests_switch_only_when_available_and_does_not_activate_itself() {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let boot = data.boot();
        profile_cmd::create_profile(&data.paths, "other", None, false).unwrap();
        for (available, busy, editable) in
            [(true, false, true), (false, false, true), (true, true, true), (true, false, false)]
        {
            let ctx = egui::Context::default();
            SettingsNavigation::select(&ctx, SettingsPage::Profile);
            let text = Localizer::new(AppLocale::En);
            let mut state = ProfileManagerUiState {
                selected_id: "other".into(),
                available,
                busy,
                ..Default::default()
            };
            let mut actions = Vec::new();
            let mut frame = |events| {
                ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
                    if let Some(action) = build_profile_manager_section(
                        ui,
                        &data.paths,
                        &boot.profile_config,
                        &mut state,
                        editable,
                        text,
                    ) {
                        actions.push(action);
                    }
                })
            };
            frame(vec![]);
            let output = frame(vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(label) = &shape.shape
                        && label.galley.job.text == text.text("profile-manager-switch")
                    {
                        Some(label.galley.rect.translate(label.pos.to_vec2()).center())
                    } else {
                        None
                    }
                })
                .expect("switch button is visible");
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            if available && !busy && editable {
                assert!(
                    matches!(actions.as_slice(), [ProfileManagerAction::Switch(id)] if id == "other")
                );
            } else {
                assert!(actions.is_empty());
            }
            assert_eq!(boot.profile_config.id, "default");
            assert_eq!(
                crate::config::load::load_app_config(&data.paths.config_toml)
                    .unwrap()
                    .active_profile,
                "default"
            );
        }
    }
}
