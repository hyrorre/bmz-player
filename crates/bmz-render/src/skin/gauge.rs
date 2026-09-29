use super::*;

/// beatoraja `SkinGauge.ANIMATION_*` (JSON `gauge.type` フィールド)。
pub(super) const SKIN_GAUGE_ANIM_RANDOM: i32 = 0;
pub(super) const SKIN_GAUGE_ANIM_DECREASE: i32 = 2;
pub(super) const SKIN_GAUGE_ANIM_FLICKERING: i32 = 3;
pub(super) const SKIN_GAUGE_ANIM_INCREASE: i32 = 1;

/// beatoraja `SkinGauge.draw` の `exgauge = (type >= CLASS ? type - 3 : type) * 6`。
pub(super) fn skin_gauge_node_base(gameplay_gauge_type: i32) -> usize {
    let adjusted =
        if gameplay_gauge_type >= 6 { gameplay_gauge_type - 3 } else { gameplay_gauge_type };
    adjusted.max(0) as usize * 6
}

pub(super) fn skin_gauge_notes_count(gauge: f32, parts: i32, max: f32) -> i32 {
    if gauge > 0.0 { ((gauge * parts as f32 / max.max(1.0)) as i32).max(1) } else { 0 }
}

pub(super) fn skin_gauge_frame_color(frame: ResolvedSkinFrame) -> Color {
    Color::rgba(
        frame.r as f32 / 255.0,
        frame.g as f32 / 255.0,
        frame.b as f32 / 255.0,
        frame.a as f32 / 255.0,
    )
}

pub(super) fn skin_gauge_destination_blend(destination: &SkinDestinationDef) -> BlendMode {
    skin_blend_mode(destination.blend)
}

pub(super) fn skin_gauge_animation_index(gauge_def: &SkinGaugeDef, state: &SkinDrawState) -> i32 {
    let cycle = gauge_def.cycle.max(1);
    let range = gauge_def.range.max(0);
    match gauge_def.gauge_type {
        SKIN_GAUGE_ANIM_RANDOM => {
            state.gauge_random_indices.get(&gauge_def.id).copied().unwrap_or(0).clamp(0, range)
        }
        SKIN_GAUGE_ANIM_FLICKERING => {
            let time = state.play_timer_ms.unwrap_or(state.elapsed_ms);
            time.rem_euclid(cycle)
        }
        SKIN_GAUGE_ANIM_INCREASE => {
            let tick = skin_gauge_animation_tick(state, cycle);
            (tick * range).rem_euclid(range + 1)
        }
        SKIN_GAUGE_ANIM_DECREASE => {
            let tick = skin_gauge_animation_tick(state, cycle);
            tick.rem_euclid(range + 1)
        }
        _ => 0,
    }
}

pub(super) fn skin_gauge_animation_tick(state: &SkinDrawState, cycle: i32) -> i32 {
    let time = state.play_timer_ms.unwrap_or(state.elapsed_ms);
    time.div_euclid(cycle.max(1))
}

/// Maps one of beatoraja's 36 logical gauge slots to the source node supplied
/// by the skin. JSON skins may provide compact 4, 8, or 12 node layouts; the
/// reference loader expands those layouts through `JsonSkinObjectLoader`'s
/// `indexmap` before constructing `SkinGauge`.
pub(super) fn skin_gauge_source_node_index(logical_index: usize, node_count: usize) -> usize {
    const MAP_4: [usize; 36] = [
        0, 1, 2, 3, 0, 1, 0, 1, 2, 3, 0, 1, 0, 1, 2, 3, 0, 1, 0, 1, 2, 3, 0, 1, 0, 1, 2, 3, 0, 1,
        0, 1, 2, 3, 0, 1,
    ];
    const MAP_8: [usize; 36] = [
        4, 5, 6, 7, 4, 5, 4, 5, 6, 7, 4, 5, 0, 1, 2, 3, 0, 1, 0, 1, 2, 3, 0, 1, 4, 5, 6, 7, 4, 5,
        4, 5, 6, 7, 4, 5,
    ];
    const MAP_12: [usize; 36] = [
        4, 5, 6, 7, 10, 11, 4, 5, 6, 7, 10, 11, 0, 1, 2, 3, 8, 9, 0, 1, 2, 3, 8, 9, 4, 5, 6, 7, 10,
        11, 4, 5, 6, 7, 10, 11,
    ];

    match (node_count, logical_index) {
        (4, index @ 0..36) => MAP_4[index],
        (8, index @ 0..36) => MAP_8[index],
        (12, index @ 0..36) => MAP_12[index],
        _ => logical_index.min(node_count.saturating_sub(1)),
    }
}

