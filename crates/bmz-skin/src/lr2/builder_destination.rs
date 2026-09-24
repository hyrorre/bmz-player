use super::*;

impl<'a> CsvBuilder<'a> {
    pub(super) fn add_destination(&mut self, line: &CsvLine) {
        self.add_destination_with_default_offsets(line, &[]);
    }

    pub(super) fn add_destination_with_default_offsets(
        &mut self,
        line: &CsvLine,
        default_offsets: &[i32],
    ) {
        let Some(current) = self.current.clone() else {
            return;
        };
        let values = parse_values(line);
        for variant in current.variants {
            let ops = self.combined_conditional_ops(&variant);
            let mut destination_values = values;
            if let Some(&(width, height)) = self.special_destination_sizes.get(&variant.id) {
                destination_values[4] = destination_values[4].saturating_sub(height);
                destination_values[5] = width;
                destination_values[6] = height;
            }
            let mut dst = if self.lr2_gauge_id.as_deref() == Some(variant.id.as_str()) {
                gauge_destination_def(
                    &variant.id,
                    &destination_values,
                    self.header.h as i32,
                    self.lr2_gauge_add_x,
                    self.lr2_gauge_add_y,
                    &ops,
                )
            } else if line.command == "DST_BARGRAPH" {
                let mut destination = destination_def_with_default_offsets(
                    &variant.id,
                    &destination_values,
                    self.header.h as i32,
                    &ops,
                    default_offsets,
                );
                if let Some(frame) = destination
                    .get_mut("dst")
                    .and_then(JsonValue::as_array_mut)
                    .and_then(|frames| frames.first_mut())
                {
                    frame["x"] = json!(destination_values[3]);
                    frame["w"] = json!(destination_values[5]);
                }
                destination
            } else if variant.id.contains("lr2-liftcover") {
                self.destination_def_with_default_offsets(
                    &variant.id,
                    &destination_values,
                    &ops,
                    &[LR2_OFFSET_LIFT],
                )
            } else if !default_offsets.is_empty() {
                self.destination_def_with_default_offsets(
                    &variant.id,
                    &destination_values,
                    &ops,
                    default_offsets,
                )
            } else {
                self.destination_def_with_ops(&variant.id, &destination_values, &ops)
            };
            if matches!(line.command.as_str(), "DST_IMAGE" | "DST_BGA") {
                dst["stretch"] = json!(self.stretch);
            }
            self.expand_destination_option_aliases(&mut dst);
            if self.current_has_destination {
                merge_or_push_current_destination(&mut self.destinations, dst);
            } else {
                self.destinations.push(dst);
                self.current_has_destination = true;
            }
        }
    }

    pub(super) fn set_current(&mut self, id: String) {
        let variant = CurrentObjectVariant { id, conditional_ops: self.conditional_ops.clone() };
        if !variant.conditional_ops.is_empty()
            && !self.current_has_destination
            && let Some(current) = &mut self.current
        {
            current.variants.push(variant);
            return;
        }

        self.current = Some(CurrentObject { variants: vec![variant] });
        self.current_has_destination = false;
    }

    pub(super) fn current_primary_variant(&self) -> Option<CurrentObjectVariant> {
        self.current.as_ref().and_then(|current| current.variants.first().cloned())
    }

    pub(super) fn combined_conditional_ops(&self, variant: &CurrentObjectVariant) -> Vec<i32> {
        let mut ops = variant.conditional_ops.clone();
        ops.extend(self.conditional_ops.iter().copied());
        ops
    }

    pub(super) fn register_runtime_option_alias(
        &mut self,
        option: i32,
        value: bool,
        condition: &[i32],
    ) {
        if value
            && !condition.is_empty()
            && !self.header.selected_ops.get(&option.abs()).copied().unwrap_or(false)
        {
            self.runtime_option_aliases.entry(option.abs()).or_insert_with(|| condition.to_vec());
        }
    }

