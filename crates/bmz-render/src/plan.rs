use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, SelectRowSnapshot};
use crate::skin::{
    Animation, BlendMode, NumberSlot, SkinContext, SkinDefinition, SkinManifest, SkinObject,
    SkinObjectId, SkinPhase, SkinPlacement, SkinRenderContext, SkinRenderItem, SkinSource,
    SkinTextureId, TextSlot, append_skin_render_items,
};
use crate::snapshot::{DisplayJudgeCounts, RenderSnapshot};
use crate::text::{BitmapTextStyle, TextRenderer};

const JUDGE_LINE_Y_RATIO: f32 = 0.86;
const NOTE_HEIGHT: f32 = 0.018;
pub const DEFAULT_NOTE_TEXTURE: TextureId = TextureId(1);
pub const DEFAULT_KEY_EVEN_NOTE_TEXTURE: TextureId = TextureId(2);
pub const DEFAULT_SCRATCH_NOTE_TEXTURE: TextureId = TextureId(3);
pub const DEFAULT_RECEPTOR_TEXTURE: TextureId = TextureId(4);
pub const DEFAULT_KEY_EVEN_RECEPTOR_TEXTURE: TextureId = TextureId(5);
pub const DEFAULT_SCRATCH_RECEPTOR_TEXTURE: TextureId = TextureId(6);
pub const DEFAULT_JUDGE_LINE_TEXTURE: TextureId = TextureId(7);
pub const DEFAULT_GAUGE_FRAME_TEXTURE: TextureId = TextureId(8);
pub const DEFAULT_GAUGE_FILL_TEXTURE: TextureId = TextureId(9);
pub const DEFAULT_COMBO_PANEL_TEXTURE: TextureId = TextureId(10);
pub const DEFAULT_COMBO_PANEL_INACTIVE_TEXTURE: TextureId = TextureId(11);

#[derive(Debug, Clone, PartialEq)]
pub struct DrawPlan {
    pub clear: Color,
    pub commands: Vec<DrawCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect { rect: Rect, color: Color },
    Image { rect: Rect, uv: UvRect, texture: TextureId, tint: Color },
    Text { origin: Point, text: String, style: TextStyle },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub color: Color,
    pub layer: TextLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextLayer {
    Ui,
    Skin,
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
        Self::from_scene_with_skin(scene, &SkinContext::default())
    }

