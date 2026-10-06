use super::*;

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
    /// beatoraja JSON FloatProperty uses the Rate family.
    #[serde(default)]
    pub value: Option<SkinPropertyValue>,
    #[serde(default, rename = "isRefNum", deserialize_with = "deserialize_skin_bool")]
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

pub(crate) fn deserialize_lift_cover_defs<'de, D>(
    deserializer: D,
) -> Result<Vec<SkinHiddenCoverDef>, D::Error>
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

pub(crate) fn default_hiterror_width() -> i32 {
    301
}
pub(crate) fn default_hiterror_judge_width_millis() -> i32 {
    150
}
pub(crate) fn default_hiterror_line_width() -> i32 {
    1
}
pub(crate) fn default_true_int() -> i32 {
    1
}
pub(crate) fn default_hiterror_line_color() -> String {
    "99CCFF80".to_string()
}
pub(crate) fn default_hiterror_center_color() -> String {
    "FFFFFFFF".to_string()
}
pub(crate) fn default_hiterror_judge_color() -> String {
    "99CCFF80".to_string()
}
pub(crate) fn default_hiterror_ema_color() -> String {
    "FF0000FF".to_string()
}
pub(crate) fn default_hiterror_alpha() -> f32 {
    0.1
}
pub(crate) fn default_hiterror_window_length() -> i32 {
    30
}

pub(crate) fn default_timing_width() -> i32 {
    301
}
pub(crate) fn default_timing_judge_width_millis() -> i32 {
    150
}
pub(crate) fn default_timing_line_color() -> String {
    "00FF00FF".to_string()
}
pub(crate) fn default_timing_center_color() -> String {
    "FFFFFFFF".to_string()
}
pub(crate) fn default_timing_pg_color() -> String {
    "000088FF".to_string()
}
pub(crate) fn default_timing_gr_color() -> String {
    "008800FF".to_string()
}
pub(crate) fn default_timing_gd_color() -> String {
    "888800FF".to_string()
}
pub(crate) fn default_timing_bd_color() -> String {
    "880000FF".to_string()
}
pub(crate) fn default_timing_pr_color() -> String {
    "000000FF".to_string()
}

pub(crate) fn default_gaugegraph_assist_clear_bg_color() -> String {
    "440044".to_string()
}
pub(crate) fn default_gaugegraph_assist_easy_fail_bg_color() -> String {
    "004444".to_string()
}
pub(crate) fn default_gaugegraph_groove_fail_bg_color() -> String {
    "004400".to_string()
}
pub(crate) fn default_gaugegraph_groove_clear_hard_bg_color() -> String {
    "440000".to_string()
}
pub(crate) fn default_gaugegraph_exhard_bg_color() -> String {
    "444400".to_string()
}
pub(crate) fn default_gaugegraph_hazard_bg_color() -> String {
    "444444".to_string()
}
pub(crate) fn default_gaugegraph_assist_clear_line_color() -> String {
    "ff00ff".to_string()
}
pub(crate) fn default_gaugegraph_assist_easy_fail_line_color() -> String {
    "00ffff".to_string()
}
pub(crate) fn default_gaugegraph_groove_fail_line_color() -> String {
    "00ff00".to_string()
}
pub(crate) fn default_gaugegraph_groove_clear_hard_line_color() -> String {
    "ff0000".to_string()
}
pub(crate) fn default_gaugegraph_exhard_line_color() -> String {
    "ffff00".to_string()
}
pub(crate) fn default_gaugegraph_hazard_line_color() -> String {
    "cccccc".to_string()
}
pub(crate) fn default_gaugegraph_borderline_color() -> String {
    "ff0000".to_string()
}
pub(crate) fn default_gaugegraph_border_color() -> String {
    "440000".to_string()
}
