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
    black_white_random: bool,
    partial_random_lanes: [bool; RANDOM_TRAINER_LANE_COUNT],
}

impl Default for RandomTrainerState {
    fn default() -> Self {
        Self {
            enabled: false,
            lane_order: IDENTITY_LANE_ORDER,
            black_white_random: false,
            partial_random_lanes: [false; RANDOM_TRAINER_LANE_COUNT],
        }
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

    pub const fn black_white_random(&self) -> bool {
        self.black_white_random
    }

    pub fn set_black_white_random(&mut self, enabled: bool) {
        self.black_white_random = enabled;
    }

    pub fn is_lane_partial_random(&self, lane: u8) -> bool {
        lane.checked_sub(1)
            .and_then(|index| self.partial_random_lanes.get(index as usize))
            .copied()
            .unwrap_or(false)
    }

    pub fn toggle_lane_partial_random(&mut self, lane: u8) {
        if let Some(index) = lane.checked_sub(1)
            && let Some(selected) = self.partial_random_lanes.get_mut(index as usize)
        {
            *selected = !*selected;
        }
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

    /// Trainer が有効な場合、現在の設定から生成した順列の beatoraja 互換 seed を返す。
    pub fn arrange_seed(&self, entropy_seed: RandomOptionSeed) -> Option<i64> {
        self.enabled.then(|| {
            let lane_order = self.resolved_lane_order(entropy_seed);
            i64::from(
                seed_for_lane_order(lane_order)
                    .expect("Random Trainer lane order must be a permutation of 1..=7")
                    .value(),
            )
        })
    }

    fn resolved_lane_order(
        &self,
        entropy_seed: RandomOptionSeed,
    ) -> [u8; RANDOM_TRAINER_LANE_COUNT] {
        let mut lane_order = self.lane_order;
        let mut rng = JavaRandom::new(i64::from(entropy_seed.value()));

        // Endless Dream と同じ順序で、青鍵（偶数）と白鍵（奇数）を個別に並べ替える。
        if self.black_white_random {
            shuffle_matching_lanes(&mut lane_order, &mut rng, |lane| lane % 2 == 0);
            shuffle_matching_lanes(&mut lane_order, &mut rng, |lane| lane % 2 != 0);
        }

        // 部分ランダムは「位置」ではなく選択した元レーン番号に追随する。
        shuffle_matching_lanes(&mut lane_order, &mut rng, |lane| self.is_lane_partial_random(lane));
        lane_order
    }
}

fn shuffle_matching_lanes(
    lane_order: &mut [u8; RANDOM_TRAINER_LANE_COUNT],
    rng: &mut JavaRandom,
    mut matches: impl FnMut(u8) -> bool,
) {
    let indices = lane_order
        .iter()
        .enumerate()
        .filter_map(|(index, &lane)| matches(lane).then_some(index))
        .collect::<Vec<_>>();
    let mut lanes = indices.iter().map(|&index| lane_order[index]).collect::<Vec<_>>();

    // java.util.Collections.shuffle と同じ後ろからの Fisher-Yates。
    for index in (1..lanes.len()).rev() {
        let swap_with = rng.next_int_bound((index + 1) as i32) as usize;
        lanes.swap(index, swap_with);
    }
    for (index, lane) in indices.into_iter().zip(lanes) {
        lane_order[index] = lane;
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
        assert!(!trainer.black_white_random());
        assert_eq!(trainer.arrange_seed(RandomOptionSeed::new(42).unwrap()), None);
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

    #[test]
    fn black_white_random_keeps_each_lane_in_its_color_slots() {
        let mut trainer = RandomTrainerState::default();
        trainer.set_black_white_random(true);

        let resolved = trainer.resolved_lane_order(RandomOptionSeed::new(42).unwrap());

        for (base, shuffled) in trainer.lane_order().iter().zip(resolved) {
            assert_eq!(base % 2, shuffled % 2);
        }
        assert_ne!(resolved, IDENTITY_LANE_ORDER);
        assert!(is_lane_order_permutation(resolved));
    }

    #[test]
    fn partial_random_only_moves_selected_lane_numbers() {
        let mut trainer = RandomTrainerState::default();
        for lane in [2, 4, 6] {
            trainer.toggle_lane_partial_random(lane);
        }

        let resolved = trainer.resolved_lane_order(RandomOptionSeed::new(42).unwrap());

        assert_eq!(resolved[0], 1);
        assert_eq!(resolved[2], 3);
        assert_eq!(resolved[4], 5);
        assert_eq!(resolved[6], 7);
        assert_eq!(
            resolved.into_iter().filter(|lane| lane % 2 == 0).collect::<Vec<_>>(),
            vec![4, 2, 6]
        );
    }

    #[test]
    fn partial_random_selection_follows_the_lane_number_after_reorder() {
        let mut trainer = RandomTrainerState::default();
        trainer.toggle_lane_partial_random(1);
        trainer.swap_positions(0, 6);

        assert!(trainer.is_lane_partial_random(1));
        assert!(!trainer.is_lane_partial_random(7));
        assert_eq!(trainer.lane_order(), &[7, 2, 3, 4, 5, 6, 1]);
    }

    #[test]
    fn black_white_then_partial_random_remains_a_full_permutation() {
        let mut trainer = RandomTrainerState::default();
        trainer.set_black_white_random(true);
        for lane in [1, 2, 5, 6] {
            trainer.toggle_lane_partial_random(lane);
        }

        let resolved = trainer.resolved_lane_order(RandomOptionSeed::new(1234).unwrap());

        assert!(is_lane_order_permutation(resolved));
    }

    #[test]
    fn generated_mode_seed_reproduces_the_resolved_lane_order() {
        let mut trainer = RandomTrainerState::default();
        trainer.set_enabled(true);
        trainer.set_black_white_random(true);
        for lane in [1, 2, 5, 6] {
            trainer.toggle_lane_partial_random(lane);
        }
        let entropy = RandomOptionSeed::new(1234).unwrap();
        let resolved = trainer.resolved_lane_order(entropy);

        let seed = trainer.arrange_seed(entropy).expect("enabled trainer must return a seed");

        assert_eq!(lane_order_for_seed(seed as u32), resolved);
    }
}
