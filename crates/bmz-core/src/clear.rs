use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearType {
    NoPlay = 0,
    Failed = 1,
    AssistEasy = 2,
    LightAssistEasy = 3,
    Easy = 4,
    Normal = 5,
    Hard = 6,
    ExHard = 7,
    FullCombo = 8,
    Perfect = 9,
    Max = 10,
}

impl ClearType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoPlay => "NoPlay",
            Self::Failed => "Failed",
            Self::AssistEasy => "AssistEasy",
            Self::LightAssistEasy => "LightAssistEasy",
            Self::Easy => "Easy",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
            Self::ExHard => "ExHard",
            Self::FullCombo => "FullCombo",
            Self::Perfect => "Perfect",
            Self::Max => "Max",
        }
    }

    /// `as_str()` の逆変換。DB / snapshot に保存された clear type ラベルを
    /// enum へ戻す。未知のラベルは `None`。
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "NoPlay" => Self::NoPlay,
            "Failed" => Self::Failed,
            "AssistEasy" => Self::AssistEasy,
            "LightAssistEasy" => Self::LightAssistEasy,
            "Easy" => Self::Easy,
            "Normal" => Self::Normal,
            "Hard" => Self::Hard,
            "ExHard" => Self::ExHard,
            "FullCombo" => Self::FullCombo,
            "Perfect" => Self::Perfect,
            "Max" => Self::Max,
            _ => return None,
        })
    }

    /// CLEAR ソート / フォルダランプ用の順位。`NoPlay` と未知ラベルは 0、
    /// それ以外は enum discriminant 順 (Failed=1 .. Max=10)。
    pub fn rank_from_label(label: &str) -> u8 {
        Self::from_label(label).map_or(0, |clear| clear as u8)
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GaugeType {
    AssistEasy = 0,
    Easy = 1,
    Normal = 2,
    Hard = 3,
    ExHard = 4,
    Hazard = 5,
    Class = 6,
    ExClass = 7,
    ExHardClass = 8,
}

impl GaugeType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AssistEasy => "AssistEasy",
            Self::Easy => "Easy",
            Self::Normal => "Normal",
            Self::Hard => "Hard",
            Self::ExHard => "ExHard",
            Self::Hazard => "Hazard",
            Self::Class => "Class",
            Self::ExClass => "ExClass",
            Self::ExHardClass => "ExHardClass",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CLEAR_TYPES: [ClearType; 11] = [
        ClearType::NoPlay,
        ClearType::Failed,
        ClearType::AssistEasy,
        ClearType::LightAssistEasy,
        ClearType::Easy,
        ClearType::Normal,
        ClearType::Hard,
        ClearType::ExHard,
        ClearType::FullCombo,
        ClearType::Perfect,
        ClearType::Max,
    ];

    #[test]
    fn clear_type_label_roundtrips() {
        for clear in ALL_CLEAR_TYPES {
            assert_eq!(ClearType::from_label(clear.as_str()), Some(clear));
        }
        assert_eq!(ClearType::from_label("unknown"), None);
    }

    #[test]
    fn clear_type_rank_follows_discriminant_order() {
        for clear in ALL_CLEAR_TYPES {
            assert_eq!(ClearType::rank_from_label(clear.as_str()), clear as u8);
        }
        // NoPlay と未知ラベルはどちらも 0。
        assert_eq!(ClearType::rank_from_label("NoPlay"), 0);
        assert_eq!(ClearType::rank_from_label("unknown"), 0);
    }
}
