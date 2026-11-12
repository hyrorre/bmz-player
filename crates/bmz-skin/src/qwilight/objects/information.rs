use super::*;

impl Builder {
    pub(super) fn information(&mut self) -> Result<()> {
        for (pipeline, name, reference) in [
            (47, "title", 12),
            (48, "artist", 16),
            (51, "genre", 13),
            (59, "wantLevel", 1002),
            (52, "levelText", 0),
        ] {
            if self.eval.definition.text("point", &format!("{name}Position0")).is_none() {
                continue;
            }
            let [x, y, width, height] = self.area(name)?;
            let size = self.eval.number("font", &format!("{name}Level"), 18.0)?.max(1.0);
            let system_x = self.point(&format!("{name}System0"), 1.0)? as i32;
            let system_y = self.point(&format!("{name}System1"), 0.0)? as i32;
            let align = match system_x {
                0 => 2,
                2 => 1,
                _ => 0,
            };
            // Zero length/height is a point-anchored native text layout, not hidden text.
            let width = if width > 0.0 {
                width
            } else {
                match align {
                    1 => 2.0 * x.min(self.layout.width - x),
                    2 => x,
                    _ => self.layout.width - x,
                }
                .max(1.0)
            };
            let y = y + match system_y {
                1 => height - size,
                2 => (height - size) / 2.0,
                _ => 0.0,
            };
            let id = format!("qw-text-{name}");
            let mut text = json!({"id":id,"ref":reference,"size":size.round() as i32,"align":align,"overflow":1});
            if name == "levelText" {
                text["value_expr"] = json!("bmz:play_level");
            }
            self.push("text", text);
            let variants = if name == "levelText" { 6 } else { 1 };
            for difficulty in 0..variants {
                let paint = match name {
                    "levelText" => format!("level{difficulty}"),
                    "wantLevel" => "wantLevelID".into(),
                    _ => name.into(),
                };
                let color = paint_color(
                    self.eval.definition.text("paint", &paint).as_deref().unwrap_or("White"),
                );
                for dx in self.alt_offsets(&format!("alt-{name}"), 0)? {
                    let mut rect = self.rect(x + dx, y, width, size);
                    for (component, value) in ["r", "g", "b", "a"].into_iter().zip(color) {
                        rect[component] = json!(value);
                    }
                    let destination = self.destination(pipeline, &id, rect, None, None);
                    if variants > 1 {
                        destination["op"] = json!([150 + difficulty]);
                    }
                }
            }
        }
        let (frames, cycle) = self.frames("level")?;
        let [x, y, w, h] = self.area("level")?;
        for difficulty in 0..6 {
            let prefix = self.eval.prefix("_GetLevel", &[difficulty, 0], "L")?;
            let id = format!("qw-level-{difficulty}");
            if w > 0.0
                && h > 0.0
                && self.image(
                    &id,
                    self.resources.sequence(&format!("Level/{prefix} {difficulty}"), frames, ""),
                    cycle,
                    None,
                )
            {
                for dx in self.alt_offsets("alt-level", 0)? {
                    self.destination(60, &id, self.rect(x + dx, y, w, h), None, None)["op"] =
                        json!([150 + difficulty]);
                }
            }
        }
        self.input_count()?;
        self.elapsed_time()?;
        for (pipeline, name, kind, alt) in [
            (1, "audioVisualizer", "spectrum", "alt-audio-visualizer"),
            (43, "judgmentVisualizer", "timing", "alt-judgment-visualizer"),
            (61, "judgmentInputVisualizer", "judgments", "alt-judgment-input-visualizer"),
        ] {
            let [x, y, w, h] = self.area(name)?;
            if w <= 0.0 || h <= 0.0 || self.eval.hidden_pipelines.contains(&pipeline) {
                continue;
            }
            let id = format!("qw-{name}");
            self.push(
                "qwilightVisualizer",
                json!({"id":id,"kind":kind,
                "system":self.point(&format!("{name}System"),3.0)? as i32,
                "lineWidth":self.point(&format!("{name}ContentsLength"),1.0)?.round() as i32,
                "lineHeight":self.point(&format!("{name}ContentsHeight"),h)?.round() as i32}),
            );
            for dx in self.alt_offsets(alt, 0)? {
                let destination =
                    self.destination(pipeline, &id, self.rect(x + dx, y, w, h), None, None);
                if pipeline == 43 {
                    destination["offset"] = json!(OFFSET_LIFT);
                }
            }
        }
        Ok(())
    }

