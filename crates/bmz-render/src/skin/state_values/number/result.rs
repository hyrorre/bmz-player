use super::*;

pub(in crate::skin) fn ir_ranking_entry(
    ranking: &crate::scene::ResultIrSnapshot,
    index: i32,
) -> Option<crate::scene::ResultIrRankingEntrySnapshot> {
    ranking.entries.get(usize::try_from(index).ok()?).copied().filter(|entry| {
        entry.rank.is_some() || entry.ex_score.is_some() || !entry.player_name.as_str().is_empty()
    })
}

pub(in crate::skin) fn ir_ranking_score_and_max(
    state: &SkinDrawState,
    slot: i32,
) -> Option<(i64, i64)> {
    if !(1..=10).contains(&slot) {
        return None;
    }
    let score = ir_ranking_entry(&state.ir_ranking, slot - 1)?.ex_score?;
    let max_score = i64::from(state.select_total_notes.max(state.total_notes).checked_mul(2)?);
    Some((score, max_score))
}

pub(in crate::skin) fn ir_ranking_score_rate_parts(
    state: &SkinDrawState,
    slot: i32,
) -> Option<(i64, i64)> {
    let (score, max_score) = ir_ranking_score_and_max(state, slot)?;
    if score <= 0 || max_score <= 0 {
        return Some((0, 0));
    }
    let scaled = i128::from(score).checked_mul(10_000)?.checked_div(i128::from(max_score))?;
    Some((i64::try_from(scaled / 100).ok()?, i64::try_from(scaled % 100).ok()?))
}

pub(in crate::skin) fn ir_clear_stat_number(
    ranking: &crate::scene::ResultIrSnapshot,
    ref_id: i32,
) -> Option<i64> {
    if ranking.state != crate::scene::ResultIrState::Loaded {
        return None;
    }
    // Legacy providers can supply an integer rate without the population.
    // Never reconstruct exact counts or a fractional digit from that rounded rate.
    if ref_id == 227 && ranking.clear_counts.is_none() {
        return ranking.clear_rate;
    }
    let counts = ranking.clear_counts?;
    let (count, part) = match ref_id {
        202..=219 | 222..=225 => {
            let index = match ref_id & !1 {
                202 => 0,
                210 => 1,
                204 => 2,
                206 => 3,
                212 => 4,
                214 => 5,
                216 => 6,
                208 => 7,
                218 => 8,
                222 => 9,
                224 => 10,
                _ => return None,
            };
            (i64::from(counts[index]), ref_id % 2)
        }
        226 | 227 | 241 => (
            counts[2..].iter().map(|&count| i64::from(count)).sum(),
            if ref_id == 241 { 2 } else { ref_id - 226 },
        ),
        228 | 229 | 242 => (
            counts[8..].iter().map(|&count| i64::from(count)).sum(),
            if ref_id == 242 { 2 } else { ref_id - 228 },
        ),
        230..=240 => {
            let index = [0, 2, 3, 7, 1, 4, 5, 6, 8, 9, 10][(ref_id - 230) as usize];
            (i64::from(counts[index]), 2)
        }
        _ => return None,
    };
    if part == 0 {
        return Some(count);
    }
    let total: i64 = counts.iter().map(|&count| i64::from(count)).sum();
    if total == 0 {
        return None;
    }
    Some(if part == 1 { count * 100 / total } else { count * 1_000 / total % 10 })
}

pub(in crate::skin) fn result_grade_diff_number(state: &SkinDrawState) -> Option<i64> {
    // beatoraja NUMBER_DIFF_NEXTRANK (154) is the positive distance to the
    // next DJ LEVEL border. Keep the signed BMZ extension 1978 separate.
    Some(score_grade_facts(state)?.next_diff.saturating_neg())
}

pub(crate) fn result_grade_diff_label(state: &SkinDrawState) -> Option<String> {
    let facts = score_grade_facts(state)?;
    Some(format!("{}{:+}", facts.next_label(), facts.next_diff))
}

pub(in crate::skin) fn grade_diff_score_available(state: &SkinDrawState) -> bool {
    !state.select_screen
        || state.select_play_count > 0
        || state.select_ex_score.is_some_and(|score| score > 0)
}

const SCORE_GRADE_LABELS: [&str; 9] = ["F", "E", "D", "C", "B", "A", "AA", "AAA", "MAX"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::skin) struct ScoreGradeFacts {
    pub(in crate::skin) current_index: usize,
    pub(in crate::skin) next_index: usize,
    pub(in crate::skin) nearest_index: usize,
    pub(in crate::skin) current_diff: i64,
    pub(in crate::skin) next_diff: i64,
    pub(in crate::skin) nearest_diff: i64,
    pub(in crate::skin) nearest_tie: bool,
}

