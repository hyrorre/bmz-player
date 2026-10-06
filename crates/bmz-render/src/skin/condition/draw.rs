use super::*;

pub(in crate::skin) fn eval_skin_draw_condition(condition: &str, state: &SkinDrawState) -> bool {
    let condition = condition.trim();
    if condition.is_empty() {
        return true;
    }
    if let Some(property) = resolve_boolean_property_name(condition) {
        return test_skin_op(property.id, &[], state) ^ property.negated;
    }
    if let Some(callback_id) =
        condition.strip_prefix(LUA_DRAW_CALLBACK_PREFIX).and_then(|id| id.parse::<usize>().ok())
    {
        let Some(runtime) = state.lua_runtime.as_ref() else {
            return false;
        };
        return runtime.runtime.evaluate_draw(
            callback_id,
            state,
            &runtime.enabled_options,
            &runtime.text_values,
        );
    }

    condition.split("||").flat_map(|segment| segment.split(" or ")).any(|branch| {
        branch
            .split("&&")
            .flat_map(|segment| segment.split(" and "))
            .all(|term| eval_skin_draw_term(term.trim(), state).unwrap_or(false))
    })
}

pub(in crate::skin) fn eval_skin_draw_term(term: &str, state: &SkinDrawState) -> Option<bool> {
    if let Some(flag_id) = parse_runtime_flag_operand(term) {
        return Some(*state.runtime_flags.get(&flag_id).unwrap_or(&false));
    }
    if let Some(flag_id) = term.strip_prefix("not ").and_then(parse_runtime_flag_operand) {
        return Some(!state.runtime_flags.get(&flag_id).copied().unwrap_or(false));
    }
    if term == "select_score_available()" {
        return Some(
            state.select_screen
                && select_score_metadata_available(state)
                && state.select_ex_score.is_some(),
        );
    }
    if let Some(panel) = parse_result_panel_predicate(term) {
        return state.result_panel.map(|current| current == panel);
    }
    if let Some((slot, lower, upper)) = parse_ir_score_rate_band_predicate(term) {
        let score = ir_ranking_entry(&state.ir_ranking, slot - 1)?.ex_score?;
        let max_score =
            i64::from(state.select_total_notes.max(state.total_notes)).checked_mul(2)?;
        if max_score <= 0 {
            return Some(false);
        }
        let score = score.clamp(0, max_score);
        return Some(score * 9 >= max_score * lower && score * 9 < max_score * upper);
    }
    if let Some((slot, lower, upper)) = parse_ir_score_rate_range_predicate(term) {
        let score = ir_ranking_entry(&state.ir_ranking, slot - 1)?.ex_score?;
        let max_score =
            i64::from(state.select_total_notes.max(state.total_notes)).checked_mul(2)?;
        if max_score <= 0 {
            return Some(false);
        }
        let score = score.clamp(0, max_score);
        return Some(score * 1000 > max_score * lower && score * 1000 <= max_score * upper);
    }
    if let Some(slot) = parse_ir_ranking_user_predicate(term) {
        let entry = ir_ranking_entry(&state.ir_ranking, slot - 1)?;
        let user_name = state.ir_ranking.user_name.as_str();
        return Some(!user_name.is_empty() && entry.player_name.as_str() == user_name);
    }
    if let Some((lower, upper)) = parse_score_rate_band_predicate(term) {
        let score = i64::from(state.select_ex_score.unwrap_or(state.ex_score));
        let total_notes = i64::from(state.select_total_notes.max(state.total_notes));
        let max_score = total_notes.checked_mul(2)?;
        if max_score <= 0 {
            return Some(false);
        }
        let score = score.clamp(0, max_score);
        return Some(score * 9 >= max_score * lower && score * 9 < max_score * upper);
    }
    if let Some((grade, sign)) = parse_nearest_rank_predicate(term) {
        let diff = nearest_grade_diff(state)?;
        return Some(diff.grade == grade && nearest_rank_sign_matches(diff.value, sign));
    }
    if let Some(width) = parse_nearest_rank_label_width_predicate(term) {
        let score = i64::from(state.select_ex_score.unwrap_or(state.ex_score));
        let max_score =
            i64::from(state.select_total_notes.max(state.total_notes)).checked_mul(2)?;
        if max_score <= 0 {
            return Some(false);
        }
        let score = score.clamp(0, max_score);
        return Some(match width {
            3 => score * 18 >= max_score * 15,
            2 => score * 18 >= max_score * 13 && score * 18 < max_score * 15,
            1 => score * 18 < max_score * 13,
            _ => false,
        });
    }
    if let Some(stage) = parse_wmii_next_rank_stage_predicate(term) {
        return Some(wmii_next_rank_stage(state) == Some(stage));
    }
    if let Some(stage) = parse_wmii_next_rank_stage_no_max_minus_predicate(term) {
        return Some(wmii_next_rank_stage_with_max_minus(state, false) == Some(stage));
    }
    if term == "wmii_next_rank_diff_zero()" {
        return Some(wmii_next_rank_diff(state) == Some(0));
    }
    if term == "wmii_next_rank_diff_nonzero()" {
        return Some(wmii_next_rank_diff(state).is_some_and(|diff| diff != 0));
    }
    if let Some(sign) = parse_nearest_rank_sign_predicate(term) {
        return nearest_grade_diff(state).map(|diff| nearest_rank_sign_matches(diff.value, sign));
    }
    if let Some((mode, digits)) = parse_gauge_value_digits_predicate(term) {
        let value = match mode {
            "percent" => clamped_gauge_value(state) * 100.0 / state.gauge_max.max(1.0),
            "amount" => clamped_gauge_value(state),
            _ => return None,
        };
        let actual_digits = if value.floor() >= 100.0 {
            3
        } else if value.floor() >= 10.0 {
            2
        } else {
            1
        };
        return Some(actual_digits == digits);
    }
    if let Some((group, part, below_border)) = parse_gauge_lead_glow_predicate(term) {
        return Some(eval_gauge_lead_glow_predicate(group, part, below_border, state));
    }
    if let Some((graph_kind, lane, slot, kind)) = parse_keylogger_draw_predicate(term) {
        let actual = match graph_kind {
            "judge" => state.keylogger_event_judge.get(lane)?.get(slot).copied()?,
            "fastslow" => state.keylogger_event_fast_slow.get(lane)?.get(slot).copied()?,
            _ => return None,
        };
        return Some(actual == kind);
    }
    if let Some(option_id) = parse_skin_option_operand(term) {
        return Some(test_skin_op(option_id, &[], state));
    }
    if let Some(option_id) = term.strip_prefix('!').and_then(parse_skin_option_operand) {
        return Some(!test_skin_op(option_id, &[], state));
    }
    let operators = [">=", "<=", "==", "!=", ">", "<"];
    for operator in operators {
        let Some(index) = term.find(operator) else {
            continue;
        };
        let left = term[..index].trim();
        let right = term[index + operator.len()..].trim();
        let left = eval_skin_draw_operand(left, state)?;
        let right = eval_skin_draw_operand(right, state)?;
        return Some(match operator {
            ">=" => left >= right,
            "<=" => left <= right,
            "==" => (left - right).abs() < f32::EPSILON,
            "!=" => (left - right).abs() >= f32::EPSILON,
            ">" => left > right,
            "<" => left < right,
            _ => false,
        });
    }
    None
}

