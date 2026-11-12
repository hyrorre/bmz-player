use super::*;

/// Compute only a fixed permutation; never draw a missing seed or import a chart.
pub(crate) fn preview_arrange_pattern(
    key_mode: KeyMode,
    arrangement: &crate::screens::play_start::BattleTargetArrangement,
) -> Option<Vec<u8>> {
    let is_double = matches!(key_mode, KeyMode::K10 | KeyMode::K14);
    let mut pattern = if let Some(pattern) = &arrangement.arrange_pattern {
        let mut sorted = pattern.clone();
        sorted.sort_unstable();
        if sorted != (0..LANE_COUNT as u8).collect::<Vec<_>>() {
            return None;
        }
        pattern.iter().map(|&lane| usize::from(lane)).collect()
    } else {
        let packed = arrangement
            .packed_seed
            .and_then(|seed| u64::try_from(seed).ok())
            .and_then(|seed| RandomOptionSeeds::unpack(seed, is_double));
        let seed =
            arrangement.arrange_seed.or_else(|| packed.map(|seeds| i64::from(seeds.p1.value())));
        let seed_2p = if arrangement.legacy_arrange_seed {
            seed.map(|seed| seed.wrapping_add(0x9e37_79b9))
        } else {
            arrangement
                .arrange_seed_2p
                .or_else(|| packed.and_then(|seeds| seeds.p2).map(|seed| i64::from(seed.value())))
        };
        let mut combined: Vec<usize> = (0..LANE_COUNT).collect();
        for (side, arrange, seed) in [
            (ArrangeSide::P1, arrangement.arrange, seed),
            (ArrangeSide::P2, arrangement.arrange_2p, seed_2p),
        ] {
            if matches!(side, ArrangeSide::P2) && !is_double {
                continue;
            }
            let perm = fixed_side_permutation(
                key_mode,
                side,
                arrange,
                seed,
                arrangement.legacy_arrange_seed,
            )?;
            merge_lane_permutation(&mut combined, &perm);
        }
        combined
    };
    if is_double && arrangement.double_option == DoubleOption::Flip {
        // FLIP is applied before the per-side arrangement during launch.
        for lane in &mut pattern {
            *lane = match *lane {
                0 => 15,
                15 => 0,
                1..=7 => *lane + 7,
                8..=14 => *lane - 7,
                _ => unreachable!(),
            };
        }
    } else if !matches!(arrangement.double_option, DoubleOption::Off | DoubleOption::Flip) {
        return None;
    }
    Some(pattern.into_iter().map(|lane| lane as u8).collect())
}

pub fn generate_arrange_seed() -> i64 {
    i64::from(RandomOptionSeeds::fresh(false).p1.value())
}

pub(super) fn normal_applied_arrange(
    seed: i64,
    legacy_seed: bool,
    s_random_scheme: SRandomScheme,
    h_random_threshold_ms: Option<u32>,
) -> AppliedArrange {
    AppliedArrange {
        arrange: ArrangeOption::Normal,
        arrange_2p: ArrangeOption::Normal,
        double_option: DoubleOption::Off,
        seed: Some(seed),
        seed_2p: None,
        legacy_seed,
        s_random_scheme,
        s_random_scheme_2p: None,
        h_random_threshold_ms,
        bms_random_choices: Vec::new(),
        bms_switch_choices: Vec::new(),
        pattern: None,
        key_mode_conversion: KeyModeConversionConfig::Off,
        seven_to_nine_pattern: SevenToNinePattern::default(),
        seven_to_nine_type: SevenToNineType::default(),
        seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
        freq_semitones: 0,
    }
}

pub fn apply_arrange(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: Option<i64>,
    pattern: Option<&[u8]>,
) -> AppliedArrange {
    apply_arrange_internal(chart, arrange, seed, pattern, false, SRandomScheme::Lm120HzV1, None)
}

