use super::*;

impl<'a> CsvBuilder<'a> {
    pub(super) fn complete_open_lr2_note_adjustment_effects(&mut self) {
        if !self.header.explicit_resolution_dimensions {
            return;
        }

        for destination in &mut self.destinations {
            let offset = destination.get("offset").and_then(JsonValue::as_i64).unwrap_or(0);
            if !open_lr2_effect_follows_note_adjustment(destination)
                // LR2 treats DST fields 21/22 as opt4/opt5. opt4=1/2 rotates the
                // 1P/2P scratch image, so keep that value while adding the lift offset.
                || !matches!(offset, 0..=2)
                || destination
                    .get("offsets")
                    .and_then(JsonValue::as_array)
                    .is_some_and(|offsets| !offsets.is_empty())
            {
                continue;
            }
            destination["offsets"] = json!([LR2_OFFSET_LIFT]);
        }
    }

    pub(super) fn add_note_source(&mut self, line: &CsvLine, slot: NoteSlot) {
        self.add_note_source_with_animation(line, slot, true);
    }

    pub(super) fn add_note_source_with_animation(
        &mut self,
        line: &CsvLine,
        slot: NoteSlot,
        animate: bool,
    ) {
        let values = parse_values(line);
        let Some(lane) = self.lr2_lane_to_beatoraja_index(values[1]) else {
            return;
        };
        let Some(region) = self.source_region(&values) else {
            return;
        };
        let id = self.alloc_id("lr2-note");
        self.images.push(json!({
            "id": id,
            "src": region.src,
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
            "divx": region.divx,
            "divy": region.divy,
            "cycle": if animate { region.cycle } else { 0 },
            "timer": if animate { region.timer } else { None },
        }));
        let note = if line.command.starts_with("SRC_AUTO_") {
            self.note.auto.get_or_insert_default().as_mut()
        } else {
            &mut self.note
        };
        set_lane_note_value_if_empty(note_vec_mut(note, slot), lane, id);
    }

    pub(super) fn add_note_destination(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(lane) = self.lr2_lane_to_beatoraja_index(values[1]) else {
            return;
        };
        let destination = self.destination_def_with_ops("notes", &values, &self.conditional_ops);
        self.note
            .destinations
            .resize_with(self.note.destinations.len().max(lane as usize + 1), || None);
        if let Some(previous) = &mut self.note.destinations[lane as usize] {
            merge_destination_entry(previous, destination);
        } else {
            self.note.destinations[lane as usize] = Some(destination);
        }
        if !self.note_marker_inserted {
            self.destinations.push(json!({ "id": "notes" }));
            self.note_marker_inserted = true;
        }
        while self.note.dst.len() < lane as usize {
            self.note.dst.push(json!({ "time": 0, "x": 0, "y": 0, "w": 0, "h": 0 }));
        }
        if self.note.dst.len() == lane as usize {
            let frame = note_destination_frame(&values, self.header.h as i32);
            set_lane_note_size_if_empty(&mut self.note.size, lane, values[6].abs());
            self.note.dst.push(frame);
        } else if is_empty_note_frame(&self.note.dst[lane as usize]) {
            let frame = note_destination_frame(&values, self.header.h as i32);
            set_lane_note_size_if_empty(&mut self.note.size, lane, values[6].abs());
            self.note.dst[lane as usize] = frame;
        }
    }

    pub(super) fn add_line_source(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let index = values[1].max(0) as usize;
        let Some(region) = self.source_region(&values) else {
            return;
        };
        let id = self.alloc_id("lr2-line");
        self.images.push(json!({
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
        }));
        if self.note.line_sources.len() <= index {
            self.note.line_sources.resize(index + 1, None);
        }
        self.note.line_sources[index] = Some(id.clone());
        self.set_current(id);
    }

    pub(super) fn add_line_destination(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let index = values[1].max(0) as usize;
        let Some(id) = self.note.line_sources.get(index).and_then(|id| id.clone()) else {
            return;
        };
        let ops = self.conditional_ops.clone();
        let destination =
            self.destination_def_with_default_offsets(&id, &values, &ops, &[LR2_OFFSET_LIFT]);
        if self.note.line_destinations.len() <= index {
            self.note.line_destinations.resize(index + 1, None);
        }
        if let Some(previous) = &mut self.note.line_destinations[index] {
            merge_destination_entry(previous, destination);
        } else {
            self.note.line_destinations[index] = Some(destination);
        }
    }

