use super::{BlockIndex, NoteEvent, NoteKind, PlayableChart, TimeUs};
use crate::screens::play_session::seven_to_six::jack_relevant;

// Source indices are Scratch, Key1..Key7; values are destination key numbers.
pub(super) type Layout = [usize; 8];
const INITIAL: Layout = [8, 1, 2, 3, 4, 5, 6, 7];
const PHRASE_GAP_US: u64 = 250_000;

pub(super) fn layout_at(layouts: &[(TimeUs, Layout)], time: TimeUs) -> Layout {
    layouts
        .partition_point(|(start, _)| *start <= time)
        .checked_sub(1)
        .map_or(INITIAL, |i| layouts[i].1)
}

#[derive(Clone, Copy)]
struct Onset {
    time: TimeUs,
    keys: u16,
}

// A shift must retain all bits, including both outside notes of a chord.
fn stair_step(a: u16, b: u16) -> i8 {
    if a << 1 == b {
        1
    } else if b << 1 == a {
        -1
    } else {
        0
    }
}

fn connected(a: Onset, b: Onset) -> bool {
    b.time.0.abs_diff(a.time.0) <= PHRASE_GAP_US
}

fn phrase_boundaries(rows: &[Onset]) -> Vec<(TimeUs, bool)> {
    // Mark both transitions of each motif so a note shared by two runs keeps its finger.
    let mut linked = vec![false; rows.len()];
    for (i, pair) in rows.windows(2).enumerate() {
        linked[i] = pair[0].keys == pair[1].keys && connected(pair[0], pair[1]);
    }
    for (i, triple) in rows.windows(3).enumerate() {
        let (a, b, c) = (triple[0], triple[1], triple[2]);
        let step = stair_step(a.keys, b.keys);
        if connected(a, b)
            && connected(b, c)
            && (a.keys == c.keys || (step != 0 && step == stair_step(b.keys, c.keys)))
        {
            linked[i..i + 2].fill(true);
        }
    }
    let mut result = Vec::new();
    let mut previous: &[Onset] = &[];
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        if linked[start] {
            while linked[end - 1] {
                end += 1;
            }
        } else {
            // Group up to four ordinary onsets, stopping before the next motif.
            while end < rows.len()
                && end - start < 4
                && !linked[end]
                && connected(rows[end - 1], rows[end])
            {
                end += 1;
            }
        }
        let phrase = &rows[start..end];
        let repeated = phrase.len() >= 2
            && previous.len() == phrase.len()
            && rows[start].time.0.abs_diff(previous[previous.len() - 1].time.0)
                <= PHRASE_GAP_US * 2
            && phrase.iter().zip(previous).all(|(a, b)| a.keys == b.keys);
        result.push((rows[start].time, repeated));
        previous = phrase;
        start = end;
    }
    result
}

pub(super) fn plan(chart: &PlayableChart, guard: u64) -> Vec<(TimeUs, Layout)> {
    let mut events: Vec<_> = chart.lane_notes[..8].iter().flatten().collect();
    events.sort_unstable_by_key(|n| (n.time, n.tick, n.id));
    let Some(first) = events.first() else { return Vec::new() };
    let rows: Vec<_> = events
        .chunk_by(|a, b| a.time == b.time)
        .filter_map(|group| {
            let keys = group
                .iter()
                .filter(|n| {
                    n.lane.index() != 0 && matches!(n.kind, NoteKind::Tap | NoteKind::LongStart)
                })
                .fold(0, |mask, n| mask | (1 << n.lane.index()));
            (keys != 0).then_some(Onset { time: group[0].time, keys })
        })
        .collect();
    let mut boundaries = Vec::new();
    if rows.first().is_none_or(|row| first.time < row.time) {
        boundaries.push((first.time, false));
    }
    boundaries.extend(phrase_boundaries(&rows));

    let mut holds: Vec<_> = chart
        .long_notes
        .iter()
        .map(|p| (p.start_time, p.end_time, p.lane.index()))
        .chain(chart.metadata.free_zones.iter().map(|z| (z.start_time, z.end_time, z.lane.index())))
        .filter(|p| p.2 < 8)
        .collect();
    holds.sort_unstable();
    let mut holds = holds.into_iter().peekable();
    let mut held_until = [None::<TimeUs>; 8];
    let blocks = BlockIndex::new(chart);
    let mut state = Placement::default();
    let mut remaining = events.as_slice();
    let mut layouts = Vec::with_capacity(boundaries.len());
    for (i, &(start, repeated)) in boundaries.iter().enumerate() {
        while let Some((_, end, key)) = holds.next_if(|(begin, _, _)| *begin < start) {
            held_until[key] = held_until[key].max(Some(end));
        }
        let pinned = held_until.map(|end| end.is_some_and(|end| end >= start));
        let end = boundaries
            .get(i + 1)
            .map_or(remaining.len(), |(next, _)| remaining.partition_point(|n| n.time < *next));
        let (phrase, rest) = remaining.split_at(end);
        state.decay(blocks.block_of(phrase[0]));
        state.layout = state.choose(phrase, &pinned, repeated, guard);
        layouts.push((start, state.layout));
        for n in phrase {
            state.decay(blocks.block_of(n));
            let source = n.lane.index();
            let lane = state.layout[source] - 1;
            if matches!(n.kind, NoteKind::Tap | NoteKind::LongStart) {
                state.load[lane] += 1.0;
            }
            if jack_relevant(n) {
                state.last[lane] = Some((source, n.time));
            }
        }
        remaining = rest;
    }
    layouts
}

