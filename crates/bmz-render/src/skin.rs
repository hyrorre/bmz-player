use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::assets::load_png_rgba;
use crate::plan::{
    Color, DrawCommand, Point, Rect, TextAlign, TextLayer, TextOutline, TextOverflow, TextShadow,
    TextStyle, TextureId, UvRect,
};
use crate::scene::{SelectRowKind, SelectRowSnapshot, SelectSnapshot};
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
    pub note: Option<SkinNoteSetDef>,
    pub gauge: Option<SkinGaugeDef>,
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
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SkinSongListDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default)]
    pub center: i32,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
    #[serde(default, rename = "constantText")]
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
}

/// beatoraja 予約 ID と衝突しない動的タイマー ID 範囲の先頭。
pub const SKIN_DYNAMIC_TIMER_BASE: i32 = 9000;
/// `SkinDrawState::dynamic_timer_ms` のスロット数。
pub const SKIN_DYNAMIC_TIMER_COUNT: usize = 64;

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
    #[serde(default, rename = "type")]
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
) -> ([Option<i32>; MAX_JUDGE_REGIONS], [Option<usize>; MAX_JUDGE_REGIONS]) {
    let mut judge_ms = [None; MAX_JUDGE_REGIONS];
    let mut judge_index = [None; MAX_JUDGE_REGIONS];
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
        judge_index[region] = judge_image_index(&judgement.text);
    }
    (judge_ms, judge_index)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SkinAnimationDef {
    pub time: Option<i32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub acc: Option<i32>,
    pub a: Option<i32>,
    pub r: Option<i32>,
    pub g: Option<i32>,
    pub b: Option<i32>,
    pub angle: Option<i32>,
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
}

impl Default for SkinContext {
    fn default() -> Self {
        Self { manifest: default_skin_manifest(), document: None, document_sources: HashMap::new() }
    }
}

impl SkinContext {
    pub fn from_manifest(manifest: SkinManifest) -> Self {
        Self { manifest, document: None, document_sources: HashMap::new() }
    }

    pub fn from_manifest_and_document(
        manifest: SkinManifest,
        document: SkinDocument,
        document_sources: impl IntoIterator<Item = SkinDocumentTexture>,
    ) -> Self {
        Self {
            manifest,
            document: Some(document),
            document_sources: document_sources
                .into_iter()
                .map(|source| (source.source_id.clone(), source))
                .collect(),
        }
    }

    pub fn manifest(&self) -> &SkinManifest {
        &self.manifest
    }

