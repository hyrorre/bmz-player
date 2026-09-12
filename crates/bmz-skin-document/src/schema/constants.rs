use super::*;

/// Skin value / slider 用の BMZ 組み込み式キー。
pub const SKIN_EXPR_ADJUSTED_COVER: &str = "bmz:adjusted_cover";
pub const SKIN_EXPR_ADJUSTED_RATE: &str = "bmz:adjusted_rate";
pub const SKIN_EXPR_ADJUSTED_RATE_ADOT: &str = "bmz:adjusted_rate_adot";
pub const SKIN_EXPR_FS_THRESHOLD: &str = "bmz:fs_threshold";
pub const SKIN_EXPR_COURSE_TABLE_TEXT: &str = "bmz:course_table_text";
pub const SKIN_EXPR_RESULT_TABLE_TITLE: &str = "bmz:result_table_title";
pub const SKIN_EXPR_DIFFICULTY_NAME: &str = "bmz:difficulty_name";
pub const SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT: &str = "bmz:fast_slow_breakdown_height";
pub const SKIN_EXPR_DEFAULT_CHART_TOTAL_COUNT: &str = "bmz:default_chart_total_count";
pub const SKIN_EXPR_DEFAULT_CHART_GAUGE: &str = "bmz:default_chart_gauge";
pub const SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER: &str = "bmz:select_total_notes_ratio_integer";
pub const SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION: &str =
    "bmz:select_total_notes_ratio_fraction";
pub const SKIN_EXPR_COURSE_CLEAR_RATE: &str = "bmz:course_clear_rate";
pub const SKIN_EXPR_GAUGE_PERCENT_INTEGER: &str = "bmz:gauge_percent_integer";
pub const SKIN_EXPR_GAUGE_PERCENT_FRACTION: &str = "bmz:gauge_percent_fraction";
pub const SKIN_EXPR_GAUGE_AMOUNT_INTEGER: &str = "bmz:gauge_amount_integer";
pub const SKIN_EXPR_GAUGE_AMOUNT_FRACTION: &str = "bmz:gauge_amount_fraction";

