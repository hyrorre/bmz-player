//! OpenLR2's BMS clock, long notes and note count, selected only by LEGACY.
//! The object pipeline itself is in `openlr2_objects`; this module applies its
//! times to the BMZ chart and reads the LR2 header values.
use std::{collections::BTreeMap, path::Path};

use bmz_core::{
    lane::KeyMode,
    time::{ChartTick, TimeUs},
};

use super::{
    BmsRandomSource, BmsTextEncoding, bms_rs_adapter, error::ImportError,
    intermediate::IntermediateChart,
};
use crate::model::{JudgeRankKind, JudgeRankSpec, PlayableChart};

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
    // A dynamic #CONDITIONAL is BMZ's own extension: LR2 has no such branch,
    // and BMZ selects the branch during play. The chart keeps BMZ's clock and
    // branches, so the extension works as without LEGACY. Channels LR2 drops,
    // such as free zones, are still dropped as LR2 does.
    if chart.metadata.conditional.is_some() {
        return Ok(());
    }
    // LR2 reads the file byte by byte with fgets: in UTF-16 every line starts
    // with a NUL or BOM byte instead of '#', so nothing is loaded.
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    let text = if utf16 {
        String::new()
    } else {
        bms_rs_adapter::openlr2_resolved_text(path, bytes, encoding, random_source)?
    };
    // LR2 remaps channels for .pms files and #FP/DSC charts; only the plain BMS
    // channel layout is rebuilt with LR2's lane construction.
    let pms = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pms"));
    let dsc = text
        .lines()
        .any(|l| l.trim_start().get(..7).is_some_and(|h| h.eq_ignore_ascii_case("#FP/DSC")));
    apply(chart, &intermediate, &text, pms, !pms && !dsc)?;
    if !pms && !dsc {
        chart.metadata.key_mode = lr2_scan_key_mode(bytes);
    }
    Ok(())
}

/// The key mode LR2 stores in song.db when it scans the file (`ParseBMSMETA`),
/// which the play loader then uses. LR2 starts at 5 keys and raises the mode by
/// the channel numbers of any data line, whatever its objects: channels 18 / 19
/// make 7 keys and P2 channels make a double mode. Lines inside an `#IF` other
/// than `#IF 1` are skipped. `.pms` and `#FP/DSC` are 9 keys and not handled here.
fn lr2_scan_key_mode(bytes: &[u8]) -> KeyMode {
    let mut mode = 5;
    let mut skip = false;
    // fgets pieces; a piece of a long line that does not start with '#' is ignored.
    for line in bytes.split(|&b| b == b'\n').flat_map(|l| l.chunks(LR2_FGETS_LIMIT)) {
        if line.starts_with(b"#IF") && !line.starts_with(b"#IF 1") {
            skip = true;
            continue;
        }
        if line.starts_with(b"#ENDIF") {
            skip = false;
            continue;
        }
        if line.first() != Some(&b'#')
            || skip
            || !line.get(1..6).is_some_and(|d| d.iter().all(u8::is_ascii_digit))
        {
            continue;
        }
        let mut data = i32::from(line[4] - b'0') * 10 + i32::from(line[5] - b'0');
        if (51..=69).contains(&data) {
            data -= 40;
        }
        if (31..=49).contains(&data) {
            data -= 20;
        }
        if matches!(data, 18 | 19 | 38 | 39 | 58 | 59) {
            mode = match mode {
                5 => 7,
                10 => 14,
                other => other,
            };
        } else if matches!(data, 28 | 29 | 48 | 49 | 68 | 69) {
            mode = 14;
        } else if (21..=29).contains(&data)
            || (41..=49).contains(&data)
            || (61..=69).contains(&data)
        {
            mode = match mode {
                5 => 10,
                7 => 14,
                other => other,
            };
        }
    }
    match mode {
        7 => KeyMode::K7,
        10 => KeyMode::K10,
        14 => KeyMode::K14,
        _ => KeyMode::K5,
    }
}

/// `fgets(buffer, 102400, ...)` reads at most 102,399 bytes per call.
const LR2_FGETS_LIMIT: usize = 102_399;

