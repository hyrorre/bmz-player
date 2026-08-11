use super::*;

pub(in crate::ui) fn build_obs_settings_section(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    state: &mut ObsScenePickerState,
    connection_status: &crate::obs::ObsConnectionStatus,
    text: Localizer,
) -> bool {
    state.poll(text);
    let mut enabled_changed = false;
    egui::CollapsingHeader::new("OBS WebSocket").id_salt("settings_obs").show(ui, |ui| {
        enabled_changed =
            ui.checkbox(&mut config.obs.enabled, tr!(text, "settings-obs-enabled")).changed();
        let (status_label, status_color) =
            obs_connection_status_label(connection_status.kind, text);
        ui.horizontal(|ui| {
            ui.label(tr!(text, "settings-obs-connection-status"));
            ui.colored_label(status_color, status_label);
            if let Some(retry_in_ms) = connection_status.retry_in_ms {
                ui.label(tr!(
                    text,
                    "settings-obs-next-retry",
                    "seconds" => retry_in_ms as f64 / 1000.0
                ));
            }
        });
        if let Some(detail) = &connection_status.detail {
            ui.label(detail);
        }
        if let Some(error) = &connection_status.last_error {
            ui.colored_label(egui::Color32::RED, error);
        }
        ui.horizontal(|ui| {
            ui.label(tr!(text, "settings-obs-host"));
            ui.add(
                egui::TextEdit::singleline(&mut config.obs.host)
                    .desired_width(180.0)
                    .hint_text("localhost"),
            );
            ui.label(tr!(text, "settings-obs-port"));
            ui.add(egui::DragValue::new(&mut config.obs.port).range(0..=65535));
        });
        ui.horizontal(|ui| {
            ui.label(tr!(text, "settings-obs-password"));
            ui.add(
                egui::TextEdit::singleline(&mut config.obs.password)
                    .desired_width(220.0)
                    .password(true),
            );
        });
        egui::ComboBox::new("obs_recording_mode", tr!(text, "settings-obs-recording-mode"))
            .selected_text(obs_recording_mode_label(config.obs.recording_mode, text))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut config.obs.recording_mode,
                    ObsRecordingMode::KeepAll,
                    obs_recording_mode_label(ObsRecordingMode::KeepAll, text),
                );
                ui.selectable_value(
                    &mut config.obs.recording_mode,
                    ObsRecordingMode::OnScreenshot,
                    obs_recording_mode_label(ObsRecordingMode::OnScreenshot, text),
                );
                ui.selectable_value(
                    &mut config.obs.recording_mode,
                    ObsRecordingMode::OnReplay,
                    obs_recording_mode_label(ObsRecordingMode::OnReplay, text),
                );
            });
        ui.add(
            egui::Slider::new(&mut config.obs.record_stop_wait_ms, 0..=10_000)
                .text(tr!(text, "settings-obs-stop-delay")),
        );

        ui.horizontal(|ui| {
            if ui
                .add_enabled(!state.busy, egui::Button::new(tr!(text, "settings-obs-load-scenes")))
                .clicked()
            {
                state.start_load(config.obs.clone());
            }
            if state.busy {
                ui.label(tr!(text, "common-loading"));
            }
        });
        if !state.message.is_empty() {
            ui.label(state.message.as_str());
        }
        if !state.error.is_empty() {
            ui.colored_label(egui::Color32::RED, state.error.as_str());
        }

        ui.separator();
        ui.strong(tr!(text, "settings-obs-state-settings"));
        egui::Grid::new("obs_state_mapping_grid").striped(true).show(ui, |ui| {
            ui.label(tr!(text, "settings-obs-state"));
            ui.label(tr!(text, "settings-obs-scene"));
            ui.label(tr!(text, "settings-obs-action"));
            ui.end_row();
            for event in crate::obs::ObsEventKey::ALL {
                let key = event.config_key();
                ui.label(obs_event_label(event, text));

                let mut scene = config.obs.scenes.get(key).cloned().unwrap_or_default();
                let selected_scene = if scene.is_empty() {
                    tr!(text, "settings-obs-no-change")
                } else {
                    scene.clone()
                };
                egui::ComboBox::from_id_salt(("obs_scene", key))
                    .selected_text(selected_scene)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut scene,
                            String::new(),
                            tr!(text, "settings-obs-no-change"),
                        );
                        if !scene.is_empty() && !state.scenes.iter().any(|name| name == &scene) {
                            let current_scene = scene.clone();
                            ui.selectable_value(&mut scene, current_scene.clone(), current_scene);
                        }
                        for candidate in &state.scenes {
                            ui.selectable_value(&mut scene, candidate.clone(), candidate);
                        }
                    });
                if scene.is_empty() {
                    config.obs.scenes.remove(key);
                } else {
                    config.obs.scenes.insert(key.to_string(), scene);
                }

                let mut action = config.obs.actions.get(key).copied().unwrap_or_default();
                egui::ComboBox::from_id_salt(("obs_action", key))
                    .selected_text(obs_action_label(action, text))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut action,
                            ObsActionConfig::None,
                            obs_action_label(ObsActionConfig::None, text),
                        );
                        ui.selectable_value(
                            &mut action,
                            ObsActionConfig::StartRecord,
                            obs_action_label(ObsActionConfig::StartRecord, text),
                        );
                        ui.selectable_value(
                            &mut action,
                            ObsActionConfig::StopRecord,
                            obs_action_label(ObsActionConfig::StopRecord, text),
                        );
                    });
                if action == ObsActionConfig::None {
                    config.obs.actions.remove(key);
                } else {
                    config.obs.actions.insert(key.to_string(), action);
                }
                ui.end_row();
            }
        });
    });
    enabled_changed
}

