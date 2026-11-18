use super::seven_to_six::{MovableUnit, can_place, push_note_sounds_as_bgm};
use super::*;
use bmz_core::time::ChartTick;
use std::collections::{HashMap, HashSet};

const RNG_DOMAIN: i64 = 0x0037_4b38;
const MEASURES_PER_BLOCK: usize = 4;
const FALLBACK_BLOCK_US: i64 = 8_000_000;
const HISTORY_DECAY: f64 = 0.75;
const GAP_DISTANCE_COST: f64 = 0.25;
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
const SIMULTANEOUS_ADJACENCY_WEIGHT: f64 = 0.15;
/// Last lane number of the left hand (lanes 1-4); lanes 5-8 are the right hand.
const LEFT_HAND_LAST_LANE: usize = 4;

/// Converts a source 7K chart into eight scratch-less lanes (experimental).
///
/// The seven keys keep their order around one gap per four-measure block.
/// Decaying destination-lane press counts guide the next gap. Long notes and
/// fast trills or stairs can carry a layout across the boundary. Scratch
/// notes then move to a free lane while
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
    let gaps = select_gaps(chart, &blocks, jack_guard_us);

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

#[derive(Clone, Default)]
struct GapBlock {
    counts: [u32; 7],
    // Include LN releases in the boundary jack guard, but only count presses as load.
    time_ranges: [Option<(i64, i64)>; 7],
    chords: Vec<Vec<usize>>,
    locked: bool,
}

/// Chooses the gap lane (1..=8) of every block in order without randomness.
fn select_gaps(chart: &PlayableChart, blocks: &BlockIndex, jack_guard_us: u64) -> Vec<usize> {
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
    let mut stats = vec![GapBlock::default(); block_count];
    let mut presses = Vec::new();
    for (index, lane) in SOURCE_KEYS.iter().enumerate() {
        for note in &chart.lane_notes[lane.index()] {
            let block = blocks.block_of(note);
            let time = note.time.0;
            if matches!(note.kind, NoteKind::Tap | NoteKind::LongStart) {
                stats[block].counts[index] += 1;
                presses.push((block, time, index + 1));
            }
            if super::seven_to_six::jack_relevant(note) {
                let range = stats[block].time_ranges[index].get_or_insert((time, time));
                range.0 = range.0.min(time);
                range.1 = range.1.max(time);
            }
        }
    }
    collect_keyboard_patterns(&mut stats, presses, jack_guard_us);
    for pair in &chart.long_notes {
        if pair.lane == Lane::Scratch || source_key_number(pair.lane).is_some() {
            let start = blocks.block_at(pair.start_tick, pair.start_time);
            let end = blocks.block_at(pair.end_tick, pair.end_time);
            for block in stats.iter_mut().take(end + 1).skip(start + 1) {
                block.locked = true;
            }
        }
    }

    let mut gaps = Vec::with_capacity(block_count);
    let mut history = [0.0; 8];
    let mut last_by_lane = [None; 8];
    let mut previous = None;
    for block in stats {
        for load in &mut history {
            *load *= HISTORY_DECAY;
        }
        let total = block.counts.iter().sum::<u32>();
        if total > 0 && (previous.is_none() || !block.locked) {
            previous =
                Some(block.best_gap(&history, &last_by_lane, jack_guard_us, total, previous));
        }
        if let Some(gap) = previous {
            for (index, count) in block.counts.iter().enumerate() {
                let lane = embed(index + 1, gap) - 1;
                history[lane] += f64::from(*count);
                if let Some((_, last)) = block.time_ranges[index] {
                    last_by_lane[lane] = Some((index, last));
                }
            }
        }
        gaps.push(previous);
    }
    // Leading scratch-only blocks use the first keyboard layout.
    let first = gaps.iter().flatten().copied().next().unwrap_or(INITIAL_GAP);
    gaps.into_iter().map(|gap| gap.unwrap_or(first)).collect()
}

fn collect_keyboard_patterns(
    blocks: &mut [GapBlock],
    mut presses: Vec<(usize, i64, usize)>,
    jack_guard_us: u64,
) {
    presses.sort_unstable();
    presses.dedup();
    let mut preceding: [Option<(usize, i64, usize)>; 2] = [None; 2];
    for onset in presses.chunk_by(|a, b| (a.0, a.1) == (b.0, b.1)) {
        let (block, time, key) = onset[0];
        if onset.len() > 1 {
            blocks[block].chords.push(onset.iter().map(|press| press.2).collect());
            preceding = [None; 2];
            continue;
        }
        if let [Some((start, first_time, a)), Some((_, middle_time, b))] = preceding {
            let trill = a == key;
            let stair = (a + 1 == b && b + 1 == key) || (a == b + 1 && b == key + 1);
            if start < block
                && (trill || stair)
                && middle_time.abs_diff(first_time) <= jack_guard_us
                && time.abs_diff(middle_time) <= jack_guard_us
            {
                for locked in &mut blocks[start + 1..=block] {
                    locked.locked = true;
                }
            }
        }
        preceding = [preceding[1], Some((block, time, key))];
    }
}

