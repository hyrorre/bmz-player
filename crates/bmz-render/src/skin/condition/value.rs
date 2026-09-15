use super::*;

pub(in crate::skin) fn skin_builtin_value_f32(expr: &str, state: &SkinDrawState) -> Option<f32> {
    if let Some(slot) = expr.trim().strip_prefix("bmz:ir_score_rate:") {
        let slot = slot.parse::<i32>().ok()?;
        if !(1..=10).contains(&slot) {
            return None;
        }
        let (score, max_score) = ir_ranking_score_and_max(state, slot)?;
        return Some(if max_score == 0 { 0.0 } else { score as f32 / max_score as f32 });
    }
    if expr.trim() == "bmz:keylogger_nps" {
        return Some(state.keylogger_nps as f32);
    }
    if let Some(value) = keylogger_graph_value(expr.trim(), state) {
        return Some(value);
    }
    match expr.trim() {
        SKIN_EXPR_ADJUSTED_COVER => state.adjusted_cover_progress,
        SKIN_EXPR_ADJUSTED_RATE => state.adjusted_rate,
        SKIN_EXPR_ADJUSTED_RATE_ADOT => state.adjusted_rate_adot.map(|value| value as f32),
        SKIN_EXPR_FS_THRESHOLD => Some(state.fs_threshold_ms as f32),
        SKIN_EXPR_DEFAULT_CHART_TOTAL_COUNT => Some(default_chart_total_count_value(state)),
        SKIN_EXPR_DEFAULT_CHART_GAUGE => Some(default_chart_gauge_graph_value(state)),
        SKIN_EXPR_COURSE_CLEAR_RATE => Some(course_clear_rate_value(state)),
        SKIN_EXPR_GAUGE_PERCENT_INTEGER => {
            Some((clamped_gauge_value(state) * 100.0 / state.gauge_max.max(1.0)).floor())
        }
        SKIN_EXPR_GAUGE_PERCENT_FRACTION => {
            Some((clamped_gauge_value(state) * 10_000.0 / state.gauge_max.max(1.0)).floor() % 100.0)
        }
        SKIN_EXPR_GAUGE_AMOUNT_INTEGER => Some(clamped_gauge_value(state).floor()),
        SKIN_EXPR_GAUGE_AMOUNT_FRACTION => {
            Some((clamped_gauge_value(state) * 100.0).floor() % 100.0)
        }
        _ => None,
    }
}

pub(in crate::skin) fn keylogger_graph_value(expr: &str, state: &SkinDrawState) -> Option<f32> {
    let rest = expr.strip_prefix("bmz:keylogger_graph:")?;
    let mut parts = rest.split(':');
    let graph_kind = parts.next()?;
    let lane = parts.next()?.parse::<usize>().ok()?.checked_sub(1)?;
    let layer = parts.next()?;
    if parts.next().is_some() || lane >= LANE_COUNT {
        return None;
    }
    match graph_kind {
        "judge" => {
            let start = match layer {
                "cool" => 0,
                "great" => 1,
                "good" => 2,
                "bad" => 3,
                _ => return None,
            };
            let denominator_start = usize::from(state.keylogger_exclude_cool);
            let max = state
                .keylogger_judge_counts
                .iter()
                .map(|counts| counts[denominator_start..].iter().sum::<u32>())
                .max()
                .unwrap_or(0);
            let count = state.keylogger_judge_counts[lane][start..].iter().sum::<u32>();
            Some(if max == 0 { 0.0 } else { count as f32 / max as f32 })
        }
        "fastslow" => {
            let start = match layer {
                "cool" => 0,
                "fast" => 1,
                "slow" => 2,
                _ => return None,
            };
            let denominator_start = usize::from(state.keylogger_exclude_cool);
            let max = state
                .keylogger_fast_slow_counts
                .iter()
                .map(|counts| counts[denominator_start..].iter().sum::<u32>())
                .max()
                .unwrap_or(0);
            let count = state.keylogger_fast_slow_counts[lane][start..].iter().sum::<u32>();
            Some(if max == 0 { 0.0 } else { count as f32 / max as f32 })
        }
        _ => None,
    }
}

