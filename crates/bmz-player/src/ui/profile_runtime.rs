use super::*;

impl EguiLayer {
    pub(crate) fn profile_operations_busy(&self) -> bool {
        self.ir_login.busy || self.ir_device_key.receiver.is_some() || self.show_course_editor
    }

    pub(crate) fn set_profile_change_available(&mut self, available: bool) {
        self.profile_manager.available = available && !self.profile_operations_busy();
    }

    pub(crate) fn profile_change_started(&mut self) {
        self.profile_manager.busy = true;
        self.profile_manager.error.clear();
        self.profile_manager.message.clear();
        self.cancel_key_config_listening();
    }

    pub(crate) fn reset_for_profile(&mut self, profile: &ProfileConfig) {
        self.ir_login = IrLoginUiState::default();
        self.ir_device_key = IrDeviceKeyUiState::default();
        self.key_config = EguiKeyConfigUiState::default();
        self.skin_ui_path_cache = SkinUiPathCache::default();
        self.profile_manager.selected_id = profile.id.clone();
        self.profile_manager.copy_source_id = profile.id.clone();
        self.show_fps = profile.ui.show_fps;
        self.show_random_trainer = false;
        self.score_import_status.clear();
        self.score_import_error.clear();
        self.replay_import_status.clear();
        self.replay_import_error.clear();
        self.replay_import_progress = None;
        SettingsFeedback::profile_changed(&self.ctx, &profile.id);
    }

    pub(crate) fn profile_change_finished(
        &mut self,
        action: &ProfileManagerAction,
        result: Result<(), String>,
        locale: crate::i18n::AppLocale,
    ) {
        self.profile_manager.busy = false;
        let text = Localizer::new(locale);
        match result {
            Ok(()) => {
                self.profile_manager.error.clear();
                self.profile_manager.message = match action {
                    ProfileManagerAction::Switch(id) => {
                        tr!(text, "profile-manager-switched", "id" => id.clone())
                    }
                    ProfileManagerAction::Create { id, activate, .. } => {
                        self.profile_manager.create_id.clear();
                        self.profile_manager.create_display_name.clear();
                        if *activate {
                            tr!(text, "profile-manager-switched", "id" => id.clone())
                        } else {
                            tr!(text, "profile-manager-created", "id" => id.clone())
                        }
                    }
                    ProfileManagerAction::Copy { source_id, id, activate, .. } => {
                        self.profile_manager.copy_target_id.clear();
                        self.profile_manager.copy_display_name.clear();
                        if *activate {
                            tr!(text, "profile-manager-switched", "id" => id.clone())
                        } else {
                            tr!(text, "profile-manager-copied", "source_id" => source_id.clone(), "target_id" => id.clone())
                        }
                    }
                };
            }
            Err(error) => {
                self.profile_manager.message.clear();
                self.profile_manager.error = error;
            }
        }
    }
}