impl ScoreGradeFacts {
    fn new(ex_score: u32, total_notes: u32) -> Option<Self> {
        let max_score = i64::from(total_notes).checked_mul(2)?;
        if max_score <= 0 {
            return None;
        }
        let score = i64::from(ex_score).clamp(0, max_score);
        let mut borders = [0_i64; SCORE_GRADE_LABELS.len()];
        for numerator in 2_i64..=8 {
            borders[(numerator - 1) as usize] = div_ceil(max_score * numerator, 9);
        }
        borders[8] = max_score;

        // Tiny charts can have duplicate integer borders. CURRENT takes the
        // highest grade already reached and NEXT skips every equal border.
        let current_index = borders.iter().rposition(|&border| border <= score)?;
        let next_index = borders.iter().position(|&border| border > score).unwrap_or(8);
        let current_diff = score - borders[current_index];
        let next_distance = borders[next_index] - score;
        let next_diff = -next_distance;
        let nearest_tie = current_index != next_index && current_diff == next_distance;
        let nearest_index = if current_diff <= next_distance { current_index } else { next_index };
        let nearest_diff = if nearest_index == current_index { current_diff } else { next_diff };

        Some(Self {
            current_index,
            next_index,
            nearest_index,
            current_diff,
            next_diff,
            nearest_diff,
            nearest_tie,
        })
    }

    pub(in crate::skin) fn current_label(self) -> &'static str {
        SCORE_GRADE_LABELS[self.current_index]
    }

    pub(in crate::skin) fn next_label(self) -> &'static str {
        SCORE_GRADE_LABELS[self.next_index]
    }

    pub(in crate::skin) fn nearest_label(self) -> &'static str {
        SCORE_GRADE_LABELS[self.nearest_index]
    }

    pub(in crate::skin) fn nearest_is_current(self) -> bool {
        self.nearest_index == self.current_index
    }

    pub(in crate::skin) fn nearest_is_next(self) -> bool {
        self.nearest_index == self.next_index && self.current_index != self.next_index
    }
}

pub(in crate::skin) fn score_grade_facts(state: &SkinDrawState) -> Option<ScoreGradeFacts> {
    if !grade_diff_score_available(state) {
        return None;
    }
    if state.select_screen {
        ScoreGradeFacts::new(state.select_ex_score.unwrap_or_default(), state.select_total_notes)
    } else {
        ScoreGradeFacts::new(state.ex_score, state.total_notes)
    }
}

/// Computes the forward difference used by WMII PLAY's Lua `next_rank_info`.
/// It differs from BMZ's generic nearest-rank display because WMII always
/// targets the next higher boundary and can optionally add a MAX- boundary.
pub(in crate::skin) fn wmii_next_rank_diff(state: &SkinDrawState) -> Option<i64> {
    wmii_next_rank_diff_with_max_minus(state, true)
}

pub(in crate::skin) fn wmii_next_rank_diff_with_max_minus(
    state: &SkinDrawState,
    include_max_minus: bool,
) -> Option<i64> {
    let ex_score = i64::from(state.ex_score);
    let total_notes = i64::from(state.total_notes);
    let max_score = total_notes.checked_mul(2)?;
    if max_score <= 0 {
        return None;
    }
    let ex_score = ex_score.clamp(0, max_score);
    for (numerator, denominator) in
        [(6_i64, 27_i64), (9, 27), (12, 27), (15, 27), (18, 27), (21, 27), (24, 27)]
    {
        let threshold = div_ceil(max_score * numerator, denominator);
        if ex_score < threshold {
            return Some(threshold - ex_score);
        }
    }
    if include_max_minus {
        let threshold = div_ceil(max_score * 17, 18);
        if ex_score < threshold {
            return Some(threshold - ex_score);
        }
    }
    if ex_score < max_score {
        return Some(max_score - ex_score);
    }
    Some(0)
}

pub(in crate::skin) fn wmii_next_rank_stage(state: &SkinDrawState) -> Option<i32> {
    wmii_next_rank_stage_with_max_minus(state, true)
}

