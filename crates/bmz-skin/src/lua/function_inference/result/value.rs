use super::*;

pub(in crate::lua) fn infer_gauge_type_imageset_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    {
        main_state_probe.lock().ok()?.begin_gauge_type_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let (gauge_calls, number_calls) = {
        let mut probe = main_state_probe.lock().ok()?;
        let gauge_calls = probe.gauge_type_calls;
        let number_calls = probe.number_calls.clone();
        probe.end_recording();
        (gauge_calls, number_calls)
    };
    (gauge_calls > 0 && number_calls.is_empty()).then_some(SKIN_REF_PLAY_GAUGE_TYPE)
}

pub(in crate::lua) fn infer_course_table_text_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    if object_id == Some("table") {
        return Some(SKIN_EXPR_COURSE_TABLE_TEXT.to_string());
    }

    let option_calls = collect_option_calls(function, main_state_probe)?;
    if !option_calls.contains(&290) {
        return None;
    }

    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let text_calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.text_calls.clone();
        probe.end_recording();
        calls
    };
    if text_calls.iter().any(|ref_id| (1001..=1003).contains(ref_id)) {
        Some(SKIN_EXPR_COURSE_TABLE_TEXT.to_string())
    } else {
        None
    }
}

pub(in crate::lua) fn infer_main_state_text_ref(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let result = function.call::<Value>(()).ok();
    let (text_calls, other_state) = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.text_calls.clone();
        let other_state = !probe.number_calls.is_empty()
            || !probe.option_calls.is_empty()
            || !probe.timer_calls.is_empty();
        probe.end_recording();
        (calls, other_state)
    };
    let id = single_number_call(&text_calls)?;
    let Value::String(text) = result? else {
        return None;
    };
    (!other_state && text.to_string_lossy() == format!("Text{id}")).then_some(id)
}

pub(in crate::lua) fn infer_text_concat_expr(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    main_state_probe.lock().ok()?.begin_number_call_recording(0);
    let result = function.call::<Value>(()).ok();
    let text_calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.text_calls.clone();
        probe.end_recording();
        calls
    };
    if text_calls != [1001, 1002] {
        return None;
    }
    let Value::String(text) = result? else {
        return None;
    };
    (text.to_string_lossy() == "Text1001 Text1002").then(|| "bmz:text_concat:1001:1002".to_string())
}

pub(in crate::lua) fn infer_nearest_rank_diff_value_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    let supported = match object_id {
        Some("diff_rank") => refs == [71, 74],
        Some("rank_diff_count")
        | Some("default_playerdata_diff_count")
        | Some("default_playerdata_mode_diff_count") => {
            refs.contains(&71)
                && refs.contains(&74)
                && refs.iter().all(|ref_id| matches!(ref_id, 71 | 74 | 170 | 271))
        }
        _ => false,
    };
    if !supported {
        return None;
    }
    let mut matches_nearest = true;
    let mut matches_wmii = object_id == Some("diff_rank");
    let mut matches_wmii_without_max_minus = object_id == Some("diff_rank");
    for total_notes in [9, 10, 37] {
        for ex_score in 0..=total_notes * 2 {
            let values = refs
                .iter()
                .copied()
                .map(|ref_id| {
                    let value = match ref_id {
                        71 => ex_score,
                        74 => total_notes,
                        _ => 0,
                    };
                    (ref_id, value)
                })
                .collect();
            let actual = call_number_float_with_values(function, main_state_probe, values)?;
            let expected = match object_id {
                Some(
                    "rank_diff_count"
                    | "default_playerdata_diff_count"
                    | "default_playerdata_mode_diff_count",
                ) => luxe_flat_nearest_rank_diff(ex_score, total_notes)? as f64,
                _ => wmii_nearest_rank(ex_score, total_notes)?.2 as f64,
            };
            if !approx_float_eq(actual, expected) {
                matches_nearest = false;
            }
            if matches_wmii
                && !approx_float_eq(
                    actual,
                    wmii_next_rank_diff(ex_score, total_notes, true)? as f64,
                )
            {
                matches_wmii = false;
            }
            if matches_wmii_without_max_minus
                && !approx_float_eq(
                    actual,
                    wmii_next_rank_diff(ex_score, total_notes, false)? as f64,
                )
            {
                matches_wmii_without_max_minus = false;
            }
        }
    }
    if matches_wmii {
        Some("bmz:wmii_next_rank_diff".to_string())
    } else if matches_wmii_without_max_minus {
        Some("bmz:wmii_next_rank_diff_no_max_minus".to_string())
    } else if matches_nearest {
        Some("bmz:nearest_rank_diff_abs".to_string())
    } else {
        None
    }
}

