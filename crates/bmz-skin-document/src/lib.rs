//! beatoraja JSON skin の document スキーマ (schema/decode 専用 crate)。
//!
//! `bmz-render` (描画評価) と `bmz-skin` (Lua/LR2 decode) の両方から使う
//! `SkinDocument` 型群・JSON ロード/前処理・serde ヘルパを持つ。
//! wgpu / egui 等の描画依存は持たない。

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value as JsonValue;

mod load;
mod runtime;
#[cfg(test)]
mod tests;

pub use load::*;
pub use runtime::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinTextureId(pub u32);

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinDocument {
    #[serde(default, rename = "type")]
    pub skin_type: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_skin_canvas_width")]
    pub w: u32,
    #[serde(default = "default_skin_canvas_height")]
    pub h: u32,
    #[serde(default)]
    pub fadeout: i32,
    #[serde(default)]
    pub input: i32,
    #[serde(default)]
    pub ranktime: i32,
    #[serde(default)]
    pub scene: i32,
    #[serde(default)]
    pub close: i32,
    #[serde(default)]
    pub loadstart: i32,
    #[serde(default)]
    pub loadend: i32,
    #[serde(default)]
    pub playstart: i32,
    #[serde(default = "default_judgetimer")]
    pub judgetimer: i32,
    #[serde(default)]
    pub finishmargin: i32,
    #[serde(default)]
    pub category: Vec<SkinCategoryDef>,
    #[serde(default)]
    pub property: Vec<SkinPropertyDef>,
    #[serde(default)]
    pub filepath: Vec<SkinFilepathDef>,
    #[serde(default)]
    pub offset: Vec<SkinOffsetDef>,
    #[serde(default)]
    pub source: Vec<SkinSourceDef>,
    #[serde(default)]
    pub font: Vec<SkinFontDef>,
    #[serde(default)]
    pub image: Vec<SkinImageDef>,
    #[serde(default)]
    pub imageset: Vec<SkinImageSetDef>,
    #[serde(default)]
    pub value: Vec<SkinValueDef>,
    #[serde(default)]
    pub text: Vec<SkinTextDef>,
    #[serde(default)]
    pub slider: Vec<SkinSliderDef>,
    #[serde(default)]
    pub graph: Vec<SkinGraphDef>,
    #[serde(default, rename = "hiddenCover")]
    pub hidden_cover: Vec<SkinHiddenCoverDef>,
    #[serde(default, rename = "liftCover", deserialize_with = "deserialize_lift_cover_defs")]
    pub lift_cover: Vec<SkinHiddenCoverDef>,
    #[serde(default, rename = "hiterrorvisualizer")]
    pub hiterror_visualizer: Vec<SkinHitErrorVisualizerDef>,
    #[serde(default)]
    pub timingvisualizer: Vec<SkinTimingVisualizerDef>,
    #[serde(default)]
    pub timingdistributiongraph: Vec<SkinTimingDistributionGraphDef>,
    #[serde(default)]
    pub gaugegraph: Vec<SkinGaugeGraphDef>,
    #[serde(default)]
    pub judgegraph: Vec<SkinJudgeGraphDef>,
    #[serde(default)]
    pub bpmgraph: Vec<SkinBpmGraphDef>,
    pub note: Option<SkinNoteSetDef>,
    pub gauge: Option<SkinGaugeDef>,
    #[serde(default)]
    pub gauges: Vec<SkinGaugeDef>,
    #[serde(default)]
    pub judge: Vec<SkinJudgeDef>,
    pub bga: Option<SkinBgaDef>,
    pub songlist: Option<SkinSongListDef>,
    #[serde(default)]
    pub destination: Vec<DestinationListEntry>,
    /// Lua `timer_util.timer_observe_boolean` から変換された動的タイマー定義。
    #[serde(default, rename = "dynamicTimer")]
    pub dynamic_timers: Vec<SkinDynamicTimerDef>,
    /// Lua `customTimers` のうち、既存タイマー開始時刻へ固定 delay を加える定義。
    #[serde(default, rename = "fixedDelayTimer")]
    pub fixed_delay_timers: Vec<SkinFixedDelayTimerDef>,
    /// Lua skin callback をロード時に変換した、初期値を持つ内部フラグ。
    #[serde(default, rename = "runtimeFlag")]
    pub runtime_flags: Vec<SkinRuntimeFlagDef>,
    /// Lua skin callback をロード時に変換した、内部フラグのトグルイベント。
    #[serde(default, rename = "runtimeEvent")]
    pub runtime_events: Vec<SkinRuntimeEventDef>,
    /// Lua のロード中に呼ばれた `main_state.audio_*` をシーン開始時の命令へ変換したもの。
    #[serde(default, rename = "sceneAudio")]
    pub scene_audio: Vec<SkinAudioActionDef>,
    /// Lua `customEvents` のうち、タイマー開始を条件とする宣言的な音声イベント。
    #[serde(default, rename = "customEvents")]
    pub custom_events: Vec<SkinCustomEventDef>,
    /// Lua Result スキンがロード時に選んだ展開パネル。
    ///
    /// WMII の `Expand_op` をロード時宣言へ変換した場合だけ設定され、
    /// 0=非表示、1=IR、2=グラフとして Result 入力と描画状態を同期する。
    #[serde(default, rename = "resultPanelDefault")]
    pub result_panel_default: Option<i32>,
    /// ユーザがスキン設定パネルで選んだオプションから算出した有効 op コード列。
    /// `Some` のときレンダー時の `enabled_options()` はこれを返し、`None` の
    /// ときは従来通り `property.def` (または各 property の先頭 item) を既定として
    /// 計算する。
    #[serde(skip)]
    pub user_selected_options: Option<Vec<i32>>,
    /// LR2 `#SETOPTION` など、設定 UI に出さず内部的に有効化する op。
    #[serde(skip, default)]
    pub internal_enabled_options: Vec<i32>,
    /// プレイ描画時のみ plan 側が設定する judgegraph 密度。
    #[serde(skip, default)]
    pub play_judge_graph_density: Vec<u8>,
    /// プレイ描画時のみ plan 側が設定する bpmgraph 線分。
    #[serde(skip, default)]
    pub play_bpm_graph_segments: Vec<BpmGraphSegment>,
    /// リザルト描画時のみ plan 側が設定する gaugegraph 推移。
    #[serde(skip, default)]
    pub result_gauge_graph_points: Vec<ResultGaugeGraphPoint>,
    /// リザルト描画時のみ plan 側が設定する timing graph 推移。
    #[serde(skip, default)]
    pub result_timing_points: Vec<ResultTimingPoint>,
    /// リザルト描画時のみ plan 側が設定する judgegraph(type=1) 用の秒別 state 集計。
    #[serde(skip, default)]
    pub result_judge_graph_buckets: Vec<ResultJudgeGraphBucket>,
    /// リザルト描画時のみ plan 側が設定する judgegraph(type=2) 用の FAST/SLOW 秒別集計。
    #[serde(skip, default)]
    pub result_early_late_graph_buckets: Vec<ResultEarlyLateGraphBucket>,
    /// リザルト描画時のみ plan 側が設定する timingdistributiongraph 用の固定分布。
    #[serde(skip, default)]
    pub result_timing_distribution: ResultTimingDistribution,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SkinSongListDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub center: i32,
    #[serde(default)]
    pub clickable: Vec<i32>,
    #[serde(default)]
    pub listoff: Vec<DestinationListEntry>,
    #[serde(default)]
    pub liston: Vec<DestinationListEntry>,
    #[serde(default)]
    pub text: Vec<DestinationListEntry>,
    #[serde(default)]
    pub level: Vec<DestinationListEntry>,
    #[serde(default)]
    pub lamp: Vec<DestinationListEntry>,
    #[serde(default)]
    pub playerlamp: Vec<DestinationListEntry>,
    #[serde(default)]
    pub rivallamp: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_destination_entries")]
    pub trophy: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_destination_entries")]
    pub graph: Vec<DestinationListEntry>,
    #[serde(default, deserialize_with = "deserialize_destination_entries")]
    pub label: Vec<DestinationListEntry>,
    #[serde(default)]
    pub judgegraph: Vec<DestinationListEntry>,
    #[serde(default)]
    pub bpmgraph: Vec<DestinationListEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinCategoryDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub item: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinPropertyDef {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub item: Vec<SkinPropertyItemDef>,
    #[serde(default)]
    pub def: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinPropertyItemDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub op: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinFilepathDef {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub def: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinOffsetDef {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub x: bool,
    #[serde(default)]
    pub y: bool,
    #[serde(default)]
    pub w: bool,
    #[serde(default)]
    pub h: bool,
    #[serde(default)]
    pub r: bool,
    #[serde(default)]
    pub a: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinSourceDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinFontDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "type")]
    pub font_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinImageDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default)]
    pub len: i32,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default)]
    pub click: i32,
    #[serde(default)]
    pub act: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinImageSetDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub images: Vec<String>,
    #[serde(default)]
    pub click: i32,
    #[serde(default)]
    pub act: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
