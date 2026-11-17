//! LR2 互換の乱数列。
//!
//! LR2 は DxLib の MT19937 を使う。生成器本体は標準の MT19937 だが、初期化は現行標準の
//! `init_genrand` ではなく、DxLib の旧式 `srandMT`（69069 の LCG で上位16bitずつ詰める）を
//! 使う。1つの seed から作った列を、譜面の `#RANDOM` 解決、RANDOM のレーン抽選の順に
//! 消費する。このため `#RANDOM` を解決した後の状態を [`Clone`] で配置の計算へ渡す。

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// LR2 のランキングが記録する seed の最大値。
pub const LR2_SEED_MAX: u32 = 0x7ffe;

/// DxLib 旧式初期化の MT19937。
#[derive(Clone)]
pub struct Lr2Mt19937 {
    state: Box<[u32; N]>,
    index: usize,
}

impl std::fmt::Debug for Lr2Mt19937 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lr2Mt19937").field("index", &self.index).finish_non_exhaustive()
    }
}

impl Lr2Mt19937 {
    /// DxLib の `srandMT(seed)` と同じ状態を作る。初期化の直後に1回 twist する。
    pub fn new(seed: u32) -> Self {
        let mut state = Box::new([0u32; N]);
        let mut seed = seed;
        for word in state.iter_mut() {
            *word = seed & 0xffff_0000;
            seed = seed.wrapping_mul(69069).wrapping_add(1);
            *word |= (seed & 0xffff_0000) >> 16;
            seed = seed.wrapping_mul(69069).wrapping_add(1);
        }
        let mut rng = Self { state, index: N };
        rng.twist();
        rng
    }

    fn twist(&mut self) {
        let mt = &mut self.state;
        for k in 0..N {
            let y = (mt[k] & UPPER_MASK) | (mt[(k + 1) % N] & LOWER_MASK);
            let mag = if y & 1 == 0 { 0 } else { MATRIX_A };
            mt[k] = mt[(k + M) % N] ^ (y >> 1) ^ mag;
        }
        self.index = 0;
    }

    /// 標準の tempering を通した 32bit 出力。
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// DxLib の `GetRand(max)`。`0..=max` を剰余ではなく乗算と上位32bitで選ぶ。
    ///
    /// 常に1回消費する。`max` が負（`#RANDOM 0` の `GetRand(-1)` 等）のときは 0 を返す。
    pub fn get_rand(&mut self, max: i64) -> i64 {
        let value = u64::from(self.next_u32());
        let range = u64::try_from(max.saturating_add(1)).unwrap_or(0);
        ((u128::from(value) * u128::from(range)) >> 32) as i64
    }

    /// RANDOM のレーン表を作る。`keys` 本の鍵盤を対象にし、戻り値は
    /// `table[元の鍵盤] = 移動先の鍵盤` で、鍵盤は 1 始まり（`table[0]` は常に 0）。
    ///
    /// LR2 は前方から `c = 1..keys` について `a = c + GetRand(keys - c)` と交換する。
    /// スクラッチを含めて回す変種もあるが、ランキングの記録にその情報が無いため、
    /// ここでは常にスクラッチを回さない前提で鍵盤だけを扱う。
    pub fn shuffle_key_table(&mut self, keys: usize) -> Vec<usize> {
        let mut table: Vec<usize> = (0..=keys).collect();
        for c in 1..keys {
            let a = c + self.get_rand((keys - c) as i64) as usize;
            table.swap(c, a);
        }
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_outputs(seed: u32) -> [u32; 5] {
        let mut rng = Lr2Mt19937::new(seed);
        std::array::from_fn(|_| rng.next_u32())
    }

    #[test]
    fn raw_outputs_match_the_dxlib_initialization() {
        assert_eq!(first_outputs(391), [228831979, 266585037, 255280781, 80915365, 333757622]);
        assert_eq!(first_outputs(2179), [88460912, 2752084135, 2394320606, 2582019784, 1363510547]);
        assert_eq!(first_outputs(0), [477480905, 3667282373, 2229421029, 2807835183, 2861012660]);
    }

    #[test]
    fn get_rand_scales_the_output_and_always_consumes() {
        let mut rng = Lr2Mt19937::new(2179);
        // 88460912 * 3 >> 32 == 0
        assert_eq!(rng.get_rand(2), 0);
        let mut reference = Lr2Mt19937::new(2179);
        reference.next_u32();
        assert_eq!(rng.get_rand(-1), 0);
        reference.next_u32();
        assert_eq!(rng.next_u32(), reference.next_u32());
    }

    #[test]
    fn seven_key_tables_follow_the_seed_and_earlier_draws() {
        assert_eq!(Lr2Mt19937::new(391).shuffle_key_table(7), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(Lr2Mt19937::new(2179).shuffle_key_table(7), [0, 1, 5, 2, 6, 3, 7, 4]);

        let mut rng = Lr2Mt19937::new(2179);
        assert_eq!(rng.get_rand(3 - 1) + 1, 1);
        assert_eq!(rng.shuffle_key_table(7), [0, 5, 1, 6, 2, 7, 4, 3]);

        let mut rng = Lr2Mt19937::new(391);
        assert_eq!(rng.get_rand(2 - 1) + 1, 1);
        assert_eq!(rng.get_rand(4 - 1) + 1, 1);
        assert_eq!(rng.shuffle_key_table(7), [0, 1, 2, 3, 4, 6, 5, 7]);
    }

    #[test]
    fn double_play_builds_the_second_side_from_the_same_sequence() {
        let mut rng = Lr2Mt19937::new(2179);
        assert_eq!(rng.shuffle_key_table(7), [0, 1, 5, 2, 6, 3, 7, 4]);
        assert_eq!(rng.shuffle_key_table(7), [0, 4, 6, 5, 2, 1, 7, 3]);
    }

    #[test]
    fn five_key_table() {
        assert_eq!(Lr2Mt19937::new(2179).shuffle_key_table(5), [0, 1, 4, 2, 5, 3]);
    }
}
