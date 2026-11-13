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

fn element(pipeline: i32) -> Option<&'static Element> {
    let pipeline = match pipeline {
        35..=40 => 35,
        58 => 28,
        _ => pipeline,
    };
    ELEMENTS.iter().find(|(id, _, _)| *id == pipeline)
}

pub(in crate::qwilight) fn definitions(definition: &Definition) -> Vec<Json> {
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
        let [bx, by, bw, bh] = bounds;
        let [x, y, w, h] = area;
        let offset = self.element_offset(pipeline);
        if offset == LuaSkinOffsetValue::default() {
            return self.rect(x, y, w, h);
        }
        let width = (bw + f64::from(offset.w)).max(1.0);
        let height = (bh + f64::from(offset.h)).max(1.0);
        let sx = if bw > 0.0 { width / bw } else { 1.0 };
        let sy = if bh > 0.0 { height / bh } else { 1.0 };
        // Qwilight uses downward-positive y. Keep the judgment line's top
        // fixed when changing thickness: only LIFT may move its hit position.
        let dy = if pipeline == 31 { 0.0 } else { -f64::from(offset.y) - (height - bh) / 2.0 };
        let mut rect = self.rect(
            bx + f64::from(offset.x) - (width - bw) / 2.0 + (x - bx) * sx,
            by + dy + (y - by) * sy,
            w * sx,
            h * sy,
        );
        // Spectrum has its own 10% base opacity; its alpha is adjusted there.
        if pipeline != 1 && offset.a != 0 {
            rect["a"] = json!(self.element_alpha(pipeline, 255));
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
