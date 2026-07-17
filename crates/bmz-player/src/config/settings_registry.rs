use super::play::{TARGET_GREEN_NUMBER_MAX, TARGET_GREEN_NUMBER_MIN};
use super::profile_config::{
    AssistOptionConfig, BgaExpandConfig, BgaModeConfig, BottomShiftableGaugeConfig,
    DoubleOptionConfig, GaugeAutoShiftConfig, GaugeTypeConfig, HISPEED_STEP_MAX, HISPEED_STEP_MIN,
    HispeedModeConfig, HsFixConfig, JudgeAlgorithmConfig, LaneEffectConfig, ProfileConfig,
    RandomOptionConfig, ReplaySlotRule, ScratchInputMode, SelectInputModeConfig,
    TargetOptionConfig, default_hispeed_step_fhs, default_hispeed_step_nhs, normalize_hispeed_step,
};
use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::ResultGradeDiffDisplay;

use crate::ln_policy::LnPolicySetting;

/// ゲーム内設定で編集可能な profile.toml 項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsEntryId {
    NormalizeChartVolume,
    MasterVolume,
    KeyVolume,
    BgmVolume,
    PreviewVolume,
    SystemBgmVolume,
    SystemSeVolume,
    InputOffsetMs,
    VisualOffsetMs,
    VisualOffsetAutoAdjust,
    JudgeAlgorithm,
    RuleMode,
    LnModePolicy,
    Gauge,
    GaugeAutoShift,
    BottomShiftableGauge,
    Random,
    Random2,
    DoubleOption,
    HsFix,
    Target,
    GradeDiffDisplay,
    LaneEffect,
    Assist,
    BgaMode,
    BgaExpand,
    AutoPlay,
    MisslayerDurationMs,
    ShowLnTailCap,
    Hispeed,
    HispeedMode,
    HispeedStepNhs,
    HispeedStepFhs,
    Sudden,
    Lift,
    Hidden,
    TargetGreenNumber,
    SelectInputMode,
    ScratchInputMode,
    AnalogScratchSensitivity,
    AnalogScratchThreshold,
    AnalogTicksPerScroll,
    SelectRandomSelect,
    ReplayAutoSave,
    ReplaySlot1Rule,
    ReplaySlot2Rule,
    ReplaySlot3Rule,
    ReplaySlot4Rule,
}

impl SettingsEntryId {
    pub const VOLUME_ENTRIES: &'static [Self] = &[
        Self::NormalizeChartVolume,
        Self::MasterVolume,
        Self::KeyVolume,
        Self::BgmVolume,
        Self::PreviewVolume,
        Self::SystemBgmVolume,
        Self::SystemSeVolume,
    ];

    pub const JUDGE_ENTRIES: &'static [Self] = &[
        Self::InputOffsetMs,
        Self::VisualOffsetMs,
        Self::VisualOffsetAutoAdjust,
        Self::JudgeAlgorithm,
    ];

    pub const PLAY_ENTRIES: &'static [Self] = &[
        Self::Gauge,
        Self::RuleMode,
        Self::LnModePolicy,
        Self::GaugeAutoShift,
        Self::BottomShiftableGauge,
        Self::Random,
        Self::Random2,
        Self::DoubleOption,
        Self::HsFix,
        Self::Target,
        Self::GradeDiffDisplay,
        Self::LaneEffect,
        Self::Assist,
        Self::BgaMode,
        Self::BgaExpand,
        Self::AutoPlay,
        Self::MisslayerDurationMs,
        Self::ShowLnTailCap,
    ];

    pub const DISPLAY_ENTRIES: &'static [Self] = &[
        Self::Hispeed,
        Self::HispeedMode,
        Self::HispeedStepNhs,
        Self::HispeedStepFhs,
        Self::Sudden,
        Self::Lift,
        Self::Hidden,
        Self::TargetGreenNumber,
    ];

    pub const INPUT_ENTRIES: &'static [Self] = &[
        Self::SelectInputMode,
        Self::ScratchInputMode,
        Self::AnalogScratchSensitivity,
        Self::AnalogScratchThreshold,
        Self::AnalogTicksPerScroll,
    ];

    pub const SELECT_ENTRIES: &'static [Self] = &[Self::SelectRandomSelect];

    pub const REPLAY_ENTRIES: &'static [Self] = &[
        Self::ReplayAutoSave,
        Self::ReplaySlot1Rule,
        Self::ReplaySlot2Rule,
        Self::ReplaySlot3Rule,
        Self::ReplaySlot4Rule,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NormalizeChartVolume => "NORMALIZE",
            Self::MasterVolume => "MASTER",
            Self::KeyVolume => "KEY",
            Self::BgmVolume => "BGM",
            Self::PreviewVolume => "PREVIEW",
            Self::SystemBgmVolume => "SYS BGM",
            Self::SystemSeVolume => "SYS SE",
            Self::InputOffsetMs => "INPUT OFFSET",
            Self::VisualOffsetMs => "VISUAL OFFSET",
            Self::VisualOffsetAutoAdjust => "AUTO ADJUST",
            Self::JudgeAlgorithm => "JUDGE ALGO",
            Self::RuleMode => "RULE MODE",
            Self::LnModePolicy => "LN MODE",
            Self::Gauge => "GAUGE",
            Self::GaugeAutoShift => "GAUGE SHIFT",
            Self::BottomShiftableGauge => "GAS BOTTOM",
            Self::Random => "RANDOM",
            Self::Random2 => "RANDOM 2P",
            Self::DoubleOption => "DP OPTION",
            Self::HsFix => "HS-FIX",
            Self::Target => "TARGET",
            Self::GradeDiffDisplay => "GRADE DIFF",
            Self::LaneEffect => "LANE FX",
            Self::Assist => "ASSIST",
            Self::BgaMode => "BGA",
            Self::BgaExpand => "BGA FIT",
            Self::AutoPlay => "AUTO PLAY",
            Self::MisslayerDurationMs => "MISSLAYER",
            Self::ShowLnTailCap => "LN TAIL CAP",
            Self::Hispeed => "HISPEED",
            Self::HispeedMode => "HS MODE",
            Self::HispeedStepNhs => "HS STEP NHS",
            Self::HispeedStepFhs => "HS STEP FHS",
            Self::Sudden => "SUDDEN+",
            Self::Lift => "LIFT",
            Self::Hidden => "HIDDEN",
            Self::TargetGreenNumber => "GREEN NO.",
            Self::SelectInputMode => "SELECT INPUT",
            Self::ScratchInputMode => "SCRATCH",
            Self::AnalogScratchSensitivity => "ANALOG SENS",
            Self::AnalogScratchThreshold => "ANALOG STOP",
            Self::AnalogTicksPerScroll => "ANALOG SCROLL",
            Self::SelectRandomSelect => "RANDOM SELECT",
            Self::ReplayAutoSave => "REPLAY SAVE",
            Self::ReplaySlot1Rule => "REPLAY 1",
            Self::ReplaySlot2Rule => "REPLAY 2",
            Self::ReplaySlot3Rule => "REPLAY 3",
            Self::ReplaySlot4Rule => "REPLAY 4",
        }
    }
}

