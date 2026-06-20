use bmz_chart::model::{JudgeRankEvent, JudgeRankKind, JudgeRankSpec};
use bmz_core::lane::KeyMode;

use crate::rule::RuleMode;

use super::model::{JudgeWindow, JudgeWindows};

const BEATORAJA_NORMAL_JUDGE_RANK: [i32; 5] = [25, 50, 75, 100, 125];
const BEATORAJA_PMS_JUDGE_RANK: [i32; 5] = [33, 50, 70, 100, 133];
const BEATORAJA_NORMAL_FIX_JUDGE: [bool; 5] = [false, false, false, false, true];
const BEATORAJA_PMS_FIX_JUDGE: [bool; 5] = [true, false, false, true, true];

/// BMS `#RANK` 値を beatoraja 準拠の判定窓倍率 (%) に変換する。
pub fn judge_rank_to_percent(rank: i32) -> i32 {
    beatoraja_rank_to_percent_for_table(rank, BEATORAJA_NORMAL_JUDGE_RANK)
}

pub fn beatoraja_judge_rank_to_percent_for_keymode(rank: i32, key_mode: KeyMode) -> i32 {
    let table = beatoraja_rank_table_for_keymode(key_mode);
    beatoraja_rank_to_percent_for_table(rank, table)
}

fn beatoraja_rank_to_percent_for_table(rank: i32, table: [i32; 5]) -> i32 {
    match rank {
        0..=4 => table[rank as usize],
        r if r >= 10 => r,
        // beatoraja `BMSPlayerRule.validate`: 範囲外は NORMAL (#RANK 2) へフォールバック
        _ => table[2],
    }
}

/// `#RANK` 未指定時は beatoraja 既定の EASY (100%) を使う。
pub fn judge_rank_to_percent_optional(rank: Option<i32>) -> i32 {
    rank.map(judge_rank_to_percent).unwrap_or(100)
}

pub fn judge_rank_to_percent_for_rule_mode(rank: i32, rule_mode: RuleMode) -> i32 {
    match rule_mode {
        RuleMode::Lr2Oraja => lr2oraja_judge_rank_to_percent(rank),
        RuleMode::Beatoraja | RuleMode::Dx => judge_rank_to_percent(rank),
    }
}

pub fn judge_rank_to_percent_optional_for_rule_mode(rank: Option<i32>, rule_mode: RuleMode) -> i32 {
    match rule_mode {
        RuleMode::Lr2Oraja => rank.map(lr2oraja_judge_rank_to_percent).unwrap_or(75),
        RuleMode::Beatoraja | RuleMode::Dx => judge_rank_to_percent_optional(rank),
    }
}

pub fn judge_rank_spec_to_percent_optional_for_rule_mode(
    spec: Option<JudgeRankSpec>,
    rule_mode: RuleMode,
) -> i32 {
    judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(spec, KeyMode::K7, rule_mode)
}

pub fn judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
    spec: Option<JudgeRankSpec>,
    key_mode: KeyMode,
    rule_mode: RuleMode,
) -> i32 {
    match rule_mode {
        RuleMode::Lr2Oraja => lr2oraja_judge_rank_spec_to_percent(spec),
        RuleMode::Beatoraja | RuleMode::Dx => beatoraja_judge_rank_spec_to_percent(spec, key_mode),
    }
}

fn beatoraja_judge_rank_spec_to_percent(spec: Option<JudgeRankSpec>, key_mode: KeyMode) -> i32 {
    match spec {
        None => 100,
        Some(JudgeRankSpec { value, kind: JudgeRankKind::BmsRank }) => {
            beatoraja_judge_rank_to_percent_for_keymode(value, key_mode)
        }
        Some(JudgeRankSpec { value, kind: JudgeRankKind::DefExRank }) => {
            let normal = beatoraja_judge_rank_to_percent_for_keymode(2, key_mode);
            if value > 0 { value * normal / 100 } else { normal }
        }
        Some(JudgeRankSpec { value, kind: JudgeRankKind::BmsonJudgeRank }) if value > 0 => value,
        Some(JudgeRankSpec { kind: JudgeRankKind::BmsonJudgeRank, .. }) => 100,
    }
}