    pub(super) fn expand_destination_option_aliases(&self, destination: &mut JsonValue) {
        let Some(ops) = destination.get_mut("op").and_then(JsonValue::as_array_mut) else {
            return;
        };
        let original = std::mem::take(ops);
        for op in original {
            let Some(op) = op.as_i64().and_then(|value| i32::try_from(value).ok()) else {
                continue;
            };
            let option_id = op.abs();
            if let Some(alias) = self.runtime_option_aliases.get(&option_id) {
                if op >= 0 {
                    ops.extend(alias.iter().map(|value| json!(value)));
                } else if alias.len() == 1 {
                    ops.push(json!(-alias[0]));
                } else {
                    ops.push(json!(op));
                }
            } else {
                ops.push(json!(op));
            }
        }
        let mut seen = BTreeSet::new();
        ops.retain(|op| op.as_i64().is_none_or(|value| seen.insert(value)));
    }

    pub(super) fn destination_def_with_ops(
        &self,
        id: &str,
        values: &[i32; 22],
        conditional_ops: &[i32],
    ) -> JsonValue {
        let mut destination = destination_def_with_default_offsets(
            id,
            values,
            self.header.h as i32,
            conditional_ops,
            &[],
        );
        self.expand_destination_option_aliases(&mut destination);
        destination
    }

    pub(super) fn destination_def_with_default_offsets(
        &self,
        id: &str,
        values: &[i32; 22],
        conditional_ops: &[i32],
        default_offsets: &[i32],
    ) -> JsonValue {
        let mut destination = destination_def_with_default_offsets(
            id,
            values,
            self.header.h as i32,
            conditional_ops,
            default_offsets,
        );
        self.expand_destination_option_aliases(&mut destination);
        destination
    }

    pub(super) fn ensure_judge(&mut self, index: usize) {
        while self.judges.len() <= index {
            self.judges.push(JudgeState::default());
        }
    }

    pub(super) fn source_region(&mut self, values: &[i32; 22]) -> Option<SourceRegion> {
        let source_index = values[2];
        if source_index < 0 {
            return None;
        }
        let source_id = source_index.to_string();
        if LR2_REFERENCE_IMAGES.contains(&source_index) {
            self.ensure_reference_source(source_index);
            return Some(SourceRegion {
                src: source_id,
                x: 0,
                y: 0,
                w: -1,
                h: -1,
                divx: 1,
                divy: 1,
                cycle: 0,
                timer: None,
            });
        }
        if source_index as usize >= self.source_paths.len() {
            self.warn(format!("lr2 csv source index {source_index} is not defined"));
            return None;
        }
        Some(SourceRegion {
            src: source_id,
            x: values[3],
            y: values[4],
            w: values[5],
            h: values[6],
            divx: values[7].max(1),
            divy: values[8].max(1),
            cycle: values[9],
            timer: (values[10] != 0).then_some(values[10]),
        })
    }

    pub(super) fn resolve_source_path(&mut self, raw_path: &str) -> String {
        let normalized = self.relative_source_path(&normalize_lr2_asset_path(raw_path));
        if let Some(file) = self.header.files.iter().find(|file| file.path == normalized) {
            let file_name = file.name.clone();
            let file_path = file.path.clone();
            self.file_dependencies.insert(file_name.clone());
            if let Some(selected) =
                self.files.get(&file_name).filter(|selected| !selected.is_empty())
                && let Some(selected_path) =
                    self.selected_skin_file_for_definition(&file_path, selected)
            {
                return selected_path;
            }
        }
        if let Some(file) =
            self.header.files.iter().find(|file| same_wildcard_prefix(&file.path, &normalized))
        {
            let file_name = file.name.clone();
            let file_path = file.path.clone();
            let file_default = file.default.clone();
            self.file_dependencies.insert(file_name.clone());
            if let Some(selected) =
                self.files.get(&file_name).filter(|selected| !selected.is_empty())
                && let Some(selected_path) =
                    self.selected_skin_file_for_definition(&file_path, selected)
                && selected_wildcard_value(&file_path, &selected_path).is_some()
            {
                return substitute_wildcard(&normalized, &file_path, &selected_path);
            }
            if !file_default.is_empty() {
                return substitute_wildcard_default(&normalized, &file_path, &file_default);
            }
        }
        normalized
    }