fn wmii_next_rank_diff(ex_score: i32, total_notes: i32, include_max_minus: bool) -> Option<i32> {
    let max_score = total_notes.checked_mul(2)?;
    if max_score <= 0 {
        return None;
    }
    let ex_score = ex_score.clamp(0, max_score);
    for (numerator, denominator) in
        [(6, 27), (9, 27), (12, 27), (15, 27), (18, 27), (21, 27), (24, 27)]
    {
        let threshold = (max_score * numerator + denominator - 1) / denominator;
        if ex_score < threshold {
            return Some(threshold - ex_score);
        }
    }
    if include_max_minus {
        let threshold = (max_score * 17 + 17) / 18;
        if ex_score < threshold {
            return Some(threshold - ex_score);
        }
    }
    if ex_score < max_score {
        return Some(max_score - ex_score);
    }
    Some(0)
}

fn wmii_next_rank_stage(ex_score: i32, total_notes: i32, include_max_minus: bool) -> Option<i32> {
    let max_score = total_notes.checked_mul(2)?;
    if max_score <= 0 {
        return None;
    }
    let ex_score = ex_score.clamp(0, max_score);
    for (numerator, denominator, stage) in
        [(6, 27, 7), (9, 27, 6), (12, 27, 5), (15, 27, 4), (18, 27, 3), (21, 27, 2), (24, 27, 1)]
    {
        let threshold = (max_score * numerator + denominator - 1) / denominator;
        if ex_score < threshold {
            return Some(stage);
        }
    }
    if include_max_minus {
        let threshold = (max_score * 17 + 17) / 18;
        if ex_score < threshold {
            return Some(8);
        }
    }
    Some(0)
}

fn verify_wmii_next_rank_predicate(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    expected: impl Fn(i32, i32) -> bool,
) -> bool {
    if collect_number_refs(function, main_state_probe).as_deref() != Some(&[71, 74]) {
        return false;
    }
    for total_notes in [9, 10, 37] {
        for ex_score in 0..=total_notes * 2 {
            let actual = call_draw_with_numbers(
                function,
                main_state_probe,
                BTreeMap::from([(71, ex_score), (74, total_notes)]),
            );
            if actual != Some(expected(ex_score, total_notes)) {
                return false;
            }
        }
    }
    true
}

pub(in crate::lua) fn verify_wmii_next_rank_stage_draw(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    stage: i32,
    include_max_minus: bool,
) -> bool {
    (0..=8).contains(&stage)
        && verify_wmii_next_rank_predicate(function, main_state_probe, |ex_score, total_notes| {
            wmii_next_rank_stage(ex_score, total_notes, include_max_minus) == Some(stage)
        })
}

pub(in crate::lua) fn verify_wmii_next_rank_diff_zero_draw(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    verify_wmii_next_rank_predicate(function, main_state_probe, |ex_score, total_notes| {
        wmii_next_rank_diff(ex_score, total_notes, true) == Some(0)
    })
}

pub(in crate::lua) fn verify_wmii_next_rank_diff_nonzero_draw(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    verify_wmii_next_rank_predicate(function, main_state_probe, |ex_score, total_notes| {
        wmii_next_rank_diff(ex_score, total_notes, true).is_some_and(|diff| diff != 0)
    })
}

pub(in crate::lua) fn luxe_flat_nearest_rank_diff(ex_score: i32, total_notes: i32) -> Option<i32> {
    luxe_flat_nearest_rank(ex_score, total_notes).map(|(_, _, diff)| diff)
}

fn luxe_flat_nearest_rank(
    ex_score: i32,
    total_notes: i32,
) -> Option<(&'static str, &'static str, i32)> {
    let max = total_notes.checked_mul(2)?;
    if max <= 0 {
        return None;
    }
    let ex_score = ex_score.clamp(0, max);
    if ex_score >= max {
        return Some(("MAX", "plus", 0));
    }
    const RANKS: [(&str, i32); 9] = [
        ("F", 0),
        ("E", 2),
        ("D", 3),
        ("C", 4),
        ("B", 5),
        ("A", 6),
        ("AA", 7),
        ("AAA", 8),
        ("MAX", 9),
    ];
    let current =
        RANKS.iter().rposition(|(_, boundary)| ex_score * 9 >= *boundary * max).unwrap_or(0);
    let (lower_grade, lower) = RANKS[current];
    let (upper_grade, upper) = *RANKS.get(current + 1)?;
    let lower_score = (lower * max + 8) / 9;
    let upper_score = (upper * max + 8) / 9;
    if ex_score * 18 < (lower + upper) * max {
        Some((lower_grade, "plus", (ex_score - lower_score).max(0)))
    } else {
        Some((upper_grade, "minus", (upper_score - ex_score).max(0)))
    }
}