impl GapBlock {
    fn best_gap(
        &self,
        history: &[f64; 8],
        last_by_lane: &[Option<(usize, i64)>; 8],
        jack_guard_us: u64,
        total: u32,
        previous: Option<usize>,
    ) -> usize {
        let rank = |gap: usize| {
            let mut boundary_jacks = 0u32;
            let mut load = 0.0;
            for (index, count) in self.counts.iter().enumerate() {
                let lane = embed(index + 1, gap) - 1;
                // Only the cross term of (history + count)^2 depends on the gap.
                load += history[lane] * f64::from(*count);
                if let (Some((first, _)), Some((source, last))) =
                    (self.time_ranges[index], last_by_lane[lane])
                    && source != index
                    && first.abs_diff(last) <= jack_guard_us
                {
                    boundary_jacks += 1;
                }
            }
            let movement = previous.map_or(0, |p| p.abs_diff(gap));
            let score = 2.0 * load / f64::from(total)
                + SIMULTANEOUS_ADJACENCY_WEIGHT
                    * f64::from(simultaneous_adjacency(&self.chords, gap))
                + GAP_DISTANCE_COST * movement as f64;
            let distance = previous.map_or((2 * gap).abs_diff(9), |_| movement);
            (boundary_jacks, score, distance, gap)
        };
        (1..=8)
            .map(rank)
            .min_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.total_cmp(&b.1))
                    .then(a.2.cmp(&b.2))
                    .then(a.3.cmp(&b.3))
            })
            .expect("eight candidate gaps")
            .3
    }
}

