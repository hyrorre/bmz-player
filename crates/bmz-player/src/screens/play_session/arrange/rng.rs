use super::*;

#[derive(Debug, Clone)]
pub(super) struct SplitMix64 {
    pub(super) seed: u64,
}

impl SplitMix64 {
    pub(super) fn new(seed: i64) -> Self {
        Self { seed: seed as u64 }
    }

    pub(super) fn next_u64(&mut self) -> u64 {
        self.seed = self.seed.wrapping_add(0x9E3779B97F4A7C15);
        let mut value = self.seed;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
        value ^ (value >> 31)
    }

    pub(super) fn next_usize(&mut self, bound: usize) -> usize {
        assert!(bound > 0);
        let bound = bound as u128;
        let zone = ((1u128 << 64) / bound) * bound;
        loop {
            let value = self.next_u64() as u128;
            if value < zone {
                return (value % bound) as usize;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum ArrangeRng {
    Beatoraja(JavaRandom),
    Legacy(SplitMix64),
}

impl ArrangeRng {
    pub(super) fn new(seed: i64, legacy_seed: bool) -> Self {
        if legacy_seed {
            Self::Legacy(SplitMix64::new(seed))
        } else {
            Self::Beatoraja(JavaRandom::new(seed))
        }
    }

    pub(super) fn next_usize(&mut self, bound: usize) -> usize {
        assert!(bound > 0);
        match self {
            Self::Beatoraja(random) => random.next_int_bound(bound as i32) as usize,
            Self::Legacy(random) => random.next_usize(bound),
        }
    }
}

pub(super) fn shuffle_lane_group(
    rng: &mut ArrangeRng,
    lanes: &[usize],
    perm: &mut [usize],
    legacy_seed: bool,
) {
    if legacy_seed {
        fisher_yates_shuffle(rng, lanes, perm);
        return;
    }

    // beatoraja LaneRandomShuffleModifier: source lane order is stable and
    // each destination is selected from, then removed from, the remaining list.
    let mut remaining = lanes.to_vec();
    for &lane in lanes {
        let index = rng.next_usize(remaining.len());
        perm[lane] = remaining.remove(index);
    }
}

/// Whether the beatoraja shuffle above sends `lanes` to `target`, stopping at the first
/// mismatch. Seed searches call this millions of times, so it does not allocate.
pub(super) fn shuffle_lane_group_matches(
    rng: &mut ArrangeRng,
    lanes: &[usize],
    target: &[usize],
) -> bool {
    let mut remaining = [0usize; LANE_COUNT];
    remaining[..lanes.len()].copy_from_slice(lanes);
    let mut len = lanes.len();
    for &lane in lanes {
        let index = rng.next_usize(len);
        if remaining[index] != target[lane] {
            return false;
        }
        remaining.copy_within(index + 1..len, index);
        len -= 1;
    }
    true
}

pub(super) fn fisher_yates_shuffle(rng: &mut ArrangeRng, lanes: &[usize], perm: &mut [usize]) {
    if lanes.len() < 2 {
        return;
    }
    let mut indices: Vec<usize> = lanes.to_vec();
    for i in (1..indices.len()).rev() {
        let j = rng.next_usize(i + 1);
        indices.swap(i, j);
    }
    for (orig, new_target) in lanes.iter().zip(indices.iter()) {
        perm[*orig] = *new_target;
    }
}
