//! BMSON `lines` を BMS 小節長 / `ObjTime` へ変換する。

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU8;
use std::path::PathBuf;

use bms_rs::bms::command::StringValue;
use bms_rs::bms::command::channel::{Channel, Key, NoteChannelId, NoteKind, PlayerSide};
use bms_rs::bms::command::time::ObjTime;
use bms_rs::bms::model::Bms;
use bms_rs::bms::model::obj::{SectionLenChangeObj, WavObj};
use bms_rs::bms::prelude::{
    BgaLayer, BgaObj, BpmChangeObj, KeyLayoutMapper, ObjId, ScrollingFactorObj, StopObj, Track,
};
use bms_rs::bmson::bmson_to_bms::BmsonToBmsWarning;
use bms_rs::bmson::prelude::FinF64;
use bms_rs::bmson::{BarLine, BgaId, Bmson};
use strict_num_extended::NonNegativeF64;

/// BMSON 小節境界 (pulse)。
#[derive(Debug, Clone)]
pub struct MeasureBoundaries {
    pub starts: Vec<u64>,
    pub default_step: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BmsonLaneLayout {
    Beat,
    Pms,
}

impl MeasureBoundaries {
    pub fn measure_index_for_pulse(&self, pulse: u64) -> usize {
        self.starts.partition_point(|&start| start <= pulse).saturating_sub(1)
    }