/// Simultaneous adjacency term of the gap cost: the number of neighbouring
/// lane pairs inside one hand that every group of simultaneous presses reaches
/// after embedding around `gap`. Lanes 4 and 5 belong to different hands and
/// do not form a pair.
///
/// A gap inside a dense chord splits it, so chord-heavy blocks can prefer a
/// gap off the edges. Blocks of single presses add zero.
fn simultaneous_adjacency(groups: &[Vec<usize>], gap: usize) -> u32 {
    groups
        .iter()
        .map(|keys| {
            // `embed` keeps the order, so only consecutive keys can become
            // neighbouring lanes.
            keys.windows(2)
                .filter(|pair| {
                    let lower = embed(pair[0], gap);
                    embed(pair[1], gap) == lower + 1 && lower != LEFT_HAND_LAST_LANE
                })
                .count() as u32
        })
        .sum()
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
    fn four_measures_use_one_layout_then_fill_a_previously_unused_lane() {
        let mut chart = chart();
        for measure in 0..8u32 {
            for (index, lane) in SOURCE_KEYS.into_iter().enumerate() {
                let id = measure * 10 + index as u32;
                let time = i64::from(measure) * MEASURE_US + index as i64 * 50_000;
                push(&mut chart, note(id, lane, NoteKind::Tap, time));
            }
        }
        let mut padded = chart.clone();
        bmz_chart::start_margin::apply_fixed_start_margin(&mut padded, TimeUs(MEASURE_US));
        for chart in [&mut chart, &mut padded] {
            apply_seven_to_eight(chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            for key in 0..7 {
                for measure in 0..4 {
                    assert_eq!(lane_of(chart, key), lane_of(chart, measure * 10 + key));
                    assert_eq!(lane_of(chart, 40 + key), lane_of(chart, (measure + 4) * 10 + key));
                }
            }
            for lane in DESTINATION_LANES {
                assert!(!chart.lane_notes[lane.index()].is_empty());
            }
        }
        for note in chart.lane_notes.iter().flatten() {
            assert_eq!(lane_of(&padded, note.id.0), note.lane);
        }
    }

    #[test]
    fn rests_decay_load_and_missing_bar_lines_use_eight_second_windows() {
        let mut chart = chart();
        chart.bar_lines.truncate(1);
        for (id, seconds) in [(1, 1), (2, 8), (3, 9), (4, 10), (5, 200)] {
            push(&mut chart, note(id, Lane::Key4, NoteKind::Tap, seconds * MEASURE_US));
        }
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        assert_eq!(lane_of(&chart, 1), Lane::Key5);
        assert_eq!(lane_of(&chart, 2), Lane::Key5);
        assert_eq!(lane_of(&chart, 3), Lane::Key4);
        assert_eq!(lane_of(&chart, 4), Lane::Key4);
        // Lane 4 was busier in the second window. After the rest, its excess
        // load has decayed below the cost of moving back to lane 5.
        assert_eq!(lane_of(&chart, 5), Lane::Key4);
    }

    #[test]
    fn gap_avoids_a_new_boundary_jack_including_the_threshold() {
        let mut source = chart();
        for id in 0..8 {
            push(&mut source, note(id, Lane::Key5, NoteKind::Tap, i64::from(id) * 250_000));
        }
        push(&mut source, note(10, Lane::Key4, NoteKind::Tap, 4 * MEASURE_US - 125_000));
        push(&mut source, note(11, Lane::Key5, NoteKind::Tap, 4 * MEASURE_US));
        for (threshold, expected) in [(125, Lane::Key6), (124, Lane::Key5)] {
            let mut chart = source.clone();
            apply_seven_to_eight(&mut chart, 1, false, threshold);
            assert_eq!(lane_of(&chart, 10), Lane::Key5);
            assert_eq!(lane_of(&chart, 11), expected);
        }
    }

    #[test]
    fn fast_trills_and_stairs_keep_the_layout_across_a_boundary() {
        for pattern in [[2, 4, 2], [2, 3, 4], [4, 3, 2]] {
            let mut source = chart();
            for id in 0..8 {
                push(
                    &mut source,
                    note(id, Lane::Key4, NoteKind::Tap, MEASURE_US + i64::from(id) * 200_000),
                );
            }
            for (i, key) in pattern.into_iter().enumerate() {
                let time = 4 * MEASURE_US + (i as i64 - 1) * 100_000;
                push(&mut source, note(10 + i as u32, SOURCE_KEYS[key - 1], NoteKind::Tap, time));
            }
            push(&mut source, note(20, Lane::Key4, NoteKind::Tap, 5 * MEASURE_US));
            for (threshold, expected) in [(100, Lane::Key5), (99, Lane::Key4)] {
                let mut chart = source.clone();
                apply_seven_to_eight(&mut chart, 1, false, threshold);
                assert_eq!(lane_of(&chart, 20), expected, "pattern {pattern:?}");
            }
        }
    }

    #[test]
    fn keyboard_and_scratch_holds_lock_the_four_measure_boundary() {
        for lane in [Lane::Key1, Lane::Scratch] {
            let mut chart = chart();
            for (id, measure) in [(1, 1), (2, 5)] {
                push(&mut chart, note(id, Lane::Key4, NoteKind::Tap, measure * MEASURE_US));
            }
            push_long(&mut chart, 10, lane, 3 * MEASURE_US, 4 * MEASURE_US);
            apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
            assert_eq!(lane_of(&chart, 1), lane_of(&chart, 2));
            assert_eq!(lane_of(&chart, 10), lane_of(&chart, 11));
            assert_eq!(chart.long_notes[0].lane, lane_of(&chart, 10));
        }
    }

    #[test]
    fn chord_puts_the_gap_between_adjacent_keys_of_one_hand() {
        let mut chart = chart();
        for (i, lane) in SOURCE_KEYS[..4].iter().enumerate() {
            push(&mut chart, note(i as u32, *lane, NoteKind::Tap, MEASURE_US));
        }
        apply_seven_to_eight(&mut chart, 1, false, DEFAULT_H_RANDOM_THRESHOLD_MS);
        // Gaps 2 and 3 split one adjacent pair. Gap 3 is closer to the centre.
        assert_eq!(
            lanes_in(&chart, 1),
            HashSet::from([Lane::Key1, Lane::Key2, Lane::Key4, Lane::Key5])
        );
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

        // Gaps 2, 3, 6 and 7 minimize adjacency. The centre tie-break chooses 3.
        assert_eq!(lane_of(&chart, 10), Lane::Key1);
        assert_eq!(lane_of(&chart, 11), Lane::Key2);
        assert_eq!(lane_of(&chart, 1), Lane::Key3);
        assert_eq!(lane_of(&chart, 2), Lane::Key3);
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
        // With no previous load or chords, the initial gap is 4.
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
        // 48 keyboard taps plus the placed scratch.
        assert_eq!(chart.total_notes, 49);

        let mut not_seven = self::chart();
        not_seven.metadata.key_mode = KeyMode::K5;
        assert!(!apply_seven_to_eight(&mut not_seven, 9, false, DEFAULT_H_RANDOM_THRESHOLD_MS));
    }
}
