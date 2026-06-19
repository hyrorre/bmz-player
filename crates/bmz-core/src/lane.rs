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
    // K9 モードでは Key8/Key9 は PMS Key8/Key9 として使う。
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

    pub const fn from_pms_key(key: u8) -> Option<Self> {
        match key {
            1 => Some(Lane::Key1),
            2 => Some(Lane::Key2),
            3 => Some(Lane::Key3),
            4 => Some(Lane::Key4),
            5 => Some(Lane::Key5),
            6 => Some(Lane::Key6),
            7 => Some(Lane::Key7),
            8 => Some(Lane::Key8),
            9 => Some(Lane::Key9),
            _ => None,
        }
    }
}

/// BMS キーモード。BMS ファイルのチャネル使用状況から判定する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum KeyMode {
    /// Qwilight-style 4K (Scratch なし)
    K4,
    K5,
    #[default]
    K7,
    /// Qwilight-style 6K (Scratch なし)
    K6,
    /// Qwilight-style 8K (Scratch なし)
    K8,
    /// PMS 9K (Pop'n)。Scratch なし。PMS 5K もスキン/入力は K9 に寄せる。
    K9,
    K10,
    K14,
}

/// Beat 系譜面のキーレイアウト。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatKeyLayout {
    Beat,
}

/// PMS 譜面のキーレイアウト variant。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmsKeyLayout {
    Standard,
    BmeType,
}

/// import 時のキーレイアウト family。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKeyLayout {
    Beat(BeatKeyLayout),
    Pms(PmsKeyLayout),
}

impl ChartKeyLayout {
    pub const fn beat() -> Self {
        Self::Beat(BeatKeyLayout::Beat)
    }

    pub const fn pms(variant: PmsKeyLayout) -> Self {
        Self::Pms(variant)
    }

    pub const fn is_pms(self) -> bool {
        matches!(self, Self::Pms(_))
    }
}

/// 4K Qwilight 表示順 (Key1-4, Scratch なし)
const ACTIVE_K4: [Lane; 4] = [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4];

/// 6K Qwilight 表示順 (Key1-6, Scratch なし)
const ACTIVE_K6: [Lane; 6] =
    [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6];

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

/// 8K Qwilight 表示順 (Key1-8, Scratch なし)
const ACTIVE_K8: [Lane; 8] = [
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
    Lane::Key8,
];

/// 9K PMS 表示順 (Scratch なし, Key1-9)
const ACTIVE_K9: [Lane; 9] = [
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
    Lane::Key8,
    Lane::Key9,
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
            KeyMode::K4 => &ACTIVE_K4,
            KeyMode::K5 => &ACTIVE_K5,
            KeyMode::K6 => &ACTIVE_K6,
            KeyMode::K7 => &ACTIVE_K7,
            KeyMode::K8 => &ACTIVE_K8,
            KeyMode::K9 => &ACTIVE_K9,
            KeyMode::K10 => &ACTIVE_K10,
            KeyMode::K14 => &ACTIVE_K14,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            KeyMode::K4 => "4K",
            KeyMode::K5 => "5K",
            KeyMode::K6 => "6K",
            KeyMode::K7 => "7K",
            KeyMode::K8 => "8K",
            KeyMode::K9 => "9K",
            KeyMode::K10 => "10K",
            KeyMode::K14 => "14K",
        }
    }

    /// profile `[input.play.*]` の map キー (小文字)。
    pub fn play_map_key(self) -> &'static str {
        match self {
            KeyMode::K4 => "4k",
            KeyMode::K5 => "5k",
            KeyMode::K6 => "6k",
            KeyMode::K7 => "7k",
            KeyMode::K8 => "8k",
            KeyMode::K9 => "9k",
            KeyMode::K10 => "10k",
            KeyMode::K14 => "14k",
        }
    }

    /// `play_map_key` / `as_str` の逆 (大文字小文字無視)。
    pub fn from_play_map_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "4k" => Some(KeyMode::K4),
            "5k" => Some(KeyMode::K5),
            "6k" => Some(KeyMode::K6),
            "7k" => Some(KeyMode::K7),
            "8k" => Some(KeyMode::K8),
            "9k" => Some(KeyMode::K9),
            "10k" => Some(KeyMode::K10),
            "14k" => Some(KeyMode::K14),
            _ => None,
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        Self::from_play_map_key(value)
    }

    /// layout family に応じてキーモードを推定する。
    pub fn detect_from_lanes_with_layout(
        layout: ChartKeyLayout,
        lanes: impl Iterator<Item = Lane>,
    ) -> Self {
        match layout {
            ChartKeyLayout::Beat(_) => Self::detect_from_lanes(lanes),
            ChartKeyLayout::Pms(_) => Self::detect_pms_from_lanes(lanes),
        }
    }

    /// PMS 譜面用。5K/9K どちらもスキン/入力は K9 に寄せる。
    pub fn detect_pms_from_lanes(_lanes: impl Iterator<Item = Lane>) -> Self {
        KeyMode::K9
    }

    /// Beat 系 BMS オブジェクトのレーン集合からキーモードを推定する。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn k9_active_lanes_are_nine_keys_without_scratch() {
        assert_eq!(KeyMode::K9.lane_count(), 9);
        assert_eq!(KeyMode::K9.active_lanes(), &ACTIVE_K9);
        assert_eq!(KeyMode::K9.as_str(), "9K");
        assert_eq!(KeyMode::from_str_opt("9k"), Some(KeyMode::K9));
    }

    #[test]
    fn k8_active_lanes_are_eight_keys_without_scratch() {
        assert_eq!(KeyMode::K8.lane_count(), 8);
        assert_eq!(KeyMode::K8.active_lanes(), &ACTIVE_K8);
        assert_eq!(KeyMode::K8.as_str(), "8K");
        assert_eq!(KeyMode::from_str_opt("8k"), Some(KeyMode::K8));
    }

    #[test]
    fn pms_detect_always_returns_k9() {
        let lanes = [Lane::Key1, Lane::Key2, Lane::Key3];
        assert_eq!(KeyMode::detect_pms_from_lanes(lanes.into_iter()), KeyMode::K9,);
    }

    #[test]
    fn beat_detect_does_not_classify_k9_lanes_as_k10() {
        let lanes = [Lane::Key1, Lane::Key8, Lane::Key9];
        assert_eq!(KeyMode::detect_from_lanes(lanes.into_iter()), KeyMode::K10);
    }

    #[test]
    fn detect_with_pms_layout_uses_k9_even_for_five_keys() {
        let lanes = [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5];
        assert_eq!(
            KeyMode::detect_from_lanes_with_layout(
                ChartKeyLayout::pms(PmsKeyLayout::Standard),
                lanes.into_iter(),
            ),
            KeyMode::K9,
        );
    }

    #[test]
    fn lane_from_pms_key_maps_one_to_nine() {
        assert_eq!(Lane::from_pms_key(1), Some(Lane::Key1));
        assert_eq!(Lane::from_pms_key(9), Some(Lane::Key9));
        assert_eq!(Lane::from_pms_key(0), None);
    }
}