pub(in crate::ui) fn build_play_overlay_settings_section(
    ui: &mut egui::Ui,
    profile: &mut ProfileConfig,
    text: Localizer,
) -> bool {
    let config = &mut profile.play_overlay;
    let mut changed = false;
    egui::CollapsingHeader::new(tr!(text, "settings-play-overlay-title"))
        .id_salt("settings_play_overlay")
        .show(ui, |ui| {
            changed |= ui
                .checkbox(
                    &mut config.websocket_enabled,
                    tr!(text, "settings-play-overlay-websocket-enabled"),
                )
                .changed();
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-play-overlay-websocket-url"));
                ui.monospace(format!("ws://127.0.0.1:{}", config.websocket_port));
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-play-overlay-port"));
                changed |= ui
                    .add(egui::DragValue::new(&mut config.websocket_port).range(1..=65535))
                    .changed();
                ui.label(tr!(text, "settings-play-overlay-update-rate"));
                egui::ComboBox::from_id_salt("settings_play_overlay_update_rate")
                    .selected_text(play_overlay_update_rate_label(
                        config.websocket_update_rate,
                        text,
                    ))
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(
                                &mut config.websocket_update_rate,
                                PlayOverlayUpdateRateConfig::Fps60,
                                play_overlay_update_rate_label(
                                    PlayOverlayUpdateRateConfig::Fps60,
                                    text,
                                ),
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut config.websocket_update_rate,
                                PlayOverlayUpdateRateConfig::Fps120,
                                play_overlay_update_rate_label(
                                    PlayOverlayUpdateRateConfig::Fps120,
                                    text,
                                ),
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut config.websocket_update_rate,
                                PlayOverlayUpdateRateConfig::Fps240,
                                play_overlay_update_rate_label(
                                    PlayOverlayUpdateRateConfig::Fps240,
                                    text,
                                ),
                            )
                            .changed();
                    });
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-play-overlay-release-window"));
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut config.release_window_ms)
                            .range(100..=60000)
                            .suffix(" ms"),
                    )
                    .changed();
                ui.label(tr!(text, "settings-play-overlay-ln-threshold"));
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut config.release_ignore_threshold_ms)
                            .range(0..=5000)
                            .suffix(" ms"),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-play-overlay-release-ok"));
                let ok_changed = ui
                    .add(
                        egui::DragValue::new(&mut config.release_ok_threshold_ms)
                            .range(0..=5000)
                            .suffix(" ms"),
                    )
                    .changed();
                ui.label(tr!(text, "settings-play-overlay-release-ng"));
                let ng_changed = ui
                    .add(
                        egui::DragValue::new(&mut config.release_ng_threshold_ms)
                            .range(0..=5000)
                            .suffix(" ms"),
                    )
                    .changed();
                if ok_changed && config.release_ok_threshold_ms > config.release_ng_threshold_ms {
                    config.release_ok_threshold_ms = config.release_ng_threshold_ms;
                }
                if ng_changed && config.release_ng_threshold_ms < config.release_ok_threshold_ms {
                    config.release_ng_threshold_ms = config.release_ok_threshold_ms;
                }
                changed |= ok_changed || ng_changed;
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-play-overlay-controller-mode"));
                egui::ComboBox::from_id_salt("settings_play_overlay_controller_mode")
                    .selected_text(play_overlay_controller_mode_label(config.controller_mode, text))
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(
                                &mut config.controller_mode,
                                PlayOverlayControllerModeConfig::Key7P1,
                                play_overlay_controller_mode_label(
                                    PlayOverlayControllerModeConfig::Key7P1,
                                    text,
                                ),
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut config.controller_mode,
                                PlayOverlayControllerModeConfig::Key7P2,
                                play_overlay_controller_mode_label(
                                    PlayOverlayControllerModeConfig::Key7P2,
                                    text,
                                ),
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut config.controller_mode,
                                PlayOverlayControllerModeConfig::Key14,
                                play_overlay_controller_mode_label(
                                    PlayOverlayControllerModeConfig::Key14,
                                    text,
                                ),
                            )
                            .changed();
                    });
            });
            ui.horizontal(|ui| {
                ui.label(tr!(text, "settings-play-overlay-display-mode"));
                egui::ComboBox::from_id_salt("settings_play_overlay_release_display_mode")
                    .selected_text(play_overlay_release_display_mode_label(
                        config.release_display_mode,
                        text,
                    ))
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(
                                &mut config.release_display_mode,
                                PlayOverlayReleaseDisplayModeConfig::ReleaseOnly,
                                play_overlay_release_display_mode_label(
                                    PlayOverlayReleaseDisplayModeConfig::ReleaseOnly,
                                    text,
                                ),
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut config.release_display_mode,
                                PlayOverlayReleaseDisplayModeConfig::ReleaseAndNotes,
                                play_overlay_release_display_mode_label(
                                    PlayOverlayReleaseDisplayModeConfig::ReleaseAndNotes,
                                    text,
                                ),
                            )
                            .changed();
                        changed |= ui
                            .selectable_value(
                                &mut config.release_display_mode,
                                PlayOverlayReleaseDisplayModeConfig::NotesOnly,
                                play_overlay_release_display_mode_label(
                                    PlayOverlayReleaseDisplayModeConfig::NotesOnly,
                                    text,
                                ),
                            )
                            .changed();
                    });
            });
            ui.small(tr!(text, "settings-play-overlay-html-preview-help"));
        });
    changed
}

