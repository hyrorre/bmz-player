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
    pub player_name: String,
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
}

pub fn default_select_mode_filter() -> String {
    "ALL".to_string()
}

pub fn default_select_sort() -> String {
    "TITLE".to_string()
}

impl Default for SelectStateConfig {
    fn default() -> Self {
        Self { mode_filter: default_select_mode_filter(), sort: default_select_sort() }
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
    pub visual_offset_us: i64,
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
    Duration,
    Lowest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaneViewConfig {
    pub hispeed: f32,
    #[serde(default = "default_hispeed_mode")]
    pub hispeed_mode: HispeedModeConfig,
    /// SUDDEN+ レーンカバー量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub sudden: u32,
    /// LIFT 量。0..=1000 の整数で持ち、ランタイムでは /1000 して扱う。
    pub lift: u32,
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
    #[serde(default = "default_analog_scratch_timeout_ms")]
    pub analog_scratch_timeout_ms: u32,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ScratchInputMode {
    Normal,
    AnyDirection,
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
    Always,
    ScoreUpdate,
    BpUpdate,
    MaxComboUpdate,
    ClearUpdate,
}

impl ReplaySlotRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::ScoreUpdate => "ScoreUpdate",
            Self::BpUpdate => "BpUpdate",
            Self::MaxComboUpdate => "MaxComboUpdate",
            Self::ClearUpdate => "ClearUpdate",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
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
        ReplaySlotRule::MaxComboUpdate,
    ]
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
    100
}

pub fn default_system_se_volume() -> u32 {
    100
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
    "defaultsound".to_string()
}

impl Default for SystemSoundConfig {
    fn default() -> Self {
        Self {
            bgm_dir: String::new(),
            se_dir: String::new(),
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
    /// `.json` / `.lr2skin` で終わるパスは beatoraja スキン、それ以外は
    /// `skin.toml` を含む bmz スキンディレクトリとして扱う。
    #[serde(default)]
    pub play5: String,
    /// 7K プレイ画面スキンのパス。フォーマットは [`play5`] と同じ。
    #[serde(default)]
    pub play7: String,
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
    #[serde(default)]
    pub offsets: Vec<SkinOffsetConfig>,
    /// 選曲スキンのカスタマイズオプション選択 (オプション名 -> 選択肢名)。
    #[serde(default)]
    pub select_options: BTreeMap<String, String>,
    /// 決定スキンのカスタマイズオプション選択。
    #[serde(default)]
    pub decide_options: BTreeMap<String, String>,
    /// 5K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play5_options: BTreeMap<String, String>,
    /// 7K プレイスキンのカスタマイズオプション選択。
    #[serde(default)]
    pub play7_options: BTreeMap<String, String>,
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
    /// 選曲スキンのファイル選択 (filepath 定義名 -> 選択ファイルの相対パス)。
    #[serde(default)]
    pub select_files: BTreeMap<String, String>,
    /// 決定スキンのファイル選択。
    #[serde(default)]
    pub decide_files: BTreeMap<String, String>,
    /// 5K プレイスキンのファイル選択。
    #[serde(default)]
    pub play5_files: BTreeMap<String, String>,
    /// 7K プレイスキンのファイル選択。
    #[serde(default)]
    pub play7_files: BTreeMap<String, String>,
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
    /// スキンファイル path ごとのカスタマイズ履歴。
    ///
    /// beatoraja の `skinHistory` 相当。スキンを切り替えても、各スキンの
    /// option / filepath / offset を前回値へ戻せるように保持する。
    #[serde(default)]
    pub history: BTreeMap<String, SkinHistoryEntryConfig>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IrConfig {
    #[serde(default)]
    pub primary_provider: String,
    /// 秘密情報 (refresh token / device key) の保存先。
    /// 開発時の Keychain 許可ダイアログを避けるため既定はファイル保存。
    #[serde(default)]
    pub credential_store: IrCredentialStoreConfig,
    #[serde(default)]
    pub providers: Vec<IrProviderConfig>,
    #[serde(default)]
    pub prefetch_global_ranking_on_score_submit: bool,
    #[serde(default)]
    pub prefetch_rival_ranking_on_score_submit: bool,
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
    #[default]
    UpdateScore,
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
            player_name: "NONAME".to_string(),
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
                judge_algorithm: JudgeAlgorithmConfig::Combo,
                fast_slow_display_threshold_ms: 0,
                fast_slow_display_scope: FastSlowDisplayScope::Auto,
            },
            lane: LaneViewConfig {
                hispeed: 2.0,
                hispeed_mode: default_hispeed_mode(),
                sudden: 0,
                lift: 0,
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
                confirm_on_exit: true,
            },
            audio_mix: AudioMixConfig {
                master_volume: 20,
                key_volume: 100,
                bgm_volume: 100,
                preview_volume: 100,
                system_bgm_volume: default_system_bgm_volume(),
                system_se_volume: default_system_se_volume(),
            },
            system_sound: SystemSoundConfig::default(),
            skin: SkinConfig::default(),
            select: SelectStateConfig::default(),
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
        binding("LShift", LaneConfig::Scratch),
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
    ]
}

pub fn default_gamepad_bindings() -> Vec<BindingConfigEntry> {
    vec![
        gamepad_binding("AxisLeftX+", LaneConfig::Scratch),
        gamepad_binding("AxisLeftX-", LaneConfig::Scratch),
        gamepad_binding("Button1", LaneConfig::Key1),
        gamepad_binding("Button2", LaneConfig::Key2),
        gamepad_binding("Button3", LaneConfig::Key3),
        gamepad_binding("Button4", LaneConfig::Key4),
        gamepad_binding("Button5", LaneConfig::Key5),
        gamepad_binding("Button6", LaneConfig::Key6),
        gamepad_binding("Button7", LaneConfig::Key7),
        gamepad_action_binding("Start", InputActionConfig::E1),
        gamepad_action_binding("Button1", InputActionConfig::SelectEnter),
        gamepad_action_binding("DPadRight", InputActionConfig::SelectEnter),
        gamepad_action_binding("Select", InputActionConfig::E2),
        gamepad_action_binding("DPadLeft", InputActionConfig::E2),
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

fn gamepad_binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "gamepad".to_string(),
        control: control.to_string(),
        lane: Some(lane),
        action: None,
        scratch: None,
    }
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
        assert_eq!(play.grade_diff_display, ResultGradeDiffDisplay::Beatoraja);
        assert_eq!(play.rule_mode, RuleMode::Beatoraja);
        assert_eq!(play.ln_mode_policy, LnPolicySetting::AutoLn);
        assert_eq!(play.bga, BgaModeConfig::On);
        assert_eq!(play.bga_expand, BgaExpandConfig::KeepAspect);
        assert_eq!(play.misslayer_duration_ms, 500);
        assert_eq!(play.play_exit_hold_ms, 1000);
        assert_eq!(play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::AssistEasy);
    }