    pub fn from_scene_with_skin(scene: &AppSceneSnapshot, skin: &SkinContext) -> Self {
        match scene {
            AppSceneSnapshot::Select(snapshot) => {
                plan_select(snapshot.chart_count, snapshot.selected_index, &snapshot.rows)
            }
            AppSceneSnapshot::Play(snapshot) => plan_play(snapshot, skin),
            AppSceneSnapshot::Result(snapshot) => plan_result(
                snapshot.clear_type.as_str(),
                snapshot.ex_score,
                snapshot.ex_score_rate,
                snapshot.max_combo,
                snapshot.gauge_value,
                snapshot.total_notes,
                &snapshot.judge_counts,
                snapshot.score_history_id,
                snapshot.replay_saved,
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

fn plan_play(snapshot: &RenderSnapshot, skin: &SkinContext) -> DrawPlan {
    let mut commands = Vec::new();
    let text = TextRenderer;
    let skin_manifest = skin.manifest();
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
    append_skin_render_items(
        &mut commands,
        &skin.static_document_items_for_state(crate::skin::SkinDrawState {
            elapsed_ms: (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            gauge: snapshot.gauge,
        }),
    );
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
            let y = note_rect_y(board, note.y);
            let rect =
                Rect { x: x + lane_width * 0.08, y, width: lane_width * 0.84, height: NOTE_HEIGHT };
            if let Some(item) = skin.document_note_item(lane, rect) {
                append_skin_render_items(&mut commands, &[item]);
            } else {
                push_default_note_skin(&skin_manifest, &mut commands, lane, rect);
            }
        }
    }

    push_receptors(&skin_manifest, &mut commands, board, lane_width);
    for bar in &snapshot.bar_lines {
        let y = play_object_y(board, bar.y);
        commands.push(DrawCommand::Rect {
            rect: Rect { x: board.x, y, width: board.width, height: 0.004 },
            color: Color::rgb(0.45, 0.48, 0.5),
        });
    }
    push_judge_line(&skin_manifest, &mut commands, board);
    push_gauge(
        skin,
        &skin_manifest,
        &mut commands,
        snapshot.gauge,
        (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
    );
    push_combo_panel(&skin_manifest, &mut commands, snapshot.combo);
    push_default_play_skin(skin, &mut commands, snapshot);
    push_play_text(&text, &mut commands, snapshot);
    push_lane_text(&text, &mut commands, board, lane_width);
    push_judgement_history(&text, &mut commands, snapshot);
    push_start_overlay(&text, &mut commands, snapshot);

    DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands }
}

fn plan_result(
    clear_type: &str,
    ex_score: u32,
    ex_score_rate: f32,
    max_combo: u32,
    gauge_value: f32,
    total_notes: u32,
    judge_counts: &DisplayJudgeCounts,
    score_history_id: i64,
    replay_saved: bool,
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
        &format!("RATE {}", format_percent(ex_score_rate)),
        BitmapTextStyle { x: 0.16, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        &format!("NOTES {}", total_notes),
        BitmapTextStyle { x: 0.34, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        &format!("ID {}", score_history_id.max(0)),
        BitmapTextStyle { x: 0.52, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        if replay_saved { "REPLAY SAVED" } else { "REPLAY NONE" },
        BitmapTextStyle { x: 0.68, y: 0.675, cell: 0.005, color: Color::rgb(0.66, 0.78, 0.76) },
    );
    for (index, label) in result_judge_labels(judge_counts).into_iter().enumerate() {
        let column = index % 3;
        let row = index / 3;
        text.push_text(
            &mut commands,
            &label,
            BitmapTextStyle {
                x: 0.16 + column as f32 * 0.18,
                y: 0.735 + row as f32 * 0.045,
                cell: 0.006,
                color: Color::rgb(0.78, 0.82, 0.8),
            },
        );
    }
    text.push_text(
        &mut commands,
        "R RETRY  ENTER/ESC SELECT",
        BitmapTextStyle { x: 0.14, y: 0.86, cell: 0.006, color: Color::rgb(0.74, 0.78, 0.8) },
    );

    DrawPlan { clear: Color::rgb(0.025, 0.02, 0.018), commands }
}

fn result_judge_labels(judge_counts: &DisplayJudgeCounts) -> [String; 6] {
    [
        format!("PG {}", judge_counts.pgreat),
        format!("GR {}", judge_counts.great),
        format!("GD {}", judge_counts.good),
        format!("BD {}", judge_counts.bad),
        format!("PR {}", judge_counts.poor),
        format!("EP {}", judge_counts.empty_poor),
    ]
}

fn push_judge_line(skin_manifest: &SkinManifest, commands: &mut Vec<DrawCommand>, board: Rect) {
    let image = skin_manifest.play_judge_line_image();
    let line_y = judge_line_y(board);
    append_skin_render_items(
        commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(image.texture),
            rect: Rect { x: board.x, y: line_y, width: board.width, height: 0.006 },
            uv: image.uv,
            tint: skin_image_tint(Lane::Key1),
            blend: BlendMode::Normal,
            scale: image.scale,
            border: image.border,
            source_size: image.source_size,
        }],
    );
}

fn note_rect_y(board: Rect, progress_to_hit: f32) -> f32 {
    (play_object_y(board, progress_to_hit) - NOTE_HEIGHT / 2.0).max(board.y)
}

fn play_object_y(board: Rect, progress_to_hit: f32) -> f32 {
    let judge_y = judge_line_y(board);
    judge_y - progress_to_hit.clamp(0.0, 1.0) * (judge_y - board.y)
}

fn judge_line_y(board: Rect) -> f32 {
    board.y + board.height * JUDGE_LINE_Y_RATIO
}

fn push_receptors(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lane_width: f32,
) {
    let receptor = skin_manifest.play_receptor_image();
    let receptor_y = board.y + board.height * 0.825;
    for lane in Lane::ALL {
        let lane_index = lane.index();
        let x = board.x + lane_index as f32 * lane_width;
        append_skin_render_items(
            commands,
            &[SkinRenderItem::Image {
                texture: SkinTextureId(receptor.texture_for_lane(lane)),
                rect: Rect {
                    x: x + lane_width * 0.1,
                    y: receptor_y,
                    width: lane_width * 0.8,
                    height: 0.026,
                },
                uv: receptor.uv,
                tint: skin_image_tint(lane),
                blend: BlendMode::Normal,
                scale: receptor.scale,
                border: receptor.border,
                source_size: receptor.source_size,
            }],
        );
    }
}

fn push_gauge(
    skin: &SkinContext,
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    gauge: f32,
    elapsed_ms: i32,
) {
    if let Some(items) = skin.document_gauge_items(gauge, elapsed_ms) {
        append_skin_render_items(commands, &items);
        return;
    }

    let frame = Rect { x: 0.84, y: 0.08, width: 0.035, height: 0.82 };
    let fill = gauge.clamp(0.0, 100.0) / 100.0;
    let frame_image = skin_manifest.play_gauge_frame_image();
    let fill_image = skin_manifest.play_gauge_fill_image();
    append_skin_render_items(
        commands,
        &[
            SkinRenderItem::Image {
                texture: SkinTextureId(frame_image.texture),
                rect: frame,
                uv: frame_image.uv,
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                scale: frame_image.scale,
                border: frame_image.border,
                source_size: frame_image.source_size,
            },
            SkinRenderItem::Image {
                texture: SkinTextureId(fill_image.texture),
                rect: Rect {
                    x: frame.x + 0.006,
                    y: frame.y + frame.height * (1.0 - fill),
                    width: frame.width - 0.012,
                    height: frame.height * fill,
                },
                uv: fill_image.uv,
                tint: gauge_color(gauge),
                blend: BlendMode::Normal,
                scale: fill_image.scale,
                border: fill_image.border,
                source_size: fill_image.source_size,
            },
        ],
    );
}

fn push_combo_panel(skin_manifest: &SkinManifest, commands: &mut Vec<DrawCommand>, combo: u32) {
    let width = if combo >= 1000 { 0.2 } else { 0.15 };
    let image = skin_manifest.play_combo_panel_image(combo > 0);
    append_skin_render_items(
        commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(image.texture),
            rect: Rect { x: 0.425 - width / 2.0, y: 0.16, width, height: 0.07 },
            uv: image.uv,
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: image.scale,
            border: image.border,
            source_size: image.source_size,
        }],
    );
}

fn push_play_text(text: &TextRenderer, commands: &mut Vec<DrawCommand>, snapshot: &RenderSnapshot) {
    push_play_status_text(text, commands, snapshot);
    push_judge_count_text(text, commands, snapshot);
    text.push_text(
        commands,
        &format!("G{}", snapshot.gauge.round() as u32),
        BitmapTextStyle { x: 0.885, y: 0.08, cell: 0.007, color: Color::rgb(0.8, 0.92, 0.86) },
    );
    if let Some(judgement) = snapshot.recent_judgements.last() {
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

fn push_default_play_skin(
    skin_context: &SkinContext,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let skin = default_play_skin(snapshot);
    let recent_judgement = snapshot.recent_judgements.last();
    let judge_text = recent_judgement.map(|judgement| judgement.text.clone()).unwrap_or_default();
    let judge_image = recent_judgement.and_then(|judgement| {
        skin_context.document_judge_item(
            &judgement.text,
            ((snapshot.time.0 - judgement.time.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
        )
    });
    let has_judge_image = judge_image.is_some();
    if let Some(judge_image) = judge_image {
        append_skin_render_items(commands, &[judge_image]);
    }
    let text_values = [(TextSlot::Judge, judge_text)];
    let number_values = [
        (NumberSlot::Combo, snapshot.combo as i64),
        (NumberSlot::Gauge, snapshot.gauge.round() as i64),
        (NumberSlot::Hispeed, (snapshot.hispeed * 100.0).round() as i64),
    ];
    let items = skin.resolve(&SkinRenderContext {
        phase: SkinPhase::Play,
        elapsed_ms: (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        text: &text_values,
        numbers: &number_values,
    });
    let items = if has_judge_image {
        items
            .into_iter()
            .filter(|item| !matches!(item, SkinRenderItem::Text { text, .. } if text == &text_values[0].1))
            .collect::<Vec<_>>()
    } else {
        items
    };
    append_skin_render_items(commands, &items);
}

fn push_default_note_skin(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
) {
    let note = skin_manifest.play_note_image();
    append_skin_render_items(
        commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(note.texture_for_lane(lane)),
            rect,
            uv: note.uv,
            tint: skin_image_tint(lane),
            blend: BlendMode::Normal,
            scale: note.scale,
            border: note.border,
            source_size: note.source_size,
        }],
    );
}

fn default_play_skin(snapshot: &RenderSnapshot) -> SkinDefinition {
    let mut objects = Vec::new();
    if snapshot.combo > 0 {
        objects.push(SkinObject {
            id: SkinObjectId(1),
            source: SkinSource::Number {
                slot: NumberSlot::Combo,
                style: TextStyle {
                    size: 0.05,
                    color: Color::rgb(0.94, 0.98, 1.0),
                    layer: TextLayer::Skin,
                },
                digits: 0,
            },
            placements: vec![skin_placement(Rect { x: 0.38, y: 0.18, width: 0.18, height: 0.07 })],
        });
    }

    if snapshot.recent_judgements.last().is_some() {
        objects.push(SkinObject {
            id: SkinObjectId(2),
            source: SkinSource::Text {
                slot: TextSlot::Judge,
                style: TextStyle {
                    size: 0.03,
                    color: Color::rgb(0.96, 0.92, 0.54),
                    layer: TextLayer::Skin,
                },
            },
            placements: vec![skin_placement(Rect { x: 0.38, y: 0.245, width: 0.3, height: 0.04 })],
        });
    }

    SkinDefinition { objects }
}

fn skin_placement(rect: Rect) -> SkinPlacement {
    SkinPlacement {
        phase: SkinPhase::Play,
        time_ms: 0,
        rect,
        alpha: 1.0,
        blend: BlendMode::Normal,
        animation: Animation::none(),
    }
}

fn push_start_overlay(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let Some(label) = start_overlay_label(snapshot.time) else {
        return;
    };
    let cell = 0.018;
    text.push_text(
        commands,
        label,
        BitmapTextStyle {
            x: 0.5 - label_width(label, cell) / 2.0,
            y: 0.385,
            cell,
            color: if label == "READY" {
                Color::rgb(0.74, 0.88, 0.9)
            } else {
                Color::rgb(0.96, 0.92, 0.54)
            },
        },
    );
}

fn push_judge_count_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.05, y: 0.36, width: 0.11, height: 0.235 },
        color: Color::rgb(0.032, 0.036, 0.04),
    });

