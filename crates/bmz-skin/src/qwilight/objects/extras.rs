use super::*;

impl Builder {
    pub(super) fn extras(&mut self) -> Result<()> {
        for (pipeline, name, alt, stem, mode, variants) in [
            (6, "hitPoints", "alt-hit-points", "HP", "gauge", 6),
            (28, "status", "alt-status", "Status", "progress", 1),
            (30, "judgmentPoints", "alt-judgment-points", "J", "ratio", 6),
        ] {
            let [x, y, w, h] = self.area(name)?;
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let system = self.point(&format!("{name}System"), 0.0)? as i32;
            let (count, cycle) = if pipeline == 6 { self.frames("hit-points")? } else { (1, 0) };
            let beat =
                pipeline == 6 && self.eval.number("lambda", "hitPointsMode", 0.0)? as i32 == 1;
            for variant in 0..variants {
                let stem = if variants == 1 {
                    format!("Drawing/{stem}")
                } else {
                    format!("Drawing/{stem} {variant}")
                };
                let frames = count + usize::from(beat);
                let mut paths = self
                    .resources
                    .animated(frames, |frame| Ok(self.resources.frame(&stem, frame, "")))?;
                if pipeline == 6 {
                    let fallback = self.resources.animated(frames, |frame| {
                        Ok(self.resources.frame("Drawing/HP 1", frame, ""))
                    })?;
                    if paths.is_empty() {
                        paths = fallback;
                    } else {
                        for (path, fallback) in paths.iter_mut().zip(fallback) {
                            if path == "bmz://lr2/empty" {
                                *path = fallback;
                            }
                        }
                    }
                }
                let id = format!("qw-{name}-{variant}");
                let frames = paths.len();
                if self.image(&id, paths, cycle, None) {
                    let fps = if pipeline == 6 {
                        self.eval.number("frame", "hit-points-framerate", 60.0)?
                    } else {
                        0.0
                    };
                    self.document["image"].as_array_mut().unwrap().last_mut().unwrap()["qwilightAnimation"] = json!({"mode":mode,"count":frames,"fps":fps as u32,"index":variant,"system":if mode=="ratio" {system^1} else {system},"looping":i32::from(beat)});
                    for dx in self.alt_offsets(alt, 2)? {
                        self.destination(
                            pipeline,
                            &id,
                            self.element_rect(pipeline, [x + dx, y, w, h]),
                            None,
                            None,
                        );
                    }
                }
            }
        }
        self.progress_thumb()?;
        self.auto_indicators()?;
        self.supplementary_numbers()?;
        self.lane_timing()?;
        Ok(())
    }