pub(in crate::skin) fn parse_result_panel_predicate(term: &str) -> Option<i32> {
    let panel = term.strip_prefix("result_panel(")?.strip_suffix(')')?.trim().parse().ok()?;
    (0..=2).contains(&panel).then_some(panel)
}

pub(in crate::skin) fn parse_score_rate_band_predicate(term: &str) -> Option<(i64, i64)> {
    let inner = term.strip_prefix("score_rate_band(")?.strip_suffix(')')?;
    let (lower, upper) = inner.split_once(',')?;
    let lower = lower.trim().parse::<i64>().ok()?;
    let upper = upper.trim().parse::<i64>().ok()?;
    (0 <= lower && lower < upper && upper <= 10).then_some((lower, upper))
}

pub(in crate::skin) fn parse_ir_score_rate_band_predicate(term: &str) -> Option<(i32, i64, i64)> {
    let inner = term.strip_prefix("ir_score_rate_band(")?.strip_suffix(')')?;
    let mut args = inner.split(',').map(str::trim);
    let slot = args.next()?.parse::<i32>().ok()?;
    let lower = args.next()?.parse::<i64>().ok()?;
    let upper = args.next()?.parse::<i64>().ok()?;
    if args.next().is_some()
        || !(1..=10).contains(&slot)
        || !(0 <= lower && lower < upper && upper <= 10)
    {
        return None;
    }
    Some((slot, lower, upper))
}

