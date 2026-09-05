use super::*;

pub(in crate::ui::profile_panel) fn build_profile_basic_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let unrestricted = section.unrestricted;
    let text = section.text;
    egui::CollapsingHeader::new(tr!(text, "profile-basic-title"))
        .id_salt("profile_basic")
        .default_open(true)
        .show(ui, |ui| {
            if !unrestricted {
                ui.disable();
            }
            ui.horizontal(|ui| {
                ui.label(tr!(text, "profile-display-name"));
                ui.text_edit_singleline(&mut profile.display_name);
            });
            ui.horizontal(|ui| {
                ui.label("ID");
                ui.monospace(&profile.id);
            });
        });
}

pub(in crate::ui::profile_panel) fn build_profile_volume_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let text = section.text;
    egui::CollapsingHeader::new(tr!(text, "profile-volume-title"))
        .id_salt("profile_volume")
        .default_open(true)
        .show(ui, |ui| {
            ui.checkbox(
                &mut profile.audio_mix.normalize_chart_volume,
                tr!(text, "profile-volume-normalize"),
            );
            ui.checkbox(
                &mut profile.audio_mix.normalize_system_bgm_volume,
                tr!(text, "profile-volume-normalize-system-bgm"),
            );
            volume_slider(
                ui,
                &mut profile.audio_mix.master_volume,
                &tr!(text, "profile-volume-master"),
            );
            volume_slider(
                ui,
                &mut profile.audio_mix.key_volume,
                &tr!(text, "profile-volume-keysound"),
            );
            ui.checkbox(
                &mut profile.audio_mix.auto_keysound,
                tr!(text, "profile-volume-keysound-auto"),
            );
            ui.add_enabled(
                profile.audio_mix.auto_keysound,
                egui::Checkbox::new(
                    &mut profile.audio_mix.auto_keysound_fallback,
                    tr!(text, "profile-volume-keysound-auto-fallback"),
                ),
            );
            ui.add_enabled(
                profile.audio_mix.auto_keysound,
                egui::Checkbox::new(
                    &mut profile.audio_mix.auto_keysound_mine,
                    tr!(text, "profile-volume-keysound-auto-mine"),
                ),
            );
            volume_slider(ui, &mut profile.audio_mix.bgm_volume, "BGM");
            volume_slider(
                ui,
                &mut profile.audio_mix.preview_volume,
                &tr!(text, "profile-volume-preview"),
            );
            volume_slider(
                ui,
                &mut profile.audio_mix.system_bgm_volume,
                &tr!(text, "profile-volume-system-bgm"),
            );
            volume_slider(
                ui,
                &mut profile.audio_mix.system_se_volume,
                &tr!(text, "profile-volume-system-se"),
            );
            ui.label(tr!(text, "profile-volume-help"));
        });
}

pub(in crate::ui::profile_panel) fn build_profile_judge_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let text = section.text;
    egui::CollapsingHeader::new(tr!(text, "profile-judge-title")).id_salt("profile_judge").show(
        ui,
        |ui| {
            offset_ms_slider(
                ui,
                &mut profile.judge.input_offset_us,
                &tr!(text, "profile-judge-input-offset"),
            );
            offset_ms_slider(
                ui,
                &mut profile.judge.visual_offset_us,
                &tr!(text, "profile-judge-visual-offset"),
            );
            ui.checkbox(
                &mut profile.judge.visual_offset_auto_adjust,
                tr!(text, "profile-judge-auto-adjust"),
            );
            egui::ComboBox::new("profile_judge_algorithm", tr!(text, "profile-judge-algorithm"))
                .selected_text(judge_algorithm_label(profile.judge.judge_algorithm))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut profile.judge.judge_algorithm,
                        JudgeAlgorithmConfig::Combo,
                        judge_algorithm_label(JudgeAlgorithmConfig::Combo),
                    );
                    ui.selectable_value(
                        &mut profile.judge.judge_algorithm,
                        JudgeAlgorithmConfig::Duration,
                        judge_algorithm_label(JudgeAlgorithmConfig::Duration),
                    );
                    ui.selectable_value(
                        &mut profile.judge.judge_algorithm,
                        JudgeAlgorithmConfig::Lowest,
                        judge_algorithm_label(JudgeAlgorithmConfig::Lowest),
                    );
                });
            egui::ComboBox::new("profile_fast_slow_scope", tr!(text, "profile-fast-slow-mode"))
                .selected_text(fast_slow_scope_label(text, profile.judge.fast_slow_display_scope))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut profile.judge.fast_slow_display_scope,
                        FastSlowDisplayScope::Auto,
                        fast_slow_scope_label(text, FastSlowDisplayScope::Auto),
                    );
                    ui.selectable_value(
                        &mut profile.judge.fast_slow_display_scope,
                        FastSlowDisplayScope::ThresholdMs,
                        fast_slow_scope_label(text, FastSlowDisplayScope::ThresholdMs),
                    );
                });
            if profile.judge.fast_slow_display_scope == FastSlowDisplayScope::ThresholdMs {
                ui.add(
                    egui::Slider::new(&mut profile.judge.fast_slow_display_threshold_ms, 0..=50)
                        .text(tr!(text, "profile-fast-slow-threshold")),
                );
                ui.label(tr!(text, "profile-fast-slow-threshold-help"));
            }
        },
    );
}

