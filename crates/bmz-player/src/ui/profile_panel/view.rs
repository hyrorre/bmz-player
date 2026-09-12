use super::*;

pub(in crate::ui) fn build_profile_settings_panel(
    ui: &mut egui::Ui,
    context: ProfileSettingsPanelContext<'_>,
) -> ProfileSettingsPanelActions {
    let ProfileSettingsPanelContext {
        profile,
        app_config,
        show_fps,
        ir_login,
        ir_device_key,
        profile_manager,
        key_config,
        profile_root,
        unrestricted,
        text,
    } = context;

    if !SettingsNavigation::load(ui.ctx()).accepts_key_capture() || !unrestricted {
        key_config.listening = None;
    }

    // 非同期ログインの完了は runtime 側で、表示ページに関係なく反映する。
    let save_clicked = false;
    let readonly_profile = (!unrestricted).then(|| profile.clone());
    let readonly_app_config = (!unrestricted).then(|| app_config.clone());
    let mut section = ProfileSectionContext {
        profile,
        app_config,
        show_fps,
        ir_login,
        ir_device_key,
        profile_manager,
        key_config,
        profile_root,
        unrestricted,
        text,
        save_clicked,
        save_app_config: false,
        key_config_action: None,
    };

    if !section.unrestricted && !SettingsNavigation::load(ui.ctx()).page.editable_during_play() {
        ui.label(tr!(section.text, "profile-settings-restricted"));
        ui.separator();
    }
    build_profile_basic_section(ui, &mut section);
    section.save_app_config |= build_profile_manager_section(
        ui,
        section.app_config,
        section.profile,
        section.profile_manager,
        section.unrestricted,
        section.text,
    );
    build_profile_volume_section(ui, &mut section);
    build_profile_judge_section(ui, &mut section);
    build_profile_play_section(ui, &mut section);
    build_profile_display_section(ui, &mut section);
    build_profile_select_section(ui, &mut section);
    build_profile_input_section(ui, &mut section);
    build_profile_key_config_section(ui, &mut section);
    build_profile_replay_section(ui, &mut section);
    build_profile_system_sound_section(ui, &mut section);
    build_profile_ir_section(ui, &mut section);
    build_profile_ui_section(ui, &mut section);

    if let Some(readonly) = readonly_profile {
        restore_restricted_profile_settings(section.profile, readonly);
    }
    if let Some(readonly) = readonly_app_config {
        *section.app_config = readonly;
        section.save_app_config = false;
    }
    ProfileSettingsPanelActions {
        save: section.save_clicked,
        save_app_config: section.save_app_config,
        key_config_action: section.key_config_action,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_fps_checkbox_remains_editable_and_persisted_during_play() {
        let ctx = egui::Context::default();
        SettingsNavigation::select(&ctx, SettingsPage::General);
        let text = Localizer::new(AppLocale::En);
        let mut profile = ProfileConfig::new_default("test", "Test", 1);
        profile.ui.show_fps = false;
        let mut app_config = AppConfig::default();
        let mut show_fps = false;
        let mut ir_login = IrLoginUiState::default();
        let mut ir_device_key = IrDeviceKeyUiState::default();
        let mut profile_manager = ProfileManagerUiState::default();
        let mut key_config = EguiKeyConfigUiState::default();
        let mut frame = |events| {
            ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
                build_profile_settings_panel(
                    ui,
                    ProfileSettingsPanelContext {
                        profile: &mut profile,
                        app_config: &mut app_config,
                        show_fps: &mut show_fps,
                        ir_login: &mut ir_login,
                        ir_device_key: &mut ir_device_key,
                        profile_manager: &mut profile_manager,
                        key_config: &mut key_config,
                        profile_root: std::path::Path::new("."),
                        unrestricted: false,
                        text,
                    },
                );
            })
        };
        frame(vec![]);
        let output = frame(vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(label) = &shape.shape
                    && label.galley.job.text == text.text("settings-show-fps")
                {
                    Some(label.galley.rect.translate(label.pos.to_vec2()).center())
                } else {
                    None
                }
            })
            .expect("General FPS checkbox is visible");
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
        frame(vec![]);
        assert!(show_fps);
        assert!(profile.ui.show_fps, "restricted restore must preserve the edited UI setting");
    }
}