/// 設定値 1 ステップの増減量。
pub fn settings_adjust_step(id: SettingsEntryId) -> i32 {
    match id {
        SettingsEntryId::InputOffsetMs | SettingsEntryId::VisualOffsetMs => 1,
        SettingsEntryId::Sudden | SettingsEntryId::Lift | SettingsEntryId::Hidden => 25,
        SettingsEntryId::TargetGreenNumber => 10,
        SettingsEntryId::MisslayerDurationMs => 50,
        SettingsEntryId::AnalogScratchThreshold => 10,
        _ => 1,
    }
}

pub fn format_settings_value(profile: &ProfileConfig, id: SettingsEntryId) -> String {
    match id {
        SettingsEntryId::NormalizeChartVolume => {
            format_bool_on_off(profile.audio_mix.normalize_chart_volume)
        }
        SettingsEntryId::MasterVolume => format!("{}", profile.audio_mix.master_volume),
        SettingsEntryId::KeyVolume => format!("{}", profile.audio_mix.key_volume),
        SettingsEntryId::BgmVolume => format!("{}", profile.audio_mix.bgm_volume),
        SettingsEntryId::PreviewVolume => format!("{}", profile.audio_mix.preview_volume),
        SettingsEntryId::SystemBgmVolume => format!("{}", profile.audio_mix.system_bgm_volume),
        SettingsEntryId::SystemSeVolume => format!("{}", profile.audio_mix.system_se_volume),
        SettingsEntryId::InputOffsetMs => {
            format!("{} ms", profile.judge.input_offset_us / 1_000)
        }
        SettingsEntryId::VisualOffsetMs => {
            format!("{} ms", profile.judge.visual_offset_us / 1_000)
        }
        SettingsEntryId::VisualOffsetAutoAdjust => {
            format_bool_on_off(profile.judge.visual_offset_auto_adjust)
        }
        SettingsEntryId::JudgeAlgorithm => format_judge_algorithm(profile.judge.judge_algorithm),
        SettingsEntryId::RuleMode => format_rule_mode(profile.play.rule_mode),
        SettingsEntryId::LnModePolicy => profile.play.ln_mode_policy.display_label().to_string(),
        SettingsEntryId::Gauge => format_gauge(profile.play.gauge),
        SettingsEntryId::GaugeAutoShift => format_gauge_auto_shift(profile.play.gauge_auto_shift),
        SettingsEntryId::BottomShiftableGauge => {
            format_bottom_shiftable_gauge(profile.play.bottom_shiftable_gauge)
        }
        SettingsEntryId::Random => format_random(profile.play.random),
        SettingsEntryId::Random2 => format_random(profile.play.random2),
        SettingsEntryId::DoubleOption => format_double_option(profile.play.double_option),
        SettingsEntryId::HsFix => format_hs_fix(profile.play.hs_fix),
        SettingsEntryId::Target => format_target(profile.play.target),
        SettingsEntryId::GradeDiffDisplay => {
            format_grade_diff_display(profile.play.grade_diff_display)
        }
        SettingsEntryId::LaneEffect => format_lane_effect(profile.play.lane_effect),
        SettingsEntryId::Assist => format_assist(profile.play.assist),
        SettingsEntryId::BgaMode => format_bga_mode(profile.play.bga),
        SettingsEntryId::BgaExpand => format_bga_expand(profile.play.bga_expand),
        SettingsEntryId::AutoPlay => format_bool_on_off(profile.play.auto_play),
        SettingsEntryId::ShowLnTailCap => format_bool_on_off(profile.play.show_ln_tail_cap),
        SettingsEntryId::MisslayerDurationMs => {
            format!("{} ms", profile.play.misslayer_duration_ms)
        }
        SettingsEntryId::Hispeed => format!("{:.2}", profile.lane.hispeed),
        SettingsEntryId::HispeedMode => format_hispeed_mode(profile.lane.hispeed_mode),
        SettingsEntryId::HispeedStepNhs => format!("{:.2}", profile.lane.hispeed_step_nhs),
        SettingsEntryId::HispeedStepFhs => format!("{:.2}", profile.lane.hispeed_step_fhs),
        SettingsEntryId::Sudden => format_lane_unit(profile.lane.sudden),
        SettingsEntryId::Lift => format_lane_unit(profile.lane.lift),
        SettingsEntryId::Hidden => format_lane_unit(profile.lane.hidden),
        SettingsEntryId::TargetGreenNumber => format!("{}", profile.lane.target_green_number),
        SettingsEntryId::SelectInputMode => {
            profile.input.select_input_mode.display_label().to_string()
        }
        SettingsEntryId::ScratchInputMode => format_scratch_input_mode(profile.input.scratch_mode),
        SettingsEntryId::AnalogScratchSensitivity => {
            format!("{:.1}", profile.input.analog_scratch_sensitivity)
        }
        SettingsEntryId::AnalogScratchThreshold => {
            format!("{} ticks", profile.input.analog_scratch_threshold)
        }
        SettingsEntryId::AnalogTicksPerScroll => {
            format!("{} ticks", profile.input.analog_ticks_per_scroll)
        }
        SettingsEntryId::SelectRandomSelect => format_bool_on_off(profile.select.random_select),
        SettingsEntryId::ReplayAutoSave => format_bool_on_off(profile.replay.auto_save),
        SettingsEntryId::ReplaySlot1Rule => format_replay_slot_rule(profile.replay.slot_rules[0]),
        SettingsEntryId::ReplaySlot2Rule => format_replay_slot_rule(profile.replay.slot_rules[1]),
        SettingsEntryId::ReplaySlot3Rule => format_replay_slot_rule(profile.replay.slot_rules[2]),
        SettingsEntryId::ReplaySlot4Rule => format_replay_slot_rule(profile.replay.slot_rules[3]),
    }
}

