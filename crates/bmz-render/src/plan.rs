use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, SelectRowSnapshot};
use crate::snapshot::RenderSnapshot;
use crate::text::{BitmapTextStyle, TextRenderer};

#[derive(Debug, Clone, PartialEq)]
pub struct DrawPlan {
    pub clear: Color,
    pub commands: Vec<DrawCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect { rect: Rect, color: Color },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl DrawPlan {
    pub fn from_scene(scene: &AppSceneSnapshot) -> Self {
        match scene {
            AppSceneSnapshot::Select(snapshot) => {
                plan_select(snapshot.chart_count, snapshot.selected_index, &snapshot.rows)
            }
            AppSceneSnapshot::Play(snapshot) => plan_play(snapshot),
            AppSceneSnapshot::Result(snapshot) => plan_result(
                snapshot.clear_type.as_str(),
                snapshot.ex_score,
                snapshot.ex_score_rate,
                snapshot.max_combo,
                snapshot.gauge_value,
            ),
        }
    }
}

impl Color {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color { r: self.r as f64, g: self.g as f64, b: self.b as f64, a: self.a as f64 }
    }
}

fn plan_select(chart_count: u32, selected_index: u32, rows: &[SelectRowSnapshot]) -> DrawPlan {
    let mut commands = Vec::new();
    let text = TextRenderer;
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.06, y: 0.08, width: 0.88, height: 0.08 },
        color: Color::rgb(0.08, 0.11, 0.13),
    });
    text.push_text(
        &mut commands,
        "SELECT",
        BitmapTextStyle { x: 0.08, y: 0.105, cell: 0.009, color: Color::rgb(0.82, 0.9, 0.95) },
    );
    text.push_text(
        &mut commands,
        &format!("CHARTS {}", chart_count),
        BitmapTextStyle { x: 0.78, y: 0.112, cell: 0.005, color: Color::rgb(0.62, 0.78, 0.84) },
    );
    let visible_rows = rows.len().max(1).min(7);
    for row in 0..visible_rows {
        let snapshot_row = rows.get(row);
        let selected = snapshot_row.map(|row| row.index == selected_index).unwrap_or(row == 0);
        let row_y = 0.2 + row as f32 * 0.09;
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.08, y: row_y, width: 0.68, height: 0.065 },
            color: if selected {
                Color::rgb(0.22, 0.28, 0.31)
            } else {
                Color::rgb(0.075, 0.09, 0.1)
            },
        });
        push_select_title_text(&text, &mut commands, snapshot_row, row_y, selected);
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.78, y: row_y, width: 0.14, height: 0.065 },
            color: if selected {
                Color::rgb(0.16, 0.21, 0.23)
            } else {
                Color::rgb(0.055, 0.065, 0.072)
            },
        });
        push_select_score_text(&text, &mut commands, snapshot_row, row_y, selected);
    }
    text.push_text(
        &mut commands,
        "UP DOWN PAGE HOME END  ENTER START",
        BitmapTextStyle { x: 0.08, y: 0.86, cell: 0.006, color: Color::rgb(0.88, 0.9, 0.86) },
    );
    text.push_text(
        &mut commands,
        "F1 SELECT  F2 PLAY  F3 RESULT",
        BitmapTextStyle { x: 0.08, y: 0.895, cell: 0.005, color: Color::rgb(0.58, 0.67, 0.7) },
    );

    DrawPlan { clear: Color::rgb(0.02, 0.025, 0.03), commands }
}

