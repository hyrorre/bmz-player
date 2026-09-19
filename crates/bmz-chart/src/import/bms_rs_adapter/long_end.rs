use super::super::intermediate::IntermediateLayeredSound;
use super::*;
use std::collections::BTreeSet;

pub(super) fn typed_markers(
    text: &str,
    base62: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<(u16, LongNoteMode)> {
    let mut markers = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some(body) = line.trim_start().strip_prefix('#') else { continue };
        let (command, args) =
            body.trim_start().split_once(char::is_whitespace).unwrap_or((body, ""));
        let (mode, code) = match command.to_ascii_uppercase().as_str() {
            "CNOBJ" => (LongNoteMode::Cn, "InvalidCnobj"),
            "HCNOBJ" => (LongNoteMode::Hcn, "InvalidHcnobj"),
            "HLNOBJ" => (LongNoteMode::Hln, "InvalidHlnobj"),
            _ => continue,
        };
        let token = args.split_whitespace().next().unwrap_or("");
        match ObjId::try_from(token, base62) {
            Ok(id) if id.as_u16() != 0 => {
                markers.retain(|(_, previous_mode)| *previous_mode != mode);
                markers.push((id.as_u16(), mode));
            }
            _ => warnings.push(ImportWarning::ParserDiagnostic {
                code: code.to_string(),
                message: format!("line {} #{command} has invalid object id {token:?}", index + 1),
            }),
        }
    }
    markers
}

/// 同位置の重ね置きを、parser の通常ノーツ上書きより前の情報から復元する。
/// マーカー位置だけ置換し、通常位置の既存の後勝ち規則には触れない。
pub(super) fn restore_marker_overlays<T: KeyLayoutMapper>(
    text: &str,
    layout: ChartKeyLayout,
    base62: bool,
    chart: &mut IntermediateChart,
) {
    if chart.lnobj_wav_key.is_none() && chart.typed_lnobj.is_empty() {
        return;
    }
    let mut positions: BTreeMap<(u32, u32, u32, usize), BTreeSet<u16>> = BTreeMap::new();
    for line in text.lines() {
        let Some((header, payload)) =
            line.trim().strip_prefix('#').and_then(|line| line.split_once(':'))
        else {
            continue;
        };
        if !header.is_ascii() || header.len() < 5 {
            continue;
        }
        let (measure, channel) = header.split_at(header.len() - 2);
        let Ok(measure) = measure.parse::<u32>() else { continue };
        if measure > MAX_SUPPORTED_MEASURE {
            continue;
        }
        let Some(Channel::Note { channel_id }) = read_channel(&channel.to_ascii_uppercase()) else {
            continue;
        };
        let Some(mapping) = T::from_channel_id(channel_id) else { continue };
        if mapping.kind() != BmsNoteKind::Visible {
            continue;
        }
        let Some(lane) = map_lane(layout, mapping.side(), mapping.key()) else { continue };
        let payload = payload.trim();
        if payload.len() % 2 != 0 {
            continue;
        }
        let count = payload.len() / 2;
        for (index, bytes) in payload.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let Ok(token) = std::str::from_utf8(bytes) else { continue };
            let Ok(id) = ObjId::try_from(token, base62) else { continue };
            if id.as_u16() == 0 {
                continue;
            }
            let Some(time) = ObjTime::new(u64::from(measure), index as u64, count as u64) else {
                continue;
            };
            positions
                .entry((
                    track_of(time),
                    time.numerator() as u32,
                    time.denominator().get() as u32,
                    lane.index(),
                ))
                .or_default()
                .insert(id.as_u16());
        }
    }
    let mut replacements = BTreeMap::new();
    for (position, keys) in positions {
        let marker = chart
            .typed_lnobj
            .iter()
            .filter(|(key, _)| keys.contains(key))
            .max_by_key(|(_, mode)| crate::import::long_note::marker_priority(*mode))
            .map(|(key, _)| *key)
            .or_else(|| chart.lnobj_wav_key.filter(|key| keys.contains(key)));
        let Some(marker) = marker else { continue };
        replacements.insert(position, marker);
        for key in keys {
            if chart.lnobj_wav_key == Some(key)
                || chart.typed_lnobj.iter().any(|(id, _)| *id == key)
            {
                continue;
            }
            chart.long_end_sounds.push(IntermediateLayeredSound {
                lane: Lane::ALL[position.3],
                measure: position.0,
                position_num: position.1,
                position_den: position.2,
                wav_key: key,
            });
        }
    }
    chart.objects.retain(|object| {
        let IntermediateObjectKind::VisibleNote { lane, .. } = object.kind else { return true };
        !replacements.contains_key(&(
            object.measure,
            object.position_num,
            object.position_den,
            lane.index(),
        ))
    });
    chart.objects.extend(replacements.into_iter().map(
        |((measure, position_num, position_den, lane), key)| IntermediateObject {
            measure,
            position_num,
            position_den,
            kind: IntermediateObjectKind::VisibleNote { lane: Lane::ALL[lane], wav_key: Some(key) },
        },
    ));
}