/// 設定値を 1 ステップ変更する。変更があった場合 `true`。
pub fn adjust_settings_value(profile: &mut ProfileConfig, id: SettingsEntryId, delta: i32) -> bool {
    if delta == 0 {
        return false;
    }
    match id {
        SettingsEntryId::NormalizeChartVolume => {
            profile.audio_mix.normalize_chart_volume = !profile.audio_mix.normalize_chart_volume;
            true
        }
        SettingsEntryId::MasterVolume => {
            adjust_u32(&mut profile.audio_mix.master_volume, delta, 0, 100)
        }
        SettingsEntryId::KeyVolume => adjust_u32(&mut profile.audio_mix.key_volume, delta, 0, 100),
        SettingsEntryId::BgmVolume => adjust_u32(&mut profile.audio_mix.bgm_volume, delta, 0, 100),
        SettingsEntryId::PreviewVolume => {
            adjust_u32(&mut profile.audio_mix.preview_volume, delta, 0, 100)
        }
        SettingsEntryId::SystemBgmVolume => {
            adjust_u32(&mut profile.audio_mix.system_bgm_volume, delta, 0, 100)
        }
        SettingsEntryId::SystemSeVolume => {
            adjust_u32(&mut profile.audio_mix.system_se_volume, delta, 0, 100)
        }
        SettingsEntryId::InputOffsetMs => {
            adjust_offset_ms(&mut profile.judge.input_offset_us, delta)
        }
        SettingsEntryId::VisualOffsetMs => {
            adjust_offset_ms(&mut profile.judge.visual_offset_us, delta)
        }
        SettingsEntryId::VisualOffsetAutoAdjust => {
            profile.judge.visual_offset_auto_adjust = !profile.judge.visual_offset_auto_adjust;
            true
        }
        SettingsEntryId::JudgeAlgorithm => {
            cycle_enum(delta, profile.judge.judge_algorithm, cycle_judge_algorithm)
                .map(|next| profile.judge.judge_algorithm = next)
                .is_some()
        }
        SettingsEntryId::RuleMode => cycle_enum(delta, profile.play.rule_mode, cycle_rule_mode)
            .map(|next| profile.play.rule_mode = next)
            .is_some(),
        SettingsEntryId::LnModePolicy => {
            cycle_enum(delta, profile.play.ln_mode_policy, cycle_ln_mode_policy)
                .map(|next| profile.play.ln_mode_policy = next)
                .is_some()
        }
        SettingsEntryId::Gauge => cycle_enum(delta, profile.play.gauge, cycle_gauge)
            .map(|next| profile.play.gauge = next)
            .is_some(),
        SettingsEntryId::GaugeAutoShift => {
            cycle_enum(delta, profile.play.gauge_auto_shift, cycle_gauge_auto_shift)
                .map(|next| profile.play.gauge_auto_shift = next)
                .is_some()
        }
        SettingsEntryId::BottomShiftableGauge => {
            cycle_enum(delta, profile.play.bottom_shiftable_gauge, cycle_bottom_shiftable_gauge)
                .map(|next| profile.play.bottom_shiftable_gauge = next)
                .is_some()
        }
        SettingsEntryId::Random => cycle_enum(delta, profile.play.random, cycle_random)
            .map(|next| profile.play.random = next)
            .is_some(),
        SettingsEntryId::Random2 => cycle_enum(delta, profile.play.random2, cycle_random)
            .map(|next| profile.play.random2 = next)
            .is_some(),
        SettingsEntryId::DoubleOption => {
            cycle_enum(delta, profile.play.double_option, cycle_double_option)
                .map(|next| profile.play.double_option = next)
                .is_some()
        }
        SettingsEntryId::HsFix => cycle_enum(delta, profile.play.hs_fix, cycle_hs_fix)
            .map(|next| profile.play.hs_fix = next)
            .is_some(),
        SettingsEntryId::Target => cycle_enum(delta, profile.play.target, cycle_target)
            .map(|next| profile.play.target = next)
            .is_some(),
        SettingsEntryId::GradeDiffDisplay => {
            cycle_enum(delta, profile.play.grade_diff_display, cycle_grade_diff_display)
                .map(|next| profile.play.grade_diff_display = next)
                .is_some()
        }
        SettingsEntryId::LaneEffect => {
            cycle_enum(delta, profile.play.lane_effect, cycle_lane_effect)
                .map(|next| profile.play.lane_effect = next)
                .is_some()
        }
        SettingsEntryId::Assist => cycle_enum(delta, profile.play.assist, cycle_assist)
            .map(|next| profile.play.assist = next)
            .is_some(),
        SettingsEntryId::BgaMode => cycle_enum(delta, profile.play.bga, cycle_bga_mode)
            .map(|next| profile.play.bga = next)
            .is_some(),
        SettingsEntryId::BgaExpand => cycle_enum(delta, profile.play.bga_expand, cycle_bga_expand)
            .map(|next| profile.play.bga_expand = next)
            .is_some(),
        SettingsEntryId::AutoPlay => {
            if delta == 0 {
                false
            } else {
                profile.play.auto_play = !profile.play.auto_play;
                true
            }
        }
        SettingsEntryId::MisslayerDurationMs => {
            adjust_u32(&mut profile.play.misslayer_duration_ms, delta, 0, 5000)
        }
        SettingsEntryId::ShowLnTailCap => {
            if delta == 0 {
                false
            } else {
                profile.play.show_ln_tail_cap = !profile.play.show_ln_tail_cap;
                true
            }
        }
        SettingsEntryId::Hispeed => {
            let (step, default) = match profile.lane.hispeed_mode {
                HispeedModeConfig::Normal => {
                    (profile.lane.hispeed_step_nhs, default_hispeed_step_nhs())
                }
                HispeedModeConfig::Floating => {
                    (profile.lane.hispeed_step_fhs, default_hispeed_step_fhs())
                }
            };
            adjust_hispeed(&mut profile.lane.hispeed, delta, step, default)
        }
        SettingsEntryId::HispeedMode => {
            cycle_enum(delta, profile.lane.hispeed_mode, cycle_hispeed_mode)
                .map(|next| profile.lane.hispeed_mode = next)
                .is_some()
        }
        SettingsEntryId::HispeedStepNhs => {
            adjust_hispeed_step(&mut profile.lane.hispeed_step_nhs, delta)
        }
        SettingsEntryId::HispeedStepFhs => {
            adjust_hispeed_step(&mut profile.lane.hispeed_step_fhs, delta)
        }
        SettingsEntryId::Sudden => adjust_u32(
            &mut profile.lane.sudden,
            delta,
            0,
            crate::config::play::lane_unit_max_for_other(profile.lane.lift),
        ),
        SettingsEntryId::Lift => adjust_u32(
            &mut profile.lane.lift,
            delta,
            0,
            crate::config::play::lane_unit_max_for_other(profile.lane.sudden),
        ),
        SettingsEntryId::Hidden => adjust_u32(&mut profile.lane.hidden, delta, 0, 1000),
        SettingsEntryId::TargetGreenNumber => adjust_u32(
            &mut profile.lane.target_green_number,
            delta,
            TARGET_GREEN_NUMBER_MIN,
            TARGET_GREEN_NUMBER_MAX,
        ),
        SettingsEntryId::SelectInputMode => {
            cycle_enum(delta, profile.input.select_input_mode, cycle_select_input_mode)
                .map(|next| profile.input.select_input_mode = next)
                .is_some()
        }
        SettingsEntryId::ScratchInputMode => {
            cycle_enum(delta, profile.input.scratch_mode, cycle_scratch_input_mode)
                .map(|next| profile.input.scratch_mode = next)
                .is_some()
        }
        SettingsEntryId::AnalogScratchSensitivity => {
            adjust_f32_tenths(&mut profile.input.analog_scratch_sensitivity, delta, 0.1, 5.0)
        }
        SettingsEntryId::AnalogScratchThreshold => {
            adjust_u32(&mut profile.input.analog_scratch_threshold, delta, 1, 1000)
        }
        SettingsEntryId::AnalogTicksPerScroll => {
            adjust_u32(&mut profile.input.analog_ticks_per_scroll, delta, 1, 100)
        }
        SettingsEntryId::SelectRandomSelect => {
            if delta == 0 {
                false
            } else {
                profile.select.random_select = !profile.select.random_select;
                true
            }
        }
        SettingsEntryId::ReplayAutoSave => {
            if delta == 0 {
                false
            } else {
                profile.replay.auto_save = !profile.replay.auto_save;
                true
            }
        }
        SettingsEntryId::ReplaySlot1Rule => {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[0], delta)
        }
        SettingsEntryId::ReplaySlot2Rule => {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[1], delta)
        }
        SettingsEntryId::ReplaySlot3Rule => {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[2], delta)
        }
        SettingsEntryId::ReplaySlot4Rule => {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[3], delta)
        }
    }
}

