use bmz_chart::model::LongNoteMode;
use bmz_core::clear::GaugeType;
use bmz_core::lane::KeyMode;
use bmz_gameplay::gauge::GaugeAutoShiftMode;
use bmz_gameplay::judge::model::JudgeAlgorithm;
use bmz_gameplay::rule::RuleMode;
use bmz_render::snapshot::{
    SKIN_SOURCE_LN_DEFINED_CN_BIT, SKIN_SOURCE_LN_DEFINED_HCN_BIT, SKIN_SOURCE_LN_DEFINED_LN_BIT,
    SKIN_SOURCE_LN_UNDEFINED_BIT,
};

use crate::config::profile_config::KeyModeConversionConfig;
use crate::ln_policy::{ChartLnProfile, LnPolicySetting, LnScorePolicy, played_ln_mode};
use crate::select_options::{DoubleOption, HsFixOption, SessionMode};

pub(crate) const fn rule_mode_index(mode: RuleMode) -> usize {
    match mode {
        RuleMode::Beatoraja => 0,
        RuleMode::Lr2Oraja => 1,
        RuleMode::Dx => 2,
    }
}

pub(crate) const fn ln_policy_setting_index(policy: LnPolicySetting) -> usize {
    match policy {
        LnPolicySetting::AutoLn => 0,
        LnPolicySetting::AutoCn => 1,
        LnPolicySetting::AutoHcn => 2,
        LnPolicySetting::ForceLn => 3,
        LnPolicySetting::ForceCn => 4,
        LnPolicySetting::ForceHcn => 5,
    }
}

pub(crate) const fn ln_score_policy_index(policy: LnScorePolicy) -> usize {
    match policy {
        LnScorePolicy::AutoLn => 0,
        LnScorePolicy::AutoCn => 1,
        LnScorePolicy::AutoHcn => 2,
        LnScorePolicy::ForceLn => 3,
        LnScorePolicy::ForceCn => 4,
        LnScorePolicy::ForceHcn => 5,
        LnScorePolicy::ForceHln => 6,
    }
}

pub(crate) const fn session_mode_index(mode: SessionMode) -> usize {
    match mode {
        SessionMode::Normal => 0,
        SessionMode::Practice => 1,
        SessionMode::Autoplay => 2,
        SessionMode::AutoplayBattle => 3,
        SessionMode::GBattle => 4,
    }
}

pub(crate) const fn double_option_index(option: DoubleOption) -> usize {
    match option {
        DoubleOption::Off => 0,
        DoubleOption::Flip => 1,
        DoubleOption::Battle => 2,
        DoubleOption::BattleAutoScratch => 3,
    }
}

pub(crate) const fn hsfix_index(option: HsFixOption) -> usize {
    match option {
        HsFixOption::Off => 0,
        HsFixOption::StartBpm => 1,
        HsFixOption::MaxBpm => 2,
        HsFixOption::MainBpm => 3,
        HsFixOption::MinBpm => 4,
    }
}

pub(crate) const fn gauge_auto_shift_index(mode: GaugeAutoShiftMode) -> usize {
    match mode {
        GaugeAutoShiftMode::Off => 0,
        GaugeAutoShiftMode::Continue => 1,
        GaugeAutoShiftMode::HardToGroove => 2,
        GaugeAutoShiftMode::BestClear => 3,
        GaugeAutoShiftMode::SelectToUnder => 4,
    }
}

pub(crate) const fn bottom_shiftable_gauge_index(gauge: GaugeType) -> usize {
    match gauge {
        GaugeType::Easy => 1,
        GaugeType::Normal => 2,
        _ => 0,
    }
}

pub(crate) const fn judge_algorithm_index(algorithm: JudgeAlgorithm) -> usize {
    match algorithm {
        JudgeAlgorithm::Combo => 0,
        JudgeAlgorithm::Duration | JudgeAlgorithm::Score => 1,
        JudgeAlgorithm::Lowest => 2,
    }
}

pub(crate) const fn long_note_mode_index(mode: LongNoteMode) -> usize {
    match mode {
        LongNoteMode::Ln => 0,
        LongNoteMode::Cn => 1,
        LongNoteMode::Hcn => 2,
        LongNoteMode::Hln => 3,
    }
}

pub(crate) fn effective_ln_mode_index(profile: ChartLnProfile, policy: LnScorePolicy) -> usize {
    long_note_mode_index(played_ln_mode(profile, policy).unwrap_or(ln_score_policy_mode(policy)))
}

pub(crate) const fn ln_score_policy_mode(policy: LnScorePolicy) -> LongNoteMode {
    match policy {
        LnScorePolicy::AutoLn | LnScorePolicy::ForceLn => LongNoteMode::Ln,
        LnScorePolicy::AutoCn | LnScorePolicy::ForceCn => LongNoteMode::Cn,
        LnScorePolicy::AutoHcn | LnScorePolicy::ForceHcn => LongNoteMode::Hcn,
        LnScorePolicy::ForceHln => LongNoteMode::Hln,
    }
}

