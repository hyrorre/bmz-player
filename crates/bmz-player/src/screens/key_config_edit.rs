use bmz_core::lane::KeyMode;

use crate::config::key_config::{KeyBindingTarget, format_play_binding, snapshot_play_mode_config};
use crate::config::profile_config::ProfileConfig;

/// キー割り当ての待ち受け状態。
#[derive(Debug, Clone)]
pub struct KeyConfigEditSession {
    pub key_mode: KeyMode,
    pub target: KeyBindingTarget,
    baseline_play_config: Option<crate::config::profile_config::PlayModeInputConfig>,
    baseline_ui_bindings: Vec<crate::config::profile_config::BindingConfigEntry>,
    pub listening: bool,
}

impl KeyConfigEditSession {
    pub fn begin(key_mode: KeyMode, target: KeyBindingTarget, profile: &ProfileConfig) -> Self {
        Self {
            key_mode,
            target,
            baseline_play_config: snapshot_play_mode_config(&profile.input, key_mode),
            baseline_ui_bindings: profile.input.ui.bindings.clone(),
            listening: true,
        }
    }

    pub fn cancel(&self, profile: &mut ProfileConfig) {
        crate::config::key_config::restore_play_mode_config(
            &mut profile.input,
            self.key_mode,
            self.baseline_play_config.clone(),
        );
        profile.input.ui.bindings = self.baseline_ui_bindings.clone();
    }

    pub fn preview_value(&self, profile: &ProfileConfig) -> String {
        if self.listening {
            self.target.slot().listen_hint().to_string()
        } else {
            format_play_binding(profile, self.key_mode, self.target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::key_config::KeyBindingSlot;
    use crate::config::profile_config::{HispeedDirectionConfig, LaneConfig};

    #[test]
    fn cancel_restores_eight_key_hispeed_config_with_bindings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(crate::config::play_input::set_eight_key_hispeed_direction(
            &mut profile.input,
            LaneConfig::Key1,
            HispeedDirectionConfig::Down,
        ));
        let target =
            KeyBindingTarget::Key { lane: LaneConfig::Key1, slot: KeyBindingSlot::KeyboardPrimary };
        let edit = KeyConfigEditSession::begin(KeyMode::K8, target, &profile);

        profile.input.play.get_mut("8k").unwrap().hispeed.clear();
        edit.cancel(&mut profile);

        assert_eq!(
            profile.input.play["8k"].hispeed.get(&LaneConfig::Key1),
            Some(&HispeedDirectionConfig::Down),
        );
    }
}