fn adjust_u32(value: &mut u32, delta: i32, min: u32, max: u32) -> bool {
    let before = *value;
    let next = (*value as i32).saturating_add(delta).clamp(min as i32, max as i32) as u32;
    *value = next;
    *value != before
}

fn adjust_offset_ms(value: &mut i64, delta: i32) -> bool {
    let before = *value;
    let ms = (*value / 1_000).saturating_add(delta as i64).clamp(-500, 500);
    *value = ms * 1_000;
    *value != before
}

fn adjust_hispeed(value: &mut f32, delta: i32, step: f32, default_step: f32) -> bool {
    let before = *value;
    let step = normalize_hispeed_step(step, default_step);
    *value = (*value + step * delta.signum() as f32).clamp(0.5, 10.0);
    (*value - before).abs() > f32::EPSILON
}

fn adjust_hispeed_step(value: &mut f32, delta: i32) -> bool {
    let before = *value;
    let current = normalize_hispeed_step(*value, 0.25);
    let next = ((current / 0.05).round() as i32 + delta)
        .clamp((HISPEED_STEP_MIN / 0.05).round() as i32, (HISPEED_STEP_MAX / 0.05).round() as i32);
    *value = next as f32 / 20.0;
    (*value - before).abs() > f32::EPSILON
}

