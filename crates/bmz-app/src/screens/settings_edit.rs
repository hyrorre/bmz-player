use std::collections::HashSet;

use bmz_core::lane::KeyMode;

use crate::config::play_input::resolve_play_bindings;
use crate::config::profile_config::{
    InputActionConfig, LaneConfig, ProfileConfig, ProfileInputConfig,
};
use crate::config::settings_registry::{SettingsEntryId, adjust_settings_value};

/// 7KEY + スクラッチ向けの設定画面入力マッピング。
#[derive(Debug, Clone)]
pub struct SettingsBindings {
    confirm: HashSet<String>,
    back: HashSet<String>,
    increase: HashSet<String>,
    decrease: HashSet<String>,
}

impl SettingsBindings {
    pub fn from_profile(input: &ProfileInputConfig) -> Self {
        let mut confirm = HashSet::new();
        let mut back = HashSet::new();
        let mut increase = HashSet::new();
        let mut decrease = HashSet::new();

        if let Ok(play) = resolve_play_bindings(input, KeyMode::K7) {
            for entry in play {
                let Some(lane) = entry.lane else { continue };
                match lane {
                    LaneConfig::Key1 | LaneConfig::Key3 | LaneConfig::Key5 | LaneConfig::Key7 => {
                        confirm.insert(entry.control.clone());
                    }
                    LaneConfig::Key2 | LaneConfig::Key4 | LaneConfig::Key6 => {
                        back.insert(entry.control.clone());
                    }
                    LaneConfig::Scratch => {
                        classify_scratch_control(&entry.control, &mut increase, &mut decrease);
                    }
                    _ => {}
                }
            }
        }

        for entry in &input.ui.bindings {
            match entry.action {
                Some(InputActionConfig::SelectEnter) => {
                    confirm.insert(entry.control.clone());
                }
                Some(InputActionConfig::E2) => {
                    back.insert(entry.control.clone());
                }
                _ => {}
            }
        }

        for key in ["Enter", "Space", "ArrowRight"] {
            confirm.insert(key.to_string());
        }
        for key in ["ArrowLeft", "Escape"] {
            back.insert(key.to_string());
        }
        for key in ["ArrowDown", "DPadDown", "ScratchDown"] {
            increase.insert(key.to_string());
        }
        for key in ["ArrowUp", "DPadUp", "ScratchUp"] {
            decrease.insert(key.to_string());
        }
        for key in ["DPadRight", "Button1"] {
            confirm.insert(key.to_string());
        }
        for key in ["DPadLeft", "Select"] {
            back.insert(key.to_string());
        }

        Self { confirm, back, increase, decrease }
    }

    pub fn is_confirm(&self, control: &str) -> bool {
        self.confirm.contains(control)
    }

    pub fn is_back(&self, control: &str) -> bool {
        self.back.contains(control)
    }

    pub fn is_increase(&self, control: &str) -> bool {
        self.increase.contains(control)
    }

    pub fn is_decrease(&self, control: &str) -> bool {
        self.decrease.contains(control)
    }
}

fn classify_scratch_control(
    control: &str,
    increase: &mut HashSet<String>,
    decrease: &mut HashSet<String>,
) {
    if control.contains("ScratchDown")
        || control.ends_with('+')
        || control == "AxisLeftX+"
        || control == "Button8"
    {
        increase.insert(control.to_string());
        return;
    }
    if control.contains("ScratchUp")
        || control.ends_with('-')
        || control == "AxisLeftX-"
        || control == "Button9"
    {
        decrease.insert(control.to_string());
        return;
    }
    increase.insert(control.to_string());
    decrease.insert(control.to_string());
}

/// 編集開始時点の値。キャンセル時に profile へ戻す。
#[derive(Debug, Clone)]
pub struct SettingsEditSession {
    pub entry_id: SettingsEntryId,
    baseline_volume: Option<u32>,
    baseline_offset_us: Option<i64>,
}

impl SettingsEditSession {
    pub fn capture(profile: &ProfileConfig, entry_id: SettingsEntryId) -> Self {
        let (baseline_volume, baseline_offset_us) = match entry_id {
            SettingsEntryId::MasterVolume => (Some(profile.audio_mix.master_volume), None),
            SettingsEntryId::KeyVolume => (Some(profile.audio_mix.key_volume), None),
            SettingsEntryId::BgmVolume => (Some(profile.audio_mix.bgm_volume), None),
            SettingsEntryId::PreviewVolume => (Some(profile.audio_mix.preview_volume), None),
            SettingsEntryId::InputOffsetMs => (None, Some(profile.judge.input_offset_us)),
            SettingsEntryId::VisualOffsetMs => (None, Some(profile.judge.visual_offset_us)),
        };
        Self { entry_id, baseline_volume, baseline_offset_us }
    }

    pub fn restore(&self, profile: &mut ProfileConfig) {
        if let Some(value) = self.baseline_volume {
            match self.entry_id {
                SettingsEntryId::MasterVolume => profile.audio_mix.master_volume = value,
                SettingsEntryId::KeyVolume => profile.audio_mix.key_volume = value,
                SettingsEntryId::BgmVolume => profile.audio_mix.bgm_volume = value,
                SettingsEntryId::PreviewVolume => profile.audio_mix.preview_volume = value,
                _ => {}
            }
        }
        if let Some(value) = self.baseline_offset_us {
            match self.entry_id {
                SettingsEntryId::InputOffsetMs => profile.judge.input_offset_us = value,
                SettingsEntryId::VisualOffsetMs => profile.judge.visual_offset_us = value,
                _ => {}
            }
        }
    }
}

pub fn adjust_settings_draft(
    profile: &mut ProfileConfig,
    session: &SettingsEditSession,
    delta: i32,
) -> bool {
    adjust_settings_value(profile, session.entry_id, delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::ProfileConfig;

    #[test]
    fn default_7k_bindings_map_scratch_and_keys() {
        let profile = ProfileConfig::new_default("default", "Default", 0);
        let bindings = SettingsBindings::from_profile(&profile.input);

        assert!(bindings.is_confirm("Z"));
        assert!(bindings.is_confirm("C"));
        assert!(bindings.is_back("S"));
        assert!(bindings.is_back("D"));
        assert!(bindings.is_increase("AxisLeftX+") || bindings.is_increase("LShift"));
    }

    #[test]
    fn edit_session_restore_reverts_volume() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let session = SettingsEditSession::capture(&profile, SettingsEntryId::MasterVolume);
        profile.audio_mix.master_volume = 50;
        session.restore(&mut profile);
        assert_eq!(profile.audio_mix.master_volume, 20);
    }
}
