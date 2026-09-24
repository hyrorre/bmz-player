use super::skin_ir::default_true;
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(skip)]
    pub cli_play: Option<Box<(crate::cli::PlayOverrides, crate::cli::PlayOverrides)>>,
    pub version: u32,
    pub id: String,
    pub display_name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub play: PlayDefaultsConfig,
    pub judge: JudgeConfig,
    pub lane: LaneViewConfig,
    /// Key-mode-specific play presentation settings. Lowercase keys match
    /// `KeyMode::play_map_key()` (`4k`, `5k`, ..., `14k`).
    #[serde(default)]
    pub play_mode: BTreeMap<String, PlayModeConfig>,
    /// `play` / `judge` / `lane` contain the editable mirror for this mode.
    /// Runtime-only so legacy callers can keep using the existing fields.
    #[serde(skip)]
    pub active_play_mode: KeyMode,
    pub input: ProfileInputConfig,
    pub rival: RivalConfig,
    pub replay: ReplayConfig,
    #[serde(default)]
    pub ir: IrConfig,
    pub ui: UiConfig,
    pub audio_mix: AudioMixConfig,
    #[serde(default)]
    pub system_sound: SystemSoundConfig,
    #[serde(default)]
    pub skin: SkinConfig,
    #[serde(default)]
    pub select: SelectStateConfig,
    #[serde(default)]
    pub statistics: StatisticsConfig,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StatisticsConfig {
    /// Local hour at which BMZ starts a new statistics day (0..=23).
    #[serde(default)]
    pub day_start_hour: u8,
}

/// 選曲画面の表示状態。キーモード・譜面難易度フィルターとソートを永続化する。
/// 値は app 層の各 enum の `as_str()` を文字列で保持し、
/// 読込時に未知の値なら既定へフォールバックする。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectStateConfig {
    #[serde(default = "default_select_mode_filter")]
    pub mode_filter: String,
    #[serde(default = "default_select_difficulty_filter")]
    pub difficulty_filter: String,
    #[serde(default = "default_select_sort")]
    pub sort: String,
    /// 難易度表のレベルフォルダ内で、表のレベルと譜面本来のレベルの
    /// どちらを選曲表示へ使うか。
    #[serde(default)]
    pub difficulty_table_level_display: DifficultyTableLevelDisplay,
    #[serde(default)]
    pub random_select: bool,
    #[serde(default)]
    pub random_select_no_play: bool,
    #[serde(default)]
    pub random_select_failed: bool,
    #[serde(default)]
    pub random_select_not_easy: bool,
    #[serde(default)]
    pub random_select_not_clear: bool,
    #[serde(default)]
    pub random_select_not_hard: bool,
    #[serde(default)]
    pub random_select_not_ex_hard: bool,
    #[serde(default)]
    pub random_select_not_full_combo: bool,
    #[serde(default)]
    pub random_mix: RandomMixConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifficultyTableLevelDisplay {
    #[default]
    Table,
    Chart,
}

/// LR2-style RANDOM MIX generation constraints.
///
/// Zero disables a bound. `stages = 0` selects 2..=4 stages randomly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RandomMixConfig {
    #[serde(default)]
    pub target_level: u32,
    #[serde(default)]
    pub max_level: u32,
    #[serde(default)]
    pub min_level: u32,
    #[serde(default = "default_random_mix_bpm_range")]
    pub bpm_range: u32,
    #[serde(default)]
    pub max_bpm: u32,
    #[serde(default)]
    pub min_bpm: u32,
    #[serde(default = "default_random_mix_stages")]
    pub stages: u32,
}

pub const fn default_random_mix_bpm_range() -> u32 {
    10
}

pub const fn default_random_mix_stages() -> u32 {
    5
}

impl Default for RandomMixConfig {
    fn default() -> Self {
        Self {
            target_level: 0,
            max_level: 0,
            min_level: 0,
            bpm_range: default_random_mix_bpm_range(),
            max_bpm: 0,
            min_bpm: 0,
            stages: default_random_mix_stages(),
        }
    }
}

pub fn default_select_mode_filter() -> String {
    "ALL".to_string()
}

pub fn default_select_sort() -> String {
    "TITLE".to_string()
}

pub fn default_select_difficulty_filter() -> String {
    "ALL".to_string()
}

impl SelectStateConfig {
    pub fn random_select_flags(&self) -> [bool; 8] {
        [
            self.random_select,
            self.random_select_no_play,
            self.random_select_failed,
            self.random_select_not_easy,
            self.random_select_not_clear,
            self.random_select_not_hard,
            self.random_select_not_ex_hard,
            self.random_select_not_full_combo,
        ]
    }
}

