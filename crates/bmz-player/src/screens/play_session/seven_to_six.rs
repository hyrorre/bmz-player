use super::*;
use std::collections::{HashMap, HashSet};

const RNG_DOMAIN: i64 = 0x0037_4b36;
const DESTINATION_LANES: [Lane; 6] =
    [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6];

#[derive(Debug)]
struct MovableUnit {
    notes: Vec<NoteEvent>,
    long_note: Option<LongNotePair>,
}

impl MovableUnit {
    fn start_time(&self) -> TimeUs {
        self.long_note.as_ref().map_or(self.notes[0].time, |pair| pair.start_time)
    }

    fn jack_times(&self) -> impl Iterator<Item = TimeUs> + '_ {
        self.notes.iter().filter_map(|note| jack_relevant(note).then_some(note.time))
    }
}

pub fn normalize_arrange_for_seven_to_six(arrange: ArrangeOption) -> ArrangeOption {
    match arrange {
        ArrangeOption::RandomEx => ArrangeOption::Random,
        ArrangeOption::SRandomEx => ArrangeOption::SRandom,
        ArrangeOption::AllScratch => ArrangeOption::Normal,
        other => other,
    }
}

/// Converts a source 7K chart into six scratch-less lanes.
///
/// Key1/2/3/5/6/7 are fixed to destination Key1..6. Scratch and source Key4
/// are placed into free lanes while avoiding press notes at intervals of
/// `jack_threshold_ms` or less (the H-RANDOM threshold). A movable object that
/// cannot be placed becomes BGM. Only invisible notes originating from
/// Scratch/Key4 are discarded entirely.
pub fn apply_seven_to_six(
    chart: &mut PlayableChart,
    seed: i64,
    legacy_seed: bool,
    jack_threshold_ms: u32,
) -> bool {
    if chart.metadata.key_mode != KeyMode::K7 {
        return false;
    }
    let jack_guard_us = u64::from(jack_threshold_ms).saturating_mul(1_000);

    let mut destination_notes: [Vec<NoteEvent>; LANE_COUNT] = std::array::from_fn(|_| Vec::new());
    let mut movable_by_id = HashMap::new();
    for source_lane in Lane::ALL {
        for mut note in std::mem::take(&mut chart.lane_notes[source_lane.index()]) {
            if let Some(destination) = fixed_destination(source_lane) {
                note.lane = destination;
                destination_notes[destination.index()].push(note);
            } else if matches!(source_lane, Lane::Scratch | Lane::Key4)
                && note.kind != NoteKind::Invisible
            {
                movable_by_id.insert(note.id, note);
            }
        }
    }

    let mut destination_long_notes = Vec::new();
    let mut movable_units = Vec::new();
    for mut pair in std::mem::take(&mut chart.long_notes) {
        if let Some(destination) = fixed_destination(pair.lane) {
            pair.lane = destination;
            destination_long_notes.push(pair);
            continue;
        }
        if !matches!(pair.lane, Lane::Scratch | Lane::Key4) {
            continue;
        }
        let start = movable_by_id.remove(&pair.start_note_id);
        let end = movable_by_id.remove(&pair.end_note_id);
        match (start, end) {
            (Some(start), Some(end)) => {
                movable_units.push(MovableUnit { notes: vec![start, end], long_note: Some(pair) });
            }
            (start, end) => {
                for note in start.iter().chain(end.iter()) {
                    push_note_sounds_as_bgm(&mut chart.bgm_events, note);
                }
            }
        }
    }
    movable_units.extend(
        movable_by_id.into_values().map(|note| MovableUnit { notes: vec![note], long_note: None }),
    );
    movable_units.sort_by_key(|unit| (unit.start_time(), unit.notes[0].tick, unit.notes[0].id));

    let mut rng = ArrangeRng::new(seed ^ RNG_DOMAIN, legacy_seed);
    let mut dropped = 0u32;
    let mut index = 0;
    while index < movable_units.len() {
        let time = movable_units[index].start_time();
        let end = movable_units[index..]
            .iter()
            .position(|unit| unit.start_time() != time)
            .map_or(movable_units.len(), |offset| index + offset);
        let group = &movable_units[index..end];
        let assignments =
            best_assignments(group, &destination_notes, &destination_long_notes, jack_guard_us);
        let selected = &assignments[rng.next_usize(assignments.len())];
        for (unit, destination) in group.iter().zip(selected) {
            if let Some(destination) = destination {
                for note in &unit.notes {
                    let mut note = note.clone();
                    note.lane = *destination;
                    destination_notes[destination.index()].push(note);
                }
                if let Some(pair) = &unit.long_note {
                    let mut pair = pair.clone();
                    pair.lane = *destination;
                    destination_long_notes.push(pair);
                }
            } else {
                dropped = dropped.saturating_add(1);
                for note in &unit.notes {
                    push_note_sounds_as_bgm(&mut chart.bgm_events, note);
                }
            }
        }
        index = end;
    }

    for notes in &mut destination_notes {
        notes.sort_by_key(|note| (note.time, note.tick, note.id));
    }
    destination_long_notes
        .sort_by_key(|pair| (pair.start_time, pair.start_tick, pair.start_note_id));
    chart.bgm_events.sort_by_key(|event| (event.time, event.tick, event.sound));
    chart.lane_notes = destination_notes;
    chart.long_notes = destination_long_notes;
    chart.metadata.key_mode = KeyMode::K6;
    chart.total_notes = chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        .count() as u32;
    tracing::info!(dropped, total_notes = chart.total_notes, "converted 7K chart to 6K");
    true
}

