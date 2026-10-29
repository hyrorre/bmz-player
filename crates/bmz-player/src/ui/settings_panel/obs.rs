use super::*;
use crate::config::app_config::{ObsRule, ObsRuleAction, ObsTrigger};

pub(in crate::ui) fn build_obs_settings_section(
    ui: &mut egui::Ui,
    config: &mut AppConfig,
    state: &mut ObsScenePickerState,
    connection_status: &crate::obs::ObsConnectionStatus,
    text: Localizer,
) -> bool {
    state.poll(text, &config.obs);
    let mut enabled_changed = false;
    SettingsSection::new(SettingsPage::Integration, "OBS WebSocket")
        .scope(tr!(text, "settings-scope-app"))
        .id_salt("settings_obs")
        .show(ui, |ui| {
            ui.label(tr!(text, "settings-obs-deferred-help"));
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
                    .add_enabled(
                        !state.busy,
                        egui::Button::new(tr!(text, "settings-obs-load-scenes")),
                    )
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
            build_obs_command_editor(ui, &mut config.obs, state, text);
        });
    enabled_changed
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

fn trigger_label(trigger: ObsTrigger, text: Localizer) -> String {
    use crate::obs::ObsEventKey;
    let (screen, end) = match trigger {
        ObsTrigger::MainStart => (tr!(text, "settings-obs-main"), false),
        ObsTrigger::MainEnd => (tr!(text, "settings-obs-main"), true),
        ObsTrigger::MusicSelectStart => (obs_event_label(ObsEventKey::MusicSelect, text), false),
        ObsTrigger::MusicSelectEnd => (obs_event_label(ObsEventKey::MusicSelect, text), true),
        ObsTrigger::DecideStart => (obs_event_label(ObsEventKey::Decide, text), false),
        ObsTrigger::DecideEnd => (obs_event_label(ObsEventKey::Decide, text), true),
        ObsTrigger::PlayStart => (obs_event_label(ObsEventKey::Play, text), false),
        ObsTrigger::PlayEnd => (obs_event_label(ObsEventKey::Play, text), true),
        ObsTrigger::ResultStart => (obs_event_label(ObsEventKey::Result, text), false),
        ObsTrigger::ResultEnd => (obs_event_label(ObsEventKey::Result, text), true),
        ObsTrigger::CourseResultStart => (obs_event_label(ObsEventKey::CourseResult, text), false),
        ObsTrigger::CourseResultEnd => (obs_event_label(ObsEventKey::CourseResult, text), true),
    };
    tr!(text, "settings-obs-trigger", "screen" => screen, "edge" => if end {tr!(text, "settings-obs-end")} else {tr!(text, "settings-obs-start")})
}

fn action_kind(action: &ObsRuleAction) -> usize {
    match action {
        ObsRuleAction::SwitchScene { .. } => 0,
        ObsRuleAction::ShowSource { .. } => 1,
        ObsRuleAction::HideSource { .. } => 2,
        ObsRuleAction::StartRecord => 3,
        ObsRuleAction::StopRecord => 4,
    }
}

fn action_label(kind: usize, text: Localizer) -> String {
    text.text(
        [
            "settings-obs-switch-scene",
            "settings-obs-show-source",
            "settings-obs-hide-source",
            "settings-obs-action-start",
            "settings-obs-action-stop",
        ][kind],
    )
}

fn name_picker(ui: &mut egui::Ui, name: &mut String, candidates: &[String], text: Localizer) {
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(name).desired_width(150.0));
        egui::ComboBox::from_id_salt("candidates")
            .width(60.0)
            .selected_text(tr!(text, "settings-obs-choose"))
            .show_ui(ui, |ui| {
                for candidate in candidates {
                    ui.selectable_value(name, candidate.clone(), candidate);
                }
            });
    });
}

