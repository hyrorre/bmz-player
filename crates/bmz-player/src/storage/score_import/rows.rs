use super::*;

#[derive(Debug)]
pub(super) struct Lr2ScoreRow {
    pub(super) md5: String,
    pub(super) clear: i64,
    pub(super) perfect: u32,
    pub(super) great: u32,
    pub(super) good: u32,
    pub(super) bad: u32,
    pub(super) poor: u32,
    pub(super) total_notes: u32,
    pub(super) max_combo: u32,
    pub(super) min_bp: Option<i64>,
    pub(super) play_count: u32,
    pub(super) clear_count: u32,
    pub(super) ghost: String,
    pub(super) random_seed: Option<i64>,
    pub(super) op_best: i64,
}

pub(super) fn lr2_row(row: &Row<'_>) -> rusqlite::Result<Lr2ScoreRow> {
    Ok(Lr2ScoreRow {
        md5: row.get(0)?,
        clear: row.get(1)?,
        perfect: row.get(2)?,
        great: row.get(3)?,
        good: row.get(4)?,
        bad: row.get(5)?,
        poor: row.get(6)?,
        total_notes: row.get(7)?,
        max_combo: row.get(8)?,
        min_bp: row.get(9)?,
        play_count: row.get(10)?,
        clear_count: row.get(11)?,
        ghost: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
        random_seed: row.get(13)?,
        op_best: row.get(14)?,
    })
}

#[derive(Debug)]
pub(super) struct BeatorajaScoreRow {
    pub(super) sha256: String,
    pub(super) mode: i64,
    pub(super) clear: i64,
    pub(super) epg: u32,
    pub(super) lpg: u32,
    pub(super) egr: u32,
    pub(super) lgr: u32,
    pub(super) egd: u32,
    pub(super) lgd: u32,
    pub(super) ebd: u32,
    pub(super) lbd: u32,
    pub(super) epr: u32,
    pub(super) lpr: u32,
    pub(super) ems: u32,
    pub(super) lms: u32,
    pub(super) total_notes: u32,
    pub(super) max_combo: u32,
    pub(super) min_bp: Option<i64>,
    pub(super) ghost: String,
    pub(super) random_seed: Option<i64>,
    pub(super) date: i64,
    pub(super) option: i64,
}

pub(super) fn beatoraja_row(row: &Row<'_>) -> rusqlite::Result<BeatorajaScoreRow> {
    Ok(BeatorajaScoreRow {
        sha256: row.get(0)?,
        mode: row.get(1)?,
        clear: row.get(2)?,
        epg: row.get(3)?,
        lpg: row.get(4)?,
        egr: row.get(5)?,
        lgr: row.get(6)?,
        egd: row.get(7)?,
        lgd: row.get(8)?,
        ebd: row.get(9)?,
        lbd: row.get(10)?,
        epr: row.get(11)?,
        lpr: row.get(12)?,
        ems: row.get(13)?,
        lms: row.get(14)?,
        total_notes: row.get(15)?,
        max_combo: row.get(16)?,
        min_bp: row.get(17)?,
        ghost: row.get::<_, Option<String>>(18)?.unwrap_or_default(),
        random_seed: row.get(19)?,
        date: row.get(20)?,
        option: row.get(21)?,
    })
}

pub(super) fn score_state_from_lr2(row: &Lr2ScoreRow, expected_notes: u32) -> ScoreState {
    let ghost = decode_lr2_ghost(&row.ghost, expected_notes);
    let _ = (row.play_count, row.clear_count, row.total_notes);
    ScoreState {
        judges: JudgeCounts {
            fast_pgreat: row.perfect,
            fast_great: row.great,
            fast_good: row.good,
            fast_bad: row.bad,
            fast_poor: row.poor,
            ..Default::default()
        },
        combo: 0,
        max_combo: row.max_combo,
        past_notes: expected_notes,
        ghost,
        empty_poor_breaks_combo: false,
    }
}

pub(super) fn score_state_from_beatoraja(
    row: &BeatorajaScoreRow,
    expected_notes: u32,
) -> ScoreState {
    let ghost = decode_external_ghost(&row.ghost, expected_notes);
    let _ = row.total_notes;
    ScoreState {
        judges: JudgeCounts {
            fast_pgreat: row.epg,
            slow_pgreat: row.lpg,
            fast_great: row.egr,
            slow_great: row.lgr,
            fast_good: row.egd,
            slow_good: row.lgd,
            fast_bad: row.ebd,
            slow_bad: row.lbd,
            fast_poor: row.epr,
            slow_poor: row.lpr,
            fast_empty_poor: row.ems,
            slow_empty_poor: row.lms,
        },
        combo: 0,
        max_combo: row.max_combo,
        past_notes: expected_notes,
        ghost,
        empty_poor_breaks_combo: false,
    }
}
