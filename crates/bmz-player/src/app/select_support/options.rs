pub(in crate::app) fn cycle_gauge_option(current: GaugeTypeConfig) -> GaugeTypeConfig {
    match current {
        GaugeTypeConfig::AssistEasy => GaugeTypeConfig::Easy,
        GaugeTypeConfig::Easy => GaugeTypeConfig::Normal,
        GaugeTypeConfig::Normal => GaugeTypeConfig::Hard,
        GaugeTypeConfig::Hard => GaugeTypeConfig::ExHard,
        GaugeTypeConfig::ExHard | GaugeTypeConfig::AutoShift => GaugeTypeConfig::Hazard,
        GaugeTypeConfig::Hazard => GaugeTypeConfig::AssistEasy,
    }
}

pub(in crate::app) fn cycle_gauge_option_prev(current: GaugeTypeConfig) -> GaugeTypeConfig {
    cycle_gauge_option_with_direction(current, -1)
}

pub(in crate::app) fn cycle_gauge_option_with_direction(
    current: GaugeTypeConfig,
    direction: i32,
) -> GaugeTypeConfig {
    const VALUES: [GaugeTypeConfig; 6] = [
        GaugeTypeConfig::AssistEasy,
        GaugeTypeConfig::Easy,
        GaugeTypeConfig::Normal,
        GaugeTypeConfig::Hard,
        GaugeTypeConfig::ExHard,
        GaugeTypeConfig::Hazard,
    ];
    cycle_enum(VALUES, normalize_gauge_option(current), direction)
}

pub(in crate::app) fn normalize_gauge_option(current: GaugeTypeConfig) -> GaugeTypeConfig {
    match current {
        GaugeTypeConfig::AutoShift => GaugeTypeConfig::ExHard,
        _ => current,
    }
}

pub(in crate::app) fn gauge_option_as_str(gauge: GaugeTypeConfig) -> &'static str {
    match gauge {
        GaugeTypeConfig::AssistEasy => "A-EASY",
        GaugeTypeConfig::Easy => "EASY",
        GaugeTypeConfig::Normal => "NORMAL",
        GaugeTypeConfig::Hard => "HARD",
        GaugeTypeConfig::ExHard => "EX-HARD",
        GaugeTypeConfig::AutoShift => "EX-HARD",
        GaugeTypeConfig::Hazard => "HAZARD",
    }
}

pub(in crate::app) fn cycle_gauge_auto_shift_option(
    current: GaugeAutoShiftConfig,
) -> GaugeAutoShiftConfig {
    match current {
        GaugeAutoShiftConfig::Off => GaugeAutoShiftConfig::Continue,
        GaugeAutoShiftConfig::Continue => GaugeAutoShiftConfig::HardToGroove,
        GaugeAutoShiftConfig::HardToGroove => GaugeAutoShiftConfig::BestClear,
        GaugeAutoShiftConfig::BestClear => GaugeAutoShiftConfig::SelectToUnder,
        GaugeAutoShiftConfig::SelectToUnder => GaugeAutoShiftConfig::Off,
    }
}

pub(in crate::app) fn cycle_gauge_auto_shift_option_with_direction(
    current: GaugeAutoShiftConfig,
    direction: i32,
) -> GaugeAutoShiftConfig {
    const VALUES: [GaugeAutoShiftConfig; 5] = [
        GaugeAutoShiftConfig::Off,
        GaugeAutoShiftConfig::Continue,
        GaugeAutoShiftConfig::HardToGroove,
        GaugeAutoShiftConfig::BestClear,
        GaugeAutoShiftConfig::SelectToUnder,
    ];
    cycle_enum(VALUES, current, direction)
}

pub(in crate::app) fn gauge_auto_shift_as_str(mode: GaugeAutoShiftConfig) -> &'static str {
    match mode {
        GaugeAutoShiftConfig::Off => "OFF",
        GaugeAutoShiftConfig::Continue => "CONTINUE",
        GaugeAutoShiftConfig::HardToGroove => "HARD TO GROOVE",
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR",
        GaugeAutoShiftConfig::SelectToUnder => "SELECT TO UNDER",
    }
}

