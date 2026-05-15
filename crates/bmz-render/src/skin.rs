use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use bmz_core::lane::Lane;
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::assets::load_png_rgba;
use crate::plan::{
    Color, DrawCommand, Point, Rect, TextAlign, TextLayer, TextOutline, TextOverflow, TextShadow,
    TextStyle, TextureId, UvRect,
};
use crate::snapshot::DisplayJudgeCounts;

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
    #[serde(default)]
    pub destination: Vec<SkinDestinationDef>,
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
    pub dst: Vec<SkinAnimationDef>,
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
    #[serde(default, rename = "loop")]
    pub loop_time: i32,
    #[serde(default)]
    pub center: i32,
    #[serde(default)]
    pub offset: i32,
    #[serde(default)]
    pub offsets: Vec<i32>,
    #[serde(default = "default_stretch")]
    pub stretch: i32,
    #[serde(default)]
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

    pub fn document_note_item(&self, lane: Lane, rect: Rect) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_image_render_item(lane, rect, &self.document_sources)
    }

    pub fn document_gauge_items(&self, gauge: f32, elapsed_ms: i32) -> Option<Vec<SkinRenderItem>> {
        let document = self.document.as_ref()?;
        document.gauge_render_items(gauge, elapsed_ms, &self.document_sources)
    }

    pub fn document_judge_items(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
    ) -> Option<Vec<SkinRenderItem>> {
        let document = self.document.as_ref()?;
        document.judge_render_items(judge, combo, elapsed_ms, &self.document_sources)
    }

    pub fn document_lane_effect_items(
        &self,
        lane: Lane,
        judge: &str,
        elapsed_ms: i32,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        document.imageset_render_items_for_lane(lane, judge, elapsed_ms, &self.document_sources)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinDocumentTexture {
    pub source_id: String,
    pub texture: SkinTextureId,
    pub source_size: SkinImageSize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkinDrawState {
    pub elapsed_ms: i32,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub gauge: f32,
    pub play_progress: f32,
    pub end_of_note: bool,
    /// 各レーン(Scratch=0, Key1=1..Key7=7)のボムタイマー経過ms。Noneなら非アクティブ。
    pub bomb_ms: [Option<i32>; 8],
    /// 各レーンのkeyon(押下中ビーム)タイマー経過ms。Noneなら非アクティブ。
    pub keyon_ms: [Option<i32>; 8],
    /// 判定タイマー経過ms (TIMER_JUDGE_1P=46)。Noneなら非アクティブ。
    pub judge_ms: Option<i32>,
    /// OFFSET_LIFT (id=3) の y 値 (skin canvas pixel 単位)。リフト量に応じて要素をシフトする。
    pub offset_lift_px: i32,
    /// OFFSET_LANECOVER (id=4) の y 値 (skin canvas pixel 単位)。レーンカバー位置インジケータのシフト。
    pub offset_lanecover_px: i32,
    /// 現在のハイスピード倍率 (NUMBER_HISPEED=310, NUMBER_HISPEED_AFTERDOT=311 に使用)。
    pub hispeed: f32,
    /// 曲残り時間 ms (NUMBER_TIMELEFT_MINUTE=163, NUMBER_TIMELEFT_SECOND=164 に使用)。
    pub timeleft_ms: i32,
    /// 曲の合計長さ ms (NUMBER_DURATION=312 に使用)。
    pub total_duration_ms: i32,
    /// レーンカバー割合 0.0-1.0 (NUMBER_LANECOVER1=14 に使用)。0=なし, 1=全画面。
    pub lane_cover: f32,
    /// 現在 BPM (NUMBER_NOWBPM=160 に使用)。
    pub now_bpm: f32,
    /// 最小 BPM (NUMBER_MINBPM=91 に使用)。
    pub min_bpm: f32,
    /// 最大 BPM (NUMBER_MAXBPM=90 に使用)。
    pub max_bpm: f32,
    /// 最後の判定のタイミングずれ ms (VALUE_JUDGE_1P_DURATION=525 に使用)。Noneなら非表示。
    pub judge_timing_ms: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinTextState<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    pub subartist: &'a str,
    pub genre: &'a str,
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
            Lane::Scratch => self.scratch_texture.unwrap_or(self.texture),
            Lane::Key2 | Lane::Key4 | Lane::Key6 => self.key_even_texture.unwrap_or(self.texture),
            Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => self.texture,
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
        serde_json::from_value(expanded)
            .with_context(|| format!("failed to parse skin json: {}", path.display()))
    }

    pub fn source_map(&self) -> HashMap<&str, &SkinSourceDef> {
        self.source.iter().map(|source| (source.id.as_str(), source)).collect()
    }

    pub fn image_map(&self) -> HashMap<&str, &SkinImageDef> {
        self.image.iter().map(|image| (image.id.as_str(), image)).collect()
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
        let images = self.image_map();
        let enabled_options = self.enabled_options();
        self.destination
            .iter()
            .filter(|destination| test_skin_ops(&destination.op, &enabled_options))
            .filter(|destination| eval_skin_draw_condition(&destination.draw, state))
            .filter_map(|destination| {
                let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
                let mut frame = resolve_destination_frame(destination, elapsed, &enabled_options)?;
                let (dx, dy) = skin_offset_shift(destination, state);
                frame.x += dx;
                frame.y += dy;
                if let Some(image) = images.get(destination.id.as_str()) {
                    let source = sources.get(&image.src)?;
                    let (rect, uv) = stretch_skin_image_geometry(
                        destination.stretch,
                        normalize_skin_frame_rect(frame, self.w, self.h),
                        skin_image_texture_region(image, source.source_size, elapsed),
                        source.source_size,
                        self.w,
                        self.h,
                    );
                    return Some(vec![SkinRenderItem::Image {
                        texture: source.texture,
                        rect,
                        uv,
                        tint: Color::rgba(
                            frame.r as f32 / 255.0,
                            frame.g as f32 / 255.0,
                            frame.b as f32 / 255.0,
                            frame.a as f32 / 255.0,
                        ),
                        blend: if destination.blend == 2 {
                            BlendMode::Add
                        } else {
                            BlendMode::Normal
                        },
                        scale: SkinImageScale::Stretch,
                        border: None,
                        source_size: Some(source.source_size),
                        linear_filter: destination.filter != 0,
                    }]);
                }

                if let Some(value) = self.value.iter().find(|value| value.id == destination.id) {
                    let number = skin_state_number(value.ref_id, state)?;
                    return Some(self.value_number_render_items(
                        &value.id,
                        number,
                        ResolvedSkinFrame::default(),
                        frame,
                        sources,
                    ));
                }

                if let Some(text) = self.text.iter().find(|text| text.id == destination.id)
                    && let Some(item) = self.text_render_item(text, frame, text_state)
                {
                    return Some(vec![item]);
                }

                if let Some(slider) = self.slider.iter().find(|slider| slider.id == destination.id)
                    && let Some(item) =
                        self.slider_render_item(slider, destination, frame, state, sources)
                {
                    return Some(vec![item]);
                }

                if let Some(graph) = self.graph.iter().find(|g| g.id == destination.id) {
                    return self
                        .graph_render_item(graph, frame, state, sources)
                        .map(|item| vec![item]);
                }

                let hidden_cover =
                    self.hidden_cover.iter().find(|cover| cover.id == destination.id)?;
                self.hidden_cover_render_item(hidden_cover, destination, frame, sources)
                    .map(|item| vec![item])
            })
            .flatten()
            .collect()
    }

    pub fn enabled_options(&self) -> Vec<i32> {
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

    pub fn note_image_render_item(
        &self,
        lane: Lane,
        rect: Rect,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let note = self.note.as_ref()?;
        let image_id = note.note.get(beatoraja_7k_note_index(lane))?;
        let image = self.image.iter().find(|image| image.id == *image_id)?;
        let source = sources.get(&image.src)?;
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

    pub fn gauge_render_items(
        &self,
        gauge: f32,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<Vec<SkinRenderItem>> {
        let gauge_def = self.gauge.as_ref()?;
        let enabled_options = self.enabled_options();
        let destination = self.destination.iter().find(|destination| {
            destination.id == gauge_def.id
                && destination.timer.is_none()
                && test_skin_ops(&destination.op, &enabled_options)
                && eval_skin_draw_condition(
                    &destination.draw,
                    SkinDrawState { elapsed_ms, gauge, ..SkinDrawState::default() },
                )
        })?;
        let frame = resolve_destination_frame(destination, elapsed_ms, &enabled_options)?;
        let rect = normalize_skin_frame_rect(frame, self.w, self.h);
        let filled =
            (gauge.clamp(0.0, 100.0) / 100.0 * gauge_def.parts.max(1) as f32).round() as i32;
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
        let judge_index = judge_image_index(judge)?;
        let judge = self.judge.first()?;
        let image_destination = judge.images.get(judge_index)?;
        let enabled_options = self.enabled_options();
        let image_frame =
            resolve_destination_frame_until_end(image_destination, elapsed_ms, &enabled_options)?;
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
            && let Some(number_frame) = resolve_destination_frame_until_end(
                number_destination,
                elapsed_ms,
                &enabled_options,
            )
        {
            items.extend(self.value_number_render_items(
                &number_destination.id,
                combo as i64,
                image_frame,
                number_frame,
                sources,
            ));
        }
        Some(items)
    }

    pub fn judge_image_render_item(
        &self,
        judge: &str,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        self.judge_render_items(judge, 0, elapsed_ms, sources)?.into_iter().next()
    }

    pub fn imageset_render_items_for_lane(
        &self,
        lane: Lane,
        judge: &str,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let enabled_options = self.enabled_options();
        self.imageset
            .iter()
            .filter(|imageset| imageset_ref_lane(imageset.ref_id) == Some(lane))
            .filter_map(|imageset| {
                let destination = self.destination.iter().find(|destination| {
                    destination.id == imageset.id
                        && destination.timer.is_some()
                        && test_skin_ops(&destination.op, &enabled_options)
                })?;
                let frame =
                    resolve_destination_frame_until_end(destination, elapsed_ms, &enabled_options)?;
                let image_id = imageset_judge_image_id(imageset, judge)?;
                self.image_render_item(
                    &image_id,
                    normalize_skin_frame_rect(frame, self.w, self.h),
                    elapsed_ms,
                    sources,
                    destination.stretch,
                    destination.filter != 0,
                )
            })
            .collect()
    }

    fn value_number_render_items(
        &self,
        value_id: &str,
        number: i64,
        base_frame: ResolvedSkinFrame,
        frame: ResolvedSkinFrame,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Vec<SkinRenderItem> {
        let Some(value) = self.value.iter().find(|value| value.id == value_id) else {
            return Vec::new();
        };
        let Some(source) = sources.get(&value.src) else {
            return Vec::new();
        };
        let source_width = source.source_size.width.max(1.0);
        let source_height = source.source_size.height.max(1.0);
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
        let digits = display_number_digits(number, max_digits, zero_pad);
        let digit_width = frame.w as f32 / self.w.max(1) as f32;
        let digit_height = frame.h as f32 / self.h.max(1) as f32;
        // 桁間スペース (space フィールド、px 単位)
        let space_norm = value.space as f32 / self.w.max(1) as f32;
        let digit_step = digit_width + space_norm;
        let origin_x = (base_frame.x + frame.x) as f32 / self.w.max(1) as f32;
        let origin_y = (base_frame.y + frame.y) as f32 / self.h.max(1) as f32;
        // 先頭の空き桁数 (align のためのオフセット計算に使用)
        let shiftbase = max_digits.saturating_sub(digits.len());
        // align=0: 右寄せ (デフォルト), align=1: 左寄せ, align=2: 中央
        let shift = match value.align {
            1 => digit_step * shiftbase as f32,
            2 => digit_step * shiftbase as f32 * 0.5,
            _ => 0.0,
        };

        digits
            .into_iter()
            .enumerate()
            .map(|(index, digit)| {
                let source_column = digit as i32 % divx;
                let source_row = digit as i32 / divx;
                SkinRenderItem::Image {
                    texture: source.texture,
                    rect: Rect {
                        x: origin_x + digit_step * (shiftbase + index) as f32 - shift,
                        y: origin_y,
                        width: digit_width,
                        height: digit_height,
                    },
                    uv: TextureRegion {
                        x: (value.x as f32 + cell_width_px * source_column as f32) / source_width,
                        y: (value.y as f32 + cell_height_px * source_row as f32) / source_height,
                        width: cell_width_px / source_width,
                        height: cell_height_px / source_height,
                    },
                    tint: Color::rgb(1.0, 1.0, 1.0),
                    blend: BlendMode::Normal,
                    scale: SkinImageScale::Stretch,
                    border: None,
                    source_size: Some(source.source_size),
                    linear_filter: false,
                }
            })
            .collect()
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
        let source = sources.get(&image.src)?;
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
        Some(SkinRenderItem::Text {
            origin: Point {
                x: frame.x as f32 / self.w.max(1) as f32,
                y: frame.y as f32 / self.h.max(1) as f32,
            },
            text: content,
            style: TextStyle {
                font_id: (!text.font.is_empty()).then(|| text.font.clone()),
                size: frame.h.max(text.size).max(1) as f32 / self.h.max(1) as f32,
                color: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
                ),
                layer: TextLayer::Ui,
                align: skin_text_align(text.align),
                max_width: frame.w.max(0) as f32 / self.w.max(1) as f32,
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
        let uv = TextureRegion {
            x: slider.x as f32 / source_width,
            y: slider.y as f32 / source_height,
            width: slider.w as f32 / source_width,
            height: slider.h as f32 / source_height,
        };
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
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        let source = sources.get(&cover.src)?;
        let source_width = source.source_size.width.max(1.0);
        let source_height = source.source_size.height.max(1.0);
        let uv = TextureRegion {
            x: cover.x as f32 / source_width,
            y: cover.y as f32 / source_height,
            width: cover.w as f32 / source_width,
            height: cover.h as f32 / source_height,
        };
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
        let value = graph_value(graph.graph_type, state).clamp(0.0, 1.0);
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

fn beatoraja_7k_note_index(lane: Lane) -> usize {
    match lane {
        Lane::Key1 => 0,
        Lane::Key2 => 1,
        Lane::Key3 => 2,
        Lane::Key4 => 3,
        Lane::Key5 => 4,
        Lane::Key6 => 5,
        Lane::Key7 => 6,
        Lane::Scratch => 7,
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

fn imageset_judge_image_id(imageset: &SkinImageSetDef, judge: &str) -> Option<String> {
    let len = imageset.images.len();
    if len == 0 {
        return None;
    }
    let index = if len == 2 {
        usize::from(judge.trim().starts_with("PGREAT"))
    } else {
        judge_image_index(judge).unwrap_or(0).min(len - 1)
    };
    imageset.images.get(index).cloned()
}

fn judge_image_index(judge: &str) -> Option<usize> {
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

fn test_skin_ops(ops: &[i32], enabled_options: &[i32]) -> bool {
    ops.iter().all(|op| test_json_option_number(*op, enabled_options))
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
                toml::from_str(include_str!("../../../assets/skins/default/skin.toml"))
                    .expect("bundled default skin manifest must parse");
            manifest.with_texture_source_sizes(&default_skin_root())
        })
        .clone()
}

fn default_skin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/skins/default")
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
                scale,
                border,
                source_size,
                linear_filter,
                ..
            } => {
                append_skin_image_command(
                    commands,
                    *texture,
                    *rect,
                    *uv,
                    *tint,
                    *scale,
                    *border,
                    *source_size,
                    *linear_filter,
                );
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

    condition
        .split("&&")
        .flat_map(|segment| segment.split(" and "))
        .all(|term| eval_skin_draw_term(term.trim(), state).unwrap_or(false))
}

fn eval_skin_draw_term(term: &str, state: SkinDrawState) -> Option<bool> {
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
    match operand {
        "gauge()" | "gauge" => Some(state.gauge),
        value => value.parse::<f32>().ok(),
    }
}

fn skin_state_number(ref_id: i32, state: SkinDrawState) -> Option<i64> {
    match ref_id {
        71 | 101 | 171 => Some(state.ex_score as i64),
        72 => Some(state.total_notes as i64 * 2),
        74 | 106 | 333 => Some(state.total_notes as i64),
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
        310 => Some(state.hispeed.floor() as i64),
        311 => Some(((state.hispeed * 100.0) as i64) % 100),
        312 => Some((state.total_duration_ms / 1_000) as i64),
        // BPM 系: NUMBER_MAXBPM=90, NUMBER_MINBPM=91, NUMBER_NOWBPM=160
        90 => Some(state.max_bpm.round() as i64),
        91 => Some(state.min_bpm.round() as i64),
        160 => Some(state.now_bpm.round() as i64),
        // レーンカバー: NUMBER_LANECOVER1=14 (0-100%)
        14 => Some((state.lane_cover.clamp(0.0, 1.0) * 100.0).round() as i64),
        // 判定タイミングずれ: VALUE_JUDGE_1P_DURATION=525 (ms、絶対値)
        525 => state.judge_timing_ms.map(|ms| ms.unsigned_abs() as i64),
        420 => Some(state.judge_counts.empty_poor as i64),
        425 | 427 => Some((state.judge_counts.bad + state.judge_counts.poor) as i64),
        426 => Some(state.judge_counts.poor as i64),
        _ => None,
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
    let source_width = source_size.width.max(1.0);
    let source_height = source_size.height.max(1.0);
    let divx = image.divx.max(1);
    let divy = image.divy.max(1);
    let frame_count = divx * divy;
    let frame_index = if image.cycle > 0 && frame_count > 1 {
        (elapsed_ms.rem_euclid(image.cycle) * frame_count / image.cycle).min(frame_count - 1)
    } else {
        0
    };
    let cell_width = image.w as f32 / divx as f32;
    let cell_height = image.h as f32 / divy as f32;
    let source_column = frame_index % divx;
    let source_row = frame_index / divx;
    TextureRegion {
        x: (image.x as f32 + cell_width * source_column as f32) / source_width,
        y: (image.y as f32 + cell_height * source_row as f32) / source_height,
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
        _ => 0.0, // BARGRAPH_BESTSCORERATE, TARGETSCORERATE etc. need external data
    }
}

fn judge_rate(count: u32, total: u32) -> f32 {
    if total > 0 { count as f32 / total as f32 } else { 0.0 }
}

fn skin_slider_progress(slider_type: i32, state: SkinDrawState) -> Option<f32> {
    match slider_type {
        6 => Some(state.play_progress.clamp(0.0, 1.0)),
        _ => None,
    }
}

fn skin_timer_elapsed_ms(timer: Option<i32>, state: SkinDrawState) -> Option<i32> {
    match timer {
        None => Some(state.elapsed_ms),
        Some(40 | 41) => Some(state.elapsed_ms),
        Some(46) => state.judge_ms,
        Some(50..=57) => state.bomb_ms[(timer.unwrap() - 50) as usize],
        Some(100..=107) => state.keyon_ms[(timer.unwrap() - 100) as usize],
        Some(143) => state.end_of_note.then_some(state.elapsed_ms),
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
    match text.ref_id {
        10 => state.title.to_string(),
        11 => state.subtitle.to_string(),
        12 => full_label(state.title, state.subtitle),
        13 => state.genre.to_string(),
        14 => state.artist.to_string(),
        15 => state.subartist.to_string(),
        16 => full_label(state.artist, state.subartist),
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

/// Returns the (x, y) shift in skin canvas pixels for the given destination's offset IDs.
fn skin_offset_shift(destination: &SkinDestinationDef, state: SkinDrawState) -> (i32, i32) {
    let mut dy = 0i32;
    let all_offsets = destination.offsets.iter().copied().chain(if destination.offset != 0 {
        Some(destination.offset)
    } else {
        None
    });
    for offset_id in all_offsets {
        match offset_id {
            3 => dy += state.offset_lift_px,
            4 => dy += state.offset_lanecover_px,
            _ => {}
        }
    }
    (0, dy)
}

fn resolve_destination_frame(
    destination: &SkinDestinationDef,
    elapsed_ms: i32,
    enabled_options: &[i32],
) -> Option<ResolvedSkinFrame> {
    let elapsed_ms = destination_animation_elapsed_ms(destination, elapsed_ms);
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    let acc = destination_interpolation_acc_from_frames(&animations);
    let mut frame = ResolvedSkinFrame::default();
    let mut previous = None;
    for animation in &animations {
        apply_skin_animation(&mut frame, animation);
        if frame.time <= elapsed_ms {
            previous = Some(frame);
            continue;
        }
        return Some(match previous {
            Some(previous) => interpolate_skin_frame(previous, frame, elapsed_ms, acc),
            None => frame,
        });
    }
    previous.or_else(|| animations.first().map(|_| frame))
}

fn resolve_destination_frame_until_end(
    destination: &SkinDestinationDef,
    elapsed_ms: i32,
    enabled_options: &[i32],
) -> Option<ResolvedSkinFrame> {
    if destination.loop_time > 0 {
        return resolve_destination_frame(destination, elapsed_ms, enabled_options);
    }
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    let last_time = animations.iter().filter_map(|a| a.time).max()?;
    if elapsed_ms > last_time {
        return None;
    }
    resolve_destination_frame(destination, elapsed_ms, enabled_options)
}

fn destination_animation_elapsed_ms(destination: &SkinDestinationDef, elapsed_ms: i32) -> i32 {
    if destination.loop_time > 0 {
        elapsed_ms.rem_euclid(destination.loop_time)
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
}

fn normalize_skin_frame_rect(
    frame: ResolvedSkinFrame,
    canvas_width: u32,
    canvas_height: u32,
) -> Rect {
    let width = frame.w as f32 / canvas_width.max(1) as f32;
    let height = frame.h as f32 / canvas_height.max(1) as f32;
    Rect {
        x: if width < 0.0 {
            frame.x as f32 / canvas_width.max(1) as f32 + width
        } else {
            frame.x as f32 / canvas_width.max(1) as f32
        },
        y: if height < 0.0 {
            frame.y as f32 / canvas_height.max(1) as f32 + height
        } else {
            frame.y as f32 / canvas_height.max(1) as f32
        },
        width: width.abs(),
        height: height.abs(),
    }
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
}

impl Default for ResolvedSkinFrame {
    fn default() -> Self {
        Self { time: 0, x: 0, y: 0, w: 0, h: 0, acc: 0, a: 255, r: 255, g: 255, b: 255 }
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

fn deserialize_skin_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(SkinIdVisitor)
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
                    blend: BlendMode::Normal,
                    scale: SkinImageScale::Stretch,
                    border: None,
                    source_size: None,
                    linear_filter: false,
                },
            ],
        );

        assert_eq!(commands.len(), 2);
        assert!(matches!(commands[1], DrawCommand::Image { texture: TextureId(1), .. }));
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
        assert_eq!(document.destination[0].id, "200");
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
        assert_eq!(document.destination[0].id, "included");
        let SkinDstEntry::Frame(frame) = &document.destination[0].dst[0] else {
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
                && approx_eq(y, 0.1)
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
                && approx_eq(y, 0.2)
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
                && approx_eq(y, 0.6)
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
                && approx_eq(y, 0.3)
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
                    { "id": "panel", "loop": 300, "dst": [
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

        let wrapped = document.static_image_render_items(
            &sources,
            SkinDrawState { elapsed_ms: 350, ..SkinDrawState::default() },
        );

        assert!(matches!(wrapped[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
                if approx_eq(x, 0.15)));
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
                Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
                &sources,
            )
            .unwrap();
        let scratch = document
            .note_image_render_item(
                Lane::Scratch,
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
                && approx_eq(y, 0.1)
                && approx_eq(width, 0.1)
                && approx_eq(height, 0.1)
                && approx_eq(u, 0.1)
                && approx_eq(uv_width, 0.05)));
        assert!(matches!(items[1], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
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
                && approx_eq(y, 0.1)
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
        assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uv_width, height: uv_height },
                ..
            } if approx_eq(x, 0.3)
                && approx_eq(y, 0.15)
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
            } if approx_eq(x, 0.35) && approx_eq(u, 0.2)));
        assert!(matches!(items[3], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.4) && approx_eq(u, 0.3)));
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
                    { "id": "beam1", "timer": 51, "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] },
                    { "id": "bomb1", "timer": 51, "dst": [{ "time": 0, "x": 30, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] },
                    { "id": "beam2", "timer": 52, "dst": [{ "time": 0, "x": 50, "y": 20, "w": 20, "h": 10 }, { "time": 100 }] }
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

        let pgreat = document.imageset_render_items_for_lane(Lane::Key1, "PGREAT", 50, &sources);
        let good = document.imageset_render_items_for_lane(Lane::Key1, "GOOD", 50, &sources);
        let expired = document.imageset_render_items_for_lane(Lane::Key1, "PGREAT", 150, &sources);

        assert_eq!(pgreat.len(), 2);
        assert!(matches!(pgreat[0], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.1) && approx_eq(u, 0.1)));
        assert!(matches!(good[0], SkinRenderItem::Image {
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(u, 0.0)));
        assert!(matches!(good[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.3) && approx_eq(u, 0.2)));
        assert!(expired.is_empty());
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
                    { "id": "unknown", "src": 1, "x": 0, "y": 0, "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 9999 }
                ],
                "destination": [
                    { "id": "combo", "dst": [{ "x": 10, "y": 20, "w": 5, "h": 10 }] },
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
            SkinDrawState { elapsed_ms: 0, combo: 45, ..SkinDrawState::default() },
        );

        // combo=45 (2 digits), digit=3 → shiftbase=1, align=0 (right-aligned, default)
        // digit_step = 5/100 = 0.05, origin_x = 10/100 = 0.1
        // digit "4": x = 0.1 + 0.05 * (1+0) - 0 = 0.15
        // digit "5": x = 0.1 + 0.05 * (1+1) - 0 = 0.20
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.15) && approx_eq(y, 0.2) && approx_eq(u, 0.4)));
        assert!(matches!(items[1], SkinRenderItem::Image {
                rect: Rect { x, .. },
                uv: TextureRegion { x: u, .. },
                ..
            } if approx_eq(x, 0.20) && approx_eq(u, 0.5)));
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
            } if approx_eq(*x, 0.1)
                && approx_eq(*y, 0.2)
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
                    { "id": "unknown", "src": 1, "x": 10, "y": 20, "w": 5, "h": 6, "angle": 0, "range": 40, "type": 999 }
                ],
                "destination": [
                    { "id": "progress", "blend": 2, "dst": [{ "x": 30, "y": 80, "w": 5, "h": 6 }] },
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
            SkinDrawState { play_progress: 0.25, ..SkinDrawState::default() },
        );

        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uw, height: uh },
                blend,
                ..
            } if approx_eq(x, 0.3)
                && approx_eq(y, 0.7)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.06)
                && approx_eq(u, 0.1)
                && approx_eq(v, 0.2)
                && approx_eq(uw, 0.05)
                && approx_eq(uh, 0.06)
                && blend == BlendMode::Add));
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
                && approx_eq(y, 0.2)
                && approx_eq(width, 0.05)
                && approx_eq(height, 0.06)));
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

        let items = document.static_image_render_items(&sources, SkinDrawState::default());

        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], SkinRenderItem::Image {
                rect: Rect { x, y, width, height },
                uv: TextureRegion { x: u, y: v, width: uw, height: uh },
                tint: Color { a, .. },
                blend,
                ..
            } if approx_eq(x, 0.2)
                && approx_eq(y, -0.4)
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
        assert_eq!(skin_state_number(420, state), Some(2));
        assert_eq!(skin_state_number(425, state), Some(7));
        assert_eq!(skin_state_number(426, state), Some(3));
        assert_eq!(skin_state_number(427, state), Some(7));
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
        let mut active_bomb_ms = [None; 8];
        active_bomb_ms[1] = Some(0); // Lane::Key1.index() = 1
        let active_state = SkinDrawState { bomb_ms: active_bomb_ms, ..SkinDrawState::default() };
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
        let state_early = SkinDrawState { judge_ms: Some(100), ..SkinDrawState::default() };
        let items_early = document.static_image_render_items(&sources, state_early);
        assert_eq!(items_early.len(), 1);
        assert!(
            matches!(items_early[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.25)),
            "at judge_ms=100, x should interpolate to 0.25 (halfway between 0 and 0.5)"
        );

        // judge_ms=Some(300) → past last frame → last frame x=0.5
        let state_late = SkinDrawState { judge_ms: Some(300), ..SkinDrawState::default() };
        let items_late = document.static_image_render_items(&sources, state_late);
        assert_eq!(items_late.len(), 1);
        assert!(
            matches!(items_late[0], SkinRenderItem::Image { rect: Rect { x, .. }, .. }
            if approx_eq(x, 0.5)),
            "at judge_ms=300 (past last frame), x should be at last frame x=0.5"
        );

        // judge_ms=None → no items (timer inactive)
        let state_inactive = SkinDrawState { judge_ms: None, ..SkinDrawState::default() };
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
        // No property → no enabled options → conditional frame skipped, only end frame {time:500}
        // With elapsed=0 the element renders at the default position (0,0)
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
        let state = SkinDrawState::default();
        let items = document.static_image_render_items(&sources, state);

        // Conditional frame skipped → position defaults to (0,0); {time:500} still provides a frame
        assert_eq!(items.len(), 1);
        let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
        assert!(approx_eq(rect.x, 0.0), "expected default x=0, got {}", rect.x);
        assert!(approx_eq(rect.y, 0.0), "expected default y=0, got {}", rect.y);
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

        // With lift=72px on a 720h canvas: y shifts by 72/720 = 0.1
        assert!(approx_eq(rect_no_lift.y, 200.0 / 720.0));
        assert!(
            approx_eq(rect_lifted.y, (200 + 72) as f32 / 720.0),
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
        // y=720 shifted by -360 → 360/720 = 0.5
        assert!(approx_eq(rect.y, 360.0 / 720.0), "expected y=0.5, got {}", rect.y);
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
        // value=0.5 → height = 480/720 * 0.5, y = 480/720 * 0.5
        let dst_h = 480.0 / 720.0;
        assert!(
            approx_eq(rect.height, dst_h * 0.5),
            "bar height should be half: got {}",
            rect.height
        );
        assert!(
            approx_eq(rect.y, dst_h * 0.5),
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
        // NUMBER_DURATION (312) = 183000 / 1000 = 183
        assert_eq!(skin_state_number(312, state), Some(183));
        // VALUE_JUDGE_1P_DURATION (525) = abs(-3) = 3
        assert_eq!(skin_state_number(525, state), Some(3));
        // When no recent judgement, 525 returns None
        let no_judge = SkinDrawState { judge_timing_ms: None, ..state };
        assert_eq!(skin_state_number(525, no_judge), None);
    }

    #[test]
    fn skin_state_text_maps_string_refs() {
        let state = SkinTextState {
            title: "My Title",
            subtitle: "Sub",
            artist: "Artist Name",
            subartist: "Feat. X",
            genre: "TRANCE",
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
