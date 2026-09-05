use super::*;

/// 鍵盤入力でハイスピードを変更する方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HispeedDirectionConfig {
    Down,
    Up,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RivalConfig {
    pub active_rival: String,
    #[serde(default)]
    pub chart_replication_mode: ChartReplicationModeConfig,
    pub entries: Vec<RivalEntry>,
}

/// beatoraja `MusicSelector.ChartReplicationMode` のユーザー選択可能な3値。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartReplicationModeConfig {
    #[serde(rename = "NONE")]
    None,
    #[default]
    #[serde(rename = "RIVALCHART")]
    RivalChart,
    #[serde(rename = "RIVALOPTION")]
    RivalOption,
}

impl ChartReplicationModeConfig {
    pub const CYCLE_ORDER: [Self; 3] = [Self::None, Self::RivalChart, Self::RivalOption];

    pub fn cycle(self, forward: bool) -> Self {
        let index = Self::CYCLE_ORDER.iter().position(|mode| *mode == self).unwrap_or(0);
        let offset = if forward { 1 } else { Self::CYCLE_ORDER.len() - 1 };
        Self::CYCLE_ORDER[(index + offset) % Self::CYCLE_ORDER.len()]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::RivalChart => "RIVALCHART",
            Self::RivalOption => "RIVALOPTION",
        }
    }
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
    #[serde(
        default = "default_profile_language",
        deserialize_with = "deserialize_profile_language",
        serialize_with = "serialize_profile_language"
    )]
    pub language: String,
    pub theme: String,
    pub show_fps: bool,
    pub confirm_on_exit: bool,
}

impl UiConfig {
    /// 保存互換の String から、UI で利用する型付き locale を返す。
    pub fn locale(&self) -> AppLocale {
        AppLocale::profile_language(&self.language)
    }

    /// 言語 ComboBox などで選ばれた locale を canonical code で保持する。
    pub fn set_locale(&mut self, locale: AppLocale) {
        self.language = locale.code().to_owned();
    }

    /// alias や不明値を profile 保存用の canonical code へ正規化する。
    pub fn normalize_language(&mut self) -> AppLocale {
        let locale = self.locale();
        self.set_locale(locale);
        locale
    }
}

fn default_profile_language() -> String {
    AppLocale::DEFAULT.code().to_owned()
}

fn deserialize_profile_language<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let language = String::deserialize(deserializer)?;
    Ok(AppLocale::profile_language(&language).code().to_owned())
}

fn serialize_profile_language<S>(language: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(AppLocale::profile_language(language).code())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMixConfig {
    #[serde(default)]
    pub normalize_chart_volume: bool,
    /// システム BGM (Select / Decide) の音量正規化を有効にする。
    #[serde(default = "default_normalize_system_bgm_volume")]
    pub normalize_system_bgm_volume: bool,
    /// マスターボリューム。0..=100 の整数で持ち、ランタイムでは /100 して扱う。
    pub master_volume: u32,
    /// キーボリューム。0..=100 の整数で持ち、ランタイムでは /100 して扱う。
    pub key_volume: u32,
    /// キー音自動再生モード。true なら押鍵時のキー音を鳴らさず、譜面の生タイミング
    /// (入力オフセット・表示オフセットの影響を受けない) でキー音を自動再生する。
    /// 音量は key_volume を使う。音ズレが気になる環境での練習向け。
    #[serde(default)]
    pub auto_keysound: bool,
    /// `auto_keysound` 有効時、空押し (判定候補が無かった押下) の代替キー音も
    /// 鳴らすかどうか。既定は false (自動再生モードでは入力非依存を優先)。
    #[serde(default)]
    pub auto_keysound_fallback: bool,
    /// `auto_keysound` 有効時、地雷命中時の譜面指定キー音も鳴らすかどうか。
    /// 既定は true (無効化すると地雷を踏んでもダメージのみで無音になる)。
    #[serde(default = "default_auto_keysound_mine")]
    pub auto_keysound_mine: bool,
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

pub fn default_auto_keysound_mine() -> bool {
    true
}

pub fn default_normalize_system_bgm_volume() -> bool {
    true
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
