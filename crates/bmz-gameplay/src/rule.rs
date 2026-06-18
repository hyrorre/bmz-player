use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RuleMode {
    #[default]
    Beatoraja,
    Lr2Oraja,
    Dx,
}

impl RuleMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Beatoraja => "Beatoraja",
            Self::Lr2Oraja => "Lr2Oraja",
            Self::Dx => "Dx",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "Beatoraja" => Some(Self::Beatoraja),
            "Lr2Oraja" => Some(Self::Lr2Oraja),
            "Dx" => Some(Self::Dx),
            _ => None,
        }
    }
}
