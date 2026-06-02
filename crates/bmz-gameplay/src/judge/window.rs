use bmz_core::lane::KeyMode;

use crate::rule::RuleMode;

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
        bad_fast_us: scale_window_us(base.bad_fast_us, percent),
        bad_slow_us: scale_window_us(base.bad_slow_us, percent),
        empty_poor_fast_us: base.empty_poor_fast_us,
        empty_poor_slow_us: base.empty_poor_slow_us,
        mine_hit_us: base.mine_hit_us,
    }
}

pub fn judge_window_for_rule_mode(
    base: JudgeWindow,
    percent: i32,
    rule_mode: RuleMode,
) -> JudgeWindow {
    match rule_mode {
        RuleMode::Beatoraja => judge_window_for_rank(base, percent),
        RuleMode::Lr2Oraja => lr2oraja_judge_window_for_rank(base, percent),
        RuleMode::Dx => base,
    }
}

pub fn judge_window_from_chart_rank(judge_rank: Option<i32>, base: JudgeWindow) -> JudgeWindow {
    judge_window_for_rank(base, judge_rank_to_percent_optional(judge_rank))
}

pub fn judge_window_from_chart_rank_for_rule_mode(
    judge_rank: Option<i32>,
    base: JudgeWindow,
    rule_mode: RuleMode,
) -> JudgeWindow {
    judge_window_for_rule_mode(base, judge_rank_to_percent_optional(judge_rank), rule_mode)
}

pub const fn note_judge_window_for_rule_mode(
    key_mode: KeyMode,
    rule_mode: RuleMode,
) -> JudgeWindow {
    match rule_mode {
        RuleMode::Beatoraja => beatoraja_note_judge_window_for_keymode(key_mode),
        RuleMode::Lr2Oraja => lr2oraja_note_judge_window(),
        RuleMode::Dx => dx_note_judge_window(),
    }
}

/// beatoraja `JudgeProperty` NOTE table for the default player rule of a key mode.
pub const fn beatoraja_note_judge_window_for_keymode(key_mode: KeyMode) -> JudgeWindow {
    match key_mode {
        KeyMode::K5 | KeyMode::K10 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 50_000,
            good_us: 100_000,
            bad_fast_us: 150_000,
            bad_slow_us: 150_000,
            empty_poor_fast_us: 150_000,
            empty_poor_slow_us: 500_000,
            mine_hit_us: 16_000,
        },
        KeyMode::K9 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 50_000,
            good_us: 117_000,
            bad_fast_us: 183_000,
            bad_slow_us: 183_000,
            empty_poor_fast_us: 175_000,
            empty_poor_slow_us: 500_000,
            mine_hit_us: 16_000,
        },
        KeyMode::K7 | KeyMode::K14 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 60_000,
            good_us: 150_000,
            bad_fast_us: 280_000,
            bad_slow_us: 220_000,
            empty_poor_fast_us: 150_000,
            empty_poor_slow_us: 500_000,
            mine_hit_us: 16_000,
        },
        // beatoraja `Beatoraja_Other` uses SEVENKEYS rules.
        KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 60_000,
            good_us: 150_000,
            bad_fast_us: 280_000,
            bad_slow_us: 220_000,
            empty_poor_fast_us: 150_000,
            empty_poor_slow_us: 500_000,
            mine_hit_us: 16_000,
        },
    }
}

/// LR2oraja `JudgeProperty.LR2` NOTE window.
pub const fn lr2oraja_note_judge_window() -> JudgeWindow {
    JudgeWindow {
        pgreat_us: 21_000,
        great_us: 60_000,
        good_us: 120_000,
        bad_fast_us: 200_000,
        bad_slow_us: 200_000,
        empty_poor_fast_us: 1_000_000,
        empty_poor_slow_us: 0,
        mine_hit_us: 16_000,
    }
}

/// LR2oraja `JudgeProperty.IIDX` NOTE window used by DX mode.
pub const fn dx_note_judge_window() -> JudgeWindow {
    JudgeWindow {
        pgreat_us: 16_666,
        great_us: 33_333,
        good_us: 116_666,
        bad_fast_us: 200_000,
        bad_slow_us: 200_000,
        empty_poor_fast_us: 1_000_000,
        empty_poor_slow_us: 200_000,
        mine_hit_us: 16_000,
    }
}

/// 譜面ヘッダ `#RANK` と `#EXRANK` イベントから、指定時刻の判定倍率 (%) を求める。
pub fn judge_percent_at_time(
    header_rank: Option<i32>,
    events: &[bmz_chart::model::JudgeRankEvent],
    now: bmz_core::time::TimeUs,
) -> i32 {
    let mut percent = judge_rank_to_percent_optional(header_rank);
    for event in events {
        if event.time <= now {
            percent = event.rank_percent;
        } else {
            break;
        }
    }
    percent
}

fn scale_window_us(value: i64, percent: i32) -> i64 {
    ((value as i128) * percent as i128 / 100).try_into().unwrap_or(if value < 0 {
        i64::MIN
    } else {
        i64::MAX
    })
}