    pub(super) fn complete_play_lines(&mut self) {
        for side in 0..2 {
            let Some(base) = self.note.line_destinations.get(side).and_then(|line| line.clone())
            else {
                continue;
            };
            self.note.group.push(base.clone());
            for (slot, height_scale, color) in [
                (side + 2, 2, [0, 192, 0]),
                (side + 4, 2, [192, 192, 0]),
                (side + 6, 1, [64, 192, 192]),
            ] {
                let destination = if let Some(explicit) =
                    self.note.line_destinations.get(slot).and_then(|line| line.clone())
                {
                    explicit
                } else {
                    self.default_play_line_destination(&base, height_scale, color)
                };
                match slot {
                    2 | 3 => self.note.bpm.push(destination),
                    4 | 5 => self.note.stop.push(destination),
                    6 | 7 => self.note.time.push(destination),
                    _ => {}
                }
            }
        }
    }

    pub(super) fn default_play_line_destination(
        &mut self,
        base: &JsonValue,
        height_scale: i32,
        color: [i32; 3],
    ) -> JsonValue {
        self.ensure_reference_source(111);
        let id = self.alloc_id("lr2-default-line");
        self.images.push(json!({
            "id": id,
            "src": "111",
            "x": 0,
            "y": 0,
            "w": 1,
            "h": 1,
            "divx": 1,
            "divy": 1,
        }));
        let mut destination = base.clone();
        destination["id"] = json!(id);
        if let Some(frames) = destination.get_mut("dst").and_then(JsonValue::as_array_mut) {
            for frame in frames {
                if let Some(object) = frame.as_object_mut() {
                    if let Some(height) = object.get("h").and_then(JsonValue::as_i64) {
                        object.insert(
                            "h".to_string(),
                            json!(height.saturating_mul(height_scale as i64)),
                        );
                    }
                    object.insert("a".to_string(), json!(255));
                    object.insert("r".to_string(), json!(color[0]));
                    object.insert("g".to_string(), json!(color[1]));
                    object.insert("b".to_string(), json!(color[2]));
                }
            }
        }
        destination
    }

    pub(super) fn lr2_lane_to_beatoraja_index(&self, lane: i32) -> Option<i32> {
        let mapped = lr2_lane_to_beatoraja_index(lane)?;
        if self.remap_single_play_2p_lanes && mapped >= 8 { Some(mapped - 8) } else { Some(mapped) }
    }

