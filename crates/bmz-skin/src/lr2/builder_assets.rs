use super::*;

impl<'a> CsvBuilder<'a> {
    pub(super) fn add_source(&mut self, raw_path: &str) {
        let path = self.resolve_source_path(raw_path);
        let id = format!("{}", self.source_paths.len());
        self.sources.push(json!({ "id": id, "path": path }));
        self.source_paths.push(Some(path));
    }

    pub(super) fn ensure_reference_source(&mut self, source_index: i32) {
        let path = match source_index {
            // beatoraja SkinProperty.IMAGE_BACKBMP / IMAGE_BLACK / IMAGE_WHITE.
            101 => "bmz://lr2/backbmp",
            110 => "bmz://lr2/black",
            111 => "bmz://lr2/white",
            _ => return,
        };
        let id = source_index.to_string();
        if !self.sources.iter().any(|source| {
            source.get("id").and_then(JsonValue::as_str).is_some_and(|existing| existing == id)
        }) {
            self.sources.push(json!({ "id": id, "path": path }));
        }
    }

    pub(super) fn add_system_font(&mut self, line: &CsvLine) {
        let _ = line;
    }

    pub(super) fn add_lr2_font(&mut self, raw_path: &str) {
        let path = self.resolve_lr2_font_path(raw_path);
        let id = format!("lr2font-{}", self.fonts.len());
        self.fonts.push(json!({ "id": id, "path": path, "type": 1 }));
        self.lr2font_ids.push(Some(id));
    }