pub struct SkinValueDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default)]
    pub align: i32,
    #[serde(default)]
    pub digit: i32,
    #[serde(default)]
    pub padding: i32,
    #[serde(default)]
    pub zeropadding: i32,
    #[serde(default)]
    pub space: i32,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default)]
    pub expr: String,
    /// Lua `value = function()` から変換した浮動小数 digit 式。空なら `expr` / `ref` を使う。
    #[serde(default)]
    pub value_expr: String,
    #[serde(default)]
    pub offset: Vec<SkinValueDef>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct SkinTextDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub font: String,
    #[serde(default)]
    pub size: i32,
    #[serde(default)]
    pub align: i32,
    #[serde(default, rename = "ref")]
    pub ref_id: i32,
    #[serde(default, rename = "constantText", deserialize_with = "deserialize_skin_string")]
    pub constant_text: String,
    /// BMZ extension: render a numeric skin ref with the text renderer.
    /// beatoraja-compatible value sprites remain supported; this is used by the bundled
    /// default JSON skin to avoid shipping a separate digit atlas.
    #[serde(default, rename = "numberRef")]
    pub number_ref: Option<i32>,
    /// BMZ extension: render the latest judgement text for a judge region.
    /// Region 0 corresponds to the normal 1P judgement area.
    #[serde(default, rename = "judgeRegion")]
    pub judge_region: Option<usize>,
    /// BMZ extension: color `judgeRegion` text by judgement category.
    #[serde(default, rename = "judgeColor")]
    pub judge_color: bool,
    /// BMZ extension: render FAST/SLOW text for a judge region.
    #[serde(default, rename = "judgeTimingRegion")]
    pub judge_timing_region: Option<usize>,
    /// BMZ extension: color `judgeTimingRegion` text by FAST/SLOW side.
    #[serde(default, rename = "judgeTimingColor")]
    pub judge_timing_color: bool,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
    #[serde(default)]
    pub wrapping: bool,
    #[serde(default)]
    pub overflow: i32,
    #[serde(default, rename = "outlineColor")]
    pub outline_color: String,
    #[serde(default, rename = "outlineWidth")]
    pub outline_width: f32,
    #[serde(default, rename = "shadowColor")]
    pub shadow_color: String,
    #[serde(default, rename = "shadowOffsetX")]
    pub shadow_offset_x: f32,
    #[serde(default, rename = "shadowOffsetY")]
    pub shadow_offset_y: f32,
    #[serde(default, rename = "shadowSmoothness")]
    pub shadow_smoothness: f32,
    /// Lua `value = function()` から変換したコース表テキスト式。空なら `ref` を使う。
    #[serde(default)]
    pub value_expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinSliderDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default)]
    pub angle: i32,
    #[serde(default)]
    pub range: i32,
    #[serde(default, rename = "type")]
    pub slider_type: i32,
    #[serde(default = "default_true")]
    pub changeable: bool,
    #[serde(default, rename = "isRefNum")]
    pub is_ref_num: bool,
    #[serde(default)]
    pub min: i32,
    #[serde(default)]
    pub max: i32,
    /// Lua `value = function()` から変換した slider 進捗式 (0.0–1.0)。空なら `type` を使う。
    #[serde(default)]
    pub value_expr: String,
}

