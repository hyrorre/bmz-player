use crate::config::app_config::{AppConfig, AudioConfig, VideoConfig};
use crate::config::app_settings_registry::{
    AppSettingsChoices, AppSettingsEntryId, adjust_app_settings_value,
};
use crate::config::profile_config::ProfileConfig;
use crate::config::settings_registry::{
    SettingsEntryId, adjust_settings_value, format_settings_value,
};

/// Captures only the setting being edited, including its dependent fields.
#[derive(Debug, Clone)]
pub struct SettingsEditSession {
    baseline: crate::config::settings_registry::SettingsSnapshot,
}

#[derive(Debug, Clone)]
enum AppSettingsBaseline {
    Audio(AudioConfig),
    Video(VideoConfig),
}

/// `data/config.toml` の音声・映像設定を一項目ずつ編集するセッション。
#[derive(Debug, Clone)]
pub struct AppSettingsEditSession {
    pub entry_id: AppSettingsEntryId,
    baseline: AppSettingsBaseline,
    choices: AppSettingsChoices,
}

impl AppSettingsEditSession {
    pub fn capture(
        config: &AppConfig,
        entry_id: AppSettingsEntryId,
        choices: AppSettingsChoices,
    ) -> Self {
        let baseline = if entry_id.is_audio() {
            AppSettingsBaseline::Audio(config.audio.clone())
        } else {
            AppSettingsBaseline::Video(config.video.clone())
        };
        Self { entry_id, baseline, choices }
    }

    pub fn restore(&self, config: &mut AppConfig) {
        match &self.baseline {
            AppSettingsBaseline::Audio(value) => config.audio = value.clone(),
            AppSettingsBaseline::Video(value) => config.video = value.clone(),
        }
    }

    pub fn adjust(&self, config: &mut AppConfig, direction: i32) -> bool {
        adjust_app_settings_value(config, self.entry_id, &self.choices, direction)
    }
}

/// 選曲設定で現在編集中の保存先を表す。
#[derive(Debug, Clone)]
pub enum SelectSettingsEditSession {
    Profile(SettingsEditSession),
    App(AppSettingsEditSession),
}

impl SettingsEditSession {
    pub fn capture(profile: &ProfileConfig, entry_id: SettingsEntryId) -> Self {
        Self { baseline: entry_id.capture(profile) }
    }

    pub fn entry_id(&self) -> SettingsEntryId {
        self.baseline.entry_id()
    }

    pub fn restore(&self, profile: &mut ProfileConfig) {
        self.baseline.restore(profile);
    }

    pub fn preview_value(&self, profile: &ProfileConfig) -> String {
        format_settings_value(profile, self.entry_id())
    }
}

