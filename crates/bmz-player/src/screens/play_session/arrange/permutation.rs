use super::*;

pub(super) fn apply_arrange_side(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: Option<i64>,
    side: ArrangeSide,
    legacy_seed: bool,
    s_random_scheme: SRandomScheme,
    h_random_threshold_ms: Option<u32>,
) -> Option<Vec<usize>> {
    if arrange == ArrangeOption::Normal {
        return None;
    }

    let include_scratch = matches!(
        arrange,
        ArrangeOption::AllScratch | ArrangeOption::RandomEx | ArrangeOption::SRandomEx
    );
    let groups = arrange_lane_groups_for_side(chart.metadata.key_mode, include_scratch, side);
    if groups.is_empty() {
        return None;
    }

    match arrange {
        ArrangeOption::Normal => None,
        ArrangeOption::Mirror => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            for group in groups {
                reverse_lane_group(&mut perm, &group);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::Random => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = ArrangeRng::new(seed.unwrap_or_else(generate_arrange_seed), legacy_seed);
            for group in groups {
                shuffle_lane_group(&mut rng, &group, &mut perm, legacy_seed);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::RRandom => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = ArrangeRng::new(seed.unwrap_or_else(generate_arrange_seed), legacy_seed);
            for group in groups {
                rotate_lane_group(&mut rng, &group, &mut perm);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::RandomEx => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = ArrangeRng::new(seed.unwrap_or_else(generate_arrange_seed), legacy_seed);
            for group in groups {
                shuffle_lane_group(&mut rng, &group, &mut perm, legacy_seed);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::FRandom | ArrangeOption::MFRandom => {
            let perm = f_random_lane_permutation_for_side(
                seed.unwrap_or_else(generate_arrange_seed),
                chart.metadata.key_mode,
                arrange,
                side,
                legacy_seed,
            );
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::SRandom
        | ArrangeOption::Spiral
        | ArrangeOption::HRandom
        | ArrangeOption::AllScratch
        | ArrangeOption::SRandomEx => {
            apply_note_arrange_for_groups(
                chart,
                arrange,
                seed.unwrap_or_else(generate_arrange_seed),
                &groups,
                legacy_seed,
                s_random_scheme,
                h_random_threshold_ms,
            );
            None
        }
    }
}

/// The lane permutation one side gets from a fixed arrangement, as during launch.
/// `None` for per-note arrangements (S-RANDOM etc.) or a seeded one without a seed.
pub(super) fn fixed_side_permutation(
    key_mode: KeyMode,
    side: ArrangeSide,
    arrange: ArrangeOption,
    seed: Option<i64>,
    legacy_seed: bool,
) -> Option<Vec<usize>> {
    let arrange = if arrange_requires_scratch(arrange) && !key_mode_has_scratch(key_mode) {
        ArrangeOption::Normal
    } else {
        arrange
    };
    let groups = arrange_lane_groups_for_side(key_mode, arrange == ArrangeOption::RandomEx, side);
    Some(match arrange {
        ArrangeOption::Normal => (0..LANE_COUNT).collect(),
        ArrangeOption::Mirror => mirror_lane_permutation_for_side(key_mode, side),
        ArrangeOption::Random | ArrangeOption::RandomEx => {
            shuffle_lane_groups(seed?, groups, legacy_seed)
        }
        ArrangeOption::RRandom => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = ArrangeRng::new(seed?, legacy_seed);
            for group in groups {
                rotate_lane_group(&mut rng, &group, &mut perm);
            }
            perm
        }
        ArrangeOption::FRandom | ArrangeOption::MFRandom => {
            f_random_lane_permutation_for_side(seed?, key_mode, arrange, side, legacy_seed)
        }
        _ => return None,
    })
}

pub(super) fn merge_lane_permutation(target: &mut [usize], source: &[usize]) {
    for (index, &source_lane) in source.iter().enumerate() {
        if source_lane != index {
            target[index] = source_lane;
        }
    }
}

pub(super) fn mirror_permutation(key_mode: KeyMode) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for group in arrange_lane_groups(key_mode, false) {
        reverse_lane_group(&mut perm, &group);
    }
    perm
}

pub(super) fn random_lane_permutation(
    seed: i64,
    key_mode: KeyMode,
    include_scratch: bool,
    legacy_seed: bool,
) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = ArrangeRng::new(seed, legacy_seed);
    for group in arrange_lane_groups(key_mode, include_scratch) {
        shuffle_lane_group(&mut rng, &group, &mut perm, legacy_seed);
    }
    perm
}

pub(super) fn f_random_lane_permutation(
    seed: i64,
    key_mode: KeyMode,
    arrange: ArrangeOption,
    legacy_seed: bool,
) -> Vec<usize> {
    let f_random = shuffle_lane_groups(seed, f_random_lane_groups(key_mode), legacy_seed);
    if arrange == ArrangeOption::MFRandom {
        compose_lane_permutations(&f_random, &mirror_permutation(key_mode))
    } else {
        f_random
    }
}

pub(super) fn f_random_lane_permutation_for_side(
    seed: i64,
    key_mode: KeyMode,
    arrange: ArrangeOption,
    side: ArrangeSide,
    legacy_seed: bool,
) -> Vec<usize> {
    let f_random =
        shuffle_lane_groups(seed, f_random_lane_groups_for_side(key_mode, side), legacy_seed);
    if arrange == ArrangeOption::MFRandom {
        let mirror = mirror_lane_permutation_for_side(key_mode, side);
        compose_lane_permutations(&f_random, &mirror)
    } else {
        f_random
    }
}

pub(super) fn shuffle_lane_groups(
    seed: i64,
    groups: Vec<Vec<usize>>,
    legacy_seed: bool,
) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = ArrangeRng::new(seed, legacy_seed);
    for group in groups {
        shuffle_lane_group(&mut rng, &group, &mut perm, legacy_seed);
    }
    perm
}

pub(super) fn mirror_lane_permutation_for_side(key_mode: KeyMode, side: ArrangeSide) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for group in arrange_lane_groups_for_side(key_mode, false, side) {
        reverse_lane_group(&mut perm, &group);
    }
    perm
}