fn lr2oraja_judge_rank_to_percent(rank: i32) -> i32 {
    match rank {
        0 => 25,
        1 => 50,
        2 => 75,
        3 => 100,
        // 元祖 LR2 は #RANK 4 非対応で、NORMAL (#RANK 2) 相当にフォールバックする。
        4 => 75,
        r if r >= 10 => r,
        _ => 75,
    }
}

fn lr2oraja_judge_rank_spec_to_percent(spec: Option<JudgeRankSpec>) -> i32 {
    match spec {
        None => 75,
        Some(JudgeRankSpec { value, kind: JudgeRankKind::BmsRank }) => {
            lr2oraja_judge_rank_to_percent(value)
        }
        Some(JudgeRankSpec { value, kind: JudgeRankKind::DefExRank }) if value > 0 => {
            value * 75 / 100
        }
        Some(JudgeRankSpec { kind: JudgeRankKind::DefExRank, .. }) => 75,
        Some(JudgeRankSpec { value, kind: JudgeRankKind::BmsonJudgeRank }) => {
            if value > 0 {
                value
            } else {
                100
            }
        }
    }
}

/// beatoraja `JudgeWindowRule.NORMAL` に合わせ、PG/GR/GD/BD のみ倍率適用する。
pub fn judge_window_for_rank(base: JudgeWindow, percent: i32) -> JudgeWindow {
    beatoraja_judge_window_for_rank_and_keymode(base, percent, KeyMode::K7)
}

pub fn judge_windows_for_rank(base: JudgeWindows, percent: i32) -> JudgeWindows {
    JudgeWindows {
        note: judge_window_for_rank(base.note, percent),
        scratch: judge_window_for_rank(base.scratch, percent),
        long_note_end: judge_window_for_rank(base.long_note_end, percent),
        long_scratch_end: judge_window_for_rank(base.long_scratch_end, percent),
    }
}

pub fn judge_window_for_rule_mode(
    base: JudgeWindow,
    percent: i32,
    rule_mode: RuleMode,
) -> JudgeWindow {
    judge_window_for_rule_mode_and_keymode(base, percent, rule_mode, KeyMode::K7)
}

pub fn judge_window_for_rule_mode_and_keymode(
    base: JudgeWindow,
    percent: i32,
    rule_mode: RuleMode,
    key_mode: KeyMode,
) -> JudgeWindow {
    match rule_mode {
        RuleMode::Beatoraja => beatoraja_judge_window_for_rank_and_keymode(base, percent, key_mode),
        RuleMode::Lr2Oraja => lr2oraja_judge_window_for_rank(base, percent),
        RuleMode::Dx => base,
    }
}

pub fn judge_windows_for_rule_mode(
    base: JudgeWindows,
    percent: i32,
    rule_mode: RuleMode,
) -> JudgeWindows {
    judge_windows_for_rule_mode_and_keymode(base, percent, rule_mode, KeyMode::K7)
}