    pub(super) fn add_image(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-image");
        let mut image = json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
        });
        if line.command == "SRC_BUTTON" {
            image["act"] = json!(values[11]);
            if matches!(self.header.skin_type, 0 | 1 | 2 | 3 | 4 | 12 | 13) {
                match values[11] {
                    40 => image["ref"] = json!(SKIN_REF_BMZ_LR2_GAUGE_TYPE_1P),
                    41 => image["ref"] = json!(SKIN_REF_BMZ_LR2_GAUGE_TYPE_2P),
                    _ => {}
                }
            }
            image["clickable"] = json!(values[12] == 1);
            image["click"] = json!(if values[14] > 0 {
                0
            } else if values[14] < 0 {
                1
            } else {
                2
            });
            image["len"] = json!(values[15]);
        }
        self.images.push(image);
        self.set_current(id);
    }

    pub(super) fn add_imageset_source(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            return;
        };
        self.lr2_imagesets.push(vec![region]);
    }

    pub(super) fn add_imageset(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let count = values[4].max(0) as usize;
        let mut image_ids = Vec::with_capacity(count);
        for index in 0..count {
            let source_index = parse_i32(line.fields.get(5 + index));
            let Some(regions) = self.lr2_imagesets.get(source_index.max(0) as usize).cloned()
            else {
                self.warn(format!("lr2 csv imageset index {source_index} is not defined"));
                continue;
            };
            for region in regions {
                let id = self.alloc_id("lr2-imageset-image");
                self.images.push(json!({
                    "id": id,
                    "src": region.src,
                    "x": region.x,
                    "y": region.y,
                    "w": region.w,
                    "h": region.h,
                    "divx": region.divx,
                    "divy": region.divy,
                    "cycle": values[1],
                    "timer": if values[2] != 0 { json!(values[2]) } else { JsonValue::Null },
                }));
                image_ids.push(id);
            }
        }
        if image_ids.is_empty() {
            self.current = None;
            return;
        }
        let id = self.alloc_id("lr2-imageset");
        self.imagesets.push(json!({ "id": id, "ref": values[3], "images": image_ids }));
        self.set_current(id);
    }

    pub(super) fn add_number(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let cells = region.divx.saturating_mul(region.divy);
        let signed_layout = cells >= 24 && cells % 24 == 0;
        let digit = if signed_layout { values[13].saturating_add(1) } else { values[13] };
        let zeropadding = if signed_layout {
            if line.fields.get(14).is_some_and(|value| !value.is_empty()) { values[14] } else { 2 }
        } else if cells % 10 != 0 {
            2
        } else {
            0
        };
        let ref_id = if matches!(self.header.skin_type, 0 | 1 | 2 | 3 | 4 | 12 | 13) {
            match values[11] {
                10 | 11 => SKIN_REF_BMZ_LR2_HISPEED,
                121 if matches!(self.header.skin_type, 12 | 13) => 271,
                127 => SKIN_REF_BMZ_LR2_GAUGE_2P,
                // Modified LR2 / OpenLR2 FAST/SLOW extension. Keep these aliases
                // conversion-local because beatoraja assigns other meanings to 210/212/214.
                210 => SKIN_REF_BMZ_LR2_FAST_SLOW_1P,
                212 => 423,
                214 => 424,
                ref_id => ref_id,
            }
        } else {
            values[11]
        };
        let id = self.alloc_id("lr2-number");
        self.values.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "ref": ref_id,
            "align": values[12],
            "digit": digit,
            "zeropadding": zeropadding,
            "space": values[15],
        }));
        self.set_current(id);
    }

    pub(super) fn add_text(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let id = self.alloc_id("lr2-text");
        let font = self
            .lr2font_ids
            .get(values[2].max(0) as usize)
            .and_then(|id| id.clone())
            .unwrap_or_default();
        self.texts.push(json!({
            "id": id,
            "font": font,
            "ref": values[3],
            "align": values[4],
            "overflow": 1,
            "size": self.lr2_text_size(values[2]),
        }));
        self.set_current(id);
    }

    pub(super) fn add_slider(&mut self, line: &CsvLine, is_ref_num: bool) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-slider");
        self.sliders.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "angle": values[11],
            "range": values[12],
            "type": values[13],
            "changeable": values[14] == 0,
            "isRefNum": is_ref_num,
            "min": values[15],
            "max": values[16],
        }));
        self.set_current(id);
    }

    pub(super) fn add_graph(&mut self, line: &CsvLine, is_ref_num: bool) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-graph");
        let graph_type = if is_ref_num { values[11] } else { values[11] + 100 };
        self.graphs.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": region.cycle,
            "timer": region.timer,
            "type": graph_type,
            "angle": values[12],
            "isRefNum": is_ref_num,
            "min": values[13],
            "max": values[14],
        }));
        self.set_current(id);
    }

    pub(super) fn add_note_chart(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let id = self.alloc_id("lr2-notechart");
        self.judge_graphs.push(json!({
            "id": id,
            "type": values[1],
            "delay": values[15],
            "backTexOff": values[16],
            "orderReverse": values[17],
            "noGap": values[18],
            "noGapX": values[19],
        }));
        self.special_destination_sizes.insert(id.clone(), (values[11], values[12]));
        self.set_current(id);
    }

    pub(super) fn add_bpm_chart(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let id = self.alloc_id("lr2-bpmchart");
        self.bpm_graphs.push(json!({
            "id": id,
            "delay": values[3],
            "lineWidth": values[4],
            "mainBPMColor": field(line, 5),
            "minBPMColor": field(line, 6),
            "maxBPMColor": field(line, 7),
            "otherBPMColor": field(line, 8),
            "stopLineColor": field(line, 9),
            "transitionLineColor": field(line, 10),
        }));
        self.special_destination_sizes.insert(id.clone(), (values[1], values[2]));
        self.set_current(id);
    }

    pub(super) fn add_timing_visualizer(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let id = self.alloc_id("lr2-timing");
        self.timing_visualizers.push(json!({
            "id": id,
            "width": values[4],
            "judgeWidthMillis": values[6],
            "lineWidth": values[7],
            "lineColor": field(line, 8),
            "centerColor": field(line, 9),
            "PGColor": field(line, 10),
            "GRColor": field(line, 11),
            "GDColor": field(line, 12),
            "BDColor": field(line, 13),
            "PRColor": field(line, 14),
            "transparent": values[15],
            "drawDecay": values[16],
        }));
        self.special_destination_sizes.insert(id.clone(), (values[4], values[5]));
        self.set_current(id);
    }

    pub(super) fn add_gauge(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-gauge");
        let source_cells = (region.divx * region.divy).max(1);
        let cell_ids = (0..source_cells)
            .map(|index| {
                let image_id = format!("{id}-cell-{index}");
                let divx = region.divx.max(1);
                let divy = region.divy.max(1);
                let cell_w = (region.w / divx).max(1);
                let cell_h = (region.h / divy).max(1);
                self.images.push(json!({
                    "id": image_id,
                    "src": region.src,
                    "x": region.x + cell_w * (index % divx),
                    "y": region.y + cell_h * (index / divx),
                    "w": cell_w,
                    "h": cell_h,
                    "divx": 1,
                    "divy": 1,
                    "cycle": region.cycle,
                    "timer": region.timer,
                }));
                image_id
            })
            .collect::<Vec<_>>();
        let default_gauge = values[13] == 0;
        let gauge_type = if default_gauge { 0 } else { values[14] };
        let range = if default_gauge {
            if matches!(self.header.skin_type, 4 | 14) { 0 } else { 3 }
        } else {
            values[15]
        };
        let cycle = if default_gauge { 33 } else { values[16] };
        let nodes = lr2_gauge_nodes(&cell_ids, gauge_type, line.command == "SRC_GROOVEGAUGE_EX");
        self.lr2_gauge_id = Some(id.clone());
        self.lr2_gauge_add_x = values[11];
        self.lr2_gauge_add_y = values[12];
        let gauge = json!({
            "id": id,
            "nodes": nodes,
            "parts": if default_gauge {
                if matches!(self.header.skin_type, 4 | 14) { 24 } else { 50 }
            } else {
                values[13]
            },
            "type": gauge_type,
            "range": range,
            "cycle": cycle,
            "starttime": values[17],
            "endtime": values[18],
        });
        if self.gauge.is_none() {
            self.gauge = Some(gauge.clone());
        }
        self.gauges.push(gauge);
        self.set_current(id);
    }

    pub(super) fn add_bga(&mut self) {
        let id = "bga".to_string();
        self.bga = Some(json!({ "id": id }));
        self.set_current(id);
    }
}
