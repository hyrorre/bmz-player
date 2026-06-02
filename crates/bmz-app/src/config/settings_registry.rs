use super::profile_config::{
    AssistOptionConfig, BgaExpandConfig, BgaModeConfig, GaugeAutoShiftConfig, GaugeTypeConfig,
    JudgeAlgorithmConfig, LaneEffectConfig, ProfileConfig, RandomOptionConfig, TargetOptionConfig,
};

/// ゲーム内設定で編集可能な profile.toml 項目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsEntryId {
    MasterVolume,
    KeyVolume,
    BgmVolume,
    PreviewVolume,
    SystemBgmVolume,
    SystemSeVolume,
    InputOffsetMs,
    VisualOffsetMs,
    JudgeAlgorithm,
    Gauge,
    GaugeAutoShift,
    Random,
    Target,
    LaneEffect,
    Assist,
    BgaMode,
    BgaExpand,
    AutoPlay,
    Hispeed,
    Sudden,
    Lift,
    Hidden,
}

impl SettingsEntryId {
    pub const VOLUME_ENTRIES: &'static [Self] = &[
        Self::MasterVolume,
        Self::KeyVolume,
        Self::BgmVolume,
        Self::PreviewVolume,
        Self::SystemBgmVolume,
        Self::SystemSeVolume,
    ];

    pub const JUDGE_ENTRIES: &'static [Self] =
        &[Self::InputOffsetMs, Self::VisualOffsetMs, Self::JudgeAlgorithm];

    pub const PLAY_ENTRIES: &'static [Self] = &[
        Self::Gauge,
        Self::GaugeAutoShift,
        Self::Random,
        Self::Target,
        Self::LaneEffect,
        Self::Assist,
        Self::BgaMode,
        Self::BgaExpand,
        Self::AutoPlay,
    ];

    pub const DISPLAY_ENTRIES: &'static [Self] =
        &[Self::Hispeed, Self::Sudden, Self::Lift, Self::Hidden];

    pub fn label(self) -> &'static str {
        match self {
            Self::MasterVolume => "MASTER",
            Self::KeyVolume => "KEY",
            Self::BgmVolume => "BGM",
            Self::PreviewVolume => "PREVIEW",
            Self::SystemBgmVolume => "SYS BGM",
            Self::SystemSeVolume => "SYS SE",
            Self::InputOffsetMs => "INPUT OFFSET",
            Self::VisualOffsetMs => "VISUAL OFFSET",
            Self::JudgeAlgorithm => "JUDGE ALGO",
            Self::Gauge => "GAUGE",
            Self::GaugeAutoShift => "GAUGE SHIFT",
            Self::Random => "RANDOM",
            Self::Target => "TARGET",
            Self::LaneEffect => "LANE FX",
            Self::Assist => "ASSIST",
            Self::BgaMode => "BGA",
            Self::BgaExpand => "BGA FIT",
            Self::AutoPlay => "AUTO PLAY",
            Self::Hispeed => "HISPEED",
            Self::Sudden => "SUDDEN+",
            Self::Lift => "LIFT",
            Self::Hidden => "HIDDEN",
        }
    }
}

/// 設定値 1 ステップの増減量。
pub fn settings_adjust_step(id: SettingsEntryId) -> i32 {
    match id {
        SettingsEntryId::InputOffsetMs | SettingsEntryId::VisualOffsetMs => 1,
        SettingsEntryId::Sudden | SettingsEntryId::Lift | SettingsEntryId::Hidden => 25,
        _ => 1,
    }
}

pub fn format_settings_value(profile: &ProfileConfig, id: SettingsEntryId) -> String {
    match id {
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
        SettingsEntryId::JudgeAlgorithm => format_judge_algorithm(profile.judge.judge_algorithm),
        SettingsEntryId::Gauge => format_gauge(profile.play.gauge),
        SettingsEntryId::GaugeAutoShift => format_gauge_auto_shift(profile.play.gauge_auto_shift),
        SettingsEntryId::Random => format_random(profile.play.random),
        SettingsEntryId::Target => format_target(profile.play.target),
        SettingsEntryId::LaneEffect => format_lane_effect(profile.play.lane_effect),
        SettingsEntryId::Assist => format_assist(profile.play.assist),
        SettingsEntryId::BgaMode => format_bga_mode(profile.play.bga),
        SettingsEntryId::BgaExpand => format_bga_expand(profile.play.bga_expand),
        SettingsEntryId::AutoPlay => format_bool_on_off(profile.play.auto_play),
        SettingsEntryId::Hispeed => format!("{:.2}", profile.lane.hispeed),
        SettingsEntryId::Sudden => format_lane_unit(profile.lane.sudden),
        SettingsEntryId::Lift => format_lane_unit(profile.lane.lift),
        SettingsEntryId::Hidden => format_lane_unit(profile.lane.hidden),
    }
}

