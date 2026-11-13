//! BMSON branches share a pulse grid, but retain independent sound-channel slices.
use super::*;
use serde_json::Value;

const ARRAYS: &[&str] = &[
    "sound_channels",
    "key_channels",
    "mine_channels",
    "free_zone_channels",
    "potion_notes",
    "dummy_notes",
    "dummy_mine_notes",
    "dummy_potion_notes",
    "bpm_events",
    "stop_events",
    "scroll_events",
    "speed_events",
    "lines",
];
const BGA_ARRAYS: &[&str] = &["bga_events", "layer_events", "poor_events"];

pub(crate) fn compile_bmson(
    source: &Value,
    path: &Path,
    mut parse: impl FnMut(&Value) -> Result<IntermediateChart, ImportError>,
) -> Result<IntermediateChart, ImportError> {
    let mut root = source.clone();
    let raw_blocks = root.as_object_mut().and_then(|object| object.remove("conditional_blocks"));
    let mut common = parse(&root)?;
    let mut diagnostics = Vec::new();
    let mut blocks = Vec::new();
    let resolution = source["info"]["resolution"].as_u64().filter(|r| *r > 0).unwrap_or(240);
    let mut resources = common.resources.clone();
    resolve_bpm(&mut common);
    let mut raw_blocks =
        raw_blocks.and_then(|value| value.as_array().cloned()).unwrap_or_else(|| {
            diagnostics.push("BMSON conditional_blocks must be an array".to_string());
            Vec::new()
        });
    raw_blocks.sort_by_key(|block| block["y"].as_u64().unwrap_or(u64::MAX));
    for raw in raw_blocks {
        let Some(y) = raw["y"].as_u64().filter(|y| *y / resolution / 4 < 10_000) else {
            diagnostics.push("invalid BMSON conditional evaluation position; block ignored".into());
            continue;
        };
        let Some(raw_branches) = raw["branches"].as_array() else {
            diagnostics.push("BMSON conditional block has no branches; ignored".into());
            continue;
        };
        if !raw_branches.iter().any(|b| b["default"].as_bool() == Some(true)) {
            diagnostics.push("BMSON conditional block has no DEFAULT; ignored".into());
            continue;
        }
        let mut conditions = Vec::new();
        let mut branches = Vec::new();
        let mut default = None;
        for raw_branch in raw_branches {
            let is_default = raw_branch["default"].as_bool() == Some(true);
            let condition = if is_default {
                if default.is_some() {
                    diagnostics.push("duplicate BMSON DEFAULT; ignored".into());
                    continue;
                }
                None
            } else {
                match raw_branch["when"].as_str().map(Condition::parse) {
                    Some(Ok(condition)) => Some(condition),
                    _ => {
                        diagnostics.push("invalid BMSON branch condition; ignored".into());
                        continue;
                    }
                }
            };
            let mut input = root.clone();
            for &key in ARRAYS {
                input[key] = Value::Array(Vec::new());
            }
            for &key in BGA_ARRAYS {
                input["bga"][key] = Value::Array(Vec::new());
            }
            for &key in ARRAYS {
                if let Some(value) = raw_branch.get(key) {
                    input[key] = sanitize(value, y, &mut diagnostics);
                }
            }
            for &key in BGA_ARRAYS {
                let mut events = Vec::new();
                for value in [raw_branch["bga"].get(key), raw_branch.get(key)].into_iter().flatten()
                {
                    if let Value::Array(mut array) = sanitize(value, y, &mut diagnostics) {
                        events.append(&mut array);
                    }
                }
                input["bga"][key] = Value::Array(events);
            }
            let mut branch = match parse(&input) {
                Ok(chart) => chart,
                Err(error) => {
                    diagnostics.push(format!("BMSON branch could not be interpreted: {error}"));
                    let mut empty = common.clone();
                    empty.objects.clear();
                    empty.layered_note_sounds.clear();
                    empty.long_end_sounds.clear();
                    empty.resources.wavs.clear();
                    empty.resources.metric_sound_slices.clear();
                    empty
                }
            };
            resolve_bpm(&mut branch);
            remap_sounds(&mut branch, &mut resources)?;
            if let Some(condition) = condition {
                conditions.push(condition);
                branches.push(branch);
            } else {
                default = Some(branch);
            }
        }
        let Some(default) = default else { continue };
        branches.push(default);
        blocks.push(ConditionalBlock {
            position: y as f64 / (resolution as f64 * 4.0),
            conditions,
            lengths: vec![BTreeMap::new(); branches.len()],
            branches,
        });
    }
    common.resources = resources;
    common.metadata.kaleid.declared.conditional = true;
    common.metadata.kaleid.source.conditional = true;
    common.metadata.kaleid.diagnostics.extend(diagnostics);
    if blocks.is_empty() {
        return Ok(common);
    }
    let mut empty = common.clone();
    empty.objects.clear();
    empty.layered_note_sounds.clear();
    empty.long_end_sounds.clear();
    let mut chunks = vec![(common.clone(), BTreeMap::new())];
    chunks.extend((0..blocks.len()).map(|_| (empty.clone(), BTreeMap::new())));
    finish_program(
        ConditionalProgram {
            blocks,
            source_ln_modes: 0,
            common,
            common_chunks: chunks,
            source: path.into(),
            positions: BTreeMap::new(),
            voice_ids: BTreeMap::new(),
            bga_assets: None,
            margin: TimeUs(0),
            transform: None,
        },
        false,
    )
}