pub(in crate::skin) fn parse_ir_score_rate_range_predicate(term: &str) -> Option<(i32, i64, i64)> {
    let inner = term.strip_prefix("ir_score_rate_range(")?.strip_suffix(')')?;
    let mut args = inner.split(',').map(str::trim);
    let slot = args.next()?.parse::<i32>().ok()?;
    let lower = args.next()?.parse::<i64>().ok()?;
    let upper = args.next()?.parse::<i64>().ok()?;
    if args.next().is_some()
        || !(1..=10).contains(&slot)
        || !(-10 <= lower && lower < upper && upper <= 1000)
    {
        return None;
    }
    Some((slot, lower, upper))
}

pub(in crate::skin) fn parse_ir_ranking_user_predicate(term: &str) -> Option<i32> {
    let slot = term.strip_prefix("ir_ranking_user(")?.strip_suffix(')')?.trim().parse().ok()?;
    (1..=10).contains(&slot).then_some(slot)
}

pub(in crate::skin) fn parse_nearest_rank_predicate(term: &str) -> Option<(&str, &str)> {
    let inner = term.strip_prefix("nearest_rank(")?.strip_suffix(')')?;
    let (grade, sign) = inner.split_once(',')?;
    let grade = grade.trim();
    let sign = sign.trim();
    (matches!(grade, "F" | "E" | "D" | "C" | "B" | "A" | "AA" | "AAA" | "MAX")
        && matches!(sign, "plus" | "minus"))
    .then_some((grade, sign))
}

pub(in crate::skin) fn parse_nearest_rank_sign_predicate(term: &str) -> Option<&str> {
    let sign = term.strip_prefix("nearest_rank_sign(")?.strip_suffix(')')?.trim();
    matches!(sign, "plus" | "minus").then_some(sign)
}

pub(in crate::skin) fn parse_nearest_rank_label_width_predicate(term: &str) -> Option<i32> {
    let width = term
        .strip_prefix("nearest_rank_label_width(")?
        .strip_suffix(')')?
        .trim()
        .parse::<i32>()
        .ok()?;
    (1..=3).contains(&width).then_some(width)
}

pub(in crate::skin) fn parse_wmii_next_rank_stage_predicate(term: &str) -> Option<i32> {
    let stage =
        term.strip_prefix("wmii_next_rank_stage(")?.strip_suffix(')')?.trim().parse().ok()?;
    (0..=8).contains(&stage).then_some(stage)
}

pub(in crate::skin) fn parse_wmii_next_rank_stage_no_max_minus_predicate(
    term: &str,
) -> Option<i32> {
    let stage = term
        .strip_prefix("wmii_next_rank_stage_no_max_minus(")?
        .strip_suffix(')')?
        .trim()
        .parse()
        .ok()?;
    (0..=7).contains(&stage).then_some(stage)
}

pub(in crate::skin) fn nearest_rank_sign_matches(value: i64, sign: &str) -> bool {
    match sign {
        "plus" => value >= 0,
        "minus" => value < 0,
        _ => false,
    }
}