#[cfg(test)]
pub(in crate::app) fn cycle_bottom_shiftable_gauge_with_direction(
    current: BottomShiftableGaugeConfig,
    direction: i32,
) -> BottomShiftableGaugeConfig {
    const VALUES: [BottomShiftableGaugeConfig; 3] = [
        BottomShiftableGaugeConfig::AssistEasy,
        BottomShiftableGaugeConfig::Easy,
        BottomShiftableGaugeConfig::Normal,
    ];
    cycle_enum(VALUES, current, direction)
}

pub(in crate::app) fn cycle_judge_algorithm_with_direction(
    current: JudgeAlgorithmConfig,
    direction: i32,
) -> JudgeAlgorithmConfig {
    cycle_enum(JudgeAlgorithmConfig::ORDER, current, direction)
}

pub(in crate::app) fn bottom_shiftable_gauge_as_str(
    gauge: BottomShiftableGaugeConfig,
) -> &'static str {
    match gauge {
        BottomShiftableGaugeConfig::AssistEasy => "A-EASY",
        BottomShiftableGaugeConfig::Easy => "EASY",
        BottomShiftableGaugeConfig::Normal => "NORMAL",
    }
}

pub(in crate::app) fn bga_mode_as_str(bga: BgaModeConfig) -> &'static str {
    match bga {
        BgaModeConfig::On => "ON",
        BgaModeConfig::Auto => "AUTO",
        BgaModeConfig::Off => "OFF",
    }
}

pub(in crate::app) fn volume_f32_to_unit(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * 100.0).round() as u32
}

pub(in crate::app) fn cycle_arrange_option_with_direction(
    current: ArrangeOption,
    direction: i32,
) -> ArrangeOption {
    cycle_enum(ArrangeOption::VALUES, current, direction)
}

pub(in crate::app) fn cycle_double_option_with_direction(
    current: DoubleOption,
    direction: i32,
) -> DoubleOption {
    const VALUES: [DoubleOption; 4] = [
        DoubleOption::Off,
        DoubleOption::Flip,
        DoubleOption::Battle,
        DoubleOption::BattleAutoScratch,
    ];
    cycle_enum(VALUES, current, direction)
}

pub(in crate::app) fn cycle_hs_fix_option_with_direction(
    current: HsFixOption,
    direction: i32,
) -> HsFixOption {
    const VALUES: [HsFixOption; 5] = [
        HsFixOption::Off,
        HsFixOption::StartBpm,
        HsFixOption::MaxBpm,
        HsFixOption::MainBpm,
        HsFixOption::MinBpm,
    ];
    cycle_enum(VALUES, current, direction)
}

pub(in crate::app) fn cycle_bga_option(current: BgaModeConfig) -> BgaModeConfig {
    match current {
        BgaModeConfig::On => BgaModeConfig::Auto,
        BgaModeConfig::Auto => BgaModeConfig::Off,
        BgaModeConfig::Off => BgaModeConfig::On,
    }
}

pub(in crate::app) fn cycle_result_gauge_graph_type(current: i32) -> i32 {
    if (GaugeType::AssistEasy as i32..=GaugeType::Hazard as i32).contains(&current) {
        (current + 1).rem_euclid(6)
    } else {
        (current - 5).rem_euclid(3) + 6
    }
}

pub(in crate::app) fn toggled_select_sudden(current: LaneEffectConfig) -> LaneEffectConfig {
    match current {
        LaneEffectConfig::Off => LaneEffectConfig::Sudden,
        LaneEffectConfig::Hidden => LaneEffectConfig::HiddenSudden,
        LaneEffectConfig::Sudden => LaneEffectConfig::Off,
        LaneEffectConfig::HiddenSudden => LaneEffectConfig::Hidden,
    }
}

pub(in crate::app) fn toggled_select_hidden(current: LaneEffectConfig) -> LaneEffectConfig {
    match current {
        LaneEffectConfig::Off => LaneEffectConfig::Hidden,
        LaneEffectConfig::Hidden => LaneEffectConfig::Off,
        LaneEffectConfig::Sudden => LaneEffectConfig::HiddenSudden,
        LaneEffectConfig::HiddenSudden => LaneEffectConfig::Sudden,
    }
}
use super::*;
