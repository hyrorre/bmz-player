use super::*;

impl CsvBuilder<'_> {
    pub(super) fn execute_result_command(&mut self, line: &CsvLine) -> bool {
        if !matches!(self.header.skin_type, 7 | 15) {
            return false;
        }
        match line.command.as_str() {
            "FLIPRESULT" => self.result_flip = true,
            "DISABLEFLIP" => self.result_flip = false,
            "SRC_GAUGECHART_1P" | "SRC_GAUGECHART_2P" | "SRC_SCORECHART" => {
                let values = parse_values(line);
                self.current = None;
                self.add_image(line);
                if let Some(variant) = self.current_primary_variant() {
                    self.result_charts.push(json!({
                        "id": variant.id, "score": line.command == "SRC_SCORECHART",
                        "player": if line.command.ends_with("2P") { 1 } else { 0 },
                        "index": values[1], "width": values[11].clamp(0, 16384),
                        "height": values[12].clamp(0, 16384),
                        "start": values[13].max(0), "end": values[14].max(values[13]),
                    }));
                }
            }
            "DST_GAUGECHART_1P" | "DST_GAUGECHART_2P" | "DST_SCORECHART" => {
                self.add_destination(line)
            }
            _ => return false,
        }
        true
    }

    pub(super) fn finish_lr2_result(&self) -> JsonValue {
        if !matches!(self.header.skin_type, 7 | 15) {
            return JsonValue::Null;
        }
        let first = self
            .result_charts
            .iter()
            .find(|c| c["player"] == 0 && c["index"] == 0 && c["score"] == false);
        // Skins such as W-MIX / ECSE draw nothing on the update timer (152) and
        // leave the Result with the press after the rank stage.
        let update_stage = [&self.destinations, &self.images, &self.values]
            .into_iter()
            .flatten()
            .any(|value| uses_timer(value, 152));
        json!({
            "rank_wait": self.header.rank_wait, "update_wait": self.header.update_wait,
            "graph_start": first.map(|c| c["start"].clone()).unwrap_or(json!(0)),
            "graph_end": first.map(|c| c["end"].clone()).unwrap_or(json!(0)),
            "flip": self.result_flip,
            "update_stage": update_stage,
        })
    }
}

fn uses_timer(value: &JsonValue, timer: i64) -> bool {
    match value {
        JsonValue::Object(map) => map.iter().any(|(key, value)| {
            (key == "timer" && value.as_i64() == Some(timer)) || uses_timer(value, timer)
        }),
        JsonValue::Array(values) => values.iter().any(|value| uses_timer(value, timer)),
        _ => false,
    }
}
