use std::collections::HashSet;

use bmz_core::lane::KeyMode;
use bmz_gameplay::rule::RuleMode;

use crate::config::play_input::resolve_play_bindings;
use crate::config::profile_config::{
    AssistOptionConfig, BgaExpandConfig, BgaModeConfig, BottomShiftableGaugeConfig,
    DoubleOptionConfig, GaugeAutoShiftConfig, GaugeTypeConfig, HispeedModeConfig, HsFixConfig,
    InputActionConfig, JudgeAlgorithmConfig, LaneConfig, LaneEffectConfig, ProfileConfig,
    ProfileInputConfig, RandomOptionConfig, ReplaySlotRule, ScratchDirectionConfig,
    ScratchInputMode, SelectInputModeConfig, TargetOptionConfig,
};
use crate::config::settings_registry::{
    SettingsEntryId, adjust_settings_value, format_settings_value,
};
use bmz_render::scene::ResultGradeDiffDisplay;

use crate::ln_policy::LnPolicySetting;

/// 7KEY / 14KEY + スクラッチ向けの設定画面入力マッピング。
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

        let mut play_controls = HashSet::new();
        match input.select_input_mode {
            SelectInputModeConfig::Key7Key14 => {
                collect_play_settings_bindings(
                    input,
                    KeyMode::K7,
                    &mut confirm,
                    &mut back,
                    &mut increase,
                    &mut decrease,
                );
                collect_play_settings_bindings(
                    input,
                    KeyMode::K14,
                    &mut confirm,
                    &mut back,
                    &mut increase,
                    &mut decrease,
                );
            }
            SelectInputModeConfig::Key9 => {
                collect_play_9k_settings_bindings(
                    input,
                    &mut confirm,
                    &mut back,
                    &mut increase,
                    &mut decrease,
                    &mut play_controls,
                );
            }
        }

        for entry in &input.ui.bindings {
            if input.select_input_mode == SelectInputModeConfig::Key9
                && play_controls.contains(&entry.control)
            {
                continue;
            }
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
        for key in ["ArrowUp", "DPadDown", "ScratchDown"] {
            increase.insert(key.to_string());
        }
        for key in ["ArrowDown", "DPadUp", "ScratchUp"] {
            decrease.insert(key.to_string());
        }
        confirm.insert("Button1".to_string());
        back.insert("Select".to_string());

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

fn collect_play_9k_settings_bindings(
    input: &ProfileInputConfig,
    confirm: &mut HashSet<String>,
    back: &mut HashSet<String>,
    increase: &mut HashSet<String>,
    decrease: &mut HashSet<String>,
    play_controls: &mut HashSet<String>,
) {
    let Ok(play) = resolve_play_bindings(input, KeyMode::K9) else {
        return;
    };
    for entry in play {
        play_controls.insert(entry.control.clone());
        let Some(lane) = entry.lane else { continue };
        match lane {
            LaneConfig::Key3 => {
                back.insert(entry.control.clone());
            }
            LaneConfig::Key4 => {
                increase.insert(entry.control.clone());
            }
            LaneConfig::Key5 | LaneConfig::Key7 => {
                confirm.insert(entry.control.clone());
            }
            LaneConfig::Key6 => {
                decrease.insert(entry.control.clone());
            }
            _ => {}
        }
    }
}

fn collect_play_settings_bindings(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    confirm: &mut HashSet<String>,
    back: &mut HashSet<String>,
    increase: &mut HashSet<String>,
    decrease: &mut HashSet<String>,
) {
    let Ok(play) = resolve_play_bindings(input, key_mode) else {
        return;
    };
    for entry in play {
        let Some(lane) = entry.lane else { continue };
        match lane {
            LaneConfig::Key1
            | LaneConfig::Key3
            | LaneConfig::Key5
            | LaneConfig::Key7
            | LaneConfig::Key8
            | LaneConfig::Key10
            | LaneConfig::Key12
            | LaneConfig::Key14 => {
                confirm.insert(entry.control.clone());
            }
            LaneConfig::Key2
            | LaneConfig::Key4
            | LaneConfig::Key6
            | LaneConfig::Key9
            | LaneConfig::Key11
            | LaneConfig::Key13 => {
                back.insert(entry.control.clone());
            }
            LaneConfig::Scratch | LaneConfig::Scratch2 => match entry.scratch {
                Some(ScratchDirectionConfig::Down) => {
                    increase.insert(entry.control.clone());
                }
                Some(ScratchDirectionConfig::Up) => {
                    decrease.insert(entry.control.clone());
                }
                None => {
                    classify_scratch_control(&entry.control, increase, decrease);
                }
            },
        }
    }
}

fn classify_scratch_control(
    control: &str,
    increase: &mut HashSet<String>,
    decrease: &mut HashSet<String>,
) {
    if control.contains("ScratchDown")
        || control.ends_with('+')
        || control == "Axis1+"
        || control == "Button8"
    {
        increase.insert(control.to_string());
        return;
    }
    if control.contains("ScratchUp")
        || control.ends_with('-')
        || control == "Axis1-"
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
    RuleMode(RuleMode),
    LnModePolicy(LnPolicySetting),
    Gauge(GaugeTypeConfig),
    GaugeAutoShift(GaugeAutoShiftConfig),
    BottomShiftableGauge(BottomShiftableGaugeConfig),
    Random(RandomOptionConfig),
    DoubleOption(DoubleOptionConfig),
    HsFix(HsFixConfig),
    Target(TargetOptionConfig),
    GradeDiffDisplay(ResultGradeDiffDisplay),
    LaneEffect(LaneEffectConfig),
    Assist(AssistOptionConfig),
    BgaMode(BgaModeConfig),
    BgaExpand(BgaExpandConfig),
    HispeedMode(HispeedModeConfig),
    SelectInputMode(SelectInputModeConfig),
    ScratchInputMode(ScratchInputMode),
    ReplaySlotRule(ReplaySlotRule),
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
            SettingsEntryId::NormalizeChartVolume => {
                SettingsBaseline::Bool(profile.audio_mix.normalize_chart_volume)
            }
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
            SettingsEntryId::VisualOffsetAutoAdjust => {
                SettingsBaseline::Bool(profile.judge.visual_offset_auto_adjust)
            }
            SettingsEntryId::JudgeAlgorithm => {
                SettingsBaseline::JudgeAlgorithm(profile.judge.judge_algorithm)
            }
            SettingsEntryId::RuleMode => SettingsBaseline::RuleMode(profile.play.rule_mode),
            SettingsEntryId::LnModePolicy => {
                SettingsBaseline::LnModePolicy(profile.play.ln_mode_policy)
            }
            SettingsEntryId::Gauge => SettingsBaseline::Gauge(profile.play.gauge),
            SettingsEntryId::GaugeAutoShift => {
                SettingsBaseline::GaugeAutoShift(profile.play.gauge_auto_shift)
            }
            SettingsEntryId::BottomShiftableGauge => {
                SettingsBaseline::BottomShiftableGauge(profile.play.bottom_shiftable_gauge)
            }
            SettingsEntryId::Random => SettingsBaseline::Random(profile.play.random),
            SettingsEntryId::Random2 => SettingsBaseline::Random(profile.play.random2),
            SettingsEntryId::DoubleOption => {
                SettingsBaseline::DoubleOption(profile.play.double_option)
            }
            SettingsEntryId::HsFix => SettingsBaseline::HsFix(profile.play.hs_fix),
            SettingsEntryId::Target => SettingsBaseline::Target(profile.play.target),
            SettingsEntryId::GradeDiffDisplay => {
                SettingsBaseline::GradeDiffDisplay(profile.play.grade_diff_display)
            }
            SettingsEntryId::LaneEffect => SettingsBaseline::LaneEffect(profile.play.lane_effect),
            SettingsEntryId::Assist => SettingsBaseline::Assist(profile.play.assist),
            SettingsEntryId::BgaMode => SettingsBaseline::BgaMode(profile.play.bga),
            SettingsEntryId::BgaExpand => SettingsBaseline::BgaExpand(profile.play.bga_expand),
            SettingsEntryId::AutoPlay => SettingsBaseline::Bool(profile.play.auto_play),
            SettingsEntryId::MisslayerDurationMs => {
                SettingsBaseline::U32(profile.play.misslayer_duration_ms)
            }
            SettingsEntryId::ShowLnTailCap => SettingsBaseline::Bool(profile.play.show_ln_tail_cap),
            SettingsEntryId::Hispeed => SettingsBaseline::F32(profile.lane.hispeed),
            SettingsEntryId::HispeedMode => {
                SettingsBaseline::HispeedMode(profile.lane.hispeed_mode)
            }
            SettingsEntryId::Sudden => SettingsBaseline::U32(profile.lane.sudden),
            SettingsEntryId::Lift => SettingsBaseline::U32(profile.lane.lift),
            SettingsEntryId::Hidden => SettingsBaseline::U32(profile.lane.hidden),
            SettingsEntryId::TargetGreenNumber => {
                SettingsBaseline::U32(profile.lane.target_green_number)
            }
            SettingsEntryId::SelectInputMode => {
                SettingsBaseline::SelectInputMode(profile.input.select_input_mode)
            }
            SettingsEntryId::ScratchInputMode => {
                SettingsBaseline::ScratchInputMode(profile.input.scratch_mode)
            }
            SettingsEntryId::AnalogScratchSensitivity => {
                SettingsBaseline::F32(profile.input.analog_scratch_sensitivity)
            }
            SettingsEntryId::AnalogScratchThreshold => {
                SettingsBaseline::U32(profile.input.analog_scratch_threshold)
            }
            SettingsEntryId::AnalogTicksPerScroll => {
                SettingsBaseline::U32(profile.input.analog_ticks_per_scroll)
            }
            SettingsEntryId::ReplayAutoSave => SettingsBaseline::Bool(profile.replay.auto_save),
            SettingsEntryId::ReplaySlot1Rule => {
                SettingsBaseline::ReplaySlotRule(profile.replay.slot_rules[0])
            }
            SettingsEntryId::ReplaySlot2Rule => {
                SettingsBaseline::ReplaySlotRule(profile.replay.slot_rules[1])
            }
            SettingsEntryId::ReplaySlot3Rule => {
                SettingsBaseline::ReplaySlotRule(profile.replay.slot_rules[2])
            }
            SettingsEntryId::ReplaySlot4Rule => {
                SettingsBaseline::ReplaySlotRule(profile.replay.slot_rules[3])
            }
        };
        Self { entry_id, baseline }
    }

    pub fn restore(&self, profile: &mut ProfileConfig) {
        match (&self.entry_id, &self.baseline) {
            (SettingsEntryId::NormalizeChartVolume, SettingsBaseline::Bool(value)) => {
                profile.audio_mix.normalize_chart_volume = *value;
            }
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
            (SettingsEntryId::VisualOffsetAutoAdjust, SettingsBaseline::Bool(value)) => {
                profile.judge.visual_offset_auto_adjust = *value;
            }
            (SettingsEntryId::JudgeAlgorithm, SettingsBaseline::JudgeAlgorithm(value)) => {
                profile.judge.judge_algorithm = *value;
            }
            (SettingsEntryId::RuleMode, SettingsBaseline::RuleMode(value)) => {
                profile.play.rule_mode = *value;
            }
            (SettingsEntryId::LnModePolicy, SettingsBaseline::LnModePolicy(value)) => {
                profile.play.ln_mode_policy = *value;
            }
            (SettingsEntryId::Gauge, SettingsBaseline::Gauge(value)) => {
                profile.play.gauge = *value;
            }
            (SettingsEntryId::GaugeAutoShift, SettingsBaseline::GaugeAutoShift(value)) => {
                profile.play.gauge_auto_shift = *value;
            }
            (
                SettingsEntryId::BottomShiftableGauge,
                SettingsBaseline::BottomShiftableGauge(value),
            ) => {
                profile.play.bottom_shiftable_gauge = *value;
            }
            (SettingsEntryId::Random, SettingsBaseline::Random(value)) => {
                profile.play.random = *value;
            }
            (SettingsEntryId::Random2, SettingsBaseline::Random(value)) => {
                profile.play.random2 = *value;
            }
            (SettingsEntryId::DoubleOption, SettingsBaseline::DoubleOption(value)) => {
                profile.play.double_option = *value;
            }
            (SettingsEntryId::HsFix, SettingsBaseline::HsFix(value)) => {
                profile.play.hs_fix = *value;
            }
            (SettingsEntryId::Target, SettingsBaseline::Target(value)) => {
                profile.play.target = *value;
            }
            (SettingsEntryId::GradeDiffDisplay, SettingsBaseline::GradeDiffDisplay(value)) => {
                profile.play.grade_diff_display = *value;
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
            (SettingsEntryId::MisslayerDurationMs, SettingsBaseline::U32(value)) => {
                profile.play.misslayer_duration_ms = *value;
            }
            (SettingsEntryId::ShowLnTailCap, SettingsBaseline::Bool(value)) => {
                profile.play.show_ln_tail_cap = *value;
            }
            (SettingsEntryId::Hispeed, SettingsBaseline::F32(value)) => {
                profile.lane.hispeed = *value;
            }
            (SettingsEntryId::HispeedMode, SettingsBaseline::HispeedMode(value)) => {
                profile.lane.hispeed_mode = *value;
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
            (SettingsEntryId::TargetGreenNumber, SettingsBaseline::U32(value)) => {
                profile.lane.target_green_number = *value;
            }
            (SettingsEntryId::SelectInputMode, SettingsBaseline::SelectInputMode(value)) => {
                profile.input.select_input_mode = *value;
            }
            (SettingsEntryId::ScratchInputMode, SettingsBaseline::ScratchInputMode(value)) => {
                profile.input.scratch_mode = *value;
            }
            (SettingsEntryId::AnalogScratchSensitivity, SettingsBaseline::F32(value)) => {
                profile.input.analog_scratch_sensitivity = *value;
            }
            (SettingsEntryId::AnalogScratchThreshold, SettingsBaseline::U32(value)) => {
                profile.input.analog_scratch_threshold = *value;
            }
            (SettingsEntryId::AnalogTicksPerScroll, SettingsBaseline::U32(value)) => {
                profile.input.analog_ticks_per_scroll = *value;
            }
            (SettingsEntryId::ReplayAutoSave, SettingsBaseline::Bool(value)) => {
                profile.replay.auto_save = *value;
            }
            (SettingsEntryId::ReplaySlot1Rule, SettingsBaseline::ReplaySlotRule(value)) => {
                profile.replay.slot_rules[0] = *value;
            }
            (SettingsEntryId::ReplaySlot2Rule, SettingsBaseline::ReplaySlotRule(value)) => {
                profile.replay.slot_rules[1] = *value;
            }
            (SettingsEntryId::ReplaySlot3Rule, SettingsBaseline::ReplaySlotRule(value)) => {
                profile.replay.slot_rules[2] = *value;
            }
            (SettingsEntryId::ReplaySlot4Rule, SettingsBaseline::ReplaySlotRule(value)) => {
                profile.replay.slot_rules[3] = *value;
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
        assert!(bindings.is_increase("Axis1+") || bindings.is_increase("LShift"));
    }

    #[test]
    fn default_14k_2p_bindings_map_scratch_and_keys() {
        let profile = ProfileConfig::new_default("default", "Default", 0);
        let bindings = SettingsBindings::from_profile(&profile.input);

        assert!(bindings.is_confirm("M"));
        assert!(bindings.is_confirm("Period"));
        assert!(bindings.is_confirm("Slash"));
        assert!(bindings.is_back("K"));
        assert!(bindings.is_back("L"));
        assert!(bindings.is_back("Semicolon"));
        assert!(bindings.is_decrease("RShift"));
        assert!(bindings.is_increase("RControl"));
    }

    #[test]
    fn cursor_up_increases_and_down_decreases_settings_values() {
        let profile = ProfileConfig::new_default("default", "Default", 0);
        let bindings = SettingsBindings::from_profile(&profile.input);

        assert!(bindings.is_increase("ArrowUp"));
        assert!(!bindings.is_decrease("ArrowUp"));
        assert!(bindings.is_decrease("ArrowDown"));
        assert!(!bindings.is_increase("ArrowDown"));
    }

    #[test]
    fn key9_select_input_maps_settings_navigation_keys() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.input.select_input_mode = SelectInputModeConfig::Key9;
        let bindings = SettingsBindings::from_profile(&profile.input);

        assert!(bindings.is_confirm("C"));
        assert!(bindings.is_confirm("V"));
        assert!(bindings.is_back("X"));
        assert!(bindings.is_increase("D"));
        assert!(bindings.is_decrease("F"));
        assert!(!bindings.is_confirm("Z"));
        assert!(!bindings.is_back("S"));
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
        profile.audio_mix.normalize_chart_volume = true;
        normalize_session.restore(&mut profile);
        assert!(!profile.audio_mix.normalize_chart_volume);
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
        let scratch_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::ScratchInputMode);
        profile.input.scratch_mode = ScratchInputMode::AnyDirection;
        scratch_session.restore(&mut profile);
        assert_eq!(profile.input.scratch_mode, ScratchInputMode::Normal);

        let replay_session =
            SettingsEditSession::capture(&profile, SettingsEntryId::ReplaySlot2Rule);
        profile.replay.slot_rules[1] = ReplaySlotRule::ClearUpdate;
        replay_session.restore(&mut profile);
        assert_eq!(profile.replay.slot_rules[1], ReplaySlotRule::ScoreUpdate);
    }
}
