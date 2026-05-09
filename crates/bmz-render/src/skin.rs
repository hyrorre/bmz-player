use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use bmz_core::lane::Lane;
use serde::{Deserialize, Deserializer};
use serde::de::{Error as DeError, Visitor};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::assets::load_png_rgba;
use crate::plan::{Color, DrawCommand, Point, Rect, TextStyle, TextureId, UvRect};

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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinTextDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub font: String,
    #[serde(default)]
    pub size: i32,
    #[serde(default)]
    pub align: i32,
    #[serde(default)]
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
    pub dst: Vec<SkinAnimationDef>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct SkinContext {
    manifest: SkinManifest,
}

impl Default for SkinContext {
    fn default() -> Self {
        Self { manifest: default_skin_manifest() }
    }
}

impl SkinContext {
    pub fn from_manifest(manifest: SkinManifest) -> Self {
        Self { manifest }
    }

    pub fn manifest(&self) -> &SkinManifest {
        &self.manifest
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
                    },
                    SkinSource::Text { slot, style } => SkinRenderItem::Text {
                        origin: Point { x: resolved.rect.x, y: resolved.rect.y },
                        text: text(*slot),
                        style: style.with_alpha(resolved.alpha),
                        blend: resolved.blend,
                    },
                    SkinSource::Number { slot, style, digits } => SkinRenderItem::Text {
                        origin: Point { x: resolved.rect.x, y: resolved.rect.y },
                        text: format_number(number(*slot), *digits),
                        style: style.with_alpha(resolved.alpha),
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
                expanded.push(expand_json_skin_value(item, current_dir, root_dir, enabled_options)?);
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
    let include = include
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("skin json include must be a string"))?;
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
                        style: *style,
                    });
                }
            }
            SkinRenderItem::Image {
                texture, rect, uv, tint, scale, border, source_size, ..
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
            );
        }
        _ => commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            texture: TextureId(texture.0),
            tint,
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
        Self { color: self.color.with_alpha(self.color.a * alpha), ..self }
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
                    size: 0.04,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: TextLayer::Skin,
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
                        size: 0.04,
                        color: Color::rgb(1.0, 1.0, 1.0),
                        layer: TextLayer::Skin,
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
        assert_eq!(document.destination[0].dst[0].x, Some(1));
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

    fn approx_eq(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.0001
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        path
    }
}
