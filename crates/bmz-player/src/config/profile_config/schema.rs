use super::skin_ir::default_true;
use super::*;

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
    #[serde(default)]
    pub play_overlay: PlayOverlayConfig,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StatisticsConfig {
    /// Local hour at which BMZ starts a new statistics day (0..=23).
    #[serde(default)]
    pub day_start_hour: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayOverlayConfig {
    #[serde(default)]
    pub websocket_enabled: bool,
    #[serde(default = "default_play_overlay_websocket_port")]
    pub websocket_port: u16,
    #[serde(default)]
    pub websocket_update_rate: PlayOverlayUpdateRateConfig,
    #[serde(default = "default_play_overlay_release_ignore_threshold_ms")]
    pub release_ignore_threshold_ms: u32,
    #[serde(default = "default_play_overlay_release_window_ms")]
    pub release_window_ms: u32,
    #[serde(default = "default_play_overlay_release_ok_threshold_ms")]
    pub release_ok_threshold_ms: u32,
    #[serde(default = "default_play_overlay_release_ng_threshold_ms")]
    pub release_ng_threshold_ms: u32,
    #[serde(default)]
    pub release_display_mode: PlayOverlayReleaseDisplayModeConfig,
    #[serde(default)]
    pub controller_mode: PlayOverlayControllerModeConfig,
}

impl Default for PlayOverlayConfig {
    fn default() -> Self {
        Self {
            websocket_enabled: false,
            websocket_port: default_play_overlay_websocket_port(),
            websocket_update_rate: PlayOverlayUpdateRateConfig::default(),
            release_ignore_threshold_ms: default_play_overlay_release_ignore_threshold_ms(),
            release_window_ms: default_play_overlay_release_window_ms(),
            release_ok_threshold_ms: default_play_overlay_release_ok_threshold_ms(),
            release_ng_threshold_ms: default_play_overlay_release_ng_threshold_ms(),
            release_display_mode: PlayOverlayReleaseDisplayModeConfig::default(),
            controller_mode: PlayOverlayControllerModeConfig::default(),
        }
    }
}

pub fn default_play_overlay_websocket_port() -> u16 {
    29470
}

pub fn default_play_overlay_release_ignore_threshold_ms() -> u32 {
    250
}

pub fn default_play_overlay_release_window_ms() -> u32 {
    2_000
}

pub fn default_play_overlay_release_ok_threshold_ms() -> u32 {
    60
}

pub fn default_play_overlay_release_ng_threshold_ms() -> u32 {
    80
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlayOverlayUpdateRateConfig {
    #[default]
    Fps60,
    Fps120,
    Fps240,
}

impl PlayOverlayUpdateRateConfig {
    pub fn fps(self) -> u32 {
        match self {
            Self::Fps60 => 60,
            Self::Fps120 => 120,
            Self::Fps240 => 240,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlayOverlayReleaseDisplayModeConfig {
    #[default]
    ReleaseOnly,
    ReleaseAndNotes,
    NotesOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum PlayOverlayControllerModeConfig {
    #[default]
    Key7P1,
    Key7P2,
    Key14,
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
    #[serde(default = "default_true")]
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

pub(super) fn default_hispeed_mode() -> HispeedModeConfig {
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
