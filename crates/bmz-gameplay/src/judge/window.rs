use bmz_core::lane::KeyMode;

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

pub fn judge_window_from_chart_rank(judge_rank: Option<i32>, base: JudgeWindow) -> JudgeWindow {
    judge_window_for_rank(base, judge_rank_to_percent_optional(judge_rank))
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