pub(in crate::skin) fn wmii_next_rank_stage_with_max_minus(
    state: &SkinDrawState,
    include_max_minus: bool,
) -> Option<i32> {
    let ex_score = i64::from(state.ex_score);
    let total_notes = i64::from(state.total_notes);
    let max_score = total_notes.checked_mul(2)?;
    if max_score <= 0 {
        return None;
    }
    let ex_score = ex_score.clamp(0, max_score);
    for (numerator, denominator, stage) in [
        (6_i64, 27_i64, 7_i32),
        (9, 27, 6),
        (12, 27, 5),
        (15, 27, 4),
        (18, 27, 3),
        (21, 27, 2),
        (24, 27, 1),
    ] {
        let threshold = div_ceil(max_score * numerator, denominator);
        if ex_score < threshold {
            return Some(stage);
        }
    }
    if include_max_minus {
        let threshold = div_ceil(max_score * 17, 18);
        if ex_score < threshold {
            return Some(8);
        }
    }
    Some(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::skin) struct NearestGradeDiff {
    pub(in crate::skin) grade: &'static str,
    pub(in crate::skin) value: i64,
}

pub(in crate::skin) fn nearest_grade_diff(state: &SkinDrawState) -> Option<NearestGradeDiff> {
    let facts = score_grade_facts(state)?;
    Some(NearestGradeDiff { grade: facts.nearest_label(), value: facts.nearest_diff })
}

pub(in crate::skin) fn projected_score_at_progress(final_score: u32, state: &SkinDrawState) -> u32 {
    if state.total_notes == 0 {
        return final_score;
    }
    let past_notes = state.past_notes.min(state.total_notes);
    ((final_score as u64 * past_notes as u64) / state.total_notes as u64) as u32
}

pub(in crate::skin) fn result_or_select_length_ms(state: &SkinDrawState) -> i64 {
    if state.result_failed.is_some() {
        if state.result_duration_ms > 0 {
            state.result_duration_ms as i64
        } else {
            state.total_duration_ms.max(0) as i64
        }
    } else {
        state.select_length_ms.max(0)
    }
}

pub(in crate::skin) fn projected_best_score_at_progress(state: &SkinDrawState) -> Option<u32> {
    if state.result_failed.is_some() {
        return result_mybest_ex_score_display(state);
    }
    if state.play_screen {
        if state.practice_mode {
            return Some(0);
        }
        return state.projected_best_ex_score.or_else(|| {
            state.best_ex_score.map(|score| projected_score_at_progress(score, state)).or(Some(0))
        });
    }
    state.projected_best_ex_score.or_else(|| {
        result_mybest_ex_score(state).map(|score| projected_score_at_progress(score, state))
    })
}

/// beatoraja NUMBER_HIGHSCORE / NUMBER_HIGHSCORE2.
///
/// Playでは保存済みの最終EXスコアを進捗で投影せず返す。未プレイは空の
/// ScoreDataと同じ0。Resultでは保存前のベストを使い、初回は同じく0。
pub(in crate::skin) fn best_score_number(state: &SkinDrawState) -> Option<u32> {
    if state.result_failed.is_some() {
        result_mybest_ex_score_display(state)
    } else if state.play_screen {
        Some(if state.practice_mode { 0 } else { state.best_ex_score.unwrap_or(0) })
    } else {
        state.best_ex_score
    }
}

pub(in crate::skin) fn div_ceil(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        return 0;
    }
    numerator.div_euclid(denominator) + i64::from(numerator.rem_euclid(denominator) != 0)
}

pub(in crate::skin) fn rank_threshold(max_score: u32, rank_step: u32) -> u32 {
    div_ceil(rank_step as i64 * max_score as i64, 27).clamp(0, u32::MAX as i64) as u32
}

pub(in crate::skin) fn judge_rank_option_matches(op: i32, judge_rank: Option<i32>) -> bool {
    let Some(rank) = judge_rank else {
        return op == 182;
    };
    match op {
        180 => rank == 0 || (10..35).contains(&rank),
        181 => rank == 1 || (35..60).contains(&rank),
        182 => rank == 2 || (60..85).contains(&rank),
        183 => rank == 3 || (85..110).contains(&rank),
        184 => rank == 4 || rank >= 110,
        _ => false,
    }
}

pub(in crate::skin) fn judge_rate_int(count: u32, total_notes: u32) -> Option<i64> {
    if total_notes == 0 {
        return None;
    }
    Some(count as i64 * 100 / total_notes as i64)
}

pub(in crate::skin) fn poor_plus_miss(counts: DisplayJudgeCounts) -> u32 {
    counts.poor.saturating_add(counts.empty_poor)
}

pub(in crate::skin) fn bad_plus_poor_plus_miss(counts: DisplayJudgeCounts) -> u32 {
    counts.bad.saturating_add(poor_plus_miss(counts))
}

pub(in crate::skin) fn score_rate_parts(ex_score: u32, total_notes: u32) -> (u32, u32) {
    if total_notes == 0 {
        return (0, 0);
    }
    // beatoraja ScoreDataProperty: rateInt=(int)(rate*100), rateAfterDot=((int)(rate*10000))%100
    let rate_scaled = ex_score.saturating_mul(10000) / total_notes.saturating_mul(2).max(1);
    (rate_scaled / 100, rate_scaled % 100)
}

pub(in crate::skin) fn current_score_rate_notes(state: &SkinDrawState) -> Option<u32> {
    if state.past_notes > 0 {
        Some(state.past_notes)
    } else if state.total_notes > 0 || state.select_total_notes > 0 {
        Some(0)
    } else {
        None
    }
}

pub(in crate::skin) fn current_score_rate_value(state: &SkinDrawState) -> f32 {
    match current_score_rate_notes(state) {
        Some(0) => 1.0,
        Some(notes) => state.ex_score as f32 / notes.saturating_mul(2).max(1) as f32,
        None => 0.0,
    }
}

pub(in crate::skin) fn current_score_rate_parts(state: &SkinDrawState) -> (u32, u32) {
    match current_score_rate_notes(state) {
        Some(0) => (100, 0),
        Some(notes) => score_rate_parts(state.ex_score, notes),
        None => (0, 0),
    }
}
