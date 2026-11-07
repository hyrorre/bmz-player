//! Deterministic, sound-aware BGM placement on the final player arrangement.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bmz_chart::model::{NoteEvent, NoteKind, PlayableChart, TimingEventKind};
use bmz_core::ids::SoundId;
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

const SECOND: i64 = 1_000_000;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum SoundKey {
    Asset(u32, u64, Option<u64>),
    Undefined(u32),
}

impl SoundKey {
    fn hash(&self) -> u64 {
        // Explicit FNV-1a rather than DefaultHasher: independent of process/platform/RNG.
        let bytes = match self {
            Self::Asset(id, start, duration) => format!("extra-v1:asset:{id}:{start}:{duration:?}"),
            Self::Undefined(id) => format!("extra-v1:undefined:{id}"),
        };
        bytes.bytes().fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
    }
}

fn sound_keys(chart: &PlayableChart) -> BTreeMap<u32, SoundKey> {
    let assets: Vec<_> = chart
        .sounds
        .iter()
        .map(|sound| {
            // Lexical only; archived assets need no filesystem access. Keep case significant.
            let path = sound.path.to_string_lossy().replace('\\', "/");
            let mut parts: Vec<&str> = Vec::new();
            for part in path.split('/') {
                match part {
                    "." => {}
                    ".." if parts.last().is_some_and(|last| *last != ".." && !last.is_empty()) => {
                        parts.pop();
                    }
                    _ => parts.push(part),
                }
            }
            let slice = sound.slice;
            (
                sound.id.0,
                (
                    parts.join("/"),
                    slice.map_or(0, |s| s.start_us),
                    slice.and_then(|s| s.duration_us),
                ),
            )
        })
        .collect();
    let mut representatives = BTreeMap::<_, u32>::new();
    for (id, identity) in &assets {
        representatives
            .entry(identity.clone())
            .and_modify(|old| *old = (*old).min(*id))
            .or_insert(*id);
    }
    // Hash the representative ID, never an installation-specific absolute path.
    assets
        .into_iter()
        .map(|(id, identity)| {
            (id, SoundKey::Asset(representatives[&identity], identity.1, identity.2))
        })
        .collect()
}

struct Tempo {
    initial: f64,
    changes: Vec<(TimeUs, f64)>,
}

impl Tempo {
    fn new(chart: &PlayableChart) -> Self {
        let valid = |bpm: f64| bpm.is_finite() && bpm.abs() > 0.0;
        let initial = if valid(chart.metadata.initial_bpm) {
            chart.metadata.initial_bpm.abs()
        } else {
            120.0
        };
        let mut changes: Vec<_> = chart
            .timing_events
            .iter()
            .filter_map(|event| match event.kind {
                TimingEventKind::BpmChange { bpm } if valid(bpm) => Some((event.time, bpm.abs())),
                _ => None,
            })
            .collect();
        changes.sort_by_key(|&(time, _)| time);
        Self { initial, changes }
    }

    fn gap(&self, time: TimeUs, lane: Lane) -> i64 {
        let index = self.changes.partition_point(|&(at, _)| at <= time);
        let bpm = index.checked_sub(1).map_or(self.initial, |i| self.changes[i].1);
        let minimum = if is_scratch(lane) { 250_000 } else { 125_000 };
        // Round up so fractional microseconds cannot weaken the interval guarantee.
        ((30_000_000.0 / bpm).ceil() as i64).max(minimum)
    }
}

fn is_scratch(lane: Lane) -> bool {
    matches!(lane, Lane::Scratch | Lane::Scratch2)
}

fn scored(note: &NoteEvent) -> bool {
    matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
}

struct LaneIndex {
    obstacles: Vec<(TimeUs, i64)>,
    exclusions: Vec<(i64, i64)>,
    onsets: Vec<TimeUs>,
    holds: Vec<(TimeUs, TimeUs)>,
    added: Vec<(TimeUs, i64)>,
}

