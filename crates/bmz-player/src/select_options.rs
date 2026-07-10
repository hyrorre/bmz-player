use bmz_core::lane::KeyMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ArrangeOption {
    #[default]
    Normal,
    Mirror,
    Random,
    RRandom,
    SRandom,
    Spiral,
    HRandom,
    AllScratch,
    RandomEx,
    SRandomEx,
    FRandom,
    MFRandom,
}

impl ArrangeOption {
    pub const VALUES: [Self; 12] = [
        Self::Normal,
        Self::Mirror,
        Self::Random,
        Self::RRandom,
        Self::SRandom,
        Self::Spiral,
        Self::HRandom,
        Self::AllScratch,
        Self::RandomEx,
        Self::SRandomEx,
        Self::FRandom,
        Self::MFRandom,
    ];

    pub fn cycle(self) -> Self {
        let index = Self::VALUES.iter().position(|&value| value == self).unwrap_or(0);
        Self::VALUES[(index + 1) % Self::VALUES.len()]
    }

    pub fn cycle_prev(self) -> Self {
        let index = Self::VALUES.iter().position(|&value| value == self).unwrap_or(0);
        Self::VALUES[(index + Self::VALUES.len() - 1) % Self::VALUES.len()]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Mirror => "MIRROR",
            Self::Random => "RANDOM",
            Self::RRandom => "R-RANDOM",
            Self::SRandom => "S-RANDOM",
            Self::Spiral => "SPIRAL",
            Self::HRandom => "H-RANDOM",
            Self::AllScratch => "ALL-SCR",
            Self::RandomEx => "RANDOM-EX",
            Self::SRandomEx => "S-RANDOM-EX",
            Self::FRandom => "F-RANDOM",
            Self::MFRandom => "MF-RANDOM",
        }
    }

    pub fn to_persistent_str(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Mirror => "Mirror",
            Self::Random => "Random",
            Self::RRandom => "RRandom",
            Self::SRandom => "SRandom",
            Self::Spiral => "Spiral",
            Self::HRandom => "HRandom",
            Self::AllScratch => "AllScratch",
            Self::RandomEx => "RandomEx",
            Self::SRandomEx => "SRandomEx",
            Self::FRandom => "FRandom",
            Self::MFRandom => "MFRandom",
        }
    }

    pub fn from_persistent_str(value: &str) -> Self {
        match value {
            "Mirror" => Self::Mirror,
            "Random" => Self::Random,
            "RRandom" => Self::RRandom,
            "SRandom" => Self::SRandom,
            "Spiral" => Self::Spiral,
            "HRandom" => Self::HRandom,
            "AllScratch" => Self::AllScratch,
            "RandomEx" => Self::RandomEx,
            "SRandomEx" => Self::SRandomEx,
            "FRandom" => Self::FRandom,
            "MFRandom" => Self::MFRandom,
            _ => Self::Normal,
        }
    }

    pub fn uses_seed(self) -> bool {
        !matches!(self, Self::Normal | Self::Mirror)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetOption {
    #[default]
    None,
    RankA,
    RankAaMinus,
    RankAa,
    RankAaaMinus,
    RankAaa,
    RankMaxMinus,
    Max,
    RankNext,
    IrTop,
    IrNext,
    RivalTop,
    RivalNext,
    RivalIndex(u8),
}

impl TargetOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::None => Self::RankA,
            Self::RankA => Self::RankAaMinus,
            Self::RankAaMinus => Self::RankAa,
            Self::RankAa => Self::RankAaaMinus,
            Self::RankAaaMinus => Self::RankAaa,
            Self::RankAaa => Self::RankMaxMinus,
            Self::RankMaxMinus => Self::Max,
            Self::Max => Self::RankNext,
            Self::RankNext => Self::IrTop,
            Self::IrTop => Self::IrNext,
            Self::IrNext => Self::RivalTop,
            Self::RivalTop => Self::RivalNext,
            Self::RivalNext => Self::None,
            Self::RivalIndex(_) => Self::None,
        }
    }

    pub fn cycle_prev(self) -> Self {
        match self {
            Self::None => Self::RivalNext,
            Self::RankA => Self::None,
            Self::RankAaMinus => Self::RankA,
            Self::RankAa => Self::RankAaMinus,
            Self::RankAaaMinus => Self::RankAa,
            Self::RankAaa => Self::RankAaaMinus,
            Self::RankMaxMinus => Self::RankAaa,
            Self::Max => Self::RankMaxMinus,
            Self::RankNext => Self::Max,
            Self::IrTop => Self::RankNext,
            Self::IrNext => Self::IrTop,
            Self::RivalTop => Self::IrNext,
            Self::RivalNext => Self::RivalTop,
            Self::RivalIndex(_) => Self::RivalNext,
        }
    }

    pub fn as_string(self) -> String {
        match self {
            Self::None => "NONE".to_string(),
            Self::RankA => "RANK_A".to_string(),
            Self::RankAaMinus => "RANK_AA-".to_string(),
            Self::RankAa => "RANK_AA".to_string(),
            Self::RankAaaMinus => "RANK_AAA-".to_string(),
            Self::RankAaa => "RANK_AAA".to_string(),
            Self::RankMaxMinus => "RANK_MAX-".to_string(),
            Self::Max => "MAX".to_string(),
            Self::RankNext => "RANK_NEXT".to_string(),
            Self::IrTop => "IR_TOP".to_string(),
            Self::IrNext => "IR_NEXT".to_string(),
            Self::RivalTop => "RIVAL TOP".to_string(),
            Self::RivalNext => "RIVAL NEXT".to_string(),
            Self::RivalIndex(index) => format!("RIVAL_{index}"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::RankA => "RANK_A",
            Self::RankAaMinus => "RANK_AA-",
            Self::RankAa => "RANK_AA",
            Self::RankAaaMinus => "RANK_AAA-",
            Self::RankAaa => "RANK_AAA",
            Self::RankMaxMinus => "RANK_MAX-",
            Self::Max => "MAX",
            Self::RankNext => "RANK_NEXT",
            Self::IrTop => "IR_TOP",
            Self::IrNext => "IR_NEXT",
            Self::RivalTop => "RIVAL TOP",
            Self::RivalNext => "RIVAL NEXT",
            Self::RivalIndex(_) => "RIVAL",
        }
    }

    pub fn target_ex_score(self, total_notes: u32) -> Option<u32> {
        let max = total_notes.saturating_mul(2);
        match self {
            Self::None => None,
            Self::RankA => Some(rank_threshold(max, 12)),
            Self::RankAaMinus => Some(rank_threshold(max, 13)),
            Self::RankAa => Some(rank_threshold(max, 14)),
            Self::RankAaaMinus => Some(rank_threshold(max, 15)),
            Self::RankAaa => Some(rank_threshold(max, 16)),
            Self::RankMaxMinus => Some(rank_threshold(max, 17)),
            Self::Max => Some(max),
            Self::RankNext
            | Self::IrTop
            | Self::IrNext
            | Self::RivalTop
            | Self::RivalNext
            | Self::RivalIndex(_) => None,
        }
    }

    pub const fn uses_ir_ranking(self) -> bool {
        matches!(
            self,
            Self::IrTop | Self::IrNext | Self::RivalTop | Self::RivalNext | Self::RivalIndex(_)
        )
    }

    pub fn rank_next_ex_score(total_notes: u32, current_ex_score: u32) -> u32 {
        let max = total_notes.saturating_mul(2);
        for eighteenths in 12..=17 {
            let target = rank_threshold(max, eighteenths);
            if current_ex_score < target {
                return target;
            }
        }
        max
    }

    pub fn target_ex_score_with_best(
        self,
        total_notes: u32,
        local_best_ex_score: Option<u32>,
    ) -> Option<u32> {
        match self {
            Self::RankNext => {
                Some(Self::rank_next_ex_score(total_notes, local_best_ex_score.unwrap_or(0)))
            }
            Self::IrTop | Self::IrNext | Self::RivalTop | Self::RivalNext | Self::RivalIndex(_) => {
                None
            }
            _ => self.target_ex_score(total_notes),
        }
    }
}