pub(super) fn apply_arrange_internal(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: Option<i64>,
    pattern: Option<&[u8]>,
    legacy_seed: bool,
    s_random_scheme: SRandomScheme,
    h_random_threshold_ms: Option<u32>,
) -> AppliedArrange {
    let key_mode = chart.metadata.key_mode;
    let used_seed = seed.unwrap_or_else(generate_arrange_seed);
    if arrange_requires_scratch(arrange) && !key_mode_has_scratch(key_mode) {
        return normal_applied_arrange(
            used_seed,
            legacy_seed,
            s_random_scheme,
            h_random_threshold_ms,
        );
    }

    if let Some(perm) = pattern {
        let perm_usize: Vec<usize> = perm.iter().map(|&i| i as usize).collect();
        apply_lane_permutation(chart, &perm_usize);
        return AppliedArrange {
            arrange,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            seed: Some(used_seed),
            seed_2p: None,
            legacy_seed,
            s_random_scheme,
            s_random_scheme_2p: None,
            h_random_threshold_ms,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            pattern: Some(perm.to_vec()),
            key_mode_conversion: KeyModeConversionConfig::Off,
            seven_to_nine_pattern: SevenToNinePattern::default(),
            seven_to_nine_type: SevenToNineType::default(),
            seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
            freq_semitones: 0,
        };
    }

    match arrange {
        ArrangeOption::Normal => {
            normal_applied_arrange(used_seed, legacy_seed, s_random_scheme, h_random_threshold_ms)
        }
        ArrangeOption::Mirror => {
            let perm = mirror_permutation(key_mode);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::Mirror,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                seed_2p: None,
                legacy_seed,
                s_random_scheme,
                s_random_scheme_2p: None,
                h_random_threshold_ms,
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                freq_semitones: 0,
            }
        }
        ArrangeOption::Random => {
            let perm = random_lane_permutation(used_seed, key_mode, false, legacy_seed);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::Random,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                seed_2p: None,
                legacy_seed,
                s_random_scheme,
                s_random_scheme_2p: None,
                h_random_threshold_ms,
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                freq_semitones: 0,
            }
        }
        ArrangeOption::RRandom => {
            let perm = rotate_lane_permutation(used_seed, key_mode, false, legacy_seed);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::RRandom,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                seed_2p: None,
                legacy_seed,
                s_random_scheme,
                s_random_scheme_2p: None,
                h_random_threshold_ms,
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                freq_semitones: 0,
            }
        }
        ArrangeOption::RandomEx => {
            let perm = random_lane_permutation(used_seed, key_mode, true, legacy_seed);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::RandomEx,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                seed_2p: None,
                legacy_seed,
                s_random_scheme,
                s_random_scheme_2p: None,
                h_random_threshold_ms,
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                freq_semitones: 0,
            }
        }
        ArrangeOption::FRandom | ArrangeOption::MFRandom => {
            let perm = f_random_lane_permutation(used_seed, key_mode, arrange, legacy_seed);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                seed_2p: None,
                legacy_seed,
                s_random_scheme,
                s_random_scheme_2p: None,
                h_random_threshold_ms,
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                freq_semitones: 0,
            }
        }
        ArrangeOption::SRandom
        | ArrangeOption::Spiral
        | ArrangeOption::HRandom
        | ArrangeOption::AllScratch
        | ArrangeOption::SRandomEx => {
            apply_note_arrange(
                chart,
                arrange,
                used_seed,
                legacy_seed,
                s_random_scheme,
                h_random_threshold_ms,
            );
            AppliedArrange {
                arrange,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                seed_2p: None,
                legacy_seed,
                s_random_scheme,
                s_random_scheme_2p: None,
                h_random_threshold_ms,
                bms_random_choices: Vec::new(),
                bms_switch_choices: Vec::new(),
                pattern: None,
                key_mode_conversion: KeyModeConversionConfig::Off,
                seven_to_nine_pattern: SevenToNinePattern::default(),
                seven_to_nine_type: SevenToNineType::default(),
                seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
                freq_semitones: 0,
            }
        }
    }
}

