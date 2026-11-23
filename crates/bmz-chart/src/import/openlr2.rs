//! OpenLR2's sequential double-precision BMS clock, selected only by LEGACY.
//! Source: LR2_bmsload.cpp, CMP_NotesByBmsTiming and the bpmt_realtime loop.
use std::{collections::BTreeMap, path::Path};

use bmz_core::time::{ChartTick, TimeUs};

use super::{
    BmsRandomSource, BmsTextEncoding, bms_rs_adapter,
    error::ImportError,
    intermediate::{IntermediateChart, IntermediateObjectKind as Kind},
    normalize::object_to_tick,
};
use crate::model::{JudgeRankKind, JudgeRankSpec, PlayableChart};

#[derive(Clone, Copy)]
enum Action {
    None,
    Measure(f64),
    Bpm(f64),
    Stop(f64),
}

/// Re-evaluate the same source branches, without changing the import identity,
/// sounds or random layout. All playback events share the resulting clock.
pub fn apply_from_bytes(
    chart: &mut PlayableChart,
    path: &Path,
    bytes: &[u8],
    random_source: &BmsRandomSource,
    encoding: Option<BmsTextEncoding>,
) -> Result<(), ImportError> {
    if !matches!(chart.metadata.source_format, crate::model::ChartSourceFormat::Bms) {
        return Ok(());
    }
    let intermediate = bms_rs_adapter::import_bms_bytes_to_intermediate(
        path,
        (bytes, encoding),
        random_source,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut None,
        &mut Vec::new(),
    )?;
    if intermediate.identity.file_sha256 != chart.identity.file_sha256 {
        return Err(ImportError::InvalidChart {
            message: "chart source changed while preparing LEGACY timing".into(),
        });
    }
    let text = bms_rs_adapter::openlr2_resolved_text(path, bytes, encoding, random_source)?;
    apply(chart, &intermediate, &text)
}

fn apply(
    chart: &mut PlayableChart,
    source: &IntermediateChart,
    text: &str,
) -> Result<(), ImportError> {
    let raw = raw_objects(text, source)?;
    let origin_shift = raw.iter().any(|p| p.0 == 0.0 && matches!(p.1, 11..=29 | 31..=49));
    let shift = if origin_shift { 1.0 } else { 0.0 };
    let mut points = Vec::new();
    for measure in &source.measures {
        points.push((
            f64::from(measure.index) + shift,
            2,
            measure.start_tick.0 / crate::timing::IMPORT_TICK_SCALE,
            Action::Measure(measure.length),
        ));
    }
    for (position, priority, tick, action) in raw {
        points.push((position + shift, priority, tick, action));
    }
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut previous = 0.0;
    let mut realtime_ms = 0.0;
    let mut bpm = source.metadata.initial_bpm;
    let mut length = source.measures.first().map_or(1.0, |m| m.length);
    let mut stop_units = 0.0;
    let mut times = BTreeMap::new();
    for (position, _, tick, action) in points {
        if position != previous {
            // Keep LR2's operation order: these floating-point roundoff errors
            // are observable after each note is converted to integer milliseconds.
            let advance = 240.0 / bpm * length * (position - previous) * 1000.0;
            // LR2: stopVal / 192.0 * 240000.0 / nowBPM, in this order.
            let stop = stop_units / 192.0 * 240000.0 / bpm;
            realtime_ms += advance + stop;
            stop_units = 0.0;
            previous = position;
        }
        // Preserve which side of the integer-ms boundary the double lies on.
        times.insert(tick, TimeUs((realtime_ms * 1000.0).floor() as i64));
        match action {
            Action::None => {}
            Action::Measure(value) => length = value,
            Action::Bpm(value) if value > 0.0 => bpm = value,
            Action::Stop(value) => stop_units = value.trunc(),
            Action::Bpm(_) => {}
        }
    }
    let at = |tick: ChartTick, old: TimeUs| times.get(&tick.0).copied().unwrap_or(old);
    let shift_ticks = if origin_shift {
        source.measures.first().map_or(0, |m| m.tick_len / crate::timing::IMPORT_TICK_SCALE)
    } else {
        0
    };
    for notes in &mut chart.lane_notes {
        for note in notes {
            note.time = at(note.tick, note.time);
            note.tick.0 = note.tick.0.saturating_add(shift_ticks);
        }
    }
    for pair in &mut chart.long_notes {
        pair.start_time = at(pair.start_tick, pair.start_time);
        pair.end_time = at(pair.end_tick, pair.end_time);
        pair.start_tick.0 = pair.start_tick.0.saturating_add(shift_ticks);
        pair.end_tick.0 = pair.end_tick.0.saturating_add(shift_ticks);
    }
    macro_rules! retime { ($($field:ident),*) => { $(for e in &mut chart.$field { e.time = at(e.tick,e.time); e.tick.0 = e.tick.0.saturating_add(shift_ticks); })* }; }
    retime!(
        bgm_events,
        bga_events,
        timing_events,
        scroll_events,
        speed_events,
        judge_rank_events,
        bgm_volume_events,
        key_volume_events,
        text_events,
        bga_opacity_events,
        bga_argb_events,
        bga_keybound_events,
        bar_lines
    );
    if origin_shift {
        for bar in &mut chart.bar_lines {
            bar.measure = bar.measure.saturating_add(1);
        }
        chart
            .bar_lines
            .insert(0, crate::model::BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) });
        chart.metadata.start_note_margin = times.get(&0).copied().unwrap_or(TimeUs(0));
    }
    chart.end_time =
        chart.lane_notes.iter().flatten().map(|n| n.time).max().unwrap_or(chart.end_time);
    // LR2 recognizes #RANK here, not beatoraja's #DEFEXRANK.
    let rank = text
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            words.next().filter(|w| w.eq_ignore_ascii_case("#RANK"))?;
            words.next()?.parse().ok()
        })
        .next_back();
    // LR2 stores #TOTAL as an int read by atol: "160.5" is 160. A missing
    // value is left for the gauge to replace with LR2's integer default.
    chart.metadata.total = text
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let head = line.get(..7)?;
            head.eq_ignore_ascii_case("#TOTAL ").then(|| f64::from(atol(&line[7..])))
        })
        .next_back();
    chart.metadata.judge_rank = rank;
    chart.metadata.judge_rank_spec =
        rank.map(|value| JudgeRankSpec { value, kind: JudgeRankKind::BmsRank });
    if source.typed_lnobj.is_empty() {
        // OpenLR2's BMS loader does not implement #LNMODE CN/HCN overrides.
        chart.metadata.long_note_mode = crate::model::LongNoteMode::Ln;
        for pair in &mut chart.long_notes {
            pair.mode = Some(crate::model::LongNoteMode::Ln);
        }
    }
    chart.judge_rank_events.clear();
    Ok(())
}

