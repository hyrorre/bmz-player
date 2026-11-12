use super::*;

#[derive(Clone)]
pub(super) struct Lane {
    pub drawing: usize,
    pub material: usize,
    pub x: f64,
    pub width: f64,
    pub height: f64,
    pub timer_index: i32,
}

pub(super) struct Layout {
    pub skin_type: i32,
    pub width: f64,
    pub height: f64,
    pub judge_y: f64,
    /// Left edge and width of the 1P lanes (Qwilight `mainPosition` / `P1BuiltLength`).
    pub main_x: f64,
    pub p1_width: f64,
    pub dp_offset: Option<f64>,
    pub lanes: Vec<Lane>, // beatoraja note order, scratch last in each region
}

impl Layout {
    pub fn build(eval: &mut Evaluator, mode: i32) -> Result<Self> {
        let (input_mode, count, scratches, skin_type, fallback): (_, usize, usize, _, &[usize]) =
            match mode {
                4 => (4, 4, 0, 22, &[2, 3, 3, 2]),
                6 => (6, 6, 0, 23, &[2, 3, 2, 2, 3, 2]),
                8 => (8, 8, 0, 24, &[2, 3, 2, 3, 3, 2, 3, 2]),
                9 => (9, 9, 0, 4, &[2, 3, 2, 3, 4, 3, 2, 3, 2]),
                5 => (10, 6, 1, 1, &[1, 2, 3, 4, 3, 2]),
                7 => (11, 8, 1, 0, &[1, 2, 3, 2, 5, 2, 3, 2]),
                10 => (12, 12, 2, 3, &[1, 2, 3, 4, 3, 2, 2, 3, 4, 3, 2, 10]),
                14 => (13, 16, 2, 2, &[1, 2, 3, 2, 5, 2, 3, 2, 2, 3, 2, 5, 2, 3, 2, 10]),
                _ => bail!("unsupported Qwilight mode {mode}"),
            };
        let globals = eval.lua.globals();
        globals.set("inputCount", count)?;
        globals.set("defaultInputCount", count - scratches)?;
        globals.set("autoableInputCount", scratches)?;
        globals.set("has2P", scratches == 2)?;
        let data = eval.list("lambda", &format!("drawingInputMode{input_mode}"))?;
        let mut drawings = if data.is_empty() {
            fallback.to_vec()
        } else {
            data.iter().map(|s| eval.numeric(s).map(|v| v as usize)).collect::<Result<Vec<_>>>()?
        };
        if scratches == 2 && !data.is_empty() {
            match eval.number("lambda", "drawingInputModeSystem", 2.0)? as i32 {
                0 => {
                    let right = drawings
                        .iter()
                        .skip(1)
                        .chain(drawings.first())
                        .copied()
                        .collect::<Vec<_>>();
                    drawings.extend(right);
                }
                1 => {
                    let right = drawings.iter().rev().copied().collect::<Vec<_>>();
                    drawings.extend(right);
                }
                2 => {}
                _ => bail!("invalid Qwilight drawingInputModeSystem"),
            }
        }
        ensure!(
            drawings.len() == count && drawings.iter().all(|v| (1..=64).contains(v)),
            "invalid Qwilight lane map for {mode}K"
        );
        let widths = drawings
            .iter()
            .map(|&v| eval.lane_value("noteLength", v, 50.0))
            .collect::<Result<Vec<_>>>()?;
        ensure!(widths.iter().all(|v| *v > 0.0 && *v <= 4096.0), "invalid Qwilight lane widths");
        let half = if scratches == 2 { count / 2 } else { count };
        let mut cursor = 0.0;
        let mut positions = Vec::new();
        let mut drawing_widths = Vec::new();
        for (&drawing, &width) in drawings.iter().zip(&widths) {
            let floating = eval.lane_value("floatingNoteLength", drawing, 0.0)?;
            if floating > 0.0 {
                positions.push(eval.lane_value("floatingNotePosition0", drawing, 0.0)?);
                drawing_widths.push(floating);
            } else {
                cursor += eval.lane_value("slashNotePosition0", drawing, 0.0)?;
                positions.push(cursor);
                drawing_widths.push(width);
                cursor += width;
            }
        }
        let extent = |n: usize| {
            positions[..n].iter().zip(&drawing_widths).map(|(x, w)| x + w).fold(0.0, f64::max)
        };
        let p1 = extent(half);
        let all = extent(count);
        for (name, width) in [("P1Length", p1), ("P1BuiltLength", p1), ("P2BuiltLength", all)] {
            // Qwilight installs builtins before the skin's Lua. Keep overrides
            // such as CR_minimal's P1Length(), which adds its frame border.
            if !globals.contains_key(name)? {
                globals
                    .set(name, eval.lua.create_function(move |_, scale: f64| Ok(width * scale))?)?;
            }
        }
        let main_x = eval.number("point", "mainPosition", 0.0)?;
        if !globals.contains_key("Wall")? {
            globals.set(
                "Wall",
                eval.lua.create_function(move |_, offset: f64| Ok(main_x + p1 + offset))?,
            )?;
        }
        if !globals.contains_key("Contents")? {
            globals.set(
                "Contents",
                eval.lua.create_function(move |_, scale: f64| Ok(main_x + p1 * scale))?,
            )?;
        }
        let gap = if scratches == 2 { eval.number("point", "p2Position", 0.0)? } else { 0.0 };
        let mut physical = Vec::new();
        for (i, (&drawing, &width)) in drawings.iter().zip(&drawing_widths).enumerate() {
            let floating = eval.lane_value("floatingNoteLength", drawing, 0.0)? > 0.0;
            let x = main_x + positions[i] + if i >= half && !floating { gap } else { 0.0 };
            let mut material = drawing;
            for group in 1..=64 {
                if eval
                    .list("lambda", &format!("drawing{group}"))?
                    .iter()
                    .any(|v| v.parse::<usize>().ok() == Some(drawing))
                {
                    material = group;
                    break;
                }
            }
            let timer_index = if scratches == 0 {
                i as i32 + 1
            } else if i == 0 {
                0
            } else if scratches == 2 && i == count - 1 {
                10
            } else if i >= half {
                10 + (i - half + 1) as i32
            } else {
                i as i32
            };
            physical.push(Lane {
                drawing,
                material,
                x,
                width,
                height: eval.lane_value("noteHeight", drawing, 12.0)?,
                timer_index,
            });
        }
        let order: Vec<usize> = if scratches == 1 {
            (1..count).chain([0]).collect()
        } else if scratches == 2 {
            (1..half).chain([0]).chain(half..count).collect()
        } else {
            (0..count).collect()
        };
        let width = eval.number("format", "defaultLength", 1280.0)?;
        let height = eval.number("format", "defaultHeight", 720.0)?;
        ensure!(
            (1.0..=16384.0).contains(&width) && (1.0..=16384.0).contains(&height),
            "invalid Qwilight canvas"
        );
        let judge_y = eval.number("point", "judgmentMainPosition", height * 0.8)?;
        ensure!(judge_y > 0.0 && judge_y <= height, "invalid Qwilight judgment line");
        Ok(Self {
            skin_type,
            width,
            height,
            judge_y,
            main_x,
            p1_width: p1,
            dp_offset: (scratches == 2).then_some(p1 + gap),
            lanes: order.into_iter().map(|i| physical[i].clone()).collect(),
        })
    }
}
