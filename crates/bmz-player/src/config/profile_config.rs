use std::collections::BTreeMap;

use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::ResultGradeDiffDisplay;
use serde::{Deserialize, Serialize};

use crate::ln_policy::LnPolicySetting;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub version: u32,
    pub id: String,
    pub display_name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub play: PlayDefaultsConfig,
    pub judge: JudgeConfig,
    pub lane: LaneViewConfig,
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

/// 選曲画面の表示状態。フィルター (5K/7K など) とソートを永続化する。
/// 値は app 層の `SelectModeFilter` / `SelectSort` の `as_str()` を文字列で保持し、
/// 読込時に未知の値なら既定へフォールバックする。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectStateConfig {
    #[serde(default = "default_select_mode_filter")]
    pub mode_filter: String,
    #[serde(default = "default_select_sort")]
    pub sort: String,
    #[serde(default)]
    pub random_select: bool,
}

pub fn default_select_mode_filter() -> String {
    "ALL".to_string()
}

pub fn default_select_sort() -> String {
    "TITLE".to_string()
}

impl Default for SelectStateConfig {
    fn default() -> Self {
        Self {
            mode_filter: default_select_mode_filter(),
            sort: default_select_sort(),
            random_select: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayDefaultsConfig {
    #[serde(default)]
    pub rule_mode: RuleMode,
    #[serde(default)]
    pub ln_mode_policy: LnPolicySetting,
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
    #[serde(default)]
    pub grade_diff_display: ResultGradeDiffDisplay,
    pub lane_effect: LaneEffectConfig,
    pub assist: AssistOptionConfig,
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
    /// LN モードでも終端 (tail) キャップを描画するか。
    /// beatoraja は LN モードで tail キャップを描画しないため既定 OFF。
    #[serde(default)]
    pub show_ln_tail_cap: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LaneEffectConfig {
    Off,
    Hidden,
    Sudden,
    HiddenSudden,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AssistOptionConfig {
    None,
    AutoScratch,
    LegacyNote,
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
    /// 判定種別を問わず、|delta| >= fast_slow_display_threshold_ms のときのみ表示。
    /// PGREAT も対象になる。threshold_ms = 0 なら全判定で常時表示。
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
    pub hispeed: f32,
    #[serde(default = "default_hispeed_mode")]
    pub hispeed_mode: HispeedModeConfig,
    /// NHS のプレイ中 HS 変更刻み。0.05..=1.0 の範囲で持つ。
    #[serde(default = "default_hispeed_step_nhs")]
    pub hispeed_step_nhs: f32,
    /// FHS のプレイ中 HS 変更刻み。0.05..=1.0 の範囲で持つ。
    #[serde(default = "default_hispeed_step_fhs")]
    pub hispeed_step_fhs: f32,
    /// SUDDEN+ レーンカバー量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub sudden: u32,
    /// LIFT 量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub lift: u32,
    /// beatoraja `PlayConfig.enablelift` 相当。古いprofileは従来挙動を保つため有効扱い。
    #[serde(default = "default_true")]
    pub lift_enabled: bool,
    /// beatoraja `PlayConfig.hispeedautoadjust` 相当。
    #[serde(default)]
    pub hispeed_auto_adjust: bool,
    /// HIDDEN レーンカバー量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub hidden: u32,
    pub target_green_number: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HispeedModeConfig {
    Normal,
    Floating,
}

fn default_hispeed_mode() -> HispeedModeConfig {
    HispeedModeConfig::Normal
}

pub const HISPEED_STEP_MIN: f32 = 0.05;
pub const HISPEED_STEP_MAX: f32 = 1.0;

pub fn default_hispeed_step_nhs() -> f32 {
    0.25
}

pub fn default_hispeed_step_fhs() -> f32 {
    0.50
}

pub fn normalize_hispeed_step(value: f32, default: f32) -> f32 {
    if value.is_finite() { value.clamp(HISPEED_STEP_MIN, HISPEED_STEP_MAX) } else { default }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiInputConfig {
    #[serde(default = "default_ui_bindings")]
    pub bindings: Vec<BindingConfigEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayModeInputConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit: Option<String>,
    #[serde(default)]
    pub bindings: Vec<BindingConfigEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInputConfig {
    pub scratch_mode: ScratchInputMode,
    #[serde(default)]
    pub select_input_mode: SelectInputModeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_key: Option<String>,
    #[serde(default)]
    pub ui: UiInputConfig,
    #[serde(default)]
    pub play: BTreeMap<String, PlayModeInputConfig>,
    /// 旧 `[[input.bindings]]` (lane + action 混在)。読込時のみ。保存時は出力しない。
    #[serde(default, rename = "bindings", skip_serializing)]
    pub legacy_bindings: Vec<BindingConfigEntry>,
    #[serde(default = "default_analog_scratch_sensitivity")]
    pub analog_scratch_sensitivity: f32,
    /// 旧アナログ皿の壁時計タイムアウト。読込互換だけに残し、保存時は出力しない。
    #[serde(default = "default_analog_scratch_timeout_ms", skip_serializing)]
    pub analog_scratch_timeout_ms: u32,
    /// beatoraja の analogScratchThreshold 相当。既定は Version2 向けの 100。
    #[serde(default = "default_analog_scratch_threshold")]
    pub analog_scratch_threshold: u32,
    /// 選曲画面でアナログスクラッチ何 tick ごとにカーソルを 1 つ動かすか (beatoraja の analogTicksPerScroll)。
    #[serde(default = "default_analog_ticks_per_scroll")]
    pub analog_ticks_per_scroll: u32,
}

fn default_analog_scratch_sensitivity() -> f32 {
    1.0
}

fn default_analog_scratch_timeout_ms() -> u32 {
    500
}

pub fn default_analog_scratch_threshold() -> u32 {
    100
}

fn default_analog_ticks_per_scroll() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingConfigEntry {
    pub device: String,
    pub control: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<LaneConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<InputActionConfig>,
    /// スクラッチレーンの回転方向。コントロール名からの推測 (`+`/`-` 等) に
    /// 依存せず方向を確定させるため、キーコンフィグで設定した entry に保存する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch: Option<ScratchDirectionConfig>,
}

/// スクラッチバインドの方向タグ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScratchDirectionConfig {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum InputActionConfig {
    E1,
    #[serde(rename = "Enter")]
    SelectEnter,
    E2,
    E3,
    E4,
    #[serde(rename = "OptionArrange")]
    SelectOptionArrange,
    #[serde(rename = "OptionGauge")]
    SelectOptionGauge,
    #[serde(rename = "OptionAssist")]
    SelectOptionAssist,
    #[serde(rename = "OptionBga")]
    SelectOptionBga,
    #[serde(rename = "FavoriteSong")]
    SelectFavoriteSong,
    #[serde(rename = "FavoriteChart")]
    SelectFavoriteChart,
    #[serde(rename = "SameFolder")]
    SelectSameFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ScratchInputMode {
    Normal,
    AnyDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelectInputModeConfig {
    #[default]
    #[serde(rename = "7K14K")]
    Key7Key14,
    #[serde(rename = "9K")]
    Key9,
}

impl SelectInputModeConfig {
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Key7Key14 => "7K/14K",
            Self::Key9 => "9K",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LaneConfig {
    Scratch,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    // 2P lanes for 10K/14K
    Scratch2,
    Key8,
    Key9,
    Key10,
    Key11,
    Key12,
    Key13,
    Key14,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RivalConfig {
    pub active_rival: String,
    pub entries: Vec<RivalEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RivalEntry {
    pub id: String,
    pub display_name: String,
    pub source: RivalSourceConfig,
    pub profile_id: String,
    pub path: String,
    pub ir_service: String,
    pub ir_user_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RivalSourceConfig {
    None,
    LocalProfile,
    ExternalFile,
    Ir,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplaySlotRule {
    #[serde(rename = "")]
    Disabled,
    Always,
    ScoreUpdate,
    BpUpdate,
    MaxComboUpdate,
    ClearUpdate,
}

impl ReplaySlotRule {
    pub const CYCLE_ORDER: [Self; 6] = [
        Self::Disabled,
        Self::Always,
        Self::ScoreUpdate,
        Self::BpUpdate,
        Self::MaxComboUpdate,
        Self::ClearUpdate,
    ];

    /// beatoraja `ReplayAutoSaveConstraint` / launcher autosave combo row for
    /// IndexType `autosave_replay1..4` image refs.
    pub fn image_index(self) -> i64 {
        match self {
            Self::Disabled => 0,
            Self::ScoreUpdate => 1,
            Self::BpUpdate => 3,
            Self::MaxComboUpdate => 5,
            Self::ClearUpdate => 7,
            Self::Always => 10,
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        let index = Self::CYCLE_ORDER.iter().position(|rule| *rule == self).unwrap_or(0);
        if forward {
            Self::CYCLE_ORDER[(index + 1) % Self::CYCLE_ORDER.len()]
        } else {
            Self::CYCLE_ORDER[(index + Self::CYCLE_ORDER.len() - 1) % Self::CYCLE_ORDER.len()]
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "",
            Self::Always => "Always",
            Self::ScoreUpdate => "ScoreUpdate",
            Self::BpUpdate => "BpUpdate",
            Self::MaxComboUpdate => "MaxComboUpdate",
            Self::ClearUpdate => "ClearUpdate",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "" => Some(Self::Disabled),
            "Always" => Some(Self::Always),
            "ScoreUpdate" => Some(Self::ScoreUpdate),
            "BpUpdate" => Some(Self::BpUpdate),
            "MaxComboUpdate" => Some(Self::MaxComboUpdate),
            "ClearUpdate" => Some(Self::ClearUpdate),
            _ => None,
        }
    }
}

pub fn default_slot_rules() -> [ReplaySlotRule; 4] {
    [
        ReplaySlotRule::Always,
        ReplaySlotRule::ScoreUpdate,
        ReplaySlotRule::BpUpdate,
        ReplaySlotRule::Disabled,
    ]
}

pub fn replay_slot_rule_indices(rules: &[ReplaySlotRule; 4]) -> [i64; 4] {
    [rules[0].image_index(), rules[1].image_index(), rules[2].image_index(), rules[3].image_index()]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    pub auto_save: bool,
    pub compress: bool,
    #[serde(default = "default_slot_rules")]
    pub slot_rules: [ReplaySlotRule; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub language: String,
    pub theme: String,
    pub show_fps: bool,
    pub confirm_on_exit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMixConfig {
    #[serde(default)]
    pub normalize_chart_volume: bool,
    /// マスターボリューム。0..=100 の整数で持ち、ランタイムでは /100 して扱う。
    pub master_volume: u32,
    /// キーボリューム。0..=100 の整数で持ち、ランタイムでは /100 して扱う。
    pub key_volume: u32,
    /// BGM ボリューム。0..=100 の整数で持ち、ランタイムでは /100 して扱う。
    pub bgm_volume: u32,
    /// 選曲プレビューのボリューム。0..=100 の整数で持ち、ランタイムでは /100 して扱う。
    pub preview_volume: u32,
    /// システム BGM (Select / Decide) のボリューム。0..=100 の整数。
    #[serde(default = "default_system_bgm_volume")]
    pub system_bgm_volume: u32,
    /// システム SE のボリューム。0..=100 の整数。
    #[serde(default = "default_system_se_volume")]
    pub system_se_volume: u32,
}

pub fn default_system_bgm_volume() -> u32 {
    50
}

pub fn default_system_se_volume() -> u32 {
    50
}

/// beatoraja 互換のシステム SE / BGM (選曲 BGM、フォルダ SE 等) の設定。
/// 旧来 `[audio]` (config.toml) ではなく、ユーザーごとに切り替えたい設定として
/// profile.toml の `[system_sound]` に配置する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSoundConfig {
    /// システム BGM セットのルート(`select.wav` を含むディレクトリの親)。
    /// 空文字列ならスキャンせず、`default_sound_dir` だけを参照する。
    #[serde(default)]
    pub bgm_dir: String,
    /// システム SE セットのルート(`clear.wav` を含むディレクトリの親)。
    /// 空文字列ならスキャンせず、`default_sound_dir` だけを参照する。
    #[serde(default)]
    pub se_dir: String,
    /// 各システム音のフォールバック先(beatoraja 既定の `defaultsound/` 相当)。
    #[serde(default = "default_system_sound_default_dir")]
    pub default_sound_dir: String,
}

pub fn default_system_sound_default_dir() -> String {
    "data/defaultsound".to_string()
}

impl Default for SystemSoundConfig {
    fn default() -> Self {
        Self {
            bgm_dir: "data/bgm".to_string(),
            se_dir: "data/se".to_string(),
            default_sound_dir: default_system_sound_default_dir(),
        }
    }
}

/// スキン設定。スキンはプロファイルごとに切り替えられる。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkinConfig {
    /// 選曲画面スキンのパス。
    /// 空文字列なら bmz の固定描画を使用。
    /// `.json` / `.lr2skin` で終わるパスは beatoraja スキンとして扱う。
    #[serde(default)]
    pub select: String,
    /// 決定画面スキンのパス。
    /// 空文字列ならプレイ開始前もプレイスキン側の描画を使用。
    /// `.json` / `.luaskin` / `.lua` / `.lr2skin` で終わるパスは beatoraja スキンとして扱う。
    #[serde(default)]
    pub decide: String,
    /// 5K プレイ画面スキンのパス。
    /// 空文字列なら内蔵デフォルトスキンを使用。
    /// `.json` / `.luaskin` / `.lua` / `.lr2skin` で終わるパスは beatoraja スキンとして扱う。
    #[serde(default)]
    pub play5: String,
    /// 4K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play4: String,
    /// 6K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play6: String,
    /// 7K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play7: String,
    /// 8K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play8: String,
    /// 10K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play10: String,
    /// 14K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play14: String,
    /// 9K プレイ画面スキンのパス (PMS / Pop'n)。フォーマットは [`play5`] と同じ。
    /// 空文字列なら内蔵デフォルトスキンを使用。
    #[serde(default)]
    pub play9: String,
    /// リザルト画面スキンのパス。
    /// 空文字列なら bmz の固定描画を使用。
    /// `.json` / `.lr2skin` で終わるパスは beatoraja スキンとして扱う。
    #[serde(default)]
    pub result: String,
    /// コース最終リザルト画面スキンのパス。
    /// 空文字列なら bmz の固定描画を使用。
    /// `.json` / `.lr2skin` で終わるパスは beatoraja スキンとして扱う。
    #[serde(default)]
    pub course_result: String,
    /// 選曲スキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub select_offsets: Vec<SkinOffsetConfig>,
    /// 決定スキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decide_offsets: Vec<SkinOffsetConfig>,
    /// 4K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play4_offsets: Vec<SkinOffsetConfig>,
    /// 5K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play5_offsets: Vec<SkinOffsetConfig>,
    /// 6K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play6_offsets: Vec<SkinOffsetConfig>,
    /// 7K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play7_offsets: Vec<SkinOffsetConfig>,
    /// 8K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play8_offsets: Vec<SkinOffsetConfig>,
    /// 9K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play9_offsets: Vec<SkinOffsetConfig>,
    /// 10K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play10_offsets: Vec<SkinOffsetConfig>,
    /// 14K プレイスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub play14_offsets: Vec<SkinOffsetConfig>,
    /// リザルトスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_offsets: Vec<SkinOffsetConfig>,
    /// コースリザルトスキンのオフセット設定。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub course_result_offsets: Vec<SkinOffsetConfig>,
    /// v0.1.9 以前の全スロット共通オフセット。ロード時に各スロットへ移行する。
    #[serde(rename = "offsets", default, skip_serializing)]
    pub(crate) legacy_offsets: Vec<SkinOffsetConfig>,
    /// 選曲スキンのカスタマイズオプション選択 (オプション名 -> 選択肢名)。
    #[serde(default)]
    pub select_options: BTreeMap<String, String>,
    /// 決定スキンのカスタマイズオプション選択。
    #[serde(default)]
    pub decide_options: BTreeMap<String, String>,
    /// 5K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play5_options: BTreeMap<String, String>,
    /// 4K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play4_options: BTreeMap<String, String>,
    /// 6K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play6_options: BTreeMap<String, String>,
    /// 7K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play7_options: BTreeMap<String, String>,
    /// 8K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play8_options: BTreeMap<String, String>,
    /// 10K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play10_options: BTreeMap<String, String>,
    /// 14K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play14_options: BTreeMap<String, String>,
    /// 9K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play9_options: BTreeMap<String, String>,
    /// リザルトスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub result_options: BTreeMap<String, String>,
    /// コースリザルトスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub course_result_options: BTreeMap<String, String>,
    /// 選曲スキンのファイル選択 (filepath 定義名 -> 選択ファイルの相対パス)。
    #[serde(default)]
    pub select_files: BTreeMap<String, String>,
    /// 決定スキンのファイル選択。
    #[serde(default)]
    pub decide_files: BTreeMap<String, String>,
    /// 5K プレイスキンのファイル選択。
    #[serde(default)]
    pub play5_files: BTreeMap<String, String>,
    /// 4K プレイスキンのファイル選択。
    #[serde(default)]
    pub play4_files: BTreeMap<String, String>,
    /// 6K プレイスキンのファイル選択。
    #[serde(default)]
    pub play6_files: BTreeMap<String, String>,
    /// 7K プレイスキンのファイル選択。
    #[serde(default)]
    pub play7_files: BTreeMap<String, String>,
    /// 8K プレイスキンのファイル選択。
    #[serde(default)]
    pub play8_files: BTreeMap<String, String>,
    /// 10K プレイスキンのファイル選択。
    #[serde(default)]
    pub play10_files: BTreeMap<String, String>,
    /// 14K プレイスキンのファイル選択。
    #[serde(default)]
    pub play14_files: BTreeMap<String, String>,
    /// 9K プレイスキンのファイル選択。
    #[serde(default)]
    pub play9_files: BTreeMap<String, String>,
    /// リザルトスキンのファイル選択。
    #[serde(default)]
    pub result_files: BTreeMap<String, String>,
    /// コースリザルトスキンのファイル選択。
    #[serde(default)]
    pub course_result_files: BTreeMap<String, String>,
    /// スキンスロットとファイル path ごとのカスタマイズ履歴。
    ///
    /// beatoraja の `skinHistory` 相当。スキンを切り替えても、各スキンの
    /// option / filepath / offset を前回値へ戻せるように保持する。
    #[serde(default)]
    pub history: BTreeMap<String, SkinHistoryEntryConfig>,
}

impl SkinConfig {
    /// 旧形式の共通オフセットを、まだ個別設定がない全スロットへ引き継ぐ。
    pub fn migrate_legacy_offsets(&mut self) {
        let legacy_offsets = std::mem::take(&mut self.legacy_offsets);
        if legacy_offsets.is_empty() {
            return;
        }

        for offsets in [
            &mut self.select_offsets,
            &mut self.decide_offsets,
            &mut self.play4_offsets,
            &mut self.play5_offsets,
            &mut self.play6_offsets,
            &mut self.play7_offsets,
            &mut self.play8_offsets,
            &mut self.play9_offsets,
            &mut self.play10_offsets,
            &mut self.play14_offsets,
            &mut self.result_offsets,
            &mut self.course_result_offsets,
        ] {
            if offsets.is_empty() {
                *offsets = legacy_offsets.clone();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkinHistoryEntryConfig {
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub offsets: Vec<SkinOffsetConfig>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkinOffsetConfig {
    pub id: i32,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default)]
    pub r: i32,
    #[serde(default)]
    pub a: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrConfig {
    #[serde(default)]
    pub primary_provider: String,
    /// 秘密情報 (refresh token / device key) の保存先。
    /// 開発時の Keychain 許可ダイアログを避けるため既定はファイル保存。
    #[serde(default)]
    pub credential_store: IrCredentialStoreConfig,
    #[serde(default)]
    pub providers: Vec<IrProviderConfig>,
    #[serde(default = "default_true")]
    pub prefetch_global_ranking_on_score_submit: bool,
    #[serde(default = "default_true")]
    pub prefetch_rival_ranking_on_score_submit: bool,
}

fn default_true() -> bool {
    true
}

impl Default for IrConfig {
    fn default() -> Self {
        Self {
            primary_provider: String::new(),
            credential_store: IrCredentialStoreConfig::default(),
            providers: Vec::new(),
            prefetch_global_ranking_on_score_submit: true,
            prefetch_rival_ranking_on_score_submit: true,
        }
    }
}

/// IR 秘密情報の保存先。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum IrCredentialStoreConfig {
    /// プロファイル配下の JSON ファイル (unix では 0600)。
    #[default]
    File,
    /// OS credential store (Keychain / Credential Manager / Secret Service)。
    Os,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IrProviderConfig {
    pub provider: String,
    /// IR サーバーが返す provider key。credentials / device key / queued job の識別に使う。
    #[serde(default)]
    pub provider_key: String,
    /// IR サーバーの base URL (例: `https://ir.example.com`)。
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub account_display_name: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub send_policy: IrSendPolicyConfig,
    #[serde(default)]
    pub role: IrProviderRoleConfig,
    #[serde(default)]
    pub last_login_at: Option<i64>,
    #[serde(default)]
    pub last_success_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IrSendPolicyConfig {
    UpdateScore,
    #[default]
    Always,
    CompleteSong,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IrProviderRoleConfig {
    #[default]
    SubmitOnly,
    Primary,
}

impl ProfileConfig {
    pub fn new_default(id: &str, display_name: &str, now: i64) -> Self {
        Self {
            version: 1,
            id: id.to_string(),
            display_name: display_name.to_string(),
            created_at: now,
            updated_at: now,
            play: PlayDefaultsConfig {
                rule_mode: RuleMode::Beatoraja,
                ln_mode_policy: LnPolicySetting::AutoLn,
                gauge: GaugeTypeConfig::Normal,
                gauge_auto_shift: GaugeAutoShiftConfig::Off,
                bottom_shiftable_gauge: BottomShiftableGaugeConfig::AssistEasy,
                random: RandomOptionConfig::Off,
                random2: RandomOptionConfig::Off,
                double_option: DoubleOptionConfig::Off,
                hs_fix: HsFixConfig::Off,
                target: TargetOptionConfig::None,
                grade_diff_display: ResultGradeDiffDisplay::default(),
                lane_effect: LaneEffectConfig::Off,
                assist: AssistOptionConfig::None,
                auto_play: false,
                bga: default_bga_mode(),
                bga_expand: default_bga_expand(),
                misslayer_duration_ms: default_misslayer_duration_ms(),
                play_exit_hold_ms: default_play_exit_hold_ms(),
                show_ln_tail_cap: false,
            },
            judge: JudgeConfig {
                input_offset_us: 0,
                visual_offset_us: 0,
                visual_offset_auto_adjust: false,
                judge_algorithm: JudgeAlgorithmConfig::Combo,
                fast_slow_display_threshold_ms: 0,
                fast_slow_display_scope: FastSlowDisplayScope::Auto,
            },
            lane: LaneViewConfig {
                hispeed: 2.0,
                hispeed_mode: default_hispeed_mode(),
                hispeed_step_nhs: default_hispeed_step_nhs(),
                hispeed_step_fhs: default_hispeed_step_fhs(),
                sudden: 0,
                lift: 0,
                lift_enabled: true,
                hispeed_auto_adjust: false,
                hidden: 0,
                target_green_number: 300,
            },
            input: crate::config::play_input::default_profile_input(),
            rival: RivalConfig { active_rival: String::new(), entries: Vec::new() },
            replay: ReplayConfig {
                auto_save: true,
                compress: false,
                slot_rules: default_slot_rules(),
            },
            ir: IrConfig::default(),
            ui: UiConfig {
                language: "ja".to_string(),
                theme: "default".to_string(),
                show_fps: false,
                confirm_on_exit: false,
            },
            audio_mix: AudioMixConfig {
                normalize_chart_volume: true,
                master_volume: 50,
                key_volume: 50,
                bgm_volume: 50,
                preview_volume: 50,
                system_bgm_volume: default_system_bgm_volume(),
                system_se_volume: default_system_se_volume(),
            },
            system_sound: SystemSoundConfig::default(),
            skin: SkinConfig::default(),
            select: SelectStateConfig::default(),
            statistics: StatisticsConfig::default(),
        }
    }
}

pub fn default_bindings() -> Vec<BindingConfigEntry> {
    let mut bindings = default_play_lane_bindings();
    bindings.extend(default_ui_bindings());
    bindings
}

pub fn default_ui_bindings() -> Vec<BindingConfigEntry> {
    default_keyboard_bindings()
        .into_iter()
        .filter(|entry| entry.action.is_some())
        .chain(default_gamepad_bindings().into_iter().filter(|entry| entry.action.is_some()))
        .collect()
}

fn default_play_lane_bindings() -> Vec<BindingConfigEntry> {
    default_keyboard_bindings()
        .into_iter()
        .filter(|entry| entry.lane.is_some())
        .chain(default_gamepad_bindings().into_iter().filter(|entry| entry.lane.is_some()))
        .collect()
}

pub fn default_keyboard_bindings() -> Vec<BindingConfigEntry> {
    vec![
        scratch_binding("LShift", LaneConfig::Scratch, ScratchDirectionConfig::Up),
        scratch_binding("LControl", LaneConfig::Scratch, ScratchDirectionConfig::Down),
        binding("Z", LaneConfig::Key1),
        binding("S", LaneConfig::Key2),
        binding("X", LaneConfig::Key3),
        binding("D", LaneConfig::Key4),
        binding("C", LaneConfig::Key5),
        binding("F", LaneConfig::Key6),
        binding("V", LaneConfig::Key7),
        action_binding("Q", InputActionConfig::E1),
        action_binding("Z", InputActionConfig::SelectEnter),
        action_binding("X", InputActionConfig::SelectEnter),
        action_binding("C", InputActionConfig::SelectEnter),
        action_binding("V", InputActionConfig::SelectEnter),
        action_binding("W", InputActionConfig::E2),
        action_binding("E", InputActionConfig::E3),
        action_binding("R", InputActionConfig::E4),
        action_binding("Z", InputActionConfig::SelectOptionArrange),
        action_binding("X", InputActionConfig::SelectOptionGauge),
        action_binding("C", InputActionConfig::SelectOptionAssist),
        action_binding("Z", InputActionConfig::SelectOptionBga),
        action_binding("F8", InputActionConfig::SelectFavoriteSong),
        action_binding("F9", InputActionConfig::SelectFavoriteChart),
        action_binding("Numpad8", InputActionConfig::SelectSameFolder),
    ]
}

pub fn default_gamepad_bindings() -> Vec<BindingConfigEntry> {
    vec![
        gamepad_scratch_binding("Axis1+", LaneConfig::Scratch, ScratchDirectionConfig::Up),
        gamepad_scratch_binding("Axis1-", LaneConfig::Scratch, ScratchDirectionConfig::Down),
        gamepad_binding("Button1", LaneConfig::Key1),
        gamepad_binding("Button2", LaneConfig::Key2),
        gamepad_binding("Button3", LaneConfig::Key3),
        gamepad_binding("Button4", LaneConfig::Key4),
        gamepad_binding("Button5", LaneConfig::Key5),
        gamepad_binding("Button6", LaneConfig::Key6),
        gamepad_binding("Button7", LaneConfig::Key7),
        gamepad_action_binding("Button9", InputActionConfig::E1),
        gamepad_action_binding("Button1", InputActionConfig::SelectEnter),
        gamepad_action_binding("Button10", InputActionConfig::E2),
        gamepad_action_binding("Button11", InputActionConfig::E3),
        gamepad_action_binding("Button12", InputActionConfig::E4),
        gamepad_action_binding("Button1", InputActionConfig::SelectOptionArrange),
        gamepad_action_binding("Button3", InputActionConfig::SelectOptionGauge),
        gamepad_action_binding("Button5", InputActionConfig::SelectOptionAssist),
        gamepad_action_binding("Button1", InputActionConfig::SelectOptionBga),
    ]
}

fn binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "keyboard".to_string(),
        control: control.to_string(),
        lane: Some(lane),
        action: None,
        scratch: None,
    }
}

fn scratch_binding(
    control: &str,
    lane: LaneConfig,
    scratch: ScratchDirectionConfig,
) -> BindingConfigEntry {
    let mut entry = binding(control, lane);
    entry.scratch = Some(scratch);
    entry
}

fn gamepad_binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "gamepad".to_string(),
        control: control.to_string(),
        lane: Some(lane),
        action: None,
        scratch: None,
    }
}

fn gamepad_scratch_binding(
    control: &str,
    lane: LaneConfig,
    scratch: ScratchDirectionConfig,
) -> BindingConfigEntry {
    let mut entry = gamepad_binding(control, lane);
    entry.scratch = Some(scratch);
    entry
}

fn action_binding(control: &str, action: InputActionConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "keyboard".to_string(),
        control: control.to_string(),
        lane: None,
        action: Some(action),
        scratch: None,
    }
}

fn gamepad_action_binding(control: &str, action: InputActionConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "gamepad".to_string(),
        control: control.to_string(),
        lane: None,
        action: Some(action),
        scratch: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_score_judge_algorithm_is_loaded_as_duration() {
        let judge: JudgeConfig = toml::from_str(
            r#"
            input_offset_us = 0
            visual_offset_us = 0
            judge_algorithm = "Score"
            fast_slow_display_threshold_ms = 0
            fast_slow_display_scope = "Auto"
            "#,
        )
        .unwrap();

        assert_eq!(judge.judge_algorithm, JudgeAlgorithmConfig::Duration);
    }

    #[test]
    fn play_defaults_uses_default_misslayer_duration_for_old_profiles() {
        let play: PlayDefaultsConfig = toml::from_str(
            r#"
            gauge = "Normal"
            random = "Off"
            lane_effect = "Off"
            assist = "None"
            auto_play = false
            "#,
        )
        .unwrap();

        assert_eq!(play.target, TargetOptionConfig::None);
        assert_eq!(play.grade_diff_display, ResultGradeDiffDisplay::Nearest);
        assert_eq!(play.rule_mode, RuleMode::Beatoraja);
        assert_eq!(play.ln_mode_policy, LnPolicySetting::AutoLn);
        assert_eq!(play.bga, BgaModeConfig::On);
        assert_eq!(play.bga_expand, BgaExpandConfig::KeepAspect);
        assert_eq!(play.misslayer_duration_ms, 500);
        assert_eq!(play.play_exit_hold_ms, 1000);
        assert_eq!(play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::AssistEasy);
    }

    #[test]
    fn lane_view_uses_mode_specific_hispeed_step_defaults_for_old_profiles() {
        let lane: LaneViewConfig = toml::from_str(
            r#"
            hispeed = 2.0
            hispeed_mode = "Normal"
            sudden = 0
            lift = 0
            hidden = 0
            target_green_number = 300
            "#,
        )
        .unwrap();

        assert_eq!(lane.hispeed_step_nhs, 0.25);
        assert_eq!(lane.hispeed_step_fhs, 0.50);
        assert!(lane.lift_enabled);
        assert!(!lane.hispeed_auto_adjust);

        let serialized = toml::to_string(&lane).unwrap();
        assert!(serialized.contains("hispeed_step_nhs = 0.25"));
        assert!(serialized.contains("hispeed_step_fhs = 0.5"));
        assert!(serialized.contains("lift_enabled = true"));
        assert!(serialized.contains("hispeed_auto_adjust = false"));
    }

    #[test]
    fn grade_diff_display_uses_next_nearest_keys() {
        fn parse_grade_diff_display(value: &str) -> ResultGradeDiffDisplay {
            let toml = format!(
                r#"
                gauge = "Normal"
                random = "Off"
                target = "None"
                grade_diff_display = "{value}"
                lane_effect = "Off"
                assist = "None"
                auto_play = false
                "#
            );
            toml::from_str::<PlayDefaultsConfig>(&toml).unwrap().grade_diff_display
        }

        assert_eq!(parse_grade_diff_display("Next"), ResultGradeDiffDisplay::Next);
        assert_eq!(parse_grade_diff_display("Nearest"), ResultGradeDiffDisplay::Nearest);

        let mut play = PlayDefaultsConfig {
            grade_diff_display: ResultGradeDiffDisplay::Next,
            ..ProfileConfig::new_default("default", "Default", 0).play
        };
        let serialized = toml::to_string(&play).unwrap();
        assert!(serialized.contains(r#"grade_diff_display = "Next""#));

        play.grade_diff_display = ResultGradeDiffDisplay::Nearest;
        let serialized = toml::to_string(&play).unwrap();
        assert!(serialized.contains(r#"grade_diff_display = "Nearest""#));
    }

    #[test]
    fn target_option_uses_beatoraja_ids_with_legacy_aliases() {
        fn parse_target(value: &str) -> TargetOptionConfig {
            let toml = format!(
                r#"
                gauge = "Normal"
                random = "Off"
                target = "{value}"
                grade_diff_display = "Next"
                lane_effect = "Off"
                assist = "None"
                auto_play = false
                "#
            );
            toml::from_str::<PlayDefaultsConfig>(&toml).unwrap().target
        }

        assert_eq!(parse_target("RANK_AAA"), TargetOptionConfig::RankAaa);
        assert_eq!(parse_target("AAA"), TargetOptionConfig::RankAaa);
        assert_eq!(parse_target("RIVAL_TOP"), TargetOptionConfig::RivalTop);
        assert_eq!(parse_target("Rival"), TargetOptionConfig::RivalTop);
        assert_eq!(parse_target("RIVAL_3"), TargetOptionConfig::RivalIndex(3));

        let mut play = PlayDefaultsConfig {
            target: TargetOptionConfig::RivalIndex(2),
            ..ProfileConfig::new_default("default", "Default", 0).play
        };
        let serialized = toml::to_string(&play).unwrap();
        assert!(serialized.contains(r#"target = "RIVAL_2""#));

        play.target = TargetOptionConfig::RankAaMinus;
        let serialized = toml::to_string(&play).unwrap();
        assert!(serialized.contains(r#"target = "RANK_AA-""#));
    }

    #[test]
    fn select_state_uses_defaults_for_old_profiles() {
        // `[select]` セクションが無い旧 profile.toml でも既定値になる。
        let select: SelectStateConfig = toml::from_str("").unwrap();

        assert_eq!(select.mode_filter, "ALL");
        assert_eq!(select.sort, "TITLE");
        assert!(!select.random_select);
    }

    #[test]
    fn select_state_roundtrips_through_toml() {
        let select = SelectStateConfig {
            mode_filter: "7K".to_string(),
            sort: "LEVEL".to_string(),
            random_select: true,
        };

        let toml = toml::to_string(&select).unwrap();
        let parsed: SelectStateConfig = toml::from_str(&toml).unwrap();

        assert_eq!(parsed.mode_filter, "7K");
        assert_eq!(parsed.sort, "LEVEL");
        assert!(parsed.random_select);
    }

    #[test]
    fn skin_config_separates_result_and_course_result_slots() {
        let skin: SkinConfig = toml::from_str(
            r#"
            result = "data/skins/result/result.luaskin"

            [result_options]
            Layout = "A"

            [result_files]
            Background = "normal.png"
            "#,
        )
        .unwrap();

        assert_eq!(skin.result, "data/skins/result/result.luaskin");
        assert!(skin.course_result.is_empty());
        assert_eq!(skin.result_options.get("Layout").map(String::as_str), Some("A"));
        assert!(skin.course_result_options.is_empty());

        let mut skin = skin;
        skin.course_result = "data/skins/result/course_result.luaskin".to_string();
        skin.course_result_options.insert("Layout".to_string(), "Course".to_string());
        skin.course_result_files.insert("Background".to_string(), "course.png".to_string());
        let toml = toml::to_string(&skin).unwrap();

        assert!(toml.contains("course_result = \"data/skins/result/course_result.luaskin\""));
        assert!(toml.contains("[course_result_options]"));
        assert!(toml.contains("[course_result_files]"));
    }

    #[test]
    fn skin_config_migrates_legacy_offsets_to_each_slot() {
        let mut skin: SkinConfig = toml::from_str(
            r#"
            [[offsets]]
            id = 30
            h = 12

            [[play7_offsets]]
            id = 30
            h = 7
            "#,
        )
        .unwrap();

        skin.migrate_legacy_offsets();

        assert_eq!(skin.select_offsets[0].h, 12);
        assert_eq!(skin.play4_offsets[0].h, 12);
        assert_eq!(skin.play7_offsets[0].h, 7);
        assert_eq!(skin.course_result_offsets[0].h, 12);

        let serialized = toml::to_string(&skin).unwrap();
        assert!(!serialized.contains("[[offsets]]"));
        assert!(serialized.contains("[[select_offsets]]"));
        assert!(serialized.contains("[[play7_offsets]]"));
    }

    #[test]
    fn default_profile_stores_select_start_in_bindings() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        assert_eq!(profile.play.ln_mode_policy, LnPolicySetting::AutoLn);
        assert!(profile.input.start_key.is_none());
        assert!(profile.input.ui.bindings.iter().any(|entry| {
            entry.device == "keyboard"
                && entry.control == "Q"
                && entry.action == Some(InputActionConfig::E1)
        }));
    }

    #[test]
    fn default_profile_uses_normalized_quieter_audio_and_prefetches_ir_rankings() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        assert_eq!(profile.audio_mix.master_volume, 50);
        assert!(profile.audio_mix.normalize_chart_volume);
        assert_eq!(profile.audio_mix.key_volume, 50);
        assert_eq!(profile.audio_mix.bgm_volume, 50);
        assert_eq!(profile.audio_mix.preview_volume, 50);
        assert_eq!(profile.audio_mix.system_bgm_volume, 50);
        assert_eq!(profile.audio_mix.system_se_volume, 50);
        assert!(profile.ir.prefetch_global_ranking_on_score_submit);
        assert!(profile.ir.prefetch_rival_ranking_on_score_submit);
    }

    #[test]
    fn ir_provider_defaults_to_always_send_policy() {
        let ir: IrConfig = toml::from_str(
            r#"
            primary_provider = "bmz-official"

            [[providers]]
            provider = "bmz-official"
            enabled = true
            "#,
        )
        .unwrap();

        assert_eq!(IrSendPolicyConfig::default(), IrSendPolicyConfig::Always);
        assert_eq!(ir.providers[0].send_policy, IrSendPolicyConfig::Always);
    }

    #[test]
    fn judge_config_serializes_visual_offset_auto_adjust_key() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.judge.visual_offset_auto_adjust = true;

        let toml = toml::to_string(&profile).unwrap();

        assert!(toml.contains("visual_offset_auto_adjust = true"));
        assert!(!toml.contains("input_offset_auto_adjust"));
    }

    #[test]
    fn replay_slot_rule_image_index_matches_beatoraja_autosave_rows() {
        use super::ReplaySlotRule;

        assert_eq!(ReplaySlotRule::Disabled.image_index(), 0);
        assert_eq!(ReplaySlotRule::ScoreUpdate.image_index(), 1);
        assert_eq!(ReplaySlotRule::BpUpdate.image_index(), 3);
        assert_eq!(ReplaySlotRule::MaxComboUpdate.image_index(), 5);
        assert_eq!(ReplaySlotRule::ClearUpdate.image_index(), 7);
        assert_eq!(ReplaySlotRule::Always.image_index(), 10);
        assert_eq!(replay_slot_rule_indices(&default_slot_rules()), [10, 1, 3, 0]);
    }

    #[test]
    fn replay_slot_rule_empty_string_disables_slot() {
        let profile: ProfileConfig = toml::from_str(
            r#"
            version = 1
            id = "default"
            display_name = "Default"
            player_name = "NONAME"
            created_at = 1
            updated_at = 1

            [play]
            gauge = "Normal"
            random = "Off"
            lane_effect = "Off"
            assist = "None"
            auto_play = false

            [judge]
            input_offset_us = 0
            visual_offset_us = 0
            judge_algorithm = "Combo"
            fast_slow_display_threshold_ms = 0
            fast_slow_display_scope = "Auto"

            [lane]
            hispeed = 2.0
            hispeed_mode = "Normal"
            sudden = 0
            lift = 0
            hidden = 0
            target_green_number = 300

            [input]
            scratch_mode = "Normal"
            analog_scratch_sensitivity = 1.0
            analog_scratch_timeout_ms = 500

            [rival]
            active_rival = ""
            entries = []

            [replay]
            auto_save = true
            compress = false
            slot_rules = ["Always", "ScoreUpdate", "BpUpdate", ""]

            [ir]
            primary_provider = ""
            providers = []

            [ui]
            language = "ja"
            theme = "default"
            show_fps = false
            confirm_on_exit = false

            [audio_mix]
            normalize_chart_volume = true
            master_volume = 50
            key_volume = 50
            bgm_volume = 50
            preview_volume = 50
            system_bgm_volume = 50
            system_se_volume = 50

            [system_sound]
            bgm_dir = "data/bgm"
            se_dir = "data/se"
            default_sound_dir = "data/defaultsound"
            "#,
        )
        .unwrap();

        assert_eq!(profile.replay.slot_rules[3], ReplaySlotRule::Disabled);
        assert!(profile.ir.prefetch_global_ranking_on_score_submit);
        assert!(profile.ir.prefetch_rival_ranking_on_score_submit);
    }

    #[test]
    fn default_gamepad_ui_bindings_use_thumb_buttons_without_dpad_enter_back() {
        let bindings = default_ui_bindings();

        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad"
                && entry.control == "Button9"
                && entry.action == Some(InputActionConfig::E1)
        }));
        assert!(bindings.iter().any(|entry| {
            entry.device == "gamepad"
                && entry.control == "Button12"
                && entry.action == Some(InputActionConfig::E4)
        }));
        assert!(!bindings.iter().any(|entry| {
            entry.device == "gamepad"
                && matches!(entry.control.as_str(), "DPadLeft" | "DPadRight")
                && matches!(
                    entry.action,
                    Some(InputActionConfig::E2 | InputActionConfig::SelectEnter)
                )
        }));
    }

    #[test]
    fn input_config_reads_legacy_start_key() {
        let input: ProfileInputConfig = toml::from_str(
            r#"
            scratch_mode = "Normal"
            start_key = "E"
            analog_scratch_sensitivity = 1.0
            analog_scratch_timeout_ms = 500

            [[bindings]]
            device = "keyboard"
            control = "Z"
            lane = "Key1"
            "#,
        )
        .unwrap();

        assert_eq!(input.start_key.as_deref(), Some("E"));
        assert_eq!(input.legacy_bindings[0].lane, Some(LaneConfig::Key1));
        assert_eq!(input.analog_scratch_timeout_ms, 500);
        assert_eq!(input.analog_scratch_threshold, default_analog_scratch_threshold());
    }

    #[test]
    fn input_config_serializes_select_actions_without_start_key() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let toml = toml::to_string(&profile.input).unwrap();

        assert!(!toml.contains("start_key"));
        assert!(!toml.contains("analog_scratch_timeout_ms"));
        assert!(toml.contains("analog_scratch_threshold = 100"));
        assert!(toml.contains("action = \"E1\""));
        assert!(toml.contains("action = \"E2\""));
        assert!(toml.contains("action = \"E3\""));
        assert!(toml.contains("action = \"E4\""));
        assert!(toml.contains("control = \"Axis1+\"\nlane = \"Scratch\"\nscratch = \"up\""));
        assert!(toml.contains("control = \"Axis1-\"\nlane = \"Scratch\"\nscratch = \"down\""));
    }
}
