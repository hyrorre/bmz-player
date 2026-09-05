use bmz_core::clear::GaugeType;
use bmz_core::lane::{KeyMode, Lane};
use bmz_gameplay::gauge::GaugeAutoShiftMode;
use bmz_gameplay::input::binding::LaneBinding;
use bmz_gameplay::input::bounce::InputBounceConfig;
use bmz_gameplay::judge::model::JudgeWindow;
use bmz_gameplay::session::{PlayAudioMix, PlayOffsets};

use super::play_input::{lane_binding_for_key_mode, lane_binding_for_key_mode_with_slots};
use super::profile_config::{
    BottomShiftableGaugeConfig, GaugeAutoShiftConfig, GaugeTypeConfig, LaneConfig, ProfileConfig,
    ProfileInputConfig,
};
use crate::input::gamepad::GamepadSlotMap;

pub const DEFAULT_JUDGE_WINDOW: JudgeWindow = JudgeWindow {
    pgreat_us: 20_000,
    great_us: 60_000,
    good_us: 150_000,
    bad_fast_us: 280_000,
    bad_slow_us: 220_000,
    empty_poor_fast_us: 150_000,
    empty_poor_slow_us: 500_000,
    mine_hit_us: 16_000,
};

pub const TARGET_GREEN_NUMBER_MIN: u32 = 1;
/// beatoraja duration upper bound (10,000ms) expressed as green number.
pub const TARGET_GREEN_NUMBER_MAX: u32 = 6_000;
pub const NORMAL_HISPEED_LEVEL_MIN: u8 = 1;
pub const NORMAL_HISPEED_LEVEL_MAX: u8 = 20;
pub const NORMAL_HISPEED_GREEN_NUMBERS: [u32; 20] = [
    1_200, 1_000, 800, 700, 650, 600, 550, 500, 480, 460, 440, 420, 400, 380, 360, 340, 320, 300,
    280, 260,
];

pub const fn normalize_normal_hispeed_level(level: u8) -> u8 {
    if level < NORMAL_HISPEED_LEVEL_MIN {
        NORMAL_HISPEED_LEVEL_MIN
    } else if level > NORMAL_HISPEED_LEVEL_MAX {
        NORMAL_HISPEED_LEVEL_MAX
    } else {
        level
    }
}

pub const fn normal_hispeed_green_number(level: u8) -> u32 {
    NORMAL_HISPEED_GREEN_NUMBERS[(normalize_normal_hispeed_level(level) - 1) as usize]
}

pub fn normal_hispeed_level_for_green_number(green_number: u32) -> u8 {
    if green_number >= NORMAL_HISPEED_GREEN_NUMBERS[0] {
        return NORMAL_HISPEED_LEVEL_MIN;
    }
    for (index, pair) in NORMAL_HISPEED_GREEN_NUMBERS.windows(2).enumerate() {
        let slower = pair[0];
        let faster = pair[1];
        if green_number < faster || green_number > slower {
            continue;
        }
        let boundary = if slower - faster == 50 && slower <= 700 && faster >= 500 {
            faster + 30
        } else {
            (slower + faster) / 2
        };
        return if green_number <= boundary { index as u8 + 2 } else { index as u8 + 1 };
    }
    NORMAL_HISPEED_LEVEL_MAX
}
pub const NOTE_DISPLAY_DURATION_MIN_MS: u32 = 1;
pub const NOTE_DISPLAY_DURATION_MAX_MS: u32 = 10_000;
pub const CONSTANT_FADE_MIN_MS: i32 = -1_000;
pub const CONSTANT_FADE_MAX_MS: i32 = 1_000;
/// beatoraja `PlayConfig.HISPEED_MIN` / `HISPEED_MAX` compatible range.
pub const HISPEED_MIN: f32 = 0.01;
pub const HISPEED_MAX: f32 = 20.0;

pub fn clamp_hispeed(hispeed: f32) -> f32 {
    hispeed.clamp(HISPEED_MIN, HISPEED_MAX)
}

pub const fn duration_ms_from_green_number(green_number: u32) -> u32 {
    green_number.saturating_mul(5).saturating_add(1) / 3
}

