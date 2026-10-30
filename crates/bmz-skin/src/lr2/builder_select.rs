use super::*;

/// LR2 lamp slot of each beatoraja clear index. Only display lamp families are
/// collapsed; stored clear types remain unchanged.
const LR2_LAMPS: [i32; 11] = [0, 1, 2, 2, 2, 3, 4, 4, 5, 5, 5];

#[derive(Default)]
pub(super) struct SelectState {
    sources: BTreeMap<(String, i32), CurrentObject>,
    destinations: BTreeMap<(String, i32), Vec<JsonValue>>,
    center: i32,
    available: Option<(i32, i32)>,
    marker: bool,
}

/// The songlist entry list and indices drawn by an LR2 `DST_BAR_*` part.
fn lr2_bar_part(kind: &str, index: i32) -> Option<JsonValue> {
    let (part, indices): (&str, Vec<i32>) = match kind {
        "BODY_OFF" => ("listoff", vec![index]),
        "BODY_ON" => ("liston", vec![index]),
        "TITLE" => ("text", vec![index]),
        "LEVEL" => ("level", vec![index]),
        "LAMP" | "MY_LAMP" | "RIVAL_LAMP" => (
            match kind {
                "LAMP" => "lamp",
                "MY_LAMP" => "playerlamp",
                _ => "rivallamp",
            },
            (0..LR2_LAMPS.len() as i32)
                .filter(|&clear| LR2_LAMPS[clear as usize] == index)
                .collect(),
        ),
        "RIVAL" => ("lr2rival", vec![index]),
        "LABEL" => ("label", vec![index]),
        "TROPHY" => ("trophy", vec![index]),
        "FLASH" => ("flash", vec![0]),
        _ => return None,
    };
    Some(json!({"part": part, "indices": indices}))
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
                | "LABEL"
                | "TROPHY"
        ) {
            return false;
        }
        let index = values[1].clamp(0, 29);
        if (kind == "LABEL" && !(0..5).contains(&values[1]))
            || (kind == "TROPHY" && !(0..3).contains(&values[1]))
        {
            self.current = None;
            return true;
        }
        if kind == "RANK" {
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
            self.select.marker = true;
            if matches!(kind, "LABEL" | "TROPHY") {
                for (position, size) in [(3, 5), (4, 6)] {
                    if values[size] < 0 {
                        values[position] = values[position].saturating_add(values[size]);
                        values[size] = values[size].saturating_neg();
                    }
                }
            }
            let body = kind.starts_with("BODY_");
            let variants = if body {
                vec![CurrentObjectVariant {
                    id: "lr2-bars".to_string(),
                    conditional_ops: Vec::new(),
                    excluded_ops: Vec::new(),
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
                let mut excluded = variant.excluded_ops;
                excluded.extend(self.conditional_not_ops.clone());
                destination["lr2Not"] = json!(excluded);
                self.expand_destination_option_aliases(&mut destination);
                let entries =
                    self.select.destinations.entry((kind.to_string(), index)).or_default();
                // Keyframes of the same conditional destination share a marker.
                // A different source or IF branch keeps its own declaration position.
                if let Some(previous) = entries.iter_mut().rev().find(|previous| {
                    previous["id"] == destination["id"]
                        && previous["op"] == destination["op"]
                        && previous["lr2Not"] == destination["lr2Not"]
                }) && merge_destination_entry(previous, destination.clone())
                {
                    continue;
                }
                if let Some(mut part) = lr2_bar_part(kind, index) {
                    part["order"] = json!(self.destinations.len());
                    destination["lr2BarPart"] = part.clone();
                    self.destinations.push(json!({"id": "lr2-songlist", "lr2BarPart": part}));
                }
                entries.push(destination);
            }
        }
        true
    }

    pub(super) fn finish_songlist(&mut self) -> JsonValue {
        if !self.select.marker {
            return JsonValue::Null;
        }
        // Keep LR2's native body slots, including course (8) and random course (9).
        // The renderer translates BMZ row kinds instead of reusing beatoraja indices.
        let body_ids = std::array::from_fn::<_, 10, _>(|index| {
            self.select
                .sources
                .get(&("BODY".to_string(), index as i32))
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
        let lamps = LR2_LAMPS;
        let (first, last) = self.select.available.unwrap_or((0, 29));
        json!({
            "id": "lr2-songlist", "center": self.select.center,
            "lr2BottomOrigin": true,
            "clickable": (first..=last).collect::<Vec<_>>(),
            "listoff": slots("BODY_OFF", &(0..30).collect::<Vec<_>>()),
            "liston": slots("BODY_ON", &(0..30).collect::<Vec<_>>()),
            "text": slots("TITLE", &[0, 1]),
            "level": slots("LEVEL", &[0, 1, 2, 3, 4, 5, 6]),
            "lamp": slots("LAMP", &lamps),
            "playerlamp": slots("MY_LAMP", &lamps),
            "rivallamp": slots("RIVAL_LAMP", &lamps),
            "lr2rival": slots("RIVAL", &[0, 1, 2, 3, 4, 5]),
            "flash": slots("FLASH", &[0]),
            "label": slots("LABEL", &[0, 1, 2, 3, 4]),
            "trophy": slots("TROPHY", &[0, 1, 2]),
        })
    }
}