/// Returns true once the user confirms removing every command.
fn confirm_clear_commands(
    ctx: &egui::Context,
    state: &mut ObsScenePickerState,
    text: Localizer,
) -> bool {
    let mut confirmed = false;
    let mut close = false;
    let response = egui::Modal::new(egui::Id::new("obs_clear_commands_confirm")).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.heading(tr!(text, "settings-obs-clear-commands-confirm-title"));
        ui.label(tr!(text, "settings-obs-clear-commands-confirm-body"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tr!(text, "settings-obs-clear-commands")).clicked() {
                confirmed = true;
            }
            if ui.button(tr!(text, "common-cancel")).clicked() {
                close = true;
            }
        });
    });
    if confirmed || close || response.should_close() {
        state.confirm_clear_commands = false;
    }
    confirmed
}

fn build_obs_command_editor(
    ui: &mut egui::Ui,
    config: &mut crate::config::app_config::ObsConfig,
    state: &mut ObsScenePickerState,
    text: Localizer,
) {
    ui.strong(tr!(text, "settings-obs-commands"));
    ui.label(tr!(text, "settings-obs-command-help"));
    // Viewing never migrates settings. The first edit materializes the legacy list.
    let mut rules = config.effective_rules();
    let before = rules.clone();
    let mut edited = false;
    ui.horizontal(|ui| {
        if ui.add_enabled(rules.len() < 512, egui::Button::new(tr!(text, "common-add"))).clicked() {
            rules.push(ObsRule {
                trigger: ObsTrigger::PlayStart,
                action: ObsRuleAction::SwitchScene { scene_name: String::new() },
            });
            edited = true;
        }
        if ui
            .add_enabled(
                !rules.is_empty(),
                egui::Button::new(tr!(text, "settings-obs-clear-commands")),
            )
            .clicked()
        {
            state.confirm_clear_commands = true;
        }
    });
    if state.confirm_clear_commands && confirm_clear_commands(ui.ctx(), state, text) {
        rules.clear();
        edited = true;
    }
    let count = rules.len();
    let mut operation = None;
    for (index, rule) in rules.iter_mut().enumerate() {
        ui.push_id(("obs_rule", index), |ui| {
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("trigger")
                        .selected_text(trigger_label(rule.trigger, text))
                        .show_ui(ui, |ui| {
                            for trigger in ObsTrigger::ALL {
                                ui.selectable_value(
                                    &mut rule.trigger,
                                    trigger,
                                    trigger_label(trigger, text),
                                );
                            }
                        });
                    let mut kind = action_kind(&rule.action);
                    egui::ComboBox::from_id_salt("action")
                        .selected_text(action_label(kind, text))
                        .show_ui(ui, |ui| {
                            for candidate in 0..5 {
                                ui.selectable_value(
                                    &mut kind,
                                    candidate,
                                    action_label(candidate, text),
                                );
                            }
                        });
                    if kind != action_kind(&rule.action) {
                        let (scene_name, source_name) = match &rule.action {
                            ObsRuleAction::SwitchScene { scene_name } => {
                                (scene_name.clone(), String::new())
                            }
                            ObsRuleAction::ShowSource { scene_name, source_name }
                            | ObsRuleAction::HideSource { scene_name, source_name } => {
                                (scene_name.clone(), source_name.clone())
                            }
                            _ => (String::new(), String::new()),
                        };
                        rule.action = match kind {
                            0 => ObsRuleAction::SwitchScene { scene_name },
                            1 => ObsRuleAction::ShowSource { scene_name, source_name },
                            2 => ObsRuleAction::HideSource { scene_name, source_name },
                            3 => ObsRuleAction::StartRecord,
                            _ => ObsRuleAction::StopRecord,
                        };
                    }
                    if ui
                        .add_enabled(index > 0, egui::Button::new(tr!(text, "common-move-up")))
                        .clicked()
                    {
                        operation = Some(SettingsListAction::MoveUp(index));
                    }
                    if ui
                        .add_enabled(
                            index + 1 < count,
                            egui::Button::new(tr!(text, "common-move-down")),
                        )
                        .clicked()
                    {
                        operation = Some(SettingsListAction::MoveDown(index));
                    }
                    if ui.button(tr!(text, "settings-obs-remove-command")).clicked() {
                        operation = Some(SettingsListAction::Remove(index));
                    }
                });
                match &mut rule.action {
                    ObsRuleAction::SwitchScene { scene_name } => {
                        ui.label(tr!(text, "settings-obs-scene"));
                        ui.push_id("scene", |ui| name_picker(ui, scene_name, &state.scenes, text));
                    }
                    ObsRuleAction::ShowSource { scene_name, source_name }
                    | ObsRuleAction::HideSource { scene_name, source_name } => {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr!(text, "settings-obs-scene"));
                            ui.push_id("scene", |ui| {
                                name_picker(ui, scene_name, &state.scenes, text)
                            });
                            ui.label(tr!(text, "settings-obs-source"));
                            let candidates = state
                                .sources
                                .get(scene_name)
                                .map(Vec::as_slice)
                                .unwrap_or_default();
                            ui.push_id("source", |ui| {
                                name_picker(ui, source_name, candidates, text)
                            });
                        });
                    }
                    _ => {}
                }
                if !rule.action.is_valid() {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        tr!(text, "settings-obs-command-incomplete"),
                    );
                }
            });
        });
    }
    if let Some(operation) = operation {
        match operation {
            SettingsListAction::MoveUp(index) => rules.swap(index, index - 1),
            SettingsListAction::MoveDown(index) => rules.swap(index, index + 1),
            SettingsListAction::Remove(index) => {
                rules.remove(index);
            }
            _ => {}
        }
        edited = true;
    }
    if edited || rules != before {
        config.rules = Some(rules);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::AppLocale;

    #[test]
    fn viewing_disconnected_commands_preserves_legacy_and_missing_names() {
        let mut config = crate::config::app_config::ObsConfig::default();
        config.scenes.insert("PLAY".into(), "Missing scene".into());
        let ctx = egui::Context::default();
        let mut picker = ObsScenePickerState::default();
        let before = config.clone();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            build_obs_command_editor(ui, &mut config, &mut picker, Localizer::new(AppLocale::En));
        });
        assert_eq!(config, before);
        config.rules = Some(vec![ObsRule {
            trigger: ObsTrigger::MainEnd,
            action: ObsRuleAction::HideSource {
                scene_name: "Removed".into(),
                source_name: "Removed source".into(),
            },
        }]);
        let before = config.clone();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            build_obs_command_editor(ui, &mut config, &mut picker, Localizer::new(AppLocale::En));
        });
        assert_eq!(config, before);
    }

    #[test]
    fn clear_commands_waits_for_confirmation() {
        let mut config = crate::config::app_config::ObsConfig {
            rules: Some(vec![ObsRule {
                trigger: ObsTrigger::PlayStart,
                action: ObsRuleAction::StartRecord,
            }]),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut picker = ObsScenePickerState { confirm_clear_commands: true, ..Default::default() };
        let before = config.clone();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            build_obs_command_editor(ui, &mut config, &mut picker, Localizer::new(AppLocale::En));
        });
        assert_eq!(config, before);
        assert!(picker.confirm_clear_commands);
    }

    #[test]
    fn obs_editor_labels_resolve_for_every_supported_locale() {
        for locale in AppLocale::SUPPORTED {
            let text = Localizer::new(locale);
            for trigger in ObsTrigger::ALL {
                assert!(!trigger_label(trigger, text).contains("settings-obs-"));
            }
            for kind in 0..5 {
                assert!(!action_label(kind, text).contains("settings-obs-"));
            }
            for key in [
                "settings-obs-clear-commands-confirm-title",
                "settings-obs-clear-commands-confirm-body",
            ] {
                assert!(!text.text(key).contains("settings-obs-"));
            }
        }
    }
}
