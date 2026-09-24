use super::*;

/// Rm-skin `text id="table"` と beatoraja `TEXT_TABLE1..3` (1001..1003) の表示ロジック。
pub fn format_rm_skin_course_table_text(
    course_stage: Option<CourseStageMarker>,
    primary: &str,
    secondary: &str,
    fallback: &str,
) -> String {
    if let Some(stage) = course_stage {
        return match stage {
            CourseStageMarker::Final => "COURSE : STAGE FINAL".to_string(),
            CourseStageMarker::Stage1 => "COURSE : STAGE 1".to_string(),
            CourseStageMarker::Stage2 => "COURSE : STAGE 2".to_string(),
            CourseStageMarker::Stage3 => "COURSE : STAGE 3".to_string(),
            CourseStageMarker::Stage4 => "COURSE : STAGE 4".to_string(),
        };
    }

    // Lua: `not tx1 or tx1 == "" and not tx2 or tx2 == ""`
    let use_fallback = secondary.is_empty() || (primary.is_empty() && secondary.is_empty());
    if use_fallback {
        if fallback.is_empty() {
            return "# No-Table".to_string();
        }
        return fallback.to_string();
    }

    if primary.is_empty() { format!(" > {secondary}") } else { format!("{primary} > {secondary}") }
}

#[cfg(test)]
pub(super) fn skin_state_text(text: &SkinTextDef, state: &SkinTextState<'_>) -> String {
    skin_state_text_with_draw_state(text, None, state)
}

pub(super) fn skin_state_text_with_draw_state(
    text: &SkinTextDef,
    draw_state: Option<&SkinDrawState>,
    state: &SkinTextState<'_>,
) -> String {
    if let Some(draw_state) = draw_state
        && lua_value_callback_id(&text.value_expr).is_some()
    {
        return evaluate_lua_text_expr(&text.value_expr, draw_state).unwrap_or_default();
    }
    if let Some(draw_state) = draw_state
        && let Some(value) = m_select_daily_stats_text(&text.id, &draw_state.player_stats.daily)
    {
        return value;
    }
    if text.value_expr.trim() == "bmz:text_concat:1001:1002" {
        return format!("{} {}", state.table_text_primary, state.table_level);
    }
    if text.value_expr.trim() == SKIN_EXPR_RESULT_TABLE_TITLE {
        return format!(
            "{} {} {}",
            state.table_level,
            state.table_text_primary,
            full_label(state.title, state.subtitle)
        );
    }
    if text.value_expr.trim() == SKIN_EXPR_DIFFICULTY_NAME {
        return state.difficulty_name.to_string();
    }
    if !text.constant_text.is_empty() {
        return text.constant_text.clone();
    }
    if let Some(ref_id) = text.number_ref {
        let Some(value) = draw_state.and_then(|state| skin_state_number(ref_id, state)) else {
            return String::new();
        };
        return format!("{}{}{}", text.prefix, value, text.suffix);
    }
    if let Some(region) = text.judge_region {
        let Some(state) = draw_state else {
            return String::new();
        };
        let Some(value) = skin_judge_region_text(state, region) else {
            return String::new();
        };
        return format!("{}{}{}", text.prefix, value, text.suffix);
    }
    if let Some(region) = text.judge_timing_region {
        let Some(state) = draw_state else {
            return String::new();
        };
        let Some(value) = skin_judge_timing_text(state, region) else {
            return String::new();
        };
        return format!("{}{}{}", text.prefix, value, text.suffix);
    }
    if text.value_expr.trim() == SKIN_EXPR_COURSE_TABLE_TEXT {
        return format_rm_skin_course_table_text(
            state.course_stage,
            state.table_text_primary,
            state.table_text_secondary,
            state.table_text_fallback,
        );
    }
    if is_select_bar_text_id(&text.id) {
        return state.bar_text.to_string();
    }
    match text.id.as_str() {
        "bmz_select_arrange" => return state.select_arrange.to_string(),
        "bmz_select_arrange_2p" => return state.select_arrange_2p.to_string(),
        "bmz_select_target" => return select_target_name(state.target),
        "bmz_select_gauge" => return state.select_gauge.to_string(),
        "bmz_select_gauge_auto_shift" => return state.select_gauge_auto_shift.to_string(),
        "bmz_select_bottom_shiftable_gauge" => {
            return state.select_bottom_shiftable_gauge.to_string();
        }
        "bmz_select_double_option" => return state.select_double_option.to_string(),
        "bmz_select_hs_fix" => return state.select_hs_fix.to_string(),
        "bmz_select_assist" => return state.select_assist.to_string(),
        "bmz_select_mode" => return state.select_mode.to_string(),
        "bmz_select_sort" => return state.select_sort.to_string(),
        "bmz_select_ln_mode" => return state.select_ln_mode.to_string(),
        "bmz_select_bga" => return state.select_bga.to_string(),
        "bmz_select_chart_replication" => return state.select_chart_replication.to_string(),
        "bmz_select_judge_timing_auto_adjust" => {
            return state.select_judge_timing_auto_adjust.to_string();
        }
        _ => {}
    }
    skin_main_state_text(text.ref_id, draw_state, state)
}