fn rank_threshold(max_ex_score: u32, eighteenths: u32) -> u32 {
    max_ex_score.saturating_mul(eighteenths).div_ceil(18)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssistOption {
    #[default]
    Normal,
    Autoplay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DoubleOption {
    #[default]
    Off,
    Flip,
    Battle,
    BattleAutoScratch,
}

impl DoubleOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::Flip,
            Self::Flip => Self::Battle,
            Self::Battle => Self::BattleAutoScratch,
            Self::BattleAutoScratch => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Flip => "FLIP",
            Self::Battle => "BATTLE",
            Self::BattleAutoScratch => "BATTLE AS",
        }
    }

    pub fn normalize_for_key_mode(self, key_mode: KeyMode) -> Self {
        match self {
            Self::Off => Self::Off,
            Self::Flip if matches!(key_mode, KeyMode::K10 | KeyMode::K14) => Self::Flip,
            Self::Flip => Self::Off,
            Self::Battle | Self::BattleAutoScratch
                if matches!(key_mode, KeyMode::K5 | KeyMode::K7) =>
            {
                self
            }
            Self::Battle | Self::BattleAutoScratch => Self::Off,
        }
    }

    pub fn score_bucket(self) -> DoubleOptionScoreBucket {
        match self {
            Self::Off | Self::Flip => DoubleOptionScoreBucket::Off,
            Self::Battle => DoubleOptionScoreBucket::Battle,
            Self::BattleAutoScratch => DoubleOptionScoreBucket::BattleAutoScratch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DoubleOptionScoreBucket {
    #[default]
    Off,
    Battle,
    BattleAutoScratch,
}

impl DoubleOptionScoreBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Battle => "Battle",
            Self::BattleAutoScratch => "BattleAutoScratch",
        }
    }

    pub fn from_str_or_off(value: &str) -> Self {
        match value {
            "Battle" => Self::Battle,
            "BattleAutoScratch" | "BattleAssist" => Self::BattleAutoScratch,
            _ => Self::Off,
        }
    }

    pub fn ir_query_value(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Battle => Some("battle"),
            Self::BattleAutoScratch => Some("battle_auto_scratch"),
        }
    }

    pub fn ir_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Battle => "battle",
            Self::BattleAutoScratch => "battle_auto_scratch",
        }
    }

    pub fn from_ir_query_or_off(value: Option<&str>) -> Self {
        match value {
            Some("battle") => Self::Battle,
            Some("battle_auto_scratch") | Some("battle_assist") => Self::BattleAutoScratch,
            _ => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HsFixOption {
    #[default]
    Off,
    StartBpm,
    MinBpm,
    MaxBpm,
    MainBpm,
}

impl HsFixOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::StartBpm,
            Self::StartBpm => Self::MaxBpm,
            Self::MaxBpm => Self::MainBpm,
            Self::MainBpm => Self::MinBpm,
            Self::MinBpm => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::StartBpm => "START BPM",
            Self::MinBpm => "MIN BPM",
            Self::MaxBpm => "MAX BPM",
            Self::MainBpm => "MAIN BPM",
        }
    }
}

