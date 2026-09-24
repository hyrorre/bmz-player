use super::*;

pub(in crate::skin) fn eval_skin_draw_operand(operand: &str, state: &SkinDrawState) -> Option<f32> {
    eval_skin_draw_atom_operand(operand, state)
        .or_else(|| eval_skin_draw_sum_operand(operand, state))
}

pub(in crate::skin) fn eval_skin_draw_sum_operand(
    operand: &str,
    state: &SkinDrawState,
) -> Option<f32> {
    let mut total = 0.0;
    let mut sign = 1.0;
    let mut start = 0;
    let mut saw_operator = false;

    for (index, ch) in operand.char_indices() {
        if index == 0 || !matches!(ch, '+' | '-') {
            continue;
        }
        let term = operand[start..index].trim();
        if term.is_empty() {
            return None;
        }
        total += sign * eval_skin_draw_atom_operand(term, state)?;
        sign = if ch == '-' { -1.0 } else { 1.0 };
        start = index + ch.len_utf8();
        saw_operator = true;
    }

    if !saw_operator {
        return None;
    }
    let term = operand[start..].trim();
    if term.is_empty() {
        return None;
    }
    total += sign * eval_skin_draw_atom_operand(term, state)?;
    Some(total)
}

pub(in crate::skin) fn eval_skin_draw_atom_operand(
    operand: &str,
    state: &SkinDrawState,
) -> Option<f32> {
    if let Some(flag_id) = parse_runtime_flag_operand(operand) {
        return Some(if state.runtime_flags.get(&flag_id).copied().unwrap_or(false) {
            1.0
        } else {
            0.0
        });
    }
    if let Some(ref_id) = parse_skin_float_number_operand(operand) {
        return skin_state_float_number(ref_id, state);
    }
    if let Some(event_id) = parse_skin_event_index_operand(operand) {
        return Some(skin_state_event_index(event_id, state) as f32);
    }
    if let Some(ref_id) = parse_skin_number_operand(operand) {
        return skin_state_number(ref_id, state)
            .or_else(|| lua_missing_number_sentinel(ref_id, state))
            .map(|value| value as f32);
    }
    if let Some(timer_id) = parse_skin_timer_operand(operand) {
        return Some(skin_timer_elapsed_ms(Some(timer_id), state).unwrap_or(i32::MIN) as f32);
    }
    if let Some(timer_id) = parse_skin_keybeam_operand(operand, "keybeam_hold") {
        return keybeam_lane_for_keyon_timer(timer_id)
            .map(|lane| i32::from(state.keybeam_hold_active[lane]) as f32);
    }
    if let Some(timer_id) = parse_skin_keybeam_operand(operand, "keybeam_fade") {
        return keybeam_lane_for_keyoff_timer(timer_id)
            .map(|lane| i32::from(state.keybeam_fade_active[lane]) as f32);
    }
    match operand {
        "gauge()" | "gauge" => Some(state.gauge),
        "gauge_type()" | "gauge_type" => Some(state.gauge_type as f32),
        "gauge_auto_shift()" | "gauge_auto_shift" => {
            Some(if state.gauge_auto_shift { 1.0 } else { 0.0 })
        }
        "gauge_auto_shift_mode()" | "gauge_auto_shift_mode" => {
            Some(state.select_gauge_auto_shift_index as f32)
        }
        "timer_off" | "timer_off_value" => Some(i32::MIN as f32),
        value => value.parse::<f32>().ok(),
    }
}

pub(in crate::skin) fn parse_runtime_flag_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("runtime_flag(")?.strip_suffix(')')?.trim();
    inner.parse().ok()
}