fn push_select_title_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    row: Option<&SelectRowSnapshot>,
    row_y: f32,
    selected: bool,
) {
    let title = display_title(row.map(|row| row.title.as_str()).unwrap_or_default());
    text.push_text(
        commands,
        &title,
        BitmapTextStyle {
            x: 0.1,
            y: row_y + if selected { 0.016 } else { 0.022 },
            cell: if selected { 0.006 } else { 0.005 },
            color: if selected {
                Color::rgb(0.9, 0.96, 0.98)
            } else {
                Color::rgb(0.58, 0.66, 0.68)
            },
        },
    );

    let Some(row) = row else {
        return;
    };
    if selected && !row.artist.is_empty() {
        text.push_text(
            commands,
            &display_label(&row.artist, 30),
            BitmapTextStyle {
                x: 0.1,
                y: row_y + 0.046,
                cell: 0.0035,
                color: Color::rgb(0.58, 0.71, 0.73),
            },
        );
    }
}

fn push_select_score_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    row: Option<&SelectRowSnapshot>,
    row_y: f32,
    selected: bool,
) {
    let status = row_status_label(row);
    text.push_text(
        commands,
        &status,
        BitmapTextStyle {
            x: 0.805,
            y: row_y + if selected { 0.016 } else { 0.018 },
            cell: if selected { 0.0055 } else { 0.0045 },
            color: if selected {
                Color::rgb(0.74, 0.88, 0.9)
            } else {
                Color::rgb(0.38, 0.46, 0.48)
            },
        },
    );

    let Some(row) = row else {
        return;
    };
    if let Some(ex_score) = row.ex_score {
        text.push_text(
            commands,
            &format!("EX {}", ex_score),
            BitmapTextStyle {
                x: 0.805,
                y: row_y + 0.043,
                cell: if selected { 0.004 } else { 0.0035 },
                color: if selected {
                    Color::rgb(0.86, 0.9, 0.82)
                } else {
                    Color::rgb(0.35, 0.42, 0.38)
                },
            },
        );
    }
}

fn row_status_label(row: Option<&SelectRowSnapshot>) -> String {
    let Some(row) = row else {
        return "EMPTY".to_string();
    };
    let clear_type = clear_type_label(&row.clear_type);
    if !clear_type.is_empty() {
        clear_type.to_string()
    } else if !row.play_level.is_empty() {
        format!("LV {}", display_label(&row.play_level, 4))
    } else {
        "READY".to_string()
    }
}

fn clear_type_label(clear_type: &str) -> &'static str {
    match clear_type {
        "Failed" => "FAILED",
        "AssistEasy" => "AEASY",
        "LightAssistEasy" => "LAEASY",
        "Easy" => "EASY",
        "Normal" => "NORMAL",
        "Hard" => "HARD",
        "ExHard" => "EXHARD",
        "FullCombo" => "FC",
        "Perfect" => "PERFECT",
        "Max" => "MAX",
        _ => "",
    }
}

fn plan_play(snapshot: &RenderSnapshot) -> DrawPlan {
    let mut commands = Vec::new();
    let text = TextRenderer;
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
    commands.push(DrawCommand::Rect { rect: board, color: Color::rgb(0.025, 0.025, 0.028) });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: board.x - 0.006, y: board.y, width: 0.006, height: board.height },
        color: Color::rgb(0.18, 0.2, 0.21),
    });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: board.x + board.width, y: board.y, width: 0.006, height: board.height },
        color: Color::rgb(0.18, 0.2, 0.21),
    });

    let lane_width = board.width / LANE_COUNT as f32;
    for lane in Lane::ALL {
        let lane_index = lane.index();
        let x = board.x + lane_index as f32 * lane_width;
        let color = if lane_index % 2 == 0 {
            Color::rgb(0.07, 0.075, 0.08)
        } else {
            Color::rgb(0.045, 0.05, 0.055)
        };
        commands.push(DrawCommand::Rect {
            rect: Rect { x, y: board.y, width: lane_width, height: board.height },
            color,
        });
        if let Some(color) = lane_flash_color(snapshot, lane) {
            commands.push(DrawCommand::Rect {
                rect: Rect {
                    x: x + lane_width * 0.04,
                    y: board.y + board.height * 0.76,
                    width: lane_width * 0.92,
                    height: board.height * 0.18,
                },
                color,
            });
        }

        for note in &snapshot.visible_notes[lane_index] {
            let y = board.y + (1.0 - note.y.clamp(0.0, 1.0)) * board.height;
            commands.push(DrawCommand::Rect {
                rect: Rect { x: x + lane_width * 0.08, y, width: lane_width * 0.84, height: 0.018 },
                color: note_color(lane),
            });
        }
    }

    push_receptors(&mut commands, board, lane_width);
    for bar in &snapshot.bar_lines {
        let y = board.y + (1.0 - bar.y.clamp(0.0, 1.0)) * board.height;
        commands.push(DrawCommand::Rect {
            rect: Rect { x: board.x, y, width: board.width, height: 0.004 },
            color: Color::rgb(0.45, 0.48, 0.5),
        });
    }
    push_judge_line(&mut commands, board);
    push_gauge(&mut commands, snapshot.gauge);
    push_combo_panel(&mut commands, snapshot.combo);
    push_play_text(&text, &mut commands, snapshot);
    push_lane_text(&text, &mut commands, board, lane_width);

    DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands }
}