#[cfg(test)]
mod rival_target_tests {
    use super::*;

    #[test]
    fn rank_next_target_uses_source_total_and_actual_best() {
        assert_eq!(
            TargetOption::RankNext.target_ex_score_with_best(1000, Some(1500)),
            Some(TargetOption::rank_next_ex_score(1000, 1500))
        );
        assert_eq!(TargetOption::RivalTop.target_ex_score_with_best(1000, Some(1500)), None);
        assert_eq!(TargetOption::Max.target_ex_score_with_best(1000, Some(1)), Some(2000));
    }

    #[test]
    fn rival_targets_cycle_after_ir_targets() {
        assert_eq!(TargetOption::IrNext.cycle(), TargetOption::RivalTop);
        assert_eq!(TargetOption::RivalTop.cycle(), TargetOption::RivalNext);
        assert_eq!(TargetOption::RivalNext.cycle(), TargetOption::None);
        assert_eq!(TargetOption::None.cycle_prev(), TargetOption::RivalNext);
        assert_eq!(TargetOption::RivalIndex(1).cycle(), TargetOption::None);
        assert_eq!(TargetOption::RivalIndex(2).as_string(), "RIVAL_2");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_targets_resolve_from_total_notes() {
        assert_eq!(TargetOption::None.target_ex_score(900), None);
        assert_eq!(TargetOption::Max.target_ex_score(900), Some(1800));
        assert_eq!(TargetOption::RankA.target_ex_score(900), Some(1200));
        assert_eq!(TargetOption::RankAaMinus.target_ex_score(900), Some(1300));
        assert_eq!(TargetOption::RankAa.target_ex_score(900), Some(1400));
        assert_eq!(TargetOption::RankAaaMinus.target_ex_score(900), Some(1500));
        assert_eq!(TargetOption::RankAaa.target_ex_score(900), Some(1600));
        assert_eq!(TargetOption::RankMaxMinus.target_ex_score(900), Some(1700));
    }

    #[test]
    fn fixed_targets_round_up_to_rank_threshold() {
        assert_eq!(TargetOption::RankA.target_ex_score(1), Some(2));
        assert_eq!(TargetOption::RankAa.target_ex_score(1), Some(2));
        assert_eq!(TargetOption::RankAaa.target_ex_score(1), Some(2));
    }

    #[test]
    fn rank_next_uses_same_thresholds_as_fixed_rank_targets() {
        assert_eq!(TargetOption::rank_next_ex_score(900, 1199), 1200);
        assert_eq!(TargetOption::rank_next_ex_score(900, 1200), 1300);
        assert_eq!(TargetOption::rank_next_ex_score(900, 1700), 1800);
    }

    #[test]
    fn double_option_normalizes_for_supported_key_modes() {
        assert_eq!(DoubleOption::Flip.normalize_for_key_mode(KeyMode::K7), DoubleOption::Off);
        assert_eq!(DoubleOption::Flip.normalize_for_key_mode(KeyMode::K14), DoubleOption::Flip);
        assert_eq!(DoubleOption::Battle.normalize_for_key_mode(KeyMode::K7), DoubleOption::Battle);
        assert_eq!(
            DoubleOption::BattleAutoScratch.normalize_for_key_mode(KeyMode::K5),
            DoubleOption::BattleAutoScratch
        );
        assert_eq!(DoubleOption::Battle.normalize_for_key_mode(KeyMode::K14), DoubleOption::Off);
    }

    #[test]
    fn double_option_score_bucket_groups_off_and_flip() {
        assert_eq!(DoubleOption::Off.score_bucket(), DoubleOptionScoreBucket::Off);
        assert_eq!(DoubleOption::Flip.score_bucket(), DoubleOptionScoreBucket::Off);
        assert_eq!(DoubleOption::Battle.score_bucket(), DoubleOptionScoreBucket::Battle);
        assert_eq!(
            DoubleOption::BattleAutoScratch.score_bucket(),
            DoubleOptionScoreBucket::BattleAutoScratch
        );
    }

    #[test]
    fn hs_fix_cycles_in_beatoraja_order() {
        assert_eq!(HsFixOption::Off.cycle(), HsFixOption::StartBpm);
        assert_eq!(HsFixOption::StartBpm.cycle(), HsFixOption::MaxBpm);
        assert_eq!(HsFixOption::MaxBpm.cycle(), HsFixOption::MainBpm);
        assert_eq!(HsFixOption::MainBpm.cycle(), HsFixOption::MinBpm);
        assert_eq!(HsFixOption::MinBpm.cycle(), HsFixOption::Off);
    }
}

impl AssistOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::Normal => Self::Autoplay,
            Self::Autoplay => Self::Normal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Autoplay => "AUTOPLAY",
        }
    }
}
