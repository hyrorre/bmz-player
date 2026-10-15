use super::*;

pub(in crate::skin) fn skin_hispeed_mode_index(state: &SkinDrawState) -> i32 {
    state.hispeed_mode_index.clamp(0, 1)
}

pub(in crate::skin) fn skin_hispeed_mode_is_floating(state: &SkinDrawState) -> bool {
    skin_hispeed_mode_index(state) == 1
}

pub(in crate::skin) fn skin_base_hispeed_index(state: &SkinDrawState) -> i32 {
    state.base_hispeed_index.clamp(0, 1)
}

pub(in crate::skin) fn skin_normal_hispeed_level(state: &SkinDrawState) -> i64 {
    i64::from(state.normal_hispeed_level.clamp(1, 20))
}

pub(in crate::skin) fn skin_hispeed_config_index(state: &SkinDrawState) -> i32 {
    state.hispeed_config_index.clamp(0, 5)
}

pub(in crate::skin) fn skin_target_green_number(state: &SkinDrawState) -> i64 {
    if skin_hispeed_mode_is_floating(state) && state.target_green_number > 0 {
        i64::from(state.target_green_number)
    } else {
        state_duration_green_number_ms(state)
    }
}

pub(in crate::skin) fn lane_cover_duration_number(
    ref_id: i32,
    state: &SkinDrawState,
) -> Option<i64> {
    if state.select_screen && !duration_refs_available(state) {
        return None;
    }
    let offset = ref_id.checked_sub(1312)?;
    let green = offset % 2 == 1;
    let cover = offset % 4 < 2;
    let mode = offset / 4;
    let duration = if mode == 0 {
        current_lane_cover_duration_number_ms(cover, state)
            .or_else(|| lane_cover_duration_number_ms_for_bpm(cover, mode, state))?
    } else {
        lane_cover_duration_number_ms_for_bpm(cover, mode, state)?
    };
    Some(if green { duration_to_green_number_ms_i64(duration) } else { duration })
}

pub(in crate::skin) fn current_lane_cover_duration_number_ms(
    cover: bool,
    state: &SkinDrawState,
) -> Option<i64> {
    if !duration_refs_available(state) {
        return None;
    }
    let cover_on_visible = bmz_visible_lane_fraction(state.lane_cover, state.lift);
    let target_visible =
        if cover { cover_on_visible } else { bmz_visible_lane_fraction(0.0, state.lift) };
    let duration = state.total_duration_ms.max(0) as i64;
    let duration = if duration > 0 { duration } else { state_duration_number_ms(state) };
    if cover {
        return Some(duration);
    }
    if cover_on_visible > f32::EPSILON {
        return Some(
            (duration.max(0) as f64 * target_visible as f64 / cover_on_visible as f64)
                .round()
                .max(0.0) as i64,
        );
    }
    None
}

pub(in crate::skin) fn lane_cover_duration_number_ms_for_bpm(
    cover: bool,
    mode: i32,
    state: &SkinDrawState,
) -> Option<i64> {
    let target_bpm = match mode {
        0 => bpm_value_or_select(state.now_bpm, state.select_bpm),
        1 => bpm_value_or_select(state.main_bpm, state.select_bpm),
        2 => bpm_value_or_select(state.min_bpm, state.select_min_bpm),
        3 => bpm_value_or_select(state.max_bpm, state.select_max_bpm),
        _ => return None,
    }?;
    let visible = if cover {
        bmz_visible_lane_fraction(state.lane_cover, state.lift)
    } else {
        bmz_visible_lane_fraction(0.0, state.lift)
    };
    let duration = 240_000.0 / target_bpm as f64 / state.hispeed.max(0.01) as f64 * visible as f64;
    Some(duration.round().max(0.0) as i64)
}

pub(in crate::skin) fn bmz_lane_cover_for_lift(lane_cover: f32, lift: f32) -> f32 {
    lane_cover.clamp(0.0, (1.0 - lift.clamp(0.0, 1.0)).clamp(0.0, 1.0))
}

pub(in crate::skin) fn bmz_visible_lane_fraction(lane_cover: f32, lift: f32) -> f32 {
    (1.0 - bmz_lane_cover_for_lift(lane_cover, lift) - lift.clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

pub(in crate::skin) fn duration_refs_available(state: &SkinDrawState) -> bool {
    state.duration_green_ms.is_some_and(|value| value > 0) || state.total_duration_ms > 0
}

pub(in crate::skin) fn state_duration_number_ms(state: &SkinDrawState) -> i64 {
    if state.total_duration_ms > 0 {
        i64::from(state.total_duration_ms)
    } else {
        state.duration_green_ms.map(green_duration_to_duration).unwrap_or(0)
    }
}

pub(in crate::skin) fn state_duration_green_number_ms(state: &SkinDrawState) -> i64 {
    state
        .duration_green_ms
        .map(|value| value.max(0) as i64)
        .unwrap_or_else(|| duration_to_green_number_ms(state.total_duration_ms) as i64)
}

pub(in crate::skin) fn green_duration_to_duration(green_duration_ms: i32) -> i64 {
    let green = green_duration_ms.max(0) as i64;
    (green.saturating_mul(5).saturating_add(1)) / 3
}

pub fn green_duration_to_duration_i32(green_duration_ms: i32) -> i32 {
    green_duration_to_duration(green_duration_ms).min(i32::MAX as i64) as i32
}

pub fn duration_to_green_number_ms(duration_ms: i32) -> i32 {
    let duration = duration_ms.max(0) as i64;
    duration_to_green_number_ms_i64(duration) as i32
}

pub(in crate::skin) fn duration_to_green_number_ms_i64(duration_ms: i64) -> i64 {
    duration_ms.max(0).saturating_mul(3).saturating_add(2).saturating_div(5).min(i32::MAX as i64)
}

pub(in crate::skin) fn bpm_value_or_select(value: f32, fallback: f32) -> Option<f32> {
    if value > 0.0 {
        Some(value)
    } else if fallback > 0.0 {
        Some(fallback)
    } else {
        None
    }
}
