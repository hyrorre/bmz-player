//! LR2 RANDOM: the lanes LR2 draws from its seed after the chart's `#RANDOM`.

use super::*;
use bmz_chart::lr2_random::Lr2Mt19937;

/// The lane pattern LR2 gives `key_mode`, continuing `rng` on the 1P side and then the
/// 2P side. Only RANDOM sides draw; MIRROR and NORMAL sides take their fixed lanes.
///
/// `None` when no side is RANDOM, when the mode has no LR2 RANDOM, or when a side uses
/// another option. S-RANDOM and the others keep BMZ's own arrangement, which a single
/// pattern for both sides would override.
pub(super) fn lr2_random_arrange_pattern(
    rng: &mut Lr2Mt19937,
    key_mode: KeyMode,
    arrange: ArrangeOption,
    arrange_2p: ArrangeOption,
) -> Option<Vec<u8>> {
    let sides = match key_mode {
        KeyMode::K5 | KeyMode::K7 | KeyMode::K9 => vec![(ArrangeSide::P1, arrange)],
        KeyMode::K10 | KeyMode::K14 => {
            vec![(ArrangeSide::P1, arrange), (ArrangeSide::P2, arrange_2p)]
        }
        _ => return None,
    };
    if !sides.iter().any(|&(_, arrange)| arrange == ArrangeOption::Random) {
        return None;
    }
    if sides.iter().any(|&(_, arrange)| {
        !matches!(arrange, ArrangeOption::Normal | ArrangeOption::Mirror | ArrangeOption::Random)
    }) {
        return None;
    }
    let mut pattern: Vec<usize> = (0..LANE_COUNT).collect();
    for &(side, arrange) in &sides {
        let perm = match arrange {
            ArrangeOption::Random => lr2_shuffle_side(rng, key_mode, side),
            ArrangeOption::Mirror => mirror_lane_permutation_for_side(key_mode, side),
            _ => continue,
        };
        merge_lane_permutation(&mut pattern, &perm);
    }
    Some(pattern.into_iter().map(|lane| lane as u8).collect())
}

/// One side's LR2 RANDOM lanes. LR2 shuffles the keys only, as
/// [`Lr2Mt19937::shuffle_key_table`] describes.
fn lr2_shuffle_side(rng: &mut Lr2Mt19937, key_mode: KeyMode, side: ArrangeSide) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for keys in arrange_lane_groups_for_side(key_mode, false, side) {
        let table = rng.shuffle_key_table(keys.len());
        // LR2 maps each source key to its destination; a BMZ pattern lists the
        // source lane of each destination lane.
        for (source, &destination) in table.iter().enumerate().skip(1) {
            perm[keys[destination - 1]] = keys[source - 1];
        }
    }
    perm
}
