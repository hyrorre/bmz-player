use bmz_core::clear::GaugeType;
use bmz_core::lane::{KeyMode, Lane};
use bmz_gameplay::gauge::GaugeAutoShiftMode;
use bmz_gameplay::input::binding::LaneBinding;
use bmz_gameplay::judge::model::JudgeWindow;
use bmz_gameplay::session::{PlayAudioMix, PlayOffsets};

use super::play_input::lane_binding_for_key_mode;
use super::profile_config::{
    BottomShiftableGaugeConfig, GaugeAutoShiftConfig, GaugeTypeConfig, LaneConfig, ProfileConfig,
    ProfileInputConfig,
};

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

pub fn play_offsets_from_profile(profile: &ProfileConfig) -> PlayOffsets {
    PlayOffsets {
        input_offset_us: profile.judge.input_offset_us,
        visual_offset_us: profile.judge.visual_offset_us,
    }
}

pub fn audio_mix_from_profile(profile: &ProfileConfig) -> PlayAudioMix {
    PlayAudioMix {
        master_volume: volume_unit_to_f32(profile.audio_mix.master_volume),
        key_volume: volume_unit_to_f32(profile.audio_mix.key_volume),
        bgm_volume: volume_unit_to_f32(profile.audio_mix.bgm_volume),
    }
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
    lane_binding_for_key_mode(input, key_mode)
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
    fn maps_profile_audio_mix() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.audio_mix.master_volume = 80;
        profile.audio_mix.key_volume = 70;
        profile.audio_mix.bgm_volume = 60;

        let mix = audio_mix_from_profile(&profile);

        assert!((mix.master_volume - 0.8).abs() < 1e-6);
        assert!((mix.key_volume - 0.7).abs() < 1e-6);
        assert!((mix.bgm_volume - 0.6).abs() < 1e-6);
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
                && entry.control == PhysicalControl::GamepadButton("AxisLeftX+".to_string())
                && entry.scratch_direction == Some(bmz_core::input::ScratchDirection::Down)
        }));
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::GamepadButton("AxisLeftX-".to_string())
                && entry.scratch_direction == Some(bmz_core::input::ScratchDirection::Up)
        }));
    }
}