/// beatoraja 予約 ID と衝突しない動的タイマー ID 範囲の先頭。
pub const SKIN_DYNAMIC_TIMER_BASE: i32 = 9000;
/// Play 中 imageset が `main_state.gauge_type()` で選ぶ ref (beatoraja 非予約)。
pub const SKIN_REF_PLAY_GAUGE_TYPE: i32 = 44;
/// beatoraja `BUTTON_HSFIX` (`event_index(55)`)。
pub const SKIN_EVENT_HSFIX: i32 = 55;
/// BMZ extension: exact key mode number (`4`, `5`, `6`, `7`, `8`, `9`, `10`, `14`).
pub const SKIN_REF_BMZ_KEY_MODE: i32 = 1903;
/// BMZ extension: active physical lane count including scratch lanes.
pub const SKIN_REF_BMZ_ACTIVE_LANE_COUNT: i32 = 1904;
pub const SKIN_REF_BMZ_BASE_HISPEED: i32 = 1916;
pub const SKIN_REF_BMZ_NORMAL_HISPEED_LEVEL: i32 = 1917;
pub const SKIN_REF_BMZ_HISPEED_CONFIG: i32 = 1918;
/// BMZ extension: exact key mode options in K4/K5/K6/K7/K8/K9/K10/K14 order.
pub const SKIN_OPTION_BMZ_KEY_MODE_BASE: i32 = 1905;
pub const SKIN_OPTION_BMZ_KEY_MODE_COUNT: usize = 8;
pub const SKIN_OPTION_BMZ_KEY_MODE_LAST: i32 = 1912;
/// BMZ extension: scratch layout options.
pub const SKIN_OPTION_BMZ_NO_SCRATCH: i32 = 1913;
pub const SKIN_OPTION_BMZ_SINGLE_PLAY: i32 = 1914;
pub const SKIN_OPTION_BMZ_DOUBLE_PLAY: i32 = 1915;
/// BMZ extension: E1/E2/E3/E4/UI Left/Right/Up/Down held options.
pub const SKIN_OPTION_BMZ_INPUT_BASE: i32 = 1920;
pub const SKIN_OPTION_BMZ_INPUT_LAST: i32 = 1927;
pub const SKIN_BMZ_INPUT_COUNT: usize = 8;
/// BMZ extension: E1/E2 aggregate held option for shared control panels.
pub const SKIN_OPTION_BMZ_E1_E2_HELD: i32 = 1928;
/// BMZ extension: matching press-edge timers.
pub const SKIN_TIMER_BMZ_INPUT_BASE: i32 = 19_000;
pub const SKIN_TIMER_BMZ_INPUT_LAST: i32 = 19_007;
/// BMZ extension: mutually exclusive E1/E2 aggregate press/hold and release timers.
pub const SKIN_TIMER_BMZ_E1_E2_PRESS: i32 = 19_008;
pub const SKIN_TIMER_BMZ_E1_E2_RELEASE: i32 = 19_009;
/// BMZ extension: latest judgement timers split by judge region and lane kind.
/// Slots are region 0 Scratch/Keys, region 1 Scratch/Keys, region 2 Scratch/Keys.
pub const SKIN_TIMER_BMZ_JUDGE_LANE_BASE: i32 = 19_010;
pub const SKIN_TIMER_BMZ_JUDGE_LANE_LAST: i32 = 19_015;
pub const SKIN_BMZ_JUDGE_LANE_COUNT: usize = 6;
/// BMZ extension: PGREAT options for the split judgement slots.
pub const SKIN_OPTION_BMZ_JUDGE_LANE_PGREAT_BASE: i32 = 19_020;
pub const SKIN_OPTION_BMZ_JUDGE_LANE_PGREAT_LAST: i32 = 19_025;
/// BMZ extension: FAST/EARLY options for the split judgement slots.
pub const SKIN_OPTION_BMZ_JUDGE_LANE_FAST_BASE: i32 = 19_030;
pub const SKIN_OPTION_BMZ_JUDGE_LANE_FAST_LAST: i32 = 19_035;
/// BMZ extension: SLOW/LATE options for the split judgement slots.
pub const SKIN_OPTION_BMZ_JUDGE_LANE_SLOW_BASE: i32 = 19_040;
pub const SKIN_OPTION_BMZ_JUDGE_LANE_SLOW_LAST: i32 = 19_045;
/// BMZ extension: timing difference refs for the split judgement slots.
pub const SKIN_REF_BMZ_JUDGE_LANE_DURATION_BASE: i32 = 19_050;
pub const SKIN_REF_BMZ_JUDGE_LANE_DURATION_LAST: i32 = 19_055;
/// BMZ extension: generic daily statistics number refs.
pub const SKIN_REF_BMZ_DAILY_BASE: i32 = 1930;
pub const SKIN_REF_BMZ_DAILY_LAST: i32 = 1946;
/// BMZ extension: daily rank label and recent title text refs.
pub const SKIN_TEXT_BMZ_DAILY_RANK: i32 = 1943;
pub const SKIN_TEXT_BMZ_DAILY_RECENT_BASE: i32 = 1950;
pub const SKIN_TEXT_BMZ_DAILY_RECENT_LAST: i32 = 1959;
/// BMZ extension: select settings row kind (`0=other`, `1=folder`, `2=back`, `3=close`).
pub const SKIN_REF_BMZ_SELECT_SETTINGS_ROW_KIND: i32 = 1960;
/// BMZ extension: select settings folder/back/close row options.
pub const SKIN_OPTION_BMZ_SETTINGS_FOLDER: i32 = 1961;
pub const SKIN_OPTION_BMZ_SETTINGS_BACK: i32 = 1962;
pub const SKIN_OPTION_BMZ_SETTINGS_CLOSE: i32 = 1963;
/// BMZ extension: IR scope index and label (`0=Ranking`, `1=Rival`).
pub const SKIN_REF_BMZ_IR_SCOPE: i32 = 1964;
/// BMZ extension: IR scope selected options.
pub const SKIN_OPTION_BMZ_IR_SCOPE_GLOBAL: i32 = 1965;
pub const SKIN_OPTION_BMZ_IR_SCOPE_RIVAL: i32 = 1966;
/// BMZ extension: IR scope availability options.
pub const SKIN_OPTION_BMZ_IR_SCOPE_GLOBAL_SUPPORTED: i32 = 1967;
pub const SKIN_OPTION_BMZ_IR_SCOPE_RIVAL_SUPPORTED: i32 = 1968;
/// BMZ extension: number of players in the displayed IR scope.
pub const SKIN_REF_BMZ_IR_SCOPE_TOTAL: i32 = 1969;
/// BMZ extension: select session mode (`0=NORMAL`, `1=AUTOPLAY`,
/// `2=AUTO BATTLE`, `3=BATTLE`, `4=PRACTICE`).
pub const SKIN_REF_BMZ_SELECT_SESSION_MODE: i32 = 1970;
/// Deprecated BMZ extension: the removed grade difference display setting.
///
/// These IDs remain reserved for old BMZ skins. The renderer exposes a fixed
/// NEXT mode (`1971=1`, `1972=false`, `1973=true`).
pub const SKIN_REF_BMZ_GRADE_DIFF_DISPLAY: i32 = 1971;
pub const SKIN_OPTION_BMZ_GRADE_DIFF_NEAREST: i32 = 1972;
pub const SKIN_OPTION_BMZ_GRADE_DIFF_NEXT: i32 = 1973;
/// BMZ extension: exact DJ LEVEL border facts.
///
/// Grade indices use `0=F`, `1=E`, `2=D`, `3=C`, `4=B`, `5=A`, `6=AA`,
/// `7=AAA`, `8=MAX`.
pub const SKIN_REF_BMZ_SCORE_GRADE_CURRENT: i32 = 1974;
pub const SKIN_REF_BMZ_SCORE_GRADE_NEXT: i32 = 1975;
pub const SKIN_REF_BMZ_SCORE_GRADE_NEAREST: i32 = 1976;
/// EX SCORE gained since the current lower grade border.
pub const SKIN_REF_BMZ_SCORE_GRADE_CURRENT_DIFF: i32 = 1977;
/// Signed difference from the next higher grade border (`score - border`).
/// This remains independent from beatoraja's positive `ref=154` value.
pub const SKIN_REF_BMZ_SCORE_GRADE_NEXT_DIFF: i32 = 1978;
/// Signed distance from the nearest border (`score - border`).
pub const SKIN_REF_BMZ_SCORE_GRADE_NEAREST_DIFF: i32 = 1979;
/// Absolute distance from the nearest border.
pub const SKIN_REF_BMZ_SCORE_GRADE_NEAREST_ABS: i32 = 1980;
/// BMZ extension: nearest-border selection and score availability options.
pub const SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_CURRENT: i32 = 1981;
pub const SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_NEXT: i32 = 1982;
pub const SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_EXACT: i32 = 1983;
pub const SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_TIE: i32 = 1984;
pub const SKIN_OPTION_BMZ_SCORE_GRADE_AVAILABLE: i32 = 1985;
/// BMZ extension: no persisted best existed for the current score key when
/// the play attempt started. Result keeps the value captured by that attempt.
pub const SKIN_OPTION_BMZ_FIRST_PLAY: i32 = 1986;
/// BMZ extension: scoring rule mode (`0=BEATORAJA`, `1=LR2ORAJA`, `2=DX`).
pub const SKIN_REF_BMZ_RULE_MODE: i32 = 1987;
/// BMZ extension: exact scoring rule mode options in BEATORAJA/LR2ORAJA/DX order.
pub const SKIN_OPTION_BMZ_RULE_MODE_BASE: i32 = 1988;
pub const SKIN_OPTION_BMZ_RULE_MODE_COUNT: usize = 3;
pub const SKIN_OPTION_BMZ_RULE_MODE_LAST: i32 = 1990;
/// BMZ LR2 conversion bridge: built-in Judge Detail selection.
/// Keep 1997..=1999 reserved so public BMZ extensions cannot shadow generated LR2 ops.
pub const SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_OFF: i32 = 1997;
pub const SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_EARLY_LATE: i32 = 1998;
pub const SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_MS: i32 = 1999;
/// Backward-compatible Rust aliases for the initial Result-only names.
pub const SKIN_REF_BMZ_RESULT_IR_SCOPE: i32 = SKIN_REF_BMZ_IR_SCOPE;
pub const SKIN_OPTION_BMZ_RESULT_IR_SCOPE_GLOBAL: i32 = SKIN_OPTION_BMZ_IR_SCOPE_GLOBAL;
pub const SKIN_OPTION_BMZ_RESULT_IR_SCOPE_RIVAL: i32 = SKIN_OPTION_BMZ_IR_SCOPE_RIVAL;
pub const SKIN_OPTION_BMZ_RESULT_IR_SCOPE_GLOBAL_SUPPORTED: i32 =
    SKIN_OPTION_BMZ_IR_SCOPE_GLOBAL_SUPPORTED;
