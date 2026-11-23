//! OpenLR2's construction of the playable lanes (LR2_bmsload.cpp), used only by
//! LEGACY. BMZ's shared long-note resolver differs from LR2 in several corner
//! cases, so LEGACY rebuilds the lanes from the chart text with LR2's steps:
//!
//! 1. `channelConvert` maps channels (`x6` is the scratch, `x7` is dropped,
//!    `x8`/`x9` are keys 6/7) and objects are ordered by position, then by the
//!    converted channel (`CMP_NotesByBmsTiming`).
//! 2. In time order, LN channel pairs and `#LNOBJ` ends share one pending slot
//!    per key (`intArr2`). An LN channel object closes whatever is pending,
//!    including a normal note waiting for an `#LNOBJ` end, and an `#LNOBJ`
//!    note with nothing pending stays a normal note (`//CHECK: haha`).
//! 3. Per lane (`intArr`), an object inside the previous LN, up to and
//!    including its end, is dropped; a second object at the same time is
//!    dropped unless it starts an LN, which then replaces the earlier one. Mines
//!    take part but are not counted as notes.
//!
//! Dropped objects play no sound, as in LR2.
use std::collections::{BTreeMap, HashMap};

use bmz_core::ids::{NoteId, SoundId};
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};

use super::intermediate::IntermediateChart;
use super::normalize::object_to_tick;
use crate::model::{LongNoteMode, LongNotePair, LongNoteStyle, NoteEvent, NoteKind, PlayableChart};

/// One note-like object of the chart text after `channelConvert`.
#[derive(Clone, Copy, Debug)]
pub(super) struct RawNote {
    pub position: f64,
    /// Converted LR2 channel: 10..=29 notes, 50..=69 LN, 130..=149 mines.
    pub op: i32,
    /// The object's value in the chart base (`val` / `mine`).
    pub value: u16,
    /// bms-rs object id index, used to find the `#WAV` definition.
    pub wav_key: u16,
    pub tick: u64,
}

/// `channelConvert` for a non-PMS, non-DSC chart. Returns `None` for dropped
/// channels; channels that are not notes, LN or mines are not passed here.
fn convert(channel: i32) -> Option<i32> {
    let block = channel / 10 * 10;
    match channel - block {
        0 | 7 => None,
        1..=5 => Some(channel),
        6 => Some(block),
        8 => Some(block + 6),
        9 => Some(block + 7),
        _ => None,
    }
}

fn is_note_block(channel: i32) -> bool {
    matches!(channel, 11..=19 | 21..=29 | 51..=59 | 61..=69 | 131..=139 | 141..=149)
}

/// Collects the note, LN and mine objects of the resolved chart text.
pub(super) fn raw_notes(text: &str, source: &IntermediateChart) -> Vec<RawNote> {
    let base62 = source.metadata.base62_obj_ids;
    let mut notes = Vec::new();
    for line in text.lines().map(str::trim) {
        let bytes = line.as_bytes();
        if bytes.len() < 9
            || bytes[0] != b'#'
            || !bytes[1..4].iter().all(u8::is_ascii_digit)
            || bytes[6] != b':'
            || !bytes[5].is_ascii_digit()
        {
            continue;
        }
        let measure = line[1..4].parse::<u32>().unwrap();
        if measure as usize >= source.measures.len() {
            continue;
        }
        let high = bytes[4].to_ascii_uppercase();
        let high = if high.is_ascii_digit() { high - b'0' } else { high.wrapping_sub(b'A') + 10 };
        let channel = i32::from(high) * 10 + i32::from(bytes[5] - b'0');
        if !is_note_block(channel) {
            continue;
        }
        let Some(op) = convert(channel) else {
            continue;
        };
        let data = &bytes[7..];
        let divisions = data.len() / 2;
        for (index, token) in data.as_chunks::<2>().0.iter().enumerate() {
            let Some(value) = super::openlr2::slot_key(token, base62).filter(|v| *v != 0) else {
                continue;
            };
            // bms-rs indexes object ids in base 62 order, upper case unless #BASE 62.
            let key_token = if base62 { *token } else { token.map(|b| b.to_ascii_uppercase()) };
            let Some(wav_key) = super::openlr2::slot_key(&key_token, true) else {
                continue;
            };
            let object = super::intermediate::IntermediateObject {
                measure,
                position_num: index as u32,
                position_den: divisions as u32,
                kind: super::intermediate::IntermediateObjectKind::BarLine,
            };
            let Ok(tick) = object_to_tick(&object, &source.measures) else {
                continue;
            };
            notes.push(RawNote {
                position: f64::from(measure) + index as f64 / divisions as f64,
                op,
                value,
                wav_key,
                tick: tick.0 / crate::timing::IMPORT_TICK_SCALE,
            });
        }
    }
    notes.sort_by(|a, b| a.position.total_cmp(&b.position).then(a.op.cmp(&b.op)));
    notes
}

