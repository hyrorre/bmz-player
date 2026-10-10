use super::*;

#[derive(Default)]
pub(super) struct SelectState {
    sources: BTreeMap<(String, i32), CurrentObject>,
    destinations: BTreeMap<(String, i32), Vec<JsonValue>>,
    center: i32,
    available: Option<(i32, i32)>,
    marker: bool,
}

impl CsvBuilder<'_> {
    pub(super) fn execute_select_command(&mut self, line: &CsvLine) -> bool {
        if self.header.skin_type != 5 {
            return false;
        }
        let command = line.command.as_str();
        let mut values = parse_values(line);
        if command == "BAR_CENTER" {
            self.select.center = values[1].clamp(0, 29);
            return true;
        }
        if command == "BAR_AVAILABLE" {
            self.select.available = Some((values[1].clamp(0, 29), values[2].clamp(0, 29)));
            return true;
        }
        let Some(kind) =
            command.strip_prefix("SRC_BAR_").or_else(|| command.strip_prefix("DST_BAR_"))
        else {
            return false;
        };
        if !matches!(
            kind,
            "BODY"
                | "BODY_ON"
                | "BODY_OFF"
                | "TITLE"
                | "LEVEL"
                | "LAMP"
                | "MY_LAMP"
                | "RIVAL_LAMP"
                | "FLASH"
                | "RANK"
                | "RIVAL"
        ) {
            return false;
        }
        let index = values[1].clamp(0, 29);
        if matches!(kind, "RANK" | "RIVAL") {
            if command.starts_with("SRC_") {
                self.warn(format!("unsupported lr2 song bar decoration: #{command}"));
            }
            return true;
        }
        if command.starts_with("SRC_") {
            self.current = None;
            match kind {
                "TITLE" => {
                    self.add_text(line);
                    if let Some(text) = self.texts.last_mut() {
                        text["ref"] = json!(10); // STRING_BAR_TITLE
                        let id = format!("{}-bartext", text["id"].as_str().unwrap());
                        text["id"] = json!(id);
                        if let Some(current) = &mut self.current {
                            for variant in &mut current.variants {
                                variant.id.clone_from(&id);
                            }
                        }
                    }
                }
                "LEVEL" => {
                    // add_number skips an undefined image; never retarget an earlier number.
                    let before = self.values.len();
                    self.add_number(line);
                    if self.values.len() > before
                        && let Some(number) = self.values.last_mut()
                    {
                        number["ref"] = json!(96);
                    }
                }
                _ => self.add_image(line),
            }
            if let Some(current) = self.current.take() {
                self.select.sources.insert((kind.to_string(), index), current);
            }
        } else {
            if !self.select.marker {
                self.destinations.push(json!({"id": "lr2-songlist"}));
                self.select.marker = true;
            }
            let body = kind.starts_with("BODY_");
            let variants = if body {
                vec![CurrentObjectVariant {
                    id: "lr2-bars".to_string(),
                    conditional_ops: Vec::new(),
                }]
            } else {
                self.select
                    .sources
                    .get(&(kind.to_string(), index))
                    .map(|current| current.variants.clone())
                    .unwrap_or_default()
            };
            if kind == "TITLE" {
                // LR2 reserves these columns for bar titles; only #IF supplies conditions.
                values[18..=20].fill(0);
            }
            for variant in variants {
                let ops = self.combined_conditional_ops(&variant);
                let mut destination = destination_def_with_default_offsets(
                    &variant.id,
                    &values,
                    if body { self.header.h as i32 } else { 0 },
                    &ops,
                    &[],
                );
                self.expand_destination_option_aliases(&mut destination);
                let entries =
                    self.select.destinations.entry((kind.to_string(), index)).or_default();
                merge_or_push_current_destination(entries, destination);
            }
        }
        true
    }

    pub(super) fn finish_songlist(&mut self) -> JsonValue {
        if !self.select.marker {
            return JsonValue::Null;
        }
        let body_ids = [0, 1, 2, 3, 4, 5, 6, 7, 1, 1, 1].map(|index| {
            self.select
                .sources
                .get(&("BODY".to_string(), index))
                .or_else(|| self.select.sources.get(&("BODY".to_string(), 0)))
                .and_then(|current| current.variants.first())
                .map(|v| v.id.clone())
                .unwrap_or_default()
        });
        self.imagesets.push(json!({"id": "lr2-bars", "images": body_ids}));
        let entry = |kind: &str, index: i32| -> JsonValue {
            let Some(entries) = self.select.destinations.get(&(kind.to_string(), index)) else {
                return json!({"id": "lr2-missing-bar-part"});
            };
            if entries.len() == 1 {
                entries[0].clone()
            } else {
                json!({"if": [], "values": entries})
            }
        };
        let slots = |kind: &str, indices: &[i32]| {
            indices.iter().map(|&i| entry(kind, i)).collect::<Vec<_>>()
        };
        // Collapse only display lamp families; stored clear types remain unchanged.
        let lamps = [0, 1, 2, 2, 2, 3, 4, 4, 5, 5, 5];
        let (first, last) = self.select.available.unwrap_or((0, 29));
        json!({
            "id": "lr2-songlist", "center": self.select.center,
            "lr2BottomOrigin": true,
            "clickable": (first..=last).collect::<Vec<_>>(),
            "listoff": slots("BODY_OFF", &(0..30).collect::<Vec<_>>()),
            "liston": slots("BODY_ON", &(0..30).collect::<Vec<_>>()),
            "text": slots("TITLE", &[0; 14]),
            "level": slots("LEVEL", &[0, 1, 2, 3, 4, 5, 6]),
            "lamp": slots("LAMP", &lamps),
            "playerlamp": slots("MY_LAMP", &lamps),
            "rivallamp": slots("RIVAL_LAMP", &lamps),
            "flash": slots("FLASH", &[0]),
        })
    }
}