fn is_select_bar_text_id(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    ["bartext", "bar_text", "bar-text"].into_iter().any(|needle| {
        id.match_indices(needle).any(|(start, _)| {
            let suffix = &id[start + needle.len()..];
            suffix.is_empty()
                || suffix
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'_' | b'-'))
        })
    })
}

pub(super) fn skin_main_state_text(
    ref_id: i32,
    draw_state: Option<&SkinDrawState>,
    state: &SkinTextState<'_>,
) -> String {
    match ref_id {
        1 => {
            if state.rival.is_empty() {
                play_target_name(state.target, state.resolved_target_name)
            } else {
                state.rival.to_string()
            }
        }
        2 => state.player_name.to_string(),
        3 => target_setting_name(state.target),
        10 => state.title.to_string(),
        11 => state.subtitle.to_string(),
        12 => full_label(state.title, state.subtitle),
        13 => state.genre.to_string(),
        14 => state.artist.to_string(),
        15 => state.subartist.to_string(),
        16 => full_label(state.artist, state.subartist),
        17 => state.table_level.to_string(),
        30 => state.search_word.to_string(),
        86 => state.select_chart_replication.to_string(),
        120..=129 => ir_ranking_entry(state.ir_ranking, ref_id - 120)
            .map(|entry| entry.player_name.as_str().to_string())
            .unwrap_or_default(),
        150..=159 => state.course_titles[(ref_id - 150) as usize].to_string(),
        190..=196 => {
            draw_state.map(|state| random_mix_option_text(ref_id, state)).unwrap_or_default()
        }
        SKIN_REF_BMZ_RESULT_IR_SCOPE => {
            draw_state.map(|state| state.ir_ranking.scope.label().to_string()).unwrap_or_default()
        }
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT => draw_state
            .and_then(score_grade_facts)
            .map(|facts| facts.current_label().to_string())
            .unwrap_or_default(),
        SKIN_REF_BMZ_SCORE_GRADE_NEXT => draw_state
            .and_then(score_grade_facts)
            .map(|facts| facts.next_label().to_string())
            .unwrap_or_default(),
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST => draw_state
            .and_then(score_grade_facts)
            .map(|facts| facts.nearest_label().to_string())
            .unwrap_or_default(),
        SKIN_REF_BMZ_RULE_MODE => draw_state
            .map(|state| rule_mode_label(state.rule_mode_index).to_string())
            .unwrap_or_default(),
        SKIN_REF_BMZ_LN_POLICY_SETTING => draw_state
            .and_then(|state| state.ln_policy_setting_index)
            .map(|index| ln_policy_label(index).to_string())
            .unwrap_or_default(),
        SKIN_REF_BMZ_LN_SCORE_POLICY => draw_state
            .and_then(|state| state.ln_score_policy_index)
            .map(|index| ln_policy_label(index).to_string())
            .unwrap_or_default(),
        SKIN_TEXT_BMZ_DAILY_RANK => draw_state
            .map(|state| daily_rank_label(&state.player_stats.daily).to_string())
            .unwrap_or_default(),
        SKIN_TEXT_BMZ_DAILY_RECENT_BASE..=SKIN_TEXT_BMZ_DAILY_RECENT_LAST => draw_state
            .map(|state| {
                state.player_stats.daily.recent_titles
                    [(ref_id - SKIN_TEXT_BMZ_DAILY_RECENT_BASE) as usize]
                    .clone()
            })
            .unwrap_or_default(),
        1900 => draw_state
            .filter(|state| !state.select_screen || state.duration_green_ms.is_some())
            .map(|state| {
                if skin_hispeed_mode_is_floating(state) {
                    "FHS"
                } else if skin_base_hispeed_index(state) == 1 {
                    "NHS"
                } else {
                    "CHS"
                }
                .to_string()
            })
            .unwrap_or_default(),
        SKIN_REF_BMZ_BASE_HISPEED => draw_state
            .map(|state| if skin_base_hispeed_index(state) == 1 { "NHS" } else { "CHS" })
            .unwrap_or_default()
            .to_string(),
        SKIN_REF_BMZ_HISPEED_CONFIG => draw_state
            .map(|state| match skin_hispeed_config_index(state) {
                0 => "NORMAL",
                1 => "CLASSIC",
                2 => "FLOATING",
                3 => "NORMAL+FLOATING",
                _ => "CLASSIC+FLOATING",
            })
            .unwrap_or_default()
            .to_string(),
        // beatoraja StringPropertyFactory: 1001=tablename, 1002=tablelevel,
        // 1003=tablefull.  Rm-skin's combined table label is handled above by
        // id/value_expr, so direct numeric refs follow the beatoraja mapping.
        1001 => state.table_text_primary.to_string(),
        1002 => state.table_level.to_string(),
        1003 => state.table_text_fallback.to_string(),
        1010 => format!("bmz-player {}", env!("CARGO_PKG_VERSION")),
        1020 => {
            if !state.ir_ranking.online {
                String::new()
            } else {
                state.ir_ranking.provider_name.as_str().to_string()
            }
        }
        1021 => state.ir_ranking.user_name.as_str().to_string(),
        200..=209 => select_target_name_by_offset(state.target, ref_id - 210),
        210..=219 => select_target_name_by_offset(state.target, ref_id - 209),
        1000 => state.current_folder.to_string(),
        _ => String::new(),
    }
}