/// LR2 lane number (0..=7 P1, 10..=17 P2) -> BMZ lane.
fn lane_of(op: i32) -> Option<Lane> {
    match op - 10 {
        0 => Some(Lane::Scratch),
        lane @ 1..=7 => Some(Lane::ALL[lane as usize]),
        10 => Some(Lane::Scratch2),
        lane @ 11..=17 => Some(Lane::ALL[(lane - 11) as usize + Lane::Key8.index()]),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct Object {
    raw: RawNote,
    op: i32,
    time_ms: f64,
    ln_end: Option<(f64, u64)>,
    mine: bool,
    removed: bool,
    lnobj_end: bool,
}

/// Rebuilds `lane_notes` / `long_notes` / `total_notes`. `times_ms` holds
/// LR2's double-precision real time of each position (`f64::to_bits`, already
/// including the origin `shift`), `shift_ticks` the origin measure insertion.
/// Invisible notes are kept as imported.
pub(super) fn rebuild(
    chart: &mut PlayableChart,
    source: &IntermediateChart,
    raw: &[RawNote],
    lnobj: Option<u16>,
    times_ms: &BTreeMap<u64, f64>,
    shift: f64,
    shift_ticks: u64,
) {
    let floor_us = |ms: f64| TimeUs((ms * 1000.0).floor() as i64);
    let mut objects: Vec<Object> = raw
        .iter()
        .map(|&raw| Object {
            raw,
            op: raw.op,
            time_ms: times_ms.get(&(raw.position + shift).to_bits()).copied().unwrap_or(0.0),
            ln_end: None,
            mine: false,
            removed: false,
            lnobj_end: false,
        })
        .collect();

    // Step 2: the timing loop's LN pairing with the shared `intArr2`.
    let mut pending: HashMap<i32, usize> = HashMap::new();
    for i in 0..objects.len() {
        let op = objects[i].op;
        if (50..=69).contains(&op) {
            match pending.remove(&(op - 40)) {
                None => {
                    objects[i].op -= 40;
                    pending.insert(objects[i].op, i);
                }
                Some(start) => {
                    objects[start].ln_end = Some((objects[i].time_ms, objects[i].raw.tick));
                    objects[i].removed = true;
                }
            }
        } else if let Some(lnobj) = lnobj
            && (10..=29).contains(&op)
        {
            if objects[i].raw.value != lnobj || !pending.contains_key(&op) {
                pending.insert(op, i);
            } else {
                let start = pending.remove(&op).unwrap();
                objects[start].ln_end = Some((objects[i].time_ms, objects[i].raw.tick));
                objects[start].lnobj_end = true;
                objects[i].removed = true;
            }
        }
        if (130..=149).contains(&objects[i].op) {
            objects[i].op -= 120;
            objects[i].mine = true;
        }
    }

    // Step 3: per-lane placement (`intArr`), in the same order.
    let sound_of: HashMap<u16, SoundId> = source
        .resources
        .wavs
        .iter()
        .enumerate()
        .map(|(index, wav)| (wav.key, SoundId(index as u32)))
        .collect();
    let mut last: HashMap<i32, usize> = HashMap::new();
    let mut placed: Vec<Vec<usize>> = vec![Vec::new(); 20];
    let mut note_count: i64 = 0;
    // An unpaired LN start has realTiming_ln == -1 and plays as a single note.
    let ln_end_ms = |o: &Object| o.ln_end.map_or(-1.0, |(ms, _)| ms);
    for i in 0..objects.len() {
        let o = objects[i];
        if o.removed || !(10..=29).contains(&o.op) {
            continue;
        }
        let lane = (o.op - 10) as usize;
        match last.get(&o.op).copied() {
            None => {
                placed[lane].push(i);
                last.insert(o.op, i);
                note_count += i64::from(!o.mine);
            }
            Some(previous) if o.time_ms > ln_end_ms(&objects[previous]) => {
                if o.time_ms != objects[previous].time_ms {
                    placed[lane].push(i);
                    last.insert(o.op, i);
                    note_count += i64::from(!o.mine);
                } else if o.time_ms < ln_end_ms(&o) {
                    // A long note replaces a single note at the same time;
                    // LR2 decrements the count even if the replaced object
                    // was a mine.
                    note_count -= 1;
                    placed[lane].pop();
                    placed[lane].push(i);
                    last.insert(o.op, i);
                    note_count += i64::from(!o.mine);
                }
            }
            Some(_) => {}
        }
    }

    // Materialize BMZ notes. Invisible notes are kept with their ids.
    let mut next_id =
        chart.lane_notes.iter().flatten().map(|n| n.id.0).max().map_or(1, |id| id + 1);
    let mut id = || {
        let current = NoteId(next_id);
        next_id += 1;
        current
    };
    let mut lane_notes: [Vec<NoteEvent>; bmz_core::lane::LANE_COUNT] =
        std::array::from_fn(|index| {
            chart.lane_notes[index]
                .iter()
                .filter(|n| n.kind == NoteKind::Invisible)
                .cloned()
                .collect()
        });
    let mut long_notes = Vec::new();
    for (lane_index, indices) in placed.iter().enumerate() {
        let Some(lane) = lane_of(lane_index as i32 + 10) else {
            continue;
        };
        for &i in indices {
            let o = objects[i];
            let tick = ChartTick(o.raw.tick.saturating_add(shift_ticks));
            let time = floor_us(o.time_ms);
            let sound = if o.mine { None } else { sound_of.get(&o.raw.wav_key).copied() };
            let start_id = id();
            match o.ln_end.filter(|(end_ms, _)| o.time_ms < *end_ms) {
                Some((end_ms, end_tick)) if !o.mine => {
                    let end_id = id();
                    let end_tick = ChartTick(end_tick.saturating_add(shift_ticks));
                    let end_time = floor_us(end_ms);
                    lane_notes[lane.index()].push(NoteEvent {
                        id: start_id,
                        lane,
                        kind: NoteKind::LongStart,
                        tick,
                        time,
                        sound,
                        layered_sounds: Vec::new(),
                        damage: None,
                    });
                    lane_notes[lane.index()].push(NoteEvent {
                        id: end_id,
                        lane,
                        kind: NoteKind::LongEnd,
                        tick: end_tick,
                        time: end_time,
                        sound: None,
                        layered_sounds: Vec::new(),
                        damage: None,
                    });
                    long_notes.push(LongNotePair {
                        lane,
                        style: if o.lnobj_end {
                            LongNoteStyle::LnObj
                        } else {
                            LongNoteStyle::ChannelPair
                        },
                        mode: Some(LongNoteMode::Ln),
                        start_note_id: start_id,
                        end_note_id: end_id,
                        start_tick: tick,
                        end_tick,
                        start_time: time,
                        end_time,
                        sound,
                    });
                }
                _ => lane_notes[lane.index()].push(NoteEvent {
                    id: start_id,
                    lane,
                    kind: if o.mine { NoteKind::Mine } else { NoteKind::Tap },
                    tick,
                    time,
                    sound,
                    layered_sounds: Vec::new(),
                    damage: o.mine.then_some(f64::from(o.raw.value)),
                }),
            }
        }
    }
    for notes in &mut lane_notes {
        notes.sort_by_key(|n| (n.time, n.tick));
    }
    long_notes.sort_by_key(|p| (p.start_time, p.lane.index()));
    chart.lane_notes = lane_notes;
    chart.long_notes = long_notes;
    chart.metadata.free_zones.clear();
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.total_notes = note_count.max(0) as u32;
}