/// beatoraja `judgegraph[]` 要素。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinJudgeGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub graph_type: i32,
    #[serde(default, rename = "type")]
    pub type_alias: i32,
    #[serde(default, rename = "backTexOff")]
    pub back_tex_off: i32,
    #[serde(default)]
    pub delay: i32,
    #[serde(default, rename = "orderReverse")]
    pub order_reverse: i32,
    #[serde(default, rename = "noGap")]
    pub no_gap: i32,
    #[serde(default, rename = "noGapX")]
    pub no_gap_x: i32,
}

impl SkinJudgeGraphDef {
    pub fn graph_type(&self) -> i32 {
        if self.graph_type != 0 { self.graph_type } else { self.type_alias }
    }
}

/// beatoraja `gaugegraph[]` 要素。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinGaugeGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub color: Vec<String>,
    #[serde(default = "default_gaugegraph_assist_clear_bg_color", rename = "assistClearBGColor")]
    pub assist_clear_bg_color: String,
    #[serde(
        default = "default_gaugegraph_assist_easy_fail_bg_color",
        rename = "assistAndEasyFailBGColor"
    )]
    pub assist_and_easy_fail_bg_color: String,
    #[serde(default = "default_gaugegraph_groove_fail_bg_color", rename = "grooveFailBGColor")]
    pub groove_fail_bg_color: String,
    #[serde(
        default = "default_gaugegraph_groove_clear_hard_bg_color",
        rename = "grooveClearAndHardBGColor"
    )]
    pub groove_clear_and_hard_bg_color: String,
    #[serde(default = "default_gaugegraph_exhard_bg_color", rename = "exHardBGColor")]
    pub ex_hard_bg_color: String,
    #[serde(default = "default_gaugegraph_hazard_bg_color", rename = "hazardBGColor")]
    pub hazard_bg_color: String,
    #[serde(
        default = "default_gaugegraph_assist_clear_line_color",
        rename = "assistClearLineColor"
    )]
    pub assist_clear_line_color: String,
    #[serde(
        default = "default_gaugegraph_assist_easy_fail_line_color",
        rename = "assistAndEasyFailLineColor"
    )]
    pub assist_and_easy_fail_line_color: String,
    #[serde(default = "default_gaugegraph_groove_fail_line_color", rename = "grooveFailLineColor")]
    pub groove_fail_line_color: String,
    #[serde(
        default = "default_gaugegraph_groove_clear_hard_line_color",
        rename = "grooveClearAndHardLineColor"
    )]
    pub groove_clear_and_hard_line_color: String,
    #[serde(default = "default_gaugegraph_exhard_line_color", rename = "exHardLineColor")]
    pub ex_hard_line_color: String,
    #[serde(default = "default_gaugegraph_hazard_line_color", rename = "hazardLineColor")]
    pub hazard_line_color: String,
    #[serde(default = "default_gaugegraph_borderline_color", rename = "borderlineColor")]
    pub borderline_color: String,
    #[serde(default = "default_gaugegraph_border_color", rename = "borderColor")]
    pub border_color: String,
}

/// beatoraja `bpmgraph[]` 要素。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinBpmGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub delay: i32,
    #[serde(default, rename = "lineWidth")]
    pub line_width: i32,
    #[serde(default, rename = "mainBPMColor")]
    pub main_bpm_color: String,
    #[serde(default, rename = "minBPMColor")]
    pub min_bpm_color: String,
    #[serde(default, rename = "maxBPMColor")]
    pub max_bpm_color: String,
    #[serde(default, rename = "otherBPMColor")]
    pub other_bpm_color: String,
    #[serde(default, rename = "stopLineColor")]
    pub stop_line_color: String,
    #[serde(default, rename = "transitionLineColor")]
    pub transition_line_color: String,
}