pub(super) fn lua_main_state_text_values(
    draw_state: &SkinDrawState,
    text_state: &SkinTextState<'_>,
) -> BTreeMap<i32, String> {
    let mut refs = vec![
        1,
        2,
        3,
        10,
        11,
        12,
        13,
        14,
        15,
        16,
        17,
        30,
        1000,
        1001,
        1002,
        1003,
        1010,
        1020,
        1021,
        1900,
        SKIN_REF_BMZ_BASE_HISPEED,
        SKIN_REF_BMZ_HISPEED_CONFIG,
        SKIN_TEXT_BMZ_DAILY_RANK,
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT,
        SKIN_REF_BMZ_SCORE_GRADE_NEXT,
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST,
        SKIN_REF_BMZ_RULE_MODE,
        SKIN_REF_BMZ_LN_POLICY_SETTING,
        SKIN_REF_BMZ_LN_SCORE_POLICY,
    ];
    refs.extend(120..=129);
    refs.extend(150..=159);
    refs.extend(190..=196);
    refs.extend(200..=219);
    refs.extend(SKIN_TEXT_BMZ_DAILY_RECENT_BASE..=SKIN_TEXT_BMZ_DAILY_RECENT_LAST);
    refs.into_iter()
        .map(|ref_id| (ref_id, skin_main_state_text(ref_id, Some(draw_state), text_state)))
        .collect()
}

