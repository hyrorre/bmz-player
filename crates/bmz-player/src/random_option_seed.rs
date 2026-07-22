//! beatoraja 互換の RANDOM オプション用 seed と乱数生成器。
//!
//! beatoraja は各プレイサイドに 24 bit の seed を持ち、DP 時は P1 を下位、P2 を
//! 上位 24 bit に詰めて保存する。譜面の RANDOM 分岐には `java.util.Random` と同じ
//! LCG を使うため、このモジュールでその表現と生成規則を閉じ込める。

use std::time::{SystemTime, UNIX_EPOCH};

/// RANDOM オプション seed 1 本のビット数。
pub const RANDOM_OPTION_SEED_BITS: u32 = 24;
/// RANDOM オプション seed 1 本の最大値。
pub const RANDOM_OPTION_SEED_MAX: u32 = (1 << RANDOM_OPTION_SEED_BITS) - 1;
/// DP の P2 seed を詰めるときの基数。
pub const RANDOM_OPTION_SEED_PACK_BASE: u64 = 1 << RANDOM_OPTION_SEED_BITS;
/// P1/P2 を詰めた RANDOM オプション seed の最大値。
pub const RANDOM_OPTION_SEEDS_MAX: u64 = (1 << (RANDOM_OPTION_SEED_BITS * 2)) - 1;

/// 1 プレイサイド分の 24 bit RANDOM オプション seed。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RandomOptionSeed(u32);

impl RandomOptionSeed {
    /// `value` が 24 bit に収まる場合だけ seed を作る。
    pub const fn new(value: u32) -> Option<Self> {
        if value <= RANDOM_OPTION_SEED_MAX { Some(Self(value)) } else { None }
    }

    /// seed の数値表現を返す。
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// P1 と、DP 時だけ存在する P2 の RANDOM オプション seed。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RandomOptionSeeds {
    pub p1: RandomOptionSeed,
    pub p2: Option<RandomOptionSeed>,
}

impl RandomOptionSeeds {
    /// SP 用の seed 組を作る。
    pub const fn single(p1: RandomOptionSeed) -> Self {
        Self { p1, p2: None }
    }

    /// DP 用の seed 組を作る。
    pub const fn double(p1: RandomOptionSeed, p2: RandomOptionSeed) -> Self {
        Self { p1, p2: Some(p2) }
    }

    /// DP 用の P2 seed を持つかどうかを返す。
    pub const fn is_double(self) -> bool {
        self.p2.is_some()
    }

    /// beatoraja の保存形式へ詰める。P1 は下位、P2 は上位 24 bit を使う。
    pub const fn pack(self) -> u64 {
        let p2 = match self.p2 {
            Some(seed) => seed.value() as u64,
            None => 0,
        };
        self.p1.value() as u64 + p2 * RANDOM_OPTION_SEED_PACK_BASE
    }

    /// beatoraja の保存形式から展開する。
    ///
    /// `is_double` は上位 seed がゼロの DP と SP を区別するために必要である。
    pub const fn unpack(packed: u64, is_double: bool) -> Option<Self> {
        if packed > RANDOM_OPTION_SEEDS_MAX {
            return None;
        }
        if !is_double && packed > RANDOM_OPTION_SEED_MAX as u64 {
            return None;
        }

        let p1 = RandomOptionSeed((packed & RANDOM_OPTION_SEED_MAX as u64) as u32);
        if is_double {
            let p2 = RandomOptionSeed((packed >> RANDOM_OPTION_SEED_BITS) as u32);
            Some(Self::double(p1, p2))
        } else {
            Some(Self::single(p1))
        }
    }

    /// OS 乱数から新しい seed を作る。OS 乱数を得られない場合も時刻由来の値で続行する。
    pub fn fresh(is_double: bool) -> Self {
        let p1 = fresh_seed();
        if is_double { Self::double(p1, fresh_seed()) } else { Self::single(p1) }
    }
}

/// BMS `#RANDOM` の fresh play 用 seed を生成する。
///
/// RANDOM option seed とは別系統にして、譜面分岐がレーン配置設定へ影響しないようにする。
pub fn fresh_bms_random_seed() -> u64 {
    let mut bytes = [0_u8; 8];
    match getrandom::getrandom(&mut bytes) {
        Ok(()) => u64::from_le_bytes(bytes),
        Err(_) => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0),
    }
}