pub(super) fn arrange_requires_scratch(arrange: ArrangeOption) -> bool {
    matches!(
        arrange,
        ArrangeOption::AllScratch | ArrangeOption::RandomEx | ArrangeOption::SRandomEx
    )
}

pub(super) fn key_mode_has_scratch(key_mode: KeyMode) -> bool {
    key_mode.active_lanes().iter().any(|&lane| matches!(lane, Lane::Scratch | Lane::Scratch2))
}

pub fn apply_arrange_pair(
    chart: &mut PlayableChart,
    arrange_1p: ArrangeOption,
    arrange_2p: ArrangeOption,
    seed: Option<i64>,
    seed_2p: Option<i64>,
    legacy_seed: bool,
    s_random_scheme: SRandomScheme,
    s_random_scheme_2p: Option<SRandomScheme>,
    h_random_threshold_ms: Option<u32>,
    pattern: Option<&[u8]>,
) -> AppliedArrange {
    let used_seed = seed.unwrap_or_else(generate_arrange_seed);
    let key_mode = chart.metadata.key_mode;
    if !matches!(key_mode, KeyMode::K10 | KeyMode::K14) {
        return apply_arrange_internal(
            chart,
            arrange_1p,
            Some(used_seed),
            pattern,
            legacy_seed,
            s_random_scheme,
            h_random_threshold_ms,
        );
    }

    let used_seed_2p = if legacy_seed {
        used_seed.wrapping_add(0x9e37_79b9)
    } else {
        seed_2p.unwrap_or_else(generate_arrange_seed)
    };
    let used_s_random_scheme_2p = s_random_scheme_2p.unwrap_or(s_random_scheme);
    if let Some(perm) = pattern {
        let perm_usize: Vec<usize> = perm.iter().map(|&i| i as usize).collect();
        apply_lane_permutation(chart, &perm_usize);
        return AppliedArrange {
            arrange: arrange_1p,
            arrange_2p,
            double_option: DoubleOption::Off,
            seed: Some(used_seed),
            seed_2p: Some(used_seed_2p),
            legacy_seed,
            s_random_scheme,
            s_random_scheme_2p: Some(used_s_random_scheme_2p),
            h_random_threshold_ms,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            pattern: Some(perm.to_vec()),
            key_mode_conversion: KeyModeConversionConfig::Off,
            seven_to_nine_pattern: SevenToNinePattern::default(),
            seven_to_nine_type: SevenToNineType::default(),
            seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
            freq_semitones: 0,
        };
    }

    let mut combined_perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut has_perm = false;

    if let Some(perm) = apply_arrange_side(
        chart,
        arrange_1p,
        Some(used_seed),
        ArrangeSide::P1,
        legacy_seed,
        s_random_scheme,
        h_random_threshold_ms,
    ) {
        merge_lane_permutation(&mut combined_perm, &perm);
        has_perm = true;
    }
    if let Some(perm) = apply_arrange_side(
        chart,
        arrange_2p,
        Some(used_seed_2p),
        ArrangeSide::P2,
        legacy_seed,
        used_s_random_scheme_2p,
        h_random_threshold_ms,
    ) {
        merge_lane_permutation(&mut combined_perm, &perm);
        has_perm = true;
    }

    AppliedArrange {
        arrange: arrange_1p,
        arrange_2p,
        double_option: DoubleOption::Off,
        seed: Some(used_seed),
        seed_2p: Some(used_seed_2p),
        legacy_seed,
        s_random_scheme,
        s_random_scheme_2p: Some(used_s_random_scheme_2p),
        h_random_threshold_ms,
        bms_random_choices: Vec::new(),
        bms_switch_choices: Vec::new(),
        pattern: has_perm.then(|| combined_perm.iter().map(|&i| i as u8).collect()),
        key_mode_conversion: KeyModeConversionConfig::Off,
        seven_to_nine_pattern: SevenToNinePattern::default(),
        seven_to_nine_type: SevenToNineType::default(),
        seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
        freq_semitones: 0,
    }
}