pub const fn green_number_from_duration_ms(duration_ms: u32) -> u32 {
    duration_ms.saturating_mul(3).saturating_add(2) / 5
}

pub fn adjust_green_number_by_duration_ms(green_number: u32, delta_ms: i32) -> u32 {
    let current = green_number.clamp(TARGET_GREEN_NUMBER_MIN, TARGET_GREEN_NUMBER_MAX);
    if delta_ms == 0 {
        return current;
    }
    let current_duration = duration_ms_from_green_number(current);
    let requested_duration = (i64::from(current_duration) + i64::from(delta_ms))
        .clamp(i64::from(NOTE_DISPLAY_DURATION_MIN_MS), i64::from(NOTE_DISPLAY_DURATION_MAX_MS))
        as u32;
    let converted = green_number_from_duration_ms(requested_duration)
        .clamp(TARGET_GREEN_NUMBER_MIN, TARGET_GREEN_NUMBER_MAX);
    if converted != current || requested_duration == current_duration {
        converted
    } else if delta_ms > 0 {
        current.saturating_add(1).min(TARGET_GREEN_NUMBER_MAX)
    } else {
        current.saturating_sub(1).max(TARGET_GREEN_NUMBER_MIN)
    }
}

pub fn play_offsets_from_profile(profile: &ProfileConfig) -> PlayOffsets {
    play_offsets_from_profile_for_mode(profile, profile.active_play_mode)
}

pub fn play_offsets_from_profile_for_mode(
    profile: &ProfileConfig,
    key_mode: KeyMode,
) -> PlayOffsets {
    PlayOffsets {
        input_offset_us: profile.judge.input_offset_us,
        visual_offset_us: profile.play_mode_config(key_mode).visual_offset_us,
    }
}

pub fn input_bounce_config_from_profile(input: &ProfileInputConfig) -> InputBounceConfig {
    InputBounceConfig {
        keyboard_threshold_us: u64::from(input.keyboard_release_bounce_ms) * 1_000,
        controller_threshold_us: u64::from(input.controller_release_bounce_ms) * 1_000,
    }
}

pub fn audio_mix_from_profile(profile: &ProfileConfig) -> PlayAudioMix {
    audio_mix_from_profile_with_chart_gain(profile, 1.0)
}

pub fn audio_mix_from_profile_with_chart_gain(
    profile: &ProfileConfig,
    chart_normalization_gain: f32,
) -> PlayAudioMix {
    PlayAudioMix {
        master_volume: volume_unit_to_f32(profile.audio_mix.master_volume),
        chart_normalization_gain,
        normalize_chart_volume: profile.audio_mix.normalize_chart_volume,
        key_volume: volume_unit_to_f32(profile.audio_mix.key_volume),
        bgm_volume: volume_unit_to_f32(profile.audio_mix.bgm_volume),
        auto_keysound: profile.audio_mix.auto_keysound,
        auto_keysound_fallback: profile.audio_mix.auto_keysound_fallback,
        auto_keysound_mine: profile.audio_mix.auto_keysound_mine,
    }
}

/// 譜面正規化の sample peak 判定より後段で掛かる最大出力倍率。
///
/// 解析キャッシュは BGM とキー音を unity で合成した指標を保持するため、プレイ時は
/// 大きい方のカテゴリ音量を代表値として使い、master volume と合わせて補正する。
pub fn chart_normalization_output_gain(profile: &ProfileConfig) -> f32 {
    let master = volume_unit_to_f32(profile.audio_mix.master_volume);
    let category =
        volume_unit_to_f32(profile.audio_mix.key_volume.max(profile.audio_mix.bgm_volume));
    master * category
}

/// profile.toml の 0..=100 整数ボリュームを 0.0..=1.0 の f32 に変換する。
pub fn volume_unit_to_f32(value: u32) -> f32 {
    (value.min(100) as f32) / 100.0
}

/// profile.toml の 0..=1000 整数 (sudden / lift / hidden) を 0.0..=1.0 の f32 に変換する。
pub fn lane_unit_to_f32(value: u32) -> f32 {
    (value.min(1000) as f32) / 1000.0
}