impl Default for SelectStateConfig {
    fn default() -> Self {
        Self {
            mode_filter: default_select_mode_filter(),
            difficulty_filter: default_select_difficulty_filter(),
            sort: default_select_sort(),
            difficulty_table_level_display: DifficultyTableLevelDisplay::default(),
            random_select: false,
            random_select_no_play: false,
            random_select_failed: false,
            random_select_not_easy: false,
            random_select_not_clear: false,
            random_select_not_hard: false,
            random_select_not_ex_hard: false,
            random_select_not_full_combo: false,
            random_mix: RandomMixConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayDefaultsConfig {
    #[serde(default)]
    pub rule_mode: RuleMode,
    #[serde(default)]
    pub ln_mode_policy: LnPolicySetting,
    /// Key-mode conversion applied before play. Unsupported source modes leave
    /// the chart unchanged and remain score-eligible.
    #[serde(default)]
    pub key_mode_conversion: KeyModeConversionConfig,
    /// beatoraja `sevenToNinePattern` (1..=6). OFF is represented by
    /// `key_mode_conversion = "Off"`, so this value always remembers the last
    /// active placement.
    #[serde(default)]
    pub seven_to_nine_pattern: SevenToNinePattern,
    /// beatoraja `sevenToNineType` (0..=2).
    #[serde(default)]
    pub seven_to_nine_type: SevenToNineType,
    /// 7K to 9K conversion scoring rules. `7K` keeps the source chart's
    /// judgement/gauge rules and remains score eligible; `9K` uses PMS rules
    /// and disables every persistence path.
    #[serde(default)]
    pub seven_to_nine_rule_mode: SevenToNineRuleMode,
    /// Legacy BMZ profile migration field. New profiles use
    /// `key_mode_conversion = "SevenToSix"` and omit this field.
    #[serde(default, skip_serializing_if = "is_false")]
    pub seven_to_six: bool,
    pub gauge: GaugeTypeConfig,
    #[serde(default)]
    pub gauge_auto_shift: GaugeAutoShiftConfig,
    #[serde(default)]
    pub bottom_shiftable_gauge: BottomShiftableGaugeConfig,
    pub random: RandomOptionConfig,
    #[serde(default)]
    pub random2: RandomOptionConfig,
    #[serde(default)]
    pub double_option: DoubleOptionConfig,
    #[serde(default)]
    pub hs_fix: HsFixConfig,
    #[serde(default)]
    pub target: TargetOptionConfig,
    pub lane_effect: LaneEffectConfig,
    #[serde(default)]
    pub assist: AssistOptionConfig,
    /// beatoraja `PlayerConfig.guideSE` 相当。判定時に guide-*.wav を再生する。
    #[serde(default)]
    pub guide_se: bool,
    /// lr2oraja-endlessdream `PlayerConfig.noteretention` 相当。
    /// 未処理の通常ノートを判定ラインへ滞留させる。
    #[serde(default)]
    pub note_retention: bool,
    /// 選曲画面で選んだセッション全体のモード。
    ///
    /// 旧 profile の `auto_play` を読み込めるよう Option とし、None の場合だけ
    /// `auto_play` から Normal / Autoplay を復元する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_mode: Option<SessionMode>,
    /// v0.1 系 profile / 設定 UI との互換用ミラー。
    /// 新規保存では `session_mode.primary_autoplay()` と同期する。
    pub auto_play: bool,
    #[serde(default = "default_bga_mode")]
    pub bga: BgaModeConfig,
    #[serde(default = "default_bga_expand")]
    pub bga_expand: BgaExpandConfig,
    #[serde(default = "default_misslayer_duration_ms")]
    pub misslayer_duration_ms: u32,
    /// E1+E2 長押し強制終了までの時間(ms)。beatoraja 既定 1000ms。
    #[serde(default = "default_play_exit_hold_ms")]
    pub play_exit_hold_ms: u32,
    /// LN / HLN モードでも終端 (tail) キャップを描画するか。
    /// beatoraja は LN モードで tail キャップを描画しないため既定 OFF。
    #[serde(default)]
    pub show_ln_tail_cap: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum KeyModeConversionConfig {
    #[default]
    Off,
    SpToDp,
    SevenToNine,
    SevenToSix,
}

impl KeyModeConversionConfig {
    pub const VALUES: [Self; 4] = [Self::Off, Self::SpToDp, Self::SevenToNine, Self::SevenToSix];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::SpToDp => "SP TO DP",
            Self::SevenToNine => "7K TO 9K",
            Self::SevenToSix => "7K TO 6K",
        }
    }

    pub const fn applies_to(self, source: KeyMode) -> bool {
        match self {
            Self::Off => false,
            Self::SpToDp => matches!(source, KeyMode::K5 | KeyMode::K7),
            Self::SevenToNine | Self::SevenToSix => matches!(source, KeyMode::K7),
        }
    }

    pub const fn effective_key_mode(self, source: KeyMode) -> KeyMode {
        match (self, source) {
            (Self::SpToDp, KeyMode::K5) => KeyMode::K10,
            (Self::SpToDp, KeyMode::K7) => KeyMode::K14,
            (Self::SevenToNine, KeyMode::K7) => KeyMode::K9,
            (Self::SevenToSix, KeyMode::K7) => KeyMode::K6,
            _ => source,
        }
    }

    pub const fn score_persistence_disabled(
        self,
        seven_to_nine_rule_mode: SevenToNineRuleMode,
    ) -> bool {
        match self {
            Self::Off => false,
            Self::SevenToNine => matches!(seven_to_nine_rule_mode, SevenToNineRuleMode::Keys9),
            Self::SpToDp | Self::SevenToSix => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum SevenToNinePattern {
    Sc1Key2To8 = 1,
    Sc1Key3To9 = 2,
    Sc2Key3To9 = 3,
    Sc8Key1To7 = 4,
    #[default]
    Sc9Key1To7 = 5,
    Sc9Key2To8 = 6,
}

impl SevenToNinePattern {
    pub const VALUES: [Self; 6] = [
        Self::Sc1Key2To8,
        Self::Sc1Key3To9,
        Self::Sc2Key3To9,
        Self::Sc8Key1To7,
        Self::Sc9Key1To7,
        Self::Sc9Key2To8,
    ];

    pub const fn value(self) -> u8 {
        self as u8
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Sc1Key2To8 => "SC1 KEY2-8",
            Self::Sc1Key3To9 => "SC1 KEY3-9",
            Self::Sc2Key3To9 => "SC2 KEY3-9",
            Self::Sc8Key1To7 => "SC8 KEY1-7",
            Self::Sc9Key1To7 => "SC9 KEY1-7",
            Self::Sc9Key2To8 => "SC9 KEY2-8",
        }
    }
}

impl TryFrom<u8> for SevenToNinePattern {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::VALUES
            .into_iter()
            .find(|pattern| pattern.value() == value)
            .ok_or_else(|| format!("sevenToNinePattern must be in 1..=6, got {value}"))
    }
}

impl From<SevenToNinePattern> for u8 {
    fn from(value: SevenToNinePattern) -> Self {
        value.value()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum SevenToNineType {
    #[default]
    Fixed = 0,
    NoMashing = 1,
    Alternation = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SevenToNineRuleMode {
    #[default]
    #[serde(rename = "7K")]
    Keys7,
    #[serde(rename = "9K")]
    Keys9,
}

impl SevenToNineRuleMode {
    pub const VALUES: [Self; 2] = [Self::Keys7, Self::Keys9];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keys7 => "7K",
            Self::Keys9 => "9K (NO SAVE)",
        }
    }
}

impl SevenToNineType {
    pub const VALUES: [Self; 3] = [Self::Fixed, Self::NoMashing, Self::Alternation];

    pub const fn value(self) -> u8 {
        self as u8
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Fixed => "FIXED",
            Self::NoMashing => "NO MASHING",
            Self::Alternation => "ALTERNATION",
        }
    }

    pub const fn next(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::Fixed, true) | (Self::Alternation, false) => Self::NoMashing,
            (Self::NoMashing, true) | (Self::Fixed, false) => Self::Alternation,
            (Self::Alternation, true) | (Self::NoMashing, false) => Self::Fixed,
        }
    }
}

impl TryFrom<u8> for SevenToNineType {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::VALUES
            .into_iter()
            .find(|kind| kind.value() == value)
            .ok_or_else(|| format!("sevenToNineType must be in 0..=2, got {value}"))
    }
}