pub fn judge_windows_for_rule_mode_and_keymode(
    base: JudgeWindows,
    percent: i32,
    rule_mode: RuleMode,
    key_mode: KeyMode,
) -> JudgeWindows {
    match rule_mode {
        RuleMode::Beatoraja => JudgeWindows {
            note: beatoraja_judge_window_for_rank_and_keymode(base.note, percent, key_mode),
            scratch: beatoraja_judge_window_for_rank_and_keymode(base.scratch, percent, key_mode),
            long_note_end: beatoraja_judge_window_for_rank_and_keymode(
                base.long_note_end,
                percent,
                key_mode,
            ),
            long_scratch_end: beatoraja_judge_window_for_rank_and_keymode(
                base.long_scratch_end,
                percent,
                key_mode,
            ),
        },
        RuleMode::Lr2Oraja => JudgeWindows {
            note: lr2oraja_judge_window_for_rank(base.note, percent),
            scratch: lr2oraja_judge_window_for_rank(base.scratch, percent),
            long_note_end: lr2oraja_judge_window_for_rank(base.long_note_end, percent),
            long_scratch_end: lr2oraja_judge_window_for_rank(base.long_scratch_end, percent),
        },
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
    judge_window_for_rule_mode(
        base,
        judge_rank_to_percent_optional_for_rule_mode(judge_rank, rule_mode),
        rule_mode,
    )
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

pub const fn judge_windows_for_keymode_and_rule_mode(
    key_mode: KeyMode,
    rule_mode: RuleMode,
) -> JudgeWindows {
    match rule_mode {
        RuleMode::Beatoraja => beatoraja_judge_windows_for_keymode(key_mode),
        RuleMode::Lr2Oraja => lr2oraja_judge_windows(),
        RuleMode::Dx => dx_judge_windows(),
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
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 150_000,
            mine_hit_us: 16_000,
        },
        KeyMode::K9 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 50_000,
            good_us: 117_000,
            bad_fast_us: 183_000,
            bad_slow_us: 183_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 175_000,
            mine_hit_us: 16_000,
        },
        KeyMode::K7 | KeyMode::K14 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 60_000,
            good_us: 150_000,
            bad_fast_us: 220_000,
            bad_slow_us: 280_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 150_000,
            mine_hit_us: 16_000,
        },
        // beatoraja `Beatoraja_Other` uses SEVENKEYS rules.
        KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => JudgeWindow {
            pgreat_us: 20_000,
            great_us: 60_000,
            good_us: 150_000,
            bad_fast_us: 220_000,
            bad_slow_us: 280_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 150_000,
            mine_hit_us: 16_000,
        },
    }
}

pub const fn beatoraja_scratch_judge_window_for_keymode(key_mode: KeyMode) -> JudgeWindow {
    match key_mode {
        KeyMode::K5 | KeyMode::K10 => JudgeWindow {
            pgreat_us: 30_000,
            great_us: 60_000,
            good_us: 110_000,
            bad_fast_us: 160_000,
            bad_slow_us: 160_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 160_000,
            mine_hit_us: 16_000,
        },
        KeyMode::K9 => beatoraja_note_judge_window_for_keymode(key_mode),
        KeyMode::K7 | KeyMode::K14 | KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => JudgeWindow {
            pgreat_us: 30_000,
            great_us: 70_000,
            good_us: 160_000,
            bad_fast_us: 230_000,
            bad_slow_us: 290_000,
            empty_poor_fast_us: 500_000,
            empty_poor_slow_us: 160_000,
            mine_hit_us: 16_000,
        },
    }
}

pub const fn beatoraja_long_note_end_judge_window_for_keymode(key_mode: KeyMode) -> JudgeWindow {
    match key_mode {
        KeyMode::K5 | KeyMode::K10 => JudgeWindow {
            pgreat_us: 120_000,
            great_us: 150_000,
            good_us: 200_000,
            bad_fast_us: 250_000,
            bad_slow_us: 250_000,
            empty_poor_fast_us: 0,
            empty_poor_slow_us: 0,
            mine_hit_us: 16_000,
        },
        KeyMode::K9 => JudgeWindow {
            pgreat_us: 120_000,
            great_us: 150_000,
            good_us: 217_000,
            bad_fast_us: 283_000,
            bad_slow_us: 283_000,
            empty_poor_fast_us: 0,
            empty_poor_slow_us: 0,
            mine_hit_us: 16_000,
        },
        KeyMode::K7 | KeyMode::K14 | KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => JudgeWindow {
            pgreat_us: 120_000,
            great_us: 160_000,
            good_us: 200_000,
            bad_fast_us: 220_000,
            bad_slow_us: 280_000,
            empty_poor_fast_us: 0,
            empty_poor_slow_us: 0,
            mine_hit_us: 16_000,
        },
    }
}

pub const fn beatoraja_long_scratch_end_judge_window_for_keymode(key_mode: KeyMode) -> JudgeWindow {
    match key_mode {
        KeyMode::K5 | KeyMode::K10 => JudgeWindow {
            pgreat_us: 130_000,
            great_us: 160_000,
            good_us: 110_000,
            bad_fast_us: 260_000,
            bad_slow_us: 260_000,
            empty_poor_fast_us: 0,
            empty_poor_slow_us: 0,
            mine_hit_us: 16_000,
        },
        KeyMode::K9 => beatoraja_long_note_end_judge_window_for_keymode(key_mode),
        KeyMode::K7 | KeyMode::K14 | KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => JudgeWindow {
            pgreat_us: 130_000,
            great_us: 170_000,
            good_us: 210_000,
            bad_fast_us: 230_000,
            bad_slow_us: 290_000,
            empty_poor_fast_us: 0,
            empty_poor_slow_us: 0,
            mine_hit_us: 16_000,
        },
    }
}

