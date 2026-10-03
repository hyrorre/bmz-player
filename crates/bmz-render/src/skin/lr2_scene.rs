use super::*;

pub(super) fn button_index(button: i32, state: &SkinDrawState) -> i32 {
    let optional = |value: Option<usize>| value.map_or(-1, |v| v as i32);
    match button {
        1..=8 => i32::from(i32::from(state.select_option_panel) == button),
        40 => optional(lr2_gauge_index(if state.select_screen {
            state.select_gauge_index
        } else {
            state.gauge_type.max(0) as usize
        })),
        41 => {
            state.opponent_gauge_type.map_or(-1, |v| optional(lr2_gauge_index(v.max(0) as usize)))
        }
        42 => optional(lr2_arrange_index(state.select_extended_arrange_index)),
        43 => optional(lr2_arrange_index(state.select_extended_arrange_2p_index)),
        54 => match state.select_double_option_index {
            0 | 1 => state.select_double_option_index as i32,
            _ => -1,
        },
        55 => match state.skin_attempt.hsfix_index.unwrap_or(state.select_hs_fix_index) {
            0 => 0,
            2 => 1,
            4 => 2,
            3 => 5,
            _ => -1,
        },
        11 => state.lr2_select_mode_index,
        12 => match state.select_sort_index {
            0 => 2,
            4 => 1,
            5 => 3,
            6 => 4,
            _ => -1,
        },
        10 => skin_state_event_index(button, state),
        72 => match state.select_bga_index {
            0 => 1,
            1 => 2,
            2 => 0,
            _ => -1,
        },
        74 => 0,
        91..=96 => i32::from(state.select_difficulty_filter_index == (button - 91) as usize),
        13..=19 | 57 | 58 => 0,
        _ => -1,
    }
}

pub(super) fn chart_items(
    document: &SkinDocument,
    chart: &SkinLr2ChartDef,
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    context: &DestinationResolveContext<'_, '_>,
) -> Vec<SkinRenderItem> {
    let state = context.state;
    // A flipped single-player Result uses its 2P graph destination for the player.
    let player = i32::from(document.lr2_result.as_ref().is_some_and(|r| r.flip));
    if (!chart.score && chart.player != player) || (chart.score && chart.index != 0) {
        // No comparison curve is synthesized from a final score.
        return Vec::new();
    }
    let Some(image) = context.images.get(chart.id.as_str()) else {
        return Vec::new();
    };
    let Some(source) = resolve_document_source(context.sources, &image.src) else {
        return Vec::new();
    };
    let gauges = context
        .runtime_graphs
        .result_gauge_graph_points
        .iter()
        .filter(|point| point.gauge_type == state.gauge_type)
        .collect::<Vec<_>>();
    let judges = context.runtime_graphs.result_judge_graph_buckets;
    if chart.width <= 0
        || (chart.score && (judges.is_empty() || state.total_notes == 0))
        || (!chart.score && gauges.is_empty())
    {
        return Vec::new();
    }
    let end = state.result_graph_end_ms.map(|age| state.elapsed_ms - age).unwrap_or(chart.end);
    let progress = if end <= chart.start {
        1.0
    } else {
        ((state.elapsed_ms - chart.start) as f32 / (end - chart.start) as f32).clamp(0.0, 1.0)
    };
    let width = chart.width.min(16384);
    let step = frame.w.max(1);
    let uv = skin_image_texture_region(image, source.source_size, state.elapsed_ms);
    let mut cumulative = Vec::new();
    if chart.score {
        let mut score = 0_u64;
        for bucket in judges {
            score += u64::from(bucket.values[1]) * 2 + u64::from(bucket.values[2]);
            cumulative.push(score as f32 / (state.total_notes as f32 * 2.0));
        }
    }
    let mut items = Vec::new();
    let mut previous_y = None;
    let duration = state.result_duration_ms.max(gauges.last().map_or(0, |p| p.time_ms)).max(1);
    for x in (0..(width as f32 * progress).ceil() as i32).step_by(step as usize) {
        let ratio = (x as f32 / (width - step).max(1) as f32).clamp(0.0, 1.0);
        let value = if chart.score {
            cumulative[((ratio * cumulative.len() as f32) as usize).min(cumulative.len() - 1)]
                .clamp(0.0, 1.0)
        } else {
            let time = (ratio * duration as f32) as i32;
            let index = gauges.partition_point(|point| point.time_ms <= time).saturating_sub(1);
            let point = gauges[index];
            (point.value / point.max.max(1.0)).clamp(0.0, 1.0)
        };
        let y = (value * chart.height as f32).round() as i32;
        let old_y = previous_y.replace(y).unwrap_or(y);
        let below_border =
            state.course_result.stage_count == 0 && state.gauge_type < 3 && value < 0.8;
        if !chart.score && chart.index != i32::from(!below_border) {
            continue;
        }
        let segment = ResolvedSkinFrame {
            x: frame.x + x,
            y: frame.y + y.min(old_y),
            h: frame.h.max(1) + (y - old_y).abs(),
            ..frame
        };
        items.push(skin_image_item_for_frame(
            source.texture,
            normalize_skin_frame_rect(segment, document.w, document.h),
            uv,
            segment,
            destination.center,
            skin_blend_mode(destination.blend),
            Some(source.source_size),
            destination.filter != 0,
        ));
    }
    items
}
