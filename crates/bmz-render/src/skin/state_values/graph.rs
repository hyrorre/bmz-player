use super::*;

/// Returns the graph bar fill ratio (0.0-1.0) for a given `BARGRAPH_*` type.
pub(super) fn graph_value(graph_type: i32, state: &SkinDrawState) -> f32 {
    match graph_type {
        101 => state.play_progress, // BARGRAPH_MUSIC_PROGRESS: elapsed / total playtime
        102 => 1.0,                 // BARGRAPH_LOAD_PROGRESS: always complete during play
        110 => {
            // BARGRAPH_SCORERATE: ex_score / max_ex_score
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 { state.ex_score as f32 / max } else { 0.0 }
        }
        111 => current_score_rate_value(state),
        // BARGRAPH_RATE_PGREAT..RATE_EXSCORE: judge count / past_notes (or total_notes)
        140 => judge_rate(state.judge_counts.pgreat, state.past_notes),
        141 => judge_rate(state.judge_counts.great, state.past_notes),
        142 => judge_rate(state.judge_counts.good, state.past_notes),
        143 => judge_rate(state.judge_counts.bad, state.past_notes),
        144 => judge_rate(state.judge_counts.poor, state.past_notes),
        145 => judge_rate(state.max_combo, state.total_notes),
        146 => {
            // BARGRAPH_RATE_SCORE: (pgreat + great*0.5) / total_notes
            let max = (state.past_notes * 2) as f32;
            if max > 0.0 {
                (state.judge_counts.pgreat * 2 + state.judge_counts.great) as f32 / max
            } else {
                0.0
            }
        }
        147 => {
            // BARGRAPH_RATE_EXSCORE: ex_score so far / (past_notes * 2)
            let notes = if state.select_screen {
                state.select_total_notes.max(state.total_notes)
            } else {
                state.past_notes
            };
            let max = (notes * 2) as f32;
            if max > 0.0 { state.ex_score as f32 / max } else { 0.0 }
        }
        // BARGRAPH_BESTSCORERATE_NOW (112): best score at current progress / max_ex_score.
        // When a beatoraja ghost is available, use its per-note progression instead of a
        // linear projection from the final best score.
        112 => {
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 {
                projected_best_score_at_progress(state).unwrap_or(0) as f32 / max
            } else {
                0.0
            }
        }
        // BARGRAPH_BESTSCORERATE (113): best_ex_score / (total_notes * 2)
        113 => {
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 { result_mybest_ex_score(state).unwrap_or(0) as f32 / max } else { 0.0 }
        }
        // BARGRAPH_TARGETSCORERATE_NOW (114): target_ex_score * past_notes / (total_notes^2 * 2)
        114 => {
            let max = (state.total_notes as f64).powi(2) * 2.0;
            if max > 0.0 {
                (state.target_ex_score.unwrap_or(0) as f64 * state.past_notes as f64 / max) as f32
            } else {
                0.0
            }
        }
        // BARGRAPH_TARGETSCORERATE (115): target_ex_score / (total_notes * 2)
        115 => {
            let max = (state.total_notes * 2) as f32;
            if max > 0.0 { state.target_ex_score.unwrap_or(0) as f32 / max } else { 0.0 }
        }
        -1 => (state.select_clear_index as f32 / 10.0).clamp(0.0, 1.0),
        -2 => {
            let total_notes = state.select_total_notes.max(state.total_notes);
            let max = (total_notes * 2) as f32;
            if max > 0.0 { state.ex_score as f32 / max } else { 0.0 }
        }
        17 => state.select_master_volume.clamp(0.0, 1.0),
        18 => state.select_key_volume.clamp(0.0, 1.0),
        19 => state.select_bgm_volume.clamp(0.0, 1.0),
        // Lua fast/slow 比率 graph (ECFN select 等)
        148 => fast_slow_ratio_fast(state),
        149 => fast_slow_ratio_slow(state),
        _ => 0.0,
    }
}

pub(super) fn graph_raw_value(graph: &SkinGraphDef, state: &SkinDrawState) -> f32 {
    if !graph.value_expr.trim().is_empty() {
        if let Some(value) = evaluate_lua_number_expr(&graph.value_expr, state) {
            return value as f32;
        }
        if let Some(value) = skin_builtin_value_f32(&graph.value_expr, state) {
            return value;
        }
        skin_state_float_expr(&graph.value_expr, state).unwrap_or(0.0)
    } else if graph.is_ref_num {
        skin_state_number(graph.graph_type, state).unwrap_or(0) as f32
    } else {
        graph_value(graph.graph_type, state)
    }
}