/// 設定値を 1 ステップ変更する。変更があった場合 `true`。
pub fn adjust_settings_value(profile: &mut ProfileConfig, id: SettingsEntryId, delta: i32) -> bool {
    if delta == 0 {
        return false;
    }
    match id {
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
        SettingsEntryId::JudgeAlgorithm => {
            cycle_enum(delta, profile.judge.judge_algorithm, cycle_judge_algorithm)
                .map(|next| profile.judge.judge_algorithm = next)
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
        SettingsEntryId::Random => cycle_enum(delta, profile.play.random, cycle_random)
            .map(|next| profile.play.random = next)
            .is_some(),
        SettingsEntryId::Target => cycle_enum(delta, profile.play.target, cycle_target)
            .map(|next| profile.play.target = next)
            .is_some(),
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
        SettingsEntryId::Hispeed => adjust_hispeed(&mut profile.lane.hispeed, delta),
        SettingsEntryId::Sudden => adjust_u32(&mut profile.lane.sudden, delta, 0, 1000),
        SettingsEntryId::Lift => adjust_u32(&mut profile.lane.lift, delta, 0, 1000),
        SettingsEntryId::Hidden => adjust_u32(&mut profile.lane.hidden, delta, 0, 1000),
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

fn adjust_hispeed(value: &mut f32, delta: i32) -> bool {
    let before = *value;
    let step = 0.25 * delta.signum() as f32;
    *value = (*value + step).clamp(0.5, 10.0);
    (*value - before).abs() > f32::EPSILON
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

fn format_gauge_auto_shift(value: GaugeAutoShiftConfig) -> String {
    match value {
        GaugeAutoShiftConfig::Off => "OFF".to_string(),
        GaugeAutoShiftConfig::Continue => "CONTINUE".to_string(),
        GaugeAutoShiftConfig::HardToGroove => "HARD->GROOVE".to_string(),
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR".to_string(),
        GaugeAutoShiftConfig::SelectToUnder => "SELECT UNDER".to_string(),
    }
}

fn format_random(value: RandomOptionConfig) -> String {
    match value {
        RandomOptionConfig::Off => "OFF".to_string(),
        RandomOptionConfig::Mirror => "MIRROR".to_string(),
        RandomOptionConfig::Random => "RANDOM".to_string(),
        RandomOptionConfig::RRandom => "R-RANDOM".to_string(),
        RandomOptionConfig::SRandom => "S-RANDOM".to_string(),
    }
}

fn format_target(value: TargetOptionConfig) -> String {
    match value {
        TargetOptionConfig::None => "NONE".to_string(),
        TargetOptionConfig::Max => "MAX".to_string(),
        TargetOptionConfig::Aaa => "AAA".to_string(),
        TargetOptionConfig::Aa => "AA".to_string(),
        TargetOptionConfig::A => "A".to_string(),
        TargetOptionConfig::B => "B".to_string(),
        TargetOptionConfig::C => "C".to_string(),
        TargetOptionConfig::D => "D".to_string(),
        TargetOptionConfig::E => "E".to_string(),
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

fn cycle_enum<T: Copy + PartialEq>(delta: i32, current: T, cycle: fn(T, bool) -> T) -> Option<T> {
    if delta == 0 {
        return None;
    }
    let forward = delta > 0;
    Some(cycle(current, forward))
}

fn cycle_judge_algorithm(current: JudgeAlgorithmConfig, forward: bool) -> JudgeAlgorithmConfig {
    const VALUES: [JudgeAlgorithmConfig; 3] =
        [JudgeAlgorithmConfig::Combo, JudgeAlgorithmConfig::Duration, JudgeAlgorithmConfig::Lowest];
    cycle_in_slice(&VALUES, current, forward)
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

fn cycle_random(current: RandomOptionConfig, forward: bool) -> RandomOptionConfig {
    const VALUES: [RandomOptionConfig; 5] = [
        RandomOptionConfig::Off,
        RandomOptionConfig::Mirror,
        RandomOptionConfig::Random,
        RandomOptionConfig::RRandom,
        RandomOptionConfig::SRandom,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

fn cycle_target(current: TargetOptionConfig, forward: bool) -> TargetOptionConfig {
    const VALUES: [TargetOptionConfig; 9] = [
        TargetOptionConfig::None,
        TargetOptionConfig::Max,
        TargetOptionConfig::Aaa,
        TargetOptionConfig::Aa,
        TargetOptionConfig::A,
        TargetOptionConfig::B,
        TargetOptionConfig::C,
        TargetOptionConfig::D,
        TargetOptionConfig::E,
    ];
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
    fn adjust_hispeed_in_quarter_steps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed, 1));
        assert!((profile.lane.hispeed - 2.25).abs() < f32::EPSILON);
    }

    #[test]
    fn cycle_gauge_wraps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.play.gauge = GaugeTypeConfig::Hazard;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Gauge, 1));
        assert_eq!(profile.play.gauge, GaugeTypeConfig::AutoShift);
    }

    #[test]
    fn auto_play_toggles() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(!profile.play.auto_play);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AutoPlay, 1));
        assert!(profile.play.auto_play);
    }
}
