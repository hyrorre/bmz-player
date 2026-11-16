use super::*;
use crate::config::key_config::{
    COMMON_KEY_BINDING_SLOTS, KEY_BINDING_SLOTS, KEY_CONFIG_MODES, KeyBindingGroup, KeyBindingSlot,
    KeyBindingTarget, binding_target_label, format_play_binding, key_binding_conflict_description,
    key_binding_targets_for_group, key_mode_binding_targets,
};

pub(in crate::ui::profile_panel) fn build_profile_key_config_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    SettingsSection::new(SettingsPage::KeyConfig, tr!(section.text, "profile-key-config-title"))
        .scope(tr!(section.text, "settings-scope-profile"))
        .id_salt("profile_key_config")
        .show(ui, |ui| {
            if !section.unrestricted {
                section.key_config.listening = None;
                ui.label(tr!(section.text, "profile-key-config-unavailable"));
                return;
            }

            let previous_section = section.key_config.section;
            egui::ComboBox::new(
                "profile_key_config_mode",
                tr!(section.text, "profile-key-config-mode"),
            )
            .selected_text(key_config_section_label(section.text, section.key_config.section))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut section.key_config.section,
                    EguiKeyConfigSection::CommonSelect,
                    tr!(section.text, "profile-key-config-common-select"),
                );
                ui.selectable_value(
                    &mut section.key_config.section,
                    EguiKeyConfigSection::Play,
                    tr!(section.text, "settings-nav-play"),
                );
                ui.selectable_value(
                    &mut section.key_config.section,
                    EguiKeyConfigSection::Result,
                    tr!(section.text, "settings-nav-result"),
                );
                for &key_mode in KEY_CONFIG_MODES {
                    ui.selectable_value(
                        &mut section.key_config.section,
                        EguiKeyConfigSection::KeyMode(key_mode),
                        key_mode.as_str(),
                    );
                }
            });

            let slots = match section.key_config.section {
                EguiKeyConfigSection::CommonSelect => COMMON_KEY_BINDING_SLOTS,
                _ => KEY_BINDING_SLOTS,
            };
            // 1P / 2P の個別スロットは共通操作だけにある。他の区分へ移ったら共通コントローラーへ戻す。
            if !slots.contains(&section.key_config.slot) {
                section.key_config.slot = KeyBindingSlot::Controller;
            }
            let previous_slot = section.key_config.slot;
            egui::ComboBox::new(
                "profile_key_config_slot",
                tr!(section.text, "profile-key-config-slot"),
            )
            .selected_text(section.key_config.slot.suffix())
            .show_ui(ui, |ui| {
                for &slot in slots {
                    ui.selectable_value(&mut section.key_config.slot, slot, slot.suffix());
                }
            });
            if previous_section != section.key_config.section
                || previous_slot != section.key_config.slot
            {
                section.key_config.listening = None;
                section.key_config.status = None;
            }

            ui.label(tr!(section.text, "profile-key-config-help"));
            ui.separator();

            if section.key_config.section == EguiKeyConfigSection::KeyMode(KeyMode::K8) {
                build_eight_key_hispeed_rows(ui, section);
                ui.separator();
            }

            let slot = section.key_config.slot;
            if let EguiKeyConfigSection::KeyMode(key_mode) = section.key_config.section {
                build_key_binding_rows(
                    ui,
                    section,
                    key_mode,
                    None,
                    key_mode_binding_targets(key_mode, slot),
                );
            } else {
                let groups = section.key_config.section.action_groups();
                for &group in groups {
                    let targets = key_binding_targets_for_group(group, slot);
                    if groups.len() > 1 {
                        if targets.is_empty() {
                            continue;
                        }
                        ui.heading(section.text.text(group.label_key()));
                    }
                    build_key_binding_rows(ui, section, KeyMode::K7, Some(group), targets);
                }
            }

            if ui.button(tr!(section.text, "profile-key-config-defaults")).clicked() {
                section.key_config.listening = None;
                section.key_config.status = None;
                section.key_config_action = Some(EguiKeyConfigAction::RestoreDefaults {
                    section: section.key_config.section,
                    slot: section.key_config.slot,
                });
            }

            if let Some(status) = &section.key_config.status {
                let color = if status.error {
                    egui::Color32::LIGHT_RED
                } else {
                    egui::Color32::LIGHT_GREEN
                };
                ui.colored_label(color, &status.message);
            }
        });
}

fn key_config_section_label(text: Localizer, section: EguiKeyConfigSection) -> String {
    match section {
        EguiKeyConfigSection::CommonSelect => tr!(text, "profile-key-config-common-select"),
        EguiKeyConfigSection::Play => tr!(text, "settings-nav-play"),
        EguiKeyConfigSection::Result => tr!(text, "settings-nav-result"),
        EguiKeyConfigSection::KeyMode(key_mode) => key_mode.as_str().to_string(),
    }
}

fn build_key_binding_rows(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
    key_mode: KeyMode,
    group: Option<KeyBindingGroup>,
    targets: Vec<KeyBindingTarget>,
) {
    if targets.is_empty() {
        ui.weak(tr!(section.text, "profile-key-config-empty"));
        return;
    }

    egui::Grid::new(("profile_key_config_bindings", group))
        .num_columns(4)
        .striped(true)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for target in targets {
                ui.label(binding_target_label(key_mode, target));
                if let Some(conflict) =
                    key_binding_conflict_description(section.profile, key_mode, target)
                {
                    ui.colored_label(egui::Color32::YELLOW, "⚠").on_hover_text(conflict);
                } else {
                    ui.label("");
                }
                let listening = section
                    .key_config
                    .listening
                    .is_some_and(|active| active.key_mode == key_mode && active.target == target);
                let value = if listening {
                    if target.slot().is_controller() {
                        tr!(section.text, "profile-key-config-listen-button")
                    } else {
                        tr!(section.text, "profile-key-config-listen-key")
                    }
                } else {
                    format_play_binding(section.profile, key_mode, target)
                };
                if ui.add_sized([150.0, 22.0], egui::Button::new(value)).clicked() {
                    section.key_config.listening =
                        Some(EguiKeyConfigListenTarget { key_mode, target });
                    section.key_config.status = None;
                }
                if ui.small_button(tr!(section.text, "profile-key-config-clear")).clicked() {
                    section.key_config.listening = None;
                    section.key_config.status = None;
                    section.key_config_action =
                        Some(EguiKeyConfigAction::Clear { key_mode, target });
                }
                ui.end_row();
            }
        });
}

fn build_eight_key_hispeed_rows(ui: &mut egui::Ui, section: &mut ProfileSectionContext<'_>) {
    ui.label(tr!(section.text, "profile-key-config-8k-hispeed"));
    egui::Grid::new("profile_key_config_8k_hispeed")
        .num_columns(3)
        .striped(true)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for &entry_id in crate::config::settings_registry::SettingsEntryId::HISPEED_8K_ENTRIES {
                ui.label(entry_id.label());
                let current = crate::config::settings_registry::format_settings_value(
                    section.profile,
                    entry_id,
                );
                let up = current == "UP";
                if ui.selectable_label(up, "UP").clicked() && !up {
                    section.key_config_action =
                        Some(EguiKeyConfigAction::ToggleEightKeyHispeed { entry_id });
                }
                if ui.selectable_label(!up, "DOWN").clicked() && up {
                    section.key_config_action =
                        Some(EguiKeyConfigAction::ToggleEightKeyHispeed { entry_id });
                }
                ui.end_row();
            }
        });
}
