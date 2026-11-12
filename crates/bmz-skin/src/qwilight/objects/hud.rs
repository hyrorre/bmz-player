use super::*;

impl Builder {
    /// Qwilight's JM -/+ sprites are FAST/SLOW labels. Ref 525 has beatoraja's
    /// opposite sign (FAST positive); BMZ's display filter can hide this ref.
    pub(super) fn judgment_meter(&mut self) -> Result<()> {
        let width = self.point("binJudgmentMeterLength", self.point("binLength", 0.0)?)?;
        let height = self.point("binJudgmentMeterHeight", self.point("binHeight", 0.0)?)?;
        if width <= 0.0 || height <= 0.0 {
            return Ok(());
        }
        let id = "qw-judgment-meter";
        if !self.digits(id, "", "JM", 525, 10)? {
            return Ok(());
        }
        let value = self.document["value"].as_array_mut().unwrap().last_mut().unwrap();
        value["align"] = json!(1);
        let mut slow = value.clone();
        slow["id"] = json!(format!("{id}-slow"));
        slow["expr"] = json!("-number(525)");
        self.push("value", slow);
        for (suffix, preferred, fallback, label) in [
            ("fast-label", "JM -", "-", "FAST"),
            ("slow-label", "JM +", "+", "SLOW"),
            ("unit", "Millis", "Millis", "ms"),
        ] {
            let part = format!("{id}-{suffix}");
            let path = self
                .resources
                .path(&format!("Bin/{preferred}"))
                .or_else(|| self.resources.path(&format!("Bin/{fallback}")));
            if !self.image(&part, path.into_iter().collect(), 0, None) {
                self.push(
                    "text",
                    json!({"id":part,"constantText":label,"size":height.round() as i32}),
                );
            }
        }
        let x = self.point("judgmentMeterPosition0", 0.0)?;
        let y = self.point("judgmentMeterPosition1", 0.0)?;
        let front = self.point("judgmentMeterFrontDrawingLength", width)?;
        let unit = self.point("judgmentMeterUnitDrawingLength", width)?;
        let align = self.point("judgmentMeterSystem", 0.0)? as i32;
        for digits in 1..=10 {
            let lower = if digits == 1 { 0 } else { 10_i64.pow(digits - 1) };
            let upper = 10_i64.pow(digits);
            for (slow, number, label) in [
                (false, id.to_string(), format!("{id}-fast-label")),
                (true, format!("{id}-slow"), format!("{id}-slow-label")),
            ] {
                let condition = if slow {
                    format!(
                        "number(525) <= -{} and number(525) > -{upper} and number(525) > -2147483648",
                        lower.max(1)
                    )
                } else {
                    format!("number(525) >= {lower} and number(525) < {upper}")
                };
                // Qwilight aligns the label and digits together; the unit is outside.
                let left = x - match align {
                    1 => (width * f64::from(digits) + front) / 2.0,
                    2 => width * f64::from(digits) + front,
                    _ => width,
                };
                for dx in self.alt_offsets("alt-judgment-meter", 2)? {
                    for (part, px, w, is_label) in [
                        (label.as_str(), left, front, true),
                        (number.as_str(), left + front, width, false),
                        (
                            &format!("{id}-unit"),
                            left + front + width * f64::from(digits),
                            unit,
                            false,
                        ),
                    ] {
                        if w <= 0.0 {
                            continue;
                        }
                        let rect = self.rect(px + dx, y, w, height);
                        let dest = self.destination(10, part, rect, Some(46), Some(1000));
                        dest["draw"] = json!(if is_label {
                            format!("{condition} and number(525) != 0")
                        } else {
                            condition.clone()
                        });
                        dest["offsets"] = json!([OFFSET_LIFT, OFFSET_JUDGE]);
                        // The same 1 second decay as Qwilight's judgment-meter overlay.
                        dest["dst"][1]["a"] = json!(0);
                    }
                }
            }
        }
        Ok(())
    }
}