/// Skin value / slider 用の BMZ 組み込み式キー。
pub const SKIN_EXPR_ADJUSTED_COVER: &str = "bmz:adjusted_cover";
pub const SKIN_EXPR_ADJUSTED_RATE: &str = "bmz:adjusted_rate";
pub const SKIN_EXPR_ADJUSTED_RATE_ADOT: &str = "bmz:adjusted_rate_adot";
pub const SKIN_EXPR_FS_THRESHOLD: &str = "bmz:fs_threshold";
pub const SKIN_EXPR_COURSE_TABLE_TEXT: &str = "bmz:course_table_text";
pub const SKIN_EXPR_RESULT_TABLE_TITLE: &str = "bmz:result_table_title";
pub const SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT: &str = "bmz:fast_slow_breakdown_height";
pub const SKIN_EXPR_DEFAULT_CHART_TOTAL_COUNT: &str = "bmz:default_chart_total_count";
pub const SKIN_EXPR_DEFAULT_CHART_GAUGE: &str = "bmz:default_chart_gauge";
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
/// Lua result skin の定数 `Expand_op` 代入を宣言的クリックイベントへ変換する ID。
/// beatoraja の正数イベント ID と衝突しない BMZ 内部予約値を使う。
pub const SKIN_EVENT_RESULT_PANEL_IR: i32 = -10_001;
pub const SKIN_EVENT_RESULT_PANEL_GRAPH: i32 = -10_002;
/// Lua callback から変換する runtime event の内部予約 ID 範囲。
/// beatoraja 正数イベント ID と衝突しないよう負数を使う。
pub const SKIN_EVENT_RUNTIME_BASE: i32 = -20_000;
/// beatoraja `NUMBER_RANDOM_1P_1KEY..NUMBER_RANDOM_2P_SCR` (450..469).
pub const SKIN_RANDOM_LANE_REF_BASE: i32 = 450;
pub const SKIN_RANDOM_LANE_REF_COUNT: usize = 20;
/// `SkinDrawState::dynamic_timer_ms` のスロット数。
pub const SKIN_DYNAMIC_TIMER_COUNT: usize = 64;

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default = "default_graph_angle")]
    pub angle: i32,
    #[serde(default, rename = "type")]
    pub graph_type: i32,
    /// Lua `value = function()` から変換した fill 比率式 (0.0–1.0)。空なら `graph_type` を使う。
    #[serde(default)]
    pub value_expr: String,
    #[serde(default, rename = "isRefNum")]
    pub is_ref_num: bool,
    #[serde(default)]
    pub min: i32,
    #[serde(default)]
    pub max: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinHiddenCoverDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
    #[serde(default = "default_grid_division")]
    pub divx: i32,
    #[serde(default = "default_grid_division")]
    pub divy: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    #[serde(default)]
    pub cycle: i32,
    #[serde(default, rename = "disapearLine")]
    pub disappear_line: i32,
    #[serde(default = "default_true", rename = "isDisapearLineLinkLift")]
    pub is_disappear_line_link_lift: bool,
}

fn deserialize_lift_cover_defs<'de, D>(deserializer: D) -> Result<Vec<SkinHiddenCoverDef>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut values = Vec::<JsonValue>::deserialize(deserializer)?;
    for value in &mut values {
        if let Some(object) = value.as_object_mut() {
            object.entry("isDisapearLineLinkLift").or_insert(JsonValue::Bool(false));
        }
    }
    values
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(D::Error::custom))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinHitErrorVisualizerDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default = "default_hiterror_width")]
    pub width: i32,
    #[serde(default = "default_hiterror_judge_width_millis", rename = "judgeWidthMillis")]
    pub judge_width_millis: i32,
    #[serde(default = "default_hiterror_line_width", rename = "lineWidth")]
    pub line_width: i32,
    #[serde(default, rename = "colorMode")]
    pub color_mode: i32,
    #[serde(default = "default_true_int", rename = "hiterrorMode")]
    pub hiterror_mode: i32,
    #[serde(default = "default_true_int", rename = "emaMode")]
    pub ema_mode: i32,
    #[serde(default = "default_hiterror_line_color", rename = "lineColor")]
    pub line_color: String,
    #[serde(default = "default_hiterror_center_color", rename = "centerColor")]
    pub center_color: String,
    #[serde(default = "default_hiterror_judge_color", rename = "PGColor")]
    pub pg_color: String,
    #[serde(default = "default_hiterror_judge_color", rename = "GRColor")]
    pub gr_color: String,
    #[serde(default = "default_hiterror_judge_color", rename = "GDColor")]
    pub gd_color: String,
    #[serde(default = "default_hiterror_judge_color", rename = "BDColor")]
    pub bd_color: String,
    #[serde(default = "default_hiterror_judge_color", rename = "PRColor")]
    pub pr_color: String,
    #[serde(default = "default_hiterror_ema_color", rename = "emaColor")]
    pub ema_color: String,
    #[serde(default = "default_hiterror_alpha")]
    pub alpha: f32,
    #[serde(default = "default_hiterror_window_length", rename = "windowLength")]
    pub window_length: i32,
    #[serde(default)]
    pub transparent: i32,
    #[serde(default = "default_true_int", rename = "drawDecay")]
    pub draw_decay: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinTimingVisualizerDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default = "default_timing_width")]
    pub width: i32,
    #[serde(default = "default_timing_judge_width_millis", rename = "judgeWidthMillis")]
    pub judge_width_millis: i32,
    #[serde(default = "default_true_int", rename = "lineWidth")]
    pub line_width: i32,
    #[serde(default = "default_timing_line_color", rename = "lineColor")]
    pub line_color: String,
    #[serde(default = "default_timing_center_color", rename = "centerColor")]
    pub center_color: String,
    #[serde(default = "default_timing_pg_color", rename = "PGColor")]
    pub pg_color: String,
    #[serde(default = "default_timing_gr_color", rename = "GRColor")]
    pub gr_color: String,
    #[serde(default = "default_timing_gd_color", rename = "GDColor")]
    pub gd_color: String,
    #[serde(default = "default_timing_bd_color", rename = "BDColor")]
    pub bd_color: String,
    #[serde(default = "default_timing_pr_color", rename = "PRColor")]
    pub pr_color: String,
    #[serde(default)]
    pub transparent: i32,
    #[serde(default = "default_true_int", rename = "drawDecay")]
    pub draw_decay: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinTimingDistributionGraphDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default = "default_timing_width")]
    pub width: i32,
    #[serde(default = "default_true_int", rename = "lineWidth")]
    pub line_width: i32,
    #[serde(default = "default_timing_line_color", rename = "graphColor")]
    pub graph_color: String,
    #[serde(default = "default_timing_center_color", rename = "averageColor")]
    pub average_color: String,
    #[serde(default = "default_timing_center_color", rename = "devColor")]
    pub dev_color: String,
    #[serde(default = "default_timing_pg_color", rename = "PGColor")]
    pub pg_color: String,
    #[serde(default = "default_timing_gr_color", rename = "GRColor")]
    pub gr_color: String,
    #[serde(default = "default_timing_gd_color", rename = "GDColor")]
    pub gd_color: String,
    #[serde(default = "default_timing_bd_color", rename = "BDColor")]
    pub bd_color: String,
    #[serde(default = "default_timing_pr_color", rename = "PRColor")]
    pub pr_color: String,
    #[serde(default = "default_true_int", rename = "drawAverage")]
    pub draw_average: i32,
    #[serde(default = "default_true_int", rename = "drawDev")]
    pub draw_dev: i32,
}