impl LaneIndex {
    fn new(chart: &PlayableChart, lane: Lane, tempo: &Tempo) -> Self {
        let notes = &chart.lane_notes[lane.index()];
        let mut obstacles: Vec<_> =
            notes.iter().map(|n| (n.time, tempo.gap(n.time, lane))).collect();
        let mut onsets: Vec<_> = notes.iter().filter(|n| scored(n)).map(|n| n.time).collect();
        let mut holds: Vec<_> = chart
            .long_notes
            .iter()
            .filter(|p| p.lane == lane)
            .map(|p| (p.start_time, p.end_time))
            .collect();
        holds.sort_unstable();
        let mut merged: Vec<(TimeUs, TimeUs)> = Vec::new();
        for (start, end) in holds {
            obstacles.extend([(start, tempo.gap(start, lane)), (end, tempo.gap(end, lane))]);
            if let Some(last) = merged.last_mut().filter(|last| last.1 >= start) {
                last.1 = last.1.max(end);
            } else {
                merged.push((start, end));
            }
        }
        obstacles.sort_unstable();
        obstacles.dedup();
        // A distant slow-BPM note can exclude more time than its nearer fast-BPM
        // neighbour. Index the union, rather than only checking nearest notes.
        let mut ranges: Vec<_> = obstacles
            .iter()
            .map(|&(at, gap)| {
                let radius = gap.saturating_sub(1);
                (at.0.saturating_sub(radius), at.0.saturating_add(radius))
            })
            .collect();
        ranges.sort_unstable();
        let mut exclusions: Vec<(i64, i64)> = Vec::new();
        for (start, end) in ranges {
            if let Some(last) = exclusions.last_mut().filter(|last| last.1 >= start) {
                last.1 = last.1.max(end);
            } else {
                exclusions.push((start, end));
            }
        }
        onsets.sort_unstable();
        Self { obstacles, exclusions, onsets, holds: merged, added: Vec::new() }
    }

    fn holding(&self, time: TimeUs) -> bool {
        self.holds
            .partition_point(|&(start, _)| start <= time)
            .checked_sub(1)
            .is_some_and(|i| self.holds[i].1 >= time)
    }

    fn busy(&self, time: TimeUs) -> bool {
        self.holding(time)
            || self.onsets.binary_search(&time).is_ok()
            || self.added.last().is_some_and(|&(at, _)| at == time)
    }

    fn slack(&self, time: TimeUs, gap: i64) -> Option<i64> {
        if self.holding(time) {
            return None;
        }
        if self
            .exclusions
            .partition_point(|&(start, _)| start <= time.0)
            .checked_sub(1)
            .is_some_and(|i| self.exclusions[i].1 >= time.0)
        {
            return None;
        }
        let i = self.obstacles.partition_point(|&(at, _)| at < time);
        let mut slack = i64::MAX;
        for (at, other_gap) in i
            .checked_sub(1)
            .and_then(|j| self.obstacles.get(j))
            .into_iter()
            .chain(self.obstacles.get(i))
            .chain(self.added.last())
        {
            let distance = time.0.abs_diff(at.0).min(i64::MAX as u64) as i64;
            if distance < gap.max(*other_gap) {
                return None;
            }
            slack = slack.min(distance);
        }
        Some(slack)
    }
}

