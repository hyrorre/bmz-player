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
mod property_names;
mod property_value;
pub use property_names::*;
pub use property_value::*;
mod lr2;
pub use lr2::*;
mod runtime;
#[cfg(test)]
mod tests;

pub use load::*;
pub use runtime::*;

#[path = "schema/constants.rs"]
mod schema_constants;
#[path = "schema/document.rs"]
mod schema_document;
#[path = "schema/play.rs"]
mod schema_play;
#[path = "schema/visualizers.rs"]
mod schema_visualizers;

pub use schema_constants::*;
pub use schema_document::*;
pub use schema_play::*;
pub use schema_visualizers::*;

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

pub fn deserialize_skin_bool<'de, D>(deserializer: D) -> std::result::Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = JsonValue::deserialize(deserializer)?;
    match value {
        JsonValue::Bool(value) => Ok(value),
        JsonValue::Number(value) if value.as_i64() == Some(0) => Ok(false),
        JsonValue::Number(value) if value.as_i64() == Some(1) => Ok(true),
        _ => Err(D::Error::custom("expected a boolean or integer 0/1")),
    }
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
    /// DETAIL OPTIONS is opt-in; absent/unknown versions keep the legacy E2 UI.
    pub fn uses_detail_options(&self) -> bool {
        self.skin_type == 5 && self.bmz_detail_options == 1
    }

    pub fn load_beatoraja_json(path: &Path) -> Result<Self> {
        let raw = load_json_value(path)?;
        let options = default_enabled_options(&raw);
        Self::load_beatoraja_json_value_with_options(path, raw, &options)
    }

    pub fn load_beatoraja_json_with_options(path: &Path, enabled_options: &[i32]) -> Result<Self> {
        let raw = load_json_value(path)?;
        Self::load_beatoraja_json_value_with_options(path, raw, enabled_options)
    }

    /// Loads a JSON skin through the normal include/condition/number pipeline,
    /// but returns `None` when the expanded document does not explicitly declare
    /// a numeric skin type. This is intended for catalog discovery, where an
    /// unrelated JSON object must not inherit `SkinDocument`'s type-0 default.
    pub fn load_typed_beatoraja_json(path: &Path) -> Result<Option<Self>> {
        let raw = load_json_value(path)?;
        let options = default_enabled_options(&raw);
        let expanded = load_expanded_beatoraja_json_value(path, raw, &options)?;
        if expanded.get("type").and_then(JsonValue::as_i64).is_none() {
            return Ok(None);
        }
        serde_json::from_value(expanded)
            .with_context(|| format!("failed to parse skin json: {}", path.display()))
            .map(Some)
    }

    fn load_beatoraja_json_value_with_options(
        path: &Path,
        raw: JsonValue,
        enabled_options: &[i32],
    ) -> Result<Self> {
        let expanded = load_expanded_beatoraja_json_value(path, raw, enabled_options)?;
        serde_json::from_value(expanded)
            .with_context(|| format!("failed to parse skin json: {}", path.display()))
    }
}

fn load_expanded_beatoraja_json_value(
    path: &Path,
    raw: JsonValue,
    enabled_options: &[i32],
) -> Result<JsonValue> {
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let expanded = expand_json_skin_value(raw, root, root, enabled_options)
        .with_context(|| format!("failed to expand skin json: {}", path.display()))?;
    Ok(normalize_json_skin_integer_numbers(expanded))
}

impl SkinDocument {
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
    /// space (top-left origin). This is a static document-inspection helper;
    /// runtime input bounds use bmz-render's resolved destination frame so skin
    /// options, offsets, and the canvas viewport are included.
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

    /// beatoraja JSON play skin の `practice.id` が参照する destination の
    /// 初期位置を、画面左上原点の正規化座標で返す。
    pub fn practice_destination_position(&self) -> Option<(f32, f32)> {
        let practice_id = self.practice.as_ref()?.id.as_str();
        if practice_id.is_empty() {
            return None;
        }
        let canvas_w = self.w.max(1) as f32;
        let canvas_h = self.h.max(1) as f32;
        for entry in &self.destination {
            let candidates: Vec<&SkinDestinationDef> = match entry {
                DestinationListEntry::Single(destination) => vec![destination],
                DestinationListEntry::Conditional { destinations, .. } => {
                    destinations.iter().collect()
                }
            };
            for destination in candidates {
                if destination.id != practice_id {
                    continue;
                }
                let frame = destination.dst.iter().find_map(|dst| match dst {
                    SkinDstEntry::Frame(frame) => Some(frame),
                    SkinDstEntry::Conditional { frames, .. } => frames.first(),
                })?;
                let raw_x = frame.x.unwrap_or(0) as f32;
                let raw_y = frame.y.unwrap_or(0) as f32;
                let raw_h = frame.h.unwrap_or(0).max(0) as f32;
                return Some((raw_x / canvas_w, (canvas_h - (raw_y + raw_h)) / canvas_h));
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
