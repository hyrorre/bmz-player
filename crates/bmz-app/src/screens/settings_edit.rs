use std::collections::HashSet;

use bmz_core::lane::KeyMode;

use crate::config::play_input::resolve_play_bindings;
use crate::config::profile_config::{
    AssistOptionConfig, BgaExpandConfig, BgaModeConfig, GaugeAutoShiftConfig, GaugeTypeConfig,
    InputActionConfig, JudgeAlgorithmConfig, LaneConfig, LaneEffectConfig, ProfileConfig,
    ProfileInputConfig, RandomOptionConfig, TargetOptionConfig,
};
use crate::config::settings_registry::{
    SettingsEntryId, adjust_settings_value, format_settings_value,
};

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

#[derive(Debug, Clone)]
enum SettingsBaseline {
    Volume(u32),
    OffsetUs(i64),
    F32(f32),
    U32(u32),
    Bool(bool),
    JudgeAlgorithm(JudgeAlgorithmConfig),
    Gauge(GaugeTypeConfig),
    GaugeAutoShift(GaugeAutoShiftConfig),
    Random(RandomOptionConfig),
    Target(TargetOptionConfig),
    LaneEffect(LaneEffectConfig),
    Assist(AssistOptionConfig),
    BgaMode(BgaModeConfig),
    BgaExpand(BgaExpandConfig),
}

/// 編集開始時点の値。キャンセル時に profile へ戻す。
#[derive(Debug, Clone)]
pub struct SettingsEditSession {
    pub entry_id: SettingsEntryId,
    baseline: SettingsBaseline,
}

impl SettingsEditSession {
    pub fn capture(profile: &ProfileConfig, entry_id: SettingsEntryId) -> Self {
        let baseline = match entry_id {
            SettingsEntryId::MasterVolume => {
                SettingsBaseline::Volume(profile.audio_mix.master_volume)
            }
            SettingsEntryId::KeyVolume => SettingsBaseline::Volume(profile.audio_mix.key_volume),
            SettingsEntryId::BgmVolume => SettingsBaseline::Volume(profile.audio_mix.bgm_volume),
            SettingsEntryId::PreviewVolume => {
                SettingsBaseline::Volume(profile.audio_mix.preview_volume)
            }
            SettingsEntryId::SystemBgmVolume => {
                SettingsBaseline::Volume(profile.audio_mix.system_bgm_volume)
            }
            SettingsEntryId::SystemSeVolume => {
                SettingsBaseline::Volume(profile.audio_mix.system_se_volume)
            }
            SettingsEntryId::InputOffsetMs => {
                SettingsBaseline::OffsetUs(profile.judge.input_offset_us)
            }
            SettingsEntryId::VisualOffsetMs => {
                SettingsBaseline::OffsetUs(profile.judge.visual_offset_us)
            }
            SettingsEntryId::JudgeAlgorithm => {
                SettingsBaseline::JudgeAlgorithm(profile.judge.judge_algorithm)
            }
            SettingsEntryId::Gauge => SettingsBaseline::Gauge(profile.play.gauge),
            SettingsEntryId::GaugeAutoShift => {
                SettingsBaseline::GaugeAutoShift(profile.play.gauge_auto_shift)
            }
            SettingsEntryId::Random => SettingsBaseline::Random(profile.play.random),
            SettingsEntryId::Target => SettingsBaseline::Target(profile.play.target),
            SettingsEntryId::LaneEffect => SettingsBaseline::LaneEffect(profile.play.lane_effect),
            SettingsEntryId::Assist => SettingsBaseline::Assist(profile.play.assist),
            SettingsEntryId::BgaMode => SettingsBaseline::BgaMode(profile.play.bga),
            SettingsEntryId::BgaExpand => SettingsBaseline::BgaExpand(profile.play.bga_expand),
            SettingsEntryId::AutoPlay => SettingsBaseline::Bool(profile.play.auto_play),
            SettingsEntryId::Hispeed => SettingsBaseline::F32(profile.lane.hispeed),
            SettingsEntryId::Sudden => SettingsBaseline::U32(profile.lane.sudden),
            SettingsEntryId::Lift => SettingsBaseline::U32(profile.lane.lift),
            SettingsEntryId::Hidden => SettingsBaseline::U32(profile.lane.hidden),
        };
        Self { entry_id, baseline }
    }