/// Returns (fill multiplier on dst extent, UV clip ratio 0.0-1.0).
pub(super) fn graph_fill_dimensions(graph: &SkinGraphDef, state: &SkinDrawState) -> (f32, f32) {
    let raw = graph_raw_value(graph, state);
    if !graph.value_expr.trim().is_empty() {
        // beatoraja Lua graph: rendered size = dst.w * value (value is pixel multiplier).
        let max = graph.max.max(1) as f32;
        let raw = raw.max(0.0);
        return (raw, (raw / max).clamp(0.0, 1.0));
    }
    if graph.is_ref_num {
        let range = graph.max as f64 - graph.min as f64;
        let ratio = if range == 0.0 {
            // A degenerate range has no drawable extent at the boundary.
            if raw < graph.max as f32 { 1.0 } else { 0.0 }
        } else {
            ((f64::from(raw) - graph.min as f64) / range).clamp(0.0, 1.0) as f32
        };
        return (ratio, ratio);
    }
    let ratio = raw.clamp(0.0, 1.0);
    (ratio, ratio)
}

pub(super) fn skin_grid_cell_size(size: i32, divisions: i32) -> i32 {
    let divisions = divisions.max(1);
    size / divisions
}

pub(super) fn fast_slow_ratio_fast(state: &SkinDrawState) -> f32 {
    let Some(counts) = state.fast_slow_counts else {
        return 0.0;
    };
    let fast = fast_slow_ratio_fast_total(counts);
    let slow = fast_slow_ratio_slow_total(counts);
    let total = fast + slow;
    if total == 0 { 0.0 } else { fast as f32 / total as f32 }
}

pub(super) fn fast_slow_ratio_slow(state: &SkinDrawState) -> f32 {
    let Some(counts) = state.fast_slow_counts else {
        return 0.0;
    };
    let fast = fast_slow_ratio_fast_total(counts);
    let slow = fast_slow_ratio_slow_total(counts);
    let total = fast + slow;
    if total == 0 { 0.0 } else { slow as f32 / total as f32 }
}

pub(super) fn fast_slow_ratio_fast_total(counts: crate::snapshot::FastSlowJudgeCounts) -> u32 {
    counts.fast_pgreat
        + counts.fast_great
        + counts.fast_good
        + counts.fast_bad
        + counts.fast_poor
        + counts.fast_empty_poor
}

pub(super) fn fast_slow_ratio_slow_total(counts: crate::snapshot::FastSlowJudgeCounts) -> u32 {
    counts.slow_pgreat
        + counts.slow_great
        + counts.slow_good
        + counts.slow_bad
        + counts.slow_poor
        + counts.slow_empty_poor
}

pub(super) fn skin_frame_expr_value(expr: SkinFrameExpr, state: &SkinDrawState) -> Option<i32> {
    match expr {
        SkinFrameExpr::FastSlowBreakdownHeight(ref_id) => fast_slow_breakdown_height(ref_id, state),
    }
}

pub(super) fn fast_slow_breakdown_height(ref_id: i32, state: &SkinDrawState) -> Option<i32> {
    const REFS: [i32; 12] = [422, 419, 417, 415, 413, 411, 410, 412, 414, 416, 418, 421];
    if !REFS.contains(&ref_id) {
        return None;
    }
    let values = REFS.map(|candidate| skin_state_number(candidate, state).unwrap_or(0).max(0));
    let max = values.into_iter().max().unwrap_or(0);
    if max <= 0 {
        return Some(0);
    }
    let value = skin_state_number(ref_id, state).unwrap_or(0).max(0);
    Some((value as f32 / max as f32 * 100.0).round() as i32)
}

pub(super) fn judge_rate(count: u32, total: u32) -> f32 {
    if total > 0 { count as f32 / total as f32 } else { 0.0 }
}

pub(super) fn skin_slider_progress(slider: &SkinSliderDef, state: &SkinDrawState) -> Option<f32> {
    if let Some(progress) = evaluate_lua_number_expr(&slider.value_expr, state) {
        return Some((progress as f32).clamp(0.0, 1.0));
    }
    if !slider.value_expr.trim().is_empty()
        && let Some(progress) = skin_builtin_value_f32(&slider.value_expr, state)
    {
        return Some(progress.clamp(0.0, 1.0));
    }
    skin_slider_progress_by_type(slider.slider_type, state)
}

pub(super) fn skin_slider_progress_by_type(slider_type: i32, state: &SkinDrawState) -> Option<f32> {
    match slider_type {
        1 => Some(state.select_scroll_progress.clamp(0.0, 1.0)),
        4 | 5 => {
            let lane_cover = bmz_lane_cover_for_lift(state.lane_cover, state.lift);
            (lane_cover > 0.0).then_some(lane_cover)
        }
        6 => Some(state.play_progress.clamp(0.0, 1.0)),
        8 => Some(ir_ranking_scroll_progress(state)),
        17 => Some(state.select_master_volume.clamp(0.0, 1.0)),
        18 => Some(state.select_key_volume.clamp(0.0, 1.0)),
        19 => Some(state.select_bgm_volume.clamp(0.0, 1.0)),
        _ => None,
    }
}
