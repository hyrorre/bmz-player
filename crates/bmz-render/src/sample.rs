use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use crate::snapshot::{
    DisplayJudgeCounts, DisplayJudgement, RenderSnapshot, VisibleBarLine, VisibleLongNote,
    VisibleNote,
};

pub fn sample_select_scene() -> AppSceneSnapshot {
    let rows = (0..7)
        .map(|index| SelectRowSnapshot {
            index,
            title: format!("Sample BMS {}", index + 1),
            artist: "bmz".to_string(),
            play_level: (index + 1).to_string(),
            table_level: String::new(),
            clear_type: if index == 0 { "Normal".to_string() } else { String::new() },
            ex_score: (index == 0).then_some(1888),
            is_folder: false,
        })
        .collect();

    AppSceneSnapshot::Select(SelectSnapshot {
        chart_count: 7,
        selected_index: 0,
        selected_chart_id: Some(1),
        selected_title: "Sample BMS".to_string(),
        rows,
        arrange: "NORMAL".to_string(),
        gauge: "NORMAL".to_string(),
        assist: "NORMAL".to_string(),
        current_folder: String::new(),
        key_hint: "UP DOWN  RIGHT/Z/X/C/V:ENTER  LEFT/S:BACK".to_string(),
        option_hint:
            "F1 SELECT  F2 RELOAD  F3 RESULT  F4 PLAY   Q+Z:ARRANGE  Q+X:GAUGE  Q+C:ASSIST"
                .to_string(),
    })
}

pub fn sample_play_scene() -> AppSceneSnapshot {
    let mut snapshot = RenderSnapshot {
        time: TimeUs(12_345_000),
        duration: TimeUs(90_000_000),
        title: "BMZ Sample Playable".to_string(),
        artist: "bmz".to_string(),
        genre: "BMS".to_string(),
        combo: 1234,
        gauge: 82.0,
        hispeed: 2.0,
        ..Default::default()
    };

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
    // ホールド中のロングノート（Key4）と上空に伸びるロングノート（Key6）
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key4,
        head_y: 0.0,
        tail_y: 0.45,
    });
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key6,
        head_y: 0.3,
        tail_y: 0.82,
    });
    snapshot.recent_judgements.push(DisplayJudgement {
        lane: Lane::Key3,
        text: "PGREAT FAST".to_string(),
        delta_us: -12_000,
        time: TimeUs(12_300_000),
        is_miss: false,
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
        judge_counts: DisplayJudgeCounts {
            pgreat: 777,
            great: 334,
            good: 22,
            bad: 3,
            poor: 5,
            empty_poor: 9,
        },
        score_history_id: 42,
        replay_saved: true,
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