fn adjust_f32_tenths(value: &mut f32, delta: i32, min: f32, max: f32) -> bool {
    let before = *value;
    let next = ((*value * 10.0).round() as i32 + delta)
        .clamp((min * 10.0).round() as i32, (max * 10.0).round() as i32);
    *value = next as f32 / 10.0;
    (*value - before).abs() > f32::EPSILON
}

fn adjust_replay_slot_rule(value: &mut ReplaySlotRule, delta: i32) -> bool {
    let forward = delta >= 0;
    let steps = delta.unsigned_abs().max(1) as usize;
    let mut next = *value;
    for _ in 0..steps {
        next = next.cycle(forward);
    }
    if next == *value {
        return false;
    }
    *value = next;
    true
}

fn format_lane_unit(value: u32) -> String {
    format!("{}", value.min(1000))
}

fn format_bool_on_off(value: bool) -> String {
    if value { "ON".to_string() } else { "OFF".to_string() }
}

fn format_gauge(value: GaugeTypeConfig) -> String {
    match value {
        GaugeTypeConfig::AssistEasy => "ASSIST EASY".to_string(),
        GaugeTypeConfig::Easy => "EASY".to_string(),
        GaugeTypeConfig::Normal => "NORMAL".to_string(),
        GaugeTypeConfig::Hard => "HARD".to_string(),
        GaugeTypeConfig::ExHard => "EX HARD".to_string(),
        GaugeTypeConfig::AutoShift => "AUTO SHIFT".to_string(),
        GaugeTypeConfig::Hazard => "HAZARD".to_string(),
    }
}

fn format_rule_mode(value: RuleMode) -> String {
    match value {
        RuleMode::Beatoraja => "BEATORAJA".to_string(),
        RuleMode::Lr2Oraja => "LR2ORAJA".to_string(),
        RuleMode::Dx => "DX".to_string(),
    }
}

fn format_gauge_auto_shift(value: GaugeAutoShiftConfig) -> String {
    match value {
        GaugeAutoShiftConfig::Off => "OFF".to_string(),
        GaugeAutoShiftConfig::Continue => "CONTINUE".to_string(),
        GaugeAutoShiftConfig::HardToGroove => "HARD->GROOVE".to_string(),
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR".to_string(),
        GaugeAutoShiftConfig::SelectToUnder => "SELECT UNDER".to_string(),
    }
}

fn format_bottom_shiftable_gauge(value: BottomShiftableGaugeConfig) -> String {
    match value {
        BottomShiftableGaugeConfig::AssistEasy => "ASSIST EASY".to_string(),
        BottomShiftableGaugeConfig::Easy => "EASY".to_string(),
        BottomShiftableGaugeConfig::Normal => "NORMAL".to_string(),
    }
}

fn format_random(value: RandomOptionConfig) -> String {
    match value {
        RandomOptionConfig::Off => "OFF".to_string(),
        RandomOptionConfig::Mirror => "MIRROR".to_string(),
        RandomOptionConfig::Random => "RANDOM".to_string(),
        RandomOptionConfig::RRandom => "R-RANDOM".to_string(),
        RandomOptionConfig::SRandom => "S-RANDOM".to_string(),
        RandomOptionConfig::Spiral => "SPIRAL".to_string(),
        RandomOptionConfig::HRandom => "H-RANDOM".to_string(),
        RandomOptionConfig::AllScratch => "ALL-SCR".to_string(),
        RandomOptionConfig::RandomEx => "RANDOM-EX".to_string(),
        RandomOptionConfig::SRandomEx => "S-RANDOM-EX".to_string(),
        RandomOptionConfig::FRandom => "F-RANDOM".to_string(),
        RandomOptionConfig::MFRandom => "MF-RANDOM".to_string(),
    }
}

fn format_double_option(value: DoubleOptionConfig) -> String {
    match value {
        DoubleOptionConfig::Off => "OFF".to_string(),
        DoubleOptionConfig::Flip => "FLIP".to_string(),
        DoubleOptionConfig::Battle => "BATTLE".to_string(),
        DoubleOptionConfig::BattleAutoScratch => "BATTLE AS".to_string(),
    }
}

fn format_hs_fix(value: HsFixConfig) -> String {
    match value {
        HsFixConfig::Off => "OFF".to_string(),
        HsFixConfig::StartBpm => "START BPM".to_string(),
        HsFixConfig::MinBpm => "MIN BPM".to_string(),
        HsFixConfig::MaxBpm => "MAX BPM".to_string(),
        HsFixConfig::MainBpm => "MAIN BPM".to_string(),
    }
}

fn format_target(value: TargetOptionConfig) -> String {
    match value {
        TargetOptionConfig::None => "NONE".to_string(),
        TargetOptionConfig::RankA => "RANK_A".to_string(),
        TargetOptionConfig::RankAaMinus => "RANK_AA-".to_string(),
        TargetOptionConfig::RankAa => "RANK_AA".to_string(),
        TargetOptionConfig::RankAaaMinus => "RANK_AAA-".to_string(),
        TargetOptionConfig::RankAaa => "RANK_AAA".to_string(),
        TargetOptionConfig::RankMaxMinus => "RANK_MAX-".to_string(),
        TargetOptionConfig::Max => "MAX".to_string(),
        TargetOptionConfig::RankNext => "RANK_NEXT".to_string(),
        TargetOptionConfig::IrTop => "IR_TOP".to_string(),
        TargetOptionConfig::IrNext => "IR_NEXT".to_string(),
        TargetOptionConfig::RivalTop => "RIVAL TOP".to_string(),
        TargetOptionConfig::RivalNext => "RIVAL NEXT".to_string(),
        TargetOptionConfig::RivalIndex(index) => format!("RIVAL_{index}"),
    }
}