    pub fn restore(&self, profile: &mut ProfileConfig) {
        match (&self.entry_id, &self.baseline) {
            (SettingsEntryId::MasterVolume, SettingsBaseline::Volume(value)) => {
                profile.audio_mix.master_volume = *value;
            }
            (SettingsEntryId::KeyVolume, SettingsBaseline::Volume(value)) => {
                profile.audio_mix.key_volume = *value;
            }
            (SettingsEntryId::BgmVolume, SettingsBaseline::Volume(value)) => {
                profile.audio_mix.bgm_volume = *value;
            }
            (SettingsEntryId::PreviewVolume, SettingsBaseline::Volume(value)) => {
                profile.audio_mix.preview_volume = *value;
            }
            (SettingsEntryId::SystemBgmVolume, SettingsBaseline::Volume(value)) => {
                profile.audio_mix.system_bgm_volume = *value;
            }
            (SettingsEntryId::SystemSeVolume, SettingsBaseline::Volume(value)) => {
                profile.audio_mix.system_se_volume = *value;
            }
            (SettingsEntryId::InputOffsetMs, SettingsBaseline::OffsetUs(value)) => {
                profile.judge.input_offset_us = *value;
            }
            (SettingsEntryId::VisualOffsetMs, SettingsBaseline::OffsetUs(value)) => {
                profile.judge.visual_offset_us = *value;
            }
            (SettingsEntryId::JudgeAlgorithm, SettingsBaseline::JudgeAlgorithm(value)) => {
                profile.judge.judge_algorithm = *value;
            }
            (SettingsEntryId::Gauge, SettingsBaseline::Gauge(value)) => {
                profile.play.gauge = *value;
            }
            (SettingsEntryId::GaugeAutoShift, SettingsBaseline::GaugeAutoShift(value)) => {
                profile.play.gauge_auto_shift = *value;
            }
            (SettingsEntryId::Random, SettingsBaseline::Random(value)) => {
                profile.play.random = *value;
            }
            (SettingsEntryId::Target, SettingsBaseline::Target(value)) => {
                profile.play.target = *value;
            }
            (SettingsEntryId::LaneEffect, SettingsBaseline::LaneEffect(value)) => {
                profile.play.lane_effect = *value;
            }
            (SettingsEntryId::Assist, SettingsBaseline::Assist(value)) => {
                profile.play.assist = *value;
            }
            (SettingsEntryId::BgaMode, SettingsBaseline::BgaMode(value)) => {
                profile.play.bga = *value;
            }
            (SettingsEntryId::BgaExpand, SettingsBaseline::BgaExpand(value)) => {
                profile.play.bga_expand = *value;
            }
            (SettingsEntryId::AutoPlay, SettingsBaseline::Bool(value)) => {
                profile.play.auto_play = *value;
            }
            (SettingsEntryId::Hispeed, SettingsBaseline::F32(value)) => {
                profile.lane.hispeed = *value;
            }
            (SettingsEntryId::Sudden, SettingsBaseline::U32(value)) => {
                profile.lane.sudden = *value;
            }
            (SettingsEntryId::Lift, SettingsBaseline::U32(value)) => {
                profile.lane.lift = *value;
            }
            (SettingsEntryId::Hidden, SettingsBaseline::U32(value)) => {
                profile.lane.hidden = *value;
            }
            _ => {}
        }
    }

    pub fn preview_value(&self, profile: &ProfileConfig) -> String {
        format_settings_value(profile, self.entry_id)
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

    #[test]
    fn edit_session_restore_reverts_gauge() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        let session = SettingsEditSession::capture(&profile, SettingsEntryId::Gauge);
        profile.play.gauge = GaugeTypeConfig::Hazard;
        session.restore(&mut profile);
        assert_eq!(profile.play.gauge, GaugeTypeConfig::Normal);
    }
}