    pub(super) fn add_judge_image(&mut self, line: &CsvLine, index: usize) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            return;
        };
        let id = self.alloc_id("lr2-judge-image");
        self.images.push(json!({
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
        }));
        self.ensure_judge(index);
        if !self.judges[index].marker_inserted {
            self.destinations.push(json!({ "id": format!("judge-{index}") }));
            self.judges[index].marker_inserted = true;
        }
        self.judges[index].shift = values[11] != 1;
        set_judge_slot(
            &mut self.judges[index].images,
            lr2_judge_slot(values[1]),
            json!({ "id": id, "dst": [] }),
        );
        self.set_current(id);
    }

    pub(super) fn add_judge_image_destination(&mut self, line: &CsvLine, index: usize) {
        let Some(current) = self.current.clone() else {
            return;
        };
        self.ensure_judge(index);
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let dst = self.destination_def_with_default_offsets(
                &variant.id,
                &values,
                &ops,
                &[LR2_OFFSET_JUDGE_1P, LR2_OFFSET_LIFT],
            );
            if let Some(entry) = self.judges[index].images.iter_mut().rev().find(|entry| {
                entry.get("id").and_then(JsonValue::as_str) == Some(variant.id.as_str())
            }) {
                merge_destination_entry(entry, dst);
            }
        }
        if !self.judges[index].detail_inserted {
            self.judges[index].detail_inserted = true;
            self.add_default_judge_detail(&values, index);
        }
    }

    pub(super) fn add_default_judge_detail(&mut self, judge_dst: &[i32; 22], side: usize) {
        const TIMERS: [i32; 3] = [46, 47, 247];
        const EARLY_OPTIONS: [i32; 3] = [1242, 1262, 1362];
        const LATE_OPTIONS: [i32; 3] = [1243, 1263, 1363];
        const PERFECT_OPTIONS: [i32; 3] = [241, 261, 361];
        const DURATION_REFS: [i32; 3] = [525, 526, 527];
        let side = side.min(2);
        if !self.sources.iter().any(|source| {
            source.get("id").and_then(JsonValue::as_str) == Some("lr2-judgedetail-source")
        }) {
            self.sources.push(json!({
                "id": "lr2-judgedetail-source",
                "path": "bmz://lr2/judgedetail",
            }));
        }

        let center_x = judge_dst[3].saturating_add(judge_dst[5] / 2);
        let y = (self.header.h as i32).saturating_sub(judge_dst[4].saturating_sub(5));
        let label_w = ((40_i64 * i64::from(self.header.w)) / 1280).max(1) as i32;
        let digit_w = ((8_i64 * i64::from(self.header.w)) / 1280).max(1) as i32;
        let height = ((16_i64 * i64::from(self.header.h)) / 720).max(1) as i32;
        let timer = TIMERS[side];

        for (name, x, op) in [("early", 0, EARLY_OPTIONS[side]), ("late", 50, LATE_OPTIONS[side])] {
            let id = self.alloc_id(&format!("lr2-judge-detail-{name}"));
            self.images.push(json!({
                "id": id,
                "src": "lr2-judgedetail-source",
                "x": x,
                "y": 0,
                "w": 50,
                "h": 20,
                "divx": 1,
                "divy": 1,
            }));
            self.destinations.push(judge_detail_destination(
                &id,
                center_x,
                y,
                label_w,
                height,
                timer,
                &[SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_EARLY_LATE, op],
            ));
        }

        for (index, (source_y, perfect_op)) in
            [(20, PERFECT_OPTIONS[side]), (60, -PERFECT_OPTIONS[side])].into_iter().enumerate()
        {
            let id = self.alloc_id(&format!("lr2-judge-detail-number-{index}"));
            self.values.push(json!({
                "id": id,
                "src": "lr2-judgedetail-source",
                "x": 0,
                "y": source_y,
                "w": 120,
                "h": 40,
                "divx": 12,
                "divy": 2,
                "ref": DURATION_REFS[side],
                "align": judge_dst[12],
                "digit": 4,
                "zeropadding": 0,
                "space": judge_dst[15],
            }));
            self.destinations.push(judge_detail_destination(
                &id,
                center_x,
                y,
                digit_w,
                height,
                timer,
                &[SKIN_OPTION_BMZ_LR2_JUDGE_DETAIL_MS, perfect_op],
            ));
        }
    }

    pub(super) fn add_judge_number(&mut self, line: &CsvLine, index: usize) {
        let values = parse_values(line);
        self.add_number(line);
        if let Some(variant) = self.current_primary_variant() {
            if let Some(value) = self.values.iter_mut().find(|value| {
                value.get("id").and_then(JsonValue::as_str) == Some(variant.id.as_str())
            }) {
                value["judgeAlign"] = json!(if values[12] == 1 { 2 } else { values[12] });
            }
            self.ensure_judge(index);
            set_judge_slot(
                &mut self.judges[index].numbers,
                lr2_judge_slot(values[1]),
                json!({ "id": variant.id, "dst": [] }),
            );
        }
    }

    pub(super) fn add_judge_number_destination(&mut self, line: &CsvLine, index: usize) {
        let Some(current) = self.current.clone() else {
            return;
        };
        self.ensure_judge(index);
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let mut dst = judge_combo_destination_def(
                &variant.id,
                &values,
                &ops,
                &[LR2_OFFSET_JUDGE_1P, LR2_OFFSET_LIFT],
            );
            self.expand_destination_option_aliases(&mut dst);
            if let Some(entry) = self.judges[index].numbers.iter_mut().rev().find(|entry| {
                entry.get("id").and_then(JsonValue::as_str) == Some(variant.id.as_str())
            }) {
                merge_destination_entry(entry, dst);
            }
        }
    }

    pub(super) fn add_hidden_cover(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-hidden");
        self.hidden_covers.push(json!({
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
            "disapearLine": lr2_disappear_line(values[11], self.header.h as i32),
            "isDisapearLineLinkLift": lr2_hidden_link_lift(line, &values),
        }));
        self.set_current(id);
    }

    pub(super) fn add_lift_cover(&mut self, line: &CsvLine) {
        let values = parse_values(line);
        let Some(region) = self.source_region(&values) else {
            self.current = None;
            return;
        };
        let id = self.alloc_id("lr2-liftcover");
        self.hidden_covers.push(json!({
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
            "disapearLine": lr2_disappear_line(values[11], self.header.h as i32),
            "isDisapearLineLinkLift": lr2_hidden_link_lift(line, &values),
        }));
        self.set_current(id);
    }
}

fn open_lr2_effect_follows_note_adjustment(destination: &JsonValue) -> bool {
    let timer = destination.get("timer").and_then(JsonValue::as_i64).unwrap_or(0);
    if (50..90).contains(&timer) {
        return true;
    }
    if !(100..140).contains(&timer) {
        return false;
    }

    let Some(frames) = destination.get("dst").and_then(JsonValue::as_array) else {
        return false;
    };
    let frame_height = |frame: &JsonValue| {
        frame.get("h").and_then(JsonValue::as_i64).unwrap_or(0).saturating_abs()
    };
    match (frames.first(), frames.last()) {
        (Some(first), Some(last)) => frame_height(first) >= 100 || frame_height(last) >= 100,
        _ => false,
    }
}
