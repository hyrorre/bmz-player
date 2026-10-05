pub(in crate::app) fn arrange_option_from_profile(random: RandomOptionConfig) -> ArrangeOption {
    match random {
        RandomOptionConfig::Mirror => ArrangeOption::Mirror,
        RandomOptionConfig::Random => ArrangeOption::Random,
        RandomOptionConfig::RRandom => ArrangeOption::RRandom,
        RandomOptionConfig::SRandom => ArrangeOption::SRandom,
        RandomOptionConfig::Spiral => ArrangeOption::Spiral,
        RandomOptionConfig::HRandom => ArrangeOption::HRandom,
        RandomOptionConfig::AllScratch => ArrangeOption::AllScratch,
        RandomOptionConfig::RandomEx => ArrangeOption::RandomEx,
        RandomOptionConfig::SRandomEx => ArrangeOption::SRandomEx,
        RandomOptionConfig::FRandom => ArrangeOption::FRandom,
        RandomOptionConfig::MFRandom => ArrangeOption::MFRandom,
        RandomOptionConfig::Off => ArrangeOption::Normal,
    }
}

pub(in crate::app) fn random_config_from_arrange(arrange: ArrangeOption) -> RandomOptionConfig {
    match arrange {
        ArrangeOption::Normal => RandomOptionConfig::Off,
        ArrangeOption::Mirror => RandomOptionConfig::Mirror,
        ArrangeOption::Random => RandomOptionConfig::Random,
        ArrangeOption::RRandom => RandomOptionConfig::RRandom,
        ArrangeOption::SRandom => RandomOptionConfig::SRandom,
        ArrangeOption::Spiral => RandomOptionConfig::Spiral,
        ArrangeOption::HRandom => RandomOptionConfig::HRandom,
        ArrangeOption::AllScratch => RandomOptionConfig::AllScratch,
        ArrangeOption::RandomEx => RandomOptionConfig::RandomEx,
        ArrangeOption::SRandomEx => RandomOptionConfig::SRandomEx,
        ArrangeOption::FRandom => RandomOptionConfig::FRandom,
        ArrangeOption::MFRandom => RandomOptionConfig::MFRandom,
    }
}

pub(in crate::app) fn double_option_from_profile(
    double_option: DoubleOptionConfig,
) -> DoubleOption {
    match double_option {
        DoubleOptionConfig::Off => DoubleOption::Off,
        DoubleOptionConfig::Flip => DoubleOption::Flip,
        DoubleOptionConfig::Battle => DoubleOption::Battle,
        DoubleOptionConfig::BattleAutoScratch => DoubleOption::BattleAutoScratch,
    }
}

pub(in crate::app) fn double_config_from_option(double_option: DoubleOption) -> DoubleOptionConfig {
    match double_option {
        DoubleOption::Off => DoubleOptionConfig::Off,
        DoubleOption::Flip => DoubleOptionConfig::Flip,
        DoubleOption::Battle => DoubleOptionConfig::Battle,
        DoubleOption::BattleAutoScratch => DoubleOptionConfig::BattleAutoScratch,
    }
}

pub(in crate::app) fn play_skin_key_mode_for_options(
    chart_key_mode: KeyMode,
    double_option: DoubleOption,
    session_mode: SessionMode,
    key_mode_conversion: KeyModeConversionConfig,
    has_battle_target: bool,
) -> KeyMode {
    if session_mode.is_battle() || has_battle_target {
        return chart_key_mode;
    }
    let double_option = double_option.normalize_for_key_mode(chart_key_mode);
    match double_option {
        DoubleOption::Battle | DoubleOption::BattleAutoScratch => match chart_key_mode {
            KeyMode::K5 => KeyMode::K10,
            KeyMode::K7 => KeyMode::K14,
            _ => chart_key_mode,
        },
        DoubleOption::Off | DoubleOption::Flip
            if key_mode_conversion.applies_to(chart_key_mode) =>
        {
            key_mode_conversion.effective_key_mode(chart_key_mode)
        }
        DoubleOption::Off | DoubleOption::Flip => chart_key_mode,
    }
}

pub(in crate::app) const fn uses_battle_presentation(
    source_key_mode: KeyMode,
    rendered_key_mode: KeyMode,
    session_mode: SessionMode,
    has_battle_target: bool,
) -> bool {
    (session_mode.is_battle() || has_battle_target)
        && matches!(
            (source_key_mode, rendered_key_mode),
            (KeyMode::K5, KeyMode::K10) | (KeyMode::K7, KeyMode::K14)
        )
}