    let rows = [
        ("PG", snapshot.judge_counts.pgreat, Color::rgb(0.66, 0.92, 0.98)),
        ("GR", snapshot.judge_counts.great, Color::rgb(0.66, 0.92, 0.98)),
        ("GD", snapshot.judge_counts.good, Color::rgb(0.84, 0.88, 0.48)),
        ("BD", snapshot.judge_counts.bad, Color::rgb(0.94, 0.56, 0.36)),
        ("PR", snapshot.judge_counts.poor, Color::rgb(0.96, 0.4, 0.44)),
        ("EP", snapshot.judge_counts.empty_poor, Color::rgb(0.96, 0.4, 0.44)),
    ];

    for (index, (label, count, color)) in rows.into_iter().enumerate() {
        text.push_text(
            commands,
            &format!("{label} {count}"),
            BitmapTextStyle { x: 0.065, y: 0.382 + index as f32 * 0.032, cell: 0.004, color },
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
        rect: Rect { x: 0.05, y: 0.08, width: 0.11, height: 0.285 },
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
    text.push_text(
        commands,
        &format!("HS {:.2}", snapshot.hispeed),
        BitmapTextStyle { x: 0.065, y: 0.32, cell: 0.0045, color: Color::rgb(0.72, 0.82, 0.8) },
    );
}

fn push_judgement_history(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    if snapshot.recent_judgements.is_empty() {
        return;
    }

    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.885, y: 0.17, width: 0.09, height: 0.19 },
        color: Color::rgb(0.03, 0.035, 0.038),
    });
    text.push_text(
        commands,
        "JUDGE",
        BitmapTextStyle { x: 0.897, y: 0.188, cell: 0.004, color: Color::rgb(0.5, 0.62, 0.64) },
    );

    for (index, judgement) in snapshot.recent_judgements.iter().rev().take(4).enumerate() {
        let y = 0.225 + index as f32 * 0.032;
        text.push_text(
            commands,
            &judgement_history_label(judgement),
            BitmapTextStyle {
                x: 0.897,
                y,
                cell: 0.0038,
                color: judgement_history_color(&judgement.text),
            },
        );
    }
}

