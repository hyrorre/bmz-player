use super::*;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum GroupKey {
    Sound(bmz_core::ids::SoundId),
    Note(NoteId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    P1,
    P2,
}

impl Side {
    fn other(self) -> Self {
        match self {
            Self::P1 => Self::P2,
            Self::P2 => Self::P1,
        }
    }
}

/// Converts 5K/7K SP charts into 10K/14K by distributing keysound groups
/// across both player sides, following LR2's SP-to-DP design.
///
/// Groups are balanced per measure while retaining their previous side when
/// possible. A measure that would otherwise occupy only one side is split by
/// note unit, and Scratch is assigned to the side with fewer nearby key notes.
/// Long-note endpoints always remain on the same side.
pub fn apply_sp_to_dp(chart: &mut PlayableChart) -> bool {
    let target_mode = match chart.metadata.key_mode {
        KeyMode::K5 => KeyMode::K10,
        KeyMode::K7 => KeyMode::K14,
        _ => return false,
    };

    let mut source_notes = Vec::new();
    for lane in Lane::ALL {
        source_notes.extend(std::mem::take(&mut chart.lane_notes[lane.index()]));
    }
    source_notes.sort_by_key(|note| (note.time, note.tick, note.id));

    let note_by_id = source_notes.iter().map(|note| (note.id, note)).collect::<HashMap<_, _>>();
    let mut group_by_note_id = HashMap::new();
    for pair in &chart.long_notes {
        let group = note_by_id
            .get(&pair.start_note_id)
            .and_then(|note| note.sound)
            .map(GroupKey::Sound)
            .unwrap_or(GroupKey::Note(pair.start_note_id));
        group_by_note_id.insert(pair.start_note_id, group);
        group_by_note_id.insert(pair.end_note_id, group);
    }
    for note in &source_notes {
        group_by_note_id
            .entry(note.id)
            .or_insert_with(|| note.sound.map(GroupKey::Sound).unwrap_or(GroupKey::Note(note.id)));
    }

    let mut groups_by_measure: BTreeMap<u32, BTreeMap<GroupKey, Vec<NoteId>>> = BTreeMap::new();
    for note in &source_notes {
        if note.lane == Lane::Scratch || note.kind == NoteKind::Invisible {
            continue;
        }
        let measure = measure_for_tick(&chart.bar_lines, note.tick);
        groups_by_measure
            .entry(measure)
            .or_default()
            .entry(group_by_note_id[&note.id])
            .or_default()
            .push(note.id);
    }

    let mut side_by_note_id = HashMap::new();
    let mut previous_group_side = HashMap::new();
    let mut tie_side = Side::P1;
    for groups in groups_by_measure.values() {
        let mut ordered = groups.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(group, notes)| (std::cmp::Reverse(notes.len()), **group));
        let mut totals = [0usize; 2];
        for (group, notes) in ordered {
            let preferred = previous_group_side.get(group).copied();
            let side = match preferred {
                Some(side) if side_total(side, totals) <= side_total(side.other(), totals) => side,
                _ if totals[0] < totals[1] => Side::P1,
                _ if totals[1] < totals[0] => Side::P2,
                _ => {
                    let side = tie_side;
                    tie_side = tie_side.other();
                    side
                }
            };
            totals[side_index(side)] += notes.len();
            previous_group_side.insert(*group, side);
            for note_id in notes.iter().copied() {
                side_by_note_id.insert(note_id, side);
            }
        }

        if totals[0] == 0 || totals[1] == 0 {
            let mut split_side = if totals[0] == 0 { Side::P1 } else { Side::P2 };
            let mut seen_long_pairs = HashMap::new();
            let mut notes = groups.values().flatten().copied().collect::<Vec<_>>();
            notes.sort_by_key(|id| {
                let note = note_by_id[id];
                (note.time, note.tick, note.id)
            });
            for note_id in notes {
                let note = note_by_id[&note_id];
                if note.kind == NoteKind::LongEnd {
                    if let Some(&side) = seen_long_pairs.get(&group_by_note_id[&note_id]) {
                        side_by_note_id.insert(note_id, side);
                    }
                    continue;
                }
                side_by_note_id.insert(note_id, split_side);
                if note.kind == NoteKind::LongStart {
                    seen_long_pairs.insert(group_by_note_id[&note_id], split_side);
                }
                split_side = split_side.other();
            }
        }
    }

    // Keep every long-note pair on the side selected for its start.
    for pair in &chart.long_notes {
        if let Some(&side) = side_by_note_id.get(&pair.start_note_id) {
            side_by_note_id.insert(pair.end_note_id, side);
        }
    }

    // LR2 moves Scratch to the side with fewer key notes in a 200ms
    // neighborhood. Preserve one side throughout Scratch long notes.
    let key_timeline = source_notes
        .iter()
        .filter(|note| note.lane != Lane::Scratch && note.kind != NoteKind::Invisible)
        .filter_map(|note| side_by_note_id.get(&note.id).copied().map(|side| (note.time.0, side)))
        .collect::<Vec<_>>();
    let mut scratch_side = Side::P1;
    let mut active_scratch_long_note = None;
    let mut previous_scratch_time = i64::MIN / 4;
    for note in source_notes.iter().filter(|note| note.lane == Lane::Scratch) {
        let side = if note.kind == NoteKind::LongEnd {
            active_scratch_long_note.unwrap_or(scratch_side)
        } else {
            let mut nearby = [0usize; 2];
            for &(time, side) in &key_timeline {
                if time.abs_diff(note.time.0) <= 200_000 {
                    nearby[side_index(side)] += 1;
                }
            }
            if nearby[0] < nearby[1] {
                scratch_side = Side::P1;
            } else if nearby[1] < nearby[0] {
                scratch_side = Side::P2;
            } else if note.time.0.saturating_sub(previous_scratch_time) > 500_000 {
                scratch_side = scratch_side.other();
            }
            if note.kind == NoteKind::LongStart {
                active_scratch_long_note = Some(scratch_side);
            }
            scratch_side
        };
        side_by_note_id.insert(note.id, side);
        if note.kind == NoteKind::LongEnd {
            active_scratch_long_note = None;
        }
        if note.kind != NoteKind::Invisible {
            previous_scratch_time = note.time.0;
        }
    }

    let mut destination_notes: [Vec<NoteEvent>; LANE_COUNT] = std::array::from_fn(|_| Vec::new());
    for mut note in source_notes {
        let side = side_by_note_id.get(&note.id).copied().unwrap_or(Side::P1);
        let destination = if side == Side::P2 { second_player_lane(note.lane) } else { note.lane };
        note.lane = destination;
        destination_notes[destination.index()].push(note);
    }
    for pair in &mut chart.long_notes {
        if side_by_note_id.get(&pair.start_note_id) == Some(&Side::P2) {
            pair.lane = second_player_lane(pair.lane);
        }
    }
    for notes in &mut destination_notes {
        notes.sort_by_key(|note| (note.time, note.tick, note.id));
    }
    chart.lane_notes = destination_notes;
    chart.metadata.key_mode = target_mode;
    chart.total_notes = chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart))
        .count() as u32;
    tracing::info!(
        mode = target_mode.as_str(),
        total_notes = chart.total_notes,
        "converted SP chart to DP"
    );
    true
}