pub(in crate::app) fn effective_play_key_mode(
    chart_key_mode: KeyMode,
    key_mode_conversion: KeyModeConversionConfig,
) -> KeyMode {
    key_mode_conversion.effective_key_mode(chart_key_mode)
}

/// Battle duplicates lanes for presentation, but uses the source mode's profile slot.
pub(in crate::app) fn key_mode_conversion_for_session(
    conversion: KeyModeConversionConfig,
    session: SessionMode,
    double: DoubleOption,
    has_battle_target: bool,
) -> KeyModeConversionConfig {
    if session.is_battle()
        || has_battle_target
        || matches!(double, DoubleOption::Battle | DoubleOption::BattleAutoScratch)
    {
        KeyModeConversionConfig::Off
    } else {
        conversion
    }
}

pub(in crate::app) fn skin_reload_request_includes_key_mode(
    request: SkinReloadRequest,
    key_mode: KeyMode,
) -> bool {
    match key_mode {
        KeyMode::K4 => request.play4,
        KeyMode::K5 => request.play5,
        KeyMode::K6 => request.play6,
        KeyMode::K7 => request.play7,
        KeyMode::K8 => request.play8,
        KeyMode::K9 => request.play9,
        KeyMode::K10 => request.play10,
        KeyMode::K14 => request.play14,
    }
}

pub(in crate::app) fn hs_fix_option_from_profile(hs_fix: HsFixConfig) -> HsFixOption {
    match hs_fix {
        HsFixConfig::Off => HsFixOption::Off,
        HsFixConfig::StartBpm => HsFixOption::StartBpm,
        HsFixConfig::MinBpm => HsFixOption::MinBpm,
        HsFixConfig::MaxBpm => HsFixOption::MaxBpm,
        HsFixConfig::MainBpm => HsFixOption::MainBpm,
    }
}

pub(in crate::app) fn hs_fix_config_from_option(hs_fix: HsFixOption) -> HsFixConfig {
    match hs_fix {
        HsFixOption::Off => HsFixConfig::Off,
        HsFixOption::StartBpm => HsFixConfig::StartBpm,
        HsFixOption::MinBpm => HsFixConfig::MinBpm,
        HsFixOption::MaxBpm => HsFixConfig::MaxBpm,
        HsFixOption::MainBpm => HsFixConfig::MainBpm,
    }
}

pub(in crate::app) fn target_option_from_profile(target: TargetOptionConfig) -> TargetOption {
    match target {
        TargetOptionConfig::None => TargetOption::None,
        TargetOptionConfig::RankA => TargetOption::RankA,
        TargetOptionConfig::RankAaMinus => TargetOption::RankAaMinus,
        TargetOptionConfig::RankAa => TargetOption::RankAa,
        TargetOptionConfig::RankAaaMinus => TargetOption::RankAaaMinus,
        TargetOptionConfig::RankAaa => TargetOption::RankAaa,
        TargetOptionConfig::RankMaxMinus => TargetOption::RankMaxMinus,
        TargetOptionConfig::Max => TargetOption::Max,
        TargetOptionConfig::RankNext => TargetOption::RankNext,
        TargetOptionConfig::IrTop => TargetOption::IrTop,
        TargetOptionConfig::IrNext => TargetOption::IrNext,
        TargetOptionConfig::RivalTop => TargetOption::RivalTop,
        TargetOptionConfig::RivalNext => TargetOption::RivalNext,
        TargetOptionConfig::RivalIndex(index) => TargetOption::RivalIndex(index),
    }
}

pub(in crate::app) fn target_config_from_option(target: TargetOption) -> TargetOptionConfig {
    match target {
        TargetOption::None => TargetOptionConfig::None,
        TargetOption::RankA => TargetOptionConfig::RankA,
        TargetOption::RankAaMinus => TargetOptionConfig::RankAaMinus,
        TargetOption::RankAa => TargetOptionConfig::RankAa,
        TargetOption::RankAaaMinus => TargetOptionConfig::RankAaaMinus,
        TargetOption::RankAaa => TargetOptionConfig::RankAaa,
        TargetOption::RankMaxMinus => TargetOptionConfig::RankMaxMinus,
        TargetOption::Max => TargetOptionConfig::Max,
        TargetOption::RankNext => TargetOptionConfig::RankNext,
        TargetOption::IrTop => TargetOptionConfig::IrTop,
        TargetOption::IrNext => TargetOptionConfig::IrNext,
        TargetOption::RivalTop => TargetOptionConfig::RivalTop,
        TargetOption::RivalNext => TargetOptionConfig::RivalNext,
        TargetOption::RivalIndex(index) => TargetOptionConfig::RivalIndex(index),
    }
}
use super::*;
