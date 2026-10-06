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
            let available = crate::input::availability::BackendAvailability::current();
            egui::ComboBox::new("input_backend", tr!(text, "settings-input-keyboard-backend"))
                .selected_text(input_backend_label(&config.input.backend, text))
                .show_ui(ui, |ui| {
                    for backend in available.keyboards() {
                        let label = input_backend_label(&backend, text);
                        ui.selectable_value(&mut config.input.backend, backend, label);
                    }
                });
            #[cfg(target_os = "macos")]
            if config.input.backend == InputBackendKind::MacOsGameController {
                let key = match crate::input::gamecontroller::keyboard_status() {
                    1 => "settings-input-gc-keyboard-active",
                    2 => "settings-input-gc-keyboard-waiting",
                    4 => "settings-input-gc-keyboard-unsupported",
                    3 => "settings-input-gc-failed",
                    _ => "settings-input-gc-unavailable",
                };
                ui.label(text.text(key));
            }
            #[cfg(not(all(target_os = "linux", feature = "linux-evdev")))]
            if config.input.backend == InputBackendKind::LinuxEvdev {
                ui.label(tr!(text, "settings-input-evdev-unbuilt"));
                if ui.button(tr!(text, "settings-input-evdev-restore")).clicked() {
                    config.input.backend = InputBackendKind::Winit;
                }
            }
            #[cfg(target_os = "linux")]
            ui.checkbox(
                &mut config.input.linux_gamepad_legacy_poll,
                tr!(text, "settings-input-linux-poll"),
            );
            #[cfg(all(target_os = "linux", feature = "linux-evdev"))]
            if config.input.backend == InputBackendKind::LinuxEvdev {
                use crate::input::linux_evdev;
                ui.label(tr!(text, "settings-input-evdev-help"));
                if crate::input::linux_evdev::unsupported_key_seen() {
                    ui.label(tr!(text, "settings-input-evdev-unmapped"));
                }
                ui.label(text.text(match linux_evdev::status() {
                    1 => "settings-input-evdev-active",
                    2 => "settings-input-evdev-session",
                    3 => "settings-input-evdev-selection",
                    4 => "settings-input-evdev-permission",
                    5 => "settings-input-evdev-missing",
                    6 => "settings-input-evdev-failed",
                    7 => "settings-input-evdev-suppressed",
                    _ => "settings-input-evdev-inactive",
                }));
                // Explicit refresh only; never enumerate devices per input event.
                let id = ui.make_persistent_id("evdev_candidates");
                if ui.button(tr!(text, "settings-input-evdev-refresh")).clicked() {
                    ui.ctx().data_mut(|data| data.insert_temp(id, linux_evdev::device_paths()));
                }
                let paths =
                    ui.ctx().data(|data| data.get_temp::<Vec<String>>(id)).unwrap_or_default();
                for path in paths {
                    let mut selected = config.input.linux_evdev_devices.contains(&path);
                    if ui.checkbox(&mut selected, &path).changed() {
                        if selected {
                            config.input.linux_evdev_devices.push(path);
                        } else {
                            config.input.linux_evdev_devices.retain(|p| p != &path);
                        }
                    }
                }
                let mut paths = config.input.linux_evdev_devices.join("\n");
                ui.label(tr!(text, "settings-input-evdev-paths"));
                if ui.text_edit_multiline(&mut paths).changed() {
                    config.input.linux_evdev_devices = paths
                        .lines()
                        .map(str::trim)
                        .filter(|p| !p.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                if ui.button(tr!(text, "settings-input-evdev-restore")).clicked() {
                    config.input.backend = InputBackendKind::Winit;
                }
            }
            #[cfg(all(target_os = "macos", feature = "macos-iohid"))]
            if config.input.backend == InputBackendKind::MacOsHid {
                let status = crate::input::macos::status();
                let key = match status {
                    1 => "settings-input-macos-active",
                    2 => "settings-input-macos-permission",
                    3 => "settings-input-macos-failed",
                    4 => "settings-input-macos-exclusive",
                    _ => "settings-input-macos-inactive",
                };
                ui.label(text.text(key));
                if status == 2 && ui.button(tr!(text, "settings-input-macos-request")).clicked() {
                    crate::input::macos::request_permission();
                }
                if matches!(status, 2..=4) {
                    ui.label(tr!(text, "settings-input-macos-retry"));
                }
            }
            egui::ComboBox::new("gamepad_backend", tr!(text, "settings-input-gamepad-backend"))
                .selected_text(gamepad_backend_label(&config.input.gamepad_backend, text))
                .show_ui(ui, |ui| {
                    for backend in available.gamepads() {
                        let label = gamepad_backend_label(&backend, text);
                        ui.selectable_value(&mut config.input.gamepad_backend, backend, label);
                    }
                });
            #[cfg(target_os = "macos")]
            if config.input.gamepad_backend == GamepadBackendKind::GameController {
                ui.label(text.text(match crate::input::gamecontroller::pad_status() {
                    1 | 2 => "settings-input-gc-pad-active",
                    3 => "settings-input-gc-pad-waiting",
                    4 => "settings-input-gc-failed",
                    _ => "settings-input-gc-unavailable",
                }));
                ui.label(tr!(text, "settings-input-gc-pad-help"));
            }
            ui.checkbox(&mut config.input.keyboard_enabled, tr!(text, "settings-input-keyboard"));
            ui.checkbox(&mut config.input.gamepad_enabled, tr!(text, "settings-input-gamepad"));
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
            if config.input.gamepad_backend == GamepadBackendKind::GameController {
                ui.label(tr!(text, "settings-input-gc-assignment-help"));
            } else {
                ui.label(tr!(text, "settings-input-assignment-help"));
            }
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
