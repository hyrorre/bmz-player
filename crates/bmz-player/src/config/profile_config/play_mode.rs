use super::*;

pub const PLAY_MODE_CONFIG_MODES: [KeyMode; 8] = [
    KeyMode::K4,
    KeyMode::K5,
    KeyMode::K6,
    KeyMode::K7,
    KeyMode::K8,
    KeyMode::K9,
    KeyMode::K10,
    KeyMode::K14,
];

impl ProfileConfig {
    /// Returns the effective settings for `key_mode`. The active mode is read
    /// from the legacy-compatible editable mirror so unsaved UI changes are
    /// immediately visible to play/session construction.
    pub fn play_mode_config(&self, key_mode: KeyMode) -> PlayModeConfig {
        if key_mode != self.active_play_mode && self.cli_play.is_some() {
            let mut effective = self.clone();
            effective.activate_play_mode(key_mode);
            return effective.editable_play_mode_config();
        }
        if key_mode == self.active_play_mode {
            return self.editable_play_mode_config();
        }
        self.play_mode
            .get(key_mode.play_map_key())
            .cloned()
            .unwrap_or_else(|| self.editable_play_mode_config())
    }

    /// Stores the editable mirror and replaces it with `key_mode` settings.
    pub fn activate_play_mode(&mut self, key_mode: KeyMode) {
        if key_mode == self.active_play_mode {
            return;
        }
        let cli = self.clear_cli_play();
        self.sync_active_play_mode();
        let next = self
            .play_mode
            .get(key_mode.play_map_key())
            .cloned()
            .unwrap_or_else(|| self.editable_play_mode_config());
        self.apply_editable_play_mode_config(&next);
        self.active_play_mode = key_mode;
        if let Some(cli) = cli {
            self.set_cli_play(cli);
        }
    }

    /// Copies the editable legacy-compatible fields into the persistent map.
    pub fn sync_active_play_mode(&mut self) {
        let cli = self.clear_cli_play();
        self.play_mode.insert(
            self.active_play_mode.play_map_key().to_string(),
            self.editable_play_mode_config(),
        );
        if let Some(cli) = cli {
            self.set_cli_play(cli);
        }
    }

    /// Migrates old profiles by copying their single set of values to every
    /// supported key mode, then restores the K7 editable mirror.
    pub fn normalize_play_mode_configs(&mut self) {
        let cli = self.clear_cli_play();
        self.lane.normal_hispeed_level =
            crate::config::play::normalize_normal_hispeed_level(self.lane.normal_hispeed_level);
        self.lane.constant_fade_ms = self.lane.constant_fade_ms.clamp(
            crate::config::play::CONSTANT_FADE_MIN_MS,
            crate::config::play::CONSTANT_FADE_MAX_MS,
        );
        for config in self.play_mode.values_mut() {
            normalize_play_mode_config(config);
        }
        let legacy = self.editable_play_mode_config();
        for key_mode in PLAY_MODE_CONFIG_MODES {
            self.play_mode
                .entry(key_mode.play_map_key().to_string())
                .or_insert_with(|| legacy.clone());
        }
        self.active_play_mode = KeyMode::K7;
        let mode7 = self.play_mode.get(KeyMode::K7.play_map_key()).cloned().unwrap_or(legacy);
        self.apply_editable_play_mode_config(&mode7);
        if let Some(cli) = cli {
            self.set_cli_play(cli);
        }
    }

    fn editable_play_mode_config(&self) -> PlayModeConfig {
        PlayModeConfig {
            hispeed: self.lane.hispeed,
            base_hispeed: self.lane.base_hispeed,
            floating_policy: self.lane.floating_policy,
            normal_hispeed_level: self.lane.normal_hispeed_level,
            hs_fix: self.play.hs_fix,
            lane_effect: self.play.lane_effect,
            sudden: self.lane.sudden,
            lift: self.lane.lift,
            lift_enabled: self.lane.lift_enabled,
            hispeed_auto_adjust: self.lane.hispeed_auto_adjust,
            hidden: self.lane.hidden,
            target_green_number: self.lane.target_green_number,
            constant_enabled: self.lane.constant_enabled,
            constant_fade_ms: self.lane.constant_fade_ms,
            visual_offset_us: self.judge.visual_offset_us,
        }
    }

    fn apply_editable_play_mode_config(&mut self, config: &PlayModeConfig) {
        self.lane.hispeed = config.hispeed;
        self.lane.base_hispeed = config.base_hispeed;
        self.lane.floating_policy = config.floating_policy;
        self.lane.normal_hispeed_level = config.normal_hispeed_level;
        self.play.hs_fix = config.hs_fix;
        self.play.lane_effect = config.lane_effect;
        self.lane.sudden = config.sudden;
        self.lane.lift = config.lift;
        self.lane.lift_enabled = config.lift_enabled;
        self.lane.hispeed_auto_adjust = config.hispeed_auto_adjust;
        self.lane.hidden = config.hidden;
        self.lane.target_green_number = config.target_green_number;
        self.lane.constant_enabled = config.constant_enabled;
        self.lane.constant_fade_ms = config.constant_fade_ms;
        self.judge.visual_offset_us = config.visual_offset_us;
    }
}