    pub(super) fn resolve_lr2_font_path(&mut self, raw_path: &str) -> String {
        let path = self.resolve_source_path(raw_path);
        if !path.to_ascii_lowercase().ends_with(".lr2font") {
            return path;
        }
        let fnt = format!("{}fnt", &path[..path.len() - "lr2font".len()]);
        if self.skin_root.join(&fnt).is_file() || self.skin_file_dir.join(&fnt).is_file() {
            return self.relative_font_path_for_skin_file(&fnt);
        }
        self.relative_font_path_for_skin_file(&path)
    }

    pub(super) fn relative_font_path_for_skin_file(&self, path: &str) -> String {
        if self.skin_file_dir.join(path).is_file() {
            return path.to_string();
        }
        let parent_relative = format!("../{path}");
        if self.skin_file_dir.join(&parent_relative).is_file() {
            return parent_relative;
        }
        path.to_string()
    }

    pub(super) fn lr2_text_size(&self, font_index: i32) -> i32 {
        if let Some(Some(font_id)) = self.lr2font_ids.get(font_index.max(0) as usize)
            && self.fonts.iter().any(|font| {
                font.get("id").and_then(JsonValue::as_str) == Some(font_id.as_str())
                    && font.get("path").and_then(JsonValue::as_str).is_some_and(|path| {
                        let path = path.to_ascii_lowercase();
                        path.ends_with(".fnt") || path.ends_with(".lr2font")
                    })
            })
        {
            return 0;
        }
        self.fonts
            .get(font_index.max(0) as usize)
            .and_then(|font| font.get("size"))
            .and_then(JsonValue::as_i64)
            .and_then(|size| i32::try_from(size).ok())
            .filter(|size| *size > 0)
            .unwrap_or(48)
    }

    pub(super) fn selected_skin_file_for_definition(
        &self,
        definition: &str,
        selected: &str,
    ) -> Option<String> {
        let selected = normalize_selected_skin_file(selected)?;
        if self.skin_file_dir.join(&selected).is_file() {
            return Some(selected);
        }
        if selected.contains('/') {
            return None;
        }
        let definition = definition.replace('\\', "/");
        let star = definition.find('*')?;
        let prefix = &definition[..star];
        let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
        let candidate = format!("{}{selected}", &prefix[..slash]);
        let candidate = normalize_selected_skin_file(&candidate)?;
        self.skin_file_dir.join(&candidate).is_file().then_some(candidate)
    }

    pub(super) fn relative_source_path(&self, normalized: &str) -> String {
        if let Some(dir_name) = &self.skin_file_dir_name
            && let Some(stripped) = normalized.strip_prefix(&format!("{dir_name}/"))
        {
            return stripped.to_string();
        }
        normalized.to_string()
    }