    #[test]
    fn grade_diff_display_uses_next_nearest_keys_with_legacy_aliases() {
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

        assert_eq!(parse_grade_diff_display("Next"), ResultGradeDiffDisplay::Beatoraja);
        assert_eq!(parse_grade_diff_display("Nearest"), ResultGradeDiffDisplay::HalfGrade);
        assert_eq!(parse_grade_diff_display("Beatoraja"), ResultGradeDiffDisplay::Beatoraja);
        assert_eq!(parse_grade_diff_display("HalfGrade"), ResultGradeDiffDisplay::HalfGrade);

        let mut play = PlayDefaultsConfig {
            grade_diff_display: ResultGradeDiffDisplay::Beatoraja,
            ..ProfileConfig::new_default("default", "Default", 0).play
        };
        let serialized = toml::to_string(&play).unwrap();
        assert!(serialized.contains(r#"grade_diff_display = "Next""#));

        play.grade_diff_display = ResultGradeDiffDisplay::HalfGrade;
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
    }

    #[test]
    fn select_state_roundtrips_through_toml() {
        let select = SelectStateConfig { mode_filter: "7K".to_string(), sort: "LEVEL".to_string() };

        let toml = toml::to_string(&select).unwrap();
        let parsed: SelectStateConfig = toml::from_str(&toml).unwrap();

        assert_eq!(parsed.mode_filter, "7K");
        assert_eq!(parsed.sort, "LEVEL");
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
    }

    #[test]
    fn input_config_serializes_select_actions_without_start_key() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let toml = toml::to_string(&profile.input).unwrap();

        assert!(!toml.contains("start_key"));
        assert!(toml.contains("action = \"E1\""));
        assert!(toml.contains("action = \"E2\""));
        assert!(toml.contains("action = \"E3\""));
        assert!(toml.contains("action = \"E4\""));
    }
}