fn play_overlay_update_rate_label(rate: PlayOverlayUpdateRateConfig, text: Localizer) -> String {
    match rate {
        PlayOverlayUpdateRateConfig::Fps60 => tr!(text, "settings-play-overlay-update-rate-60"),
        PlayOverlayUpdateRateConfig::Fps120 => tr!(text, "settings-play-overlay-update-rate-120"),
        PlayOverlayUpdateRateConfig::Fps240 => tr!(text, "settings-play-overlay-update-rate-240"),
    }
}

fn play_overlay_controller_mode_label(
    mode: PlayOverlayControllerModeConfig,
    text: Localizer,
) -> String {
    match mode {
        PlayOverlayControllerModeConfig::Key7P1 => {
            tr!(text, "settings-play-overlay-controller-7k-p1")
        }
        PlayOverlayControllerModeConfig::Key7P2 => {
            tr!(text, "settings-play-overlay-controller-7k-p2")
        }
        PlayOverlayControllerModeConfig::Key14 => {
            tr!(text, "settings-play-overlay-controller-14k")
        }
    }
}

fn play_overlay_release_display_mode_label(
    mode: PlayOverlayReleaseDisplayModeConfig,
    text: Localizer,
) -> String {
    match mode {
        PlayOverlayReleaseDisplayModeConfig::ReleaseOnly => {
            tr!(text, "settings-play-overlay-display-release-only")
        }
        PlayOverlayReleaseDisplayModeConfig::ReleaseAndNotes => {
            tr!(text, "settings-play-overlay-display-release-and-notes")
        }
        PlayOverlayReleaseDisplayModeConfig::NotesOnly => {
            tr!(text, "settings-play-overlay-display-notes-only")
        }
    }
}

