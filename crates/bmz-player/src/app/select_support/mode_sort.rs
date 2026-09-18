pub(in crate::app) fn initial_folder_stack(
    _app_config: &crate::config::app_config::AppConfig,
) -> Vec<String> {
    // 有効な曲フォルダが 1 つだけでも、設定フォルダ等を含む選曲ルートから始める。
    Vec::new()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum SelectModeFilter {
    All,
    K7,
    K14,
    K9,
    K5,
    K10,
    K4,
    K6,
    K8,
}

impl SelectModeFilter {
    pub(in crate::app) const ORDER: [Self; 9] = [
        Self::All,
        Self::K7,
        Self::K14,
        Self::K9,
        Self::K5,
        Self::K10,
        Self::K4,
        Self::K6,
        Self::K8,
    ];

    pub(in crate::app) fn next(self) -> Self {
        cycle_enum(Self::ORDER, self, 1)
    }

    pub(in crate::app) fn previous(self) -> Self {
        cycle_enum(Self::ORDER, self, -1)
    }

    pub(in crate::app) fn as_str(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::K7 => "7K",
            Self::K14 => "14K",
            Self::K9 => "9K",
            Self::K5 => "5K",
            Self::K10 => "10K",
            Self::K4 => "4K",
            Self::K6 => "6K",
            Self::K8 => "8K",
        }
    }

    pub(in crate::app) fn key_mode(self) -> Option<KeyMode> {
        match self {
            Self::All => None,
            Self::K7 => Some(KeyMode::K7),
            Self::K14 => Some(KeyMode::K14),
            Self::K9 => Some(KeyMode::K9),
            Self::K5 => Some(KeyMode::K5),
            Self::K10 => Some(KeyMode::K10),
            Self::K4 => Some(KeyMode::K4),
            Self::K6 => Some(KeyMode::K6),
            Self::K8 => Some(KeyMode::K8),
        }
    }

    /// `as_str()` の逆変換。未知の値は `ALL` へフォールバックする。
    pub(in crate::app) fn from_str_or_default(value: &str) -> Self {
        Self::ORDER.into_iter().find(|mode| mode.as_str() == value).unwrap_or(Self::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum SelectDifficultyFilter {
    All,
    Beginner,
    Normal,
    Hyper,
    Another,
    Insane,
}

impl SelectDifficultyFilter {
    pub(in crate::app) const ORDER: [Self; 6] =
        [Self::All, Self::Beginner, Self::Normal, Self::Hyper, Self::Another, Self::Insane];

    pub(in crate::app) fn next(self) -> Self {
        cycle_enum(Self::ORDER, self, 1)
    }

    pub(in crate::app) fn previous(self) -> Self {
        cycle_enum(Self::ORDER, self, -1)
    }

    pub(in crate::app) fn as_str(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Beginner => "BEGINNER",
            Self::Normal => "NORMAL",
            Self::Hyper => "HYPER",
            Self::Another => "ANOTHER",
            Self::Insane => "INSANE",
        }
    }

    pub(in crate::app) const fn difficulty_code(self) -> u8 {
        match self {
            Self::All => 0,
            Self::Beginner => 1,
            Self::Normal => 2,
            Self::Hyper => 3,
            Self::Another => 4,
            Self::Insane => 5,
        }
    }

    pub(in crate::app) fn from_str_or_default(value: &str) -> Self {
        Self::ORDER.into_iter().find(|filter| filter.as_str() == value).unwrap_or(Self::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum SelectSort {
    Title,
    Artist,
    Bpm,
    Length,
    Level,
    Clear,
    Score,
    Bp,
}

impl SelectSort {
    pub(in crate::app) const ORDER: [Self; 8] = [
        Self::Title,
        Self::Artist,
        Self::Bpm,
        Self::Length,
        Self::Level,
        Self::Clear,
        Self::Score,
        Self::Bp,
    ];

    pub(in crate::app) fn next(self) -> Self {
        cycle_enum(Self::ORDER, self, 1)
    }

    pub(in crate::app) fn previous(self) -> Self {
        cycle_enum(Self::ORDER, self, -1)
    }

    pub(in crate::app) fn as_str(self) -> &'static str {
        match self {
            Self::Title => "TITLE",
            Self::Artist => "ARTIST",
            Self::Bpm => "BPM",
            Self::Length => "LENGTH",
            Self::Level => "LEVEL",
            Self::Clear => "CLEAR",
            Self::Score => "SCORE",
            Self::Bp => "BPCOUNT",
        }
    }

    /// `as_str()` の逆変換。未知の値は `TITLE` へフォールバックする。
    pub(in crate::app) fn from_str_or_default(value: &str) -> Self {
        Self::ORDER.into_iter().find(|sort| sort.as_str() == value).unwrap_or(Self::Title)
    }
}

pub(in crate::app) fn cycle_enum<T: Copy + PartialEq, const N: usize>(
    values: [T; N],
    current: T,
    direction: i32,
) -> T {
    let index = values.iter().position(|value| *value == current).unwrap_or(0);
    let len = values.len();
    if direction >= 0 { values[(index + 1) % len] } else { values[(index + len - 1) % len] }
}
use super::*;
