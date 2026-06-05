use bmz_chart::model::LongNoteMode;
use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use crate::snapshot::{
    DisplayJudgeCounts, DisplayJudgement, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot,
    VisibleBarLine, VisibleLongNote, VisibleNote,
};

pub fn sample_select_scene() -> AppSceneSnapshot {
    let rows = (0..7)
        .map(|index| SelectRowSnapshot {
            index,
            title: format!("Sample BMS {}", index + 1),
            artist: "bmz".to_string(),
            difficulty_name: "NORMAL".to_string(),
            play_level: (index + 1).to_string(),
            table_level: String::new(),
            total_notes: 1200 + index * 10,
            initial_bpm: 128.0,
            min_bpm: 128.0,
            max_bpm: 160.0,
            length_ms: 90_000 + i64::from(index) * 1_000,
            clear_type: if index == 0 { "Normal".to_string() } else { String::new() },
            ex_score: (index == 0).then_some(1888),
            max_combo: (index == 0).then_some(777),
            gauge_value: (index == 0).then_some(80.0),
            replay_slots: [index == 0, false, false, false],
            is_folder: false,
            kind: Default::default(),
            ..SelectRowSnapshot::default()
        })
        .collect();

    AppSceneSnapshot::Select(SelectSnapshot {
        time: TimeUs(12_345_000),
        selection_time: TimeUs(345_000),
        option_panel_time: TimeUs(0),
        option_panel: 0,
        chart_count: 7,
        selected_index: 0,
        selected_chart_id: Some(1),
        selected_title: "Sample BMS".to_string(),
        rows,
        arrange: "NORMAL".to_string(),
        target: "NONE".to_string(),
        gauge: "NORMAL".to_string(),
        gauge_auto_shift: "OFF".to_string(),
        assist: "NORMAL".to_string(),
        select_mode: "ALL".to_string(),
        select_sort: "TITLE".to_string(),
        select_ln_mode: "LN".to_string(),
        bga: "ON".to_string(),
        master_volume: 1.0,
        key_volume: 1.0,
        bgm_volume: 1.0,
        current_folder: String::new(),
        key_hint: "UP DOWN  RIGHT/Z/X/C/V:ENTER  LEFT/S:BACK".to_string(),
        option_hint:
            "F1 SELECT  F2 RELOAD  F3 RESULT  F4 PLAY   Q+Z:ARRANGE  Q+X:GAUGE  Q+C:ASSIST"
                .to_string(),
        exit_hold_progress: 0.0,
        overlay: OverlaySnapshot::default(),
        stage_background: false,
        banner_image: false,
        in_settings: false,
        settings_editing: false,
        search_word: String::new(),
        search_word_alpha: 1.0,
        mouse_position: None,
    })
}

pub fn sample_play_scene() -> AppSceneSnapshot {
    let mut snapshot = RenderSnapshot {
        time: TimeUs(12_345_000),
        duration: TimeUs(90_000_000),
        title: "BMZ Sample Playable".to_string(),
        artist: "bmz".to_string(),
        genre: "BMS".to_string(),
        difficulty_name: "NORMAL".to_string(),
        play_level: "7".to_string(),
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
            processed_judge: None,
        });
    }
    snapshot.visible_notes[Lane::Key2.index()].push(VisibleNote {
        lane: Lane::Key2,
        time: TimeUs(13_200_000),
        y: 0.86,
        processed_judge: None,
    });
    snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(12_000_000), y: 0.25 });
    snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(13_000_000), y: 0.78 });
    // ホールド中のロングノート（Key4）と上空に伸びるロングノート（Key6）
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key4,
        mode: LongNoteMode::Ln,
        head_y: 0.0,
        tail_y: 0.45,
    });
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key6,
        mode: LongNoteMode::Hcn,
        head_y: 0.3,
        tail_y: 0.82,
    });
    snapshot.recent_judgements.push(DisplayJudgement {
        lane: Lane::Key3,
        judge: bmz_core::judge::Judge::PGreat,
        side: bmz_core::judge::TimingSide::Fast,
        text: "PGREAT FAST".to_string(),
        combo: 1234,
        delta_us: -12_000,
        time: TimeUs(12_300_000),
        is_miss: false,
    });

    AppSceneSnapshot::Play(snapshot)
}

pub fn sample_result_scene() -> AppSceneSnapshot {
    AppSceneSnapshot::Result(ResultSnapshot {
        clear_type: bmz_core::clear::ClearType::Normal,
        arrange: "NORMAL".to_string(),
        ex_score: 1888,
        ex_score_rate: 0.944,
        max_combo: 777,
        gauge_value: 84.0,
        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
        total_notes: 1000,
        judge_counts: DisplayJudgeCounts {
            pgreat: 777,
            great: 334,
            good: 22,
            bad: 3,
            poor: 5,
            empty_poor: 9,
        },
        fast_slow_counts: FastSlowJudgeCounts {
            fast_pgreat: 350,
            slow_pgreat: 427,
            fast_great: 180,
            slow_great: 154,
            fast_good: 12,
            slow_good: 10,
            fast_bad: 2,
            slow_bad: 1,
            fast_poor: 3,
            slow_poor: 2,
            fast_empty_poor: 5,
            slow_empty_poor: 4,
        },
        score_history_id: 42,
        replay_saved: true,
        replay_slots: [true, false, false, false],
        saved_replay_slots: [true, false, false, false],
        best_ex_score: Some(1700),
        best_clear_type: Some(bmz_core::clear::ClearType::Hard),
        target_ex_score: Some(1900),
        best_max_combo: Some(820),
        target_max_combo: Some(1000),
        best_bp: Some(15),
        target_bp: Some(0),
        previous_best_ex_score: Some(1600),
        previous_best_max_combo: Some(800),
        previous_best_bp: Some(20),
        target_clear_type: Some(bmz_core::clear::ClearType::FullCombo),
        elapsed_time: TimeUs(0),
        fadeout_elapsed: None,
        title: "BMZ Sample Playable".to_string(),
        subtitle: String::new(),
        artist: "bmz".to_string(),
        subartist: String::new(),
        genre: "BMS".to_string(),
        difficulty_name: "NORMAL".to_string(),
        play_level: "7".to_string(),
        graph: crate::snapshot::ResultGraphSnapshot::default(),
        overlay: OverlaySnapshot::default(),
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