/// C `atol` limited to `int`: leading white space, an optional sign, then
/// decimal digits up to the first other character. No digits give 0.
fn atol(text: &str) -> i32 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let mut value: i64 = 0;
    for byte in digits.bytes().take_while(u8::is_ascii_digit) {
        value = (value * 10 + i64::from(byte - b'0')).min(i64::from(i32::MAX) + 1);
    }
    let value = if negative { -value } else { value };
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

type Point = (f64, i32, u64, Action);

/// `#BPMxx` / `#STOPxx` slots read as LR2_bmsload.cpp does: the key in the
/// chart's base, `#BPM` by `atof` from column 7, `#STOP` by `atol` from column 8
/// (kept only when positive), later lines replacing earlier ones. An unset
/// slot is -1: an extended BPM then plays at |-1| = 1 and a STOP is -1.
struct Slots {
    bpm: std::collections::HashMap<u16, f64>,
    stop: std::collections::HashMap<u16, f64>,
}

fn slot_key(token: &[u8], base62: bool) -> Option<u16> {
    let digit = |b: u8| -> Option<u16> {
        let n = match b {
            b'0'..=b'9' => b - b'0',
            b'A'..=b'Z' => b - b'A' + 10,
            b'a'..=b'z' if base62 => b - b'a' + 36,
            b'a'..=b'z' => b - b'a' + 10,
            _ => return None,
        };
        Some(u16::from(n))
    };
    let radix = if base62 { 62 } else { 36 };
    Some(digit(*token.first()?)? * radix + digit(*token.get(1)?)?)
}

fn slots(text: &str, base62: bool) -> Slots {
    let mut slots = Slots { bpm: Default::default(), stop: Default::default() };
    for line in text.lines().map(str::trim) {
        let bytes = line.as_bytes();
        if bytes.len() >= 7 && bytes[..4].eq_ignore_ascii_case(b"#BPM") {
            if let Some(key) = slot_key(&bytes[4..6], base62).filter(|k| *k >= 1) {
                slots.bpm.insert(key, atof(line.get(7..).unwrap_or("")));
            }
        } else if bytes.len() >= 8 && bytes[..5].eq_ignore_ascii_case(b"#STOP") {
            let value = atol(line.get(8..).unwrap_or(""));
            if let Some(key) = slot_key(&bytes[5..7], base62).filter(|k| *k >= 1)
                && value > 0
            {
                slots.stop.insert(key, f64::from(value));
            }
        }
    }
    slots
}

