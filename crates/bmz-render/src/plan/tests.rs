use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::TimeUs;

use crate::skin::{SkinDocument, SkinDocumentTexture, SkinImageSize, SkinTextureId};
use crate::snapshot::{
    DisplayInput, DisplayJudgeCounts, DisplayJudgement, LongBodyState, NoteVisualKind,
    RenderSnapshot, VisibleBarLine, VisibleLongNote, VisibleMine, VisibleNote,
};

use super::*;

fn select_rows(count: u32) -> Vec<crate::scene::SelectRowSnapshot> {
    (0..count)
        .map(|index| crate::scene::SelectRowSnapshot {
            index,
            title: format!("Title {index}"),
            artist: format!("Artist {index}"),
            difficulty_name: "NORMAL".to_string(),
            play_level: index.to_string(),
            table_level: String::new(),
            total_notes: 1000 + index,
            initial_bpm: 128.0,
            min_bpm: 128.0,
            max_bpm: 128.0,
            length_ms: 90_000,
            clear_type: if index == 0 { "Normal".to_string() } else { String::new() },
            ex_score: (index == 0).then_some(1234),
            max_combo: (index == 0).then_some(777),
            gauge_value: (index == 0).then_some(80.0),
            replay_slots: [false; 4],
            is_folder: false,
            kind: Default::default(),
            ..Default::default()
        })
        .collect()
}

fn history_label(text: &str) -> String {
    judgement_history_label(&DisplayJudgement {
        lane: Lane::Key1,
        judge: Judge::PGreat,
        side: Some(TimingSide::Fast),
        text: text.to_string(),
        combo: 0,
        delta_us: 0,
        time: TimeUs(0),
        is_miss: false,
        timing_ms_suppressed: false,
    })
}

fn approx_eq(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 0.0001
}

fn draw_command_has_rect_color(command: &DrawCommand, predicate: impl Fn(&Color) -> bool) -> bool {
    match command {
        DrawCommand::Rect { color, .. } => predicate(color),
        DrawCommand::RectBatch { rects, .. } => rects.iter().any(|rect| predicate(&rect.color)),
        _ => false,
    }
}

fn draw_command_has_rect(command: &DrawCommand, predicate: impl Fn(&Rect, &Color) -> bool) -> bool {
    match command {
        DrawCommand::Rect { rect, color } => predicate(rect, color),
        DrawCommand::RectBatch { rects, .. } => {
            rects.iter().any(|command| predicate(&command.rect, &command.color))
        }
        _ => false,
    }
}

#[path = "tests/bpm_change.rs"]
mod bpm_change;
#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
#[path = "tests/cases_03.rs"]
mod cases_03;
#[path = "tests/cases_04.rs"]
mod cases_04;
#[path = "tests/cases_05.rs"]
mod cases_05;