pub const fn beatoraja_judge_windows_for_keymode(key_mode: KeyMode) -> JudgeWindows {
    JudgeWindows {
        note: beatoraja_note_judge_window_for_keymode(key_mode),
        scratch: beatoraja_scratch_judge_window_for_keymode(key_mode),
        long_note_end: beatoraja_long_note_end_judge_window_for_keymode(key_mode),
        long_scratch_end: beatoraja_long_scratch_end_judge_window_for_keymode(key_mode),
    }
}

pub fn beatoraja_judge_window_for_rank_and_keymode(
    base: JudgeWindow,
    percent: i32,
    key_mode: KeyMode,
) -> JudgeWindow {
    let fixjudge = beatoraja_fixjudge_for_keymode(key_mode);
    let fast = beatoraja_create_judge_bands(
        [base.pgreat_us, base.great_us, base.good_us, base.bad_fast_us],
        percent,
        fixjudge,
    );
    let slow = beatoraja_create_judge_bands(
        [base.pgreat_us, base.great_us, base.good_us, base.bad_slow_us],
        percent,
        fixjudge,
    );

    JudgeWindow {
        pgreat_us: fast[0].max(slow[0]),
        great_us: fast[1].max(slow[1]),
        good_us: fast[2].max(slow[2]),
        bad_fast_us: fast[3],
        bad_slow_us: slow[3],
        empty_poor_fast_us: base.empty_poor_fast_us,
        empty_poor_slow_us: base.empty_poor_slow_us,
        mine_hit_us: base.mine_hit_us,
    }
}

fn beatoraja_create_judge_bands(base: [i64; 4], percent: i32, fixjudge: [bool; 5]) -> [i64; 4] {
    let mut judge = [0; 4];
    for i in 0..judge.len() {
        judge[i] = if fixjudge[i] { base[i] } else { scale_window_us(base[i], percent) };
    }

    let mut fixmin = None;
    for i in 0..judge.len() {
        if fixjudge[i] {
            fixmin = Some(i);
            continue;
        }
        let fixmax = ((i + 1)..judge.len()).find(|&index| fixjudge[index]);
        if let Some(min_index) = fixmin
            && judge[i].abs() < judge[min_index].abs()
        {
            judge[i] = judge[min_index];
        }
        if let Some(max_index) = fixmax
            && judge[i].abs() > judge[max_index].abs()
        {
            judge[i] = judge[max_index];
        }
    }

    for i in 0..3 {
        if judge[i].abs() > judge[3].abs() {
            judge[i] = judge[3];
        }
        if i > 0 && judge[i].abs() < judge[i - 1].abs() {
            judge[i] = judge[i - 1];
        }
    }

    judge
}

fn beatoraja_rank_table_for_keymode(key_mode: KeyMode) -> [i32; 5] {
    if key_mode == KeyMode::K9 { BEATORAJA_PMS_JUDGE_RANK } else { BEATORAJA_NORMAL_JUDGE_RANK }
}