struct Placement {
    layout: Layout,
    load: [f64; 8],
    last: [Option<(usize, TimeUs)>; 8],
    block: usize,
}

impl Default for Placement {
    fn default() -> Self {
        Self { layout: INITIAL, load: [0.0; 8], last: [None; 8], block: 0 }
    }
}

impl Placement {
    fn decay(&mut self, block: usize) {
        if block > self.block {
            let factor = 0.75_f64.powf((block - self.block) as f64);
            for load in &mut self.load {
                *load *= factor;
            }
            self.block = block;
        }
    }

    fn choose(
        &self,
        notes: &[&NoteEvent],
        pinned: &[bool; 8],
        repeated: bool,
        guard: u64,
    ) -> Layout {
        let mut counts = [0u32; 8];
        let mut first = [None; 8];
        for n in notes {
            let k = n.lane.index();
            if matches!(n.kind, NoteKind::Tap | NoteKind::LongStart) {
                counts[k] += 1;
            }
            if jack_relevant(n) {
                first[k].get_or_insert(n.time);
            }
        }
        let keys: Vec<_> = (1..8).filter(|&k| counts[k] > 0).collect();
        let rank = |layout: Layout| {
            let mut jacks = 0;
            let mut changed = 0;
            let mut load = 0.0;
            let mut travel = 0;
            for (k, &lane) in layout.iter().enumerate() {
                if let (Some(time), Some((other, last))) = (first[k], self.last[lane - 1])
                    && (k != other || k == 0)
                    && time.0.abs_diff(last.0) <= guard
                {
                    jacks += 1;
                }
                if counts[k] == 0 {
                    continue;
                }
                let movement = lane.abs_diff(self.layout[k]);
                if k != 0 && movement != 0 {
                    changed += 1;
                }
                travel += movement;
                // Scale the seven source positions to the width of the eight keys.
                let home_distance =
                    if k == 0 { 0.0 } else { (6 * (lane - 1)).abs_diff(7 * (k - 1)) as f64 / 6.0 };
                load += f64::from(counts[k])
                    * (self.load[lane - 1] + home_distance + movement as f64 * 0.25);
            }
            let distortion: usize = keys
                .windows(2)
                .map(|p| {
                    let expected = (p[1] - p[0]) as isize;
                    let actual = layout[p[1]] as isize - layout[p[0]] as isize;
                    expected.abs_diff(actual)
                })
                .sum();
            (distortion, jacks, if repeated { changed } else { 0 }, load, travel)
        };
        // The preceding bijection is always a collision-free fallback for held keys.
        let mut best = self.layout;
        let mut best_rank = rank(best);
        let mut consider = |layout: Layout| {
            if (0..8).any(|k| pinned[k] && layout[k] != self.layout[k]) {
                return;
            }
            let score = rank(layout);
            if score < best_rank {
                best = layout;
                best_rank = score;
            }
        };
        // An ordered subset describes the whole keyboard phrase, including its chords.
        for mask in 0u16..256 {
            if mask.count_ones() as usize != keys.len() {
                continue;
            }
            let mut layout = self.layout;
            let mut targets = mask;
            for &key in &keys {
                let lane = targets.trailing_zeros() as usize + 1;
                targets &= targets - 1;
                let other = layout.iter().position(|&l| l == lane).unwrap();
                layout.swap(key, other);
            }
            consider(layout);
            if counts[0] > 0 && !pinned[0] {
                for other in 1..8 {
                    if mask & (1 << (layout[other] - 1)) == 0 {
                        let mut with_scratch = layout;
                        with_scratch.swap(0, other);
                        consider(with_scratch);
                    }
                }
            }
        }
        best
    }
}
