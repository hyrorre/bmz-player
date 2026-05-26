use serde::{Deserialize, Serialize};

pub const LANE_COUNT: usize = 16;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lane {
    // 1P (SP / P1 side of DP)
    Scratch = 0,
    Key1 = 1,
    Key2 = 2,
    Key3 = 3,
    Key4 = 4,
    Key5 = 5,
    Key6 = 6,
    Key7 = 7,
    // 2P (P2 side of DP): Key8=P2Key1..Key14=P2Key7, Scratch2=P2Scratch
    Key8 = 8,
    Key9 = 9,
    Key10 = 10,
    Key11 = 11,
    Key12 = 12,
    Key13 = 13,
    Key14 = 14,
    Scratch2 = 15,
}

impl Lane {
    pub const ALL: [Lane; LANE_COUNT] = [
        Lane::Scratch,
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key8,
        Lane::Key9,
        Lane::Key10,
        Lane::Key11,
        Lane::Key12,
        Lane::Key13,
        Lane::Key14,
        Lane::Scratch2,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// BMS キーモード。BMS ファイルのチャネル使用状況から判定する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyMode {
    K5,
    #[default]
    K7,
    K10,
    K14,
}

/// 5K 表示順レーン配列 (Scratch, Key1-5)
const ACTIVE_K5: [Lane; 6] =
    [Lane::Scratch, Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5];

/// 7K 表示順レーン配列 (Scratch, Key1-7)
const ACTIVE_K7: [Lane; 8] = [
    Lane::Scratch,
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
];

/// 10K 表示順レーン配列 (P1Scratch, P1Key1-5, P2Key1-5, P2Scratch)
const ACTIVE_K10: [Lane; 12] = [
    Lane::Scratch,
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key8,
    Lane::Key9,
    Lane::Key10,
    Lane::Key11,
    Lane::Key12,
    Lane::Scratch2,
];

/// 14K 表示順レーン配列 (P1Scratch, P1Key1-7, P2Key1-7, P2Scratch)
const ACTIVE_K14: [Lane; 16] = Lane::ALL;

impl KeyMode {
    pub fn lane_count(self) -> usize {
        self.active_lanes().len()
    }

    pub fn active_lanes(self) -> &'static [Lane] {
        match self {
            KeyMode::K5 => &ACTIVE_K5,
            KeyMode::K7 => &ACTIVE_K7,
            KeyMode::K10 => &ACTIVE_K10,
            KeyMode::K14 => &ACTIVE_K14,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            KeyMode::K5 => "5K",
            KeyMode::K7 => "7K",
            KeyMode::K10 => "10K",
            KeyMode::K14 => "14K",
        }
    }

    /// `as_str` の逆。"5K" / "7K" / "10K" / "14K" を受ける (大文字小文字無視)。
    /// 未知値は `None`。
    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "5K" => Some(KeyMode::K5),
            "7K" => Some(KeyMode::K7),
            "10K" => Some(KeyMode::K10),
            "14K" => Some(KeyMode::K14),
            _ => None,
        }
    }

    /// BMS オブジェクトのレーン集合からキーモードを推定する。
    pub fn detect_from_lanes(lanes: impl Iterator<Item = Lane>) -> Self {
        let mut has_p2_key = false;
        let mut has_p2_key67 = false;
        let mut has_p1_key67 = false;

        for lane in lanes {
            match lane {
                Lane::Key8
                | Lane::Key9
                | Lane::Key10
                | Lane::Key11
                | Lane::Key12
                | Lane::Scratch2 => has_p2_key = true,
                Lane::Key13 | Lane::Key14 => {
                    has_p2_key = true;
                    has_p2_key67 = true;
                }
                Lane::Key6 | Lane::Key7 => has_p1_key67 = true,
                _ => {}
            }
        }

        if has_p2_key {
            if has_p1_key67 || has_p2_key67 { KeyMode::K14 } else { KeyMode::K10 }
        } else if has_p1_key67 {
            KeyMode::K7
        } else {
            KeyMode::K5
        }
    }
}