pub fn adjust_settings_draft(
    profile: &mut ProfileConfig,
    session: &SettingsEditSession,
    delta: i32,
) -> bool {
    adjust_settings_value(profile, session.entry_id(), delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::AppConfig;
    use crate::config::profile_config::{
        DifficultyTableLevelDisplay, DoubleOptionConfig, GaugeTypeConfig, KeyModeConversionConfig,
        ProfileConfig, ReplaySlotRule,
    };
    use crate::select_options::SessionMode;

    #[test]
    fn every_setting_cancel_restores_its_values_and_preserves_unrelated_edits() {
        let mut default = ProfileConfig::new_default("default", "Default", 0);
        // Direction editing materializes this optional section even when the
        // effective default is restored. Compare explicit sections on both sides.
        default
            .input
            .play
            .entry(bmz_core::lane::KeyMode::K8.play_map_key().to_string())
            .or_default();
        let mut adjusted = default.clone();
        for &entry in SettingsEntryId::ALL {
            adjust_settings_value(&mut adjusted, entry, 1);
        }
        for original in [default, adjusted] {
            for &entry in SettingsEntryId::ALL {
                for delta in [-10_000, -1, 0, 1, 10_000] {
                    let mut profile = original.clone();
                    let session = SettingsEditSession::capture(&profile, entry);
                    assert_eq!(session.entry_id(), entry);
                    let display = session.preview_value(&profile);
                    adjust_settings_draft(&mut profile, &session, delta);
                    profile.display_name = "Changed independently".to_string();
                    session.restore(&mut profile);
                    let mut expected = original.clone();
                    expected.display_name.clone_from(&profile.display_name);
                    assert_eq!(
                        serde_json::to_value(&profile).unwrap(),
                        serde_json::to_value(&expected).unwrap(),
                        "cancel {entry:?}, delta={delta}",
                    );
                    assert_eq!(session.preview_value(&profile), display, "{entry:?}");
                }
            }
        }
    }

    #[test]
    fn edit_session_restore_reverts_volume() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let session = SettingsEditSession::capture(&profile, SettingsEntryId::MasterVolume);
        profile.audio_mix.master_volume = 20;
        session.restore(&mut profile);
        assert_eq!(profile.audio_mix.master_volume, 50);

        let normalize_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::NormalizeChartVolume);
        profile.audio_mix.normalize_chart_volume = false;
        normalize_session.restore(&mut profile);
        assert!(profile.audio_mix.normalize_chart_volume);

        let system_bgm_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::NormalizeSystemBgmVolume);
        profile.audio_mix.normalize_system_bgm_volume = false;
        system_bgm_session.restore(&mut profile);
        assert!(profile.audio_mix.normalize_system_bgm_volume);
    }

    #[test]
    fn app_edit_session_restores_the_edited_config_section() {
        let mut config = AppConfig::default();
        let audio = AppSettingsEditSession::capture(
            &config,
            AppSettingsEntryId::AudioBufferSize,
            AppSettingsChoices::None,
        );
        config.audio.buffer_size = 64;
        audio.restore(&mut config);
        assert_eq!(config.audio.buffer_size, 256);

        let video = AppSettingsEditSession::capture(
            &config,
            AppSettingsEntryId::VideoTargetFps,
            AppSettingsChoices::None,
        );
        config.video.target_fps = 0;
        video.restore(&mut config);
        assert_eq!(config.video.target_fps, 240);
    }

    #[test]
    fn edit_session_restore_reverts_gauge() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let session = SettingsEditSession::capture(&profile, SettingsEntryId::Gauge);
        profile.play.gauge = GaugeTypeConfig::Hazard;
        session.restore(&mut profile);
        assert_eq!(profile.play.gauge, GaugeTypeConfig::Normal);
    }

    #[test]
    fn edit_session_restore_reverts_input_and_replay_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let table_level_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::DifficultyTableLevelDisplay);
        profile.select.difficulty_table_level_display = DifficultyTableLevelDisplay::Chart;
        table_level_session.restore(&mut profile);
        assert_eq!(
            profile.select.difficulty_table_level_display,
            DifficultyTableLevelDisplay::Table
        );

        let analog_2p_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::AnalogScratch2P);
        profile.input.gamepad2.analog_scratch = false;
        analog_2p_session.restore(&mut profile);
        assert!(profile.input.gamepad2.analog_scratch);

        let controller_bounce_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::ControllerReleaseBounceMs);
        profile.input.controller_release_bounce_ms = 12;
        controller_bounce_session.restore(&mut profile);
        assert_eq!(profile.input.controller_release_bounce_ms, 0);

        let replay_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::ReplaySlot2Rule);
        profile.replay.slot_rules[1] = ReplaySlotRule::ClearUpdate;
        replay_session.restore(&mut profile);
        assert_eq!(profile.replay.slot_rules[1], ReplaySlotRule::ScoreUpdate);
    }

    #[test]
    fn edit_session_restore_reverts_new_dependent_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        let session_mode = SettingsEditSession::capture(&profile, SettingsEntryId::SessionMode);
        assert!(adjust_settings_draft(&mut profile, &session_mode, 1));
        assert!(adjust_settings_draft(&mut profile, &session_mode, 1));
        assert_eq!(profile.play.session_mode, Some(SessionMode::Autoplay));
        assert!(profile.play.auto_play);
        session_mode.restore(&mut profile);
        assert_eq!(profile.play.session_mode, Some(SessionMode::Normal));
        assert!(!profile.play.auto_play);

        profile.play.double_option = DoubleOptionConfig::Battle;
        let conversion = SettingsEditSession::capture(&profile, SettingsEntryId::KeyModeConversion);
        assert!(adjust_settings_draft(&mut profile, &conversion, 1));
        assert_eq!(profile.play.double_option, DoubleOptionConfig::Off);
        conversion.restore(&mut profile);
        assert_eq!(profile.play.key_mode_conversion, KeyModeConversionConfig::Off);
        assert_eq!(profile.play.double_option, DoubleOptionConfig::Battle);

        let assist = SettingsEditSession::capture(&profile, SettingsEntryId::AssistScrollRate);
        assert!(adjust_settings_draft(&mut profile, &assist, 5));
        assist.restore(&mut profile);
        assert!((profile.play.assist.scroll_rate - 0.5).abs() < f64::EPSILON);

        let note_retention = SettingsEditSession::capture(&profile, SettingsEntryId::NoteRetention);
        assert!(adjust_settings_draft(&mut profile, &note_retention, 1));
        assert!(profile.play.note_retention);
        note_retention.restore(&mut profile);
        assert!(!profile.play.note_retention);

        let duration =
            SettingsEditSession::capture(&profile, SettingsEntryId::NoteDisplayDurationMs);
        assert!(adjust_settings_draft(&mut profile, &duration, 10));
        assert_eq!(profile.lane.target_green_number, 306);
        duration.restore(&mut profile);
        assert_eq!(profile.lane.target_green_number, 300);

        let language = SettingsEditSession::capture(&profile, SettingsEntryId::Language);
        assert!(adjust_settings_draft(&mut profile, &language, 1));
        language.restore(&mut profile);
        assert_eq!(profile.ui.language, "ja");
    }

    #[test]
    fn random_select_settings_toggle_independently_and_cancel() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let entries = [
            SettingsEntryId::SelectRandomSelect,
            SettingsEntryId::SelectRandomNoPlay,
            SettingsEntryId::SelectRandomFailed,
            SettingsEntryId::SelectRandomNotEasy,
            SettingsEntryId::SelectRandomNotClear,
            SettingsEntryId::SelectRandomNotHard,
            SettingsEntryId::SelectRandomNotExHard,
            SettingsEntryId::SelectRandomNotFullCombo,
        ];
        for (index, entry) in entries.into_iter().enumerate() {
            assert!(SettingsEntryId::SELECT_ENTRIES.contains(&entry));
            let session = SettingsEditSession::capture(&profile, entry);
            assert!(!adjust_settings_draft(&mut profile, &session, 0));
            assert!(adjust_settings_draft(&mut profile, &session, 1));
            let mut expected = [false; 8];
            expected[index] = true;
            assert_eq!(profile.select.random_select_flags(), expected);
            session.restore(&mut profile);
            assert_eq!(profile.select.random_select_flags(), [false; 8]);
        }
    }
}
