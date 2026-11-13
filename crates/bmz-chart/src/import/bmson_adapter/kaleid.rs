use super::*;
use crate::import::intermediate::{KaleidNoteKind, WavDef};
use crate::timing::{IMPORT_TICK_SCALE, TICKS_PER_MEASURE};
use serde_json::Value;

const NOTE_ARRAYS: &[&str] =
    &["potion_notes", "dummy_notes", "dummy_mine_notes", "dummy_potion_notes"];

pub(super) fn max_pulse(value: &Value, resolution: u64) -> u64 {
    let singles = NOTE_ARRAYS
        .iter()
        .copied()
        .chain(["speed_events"])
        .flat_map(|key| value[key].as_array().into_iter().flatten());
    let zones = value["free_zone_channels"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|ch| ch["notes"].as_array().into_iter().flatten());
    singles
        .map(|n| n["y"].as_u64().unwrap_or(0).saturating_add(n["l"].as_u64().unwrap_or(0)))
        .chain(zones.map(|n| {
            n["y"]
                .as_u64()
                .unwrap_or(0)
                .saturating_add(n["l"].as_u64().filter(|l| *l > 0).unwrap_or(resolution))
        }))
        .max()
        .unwrap_or(0)
}

pub(super) fn append(
    chart: &mut IntermediateChart,
    value: &Value,
    layout: BmsonLaneLayout,
    boundaries: &super::super::bmson_timing::MeasureBoundaries,
    resolution: u64,
) {
    for key in NOTE_ARRAYS.iter().copied().chain(["free_zone_channels", "speed_events"]) {
        if value.get(key).is_some_and(|items| !items.is_array()) {
            diagnostic(chart, key);
        }
    }
    if let Some(mode) = value.pointer("/info/ln_type")
        && super::parse_ln_type_json(mode).is_none()
        && mode.as_u64() != Some(0)
    {
        diagnostic(chart, "info.ln_type");
    }
    for note in value["sound_channels"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|channel| channel["notes"].as_array().into_iter().flatten())
    {
        if let Some(mode) = note.get("t")
            && super::parse_ln_type_json(mode).is_none()
            && mode.as_u64() != Some(0)
        {
            diagnostic(chart, "sound_channels.notes.t");
        }
    }
    for &key in NOTE_ARRAYS {
        for note in value[key].as_array().into_iter().flatten() {
            let (Some(x), Some(y)) =
                (note["x"].as_u64().and_then(|x| u8::try_from(x).ok()), note["y"].as_u64())
            else {
                diagnostic(chart, key);
                continue;
            };
            let Some(lane) = bmson_lane(x, layout) else {
                diagnostic(chart, key);
                continue;
            };
            let length = note["l"].as_u64().unwrap_or(0);
            let kind = match key {
                "potion_notes" => KaleidNoteKind::Potion,
                "dummy_mine_notes" => KaleidNoteKind::DummyMine,
                "dummy_potion_notes" => KaleidNoteKind::DummyPotion,
                _ if length > 0 => KaleidNoteKind::DummyLong,
                _ => KaleidNoteKind::DummyTap,
            };
            let recovery = note["recovery"].as_f64().unwrap_or(0.0);
            if kind == KaleidNoteKind::Potion && (!recovery.is_finite() || recovery <= 0.0) {
                diagnostic(chart, key);
                continue;
            }
            let mode = match note["t"].as_u64() {
                Some(1) => Some(LongNoteMode::Ln),
                Some(2) => Some(LongNoteMode::Cn),
                Some(3) => Some(LongNoteMode::Hcn),
                Some(4) => Some(LongNoteMode::Hln),
                _ => None,
            };
            push(
                chart,
                boundaries,
                y,
                IntermediateObjectKind::KaleidNote { lane, kind, recovery, mode },
            );
            if kind == KaleidNoteKind::DummyLong {
                push(
                    chart,
                    boundaries,
                    y.saturating_add(length),
                    IntermediateObjectKind::KaleidNote { lane, kind, recovery: 0.0, mode },
                );
            }
            chart.metadata.kaleid.source.potion |= kind == KaleidNoteKind::Potion;
            chart.metadata.kaleid.source.dummy |= kind != KaleidNoteKind::Potion;
        }
    }
    for channel in value["free_zone_channels"].as_array().into_iter().flatten() {
        let name = channel["name"].as_str().unwrap_or("");
        let wav = if name.is_empty() {
            None
        } else {
            let next = chart.resources.wavs.iter().map(|w| w.key).max().unwrap_or(0).checked_add(1);
            if let Some(key) = next {
                chart.resources.wavs.push(WavDef { key, path: PathBuf::from(name), slice: None });
            }
            next
        };
        for note in channel["notes"].as_array().into_iter().flatten() {
            let (Some(lane), Some(y)) = (
                note["x"]
                    .as_u64()
                    .and_then(|x| u8::try_from(x).ok())
                    .and_then(|x| bmson_lane(x, layout))
                    .filter(|l| matches!(l, Lane::Scratch | Lane::Scratch2)),
                note["y"].as_u64(),
            ) else {
                diagnostic(chart, "free_zone_channels");
                continue;
            };
            let length = note["l"].as_u64().filter(|l| *l > 0).unwrap_or(resolution);
            let length_ticks =
                (u128::from(length) * u128::from(TICKS_PER_MEASURE) * u128::from(IMPORT_TICK_SCALE)
                    / (u128::from(resolution.max(1)) * 4))
                    .min(u128::from(u64::MAX)) as u64;
            push(
                chart,
                boundaries,
                y,
                IntermediateObjectKind::FreeZone {
                    lane,
                    paired: false,
                    length_ticks: Some(length_ticks),
                },
            );
            if let Some(wav_key) = wav {
                push(chart, boundaries, y, IntermediateObjectKind::Bgm { wav_key });
            }
            chart.metadata.kaleid.source.free_zone = true;
        }
    }
    for event in value["speed_events"].as_array().into_iter().flatten() {
        if let (Some(y), Some(factor)) =
            (event["y"].as_u64(), event["rate"].as_f64().filter(|v| v.is_finite() && *v > 0.0))
        {
            push(chart, boundaries, y, IntermediateObjectKind::SetSpeed { factor });
        } else {
            diagnostic(chart, "speed_events");
        }
    }
    chart.metadata.kaleid.declared =
        chart.metadata.kaleid.declared.union(chart.metadata.kaleid.source);
}

fn push(
    chart: &mut IntermediateChart,
    boundaries: &super::super::bmson_timing::MeasureBoundaries,
    y: u64,
    kind: IntermediateObjectKind,
) {
    let time = pulse_to_obj_time(y, boundaries);
    chart.objects.push(IntermediateObject {
        measure: time.track().0 as u32,
        position_num: time.numerator() as u32,
        position_den: time.denominator().get() as u32,
        kind,
    });
}

fn diagnostic(chart: &mut IntermediateChart, key: &str) {
    chart.metadata.kaleid.diagnostics.push(format!("invalid BMSON {key} object; ignored"));
}