    pub(super) fn alloc_id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }

    pub(super) fn warn(&mut self, message: String) {
        self.warnings.push(SkinLoadWarning { message });
    }

    pub(super) fn finish(mut self) -> JsonValue {
        self.complete_open_lr2_note_adjustment_effects();
        self.complete_play_lines();
        let category = json!([{ "name": "LR2", "item": ["property", "filepath", "offset"] }]);
        let property = self
            .header
            .options
            .iter()
            .map(|option| {
                let items = option
                    .items
                    .iter()
                    .enumerate()
                    .map(|(index, name)| json!({ "name": name, "op": option.base + index as i32 }))
                    .collect::<Vec<_>>();
                let default_item = option
                    .items
                    .iter()
                    .enumerate()
                    .find(|(index, _)| {
                        self.header
                            .selected_ops
                            .get(&(option.base + *index as i32))
                            .copied()
                            .unwrap_or(false)
                    })
                    .map(|(_, item)| item.clone())
                    .or_else(|| option.items.first().cloned())
                    .unwrap_or_default();
                json!({
                    "category": "LR2",
                    "name": option.name,
                    "item": items,
                    "def": default_item,
                })
            })
            .collect::<Vec<_>>();
        let filepath = self
            .header
            .files
            .iter()
            .map(|file| {
                json!({
                    "category": "LR2",
                    "name": file.name,
                    "path": file.path,
                    "def": file.default,
                })
            })
            .collect::<Vec<_>>();
        let offset = self
            .header
            .offsets
            .iter()
            .map(|offset| {
                json!({
                    "category": "LR2",
                    "name": offset.name,
                    "id": offset.id,
                    "x": offset.flags[0],
                    "y": offset.flags[1],
                    "w": offset.flags[2],
                    "h": offset.flags[3],
                    "r": offset.flags[4],
                    "a": offset.flags[5],
                })
            })
            .collect::<Vec<_>>();
        let note = (!self.note.note.is_empty() || !self.note.dst.is_empty()).then(|| {
            let dst2 = self.note.dst2.map(|y| {
                (self.header.h as i32)
                    .saturating_sub(y.saturating_add(self.note.size.first().copied().unwrap_or(0)))
            });
            json!({
                "id": "notes",
                "note": self.note.note,
                "lnstart": self.note.lnstart,
                "lnend": self.note.lnend,
                "lnbody": self.note.lnbody,
                "lnbodyActive": self.note.lnbody_active,
                "hcnstart": self.note.hcnstart,
                "hcnend": self.note.hcnend,
                "hcnbody": self.note.hcnbody,
                "hcnactive": self.note.hcnactive,
                "hcndamage": self.note.hcndamage,
                "hcnreactive": self.note.hcnreactive,
                "hlnstart": self.note.hlnstart,
                "hlnend": self.note.hlnend,
                "hlnbody": self.note.hlnbody,
                "hlnbodyActive": self.note.hlnactive,
                "hlnbodyReactive": self.note.hlnreactive,
                "hlnbodyMiss": self.note.hlndamage,
                "mine": self.note.mine,
                "size": self.note.size,
                "dst2": dst2.unwrap_or(i32::MIN),
                "expansionrate": self.note.expansion_rate.unwrap_or([100, 100]),
                "dst": self.note.dst,
                "group": self.note.group,
                "bpm": self.note.bpm,
                "stop": self.note.stop,
                "time": self.note.time,
            })
        });
        let judge = self
            .judges
            .into_iter()
            .enumerate()
            .map(|(index, judge)| {
                json!({
                    "id": format!("judge-{index}"),
                    "index": index as i32,
                    "images": judge.images,
                    "numbers": judge.numbers,
                    "shift": judge.shift,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "type": self.header.skin_type,
            "name": self.header.name,
            "author": self.header.author,
            "w": self.header.w,
            "h": self.header.h,
            "fadeout": self.header.fadeout,
            "input": self.header.input,
            "scene": self.header.scene,
            "close": self.header.close,
            "loadstart": self.header.loadstart,
            "loadend": self.header.loadend,
            "playstart": self.header.playstart,
            "judgetimer": self.header.judgetimer,
            "finishmargin": self.header.finishmargin,
            "category": category,
            "property": property,
            "filepath": filepath,
            "offset": offset,
            "source": self.sources,
            "font": self.fonts,
            "image": self.images,
            "imageset": self.imagesets,
            "value": self.values,
            "text": self.texts,
            "slider": self.sliders,
            "graph": self.graphs,
            "judgegraph": self.judge_graphs,
            "bpmgraph": self.bpm_graphs,
            "timingvisualizer": self.timing_visualizers,
            "hiddenCover": self.hidden_covers,
            "gauge": self.gauge,
            "gauges": self.gauges,
            "note": note,
            "judge": judge,
            "bga": self.bga,
            "destination": self.destinations,
        })
    }
}