fn format_grade_diff_display(value: ResultGradeDiffDisplay) -> String {
    match value {
        ResultGradeDiffDisplay::Next => "NEXT".to_string(),
        ResultGradeDiffDisplay::Nearest => "NEAREST".to_string(),
    }
}

fn format_lane_effect(value: LaneEffectConfig) -> String {
    match value {
        LaneEffectConfig::Off => "OFF".to_string(),
        LaneEffectConfig::Hidden => "HIDDEN".to_string(),
        LaneEffectConfig::Sudden => "SUDDEN".to_string(),
        LaneEffectConfig::HiddenSudden => "HIDDEN+SUDDEN".to_string(),
    }
}

fn format_assist(value: AssistOptionConfig) -> String {
    match value {
        AssistOptionConfig::None => "NONE".to_string(),
        AssistOptionConfig::AutoScratch => "AUTO SCRATCH".to_string(),
        AssistOptionConfig::LegacyNote => "LEGACY NOTE".to_string(),
    }
}

fn format_bga_mode(value: BgaModeConfig) -> String {
    match value {
        BgaModeConfig::On => "ON".to_string(),
        BgaModeConfig::Auto => "AUTO".to_string(),
        BgaModeConfig::Off => "OFF".to_string(),
    }
}

fn format_bga_expand(value: BgaExpandConfig) -> String {
    match value {
        BgaExpandConfig::Full => "FULL".to_string(),
        BgaExpandConfig::KeepAspect => "KEEP ASPECT".to_string(),
        BgaExpandConfig::Off => "OFF".to_string(),
    }
}

fn format_judge_algorithm(value: JudgeAlgorithmConfig) -> String {
    match value {
        JudgeAlgorithmConfig::Combo => "COMBO".to_string(),
        JudgeAlgorithmConfig::Duration => "DURATION".to_string(),
        JudgeAlgorithmConfig::Lowest => "LOWEST".to_string(),
    }
}

fn format_hispeed_mode(value: HispeedModeConfig) -> String {
    match value {
        HispeedModeConfig::Normal => "NORMAL".to_string(),
        HispeedModeConfig::Floating => "FLOATING".to_string(),
    }
}

fn format_scratch_input_mode(value: ScratchInputMode) -> String {
    match value {
        ScratchInputMode::Normal => "NORMAL".to_string(),
        ScratchInputMode::AnyDirection => "ANY DIRECTION".to_string(),
    }
}

fn format_replay_slot_rule(value: ReplaySlotRule) -> String {
    match value {
        ReplaySlotRule::Disabled => "DISABLED".to_string(),
        ReplaySlotRule::Always => "ALWAYS".to_string(),
        ReplaySlotRule::ScoreUpdate => "SCORE UPDATE".to_string(),
        ReplaySlotRule::BpUpdate => "BP UPDATE".to_string(),
        ReplaySlotRule::MaxComboUpdate => "MAX COMBO UPDATE".to_string(),
        ReplaySlotRule::ClearUpdate => "CLEAR UPDATE".to_string(),
    }
}

fn cycle_enum<T: Copy + PartialEq>(delta: i32, current: T, cycle: fn(T, bool) -> T) -> Option<T> {
    if delta == 0 {
        return None;
    }
    let forward = delta > 0;
    Some(cycle(current, forward))
}

fn cycle_judge_algorithm(current: JudgeAlgorithmConfig, forward: bool) -> JudgeAlgorithmConfig {
    cycle_in_slice(&JudgeAlgorithmConfig::ORDER, current, forward)
}

