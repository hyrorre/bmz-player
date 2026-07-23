//! 7K の通常 RANDOM 配置を固定するセッション内 Random Trainer。
//!
//! Endless Dream と同様、指定した7レーン順列を生成する beatoraja 互換 24-bit seed を
//! 探し、既存の通常 RANDOM 経路へ渡す。設定は profile へ保存せず、アプリ実行中だけ
//! 保持する。

use crate::random_option_seed::{JavaRandom, RANDOM_OPTION_SEED_MAX, RandomOptionSeed};

pub const RANDOM_TRAINER_LANE_COUNT: usize = 7;
const IDENTITY_LANE_ORDER: [u8; RANDOM_TRAINER_LANE_COUNT] = [1, 2, 3, 4, 5, 6, 7];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomTrainerState {
    enabled: bool,
    lane_order: [u8; RANDOM_TRAINER_LANE_COUNT],
}

impl Default for RandomTrainerState {
    fn default() -> Self {
        Self { enabled: false, lane_order: IDENTITY_LANE_ORDER }
    }
}

impl RandomTrainerState {
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub const fn lane_order(&self) -> &[u8; RANDOM_TRAINER_LANE_COUNT] {
        &self.lane_order
    }

    pub fn lane_order_string(&self) -> String {
        self.lane_order.iter().map(u8::to_string).collect()
    }

    pub fn reset(&mut self) {
        self.lane_order = IDENTITY_LANE_ORDER;
    }

    pub fn mirror(&mut self) {
        self.lane_order.reverse();
    }

    pub fn shift_left(&mut self) {
        self.lane_order.rotate_left(1);
    }

    pub fn shift_right(&mut self) {
        self.lane_order.rotate_right(1);
    }

    pub fn swap_positions(&mut self, from: usize, to: usize) {
        if from < RANDOM_TRAINER_LANE_COUNT && to < RANDOM_TRAINER_LANE_COUNT {
            self.lane_order.swap(from, to);
        }
    }

    /// Trainer が有効な場合、現在の順列を生成する beatoraja 互換 seed を返す。
    pub fn arrange_seed(&self) -> Option<i64> {
        self.enabled.then(|| {
            i64::from(
                seed_for_lane_order(self.lane_order)
                    .expect("Random Trainer lane order must be a permutation of 1..=7")
                    .value(),
            )
        })
    }
}

/// 指定した7レーン順列を通常 RANDOM で生成する最初の24-bit seedを探す。
///
/// 現在の Java 互換 remove-at-index shuffle では、全5040順列が seed 47587 までに
/// 出現する。上限は互換形式そのものの24-bit範囲とし、将来LUTを持たなくても安全に
/// 全域を探索できるようにする。
pub fn seed_for_lane_order(
    lane_order: [u8; RANDOM_TRAINER_LANE_COUNT],
) -> Option<RandomOptionSeed> {
    if !is_lane_order_permutation(lane_order) {
        return None;
    }

    (0..=RANDOM_OPTION_SEED_MAX).find_map(|seed| {
        (lane_order_for_seed(seed) == lane_order)
            .then(|| RandomOptionSeed::new(seed).expect("24-bit search seed must be valid"))
    })
}

fn is_lane_order_permutation(lane_order: [u8; RANDOM_TRAINER_LANE_COUNT]) -> bool {
    let mut sorted = lane_order;
    sorted.sort_unstable();
    sorted == IDENTITY_LANE_ORDER
}

fn lane_order_for_seed(seed: u32) -> [u8; RANDOM_TRAINER_LANE_COUNT] {
    let mut rng = JavaRandom::new(i64::from(seed));
    let mut remaining = IDENTITY_LANE_ORDER.to_vec();
    let mut lane_order = [0; RANDOM_TRAINER_LANE_COUNT];
    for destination in &mut lane_order {
        let index = rng.next_int_bound(remaining.len() as i32) as usize;
        *destination = remaining.remove(index);
    }
    lane_order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_disabled_identity_order() {
        let trainer = RandomTrainerState::default();

        assert!(!trainer.is_enabled());
        assert_eq!(trainer.lane_order(), &[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(trainer.arrange_seed(), None);
    }

    #[test]
    fn fixed_orders_resolve_to_endless_dream_compatible_seeds() {
        for (order, expected_seed) in [
            ([1, 2, 3, 4, 5, 6, 7], 4_752),
            ([7, 6, 5, 4, 3, 2, 1], 2_701),
            ([2, 1, 4, 3, 6, 5, 7], 322),
        ] {
            let seed = seed_for_lane_order(order).expect("known permutation must resolve");
            assert_eq!(seed.value(), expected_seed);
            assert_eq!(lane_order_for_seed(seed.value()), order);
        }
    }

    #[test]
    fn invalid_lane_orders_are_rejected() {
        assert_eq!(seed_for_lane_order([1, 2, 3, 4, 5, 6, 6]), None);
        assert_eq!(seed_for_lane_order([0, 1, 2, 3, 4, 5, 6]), None);
        assert_eq!(seed_for_lane_order([1, 2, 3, 4, 5, 6, 8]), None);
    }

    #[test]
    fn quick_transform_controls_preserve_a_permutation() {
        let mut trainer = RandomTrainerState::default();

        trainer.shift_left();
        assert_eq!(trainer.lane_order(), &[2, 3, 4, 5, 6, 7, 1]);
        trainer.shift_right();
        assert_eq!(trainer.lane_order(), &[1, 2, 3, 4, 5, 6, 7]);
        trainer.mirror();
        assert_eq!(trainer.lane_order(), &[7, 6, 5, 4, 3, 2, 1]);
        trainer.swap_positions(0, 6);
        assert_eq!(trainer.lane_order(), &[1, 6, 5, 4, 3, 2, 7]);
        trainer.reset();
        assert_eq!(trainer.lane_order(), &[1, 2, 3, 4, 5, 6, 7]);
    }
}
