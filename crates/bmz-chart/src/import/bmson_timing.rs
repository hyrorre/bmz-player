//! BMSON `lines` を BMS 小節長 / `ObjTime` へ変換する。

use std::collections::HashMap;
use std::num::NonZeroU8;
use std::path::PathBuf;

use bms_rs::bms::command::channel::{Channel, Key, NoteChannelId, NoteKind, PlayerSide};
use bms_rs::bms::command::time::ObjTime;
use bms_rs::bms::command::{LnMode, StringValue};
use bms_rs::bms::model::Bms;
use bms_rs::bms::model::obj::{SectionLenChangeObj, WavObj};
use bms_rs::bms::prelude::{
    BgaLayer, BgaObj, BpmChangeObj, KeyLayoutMapper, ObjId, ScrollingFactorObj, Track,
};
use bms_rs::bmson::bmson_to_bms::BmsonToBmsWarning;
use bms_rs::bmson::prelude::FinF64;
use bms_rs::bmson::{BarLine, BgaId, Bmson, SoundChannel};

use crate::model::SoundSlice;

/// BMSON 小節境界 (pulse)。
#[derive(Debug, Clone)]
pub struct MeasureBoundaries {
    pub starts: Vec<u64>,
    pub default_step: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BmsonLaneLayout {
    Beat5,
    Beat7,
    Beat10,
    Beat14,
    Pms5,
    Pms9,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BmsonObjectPosition {
    pub measure: u32,
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BmsonLongNoteExtension {
    pub lane: NonZeroU8,
    pub start: BmsonObjectPosition,
    pub end: BmsonObjectPosition,
    pub mode: Option<LnMode>,
    pub hln: bool,
    pub end_wav_key: Option<u16>,
    pub explicit_end_sound: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BmsonSoundSliceExtension {
    pub wav_key: u16,
    pub slice: SoundSlice,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BmsonLayeredSoundExtension {
    pub lane: NonZeroU8,
    pub position: BmsonObjectPosition,
    pub wav_key: u16,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BmsonBgaKind {
    Base,
    Layer,
    Poor,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BmsonBgaExtension {
    pub position: BmsonObjectPosition,
    pub bmp_key: u16,
    pub kind: BmsonBgaKind,
}

#[derive(Debug, Default)]
pub(crate) struct BmsonRebuildInfo {
    pub long_notes: Vec<BmsonLongNoteExtension>,
    pub sound_slices: Vec<BmsonSoundSliceExtension>,
    pub layered_sounds: Vec<BmsonLayeredSoundExtension>,
    pub bga_events: Vec<BmsonBgaExtension>,
    pub mine_channel_wav_keys: Vec<u16>,
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
        consider(event.y.0, 0);
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
    hln_notes: &std::collections::HashSet<(usize, usize)>,
    warnings: &mut Vec<BmsonToBmsWarning>,
) -> BmsonRebuildInfo {
    let wav_by_path: HashMap<PathBuf, ObjId> =
        bms.wav.wav_files.iter().map(|(id, path)| (path.clone(), *id)).collect();
    let bga_id_to_obj_id = bmson
        .bga
        .bga_header
        .iter()
        .zip(ObjId::all_values())
        .map(|(header, obj_id)| (header.id, obj_id))
        .collect::<HashMap<_, _>>();

    let mut wav_obj_id_issuer = ObjId::all_values();
    let mut bpm_def_obj_id_issuer = ObjId::all_values();
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

    let sound_slice_times = bmson_metric_times_us(
        bmson,
        bmson.sound_channels.iter().flat_map(|channel| channel.notes.iter().map(|note| note.y.0)),
    );

    let mut rebuild_info = BmsonRebuildInfo::default();
    let mut sound_channel_wav_ids = Vec::with_capacity(bmson.sound_channels.len());
    for sound_channel in &bmson.sound_channels {
        let wav_path = PathBuf::from(sound_channel.name.as_ref());
        let mut wav_ids = HashMap::new();
        for (pulse, slice) in bmson_sound_slice_plan(sound_channel, &sound_slice_times) {
            let obj_id = wav_obj_id_issuer.next().unwrap_or_else(|| {
                warnings.push(BmsonToBmsWarning::WavObjIdOutOfRange);
                ObjId::null()
            });
            bms.wav.wav_files.entry(obj_id).or_insert_with(|| wav_path.clone());
            wav_ids.insert(pulse, obj_id);
            rebuild_info
                .sound_slices
                .push(BmsonSoundSliceExtension { wav_key: obj_id.as_u16(), slice });
        }
        sound_channel_wav_ids.push(wav_ids);
    }

    let mut up_wav_by_position = HashMap::new();
    for (sound_channel, wav_ids) in bmson.sound_channels.iter().zip(&sound_channel_wav_ids) {
        for note in &sound_channel.notes {
            if note.up == Some(true)
                && let Some(obj_id) = wav_ids.get(&note.y.0)
            {
                up_wav_by_position.insert((note.x, note.y.0), *obj_id);
            }
        }
    }

    let mut seen_key_notes = HashMap::new();
    for (channel_index, (sound_channel, wav_ids)) in
        bmson.sound_channels.iter().zip(sound_channel_wav_ids).enumerate()
    {
        for (note_index, note) in sound_channel.notes.iter().enumerate() {
            if note.up == Some(true) {
                continue;
            }
            let time = pulse_to_obj_time(note.y.0, boundaries);
            let Some(obj_id) = wav_ids.get(&note.y.0).copied() else {
                continue;
            };
            if let Some(lane) = note.x {
                let position = (lane, note.y.0);
                if let Some(first_channel_index) = seen_key_notes.get(&position) {
                    if *first_channel_index != channel_index {
                        rebuild_info.layered_sounds.push(BmsonLayeredSoundExtension {
                            lane,
                            position: bmson_object_position(time),
                            wav_key: obj_id.as_u16(),
                        });
                    }
                    continue;
                }
                seen_key_notes.insert(position, channel_index);
            }
            let kind = if note.l > 0 { NoteKind::Long } else { NoteKind::Visible };
            let Some(channel_id) = bmson_note_channel::<T>(note.x, kind, lane_layout) else {
                continue;
            };
            bms.wav.notes.push_note(WavObj { offset: time, channel_id, wav_id: obj_id });
            if note.l > 0
                && let Some(lane) = note.x
            {
                let end_pulse = note.y.0.saturating_add(note.l);
                let explicit_end_wav = up_wav_by_position.get(&(note.x, end_pulse)).copied();
                let end_time = pulse_to_obj_time(end_pulse, boundaries);
                bms.wav.notes.push_note(WavObj {
                    offset: end_time,
                    channel_id,
                    wav_id: explicit_end_wav.unwrap_or(obj_id),
                });
                rebuild_info.long_notes.push(BmsonLongNoteExtension {
                    lane,
                    start: bmson_object_position(time),
                    end: bmson_object_position(end_time),
                    mode: note.t,
                    hln: hln_notes.contains(&(channel_index, note_index)),
                    end_wav_key: explicit_end_wav.map(ObjId::as_u16),
                    explicit_end_sound: explicit_end_wav.is_some(),
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
        rebuild_info.mine_channel_wav_keys.push(obj_id.as_u16());
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
            let Some(channel_id) =
                bmson_note_channel::<T>(key_event.x, NoteKind::Invisible, lane_layout)
            else {
                continue;
            };
            bms.wav.notes.push_note(WavObj { offset: time, channel_id, wav_id: obj_id });
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
        rebuild_info.bga_events.push(BmsonBgaExtension {
            position: bmson_object_position(time),
            bmp_key: obj_id.as_u16(),
            kind: BmsonBgaKind::Base,
        });
        bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer: BgaLayer::Base });
    }
    for bga_event in &bmson.bga.layer_events {
        let time = pulse_to_obj_time(bga_event.y.0, boundaries);
        let obj_id = get_bga_obj_id(&bga_event.id);
        rebuild_info.bga_events.push(BmsonBgaExtension {
            position: bmson_object_position(time),
            bmp_key: obj_id.as_u16(),
            kind: BmsonBgaKind::Layer,
        });
        bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer: BgaLayer::Overlay });
    }
    for bga_event in &bmson.bga.poor_events {
        let time = pulse_to_obj_time(bga_event.y.0, boundaries);
        let obj_id = get_bga_obj_id(&bga_event.id);
        rebuild_info.bga_events.push(BmsonBgaExtension {
            position: bmson_object_position(time),
            bmp_key: obj_id.as_u16(),
            kind: BmsonBgaKind::Poor,
        });
        bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer: BgaLayer::Poor });
    }

    rebuild_info
}

fn bmson_object_position(time: ObjTime) -> BmsonObjectPosition {
    BmsonObjectPosition {
        measure: time.track().0 as u32,
        numerator: time.numerator() as u32,
        denominator: time.denominator().get() as u32,
    }
}

fn bmson_sound_slice_plan(
    channel: &SoundChannel<'_>,
    metric_times_us: &HashMap<u64, u64>,
) -> Vec<(u64, SoundSlice)> {
    let mut positions = channel.notes.iter().map(|note| (note.y.0, note.c)).collect::<Vec<_>>();
    positions.sort_by_key(|(pulse, _)| *pulse);
    positions.dedup_by_key(|(pulse, _)| *pulse);

    let mut start_us = 0_u64;
    let mut slices = Vec::with_capacity(positions.len());
    for (index, &(pulse, continues)) in positions.iter().enumerate() {
        if !continues {
            start_us = 0;
        }
        let duration_us = positions.get(index + 1).and_then(|&(next_pulse, next_continues)| {
            next_continues
                .then(|| metric_times_us[&next_pulse].saturating_sub(metric_times_us[&pulse]))
        });
        slices.push((pulse, SoundSlice { start_us, duration_us }));
        if let Some(duration_us) = duration_us {
            start_us = start_us.saturating_add(duration_us);
        }
    }
    slices
}

fn bmson_metric_times_us(
    bmson: &Bmson<'_>,
    target_pulses: impl IntoIterator<Item = u64>,
) -> HashMap<u64, u64> {
    let resolution = bmson.info.resolution.get().max(1);
    let mut targets = target_pulses.into_iter().collect::<Vec<_>>();
    targets.sort_unstable();
    targets.dedup();

    let mut bpm_by_pulse = HashMap::new();
    let mut stop_durations_by_pulse = HashMap::<u64, Vec<u64>>::new();
    for event in &bmson.bpm_events {
        // 同一 pulse の BPM は source order の最後の定義が有効。
        bpm_by_pulse.insert(event.y.0, event.bpm.get().max(1.0));
    }
    for event in &bmson.stop_events {
        stop_durations_by_pulse.entry(event.y.0).or_default().push(event.duration);
    }

    let mut event_pulses =
        bpm_by_pulse.keys().chain(stop_durations_by_pulse.keys()).copied().collect::<Vec<_>>();
    event_pulses.sort_unstable();
    event_pulses.dedup();

    let mut current_pulse = 0_u64;
    let mut current_time = 0_u64;
    let mut current_bpm = bmson.info.init_bpm.get().max(1.0);
    let mut event_index = 0;
    let mut times = HashMap::with_capacity(targets.len());

    for target_pulse in targets {
        while event_pulses.get(event_index).is_some_and(|pulse| *pulse < target_pulse) {
            let pulse = event_pulses[event_index];
            current_time = current_time.saturating_add(bmson_pulse_span_us(
                pulse.saturating_sub(current_pulse),
                resolution,
                current_bpm,
            ));
            current_pulse = pulse;
            if let Some(bpm) = bpm_by_pulse.get(&pulse) {
                current_bpm = *bpm;
            }
            if let Some(durations) = stop_durations_by_pulse.get(&pulse) {
                for &duration in durations {
                    current_time = current_time.saturating_add(bmson_pulse_span_us(
                        duration,
                        resolution,
                        current_bpm,
                    ));
                }
            }
            event_index += 1;
        }

        times.insert(
            target_pulse,
            current_time.saturating_add(bmson_pulse_span_us(
                target_pulse.saturating_sub(current_pulse),
                resolution,
                current_bpm,
            )),
        );
    }

    times
}

fn bmson_pulse_span_us(pulses: u64, resolution: u64, bpm: f64) -> u64 {
    let us = pulses as f64 * 60_000_000.0 / resolution.max(1) as f64 / bpm.max(1.0);
    us.round().clamp(0.0, u64::MAX as f64) as u64
}

fn bmson_note_channel<T: KeyLayoutMapper>(
    lane: Option<NonZeroU8>,
    kind: NoteKind,
    layout: BmsonLaneLayout,
) -> Option<NoteChannelId> {
    let Some(lane_value) = lane.map(std::num::NonZero::get) else {
        return Some(Channel::Bgm.into());
    };
    let mapped = match layout {
        BmsonLaneLayout::Pms5 if lane_value <= 5 => {
            Some((Key::Key(lane_value), PlayerSide::Player1))
        }
        BmsonLaneLayout::Pms9 if lane_value <= 9 => {
            Some((Key::Key(lane_value), PlayerSide::Player1))
        }
        BmsonLaneLayout::Beat5 | BmsonLaneLayout::Beat7 => {
            let max_key = if layout == BmsonLaneLayout::Beat5 { 5 } else { 7 };
            match lane_value {
                key if key <= max_key => Some((Key::Key(key), PlayerSide::Player1)),
                8 => Some((Key::Scratch(1), PlayerSide::Player1)),
                _ => None,
            }
        }
        BmsonLaneLayout::Beat10 | BmsonLaneLayout::Beat14 => {
            let (adjusted_lane, side) = if lane_value > 8 {
                (lane_value - 8, PlayerSide::Player2)
            } else {
                (lane_value, PlayerSide::Player1)
            };
            let max_key = if layout == BmsonLaneLayout::Beat10 { 5 } else { 7 };
            match adjusted_lane {
                key if key <= max_key => Some((Key::Key(key), side)),
                8 => Some((Key::Scratch(1), side)),
                _ => None,
            }
        }
        BmsonLaneLayout::Pms5 | BmsonLaneLayout::Pms9 => None,
    };
    mapped.map(|(key, side)| T::new(side, kind, key).to_channel_id())
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

    #[test]
    fn metric_time_cache_matches_bpm_and_stop_segment_rounding() {
        let json = r#"{
            "version":"1.0.0",
            "info":{"title":"t","artist":"a","genre":"g","level":1,"init_bpm":120,"resolution":240},
            "bpm_events":[{"y":240,"bpm":240}],
            "stop_events":[{"y":480,"duration":240},{"y":480,"duration":120}],
            "sound_channels":[]
        }"#;
        let bmson = bms_rs::bmson::parse_bmson(json).bmson.unwrap();

        let times = bmson_metric_times_us(&bmson, [0, 240, 480, 720]);

        assert_eq!(times[&0], 0);
        assert_eq!(times[&240], 500_000);
        assert_eq!(times[&480], 750_000);
        assert_eq!(times[&720], 1_375_000);
    }
}