fn cycle_rule_mode(current: RuleMode, forward: bool) -> RuleMode {
    const VALUES: [RuleMode; 3] = [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_ln_mode_policy(current: LnPolicySetting, forward: bool) -> LnPolicySetting {
    cycle_in_slice(&LnPolicySetting::ORDER, current, forward)
}

fn cycle_gauge(current: GaugeTypeConfig, forward: bool) -> GaugeTypeConfig {
    const VALUES: [GaugeTypeConfig; 7] = [
        GaugeTypeConfig::AssistEasy,
        GaugeTypeConfig::Easy,
        GaugeTypeConfig::Normal,
        GaugeTypeConfig::Hard,
        GaugeTypeConfig::ExHard,
        GaugeTypeConfig::Hazard,
        GaugeTypeConfig::AutoShift,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_gauge_auto_shift(current: GaugeAutoShiftConfig, forward: bool) -> GaugeAutoShiftConfig {
    const VALUES: [GaugeAutoShiftConfig; 5] = [
        GaugeAutoShiftConfig::Off,
        GaugeAutoShiftConfig::Continue,
        GaugeAutoShiftConfig::HardToGroove,
        GaugeAutoShiftConfig::BestClear,
        GaugeAutoShiftConfig::SelectToUnder,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_bottom_shiftable_gauge(
    current: BottomShiftableGaugeConfig,
    forward: bool,
) -> BottomShiftableGaugeConfig {
    const VALUES: [BottomShiftableGaugeConfig; 3] = [
        BottomShiftableGaugeConfig::AssistEasy,
        BottomShiftableGaugeConfig::Easy,
        BottomShiftableGaugeConfig::Normal,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_random(current: RandomOptionConfig, forward: bool) -> RandomOptionConfig {
    const VALUES: [RandomOptionConfig; 12] = [
        RandomOptionConfig::Off,
        RandomOptionConfig::Mirror,
        RandomOptionConfig::Random,
        RandomOptionConfig::RRandom,
        RandomOptionConfig::SRandom,
        RandomOptionConfig::Spiral,
        RandomOptionConfig::HRandom,
        RandomOptionConfig::AllScratch,
        RandomOptionConfig::RandomEx,
        RandomOptionConfig::SRandomEx,
        RandomOptionConfig::FRandom,
        RandomOptionConfig::MFRandom,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_double_option(current: DoubleOptionConfig, forward: bool) -> DoubleOptionConfig {
    const VALUES: [DoubleOptionConfig; 4] = [
        DoubleOptionConfig::Off,
        DoubleOptionConfig::Flip,
        DoubleOptionConfig::Battle,
        DoubleOptionConfig::BattleAutoScratch,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_hs_fix(current: HsFixConfig, forward: bool) -> HsFixConfig {
    const VALUES: [HsFixConfig; 5] = [
        HsFixConfig::Off,
        HsFixConfig::StartBpm,
        HsFixConfig::MaxBpm,
        HsFixConfig::MainBpm,
        HsFixConfig::MinBpm,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_target(current: TargetOptionConfig, forward: bool) -> TargetOptionConfig {
    const VALUES: [TargetOptionConfig; 13] = [
        TargetOptionConfig::None,
        TargetOptionConfig::RankA,
        TargetOptionConfig::RankAaMinus,
        TargetOptionConfig::RankAa,
        TargetOptionConfig::RankAaaMinus,
        TargetOptionConfig::RankAaa,
        TargetOptionConfig::RankMaxMinus,
        TargetOptionConfig::Max,
        TargetOptionConfig::RankNext,
        TargetOptionConfig::IrTop,
        TargetOptionConfig::IrNext,
        TargetOptionConfig::RivalTop,
        TargetOptionConfig::RivalNext,
    ];
    let current = if matches!(current, TargetOptionConfig::RivalIndex(_)) {
        TargetOptionConfig::None
    } else {
        current
    };
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_grade_diff_display(
    current: ResultGradeDiffDisplay,
    forward: bool,
) -> ResultGradeDiffDisplay {
    const VALUES: [ResultGradeDiffDisplay; 2] =
        [ResultGradeDiffDisplay::Nearest, ResultGradeDiffDisplay::Next];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_lane_effect(current: LaneEffectConfig, forward: bool) -> LaneEffectConfig {
    const VALUES: [LaneEffectConfig; 4] = [
        LaneEffectConfig::Off,
        LaneEffectConfig::Hidden,
        LaneEffectConfig::Sudden,
        LaneEffectConfig::HiddenSudden,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_assist(current: AssistOptionConfig, forward: bool) -> AssistOptionConfig {
    const VALUES: [AssistOptionConfig; 3] =
        [AssistOptionConfig::None, AssistOptionConfig::AutoScratch, AssistOptionConfig::LegacyNote];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_bga_mode(current: BgaModeConfig, forward: bool) -> BgaModeConfig {
    const VALUES: [BgaModeConfig; 3] = [BgaModeConfig::On, BgaModeConfig::Auto, BgaModeConfig::Off];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_bga_expand(current: BgaExpandConfig, forward: bool) -> BgaExpandConfig {
    const VALUES: [BgaExpandConfig; 3] =
        [BgaExpandConfig::KeepAspect, BgaExpandConfig::Full, BgaExpandConfig::Off];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_hispeed_mode(current: HispeedModeConfig, forward: bool) -> HispeedModeConfig {
    const VALUES: [HispeedModeConfig; 2] = [HispeedModeConfig::Normal, HispeedModeConfig::Floating];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_scratch_input_mode(current: ScratchInputMode, forward: bool) -> ScratchInputMode {
    const VALUES: [ScratchInputMode; 2] =
        [ScratchInputMode::Normal, ScratchInputMode::AnyDirection];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_select_input_mode(current: SelectInputModeConfig, forward: bool) -> SelectInputModeConfig {
    const VALUES: [SelectInputModeConfig; 2] =
        [SelectInputModeConfig::Key7Key14, SelectInputModeConfig::Key9];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_in_slice<T: Copy + PartialEq>(values: &[T], current: T, forward: bool) -> T {
    let index = values.iter().position(|value| *value == current).unwrap_or(0);
    if forward {
        values[(index + 1) % values.len()]
    } else {
        values[(index + values.len() - 1) % values.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::ProfileConfig;

    #[test]
    fn adjust_volume_clamps_to_range() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(profile.audio_mix.normalize_chart_volume);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::NormalizeChartVolume, 1));
        assert!(!profile.audio_mix.normalize_chart_volume);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::NormalizeChartVolume), "OFF");
        profile.audio_mix.master_volume = 98;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::MasterVolume, 5));
        assert_eq!(profile.audio_mix.master_volume, 100);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::MasterVolume, -200));
        assert_eq!(profile.audio_mix.master_volume, 0);
    }

    #[test]
    fn adjust_judge_offset_in_millisecond_steps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::InputOffsetMs, 3));
        assert_eq!(profile.judge.input_offset_us, 3_000);
    }

    #[test]
    fn visual_offset_auto_adjust_toggles() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(SettingsEntryId::JUDGE_ENTRIES.contains(&SettingsEntryId::VisualOffsetAutoAdjust));
        assert_eq!(format_settings_value(&profile, SettingsEntryId::VisualOffsetAutoAdjust), "OFF");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::VisualOffsetAutoAdjust, 1));
        assert!(profile.judge.visual_offset_auto_adjust);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::VisualOffsetAutoAdjust), "ON");
    }

    #[test]
    fn cycle_judge_algorithm_uses_beatoraja_order() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        assert_eq!(format_settings_value(&profile, SettingsEntryId::JudgeAlgorithm), "COMBO");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::JudgeAlgorithm, 1));
        assert_eq!(profile.judge.judge_algorithm, JudgeAlgorithmConfig::Duration);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::JudgeAlgorithm, 1));
        assert_eq!(profile.judge.judge_algorithm, JudgeAlgorithmConfig::Lowest);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::JudgeAlgorithm, 1));
        assert_eq!(profile.judge.judge_algorithm, JudgeAlgorithmConfig::Combo);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::JudgeAlgorithm), "COMBO");
    }

    #[test]
    fn adjust_hispeed_uses_mode_specific_steps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed, 1));
        assert!((profile.lane.hispeed - 2.25).abs() < f32::EPSILON);

        profile.lane.hispeed_mode = HispeedModeConfig::Floating;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed, 1));
        assert!((profile.lane.hispeed - 2.75).abs() < f32::EPSILON);
    }

    #[test]
    fn adjust_hispeed_step_settings_increments_by_five_hundredths() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::HispeedStepNhs), "0.25");
        assert_eq!(format_settings_value(&profile, SettingsEntryId::HispeedStepFhs), "0.50");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HispeedStepNhs, 1));
        assert!((profile.lane.hispeed_step_nhs - 0.30).abs() < f32::EPSILON);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HispeedStepFhs, -1));
        assert!((profile.lane.hispeed_step_fhs - 0.45).abs() < f32::EPSILON);
    }

    #[test]
    fn adjust_lane_cover_and_lift_keep_combined_range() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.lane.sudden = 900;
        profile.lane.lift = 200;

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Sudden, 1));
        assert_eq!(profile.lane.sudden, 800);

        profile.lane.sudden = 300;
        profile.lane.lift = 700;
        assert!(!adjust_settings_value(&mut profile, SettingsEntryId::Lift, 1));
        assert_eq!(profile.lane.lift, 700);
    }

    #[test]
    fn cycle_gauge_wraps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.play.gauge = GaugeTypeConfig::Hazard;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Gauge, 1));
        assert_eq!(profile.play.gauge, GaugeTypeConfig::AutoShift);
    }

    #[test]
    fn cycle_grade_diff_display_wraps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(SettingsEntryId::PLAY_ENTRIES.contains(&SettingsEntryId::GradeDiffDisplay));
        assert_eq!(format_settings_value(&profile, SettingsEntryId::GradeDiffDisplay), "NEAREST");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::GradeDiffDisplay, 1));
        assert_eq!(profile.play.grade_diff_display, ResultGradeDiffDisplay::Next);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::GradeDiffDisplay), "NEXT");
    }

    #[test]
    fn cycle_rule_mode_and_format_value() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RuleMode), "BEATORAJA");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RuleMode, 1));
        assert_eq!(profile.play.rule_mode, RuleMode::Lr2Oraja);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RuleMode), "LR2ORAJA");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RuleMode, 1));
        assert_eq!(profile.play.rule_mode, RuleMode::Dx);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RuleMode), "DX");
    }

    #[test]
    fn cycle_hs_fix_uses_beatoraja_order() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::HsFix), "OFF");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::StartBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MaxBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MainBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MinBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::Off);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, -1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MinBpm);
    }

    #[test]
    fn auto_play_toggles() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(!profile.play.auto_play);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AutoPlay, 1));
        assert!(profile.play.auto_play);
    }

    #[test]
    fn cycle_ln_mode_policy_and_hispeed_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::LnModePolicy), "AUTO(LN)");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::LnModePolicy, 1));
        assert_eq!(profile.play.ln_mode_policy, crate::ln_policy::LnPolicySetting::AutoCn);

        assert_eq!(format_settings_value(&profile, SettingsEntryId::HispeedMode), "NORMAL");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HispeedMode, 1));
        assert_eq!(
            profile.lane.hispeed_mode,
            crate::config::profile_config::HispeedModeConfig::Floating
        );
    }

    #[test]
    fn adjust_green_number_misslayer_and_analog_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.lane.target_green_number = 995;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::TargetGreenNumber, 10));
        assert_eq!(profile.lane.target_green_number, 999);

        profile.play.misslayer_duration_ms = 4_980;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::MisslayerDurationMs, 50));
        assert_eq!(profile.play.misslayer_duration_ms, 5_000);

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AnalogScratchSensitivity, 1));
        assert!((profile.input.analog_scratch_sensitivity - 1.1).abs() < f32::EPSILON);

        profile.input.analog_scratch_threshold = 995;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AnalogScratchThreshold, 10));
        assert_eq!(profile.input.analog_scratch_threshold, 1_000);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::AnalogScratchThreshold),
            "1000 ticks"
        );
    }

    #[test]
    fn cycle_input_and_replay_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::SelectInputMode), "7K/14K");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SelectInputMode, 1));
        assert_eq!(
            profile.input.select_input_mode,
            crate::config::profile_config::SelectInputModeConfig::Key9
        );

        assert_eq!(format_settings_value(&profile, SettingsEntryId::ScratchInputMode), "NORMAL");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ScratchInputMode, 1));
        assert_eq!(
            profile.input.scratch_mode,
            crate::config::profile_config::ScratchInputMode::AnyDirection
        );

        assert!(profile.replay.auto_save);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ReplayAutoSave, 1));
        assert!(!profile.replay.auto_save);

        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::ReplaySlot2Rule),
            "SCORE UPDATE"
        );
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ReplaySlot2Rule, 1));
        assert_eq!(
            profile.replay.slot_rules[1],
            crate::config::profile_config::ReplaySlotRule::BpUpdate
        );
    }
}