pub(super) fn apply_double_option(chart: &mut PlayableChart, double_option: DoubleOption) {
    match double_option {
        DoubleOption::Off => return,
        DoubleOption::Flip => {
            if !matches!(chart.metadata.key_mode, KeyMode::K10 | KeyMode::K14) {
                return;
            }
        }
        DoubleOption::Battle | DoubleOption::BattleAutoScratch => {
            apply_battle_double_option(chart);
            return;
        }
    }

    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for (left, right) in [
        (Lane::Scratch, Lane::Scratch2),
        (Lane::Key1, Lane::Key8),
        (Lane::Key2, Lane::Key9),
        (Lane::Key3, Lane::Key10),
        (Lane::Key4, Lane::Key11),
        (Lane::Key5, Lane::Key12),
        (Lane::Key6, Lane::Key13),
        (Lane::Key7, Lane::Key14),
    ] {
        let left = left.index();
        let right = right.index();
        perm[left] = right;
        perm[right] = left;
    }
    apply_lane_permutation(chart, &perm);
}

pub(super) fn apply_battle_double_option(chart: &mut PlayableChart) {
    let (next_mode, pairs): (KeyMode, &[(Lane, Lane)]) = match chart.metadata.key_mode {
        KeyMode::K5 => (
            KeyMode::K10,
            &[
                (Lane::Scratch, Lane::Scratch2),
                (Lane::Key1, Lane::Key8),
                (Lane::Key2, Lane::Key9),
                (Lane::Key3, Lane::Key10),
                (Lane::Key4, Lane::Key11),
                (Lane::Key5, Lane::Key12),
            ],
        ),
        KeyMode::K7 => (
            KeyMode::K14,
            &[
                (Lane::Scratch, Lane::Scratch2),
                (Lane::Key1, Lane::Key8),
                (Lane::Key2, Lane::Key9),
                (Lane::Key3, Lane::Key10),
                (Lane::Key4, Lane::Key11),
                (Lane::Key5, Lane::Key12),
                (Lane::Key6, Lane::Key13),
                (Lane::Key7, Lane::Key14),
            ],
        ),
        _ => return,
    };

    let mut next_note_id = next_note_id(chart);
    let mut cloned_ids = std::collections::HashMap::new();
    for &(source, dest) in pairs {
        let source_index = source.index();
        let dest_index = dest.index();
        let clones: Vec<NoteEvent> = chart.lane_notes[source_index]
            .iter()
            .cloned()
            .map(|mut note| {
                let new_id = next_note_id;
                next_note_id.0 = next_note_id.0.saturating_add(1);
                cloned_ids.insert(note.id, new_id);
                note.id = new_id;
                note.lane = dest;
                note
            })
            .collect();
        chart.lane_notes[dest_index].extend(clones);
    }

    let source_to_dest: std::collections::HashMap<_, _> = pairs.iter().copied().collect();
    let mut cloned_long_notes = Vec::new();
    for pair in &chart.long_notes {
        let Some(&dest) = source_to_dest.get(&pair.lane) else {
            continue;
        };
        let (Some(&start_note_id), Some(&end_note_id)) =
            (cloned_ids.get(&pair.start_note_id), cloned_ids.get(&pair.end_note_id))
        else {
            continue;
        };
        let mut cloned = pair.clone();
        cloned.lane = dest;
        cloned.start_note_id = start_note_id;
        cloned.end_note_id = end_note_id;
        cloned_long_notes.push(cloned);
    }
    chart.long_notes.extend(cloned_long_notes);
    chart.total_notes = chart.total_notes.saturating_mul(2);
    chart.metadata.key_mode = next_mode;
}

