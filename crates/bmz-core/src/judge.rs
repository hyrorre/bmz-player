use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Judge {
    PGreat,
    Great,
    Good,
    Bad,
    Poor,
    EmptyPoor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeBucket {
    Pg = 0,
    Gr = 1,
    Gd = 2,
    Bd = 3,
    Pr = 4,
    Epr = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingSide {
    Fast,
    Slow,
}

impl Judge {
    pub const fn bucket(self) -> JudgeBucket {
        match self {
            Judge::PGreat => JudgeBucket::Pg,
            Judge::Great => JudgeBucket::Gr,
            Judge::Good => JudgeBucket::Gd,
            Judge::Bad => JudgeBucket::Bd,
            Judge::Poor => JudgeBucket::Pr,
            Judge::EmptyPoor => JudgeBucket::Epr,
        }
    }
}
