pub(in crate::app) fn select_click_event_arg(
    click_type: i32,
    button: MouseButton,
    rect: Rect,
    x: f32,
    y: f32,
) -> Option<i32> {
    let button_arg = match button {
        MouseButton::Left => 1,
        MouseButton::Right => -1,
        MouseButton::Middle => 1,
        _ => return None,
    };
    match click_type {
        0 => Some(button_arg),
        1 => Some(-button_arg),
        2 => Some(if x >= rect.x + rect.width * 0.5 { 1 } else { -1 }),
        3 => Some(if y <= rect.y + rect.height * 0.5 { 1 } else { -1 }),
        _ => None,
    }
}

pub(in crate::app) fn chart_snapshot_metadata_for_chart(
    select_items: &[SelectItem],
    chart_id: i64,
    fallback: impl FnOnce(i64) -> Option<ChartListItem>,
) -> Option<(ChartListItem, Option<u32>)> {
    select_items
        .iter()
        .find_map(|item| match item {
            SelectItem::Chart(row) => row.chart.as_ref().and_then(|chart| {
                (chart.chart_id == chart_id)
                    .then(|| (chart.clone(), row.best_score.as_ref().map(|best| best.ex_score)))
            }),
            _ => None,
        })
        .or_else(|| fallback(chart_id).map(|chart| (chart, None)))
}

pub(in crate::app) fn apply_chart_metadata_to_snapshot(
    snapshot: &mut RenderSnapshot,
    chart: &ChartListItem,
    total_notes: u32,
    best_ex_score: Option<u32>,
) {
    snapshot.title.clone_from(&chart.title);
    snapshot.subtitle.clone_from(&chart.subtitle);
    snapshot.artist.clone_from(&chart.artist);
    snapshot.subartist.clone_from(&chart.subartist);
    snapshot.genre.clone_from(&chart.genre);
    snapshot.difficulty_name.clone_from(&chart.difficulty_name);
    snapshot.play_level.clone_from(&chart.play_level);
    snapshot.judge_rank = chart.judge_rank;
    snapshot.total_notes = total_notes;
    snapshot.duration = TimeUs(chart.length_ms.saturating_mul(1_000));
    snapshot.min_bpm = chart.min_bpm as f32;
    snapshot.max_bpm = chart.max_bpm as f32;
    snapshot.now_bpm = chart.initial_bpm as f32;
    snapshot.has_long_notes = Some(chart.has_long_notes);
    // PACEMAKER の MyBest 表示。projected (ghost 進行値) は進捗 0 なので 0。
    snapshot.best_ex_score = best_ex_score;
    snapshot.projected_best_ex_score = best_ex_score.map(|_| 0);
}

pub(in crate::app) fn course_titles_from_entries<'a>(
    entries: impl IntoIterator<Item = (&'a str, bool)>,
) -> [String; 10] {
    let mut titles: [String; 10] = Default::default();
    for (index, (title, resolved)) in entries.into_iter().take(10).enumerate() {
        titles[index] = if resolved {
            title.to_string()
        } else {
            format!("(no song) {}", if title.is_empty() { "----" } else { title })
        };
    }
    titles
}

pub(in crate::app) fn course_constraint_flags(
    constraints: &bmz_core::course::CourseConstraints,
) -> bmz_render::scene::CourseConstraintFlags {
    use bmz_core::course::{
        CourseClassConstraint, CourseGaugeConstraint, CourseJudgeConstraint, CourseLnConstraint,
        CourseSpeedConstraint,
    };

    bmz_render::scene::CourseConstraintFlags {
        class: constraints.class == CourseClassConstraint::Grade,
        mirror: constraints.class == CourseClassConstraint::GradeMirrorAllowed,
        random: constraints.class == CourseClassConstraint::GradeRandomAllowed,
        no_speed: constraints.speed == CourseSpeedConstraint::NoSpeed,
        no_good: constraints.judge == CourseJudgeConstraint::NoGood,
        no_great: constraints.judge == CourseJudgeConstraint::NoGreat,
        gauge_lr2: constraints.gauge == CourseGaugeConstraint::Lr2,
        gauge_5k: constraints.gauge == CourseGaugeConstraint::Keys5,
        gauge_7k: constraints.gauge == CourseGaugeConstraint::Keys7,
        gauge_9k: constraints.gauge == CourseGaugeConstraint::Keys9,
        gauge_24k: constraints.gauge == CourseGaugeConstraint::Keys24,
        ln: constraints.ln == CourseLnConstraint::Ln,
        cn: constraints.ln == CourseLnConstraint::Cn,
        hcn: constraints.ln == CourseLnConstraint::Hcn,
    }
}

pub(in crate::app) fn active_course_speed_locked(course: Option<&ActiveCourseSession>) -> bool {
    course.is_some_and(|course| {
        course.definition.constraints.speed == bmz_core::course::CourseSpeedConstraint::NoSpeed
    })
}

pub(in crate::app) fn moved_select_index(
    current_index: usize,
    row_count: usize,
    select_move: SelectMove,
) -> usize {
    if row_count == 0 {
        return 0;
    }

    match select_move {
        SelectMove::Previous => (current_index + row_count - 1) % row_count,
        SelectMove::Next => (current_index + 1) % row_count,
        SelectMove::PagePrevious => (current_index + row_count - (7 % row_count)) % row_count,
        SelectMove::PageNext => (current_index + 7) % row_count,
        SelectMove::First => 0,
        SelectMove::Last => row_count - 1,
    }
}

pub(in crate::app) fn select_move_scroll_rows(select_move: SelectMove) -> i32 {
    match select_move {
        SelectMove::Previous => -1,
        SelectMove::Next => 1,
        SelectMove::PagePrevious => -7,
        SelectMove::PageNext => 7,
        SelectMove::First | SelectMove::Last => 0,
    }
}
use super::*;