fn plan_result(
    clear_type: &str,
    ex_score: u32,
    ex_score_rate: f32,
    max_combo: u32,
    gauge_value: f32,
) -> DrawPlan {
    let mut commands = Vec::new();
    let text = TextRenderer;
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.1, y: 0.16, width: 0.8, height: 0.18 },
        color: Color::rgb(0.16, 0.13, 0.11),
    });
    text.push_text(
        &mut commands,
        "RESULT",
        BitmapTextStyle { x: 0.14, y: 0.205, cell: 0.014, color: Color::rgb(0.95, 0.9, 0.8) },
    );
    text.push_text(
        &mut commands,
        &display_label(clear_type, 18),
        BitmapTextStyle { x: 0.55, y: 0.22, cell: 0.008, color: Color::rgb(0.84, 0.93, 0.9) },
    );
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.14, y: 0.42, width: 0.72, height: 0.045 },
        color: Color::rgb(0.065, 0.06, 0.058),
    });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.14, y: 0.42, width: 0.72 * ex_score_rate.clamp(0.0, 1.0), height: 0.045 },
        color: Color::rgb(0.55, 0.78, 0.86),
    });
    for index in 0..4 {
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.14 + index as f32 * 0.18, y: 0.55, width: 0.14, height: 0.1 },
            color: Color::rgb(0.09, 0.08, 0.075),
        });
    }
    text.push_text(
        &mut commands,
        &format!("EX {}", ex_score),
        BitmapTextStyle { x: 0.16, y: 0.565, cell: 0.008, color: Color::rgb(0.86, 0.9, 0.92) },
    );
    text.push_text(
        &mut commands,
        &format!("MAX {}", max_combo),
        BitmapTextStyle { x: 0.34, y: 0.565, cell: 0.008, color: Color::rgb(0.86, 0.9, 0.92) },
    );
    text.push_text(
        &mut commands,
        &format!("GAUGE {}", gauge_value.round() as u32),
        BitmapTextStyle { x: 0.52, y: 0.565, cell: 0.008, color: Color::rgb(0.86, 0.9, 0.92) },
    );
    text.push_text(
        &mut commands,
        "ENTER OR ESC",
        BitmapTextStyle { x: 0.14, y: 0.76, cell: 0.006, color: Color::rgb(0.74, 0.78, 0.8) },
    );

    DrawPlan { clear: Color::rgb(0.025, 0.02, 0.018), commands }
}

fn push_judge_line(commands: &mut Vec<DrawCommand>, board: Rect) {
    let line_y = board.y + board.height * 0.86;
    commands.push(DrawCommand::Rect {
        rect: Rect { x: board.x, y: line_y, width: board.width, height: 0.006 },
        color: Color::rgb(0.96, 0.92, 0.54),
    });
}

