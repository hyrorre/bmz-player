use super::*;

pub(super) fn best_rank_op_matches(op: i32, state: &SkinDrawState) -> bool {
    if state.in_settings {
        return false;
    }
    let score = if state.result_failed.is_some() {
        result_mybest_ex_score_display(state)
    } else {
        result_mybest_ex_score(state)
    };
    let Some(rank) = rank_index(score, state.total_notes) else {
        return false;
    };
    op == 320 + rank as i32
}

/// 現在のランク判定の基準値 (ex_score, notes)。
/// Play 画面では beatoraja の `qualifyNowRank` と同じく past notes を分母にする。
pub(super) fn current_rank_inputs(state: &SkinDrawState) -> (Option<u32>, u32) {
    if state.result_failed.is_some() {
        (Some(state.ex_score), state.total_notes)
    } else if state.select_screen {
        (state.select_ex_score, state.select_total_notes)
    } else if let Some(notes) = current_score_rate_notes(state) {
        (Some(state.ex_score), notes)
    } else {
        (Some(state.ex_score), state.total_notes)
    }
}

pub(super) fn current_rank_index(state: &SkinDrawState) -> Option<usize> {
    let (ex_score, total_notes) = current_rank_inputs(state);
    if !state.select_screen
        && state.result_failed.is_none()
        && total_notes == 0
        && current_score_rate_notes(state) == Some(0)
    {
        return rank_index(Some(2), 1);
    }
    rank_index(ex_score, total_notes)
}

pub(super) fn rank_index(ex_score: Option<u32>, total_notes: u32) -> Option<usize> {
    bmz_core::score::score_rank_index(ex_score?, total_notes)
}