fn format_delta_ms(delta_us: i64) -> String {
    let sign = if delta_us < 0 { "-" } else { "+" };
    format!("{}{}MS", sign, delta_us.abs() / 1_000)
}

fn format_percent(rate: f32) -> String {
    format!("{}%", (rate.clamp(0.0, 1.0) * 100.0).round() as u32)
}

fn format_time(time: TimeUs) -> String {
    let seconds = (time.0.max(0) / 1_000_000) as u32;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn start_overlay_label(time: TimeUs) -> Option<&'static str> {
    match time.0 {
        ..=999_999 => Some("READY"),
        1_000_000..=1_599_999 => Some("GO"),
        _ => None,
    }
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

fn judgement_history_label(judgement: &crate::snapshot::DisplayJudgement) -> String {
    format!("{} {}", judge_short_label(&judgement.text), side_short_label(&judgement.text))
}

fn judge_short_label(text: &str) -> &'static str {
    if text.starts_with("PGREAT") {
        "PG"
    } else if text.starts_with("GREAT") {
        "GR"
    } else if text.starts_with("GOOD") {
        "GD"
    } else if text.starts_with("BAD") {
        "BD"
    } else if text.starts_with("EMPTY POOR") {
        "EP"
    } else if text.starts_with("POOR") {
        "PR"
    } else {
        "??"
    }
}