fn default_hiterror_width() -> i32 {
    301
}
fn default_hiterror_judge_width_millis() -> i32 {
    150
}
fn default_hiterror_line_width() -> i32 {
    1
}
fn default_true_int() -> i32 {
    1
}
fn default_hiterror_line_color() -> String {
    "99CCFF80".to_string()
}
fn default_hiterror_center_color() -> String {
    "FFFFFFFF".to_string()
}
fn default_hiterror_judge_color() -> String {
    "99CCFF80".to_string()
}
fn default_hiterror_ema_color() -> String {
    "FF0000FF".to_string()
}
fn default_hiterror_alpha() -> f32 {
    0.1
}
fn default_hiterror_window_length() -> i32 {
    30
}

fn default_timing_width() -> i32 {
    301
}
fn default_timing_judge_width_millis() -> i32 {
    150
}
fn default_timing_line_color() -> String {
    "00FF00FF".to_string()
}
fn default_timing_center_color() -> String {
    "FFFFFFFF".to_string()
}
fn default_timing_pg_color() -> String {
    "000088FF".to_string()
}
fn default_timing_gr_color() -> String {
    "008800FF".to_string()
}
fn default_timing_gd_color() -> String {
    "888800FF".to_string()
}
fn default_timing_bd_color() -> String {
    "880000FF".to_string()
}
fn default_timing_pr_color() -> String {
    "000000FF".to_string()
}

