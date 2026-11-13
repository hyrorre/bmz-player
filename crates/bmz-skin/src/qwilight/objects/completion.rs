use super::*;
use bmz_skin_document::SKIN_TIMER_BMZ_RESULT_SETTLED;

impl Builder {
    pub(super) fn completion(&mut self) -> Result<()> {
        if self.eval.hidden_pipelines.contains(&62) {
            return Ok(());
        }
        for (name, category, timer, condition) in [
            ("band!", 9, 48, "number(110) < number(74)"),
            ("last", 10, SKIN_TIMER_BMZ_RESULT_SETTLED, "timer(48) < 0"),
            ("yell!", 11, 48, "number(74) > 0 and number(110) == number(74)"),
        ] {
            let count = self.eval.number("frame", &format!("{name}-frame"), 1.0)? as usize;
            let fps = self.eval.number("frame", &format!("{name}-framerate"), 60.0)?;
            ensure!(
                (1..=4096).contains(&count) && fps.is_finite() && fps > 0.0 && fps <= 1000.0,
                "invalid Qwilight completion animation: {name}"
            );
            // Ending categories use the fixed J prefix in the native loader.
            let mut paths = self.resources.animated(count, |frame| {
                Ok(self.resources.path(&format!("Paint/J {category} {frame}")))
            })?;
            if category == 11 {
                let fallback = self.resources.animated(count, |frame| {
                    Ok(self.resources.path(&format!("Paint/J 9 {frame}")))
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
            let [x, y, w, h] = self.area(name)?;
            let x = x - match self.point(&format!("{name}System"), 0.0)? as i32 {
                1 => w / 2.0,
                2 => w,
                _ => 0.0,
            };
            if paths.is_empty() || w <= 0.0 || h <= 0.0 {
                continue;
            }
            let mut sources = Vec::with_capacity(paths.len());
            for path in paths {
                let is_deferred = path != "bmz://lr2/empty";
                let source = self.source(vec![path], 1);
                let deferred = self.document["deferredSources"].as_array_mut().unwrap();
                if is_deferred && !deferred.contains(&json!(source)) {
                    deferred.push(json!(source));
                }
                sources.push(source);
            }
            let id = format!("qw-completion-{category}");
            self.push("image", json!({"id":id,"src":sources[0],"w":-1,"h":-1,"timer":timer,
                "qwilightAnimation":{"sources":sources,"mode":if category == 10 {"clear"} else {"once"},"fps":fps.round() as u32,"count":count}}));
            let duration = (count as f64 * 1000.0 / fps).ceil() as i32;
            for dx in self.alt_offsets(&format!("alt-{name}"), 2)? {
                let dest = self.destination(
                    62,
                    &id,
                    self.rect(x + dx, y, w, h),
                    Some(timer),
                    Some(duration),
                );
                dest["draw"] = json!(condition);
                dest["offset"] = json!(OFFSET_LIFT);
            }
        }
        Ok(())
    }
}
