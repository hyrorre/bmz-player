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
    pub skin: SkinConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayDefaultsConfig {
    pub gauge: GaugeTypeConfig,
    pub random: RandomOptionConfig,
    pub lane_effect: LaneEffectConfig,
    pub assist: AssistOptionConfig,
    pub auto_play: bool,
    #[serde(default = "default_misslayer_duration_ms")]
    pub misslayer_duration_ms: u32,
}

pub fn default_misslayer_duration_ms() -> u32 {
    500
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
    #[serde(default = "default_start_key")]
    pub start_key: String,
    pub bindings: Vec<BindingConfigEntry>,
}

fn default_start_key() -> String {
    "Q".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingConfigEntry {
    pub device: String,
    pub control: String,
    pub lane: LaneConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayConfig {
    pub auto_save: bool,
    pub save_failed_runs: bool,
    pub save_autoplay_runs: bool,
    pub compress: bool,
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

/// スキン設定。スキンはプロファイルごとに切り替えられる。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkinConfig {
    /// プレイ画面スキンのパス。
    /// 空文字列なら内蔵デフォルトスキンを使用。
    /// `.json` で終わるパスは beatoraja JSON スキン、それ以外は
    /// `skin.toml` を含む bmz スキンディレクトリとして扱う。
    #[serde(default)]
    pub play: String,
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
                start_key: default_start_key(),
                bindings: default_keyboard_bindings(),
            },
            rival: RivalConfig { active_rival: String::new(), entries: Vec::new() },
            replay: ReplayConfig {
                auto_save: true,
                save_failed_runs: false,
                save_autoplay_runs: false,
                compress: false,
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
            skin: SkinConfig::default(),
        }
    }
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
    ]
}

fn binding(control: &str, lane: LaneConfig) -> BindingConfigEntry {
    BindingConfigEntry { device: "keyboard".to_string(), control: control.to_string(), lane }
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

        assert_eq!(play.misslayer_duration_ms, 500);
    }
}