fn default_gaugegraph_assist_clear_bg_color() -> String {
    "440044".to_string()
}
fn default_gaugegraph_assist_easy_fail_bg_color() -> String {
    "004444".to_string()
}
fn default_gaugegraph_groove_fail_bg_color() -> String {
    "004400".to_string()
}
fn default_gaugegraph_groove_clear_hard_bg_color() -> String {
    "440000".to_string()
}
fn default_gaugegraph_exhard_bg_color() -> String {
    "444400".to_string()
}
fn default_gaugegraph_hazard_bg_color() -> String {
    "444444".to_string()
}
fn default_gaugegraph_assist_clear_line_color() -> String {
    "ff00ff".to_string()
}
fn default_gaugegraph_assist_easy_fail_line_color() -> String {
    "00ffff".to_string()
}
fn default_gaugegraph_groove_fail_line_color() -> String {
    "00ff00".to_string()
}
fn default_gaugegraph_groove_clear_hard_line_color() -> String {
    "ff0000".to_string()
}
fn default_gaugegraph_exhard_line_color() -> String {
    "ffff00".to_string()
}
fn default_gaugegraph_hazard_line_color() -> String {
    "cccccc".to_string()
}
fn default_gaugegraph_borderline_color() -> String {
    "ff0000".to_string()
}
fn default_gaugegraph_border_color() -> String {
    "440000".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinNoteSetDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub note: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub lnstart: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub lnend: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub lnbody: Vec<String>,
    /// 新形式: 押下中の LN 胴体。定義時は lnbody=非押下 / lnbodyActive=押下中。
    #[serde(default, rename = "lnbodyActive", deserialize_with = "deserialize_skin_id_vec")]
    pub lnbody_active: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub lnactive: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnstart: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnend: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnbody: Vec<String>,
    /// 新形式: processing(正しく押下)中の HCN 胴体。
    #[serde(default, rename = "hcnbodyActive", deserialize_with = "deserialize_skin_id_vec")]
    pub hcnbody_active: Vec<String>,
    /// 新形式: passing 中で inclease(回復中)の HCN 胴体。
    #[serde(default, rename = "hcnbodyReactive", deserialize_with = "deserialize_skin_id_vec")]
    pub hcnbody_reactive: Vec<String>,
    /// 新形式: passing 中で離している(減衰中)の HCN 胴体。
    #[serde(default, rename = "hcnbodyMiss", deserialize_with = "deserialize_skin_id_vec")]
    pub hcnbody_miss: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnactive: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcndamage: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnreactive: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub mine: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hidden: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub processed: Vec<String>,
    #[serde(default)]
    pub size: Vec<i32>,
    #[serde(default)]
    pub dst: Vec<SkinDstEntry>,
    #[serde(default)]
    pub group: Vec<SkinDestinationDef>,
    #[serde(default)]
    pub bpm: Vec<SkinDestinationDef>,
    #[serde(default)]
    pub stop: Vec<SkinDestinationDef>,
    #[serde(default)]
    pub time: Vec<SkinDestinationDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinGaugeDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub nodes: Vec<String>,
    #[serde(default = "default_gauge_parts")]
    pub parts: i32,
    /// beatoraja `SkinGauge` のアニメ種別 (`ANIMATION_*`)。JSON で省略時は 0 (RANDOM)。
    #[serde(default = "default_skin_gauge_animation_type", rename = "type")]
    pub gauge_type: i32,
    #[serde(default = "default_gauge_range")]
    pub range: i32,
    #[serde(default = "default_gauge_cycle")]
    pub cycle: i32,
    #[serde(default)]
    pub starttime: i32,
    #[serde(default = "default_gauge_endtime")]
    pub endtime: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinJudgeDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub index: i32,
    #[serde(default)]
    pub images: Vec<SkinDestinationDef>,
    #[serde(default)]
    pub numbers: Vec<SkinDestinationDef>,
    #[serde(default)]
    pub shift: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinBgaDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinDestinationDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub blend: i32,
    #[serde(default)]
    pub filter: i32,
    #[serde(default)]
    pub timer: Option<i32>,
    /// BMZ限定のruntime timer式。PeacefulPlay key loggerの反復event timerに使う。
    #[serde(default)]
    pub timer_expr: String,
    /// `loop` フィールド。未指定(None)＝ループなし(1回再生して最終フレーム保持)。
    /// `Some(n>=0)`＝終端到達後 n 時刻へループバック。`Some(n<0)`＝終端後に非表示。
    #[serde(default, rename = "loop")]
    pub loop_time: Option<i32>,
    #[serde(default)]
    pub center: i32,
    #[serde(default)]
    pub offset: i32,
    #[serde(default)]
    pub offsets: Vec<i32>,
    #[serde(default = "default_stretch")]
    pub stretch: i32,
    #[serde(default, deserialize_with = "deserialize_op_codes")]
    pub op: Vec<i32>,
    #[serde(default)]
    pub draw: String,
    #[serde(default)]
    pub dst: Vec<SkinDstEntry>,
    #[serde(rename = "mouseRect")]
    pub mouse_rect: Option<SkinRectDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SkinRectDef {
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SkinAnimationDef {
    pub time: Option<i32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: Option<i32>,
    pub h: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_skin_frame_expr_opt")]
    pub h_expr: Option<SkinFrameExpr>,
    pub acc: Option<i32>,
    pub a: Option<i32>,
    pub r: Option<i32>,
    pub g: Option<i32>,
    pub b: Option<i32>,
    pub angle: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinFrameExpr {
    FastSlowBreakdownHeight(i32),
}

/// A single entry in a destination `dst` array — either a plain animation frame or a
/// conditional frame that is only included when all listed option IDs are enabled.
#[derive(Debug, Clone, PartialEq)]
pub enum SkinDstEntry {
    Frame(SkinAnimationDef),
    /// `{"if": [...], "value": {...}}` or `{"if": [...], "values": [...]}`
    Conditional {
        if_ops: Vec<i32>,
        frames: Vec<SkinAnimationDef>,
    },
}

impl<'de> Deserialize<'de> for SkinDstEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = JsonValue::deserialize(deserializer)?;
        if value.get("if").is_some() {
            let if_ops = parse_skin_dst_if_ops(value.get("if").unwrap());
            let frames = if let Some(values_field) = value.get("values") {
                serde_json::from_value::<Vec<SkinAnimationDef>>(values_field.clone())
                    .unwrap_or_default()
            } else if let Some(value_field) = value.get("value") {
                serde_json::from_value::<SkinAnimationDef>(value_field.clone())
                    .ok()
                    .into_iter()
                    .collect()
            } else {
                vec![]
            };
            Ok(SkinDstEntry::Conditional { if_ops, frames })
        } else {
            serde_json::from_value(value).map(SkinDstEntry::Frame).map_err(serde::de::Error::custom)
        }
    }
}

/// `destination` 配列の1エントリ。通常の `SkinDestinationDef` か、
/// `{"if": [...], "values": [...]}` 形式の条件付きグループ。
#[derive(Debug, Clone, PartialEq)]
pub enum DestinationListEntry {
    Single(SkinDestinationDef),
    /// `{"if": [...], "values": [...]}` 形式。条件が満たされた場合のみ内部エントリを展開する。
    Conditional {
        if_ops: Vec<i32>,
        destinations: Vec<SkinDestinationDef>,
    },
}

impl<'de> Deserialize<'de> for DestinationListEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = JsonValue::deserialize(deserializer)?;
        if value.get("if").is_some() {
            let if_ops = parse_skin_dst_if_ops(value.get("if").unwrap());
            let destinations = if let Some(values_field) = value.get("values") {
                serde_json::from_value::<Vec<SkinDestinationDef>>(values_field.clone())
                    .unwrap_or_default()
            } else {
                vec![]
            };
            Ok(DestinationListEntry::Conditional { if_ops, destinations })
        } else {
            serde_json::from_value(value)
                .map(DestinationListEntry::Single)
                .map_err(serde::de::Error::custom)
        }
    }
}