fn push_receptors(commands: &mut Vec<DrawCommand>, board: Rect, lane_width: f32) {
    let receptor_y = board.y + board.height * 0.825;
    for lane in Lane::ALL {
        let lane_index = lane.index();
        let x = board.x + lane_index as f32 * lane_width;
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: x + lane_width * 0.1,
                y: receptor_y,
                width: lane_width * 0.8,
                height: 0.026,
            },
            color: receptor_color(lane),
        });
    }
}

fn push_gauge(commands: &mut Vec<DrawCommand>, gauge: f32) {
    let frame = Rect { x: 0.84, y: 0.08, width: 0.035, height: 0.82 };
    let fill = gauge.clamp(0.0, 100.0) / 100.0;
    commands.push(DrawCommand::Rect { rect: frame, color: Color::rgb(0.06, 0.065, 0.07) });
    commands.push(DrawCommand::Rect {
        rect: Rect {
            x: frame.x + 0.006,
            y: frame.y + frame.height * (1.0 - fill),
            width: frame.width - 0.012,
            height: frame.height * fill,
        },
        color: gauge_color(gauge),
    });
}

fn push_combo_panel(commands: &mut Vec<DrawCommand>, combo: u32) {
    let width = if combo >= 1000 { 0.2 } else { 0.15 };
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.425 - width / 2.0, y: 0.16, width, height: 0.07 },
        color: if combo > 0 { Color::rgb(0.14, 0.18, 0.2) } else { Color::rgb(0.055, 0.06, 0.065) },
    });
}

fn push_play_text(text: &TextRenderer, commands: &mut Vec<DrawCommand>, snapshot: &RenderSnapshot) {
    push_play_status_text(text, commands, snapshot);
    if snapshot.combo > 0 {
        text.push_text(
            commands,
            &snapshot.combo.to_string(),
            BitmapTextStyle { x: 0.38, y: 0.18, cell: 0.01, color: Color::rgb(0.94, 0.98, 1.0) },
        );
    }
    text.push_text(
        commands,
        &format!("G{}", snapshot.gauge.round() as u32),
        BitmapTextStyle { x: 0.885, y: 0.08, cell: 0.007, color: Color::rgb(0.8, 0.92, 0.86) },
    );
    if let Some(judgement) = snapshot.recent_judgements.last() {
        text.push_text(
            commands,
            &judgement.text,
            BitmapTextStyle { x: 0.38, y: 0.245, cell: 0.006, color: Color::rgb(0.96, 0.92, 0.54) },
        );
        text.push_text(
            commands,
            &format_delta_ms(judgement.delta_us),
            BitmapTextStyle {
                x: 0.405,
                y: 0.282,
                cell: 0.0045,
                color: Color::rgb(0.72, 0.82, 0.86),
            },
        );
    }
}

fn push_lane_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lane_width: f32,
) {
    for lane in Lane::ALL {
        let lane_index = lane.index();
        let center_x = board.x + lane_index as f32 * lane_width + lane_width / 2.0;
        let label = lane_label(lane);
        text.push_text(
            commands,
            label,
            BitmapTextStyle {
                x: center_x - label_width(label, 0.0035) / 2.0,
                y: board.y + 0.018,
                cell: 0.0035,
                color: Color::rgb(0.45, 0.55, 0.58),
            },
        );
        let key = lane_key_label(lane);
        text.push_text(
            commands,
            key,
            BitmapTextStyle {
                x: center_x - label_width(key, 0.004) / 2.0,
                y: board.y + board.height * 0.9,
                cell: 0.004,
                color: Color::rgb(0.78, 0.86, 0.84),
            },
        );
    }
}

