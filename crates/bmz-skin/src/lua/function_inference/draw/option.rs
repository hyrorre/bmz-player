use super::*;

pub(in crate::lua) fn infer_main_state_option_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_option_call_recording(true);
    }
    let _ = function.call::<Value>(()).ok();
    let calls = {
        let mut probe = main_state_probe.lock().ok()?;
        let calls = probe.option_calls.clone();
        probe.end_recording();
        calls
    };
    let option_id = single_number_call(&calls)?;
    let off = call_draw_with_option(function, main_state_probe, option_id, false)?;
    let on = call_draw_with_option(function, main_state_probe, option_id, true)?;
    match (off, on) {
        (false, true) => Some(format!("option({option_id})")),
        (true, false) => Some(format!("!option({option_id})")),
        _ => None,
    }
}

pub(in crate::lua) fn infer_main_state_option_number_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let option_id = single_number_call(&collect_option_calls(function, main_state_probe)?)?;
    let mut number_refs =
        collect_number_refs_with_option_value(function, main_state_probe, option_id, true)?;
    number_refs.extend(collect_number_refs_with_option_value(
        function,
        main_state_probe,
        option_id,
        false,
    )?);
    number_refs.sort_unstable();
    number_refs.dedup();
    if (2..=8).contains(&number_refs.len()) {
        let option_values = |value| BTreeMap::from([(option_id, value)]);
        let number_values = |active_ref: Option<i32>, value: i32| {
            number_refs
                .iter()
                .copied()
                .map(|ref_id| (ref_id, if active_ref == Some(ref_id) { value } else { 0 }))
                .collect::<BTreeMap<_, _>>()
        };
        for option_value in [false, true] {
            let all_zero = call_draw_with_numbers_and_options(
                function,
                main_state_probe,
                number_values(None, 0),
                option_values(option_value),
            )?;
            if all_zero {
                return None;
            }
            let all_positive = call_draw_with_numbers_and_options(
                function,
                main_state_probe,
                number_refs.iter().copied().map(|ref_id| (ref_id, 5)).collect(),
                option_values(option_value),
            )?;
            if all_positive != option_value {
                return None;
            }
            for ref_id in &number_refs {
                let positive = call_draw_with_numbers_and_options(
                    function,
                    main_state_probe,
                    number_values(Some(*ref_id), 5),
                    option_values(option_value),
                )?;
                let negative = call_draw_with_numbers_and_options(
                    function,
                    main_state_probe,
                    number_values(Some(*ref_id), -5),
                    option_values(option_value),
                )?;
                if positive != option_value || negative {
                    return None;
                }
            }
        }
        return Some(
            number_refs
                .iter()
                .map(|ref_id| format!("option({option_id}) and number({ref_id}) > 0"))
                .collect::<Vec<_>>()
                .join(" or "),
        );
    }
    let number_ref = single_number_call(&number_refs)?;

    let false_zero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 0, option_id, false)?;
    let false_nonzero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 5, option_id, false)?;
    let true_zero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 0, option_id, true)?;
    let true_nonzero =
        call_draw_with_number_option(function, main_state_probe, number_ref, 5, option_id, true)?;

    match (false_zero, false_nonzero, true_zero, true_nonzero) {
        (false, false, false, true) => {
            Some(format!("option({option_id}) && number({number_ref}) != 0"))
        }
        (false, false, true, false) => {
            Some(format!("option({option_id}) && number({number_ref}) == 0"))
        }
        (false, true, false, false) => {
            Some(format!("!option({option_id}) && number({number_ref}) != 0"))
        }
        (true, false, false, false) => {
            Some(format!("!option({option_id}) && number({number_ref}) == 0"))
        }
        _ => None,
    }
}