fn fresh_seed() -> RandomOptionSeed {
    let mut bytes = [0_u8; 3];
    let value = match getrandom::getrandom(&mut bytes) {
        Ok(()) => u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]),
        Err(_) => {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u32)
                .unwrap_or(0)
                & RANDOM_OPTION_SEED_MAX
        }
    };

    // 3 byte から作った値と時刻値はともに 24 bit だが、不変条件をここでも明示する。
    RandomOptionSeed(value & RANDOM_OPTION_SEED_MAX)
}

/// `java.util.Random` 互換の 48 bit LCG。
#[derive(Debug, Clone)]
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5DEECE66D;
    const ADDEND: u64 = 0xB;
    const MASK: u64 = (1 << 48) - 1;

    /// Java と同じ seed scrambling を行って生成器を初期化する。
    pub const fn new(seed: i64) -> Self {
        Self { seed: (seed as u64 ^ Self::MULTIPLIER) & Self::MASK }
    }

    fn next(&mut self, bits: u32) -> i32 {
        debug_assert!((1..=32).contains(&bits));
        self.seed =
            (self.seed.wrapping_mul(Self::MULTIPLIER).wrapping_add(Self::ADDEND)) & Self::MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// Java の `Random.nextInt(bound)` と同じ値を返す。
    ///
    /// `bound` は Java と同様に正数でなければならない。
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");

        if (bound & -bound) == bound {
            return ((bound as i64 * self.next(31) as i64) >> 31) as i32;
        }

        loop {
            let bits = self.next(31);
            let value = bits % bound;
            // Java の `bits - value + (bound - 1) >= 0` は i32 overflow を
            // rejection の条件に使う。i64 で同じ境界を明示して判定する。
            if bits - value <= i32::MAX - (bound - 1) {
                return value;
            }
        }
    }

    /// Java の `Random.nextBoolean()` と同じ値を返す。
    pub fn next_bool(&mut self) -> bool {
        self.next(1) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_accepts_exactly_24_bits() {
        assert_eq!(RandomOptionSeed::new(0).unwrap().value(), 0);
        assert_eq!(
            RandomOptionSeed::new(RANDOM_OPTION_SEED_MAX).unwrap().value(),
            RANDOM_OPTION_SEED_MAX
        );
        assert_eq!(RandomOptionSeed::new(RANDOM_OPTION_SEED_MAX + 1), None);
    }

    #[test]
    fn pack_and_unpack_preserve_sp_and_dp_seeds() {
        let p1 = RandomOptionSeed::new(0x123456).unwrap();
        let p2 = RandomOptionSeed::new(0xFEDCBA).unwrap();

        let sp = RandomOptionSeeds::single(p1);
        assert_eq!(sp.pack(), 0x123456);
        assert_eq!(RandomOptionSeeds::unpack(sp.pack(), false), Some(sp));

        let dp = RandomOptionSeeds::double(p1, p2);
        assert_eq!(dp.pack(), 0xFEDCBA_123456);
        assert_eq!(RandomOptionSeeds::unpack(dp.pack(), true), Some(dp));
        assert_eq!(RandomOptionSeeds::unpack(RANDOM_OPTION_SEEDS_MAX + 1, true), None);
    }

    #[test]
    fn unpack_keeps_zero_p2_for_double_play() {
        let p1 = RandomOptionSeed::new(42).unwrap();
        let zero = RandomOptionSeed::new(0).unwrap();
        assert_eq!(
            RandomOptionSeeds::unpack(RandomOptionSeeds::double(p1, zero).pack(), true),
            Some(RandomOptionSeeds::double(p1, zero))
        );
    }

    #[test]
    fn java_random_matches_known_seed_zero_vectors() {
        let mut random = JavaRandom::new(0);
        assert_eq!(random.next(32), -1_155_484_576);
        assert_eq!(random.next(32), -723_955_400);

        let mut random = JavaRandom::new(0);
        assert_eq!(random.next_int_bound(16), 11);

        let mut random = JavaRandom::new(0);
        assert_eq!(random.next_int_bound(100), 60);
        assert!(random.next_bool());
    }
}
