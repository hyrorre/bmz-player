use super::*;

/// What an LR2 select `SRC_BUTTON` id changes in BMZ.
///
/// LR2 options with a BMZ counterpart cycle the profile value directly. LR2 options
/// without one are [`Lr2SelectButtonAction::Unsupported`] and never open a BMZ window;
/// BMZ-only settings stay in the BMZ settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum Lr2SelectButtonAction {
    /// Opens LR2 option panel 1..6, or closes it when already open.
    TogglePanel(u8),
    DifficultyTab(usize),
    /// Same meaning as the beatoraja select event with this id.
    SkinEvent(i32),
    KeyFilter,
    Sort,
    Gauge,
    Arrange1P,
    Arrange2P,
    LaneEffect,
    DpFlip,
    HsFix,
    JudgeAutoAdjust,
    Target,
    /// Embedded skin settings: skin type to edit, skin change, customize row.
    SkinTarget(i32),
    SkinChange,
    SkinItem(usize),
    Unsupported,
}

pub(in crate::app) fn lr2_select_button_action(button: i32) -> Lr2SelectButtonAction {
    use Lr2SelectButtonAction::*;
    match button {
        // LR2 skin timers cover panels 1..6 (21..26 / 31..36).
        1..=6 => TogglePanel(button as u8),
        91..=96 => DifficultyTab((button - 91) as usize),
        10 | 13..=17 | 19 | 57 | 72 | 74 => SkinEvent(button),
        11 => KeyFilter,
        12 => Sort,
        40 => Gauge,
        42 => Arrange1P,
        43 => Arrange2P,
        50 | 51 => LaneEffect,
        54 => DpFlip,
        55 => HsFix,
        75 => JudgeAutoAdjust,
        77 => Target,
        170..=185 => SkinTarget(button),
        190 => SkinChange,
        220..=229 => SkinItem((button - 220) as usize),
        // Panels 7..8, FX/EQ/PITCH, 2P gauge/HS, AUTO SCRATCH, shutter type,
        // BATTLE, score graph, ghost, BGA size, default target rate, system and
        // favorite tag buttons have no identical BMZ setting.
        _ => Unsupported,
    }
}

/// LR2 buttons react only to the left mouse button; the skin decides +/- by its
/// plus/minus type or by the clicked half. Other skin events keep beatoraja's rules.
pub(in crate::app) fn select_skin_click_allowed(event_id: i32, button: MouseButton) -> bool {
    !(bmz_render::skin::LR2_BUTTON_BASE..=bmz_render::skin::LR2_BUTTON_LAST).contains(&event_id)
        || button == MouseButton::Left
}

/// LR2 advances the difficulty, key and sort filters regardless of the clicked half.
pub(in crate::app) fn lr2_select_button_direction(button: i32, direction: i32) -> i32 {
    if (10..=12).contains(&button) || direction >= 0 { 1 } else { -1 }
}

/// LR2 values in LR2 order. Values LR2 lacks are only reachable from BMZ settings.
pub(in crate::app) const LR2_KEY_FILTERS: [SelectModeFilter; 6] = [
    SelectModeFilter::All,
    SelectModeFilter::K7,
    SelectModeFilter::K5,
    SelectModeFilter::K14,
    SelectModeFilter::K10,
    SelectModeFilter::K9,
];
pub(in crate::app) const LR2_SORTS: [SelectSort; 4] =
    [SelectSort::Level, SelectSort::Title, SelectSort::Clear, SelectSort::Score];
pub(in crate::app) const LR2_GAUGES: [GaugeTypeConfig; 4] = [
    GaugeTypeConfig::Normal,
    GaugeTypeConfig::Hard,
    GaugeTypeConfig::Hazard,
    GaugeTypeConfig::Easy,
];
pub(in crate::app) const LR2_ARRANGES: [ArrangeOption; 6] = [
    ArrangeOption::Normal,
    ArrangeOption::Mirror,
    ArrangeOption::Random,
    ArrangeOption::SRandom,
    ArrangeOption::HRandom,
    ArrangeOption::AllScratch,
];
pub(in crate::app) const LR2_LANE_EFFECTS: [LaneEffectConfig; 4] = [
    LaneEffectConfig::Off,
    LaneEffectConfig::Hidden,
    LaneEffectConfig::Sudden,
    LaneEffectConfig::HiddenSudden,
];
pub(in crate::app) const LR2_DP_OPTIONS: [DoubleOption; 2] =
    [DoubleOption::Off, DoubleOption::Flip];
pub(in crate::app) const LR2_HS_FIXES: [HsFixOption; 4] =
    [HsFixOption::Off, HsFixOption::MaxBpm, HsFixOption::MinBpm, HsFixOption::MainBpm];
pub(in crate::app) const LR2_TARGETS: [TargetOption; 6] = [
    TargetOption::None,
    TargetOption::RankAaa,
    TargetOption::RankAa,
    TargetOption::RankA,
    TargetOption::IrTop,
    TargetOption::IrNext,
];

/// Steps through LR2's values. A BMZ-only current value enters the LR2 list at
/// its first value going forward, or at its last value going backward.
pub(in crate::app) fn cycle_lr2_value<T: Copy + PartialEq, const N: usize>(
    values: [T; N],
    current: T,
    direction: i32,
) -> T {
    if values.contains(&current) {
        cycle_enum(values, current, direction)
    } else if direction >= 0 {
        values[0]
    } else {
        values[N - 1]
    }
}