pub(in crate::skin) fn parse_gauge_value_digits_predicate(term: &str) -> Option<(&str, usize)> {
    let inner = term.strip_prefix("gauge_value_digits(")?.strip_suffix(')')?;
    let (mode, digits) = inner.split_once(',')?;
    let mode = mode.trim();
    let digits = digits.trim().parse::<usize>().ok()?;
    (matches!(mode, "percent" | "amount") && (1..=3).contains(&digits)).then_some((mode, digits))
}

pub(in crate::skin) fn parse_gauge_lead_glow_predicate(term: &str) -> Option<(&str, i32, bool)> {
    let inner = term.strip_prefix("gauge_lead_glow(")?.strip_suffix(')')?;
    let mut args = inner.split(',').map(str::trim);
    let group = args.next()?;
    let part = args.next()?.parse::<i32>().ok()?;
    let below_border = match args.next()? {
        "above" => false,
        "below" => true,
        _ => return None,
    };
    if args.next().is_some()
        || !(1..=24).contains(&part)
        || !matches!(group, "assist_easy" | "easy" | "groove" | "hard" | "exhard" | "hazard")
    {
        return None;
    }
    Some((group, part, below_border))
}

pub(in crate::skin) fn eval_gauge_lead_glow_predicate(
    group: &str,
    part: i32,
    below_border: bool,
    state: &SkinDrawState,
) -> bool {
    let actual_group = match state.gauge_type {
        0 => "assist_easy",
        1 => "easy",
        2 => "groove",
        3 | 6 => "hard",
        4 | 7 => "exhard",
        5 | 8 => "hazard",
        _ => "groove",
    };
    if actual_group != group || state.gauge <= 0.0 {
        return false;
    }
    let max = state.gauge_max.max(1.0);
    let border = match (max > 100.0, state.gauge_type) {
        (true, 0) => 65.0,
        (true, 1 | 2) => 85.0,
        (false, 0) => 60.0,
        (false, 1 | 2) => 80.0,
        _ => 0.0,
    };
    let lead = ((state.gauge * 24.0 / max).floor() as i32).clamp(1, 24);
    lead == part && ((part as f32 * max / 24.0) < border) == below_border
}

pub(in crate::skin) fn parse_keylogger_draw_predicate(
    term: &str,
) -> Option<(&str, usize, usize, u8)> {
    let inner = term.strip_prefix("keylogger_")?.strip_suffix(')')?;
    let (graph_kind, args) = inner.split_once('(')?;
    let mut args = args.split(',');
    let lane = args.next()?.trim().parse::<usize>().ok()?.checked_sub(1)?;
    let slot = args.next()?.trim().parse::<usize>().ok()?.checked_sub(1)?;
    let kind_name = args.next()?.trim();
    if args.next().is_some() || lane >= LANE_COUNT || slot >= 16 {
        return None;
    }
    let kind = match (graph_kind, kind_name) {
        ("judge", "none") | ("fastslow", "none") => 0,
        ("judge", "cool") | ("fastslow", "cool") => 1,
        ("judge", "great") | ("fastslow", "fast") => 2,
        ("judge", "good") | ("fastslow", "slow") => 3,
        ("judge", "bad") => 4,
        _ => return None,
    };
    Some((graph_kind, lane, slot, kind))
}

pub(in crate::skin) fn destination_timer_elapsed_ms(
    destination: &SkinDestinationDef,
    state: &SkinDrawState,
) -> Option<i32> {
    if let Some(rest) = destination.timer_expr.strip_prefix("bmz:keylogger_chattering:") {
        let lane = rest.parse::<usize>().ok()?.checked_sub(1)?;
        return state.keylogger_chattering_ms.get(lane).copied().flatten();
    }
    if let Some(rest) = destination.timer_expr.strip_prefix("bmz:keylogger_event:") {
        let mut parts = rest.split(':');
        let lane = parts.next()?.parse::<usize>().ok()?.checked_sub(1)?;
        let slot = parts.next()?.parse::<usize>().ok()?.checked_sub(1)?;
        if parts.next().is_some() {
            return None;
        }
        return state.keylogger_event_ms.get(lane)?.get(slot).copied().flatten();
    }
    skin_timer_elapsed_ms(destination.timer, state)
}