fn fixed_destination(source: Lane) -> Option<Lane> {
    match source {
        Lane::Key1 => Some(Lane::Key1),
        Lane::Key2 => Some(Lane::Key2),
        Lane::Key3 => Some(Lane::Key3),
        Lane::Key5 => Some(Lane::Key4),
        Lane::Key6 => Some(Lane::Key5),
        Lane::Key7 => Some(Lane::Key6),
        _ => None,
    }
}

fn best_assignments(
    units: &[MovableUnit],
    notes: &[Vec<NoteEvent>; LANE_COUNT],
    long_notes: &[LongNotePair],
    jack_guard_us: u64,
) -> Vec<Vec<Option<Lane>>> {
    fn visit(
        units: &[MovableUnit],
        notes: &[Vec<NoteEvent>; LANE_COUNT],
        long_notes: &[LongNotePair],
        jack_guard_us: u64,
        index: usize,
        used: &mut HashSet<Lane>,
        current: &mut Vec<Option<Lane>>,
        best_count: &mut usize,
        best: &mut Vec<Vec<Option<Lane>>>,
    ) {
        if index == units.len() {
            let count = current.iter().filter(|lane| lane.is_some()).count();
            if count > *best_count {
                *best_count = count;
                best.clear();
            }
            if count == *best_count {
                best.push(current.clone());
            }
            return;
        }
        for lane in DESTINATION_LANES {
            if used.contains(&lane)
                || !can_place(&units[index], lane, notes, long_notes, jack_guard_us)
            {
                continue;
            }
            used.insert(lane);
            current.push(Some(lane));
            visit(
                units,
                notes,
                long_notes,
                jack_guard_us,
                index + 1,
                used,
                current,
                best_count,
                best,
            );
            current.pop();
            used.remove(&lane);
        }
        current.push(None);
        visit(units, notes, long_notes, jack_guard_us, index + 1, used, current, best_count, best);
        current.pop();
    }

    let mut best = Vec::new();
    let mut best_count = 0;
    visit(
        units,
        notes,
        long_notes,
        jack_guard_us,
        0,
        &mut HashSet::new(),
        &mut Vec::new(),
        &mut best_count,
        &mut best,
    );
    best
}

