//! RIVALCHART MIRROR: the copied rival chart mirrored left to right on each side, written
//! back as an ordinary arrangement option and beatoraja seed so that the score and the
//! replay reproduce it.

use super::*;
use crate::random_option_seed::RANDOM_OPTION_SEED_MAX;

/// The rival's chart as copied for this play.
pub(crate) struct CopiedArrangement<'a> {
    pub(crate) arrange: ArrangeOption,
    pub(crate) arrange_2p: ArrangeOption,
    pub(crate) double_option: DoubleOption,
    /// The rival's seeds; `None` when only the options were copied.
    pub(crate) seed: Option<i64>,
    pub(crate) seed_2p: Option<i64>,
    pub(crate) legacy_seed: bool,
    /// The rival's final lanes, as stored with a replay.
    pub(crate) pattern: Option<&'a [u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MirroredArrangement {
    pub(crate) arrange: ArrangeOption,
    pub(crate) arrange_2p: ArrangeOption,
    pub(crate) seed: Option<i64>,
    pub(crate) seed_2p: Option<i64>,
}

/// Mirrors each side of a copied chart. NORMAL and MIRROR swap, as do F-RANDOM and
/// MF-RANDOM; RANDOM, RANDOM-EX and R-RANDOM keep their option. Seeded sides take the
/// first beatoraja seed that produces the mirrored lanes. Per-note arrangements
/// (S-RANDOM etc.) and sides without the rival's seed or lanes keep the copied chart.
/// `None` leaves the whole copy unchanged.
pub(crate) fn mirror_copied_arrangement(
    key_mode: KeyMode,
    copied: &CopiedArrangement<'_>,
) -> Option<MirroredArrangement> {
    if !matches!(copied.double_option, DoubleOption::Off | DoubleOption::Flip) {
        return None;
    }
    let is_double = matches!(key_mode, KeyMode::K10 | KeyMode::K14);
    let pattern = copied.pattern.filter(|pattern| {
        let mut sorted = pattern.to_vec();
        sorted.sort_unstable();
        sorted == (0..LANE_COUNT as u8).collect::<Vec<_>>()
    });
    let pattern: Option<Vec<usize>> =
        pattern.map(|pattern| pattern.iter().map(|&lane| usize::from(lane)).collect());
    // Legacy BMZ seeds derive the 2P seed from the 1P seed.
    let source_seed_2p = if copied.legacy_seed {
        copied.seed.map(|seed| seed.wrapping_add(0x9e37_79b9))
    } else {
        copied.seed_2p
    };
    let mirror = |side, arrange, seed| {
        mirror_side(key_mode, side, arrange, seed, copied.legacy_seed, pattern.as_deref())
    };
    let first = mirror(ArrangeSide::P1, copied.arrange, copied.seed);
    let second =
        if is_double { mirror(ArrangeSide::P2, copied.arrange_2p, source_seed_2p) } else { None };
    if first.is_none() && second.is_none() {
        return None;
    }
    // The result uses the current seed scheme: a legacy seed kept on the other side
    // would be read differently.
    if copied.legacy_seed && (first.is_none() || (is_double && second.is_none())) {
        return None;
    }
    let (arrange, seed) = first.unwrap_or((copied.arrange, copied.seed));
    let (arrange_2p, seed_2p) = second.unwrap_or((copied.arrange_2p, copied.seed_2p));
    Some(MirroredArrangement { arrange, arrange_2p, seed, seed_2p })
}

fn mirror_side(
    key_mode: KeyMode,
    side: ArrangeSide,
    arrange: ArrangeOption,
    seed: Option<i64>,
    legacy_seed: bool,
    pattern: Option<&[usize]>,
) -> Option<(ArrangeOption, Option<i64>)> {
    let arrange = if arrange_requires_scratch(arrange) && !key_mode_has_scratch(key_mode) {
        ArrangeOption::Normal
    } else {
        arrange
    };
    let target_arrange = match arrange {
        ArrangeOption::Normal => return Some((ArrangeOption::Mirror, seed)),
        ArrangeOption::Mirror => return Some((ArrangeOption::Normal, seed)),
        ArrangeOption::FRandom => ArrangeOption::MFRandom,
        ArrangeOption::MFRandom => ArrangeOption::FRandom,
        ArrangeOption::Random | ArrangeOption::RandomEx | ArrangeOption::RRandom => arrange,
        _ => return None,
    };
    let source = match pattern {
        Some(pattern) => pattern.to_vec(),
        None => fixed_side_permutation(key_mode, side, arrange, seed, legacy_seed)?,
    };
    let mirror = mirror_lane_permutation_for_side(key_mode, side);
    let target = compose_lane_permutations(&source, &mirror);
    let found = find_seed(key_mode, side, target_arrange, &target, &mirror);
    if found.is_none() {
        tracing::warn!(
            arrange = target_arrange.as_str(),
            ?key_mode,
            "no arrangement seed produces the mirrored lanes; keeping the copied chart"
        );
    }
    found.map(|seed| (target_arrange, Some(seed)))
}

/// The first beatoraja seed (from 0) for which `arrange` puts this side's lanes as in
/// `target`. Every lane order of up to 9 lanes has at least 20 of the 24-bit seeds.
fn find_seed(
    key_mode: KeyMode,
    side: ArrangeSide,
    arrange: ArrangeOption,
    target: &[usize],
    mirror: &[usize],
) -> Option<i64> {
    let max = i64::from(RANDOM_OPTION_SEED_MAX);
    if arrange == ArrangeOption::RRandom {
        // Only 2 * (lanes - 1) rotations and reflections: generate each candidate.
        let lanes: Vec<usize> =
            arrange_lane_groups_for_side(key_mode, false, side).into_iter().flatten().collect();
        return (0..=max).find(|&seed| {
            fixed_side_permutation(key_mode, side, arrange, Some(seed), false)
                .is_some_and(|perm| lanes.iter().all(|&lane| perm[lane] == target[lane]))
        });
    }
    let groups = match arrange {
        ArrangeOption::FRandom | ArrangeOption::MFRandom => {
            f_random_lane_groups_for_side(key_mode, side)
        }
        _ => arrange_lane_groups_for_side(key_mode, arrange == ArrangeOption::RandomEx, side),
    };
    // MF-RANDOM is F-RANDOM followed by the side's mirror, which is its own inverse.
    let shuffled = if arrange == ArrangeOption::MFRandom {
        compose_lane_permutations(target, mirror)
    } else {
        target.to_vec()
    };
    (0..=max).find(|&seed| {
        let mut rng = ArrangeRng::new(seed, false);
        groups.iter().all(|group| shuffle_lane_group_matches(&mut rng, group, &shuffled))
    })
}
