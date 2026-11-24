//! OpenLR2's BMS object pipeline (`ParseBmsFile` in LR2_bmsload.cpp), used only
//! by LEGACY. LEGACY reads the chart text again and repeats LR2's steps so that
//! note times, long notes and the note count come out bit for bit the same:
//!
//! 1. Lines are read as `fgets` with a 102,400-byte buffer does, and every
//!    non-zero object of every channel becomes one entry of `bmsobj`, in file
//!    order. Values use LR2's base conversion: a character outside the base
//!    counts as 0 instead of making the object invalid. One measure-length
//!    entry per measure 0..1000 and the "sound only" BGA entry are appended.
//! 2. `bmsobj` is sorted with the UCRT `qsort` and `CMP_NotesByBmsTiming`.
//!    The comparator truncates position differences to 1/100000 measure, so it
//!    is not a consistent order; the result depends on the sort algorithm,
//!    which is ported exactly ([`ucrt_qsort`]).
//! 3. The real-time loop walks that order: positions can step backwards, and
//!    every step is added to the running double-precision time. LN channel
//!    pairs and `#LNOBJ` ends share one pending slot per key (`intArr2`).
//! 4. `bmsobj` is sorted again with `CMP_NotesByRealTiming`, which truncates
//!    the difference to whole milliseconds.
//! 5. Per lane (`intArr`), an object inside the previous LN, up to and
//!    including its end, is dropped; a second object at the same time is
//!    dropped unless it starts an LN, which then replaces the earlier one.
//!    Mines take part but are not counted as notes. Dropped objects play no
//!    sound, as in LR2.
use std::collections::HashMap;

use bmz_core::ids::{NoteId, SoundId};
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};

use super::intermediate::{IntermediateChart, IntermediateObject, IntermediateObjectKind};
use super::normalize::object_to_tick;
use crate::model::{LongNoteMode, LongNotePair, LongNoteStyle, NoteEvent, NoteKind, PlayableChart};

/// `fgets(buf, 102400, file)` stores at most 102,399 characters per call.
const FGETS_LIMIT: usize = 102_399;
const MEASURE_OBJECTS: u32 = 1000;

/// Where a `bmsobj` entry came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Origin {
    /// The measure-length entry LR2 adds for every measure.
    Measure(u32),
    /// An object of a channel line.
    Data,
    /// The BGA base LR2 adds when a chart has no BGA (`soundonly`).
    SoundOnly,
}

/// One `NoteStruct` of `bmsobj`.
#[derive(Clone, Copy, Debug)]
pub(super) struct LrObject {
    /// `bmsTiming` before the real-time loop.
    pub position: f64,
    /// Converted LR2 channel; changed by the loop as in LR2.
    pub op: i32,
    pub val: f64,
    pub origin: Origin,
    /// BMZ tick of the object, when it maps to one.
    pub tick: Option<u64>,
    /// bms-rs object id index of the token, used to find the `#WAV` definition.
    pub wav_key: Option<u16>,
    pub realtime: f64,
    pub realtime_ln: f64,
    pub ln_end_tick: Option<u64>,
    pub mine: f64,
    pub lnobj_end: bool,
}

/// `bmsobj` after the real-time loop and the second sort.
pub(super) struct Loaded {
    pub objects: Vec<LrObject>,
    /// LR2 moved every object one measure later for a note at position 0.
    pub origin_shift: bool,
}

/// `Base36ToInt` / `Base62ToInt`: characters outside the base count as 0.
fn lr2_base(ch1: u8, ch2: u8, base62: bool) -> i32 {
    let digit = |c: u8| -> i32 {
        match c {
            b'a'..=b'z' if base62 => i32::from(c - b'a') + 36,
            b'a'..=b'z' => i32::from(c - b'a') + 10,
            b'1'..=b'9' => i32::from(c - b'0'),
            b'A'..=b'Z' => i32::from(c - b'A') + 10,
            _ => 0,
        }
    };
    digit(ch1) * if base62 { 62 } else { 36 } + digit(ch2)
}

/// `HEXcharToInt`: characters outside hexadecimal count as 0.
fn lr2_hex(ch1: u8, ch2: u8) -> i32 {
    let digit = |c: u8| -> i32 {
        match c {
            b'a'..=b'f' => i32::from(c - b'a') + 10,
            b'1'..=b'9' => i32::from(c - b'0'),
            b'A'..=b'F' => i32::from(c - b'A') + 10,
            _ => 0,
        }
    };
    (digit(ch1) << 4) + digit(ch2)
}

