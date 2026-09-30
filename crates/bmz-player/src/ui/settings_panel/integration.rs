use super::*;

pub(super) fn build_integration_settings_sections(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    text: Localizer,
    state: &mut SettingsPanelState<'_>,
    save_clicked: &mut bool,
    check_update_clicked: &mut bool,
) {
    SettingsSection::new(SettingsPage::General, tr!(text, "settings-updates-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_updates")
        .show(ui, |ui| {
            ui.checkbox(&mut config.updates.enabled, tr!(text, "settings-updates-notifications"));
            ui.checkbox(
                &mut config.updates.check_on_startup,
                tr!(text, "settings-updates-on-startup"),
            );
            egui::ComboBox::new("updates_channel", tr!(text, "settings-updates-channel"))
                .selected_text(update_channel_label(config.updates.channel))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.updates.channel,
                        UpdateChannelConfig::Stable,
                        update_channel_label(UpdateChannelConfig::Stable),
                    );
                    ui.selectable_value(
                        &mut config.updates.channel,
                        UpdateChannelConfig::Prerelease,
                        update_channel_label(UpdateChannelConfig::Prerelease),
                    );
                });
            if config.updates.skipped_version.is_empty() {
                ui.label(tr!(text, "settings-updates-no-skipped-release"));
            } else {
                ui.horizontal(|ui| {
                    ui.label(tr!(
                        text,
                        "settings-updates-skipping",
                        "version" => config.updates.skipped_version.as_str()
                    ));
                    if ui.button(tr!(text, "common-clear")).clicked() {
                        config.updates.skipped_version.clear();
                        *save_clicked = true;
                    }
                });
            }
            if ui.button(tr!(text, "settings-updates-check")).clicked() {
                *check_update_clicked = true;
            }
        });

    SettingsSection::new(SettingsPage::Integration, "Discord")
        .scope(tr!(text, "settings-scope-app"))
        .show(ui, |ui| {
            ui.checkbox(&mut config.discord.enabled, "Rich Presence");
            ui.horizontal(|ui| {
                ui.label("Application ID");
                ui.add(
                    egui::TextEdit::singleline(&mut config.discord.application_id)
                        .desired_width(260.0)
                        .hint_text(tr!(text, "settings-discord-default-hint")),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Large image key");
                ui.add(
                    egui::TextEdit::singleline(&mut config.discord.large_image_key)
                        .desired_width(160.0)
                        .hint_text("bmz"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Large image text");
                ui.add(
                    egui::TextEdit::singleline(&mut config.discord.large_image_text)
                        .desired_width(220.0)
                        .hint_text("BMZ Player"),
                );
            });
            ui.checkbox(
                &mut config.discord.show_song_details,
                tr!(text, "settings-discord-song-details"),
            );
            ui.label(tr!(text, "settings-discord-default-help"));
        });

    SettingsSection::new(SettingsPage::InputDevices, tr!(text, "settings-input-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_input")
        .show(ui, |ui| {
            egui::ComboBox::new("input_backend", tr!(text, "settings-input-keyboard-backend"))
                .selected_text(input_backend_label(&config.input.backend, text))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.input.backend,
                        InputBackendKind::Auto,
                        input_backend_label(&InputBackendKind::Auto, text),
                    );
                    ui.selectable_value(
                        &mut config.input.backend,
                        InputBackendKind::Winit,
                        input_backend_label(&InputBackendKind::Winit, text),
                    );
                    ui.selectable_value(
                        &mut config.input.backend,
                        InputBackendKind::RawInput,
                        input_backend_label(&InputBackendKind::RawInput, text),
                    );
                    #[cfg(target_os = "macos")]
                    ui.selectable_value(
                        &mut config.input.backend,
                        InputBackendKind::MacOsHid,
                        "macOS IOHID",
                    );
                });
            #[cfg(target_os = "macos")]
            if config.input.backend == InputBackendKind::MacOsHid {
                let key = match crate::input::macos::status() {
                    1 => "settings-input-macos-active",
                    2 => "settings-input-macos-permission",
                    3 => "settings-input-macos-failed",
                    _ => "settings-input-macos-inactive",
                };
                ui.label(text.text(key));
                if ui.button(tr!(text, "settings-input-macos-request")).clicked() {
                    crate::input::macos::request_permission();
                }
                ui.label(tr!(text, "settings-input-macos-retry"));
            }
            egui::ComboBox::new("gamepad_backend", tr!(text, "settings-input-gamepad-backend"))
                .selected_text(gamepad_backend_label(&config.input.gamepad_backend, text))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.input.gamepad_backend,
                        GamepadBackendKind::Auto,
                        gamepad_backend_label(&GamepadBackendKind::Auto, text),
                    );
                    ui.selectable_value(
                        &mut config.input.gamepad_backend,
                        GamepadBackendKind::Gilrs,
                        gamepad_backend_label(&GamepadBackendKind::Gilrs, text),
                    );
                    #[cfg(windows)]
                    ui.selectable_value(
                        &mut config.input.gamepad_backend,
                        GamepadBackendKind::RawInput,
                        gamepad_backend_label(&GamepadBackendKind::RawInput, text),
                    );
                    #[cfg(all(windows, feature = "experimental-gameinput"))]
                    ui.selectable_value(
                        &mut config.input.gamepad_backend,
                        GamepadBackendKind::GameInput,
                        gamepad_backend_label(&GamepadBackendKind::GameInput, text),
                    );
                });
            ui.checkbox(&mut config.input.keyboard_enabled, tr!(text, "settings-input-keyboard"));
            ui.checkbox(&mut config.input.gamepad_enabled, tr!(text, "settings-input-gamepad"));
            ui.label(tr!(text, "settings-input-backend-help"));
            ui.separator();
            ui.label(tr!(text, "settings-input-controller-assignment"));
            ui.label(tr!(
                text,
                "settings-input-connected-count",
                "count" => state.connected_gamepads.iter().filter(|pad| pad.is_connected).count()
            ));
            if state.connected_gamepads.is_empty() {
                ui.label(tr!(text, "settings-input-no-gamepads"));
            } else {
                for pad in state.connected_gamepads {
                    let status = if pad.is_connected {
                        tr!(text, "common-connected")
                    } else {
                        tr!(text, "common-disconnected")
                    };
                    ui.label(format!("#{} {} ({})", pad.backend_id, pad.name, status));
                }
            }
            for (slot_index, label) in [
                (0usize, tr!(text, "settings-input-controller-1p")),
                (1usize, tr!(text, "settings-input-controller-2p")),
            ] {
                let current = config.input.gamepad_slot_device_ids[slot_index].as_deref();
                let selected_text = match current {
                    Some(stable_id) => state
                        .connected_gamepads
                        .iter()
                        .find(|pad| pad.stable_id == stable_id)
                        .map(|pad| format!("#{} {}", pad.backend_id, pad.name))
                        .unwrap_or_else(|| {
                            let end = stable_id.len().min(20);
                            tr!(
                                text,
                                "settings-input-device-disconnected",
                                "device" => format!("{}...", &stable_id[..end])
                            )
                        }),
                    None => config.input.gamepad_slot_gilrs_ids[slot_index]
                        .and_then(|id| {
                            state.connected_gamepads.iter().find(|pad| pad.backend_id == id).map(
                                |pad| {
                                    tr!(
                                        text,
                                        "settings-input-legacy-device",
                                        "device" => format!("#{} {}", pad.backend_id, pad.name)
                                    )
                                },
                            )
                        })
                        .unwrap_or_else(|| tr!(text, "settings-input-auto-order")),
                };
                egui::ComboBox::from_label(label).selected_text(selected_text).show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut config.input.gamepad_slot_device_ids[slot_index],
                            None,
                            tr!(text, "settings-input-auto-order"),
                        )
                        .clicked()
                    {
                        config.input.gamepad_slot_gilrs_ids[slot_index] = None;
                    }
                    for pad in state.connected_gamepads {
                        if ui
                            .selectable_value(
                                &mut config.input.gamepad_slot_device_ids[slot_index],
                                Some(pad.stable_id.clone()),
                                format!("#{} {}", pad.backend_id, pad.name),
                            )
                            .clicked()
                        {
                            config.input.gamepad_slot_gilrs_ids[slot_index] = None;
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.button(tr!(text, "settings-input-auto-assign")).clicked() {
                    let connected: Vec<String> = state
                        .connected_gamepads
                        .iter()
                        .filter(|pad| pad.is_connected)
                        .map(|pad| pad.stable_id.clone())
                        .collect();
                    config.input.gamepad_slot_device_ids[0] = connected.first().cloned();
                    config.input.gamepad_slot_device_ids[1] = connected.get(1).cloned();
                    config.input.gamepad_slot_gilrs_ids = [None, None];
                }
                if ui.button(tr!(text, "settings-input-swap")).clicked() {
                    config.input.gamepad_slot_device_ids.swap(0, 1);
                    config.input.gamepad_slot_gilrs_ids.swap(0, 1);
                }
                if ui.button(tr!(text, "settings-input-clear-assignment")).clicked() {
                    config.input.gamepad_slot_device_ids = [None, None];
                    config.input.gamepad_slot_gilrs_ids = [None, None];
                }
            });
            ui.label(tr!(text, "settings-input-assignment-help"));
        });

    SettingsSection::new(SettingsPage::General, tr!(text, "settings-logging-title"))
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_logging")
        .show(ui, |ui| {
            egui::ComboBox::new("logging_level", tr!(text, "settings-logging-level"))
                .selected_text(log_level_label(&config.logging.level))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut config.logging.level,
                        LogLevel::Trace,
                        log_level_label(&LogLevel::Trace),
                    );
                    ui.selectable_value(
                        &mut config.logging.level,
                        LogLevel::Debug,
                        log_level_label(&LogLevel::Debug),
                    );
                    ui.selectable_value(
                        &mut config.logging.level,
                        LogLevel::Info,
                        log_level_label(&LogLevel::Info),
                    );
                    ui.selectable_value(
                        &mut config.logging.level,
                        LogLevel::Warn,
                        log_level_label(&LogLevel::Warn),
                    );
                    ui.selectable_value(
                        &mut config.logging.level,
                        LogLevel::Error,
                        log_level_label(&LogLevel::Error),
                    );
                });
            ui.checkbox(&mut config.logging.file_logging, tr!(text, "settings-logging-file"));
            ui.label(tr!(text, "settings-logging-help"));
        });

    ui.separator();
}
