use super::*;

impl WinitApp {
    pub(super) fn capture_egui_key_config_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
    ) -> bool {
        let Some(egui) = self.ui.egui.as_mut() else {
            return false;
        };
        if !egui.key_config_listening() {
            return false;
        }
        if event.physical_key == PhysicalKey::Code(KeyCode::F1) {
            egui.cancel_key_config_listening();
            return false;
        }
        if event.state != ElementState::Pressed || event.repeat {
            return true;
        }
        let result = physical_key_name(event.physical_key)
            .map(|control| egui.capture_key_config_keyboard(&control))
            .unwrap_or(EguiKeyConfigInput::Consumed);
        self.apply_egui_key_config_input(result)
    }

    pub(super) fn capture_egui_key_config_gamepad(&mut self, control: &str, pressed: bool) -> bool {
        let Some(egui) = self.ui.egui.as_mut() else {
            return false;
        };
        if !egui.key_config_listening() {
            return false;
        }
        let result = if pressed {
            egui.capture_key_config_gamepad(control)
        } else {
            EguiKeyConfigInput::Consumed
        };
        self.apply_egui_key_config_input(result)
    }

    pub(super) fn capture_egui_key_config_axis(&mut self, control: &str) -> bool {
        let Some(egui) = self.ui.egui.as_mut() else {
            return false;
        };
        if !egui.key_config_listening() {
            return false;
        }
        let result = egui.capture_key_config_gamepad(control);
        self.apply_egui_key_config_input(result)
    }

    fn apply_egui_key_config_input(&mut self, input: EguiKeyConfigInput) -> bool {
        match input {
            EguiKeyConfigInput::NotHandled => false,
            EguiKeyConfigInput::Consumed => true,
            EguiKeyConfigInput::Action(action) => {
                self.apply_egui_key_config_action(action);
                true
            }
        }
    }

    pub(super) fn apply_egui_key_config_action(&mut self, action: EguiKeyConfigAction) {
        let (key_mode, target) = match &action {
            EguiKeyConfigAction::Bind { key_mode, target, .. }
            | EguiKeyConfigAction::Clear { key_mode, target } => (*key_mode, *target),
            EguiKeyConfigAction::ToggleEightKeyHispeed { entry_id } => (
                KeyMode::K8,
                KeyBindingTarget::Key {
                    lane: crate::config::settings_registry::eight_key_hispeed_lane(*entry_id)
                        .expect("egui only emits 8K hi-speed entries"),
                    slot: KeyBindingSlot::KeyboardPrimary,
                },
            ),
            EguiKeyConfigAction::RestoreDefaults { section, slot } => (
                match section {
                    EguiKeyConfigSection::KeyMode(key_mode) => *key_mode,
                    _ => KeyMode::K7,
                },
                KeyBindingTarget::Key {
                    lane: crate::config::profile_config::LaneConfig::Key1,
                    slot: *slot,
                },
            ),
        };
        let session = KeyConfigEditSession::begin(key_mode, target, &self.boot.profile_config);
        let result = match action {
            EguiKeyConfigAction::Bind { control, .. } => {
                apply_play_binding(&mut self.boot.profile_config.input, key_mode, target, &control)
            }
            EguiKeyConfigAction::Clear { .. } => {
                clear_play_binding(&mut self.boot.profile_config.input, key_mode, target)
            }
            EguiKeyConfigAction::ToggleEightKeyHispeed { entry_id } => {
                if crate::config::settings_registry::adjust_settings_value(
                    &mut self.boot.profile_config,
                    entry_id,
                    1,
                ) {
                    Ok(())
                } else {
                    return;
                }
            }
            EguiKeyConfigAction::RestoreDefaults { section, slot } => match section {
                EguiKeyConfigSection::Common => {
                    crate::config::key_config::restore_action_group_defaults(
                        &mut self.boot.profile_config.input,
                        crate::config::key_config::KeyBindingGroup::Common,
                        slot,
                    );
                    Ok(())
                }
                EguiKeyConfigSection::Select => {
                    crate::config::key_config::restore_action_group_defaults(
                        &mut self.boot.profile_config.input,
                        crate::config::key_config::KeyBindingGroup::Select,
                        slot,
                    );
                    Ok(())
                }
                EguiKeyConfigSection::Play => {
                    crate::config::key_config::restore_action_group_defaults(
                        &mut self.boot.profile_config.input,
                        crate::config::key_config::KeyBindingGroup::Play,
                        slot,
                    );
                    Ok(())
                }
                EguiKeyConfigSection::Result => {
                    crate::config::key_config::restore_action_group_defaults(
                        &mut self.boot.profile_config.input,
                        crate::config::key_config::KeyBindingGroup::Result,
                        slot,
                    );
                    Ok(())
                }
                EguiKeyConfigSection::KeyMode(key_mode) => {
                    crate::config::key_config::restore_key_mode_defaults(
                        &mut self.boot.profile_config.input,
                        key_mode,
                        slot,
                    )
                }
            },
        };
        if let Err(error) = result {
            tracing::warn!(%error, ?key_mode, ?target, "failed to apply egui key binding");
            session.cancel(&mut self.boot.profile_config);
            self.set_egui_key_config_status(false);
            return;
        }

        if self.persist_key_config_edit_session(&session) {
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            self.set_egui_key_config_status(true);
            tracing::info!(?key_mode, ?target, "egui key config saved");
        } else {
            self.set_egui_key_config_status(false);
            tracing::error!(?key_mode, ?target, "failed to save egui key config");
        }
    }

    pub(super) fn persist_key_config_edit_session(
        &mut self,
        session: &KeyConfigEditSession,
    ) -> bool {
        let previous_updated_at = self.boot.profile_config.updated_at;
        self.boot.profile_config.updated_at = now_unix_seconds();
        if let Err(error) = self.save_profile_config_now() {
            tracing::error!(%error, ?session.target, "failed to save key config");
            session.cancel(&mut self.boot.profile_config);
            self.boot.profile_config.updated_at = previous_updated_at;
            return false;
        }

        self.select.select_keys = SelectKeyBindings::from_profile(&self.boot.profile_config.input);
        self.clear_select_hold();
        self.invalidate_play_preload();
        self.play.play_media_cache = None;
        self.suppress_select_analog_until_idle();
        true
    }

    fn set_egui_key_config_status(&mut self, success: bool) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let message = if success {
            text.text("profile-key-config-saved")
        } else {
            text.text("profile-key-config-save-failed")
        };
        if let Some(egui) = self.ui.egui.as_mut() {
            egui.set_key_config_status(message, !success);
        }
        self.request_redraw();
    }
}