/// `channelConvert`.
pub(super) fn channel_convert(channel: i32, dsc: bool, pms: bool) -> i32 {
    // Within each block of ten: x0 and x7 are dropped (x7 is a PMS key), x6
    // is the scratch, x8/x9 are keys 6/7. DSC and PMS remap further.
    let block = channel / 10 * 10;
    let note_block = matches!(block, 10 | 20 | 30 | 40 | 50 | 60 | 130 | 140);
    if !note_block {
        return if channel == 0 { -1 } else { channel };
    }
    let p1 = matches!(block, 10 | 30 | 50 | 130);
    let unit = channel - block;
    match (p1, unit) {
        (_, 0) => -1,
        (true, 1..=5) if dsc && block != 130 => channel + 1,
        (true, 1..=5) => channel,
        (true, 6) if dsc => block + 1,
        (true, 6) if pms => block + 8,
        (true, 6) => block,
        (true, 7) if pms => block + 9,
        (true, 7) => -1,
        (true, 8) if dsc => block + 7,
        (true, 8) => block + 6,
        (true, 9) if dsc => block + 8,
        (true, 9) => block + 7,
        (false, 1) => channel,
        (false, 2..=5) if pms => block - 10 + unit + 4,
        (false, 2..=5) => channel,
        (false, 6) if dsc => block - 10 + 9,
        (false, 6) => block,
        (false, 7) => -1,
        (false, 8) => block + 6,
        (false, 9) => block + 7,
        _ => channel,
    }
}

/// The C `atof` used for measure lengths.
fn atof(text: &[u8]) -> f64 {
    super::openlr2::atof(std::str::from_utf8(text).unwrap_or(""))
}

/// The UCRT `qsort` (ucrt/stdlib/qsort.cpp), step for step, so that an
/// inconsistent comparator gives the same order as in OpenLR2.
pub(super) fn ucrt_qsort<T>(v: &mut [T], mut cmp: impl FnMut(&T, &T) -> i32) {
    if v.len() < 2 {
        return;
    }
    let mut c = |v: &[T], a: isize, b: isize| cmp(&v[a as usize], &v[b as usize]);
    let swap = |v: &mut [T], a: isize, b: isize| v.swap(a as usize, b as usize);
    let mut stack: Vec<(isize, isize)> = Vec::new();
    let mut lo: isize = 0;
    let mut hi: isize = v.len() as isize - 1;
    loop {
        let size = hi - lo + 1;
        let mut next = None;
        if size <= 8 {
            let mut h = hi;
            while h > lo {
                let mut max = lo;
                for p in lo + 1..=h {
                    if c(v, p, max) > 0 {
                        max = p;
                    }
                }
                swap(v, max, h);
                h -= 1;
            }
        } else {
            let mut mid = lo + size / 2;
            if c(v, lo, mid) > 0 {
                swap(v, lo, mid);
            }
            if c(v, lo, hi) > 0 {
                swap(v, lo, hi);
            }
            if c(v, mid, hi) > 0 {
                swap(v, mid, hi);
            }
            let mut loguy = lo;
            let mut higuy = hi;
            loop {
                if mid > loguy {
                    loop {
                        loguy += 1;
                        if !(loguy < mid && c(v, loguy, mid) <= 0) {
                            break;
                        }
                    }
                }
                if mid <= loguy {
                    loop {
                        loguy += 1;
                        if !(loguy <= hi && c(v, loguy, mid) <= 0) {
                            break;
                        }
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > mid && c(v, higuy, mid) > 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                swap(v, loguy, higuy);
                if mid == higuy {
                    mid = loguy;
                }
            }
            higuy += 1;
            if mid < higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > mid && c(v, higuy, mid) == 0) {
                        break;
                    }
                }
            }
            if mid >= higuy {
                loop {
                    higuy -= 1;
                    if !(higuy > lo && c(v, higuy, mid) == 0) {
                        break;
                    }
                }
            }
            if higuy - lo >= hi - loguy {
                if lo < higuy {
                    stack.push((lo, higuy));
                }
                if loguy < hi {
                    next = Some((loguy, hi));
                }
            } else {
                if loguy < hi {
                    stack.push((loguy, hi));
                }
                if lo < higuy {
                    next = Some((lo, higuy));
                }
            }
        }
        match next.or_else(|| stack.pop()) {
            Some((l, h)) => (lo, hi) = (l, h),
            None => return,
        }
    }
}