pub(in crate::skin) fn skin_builtin_value_i64(expr: &str, state: &SkinDrawState) -> Option<i64> {
    if let Some(slot) = expr.trim().strip_prefix("bmz:ir_score_rate_integer:") {
        let slot = slot.parse::<i32>().ok()?;
        return ir_ranking_score_rate_parts(state, slot).map(|parts| parts.0);
    }
    if let Some(slot) = expr.trim().strip_prefix("bmz:ir_score_rate_fraction:") {
        let slot = slot.parse::<i32>().ok()?;
        return ir_ranking_score_rate_parts(state, slot).map(|parts| parts.1);
    }
    if let Some(slot) = expr.trim().strip_prefix("bmz:ir_score_diff:") {
        let slot = slot.parse::<i32>().ok()?;
        if !(1..=10).contains(&slot) {
            return None;
        }
        let local_best = skin_state_number(170, state)?.max(skin_state_number(171, state)?);
        let ranking_score = ir_ranking_entry(&state.ir_ranking, slot - 1)?.ex_score?;
        return local_best.checked_sub(ranking_score);
    }
    if expr.trim() == "bmz:nearest_rank_diff_abs" {
        return nearest_grade_diff(state).map(|diff| diff.value.abs());
    }
    if expr.trim() == "bmz:wmii_next_rank_diff" {
        return wmii_next_rank_diff(state);
    }
    if expr.trim() == "bmz:wmii_next_rank_diff_no_max_minus" {
        return wmii_next_rank_diff_with_max_minus(state, false);
    }
    if expr.trim() == SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER {
        return select_total_notes_ratio_parts(state).map(|parts| parts.0);
    }
    if expr.trim() == SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION {
        return select_total_notes_ratio_parts(state).map(|parts| parts.1);
    }
    let number = skin_builtin_value_f32(expr, state)?;
    Some(match expr.trim() {
        SKIN_EXPR_DEFAULT_CHART_TOTAL_COUNT | SKIN_EXPR_DEFAULT_CHART_GAUGE => {
            number.round() as i64
        }
        _ => integer_property_value(number),
    })
}

pub(in crate::skin) fn select_total_notes_ratio_parts(state: &SkinDrawState) -> Option<(i64, i64)> {
    let total = skin_state_number(368, state)?.max(0);
    let notes = skin_state_number(74, state)?;
    if notes <= 0 {
        return Some((0, 0));
    }
    let scaled = i128::from(total).checked_mul(1_000)?.checked_div(i128::from(notes))?;
    Some((i64::try_from(scaled / 1_000).ok()?, i64::try_from(scaled % 1_000).ok()?))
}

pub(in crate::skin) fn skin_value_number(
    value: &SkinValueDef,
    state: &SkinDrawState,
) -> Option<i64> {
    if value.id == "Number_Todayplayednotes" {
        return Some(player_stat_u64(daily_completed_notes(&state.player_stats.daily)));
    }
    if !value.expr.trim().is_empty() {
        return skin_state_number_expr(&value.expr, state);
    }
    if !value.value_expr.trim().is_empty() {
        if let Some(number) = evaluate_lua_number_expr(&value.value_expr, state) {
            return Some(number as i64);
        }
        if let Some(number) = skin_builtin_value_i64(&value.value_expr, state) {
            return Some(number);
        }
        return skin_state_float_expr(&value.value_expr, state).map(integer_property_value);
    }
    skin_state_number(value.ref_id, state)
}

pub(in crate::skin) fn lua_value_callback_id(expr: &str) -> Option<usize> {
    expr.trim().strip_prefix(LUA_VALUE_CALLBACK_PREFIX).and_then(|id| id.parse::<usize>().ok())
}

pub(in crate::skin) fn evaluate_lua_number_expr(expr: &str, state: &SkinDrawState) -> Option<f64> {
    let callback_id = lua_value_callback_id(expr)?;
    let runtime = state.lua_runtime.as_ref()?;
    runtime.runtime.evaluate_number(
        callback_id,
        state,
        &runtime.enabled_options,
        &runtime.text_values,
    )
}

pub(in crate::skin) fn evaluate_lua_text_expr(expr: &str, state: &SkinDrawState) -> Option<String> {
    let callback_id = lua_value_callback_id(expr)?;
    let runtime = state.lua_runtime.as_ref()?;
    runtime.runtime.evaluate_text(
        callback_id,
        state,
        &runtime.enabled_options,
        &runtime.text_values,
    )
}

pub(in crate::skin) fn integer_property_value(value: f32) -> i64 {
    value as i64
}

pub(in crate::skin) fn skin_value_number_for_destination(
    value: &SkinValueDef,
    state: &SkinDrawState,
) -> Option<i64> {
    if let Some(level) = state.select_songlist_level_override {
        return Some(level);
    }
    if value.ref_id == 0 && value.expr.trim().is_empty() && value.value_expr.trim().is_empty() {
        return Some(if state.play_level != 0 {
            state.play_level
        } else {
            state.select_play_level
        });
    }
    skin_value_number(value, state)
}

pub(in crate::skin) fn skin_state_number_expr(expr: &str, state: &SkinDrawState) -> Option<i64> {
    let normalized = expr.replace('+', " + ").replace('-', " - ");
    let mut sign = 1_i64;
    let mut total = 0_i64;
    let mut expecting_value = true;
    for token in normalized.split_whitespace() {
        match token {
            "+" if expecting_value => sign = 1,
            "-" if expecting_value => sign = -1,
            "+" if !expecting_value => {
                sign = 1;
                expecting_value = true;
            }
            "-" if !expecting_value => {
                sign = -1;
                expecting_value = true;
            }
            value => {
                if !expecting_value {
                    return None;
                }
                let term = skin_state_number_expr_term(value, state)?;
                total += sign * term;
                sign = 1;
                expecting_value = false;
            }
        }
    }
    if expecting_value {
        return None;
    }
    Some(total)
}
