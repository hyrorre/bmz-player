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
}

impl ArrangeOption {
    pub const VALUES: [Self; 10] = [
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
    ];

    pub fn cycle(self) -> Self {
        let index = Self::VALUES.iter().position(|&value| value == self).unwrap_or(0);
        Self::VALUES[(index + 1) % Self::VALUES.len()]
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
    Max,
    Aaa,
    Aa,
    A,
    B,
    C,
    D,
    E,
}

impl TargetOption {
    pub fn cycle(self) -> Self {
        match self {
            Self::None => Self::Max,
            Self::Max => Self::Aaa,
            Self::Aaa => Self::Aa,
            Self::Aa => Self::A,
            Self::A => Self::B,
            Self::B => Self::C,
            Self::C => Self::D,
            Self::D => Self::E,
            Self::E => Self::None,
        }
    }

    pub fn cycle_prev(self) -> Self {
        match self {
            Self::None => Self::E,
            Self::Max => Self::None,
            Self::Aaa => Self::Max,
            Self::Aa => Self::Aaa,
            Self::A => Self::Aa,
            Self::B => Self::A,
            Self::C => Self::B,
            Self::D => Self::C,
            Self::E => Self::D,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Max => "MAX",
            Self::Aaa => "AAA",
            Self::Aa => "AA",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
        }
    }

    pub fn target_ex_score(self, total_notes: u32) -> Option<u32> {
        let max = total_notes.saturating_mul(2);
        match self {
            Self::None => None,
            Self::Max => Some(max),
            Self::Aaa => Some(rank_threshold(max, 8)),
            Self::Aa => Some(rank_threshold(max, 7)),
            Self::A => Some(rank_threshold(max, 6)),
            Self::B => Some(rank_threshold(max, 5)),
            Self::C => Some(rank_threshold(max, 4)),
            Self::D => Some(rank_threshold(max, 3)),
            Self::E => Some(rank_threshold(max, 2)),
        }
    }
}

fn rank_threshold(max_ex_score: u32, ninths: u32) -> u32 {
    max_ex_score.saturating_mul(ninths).div_ceil(9)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssistOption {
    #[default]
    Normal,
    Autoplay,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_targets_resolve_from_total_notes() {
        assert_eq!(TargetOption::None.target_ex_score(900), None);
        assert_eq!(TargetOption::Max.target_ex_score(900), Some(1800));
        assert_eq!(TargetOption::Aaa.target_ex_score(900), Some(1600));
        assert_eq!(TargetOption::Aa.target_ex_score(900), Some(1400));
        assert_eq!(TargetOption::A.target_ex_score(900), Some(1200));
        assert_eq!(TargetOption::E.target_ex_score(900), Some(400));
    }

    #[test]
    fn fixed_targets_round_up_to_rank_threshold() {
        assert_eq!(TargetOption::Aaa.target_ex_score(1), Some(2));
        assert_eq!(TargetOption::Aa.target_ex_score(1), Some(2));
        assert_eq!(TargetOption::A.target_ex_score(1), Some(2));
        assert_eq!(TargetOption::B.target_ex_score(1), Some(2));
        assert_eq!(TargetOption::C.target_ex_score(1), Some(1));
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
