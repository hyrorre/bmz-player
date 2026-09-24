use super::*;

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
    /// BMZ HLN extension. Undefined parts fall back to the matching HCN/LN part.
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hlnstart: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hlnend: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hlnbody: Vec<String>,
    #[serde(default, rename = "hlnbodyActive", deserialize_with = "deserialize_skin_id_vec")]
    pub hlnbody_active: Vec<String>,
    #[serde(default, rename = "hlnbodyReactive", deserialize_with = "deserialize_skin_id_vec")]
    pub hlnbody_reactive: Vec<String>,
    #[serde(default, rename = "hlnbodyMiss", deserialize_with = "deserialize_skin_id_vec")]
    pub hlnbody_miss: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub mine: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub hidden: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_skin_id_vec")]
    pub processed: Vec<String>,
    #[serde(default)]
    pub size: Vec<i32>,
    #[serde(default = "default_note_dst2")]
    pub dst2: i32,
    #[serde(default = "default_note_expansion_rate")]
    pub expansionrate: Vec<i32>,
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

fn default_note_dst2() -> i32 {
    i32::MIN
}

fn default_note_expansion_rate() -> Vec<i32> {
    vec![100, 100]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinPmCharaDef {
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_skin_id")]
    pub src: String,
    #[serde(default = "default_pm_chara_color")]
    pub color: i32,
    #[serde(default, rename = "type")]
    pub chara_type: i32,
    #[serde(default = "default_pm_chara_side")]
    pub side: i32,
    /// BMZ が選択済み `.chp` を展開した描画用データ。
    #[serde(skip)]
    pub runtime: Option<SkinPmCharaRuntimeDef>,
}

fn default_pm_chara_color() -> i32 {
    1
}

fn default_pm_chara_side() -> i32 {
    1
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkinPmCharaRuntimeDef {
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub motions: Vec<SkinPmCharaMotionLayerDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinPmCharaMotionLayerDef {
    pub motion: i32,
    pub source_id: String,
    pub frame_ms: i32,
    /// Number of one-shot frames before the looping suffix begins.
    pub loop_start: usize,
    pub frames: Vec<SkinPmCharaFrameDef>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinPmCharaFrameDef {
    pub source_x: i32,
    pub source_y: i32,
    pub source_w: i32,
    pub source_h: i32,
    pub destination_x: i32,
    pub destination_y: i32,
    pub destination_w: i32,
    pub destination_h: i32,
    pub alpha: i32,
    pub angle: i32,
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
    /// `loop` フィールド。未指定(None)はLR2互換評価で0時刻へループバックする。
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
    /// BMZ extension: make any destination, including text and panel, an event target.
    #[serde(default)]
    pub act: Option<i32>,
    #[serde(default)]
    pub click: i32,
    /// Explicitly disable or enable the destination click target.
    /// When omitted, destination `act` or the legacy image/imageset attributes apply.
    #[serde(default)]
    pub clickable: Option<bool>,
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
