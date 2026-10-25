use super::*;

pub(in crate::lua) fn infer_slider_value_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    match object_id {
        Some("adjustedcover") | Some("adjusted-cover") | Some("adjusted_cover") => {
            Some(SKIN_EXPR_ADJUSTED_COVER.to_string())
        }
        _ => infer_hsfix_dependent_float(function, main_state_probe)
            .map(|_| SKIN_EXPR_ADJUSTED_COVER.to_string()),
    }
}

pub(in crate::lua) fn infer_bmz_builtin_value_expr(
    function: &Function,
    object_id: Option<&str>,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    match object_id {
        Some("adjusted-rate-num") | Some("adjusted_rate_num") => {
            Some(SKIN_EXPR_ADJUSTED_RATE.to_string())
        }
        Some("adjusted-rate-adot-num") | Some("adjusted_rate_adot_num") => {
            Some(SKIN_EXPR_ADJUSTED_RATE_ADOT.to_string())
        }
        Some("threshold-num") | Some("threshold_num") | Some("fs-threshold") => {
            Some(SKIN_EXPR_FS_THRESHOLD.to_string())
        }
        Some("courseClearRate") | Some("course-clear-rate") | Some("course_clear_rate") => {
            Some(SKIN_EXPR_COURSE_CLEAR_RATE.to_string())
        }
        Some("val-gauge-percent-integer") => Some(SKIN_EXPR_GAUGE_PERCENT_INTEGER.to_string()),
        Some("val-gauge-percent-fraction") => Some(SKIN_EXPR_GAUGE_PERCENT_FRACTION.to_string()),
        Some("val-gauge-amount-integer") => Some(SKIN_EXPR_GAUGE_AMOUNT_INTEGER.to_string()),
        Some("val-gauge-amount-fraction") => Some(SKIN_EXPR_GAUGE_AMOUNT_FRACTION.to_string()),
        Some("tn_count") | Some("tn_dot_count") => {
            let refs = collect_number_refs(function, main_state_probe)?;
            if !refs.contains(&74) || !refs.contains(&368) {
                return None;
            }
            Some(
                if object_id == Some("tn_dot_count") {
                    SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_FRACTION
                } else {
                    SKIN_EXPR_SELECT_TOTAL_NOTES_RATIO_INTEGER
                }
                .to_string(),
            )
        }
        _ => {
            let refs = collect_number_refs(function, main_state_probe)?;
            if refs.iter().any(|ref_id| matches!(ref_id, 160 | 90 | 91 | 314 | 14)) {
                infer_hsfix_dependent_float(function, main_state_probe).map(|_| {
                    if object_id.is_some_and(|id| id.contains("adot") || id.contains("dot")) {
                        SKIN_EXPR_ADJUSTED_RATE_ADOT.to_string()
                    } else {
                        SKIN_EXPR_ADJUSTED_RATE.to_string()
                    }
                })
            } else if collect_option_calls(function, main_state_probe)
                .is_some_and(|options| options.iter().any(|option| (180..=183).contains(option)))
            {
                Some(SKIN_EXPR_FS_THRESHOLD.to_string())
            } else {
                None
            }
        }
    }
}