/// ランタイムの 0.0..=1.0 を 0..=1000 整数 (sudden / lift) に変換する。
pub fn lane_f32_to_unit(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * 1000.0).round() as u32
}

/// BMZ では SUDDEN+ と LIFT を lift=0 の判定ラインまでの絶対量として扱う。
pub fn visible_lane_fraction(lane_cover: f32, lift: f32) -> f32 {
    (1.0 - lane_cover.clamp(0.0, 1.0) - lift.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

pub fn lane_cover_max_for_lift(lift: f32) -> f32 {
    (1.0 - lift.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

pub fn clamp_lane_cover_for_lift(lane_cover: f32, lift: f32) -> f32 {
    lane_cover.clamp(0.0, lane_cover_max_for_lift(lift))
}

pub fn lane_unit_max_for_other(other: u32) -> u32 {
    1000_u32.saturating_sub(other.min(1000))
}

pub fn gauge_type_from_config(config: GaugeTypeConfig) -> GaugeType {
    match config {
        GaugeTypeConfig::AssistEasy => GaugeType::AssistEasy,
        GaugeTypeConfig::Easy => GaugeType::Easy,
        GaugeTypeConfig::Normal => GaugeType::Normal,
        GaugeTypeConfig::Hard => GaugeType::Hard,
        GaugeTypeConfig::ExHard | GaugeTypeConfig::AutoShift => GaugeType::ExHard,
        GaugeTypeConfig::Hazard => GaugeType::Hazard,
    }
}

pub fn gauge_auto_shift_from_config(
    gauge: GaugeTypeConfig,
    config: GaugeAutoShiftConfig,
) -> GaugeAutoShiftMode {
    if matches!(gauge, GaugeTypeConfig::AutoShift) {
        GaugeAutoShiftMode::BestClear
    } else {
        match config {
            GaugeAutoShiftConfig::Off => GaugeAutoShiftMode::Off,
            GaugeAutoShiftConfig::Continue => GaugeAutoShiftMode::Continue,
            GaugeAutoShiftConfig::HardToGroove => GaugeAutoShiftMode::HardToGroove,
            GaugeAutoShiftConfig::BestClear => GaugeAutoShiftMode::BestClear,
            GaugeAutoShiftConfig::SelectToUnder => GaugeAutoShiftMode::SelectToUnder,
        }
    }
}

pub fn bottom_shiftable_gauge_from_config(config: BottomShiftableGaugeConfig) -> GaugeType {
    match config {
        BottomShiftableGaugeConfig::AssistEasy => GaugeType::AssistEasy,
        BottomShiftableGaugeConfig::Easy => GaugeType::Easy,
        BottomShiftableGaugeConfig::Normal => GaugeType::Normal,
    }
}

pub fn lane_from_config(config: LaneConfig) -> Lane {
    match config {
        LaneConfig::Scratch => Lane::Scratch,
        LaneConfig::Key1 => Lane::Key1,
        LaneConfig::Key2 => Lane::Key2,
        LaneConfig::Key3 => Lane::Key3,
        LaneConfig::Key4 => Lane::Key4,
        LaneConfig::Key5 => Lane::Key5,
        LaneConfig::Key6 => Lane::Key6,
        LaneConfig::Key7 => Lane::Key7,
        LaneConfig::Scratch2 => Lane::Scratch2,
        LaneConfig::Key8 => Lane::Key8,
        LaneConfig::Key9 => Lane::Key9,
        LaneConfig::Key10 => Lane::Key10,
        LaneConfig::Key11 => Lane::Key11,
        LaneConfig::Key12 => Lane::Key12,
        LaneConfig::Key13 => Lane::Key13,
        LaneConfig::Key14 => Lane::Key14,
    }
}

pub fn lane_to_config(lane: Lane) -> LaneConfig {
    match lane {
        Lane::Scratch => LaneConfig::Scratch,
        Lane::Key1 => LaneConfig::Key1,
        Lane::Key2 => LaneConfig::Key2,
        Lane::Key3 => LaneConfig::Key3,
        Lane::Key4 => LaneConfig::Key4,
        Lane::Key5 => LaneConfig::Key5,
        Lane::Key6 => LaneConfig::Key6,
        Lane::Key7 => LaneConfig::Key7,
        Lane::Scratch2 => LaneConfig::Scratch2,
        Lane::Key8 => LaneConfig::Key8,
        Lane::Key9 => LaneConfig::Key9,
        Lane::Key10 => LaneConfig::Key10,
        Lane::Key11 => LaneConfig::Key11,
        Lane::Key12 => LaneConfig::Key12,
        Lane::Key13 => LaneConfig::Key13,
        Lane::Key14 => LaneConfig::Key14,
    }
}

pub fn lane_binding_from_profile_input(input: &ProfileInputConfig) -> LaneBinding {
    lane_binding_for_key_mode(input, KeyMode::K7)
        .unwrap_or_else(|_| LaneBinding { entries: Vec::new() })
}

pub fn lane_binding_for_chart(input: &ProfileInputConfig, key_mode: KeyMode) -> LaneBinding {
    lane_binding_for_chart_with_slots(input, key_mode, GamepadSlotMap::default())
}

pub fn lane_binding_for_chart_with_slots(
    input: &ProfileInputConfig,
    key_mode: KeyMode,
    slots: GamepadSlotMap,
) -> LaneBinding {
    lane_binding_for_key_mode_with_slots(input, key_mode, slots)
        .unwrap_or_else(|_| LaneBinding { entries: Vec::new() })
}

#[cfg(test)]
mod tests {
    use bmz_gameplay::input::backend::PhysicalControl;

    use super::*;
    use crate::config::profile_config::ProfileConfig;

    #[test]
    fn maps_profile_offsets() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.judge.input_offset_us = -1_000;
        profile.judge.visual_offset_us = 2_000;

        let offsets = play_offsets_from_profile(&profile);

        assert_eq!(offsets.input_offset_us, -1_000);
        assert_eq!(offsets.visual_offset_us, 2_000);
    }

    #[test]
    fn green_number_is_the_canonical_note_duration_value() {
        for green_number in TARGET_GREEN_NUMBER_MIN..=TARGET_GREEN_NUMBER_MAX {
            assert_eq!(
                green_number_from_duration_ms(duration_ms_from_green_number(green_number)),
                green_number
            );
        }
    }

    #[test]
    fn duration_adjustment_moves_to_the_next_representable_green_number() {
        assert_eq!(adjust_green_number_by_duration_ms(2, 1), 3);
        assert_eq!(adjust_green_number_by_duration_ms(2, -1), 1);
        assert_eq!(adjust_green_number_by_duration_ms(300, 10), 306);
        assert_eq!(adjust_green_number_by_duration_ms(TARGET_GREEN_NUMBER_MAX, 1), 6_000);
        assert_eq!(adjust_green_number_by_duration_ms(TARGET_GREEN_NUMBER_MIN, -1), 1);
    }

    #[test]
    fn normal_hispeed_levels_use_the_fixed_green_number_table() {
        for (index, green_number) in NORMAL_HISPEED_GREEN_NUMBERS.iter().copied().enumerate() {
            let level = index as u8 + 1;
            assert_eq!(normal_hispeed_green_number(level), green_number);
            assert_eq!(normal_hispeed_level_for_green_number(green_number), level);
        }
        assert_eq!(normal_hispeed_green_number(0), 1_200);
        assert_eq!(normal_hispeed_green_number(21), 260);
        assert_eq!(normal_hispeed_level_for_green_number(1_500), 1);
        assert_eq!(normal_hispeed_level_for_green_number(100), 20);
    }

    #[test]
    fn normal_hispeed_rounding_uses_faster_side_for_ordinary_ties() {
        assert_eq!(normal_hispeed_level_for_green_number(371), 14);
        assert_eq!(normal_hispeed_level_for_green_number(370), 15);
        assert_eq!(normal_hispeed_level_for_green_number(369), 15);
    }

    #[test]
    fn normal_hispeed_rounding_uses_thirty_point_boundary_for_wide_middle_steps() {
        assert_eq!(normal_hispeed_level_for_green_number(581), 6);
        assert_eq!(normal_hispeed_level_for_green_number(580), 7);
        assert_eq!(normal_hispeed_level_for_green_number(579), 7);
        assert_eq!(normal_hispeed_level_for_green_number(681), 4);
        assert_eq!(normal_hispeed_level_for_green_number(680), 5);
    }

    #[test]
    fn maps_profile_audio_mix() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.audio_mix.master_volume = 80;
        profile.audio_mix.key_volume = 70;
        profile.audio_mix.bgm_volume = 60;

        let mix = audio_mix_from_profile(&profile);

        assert!((mix.master_volume - 0.8).abs() < 1e-6);
        assert!((mix.chart_normalization_gain - 1.0).abs() < 1e-6);
        assert!(mix.normalize_chart_volume);
        assert!((mix.key_volume - 0.7).abs() < 1e-6);
        assert!((mix.bgm_volume - 0.6).abs() < 1e-6);
        assert!((chart_normalization_output_gain(&profile) - 0.56).abs() < 1e-6);
    }

    #[test]
    fn profile_audio_mix_toggle_preserves_chart_gain() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        let enabled = audio_mix_from_profile_with_chart_gain(&profile, 0.25);
        assert_eq!(enabled.chart_normalization_gain, 0.25);
        assert_eq!(enabled.effective_normalization_gain(), 0.25);

        profile.audio_mix.normalize_chart_volume = false;
        let disabled = audio_mix_from_profile_with_chart_gain(&profile, 0.25);
        assert_eq!(disabled.chart_normalization_gain, 0.25);
        assert_eq!(disabled.effective_normalization_gain(), 1.0);
    }

    #[test]
    fn lane_cover_and_lift_share_absolute_lane_range() {
        assert!((visible_lane_fraction(0.3, 0.2) - 0.5).abs() < f32::EPSILON);
        assert!((lane_cover_max_for_lift(0.2) - 0.8).abs() < f32::EPSILON);
        assert!((clamp_lane_cover_for_lift(0.9, 0.2) - 0.8).abs() < f32::EPSILON);
        assert_eq!(lane_unit_max_for_other(200), 800);
        assert_eq!(lane_unit_max_for_other(1_500), 0);
    }

    #[test]
    fn maps_profile_enums_to_runtime_types() {
        assert_eq!(gauge_type_from_config(GaugeTypeConfig::Hard), GaugeType::Hard);
        assert_eq!(gauge_type_from_config(GaugeTypeConfig::AutoShift), GaugeType::ExHard);
        assert_eq!(
            gauge_auto_shift_from_config(GaugeTypeConfig::AutoShift, GaugeAutoShiftConfig::Off),
            GaugeAutoShiftMode::BestClear
        );
        assert_eq!(
            gauge_auto_shift_from_config(GaugeTypeConfig::ExHard, GaugeAutoShiftConfig::Off),
            GaugeAutoShiftMode::Off
        );
        assert_eq!(lane_from_config(LaneConfig::Key7), Lane::Key7);
    }

    #[test]
    fn maps_profile_input_bindings_to_lane_binding() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let binding = lane_binding_from_profile_input(&profile.input);

        // キーボード 9 (スクラッチ Up/Down + 鍵盤 ×7) + ゲームパッド 9 = 18
        assert_eq!(binding.entries.len(), 18);
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::KeyboardKey("LShift".to_string())
                && entry.scratch_direction == Some(bmz_core::input::ScratchDirection::Up)
        }));
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::KeyboardKey("LControl".to_string())
                && entry.scratch_direction == Some(bmz_core::input::ScratchDirection::Down)
        }));
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::GamepadButton("Axis1+".to_string())
                && entry.scratch_direction == Some(bmz_core::input::ScratchDirection::Up)
        }));
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::GamepadButton("Axis1-".to_string())
                && entry.scratch_direction == Some(bmz_core::input::ScratchDirection::Down)
        }));
    }
}
