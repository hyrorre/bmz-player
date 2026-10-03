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
        json!({
            "rank_wait": self.header.rank_wait, "update_wait": self.header.update_wait,
            "graph_start": first.map(|c| c["start"].clone()).unwrap_or(json!(0)),
            "graph_end": first.map(|c| c["end"].clone()).unwrap_or(json!(0)),
            "flip": self.result_flip,
        })
    }
}