fn can_place(
    unit: &MovableUnit,
    lane: Lane,
    notes: &[Vec<NoteEvent>; LANE_COUNT],
    long_notes: &[LongNotePair],
    jack_guard_us: u64,
) -> bool {
    let lane_notes = &notes[lane.index()];
    let start = unit.start_time().0;
    let end = unit.long_note.as_ref().map_or(start, |pair| pair.end_time.0);

    if lane_notes
        .iter()
        .any(|note| note.kind != NoteKind::Invisible && note.time.0 >= start && note.time.0 <= end)
    {
        return false;
    }
    if long_notes
        .iter()
        .any(|pair| pair.lane == lane && pair.start_time.0 <= end && pair.end_time.0 >= start)
    {
        return false;
    }
    if unit.jack_times().any(|unit_time| {
        lane_notes
            .iter()
            .any(|note| jack_relevant(note) && note.time.0.abs_diff(unit_time.0) <= jack_guard_us)
    }) {
        return false;
    }
    true
}

fn jack_relevant(note: &NoteEvent) -> bool {
    !matches!(note.kind, NoteKind::Invisible | NoteKind::Mine)
}

fn push_note_sounds_as_bgm(bgm_events: &mut Vec<SoundEvent>, note: &NoteEvent) {
    bgm_events.extend(note.sounds().map(|sound| SoundEvent {
        tick: note.tick,
        time: note.time,
        sound,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, SoundAssetRef};
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::time::ChartTick;

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"seven-to-six"),
            metadata: ChartMetadata {
                key_mode: KeyMode::K7,
                total: Some(180.0),
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::<SoundAssetRef>::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(2_000_000),
        }
    }

    fn note(id: u32, lane: Lane, kind: NoteKind, time_us: i64) -> NoteEvent {
        NoteEvent {
            id: NoteId(id),
            lane,
            kind,
            tick: ChartTick(time_us.max(0) as u64),
            time: TimeUs(time_us),
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        }
    }

    fn push(chart: &mut PlayableChart, note: NoteEvent) {
        chart.lane_notes[note.lane.index()].push(note);
    }

    #[test]
    fn maps_fixed_lanes_and_only_removes_movable_invisible_notes() {
        let mut chart = chart();
        push(&mut chart, note(1, Lane::Key1, NoteKind::Invisible, 10_000));
        push(&mut chart, note(2, Lane::Key5, NoteKind::Tap, 20_000));
        push(&mut chart, note(3, Lane::Scratch, NoteKind::Invisible, 30_000));
        push(&mut chart, note(4, Lane::Key4, NoteKind::Invisible, 40_000));
        push(&mut chart, note(5, Lane::Key4, NoteKind::Tap, 50_000));

        assert!(apply_seven_to_six(&mut chart, 42, false, DEFAULT_H_RANDOM_THRESHOLD_MS));

        assert_eq!(chart.metadata.key_mode, KeyMode::K6);
        assert_eq!(chart.metadata.total, Some(180.0));
        assert!(chart.lane_notes[Lane::Key1.index()].iter().any(|note| note.id == NoteId(1)));
        assert!(chart.lane_notes[Lane::Key4.index()].iter().any(|note| note.id == NoteId(2)));
        assert!(!chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(3)));
        assert!(!chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(4)));
        assert!(chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(5)));
        assert_eq!(chart.total_notes, 2);
    }

    #[test]
    fn sends_unplaceable_note_sounds_to_bgm_and_guards_exactly_one_hundred_ms() {
        let mut chart = chart();
        push(&mut chart, note(1, Lane::Key1, NoteKind::Tap, 0));
        for (id, lane) in
            [(2, Lane::Key2), (3, Lane::Key3), (4, Lane::Key5), (5, Lane::Key6), (6, Lane::Key7)]
        {
            push(&mut chart, note(id, lane, NoteKind::Tap, 100_000));
        }
        let mut movable = note(7, Lane::Scratch, NoteKind::Tap, 100_000);
        movable.sound = Some(SoundId(7));
        movable.layered_sounds.push(SoundId(8));
        push(&mut chart, movable);

        apply_seven_to_six(&mut chart, 7, false, 100);

        assert!(!chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(7)));
        assert_eq!(
            chart.bgm_events.iter().map(|event| event.sound).collect::<Vec<_>>(),
            vec![SoundId(7), SoundId(8)]
        );
        assert_eq!(chart.total_notes, 6);
    }

    /// Key1 at 0 and Key2..Key6 at `time_us`, plus a movable Scratch note at
    /// `time_us`: the only free destination is Key1, `time_us` after its note.
    fn single_free_lane_chart(time_us: i64) -> PlayableChart {
        let mut chart = chart();
        push(&mut chart, note(1, Lane::Key1, NoteKind::Tap, 0));
        for (id, lane) in
            [(2, Lane::Key2), (3, Lane::Key3), (4, Lane::Key5), (5, Lane::Key6), (6, Lane::Key7)]
        {
            push(&mut chart, note(id, lane, NoteKind::Tap, time_us));
        }
        push(&mut chart, note(7, Lane::Scratch, NoteKind::Tap, time_us));
        chart
    }

    fn movable_placed(chart: &PlayableChart) -> bool {
        chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(7))
    }

    #[test]
    fn jack_guard_follows_the_h_random_threshold() {
        // 120ms is outside the historical 100ms guard but inside the 125ms
        // H-RANDOM default, so only the latter refuses to stack it on Key1.
        let mut legacy = single_free_lane_chart(120_000);
        apply_seven_to_six(&mut legacy, 7, false, 100);
        assert!(movable_placed(&legacy));

        let mut default = single_free_lane_chart(120_000);
        apply_seven_to_six(&mut default, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        assert!(!movable_placed(&default));

        // The threshold itself is still guarded; one microsecond beyond is free.
        let mut boundary = single_free_lane_chart(125_000);
        apply_seven_to_six(&mut boundary, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        assert!(!movable_placed(&boundary));
        let mut beyond = single_free_lane_chart(125_001);
        apply_seven_to_six(&mut beyond, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        assert!(movable_placed(&beyond));
    }

    #[test]
    fn simultaneous_scratch_and_key4_use_distinct_lanes_deterministically() {
        let mut first = chart();
        push(&mut first, note(1, Lane::Scratch, NoteKind::Tap, 1_000_000));
        push(&mut first, note(2, Lane::Key4, NoteKind::Tap, 1_000_000));
        let mut second = first.clone();

        apply_seven_to_six(&mut first, 1234, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        apply_seven_to_six(&mut second, 1234, false, DEFAULT_H_RANDOM_THRESHOLD_MS);

        let lanes = |chart: &PlayableChart| {
            let mut result = chart
                .lane_notes
                .iter()
                .flatten()
                .map(|note| (note.id, note.lane))
                .collect::<Vec<_>>();
            result.sort_by_key(|entry| entry.0);
            result
        };
        assert_eq!(lanes(&first), lanes(&second));
        assert_ne!(lanes(&first)[0].1, lanes(&first)[1].1);
    }

    #[test]
    fn note_level_arranges_remain_available_after_conversion() {
        for arrange in [ArrangeOption::SRandom, ArrangeOption::Spiral] {
            let mut chart = chart();
            push(&mut chart, note(1, Lane::Scratch, NoteKind::Tap, 1_000_000));
            apply_seven_to_six(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            let applied = apply_arrange(&mut chart, arrange, Some(2), None);
            assert_eq!(applied.arrange, arrange);
            assert_eq!(chart.metadata.key_mode, KeyMode::K6);
        }
        assert_eq!(
            normalize_arrange_for_seven_to_six(ArrangeOption::SRandomEx),
            ArrangeOption::SRandom
        );
        assert_eq!(
            normalize_arrange_for_seven_to_six(ArrangeOption::RandomEx),
            ArrangeOption::Random
        );
        assert_eq!(
            normalize_arrange_for_seven_to_six(ArrangeOption::AllScratch),
            ArrangeOption::Normal
        );
    }
}
