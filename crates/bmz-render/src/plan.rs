use bmz_core::lane::{LANE_COUNT, Lane};

use crate::scene::AppSceneSnapshot;
use crate::snapshot::RenderSnapshot;

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
            AppSceneSnapshot::Select(_) => Self {
                clear: Color::rgb(0.02, 0.025, 0.03),
                commands: vec![DrawCommand::Rect {
                    rect: Rect { x: 0.08, y: 0.18, width: 0.84, height: 0.12 },
                    color: Color::rgb(0.12, 0.15, 0.17),
                }],
            },
            AppSceneSnapshot::Play(snapshot) => plan_play(snapshot),
            AppSceneSnapshot::Result(_) => Self {
                clear: Color::rgb(0.025, 0.02, 0.018),
                commands: vec![DrawCommand::Rect {
                    rect: Rect { x: 0.1, y: 0.18, width: 0.8, height: 0.2 },
                    color: Color::rgb(0.16, 0.13, 0.11),
                }],
            },
        }
    }
}

impl Color {
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color { r: self.r as f64, g: self.g as f64, b: self.b as f64, a: self.a as f64 }
    }
}

fn plan_play(snapshot: &RenderSnapshot) -> DrawPlan {
    let mut commands = Vec::new();
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
    commands.push(DrawCommand::Rect { rect: board, color: Color::rgb(0.025, 0.025, 0.028) });

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

        for note in &snapshot.visible_notes[lane_index] {
            let y = board.y + (1.0 - note.y.clamp(0.0, 1.0)) * board.height;
            commands.push(DrawCommand::Rect {
                rect: Rect { x: x + lane_width * 0.08, y, width: lane_width * 0.84, height: 0.018 },
                color: note_color(lane),
            });
        }
    }

    for bar in &snapshot.bar_lines {
        let y = board.y + (1.0 - bar.y.clamp(0.0, 1.0)) * board.height;
        commands.push(DrawCommand::Rect {
            rect: Rect { x: board.x, y, width: board.width, height: 0.004 },
            color: Color::rgb(0.45, 0.48, 0.5),
        });
    }

    DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands }
}

fn note_color(lane: Lane) -> Color {
    match lane {
        Lane::Scratch => Color::rgb(0.45, 0.7, 0.62),
        Lane::Key1 | Lane::Key3 | Lane::Key5 | Lane::Key7 => Color::rgb(0.82, 0.86, 0.9),
        Lane::Key2 | Lane::Key4 | Lane::Key6 => Color::rgb(0.35, 0.68, 0.95),
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use crate::snapshot::{RenderSnapshot, VisibleBarLine, VisibleNote};

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
}
