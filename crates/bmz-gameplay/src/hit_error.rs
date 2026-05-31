use bmz_core::judge::Judge;

/// beatoraja `JudgeManager.recentJudges` と同じ長さ。
pub const HIT_ERROR_RING_LEN: usize = 100;
/// beatoraja `Long.MIN_VALUE` — 未使用スロット。
pub const HIT_ERROR_EMPTY: i64 = i64::MIN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitErrorRing {
    pub values: [i64; HIT_ERROR_RING_LEN],
    pub index: usize,
}

impl Default for HitErrorRing {
    fn default() -> Self {
        Self { values: [HIT_ERROR_EMPTY; HIT_ERROR_RING_LEN], index: 0 }
    }
}

impl HitErrorRing {
    /// PG/GR/GD/BD のみ beatoraja と同様に記録する (`judge < 4`)。
    pub fn push_judgement(&mut self, judge: Judge, delta_us: i64) {
        if !matches!(judge, Judge::PGreat | Judge::Great | Judge::Good | Judge::Bad) {
            return;
        }
        self.index = (self.index + 1) % HIT_ERROR_RING_LEN;
        self.values[self.index] = delta_us / 1_000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_error_ring_records_pg_and_skips_poor() {
        let mut ring = HitErrorRing::default();
        ring.push_judgement(Judge::PGreat, 12_000);
        ring.push_judgement(Judge::Poor, 5_000);
        assert_eq!(ring.values[ring.index], 12);
        assert_eq!(ring.index, 1);
    }
}