/// C `atof` for the plain decimal forms: optional sign, digits, an optional
/// fraction and exponent, up to the first other character. No number gives 0.
fn atof(text: &str) -> f64 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let bytes = text.as_bytes();
    let mut end = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    let digits_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == digits_start || (end == digits_start + 1 && bytes[digits_start] == b'.') {
        return 0.0;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exp = end + 1;
        if matches!(bytes.get(exp), Some(b'+' | b'-')) {
            exp += 1;
        }
        if bytes.get(exp).is_some_and(u8::is_ascii_digit) {
            while bytes.get(exp).is_some_and(u8::is_ascii_digit) {
                exp += 1;
            }
            end = exp;
        }
    }
    text[..end].parse().unwrap_or(0.0)
}

fn raw_objects(text: &str, source: &IntermediateChart) -> Result<Vec<Point>, ImportError> {
    let slots = slots(text, source.metadata.base62_obj_ids);
    let mut points = Vec::new();
    for line in text.lines().map(str::trim) {
        let bytes = line.as_bytes();
        if bytes.len() < 9
            || bytes[0] != b'#'
            || !bytes[1..4].iter().all(u8::is_ascii_digit)
            || bytes[6] != b':'
            || (!bytes[5].is_ascii_digit() && !bytes[4..6].eq_ignore_ascii_case(b"SC"))
        {
            continue;
        }
        let measure = line[1..4].parse::<u32>().unwrap();
        let channel = if bytes[4..6].eq_ignore_ascii_case(b"SC") {
            2888
        } else {
            i32::from(bytes[4].to_ascii_uppercase().saturating_sub(if bytes[4].is_ascii_digit() {
                b'0'
            } else {
                b'A' - 10
            })) * 10
                + i32::from(bytes[5] - b'0')
        };
        if measure as usize >= source.measures.len() {
            continue;
        }
        if channel <= 0 || channel == 2 {
            continue;
        }
        let data = &bytes[7..];
        let divisions = data.len() / 2;
        for (index, token) in data.as_chunks::<2>().0.iter().enumerate() {
            let radix = if channel == 3 {
                16
            } else if source.metadata.base62_obj_ids {
                62
            } else {
                36
            };
            let digit = |b: u8| -> Option<u16> {
                let n = match b {
                    b'0'..=b'9' => b - b'0',
                    b'A'..=b'Z' => b - b'A' + 10,
                    b'a'..=b'z' if radix == 62 => b - b'a' + 36,
                    b'a'..=b'z' => b - b'a' + 10,
                    _ => return None,
                };
                (u16::from(n) < radix).then_some(u16::from(n))
            };
            let Some(value) = digit(token[0])
                .zip(digit(token[1]))
                .map(|(a, b)| a * radix + b)
                .filter(|v| *v != 0)
            else {
                continue;
            };
            let action = match channel {
                3 => Action::Bpm(f64::from(value)),
                // `BPM < 0.0` is negated; an unset slot is -1.
                8 => Action::Bpm(slots.bpm.get(&value).copied().unwrap_or(-1.0).abs()),
                9 => Action::Stop(slots.stop.get(&value).copied().unwrap_or(-1.0)),
                _ => Action::None,
            };
            let object = super::intermediate::IntermediateObject {
                measure,
                position_num: index as u32,
                position_den: divisions as u32,
                kind: Kind::BarLine,
            };
            let tick =
                object_to_tick(&object, &source.measures)?.0 / crate::timing::IMPORT_TICK_SCALE;
            points.push((
                f64::from(measure) + index as f64 / divisions as f64,
                channel,
                tick,
                action,
            ));
        }
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{AssetLookup, import_chart_bytes_with_assets};
    use bmz_core::lane::Lane;

    fn load(text: &str) -> PlayableChart {
        let path = Path::new("legacy.bms");
        let source = BmsRandomSource::Lr2Seed(1);
        let mut imported = import_chart_bytes_with_assets(
            path,
            text.as_bytes(),
            source.clone(),
            AssetLookup::Declared,
        )
        .unwrap();
        apply_from_bytes(&mut imported.chart, path, text.as_bytes(), &source, None).unwrap();
        imported.chart
    }

    #[test]
    fn sequential_double_rounding_survives_integer_millisecond_conversion() {
        let chart = load(
            "#BPM 180\n#00001:00010101010101000000000000000000\n#00011:00000000000001000000000000000000\n",
        );
        // LR2's six increments of 83.333...ms produce 499.99999999999994ms.
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].time.0 / 1000, 499);
    }

    #[test]
    fn unhandled_channel_objects_still_split_the_lr2_clock() {
        let chart = load(
            "#BPM 180\n#00099:00010101010101000000000000000000\n#00011:00000000000001000000000000000000\n",
        );
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].time.0 / 1000, 499);
    }

    #[test]
    fn invisible_note_at_origin_adds_a_whole_measure() {
        let chart = load("#BPM 120\n#00036:01\n#00111:01\n");
        assert_eq!(chart.lane_notes[Lane::Scratch.index()][0].time.0, 2_000_000);
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].time.0, 4_000_000);
        assert_eq!(
            chart.lane_notes[Lane::Key1.index()][0].tick.0,
            u64::from(crate::timing::TICKS_PER_MEASURE) * 2
        );
        assert_eq!(chart.bar_lines[0].time, TimeUs(0));
    }

    #[test]
    fn rank_ignores_defexrank_and_keeps_the_original_rank() {
        let chart = load("#BPM 120\n#RANK 3\n#DEFEXRANK 120\n#00111:01\n");
        assert_eq!(chart.metadata.judge_rank, Some(3));
        assert_eq!(chart.metadata.judge_rank_spec.unwrap().kind, JudgeRankKind::BmsRank);
        assert_eq!(chart.metadata.judge_rank_spec.unwrap().value, 3);
    }

    #[test]
    fn total_is_read_like_lr2_atol() {
        assert_eq!(load("#BPM 120\n#TOTAL 160.9\n#00111:01\n").metadata.total, Some(160.0));
        assert_eq!(load("#BPM 120\n#total 300\n#00111:01\n").metadata.total, Some(300.0));
        assert_eq!(load("#BPM 120\n#TOTAL abc\n#00111:01\n").metadata.total, Some(0.0));
        assert_eq!(load("#BPM 120\n#00111:01\n").metadata.total, None);
        assert_eq!(super::atol("  -12x"), -12);
        assert_eq!(super::atol("+7"), 7);
        assert_eq!(super::atol("99999999999"), i32::MAX);
    }

    #[test]
    fn bpm_stop_and_measure_length_share_the_clock() {
        let chart = load(
            "#BPM 120\n#BPM01 240\n#STOP01 192\n#00002:0.5\n#00108:01\n#00109:01\n#00111:01\n#00211:01\n",
        );
        let notes = &chart.lane_notes[Lane::Key1.index()];
        assert_eq!(notes[0].time.0, 1_000_000);
        assert_eq!(notes[1].time.0, 3_000_000);
    }

    /// Slot keys are base 36 in a normal chart: `1A` is 46, which bms-rs
    /// indexes as 72. The extended BPM and STOP must still be found.
    #[test]
    fn extended_bpm_and_stop_slots_use_the_chart_base() {
        let chart =
            load("#BPM 120\n#BPM1A 240\n#STOP1A 192\n#00108:1A\n#00109:1A\n#00111:01\n#00211:01\n");
        let notes = &chart.lane_notes[Lane::Key1.index()];
        // 2 s at 120, then a 1 s stop at 240 (192 / 192 * 240000 / 240).
        assert_eq!(notes[0].time.0, 2_000_000);
        assert_eq!(notes[1].time.0, 4_000_000);
    }

    /// LR2 leaves an unset slot at -1: BPM |-1| = 1, STOP -1 (a negative stop).
    #[test]
    fn unset_extended_bpm_and_stop_slots_are_minus_one() {
        let bpm = load("#BPM 120\n#00108:02\n#00111:01\n#00211:01\n");
        let notes = &bpm.lane_notes[Lane::Key1.index()];
        assert_eq!(notes[1].time.0 - notes[0].time.0, 240_000_000);
        let stop = load("#BPM 120\n#00109:02\n#00111:01\n#00211:01\n");
        let notes = &stop.lane_notes[Lane::Key1.index()];
        assert_eq!(notes[1].time.0 - notes[0].time.0, 1_989_583);
    }

    /// A mine channel value is a base-36 number unless `#BASE 62` is set.
    #[test]
    fn mine_damage_is_a_base36_number() {
        let chart = load("#BPM 120\n#001D1:ZZ\n#00211:01\n");
        let mine = chart.lane_notes[Lane::Key1.index()]
            .iter()
            .find(|n| n.kind == crate::model::NoteKind::Mine)
            .unwrap();
        assert_eq!(mine.damage, Some(1295.0));
    }

    #[test]
    fn atof_reads_the_leading_number_like_c() {
        assert_eq!(super::atof(" 150.5abc"), 150.5);
        assert_eq!(super::atof("-2e2"), -200.0);
        assert_eq!(super::atof("1e"), 1.0);
        assert_eq!(super::atof(".5"), 0.5);
        assert_eq!(super::atof("x"), 0.0);
    }
}