    fn input_count(&mut self) -> Result<()> {
        let w = self.point("binInputVisualizerLength", 0.0)?;
        let h = self.point("binInputVisualizerHeight", 0.0)?;
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }
        let id = "qw-nps";
        if !self.digits(id, "_GetInputVisualizerBin", "IS", 0, 4)? {
            return Ok(());
        }
        let value = self.document["value"].as_array_mut().unwrap().last_mut().unwrap();
        value["value_expr"] = json!("bmz:keylogger_nps");
        let align = self.point("inputVisualizerSystem", 0.0)? as i32;
        let x = self.point("inputVisualizerPosition0", 0.0)?;
        let y = self.point("inputVisualizerPosition1", 0.0)?;
        let value = self.document["value"].as_array_mut().unwrap().last_mut().unwrap();
        value["align"] = json!(match align {
            1 => 2,
            2 => 0,
            _ => 1,
        });
        for dx in self.alt_offsets("alt-input-visualizer", 0)? {
            let left = x + dx
                - match align {
                    1 => 2.0 * w,
                    2 => 4.0 * w,
                    _ => 0.0,
                };
            self.destination(44, id, self.rect(left, y, w, h), None, None);
        }
        Ok(())
    }

    fn elapsed_time(&mut self) -> Result<()> {
        let w = self.point("binHmsLength", 0.0)?;
        let h = self.point("binHmsHeight", 0.0)?;
        if w <= 0.0 || h <= 0.0 {
            return Ok(());
        }
        for (name, reference, digits, padding) in [
            ("minutes", 161, 4, 0),
            ("seconds", 162, 2, 1),
            ("total-minutes", bmz_skin_document::SKIN_REF_BMZ_CHART_TOTAL_MINUTES, 4, 0),
            ("total-seconds", 1164, 2, 1),
        ] {
            if !self.digits(&format!("qw-time-{name}"), "_GetHmsBin", "W", reference, digits)? {
                return Ok(());
            }
            let value = self.document["value"].as_array_mut().unwrap().last_mut().unwrap();
            value["align"] = json!(1);
            value["padding"] = json!(padding);
        }
        for (name, stem, fallback) in [("colon", "..", ":"), ("slash", "Slash", "/")] {
            let id = format!("qw-time-{name}");
            if !self.image(
                &id,
                self.resources.path(&format!("Bin/{stem}")).into_iter().collect(),
                0,
                None,
            ) {
                self.push("text", json!({"id":id,"constantText":fallback,"size":h.round() as i32}));
            }
        }
        let colon = self.point("hmsColonDrawingLength", w)?;
        let slash = self.point("hmsSlashDrawingLength", w)?;
        let x = self.point("hmsPosition0", 0.0)?;
        let y = self.point("hmsPosition1", 0.0)?;
        let align = self.point("hmsSystem", 0.0)? as i32;
        for m in 1..=4_u32 {
            for total in 1..=4_u32 {
                let range = |reference, digits: u32| {
                    format!(
                        "number({reference}) >= {} and number({reference}) < {}",
                        if digits == 1 { 0 } else { 10_i64.pow(digits - 1) },
                        10_i64.pow(digits)
                    )
                };
                let condition = format!(
                    "{} and {}",
                    range(161, m),
                    range(bmz_skin_document::SKIN_REF_BMZ_CHART_TOTAL_MINUTES, total)
                );
                let width = (m + total + 4) as f64 * w + 2.0 * colon + slash;
                for dx in self.alt_offsets("alt-hms", 0)? {
                    let mut left = x + dx
                        - match align {
                            1 => width / 2.0,
                            2 => width,
                            _ => 0.0,
                        };
                    for (name, length, cell) in [
                        ("minutes", m as f64 * w, w),
                        ("colon", colon, colon),
                        ("seconds", 2.0 * w, w),
                        ("slash", slash, slash),
                        ("total-minutes", total as f64 * w, w),
                        ("colon", colon, colon),
                        ("total-seconds", 2.0 * w, w),
                    ] {
                        self.destination(
                            29,
                            &format!("qw-time-{name}"),
                            self.rect(left, y, cell, h),
                            None,
                            None,
                        )["draw"] = json!(condition);
                        left += length;
                    }
                }
            }
        }
        Ok(())
    }
}

fn paint_color(value: &str) -> [u8; 4] {
    let rgb = match value.to_ascii_lowercase().as_str() {
        "black" => 0x000000,
        "gray" | "grey" => 0x808080,
        "red" => 0xff0000,
        "yellow" => 0xffff00,
        "blue" => 0x0000ff,
        "cyan" | "aqua" => 0x00ffff,
        "lime" => 0x00ff00,
        "green" => 0x008000,
        "orange" => 0xffa500,
        "deepskyblue" => 0x00bfff,
        "lightgreen" => 0x90ee90,
        "springgreen" => 0x00ff7f,
        "mediumspringgreen" => 0x00fa9a,
        "mediumturquoise" => 0x48d1cc,
        "mediumpurple" => 0x9370db,
        _ => {
            if let Some(hex) = value.strip_prefix('#') {
                if hex.len() == 8
                    && let Ok(argb) = u32::from_str_radix(hex, 16)
                {
                    return [(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8];
                }
                u32::from_str_radix(hex, 16).unwrap_or(0xffffff)
            } else {
                0xffffff
            }
        }
    };
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]
}