fn push_play_status_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.05, y: 0.08, width: 0.11, height: 0.24 },
        color: Color::rgb(0.035, 0.04, 0.044),
    });
    text.push_text(
        commands,
        &format!("EX {}", snapshot.ex_score),
        BitmapTextStyle { x: 0.065, y: 0.105, cell: 0.0055, color: Color::rgb(0.82, 0.9, 0.92) },
    );
    text.push_text(
        commands,
        &format!("MAX {}", snapshot.max_combo),
        BitmapTextStyle { x: 0.065, y: 0.15, cell: 0.0055, color: Color::rgb(0.82, 0.9, 0.92) },
    );
    text.push_text(
        commands,
        &format!("NOTE {}", snapshot.past_notes.min(snapshot.total_notes)),
        BitmapTextStyle { x: 0.065, y: 0.195, cell: 0.005, color: Color::rgb(0.68, 0.78, 0.8) },
    );
    text.push_text(
        commands,
        &format!("/{}", snapshot.total_notes),
        BitmapTextStyle { x: 0.065, y: 0.235, cell: 0.005, color: Color::rgb(0.68, 0.78, 0.8) },
    );
    text.push_text(
        commands,
        &format_time(snapshot.time),
        BitmapTextStyle { x: 0.065, y: 0.28, cell: 0.0045, color: Color::rgb(0.48, 0.62, 0.66) },
    );
}

fn format_delta_ms(delta_us: i64) -> String {
    let sign = if delta_us < 0 { "-" } else { "+" };
    format!("{}{}MS", sign, delta_us.abs() / 1_000)
}

fn format_time(time: TimeUs) -> String {
    let seconds = (time.0.max(0) / 1_000_000) as u32;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    if let Some(judgement_color) = judgement_lane_flash_color(snapshot, lane) {
        return Some(judgement_color);
    }

    input_lane_flash_color(snapshot, lane)
}

fn judgement_lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    let judgement = snapshot.recent_judgements.iter().rev().find(|judgement| {
        judgement.lane == lane && (0..=220_000).contains(&(snapshot.time.0 - judgement.time.0))
    })?;
    let age_us = (snapshot.time.0 - judgement.time.0).max(0) as f32;
    let alpha = (1.0 - age_us / 220_000.0).clamp(0.0, 1.0) * 0.55;
    Some(judge_flash_color(&judgement.text, alpha))
}

fn input_lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    let input = snapshot.recent_inputs.iter().rev().find(|input| {
        input.lane == lane && (0..=140_000).contains(&(snapshot.time.0 - input.time.0))
    })?;
    let age_us = (snapshot.time.0 - input.time.0).max(0) as f32;
    let alpha = (1.0 - age_us / 140_000.0).clamp(0.0, 1.0) * 0.32;
    Some(Color::rgba(0.95, 0.98, 1.0, alpha))
}

fn judge_flash_color(text: &str, alpha: f32) -> Color {
    if text.starts_with("PGREAT") || text.starts_with("GREAT") {
        Color::rgba(0.55, 0.9, 1.0, alpha)
    } else if text.starts_with("GOOD") {
        Color::rgba(0.85, 0.9, 0.45, alpha)
    } else {
        Color::rgba(1.0, 0.28, 0.32, alpha)
    }
}

fn display_title(title: &str) -> String {
    display_label(title, 24)
}

fn display_label(text: &str, max_chars: usize) -> String {
    let ascii: String = text
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '.' | '/' | ':') {
                ch
            } else {
                '?'
            }
        })
        .take(max_chars)
        .collect();
    if ascii.is_empty() { "NO TITLE".to_string() } else { ascii }
}

fn gauge_color(gauge: f32) -> Color {
    if gauge >= 80.0 {
        Color::rgb(0.35, 0.9, 0.6)
    } else if gauge >= 30.0 {
        Color::rgb(0.9, 0.78, 0.35)
    } else {
        Color::rgb(0.9, 0.32, 0.32)
    }
}

fn receptor_color(lane: Lane) -> Color {
    match lane {
        Lane::Scratch => Color::rgb(0.22, 0.34, 0.3),
        Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => Color::rgb(0.24, 0.26, 0.28),
        Lane::Key2 | Lane::Key4 | Lane::Key6 => Color::rgb(0.14, 0.26, 0.36),
    }
}