pub(super) fn compose_lane_permutations(first: &[usize], second: &[usize]) -> Vec<usize> {
    second.iter().map(|&source| first[source]).collect()
}

pub(super) fn rotate_lane_permutation(
    seed: i64,
    key_mode: KeyMode,
    include_scratch: bool,
    legacy_seed: bool,
) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = ArrangeRng::new(seed, legacy_seed);
    for group in arrange_lane_groups(key_mode, include_scratch) {
        rotate_lane_group(&mut rng, &group, &mut perm);
    }
    perm
}

pub(super) fn rotate_lane_group(rng: &mut ArrangeRng, group: &[usize], perm: &mut [usize]) {
    if group.len() < 2 {
        return;
    }
    let inc = rng.next_usize(2) == 1;
    let mut index = rng.next_usize(group.len() - 1);
    if inc {
        index += 1;
    }
    for &lane in group {
        perm[lane] = group[index];
        index =
            if inc { (index + 1) % group.len() } else { (index + group.len() - 1) % group.len() };
    }
}

pub(super) fn arrange_lane_groups_for_side(
    key_mode: KeyMode,
    include_scratch: bool,
    side: ArrangeSide,
) -> Vec<Vec<usize>> {
    let groups = arrange_lane_groups(key_mode, include_scratch);
    match (key_mode, side) {
        (KeyMode::K10 | KeyMode::K14, ArrangeSide::P1) => groups.into_iter().take(1).collect(),
        (KeyMode::K10 | KeyMode::K14, ArrangeSide::P2) => {
            groups.into_iter().skip(1).take(1).collect()
        }
        (_, ArrangeSide::P1) => groups,
        (_, ArrangeSide::P2) => Vec::new(),
    }
}

pub(super) fn f_random_lane_groups(key_mode: KeyMode) -> Vec<Vec<usize>> {
    arrange_lane_groups(key_mode, false).into_iter().flat_map(split_f_random_group).collect()
}

pub(super) fn f_random_lane_groups_for_side(
    key_mode: KeyMode,
    side: ArrangeSide,
) -> Vec<Vec<usize>> {
    arrange_lane_groups_for_side(key_mode, false, side)
        .into_iter()
        .flat_map(split_f_random_group)
        .collect()
}

pub(super) fn split_f_random_group(group: Vec<usize>) -> Vec<Vec<usize>> {
    let len = group.len();
    if len < 2 {
        return Vec::new();
    }

    let mid = len / 2;
    let mut groups = Vec::with_capacity(2);
    let left = group[..mid].to_vec();
    if left.len() >= 2 {
        groups.push(left);
    }
    let right_start = if len.is_multiple_of(2) { mid } else { mid + 1 };
    let right = group[right_start..].to_vec();
    if right.len() >= 2 {
        groups.push(right);
    }
    groups
}

pub(super) fn arrange_lane_groups(key_mode: KeyMode, include_scratch: bool) -> Vec<Vec<usize>> {
    let active = key_mode.active_lanes();
    match key_mode {
        KeyMode::K4 | KeyMode::K6 | KeyMode::K9 => {
            vec![active.iter().map(|&lane| lane as usize).collect()]
        }
        KeyMode::K5 | KeyMode::K7 | KeyMode::K8 => {
            vec![
                active
                    .iter()
                    .filter(|&&lane| include_scratch || lane != Lane::Scratch)
                    .map(|&lane| lane as usize)
                    .collect(),
            ]
        }
        KeyMode::K10 | KeyMode::K14 => {
            let p1 = active
                .iter()
                .filter(|&&lane| {
                    matches!(
                        lane,
                        Lane::Scratch
                            | Lane::Key1
                            | Lane::Key2
                            | Lane::Key3
                            | Lane::Key4
                            | Lane::Key5
                            | Lane::Key6
                            | Lane::Key7
                    ) && (include_scratch || lane != Lane::Scratch)
                })
                .map(|&lane| lane as usize)
                .collect();
            let p2 = active
                .iter()
                .filter(|&&lane| {
                    matches!(
                        lane,
                        Lane::Key8
                            | Lane::Key9
                            | Lane::Key10
                            | Lane::Key11
                            | Lane::Key12
                            | Lane::Key13
                            | Lane::Key14
                            | Lane::Scratch2
                    ) && (include_scratch || lane != Lane::Scratch2)
                })
                .map(|&lane| lane as usize)
                .collect();
            vec![p1, p2]
        }
    }
}

