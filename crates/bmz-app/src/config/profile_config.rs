use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
    pub ui: UiConfig,
    pub audio_mix: AudioMixConfig,
    #[serde(default)]
    pub system_sound: SystemSoundConfig,
    #[serde(default)]
    pub skin: SkinConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayDefaultsConfig {
    pub gauge: GaugeTypeConfig,
    pub random: RandomOptionConfig,
    pub lane_effect: LaneEffectConfig,
    pub assist: AssistOptionConfig,
    pub auto_play: bool,
    #[serde(default = "default_bga_mode")]
    pub bga: BgaModeConfig,
    #[serde(default = "default_bga_expand")]
    pub bga_expand: BgaExpandConfig,
    #[serde(default = "default_misslayer_duration_ms")]
    pub misslayer_duration_ms: u32,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GaugeTypeConfig {
    AssistEasy,
    Easy,
    Normal,
    Hard,
    ExHard,
    Hazard,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum RandomOptionConfig {
    Off,
    Mirror,
    Random,
    RRandom,
    SRandom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum LaneEffectConfig {
    Off,
    Hidden,
    Sudden,
    HiddenSudden,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum JudgeAlgorithmConfig {
    Combo,
    Duration,
    Lowest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaneViewConfig {
    pub hispeed: f32,
    pub lane_cover: f32,
    pub lift: f32,
    pub hidden: f32,
    pub note_scale: f32,
    pub target_green_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInputConfig {
    pub scratch_mode: ScratchInputMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_key: Option<String>,
    pub bindings: Vec<BindingConfigEntry>,
    #[serde(default = "default_analog_scratch_sensitivity")]
    pub analog_scratch_sensitivity: f32,
    #[serde(default = "default_analog_scratch_timeout_ms")]
    pub analog_scratch_timeout_ms: u32,
}

fn default_analog_scratch_sensitivity() -> f32 {
    1.0
}

fn default_analog_scratch_timeout_ms() -> u32 {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingConfigEntry {
    pub device: String,
    pub control: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<LaneConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<InputActionConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum InputActionConfig {
    SelectStart,
    SelectEnter,
    SelectBack,
    SelectOptionArrange,
    SelectOptionGauge,
    SelectOptionAssist,
    SelectOptionBga,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ScratchInputMode {
    Normal,
    AnyDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    MissCountUpdate,
    MaxComboUpdate,
    ClearUpdate,
}

impl ReplaySlotRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::ScoreUpdate => "ScoreUpdate",
            Self::MissCountUpdate => "MissCountUpdate",
            Self::MaxComboUpdate => "MaxComboUpdate",
            Self::ClearUpdate => "ClearUpdate",
        }
    }

    pub fn from_str_opt(value: &str) -> Option<Self> {
        match value {
            "Always" => Some(Self::Always),
            "ScoreUpdate" => Some(Self::ScoreUpdate),
            "MissCountUpdate" => Some(Self::MissCountUpdate),
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
        ReplaySlotRule::MissCountUpdate,
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
    pub master_volume: f32,
    pub key_volume: f32,
    pub bgm_volume: f32,
    pub preview_volume: f32,
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
    /// システム SE / BGM のマスターボリューム(0.0..=1.0)。
    #[serde(default = "default_system_sound_volume")]
    pub volume: f32,
}

pub fn default_system_sound_default_dir() -> String {
    "defaultsound".to_string()
}

pub fn default_system_sound_volume() -> f32 {
    1.0
}

impl Default for SystemSoundConfig {
    fn default() -> Self {
        Self {
            bgm_dir: String::new(),
            se_dir: String::new(),
            default_sound_dir: default_system_sound_default_dir(),
            volume: default_system_sound_volume(),
        }
    }
}

/// スキン設定。スキンはプロファイルごとに切り替えられる。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkinConfig {
    /// 選曲画面スキンのパス。
    /// 空文字列なら bmz の固定描画を使用。
    /// `.json` で終わるパスは beatoraja JSON スキンとして扱う。
    #[serde(default)]
    pub select: String,
    /// 決定画面スキンのパス。
    /// 空文字列ならプレイ開始前もプレイスキン側の描画を使用。
    /// `.json` / `.luaskin` / `.lua` で終わるパスは beatoraja スキンとして扱う。
    #[serde(default)]
    pub decide: String,
    /// 5K プレイ画面スキンのパス。
    /// 空文字列なら内蔵デフォルトスキンを使用。
    /// `.json` で終わるパスは beatoraja JSON スキン、それ以外は
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
    /// リザルト画面スキンのパス。
    /// 空文字列なら bmz の固定描画を使用。
    /// `.json` で終わるパスは beatoraja JSON スキンとして扱う。
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
                gauge: GaugeTypeConfig::Normal,
                random: RandomOptionConfig::Off,
                lane_effect: LaneEffectConfig::Off,
                assist: AssistOptionConfig::None,
                auto_play: false,
                bga: default_bga_mode(),
                bga_expand: default_bga_expand(),
                misslayer_duration_ms: default_misslayer_duration_ms(),
            },
            judge: JudgeConfig {
                input_offset_us: 0,
                visual_offset_us: 0,
                judge_algorithm: JudgeAlgorithmConfig::Combo,
            },
            lane: LaneViewConfig {
                hispeed: 2.0,
                lane_cover: 0.0,
                lift: 0.0,
                hidden: 0.0,
                note_scale: 1.0,
                target_green_number: 300,
            },
            input: ProfileInputConfig {
                scratch_mode: ScratchInputMode::Normal,
                start_key: None,
                bindings: default_bindings(),
                analog_scratch_sensitivity: default_analog_scratch_sensitivity(),
                analog_scratch_timeout_ms: default_analog_scratch_timeout_ms(),
            },
            rival: RivalConfig { active_rival: String::new(), entries: Vec::new() },
            replay: ReplayConfig {
                auto_save: true,
                compress: false,
                slot_rules: default_slot_rules(),
            },
            ui: UiConfig {
                language: "ja".to_string(),
                theme: "default".to_string(),
                show_fps: false,
                confirm_on_exit: true,
            },
            audio_mix: AudioMixConfig {
                master_volume: 1.0,
                key_volume: 1.0,
                bgm_volume: 1.0,
                preview_volume: 0.7,
            },
            system_sound: SystemSoundConfig::default(),
            skin: SkinConfig::default(),
        }
    }
}

pub fn default_bindings() -> Vec<BindingConfigEntry> {
    let mut bindings = default_keyboard_bindings();
    bindings.extend(default_gamepad_bindings());
    bindings
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
        action_binding("Q", InputActionConfig::SelectStart),
        action_binding("Z", InputActionConfig::SelectEnter),
        action_binding("X", InputActionConfig::SelectEnter),
        action_binding("C", InputActionConfig::SelectEnter),
        action_binding("V", InputActionConfig::SelectEnter),
        action_binding("S", InputActionConfig::SelectBack),
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
        gamepad_action_binding("Start", InputActionConfig::SelectStart),
        gamepad_action_binding("Button1", InputActionConfig::SelectEnter),
        gamepad_action_binding("DPadRight", InputActionConfig::SelectEnter),
        gamepad_action_binding("Select", InputActionConfig::SelectBack),
        gamepad_action_binding("DPadLeft", InputActionConfig::SelectBack),
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
    }
}

fn gamepad_binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "gamepad".to_string(),
        control: control.to_string(),
        lane: Some(lane),
        action: None,
    }
}

fn action_binding(control: &str, action: InputActionConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "keyboard".to_string(),
        control: control.to_string(),
        lane: None,
        action: Some(action),
    }
}

fn gamepad_action_binding(control: &str, action: InputActionConfig) -> BindingConfigEntry {
    BindingConfigEntry {
        device: "gamepad".to_string(),
        control: control.to_string(),
        lane: None,
        action: Some(action),
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

        assert_eq!(play.bga, BgaModeConfig::On);
        assert_eq!(play.bga_expand, BgaExpandConfig::KeepAspect);
        assert_eq!(play.misslayer_duration_ms, 500);
    }

    #[test]
    fn default_profile_stores_select_start_in_bindings() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        assert!(profile.input.start_key.is_none());
        assert!(profile.input.bindings.iter().any(|entry| {
            entry.device == "keyboard"
                && entry.control == "Q"
                && entry.action == Some(InputActionConfig::SelectStart)
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
        assert_eq!(input.bindings[0].lane, Some(LaneConfig::Key1));
    }

    #[test]
    fn input_config_serializes_select_actions_without_start_key() {
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let toml = toml::to_string(&profile.input).unwrap();

        assert!(!toml.contains("start_key"));
        assert!(toml.contains("action = \"SelectStart\""));
        assert!(toml.contains("action = \"SelectBack\""));
    }
}