fn note_color(lane: Lane) -> Color {
    match lane {
        Lane::Scratch => Color::rgb(0.45, 0.7, 0.62),
        Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => Color::rgb(0.82, 0.86, 0.9),
        Lane::Key2 | Lane::Key4 | Lane::Key6 => Color::rgb(0.35, 0.68, 0.95),
    }
}

fn lane_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Scratch => "SC",
        Lane::Key1 => "1",
        Lane::Key2 => "2",
        Lane::Key3 => "3",
        Lane::Key4 => "4",
        Lane::Key5 => "5",
        Lane::Key6 => "6",
        Lane::Key7 => "7",
    }
}

fn lane_key_label(lane: Lane) -> &'static str {
    match lane {
        Lane::Scratch => "LS",
        Lane::Key1 => "Z",
        Lane::Key2 => "S",
        Lane::Key3 => "X",
        Lane::Key4 => "D",
        Lane::Key5 => "C",
        Lane::Key6 => "F",
        Lane::Key7 => "V",
    }
}

fn label_width(label: &str, cell: f32) -> f32 {
    let chars = label.chars().count() as f32;
    if chars == 0.0 { 0.0 } else { (chars * 3.0 + (chars - 1.0)) * cell }
}

#[cfg(test)]
mod tests {
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use crate::snapshot::{
        DisplayInput, DisplayJudgement, RenderSnapshot, VisibleBarLine, VisibleNote,
    };

    use super::*;