pub(in crate::ui) fn obs_connection_status_label(
    kind: crate::obs::ObsConnectionStatusKind,
    text: Localizer,
) -> (String, egui::Color32) {
    match kind {
        crate::obs::ObsConnectionStatusKind::Disabled => {
            (tr!(text, "settings-obs-disabled"), egui::Color32::GRAY)
        }
        crate::obs::ObsConnectionStatusKind::Connecting => {
            (tr!(text, "common-connecting"), egui::Color32::from_rgb(120, 190, 255))
        }
        crate::obs::ObsConnectionStatusKind::WaitingForServer => {
            (tr!(text, "settings-obs-waiting"), egui::Color32::from_rgb(225, 185, 75))
        }
        crate::obs::ObsConnectionStatusKind::Connected => {
            (tr!(text, "common-connected"), egui::Color32::GREEN)
        }
        crate::obs::ObsConnectionStatusKind::Reconnecting => {
            (tr!(text, "settings-obs-reconnecting"), egui::Color32::YELLOW)
        }
        crate::obs::ObsConnectionStatusKind::AuthenticationFailed => {
            (tr!(text, "settings-obs-auth-failed"), egui::Color32::RED)
        }
        crate::obs::ObsConnectionStatusKind::ConfigurationError => {
            (tr!(text, "settings-obs-config-error"), egui::Color32::RED)
        }
    }
}

pub(in crate::ui) fn obs_recording_mode_label(mode: ObsRecordingMode, text: Localizer) -> String {
    match mode {
        ObsRecordingMode::KeepAll => tr!(text, "settings-obs-recording-keep-all"),
        ObsRecordingMode::OnScreenshot => tr!(text, "settings-obs-recording-screenshot"),
        ObsRecordingMode::OnReplay => tr!(text, "settings-obs-recording-replay"),
    }
}

pub(in crate::ui) fn obs_action_label(action: ObsActionConfig, text: Localizer) -> String {
    match action {
        ObsActionConfig::None => tr!(text, "settings-obs-action-none"),
        ObsActionConfig::StartRecord => tr!(text, "settings-obs-action-start"),
        ObsActionConfig::StopRecord => tr!(text, "settings-obs-action-stop"),
    }
}

pub(in crate::ui) fn obs_event_label(event: crate::obs::ObsEventKey, text: Localizer) -> String {
    match event {
        crate::obs::ObsEventKey::MusicSelect => tr!(text, "settings-obs-event-select"),
        crate::obs::ObsEventKey::Decide => tr!(text, "settings-obs-event-decide"),
        crate::obs::ObsEventKey::Play => tr!(text, "settings-obs-event-play"),
        crate::obs::ObsEventKey::PlayEnded => tr!(text, "settings-obs-event-play-ended"),
        crate::obs::ObsEventKey::Result => tr!(text, "settings-obs-event-result"),
        crate::obs::ObsEventKey::CourseResult => tr!(text, "settings-obs-event-course-result"),
    }
}