pub(in crate::lua) fn call_draw_with_option(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
    option_id: i32,
    value: bool,
) -> Option<bool> {
    {
        let mut probe = main_state_probe.lock().ok()?;
        probe.begin_option_recording_with_value(option_id, value);
        probe.text_calls.clear();
        probe.float_number_calls.clear();
        probe.os_clock_calls = 0;
    }
    let result = function.call::<Value>(()).ok();
    let option_only = {
        let mut probe = main_state_probe.lock().ok()?;
        let option_only = probe.option_calls.iter().all(|id| *id == option_id)
            && probe.number_calls.is_empty()
            && probe.timer_calls.is_empty()
            && probe.event_index_calls.is_empty()
            && probe.text_calls.is_empty()
            && probe.float_number_calls.is_empty()
            && probe.gauge_type_calls == 0
            && probe.os_clock_calls == 0;
        probe.end_recording();
        option_only
    };
    // Both short-circuit branches must depend exclusively on this option.
    // Otherwise a successful probe at number=0 can erase a numeric threshold.
    if !option_only {
        return None;
    }
    match result? {
        Value::Boolean(value) => Some(value),
        _ => None,
    }
}

pub(in crate::lua) fn infer_main_state_timer_option_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    {
        main_state_probe.lock().ok()?.begin_timer_option_call_recording();
    }
    let _ = function.call::<Value>(()).ok();
    let (timer_calls, option_calls) = {
        let mut probe = main_state_probe.lock().ok()?;
        let timer_calls = probe.timer_calls.clone();
        let option_calls = probe.option_calls.clone();
        probe.end_recording();
        (timer_calls, option_calls)
    };
    let timer_id = single_number_call(&timer_calls)?;
    let option_id = single_number_call(&option_calls)?;
    let samples =
        [(i32::MIN, false), (i32::MIN, true), (0, false), (0, true), (100, false), (100, true)];
    let observed = samples
        .iter()
        .map(|(timer_value, option_value)| {
            call_draw_with_timer_option(
                function,
                main_state_probe,
                timer_id,
                *timer_value,
                option_id,
                *option_value,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let candidates = [
        (
            format!("timer({timer_id}) == timer_off and option({option_id})"),
            samples
                .iter()
                .map(|(timer_value, option_value)| *timer_value == i32::MIN && *option_value)
                .collect::<Vec<_>>(),
        ),
        (
            format!("timer({timer_id}) != timer_off and option({option_id})"),
            samples
                .iter()
                .map(|(timer_value, option_value)| *timer_value != i32::MIN && *option_value)
                .collect::<Vec<_>>(),
        ),
        (
            format!("timer({timer_id}) > 0 and option({option_id})"),
            samples
                .iter()
                .map(|(timer_value, option_value)| *timer_value > 0 && *option_value)
                .collect::<Vec<_>>(),
        ),
    ];
    candidates
        .into_iter()
        .find_map(|(condition, expected)| (observed == expected).then_some(condition))
}

pub(in crate::lua) fn infer_main_state_two_options_timer_draw_condition(
    function: &Function,
    main_state_probe: &Arc<Mutex<MainStateProbe>>,
) -> Option<String> {
    let mut option_calls = collect_option_calls(function, main_state_probe)?;
    option_calls.sort_unstable();
    option_calls.dedup();
    if option_calls.len() != 2 {
        return None;
    }
    let option_a = option_calls[0];
    let option_b = option_calls[1];

    // Force both option branches open so a timer hidden behind Lua's short-circuit
    // evaluation is recorded as well.
    let timer_id = {
        let mut probe = main_state_probe.lock().ok()?;
        probe.begin_timer_options_recording_with_values(
            BTreeMap::new(),
            BTreeMap::from([(option_a, false), (option_b, true)]),
        );
        drop(probe);
        let _ = function.call::<Value>(()).ok();
        let mut probe = main_state_probe.lock().ok()?;
        let timer_calls = probe.timer_calls.clone();
        probe.end_recording();
        single_number_call(&timer_calls)?
    };

    let samples = [
        (false, false, i32::MIN),
        (false, false, 100),
        (false, true, i32::MIN),
        (false, true, 100),
        (true, false, i32::MIN),
        (true, false, 100),
        (true, true, i32::MIN),
        (true, true, 100),
    ];
    let observed = samples
        .iter()
        .map(|(a, b, timer)| {
            call_draw_with_timer_options(
                function,
                main_state_probe,
                timer_id,
                *timer,
                [(option_a, *a), (option_b, *b)],
            )
        })
        .collect::<Option<Vec<_>>>()?;
    let expected =
        samples.iter().map(|(a, b, timer)| *a || (*b && *timer == i32::MIN)).collect::<Vec<_>>();
    (observed == expected).then(|| {
        format!("option({option_a}) or option({option_b}) and timer({timer_id}) == timer_off")
    })
}