    pub fn measure_pulse_len(&self, index: usize) -> u64 {
        let start = self.starts.get(index).copied().unwrap_or(0);
        let end = self
            .starts
            .get(index + 1)
            .copied()
            .unwrap_or_else(|| start.saturating_add(self.default_step));
        end.saturating_sub(start).max(1)
    }
}

pub fn max_pulse_in_bmson(bmson: &Bmson<'_>) -> u64 {
    let mut max = 0_u64;

    let mut consider = |y: u64, length: u64| {
        max = max.max(y.saturating_add(length));
    };

    for channel in &bmson.sound_channels {
        for note in &channel.notes {
            consider(note.y.0, note.l);
        }
    }
    for channel in &bmson.mine_channels {
        for note in &channel.notes {
            consider(note.y.0, 0);
        }
    }
    for channel in &bmson.key_channels {
        for note in &channel.notes {
            consider(note.y.0, 0);
        }
    }
    for event in &bmson.bpm_events {
        consider(event.y.0, 0);
    }
    for event in &bmson.stop_events {
        consider(event.y.0, event.duration);
    }
    for event in &bmson.scroll_events {
        consider(event.y.0, 0);
    }
    for event in &bmson.bga.bga_events {
        consider(event.y.0, 0);
    }
    for event in &bmson.bga.layer_events {
        consider(event.y.0, 0);
    }
    for event in &bmson.bga.poor_events {
        consider(event.y.0, 0);
    }
    if let Some(lines) = &bmson.lines {
        for line in lines {
            max = max.max(line.y.0);
        }
    }

    max
}

pub fn build_measure_boundaries(
    lines: Option<&[BarLine]>,
    resolution: u64,
    max_pulse: u64,
) -> MeasureBoundaries {
    let default_step = 4_u64.saturating_mul(resolution);

    match lines {
        None | Some([]) => {
            let mut starts = vec![0_u64];
            while starts.last().copied().unwrap_or(0) <= max_pulse {
                let next = starts.last().copied().unwrap_or(0).saturating_add(default_step);
                if next == *starts.last().unwrap_or(&0) {
                    break;
                }
                starts.push(next);
            }
            MeasureBoundaries { starts, default_step }
        }
        Some(lines) => {
            let mut starts: Vec<u64> = lines.iter().map(|line| line.y.0).collect();
            starts.sort_unstable();
            starts.dedup();
            if starts.first().copied() != Some(0) {
                starts.insert(0, 0);
            }
            while starts.last().copied().unwrap_or(0) <= max_pulse {
                let next = starts.last().copied().unwrap_or(0).saturating_add(default_step);
                if next == *starts.last().unwrap_or(&0) {
                    break;
                }
                starts.push(next);
            }
            MeasureBoundaries { starts, default_step }
        }
    }
}

pub fn pulse_to_obj_time(pulse: u64, boundaries: &MeasureBoundaries) -> ObjTime {
    let index = boundaries.measure_index_for_pulse(pulse);
    let start = boundaries.starts.get(index).copied().unwrap_or(0);
    let end = boundaries
        .starts
        .get(index + 1)
        .copied()
        .unwrap_or_else(|| start.saturating_add(boundaries.default_step));
    let num = pulse.saturating_sub(start);
    let den = end.saturating_sub(start).max(1);
    ObjTime::new(index as u64, num, den).expect("measure pulse length should be non-zero")
}

pub fn apply_section_lengths(bms: &mut Bms, boundaries: &MeasureBoundaries, resolution: u64) {
    bms.section_len.section_len_changes.clear();
    let quarter = 4_u64.saturating_mul(resolution).max(1);
    for index in 0..boundaries.starts.len().saturating_sub(1) {
        let pulse_len = boundaries.measure_pulse_len(index);
        let section_len = pulse_len as f64 / quarter as f64;
        let length = FinF64::new(section_len).unwrap_or(FinF64::ONE);
        bms.section_len.section_len_changes.insert(
            Track(index as u64),
            SectionLenChangeObj { track: Track(index as u64), length },
        );
    }
}

pub(crate) fn rebuild_bms_timing_from_bmson<T: KeyLayoutMapper>(
    bms: &mut Bms,
    bmson: &Bmson<'_>,
    boundaries: &MeasureBoundaries,
    lane_layout: BmsonLaneLayout,
    warnings: &mut Vec<BmsonToBmsWarning>,
) {
    let wav_by_path: HashMap<PathBuf, ObjId> =
        bms.wav.wav_files.iter().map(|(id, path)| (path.clone(), *id)).collect();
    let mut bga_id_to_obj_id = HashMap::new();
    for header in &bmson.bga.bga_header {
        let path = PathBuf::from(header.name.as_ref());
        if let Some(obj_id) =
            bms.bmp.bmp_files.iter().find_map(|(id, bmp)| (bmp.file == path).then_some(*id))
        {
            bga_id_to_obj_id.insert(header.id, obj_id);
        }
    }

    let mut wav_obj_id_issuer = ObjId::all_values();
    let mut bpm_def_obj_id_issuer = ObjId::all_values();
    let mut stop_def_obj_id_issuer = ObjId::all_values();
    let mut scroll_def_obj_id_issuer = ObjId::all_values();

    bms.bpm.bpm_changes.clear();
    bms.bpm.bpm_defs.clear();
    bms.stop.stops.clear();
    bms.stop.stop_defs.clear();
    bms.scroll.scrolling_factor_changes.clear();
    bms.scroll.scroll_defs.clear();
    bms.wav.notes = Default::default();
    bms.bmp.bga_changes.clear();

    apply_section_lengths(bms, boundaries, bmson.info.resolution.get());

    for bpm_event in &bmson.bpm_events {
        let time = pulse_to_obj_time(bpm_event.y.0, boundaries);
        let bpm = bpm_event.bpm;
        let bpm_def_id = bpm_def_obj_id_issuer.next().unwrap_or_else(|| {
            warnings.push(BmsonToBmsWarning::BpmDefOutOfRange);
            ObjId::null()
        });
        bms.bpm.bpm_defs.insert(bpm_def_id, StringValue::from_value(bpm));
        bms.bpm.bpm_changes.insert(time, BpmChangeObj { time, bpm });
    }

    for stop_event in &bmson.stop_events {
        let time = pulse_to_obj_time(stop_event.y.0, boundaries);
        let duration = NonNegativeF64::new(stop_event.duration as f64)
            .expect("stop duration should be finite");
        let stop_def_id = stop_def_obj_id_issuer.next().unwrap_or_else(|| {
            warnings.push(BmsonToBmsWarning::StopDefOutOfRange);
            ObjId::null()
        });
        bms.stop.stop_defs.insert(stop_def_id, StringValue::from_value(duration));
        bms.stop.stops.insert(time, StopObj { time, duration });
    }

    for scroll_event in &bmson.scroll_events {
        let time = pulse_to_obj_time(scroll_event.y.0, boundaries);
        let factor = scroll_event.rate;
        let scroll_def_id = scroll_def_obj_id_issuer.next().unwrap_or_else(|| {
            warnings.push(BmsonToBmsWarning::ScrollDefOutOfRange);
            ObjId::null()
        });
        bms.scroll.scroll_defs.insert(scroll_def_id, StringValue::from_value(factor));
        bms.scroll.scrolling_factor_changes.insert(time, ScrollingFactorObj { time, factor });
    }

    let mut seen_key_notes = HashSet::new();
    for sound_channel in &bmson.sound_channels {
        let wav_path = PathBuf::from(sound_channel.name.as_ref());
        let obj_id = wav_by_path.get(&wav_path).copied().unwrap_or_else(|| {
            wav_obj_id_issuer.next().unwrap_or_else(|| {
                warnings.push(BmsonToBmsWarning::WavObjIdOutOfRange);
                ObjId::null()
            })
        });
        bms.wav.wav_files.entry(obj_id).or_insert(wav_path);

        for note in &sound_channel.notes {
            if let Some(x) = note.x
                && !seen_key_notes.insert((x, note.y.0))
            {
                continue;
            }
            let time = pulse_to_obj_time(note.y.0, boundaries);
            let kind = if note.l > 0 { NoteKind::Long } else { NoteKind::Visible };
            let channel_id = bmson_note_channel::<T>(note.x, kind, lane_layout);
            bms.wav.notes.push_note(WavObj { offset: time, channel_id, wav_id: obj_id });
            if note.l > 0 && note.x.is_some() {
                bms.wav.notes.push_note(WavObj {
                    offset: pulse_to_obj_time(note.y.0.saturating_add(note.l), boundaries),
                    channel_id,
                    wav_id: obj_id,
                });
            }
        }
    }

    for mine_channel in &bmson.mine_channels {
        let wav_path = PathBuf::from(mine_channel.name.as_ref());
        let obj_id = wav_by_path.get(&wav_path).copied().unwrap_or_else(|| {
            wav_obj_id_issuer.next().unwrap_or_else(|| {
                warnings.push(BmsonToBmsWarning::WavObjIdOutOfRange);
                ObjId::null()
            })
        });
        bms.wav.wav_files.entry(obj_id).or_insert(wav_path);

        for mine_event in &mine_channel.notes {
            let time = pulse_to_obj_time(mine_event.y.0, boundaries);
            bms.wav.notes.push_note(WavObj {
                offset: time,
                channel_id: bmson_note_channel::<T>(mine_event.x, NoteKind::Landmine, lane_layout),
                wav_id: obj_id,
            });
        }
    }

    for key_channel in &bmson.key_channels {
        let wav_path = PathBuf::from(key_channel.name.as_ref());
        let obj_id = wav_by_path.get(&wav_path).copied().unwrap_or_else(|| {
            wav_obj_id_issuer.next().unwrap_or_else(|| {
                warnings.push(BmsonToBmsWarning::WavObjIdOutOfRange);
                ObjId::null()
            })
        });
        bms.wav.wav_files.entry(obj_id).or_insert(wav_path);

        for key_event in &key_channel.notes {
            let time = pulse_to_obj_time(key_event.y.0, boundaries);
            bms.wav.notes.push_note(WavObj {
                offset: time,
                channel_id: bmson_note_channel::<T>(key_event.x, NoteKind::Invisible, lane_layout),
                wav_id: obj_id,
            });
        }
    }

    let mut get_bga_obj_id = |bga_id: &BgaId| -> ObjId {
        bga_id_to_obj_id.get(bga_id).copied().unwrap_or_else(|| {
            warnings.push(BmsonToBmsWarning::BgaEventObjIdOutOfRange);
            ObjId::null()
        })
    };

    for bga_event in &bmson.bga.bga_events {
        let time = pulse_to_obj_time(bga_event.y.0, boundaries);
        let obj_id = get_bga_obj_id(&bga_event.id);
        bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer: BgaLayer::Base });
    }
    for bga_event in &bmson.bga.layer_events {
        let time = pulse_to_obj_time(bga_event.y.0, boundaries);
        let obj_id = get_bga_obj_id(&bga_event.id);
        bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer: BgaLayer::Overlay });
    }
    for bga_event in &bmson.bga.poor_events {
        let time = pulse_to_obj_time(bga_event.y.0, boundaries);
        let obj_id = get_bga_obj_id(&bga_event.id);
        bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer: BgaLayer::Poor });
    }
}