fn normalize_play_mode_config(config: &mut PlayModeConfig) {
    config.normal_hispeed_level =
        crate::config::play::normalize_normal_hispeed_level(config.normal_hispeed_level);
    config.constant_fade_ms = config.constant_fade_ms.clamp(
        crate::config::play::CONSTANT_FADE_MIN_MS,
        crate::config::play::CONSTANT_FADE_MAX_MS,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_play_settings_are_copied_to_every_key_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hispeed = 3.25;
        profile.lane.base_hispeed = BaseHispeedConfig::Normal;
        profile.lane.floating_policy = FloatingPolicyConfig::Locked;
        profile.play.hs_fix = HsFixConfig::MainBpm;
        profile.play.lane_effect = LaneEffectConfig::HiddenSudden;
        profile.lane.sudden = 420;
        profile.lane.hidden = 120;
        profile.lane.lift = 180;
        profile.lane.lift_enabled = true;
        profile.lane.hispeed_auto_adjust = true;
        profile.lane.target_green_number = 275;
        profile.judge.visual_offset_us = -7_000;

        profile.normalize_play_mode_configs();

        assert_eq!(profile.play_mode.len(), PLAY_MODE_CONFIG_MODES.len());
        let expected = profile.play_mode_config(KeyMode::K7);
        for key_mode in PLAY_MODE_CONFIG_MODES {
            assert_eq!(profile.play_mode_config(key_mode), expected, "{}", key_mode.as_str());
        }
    }

    #[test]
    fn switching_key_modes_preserves_independent_play_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.normalize_play_mode_configs();

        profile.lane.hispeed = 2.75;
        profile.lane.target_green_number = 290;
        profile.lane.sudden = 350;
        profile.play.lane_effect = LaneEffectConfig::Sudden;
        profile.play.hs_fix = HsFixConfig::StartBpm;
        profile.judge.visual_offset_us = 4_000;

        profile.activate_play_mode(KeyMode::K14);
        profile.lane.hispeed = 4.25;
        profile.lane.target_green_number = 240;
        profile.lane.sudden = 610;
        profile.lane.hidden = 210;
        profile.lane.lift = 90;
        profile.lane.lift_enabled = true;
        profile.play.lane_effect = LaneEffectConfig::HiddenSudden;
        profile.play.hs_fix = HsFixConfig::MaxBpm;
        profile.judge.visual_offset_us = -11_000;

        profile.activate_play_mode(KeyMode::K7);
        assert_eq!(profile.lane.hispeed, 2.75);
        assert_eq!(profile.lane.target_green_number, 290);
        assert_eq!(profile.lane.sudden, 350);
        assert_eq!(profile.play.lane_effect, LaneEffectConfig::Sudden);
        assert_eq!(profile.play.hs_fix, HsFixConfig::StartBpm);
        assert_eq!(profile.judge.visual_offset_us, 4_000);

        profile.activate_play_mode(KeyMode::K14);
        assert_eq!(profile.lane.hispeed, 4.25);
        assert_eq!(profile.lane.target_green_number, 240);
        assert_eq!(profile.lane.sudden, 610);
        assert_eq!(profile.lane.hidden, 210);
        assert_eq!(profile.lane.lift, 90);
        assert!(profile.lane.lift_enabled);
        assert_eq!(profile.play.lane_effect, LaneEffectConfig::HiddenSudden);
        assert_eq!(profile.play.hs_fix, HsFixConfig::MaxBpm);
        assert_eq!(profile.judge.visual_offset_us, -11_000);
    }

    #[test]
    fn legacy_note_display_duration_is_ignored_and_not_serialized() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let serialized = toml::to_string(&profile).expect("serialize profile");
        let legacy = serialized.replace(
            "floating_target_green = 300",
            "floating_target_green = 300\nnote_display_duration_ms = 497",
        );
        assert_ne!(legacy, serialized, "green number must be present in serialized profile");

        let loaded: ProfileConfig = toml::from_str(&legacy).expect("load legacy duration field");

        assert_eq!(loaded.lane.target_green_number, 300);
        for key_mode in PLAY_MODE_CONFIG_MODES {
            assert_eq!(loaded.play_mode_config(key_mode).target_green_number, 300);
        }
        assert!(
            !toml::to_string(&loaded)
                .expect("serialize migrated profile")
                .contains("note_display_duration_ms")
        );
    }
}
