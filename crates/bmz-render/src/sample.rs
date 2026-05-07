use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use crate::snapshot::{DisplayJudgement, RenderSnapshot, VisibleBarLine, VisibleNote};

pub fn sample_select_scene() -> AppSceneSnapshot {
    let rows = (0..7)
        .map(|index| SelectRowSnapshot {
            index,
            title: format!("Sample BMS {}", index + 1),
            artist: "bmz".to_string(),
            play_level: (index + 1).to_string(),
            clear_type: if index == 0 { "Normal".to_string() } else { String::new() },
            ex_score: (index == 0).then_some(1888),
        })
        .collect();

    AppSceneSnapshot::Select(SelectSnapshot {
        chart_count: 7,
        selected_index: 0,
        selected_chart_id: Some(1),
        selected_title: "Sample BMS".to_string(),
        rows,
    })
}

pub fn sample_play_scene() -> AppSceneSnapshot {
    let mut snapshot =
        RenderSnapshot { time: TimeUs(12_345_000), combo: 1234, gauge: 82.0, ..Default::default() };

    for (index, lane) in Lane::ALL.into_iter().enumerate() {
        snapshot.visible_notes[lane.index()].push(VisibleNote {
            lane,
            time: TimeUs(12_500_000 + index as i64 * 80_000),
            y: 0.18 + index as f32 * 0.08,
        });
    }
    snapshot.visible_notes[Lane::Key2.index()].push(VisibleNote {
        lane: Lane::Key2,
        time: TimeUs(13_200_000),
        y: 0.86,
    });
    snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(12_000_000), y: 0.25 });
    snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(13_000_000), y: 0.78 });
    snapshot.recent_judgements.push(DisplayJudgement {
        text: "PGREAT FAST".to_string(),
        delta_us: -12_000,
        time: TimeUs(12_300_000),
    });

    AppSceneSnapshot::Play(snapshot)
}

pub fn sample_result_scene() -> AppSceneSnapshot {
    AppSceneSnapshot::Result(ResultSnapshot {
        clear_type: bmz_core::clear::ClearType::Normal,
        ex_score: 1888,
        ex_score_rate: 0.944,
        max_combo: 777,
        gauge_value: 84.0,
        total_notes: 1000,
    })
}

#[cfg(test)]
mod tests {
    use crate::plan::DrawPlan;

    use super::*;

    #[test]
    fn sample_play_scene_builds_non_empty_draw_plan() {
        let plan = DrawPlan::from_scene(&sample_play_scene());

        assert!(plan.commands.len() > 20);
    }

    #[test]
    fn sample_scenes_cover_all_scene_kinds() {
        assert!(matches!(sample_select_scene(), AppSceneSnapshot::Select(_)));
        assert!(matches!(sample_play_scene(), AppSceneSnapshot::Play(_)));
        assert!(matches!(sample_result_scene(), AppSceneSnapshot::Result(_)));
    }
}