fn deserialize_destination_entries<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<DestinationListEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = JsonValue::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(Vec::new());
    }
    if value.is_array() {
        serde_json::from_value(value).map_err(serde::de::Error::custom)
    } else {
        serde_json::from_value(value).map(|entry| vec![entry]).map_err(serde::de::Error::custom)
    }
}

/// Parses the `if` field of a conditional dst entry into a flat list of required option IDs.
/// Each ID is positive (must be enabled) or negative (must be disabled).
/// Nested arrays (OR groups) are flattened to their first element for simplicity.
pub fn parse_skin_dst_if_ops(value: &JsonValue) -> Vec<i32> {
    match value {
        JsonValue::Number(n) => n.as_i64().map(|n| vec![n as i32]).unwrap_or_default(),
        JsonValue::Array(arr) => arr
            .iter()
            .flat_map(|v| match v {
                JsonValue::Number(n) => n.as_i64().map(|n| vec![n as i32]).unwrap_or_default(),
                JsonValue::Array(inner) => inner
                    .iter()
                    .find_map(|v2| v2.as_i64())
                    .map(|n| vec![n as i32])
                    .unwrap_or_default(),
                _ => vec![],
            })
            .collect(),
        _ => vec![],
    }
}

pub fn test_skin_dst_if(if_ops: &[i32], enabled_options: &[i32]) -> bool {
    if_ops.iter().all(|&op| test_json_option_number(op, enabled_options))
}

pub fn default_skin_canvas_width() -> u32 {
    1280
}

pub fn default_skin_canvas_height() -> u32 {
    720
}

pub fn default_judgetimer() -> i32 {
    1
}

pub fn default_grid_division() -> i32 {
    1
}

pub fn default_true() -> bool {
    true
}

pub fn default_graph_angle() -> i32 {
    1
}

pub fn default_skin_gauge_animation_type() -> i32 {
    0
}

pub fn default_gauge_parts() -> i32 {
    50
}

pub fn default_gauge_range() -> i32 {
    3
}

pub fn default_gauge_cycle() -> i32 {
    33
}

pub fn default_gauge_endtime() -> i32 {
    500
}

pub fn default_stretch() -> i32 {
    -1
}

pub fn deserialize_skin_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(SkinIdVisitor)
}

pub fn deserialize_skin_string<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(SkinIdVisitor)
}

pub fn deserialize_skin_frame_expr_opt<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<SkinFrameExpr>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(expr) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    parse_skin_frame_expr(&expr).map(Some).map_err(D::Error::custom)
}

pub fn parse_skin_frame_expr(expr: &str) -> std::result::Result<SkinFrameExpr, String> {
    let expr = expr.trim();
    let prefix = format!("{SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT}(");
    let Some(arg) = expr.strip_prefix(&prefix).and_then(|rest| rest.strip_suffix(')')) else {
        return Err(format!("unsupported skin frame expression `{expr}`"));
    };
    let ref_id = arg
        .trim()
        .parse::<i32>()
        .map_err(|_| format!("invalid fast/slow breakdown ref `{arg}`"))?;
    Ok(SkinFrameExpr::FastSlowBreakdownHeight(ref_id))
}

/// `op` フィールドは beatoraja Lua スキンで単一整数または整数配列のどちらでも
/// 書ける。`Vec<i32>` への直接デシリアライズは整数を拒否してしまうため、
/// スカラーは長さ 1 の配列として受け入れる。
pub fn deserialize_op_codes<'de, D>(deserializer: D) -> std::result::Result<Vec<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Many(Vec<i32>),
        One(i32),
    }
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::Many(values) => values,
        OneOrMany::One(value) => vec![value],
    })
}

pub fn deserialize_skin_id_vec<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct SkinIdVecVisitor;

    impl<'de> Visitor<'de> for SkinIdVecVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a list of skin ids")
        }

        fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut ids = Vec::new();
            while let Some(id) = seq.next_element_seed(SkinIdSeed)? {
                ids.push(id);
            }
            Ok(ids)
        }
    }

    deserializer.deserialize_seq(SkinIdVecVisitor)
}

struct SkinIdSeed;

impl<'de> serde::de::DeserializeSeed<'de> for SkinIdSeed {
    type Value = String;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_skin_id(deserializer)
    }
}

struct SkinIdVisitor;

impl<'de> Visitor<'de> for SkinIdVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string or numeric skin id")
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value.to_string())
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value)
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value.to_string())
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
    where
        E: DeError,
    {
        Ok(value.to_string())
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: DeError,
    {
        if value.fract() == 0.0 {
            Ok(format!("{value:.0}"))
        } else {
            Err(E::custom("skin id numbers must be integers"))
        }
    }
}