#[derive(Clone, Copy)]
enum LuaNumberSlot {
    Global(f64),
    Upvalue(i32, f64),
}

impl LuaNumberSlot {
    fn scorerate(lua: &Lua, function: &Function) -> Option<Self> {
        if let Some((index, value)) = lua_scorerate_upvalue(lua, function) {
            return Some(Self::Upvalue(index, value));
        }
        match lua.globals().get::<Value>("scorerate").ok()? {
            Value::Integer(value) => Some(Self::Global(value as f64)),
            Value::Number(value) => Some(Self::Global(value)),
            _ => None,
        }
    }

    fn set(self, lua: &Lua, function: &Function, value: f64) -> bool {
        match self {
            Self::Global(_) => lua.globals().set("scorerate", value).is_ok(),
            Self::Upvalue(index, _) => set_lua_number_upvalue(lua, function, index, value),
        }
    }

    fn restore(self, lua: &Lua, function: &Function) {
        let value = match self {
            Self::Global(value) | Self::Upvalue(_, value) => value,
        };
        let _ = self.set(lua, function, value);
    }
}

#[derive(Clone, Copy)]
enum LuaBooleanSlot {
    Global(&'static str, bool),
    Upvalue(i32, bool),
}

impl LuaBooleanSlot {
    fn rank_plus(lua: &Lua, function: &Function) -> Option<Self> {
        if let Some((index, value)) = lua_rank_plus_upvalue(lua, function) {
            return Some(Self::Upvalue(index, value));
        }
        Some(Self::Global("rank_plus", lua.globals().get::<bool>("rank_plus").ok()?))
    }

    fn flag_score(lua: &Lua, function: &Function) -> Option<Self> {
        if let Ok(value) = lua.globals().get::<bool>("flag_score") {
            return Some(Self::Global("flag_score", value));
        }
        let (index, value) = lua_flag_score_upvalue(lua, function)?;
        Some(Self::Upvalue(index, value))
    }

    fn set(self, lua: &Lua, function: &Function, value: bool) -> bool {
        match self {
            Self::Global(name, _) => lua.globals().set(name, value).is_ok(),
            Self::Upvalue(index, _) => set_lua_boolean_upvalue(lua, function, index, value),
        }
    }

