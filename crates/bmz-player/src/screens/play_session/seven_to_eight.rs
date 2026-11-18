use super::seven_to_six::{MovableUnit, can_place, push_note_sounds_as_bgm};
use super::*;
use bmz_core::time::ChartTick;
use std::collections::{HashMap, HashSet};

const RNG_DOMAIN: i64 = 0x0037_4b38;
/// λ of the gap switch penalty: switching costs this much per keyboard note in
/// the block. Experimental; tune it after trying the conversion on charts.
const GAP_SWITCH_PENALTY_PER_NOTE: f64 = 1.0;
/// Block length from the first note when the chart has no usable bar lines.
const FALLBACK_BLOCK_US: i64 = 2_000_000;
/// Gap used while no block has decided one yet (the lower of the two centre lanes).
const INITIAL_GAP: usize = 4;
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
/// Destination lanes 1..=LEFT_HAND_LANES are counted as the left hand.
const LEFT_HAND_LANES: usize = 4;

/// Converts a source 7K chart into eight scratch-less lanes (experimental).
///
/// Each measure (or a fixed window when the chart has no bar lines) leaves one
/// gap lane empty and spreads the seven keys over the others in order. The
/// gap only changes between measures and never while a long note continues
/// from the previous measure. Scratch notes then move to a free lane while
/// avoiding press notes at intervals of `jack_threshold_ms` or less, the same
/// way as 7K to 6K; a scratch that cannot be placed becomes BGM, and invisible
/// scratch notes are discarded.
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
    let blocks = BlockIndex::new(chart);
    let gaps = select_gaps(chart, &blocks);

    // Long notes take the gap of the block where they start for both ends.
    let mut long_end_gaps = HashMap::new();
    for pair in &chart.long_notes {
        if source_key_number(pair.lane).is_some() {
            let gap = gap_at(&gaps, blocks.block_at(pair.start_tick, pair.start_time));
            long_end_gaps.insert(pair.end_note_id, gap);
        }
    }

    let mut destination_notes: [Vec<NoteEvent>; LANE_COUNT] = std::array::from_fn(|_| Vec::new());
    let mut movable_by_id = HashMap::new();
    for source_lane in Lane::ALL {
        for mut note in std::mem::take(&mut chart.lane_notes[source_lane.index()]) {
            if let Some(key) = source_key_number(source_lane) {
                let gap = long_end_gaps
                    .get(&note.id)
                    .copied()
                    .unwrap_or_else(|| gap_at(&gaps, blocks.block_of(&note)));
                note.lane = destination_lane(embed(key, gap));
                destination_notes[note.lane.index()].push(note);
            } else if source_lane == Lane::Scratch && note.kind != NoteKind::Invisible {
                movable_by_id.insert(note.id, note);
            }
        }
    }

    let mut destination_long_notes = Vec::new();
    let mut movable_units = Vec::new();
    for mut pair in std::mem::take(&mut chart.long_notes) {
        if let Some(key) = source_key_number(pair.lane) {
            let gap = gap_at(&gaps, blocks.block_at(pair.start_tick, pair.start_time));
            pair.lane = destination_lane(embed(key, gap));
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
        let ranks = group
            .iter()
            .map(|unit| {
                let first = &unit.notes[0];
                lane_ranks(previous_scratch, gap_at(&gaps, blocks.block_at(first.tick, time)))
            })
            .collect::<Vec<_>>();
        let assignments = best_assignments(
            group,
            &ranks,
            &destination_notes,
            &destination_long_notes,
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
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        .count() as u32;
    tracing::info!(
        dropped,
        blocks = gaps.len(),
        total_notes = chart.total_notes,
        "converted 7K chart to 8K"
    );
    true
}

/// Splits the chart into gap blocks.
///
/// Notes carry no measure number, so a note belongs to the measure of the last
/// bar line at or before its tick (block 0 is anything before the first bar
/// line). Charts with fewer than two distinct bar lines, such as BMSON without
/// lines, use fixed windows from the first note instead.
enum BlockIndex {
    Bars(Vec<u64>),
    Windows { origin_us: i64 },
}

impl BlockIndex {
    fn new(chart: &PlayableChart) -> Self {
        let mut ticks = chart.bar_lines.iter().map(|bar| bar.tick.0).collect::<Vec<_>>();
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
            Self::Bars(ticks) => ticks.partition_point(|bar| *bar <= tick.0),
            Self::Windows { origin_us } => {
                (time.0.saturating_sub(*origin_us).max(0) / FALLBACK_BLOCK_US) as usize
            }
        }
    }
}

/// Chooses the gap lane (1..=8) of every block in order without randomness.
fn select_gaps(chart: &PlayableChart, blocks: &BlockIndex) -> Vec<usize> {
    let block_count = chart
        .lane_notes
        .iter()
        .flatten()
        .map(|note| blocks.block_of(note) + 1)
        .chain(
            chart.long_notes.iter().map(|pair| blocks.block_at(pair.end_tick, pair.end_time) + 1),
        )
        .max()
        .unwrap_or(0);

    let mut counts = vec![[0u32; 7]; block_count];
    for (key, lane) in SOURCE_KEYS.iter().enumerate() {
        for note in &chart.lane_notes[lane.index()] {
            if matches!(note.kind, NoteKind::Tap | NoteKind::LongStart | NoteKind::Mine) {
                counts[blocks.block_of(note)][key] += 1;
            }
        }
    }

    // A long note started in an earlier block keeps the gap of that block.
    let mut locked = vec![false; block_count];
    for pair in &chart.long_notes {
        if pair.lane != Lane::Scratch && source_key_number(pair.lane).is_none() {
            continue;
        }
        let start = blocks.block_at(pair.start_tick, pair.start_time);
        let end = blocks.block_at(pair.end_tick, pair.end_time);
        for flag in locked.iter_mut().take(end + 1).skip(start + 1) {
            *flag = true;
        }
    }

    let mut gaps = Vec::with_capacity(block_count);
    let mut previous = None;
    for (block, counts) in counts.iter().enumerate() {
        let total = counts.iter().sum::<u32>();
        let gap = match previous {
            Some(previous) if locked[block] || total == 0 => Some(previous),
            None if total == 0 => None,
            _ => Some(best_gap(counts, total, previous)),
        };
        previous = gap.or(previous);
        gaps.push(gap);
    }
    // Blocks before the first keyboard note follow the first decided gap.
    let first = gaps.iter().flatten().copied().next().unwrap_or(INITIAL_GAP);
    gaps.into_iter().map(|gap| gap.unwrap_or(first)).collect()
}

fn best_gap(counts: &[u32; 7], total: u32, previous: Option<usize>) -> usize {
    let mut best: Option<(f64, usize, usize)> = None;
    for gap in 1..=DESTINATION_LANES.len() {
        let switch = previous.is_some_and(|previous| previous != gap);
        let penalty = if switch { GAP_SWITCH_PENALTY_PER_NOTE * f64::from(total) } else { 0.0 };
        let score = gap_load_score(counts, gap) + penalty;
        // Distance to the previous gap, or twice the distance to the centre
        // (4.5) for the first block so both stay integers.
        let distance = previous.map_or((2 * gap).abs_diff(9), |previous| previous.abs_diff(gap));
        // Gaps are visited in ascending order, so a full tie keeps the lower one.
        if best.is_none_or(|best| score < best.0 || (score == best.0 && distance < best.1)) {
            best = Some((score, distance, gap));
        }
    }
    best.map_or(INITIAL_GAP, |best| best.2)
}

/// Load term of the gap cost: the squared note counts of the two hands
/// (destination lanes 1-4 and 5-8) after embedding.
///
/// Per-lane squared counts are the same for every gap because the embedding
/// only moves whole lanes, so the hand split is what makes gaps comparable.
fn gap_load_score(counts: &[u32; 7], gap: usize) -> f64 {
    let mut hands = [0u64; 2];
    for (index, count) in counts.iter().enumerate() {
        let hand = usize::from(embed(index + 1, gap) > LEFT_HAND_LANES);
        hands[hand] += u64::from(*count);
    }
    hands.iter().map(|count| (count * count) as f64).sum()
}

/// Destination lane number (1..=8) of source key `key` (1..=7) around `gap`.
const fn embed(key: usize, gap: usize) -> usize {
    if key < gap { key } else { key + 1 }
}

fn source_key_number(lane: Lane) -> Option<usize> {
    SOURCE_KEYS.iter().position(|key| *key == lane).map(|index| index + 1)
}

fn destination_lane(number: usize) -> Lane {
    DESTINATION_LANES[number - 1]
}

fn gap_at(gaps: &[usize], block: usize) -> usize {
    gaps.get(block).or(gaps.last()).copied().unwrap_or(INITIAL_GAP)
}

/// Rank of each destination lane for a scratch unit: the previous scratch
/// lane, then the gap, then lanes nearest to the previous scratch lane (ties
/// nearest to the gap, then the lower lane).
fn lane_ranks(previous: Option<usize>, gap: usize) -> [usize; 8] {
    let mut order = (1..=DESTINATION_LANES.len()).collect::<Vec<_>>();
    order.sort_by_key(|lane| {
        (
            Some(*lane) != previous,
            *lane != gap,
            previous.map_or(0, |previous| previous.abs_diff(*lane)),
            lane.abs_diff(gap),
            *lane,
        )
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
    ranks: &[[usize; 8]],
    notes: &[Vec<NoteEvent>; LANE_COUNT],
    long_notes: &[LongNotePair],
    jack_guard_us: u64,
) -> Vec<Vec<Option<Lane>>> {
    struct Search<'a> {
        units: &'a [MovableUnit],
        ranks: &'a [[usize; 8]],
        notes: &'a [Vec<NoteEvent>; LANE_COUNT],
        long_notes: &'a [LongNotePair],
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
            for (number, lane) in DESTINATION_LANES.into_iter().enumerate() {
                if self.used.contains(&lane)
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
                self.visit(index + 1, placed + 1, cost + self.ranks[index][number]);
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

    /// Measure 1 presses Key1..Key4 twice each (left heavy) and measure 2
    /// presses Key4..Key7 twice each (right heavy). Ids are 100 + key * 10 + n.
    fn left_then_right_chart() -> PlayableChart {
        let mut chart = chart();
        for (keys, measure) in [(1..=4, 1), (4..=7, 2)] {
            for key in keys {
                for step in 0..2 {
                    let time = measure * MEASURE_US + (key as i64) * 50_000 + step * 400_000;
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

    fn lanes_in(chart: &PlayableChart, measure: i64) -> HashSet<Lane> {
        chart
            .lane_notes
            .iter()
            .flatten()
            .filter(|note| note.time.0 / MEASURE_US == measure)
            .map(|note| note.lane)
            .collect()
    }

    #[test]
    fn gap_switches_between_left_and_right_heavy_measures() {
        let mut chart = left_then_right_chart();
        assert!(apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS));

        // Measure 1: Key4 goes to the lighter right hand, gap at lane 4.
        assert_eq!(lane_of(&chart, 210), Lane::Key1);
        assert_eq!(lane_of(&chart, 240), Lane::Key5);
        assert!(!lanes_in(&chart, 1).contains(&Lane::Key4));
        // Measure 2: Key4 goes to the lighter left hand, gap at lane 5.
        assert_eq!(lane_of(&chart, 340), Lane::Key4);
        assert_eq!(lane_of(&chart, 370), Lane::Key8);
        assert!(!lanes_in(&chart, 2).contains(&Lane::Key5));
    }

    #[test]
    fn gap_switches_with_fixed_windows_without_bar_lines() {
        let mut chart = left_then_right_chart();
        chart.bar_lines = vec![BarLine { measure: 0, tick: ChartTick(0), time: TimeUs(0) }];
        // The first note at 1.05s starts the 2s windows, so stretch the right
        // heavy part into the second window.
        for lane in &mut chart.lane_notes {
            for note in lane.iter_mut().filter(|note| note.time.0 >= 2 * MEASURE_US) {
                note.time = TimeUs(note.time.0 + 2 * MEASURE_US);
                note.tick = ChartTick(note.time.0 as u64);
            }
        }
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);

        assert_eq!(lane_of(&chart, 240), Lane::Key5);
        assert_eq!(lane_of(&chart, 340), Lane::Key4);
    }

    #[test]
    fn long_note_across_a_measure_keeps_the_gap() {
        let mut chart = left_then_right_chart();
        push_long(&mut chart, 1, Lane::Key1, MEASURE_US + 900_000, 2 * MEASURE_US + 500_000);
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);

        // Measure 2 is locked to the gap of measure 1 (lane 4).
        assert_eq!(lane_of(&chart, 340), Lane::Key5);
        assert_eq!(lane_of(&chart, 370), Lane::Key8);
        assert!(!lanes_in(&chart, 2).contains(&Lane::Key4));
        assert_eq!(lane_of(&chart, 1), Lane::Key1);
        assert_eq!(lane_of(&chart, 2), Lane::Key1);
        assert_eq!(chart.long_notes[0].lane, Lane::Key1);
    }

    #[test]
    fn scratch_uses_the_gap_and_becomes_bgm_without_a_free_lane() {
        let mut chart = chart();
        // Measure 1 has a free gap for the scratch beside a full 7-key chord.
        for (index, lane) in SOURCE_KEYS.into_iter().enumerate() {
            push(&mut chart, note(10 + index as u32, lane, NoteKind::Tap, MEASURE_US));
        }
        push(&mut chart, note(1, Lane::Scratch, NoteKind::Tap, MEASURE_US));
        // In measure 2 the previous scratch sits in the gap 100ms before the
        // chord, so the jack guard leaves no lane for the second scratch.
        push(&mut chart, note(2, Lane::Scratch, NoteKind::Tap, 2 * MEASURE_US));
        let chord_us = 2 * MEASURE_US + 100_000;
        for (index, lane) in SOURCE_KEYS.into_iter().enumerate() {
            push(&mut chart, note(20 + index as u32, lane, NoteKind::Tap, chord_us));
        }
        let mut dropped = note(3, Lane::Scratch, NoteKind::Tap, chord_us);
        dropped.sound = Some(SoundId(3));
        push(&mut chart, dropped);

        apply_seven_to_eight(&mut chart, 7, false, DEFAULT_H_RANDOM_THRESHOLD_MS);

        assert_eq!(lane_of(&chart, 1), Lane::Key4);
        assert_eq!(lane_of(&chart, 2), Lane::Key4);
        assert!(!chart.lane_notes.iter().flatten().any(|note| note.id == NoteId(3)));
        assert_eq!(
            chart.bgm_events.iter().map(|event| event.sound).collect::<Vec<_>>(),
            vec![SoundId(3)]
        );
        assert_eq!(chart.total_notes, 16);
    }

    #[test]
    fn scratch_long_note_keeps_both_ends_on_one_lane() {
        let mut chart = chart();
        push(&mut chart, note(10, Lane::Key4, NoteKind::Tap, MEASURE_US + 200_000));
        push_long(&mut chart, 1, Lane::Scratch, MEASURE_US, MEASURE_US + 500_000);
        apply_seven_to_eight(&mut chart, 3, false, DEFAULT_H_RANDOM_THRESHOLD_MS);

        let pair = &chart.long_notes[0];
        // Gap 4 is the first choice, and Key4 moved to lane 5.
        assert_eq!(pair.lane, Lane::Key4);
        assert_eq!(lane_of(&chart, 1), pair.lane);
        assert_eq!(lane_of(&chart, 2), pair.lane);
        assert_eq!(lane_of(&chart, 10), Lane::Key5);
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
        // 16 keyboard taps plus the placed scratch.
        assert_eq!(chart.total_notes, 17);

        let mut not_seven = self::chart();
        not_seven.metadata.key_mode = KeyMode::K5;
        assert!(!apply_seven_to_eight(&mut not_seven, 9, false, DEFAULT_H_RANDOM_THRESHOLD_MS));
    }
}