fn rule_mode_label(index: usize) -> &'static str {
    match index {
        0 => "BEATORAJA",
        1 => "LR2ORAJA",
        2 => "DX",
        _ => "",
    }
}

fn ln_policy_label(index: usize) -> &'static str {
    match index {
        0 => "AUTO(LN)",
        1 => "AUTO(CN)",
        2 => "AUTO(HCN)",
        3 => "FORCE(LN)",
        4 => "FORCE(CN)",
        5 => "FORCE(HCN)",
        6 => "FORCE(HLN)",
        _ => "",
    }
}

fn random_mix_option_text(ref_id: i32, state: &SkinDrawState) -> String {
    let value = state.random_mix_options[(ref_id - 190) as usize];
    match ref_id {
        190 => {
            if value == 0 {
                "OFF".to_string()
            } else {
                format!("LEVEL {value}")
            }
        }
        191 | 192 => {
            if value == 0 {
                "NO LIMIT".to_string()
            } else {
                format!("LEVEL {value}")
            }
        }
        193 => {
            if value == 0 {
                "NO LIMIT".to_string()
            } else {
                format!("+- {value}BPM")
            }
        }
        194 | 195 => {
            if value == 0 {
                "NO LIMIT".to_string()
            } else {
                format!("{value} BPM")
            }
        }
        196 => {
            if value == 0 {
                "RANDOM".to_string()
            } else {
                format!("{value} STAGE")
            }
        }
        _ => String::new(),
    }
}

pub fn lua_main_state_option(
    option_id: i32,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> bool {
    test_skin_op(option_id, enabled_options, state)
}

pub fn lua_main_state_number(ref_id: i32, state: &SkinDrawState) -> i64 {
    skin_state_number(ref_id, state)
        .or_else(|| lua_missing_number_sentinel(ref_id, state))
        .unwrap_or_default()
}

/// beatorajaは未プレイResultの前回BPとBP差分をInteger.MIN_VALUEでLuaへ返し、
/// SkinNumber側だけを非表示にする。通常のnumber描画はNoneのまま維持する。
pub(in crate::skin) fn lua_missing_number_sentinel(
    ref_id: i32,
    state: &SkinDrawState,
) -> Option<i64> {
    (state.result_failed.is_some() && matches!(ref_id, 176 | 178)).then_some(i64::from(i32::MIN))
}

pub fn lua_main_state_float(ref_id: i32, state: &SkinDrawState) -> f64 {
    f64::from(skin_state_float_number(ref_id, state).unwrap_or_default())
}

pub fn lua_main_state_timer(timer_id: i32, state: &SkinDrawState) -> Option<i32> {
    skin_timer_elapsed_ms(Some(timer_id), state)
}

pub fn lua_main_state_event_index(event_id: i32, state: &SkinDrawState) -> i32 {
    skin_state_event_index(event_id, state)
}

pub(super) const SELECT_TARGET_IDS: [&str; 13] = [
    "NONE",
    "RANK_A",
    "RANK_AA-",
    "RANK_AA",
    "RANK_AAA-",
    "RANK_AAA",
    "RANK_MAX-",
    "MAX",
    "RANK_NEXT",
    "IR_TOP",
    "IR_NEXT",
    "RIVAL TOP",
    "RIVAL NEXT",
];
pub(super) const SELECT_TARGET_NAMES: [&str; 13] = [
    "NO TARGET",
    "RANK A",
    "RANK AA-",
    "RANK AA",
    "RANK AAA-",
    "RANK AAA",
    "RANK MAX-",
    "MAX",
    "NEXT RANK",
    "IR TOP",
    "IR NEXT",
    "RIVAL TOP",
    "RIVAL NEXT",
];
