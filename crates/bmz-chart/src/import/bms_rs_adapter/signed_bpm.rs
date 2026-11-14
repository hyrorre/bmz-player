use super::*;

pub(super) fn negative_bpm(raw: &str) -> Option<f64> {
    let bpm = raw.trim().parse::<f64>().ok()?;
    (bpm.is_finite() && bpm < 0.0).then_some(bpm)
}

/// bms-rs keeps the original text of a negative definition but its BPM objects
/// only accept positive values. Resolve channel 08 from the selected source
/// when it references a negative definition. This keeps the sign even when a
/// positive and a negative definition have the same magnitude.
pub(super) fn resolve(
    bms: &Bms,
    source: &PreparedBmsSource,
    base62: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Option<Vec<IntermediateObject>> {
    if !bms.bpm.bpm_defs.values().any(|def| negative_bpm(def.raw()).is_some()) {
        return None;
    }

    let mut changes = BTreeMap::new();
    let mut missing = Vec::new();
    for line in source.parse_text.lines() {
        let message = if let Some(message) =
            sparse_marker_id(line).and_then(|id| source.sparse_messages.get(id))
        {
            if message.channel == "08" {
                resolve_message(
                    bms,
                    base62,
                    (message.measure, message.object_count, &message.objects),
                    &mut changes,
                    &mut missing,
                );
            }
            continue;
        } else {
            ObjectMessageLine::parse(line)
                .filter(|message| message.channel == "08")
                .and_then(|message| message.paired_objects())
        };
        let Some((measure, slots, objects)) = message else {
            continue;
        };
        resolve_message(bms, base62, (measure, slots, &objects), &mut changes, &mut missing);
    }
    if !changes.values().any(|&bpm| bpm < 0.0) {
        return None;
    }
    missing.sort_unstable();
    missing.dedup();
    warnings.extend(missing.into_iter().map(|key| ImportWarning::MissingBpmDefinition { key }));
    Some(
        changes
            .into_iter()
            .map(|(time, bpm)| IntermediateObject {
                measure: track_of(time),
                position_num: time.numerator() as u32,
                position_den: time.denominator().get() as u32,
                kind: if bpm < 0.0 {
                    IntermediateObjectKind::SetNegativeBmsBpm { magnitude: -bpm }
                } else {
                    IntermediateObjectKind::SetBpm { bpm }
                },
            })
            .collect(),
    )
}

fn resolve_message(
    bms: &Bms,
    base62: bool,
    message: (u64, u64, &[SparseBmsObject]),
    changes: &mut BTreeMap<ObjTime, f64>,
    missing: &mut Vec<u16>,
) {
    let (measure, slots, objects) = message;
    for object in objects {
        let Ok(id) = ObjId::try_from(object.token(), base62) else {
            continue;
        };
        let Some(time) = ObjTime::new(measure, object.index, slots) else {
            continue;
        };
        let Some(def) = bms.bpm.bpm_defs.get(&id) else {
            missing.push(id.as_u16());
            continue;
        };
        if let Some(bpm) =
            def.value().as_ref().ok().map(|bpm| bpm.get()).or_else(|| negative_bpm(def.raw()))
        {
            // Match the adapter's last-valid-object precedence at a shared position.
            changes.insert(time, bpm);
        }
    }
}
