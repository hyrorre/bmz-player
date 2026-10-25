use super::*;

pub(in crate::ui::profile_panel) fn build_profile_display_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let text = section.text;
    SettingsSection::new(SettingsPage::Play, tr!(text, "profile-display-title"))
        .scope(tr!(text, "settings-scope-profile"))
        .id_salt("profile_display")
        .show(ui, |ui| {
            let mut preset = profile.lane.hispeed_config();
            egui::ComboBox::new("profile_hispeed_mode", tr!(text, "profile-display-hispeed-mode"))
                .selected_text(hispeed_mode_label(preset))
                .show_ui(ui, |ui| {
                    for value in HispeedConfigPreset::ORDER {
                        ui.selectable_value(&mut preset, value, hispeed_mode_label(value));
                    }
                });
            profile.lane.set_hispeed_config(preset);
            if preset.supports_normal() {
                let normal_green = crate::config::play::normal_hispeed_green_number(
                    profile.lane.normal_hispeed_level,
                );
                ui.add(
                    egui::Slider::new(
                        &mut profile.lane.normal_hispeed_level,
                        crate::config::play::NORMAL_HISPEED_LEVEL_MIN
                            ..=crate::config::play::NORMAL_HISPEED_LEVEL_MAX,
                    )
                    .text(format!(
                        "{} ({normal_green})",
                        tr!(text, "profile-display-normal-hispeed")
                    )),
                );
            }
            if preset.supports_classic() {
                let hispeed_step =
                    normalize_classic_hispeed_step(profile.lane.classic_hispeed_step);
                ui.add(
                    egui::Slider::new(
                        &mut profile.lane.hispeed,
                        crate::config::play::HISPEED_MIN..=crate::config::play::HISPEED_MAX,
                    )
                    .step_by(hispeed_step as f64)
                    .text(tr!(text, "profile-display-hispeed")),
                );
                ui.add(
                    egui::Slider::new(
                        &mut profile.lane.classic_hispeed_step,
                        CLASSIC_HISPEED_STEP_MIN..=HISPEED_STEP_MAX,
                    )
                    .step_by(0.01)
                    .text(tr!(text, "profile-display-classic-hispeed-step")),
                );
            }
            if preset.supports_floating() {
                ui.add(
                    egui::Slider::new(
                        &mut profile.lane.floating_hispeed_step,
                        FLOATING_HISPEED_STEP_MIN..=HISPEED_STEP_MAX,
                    )
                    .step_by(0.05)
                    .text(tr!(text, "profile-display-floating-hispeed-step")),
                );
            }
            if preset.supports_classic() || preset.supports_floating() {
                ui.label(tr!(text, "profile-display-step-range"));
            }
            if preset.supports_floating() {
                ui.checkbox(
                    &mut profile.lane.hispeed_auto_adjust,
                    tr!(text, "profile-display-auto-adjust-hispeed"),
                );
                ui.add(
                    egui::Slider::new(
                        &mut profile.lane.target_green_number,
                        TARGET_GREEN_NUMBER_MIN..=TARGET_GREEN_NUMBER_MAX,
                    )
                    .text(tr!(text, "profile-display-green-number")),
                );
                let mut note_display_duration_ms =
                    crate::config::play::duration_ms_from_green_number(
                        profile.lane.target_green_number,
                    );
                let duration_changed = ui
                    .add(
                        egui::Slider::new(
                            &mut note_display_duration_ms,
                            crate::config::play::NOTE_DISPLAY_DURATION_MIN_MS
                                ..=crate::config::play::NOTE_DISPLAY_DURATION_MAX_MS,
                        )
                        .text(tr!(text, "profile-display-note-duration")),
                    )
                    .changed();
                if duration_changed {
                    profile.lane.target_green_number =
                        crate::config::play::green_number_from_duration_ms(
                            note_display_duration_ms,
                        )
                        .clamp(TARGET_GREEN_NUMBER_MIN, TARGET_GREEN_NUMBER_MAX);
                }
            }
            let mut sudden_enabled = profile.play.lane_effect.sudden_enabled();
            if ui
                .checkbox(&mut sudden_enabled, tr!(text, "profile-display-sudden-enabled"))
                .changed()
            {
                profile.play.lane_effect =
                    profile.play.lane_effect.with_sudden_enabled(sudden_enabled);
            }
            let sudden_max = crate::config::play::lane_unit_max_for_other(profile.lane.lift);
            lane_unit_slider_with_max(ui, &mut profile.lane.sudden, "SUDDEN+", sudden_max);
            let lift_max = crate::config::play::lane_unit_max_for_other(profile.lane.sudden);
            ui.checkbox(&mut profile.lane.lift_enabled, tr!(text, "profile-display-lift-enabled"));
            lane_unit_slider_with_max(ui, &mut profile.lane.lift, "LIFT", lift_max);
            let mut hidden_enabled = profile.play.lane_effect.hidden_enabled();
            if ui
                .checkbox(&mut hidden_enabled, tr!(text, "profile-display-hidden-enabled"))
                .changed()
            {
                profile.play.lane_effect =
                    profile.play.lane_effect.with_hidden_enabled(hidden_enabled);
            }
            lane_unit_slider(ui, &mut profile.lane.hidden, "HIDDEN");
            ui.checkbox(&mut profile.lane.constant_enabled, tr!(text, "profile-display-constant"));
            ui.add_enabled(
                profile.lane.constant_enabled,
                egui::Slider::new(
                    &mut profile.lane.constant_fade_ms,
                    crate::config::play::CONSTANT_FADE_MIN_MS
                        ..=crate::config::play::CONSTANT_FADE_MAX_MS,
                )
                .text(tr!(text, "profile-display-constant-fade")),
            );
        });
}
