use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
}
