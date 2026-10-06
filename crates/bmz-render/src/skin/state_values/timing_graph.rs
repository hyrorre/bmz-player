use super::*;

pub(super) fn timing_visualizer_judge_colors(visualizer: &SkinTimingVisualizerDef) -> [Color; 5] {
    [
        timing_color(&visualizer.pg_color, 1.0),
        timing_color(&visualizer.gr_color, 1.0),
        timing_color(&visualizer.gd_color, 1.0),
        timing_color(&visualizer.bd_color, 1.0),
        if visualizer.transparent == 1 {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        } else {
            timing_color(&visualizer.pr_color, 1.0)
        },
    ]
}

pub(super) fn timing_distribution_judge_colors(
    graph: &SkinTimingDistributionGraphDef,
) -> [Color; 5] {
    [
        timing_color(&graph.pg_color, 1.0),
        timing_color(&graph.gr_color, 1.0),
        timing_color(&graph.gd_color, 1.0),
        timing_color(&graph.bd_color, 1.0),
        timing_color(&graph.pr_color, 1.0),
    ]
}

pub(super) fn judge_timing_color(
    judge: Judge,
    visualizer: &SkinTimingVisualizerDef,
    fallback: Color,
) -> Color {
    match judge {
        Judge::PGreat => timing_color(&visualizer.pg_color, 1.0),
        Judge::Great => timing_color(&visualizer.gr_color, 1.0),
        Judge::Good => timing_color(&visualizer.gd_color, 1.0),
        Judge::Bad => timing_color(&visualizer.bd_color, 1.0),
        Judge::Poor | Judge::EmptyPoor if visualizer.transparent == 1 => {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        }
        Judge::Poor | Judge::EmptyPoor => timing_color(&visualizer.pr_color, 1.0),
    }
    .with_alpha(fallback.a)
}

pub(super) fn timing_judge_band_items(
    rect: Rect,
    center_ms: f32,
    frame_alpha: f32,
    blend: BlendMode,
    colors: [Color; 5],
    state: &SkinDrawState,
) -> Vec<SkinRenderItem> {
    let areas = timing_judge_areas(state);
    let mut items = Vec::new();
    let mut inner_late_ms = 0.0;
    let mut inner_early_ms = 0.0;
    for (area, color) in areas.into_iter().zip(colors) {
        let late_ms = area.late_ms.clamp(-center_ms, center_ms);
        let early_ms = area.early_ms.clamp(-center_ms, center_ms);
        push_timing_judge_band_rect(
            &mut items,
            rect,
            center_ms,
            late_ms,
            inner_late_ms,
            color,
            frame_alpha,
            blend,
        );
        push_timing_judge_band_rect(
            &mut items,
            rect,
            center_ms,
            inner_early_ms,
            early_ms,
            color,
            frame_alpha,
            blend,
        );
        inner_late_ms = inner_late_ms.min(late_ms);
        inner_early_ms = inner_early_ms.max(early_ms);
    }
    items
}

pub(super) fn push_timing_judge_band_rect(
    items: &mut Vec<SkinRenderItem>,
    rect: Rect,
    center_ms: f32,
    start_ms: f32,
    end_ms: f32,
    color: Color,
    frame_alpha: f32,
    blend: BlendMode,
) {
    if end_ms <= start_ms {
        return;
    }
    let x1 = rect.x + ((start_ms + center_ms) / (center_ms * 2.0)) * rect.width;
    let x2 = rect.x + ((end_ms + center_ms) / (center_ms * 2.0)) * rect.width;
    items.push(SkinRenderItem::Rect {
        rect: Rect { x: x1, y: rect.y, width: (x2 - x1).max(0.0), height: rect.height },
        color: color.with_alpha(color.a * frame_alpha * 0.25),
        blend,
    });
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TimingJudgeArea {
    pub(super) late_ms: f32,
    pub(super) early_ms: f32,
}

pub(super) fn beatoraja_timing_judge_areas(state: &SkinDrawState) -> [TimingJudgeArea; 5] {
    let base = bmz_gameplay::judge::window::beatoraja_note_judge_window_for_keymode(state.key_mode);
    let percent = beatoraja_judge_rank_percent_for_mode(state.key_mode, state.judge_rank);
    let window = bmz_gameplay::judge::window::beatoraja_judge_window_for_rank_and_keymode(
        base,
        percent,
        state.key_mode,
    );
    timing_judge_areas_from_window(window)
}

pub(super) fn timing_judge_areas(state: &SkinDrawState) -> [TimingJudgeArea; 5] {
    use bmz_gameplay::judge::window::{
        judge_rank_to_percent_optional_for_rule_mode, judge_window_for_rule_mode_and_keymode,
        judge_windows_for_keymode_and_rule_mode,
    };
    use bmz_gameplay::rule::RuleMode;

    if let Some(window) = state.note_judge_window {
        return timing_judge_areas_from_window(window);
    }
    // Placeholder/debug scenes have no session. Preserve legacy beatoraja rank
    // interpretation, but do not display beatoraja bands for known LR2/DX rules.
    let rule_mode = match state.rule_mode_index {
        1 => RuleMode::Lr2Oraja,
        2 => RuleMode::Dx,
        _ => return beatoraja_timing_judge_areas(state),
    };
    let base = judge_windows_for_keymode_and_rule_mode(state.key_mode, rule_mode).note;
    let percent = judge_rank_to_percent_optional_for_rule_mode(state.judge_rank, rule_mode);
    timing_judge_areas_from_window(judge_window_for_rule_mode_and_keymode(
        base,
        percent,
        rule_mode,
        state.key_mode,
    ))
}

pub(super) fn timing_judge_areas_from_window(
    window: bmz_gameplay::judge::model::JudgeWindow,
) -> [TimingJudgeArea; 5] {
    [
        symmetric_timing_judge_area(window.pgreat_us),
        symmetric_timing_judge_area(window.great_us),
        symmetric_timing_judge_area(window.good_us),
        TimingJudgeArea {
            late_ms: -window.bad_fast_us as f32 / 1_000.0,
            early_ms: window.bad_slow_us as f32 / 1_000.0,
        },
        TimingJudgeArea {
            late_ms: -window.empty_poor_fast_us as f32 / 1_000.0,
            early_ms: window.empty_poor_slow_us as f32 / 1_000.0,
        },
    ]
}

pub(super) fn symmetric_timing_judge_area(us: i64) -> TimingJudgeArea {
    let ms = us as f32 / 1_000.0;
    TimingJudgeArea { late_ms: -ms, early_ms: ms }
}

pub(super) fn beatoraja_judge_rank_percent_for_mode(
    key_mode: KeyMode,
    judge_rank: Option<i32>,
) -> i32 {
    let Some(rank) = judge_rank else {
        return 100;
    };
    if rank >= 10 {
        return rank;
    }
    let table =
        if key_mode == KeyMode::K9 { [33, 50, 70, 100, 133] } else { [25, 50, 75, 100, 125] };
    table.get(rank as usize).copied().unwrap_or(table[2])
}

pub(super) fn timing_distribution_x(rect: Rect, center: usize, value_ms: f32) -> f32 {
    let span = (center.max(1) * 2) as f32;
    rect.x + ((center as f32 + value_ms) / span).clamp(0.0, 1.0) * rect.width
}