pub(in crate::lua) fn infer_hsfix_dependent_float(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<f64> {
    let number_refs = collect_number_refs(function, main_state_probe)?;
    let float_refs = collect_float_number_refs(function, main_state_probe)?;
    if number_refs.iter().any(|ref_id| matches!(ref_id, 160 | 90 | 91))
        || float_refs.iter().any(|ref_id| matches!(ref_id, 14 | 314))
    {
        Some(0.0)
    } else {
        None
    }
}

pub(in crate::lua) fn collect_option_calls(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<Vec<i32>> {
    {
        main_state_probe.lock().ok()?.begin_number_call_recording(0);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.option_calls.clone();
        probe.end_recording();
        calls
    };
    (!calls.is_empty()).then_some(calls)
}

/// optionの真偽で戻り値が変わる数値functionか。推論用VMのoptionは常にfalseのため、
/// 単一経路の推論ではoption ON時の分岐（例: BMZ Floating中だけ`number(1902)`を返す）を失う。
pub(in crate::lua) fn value_depends_on_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> bool {
    let Some(mut options) = collect_option_calls(function, main_state_probe) else {
        return false;
    };
    options.sort_unstable();
    options.dedup();
    if options.len() > 12 {
        return false;
    }
    let base_refs = collect_number_refs(function, main_state_probe).unwrap_or_default();
    options.iter().any(|&option_id| {
        let mut refs = base_refs.clone();
        refs.extend(
            collect_number_refs_with_option(function, main_state_probe, option_id)
                .unwrap_or_default(),
        );
        // 0では `0.6*number(a)` と `number(b)` が一致するため、refごとに異なる値で比べる。
        let values = refs
            .into_iter()
            .map(|ref_id| (ref_id, ref_id.rem_euclid(97) + 3))
            .collect::<BTreeMap<_, _>>();
        let off = call_number_float_with_values_and_options(
            function,
            main_state_probe,
            values.clone(),
            BTreeMap::new(),
        );
        let on = call_number_float_with_values_and_options(
            function,
            main_state_probe,
            values,
            BTreeMap::from([(option_id, true)]),
        );
        match (off, on) {
            (Some(off), Some(on)) => !approx_float_eq(off, on),
            (None, None) => false,
            _ => true,
        }
    })
}

pub(in crate::lua) fn infer_value_float_expr(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    infer_remain_rate_scaled(function, main_state_probe)
        .or_else(|| infer_number_scalar_multiply(function, main_state_probe))
        .or_else(|| infer_option_weighted_number_sum(function, main_state_probe))
        .or_else(|| infer_weighted_number_ratio_scaled(function, main_state_probe))
        .or_else(|| infer_division_of_number_sums(function, main_state_probe))
}

pub(in crate::lua) const REMAIN_NOTE_REFS: [i32; 6] = [106, 110, 111, 112, 113, 114];

pub(in crate::lua) fn remain_notes_numerator_expr() -> String {
    "number(106)-number(110)-number(111)-number(112)-number(113)-number(114)".to_string()
}

pub(in crate::lua) fn remain_notes_value(values: &BTreeMap<i32, i32>) -> i32 {
    REMAIN_NOTE_REFS
        .iter()
        .map(|ref_id| {
            let value = values.get(ref_id).copied().unwrap_or(0);
            if *ref_id == 106 { value } else { -value }
        })
        .sum()
}

pub(in crate::lua) fn infer_remain_rate_scaled(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() != 6 || !refs.iter().all(|ref_id| REMAIN_NOTE_REFS.contains(ref_id)) {
        return None;
    }
    let mut probe_values = BTreeMap::from([(106, 10)]);
    for ref_id in REMAIN_NOTE_REFS {
        probe_values.entry(ref_id).or_insert(0);
    }
    let scale_sample =
        call_number_float_with_values(function, main_state_probe, probe_values.clone())?;
    let scale = scale_sample.round();
    if (scale - 100.0).abs() > 0.5 && (scale - 10000.0).abs() > 0.5 {
        return None;
    }
    let numerator = remain_notes_numerator_expr();
    let expr = format!("({numerator})/number(106)*{}", scale as i64);
    let expected = |values: &BTreeMap<i32, i32>| {
        let remain: f64 = REMAIN_NOTE_REFS
            .iter()
            .map(|ref_id| {
                let value = values.get(ref_id).copied().unwrap_or(0) as f64;
                if *ref_id == 106 { value } else { -value }
            })
            .sum();
        let total = values.get(&106).copied().unwrap_or(0) as f64;
        if total.abs() < f64::EPSILON { 0.0 } else { remain / total * scale }
    };
    for test_values in [
        probe_values.clone(),
        BTreeMap::from([(106, 20), (110, 5)]),
        BTreeMap::from([(106, 30), (110, 10), (111, 5)]),
    ] {
        let actual =
            call_number_float_with_values(function, main_state_probe, test_values.clone())?;
        if !approx_float_eq(actual, expected(&test_values)) {
            return None;
        }
    }
    Some(expr)
}

pub(in crate::lua) fn infer_number_scalar_multiply(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() != 1 {
        return None;
    }
    let ref_id = refs[0];
    let baseline = call_number_float_with_values(function, main_state_probe, BTreeMap::new())?;
    let at_one =
        call_number_float_with_values(function, main_state_probe, BTreeMap::from([(ref_id, 1)]))?;
    let coefficient = at_one - baseline;
    if coefficient.abs() < f64::EPSILON {
        return None;
    }
    let at_three =
        call_number_float_with_values(function, main_state_probe, BTreeMap::from([(ref_id, 3)]))?;
    if !approx_float_eq(at_three - baseline, coefficient * 3.0) {
        return None;
    }
    Some(format!("{coefficient}*number({ref_id})"))
}

pub(in crate::lua) fn infer_option_weighted_number_sum(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let options = collect_option_calls(function, main_state_probe)?;
    if options.is_empty() || options.len() > 12 {
        return None;
    }

    let mut refs = Vec::new();
    for option_id in &options {
        refs.extend(collect_number_refs_with_option(function, main_state_probe, *option_id)?);
    }
    refs.sort_unstable();
    refs.dedup();
    if refs.is_empty() || refs.len() > 16 {
        return None;
    }

    let mut terms = Vec::new();
    for option_id in &options {
        let option_values = BTreeMap::from([(*option_id, true)]);
        let zero_values = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect();
        let baseline = call_number_float_with_values_and_options(
            function,
            main_state_probe,
            zero_values,
            option_values.clone(),
        )?;
        for ref_id in &refs {
            let mut values = refs.iter().copied().map(|id| (id, 0)).collect::<BTreeMap<_, _>>();
            values.insert(*ref_id, 1);
            let at_one = call_number_float_with_values_and_options(
                function,
                main_state_probe,
                values,
                option_values.clone(),
            )?;
            let coefficient = at_one - baseline;
            if coefficient.abs() > f64::EPSILON {
                terms.push(format!("{coefficient}*option({option_id})*number({ref_id})"));
            }
        }
    }
    if terms.is_empty() {
        return None;
    }

    for option_id in &options {
        let option_values = BTreeMap::from([(*option_id, true)]);
        for sample in [1, 3, 7] {
            let values = refs.iter().copied().map(|ref_id| (ref_id, sample)).collect();
            let actual = call_number_float_with_values_and_options(
                function,
                main_state_probe,
                values,
                option_values.clone(),
            )?;
            let expected = evaluate_option_weighted_number_terms(
                &terms,
                *option_id,
                &refs.iter().copied().map(|ref_id| (ref_id, sample)).collect(),
            )?;
            if !approx_float_eq(actual, expected) {
                return None;
            }
        }
    }

    Some(terms.join("+"))
}

/// `infer_option_weighted_number_sum` は1つのoptionだけがONの状態で係数を求める。
/// 全option OFFでは式が0になるため、実際の戻り値も0の場合だけ採用する。
pub(in crate::lua) fn infer_option_weighted_number_sum_with_all_off(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let expr = infer_option_weighted_number_sum(function, main_state_probe)?;
    let refs = collect_number_refs(function, main_state_probe)?;
    for sample in [1, 3, 7] {
        let values = refs.iter().copied().map(|ref_id| (ref_id, sample)).collect();
        let actual = call_number_float_with_values_and_options(
            function,
            main_state_probe,
            values,
            BTreeMap::new(),
        )?;
        if !approx_float_eq(actual, 0.0) {
            return None;
        }
    }
    Some(expr)
}

pub(in crate::lua) fn evaluate_option_weighted_number_terms(
    terms: &[String],
    active_option: i32,
    values: &BTreeMap<i32, i32>,
) -> Option<f64> {
    let mut total = 0.0;
    for term in terms {
        let mut factors = term.split('*');
        let coefficient = factors.next()?.parse::<f64>().ok()?;
        let option = factors.next()?.trim();
        let number = factors.next()?.trim();
        if factors.next().is_some() {
            return None;
        }
        let option_id = option.strip_prefix("option(")?.strip_suffix(')')?.parse::<i32>().ok()?;
        let ref_id = number.strip_prefix("number(")?.strip_suffix(')')?.parse::<i32>().ok()?;
        if option_id == active_option {
            total += coefficient * f64::from(values.get(&ref_id).copied().unwrap_or(0));
        }
    }
    Some(total)
}

pub(in crate::lua) fn infer_weighted_number_ratio_scaled(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() < 2 || refs.len() > 16 {
        return None;
    }
    refs.iter().find_map(|denominator_ref| {
        infer_weighted_number_ratio_scaled_with_denominator(
            function,
            main_state_probe,
            &refs,
            *denominator_ref,
        )
    })
}

pub(in crate::lua) fn infer_weighted_number_ratio_scaled_with_denominator(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    refs: &[i32],
    denominator_ref: i32,
) -> Option<String> {
    const PROBE_DENOMINATOR: i32 = 1000;
    let mut base_values =
        refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>();
    base_values.insert(denominator_ref, PROBE_DENOMINATOR);
    let baseline = call_number_float_with_values(function, main_state_probe, base_values.clone())?;
    if !approx_float_eq(baseline, 0.0) {
        return None;
    }

    let mut terms = Vec::new();
    for ref_id in refs.iter().copied().filter(|ref_id| *ref_id != denominator_ref) {
        let mut values = base_values.clone();
        values.insert(ref_id, 1);
        let at_one = call_number_float_with_values(function, main_state_probe, values)?;
        if at_one - baseline < 1.0 {
            continue;
        }
        let coefficient = ((at_one - baseline) * f64::from(PROBE_DENOMINATOR)).round() as i64;
        if coefficient <= 0 {
            continue;
        }
        terms.push((ref_id, coefficient));
    }
    if terms.is_empty() {
        return None;
    }

    let test_cases = [
        refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>(),
        terms
            .iter()
            .map(|(ref_id, _)| (*ref_id, 1))
            .chain(std::iter::once((denominator_ref, PROBE_DENOMINATOR)))
            .collect::<BTreeMap<_, _>>(),
        terms
            .iter()
            .map(|(ref_id, _)| (*ref_id, 3))
            .chain(std::iter::once((denominator_ref, PROBE_DENOMINATOR)))
            .collect::<BTreeMap<_, _>>(),
        terms
            .iter()
            .map(|(ref_id, _)| (*ref_id, 1))
            .chain(std::iter::once((denominator_ref, 74)))
            .collect::<BTreeMap<_, _>>(),
    ];
    for values in test_cases {
        let expected = weighted_ratio_floor(&terms, denominator_ref, &values) as f64;
        let actual = match call_number_float_with_values(function, main_state_probe, values) {
            Some(value) if value.is_finite() => value,
            _ if expected.abs() < f64::EPSILON => 0.0,
            _ => return None,
        };
        if !approx_float_eq(actual, expected) {
            return None;
        }
    }

    let numerator = terms
        .iter()
        .map(|(ref_id, coefficient)| {
            if *coefficient == 1 {
                format!("number({ref_id})")
            } else {
                format!("{coefficient}*number({ref_id})")
            }
        })
        .collect::<Vec<_>>()
        .join("+");
    Some(format!("floor(({numerator})/number({denominator_ref}))"))
}

pub(in crate::lua) fn weighted_ratio_floor(
    terms: &[(i32, i64)],
    denominator_ref: i32,
    values: &BTreeMap<i32, i32>,
) -> i64 {
    let denominator = values.get(&denominator_ref).copied().unwrap_or(0);
    if denominator <= 0 {
        return 0;
    }
    let numerator = terms
        .iter()
        .map(|(ref_id, coefficient)| {
            coefficient.saturating_mul(i64::from(values.get(ref_id).copied().unwrap_or(0)))
        })
        .sum::<i64>();
    numerator / i64::from(denominator)
}

pub(in crate::lua) fn fast_slow_ref_set() -> BTreeMap<i32, ()> {
    FAST_SLOW_FAST_REFS.into_iter().chain(FAST_SLOW_SLOW_REFS).map(|ref_id| (ref_id, ())).collect()
}

pub(in crate::lua) fn infer_fast_slow_ratio_graph_type(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<i32> {
    let refs = collect_number_refs(function, main_state_probe)?;
    let expected = fast_slow_ref_set();
    if refs.len() != expected.len() || !refs.iter().all(|ref_id| expected.contains_key(ref_id)) {
        return None;
    }
    let fast_set: BTreeMap<i32, ()> =
        FAST_SLOW_FAST_REFS.into_iter().map(|ref_id| (ref_id, ())).collect();
    let slow_set: BTreeMap<i32, ()> =
        FAST_SLOW_SLOW_REFS.into_iter().map(|ref_id| (ref_id, ())).collect();
    if verify_fast_slow_ratio(function, main_state_probe, &refs, &fast_set) {
        return Some(148);
    }
    if verify_fast_slow_ratio(function, main_state_probe, &refs, &slow_set) {
        return Some(149);
    }
    None
}

pub(in crate::lua) fn approx_float_eq(actual: f64, expected: f64) -> bool {
    if expected.abs() < f64::EPSILON && (!actual.is_finite() || actual.abs() < f64::EPSILON) {
        return true;
    }
    (actual - expected).abs() <= 0.02
}

pub(in crate::lua) fn verify_fast_slow_ratio(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    refs: &[i32],
    numerator_refs: &BTreeMap<i32, ()>,
) -> bool {
    let ratio = |values: &BTreeMap<i32, i32>| {
        let num: f64 = numerator_refs
            .keys()
            .map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64)
            .sum();
        let den: f64 =
            refs.iter().map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64).sum();
        if den.abs() < f64::EPSILON { 0.0 } else { num / den }
    };
    let all_zero: BTreeMap<i32, i32> = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect();
    let all_one: BTreeMap<i32, i32> = refs.iter().copied().map(|ref_id| (ref_id, 1)).collect();
    let mut numerator_only = all_zero.clone();
    for ref_id in numerator_refs.keys() {
        numerator_only.insert(*ref_id, 5);
    }
    let mut complement_only =
        refs.iter().copied().map(|ref_id| (ref_id, 5)).collect::<BTreeMap<_, _>>();
    for ref_id in numerator_refs.keys() {
        complement_only.insert(*ref_id, 0);
    }
    let ratio_all_one = ratio(&all_one);
    let ratio_numerator_only = ratio(&numerator_only);
    let ratio_complement_only = ratio(&complement_only);
    for (values, expected) in [
        (all_zero, 0.0),
        (all_one, ratio_all_one),
        (numerator_only, ratio_numerator_only),
        (complement_only, ratio_complement_only),
    ] {
        let actual = match call_number_float_with_values(function, main_state_probe, values) {
            Some(value) if value.is_finite() => value,
            _ if expected.abs() < f64::EPSILON => 0.0,
            _ => return false,
        };
        if !approx_float_eq(actual, expected) {
            return false;
        }
    }
    true
}

pub(in crate::lua) fn infer_division_of_number_sums(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let refs = collect_number_refs(function, main_state_probe)?;
    if refs.len() < 2 || refs.len() > 24 {
        return None;
    }
    let zero_values = refs.iter().copied().map(|ref_id| (ref_id, 0)).collect::<BTreeMap<_, _>>();
    // Lua の 0/0 は NaN になる。beatoraja の graph 描画では非有限値が実質0幅に
    // なるため、比率推論でも全ゼロ入力だけは0として扱う。
    let baseline =
        call_number_float_raw_with_values(function, main_state_probe, zero_values.clone())?;
    let baseline = if baseline.is_finite() { baseline } else { 0.0 };
    let mut numerator_refs = Vec::new();
    for ref_id in &refs {
        let mut values = zero_values.clone();
        values.insert(*ref_id, 5);
        let value = call_number_float_with_values(function, main_state_probe, values)?;
        if value > baseline + f64::EPSILON {
            numerator_refs.push(*ref_id);
        }
    }
    if numerator_refs.is_empty() {
        return None;
    }
    let numerator = format_number_sum_expr(&numerator_refs);
    let denominator = format_number_sum_expr(&refs);
    let expr = format!("({numerator})/({denominator})");
    let expected_ratio = |values: &BTreeMap<i32, i32>| {
        let num: f64 = numerator_refs
            .iter()
            .map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64)
            .sum();
        let den: f64 =
            refs.iter().map(|ref_id| values.get(ref_id).copied().unwrap_or(0) as f64).sum();
        if den.abs() < f64::EPSILON { 0.0 } else { num / den }
    };
    let mut numerator_only = zero_values.clone();
    for ref_id in &numerator_refs {
        numerator_only.insert(*ref_id, 5);
    }
    let mut denominator_only =
        refs.iter().copied().map(|ref_id| (ref_id, 5)).collect::<BTreeMap<_, _>>();
    for ref_id in &numerator_refs {
        denominator_only.insert(*ref_id, 0);
    }
    let test_cases = [
        zero_values,
        refs.iter().copied().map(|id| (id, 1)).collect(),
        refs.iter().copied().map(|id| (id, 3)).collect(),
        numerator_only,
        denominator_only,
    ];
    for values in test_cases {
        let expected = expected_ratio(&values);
        let actual = call_number_float_raw_with_values(function, main_state_probe, values)?;
        let actual = if actual.is_finite() {
            actual
        } else if expected.abs() < f64::EPSILON {
            0.0
        } else {
            return None;
        };
        if !approx_float_eq(actual, expected) {
            return None;
        }
    }
    Some(expr)
}