/// Move each accepted BGM event exactly once; rejected events retain their time/sound.
/// `primary_only` excludes presentation-only opponent lanes in SP battle scenes.
pub(super) fn add_extra_notes(
    chart: &mut PlayableChart,
    depth: u8,
    scratch: bool,
    primary_only: bool,
) -> bool {
    if depth == 0 || chart.bgm_events.is_empty() {
        return false;
    }
    let Some(first_id) = chart
        .lane_notes
        .iter()
        .flatten()
        .map(|note| note.id.0)
        .chain(chart.long_notes.iter().flat_map(|pair| [pair.start_note_id.0, pair.end_note_id.0]))
        .max()
        .unwrap_or(0)
        .checked_add(1)
    else {
        return false;
    };
    let lanes: Vec<_> = chart
        .metadata
        .key_mode
        .active_lanes()
        .iter()
        .copied()
        .filter(|lane| !primary_only || lane.index() < 8)
        .collect();
    let eligible: Vec<_> =
        lanes.iter().copied().filter(|&lane| scratch || !is_scratch(lane)).collect();
    if eligible.is_empty() {
        return false;
    }
    let dp = !primary_only && matches!(chart.metadata.key_mode, KeyMode::K10 | KeyMode::K14);
    let side = |lane: Lane| usize::from(dp && lane.index() >= 8);
    let tempo = Tempo::new(chart);
    let mut indices: [LaneIndex; LANE_COUNT] =
        std::array::from_fn(|i| LaneIndex::new(chart, Lane::ALL[i], &tempo));
    let mut source: [Vec<TimeUs>; 2] = std::array::from_fn(|_| Vec::new());
    let mut end = None;
    for &lane in &lanes {
        source[side(lane)].extend(indices[lane.index()].onsets.iter().copied());
        for &(_, until) in &indices[lane.index()].holds {
            end = Some(end.map_or(until, |old: TimeUs| old.max(until)));
        }
    }
    for times in &mut source {
        times.sort_unstable();
    }
    let Some(start) = source.iter().filter_map(|times| times.first()).min().copied() else {
        return false;
    };
    let end = end
        .into_iter()
        .chain(source.iter().filter_map(|times| times.last().copied()))
        .max()
        .unwrap();
    let chord_cap: [usize; 2] = std::array::from_fn(|p| {
        lanes.iter().filter(|&&lane| side(lane) == p && !is_scratch(lane)).count().div_ceil(2)
    });
    let keys = sound_keys(chart);
    let key = |id: SoundId| keys.get(&id.0).cloned().unwrap_or(SoundKey::Undefined(id.0));
    let mut affinity: BTreeMap<SoundKey, [u32; LANE_COUNT]> = BTreeMap::new();
    for &lane in &lanes {
        for note in chart.lane_notes[lane.index()].iter().filter(|n| scored(n)) {
            for sound in note.sounds().map(&key).collect::<BTreeSet<_>>() {
                affinity.entry(sound).or_default()[lane.index()] += 1;
            }
        }
    }
    let mut repeats = BTreeMap::<SoundKey, u32>::new();
    let mut grouped = BTreeMap::<TimeUs, Vec<_>>::new();
    for event in std::mem::take(&mut chart.bgm_events) {
        *repeats.entry(key(event.sound)).or_default() += 1;
        grouped.entry(event.time).or_default().push(event);
    }
    let mut recent: [VecDeque<TimeUs>; 2] = std::array::from_fn(|_| VecDeque::new());
    let mut next_id = Some(first_id);
    let mut moved = false;
    for (time, mut events) in grouped {
        for times in &mut recent {
            while times.front().is_some_and(|at| at.0 <= time.0.saturating_sub(SECOND)) {
                times.pop_front();
            }
        }
        let budget: [usize; 2] = std::array::from_fn(|p| {
            let lower = source[p].partition_point(|at| at.0 < time.0.saturating_sub(SECOND));
            let upper = source[p].partition_point(|at| at.0 < time.0.saturating_add(SECOND));
            (upper - lower).div_ceil(2).min(8)
        });
        events.sort_by_key(|event| {
            let sound = key(event.sound);
            (
                Reverse(affinity.contains_key(&sound)),
                Reverse(repeats[&sound]),
                sound.hash(),
                event.sound.0,
                event.tick,
            )
        });
        let mut count = 0;
        for event in events {
            let sound = key(event.sound);
            let counts = affinity.get(&sound).copied().unwrap_or_default();
            let hash = sound.hash();
            let rotation = (hash % eligible.len() as u64) as usize;
            let primary = (0..eligible.len())
                .min_by_key(|&i| {
                    (
                        Reverse(counts[eligible[i].index()]),
                        (i + eligible.len() - rotation) % eligible.len(),
                    )
                })
                .unwrap();
            let target =
                if next_id.is_none() || time < start || time > end || count >= usize::from(depth) {
                    None
                } else {
                    eligible
                        .iter()
                        .enumerate()
                        .filter_map(|(position, &lane)| {
                            let p = side(lane);
                            if recent[p].len() >= budget[p] {
                                return None;
                            }
                            let occupied = lanes
                                .iter()
                                .filter(|&&l| side(l) == p && indices[l.index()].busy(time))
                                .count();
                            if occupied >= chord_cap[p] {
                                return None;
                            }
                            let gap = tempo.gap(time, lane);
                            let slack = indices[lane.index()].slack(time, gap)?;
                            let load = indices[lane.index()].added.partition_point(|&(at, _)| {
                                at.0 < time.0.saturating_sub(2 * SECOND)
                            });
                            Some((
                                (
                                    Reverse(counts[lane.index()] > 0),
                                    position.abs_diff(primary),
                                    Reverse(slack),
                                    indices[lane.index()].added.len() - load,
                                    (position + eligible.len() - rotation) % eligible.len(),
                                ),
                                lane,
                                gap,
                            ))
                        })
                        .min_by_key(|&(rank, _, _)| rank)
                        .map(|(_, lane, gap)| (lane, gap))
                };
            let Some((lane, gap)) = target else {
                chart.bgm_events.push(event);
                continue;
            };
            chart.lane_notes[lane.index()].push(NoteEvent {
                id: bmz_core::ids::NoteId(next_id.expect("checked above")),
                lane,
                kind: NoteKind::Tap,
                tick: event.tick,
                time,
                sound: Some(event.sound),
                layered_sounds: Vec::new(),
                damage: None,
            });
            indices[lane.index()].added.push((time, gap));
            recent[side(lane)].push_back(time);
            next_id = next_id.and_then(|id| id.checked_add(1));
            count += 1;
            moved = true;
        }
    }
    chart.bgm_events.sort_by_key(|event| (event.time, event.tick));
    if moved {
        for notes in &mut chart.lane_notes {
            notes.sort_by_key(|note| (note.time, note.id));
        }
    }
    moved
}

#[cfg(test)]
mod tests;
