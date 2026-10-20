use super::*;

/// Captured before editing or restoring a profile, independently of the UI.
pub(super) struct ProfileSettingsSnapshot {
    locale: crate::i18n::AppLocale,
    random_select: [bool; 8],
    play: PlayDefaultsConfig,
    lane: LaneViewConfig,
    input: ProfileInputConfig,
    show_fps: bool,
}

impl ProfileSettingsSnapshot {
    pub(super) fn capture(profile: &ProfileConfig) -> Self {
        Self {
            locale: profile.ui.locale(),
            random_select: profile.select.random_select_flags(),
            play: profile.play.clone(),
            lane: profile.lane.clone(),
            input: profile.input.clone(),
            show_fps: profile.ui.show_fps,
        }
    }

    fn select_play_options(
        &self,
        current: CurrentPlayOptions,
        after: &PlayDefaultsConfig,
        select_entry: Option<SettingsEntryId>,
    ) -> CurrentPlayOptions {
        match select_entry {
            Some(entry) if entry.is_play_setting() => select_play_options_from_profile(after),
            Some(_) => current,
            None => merge_changed_select_play_options_from_profile(current, &self.play, after),
        }
    }
}

impl WinitApp {
    pub(super) fn apply_profile_settings_changes(
        &mut self,
        before: &ProfileSettingsSnapshot,
        select_entry: Option<SettingsEntryId>,
    ) {
        let locale = self.boot.profile_config.ui.locale();
        self.renderer.set_default_font_coverage(locale.font_coverage());
        let locale_changed = locale != before.locale;
        if locale_changed {
            self.select.search.clear_message();
        }
        if locale_changed
            || before.random_select != self.boot.profile_config.select.random_select_flags()
        {
            if self.current_scene_kind() == AppSceneKind::Select {
                self.reload_select_items();
            } else {
                self.jobs.pending_locale_refresh = true;
            }
        }
        // Select entry editing commits the profile's full play option set; egui only
        // merges changed fields so unrelated temporary panel choices survive.
        let next = before.select_play_options(
            self.current_select_play_options(),
            &self.boot.profile_config.play,
            select_entry,
        );
        self.set_select_play_options(next);
        if select_entry.is_some_and(SettingsEntryId::is_play_setting) {
            self.invalidate_play_preload();
            self.play.play_media_cache = None;
        }
        self.sync_changed_select_score_context(SelectScoreContext::from_play(&before.play));
        if select_entry.is_none()
            && (before.play.key_mode_conversion
                != self.boot.profile_config.play.key_mode_conversion
                || before.play.seven_to_nine_pattern
                    != self.boot.profile_config.play.seven_to_nine_pattern
                || before.play.seven_to_nine_type
                    != self.boot.profile_config.play.seven_to_nine_type
                || before.play.seven_to_nine_rule_mode
                    != self.boot.profile_config.play.seven_to_nine_rule_mode)
        {
            self.invalidate_play_preload();
            self.play.play_media_cache = None;
        }
        self.sync_changed_gamepad_analog_config_from_profile(&before.input);
        self.sync_changed_select_input_mode_from_profile(&before.input);
        if profile_lane_settings_changed(&before.lane, &self.boot.profile_config.lane)
            || before.play.lane_effect != self.boot.profile_config.play.lane_effect
        {
            self.sync_active_play_lane_settings_from_profile(&before.lane, before.play.lane_effect);
        }
        if select_entry == Some(SettingsEntryId::HispeedMode) {
            self.reload_select_items();
        }
        if before.show_fps != self.boot.profile_config.ui.show_fps
            && let Some(egui) = self.ui.egui.as_mut()
        {
            egui.set_show_fps(self.boot.profile_config.ui.show_fps);
        }
        self.sync_realtime_profile_settings();
        self.sync_discord_presence_config();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_and_restore_update_gauge_with_each_uis_temporary_option_policy() {
        for entry in [None, Some(SettingsEntryId::Gauge)] {
            let mut profile = ProfileConfig::new_default("test", "Test", 0);
            profile.play.gauge = GaugeTypeConfig::Normal;
            let mut current = select_play_options_from_profile(&profile.play);
            current.arrange = ArrangeOption::Random;
            let undo = SettingsEditSession::capture(&profile, SettingsEntryId::Gauge);
            let before = ProfileSettingsSnapshot::capture(&profile);
            profile.play.gauge = GaugeTypeConfig::Hard;
            let changed = before.select_play_options(current, &profile.play, entry);
            assert_eq!(changed.gauge, GaugeTypeConfig::Hard);
            let expected_arrange = if entry.is_some() {
                select_play_options_from_profile(&profile.play).arrange
            } else {
                current.arrange
            };
            assert_eq!(changed.arrange, expected_arrange);
            let before_restore = ProfileSettingsSnapshot::capture(&profile);
            undo.restore(&mut profile);
            let restored = before_restore.select_play_options(changed, &profile.play, entry);
            assert_eq!(restored.gauge, GaugeTypeConfig::Normal);
            assert_eq!(restored.arrange, expected_arrange);
        }
    }

    #[test]
    fn unrelated_ui_edits_preserve_temporary_play_options() {
        let mut profile = ProfileConfig::new_default("test", "Test", 0);
        let mut current = select_play_options_from_profile(&profile.play);
        current.arrange = ArrangeOption::Random;
        let before = ProfileSettingsSnapshot::capture(&profile);
        profile.ui.show_fps = !profile.ui.show_fps;
        for entry in [None, Some(SettingsEntryId::ShowFps)] {
            assert_eq!(before.select_play_options(current, &profile.play, entry), current);
        }
    }
}