fn side_index(side: Side) -> usize {
    usize::from(side == Side::P2)
}

fn side_total(side: Side, totals: [usize; 2]) -> usize {
    totals[side_index(side)]
}

fn measure_for_tick(
    bar_lines: &[bmz_chart::model::BarLine],
    tick: bmz_core::time::ChartTick,
) -> u32 {
    let index = bar_lines.partition_point(|line| line.tick <= tick);
    index.checked_sub(1).map_or(0, |index| bar_lines[index].measure)
}

fn second_player_lane(lane: Lane) -> Lane {
    match lane {
        Lane::Scratch => Lane::Scratch2,
        Lane::Key1 => Lane::Key8,
        Lane::Key2 => Lane::Key9,
        Lane::Key3 => Lane::Key10,
        Lane::Key4 => Lane::Key11,
        Lane::Key5 => Lane::Key12,
        Lane::Key6 => Lane::Key13,
        Lane::Key7 => Lane::Key14,
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, SoundAssetRef};
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::time::ChartTick;

    fn chart(mode: KeyMode) -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"sp-to-dp"),
            metadata: ChartMetadata { key_mode: mode, ..Default::default() },
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

    fn note(id: u32, lane: Lane, time_us: i64, sound: u32) -> NoteEvent {
        NoteEvent {
            id: NoteId(id),
            lane,
            kind: NoteKind::Tap,
            tick: ChartTick(time_us as u64),
            time: TimeUs(time_us),
            sound: Some(SoundId(sound)),
            layered_sounds: Vec::new(),
            damage: None,
        }
    }

    #[test]
    fn maps_five_and_seven_key_sources_to_dp_modes() {
        for (source, target, last_2p) in
            [(KeyMode::K5, KeyMode::K10, Lane::Key12), (KeyMode::K7, KeyMode::K14, Lane::Key14)]
        {
            let mut chart = chart(source);
            let source_keys = match source {
                KeyMode::K5 => &KeyMode::K5.active_lanes()[1..],
                KeyMode::K7 => &KeyMode::K7.active_lanes()[1..],
                _ => unreachable!(),
            };
            for (index, lane) in source_keys.iter().copied().enumerate() {
                chart.lane_notes[lane.index()].push(note(
                    index as u32 + 1,
                    lane,
                    index as i64 * 1_000,
                    index as u32 + 1,
                ));
            }

            assert!(apply_sp_to_dp(&mut chart));
            assert_eq!(chart.metadata.key_mode, target);
            assert!(chart.lane_notes.iter().take(8).flatten().next().is_some());
            assert!(
                chart.lane_notes[Lane::Key8.index()..=last_2p.index()]
                    .iter()
                    .flatten()
                    .next()
                    .is_some()
            );
        }
    }

    #[test]
    fn same_input_produces_the_same_split() {
        let mut first = chart(KeyMode::K7);
        for index in 0..12 {
            let lane = KeyMode::K7.active_lanes()[1 + index % 7];
            first.lane_notes[lane.index()].push(note(
                index as u32 + 1,
                lane,
                index as i64 * 100_000,
                (index % 3) as u32,
            ));
        }
        let mut second = first.clone();

        apply_sp_to_dp(&mut first);
        apply_sp_to_dp(&mut second);

        let lanes = |chart: &PlayableChart| {
            chart.lane_notes.iter().flatten().map(|note| (note.id, note.lane)).collect::<Vec<_>>()
        };
        assert_eq!(lanes(&first), lanes(&second));
    }
}
