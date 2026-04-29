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