pub(crate) const fn source_ln_profile_bits(profile: ChartLnProfile) -> u8 {
    (if profile.has_undefined_ln { SKIN_SOURCE_LN_UNDEFINED_BIT } else { 0 })
        | (if profile.has_defined_ln { SKIN_SOURCE_LN_DEFINED_LN_BIT } else { 0 })
        | (if profile.has_defined_cn { SKIN_SOURCE_LN_DEFINED_CN_BIT } else { 0 })
        | (if profile.has_defined_hcn { SKIN_SOURCE_LN_DEFINED_HCN_BIT } else { 0 })
        | (if profile.has_defined_hln {
            bmz_render::snapshot::SKIN_SOURCE_LN_DEFINED_HLN_BIT
        } else {
            0
        })
}

pub(crate) const fn effective_key_mode(
    source: KeyMode,
    double_option: DoubleOption,
    session_mode: SessionMode,
    key_mode_conversion: KeyModeConversionConfig,
) -> KeyMode {
    if session_mode.is_battle()
        || matches!(double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch)
    {
        return match source {
            KeyMode::K5 => KeyMode::K10,
            KeyMode::K7 => KeyMode::K14,
            _ => source,
        };
    }
    key_mode_conversion.effective_key_mode(source)
}

/// Select に公開する実効 key mode。
///
/// SessionMode の battle は 5K/7K の専用 battle skin を選ぶための状態であり、
/// 選曲中の譜面モード自体は 10K/14K に変換しない。DP OPTION の BATTLE は従来どおり
/// Normal/Practice/Autoplay で 10K/14K を返す。
pub(crate) const fn select_effective_key_mode(
    source: KeyMode,
    double_option: DoubleOption,
    session_mode: SessionMode,
    key_mode_conversion: KeyModeConversionConfig,
) -> KeyMode {
    if session_mode.uses_battle_skin() {
        return source;
    }
    effective_key_mode(source, double_option, SessionMode::Normal, key_mode_conversion)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_indices_follow_the_documented_stable_order() {
        assert_eq!(rule_mode_index(RuleMode::Beatoraja), 0);
        assert_eq!(rule_mode_index(RuleMode::Lr2Oraja), 1);
        assert_eq!(rule_mode_index(RuleMode::Dx), 2);
        assert_eq!(session_mode_index(SessionMode::Normal), 0);
        assert_eq!(session_mode_index(SessionMode::Practice), 1);
        assert_eq!(session_mode_index(SessionMode::Autoplay), 2);
        assert_eq!(session_mode_index(SessionMode::AutoplayBattle), 3);
        assert_eq!(session_mode_index(SessionMode::GBattle), 4);

        let score_policies = [
            LnScorePolicy::AutoLn,
            LnScorePolicy::AutoCn,
            LnScorePolicy::AutoHcn,
            LnScorePolicy::ForceLn,
            LnScorePolicy::ForceCn,
            LnScorePolicy::ForceHcn,
        ];
        for (index, (setting, score_policy)) in
            LnPolicySetting::ORDER.into_iter().zip(score_policies).enumerate()
        {
            assert_eq!(ln_policy_setting_index(setting), index);
            assert_eq!(ln_score_policy_index(score_policy), index);
        }
    }

    #[test]
    fn select_keeps_source_key_mode_for_battle_sessions() {
        assert_eq!(
            select_effective_key_mode(
                KeyMode::K5,
                DoubleOption::Battle,
                SessionMode::AutoplayBattle,
                KeyModeConversionConfig::Off,
            ),
            KeyMode::K5
        );
        assert_eq!(
            select_effective_key_mode(
                KeyMode::K7,
                DoubleOption::BattleAutoScratch,
                SessionMode::GBattle,
                KeyModeConversionConfig::Off,
            ),
            KeyMode::K7
        );
        assert_eq!(
            select_effective_key_mode(
                KeyMode::K7,
                DoubleOption::Battle,
                SessionMode::Normal,
                KeyModeConversionConfig::Off,
            ),
            KeyMode::K14
        );
        assert_eq!(
            select_effective_key_mode(
                KeyMode::K7,
                DoubleOption::Off,
                SessionMode::AutoplayBattle,
                KeyModeConversionConfig::SevenToSix,
            ),
            KeyMode::K7
        );
        assert_eq!(
            select_effective_key_mode(
                KeyMode::K7,
                DoubleOption::Off,
                SessionMode::Normal,
                KeyModeConversionConfig::SevenToSix,
            ),
            KeyMode::K6
        );
    }
}