fn side_short_label(text: &str) -> &'static str {
    if text.ends_with("FAST") {
        "F"
    } else if text.ends_with("SLOW") {
        "S"
    } else {
        "-"
    }
}

fn judgement_history_color(text: &str) -> Color {
    if text.starts_with("PGREAT") || text.starts_with("GREAT") {
        Color::rgb(0.64, 0.9, 0.98)
    } else if text.starts_with("GOOD") {
        Color::rgb(0.84, 0.88, 0.48)
    } else {
        Color::rgb(0.96, 0.4, 0.44)
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

fn skin_image_tint(_lane: Lane) -> Color {
    Color::rgb(1.0, 1.0, 1.0)
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
        DisplayInput, DisplayJudgeCounts, DisplayJudgement, RenderSnapshot, VisibleBarLine,
        VisibleNote,
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
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_NOTE_TEXTURE && *tint == skin_image_tint(Lane::Key1)
        )));
    }

    #[test]
    fn play_plan_uses_note_textures_by_lane() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Scratch.index()].push(VisibleNote {
            lane: Lane::Scratch,
            time: TimeUs(1_000),
            y: 0.5,
        });
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.5,
        });
        snapshot.visible_notes[Lane::Key2.index()].push(VisibleNote {
            lane: Lane::Key2,
            time: TimeUs(1_000),
            y: 0.5,
        });

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == DEFAULT_SCRATCH_NOTE_TEXTURE
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == DEFAULT_NOTE_TEXTURE
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == DEFAULT_KEY_EVEN_NOTE_TEXTURE
        )));
    }

    #[test]
    fn play_plan_uses_supplied_skin_context() {
        let manifest: SkinManifest = toml::from_str(
            r#"
            [play.note]
            texture = 42
            "#,
        )
        .unwrap();
        let skin = SkinContext::from_manifest(manifest);
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.5,
        });

        let plan = DrawPlan::from_scene_with_skin(&AppSceneSnapshot::Play(snapshot), &skin);

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == TextureId(42)
        )));
    }

    #[test]
    fn play_plan_maps_normalized_note_y_to_distinct_screen_positions() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.75,
        });
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(2_000),
            y: 0.25,
        });

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));
        let note_ys: Vec<f32> = plan
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Image { rect, texture, .. } if *texture == DEFAULT_NOTE_TEXTURE => {
                    Some(rect.y)
                }
                _ => None,
            })
            .collect();

        assert!(note_ys.iter().any(|y| approx_eq(*y, 0.2345)));
        assert!(note_ys.iter().any(|y| approx_eq(*y, 0.6215)));
    }

    #[test]
    fn play_plan_places_hit_timing_note_on_judge_line() {
        let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };

        assert!(approx_eq(note_rect_y(board, 0.0) + NOTE_HEIGHT / 2.0, judge_line_y(board)));
    }

    #[test]
    fn start_overlay_label_covers_opening_window() {
        assert_eq!(start_overlay_label(TimeUs(0)), Some("READY"));
        assert_eq!(start_overlay_label(TimeUs(999_999)), Some("READY"));
        assert_eq!(start_overlay_label(TimeUs(1_000_000)), Some("GO"));
        assert_eq!(start_overlay_label(TimeUs(1_599_999)), Some("GO"));
        assert_eq!(start_overlay_label(TimeUs(1_600_000)), None);
    }

    #[test]
    fn play_plan_includes_ready_overlay_at_start() {
        let snapshot = RenderSnapshot { time: TimeUs(0), ..Default::default() };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { style, .. } if style.color == Color::rgb(0.74, 0.88, 0.9)
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
        let plan =
            plan_result("Normal", 0, 1.5, 0, 0.0, 100, &DisplayJudgeCounts::default(), 1, true);

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.width == 0.72 && *color == Color::rgb(0.55, 0.78, 0.86)
        )));
    }

    #[test]
    fn result_plan_includes_extended_summary_text() {
        let plan = plan_result(
            "Normal",
            1500,
            0.75,
            500,
            82.0,
            1000,
            &DisplayJudgeCounts::default(),
            42,
            true,
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { style, .. } if style.color == Color::rgb(0.72, 0.84, 0.86)
        )));
        assert_eq!(format_percent(0.754), "75%");
    }

    #[test]
    fn result_judge_labels_include_all_counts() {
        let labels = result_judge_labels(&DisplayJudgeCounts {
            pgreat: 1,
            great: 2,
            good: 3,
            bad: 4,
            poor: 5,
            empty_poor: 6,
        });

        assert_eq!(labels, ["PG 1", "GR 2", "GD 3", "BD 4", "PR 5", "EP 6"]);
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
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_JUDGE_LINE_TEXTURE && *tint == skin_image_tint(Lane::Key1)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_GAUGE_FRAME_TEXTURE && *tint == Color::rgb(1.0, 1.0, 1.0)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_GAUGE_FILL_TEXTURE && *tint == gauge_color(82.0)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_COMBO_PANEL_TEXTURE && *tint == Color::rgb(1.0, 1.0, 1.0)
        )));
        assert_eq!(
            plan.commands
                .iter()
                .filter(|command| matches!(
                    command,
                    DrawCommand::Image { texture, .. } if *texture == DEFAULT_COMBO_PANEL_TEXTURE
                ))
                .count(),
            9
        );
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.05 && rect.width == 0.11 && *color == Color::rgb(0.035, 0.04, 0.044)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.05 && rect.y == 0.36 && *color == Color::rgb(0.032, 0.036, 0.04)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_KEY_EVEN_RECEPTOR_TEXTURE && *tint == skin_image_tint(Lane::Key2)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_SCRATCH_RECEPTOR_TEXTURE && *tint == skin_image_tint(Lane::Scratch)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, style, .. }
                if text == "1234" && style.layer == TextLayer::Skin
        )));
    }

    #[test]
    fn play_plan_routes_recent_judge_text_through_default_skin() {
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
            DrawCommand::Text { text, style, .. }
                if text == "PGREAT FAST" && style.layer == TextLayer::Skin
        )));
    }

    #[test]
    fn play_plan_includes_judge_count_text() {
        let snapshot = RenderSnapshot {
            judge_counts: DisplayJudgeCounts {
                pgreat: 2,
                great: 1,
                good: 1,
                bad: 1,
                poor: 1,
                empty_poor: 3,
            },
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { style, .. } if style.color == Color::rgb(0.66, 0.92, 0.98)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { style, .. } if style.color == Color::rgb(0.96, 0.4, 0.44)
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
    fn play_plan_includes_recent_judgement_history_panel() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key2,
                text: "EMPTY POOR SLOW".to_string(),
                delta_us: 50_000,
                time: TimeUs(980_000),
            }],
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.x == 0.885 && rect.y == 0.17 && *color == Color::rgb(0.03, 0.035, 0.038)
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
    fn judgement_history_label_abbreviates_judges_and_sides() {
        assert_eq!(history_label("PGREAT FAST"), "PG F");
        assert_eq!(history_label("GREAT SLOW"), "GR S");
        assert_eq!(history_label("GOOD FAST"), "GD F");
        assert_eq!(history_label("BAD SLOW"), "BD S");
        assert_eq!(history_label("POOR FAST"), "PR F");
        assert_eq!(history_label("EMPTY POOR SLOW"), "EP S");
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

    fn history_label(text: &str) -> String {
        judgement_history_label(&DisplayJudgement {
            lane: Lane::Key1,
            text: text.to_string(),
            delta_us: 0,
            time: TimeUs(0),
        })
    }

    fn approx_eq(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.0001
    }
}