pub(super) fn reverse_lane_group(perm: &mut [usize], lanes: &[usize]) {
    if lanes.len() < 2 {
        return;
    }
    let reversed: Vec<usize> = lanes.iter().rev().copied().collect();
    for (orig, rev) in lanes.iter().zip(reversed.iter()) {
        perm[*orig] = *rev;
    }
}

pub(super) fn apply_lane_permutation(chart: &mut PlayableChart, perm: &[usize]) {
    let mut old_notes: Vec<Option<Vec<NoteEvent>>> =
        (0..LANE_COUNT).map(|i| Some(std::mem::take(&mut chart.lane_notes[i]))).collect();
    for (new_idx, &old_idx) in perm.iter().enumerate() {
        let new_lane = Lane::ALL[new_idx];
        let notes = old_notes[old_idx].take().unwrap_or_default();
        chart.lane_notes[new_idx] = notes
            .into_iter()
            .map(|mut n| {
                n.lane = new_lane;
                n
            })
            .collect();
    }

    let mut reverse = [0usize; LANE_COUNT];
    for (new_idx, &old_idx) in perm.iter().enumerate() {
        reverse[old_idx] = new_idx;
    }
    for ln in &mut chart.long_notes {
        ln.lane = Lane::ALL[reverse[ln.lane as usize]];
    }
    for zone in &mut chart.metadata.free_zones {
        zone.lane = Lane::ALL[reverse[zone.lane.index()]];
    }
}

pub(super) fn apply_note_arrange(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: i64,
    legacy_seed: bool,
    s_random_scheme: SRandomScheme,
    h_random_threshold_ms: Option<u32>,
) {
    let include_scratch = matches!(arrange, ArrangeOption::AllScratch | ArrangeOption::SRandomEx);
    let groups = arrange_lane_groups(chart.metadata.key_mode, include_scratch);
    apply_note_arrange_for_groups(
        chart,
        arrange,
        seed,
        &groups,
        legacy_seed,
        s_random_scheme,
        h_random_threshold_ms,
    );
}

pub(super) fn apply_note_arrange_for_groups(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: i64,
    groups: &[Vec<usize>],
    legacy_seed: bool,
    s_random_scheme: SRandomScheme,
    h_random_threshold_ms: Option<u32>,
) {
    let mut engine = NoteArrangeEngine::new(
        arrange,
        seed,
        groups,
        legacy_seed,
        s_random_scheme,
        h_random_threshold_ms,
    );
    let mut notes: Vec<NoteEvent> = chart.lane_notes.iter_mut().flat_map(std::mem::take).collect();
    // ED's note shuffle moves inner notes but leaves the zone itself on its lane.
    // The virtual scoring endpoint must not consume RNG or become a key note.
    let mut zone_ends = Vec::new();
    notes.retain(|note| {
        if note.kind == NoteKind::FreeZoneEnd {
            zone_ends.push(note.clone());
            false
        } else {
            true
        }
    });
    notes.sort_by_key(|note| (note.tick, note.time, note.lane as u8, note.id));

    let mut start_to_end = std::collections::HashMap::new();
    let mut end_to_start = std::collections::HashMap::new();
    for ln in &chart.long_notes {
        start_to_end.insert(ln.start_note_id, ln.end_note_id);
        end_to_start.insert(ln.end_note_id, ln.start_note_id);
    }

    let mut arranged = Vec::with_capacity(notes.len());
    let mut index = 0;
    while index < notes.len() {
        let tick = notes[index].tick;
        let mut end = index + 1;
        while end < notes.len() && notes[end].tick == tick {
            end += 1;
        }
        let mut group_notes = notes[index..end].to_vec();
        engine.arrange_timeline(&mut group_notes, &start_to_end, &end_to_start);
        arranged.extend(group_notes);
        index = end;
    }

    for lane_notes in &mut chart.lane_notes {
        lane_notes.clear();
    }
    let mut start_lane = std::collections::HashMap::new();
    for note in arranged.into_iter().chain(zone_ends) {
        if matches!(note.kind, NoteKind::LongStart | NoteKind::DummyLongStart) {
            start_lane.insert(note.id, note.lane);
        }
        chart.lane_notes[note.lane.index()].push(note);
    }
    for ln in &mut chart.long_notes {
        if let Some(&lane) = start_lane.get(&ln.start_note_id) {
            ln.lane = lane;
        }
    }
    for notes in &mut chart.lane_notes {
        notes.sort_by_key(|note| (note.time, note.id));
    }
}