/// beatoraja `SkinGauge.draw` のスプライト選択 (`exgauge + offset + underclear`)。
pub(super) fn skin_gauge_sprite_node_index(
    exgauge: usize,
    part: i32,
    notes: i32,
    animation: i32,
    border: f32,
    part_border: f32,
    node_count: usize,
    anim_type: i32,
) -> usize {
    let offset = if anim_type == SKIN_GAUGE_ANIM_FLICKERING {
        if notes >= part { 0 } else { 2 }
    } else if notes == part {
        4
    } else if notes - animation > part {
        0
    } else {
        2
    };
    let underclear = if part_border < border { 1 } else { 0 };
    skin_gauge_source_node_index(exgauge + offset + underclear, node_count)
}

pub(super) fn skin_gauge_flicker_tip_node_index(
    exgauge: usize,
    border: f32,
    part_border: f32,
    node_count: usize,
) -> Option<usize> {
    let underclear = if part_border < border { 1 } else { 0 };
    Some(skin_gauge_source_node_index(exgauge + 4 + underclear, node_count))
}

/// beatoraja `SkinGauge` FLICKERING の先端 α (`duration` = JSON `gauge.cycle`)。
///
/// `orgAlpha * (animation < duration/2 ? animation/(duration/2-1) : (duration-1-animation)/(duration/2-1))`
pub(super) fn skin_gauge_flicker_alpha(animation: i32, duration: i32) -> f32 {
    let duration = duration.max(1);
    let half = (duration / 2).max(1);
    let denom = (half - 1).max(1) as f32;
    if animation < half {
        animation as f32 / denom
    } else {
        ((duration - 1) - animation) as f32 / denom
    }
}

pub(super) fn skin_gauge_reverse_parts(frame: ResolvedSkinFrame) -> bool {
    if frame.w.abs() >= frame.h.abs() { frame.w < 0 } else { frame.h < 0 }
}

pub(super) fn skin_gauge_part_rect(rect: Rect, parts: i32, part: i32, reverse: bool) -> Rect {
    if rect.width.abs() >= rect.height.abs() {
        let part_width = rect.width / parts as f32;
        let x = if reverse {
            rect.x + rect.width - part_width * part as f32
        } else {
            rect.x + part_width * (part - 1) as f32
        };
        Rect { x, y: rect.y, width: part_width, height: rect.height }
    } else {
        let part_height = rect.height / parts as f32;
        let y = if reverse {
            rect.y + part_height * (part - 1) as f32
        } else {
            rect.y + rect.height - part_height * part as f32
        };
        Rect { x: rect.x, y, width: rect.width, height: part_height }
    }
}

pub(super) fn resolve_document_source<'a>(
    sources: &'a HashMap<String, SkinDocumentTexture>,
    src: &str,
) -> Option<&'a SkinDocumentTexture> {
    sources.get(src)
}

pub(super) fn destination_render_layer<'a>(
    timer: Option<i32>,
    after_notes_marker: bool,
    behind: &'a mut Vec<SkinRenderItem>,
    front: &'a mut Vec<SkinRenderItem>,
    failed_overlay: &'a mut Vec<SkinRenderItem>,
) -> &'a mut Vec<SkinRenderItem> {
    if timer == Some(3) {
        failed_overlay
    } else if after_notes_marker {
        front
    } else {
        behind
    }
}