pub const SKIN_OPTION_BMZ_RESULT_IR_SCOPE_RIVAL_SUPPORTED: i32 =
    SKIN_OPTION_BMZ_IR_SCOPE_RIVAL_SUPPORTED;
pub const SKIN_REF_BMZ_RESULT_IR_SCOPE_TOTAL: i32 = SKIN_REF_BMZ_IR_SCOPE_TOTAL;
/// BMZ extension: course result stage count and ten stage slots.
pub const SKIN_REF_BMZ_COURSE_STAGE_COUNT: i32 = 19_100;
pub const SKIN_REF_BMZ_COURSE_STAGE_EX_BASE: i32 = 19_110;
pub const SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE: i32 = 19_120;
pub const SKIN_REF_BMZ_COURSE_STAGE_BP_BASE: i32 = 19_130;
pub const SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE: i32 = 19_140;
pub const SKIN_BMZ_COURSE_STAGE_COUNT: usize = 10;
/// BMZ extension: normalized LN policy stored in the score key.
/// The index order is AUTO(LN/CN/HCN), FORCE(LN/CN/HCN).
pub const SKIN_REF_BMZ_LN_SCORE_POLICY: i32 = 19_150;
pub const SKIN_OPTION_BMZ_LN_SCORE_POLICY_BASE: i32 = 19_151;
pub const SKIN_OPTION_BMZ_LN_SCORE_POLICY_COUNT: usize = 6;
pub const SKIN_OPTION_BMZ_LN_SCORE_POLICY_LAST: i32 = 19_156;
pub const SKIN_OPTION_BMZ_LN_SCORE_POLICY_AUTO: i32 = 19_157;
pub const SKIN_OPTION_BMZ_LN_SCORE_POLICY_FORCE: i32 = 19_158;
pub const SKIN_OPTION_BMZ_LN_SCORE_POLICY_AVAILABLE: i32 = 19_159;
/// BMZ extension: profile LN policy setting before chart-dependent normalization.
/// The index order is AUTO(LN/CN/HCN), FORCE(LN/CN/HCN).
pub const SKIN_REF_BMZ_LN_POLICY_SETTING: i32 = 19_160;
pub const SKIN_OPTION_BMZ_LN_POLICY_SETTING_BASE: i32 = 19_161;
pub const SKIN_OPTION_BMZ_LN_POLICY_SETTING_COUNT: usize = 6;
pub const SKIN_OPTION_BMZ_LN_POLICY_SETTING_LAST: i32 = 19_166;
pub const SKIN_OPTION_BMZ_LN_POLICY_SETTING_AUTO: i32 = 19_167;
pub const SKIN_OPTION_BMZ_LN_POLICY_SETTING_FORCE: i32 = 19_168;
/// BMZ LR2 conversion bridge: modified-LR2 `ref=210` FAST/SLOW state.
/// Values are `0=blank`, `1=FAST`, `2=SLOW`. LR2 play skin decode rewrites
/// the legacy ref to this ID so beatoraja's standard `ref=210` meaning remains intact.
pub const SKIN_REF_BMZ_LR2_FAST_SLOW_1P: i32 = 19_170;
/// LR2 play conversion bridges; kept separate from beatoraja image/value IDs.
pub const SKIN_REF_BMZ_LR2_HISPEED: i32 = 19_171;
pub const SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P: i32 = 19_172;
pub const SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P: i32 = 19_173;
pub const SKIN_REF_BMZ_LR2_GAUGE_2P: i32 = 19_174;
/// BMZ extension: source chart key mode before BATTLE / 7K-to-6K conversion.
pub const SKIN_REF_BMZ_SOURCE_KEY_MODE: i32 = 19_180;
/// BMZ extension: exact source key mode options in K4/K5/K6/K7/K8/K9/K10/K14 order.
pub const SKIN_OPTION_BMZ_SOURCE_KEY_MODE_BASE: i32 = 19_181;
pub const SKIN_OPTION_BMZ_SOURCE_KEY_MODE_COUNT: usize = 8;
pub const SKIN_OPTION_BMZ_SOURCE_KEY_MODE_LAST: i32 = 19_188;
/// BMZ extension: the active chart was converted from source 7K to effective 6K.
pub const SKIN_OPTION_BMZ_SEVEN_TO_SIX: i32 = 19_189;
/// BMZ extension: source-chart LN profile bit mask.
/// bit0=undefined LN, bit1=defined LN, bit2=defined CN, bit3=defined HCN.
pub const SKIN_REF_BMZ_SOURCE_LN_PROFILE: i32 = 19_190;
pub const SKIN_OPTION_BMZ_SOURCE_LN_UNDEFINED: i32 = 19_191;
pub const SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_LN: i32 = 19_192;
pub const SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_CN: i32 = 19_193;
pub const SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_HCN: i32 = 19_194;
pub const SKIN_OPTION_BMZ_SOURCE_LN_MIXED: i32 = 19_195;
pub const SKIN_OPTION_BMZ_SOURCE_LN_PROFILE_AVAILABLE: i32 = 19_196;
/// Lua result skin の定数 `Expand_op` 代入を宣言的クリックイベントへ変換する ID。
/// beatoraja の正数イベント ID と衝突しない BMZ 内部予約値を使う。
pub const SKIN_EVENT_RESULT_PANEL_IR: i32 = -10_001;
pub const SKIN_EVENT_RESULT_PANEL_GRAPH: i32 = -10_002;
/// BMZ IR scope selection events for compatible skins.
pub const SKIN_EVENT_IR_SCOPE_GLOBAL: i32 = -10_003;
pub const SKIN_EVENT_IR_SCOPE_RIVAL: i32 = -10_004;
pub const SKIN_EVENT_IR_SCOPE_TOGGLE: i32 = -10_005;
/// Backward-compatible Rust aliases for the initial Result-only names.
pub const SKIN_EVENT_RESULT_IR_SCOPE_GLOBAL: i32 = SKIN_EVENT_IR_SCOPE_GLOBAL;
pub const SKIN_EVENT_RESULT_IR_SCOPE_RIVAL: i32 = SKIN_EVENT_IR_SCOPE_RIVAL;
pub const SKIN_EVENT_RESULT_IR_SCOPE_TOGGLE: i32 = SKIN_EVENT_IR_SCOPE_TOGGLE;
/// Clear the visible daily statistics window without deleting score history.
pub const SKIN_EVENT_DAILY_STATISTICS_RESET: i32 = -10_100;
/// Lua callback から変換する runtime event の内部予約 ID 範囲。
/// beatoraja 正数イベント ID と衝突しないよう負数を使う。
pub const SKIN_EVENT_RUNTIME_BASE: i32 = -20_000;
/// beatoraja `NUMBER_RANDOM_1P_1KEY..NUMBER_RANDOM_2P_SCR` (450..469).
/// BMZではResult互換に加え、Play/Selectの確定済み固定配置にも使用する。
pub const SKIN_RANDOM_LANE_REF_BASE: i32 = 450;
pub const SKIN_RANDOM_LANE_REF_COUNT: usize = 20;
/// `SkinDrawState::dynamic_timer_ms` のスロット数。
pub const SKIN_DYNAMIC_TIMER_COUNT: usize = 64;