pub(in crate::ui::profile_panel) fn build_profile_input_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let text = section.text;
    egui::CollapsingHeader::new(tr!(text, "profile-input-title")).id_salt("profile_input").show(
        ui,
        |ui| {
            for (label, config) in [
                (tr!(text, "settings-input-controller-1p"), &mut profile.input.gamepad1),
                (tr!(text, "settings-input-controller-2p"), &mut profile.input.gamepad2),
            ] {
                ui.label(label);
                ui.checkbox(&mut config.analog_scratch, tr!(text, "profile-input-analog-scratch"));
                ui.add_enabled(
                    config.analog_scratch,
                    egui::Slider::new(&mut config.analog_scratch_sensitivity, 0.1..=5.0)
                        .text(tr!(text, "profile-input-analog-sensitivity")),
                );
                ui.add_enabled(
                    config.analog_scratch,
                    egui::Slider::new(&mut config.analog_scratch_threshold, 1..=1000)
                        .text(tr!(text, "profile-input-analog-stop-threshold")),
                );
                ui.separator();
            }
            ui.add(
                egui::Slider::new(
                    &mut profile.input.keyboard_release_bounce_ms,
                    0..=RELEASE_BOUNCE_MS_MAX,
                )
                .text(tr!(text, "profile-input-keyboard-release-bounce-ms")),
            );
            ui.add(
                egui::Slider::new(
                    &mut profile.input.controller_release_bounce_ms,
                    0..=RELEASE_BOUNCE_MS_MAX,
                )
                .text(tr!(text, "profile-input-controller-release-bounce-ms")),
            );
            ui.label(tr!(text, "profile-input-release-bounce-help"));
            ui.label(tr!(text, "profile-input-key-bindings-help"));
        },
    );
}

pub(in crate::ui::profile_panel) fn build_profile_replay_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let unrestricted = section.unrestricted;
    let text = section.text;
    egui::CollapsingHeader::new(tr!(text, "profile-replay-title")).id_salt("profile_replay").show(
        ui,
        |ui| {
            if !unrestricted {
                ui.disable();
            }
            ui.checkbox(&mut profile.replay.auto_save, tr!(text, "profile-replay-auto-save"));
            ui.checkbox(&mut profile.replay.compress, tr!(text, "profile-replay-compress"));
            for (index, rule) in profile.replay.slot_rules.iter_mut().enumerate() {
                egui::ComboBox::new(
                    ("profile_replay_slot", index),
                    tr!(text, "profile-replay-slot", "number" => index + 1),
                )
                .selected_text(replay_slot_rule_label(*rule))
                .show_ui(ui, |ui| {
                    for value in [
                        ReplaySlotRule::Disabled,
                        ReplaySlotRule::Always,
                        ReplaySlotRule::ScoreUpdate,
                        ReplaySlotRule::BpUpdate,
                        ReplaySlotRule::MaxComboUpdate,
                        ReplaySlotRule::ClearUpdate,
                    ] {
                        ui.selectable_value(rule, value, replay_slot_rule_label(value));
                    }
                });
            }
        },
    );
}

pub(in crate::ui::profile_panel) fn build_profile_system_sound_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let unrestricted = section.unrestricted;
    let text = section.text;
    egui::CollapsingHeader::new(tr!(text, "profile-system-sound-title"))
        .id_salt("profile_system_sound")
        .show(ui, |ui| {
            if !unrestricted {
                ui.disable();
            }
            system_sound_path_row(
                ui,
                text,
                &tr!(text, "profile-system-sound-bgm-root"),
                &mut profile.system_sound.bgm_dir,
            );
            system_sound_path_row(
                ui,
                text,
                &tr!(text, "profile-system-sound-se-root"),
                &mut profile.system_sound.se_dir,
            );
            system_sound_path_row(
                ui,
                text,
                &tr!(text, "profile-system-sound-fallback"),
                &mut profile.system_sound.default_sound_dir,
            );
            ui.label(tr!(text, "profile-system-sound-rescan-help"));
        });
}