    #[test]
    fn play_plan_includes_lanes_notes_and_bar_lines() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.5,
        });
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(900), y: 0.25 });

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.len() >= LANE_COUNT + 3);
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if *color == note_color(Lane::Key1)
        )));
    }

    #[test]
    fn select_plan_has_non_empty_commands() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(Default::default()));

        assert!(!plan.commands.is_empty());
    }

    #[test]
    fn select_plan_clamps_visible_rows() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(crate::scene::SelectSnapshot {
            chart_count: 20,
            rows: select_rows(20),
            ..Default::default()
        }));

        let selected_row_color = Color::rgb(0.22, 0.28, 0.31);
        let row_color = Color::rgb(0.075, 0.09, 0.1);
        let row_count = plan
            .commands
            .iter()
            .filter(|command| matches!(
                command,
                DrawCommand::Rect { color, .. } if *color == selected_row_color || *color == row_color
            ))
            .count();
        assert_eq!(row_count, 7);
    }

    #[test]
    fn select_plan_renders_empty_row_when_no_rows_are_available() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(Default::default()));

        let selected_row_color = Color::rgb(0.22, 0.28, 0.31);
        let row_count = plan
            .commands
            .iter()
            .filter(|command| {
                matches!(command, DrawCommand::Rect { color, .. } if *color == selected_row_color)
            })
            .count();
        assert_eq!(row_count, 1);
    }

    #[test]
    fn result_plan_clamps_ex_score_bar() {
        let plan = plan_result("Normal", 0, 1.5, 0, 0.0);

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.width == 0.72 && *color == Color::rgb(0.55, 0.78, 0.86)
        )));
    }

    #[test]
    fn play_plan_includes_judge_line_gauge_and_combo_panel() {
        let snapshot = RenderSnapshot {
            combo: 1234,
            max_combo: 1234,
            ex_score: 2000,
            total_notes: 1200,
            past_notes: 900,
            gauge: 82.0,
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if *color == Color::rgb(0.96, 0.92, 0.54)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if *color == gauge_color(82.0)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.width >= 0.2 && *color == Color::rgb(0.14, 0.18, 0.2)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.05 && rect.width == 0.11 && *color == Color::rgb(0.035, 0.04, 0.044)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if *color == receptor_color(Lane::Key2)
        )));
    }

    #[test]
    fn play_plan_flashes_recent_judgement_lane() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key2,
                text: "PGREAT FAST".to_string(),
                delta_us: -3_000,
                time: TimeUs(920_000),
            }],
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if *color == judge_flash_color("PGREAT FAST", 0.35)
        )));
    }

    #[test]
    fn lane_flash_expires_old_judgements() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key2,
                text: "BAD SLOW".to_string(),
                delta_us: 88_000,
                time: TimeUs(700_000),
            }],
            ..Default::default()
        };

        assert_eq!(lane_flash_color(&snapshot, Lane::Key2), None);
    }

    #[test]
    fn play_plan_flashes_recent_input_lane_without_judgement() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_inputs: vec![DisplayInput { lane: Lane::Key4, time: TimeUs(930_000) }],
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if *color == Color::rgba(0.95, 0.98, 1.0, 0.16)
        )));
    }

    #[test]
    fn input_lane_flash_expires_old_inputs() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_inputs: vec![DisplayInput { lane: Lane::Key4, time: TimeUs(800_000) }],
            ..Default::default()
        };

        assert_eq!(input_lane_flash_color(&snapshot, Lane::Key4), None);
    }

    #[test]
    fn gauge_color_reflects_life_thresholds() {
        assert_eq!(gauge_color(90.0), Color::rgb(0.35, 0.9, 0.6));
        assert_eq!(gauge_color(50.0), Color::rgb(0.9, 0.78, 0.35));
        assert_eq!(gauge_color(10.0), Color::rgb(0.9, 0.32, 0.32));
    }

    #[test]
    fn lane_text_labels_match_default_bindings() {
        assert_eq!(lane_label(Lane::Scratch), "SC");
        assert_eq!(lane_label(Lane::Key7), "7");
        assert_eq!(lane_key_label(Lane::Scratch), "LS");
        assert_eq!(lane_key_label(Lane::Key1), "Z");
        assert_eq!(lane_key_label(Lane::Key7), "V");
    }

    #[test]
    fn display_title_falls_back_and_sanitizes_non_ascii() {
        assert_eq!(display_title(""), "NO TITLE");
        assert_eq!(display_title("AあB"), "A?B");
    }

    #[test]
    fn display_label_sanitizes_and_truncates_text() {
        assert_eq!(display_label("FullCombo!!", 8), "FullComb");
        assert_eq!(display_label("A_B", 8), "A?B");
    }

    #[test]
    fn play_text_formats_delta_and_time() {
        assert_eq!(format_delta_ms(-12_345), "-12MS");
        assert_eq!(format_delta_ms(8_999), "+8MS");
        assert_eq!(format_time(TimeUs(65_000_000)), "01:05");
    }

    #[test]
    fn judge_flash_color_reflects_judge_family() {
        assert_eq!(judge_flash_color("GREAT SLOW", 0.5), Color::rgba(0.55, 0.9, 1.0, 0.5));
        assert_eq!(judge_flash_color("GOOD FAST", 0.5), Color::rgba(0.85, 0.9, 0.45, 0.5));
        assert_eq!(judge_flash_color("POOR SLOW", 0.5), Color::rgba(1.0, 0.28, 0.32, 0.5));
    }

    #[test]
    fn clear_type_label_abbreviates_long_names() {
        assert_eq!(clear_type_label("Normal"), "NORMAL");
        assert_eq!(clear_type_label("LightAssistEasy"), "LAEASY");
        assert_eq!(clear_type_label("FullCombo"), "FC");
        assert_eq!(clear_type_label(""), "");
    }

    fn select_rows(count: u32) -> Vec<crate::scene::SelectRowSnapshot> {
        (0..count)
            .map(|index| crate::scene::SelectRowSnapshot {
                index,
                title: format!("Title {index}"),
                artist: format!("Artist {index}"),
                play_level: index.to_string(),
                clear_type: if index == 0 { "Normal".to_string() } else { String::new() },
                ex_score: (index == 0).then_some(1234),
            })
            .collect()
    }
}
