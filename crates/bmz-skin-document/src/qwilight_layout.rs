//! Immutable Qwilight layout recipes, evaluated only when settings change.
use std::{collections::BTreeMap, sync::Arc};

use crate::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QwilightOffsetValue {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub a: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QwilightRectRecipe {
    pub id: i32,
    pub bounds: [f64; 4],
    pub area: [f64; 4],
    #[serde(default)]
    pub own: Option<i32>,
    #[serde(default)]
    pub fade: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum RectTarget {
    Destination(usize, usize),
    Judge(usize, usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
struct RectBinding {
    target: RectTarget,
    recipe: QwilightRectRecipe,
    alpha: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct QwilightMetricRecipe {
    pub id: i32,
    pub base: f64,
    /// Absent means an additive height; otherwise scale with this group dimension.
    #[serde(default)]
    pub span: Option<f64>,
    #[serde(default)]
    pub width: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QwilightLayout {
    height: f64,
    rects: Vec<RectBinding>,
    text_sizes: Vec<(usize, QwilightMetricRecipe)>,
    slider_ranges: Vec<(usize, QwilightMetricRecipe)>,
    visualizers: Vec<(usize, QwilightMetricRecipe, QwilightMetricRecipe)>,
}

impl QwilightLayout {
    /// Markers are emitted by the adapter, not accepted as runtime skin instructions.
    pub fn from_adapter_json(json: &JsonValue, height: f64) -> Result<Self> {
        let mut layout = Self {
            height,
            rects: Vec::new(),
            text_sizes: Vec::new(),
            slider_ranges: Vec::new(),
            visualizers: Vec::new(),
        };
        let mut add =
            |destination: &JsonValue, target: &dyn Fn(usize) -> RectTarget| -> Result<()> {
                if let Some(frames) = destination["dst"].as_array() {
                    for (i, frame) in frames.iter().enumerate() {
                        if let Some(recipe) = frame.get("qwOffsetRect") {
                            layout.rects.push(RectBinding {
                                target: target(i),
                                recipe: serde_json::from_value(recipe.clone())?,
                                alpha: frame["a"].as_i64().map(|a| a as i32),
                            });
                        }
                    }
                }
                Ok(())
            };
        if let Some(destinations) = json["destination"].as_array() {
            for (i, destination) in destinations.iter().enumerate() {
                add(destination, &|frame| RectTarget::Destination(i, frame))?;
            }
        }
        if let Some(judges) = json["judge"].as_array() {
            for (i, judge) in judges.iter().enumerate() {
                if let Some(images) = judge["images"].as_array() {
                    for (j, image) in images.iter().enumerate() {
                        add(image, &|frame| RectTarget::Judge(i, j, frame))?;
                    }
                }
            }
        }
        for (field, marker, output) in [
            ("text", "qwOffsetSize", &mut layout.text_sizes),
            ("slider", "qwOffsetRange", &mut layout.slider_ranges),
        ] {
            if let Some(items) = json[field].as_array() {
                for (i, item) in items.iter().enumerate() {
                    if let Some(recipe) = item.get(marker) {
                        output.push((i, serde_json::from_value(recipe.clone())?));
                    }
                }
            }
        }
        if let Some(items) = json["qwilightVisualizer"].as_array() {
            for (i, item) in items.iter().enumerate() {
                if let Some(width) = item.get("qwOffsetWidth") {
                    layout.visualizers.push((
                        i,
                        serde_json::from_value(width.clone())?,
                        serde_json::from_value(item["qwOffsetHeight"].clone())?,
                    ));
                }
            }
        }
        Ok(layout)
    }
}

fn adjust(
    offset: QwilightOffsetValue,
    bounds: [f64; 4],
    area: [f64; 4],
    keep_top: bool,
) -> [f64; 4] {
    let [bx, by, bw, bh] = bounds;
    let [x, y, w, h] = area;
    let width = (bw + f64::from(offset.w)).max(1.0);
    let height = (bh + f64::from(offset.h)).max(1.0);
    let sx = if bw > 0.0 { width / bw } else { 1.0 };
    let sy = if bh > 0.0 { height / bh } else { 1.0 };
    let dy = if keep_top { 0.0 } else { -f64::from(offset.y) - (height - bh) / 2.0 };
    [
        bx + f64::from(offset.x) - (width - bw) / 2.0 + (x - bx) * sx,
        by + dy + (y - by) * sy,
        w * sx,
        h * sy,
    ]
}

impl SkinDocument {
    /// Apply absolute settings from the original geometry: repeated edits never accumulate drift.
    pub fn apply_qwilight_offsets(&mut self, values: &BTreeMap<i32, QwilightOffsetValue>) -> bool {
        let Some(layout) = self.qwilight_layout.as_ref().map(Arc::clone) else { return false };
        let offset = |id| {
            if id < 0 {
                QwilightOffsetValue::default()
            } else {
                values.get(&id).copied().unwrap_or_default()
            }
        };
        for binding in &layout.rects {
            let frame = match binding.target {
                RectTarget::Destination(i, j) => {
                    self.destination.get_mut(i).and_then(|d| match d {
                        DestinationListEntry::Single(d) => d.dst.get_mut(j),
                        _ => None,
                    })
                }
                RectTarget::Judge(i, j, k) => self
                    .judge
                    .get_mut(i)
                    .and_then(|judge| judge.images.get_mut(j))
                    .and_then(|d| d.dst.get_mut(k)),
            };
            let Some(SkinDstEntry::Frame(frame)) = frame else { continue };
            let recipe = &binding.recipe;
            let value = offset(recipe.id);
            let mut area = recipe.area;
            if value != QwilightOffsetValue::default() {
                area = adjust(value, recipe.bounds, area, recipe.id == 1131);
            }
            let own = recipe.own.map(offset).unwrap_or_default();
            if own != QwilightOffsetValue::default() {
                area = adjust(own, area, area, false);
            }
            let [x, y, w, h] = area;
            frame.x = Some(x.round() as i32);
            frame.y = Some((layout.height - y - h).round() as i32);
            frame.w = Some(w.round() as i32);
            frame.h = Some(h.round() as i32);
            let delta = i64::from(value.a) + i64::from(own.a);
            frame.a = if recipe.fade || (recipe.id == 1101 && recipe.own.is_none()) || delta == 0 {
                binding.alpha
            } else {
                Some((i64::from(binding.alpha.unwrap_or(255)) + delta).clamp(0, 255) as i32)
            };
        }
        let metric = |recipe: &QwilightMetricRecipe| {
            let value = offset(recipe.id);
            let delta = f64::from(if recipe.width { value.w } else { value.h });
            match recipe.span {
                Some(span) if span > 0.0 => {
                    (recipe.base * (span + delta).max(1.0) / span).round() as i32
                }
                Some(_) => recipe.base.round() as i32,
                None => (recipe.base + delta).max(1.0).round() as i32,
            }
        };
        // A target missing from this document is skipped, as rect bindings are.
        for (i, recipe) in &layout.text_sizes {
            if let Some(text) = self.text.get_mut(*i) {
                text.size = metric(recipe);
            }
        }
        for (i, recipe) in &layout.slider_ranges {
            if let Some(slider) = self.slider.get_mut(*i) {
                slider.range = metric(recipe);
            }
        }
        for (i, width, height) in &layout.visualizers {
            let Some(visualizer) = self.qwilight_visualizer.get_mut(*i) else { continue };
            visualizer.line_width = metric(width);
            visualizer.line_height = metric(height);
            if width.id == 1101 {
                visualizer.opacity =
                    Some((0.1 + offset(width.id).a as f32 / 255.0).clamp(0.0, 1.0));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_resize_metrics_and_skip_targets_missing_from_the_document() {
        let json = serde_json::json!({
            "type": 0, "w": 100, "h": 100,
            "text": [{"id": "title", "size": 20, "qwOffsetSize": {"id": 1147, "base": 20.0}}],
            "slider": [{"id": "status", "range": 40,
                "qwOffsetRange": {"id": 1128, "base": 40.0, "span": 40.0}}],
            "qwilightVisualizer": [{"id": "spectrum", "kind": "spectrum", "lineWidth": 2,
                "lineHeight": 10,
                "qwOffsetWidth": {"id": 1101, "base": 2.0, "span": 100.0, "width": true},
                "qwOffsetHeight": {"id": 1101, "base": 10.0, "span": 10.0}}],
        });
        let layout = Arc::new(QwilightLayout::from_adapter_json(&json, 100.0).unwrap());
        let mut document: SkinDocument = serde_json::from_value(json).unwrap();
        document.qwilight_layout = Some(layout);
        let value = |w, h| QwilightOffsetValue { w, h, ..Default::default() };
        let values =
            BTreeMap::from([(1147, value(0, 4)), (1128, value(40, 0)), (1101, value(100, 10))]);
        let mut full = document.clone();
        assert!(full.apply_qwilight_offsets(&values));
        assert_eq!(full.text[0].size, 24);
        assert_eq!(full.slider[0].range, 40, "the vertical bar keeps its travel for w");
        assert_eq!(
            (full.qwilight_visualizer[0].line_width, full.qwilight_visualizer[0].line_height),
            (4, 20)
        );
        // A document whose lists no longer match the recipe must not panic.
        document.text.clear();
        document.slider.clear();
        document.qwilight_visualizer.clear();
        assert!(document.apply_qwilight_offsets(&values));
    }
}
