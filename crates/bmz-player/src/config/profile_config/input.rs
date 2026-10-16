use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiInputConfig {
    /// UI action binding schema version. Missing values are migrated on load.
    #[serde(default)]
    pub version: u32,
    #[serde(default = "default_ui_bindings")]
    pub bindings: Vec<BindingConfigEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayModeInputConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherit: Option<String>,
    #[serde(default)]
    pub bindings: Vec<BindingConfigEntry>,
    /// 8K の論理キーごとのハイスピード操作方向 override。
    ///
    /// 未指定のキーはプレイ側のモード既定方向を使う。8K 以外では保存されない
    /// 想定だが、profile の前方互換性のため入力設定型では値を保持する。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hispeed: BTreeMap<LaneConfig, HispeedDirectionConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInputConfig {
    /// 旧 profile の読込互換用。入力動作には使用せず、保存時は出力しない。
    #[serde(default, skip_serializing)]
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
    /// 旧profileの共通アナログ感度。読込時に1P/2Pへ移行し、保存時は出力しない。
    #[serde(default, rename = "analog_scratch_sensitivity", skip_serializing)]
    pub legacy_analog_scratch_sensitivity: Option<f32>,
    /// 旧アナログ皿の壁時計タイムアウト。読込互換だけに残し、保存時は出力しない。
    #[serde(default = "default_analog_scratch_timeout_ms", skip_serializing)]
    pub analog_scratch_timeout_ms: u32,
    /// 旧profileの共通停止閾値。読込時に1P/2Pへ移行し、保存時は出力しない。
    #[serde(default, rename = "analog_scratch_threshold", skip_serializing)]
    pub legacy_analog_scratch_threshold: Option<u32>,
    /// 論理1Pコントローラー (`gamepad1`) のスクラッチ設定。
    #[serde(default)]
    pub gamepad1: GamepadScratchConfig,
    /// 論理2Pコントローラー (`gamepad2`) のスクラッチ設定。
    #[serde(default)]
    pub gamepad2: GamepadScratchConfig,
    /// 選曲画面でアナログスクラッチ何 tick ごとにカーソルを 1 つ動かすか (beatoraja の analogTicksPerScroll)。
    #[serde(default = "default_analog_ticks_per_scroll")]
    pub analog_ticks_per_scroll: u32,
    /// プレイ中のレーンカバー操作キー (↑↓) を1回押したときの変化量。1/1000 単位。
    #[serde(default = "default_lane_cover_step")]
    pub lane_cover_step: u32,
    /// レーンカバー操作キーを押し続けてから連続変化を始めるまでの時間。
    #[serde(default = "default_lane_cover_hold_delay_ms")]
    pub lane_cover_hold_delay_ms: u32,
    /// 連続変化中に1/60秒ごとに加える変化量。1/1000 単位。
    #[serde(default = "default_lane_cover_hold_step")]
    pub lane_cover_hold_step: u32,
    /// プレイ中のマウスホイール1ノッチでレーンカバーを動かす量。1/1000 単位。
    #[serde(default = "default_lane_cover_wheel_step")]
    pub lane_cover_wheel_step: u32,
    /// Release 直後に同じキーボードキーから届く Press を無視する時間。
    ///
    /// 0 はフィルタ無効。物理スイッチの Release 側チャタリングを対象とし、
    /// Press 自体や Release の判定時刻は遅延させない。
    #[serde(default = "default_keyboard_release_bounce_ms")]
    pub keyboard_release_bounce_ms: u32,
    /// Release 直後に同じコントローラーボタンから届く Press を無視する時間。
    ///
    /// 0 はフィルタ無効。
    #[serde(default = "default_controller_release_bounce_ms")]
    pub controller_release_bounce_ms: u32,
}

pub const RELEASE_BOUNCE_MS_MAX: u32 = 20;

fn default_analog_scratch_sensitivity() -> f32 {
    1.0
}

fn default_analog_scratch_timeout_ms() -> u32 {
    500
}