/// `CMP_NotesByBmsTiming`.
fn cmp_bms_timing(n1: &LrObject, n2: &LrObject) -> i32 {
    let (a, b) = (n1.position, n2.position);
    if a >= 0.0 && b < 0.0 {
        return -1;
    }
    if a < 0.0 && b < 0.0 {
        return 0;
    }
    if a < 0.0 && b >= 0.0 {
        return 1;
    }
    if b == a {
        return n1.op - n2.op;
    }
    (a * 100000.0 - b * 100000.0) as i32
}

/// `CMP_NotesByRealTiming`.
fn cmp_real_timing(n1: &LrObject, n2: &LrObject) -> i32 {
    (n1.realtime - n2.realtime) as i32
}

/// The resolved chart text split as `fgets` reads it.
fn lr2_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().flat_map(|line| {
        // The newline counts toward the buffer; a longer line is read in
        // pieces, each handled as a line of its own.
        let pieces = if line.len() < FGETS_LIMIT || !line.is_ascii() {
            vec![line]
        } else {
            (0..line.len())
                .step_by(FGETS_LIMIT)
                .map(|s| &line[s..(s + FGETS_LIMIT).min(line.len())])
                .collect()
        };
        pieces.into_iter()
    })
}

/// Runs steps 1-4 on the resolved chart text.
pub(super) fn load(text: &str, source: &IntermediateChart, pms: bool) -> Loaded {
    let mut objects = Vec::new();
    let mut measure_length = vec![1.0f64; 5000];
    let mut bpm_slots: HashMap<i32, f64> = HashMap::new();
    let mut stop_slots: HashMap<i32, f64> = HashMap::new();
    let mut now_bpm: Option<f64> = None;
    let mut lnobj = -1;
    let mut base62 = false;
    let mut dsc = false;
    let mut sound_only = true;
    for line in lr2_lines(text) {
        let org = line.trim_matches([' ', '\t', '\r', '\n']).as_bytes();
        if org.first() != Some(&b'#') {
            continue;
        }
        let upper = org.to_ascii_uppercase();
        let at = |i: usize| upper.get(i).copied().unwrap_or(0);
        let org_at = |i: usize| org.get(i).copied().unwrap_or(0);
        let rest =
            |from: usize| std::str::from_utf8(upper.get(from..).unwrap_or(&[])).unwrap_or("");
        if at(1).is_ascii_digit()
            && at(2).is_ascii_digit()
            && at(3).is_ascii_digit()
            && (at(5).is_ascii_digit() || (at(4) == b'S' && at(5) == b'C'))
        {
            let measure = u32::from(at(1) - b'0') * 100
                + u32::from(at(2) - b'0') * 10
                + u32::from(at(3) - b'0');
            let mut channel = if at(4) == b'S' && at(5) == b'C' {
                2888
            } else {
                i32::from(at(5) - b'0') + lr2_hex(b'0', at(4)) * 10
            };
            if channel == 4 || channel == 7 {
                sound_only = false;
            }
            channel = channel_convert(channel, dsc, pms);
            let divisions = (upper.len() as i64 - 7) / 2;
            if channel == 2 {
                let value = atof(upper.get(7..).unwrap_or(&[]));
                if value > 0.0 && measure <= 4999 {
                    measure_length[measure as usize] = value;
                }
                continue;
            }
            if channel != 3 && channel <= 0 {
                continue;
            }
            for index in 0..divisions.max(0) as usize {
                let ii = index * 2 + 7;
                let value = if channel == 3 {
                    lr2_hex(at(ii), at(ii + 1))
                } else {
                    lr2_base(org_at(ii), org_at(ii + 1), base62)
                };
                if value == 0 {
                    continue;
                }
                let object = IntermediateObject {
                    measure,
                    position_num: index as u32,
                    position_den: divisions as u32,
                    kind: IntermediateObjectKind::BarLine,
                };
                let tick = ((measure as usize) < source.measures.len())
                    .then(|| object_to_tick(&object, &source.measures).ok())
                    .flatten()
                    .map(|t| t.0 / crate::timing::IMPORT_TICK_SCALE);
                // bms-rs indexes object ids in base 62 order, upper case unless #BASE 62.
                let token = [org_at(ii), org_at(ii + 1)];
                let key = if base62 { token } else { token.map(|b| b.to_ascii_uppercase()) };
                objects.push(LrObject {
                    position: f64::from(measure) + index as f64 / divisions as f64,
                    op: channel,
                    val: f64::from(value),
                    origin: Origin::Data,
                    tick,
                    wav_key: super::openlr2::slot_key(&key, true),
                    realtime: -1.0,
                    realtime_ln: -1.0,
                    ln_end_tick: None,
                    mine: 0.0,
                    lnobj_end: false,
                });
            }
        } else if upper.starts_with(b"#BPM ") {
            now_bpm = Some(super::openlr2::atof(rest(5)));
        } else if upper.starts_with(b"#FP/DSC") {
            dsc = true;
        } else if upper.starts_with(b"#LNOBJ ") {
            lnobj = lr2_base(org_at(7), org_at(8), base62);
        } else if upper.starts_with(b"#BASE 62") {
            base62 = true;
        } else if upper.starts_with(b"#BPM") {
            let key = lr2_base(org_at(4), org_at(5), base62);
            if (1..SINGLE_SLOTS).contains(&key) {
                bpm_slots.insert(key, super::openlr2::atof(rest(7)));
            }
        } else if upper.starts_with(b"#STOP") {
            let key = lr2_base(org_at(5), org_at(6), base62);
            let value = super::openlr2::atol(rest(8));
            if (1..SINGLE_SLOTS).contains(&key) && value > 0 {
                stop_slots.insert(key, f64::from(value));
            }
        }
    }
    if sound_only {
        objects.push(LrObject {
            position: 0.0,
            op: 4,
            val: 1295.0,
            origin: Origin::SoundOnly,
            tick: None,
            wav_key: None,
            realtime: -1.0,
            realtime_ln: -1.0,
            ln_end_tick: None,
            mine: 0.0,
            lnobj_end: false,
        });
    }
    for measure in 0..MEASURE_OBJECTS {
        objects.push(LrObject {
            position: f64::from(measure),
            op: 2,
            val: measure_length[measure as usize],
            origin: Origin::Measure(measure),
            tick: source
                .measures
                .get(measure as usize)
                .map(|m| m.start_tick.0 / crate::timing::IMPORT_TICK_SCALE),
            wav_key: None,
            realtime: -1.0,
            realtime_ln: -1.0,
            ln_end_tick: None,
            mine: 0.0,
            lnobj_end: false,
        });
    }

    // Step 2.
    ucrt_qsort(&mut objects, cmp_bms_timing);
    let mut origin_shift = false;
    for i in 0..objects.len() {
        if objects[i].position != 0.0 {
            break;
        }
        let op = objects[i].op;
        if (10..29).contains(&op) || (30..49).contains(&op) {
            for o in &mut objects {
                o.position += 1.0;
            }
            origin_shift = true;
            break;
        }
    }

    // Step 3. A chart without `#BPM` has nowBPM 0 in LR2 (infinite times);
    // BMZ keeps its import default instead.
    let mut now_bpm = now_bpm.filter(|b| *b != 0.0).unwrap_or(source.metadata.initial_bpm);
    let mut mea_length = measure_length[0];
    let mut stop_realtime = 0.0f64;
    let mut stop_val = 0.0f64;
    let mut prev = 0.0f64;
    let mut realtime = 0.0f64;
    let mut pending = [usize::MAX; 30];
    for i in 0..objects.len() {
        if objects[i].position != prev {
            if stop_realtime != 0.0 {
                stop_realtime = (stop_val / 192.0) * (240000.0 / now_bpm);
            }
            let add = 240.0 / now_bpm * mea_length * (objects[i].position - prev) * 1000.0;
            realtime += add + stop_realtime;
            prev = objects[i].position;
            stop_realtime = 0.0;
        }
        objects[i].realtime = realtime;
        let op = objects[i].op;
        if (50..=69).contains(&op) {
            let slot = (op - 40) as usize;
            if pending[slot] == usize::MAX {
                objects[i].op -= 40;
                pending[slot] = i;
            } else {
                let start = pending[slot];
                objects[start].realtime_ln = realtime;
                objects[start].ln_end_tick = objects[i].tick;
                pending[slot] = usize::MAX;
            }
        } else if lnobj != -1 && (10..=29).contains(&op) {
            let slot = op as usize;
            if objects[i].val != f64::from(lnobj) || pending[slot] == usize::MAX {
                pending[slot] = i;
            } else {
                let start = pending[slot];
                objects[start].realtime_ln = realtime;
                objects[start].ln_end_tick = objects[i].tick;
                objects[start].lnobj_end = true;
                pending[slot] = usize::MAX;
                objects[i].op += 40;
            }
        }
        if (130..149).contains(&objects[i].op) {
            objects[i].op -= 120;
            objects[i].mine = objects[i].val;
            objects[i].val = 0.0;
        }
        match objects[i].op {
            2 => mea_length = objects[i].val,
            3 if objects[i].val > 0.0 => now_bpm = objects[i].val,
            8 => now_bpm = bpm_slots.get(&(objects[i].val as i32)).copied().unwrap_or(-1.0).abs(),
            9 => {
                stop_val = stop_slots.get(&(objects[i].val as i32)).copied().unwrap_or(-1.0);
                stop_realtime = stop_val / 192.0 * 240000.0 / now_bpm;
            }
            _ => {}
        }
    }

    // Step 4.
    ucrt_qsort(&mut objects, cmp_real_timing);
    Loaded { objects, origin_shift }
}

