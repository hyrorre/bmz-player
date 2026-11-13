use super::{customize::LaneCoverChoice, layout::Layout, resources::Resources, *};

mod colors;
mod completion;
mod extras;
mod hud;
mod information;
pub(super) mod offsets;

/// beatoraja's common play offsets that BMZ shows for every play skin.
const OFFSET_LIFT: i32 = 3;
const OFFSET_NOTES: i32 = 30;
const OFFSET_JUDGE: i32 = 32;
/// Qwilight `PaintPipelineID` values used for implicit ordering.
const PIPELINE_NOTE: i32 = 7;
const PIPELINE_JUDGMENT_PAINT: i32 = 15;
const PIPELINE_HIT_NOTE_PAINT: i32 = 45;
const PIPELINE_VEIL: i32 = 50;
/// BMZ SUDDEN+ slider (beatoraja SLIDER_LANECOVER), moving downward.
const SLIDER_LANE_COVER: i32 = 4;

pub(super) struct Builder {
    pub document: Json,
    pub warnings: Vec<String>,
    eval: Evaluator,
    layout: Layout,
    resources: Resources,
    context: SkinPathContext,
    lane_cover: LaneCoverChoice,
    destinations: BTreeMap<i32, Vec<Json>>,
    source_ids: BTreeMap<(Vec<String>, usize), String>,
    /// Unadjusted areas of each element, per DP copy or lane, for decorations
    /// that the skin associates with that element.
    element_bounds: std::cell::RefCell<BTreeMap<i32, Vec<[f64; 4]>>>,
}
impl Builder {
    pub fn new(
        eval: Evaluator,
        layout: Layout,
        resources: Resources,
        context: &SkinPathContext,
        lane_cover: LaneCoverChoice,
    ) -> Result<Self> {
        let lane_covers = customize::lane_cover_names(context);
        let mut document = eval.definition.header(&eval.available, &lane_covers);
        document["w"] = json!(layout.width.round() as u32);
        document["h"] = json!(layout.height.round() as u32);
        document["type"] = json!(layout.skin_type);
        document["deferredSources"] = json!([]);
        Ok(Self {
            document,
            warnings: Vec::new(),
            eval,
            layout,
            resources,
            context: context.clone(),
            lane_cover,
            destinations: BTreeMap::new(),
            source_ids: BTreeMap::new(),
            element_bounds: Default::default(),
        })
    }
    fn push(&mut self, field: &str, value: Json) {
        self.document[field].as_array_mut().unwrap().push(value);
    }
    fn rect(&self, x: f64, y: f64, w: f64, h: f64) -> Json {
        json!({"x":x.round() as i32,"y":(self.layout.height-y-h).round() as i32,"w":w.round() as i32,"h":h.round() as i32})
    }
    fn destination(
        &mut self,
        pipeline: i32,
        id: &str,
        rect: Json,
        timer: Option<i32>,
        duration: Option<i32>,
    ) -> &mut Json {
        let mut frames = vec![rect.clone()];
        let mut d = json!({"id":id,"filter":1,"dst":frames});
        if let Some(timer) = timer {
            d["timer"] = json!(timer);
        }
        if let Some(duration) = duration {
            let mut last = rect;
            last["time"] = json!(duration);
            frames.push(last);
            d["dst"] = json!(frames);
            d["loop"] = json!(-1);
        }
        let destinations = self.destinations.entry(pipeline).or_default();
        destinations.push(d);
        destinations.last_mut().expect("destination was just pushed")
    }
    fn frames(&self, name: &str) -> Result<(usize, i32)> {
        let n = self.eval.number("frame", &format!("{name}-frame"), 1.0)? as usize;
        ensure!((1..=256).contains(&n), "invalid Qwilight frame count: {name}");
        let fps = self.eval.number("frame", &format!("{name}-framerate"), 60.0)?;
        ensure!(fps > 0.0, "invalid Qwilight framerate: {name}");
        Ok((n, (n as f64 * 1000.0 / fps).round().max(1.0) as i32))
    }
    fn source(&mut self, paths: Vec<String>, columns: usize) -> String {
        if let Some(id) = self.source_ids.get(&(paths.clone(), columns)) {
            return id.clone();
        }
        let id = format!("qw-source-{}", self.source_ids.len());
        self.push("source", json!({"id":id,"path":paths[0]}));
        if paths.len() > 1 {
            self.document["sourceFrames"][&id] = json!({"paths":paths,"columns":columns});
        }
        self.source_ids.insert((paths, columns), id.clone());
        id
    }
    fn image(&mut self, id: &str, paths: Vec<String>, cycle: i32, timer: Option<i32>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let n = paths.len();
        // Sparse frames hold the previous image, often for most of an effect
        // (QMAX_Humanoid judgments: 11 images over 120 frames). Store each
        // distinct image once and map logical frames to its cell.
        let (mut cells, mut distinct) = (Vec::with_capacity(n), Vec::<String>::new());
        for path in &paths {
            let cell = distinct.iter().position(|seen| seen == path).unwrap_or_else(|| {
                distinct.push(path.clone());
                distinct.len() - 1
            });
            cells.push(cell);
        }
        let deduplicated = distinct.len() < n;
        let mut paths = if deduplicated { distinct } else { paths };
        // Prime frame counts must not force tall effects into a single column.
        // Preserve logical timing when the final atlas row has unused cells.
        let cols = (paths.len() as f64).sqrt().ceil() as usize;
        let rows = paths.len().div_ceil(cols);
        paths.resize(cols * rows, paths.last().unwrap().clone());
        let src = self.source(paths, cols);
        let animation = (deduplicated || cols * rows != n)
            .then(|| json!({"mode":"loop","count":n,"cycleMs":cycle}));
        let mut image = json!({"id":id,"src":src,"w":-1,"h":-1,"divx":cols,"divy":rows,
            "cycle":cycle,"timer":timer,"qwilightAnimation":animation});
        if deduplicated {
            image["qwilightFrameCells"] = json!(cells);
        }
        self.push("image", image);
        true
    }
    fn point(&self, name: &str, default: f64) -> Result<f64> {
        self.eval.number("point", name, default)
    }
    fn alt_offsets(&self, key: &str, default: i32) -> Result<Vec<f64>> {
        let Some(distance) = self.layout.dp_offset else {
            return Ok(vec![0.0]);
        };
        let alt = self.eval.number("lambda", key, default as f64)? as i32;
        Ok(match alt {
            1 => vec![distance],
            2 => vec![0.0, distance],
            _ => vec![0.0],
        })
    }
    fn area(&self, name: &str) -> Result<[f64; 4]> {
        Ok([
            self.point(&format!("{name}Position0"), 0.0)?,
            self.point(&format!("{name}Position1"), 0.0)?,
            self.point(&format!("{name}Length"), 0.0)?,
            self.point(&format!("{name}Height"), 0.0)?,
        ])
    }
    pub fn build(&mut self) -> Result<()> {
        self.notes()?;
        self.lane_images()?;
        self.decorations()?;
        self.judgments()?;
        self.hud()?;
        self.information()?;
        self.extras()?;
        // Decorations can follow an associated element, so convert them last.
        self.paint_properties()?;
        self.completion()?;
        self.lane_cover()?;
        self.destinations.retain(|id, _| !self.eval.hidden_pipelines.contains(id));
        let pipeline = self
            .eval
            .list("lambda", "pipeline")?
            .iter()
            .map(|v| v.parse::<i32>().context("invalid Qwilight pipeline"))
            .collect::<Result<Vec<_>>>()?;
        let mut destinations = Vec::new();
        // The BGA, bomb and veil layers are implicit in older Qwilight skins.
        if !pipeline.contains(&0) {
            destinations.extend(self.destinations.remove(&0).unwrap_or_default());
        }
        let implicit_hits = !pipeline.contains(&PIPELINE_HIT_NOTE_PAINT);
        let implicit_veil = !pipeline.contains(&PIPELINE_VEIL);
        for id in pipeline {
            destinations.extend(self.destinations.remove(&id).unwrap_or_default());
            if implicit_hits && id == PIPELINE_JUDGMENT_PAINT {
                destinations
                    .extend(self.destinations.remove(&PIPELINE_HIT_NOTE_PAINT).unwrap_or_default());
            }
            if implicit_veil && id == PIPELINE_NOTE {
                destinations.extend(self.destinations.remove(&PIPELINE_VEIL).unwrap_or_default());
            }
        }
        destinations.extend(self.destinations.remove(&62).unwrap_or_default());
        self.document["destination"] = json!(destinations);
        self.warnings.push("Qwilight play compatibility is experimental; online UI and pause-menu assets are not implemented. Scoring and judgments follow the selected BMZ rules.".into());
        Ok(())
    }
    fn notes(&mut self) -> Result<()> {
        let (count, cycle) = self.frames("note")?;
        let mut note = json!({"id":"notes","note":[],"lnstart":[],"lnend":[],"lnbody":[],"lnbodyActive":[],"mine":[],"hidden":[],"size":[],"dst":[],"qwilightGeometry":[]});
        for (i, lane) in self.layout.lanes.clone().iter().enumerate() {
            for (field, kind, status) in [
                ("note", 0, 0),
                ("lnstart", 3, 0),
                ("lnend", 1, 0),
                ("lnbody", 2, 0),
                ("lnbodyActive", 2, 1),
                ("mine", 9, 0),
                ("hidden", 4, 0),
            ] {
                let paths = self.resources.animated(count, |frame| {
                    let prefix =
                        self.eval.prefix("_GetNote", &[lane.material, frame, kind, status], "N")?;
                    let stem = format!("Note/{prefix} {}", lane.material);
                    let path = if (1..=3).contains(&kind) {
                        self.resources
                            .frame(&stem, frame, &format!(" {kind} {status}"))
                            .or_else(|| self.resources.frame(&stem, frame, &format!(" {kind} 0")))
                    } else {
                        None
                    };
                    Ok(path
                        .or_else(|| self.resources.frame(&stem, frame, &format!(" {kind}")))
                        .or_else(|| {
                            self.resources.frame(
                                &format!("Note/{prefix} 0"),
                                frame,
                                &format!(" {kind}"),
                            )
                        })
                        .or_else(|| {
                            if (1..=3).contains(&kind) {
                                self.resources.frame(&stem, frame, " 0")
                            } else {
                                None
                            }
                        }))
                })?;
                ensure!(
                    !paths.is_empty() || field != "note",
                    "missing Qwilight note image for lane {}",
                    i + 1
                );
                let id = format!("qw-{field}-{i}");
                self.image(&id, paths, cycle, Some(41));
                note[field].as_array_mut().unwrap().push(json!(id));
            }
            note["size"].as_array_mut().unwrap().push(json!(lane.height.round().max(1.0) as i32));
            note["dst"].as_array_mut().unwrap().push(self.rect(
                lane.x,
                0.0,
                lane.width,
                self.layout.judge_y,
            ));
            note["qwilightGeometry"].as_array_mut().unwrap().push(json!({
                "frontHeight":self.eval.lane_value("longNoteFrontEdgeHeight",lane.drawing,lane.height)?,
                "frontOffset":self.eval.lane_value("longNoteFrontEdgePosition",lane.drawing,-lane.height)?,
                "tailHeight":self.eval.lane_value("longNoteTailEdgeHeight",lane.drawing,lane.height)?,
                "tailOffset":self.eval.lane_value("longNoteTailEdgePosition",lane.drawing,0.0)?,
                "frontBodyExtension":self.eval.lane_value("longNoteFrontContentsHeight",lane.drawing,0.0)?,
                "tailBodyExtension":self.eval.lane_value("longNoteTailContentsHeight",lane.drawing,0.0)?,
                "maintainFront":self.eval.definition.text("lambda","maintainLongNoteFrontEdge").is_some_and(|s|s.eq_ignore_ascii_case("true"))
            }));
        }
        self.document["note"] = note;
        self.destinations.insert(PIPELINE_NOTE, vec![json!({"id":"notes","offset":OFFSET_NOTES})]);
        Ok(())
    }
    fn lane_images(&mut self) -> Result<()> {
        for (i, lane) in self.layout.lanes.clone().iter().enumerate() {
            for (pipeline, folder, prefix_fn, prefix, frame, position, height, timer) in [
                (3, "Main", "_GetMain", "M", "main", "mainPosition1", "mainHeight", None),
                (
                    31,
                    "Main",
                    "_GetJudgmentMain",
                    "J",
                    "judgment-main",
                    "judgmentMainPosition1",
                    "judgmentMainHeight",
                    None,
                ),
                (8, "Input", "_GetInput", "I", "input", "inputPosition1", "inputHeight", None),
            ] {
                let (count, cycle) = self.frames(frame)?;
                let paths = self.resources.animated(count + 1, |frame| {
                    let prefix = self.eval.prefix(prefix_fn, &[lane.material, frame], prefix)?;
                    Ok(self.resources.frame(
                        &format!("{folder}/{prefix} {}", lane.material),
                        frame,
                        "",
                    ))
                })?;
                let id = format!("qw-lane-{pipeline}-{i}");
                if self.image(&id, paths, cycle, timer) {
                    let fps = self.eval.number("frame", &format!("{frame}-framerate"), 60.0)?;
                    let looping = self.eval.number(
                        "lambda",
                        if pipeline == 3 { "loopingMain" } else { "loopingInput" },
                        0.0,
                    )?;
                    self.document["image"].as_array_mut().unwrap().last_mut().unwrap()["qwilightAnimation"] = json!({"mode":if pipeline==31 {"beat"} else {"key"},"count":count+1,"fps":fps as u32,"lane":lane.timer_index,"looping":looping as i32});
                    let y = self.eval.lane_value(
                        position,
                        lane.drawing,
                        if pipeline == 31 { self.layout.judge_y } else { 0.0 },
                    )?;
                    let h = self.eval.lane_value(height, lane.drawing, self.layout.judge_y)?;
                    let extra = if pipeline == 8 {
                        self.eval.lane_value("inputLength", lane.drawing, 0.0)?
                    } else {
                        0.0
                    };
                    let dx = if pipeline == 8 {
                        self.eval.lane_value("inputPosition0", lane.drawing, 0.0)?
                    } else {
                        0.0
                    };
                    let rect = self.element_rect(
                        pipeline,
                        [lane.x + dx - extra / 2.0, y, lane.width + extra, h],
                    );
                    let dest = self.destination(pipeline, &id, rect.clone(), timer, None);
                    if matches!(pipeline, 8 | 31) {
                        dest["offset"] = json!(OFFSET_LIFT);
                    }
                }
            }
            for (long, prefix_fn, default, frame, timer) in [
                (false, "_GetHitNotePaint", "N", "hit-note", 50 + lane.timer_index),
                (true, "_GetHitLongNotePaint", "L", "hit-long-note", 70 + lane.timer_index),
            ] {
                let (count, cycle) = self.frames(frame)?;
                let paths = self.resources.animated(count, |frame| {
                    let prefix = self.eval.prefix(prefix_fn, &[lane.material, frame], default)?;
                    Ok(self
                        .resources
                        .frame(&format!("Paint/{prefix} {}", lane.material), frame, "")
                        .or_else(|| self.resources.frame(&format!("Paint/{prefix}"), frame, "")))
                })?;
                let id = format!("qw-hit-{long}-{i}");
                if self.image(&id, paths, cycle, Some(timer)) {
                    if long {
                        let loop_frame = self
                            .eval
                            .number("frame", "hit-long-note-loop-frame", count as f64)?
                            .max(1.0);
                        let fps = self.eval.number("frame", "hit-long-note-framerate", 60.0)?;
                        self.document["image"].as_array_mut().unwrap().last_mut().unwrap()["qwilightAnimation"] = json!({"mode":"long","count":count,"fps":fps as u32,"loopFrame":loop_frame as usize,"lane":lane.timer_index});
                    }
                    let key = if long { "hitLongNotePaint" } else { "hitNotePaint" };
                    let dx = self.eval.lane_value(&format!("{key}Position0"), lane.drawing, 0.0)?;
                    let dy = self.eval.lane_value(&format!("{key}Position1"), lane.drawing, 0.0)?;
                    let w =
                        self.eval.lane_value(&format!("{key}Length"), lane.drawing, lane.width)?;
                    let h =
                        self.eval.lane_value(&format!("{key}Height"), lane.drawing, lane.width)?;
                    self.destination(
                        45,
                        &id,
                        self.element_rect(
                            45,
                            [lane.x + lane.width / 2.0 + dx, self.layout.judge_y + dy, w, h],
                        ),
                        Some(timer),
                        (!long).then_some(cycle),
                    )["offset"] = json!(OFFSET_LIFT);
                }
            }
        }
        Ok(())
    }
    /// `paintProperty0..255`: skin decorations, drawn in their pipeline order.
    fn paint_properties(&mut self) -> Result<()> {
        for index in 0..256 {
            let values = self.eval.list("point", &format!("paintProperty{index}"))?;
            if values.len() < 5 {
                continue;
            }
            // `I`: the element this decoration belongs to. Hiding the element
            // hides it, and its element offset moves it as well.
            let hint =
                values.get(8).map(|hint| self.eval.numeric(hint)).transpose()?.map(|v| v as i32);
            if hint.is_some_and(|hint| self.eval.hidden_pipelines.contains(&hint)) {
                continue;
            }
            let mut v =
                values.iter().take(8).map(|v| self.eval.numeric(v)).collect::<Result<Vec<_>>>()?;
            for default in [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0].iter().skip(v.len()) {
                v.push(*default);
            }
            if v[2] == 0.0 || v[3] == 0.0 {
                continue;
            }
            let mode = v[7] as i32;
            let composite =
                values.get(9).map(|value| self.eval.numeric(value)).transpose()?.unwrap_or(0.0)
                    as i32;
            ensure!((0..=12).contains(&composite), "invalid Qwilight composition mode");
            if !(0..=2).contains(&mode) {
                self.warnings
                    .push(format!("Qwilight paintProperty{index} mode {mode} is not implemented"));
                continue;
            }
            let count = (v[5] as usize).max(1) + usize::from(mode == 1);
            ensure!(count <= 256, "too many paintProperty frames");
            let paths = self.resources.animated(count, |frame| {
                let prefix = self.eval.prefix("_GetPaintProperty", &[index, frame], "P")?;
                Ok(self.resources.frame(&format!("Drawing/{prefix} {index}"), frame, ""))
            })?;
            let id = format!("qw-paint-{index}");
            let cycle = (count as f64 * 1000.0 / v[6].max(1.0)).round() as i32;
            // Mode 0 loops, mode 1 decays to frame zero after each beat,
            // mode 2 plays once and holds its final frame (often a static frame).
            let variants = if mode == 2 {
                paths
                    .into_iter()
                    .enumerate()
                    .map(|(i, path)| {
                        let start = (i as f64 * 1000.0 / v[6].max(1.0)).round() as i32;
                        let end = ((i + 1) as f64 * 1000.0 / v[6].max(1.0)).round() as i32;
                        let draw = if count == 1 {
                            String::new()
                        } else if i == 0 {
                            format!("timer(41) < {end}")
                        } else if i + 1 == count {
                            format!("timer(41) >= {start}")
                        } else {
                            format!("timer(41) >= {start} and timer(41) < {end}")
                        };
                        (format!("{id}-{i}"), vec![path], 0, draw)
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![(id, paths, cycle, String::new())]
            };
            for (id, paths, cycle, draw) in variants {
                if !self.image(&id, paths, cycle, Some(41)) {
                    continue;
                }
                if mode == 1 {
                    self.document["image"].as_array_mut().unwrap().last_mut().unwrap()["qwilightAnimation"] =
                        json!({"mode":"beat","count":count,"fps":v[6].max(1.0) as u32});
                }
                let offsets = match (self.layout.dp_offset, v[4] as i32) {
                    (Some(dx), 1) => vec![dx],
                    (Some(dx), 2) => vec![0.0, dx],
                    _ => vec![0.0],
                };
                for dx in offsets {
                    let follows_line = values.get(8).is_some_and(|v| v.trim() == "31")
                        || (v[1] - self.layout.judge_y).abs() < f64::EPSILON;
                    let rect = self.decoration_rect(index, hint, [v[0] + dx, v[1], v[2], v[3]]);
                    let destination = self.destination(100 + index as i32, &id, rect, None, None);
                    destination["draw"] = json!(draw);
                    destination["blend"] = json!(100 + composite);
                    // Decorations anchored on the judgement line (including
                    // older skins' two-pixel line without a pipeline hint)
                    // follow the line, as do explicit JudgmentMain decorations.
                    if follows_line {
                        destination["offset"] = json!(OFFSET_LIFT);
                    }
                }
            }
        }
        Ok(())
    }
    fn decorations(&mut self) -> Result<()> {
        let [x, y, w, h] = self.area("media")?;
        self.document["bga"] = json!({"id":"qw-bga"});
        for dx in self.alt_offsets("alt-media", 0)? {
            self.destination(0, "qw-bga", self.element_rect(0, [x + dx, y, w, h]), None, None);
        }
        for side in 0..2 {
            let name = format!("mainWall{side}");
            let y = self.point(&format!("{name}Position1"), 0.0)?;
            let w = self.point(&format!("{name}Length"), 0.0)?;
            let h = self.point(&format!("{name}Height"), 0.0)?;
            let x = self.layout.main_x + if side == 0 { -w } else { self.layout.p1_width };
            let prefix = self.eval.prefix("_GetWall", &[side, 0], "W")?;
            let id = format!("qw-wall-{side}");
            if self.image(
                &id,
                self.resources.path(&format!("Main/{prefix} {side}")).into_iter().collect(),
                0,
                None,
            ) {
                for dx in self.alt_offsets(&format!("alt-wall-{side}"), 2)? {
                    self.destination(2, &id, self.rect(x + dx, y, w, h), None, None);
                }
            }
        }
        Ok(())
    }
    fn digits(
        &mut self,
        id: &str,
        prefix_fn: &str,
        default: &str,
        ref_id: i32,
        digits: i32,
    ) -> Result<bool> {
        let (count, cycle) = if ref_id == 104 { self.frames("band")? } else { (1, 0) };
        let mut paths = Vec::new();
        let mut previous: Vec<Option<String>> = vec![None; 10];
        for frame in 0..count {
            for (digit, previous) in previous.iter_mut().enumerate() {
                let prefix = self.eval.prefix(prefix_fn, &[digit, frame], default)?;
                let path = self
                    .resources
                    .path(&format!("Bin/{prefix} {digit} {frame}"))
                    .or_else(|| previous.clone())
                    .or_else(|| self.resources.path(&format!("Bin/{prefix} {digit}")))
                    .or_else(|| self.resources.path(&format!("Bin/{digit}")));
                let Some(path) = path else { return Ok(false) };
                *previous = Some(path.clone());
                paths.push(path);
            }
        }
        let src = self.source(paths, 10);
        let mut value = json!({"id":id,"src":src,"w":-1,"h":-1,"divx":10,"divy":count,"digit":digits,"ref":ref_id,"align":2,"padding":0});
        if ref_id == 104 {
            value["timer"] = json!(46);
            value["cycle"] = json!(cycle);
            let loop_ms = self.eval.number("frame", "last-enlarged-band-loop-frame", 0.0)? * 1000.0
                / self.eval.number("frame", "hit-long-note-framerate", 60.0)?.max(1.0);
            value["qwilightAnimation"] = json!({"mode":"reverse","count":count,"fps":self.eval.number("frame","band-framerate",60.0)? as u32,"enlarge":self.point("enlargeBand",0.0)?.round() as i32,"looping":loop_ms.round() as i32});
        }
        self.push("value", value);
        Ok(true)
    }
    fn judgments(&mut self) -> Result<()> {
        let (count, cycle) = self.frames("judgment-paint")?;
        let [mut x, y, w, h] = self.area("judgmentPaint")?;
        let align = self.point("judgmentPaintSystem", 1.0)? as i32;
        if align == 1 {
            x -= w / 2.0;
        } else if align == 2 {
            x -= w;
        }
        let mut images = Vec::new();
        for (bmz, qw) in [0, 1, 2, 3, 4, 5, 0].into_iter().enumerate() {
            let category = ["highest", "higher", "high", "low", "lower", "lowest"][qw];
            let repeats =
                self.eval.number("frame", &format!("{category}-judgment-paint-frame-loop"), 1.0)?;
            ensure!((0.0..=1000.0).contains(&repeats), "invalid Qwilight judgment repeat count");
            let fps = self.eval.number("frame", "judgment-paint-framerate", 60.0)?;
            let duration = (count as f64 * 1000.0 * repeats / fps).round() as i32;
            let id = format!("qw-judge-{bmz}");
            let paths = self.resources.animated(count, |frame| {
                let prefix = self.eval.prefix("_GetJudgmentPaint", &[qw, frame], "J")?;
                Ok(self.resources.frame(&format!("Paint/{prefix} {qw}"), frame, ""))
            })?;
            self.image(&id, paths, cycle, None);
            let first = self.element_rect(15, [x, y, w, h]);
            let mut last = first.clone();
            last["time"] = json!(duration);
            images.push(json!({"id":id,"loop":-1,"filter":1,"offsets":[OFFSET_LIFT,OFFSET_JUDGE],
                "dst":[first,last]}));
        }
        self.push(
            "judge",
            json!({"id":"qw-judge","index":0,"images":images,"numbers":[],"shift":false}),
        );
        self.destinations
            .insert(15, vec![json!({"id":"qw-judge","dst":[{"x":0,"y":0,"w":0,"h":0}]})]);
        Ok(())
    }
    fn hud(&mut self) -> Result<()> {
        let count_bounds = self.judgment_count_bounds()?;
        for (pipeline, name, bin, prefix_fn, prefix, reference, digits) in [
            (9, "band", "binBand", "_GetBandBin", "C", 104, 5),
            (11, "stand", "binStand", "_GetStandBin", "S", 71, 6),
            (13, "bpm", "binBPM", "_GetBPMBin", "B", 160, 4),
            (41, "highestBand", "binHighestBand", "_GetHighestBandBin", "HC", 105, 5),
            (54, "earlyValue", "binEarlyValue", "", "E", 423, 6),
            (55, "lateValue", "binLateValue", "", "L", 424, 6),
            (
                35,
                "highestJudgmentValue",
                "binHighestJudgmentValue",
                "_GetHighestJudgmentValueBin",
                "J 0",
                110,
                5,
            ),
            (
                36,
                "higherJudgmentValue",
                "binHigherJudgmentValue",
                "_GetHigherJudgmentValueBin",
                "J 1",
                111,
                5,
            ),
            (
                37,
                "highJudgmentValue",
                "binHighJudgmentValue",
                "_GetHighJudgmentValueBin",
                "J 2",
                112,
                5,
            ),
            (
                38,
                "lowJudgmentValue",
                "binLowJudgmentValue",
                "_GetLowJudgmentValueBin",
                "J 3",
                113,
                5,
            ),
            (
                39,
                "lowerJudgmentValue",
                "binLowerJudgmentValue",
                "_GetLowerJudgmentValueBin",
                "J 4",
                114,
                5,
            ),
            (
                40,
                "lowestJudgmentValue",
                "binLowestJudgmentValue",
                "_GetLowestJudgmentValueBin",
                "J 5",
                420,
                5,
            ),
        ] {
            let id = format!("qw-number-{name}");
            let width = self.point(&format!("{bin}Length"), self.point("binLength", 0.0)?)?;
            let height = self.point(&format!("{bin}Height"), self.point("binHeight", 0.0)?)?;
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            if self.digits(&id, prefix_fn, prefix, reference, digits)? {
                let x = self.point(&format!("{name}Position0"), 0.0)?;
                let y = self.point(&format!("{name}Position1"), 0.0)?;
                let align = self.point(&format!("{name}System"), 0.0)? as i32;
                self.document["value"].as_array_mut().unwrap().last_mut().unwrap()["align"] =
                    json!(match align {
                        1 => 2,
                        2 => 0,
                        _ => 1,
                    });
                let x = x - match align {
                    1 => width * digits as f64 / 2.0,
                    2 => width * digits as f64,
                    _ => 0.0,
                };
                let alt = match name {
                    "band" => "alt-band",
                    "highestBand" => "alt-highest-band",
                    "earlyValue" => "alt-early-value",
                    "lateValue" => "alt-late-value",
                    _ => "alt-unused",
                };
                for dx in self.alt_offsets(alt, if name == "band" { 2 } else { 0 })? {
                    let bounds = if (35..=40).contains(&pipeline) {
                        count_bounds
                    } else {
                        [x + dx, y, width * f64::from(digits), height]
                    };
                    let rect = self.group_rect(pipeline, bounds, [x + dx, y, width, height]);
                    let destination = self.destination(pipeline, &id, rect, None, None);
                    // beatoraja's Judge offset moves the combo with the judgment.
                    if name == "band" {
                        destination["offsets"] = json!([OFFSET_LIFT, OFFSET_JUDGE]);
                        destination["draw"] = json!("number(104) > 0");
                    }
                }
            }
        }
        self.fractional_hud()?;
        self.judgment_meter()?;
        Ok(())
    }

    /// Qwilight's veil: the lane cover drawn over the 1P (and 2P) lanes with the
    /// lane width and the image's aspect ratio, its bottom edge at the cover
    /// height. BMZ moves it with SUDDEN+ instead of Qwilight's own cover height.
    fn lane_cover(&mut self) -> Result<()> {
        let path = match self.lane_cover.user_image(&self.context, &mut self.warnings) {
            Some(path) => Some(path),
            None => self.skin_lane_cover()?,
        };
        let Some(path) = path else { return Ok(()) };
        let size = bmz_skin_assets::read(Path::new(&path))
            .ok()
            .and_then(|bytes| customize::image_size(&bytes));
        let Some((image_w, image_h)) = size else {
            self.warnings.push(format!("unreadable Qwilight lane cover image: {path}"));
            return Ok(());
        };
        let width = self.layout.p1_width;
        let height = width * f64::from(image_h) / f64::from(image_w);
        if width <= 0.0 || height <= 0.0 {
            return Ok(());
        }
        let src = self.source(vec![path], 1);
        self.push(
            "slider",
            json!({"id":"qw-lane-cover","src":src,"x":0,"y":0,"w":image_w,"h":image_h,
                "angle":2,"range":self.layout.judge_y.round() as i32,
                "type":SLIDER_LANE_COVER,"changeable":false}),
        );
        let offsets = match self.layout.dp_offset {
            Some(distance) => vec![0.0, distance],
            None => vec![0.0],
        };
        for dx in offsets {
            let rect = self.rect(self.layout.main_x + dx, -height, width, height);
            self.destination(PIPELINE_VEIL, "qw-lane-cover", rect, None, None);
        }
        Ok(())
    }

    /// `_GetVeil()` names a file in `Drawing/`, or lists several as `[a, b]`;
    /// Qwilight then picks one for each play.
    fn skin_lane_cover(&mut self) -> Result<Option<String>> {
        let names = self.eval.prefix("_GetVeil", &[], "Veil")?;
        let names = names.trim();
        let names: Vec<&str> = match names.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            Some(list) => list.split(',').map(str::trim).collect(),
            None => vec![names],
        };
        let paths: Vec<String> = names
            .iter()
            .filter(|name| !name.is_empty())
            .filter_map(|name| {
                let stem = if customize::is_qwilight_lane_cover_image_path(Path::new(name)) {
                    name.rsplit_once('.').map_or(*name, |(stem, _)| stem)
                } else {
                    name
                };
                self.resources.path(&format!("Drawing/{stem}"))
            })
            .collect();
        if paths.is_empty() {
            return Ok(None);
        }
        let index = crate::random_file::random_file_index(paths.len())?;
        Ok(paths.into_iter().nth(index))
    }

    fn fractional_hud(&mut self) -> Result<()> {
        // Conditional layouts cover integer widths without a runtime VM.
        // HS and EX rate need up to three digits; the judgment ratio can grow
        // with the count of PGREATs and uses a separate infinity destination.
        for (pipeline, name, bin, prefix, whole_ref, fraction_ref, unit, fallback_unit) in [
            (14, "multiplier", "binMultiplier", "M", 310, 311, "MW", "X"),
            (12, "point", "binPoint", "P", 102, 103, "P %", "%"),
            (
                33,
                "audioMultiplier",
                "binAudioMultiplier",
                "A",
                bmz_skin_document::SKIN_REF_BMZ_PLAYBACK_INTEGER,
                bmz_skin_document::SKIN_REF_BMZ_PLAYBACK_FRACTION,
                "AW",
                "X",
            ),
            (
                56,
                "judgmentVSVisualizer",
                "binJudgmentVSVisualizer",
                "HH",
                bmz_skin_document::SKIN_REF_BMZ_JUDGMENT_RATIO_INTEGER,
                bmz_skin_document::SKIN_REF_BMZ_JUDGMENT_RATIO_FRACTION,
                "",
                "",
            ),
        ] {
            let width = self.point(&format!("{bin}Length"), self.point("binLength", 0.0)?)?.round();
            let height =
                self.point(&format!("{bin}Height"), self.point("binHeight", 0.0)?)?.round();
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            let id = format!("qw-number-{name}");
            let prefix_fn = match name {
                "point" => "_GetPointBin",
                "multiplier" => "_GetMultiplierBin",
                _ => "",
            };
            let max_digits = if name == "judgmentVSVisualizer" { 10 } else { 3 };
            if !self.digits(&id, prefix_fn, prefix, whole_ref, max_digits)? {
                continue;
            }
            let whole = self.document["value"].as_array_mut().unwrap().last_mut().unwrap();
            whole["align"] = json!(1);
            let mut fraction = whole.clone();
            let fraction_id = format!("{id}-fraction");
            fraction["id"] = json!(fraction_id);
            fraction["ref"] = json!(fraction_ref);
            fraction["digit"] = json!(2);
            fraction["padding"] = json!(1);
            self.push("value", fraction);

            let dot_id = format!("{id}-dot");
            let dot = self
                .resources
                .path(&format!("Bin/{prefix} ."))
                .or_else(|| self.resources.path("Bin/."));
            let has_dot = self.image(&dot_id, dot.into_iter().collect(), 0, None);
            let unit_id = format!("{id}-unit");
            let unit_path = self
                .resources
                .path(&format!("Bin/{unit}"))
                .or_else(|| self.resources.path(&format!("Bin/{fallback_unit}")));
            let has_unit = self.image(&unit_id, unit_path.into_iter().collect(), 0, None);
            let dot_width = self
                .point(
                    &format!("{name}StopPointDrawingLength"),
                    self.point("stopPointDrawingLength", width)?,
                )?
                .round()
                .max(0.0);
            let unit_width = if unit.is_empty() {
                0.0
            } else {
                self.point(&format!("{name}UnitDrawingLength"), width)?.round().max(0.0)
            };
            let x = self.point(&format!("{name}Position0"), 0.0)?;
            let y = self.point(&format!("{name}Position1"), 0.0)?;
            let align = self.point(&format!("{name}System"), 0.0)? as i32;
            let alt = if name == "judgmentVSVisualizer" {
                "alt-judgment-vs-visualizer".into()
            } else {
                format!("alt-{name}")
            };
            let offsets = self.alt_offsets(&alt, 2)?;
            let suffix = name == "point";
            for digits in 1..=max_digits as u32 {
                let lower = if digits == 1 { 0 } else { 10_i64.pow(digits - 1) };
                let condition = if digits == max_digits as u32 {
                    format!("number({whole_ref}) >= {lower}")
                } else {
                    format!(
                        "number({whole_ref}) >= {lower} and number({whole_ref}) < {}",
                        10_i64.pow(digits)
                    )
                };
                let number_width = (f64::from(digits) + 2.0) * width + dot_width;
                // Qwilight aligns percentage with its suffix, but HS aligns
                // just the number and places the unit to its left.
                let total_width = number_width + if suffix { unit_width } else { 0.0 };
                let left = x - match align {
                    1 => total_width / 2.0,
                    2 => total_width,
                    _ => 0.0,
                };
                for dx in &offsets {
                    let left = left + dx;
                    for (part, px, w, visible) in [
                        (&id, left, width, true),
                        (&dot_id, left + f64::from(digits) * width, dot_width, has_dot),
                        (&fraction_id, left + f64::from(digits) * width + dot_width, width, true),
                        (
                            &unit_id,
                            if suffix { left + number_width } else { left - unit_width },
                            unit_width,
                            has_unit,
                        ),
                    ] {
                        if visible && w > 0.0 {
                            let destination = json!({"id":part,"filter":1,"draw":condition,
                                "dst":[self.group_rect(pipeline,
                                    [left - if suffix {0.0} else {unit_width}, y, number_width + unit_width, height],
                                    [px,y,w,height])]});
                            self.destinations.entry(pipeline).or_default().push(destination);
                        }
                    }
                }
            }
            if name == "judgmentVSVisualizer" {
                let id = format!("{id}-infinity");
                let path = self
                    .resources
                    .path("Bin/HH Infinity")
                    .or_else(|| self.resources.path("Bin/Infinity"));
                let infinity_width =
                    self.point("judgmentVSVisualizerInfinityDrawingLength", width * 2.0)?;
                if !self.image(&id, path.into_iter().collect(), 0, None) {
                    self.push(
                        "text",
                        json!({"id":id,"constantText":"∞","size":height.round() as i32,
                        "qwOffsetSize":self.metric(pipeline,height,None,false)}),
                    );
                }
                for dx in offsets {
                    let x = x + dx
                        - match align {
                            1 => infinity_width / 2.0,
                            2 => infinity_width,
                            _ => 0.0,
                        };
                    self.destination(
                        pipeline,
                        &id,
                        self.element_rect(pipeline, [x, y, infinity_width, height]),
                        None,
                        None,
                    )["draw"] = json!(format!("number({whole_ref}) < 0"));
                }
            }
        }
        Ok(())
    }
}