impl SkinDocument {
    pub fn load_beatoraja_json(path: &Path) -> Result<Self> {
        let raw = load_json_value(path)?;
        let options = default_enabled_options(&raw);
        Self::load_beatoraja_json_with_options(path, &options)
    }

    pub fn load_beatoraja_json_with_options(path: &Path, enabled_options: &[i32]) -> Result<Self> {
        let raw = load_json_value(path)?;
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let expanded = expand_json_skin_value(raw, root, root, enabled_options)
            .with_context(|| format!("failed to expand skin json: {}", path.display()))?;
        let expanded = normalize_json_skin_integer_numbers(expanded);
        serde_json::from_value(expanded)
            .with_context(|| format!("failed to parse skin json: {}", path.display()))
    }

    pub fn source_map(&self) -> HashMap<&str, &SkinSourceDef> {
        self.source.iter().map(|source| (source.id.as_str(), source)).collect()
    }

    pub fn image_map(&self) -> HashMap<&str, &SkinImageDef> {
        self.image.iter().map(|image| (image.id.as_str(), image)).collect()
    }

    /// beatoraja `PlaySkin.judgeregion` 相当 (`max(judge.index) + 1`、最低 1)。
    pub fn judge_region_count(&self) -> usize {
        let max_index = self.judge.iter().map(|judge| judge.index).max().unwrap_or(-1);
        (max_index.max(0) as usize + 1).max(1)
    }

    pub fn enabled_options(&self) -> Vec<i32> {
        let options = if let Some(ops) = &self.user_selected_options {
            ops.clone()
        } else {
            self.property
                .iter()
                .filter_map(|property| {
                    let selected = if property.def.is_empty() {
                        property.item.first()
                    } else {
                        property.item.iter().find(|item| item.name == property.def)
                    };
                    selected.map(|item| item.op)
                })
                .collect()
        };
        self.with_internal_enabled_options(options)
    }

    pub fn with_internal_enabled_options(&self, mut enabled_options: Vec<i32>) -> Vec<i32> {
        for &op in &self.internal_enabled_options {
            if !enabled_options.contains(&op) {
                enabled_options.push(op);
            }
        }
        enabled_options
    }

    /// 有効なオプション条件に基づいて `destination` エントリを展開し、
    /// 描画対象の `SkinDestinationDef` の参照リストを返す。
    /// Returns the first dst frame of any text element whose `ref_id` equals
    /// `ref_id`, normalized into the `0.0..=1.0` rendered viewport coordinate
    /// space (top-left origin). Used by bmz-player to position the IME candidate
    /// window over the search input region without touching the skin.
    ///
    /// Beatoraja skin sources use top-down y growing from the canvas top, but
    /// `normalize_skin_frame_rect` flips that to a bottom-up rect before paint,
    /// so directly using skin y here would land the IME cursor mirrored across
    /// the canvas. Apply the same flip so the returned rect matches the on-
    /// screen rendered position.
    pub fn text_destination_rect_for_ref(&self, ref_id: i32) -> Option<(f32, f32, f32, f32)> {
        let text_id = self.text.iter().find(|t| t.ref_id == ref_id)?.id.as_str();
        let canvas_w = self.w.max(1) as f32;
        let canvas_h = self.h.max(1) as f32;
        // top-level destinations only — the search word region sits there
        // in beatoraja m-select skins.
        for entry in &self.destination {
            let candidates: Vec<&SkinDestinationDef> = match entry {
                DestinationListEntry::Single(d) => vec![d],
                DestinationListEntry::Conditional { destinations, .. } => {
                    destinations.iter().collect()
                }
            };
            for dest in candidates {
                if dest.id != text_id {
                    continue;
                }
                for dst in &dest.dst {
                    let frame_opt = match dst {
                        SkinDstEntry::Frame(f) => Some(f),
                        SkinDstEntry::Conditional { frames, .. } => frames.first(),
                    };
                    if let Some(frame) = frame_opt {
                        let raw_x = frame.x.unwrap_or(0) as f32;
                        let raw_y = frame.y.unwrap_or(0) as f32;
                        let raw_w = frame.w.unwrap_or(0).max(0) as f32;
                        let raw_h = frame.h.unwrap_or(0).max(0) as f32;
                        if raw_w <= 0.0 || raw_h <= 0.0 {
                            continue;
                        }
                        // Match `normalize_skin_frame_rect`: bottom-up render
                        // origin → top-left coordinate the IME backend wants.
                        let x = raw_x / canvas_w;
                        let y = (canvas_h - (raw_y + raw_h)) / canvas_h;
                        let w = raw_w / canvas_w;
                        let h = raw_h / canvas_h;
                        return Some((x, y, w, h));
                    }
                }
            }
        }
        None
    }

    pub fn all_destinations<'a>(&'a self, enabled_options: &[i32]) -> Vec<&'a SkinDestinationDef> {
        let mut result = Vec::new();
        for entry in &self.destination {
            match entry {
                DestinationListEntry::Single(d) => result.push(d),
                DestinationListEntry::Conditional { if_ops, destinations } => {
                    if test_skin_dst_if(if_ops, enabled_options) {
                        result.extend(destinations.iter());
                    }
                }
            }
        }
        result
    }
}
