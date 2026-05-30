use bmz_core::clear::GaugeType;
use bmz_core::lane::Lane;
use bmz_gameplay::gauge::GaugeAutoShiftMode;
use bmz_gameplay::input::backend::PhysicalControl;
use bmz_gameplay::input::binding::{BindingEntry, LaneBinding};
use bmz_gameplay::judge::model::JudgeWindow;
use bmz_gameplay::session::{PlayAudioMix, PlayOffsets};

use super::profile_config::{
    GaugeAutoShiftConfig, GaugeTypeConfig, LaneConfig, ProfileConfig, ProfileInputConfig,
};

pub const DEFAULT_JUDGE_WINDOW: JudgeWindow = JudgeWindow {
    pgreat_us: 16_000,
    great_us: 40_000,
    good_us: 80_000,
    bad_us: 120_000,
    empty_poor_fast_us: 500_000,
    empty_poor_slow_us: 200_000,
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
        master_volume: profile.audio_mix.master_volume,
        key_volume: profile.audio_mix.key_volume,
        bgm_volume: profile.audio_mix.bgm_volume,
    }
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

pub fn lane_binding_from_profile_input(input: &ProfileInputConfig) -> LaneBinding {
    LaneBinding {
        entries: input
            .bindings
            .iter()
            .filter_map(|entry| {
                let lane = entry.lane?;
                Some(BindingEntry {
                    device: None,
                    control: control_from_config(&entry.device, &entry.control),
                    lane: lane_from_config(lane),
                })
            })
            .collect(),
    }
}

fn control_from_config(device: &str, control: &str) -> PhysicalControl {
    match device.to_ascii_lowercase().as_str() {
        "gamepad" => PhysicalControl::GamepadButton(control.to_string()),
        "hid" => control
            .parse::<u32>()
            .map(PhysicalControl::HidButton)
            .unwrap_or_else(|_| PhysicalControl::KeyboardKey(control.to_string())),
        _ => PhysicalControl::KeyboardKey(control.to_string()),
    }
}

#[cfg(test)]
mod tests {
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
        profile.audio_mix.master_volume = 0.8;
        profile.audio_mix.key_volume = 0.7;
        profile.audio_mix.bgm_volume = 0.6;

        let mix = audio_mix_from_profile(&profile);

        assert_eq!(mix.master_volume, 0.8);
        assert_eq!(mix.key_volume, 0.7);
        assert_eq!(mix.bgm_volume, 0.6);
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

        // キーボード 8 + ゲームパッド 9 (スクラッチ ×2 + 鍵盤 ×7) = 17
        assert_eq!(binding.entries.len(), 17);
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::KeyboardKey("LShift".to_string())
        }));
        assert!(binding.entries.iter().any(|entry| {
            entry.lane == Lane::Scratch
                && entry.control == PhysicalControl::GamepadButton("AxisLeftX+".to_string())
        }));
    }
}
