use super::*;

pub(super) fn skin_image_texture_region(
    image: &SkinImageDef,
    source_size: SkinImageSize,
    elapsed_ms: i32,
) -> TextureRegion {
    skin_image_texture_region_with_elapsed(
        image,
        source_size,
        elapsed_ms,
        None,
        (image.x, image.y, image.w, image.h),
    )
}

pub(super) fn pre_ready_lane_cover_value_destination(
    destination: &SkinDestinationDef,
    value: &SkinValueDef,
    state: &SkinDrawState,
) -> bool {
    destination.timer == Some(40)
        && state.ready_timer_ms.is_none()
        && state.lane_cover_changing
        && destination.op.contains(&270)
        && skin_value_is_lane_cover_number(value)
}

pub(super) fn skin_value_is_lane_cover_number(value: &SkinValueDef) -> bool {
    matches!(value.ref_id, 14 | 312 | 313 | 1312..=1327)
        || skin_expr_references_lane_cover_number(&value.expr)
        || skin_expr_references_lane_cover_number(&value.value_expr)
}

pub(super) fn skin_expr_references_lane_cover_number(expr: &str) -> bool {
    ["number(14)", "number(312)", "number(313)"].iter().any(|needle| expr.contains(needle))
        || (1312..=1327).any(|ref_id| expr.contains(&format!("number({ref_id})")))
}

pub(super) fn skin_image_pixel_rect(image: &SkinImageDef) -> (i32, i32, i32, i32) {
    (image.x, image.y, image.w, image.h)
}

/// `image.ref_id` が指定されている場合、`SkinDrawState` から ref 値を引いて
/// 行インデックス（divy 方向）として使う。divx 方向は cycle 経過時間でアニメ。
/// ref 未指定なら従来通り全フレームを cycle で順次再生する。
pub(super) fn skin_image_texture_region_for_state(
    image: &SkinImageDef,
    source_size: SkinImageSize,
    state: &SkinDrawState,
    pixel_rect: (i32, i32, i32, i32),
) -> TextureRegion {
    // beatoraja の SkinImage は destination timer を座標補間だけに使い、
    // source image の cycle は scene 全体の時刻または image.timer で進める。
    // image.timer が off の間も画像自体は隠さず、先頭フレームを表示する。
    let elapsed_ms = skin_timer_elapsed_ms(image.timer, state).unwrap_or(0);
    skin_image_texture_region_with_elapsed(image, source_size, elapsed_ms, Some(state), pixel_rect)
}

fn skin_image_texture_region_with_elapsed(
    image: &SkinImageDef,
    source_size: SkinImageSize,
    elapsed_ms: i32,
    state: Option<&SkinDrawState>,
    pixel_rect: (i32, i32, i32, i32),
) -> TextureRegion {
    let source_width = source_size.width.max(1.0);
    let source_height = source_size.height.max(1.0);
    let (px, py, pw, ph) = resolve_skin_image_pixel_rect(pixel_rect, source_width, source_height);
    let divx = image.divx.max(1);
    let divy = image.divy.max(1);
    let frame_count = divx * divy;

    // ref_id / act が指定されている画像は「状態値 = 行」「cycle = 列のサブアニメ」と解釈する。
    // 値が解決できない場合 (state 未提供 or 値 None) は行 0 にフォールバックし、
    // 全フレームを順次再生する cycle モードへは落とさない（高速点滅を防ぐため）。
    let frame_index = if image.ref_id != 0 || image.act.is_some() {
        let row = state
            .and_then(|s| {
                if image.ref_id != 0 {
                    skin_image_ref_number(image.ref_id, s)
                } else {
                    image.act.map(|event_id| skin_state_event_index(event_id, s) as i64)
                }
            })
            .unwrap_or(0);
        let max_row = if image.len > 0 { image.len.min(divy) } else { divy };
        let row = row.clamp(0, (max_row - 1).max(0) as i64) as i32;
        let col = if image.cycle > 0 && divx > 1 {
            (elapsed_ms.rem_euclid(image.cycle) * divx / image.cycle).min(divx - 1)
        } else {
            0
        };
        row * divx + col
    } else if image.cycle > 0 && frame_count > 1 {
        (elapsed_ms.rem_euclid(image.cycle) * frame_count / image.cycle).min(frame_count - 1)
    } else {
        0
    };

    let cell_width = pw as f32 / divx as f32;
    let cell_height = ph as f32 / divy as f32;
    let source_column = frame_index % divx;
    let source_row = frame_index / divx;
    TextureRegion {
        x: (px as f32 + cell_width * source_column as f32) / source_width,
        y: (py as f32 + cell_height * source_row as f32) / source_height,
        width: cell_width / source_width,
        height: cell_height / source_height,
    }
}

pub(super) fn skin_image_ref_number(ref_id: i32, state: &SkinDrawState) -> Option<i64> {
    skin_image_index_number(ref_id, state)
}

pub(super) fn arrange_ref_index(state: &SkinDrawState) -> usize {
    if state.result_failed.is_some() {
        state.result_arrange_index
    } else {
        state.select_arrange_index
    }
}

pub(super) fn arrange_2p_ref_index(state: &SkinDrawState) -> usize {
    if state.result_failed.is_some() {
        state.result_arrange_2p_index
    } else {
        state.select_arrange_2p_index
    }
}

pub(super) fn extended_arrange_ref_index(state: &SkinDrawState) -> usize {
    if state.result_failed.is_some() {
        state.result_extended_arrange_index
    } else {
        state.select_extended_arrange_index
    }
}

pub(super) fn extended_arrange_2p_ref_index(state: &SkinDrawState) -> usize {
    if state.result_failed.is_some() {
        state.result_extended_arrange_2p_index
    } else {
        state.select_extended_arrange_2p_index
    }
}

