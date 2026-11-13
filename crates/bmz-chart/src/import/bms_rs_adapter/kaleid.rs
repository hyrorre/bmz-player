use super::*;
use crate::import::intermediate::KaleidNoteKind;
use crate::kaleid::KaleidFeatures;

pub(super) struct Extracted {
    pub text: String,
    pub objects: Vec<IntermediateObject>,
    pub features: KaleidFeatures,
    pub diagnostics: Vec<String>,
}

/// Only extension-bearing sources need the additional resolved-text passes.
/// Inspect every static branch here; the actual feature flags still come from
/// the selected static branch and all candidates of each dynamic branch.
pub(super) fn needs_interpretation(text: &str, layout: ChartKeyLayout) -> bool {
    text.lines().any(|line| {
        let Some(body) = line.trim_start().strip_prefix('#') else { return false };
        if let Some((head, _)) = split_bms_channel_command(body) {
            let ch = head[head.len() - 2..].to_ascii_uppercase();
            return ch == "ST"
                || (!layout.is_pms()
                    && matches!(ch.as_str(), "SF" | "SG" | "SH" | "SI" | "17" | "27"))
                || matches!(ch.as_bytes(), [b'F'..=b'O', b'1'..=b'9']);
        }
        let command = body.split_whitespace().next().unwrap_or("");
        [
            "LNMODE",
            "CNOBJ",
            "HCNOBJ",
            "HLNOBJ",
            "CONDITIONAL",
            "WHEN",
            "ELSEWHEN",
            "DEFAULT",
            "ENDCONDITIONAL",
            "BMD",
            "SOUNDFONT",
            "TARGETHASH",
            "LOADINGFILE",
            "READYFILE",
            "BGAOVERFLOW",
        ]
        .iter()
        .any(|name| command.eq_ignore_ascii_case(name))
    })
}

/// Extract after static RANDOM and before bms-rs. No filesystem timestamps participate.
pub(super) fn extract<T: KeyLayoutMapper>(text: &str, layout: ChartKeyLayout) -> Extracted {
    let mut out = Extracted {
        text: String::new(),
        objects: Vec::new(),
        features: Default::default(),
        diagnostics: Vec::new(),
    };
    let base62 = text.lines().any(|line| line.trim().eq_ignore_ascii_case("#BASE 62"));
    for (line_no, line) in text.lines().enumerate() {
        let parsed = line.trim().strip_prefix('#').and_then(|s| s.split_once(':'));
        let Some((head, data)) = parsed.filter(|(h, _)| h.is_ascii() && h.len() >= 5) else {
            out.text.push_str(line);
            out.text.push('\n');
            continue;
        };
        let (measure, channel) = head.split_at(head.len() - 2);
        let ch = channel.to_ascii_uppercase();
        let free =
            !layout.is_pms() && matches!(ch.as_str(), "SF" | "SG" | "SH" | "SI" | "17" | "27");
        let kind = match ch.as_bytes() {
            [b'F' | b'G', b'1'..=b'9'] => Some(KaleidNoteKind::Potion),
            [b'H' | b'I', b'1'..=b'9'] => Some(KaleidNoteKind::DummyTap),
            [b'J' | b'K', b'1'..=b'9'] => Some(KaleidNoteKind::DummyLong),
            [b'L' | b'M', b'1'..=b'9'] => Some(KaleidNoteKind::DummyMine),
            [b'N' | b'O', b'1'..=b'9'] => Some(KaleidNoteKind::DummyPotion),
            _ => None,
        };
        if !free && kind.is_none() && ch != "ST" {
            out.text.push_str(line);
            out.text.push('\n');
            continue;
        }
        out.text.push('\n');
        let data = data.split_whitespace().next().unwrap_or("");
        let Ok(measure) = measure.parse::<u32>() else { continue };
        if measure > MAX_SUPPORTED_MEASURE || !data.is_ascii() || data.len() % 2 != 0 {
            out.diagnostics.push(format!("line {}: invalid Kaleid channel {ch}", line_no + 1));
            continue;
        }
        let lane = if free {
            Some(if matches!(ch.as_str(), "SH" | "SI" | "27") {
                Lane::Scratch2
            } else {
                Lane::Scratch
            })
        } else if kind.is_some() {
            let side = if matches!(ch.as_bytes()[0], b'G' | b'I' | b'K' | b'M' | b'O') {
                '2'
            } else {
                '1'
            };
            let mapped = format!("{side}{}", ch.as_bytes()[1] as char);
            match read_channel(&mapped) {
                Some(Channel::Note { channel_id }) => {
                    T::from_channel_id(channel_id).and_then(|m| map_lane(layout, m.side(), m.key()))
                }
                _ => None,
            }
        } else {
            None
        };
        for (index, bytes) in data.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let token = std::str::from_utf8(bytes).expect("ASCII checked");
            let Ok(id) = ObjId::try_from(token, base62) else {
                out.diagnostics
                    .push(format!("line {}: invalid Kaleid object {token:?}", line_no + 1));
                continue;
            };
            if id.as_u16() == 0 {
                continue;
            }
            let object_kind = if free {
                out.features.free_zone = true;
                IntermediateObjectKind::FreeZone {
                    lane: lane.unwrap(),
                    paired: matches!(ch.as_str(), "SG" | "SI"),
                    length_ticks: None,
                }
            } else if let Some(kind) = kind {
                let Some(lane) = lane else { continue };
                out.features.potion |= kind == KaleidNoteKind::Potion;
                out.features.dummy |= kind != KaleidNoteKind::Potion;
                IntermediateObjectKind::KaleidNote {
                    lane,
                    kind,
                    mode: None,
                    recovery: if kind == KaleidNoteKind::Potion {
                        // Like mine damage, recovery is base36 even under BASE 62.
                        u32::from_str_radix(token, 36).unwrap_or(0) as f64
                    } else {
                        0.0
                    },
                }
            } else {
                out.features.bar_line = true;
                IntermediateObjectKind::BarLine
            };
            out.objects.push(IntermediateObject {
                measure,
                position_num: index as u32,
                position_den: (data.len() / 2) as u32,
                kind: object_kind,
            });
        }
    }
    out
}