fn lr2oraja_judge_window_for_rank(base: JudgeWindow, percent: i32) -> JudgeWindow {
    let bad = base.bad_fast_us.max(base.bad_slow_us);
    JudgeWindow {
        pgreat_us: lr2_scale_window_us(base.pgreat_us, percent, 0, bad),
        great_us: lr2_scale_window_us(base.great_us, percent, 1, bad),
        good_us: lr2_scale_window_us(base.good_us, percent, 2, bad),
        bad_fast_us: base.bad_fast_us,
        bad_slow_us: base.bad_slow_us,
        empty_poor_fast_us: base.empty_poor_fast_us,
        empty_poor_slow_us: base.empty_poor_slow_us,
        mine_hit_us: base.mine_hit_us,
    }
}

const LR2_SCALING: [[i64; 5]; 4] = [
    [0, 0, 0, 0, 0],
    [0, 8_000, 15_000, 18_000, 21_000],
    [0, 24_000, 30_000, 40_000, 60_000],
    [0, 40_000, 60_000, 100_000, 120_000],
];

fn lr2_scale_window_us(base: i64, percent: i32, index: usize, bad_window: i64) -> i64 {
    if percent >= 100 {
        return scale_window_us(base, percent);
    }

    let sign = base.signum();
    let rank = percent.max(0);
    let table_index = index + 1;
    let low_index = (rank / 25).clamp(0, 4) as usize;
    let high_index = (low_index + 1).min(4);
    let low_rank = (low_index as i32) * 25;
    let high_rank = (high_index as i32) * 25;
    let low = LR2_SCALING[table_index][low_index];
    let high = LR2_SCALING[table_index][high_index];
    let scaled = if high_rank == low_rank {
        low
    } else {
        low + (high - low) * (rank - low_rank) as i64 / (high_rank - low_rank) as i64
    };
    (sign * scaled).clamp(-bad_window, bad_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_window() -> JudgeWindow {
        JudgeWindow::symmetric(16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000)
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
        assert_eq!(scaled.bad_fast_us, 60_000);
        assert_eq!(scaled.bad_slow_us, 60_000);
        assert_eq!(scaled.empty_poor_fast_us, 500_000);
        assert_eq!(scaled.empty_poor_slow_us, 200_000);
        assert_eq!(scaled.mine_hit_us, 16_000);
    }

    #[test]
    fn very_hard_rank_halves_pgreat_window() {
        let window = judge_window_from_chart_rank(Some(0), base_window());
        assert_eq!(window.pgreat_us, 4_000);
    }

    #[test]
    fn beatoraja_7k_note_window_uses_asymmetric_bad_and_empty_poor() {
        let window = beatoraja_note_judge_window_for_keymode(KeyMode::K7);
        assert_eq!(window.pgreat_us, 20_000);
        assert_eq!(window.great_us, 60_000);
        assert_eq!(window.good_us, 150_000);
        assert_eq!(window.bad_fast_us, 280_000);
        assert_eq!(window.bad_slow_us, 220_000);
        assert_eq!(window.empty_poor_fast_us, 150_000);
        assert_eq!(window.empty_poor_slow_us, 500_000);
    }

    #[test]
    fn lr2oraja_rank_scaling_matches_reference_table() {
        let base = lr2oraja_note_judge_window();
        let window = judge_window_for_rule_mode(base, 50, RuleMode::Lr2Oraja);

        assert_eq!(window.pgreat_us, 15_000);
        assert_eq!(window.great_us, 30_000);
        assert_eq!(window.good_us, 60_000);
        assert_eq!(window.bad_fast_us, 200_000);
        assert_eq!(window.empty_poor_fast_us, 1_000_000);
        assert_eq!(window.empty_poor_slow_us, 0);
    }

    #[test]
    fn dx_mode_uses_iidx_window_without_rank_scaling() {
        let base = dx_note_judge_window();
        let window = judge_window_for_rule_mode(base, 25, RuleMode::Dx);

        assert_eq!(window.pgreat_us, 16_666);
        assert_eq!(window.great_us, 33_333);
        assert_eq!(window.good_us, 116_666);
        assert_eq!(window.bad_fast_us, 200_000);
        assert_eq!(window.empty_poor_fast_us, 1_000_000);
        assert_eq!(window.empty_poor_slow_us, 200_000);
    }

    #[test]
    fn exrank_events_override_header_rank() {
        use bmz_chart::model::JudgeRankEvent;
        use bmz_core::time::TimeUs;

        let events = vec![
            JudgeRankEvent { tick: Default::default(), time: TimeUs(1_000), rank_percent: 50 },
            JudgeRankEvent { tick: Default::default(), time: TimeUs(2_000), rank_percent: 25 },
        ];
        assert_eq!(judge_percent_at_time(Some(3), &events, TimeUs(0)), 100);
        assert_eq!(judge_percent_at_time(Some(3), &events, TimeUs(1_500)), 50);
        assert_eq!(judge_percent_at_time(Some(3), &events, TimeUs(2_500)), 25);
    }
}
