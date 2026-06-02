use crate::config::key_config::{format_play_keyboard_binding, snapshot_play_bindings};
use crate::config::profile_config::{LaneConfig, ProfileConfig};

/// キー割り当ての待ち受け状態。
#[derive(Debug, Clone)]
pub struct KeyConfigEditSession {
    pub target: LaneConfig,
    baseline_bindings: Vec<crate::config::profile_config::BindingConfigEntry>,
    pub listening: bool,
}

impl KeyConfigEditSession {
    pub fn begin(target: LaneConfig, profile: &ProfileConfig) -> Self {
        Self { target, baseline_bindings: snapshot_play_bindings(&profile.input), listening: true }
    }

    pub fn cancel(&self, profile: &mut ProfileConfig) {
        crate::config::key_config::restore_play_bindings(
            &mut profile.input,
            self.baseline_bindings.clone(),
        );
    }

    pub fn preview_value(&self, profile: &ProfileConfig) -> String {
        if self.listening {
            "PRESS KEY".to_string()
        } else {
            format_play_keyboard_binding(profile, self.target)
        }
    }
}