pub fn default_analog_scratch_threshold() -> u32 {
    100
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GamepadScratchConfig {
    /// trueは回転差分方式、falseはbeatorajaのANALOG SCRATCH OFF相当の端点方式。
    #[serde(default = "default_true")]
    pub analog_scratch: bool,
    #[serde(default = "default_analog_scratch_sensitivity")]
    pub analog_scratch_sensitivity: f32,
    /// beatoraja の analogScratchThreshold 相当。既定は Version2 向けの100。
    #[serde(default = "default_analog_scratch_threshold")]
    pub analog_scratch_threshold: u32,
}

impl Default for GamepadScratchConfig {
    fn default() -> Self {
        Self {
            analog_scratch: true,
            analog_scratch_sensitivity: default_analog_scratch_sensitivity(),
            analog_scratch_threshold: default_analog_scratch_threshold(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_analog_ticks_per_scroll() -> u32 {
    3
}

pub const LANE_COVER_STEP_MIN: u32 = 1;
pub const LANE_COVER_STEP_MAX: u32 = 1000;
pub const LANE_COVER_HOLD_DELAY_MS_MAX: u32 = 1000;
pub const LANE_COVER_HOLD_STEP_MIN: u32 = 1;
pub const LANE_COVER_HOLD_STEP_MAX: u32 = 100;
pub const LANE_COVER_WHEEL_STEP_MIN: u32 = 1;
pub const LANE_COVER_WHEEL_STEP_MAX: u32 = 1000;

pub const fn default_lane_cover_step() -> u32 {
    10
}

pub const fn default_lane_cover_hold_delay_ms() -> u32 {
    300
}

pub const fn default_lane_cover_hold_step() -> u32 {
    3
}

/// beatoraja `ControlInputProcessor` のホイール1ノッチ (0.005) 相当。
pub const fn default_lane_cover_wheel_step() -> u32 {
    5
}

fn default_keyboard_release_bounce_ms() -> u32 {
    0
}

fn default_controller_release_bounce_ms() -> u32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingConfigEntry {
    pub device: String,
    pub control: String,
    /// キーボードの主 / 副スロット。旧 profile の未指定 entry は表示側で
    /// 従来の配列順へフォールバックする。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_slot: Option<KeyboardBindingSlotConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<LaneConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<InputActionConfig>,
    /// スクラッチレーンの回転方向。コントロール名からの推測 (`+`/`-` 等) に
    /// 依存せず方向を確定させるため、キーコンフィグで設定した entry に保存する。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch: Option<ScratchDirectionConfig>,
}

/// キーボードバインドの表示・編集スロット。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardBindingSlotConfig {
    Primary,
    Secondary,
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
    PlayHispeedDown,
    PlayHispeedUp,
    PlayLaneCoverUp,
    PlayLaneCoverDown,
    PlayVisualOffsetUp,
    PlayVisualOffsetDown,
    PlayVisualOffsetAutoAdjust,
    E1,
    /// Deprecated compatibility value. Runtime selection uses play-lane bindings.
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
    /// Deprecated compatibility value. Runtime BGA selection uses KEY1.
    #[serde(rename = "OptionBga")]
    SelectOptionBga,
    #[serde(rename = "OpenFolder")]
    SelectOpenFolder,
    #[serde(rename = "Reload")]
    SelectReload,
    #[serde(rename = "AutoplayFolder")]
    SelectAutoplayFolder,
    #[serde(rename = "OpenIr")]
    SelectOpenIr,
    #[serde(rename = "OpenKeyConfig")]
    SelectOpenKeyConfig,
    Screenshot,
    #[serde(rename = "RivalCycle")]
    SelectRivalCycle,
    #[serde(rename = "OpenDocuments")]
    SelectOpenDocuments,
    #[serde(rename = "FavoriteSong")]
    SelectFavoriteSong,
    #[serde(rename = "FavoriteChart")]
    SelectFavoriteChart,
    #[serde(rename = "SameFolder")]
    SelectSameFolder,
    #[serde(rename = "ModeFilter")]
    SelectModeFilter,
    #[serde(rename = "Sort")]
    SelectSort,
    #[serde(rename = "LnMode")]
    SelectLnMode,
    #[serde(rename = "DifficultyFilter")]
    SelectDifficultyFilter,
    #[serde(rename = "ReplayCycle")]
    SelectReplayCycle,
    #[serde(rename = "ReplayPlay")]
    SelectReplayPlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum ScratchInputMode {
    #[default]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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
