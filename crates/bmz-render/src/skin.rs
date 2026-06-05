use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::assets::load_png_rgba;
use crate::plan::{
    Color, DrawCommand, Point, Rect, TextAlign, TextLayer, TextOutline, TextOverflow, TextShadow,
    TextStyle, TextureId, UvRect,
};
use crate::scene::{CourseConstraintFlags, SelectRowKind, SelectRowSnapshot, SelectSnapshot};
use crate::skin_offset::{SKIN_OFFSET_BAR_LINE, SkinOffsetValues};
use crate::snapshot::{CourseStageMarker, DisplayJudgeCounts};

const OFFSET_ALL: i32 = 10;
const OFFSET_NOTES_1P: i32 = 30;
/// beatoraja の `SkinProperty.OFFSET_JUDGE_1P`。判定文字とコンボ数の destination が
/// `offsets: [32]` で参照する。コード本体では明示注入せず destination の `offsets`
/// 経由で適用する (テスト・ドキュメント用に定数だけ保持)。
#[allow(dead_code)]
const OFFSET_JUDGE_1P: i32 = 32;
const OFFSET_JUDGEDETAIL_1P: i32 = 33;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinObjectId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkinTextureId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct SkinObject {
    pub id: SkinObjectId,
    pub source: SkinSource,
    pub placements: Vec<SkinPlacement>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinDefinition {
    pub objects: Vec<SkinObject>,
}

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
    /// ユーザがスキン設定パネルで選んだオプションから算出した有効 op コード列。
    /// `Some` のときレンダー時の `enabled_options()` はこれを返し、`None` の
    /// ときは従来通り `property.def` (または各 property の先頭 item) を既定として
    /// 計算する。
    #[serde(skip)]
    pub user_selected_options: Option<Vec<i32>>,
    /// プレイ描画時のみ plan 側が設定する judgegraph 密度。
    #[serde(skip, default)]
    pub play_judge_graph_density: Vec<u8>,
    /// プレイ描画時のみ plan 側が設定する bpmgraph 線分。
    #[serde(skip, default)]
    pub play_bpm_graph_segments: Vec<crate::chart_graph::BpmGraphSegment>,
    /// リザルト描画時のみ plan 側が設定する gaugegraph 推移。
    #[serde(skip, default)]
    pub result_gauge_graph_points: Vec<crate::snapshot::ResultGaugeGraphPoint>,
    /// リザルト描画時のみ plan 側が設定する timing graph 推移。
    #[serde(skip, default)]
    pub result_timing_points: Vec<crate::snapshot::ResultTimingPoint>,
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
pub const SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT: &str = "bmz:fast_slow_breakdown_height";

/// beatoraja 予約 ID と衝突しない動的タイマー ID 範囲の先頭。
pub const SKIN_DYNAMIC_TIMER_BASE: i32 = 9000;
/// Play 中 imageset が `main_state.gauge_type()` で選ぶ ref (beatoraja 非予約)。
pub const SKIN_REF_PLAY_GAUGE_TYPE: i32 = 44;
/// beatoraja `BUTTON_HSFIX` (`event_index(55)`)。
pub const SKIN_EVENT_HSFIX: i32 = 55;
/// `SkinDrawState::dynamic_timer_ms` のスロット数。
pub const SKIN_DYNAMIC_TIMER_COUNT: usize = 64;

fn string_array_refs(values: &[String; 10]) -> [&str; 10] {
    std::array::from_fn(|index| values[index].as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinDynamicTimerDef {
    pub id: i32,
    pub observe: String,
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
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub lnactive: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnstart: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnend: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hcnbody: Vec<String>,
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

/// beatoraja `PlaySkin.judgeregion` 上限 (TIMER_JUDGE_1P/2P/3P = 46/47/247)。
pub const MAX_JUDGE_REGIONS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeRegionState {
    pub judge_ms: [Option<i32>; MAX_JUDGE_REGIONS],
    pub judge_index: [Option<usize>; MAX_JUDGE_REGIONS],
    pub judge_combo: [u32; MAX_JUDGE_REGIONS],
    pub judge_timing_sign: [Option<i8>; MAX_JUDGE_REGIONS],
}

/// レーン index から判定領域 index へ (beatoraja `JudgeManager.updateMicro` 同式)。
pub fn lane_judge_region(lane_index: usize, lane_count: usize, region_count: usize) -> usize {
    if lane_count == 0 || region_count == 0 {
        return 0;
    }
    let region = lane_index * region_count / lane_count;
    region.min(region_count.saturating_sub(1))
}

/// `recent_judgements` から領域別の判定 timer / 画像 index を構築する。
pub fn build_judge_region_state(
    recent_judgements: &[crate::snapshot::DisplayJudgement],
    render_now_us: i64,
    region_count: usize,
) -> JudgeRegionState {
    let mut judge_ms = [None; MAX_JUDGE_REGIONS];
    let mut judge_index = [None; MAX_JUDGE_REGIONS];
    let mut judge_combo = [0; MAX_JUDGE_REGIONS];
    let mut judge_timing_sign = [None; MAX_JUDGE_REGIONS];
    let region_count = region_count.min(MAX_JUDGE_REGIONS);
    for judgement in recent_judgements.iter().rev() {
        let region = lane_judge_region(judgement.lane.index(), LANE_COUNT, region_count);
        if judge_ms[region].is_some() {
            continue;
        }
        judge_ms[region] = Some(
            ((render_now_us - judgement.time.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
        );
        judge_index[region] = Some(judge_image_index_for_judge(judgement.judge));
        judge_combo[region] = judgement.combo;
        judge_timing_sign[region] = Some(match judgement.side {
            TimingSide::Fast => 1,
            TimingSide::Slow => -1,
        });
    }
    JudgeRegionState { judge_ms, judge_index, judge_combo, judge_timing_sign }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinClickTarget {
    Event { event_id: i32, click: i32 },
    SelectRow { row_index: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinClickHit {
    pub target: SkinClickTarget,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinSliderHit {
    pub slider_type: i32,
    pub value: f32,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SkinContext {
    manifest: SkinManifest,
    document: Option<SkinDocument>,
    document_sources: HashMap<String, SkinDocumentTexture>,
    select_settings_dest_index: Arc<crate::select_settings_dest::SelectSettingsDestIndex>,
}

impl Default for SkinContext {
    fn default() -> Self {
        Self {
            manifest: default_skin_manifest(),
            document: None,
            document_sources: HashMap::new(),
            select_settings_dest_index: Arc::new(
                crate::select_settings_dest::SelectSettingsDestIndex::default(),
            ),
        }
    }
}

impl SkinContext {
    pub fn from_manifest(manifest: SkinManifest) -> Self {
        Self {
            manifest,
            document: None,
            document_sources: HashMap::new(),
            select_settings_dest_index: Arc::new(
                crate::select_settings_dest::SelectSettingsDestIndex::default(),
            ),
        }
    }

    pub fn from_manifest_and_document(
        manifest: SkinManifest,
        document: SkinDocument,
        document_sources: impl IntoIterator<Item = SkinDocumentTexture>,
    ) -> Self {
        let select_settings_dest_index =
            Arc::new(crate::select_settings_dest::build_select_settings_dest_index(&document));
        Self {
            manifest,
            document: Some(document),
            document_sources: document_sources
                .into_iter()
                .map(|source| (source.source_id.clone(), source))
                .collect(),
            select_settings_dest_index,
        }
    }

    pub fn manifest(&self) -> &SkinManifest {
        &self.manifest
    }

    pub fn document(&self) -> Option<&SkinDocument> {
        self.document.as_ref()
    }

    pub fn with_play_graphs(
        &self,
        judge_graph_density: Vec<u8>,
        bpm_graph_segments: Vec<crate::chart_graph::BpmGraphSegment>,
    ) -> Self {
        let mut cloned = self.clone();
        if let Some(document) = &mut cloned.document {
            document.play_judge_graph_density = judge_graph_density;
            document.play_bpm_graph_segments = bpm_graph_segments;
        }
        cloned
    }

    pub fn with_result_graphs(&self, graph: &crate::snapshot::ResultGraphSnapshot) -> Self {
        let mut cloned = self.clone();
        if let Some(document) = &mut cloned.document {
            document.play_judge_graph_density = graph.judge_graph_density.clone();
            document.play_bpm_graph_segments = graph.bpm_graph_segments.clone();
            document.result_gauge_graph_points = graph.gauge_points.clone();
            document.result_timing_points = graph.timing_points.clone();
        }
        cloned
    }

    pub fn static_document_items(&self) -> Vec<SkinRenderItem> {
        self.static_document_items_for_state(SkinDrawState::default())
    }

    pub fn static_document_items_for_state(&self, state: SkinDrawState) -> Vec<SkinRenderItem> {
        self.static_document_items_for_state_and_text(state, SkinTextState::default())
    }

    pub fn static_document_items_for_state_and_text(
        &self,
        state: SkinDrawState,
        text: SkinTextState<'_>,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        document.static_render_items(&self.document_sources, state, text)
    }

    pub fn select_document_items(&self, snapshot: &SelectSnapshot) -> Vec<SkinRenderItem> {
        self.select_document_items_with_dynamic_timers(snapshot, None)
    }

    pub fn select_document_items_with_dynamic_timers(
        &self,
        snapshot: &SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = &self.document else {
            return Vec::new();
        };
        document.select_render_items_with_dynamic_timers(
            &self.document_sources,
            snapshot,
            dynamic_timers,
            &self.select_settings_dest_index,
        )
    }

    pub fn select_click_hit(
        &self,
        snapshot: &SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        let document = self.document.as_ref()?;
        document.select_click_hit(
            &self.document_sources,
            snapshot,
            &self.select_settings_dest_index,
            x,
            y,
        )
    }

    pub fn select_slider_hit(
        &self,
        snapshot: &SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let document = self.document.as_ref()?;
        document.select_slider_hit(snapshot, &self.select_settings_dest_index, x, y)
    }

    /// 静的 destination を `{"id":"notes"}` マーカーと `timer: 3` (FAILED) で分割して返す。
    /// `.0` はノーツ背面、`.1` はノーツ前面、`.2` は閉店/暗転オーバーレイ（最前面）。
    pub fn static_document_items_split_for_state_and_text(
        &self,
        state: SkinDrawState,
        text: SkinTextState<'_>,
    ) -> (Vec<SkinRenderItem>, Vec<SkinRenderItem>, Vec<SkinRenderItem>) {
        let Some(document) = &self.document else {
            return (Vec::new(), Vec::new(), Vec::new());
        };
        document.static_render_items_split(&self.document_sources, state, text)
    }

    pub fn document_note_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_image_render_item(lane, key_mode, rect, &self.document_sources)
    }

    /// ロングノート胴体（`note.lnbody`）を指定矩形に伸縮描画する。
    pub fn document_long_body_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_long_body_render_item(lane, key_mode, rect, &self.document_sources)
    }

    /// Mine ノート（`note.mine`）を指定矩形に描画する。スキン側に定義が無ければ
    /// `None` を返すため、呼び出し側はデフォルトテクスチャ等のフォールバックへ
    /// 落ちる。
    pub fn document_mine_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_mine_render_item(lane, key_mode, rect, &self.document_sources)
    }

    pub fn document_note_height(&self, lane: Lane, key_mode: KeyMode) -> Option<f32> {
        let document = self.document.as_ref()?;
        document.note_height_for_lane(lane, key_mode)
    }

    pub fn document_bar_line_items(
        &self,
        note_y: f32,
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        document.note_group_render_items(note_y, state, &self.document_sources)
    }

    pub fn document_gauge_items(&self, gauge: f32, elapsed_ms: i32) -> Option<Vec<SkinRenderItem>> {
        let document = self.document.as_ref()?;
        document.gauge_render_items(gauge, elapsed_ms, &self.document_sources)
    }

    pub fn timer_animation_duration_ms(&self, timer: i32) -> i32 {
        self.document.as_ref().map_or(0, |document| {
            let enabled_options = document.enabled_options();
            document
                .all_destinations(&enabled_options)
                .into_iter()
                .filter(|destination| destination.timer == Some(timer))
                .filter_map(|destination| {
                    flatten_dst_entries(&destination.dst, &enabled_options)
                        .into_iter()
                        .map(|frame| frame.time.unwrap_or(0))
                        .max()
                })
                .max()
                .unwrap_or(0)
                .max(0)
        })
    }

    pub fn document_judge_items(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
        skin_offsets: SkinOffsetValues,
        region: usize,
    ) -> Option<Vec<SkinRenderItem>> {
        let document = self.document.as_ref()?;
        let judge_image_index = judge_image_index(judge)?;
        let judge_def = document
            .judge
            .iter()
            .find(|j| j.index == region as i32)
            .or_else(|| document.judge.first())?;
        document.judge_render_items_for_def(
            judge_def,
            judge_image_index,
            combo,
            elapsed_ms,
            &self.document_sources,
            SkinDrawState { skin_offsets, ..SkinDrawState::default() },
        )
    }

    pub fn apply_play_skin_global_offset(
        &self,
        items: Vec<SkinRenderItem>,
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        if self.document.is_none() {
            return items;
        }
        items.into_iter().map(|item| apply_all_offset_to_render_item(item, state)).collect()
    }

    pub fn apply_play_skin_global_offset_to_item(
        &self,
        item: SkinRenderItem,
        state: SkinDrawState,
    ) -> SkinRenderItem {
        if self.document.is_none() {
            return item;
        }
        apply_all_offset_to_render_item(item, state)
    }

    /// beatoraja スキンの `note.dst` からレーンのノートエリアを取得し、
    /// `note_y`（0.0=判定ライン, 1.0=最上部）に対応するノート矩形を返す。
    /// `note_height` は正規化座標での高さ。ドキュメントスキンが無い場合は `None`。
    pub fn note_rect_for_progress(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        note_y: f32,
        note_height: f32,
        state: SkinDrawState,
    ) -> Option<Rect> {
        let document = self.document.as_ref()?;
        let enabled_options = document.enabled_options();
        let area = document.note_lane_area(lane, key_mode, &enabled_options)?;
        let canvas_h = document.h.max(1) as f32;
        let bottom_y = note_progress_to_y(area, note_y, state, canvas_h);
        let rect =
            Rect { x: area.x, y: bottom_y - note_height, width: area.width, height: note_height };
        Some(document.apply_notes_offset_to_rect(rect, state))
    }

    /// ロングノート胴体の矩形を計算する。`head_y`/`tail_y` は `VisibleNote::y` と同じ
    /// 正規化座標（0.0=判定ライン, 1.0=最奥）。胴体は両端の中心を結ぶ。
    pub fn note_body_rect(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        head_y: f32,
        tail_y: f32,
        state: SkinDrawState,
    ) -> Option<Rect> {
        let document = self.document.as_ref()?;
        let enabled_options = document.enabled_options();
        let area = document.note_lane_area(lane, key_mode, &enabled_options)?;
        let canvas_h = document.h.max(1) as f32;
        let head_center = note_progress_to_y(area, head_y, state, canvas_h);
        let tail_center = note_progress_to_y(area, tail_y, state, canvas_h);
        let top = head_center.min(tail_center);
        let bottom = head_center.max(tail_center);
        Some(document.apply_notes_offset_to_rect(
            Rect { x: area.x, y: top, width: area.width, height: bottom - top },
            state,
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinDocumentTexture {
    pub source_id: String,
    pub texture: SkinTextureId,
    pub source_size: SkinImageSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinBgaFrame {
    pub texture: SkinTextureId,
    pub source_size: SkinImageSize,
    pub tint_r: f32,
    pub tint_g: f32,
    pub tint_b: f32,
    pub tint_a: f32,
    /// 動画 BGA フレームかどうか。Layer/Layer2 でも動画ならクロマキーを適用しない
    /// (beatoraja の `ffmpeg.frag` 相当)。
    pub is_video: bool,
}

impl SkinBgaFrame {
    pub fn opaque(texture: SkinTextureId, source_size: SkinImageSize) -> Self {
        Self {
            texture,
            source_size,
            tint_r: 1.0,
            tint_g: 1.0,
            tint_b: 1.0,
            tint_a: 1.0,
            is_video: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinDrawState {
    pub elapsed_ms: i32,
    pub ready_timer_ms: Option<i32>,
    pub play_timer_ms: Option<i32>,
    pub key_mode: KeyMode,
    pub select_bar_elapsed_ms: i32,
    pub select_option_panel_elapsed_ms: i32,
    pub select_option_panel: u8,
    pub select_arrange_index: usize,
    pub select_gauge_index: usize,
    pub select_gauge_auto_shift_index: usize,
    pub select_target_index: usize,
    pub select_bga_index: usize,
    pub select_assist_index: usize,
    pub select_mode_index: usize,
    pub select_sort_index: usize,
    pub select_ln_mode_index: usize,
    pub mouse_x: Option<f32>,
    pub mouse_y: Option<f32>,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub gauge: f32,
    pub gauge_type: i32,
    pub gauge_auto_shift: bool,
    pub gauge_max: f32,
    pub gauge_border: f32,
    pub play_progress: f32,
    pub end_of_note: bool,
    pub end_of_note_ms: Option<i32>,
    /// 各レーンのボムタイマー経過ms。Noneなら非アクティブ。
    pub bomb_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのkeyon(押下中ビーム)タイマー経過ms。Noneなら非アクティブ。
    pub keyon_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのkeyoff(離した直後の演出)タイマー経過ms。Noneなら非アクティブ。
    /// beatoraja の TIMER_KEYOFF_1P_KEY1..7 (121..127) / SCRATCH (120) に対応。
    pub keyoff_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの直近判定の画像インデックス (0=PGREAT,1=GREAT,2=GOOD,3=BAD,4=POOR,5=MISS)。
    /// imageset (ボム・キービーム) の画像選択に使う。Noneなら判定なし。
    pub lane_judge: [Option<usize>; LANE_COUNT],
    /// 判定タイマー経過ms。index 0/1/2 = TIMER_JUDGE_1P/2P/3P (46/47/247)。Noneなら非アクティブ。
    pub judge_ms: [Option<i32>; MAX_JUDGE_REGIONS],
    /// Full combo timer elapsed ms (TIMER_FULLCOMBO_1P/2P=48/49)。Noneなら非アクティブ。
    pub full_combo_ms: Option<i32>,
    pub music_end_ms: Option<i32>,
    /// 領域別の判定画像インデックス (0=PGREAT,1=GREAT,2=GOOD,3=BAD,4=POOR,5=MISS)。
    pub judge_index: [Option<usize>; MAX_JUDGE_REGIONS],
    /// 領域別の判定表示用 combo。beatoraja `JudgeManager.judgecombo` 相当。
    pub judge_combo: [u32; MAX_JUDGE_REGIONS],
    /// 領域別の判定タイミング符号。1=EARLY/FAST, -1=LATE/SLOW。
    pub judge_timing_sign: [Option<i8>; MAX_JUDGE_REGIONS],
    /// OFFSET_LIFT (id=3) の y 値 (skin canvas pixel 単位)。リフト量に応じて要素をシフトする。
    pub offset_lift_px: i32,
    /// OFFSET_LANECOVER (id=4) の y 値 (skin canvas pixel 単位)。レーンカバー位置インジケータのシフト。
    pub offset_lanecover_px: i32,
    /// OFFSET_HIDDEN_COVER (id=5) の y 値 (skin canvas pixel 単位)。HIDDEN カバー位置のシフト。
    pub offset_hidden_cover_px: i32,
    /// ユーザーまたは profile で指定された任意の skin offset 値。
    pub skin_offsets: SkinOffsetValues,
    /// 現在のハイスピード倍率 (NUMBER_HISPEED=310, NUMBER_HISPEED_AFTERDOT=311 に使用)。
    pub hispeed: f32,
    /// 曲残り時間 ms (NUMBER_TIMELEFT_MINUTE=163, NUMBER_TIMELEFT_SECOND=164 に使用)。
    pub timeleft_ms: i32,
    /// ノーツ表示時間 ms (NUMBER_DURATION=312 / NUMBER_DURATION_GREEN=313 に使用)。
    pub total_duration_ms: i32,
    /// レーンカバー割合 0.0-1.0 (NUMBER_LANECOVER1=14 は 0..=1000 で返す)。
    pub lane_cover: f32,
    /// リフト量 0.0-1.0 (NUMBER_LIFT1=314 に使用)。
    pub lift: f32,
    /// HIDDEN カバー割合 0.0-1.0。未対応の間は 0.0 で hiddenCover を描画しない。
    pub hidden_cover: f32,
    /// OPTION_LANECOVER1_CHANGING (270)。Start/Select 押下中に true。
    pub lane_cover_changing: bool,
    /// OPTION_LANECOVER1_ON (271)。
    pub lanecover_enabled: bool,
    /// OPTION_LIFT1_ON (272)。
    pub lift_enabled: bool,
    /// OPTION_HIDDEN1_ON (273)。
    pub hidden_enabled: bool,
    /// 現在 BPM (NUMBER_NOWBPM=160 に使用)。
    pub now_bpm: f32,
    /// 最小 BPM (NUMBER_MINBPM=91 に使用)。
    pub min_bpm: f32,
    /// 最大 BPM (NUMBER_MAXBPM=90 に使用)。
    pub max_bpm: f32,
    /// 現在の曲にBGAイベントが含まれるかどうか (OPTION_NO_BGA=170 / OPTION_BGA=171)。
    pub has_bga: bool,
    /// BGA表示設定がONかどうか。曲の有無とは分けて扱う。
    pub bga_enabled: bool,
    /// `#STAGEFILE` 相当の曲画像があるか (OPTION_NO_STAGEFILE=190 / OPTION_STAGEFILE=191)。
    pub has_stagefile: bool,
    /// `#BACKBMP` 相当の背景画像がロード済みか (OPTION_NO_BACKBMP=194 / OPTION_BACKBMP=195)。
    pub has_backbmp: bool,
    /// 現在表示するBGA本体画像。
    pub bga_base: Option<SkinBgaFrame>,
    /// 現在表示するBGAレイヤー画像。
    pub bga_layer: Option<SkinBgaFrame>,
    /// 現在表示するBGAレイヤー2画像 (ch 0A)。
    pub bga_layer2: Option<SkinBgaFrame>,
    /// 直近のBAD/POORで一時表示するミスレイヤー画像。
    pub bga_poor: Option<SkinBgaFrame>,
    /// BGA destination に stretch 指定が無い場合に使う拡大設定。
    pub bga_stretch: i32,
    /// 最後の判定のタイミングずれ ms (VALUE_JUDGE_1P_DURATION=525 に使用)。Noneなら非表示。
    pub judge_timing_ms: Option<i32>,
    /// 過去ベストスコアのexスコア (NUMBER_HIGHSCORE=150, BARGRAPH_BESTSCORERATE=113 に使用)。
    pub best_ex_score: Option<u32>,
    /// ghost から現在進行度まで積算した過去ベスト EX。None の場合は final score の線形投影を使う。
    pub projected_best_ex_score: Option<u32>,
    /// 過去ベストのクリアタイプ index (ref 371)。
    pub best_clear_index: Option<i64>,
    /// ターゲットスコアのexスコア (NUMBER_TARGET_SCORE=121, BARGRAPH_TARGETSCORERATE=115 に使用)。
    pub target_ex_score: Option<u32>,
    /// 判定タイミングオフセット設定値 ms (NUMBER_JUDGETIMING=12 に使用、beatoraja の judgetiming 設定)。
    pub judge_timing_offset_ms: i32,
    /// 選曲画面の表示曲数 (NUMBER_SELECT_BAR_COUNT=300 相当)。
    pub select_chart_count: u32,
    /// 現在の描画状態が選曲画面かどうか。番号 ref の一部は scene ごとに意味が違う。
    pub select_screen: bool,
    /// 選曲バーのスクロール位置 0.0-1.0。
    pub select_scroll_progress: f32,
    /// 選曲画面の master/key/bgm 音量 0.0-1.0。
    pub select_master_volume: f32,
    pub select_key_volume: f32,
    pub select_bgm_volume: f32,
    /// 選択中バーにバナー画像があるか (OPTION_NO_BANNER=192 / OPTION_BANNER=193)。
    pub select_has_banner: bool,
    /// 選択中曲のレベル表記から取り出した数値。
    pub select_play_level: i64,
    /// 現在の曲のレベル表記から取り出した数値 (NUMBER_PLAYLEVEL=96)。
    pub play_level: i64,
    /// 現在の曲の #DIFFICULTY code。0=OTHER, 1=BEGINNER, 2=NORMAL, 3=HYPER, 4=ANOTHER, 5=INSANE。
    pub difficulty: i64,
    /// 現在の曲の #RANK / 判定ランク。0..4 は VERYHARD..VERYEASY、10 以上は直接倍率。
    pub judge_rank: Option<i32>,
    /// 選択中曲のベストEXスコア。
    pub select_ex_score: Option<u32>,
    /// 選択中曲のリプレイスロット有無。
    pub select_replay_slots: [bool; 4],
    /// 選択中リプレイスロット。Noneなら未選択。
    pub select_replay_index: Option<usize>,
    /// 選択中曲のクリアランプ番号。
    pub select_clear_index: i64,
    /// 選択中バー種別。OPTION_FOLDERBAR / SONGBAR / GRADEBAR の判定に使う。
    pub select_row_kind: SelectRowKind,
    /// 選択中 GradeBar の制約。OPTION_GRADEBAR_* (1002..1017) の判定に使う。
    pub select_course_constraints: CourseConstraintFlags,
    /// 選択中バーがフォルダかどうか。
    pub select_is_folder: bool,
    /// 選択中曲が library.db に登録済みかどうか (OPTION_PLAYABLEBAR=5)。
    pub select_in_library: bool,
    /// 選択中曲のノーツ数。
    pub select_total_notes: u32,
    /// beatoraja SongInformation-derived selected chart detail numbers.
    pub select_chart_normal_notes: u32,
    pub select_chart_long_notes: u32,
    pub select_chart_scratch_notes: u32,
    pub select_chart_long_scratch_notes: u32,
    pub select_chart_density: f32,
    pub select_chart_peak_density: f32,
    pub select_chart_end_density: f32,
    pub select_chart_total_gauge: f32,
    pub select_chart_main_bpm: f32,
    /// 選択中曲の代表BPM。
    pub select_bpm: f32,
    /// 選択中曲の最小BPM。
    pub select_min_bpm: f32,
    /// 選択中曲の最大BPM。
    pub select_max_bpm: f32,
    /// 選択中曲の長さ ms。
    pub select_length_ms: i64,
    /// 選択中曲のプレイ回数 / クリア回数 / ミスカウント。
    pub select_play_count: u32,
    pub select_clear_count: u32,
    pub select_bp: Option<u32>,
    /// Fast/Slow 内訳 (ref 410-419/421-424)。
    /// Play/Result 中は Some、それ以外は None。
    pub fast_slow_counts: Option<crate::snapshot::FastSlowJudgeCounts>,
    /// 過去ベスト max combo (ref 172)。
    pub best_max_combo: Option<u32>,
    /// ターゲット max combo (ref 173, 175 で使用)。
    pub target_max_combo: Option<u32>,
    /// 過去ベスト bp (ref 178 で使用)。
    pub best_bp: Option<u32>,
    /// Result update/draw ops 用の保存前ベスト。
    pub previous_best_ex_score: Option<u32>,
    pub previous_best_max_combo: Option<u32>,
    pub previous_best_bp: Option<u32>,
    /// ターゲット bp (ref 176, 178 で使用)。
    pub target_bp: Option<u32>,
    /// ターゲットクリアタイプの index (ref 371)。
    pub target_clear_index: Option<i64>,
    /// リザルト画面でクリアしたか (op 90=CLEAR, op 91=FAIL)。
    /// Play 中は None、Result 中は Some(true)=Fail / Some(false)=Clear。
    pub result_failed: Option<bool>,
    /// シーン終了フェードアウトのタイマー経過 ms (TIMER_FADEOUT=2)。
    /// None ならフェードアウト中でない。`timer: 2` の destination はこの値が
    /// Some のときだけ描画され、リザルト画面終了時のアニメーションを駆動する。
    pub fadeout_ms: Option<i32>,
    /// RESULT graph begin/end timers (150/151) and update score timer (152)。
    pub result_graph_begin_ms: Option<i32>,
    pub result_graph_end_ms: Option<i32>,
    pub result_update_score_ms: Option<i32>,
    /// RESULT replay slot status for OPTION_REPLAYDATA* / *_SAVED.
    pub result_replay_slots: [bool; 4],
    pub result_saved_replay_slots: [bool; 4],
    /// 閉店/FAILED 演出のタイマー経過 ms (TIMER_FAILED=3)。
    pub failed_ms: Option<i32>,
    /// Result timing distribution average (NUMBER_AVERAGE_TIMING=374).
    pub average_timing_ms: Option<f32>,
    /// Result timing distribution standard deviation (NUMBER_STDDEV_TIMING=376).
    pub stddev_timing_ms: Option<f32>,
    /// OPTION_AUTOPLAYON (33) / OPTION_AUTOPLAYOFF (32) 用。
    pub autoplay: bool,
    /// OPTION_NOW_LOADING (80) / OPTION_LOADED (81) 用。
    pub skin_loaded: bool,
    /// OPTION_MODE_COURSE (290) とステージ別 op (280..283 / 289) 用。未対応時は None。
    pub course_stage: Option<CourseStageMarker>,
    /// beatoraja `event_index(SKIN_EVENT_HSFIX)`。0=OFF, 1=START, 2=MAX, 3=MAIN, 4=MIN。
    pub hsfix_index: i32,
    /// beatoraja `NUMBER_MAINBPM` (92) 用の代表 BPM (プレイ中)。
    pub main_bpm: f32,
    /// Rm-skin F/S threshold 表示 (ms)。
    pub fs_threshold_ms: i32,
    /// HSFIX 連動の adjusted hidden cover (0..1)。
    pub adjusted_cover_progress: Option<f32>,
    /// HSFIX 連動の BPM 比率 (0..1)。
    pub adjusted_rate: Option<f32>,
    /// HSFIX 連動の BPM 比率 ×100 整数部。
    pub adjusted_rate_adot: Option<i32>,
    /// HitErrorVisualizer 用の直近判定タイミング (ms)。
    pub hit_error_ring: [i64; bmz_gameplay::hit_error::HIT_ERROR_RING_LEN],
    pub hit_error_ring_index: usize,
    /// `dynamicTimer` で定義された observe タイマーの経過 ms。None は timer_off。
    pub dynamic_timer_ms: [Option<i32>; SKIN_DYNAMIC_TIMER_COUNT],
    /// 選曲画面の設定フォルダ内。曲メタデータ用の op / text / number を抑制する。
    pub in_settings: bool,
    /// 設定項目の編集モード中 (`in_settings` と併用)。
    pub settings_editing: bool,
    /// 選曲中の曲行キーモード。beatoraja OPTION_MODE_* (160..164) 用。
    pub select_chart_key_mode: Option<KeyMode>,
}

impl Default for SkinDrawState {
    fn default() -> Self {
        Self {
            elapsed_ms: 0,
            ready_timer_ms: None,
            play_timer_ms: None,
            key_mode: KeyMode::default(),
            select_bar_elapsed_ms: 0,
            select_option_panel_elapsed_ms: 0,
            select_option_panel: 0,
            select_arrange_index: 0,
            select_gauge_index: 2,
            select_gauge_auto_shift_index: 0,
            select_target_index: 0,
            select_bga_index: 0,
            select_assist_index: 0,
            select_mode_index: 0,
            select_sort_index: 0,
            select_ln_mode_index: 0,
            mouse_x: None,
            mouse_y: None,
            combo: 0,
            max_combo: 0,
            ex_score: 0,
            total_notes: 0,
            past_notes: 0,
            judge_counts: DisplayJudgeCounts::default(),
            gauge: 0.0,
            gauge_type: 2,
            gauge_auto_shift: false,
            gauge_max: 100.0,
            gauge_border: 80.0,
            play_progress: 0.0,
            end_of_note: false,
            end_of_note_ms: None,
            bomb_ms: [None; LANE_COUNT],
            keyon_ms: [None; LANE_COUNT],
            keyoff_ms: [None; LANE_COUNT],
            lane_judge: [None; LANE_COUNT],
            judge_ms: [None; MAX_JUDGE_REGIONS],
            full_combo_ms: None,
            music_end_ms: None,
            judge_index: [None; MAX_JUDGE_REGIONS],
            judge_combo: [0; MAX_JUDGE_REGIONS],
            judge_timing_sign: [None; MAX_JUDGE_REGIONS],
            offset_lift_px: 0,
            offset_lanecover_px: 0,
            offset_hidden_cover_px: 0,
            skin_offsets: SkinOffsetValues::default(),
            hispeed: 0.0,
            timeleft_ms: 0,
            total_duration_ms: 0,
            lane_cover: 0.0,
            lift: 0.0,
            hidden_cover: 0.0,
            lane_cover_changing: false,
            lanecover_enabled: true,
            lift_enabled: true,
            hidden_enabled: false,
            now_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            has_bga: false,
            bga_enabled: true,
            has_stagefile: false,
            has_backbmp: false,
            bga_base: None,
            bga_layer: None,
            bga_layer2: None,
            bga_poor: None,
            bga_stretch: 1,
            judge_timing_ms: None,
            best_ex_score: None,
            projected_best_ex_score: None,
            best_clear_index: None,
            target_ex_score: None,
            judge_timing_offset_ms: 0,
            select_chart_count: 0,
            select_screen: false,
            select_scroll_progress: 0.0,
            select_master_volume: 1.0,
            select_key_volume: 1.0,
            select_bgm_volume: 1.0,
            select_has_banner: false,
            select_play_level: 0,
            play_level: 0,
            difficulty: 0,
            judge_rank: None,
            select_ex_score: None,
            select_replay_slots: [false; 4],
            select_replay_index: None,
            select_clear_index: 0,
            select_row_kind: SelectRowKind::Song,
            select_course_constraints: CourseConstraintFlags::default(),
            select_is_folder: false,
            select_in_library: true,
            select_total_notes: 0,
            select_chart_normal_notes: 0,
            select_chart_long_notes: 0,
            select_chart_scratch_notes: 0,
            select_chart_long_scratch_notes: 0,
            select_chart_density: 0.0,
            select_chart_peak_density: 0.0,
            select_chart_end_density: 0.0,
            select_chart_total_gauge: 0.0,
            select_chart_main_bpm: 0.0,
            select_bpm: 0.0,
            select_min_bpm: 0.0,
            select_max_bpm: 0.0,
            select_length_ms: 0,
            select_play_count: 0,
            select_clear_count: 0,
            select_bp: None,
            fast_slow_counts: None,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_bp: None,
            target_clear_index: None,
            result_failed: None,
            fadeout_ms: None,
            result_graph_begin_ms: None,
            result_graph_end_ms: None,
            result_update_score_ms: None,
            result_replay_slots: [false; 4],
            result_saved_replay_slots: [false; 4],
            failed_ms: None,
            average_timing_ms: None,
            stddev_timing_ms: None,
            autoplay: false,
            skin_loaded: true,
            course_stage: None,
            hsfix_index: 0,
            main_bpm: 0.0,
            fs_threshold_ms: 25,
            adjusted_cover_progress: None,
            adjusted_rate: None,
            adjusted_rate_adot: None,
            hit_error_ring: [bmz_gameplay::hit_error::HIT_ERROR_EMPTY;
                bmz_gameplay::hit_error::HIT_ERROR_RING_LEN],
            hit_error_ring_index: 0,
            dynamic_timer_ms: [None; SKIN_DYNAMIC_TIMER_COUNT],
            in_settings: false,
            settings_editing: false,
            select_chart_key_mode: None,
        }
    }
}

/// `dynamicTimer` observe 条件のエッジ検出用ランタイム。Renderer が保持する。
#[derive(Debug, Clone)]
pub struct DynamicTimerRuntime {
    starts: [Option<i32>; SKIN_DYNAMIC_TIMER_COUNT],
}

impl Default for DynamicTimerRuntime {
    fn default() -> Self {
        Self { starts: [None; SKIN_DYNAMIC_TIMER_COUNT] }
    }
}

impl DynamicTimerRuntime {
    pub fn reset(&mut self) {
        self.starts = [None; SKIN_DYNAMIC_TIMER_COUNT];
    }

    /// observe 条件を評価し、`state.dynamic_timer_ms` を更新する。
    pub fn advance(
        &mut self,
        document: &SkinDocument,
        mut state: SkinDrawState,
        now_ms: i32,
    ) -> SkinDrawState {
        for def in &document.dynamic_timers {
            let idx = def.id.saturating_sub(SKIN_DYNAMIC_TIMER_BASE) as usize;
            if idx >= SKIN_DYNAMIC_TIMER_COUNT {
                continue;
            }
            if eval_skin_draw_condition(&def.observe, state) {
                let start = self.starts[idx].get_or_insert(now_ms);
                state.dynamic_timer_ms[idx] = Some(now_ms.saturating_sub(*start));
            } else {
                self.starts[idx] = None;
                state.dynamic_timer_ms[idx] = None;
            }
        }
        state
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinTextState<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    pub subartist: &'a str,
    pub genre: &'a str,
    pub difficulty_name: &'a str,
    pub play_level: &'a str,
    pub target: &'a str,
    pub current_folder: &'a str,
    pub bar_text: &'a str,
    pub table_level: &'a str,
    pub table_text_primary: &'a str,
    pub table_text_secondary: &'a str,
    pub table_text_fallback: &'a str,
    pub course_stage: Option<CourseStageMarker>,
    pub course_titles: [&'a str; 10],
    /// beatoraja `SkinProperty.STRING_SEARCHWORD` (`ref=30`). Current song search
    /// query as typed by the user.
    pub search_word: &'a str,
    /// Multiplier applied to the rendered alpha of the `ref=30` text element.
    /// `1.0` keeps the skin-defined alpha unchanged; values < 1.0 are used for
    /// placeholder / inactive states (beatoraja `messageFontColor=GRAY` 相当).
    pub search_word_alpha: f32,
}

impl<'a> Default for SkinTextState<'a> {
    fn default() -> Self {
        Self {
            title: "",
            subtitle: "",
            artist: "",
            subartist: "",
            genre: "",
            difficulty_name: "",
            play_level: "",
            target: "",
            current_folder: "",
            bar_text: "",
            table_level: "",
            table_text_primary: "",
            table_text_secondary: "",
            table_text_fallback: "",
            course_stage: None,
            course_titles: [""; 10],
            search_word: "",
            search_word_alpha: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinManifest {
    #[serde(default)]
    pub textures: Vec<SkinTextureManifest>,
    #[serde(default)]
    pub play: SkinPlayManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinTextureManifest {
    pub id: u32,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SkinPlayManifest {
    pub note: Option<SkinImageManifest>,
    pub receptor: Option<SkinImageManifest>,
    pub judge_line: Option<SkinImageManifest>,
    pub gauge_frame: Option<SkinImageManifest>,
    pub gauge_fill: Option<SkinImageManifest>,
    pub combo_panel: Option<SkinImageManifest>,
    pub combo_panel_inactive: Option<SkinImageManifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SkinImageManifest {
    pub texture: u32,
    pub key_even_texture: Option<u32>,
    pub scratch_texture: Option<u32>,
    pub source_size: Option<SkinImageSize>,
    #[serde(default)]
    pub uv: TextureRegion,
    #[serde(default)]
    pub scale: SkinImageScale,
    pub border: Option<SkinImageBorder>,
}

impl SkinImageManifest {
    pub fn texture_for_lane(self, lane: Lane) -> u32 {
        match lane {
            Lane::Scratch | Lane::Scratch2 => self.scratch_texture.unwrap_or(self.texture),
            Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key9 | Lane::Key11 | Lane::Key13 => {
                self.key_even_texture.unwrap_or(self.texture)
            }
            Lane::Key1
            | Lane::Key3
            | Lane::Key5
            | Lane::Key7
            | Lane::Key8
            | Lane::Key10
            | Lane::Key12
            | Lane::Key14 => self.texture,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SkinImageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinImageScale {
    #[default]
    Stretch,
    NineSlice,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SkinImageBorder {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    #[serde(default)]
    pub unit: SkinImageBorderUnit,
}

impl SkinImageBorder {
    fn normalized(self, source_size: Option<SkinImageSize>) -> Option<Self> {
        match self.unit {
            SkinImageBorderUnit::Normalized => Some(self),
            SkinImageBorderUnit::Pixels => {
                let size = source_size?;
                if size.width <= 0.0 || size.height <= 0.0 {
                    return None;
                }
                Some(Self {
                    left: self.left / size.width,
                    right: self.right / size.width,
                    top: self.top / size.height,
                    bottom: self.bottom / size.height,
                    unit: SkinImageBorderUnit::Normalized,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinImageBorderUnit {
    #[default]
    Normalized,
    Pixels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSkinTexture {
    pub id: TextureId,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinRenderContext<'a> {
    pub phase: SkinPhase,
    pub elapsed_ms: i32,
    pub text: &'a [(TextSlot, String)],
    pub numbers: &'a [(NumberSlot, i64)],
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkinSource {
    Image { texture: SkinTextureId, uv: TextureRegion },
    Text { slot: TextSlot, style: TextStyle },
    Number { slot: NumberSlot, style: TextStyle, digits: u8 },
    Rect { color: Color },
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct TextureRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for TextureRegion {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSlot {
    Title,
    Artist,
    Judge,
    ClearType,
    ReplayState,
    Custom(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberSlot {
    Score,
    ExScore,
    Combo,
    MaxCombo,
    Gauge,
    Hispeed,
    JudgeCount,
    Custom(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinPlacement {
    pub phase: SkinPhase,
    pub time_ms: i32,
    pub rect: Rect,
    pub alpha: f32,
    pub blend: BlendMode,
    pub animation: Animation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinPhase {
    Select,
    Play,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Add,
    /// BGA Layer/Layer2 の黒クロマキー描画。
    /// beatoraja の `layer.frag` 相当: RGB(0,0,0) ピクセルを α=0 として描画する。
    LayerMask,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub time_ms: i32,
    pub rect: Rect,
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkinRenderItem {
    Image {
        texture: SkinTextureId,
        rect: Rect,
        uv: TextureRegion,
        tint: Color,
        blend: BlendMode,
        scale: SkinImageScale,
        border: Option<SkinImageBorder>,
        source_size: Option<SkinImageSize>,
        linear_filter: bool,
    },
    RotatedImage {
        texture: SkinTextureId,
        rect: Rect,
        uv: TextureRegion,
        tint: Color,
        blend: BlendMode,
        source_size: Option<SkinImageSize>,
        linear_filter: bool,
        angle_deg: f32,
        center: Point,
    },
    Text {
        origin: Point,
        text: String,
        style: TextStyle,
        blend: BlendMode,
    },
    Rect {
        rect: Rect,
        color: Color,
        blend: BlendMode,
    },
}

impl SkinObject {
    pub fn resolve(
        &self,
        phase: SkinPhase,
        elapsed_ms: i32,
        text: impl Fn(TextSlot) -> String,
        number: impl Fn(NumberSlot) -> i64,
    ) -> Vec<SkinRenderItem> {
        self.placements
            .iter()
            .filter(|placement| placement.phase == phase)
            .map(|placement| {
                let resolved = placement.resolve(elapsed_ms);
                match &self.source {
                    SkinSource::Image { texture, uv } => SkinRenderItem::Image {
                        texture: *texture,
                        rect: resolved.rect,
                        uv: *uv,
                        tint: Color::rgba(1.0, 1.0, 1.0, resolved.alpha),
                        blend: resolved.blend,
                        scale: SkinImageScale::Stretch,
                        border: None,
                        source_size: None,
                        linear_filter: false,
                    },
                    SkinSource::Text { slot, style } => SkinRenderItem::Text {
                        origin: Point { x: resolved.rect.x, y: resolved.rect.y },
                        text: text(*slot),
                        style: style.clone().with_alpha(resolved.alpha),
                        blend: resolved.blend,
                    },
                    SkinSource::Number { slot, style, digits } => SkinRenderItem::Text {
                        origin: Point { x: resolved.rect.x, y: resolved.rect.y },
                        text: format_number(number(*slot), *digits),
                        style: style.clone().with_alpha(resolved.alpha),
                        blend: resolved.blend,
                    },
                    SkinSource::Rect { color } => SkinRenderItem::Rect {
                        rect: resolved.rect,
                        color: color.with_alpha(color.a * resolved.alpha),
                        blend: resolved.blend,
                    },
                }
            })
            .collect()
    }
}

impl SkinDefinition {
    pub fn resolve(&self, context: &SkinRenderContext<'_>) -> Vec<SkinRenderItem> {
        self.objects
            .iter()
            .flat_map(|object| {
                object.resolve(
                    context.phase,
                    context.elapsed_ms,
                    |slot| lookup_text(context.text, slot),
                    |slot| lookup_number(context.numbers, slot),
                )
            })
            .collect()
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

    pub fn static_image_render_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        self.static_render_items(sources, state, SkinTextState::default())
    }

    pub fn static_render_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: SkinDrawState,
        text_state: SkinTextState<'_>,
    ) -> Vec<SkinRenderItem> {
        let (mut behind, front, failed_overlay) =
            self.static_render_items_split(sources, state, text_state);
        behind.extend(front);
        behind.extend(failed_overlay);
        behind
    }

    /// 静的 destination を `{"id":"notes"}` マーカーと `timer: 3` で3分割して描画アイテムを返す。
    /// 戻り値 `.0` はノーツより背面、`.1` はノーツより前面、`.2` は FAILED オーバーレイ。
    pub fn static_render_items_split(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: SkinDrawState,
        text_state: SkinTextState<'_>,
    ) -> (Vec<SkinRenderItem>, Vec<SkinRenderItem>, Vec<SkinRenderItem>) {
        let images = self.image_map();
        let enabled_options = self.enabled_options();
        let mut behind = Vec::new();
        let mut front = Vec::new();
        let mut failed_overlay = Vec::new();
        let mut after_notes_marker = false;
        let destinations = self.all_destinations(&enabled_options);
        for (index, destination) in destinations.iter().enumerate() {
            // `{"id":"notes"}` はノーツ描画位置マーカー。以降の destination はノーツ前面に積む。
            if destination.id == "notes" {
                after_notes_marker = true;
                continue;
            }
            if !test_skin_ops(&destination.op, &enabled_options, state) {
                continue;
            }
            if !eval_skin_draw_condition(&destination.draw, state) {
                continue;
            }
            if self.destination_uses_skin_gauge_bar_render(destination) {
                if let Some(items) = self.resolve_gauge_destination_items(
                    destination,
                    &enabled_options,
                    state,
                    sources,
                ) {
                    let target = destination_render_layer(
                        destination.timer,
                        after_notes_marker,
                        &mut behind,
                        &mut front,
                        &mut failed_overlay,
                    );
                    target.extend(items);
                }
                continue;
            }
            if let Some(items) = self.resolve_destination_items(
                destination,
                &images,
                &enabled_options,
                state,
                text_state,
                sources,
            ) {
                let after_notes_marker = after_notes_marker
                    || self.destination_looks_like_pre_notes_judge_line(
                        destination,
                        &images,
                        &enabled_options,
                        state,
                        destinations.get(index + 1).copied(),
                    );
                let target = destination_render_layer(
                    destination.timer,
                    after_notes_marker,
                    &mut behind,
                    &mut front,
                    &mut failed_overlay,
                );
                target.extend(items);
            }
        }
        (behind, front, failed_overlay)
    }

    fn destination_looks_like_pre_notes_judge_line(
        &self,
        destination: &SkinDestinationDef,
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        next_destination: Option<&SkinDestinationDef>,
    ) -> bool {
        if !matches!(next_destination, Some(next) if next.id == "notes")
            || destination.timer.is_some()
            || !destination_uses_lift_offset_only(destination)
            || skin_image_for_destination_id(destination.id.as_str(), images).is_none()
        {
            return false;
        }
        let Some(frame) = resolve_destination_frame(destination, 0, enabled_options, state) else {
            return false;
        };
        if frame.w < 100 || frame.h <= 0 || frame.h > 48 {
            return false;
        }
        let Some(note) = &self.note else {
            return false;
        };
        flatten_dst_entries(&note.dst, enabled_options).into_iter().any(|note_frame| {
            let Some(note_y) = note_frame.y else {
                return false;
            };
            frame.y >= note_y && frame.y <= note_y.saturating_add(64)
        })
    }

    /// `hiddenCover.disapearLine` をレーンカバー系 (HIDDEN / SUDDEN+ / LIFT) のクロップ境界として使う。
    fn disappear_line_for_lane_cover_clip(&self) -> Option<(i32, bool)> {
        let cover = self.hidden_cover.first()?;
        (cover.disappear_line > 0)
            .then_some((cover.disappear_line, cover.is_disappear_line_link_lift))
    }

    fn should_clip_image_at_disappear_line(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
    ) -> bool {
        if self.hidden_cover.is_empty() {
            return false;
        }
        if is_lift_lane_cover_id(&destination.id) || is_lift_lane_cover_id(&image.id) {
            return true;
        }
        destination_uses_lift_offset_only(destination)
            && self.hidden_cover.iter().any(|cover| cover.src == image.src)
    }

    /// `liftcover` 系 ID のみ。`offset: 3` だけの destination (判定線・数値表示など) は対象外。
    fn should_skip_lift_lane_cover_render(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
    ) -> bool {
        is_lift_lane_cover_id(&destination.id) || is_lift_lane_cover_id(&image.id)
    }

    /// LIFT 用 image は `offset: 3` で既にリフト分だけ動くため、`hiddenCover` の
    /// `isDisappearLineLinkLift` は二重適用しない。
    fn link_lift_for_lane_cover_clip(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
        link_lift: bool,
    ) -> bool {
        if is_lift_lane_cover_id(&destination.id)
            || is_lift_lane_cover_id(&image.id)
            || destination_uses_lift_offset_only(destination)
        {
            return false;
        }
        link_lift
    }

    fn resolve_destination_items(
        &self,
        destination: &SkinDestinationDef,
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        text_state: SkinTextState<'_>,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        if let Some(judge_def) = self.judge.iter().find(|judge| judge.id == destination.id) {
            let region = judge_def.index.clamp(0, MAX_JUDGE_REGIONS as i32 - 1) as usize;
            let elapsed = state.judge_ms[region]?;
            let judge_image_index = state.judge_index[region]?;
            return self.judge_render_items_for_def(
                judge_def,
                judge_image_index,
                state.judge_combo[region],
                elapsed,
                sources,
                state,
            );
        }

        let value_for_destination = self.value.iter().find(|value| value.id == destination.id);
        let elapsed = skin_timer_elapsed_ms(destination.timer, state).or_else(|| {
            value_for_destination
                .filter(|value| pre_ready_lane_cover_value_destination(destination, value, state))
                .map(|_| 0)
        })?;
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options, state)?;
        let is_hidden_cover_destination =
            self.hidden_cover.iter().any(|cover| cover.id == destination.id);
        apply_skin_offset_to_frame(destination, &mut frame, state, is_hidden_cover_destination);
        if !destination_mouse_rect_contains(destination, frame, state) {
            return None;
        }
        if let Some(visualizer) =
            self.hiterror_visualizer.iter().find(|visualizer| visualizer.id == destination.id)
        {
            return Some(self.hiterror_visualizer_render_items(
                visualizer,
                destination,
                frame,
                state,
            ));
        }
        if let Some(visualizer) =
            self.timingvisualizer.iter().find(|visualizer| visualizer.id == destination.id)
        {
            return Some(self.timing_visualizer_render_items(
                visualizer,
                destination,
                frame,
                state,
            ));
        }
        if let Some(graph) =
            self.timingdistributiongraph.iter().find(|graph| graph.id == destination.id)
        {
            return Some(self.timing_distribution_graph_render_items(
                graph,
                destination,
                frame,
                state,
            ));
        }
        if let Some(gauge_graph) = self.gaugegraph.iter().find(|graph| graph.id == destination.id) {
            return Some(self.gaugegraph_render_items(gauge_graph, destination, frame, state));
        }
        if let Some(judge_graph) = self.judgegraph.iter().find(|graph| graph.id == destination.id) {
            return Some(self.judgegraph_render_items(judge_graph, destination, frame, state));
        }
        if let Some(bpm_graph) = self.bpmgraph.iter().find(|graph| graph.id == destination.id) {
            return Some(self.bpmgraph_render_items(bpm_graph, destination, frame, state));
        }
        if let Some(image) = skin_image_for_destination_id(destination.id.as_str(), images) {
            if self.should_skip_lift_lane_cover_render(destination, image)
                && state.offset_lift_px == 0
            {
                return None;
            }
            let source = resolve_document_source(sources, &image.src)?;
            let pixel_rect = skin_image_pixel_rect(image, images);
            let mut uv = skin_image_texture_region_for_state(
                image,
                source.source_size,
                elapsed,
                Some(state),
                pixel_rect,
            );
            if self.should_clip_image_at_disappear_line(destination, image)
                && let Some((disappear_line, link_lift)) = self.disappear_line_for_lane_cover_clip()
            {
                clip_skin_cover_to_disappear_line(
                    &mut frame,
                    &mut uv,
                    disappear_line,
                    self.link_lift_for_lane_cover_clip(destination, image, link_lift),
                    state,
                );
                if frame.h <= 0 {
                    return None;
                }
            }
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                uv,
                source.source_size,
                self.w,
                self.h,
            );
            return Some(vec![skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
                Some(source.source_size),
                destination.filter != 0,
            )]);
        }

        if self.bga.as_ref().is_some_and(|bga| bga.id == destination.id) {
            return (state.has_bga && state.bga_enabled).then(|| {
                let rect = normalize_skin_frame_rect(frame, self.w, self.h);
                let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
                let destination_tint = Color::rgba(1.0, 1.0, 1.0, frame.a as f32 / 255.0);
                let stretch =
                    if destination.stretch < 0 { state.bga_stretch } else { destination.stretch };
                let mut items = Vec::new();
                if let Some(bga) = state.bga_poor {
                    let tint = multiply_bga_tints(destination_tint, bga);
                    items.push(bga_image_item(
                        bga,
                        stretch,
                        rect,
                        tint,
                        blend,
                        self.w,
                        self.h,
                        destination.filter != 0,
                    ));
                } else if let Some(bga) = state.bga_base {
                    let tint = multiply_bga_tints(destination_tint, bga);
                    items.push(bga_image_item(
                        bga,
                        stretch,
                        rect,
                        tint,
                        blend,
                        self.w,
                        self.h,
                        destination.filter != 0,
                    ));
                }
                // Layer / Layer2 は beatoraja の TYPE_LAYER と同様、黒ピクセルを
                // 透過させて Base に重ねる。例外として:
                //   - Add 指定時はクロマキー不要 (黒は加算寄与ゼロ)
                //   - 動画 BGA Layer は beatoraja でも `ffmpeg.frag` を使い
                //     クロマキーをかけない
                let layer_blend_for = |bga: SkinBgaFrame| {
                    if matches!(blend, BlendMode::Add) || bga.is_video {
                        blend
                    } else {
                        BlendMode::LayerMask
                    }
                };
                if state.bga_poor.is_none()
                    && let Some(bga) = state.bga_layer
                {
                    let tint = multiply_bga_tints(destination_tint, bga);
                    items.push(bga_image_item(
                        bga,
                        stretch,
                        rect,
                        tint,
                        layer_blend_for(bga),
                        self.w,
                        self.h,
                        destination.filter != 0,
                    ));
                }
                if state.bga_poor.is_none()
                    && let Some(bga) = state.bga_layer2
                {
                    let tint = multiply_bga_tints(destination_tint, bga);
                    items.push(bga_image_item(
                        bga,
                        stretch,
                        rect,
                        tint,
                        layer_blend_for(bga),
                        self.w,
                        self.h,
                        destination.filter != 0,
                    ));
                }
                if items.is_empty() {
                    items.push(SkinRenderItem::Rect {
                        rect,
                        color: Color::rgba(0.0, 0.0, 0.0, frame.a as f32 / 255.0),
                        blend,
                    });
                }
                items
            });
        }

        // imageset (キービーム・ボム等) を destination 自身のタイマー駆動で描画する。
        // timer が非アクティブな destination は上の skin_timer_elapsed_ms で除外済み。
        if let Some(imageset) = self.imageset.iter().find(|set| set.id == destination.id) {
            let image_id = if let Some(index) = skin_state_imageset_index(imageset.ref_id, state) {
                imageset.images.get(index.min(imageset.images.len().saturating_sub(1))).cloned()
            } else {
                let judge_index = imageset_ref_lane(imageset.ref_id)
                    .and_then(|lane| state.lane_judge[lane.index()]);
                imageset_image_for_index(imageset, judge_index)
            }?;
            let image = images.get(image_id.as_str())?;
            let source = resolve_document_source(sources, &image.src)?;
            let pixel_rect = skin_image_pixel_rect(image, images);
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                skin_image_texture_region_for_state(
                    image,
                    source.source_size,
                    elapsed,
                    Some(state),
                    pixel_rect,
                ),
                source.source_size,
                self.w,
                self.h,
            );
            return Some(vec![skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
                Some(source.source_size),
                destination.filter != 0,
            )]);
        }

        if let Some(value) = value_for_destination {
            let number = skin_value_number(value, state)?;
            return Some(self.value_number_render_items(
                &value.id,
                number,
                ResolvedSkinFrame::default(),
                frame,
                elapsed,
                sources,
                false,
                None,
            ));
        }

        if let Some(graph) = self.graph.iter().find(|graph| graph.id == destination.id)
            && let Some(item) = self.graph_render_item(graph, frame, state, sources)
        {
            return Some(vec![item]);
        }

        if let Some(text) = self.text.iter().find(|text| text.id == destination.id)
            && let Some(item) = self.text_render_item(text, frame, text_state)
        {
            return Some(vec![item]);
        }

        if let Some(slider) = self.slider.iter().find(|slider| slider.id == destination.id)
            && let Some(item) = self.slider_render_item(slider, destination, frame, state, sources)
        {
            return Some(vec![item]);
        }

        if self.destination_uses_skin_gauge_overlay_render(destination) {
            return self.resolve_gauge_destination_items(
                destination,
                enabled_options,
                state,
                sources,
            );
        }

        if let Some(graph) = self.graph.iter().find(|g| g.id == destination.id) {
            return self.graph_render_item(graph, frame, state, sources).map(|item| vec![item]);
        }

        if let Some(item) = special_image_render_item(destination, frame, self.w, self.h) {
            return Some(vec![item]);
        }

        let hidden_cover = self.hidden_cover.iter().find(|cover| cover.id == destination.id)?;
        self.hidden_cover_render_item(hidden_cover, destination, frame, state, sources)
            .map(|item| vec![item])
    }

    fn resolve_offset_destination_items(
        &self,
        destination: &SkinDestinationDef,
        offset: (i32, i32),
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        text_state: SkinTextState<'_>,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        if !test_skin_ops(&destination.op, enabled_options, state)
            || !eval_skin_draw_condition(&destination.draw, state)
        {
            return None;
        }
        let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options, state)?;
        frame.x += offset.0;
        frame.y += offset.1;
        apply_skin_offset_to_frame(destination, &mut frame, state, false);

        if let Some(image) = skin_image_for_destination_id(destination.id.as_str(), images) {
            if self.should_skip_lift_lane_cover_render(destination, image)
                && state.offset_lift_px == 0
            {
                return None;
            }
            let source = resolve_document_source(sources, &image.src)?;
            let pixel_rect = skin_image_pixel_rect(image, images);
            let mut uv = skin_image_texture_region_for_state(
                image,
                source.source_size,
                elapsed,
                Some(state),
                pixel_rect,
            );
            if self.should_clip_image_at_disappear_line(destination, image)
                && let Some((disappear_line, link_lift)) = self.disappear_line_for_lane_cover_clip()
            {
                clip_skin_cover_to_disappear_line(
                    &mut frame,
                    &mut uv,
                    disappear_line,
                    self.link_lift_for_lane_cover_clip(destination, image, link_lift),
                    state,
                );
                if frame.h <= 0 {
                    return None;
                }
            }
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                uv,
                source.source_size,
                self.w,
                self.h,
            );
            return Some(vec![skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
                Some(source.source_size),
                destination.filter != 0,
            )]);
        }

        if let Some(value) = self.value.iter().find(|value| value.id == destination.id) {
            let number = skin_value_number_or_songlist_level(value, state)?;
            return Some(self.value_number_render_items(
                &value.id,
                number,
                ResolvedSkinFrame::default(),
                frame,
                elapsed,
                sources,
                false,
                None,
            ));
        }

        if let Some(graph) = self.graph.iter().find(|graph| graph.id == destination.id)
            && let Some(item) = self.graph_render_item(graph, frame, state, sources)
        {
            return Some(vec![item]);
        }

        if let Some(text) = self.text.iter().find(|text| text.id == destination.id)
            && let Some(item) = self.text_render_item(text, frame, text_state)
        {
            return Some(vec![item]);
        }

        None
    }

    pub fn enabled_options(&self) -> Vec<i32> {
        if let Some(ops) = &self.user_selected_options {
            return ops.clone();
        }
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
    }

    /// 有効なオプション条件に基づいて `destination` エントリを展開し、
    /// 描画対象の `SkinDestinationDef` の参照リストを返す。
    /// Returns the first dst frame of any text element whose `ref_id` equals
    /// `ref_id`, normalized into the `0.0..=1.0` rendered viewport coordinate
    /// space (top-left origin). Used by bmz-app to position the IME candidate
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

    pub fn select_render_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
    ) -> Vec<SkinRenderItem> {
        self.select_render_items_with_dynamic_timers(
            sources,
            snapshot,
            None,
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
        )
    }

    pub fn select_render_items_with_dynamic_timers(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
    ) -> Vec<SkinRenderItem> {
        let (state, selected_row) = self.select_draw_state(snapshot, dynamic_timers);
        let text = SkinTextState {
            title: selected_row.map(|row| row.title.as_str()).unwrap_or(&snapshot.selected_title),
            subtitle: select_detail_subtitle(snapshot, selected_row),
            artist: select_detail_artist(snapshot, selected_row),
            genre: "",
            difficulty_name: if snapshot.in_settings {
                ""
            } else {
                selected_row.map(|row| row.difficulty_name.as_str()).unwrap_or_default()
            },
            play_level: selected_row.map(|row| row.play_level.as_str()).unwrap_or_default(),
            target: if snapshot.in_settings { "" } else { &snapshot.target },
            current_folder: &snapshot.current_folder,
            table_level: selected_row.map(|row| row.table_level.as_str()).unwrap_or_default(),
            course_titles: selected_row
                .map(|row| string_array_refs(&row.course_titles))
                .unwrap_or_default(),
            search_word: &snapshot.search_word,
            search_word_alpha: snapshot.search_word_alpha,
            ..SkinTextState::default()
        };

        let images = self.image_map();
        let enabled_options = self.enabled_options();
        let mut items = Vec::new();
        for destination in self.all_destinations(&enabled_options) {
            if destination.id == self.songlist.as_ref().map(|list| list.id.as_str()).unwrap_or("") {
                items.extend(self.select_songlist_items(
                    sources,
                    snapshot,
                    &images,
                    &enabled_options,
                    state,
                ));
                continue;
            }
            if !crate::select_settings_dest::test_select_destination_visible(
                settings_dest_index,
                destination,
                &enabled_options,
                state,
                snapshot,
                selected_row,
                eval_skin_draw_condition,
                test_skin_ops,
            ) {
                continue;
            }
            if let Some(resolved) = self.resolve_destination_items(
                destination,
                &images,
                &enabled_options,
                state,
                text,
                sources,
            ) {
                items.extend(resolved);
            }
        }
        items
    }

    fn select_draw_state<'a>(
        &self,
        snapshot: &'a SelectSnapshot,
        dynamic_timers: Option<&mut DynamicTimerRuntime>,
    ) -> (SkinDrawState, Option<&'a SelectRowSnapshot>) {
        let selected_row = snapshot.rows.iter().find(|row| row.index == snapshot.selected_index);
        let mouse_position = snapshot.mouse_position.map(|(x, y)| {
            (x.clamp(0.0, 1.0) * self.w as f32, (1.0 - y.clamp(0.0, 1.0)) * self.h as f32)
        });
        let mut state = SkinDrawState {
            elapsed_ms: (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            select_bar_elapsed_ms: (snapshot.selection_time.0 / 1_000)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            select_option_panel_elapsed_ms: (snapshot.option_panel_time.0 / 1_000)
                .clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
            select_option_panel: snapshot.option_panel,
            select_arrange_index: select_arrange_index(&snapshot.arrange),
            select_gauge_index: select_gauge_index(&snapshot.gauge),
            select_gauge_auto_shift_index: select_gauge_auto_shift_index(
                &snapshot.gauge_auto_shift,
            ),
            select_target_index: select_target_index(&snapshot.target),
            select_bga_index: select_bga_index(&snapshot.bga),
            select_assist_index: select_assist_index(&snapshot.assist),
            select_mode_index: select_mode_index(&snapshot.select_mode),
            select_sort_index: select_sort_index(&snapshot.select_sort),
            select_ln_mode_index: select_ln_mode_index(&snapshot.select_ln_mode),
            select_scroll_progress: select_scroll_progress(snapshot),
            select_master_volume: snapshot.master_volume,
            select_key_volume: snapshot.key_volume,
            select_bgm_volume: snapshot.bgm_volume,
            select_has_banner: snapshot.banner_image,
            select_chart_count: snapshot.chart_count,
            select_screen: true,
            select_play_level: selected_row.map(select_row_level_number).unwrap_or(0),
            play_level: selected_row.map(select_row_level_number).unwrap_or(0),
            difficulty: selected_row.map(select_row_difficulty_code).unwrap_or(0),
            judge_rank: selected_row.and_then(|row| row.judge_rank),
            select_ex_score: selected_row.and_then(|row| row.ex_score),
            select_replay_slots: selected_row.map(|row| row.replay_slots).unwrap_or([false; 4]),
            select_replay_index: selected_row.and_then(select_row_replay_index),
            select_clear_index: selected_row.map(select_row_clear_index).unwrap_or(0) as i64,
            select_row_kind: selected_row.map(|row| row.kind).unwrap_or(SelectRowKind::Song),
            select_course_constraints: selected_row
                .map(|row| row.course_constraints)
                .unwrap_or_default(),
            select_is_folder: selected_row.is_some_and(|row| row.is_folder),
            select_in_library: selected_row.is_none_or(|row| row.in_library),
            select_total_notes: selected_row.map(|row| row.total_notes).unwrap_or(0),
            select_chart_normal_notes: selected_row.map(|row| row.chart_normal_notes).unwrap_or(0),
            select_chart_long_notes: selected_row.map(|row| row.chart_long_notes).unwrap_or(0),
            select_chart_scratch_notes: selected_row
                .map(|row| row.chart_scratch_notes)
                .unwrap_or(0),
            select_chart_long_scratch_notes: selected_row
                .map(|row| row.chart_long_scratch_notes)
                .unwrap_or(0),
            select_chart_density: selected_row.map(|row| row.chart_density).unwrap_or(0.0),
            select_chart_peak_density: selected_row
                .map(|row| row.chart_peak_density)
                .unwrap_or(0.0),
            select_chart_end_density: selected_row.map(|row| row.chart_end_density).unwrap_or(0.0),
            select_chart_total_gauge: selected_row.map(|row| row.chart_total_gauge).unwrap_or(0.0),
            select_chart_main_bpm: selected_row.map(|row| row.chart_main_bpm).unwrap_or(0.0),
            select_bpm: selected_row.map(|row| row.initial_bpm).unwrap_or(0.0),
            select_min_bpm: selected_row.map(|row| row.min_bpm).unwrap_or(0.0),
            select_max_bpm: selected_row.map(|row| row.max_bpm).unwrap_or(0.0),
            select_length_ms: selected_row.map(|row| row.length_ms).unwrap_or(0),
            select_play_count: selected_row.map(|row| row.play_count).unwrap_or(0),
            select_clear_count: selected_row.map(|row| row.clear_count).unwrap_or(0),
            select_bp: selected_row.and_then(|row| row.bp),
            max_combo: selected_row.and_then(|row| row.max_combo).unwrap_or(0),
            total_notes: selected_row.map(|row| row.total_notes).unwrap_or(0),
            gauge: selected_row.and_then(|row| row.gauge_value).unwrap_or(0.0),
            gauge_auto_shift: snapshot.gauge_auto_shift != "OFF",
            ex_score: selected_row.and_then(|row| row.ex_score).unwrap_or(0),
            in_settings: snapshot.in_settings,
            settings_editing: snapshot.settings_editing,
            select_chart_key_mode: selected_row.and_then(|row| row.chart_key_mode),
            mouse_x: mouse_position.map(|position| position.0),
            mouse_y: mouse_position.map(|position| position.1),
            ..SkinDrawState::default()
        };
        if let Some(runtime) = dynamic_timers {
            state = runtime.advance(self, state, state.elapsed_ms);
        }
        (state, selected_row)
    }

    pub fn select_click_hit(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        self.select_click_hits(sources, snapshot, settings_dest_index)
            .into_iter()
            .rev()
            .find(|hit| rect_contains(hit.rect, x, y))
    }

    pub fn select_slider_hit(
        &self,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let (state, selected_row) = self.select_draw_state(snapshot, None);
        let enabled_options = self.enabled_options();
        self.all_destinations(&enabled_options)
            .into_iter()
            .filter_map(|destination| {
                if !crate::select_settings_dest::test_select_destination_visible(
                    settings_dest_index,
                    destination,
                    &enabled_options,
                    state,
                    snapshot,
                    selected_row,
                    eval_skin_draw_condition,
                    test_skin_ops,
                ) {
                    return None;
                }
                let slider = self.slider.iter().find(|slider| slider.id == destination.id)?;
                self.destination_slider_hit(slider, destination, &enabled_options, state, x, y)
            })
            .next_back()
    }

    fn select_click_hits(
        &self,
        _sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
    ) -> Vec<SkinClickHit> {
        let (state, selected_row) = self.select_draw_state(snapshot, None);
        let enabled_options = self.enabled_options();
        let images = self.image_map();
        let mut hits = Vec::new();
        for destination in self.all_destinations(&enabled_options) {
            if destination.id == self.songlist.as_ref().map(|list| list.id.as_str()).unwrap_or("") {
                hits.extend(self.select_songlist_click_hits(snapshot, &enabled_options, state));
                continue;
            }
            if !crate::select_settings_dest::test_select_destination_visible(
                settings_dest_index,
                destination,
                &enabled_options,
                state,
                snapshot,
                selected_row,
                eval_skin_draw_condition,
                test_skin_ops,
            ) {
                continue;
            }
            let Some(target) = self.click_target_for_destination(destination, &images) else {
                continue;
            };
            let Some(rect) = self.destination_click_rect(destination, &enabled_options, state)
            else {
                continue;
            };
            hits.push(SkinClickHit { target, rect });
        }
        hits
    }

    fn select_songlist_click_hits(
        &self,
        snapshot: &SelectSnapshot,
        enabled_options: &[i32],
        state: SkinDrawState,
    ) -> Vec<SkinClickHit> {
        let Some(songlist) = &self.songlist else {
            return Vec::new();
        };
        let selected_row_position =
            select_snapshot_selected_row_position(&snapshot.rows, snapshot.selected_index) as i32;
        let mut hits = Vec::new();
        for (row_position, row) in snapshot.rows.iter().enumerate() {
            let offset = row_position as i32 - selected_row_position;
            let slot = songlist.center + offset;
            if !songlist.clickable.contains(&slot) || slot < 0 {
                continue;
            }
            let selected = row_position as i32 == selected_row_position;
            let row_destinations = if selected { &songlist.liston } else { &songlist.listoff };
            let Some(row_destination) =
                destination_entry_at(row_destinations, slot as usize, enabled_options)
            else {
                continue;
            };
            let row_state = SkinDrawState {
                select_play_level: select_row_level_number(row),
                play_level: select_row_level_number(row),
                difficulty: select_row_difficulty_code(row),
                judge_rank: row.judge_rank,
                select_ex_score: row.ex_score,
                select_replay_slots: row.replay_slots,
                select_replay_index: select_row_replay_index(row),
                select_clear_index: select_row_clear_index(row) as i64,
                select_row_kind: row.kind,
                select_course_constraints: row.course_constraints,
                select_is_folder: row.is_folder,
                select_in_library: row.in_library,
                select_total_notes: row.total_notes,
                select_chart_normal_notes: row.chart_normal_notes,
                select_chart_long_notes: row.chart_long_notes,
                select_chart_scratch_notes: row.chart_scratch_notes,
                select_chart_long_scratch_notes: row.chart_long_scratch_notes,
                select_chart_density: row.chart_density,
                select_chart_peak_density: row.chart_peak_density,
                select_chart_end_density: row.chart_end_density,
                select_chart_total_gauge: row.chart_total_gauge,
                select_chart_main_bpm: row.chart_main_bpm,
                select_length_ms: row.length_ms,
                select_play_count: row.play_count,
                select_clear_count: row.clear_count,
                select_bp: row.bp,
                max_combo: row.max_combo.unwrap_or(0),
                total_notes: row.total_notes,
                gauge: row.gauge_value.unwrap_or(0.0),
                ex_score: row.ex_score.unwrap_or(0),
                select_chart_key_mode: row.chart_key_mode,
                ..state
            };
            let Some(rect) =
                self.destination_click_rect(row_destination, enabled_options, row_state)
            else {
                continue;
            };
            hits.push(SkinClickHit {
                target: SkinClickTarget::SelectRow { row_index: row.index },
                rect,
            });
        }
        hits
    }

    fn click_target_for_destination(
        &self,
        destination: &SkinDestinationDef,
        images: &HashMap<&str, &SkinImageDef>,
    ) -> Option<SkinClickTarget> {
        if let Some(image) = images.get(destination.id.as_str())
            && let Some(event_id) = image.act
        {
            return Some(SkinClickTarget::Event { event_id, click: image.click });
        }
        let imageset = self.imageset.iter().find(|set| set.id == destination.id)?;
        imageset.act.map(|event_id| SkinClickTarget::Event { event_id, click: imageset.click })
    }

    fn destination_click_rect(
        &self,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: SkinDrawState,
    ) -> Option<Rect> {
        let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options, state)?;
        apply_skin_offset_to_frame(destination, &mut frame, state, false);
        if !destination_mouse_rect_contains(destination, frame, state) {
            return None;
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        if rect.width <= 0.0 || rect.height <= 0.0 { None } else { Some(rect) }
    }

    fn destination_slider_hit(
        &self,
        slider: &SkinSliderDef,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: SkinDrawState,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        if !slider.changeable || !matches!(slider.slider_type, 17..=19) {
            return None;
        }
        let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options, state)?;
        apply_skin_offset_to_frame(destination, &mut frame, state, false);
        if !destination_mouse_rect_contains(destination, frame, state) {
            return None;
        }
        let mouse_x = x.clamp(0.0, 1.0) * self.w as f32;
        let mouse_y = (1.0 - y.clamp(0.0, 1.0)) * self.h as f32;
        let value = slider_value_at(slider, frame, mouse_x, mouse_y)?;
        Some(SkinSliderHit { slider_type: slider.slider_type, value })
    }

    fn select_songlist_items(
        &self,
        sources: &HashMap<String, SkinDocumentTexture>,
        snapshot: &SelectSnapshot,
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let Some(songlist) = &self.songlist else {
            return Vec::new();
        };
        let mut items = Vec::new();
        let selected_row_position =
            select_snapshot_selected_row_position(&snapshot.rows, snapshot.selected_index) as i32;
        for (row_position, row) in snapshot.rows.iter().enumerate() {
            let row_state = SkinDrawState {
                select_play_level: select_row_level_number(row),
                play_level: select_row_level_number(row),
                difficulty: select_row_difficulty_code(row),
                select_ex_score: row.ex_score,
                select_replay_slots: row.replay_slots,
                select_replay_index: select_row_replay_index(row),
                select_clear_index: select_row_clear_index(row) as i64,
                select_row_kind: row.kind,
                select_course_constraints: row.course_constraints,
                select_is_folder: row.is_folder,
                select_in_library: row.in_library,
                select_total_notes: row.total_notes,
                select_chart_normal_notes: row.chart_normal_notes,
                select_chart_long_notes: row.chart_long_notes,
                select_chart_scratch_notes: row.chart_scratch_notes,
                select_chart_long_scratch_notes: row.chart_long_scratch_notes,
                select_chart_density: row.chart_density,
                select_chart_peak_density: row.chart_peak_density,
                select_chart_end_density: row.chart_end_density,
                select_chart_total_gauge: row.chart_total_gauge,
                select_chart_main_bpm: row.chart_main_bpm,
                select_length_ms: row.length_ms,
                select_play_count: row.play_count,
                select_clear_count: row.clear_count,
                select_bp: row.bp,
                max_combo: row.max_combo.unwrap_or(0),
                total_notes: row.total_notes,
                gauge: row.gauge_value.unwrap_or(0.0),
                ex_score: row.ex_score.unwrap_or(0),
                select_chart_key_mode: row.chart_key_mode,
                ..state
            };
            let offset = row_position as i32 - selected_row_position;
            let slot = songlist.center + offset;
            if slot < 0 {
                continue;
            }
            let selected = row_position as i32 == selected_row_position;
            let row_destinations = if selected { &songlist.liston } else { &songlist.listoff };
            let Some(row_destination) =
                destination_entry_at(row_destinations, slot as usize, enabled_options)
            else {
                continue;
            };
            let elapsed = skin_timer_elapsed_ms(row_destination.timer, state).unwrap_or(0);
            let Some(mut row_frame) =
                resolve_destination_frame(row_destination, elapsed, enabled_options, row_state)
            else {
                continue;
            };
            let row_origin = (row_frame.x, row_frame.y);
            apply_skin_offset_to_frame(row_destination, &mut row_frame, state, false);
            if let Some(item) = self.select_bar_item(row, row_destination, row_frame, sources) {
                items.push(item);
            }
            if select_row_shows_lamp(row) {
                let clear_index = select_row_clear_index(row);
                items.extend(self.select_songlist_child_items_by_index(
                    &songlist.lamp,
                    clear_index,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
            }
            if select_row_shows_score_decorations(row) {
                items.extend(self.select_songlist_level_items(
                    &songlist.level,
                    row,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
                for label_index in select_row_label_indices(row) {
                    items.extend(self.select_songlist_child_items_by_index(
                        &songlist.label,
                        label_index,
                        row_origin,
                        images,
                        enabled_options,
                        row_state,
                        sources,
                    ));
                }
                if select_row_shows_course_trophy(row)
                    && let Some(trophy_index) = select_row_trophy_index(row)
                {
                    items.extend(self.select_songlist_child_items_by_index(
                        &songlist.trophy,
                        trophy_index,
                        row_origin,
                        images,
                        enabled_options,
                        row_state,
                        sources,
                    ));
                }
                items.extend(self.select_songlist_all_child_items(
                    &songlist.judgegraph,
                    row,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
                items.extend(self.select_songlist_all_child_items(
                    &songlist.bpmgraph,
                    row,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
            }
            if select_row_shows_folder_distribution(row) {
                items.extend(self.select_songlist_all_child_items(
                    &songlist.graph,
                    row,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
            }
            items.extend(self.select_songlist_text_items(
                row,
                row_origin,
                images,
                enabled_options,
                row_state,
                sources,
            ));
        }
        items
    }

    fn select_songlist_all_child_items(
        &self,
        entries: &[DestinationListEntry],
        row: &SelectRowSnapshot,
        row_origin: (i32, i32),
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let mut items = Vec::new();
        for destination in destination_entries(entries, enabled_options) {
            if let Some(judge_graph) =
                self.judgegraph.iter().find(|graph| graph.id == destination.id)
            {
                items.extend(self.select_note_distribution_graph_render_items(
                    row,
                    judge_graph,
                    destination,
                    row_origin,
                    enabled_options,
                    state,
                ));
                continue;
            }
            if select_row_shows_folder_distribution(row)
                && let Some(graph) = self.graph.iter().find(|graph| graph.id == destination.id)
            {
                items.extend(self.select_folder_distribution_graph_render_items(
                    row,
                    graph,
                    destination,
                    row_origin,
                    enabled_options,
                    state,
                    sources,
                ));
                continue;
            }
            if let Some(mut resolved) = self.resolve_offset_destination_items(
                destination,
                row_origin,
                images,
                enabled_options,
                state,
                SkinTextState::default(),
                sources,
            ) {
                items.append(&mut resolved);
            }
        }
        items
    }

    fn select_folder_distribution_graph_render_items(
        &self,
        row: &SelectRowSnapshot,
        graph: &SkinGraphDef,
        destination: &SkinDestinationDef,
        row_origin: (i32, i32),
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let Some(source) = sources.get(&graph.src) else {
            return Vec::new();
        };
        if !test_skin_ops(&destination.op, enabled_options, state)
            || !eval_skin_draw_condition(&destination.draw, state)
        {
            return Vec::new();
        }
        let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, state) else {
            return Vec::new();
        };
        let Some(mut frame) =
            resolve_destination_frame(destination, elapsed, enabled_options, state)
        else {
            return Vec::new();
        };
        frame.x += row_origin.0;
        frame.y += row_origin.1;
        apply_skin_offset_to_frame(destination, &mut frame, state, false);

        let total: u32 = row.folder_lamp_counts.iter().sum();
        if total == 0 {
            return Vec::new();
        }

        let dst = normalize_skin_frame_rect(frame, self.w, self.h);
        let source_w = source.source_size.width.max(1.0);
        let source_h = source.source_size.height.max(1.0);
        let cell_w = skin_grid_cell_size(graph.w, graph.divx.max(11));
        let cell_h = skin_grid_cell_size(graph.h, graph.divy);
        if cell_w <= 0 || cell_h <= 0 {
            return Vec::new();
        }

        let mut items = Vec::new();
        let mut filled = 0.0;
        for lamp_index in (0..row.folder_lamp_counts.len()).rev() {
            let count = row.folder_lamp_counts[lamp_index];
            if count == 0 {
                continue;
            }
            let width = dst.width * (count as f32 / total as f32);
            if width <= 0.0 {
                continue;
            }
            let rect = Rect { x: dst.x + filled, width, ..dst };
            let source_x = graph.x + cell_w * lamp_index as i32;
            let uv = TextureRegion {
                x: source_x as f32 / source_w,
                y: graph.y as f32 / source_h,
                width: cell_w as f32 / source_w,
                height: cell_h as f32 / source_h,
            };
            items.push(SkinRenderItem::Image {
                texture: source.texture,
                rect,
                uv,
                tint: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
                ),
                blend: BlendMode::Normal,
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: Some(source.source_size),
                linear_filter: false,
            });
            filled += width;
        }
        items
    }

    fn select_songlist_level_items(
        &self,
        entries: &[DestinationListEntry],
        row: &SelectRowSnapshot,
        row_origin: (i32, i32),
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let level_index = select_row_difficulty_code(row).clamp(0, i64::MAX) as usize;
        self.select_songlist_child_items_by_index(
            entries,
            level_index,
            row_origin,
            images,
            enabled_options,
            state,
            sources,
        )
    }

    fn select_songlist_child_items_by_index(
        &self,
        entries: &[DestinationListEntry],
        index: usize,
        row_origin: (i32, i32),
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let mut items = Vec::new();
        let Some(destination) = destination_entry_at(entries, index, enabled_options) else {
            return items;
        };
        if let Some(mut resolved) = self.resolve_offset_destination_items(
            destination,
            row_origin,
            images,
            enabled_options,
            state,
            SkinTextState::default(),
            sources,
        ) {
            items.append(&mut resolved);
        }
        items
    }

    fn select_songlist_text_items(
        &self,
        row: &SelectRowSnapshot,
        row_origin: (i32, i32),
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let Some(songlist) = &self.songlist else {
            return Vec::new();
        };
        let mut items = Vec::new();
        let text_state = SkinTextState {
            bar_text: &row.title,
            table_level: &row.table_level,
            ..SkinTextState::default()
        };
        let destinations = destination_entries(&songlist.text, enabled_options);
        let Some(destination) = destinations
            .get(select_row_bar_text_index(row))
            .or_else(|| destinations.first())
            .copied()
        else {
            return items;
        };
        {
            if let Some(mut resolved) = self.resolve_offset_destination_items(
                destination,
                row_origin,
                images,
                enabled_options,
                state,
                text_state,
                sources,
            ) {
                items.append(&mut resolved);
            }
        }
        items
    }

    fn select_bar_item(
        &self,
        row: &SelectRowSnapshot,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let imageset = self.imageset.iter().find(|set| set.id == destination.id)?;
        let image_index = select_row_bar_image_index(row);
        let image_id = imageset.images.get(image_index).or_else(|| imageset.images.first())?;
        let image = self.image.iter().find(|image| image.id == *image_id)?;
        let source = resolve_document_source(sources, &image.src)?;
        let elapsed =
            skin_timer_elapsed_ms(destination.timer, SkinDrawState::default()).unwrap_or(0);
        let (rect, uv) = stretch_skin_image_geometry(
            destination.stretch,
            normalize_skin_frame_rect(frame, self.w, self.h),
            skin_image_texture_region(image, source.source_size, elapsed),
            source.source_size,
            self.w,
            self.h,
        );
        Some(skin_image_item_for_frame(
            source.texture,
            rect,
            uv,
            frame,
            destination.center,
            if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
            Some(source.source_size),
            destination.filter != 0,
        ))
    }

    pub fn note_image_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let note = self.note.as_ref()?;
        let image_id = note.note.get(beatoraja_note_index(lane, key_mode))?;
        self.note_part_render_item(image_id, rect, sources)
    }

    /// ロングノート胴体画像（`note.lnbody`）を描画する。
    /// `lnbody` が未定義のレーンは通常ノート画像（`note.note`）で代用する。
    pub fn note_long_body_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let note = self.note.as_ref()?;
        let index = beatoraja_note_index(lane, key_mode);
        let image_id = note.lnbody.get(index).or_else(|| note.note.get(index))?;
        self.note_part_render_item(image_id, rect, sources)
    }

    /// Mine ノート画像（`note.mine`）を描画する。スキンが `mine` を定義していない、
    /// または該当レーンの index が空なら `None` を返し、呼び出し側でフォールバックを
    /// 使う想定。
    pub fn note_mine_render_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let note = self.note.as_ref()?;
        let image_id = note.mine.get(beatoraja_note_index(lane, key_mode))?;
        self.note_part_render_item(image_id, rect, sources)
    }

    pub fn note_height_for_lane(&self, lane: Lane, key_mode: KeyMode) -> Option<f32> {
        let note = self.note.as_ref()?;
        let index = beatoraja_note_index(lane, key_mode);
        if let Some(size) = note.size.get(index).copied().filter(|size| *size > 0) {
            return Some(size as f32 / self.h.max(1) as f32);
        }
        let image_id = note.note.get(index)?;
        let image = self.image.iter().find(|image| image.id == *image_id)?;
        let divy = image.divy.max(1);
        Some((image.h.max(1) as f32 / divy as f32) / self.h.max(1) as f32)
    }

    fn note_part_render_item(
        &self,
        image_id: &str,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let image = self.image.iter().find(|image| image.id == image_id)?;
        let source = resolve_document_source(sources, &image.src)?;
        Some(SkinRenderItem::Image {
            texture: source.texture,
            rect,
            uv: skin_image_texture_region(image, source.source_size, 0),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: Some(source.source_size),
            linear_filter: false,
        })
    }

    pub fn note_group_render_items(
        &self,
        note_y: f32,
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let Some(note) = self.note.as_ref() else {
            return Vec::new();
        };
        let images = self.image_map();
        let enabled_options = self.enabled_options();
        // Key1 はすべてのキーモードでインデックス 0 なので KeyMode::K7 で代用。
        let Some(area) = self.note_lane_area(Lane::Key1, KeyMode::K7, &enabled_options) else {
            return Vec::new();
        };
        let canvas_h = self.h.max(1) as f32;
        let bottom_y = note_progress_to_y(area, note_y, state, canvas_h);
        let lane_bottom_px = canvas_h * (1.0 - (area.y + area.height));
        let timeline_bottom_px = canvas_h * (1.0 - bottom_y);
        let mut items = Vec::new();
        for destination in &note.group {
            if !test_skin_ops(&destination.op, &enabled_options, state)
                || !eval_skin_draw_condition(&destination.draw, state)
            {
                continue;
            }
            let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, state) else {
                continue;
            };
            let Some(mut frame) =
                resolve_destination_frame(destination, elapsed, &enabled_options, state)
            else {
                continue;
            };
            frame.y += (timeline_bottom_px - lane_bottom_px).round() as i32;
            apply_bar_line_skin_offsets_to_frame(destination, &mut frame, state);
            let Some(image) = images.get(destination.id.as_str()) else {
                continue;
            };
            let Some(source) = resolve_document_source(sources, &image.src) else {
                continue;
            };
            let pixel_rect = skin_image_pixel_rect(image, &images);
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                skin_image_texture_region_for_state(
                    image,
                    source.source_size,
                    elapsed,
                    Some(state),
                    pixel_rect,
                ),
                source.source_size,
                self.w,
                self.h,
            );
            let item = skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
                Some(source.source_size),
                destination.filter != 0,
            );
            items.push(item);
        }
        items
    }

    /// `note.dst` の中から有効な条件に一致するエントリを探し、
    /// 指定レーンのノートエリア矩形（正規化座標）を返す。
    /// ノートエリアはレーン列全体を表す。Y軸: 上端=ノートが最も早い時点、下端=判定ライン。
    ///
    /// note.dst の解釈は2通り:
    /// 1. `load_beatoraja_json` 経由で読んだ場合: `expand_json_skin_value` により条件ブロックが
    ///    展開済みで、dst はレーン順の Frame エントリ列になっている。
    ///    → 全 Frame をフラット配列として `lane_idx` 番目を使う。
    /// 2. 直接 JSON パースした場合: Conditional エントリの frames 配列がレーン対応を持つ。
    ///    → 条件を満たす Conditional を探し、その frames[lane_idx] を使う。
    pub fn note_lane_area(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        enabled_options: &[i32],
    ) -> Option<Rect> {
        let note = self.note.as_ref()?;
        let lane_idx = beatoraja_note_index(lane, key_mode);
        let canvas_w = self.w as f32;
        let canvas_h = self.h as f32;

        // 全エントリを展開してフラット化。Conditional は条件が合うものだけ展開する。
        let mut flat: Vec<SkinAnimationDef> = Vec::new();
        for entry in &note.dst {
            match entry {
                SkinDstEntry::Frame(f) => flat.push(*f),
                SkinDstEntry::Conditional { if_ops, frames } => {
                    if test_skin_dst_if(if_ops, enabled_options) {
                        flat.extend_from_slice(frames);
                    }
                }
            }
        }

        let frame = flat.get(lane_idx)?;
        if let (Some(x), Some(y), Some(w), Some(h)) = (frame.x, frame.y, frame.w, frame.h) {
            Some(normalize_skin_frame_rect(
                ResolvedSkinFrame { x, y, w, h, ..ResolvedSkinFrame::default() },
                canvas_w as u32,
                canvas_h as u32,
            ))
        } else {
            None
        }
    }

    fn apply_notes_offset_to_rect(&self, rect: Rect, state: SkinDrawState) -> Rect {
        let Some(offset) = state.skin_offsets.get(OFFSET_NOTES_1P) else {
            return rect;
        };
        let canvas_w = self.w.max(1) as f32;
        let canvas_h = self.h.max(1) as f32;
        let offset_w = offset.w as f32 / canvas_w;
        let offset_h = offset.h as f32 / canvas_h;
        Rect {
            x: rect.x + offset.x as f32 / canvas_w - offset_w / 2.0,
            y: rect.y - offset.y as f32 / canvas_h - offset_h / 2.0,
            width: rect.width + offset_w,
            height: rect.height + offset_h,
        }
    }

    pub fn gauge_render_items(
        &self,
        gauge: f32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        let state = SkinDrawState { elapsed_ms, gauge, ..SkinDrawState::default() };
        let enabled_options = self.enabled_options();
        let destination =
            self.all_destinations(&enabled_options).into_iter().find(|destination| {
                self.destination_uses_skin_gauge_bar_render(destination)
                    && destination.timer.is_none()
                    && test_skin_ops(&destination.op, &enabled_options, state)
                    && eval_skin_draw_condition(&destination.draw, state)
            })?;
        self.resolve_gauge_destination_items(destination, &enabled_options, state, sources)
    }

    fn destination_uses_skin_gauge_bar_render(&self, destination: &SkinDestinationDef) -> bool {
        self.skin_gauge_for_destination(destination).is_some()
            && destination.draw.trim().is_empty()
            && destination.blend != 2
    }

    fn destination_uses_skin_gauge_overlay_render(&self, destination: &SkinDestinationDef) -> bool {
        self.skin_gauge_for_destination(destination).is_some()
            && (!destination.draw.trim().is_empty() || destination.blend == 2)
    }

    fn skin_gauge_for_destination(
        &self,
        destination: &SkinDestinationDef,
    ) -> Option<&SkinGaugeDef> {
        self.gauges
            .iter()
            .find(|gauge| gauge.id == destination.id)
            .or_else(|| self.gauge.as_ref().filter(|gauge| gauge.id == destination.id))
    }

    fn resolve_gauge_destination_items(
        &self,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        let gauge_def = self.skin_gauge_for_destination(destination)?;
        let elapsed_ms = skin_timer_elapsed_ms(destination.timer, state)?;
        let frame = resolve_destination_frame(destination, elapsed_ms, enabled_options, state)?;
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let parts = gauge_def.parts.max(1);
        let max = state.gauge_max.max(1.0);
        let border = state.gauge_border;
        let notes = skin_gauge_notes_count(state.gauge, parts, max);
        let animation = skin_gauge_animation_index(gauge_def, state);
        let exgauge = skin_gauge_node_base(state.gauge_type);
        let anim_type = gauge_def.gauge_type;
        let base_color = skin_gauge_frame_color(frame);
        let blend = skin_gauge_destination_blend(destination);
        let mut items = Vec::new();
        for part in 1..=parts {
            let part_border = part as f32 * max / parts as f32;
            let node_index = skin_gauge_sprite_node_index(
                exgauge,
                part,
                notes,
                animation,
                border,
                part_border,
                gauge_def.nodes.len(),
                anim_type,
            );
            let node_id = gauge_def.nodes.get(node_index)?;
            let part_rect = skin_gauge_part_rect(rect, parts, part);
            if let Some(item) = self.gauge_image_render_item(
                node_id,
                part_rect,
                elapsed_ms,
                sources,
                base_color,
                blend,
                destination.filter != 0,
            ) {
                items.push(item);
            }
            if anim_type == SKIN_GAUGE_ANIM_FLICKERING
                && notes > 0
                && part == notes
                && let Some(tip_index) = skin_gauge_flicker_tip_node_index(
                    exgauge,
                    border,
                    part_border,
                    gauge_def.nodes.len(),
                )
                && let Some(tip_id) = gauge_def.nodes.get(tip_index)
            {
                let flicker_alpha = skin_gauge_flicker_alpha(animation, gauge_def.cycle);
                let flicker_color = Color::rgba(
                    base_color.r,
                    base_color.g,
                    base_color.b,
                    base_color.a * flicker_alpha,
                );
                if let Some(item) = self.gauge_image_render_item(
                    tip_id,
                    part_rect,
                    elapsed_ms,
                    sources,
                    flicker_color,
                    blend,
                    destination.filter != 0,
                ) {
                    items.push(item);
                }
            }
        }
        Some(items)
    }

    pub fn judge_render_items(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        self.judge_render_items_with_offsets(
            judge,
            combo,
            elapsed_ms,
            SkinOffsetValues::default(),
            sources,
        )
    }

    pub fn judge_render_items_with_offsets(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
        skin_offsets: SkinOffsetValues,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        let judge_image_index = judge_image_index(judge)?;
        let judge_def = self.judge.first()?;
        self.judge_render_items_for_def(
            judge_def,
            judge_image_index,
            combo,
            elapsed_ms,
            sources,
            SkinDrawState { skin_offsets, ..SkinDrawState::default() },
        )
    }

    pub fn judge_render_items_for_def(
        &self,
        judge: &SkinJudgeDef,
        judge_index: usize,
        combo: u32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        state: SkinDrawState,
    ) -> Option<Vec<SkinRenderItem>> {
        let image_destination = judge.images.get(judge_index)?;
        let enabled_options = self.enabled_options();
        let mut image_frame = resolve_destination_frame_until_end(
            image_destination,
            elapsed_ms,
            &enabled_options,
            state,
        )?;
        let offset_state = SkinDrawState {
            skin_offsets: state.skin_offsets,
            offset_lift_px: state.offset_lift_px,
            offset_lanecover_px: state.offset_lanecover_px,
            ..SkinDrawState::default()
        };
        // OFFSET_JUDGE_1P (id 32) は beatoraja では明示注入されず、destination の
        // `offsets` フィールドで宣言されたぶんだけ適用される。ここで重ねて
        // 注入すると、`offsets: [32]` を持つ skin (beatoraja 標準形) で
        // 二重適用になり、判定文字とコンボ数の Y が乖離する原因になる。
        apply_skin_offset_to_frame(image_destination, &mut image_frame, offset_state, false);
        // beatoraja はコンボ数字をシフト前の判定文字 X を基準に配置する。
        let image_frame_for_numbers = image_frame;
        if judge.shift
            && combo > 0
            && let Some(number_destination) = judge.numbers.get(judge_index)
            && let Some(number_frame) = resolve_destination_frame_until_end(
                number_destination,
                elapsed_ms,
                &enabled_options,
                state,
            )
        {
            image_frame.x -=
                self.value_number_length(&number_destination.id, combo as i64, number_frame) / 2;
        }
        let image = self.image.iter().find(|image| image.id == image_destination.id)?;
        let source = resolve_document_source(sources, &image.src)?;
        let uv = skin_image_texture_region(image, source.source_size, elapsed_ms);
        let (rect, uv) = stretch_skin_image_geometry(
            image_destination.stretch,
            normalize_skin_frame_rect(image_frame, self.w, self.h),
            uv,
            source.source_size,
            self.w,
            self.h,
        );
        let mut items = vec![skin_image_item_for_frame(
            source.texture,
            rect,
            uv,
            image_frame,
            image_destination.center,
            BlendMode::Normal,
            Some(source.source_size),
            image_destination.filter != 0,
        )];
        if combo > 0
            && let Some(number_destination) = judge.numbers.get(judge_index)
            && let Some(mut number_frame) = resolve_destination_frame_until_end(
                number_destination,
                elapsed_ms,
                &enabled_options,
                state,
            )
        {
            // beatoraja は SkinNumber に `setRelative(true)` を立てるため、
            // destination の offsets を適用しても x/y は移動せず w/h/r/a だけ
            // 加算される。これにより combo digit の最終位置は
            // base_frame.y (= 適用後 image_frame.y) + number_frame.y_orig となり、
            // 判定文字と同じ量だけ y シフトする (中心アンカー伸縮)。
            apply_skin_offset_to_frame_relative(
                number_destination,
                &mut number_frame,
                offset_state,
            );
            if let Some(value) = self.value.iter().find(|value| value.id == number_destination.id) {
                Self::apply_beatoraja_judge_number_dst_x(&mut number_frame, value.digit);
            }
            items.extend(self.value_number_render_items(
                &number_destination.id,
                combo as i64,
                image_frame_for_numbers,
                number_frame,
                elapsed_ms,
                sources,
                false,
                Some(2),
            ));
        }
        Some(items)
    }

    /// beatoraja `JsonPlaySkinObjectLoader` が judge number の各 dst に適用する X 補正。
    fn beatoraja_judge_number_dst_x(dst_w: i32, digit: i32) -> i32 {
        dst_w.saturating_mul(digit.max(0)) / 2
    }

    fn apply_beatoraja_judge_number_dst_x(frame: &mut ResolvedSkinFrame, digit: i32) {
        frame.x -= Self::beatoraja_judge_number_dst_x(frame.w, digit);
    }

    fn value_number_length(&self, value_id: &str, number: i64, frame: ResolvedSkinFrame) -> i32 {
        let Some(value) = self.value.iter().find(|value| value.id == value_id) else {
            return 0;
        };
        let max_digits = value.digit.max(0) as usize;
        let padding = number_padding(value);
        let digits = if ref_id_is_signed(value.ref_id) {
            display_signed_number_digits(
                number,
                max_digits,
                padding.is_zero_padding(),
                value.divx.max(1) as u32,
            )
        } else {
            display_number_digits(number, max_digits, padding)
        };
        if digits.is_empty() { 0 } else { digits.len() as i32 * (frame.w + value.space) }
    }

    pub fn judge_image_render_item(
        &self,
        judge: &str,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        self.judge_render_items(judge, 0, elapsed_ms, sources)?.into_iter().next()
    }

    fn value_number_render_items(
        &self,
        value_id: &str,
        number: i64,
        base_frame: ResolvedSkinFrame,
        frame: ResolvedSkinFrame,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        compact_digits: bool,
        align_override: Option<i32>,
    ) -> Vec<SkinRenderItem> {
        let Some(value) = self.value.iter().find(|value| value.id == value_id) else {
            return Vec::new();
        };
        let Some(source) = sources.get(&value.src) else {
            return Vec::new();
        };
        let divx = value.divx.max(1);
        let divy = value.divy.max(1);
        let source_width_px =
            if value.w == -1 { source.source_size.width.round() as i32 } else { value.w };
        let source_height_px =
            if value.h == -1 { source.source_size.height.round() as i32 } else { value.h };
        let cell_width_px = (source_width_px / divx) as f32;
        let cell_height_px = (source_height_px / divy) as f32;
        if cell_width_px <= 0.0 || cell_height_px <= 0.0 {
            return Vec::new();
        }
        let padding = number_padding(value);
        let max_digits = value.digit.max(0) as usize;
        let digits = if ref_id_is_signed(value.ref_id) {
            display_signed_number_digits(number, max_digits, padding.is_zero_padding(), divx as u32)
        } else {
            display_number_digits(number, max_digits, padding)
        };
        // 桁間スペース (space フィールド、px 単位)
        let digit_step = frame.w + value.space;
        // 先頭の空き桁数 (align のためのオフセット計算に使用)
        let shiftbase = max_digits.saturating_sub(digits.len());
        // align=0: 右寄せ (デフォルト), align=1: 左寄せ, align=2: 中央
        let align = align_override.unwrap_or(value.align);
        let shift = match align {
            1 => digit_step * shiftbase as i32,
            2 => digit_step * shiftbase as i32 / 2,
            _ => 0,
        };

        digits
            .into_iter()
            .enumerate()
            .map(|(index, digit)| {
                let digit_position = if compact_digits { index } else { shiftbase + index } as i32;
                let rect = normalize_skin_frame_rect(
                    ResolvedSkinFrame {
                        x: base_frame.x + frame.x + digit_step * digit_position - shift,
                        y: base_frame.y + frame.y,
                        w: frame.w,
                        h: frame.h,
                        ..frame
                    },
                    self.w,
                    self.h,
                );
                let uv = Self::value_digit_texture_region(
                    value,
                    digit.into(),
                    elapsed_ms,
                    source.source_size,
                    cell_width_px,
                    cell_height_px,
                    divx,
                    divy,
                );
                let tint = Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
                );
                SkinRenderItem::Image {
                    texture: source.texture,
                    rect,
                    uv,
                    tint,
                    blend: BlendMode::Normal,
                    scale: SkinImageScale::Stretch,
                    border: None,
                    source_size: Some(source.source_size),
                    linear_filter: false,
                }
            })
            .collect()
    }

    fn value_digit_texture_region(
        value: &SkinValueDef,
        digit: u32,
        elapsed_ms: i32,
        source_size: SkinImageSize,
        cell_width_px: f32,
        cell_height_px: f32,
        divx: i32,
        divy: i32,
    ) -> TextureRegion {
        let source_width = source_size.width.max(1.0);
        let source_height = source_size.height.max(1.0);
        let digit_column = digit as i32 % divx;
        let digit_row = digit as i32 / divx;
        let animation_rows = divy.saturating_sub(digit_row).max(1);
        let animation_row = if value.cycle > 0 && animation_rows > 1 {
            (elapsed_ms.rem_euclid(value.cycle) * animation_rows / value.cycle)
                .min(animation_rows - 1)
        } else {
            0
        };
        let source_row = (digit_row + animation_row).min(divy - 1);
        TextureRegion {
            x: (value.x as f32 + cell_width_px * digit_column as f32) / source_width,
            y: (value.y as f32 + cell_height_px * source_row as f32) / source_height,
            width: cell_width_px / source_width,
            height: cell_height_px / source_height,
        }
    }

    fn gauge_image_render_item(
        &self,
        image_id: &str,
        rect: Rect,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        tint: Color,
        blend: BlendMode,
        linear_filter: bool,
    ) -> Option<SkinRenderItem> {
        let image = self.image.iter().find(|image| image.id == image_id)?;
        let source = resolve_document_source(sources, &image.src)?;
        let uv = skin_image_texture_region(image, source.source_size, elapsed_ms);
        let (rect, uv) =
            stretch_skin_image_geometry(0, rect, uv, source.source_size, self.w, self.h);
        Some(SkinRenderItem::Image {
            texture: source.texture,
            rect,
            uv,
            tint,
            blend,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: Some(source.source_size),
            linear_filter,
        })
    }

    fn text_render_item(
        &self,
        text: &SkinTextDef,
        frame: ResolvedSkinFrame,
        state: SkinTextState<'_>,
    ) -> Option<SkinRenderItem> {
        let content = skin_state_text(text, state);
        if content.is_empty() {
            return None;
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        // beatoraja は dst.x を align 基準点として扱う（align=1=center なら
        // dst.x がテキストの中央, align=2=right なら dst.x がテキストの右端）。
        // bmz の renderer は origin を「テキストボックスの左端」として扱うので、
        // align に応じて origin.x を平行移動してから渡す。
        let origin_x = match text.align {
            1 => rect.x - rect.width / 2.0,
            2 => rect.x - rect.width,
            _ => rect.x,
        };
        // beatoraja `STRING_SEARCHWORD` (ref=30) は placeholder 状態で
        // messageFontColor=GRAY (半透明) になる。bmz では state から渡される
        // multiplier を skin 由来の alpha に掛け合わせて同様の見た目を再現する。
        let mut alpha = frame.a as f32 / 255.0;
        if text.ref_id == 30 {
            alpha *= state.search_word_alpha.clamp(0.0, 1.0);
        }
        Some(SkinRenderItem::Text {
            origin: Point { x: origin_x, y: rect.y },
            text: content,
            style: TextStyle {
                font_id: (!text.font.is_empty()).then(|| text.font.clone()),
                size: frame.h.abs().max(text.size).max(1) as f32 / self.h.max(1) as f32,
                bitmap_size: skin_text_bitmap_size(text, &self.font, self.h),
                color: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    alpha,
                ),
                layer: TextLayer::Ui,
                align: skin_text_align(text.align),
                max_width: frame.w.abs() as f32 / self.w.max(1) as f32,
                overflow: skin_text_overflow(text.overflow),
                wrapping: text.wrapping,
                outline: skin_text_outline(text, self.h),
                shadow: skin_text_shadow(text, self.w, self.h),
            },
            blend: BlendMode::Normal,
        })
    }

    fn hiterror_visualizer_render_items(
        &self,
        visualizer: &SkinHitErrorVisualizerDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        if visualizer.hiterror_mode == 0 {
            return Vec::new();
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let window = visualizer.window_length.clamp(1, 100) as usize;
        let width = visualizer.width.max(1) as f32;
        let line_width = visualizer.line_width.clamp(1, 4) as f32;
        let center_ms = visualizer.judge_width_millis.max(1) as f32;
        let judge_width_rate = width / (center_ms * 2.0 + 1.0);
        let line_color =
            skin_hex_color(&visualizer.line_color).unwrap_or(Color::rgba(0.6, 0.8, 1.0, 0.5));
        let center_color =
            skin_hex_color(&visualizer.center_color).unwrap_or(Color::rgba(1.0, 1.0, 1.0, 1.0));
        let canvas_h = rect.height.max(1.0);
        let mut items = Vec::new();
        let center_x = rect.x + rect.width / 2.0 - line_width / 2.0;
        items.push(SkinRenderItem::Rect {
            rect: Rect { x: center_x, y: rect.y, width: line_width, height: canvas_h },
            color: center_color.with_alpha(center_color.a * frame_alpha),
            blend,
        });
        let index = state.hit_error_ring_index;
        let recent = &state.hit_error_ring;
        for i in 1..=window {
            let ring_index = (index as i64 - window as i64 + i as i64)
                .rem_euclid(bmz_gameplay::hit_error::HIT_ERROR_RING_LEN as i64)
                as usize;
            let sample = recent[ring_index];
            if sample == bmz_gameplay::hit_error::HIT_ERROR_EMPTY {
                continue;
            }
            let clamped = sample
                .clamp(-visualizer.judge_width_millis as i64, visualizer.judge_width_millis as i64)
                as f32;
            let x = rect.x + width / 2.0 - line_width / 2.0 - clamped * judge_width_rate;
            let alpha = if visualizer.color_mode == 0 {
                line_color.a * (i as f32 / (window as f32 / 2.0)).min(1.0)
            } else {
                line_color.a
            };
            let bar_h = if visualizer.draw_decay != 0 {
                canvas_h * i as f32 / window as f32
            } else {
                canvas_h
            };
            items.push(SkinRenderItem::Rect {
                rect: Rect { x, y: rect.y + canvas_h - bar_h, width: line_width, height: bar_h },
                color: Color::rgba(line_color.r, line_color.g, line_color.b, alpha * frame_alpha),
                blend,
            });
        }
        items
    }

    fn gaugegraph_render_items(
        &self,
        graph: &SkinGaugeGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let points = &self.result_gauge_graph_points;
        if points.is_empty() {
            return Vec::new();
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let max = state.gauge_max.max(1.0);
        let border = points.first().map(|point| point.border).unwrap_or(state.gauge_border);
        let color_index = gaugegraph_color_index(
            points.last().map(|point| point.gauge_type).unwrap_or(state.gauge_type),
        );
        let colors = gaugegraph_colors(graph, color_index, frame_alpha);
        let border_y = rect.y + rect.height * (1.0 - (border / max).clamp(0.0, 1.0));
        let line_w = (2.0 / self.w.max(1) as f32).max(0.001);
        let line_h = (2.0 / self.h.max(1) as f32).max(0.001);
        let render_progress = (state.elapsed_ms.max(0) as f32 / 1500.0).clamp(0.0, 1.0);
        let render_x = rect.x + rect.width * render_progress;
        let mut items = Vec::new();
        items.push(SkinRenderItem::Rect { rect, color: colors.graph_bg, blend });
        if border_y > rect.y {
            items.push(SkinRenderItem::Rect {
                rect: Rect { x: rect.x, y: rect.y, width: rect.width, height: border_y - rect.y },
                color: colors.border_bg,
                blend,
            });
        }
        for pair in points.windows(2) {
            let from = pair[0];
            let to = pair[1];
            let x1 = rect.x + point_ratio(points, from.time_ms) * rect.width;
            if x1 > render_x {
                break;
            }
            let x2 = (rect.x + point_ratio(points, to.time_ms) * rect.width).min(render_x);
            let y1 = gaugegraph_y(rect, from.value, max);
            let y2 = gaugegraph_y(rect, to.value, max);
            if (x2 - x1).abs() <= f32::EPSILON {
                continue;
            }
            if from.value < border && to.value < border {
                push_gaugegraph_segment(
                    &mut items,
                    x1,
                    x2,
                    y1,
                    y2,
                    line_w,
                    line_h,
                    colors.graph_line,
                    blend,
                );
            } else if from.value >= border && to.value >= border {
                push_gaugegraph_segment(
                    &mut items,
                    x1,
                    x2,
                    y1,
                    y2,
                    line_w,
                    line_h,
                    colors.border_line,
                    blend,
                );
            } else {
                let split_x = if (to.value - from.value).abs() <= f32::EPSILON {
                    x1
                } else {
                    x1 + (x2 - x1)
                        * ((border - from.value) / (to.value - from.value)).clamp(0.0, 1.0)
                };
                let graph_color =
                    if from.value < border { colors.graph_line } else { colors.border_line };
                let border_color =
                    if from.value < border { colors.border_line } else { colors.graph_line };
                push_gaugegraph_segment(
                    &mut items,
                    x1,
                    split_x,
                    y1,
                    border_y,
                    line_w,
                    line_h,
                    graph_color,
                    blend,
                );
                push_gaugegraph_segment(
                    &mut items,
                    split_x,
                    x2,
                    border_y,
                    y2,
                    line_w,
                    line_h,
                    border_color,
                    blend,
                );
            }
        }
        if points.len() == 1 {
            let y = gaugegraph_y(rect, points[0].value, max);
            let color =
                if points[0].value < border { colors.graph_line } else { colors.border_line };
            items.push(SkinRenderItem::Rect {
                rect: Rect { x: rect.x, y, width: (render_x - rect.x).max(line_w), height: line_h },
                color,
                blend,
            });
        }
        items
    }

    fn timing_visualizer_render_items(
        &self,
        visualizer: &SkinTimingVisualizerDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        _state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        if self.result_timing_points.is_empty() {
            return Vec::new();
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let width = visualizer.width.max(1) as f32;
        let center_ms = visualizer.judge_width_millis.max(1) as f32;
        let line_w = (visualizer.line_width.clamp(1, 4) as f32 / self.w.max(1) as f32).max(0.001);
        let judge_width_rate = width / (center_ms * 2.0 + 1.0);
        let center_color = timing_color(&visualizer.center_color, frame_alpha);
        let base_line_color = timing_color(&visualizer.line_color, frame_alpha);
        let mut items = Vec::new();
        items.extend(timing_judge_band_items(
            rect,
            center_ms,
            frame_alpha,
            blend,
            timing_visualizer_judge_colors(visualizer),
        ));
        let center_x = rect.x + rect.width / 2.0 - line_w / 2.0;
        items.push(SkinRenderItem::Rect {
            rect: Rect { x: center_x, y: rect.y, width: line_w, height: rect.height },
            color: center_color,
            blend,
        });

        let window =
            self.result_timing_points.len().min(bmz_gameplay::hit_error::HIT_ERROR_RING_LEN);
        for (index, point) in self.result_timing_points.iter().rev().take(window).enumerate() {
            let delta_ms = point.delta_us as f32 / 1_000.0;
            if delta_ms.abs() > center_ms {
                continue;
            }
            let x = rect.x + rect.width / 2.0 - line_w / 2.0
                + delta_ms * judge_width_rate / width * rect.width;
            let age = (window - index) as f32 / window.max(1) as f32;
            let alpha = if visualizer.draw_decay == 1 { age } else { 1.0 };
            let color = judge_timing_color(point.judge, visualizer, base_line_color)
                .with_alpha(base_line_color.a * alpha);
            let height = if visualizer.draw_decay == 1 { rect.height * age } else { rect.height };
            items.push(SkinRenderItem::Rect {
                rect: Rect { x, y: rect.y + rect.height - height, width: line_w, height },
                color,
                blend,
            });
        }
        items
    }

    fn timing_distribution_graph_render_items(
        &self,
        graph: &SkinTimingDistributionGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        _state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        if self.result_timing_points.is_empty() {
            return Vec::new();
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let width = graph.width.max(1);
        let line_px = graph.line_width.clamp(1, width);
        let buckets = (width / line_px).max(1) as usize;
        let center = buckets / 2;
        let mut counts = vec![0u32; buckets];
        for point in &self.result_timing_points {
            let delta_ms = (point.delta_us as f32 / 1_000.0).round() as i32;
            let bucket = center as i32 + delta_ms;
            if (0..buckets as i32).contains(&bucket) {
                counts[bucket as usize] += 1;
            }
        }
        let max_count = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
        let bar_w = (rect.width / buckets.max(1) as f32).max(1.0 / self.w.max(1) as f32);
        let mut items = timing_judge_band_items(
            rect,
            center as f32,
            frame_alpha,
            blend,
            timing_distribution_judge_colors(graph),
        );
        let graph_color = timing_color(&graph.graph_color, frame_alpha);
        for (index, count) in counts.into_iter().enumerate() {
            if count == 0 {
                continue;
            }
            let height = rect.height * count as f32 / max_count;
            items.push(SkinRenderItem::Rect {
                rect: Rect {
                    x: rect.x + index as f32 * bar_w,
                    y: rect.y + rect.height - height,
                    width: bar_w,
                    height,
                },
                color: graph_color,
                blend,
            });
        }
        let stats = timing_stats(&self.result_timing_points);
        if graph.draw_average == 1 {
            let color = timing_color(&graph.average_color, frame_alpha);
            let x = timing_distribution_x(rect, center, stats.average_ms);
            items.push(SkinRenderItem::Rect {
                rect: Rect { x, y: rect.y, width: bar_w.max(0.001), height: rect.height },
                color,
                blend,
            });
        }
        if graph.draw_dev == 1 {
            let color = timing_color(&graph.dev_color, frame_alpha);
            for x in [
                timing_distribution_x(rect, center, stats.average_ms + stats.stddev_ms),
                timing_distribution_x(rect, center, stats.average_ms - stats.stddev_ms),
            ] {
                items.push(SkinRenderItem::Rect {
                    rect: Rect { x, y: rect.y, width: bar_w.max(0.001), height: rect.height },
                    color,
                    blend,
                });
            }
        }
        items
    }

    fn judgegraph_render_items(
        &self,
        graph: &SkinJudgeGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        _state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let density = &self.play_judge_graph_density;
        if density.is_empty() {
            return Vec::new();
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let max_density = density.iter().copied().max().unwrap_or(1).max(1) as f32;
        let count = density.len().max(1) as f32;
        let gap = if graph.no_gap != 0 { 0.0 } else { 1.0 };
        let bar_w = ((rect.width - gap * (count - 1.0)).max(1.0) / count).max(1.0);
        let color = Color::rgba(0.75, 0.85, 1.0, 0.85 * frame_alpha);
        let mut items = Vec::new();
        for (index, value) in density.iter().enumerate() {
            if *value == 0 {
                continue;
            }
            let x = rect.x + index as f32 * (bar_w + gap);
            let height = rect.height * (*value as f32 / max_density);
            items.push(SkinRenderItem::Rect {
                rect: Rect { x, y: rect.y + rect.height - height, width: bar_w, height },
                color,
                blend,
            });
        }
        items
    }

    fn select_note_distribution_graph_render_items(
        &self,
        row: &SelectRowSnapshot,
        graph: &SkinJudgeGraphDef,
        destination: &SkinDestinationDef,
        row_origin: (i32, i32),
        enabled_options: &[i32],
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        if row.chart_distribution.is_empty()
            || !test_skin_ops(&destination.op, enabled_options, state)
            || !eval_skin_draw_condition(&destination.draw, state)
        {
            return Vec::new();
        }
        let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, state) else {
            return Vec::new();
        };
        let Some(mut frame) =
            resolve_destination_frame(destination, elapsed, enabled_options, state)
        else {
            return Vec::new();
        };
        frame.x += row_origin.0;
        frame.y += row_origin.1;
        apply_skin_offset_to_frame(destination, &mut frame, state, false);
        if !destination_mouse_rect_contains(destination, frame, state) {
            return Vec::new();
        }

        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return Vec::new();
        }
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let max_density =
            row.chart_distribution.iter().map(|second| second.total()).max().unwrap_or(1).max(20)
                as f32;
        let count = row.chart_distribution.len().max(1) as f32;
        let pixel_w = 1.0 / self.w.max(1) as f32;
        let pixel_h = 1.0 / self.h.max(1) as f32;
        let gap_x = if graph.no_gap_x != 0 { 0.0 } else { pixel_w };
        let gap_y = if graph.no_gap != 0 { 0.0 } else { pixel_h };
        let bar_w = ((rect.width - gap_x * (count - 1.0)).max(pixel_w) / count).max(pixel_w);
        let colors = note_distribution_colors(frame_alpha);
        let mut items = Vec::new();

        for (index, second) in row.chart_distribution.iter().enumerate() {
            let x = rect.x + index as f32 * (bar_w + gap_x);
            let values = second.values();
            let iter: Box<dyn Iterator<Item = (usize, u16)>> = if graph.order_reverse != 0 {
                Box::new(values.into_iter().enumerate().rev())
            } else {
                Box::new(values.into_iter().enumerate())
            };
            let mut y_cursor = rect.y + rect.height;
            for (series, value) in iter {
                if value == 0 {
                    continue;
                }
                let height = (rect.height * (value as f32 / max_density) - gap_y).max(pixel_h);
                y_cursor -= height;
                items.push(SkinRenderItem::Rect {
                    rect: Rect { x, y: y_cursor, width: bar_w, height },
                    color: colors[series],
                    blend,
                });
                y_cursor -= gap_y;
                if y_cursor <= rect.y {
                    break;
                }
            }
        }

        items
    }

    fn bpmgraph_render_items(
        &self,
        graph: &SkinBpmGraphDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let segments = &self.play_bpm_graph_segments;
        if segments.is_empty() {
            return Vec::new();
        }
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let frame_alpha = frame.a as f32 / 255.0;
        let blend = if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal };
        let min_bpm = state.min_bpm.max(1.0);
        let max_bpm = state.max_bpm.max(min_bpm + 1.0);
        let line_width = graph.line_width.max(1) as f32;
        let main_color = skin_hex_color(&graph.main_bpm_color)
            .unwrap_or(Color::rgba(1.0, 0.4, 0.4, 0.9))
            .with_alpha(frame_alpha);
        let stop_color = skin_hex_color(&graph.stop_line_color)
            .unwrap_or(Color::rgba(1.0, 1.0, 1.0, 0.8))
            .with_alpha(frame_alpha);
        let other_color = skin_hex_color(&graph.other_bpm_color)
            .unwrap_or(Color::rgba(0.7, 0.7, 0.7, 0.8))
            .with_alpha(frame_alpha);
        let mut items = Vec::new();
        for segment in segments {
            let x0 = rect.x + segment.start_ratio.clamp(0.0, 1.0) * rect.width;
            let x1 = rect.x + segment.end_ratio.clamp(0.0, 1.0) * rect.width;
            if segment.is_stop {
                items.push(SkinRenderItem::Rect {
                    rect: Rect { x: x0, y: rect.y, width: line_width, height: rect.height },
                    color: stop_color,
                    blend,
                });
                continue;
            }
            let ratio = ((segment.bpm - min_bpm) / (max_bpm - min_bpm)).clamp(0.0, 1.0);
            let y = rect.y + rect.height * (1.0 - ratio) - line_width / 2.0;
            let color =
                if (segment.bpm - state.main_bpm).abs() < 0.5 { main_color } else { other_color };
            items.push(SkinRenderItem::Rect {
                rect: Rect { x: x0, y, width: (x1 - x0).max(line_width), height: line_width },
                color,
                blend,
            });
        }
        items
    }

    fn slider_render_item(
        &self,
        slider: &SkinSliderDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let progress = skin_slider_progress(slider, state)?;
        let source = sources.get(&slider.src)?;
        let source_width = source.source_size.width.max(1.0);
        let source_height = source.source_size.height.max(1.0);
        let mut frame = frame;
        let offset = (slider.range as f32 * progress).round() as i32;
        match slider.angle {
            0 => frame.y += offset,
            1 => frame.x += offset,
            2 => frame.y -= offset,
            3 => frame.x -= offset,
            _ => {}
        }
        let mut uv = TextureRegion {
            x: slider.x as f32 / source_width,
            y: slider.y as f32 / source_height,
            width: slider.w as f32 / source_width,
            height: slider.h as f32 / source_height,
        };
        if slider.slider_type == 4
            && let Some((disappear_line, link_lift)) = self.disappear_line_for_lane_cover_clip()
        {
            clip_skin_cover_to_disappear_line(
                &mut frame,
                &mut uv,
                disappear_line,
                link_lift,
                state,
            );
            if frame.h <= 0 {
                return None;
            }
        }
        let (rect, uv) = stretch_skin_image_geometry(
            destination.stretch,
            normalize_skin_frame_rect(frame, self.w, self.h),
            uv,
            source.source_size,
            self.w,
            self.h,
        );
        Some(SkinRenderItem::Image {
            texture: source.texture,
            rect,
            uv,
            tint: Color::rgba(
                frame.r as f32 / 255.0,
                frame.g as f32 / 255.0,
                frame.b as f32 / 255.0,
                frame.a as f32 / 255.0,
            ),
            blend: if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: Some(source.source_size),
            linear_filter: destination.filter != 0,
        })
    }

    fn hidden_cover_render_item(
        &self,
        cover: &SkinHiddenCoverDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        if state.hidden_cover <= 0.0 {
            return None;
        }
        let source = sources.get(&cover.src)?;
        let source_width = source.source_size.width.max(1.0);
        let source_height = source.source_size.height.max(1.0);
        let mut frame = frame;
        let mut uv = TextureRegion {
            x: cover.x as f32 / source_width,
            y: cover.y as f32 / source_height,
            width: cover.w as f32 / source_width,
            height: cover.h as f32 / source_height,
        };
        clip_skin_cover_to_disappear_line(
            &mut frame,
            &mut uv,
            cover.disappear_line,
            cover.is_disappear_line_link_lift,
            state,
        );
        if frame.h <= 0 {
            return None;
        }
        let (rect, uv) = stretch_skin_image_geometry(
            destination.stretch,
            normalize_skin_frame_rect(frame, self.w, self.h),
            uv,
            source.source_size,
            self.w,
            self.h,
        );
        Some(SkinRenderItem::Image {
            texture: source.texture,
            rect,
            uv,
            tint: Color::rgba(
                frame.r as f32 / 255.0,
                frame.g as f32 / 255.0,
                frame.b as f32 / 255.0,
                frame.a as f32 / 255.0,
            ),
            blend: if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: Some(source.source_size),
            linear_filter: destination.filter != 0,
        })
    }

    fn graph_render_item(
        &self,
        graph: &SkinGraphDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let source = sources.get(&graph.src)?;
        let value = graph_fill_ratio(graph, state).clamp(0.0, 1.0);
        let source_w = source.source_size.width.max(1.0);
        let source_h = source.source_size.height.max(1.0);
        let base_uv = TextureRegion {
            x: graph.x as f32 / source_w,
            y: graph.y as f32 / source_h,
            width: graph.w as f32 / source_w,
            height: graph.h as f32 / source_h,
        };
        let dst = normalize_skin_frame_rect(frame, self.w, self.h);
        let (rect, uv) = if graph.angle == 1 {
            // vertical: fill from bottom up
            let clipped_h = dst.height * value;
            let uv_offset = base_uv.height * (1.0 - value);
            (
                Rect { y: dst.y + dst.height - clipped_h, height: clipped_h, ..dst },
                TextureRegion {
                    y: base_uv.y + uv_offset,
                    height: base_uv.height * value,
                    ..base_uv
                },
            )
        } else {
            // horizontal: fill from left
            (
                Rect { width: dst.width * value, ..dst },
                TextureRegion { width: base_uv.width * value, ..base_uv },
            )
        };
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return None;
        }
        Some(SkinRenderItem::Image {
            texture: source.texture,
            rect,
            uv,
            tint: Color::rgba(
                frame.r as f32 / 255.0,
                frame.g as f32 / 255.0,
                frame.b as f32 / 255.0,
                frame.a as f32 / 255.0,
            ),
            blend: BlendMode::Normal,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: Some(source.source_size),
            linear_filter: false,
        })
    }
}

/// beatoraja スキンの `note` 配列インデックスをキーモードに応じて返す。
/// スキン側の並び順: 1P [Key1..KeyN, Scratch], 2P [Key(N+1)..Key(2N), Scratch2]
fn beatoraja_note_index(lane: Lane, key_mode: KeyMode) -> usize {
    match key_mode {
        KeyMode::K5 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            _ => 5, // Scratch
        },
        KeyMode::K7 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            _ => 7, // Scratch
        },
        KeyMode::K10 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Scratch => 5,
            Lane::Key8 => 6,
            Lane::Key9 => 7,
            Lane::Key10 => 8,
            Lane::Key11 => 9,
            Lane::Key12 => 10,
            _ => 11, // Scratch2
        },
        KeyMode::K14 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            Lane::Scratch => 7,
            Lane::Key8 => 8,
            Lane::Key9 => 9,
            Lane::Key10 => 10,
            Lane::Key11 => 11,
            Lane::Key12 => 12,
            Lane::Key13 => 13,
            Lane::Key14 => 14,
            _ => 15, // Scratch2
        },
        KeyMode::K9 => match lane {
            Lane::Key1 => 0,
            Lane::Key2 => 1,
            Lane::Key3 => 2,
            Lane::Key4 => 3,
            Lane::Key5 => 4,
            Lane::Key6 => 5,
            Lane::Key7 => 6,
            Lane::Key8 => 7,
            Lane::Key9 => 8,
            _ => 8,
        },
        // Qwilight 系は 7K スキンへフォールバック描画。
        KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => beatoraja_note_index(lane, KeyMode::K7),
    }
}

fn imageset_ref_lane(ref_id: i32) -> Option<Lane> {
    match ref_id {
        500 => Some(Lane::Scratch),
        501 => Some(Lane::Key1),
        502 => Some(Lane::Key2),
        503 => Some(Lane::Key3),
        504 => Some(Lane::Key4),
        505 => Some(Lane::Key5),
        506 => Some(Lane::Key6),
        507 => Some(Lane::Key7),
        _ => None,
    }
}

fn skin_state_imageset_index(ref_id: i32, state: SkinDrawState) -> Option<usize> {
    match ref_id {
        40 => Some(state.select_gauge_index),
        SKIN_REF_PLAY_GAUGE_TYPE => Some(state.gauge_type.max(0) as usize),
        41 => Some(state.select_target_index),
        42 | 43 => Some(state.select_arrange_index),
        54 | 55 => Some(0),
        72 => Some(state.select_bga_index),
        78 => Some(state.select_gauge_auto_shift_index),
        11 => Some(state.select_mode_index),
        12 => Some(state.select_sort_index),
        301..=307 => Some(0),
        308 => Some(state.select_ln_mode_index),
        _ => None,
    }
}

/// imageset の画像を判定インデックス (0=PGREAT..4=POOR,5=MISS) で選ぶ。
/// 2枚構成 (通常/PGREAT) は PGREAT 判定でのみ2枚目を使う。
fn imageset_image_for_index(
    imageset: &SkinImageSetDef,
    judge_index: Option<usize>,
) -> Option<String> {
    let len = imageset.images.len();
    if len == 0 {
        return None;
    }
    let index = if len == 2 {
        usize::from(judge_index == Some(0))
    } else {
        judge_index.unwrap_or(0).min(len - 1)
    };
    imageset.images.get(index).cloned()
}

pub(crate) fn judge_image_index(judge: &str) -> Option<usize> {
    let judge = judge.trim();
    if judge.starts_with("PGREAT") {
        Some(0)
    } else if judge.starts_with("GREAT") {
        Some(1)
    } else if judge.starts_with("GOOD") {
        Some(2)
    } else if judge.starts_with("BAD") {
        Some(3)
    } else if judge.starts_with("POOR") {
        Some(4)
    } else if judge.starts_with("EMPTY") {
        Some(5)
    } else {
        None
    }
}

pub(crate) fn judge_image_index_for_judge(judge: Judge) -> usize {
    match judge {
        Judge::PGreat => 0,
        Judge::Great => 1,
        Judge::Good => 2,
        Judge::Bad => 3,
        Judge::Poor => 4,
        Judge::EmptyPoor => 5,
    }
}

impl SkinManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read skin manifest: {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("failed to parse skin manifest: {}", path.display()))
    }

    pub fn resolve_textures(&self, base_dir: &Path) -> Vec<ResolvedSkinTexture> {
        self.textures
            .iter()
            .map(|texture| {
                let path = Path::new(&texture.path);
                let path =
                    if path.is_absolute() { path.to_path_buf() } else { base_dir.join(path) };
                ResolvedSkinTexture { id: TextureId(texture.id), path }
            })
            .collect()
    }

    pub fn with_texture_source_sizes(mut self, base_dir: &Path) -> Self {
        let sizes = self.texture_source_sizes(base_dir);
        fill_image_source_size(&mut self.play.note, &sizes);
        fill_image_source_size(&mut self.play.receptor, &sizes);
        fill_image_source_size(&mut self.play.judge_line, &sizes);
        fill_image_source_size(&mut self.play.gauge_frame, &sizes);
        fill_image_source_size(&mut self.play.gauge_fill, &sizes);
        fill_image_source_size(&mut self.play.combo_panel, &sizes);
        fill_image_source_size(&mut self.play.combo_panel_inactive, &sizes);
        self
    }

    fn texture_source_sizes(&self, base_dir: &Path) -> HashMap<u32, SkinImageSize> {
        self.resolve_textures(base_dir)
            .into_iter()
            .filter_map(|texture| {
                let asset = load_png_rgba(&texture.path).ok()?;
                Some((
                    texture.id.0,
                    SkinImageSize { width: asset.width as f32, height: asset.height as f32 },
                ))
            })
            .collect()
    }

    pub fn play_note_image(&self) -> SkinImageManifest {
        self.play.note.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_NOTE_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_receptor_image(&self) -> SkinImageManifest {
        self.play.receptor.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_RECEPTOR_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_judge_line_image(&self) -> SkinImageManifest {
        self.play.judge_line.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_JUDGE_LINE_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_gauge_frame_image(&self) -> SkinImageManifest {
        self.play.gauge_frame.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_GAUGE_FRAME_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_gauge_fill_image(&self) -> SkinImageManifest {
        self.play.gauge_fill.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_GAUGE_FILL_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_combo_panel_image(&self, active: bool) -> SkinImageManifest {
        if active { self.play.combo_panel } else { self.play.combo_panel_inactive }.unwrap_or(
            SkinImageManifest {
                texture: if active {
                    crate::plan::DEFAULT_COMBO_PANEL_TEXTURE.0
                } else {
                    crate::plan::DEFAULT_COMBO_PANEL_INACTIVE_TEXTURE.0
                },
                key_even_texture: None,
                scratch_texture: None,
                source_size: None,
                uv: TextureRegion::default(),
                scale: SkinImageScale::Stretch,
                border: None,
            },
        )
    }
}

fn load_json_value(path: &Path) -> Result<JsonValue> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read skin json: {}", path.display()))?;
    let text = strip_json_trailing_commas(&text);
    let text = insert_missing_commas_between_json_values(&text);
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse skin json: {}", path.display()))
}

fn strip_json_trailing_commas(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while matches!(lookahead.peek(), Some(next) if next.is_whitespace()) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some(']' | '}')) {
                continue;
            }
        }

        output.push(ch);
    }

    output
}

fn insert_missing_commas_between_json_values(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        output.push(ch);
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        if ch != '}' && ch != ']' {
            continue;
        }

        let mut lookahead = chars.clone();
        let mut whitespace = String::new();
        while let Some(next) = lookahead.peek().copied() {
            if next.is_whitespace() {
                whitespace.push(next);
                lookahead.next();
            } else {
                break;
            }
        }
        if matches!(lookahead.peek(), Some('{') | Some('[')) {
            output.push(',');
        }
    }

    output
}

fn normalize_json_skin_integer_numbers(value: JsonValue) -> JsonValue {
    normalize_json_skin_integer_numbers_for_key(None, value)
}

fn normalize_json_skin_integer_numbers_for_key(key: Option<&str>, value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(values) => JsonValue::Array(
            values
                .into_iter()
                .map(|value| {
                    if is_json_skin_integer_key(key) {
                        normalize_json_skin_integer_value(value)
                    } else {
                        normalize_json_skin_integer_numbers_for_key(key, value)
                    }
                })
                .collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = if is_json_skin_integer_key(Some(&key)) {
                        normalize_json_skin_integer_value(value)
                    } else {
                        normalize_json_skin_integer_numbers_for_key(Some(&key), value)
                    };
                    (key, value)
                })
                .collect::<JsonMap<_, _>>(),
        ),
        JsonValue::Number(number) if is_json_skin_integer_key(key) => {
            json_number_to_rounded_i64(&number)
                .and_then(serde_json::Number::from_i128)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Number(number))
        }
        value => value,
    }
}

fn normalize_json_skin_integer_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Number(number) => json_number_to_rounded_i64(&number)
            .and_then(serde_json::Number::from_i128)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Number(number)),
        JsonValue::Array(values) => {
            JsonValue::Array(values.into_iter().map(normalize_json_skin_integer_value).collect())
        }
        JsonValue::Object(map) => JsonValue::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = if is_json_skin_integer_key(Some(&key)) {
                        normalize_json_skin_integer_value(value)
                    } else {
                        normalize_json_skin_integer_numbers_for_key(Some(&key), value)
                    };
                    (key, value)
                })
                .collect::<JsonMap<_, _>>(),
        ),
        value => value,
    }
}

fn json_number_to_rounded_i64(number: &serde_json::Number) -> Option<i128> {
    if let Some(value) = number.as_i64() {
        return Some(value as i128);
    }
    if let Some(value) = number.as_u64() {
        return Some(value as i128);
    }
    let value = number.as_f64()?;
    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
        return None;
    }
    Some(value.round() as i128)
}

fn is_json_skin_integer_key(key: Option<&str>) -> bool {
    matches!(
        key,
        Some(
            "a" | "acc"
                | "align"
                | "angle"
                | "b"
                | "blend"
                | "center"
                | "click"
                | "cycle"
                | "digit"
                | "disapearLine"
                | "divx"
                | "divy"
                | "endtime"
                | "filter"
                | "g"
                | "h"
                | "index"
                | "len"
                | "loop"
                | "max"
                | "min"
                | "offset"
                | "offsets"
                | "op"
                | "padding"
                | "parts"
                | "r"
                | "range"
                | "ref"
                | "size"
                | "space"
                | "starttime"
                | "stretch"
                | "time"
                | "timer"
                | "type"
                | "w"
                | "x"
                | "y"
                | "zeropadding"
        )
    )
}

fn expand_json_skin_value(
    value: JsonValue,
    current_dir: &Path,
    root_dir: &Path,
    enabled_options: &[i32],
) -> Result<JsonValue> {
    match value {
        JsonValue::Array(items) => {
            let mut expanded = Vec::new();
            for item in items {
                if let JsonValue::Object(object) = &item {
                    if let Some(include) = object.get("include") {
                        let included = load_included_json(include, current_dir, root_dir)?;
                        let included_dir = included.parent().unwrap_or(current_dir);
                        let included_value = expand_json_skin_value(
                            load_json_value(&included)?,
                            included_dir,
                            root_dir,
                            enabled_options,
                        )?;
                        match included_value {
                            JsonValue::Array(values) => expanded.extend(values),
                            other => expanded.push(other),
                        }
                        continue;
                    }
                    if object.contains_key("if")
                        && (object.contains_key("value") || object.contains_key("values"))
                    {
                        if test_json_option(object.get("if"), enabled_options) {
                            if let Some(value) = object.get("value") {
                                expanded.push(expand_json_skin_value(
                                    value.clone(),
                                    current_dir,
                                    root_dir,
                                    enabled_options,
                                )?);
                            }
                            if let Some(values) = object.get("values") {
                                let values = expand_json_skin_value(
                                    values.clone(),
                                    current_dir,
                                    root_dir,
                                    enabled_options,
                                )?;
                                match values {
                                    JsonValue::Array(values) => expanded.extend(values),
                                    other => expanded.push(other),
                                }
                            }
                        }
                        continue;
                    }
                }
                expanded.push(expand_json_skin_value(
                    item,
                    current_dir,
                    root_dir,
                    enabled_options,
                )?);
            }
            Ok(JsonValue::Array(expanded))
        }
        JsonValue::Object(mut object) => {
            if let Some(include) = object.get("include") {
                let included = load_included_json(include, current_dir, root_dir)?;
                let included_dir = included.parent().unwrap_or(current_dir);
                return expand_json_skin_value(
                    load_json_value(&included)?,
                    included_dir,
                    root_dir,
                    enabled_options,
                );
            }
            if object.contains_key("if") && object.contains_key("value") {
                return if test_json_option(object.get("if"), enabled_options) {
                    expand_json_skin_value(
                        object.remove("value").unwrap_or(JsonValue::Null),
                        current_dir,
                        root_dir,
                        enabled_options,
                    )
                } else {
                    Ok(JsonValue::Null)
                };
            }
            let mut expanded = JsonMap::new();
            for (key, value) in object {
                expanded.insert(
                    key,
                    expand_json_skin_value(value, current_dir, root_dir, enabled_options)?,
                );
            }
            Ok(JsonValue::Object(expanded))
        }
        other => Ok(other),
    }
}

fn load_included_json(include: &JsonValue, current_dir: &Path, root_dir: &Path) -> Result<PathBuf> {
    let include =
        include.as_str().ok_or_else(|| anyhow::anyhow!("skin json include must be a string"))?;
    let path = current_dir.join(include);
    let canonical_root = root_dir
        .canonicalize()
        .with_context(|| format!("failed to canonicalize skin root: {}", root_dir.display()))?;
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize skin include: {}", path.display()))?;
    anyhow::ensure!(
        canonical_path.starts_with(&canonical_root),
        "skin include escapes skin root: {}",
        path.display()
    );
    Ok(canonical_path)
}

fn test_json_option(option: Option<&JsonValue>, enabled_options: &[i32]) -> bool {
    let Some(option) = option else {
        return true;
    };
    match option {
        JsonValue::Number(number) => number.as_i64().is_some_and(|value| {
            test_json_option_number(i32::try_from(value).unwrap_or(i32::MIN), enabled_options)
        }),
        JsonValue::Array(values) => values.iter().all(|value| match value {
            JsonValue::Number(number) => number.as_i64().is_some_and(|value| {
                test_json_option_number(i32::try_from(value).unwrap_or(i32::MIN), enabled_options)
            }),
            JsonValue::Array(or_values) => or_values.iter().any(|or_value| {
                let JsonValue::Number(number) = or_value else {
                    return false;
                };
                number.as_i64().is_some_and(|value| {
                    test_json_option_number(
                        i32::try_from(value).unwrap_or(i32::MIN),
                        enabled_options,
                    )
                })
            }),
            _ => false,
        }),
        _ => false,
    }
}

fn test_json_option_number(option: i32, enabled_options: &[i32]) -> bool {
    if option >= 0 {
        enabled_options.contains(&option)
    } else {
        !enabled_options.contains(&-option)
    }
}

pub(crate) fn test_skin_ops(ops: &[i32], enabled_options: &[i32], state: SkinDrawState) -> bool {
    ops.iter().all(|op| test_skin_op(*op, enabled_options, state))
}

fn test_skin_op(op: i32, enabled_options: &[i32], state: SkinDrawState) -> bool {
    if op < 0 {
        return op
            .checked_neg()
            .is_some_and(|positive| !test_skin_op(positive, enabled_options, state));
    }
    match op {
        40 => false,
        41 => true,
        1 => matches!(
            state.select_row_kind,
            SelectRowKind::Folder | SelectRowKind::TableFolder | SelectRowKind::SettingsFolder
        ),
        2 => select_song_detail_row(state),
        3 => state.select_row_kind == SelectRowKind::Course,
        1002..=1017 => gradebar_constraint_op_matches(op, state),
        5 => {
            !state.in_settings
                && !state.select_is_folder
                && state.select_in_library
                && state.select_row_kind == SelectRowKind::Song
        }
        // BMZ currently has no IR backend, matching beatoraja's offline state.
        50 => true,
        51 => false,
        21 => state.select_option_panel == 1,
        22 => state.select_option_panel == 2,
        23 => state.select_option_panel == 3,
        160..=164 => select_key_mode_option_matches(op, state),
        1160 | 1161 => select_key_mode_option_matches(op, state),
        196 | 197 | 198 | 1196..=1208 if state.result_failed.is_some() => {
            result_replay_op_matches(op, state)
        }
        196 | 197 | 198 | 1196..=1208 => select_replay_op_matches(op, state),
        200..=207 => select_rank_op_matches(op, state),
        300..=318 if state.result_failed.is_some() => result_rank_op_matches(op, state),
        300..=307 => select_small_rank_op_matches(op, state),
        320..=327 => best_rank_op_matches(op, state),
        170 => !state.has_bga,
        171 => state.has_bga,
        // OPTION_NOW_LOADING / OPTION_LOADED
        80 => !state.skin_loaded,
        81 => state.skin_loaded,
        // OPTION_NO_STAGEFILE / OPTION_STAGEFILE
        190 => !state.has_stagefile,
        191 => state.has_stagefile,
        // OPTION_NO_BANNER / OPTION_BANNER (192/193)
        192 => select_banner_option_matches(false, state),
        193 => select_banner_option_matches(true, state),
        // OPTION_NO_BACKBMP / OPTION_BACKBMP
        194 => !state.has_backbmp,
        195 => state.has_backbmp,
        // OPTION_LANECOVER1_CHANGING / OPTION_LANECOVER1_ON / OPTION_LIFT1_ON / OPTION_HIDDEN1_ON
        270 => state.lane_cover_changing,
        271 => state.lanecover_enabled,
        272 => state.lift_enabled,
        273 => state.hidden_enabled,
        // Result/update comparison options. In play skins these are often reused
        // as target-reached draw conditions.
        330 => state.previous_best_ex_score.is_some_and(|best| state.ex_score > best),
        1330 => state.previous_best_ex_score.is_some_and(|best| state.ex_score == best),
        331 => state.previous_best_max_combo.is_some_and(|best| state.max_combo > best),
        1331 => state.previous_best_max_combo.is_some_and(|best| state.max_combo == best),
        332 => state.previous_best_bp.is_some_and(|best| current_bp(state) < best),
        1332 => state.previous_best_bp.is_some_and(|best| current_bp(state) == best),
        335 => state.previous_best_ex_score.is_some_and(|best| {
            score_rate_cmp_value(state.ex_score, state.total_notes)
                > score_rate_cmp_value(best, state.total_notes)
        }),
        1335 => state.previous_best_ex_score.is_some_and(|best| {
            score_rate_cmp_value(state.ex_score, state.total_notes)
                == score_rate_cmp_value(best, state.total_notes)
        }),
        336 => state.target_ex_score.is_some_and(|target| state.ex_score > target),
        1336 => state.target_ex_score.is_some_and(|target| state.ex_score == target),
        350 => true,
        351 => false,
        352 => state.target_ex_score.is_some_and(|target| state.ex_score > target),
        353 => state.target_ex_score.is_some_and(|target| state.ex_score < target),
        354 => state.target_ex_score.is_some_and(|target| state.ex_score == target),
        // OPTION_GAUGE_GROOVE / OPTION_GAUGE_HARD / OPTION_GAUGE_EX.
        // beatoraja uses the current gauge type index: 0..2 are groove-family,
        // 3+ are hard-family, and 1046 is true for assist/easy/ex variants.
        42 => state.gauge_type <= 2,
        43 => state.gauge_type >= 3,
        1046 => matches!(state.gauge_type, 0 | 1 | 4 | 5 | 7 | 8),
        601..=608 => false,
        // OPTION_DIFFICULTY0..5. 0 は UNKNOWN/OTHER、1..5 は BMS #DIFFICULTY。
        150 => state.difficulty <= 0 || state.difficulty > 5,
        151..=155 => state.difficulty == i64::from(op - 150),
        // OPTION_JUDGE_VERYHARD..VERYEASY (180..184)
        180..=184 => {
            !(state.select_screen && state.in_settings)
                && judge_rank_option_matches(op, state.judge_rank)
        }
        // OPTION_RESULT_CLEAR=90, OPTION_RESULT_FAIL=91
        // Result 画面以外 (result_failed == None) では両方 false。
        90 => state.result_failed == Some(false),
        91 => state.result_failed == Some(true),
        // OPTION_AUTOPLAYOFF / OPTION_AUTOPLAYON
        32 => !state.autoplay,
        33 => state.autoplay,
        // OPTION_1P/2P/3P_PERFECT and EARLY/LATE judge-detail conditions.
        // beatoraja maps FAST/EARLY to positive recent judge timing, LATE/SLOW to negative.
        241 => state.judge_index[0] == Some(0),
        1242 => {
            state.judge_index[0].is_some_and(|index| index > 0)
                && state.judge_timing_sign[0] == Some(1)
        }
        1243 => {
            state.judge_index[0].is_some_and(|index| index > 0)
                && state.judge_timing_sign[0] == Some(-1)
        }
        261 => state.judge_index[1] == Some(0),
        1262 => {
            state.judge_index[1].is_some_and(|index| index > 0)
                && state.judge_timing_sign[1] == Some(1)
        }
        1263 => {
            state.judge_index[1].is_some_and(|index| index > 0)
                && state.judge_timing_sign[1] == Some(-1)
        }
        361 => state.judge_index[2] == Some(0),
        1362 => {
            state.judge_index[2].is_some_and(|index| index > 0)
                && state.judge_timing_sign[2] == Some(1)
        }
        1363 => {
            state.judge_index[2].is_some_and(|index| index > 0)
                && state.judge_timing_sign[2] == Some(-1)
        }
        // OPTION_COURSE_STAGE1..4 / OPTION_COURSE_STAGE_FINAL
        280 => state.course_stage == Some(CourseStageMarker::Stage1),
        281 => state.course_stage == Some(CourseStageMarker::Stage2),
        282 => state.course_stage == Some(CourseStageMarker::Stage3),
        283 => state.course_stage == Some(CourseStageMarker::Stage4),
        289 => state.course_stage == Some(CourseStageMarker::Final),
        // OPTION_MODE_COURSE
        290 => state.course_stage.is_some(),
        // beatoraja defines OPTION_MODE_NONSTOP / EXPERT / GRADE (291..293)
        // but does not expose BooleanProperty handlers for them.  Return
        // false here instead of falling through to skin property defaults.
        291..=293 => false,
        value => test_json_option_number(value, enabled_options),
    }
}

fn gradebar_constraint_op_matches(op: i32, state: SkinDrawState) -> bool {
    if state.select_row_kind != SelectRowKind::Course {
        return false;
    }
    let constraints = state.select_course_constraints;
    match op {
        1002 => constraints.class,
        1003 => constraints.mirror,
        1004 => constraints.random,
        1005 => constraints.no_speed,
        1006 => constraints.no_good,
        1007 => constraints.no_great,
        1010 => constraints.gauge_lr2,
        1011 => constraints.gauge_5k,
        1012 => constraints.gauge_7k,
        1013 => constraints.gauge_9k,
        1014 => constraints.gauge_24k,
        1015 => constraints.ln,
        1016 => constraints.cn,
        1017 => constraints.hcn,
        _ => false,
    }
}

fn default_enabled_options(value: &JsonValue) -> Vec<i32> {
    let Some(properties) = value.get("property").and_then(JsonValue::as_array) else {
        return Vec::new();
    };
    properties.iter().filter_map(default_property_option).collect()
}

fn default_property_option(property: &JsonValue) -> Option<i32> {
    let items = property.get("item")?.as_array()?;
    let default_name = property.get("def").and_then(JsonValue::as_str).unwrap_or_default();
    if let Some(default_item) = items.iter().find(|item| {
        !default_name.is_empty()
            && item.get("name").and_then(JsonValue::as_str).is_some_and(|name| name == default_name)
    }) {
        return default_item
            .get("op")
            .and_then(JsonValue::as_i64)
            .and_then(|op| i32::try_from(op).ok());
    }
    items
        .first()
        .and_then(|item| item.get("op"))
        .and_then(JsonValue::as_i64)
        .and_then(|op| i32::try_from(op).ok())
}

pub fn default_skin_manifest() -> SkinManifest {
    static DEFAULT_SKIN_MANIFEST: OnceLock<SkinManifest> = OnceLock::new();
    DEFAULT_SKIN_MANIFEST
        .get_or_init(|| {
            let manifest: SkinManifest =
                toml::from_str(include_str!("../../../data/skins/default/skin.toml"))
                    .expect("bundled default skin manifest must parse");
            manifest.with_texture_source_sizes(&default_skin_root())
        })
        .clone()
}

fn default_skin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default")
}

fn fill_image_source_size(
    image: &mut Option<SkinImageManifest>,
    sizes: &HashMap<u32, SkinImageSize>,
) {
    let Some(image) = image else {
        return;
    };
    if image.source_size.is_none() {
        image.source_size = sizes.get(&image.texture).copied();
    }
}

pub fn append_skin_render_items(commands: &mut Vec<DrawCommand>, items: &[SkinRenderItem]) {
    for item in items {
        match item {
            SkinRenderItem::Rect { rect, color, .. } => {
                commands.push(DrawCommand::Rect { rect: *rect, color: *color });
            }
            SkinRenderItem::Text { origin, text, style, .. } => {
                if !text.is_empty() {
                    commands.push(DrawCommand::Text {
                        origin: *origin,
                        text: text.clone(),
                        style: style.clone(),
                    });
                }
            }
            SkinRenderItem::Image {
                texture,
                rect,
                uv,
                tint,
                blend,
                scale,
                border,
                source_size,
                linear_filter,
            } => {
                append_skin_image_command(
                    commands,
                    *texture,
                    *rect,
                    *uv,
                    *tint,
                    *blend,
                    *scale,
                    *border,
                    *source_size,
                    *linear_filter,
                );
            }
            SkinRenderItem::RotatedImage {
                texture,
                rect,
                uv,
                tint,
                blend,
                source_size: _,
                linear_filter,
                angle_deg,
                center,
            } => {
                commands.push(DrawCommand::RotatedImage {
                    rect: *rect,
                    uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
                    texture: TextureId(texture.0),
                    tint: *tint,
                    blend: *blend,
                    linear_filter: *linear_filter,
                    angle_rad: angle_deg.to_radians(),
                    center: *center,
                });
            }
        }
    }
}

fn append_skin_image_command(
    commands: &mut Vec<DrawCommand>,
    texture: SkinTextureId,
    rect: Rect,
    uv: TextureRegion,
    tint: Color,
    blend: BlendMode,
    scale: SkinImageScale,
    border: Option<SkinImageBorder>,
    source_size: Option<SkinImageSize>,
    linear_filter: bool,
) {
    match (scale, border) {
        (SkinImageScale::NineSlice, Some(border)) => {
            append_nine_slice_image_commands(
                commands,
                texture,
                rect,
                uv,
                tint,
                blend,
                border,
                source_size,
                linear_filter,
            );
        }
        _ => commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            texture: TextureId(texture.0),
            tint,
            blend,
            linear_filter,
        }),
    }
}

fn append_nine_slice_image_commands(
    commands: &mut Vec<DrawCommand>,
    texture: SkinTextureId,
    rect: Rect,
    uv: TextureRegion,
    tint: Color,
    blend: BlendMode,
    border: SkinImageBorder,
    source_size: Option<SkinImageSize>,
    linear_filter: bool,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || uv.width <= 0.0 || uv.height <= 0.0 {
        return;
    }

    let Some(border) = border.normalized(source_size) else {
        commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            texture: TextureId(texture.0),
            tint,
            blend,
            linear_filter,
        });
        return;
    };
    let left = border.left.clamp(0.0, 0.5);
    let right = border.right.clamp(0.0, 0.5);
    let top = border.top.clamp(0.0, 0.5);
    let bottom = border.bottom.clamp(0.0, 0.5);
    if left + right >= 1.0 || top + bottom >= 1.0 {
        commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            texture: TextureId(texture.0),
            tint,
            blend,
            linear_filter,
        });
        return;
    }

    let xs = [
        rect.x,
        rect.x + rect.width * left,
        rect.x + rect.width * (1.0 - right),
        rect.x + rect.width,
    ];
    let ys = [
        rect.y,
        rect.y + rect.height * top,
        rect.y + rect.height * (1.0 - bottom),
        rect.y + rect.height,
    ];
    let us = [uv.x, uv.x + uv.width * left, uv.x + uv.width * (1.0 - right), uv.x + uv.width];
    let vs = [uv.y, uv.y + uv.height * top, uv.y + uv.height * (1.0 - bottom), uv.y + uv.height];

    for row in 0..3 {
        for column in 0..3 {
            let piece = Rect {
                x: xs[column],
                y: ys[row],
                width: xs[column + 1] - xs[column],
                height: ys[row + 1] - ys[row],
            };
            let piece_uv = UvRect {
                x: us[column],
                y: vs[row],
                width: us[column + 1] - us[column],
                height: vs[row + 1] - vs[row],
            };
            if piece.width > 0.0
                && piece.height > 0.0
                && piece_uv.width > 0.0
                && piece_uv.height > 0.0
            {
                commands.push(DrawCommand::Image {
                    rect: piece,
                    uv: piece_uv,
                    texture: TextureId(texture.0),
                    tint,
                    blend,
                    linear_filter,
                });
            }
        }
    }
}

impl SkinPlacement {
    fn resolve(&self, elapsed_ms: i32) -> ResolvedPlacement {
        let Some(frame) = self.animation.sample(elapsed_ms) else {
            return ResolvedPlacement { rect: self.rect, alpha: self.alpha, blend: self.blend };
        };

        ResolvedPlacement { rect: frame.rect, alpha: self.alpha * frame.alpha, blend: self.blend }
    }
}

impl Animation {
    pub fn none() -> Self {
        Self { keyframes: Vec::new() }
    }

    fn sample(&self, elapsed_ms: i32) -> Option<Keyframe> {
        self.keyframes
            .iter()
            .filter(|frame| frame.time_ms <= elapsed_ms)
            .max_by_key(|frame| frame.time_ms)
            .copied()
    }
}

impl TextStyle {
    fn with_alpha(self, alpha: f32) -> Self {
        Self {
            color: self.color.with_alpha(self.color.a * alpha),
            outline: self.outline.map(|outline| TextOutline {
                color: outline.color.with_alpha(outline.color.a * alpha),
                ..outline
            }),
            shadow: self.shadow.map(|shadow| TextShadow {
                color: shadow.color.with_alpha(shadow.color.a * alpha),
                ..shadow
            }),
            ..self
        }
    }
}

impl Color {
    fn with_alpha(self, alpha: f32) -> Self {
        Self { a: alpha.clamp(0.0, 1.0), ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ResolvedPlacement {
    rect: Rect,
    alpha: f32,
    blend: BlendMode,
}

fn format_number(value: i64, digits: u8) -> String {
    if digits == 0 {
        value.to_string()
    } else {
        format!("{:0width$}", value.max(0), width = digits as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumberPadding {
    None,
    Zero,
    Blank,
}

impl NumberPadding {
    fn is_zero_padding(self) -> bool {
        matches!(self, Self::Zero)
    }
}

fn number_padding(value: &SkinValueDef) -> NumberPadding {
    if value.zeropadding == 2 || value.padding == 2 {
        return NumberPadding::Blank;
    }
    if value.zeropadding != 0 || value.padding != 0 {
        return NumberPadding::Zero;
    }
    let image_cells = value.divx.max(1).saturating_mul(value.divy.max(1));
    if !ref_id_is_signed(value.ref_id) && image_cells % 10 != 0 {
        return NumberPadding::Blank;
    }
    NumberPadding::None
}

fn display_number_digits(value: i64, max_digits: usize, padding: NumberPadding) -> Vec<u8> {
    let mut text = if padding.is_zero_padding() && max_digits > 0 {
        format!("{:0width$}", value.max(0), width = max_digits)
    } else {
        value.max(0).to_string()
    };
    if max_digits > 0 && text.len() > max_digits {
        text = text[text.len() - max_digits..].to_string();
    }
    let mut digits: Vec<u8> =
        text.bytes().filter(|byte| byte.is_ascii_digit()).map(|byte| byte - b'0').collect();
    if matches!(padding, NumberPadding::Blank) && max_digits > digits.len() {
        let mut padded = vec![10; max_digits - digits.len()];
        padded.extend(digits);
        digits = padded;
    }
    digits
}

/// 符号付き数値（beatoraja の mimage 慣習）用に、divx 列のテクスチャセル index を返す。
///
/// レイアウト (`divx`>=12, `divy`>=2):
/// - 各行は `[0,1,2,3,4,5,6,7,8,9, blank, sign]`
/// - 行0: 正数用 (sign cell = `+`)
/// - 行1: 負数用 (sign cell = `-`)
///
/// 返り値の各 byte は `digit_index % divx` が列、`digit_index / divx` が行になる。
/// 先頭要素は符号セル (index 11)、続けて絶対値の右寄せ桁が並ぶ。
fn display_signed_number_digits(
    value: i64,
    max_digits: usize,
    zero_pad: bool,
    divx: u32,
) -> Vec<u8> {
    if max_digits == 0 {
        return Vec::new();
    }
    let row_offset = if value < 0 { divx as u8 } else { 0 };
    let inner_width = max_digits.saturating_sub(1);
    let abs = value.unsigned_abs();
    let abs_text = if zero_pad && inner_width > 0 {
        format!("{:0width$}", abs, width = inner_width)
    } else {
        abs.to_string()
    };
    let trimmed: String = if inner_width > 0 && abs_text.len() > inner_width {
        abs_text[abs_text.len() - inner_width..].to_string()
    } else {
        abs_text
    };
    let mut digits = Vec::with_capacity(trimmed.len() + 1);
    // 先頭: 符号セル (sign image index = 11)
    digits.push(11u8 + row_offset);
    for byte in trimmed.bytes() {
        if byte.is_ascii_digit() {
            digits.push((byte - b'0') + row_offset);
        }
    }
    digits
}

/// `ref_id` が符号付き表示を要求する Result 系 ref か。
/// beatoraja の `NUMBER_DIFF_*` 系 (152, 153, 172, 175, 178) を対象とする。
fn ref_id_is_signed(ref_id: i32) -> bool {
    matches!(ref_id, 152 | 153 | 172 | 175 | 178)
}

fn lookup_text(values: &[(TextSlot, String)], slot: TextSlot) -> String {
    values
        .iter()
        .find(|(candidate, _)| *candidate == slot)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

fn lookup_number(values: &[(NumberSlot, i64)], slot: NumberSlot) -> i64 {
    values
        .iter()
        .find(|(candidate, _)| *candidate == slot)
        .map(|(_, value)| *value)
        .unwrap_or_default()
}

fn eval_skin_draw_condition(condition: &str, state: SkinDrawState) -> bool {
    let condition = condition.trim();
    if condition.is_empty() {
        return true;
    }

    condition.split("||").flat_map(|segment| segment.split(" or ")).any(|branch| {
        branch
            .split("&&")
            .flat_map(|segment| segment.split(" and "))
            .all(|term| eval_skin_draw_term(term.trim(), state).unwrap_or(false))
    })
}

fn eval_skin_draw_term(term: &str, state: SkinDrawState) -> Option<bool> {
    if let Some(option_id) = parse_skin_option_operand(term) {
        return Some(test_skin_op(option_id, &[], state));
    }
    if let Some(option_id) = term.strip_prefix('!').and_then(parse_skin_option_operand) {
        return Some(!test_skin_op(option_id, &[], state));
    }
    let operators = [">=", "<=", "==", "!=", ">", "<"];
    for operator in operators {
        let Some(index) = term.find(operator) else {
            continue;
        };
        let left = term[..index].trim();
        let right = term[index + operator.len()..].trim();
        let left = eval_skin_draw_operand(left, state)?;
        let right = eval_skin_draw_operand(right, state)?;
        return Some(match operator {
            ">=" => left >= right,
            "<=" => left <= right,
            "==" => (left - right).abs() < f32::EPSILON,
            "!=" => (left - right).abs() >= f32::EPSILON,
            ">" => left > right,
            "<" => left < right,
            _ => false,
        });
    }
    None
}

fn eval_skin_draw_operand(operand: &str, state: SkinDrawState) -> Option<f32> {
    if let Some(ref_id) = parse_skin_float_number_operand(operand) {
        return skin_state_float_number(ref_id, state);
    }
    if let Some(event_id) = parse_skin_event_index_operand(operand) {
        return Some(skin_state_event_index(event_id, state) as f32);
    }
    if let Some(ref_id) = parse_skin_number_operand(operand) {
        return skin_state_number(ref_id, state).map(|value| value as f32);
    }
    if let Some(timer_id) = parse_skin_timer_operand(operand) {
        return Some(skin_timer_elapsed_ms(Some(timer_id), state).unwrap_or(i32::MIN) as f32);
    }
    match operand {
        "gauge()" | "gauge" => Some(state.gauge),
        "gauge_type()" | "gauge_type" => Some(state.gauge_type as f32),
        "gauge_auto_shift()" | "gauge_auto_shift" => {
            Some(if state.gauge_auto_shift { 1.0 } else { 0.0 })
        }
        "gauge_auto_shift_mode()" | "gauge_auto_shift_mode" => {
            Some(state.select_gauge_auto_shift_index as f32)
        }
        "timer_off" | "timer_off_value" => Some(i32::MIN as f32),
        value => value.parse::<f32>().ok(),
    }
}

fn parse_skin_number_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("number(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn parse_skin_float_number_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("float_number(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn parse_skin_event_index_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("event_index(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn skin_state_event_index(event_id: i32, state: SkinDrawState) -> i32 {
    match event_id {
        SKIN_EVENT_HSFIX => state.hsfix_index,
        _ => 0,
    }
}

/// beatoraja `main_state.float_number(ref)`。BARGRAPH / SLIDER 系の比率 0.0-1.0。
fn skin_state_float_number(ref_id: i32, state: SkinDrawState) -> Option<f32> {
    Some(match ref_id {
        14 => state.lane_cover.clamp(0.0, 1.0),
        _ => graph_value(ref_id, state),
    })
}

fn parse_skin_option_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("option(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn parse_skin_timer_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("timer(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn skin_builtin_value_f32(expr: &str, state: SkinDrawState) -> Option<f32> {
    match expr.trim() {
        SKIN_EXPR_ADJUSTED_COVER => state.adjusted_cover_progress,
        SKIN_EXPR_ADJUSTED_RATE => state.adjusted_rate,
        SKIN_EXPR_ADJUSTED_RATE_ADOT => state.adjusted_rate_adot.map(|value| value as f32),
        SKIN_EXPR_FS_THRESHOLD => Some(state.fs_threshold_ms as f32),
        _ => None,
    }
}

fn skin_value_number(value: &SkinValueDef, state: SkinDrawState) -> Option<i64> {
    if !value.expr.trim().is_empty() {
        return skin_state_number_expr(&value.expr, state);
    }
    if !value.value_expr.trim().is_empty() {
        if let Some(number) = skin_builtin_value_f32(&value.value_expr, state) {
            return Some(number.round() as i64);
        }
        return skin_state_digit_float_expr(&value.value_expr, state)
            .map(|value| value.round() as i64);
    }
    skin_state_number(value.ref_id, state)
}

fn skin_state_digit_float_expr(expr: &str, state: SkinDrawState) -> Option<f32> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }
    if let Some((left, right)) = expr.split_once('*') {
        let left = skin_state_digit_float_expr(left.trim(), state)?;
        let right = skin_state_digit_float_expr(right.trim(), state)?;
        return Some(left * right);
    }
    if let Some((numerator, denominator)) = expr.split_once('/') {
        let numerator = skin_state_additive_float_expr(numerator.trim(), state)?;
        let denominator = skin_state_additive_float_expr(denominator.trim(), state)?;
        if denominator.abs() < f32::EPSILON {
            return Some(0.0);
        }
        return Some(numerator / denominator);
    }
    skin_state_additive_float_expr(expr, state)
}

fn skin_value_number_or_songlist_level(value: &SkinValueDef, state: SkinDrawState) -> Option<i64> {
    if value.ref_id == 0 && value.expr.trim().is_empty() {
        return Some(if state.play_level != 0 {
            state.play_level
        } else {
            state.select_play_level
        });
    }
    skin_value_number(value, state)
}

fn skin_state_number_expr(expr: &str, state: SkinDrawState) -> Option<i64> {
    let normalized = expr.replace('+', " + ").replace('-', " - ");
    let mut sign = 1_i64;
    let mut total = 0_i64;
    let mut expecting_value = true;
    for token in normalized.split_whitespace() {
        match token {
            "+" if expecting_value => sign = 1,
            "-" if expecting_value => sign = -1,
            "+" if !expecting_value => {
                sign = 1;
                expecting_value = true;
            }
            "-" if !expecting_value => {
                sign = -1;
                expecting_value = true;
            }
            value => {
                if !expecting_value {
                    return None;
                }
                let term = skin_state_number_expr_term(value, state)?;
                total += sign * term;
                sign = 1;
                expecting_value = false;
            }
        }
    }
    if expecting_value {
        return None;
    }
    Some(total)
}

fn skin_state_number_expr_term(term: &str, state: SkinDrawState) -> Option<i64> {
    if let Some(ref_id) = parse_skin_number_operand(term) {
        return skin_state_number(ref_id, state);
    }
    if let Some((coefficient, operand)) = term.split_once('*') {
        let coefficient = coefficient.parse::<i64>().ok()?;
        let ref_id = parse_skin_number_operand(operand.trim())?;
        return skin_state_number(ref_id, state).map(|value| coefficient * value);
    }
    term.parse::<i64>().ok()
}

fn skin_state_float_expr(expr: &str, state: SkinDrawState) -> Option<f32> {
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }
    if let Some((numerator, denominator)) = expr.split_once('/') {
        let numerator = skin_state_additive_float_expr(numerator.trim(), state)?;
        let denominator = skin_state_additive_float_expr(denominator.trim(), state)?;
        if denominator.abs() < f32::EPSILON {
            return Some(0.0);
        }
        return Some((numerator / denominator).clamp(0.0, 1.0));
    }
    skin_state_additive_float_expr(expr, state)
}

fn skin_state_additive_float_expr(expr: &str, state: SkinDrawState) -> Option<f32> {
    let normalized = expr.replace('+', " + ").replace('-', " - ");
    let mut sign = 1.0_f32;
    let mut total = 0.0_f32;
    let mut expecting_value = true;
    for token in normalized.split_whitespace() {
        match token {
            "+" if expecting_value => sign = 1.0,
            "-" if expecting_value => sign = -1.0,
            "+" if !expecting_value => {
                sign = 1.0;
                expecting_value = true;
            }
            "-" if !expecting_value => {
                sign = -1.0;
                expecting_value = true;
            }
            value => {
                if !expecting_value {
                    return None;
                }
                let term = skin_state_float_expr_term(value, state)?;
                total += sign * term;
                sign = 1.0;
                expecting_value = false;
            }
        }
    }
    if expecting_value {
        return None;
    }
    Some(total)
}

fn skin_state_float_expr_term(term: &str, state: SkinDrawState) -> Option<f32> {
    if let Some(ref_id) = parse_skin_float_number_operand(term) {
        return skin_state_float_number(ref_id, state);
    }
    if let Some(event_id) = parse_skin_event_index_operand(term) {
        return Some(skin_state_event_index(event_id, state) as f32);
    }
    if let Some(ref_id) = parse_skin_number_operand(term) {
        return skin_state_number(ref_id, state).map(|value| value as f32);
    }
    if term.contains('*') {
        return skin_state_float_product_expr_term(term, state);
    }
    term.parse::<f32>().ok()
}

fn skin_state_float_product_expr_term(term: &str, state: SkinDrawState) -> Option<f32> {
    let mut product = 1.0_f32;
    for factor in term.split('*') {
        let factor = factor.trim();
        if let Some(ref_id) = parse_skin_float_number_operand(factor) {
            product *= skin_state_float_number(ref_id, state)?;
        } else if let Some(ref_id) = parse_skin_number_operand(factor) {
            product *= skin_state_number(ref_id, state)? as f32;
        } else if let Some(option_id) = parse_skin_option_operand(factor) {
            product *= if test_skin_op(option_id, &[], state) { 1.0 } else { 0.0 };
        } else {
            product *= factor.parse::<f32>().ok()?;
        }
    }
    Some(product)
}

/// 設定フォルダ内で曲メタデータ用 number を出さない ref。
fn select_settings_screen_number_hidden(ref_id: i32) -> bool {
    matches!(
        ref_id,
        30 | 71 | 72 | 74 | 77 | 78 | 90 | 91 | 92 | 1163 | 1164 | 121 | 150 | 170 | 350 | 370
    )
}

fn select_volume_number(volume: f32) -> i64 {
    (volume.clamp(0.0, 1.0) * 100.0 + 0.0001) as i64
}

fn select_settings_screen_number(ref_id: i32, state: SkinDrawState) -> Option<i64> {
    match ref_id {
        96 if state.select_row_kind == SelectRowKind::Config => {
            Some(if state.play_level != 0 { state.play_level } else { state.select_play_level })
        }
        57 => Some(select_volume_number(state.select_master_volume)),
        58 => Some(select_volume_number(state.select_key_volume)),
        59 => Some(select_volume_number(state.select_bgm_volume)),
        12 => Some(state.judge_timing_offset_ms as i64),
        _ => None,
    }
}

fn skin_state_number(ref_id: i32, state: SkinDrawState) -> Option<i64> {
    if state.select_screen && state.in_settings {
        if let Some(value) = select_settings_screen_number(ref_id, state) {
            return Some(value);
        }
        if select_settings_screen_number_hidden(ref_id) {
            return None;
        }
    }
    match ref_id {
        // Lua draw 畳み込みのプレースホルダ (`number(0) >= 0` 等)
        0 => Some(0),
        21..=26 => current_datetime_number(ref_id),
        11 if state.select_screen => Some(state.select_mode_index as i64),
        12 if state.select_screen => Some(state.select_sort_index as i64),
        300 => Some(state.select_chart_count as i64),
        30 if state.select_screen => Some(state.select_play_count as i64),
        96 => Some(if state.play_level != 0 { state.play_level } else { state.select_play_level }),
        370 => Some(state.select_clear_index),
        92 if state.select_screen => {
            Some(select_chart_main_bpm(state).unwrap_or(state.select_bpm).round() as i64)
        }
        92 => Some(state.main_bpm.round() as i64),
        100 => Some(skin_point_score(state) as i64),
        71 | 101 | 171 => Some(state.ex_score as i64),
        72 => Some(state.total_notes as i64 * 2),
        74 | 106 | 333 => Some(state.total_notes.max(state.select_total_notes) as i64),
        350 if state.select_screen => Some(select_chart_normal_notes(state) as i64),
        351 if state.select_screen => Some(state.select_chart_long_notes as i64),
        352 if state.select_screen => Some(state.select_chart_scratch_notes as i64),
        353 if state.select_screen => Some(state.select_chart_long_scratch_notes as i64),
        360 if state.select_screen => Some(state.select_chart_peak_density.floor() as i64),
        361 if state.select_screen => Some(decimal_afterdot(state.select_chart_peak_density)),
        362 if state.select_screen => Some(state.select_chart_end_density.floor() as i64),
        363 if state.select_screen => Some(decimal_afterdot(state.select_chart_end_density)),
        364 if state.select_screen => Some(state.select_chart_density.floor() as i64),
        365 if state.select_screen => Some(decimal_afterdot(state.select_chart_density)),
        368 if state.select_screen => Some(state.select_chart_total_gauge.floor() as i64),
        75 | 105 | 174 => Some(state.max_combo as i64),
        76 if state.select_screen => state.select_bp.map(|count| count as i64).or(Some(0)),
        76 => Some((state.judge_counts.bad + state.judge_counts.poor) as i64),
        77 if state.select_screen => Some(state.select_play_count as i64),
        77 => Some(state.select_target_index as i64),
        78 if state.select_screen => Some(state.select_clear_count as i64),
        78 => Some(state.select_gauge_auto_shift_index as i64),
        80 | 110 => Some(state.judge_counts.pgreat as i64),
        81 | 111 => Some(state.judge_counts.great as i64),
        82 | 112 => Some(state.judge_counts.good as i64),
        83 | 113 => Some(state.judge_counts.bad as i64),
        84 | 114 => Some(state.judge_counts.poor as i64),
        85 => judge_rate_int(state.judge_counts.pgreat, state.total_notes),
        86 => judge_rate_int(state.judge_counts.great, state.total_notes),
        87 => judge_rate_int(state.judge_counts.good, state.total_notes),
        88 => judge_rate_int(state.judge_counts.bad, state.total_notes),
        89 => judge_rate_int(state.judge_counts.poor, state.total_notes),
        102 | 115 | 155 => Some(score_rate_parts(state.ex_score, state.total_notes).0 as i64),
        103 | 116 | 156 => Some(score_rate_parts(state.ex_score, state.total_notes).1 as i64),
        104 => Some(state.combo as i64),
        107 => Some(state.gauge.floor() as i64),
        407 => Some(gauge_after_dot(state.gauge) as i64),
        163 => Some((state.timeleft_ms / 60_000) as i64),
        164 => Some(((state.timeleft_ms / 1_000) % 60) as i64),
        1163 => Some(state.select_length_ms.max(0) / 60_000),
        1164 => Some((state.select_length_ms.max(0) / 1_000) % 60),
        310 => Some(state.hispeed.floor() as i64),
        311 => Some(((state.hispeed * 100.0) as i64) % 100),
        312 => Some(state.total_duration_ms as i64),
        313 => Some(((state.total_duration_ms as i64) * 3 + 2) / 5),
        308 if state.select_screen => Some(state.select_ln_mode_index as i64),
        // BPM 系: NUMBER_MAXBPM=90, NUMBER_MINBPM=91, NUMBER_NOWBPM=160
        90 => {
            Some(if state.max_bpm > 0.0 { state.max_bpm } else { state.select_max_bpm }.round()
                as i64)
        }
        91 => {
            Some(if state.min_bpm > 0.0 { state.min_bpm } else { state.select_min_bpm }.round()
                as i64)
        }
        160 => {
            Some(if state.now_bpm > 0.0 { state.now_bpm } else { state.select_bpm }.round() as i64)
        }
        // レーンカバー: NUMBER_LANECOVER1=14 (0-1000)
        14 => Some((state.lane_cover.clamp(0.0, 1.0) * 1000.0).round() as i64),
        // リフト: NUMBER_LIFT1=314 (0-1000)
        314 => Some((state.lift.clamp(0.0, 1.0) * 1000.0).round() as i64),
        // 選曲画面の音量表示: MASTER/KEY/BGM volume (0-100)
        57 => Some(select_volume_number(state.select_master_volume)),
        58 => Some(select_volume_number(state.select_key_volume)),
        59 => Some(select_volume_number(state.select_bgm_volume)),
        // 判定タイミングずれ: VALUE_JUDGE_1P_DURATION=525 (ms、絶対値)
        525 => state.judge_timing_ms.map(|ms| ms.unsigned_abs() as i64),
        // 判定タイミングオフセット設定値 (NUMBER_JUDGETIMING=12)
        12 => Some(state.judge_timing_offset_ms as i64),
        // Result timing distribution stats.
        374 => state.average_timing_ms.map(|value| value as i64),
        375 => state.average_timing_ms.map(timing_afterdot),
        376 => state.stddev_timing_ms.map(|value| value as i64),
        377 => state.stddev_timing_ms.map(|value| ((value.abs() * 100.0) as i64) % 100),
        // IR numbers. BMZ does not have an IR backend yet, so mirror beatoraja's
        // offline/no-ranking state by returning no value (Integer.MIN_VALUE).
        179..=182 | 200..=242 => None,
        // ベストスコア / ターゲットスコア (DB から供給、未取得時は None)
        150 | 170 => projected_best_score_at_progress(state).map(|s| s as i64),
        121 | 151 => state.target_ex_score.map(|s| projected_score_at_progress(s, state) as i64),
        122 | 123 | 135 | 136 | 157 | 158 => {
            state.target_ex_score.map(|target| score_rate_parts(target, state.total_notes)).map(
                |parts| (if matches!(ref_id, 122 | 135 | 157) { parts.0 } else { parts.1 }) as i64,
            )
        }
        183 | 184 => state
            .best_ex_score
            .map(|best| score_rate_parts(best, state.total_notes))
            .map(|parts| if ref_id == 183 { parts.0 } else { parts.1 } as i64),
        400 => state.judge_rank.map(|rank| rank as i64),
        154 => next_rank_diff(state),
        // NUMBER_DIFF_HIGHSCORE=152, NUMBER_DIFF_HIGHSCORE2=172 (符号付き、ex_score - best)
        152 | 172 => {
            projected_best_score_at_progress(state).map(|best| state.ex_score as i64 - best as i64)
        }
        // NUMBER_DIFF_TARGETSCORE=153 (符号付き、ex_score - target)
        153 => state.target_ex_score.map(|target| {
            state.ex_score as i64 - projected_score_at_progress(target, state) as i64
        }),
        // NUMBER_TARGET_MAXCOMBO=173
        173 => state.target_max_combo.map(|c| c as i64),
        // NUMBER_DIFF_MAXCOMBO=175 (符号付き、max_combo - target_max_combo)
        175 => state.target_max_combo.map(|target| state.max_combo as i64 - target as i64),
        // NUMBER_TARGET_BPCOUNT=176
        176 => state.target_bp.map(|c| c as i64),
        // NUMBER_DIFF_BPCOUNT=178 (符号付き、現在 bp - target_bp)
        178 => state.target_bp.map(|target| {
            (state.judge_counts.bad + state.judge_counts.poor) as i64 - target as i64
        }),
        // NUMBER_TARGET_CLEAR=371
        371 => state.best_clear_index.or(state.target_clear_index),
        // Fast/Slow split (PGREAT/GREAT/GOOD/BAD/POOR)
        410 => state.fast_slow_counts.map(|c| c.fast_pgreat as i64),
        411 => state.fast_slow_counts.map(|c| c.slow_pgreat as i64),
        412 => state.fast_slow_counts.map(|c| c.fast_great as i64),
        413 => state.fast_slow_counts.map(|c| c.slow_great as i64),
        414 => state.fast_slow_counts.map(|c| c.fast_good as i64),
        415 => state.fast_slow_counts.map(|c| c.slow_good as i64),
        416 => state.fast_slow_counts.map(|c| c.fast_bad as i64),
        417 => state.fast_slow_counts.map(|c| c.slow_bad as i64),
        418 => state.fast_slow_counts.map(|c| c.fast_poor as i64),
        419 => state.fast_slow_counts.map(|c| c.slow_poor as i64),
        420 => Some(state.judge_counts.empty_poor as i64),
        421 => state.fast_slow_counts.map(|c| c.fast_empty_poor as i64),
        422 => state.fast_slow_counts.map(|c| c.slow_empty_poor as i64),
        // NUMBER_TOTALEARLY=423, NUMBER_TOTALLATE=424
        423 => state.fast_slow_counts.map(|c| c.fast_total() as i64),
        424 => state.fast_slow_counts.map(|c| c.slow_total() as i64),
        425 | 427 => Some((state.judge_counts.bad + state.judge_counts.poor) as i64),
        426 => Some(state.judge_counts.poor as i64),
        _ => None,
    }
}

fn next_rank_diff(state: SkinDrawState) -> Option<i64> {
    let ex_score = state.select_ex_score.unwrap_or(state.ex_score) as i64;
    let total_notes = state.select_total_notes.max(state.total_notes) as i64;
    let max_score = total_notes.checked_mul(2)?;
    if max_score <= 0 {
        return None;
    }
    let ex_score = ex_score.clamp(0, max_score);
    for rank_step in (0..=24).step_by(3) {
        let threshold = div_ceil(rank_step as i64 * max_score, 27);
        if ex_score < threshold {
            return Some(threshold - ex_score);
        }
    }
    Some(max_score - ex_score)
}

fn projected_score_at_progress(final_score: u32, state: SkinDrawState) -> u32 {
    if state.total_notes == 0 {
        return final_score;
    }
    let past_notes = state.past_notes.min(state.total_notes);
    ((final_score as u64 * past_notes as u64) / state.total_notes as u64) as u32
}

fn projected_best_score_at_progress(state: SkinDrawState) -> Option<u32> {
    state
        .projected_best_ex_score
        .or_else(|| state.best_ex_score.map(|score| projected_score_at_progress(score, state)))
}

fn div_ceil(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        return 0;
    }
    numerator.div_euclid(denominator) + i64::from(numerator.rem_euclid(denominator) != 0)
}

fn rank_threshold(max_score: u32, rank_step: u32) -> u32 {
    div_ceil(rank_step as i64 * max_score as i64, 27).clamp(0, u32::MAX as i64) as u32
}

fn judge_rank_option_matches(op: i32, judge_rank: Option<i32>) -> bool {
    let Some(rank) = judge_rank else {
        return op == 182;
    };
    match op {
        180 => rank == 0 || (10..35).contains(&rank),
        181 => rank == 1 || (35..60).contains(&rank),
        182 => rank == 2 || (60..85).contains(&rank),
        183 => rank == 3 || (85..110).contains(&rank),
        184 => rank == 4 || rank >= 110,
        _ => false,
    }
}

fn judge_rate_int(count: u32, total_notes: u32) -> Option<i64> {
    if total_notes == 0 {
        return None;
    }
    Some(count as i64 * 100 / total_notes as i64)
}

fn score_rate_parts(ex_score: u32, total_notes: u32) -> (u32, u32) {
    if total_notes == 0 {
        return (0, 0);
    }
    let rate_tenths = ex_score.saturating_mul(1000) / total_notes.saturating_mul(2).max(1);
    (rate_tenths / 10, rate_tenths % 10)
}

fn skin_image_texture_region(
    image: &SkinImageDef,
    source_size: SkinImageSize,
    elapsed_ms: i32,
) -> TextureRegion {
    skin_image_texture_region_for_state(
        image,
        source_size,
        elapsed_ms,
        None,
        (image.x, image.y, image.w, image.h),
    )
}

fn pre_ready_lane_cover_value_destination(
    destination: &SkinDestinationDef,
    value: &SkinValueDef,
    state: SkinDrawState,
) -> bool {
    destination.timer == Some(40)
        && state.ready_timer_ms.is_none()
        && state.lane_cover_changing
        && destination.op.contains(&270)
        && skin_value_is_lane_cover_number(value)
}

fn skin_value_is_lane_cover_number(value: &SkinValueDef) -> bool {
    matches!(value.ref_id, 14 | 312 | 313)
        || skin_expr_references_lane_cover_number(&value.expr)
        || skin_expr_references_lane_cover_number(&value.value_expr)
}

fn skin_expr_references_lane_cover_number(expr: &str) -> bool {
    ["number(14)", "number(312)", "number(313)"].iter().any(|needle| expr.contains(needle))
}

/// Starseeker 閉店の `src = 0, x = 0, y = 0` sentinel は `system` の黒 1px
/// (`black` image と同じ UV) を指す。ECFN の判定ラインなど、`src = 0` でも
/// 明示的な crop 座標を持つ画像はそのまま扱う。
fn skin_image_pixel_rect(
    image: &SkinImageDef,
    images: &HashMap<&str, &SkinImageDef>,
) -> (i32, i32, i32, i32) {
    if image.src == "0"
        && image.x == 0
        && image.y == 0
        && let Some(black) = images.get("black")
    {
        return (black.x, black.y, black.w, black.h);
    }
    (image.x, image.y, image.w, image.h)
}

/// `image.ref_id` が指定されている場合、`SkinDrawState` から ref 値を引いて
/// 行インデックス（divy 方向）として使う。divx 方向は cycle 経過時間でアニメ。
/// ref 未指定なら従来通り全フレームを cycle で順次再生する。
fn skin_image_texture_region_for_state(
    image: &SkinImageDef,
    source_size: SkinImageSize,
    elapsed_ms: i32,
    state: Option<SkinDrawState>,
    pixel_rect: (i32, i32, i32, i32),
) -> TextureRegion {
    let source_width = source_size.width.max(1.0);
    let source_height = source_size.height.max(1.0);
    let (px, py, pw, ph) = resolve_skin_image_pixel_rect(pixel_rect, source_width, source_height);
    let divx = image.divx.max(1);
    let divy = image.divy.max(1);
    let frame_count = divx * divy;

    // ref_id が指定されている画像は「ref 値 = 行」「cycle = 列のサブアニメ」と解釈する。
    // ref 値が解決できない場合 (state 未提供 or 値 None) は行 0 にフォールバックし、
    // 全フレームを順次再生する cycle モードへは落とさない（高速点滅を防ぐため）。
    let frame_index = if image.ref_id != 0 {
        let row = state.and_then(|s| skin_state_number(image.ref_id, s)).unwrap_or(0);
        let max_row = if image.len > 0 { image.len.min(divy) } else { divy };
        let row = row.clamp(0, (max_row - 1).max(0) as i64) as i32;
        let col = if image.cycle > 0 && divx > 1 {
            (elapsed_ms.rem_euclid(image.cycle) * divx / image.cycle).min(divx - 1)
        } else {
            0
        };
        row * divx + col
    } else if image.cycle > 0 && frame_count > 1 {
        (elapsed_ms.rem_euclid(image.cycle) * frame_count / image.cycle).min(frame_count - 1)
    } else {
        0
    };

    let cell_width = pw as f32 / divx as f32;
    let cell_height = ph as f32 / divy as f32;
    let source_column = frame_index % divx;
    let source_row = frame_index / divx;
    TextureRegion {
        x: (px as f32 + cell_width * source_column as f32) / source_width,
        y: (py as f32 + cell_height * source_row as f32) / source_height,
        width: cell_width / source_width,
        height: cell_height / source_height,
    }
}

fn resolve_skin_image_pixel_rect(
    pixel_rect: (i32, i32, i32, i32),
    source_width: f32,
    source_height: f32,
) -> (i32, i32, i32, i32) {
    let (px, py, pw, ph) = pixel_rect;
    let resolved_w =
        if pw < 0 { (source_width.round() as i32).saturating_sub(px).max(0) } else { pw };
    let resolved_h =
        if ph < 0 { (source_height.round() as i32).saturating_sub(py).max(0) } else { ph };
    (px, py, resolved_w, resolved_h)
}

fn gauge_after_dot(gauge: f32) -> u32 {
    if gauge > 0.0 && gauge < 0.1 { 1 } else { ((gauge.max(0.0) * 10.0) as u32) % 10 }
}

fn timing_afterdot(value: f32) -> i64 {
    let afterdot = ((value.abs() * 100.0) as i64) % 100;
    if value < 0.0 { -afterdot } else { afterdot }
}

fn decimal_afterdot(value: f32) -> i64 {
    ((value.abs() * 100.0) as i64) % 100
}

fn select_chart_normal_notes(state: SkinDrawState) -> u32 {
    if state.select_chart_normal_notes > 0 {
        state.select_chart_normal_notes
    } else {
        state.select_total_notes
    }
}

fn select_chart_main_bpm(state: SkinDrawState) -> Option<f32> {
    (state.select_chart_main_bpm > 0.0).then_some(state.select_chart_main_bpm)
}

fn current_bp(state: SkinDrawState) -> u32 {
    state.judge_counts.bad + state.judge_counts.poor
}

fn skin_point_score(state: SkinDrawState) -> u32 {
    let total_notes = state.total_notes;
    if total_notes == 0 {
        return 0;
    }
    let counts = state.judge_counts;
    let numerator = match state.key_mode {
        KeyMode::K5 | KeyMode::K10 => {
            100_000_u64 * u64::from(counts.pgreat)
                + 100_000_u64 * u64::from(counts.great)
                + 50_000_u64 * u64::from(counts.good)
        }
        KeyMode::K7 | KeyMode::K14 | KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => {
            150_000_u64 * u64::from(counts.pgreat)
                + 100_000_u64 * u64::from(counts.great)
                + 20_000_u64 * u64::from(counts.good)
                + 50_000_u64 * u64::from(state.max_combo)
        }
        KeyMode::K9 => {
            100_000_u64 * u64::from(counts.pgreat)
                + 70_000_u64 * u64::from(counts.great)
                + 40_000_u64 * u64::from(counts.good)
        }
    };
    (numerator / u64::from(total_notes)).min(u64::from(u32::MAX)) as u32
}

fn score_rate_cmp_value(ex_score: u32, total_notes: u32) -> u32 {
    if total_notes == 0 { 0 } else { ex_score.saturating_mul(1000) / total_notes.max(1) }
}

/// Returns the graph bar fill ratio (0.0-1.0) for a given `BARGRAPH_*` type.
fn graph_value(graph_type: i32, state: SkinDrawState) -> f32 {
    match graph_type {
        101 => state.play_progress, // BARGRAPH_MUSIC_PROGRESS: elapsed / total playtime
        102 => 1.0,                 // BARGRAPH_LOAD_PROGRESS: always complete during play
        110 | 111 => {
            // BARGRAPH_SCORERATE / SCORERATE_FINAL: ex_score / max_ex_score
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 { state.ex_score as f32 / max } else { 0.0 }
        }
        // BARGRAPH_RATE_PGREAT..RATE_EXSCORE: judge count / past_notes (or total_notes)
        140 => judge_rate(state.judge_counts.pgreat, state.past_notes),
        141 => judge_rate(state.judge_counts.great, state.past_notes),
        142 => judge_rate(state.judge_counts.good, state.past_notes),
        143 => judge_rate(state.judge_counts.bad, state.past_notes),
        144 => judge_rate(state.judge_counts.poor, state.past_notes),
        145 => judge_rate(state.max_combo, state.total_notes),
        146 => {
            // BARGRAPH_RATE_SCORE: (pgreat + great*0.5) / total_notes
            let max = (state.past_notes * 2) as f32;
            if max > 0.0 {
                (state.judge_counts.pgreat * 2 + state.judge_counts.great) as f32 / max
            } else {
                0.0
            }
        }
        147 => {
            // BARGRAPH_RATE_EXSCORE: ex_score so far / (past_notes * 2)
            let max = (state.past_notes * 2) as f32;
            if max > 0.0 { state.ex_score as f32 / max } else { 0.0 }
        }
        // BARGRAPH_BESTSCORERATE_NOW (112): best score at current progress / max_ex_score.
        // When a beatoraja ghost is available, use its per-note progression instead of a
        // linear projection from the final best score.
        112 => {
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 {
                projected_best_score_at_progress(state).unwrap_or(0) as f32 / max
            } else {
                0.0
            }
        }
        // BARGRAPH_BESTSCORERATE (113): best_ex_score / (total_notes * 2)
        113 => {
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 { state.best_ex_score.unwrap_or(0) as f32 / max } else { 0.0 }
        }
        // BARGRAPH_TARGETSCORERATE_NOW (114): target_ex_score * past_notes / (total_notes^2 * 2)
        114 => {
            let max = (state.total_notes as f64).powi(2) * 2.0;
            if max > 0.0 {
                (state.target_ex_score.unwrap_or(0) as f64 * state.past_notes as f64 / max) as f32
            } else {
                0.0
            }
        }
        // BARGRAPH_TARGETSCORERATE (115): target_ex_score / (total_notes * 2)
        115 => {
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 { state.target_ex_score.unwrap_or(0) as f32 / max } else { 0.0 }
        }
        -1 => (state.select_clear_index as f32 / 10.0).clamp(0.0, 1.0),
        -2 => {
            let total_notes = state.select_total_notes.max(state.total_notes);
            let max = (total_notes * 2) as f32;
            if max > 0.0 { state.ex_score as f32 / max } else { 0.0 }
        }
        17 => state.select_master_volume.clamp(0.0, 1.0),
        18 => state.select_key_volume.clamp(0.0, 1.0),
        19 => state.select_bgm_volume.clamp(0.0, 1.0),
        // Lua fast/slow 比率 graph (ECFN select 等)
        148 => fast_slow_ratio_fast(state),
        149 => fast_slow_ratio_slow(state),
        _ => 0.0,
    }
}

fn graph_fill_ratio(graph: &SkinGraphDef, state: SkinDrawState) -> f32 {
    if !graph.value_expr.trim().is_empty() {
        return skin_state_float_expr(&graph.value_expr, state).unwrap_or(0.0);
    }
    graph_value(graph.graph_type, state)
}

fn skin_grid_cell_size(size: i32, divisions: i32) -> i32 {
    let divisions = divisions.max(1);
    size / divisions
}

fn fast_slow_ratio_fast(state: SkinDrawState) -> f32 {
    let Some(counts) = state.fast_slow_counts else {
        return 0.0;
    };
    let total = counts.fast_total() + counts.slow_total();
    if total == 0 { 0.0 } else { counts.fast_total() as f32 / total as f32 }
}

fn fast_slow_ratio_slow(state: SkinDrawState) -> f32 {
    let Some(counts) = state.fast_slow_counts else {
        return 0.0;
    };
    let total = counts.fast_total() + counts.slow_total();
    if total == 0 { 0.0 } else { counts.slow_total() as f32 / total as f32 }
}

fn skin_frame_expr_value(expr: SkinFrameExpr, state: SkinDrawState) -> Option<i32> {
    match expr {
        SkinFrameExpr::FastSlowBreakdownHeight(ref_id) => fast_slow_breakdown_height(ref_id, state),
    }
}

fn fast_slow_breakdown_height(ref_id: i32, state: SkinDrawState) -> Option<i32> {
    const REFS: [i32; 12] = [422, 419, 417, 415, 413, 411, 410, 412, 414, 416, 418, 421];
    if !REFS.contains(&ref_id) {
        return None;
    }
    let values = REFS.map(|candidate| skin_state_number(candidate, state).unwrap_or(0).max(0));
    let max = values.into_iter().max().unwrap_or(0);
    if max <= 0 {
        return Some(0);
    }
    let value = skin_state_number(ref_id, state).unwrap_or(0).max(0);
    Some((value as f32 / max as f32 * 100.0).round() as i32)
}

fn judge_rate(count: u32, total: u32) -> f32 {
    if total > 0 { count as f32 / total as f32 } else { 0.0 }
}

fn skin_slider_progress(slider: &SkinSliderDef, state: SkinDrawState) -> Option<f32> {
    if !slider.value_expr.trim().is_empty()
        && let Some(progress) = skin_builtin_value_f32(&slider.value_expr, state)
    {
        return Some(progress.clamp(0.0, 1.0));
    }
    skin_slider_progress_by_type(slider.slider_type, state)
}

fn skin_slider_progress_by_type(slider_type: i32, state: SkinDrawState) -> Option<f32> {
    match slider_type {
        1 => Some(state.select_scroll_progress.clamp(0.0, 1.0)),
        4 => (state.lane_cover > 0.0).then_some(state.lane_cover.clamp(0.0, 1.0)),
        6 => Some(state.play_progress.clamp(0.0, 1.0)),
        17 => Some(state.select_master_volume.clamp(0.0, 1.0)),
        18 => Some(state.select_key_volume.clamp(0.0, 1.0)),
        19 => Some(state.select_bgm_volume.clamp(0.0, 1.0)),
        _ => None,
    }
}

fn skin_timer_elapsed_ms(timer: Option<i32>, state: SkinDrawState) -> Option<i32> {
    match timer {
        None => Some(state.elapsed_ms),
        Some(2) => state.fadeout_ms,
        Some(3) => state.failed_ms,
        Some(150) => state.result_graph_begin_ms,
        Some(151) => state.result_graph_end_ms,
        Some(152) => state.result_update_score_ms,
        // TIMER_IR_CONNECT_BEGIN/SUCCESS/FAIL. BMZ has no IR backend yet.
        Some(172..=174) => None,
        Some(40) => state.ready_timer_ms,
        Some(41) => state.play_timer_ms,
        Some(11) => Some(state.select_bar_elapsed_ms),
        Some(21..=23) => Some(state.select_option_panel_elapsed_ms),
        Some(348..=352) => score_target_timer_elapsed_ms(timer.unwrap(), state),
        Some(46) => state.judge_ms[0],
        Some(47) => state.judge_ms[1],
        Some(247) => state.judge_ms[2],
        Some(48 | 49) => state.full_combo_ms,
        Some(908) => state.music_end_ms,
        Some(50..=57) => state.bomb_ms[(timer.unwrap() - 50) as usize],
        // 2P bomb: timer 60=Scratch2, 61-67=Key8-14
        Some(60) => state.bomb_ms[Lane::Scratch2.index()],
        Some(61..=67) => state.bomb_ms[Lane::Key8.index() + (timer.unwrap() - 61) as usize],
        Some(100..=107) => state.keyon_ms[(timer.unwrap() - 100) as usize],
        // 2P keyon: timer 110=Scratch2, 111-117=Key8-14
        Some(110) => state.keyon_ms[Lane::Scratch2.index()],
        Some(111..=117) => state.keyon_ms[Lane::Key8.index() + (timer.unwrap() - 111) as usize],
        Some(120..=127) => state.keyoff_ms[(timer.unwrap() - 120) as usize],
        // 2P keyoff: timer 130=Scratch2, 131-137=Key8-14
        Some(130) => state.keyoff_ms[Lane::Scratch2.index()],
        Some(131..=137) => state.keyoff_ms[Lane::Key8.index() + (timer.unwrap() - 131) as usize],
        Some(143 | 144) => state.end_of_note_ms,
        Some(id)
            if (SKIN_DYNAMIC_TIMER_BASE
                ..SKIN_DYNAMIC_TIMER_BASE + SKIN_DYNAMIC_TIMER_COUNT as i32)
                .contains(&id) =>
        {
            let idx = (id - SKIN_DYNAMIC_TIMER_BASE) as usize;
            state.dynamic_timer_ms[idx]
        }
        _ => None,
    }
}

fn skin_text_align(align: i32) -> TextAlign {
    match align {
        1 => TextAlign::Center,
        2 => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

fn skin_text_bitmap_size(
    text: &SkinTextDef,
    fonts: &[SkinFontDef],
    skin_height: u32,
) -> Option<f32> {
    if text.size <= 0 || text.font.is_empty() {
        return None;
    }
    let font_id = text.font.rsplit_once(':').map_or(text.font.as_str(), |(_, id)| id);
    let font = fonts.iter().find(|font| font.id == text.font || font.id == font_id)?;
    let extension = Path::new(&font.path).extension()?.to_str()?;
    extension.eq_ignore_ascii_case("fnt").then_some(text.size as f32 / skin_height.max(1) as f32)
}

fn skin_text_overflow(overflow: i32) -> TextOverflow {
    match overflow {
        1 => TextOverflow::Shrink,
        2 => TextOverflow::Truncate,
        _ => TextOverflow::Overflow,
    }
}

fn skin_text_shadow(text: &SkinTextDef, skin_width: u32, skin_height: u32) -> Option<TextShadow> {
    let color = skin_hex_color(&text.shadow_color)?;
    if color.a <= 0.0 {
        return None;
    }
    Some(TextShadow {
        color,
        offset: Point {
            x: text.shadow_offset_x / skin_width.max(1) as f32,
            y: text.shadow_offset_y / skin_height.max(1) as f32,
        },
    })
}

fn skin_text_outline(text: &SkinTextDef, skin_height: u32) -> Option<TextOutline> {
    if text.outline_width <= 0.0 {
        return None;
    }
    let color = skin_hex_color(&text.outline_color)?;
    if color.a <= 0.0 {
        return None;
    }
    Some(TextOutline { color, width: text.outline_width / skin_height.max(1) as f32 })
}

fn skin_hex_color(value: &str) -> Option<Color> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    let a =
        if hex.len() == 8 { u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0 } else { 1.0 };
    Some(Color::rgba(r, g, b, a))
}

#[derive(Debug, Clone, Copy)]
struct GaugeGraphColors {
    graph_bg: Color,
    graph_line: Color,
    border_bg: Color,
    border_line: Color,
}

fn gaugegraph_color_index(gauge_type: i32) -> usize {
    const TYPE_TABLE: [usize; 10] = [0, 1, 2, 3, 4, 5, 3, 4, 5, 3];
    TYPE_TABLE.get(gauge_type.max(0) as usize).copied().unwrap_or(3)
}

fn gaugegraph_colors(
    graph: &SkinGaugeGraphDef,
    color_index: usize,
    frame_alpha: f32,
) -> GaugeGraphColors {
    let colors = if graph.color.is_empty() {
        gaugegraph_default_color_strings(graph)
    } else {
        gaugegraph_explicit_color_strings(graph)
    };
    GaugeGraphColors {
        border_line: skin_hex_color(&colors[color_index][0])
            .unwrap_or(Color::rgb(0.0, 0.0, 0.0))
            .with_alpha(frame_alpha),
        border_bg: skin_hex_color(&colors[color_index][1])
            .unwrap_or(Color::rgb(0.0, 0.0, 0.0))
            .with_alpha(frame_alpha),
        graph_line: skin_hex_color(&colors[color_index][2])
            .unwrap_or(Color::rgb(0.0, 0.0, 0.0))
            .with_alpha(frame_alpha),
        graph_bg: skin_hex_color(&colors[color_index][3])
            .unwrap_or(Color::rgb(0.0, 0.0, 0.0))
            .with_alpha(frame_alpha),
    }
}

fn gaugegraph_explicit_color_strings(graph: &SkinGaugeGraphDef) -> [[String; 4]; 6] {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            graph.color.get(row * 4 + column).cloned().unwrap_or_else(|| "000000".to_string())
        })
    })
}

fn gaugegraph_default_color_strings(graph: &SkinGaugeGraphDef) -> [[String; 4]; 6] {
    let mut colors = [
        [
            graph.borderline_color.clone(),
            graph.border_color.clone(),
            graph.assist_clear_line_color.clone(),
            graph.assist_clear_bg_color.clone(),
        ],
        [
            graph.borderline_color.clone(),
            graph.border_color.clone(),
            graph.assist_and_easy_fail_line_color.clone(),
            graph.assist_and_easy_fail_bg_color.clone(),
        ],
        [
            graph.borderline_color.clone(),
            graph.border_color.clone(),
            graph.groove_fail_line_color.clone(),
            graph.groove_fail_bg_color.clone(),
        ],
        [
            graph.groove_clear_and_hard_line_color.clone(),
            graph.groove_clear_and_hard_bg_color.clone(),
            graph.groove_clear_and_hard_line_color.clone(),
            graph.groove_clear_and_hard_bg_color.clone(),
        ],
        [
            graph.ex_hard_line_color.clone(),
            graph.ex_hard_bg_color.clone(),
            graph.ex_hard_line_color.clone(),
            graph.ex_hard_bg_color.clone(),
        ],
        [
            graph.hazard_line_color.clone(),
            graph.hazard_bg_color.clone(),
            graph.hazard_line_color.clone(),
            graph.hazard_bg_color.clone(),
        ],
    ];
    for row in &mut colors {
        for color in row {
            if color.is_empty() {
                *color = "000000".to_string();
            }
        }
    }
    colors
}

fn gaugegraph_y(rect: Rect, gauge: f32, max: f32) -> f32 {
    rect.y + rect.height * (1.0 - (gauge / max).clamp(0.0, 1.0))
}

fn point_ratio(points: &[crate::snapshot::ResultGaugeGraphPoint], time_ms: i32) -> f32 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    let Some(last) = points.last() else {
        return 0.0;
    };
    let span = (last.time_ms - first.time_ms).max(1) as f32;
    (time_ms - first.time_ms).max(0) as f32 / span
}

#[allow(clippy::too_many_arguments)]
fn push_gaugegraph_segment(
    items: &mut Vec<SkinRenderItem>,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    line_w: f32,
    line_h: f32,
    color: Color,
    blend: BlendMode,
) {
    let width = (x2 - x1).max(line_w);
    items.push(SkinRenderItem::Rect {
        rect: Rect { x: x1, y: y1.min(y2), width: line_w, height: (y2 - y1).abs() + line_h },
        color,
        blend,
    });
    items.push(SkinRenderItem::Rect {
        rect: Rect { x: x1, y: y2, width, height: line_h },
        color,
        blend,
    });
}

#[derive(Debug, Clone, Copy, Default)]
struct TimingStats {
    average_ms: f32,
    stddev_ms: f32,
}

fn timing_stats(points: &[crate::snapshot::ResultTimingPoint]) -> TimingStats {
    if points.is_empty() {
        return TimingStats::default();
    }
    let count = points.len() as f32;
    let average_ms =
        points.iter().map(|point| point.delta_us as f32 / 1_000.0).sum::<f32>() / count;
    let variance = points
        .iter()
        .map(|point| {
            let diff = point.delta_us as f32 / 1_000.0 - average_ms;
            diff * diff
        })
        .sum::<f32>()
        / count;
    TimingStats { average_ms, stddev_ms: variance.sqrt() }
}

fn timing_color(value: &str, frame_alpha: f32) -> Color {
    skin_hex_color(value)
        .or_else(|| skin_hex_color("FF0000FF"))
        .unwrap_or(Color::rgb(1.0, 0.0, 0.0))
        .with_alpha(frame_alpha)
}

fn note_distribution_colors(alpha: f32) -> [Color; 7] {
    [
        Color::rgba(0.27, 1.0, 0.27, alpha),
        Color::rgba(0.13, 0.53, 0.13, alpha),
        Color::rgba(1.0, 0.27, 0.27, alpha),
        Color::rgba(0.27, 0.27, 1.0, alpha),
        Color::rgba(0.13, 0.13, 0.53, alpha),
        Color::rgba(0.80, 0.80, 0.80, alpha),
        Color::rgba(0.53, 0.0, 0.0, alpha),
    ]
}

fn timing_visualizer_judge_colors(visualizer: &SkinTimingVisualizerDef) -> [Color; 5] {
    [
        timing_color(&visualizer.pg_color, 1.0),
        timing_color(&visualizer.gr_color, 1.0),
        timing_color(&visualizer.gd_color, 1.0),
        timing_color(&visualizer.bd_color, 1.0),
        if visualizer.transparent == 1 {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        } else {
            timing_color(&visualizer.pr_color, 1.0)
        },
    ]
}

fn timing_distribution_judge_colors(graph: &SkinTimingDistributionGraphDef) -> [Color; 5] {
    [
        timing_color(&graph.pg_color, 1.0),
        timing_color(&graph.gr_color, 1.0),
        timing_color(&graph.gd_color, 1.0),
        timing_color(&graph.bd_color, 1.0),
        timing_color(&graph.pr_color, 1.0),
    ]
}

fn judge_timing_color(
    judge: Judge,
    visualizer: &SkinTimingVisualizerDef,
    fallback: Color,
) -> Color {
    match judge {
        Judge::PGreat => timing_color(&visualizer.pg_color, 1.0),
        Judge::Great => timing_color(&visualizer.gr_color, 1.0),
        Judge::Good => timing_color(&visualizer.gd_color, 1.0),
        Judge::Bad => timing_color(&visualizer.bd_color, 1.0),
        Judge::Poor | Judge::EmptyPoor if visualizer.transparent == 1 => {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        }
        Judge::Poor | Judge::EmptyPoor => timing_color(&visualizer.pr_color, 1.0),
    }
    .with_alpha(fallback.a)
}

fn timing_judge_band_items(
    rect: Rect,
    center_ms: f32,
    frame_alpha: f32,
    blend: BlendMode,
    colors: [Color; 5],
) -> Vec<SkinRenderItem> {
    let bands = [
        (-16.0, 16.0, colors[0]),
        (-33.0, -16.0, colors[1]),
        (16.0, 33.0, colors[1]),
        (-66.0, -33.0, colors[2]),
        (33.0, 66.0, colors[2]),
        (-100.0, -66.0, colors[3]),
        (66.0, 100.0, colors[3]),
        (-center_ms, -100.0, colors[4]),
        (100.0, center_ms, colors[4]),
    ];
    let mut items = Vec::new();
    for (start, end, color) in bands {
        let start = start.clamp(-center_ms, center_ms);
        let end = end.clamp(-center_ms, center_ms);
        if end <= start {
            continue;
        }
        let x1 = rect.x + ((start + center_ms) / (center_ms * 2.0)) * rect.width;
        let x2 = rect.x + ((end + center_ms) / (center_ms * 2.0)) * rect.width;
        items.push(SkinRenderItem::Rect {
            rect: Rect { x: x1, y: rect.y, width: (x2 - x1).max(0.0), height: rect.height },
            color: color.with_alpha(color.a * frame_alpha * 0.25),
            blend,
        });
    }
    items
}

fn timing_distribution_x(rect: Rect, center: usize, value_ms: f32) -> f32 {
    let span = (center.max(1) * 2) as f32;
    rect.x + ((center as f32 + value_ms) / span).clamp(0.0, 1.0) * rect.width
}

/// Rm-skin `text id="table"` と beatoraja `TEXT_TABLE1..3` (1001..1003) の表示ロジック。
pub fn format_rm_skin_course_table_text(
    course_stage: Option<CourseStageMarker>,
    primary: &str,
    secondary: &str,
    fallback: &str,
) -> String {
    if let Some(stage) = course_stage {
        return match stage {
            CourseStageMarker::Final => "COURSE : STAGE FINAL".to_string(),
            CourseStageMarker::Stage1 => "COURSE : STAGE 1".to_string(),
            CourseStageMarker::Stage2 => "COURSE : STAGE 2".to_string(),
            CourseStageMarker::Stage3 => "COURSE : STAGE 3".to_string(),
            CourseStageMarker::Stage4 => "COURSE : STAGE 4".to_string(),
        };
    }

    // Lua: `not tx1 or tx1 == "" and not tx2 or tx2 == ""`
    let use_fallback = secondary.is_empty() || (primary.is_empty() && secondary.is_empty());
    if use_fallback {
        if fallback.is_empty() {
            return "# No-Table".to_string();
        }
        return fallback.to_string();
    }

    if primary.is_empty() { format!(" > {secondary}") } else { format!("{primary} > {secondary}") }
}

fn skin_state_text(text: &SkinTextDef, state: SkinTextState<'_>) -> String {
    if !text.constant_text.is_empty() {
        return text.constant_text.clone();
    }
    if text.value_expr.trim() == SKIN_EXPR_COURSE_TABLE_TEXT {
        return format_rm_skin_course_table_text(
            state.course_stage,
            state.table_text_primary,
            state.table_text_secondary,
            state.table_text_fallback,
        );
    }
    if text.id == "table" {
        return format_rm_skin_course_table_text(
            state.course_stage,
            state.table_text_primary,
            state.table_text_secondary,
            state.table_text_fallback,
        );
    }
    if text.id.contains("bartext") {
        return state.bar_text.to_string();
    }
    if text.id == "table_level" {
        return state.table_level.to_string();
    }
    if text.id == "difficulty" || text.id == "difficulty_name" {
        return state.difficulty_name.to_string();
    }
    if text.id == "level" || text.id == "play_level" {
        return state.play_level.to_string();
    }
    match text.ref_id {
        3 => select_target_name(state.target),
        10 => state.title.to_string(),
        11 => state.subtitle.to_string(),
        12 => full_label(state.title, state.subtitle),
        13 => state.genre.to_string(),
        14 => state.artist.to_string(),
        15 => state.subartist.to_string(),
        16 => full_label(state.artist, state.subartist),
        17 => state.table_level.to_string(),
        30 => state.search_word.to_string(),
        150..=159 => state.course_titles[(text.ref_id - 150) as usize].to_string(),
        // beatoraja StringPropertyFactory: 1001=tablename, 1002=tablelevel,
        // 1003=tablefull.  Rm-skin's combined table label is handled above by
        // id/value_expr, so direct numeric refs follow the beatoraja mapping.
        1001 => state.table_text_primary.to_string(),
        1002 => state.table_level.to_string(),
        1003 => state.table_text_fallback.to_string(),
        1020 | 1021 => String::new(),
        200..=209 => select_target_name_by_offset(state.target, text.ref_id - 210),
        210..=219 => select_target_name_by_offset(state.target, text.ref_id - 209),
        1000 => state.current_folder.to_string(),
        _ => String::new(),
    }
}

const SELECT_TARGET_IDS: [&str; 9] = ["NONE", "MAX", "AAA", "AA", "A", "B", "C", "D", "E"];
const SELECT_TARGET_NAMES: [&str; 9] =
    ["NO TARGET", "MAX", "RANK AAA", "RANK AA", "RANK A", "RANK B", "RANK C", "RANK D", "RANK E"];

fn select_target_name(target: &str) -> String {
    SELECT_TARGET_IDS
        .iter()
        .position(|id| *id == target)
        .map(|index| SELECT_TARGET_NAMES[index].to_string())
        .unwrap_or_default()
}

fn select_target_name_by_offset(target: &str, offset: i32) -> String {
    let Some(index) = SELECT_TARGET_IDS.iter().position(|id| *id == target) else {
        return String::new();
    };
    let len = SELECT_TARGET_NAMES.len() as i32;
    let shifted = (index as i32 + offset).rem_euclid(len) as usize;
    SELECT_TARGET_NAMES[shifted].to_string()
}

fn full_label(primary: &str, secondary: &str) -> String {
    match (primary.is_empty(), secondary.is_empty()) {
        (true, true) => String::new(),
        (false, true) => primary.to_string(),
        (true, false) => secondary.to_string(),
        (false, false) => format!("{primary} {secondary}"),
    }
}

fn select_row_level_number(row: &SelectRowSnapshot) -> i64 {
    let source = if !row.table_level.is_empty() { &row.table_level } else { &row.play_level };
    source.chars().filter(|ch| ch.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn select_row_difficulty_code(row: &SelectRowSnapshot) -> i64 {
    difficulty_code_from_label(&row.difficulty_name)
}

fn difficulty_code_from_label(label: &str) -> i64 {
    let normalized = label.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "1" | "BEGINNER" => 1,
        "2" | "NORMAL" => 2,
        "3" | "HYPER" => 3,
        "4" | "ANOTHER" => 4,
        "5" | "INSANE" => 5,
        _ => 0,
    }
}

fn score_target_timer_elapsed_ms(timer_id: i32, state: SkinDrawState) -> Option<i32> {
    let max = state.total_notes.saturating_mul(2);
    let threshold = match timer_id {
        348 => rank_threshold(max, 18), // RANK A
        349 => rank_threshold(max, 21), // RANK AA
        350 => rank_threshold(max, 24), // RANK AAA
        351 => state.best_ex_score?,
        352 => state.target_ex_score?,
        _ => return None,
    };
    (threshold > 0 && state.ex_score >= threshold).then_some(state.elapsed_ms)
}

fn select_row_bar_image_index(row: &SelectRowSnapshot) -> usize {
    match row.kind {
        SelectRowKind::Song if !row.in_library => 4,
        SelectRowKind::Song => 0,
        SelectRowKind::Folder => 1,
        SelectRowKind::TableFolder => 2,
        SelectRowKind::Course => 3,
        SelectRowKind::SettingsFolder => 1,
        SelectRowKind::Config => 0,
    }
}

fn select_row_bar_text_index(row: &SelectRowSnapshot) -> usize {
    match row.kind {
        SelectRowKind::Song if !row.in_library => 8,
        SelectRowKind::Song => 2,
        SelectRowKind::Folder => 4,
        SelectRowKind::TableFolder => 6,
        // Course rows display the course title in the same slot as a song title
        // (text index 2), not the folder slot (6).
        SelectRowKind::Course => 2,
        SelectRowKind::SettingsFolder => 4,
        SelectRowKind::Config => 2,
    }
}

fn select_row_clear_index(row: &SelectRowSnapshot) -> usize {
    match row.clear_type.as_str() {
        "Failed" => 1,
        "AssistEasy" => 2,
        "LightAssistEasy" => 3,
        "Easy" => 4,
        "Normal" => 5,
        "Hard" => 6,
        "ExHard" => 7,
        "FullCombo" => 8,
        "Perfect" => 9,
        "Max" => 10,
        _ => 0,
    }
}

fn select_row_replay_index(row: &SelectRowSnapshot) -> Option<usize> {
    row.replay_slots.iter().position(|has_replay| *has_replay)
}

fn select_row_trophy_index(row: &SelectRowSnapshot) -> Option<usize> {
    let mut trophy_index = None;
    for name in &row.achieved_trophy_names {
        let rank = match name.as_str() {
            "bronzemedal" => 0,
            "silvermedal" => 1,
            "goldmedal" => 2,
            _ => continue,
        };
        trophy_index = Some(trophy_index.map_or(rank, |current: usize| current.max(rank)));
    }
    if trophy_index.is_some() {
        return trophy_index;
    }

    let ex_score = row.ex_score?;
    let max_score = row.total_notes.checked_mul(2)?;
    if max_score == 0 {
        return None;
    }
    let score = ex_score.min(max_score);
    if score * 9 >= max_score * 8 {
        Some(2)
    } else if score * 9 >= max_score * 7 {
        Some(1)
    } else if score * 9 >= max_score * 6 {
        Some(0)
    } else {
        None
    }
}

fn select_row_label_indices(row: &SelectRowSnapshot) -> Vec<usize> {
    let mut indices = Vec::new();
    if row.has_long_notes {
        indices.push(0);
    }
    if row.has_random {
        indices.push(1);
    }
    if row.has_mines {
        indices.push(2);
    }
    indices
}

fn select_replay_op_matches(op: i32, state: SkinDrawState) -> bool {
    if state.in_settings {
        return false;
    }
    let slot = match op {
        196..=198 => Some(0),
        1196..=1198 => Some(1),
        1199..=1201 => Some(2),
        1202..=1204 => Some(3),
        1205..=1208 => return state.select_replay_index == Some((op - 1205) as usize),
        _ => None,
    };
    let Some(slot) = slot else {
        return false;
    };
    let has_replay = state.select_replay_slots.get(slot).copied().unwrap_or(false);
    match op {
        196 | 1196 | 1199 | 1202 => !has_replay,
        197 | 1197 | 1200 | 1203 => has_replay,
        198 | 1198 | 1201 | 1204 => false,
        _ => false,
    }
}

fn result_replay_op_matches(op: i32, state: SkinDrawState) -> bool {
    let slot = match op {
        196..=198 => Some(0),
        1196..=1198 => Some(1),
        1199..=1201 => Some(2),
        1202..=1204 => Some(3),
        1205..=1208 => return false,
        _ => None,
    };
    let Some(slot) = slot else {
        return false;
    };
    let saved = state.result_saved_replay_slots.get(slot).copied().unwrap_or(false);
    let exists = state.result_replay_slots.get(slot).copied().unwrap_or(false) && !saved;
    match op {
        196 | 1196 | 1199 | 1202 => !exists && !saved,
        197 | 1197 | 1200 | 1203 => exists,
        198 | 1198 | 1201 | 1204 => saved,
        _ => false,
    }
}

fn select_song_detail_row(state: SkinDrawState) -> bool {
    matches!(
        state.select_row_kind,
        SelectRowKind::Song if !state.select_is_folder && state.select_in_library
    )
}

fn select_banner_option_matches(want_banner: bool, state: SkinDrawState) -> bool {
    if !state.select_screen {
        return false;
    }
    state.select_has_banner == want_banner
}

fn select_key_mode_option_matches(op: i32, state: SkinDrawState) -> bool {
    if state.in_settings || state.select_row_kind != SelectRowKind::Song {
        return false;
    }
    let Some(mode) = state.select_chart_key_mode else {
        return false;
    };
    match op {
        160 => matches!(mode, KeyMode::K7 | KeyMode::K8),
        161 => matches!(mode, KeyMode::K5),
        162 => matches!(mode, KeyMode::K14),
        163 => matches!(mode, KeyMode::K10),
        164 => matches!(mode, KeyMode::K9),
        1160 | 1161 => false,
        _ => false,
    }
}

fn select_detail_artist<'a>(
    snapshot: &SelectSnapshot,
    selected_row: Option<&'a SelectRowSnapshot>,
) -> &'a str {
    if !snapshot.in_settings {
        return selected_row.map(|row| row.artist.as_str()).unwrap_or_default();
    }
    selected_row
        .filter(|row| row.kind == SelectRowKind::Config)
        .map(|row| row.artist.as_str())
        .unwrap_or_default()
}

fn select_detail_subtitle<'a>(
    snapshot: &SelectSnapshot,
    selected_row: Option<&'a SelectRowSnapshot>,
) -> &'a str {
    if snapshot.in_settings {
        if snapshot.settings_editing
            && selected_row.is_some_and(|row| row.kind == SelectRowKind::Config)
        {
            return "[編集中]";
        }
        return "";
    }
    selected_row.map(|row| row.subtitle.as_str()).unwrap_or_default()
}

fn select_row_shows_score_decorations(row: &SelectRowSnapshot) -> bool {
    !row.is_folder
        && row.in_library
        && !matches!(row.kind, SelectRowKind::Config | SelectRowKind::SettingsFolder)
}

fn select_row_shows_lamp(row: &SelectRowSnapshot) -> bool {
    row.in_library && !matches!(row.kind, SelectRowKind::Config | SelectRowKind::SettingsFolder)
}

fn select_row_shows_course_trophy(row: &SelectRowSnapshot) -> bool {
    row.kind == SelectRowKind::Course
}

fn select_row_shows_folder_distribution(row: &SelectRowSnapshot) -> bool {
    row.is_folder && matches!(row.kind, SelectRowKind::Folder | SelectRowKind::TableFolder)
}

fn select_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
    if !select_rank_available(state) {
        return false;
    }
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    op == 200 + rank as i32
}

fn select_small_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
    if !select_rank_available(state) {
        return false;
    }
    let (ex_score, total_notes) = current_rank_inputs(state);
    let max_score = total_notes.saturating_mul(2);
    if max_score == 0 || ex_score.is_none() {
        return false;
    }
    let ex_score = ex_score.unwrap();
    if ex_score >= max_score {
        return op == 300;
    }
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    rank <= 6 && op == 301 + rank as i32
}

fn select_rank_available(state: SkinDrawState) -> bool {
    if state.in_settings {
        return false;
    }
    !state.select_screen
        || (state.select_row_kind == SelectRowKind::Song
            && !state.select_is_folder
            && state.select_in_library)
}

fn result_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
    if matches!(op, 308 | 318) {
        return state.ex_score == 0 && state.total_notes > 0;
    }
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    match op {
        300..=307 => op == 300 + rank as i32,
        310..=317 => op == 310 + rank as i32,
        _ => false,
    }
}

fn current_datetime_number(ref_id: i32) -> Option<i64> {
    let seconds =
        SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs().min(i64::MAX as u64) as i64;
    let date = unix_seconds_to_local_datetime(seconds)
        .unwrap_or_else(|| unix_seconds_to_utc_datetime(seconds));
    match ref_id {
        21 => Some(date.year as i64),
        22 => Some(date.month as i64),
        23 => Some(date.day as i64),
        24 => Some(date.hour as i64),
        25 => Some(date.minute as i64),
        26 => Some(date.second as i64),
        _ => None,
    }
}

#[cfg(unix)]
fn unix_seconds_to_local_datetime(seconds: i64) -> Option<SkinDateTime> {
    let raw_time = seconds as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` are valid pointers for the duration of the call.
    // `localtime_r` initializes `tm` on success and returns null on failure.
    let result = unsafe { libc::localtime_r(&raw_time, tm.as_mut_ptr()) };
    if result.is_null() {
        return None;
    }
    // SAFETY: The non-null result means `tm` has been fully initialized.
    let tm = unsafe { tm.assume_init() };
    Some(datetime_from_tm(tm))
}

#[cfg(windows)]
fn unix_seconds_to_local_datetime(seconds: i64) -> Option<SkinDateTime> {
    let raw_time = seconds as libc::time_t;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` are valid pointers for the duration of the call.
    // `localtime_s` initializes `tm` when it returns zero.
    let result = unsafe { libc::localtime_s(tm.as_mut_ptr(), &raw_time) };
    if result != 0 {
        return None;
    }
    // SAFETY: A zero return value means `tm` has been fully initialized.
    let tm = unsafe { tm.assume_init() };
    Some(datetime_from_tm(tm))
}

#[cfg(not(any(unix, windows)))]
fn unix_seconds_to_local_datetime(_seconds: i64) -> Option<SkinDateTime> {
    None
}

fn datetime_from_tm(tm: libc::tm) -> SkinDateTime {
    SkinDateTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1).clamp(1, 12) as u32,
        day: tm.tm_mday.clamp(1, 31) as u32,
        hour: tm.tm_hour.clamp(0, 23) as u32,
        minute: tm.tm_min.clamp(0, 59) as u32,
        second: tm.tm_sec.clamp(0, 59) as u32,
    }
}

#[derive(Debug, Clone, Copy)]
struct SkinDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

fn unix_seconds_to_utc_datetime(seconds: i64) -> SkinDateTime {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    SkinDateTime {
        year,
        month,
        day,
        hour: seconds_of_day / 3_600,
        minute: (seconds_of_day % 3_600) / 60,
        second: seconds_of_day % 60,
    }
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

fn best_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
    if state.in_settings {
        return false;
    }
    let Some(rank) = rank_index(state.best_ex_score, state.total_notes) else {
        return false;
    };
    op == 320 + rank as i32
}

/// 現在のランク判定の基準値 (ex_score, total_notes)。
/// Result 画面なら結果値、それ以外は select の選択中曲のベスト値を使う。
fn current_rank_inputs(state: SkinDrawState) -> (Option<u32>, u32) {
    if state.result_failed.is_some() {
        (Some(state.ex_score), state.total_notes)
    } else if state.select_screen {
        (state.select_ex_score, state.select_total_notes)
    } else {
        (Some(state.ex_score), state.total_notes)
    }
}

fn current_rank_index(state: SkinDrawState) -> Option<usize> {
    let (ex_score, total_notes) = current_rank_inputs(state);
    rank_index(ex_score, total_notes)
}

fn rank_index(ex_score: Option<u32>, total_notes: u32) -> Option<usize> {
    let ex_score = ex_score?;
    let max_score = total_notes.saturating_mul(2);
    if max_score == 0 {
        return None;
    }
    let score = ex_score.min(max_score) as u64;
    let max = max_score as u64;
    let rank = if score * 9 >= max * 8 {
        0
    } else if score * 9 >= max * 7 {
        1
    } else if score * 9 >= max * 6 {
        2
    } else if score * 9 >= max * 5 {
        3
    } else if score * 9 >= max * 4 {
        4
    } else if score * 9 >= max * 3 {
        5
    } else if score * 9 >= max * 2 {
        6
    } else {
        7
    };
    Some(rank)
}

pub(crate) fn select_arrange_index(arrange: &str) -> usize {
    match arrange {
        "MIRROR" => 1,
        "RANDOM" => 2,
        _ => 0,
    }
}

fn select_gauge_index(gauge: &str) -> usize {
    match gauge {
        "A-EASY" => 0,
        "EASY" => 1,
        "NORMAL" => 2,
        "HARD" => 3,
        "EX-HARD" => 4,
        "HAZARD" => 5,
        _ => 2,
    }
}

fn select_gauge_auto_shift_index(mode: &str) -> usize {
    match mode {
        "CONTINUE" => 1,
        "HARD TO GROOVE" => 2,
        "BEST CLEAR" => 3,
        "SELECT TO UNDER" => 4,
        _ => 0,
    }
}

fn select_target_index(target: &str) -> usize {
    match target {
        "MAX" => 1,
        "AAA" => 2,
        "AA" => 3,
        "A" => 4,
        "B" => 5,
        "C" => 6,
        "D" => 7,
        "E" => 8,
        _ => 0,
    }
}

fn select_bga_index(bga: &str) -> usize {
    match bga {
        "AUTO" => 1,
        "OFF" => 2,
        _ => 0,
    }
}

fn select_assist_index(assist: &str) -> usize {
    match assist {
        "AUTOPLAY" => 1,
        _ => 0,
    }
}

fn select_mode_index(mode: &str) -> usize {
    match mode {
        "7K" => 1,
        "14K" => 2,
        "9K" => 3,
        "5K" => 4,
        "10K" => 5,
        "24K" => 6,
        "24K_DOUBLE" => 7,
        _ => 0,
    }
}

fn select_sort_index(sort: &str) -> usize {
    match sort {
        "ARTIST" => 1,
        "BPM" => 2,
        "LENGTH" => 3,
        "LEVEL" => 4,
        "CLEAR" => 5,
        "SCORE" => 6,
        "BPCOUNT" => 7,
        _ => 0,
    }
}

fn select_ln_mode_index(mode: &str) -> usize {
    match mode {
        "CN" | "AUTO(CN)" | "FORCE(CN)" => 1,
        "HCN" | "AUTO(HCN)" | "FORCE(HCN)" => 2,
        _ => 0,
    }
}

fn select_scroll_progress(snapshot: &SelectSnapshot) -> f32 {
    if snapshot.chart_count <= 1 {
        return 0.0;
    }
    snapshot.selected_index.min(snapshot.chart_count - 1) as f32 / (snapshot.chart_count - 1) as f32
}

fn select_snapshot_selected_row_position(rows: &[SelectRowSnapshot], selected_index: u32) -> usize {
    let center = rows.len() / 2;
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.index == selected_index)
        .min_by_key(|(index, _)| index.abs_diff(center))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn destination_entry_at<'a>(
    entries: &'a [DestinationListEntry],
    index: usize,
    enabled_options: &[i32],
) -> Option<&'a SkinDestinationDef> {
    destination_entries(entries, enabled_options).into_iter().nth(index)
}

fn destination_entries<'a>(
    entries: &'a [DestinationListEntry],
    enabled_options: &[i32],
) -> Vec<&'a SkinDestinationDef> {
    let mut result = Vec::new();
    for entry in entries {
        match entry {
            DestinationListEntry::Single(destination) => result.push(destination),
            DestinationListEntry::Conditional { if_ops, destinations } => {
                if test_skin_dst_if(if_ops, enabled_options) {
                    result.extend(destinations);
                }
            }
        }
    }
    result
}

/// Parses the `if` field of a conditional dst entry into a flat list of required option IDs.
/// Each ID is positive (must be enabled) or negative (must be disabled).
/// Nested arrays (OR groups) are flattened to their first element for simplicity.
fn parse_skin_dst_if_ops(value: &JsonValue) -> Vec<i32> {
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

fn test_skin_dst_if(if_ops: &[i32], enabled_options: &[i32]) -> bool {
    if_ops.iter().all(|&op| test_json_option_number(op, enabled_options))
}

/// Expands a dst entry list into animation frames, filtering conditional entries by `enabled_options`.
fn flatten_dst_entries(dst: &[SkinDstEntry], enabled_options: &[i32]) -> Vec<SkinAnimationDef> {
    let mut result = Vec::new();
    for entry in dst {
        match entry {
            SkinDstEntry::Frame(anim) => result.push(*anim),
            SkinDstEntry::Conditional { if_ops, frames } => {
                if test_skin_dst_if(if_ops, enabled_options) {
                    result.extend(frames.iter().copied());
                }
            }
        }
    }
    result
}

fn apply_skin_offset_to_frame(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: SkinDrawState,
    include_hidden_cover_offsets: bool,
) {
    apply_skin_offset_to_frame_inner(destination, frame, state, include_hidden_cover_offsets, false)
}

/// beatoraja の `SkinObject.setRelative(true)` 相当 (SkinNumber 等で使用)。
/// destination の offset を適用する際、x/y シフトはスキップし w/h/r/a のみ加算する。
fn apply_skin_offset_to_frame_relative(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: SkinDrawState,
) {
    apply_skin_offset_to_frame_inner(destination, frame, state, false, true)
}

fn apply_skin_offset_to_frame_inner(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: SkinDrawState,
    include_hidden_cover_offsets: bool,
    relative: bool,
) {
    let mut ids: Vec<i32> = destination.offsets.clone();
    if destination.offset != 0 {
        ids.push(destination.offset);
    }
    if is_judge_detail_destination_id(&destination.id) && !ids.contains(&OFFSET_JUDGEDETAIL_1P) {
        ids.push(OFFSET_JUDGEDETAIL_1P);
    }
    if include_hidden_cover_offsets {
        if !ids.contains(&3) {
            ids.push(3);
        }
        if !ids.contains(&5) {
            ids.push(5);
        }
    }

    apply_skin_offset_ids_to_frame(&ids, frame, state, relative);
}

fn apply_skin_offset_ids_to_frame(
    ids: &[i32],
    frame: &mut ResolvedSkinFrame,
    state: SkinDrawState,
    relative: bool,
) {
    for &offset_id in ids {
        match offset_id {
            3 => frame.y += state.offset_lift_px,
            4 => frame.y += state.offset_lanecover_px,
            5 => {
                frame.y += state.offset_hidden_cover_px;
                if state.hidden_cover <= 0.0 {
                    frame.a = (frame.a - 255).clamp(0, 255);
                }
            }
            SKIN_OFFSET_BAR_LINE => {}
            OFFSET_NOTES_1P => {}
            _ => {
                if let Some(offset) = state.skin_offsets.get(offset_id) {
                    if !relative {
                        // beatoraja: !relative のとき x/y は中央アンカーでシフト
                        frame.x += offset.x - offset.w / 2;
                        frame.y += offset.y - offset.h / 2;
                    }
                    frame.w += offset.w;
                    frame.h += offset.h;
                    frame.angle += offset.r;
                    frame.a = (frame.a + offset.a).clamp(0, 255);
                }
            }
        }
    }
}

/// `note_y` progress (0=判定ライン, 1=最奥) を `note.dst` エリア内の正規化 Y に変換する。
/// LIFT (`offset_lift_px`) により判定ラインを上げ、スクロール範囲を縮める。
fn note_progress_to_y(area: Rect, progress: f32, state: SkinDrawState, canvas_h: f32) -> f32 {
    let lift_norm = state.offset_lift_px as f32 / canvas_h.max(1.0);
    let scroll_top = area.y;
    let judge_bottom = (area.y + area.height - lift_norm).max(scroll_top);
    let progress = progress.clamp(0.0, 1.0);
    judge_bottom - progress * (judge_bottom - scroll_top)
}

/// 小節線 (`note.group`) 向けオフセット適用。Notes offset (30) はノーツ専用のため除外する。
fn apply_bar_line_skin_offsets_to_frame(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: SkinDrawState,
) {
    let mut ids: Vec<i32> = destination
        .offsets
        .iter()
        .copied()
        .filter(|&id| id != OFFSET_NOTES_1P && id != SKIN_OFFSET_BAR_LINE)
        .collect();
    if destination.offset != 0
        && destination.offset != OFFSET_NOTES_1P
        && destination.offset != SKIN_OFFSET_BAR_LINE
    {
        ids.push(destination.offset);
    }
    apply_skin_offset_ids_to_frame(&ids, frame, state, false);
    apply_bar_line_offset_to_frame(frame, state);
}

fn apply_bar_line_offset_to_frame(frame: &mut ResolvedSkinFrame, state: SkinDrawState) {
    if let Some(offset) = state.skin_offsets.get(SKIN_OFFSET_BAR_LINE) {
        frame.h = (frame.h + offset.h).max(0);
    }
}

fn is_judge_detail_destination_id(id: &str) -> bool {
    matches!(id, "judge-early" | "judge-late") || id.starts_with("judgems")
}

fn apply_all_offset_to_render_item(item: SkinRenderItem, state: SkinDrawState) -> SkinRenderItem {
    let Some(offset) = state.skin_offsets.get(OFFSET_ALL) else {
        return item;
    };
    if offset.x == 0 && offset.y == 0 && offset.w == 0 && offset.h == 0 {
        return item;
    }
    let scale_x = (offset.w + 100) as f32 / 100.0;
    let scale_y = (offset.h + 100) as f32 / 100.0;
    let translate_x = offset.x as f32 / 100.0;
    let translate_y = offset.y as f32 / 100.0;
    match item {
        SkinRenderItem::Image {
            texture,
            rect,
            uv,
            tint,
            blend,
            scale,
            border,
            source_size,
            linear_filter,
        } => SkinRenderItem::Image {
            texture,
            rect: apply_all_offset_to_rect(rect, scale_x, scale_y, translate_x, translate_y),
            uv,
            tint,
            blend,
            scale,
            border,
            source_size,
            linear_filter,
        },
        SkinRenderItem::RotatedImage {
            texture,
            rect,
            uv,
            tint,
            blend,
            source_size,
            linear_filter,
            angle_deg,
            center,
        } => SkinRenderItem::RotatedImage {
            texture,
            rect: apply_all_offset_to_rect(rect, scale_x, scale_y, translate_x, translate_y),
            uv,
            tint,
            blend,
            source_size,
            linear_filter,
            angle_deg,
            center: apply_all_offset_to_point(center, scale_x, scale_y, translate_x, translate_y),
        },
        SkinRenderItem::Text { origin, text, style, blend } => SkinRenderItem::Text {
            origin: apply_all_offset_to_point(origin, scale_x, scale_y, translate_x, translate_y),
            text,
            style,
            blend,
        },
        SkinRenderItem::Rect { rect, color, blend } => SkinRenderItem::Rect {
            rect: apply_all_offset_to_rect(rect, scale_x, scale_y, translate_x, translate_y),
            color,
            blend,
        },
    }
}

fn apply_all_offset_to_rect(
    rect: Rect,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Rect {
    Rect {
        x: rect.x * scale_x + translate_x,
        y: rect.y * scale_y - translate_y,
        width: rect.width * scale_x,
        height: rect.height * scale_y,
    }
}

fn apply_all_offset_to_point(
    point: Point,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Point {
    Point { x: point.x * scale_x + translate_x, y: point.y * scale_y - translate_y }
}

fn skin_image_item_for_frame(
    texture: SkinTextureId,
    rect: Rect,
    uv: TextureRegion,
    frame: ResolvedSkinFrame,
    center: i32,
    blend: BlendMode,
    source_size: Option<SkinImageSize>,
    linear_filter: bool,
) -> SkinRenderItem {
    let tint = Color::rgba(
        frame.r as f32 / 255.0,
        frame.g as f32 / 255.0,
        frame.b as f32 / 255.0,
        frame.a as f32 / 255.0,
    );
    if frame.angle == 0 {
        return SkinRenderItem::Image {
            texture,
            rect,
            uv,
            tint,
            blend,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size,
            linear_filter,
        };
    }
    SkinRenderItem::RotatedImage {
        texture,
        rect,
        uv,
        tint,
        blend,
        source_size,
        linear_filter,
        angle_deg: frame.angle as f32,
        center: skin_rotation_center(center),
    }
}

fn skin_rotation_center(center: i32) -> Point {
    const CENTER_X: [f32; 10] = [0.5, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0];
    const CENTER_Y_BOTTOM_ORIGIN: [f32; 10] = [0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0];
    let index = usize::try_from(center).ok().filter(|index| *index < CENTER_X.len()).unwrap_or(0);
    Point { x: CENTER_X[index], y: 1.0 - CENTER_Y_BOTTOM_ORIGIN[index] }
}

fn resolve_destination_frame(
    destination: &SkinDestinationDef,
    elapsed_ms: i32,
    enabled_options: &[i32],
    state: SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    // `cycle` はアニメーション終端（最後のキーフレーム時刻）。
    let cycle = animations.iter().filter_map(|a| a.time).max().unwrap_or(0);
    let elapsed_ms = match destination.loop_time {
        // loop:負値 → ループせず、終端を過ぎたら描画しない（READY やボム等の単発演出）。
        Some(loop_point) if loop_point < 0 => {
            if elapsed_ms > cycle {
                return None;
            }
            elapsed_ms
        }
        // loop:0以上 → 終端到達後 loop_point 時刻へループバック。
        Some(loop_point) => resolve_loop_elapsed(loop_point, elapsed_ms, cycle),
        // loop 未指定 → ループなし。1回再生して最終フレームを保持する。
        None => elapsed_ms,
    };
    let acc = destination_interpolation_acc_from_frames(&animations);
    let mut frame = ResolvedSkinFrame::default();
    let mut previous = None;
    for animation in &animations {
        apply_skin_animation(&mut frame, animation, state);
        if frame.time <= elapsed_ms {
            previous = Some(frame);
            continue;
        }
        // previous=None は最初のキーフレーム時刻より前 → destination はまだ表示開始
        // していない。beatoraja 同様、開始時刻前のオブジェクトは描画しない。
        return previous.map(|previous| interpolate_skin_frame(previous, frame, elapsed_ms, acc));
    }
    previous.or_else(|| animations.first().map(|_| frame))
}

fn resolve_destination_frame_until_end(
    destination: &SkinDestinationDef,
    elapsed_ms: i32,
    enabled_options: &[i32],
    state: SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    if matches!(destination.loop_time, Some(loop_point) if loop_point > 0) {
        return resolve_destination_frame(destination, elapsed_ms, enabled_options, state);
    }
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    let last_time = animations.iter().filter_map(|a| a.time).max()?;
    if elapsed_ms > last_time {
        return None;
    }
    resolve_destination_frame(destination, elapsed_ms, enabled_options, state)
}

/// beatoraja の `loop` セマンティクスでアニメーション内の経過時刻を求める。
///
/// `loop` フィールドはループ「周期」ではなく、終端到達後に戻る「ループバック地点」。
/// - `loop_point >= 0` かつ `elapsed >= cycle`: `[loop_point, cycle)` 区間を繰り返す。
///   `loop_point >= cycle`（`loop == 終端` を含む）の場合は終端で停止し、
///   アニメーションは1回再生して最終フレームを保持する。
/// - `loop_point < 0`: ループしない（終端後の非表示は呼び出し側で判定）。
fn resolve_loop_elapsed(loop_point: i32, elapsed_ms: i32, cycle: i32) -> i32 {
    if loop_point >= 0 && elapsed_ms >= cycle {
        let span = cycle - loop_point;
        if span > 0 { (elapsed_ms - loop_point).rem_euclid(span) + loop_point } else { cycle }
    } else {
        elapsed_ms
    }
}

fn interpolate_skin_frame(
    start: ResolvedSkinFrame,
    end: ResolvedSkinFrame,
    elapsed_ms: i32,
    acc: i32,
) -> ResolvedSkinFrame {
    let duration = end.time - start.time;
    if duration <= 0 {
        return end;
    }
    let t = eased_skin_frame_rate(
        ((elapsed_ms - start.time) as f32 / duration as f32).clamp(0.0, 1.0),
        acc,
    );
    ResolvedSkinFrame {
        time: elapsed_ms,
        x: interpolate_i32(start.x, end.x, t),
        y: interpolate_i32(start.y, end.y, t),
        w: interpolate_i32(start.w, end.w, t),
        h: interpolate_i32(start.h, end.h, t),
        acc: end.acc,
        a: interpolate_i32(start.a, end.a, t),
        r: interpolate_i32(start.r, end.r, t),
        g: interpolate_i32(start.g, end.g, t),
        b: interpolate_i32(start.b, end.b, t),
        angle: interpolate_i32(start.angle, end.angle, t),
    }
}

fn destination_interpolation_acc_from_frames(animations: &[SkinAnimationDef]) -> i32 {
    let mut frame = ResolvedSkinFrame::default();
    for animation in animations {
        apply_skin_animation(&mut frame, animation, SkinDrawState::default());
        if frame.acc != 0 {
            return frame.acc;
        }
    }
    0
}

fn eased_skin_frame_rate(t: f32, acc: i32) -> f32 {
    match acc {
        1 => t * t,
        2 => 1.0 - (t - 1.0) * (t - 1.0),
        3 => 0.0,
        _ => t,
    }
}

fn interpolate_i32(start: i32, end: i32, t: f32) -> i32 {
    (start as f32 + (end - start) as f32 * t).round() as i32
}

fn apply_skin_animation(
    frame: &mut ResolvedSkinFrame,
    animation: &SkinAnimationDef,
    state: SkinDrawState,
) {
    if let Some(time) = animation.time {
        frame.time = time;
    }
    if let Some(x) = animation.x {
        frame.x = x;
    }
    if let Some(y) = animation.y {
        frame.y = y;
    }
    if let Some(w) = animation.w {
        frame.w = w;
    }
    if let Some(h) = animation.h {
        frame.h = h;
    }
    if let Some(expr) = animation.h_expr
        && let Some(h) = skin_frame_expr_value(expr, state)
    {
        frame.h = h;
    }
    if let Some(acc) = animation.acc {
        frame.acc = acc;
    }
    if let Some(a) = animation.a {
        frame.a = a;
    }
    if let Some(r) = animation.r {
        frame.r = r;
    }
    if let Some(g) = animation.g {
        frame.g = g;
    }
    if let Some(b) = animation.b {
        frame.b = b;
    }
    if let Some(angle) = animation.angle {
        frame.angle = angle;
    }
}

fn destination_uses_skin_offset(destination: &SkinDestinationDef, offset_id: i32) -> bool {
    destination.offset == offset_id || destination.offsets.contains(&offset_id)
}

fn destination_uses_lift_offset_only(destination: &SkinDestinationDef) -> bool {
    destination_uses_skin_offset(destination, 3)
        && !destination_uses_skin_offset(destination, 4)
        && !destination_uses_skin_offset(destination, 5)
}

/// Starseeker 等で `groove_frame_iidx` destination が `groove_frame` image を共有する。
fn skin_image_for_destination_id<'a>(
    destination_id: &str,
    images: &'a HashMap<&str, &SkinImageDef>,
) -> Option<&'a SkinImageDef> {
    images
        .get(destination_id)
        .copied()
        .or_else(|| destination_id.strip_suffix("_iidx").and_then(|base| images.get(base)).copied())
}

fn is_lift_lane_cover_id(id: &str) -> bool {
    id.eq_ignore_ascii_case("liftcover")
        || id.eq_ignore_ascii_case("lift-cover")
        || id.eq_ignore_ascii_case("lift_cover")
        || id.to_ascii_lowercase().contains("liftcover")
}

/// beatoraja `SkinHidden` 準拠: `disappear_line` より下 (y が小さい側) を切り、上側だけ残す。
/// 上端が消失ライン以下のときは描画しない。
fn clip_skin_cover_to_disappear_line(
    frame: &mut ResolvedSkinFrame,
    uv: &mut TextureRegion,
    disappear_line: i32,
    link_lift: bool,
    state: SkinDrawState,
) {
    if disappear_line <= 0 || frame.h <= 0 {
        return;
    }
    let mut disappear_y = disappear_line;
    if link_lift {
        disappear_y = disappear_y.saturating_add(state.offset_lift_px);
    }
    let bottom = frame.y;
    let top = bottom.saturating_add(frame.h);
    if top < disappear_y {
        frame.h = 0;
        return;
    }
    // 下端が消失ライン以上なら加工不要 (SUDDEN+ の全開など)
    if bottom >= disappear_y {
        return;
    }
    if top <= disappear_y {
        return;
    }
    // 消失ラインより下 (y が小さい側) だけ切り、上側を残す
    let original_h = frame.h.max(1);
    let new_h = top - disappear_y;
    let ratio = new_h as f32 / original_h as f32;
    frame.y = disappear_y;
    frame.h = new_h;
    uv.height *= ratio;
}

fn normalize_skin_frame_rect(
    frame: ResolvedSkinFrame,
    canvas_width: u32,
    canvas_height: u32,
) -> Rect {
    let canvas_width = canvas_width.max(1) as f32;
    let canvas_height = canvas_height.max(1) as f32;
    let x0 = frame.x as f32;
    let x1 = (frame.x + frame.w) as f32;
    let y0 = frame.y as f32;
    let y1 = (frame.y + frame.h) as f32;
    Rect {
        x: x0.min(x1) / canvas_width,
        y: (canvas_height - y0.max(y1)) / canvas_height,
        width: (x1 - x0).abs() / canvas_width,
        height: (y1 - y0).abs() / canvas_height,
    }
}

fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    rect.x <= x && x <= rect.x + rect.width && rect.y <= y && y <= rect.y + rect.height
}

fn destination_mouse_rect_contains(
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    state: SkinDrawState,
) -> bool {
    let Some(mouse_rect) = destination.mouse_rect else {
        return true;
    };
    let (Some(mouse_x), Some(mouse_y)) = (state.mouse_x, state.mouse_y) else {
        return true;
    };
    let relative_x = mouse_x - frame.x as f32;
    let relative_y = mouse_y - frame.y as f32;
    let x0 = mouse_rect.x as f32;
    let x1 = (mouse_rect.x + mouse_rect.w) as f32;
    let y0 = mouse_rect.y as f32;
    let y1 = (mouse_rect.y + mouse_rect.h) as f32;
    x0.min(x1) <= relative_x
        && relative_x <= x0.max(x1)
        && y0.min(y1) <= relative_y
        && relative_y <= y0.max(y1)
}

fn slider_value_at(
    slider: &SkinSliderDef,
    frame: ResolvedSkinFrame,
    x: f32,
    y: f32,
) -> Option<f32> {
    let range = slider.range.unsigned_abs() as f32;
    if range <= f32::EPSILON {
        return None;
    }
    let frame_x = frame.x as f32;
    let frame_y = frame.y as f32;
    let frame_w = frame.w as f32;
    let frame_h = frame.h as f32;
    let value = match slider.angle {
        0 if frame_x <= x && x <= frame_x + frame_w && frame_y <= y && y <= frame_y + range => {
            (y - frame_y) / range
        }
        1 if frame_x <= x && x <= frame_x + range && frame_y <= y && y <= frame_y + frame_h => {
            (x - frame_x) / range
        }
        2 if frame_x <= x && x <= frame_x + frame_w && frame_y - range <= y && y <= frame_y => {
            (frame_y - y) / range
        }
        3 if frame_x - range <= x && x <= frame_x && frame_y <= y && y <= frame_y + frame_h => {
            (frame_x - x) / range
        }
        _ => return None,
    };
    Some(value.clamp(0.0, 1.0))
}

fn multiply_bga_tints(destination: Color, bga: SkinBgaFrame) -> Color {
    Color::rgba(
        destination.r * bga.tint_r,
        destination.g * bga.tint_g,
        destination.b * bga.tint_b,
        destination.a * bga.tint_a,
    )
}

fn bga_image_item(
    bga: SkinBgaFrame,
    stretch: i32,
    rect: Rect,
    tint: Color,
    blend: BlendMode,
    canvas_width: u32,
    canvas_height: u32,
    linear_filter: bool,
) -> SkinRenderItem {
    let (rect, uv) = stretch_skin_image_geometry(
        stretch,
        rect,
        TextureRegion::default(),
        bga.source_size,
        canvas_width,
        canvas_height,
    );
    SkinRenderItem::Image {
        texture: bga.texture,
        rect,
        uv,
        tint,
        blend,
        scale: SkinImageScale::Stretch,
        border: None,
        source_size: Some(bga.source_size),
        linear_filter,
    }
}

fn special_image_render_item(
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    canvas_width: u32,
    canvas_height: u32,
) -> Option<SkinRenderItem> {
    let (base_r, base_g, base_b) = match destination.id.as_str() {
        "-110" => (0.0, 0.0, 0.0),
        "-111" => (1.0, 1.0, 1.0),
        _ => return None,
    };
    Some(SkinRenderItem::Rect {
        rect: normalize_skin_frame_rect(frame, canvas_width, canvas_height),
        color: Color::rgba(
            base_r * frame.r as f32 / 255.0,
            base_g * frame.g as f32 / 255.0,
            base_b * frame.b as f32 / 255.0,
            frame.a as f32 / 255.0,
        ),
        blend: if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal },
    })
}

fn stretch_skin_image_geometry(
    stretch: i32,
    rect: Rect,
    uv: TextureRegion,
    source_size: SkinImageSize,
    canvas_width: u32,
    canvas_height: u32,
) -> (Rect, TextureRegion) {
    if stretch <= 0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return (rect, uv);
    }

    let canvas_width = canvas_width.max(1) as f32;
    let canvas_height = canvas_height.max(1) as f32;
    let source_width = (uv.width.abs() * source_size.width).max(1.0);
    let source_height = (uv.height.abs() * source_size.height).max(1.0);
    let rect_px = SkinPixelRect {
        x: rect.x * canvas_width,
        y: rect.y * canvas_height,
        width: rect.width * canvas_width,
        height: rect.height * canvas_height,
    };

    let (rect_px, uv) = match stretch {
        1 => (fit_inner_rect(rect_px, source_width, source_height), uv),
        2 => (fit_outer_rect(rect_px, source_width, source_height), uv),
        3 => fit_outer_trimmed_rect(rect_px, uv, source_width, source_height),
        4 => (fit_width_rect(rect_px, source_width, source_height), uv),
        5 => fit_width_trimmed_rect(rect_px, uv, source_width, source_height),
        6 => (fit_height_rect(rect_px, source_width, source_height), uv),
        7 => fit_height_trimmed_rect(rect_px, uv, source_width, source_height),
        8 => (fit_no_expanding_rect(rect_px, source_width, source_height), uv),
        9 => (resize_about_center(rect_px, source_width, source_height), uv),
        10 => fit_no_resize_trimmed_rect(rect_px, uv, source_width, source_height),
        _ => (rect_px, uv),
    };

    (
        Rect {
            x: rect_px.x / canvas_width,
            y: rect_px.y / canvas_height,
            width: rect_px.width / canvas_width,
            height: rect_px.height / canvas_height,
        },
        uv,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SkinPixelRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn fit_inner_rect(rect: SkinPixelRect, source_width: f32, source_height: f32) -> SkinPixelRect {
    let scale_x = rect.width / source_width;
    let scale_y = rect.height / source_height;
    if scale_x <= scale_y {
        resize_about_center(rect, rect.width, source_height * scale_x)
    } else {
        resize_about_center(rect, source_width * scale_y, rect.height)
    }
}

fn fit_outer_rect(rect: SkinPixelRect, source_width: f32, source_height: f32) -> SkinPixelRect {
    let scale_x = rect.width / source_width;
    let scale_y = rect.height / source_height;
    if scale_x >= scale_y {
        resize_about_center(rect, rect.width, source_height * scale_x)
    } else {
        resize_about_center(rect, source_width * scale_y, rect.height)
    }
}

fn fit_width_rect(rect: SkinPixelRect, source_width: f32, source_height: f32) -> SkinPixelRect {
    resize_about_center(rect, rect.width, source_height * rect.width / source_width)
}

fn fit_height_rect(rect: SkinPixelRect, source_width: f32, source_height: f32) -> SkinPixelRect {
    resize_about_center(rect, source_width * rect.height / source_height, rect.height)
}

fn fit_no_expanding_rect(
    rect: SkinPixelRect,
    source_width: f32,
    source_height: f32,
) -> SkinPixelRect {
    let scale = (rect.width / source_width).min(rect.height / source_height).min(1.0);
    resize_about_center(rect, source_width * scale, source_height * scale)
}

fn fit_outer_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let scale_x = rect.width / source_width;
    let scale_y = rect.height / source_height;
    if scale_x >= scale_y {
        fit_height_or_trim(rect, uv, source_height * scale_x)
    } else {
        fit_width_or_trim(rect, uv, source_width * scale_y)
    }
}

fn fit_width_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let scale = rect.width / source_width;
    fit_height_or_trim(rect, uv, source_height * scale)
}

fn fit_height_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let scale = rect.height / source_height;
    fit_width_or_trim(rect, uv, source_width * scale)
}

fn fit_no_resize_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let (rect, uv) = fit_width_or_trim(rect, uv, source_width);
    fit_height_or_trim(rect, uv, source_height)
}

fn fit_width_or_trim(
    rect: SkinPixelRect,
    uv: TextureRegion,
    target_width: f32,
) -> (SkinPixelRect, TextureRegion) {
    if rect.width < target_width {
        let visible_ratio = (rect.width / target_width).clamp(0.0, 1.0);
        let trim = uv.width * (1.0 - visible_ratio) * 0.5;
        (rect, TextureRegion { x: uv.x + trim, width: uv.width - trim * 2.0, ..uv })
    } else {
        (resize_about_center(rect, target_width, rect.height), uv)
    }
}

fn fit_height_or_trim(
    rect: SkinPixelRect,
    uv: TextureRegion,
    target_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    if rect.height < target_height {
        let visible_ratio = (rect.height / target_height).clamp(0.0, 1.0);
        let trim = uv.height * (1.0 - visible_ratio) * 0.5;
        (rect, TextureRegion { y: uv.y + trim, height: uv.height - trim * 2.0, ..uv })
    } else {
        (resize_about_center(rect, rect.width, target_height), uv)
    }
}

fn resize_about_center(rect: SkinPixelRect, width: f32, height: f32) -> SkinPixelRect {
    SkinPixelRect {
        x: rect.x + (rect.width - width) * 0.5,
        y: rect.y + (rect.height - height) * 0.5,
        width,
        height,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedSkinFrame {
    time: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    acc: i32,
    a: i32,
    r: i32,
    g: i32,
    b: i32,
    angle: i32,
}

impl Default for ResolvedSkinFrame {
    fn default() -> Self {
        Self { time: 0, x: 0, y: 0, w: 0, h: 0, acc: 0, a: 255, r: 255, g: 255, b: 255, angle: 0 }
    }
}

fn default_skin_canvas_width() -> u32 {
    1280
}

fn default_skin_canvas_height() -> u32 {
    720
}

fn default_judgetimer() -> i32 {
    1
}

fn default_grid_division() -> i32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_graph_angle() -> i32 {
    1
}

/// beatoraja `SkinGauge.ANIMATION_*` (JSON `gauge.type` フィールド)。
const SKIN_GAUGE_ANIM_RANDOM: i32 = 0;
const SKIN_GAUGE_ANIM_DECREASE: i32 = 2;
const SKIN_GAUGE_ANIM_FLICKERING: i32 = 3;
const SKIN_GAUGE_ANIM_INCREASE: i32 = 1;

/// beatoraja `SkinGauge.draw` の `exgauge = (type >= CLASS ? type - 3 : type) * 6`。
fn skin_gauge_node_base(gameplay_gauge_type: i32) -> usize {
    let adjusted =
        if gameplay_gauge_type >= 6 { gameplay_gauge_type - 3 } else { gameplay_gauge_type };
    adjusted.max(0) as usize * 6
}

fn skin_gauge_notes_count(gauge: f32, parts: i32, max: f32) -> i32 {
    if gauge > 0.0 { ((gauge * parts as f32 / max.max(1.0)) as i32).max(1) } else { 0 }
}

fn skin_gauge_frame_color(frame: ResolvedSkinFrame) -> Color {
    Color::rgba(
        frame.r as f32 / 255.0,
        frame.g as f32 / 255.0,
        frame.b as f32 / 255.0,
        frame.a as f32 / 255.0,
    )
}

fn skin_gauge_destination_blend(destination: &SkinDestinationDef) -> BlendMode {
    if destination.blend == 2 { BlendMode::Add } else { BlendMode::Normal }
}

fn skin_gauge_animation_index(gauge_def: &SkinGaugeDef, state: SkinDrawState) -> i32 {
    let cycle = gauge_def.cycle.max(1);
    let range = gauge_def.range.max(0);
    match gauge_def.gauge_type {
        SKIN_GAUGE_ANIM_RANDOM => {
            let tick = skin_gauge_animation_tick(state, cycle);
            skin_gauge_random_animation_index(tick, range)
        }
        SKIN_GAUGE_ANIM_FLICKERING => {
            let time = state.play_timer_ms.unwrap_or(state.elapsed_ms);
            time.rem_euclid(cycle)
        }
        SKIN_GAUGE_ANIM_INCREASE => {
            let tick = skin_gauge_animation_tick(state, cycle);
            (tick * range).rem_euclid(range + 1)
        }
        SKIN_GAUGE_ANIM_DECREASE => {
            let tick = skin_gauge_animation_tick(state, cycle);
            tick.rem_euclid(range + 1)
        }
        _ => 0,
    }
}

fn skin_gauge_animation_tick(state: SkinDrawState, cycle: i32) -> i32 {
    let time = state.play_timer_ms.unwrap_or(state.elapsed_ms);
    time.div_euclid(cycle.max(1))
}

fn skin_gauge_random_animation_index(tick: i32, range: i32) -> i32 {
    let span = range + 1;
    if span <= 1 {
        return 0;
    }
    let mut value = tick as u32;
    value ^= value.wrapping_shl(13);
    value ^= value.wrapping_shr(17);
    value ^= value.wrapping_shl(5);
    (value % span as u32) as i32
}

/// beatoraja `SkinGauge.draw` のスプライト選択 (`exgauge + offset + underclear`)。
fn skin_gauge_sprite_node_index(
    exgauge: usize,
    part: i32,
    notes: i32,
    animation: i32,
    border: f32,
    part_border: f32,
    node_count: usize,
    anim_type: i32,
) -> usize {
    let offset = if anim_type == SKIN_GAUGE_ANIM_FLICKERING {
        if notes >= part { 0 } else { 2 }
    } else if notes == part {
        4
    } else if notes - animation > part {
        0
    } else {
        2
    };
    let underclear = if part_border < border { 1 } else { 0 };
    (exgauge + offset + underclear).min(node_count.saturating_sub(1))
}

fn skin_gauge_flicker_tip_node_index(
    exgauge: usize,
    border: f32,
    part_border: f32,
    node_count: usize,
) -> Option<usize> {
    let underclear = if part_border < border { 1 } else { 0 };
    Some((exgauge + 4 + underclear).min(node_count.saturating_sub(1)))
}

/// beatoraja `SkinGauge` FLICKERING の先端 α (`duration` = JSON `gauge.cycle`)。
///
/// `orgAlpha * (animation < duration/2 ? animation/(duration/2-1) : (duration-1-animation)/(duration/2-1))`
fn skin_gauge_flicker_alpha(animation: i32, duration: i32) -> f32 {
    let duration = duration.max(1);
    let half = (duration / 2).max(1);
    let denom = (half - 1).max(1) as f32;
    if animation < half {
        animation as f32 / denom
    } else {
        ((duration - 1) - animation) as f32 / denom
    }
}

fn skin_gauge_part_rect(rect: Rect, parts: i32, part: i32) -> Rect {
    if rect.width.abs() >= rect.height.abs() {
        let part_width = rect.width / parts as f32;
        Rect {
            x: rect.x + part_width * (part - 1) as f32,
            y: rect.y,
            width: part_width,
            height: rect.height,
        }
    } else {
        let part_height = rect.height / parts as f32;
        Rect {
            x: rect.x,
            y: rect.y + rect.height - part_height * part as f32,
            width: rect.width,
            height: part_height,
        }
    }
}

fn default_skin_gauge_animation_type() -> i32 {
    0
}

fn default_gauge_parts() -> i32 {
    50
}

fn default_gauge_range() -> i32 {
    3
}

fn default_gauge_cycle() -> i32 {
    33
}

fn default_gauge_endtime() -> i32 {
    500
}

fn default_stretch() -> i32 {
    -1
}

/// Starseeker 等の閉店 Lua は `src = "bg"` / `src = 0` と書くが、実体は `system.png`。
fn resolve_document_source<'a>(
    sources: &'a HashMap<String, SkinDocumentTexture>,
    src: &str,
) -> Option<&'a SkinDocumentTexture> {
    if let Some(texture) = sources.get(src) {
        return Some(texture);
    }
    match src {
        "bg" | "0" => sources.get("system"),
        _ => None,
    }
}

fn destination_render_layer<'a>(
    timer: Option<i32>,
    after_notes_marker: bool,
    behind: &'a mut Vec<SkinRenderItem>,
    front: &'a mut Vec<SkinRenderItem>,
    failed_overlay: &'a mut Vec<SkinRenderItem>,
) -> &'a mut Vec<SkinRenderItem> {
    if timer == Some(3) {
        failed_overlay
    } else if after_notes_marker {
        front
    } else {
        behind
    }
}

fn deserialize_skin_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(SkinIdVisitor)
}

fn deserialize_skin_string<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(SkinIdVisitor)
}

fn deserialize_skin_frame_expr_opt<'de, D>(
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

fn parse_skin_frame_expr(expr: &str) -> std::result::Result<SkinFrameExpr, String> {
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
fn deserialize_op_codes<'de, D>(deserializer: D) -> std::result::Result<Vec<i32>, D::Error>
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

fn deserialize_skin_id_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
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

#[cfg(test)]
mod tests {
    use bmz_core::time::TimeUs;

    use crate::plan::TextLayer;

    use super::*;

    fn judge_region_state(region: usize, ms: i32, image_index: usize) -> JudgeRegionState {
        let mut judge_ms = [None; MAX_JUDGE_REGIONS];
        let mut judge_index = [None; MAX_JUDGE_REGIONS];
        let mut judge_combo = [0; MAX_JUDGE_REGIONS];
        let mut judge_timing_sign = [None; MAX_JUDGE_REGIONS];
        if region < MAX_JUDGE_REGIONS {
            judge_ms[region] = Some(ms);
            judge_index[region] = Some(image_index);
            judge_combo[region] = 42;
            judge_timing_sign[region] = Some(1);
        }
        JudgeRegionState { judge_ms, judge_index, judge_combo, judge_timing_sign }
    }

    #[test]
    fn number_object_resolves_to_padded_text() {
        let object = SkinObject {
            id: SkinObjectId(1),
            source: SkinSource::Number {
                slot: NumberSlot::ExScore,
                style: TextStyle {
                    font_id: None,
                    size: 0.04,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
                digits: 4,
            },
            placements: vec![SkinPlacement {
                phase: SkinPhase::Result,
                time_ms: 0,
                rect: Rect { x: 0.1, y: 0.2, width: 0.2, height: 0.05 },
                alpha: 0.5,
                blend: BlendMode::Normal,
                animation: Animation::none(),
            }],
        };

        let items = object.resolve(SkinPhase::Result, 0, |_| String::new(), |_| 123);

        assert!(matches!(
            &items[0],
            SkinRenderItem::Text { text, style, .. }
                if text == "0123" && style.color.a == 0.5
        ));
    }

    #[test]
    fn placement_uses_latest_animation_keyframe() {
        let placement = SkinPlacement {
            phase: SkinPhase::Play,
            time_ms: 0,
            rect: Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
            alpha: 1.0,
            blend: BlendMode::Normal,
            animation: Animation {
                keyframes: vec![
                    Keyframe {
                        time_ms: 0,
                        rect: Rect { x: 0.1, y: 0.0, width: 0.1, height: 0.1 },
                        alpha: 1.0,
                    },
                    Keyframe {
                        time_ms: 100,
                        rect: Rect { x: 0.2, y: 0.0, width: 0.1, height: 0.1 },
                        alpha: 0.8,
                    },
                ],
            },
        };

        assert_eq!(placement.resolve(120).rect.x, 0.2);
    }

    #[test]
    fn skin_definition_resolves_context_values() {
        let skin = SkinDefinition {
            objects: vec![SkinObject {
                id: SkinObjectId(1),
                source: SkinSource::Text {
                    slot: TextSlot::Judge,
                    style: TextStyle {
                        font_id: None,
                        size: 0.04,
                        bitmap_size: None,
                        color: Color::rgb(1.0, 1.0, 1.0),
                        layer: TextLayer::Skin,
                        align: TextAlign::Left,
                        max_width: 0.0,
                        overflow: TextOverflow::Overflow,
                        wrapping: false,
                        outline: None,
                        shadow: None,
                    },
                },
                placements: vec![SkinPlacement {
                    phase: SkinPhase::Play,
                    time_ms: 0,
                    rect: Rect { x: 0.3, y: 0.4, width: 0.2, height: 0.05 },
                    alpha: 1.0,
                    blend: BlendMode::Normal,
                    animation: Animation::none(),
                }],
            }],
        };
        let context = SkinRenderContext {
            phase: SkinPhase::Play,
            elapsed_ms: 12,
            text: &[(TextSlot::Judge, "PGREAT FAST".to_string())],
            numbers: &[],
        };

        let items = skin.resolve(&context);

        assert!(matches!(&items[0], SkinRenderItem::Text { text, .. } if text == "PGREAT FAST"));
    }

    #[test]
    fn append_skin_render_items_emits_image_commands() {
        let mut commands = Vec::new();
        append_skin_render_items(
            &mut commands,
            &[
                SkinRenderItem::Rect {
                    rect: Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                    color: Color::rgb(1.0, 1.0, 1.0),
                    blend: BlendMode::Normal,
                },
                SkinRenderItem::Image {
                    texture: SkinTextureId(1),
                    rect: Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                    uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    tint: Color::rgb(1.0, 1.0, 1.0),
                    blend: BlendMode::Add,
                    scale: SkinImageScale::Stretch,
                    border: None,
                    source_size: None,
                    linear_filter: false,
                },
            ],
        );

        assert_eq!(commands.len(), 2);
        assert!(matches!(
            commands[1],
            DrawCommand::Image { texture: TextureId(1), blend: BlendMode::Add, .. }
        ));
    }

    #[test]
    fn append_skin_render_items_expands_nine_slice_images() {
        let mut commands = Vec::new();
        append_skin_render_items(
            &mut commands,
            &[SkinRenderItem::Image {
                texture: SkinTextureId(10),
                rect: Rect { x: 0.1, y: 0.2, width: 0.6, height: 0.3 },
                uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                scale: SkinImageScale::NineSlice,
                border: Some(SkinImageBorder {
                    left: 0.1,
                    right: 0.2,
                    top: 0.25,
                    bottom: 0.25,
                    unit: SkinImageBorderUnit::Normalized,
                }),
                source_size: None,
                linear_filter: false,
            }],
        );

        assert_eq!(commands.len(), 9);
        assert!(matches!(
            commands[0],
            DrawCommand::Image {
                rect: Rect { x: 0.1, y: 0.2, width, height },
                uv: UvRect { x: 0.0, y: 0.0, width: uv_width, height: uv_height },
                texture: TextureId(10),
                ..
            } if approx_eq(width, 0.06)
                && approx_eq(height, 0.075)
                && approx_eq(uv_width, 0.1)
                && approx_eq(uv_height, 0.25)
        ));
        assert!(matches!(
            commands[4],
            DrawCommand::Image {
                rect: Rect { width, height, .. },
                uv: UvRect { width: uv_width, height: uv_height, .. },
                texture: TextureId(10),
                ..
            } if approx_eq(width, 0.42)
                && approx_eq(height, 0.15)
                && approx_eq(uv_width, 0.7)
                && approx_eq(uv_height, 0.5)
        ));
    }

    #[test]
    fn append_skin_render_items_expands_pixel_based_nine_slice_images() {
        let mut commands = Vec::new();
        append_skin_render_items(
            &mut commands,
            &[SkinRenderItem::Image {
                texture: SkinTextureId(8),
                rect: Rect { x: 0.2, y: 0.1, width: 0.36, height: 0.48 },
                uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                scale: SkinImageScale::NineSlice,
                border: Some(SkinImageBorder {
                    left: 2.0,
                    right: 2.0,
                    top: 3.0,
                    bottom: 3.0,
                    unit: SkinImageBorderUnit::Pixels,
                }),
                source_size: Some(SkinImageSize { width: 12.0, height: 48.0 }),
                linear_filter: false,
            }],
        );

        assert_eq!(commands.len(), 9);
        assert!(matches!(
            commands[0],
            DrawCommand::Image {
                rect: Rect { width, height, .. },
                uv: UvRect { width: uv_width, height: uv_height, .. },
                ..
            } if approx_eq(width, 0.06)
                && approx_eq(height, 0.03)
                && approx_eq(uv_width, 2.0 / 12.0)
                && approx_eq(uv_height, 3.0 / 48.0)
        ));
    }

    #[test]
    fn skin_manifest_resolves_relative_texture_paths() {
        let manifest: SkinManifest = toml::from_str(
            r#"
            [[textures]]
            id = 1
            path = "note.png"

            [[textures]]
            id = 2
            path = "note-blue.png"

            [[textures]]
            id = 3
            path = "note-red.png"

            [[textures]]
            id = 4
            path = "receptor.png"

            [[textures]]
            id = 5
            path = "receptor-blue.png"

            [[textures]]
            id = 6
            path = "receptor-red.png"

            [[textures]]
            id = 7
            path = "judge-line.png"

            [[textures]]
            id = 8
            path = "gauge-frame.png"

            [[textures]]
            id = 9
            path = "gauge-fill.png"

            [[textures]]
            id = 10
            path = "combo-panel.png"

            [[textures]]
            id = 11
            path = "combo-panel-inactive.png"

            [play.note]
            texture = 1
            key_even_texture = 2
            scratch_texture = 3

            [play.receptor]
            texture = 4
            key_even_texture = 5
            scratch_texture = 6

            [play.judge_line]
            texture = 7
            scale = "stretch"

            [play.gauge_frame]
            texture = 8
            source_size = { width = 12.0, height = 48.0 }
            scale = "nine_slice"
            border = { left = 2.0, right = 2.0, top = 3.0, bottom = 3.0, unit = "pixels" }

            [play.gauge_fill]
            texture = 9
            source_size = { width = 8.0, height = 48.0 }
            scale = "stretch"

            [play.combo_panel]
            texture = 10
            source_size = { width = 48.0, height = 16.0 }
            scale = "nine_slice"

            [play.combo_panel_inactive]
            texture = 11
            source_size = { width = 48.0, height = 16.0 }
            scale = "stretch"
            "#,
        )
        .unwrap();

        let textures = manifest.resolve_textures(Path::new("/skin/default"));

        assert_eq!(textures[0].id, TextureId(1));
        assert_eq!(textures[0].path, PathBuf::from("/skin/default/note.png"));
        assert_eq!(textures[1].id, TextureId(2));
        assert_eq!(textures[1].path, PathBuf::from("/skin/default/note-blue.png"));
        assert_eq!(textures[2].id, TextureId(3));
        assert_eq!(textures[2].path, PathBuf::from("/skin/default/note-red.png"));
        assert_eq!(textures[3].id, TextureId(4));
        assert_eq!(textures[3].path, PathBuf::from("/skin/default/receptor.png"));
        assert_eq!(textures[4].id, TextureId(5));
        assert_eq!(textures[4].path, PathBuf::from("/skin/default/receptor-blue.png"));
        assert_eq!(textures[5].id, TextureId(6));
        assert_eq!(textures[5].path, PathBuf::from("/skin/default/receptor-red.png"));
        assert_eq!(textures[6].id, TextureId(7));
        assert_eq!(textures[6].path, PathBuf::from("/skin/default/judge-line.png"));
        assert_eq!(textures[7].id, TextureId(8));
        assert_eq!(textures[7].path, PathBuf::from("/skin/default/gauge-frame.png"));
        assert_eq!(textures[8].id, TextureId(9));
        assert_eq!(textures[8].path, PathBuf::from("/skin/default/gauge-fill.png"));
        assert_eq!(textures[9].id, TextureId(10));
        assert_eq!(textures[9].path, PathBuf::from("/skin/default/combo-panel.png"));
        assert_eq!(textures[10].id, TextureId(11));
        assert_eq!(textures[10].path, PathBuf::from("/skin/default/combo-panel-inactive.png"));
        assert_eq!(manifest.play_note_image().texture_for_lane(Lane::Key2), 2);
        assert_eq!(manifest.play_note_image().texture_for_lane(Lane::Scratch), 3);
        assert_eq!(manifest.play_receptor_image().texture_for_lane(Lane::Key2), 5);
        assert_eq!(manifest.play_receptor_image().texture_for_lane(Lane::Scratch), 6);
        assert_eq!(manifest.play_judge_line_image().texture, 7);
        assert_eq!(manifest.play_gauge_frame_image().texture, 8);
        assert_eq!(manifest.play_gauge_frame_image().scale, SkinImageScale::NineSlice);
        assert_eq!(
            manifest.play_gauge_frame_image().source_size,
            Some(SkinImageSize { width: 12.0, height: 48.0 })
        );
        assert_eq!(
            manifest.play_gauge_frame_image().border,
            Some(SkinImageBorder {
                left: 2.0,
                right: 2.0,
                top: 3.0,
                bottom: 3.0,
                unit: SkinImageBorderUnit::Pixels,
            })
        );
        assert_eq!(manifest.play_gauge_fill_image().texture, 9);
        assert_eq!(manifest.play_combo_panel_image(true).texture, 10);
        assert_eq!(manifest.play_combo_panel_image(true).scale, SkinImageScale::NineSlice);
        assert_eq!(manifest.play_combo_panel_image(false).texture, 11);
    }

    #[test]
    fn bundled_default_skin_manifest_defines_play_lane_images() {
        let manifest = default_skin_manifest();
        let note = manifest.play_note_image();
        let receptor = manifest.play_receptor_image();
        let judge_line = manifest.play_judge_line_image();
        let gauge_frame = manifest.play_gauge_frame_image();
        let gauge_fill = manifest.play_gauge_fill_image();
        let combo_panel = manifest.play_combo_panel_image(true);
        let combo_panel_inactive = manifest.play_combo_panel_image(false);

        assert_eq!(note.texture, 1);
        assert_eq!(note.texture_for_lane(Lane::Key1), 1);
        assert_eq!(note.texture_for_lane(Lane::Key2), 2);
        assert_eq!(note.texture_for_lane(Lane::Key4), 2);
        assert_eq!(note.texture_for_lane(Lane::Key6), 2);
        assert_eq!(note.texture_for_lane(Lane::Scratch), 3);
        assert_eq!(note.uv, TextureRegion::default());
        assert_eq!(receptor.texture, 4);
        assert_eq!(receptor.texture_for_lane(Lane::Key1), 4);
        assert_eq!(receptor.texture_for_lane(Lane::Key2), 5);
        assert_eq!(receptor.texture_for_lane(Lane::Key4), 5);
        assert_eq!(receptor.texture_for_lane(Lane::Key6), 5);
        assert_eq!(receptor.texture_for_lane(Lane::Scratch), 6);
        assert_eq!(receptor.uv, TextureRegion::default());
        assert_eq!(judge_line.texture, 7);
        assert_eq!(judge_line.uv, TextureRegion::default());
        assert_eq!(gauge_frame.texture, 8);
        assert_eq!(gauge_frame.scale, SkinImageScale::NineSlice);
        assert_eq!(gauge_frame.source_size, Some(SkinImageSize { width: 12.0, height: 48.0 }));
        assert!(matches!(
            gauge_frame.border,
            Some(SkinImageBorder { unit: SkinImageBorderUnit::Pixels, .. })
        ));
        assert_eq!(gauge_fill.texture, 9);
        assert_eq!(gauge_fill.source_size, Some(SkinImageSize { width: 8.0, height: 48.0 }));
        assert_eq!(combo_panel.texture, 10);
        assert_eq!(combo_panel.scale, SkinImageScale::NineSlice);
        assert_eq!(combo_panel.source_size, Some(SkinImageSize { width: 48.0, height: 16.0 }));
        assert!(matches!(
            combo_panel.border,
            Some(SkinImageBorder { unit: SkinImageBorderUnit::Pixels, .. })
        ));
        assert_eq!(combo_panel_inactive.texture, 11);
        assert_eq!(combo_panel_inactive.scale, SkinImageScale::NineSlice);
        assert_eq!(
            combo_panel_inactive.source_size,
            Some(SkinImageSize { width: 48.0, height: 16.0 })
        );
        assert!(matches!(
            combo_panel_inactive.border,
            Some(SkinImageBorder { unit: SkinImageBorderUnit::Pixels, .. })
        ));
    }

    #[test]
    fn skin_document_normalizes_numeric_and_string_ids() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "source": [
                    { "id": 100, "path": "a.png" },
                    { "id": "100", "path": "b.png" }
                ],
                "image": [
                    { "id": 200, "src": 100, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "300", "src": "100", "x": 8, "y": 0, "w": 8, "h": 8 }
                ],
                "imageset": [
                    { "id": "set", "images": [200, "300"] }
                ],
                "destination": [
                    { "id": 200, "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

        assert_eq!(document.source[0].id, "100");
        assert_eq!(document.source[1].id, "100");
        assert_eq!(document.image[0].id, "200");
        assert_eq!(document.image[0].src, "100");
        assert_eq!(document.image[1].src, "100");
        assert_eq!(document.imageset[0].images, ["200", "300"]);
        let DestinationListEntry::Single(dst0) = &document.destination[0] else {
            panic!("expected Single destination");
        };
        assert_eq!(dst0.id, "200");
    }

    #[test]
    fn bga_destination_renders_placeholder_only_when_chart_has_bga() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40, "a": 128 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let no_bga_items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState { has_bga: false, ..SkinDrawState::default() },
            SkinTextState::default(),
        );
        let bga_items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState { has_bga: true, ..SkinDrawState::default() },
            SkinTextState::default(),
        );

        assert!(no_bga_items.is_empty());
        assert!(matches!(
            bga_items.as_slice(),
            [SkinRenderItem::Rect {
                rect: Rect { x, y, width, height },
                color: Color { r: 0.0, g: 0.0, b: 0.0, a },
                ..
            }] if approx_eq(*x, 0.1)
                && approx_eq(*y, 0.4)
                && approx_eq(*width, 0.3)
                && approx_eq(*height, 0.4)
                && approx_eq(*a, 128.0 / 255.0)
        ));
    }

    #[test]
    fn bga_destination_is_hidden_when_bga_is_disabled() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState {
                has_bga: true,
                bga_enabled: false,
                bga_base: Some(SkinBgaFrame {
                    texture: SkinTextureId(20000),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                ..SkinDrawState::default()
            },
            SkinTextState::default(),
        );

        assert!(items.is_empty());
    }

    #[test]
    fn bga_option_conditions_still_reflect_song_bga_when_disabled() {
        assert!(!test_skin_op(
            170,
            &[],
            SkinDrawState { has_bga: true, bga_enabled: false, ..SkinDrawState::default() }
        ));
        assert!(test_skin_op(
            171,
            &[],
            SkinDrawState { has_bga: true, bga_enabled: false, ..SkinDrawState::default() }
        ));
    }

    #[test]
    fn difficulty_ops_reflect_chart_difficulty_code() {
        let unknown = SkinDrawState::default();
        let normal = SkinDrawState { difficulty: 2, ..SkinDrawState::default() };
        let insane = SkinDrawState { difficulty: 5, ..SkinDrawState::default() };

        assert!(test_skin_op(150, &[], unknown));
        assert!(!test_skin_op(150, &[], normal));
        assert!(test_skin_op(152, &[], normal));
        assert!(!test_skin_op(153, &[], normal));
        assert!(test_skin_op(155, &[], insane));
    }

    #[test]
    fn select_row_bar_image_index_unowned_song_uses_nograde() {
        let owned = SelectRowSnapshot { in_library: true, ..SelectRowSnapshot::default() };
        let unowned = SelectRowSnapshot { in_library: false, ..SelectRowSnapshot::default() };
        assert_eq!(select_row_bar_image_index(&owned), 0);
        assert_eq!(select_row_bar_image_index(&unowned), 4);
    }

    #[test]
    fn select_row_bar_text_index_unowned_song_uses_no_songs_text() {
        let owned = SelectRowSnapshot { in_library: true, ..SelectRowSnapshot::default() };
        let unowned = SelectRowSnapshot { in_library: false, ..SelectRowSnapshot::default() };
        assert_eq!(select_row_bar_text_index(&owned), 2);
        assert_eq!(select_row_bar_text_index(&unowned), 8);
    }

    #[test]
    fn select_bar_type_ops_match_song_folder_and_course_rows() {
        let song = SkinDrawState {
            select_row_kind: SelectRowKind::Song,
            select_is_folder: false,
            ..SkinDrawState::default()
        };
        let folder = SkinDrawState {
            select_row_kind: SelectRowKind::Folder,
            select_is_folder: true,
            ..SkinDrawState::default()
        };
        let table_folder = SkinDrawState {
            select_row_kind: SelectRowKind::TableFolder,
            select_is_folder: true,
            ..SkinDrawState::default()
        };
        let course = SkinDrawState {
            select_row_kind: SelectRowKind::Course,
            select_is_folder: false,
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(2, &[], song));
        assert!(!test_skin_op(1, &[], song));
        assert!(!test_skin_op(3, &[], song));
        assert!(test_skin_op(1, &[], folder));
        assert!(test_skin_op(1, &[], table_folder));
        assert!(!test_skin_op(2, &[], folder));
        assert!(test_skin_op(3, &[], course));
        assert!(!test_skin_op(2, &[], course));
    }

    #[test]
    fn gradebar_constraint_ops_match_course_constraint_flags() {
        let course = SkinDrawState {
            select_row_kind: SelectRowKind::Course,
            select_course_constraints: CourseConstraintFlags {
                mirror: true,
                no_speed: true,
                no_great: true,
                gauge_7k: true,
                hcn: true,
                ..CourseConstraintFlags::default()
            },
            ..SkinDrawState::default()
        };
        let song = SkinDrawState {
            select_row_kind: SelectRowKind::Song,
            select_course_constraints: course.select_course_constraints,
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(1003, &[], course));
        assert!(test_skin_op(1005, &[], course));
        assert!(test_skin_op(1007, &[], course));
        assert!(test_skin_op(1012, &[], course));
        assert!(test_skin_op(1017, &[], course));
        assert!(!test_skin_op(1002, &[], course));
        assert!(!test_skin_op(1016, &[], course));
        assert!(!test_skin_op(1003, &[], song));
        assert!(test_skin_op(-1003, &[], song));
    }

    #[test]
    fn select_row_trophy_index_prefers_achieved_course_trophy_names() {
        let row = SelectRowSnapshot {
            kind: SelectRowKind::Course,
            achieved_trophy_names: vec!["bronzemedal".to_string(), "goldmedal".to_string()],
            ex_score: Some(0),
            total_notes: 100,
            ..SelectRowSnapshot::default()
        };
        assert_eq!(select_row_trophy_index(&row), Some(2));

        let silver = SelectRowSnapshot {
            kind: SelectRowKind::Course,
            achieved_trophy_names: vec!["silvermedal".to_string()],
            ..SelectRowSnapshot::default()
        };
        assert_eq!(select_row_trophy_index(&silver), Some(1));
    }

    #[test]
    fn playable_bar_op_matches_library_presence() {
        let owned_song = SkinDrawState {
            select_is_folder: false,
            select_in_library: true,
            ..SkinDrawState::default()
        };
        let unowned_song = SkinDrawState {
            select_is_folder: false,
            select_in_library: false,
            ..SkinDrawState::default()
        };
        let folder = SkinDrawState {
            select_is_folder: true,
            select_in_library: true,
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(5, &[], owned_song));
        assert!(!test_skin_op(5, &[], unowned_song));
        assert!(!test_skin_op(5, &[], folder));
        assert!(!test_skin_op(-5, &[], owned_song));
        assert!(test_skin_op(-5, &[], unowned_song));
        assert!(test_skin_op(-5, &[], folder));
    }

    #[test]
    fn select_banner_ops_follow_selected_banner_presence() {
        let no_banner = SkinDrawState {
            select_screen: true,
            select_has_banner: false,
            ..SkinDrawState::default()
        };
        let with_banner = SkinDrawState {
            select_screen: true,
            select_has_banner: true,
            ..SkinDrawState::default()
        };
        let play_screen = SkinDrawState {
            select_screen: false,
            select_has_banner: true,
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(192, &[], no_banner));
        assert!(!test_skin_op(193, &[], no_banner));
        assert!(!test_skin_op(192, &[], with_banner));
        assert!(test_skin_op(193, &[], with_banner));
        assert!(!test_skin_op(192, &[], play_screen));
        assert!(!test_skin_op(193, &[], play_screen));

        assert!(test_skin_ops(&[2, 192], &[], no_banner));
        assert!(!test_skin_ops(&[2, 193], &[], no_banner));
        assert!(!test_skin_ops(&[2, 192], &[], with_banner));
        assert!(test_skin_ops(&[2, 193], &[], with_banner));
    }

    #[test]
    fn play_mode_option_ops_reflect_autoplay_and_course_stage() {
        let normal_play = SkinDrawState::default();
        let autoplay = SkinDrawState { autoplay: true, ..SkinDrawState::default() };
        let course_stage1 = SkinDrawState {
            course_stage: Some(CourseStageMarker::Stage1),
            ..SkinDrawState::default()
        };
        let course_final = SkinDrawState {
            course_stage: Some(CourseStageMarker::Final),
            ..SkinDrawState::default()
        };

        // Starseeker freestage: op = {32, -290}
        assert!(test_skin_op(32, &[], normal_play));
        assert!(!test_skin_op(290, &[], normal_play));
        assert!(test_skin_ops(&[32, -290], &[], normal_play));

        // Starseeker auto_play: op = {33}
        assert!(!test_skin_op(33, &[], normal_play));
        assert!(test_skin_op(33, &[], autoplay));

        // Course stage labels
        assert!(test_skin_ops(&[32, 290, 280], &[], course_stage1));
        assert!(!test_skin_ops(&[32, 290, 280], &[], course_final));
        assert!(test_skin_ops(&[32, 290, 289], &[], course_final));

        // beatoraja currently leaves these defined constants without BooleanProperty handlers.
        for op in 291..=293 {
            assert!(
                !test_skin_op(op, &[op], course_stage1),
                "{op} must not fall back to property defaults"
            );
            assert!(test_skin_op(-op, &[op], course_stage1), "negative {op} should invert false");
        }
    }

    #[test]
    fn play_asset_and_loading_ops_reflect_skin_state() {
        let unloaded = SkinDrawState { skin_loaded: false, ..SkinDrawState::default() };
        assert!(test_skin_op(80, &[], unloaded));
        assert!(!test_skin_op(81, &[], unloaded));

        let loaded = SkinDrawState::default();
        assert!(!test_skin_op(80, &[], loaded));
        assert!(test_skin_op(81, &[], loaded));
        assert!(test_skin_op(190, &[], loaded));
        assert!(!test_skin_op(191, &[], loaded));
        assert!(test_skin_op(194, &[], loaded));
        assert!(!test_skin_op(195, &[], loaded));

        let with_backbmp = SkinDrawState { has_backbmp: true, ..SkinDrawState::default() };
        assert!(!test_skin_op(194, &[], with_backbmp));
        assert!(test_skin_op(195, &[], with_backbmp));
    }

    #[test]
    fn lane_cover_changing_op_is_true_while_lane_cover_is_visible() {
        assert!(!test_skin_op(270, &[], SkinDrawState::default()));
        assert!(!test_skin_op(
            270,
            &[],
            SkinDrawState { lane_cover: 0.2, ..SkinDrawState::default() }
        ));
        assert!(test_skin_op(
            270,
            &[],
            SkinDrawState { lane_cover_changing: true, ..SkinDrawState::default() }
        ));
        assert!(test_skin_op(
            271,
            &[],
            SkinDrawState { lanecover_enabled: true, ..SkinDrawState::default() }
        ));
    }

    #[test]
    fn folded_constant_draw_condition_number_zero_is_true() {
        assert!(eval_skin_draw_condition("number(0) >= 0", SkinDrawState::default()));
        assert!(!eval_skin_draw_condition("number(0) < 0", SkinDrawState::default()));
    }

    #[test]
    fn judge_line_with_lift_offset_still_renders_at_minimum_lift() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "line.png" }],
                "image": [{ "id": "judge_line", "src": 12, "w": 431, "h": 8 }],
                "destination": [
                    { "id": "judge_line", "offset": 3, "dst": [{ "time": 0, "x": 20, "y": 357, "w": 431, "h": 8, "a": 255 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 431.0, height: 8.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() },
        );
        assert_eq!(items.len(), 1, "judge_line must not be skipped with liftcover skip logic");
    }

    #[test]
    fn select_rank_ops_reflect_selected_ex_score() {
        let aa_state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Song,
            select_in_library: true,
            select_ex_score: Some(1556),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };
        let max_state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Song,
            select_in_library: true,
            select_ex_score: Some(2000),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };
        let f_state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Song,
            select_in_library: true,
            select_ex_score: Some(300),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(201, &[], aa_state));
        assert!(test_skin_op(302, &[], aa_state));
        assert!(!test_skin_op(200, &[], aa_state));
        assert!(test_skin_op(-200, &[], aa_state));
        assert!(test_skin_op(200, &[], max_state));
        assert!(test_skin_op(300, &[], max_state));
        assert!(test_skin_op(207, &[], f_state));
        assert!(!test_skin_op(307, &[], f_state));
        assert!(!test_skin_op(200, &[], SkinDrawState::default()));
    }

    #[test]
    fn select_rank_ops_are_false_for_folder_rows() {
        let state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Folder,
            select_is_folder: true,
            select_in_library: true,
            select_ex_score: Some(1556),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };

        assert!(!test_skin_op(201, &[], state));
        assert!(!test_skin_op(302, &[], state));
    }

    #[test]
    fn select_key_mode_op_160_requires_song_row_key_mode() {
        let config_row = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Config,
            in_settings: true,
            ..SkinDrawState::default()
        };
        assert!(!test_skin_op(160, &[], config_row));

        let song_7k = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Song,
            select_in_library: true,
            select_chart_key_mode: Some(KeyMode::K7),
            ..SkinDrawState::default()
        };
        assert!(test_skin_op(160, &[], song_7k));
        assert!(!test_skin_op(161, &[], song_7k));
    }

    #[test]
    fn select_settings_screen_hides_bpm_numbers() {
        let state = SkinDrawState {
            select_screen: true,
            in_settings: true,
            select_max_bpm: 180.0,
            select_min_bpm: 120.0,
            ..SkinDrawState::default()
        };
        assert_eq!(skin_state_number(90, state), None);
        assert_eq!(skin_state_number(91, state), None);
    }

    #[test]
    fn select_settings_screen_volume_numbers_match_beatoraja_refs() {
        let state = SkinDrawState {
            select_screen: true,
            in_settings: true,
            select_master_volume: 0.42,
            select_key_volume: 0.73,
            select_bgm_volume: 0.18,
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_number(57, state), Some(42));
        assert_eq!(skin_state_number(58, state), Some(73));
        assert_eq!(skin_state_number(59, state), Some(18));
    }

    #[test]
    fn select_rank_and_judge_ops_are_hidden_in_settings() {
        let state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Config,
            select_in_library: true,
            select_ex_score: Some(1556),
            select_total_notes: 1000,
            judge_rank: Some(2),
            in_settings: true,
            ..SkinDrawState::default()
        };

        assert!(!test_skin_op(200, &[], state));
        assert!(!test_skin_op(201, &[], state));
        assert!(!test_skin_op(302, &[], state));
        assert!(!test_skin_op(180, &[], state));
    }

    #[test]
    fn select_detail_artist_shows_config_value_in_settings() {
        let snapshot = SelectSnapshot {
            in_settings: true,
            settings_editing: true,
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                title: "MASTER".to_string(),
                artist: "25".to_string(),
                kind: SelectRowKind::Config,
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };
        let row = &snapshot.rows[0];
        assert_eq!(select_detail_artist(&snapshot, Some(row)), "25");
        assert_eq!(select_detail_subtitle(&snapshot, Some(row)), "[編集中]");
        assert_eq!(
            skin_state_text(
                &SkinTextDef { id: "t".to_string(), ref_id: 3, ..SkinTextDef::default() },
                SkinTextState { target: "", ..SkinTextState::default() },
            ),
            ""
        );
    }

    #[test]
    fn play_rank_ops_reflect_current_ex_score() {
        let aa_state =
            SkinDrawState { ex_score: 1556, total_notes: 1000, ..SkinDrawState::default() };
        let aaa_state =
            SkinDrawState { ex_score: 1800, total_notes: 1000, ..SkinDrawState::default() };

        assert!(test_skin_op(201, &[], aa_state));
        assert!(!test_skin_op(200, &[], aa_state));
        assert!(test_skin_op(200, &[], aaa_state));
    }

    #[test]
    fn skin_state_number_maps_next_rank_diff() {
        let a_state = SkinDrawState {
            select_ex_score: Some(1300),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };
        let aaa_state = SkinDrawState {
            select_ex_score: Some(1800),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };
        let max_state = SkinDrawState {
            select_ex_score: Some(2000),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_number(154, a_state), Some(34));
        assert_eq!(skin_state_number(154, aaa_state), Some(200));
        assert_eq!(skin_state_number(154, max_state), Some(0));
        assert_eq!(skin_state_number(154, SkinDrawState::default()), None);
    }

    #[test]
    fn select_replay_ops_reflect_replay_slots_and_selection() {
        let no_replay = SkinDrawState::default();
        let first_replay = SkinDrawState {
            select_replay_slots: [true, false, false, false],
            select_replay_index: Some(0),
            ..SkinDrawState::default()
        };
        let second_replay = SkinDrawState {
            select_replay_slots: [false, true, false, false],
            select_replay_index: Some(1),
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(196, &[], no_replay));
        assert!(!test_skin_op(197, &[], no_replay));
        assert!(!test_skin_op(1205, &[], no_replay));
        assert!(test_skin_op(197, &[], first_replay));
        assert!(!test_skin_op(196, &[], first_replay));
        assert!(test_skin_op(1205, &[], first_replay));
        assert!(test_skin_op(-1205, &[], no_replay));
        assert!(test_skin_op(1197, &[], second_replay));
        assert!(test_skin_op(1206, &[], second_replay));
        assert!(!test_skin_op(1205, &[], second_replay));
        assert!(!test_skin_op(198, &[], first_replay));
    }

    #[test]
    fn result_replay_ops_reflect_result_replay_slots() {
        let no_replay = SkinDrawState { result_failed: Some(false), ..SkinDrawState::default() };
        let existing = SkinDrawState {
            result_failed: Some(false),
            result_replay_slots: [true, false, false, false],
            ..SkinDrawState::default()
        };
        let saved = SkinDrawState {
            result_failed: Some(false),
            result_replay_slots: [true, true, false, false],
            result_saved_replay_slots: [true, false, false, false],
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(196, &[], no_replay));
        assert!(!test_skin_op(197, &[], no_replay));
        assert!(!test_skin_op(198, &[], no_replay));
        assert!(test_skin_op(197, &[], existing));
        assert!(!test_skin_op(196, &[], existing));
        assert!(!test_skin_op(198, &[], existing));
        assert!(test_skin_op(198, &[], saved));
        assert!(!test_skin_op(197, &[], saved));
        assert!(test_skin_op(1197, &[], saved));
        assert!(!test_skin_op(1198, &[], saved));
    }

    #[test]
    fn select_row_snapshot_carries_achieved_trophy_names() {
        // SelectRowSnapshot is the carrier — SkinDrawState intentionally does
        // not duplicate this field (it must stay Copy).  This test simply
        // pins down that course rows preserve the data and song rows default
        // to empty, so future skin ops have a stable contract to consume.
        use crate::scene::{SelectRowKind, SelectRowSnapshot};
        let course = SelectRowSnapshot {
            kind: SelectRowKind::Course,
            achieved_trophy_names: vec!["gold".to_string(), "silver".to_string()],
            ..SelectRowSnapshot::default()
        };
        let song = SelectRowSnapshot { kind: SelectRowKind::Song, ..SelectRowSnapshot::default() };

        assert_eq!(course.achieved_trophy_names, vec!["gold".to_string(), "silver".to_string()]);
        assert!(song.achieved_trophy_names.is_empty());
    }

    #[test]
    fn select_row_replay_index_is_row_kind_agnostic() {
        // Regression: course rows must surface their replay slot indicators
        // exactly like song rows.  `select_row_replay_index` looks only at
        // `row.replay_slots`, so swapping row.kind must not change the
        // result.  This locks the invariant for future refactors.
        use crate::scene::{SelectRowKind, SelectRowSnapshot};
        let mut song = SelectRowSnapshot::default();
        song.kind = SelectRowKind::Song;
        song.replay_slots = [false, true, false, true];
        let mut course = SelectRowSnapshot::default();
        course.kind = SelectRowKind::Course;
        course.replay_slots = [false, true, false, true];

        assert_eq!(select_row_replay_index(&song), Some(1));
        assert_eq!(select_row_replay_index(&course), Some(1));
    }

    #[test]
    fn bga_destination_renders_current_bga_images() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "stretch": 1, "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40, "a": 128 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState {
                has_bga: true,
                bga_base: Some(SkinBgaFrame {
                    texture: SkinTextureId(20000),
                    source_size: SkinImageSize { width: 256.0, height: 128.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                bga_layer: Some(SkinBgaFrame {
                    texture: SkinTextureId(20001),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                ..SkinDrawState::default()
            },
            SkinTextState::default(),
        );

        assert!(matches!(
            items.as_slice(),
            [
                SkinRenderItem::Image {
                    texture: SkinTextureId(20000),
                    rect: Rect { x, y, width, height },
                    tint: Color { a, .. },
                    ..
                },
                SkinRenderItem::Image { texture: SkinTextureId(20001), .. },
            ] if approx_eq(*x, 0.1)
                && approx_eq(*y, 0.525)
                && approx_eq(*width, 0.3)
                && approx_eq(*height, 0.15)
                && approx_eq(*a, 128.0 / 255.0)
        ));
    }

    #[test]
    fn bga_destination_renders_poor_bga_instead_of_base_and_layer() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState {
                has_bga: true,
                bga_base: Some(SkinBgaFrame {
                    texture: SkinTextureId(20000),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                bga_layer: Some(SkinBgaFrame {
                    texture: SkinTextureId(20001),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                bga_poor: Some(SkinBgaFrame {
                    texture: SkinTextureId(20002),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                ..SkinDrawState::default()
            },
            SkinTextState::default(),
        );

        assert!(matches!(
            items.as_slice(),
            [SkinRenderItem::Image { texture: SkinTextureId(20002), .. }]
        ));
    }

    #[test]
    fn bga_destination_uses_profile_stretch_when_destination_omits_stretch() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState {
                has_bga: true,
                bga_base: Some(SkinBgaFrame {
                    texture: SkinTextureId(20000),
                    source_size: SkinImageSize { width: 256.0, height: 128.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                bga_stretch: 1,
                ..SkinDrawState::default()
            },
            SkinTextState::default(),
        );

        assert!(matches!(
            items.as_slice(),
            [SkinRenderItem::Image {
                texture: SkinTextureId(20000),
                rect: Rect { x, y, width, height },
                ..
            }] if approx_eq(*x, 0.1)
                && approx_eq(*y, 0.525)
                && approx_eq(*width, 0.3)
                && approx_eq(*height, 0.15)
        ));
    }

    #[test]
    fn bga_destination_stretch_overrides_profile_stretch() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "bga": { "id": "bga" },
                "destination": [
                    { "id": "bga", "stretch": 0, "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState {
                has_bga: true,
                bga_base: Some(SkinBgaFrame {
                    texture: SkinTextureId(20000),
                    source_size: SkinImageSize { width: 256.0, height: 128.0 },
                    tint_r: 1.0,
                    tint_g: 1.0,
                    tint_b: 1.0,
                    tint_a: 1.0,
                    is_video: false,
                }),
                bga_stretch: 1,
                ..SkinDrawState::default()
            },
            SkinTextState::default(),
        );

        assert!(matches!(
            items.as_slice(),
            [SkinRenderItem::Image {
                texture: SkinTextureId(20000),
                rect: Rect { x, y, width, height },
                ..
            }] if approx_eq(*x, 0.1)
                && approx_eq(*y, 0.4)
                && approx_eq(*width, 0.3)
                && approx_eq(*height, 0.4)
        ));
    }

    #[test]
    fn song_bga_options_are_evaluated_from_draw_state() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": "src", "path": "dummy.png" }],
                "image": [
                    { "id": "no-bga", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "has-bga", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "no-bga", "op": [170], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "has-bga", "op": [171], "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "src".to_string(),
            SkinDocumentTexture {
                source_id: "src".to_string(),
                texture: SkinTextureId(1),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        let no_bga_items = document.static_image_render_items(
            &sources,
            SkinDrawState { has_bga: false, ..SkinDrawState::default() },
        );
        let bga_items = document.static_image_render_items(
            &sources,
            SkinDrawState { has_bga: true, ..SkinDrawState::default() },
        );

        assert!(matches!(
            no_bga_items.as_slice(),
            [SkinRenderItem::Image { rect: Rect { x, .. }, .. }] if approx_eq(*x, 0.0)
        ));
        assert!(matches!(
            bga_items.as_slice(),
            [SkinRenderItem::Image { rect: Rect { x, .. }, .. }] if approx_eq(*x, 0.2)
        ));
    }

    #[test]
    fn skin_document_expands_conditions_and_includes() {
        let root = unique_test_dir("bmz-skin-json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("included.json"),
            r#"
            [
                { "id": "included", "src": "1", "x": 0, "y": 0, "w": 8, "h": 8, },
                { "if": -901, "value": { "id": "disabled", "src": "1" } }
            ]
            "#,
        )
        .unwrap();
        std::fs::write(
            root.join("skin.json"),
            r#"
            {
                "type": 0,
                "property": [
                    { "name": "Graph", "def": "On", "item": [
                        { "name": "Off", "op": 900 },
                        { "name": "On", "op": 901 }
                    ]}
                ],
                "source": [{ "id": 1, "path": "system.png" }],
                "image": { "include": "included.json" },
                "destination": [
                    { "if": 901, "value": { "id": "included", "dst": [{ "x": 1, "y": 2, "w": 3, "h": 4 }] } },
                    { "if": -901, "value": { "id": "disabled", "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] } }
                ],
            }
            "#,
        )
        .unwrap();

        let document = SkinDocument::load_beatoraja_json(&root.join("skin.json")).unwrap();

        assert_eq!(document.source[0].id, "1");
        assert_eq!(document.image.len(), 1);
        assert_eq!(document.image[0].id, "included");
        assert_eq!(document.destination.len(), 1);
        let DestinationListEntry::Single(dst0) = &document.destination[0] else {
            panic!("expected Single destination");
        };
        assert_eq!(dst0.id, "included");
        let SkinDstEntry::Frame(frame) = &dst0.dst[0] else {
            panic!("expected Frame entry");
        };
        assert_eq!(frame.x, Some(1));
    }

    #[test]
    fn skin_document_resolves_static_image_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1280,
                "h": 720,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 16, "y": 32, "w": 64, "h": 128 }],
                "destination": [
                    { "id": "panel", "blend": 2, "dst": [
                        { "x": 128, "y": 72, "w": 256, "h": 144, "a": 128, "r": 64 }
                    ]},
                    { "id": "panel", "timer": 1, "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 256.0, height: 512.0 },
            },
        )]);

        let items = document.static_image_render_items(&sources, SkinDrawState::default());

        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0],
            SkinRenderItem::Image {
                texture: SkinTextureId(42),
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uv_width, height: uv_height },
                tint: Color { r, a, .. },
                blend: BlendMode::Add,
                ..
            } if approx_eq(x, 0.1)
                && approx_eq(y, 0.7)
                && approx_eq(width, 0.2)
                && approx_eq(height, 0.2)
                && approx_eq(u, 16.0 / 256.0)
                && approx_eq(v, 32.0 / 512.0)
                && approx_eq(uv_width, 64.0 / 256.0)
                && approx_eq(uv_height, 128.0 / 512.0)
                && approx_eq(r, 64.0 / 255.0)
                && approx_eq(a, 128.0 / 255.0)
        ));
    }

    #[test]
    fn static_render_items_split_at_notes_marker() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [
                    { "id": "behind", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "cover", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "frame", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": "behind", "dst": [{ "x": 0, "y": 0, "w": 100, "h": 100 }] },
                    { "id": "notes" },
                    { "id": "cover", "dst": [{ "x": 10, "y": 10, "w": 20, "h": 20 }] },
                    { "id": "frame", "dst": [{ "x": 5, "y": 5, "w": 90, "h": 90 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 8.0, 8.0);

        let (behind, front, failed_overlay) = document.static_render_items_split(
            &sources,
            SkinDrawState::default(),
            SkinTextState::default(),
        );

        // `{"id":"notes"}` マーカーより前の destination は背面、後ろは前面に入る。
        assert_eq!(behind.len(), 1, "behind = destinations before the notes marker");
        assert_eq!(front.len(), 2, "front = destinations after the notes marker");
        assert!(failed_overlay.is_empty());
        // 結合版 static_render_items は behind→front→failed の順で全アイテムを返す。
        let all = document.static_render_items(
            &sources,
            SkinDrawState::default(),
            SkinTextState::default(),
        );
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn pre_notes_lift_line_at_note_origin_renders_in_front() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [
                    { "id": "backdrop", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": 15, "src": 1, "x": 16, "y": 0, "w": 8, "h": 8 },
                    { "id": "note", "src": 1, "x": 0, "y": 0, "w": 51, "h": 36 }
                ],
                "destination": [
                    { "id": "backdrop", "dst": [{ "x": 0, "y": 0, "w": 720, "h": 720 }] },
                    { "id": 15, "offset": 3, "dst": [{ "x": 76, "y": 357, "w": 431, "h": 8 }] },
                    { "id": "notes" }
                ],
                "note": {
                    "id": "notes",
                    "note": ["note"],
                    "dst": [{ "x": 168, "y": 345, "w": 51, "h": 723 }]
                }
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 720.0, 720.0);

        let (behind, front, failed_overlay) = document.static_render_items_split(
            &sources,
            SkinDrawState::default(),
            SkinTextState::default(),
        );

        assert_eq!(behind.len(), 1, "ordinary pre-notes items stay behind notes");
        assert_eq!(front.len(), 1, "ECFN-style judge line is drawn in front of notes");
        assert!(failed_overlay.is_empty());
        assert!(matches!(
            front.first(),
            Some(SkinRenderItem::Image { rect, .. })
                if approx_eq(rect.y, 355.0 / 720.0)
                    && approx_eq(rect.height, 8.0 / 720.0)
        ));
    }

    #[test]
    fn skin_document_applies_destination_stretch_to_static_images() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "wide", "src": 1, "x": 0, "y": 0, "w": 200, "h": 100 }],
                "destination": [
                    { "id": "wide", "stretch": 1, "dst": [{ "x": 10, "y": 10, "w": 40, "h": 40 }] },
                    { "id": "wide", "stretch": 3, "dst": [{ "x": 10, "y": 60, "w": 40, "h": 40 }] },
                    { "id": "wide", "stretch": 9, "dst": [{ "x": 70, "y": 70, "w": 20, "h": 20 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 200.0, height: 100.0 },
            },
        )]);

        let items = document.static_image_render_items(&sources, SkinDrawState::default());

        assert_eq!(items.len(), 3);
        assert!(matches!(
            items[0],
            SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, width: uv_width, .. },
                ..
            } if approx_eq(x, 0.1)
                && approx_eq(y, 0.6)
                && approx_eq(width, 0.4)
                && approx_eq(height, 0.2)
                && approx_eq(u, 0.0)
                && approx_eq(uv_width, 1.0)
        ));
        assert!(matches!(
            items[1],
            SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, width: uv_width, .. },
                ..
            } if approx_eq(x, 0.1)
                && approx_eq(y, 0.0)
                && approx_eq(width, 0.4)
                && approx_eq(height, 0.4)
                && approx_eq(u, 0.25)
                && approx_eq(uv_width, 0.5)
        ));
        assert!(matches!(
            items[2],
            SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, -0.2)
                && approx_eq(y, -0.3)
                && approx_eq(width, 2.0)
                && approx_eq(height, 1.0)
        ));
    }

    #[test]
    fn skin_document_evaluates_safe_gauge_draw_conditions() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "draw": "gauge() >= 75", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "gauge() >= 50 and gauge() < 75", "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "gauge() < 25", "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "unknown() > 0", "dst": [{ "x": 30, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        let high = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 0, gauge: 80.0, ..SkinDrawState::default() },
        );
        let middle = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 0, gauge: 60.0, ..SkinDrawState::default() },
        );
        let low = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 0, gauge: 10.0, ..SkinDrawState::default() },
        );

        assert_eq!(high.len(), 1);
        assert_eq!(middle.len(), 1);
        assert_eq!(low.len(), 1);
        assert!(
            matches!(high[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.0))
        );
        assert!(
            matches!(middle[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.1))
        );
        assert!(
            matches!(low[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.2))
        );
    }

    #[test]
    fn skin_document_evaluates_number_draw_conditions() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "draw": "number(425) > 0", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "draw": "number(425) == 0", "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        let no_miss = document.static_image_render_items(&sources, SkinDrawState::default());
        let miss = document.static_image_render_items(
            &sources,
            SkinDrawState {
                judge_counts: DisplayJudgeCounts { bad: 1, poor: 2, ..Default::default() },
                ..SkinDrawState::default()
            },
        );

        assert!(
            matches!(no_miss[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.1))
        );
        assert!(
            matches!(miss[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.0))
        );
        assert!(eval_skin_draw_condition(
            "number(410) == number(411) or number(110) == number(410)",
            SkinDrawState {
                judge_counts: DisplayJudgeCounts { pgreat: 300, ..Default::default() },
                fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
                    fast_pgreat: 300,
                    slow_pgreat: 0,
                    ..Default::default()
                }),
                ..Default::default()
            }
        ));
        assert!(eval_skin_draw_condition(
            "number(410) > number(411) and number(411) >= 1",
            SkinDrawState {
                fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
                    fast_pgreat: 120,
                    slow_pgreat: 20,
                    ..Default::default()
                }),
                ..Default::default()
            }
        ));
    }

    #[test]
    fn skin_document_evaluates_option_draw_conditions() {
        assert!(eval_skin_draw_condition(
            "option(197)",
            SkinDrawState {
                select_replay_slots: [true, false, false, false],
                ..Default::default()
            }
        ));
        assert!(eval_skin_draw_condition("!option(197)", SkinDrawState::default()));
        assert!(!eval_skin_draw_condition(
            "!option(197)",
            SkinDrawState {
                select_replay_slots: [true, false, false, false],
                ..Default::default()
            }
        ));
    }

    #[test]
    fn skin_document_evaluates_timer_draw_conditions() {
        assert!(eval_skin_draw_condition("timer(46) == timer_off", SkinDrawState::default()));
        assert!(eval_skin_draw_condition(
            "timer(46) != timer_off",
            SkinDrawState {
                judge_ms: judge_region_state(0, 120, 0).judge_ms,
                ..Default::default()
            }
        ));
        assert!(eval_skin_draw_condition(
            "timer(46) > 0 and option(197)",
            SkinDrawState {
                judge_ms: judge_region_state(0, 120, 0).judge_ms,
                select_replay_slots: [true, false, false, false],
                ..Default::default()
            }
        ));
    }

    #[test]
    fn skin_document_evaluates_gauge_type_draw_conditions() {
        assert!(eval_skin_draw_condition(
            "gauge_type() == 4 or gauge_type() == 5",
            SkinDrawState { gauge_type: 4, ..Default::default() }
        ));
        assert!(eval_skin_draw_condition(
            "gauge_type() == 4 or gauge_type() == 5",
            SkinDrawState { gauge_type: 5, ..Default::default() }
        ));
        assert!(!eval_skin_draw_condition(
            "gauge_type() == 4 or gauge_type() == 5",
            SkinDrawState { gauge_type: 2, ..Default::default() }
        ));
    }

    #[test]
    fn skin_document_evaluates_gauge_auto_shift_draw_conditions() {
        assert!(eval_skin_draw_condition(
            "gauge_auto_shift() == 1",
            SkinDrawState { gauge_auto_shift: true, ..Default::default() }
        ));
        assert!(!eval_skin_draw_condition(
            "gauge_auto_shift() == 1",
            SkinDrawState { gauge_auto_shift: false, ..Default::default() }
        ));
        assert_eq!(select_gauge_auto_shift_index("BEST CLEAR"), 3);
        assert_eq!(
            skin_state_imageset_index(
                78,
                SkinDrawState { select_gauge_auto_shift_index: 3, ..Default::default() }
            ),
            Some(3)
        );
    }

    #[test]
    fn static_render_items_resolve_iidx_destination_with_base_image() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "frame.png" }],
                "image": [{ "id": "groove_frame", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "groove_frame_iidx", "timer": 9001, "dst": [{ "x": 1, "y": 2, "w": 10, "h": 10 }] }
                ],
                "dynamicTimer": [{ "id": 9001, "observe": "gauge_type() == 4 or gauge_type() == 5" }]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(7),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);
        let mut runtime = DynamicTimerRuntime::default();
        let state = runtime.advance(
            &document,
            SkinDrawState { gauge_type: 4, elapsed_ms: 100, ..Default::default() },
            100,
        );
        let (behind, front, _) =
            document.static_render_items_split(&sources, state, SkinTextState::default());
        let items = behind.into_iter().chain(front).collect::<Vec<_>>();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], SkinRenderItem::Image { .. }));
    }

    #[test]
    fn static_render_items_resolve_exhard_gauge_additive_overlay() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [{ "id": "gauge-node", "src": 1, "x": 0, "y": 0, "w": 5, "h": 10 }],
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 2 },
                "destination": [
                    {
                        "id": "gauge",
                        "loop": 1200,
                        "draw": "gauge_type() == 4 or gauge_type() == 5",
                        "blend": 2,
                        "dst": [
                            { "time": 1200, "x": 54, "y": 151, "w": 450, "h": 28, "a": 0 },
                            { "time": 1700, "a": 80 },
                            { "time": 2000, "a": 0 }
                        ]
                    }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);
        let (behind, front, _) = document.static_render_items_split(
            &sources,
            SkinDrawState { gauge_type: 4, elapsed_ms: 1700, ..Default::default() },
            SkinTextState::default(),
        );
        let items = behind.into_iter().chain(front).collect::<Vec<_>>();
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| matches!(
            item,
            SkinRenderItem::Image {
                tint: Color { a, .. },
                blend: BlendMode::Add,
                ..
            } if (*a - 80.0 / 255.0).abs() < 0.01
        )));
    }

    #[test]
    fn skin_document_evaluates_destination_option_conditions() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "property": [
                    { "name": "Play Side", "item": [
                        { "name": "1P", "op": 920 },
                        { "name": "2P", "op": 921 }
                    ]},
                    { "name": "Score Graph", "def": "On", "item": [
                        { "name": "Off", "op": 900 },
                        { "name": "On", "op": 901 }
                    ]}
                ],
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "op": [920, 901], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "op": [921], "dst": [{ "x": 10, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "panel", "op": [-901], "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        let items = document.static_image_render_items(&sources, SkinDrawState::default());

        assert_eq!(document.enabled_options(), [920, 901]);
        assert_eq!(items.len(), 1);
        assert!(
            matches!(items[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. } if approx_eq(x, 0.0))
        );
    }

    #[test]
    fn skin_document_samples_destination_keyframes_by_elapsed_time() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                        { "time": 100, "x": 30, "a": 128 },
                        { "time": 200, "x": 60, "w": 20 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        let early = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 50, ..SkinDrawState::default() },
        );
        let middle = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 150, ..SkinDrawState::default() },
        );
        let late = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 250, ..SkinDrawState::default() },
        );

        assert!(
            matches!(early[0], SkinRenderItem::Image { rect: Rect { x, width, .. }, tint: Color { a, .. }, .. }
                if approx_eq(x, 0.15) && approx_eq(width, 0.1) && approx_eq(a, 192.0 / 255.0))
        );
        assert!(
            matches!(middle[0], SkinRenderItem::Image { rect: Rect { x, width, .. }, tint: Color { a, .. }, .. }
                if approx_eq(x, 0.45) && approx_eq(width, 0.15) && approx_eq(a, 128.0 / 255.0))
        );
        assert!(
            matches!(late[0], SkinRenderItem::Image { rect: Rect { x, width, .. }, tint: Color { a, .. }, .. }
                if approx_eq(x, 0.6) && approx_eq(width, 0.2) && approx_eq(a, 128.0 / 255.0))
        );
    }

    #[test]
    fn skin_document_applies_destination_acc_easing() {
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        for (acc, expected_x) in [(1, 0.25), (2, 0.75), (3, 0.0)] {
            let document: SkinDocument = serde_json::from_str(&format!(
                r#"
                {{
                    "type": 0,
                    "w": 100,
                    "h": 100,
                    "source": [{{ "id": 1, "path": "system.png" }}],
                    "image": [{{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }}],
                    "destination": [
                        {{ "id": "panel", "dst": [
                            {{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }},
                            {{ "time": 100, "x": 100, "acc": {acc} }}
                        ]}}
                    ]
                }}
                "#
            ))
            .unwrap();

            let items = document.static_image_render_items(
                &sources,
                SkinDrawState { elapsed_ms: 50, ..SkinDrawState::default() },
            );

            assert!(matches!(items[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                    if approx_eq(x, expected_x)));
        }
    }

    #[test]
    fn skin_document_loops_destination_keyframes() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "loop": 100, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                        { "time": 100, "x": 30 },
                        { "time": 200, "x": 60 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        // loop=100, 終端=200。elapsed=350 は終端超過なので [100, 200) 区間へループバック:
        // (350 - 100) % (200 - 100) + 100 = 150 → time 150 は keyframe 100(x=30)/200(x=60) の中間
        // x = 45 → 正規化 0.45
        let wrapped = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 350, ..SkinDrawState::default() },
        );

        assert!(matches!(wrapped[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(x, 0.45)));
    }

    #[test]
    fn skin_document_resolves_lane_note_images() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "source": [{ "id": 1, "path": "notes.png" }],
                "image": [
                    { "id": "note-w", "src": 1, "x": 0, "y": 0, "w": 20, "h": 10 },
                    { "id": "note-b", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 },
                    { "id": "note-s", "src": 1, "x": 30, "y": 0, "w": 30, "h": 10 }
                ],
                "note": {
                    "id": "notes",
                    "note": ["note-w", "note-b", "note-w", "note-b", "note-w", "note-b", "note-w", "note-s"]
                }
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 50.0 },
            },
        )]);

        let key2 = document
            .note_image_render_item(
                Lane::Key2,
                KeyMode::K7,
                Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                &sources,
            )
            .unwrap();
        let scratch = document
            .note_image_render_item(
                Lane::Scratch,
                KeyMode::K7,
                Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                &sources,
            )
            .unwrap();

        assert!(matches!(
            key2,
            SkinRenderItem::Image {
                texture: SkinTextureId(42),
                uv: TextureRegion { x, width, .. },
                ..
            } if approx_eq(x, 0.2) && approx_eq(width, 0.1)
        ));
        assert!(matches!(
            scratch,
            SkinRenderItem::Image {
                texture: SkinTextureId(42),
                uv: TextureRegion { x, width, .. },
                ..
            } if approx_eq(x, 0.3) && approx_eq(width, 0.3)
        ));
    }

    #[test]
    fn skin_document_resolves_gauge_nodes_into_parts() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [{ "id": "gauge-node", "src": 1, "x": 10, "y": 0, "w": 5, "h": 10 }],
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 4, "type": 0 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 80, "y": 10, "w": -40, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.gauge_render_items(50.0, 0, &sources).unwrap();

        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|item| matches!(item, SkinRenderItem::Image { .. })));
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, 0.4)
                && approx_eq(y, 0.8)
                && approx_eq(width, 0.1)
                && approx_eq(height, 0.1)));
    }

    #[test]
    fn skin_gauge_sprite_selects_exhard_nodes_and_tip_frame() {
        let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [],
                "gauge": { "id": "gauge", "nodes": [], "parts": 4, "type": 3, "cycle": 33 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        document.gauge.as_mut().unwrap().nodes =
            (0..36).map(|index| format!("node-{index}")).collect();
        document.image = (0..36)
            .map(|index| SkinImageDef {
                id: format!("node-{index}"),
                src: "1".to_string(),
                x: index as i32,
                y: 0,
                w: 1,
                h: 1,
                divx: 1,
                divy: 1,
                timer: None,
                cycle: 0,
                len: 0,
                ref_id: 0,
                click: 0,
                act: None,
            })
            .collect();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 36.0, height: 1.0 },
            },
        )]);
        let items = document
            .static_image_render_items(
                &sources,
                SkinDrawState {
                    elapsed_ms: 1_000,
                    gauge: 75.0,
                    gauge_max: 100.0,
                    gauge_border: 1.0,
                    gauge_type: 4,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter_map(|item| match item {
                SkinRenderItem::Image { .. } => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(items.len(), 5, "4 parts + flickering tip overlay");
        let tip_flicker = items.iter().find_map(|item| match item {
            SkinRenderItem::Image { uv, blend: BlendMode::Normal, .. } if uv.x > 0.7 => Some(uv.x),
            _ => None,
        });
        assert!(
            tip_flicker.is_some(),
            "EX-HARD flickering tip should use node index 28+ (normal blend overlay)"
        );
    }

    #[test]
    fn skin_gauge_flickering_draws_normal_tip_overlay() {
        let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [],
                "gauge": { "id": "gauge", "nodes": [], "parts": 4, "type": 3, "cycle": 33 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        document.gauge.as_mut().unwrap().nodes =
            (0..36).map(|index| format!("node-{index}")).collect();
        document.image = (0..36)
            .map(|index| SkinImageDef {
                id: format!("node-{index}"),
                src: "1".to_string(),
                x: index as i32,
                y: 0,
                w: 1,
                h: 1,
                divx: 1,
                divy: 1,
                timer: None,
                cycle: 0,
                len: 0,
                ref_id: 0,
                click: 0,
                act: None,
            })
            .collect();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 36.0, height: 1.0 },
            },
        )]);
        let items = document
            .static_image_render_items(
                &sources,
                SkinDrawState {
                    elapsed_ms: 8,
                    gauge: 75.0,
                    gauge_max: 100.0,
                    gauge_border: 1.0,
                    gauge_type: 2,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter_map(|item| match item {
                SkinRenderItem::Image { .. } => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(items.len(), 5, "4 parts + flickering tip overlay");
        let flicker = items.iter().find(|item| {
            matches!(
                item,
                SkinRenderItem::Image {
                    blend: BlendMode::Normal,
                    tint: Color { a, .. },
                    ..
                } if *a > 0.2
            )
        });
        assert!(flicker.is_some(), "expected normal-blend tip overlay with alpha fade");
    }

    #[test]
    fn skin_gauge_defaults_to_random_when_type_omitted() {
        let document: SkinDocument =
            serde_json::from_str(r#"{"type":0,"w":100,"h":100,"gauge":{"id":"g","nodes":[]}}"#)
                .unwrap();
        assert_eq!(document.gauge.as_ref().unwrap().gauge_type, 0);
    }

    #[test]
    fn skin_gauge_random_animation_changes_by_cycle() {
        let gauge = SkinGaugeDef {
            id: "g".to_string(),
            nodes: Vec::new(),
            parts: 4,
            gauge_type: 0,
            range: 3,
            cycle: 33,
            starttime: 0,
            endtime: 500,
        };
        let first = skin_gauge_animation_index(
            &gauge,
            SkinDrawState { elapsed_ms: 33, ..Default::default() },
        );
        let second = skin_gauge_animation_index(
            &gauge,
            SkinDrawState { elapsed_ms: 66, ..Default::default() },
        );

        assert_ne!(first, second, "type=0 RANDOM should not stay fixed at frame 0");
        assert!((0..=3).contains(&first));
        assert!((0..=3).contains(&second));
    }

    #[test]
    fn skin_gauge_decrease_animation_advances_forward() {
        let gauge = SkinGaugeDef {
            id: "g".to_string(),
            nodes: Vec::new(),
            parts: 4,
            gauge_type: 2,
            range: 3,
            cycle: 33,
            starttime: 0,
            endtime: 500,
        };

        assert_eq!(
            skin_gauge_animation_index(
                &gauge,
                SkinDrawState { elapsed_ms: 33, ..Default::default() }
            ),
            1
        );
        assert_eq!(
            skin_gauge_animation_index(
                &gauge,
                SkinDrawState { elapsed_ms: 66, ..Default::default() }
            ),
            2
        );
    }

    #[test]
    fn skin_gauge_notes_count_truncates_toward_zero() {
        assert_eq!(skin_gauge_notes_count(74.9, 4, 100.0), 2);
        assert_eq!(skin_gauge_notes_count(75.0, 4, 100.0), 3);
        assert_eq!(skin_gauge_notes_count(0.0, 4, 100.0), 0);
    }

    #[test]
    fn skin_gauge_omitted_type_has_no_flickering_overlay() {
        let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [],
                "gauge": { "id": "gauge", "nodes": [], "parts": 4 },
                "destination": [
                    { "id": "gauge", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        document.gauge.as_mut().unwrap().nodes =
            (0..36).map(|index| format!("node-{index}")).collect();
        document.image = (0..36)
            .map(|index| SkinImageDef {
                id: format!("node-{index}"),
                src: "1".to_string(),
                x: index as i32,
                y: 0,
                w: 1,
                h: 1,
                divx: 1,
                divy: 1,
                timer: None,
                cycle: 0,
                len: 0,
                ref_id: 0,
                click: 0,
                act: None,
            })
            .collect();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 36.0, height: 1.0 },
            },
        )]);
        let items = document
            .static_image_render_items(
                &sources,
                SkinDrawState {
                    elapsed_ms: 8,
                    gauge: 75.0,
                    gauge_max: 100.0,
                    gauge_border: 1.0,
                    gauge_type: 2,
                    ..Default::default()
                },
            )
            .into_iter()
            .filter_map(|item| match item {
                SkinRenderItem::Image { .. } => Some(item),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(items.len(), 4, "type=0 should not add flickering tip overlay");
    }

    #[test]
    fn static_render_items_resolve_gauge_in_destination_order() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "gauge.png" }],
                "image": [
                    { "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "gauge-node", "src": 1, "x": 10, "y": 0, "w": 5, "h": 10 }
                ],
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 4, "type": 0 },
                "destination": [
                    { "id": "panel", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "gauge", "timer": 2, "dst": [{ "x": 80, "y": 10, "w": -40, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let inactive = document.static_image_render_items(
            &sources,
            SkinDrawState {
                elapsed_ms: 500,
                gauge: 50.0,
                gauge_max: 100.0,
                fadeout_ms: None,
                ..Default::default()
            },
        );
        let active = document.static_image_render_items(
            &sources,
            SkinDrawState {
                elapsed_ms: 500,
                gauge: 50.0,
                gauge_max: 100.0,
                fadeout_ms: Some(250),
                ..Default::default()
            },
        );

        assert_eq!(inactive.len(), 1);
        // beatoraja は全 `parts` 分のセルを描画する (埋まり具合でスプライトだけ変える)。
        assert_eq!(active.len(), 5);
        assert!(active[1..].iter().all(|item| matches!(item, SkinRenderItem::Image { .. })));
    }

    #[test]
    fn skin_document_resolves_judge_images_by_label() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 20, "divy": 2, "cycle": 100 },
                    { "id": "judgef-gr", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 },
                    { "id": "judgef-gd", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 },
                    { "id": "judgef-bd", "src": 1, "x": 30, "y": 0, "w": 10, "h": 10 },
                    { "id": "judgef-pr", "src": 1, "x": 40, "y": 0, "w": 10, "h": 10 },
                    { "id": "judgef-ms", "src": 1, "x": 50, "y": 0, "w": 10, "h": 10 }
                ],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judgef-pg", "dst": [{ "time": 0, "x": 0, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] },
                        { "id": "judgef-gr", "dst": [{ "time": 0, "x": 0, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] },
                        { "id": "judgef-gd", "dst": [{ "time": 0, "x": 0, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] },
                        { "id": "judgef-bd", "dst": [{ "time": 0, "x": 0, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] },
                        { "id": "judgef-pr", "dst": [{ "time": 0, "x": 0, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] },
                        { "id": "judgef-ms", "dst": [{ "time": 0, "x": 0, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] }
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let pgreat = document.judge_image_render_item("PGREAT FAST", 175, &sources).unwrap();
        let poor = document.judge_image_render_item("POOR SLOW", 120, &sources).unwrap();
        let empty_poor =
            document.judge_image_render_item("EMPTY POOR SLOW", 120, &sources).unwrap();
        let expired = document.judge_image_render_item("PGREAT", 600, &sources);

        assert!(matches!(pgreat, SkinRenderItem::Image {
                uv: TextureRegion { x, y: u_y, height: u_height, .. },
                rect: Rect { y, width, .. },
                ..
            } if approx_eq(x, 0.0)
                && approx_eq(u_y, 0.1)
                && approx_eq(u_height, 0.1)
                && approx_eq(y, 0.8)
                && approx_eq(width, 0.2)));
        assert!(matches!(poor, SkinRenderItem::Image {
                uv: TextureRegion { x, .. },
                ..
            } if approx_eq(x, 0.4)));
        assert!(matches!(empty_poor, SkinRenderItem::Image {
                uv: TextureRegion { x, .. },
                ..
            } if approx_eq(x, 0.5)));
        assert!(expired.is_none());
    }

    #[test]
    fn skin_document_resolves_judge_number_images() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 3 }
                ],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judgef-pg", "dst": [{ "time": 0, "x": 10, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] }
                    ],
                    "numbers": [
                        { "id": "judgen-pg", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.judge_render_items("PGREAT", 123, 100, &sources).unwrap();

        assert_eq!(items.len(), 4);
        // judge number: dst x 20 - w*digit/2 = 13, align=2, base judge x=10 → digits at 0.23/0.28/0.33
        assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uv_width, height: uv_height },
                ..
            } if approx_eq(x, 0.23)
                && approx_eq(y, 0.75)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.1)
                && approx_eq(u, 0.1)
                && approx_eq(v, 0.2)
                && approx_eq(uv_width, 0.1)
                && approx_eq(uv_height, 0.1)));
        assert!(matches!(items[2], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.28) && approx_eq(u, 0.2)));
        assert!(matches!(items[3], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.33) && approx_eq(u, 0.3)));
    }

    #[test]
    fn skin_document_animates_judge_number_value_rows() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 20, "divx": 10, "divy": 2, "digit": 1, "cycle": 100 }
                ],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judgef-pg", "dst": [{ "time": 0, "x": 10, "y": 10, "w": 20, "h": 10 }, { "time": 500 }] }
                    ],
                    "numbers": [
                        { "id": "judgen-pg", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let early = document.judge_render_items("PGREAT", 7, 25, &sources).unwrap();
        let late = document.judge_render_items("PGREAT", 7, 75, &sources).unwrap();

        assert!(matches!(early[1], SkinRenderItem::Image {
                uv: TextureRegion { y, .. },
                ..
            } if approx_eq(y, 0.2)));
        assert!(matches!(late[1], SkinRenderItem::Image {
                uv: TextureRegion { y, .. },
                ..
            } if approx_eq(y, 0.3)));
    }

    #[test]
    fn skin_document_renders_judge_destination_insert() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "property": [
                    { "name": "Play Side", "item": [
                        { "name": "1P", "op": 920 },
                        { "name": "2P", "op": 921 }
                    ]}
                ],
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 3 }
                ],
                "judge": [{
                    "id": 2010,
                    "images": [
                        { "id": "judgef-pg", "loop": -1, "offset": 3, "dst": [
                            { "if": [920], "value": { "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 } },
                            { "if": [921], "value": { "time": 0, "x": 70, "y": 20, "w": 20, "h": 10 } },
                            { "time": 500 }
                        ]}
                    ],
                    "numbers": [
                        { "id": "judgen-pg", "loop": -1, "dst": [
                            { "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 },
                            { "time": 500 }
                        ]}
                    ]
                }],
                "destination": [
                    { "id": 2010 }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.static_render_items(
            &sources,
            SkinDrawState {
                judge_ms: judge_region_state(0, 100, 0).judge_ms,
                judge_index: judge_region_state(0, 100, 0).judge_index,
                judge_combo: {
                    let mut combo = [0; MAX_JUDGE_REGIONS];
                    combo[0] = 123;
                    combo
                },
                offset_lift_px: 10,
                ..SkinDrawState::default()
            },
            SkinTextState::default(),
        );

        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, 0.1)
                && approx_eq(y, 0.6)
                && approx_eq(width, 0.2)
                && approx_eq(height, 0.1)));
        assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, y, .. },
                ..
            } if approx_eq(x, 0.23) && approx_eq(y, 0.55)));
    }

    #[test]
    fn lane_judge_region_maps_14k_sides() {
        assert_eq!(lane_judge_region(0, 16, 2), 0);
        assert_eq!(lane_judge_region(7, 16, 2), 0);
        assert_eq!(lane_judge_region(8, 16, 2), 1);
        assert_eq!(lane_judge_region(15, 16, 2), 1);
    }

    #[test]
    fn dual_judge_regions_render_combo_at_separate_positions() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 3 }
                ],
                "judge": [
                    {
                        "id": "judge",
                        "index": 0,
                        "images": [
                            { "id": "judgef-pg", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                        ],
                        "numbers": [
                            { "id": "judgen-pg", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                        ]
                    },
                    {
                        "id": "judge1",
                        "index": 1,
                        "images": [
                            { "id": "judgef-pg", "dst": [{ "time": 0, "x": 60, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                        ],
                        "numbers": [
                            { "id": "judgen-pg", "dst": [{ "time": 0, "x": 70, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                        ]
                    }
                ],
                "destination": [
                    { "id": "judge" },
                    { "id": "judge1" }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        assert_eq!(document.judge_region_count(), 2);
        let state = SkinDrawState {
            judge_ms: {
                let mut ms = [None; MAX_JUDGE_REGIONS];
                ms[0] = Some(100);
                ms[1] = Some(100);
                ms
            },
            judge_index: {
                let mut idx = [None; MAX_JUDGE_REGIONS];
                idx[0] = Some(0);
                idx[1] = Some(0);
                idx
            },
            judge_combo: {
                let mut combo = [0; MAX_JUDGE_REGIONS];
                combo[0] = 42;
                combo[1] = 42;
                combo
            },
            combo: 42,
            ..SkinDrawState::default()
        };
        let left = document
            .judge_render_items_for_def(&document.judge[0], 0, 42, 100, &sources, state)
            .unwrap();
        let right = document
            .judge_render_items_for_def(&document.judge[1], 0, 42, 100, &sources, state)
            .unwrap();
        let left_digit = match &left[1] {
            SkinRenderItem::Image { rect, .. } => rect.x,
            _ => panic!("expected digit image"),
        };
        let right_digit = match &right[1] {
            SkinRenderItem::Image { rect, .. } => rect.x,
            _ => panic!("expected digit image"),
        };
        assert!(
            right_digit > left_digit + 0.2,
            "right region digit x={right_digit} should be right of left x={left_digit}"
        );

        let static_items = document.static_render_items(&sources, state, SkinTextState::default());
        assert_eq!(static_items.len(), 6);
        let static_left = match &static_items[1] {
            SkinRenderItem::Image { rect, .. } => rect.x,
            _ => panic!(),
        };
        let static_right = match &static_items[4] {
            SkinRenderItem::Image { rect, .. } => rect.x,
            _ => panic!(),
        };
        assert!(static_right > static_left + 0.2);
    }

    #[test]
    fn skin_document_hides_judge_combo_when_region_combo_is_zero() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judge-poor", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "combo", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 3 }
                ],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judge-poor", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                    ],
                    "numbers": [
                        { "id": "combo", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                    ]
                }],
                "destination": [{ "id": "judge" }]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let state = SkinDrawState {
            combo: 123,
            judge_ms: judge_region_state(0, 100, 0).judge_ms,
            judge_index: judge_region_state(0, 100, 0).judge_index,
            judge_combo: [0; MAX_JUDGE_REGIONS],
            ..SkinDrawState::default()
        };

        let items = document.static_render_items(&sources, state, SkinTextState::default());

        assert_eq!(items.len(), 1);
    }

    #[test]
    fn skin_draw_options_match_judge_fast_slow_regions() {
        let fast = SkinDrawState {
            judge_index: [Some(1), None, None],
            judge_timing_sign: [Some(1), None, None],
            ..SkinDrawState::default()
        };
        let slow = SkinDrawState {
            judge_index: [Some(1), None, None],
            judge_timing_sign: [Some(-1), None, None],
            ..SkinDrawState::default()
        };
        let perfect = SkinDrawState {
            judge_index: [Some(0), None, None],
            judge_timing_sign: [Some(1), None, None],
            ..SkinDrawState::default()
        };

        assert!(test_skin_op(1242, &[], fast));
        assert!(!test_skin_op(1243, &[], fast));
        assert!(test_skin_op(1243, &[], slow));
        assert!(!test_skin_op(1242, &[], slow));
        assert!(test_skin_op(241, &[], perfect));
        assert!(!test_skin_op(1242, &[], perfect));
    }

    #[test]
    fn skin_document_shifts_judge_combo_numbers_beatoraja_style() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "judge.png" }],
                "image": [
                    { "id": "judgef-pg", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "value": [
                    { "id": "judgen-pg", "src": 1, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 6 }
                ],
                "judge": [{
                    "id": 2010,
                    "shift": true,
                    "images": [
                        { "id": "judgef-pg", "dst": [{ "time": 0, "x": 30, "y": 20, "w": 20, "h": 10 }, { "time": 500 }] }
                    ],
                    "numbers": [
                        { "id": "judgen-pg", "dst": [{ "time": 0, "x": 20, "y": 5, "w": 5, "h": 10 }, { "time": 500 }] }
                    ]
                }],
                "destination": [
                    { "id": 2010 }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let items = document.static_render_items(
            &sources,
            SkinDrawState {
                judge_ms: judge_region_state(0, 100, 0).judge_ms,
                judge_index: judge_region_state(0, 100, 0).judge_index,
                judge_combo: {
                    let mut combo = [0; MAX_JUDGE_REGIONS];
                    combo[0] = 123;
                    combo
                },
                ..Default::default()
            },
            SkinTextState::default(),
        );

        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.23)));
        // dst x 20 - w*6/2 = 5, align=2, shiftbase=3, judge x 30 - length/2 = 23
        assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.43)));
        assert!(matches!(items[2], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.48)));
        assert!(matches!(items[3], SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(x, 0.53)));
    }

    #[test]
    fn skin_document_resolves_lane_imageset_effects() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "effect.png" }],
                "image": [
                    { "id": "normal", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "pgreat", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 },
                    { "id": "good", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 }
                ],
                "imageset": [
                    { "id": "beam1", "ref": 501, "images": ["normal", "pgreat"] },
                    { "id": "bomb1", "ref": 501, "images": ["normal", "pgreat", "good"] },
                    { "id": "beam2", "ref": 502, "images": ["normal", "pgreat"] }
                ],
                "destination": [
                    { "id": "beam1", "timer": 51, "loop": -1, "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] },
                    { "id": "bomb1", "timer": 51, "loop": -1, "dst": [{ "time": 0, "x": 30, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] },
                    { "id": "beam2", "timer": 52, "loop": -1, "dst": [{ "time": 0, "x": 50, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        // Key1 (timer 51 = bomb_ms[1]) でボムタイマー進行中、直近判定 PGREAT
        let pgreat_state = SkinDrawState {
            bomb_ms: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(50);
                a
            },
            lane_judge: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(0);
                a
            },
            ..SkinDrawState::default()
        };
        let pgreat = document.static_render_items(&sources, pgreat_state, SkinTextState::default());
        // GOOD 判定
        let good_state = SkinDrawState {
            bomb_ms: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(50);
                a
            },
            lane_judge: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(2);
                a
            },
            ..SkinDrawState::default()
        };
        let good = document.static_render_items(&sources, good_state, SkinTextState::default());
        // タイマーがアニメーション終端を超過 → loop:-1 で非表示
        let expired_state = SkinDrawState {
            bomb_ms: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(150);
                a
            },
            lane_judge: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(0);
                a
            },
            ..SkinDrawState::default()
        };
        let expired =
            document.static_render_items(&sources, expired_state, SkinTextState::default());

        // beam1 と bomb1 のみ描画される (beam2 は timer 52 非アクティブ)
        assert_eq!(pgreat.len(), 2);
        // beam1: 2枚構成 + PGREAT → "pgreat" 画像 (u=0.1), rect x=0.1
        assert!(matches!(pgreat[0], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.1) && approx_eq(u, 0.1)));
        // beam1: 2枚構成 + GOOD → "normal" 画像 (u=0.0)
        assert!(matches!(good[0], SkinRenderItem::Image {
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(u, 0.0)));
        // bomb1: 3枚構成 + GOOD(index2) → "good" 画像 (u=0.2), rect x=0.3
        assert!(matches!(good[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.3) && approx_eq(u, 0.2)));
        assert!(expired.is_empty());
    }

    #[test]
    fn select_skin_document_renders_songlist_rows() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [
                    { "id": 1, "path": "bar.png" },
                    { "id": 2, "path": "num.png" },
                    { "id": 3, "path": "lamp.png" },
                    { "id": 4, "path": "graph.png" }
                ],
                "image": [
                    { "id": "bar-song", "src": 1, "x": 0, "y": 0, "w": 40, "h": 10 },
                    { "id": "bar-folder", "src": 1, "x": 0, "y": 10, "w": 40, "h": 10 },
                    { "id": "bar-table", "src": 1, "x": 0, "y": 30, "w": 40, "h": 10 },
                    { "id": "song-op-marker", "src": 1, "x": 0, "y": 20, "w": 4, "h": 4 },
                    { "id": "folder-op-marker", "src": 1, "x": 4, "y": 20, "w": 4, "h": 4 },
                    { "id": "trophy-bronze", "src": 3, "x": 0, "y": 0, "w": 4, "h": 4 },
                    { "id": "trophy-silver", "src": 3, "x": 4, "y": 0, "w": 4, "h": 4 },
                    { "id": "trophy-gold", "src": 3, "x": 8, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-none", "src": 3, "x": 0, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-failed", "src": 3, "x": 4, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-assist", "src": 3, "x": 8, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-light-assist", "src": 3, "x": 12, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-easy", "src": 3, "x": 16, "y": 0, "w": 4, "h": 4 },
                    { "id": "lamp-normal", "src": 3, "x": 20, "y": 0, "w": 4, "h": 4 },
                    { "id": "label-ln", "src": 1, "x": 0, "y": 40, "w": 4, "h": 4 },
                    { "id": "label-random", "src": 1, "x": 4, "y": 40, "w": 4, "h": 4 },
                    { "id": "label-mine", "src": 1, "x": 8, "y": 40, "w": 4, "h": 4 }
                ],
                "imageset": [{ "id": "bar", "images": ["bar-song", "bar-folder", "bar-table"] }],
                "text": [
                    { "id": "bartext", "font": "main", "size": 10 },
                    { "id": "bartext1", "font": "folder", "size": 10 },
                    { "id": "bartext2", "font": "table", "size": 10 },
                    { "id": "bartext3", "font": "main", "size": 10 },
                    { "id": "bartext4", "font": "folder", "size": 10 }
                ],
                "value": [
                    { "id": "level-other", "src": 2, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 2 },
                    { "id": "level-beginner", "src": 2, "x": 0, "y": 10, "w": 100, "h": 10, "divx": 10, "digit": 2 },
                    { "id": "level-normal", "src": 2, "x": 0, "y": 20, "w": 100, "h": 10, "divx": 10, "digit": 2 }
                ],
                "graph": [{ "id": "graph-lamp", "src": 4, "x": 0, "y": 0, "w": 44, "h": 4, "divx": 11, "angle": 0, "type": -1 }],
                "songlist": {
                    "id": "songlist",
                    "center": 1,
                    "listoff": [
                        { "id": "bar", "dst": [{ "x": 10, "y": 70, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 10, "y": 30, "w": 40, "h": 10 }] }
                    ],
                    "liston": [
                        { "id": "bar", "dst": [{ "x": 12, "y": 70, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 12, "y": 50, "w": 40, "h": 10 }] },
                        { "id": "bar", "dst": [{ "x": 12, "y": 30, "w": 40, "h": 10 }] }
                    ],
                    "text": [
                        { "id": "bartext", "dst": [{ "x": 1, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext", "dst": [{ "x": 2, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext", "dst": [{ "x": 5, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext", "dst": [{ "x": 6, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext4", "dst": [{ "x": 7, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext4", "dst": [{ "x": 8, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext2", "dst": [{ "x": 9, "y": 2, "w": 20, "h": 8 }] }
                    ],
                    "judgegraph": [
                        { "id": "song-op-marker", "op": [2], "dst": [{ "x": 8, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "folder-op-marker", "op": [1], "dst": [{ "x": 12, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "level": [
                        { "id": "level-other", "dst": [{ "x": 30, "y": 2, "w": 5, "h": 8 }] },
                        { "id": "level-beginner", "dst": [{ "x": 30, "y": 2, "w": 5, "h": 8 }] },
                        { "id": "level-normal", "dst": [{ "x": 30, "y": 2, "w": 5, "h": 8 }] }
                    ],
                    "trophy": [
                        { "id": "trophy-bronze", "dst": [{ "x": 35, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "trophy-silver", "dst": [{ "x": 35, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "trophy-gold", "dst": [{ "x": 35, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "label": [
                        { "id": "label-ln", "dst": [{ "x": 40, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "label-random", "dst": [{ "x": 44, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "label-mine", "dst": [{ "x": 48, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "graph": { "id": "graph-lamp", "dst": [{ "x": 5, "y": 1, "w": 20, "h": 2 }] },
                    "lamp": [
                        { "id": "lamp-none", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-failed", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-light-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-easy", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-normal", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] }
                    ],
                    "playerlamp": [
                        { "id": "lamp-none", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-failed", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-assist", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-light-assist", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-easy", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-normal", "dst": [{ "x": 60, "y": 1, "w": 4, "h": 4 }] }
                    ]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
        let mut sources = mock_source("1", 100.0, 100.0);
        sources.extend(mock_source("2", 100.0, 100.0));
        sources.extend(mock_source("3", 24.0, 4.0));
        sources.extend(mock_source("4", 44.0, 4.0));
        let snapshot = SelectSnapshot {
            selected_index: 2,
            rows: vec![
                SelectRowSnapshot {
                    index: 1,
                    title: "Folder".to_string(),
                    play_level: "0".to_string(),
                    clear_type: "Normal".to_string(),
                    folder_lamp_counts: {
                        let mut counts = [0; 11];
                        counts[5] = 1;
                        counts[6] = 1;
                        counts
                    },
                    is_folder: true,
                    kind: SelectRowKind::Folder,
                    ..SelectRowSnapshot::default()
                },
                SelectRowSnapshot {
                    index: 2,
                    title: "Song".to_string(),
                    difficulty_name: "2".to_string(),
                    play_level: "12".to_string(),
                    clear_type: "Normal".to_string(),
                    total_notes: 100,
                    ex_score: Some(180),
                    has_long_notes: true,
                    has_mines: true,
                    ..SelectRowSnapshot::default()
                },
                SelectRowSnapshot {
                    index: 3,
                    title: "Table".to_string(),
                    play_level: "0".to_string(),
                    is_folder: true,
                    kind: SelectRowKind::TableFolder,
                    ..SelectRowSnapshot::default()
                },
            ],
            ..SelectSnapshot::default()
        };

        let items = document.select_render_items(&sources, &snapshot);

        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image { .. })));
        assert!(
            items
                .iter()
                .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Song"))
        );
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Text {
                origin: Point { x, y },
                text,
                style,
                ..
            } if text == "Folder"
                && style.font_id.as_deref() == Some("folder")
                && approx_eq(*x, 0.17)
                && approx_eq(*y, 0.2))));
        assert_eq!(
            items
                .iter()
                .filter(
                    |item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Folder")
                )
                .count(),
            1
        );
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Text {
                text,
                style,
                ..
            } if text == "Table"
                && style.font_id.as_deref() == Some("table"))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                uv: TextureRegion { y: v, .. },
                ..
            } if approx_eq(*v, 30.0 / 100.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.13)
                && approx_eq(*y, 0.45)
                && approx_eq(*width, 0.04)
                && approx_eq(*height, 0.04)
                && approx_eq(*u, 20.0 / 24.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.11)
                && approx_eq(*y, 0.25)
                && approx_eq(*width, 0.04)
                && approx_eq(*height, 0.04)
                && approx_eq(*u, 20.0 / 24.0))));
        assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(*x, 0.72)
                && approx_eq(*y, 0.45)
                && approx_eq(*width, 0.04)
                && approx_eq(*height, 0.04))));
        assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 24.0))));
        let course_snapshot = SelectSnapshot {
            selected_index: 4,
            rows: vec![SelectRowSnapshot {
                index: 4,
                title: "Course".to_string(),
                kind: SelectRowKind::Course,
                achieved_trophy_names: vec!["goldmedal".to_string()],
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };
        let course_items = document.select_render_items(&sources, &course_snapshot);
        assert!(course_items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 24.0))));
        assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, .. },
                uv: TextureRegion { width: u_width, .. },
                ..
            } if approx_eq(*x, 0.17)
                && approx_eq(*y, 0.47)
                && approx_eq(*width, 0.1)
                && approx_eq(*u_width, 0.5))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, .. },
                uv: TextureRegion { x: u, width: u_width, .. },
                ..
            } if approx_eq(*x, 0.15)
                && approx_eq(*y, 0.27)
                && approx_eq(*width, 0.1)
                && approx_eq(*u, 24.0 / 44.0)
                && approx_eq(*u_width, 4.0 / 44.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, width, .. },
                uv: TextureRegion { x: u, width: u_width, .. },
                ..
            } if approx_eq(*x, 0.25)
                && approx_eq(*y, 0.27)
                && approx_eq(*width, 0.1)
                && approx_eq(*u, 20.0 / 44.0)
                && approx_eq(*u_width, 4.0 / 44.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { y: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.4)
                && approx_eq(*u, 0.2))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.2)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 20.0 / 100.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.52)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 40.0 / 100.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.60)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 100.0)
                && approx_eq(*v, 40.0 / 100.0))));
        assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.22)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 4.0 / 100.0)
                && approx_eq(*v, 20.0 / 100.0))));

        let folder_selected = SelectSnapshot { selected_index: 1, ..snapshot };
        let items = document.select_render_items(&sources, &folder_selected);
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.18)
                && approx_eq(*y, 0.65)
                && approx_eq(*u, 0.0)
                && approx_eq(*v, 20.0 / 100.0))));
        assert!(!items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, y: v, .. },
                ..
            } if approx_eq(*x, 0.22)
                && approx_eq(*y, 0.65)
                && approx_eq(*u, 4.0 / 100.0)
                && approx_eq(*v, 20.0 / 100.0))));

        let wrapped_snapshot = SelectSnapshot {
            selected_index: 0,
            rows: vec![
                SelectRowSnapshot {
                    index: 2,
                    title: "Last".to_string(),
                    play_level: "2".to_string(),
                    ..SelectRowSnapshot::default()
                },
                SelectRowSnapshot {
                    index: 0,
                    title: "First".to_string(),
                    play_level: "1".to_string(),
                    ..SelectRowSnapshot::default()
                },
                SelectRowSnapshot {
                    index: 1,
                    title: "Second".to_string(),
                    play_level: "2".to_string(),
                    ..SelectRowSnapshot::default()
                },
            ],
            ..SelectSnapshot::default()
        };
        let items = document.select_render_items(&sources, &wrapped_snapshot);
        assert!(
            items
                .iter()
                .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Last"))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "First"))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item, SkinRenderItem::Text { text, .. } if text == "Second"))
        );
    }

    #[test]
    fn select_songlist_judgegraph_renders_chart_distribution() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "judgegraph": [{ "id": "density", "noGap": 1, "noGapX": 1 }],
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "liston": [{ "id": "row", "dst": [{ "x": 10, "y": 40, "w": 80, "h": 20 }] }],
                    "listoff": [{ "id": "row", "dst": [{ "x": 10, "y": 40, "w": 80, "h": 20 }] }],
                    "judgegraph": [{ "id": "density", "dst": [{ "x": 0, "y": 0, "w": 40, "h": 10 }] }]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
        let snapshot = SelectSnapshot {
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                kind: SelectRowKind::Song,
                in_library: true,
                chart_distribution: vec![
                    crate::scene::SelectChartDistributionSecond {
                        key_taps: 4,
                        mines: 1,
                        ..Default::default()
                    },
                    crate::scene::SelectChartDistributionSecond {
                        scratch_taps: 2,
                        key_long_bodies: 3,
                        ..Default::default()
                    },
                ],
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };

        let sources = HashMap::new();
        let items = document.select_render_items(&sources, &snapshot);
        let rect_count =
            items.iter().filter(|item| matches!(item, SkinRenderItem::Rect { .. })).count();

        assert_eq!(rect_count, 4);
    }

    #[test]
    fn select_click_hit_resolves_image_act_event() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "button.png" }],
                "image": [
                    { "id": "button_play", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "act": 15, "click": 2 }
                ],
                "destination": [
                    { "id": "button_play", "dst": [{ "x": 10, "y": 20, "w": 30, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let snapshot = match crate::sample::sample_select_scene() {
            crate::scene::AppSceneSnapshot::Select(snapshot) => snapshot,
            _ => unreachable!(),
        };

        let hit = document
            .select_click_hit(
                &sources,
                &snapshot,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.2,
                0.75,
            )
            .unwrap();

        assert_eq!(hit.target, SkinClickTarget::Event { event_id: 15, click: 2 });
        assert_eq!(hit.rect, Rect { x: 0.1, y: 0.7, width: 0.3, height: 0.1 });
    }

    #[test]
    fn select_mouse_rect_gates_render_and_click_hits() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "button.png" }],
                "image": [
                    { "id": "button", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "act": 15 }
                ],
                "destination": [
                    {
                        "id": "button",
                        "dst": [{ "x": 10, "y": 20, "w": 30, "h": 10 }],
                        "mouseRect": { "x": 5, "y": 2, "w": 10, "h": 4 }
                    }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let inside =
            SelectSnapshot { mouse_position: Some((0.16, 0.75)), ..SelectSnapshot::default() };
        let outside =
            SelectSnapshot { mouse_position: Some((0.01, 0.01)), ..SelectSnapshot::default() };

        assert!(document.select_render_items(&sources, &inside).iter().any(|item| {
            matches!(item, SkinRenderItem::Image { texture: SkinTextureId(9999), .. })
        }));
        assert!(!document.select_render_items(&sources, &outside).iter().any(|item| {
            matches!(item, SkinRenderItem::Image { texture: SkinTextureId(9999), .. })
        }));

        assert!(
            document
                .select_click_hit(
                    &sources,
                    &inside,
                    &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                    0.2,
                    0.75,
                )
                .is_some()
        );
        assert!(
            document
                .select_click_hit(
                    &sources,
                    &outside,
                    &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                    0.2,
                    0.75,
                )
                .is_none()
        );
    }

    #[test]
    fn select_slider_hit_resolves_changeable_volume_slider() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "slider": [
                    { "id": "master", "src": 1, "x": 0, "y": 0, "w": 10, "h": 5, "angle": 1, "range": 50, "type": 17 }
                ],
                "destination": [
                    { "id": "master", "dst": [{ "x": 10, "y": 20, "w": 10, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let snapshot = SelectSnapshot::default();

        let hit = document
            .select_slider_hit(
                &snapshot,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.35,
                0.775,
            )
            .unwrap();

        assert_eq!(hit.slider_type, 17);
        assert!(approx_eq(hit.value, 0.5));
        assert!(
            document
                .select_slider_hit(
                    &snapshot,
                    &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                    0.70,
                    0.775,
                )
                .is_none()
        );
    }

    #[test]
    fn select_click_hit_resolves_clickable_songlist_row() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "clickable": [0],
                    "liston": [
                        { "id": "bar", "dst": [{ "x": 0, "y": 0, "w": 50, "h": 10 }] }
                    ],
                    "listoff": [
                        { "id": "bar", "dst": [{ "x": 50, "y": 0, "w": 50, "h": 10 }] }
                    ]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
        let snapshot = match crate::sample::sample_select_scene() {
            crate::scene::AppSceneSnapshot::Select(snapshot) => snapshot,
            _ => unreachable!(),
        };

        let hit = document
            .select_click_hit(
                &HashMap::new(),
                &snapshot,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                0.25,
                0.95,
            )
            .unwrap();

        assert_eq!(hit.target, SkinClickTarget::SelectRow { row_index: 0 });
        assert_eq!(hit.rect, Rect { x: 0.0, y: 0.9, width: 0.5, height: 0.1 });
        assert!(
            document
                .select_click_hit(
                    &HashMap::new(),
                    &snapshot,
                    &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                    0.75,
                    0.95,
                )
                .is_none()
        );
    }

    #[test]
    fn select_skin_document_advances_dynamic_timers() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "marker.png" }],
                "image": [{ "id": "marker", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "marker", "timer": 9001, "dst": [{ "x": 10, "y": 10, "w": 10, "h": 10 }] }
                ],
                "dynamicTimer": [{ "id": 9001, "observe": "number(300) > 0" }]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let snapshot =
            SelectSnapshot { time: TimeUs(100_000), chart_count: 1, ..SelectSnapshot::default() };

        assert!(document.select_render_items(&sources, &snapshot).is_empty());

        let mut runtime = DynamicTimerRuntime::default();
        let items = document.select_render_items_with_dynamic_timers(
            &sources,
            &snapshot,
            Some(&mut runtime),
            &crate::select_settings_dest::SelectSettingsDestIndex::default(),
        );

        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], SkinRenderItem::Image { .. }));
    }

    #[test]
    fn select_skin_document_renders_unowned_song_with_nograde_bar() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "bar.png" }],
                "image": [
                    { "id": "bar-song", "src": 1, "x": 0, "y": 0, "w": 40, "h": 10 },
                    { "id": "bar-nograde", "src": 1, "x": 0, "y": 40, "w": 40, "h": 10 }
                ],
                "imageset": [{
                    "id": "bar",
                    "images": ["bar-song", "bar-song", "bar-song", "bar-song", "bar-nograde"]
                }],
                "text": [
                    { "id": "bartext-owned", "font": "main", "size": 10 },
                    { "id": "bartext-owned2", "font": "main", "size": 10 },
                    { "id": "bartext-owned3", "font": "main", "size": 10 },
                    { "id": "bartext-owned4", "font": "main", "size": 10 },
                    { "id": "bartext-owned5", "font": "main", "size": 10 },
                    { "id": "bartext-owned6", "font": "main", "size": 10 },
                    { "id": "bartext-owned7", "font": "main", "size": 10 },
                    { "id": "bartext-owned8", "font": "main", "size": 10 },
                    { "id": "bartext-unowned", "font": "unowned", "size": 10 }
                ],
                "songlist": {
                    "id": "songlist",
                    "center": 0,
                    "listoff": [{ "id": "bar", "dst": [{ "x": 10, "y": 50, "w": 40, "h": 10 }] }],
                    "liston": [{ "id": "bar", "dst": [{ "x": 12, "y": 50, "w": 40, "h": 10 }] }],
                    "text": [
                        { "id": "bartext-owned", "dst": [{ "x": 1, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned2", "dst": [{ "x": 2, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned3", "dst": [{ "x": 3, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned4", "dst": [{ "x": 4, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned5", "dst": [{ "x": 5, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned6", "dst": [{ "x": 6, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned7", "dst": [{ "x": 7, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-owned8", "dst": [{ "x": 8, "y": 2, "w": 20, "h": 8 }] },
                        { "id": "bartext-unowned", "dst": [{ "x": 9, "y": 2, "w": 20, "h": 8 }] }
                    ]
                },
                "destination": [{ "id": "songlist" }]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let snapshot = SelectSnapshot {
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                title: "Missing Song".to_string(),
                in_library: false,
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };

        let items = document.select_render_items(&sources, &snapshot);

        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                uv: TextureRegion { y: v, .. },
                ..
            } if approx_eq(*v, 40.0 / 100.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Text {
                text,
                style,
                ..
            } if text == "Missing Song" && style.font_id.as_deref() == Some("unowned"))));
    }

    #[test]
    fn select_skin_uses_snapshot_time_and_bar_type_ops() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "panel.png" }],
                "image": [
                    { "id": "song-panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "folder-panel", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "song-panel", "timer": 11, "loop": 200, "op": [2], "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 },
                        { "time": 200, "x": 20 }
                    ] },
                    { "id": "folder-panel", "op": [1], "dst": [
                        { "x": 50, "y": 0, "w": 10, "h": 10 }
                    ] },
                    { "id": "song-panel", "timer": 21, "op": [21], "dst": [
                        { "time": 0, "x": 30, "y": 0, "w": 10, "h": 10 },
                        { "time": 200, "x": 50 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("1", 100.0, 100.0);
        let snapshot = SelectSnapshot {
            time: bmz_core::time::TimeUs(100_000),
            selection_time: bmz_core::time::TimeUs(100_000),
            option_panel_time: bmz_core::time::TimeUs(100_000),
            option_panel: 1,
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                title: "Song".to_string(),
                is_folder: false,
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };

        let items = document.select_render_items(&sources, &snapshot);

        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.1) && approx_eq(*u, 0.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                rect: Rect { x, .. },
                ..
            } if approx_eq(*x, 0.4))));
    }

    #[test]
    fn skin_document_resolves_static_value_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "number.png" }],
                "value": [
                    { "id": "combo", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 104 },
                    { "id": "remain", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "expr": "number(106) - number(110) - number(111)" },
                    { "id": "unknown", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 9999 }
                ],
                "destination": [
                    { "id": "combo", "dst": [{ "x": 10, "y": 20, "w": 5, "h": 10 }] },
                    { "id": "remain", "dst": [{ "x": 10, "y": 30, "w": 5, "h": 10 }] },
                    { "id": "unknown", "dst": [{ "x": 10, "y": 40, "w": 5, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState {
                elapsed_ms: 0,
                combo: 45,
                total_notes: 100,
                judge_counts: DisplayJudgeCounts { pgreat: 30, great: 20, ..Default::default() },
                ..SkinDrawState::default()
            },
        );

        // combo=45 (2 digits), digit=3 → shiftbase=1, align=0 (right-aligned, default)
        // digit_step = 5/100 = 0.05, origin_x = 10/100 = 0.1
        // digit "4": x = 0.1 + 0.05 * (1+0) - 0 = 0.15
        // digit "5": x = 0.1 + 0.05 * (1+1) - 0 = 0.20
        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.15) && approx_eq(y, 0.7) && approx_eq(u, 0.4)));
        assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.20) && approx_eq(u, 0.5)));
        assert!(matches!(items[2], SkinRenderItem::Image {
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.15) && approx_eq(y, 0.6) && approx_eq(u, 0.5)));
        assert!(matches!(items[3], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.20) && approx_eq(u, 0.0)));
    }

    #[test]
    fn lane_cover_numbers_render_before_ready_while_changing() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "number.png" }],
                "value": [
                    { "id": "white", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 14 },
                    { "id": "green", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 313 },
                    { "id": "combo", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 104 }
                ],
                "destination": [
                    { "id": "white", "timer": 40, "op": [270], "dst": [{ "x": 10, "y": 20, "w": 5, "h": 10 }] },
                    { "id": "green", "timer": 40, "op": [270], "dst": [{ "x": 10, "y": 30, "w": 5, "h": 10 }] },
                    { "id": "combo", "timer": 40, "op": [270], "dst": [{ "x": 10, "y": 40, "w": 5, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let inactive = document.static_image_render_items(
            &sources,
            SkinDrawState { ready_timer_ms: None, ..SkinDrawState::default() },
        );
        assert!(inactive.is_empty());

        let active = document.static_image_render_items(
            &sources,
            SkinDrawState {
                ready_timer_ms: None,
                lane_cover_changing: true,
                lane_cover: 0.25,
                total_duration_ms: 300,
                combo: 123,
                ..SkinDrawState::default()
            },
        );
        assert_eq!(active.len(), 6);
    }

    #[test]
    fn skin_document_resolves_static_text_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "text": [
                    { "id": "title", "font": "main", "size": 8, "align": 1, "wrapping": true, "outlineColor": "ff000080", "outlineWidth": 1, "shadowColor": "00000080", "shadowOffsetX": 2, "shadowOffsetY": 3, "ref": 12 },
                    { "id": "genre", "size": 6, "align": 2, "overflow": 1, "ref": 13 },
                    { "id": "constant", "size": 5, "constantText": "READY" },
                    { "id": "numeric-constant", "size": 5, "constantText": 1 }
                ],
                "destination": [
                    { "id": "title", "dst": [{ "x": 10, "y": 20, "w": 50, "h": 10, "r": 128, "g": 200, "b": 255 }] },
                    { "id": "genre", "dst": [{ "x": 10, "y": 40, "w": 40, "h": 6 }] },
                    { "id": "constant", "dst": [{ "x": 10, "y": 60, "h": 5, "a": 128 }] },
                    { "id": "numeric-constant", "dst": [{ "x": 10, "y": 70, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let items = document.static_render_items(
            &HashMap::new(),
            SkinDrawState::default(),
            SkinTextState {
                title: "Song",
                subtitle: "Another",
                genre: "Techno",
                ..SkinTextState::default()
            },
        );

        assert_eq!(items.len(), 4);
        assert!(matches!(&items[0], SkinRenderItem::Text {
                origin: Point { x, y },
                text,
                style,
                ..
            } if approx_eq(*x, -0.15)
                && approx_eq(*y, 0.7)
                && text == "Song Another"
                && style.font_id.as_deref() == Some("main")
                && approx_eq(style.size, 0.1)
                && style.align == TextAlign::Center
                && style.wrapping
                && matches!(style.outline, Some(TextOutline { color, width })
                    if color == Color::rgba(1.0, 0.0, 0.0, 128.0 / 255.0)
                        && approx_eq(width, 0.01))
                && matches!(style.shadow, Some(TextShadow { color, offset })
                    if color == Color::rgba(0.0, 0.0, 0.0, 128.0 / 255.0)
                        && approx_eq(offset.x, 0.02)
                        && approx_eq(offset.y, 0.03))
                && approx_eq(style.max_width, 0.5)
                && style.color == Color::rgba(128.0 / 255.0, 200.0 / 255.0, 1.0, 1.0)));
        assert!(matches!(&items[1], SkinRenderItem::Text { text, style, .. }
                if text == "Techno"
                    && style.align == TextAlign::Right
                    && style.overflow == TextOverflow::Shrink
                    && approx_eq(style.max_width, 0.4)));
        assert!(
            matches!(&items[2], SkinRenderItem::Text { text, style, .. } if text == "READY" && approx_eq(style.color.a, 128.0 / 255.0))
        );
        assert!(matches!(&items[3], SkinRenderItem::Text { text, .. } if text == "1"));
    }

    #[test]
    fn skin_document_resolves_music_progress_slider() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "slider": [
                    { "id": "progress", "src": 1, "x": 10, "y": 20, "w": 5, "h": 6, "angle": 2, "range": 40, "type": 6 },
                    { "id": "lane-cover", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10, "angle": 2, "range": 20, "type": 4 },
                    { "id": "song-scroll", "src": 1, "x": 20, "y": 20, "w": 5, "h": 6, "angle": 2, "range": 40, "type": 1 },
                    { "id": "master", "src": 1, "x": 30, "y": 20, "w": 5, "h": 6, "angle": 1, "range": 40, "type": 17 },
                    { "id": "unknown", "src": 1, "x": 10, "y": 20, "w": 5, "h": 6, "angle": 0, "range": 40, "type": 999 }
                ],
                "destination": [
                    { "id": "progress", "blend": 2, "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
                    { "id": "lane-cover", "dst": [{ "x": 10, "y": 50, "w": 10, "h": 10 }] },
                    { "id": "song-scroll", "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
                    { "id": "master", "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
                    { "id": "unknown", "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState {
                play_progress: 0.25,
                select_scroll_progress: 0.5,
                select_master_volume: 0.75,
                ..SkinDrawState::default()
            },
        );

        assert_eq!(items.len(), 3);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uw, height: uh },
                blend,
                ..
            } if approx_eq(x, 0.3)
                && approx_eq(y, 0.24)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.06)
                && approx_eq(u, 0.1)
                && approx_eq(v, 0.2)
                && approx_eq(uw, 0.05)
                && approx_eq(uh, 0.06)
                && blend == BlendMode::Add));
        assert!(matches!(
            items[1],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.3) && approx_eq(y, 0.34)
        ));
        assert!(matches!(
            items[2],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.6) && approx_eq(y, 0.14)
        ));

        let no_lane_cover = document.static_image_render_items(
            &sources,
            SkinDrawState { lane_cover: 0.0, ..SkinDrawState::default() },
        );
        assert_eq!(no_lane_cover.len(), 3);

        let lane_cover = document.static_image_render_items(
            &sources,
            SkinDrawState { lane_cover: 0.5, ..SkinDrawState::default() },
        );
        assert_eq!(lane_cover.len(), 4);
        assert!(matches!(
            lane_cover[1],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.1) && approx_eq(y, 0.5)
        ));
    }

    #[test]
    fn skin_document_moves_sliders_in_beatoraja_directions() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "slider": [
                    { "id": "up", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 0, "range": 20, "type": 17 },
                    { "id": "right", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 1, "range": 20, "type": 17 },
                    { "id": "down", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 2, "range": 20, "type": 17 },
                    { "id": "left", "src": 1, "x": 0, "y": 0, "w": 5, "h": 5, "angle": 3, "range": 20, "type": 17 }
                ],
                "destination": [
                    { "id": "up", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] },
                    { "id": "right", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] },
                    { "id": "down", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] },
                    { "id": "left", "dst": [{ "x": 50, "y": 50, "w": 5, "h": 5 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { select_master_volume: 0.5, ..SkinDrawState::default() },
        );

        assert_eq!(items.len(), 4);
        assert!(matches!(
            items[0],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.5) && approx_eq(y, 0.35)
        ));
        assert!(matches!(
            items[1],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.6) && approx_eq(y, 0.45)
        ));
        assert!(matches!(
            items[2],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.5) && approx_eq(y, 0.55)
        ));
        assert!(matches!(
            items[3],
            SkinRenderItem::Image { rect: Rect { x, y, .. }, .. }
                if approx_eq(x, 0.4) && approx_eq(y, 0.45)
        ));
    }

    #[test]
    fn skin_document_resolves_end_of_note_timer_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "marker", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "marker", "timer": 143, "dst": [{ "x": 10, "y": 20, "w": 5, "h": 6 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let hidden = document.static_image_render_items(
            &sources,
            SkinDrawState { end_of_note: false, ..SkinDrawState::default() },
        );
        let visible = document.static_image_render_items(
            &sources,
            SkinDrawState {
                end_of_note: true,
                end_of_note_ms: Some(0),
                ..SkinDrawState::default()
            },
        );

        assert!(hidden.is_empty());
        assert_eq!(visible.len(), 1);
        assert!(matches!(visible[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, 0.1)
                && approx_eq(y, 0.74)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.06)));
    }

    #[test]
    fn skin_document_resolves_full_combo_timer_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "fc", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "fc", "timer": 48, "loop": -1, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 5, "h": 6, "a": 255 },
                        { "time": 1000, "a": 0 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let hidden = document.static_image_render_items(
            &sources,
            SkinDrawState { full_combo_ms: None, ..SkinDrawState::default() },
        );
        let visible = document.static_image_render_items(
            &sources,
            SkinDrawState { full_combo_ms: Some(500), ..SkinDrawState::default() },
        );

        assert!(hidden.is_empty());
        assert_eq!(visible.len(), 1);
        assert!(matches!(visible[0], SkinRenderItem::Image {
                tint: Color { a, .. },
                ..
            } if approx_eq(a, 128.0 / 255.0)));
    }

    #[test]
    fn skin_context_reports_timer_animation_duration() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "fc", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "fc", "timer": 48, "loop": -1, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 5, "h": 6 },
                        { "time": 1966, "a": 0 }
                    ] },
                    { "id": "other", "timer": 2, "dst": [{ "time": 3000 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let context =
            SkinContext::from_manifest_and_document(default_skin_manifest(), document, Vec::new());

        assert_eq!(context.timer_animation_duration_ms(48), 1966);
        assert_eq!(context.timer_animation_duration_ms(49), 0);
    }

    #[test]
    fn skin_document_resolves_fadeout_timer_destinations() {
        // timer=2 (TIMER_FADEOUT) はシーン終了アニメーション用。
        // fadeout_ms=None なら非アクティブで描画されず、Some なら経過 ms で
        // keyframe アニメーションが進行する。
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "curtain", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "curtain", "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 0 },
                        { "time": 200, "x": 0, "y": 0, "w": 100, "h": 100 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(7),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let inactive = document.static_image_render_items(
            &sources,
            SkinDrawState { fadeout_ms: None, ..SkinDrawState::default() },
        );
        let mid = document.static_image_render_items(
            &sources,
            SkinDrawState { fadeout_ms: Some(100), ..SkinDrawState::default() },
        );

        assert!(inactive.is_empty(), "fadeout timer is inactive when fadeout_ms is None");
        assert_eq!(mid.len(), 1);
        assert!(matches!(mid[0], SkinRenderItem::Image {
                rect: Rect { height, .. },
                ..
            } if approx_eq(height, 0.5)));
    }

    #[test]
    fn skin_document_resolves_special_black_fade_rect() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 6,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -110, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 200, "a": 255 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();

        let mid = document.static_image_render_items(
            &HashMap::new(),
            SkinDrawState { fadeout_ms: Some(100), ..SkinDrawState::default() },
        );

        assert_eq!(mid.len(), 1);
        assert!(matches!(mid[0], SkinRenderItem::Rect {
                rect: Rect { width, height, .. },
                color: Color { r, g, b, a },
                ..
            } if approx_eq(width, 1.0)
                && approx_eq(height, 1.0)
                && approx_eq(r, 0.0)
                && approx_eq(g, 0.0)
                && approx_eq(b, 0.0)
                && approx_eq(a, 128.0 / 255.0)));
    }

    #[test]
    fn skin_document_resolves_failed_timer_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -111, "timer": 3, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 100, "a": 255 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();

        let inactive = document.static_image_render_items(
            &HashMap::new(),
            SkinDrawState { failed_ms: None, ..SkinDrawState::default() },
        );
        let active = document.static_image_render_items(
            &HashMap::new(),
            SkinDrawState { failed_ms: Some(50), ..SkinDrawState::default() },
        );

        assert!(inactive.is_empty());
        assert_eq!(active.len(), 1);
        assert!(matches!(active[0], SkinRenderItem::Rect {
                color: Color { r, g, b, a },
                ..
            } if approx_eq(r, 1.0)
                && approx_eq(g, 1.0)
                && approx_eq(b, 1.0)
                && approx_eq(a, 128.0 / 255.0)));
    }

    #[test]
    fn src_zero_image_uses_black_pixel_crop() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": "system", "path": "system.png" }],
                "image": [
                    { "id": 7, "src": 0, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "black", "src": "bg", "x": 391, "y": 1080, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": 7, "timer": 3, "dst": [{ "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 200 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let images = document.image_map();
        let image = images.get("7").unwrap();
        let black = images.get("black").unwrap();
        let rect = skin_image_pixel_rect(image, &images);
        assert_eq!(rect, (black.x, black.y, black.w, black.h));
    }

    #[test]
    fn src_zero_with_explicit_crop_keeps_pixel_rect() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": "system", "path": "system.png" }],
                "image": [
                    { "id": "black", "src": "bg", "x": 391, "y": 1080, "w": 8, "h": 8 },
                    { "id": 15, "src": 0, "x": 16, "y": 0, "w": 8, "h": 8 }
                ]
            }
            "#,
        )
        .unwrap();
        let images = document.image_map();
        let image = images.get("15").unwrap();
        let rect = skin_image_pixel_rect(image, &images);
        assert_eq!(rect, (16, 0, 8, 8));
    }

    #[test]
    fn image_negative_crop_size_uses_remaining_source_extent() {
        let image = SkinImageDef {
            id: "frame".to_string(),
            src: "src".to_string(),
            x: 10,
            y: 20,
            w: -1,
            h: -1,
            divx: 1,
            divy: 1,
            timer: None,
            cycle: 0,
            len: 0,
            ref_id: 0,
            click: 0,
            act: None,
        };

        let uv =
            skin_image_texture_region(&image, SkinImageSize { width: 110.0, height: 220.0 }, 0);

        assert!(approx_eq(uv.x, 10.0 / 110.0));
        assert!(approx_eq(uv.y, 20.0 / 220.0));
        assert!(approx_eq(uv.width, 100.0 / 110.0));
        assert!(approx_eq(uv.height, 200.0 / 220.0));
    }

    /// Starseeker 閉店の `black` 相当: `src = "bg"` を `system` に解決し、timer 3 で暗転フェード。
    #[test]
    fn failed_close_black_fades_in_over_fullscreen() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "source": [{ "id": "system", "path": "system.png" }],
                "image": [{ "id": "black", "src": "bg", "x": 391, "y": 1080, "w": 8, "h": 8 }],
                "destination": [
                    { "id": "black", "loop": 1000, "timer": 3, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 1000, "a": 255 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("system", 1920.0, 1080.0);

        let inactive = document.static_image_render_items(
            &sources,
            SkinDrawState { failed_ms: None, ..SkinDrawState::default() },
        );
        let mid = document.static_image_render_items(
            &sources,
            SkinDrawState { failed_ms: Some(500), ..SkinDrawState::default() },
        );
        let (_, _, failed_overlay) = document.static_render_items_split(
            &sources,
            SkinDrawState { failed_ms: Some(500), ..SkinDrawState::default() },
            SkinTextState::default(),
        );

        assert!(inactive.is_empty());
        assert_eq!(mid.len(), 1);
        assert_eq!(failed_overlay.len(), 1);
        assert!(matches!(mid[0], SkinRenderItem::Image {
                rect: Rect { width, height, .. },
                tint: Color { a, .. },
                ..
            } if approx_eq(width, 1.0)
                && approx_eq(height, 1.0)
                && approx_eq(a, 128.0 / 255.0)));
    }

    #[test]
    fn skin_document_resolves_hidden_cover_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 12, "path": "cover.png" }],
                "hiddenCover": [
                    { "id": "hidden-cover", "src": 12, "x": 10, "y": 20, "w": 30, "h": 40 }
                ],
                "destination": [
                    { "id": "hidden-cover", "blend": 2, "dst": [{ "x": 20, "y": -40, "w": 30, "h": 40, "a": 128 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let hidden = document.static_image_render_items(&sources, SkinDrawState::default());
        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { hidden_cover: 1.0, ..SkinDrawState::default() },
        );

        assert!(hidden.is_empty());
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uw, height: uh },
                tint: Color { a, .. },
                blend,
                ..
            } if approx_eq(x, 0.2)
                && approx_eq(y, 1.0)
                && approx_eq(width, 0.3)
                && approx_eq(height, 0.4)
                && approx_eq(u, 0.1)
                && approx_eq(v, 0.2)
                && approx_eq(uw, 0.3)
                && approx_eq(uh, 0.4)
                && approx_eq(a, 128.0 / 255.0)
                && blend == BlendMode::Add));
        assert_eq!(document.hidden_cover[0].disappear_line, 0);
        assert!(document.hidden_cover[0].is_disappear_line_link_lift);
    }

    #[test]
    fn hidden_cover_clips_at_disappear_line() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "cover.png" }],
                "hiddenCover": [
                    { "id": "hidden-cover", "src": 12, "x": 0, "y": 0, "w": 390, "h": 580, "disapearLine": 140 }
                ],
                "destination": [
                    { "id": "hidden-cover", "dst": [{ "x": 20, "y": -440, "w": 390, "h": 580 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 390.0, height: 580.0 },
            },
        )]);

        let flush = document.static_image_render_items(
            &sources,
            SkinDrawState { hidden_cover: 1.0, ..SkinDrawState::default() },
        );
        let SkinRenderItem::Image { rect: flush_rect, uv: flush_uv, .. } = &flush[0] else {
            panic!("expected image");
        };
        // オフセット無し: 上端 (skin y=140) が disappearLine
        assert!(approx_eq(flush_rect.y, 580.0 / 720.0));
        assert!(approx_eq(flush_rect.height, 580.0 / 720.0));

        let clipped = document.static_image_render_items(
            &sources,
            SkinDrawState {
                hidden_cover: 1.0,
                offset_hidden_cover_px: 300,
                ..SkinDrawState::default()
            },
        );
        let SkinRenderItem::Image { rect: clipped_rect, uv: clipped_uv, .. } = &clipped[0] else {
            panic!("expected image");
        };
        // offset で上げた分、判定線より下を切り、上側 300px だけ残す
        assert!(approx_eq(clipped_rect.y, 280.0 / 720.0));
        assert!(approx_eq(clipped_rect.height, 300.0 / 720.0));
        assert!(approx_eq(flush_uv.height - clipped_uv.height, 280.0 / 580.0));
    }

    #[test]
    fn lift_cover_skipped_at_minimum_lift() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "lift.png" }],
                "image": [
                    { "id": "liftcover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723 }
                ],
                "hiddenCover": [
                    { "id": "hiddencover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "liftcover", "offset": 3, "dst": [{ "x": 20, "y": -366, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 431.0, height: 723.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() },
        );
        assert!(items.is_empty());
    }

    #[test]
    fn sudden_slider_draws_above_disappear_line() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "cover.png" }],
                "slider": [
                    { "id": "lanecover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "angle": 2, "range": 723, "type": 4 }
                ],
                "hiddenCover": [
                    { "id": "hiddencover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "lanecover", "dst": [{ "x": 20, "y": 1080, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 431.0, height: 723.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { lane_cover: 1.0, ..SkinDrawState::default() },
        );
        let SkinRenderItem::Image { rect, uv, .. } = &items[0] else {
            panic!("expected sudden+ lane cover image");
        };
        assert!(approx_eq(rect.height, 723.0 / 720.0));
        assert!(approx_eq(uv.height, 1.0));
    }

    #[test]
    fn lift_cover_clips_at_disappear_line() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 720,
                "h": 720,
                "source": [{ "id": 12, "path": "lift.png" }],
                "image": [
                    { "id": "liftcover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723 }
                ],
                "hiddenCover": [
                    { "id": "hiddencover", "src": 12, "x": 0, "y": 0, "w": 431, "h": 723, "disapearLine": 357 }
                ],
                "destination": [
                    { "id": "liftcover", "offset": 3, "dst": [{ "x": 20, "y": -366, "w": 431, "h": 723 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 431.0, height: 723.0 },
            },
        )]);

        let clipped = document.static_image_render_items(
            &sources,
            SkinDrawState { offset_lift_px: 200, ..SkinDrawState::default() },
        );
        let SkinRenderItem::Image { rect: clipped_rect, uv: clipped_uv, .. } = &clipped[0] else {
            panic!("expected image");
        };
        // offset 3 で 200px 上げた分、判定線より下を切り、上側 200px だけ残す
        assert!(approx_eq(clipped_rect.y, 163.0 / 720.0));
        assert!(approx_eq(clipped_rect.height, 200.0 / 720.0));
        assert!(approx_eq(clipped_uv.height, 200.0 / 723.0));
    }

    #[test]
    fn hidden_cover_destination_applies_lift_and_hidden_offsets() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 12, "path": "cover.png" }],
                "hiddenCover": [
                    { "id": "hidden-cover", "src": 12, "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "hidden-cover", "dst": [{ "x": 20, "y": -40, "w": 30, "h": 40 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "12".to_string(),
            SkinDocumentTexture {
                source_id: "12".to_string(),
                texture: SkinTextureId(42),
                source_size: SkinImageSize { width: 100.0, height: 100.0 },
            },
        )]);

        let items = document.static_image_render_items(
            &sources,
            SkinDrawState {
                hidden_cover: 0.5,
                offset_lift_px: 10,
                offset_hidden_cover_px: 20,
                ..SkinDrawState::default()
            },
        );

        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        assert!(
            approx_eq(rect.y, (100 - (-40 + 10 + 20) - 40) as f32 / 100.0),
            "expected hidden cover to use automatic lift and hidden offsets, got {}",
            rect.y
        );
    }

    #[test]
    fn skin_state_number_maps_play_value_refs() {
        let state = SkinDrawState {
            combo: 12,
            max_combo: 45,
            ex_score: 167,
            total_notes: 100,
            judge_counts: DisplayJudgeCounts {
                pgreat: 30,
                great: 20,
                good: 10,
                bad: 4,
                poor: 3,
                empty_poor: 2,
            },
            gauge: 78.6,
            fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
                fast_pgreat: 10,
                slow_pgreat: 11,
                fast_great: 12,
                slow_great: 13,
                fast_good: 14,
                slow_good: 15,
                fast_bad: 16,
                slow_bad: 17,
                fast_poor: 18,
                slow_poor: 19,
                fast_empty_poor: 20,
                slow_empty_poor: 21,
            }),
            best_ex_score: Some(123),
            target_ex_score: Some(145),
            judge_rank: Some(1),
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_number(71, state), Some(167));
        assert_eq!(skin_state_number(72, state), Some(200));
        assert_eq!(skin_state_number(74, state), Some(100));
        assert_eq!(skin_state_number(75, state), Some(45));
        assert_eq!(skin_state_number(76, state), Some(7));
        assert_eq!(skin_state_number(102, state), Some(83));
        assert_eq!(skin_state_number(103, state), Some(5));
        assert_eq!(skin_state_number(104, state), Some(12));
        assert_eq!(skin_state_number(107, state), Some(78));
        assert_eq!(skin_state_number(407, state), Some(6));
        assert_eq!(skin_state_number(110, state), Some(30));
        assert_eq!(skin_state_number(111, state), Some(20));
        assert_eq!(skin_state_number(112, state), Some(10));
        assert_eq!(skin_state_number(113, state), Some(4));
        assert_eq!(skin_state_number(114, state), Some(3));
        assert_eq!(skin_state_number(122, state), Some(72));
        assert_eq!(skin_state_number(123, state), Some(5));
        assert_eq!(skin_state_number(183, state), Some(61));
        assert_eq!(skin_state_number(184, state), Some(5));
        assert_eq!(skin_state_number(400, state), Some(1));
        assert_eq!(skin_state_number(420, state), Some(2));
        assert_eq!(skin_state_number(423, state), Some(70));
        assert_eq!(skin_state_number(424, state), Some(75));
        assert_eq!(skin_state_number(425, state), Some(7));
        assert_eq!(skin_state_number(426, state), Some(3));
        assert_eq!(skin_state_number(427, state), Some(7));
        assert!(test_skin_op(181, &[], state));
        assert!(!test_skin_op(182, &[], state));
    }

    #[test]
    fn skin_state_number_maps_beatoraja_point_score() {
        let state = SkinDrawState {
            key_mode: KeyMode::K7,
            max_combo: 45,
            total_notes: 100,
            judge_counts: DisplayJudgeCounts {
                pgreat: 30,
                great: 20,
                good: 10,
                bad: 4,
                poor: 3,
                empty_poor: 2,
            },
            ..SkinDrawState::default()
        };
        assert_eq!(skin_state_number(100, state), Some(89_500));

        let five_key = SkinDrawState { key_mode: KeyMode::K5, ..state };
        assert_eq!(skin_state_number(100, five_key), Some(55_000));
    }

    #[test]
    fn display_signed_number_digits_uses_sign_cell_and_row_offset() {
        // divx=12, divy=2 のレイアウト想定
        // 正数 +12 (max_digits=5): [sign+(11), blank(10 or 0), blank, 1, 2]
        // 実装は内側のみ zero_pad、行0
        let positive = display_signed_number_digits(12, 5, true, 12);
        assert_eq!(positive.first(), Some(&11)); // sign cell
        assert_eq!(positive.last(), Some(&2));
        assert!(positive.iter().all(|&d| d < 12), "positive digits should be in row 0");

        // 負数 -12 (max_digits=5): row 1 (offset=12)
        let negative = display_signed_number_digits(-12, 5, true, 12);
        assert_eq!(negative.first(), Some(&(11 + 12))); // sign cell row 1
        assert_eq!(negative.last(), Some(&(2 + 12)));
        assert!(negative.iter().all(|&d| d >= 12), "negative digits should be in row 1");

        // 0 は正側
        let zero = display_signed_number_digits(0, 5, true, 12);
        assert_eq!(zero.first(), Some(&11));
        assert!(zero.iter().all(|&d| d < 12));
    }

    #[test]
    fn skin_state_number_maps_result_value_refs() {
        let fast_slow = crate::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 350,
            slow_pgreat: 427,
            fast_great: 180,
            slow_great: 154,
            fast_good: 12,
            slow_good: 10,
            fast_bad: 2,
            slow_bad: 1,
            fast_poor: 3,
            slow_poor: 2,
            fast_empty_poor: 5,
            slow_empty_poor: 4,
        };
        let state = SkinDrawState {
            ex_score: 1888,
            max_combo: 777,
            total_notes: 1000,
            past_notes: 1000,
            judge_counts: DisplayJudgeCounts {
                pgreat: 777,
                great: 334,
                good: 22,
                bad: 3,
                poor: 5,
                empty_poor: 9,
            },
            fast_slow_counts: Some(fast_slow),
            best_ex_score: Some(1700),
            best_clear_index: Some(6),
            target_ex_score: Some(1900),
            best_max_combo: Some(800),
            target_max_combo: Some(1000),
            best_bp: Some(20),
            previous_best_ex_score: Some(1800),
            previous_best_max_combo: Some(700),
            previous_best_bp: Some(10),
            target_bp: Some(0),
            target_clear_index: Some(8),
            result_failed: Some(false),
            average_timing_ms: Some(-12.34),
            stddev_timing_ms: Some(56.78),
            ..SkinDrawState::default()
        };

        // 符号付き差分
        assert_eq!(skin_state_number(170, state), Some(1700));
        assert_eq!(skin_state_number(121, state), Some(1900));
        assert_eq!(skin_state_number(151, state), Some(1900));
        assert_eq!(skin_state_number(122, state), Some(95));
        assert_eq!(skin_state_number(123, state), Some(0));
        assert_eq!(skin_state_number(135, state), Some(95));
        assert_eq!(skin_state_number(136, state), Some(0));
        assert_eq!(skin_state_number(157, state), Some(95));
        assert_eq!(skin_state_number(158, state), Some(0));
        assert_eq!(skin_state_number(183, state), Some(85));
        assert_eq!(skin_state_number(184, state), Some(0));
        assert_eq!(skin_state_number(152, state), Some(1888 - 1700));
        assert_eq!(skin_state_number(172, state), Some(1888 - 1700));
        assert_eq!(skin_state_number(153, state), Some(1888 - 1900));
        assert_eq!(skin_state_number(173, state), Some(1000));
        assert_eq!(skin_state_number(175, state), Some(777 - 1000));
        assert_eq!(skin_state_number(176, state), Some(0));
        // 現在 bp = bad+poor = 8、target = 0 → diff = 8
        assert_eq!(skin_state_number(178, state), Some(8));
        assert_eq!(skin_state_number(371, state), Some(6));
        assert!(test_skin_op(321, &[], state));
        assert!(!test_skin_op(320, &[], state));
        assert!(test_skin_op(330, &[], state));
        assert!(!test_skin_op(1330, &[], state));
        assert!(test_skin_op(331, &[], state));
        assert!(!test_skin_op(1331, &[], state));
        assert!(test_skin_op(332, &[], state));
        assert!(!test_skin_op(1332, &[], state));
        assert!(test_skin_op(335, &[], state));
        assert!(!test_skin_op(1335, &[], state));
        assert!(test_skin_op(300, &[], state));
        assert!(test_skin_op(310, &[], state));
        assert!(!test_skin_op(301, &[], state));
        assert!(!test_skin_op(308, &[], state));
        assert!(test_skin_op(350, &[], state));
        assert!(!test_skin_op(351, &[], state));
        assert!(!test_skin_op(352, &[], state));
        assert!(test_skin_op(353, &[], state));
        assert!(!test_skin_op(354, &[], state));

        let draw_state = SkinDrawState {
            ex_score: 1800,
            max_combo: 700,
            total_notes: 1000,
            judge_counts: DisplayJudgeCounts { bad: 5, poor: 5, ..DisplayJudgeCounts::default() },
            previous_best_ex_score: Some(1800),
            previous_best_max_combo: Some(700),
            previous_best_bp: Some(10),
            target_ex_score: Some(1800),
            result_failed: Some(false),
            ..SkinDrawState::default()
        };
        assert!(test_skin_op(1330, &[], draw_state));
        assert!(test_skin_op(1331, &[], draw_state));
        assert!(test_skin_op(1332, &[], draw_state));
        assert!(test_skin_op(1335, &[], draw_state));
        assert!(test_skin_op(354, &[], draw_state));

        let zero_rank_state = SkinDrawState {
            ex_score: 0,
            total_notes: 1000,
            result_failed: Some(true),
            ..SkinDrawState::default()
        };
        assert!(test_skin_op(308, &[], zero_rank_state));
        assert!(test_skin_op(318, &[], zero_rank_state));

        // Fast/Slow 内訳
        assert_eq!(skin_state_number(410, state), Some(350));
        assert_eq!(skin_state_number(411, state), Some(427));
        assert_eq!(skin_state_number(412, state), Some(180));
        assert_eq!(skin_state_number(413, state), Some(154));
        assert_eq!(skin_state_number(414, state), Some(12));
        assert_eq!(skin_state_number(415, state), Some(10));
        assert_eq!(skin_state_number(416, state), Some(2));
        assert_eq!(skin_state_number(417, state), Some(1));
        assert_eq!(skin_state_number(418, state), Some(3));
        assert_eq!(skin_state_number(419, state), Some(2));
        assert_eq!(skin_state_number(421, state), Some(5));
        assert_eq!(skin_state_number(422, state), Some(4));
        // TOTAL_EARLY = fast 全合計 = 350+180+12+2+3 = 547
        assert_eq!(skin_state_number(423, state), Some(547));
        // TOTAL_LATE = slow 全合計 = 427+154+10+1+2 = 594
        assert_eq!(skin_state_number(424, state), Some(594));

        // Result timing distribution
        assert_eq!(skin_state_number(374, state), Some(-12));
        assert_eq!(skin_state_number(375, state), Some(-34));
        assert_eq!(skin_state_number(376, state), Some(56));
        assert_eq!(skin_state_number(377, state), Some(78));

        // best/target が None のとき None を返す
        let bare = SkinDrawState::default();
        assert_eq!(skin_state_number(152, bare), None);
        assert_eq!(skin_state_number(173, bare), None);
        assert_eq!(skin_state_number(410, bare), None);
        assert_eq!(skin_state_number(374, bare), None);
    }

    #[test]
    fn best_and_target_scores_follow_note_progress() {
        let state = SkinDrawState {
            ex_score: 450,
            total_notes: 1000,
            past_notes: 250,
            best_ex_score: Some(1800),
            target_ex_score: Some(1600),
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_number(150, state), Some(450));
        assert_eq!(skin_state_number(170, state), Some(450));
        assert_eq!(skin_state_number(121, state), Some(400));
        assert_eq!(skin_state_number(151, state), Some(400));
        assert_eq!(skin_state_number(152, state), Some(0));
        assert_eq!(skin_state_number(172, state), Some(0));
        assert_eq!(skin_state_number(153, state), Some(50));
    }

    #[test]
    fn target_score_timer_and_ops_follow_current_ex_score() {
        let below = SkinDrawState {
            elapsed_ms: 1234,
            ex_score: 1599,
            total_notes: 900,
            target_ex_score: Some(1600),
            ..SkinDrawState::default()
        };
        let reached = SkinDrawState { ex_score: 1600, ..below };
        let updated = SkinDrawState { ex_score: 1601, ..below };

        assert_eq!(skin_timer_elapsed_ms(Some(352), below), None);
        assert_eq!(skin_timer_elapsed_ms(Some(352), reached), Some(1234));
        assert!(test_skin_op(1336, &[], reached));
        assert!(!test_skin_op(336, &[], reached));
        assert!(test_skin_op(336, &[], updated));
    }

    #[test]
    fn result_timers_follow_result_state() {
        let inactive = SkinDrawState::default();
        assert_eq!(skin_timer_elapsed_ms(Some(150), inactive), None);
        assert_eq!(skin_timer_elapsed_ms(Some(151), inactive), None);
        assert_eq!(skin_timer_elapsed_ms(Some(152), inactive), None);
        assert_eq!(skin_timer_elapsed_ms(Some(172), inactive), None);
        assert_eq!(skin_timer_elapsed_ms(Some(173), inactive), None);
        assert_eq!(skin_timer_elapsed_ms(Some(174), inactive), None);

        let active = SkinDrawState {
            result_graph_begin_ms: Some(120),
            result_graph_end_ms: Some(120),
            result_update_score_ms: Some(40),
            ..SkinDrawState::default()
        };
        assert_eq!(skin_timer_elapsed_ms(Some(150), active), Some(120));
        assert_eq!(skin_timer_elapsed_ms(Some(151), active), Some(120));
        assert_eq!(skin_timer_elapsed_ms(Some(152), active), Some(40));
    }

    #[test]
    fn end_of_note_timers_use_elapsed_since_end_of_note() {
        let inactive =
            SkinDrawState { elapsed_ms: 5_000, end_of_note_ms: None, ..SkinDrawState::default() };
        assert_eq!(skin_timer_elapsed_ms(Some(143), inactive), None);
        assert_eq!(skin_timer_elapsed_ms(Some(144), inactive), None);

        let active = SkinDrawState {
            elapsed_ms: 5_000,
            end_of_note: true,
            end_of_note_ms: Some(250),
            ..SkinDrawState::default()
        };
        assert_eq!(skin_timer_elapsed_ms(Some(143), active), Some(250));
        assert_eq!(skin_timer_elapsed_ms(Some(144), active), Some(250));
    }

    #[test]
    fn ir_skin_properties_use_offline_defaults() {
        let state = SkinDrawState::default();

        assert!(test_skin_op(50, &[], state));
        assert!(!test_skin_op(51, &[], state));
        for op in 601..=608 {
            assert!(!test_skin_op(op, &[], state), "IR option {op} should be false offline");
        }

        for ref_id in [179, 180, 181, 182, 200, 201, 202, 220, 226, 227, 241, 242] {
            assert_eq!(skin_state_number(ref_id, state), None, "IR number {ref_id}");
        }
    }

    #[test]
    fn skin_state_number_maps_select_refs() {
        let state = SkinDrawState {
            select_chart_count: 42,
            select_screen: true,
            select_play_level: 12,
            select_clear_index: 5,
            select_total_notes: 1200,
            select_bpm: 148.0,
            select_chart_normal_notes: 900,
            select_chart_long_notes: 180,
            select_chart_scratch_notes: 100,
            select_chart_long_scratch_notes: 20,
            select_chart_density: 4.56,
            select_chart_peak_density: 12.34,
            select_chart_end_density: 7.89,
            select_chart_total_gauge: 260.0,
            select_chart_main_bpm: 150.0,
            select_min_bpm: 120.0,
            select_max_bpm: 180.0,
            select_length_ms: 183_000,
            select_master_volume: 0.575,
            select_key_volume: 0.59,
            select_bgm_volume: 0.28,
            select_mode_index: 4,
            select_sort_index: 6,
            select_ln_mode_index: 2,
            ex_score: 1234,
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_number(11, state), Some(4));
        assert_eq!(skin_state_number(12, state), Some(6));
        assert_eq!(skin_state_number(300, state), Some(42));
        assert_eq!(skin_state_number(96, state), Some(12));
        assert_eq!(
            skin_state_number(
                96,
                SkinDrawState { select_play_level: 12, play_level: 9, ..SkinDrawState::default() }
            ),
            Some(9)
        );
        assert_eq!(skin_state_number(370, state), Some(5));
        assert_eq!(skin_state_number(74, state), Some(1200));
        assert_eq!(skin_state_number(90, state), Some(180));
        assert_eq!(skin_state_number(91, state), Some(120));
        assert_eq!(skin_state_number(92, state), Some(150));
        assert_eq!(skin_state_number(160, state), Some(148));
        assert_eq!(skin_state_number(350, state), Some(900));
        assert_eq!(skin_state_number(351, state), Some(180));
        assert_eq!(skin_state_number(352, state), Some(100));
        assert_eq!(skin_state_number(353, state), Some(20));
        assert_eq!(skin_state_number(360, state), Some(12));
        assert_eq!(skin_state_number(361, state), Some(34));
        assert_eq!(skin_state_number(362, state), Some(7));
        assert_eq!(skin_state_number(363, state), Some(89));
        assert_eq!(skin_state_number(364, state), Some(4));
        assert_eq!(skin_state_number(365, state), Some(56));
        assert_eq!(skin_state_number(368, state), Some(260));
        assert_eq!(skin_state_number(71, state), Some(1234));
        assert_eq!(skin_state_number(1163, state), Some(3));
        assert_eq!(skin_state_number(1164, state), Some(3));
        assert_eq!(skin_state_number(57, state), Some(57));
        assert_eq!(skin_state_number(58, state), Some(59));
        assert_eq!(skin_state_number(59, state), Some(28));
        assert_eq!(skin_state_number(308, state), Some(2));

        assert!(skin_state_number(21, state).is_some_and(|value| value >= 2026));
        assert!(skin_state_number(22, state).is_some_and(|value| (1..=12).contains(&value)));
        assert!(skin_state_number(23, state).is_some_and(|value| (1..=31).contains(&value)));
        assert!(skin_state_number(24, state).is_some_and(|value| (0..=23).contains(&value)));
        assert!(skin_state_number(25, state).is_some_and(|value| (0..=59).contains(&value)));
        assert!(skin_state_number(26, state).is_some_and(|value| (0..=59).contains(&value)));
    }

    #[test]
    fn skin_state_imageset_index_maps_select_options() {
        let state = SkinDrawState {
            select_arrange_index: 2,
            select_gauge_index: 4,
            select_target_index: 3,
            select_bga_index: 1,
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_imageset_index(42, state), Some(2));
        assert_eq!(skin_state_imageset_index(43, state), Some(2));
        assert_eq!(skin_state_imageset_index(40, state), Some(4));
        assert_eq!(skin_state_imageset_index(41, state), Some(3));
        assert_eq!(skin_state_imageset_index(72, state), Some(1));
        assert_eq!(skin_state_imageset_index(301, state), Some(0));
        assert_eq!(skin_state_imageset_index(500, state), None);
    }

    #[test]
    fn select_target_index_maps_fixed_targets() {
        assert_eq!(select_target_index("NONE"), 0);
        assert_eq!(select_target_index("MAX"), 1);
        assert_eq!(select_target_index("AAA"), 2);
        assert_eq!(select_target_index("AA"), 3);
        assert_eq!(select_target_index("A"), 4);
        assert_eq!(select_target_index("B"), 5);
        assert_eq!(select_target_index("C"), 6);
        assert_eq!(select_target_index("D"), 7);
        assert_eq!(select_target_index("E"), 8);
    }

    #[test]
    fn bundled_beatoraja_default_play7_json_loads_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/play7.json");
        if !path.is_file() {
            return;
        }

        let document = SkinDocument::load_beatoraja_json(&path).unwrap();

        assert_eq!(document.name, "beatoraja default");
        assert_eq!(document.w, 1280);
        assert_eq!(document.h, 720);
        assert!(document.source_map().contains_key("7"));
        assert!(document.image_map().contains_key("note-w"));
        assert_eq!(document.note.as_ref().unwrap().id, "notes");
        assert!(!document.destination.is_empty());
    }

    #[test]
    fn bundled_beatoraja_default_select_json_loads_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/select.json");
        if !path.is_file() {
            return;
        }

        let document = SkinDocument::load_beatoraja_json(&path).unwrap();

        assert_eq!(document.name, "beatoraja default");
        assert_eq!(document.skin_type, 5);
        assert!(document.songlist.is_some());
        assert!(!document.destination.is_empty());
    }

    #[test]
    fn local_ecfn_converted_play7_json_loads_when_available() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7-1p.json");
        if !path.is_file() {
            return;
        }

        let document = SkinDocument::load_beatoraja_json(&path).unwrap();

        assert!(!document.destination.is_empty());
    }

    #[test]
    fn local_ecfn_converted_select_json_loads_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ECFN/select/select-converted.json");
        if !path.is_file() {
            return;
        }

        let document = SkinDocument::load_beatoraja_json(&path).unwrap();

        assert_eq!(document.skin_type, 5);
        assert!(document.songlist.is_some());
        assert!(!document.destination.is_empty());
    }

    #[test]
    fn stretch_applied_to_judge_destination() {
        // stretch=9 (resize_about_center) should resize the image to its source dimensions
        // centered on the destination rect.
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "effect.png" }],
                "image": [{ "id": "judge-pg", "src": 1, "x": 0, "y": 0, "w": 50, "h": 20 }],
                "judge": [{
                    "id": "judge-1p",
                    "index": 0,
                    "images": [
                        { "id": "judge-pg", "stretch": 9, "dst": [
                            { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100 }
                        ]}
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(5),
                source_size: SkinImageSize { width: 50.0, height: 20.0 },
            },
        )]);

        let items = document.judge_render_items("PGREAT", 0, 0, &sources).unwrap();

        // stretch=9: resize_about_center places the 50x20 source centered in 100x100 destination.
        // In normalized coords (canvas 100x100):
        //   dest rect: x=0/100=0, y=0/100=0, w=100/100=1, h=100/100=1
        //   source size: 50x20 pixels → w=50/100=0.5, h=20/100=0.2
        //   centered: x = 0 + (1 - 0.5)*0.5 = 0.25, y = 0 + (1 - 0.2)*0.5 = 0.4
        assert!(matches!(
            items[0],
            SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                ..
            } if approx_eq(x, 0.25)
                && approx_eq(y, 0.4)
                && approx_eq(width, 0.5)
                && approx_eq(height, 0.2)
        ));
    }

    #[test]
    fn filter_nonzero_destination_returns_linear_filter_item() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "filter": 1, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(3),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        let items = document.static_image_render_items(&sources, SkinDrawState::default());

        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], SkinRenderItem::Image { linear_filter: true, .. }));
    }

    #[test]
    fn bomb_timer_activates_only_for_active_lane() {
        // timer=51 maps to bomb Key1 (TIMER_BOMB_1P_KEY1 = 50 + Lane::Key1.index() = 51)
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "bomb.png" }],
                "image": [{ "id": "bomb-img", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "bomb-img", "timer": 51, "dst": [
                        { "time": 0, "x": 10, "y": 10, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(9),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        // All lanes inactive → no items
        let inactive_state = SkinDrawState::default();
        let items_inactive = document.static_image_render_items(&sources, inactive_state);
        assert_eq!(items_inactive.len(), 0, "should be empty when all bomb timers are None");

        // Key1 (index=1) active → items returned
        let active_state = SkinDrawState {
            bomb_ms: {
                let mut a = [None; LANE_COUNT];
                a[1] = Some(0);
                a
            },
            ..SkinDrawState::default()
        };
        let items_active = document.static_image_render_items(&sources, active_state);
        assert_eq!(items_active.len(), 1, "should have one item when Key1 bomb timer is active");
    }

    #[test]
    fn judge_timer_elapsed_ms_selects_animation_frame() {
        // timer=46 → TIMER_JUDGE_1P; two dst frames at time=0 and time=200
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "system.png" }],
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "panel", "timer": 46, "dst": [
                        { "time": 0,   "x": 0,   "y": 0, "w": 10, "h": 10 },
                        { "time": 200, "x": 50,  "y": 0, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();
        let sources = HashMap::from([(
            "1".to_string(),
            SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(2),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);

        // judge_ms=Some(100) → between frame 0 and frame 200 → x should be 0.25 (interpolated)
        let state_early = SkinDrawState {
            judge_ms: judge_region_state(0, 100, 0).judge_ms,
            ..SkinDrawState::default()
        };
        let items_early = document.static_image_render_items(&sources, state_early);
        assert_eq!(items_early.len(), 1);
        assert!(
            matches!(items_early[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.25)),
            "at judge_ms=100, x should interpolate to 0.25 (halfway between 0 and 0.5)"
        );

        // judge_ms=Some(300) → past last frame → last frame x=0.5
        let state_late = SkinDrawState {
            judge_ms: judge_region_state(0, 300, 0).judge_ms,
            ..SkinDrawState::default()
        };
        let items_late = document.static_image_render_items(&sources, state_late);
        assert_eq!(items_late.len(), 1);
        assert!(
            matches!(items_late[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.5)),
            "at judge_ms=300 (past last frame), x should be at last frame x=0.5"
        );

        // judge_ms=None → no items (timer inactive)
        let state_inactive =
            SkinDrawState { judge_ms: [None; MAX_JUDGE_REGIONS], ..SkinDrawState::default() };
        let items_inactive = document.static_image_render_items(&sources, state_inactive);
        assert_eq!(items_inactive.len(), 0, "judge_ms=None should produce no items");
    }

    #[test]
    fn dst_if_value_selects_frame_by_enabled_option() {
        // property: option 920 enabled (1P)
        // destination dst has two conditional frames: one for 920, one for 921
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "property": [
                    { "name": "Side", "def": "1P", "item": [
                        { "name": "1P", "op": 920 },
                        { "name": "2P", "op": 921 }
                    ]}
                ],
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "dst": [
                        { "if": [920], "value": { "time": 0, "x": 100, "y": 200, "w": 50, "h": 50 } },
                        { "if": [921], "value": { "time": 0, "x": 900, "y": 200, "w": 50, "h": 50 } },
                        { "time": 500 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 10.0, 10.0);
        let state = SkinDrawState::default();
        let items = document.static_image_render_items(&sources, state);

        // With option 920 (1P) enabled, x should be 100/1280
        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        assert!(approx_eq(rect.x, 100.0 / 1280.0), "expected 1P x position, got {}", rect.x);
    }

    #[test]
    fn dst_if_value_uses_default_when_option_disabled() {
        // No property → no enabled options → conditional frame skipped, only end frame {time:500}.
        // 最初のキーフレーム時刻 (500) より前は描画されず、500ms 以降に既定位置 (0,0) で描画される。
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "dst": [
                        { "if": [920], "value": { "time": 0, "x": 100, "y": 200, "w": 50, "h": 50 } },
                        { "time": 500 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 10.0, 10.0);

        // elapsed=0: 最初のキーフレーム時刻 (500) より前なので描画しない。
        let before = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 0, ..SkinDrawState::default() },
        );
        assert!(before.is_empty(), "destination is not drawn before its first keyframe time");

        // elapsed=500: 条件フレームが skip され、{time:500} の既定位置 (0,0) で描画される。
        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 500, ..SkinDrawState::default() },
        );
        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        assert!(approx_eq(rect.x, 0.0), "expected default x=0, got {}", rect.x);
        assert!(approx_eq(rect.y, 1.0), "expected default y=1, got {}", rect.y);
    }

    #[test]
    fn offset_lift_shifts_destination_y() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 3, "dst": [
                        { "time": 0, "x": 100, "y": 200, "w": 50, "h": 50 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 10.0, 10.0);
        let state_no_lift = SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() };
        let state_lifted = SkinDrawState { offset_lift_px: 72, ..SkinDrawState::default() };

        let items_no_lift = document.static_image_render_items(&sources, state_no_lift);
        let items_lifted = document.static_image_render_items(&sources, state_lifted);

        assert_eq!(items_no_lift.len(), 1);
        assert_eq!(items_lifted.len(), 1);

        let SkinRenderItem::Image { rect: rect_no_lift, .. } = &items_no_lift[0] else { panic!() };
        let SkinRenderItem::Image { rect: rect_lifted, .. } = &items_lifted[0] else { panic!() };

        // With lift=72px on a 720h canvas, beatoraja y shifts upward in bottom-origin space.
        assert!(approx_eq(rect_no_lift.y, (720 - 200 - 50) as f32 / 720.0));
        assert!(
            approx_eq(rect_lifted.y, (720 - (200 + 72) - 50) as f32 / 720.0),
            "expected y shifted by lift, got {}",
            rect_lifted.y
        );
    }

    #[test]
    fn offset_lanecover_shifts_destination_y() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 4, "dst": [
                        { "time": 0, "x": 0, "y": 720, "w": 50, "h": 50 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 10.0, 10.0);
        // lanecover=0.5, lift=0 → offset_lanecover_px = (0-1)*720*0.5 = -360
        let state = SkinDrawState { offset_lanecover_px: -360, ..SkinDrawState::default() };
        let items = document.static_image_render_items(&sources, state);

        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        // y=720 shifted by -360 in bottom-origin space: top = 720 - (720 - 360 + 50).
        assert!(
            approx_eq(rect.y, (720 - (720 - 360 + 50)) as f32 / 720.0),
            "expected shifted y, got {}",
            rect.y
        );
    }

    #[test]
    fn custom_offset_adjusts_destination_geometry_and_alpha() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 42, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 30, "h": 40, "a": 200 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 10.0, 10.0);
        let mut offsets = SkinOffsetValues::default();
        offsets.set(
            42,
            crate::skin_offset::SkinOffsetValue { x: 6, y: 8, w: 10, h: 12, r: 0, a: -50 },
        );
        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() },
        );

        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, tint, .. } = &items[0] else { panic!() };
        assert!(approx_eq(rect.x, (10 + 6 - 10 / 2) as f32 / 100.0));
        assert!(approx_eq(rect.y, (100 - (20 + 8 - 12 / 2) - (40 + 12)) as f32 / 100.0));
        assert!(approx_eq(rect.width, 40.0 / 100.0));
        assert!(approx_eq(rect.height, 52.0 / 100.0));
        assert!(approx_eq(tint.a, 150.0 / 255.0));
    }

    #[test]
    fn all_offset_transforms_play_skin_render_item() {
        let mut offsets = SkinOffsetValues::default();
        offsets.set(
            OFFSET_ALL,
            crate::skin_offset::SkinOffsetValue { x: 10, y: 20, w: 50, h: -50, r: 0, a: 0 },
        );
        let item = SkinRenderItem::Image {
            texture: SkinTextureId(1),
            rect: Rect { x: 0.2, y: 0.4, width: 0.1, height: 0.2 },
            uv: TextureRegion::default(),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: None,
            linear_filter: false,
        };

        let item = apply_all_offset_to_render_item(
            item,
            SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() },
        );

        let SkinRenderItem::Image { rect, .. } = item else { panic!() };
        assert!(approx_eq(rect.x, 0.4));
        assert!(approx_eq(rect.y, 0.0));
        assert!(approx_eq(rect.width, 0.15));
        assert!(approx_eq(rect.height, 0.1));
    }

    #[test]
    fn notes_offset_adjusts_note_rect() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 100, "h": 100,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "dst": [{ "time": 0, "x": 10, "y": 20, "w": 30, "h": 40 }]
                }
            }
            "#,
        )
        .unwrap();
        let mut offsets = SkinOffsetValues::default();
        offsets.set(
            OFFSET_NOTES_1P,
            crate::skin_offset::SkinOffsetValue { x: 0, y: 0, w: 0, h: 20, r: 0, a: 0 },
        );

        let area = document.note_lane_area(Lane::Key1, KeyMode::K7, &[]).unwrap();
        let center_y = area.y + area.height * 0.5;
        let rect = document.apply_notes_offset_to_rect(
            Rect { x: area.x, y: center_y - 0.05, width: area.width, height: 0.1 },
            SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() },
        );

        assert!(approx_eq(rect.y, 0.45));
        assert!(approx_eq(rect.height, 0.3));
    }

    #[test]
    fn note_rect_for_progress_shifts_with_lift() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 720, "h": 720,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "dst": [{ "time": 0, "x": 10, "y": 140, "w": 50, "h": 580 }]
                }
            }
            "#,
        )
        .unwrap();
        let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
        let note_height = 12.0 / 720.0;
        let state_no_lift = SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() };
        let state_lifted = SkinDrawState { offset_lift_px: 72, ..SkinDrawState::default() };

        let rect_no_lift = skin
            .note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.0, note_height, state_no_lift)
            .unwrap();
        let rect_lifted = skin
            .note_rect_for_progress(Lane::Key1, KeyMode::K7, 0.0, note_height, state_lifted)
            .unwrap();

        let judge_no_lift = 580.0 / 720.0;
        let judge_lifted = judge_no_lift - 72.0 / 720.0;
        assert!(approx_eq(rect_no_lift.y + note_height, judge_no_lift));
        assert!(approx_eq(rect_lifted.y + note_height, judge_lifted));
        assert!(
            rect_lifted.y < rect_no_lift.y,
            "expected lifted note higher on screen, got no_lift={} lifted={}",
            rect_no_lift.y,
            rect_lifted.y
        );
    }

    #[test]
    fn note_body_rect_shifts_with_lift() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 720, "h": 720,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "dst": [{ "time": 0, "x": 10, "y": 140, "w": 50, "h": 580 }]
                }
            }
            "#,
        )
        .unwrap();
        let skin = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
        let state_no_lift = SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() };
        let state_lifted = SkinDrawState { offset_lift_px: 72, ..SkinDrawState::default() };

        let rect_no_lift =
            skin.note_body_rect(Lane::Key1, KeyMode::K7, 0.0, 0.5, state_no_lift).unwrap();
        let rect_lifted =
            skin.note_body_rect(Lane::Key1, KeyMode::K7, 0.0, 0.5, state_lifted).unwrap();

        assert!(
            rect_lifted.y < rect_no_lift.y,
            "expected lifted long body higher on screen, got no_lift={} lifted={}",
            rect_no_lift.y,
            rect_lifted.y
        );
        assert!(rect_lifted.height <= rect_no_lift.height + 0.0001);
    }

    #[test]
    fn judge_offset_height_keeps_image_and_combo_y_aligned() {
        // beatoraja は SkinNumber を `setRelative(true)` で扱うため、
        // OFFSET_JUDGE_1P.h を変えても 判定文字 (image) とコンボ数 (number)
        // の Y 位置は同じ量だけシフトする (中心アンカー伸縮)。
        // 過去には number_frame にも x/y シフトが二重適用され、
        // 判定文字とコンボ数の Y がずれていた。
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "judge.png" }],
                "image": [{ "id": "judgef-pg", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "value": [{
                    "id": "combo-num", "src": "src",
                    "x": 0, "y": 10, "w": 10, "h": 20,
                    "divx": 10, "divy": 1, "digit": 4, "ref": 102
                }],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judgef-pg", "offsets": [32], "dst": [
                            { "time": 0, "x": 10, "y": 20, "w": 30, "h": 10 },
                            { "time": 500 }
                        ]}
                    ],
                    "numbers": [
                        { "id": "combo-num", "offsets": [32], "dst": [
                            { "time": 0, "x": 0, "y": 30, "w": 10, "h": 20 },
                            { "time": 500 }
                        ]}
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("src", 10.0, 10.0);

        fn render_y_positions(
            document: &SkinDocument,
            sources: &HashMap<String, SkinDocumentTexture>,
            offset_h: i32,
        ) -> (f32, f32) {
            let mut offsets = SkinOffsetValues::default();
            offsets.set(
                OFFSET_JUDGE_1P,
                crate::skin_offset::SkinOffsetValue { x: 0, y: 0, w: 0, h: offset_h, r: 0, a: 0 },
            );
            let items = document
                .judge_render_items_with_offsets("PGREAT", 42, 0, offsets, sources)
                .unwrap();
            // [0] = 判定文字 image, [1..] = combo digit images
            let SkinRenderItem::Image { rect: image_rect, .. } = &items[0] else {
                panic!("first item should be image")
            };
            let SkinRenderItem::Image { rect: combo_rect, .. } = &items[1] else {
                panic!("second item should be first combo digit")
            };
            (image_rect.y + image_rect.height / 2.0, combo_rect.y + combo_rect.height / 2.0)
        }

        let (image_center_y_0, combo_center_y_0) = render_y_positions(&document, &sources, 0);
        let (image_center_y_h, combo_center_y_h) = render_y_positions(&document, &sources, 20);

        let image_shift = image_center_y_h - image_center_y_0;
        let combo_shift = combo_center_y_h - combo_center_y_0;
        assert!(
            approx_eq(image_shift, combo_shift),
            "image Y shift {image_shift} should match combo Y shift {combo_shift}"
        );
    }

    #[test]
    fn judge_offset_alpha_applies_to_judge_image_and_combo() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "judge.png" }],
                "image": [{ "id": "judgef-pg", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "value": [{
                    "id": "combo-num", "src": "src",
                    "x": 0, "y": 10, "w": 10, "h": 20,
                    "divx": 10, "divy": 1, "digit": 4, "ref": 102
                }],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judgef-pg", "offsets": [32], "dst": [
                            { "time": 0, "x": 10, "y": 20, "w": 30, "h": 10, "a": 200 },
                            { "time": 500 }
                        ]}
                    ],
                    "numbers": [
                        { "id": "combo-num", "offsets": [32], "dst": [
                            { "time": 0, "x": 0, "y": 30, "w": 10, "h": 20, "a": 200 },
                            { "time": 500 }
                        ]}
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("src", 10.0, 10.0);
        let mut offsets = SkinOffsetValues::default();
        offsets.set(
            OFFSET_JUDGE_1P,
            crate::skin_offset::SkinOffsetValue { x: 0, y: 0, w: 0, h: 0, r: 0, a: -80 },
        );

        let items =
            document.judge_render_items_with_offsets("PGREAT", 42, 0, offsets, &sources).unwrap();

        let SkinRenderItem::Image { tint: judge_tint, .. } = &items[0] else { panic!() };
        let SkinRenderItem::Image { tint: combo_tint, .. } = &items[1] else { panic!() };
        let expected = (200.0 - 80.0) / 255.0;
        assert!(approx_eq(judge_tint.a, expected), "judge alpha {}", judge_tint.a);
        assert!(approx_eq(combo_tint.a, expected), "combo alpha {}", combo_tint.a);
    }

    #[test]
    fn judge_offset_applies_to_judge_special_renderer() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "judge.png" }],
                "image": [{ "id": "judgef-pg", "src": "src", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "judge": [{
                    "id": "judge",
                    "images": [
                        { "id": "judgef-pg", "offsets": [32], "dst": [{ "time": 0, "x": 10, "y": 20, "w": 30, "h": 10 }, { "time": 500 }] }
                    ]
                }]
            }
            "#,
        )
        .unwrap();
        let sources = mock_source("src", 10.0, 10.0);
        let mut offsets = SkinOffsetValues::default();
        offsets.set(
            OFFSET_JUDGE_1P,
            crate::skin_offset::SkinOffsetValue { x: 6, y: 0, w: 0, h: 0, r: 0, a: 0 },
        );

        let items =
            document.judge_render_items_with_offsets("PGREAT", 0, 0, offsets, &sources).unwrap();

        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        assert!(approx_eq(rect.x, 0.16));
    }

    #[test]
    fn destination_angle_and_center_emit_rotated_image() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "center": 1, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 30, "h": 40, "angle": 90 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 10.0, 10.0);
        let items = document.static_image_render_items(&sources, SkinDrawState::default());

        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0],
            SkinRenderItem::RotatedImage { angle_deg, center, .. }
                if approx_eq(angle_deg, 90.0) && approx_eq(center.x, 0.0) && approx_eq(center.y, 1.0)
        ));
    }

    #[test]
    fn graph_renders_vertical_bar_proportional_to_score() {
        // BARGRAPH_SCORERATE (110): ex_score / (total_notes * 2)
        // total_notes=100, ex_score=100 → value=0.5
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "score-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 200, "type": 110 }],
                "destination": [
                    { "id": "score-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 100, "h": 480 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("bar-src", 100.0, 200.0);
        let state = SkinDrawState { ex_score: 100, total_notes: 100, ..SkinDrawState::default() };
        let items = document.static_image_render_items(&sources, state);

        assert_eq!(items.len(), 1, "expected one graph bar");
        let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
        // value=0.5 → height = 480/720 * 0.5; destination bottom is y=0 in beatoraja space.
        let dst_h = 480.0 / 720.0;
        assert!(
            approx_eq(rect.height, dst_h * 0.5),
            "bar height should be half: got {}",
            rect.height
        );
        assert!(
            approx_eq(rect.y, 1.0 - dst_h * 0.5),
            "bar y should start at half-height: got {}",
            rect.y
        );
        // UV should also be clipped to bottom half
        assert!(approx_eq(uv.height, 0.5), "uv height should be 0.5, got {}", uv.height);
        assert!(approx_eq(uv.y, 0.5), "uv y should be 0.5, got {}", uv.y);
    }

    #[test]
    fn graph_renders_horizontal_bar_for_load_progress() {
        // BARGRAPH_LOAD_PROGRESS (102): always 1.0
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "load-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 8, "angle": 0, "type": 102 }],
                "destination": [
                    { "id": "load-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 640, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("bar-src", 100.0, 8.0);
        let state = SkinDrawState::default();
        let items = document.static_image_render_items(&sources, state);

        assert_eq!(items.len(), 1, "expected one load bar");
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        // value=1.0 → full width = 640/1280 = 0.5
        assert!(approx_eq(rect.width, 640.0 / 1280.0), "full load bar width: got {}", rect.width);
    }

    #[test]
    fn graph_music_progress_uses_play_progress() {
        // BARGRAPH_MUSIC_PROGRESS (101): play_progress=0.75 → bar is 75% full
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "music-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 8, "angle": 0, "type": 101 }],
                "destination": [
                    { "id": "music-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1280, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("bar-src", 100.0, 8.0);
        let state = SkinDrawState { play_progress: 0.75, ..SkinDrawState::default() };
        let items = document.static_image_render_items(&sources, state);

        assert_eq!(items.len(), 1, "expected one music bar");
        let SkinRenderItem::Image { rect, uv, .. } = &items[0] else { panic!() };
        // value=0.75 → width = 1280/1280 * 0.75 = 0.75
        assert!(approx_eq(rect.width, 0.75), "music bar width should be 0.75, got {}", rect.width);
        assert!(approx_eq(uv.width, 0.75), "music bar uv.width should be 0.75, got {}", uv.width);
    }

    #[test]
    fn graph_rate_pgreat_uses_judge_count_over_past_notes() {
        // BARGRAPH_RATE_PGREAT (140): pgreat / past_notes
        // pgreat=60, past_notes=100 → 0.6
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "bar-src", "path": "bar.png" }],
                "graph": [{ "id": "pg-bar", "src": "bar-src", "x": 0, "y": 0, "w": 100, "h": 8, "angle": 0, "type": 140 }],
                "destination": [
                    { "id": "pg-bar", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1000, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("bar-src", 100.0, 8.0);
        let state = SkinDrawState {
            judge_counts: DisplayJudgeCounts { pgreat: 60, great: 30, ..Default::default() },
            past_notes: 100,
            total_notes: 200,
            ..SkinDrawState::default()
        };
        let items = document.static_image_render_items(&sources, state);

        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        // value=0.6, dst_width = 1000/1280
        assert!(approx_eq(rect.width, 1000.0 / 1280.0 * 0.6), "pg bar width: got {}", rect.width);
    }

    #[test]
    fn value_number_right_aligns_by_default() {
        // 3-digit number "42" in a 5-digit area (align=0, default right-aligned)
        // shiftbase=3 → first digit at position 3, second at 4
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "val", "src": "src", "x": 0, "y": 0, "w": 100, "h": 20, "divx": 10, "digit": 5, "ref": 104 }],
                "destination": [
                    { "id": "val", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 20, "h": 20 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 100.0, 20.0);
        // combo=42, total_notes=100 → ref 104 = combo = 42 → 2 digits
        let state = SkinDrawState {
            elapsed_ms: 0,
            combo: 42,
            total_notes: 100,
            ..SkinDrawState::default()
        };
        let items = document.static_image_render_items(&sources, state);

        // 2 digits in a 5-digit space, right-aligned: shiftbase=3
        // digit_width = 20/1280, digit_step = digit_width (space=0)
        // digit 0 ("4"): x = 0 + step * (3 + 0) - 0 = 3 * step
        // digit 1 ("2"): x = 0 + step * (3 + 1) - 0 = 4 * step
        assert_eq!(items.len(), 2);
        let digit_width = 20.0 / 1280.0;
        let SkinRenderItem::Image { rect: r0, .. } = &items[0] else { panic!() };
        let SkinRenderItem::Image { rect: r1, .. } = &items[1] else { panic!() };
        assert!(
            approx_eq(r0.x, 3.0 * digit_width),
            "first digit x={} expected {}",
            r0.x,
            3.0 * digit_width
        );
        assert!(
            approx_eq(r1.x, 4.0 * digit_width),
            "second digit x={} expected {}",
            r1.x,
            4.0 * digit_width
        );
    }

    #[test]
    fn volume_number_uses_blank_padding_and_digit_cell_width() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1920, "h": 1080,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "volume", "src": "src", "x": 2401, "y": 510, "w": 242, "h": 15, "divx": 11, "digit": 3, "ref": 57 }],
                "destination": [
                    { "id": "volume", "dst": [{ "time": 0, "x": 1717, "y": 360, "w": 22, "h": 15 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 3200.0, 3200.0);
        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { select_master_volume: 0.37, ..SkinDrawState::default() },
        );

        assert_eq!(items.len(), 3);
        let SkinRenderItem::Image { rect: r0, uv: uv0, .. } = &items[0] else { panic!() };
        let SkinRenderItem::Image { rect: r1, uv: uv1, .. } = &items[1] else { panic!() };
        let SkinRenderItem::Image { rect: r2, uv: uv2, .. } = &items[2] else { panic!() };
        let digit_width = 22.0 / 1920.0;
        assert!(approx_eq(r0.width, digit_width));
        assert!(approx_eq(r1.width, digit_width));
        assert!(approx_eq(r2.width, digit_width));
        assert!(approx_eq(r1.x - r0.x, digit_width));
        assert!(approx_eq(r2.x - r1.x, digit_width));
        assert!(approx_eq(uv0.width, 22.0 / 3200.0));
        assert!(approx_eq(uv1.width, 22.0 / 3200.0));
        assert!(approx_eq(uv2.width, 22.0 / 3200.0));
        assert!(approx_eq(uv0.x, (2401.0 + 10.0 * 22.0) / 3200.0));
        assert!(approx_eq(uv1.x, (2401.0 + 3.0 * 22.0) / 3200.0));
        assert!(approx_eq(uv2.x, (2401.0 + 7.0 * 22.0) / 3200.0));
        assert!(
            approx_eq(uv0.width, 242.0 / 11.0 / 3200.0),
            "value sprite must be sliced into 11 cells, got uv.width={}",
            uv0.width
        );
    }

    #[test]
    fn value_number_slices_source_with_beatoraja_integer_division() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "volume", "src": "src", "x": 3114, "y": 0, "w": 99, "h": 12, "divx": 10, "digit": 3, "ref": 57, "align": 2 }],
                "destination": [
                    { "id": "volume", "dst": [{ "time": 0, "x": 560, "y": 480, "w": 12, "h": 12 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let source_width = 3224.0;
        let sources = mock_source("src", source_width, 1024.0);
        let items = document.static_image_render_items(
            &sources,
            SkinDrawState { select_master_volume: 0.37, ..SkinDrawState::default() },
        );

        assert_eq!(items.len(), 2);
        let SkinRenderItem::Image { uv: uv0, .. } = &items[0] else { panic!() };
        let SkinRenderItem::Image { uv: uv1, .. } = &items[1] else { panic!() };
        assert!(
            approx_eq(uv0.width, 9.0 / source_width),
            "beatoraja slices 99px / 10 as 9px cells, got {}",
            uv0.width * source_width
        );
        assert!(approx_eq(uv0.x, (3114.0 + 3.0 * 9.0) / source_width));
        assert!(approx_eq(uv1.x, (3114.0 + 7.0 * 9.0) / source_width));
    }

    #[test]
    fn value_number_left_aligns_when_align_1() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "num.png" }],
                "value": [{ "id": "val", "src": "src", "x": 0, "y": 0, "w": 100, "h": 20, "divx": 10, "digit": 5, "align": 1, "ref": 104 }],
                "destination": [
                    { "id": "val", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 20, "h": 20 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let sources = mock_source("src", 100.0, 20.0);
        let state = SkinDrawState {
            elapsed_ms: 0,
            combo: 42,
            total_notes: 100,
            ..SkinDrawState::default()
        };
        let items = document.static_image_render_items(&sources, state);

        // left-aligned: shift = 3 * step, digit 0 at 0, digit 1 at step
        assert_eq!(items.len(), 2);
        let digit_width = 20.0 / 1280.0;
        let SkinRenderItem::Image { rect: r0, .. } = &items[0] else { panic!() };
        let SkinRenderItem::Image { rect: r1, .. } = &items[1] else { panic!() };
        assert!(approx_eq(r0.x, 0.0), "first digit x={} expected 0", r0.x);
        assert!(approx_eq(r1.x, digit_width), "second digit x={} expected {}", r1.x, digit_width);
    }

    #[test]
    fn skin_state_number_hispeed_and_timeleft() {
        let state = SkinDrawState { hispeed: 1.5, timeleft_ms: 90_500, ..SkinDrawState::default() };
        // NUMBER_HISPEED (310) = integer part = 1
        assert_eq!(skin_state_number(310, state), Some(1));
        // NUMBER_HISPEED_AFTERDOT (311) = decimal part × 100 = 50
        assert_eq!(skin_state_number(311, state), Some(50));
        // NUMBER_TIMELEFT_MINUTE (163) = 90500 / 60000 = 1
        assert_eq!(skin_state_number(163, state), Some(1));
        // NUMBER_TIMELEFT_SECOND (164) = (90500 / 1000) % 60 = 90 % 60 = 30
        assert_eq!(skin_state_number(164, state), Some(30));
    }

    #[test]
    fn skin_state_number_bpm_lanecover_duration_timing() {
        let state = SkinDrawState {
            now_bpm: 148.7,
            min_bpm: 80.0,
            max_bpm: 200.3,
            lane_cover: 0.25,
            total_duration_ms: 183_000,
            judge_timing_ms: Some(-3),
            ..SkinDrawState::default()
        };
        // NUMBER_NOWBPM (160) = round(148.7) = 149
        assert_eq!(skin_state_number(160, state), Some(149));
        // NUMBER_MINBPM (91) = round(80.0) = 80
        assert_eq!(skin_state_number(91, state), Some(80));
        // NUMBER_MAXBPM (90) = round(200.3) = 200
        assert_eq!(skin_state_number(90, state), Some(200));
        // NUMBER_LANECOVER1 (14) = round(0.25 * 1000) = 250
        assert_eq!(skin_state_number(14, state), Some(250));
        // NUMBER_LIFT1 (314) = round(0.42 * 1000) = 420
        let lifted = SkinDrawState { lift: 0.42, ..state };
        assert_eq!(skin_state_number(314, lifted), Some(420));
        // float_number(113) tracks BARGRAPH_BESTSCORERATE
        let best_rate = SkinDrawState {
            total_notes: 100,
            best_ex_score: Some(150),
            ..SkinDrawState::default()
        };
        assert!((skin_state_float_number(113, best_rate).unwrap() - 0.75).abs() < 0.001);
        assert!(!eval_skin_draw_condition("float_number(113) == 0", best_rate));
        assert!(eval_skin_draw_condition(
            "float_number(113) == 0",
            SkinDrawState { total_notes: 100, best_ex_score: Some(0), ..SkinDrawState::default() }
        ));
        // NUMBER_DURATION (312) = current note display duration in ms.
        assert_eq!(skin_state_number(312, state), Some(183_000));
        // NUMBER_DURATION_GREEN (313) = duration * 3 / 5.
        assert_eq!(skin_state_number(313, state), Some(109_800));
        assert_eq!(
            skin_state_number(313, SkinDrawState { total_duration_ms: 183_001, ..state }),
            Some(109_801)
        );
        // VALUE_JUDGE_1P_DURATION (525) = abs(-3) = 3
        assert_eq!(skin_state_number(525, state), Some(3));
        // When no recent judgement, 525 returns None
        let no_judge = SkinDrawState { judge_timing_ms: None, ..state };
        assert_eq!(skin_state_number(525, no_judge), None);
    }

    #[test]
    fn skin_value_number_evaluates_value_expr() {
        let state = SkinDrawState { total_duration_ms: 183_000, ..SkinDrawState::default() };
        let value = SkinValueDef {
            id: "lanecover-green".to_string(),
            src: String::new(),
            value_expr: "0.6*number(312)".to_string(),
            ..Default::default()
        };
        assert_eq!(skin_value_number(&value, state), Some(109_800));
    }

    #[test]
    fn skin_state_float_expr_evaluates_option_weighted_terms() {
        let expr = "0.102*option(180)*number(350)+0.09*option(181)*number(350)";
        let very_hard = SkinDrawState {
            judge_rank: Some(0),
            select_screen: true,
            select_total_notes: 100,
            ..SkinDrawState::default()
        };
        let hard = SkinDrawState {
            judge_rank: Some(1),
            select_screen: true,
            select_total_notes: 100,
            ..SkinDrawState::default()
        };

        assert!((skin_state_float_expr(expr, very_hard).unwrap() - 10.2).abs() < 0.001);
        assert!((skin_state_float_expr(expr, hard).unwrap() - 9.0).abs() < 0.001);
    }

    #[test]
    fn skin_state_number_best_and_target_score() {
        let state = SkinDrawState {
            best_ex_score: Some(1500),
            target_ex_score: Some(800),
            ..SkinDrawState::default()
        };
        // NUMBER_HIGHSCORE (150)
        assert_eq!(skin_state_number(150, state), Some(1500));
        // NUMBER_TARGET_SCORE (121)
        assert_eq!(skin_state_number(121, state), Some(800));
        let ghost_projected = SkinDrawState {
            best_ex_score: Some(1500),
            projected_best_ex_score: Some(321),
            ex_score: 400,
            ..SkinDrawState::default()
        };
        assert_eq!(skin_state_number(150, ghost_projected), Some(321));
        assert_eq!(skin_state_number(152, ghost_projected), Some(79));
        // When None → None
        let no_scores = SkinDrawState::default();
        assert_eq!(skin_state_number(150, no_scores), None);
        assert_eq!(skin_state_number(121, no_scores), None);
    }

    #[test]
    fn graph_value_bestscorerate_fills_bar_proportionally() {
        // BARGRAPH_BESTSCORERATE (113): best / (total_notes * 2)
        // best=800, total=500 → 800/1000 = 0.8
        let state = SkinDrawState {
            best_ex_score: Some(800),
            total_notes: 500,
            ..SkinDrawState::default()
        };
        let v = graph_value(113, state);
        assert!((v - 0.8).abs() < 1e-5, "best score rate: expected 0.8, got {v}");
    }

    #[test]
    fn graph_value_targetscorerate_fills_bar_proportionally() {
        // BARGRAPH_TARGETSCORERATE (115): target / (total_notes * 2)
        // target=600, total=600 → 600/1200 = 0.5
        let state = SkinDrawState {
            target_ex_score: Some(600),
            total_notes: 600,
            ..SkinDrawState::default()
        };
        let v = graph_value(115, state);
        assert!((v - 0.5).abs() < 1e-5, "target score rate: expected 0.5, got {v}");
    }

    #[test]
    fn graph_value_bestscorerate_now_scales_with_past_notes() {
        // BARGRAPH_BESTSCORERATE_NOW (112): best * past / (total^2 * 2)
        // best=160 (80% of max 200), past=50, total=100
        // → 160 * 50 / (100^2 * 2) = 8000 / 20000 = 0.4
        // = best_rate(0.8) × play_fraction(0.5) = 0.4
        let state = SkinDrawState {
            best_ex_score: Some(160),
            past_notes: 50,
            total_notes: 100,
            ..SkinDrawState::default()
        };
        let v = graph_value(112, state);
        assert!((v - 0.4).abs() < 1e-4, "best now rate: expected 0.4, got {v}");
    }

    #[test]
    fn graph_value_bestscorerate_now_uses_projected_best_score() {
        let state = SkinDrawState {
            best_ex_score: Some(160),
            projected_best_ex_score: Some(100),
            past_notes: 50,
            total_notes: 100,
            ..SkinDrawState::default()
        };

        let v = graph_value(112, state);

        assert!((v - 0.5).abs() < 1e-4, "best ghost now rate: expected 0.5, got {v}");
    }

    #[test]
    fn graph_value_returns_zero_when_no_best_score() {
        let state = SkinDrawState { total_notes: 100, ..SkinDrawState::default() };
        assert_eq!(graph_value(113, state), 0.0);
        assert_eq!(graph_value(115, state), 0.0);
    }

    #[test]
    fn skin_state_text_maps_string_refs() {
        let state = SkinTextState {
            title: "My Title",
            subtitle: "Sub",
            artist: "Artist Name",
            subartist: "Feat. X",
            genre: "TRANCE",
            target: "AAA",
            course_titles: [
                "Stage 1", "Stage 2", "Stage 3", "Stage 4", "Stage 5", "Stage 6", "Stage 7",
                "Stage 8", "Stage 9", "Stage 10",
            ],
            ..SkinTextState::default()
        };

        let make_text = |ref_id: i32| SkinTextDef {
            id: "t".to_string(),
            ref_id,
            constant_text: String::new(),
            ..SkinTextDef::default()
        };

        // STRING_TITLE (10)
        assert_eq!(skin_state_text(&make_text(10), state), "My Title");
        // STRING_SUBTITLE (11)
        assert_eq!(skin_state_text(&make_text(11), state), "Sub");
        // STRING_FULLTITLE (12) = title + " " + subtitle
        assert_eq!(skin_state_text(&make_text(12), state), "My Title Sub");
        // STRING_GENRE (13)
        assert_eq!(skin_state_text(&make_text(13), state), "TRANCE");
        // STRING_ARTIST (14)
        assert_eq!(skin_state_text(&make_text(14), state), "Artist Name");
        // STRING_SUBARTIST (15)
        assert_eq!(skin_state_text(&make_text(15), state), "Feat. X");
        // STRING_FULLARTIST (16) = artist + " " + subartist
        assert_eq!(skin_state_text(&make_text(16), state), "Artist Name Feat. X");
        // STRING_TARGET (3)
        assert_eq!(skin_state_text(&make_text(3), state), "RANK AAA");
        // STRING_TARGETNAME_P1/N1 (209/210)
        assert_eq!(skin_state_text(&make_text(209), state), "MAX");
        assert_eq!(skin_state_text(&make_text(210), state), "RANK AA");
        // STRING_COURSE1_TITLE..10_TITLE (150..159)
        assert_eq!(skin_state_text(&make_text(150), state), "Stage 1");
        assert_eq!(skin_state_text(&make_text(159), state), "Stage 10");
        // STRING_IR_NAME / STRING_IR_USERNAME: BMZ has no IR backend yet.
        assert_eq!(skin_state_text(&make_text(1020), state), "");
        assert_eq!(skin_state_text(&make_text(1021), state), "");
        // Unknown ref → empty
        assert_eq!(skin_state_text(&make_text(99), state), "");

        let m_select_bar_text =
            SkinTextDef { id: "default_songlist2_bartext".to_string(), ..SkinTextDef::default() };
        assert_eq!(
            skin_state_text(
                &m_select_bar_text,
                SkinTextState { bar_text: "Song Title", ..SkinTextState::default() },
            ),
            "Song Title"
        );
    }

    #[test]
    fn text_render_item_applies_search_word_alpha_multiplier_for_ref_30() {
        let document: SkinDocument =
            serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
        let text = SkinTextDef { id: "search".to_string(), ref_id: 30, ..SkinTextDef::default() };
        let frame = ResolvedSkinFrame { w: 100, h: 24, ..ResolvedSkinFrame::default() };
        let state = SkinTextState {
            search_word: "hello",
            search_word_alpha: 0.5,
            ..SkinTextState::default()
        };
        let item = document.text_render_item(&text, frame, state).unwrap();
        match item {
            SkinRenderItem::Text { style, .. } => {
                // frame.a=255 (1.0) * 0.5 = 0.5
                assert!((style.color.a - 0.5).abs() < 1e-4, "got alpha {}", style.color.a);
            }
            other => panic!("expected SkinRenderItem::Text, got {other:?}"),
        }
    }

    #[test]
    fn text_render_item_leaves_alpha_unchanged_for_other_refs() {
        let document: SkinDocument =
            serde_json::from_value(serde_json::json!({ "w": 1920, "h": 1080 })).unwrap();
        let text = SkinTextDef {
            id: "title".to_string(),
            ref_id: 10, // title, not search
            ..SkinTextDef::default()
        };
        let frame = ResolvedSkinFrame { w: 100, h: 24, ..ResolvedSkinFrame::default() };
        let state = SkinTextState {
            title: "song name",
            search_word_alpha: 0.1, // should be ignored for non-search refs
            ..SkinTextState::default()
        };
        let item = document.text_render_item(&text, frame, state).unwrap();
        match item {
            SkinRenderItem::Text { style, .. } => {
                assert!((style.color.a - 1.0).abs() < 1e-4, "got alpha {}", style.color.a);
            }
            other => panic!("expected SkinRenderItem::Text, got {other:?}"),
        }
    }

    #[test]
    fn text_render_item_separates_bitmap_font_size_from_destination_height() {
        let document: SkinDocument = serde_json::from_value(serde_json::json!({
            "w": 100,
            "h": 100,
            "font": [
                { "id": "bitmap", "path": "artist.fnt" },
                { "id": "vector", "path": "artist.ttf" }
            ]
        }))
        .unwrap();
        let frame = ResolvedSkinFrame { w: 80, h: 28, ..ResolvedSkinFrame::default() };
        let state = SkinTextState::default();
        let bitmap_text = SkinTextDef {
            id: "artist".to_string(),
            font: "result:bitmap".to_string(),
            size: 17,
            constant_text: "Aoi".to_string(),
            ..SkinTextDef::default()
        };
        let vector_text = SkinTextDef {
            id: "artist_vector".to_string(),
            font: "vector".to_string(),
            size: 17,
            constant_text: "Aoi".to_string(),
            ..SkinTextDef::default()
        };

        let bitmap_item = document.text_render_item(&bitmap_text, frame, state).unwrap();
        let vector_item = document.text_render_item(&vector_text, frame, state).unwrap();

        match bitmap_item {
            SkinRenderItem::Text { style, .. } => {
                assert!(approx_eq(style.size, 0.28), "got {}", style.size);
                assert_eq!(style.bitmap_size, Some(0.17));
            }
            other => panic!("expected SkinRenderItem::Text, got {other:?}"),
        }
        match vector_item {
            SkinRenderItem::Text { style, .. } => {
                assert!(approx_eq(style.size, 0.28), "got {}", style.size);
                assert_eq!(style.bitmap_size, None);
            }
            other => panic!("expected SkinRenderItem::Text, got {other:?}"),
        }
    }

    #[test]
    fn skin_state_text_uses_constant_text_over_ref_id() {
        let state = SkinTextState { title: "Ignored", ..SkinTextState::default() };
        let text = SkinTextDef {
            id: "t".to_string(),
            ref_id: 10,
            constant_text: "Hardcoded".to_string(),
            ..SkinTextDef::default()
        };
        assert_eq!(skin_state_text(&text, state), "Hardcoded");
    }

    #[test]
    fn format_rm_skin_course_table_text_matches_lua_branches() {
        use crate::snapshot::CourseStageMarker;

        assert_eq!(
            format_rm_skin_course_table_text(Some(CourseStageMarker::Final), "", "", ""),
            "COURSE : STAGE FINAL"
        );
        assert_eq!(
            format_rm_skin_course_table_text(
                Some(CourseStageMarker::Stage2),
                "[★] Insane",
                "★12",
                "[★] Insane"
            ),
            "COURSE : STAGE 2"
        );
        assert_eq!(
            format_rm_skin_course_table_text(None, "[★] Insane", "★12", "[★] Insane"),
            "[★] Insane > ★12"
        );
        assert_eq!(format_rm_skin_course_table_text(None, "", "★12", "[★] Insane"), " > ★12");
        assert_eq!(
            format_rm_skin_course_table_text(None, "[★] Insane", "", "[★] Insane"),
            "[★] Insane"
        );
        assert_eq!(format_rm_skin_course_table_text(None, "", "", ""), "# No-Table");
    }

    #[test]
    fn skin_state_text_course_table_uses_value_expr_and_table_id() {
        use crate::snapshot::CourseStageMarker;

        let state = SkinTextState {
            table_level: "★12",
            table_text_primary: "[★] Insane",
            table_text_secondary: "★12",
            table_text_fallback: "[★] Insane",
            course_stage: None,
            ..SkinTextState::default()
        };
        let by_expr = SkinTextDef {
            id: "table".to_string(),
            value_expr: SKIN_EXPR_COURSE_TABLE_TEXT.to_string(),
            ..SkinTextDef::default()
        };
        assert_eq!(skin_state_text(&by_expr, state), "[★] Insane > ★12");

        let by_id = SkinTextDef { id: "table".to_string(), ..SkinTextDef::default() };
        assert_eq!(skin_state_text(&by_id, state), "[★] Insane > ★12");

        let course_state = SkinTextState { course_stage: Some(CourseStageMarker::Stage1), ..state };
        assert_eq!(skin_state_text(&by_id, course_state), "COURSE : STAGE 1");

        let by_ref = SkinTextDef { ref_id: 1002, ..SkinTextDef::default() };
        assert_eq!(skin_state_text(&by_ref, state), "★12");
    }

    #[test]
    fn full_label_handles_empty_components() {
        // both empty
        assert_eq!(full_label("", ""), "");
        // only primary
        assert_eq!(full_label("Title", ""), "Title");
        // only secondary
        assert_eq!(full_label("", "Sub"), "Sub");
        // both present
        assert_eq!(full_label("Title", "Sub"), "Title Sub");
    }

    fn mock_source(id: &str, width: f32, height: f32) -> HashMap<String, SkinDocumentTexture> {
        let mut map = HashMap::new();
        map.insert(
            id.to_string(),
            SkinDocumentTexture {
                source_id: id.to_string(),
                texture: SkinTextureId(9999),
                source_size: SkinImageSize { width, height },
            },
        );
        map
    }

    #[test]
    fn note_lane_area_resolves_flat_frame_dst_after_expansion() {
        // load_beatoraja_json が expand_json_skin_value で条件ブロックを展開すると
        // note.dst はレーン順の Frame エントリ列になる。全レーンが正しく解決されること。
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "note": {
                    "dst": [
                        {"x": 90, "y": 140, "w": 50, "h": 580},
                        {"x": 140, "y": 140, "w": 40, "h": 580},
                        {"x": 180, "y": 140, "w": 50, "h": 580},
                        {"x": 230, "y": 140, "w": 40, "h": 580},
                        {"x": 270, "y": 140, "w": 50, "h": 580},
                        {"x": 320, "y": 140, "w": 40, "h": 580},
                        {"x": 360, "y": 140, "w": 50, "h": 580},
                        {"x": 20, "y": 140, "w": 70, "h": 580}
                    ]
                }
            }
            "#,
        )
        .unwrap();

        let enabled: Vec<i32> = vec![];
        // Key1 is index 0 → first Frame
        let area = document.note_lane_area(Lane::Key1, KeyMode::K7, &enabled).unwrap();
        assert!(approx_eq(area.x, 90.0 / 1280.0));
        assert!(approx_eq(area.y, 0.0));
        assert!(approx_eq(area.width, 50.0 / 1280.0));
        assert!(approx_eq(area.height, 580.0 / 720.0));
        // Key2 is index 1 → second Frame
        let area2 = document.note_lane_area(Lane::Key2, KeyMode::K7, &enabled).unwrap();
        assert!(approx_eq(area2.x, 140.0 / 1280.0));
        assert!(approx_eq(area2.width, 40.0 / 1280.0));
        // Scratch is index 7 → eighth Frame
        let scratch = document.note_lane_area(Lane::Scratch, KeyMode::K7, &enabled).unwrap();
        assert!(approx_eq(scratch.x, 20.0 / 1280.0));
        assert!(approx_eq(scratch.width, 70.0 / 1280.0));
    }

    #[test]
    fn loop_at_cycle_end_holds_final_frame() {
        // loop == cycle（終端へループバック）: 1回再生して最終フレームを保持する。
        // lane-bg(loop:1000,終端1000) や keybeam(loop:100,終端100) の挙動。
        assert_eq!(resolve_loop_elapsed(1000, 500, 1000), 500); // 再生中
        assert_eq!(resolve_loop_elapsed(1000, 1000, 1000), 1000); // 終端
        assert_eq!(resolve_loop_elapsed(1000, 5000, 1000), 1000); // 終端超過 → 保持
        // loop > cycle も終端で停止する。
        assert_eq!(resolve_loop_elapsed(300, 5000, 200), 200);
    }

    #[test]
    fn loop_before_cycle_end_repeats_segment() {
        // loop < cycle: [loop, cycle) 区間を繰り返す。
        assert_eq!(resolve_loop_elapsed(0, 150, 200), 150); // 再生中はそのまま
        assert_eq!(resolve_loop_elapsed(0, 350, 200), 150); // 350 → 150 へループ
        assert_eq!(resolve_loop_elapsed(100, 350, 200), 150); // (350-100)%100+100
    }

    #[test]
    fn negative_loop_destination_disappears_after_end() {
        // loop:-1 の destination はアニメーション終端を過ぎると描画されない（READY/ボム）。
        let destination: SkinDestinationDef = serde_json::from_str(
            r#"{ "id": "ready", "loop": -1, "dst": [
                { "time": 0, "x": 0, "y": 0, "w": 10, "h": 10, "a": 0 },
                { "time": 1000, "a": 255 }
            ]}"#,
        )
        .unwrap();
        assert!(
            resolve_destination_frame(&destination, 500, &[], SkinDrawState::default()).is_some()
        );
        assert!(
            resolve_destination_frame(&destination, 1000, &[], SkinDrawState::default()).is_some()
        );
        assert!(
            resolve_destination_frame(&destination, 1001, &[], SkinDrawState::default()).is_none()
        );
    }

    #[test]
    fn destination_frame_h_expr_resolves_fast_slow_breakdown_height() {
        let destination: SkinDestinationDef = serde_json::from_str(&format!(
            r#"{{
                "id": "graph_r",
                "dst": [
                    {{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 0 }},
                    {{ "time": 1000, "h_expr": "{}(422)" }}
                ]
            }}"#,
            SKIN_EXPR_FAST_SLOW_BREAKDOWN_HEIGHT
        ))
        .unwrap();
        let state = SkinDrawState {
            fast_slow_counts: Some(crate::snapshot::FastSlowJudgeCounts {
                slow_empty_poor: 5,
                slow_poor: 10,
                ..crate::snapshot::FastSlowJudgeCounts::default()
            }),
            ..SkinDrawState::default()
        };

        let frame = resolve_destination_frame(&destination, 1000, &[], state).unwrap();

        assert_eq!(frame.h, 50);
    }

    #[test]
    fn note_lane_area_resolves_conditional_dst_for_enabled_option() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "note": {
                    "dst": [
                        {
                            "if": [920],
                            "values": [
                                {"x": 90, "y": 140, "w": 50, "h": 580},
                                {"x": 140, "y": 140, "w": 40, "h": 580},
                                {"x": 180, "y": 140, "w": 50, "h": 580},
                                {"x": 230, "y": 140, "w": 40, "h": 580},
                                {"x": 270, "y": 140, "w": 50, "h": 580},
                                {"x": 320, "y": 140, "w": 40, "h": 580},
                                {"x": 360, "y": 140, "w": 50, "h": 580},
                                {"x": 20, "y": 140, "w": 70, "h": 580}
                            ]
                        }
                    ]
                }
            }
            "#,
        )
        .unwrap();

        let enabled = vec![920];
        // Key1 is index 0
        let area = document.note_lane_area(Lane::Key1, KeyMode::K7, &enabled).unwrap();
        assert!(approx_eq(area.x, 90.0 / 1280.0));
        assert!(approx_eq(area.y, 0.0));
        assert!(approx_eq(area.width, 50.0 / 1280.0));
        assert!(approx_eq(area.height, 580.0 / 720.0));

        // Scratch is index 7
        let scratch_area = document.note_lane_area(Lane::Scratch, KeyMode::K7, &enabled).unwrap();
        assert!(approx_eq(scratch_area.x, 20.0 / 1280.0));
        assert!(approx_eq(scratch_area.width, 70.0 / 1280.0));

        // Without the required option, returns None
        assert!(document.note_lane_area(Lane::Key1, KeyMode::K7, &[]).is_none());
    }

    fn approx_eq(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.0001
    }

    #[test]
    fn text_destination_rect_for_ref_returns_normalized_first_frame() {
        let document: SkinDocument = serde_json::from_value(serde_json::json!({
            "w": 1280,
            "h": 720,
            "text": [
                { "id": "searchword", "ref": 30, "font": "f" },
                { "id": "title", "ref": 10, "font": "f" }
            ],
            "destination": [
                {
                    "id": "title",
                    "dst": [{ "x": 0, "y": 0, "w": 100, "h": 30 }]
                },
                {
                    "id": "searchword",
                    "dst": [{ "x": 640, "y": 360, "w": 320, "h": 36 }]
                }
            ]
        }))
        .unwrap();

        let rect = document.text_destination_rect_for_ref(30).unwrap();
        assert!(approx_eq(rect.0, 0.5));
        // skin y=360, h=36 → flipped: (720 - 396) / 720 = 0.45
        assert!(approx_eq(rect.1, 0.45));
        assert!(approx_eq(rect.2, 0.25));
        assert!(approx_eq(rect.3, 0.05));

        assert!(document.text_destination_rect_for_ref(999).is_none());
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        path
    }
}
