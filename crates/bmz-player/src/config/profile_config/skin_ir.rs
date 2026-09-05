use super::schema::{
    default_base_hispeed, default_classic_hispeed_step, default_floating_hispeed_step,
    default_floating_policy,
};
use super::*;

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
    /// 5K Battle (beatoraja skin type 13) のプレイ画面スキン。
    #[serde(default)]
    pub battle5: String,
    /// 7K Battle (beatoraja skin type 12) のプレイ画面スキン。
    #[serde(default)]
    pub battle7: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battle5_offsets: Vec<SkinOffsetConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battle7_offsets: Vec<SkinOffsetConfig>,
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
    #[serde(default)]
    pub battle5_options: BTreeMap<String, String>,
    #[serde(default)]
    pub battle7_options: BTreeMap<String, String>,
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
    #[serde(default)]
    pub battle5_files: BTreeMap<String, String>,
    #[serde(default)]
    pub battle7_files: BTreeMap<String, String>,
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
            &mut self.battle5_offsets,
            &mut self.battle7_offsets,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SkinOffsetConfig {
    /// スキンが宣言した offset 名。
    ///
    /// beatoraja は設定値を ID ではなく名前で対応付ける。旧プロファイルには
    /// このフィールドが無いため、`None` の場合だけ ID を移行用 fallback とする。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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

pub const BUILTIN_IR_PROVIDER_COUNT: usize = 3;

pub(super) fn default_true() -> bool {
    true
}

impl Default for IrConfig {
    fn default() -> Self {
        Self {
            primary_provider: String::new(),
            credential_store: IrCredentialStoreConfig::default(),
            providers: vec![
                IrProviderConfig::bmz_ir(),
                IrProviderConfig::rian_ir(),
                IrProviderConfig::bms_ir(),
            ],
            prefetch_global_ranking_on_score_submit: true,
            prefetch_rival_ranking_on_score_submit: true,
        }
    }
}

impl IrConfig {
    /// 先頭3枠を公式 BMZ IR / rianIR / BMS-IR に固定し、既存のカスタム provider を後続へ保つ。
    pub fn normalize_builtin_providers(&mut self) -> bool {
        let previous = self.providers.clone();
        let mut remaining = std::mem::take(&mut self.providers);

        let mut bmz = take_matching_provider(&mut remaining, is_bmz_ir_builtin)
            .unwrap_or_else(IrProviderConfig::bmz_ir);
        bmz.provider = crate::ir::bmz_official::BMZ_IR_PROVIDER.to_string();
        bmz.base_url = crate::ir::bmz_official::BMZ_IR_DEFAULT_BASE_URL.to_string();

        let mut rian = take_matching_provider(&mut remaining, is_rian_ir_builtin)
            .unwrap_or_else(IrProviderConfig::rian_ir);
        rian.provider = crate::ir::rian_ir::RIAN_IR_PROVIDER.to_string();
        rian.base_url = crate::ir::rian_ir::RIAN_IR_PUBLIC_BASE_URL.to_string();

        let mut bms_ir = take_matching_provider(&mut remaining, is_bms_ir_builtin)
            .unwrap_or_else(IrProviderConfig::bms_ir);
        bms_ir.provider = crate::ir::bms_ir::BMS_IR_PROVIDER.to_string();
        bms_ir.base_url = crate::ir::bms_ir::BMS_IR_DEFAULT_BASE_URL.to_string();

        self.providers = Vec::with_capacity(remaining.len() + BUILTIN_IR_PROVIDER_COUNT);
        self.providers.push(bmz);
        self.providers.push(rian);
        self.providers.push(bms_ir);
        self.providers.extend(remaining);
        self.providers != previous
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

impl IrProviderConfig {
    pub fn bmz_ir() -> Self {
        Self::new(
            crate::ir::bmz_official::BMZ_IR_PROVIDER,
            crate::ir::bmz_official::BMZ_IR_DEFAULT_BASE_URL,
        )
    }

    pub fn rian_ir() -> Self {
        Self::new(crate::ir::rian_ir::RIAN_IR_PROVIDER, crate::ir::rian_ir::RIAN_IR_PUBLIC_BASE_URL)
    }

    pub fn bms_ir() -> Self {
        Self::new(crate::ir::bms_ir::BMS_IR_PROVIDER, crate::ir::bms_ir::BMS_IR_DEFAULT_BASE_URL)
    }

    pub fn custom() -> Self {
        Self::new(crate::ir::bmz_official::BMZ_IR_PROVIDER, "")
    }

    fn new(provider: &str, base_url: &str) -> Self {
        Self {
            provider: provider.to_string(),
            provider_key: String::new(),
            base_url: base_url.to_string(),
            enabled: false,
            account_display_name: String::new(),
            account_id: String::new(),
            send_policy: IrSendPolicyConfig::default(),
            role: IrProviderRoleConfig::default(),
            last_login_at: None,
            last_success_at: None,
        }
    }
}

fn take_matching_provider(
    providers: &mut Vec<IrProviderConfig>,
    predicate: impl Fn(&IrProviderConfig) -> bool,
) -> Option<IrProviderConfig> {
    let best_priority = providers
        .iter()
        .filter(|provider| predicate(provider))
        .map(provider_migration_priority)
        .max()?;
    let index = providers.iter().position(|provider| {
        predicate(provider) && provider_migration_priority(provider) == best_priority
    })?;
    Some(providers.remove(index))
}

fn provider_migration_priority(provider: &IrProviderConfig) -> (bool, bool, i64) {
    (
        !provider.provider_key.trim().is_empty(),
        provider.enabled,
        provider.last_login_at.unwrap_or(i64::MIN),
    )
}

fn is_bmz_ir_builtin(provider: &IrProviderConfig) -> bool {
    !crate::ir::rian_ir::is_rian_ir_config(provider)
        && !crate::ir::bms_ir::is_bms_ir_config(provider)
        && normalized_ir_base_url(&provider.base_url)
            == normalized_ir_base_url(crate::ir::bmz_official::BMZ_IR_DEFAULT_BASE_URL)
}

fn is_bms_ir_builtin(provider: &IrProviderConfig) -> bool {
    crate::ir::bms_ir::is_bms_ir_config(provider)
        || normalized_ir_base_url(&provider.base_url)
            == normalized_ir_base_url(crate::ir::bms_ir::BMS_IR_DEFAULT_BASE_URL)
}

fn is_rian_ir_builtin(provider: &IrProviderConfig) -> bool {
    crate::ir::rian_ir::is_rian_ir_config(provider)
        && [
            crate::ir::rian_ir::RIAN_IR_PUBLIC_BASE_URL,
            crate::ir::rian_ir::RIAN_IR_DEFAULT_BASE_URL,
        ]
        .into_iter()
        .any(|url| normalized_ir_base_url(&provider.base_url) == normalized_ir_base_url(url))
}

pub fn normalized_ir_base_url(url: &str) -> Option<String> {
    let mut parsed = reqwest::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    parsed.set_fragment(None);
    parsed.set_query(None);
    let path = parsed.path().trim_end_matches('/').to_string();
    parsed.set_path(if path.is_empty() { "/" } else { &path });
    Some(parsed.to_string().trim_end_matches('/').to_ascii_lowercase())
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
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                seven_to_six: false,
                gauge: GaugeTypeConfig::Normal,
                gauge_auto_shift: GaugeAutoShiftConfig::Off,
                bottom_shiftable_gauge: BottomShiftableGaugeConfig::AssistEasy,
                random: RandomOptionConfig::Off,
                random2: RandomOptionConfig::Off,
                double_option: DoubleOptionConfig::Off,
                hs_fix: HsFixConfig::Off,
                target: TargetOptionConfig::None,
                lane_effect: LaneEffectConfig::Off,
                assist: AssistOptionConfig::default(),
                guide_se: false,
                note_retention: false,
                session_mode: Some(SessionMode::Normal),
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
                base_hispeed: default_base_hispeed(),
                floating_policy: default_floating_policy(),
                normal_hispeed_level: default_normal_hispeed_level(),
                classic_hispeed_step: default_classic_hispeed_step(),
                floating_hispeed_step: default_floating_hispeed_step(),
                sudden: 0,
                lift: 0,
                lift_enabled: true,
                hispeed_auto_adjust: true,
                hidden: 0,
                target_green_number: 300,
                constant_enabled: false,
                constant_fade_ms: default_constant_fade_ms(),
            },
            play_mode: BTreeMap::new(),
            active_play_mode: KeyMode::K7,
            input: crate::config::play_input::default_profile_input(),
            rival: RivalConfig {
                active_rival: String::new(),
                chart_replication_mode: ChartReplicationModeConfig::default(),
                entries: Vec::new(),
            },
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
                normalize_system_bgm_volume: true,
                master_volume: 50,
                key_volume: 50,
                auto_keysound: false,
                auto_keysound_fallback: false,
                auto_keysound_mine: true,
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