fn apply(
    chart: &mut PlayableChart,
    source: &IntermediateChart,
    text: &str,
    pms: bool,
    rebuild_lanes: bool,
) -> Result<(), ImportError> {
    let loaded = super::openlr2_objects::load(text, source, pms);
    let origin_shift = loaded.origin_shift;
    // BMZ events are timed by tick. Measure entries define their tick; other
    // ticks take the first object in LR2's final order.
    let floor_us = |ms: f64| TimeUs((ms * 1000.0).floor() as i64);
    let mut times = BTreeMap::new();
    for o in &loaded.objects {
        if let (super::openlr2_objects::Origin::Measure(_), Some(tick)) = (o.origin, o.tick) {
            times.insert(tick, floor_us(o.realtime));
        }
    }
    for o in &loaded.objects {
        if let Some(tick) = o.tick {
            times.entry(tick).or_insert_with(|| floor_us(o.realtime));
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
    // BPM reflections keep BMZ's clock; the rebuild keeps them.
    for e in chart.timing_events.iter_mut().filter(|e| e.kind.is_bpm_reflection()) {
        e.time = at(e.tick, e.time);
    }
    rebuild_events(chart, source, &loaded.objects, shift_ticks);
    macro_rules! retime { ($($field:ident),*) => { $(for e in &mut chart.$field { e.time = at(e.tick,e.time); e.tick.0 = e.tick.0.saturating_add(shift_ticks); })* }; }
    retime!(
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
    if rebuild_lanes {
        super::openlr2_objects::rebuild(chart, source, &loaded, shift_ticks);
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

/// Rebuilds the BGM, BGA, BPM and STOP events from LR2's objects, as the
/// notes are: the objects LR2 loads, at their own times. Several positions of
/// a very short measure share one BMZ tick, so the tick cannot give their
/// times; and BMZ's loader can read objects LR2 does not (or the reverse).
/// A STOP has LR2's length. Kept from BMZ: a BPM reflection (LEGACY plays
/// such a chart on BMZ's clock) and the implicit #BMP00 POOR image, which LR2
/// shows from the start without an object. `shift_ticks` is the origin
/// measure BMZ inserts for LR2's origin shift.
fn rebuild_events(
    chart: &mut PlayableChart,
    source: &IntermediateChart,
    objects: &[super::openlr2_objects::LrObject],
    shift_ticks: u64,
) {
    use crate::model::{
        BgaAssetId, BgaEvent, BgaEventKind, SoundEvent, TimingEvent, TimingEventKind,
    };
    use bmz_core::ids::SoundId;
    use std::collections::HashMap;
    let floor_us = |ms: f64| TimeUs((ms * 1000.0).floor() as i64);
    let sound_of: HashMap<u16, SoundId> = source
        .resources
        .wavs
        .iter()
        .enumerate()
        .map(|(index, wav)| (wav.key, SoundId(index as u32)))
        .collect();
    let mut bmp_keys: Vec<u16> = source.resources.bmps.iter().map(|bmp| bmp.key).collect();
    bmp_keys.sort_unstable();
    let asset_of =
        |key: u16| bmp_keys.binary_search(&key).ok().map(|index| BgaAssetId(index as u32));
    let initial_poor = chart
        .bga_events
        .iter()
        .find(|e| e.kind == BgaEventKind::Poor && e.tick.0 == 0 && e.asset == asset_of(0))
        .cloned();
    let reflections = chart
        .timing_events
        .iter()
        .filter(|e| e.kind.is_bpm_reflection())
        .map(|e| TimingEvent { tick: ChartTick(e.tick.0.saturating_add(shift_ticks)), ..*e });
    let mut bgm = Vec::new();
    let mut bga = Vec::new();
    let mut timing = Vec::new();
    // LR2's real-time order; stable sorts below keep it for equal times.
    for o in objects {
        let Some(tick) = o.tick.map(|tick| ChartTick(tick.saturating_add(shift_ticks))) else {
            continue;
        };
        let time = floor_us(o.realtime);
        let bga_kind = match o.op {
            4 => Some(BgaEventKind::Base),
            6 => Some(BgaEventKind::Poor),
            7 => Some(BgaEventKind::Layer),
            _ => None,
        };
        match o.op {
            1 => {
                if let Some(&sound) = o.wav_key.and_then(|key| sound_of.get(&key)) {
                    bgm.push(SoundEvent { tick, time, sound });
                }
            }
            3 | 8 if o.bpm > 0.0 => timing.push(TimingEvent {
                tick,
                time,
                kind: TimingEventKind::BpmChange { bpm: o.bpm },
            }),
            9 => timing.push(TimingEvent {
                tick,
                time,
                kind: TimingEventKind::Stop { duration_us: (o.stop_ms * 1000.0).floor() as i64 },
            }),
            _ => {}
        }
        if let Some(kind) = bga_kind {
            let asset = o.wav_key.and_then(asset_of);
            bga.push(BgaEvent { tick, time, asset, kind });
        }
    }
    if let Some(poor) = initial_poor
        && !bga.iter().any(|e| e.kind == BgaEventKind::Poor && e.time.0 <= 0)
    {
        bga.insert(0, BgaEvent { tick: ChartTick(0), time: TimeUs(0), ..poor });
    }
    timing.extend(reflections);
    timing.sort_by_key(|e| e.time);
    bgm.sort_by_key(|e| e.time);
    bga.sort_by_key(|e| e.time);
    chart.bgm_events = bgm;
    chart.bga_events = bga;
    chart.timing_events = timing;
}

/// C `atol` limited to `int`: leading white space, an optional sign, then
/// decimal digits up to the first other character. No digits give 0.
pub(super) fn atol(text: &str) -> i32 {
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

pub(super) fn slot_key(token: &[u8], base62: bool) -> Option<u16> {
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

/// C `atof` for the plain decimal forms: optional sign, digits, an optional
/// fraction and exponent, up to the first other character. No number gives 0.
pub(super) fn atof(text: &str) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{AssetLookup, import_chart_bytes_with_assets};
    use crate::model::NoteKind;
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
    fn key_mode_follows_the_lr2_scan_of_channel_numbers() {
        use bmz_core::lane::KeyMode;
        for (text, mode) in [
            // An all-zero line still counts, as in LR2's song.db.
            ("#BPM 120\n#00111:01\n#00118:00\n", KeyMode::K7),
            // Lines inside an #IF other than #IF 1 are skipped.
            ("#BPM 120\n#00111:01\n#IF 2\n#00118:01\n#ENDIF\n", KeyMode::K5),
            ("#BPM 120\n#00111:01\n#00121:01\n", KeyMode::K10),
            ("#BPM 120\n#00111:01\n#00121:01\n#00159:00\n", KeyMode::K14),
            ("#BPM 120\n#00111:01\n#00128:00\n", KeyMode::K14),
        ] {
            assert_eq!(load(text).metadata.key_mode, mode, "{text}");
        }
    }

    #[test]
    fn an_object_before_minus_one_ms_is_a_long_note_even_a_mine() {
        // An undefined STOP slot stops for -1/192 measure. 400 of them at
        // 60 BPM move measure 1 (4000 ms) to -4333.33 ms (checked with the
        // OpenLR2 oracle, which keeps realTiming_ln == -1 for the mine).
        let text = format!("#BPM 60\n#00009:{}\n#001D1:05\n#00611:01\n", "01".repeat(400));
        let chart = load(&text);
        let notes = &chart.lane_notes[Lane::Key1.index()];
        let kinds: Vec<_> = notes.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, [NoteKind::LongStart, NoteKind::LongEnd, NoteKind::Tap]);
        assert_eq!(notes[0].time.0, -4_333_334);
        assert_eq!(notes[1].time.0, -1_000);
        assert_eq!(notes[2].time.0, 15_666_666);
        assert_eq!(chart.long_notes.len(), 1);
    }

    #[test]
    fn song_runtime_follows_the_lr2_endtime_loop() {
        // Checked with the OpenLR2 oracle. A single note's realTiming_ln is -1,
        // so the first note raises endtime to 1999 ms, not to its time + 2 s.
        for (text, runtime) in [
            ("#BPM 60\n#00111:01\n", 1_999.0),
            ("#BPM 60\n#00111:0101\n#00201:01\n", 8_000.0),
            ("#BPM 60\n#00111:0101\n#00201:0001\n", 10_000.0),
        ] {
            assert_eq!(load(text).metadata.legacy_song_runtime_ms, Some(runtime), "{text}");
        }
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
    fn events_in_one_tick_of_a_short_measure_keep_their_own_lr2_times() {
        // Measure 1 is 1/10000 long: its start, its middle and measure 2 share
        // one BMZ tick. The STOP at measure 1 lasts one 120 BPM measure.
        let chart = load(
            "#BPM 120
#WAV01 key.wav
#BMP01 image.png
#STOP01 192
#00102:0.0001
             #00109:01
#00101:0101
#00104:0101
#00111:0101
#00211:01
",
        );
        let ms = |time: TimeUs| time.0 / 1000;
        let notes: Vec<_> =
            chart.lane_notes[Lane::Key1.index()].iter().map(|n| ms(n.time)).collect();
        assert_eq!(notes, [2_000, 4_000, 4_000]);
        let bgm: Vec<_> = chart.bgm_events.iter().map(|e| ms(e.time)).collect();
        assert_eq!(bgm, [2_000, 4_000]);
        let bga: Vec<_> = chart.bga_events.iter().map(|e| ms(e.time)).collect();
        assert_eq!(bga, [2_000, 4_000]);
        let stops: Vec<_> = chart
            .timing_events
            .iter()
            .filter_map(|e| match e.kind {
                crate::model::TimingEventKind::Stop { duration_us } => {
                    Some((ms(e.time), duration_us))
                }
                _ => None,
            })
            .collect();
        assert_eq!(stops, [(2_000, 2_000_000)]);
        // The clock built from these events puts the STOP where LR2 does.
        let map = crate::timing::TimingMap::from_chart_timing_events(120.0, &chart.timing_events);
        let tick = chart.bgm_events[0].tick;
        assert_eq!(ms(map.tick_to_time(tick)), 2_000);
    }

    #[test]
    fn a_dynamic_conditional_keeps_the_bmz_chart_and_its_branches() {
        let text = "#BPM 120
#WAV01 key.wav
#00111:01
#CONDITIONAL 2
#WHEN RATE>=40
                    #00312:01
#DEFAULT
#00313:01
#ENDCONDITIONAL
";
        let path = Path::new("legacy.bms");
        let source = BmsRandomSource::Lr2Seed(1);
        let imported =
            import_chart_bytes_with_assets(path, text.as_bytes(), source, AssetLookup::Declared)
                .unwrap()
                .chart;
        let legacy = load(text);
        assert!(legacy.metadata.conditional.is_some());
        // Only the shared note and the DEFAULT branch, as without LEGACY.
        assert_eq!(legacy.total_notes, 2);
        assert_eq!(legacy.total_notes, imported.total_notes);
        let notes = |chart: &PlayableChart| {
            chart.lane_notes.iter().flatten().map(|n| (n.lane, n.time)).collect::<Vec<_>>()
        };
        assert_eq!(notes(&legacy), notes(&imported));
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

    /// (kind, time ms) of a lane after the LEGACY rebuild.
    fn lane(chart: &PlayableChart, lane: Lane) -> Vec<(crate::model::NoteKind, i64)> {
        chart.lane_notes[lane.index()]
            .iter()
            .filter(|n| n.kind != crate::model::NoteKind::Invisible)
            .map(|n| (n.kind, n.time.0 / 1000))
            .collect()
    }

    use crate::model::NoteKind::{LongEnd, LongStart, Mine, Tap};

    /// An #LNOBJ note with nothing pending stays a normal note (`//CHECK: haha`).
    #[test]
    fn rebuild_keeps_an_orphan_lnobj_as_a_tap() {
        // 120 BPM: 2 s per measure. 01 @4/12, ZZ @8/12 (end), ZZ @10/12 (orphan).
        let chart = load("#BPM 120\n#LNOBJ ZZ\n#00111:0000000001000000ZZ00ZZ00\n#00211:01\n");
        assert_eq!(
            lane(&chart, Lane::Key1),
            [(LongStart, 2666), (LongEnd, 3333), (Tap, 3666), (Tap, 4000)]
        );
        assert_eq!(chart.total_notes, 3);
    }

    /// LN channels and #LNOBJ candidates share LR2's pending slot: the first
    /// LN-channel object closes a normal note waiting for an #LNOBJ end.
    #[test]
    fn rebuild_lets_an_ln_channel_close_a_pending_normal_note() {
        let chart = load("#BPM 120\n#LNOBJ 05\n#00111:01\n#00251:0101\n#00311:01\n");
        // 01 @2 s becomes an LN to 4 s; the second 51 object starts an LN
        // with no end, which plays as a single note.
        assert_eq!(
            lane(&chart, Lane::Key1),
            [(LongStart, 2000), (LongEnd, 4000), (Tap, 5000), (Tap, 6000)]
        );
        assert_eq!(chart.total_notes, 3);
    }

    /// Objects inside an LN, including its end time, are dropped; an LN start
    /// replaces a single note at the same time.
    #[test]
    fn rebuild_drops_notes_inside_ln_and_lets_ln_replace_a_tap() {
        let chart = load("#BPM 120\n#00111:01010101\n#00151:01000001\n#00211:01\n");
        assert_eq!(lane(&chart, Lane::Key1), [(LongStart, 2000), (LongEnd, 3500), (Tap, 4000)]);
        assert_eq!(chart.total_notes, 2);
    }

    /// A mine at the time of a note is dropped; mines are not counted.
    #[test]
    fn rebuild_places_mines_after_notes_and_does_not_count_them() {
        let chart = load("#BPM 120\n#00111:0100\n#001D1:0505\n#00211:01\n");
        assert_eq!(lane(&chart, Lane::Key1), [(Tap, 2000), (Mine, 3000), (Tap, 4000)]);
        assert_eq!(chart.total_notes, 2);
    }

    /// `channelConvert`: 16 is the scratch, 17 is dropped, 18/19 are keys 6/7.
    #[test]
    fn rebuild_uses_lr2_channel_mapping() {
        let chart = load("#BPM 120\n#00116:01\n#00117:01\n#00118:01\n#00119:01\n");
        assert_eq!(lane(&chart, Lane::Scratch), [(Tap, 2000)]);
        assert_eq!(lane(&chart, Lane::Key6), [(Tap, 2000)]);
        assert_eq!(lane(&chart, Lane::Key7), [(Tap, 2000)]);
        assert_eq!(chart.total_notes, 3);
    }

    fn loaded_objects(text: &str) -> super::super::openlr2_objects::Loaded {
        let path = Path::new("legacy.bms");
        let source = bms_rs_adapter::import_bms_bytes_to_intermediate(
            path,
            (text.as_bytes(), None),
            &BmsRandomSource::Lr2Seed(1),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut None,
            &mut Vec::new(),
        )
        .unwrap();
        super::super::openlr2_objects::load(text, &source, false)
    }

    #[test]
    fn inconsistent_position_order_matches_openlr2_qsort() {
        // Two BGM lines split 9789 and 9781 ways put many positions less than
        // 1/100000 measure apart. OpenLR2 (oracle, v261101) times the notes at
        // 2399.9999999999864 and 3599.9999999999864 ms; walking the positions
        // in true order gives 2399.9999999999845 instead.
        let text = format!(
            "#BPM 200\n#WAV01 a.wav\n#00101:{}\n#00101:{}\n#00211:01\n#00311:01\n",
            "01".repeat(9789),
            "01".repeat(9781)
        );
        let loaded = loaded_objects(&text);
        let notes: Vec<f64> =
            loaded.objects.iter().filter(|o| o.op == 11).map(|o| o.realtime).collect();
        assert_eq!(notes, [2399.9999999999864, 3599.9999999999864]);
    }

    #[test]
    fn long_lines_are_cut_like_fgets() {
        // 102,399 characters per read: the first piece keeps (102399 - 7) / 2
        // objects, the note at the very end of the line is lost.
        let objects = 60_000;
        let mut data = "00".repeat(objects - 1);
        data.push_str("01");
        let chart = load(&format!("#BPM 120\n#00111:{data}\n#00211:01\n"));
        assert_eq!(chart.total_notes, 1);
    }

    #[test]
    fn utf16_charts_load_no_notes() {
        let text: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(
                "#BPM 120\r\n#WAV01 a.wav\r\n#00111:01\r\n#00101:01\r\n#00103:FF\r\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        let path = Path::new("legacy.bms");
        let source = BmsRandomSource::Lr2Seed(1);
        let mut imported =
            import_chart_bytes_with_assets(path, &text, source.clone(), AssetLookup::Declared)
                .unwrap();
        assert!(!imported.chart.bgm_events.is_empty());
        apply_from_bytes(&mut imported.chart, path, &text, &source, None).unwrap();
        assert_eq!(imported.chart.total_notes, 0);
        // Nor any BGM or BPM change.
        assert!(imported.chart.bgm_events.is_empty());
        assert!(imported.chart.timing_events.is_empty());
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
