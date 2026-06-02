use bmz_core::lane::KeyMode;

use crate::config::key_config::{KeyBindingTarget, format_play_binding, snapshot_play_bindings};
use crate::config::profile_config::ProfileConfig;

/// キー割り当ての待ち受け状態。
#[derive(Debug, Clone)]
pub struct KeyConfigEditSession {
    pub key_mode: KeyMode,
    pub target: KeyBindingTarget,
    baseline_bindings: Vec<crate::config::profile_config::BindingConfigEntry>,
    pub listening: bool,
}

impl KeyConfigEditSession {
    pub fn begin(key_mode: KeyMode, target: KeyBindingTarget, profile: &ProfileConfig) -> Self {
        Self {
            key_mode,
            target,
            baseline_bindings: snapshot_play_bindings(&profile.input, key_mode),
            listening: true,
        }
    }

    pub fn cancel(&self, profile: &mut ProfileConfig) {
        crate::config::key_config::restore_play_bindings(
            &mut profile.input,
            self.key_mode,
            self.baseline_bindings.clone(),
        );
    }

    pub fn preview_value(&self, profile: &ProfileConfig) -> String {
        if self.listening {
            self.target.slot().listen_hint().to_string()
        } else {
            format_play_binding(profile, self.key_mode, self.target)
        }
    }
}