    pub fn document(&self) -> Option<&SkinDocument> {
        self.document.as_ref()
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
        let Some(document) = &self.document else {
            return Vec::new();
        };
        document.select_render_items(&self.document_sources, snapshot)
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
        // note_y=0.0 → 判定ライン（エリア下端）、note_y=1.0 → エリア上端
        let bottom_y = area.y + area.height * (1.0 - note_y);
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
        let head_center = area.y + area.height * (1.0 - head_y);
        let tail_center = area.y + area.height * (1.0 - tail_y);
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinDrawState {
    pub elapsed_ms: i32,
    pub ready_timer_ms: Option<i32>,
    pub play_timer_ms: Option<i32>,
    pub select_bar_elapsed_ms: i32,
    pub select_option_panel_elapsed_ms: i32,
    pub select_option_panel: u8,
    pub select_arrange_index: usize,
    pub select_gauge_index: usize,
    pub select_bga_index: usize,
    pub select_assist_index: usize,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub gauge: f32,
    pub gauge_type: i32,
    pub play_progress: f32,
    pub end_of_note: bool,
    /// 各レーンのボムタイマー経過ms。Noneなら非アクティブ。
    pub bomb_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのkeyon(押下中ビーム)タイマー経過ms。Noneなら非アクティブ。
    pub keyon_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンのkeyoff(離した直後の演出)タイマー経過ms。Noneなら非アクティブ。
    /// beatoraja の TIMER_KEYOFF_1P_KEY1..7 (121..127) / SCRATCH (120) に対応。
    pub keyoff_ms: [Option<i32>; LANE_COUNT],
    /// 各レーンの直近判定の画像インデックス (0=PGREAT,1=GREAT,2=GOOD,3=BAD,4=POOR)。
    /// imageset (ボム・キービーム) の画像選択に使う。Noneなら判定なし。
    pub lane_judge: [Option<usize>; LANE_COUNT],
    /// 判定タイマー経過ms。index 0/1/2 = TIMER_JUDGE_1P/2P/3P (46/47/247)。Noneなら非アクティブ。
    pub judge_ms: [Option<i32>; MAX_JUDGE_REGIONS],
    /// Full combo timer elapsed ms (TIMER_FULLCOMBO_1P/2P=48/49)。Noneなら非アクティブ。
    pub full_combo_ms: Option<i32>,
    pub music_end_ms: Option<i32>,
    /// 領域別の判定画像インデックス (0=PGREAT,1=GREAT,2=GOOD,3=BAD,4=POOR)。
    pub judge_index: [Option<usize>; MAX_JUDGE_REGIONS],
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
    /// レーンカバー割合 0.0-1.0 (NUMBER_LANECOVER1=14 に使用)。0=なし, 1=全画面。
    pub lane_cover: f32,
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
    /// 現在表示するBGA本体画像。
    pub bga_base: Option<SkinBgaFrame>,
    /// 現在表示するBGAレイヤー画像。
    pub bga_layer: Option<SkinBgaFrame>,
    /// 直近のBAD/POORで一時表示するミスレイヤー画像。
    pub bga_poor: Option<SkinBgaFrame>,
    /// BGA destination に stretch 指定が無い場合に使う拡大設定。
    pub bga_stretch: i32,
    /// 最後の判定のタイミングずれ ms (VALUE_JUDGE_1P_DURATION=525 に使用)。Noneなら非表示。
    pub judge_timing_ms: Option<i32>,
    /// 過去ベストスコアのexスコア (NUMBER_HIGHSCORE=150, BARGRAPH_BESTSCORERATE=113 に使用)。
    pub best_ex_score: Option<u32>,
    /// 過去ベストのクリアタイプ index (ref 371)。
    pub best_clear_index: Option<i64>,
    /// ターゲットスコアのexスコア (NUMBER_TARGET_SCORE=121, BARGRAPH_TARGETSCORERATE=115 に使用)。
    pub target_ex_score: Option<u32>,
    /// 判定タイミングオフセット設定値 ms (NUMBER_JUDGETIMING=12 に使用、beatoraja の judgetiming 設定)。
    pub judge_timing_offset_ms: i32,
    /// 選曲画面の表示曲数 (NUMBER_SELECT_BAR_COUNT=300 相当)。
    pub select_chart_count: u32,
    /// 選曲バーのスクロール位置 0.0-1.0。
    pub select_scroll_progress: f32,
    /// 選曲画面の master/key/bgm 音量 0.0-1.0。
    pub select_master_volume: f32,
    pub select_key_volume: f32,
    pub select_bgm_volume: f32,
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
    /// 選択中バーがフォルダかどうか。
    pub select_is_folder: bool,
    /// 選択中曲が library.db に登録済みかどうか (OPTION_PLAYABLEBAR=5)。
    pub select_in_library: bool,
    /// 選択中曲のノーツ数。
    pub select_total_notes: u32,
    /// 選択中曲の代表BPM。
    pub select_bpm: f32,
    /// 選択中曲の最小BPM。
    pub select_min_bpm: f32,
    /// 選択中曲の最大BPM。
    pub select_max_bpm: f32,
    /// 選択中曲の長さ ms。
    pub select_length_ms: i64,
    /// Fast/Slow 内訳 (ref 410-419/421-424)。
    /// Play/Result 中は Some、それ以外は None。
    pub fast_slow_counts: Option<crate::snapshot::FastSlowJudgeCounts>,
    /// 過去ベスト max combo (ref 172)。
    pub best_max_combo: Option<u32>,
    /// ターゲット max combo (ref 173, 175 で使用)。
    pub target_max_combo: Option<u32>,
    /// 過去ベスト misscount (ref 178 で使用)。
    pub best_misscount: Option<u32>,
    /// ターゲット misscount (ref 176, 178 で使用)。
    pub target_misscount: Option<u32>,
    /// ターゲットクリアタイプの index (ref 371)。
    pub target_clear_index: Option<i64>,
    /// リザルト画面でクリアしたか (op 90=CLEAR, op 91=FAIL)。
    /// Play 中は None、Result 中は Some(true)=Fail / Some(false)=Clear。
    pub result_failed: Option<bool>,
    /// シーン終了フェードアウトのタイマー経過 ms (TIMER_FADEOUT=2)。
    /// None ならフェードアウト中でない。`timer: 2` の destination はこの値が
    /// Some のときだけ描画され、リザルト画面終了時のアニメーションを駆動する。
    pub fadeout_ms: Option<i32>,
    /// 閉店/FAILED 演出のタイマー経過 ms (TIMER_FAILED=3)。
    pub failed_ms: Option<i32>,
    /// OPTION_AUTOPLAYON (33) / OPTION_AUTOPLAYOFF (32) 用。
    pub autoplay: bool,
    /// OPTION_MODE_COURSE (290) とステージ別 op (280..283 / 289) 用。未対応時は None。
    pub course_stage: Option<CourseStageMarker>,
    /// `dynamicTimer` で定義された observe タイマーの経過 ms。None は timer_off。
    pub dynamic_timer_ms: [Option<i32>; SKIN_DYNAMIC_TIMER_COUNT],
}

impl Default for SkinDrawState {
    fn default() -> Self {
        Self {
            elapsed_ms: 0,
            ready_timer_ms: None,
            play_timer_ms: None,
            select_bar_elapsed_ms: 0,
            select_option_panel_elapsed_ms: 0,
            select_option_panel: 0,
            select_arrange_index: 0,
            select_gauge_index: 2,
            select_bga_index: 0,
            select_assist_index: 0,
            combo: 0,
            max_combo: 0,
            ex_score: 0,
            total_notes: 0,
            past_notes: 0,
            judge_counts: DisplayJudgeCounts::default(),
            gauge: 0.0,
            gauge_type: 2,
            play_progress: 0.0,
            end_of_note: false,
            bomb_ms: [None; LANE_COUNT],
            keyon_ms: [None; LANE_COUNT],
            keyoff_ms: [None; LANE_COUNT],
            lane_judge: [None; LANE_COUNT],
            judge_ms: [None; MAX_JUDGE_REGIONS],
            full_combo_ms: None,
            music_end_ms: None,
            judge_index: [None; MAX_JUDGE_REGIONS],
            offset_lift_px: 0,
            offset_lanecover_px: 0,
            offset_hidden_cover_px: 0,
            skin_offsets: SkinOffsetValues::default(),
            hispeed: 0.0,
            timeleft_ms: 0,
            total_duration_ms: 0,
            lane_cover: 0.0,
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
            bga_base: None,
            bga_layer: None,
            bga_poor: None,
            bga_stretch: 1,
            judge_timing_ms: None,
            best_ex_score: None,
            best_clear_index: None,
            target_ex_score: None,
            judge_timing_offset_ms: 0,
            select_chart_count: 0,
            select_scroll_progress: 0.0,
            select_master_volume: 1.0,
            select_key_volume: 1.0,
            select_bgm_volume: 1.0,
            select_play_level: 0,
            play_level: 0,
            difficulty: 0,
            judge_rank: None,
            select_ex_score: None,
            select_replay_slots: [false; 4],
            select_replay_index: None,
            select_clear_index: 0,
            select_is_folder: false,
            select_in_library: true,
            select_total_notes: 0,
            select_bpm: 0.0,
            select_min_bpm: 0.0,
            select_max_bpm: 0.0,
            select_length_ms: 0,
            fast_slow_counts: None,
            best_max_combo: None,
            target_max_combo: None,
            best_misscount: None,
            target_misscount: None,
            target_clear_index: None,
            result_failed: None,
            fadeout_ms: None,
            failed_ms: None,
            autoplay: false,
            course_stage: None,
            dynamic_timer_ms: [None; SKIN_DYNAMIC_TIMER_COUNT],
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinTextState<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    pub subartist: &'a str,
    pub genre: &'a str,
    pub difficulty_name: &'a str,
    pub play_level: &'a str,
    pub current_folder: &'a str,
    pub bar_text: &'a str,
    pub table_level: &'a str,
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
        for destination in self.all_destinations(&enabled_options) {
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
            if self.gauge.as_ref().is_some_and(|gauge| gauge.id == destination.id) {
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
                state.combo,
                elapsed,
                sources,
                state,
            );
        }

        let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options)?;
        let is_hidden_cover_destination =
            self.hidden_cover.iter().any(|cover| cover.id == destination.id);
        apply_skin_offset_to_frame(destination, &mut frame, state, is_hidden_cover_destination);
        if let Some(image) = images.get(destination.id.as_str()) {
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
                let tint = Color::rgba(1.0, 1.0, 1.0, frame.a as f32 / 255.0);
                let stretch =
                    if destination.stretch < 0 { state.bga_stretch } else { destination.stretch };
                let mut items = Vec::new();
                if let Some(bga) = state.bga_poor {
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
                if state.bga_poor.is_none()
                    && let Some(bga) = state.bga_layer
                {
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

        if let Some(value) = self.value.iter().find(|value| value.id == destination.id) {
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
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options)?;
        frame.x += offset.0;
        frame.y += offset.1;
        apply_skin_offset_to_frame(destination, &mut frame, state, false);

        if let Some(image) = images.get(destination.id.as_str()) {
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
        let selected_row = snapshot.rows.iter().find(|row| row.index == snapshot.selected_index);
        let state = SkinDrawState {
            elapsed_ms: (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            select_bar_elapsed_ms: (snapshot.selection_time.0 / 1_000)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            select_option_panel_elapsed_ms: (snapshot.option_panel_time.0 / 1_000)
                .clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
            select_option_panel: snapshot.option_panel,
            select_arrange_index: select_arrange_index(&snapshot.arrange),
            select_gauge_index: select_gauge_index(&snapshot.gauge),
            select_bga_index: select_bga_index(&snapshot.bga),
            select_assist_index: select_assist_index(&snapshot.assist),
            select_scroll_progress: select_scroll_progress(snapshot),
            select_master_volume: snapshot.master_volume,
            select_key_volume: snapshot.key_volume,
            select_bgm_volume: snapshot.bgm_volume,
            select_chart_count: snapshot.chart_count,
            select_play_level: selected_row.map(select_row_level_number).unwrap_or(0),
            play_level: selected_row.map(select_row_level_number).unwrap_or(0),
            difficulty: selected_row.map(select_row_difficulty_code).unwrap_or(0),
            select_ex_score: selected_row.and_then(|row| row.ex_score),
            select_replay_slots: selected_row.map(|row| row.replay_slots).unwrap_or([false; 4]),
            select_replay_index: selected_row.and_then(select_row_replay_index),
            select_clear_index: selected_row.map(select_row_clear_index).unwrap_or(0) as i64,
            select_is_folder: selected_row.is_some_and(|row| row.is_folder),
            select_in_library: selected_row.is_none_or(|row| row.in_library),
            select_total_notes: selected_row.map(|row| row.total_notes).unwrap_or(0),
            select_bpm: selected_row.map(|row| row.initial_bpm).unwrap_or(0.0),
            select_min_bpm: selected_row.map(|row| row.min_bpm).unwrap_or(0.0),
            select_max_bpm: selected_row.map(|row| row.max_bpm).unwrap_or(0.0),
            select_length_ms: selected_row.map(|row| row.length_ms).unwrap_or(0),
            max_combo: selected_row.and_then(|row| row.max_combo).unwrap_or(0),
            gauge: selected_row.and_then(|row| row.gauge_value).unwrap_or(0.0),
            ex_score: selected_row.and_then(|row| row.ex_score).unwrap_or(0),
            ..SkinDrawState::default()
        };
        let text = SkinTextState {
            title: selected_row.map(|row| row.title.as_str()).unwrap_or(&snapshot.selected_title),
            artist: selected_row.map(|row| row.artist.as_str()).unwrap_or_default(),
            genre: "",
            difficulty_name: selected_row
                .map(|row| row.difficulty_name.as_str())
                .unwrap_or_default(),
            play_level: selected_row.map(|row| row.play_level.as_str()).unwrap_or_default(),
            current_folder: &snapshot.current_folder,
            table_level: selected_row.map(|row| row.table_level.as_str()).unwrap_or_default(),
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
            if !test_skin_ops(&destination.op, &enabled_options, state)
                || !eval_skin_draw_condition(&destination.draw, state)
            {
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
                select_is_folder: row.is_folder,
                select_in_library: row.in_library,
                select_total_notes: row.total_notes,
                select_length_ms: row.length_ms,
                max_combo: row.max_combo.unwrap_or(0),
                gauge: row.gauge_value.unwrap_or(0.0),
                ex_score: row.ex_score.unwrap_or(0),
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
                resolve_destination_frame(row_destination, elapsed, enabled_options)
            else {
                continue;
            };
            let row_origin = (row_frame.x, row_frame.y);
            apply_skin_offset_to_frame(row_destination, &mut row_frame, state, false);
            if let Some(item) = self.select_bar_item(row, row_destination, row_frame, sources) {
                items.push(item);
            }
            if !row.is_folder && row.in_library {
                let clear_index = select_row_clear_index(row);
                let lamp_entries = if songlist.playerlamp.is_empty() {
                    &songlist.lamp
                } else {
                    &songlist.playerlamp
                };
                items.extend(self.select_songlist_child_items_by_index(
                    lamp_entries,
                    clear_index,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
                items.extend(self.select_songlist_level_items(
                    &songlist.level,
                    row,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
                if let Some(trophy_index) = select_row_trophy_index(row) {
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
                    &songlist.graph,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
                items.extend(self.select_songlist_all_child_items(
                    &songlist.judgegraph,
                    row_origin,
                    images,
                    enabled_options,
                    row_state,
                    sources,
                ));
                items.extend(self.select_songlist_all_child_items(
                    &songlist.bpmgraph,
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
        row_origin: (i32, i32),
        images: &HashMap<&str, &SkinImageDef>,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let mut items = Vec::new();
        for destination in destination_entries(entries, enabled_options) {
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
        let bottom_y = area.y + area.height * (1.0 - note_y);
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
            let Some(mut frame) = resolve_destination_frame(destination, elapsed, &enabled_options)
            else {
                continue;
            };
            frame.y += (timeline_bottom_px - lane_bottom_px).round() as i32;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);
            apply_bar_line_offset_to_frame(&mut frame, state);
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
            items.push(self.apply_notes_offset_to_item(item, state));
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

    fn apply_notes_offset_to_item(
        &self,
        item: SkinRenderItem,
        state: SkinDrawState,
    ) -> SkinRenderItem {
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
                rect: self.apply_notes_offset_to_rect(rect, state),
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
                rect: self.apply_notes_offset_to_rect(rect, state),
                uv,
                tint,
                blend,
                source_size,
                linear_filter,
                angle_deg,
                center,
            },
            other => other,
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
                self.gauge.as_ref().is_some_and(|gauge_def| destination.id == gauge_def.id)
                    && destination.timer.is_none()
                    && test_skin_ops(&destination.op, &enabled_options, state)
                    && eval_skin_draw_condition(&destination.draw, state)
            })?;
        self.resolve_gauge_destination_items(destination, &enabled_options, state, sources)
    }

    fn resolve_gauge_destination_items(
        &self,
        destination: &SkinDestinationDef,
        enabled_options: &[i32],
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        let gauge_def = self.gauge.as_ref()?;
        let elapsed_ms = skin_timer_elapsed_ms(destination.timer, state)?;
        let frame = resolve_destination_frame(destination, elapsed_ms, enabled_options)?;
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let filled =
            (state.gauge.clamp(0.0, 100.0) / 100.0 * gauge_def.parts.max(1) as f32).round() as i32;
        let node_id = gauge_def.nodes.first()?;
        let mut items = Vec::new();
        for index in 0..filled {
            let part_rect = if rect.width >= rect.height {
                let part_width = rect.width / gauge_def.parts.max(1) as f32;
                Rect {
                    x: rect.x + part_width * index as f32,
                    y: rect.y,
                    width: part_width,
                    height: rect.height,
                }
            } else {
                let part_height = rect.height / gauge_def.parts.max(1) as f32;
                Rect {
                    x: rect.x,
                    y: rect.y + rect.height - part_height * (index + 1) as f32,
                    width: rect.width,
                    height: part_height,
                }
            };
            if let Some(item) =
                self.image_render_item(node_id, part_rect, elapsed_ms, sources, 0, false)
            {
                items.push(item);
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
        let mut image_frame =
            resolve_destination_frame_until_end(image_destination, elapsed_ms, &enabled_options)?;
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
            )
        {
            image_frame.x -=
                self.value_number_length(&number_destination.id, combo as i64, number_frame) / 2;
        }
        let mut items = vec![self.image_render_item(
            &image_destination.id,
            normalize_skin_frame_rect(image_frame, self.w, self.h),
            elapsed_ms,
            sources,
            image_destination.stretch,
            image_destination.filter != 0,
        )?];
        if combo > 0
            && let Some(number_destination) = judge.numbers.get(judge_index)
            && let Some(mut number_frame) = resolve_destination_frame_until_end(
                number_destination,
                elapsed_ms,
                &enabled_options,
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
        let zero_pad = value.zeropadding != 0 || value.padding != 0;
        let digits = if ref_id_is_signed(value.ref_id) {
            display_signed_number_digits(number, max_digits, zero_pad, value.divx.max(1) as u32)
        } else {
            display_number_digits(number, max_digits, zero_pad)
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
        let cell_width_px = value.w as f32 / divx as f32;
        let cell_height_px = value.h as f32 / divy as f32;
        if cell_width_px <= 0.0 || cell_height_px <= 0.0 {
            return Vec::new();
        }
        // `padding` は `zeropadding` の別名
        let zero_pad = value.zeropadding != 0 || value.padding != 0;
        let max_digits = value.digit.max(0) as usize;
        let digits = if ref_id_is_signed(value.ref_id) {
            display_signed_number_digits(number, max_digits, zero_pad, divx as u32)
        } else {
            display_number_digits(number, max_digits, zero_pad)
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

    fn image_render_item(
        &self,
        image_id: &str,
        rect: Rect,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        stretch: i32,
        linear_filter: bool,
    ) -> Option<SkinRenderItem> {
        let image = self.image.iter().find(|image| image.id == image_id)?;
        let source = resolve_document_source(sources, &image.src)?;
        let uv = skin_image_texture_region(image, source.source_size, elapsed_ms);
        let (rect, uv) =
            stretch_skin_image_geometry(stretch, rect, uv, source.source_size, self.w, self.h);
        Some(SkinRenderItem::Image {
            texture: source.texture,
            rect,
            uv,
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
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
        Some(SkinRenderItem::Text {
            origin: Point { x: origin_x, y: rect.y },
            text: content,
            style: TextStyle {
                font_id: (!text.font.is_empty()).then(|| text.font.clone()),
                size: frame.h.abs().max(text.size).max(1) as f32 / self.h.max(1) as f32,
                color: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
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

    fn slider_render_item(
        &self,
        slider: &SkinSliderDef,
        destination: &SkinDestinationDef,
        frame: ResolvedSkinFrame,
        state: SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let progress = skin_slider_progress(slider.slider_type, state)?;
        let source = sources.get(&slider.src)?;
        let source_width = source.source_size.width.max(1.0);
        let source_height = source.source_size.height.max(1.0);
        let mut frame = frame;
        let offset = (slider.range as f32 * progress).round() as i32;
        match slider.angle {
            0 => frame.x += offset,
            1 => frame.x -= offset,
            2 => frame.y -= offset,
            3 => frame.y += offset,
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
        42 | 43 => Some(state.select_arrange_index),
        54 | 55 => Some(0),
        72 => Some(state.select_bga_index),
        301..=307 => Some(0),
        _ => None,
    }
}

/// imageset の画像を判定インデックス (0=PGREAT..4=POOR) で選ぶ。
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
    } else if judge.starts_with("POOR") || judge.starts_with("EMPTY") {
        Some(4)
    } else {
        None
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

fn test_skin_ops(ops: &[i32], enabled_options: &[i32], state: SkinDrawState) -> bool {
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
        1 => state.select_is_folder,
        2 => !state.select_is_folder,
        3 => false,
        5 => !state.select_is_folder && state.select_in_library,
        21 => state.select_option_panel == 1,
        22 => state.select_option_panel == 2,
        23 => state.select_option_panel == 3,
        160 => !state.select_is_folder,
        196 | 197 | 198 | 1196..=1208 => select_replay_op_matches(op, state),
        200..=207 => select_rank_op_matches(op, state),
        300..=307 => select_small_rank_op_matches(op, state),
        320..=327 => best_rank_op_matches(op, state),
        170 => !state.has_bga,
        171 => state.has_bga,
        // OPTION_LANECOVER1_CHANGING / OPTION_LANECOVER1_ON / OPTION_LIFT1_ON / OPTION_HIDDEN1_ON
        270 => state.lane_cover_changing,
        271 => state.lanecover_enabled,
        272 => state.lift_enabled,
        273 => state.hidden_enabled,
        // OPTION_DIFFICULTY0..5. 0 は UNKNOWN/OTHER、1..5 は BMS #DIFFICULTY。
        150 => state.difficulty <= 0 || state.difficulty > 5,
        151..=155 => state.difficulty == i64::from(op - 150),
        // OPTION_JUDGE_VERYHARD..VERYEASY (180..184)
        180..=184 => judge_rank_option_matches(op, state.judge_rank),
        // OPTION_RESULT_CLEAR=90, OPTION_RESULT_FAIL=91
        // Result 画面以外 (result_failed == None) では両方 false。
        90 => state.result_failed == Some(false),
        91 => state.result_failed == Some(true),
        // OPTION_AUTOPLAYOFF / OPTION_AUTOPLAYON
        32 => !state.autoplay,
        33 => state.autoplay,
        // OPTION_COURSE_STAGE1..4 / OPTION_COURSE_STAGE_FINAL
        280 => state.course_stage == Some(CourseStageMarker::Stage1),
        281 => state.course_stage == Some(CourseStageMarker::Stage2),
        282 => state.course_stage == Some(CourseStageMarker::Stage3),
        283 => state.course_stage == Some(CourseStageMarker::Stage4),
        289 => state.course_stage == Some(CourseStageMarker::Final),
        // OPTION_MODE_COURSE
        290 => state.course_stage.is_some(),
        value => test_json_option_number(value, enabled_options),
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

fn display_number_digits(value: i64, max_digits: usize, zero_pad: bool) -> Vec<u8> {
    let mut text = if zero_pad && max_digits > 0 {
        format!("{:0width$}", value.max(0), width = max_digits)
    } else {
        value.max(0).to_string()
    };
    if max_digits > 0 && text.len() > max_digits {
        text = text[text.len() - max_digits..].to_string();
    }
    text.bytes().filter(|byte| byte.is_ascii_digit()).map(|byte| byte - b'0').collect()
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
    if let Some(ref_id) = parse_skin_number_operand(operand) {
        return skin_state_number(ref_id, state).map(|value| value as f32);
    }
    if let Some(timer_id) = parse_skin_timer_operand(operand) {
        return Some(skin_timer_elapsed_ms(Some(timer_id), state).unwrap_or(i32::MIN) as f32);
    }
    match operand {
        "gauge()" | "gauge" => Some(state.gauge),
        "gauge_type()" | "gauge_type" => Some(state.gauge_type as f32),
        "timer_off" | "timer_off_value" => Some(i32::MIN as f32),
        value => value.parse::<f32>().ok(),
    }
}

fn parse_skin_number_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("number(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn parse_skin_option_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("option(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn parse_skin_timer_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("timer(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

fn skin_value_number(value: &SkinValueDef, state: SkinDrawState) -> Option<i64> {
    if !value.expr.trim().is_empty() {
        return skin_state_number_expr(&value.expr, state);
    }
    skin_state_number(value.ref_id, state)
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
    if let Some(ref_id) = parse_skin_number_operand(term) {
        return skin_state_number(ref_id, state).map(|value| value as f32);
    }
    if let Some((coefficient, operand)) = term.split_once('*') {
        let coefficient = coefficient.parse::<f32>().ok()?;
        let ref_id = parse_skin_number_operand(operand.trim())?;
        return skin_state_number(ref_id, state).map(|value| coefficient * value as f32);
    }
    term.parse::<f32>().ok()
}

fn skin_state_number(ref_id: i32, state: SkinDrawState) -> Option<i64> {
    match ref_id {
        // Lua draw 畳み込みのプレースホルダ (`number(0) >= 0` 等)
        0 => Some(0),
        300 => Some(state.select_chart_count as i64),
        96 => Some(if state.play_level != 0 { state.play_level } else { state.select_play_level }),
        370 => Some(state.select_clear_index),
        92 => Some(state.select_bpm.round() as i64),
        71 | 101 | 171 => Some(state.ex_score as i64),
        72 => Some(state.total_notes as i64 * 2),
        74 | 106 | 333 => Some(state.total_notes.max(state.select_total_notes) as i64),
        350 => Some(state.select_total_notes as i64),
        75 | 105 | 174 => Some(state.max_combo as i64),
        76 => Some((state.judge_counts.bad + state.judge_counts.poor) as i64),
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
        313 => Some((state.total_duration_ms as i64) * 3 / 5),
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
        // レーンカバー: NUMBER_LANECOVER1=14 (0-100%)
        14 => Some((state.lane_cover.clamp(0.0, 1.0) * 100.0).round() as i64),
        // 選曲画面の音量表示: MASTER/BGM/KEY volume (0-100)
        57 => Some((state.select_master_volume.clamp(0.0, 1.0) * 100.0).round() as i64),
        58 => Some((state.select_bgm_volume.clamp(0.0, 1.0) * 100.0).round() as i64),
        59 => Some((state.select_key_volume.clamp(0.0, 1.0) * 100.0).round() as i64),
        // 判定タイミングずれ: VALUE_JUDGE_1P_DURATION=525 (ms、絶対値)
        525 => state.judge_timing_ms.map(|ms| ms.unsigned_abs() as i64),
        // 判定タイミングオフセット設定値 (NUMBER_JUDGETIMING=12)
        12 => Some(state.judge_timing_offset_ms as i64),
        // ベストスコア / ターゲットスコア (DB から供給、未取得時は None)
        150 | 170 => state.best_ex_score.map(|s| s as i64),
        121 => state.target_ex_score.map(|s| s as i64),
        122 | 123 => state
            .target_ex_score
            .map(|target| score_rate_parts(target, state.total_notes))
            .map(|parts| if ref_id == 122 { parts.0 } else { parts.1 } as i64),
        183 | 184 => state
            .best_ex_score
            .map(|best| score_rate_parts(best, state.total_notes))
            .map(|parts| if ref_id == 183 { parts.0 } else { parts.1 } as i64),
        400 => state.judge_rank.map(|rank| rank as i64),
        154 => next_rank_diff(state),
        // NUMBER_DIFF_HIGHSCORE=152, NUMBER_DIFF_HIGHSCORE2=172 (符号付き、ex_score - best)
        152 | 172 => state.best_ex_score.map(|best| state.ex_score as i64 - best as i64),
        // NUMBER_DIFF_TARGETSCORE=153 (符号付き、ex_score - target)
        153 => state.target_ex_score.map(|target| state.ex_score as i64 - target as i64),
        // NUMBER_TARGET_MAXCOMBO=173
        173 => state.target_max_combo.map(|c| c as i64),
        // NUMBER_DIFF_MAXCOMBO=175 (符号付き、max_combo - target_max_combo)
        175 => state.target_max_combo.map(|target| state.max_combo as i64 - target as i64),
        // NUMBER_TARGET_MISSCOUNT=176
        176 => state.target_misscount.map(|c| c as i64),
        // NUMBER_DIFF_MISSCOUNT=178 (符号付き、現在 misscount - target_misscount)
        178 => state.target_misscount.map(|target| {
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

fn div_ceil(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        return 0;
    }
    numerator.div_euclid(denominator) + i64::from(numerator.rem_euclid(denominator) != 0)
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
    (total_notes > 0).then_some(count as i64 * 100 / total_notes as i64)
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

/// Starseeker 閉店の `src = 0` は `system` の黒 1px (`black` image と同じ UV) を指す。
fn skin_image_pixel_rect(
    image: &SkinImageDef,
    images: &HashMap<&str, &SkinImageDef>,
) -> (i32, i32, i32, i32) {
    if image.src == "0" {
        if let Some(black) = images.get("black") {
            return (black.x, black.y, black.w, black.h);
        }
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
    let (px, py, pw, ph) = pixel_rect;
    let source_width = source_size.width.max(1.0);
    let source_height = source_size.height.max(1.0);
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

fn gauge_after_dot(gauge: f32) -> u32 {
    if gauge > 0.0 && gauge < 0.1 { 1 } else { ((gauge.max(0.0) * 10.0) as u32) % 10 }
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
        // BARGRAPH_BESTSCORERATE_NOW (112): best_ex_score * past_notes / (total_notes^2 * 2)
        112 => {
            let max = (state.total_notes as f64).powi(2) * 2.0;
            if max > 0.0 {
                (state.best_ex_score.unwrap_or(0) as f64 * state.past_notes as f64 / max) as f32
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

fn judge_rate(count: u32, total: u32) -> f32 {
    if total > 0 { count as f32 / total as f32 } else { 0.0 }
}

fn skin_slider_progress(slider_type: i32, state: SkinDrawState) -> Option<f32> {
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
        Some(40) => state.ready_timer_ms,
        Some(41) => state.play_timer_ms,
        Some(11) => Some(state.select_bar_elapsed_ms),
        Some(21..=23) => Some(state.select_option_panel_elapsed_ms),
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
        Some(143) => state.end_of_note.then_some(state.elapsed_ms),
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

fn skin_state_text(text: &SkinTextDef, state: SkinTextState<'_>) -> String {
    if !text.constant_text.is_empty() {
        return text.constant_text.clone();
    }
    if text.id.starts_with("bartext") {
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
        10 => state.title.to_string(),
        11 => state.subtitle.to_string(),
        12 => full_label(state.title, state.subtitle),
        13 => state.genre.to_string(),
        14 => state.artist.to_string(),
        15 => state.subartist.to_string(),
        16 => full_label(state.artist, state.subartist),
        17 => state.table_level.to_string(),
        1000 => state.current_folder.to_string(),
        _ => String::new(),
    }
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

fn select_row_bar_image_index(row: &SelectRowSnapshot) -> usize {
    match row.kind {
        SelectRowKind::Song if !row.in_library => 4,
        SelectRowKind::Song => 0,
        SelectRowKind::Folder => 1,
        SelectRowKind::TableFolder => 2,
    }
}

fn select_row_bar_text_index(row: &SelectRowSnapshot) -> usize {
    match row.kind {
        SelectRowKind::Song if !row.in_library => 8,
        SelectRowKind::Song => 2,
        SelectRowKind::Folder => 4,
        SelectRowKind::TableFolder => 6,
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

fn select_replay_op_matches(op: i32, state: SkinDrawState) -> bool {
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

fn select_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
    let Some(rank) = current_rank_index(state) else {
        return false;
    };
    op == 200 + rank as i32
}

fn select_small_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
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

fn best_rank_op_matches(op: i32, state: SkinDrawState) -> bool {
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
    } else {
        (state.select_ex_score, state.select_total_notes)
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

fn select_arrange_index(arrange: &str) -> usize {
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

fn apply_bar_line_offset_to_frame(frame: &mut ResolvedSkinFrame, state: SkinDrawState) {
    if let Some(offset) = state.skin_offsets.get(SKIN_OFFSET_BAR_LINE) {
        frame.h = (frame.h + offset.h).max(0);
        frame.a = (frame.a + offset.a).clamp(0, 255);
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
        apply_skin_animation(&mut frame, animation);
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
) -> Option<ResolvedSkinFrame> {
    if matches!(destination.loop_time, Some(loop_point) if loop_point > 0) {
        return resolve_destination_frame(destination, elapsed_ms, enabled_options);
    }
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    let last_time = animations.iter().filter_map(|a| a.time).max()?;
    if elapsed_ms > last_time {
        return None;
    }
    resolve_destination_frame(destination, elapsed_ms, enabled_options)
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
        apply_skin_animation(&mut frame, animation);
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

fn apply_skin_animation(frame: &mut ResolvedSkinFrame, animation: &SkinAnimationDef) {
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
    destination.offset == offset_id || destination.offsets.iter().any(|&id| id == offset_id)
}

fn destination_uses_lift_offset_only(destination: &SkinDestinationDef) -> bool {
    destination_uses_skin_offset(destination, 3)
        && !destination_uses_skin_offset(destination, 4)
        && !destination_uses_skin_offset(destination, 5)
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
    use crate::plan::TextLayer;

    use super::*;

    fn judge_region_state(
        region: usize,
        ms: i32,
        image_index: usize,
    ) -> ([Option<i32>; MAX_JUDGE_REGIONS], [Option<usize>; MAX_JUDGE_REGIONS]) {
        let mut judge_ms = [None; MAX_JUDGE_REGIONS];
        let mut judge_index = [None; MAX_JUDGE_REGIONS];
        if region < MAX_JUDGE_REGIONS {
            judge_ms[region] = Some(ms);
            judge_index[region] = Some(image_index);
        }
        (judge_ms, judge_index)
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
            select_ex_score: Some(1556),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };
        let max_state = SkinDrawState {
            select_ex_score: Some(2000),
            select_total_notes: 1000,
            ..SkinDrawState::default()
        };
        let f_state = SkinDrawState {
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
                }),
                bga_layer: Some(SkinBgaFrame {
                    texture: SkinTextureId(20001),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
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
                }),
                bga_layer: Some(SkinBgaFrame {
                    texture: SkinTextureId(20001),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
                }),
                bga_poor: Some(SkinBgaFrame {
                    texture: SkinTextureId(20002),
                    source_size: SkinImageSize { width: 256.0, height: 256.0 },
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
            SkinDrawState { judge_ms: judge_region_state(0, 120, 0).0, ..Default::default() }
        ));
        assert!(eval_skin_draw_condition(
            "timer(46) > 0 and option(197)",
            SkinDrawState {
                judge_ms: judge_region_state(0, 120, 0).0,
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
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 4 },
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

        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, width: uv_width, .. },
                ..
            } if approx_eq(x, 0.4)
                && approx_eq(y, 0.8)
                && approx_eq(width, 0.1)
                && approx_eq(height, 0.1)
                && approx_eq(u, 0.1)
                && approx_eq(uv_width, 0.05)));
        assert!(matches!(items[1], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(x, 0.5)));
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
                "gauge": { "id": "gauge", "nodes": ["gauge-node"], "parts": 4 },
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
            SkinDrawState { elapsed_ms: 500, gauge: 50.0, fadeout_ms: None, ..Default::default() },
        );
        let active = document.static_image_render_items(
            &sources,
            SkinDrawState {
                elapsed_ms: 500,
                gauge: 50.0,
                fadeout_ms: Some(250),
                ..Default::default()
            },
        );

        assert_eq!(inactive.len(), 1);
        assert_eq!(active.len(), 3);
        assert!(matches!(active[1], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(x, 0.4)));
        assert!(matches!(active[2], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(x, 0.5)));
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
                combo: 123,
                judge_ms: judge_region_state(0, 100, 0).0,
                judge_index: judge_region_state(0, 100, 0).1,
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
                combo: 123,
                judge_ms: judge_region_state(0, 100, 0).0,
                judge_index: judge_region_state(0, 100, 0).1,
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
                    { "id": "lamp-normal", "src": 3, "x": 20, "y": 0, "w": 4, "h": 4 }
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
                "graph": [{ "id": "graph-lamp", "src": 4, "x": 0, "y": 0, "w": 40, "h": 4, "angle": 0, "type": -1 }],
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
                    "graph": { "id": "graph-lamp", "dst": [{ "x": 5, "y": 1, "w": 20, "h": 2 }] },
                    "playerlamp": [
                        { "id": "lamp-none", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-failed", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-light-assist", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-easy", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] },
                        { "id": "lamp-normal", "dst": [{ "x": 1, "y": 1, "w": 4, "h": 4 }] }
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
        sources.extend(mock_source("4", 40.0, 4.0));
        let snapshot = SelectSnapshot {
            selected_index: 2,
            rows: vec![
                SelectRowSnapshot {
                    index: 1,
                    title: "Folder".to_string(),
                    play_level: "0".to_string(),
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
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(*x, 0.47)
                && approx_eq(*y, 0.45)
                && approx_eq(*u, 8.0 / 24.0))));
        assert!(items.iter().any(|item| matches!(item, SkinRenderItem::Image {
                texture: SkinTextureId(9999),
                rect: Rect { x, width, .. },
                uv: TextureRegion { width: u_width, .. },
                ..
            } if approx_eq(*x, 0.17)
                && approx_eq(*width, 0.1)
                && approx_eq(*u_width, 0.5))));
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
                    { "id": "constant", "size": 5, "constantText": "READY" }
                ],
                "destination": [
                    { "id": "title", "dst": [{ "x": 10, "y": 20, "w": 50, "h": 10, "r": 128, "g": 200, "b": 255 }] },
                    { "id": "genre", "dst": [{ "x": 10, "y": 40, "w": 40, "h": 6 }] },
                    { "id": "constant", "dst": [{ "x": 10, "y": 60, "h": 5, "a": 128 }] }
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

        assert_eq!(items.len(), 3);
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
                if approx_eq(x, 0.0) && approx_eq(y, 0.14)
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
            SkinDrawState { end_of_note: true, ..SkinDrawState::default() },
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
                    { "id": "hidden-cover", "src": 12, "x": 10, "y": 20, "w": 30, "h": 40, "disapearLine": 140 }
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
        assert_eq!(document.hidden_cover[0].disappear_line, 140);
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
            best_misscount: Some(20),
            target_misscount: Some(0),
            target_clear_index: Some(8),
            ..SkinDrawState::default()
        };

        // 符号付き差分
        assert_eq!(skin_state_number(170, state), Some(1700));
        assert_eq!(skin_state_number(183, state), Some(85));
        assert_eq!(skin_state_number(184, state), Some(0));
        assert_eq!(skin_state_number(152, state), Some(1888 - 1700));
        assert_eq!(skin_state_number(172, state), Some(1888 - 1700));
        assert_eq!(skin_state_number(153, state), Some(1888 - 1900));
        assert_eq!(skin_state_number(173, state), Some(1000));
        assert_eq!(skin_state_number(175, state), Some(777 - 1000));
        assert_eq!(skin_state_number(176, state), Some(0));
        // 現在 misscount = bad+poor = 8、target = 0 → diff = 8
        assert_eq!(skin_state_number(178, state), Some(8));
        assert_eq!(skin_state_number(371, state), Some(6));
        assert!(test_skin_op(321, &[], state));
        assert!(!test_skin_op(320, &[], state));

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

        // best/target が None のとき None を返す
        let bare = SkinDrawState::default();
        assert_eq!(skin_state_number(152, bare), None);
        assert_eq!(skin_state_number(173, bare), None);
        assert_eq!(skin_state_number(410, bare), None);
    }

    #[test]
    fn skin_state_number_maps_select_refs() {
        let state = SkinDrawState {
            select_chart_count: 42,
            select_play_level: 12,
            select_clear_index: 5,
            select_total_notes: 1200,
            select_bpm: 148.0,
            select_min_bpm: 120.0,
            select_max_bpm: 180.0,
            select_length_ms: 183_000,
            select_master_volume: 0.57,
            select_bgm_volume: 0.58,
            select_key_volume: 0.59,
            ex_score: 1234,
            ..SkinDrawState::default()
        };

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
        assert_eq!(skin_state_number(92, state), Some(148));
        assert_eq!(skin_state_number(160, state), Some(148));
        assert_eq!(skin_state_number(350, state), Some(1200));
        assert_eq!(skin_state_number(71, state), Some(1234));
        assert_eq!(skin_state_number(1163, state), Some(3));
        assert_eq!(skin_state_number(1164, state), Some(3));
        assert_eq!(skin_state_number(57, state), Some(57));
        assert_eq!(skin_state_number(58, state), Some(58));
        assert_eq!(skin_state_number(59, state), Some(59));
    }

    #[test]
    fn skin_state_imageset_index_maps_select_options() {
        let state = SkinDrawState {
            select_arrange_index: 2,
            select_gauge_index: 4,
            select_bga_index: 1,
            ..SkinDrawState::default()
        };

        assert_eq!(skin_state_imageset_index(42, state), Some(2));
        assert_eq!(skin_state_imageset_index(43, state), Some(2));
        assert_eq!(skin_state_imageset_index(40, state), Some(4));
        assert_eq!(skin_state_imageset_index(72, state), Some(1));
        assert_eq!(skin_state_imageset_index(301, state), Some(0));
        assert_eq!(skin_state_imageset_index(500, state), None);
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
        let state_early =
            SkinDrawState { judge_ms: judge_region_state(0, 100, 0).0, ..SkinDrawState::default() };
        let items_early = document.static_image_render_items(&sources, state_early);
        assert_eq!(items_early.len(), 1);
        assert!(
            matches!(items_early[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.25)),
            "at judge_ms=100, x should interpolate to 0.25 (halfway between 0 and 0.5)"
        );

        // judge_ms=Some(300) → past last frame → last frame x=0.5
        let state_late =
            SkinDrawState { judge_ms: judge_region_state(0, 300, 0).0, ..SkinDrawState::default() };
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
        // NUMBER_LANECOVER1 (14) = round(0.25 * 100) = 25
        assert_eq!(skin_state_number(14, state), Some(25));
        // NUMBER_DURATION (312) = current note display duration in ms.
        assert_eq!(skin_state_number(312, state), Some(183_000));
        // NUMBER_DURATION_GREEN (313) = duration * 3 / 5.
        assert_eq!(skin_state_number(313, state), Some(109_800));
        // VALUE_JUDGE_1P_DURATION (525) = abs(-3) = 3
        assert_eq!(skin_state_number(525, state), Some(3));
        // When no recent judgement, 525 returns None
        let no_judge = SkinDrawState { judge_timing_ms: None, ..state };
        assert_eq!(skin_state_number(525, no_judge), None);
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
        // Unknown ref → empty
        assert_eq!(skin_state_text(&make_text(99), state), "");
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
        assert!(resolve_destination_frame(&destination, 500, &[]).is_some());
        assert!(resolve_destination_frame(&destination, 1000, &[]).is_some());
        assert!(resolve_destination_frame(&destination, 1001, &[]).is_none());
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