fn beatoraja_fixjudge_for_keymode(key_mode: KeyMode) -> [bool; 5] {
    if key_mode == KeyMode::K9 { BEATORAJA_PMS_FIX_JUDGE } else { BEATORAJA_NORMAL_FIX_JUDGE }
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

pub const fn lr2oraja_long_note_end_judge_window() -> JudgeWindow {
    JudgeWindow {
        pgreat_us: 120_000,
        great_us: 120_000,
        good_us: 120_000,
        bad_fast_us: 200_000,
        bad_slow_us: 200_000,
        empty_poor_fast_us: 0,
        empty_poor_slow_us: 0,
        mine_hit_us: 16_000,
    }
}

pub const fn lr2oraja_judge_windows() -> JudgeWindows {
    JudgeWindows {
        note: lr2oraja_note_judge_window(),
        scratch: lr2oraja_note_judge_window(),
        long_note_end: lr2oraja_long_note_end_judge_window(),
        long_scratch_end: lr2oraja_long_note_end_judge_window(),
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

pub const fn dx_long_note_end_judge_window() -> JudgeWindow {
    JudgeWindow {
        pgreat_us: 116_666,
        great_us: 116_666,
        good_us: 116_666,
        bad_fast_us: 200_000,
        bad_slow_us: 200_000,
        empty_poor_fast_us: 0,
        empty_poor_slow_us: 0,
        mine_hit_us: 16_000,
    }
}

pub const fn dx_judge_windows() -> JudgeWindows {
    JudgeWindows {
        note: dx_note_judge_window(),
        scratch: dx_note_judge_window(),
        long_note_end: dx_long_note_end_judge_window(),
        long_scratch_end: dx_long_note_end_judge_window(),
    }
}

/// 譜面ヘッダ `#RANK` と `#EXRANK` イベントから、指定時刻の判定倍率 (%) を求める。
pub fn judge_percent_at_time(
    header_rank: Option<JudgeRankSpec>,
    events: &[JudgeRankEvent],
    now: bmz_core::time::TimeUs,
    rule_mode: RuleMode,
) -> i32 {
    judge_percent_at_time_for_keymode(header_rank, events, now, KeyMode::K7, rule_mode)
}

pub fn judge_percent_at_time_for_keymode(
    header_rank: Option<JudgeRankSpec>,
    events: &[JudgeRankEvent],
    now: bmz_core::time::TimeUs,
    key_mode: KeyMode,
    rule_mode: RuleMode,
) -> i32 {
    let mut percent = judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
        header_rank,
        key_mode,
        rule_mode,
    );
    if rule_mode == RuleMode::Dx {
        // DX MODE uses JudgeProperty.IIDX with fixed windows; rank headers/events are ignored.
        return 100;
    }
    if matches!(rule_mode, RuleMode::Beatoraja | RuleMode::Lr2Oraja) {
        // Compatibility: beatoraja/LR2oraja keep #EXRANK/A0 out of the runtime rank path.
        // BMZ still imports those events, but does not apply them to compatible modes.
        return percent;
    }
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
    let mut pgreat_us = lr2_scale_window_us(base.pgreat_us, percent);
    let mut great_us = lr2_scale_window_us(base.great_us, percent);
    let mut good_us = lr2_scale_window_us(base.good_us, percent);

    if good_us.abs() > base.bad_fast_us.max(base.bad_slow_us).abs() {
        good_us = base.bad_fast_us.max(base.bad_slow_us);
    }
    if great_us.abs() > good_us.abs() {
        great_us = good_us;
    }
    if pgreat_us.abs() > great_us.abs() {
        pgreat_us = great_us;
    }

    JudgeWindow {
        pgreat_us,
        great_us,
        good_us,
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

fn lr2_scale_window_us(base: i64, percent: i32) -> i64 {
    if percent >= 100 {
        return scale_window_us(base, percent);
    }

    let sign = base.signum();
    let base = base.abs();
    let rank = percent.max(0);
    let last = LR2_SCALING[0].len() - 1;
    let judge_index = (rank / 25).clamp(0, 3) as usize;
    let mut row = 0;
    while row < LR2_SCALING.len() && base >= LR2_SCALING[row][last] {
        row += 1;
    }

    let (d, x1, x2) = if row < LR2_SCALING.len() {
        let n = base - LR2_SCALING[row - 1][last];
        let d = LR2_SCALING[row][last] - LR2_SCALING[row - 1][last];
        let x1 = d * LR2_SCALING[row - 1][judge_index]
            + n * (LR2_SCALING[row][judge_index] - LR2_SCALING[row - 1][judge_index]);
        let x2 = d * LR2_SCALING[row - 1][judge_index + 1]
            + n * (LR2_SCALING[row][judge_index + 1] - LR2_SCALING[row - 1][judge_index + 1]);
        (d, x1, x2)
    } else {
        let n = base;
        let d = LR2_SCALING[row - 1][last];
        let x1 = n * LR2_SCALING[row - 1][judge_index];
        let x2 = n * LR2_SCALING[row - 1][judge_index + 1];
        (d, x1, x2)
    };

    let low_rank = (judge_index as i32) * 25;
    let scaled = (x1 + (rank - low_rank) as i64 * (x2 - x1) / 25) / d;
    sign * scaled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_window() -> JudgeWindow {
        JudgeWindow::symmetric(16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000)
    }

    fn rank_spec(value: i32, kind: JudgeRankKind) -> JudgeRankSpec {
        JudgeRankSpec { value, kind }
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
    fn beatoraja_pms_rank_levels_follow_pms_table() {
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(0, KeyMode::K9), 33);
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(1, KeyMode::K9), 50);
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(2, KeyMode::K9), 70);
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(3, KeyMode::K9), 100);
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(4, KeyMode::K9), 133);
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(120, KeyMode::K9), 120);
        assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(-1, KeyMode::K9), 70);
    }

    #[test]
    fn beatoraja_rank_specs_follow_validate_rules() {
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
                Some(rank_spec(100, JudgeRankKind::DefExRank)),
                KeyMode::K7,
                RuleMode::Beatoraja,
            ),
            75
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
                Some(rank_spec(100, JudgeRankKind::DefExRank)),
                KeyMode::K9,
                RuleMode::Beatoraja,
            ),
            70
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
                Some(rank_spec(125, JudgeRankKind::DefExRank)),
                KeyMode::K9,
                RuleMode::Beatoraja,
            ),
            87
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
                Some(rank_spec(120, JudgeRankKind::BmsonJudgeRank)),
                KeyMode::K9,
                RuleMode::Beatoraja,
            ),
            120
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
                Some(rank_spec(0, JudgeRankKind::BmsonJudgeRank)),
                KeyMode::K9,
                RuleMode::Beatoraja,
            ),
            100
        );
    }

    #[test]
    fn lr2oraja_rank_levels_follow_lr2_fallbacks() {
        assert_eq!(judge_rank_to_percent_optional_for_rule_mode(None, RuleMode::Lr2Oraja), 75);
        assert_eq!(judge_rank_to_percent_for_rule_mode(0, RuleMode::Lr2Oraja), 25);
        assert_eq!(judge_rank_to_percent_for_rule_mode(1, RuleMode::Lr2Oraja), 50);
        assert_eq!(judge_rank_to_percent_for_rule_mode(2, RuleMode::Lr2Oraja), 75);
        assert_eq!(judge_rank_to_percent_for_rule_mode(3, RuleMode::Lr2Oraja), 100);
        assert_eq!(judge_rank_to_percent_for_rule_mode(4, RuleMode::Lr2Oraja), 75);
    }

    #[test]
    fn lr2oraja_defexrank_scales_against_normal_rank() {
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_rule_mode(
                Some(rank_spec(100, JudgeRankKind::DefExRank)),
                RuleMode::Lr2Oraja,
            ),
            75
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_rule_mode(
                Some(rank_spec(125, JudgeRankKind::DefExRank)),
                RuleMode::Lr2Oraja,
            ),
            93
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_rule_mode(
                Some(rank_spec(0, JudgeRankKind::DefExRank)),
                RuleMode::Lr2Oraja,
            ),
            75
        );
    }

    #[test]
    fn lr2oraja_bmson_rank_uses_raw_percent() {
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_rule_mode(
                Some(rank_spec(100, JudgeRankKind::BmsonJudgeRank)),
                RuleMode::Lr2Oraja,
            ),
            100
        );
        assert_eq!(
            judge_rank_spec_to_percent_optional_for_rule_mode(
                Some(rank_spec(0, JudgeRankKind::BmsonJudgeRank)),
                RuleMode::Lr2Oraja,
            ),
            100
        );
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
    fn beatoraja_pms_scaling_keeps_fixed_pgreat_and_bad() {
        let base = beatoraja_note_judge_window_for_keymode(KeyMode::K9);
        let window =
            judge_window_for_rule_mode_and_keymode(base, 70, RuleMode::Beatoraja, KeyMode::K9);

        assert_eq!(window.pgreat_us, 20_000);
        assert_eq!(window.great_us, 35_000);
        assert_eq!(window.good_us, 81_900);
        assert_eq!(window.bad_fast_us, 183_000);
        assert_eq!(window.bad_slow_us, 183_000);
        assert_eq!(window.empty_poor_fast_us, 500_000);
        assert_eq!(window.empty_poor_slow_us, 175_000);
    }

    #[test]
    fn beatoraja_pms_very_hard_clamps_great_to_fixed_pgreat() {
        let base = beatoraja_note_judge_window_for_keymode(KeyMode::K9);
        let window =
            judge_window_for_rule_mode_and_keymode(base, 33, RuleMode::Beatoraja, KeyMode::K9);

        assert_eq!(window.pgreat_us, 20_000);
        assert_eq!(window.great_us, 20_000);
        assert_eq!(window.good_us, 38_610);
        assert_eq!(window.bad_fast_us, 183_000);
    }

    #[test]
    fn beatoraja_normal_rule_keeps_judge_windows_monotonic() {
        let base = beatoraja_long_scratch_end_judge_window_for_keymode(KeyMode::K5);
        let window =
            judge_window_for_rule_mode_and_keymode(base, 100, RuleMode::Beatoraja, KeyMode::K5);

        assert_eq!(window.pgreat_us, 130_000);
        assert_eq!(window.great_us, 160_000);
        assert_eq!(window.good_us, 160_000);
        assert_eq!(window.bad_fast_us, 260_000);
    }

    #[test]
    fn beatoraja_7k_note_window_uses_asymmetric_bad_and_empty_poor() {
        let window = beatoraja_note_judge_window_for_keymode(KeyMode::K7);
        assert_eq!(window.pgreat_us, 20_000);
        assert_eq!(window.great_us, 60_000);
        assert_eq!(window.good_us, 150_000);
        assert_eq!(window.bad_fast_us, 220_000);
        assert_eq!(window.bad_slow_us, 280_000);
        assert_eq!(window.empty_poor_fast_us, 500_000);
        assert_eq!(window.empty_poor_slow_us, 150_000);
    }

    #[test]
    fn beatoraja_other_keymodes_use_7k_empty_poor_window() {
        let seven = beatoraja_note_judge_window_for_keymode(KeyMode::K7);
        assert_eq!(beatoraja_note_judge_window_for_keymode(KeyMode::K4), seven);
        assert_eq!(beatoraja_note_judge_window_for_keymode(KeyMode::K6), seven);
        assert_eq!(beatoraja_note_judge_window_for_keymode(KeyMode::K8), seven);
    }

    #[test]
    fn beatoraja_7k_scratch_window_uses_scratch_table() {
        let window = beatoraja_scratch_judge_window_for_keymode(KeyMode::K7);

        assert_eq!(window.pgreat_us, 30_000);
        assert_eq!(window.great_us, 70_000);
        assert_eq!(window.good_us, 160_000);
        assert_eq!(window.bad_fast_us, 230_000);
        assert_eq!(window.bad_slow_us, 290_000);
        assert_eq!(window.empty_poor_fast_us, 500_000);
        assert_eq!(window.empty_poor_slow_us, 160_000);
    }

    #[test]
    fn beatoraja_long_note_end_windows_use_long_tables() {
        let five = beatoraja_long_note_end_judge_window_for_keymode(KeyMode::K5);
        assert_eq!(five.pgreat_us, 120_000);
        assert_eq!(five.great_us, 150_000);
        assert_eq!(five.good_us, 200_000);
        assert_eq!(five.bad_fast_us, 250_000);
        assert_eq!(five.bad_slow_us, 250_000);

        let seven_scratch = beatoraja_long_scratch_end_judge_window_for_keymode(KeyMode::K7);
        assert_eq!(seven_scratch.pgreat_us, 130_000);
        assert_eq!(seven_scratch.great_us, 170_000);
        assert_eq!(seven_scratch.good_us, 210_000);
        assert_eq!(seven_scratch.bad_fast_us, 230_000);
        assert_eq!(seven_scratch.bad_slow_us, 290_000);
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
    fn lr2oraja_default_rank_scales_note_and_long_end_windows() {
        let base = lr2oraja_judge_windows();
        let window = judge_windows_for_rule_mode(base, 75, RuleMode::Lr2Oraja);

        assert_eq!(window.note.pgreat_us, 18_000);
        assert_eq!(window.note.great_us, 40_000);
        assert_eq!(window.note.good_us, 100_000);
        assert_eq!(window.note.bad_fast_us, 200_000);
        assert_eq!(window.note.empty_poor_fast_us, 1_000_000);

        assert_eq!(window.long_note_end.pgreat_us, 100_000);
        assert_eq!(window.long_note_end.great_us, 100_000);
        assert_eq!(window.long_note_end.good_us, 100_000);
        assert_eq!(window.long_note_end.bad_fast_us, 200_000);
        assert_eq!(window.long_note_end.empty_poor_fast_us, 0);
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
    fn dx_mode_uses_iidx_long_note_end_window() {
        let windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K7, RuleMode::Dx);

        assert_eq!(windows.note.pgreat_us, 16_666);
        assert_eq!(windows.note.great_us, 33_333);
        assert_eq!(windows.note.good_us, 116_666);
        assert_eq!(windows.scratch, windows.note);
        assert_eq!(windows.long_note_end.pgreat_us, 116_666);
        assert_eq!(windows.long_note_end.great_us, 116_666);
        assert_eq!(windows.long_note_end.good_us, 116_666);
        assert_eq!(windows.long_note_end.bad_fast_us, 200_000);
        assert_eq!(windows.long_note_end.empty_poor_fast_us, 0);
        assert_eq!(windows.long_scratch_end, windows.long_note_end);
    }

    #[test]
    fn dx_ignores_rank_and_exrank_events() {
        use bmz_chart::model::JudgeRankEvent;
        use bmz_core::time::TimeUs;

        let events = vec![
            JudgeRankEvent { tick: Default::default(), time: TimeUs(1_000), rank_percent: 50 },
            JudgeRankEvent { tick: Default::default(), time: TimeUs(2_000), rank_percent: 25 },
        ];
        let header = Some(rank_spec(0, JudgeRankKind::BmsRank));
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(0), RuleMode::Dx), 100);
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(1_500), RuleMode::Dx), 100);
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(2_500), RuleMode::Dx), 100);
    }

    #[test]
    fn beatoraja_ignores_exrank_events() {
        use bmz_chart::model::JudgeRankEvent;
        use bmz_core::time::TimeUs;

        let events = vec![JudgeRankEvent {
            tick: Default::default(),
            time: TimeUs(1_000),
            rank_percent: 25,
        }];
        let header = Some(rank_spec(3, JudgeRankKind::BmsRank));
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(0), RuleMode::Beatoraja), 100);
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(1_500), RuleMode::Beatoraja), 100);
    }

    #[test]
    fn beatoraja_pms_percent_at_time_uses_pms_header_and_ignores_events() {
        use bmz_chart::model::JudgeRankEvent;
        use bmz_core::time::TimeUs;

        let events = vec![JudgeRankEvent {
            tick: Default::default(),
            time: TimeUs(1_000),
            rank_percent: 25,
        }];
        let header = Some(rank_spec(2, JudgeRankKind::BmsRank));
        assert_eq!(
            judge_percent_at_time_for_keymode(
                header,
                &events,
                TimeUs(0),
                KeyMode::K9,
                RuleMode::Beatoraja,
            ),
            70
        );
        assert_eq!(
            judge_percent_at_time_for_keymode(
                header,
                &events,
                TimeUs(1_500),
                KeyMode::K9,
                RuleMode::Beatoraja,
            ),
            70
        );
    }

    #[test]
    fn lr2oraja_ignores_exrank_events() {
        use bmz_chart::model::JudgeRankEvent;
        use bmz_core::time::TimeUs;

        let events = vec![JudgeRankEvent {
            tick: Default::default(),
            time: TimeUs(1_000),
            rank_percent: 125,
        }];
        let header = Some(rank_spec(3, JudgeRankKind::BmsRank));
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(0), RuleMode::Lr2Oraja), 100);
        assert_eq!(judge_percent_at_time(header, &events, TimeUs(1_500), RuleMode::Lr2Oraja), 100);
    }
}