    fn restore(self, lua: &Lua, function: &Function) {
        let value = match self {
            Self::Global(_, value) | Self::Upvalue(_, value) => value,
        };
        let _ = self.set(lua, function, value);
    }
}

pub(in crate::lua) fn infer_luxe_flat_select_score_draw(
    lua: &Lua,
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let object_id = object_id?;
    let rank_destination = luxe_flat_select_rank_destination(object_id);
    let layout_destination =
        matches!(object_id, "default_playerdata_diff_count" | "default_playerdata_mode_diff_count");
    if rank_destination.is_none() && !layout_destination {
        return None;
    }

    let scorerate = LuaNumberSlot::scorerate(lua, function)?;
    let rank_plus = LuaBooleanSlot::rank_plus(lua, function)?;
    let flag_score = LuaBooleanSlot::flag_score(lua, function)?;
    let prepared = scorerate.set(lua, function, 0.0)
        && rank_plus.set(lua, function, true)
        && flag_score.set(lua, function, true);
    let inferred = if prepared {
        (|| {
            let mut observed = Vec::with_capacity(37);
            for ex_score in 0..=36 {
                main_state_probe.lock().ok()?.end_recording();
                if !scorerate.set(lua, function, f64::from(ex_score) / 36.0) {
                    return None;
                }
                observed.push(function.call::<bool>(()).ok()?);
            }
            if let Some((grade, sign)) = rank_destination {
                let expected = (0..=36)
                    .map(|ex_score| {
                        luxe_flat_nearest_rank(ex_score, 18).is_some_and(
                            |(actual_grade, actual_sign, _)| {
                                actual_grade == grade && actual_sign == sign
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                return (observed == expected)
                    .then(|| format!("select_score_available() and nearest_rank({grade},{sign})"));
            }
            for width in 1..=3 {
                let expected = (0..=36)
                    .map(|ex_score| match width {
                        3 => ex_score * 18 >= 36 * 15,
                        2 => ex_score * 18 >= 36 * 13 && ex_score * 18 < 36 * 15,
                        _ => ex_score * 18 < 36 * 13,
                    })
                    .collect::<Vec<_>>();
                if observed == expected {
                    return Some(format!(
                        "select_score_available() and nearest_rank_label_width({width})"
                    ));
                }
            }
            None
        })()
    } else {
        None
    };
    scorerate.restore(lua, function);
    rank_plus.restore(lua, function);
    flag_score.restore(lua, function);
    inferred
}

fn luxe_flat_select_rank_destination(id: &str) -> Option<(&'static str, &'static str)> {
    let suffix = id.strip_prefix("diff_rank_")?;
    let (grade, sign) =
        suffix.strip_suffix("_plus").map_or((suffix, "minus"), |grade| (grade, "plus"));
    let grade = match grade {
        "f" if sign == "plus" => "F",
        "e" => "E",
        "d" => "D",
        "c" => "C",
        "b" => "B",
        "a" => "A",
        "aa" => "AA",
        "aaa" => "AAA",
        "max" => "MAX",
        _ => return None,
    };
    Some((grade, sign))
}

pub(in crate::lua) fn infer_result_score_draw(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    match object_id? {
        "scoreGraph" => infer_score_rate_band(function, main_state_probe),
        id if id.starts_with("ir_scoreGraph") => {
            infer_ir_score_rate_band(function, id, main_state_probe)
        }
        id if modern_chic_ir_ranking_graph(id).is_some() => {
            infer_modern_chic_ir_score_rate_band(function, id, main_state_probe)
        }
        "irYouFrame" => infer_ir_ranking_user_draw(function, main_state_probe),
        "nextRankMinus" => {
            if verify_wmii_next_rank_stage_draw(function, main_state_probe, 8, true) {
                return Some("wmii_next_rank_stage(8)".to_string());
            }
            verify_wmii_next_rank_diff_nonzero_draw(function, main_state_probe)
                .then(|| "wmii_next_rank_diff_nonzero()".to_string())
        }
        "nextRankPlus" => verify_wmii_next_rank_diff_zero_draw(function, main_state_probe)
            .then(|| "wmii_next_rank_diff_zero()".to_string()),
        "diff_rank" => {
            if verify_wmii_next_rank_diff_nonzero_draw(function, main_state_probe) {
                return Some("wmii_next_rank_diff_nonzero()".to_string());
            }
            verify_wmii_next_rank_diff_zero_draw(function, main_state_probe)
                .then(|| "wmii_next_rank_diff_zero()".to_string())
        }
        id if id
            .strip_prefix("nextRank-")
            .and_then(|stage| stage.parse::<i32>().ok())
            .is_some() =>
        {
            let stage = id.strip_prefix("nextRank-")?.parse::<i32>().ok()?;
            if verify_wmii_next_rank_stage_draw(function, main_state_probe, stage, true) {
                return Some(format!("wmii_next_rank_stage({stage})"));
            }
            if stage == 0 && verify_wmii_next_rank_stage_draw(function, main_state_probe, 0, false)
            {
                return Some("wmii_next_rank_stage_no_max_minus(0)".to_string());
            }
            if stage == 0 && verify_wmii_next_rank_stage_draw(function, main_state_probe, 8, true) {
                return Some("wmii_next_rank_stage(8)".to_string());
            }
            None
        }
        id if id.starts_with("nextRank") => {
            let grade = id.strip_prefix("nextRank")?;
            for sign in ["plus", "minus"] {
                if verify_nearest_rank_draw(function, main_state_probe, Some(grade), sign) {
                    return Some(format!("nearest_rank({grade},{sign})"));
                }
            }
            None
        }
        id if luxe_flat_nearest_rank_destination(id).is_some() => {
            let (grade, sign) = luxe_flat_nearest_rank_destination(id)?;
            Some(format!("nearest_rank({grade},{sign})"))
        }
        "diff_plus" => verify_nearest_rank_draw(function, main_state_probe, None, "plus")
            .then(|| "nearest_rank_sign(plus)".to_string()),
        "diff_minus" => verify_nearest_rank_draw(function, main_state_probe, None, "minus")
            .then(|| "nearest_rank_sign(minus)".to_string()),
        _ => None,
    }
}

pub(in crate::lua) fn luxe_flat_nearest_rank_destination(
    id: &str,
) -> Option<(&'static str, &'static str)> {
    let suffix = id.strip_prefix("rank_diff_")?;
    let (grade, sign) = suffix.rsplit_once('_')?;
    let grade = match grade {
        "f" => "F",
        "e" => "E",
        "d" => "D",
        "c" => "C",
        "b" => "B",
        "a" => "A",
        "aa" => "AA",
        "aaa" => "AAA",
        "max" => "MAX",
        _ => return None,
    };
    let sign = match sign {
        "plus" => "plus",
        "minus" => "minus",
        _ => return None,
    };
    Some((grade, sign))
}
