use super::model::JudgeWindow;

/// BMS `#RANK` 値を beatoraja 準拠の判定窓倍率 (%) に変換する。
pub fn judge_rank_to_percent(rank: i32) -> i32 {
    match rank {
        0 => 25,
        1 => 50,
        2 => 75,
        3 => 100,
        4 => 125,
        r if r >= 10 => r,
        // beatoraja `BMSPlayerRule.validate`: 範囲外は NORMAL (75%) へフォールバック
        _ => 75,
    }
}

/// `#RANK` 未指定時は LR2 既定の EASY (100%) を使う。
pub fn judge_rank_to_percent_optional(rank: Option<i32>) -> i32 {
    rank.map(judge_rank_to_percent).unwrap_or(100)
}

/// beatoraja `JudgeWindowRule.NORMAL` に合わせ、PG/GR/GD/BD のみ倍率適用する。
pub fn judge_window_for_rank(base: JudgeWindow, percent: i32) -> JudgeWindow {
    JudgeWindow {
        pgreat_us: scale_window_us(base.pgreat_us, percent),
        great_us: scale_window_us(base.great_us, percent),
        good_us: scale_window_us(base.good_us, percent),
        bad_us: scale_window_us(base.bad_us, percent),
        empty_poor_fast_us: base.empty_poor_fast_us,
        empty_poor_slow_us: base.empty_poor_slow_us,
        mine_hit_us: base.mine_hit_us,
    }
}

pub fn judge_window_from_chart_rank(judge_rank: Option<i32>, base: JudgeWindow) -> JudgeWindow {
    judge_window_for_rank(base, judge_rank_to_percent_optional(judge_rank))
}

fn scale_window_us(value: i64, percent: i32) -> i64 {
    ((value as i128) * percent as i128 / 100)
        .try_into()
        .unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_window() -> JudgeWindow {
        JudgeWindow {
            pgreat_us: 16_000,
            great_us: 40_000,
            good_us: 80_000,
            bad_us: 120_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 200_000,
            mine_hit_us: 16_000,
        }
    }

    #[test]
    fn maps_bms_rank_levels_to_percent() {
        assert_eq!(judge_rank_to_percent(0), 25);
        assert_eq!(judge_rank_to_percent(1), 50);
        assert_eq!(judge_rank_to_percent(2), 75);
        assert_eq!(judge_rank_to_percent(3), 100);
        assert_eq!(judge_rank_to_percent(4), 125);
        assert_eq!(judge_rank_to_percent(120), 120);
        assert_eq!(judge_rank_to_percent(-1), 75);
        assert_eq!(judge_rank_to_percent(9), 75);
    }

    #[test]
    fn none_rank_uses_easy_default() {
        assert_eq!(judge_rank_to_percent_optional(None), 100);
    }

    #[test]
    fn scales_timing_judges_only() {
        let scaled = judge_window_for_rank(base_window(), 50);
        assert_eq!(scaled.pgreat_us, 8_000);
        assert_eq!(scaled.great_us, 20_000);
        assert_eq!(scaled.good_us, 40_000);
        assert_eq!(scaled.bad_us, 60_000);
        assert_eq!(scaled.empty_poor_fast_us, 500_000);
        assert_eq!(scaled.empty_poor_slow_us, 200_000);
        assert_eq!(scaled.mine_hit_us, 16_000);
    }

    #[test]
    fn very_hard_rank_halves_pgreat_window() {
        let window = judge_window_from_chart_rank(Some(0), base_window());
        assert_eq!(window.pgreat_us, 4_000);
    }
}