/// `SINGLESLOTS`: slot keys 1..3844 are kept in either base.
const SINGLE_SLOTS: i32 = 3844;

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

/// Step 5: rebuilds `lane_notes` / `long_notes` / `total_notes` from the
/// loaded objects. `shift_ticks` is the origin measure BMZ inserts for the
/// origin shift. Invisible notes are kept as imported.
pub(super) fn rebuild(
    chart: &mut PlayableChart,
    source: &IntermediateChart,
    loaded: &Loaded,
    shift_ticks: u64,
) {
    let objects = &loaded.objects;
    let floor_us = |ms: f64| TimeUs((ms * 1000.0).floor() as i64);
    let sound_of: HashMap<u16, SoundId> = source
        .resources
        .wavs
        .iter()
        .enumerate()
        .map(|(index, wav)| (wav.key, SoundId(index as u32)))
        .collect();
    let mut last: [Option<usize>; 20] = [None; 20];
    let mut placed: Vec<Vec<usize>> = vec![Vec::new(); 20];
    let mut note_count: i64 = 0;
    for (i, o) in objects.iter().enumerate() {
        if !(10..=29).contains(&o.op) {
            continue;
        }
        let lane = (o.op - 10) as usize;
        let mine = o.mine > 0.0;
        match last[lane] {
            None => {
                placed[lane].push(i);
                last[lane] = Some(i);
                note_count += i64::from(!mine);
            }
            Some(previous) if o.realtime > objects[previous].realtime_ln => {
                if o.realtime != objects[previous].realtime {
                    placed[lane].push(i);
                    last[lane] = Some(i);
                    note_count += i64::from(!mine);
                } else if o.realtime < o.realtime_ln {
                    // A long note replaces a single note at the same time; LR2
                    // decrements the count even if the replaced object was a mine.
                    note_count -= 1;
                    placed[lane].pop();
                    placed[lane].push(i);
                    last[lane] = Some(i);
                    note_count += i64::from(!mine);
                }
            }
            Some(_) => {}
        }
    }

    let mut next_id =
        chart.lane_notes.iter().flatten().map(|n| n.id.0).max().map_or(1, |id| id + 1);
    let mut id = || {
        let current = NoteId(next_id);
        next_id += 1;
        current
    };
    let invisible: [Vec<NoteEvent>; bmz_core::lane::LANE_COUNT] = std::array::from_fn(|index| {
        chart.lane_notes[index].iter().filter(|n| n.kind == NoteKind::Invisible).cloned().collect()
    });
    let mut lane_notes: [Vec<NoteEvent>; bmz_core::lane::LANE_COUNT] =
        std::array::from_fn(|_| Vec::new());
    let mut long_notes = Vec::new();
    let mut exact_ms = HashMap::new();
    for (lane_index, indices) in placed.iter().enumerate() {
        let Some(lane) = lane_of(lane_index as i32 + 10) else {
            continue;
        };
        for &i in indices {
            let o = objects[i];
            // An object BMZ has no tick for (beyond its measures) still counts.
            let Some(tick) = o.tick else {
                continue;
            };
            // ProcNoteOnTiming takes any object with realTiming < realTiming_ln
            // as a long note. A single object has realTiming_ln == -1, so one
            // before -1 ms (after a negative STOP) is a long note ending there,
            // even a mine.
            let long = o.realtime < o.realtime_ln;
            let mine = o.mine > 0.0 && !long;
            let tick = ChartTick(tick.saturating_add(shift_ticks));
            let time = floor_us(o.realtime);
            let sound = if mine { None } else { o.wav_key.and_then(|k| sound_of.get(&k).copied()) };
            let start_id = id();
            exact_ms.insert(start_id, o.realtime);
            // An unpaired LN start has realTiming_ln == -1 and plays as a single note.
            if long {
                let end_id = id();
                let end_tick =
                    ChartTick(o.ln_end_tick.unwrap_or(tick.0).saturating_add(shift_ticks));
                let end_time = floor_us(o.realtime_ln);
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
            } else {
                lane_notes[lane.index()].push(NoteEvent {
                    id: start_id,
                    lane,
                    kind: if mine { NoteKind::Mine } else { NoteKind::Tap },
                    tick,
                    time,
                    sound,
                    layered_sounds: Vec::new(),
                    damage: mine.then_some(o.mine),
                });
            }
        }
    }
    // Keep OpenLR2's lane order: the lane cursor judges objects in this order,
    // which after the truncating real-time sort can be up to a few ms out of
    // time order. Invisible notes are not judged; they go in by time.
    for (notes, invisible) in lane_notes.iter_mut().zip(invisible) {
        for note in invisible {
            let at = notes.partition_point(|n| (n.time, n.tick) <= (note.time, note.tick));
            notes.insert(at, note);
        }
    }
    long_notes.sort_by_key(|p| (p.start_time, p.lane.index()));
    chart.lane_notes = lane_notes;
    chart.long_notes = long_notes;
    chart.metadata.legacy_note_ms = Some(std::sync::Arc::new(exact_ms));
    chart.metadata.free_zones.clear();
    chart.metadata.long_note_mode = LongNoteMode::Ln;
    chart.total_notes = note_count.max(0) as u32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ucrt_qsort_sorts_consistent_input() {
        let mut v: Vec<i32> = (0..200).map(|i| (i * 7919) % 211).collect();
        let mut expected = v.clone();
        expected.sort();
        ucrt_qsort(&mut v, |a, b| a - b);
        assert_eq!(v, expected);
    }

    #[test]
    fn lr2_base_counts_unknown_characters_as_zero() {
        assert_eq!(lr2_base(b'0', b'S', false), 28);
        assert_eq!(lr2_base(b'-', b'1', false), 1);
        assert_eq!(lr2_base(b'z', b'z', true), 61 * 62 + 61);
        assert_eq!(lr2_hex(b'G', b'1'), 1);
    }

    #[test]
    fn channel_convert_matches_lr2_table() {
        for (channel, plain, dsc, pms) in [
            (11, 11, 12, 11),
            (16, 10, 11, 18),
            (17, -1, -1, 19),
            (18, 16, 17, 16),
            (19, 17, 18, 17),
            (22, 22, 22, 16),
            (25, 25, 25, 19),
            (26, 20, 19, 20),
            (27, -1, -1, -1),
            (28, 26, 26, 26),
            (56, 50, 51, 58),
            (136, 130, 131, 138),
            (131, 131, 131, 131),
            (146, 140, 139, 140),
            (1, 1, 1, 1),
            (0, -1, -1, -1),
            (2888, 2888, 2888, 2888),
        ] {
            assert_eq!(channel_convert(channel, false, false), plain, "{channel}");
            assert_eq!(channel_convert(channel, true, false), dsc, "{channel} dsc");
            assert_eq!(channel_convert(channel, false, true), pms, "{channel} pms");
        }
    }
}