pub(in crate::skin) fn parse_skin_number_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("number(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

pub(in crate::skin) fn parse_skin_float_number_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("float_number(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

pub(in crate::skin) fn parse_skin_event_index_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("event_index(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

pub(in crate::skin) fn parse_skin_keybeam_operand(operand: &str, name: &str) -> Option<i32> {
    let inner = operand.strip_prefix(name)?.strip_prefix('(')?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

pub(in crate::skin) fn keybeam_lane_for_keyon_timer(timer: i32) -> Option<usize> {
    match timer {
        100..=107 => Some((timer - 100) as usize),
        108..=109 => Some(Lane::Key8.index() + (timer - 108) as usize),
        110 => Some(Lane::Scratch2.index()),
        111..=117 => Some(Lane::Key8.index() + (timer - 111) as usize),
        _ => None,
    }
}

pub(in crate::skin) fn keybeam_lane_for_keyoff_timer(timer: i32) -> Option<usize> {
    match timer {
        120..=127 => Some((timer - 120) as usize),
        128..=129 => Some(Lane::Key8.index() + (timer - 128) as usize),
        130 => Some(Lane::Scratch2.index()),
        131..=137 => Some(Lane::Key8.index() + (timer - 131) as usize),
        _ => None,
    }
}

pub(in crate::skin) fn skin_state_event_index(event_id: i32, state: &SkinDrawState) -> i32 {
    match event_id {
        SKIN_REF_BMZ_BEST_SCORE_ARRANGE_1P..=SKIN_REF_BMZ_BEST_SCORE_DOUBLE_OPTION => {
            best_score_option_index(event_id, state) as i32
        }
        40 => state.select_gauge_index as i32,
        41 => state.select_target_index as i32,
        42 => arrange_ref_index(state) as i32,
        43 => arrange_2p_ref_index(state) as i32,
        54 => state.skin_attempt.double_option_index.unwrap_or(state.select_double_option_index)
            as i32,
        55 => state.skin_attempt.hsfix_index.unwrap_or(state.select_hs_fix_index) as i32,
        72 => state.select_bga_index as i32,
        73 => state.select_assist_index as i32,
        75 => i32::from(state.judge_timing_auto_adjust),
        78 => {
            state.skin_attempt.gauge_auto_shift_index.unwrap_or(state.select_gauge_auto_shift_index)
                as i32
        }
        308 => state
            .skin_attempt
            .ln_mode_index
            .or(state.result_ln_mode_index)
            .unwrap_or(state.select_ln_mode_index) as i32,
        10 | 309 => state.select_difficulty_filter_index as i32,
        343 => i32::from(state.guide_se_enabled),
        350 => i32::from(state.assist_extra_note_depth),
        351 => state.assist_mine_mode as i32,
        352 => state.assist_scroll_mode as i32,
        353 => state.assist_long_note_mode as i32,
        360 => i32::from(state.skin_attempt.seven_to_nine_pattern),
        361 => i32::from(state.skin_attempt.seven_to_nine_type),
        400 => i32::from(state.constant_enabled),
        340 => {
            state.skin_attempt.judge_algorithm_index.unwrap_or(state.select_judge_algorithm_index)
                as i32
        }
        341 => state
            .skin_attempt
            .bottom_shiftable_gauge_index
            .unwrap_or(state.select_bottom_shiftable_gauge_index) as i32,
        344 => extended_arrange_ref_index(state) as i32,
        345 => extended_arrange_2p_ref_index(state) as i32,
        1900 => skin_hispeed_mode_index(state),
        SKIN_REF_BMZ_BASE_HISPEED => skin_base_hispeed_index(state),
        SKIN_REF_BMZ_NORMAL_HISPEED_LEVEL => skin_normal_hispeed_level(state) as i32,
        SKIN_REF_BMZ_HISPEED_CONFIG => skin_hispeed_config_index(state),
        SKIN_REF_BMZ_KEY_MODE => effective_skin_key_mode(state).map_or(0, skin_key_mode_number),
        SKIN_REF_BMZ_SELECT_SETTINGS_ROW_KIND => {
            select_settings_row_kind_index(state.select_row_kind)
        }
        SKIN_REF_BMZ_SELECT_SESSION_MODE => {
            state.skin_attempt.session_mode_index.unwrap_or(state.select_session_mode_index) as i32
        }
        SKIN_REF_BMZ_SOURCE_KEY_MODE => {
            state.skin_attempt.source_key_mode.map_or(0, skin_key_mode_number)
        }
        SKIN_REF_BMZ_SOURCE_LN_PROFILE => {
            i32::from(state.skin_attempt.source_ln_profile_bits.unwrap_or_default())
        }
        SKIN_REF_BMZ_RULE_MODE => state.rule_mode_index as i32,
        SKIN_REF_BMZ_LN_POLICY_SETTING => state.ln_policy_setting_index.unwrap_or_default() as i32,
        SKIN_REF_BMZ_LN_SCORE_POLICY => state.ln_score_policy_index.unwrap_or_default() as i32,
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT => {
            score_grade_facts(state).map_or(0, |facts| facts.current_index as i32)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEXT => {
            score_grade_facts(state).map_or(0, |facts| facts.next_index as i32)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST => {
            score_grade_facts(state).map_or(0, |facts| facts.nearest_index as i32)
        }
        _ => skin_random_lane_ref_number(event_id, state)
            .and_then(|value| i32::try_from(value).ok())
            .or_else(|| skin_state_lane_judge_event_index(event_id, state))
            .unwrap_or(0),
    }
}

pub(in crate::skin) fn skin_state_lane_judge_event_index(
    event_id: i32,
    state: &SkinDrawState,
) -> Option<i32> {
    let lane = match event_id {
        500 => Lane::Scratch,
        501..=509 => Lane::from_pms_key((event_id - 500) as u8)?,
        510 => Lane::Scratch2,
        511 => Lane::Key8,
        512 => Lane::Key9,
        513 => Lane::Key10,
        514 => Lane::Key11,
        515 => Lane::Key12,
        516 => Lane::Key13,
        517 => Lane::Key14,
        _ => return None,
    };
    Some(match state.lane_judge[lane.index()] {
        None => 0,
        Some(0) => 1,
        Some(1) => 2,
        Some(2) => 4,
        Some(3) => 6,
        Some(4) => 7,
        Some(5) => 8,
        Some(_) => 0,
    })
}

/// beatoraja `main_state.float_number(ref)`。BARGRAPH / SLIDER 系の比率 0.0-1.0。
pub(in crate::skin) fn skin_state_float_number(ref_id: i32, state: &SkinDrawState) -> Option<f32> {
    let is_play = !state.select_screen && state.result_failed.is_none();
    match ref_id {
        // `MainStateAccessor.float_number()` calls beatoraja's getRateProperty(),
        // so only SLIDER/BARGRAPH RateType IDs belong to this namespace.
        1 => Some(if state.select_screen {
            state.select_scroll_progress.clamp(0.0, 1.0)
        } else {
            0.0
        }),
        4 | 5 => Some(if is_play && state.lanecover_enabled {
            bmz_lane_cover_for_lift(state.lane_cover, state.lift)
        } else {
            0.0
        }),
        6 | 101 => Some(if is_play { state.play_progress.clamp(0.0, 1.0) } else { 0.0 }),
        // Skin configuration scroll position is not exposed by BMZ yet.
        7 => Some(0.0),
        8 => Some(ir_ranking_scroll_progress(state)),
        17 => Some(state.select_master_volume.clamp(0.0, 1.0)),
        18 => Some(state.select_key_volume.clamp(0.0, 1.0)),
        19 => Some(state.select_bgm_volume.clamp(0.0, 1.0)),
        102 => Some(state.resource_load_progress.clamp(0.0, 1.0)),
        103 => Some(select_level_rate(state, None)),
        105..=109 => Some(select_level_rate(state, Some(ref_id - 104))),
        110..=115 => Some(graph_value(ref_id, state)),
        140..=145 | 147 => Some(if state.select_screen { graph_value(ref_id, state) } else { 0.0 }),
        285..=289 => {
            let notes = state.select_total_notes.max(state.total_notes);
            let count = state.rival_judge_counts?[usize::try_from(ref_id - 285).ok()?];
            (notes > 0).then_some(count as f32 / notes as f32)
        }
        _ => None,
    }
}

pub(in crate::skin) fn select_level_rate(state: &SkinDrawState, difficulty: Option<i32>) -> f32 {
    if !state.select_screen
        || difficulty.is_some_and(|difficulty| state.difficulty != i64::from(difficulty))
    {
        return 0.0;
    }
    // This intentionally follows beatoraja's current switch fallthrough: every supported mode
    // ends with maxLevel=10.
    (state.select_play_level as f32 / 10.0).max(0.0)
}

pub(in crate::skin) fn ir_ranking_scroll_progress(state: &SkinDrawState) -> f32 {
    if state.ir_ranking.scroll_max == 0 {
        0.0
    } else {
        (state.ir_ranking.scroll_offset as f32 / state.ir_ranking.scroll_max as f32).clamp(0.0, 1.0)
    }
}

pub(in crate::skin) fn parse_skin_option_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("option(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

pub(in crate::skin) fn parse_skin_timer_operand(operand: &str) -> Option<i32> {
    let inner = operand.strip_prefix("timer(")?.strip_suffix(')')?.trim();
    inner.parse::<i32>().ok()
}

pub(in crate::skin) fn select_chart_notes_total_formula(notes: u32) -> f64 {
    let notes = f64::from(notes);
    if notes <= 0.0 {
        return 0.0;
    }
    7.605 * notes / (0.01 * notes + 6.5)
}

pub(in crate::skin) fn default_chart_total_count_value(state: &SkinDrawState) -> f32 {
    let notes = state.select_total_notes.max(state.total_notes);
    let total = state.select_chart_total_gauge.max(0.0) as f64;
    (select_chart_notes_total_formula(notes) - total) as f32
}

pub(in crate::skin) fn default_chart_gauge_graph_value(state: &SkinDrawState) -> f32 {
    (default_chart_total_count_value(state) * 0.75).max(0.0)
}

pub(in crate::skin) fn course_clear_rate_value(state: &SkinDrawState) -> f32 {
    let notes = f64::from(state.total_notes);
    if notes <= 0.0 {
        return 0.0;
    }
    let pgreat = f64::from(state.judge_counts.pgreat);
    let great = f64::from(state.judge_counts.great);
    let good = f64::from(state.judge_counts.good);
    let bad = f64::from(state.judge_counts.bad);
    let poor_and_miss =
        f64::from(state.judge_counts.poor.saturating_add(state.judge_counts.empty_poor));
    let progress = (pgreat + great + good + bad + poor_and_miss).min(notes);
    if progress <= 0.0 {
        return 0.0;
    }

    // WMII course result's "CLEAR RATE": course progress contributes 40%,
    // while its judgement-quality formula contributes the remaining 60%.
    let x = 8.0 * (pgreat + great) + 2.0 * good - (68.0 * bad + 100.0 * poor_and_miss);
    let mut y = 100.0 * x / (6.0 * notes);
    if y >= 0.0 {
        y /= 2.0;
    }
    let performance = (0.6 * (y + 50.0)).clamp(0.0, 60.0);
    (40.0 * progress / notes + performance) as f32
}

pub(in crate::skin) fn clamped_gauge_value(state: &SkinDrawState) -> f32 {
    if state.gauge_max <= 0.0 { 0.0 } else { state.gauge.clamp(0.0, state.gauge_max) }
}
