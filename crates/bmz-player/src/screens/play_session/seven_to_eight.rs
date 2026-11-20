#[path = "seven_to_eight/phrases.rs"]
mod phrases;

use super::seven_to_six::{MovableUnit, can_place, push_note_sounds_as_bgm};
use super::*;
use bmz_chart::model::FreeZone;
use bmz_core::time::ChartTick;
use phrases::layout_at;
use std::collections::{HashMap, HashSet};

const RNG_DOMAIN: i64 = 0x0037_4b38;
const MEASURES_PER_BLOCK: usize = 4;
const FALLBACK_BLOCK_US: i64 = 8_000_000;
#[cfg(test)]
const SOURCE_KEYS: [Lane; 7] =
    [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6, Lane::Key7];
const DESTINATION_LANES: [Lane; 8] = [
    Lane::Key1,
    Lane::Key2,
    Lane::Key3,
    Lane::Key4,
    Lane::Key5,
    Lane::Key6,
    Lane::Key7,
    Lane::Key8,
];

/// Place keyboard phrases and scratches on eight keys, keeping the notes of
/// each phrase together. Unplaceable scratch units retain their sounds as BGM.
pub fn apply_seven_to_eight(
    chart: &mut PlayableChart,
    seed: i64,
    legacy_seed: bool,
    jack_threshold_ms: u32,
) -> bool {
    if chart.metadata.key_mode != KeyMode::K7 {
        return false;
    }
    let jack_guard_us = u64::from(jack_threshold_ms).saturating_mul(1_000);
    let layouts = phrases::plan(chart, jack_guard_us);
    let inner: HashSet<_> = chart
        .metadata
        .free_zones
        .iter()
        .flat_map(|zone| zone.inner_notes.iter().copied())
        .collect();
    // A free zone and the notes judged through it must share the planned lane.
    for zone in &mut chart.metadata.free_zones {
        if zone.lane.index() < 8 {
            zone.lane = destination_lane(layout_at(&layouts, zone.start_time)[zone.lane.index()]);
        }
    }

    let mut destination_notes: [Vec<NoteEvent>; LANE_COUNT] = std::array::from_fn(|_| Vec::new());
    let mut movable_by_id = HashMap::new();
    for source_lane in Lane::ALL {
        for mut note in std::mem::take(&mut chart.lane_notes[source_lane.index()]) {
            let fixed_zone_note = inner.contains(&note.id) || note.kind == NoteKind::FreeZoneEnd;
            if (1..=7).contains(&source_lane.index())
                || (source_lane == Lane::Scratch && fixed_zone_note)
            {
                note.lane = destination_lane(layout_at(&layouts, note.time)[source_lane.index()]);
                destination_notes[note.lane.index()].push(note);
            } else if source_lane == Lane::Scratch && note.kind != NoteKind::Invisible {
                movable_by_id.insert(note.id, note);
            }
        }
    }

    let mut destination_long_notes = Vec::new();
    let mut movable_units = Vec::new();
    for mut pair in std::mem::take(&mut chart.long_notes) {
        if (1..=7).contains(&pair.lane.index())
            || (pair.lane == Lane::Scratch && inner.contains(&pair.start_note_id))
        {
            pair.lane = destination_lane(layout_at(&layouts, pair.start_time)[pair.lane.index()]);
            destination_long_notes.push(pair);
            continue;
        }
        if pair.lane != Lane::Scratch {
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
    let mut previous_scratch = None;
    let mut dropped = 0u32;
    let mut index = 0;
    while index < movable_units.len() {
        let time = movable_units[index].start_time();
        let end = movable_units[index..]
            .iter()
            .position(|unit| unit.start_time() != time)
            .map_or(movable_units.len(), |offset| index + offset);
        let group = &movable_units[index..end];
        let ranks = lane_ranks(previous_scratch, layout_at(&layouts, time)[0]);
        let assignments = best_assignments(
            group,
            &ranks,
            &destination_notes,
            &destination_long_notes,
            &chart.metadata.free_zones,
            jack_guard_us,
        );
        let selected = &assignments[rng.next_usize(assignments.len())];
        for (unit, destination) in group.iter().zip(selected) {
            if let Some(destination) = destination {
                // Key1..Key8 have indices 1..8, the same as the lane numbers.
                previous_scratch = Some(destination.index());
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
    chart.metadata.key_mode = KeyMode::K8;
    chart.total_notes = chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| note.kind.is_scored_head() && !inner.contains(&note.id))
        .count() as u32;
    tracing::info!(
        dropped,
        phrases = layouts.len(),
        total_notes = chart.total_notes,
        "converted 7K chart to 8K"
    );
    true
}

/// Groups source measures for the decaying lane-load history.
///
/// Group the source measures from the first real bar line. Charts with fewer
/// than two distinct source bar lines use fixed windows from the first note.
enum BlockIndex {
    Bars(Vec<u64>),
    Windows { origin_us: i64 },
}

impl BlockIndex {
    fn new(chart: &PlayableChart) -> Self {
        let mut ticks = chart
            .bar_lines
            .iter()
            .filter(|bar| bar.time >= chart.metadata.start_note_margin)
            .map(|bar| bar.tick.0)
            .collect::<Vec<_>>();
        ticks.sort_unstable();
        ticks.dedup();
        if ticks.len() >= 2 {
            return Self::Bars(ticks);
        }
        let origin_us =
            chart.lane_notes.iter().flatten().map(|note| note.time.0).min().unwrap_or(0);
        Self::Windows { origin_us }
    }

    fn block_of(&self, note: &NoteEvent) -> usize {
        self.block_at(note.tick, note.time)
    }

    fn block_at(&self, tick: ChartTick, time: TimeUs) -> usize {
        match self {
            Self::Bars(ticks) => {
                ticks.partition_point(|bar| *bar <= tick.0).saturating_sub(1) / MEASURES_PER_BLOCK
            }
            Self::Windows { origin_us } => {
                (time.0.saturating_sub(*origin_us).max(0) / FALLBACK_BLOCK_US) as usize
            }
        }
    }
}

fn destination_lane(number: usize) -> Lane {
    DESTINATION_LANES[number - 1]
}

/// Use the scratch position selected with the keyboard phrase when available.
fn lane_ranks(previous: Option<usize>, planned: usize) -> [usize; 8] {
    let mut order = (1..=DESTINATION_LANES.len()).collect::<Vec<_>>();
    order.sort_by_key(|lane| {
        (*lane != planned, Some(*lane) != previous, lane.abs_diff(planned), *lane)
    });
    let mut ranks = [0; 8];
    for (rank, lane) in order.into_iter().enumerate() {
        ranks[lane - 1] = rank;
    }
    ranks
}

/// Distinct-lane assignments that place the most units, then have the lowest
/// rank sum. The caller picks one of the ties with the seeded RNG.
fn best_assignments(
    units: &[MovableUnit],
    ranks: &[usize; 8],
    notes: &[Vec<NoteEvent>; LANE_COUNT],
    long_notes: &[LongNotePair],
    free_zones: &[FreeZone],
    jack_guard_us: u64,
) -> Vec<Vec<Option<Lane>>> {
    struct Search<'a> {
        units: &'a [MovableUnit],
        ranks: &'a [usize; 8],
        notes: &'a [Vec<NoteEvent>; LANE_COUNT],
        long_notes: &'a [LongNotePair],
        free_zones: &'a [FreeZone],
        jack_guard_us: u64,
        used: HashSet<Lane>,
        current: Vec<Option<Lane>>,
        best_key: Option<(usize, usize)>,
        best: Vec<Vec<Option<Lane>>>,
    }

    impl Search<'_> {
        fn visit(&mut self, index: usize, placed: usize, cost: usize) {
            if index == self.units.len() {
                // Prefer more placed units, then a lower rank sum.
                let key = (usize::MAX - placed, cost);
                if self.best_key.is_none_or(|best| key < best) {
                    self.best_key = Some(key);
                    self.best.clear();
                }
                if self.best_key == Some(key) {
                    self.best.push(self.current.clone());
                }
                return;
            }
            let unit = &self.units[index];
            let start = unit.start_time();
            let end = unit.long_note.as_ref().map_or(start, |pair| pair.end_time);
            for (number, lane) in DESTINATION_LANES.into_iter().enumerate() {
                if self.used.contains(&lane)
                    || self.free_zones.iter().any(|zone| {
                        zone.lane == lane && zone.start_time <= end && start <= zone.end_time
                    })
                    || !can_place(
                        &self.units[index],
                        lane,
                        self.notes,
                        self.long_notes,
                        self.jack_guard_us,
                    )
                {
                    continue;
                }
                self.used.insert(lane);
                self.current.push(Some(lane));
                self.visit(index + 1, placed + 1, cost + self.ranks[number]);
                self.current.pop();
                self.used.remove(&lane);
            }
            self.current.push(None);
            self.visit(index + 1, placed, cost);
            self.current.pop();
        }
    }

    let mut search = Search {
        units,
        ranks,
        notes,
        long_notes,
        free_zones,
        jack_guard_us,
        used: HashSet::new(),
        current: Vec::new(),
        best_key: None,
        best: Vec::new(),
    };
    search.visit(0, 0, 0);
    search.best
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{BarLine, ChartMetadata, LongNoteStyle, SoundAssetRef};
    use bmz_core::ids::{NoteId, SoundId};

    const MEASURE_US: i64 = 1_000_000;

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"seven-to-eight"),
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
            bar_lines: (0..8)
                .map(|measure| BarLine {
                    measure,
                    tick: ChartTick(u64::from(measure) * MEASURE_US as u64),
                    time: TimeUs(i64::from(measure) * MEASURE_US),
                })
                .collect(),
            sounds: Vec::<SoundAssetRef>::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(8 * MEASURE_US),
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

    fn push_long(chart: &mut PlayableChart, start_id: u32, lane: Lane, start_us: i64, end_us: i64) {
        push(chart, note(start_id, lane, NoteKind::LongStart, start_us));
        push(chart, note(start_id + 1, lane, NoteKind::LongEnd, end_us));
        chart.long_notes.push(LongNotePair {
            lane,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(start_id),
            end_note_id: NoteId(start_id + 1),
            start_tick: ChartTick(start_us as u64),
            end_tick: ChartTick(end_us as u64),
            start_time: TimeUs(start_us),
            end_time: TimeUs(end_us),
            sound: None,
        });
    }

    /// Two runs with 48 keyboard presses in total.
    fn left_then_right_chart() -> PlayableChart {
        let mut chart = chart();
        for (keys, measure) in [(1..=4, 1), (4..=7, 2)] {
            for key in keys {
                for step in 0..6 {
                    let time = measure * MEASURE_US + (key as i64) * 50_000 + step * 100_000;
                    let id = 100 + measure as u32 * 100 + key as u32 * 10 + step as u32;
                    push(&mut chart, note(id, SOURCE_KEYS[key - 1], NoteKind::Tap, time));
                }
            }
        }
        chart
    }

    fn lane_of(chart: &PlayableChart, id: u32) -> Lane {
        chart
            .lane_notes
            .iter()
            .flatten()
            .find(|note| note.id == NoteId(id))
            .map(|note| note.lane)
            .expect("note kept")
    }

    fn put_keys(chart: &mut PlayableChart, id: u32, keys: &[usize], time: i64, interval: i64) {
        for (i, &k) in keys.iter().enumerate() {
            push(
                chart,
                note(id + i as u32, SOURCE_KEYS[k - 1], NoteKind::Tap, time + i as i64 * interval),
            );
        }
    }

    fn output_keys(chart: &PlayableChart, first: u32, count: u32) -> Vec<usize> {
        (first..first + count).map(|id| lane_of(chart, id).index()).collect()
    }

    #[test]
    fn two_stairs_use_all_eight_keys_in_one_measure() {
        let mut source = chart();
        put_keys(&mut source, 10, &[1, 2, 3, 4], 0, 100_000);
        put_keys(&mut source, 20, &[7, 6, 5, 4], 500_000, 100_000);
        for padded in [false, true] {
            let mut chart = source.clone();
            if padded {
                bmz_chart::start_margin::apply_fixed_start_margin(&mut chart, TimeUs(MEASURE_US));
            }
            apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            assert_eq!(output_keys(&chart, 10, 4), [1, 2, 3, 4]);
            assert_eq!(output_keys(&chart, 20, 4), [8, 7, 6, 5]);
        }
    }

    #[test]
    fn interleaved_scratch_and_bar_line_do_not_split_trills_or_jacks() {
        for keys in [[2, 4, 2, 4, 2, 4], [5, 5, 5, 5, 5, 5]] {
            let mut chart = chart();
            put_keys(&mut chart, 10, &keys, MEASURE_US - 250_000, 100_000);
            push(&mut chart, note(1, Lane::Scratch, NoteKind::Tap, MEASURE_US));
            apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            let actual = output_keys(&chart, 10, 6);
            let shift = actual[0] as i32 - keys[0] as i32;
            assert!(actual.iter().zip(keys).all(|(&d, k)| d as i32 - k as i32 == shift));
            assert_eq!(chart.total_notes, 7);
        }
    }

    #[test]
    fn immediate_repeated_riff_uses_the_same_fingers() {
        let mut chart = chart();
        put_keys(&mut chart, 10, &[3, 4, 5], MEASURE_US, 75_000);
        put_keys(&mut chart, 20, &[3, 4, 5], MEASURE_US + 450_000, 75_000);
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        assert_eq!(output_keys(&chart, 10, 3), output_keys(&chart, 20, 3));
    }

    #[test]
    fn dense_mixed_chords_can_leave_an_interior_lane_open() {
        let chords = [[1, 3, 6], [2, 4, 7], [1, 5, 7], [2, 4, 6]];
        for interval in [80_000, 240_000] {
            let mut chart = chart();
            for row in 0..48u32 {
                put_keys(
                    &mut chart,
                    row * 3,
                    &chords[row as usize % 4],
                    i64::from(row) * interval,
                    0,
                );
            }
            apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            let mut gaps = Vec::new();
            for phrase in 0..12 {
                let mut used = HashSet::new();
                for (row, source) in chords.iter().enumerate() {
                    let output = output_keys(&chart, phrase * 12 + row as u32 * 3, 3);
                    assert!(
                        output
                            .windows(2)
                            .zip(source.windows(2))
                            .all(|(a, b)| a[1] >= a[0] + b[1] - b[0])
                    );
                    assert!(output[2] - output[0] <= source[2] - source[0] + 1);
                    used.extend(output);
                }
                assert_eq!(used.len(), 7);
                gaps.push((1..=8).find(|k| !used.contains(k)).unwrap());
            }
            if interval == 80_000 {
                assert!(gaps.iter().any(|g| (2..=7).contains(g)), "{gaps:?}");
                assert!(gaps.windows(4).all(|w| w.iter().any(|g| g != &w[0])), "{gaps:?}");
            } else {
                assert!(gaps.iter().all(|g| matches!(g, 1 | 8)), "{gaps:?}");
            }
        }
    }

    #[test]
    fn sustained_full_chords_move_between_cycles_without_new_jacks() {
        let mut chart = chart();
        for row in 0..32 {
            put_keys(&mut chart, row * 7, &[1, 2, 3, 4, 5, 6, 7], i64::from(row) * 100_000, 0);
        }
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        let mut previous = Vec::new();
        let mut gaps = HashSet::new();
        for row in 0..32 {
            let current = output_keys(&chart, row * 7, 7);
            if row % 8 != 0 {
                assert_eq!(current, previous);
            } else {
                for (key, lane) in current.iter().enumerate() {
                    if let Some(other) = previous.iter().position(|p| p == lane) {
                        assert_eq!(other, key);
                    }
                }
                gaps.insert((1..=8).find(|k| !current.contains(k)).unwrap());
            }
            previous = current;
        }
        assert!(gaps.len() >= 3, "{gaps:?}");
    }

    #[test]
    fn a_stair_ending_in_a_jack_keeps_the_shared_key() {
        let mut chart = chart();
        chart.bar_lines.clear();
        let source = [4, 5, 6, 6, 6, 6];
        put_keys(&mut chart, 10, &source, 7_800_000, 100_000);
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        let keys = output_keys(&chart, 10, 6);
        assert_eq!(keys[1], keys[0] + 1);
        assert_eq!(keys[2], keys[1] + 1);
        assert!(keys[2..].iter().all(|k| *k == keys[2]));
    }

    #[test]
    fn scratch_free_zone_and_its_inner_notes_move_as_one() {
        let mut chart = chart();
        for id in 1..=2 {
            push(&mut chart, note(id, Lane::Scratch, NoteKind::Tap, i64::from(id) * 50_000));
        }
        let end = MEASURE_US;
        push(&mut chart, note(3, Lane::Scratch, NoteKind::FreeZoneEnd, end));
        chart.metadata.free_zones.push(FreeZone {
            note_id: NoteId(3),
            lane: Lane::Scratch,
            start_tick: ChartTick(0),
            end_tick: ChartTick(end as u64),
            start_time: TimeUs(0),
            end_time: TimeUs(end),
            inner_notes: vec![NoteId(1), NoteId(2)],
        });
        put_keys(&mut chart, 10, &[5, 6, 7], 400_000, 100_000);
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        let lane = chart.metadata.free_zones[0].lane;
        assert!(DESTINATION_LANES.contains(&lane));
        assert_eq!(output_keys(&chart, 1, 3), [lane.index(); 3]);
        assert!(!output_keys(&chart, 10, 3).contains(&lane.index()));
        assert_eq!(chart.total_notes, 4);
    }

    #[test]
    fn chord_intervals_and_simultaneous_scratch_survive_together() {
        let mut chart = chart();
        put_keys(&mut chart, 10, &[2, 4, 6], MEASURE_US, 0);
        push(&mut chart, note(1, Lane::Scratch, NoteKind::Tap, MEASURE_US));
        apply_seven_to_eight(&mut chart, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        let keys = output_keys(&chart, 10, 3);
        assert_eq!(keys[1] - keys[0], 2);
        assert_eq!(keys[2] - keys[1], 2);
        assert!(!keys.contains(&lane_of(&chart, 1).index()));
    }

    #[test]
    fn other_phrases_can_move_during_a_keyboard_or_scratch_hold() {
        for held in [Lane::Key1, Lane::Scratch] {
            let mut chart = chart();
            push_long(&mut chart, 1, held, 0, 2 * MEASURE_US);
            put_keys(&mut chart, 10, &[2, 3, 4], MEASURE_US / 4, 75_000);
            put_keys(&mut chart, 20, &[7, 6, 5, 4], MEASURE_US, 75_000);
            apply_seven_to_eight(&mut chart, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            let pinned = lane_of(&chart, 1);
            assert_eq!(pinned, lane_of(&chart, 2));
            assert_eq!(pinned, chart.long_notes[0].lane);
            assert_eq!(chart.lane_notes[pinned.index()].len(), 2);
            assert!(output_keys(&chart, 10, 3).windows(2).all(|p| p[1] == p[0] + 1));
            assert!(output_keys(&chart, 20, 4).windows(2).all(|p| p[0] == p[1] + 1));
        }
    }

    #[test]
    fn scratch_becomes_bgm_only_when_the_chord_and_jack_guard_fill_every_lane() {
        let mut chart = chart();
        for (id, time) in [(10, MEASURE_US), (20, MEASURE_US + 100_000)] {
            put_keys(&mut chart, id, &[1, 2, 3, 4, 5, 6, 7], time, 0);
            let mut scratch = note(id + 7, Lane::Scratch, NoteKind::Tap, time);
            scratch.sound = Some(SoundId(id));
            push(&mut chart, scratch);
        }
        apply_seven_to_eight(&mut chart, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        assert!(!output_keys(&chart, 10, 7).contains(&lane_of(&chart, 17).index()));
        assert_eq!(chart.bgm_events.iter().map(|e| e.sound).collect::<Vec<_>>(), [SoundId(20)]);
        assert_eq!(chart.total_notes, 15);
    }

    #[test]
    fn converts_to_eight_keys_and_preserves_keyboard_notes() {
        let mut chart = left_then_right_chart();
        push(&mut chart, note(1, Lane::Key2, NoteKind::Invisible, MEASURE_US + 10_000));
        push(&mut chart, note(2, Lane::Scratch, NoteKind::Invisible, MEASURE_US + 20_000));
        push(&mut chart, note(3, Lane::Scratch, NoteKind::Tap, 3 * MEASURE_US));
        push(&mut chart, note(4, Lane::Key7, NoteKind::Mine, 3 * MEASURE_US + 500_000));
        let keyboard_notes =
            SOURCE_KEYS.iter().map(|lane| chart.lane_notes[lane.index()].len()).sum::<usize>();

        assert!(apply_seven_to_eight(&mut chart, 9, false, DEFAULT_H_RANDOM_THRESHOLD_MS));

        assert_eq!(chart.metadata.key_mode, KeyMode::K8);
        assert_eq!(chart.metadata.total, Some(180.0));
        assert!(chart.lane_notes[Lane::Scratch.index()].is_empty());
        let lanes = chart.lane_notes.iter().flatten().map(|note| note.lane).collect::<Vec<_>>();
        assert!(lanes.iter().all(|lane| DESTINATION_LANES.contains(lane)));
        assert!(!chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(2)));
        let kept = chart.lane_notes.iter().flatten().filter(|note| note.id != NoteId(3)).count();
        assert_eq!(kept, keyboard_notes);
        // 48 keyboard taps plus the placed scratch.
        assert_eq!(chart.total_notes, 49);

        let mut not_seven = self::chart();
        not_seven.metadata.key_mode = KeyMode::K5;
        assert!(!apply_seven_to_eight(&mut not_seven, 9, false, DEFAULT_H_RANDOM_THRESHOLD_MS));
    }
}