    fn progress_thumb(&mut self) -> Result<()> {
        let Some(path) = self.resources.path("Drawing/Status Slider") else { return Ok(()) };
        let [x, y, w, h] = self.area("statusSlider")?;
        let cw = self.point("statusSliderContentsLength", w)?;
        let ch = self.point("statusSliderContentsHeight", h)?;
        let system = self.point("statusSliderSystem", 0.0)? as i32;
        let mut bounds = self.area("status")?;
        if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            bounds = [x, y, w, h];
        }
        let axis = if system < 2 { 3 } else { 2 };
        let src = self.source(vec![path], 1);
        self.push("slider",json!({"id":"qw-status-thumb","src":src,"w":-1,"h":-1,"type":6,
            "angle":match system {0=>0,1=>2,2=>3,_=>1},"range":(if system<2 {h} else {w}).round() as i32,"changeable":false,
            "qwOffsetRange":self.metric(28,if system<2 {h} else {w},Some(bounds[axis]),system>=2)}));
        let (x, y) = match system {
            0 => (x, y + h - ch / 2.0),
            1 => (x, y - ch / 2.0),
            2 => (x + w - cw / 2.0, y),
            _ => (x - cw / 2.0, y),
        };
        for dx in self.alt_offsets("alt-status-slider", 0)? {
            self.destination(
                58,
                "qw-status-thumb",
                self.group_rect(
                    28,
                    [bounds[0] + dx, bounds[1], bounds[2], bounds[3]],
                    [x + dx, y, cw, ch],
                ),
                None,
                None,
            );
        }
        Ok(())
    }

    fn lane_timing(&mut self) -> Result<()> {
        let (count, cycle) = self.frames("main-judgment-meter")?;
        let fps = self.eval.number("frame", "main-judgment-meter-framerate", 60.0)?;
        for (i, lane) in self.layout.lanes.clone().iter().enumerate() {
            let y = self.eval.lane_value("mainJudgmentMeterPosition1", lane.drawing, 0.0)?;
            let h = self.eval.lane_value("mainJudgmentMeterHeight", lane.drawing, 0.0)?;
            if h <= 0.0 {
                continue;
            }
            let mut sides = [Vec::new(), Vec::new()];
            for (side, paths) in sides.iter_mut().enumerate() {
                *paths = self.resources.animated(count + 1, |frame| {
                    let signed = if side == 0 { -(frame as i32) } else { frame as i32 };
                    // The naming callback accepts signed frame indices in Qwilight.
                    let prefix = self.eval.signed_prefix(
                        "_GetMainJudgmentMeter",
                        &[lane.material as i64, i64::from(signed)],
                        "JM",
                    )?;
                    Ok(self
                        .resources
                        .path(&format!("Main/{prefix} {} {signed}", lane.material))
                        .or_else(|| self.resources.path(&format!("Main/{prefix} 0 {signed}"))))
                })?;
            }
            if sides.iter().all(Vec::is_empty) {
                continue;
            }
            for paths in &mut sides {
                if paths.is_empty() {
                    *paths = vec!["bmz://lr2/empty".into(); count + 1];
                }
            }
            sides[0].reverse();
            let mut paths = std::mem::take(&mut sides[0]);
            paths.extend(sides[1].iter().skip(1).cloned());
            let id = format!("qw-lane-timing-{i}");
            if self.image(&id, paths, cycle, None) {
                self.document["image"].as_array_mut().unwrap().last_mut().unwrap()["qwilightAnimation"] = json!({"mode":"laneTiming","count":count+1,"fps":fps as u32,"lane":lane.timer_index});
                self.destination(
                    57,
                    &id,
                    self.element_rect(57, [lane.x, y, lane.width, h]),
                    None,
                    None,
                )["offset"] = json!(OFFSET_LIFT);
            }
        }
        Ok(())
    }

    fn auto_indicators(&mut self) -> Result<()> {
        let (count, cycle) = self.frames("auto-main")?;
        let paths = self.resources.animated(count, |frame| {
            let prefix = self.eval.prefix("_GetAutoMain", &[frame], "AM")?;
            Ok(self.resources.frame(&format!("Drawing/{prefix}"), frame, ""))
        })?;
        let [x, y, w, h] = self.area("autoMain")?;
        let align = self.point("autoMainSystem", 0.0)? as i32;
        let x = x - match align {
            1 => w / 2.0,
            2 => w,
            _ => 0.0,
        };
        if self.image("qw-auto-main", paths, cycle, None) {
            for dx in self.alt_offsets("alt-auto-main", 2)? {
                self.destination(53, "qw-auto-main", self.rect(x + dx, y, w, h), None, None)["op"] =
                    json!([33]);
            }
        }
        for (i, lane) in self.layout.lanes.clone().iter().enumerate() {
            let prefix = self.eval.prefix("_GetAutoInput", &[lane.material], "A")?;
            let id = format!("qw-auto-input-{i}");
            let paths = self
                .resources
                .path(&format!("Main/{prefix} {}", lane.material))
                .or_else(|| self.resources.path(&format!("Main/{prefix} 0")))
                .into_iter()
                .collect();
            if self.image(&id, paths, 0, None) {
                self.document["image"].as_array_mut().unwrap().last_mut().unwrap()["qwilightAnimation"] =
                    json!({"mode":"auto","lane":lane.timer_index});
                let y = self.eval.lane_value("autoInputPosition1", lane.drawing, 0.0)?;
                let h = self.eval.lane_value("autoInputHeight", lane.drawing, 0.0)?;
                self.destination(4, &id, self.rect(lane.x, y, lane.width, h), None, None);
            }
        }
        Ok(())
    }

    fn supplementary_numbers(&mut self) -> Result<()> {
        for (pipeline, name, bin, prefix, reference, unit, unit_width) in [
            (
                34,
                "hitPointsVisualizer",
                "binHitPointsVisualizer",
                "HP",
                107,
                "HP %",
                "hitPointsVisualizerUnitDrawingLength",
            ),
            (46, "hunter", "binHunter", "HU", 153, "HU +", "hunterFrontDrawingLength"),
        ] {
            let w = self.point(&format!("{bin}Length"), self.point("binLength", 0.0)?)?;
            let h = self.point(&format!("{bin}Height"), self.point("binHeight", 0.0)?)?;
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let id = format!("qw-{name}");
            if !self.digits(&id, "", prefix, reference, 10)? {
                continue;
            }
            self.document["value"].as_array_mut().unwrap().last_mut().unwrap()["align"] = json!(1);
            let u = self.point(unit_width, w)?;
            let x = self.point(&format!("{name}Position0"), 0.0)?;
            let y = self.point(&format!("{name}Position1"), 0.0)?;
            let system = self.point(&format!("{name}System"), 0.0)? as i32;
            for negative in [false, true] {
                if negative && pipeline != 46 {
                    continue;
                }
                let number = if negative { format!("{id}-negative") } else { id.clone() };
                if negative {
                    let mut value = self.document["value"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|v| v["id"] == id)
                        .unwrap()
                        .clone();
                    value["id"] = json!(number);
                    value["expr"] = json!(format!("-number({reference})"));
                    self.push("value", value);
                }
                let symbol = format!("{number}-unit");
                let stem = if negative { "HU -" } else { unit };
                let paths = self
                    .resources
                    .path(&format!("Bin/{stem}"))
                    .or_else(|| {
                        self.resources.path(if pipeline == 34 {
                            "Bin/%"
                        } else if negative {
                            "Bin/-"
                        } else {
                            "Bin/+"
                        })
                    })
                    .into_iter()
                    .collect();
                self.image(&symbol, paths, 0, None);
                for digits in 1..=10_u32 {
                    let low = if digits == 1 { 0 } else { 10_i64.pow(digits - 1) };
                    let high = 10_i64.pow(digits);
                    let draw = if negative {
                        format!(
                            "number({reference}) <= -{} and number({reference}) > -{high} and number({reference}) > -2147483648",
                            low.max(1)
                        )
                    } else {
                        format!("number({reference}) >= {low} and number({reference}) < {high}")
                    };
                    let length = w * f64::from(digits) + u;
                    let left = x - match system {
                        1 => length / 2.0,
                        2 => length,
                        _ => 0.0,
                    };
                    for dx in self.alt_offsets(
                        if pipeline == 34 { "alt-hit-points-visualizer" } else { "alt-hunter" },
                        2,
                    )? {
                        self.destination(
                            pipeline,
                            &number,
                            self.group_rect(
                                pipeline,
                                [left + dx, y, length, h],
                                [left + dx + if pipeline == 46 { u } else { 0.0 }, y, w, h],
                            ),
                            None,
                            None,
                        )["draw"] = json!(draw);
                        self.destination(
                            pipeline,
                            &symbol,
                            self.group_rect(
                                pipeline,
                                [left + dx, y, length, h],
                                [
                                    left + dx + if pipeline == 34 { length - u } else { 0.0 },
                                    y,
                                    u,
                                    h,
                                ],
                            ),
                            None,
                            None,
                        )["draw"] = json!(draw);
                    }
                }
            }
        }
        Ok(())
    }
}
