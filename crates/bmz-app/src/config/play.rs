use bmz_core::clear::GaugeType;
use bmz_core::lane::Lane;
use bmz_gameplay::judge::model::JudgeWindow;
use bmz_gameplay::session::PlayOffsets;

use super::profile_config::{GaugeTypeConfig, LaneConfig, ProfileConfig};

pub const DEFAULT_JUDGE_WINDOW: JudgeWindow = JudgeWindow {
    pgreat_us: 16_000,
    great_us: 40_000,
    good_us: 80_000,
    bad_us: 120_000,
    empty_poor_fast_us: 500_000,
    empty_poor_slow_us: 200_000,
};

pub fn play_offsets_from_profile(profile: &ProfileConfig) -> PlayOffsets {
    PlayOffsets {
        input_offset_us: profile.judge.input_offset_us,
        visual_offset_us: profile.judge.visual_offset_us,
    }
}

pub fn gauge_type_from_config(config: GaugeTypeConfig) -> GaugeType {
    match config {
        GaugeTypeConfig::AssistEasy => GaugeType::AssistEasy,
        GaugeTypeConfig::Easy => GaugeType::Easy,
        GaugeTypeConfig::Normal => GaugeType::Normal,
        GaugeTypeConfig::Hard => GaugeType::Hard,
        GaugeTypeConfig::ExHard => GaugeType::ExHard,
        GaugeTypeConfig::Hazard => GaugeType::Hazard,
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
    fn maps_profile_enums_to_runtime_types() {
        assert_eq!(gauge_type_from_config(GaugeTypeConfig::Hard), GaugeType::Hard);
        assert_eq!(lane_from_config(LaneConfig::Key7), Lane::Key7);
    }
}