fn bmson_note_channel<T: KeyLayoutMapper>(
    lane: Option<NonZeroU8>,
    kind: NoteKind,
    layout: BmsonLaneLayout,
) -> NoteChannelId {
    let Some(lane_value) = lane.map(std::num::NonZero::get) else {
        return Channel::Bgm.into();
    };
    let mapped = match layout {
        BmsonLaneLayout::Pms if lane_value <= 9 => {
            Some((Key::Key(lane_value), PlayerSide::Player1))
        }
        BmsonLaneLayout::Beat => {
            let (adjusted_lane, side) = if lane_value > 8 {
                (lane_value - 8, PlayerSide::Player2)
            } else {
                (lane_value, PlayerSide::Player1)
            };
            match adjusted_lane {
                key @ 1..=7 => Some((Key::Key(key), side)),
                8 => Some((Key::Scratch(1), side)),
                _ => None,
            }
        }
        BmsonLaneLayout::Pms => None,
    };
    mapped
        .map_or_else(|| Channel::Bgm.into(), |(key, side)| T::new(side, kind, key).to_channel_id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_to_obj_time_uses_irregular_measure_lengths() {
        let boundaries =
            MeasureBoundaries { starts: vec![0, 960, 1_680, 2_640], default_step: 960 };
        let time = pulse_to_obj_time(1_680, &boundaries);
        assert_eq!(time.track().0, 2);
        assert_eq!(time.numerator(), 0);
    }

    #[test]
    fn pulse_to_obj_time_supports_three_four_meter() {
        let boundaries = build_measure_boundaries(
            Some(&[
                BarLine { y: bms_rs::bmson::pulse::PulseNumber(720) },
                BarLine { y: bms_rs::bmson::pulse::PulseNumber(1_440) },
            ]),
            240,
            1_000,
        );
        let time = pulse_to_obj_time(720, &boundaries);
        assert_eq!(time.track().0, 1);
        assert_eq!(time.numerator(), 0);
    }

    #[test]
    fn build_measure_boundaries_defaults_to_common_time() {
        let boundaries = build_measure_boundaries(None, 240, 2_000);
        assert_eq!(boundaries.starts, vec![0, 960, 1_920, 2_880]);
    }
}