impl From<SevenToNineType> for u8 {
    fn from(value: SevenToNineType) -> Self {
        value.value()
    }
}

pub fn default_play_exit_hold_ms() -> u32 {
    1_000
}

pub fn default_bga_mode() -> BgaModeConfig {
    BgaModeConfig::On
}

pub fn default_bga_expand() -> BgaExpandConfig {
    BgaExpandConfig::KeepAspect
}

pub fn default_misslayer_duration_ms() -> u32 {
    500
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum BgaModeConfig {
    On,
    Auto,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum BgaExpandConfig {
    Full,
    KeepAspect,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GaugeTypeConfig {
    AssistEasy,
    Easy,
    Normal,
    Hard,
    ExHard,
    /// Legacy in-development value. New configs should use `gauge_auto_shift`.
    AutoShift,
    Hazard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GaugeAutoShiftConfig {
    #[default]
    Off,
    Continue,
    HardToGroove,
    BestClear,
    SelectToUnder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum BottomShiftableGaugeConfig {
    #[default]
    AssistEasy,
    Easy,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RandomOptionConfig {
    #[default]
    Off,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DoubleOptionConfig {
    #[default]
    Off,
    Flip,
    Battle,
    #[serde(alias = "BattleAssist")]
    BattleAutoScratch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HsFixConfig {
    #[default]
    Off,
    StartBpm,
    MinBpm,
    MaxBpm,
    MainBpm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetOptionConfig {
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

impl TargetOptionConfig {
    pub fn as_persistent_str(self) -> String {
        match self {
            Self::None => "None".to_string(),
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
            Self::RivalTop => "RIVAL_TOP".to_string(),
            Self::RivalNext => "RIVAL_NEXT".to_string(),
            Self::RivalIndex(index) => format!("RIVAL_{index}"),
        }
    }

    fn from_persistent_str(value: &str) -> Self {
        match value {
            "None" | "NONE" | "Off" | "OFF" => Self::None,
            "RANK_A" | "A" => Self::RankA,
            "RANK_AA-" | "AA-" => Self::RankAaMinus,
            "RANK_AA" | "AA" | "Aa" => Self::RankAa,
            "RANK_AAA-" | "AAA-" => Self::RankAaaMinus,
            "RANK_AAA" | "AAA" | "Aaa" => Self::RankAaa,
            "RANK_MAX-" | "MAX-" => Self::RankMaxMinus,
            "MAX" | "Max" => Self::Max,
            "RANK_NEXT" | "RankNext" => Self::RankNext,
            "IR_TOP" | "IrTop" => Self::IrTop,
            "IR_NEXT" | "IrNext" => Self::IrNext,
            "RIVAL_TOP" | "RIVAL TOP" | "Rival" | "RivalTop" => Self::RivalTop,
            "RIVAL_NEXT" | "RIVAL NEXT" | "RivalNext" => Self::RivalNext,
            "B" | "C" | "D" | "E" => Self::RankA,
            other => other
                .strip_prefix("RIVAL_")
                .and_then(|index| index.parse::<u8>().ok())
                .filter(|&index| index > 0)
                .map(Self::RivalIndex)
                .unwrap_or_default(),
        }
    }
}

impl Serialize for TargetOptionConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.as_persistent_str())
    }
}

impl<'de> Deserialize<'de> for TargetOptionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_persistent_str(&value))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LaneEffectConfig {
    #[default]
    Off,
    Hidden,
    Sudden,
    HiddenSudden,
}

impl LaneEffectConfig {
    pub const fn sudden_enabled(self) -> bool {
        matches!(self, Self::Sudden | Self::HiddenSudden)
    }

    pub const fn hidden_enabled(self) -> bool {
        matches!(self, Self::Hidden | Self::HiddenSudden)
    }

    pub const fn from_enabled(sudden_enabled: bool, hidden_enabled: bool) -> Self {
        match (sudden_enabled, hidden_enabled) {
            (false, false) => Self::Off,
            (false, true) => Self::Hidden,
            (true, false) => Self::Sudden,
            (true, true) => Self::HiddenSudden,
        }
    }

    pub const fn with_sudden_enabled(self, enabled: bool) -> Self {
        Self::from_enabled(enabled, self.hidden_enabled())
    }

    pub const fn with_hidden_enabled(self, enabled: bool) -> Self {
        Self::from_enabled(self.sudden_enabled(), enabled)
    }
}

/// beatoraja `PlayerConfig` のアシスト設定。
///
/// 選曲画面の7トグルは同時に有効化できるため、旧来の単一 enum ではなく
/// 独立フラグと汎用 modifier mode を保持する。旧 profile の `None` /
/// `AutoScratch` / `LegacyNote` 文字列も [`Deserialize`] で受け付ける。
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AssistOptionConfig {
    #[serde(default)]
    pub expand_judge: bool,
    #[serde(default)]
    pub judge_area: bool,
    #[serde(default)]
    pub mark_note: bool,
    #[serde(default)]
    pub bpm_guide: bool,
    #[serde(default)]
    pub scroll_mode: AssistScrollMode,
    #[serde(default)]
    pub long_note_mode: AssistLongNoteMode,
    #[serde(default)]
    pub mine_mode: AssistMineMode,
    #[serde(default = "default_assist_judge_pgreat_rate")]
    pub key_pgreat_rate: u16,
    #[serde(default = "default_assist_judge_great_rate")]
    pub key_great_rate: u16,
    #[serde(default = "default_assist_judge_good_rate")]
    pub key_good_rate: u16,
    #[serde(default = "default_assist_judge_pgreat_rate")]
    pub scratch_pgreat_rate: u16,
    #[serde(default = "default_assist_judge_great_rate")]
    pub scratch_great_rate: u16,
    #[serde(default = "default_assist_judge_good_rate")]
    pub scratch_good_rate: u16,
    #[serde(default = "default_assist_long_note_margin_rate")]
    pub long_note_margin_rate: u16,
    #[serde(default = "default_assist_scroll_section")]
    pub scroll_section: u16,
    #[serde(default = "default_assist_scroll_rate")]
    pub scroll_rate: f64,
    #[serde(default = "default_assist_long_note_rate")]
    pub long_note_rate: f64,
    #[serde(default)]
    pub extra_note_type: u8,
    #[serde(default)]
    pub extra_note_depth: u8,
    #[serde(default)]
    pub extra_note_scratch: bool,
}

impl AssistOptionConfig {
    pub fn flags(self) -> [bool; 7] {
        [
            self.expand_judge,
            self.scroll_mode == AssistScrollMode::Remove,
            self.judge_area,
            self.long_note_mode == AssistLongNoteMode::Remove,
            self.mark_note,
            self.bpm_guide,
            self.mine_mode == AssistMineMode::Remove,
        ]
    }

    pub fn any_enabled(self) -> bool {
        self.flags().into_iter().any(|enabled| enabled)
            || self.scroll_mode != AssistScrollMode::Off
            || self.long_note_mode != AssistLongNoteMode::Off
            || self.mine_mode != AssistMineMode::Off
            || self.extra_note_depth > 0
    }

    pub fn toggle_beatoraja_button(&mut self, button_id: i32) -> bool {
        match button_id {
            301 => self.expand_judge = !self.expand_judge,
            302 => {
                self.scroll_mode = if self.scroll_mode == AssistScrollMode::Remove {
                    AssistScrollMode::Off
                } else {
                    AssistScrollMode::Remove
                };
            }
            303 => self.judge_area = !self.judge_area,
            304 => {
                self.long_note_mode = if self.long_note_mode == AssistLongNoteMode::Remove {
                    AssistLongNoteMode::Off
                } else {
                    AssistLongNoteMode::Remove
                };
            }
            305 => self.mark_note = !self.mark_note,
            306 => self.bpm_guide = !self.bpm_guide,
            307 => {
                self.mine_mode = if self.mine_mode == AssistMineMode::Remove {
                    AssistMineMode::Off
                } else {
                    AssistMineMode::Remove
                };
            }
            _ => return false,
        }
        true
    }
}

impl Default for AssistOptionConfig {
    fn default() -> Self {
        Self {
            expand_judge: false,
            judge_area: false,
            mark_note: false,
            bpm_guide: false,
            scroll_mode: AssistScrollMode::Off,
            long_note_mode: AssistLongNoteMode::Off,
            mine_mode: AssistMineMode::Off,
            key_pgreat_rate: default_assist_judge_pgreat_rate(),
            key_great_rate: default_assist_judge_great_rate(),
            key_good_rate: default_assist_judge_good_rate(),
            scratch_pgreat_rate: default_assist_judge_pgreat_rate(),
            scratch_great_rate: default_assist_judge_great_rate(),
            scratch_good_rate: default_assist_judge_good_rate(),
            long_note_margin_rate: default_assist_long_note_margin_rate(),
            scroll_section: default_assist_scroll_section(),
            scroll_rate: default_assist_scroll_rate(),
            long_note_rate: default_assist_long_note_rate(),
            extra_note_type: 0,
            extra_note_depth: 0,
            extra_note_scratch: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AssistScrollMode {
    #[default]
    Off,
    Remove,
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AssistLongNoteMode {
    #[default]
    Off,
    Remove,
    AddLn,
    AddCn,
    AddHcn,
    AddAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AssistMineMode {
    #[default]
    Off,
    Remove,
    AddRandom,
    AddNear,
    AddBlank,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AssistOptionConfigRepr {
    Legacy(LegacyAssistOptionConfig),
    Current(AssistOptionConfigFields),
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
enum LegacyAssistOptionConfig {
    None,
    AutoScratch,
    LegacyNote,
}

#[derive(Deserialize, Default)]
struct AssistOptionConfigFields {
    #[serde(default)]
    expand_judge: bool,
    #[serde(default)]
    judge_area: bool,
    #[serde(default)]
    mark_note: bool,
    #[serde(default)]
    bpm_guide: bool,
    #[serde(default)]
    scroll_mode: AssistScrollMode,
    #[serde(default)]
    long_note_mode: AssistLongNoteMode,
    #[serde(default)]
    mine_mode: AssistMineMode,
    #[serde(default = "default_assist_judge_pgreat_rate")]
    key_pgreat_rate: u16,
    #[serde(default = "default_assist_judge_great_rate")]
    key_great_rate: u16,
    #[serde(default = "default_assist_judge_good_rate")]
    key_good_rate: u16,
    #[serde(default = "default_assist_judge_pgreat_rate")]
    scratch_pgreat_rate: u16,
    #[serde(default = "default_assist_judge_great_rate")]
    scratch_great_rate: u16,
    #[serde(default = "default_assist_judge_good_rate")]
    scratch_good_rate: u16,
    #[serde(default = "default_assist_long_note_margin_rate")]
    long_note_margin_rate: u16,
    #[serde(default = "default_assist_scroll_section")]
    scroll_section: u16,
    #[serde(default = "default_assist_scroll_rate")]
    scroll_rate: f64,
    #[serde(default = "default_assist_long_note_rate")]
    long_note_rate: f64,
    #[serde(default)]
    extra_note_type: u8,
    #[serde(default)]
    extra_note_depth: u8,
    #[serde(default)]
    extra_note_scratch: bool,
}

impl<'de> Deserialize<'de> for AssistOptionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = AssistOptionConfigRepr::deserialize(deserializer)?;
        Ok(match repr {
            AssistOptionConfigRepr::Legacy(LegacyAssistOptionConfig::LegacyNote) => {
                Self { long_note_mode: AssistLongNoteMode::Remove, ..Self::default() }
            }
            AssistOptionConfigRepr::Legacy(
                LegacyAssistOptionConfig::None | LegacyAssistOptionConfig::AutoScratch,
            ) => Self::default(),
            AssistOptionConfigRepr::Current(fields) => Self {
                expand_judge: fields.expand_judge,
                judge_area: fields.judge_area,
                mark_note: fields.mark_note,
                bpm_guide: fields.bpm_guide,
                scroll_mode: fields.scroll_mode,
                long_note_mode: fields.long_note_mode,
                mine_mode: fields.mine_mode,
                key_pgreat_rate: fields.key_pgreat_rate.clamp(25, 400),
                key_great_rate: fields.key_great_rate.min(400),
                key_good_rate: fields.key_good_rate.min(400),
                scratch_pgreat_rate: fields.scratch_pgreat_rate.clamp(25, 400),
                scratch_great_rate: fields.scratch_great_rate.min(400),
                scratch_good_rate: fields.scratch_good_rate.min(400),
                long_note_margin_rate: fields.long_note_margin_rate.min(400),
                scroll_section: fields.scroll_section.clamp(1, 1024),
                scroll_rate: if fields.scroll_rate.is_finite() {
                    fields.scroll_rate.clamp(0.0, 1.0)
                } else {
                    default_assist_scroll_rate()
                },
                long_note_rate: if fields.long_note_rate.is_finite() {
                    fields.long_note_rate.clamp(0.0, 1.0)
                } else {
                    default_assist_long_note_rate()
                },
                extra_note_type: fields.extra_note_type,
                extra_note_depth: fields.extra_note_depth.min(100),
                extra_note_scratch: fields.extra_note_scratch,
            },
        })
    }
}

pub const fn default_assist_judge_pgreat_rate() -> u16 {
    400
}

pub const fn default_assist_judge_great_rate() -> u16 {
    400
}

pub const fn default_assist_judge_good_rate() -> u16 {
    100
}

pub const fn default_assist_long_note_margin_rate() -> u16 {
    100
}

pub const fn default_assist_scroll_section() -> u16 {
    4
}

pub const fn default_assist_scroll_rate() -> f64 {
    0.5
}

pub const fn default_assist_long_note_rate() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeConfig {
    pub input_offset_us: i64,
    #[serde(default)]
    pub visual_offset_us: i64,
    #[serde(default)]
    pub visual_offset_auto_adjust: bool,
    pub judge_algorithm: JudgeAlgorithmConfig,
    /// FAST/SLOW を表示する最小タイミング差(ms)。|delta| がこれ未満なら FAST/SLOW 表示なし。0=常時表示。
    #[serde(default)]
    pub fast_slow_display_threshold_ms: u32,
    /// FAST/SLOW を表示する判定範囲。PGREAT を除外するなど。
    #[serde(default)]
    pub fast_slow_display_scope: FastSlowDisplayScope,
}

/// FAST/SLOW 表示モード。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FastSlowDisplayScope {
    /// beatoraja 準拠。PGREAT は FAST/SLOW を表示せず、GREAT 以下は常時表示。
    /// fast_slow_display_threshold_ms は無視される。
    #[default]
    Auto,
    /// PGREAT は |delta| >= fast_slow_display_threshold_ms のときのみ表示。
    /// GREAT 以下は常時表示し、threshold_ms = 0 なら PGREAT も常時表示。
    ThresholdMs,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum JudgeAlgorithmConfig {
    Combo,
    #[serde(alias = "Score")]
    Duration,
    Lowest,
}

impl JudgeAlgorithmConfig {
    /// beatoraja skin / launcher order.
    pub const ORDER: [Self; 3] = [Self::Combo, Self::Duration, Self::Lowest];

    pub const fn beatoraja_name(self) -> &'static str {
        match self {
            Self::Combo => "Combo",
            Self::Duration => "Duration",
            Self::Lowest => "Lowest",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaneViewConfig {
    #[serde(rename = "classic_hispeed", alias = "hispeed")]
    pub hispeed: f32,
    #[serde(default = "default_base_hispeed")]
    pub base_hispeed: BaseHispeedConfig,
    #[serde(default = "default_floating_policy")]
    pub floating_policy: FloatingPolicyConfig,
    #[serde(default = "default_normal_hispeed_level")]
    pub normal_hispeed_level: u8,
    /// Classic のプレイ中 HS 変更刻み。0.05..=1.0 の範囲で持つ。
    #[serde(default = "default_classic_hispeed_step", alias = "hispeed_step_nhs")]
    pub classic_hispeed_step: f32,
    /// Floating のプレイ中 HS 変更刻み。0.05..=1.0 の範囲で持つ。
    #[serde(default = "default_floating_hispeed_step", alias = "hispeed_step_fhs")]
    pub floating_hispeed_step: f32,
    /// SUDDEN+ レーンカバー量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub sudden: u32,
    /// LIFT 量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub lift: u32,
    /// beatoraja `PlayConfig.enablelift` 相当。古いprofileは従来挙動を保つため有効扱い。
    #[serde(default = "default_true")]
    pub lift_enabled: bool,
    /// beatoraja `PlayConfig.hispeedautoadjust` 相当。
    #[serde(default = "default_true")]
    pub hispeed_auto_adjust: bool,
    /// HIDDEN レーンカバー量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub hidden: u32,
    #[serde(rename = "floating_target_green", alias = "target_green_number")]
    pub target_green_number: u32,
    #[serde(default)]
    pub constant_enabled: bool,
    #[serde(default = "default_constant_fade_ms")]
    pub constant_fade_ms: i32,
}

impl LaneViewConfig {
    pub const fn hispeed_config(&self) -> HispeedConfigPreset {
        HispeedConfigPreset::from_parts(self.base_hispeed, self.floating_policy)
    }

    pub fn set_hispeed_config(&mut self, preset: HispeedConfigPreset) {
        (self.base_hispeed, self.floating_policy) = preset.parts();
    }
}

/// Per-key-mode values corresponding to beatoraja/LR2orajaED `PlayConfig`.
/// Global input latency and adjustment step sizes remain outside this type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayModeConfig {
    #[serde(default = "default_mode_hispeed", rename = "classic_hispeed", alias = "hispeed")]
    pub hispeed: f32,
    #[serde(default = "default_base_hispeed")]
    pub base_hispeed: BaseHispeedConfig,
    #[serde(default = "default_floating_policy")]
    pub floating_policy: FloatingPolicyConfig,
    #[serde(default = "default_normal_hispeed_level")]
    pub normal_hispeed_level: u8,
    #[serde(default)]
    pub hs_fix: HsFixConfig,
    #[serde(default)]
    pub lane_effect: LaneEffectConfig,
    #[serde(default)]
    pub sudden: u32,
    #[serde(default)]
    pub lift: u32,
    #[serde(default = "default_true")]
    pub lift_enabled: bool,
    #[serde(default = "default_true")]
    pub hispeed_auto_adjust: bool,
    #[serde(default)]
    pub hidden: u32,
    #[serde(
        default = "default_target_green_number",
        rename = "floating_target_green",
        alias = "target_green_number"
    )]
    pub target_green_number: u32,
    #[serde(default)]
    pub constant_enabled: bool,
    #[serde(default = "default_constant_fade_ms")]
    pub constant_fade_ms: i32,
    #[serde(default)]
    pub visual_offset_us: i64,
}

impl Default for PlayModeConfig {
    fn default() -> Self {
        Self {
            hispeed: default_mode_hispeed(),
            base_hispeed: default_base_hispeed(),
            floating_policy: default_floating_policy(),
            normal_hispeed_level: default_normal_hispeed_level(),
            hs_fix: HsFixConfig::Off,
            lane_effect: LaneEffectConfig::Off,
            sudden: 0,
            lift: 0,
            lift_enabled: true,
            hispeed_auto_adjust: true,
            hidden: 0,
            target_green_number: default_target_green_number(),
            constant_enabled: false,
            constant_fade_ms: default_constant_fade_ms(),
            visual_offset_us: 0,
        }
    }
}

impl PlayModeConfig {
    pub const fn hispeed_config(&self) -> HispeedConfigPreset {
        HispeedConfigPreset::from_parts(self.base_hispeed, self.floating_policy)
    }
}

pub const fn default_mode_hispeed() -> f32 {
    2.0
}

pub const fn default_target_green_number() -> u32 {
    300
}

pub const fn default_constant_fade_ms() -> i32 {
    100
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum BaseHispeedConfig {
    Normal,
    Classic,
}

pub(super) const fn default_base_hispeed() -> BaseHispeedConfig {
    BaseHispeedConfig::Classic
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FloatingPolicyConfig {
    Disabled,
    Toggle,
    Locked,
}

pub(super) const fn default_floating_policy() -> FloatingPolicyConfig {
    FloatingPolicyConfig::Toggle
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HispeedConfigPreset {
    Normal,
    Classic,
    Floating,
    NormalFloating,
    ClassicFloating,
}

impl HispeedConfigPreset {
    pub const ORDER: [Self; 5] =
        [Self::Normal, Self::Classic, Self::Floating, Self::NormalFloating, Self::ClassicFloating];

    pub const fn from_parts(base: BaseHispeedConfig, floating: FloatingPolicyConfig) -> Self {
        match (base, floating) {
            (_, FloatingPolicyConfig::Locked) => Self::Floating,
            (BaseHispeedConfig::Normal, FloatingPolicyConfig::Disabled) => Self::Normal,
            (BaseHispeedConfig::Classic, FloatingPolicyConfig::Disabled) => Self::Classic,
            (BaseHispeedConfig::Normal, FloatingPolicyConfig::Toggle) => Self::NormalFloating,
            (BaseHispeedConfig::Classic, FloatingPolicyConfig::Toggle) => Self::ClassicFloating,
        }
    }

    pub const fn parts(self) -> (BaseHispeedConfig, FloatingPolicyConfig) {
        match self {
            Self::Normal => (BaseHispeedConfig::Normal, FloatingPolicyConfig::Disabled),
            Self::Classic => (BaseHispeedConfig::Classic, FloatingPolicyConfig::Disabled),
            Self::Floating => (BaseHispeedConfig::Classic, FloatingPolicyConfig::Locked),
            Self::NormalFloating => (BaseHispeedConfig::Normal, FloatingPolicyConfig::Toggle),
            Self::ClassicFloating => (BaseHispeedConfig::Classic, FloatingPolicyConfig::Toggle),
        }
    }

    pub const fn index(self) -> i32 {
        match self {
            Self::Normal => 0,
            Self::Classic => 1,
            Self::Floating => 2,
            Self::NormalFloating => 3,
            Self::ClassicFloating => 4,
        }
    }

    pub const fn supports_normal(self) -> bool {
        matches!(self, Self::Normal | Self::NormalFloating)
    }

    pub const fn supports_classic(self) -> bool {
        matches!(self, Self::Classic | Self::ClassicFloating)
    }

    pub const fn supports_floating(self) -> bool {
        matches!(self, Self::Floating | Self::NormalFloating | Self::ClassicFloating)
    }
}

pub const fn default_normal_hispeed_level() -> u8 {
    18
}

pub const HISPEED_STEP_MIN: f32 = 0.05;
pub const HISPEED_STEP_MAX: f32 = 1.0;

pub fn default_classic_hispeed_step() -> f32 {
    0.25
}

pub fn default_floating_hispeed_step() -> f32 {
    0.50
}

pub fn normalize_hispeed_step(value: f32, default: f32) -> f32 {
    if value.is_finite() { value.clamp(HISPEED_STEP_MIN, HISPEED_STEP_MAX) } else { default }
}
