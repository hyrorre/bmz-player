use super::*;

// IDs stay outside the runtime offset range: these are applied once during
// conversion, after the skin's Lua layout and before live LIFT/Judge offsets.
type Element = (i32, &'static str, &'static [&'static str]);
const ELEMENTS: &[Element] = &[
    (0, "BGA", &["media"]),
    (1, "Spectrum", &["audioVisualizer"]),
    (6, "Gauge", &["hitPointsPosition", "hitPointsLength", "hitPointsHeight"]),
    (8, "Key beam", &["inputPosition", "inputHeight"]),
    (9, "Combo", &["bandPosition"]),
    (10, "Timing ms", &["judgmentMeterPosition"]),
    (11, "Score", &["standPosition"]),
    (12, "Score rate", &["pointPosition"]),
    (13, "BPM", &["bpmPosition"]),
    (14, "HS", &["multiplierPosition"]),
    (15, "Judgment", &["judgmentPaintPosition"]),
    (28, "Progress", &["statusPosition", "statusSliderPosition"]),
    (29, "Time", &["hmsPosition"]),
    (30, "Judgment ratio bar", &["judgmentPointsPosition"]),
    (31, "Judgment line", &["judgmentMainPosition1", "judgmentMainHeight"]),
    (33, "Playback speed", &["audioMultiplierPosition"]),
    (34, "Gauge percent", &["hitPointsVisualizerPosition"]),
    (
        35,
        "Judgment counts",
        &[
            "highestJudgmentValue",
            "higherJudgmentValue",
            "highJudgmentValue",
            "lowJudgmentValue",
            "lowerJudgmentValue",
            "lowestJudgmentValue",
        ],
    ),
    (41, "Max combo", &["highestBandPosition"]),
    (43, "Timing", &["judgmentVisualizerPosition"]),
    (44, "NPS", &["inputVisualizerPosition"]),
    (45, "Bomb", &["hitNotePaint", "hitLongNotePaint"]),
    (46, "Target difference", &["hunterPosition"]),
    (47, "Title", &["titlePosition"]),
    (48, "Artist", &["artistPosition"]),
    (51, "Genre", &["genrePosition"]),
    (52, "Level", &["levelTextPosition"]),
    (54, "FAST", &["earlyValuePosition"]),
    (55, "SLOW", &["lateValuePosition"]),
    (56, "RATE", &["judgmentVSVisualizerPosition"]),
    (57, "Lane timing", &["mainJudgmentMeterPosition"]),
    (59, "Table", &["wantLevelPosition"]),
    (60, "Difficulty", &["levelPosition"]),
    (61, "Judgment graph", &["judgmentInputVisualizerPosition"]),
];

/// Per-decoration controls: `paintProperty{n}` (`Drawing/P {n}`) uses ID 1200 + n.
const DECORATION_ID_BASE: i32 = 1200;

fn decoration_name(index: usize) -> String {
    format!("Qwilight Decoration {index} offset")
}

/// Scale `area` with its group `bounds` about their centre, then move it.
/// Qwilight uses downward-positive y; `keep_top` holds the judgment line's hit
/// position, which only LIFT moves.
fn adjust_area(
    offset: LuaSkinOffsetValue,
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

fn element(pipeline: i32) -> Option<&'static Element> {
    let pipeline = match pipeline {
        35..=40 => 35,
        58 => 28,
        _ => pipeline,
    };
    ELEMENTS.iter().find(|(id, _, _)| *id == pipeline)
}

pub(in crate::qwilight) fn definitions(definition: &Definition) -> Vec<Json> {
    let points = definition.yaml["point"].as_mapping();
    let mut decorations: Vec<usize> = points
        .into_iter()
        .flat_map(|points| points.keys())
        .filter_map(Yaml::as_str)
        .filter_map(|key| key.strip_prefix("paintProperty")?.parse().ok())
        .filter(|index| *index < 256)
        .collect();
    decorations.sort_unstable();
    decorations.dedup();
    let decorations = decorations.into_iter().map(|index| {
        json!({
            "category":"Qwilight decorations", "name":decoration_name(index),
            "id":DECORATION_ID_BASE + index as i32,
            "x":true, "y":true, "w":true, "h":true, "r":false, "a":true
        })
    });
    ELEMENTS
        .iter()
        .filter(|(_, _, prefixes)| {
            definition.yaml["point"].as_mapping().is_some_and(|points| {
                points
                    .keys()
                    .filter_map(Yaml::as_str)
                    .any(|key| prefixes.iter().any(|p| key.starts_with(p)))
            })
        })
        .map(|(id, name, _)| {
            json!({
                "category":"Qwilight elements", "name":format!("Qwilight {name} offset"),
                "id":1100+id, "x":true, "y":*id != 31, "w":true, "h":true, "r":false, "a":true
            })
        })
        .chain(decorations)
        .collect()
}

impl Builder {
    pub(super) fn element_offset(&self, pipeline: i32) -> LuaSkinOffsetValue {
        element(pipeline)
            .and_then(|(_, name, _)| self.element_offsets.get(&format!("Qwilight {name} offset")))
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn element_alpha(&self, pipeline: i32, alpha: i32) -> i32 {
        (i64::from(alpha) + i64::from(self.element_offset(pipeline).a)).clamp(0, 255) as i32
    }

    pub(super) fn element_height(&self, pipeline: i32, height: f64) -> f64 {
        (height + f64::from(self.element_offset(pipeline).h)).max(1.0)
    }

    pub(super) fn element_rect(&self, pipeline: i32, area: [f64; 4]) -> Json {
        if area[2] <= 0.0 || area[3] <= 0.0 {
            return self.rect(area[0], area[1], area[2], area[3]);
        }
        self.group_rect(pipeline, area, area)
    }

    /// Resize one logical display, not each digit/label independently. Bounds
    /// are supplied per conditional layout and per DP copy (or per lane).
    pub(super) fn group_rect(&self, pipeline: i32, bounds: [f64; 4], area: [f64; 4]) -> Json {
        if let Some((id, _, _)) = element(pipeline) {
            let mut recorded = self.element_bounds.borrow_mut();
            let list = recorded.entry(*id).or_default();
            if !list.contains(&bounds) {
                list.push(bounds);
            }
        }
        let [x, y, w, h] = area;
        let offset = self.element_offset(pipeline);
        if offset == LuaSkinOffsetValue::default() {
            return self.rect(x, y, w, h);
        }
        let [x, y, w, h] = adjust_area(offset, bounds, area, pipeline == 31);
        let mut rect = self.rect(x, y, w, h);
        // Spectrum has its own 10% base opacity; its alpha is adjusted there.
        if pipeline != 1 && offset.a != 0 {
            rect["a"] = json!(self.element_alpha(pipeline, 255));
        }
        rect
    }

    /// The associated element's unadjusted bounds for a decoration: those it
    /// overlaps horizontally (its DP side, or the lanes it spans), else the nearest.
    fn associated_bounds(&self, pipeline: i32, area: [f64; 4]) -> Option<[f64; 4]> {
        let (id, _, _) = element(pipeline)?;
        let recorded = self.element_bounds.borrow();
        let list = recorded.get(id)?;
        let (left, right) = (area[0], area[0] + area[2]);
        let overlapping: Vec<_> =
            list.iter().filter(|b| b[0] < right && b[0] + b[2] > left).copied().collect();
        let chosen =
            if overlapping.is_empty() {
                let centre = |b: &[f64; 4]| b[0] + b[2] / 2.0;
                let target = (left + right) / 2.0;
                vec![*list.iter().min_by(|a, b| {
                    (centre(a) - target).abs().total_cmp(&(centre(b) - target).abs())
                })?]
            } else {
                overlapping
            };
        let [l, t, r, b] = chosen.iter().fold(
            [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
            |[l, t, r, b], [x, y, w, h]| [l.min(*x), t.min(*y), r.max(x + w), b.max(y + h)],
        );
        Some([l, t, r - l, b - t])
    }

    /// A decoration follows its associated element (`paintProperty` field `I`)
    /// as part of that element's group, then its own decoration offset.
    pub(super) fn decoration_rect(
        &self,
        index: usize,
        element_hint: Option<i32>,
        area: [f64; 4],
    ) -> Json {
        let mut area = area;
        let mut alpha = 255;
        if let Some(pipeline) = element_hint {
            let offset = self.element_offset(pipeline);
            if offset != LuaSkinOffsetValue::default()
                && let Some(bounds) = self.associated_bounds(pipeline, area)
            {
                area = adjust_area(offset, bounds, area, pipeline == 31);
                alpha += offset.a;
            }
        }
        let own = self.element_offsets.get(&decoration_name(index)).copied().unwrap_or_default();
        if own != LuaSkinOffsetValue::default() {
            area = adjust_area(own, area, area, false);
            alpha += own.a;
        }
        let [x, y, w, h] = area;
        let mut rect = self.rect(x, y, w, h);
        if alpha != 255 {
            rect["a"] = json!(alpha.clamp(0, 255));
        }
        rect
    }

    pub(super) fn judgment_count_bounds(&self) -> Result<[f64; 4]> {
        let offset = self.element_offset(35);
        if offset.w == 0 && offset.h == 0 {
            return Ok([0.0, 0.0, 1.0, 1.0]);
        }
        let mut bounds: Option<[f64; 4]> = None;
        for prefix in ["highest", "higher", "high", "low", "lower", "lowest"] {
            let name = format!("{prefix}JudgmentValue");
            let bin = format!("bin{}JudgmentValue", {
                let mut chars = prefix.chars();
                format!("{}{}", chars.next().unwrap().to_ascii_uppercase(), chars.as_str())
            });
            let w = self.point(&format!("{bin}Length"), self.point("binLength", 0.0)?)? * 5.0;
            let h = self.point(&format!("{bin}Height"), self.point("binHeight", 0.0)?)?;
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let align = self.point(&format!("{name}System"), 0.0)? as i32;
            let x = self.point(&format!("{name}Position0"), 0.0)?
                - match align {
                    1 => w / 2.0,
                    2 => w,
                    _ => 0.0,
                };
            let y = self.point(&format!("{name}Position1"), 0.0)?;
            bounds = Some(match bounds {
                None => [x, y, x + w, y + h],
                Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x + w), b.max(y + h)],
            });
        }
        Ok(bounds.map_or([0.0, 0.0, 1.0, 1.0], |[l, t, r, b]| [l, t, r - l, b - t]))
    }
}