fn sanitize(value: &Value, y: u64, diagnostics: &mut Vec<String>) -> Value {
    let Some(array) = value.as_array() else {
        diagnostics.push("BMSON branch content must be an array; ignored".into());
        return Value::Array(Vec::new());
    };
    Value::Array(
        array
            .iter()
            .filter_map(|entry| {
                let mut entry = entry.clone();
                if let Some(notes) = entry.get("notes") {
                    let notes = sanitize(notes, y, diagnostics);
                    entry["notes"] = notes;
                    Some(entry)
                } else if entry["y"].as_u64().is_some_and(|position| position >= y) {
                    Some(entry)
                } else {
                    diagnostics.push(
                        "BMSON branch object before evaluation or with invalid y; ignored".into(),
                    );
                    None
                }
            })
            .collect(),
    )
}

fn resolve_bpm(chart: &mut IntermediateChart) {
    for object in &mut chart.objects {
        if let IntermediateObjectKind::SetExtendedBpm { bpm_key } = object.kind
            && let Some(bpm) = chart.resources.bpm_table.iter().find(|bpm| bpm.key == bpm_key)
        {
            object.kind = IntermediateObjectKind::SetBpm { bpm: bpm.bpm };
        }
    }
}

fn remap_sounds(
    chart: &mut IntermediateChart,
    all: &mut IntermediateResources,
) -> Result<(), ImportError> {
    let mut keys = BTreeMap::new();
    for wav in &chart.resources.wavs {
        let key = all
            .wavs
            .iter()
            .map(|wav| wav.key)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("too many BMSON branch sound regions"))?;
        keys.insert(wav.key, key);
        all.wavs.push(WavDef { key, ..wav.clone() });
    }
    for plan in &chart.resources.metric_sound_slices {
        all.metric_sound_slices.push(MetricSoundSlice { wav_key: keys[&plan.wav_key], ..*plan });
    }
    for object in &mut chart.objects {
        let key = match &mut object.kind {
            IntermediateObjectKind::VisibleNote { wav_key, .. }
            | IntermediateObjectKind::InvisibleNote { wav_key, .. }
            | IntermediateObjectKind::LongChannelNote { wav_key, .. }
            | IntermediateObjectKind::MineNote { wav_key, .. } => wav_key.as_mut(),
            IntermediateObjectKind::Bgm { wav_key } => Some(wav_key),
            _ => None,
        };
        if let Some(key) = key {
            *key = keys.get(key).copied().unwrap_or(*key);
        }
    }
    for layer in chart.layered_note_sounds.iter_mut().chain(&mut chart.long_end_sounds) {
        layer.wav_key = keys.get(&layer.wav_key).copied().unwrap_or(layer.wav_key);
    }
    Ok(())
}