/// Build the 2P presentation lanes from an independently arranged opponent.
/// The primary chart remains the score source; the opponent chart is judged by
/// `BattleOpponentSession` and these cloned notes are display-only.
pub(super) fn apply_battle_opponent_chart(chart: &mut PlayableChart, opponent: &PlayableChart) {
    let (next_mode, pairs): (KeyMode, &[(Lane, Lane)]) = match chart.metadata.key_mode {
        KeyMode::K5 if opponent.metadata.key_mode == KeyMode::K5 => (
            KeyMode::K10,
            &[
                (Lane::Scratch, Lane::Scratch2),
                (Lane::Key1, Lane::Key8),
                (Lane::Key2, Lane::Key9),
                (Lane::Key3, Lane::Key10),
                (Lane::Key4, Lane::Key11),
                (Lane::Key5, Lane::Key12),
            ],
        ),
        KeyMode::K7 if opponent.metadata.key_mode == KeyMode::K7 => (
            KeyMode::K14,
            &[
                (Lane::Scratch, Lane::Scratch2),
                (Lane::Key1, Lane::Key8),
                (Lane::Key2, Lane::Key9),
                (Lane::Key3, Lane::Key10),
                (Lane::Key4, Lane::Key11),
                (Lane::Key5, Lane::Key12),
                (Lane::Key6, Lane::Key13),
                (Lane::Key7, Lane::Key14),
            ],
        ),
        _ => return,
    };

    let mut next_id = next_note_id(chart);
    let mut cloned_ids = std::collections::HashMap::new();
    for &(source, destination) in pairs {
        let notes = opponent.lane_notes[source.index()]
            .iter()
            .cloned()
            .map(|mut note| {
                let cloned_id = next_id;
                next_id.0 = next_id.0.saturating_add(1);
                cloned_ids.insert(note.id, cloned_id);
                note.id = cloned_id;
                note.lane = destination;
                note
            })
            .collect();
        chart.lane_notes[destination.index()] = notes;
    }
    let source_to_destination: std::collections::HashMap<_, _> = pairs.iter().copied().collect();
    for pair in &opponent.long_notes {
        let Some(&destination) = source_to_destination.get(&pair.lane) else {
            continue;
        };
        let (Some(&start_note_id), Some(&end_note_id)) =
            (cloned_ids.get(&pair.start_note_id), cloned_ids.get(&pair.end_note_id))
        else {
            continue;
        };
        let mut cloned = pair.clone();
        cloned.lane = destination;
        cloned.start_note_id = start_note_id;
        cloned.end_note_id = end_note_id;
        chart.long_notes.push(cloned);
    }
    // Opponent notes are display-only. Keep the semantic TOTAL NOTES owned by
    // the primary chart so preload/skin/result paths cannot observe a doubled
    // value from the 14-lane presentation container.
    chart.metadata.key_mode = next_mode;
}

pub(crate) fn second_player_lane_mask() -> [bool; LANE_COUNT] {
    let mut mask = [false; LANE_COUNT];
    for lane in [
        Lane::Key8,
        Lane::Key9,
        Lane::Key10,
        Lane::Key11,
        Lane::Key12,
        Lane::Key13,
        Lane::Key14,
        Lane::Scratch2,
    ] {
        mask[lane.index()] = true;
    }
    mask
}

pub(super) fn next_note_id(chart: &PlayableChart) -> NoteId {
    let lane_max = chart.lane_notes.iter().flatten().map(|note| note.id.0).max().unwrap_or(0);
    let long_max = chart
        .long_notes
        .iter()
        .flat_map(|pair| [pair.start_note_id.0, pair.end_note_id.0])
        .max()
        .unwrap_or(0);
    NoteId(lane_max.max(long_max).saturating_add(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArrangeSide {
    P1,
    P2,
}