pub(super) fn random_lane_ref_slot(ref_id: i32) -> Option<usize> {
    match ref_id {
        450..=466 | 469 => Some((ref_id - SKIN_RANDOM_LANE_REF_BASE) as usize),
        _ => None,
    }
}

pub(super) fn skin_random_lane_ref_number(ref_id: i32, state: &SkinDrawState) -> Option<i64> {
    let slot = random_lane_ref_slot(ref_id)?;
    Some(state.random_lane_refs[slot] as i64)
}

pub(super) fn resolve_skin_image_pixel_rect(
    pixel_rect: (i32, i32, i32, i32),
    source_width: f32,
    source_height: f32,
) -> (i32, i32, i32, i32) {
    let (px, py, pw, ph) = pixel_rect;
    let resolved_w =
        if pw < 0 { (source_width.round() as i32).saturating_sub(px).max(0) } else { pw };
    let resolved_h =
        if ph < 0 { (source_height.round() as i32).saturating_sub(py).max(0) } else { ph };
    (px, py, resolved_w, resolved_h)
}

pub(super) fn gauge_after_dot(gauge: f32) -> u32 {
    if gauge > 0.0 && gauge < 0.1 { 1 } else { ((gauge.max(0.0) * 10.0) as u32) % 10 }
}

pub(super) fn timing_afterdot(value: f32) -> i64 {
    let afterdot = ((value.abs() * 100.0) as i64) % 100;
    if value < 0.0 { -afterdot } else { afterdot }
}

pub(super) fn decimal_afterdot(value: f32) -> i64 {
    ((value.abs() * 100.0) as i64) % 100
}

pub(super) fn select_chart_normal_notes(state: &SkinDrawState) -> u32 {
    if state.select_chart_normal_notes > 0
        || state.select_chart_long_notes > 0
        || state.select_chart_scratch_notes > 0
        || state.select_chart_long_scratch_notes > 0
    {
        state.select_chart_normal_notes
    } else {
        state.select_total_notes
    }
}

pub(super) fn select_chart_main_bpm(state: &SkinDrawState) -> Option<f32> {
    (state.select_chart_main_bpm > 0.0).then_some(state.select_chart_main_bpm)
}

pub(super) fn current_bp(state: &SkinDrawState) -> u32 {
    if state.result_failed.is_some() {
        return state.result_bp.unwrap_or(state.judge_counts.bad + state.judge_counts.poor);
    }
    state.judge_counts.bad + state.judge_counts.poor
}

pub(super) fn current_cb(state: &SkinDrawState) -> u32 {
    if state.result_failed.is_some() {
        return state.result_cb.unwrap_or(state.judge_counts.bad + state.judge_counts.poor);
    }
    state.judge_counts.bad + state.judge_counts.poor
}

pub(super) fn result_mybest_bp(state: &SkinDrawState) -> Option<u32> {
    if state.result_failed.is_some() { state.previous_best_bp } else { state.best_bp }
}

pub(super) fn result_diff_misscount(state: &SkinDrawState) -> Option<i64> {
    state.result_failed?;
    let previous = result_mybest_bp(state)?;
    Some(i64::from(current_bp(state)) - i64::from(previous))
}

pub(super) fn result_mybest_ex_score(state: &SkinDrawState) -> Option<u32> {
    if state.result_failed.is_some() {
        state.previous_best_ex_score
    } else if state.play_screen && state.practice_mode {
        // beatoraja PracticePlayerはsetTargetScoreのbestを0で初期化する。
        Some(0)
    } else {
        state.best_ex_score
    }
}

pub(super) fn result_mybest_clear_index(state: &SkinDrawState) -> Option<i64> {
    if state.result_failed.is_some() {
        state.previous_best_clear_index
    } else {
        state.best_clear_index
    }
}

/// Result 画面の MYBEST 表示用。過去ベストが無い初プレイは 0 (NOPLAY) を返す。
pub(super) fn result_mybest_ex_score_display(state: &SkinDrawState) -> Option<u32> {
    result_mybest_ex_score(state).or_else(|| state.result_failed.is_some().then_some(0))
}

pub(super) fn result_mybest_clear_index_display(state: &SkinDrawState) -> Option<i64> {
    result_mybest_clear_index(state).or_else(|| state.result_failed.is_some().then_some(0))
}

pub(super) fn skin_point_score(state: &SkinDrawState) -> u32 {
    let total_notes = state.total_notes;
    if total_notes == 0 {
        return 0;
    }
    let counts = state.judge_counts;
    let numerator = match state.key_mode {
        KeyMode::K5 | KeyMode::K10 => {
            100_000_u64 * u64::from(counts.pgreat)
                + 100_000_u64 * u64::from(counts.great)
                + 50_000_u64 * u64::from(counts.good)
        }
        KeyMode::K7 | KeyMode::K14 | KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => {
            150_000_u64 * u64::from(counts.pgreat)
                + 100_000_u64 * u64::from(counts.great)
                + 20_000_u64 * u64::from(counts.good)
                + 50_000_u64 * u64::from(state.max_combo)
        }
        KeyMode::K9 => {
            100_000_u64 * u64::from(counts.pgreat)
                + 70_000_u64 * u64::from(counts.great)
                + 40_000_u64 * u64::from(counts.good)
        }
    };
    (numerator / u64::from(total_notes)).min(u64::from(u32::MAX)) as u32
}

pub(super) fn score_rate_cmp_value(ex_score: u32, total_notes: u32) -> u32 {
    if total_notes == 0 { 0 } else { ex_score.saturating_mul(1000) / total_notes.max(1) }
}