/// Which IR ranking supplies standard IR refs for this skin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrScopeBinding {
    /// Preserve beatoraja-compatible global ranking behavior.
    #[default]
    Global,
    /// Bind standard IR refs to the scope currently selected by the player.
    Active,
}

/// Backward-compatible type alias for the initial Result-only name.
pub type ResultIrScopeBinding = IrScopeBinding;

/// Optional Result IR scope switch input declared by a BMZ-compatible skin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultIrScopeToggle {
    #[default]
    None,
    E1Press,
}

/// Optional Select IR scope switch input declared by a BMZ-compatible skin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectIrScopeToggle {
    #[default]
    None,
    E3Press,
}

pub fn string_array_refs(values: &[String; 10]) -> [&str; 10] {
    std::array::from_fn(|index| values[index].as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinDynamicTimerDef {
    pub id: i32,
    pub observe: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinFixedDelayTimerDef {
    pub id: i32,
    #[serde(rename = "sourceTimer")]
    pub source_timer: i32,
    #[serde(rename = "delayMs")]
    pub delay_ms: i32,
}

/// 描画ランタイムで保持する bool フラグの初期値。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinRuntimeFlagDef {
    pub id: i32,
    #[serde(default)]
    pub initial: bool,
}

/// event ID を受けて複数の runtime flag を反転する宣言的イベント。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinRuntimeEventDef {
    pub id: i32,
    #[serde(default, rename = "toggleFlags")]
    pub toggle_flags: Vec<i32>,
    /// BMZ extension: logical input press edge that dispatches this runtime event.
    #[serde(default, rename = "triggerAction")]
    pub trigger_action: Option<SkinRuntimeTriggerAction>,
}

/// Runtime event trigger that does not require Lua-side input or file access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinRuntimeTriggerAction {
    E1Press,
    E2Press,
    E3Press,
    E4Press,
    UiLeftPress,
    UiRightPress,
    UiUpPress,
    UiDownPress,
}

impl SkinRuntimeTriggerAction {
    pub const fn index(self) -> usize {
        match self {
            Self::E1Press => 0,
            Self::E2Press => 1,
            Self::E3Press => 2,
            Self::E4Press => 3,
            Self::UiLeftPress => 4,
            Self::UiRightPress => 5,
            Self::UiUpPress => 6,
            Self::UiDownPress => 7,
        }
    }
}

/// スキン音声に対する宣言的な再生・停止命令。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinAudioActionDef {
    pub action: SkinAudioActionKind,
    pub path: String,
    #[serde(default = "default_skin_audio_volume")]
    pub volume: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinAudioActionKind {
    Play,
    Loop,
    Stop,
}

/// 条件が単一 timer の ON へ落とせる Lua `customEvents` 定義。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinCustomEventDef {
    pub id: i32,
    #[serde(default)]
    pub timer: i32,
    #[serde(default)]
    pub once: bool,
    #[serde(default, rename = "audioActions")]
    pub audio_actions: Vec<SkinAudioActionDef>,
}

fn default_skin_audio_volume() -> f32 {
    1.0
}
