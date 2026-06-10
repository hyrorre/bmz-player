use std::sync::Arc;

use bmz_chart::model::LongNoteMode;
use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, SelectRowKind, SelectRowSnapshot, SelectSnapshot};
use crate::skin::{
    Animation, BlendMode, NumberSlot, SkinContext, SkinDefinition, SkinImageManifest, SkinManifest,
    SkinObject, SkinObjectId, SkinPhase, SkinPlacement, SkinRenderContext, SkinRenderItem,
    SkinSource, SkinTextState, SkinTextureId, TextSlot, append_skin_render_items,
    judge_image_index,
};
use crate::skin_offset::{SKIN_OFFSET_BAR_LINE, SkinOffsetValues};
use crate::snapshot::{
    DisplayBgaFrame, DisplayJudgeCounts, FastSlowJudgeCounts, NoteVisualKind, RenderSnapshot,
    ResultGraphSnapshot, ResultTimingPoint,
};
use crate::text::{BitmapTextStyle, TextRenderer};

const JUDGE_LINE_Y_RATIO: f32 = 0.86;
const NOTE_HEIGHT: f32 = 0.018;
/// デフォルトスキンのロングノート胴体色（半透明）。
const LONG_NOTE_BODY_COLOR: Color = Color::rgba(0.5, 0.78, 0.88, 0.5);
const CN_BODY_COLOR: Color = Color::rgba(0.45, 0.88, 0.62, 0.5);
const HCN_BODY_COLOR: Color = Color::rgba(0.95, 0.68, 0.35, 0.5);
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
pub const DEFAULT_MINE_NOTE_TEXTURE: TextureId = TextureId(12);
/// 選曲画面の `#STAGEFILE` 背景。
pub const SELECT_STAGE_TEXTURE: TextureId = TextureId(20);
/// プレイ画面の `#BACKBMP` 背景 (BGA 下)。
pub const PLAY_BACKBMP_TEXTURE: TextureId = TextureId(21);
/// 選曲画面の `#BANNER` 画像。
pub const SELECT_BANNER_TEXTURE: TextureId = TextureId(22);
/// 譜面 BGA (静止画/動画) 用テクスチャ ID の起点。
/// beatoraja スキンは scene ごとに 10000 刻み (play=10000, select=20000, …) を使うため、
/// 20000 帯に置くと select スキン PNG をプレイ中に上書きし、リザルト復帰後も背景が壊れたままになる。
pub const CHART_BGA_TEXTURE_BASE: u32 = 50_000;

fn string_array_refs(values: &[String; 10]) -> [&str; 10] {
    std::array::from_fn(|index| values[index].as_str())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawPlan {
    pub clear: Color,
    pub commands: Vec<DrawCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        color: Color,
    },
    RectBatch {
        rects: Arc<[RectCommand]>,
        cache: Option<RectBatchCache>,
    },
    Image {
        rect: Rect,
        uv: UvRect,
        texture: TextureId,
        tint: Color,
        blend: BlendMode,
        linear_filter: bool,
    },
    RotatedImage {
        rect: Rect,
        uv: UvRect,
        texture: TextureId,
        tint: Color,
        blend: BlendMode,
        linear_filter: bool,
        angle_rad: f32,
        center: Point,
    },
    Text {
        origin: Point,
        text: String,
        style: TextStyle,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectBatchCache {
    pub key: RectBatchCacheKey,
    pub bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RectBatchCacheKey(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectCommand {
    pub rect: Rect,
    pub color: Color,
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

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font_id: Option<String>,
    pub size: f32,
    pub bitmap_size: Option<f32>,
    pub color: Color,
    pub layer: TextLayer,
    pub align: TextAlign,
    pub max_width: f32,
    pub overflow: TextOverflow,
    pub wrapping: bool,
    pub outline: Option<TextOutline>,
    pub shadow: Option<TextShadow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextOutline {
    pub color: Color,
    pub width: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShadow {
    pub color: Color,
    pub offset: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextLayer {
    Ui,
    Skin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Overflow,
    Shrink,
    Truncate,
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
        Self::from_scene_with_skin(
            scene,
            &SkinContext::default(),
            &mut crate::skin::DynamicTimerRuntime::default(),
        )
    }

    pub fn from_scene_with_skin(
        scene: &AppSceneSnapshot,
        skin: &SkinContext,
        dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
    ) -> Self {
        match scene {
            AppSceneSnapshot::Select(snapshot) => plan_select(snapshot, skin, dynamic_timers),
            AppSceneSnapshot::Decide(snapshot) => plan_decide(snapshot, skin, dynamic_timers),
            AppSceneSnapshot::Play(snapshot) => plan_play(snapshot, skin, dynamic_timers),
            AppSceneSnapshot::Result(snapshot) => plan_result(snapshot, skin, dynamic_timers),
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

fn push_exit_hold_indicator(commands: &mut Vec<DrawCommand>, progress: f32) {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.0 {
        return;
    }
    // skin docが画面全体に画像を敷くケースで、DrawCommand::Rect は images より前に描画され
    // 隠れてしまうため、文字レイヤ(=最前面)で全要素を描く。
    const TOTAL_BLOCKS: usize = 16;
    let filled = (progress * TOTAL_BLOCKS as f32).round() as usize;
    let background_bar: String = "\u{2588}".repeat(TOTAL_BLOCKS); // ████ (U+2588)
    let filled_bar: String = "\u{2588}".repeat(filled);
    let text = TextRenderer;
    text.push_text(
        commands,
        &background_bar,
        BitmapTextStyle { x: 0.005, y: 0.005, cell: 0.006, color: Color::rgb(0.22, 0.22, 0.26) },
    );
    if filled > 0 {
        text.push_text(
            commands,
            &filled_bar,
            BitmapTextStyle { x: 0.005, y: 0.005, cell: 0.006, color: Color::rgb(0.92, 0.4, 0.32) },
        );
    }
    text.push_text(
        commands,
        "HOLD ESC TO EXIT",
        BitmapTextStyle { x: 0.005, y: 0.045, cell: 0.005, color: Color::rgb(0.95, 0.95, 0.95) },
    );
}

fn advance_skin_dynamic_timers(
    skin: &SkinContext,
    runtime: &mut crate::skin::DynamicTimerRuntime,
    state: crate::skin::SkinDrawState,
    now_ms: i32,
) -> crate::skin::SkinDrawState {
    skin.document()
        .filter(|document| !document.dynamic_timers.is_empty())
        .map(|document| runtime.advance(document, state, now_ms))
        .unwrap_or(state)
}

fn push_fullscreen_image(commands: &mut Vec<DrawCommand>, texture: TextureId) {
    commands.push(DrawCommand::Image {
        rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        texture,
        tint: Color::rgb(1.0, 1.0, 1.0),
        blend: BlendMode::Normal,
        linear_filter: true,
    });
}

/// デフォルトスキン (skin document 無し) 向けの全画面 BGA 描画。
fn push_fallback_bga_background(commands: &mut Vec<DrawCommand>, snapshot: &RenderSnapshot) {
    if !snapshot.has_bga || !snapshot.bga_enabled || snapshot.bga_stretch == 8 {
        return;
    }
    if let Some(poor) = snapshot.bga_poor {
        push_bga_fullscreen(commands, poor, snapshot.bga_stretch, BlendMode::Normal);
    } else if let Some(base) = snapshot.bga_base {
        push_bga_fullscreen(commands, base, snapshot.bga_stretch, BlendMode::Normal);
    }
    if snapshot.bga_poor.is_none() {
        // Layer / Layer2 は黒クロマキー (beatoraja の layer.frag 相当) で
        // Base の上に重ねる。ただし動画 BGA Layer は beatoraja でも `ffmpeg.frag`
        // を使ってクロマキーを適用しないため、is_video のときは Normal を選ぶ。
        if let Some(layer) = snapshot.bga_layer {
            push_bga_fullscreen(commands, layer, snapshot.bga_stretch, bga_layer_blend(layer));
        }
        if let Some(layer2) = snapshot.bga_layer2 {
            push_bga_fullscreen(commands, layer2, snapshot.bga_stretch, bga_layer_blend(layer2));
        }
    }
}

fn bga_layer_blend(frame: DisplayBgaFrame) -> BlendMode {
    if frame.is_video { BlendMode::Normal } else { BlendMode::LayerMask }
}

fn skin_bga_frame_from_display(frame: DisplayBgaFrame) -> crate::skin::SkinBgaFrame {
    crate::skin::SkinBgaFrame {
        texture: SkinTextureId(frame.texture_id),
        source_size: crate::skin::SkinImageSize { width: frame.width, height: frame.height },
        tint_r: frame.tint_r,
        tint_g: frame.tint_g,
        tint_b: frame.tint_b,
        tint_a: frame.tint_a,
        is_video: frame.is_video,
    }
}

fn push_bga_fullscreen(
    commands: &mut Vec<DrawCommand>,
    frame: DisplayBgaFrame,
    stretch: i32,
    blend: BlendMode,
) {
    let (rect, uv) = bga_fullscreen_geometry(frame.width, frame.height, stretch);
    commands.push(DrawCommand::Image {
        rect,
        uv,
        texture: TextureId(frame.texture_id),
        tint: Color::rgba(frame.tint_r, frame.tint_g, frame.tint_b, frame.tint_a),
        blend,
        linear_filter: true,
    });
}

/// beatoraja BGA stretch 0/1 の簡易版 (全画面 rect = 1x1 正規化座標)。
fn bga_fullscreen_geometry(source_w: f32, source_h: f32, stretch: i32) -> (Rect, UvRect) {
    let source_w = source_w.max(1.0);
    let source_h = source_h.max(1.0);
    let source_aspect = source_w / source_h;

    if stretch == 1 {
        // 縦横比を保って画面内に収める。
        if source_aspect >= 1.0 {
            let height = 1.0 / source_aspect;
            return (
                Rect { x: 0.0, y: (1.0 - height) * 0.5, width: 1.0, height },
                UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            );
        }
        let width = source_aspect;
        return (
            Rect { x: (1.0 - width) * 0.5, y: 0.0, width, height: 1.0 },
            UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        );
    }

    // stretch 0: 画面全体を覆う (center crop)。
    if source_aspect >= 1.0 {
        let uv_width = 1.0 / source_aspect;
        return (
            Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            UvRect { x: (1.0 - uv_width) * 0.5, y: 0.0, width: uv_width, height: 1.0 },
        );
    }
    let uv_height = source_aspect;
    (
        Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        UvRect { x: 0.0, y: (1.0 - uv_height) * 0.5, width: 1.0, height: uv_height },
    )
}

fn push_select_banner_image(commands: &mut Vec<DrawCommand>) {
    commands.push(DrawCommand::Image {
        rect: Rect { x: 0.72, y: 0.16, width: 0.26, height: 0.065 },
        uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        texture: SELECT_BANNER_TEXTURE,
        tint: Color::rgb(1.0, 1.0, 1.0),
        blend: BlendMode::Normal,
        linear_filter: true,
    });
}

fn plan_select(
    snapshot: &SelectSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    if skin.document().is_some_and(|document| document.skin_type == 5) {
        let mut commands = Vec::new();
        crate::skin::append_skin_render_items(
            &mut commands,
            &skin.select_document_items_with_dynamic_timers(snapshot, Some(dynamic_timers)),
        );
        if !commands.is_empty() {
            push_exit_hold_indicator(&mut commands, snapshot.exit_hold_progress);
            push_scene_overlays(&mut commands, &snapshot.overlay);
            return DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
        }
    }

    let chart_count = snapshot.chart_count;
    let selected_index = snapshot.selected_index;
    let rows = &snapshot.rows;

    let mut commands = Vec::new();
    if snapshot.stage_background {
        push_fullscreen_image(&mut commands, SELECT_STAGE_TEXTURE);
    }
    if snapshot.banner_image {
        push_select_banner_image(&mut commands);
    }
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
    if !snapshot.current_folder.is_empty() {
        text.push_text(
            &mut commands,
            &format!("> {}", snapshot.current_folder),
            BitmapTextStyle {
                x: 0.225,
                y: 0.108,
                cell: 0.006,
                color: Color::rgb(0.55, 0.72, 0.78),
            },
        );
    }
    text.push_text(
        &mut commands,
        &format!("{}", chart_count),
        BitmapTextStyle { x: 0.88, y: 0.112, cell: 0.005, color: Color::rgb(0.62, 0.78, 0.84) },
    );

    // Options bar
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.06, y: 0.163, width: 0.88, height: 0.030 },
        color: Color::rgb(0.05, 0.065, 0.08),
    });
    text.push_text(
        &mut commands,
        &format!(
            "ARRANGE: {}   TARGET: {}   GAUGE: {}   ASSIST: {}   BGA: {}",
            snapshot.arrange, snapshot.target, snapshot.gauge, snapshot.assist, snapshot.bga
        ),
        BitmapTextStyle { x: 0.08, y: 0.170, cell: 0.005, color: Color::rgb(0.72, 0.86, 0.92) },
    );
    push_select_option_panel(&text, &mut commands, snapshot);
    if snapshot.in_settings {
        let hint = if snapshot.settings_editing {
            "UP/DOWN/SCR: change   1/3/5/7 or ENTER: save   2/4/6 or LEFT/ESC: cancel"
        } else {
            "1/3/5/7 or ENTER: edit   2/4/6 or LEFT: back"
        };
        text.push_text(
            &mut commands,
            hint,
            BitmapTextStyle {
                x: 0.08,
                y: 0.198,
                cell: 0.0042,
                color: Color::rgb(0.62, 0.78, 0.72),
            },
        );
    }

    let visible_rows = rows.len().max(1);
    let selected_row_position = select_snapshot_selected_row_position(rows, selected_index);
    for row in 0..visible_rows {
        let snapshot_row = rows.get(row);
        let selected = snapshot_row.map_or(row == 0, |_| row == selected_row_position);
        let is_folder = snapshot_row.map(|r| r.is_folder).unwrap_or(false);
        let in_library = snapshot_row.map(|r| r.in_library).unwrap_or(true);
        let row_y = 0.2 + row as f32 * 0.09;
        let (left_bg, right_bg) = if is_folder {
            if selected {
                (Color::rgb(0.26, 0.21, 0.08), Color::rgb(0.20, 0.16, 0.06))
            } else {
                (Color::rgb(0.09, 0.075, 0.03), Color::rgb(0.07, 0.058, 0.023))
            }
        } else if !in_library {
            if selected {
                (Color::rgb(0.14, 0.14, 0.14), Color::rgb(0.10, 0.10, 0.10))
            } else {
                (Color::rgb(0.05, 0.05, 0.055), Color::rgb(0.04, 0.04, 0.045))
            }
        } else if selected {
            (Color::rgb(0.22, 0.28, 0.31), Color::rgb(0.16, 0.21, 0.23))
        } else {
            (Color::rgb(0.075, 0.09, 0.1), Color::rgb(0.055, 0.065, 0.072))
        };
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.08, y: row_y, width: 0.68, height: 0.065 },
            color: left_bg,
        });
        push_select_title_text(&text, &mut commands, snapshot_row, row_y, selected);
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.78, y: row_y, width: 0.14, height: 0.065 },
            color: right_bg,
        });
        push_select_score_text(&text, &mut commands, snapshot_row, row_y, selected);
    }
    text.push_text(
        &mut commands,
        &snapshot.key_hint,
        BitmapTextStyle { x: 0.08, y: 0.86, cell: 0.006, color: Color::rgb(0.88, 0.9, 0.86) },
    );
    text.push_text(
        &mut commands,
        &snapshot.option_hint,
        BitmapTextStyle { x: 0.08, y: 0.895, cell: 0.005, color: Color::rgb(0.58, 0.67, 0.7) },
    );

    push_exit_hold_indicator(&mut commands, snapshot.exit_hold_progress);
    push_scene_overlays(&mut commands, &snapshot.overlay);

    DrawPlan { clear: Color::rgb(0.02, 0.025, 0.03), commands }
}

fn push_select_option_panel(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &SelectSnapshot,
) {
    if snapshot.option_panel == 0 {
        return;
    }

    let (title, lines): (&str, Vec<String>) = match snapshot.option_panel {
        1 => (
            "PLAY OPTIONS",
            vec![
                format!("ARRANGE  {}", snapshot.arrange),
                format!("TARGET   {}", snapshot.target),
                format!("GAUGE    {}", snapshot.gauge),
                "REPLAY   1 / 2 / 3 / 4".to_string(),
            ],
        ),
        2 => ("ASSIST OPTIONS", vec![format!("ASSIST   {}", snapshot.assist)]),
        3 => (
            "DETAIL OPTIONS",
            vec![
                format!("GAS      {}", snapshot.gauge_auto_shift),
                format!("GAUGE    {}", snapshot.gauge),
                format!("BGA      {}", snapshot.bga),
                format!("VISUAL   {} ms", snapshot.judge_timing_offset_ms),
            ],
        ),
        _ => return,
    };

    let alpha = (snapshot.option_panel_time.0 as f32 / 120_000.0).clamp(0.0, 1.0);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.57, y: 0.225, width: 0.33, height: 0.12 + lines.len() as f32 * 0.035 },
        color: Color::rgba(0.02, 0.026, 0.032, 0.84 * alpha),
    });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.57, y: 0.225, width: 0.33, height: 0.028 },
        color: Color::rgba(0.11, 0.16, 0.19, 0.9 * alpha),
    });
    text.push_text(
        commands,
        title,
        BitmapTextStyle {
            x: 0.585,
            y: 0.232,
            cell: 0.0048,
            color: Color::rgba(0.74, 0.9, 0.96, alpha),
        },
    );
    for (index, line) in lines.iter().enumerate() {
        let selected = snapshot.option_panel == 3 && line.starts_with("GAS");
        text.push_text(
            commands,
            line,
            BitmapTextStyle {
                x: 0.595,
                y: 0.275 + index as f32 * 0.035,
                cell: 0.005,
                color: if selected {
                    Color::rgba(0.96, 0.9, 0.48, alpha)
                } else {
                    Color::rgba(0.78, 0.86, 0.88, alpha)
                },
            },
        );
    }
}

fn select_snapshot_selected_row_position(rows: &[SelectRowSnapshot], selected_index: u32) -> usize {
    let center = rows.len() / 2;
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.index == selected_index)
        .min_by_key(|(index, _)| index.abs_diff(center))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn push_select_title_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    row: Option<&SelectRowSnapshot>,
    row_y: f32,
    selected: bool,
) {
    let is_folder = row.map(|r| r.is_folder).unwrap_or(false);
    let in_library = row.map(|r| r.in_library).unwrap_or(true);
    let title = display_title(row.map(|row| row.title.as_str()).unwrap_or_default());
    let color = if is_folder {
        if selected { Color::rgb(0.98, 0.88, 0.55) } else { Color::rgb(0.62, 0.54, 0.26) }
    } else if !in_library {
        if selected { Color::rgb(0.55, 0.55, 0.55) } else { Color::rgb(0.38, 0.38, 0.40) }
    } else if selected {
        Color::rgb(0.9, 0.96, 0.98)
    } else {
        Color::rgb(0.58, 0.66, 0.68)
    };
    text.push_text(
        commands,
        &title,
        BitmapTextStyle {
            x: 0.1,
            y: row_y + if selected { 0.016 } else { 0.022 },
            cell: if selected { 0.006 } else { 0.005 },
            color,
        },
    );

    if is_folder {
        return;
    }
    let Some(row) = row else {
        return;
    };
    if selected && !row.artist.is_empty() {
        text.push_text(
            commands,
            &display_label(&row.artist, 30),
            BitmapTextStyle {
                x: 0.1,
                y: row_y + 0.041,
                cell: 0.0032,
                color: Color::rgb(0.58, 0.71, 0.73),
            },
        );
    }
    if selected {
        let metadata =
            difficulty_level_label(&row.difficulty_name, &row.play_level, &row.table_level);
        if !metadata.is_empty() {
            text.push_text(
                commands,
                &metadata,
                BitmapTextStyle {
                    x: 0.1,
                    y: row_y + 0.053,
                    cell: 0.0032,
                    color: Color::rgb(0.7, 0.78, 0.7),
                },
            );
        }
    }
}

fn push_select_score_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    row: Option<&SelectRowSnapshot>,
    row_y: f32,
    selected: bool,
) {
    if row.map(|r| r.is_folder).unwrap_or(false) {
        text.push_text(
            commands,
            ">",
            BitmapTextStyle {
                x: 0.838,
                y: row_y + 0.016,
                cell: 0.010,
                color: if selected {
                    Color::rgb(0.98, 0.85, 0.45)
                } else {
                    Color::rgb(0.52, 0.43, 0.18)
                },
            },
        );
        return;
    }

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
    if row.kind == SelectRowKind::Config {
        return row.play_level.clone();
    }
    if !row.in_library {
        return "NOT OWNED".to_string();
    }
    let clear_type = clear_type_label(&row.clear_type);
    if !clear_type.is_empty() {
        clear_type.to_string()
    } else if !row.table_level.is_empty() {
        row.table_level.clone()
    } else if !row.play_level.is_empty() {
        format!("LV {}", display_label(&row.play_level, 4))
    } else {
        "READY".to_string()
    }
}

fn difficulty_level_label(difficulty_name: &str, play_level: &str, table_level: &str) -> String {
    let difficulty = display_label(difficulty_name, 12);
    let level_source = if !table_level.is_empty() { table_level } else { play_level };
    let level = display_label(level_source, 8);
    match (difficulty.is_empty(), level.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("DIFFICULTY {difficulty}"),
        (true, false) => format!("LEVEL {level}"),
        (false, false) => format!("DIFFICULTY {difficulty}  LEVEL {level}"),
    }
}

fn skin_level_number(label: &str) -> i64 {
    label.chars().filter(|ch| ch.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn skin_difficulty_code(label: &str) -> i64 {
    match label.trim().to_ascii_uppercase().as_str() {
        "1" | "BEGINNER" => 1,
        "2" | "NORMAL" => 2,
        "3" | "HYPER" => 3,
        "4" | "ANOTHER" => 4,
        "5" | "INSANE" => 5,
        _ => 0,
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

fn plan_play(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    let mut commands = Vec::new();
    if snapshot.backbmp_background {
        push_fullscreen_image(&mut commands, PLAY_BACKBMP_TEXTURE);
    }
    let text = TextRenderer;
    let skin_manifest = skin.manifest();
    let has_document = skin.document().is_some();
    if !has_document {
        push_fallback_bga_background(&mut commands, snapshot);
    }
    let key_mode = snapshot.key_mode;
    let active_lanes = key_mode.active_lanes();
    let active_lane_count = active_lanes.len();
    let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
    let lane_width = board.width / active_lane_count as f32;

    // 見逃しPOOR（is_miss）はボムエフェクトを出さない
    let mut bomb_ms: [Option<i32>; LANE_COUNT] = [None; LANE_COUNT];
    let mut lane_judge: [Option<usize>; LANE_COUNT] = [None; LANE_COUNT];
    for j in &snapshot.recent_judgements {
        if j.is_miss {
            continue;
        }
        let idx = j.lane.index();
        let elapsed =
            ((snapshot.time.0 - j.time.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        bomb_ms[idx] = Some(elapsed);
        lane_judge[idx] = judge_image_index(&j.text);
    }

    // keyon/keyoff のタイマー値は session 側で per-lane に追跡された keyon/keyoff
    // 開始時刻から算出済み。snapshot.recent_inputs から再構築しない。
    let keyon_ms = snapshot.keyon_ms;
    let keyoff_ms = snapshot.keyoff_ms;

    let judge_region_count = skin.document().map(|d| d.judge_region_count()).unwrap_or(1);
    let judge_region_state = crate::skin::build_judge_region_state(
        &snapshot.recent_judgements,
        snapshot.time.0,
        judge_region_count,
    );
    let judge_timing_ms = snapshot
        .recent_judgements
        .last()
        .map(|j| (j.delta_us / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32);

    let play_elapsed_ms =
        (snapshot.play_elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let ready_timer_ms = snapshot
        .ready_elapsed_time
        .map(|time| (time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    let skin_load_delay_ms = skin
        .document()
        .map(|document| document.loadstart.max(0).saturating_add(document.loadend.max(0)))
        .unwrap_or(0);

    let skin_state = crate::skin::SkinDrawState {
        elapsed_ms: play_elapsed_ms,
        ready_timer_ms,
        play_timer_ms: (snapshot.time.0 >= 0)
            .then_some((snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        key_mode,
        select_arrange_index: crate::skin::select_arrange_index(&snapshot.arrange),
        combo: snapshot.combo,
        max_combo: snapshot.max_combo,
        ex_score: snapshot.ex_score,
        total_notes: snapshot.total_notes,
        past_notes: snapshot.past_notes,
        judge_counts: snapshot.judge_counts,
        fast_slow_counts: Some(snapshot.fast_slow_counts),
        gauge: snapshot.gauge,
        gauge_type: snapshot.gauge_type,
        gauge_auto_shift: snapshot.gauge_auto_shift,
        gauge_max: snapshot.gauge_max,
        gauge_border: snapshot.gauge_border,
        play_progress: play_progress(snapshot),
        end_of_note: end_of_note(snapshot),
        end_of_note_ms: snapshot.end_of_note_elapsed_ms,
        bomb_ms,
        keyon_ms,
        keyoff_ms,
        hold_ms: snapshot.hold_ms,
        hcn_active_ms: snapshot.hcn_active_ms,
        hcn_damage_ms: snapshot.hcn_damage_ms,
        lane_judge,
        judge_ms: judge_region_state.judge_ms,
        full_combo_ms: snapshot.full_combo_elapsed_ms,
        fadeout_ms: snapshot.fadeout_elapsed_ms,
        failed_ms: snapshot.failed_elapsed_ms,
        music_end_ms: snapshot.music_end_elapsed_ms,
        judge_index: judge_region_state.judge_index,
        judge_combo: judge_region_state.judge_combo,
        judge_timing_sign: judge_region_state.judge_timing_sign,
        offset_lift_px: {
            let canvas_h = skin.document().map_or(720, |d| d.h) as f32;
            (snapshot.lift * canvas_h).round() as i32
        },
        offset_lanecover_px: {
            let canvas_h = skin.document().map_or(720, |d| d.h) as f32;
            let lane_h = skin_lane_height_px(skin, canvas_h);
            ((snapshot.lift - 1.0) * lane_h * snapshot.lane_cover).round() as i32
        },
        offset_hidden_cover_px: {
            let canvas_h = skin.document().map_or(720, |d| d.h) as f32;
            let lane_h = skin_lane_height_px(skin, canvas_h);
            ((1.0 - snapshot.lift) * snapshot.hidden_cover * lane_h).round() as i32
        },
        skin_offsets: snapshot.skin_offsets,
        hispeed: snapshot.hispeed,
        timeleft_ms: (snapshot.duration.0.saturating_sub(snapshot.time.0) / 1_000)
            .saturating_add(1_000)
            .clamp(0, i32::MAX as i64) as i32,
        total_duration_ms: snapshot.note_display_duration_ms,
        lane_cover: snapshot.lane_cover,
        lift: snapshot.lift,
        lane_cover_changing: snapshot.lane_cover_changing,
        lanecover_enabled: snapshot.lanecover_enabled,
        lift_enabled: snapshot.lift_enabled,
        hidden_enabled: snapshot.hidden_enabled,
        hidden_cover: snapshot.hidden_cover,
        play_level: skin_level_number(&snapshot.play_level),
        difficulty: skin_difficulty_code(&snapshot.difficulty_name),
        judge_rank: snapshot.judge_rank,
        now_bpm: snapshot.now_bpm,
        min_bpm: snapshot.min_bpm,
        max_bpm: snapshot.max_bpm,
        has_bga: snapshot.has_bga,
        has_bpm_stop: snapshot.has_bpm_stop,
        bga_enabled: snapshot.bga_enabled,
        has_backbmp: snapshot.backbmp_background,
        bga_base: snapshot.bga_base.map(skin_bga_frame_from_display),
        bga_layer: snapshot.bga_layer.map(skin_bga_frame_from_display),
        bga_layer2: snapshot.bga_layer2.map(skin_bga_frame_from_display),
        bga_poor: snapshot.bga_poor.map(skin_bga_frame_from_display),
        bga_stretch: snapshot.bga_stretch,
        judge_timing_ms,
        best_ex_score: snapshot.best_ex_score,
        projected_best_ex_score: snapshot.projected_best_ex_score,
        target_ex_score: snapshot.target_ex_score,
        judge_timing_offset_ms: snapshot.judge_timing_offset_ms,
        main_bpm: snapshot.main_bpm,
        hsfix_index: snapshot.hsfix_index,
        fs_threshold_ms: snapshot.fs_threshold_ms,
        adjusted_cover_progress: snapshot.adjusted_cover_progress,
        adjusted_rate: snapshot.adjusted_rate,
        adjusted_rate_adot: snapshot.adjusted_rate_adot,
        autoplay: snapshot.autoplay,
        course_stage: snapshot.course_stage,
        hit_error_ring: snapshot.hit_error_ring.values,
        hit_error_ring_index: snapshot.hit_error_ring.index,
        skin_loaded: play_elapsed_ms >= skin_load_delay_ms,
        ..crate::skin::SkinDrawState::default()
    };
    let play_skin = skin.with_play_graphs(
        snapshot.judge_graph_density.clone(),
        snapshot.bpm_graph_segments.clone(),
    );
    let skin_state = advance_skin_dynamic_timers(skin, dynamic_timers, skin_state, play_elapsed_ms);
    let skin_text = SkinTextState {
        title: &snapshot.title,
        subtitle: &snapshot.subtitle,
        artist: &snapshot.artist,
        subartist: &snapshot.subartist,
        genre: &snapshot.genre,
        difficulty_name: &snapshot.difficulty_name,
        play_level: &snapshot.play_level,
        table_level: &snapshot.table_text_secondary,
        table_text_primary: &snapshot.table_text_primary,
        table_text_secondary: &snapshot.table_text_secondary,
        table_text_fallback: &snapshot.table_text_fallback,
        course_stage: snapshot.course_stage,
        course_titles: string_array_refs(&snapshot.course_titles),
        ..SkinTextState::default()
    };
    // `{"id":"notes"}` マーカーと `timer: 3` (FAILED) で3分割。
    // 描画順: 背面skin → ロング/ノーツ → 前面skin → 暗転/閉店オーバーレイ
    let (behind_notes_items, front_notes_items, failed_overlay_items) =
        play_skin.static_document_items_split_for_state_and_text(skin_state, skin_text);
    let behind_notes_items = skin.apply_play_skin_global_offset(behind_notes_items, skin_state);
    append_skin_render_items(&mut commands, &behind_notes_items);

    if !has_document {
        // デフォルトスキン: ボード背景・レーン背景を描画
        commands.push(DrawCommand::Rect { rect: board, color: Color::rgb(0.025, 0.025, 0.028) });
        commands.push(DrawCommand::Rect {
            rect: Rect { x: board.x - 0.006, y: board.y, width: 0.006, height: board.height },
            color: Color::rgb(0.18, 0.2, 0.21),
        });
        commands.push(DrawCommand::Rect {
            rect: Rect { x: board.x + board.width, y: board.y, width: 0.006, height: board.height },
            color: Color::rgb(0.18, 0.2, 0.21),
        });

        for (display_index, &lane) in active_lanes.iter().enumerate() {
            let lane_index = lane.index();
            let x = board.x + display_index as f32 * lane_width;
            let color = if display_index % 2 == 0 {
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

            // ロングノート胴体はタップノートより先に描画する（端のキャップを上に重ねる）
            for body in snapshot.visible_long_notes.iter().filter(|body| body.lane == lane) {
                let top = play_object_y(board, snapshot.lift, body.tail_y);
                let bottom = play_object_y(board, snapshot.lift, body.head_y);
                commands.push(DrawCommand::Rect {
                    rect: Rect {
                        x: x + lane_width * 0.18,
                        y: top,
                        width: lane_width * 0.64,
                        height: (bottom - top).max(0.0),
                    },
                    color: long_note_body_color(body.mode),
                });
                // beatoraja の drawLongNote 同様、キャップは胴体側で描画する。
                // head キャップは押下中も判定ライン (head_y=0) に留まる。
                // LN モードは head キャップのみ、CN/HCN は tail キャップも描画する。
                // show_ln_tail_cap 有効時は LN モードでも tail キャップを描画する。
                let head_rect = Rect {
                    x: x + lane_width * 0.08,
                    y: note_rect_y(board, snapshot.lift, body.head_y),
                    width: lane_width * 0.84,
                    height: NOTE_HEIGHT,
                };
                push_ln_start_skin(skin_manifest, &mut commands, lane, head_rect);
                if (body.mode != LongNoteMode::Ln || snapshot.show_ln_tail_cap) && body.tail_y < 1.0
                {
                    let tail_rect = Rect {
                        x: x + lane_width * 0.08,
                        y: note_rect_y(board, snapshot.lift, body.tail_y),
                        width: lane_width * 0.84,
                        height: NOTE_HEIGHT,
                    };
                    push_ln_end_skin(skin_manifest, &mut commands, lane, tail_rect);
                }
            }

            for note in &snapshot.visible_notes[lane_index] {
                let y = note_rect_y(board, snapshot.lift, note.y);
                let rect = Rect {
                    x: x + lane_width * 0.08,
                    y,
                    width: lane_width * 0.84,
                    height: NOTE_HEIGHT,
                };
                match note.kind {
                    NoteVisualKind::LnStart => {
                        push_ln_start_skin(skin_manifest, &mut commands, lane, rect)
                    }
                    NoteVisualKind::LnEnd => {
                        push_ln_end_skin(skin_manifest, &mut commands, lane, rect)
                    }
                    NoteVisualKind::Tap => {
                        push_default_note_skin(skin_manifest, &mut commands, lane, rect)
                    }
                }
            }

            // Mine: 通常ノーツより前面に「警告ストライプ」テクスチャを重ねる。
            // 全レーン共通の DEFAULT_MINE_NOTE_TEXTURE を使い、レーン色付けは行わない。
            for mine in &snapshot.visible_mines[lane_index] {
                let y = note_rect_y(board, snapshot.lift, mine.y);
                let rect = Rect {
                    x: x + lane_width * 0.08,
                    y,
                    width: lane_width * 0.84,
                    height: NOTE_HEIGHT,
                };
                commands.push(DrawCommand::Image {
                    rect,
                    uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    texture: DEFAULT_MINE_NOTE_TEXTURE,
                    tint: Color::rgba(1.0, 1.0, 1.0, 1.0),
                    blend: BlendMode::Normal,
                    linear_filter: false,
                });
            }
        }

        push_receptors(
            skin_manifest,
            &mut commands,
            board,
            snapshot.lift,
            lane_width,
            active_lanes,
        );
        for bar in &snapshot.bar_lines {
            push_play_bar_line(
                &mut commands,
                skin,
                skin_state,
                board,
                snapshot.lift,
                bar,
                snapshot.skin_offsets,
            );
        }
        push_judge_line(skin_manifest, &mut commands, board, snapshot.lift);

        // SUDDEN+（レーンカバー）: レーン上部を覆う。ノーツは build_render_snapshot で
        // 既に可視域外が除外されているので、ここではカバー帯を描くだけ。
        // レーン背景が暗いグレーなので、カバーは判別しやすいように明確に黒で塗り、
        // 下端に視認用のハイライト帯を付ける。
        if snapshot.lane_cover > 0.0 {
            let cover_bottom =
                play_object_y(board, snapshot.lift, (1.0 - snapshot.lane_cover).clamp(0.0, 1.0));
            let cover_height = (cover_bottom - board.y).max(0.0);
            commands.push(DrawCommand::Rect {
                rect: Rect { x: board.x, y: board.y, width: board.width, height: cover_height },
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            });
            // SUDDEN+ の下端ラインを描いて境界を視認できるようにする。
            let line_height = 0.004_f32.min(cover_height);
            if line_height > 0.0 {
                commands.push(DrawCommand::Rect {
                    rect: Rect {
                        x: board.x,
                        y: cover_bottom - line_height,
                        width: board.width,
                        height: line_height,
                    },
                    color: Color::rgb(0.95, 0.65, 0.25),
                });
            }
        }
    } else {
        // beatoraja スキン: ロングノート胴体 → タップノートの順で note.dst のエリアに配置
        for bar in &snapshot.bar_lines {
            push_play_bar_line(
                &mut commands,
                skin,
                skin_state,
                board,
                snapshot.lift,
                bar,
                snapshot.skin_offsets,
            );
        }
        for body in &snapshot.visible_long_notes {
            if let Some(rect) =
                skin.note_body_rect(body.lane, key_mode, body.head_y, body.tail_y, skin_state)
                && let Some(item) = skin.document_long_body_item(
                    body.lane,
                    key_mode,
                    rect,
                    body.mode,
                    body.body_state,
                )
            {
                let item = skin.apply_play_skin_global_offset_to_item(item, skin_state);
                append_skin_render_items(&mut commands, &[item]);
            }
            // beatoraja の drawLongNote 同様、キャップは胴体の上に重ねて描画する。
            // head キャップは押下中も判定ライン (head_y=0) に留まり描画され続ける。
            // LN モードは head キャップのみ、CN/HCN は tail キャップも描画する。
            // show_ln_tail_cap 有効時は LN モードでも tail キャップを描画する。
            let note_height = skin.document_note_height(body.lane, key_mode).unwrap_or(NOTE_HEIGHT);
            if let Some(rect) = skin.note_rect_for_progress(
                body.lane,
                key_mode,
                body.head_y,
                note_height,
                skin_state,
            ) && let Some(item) =
                skin.document_ln_start_item(body.lane, key_mode, rect, body.mode)
            {
                let item = skin.apply_play_skin_global_offset_to_item(item, skin_state);
                append_skin_render_items(&mut commands, &[item]);
            }
            if (body.mode != LongNoteMode::Ln || snapshot.show_ln_tail_cap)
                && body.tail_y < 1.0
                && let Some(rect) = skin.note_rect_for_progress(
                    body.lane,
                    key_mode,
                    body.tail_y,
                    note_height,
                    skin_state,
                )
                && let Some(item) = skin.document_ln_end_item(body.lane, key_mode, rect, body.mode)
            {
                let item = skin.apply_play_skin_global_offset_to_item(item, skin_state);
                append_skin_render_items(&mut commands, &[item]);
            }
        }
        for &lane in active_lanes {
            let lane_index = lane.index();
            let note_height = skin.document_note_height(lane, key_mode).unwrap_or(NOTE_HEIGHT);
            for note in &snapshot.visible_notes[lane_index] {
                if let Some(rect) =
                    skin.note_rect_for_progress(lane, key_mode, note.y, note_height, skin_state)
                {
                    let item = match note.kind {
                        NoteVisualKind::LnStart => {
                            skin.document_ln_start_item(lane, key_mode, rect, LongNoteMode::Ln)
                        }
                        NoteVisualKind::LnEnd => {
                            skin.document_ln_end_item(lane, key_mode, rect, LongNoteMode::Ln)
                        }
                        NoteVisualKind::Tap => skin.document_note_item(lane, key_mode, rect),
                    };
                    if let Some(item) = item {
                        let item = skin.apply_play_skin_global_offset_to_item(item, skin_state);
                        append_skin_render_items(&mut commands, &[item]);
                    }
                }
            }
            // Mine ノーツ: スキン側に `note.mine` が定義されていればそれを使い、
            // 無ければ DEFAULT_MINE_NOTE_TEXTURE をフォールバックとして重ねる。
            for mine in &snapshot.visible_mines[lane_index] {
                if let Some(rect) =
                    skin.note_rect_for_progress(lane, key_mode, mine.y, note_height, skin_state)
                {
                    if let Some(item) = skin.document_mine_item(lane, key_mode, rect) {
                        let item = skin.apply_play_skin_global_offset_to_item(item, skin_state);
                        append_skin_render_items(&mut commands, &[item]);
                    } else {
                        commands.push(DrawCommand::Image {
                            rect,
                            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                            texture: DEFAULT_MINE_NOTE_TEXTURE,
                            tint: Color::rgba(1.0, 1.0, 1.0, 1.0),
                            blend: BlendMode::Normal,
                            linear_filter: false,
                        });
                    }
                }
            }
        }
    }

    // ノーツより前面の skin 要素（レーンカバー・枠・スコア等）をノーツの上に重ねる
    let front_notes_items = skin.apply_play_skin_global_offset(front_notes_items, skin_state);
    append_skin_render_items(&mut commands, &front_notes_items);

    // 閉店の暗転 (`black` の a:0→255) 等、timer:3 を最前面に描画
    let failed_overlay_items = skin.apply_play_skin_global_offset(failed_overlay_items, skin_state);
    append_skin_render_items(&mut commands, &failed_overlay_items);

    if !has_document {
        push_combo_panel(skin_manifest, &mut commands, snapshot.combo);
        push_default_play_skin(skin, &mut commands, snapshot);
        push_play_text(&text, &mut commands, snapshot);
        push_lane_text(&text, &mut commands, board, lane_width, active_lanes);
        push_judgement_history(&text, &mut commands, snapshot);
        // READY/GO オーバーレイはデフォルトスキン専用。
        // JSON skin 等は skin 側の演出を使うため描画しない。
        push_start_overlay(&text, &mut commands, snapshot);
        push_default_failed_overlay(&text, &mut commands, snapshot);
    }
    push_chart_text(&text, &mut commands, snapshot);
    push_scene_overlays(&mut commands, &snapshot.overlay);

    DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands }
}

fn push_chart_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    if snapshot.chart_text.is_empty() {
        return;
    }
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.18, y: 0.04, width: 0.64, height: 0.06 },
        color: Color::rgba(0.0, 0.0, 0.0, 0.55),
    });
    text.push_text(
        commands,
        &snapshot.chart_text,
        BitmapTextStyle { x: 0.2, y: 0.055, cell: 0.006, color: Color::rgb(0.95, 0.95, 0.9) },
    );
}

fn plan_decide(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    if skin.document().is_some_and(|document| document.skin_type == 6) {
        let play_elapsed_ms =
            (snapshot.play_elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let state = crate::skin::SkinDrawState {
            elapsed_ms: play_elapsed_ms,
            ready_timer_ms: Some(play_elapsed_ms),
            total_notes: snapshot.total_notes,
            past_notes: snapshot.past_notes,
            ex_score: snapshot.ex_score,
            judge_counts: snapshot.judge_counts,
            fast_slow_counts: Some(snapshot.fast_slow_counts),
            gauge: snapshot.gauge,
            gauge_type: snapshot.gauge_type,
            gauge_auto_shift: snapshot.gauge_auto_shift,
            gauge_max: snapshot.gauge_max,
            gauge_border: snapshot.gauge_border,
            play_level: skin_level_number(&snapshot.play_level),
            difficulty: skin_difficulty_code(&snapshot.difficulty_name),
            judge_rank: snapshot.judge_rank,
            now_bpm: snapshot.now_bpm,
            min_bpm: snapshot.min_bpm,
            max_bpm: snapshot.max_bpm,
            has_bga: snapshot.has_bga,
            has_bpm_stop: snapshot.has_bpm_stop,
            bga_enabled: snapshot.bga_enabled,
            skin_offsets: snapshot.skin_offsets,
            hispeed: snapshot.hispeed,
            total_duration_ms: snapshot.note_display_duration_ms,
            lane_cover: snapshot.lane_cover,
            hidden_cover: snapshot.hidden_cover,
            fadeout_ms: snapshot.fadeout_elapsed_ms,
            ..crate::skin::SkinDrawState::default()
        };
        let state = advance_skin_dynamic_timers(skin, dynamic_timers, state, play_elapsed_ms);
        let text = SkinTextState {
            title: &snapshot.title,
            subtitle: &snapshot.subtitle,
            artist: &snapshot.artist,
            subartist: &snapshot.subartist,
            genre: &snapshot.genre,
            difficulty_name: &snapshot.difficulty_name,
            play_level: &snapshot.play_level,
            course_titles: string_array_refs(&snapshot.course_titles),
            ..SkinTextState::default()
        };
        let items = skin.static_document_items_for_state_and_text(state, text);
        if !items.is_empty() {
            let mut commands = Vec::new();
            crate::skin::append_skin_render_items(&mut commands, &items);
            push_scene_overlays(&mut commands, &snapshot.overlay);
            return DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
        }
    }

    plan_play(snapshot, &SkinContext::default(), dynamic_timers)
}

fn skin_lane_height_px(skin: &SkinContext, fallback_canvas_h: f32) -> f32 {
    skin.document()
        .and_then(|document| {
            let enabled_options = document.enabled_options();
            // Key1 はすべてのキーモードでインデックス 0 なので K7 で代用。
            document.note_lane_area(Lane::Key1, bmz_core::lane::KeyMode::K7, &enabled_options)
        })
        .map_or(fallback_canvas_h, |rect| rect.height * fallback_canvas_h)
}

fn plan_result(
    snapshot: &crate::scene::ResultSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    if let Some(document) = skin.document().filter(|document| matches!(document.skin_type, 7 | 15))
    {
        let state = advance_skin_dynamic_timers(
            skin,
            dynamic_timers,
            build_result_skin_draw_state(snapshot, document.ranktime),
            (snapshot.elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        );
        let grade_diff = crate::skin::result_grade_diff_label(state).unwrap_or_default();
        let text = SkinTextState {
            title: snapshot.title.as_str(),
            subtitle: snapshot.subtitle.as_str(),
            artist: snapshot.artist.as_str(),
            subartist: snapshot.subartist.as_str(),
            genre: snapshot.genre.as_str(),
            difficulty_name: snapshot.difficulty_name.as_str(),
            play_level: snapshot.play_level.as_str(),
            grade_diff: grade_diff.as_str(),
            table_level: snapshot.play_level.as_str(),
            ..SkinTextState::default()
        };
        let items =
            skin.static_document_items_for_result_state_and_text(&snapshot.graph, state, text);
        if !items.is_empty() {
            let mut commands = Vec::with_capacity(items.len() + 3);
            crate::skin::append_skin_render_items(&mut commands, &items);
            push_scene_overlays(&mut commands, &snapshot.overlay);
            return DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
        }
    }

    let mut plan = plan_result_fallback(ResultFallbackSummary {
        clear_type: snapshot.clear_type.as_str(),
        ex_score: snapshot.ex_score,
        ex_score_rate: snapshot.ex_score_rate,
        max_combo: snapshot.max_combo,
        gauge_value: snapshot.gauge_value,
        total_notes: snapshot.total_notes,
        judge_counts: &snapshot.judge_counts,
        fast_slow_counts: &snapshot.fast_slow_counts,
        graph: &snapshot.graph,
        score_history_id: snapshot.score_history_id,
        replay_saved: snapshot.replay_saved,
        difficulty_name: &snapshot.difficulty_name,
        play_level: &snapshot.play_level,
        grade_diff: crate::skin::result_grade_diff_label(build_result_skin_draw_state(snapshot, 0))
            .unwrap_or_default(),
    });
    push_scene_overlays(&mut plan.commands, &snapshot.overlay);
    plan
}

fn push_scene_overlays(
    commands: &mut Vec<DrawCommand>,
    overlay: &crate::snapshot::OverlaySnapshot,
) {
    push_scene_overlay_text_aligned(commands, &overlay.left_text, 0.015, TextAlign::Left);
    push_scene_overlay_text(commands, &overlay.fps_text, 0.015);
    push_scene_overlay_text(commands, &overlay.text, 0.975);
}

fn push_scene_overlay_text(commands: &mut Vec<DrawCommand>, overlay: &str, origin_y: f32) {
    push_scene_overlay_text_aligned(commands, overlay, origin_y, TextAlign::Right);
}

fn push_scene_overlay_text_aligned(
    commands: &mut Vec<DrawCommand>,
    overlay: &str,
    origin_y: f32,
    align: TextAlign,
) {
    if overlay.is_empty() {
        return;
    }
    // TextStyle.size は「画面高に対する比率」で扱われる (renderer.rs 側で * surface.height)。
    // ここでは 1080p を基準に 14px 相当へ合わせる。
    const OVERLAY_FONT_SIZE_RATIO: f32 = 14.0 / 1080.0;
    const OVERLAY_SHADOW_OFFSET_RATIO: f32 = 1.0 / 1080.0;
    // TextAlign::Right は max_width > 0 のときだけ効く (renderer.rs)。
    // origin.x を右端ボックスの左端、max_width をボックス幅にして右寄せする。
    let origin_x = if align == TextAlign::Left { 0.015 } else { -0.015 };
    commands.push(DrawCommand::Text {
        origin: Point { x: origin_x, y: origin_y },
        text: overlay.to_string(),
        style: TextStyle {
            font_id: None,
            size: OVERLAY_FONT_SIZE_RATIO,
            bitmap_size: None,
            color: Color::rgba(0.9, 0.9, 0.9, 0.65),
            layer: TextLayer::Ui,
            align,
            max_width: 1.0,
            overflow: TextOverflow::Overflow,
            wrapping: false,
            outline: None,
            shadow: Some(TextShadow {
                color: Color::rgba(0.0, 0.0, 0.0, 0.55),
                offset: Point { x: OVERLAY_SHADOW_OFFSET_RATIO, y: OVERLAY_SHADOW_OFFSET_RATIO },
            }),
        },
    });
}

/// リザルト画面の `SkinDrawState` を snapshot から構築する。
///
/// op 条件評価 (ランク別 BG など) を描画前に行いたい呼び出し側 (例: 動画ソースの
/// 可視判定) が、描画と同じ state を得るための公開エントリ。
pub fn result_skin_draw_state(
    snapshot: &crate::scene::ResultSnapshot,
    result_ranktime_ms: i32,
) -> crate::skin::SkinDrawState {
    build_result_skin_draw_state(snapshot, result_ranktime_ms)
}

fn build_result_skin_draw_state(
    snapshot: &crate::scene::ResultSnapshot,
    result_ranktime_ms: i32,
) -> crate::skin::SkinDrawState {
    use bmz_core::clear::ClearType;
    let result_failed = matches!(snapshot.clear_type, ClearType::Failed | ClearType::NoPlay);
    let timing_stats = snapshot
        .graph
        .timing_distribution
        .stats()
        .or_else(|| result_timing_stats(&snapshot.graph.timing_points));
    let elapsed_ms =
        (snapshot.elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let result_update_score_ms = if result_ranktime_ms <= 0 {
        Some(elapsed_ms)
    } else {
        elapsed_ms
            .checked_sub(result_ranktime_ms)
            .filter(|elapsed_after_rank| *elapsed_after_rank >= 0)
    };
    crate::skin::SkinDrawState {
        elapsed_ms,
        select_arrange_index: crate::skin::select_arrange_index(&snapshot.arrange),
        result_arrange_index: crate::skin::select_arrange_index(&snapshot.arrange),
        result_random_lane_refs: crate::skin::result_random_lane_refs(
            &snapshot.lane_shuffle_pattern,
            snapshot.key_mode,
        ),
        ex_score: snapshot.ex_score,
        total_notes: snapshot.total_notes,
        past_notes: snapshot.total_notes,
        result_grade_diff_display: snapshot.grade_diff_display,
        total_duration_ms: snapshot.duration_ms,
        max_combo: snapshot.max_combo,
        judge_counts: snapshot.judge_counts,
        fast_slow_counts: Some(snapshot.fast_slow_counts),
        gauge: snapshot.gauge_value,
        gauge_type: snapshot.gauge_type,
        result_gauge_graph_type: Some(snapshot.result_gauge_graph_type),
        gauge_max: 100.0,
        gauge_border: 80.0,
        play_progress: 1.0,
        end_of_note: true,
        best_ex_score: snapshot.best_ex_score,
        best_clear_index: snapshot.best_clear_type.map(|c| c as i64),
        target_ex_score: snapshot.target_ex_score,
        best_max_combo: snapshot.best_max_combo,
        target_max_combo: snapshot.target_max_combo,
        best_bp: snapshot.best_bp,
        result_bp: Some(snapshot.bp),
        result_cb: Some(snapshot.cb),
        previous_best_ex_score: snapshot.previous_best_ex_score,
        previous_best_max_combo: snapshot.previous_best_max_combo,
        previous_best_bp: snapshot.previous_best_bp,
        target_bp: snapshot.target_bp,
        target_clear_index: snapshot.target_clear_type.map(|c| c as i64),
        select_clear_index: snapshot.clear_type as i64,
        result_failed: Some(result_failed),
        play_level: skin_level_number(&snapshot.play_level),
        difficulty: skin_difficulty_code(&snapshot.difficulty_name),
        judge_rank: snapshot.judge_rank,
        key_mode: snapshot.key_mode,
        now_bpm: snapshot.initial_bpm,
        min_bpm: snapshot.min_bpm,
        max_bpm: snapshot.max_bpm,
        main_bpm: snapshot.main_bpm,
        select_chart_total_gauge: snapshot.total_gauge,
        fadeout_ms: snapshot
            .fadeout_elapsed
            .map(|elapsed| (elapsed.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        result_graph_begin_ms: Some(elapsed_ms),
        result_graph_end_ms: Some(elapsed_ms),
        result_update_score_ms,
        result_replay_slots: snapshot.replay_slots,
        result_saved_replay_slots: snapshot.saved_replay_slots,
        hit_error_ring: snapshot.graph.hit_error_ring.values,
        hit_error_ring_index: snapshot.graph.hit_error_ring.index,
        average_timing_ms: timing_stats.map(|stats| stats.0),
        stddev_timing_ms: timing_stats.map(|stats| stats.1),
        ..crate::skin::SkinDrawState::default()
    }
}

fn result_timing_stats(points: &[crate::snapshot::ResultTimingPoint]) -> Option<(f32, f32)> {
    if points.is_empty() {
        return None;
    }
    let count = points.len() as f32;
    let average_ms =
        points.iter().map(|point| point.delta_us as f32 / 1_000.0).sum::<f32>() / count;
    let variance = points
        .iter()
        .map(|point| {
            let diff = point.delta_us as f32 / 1_000.0 - average_ms;
            diff * diff
        })
        .sum::<f32>()
        / count;
    Some((average_ms, variance.sqrt()))
}

struct ResultFallbackSummary<'a> {
    clear_type: &'a str,
    ex_score: u32,
    ex_score_rate: f32,
    max_combo: u32,
    gauge_value: f32,
    total_notes: u32,
    judge_counts: &'a DisplayJudgeCounts,
    fast_slow_counts: &'a FastSlowJudgeCounts,
    graph: &'a ResultGraphSnapshot,
    score_history_id: i64,
    replay_saved: bool,
    difficulty_name: &'a str,
    play_level: &'a str,
    grade_diff: String,
}

fn plan_result_fallback(summary: ResultFallbackSummary<'_>) -> DrawPlan {
    let ResultFallbackSummary {
        clear_type,
        ex_score,
        ex_score_rate,
        max_combo,
        gauge_value,
        total_notes,
        judge_counts,
        fast_slow_counts,
        graph,
        score_history_id,
        replay_saved,
        difficulty_name,
        play_level,
        grade_diff,
    } = summary;
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
    let metadata = difficulty_level_label(difficulty_name, play_level, "");
    if !metadata.is_empty() {
        text.push_text(
            &mut commands,
            &metadata,
            BitmapTextStyle {
                x: 0.14,
                y: 0.292,
                cell: 0.0055,
                color: Color::rgb(0.72, 0.82, 0.76),
            },
        );
    }
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
        &format!("GRADE {}", grade_diff),
        BitmapTextStyle { x: 0.52, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
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
    push_result_detail_panels(&text, &mut commands, judge_counts, fast_slow_counts, graph);
    text.push_text(
        &mut commands,
        "R RETRY  ENTER/ESC SELECT",
        BitmapTextStyle { x: 0.14, y: 0.925, cell: 0.005, color: Color::rgb(0.74, 0.78, 0.8) },
    );

    DrawPlan { clear: Color::rgb(0.025, 0.02, 0.018), commands }
}

fn push_result_detail_panels(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    judge_counts: &DisplayJudgeCounts,
    fast_slow_counts: &FastSlowJudgeCounts,
    graph: &ResultGraphSnapshot,
) {
    push_result_panel(commands, Rect { x: 0.14, y: 0.715, width: 0.22, height: 0.17 });
    push_result_panel(commands, Rect { x: 0.38, y: 0.715, width: 0.22, height: 0.17 });
    push_result_panel(commands, Rect { x: 0.62, y: 0.715, width: 0.24, height: 0.17 });

    text.push_text(
        commands,
        "JUDGE DETAILS",
        BitmapTextStyle { x: 0.155, y: 0.732, cell: 0.0044, color: Color::rgb(0.86, 0.9, 0.88) },
    );
    text.push_text(
        commands,
        "FAST/SLOW DETAILS",
        BitmapTextStyle { x: 0.395, y: 0.732, cell: 0.0044, color: Color::rgb(0.86, 0.9, 0.88) },
    );
    text.push_text(
        commands,
        "TIMING DETAILS",
        BitmapTextStyle { x: 0.635, y: 0.732, cell: 0.0044, color: Color::rgb(0.86, 0.9, 0.88) },
    );

    push_result_judge_details(text, commands, judge_counts, graph);
    push_result_fast_slow_details(text, commands, fast_slow_counts);
    push_result_timing_details(text, commands, &graph.timing_points);
}

fn push_result_panel(commands: &mut Vec<DrawCommand>, rect: Rect) {
    commands.push(DrawCommand::Rect { rect, color: Color::rgb(0.055, 0.052, 0.05) });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: rect.x, y: rect.y, width: rect.width, height: 0.002 },
        color: Color::rgb(0.36, 0.46, 0.48),
    });
}

fn push_result_judge_details(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    judge_counts: &DisplayJudgeCounts,
    graph: &ResultGraphSnapshot,
) {
    let values = [
        ("PG", judge_counts.pgreat, Color::rgb(0.68, 0.9, 1.0)),
        ("GR", judge_counts.great, Color::rgb(0.76, 0.94, 0.68)),
        ("GD", judge_counts.good, Color::rgb(0.95, 0.86, 0.48)),
        ("BD", judge_counts.bad, Color::rgb(0.96, 0.55, 0.42)),
        ("PR", judge_counts.poor, Color::rgb(0.84, 0.48, 0.58)),
        ("EP", judge_counts.empty_poor, Color::rgb(0.68, 0.58, 0.82)),
    ];
    let max = values.iter().map(|(_, value, _)| *value).max().unwrap_or(0).max(1) as f32;
    for (index, (label, value, color)) in values.iter().enumerate() {
        let y = 0.756 + index as f32 * 0.014;
        text.push_text(
            commands,
            &format!("{label} {value}"),
            BitmapTextStyle { x: 0.155, y, cell: 0.0039, color: Color::rgb(0.78, 0.82, 0.8) },
        );
        let width = 0.105 * (*value as f32 / max);
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.245, y: y + 0.0015, width, height: 0.006 },
            color: *color,
        });
    }
    push_result_density_graph(
        commands,
        Rect { x: 0.245, y: 0.855, width: 0.095, height: 0.018 },
        &graph.judge_graph_density,
    );
}

fn push_result_fast_slow_details(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    counts: &FastSlowJudgeCounts,
) {
    let rows = [
        ("PG", counts.fast_pgreat, counts.slow_pgreat),
        ("GR", counts.fast_great, counts.slow_great),
        ("GD", counts.fast_good, counts.slow_good),
        ("BD", counts.fast_bad, counts.slow_bad),
        ("PR", counts.fast_poor, counts.slow_poor),
        ("EP", counts.fast_empty_poor, counts.slow_empty_poor),
    ];
    let max =
        rows.iter().map(|(_, fast, slow)| fast.saturating_add(*slow)).max().unwrap_or(0).max(1)
            as f32;
    for (index, (label, fast, slow)) in rows.iter().enumerate() {
        let y = 0.756 + index as f32 * 0.014;
        text.push_text(
            commands,
            label,
            BitmapTextStyle { x: 0.395, y, cell: 0.0039, color: Color::rgb(0.78, 0.82, 0.8) },
        );
        let total = fast.saturating_add(*slow) as f32;
        let bar_total_w = 0.122 * (total / max);
        let fast_w = if total <= 0.0 { 0.0 } else { bar_total_w * *fast as f32 / total };
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.425, y: y + 0.0015, width: fast_w, height: 0.006 },
            color: Color::rgb(0.45, 0.86, 0.96),
        });
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: 0.425 + fast_w,
                y: y + 0.0015,
                width: (bar_total_w - fast_w).max(0.0),
                height: 0.006,
            },
            color: Color::rgb(0.96, 0.52, 0.64),
        });
        text.push_text(
            commands,
            &format!("{fast}/{slow}"),
            BitmapTextStyle { x: 0.55, y, cell: 0.0034, color: Color::rgb(0.7, 0.75, 0.74) },
        );
    }
    text.push_text(
        commands,
        &format!("F {}  S {}", counts.fast_total(), counts.slow_total()),
        BitmapTextStyle { x: 0.395, y: 0.868, cell: 0.0038, color: Color::rgb(0.72, 0.84, 0.86) },
    );
}

fn push_result_timing_details(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    points: &[ResultTimingPoint],
) {
    let graph_rect = Rect { x: 0.635, y: 0.758, width: 0.21, height: 0.07 };
    commands.push(DrawCommand::Rect { rect: graph_rect, color: Color::rgb(0.032, 0.034, 0.036) });
    commands.push(DrawCommand::Rect {
        rect: Rect {
            x: graph_rect.x + graph_rect.width / 2.0,
            y: graph_rect.y,
            width: 0.001,
            height: graph_rect.height,
        },
        color: Color::rgb(0.5, 0.56, 0.56),
    });
    push_result_timing_distribution(commands, graph_rect, points);

    if let Some((average, stddev)) = result_timing_stats(points) {
        text.push_text(
            commands,
            &format!("AVG {}ms", format_timing_ms(average)),
            BitmapTextStyle {
                x: 0.635,
                y: 0.845,
                cell: 0.0038,
                color: Color::rgb(0.74, 0.84, 0.86),
            },
        );
        text.push_text(
            commands,
            &format!("DEV {}ms", format_timing_ms(stddev)),
            BitmapTextStyle {
                x: 0.735,
                y: 0.845,
                cell: 0.0038,
                color: Color::rgb(0.74, 0.84, 0.86),
            },
        );
        text.push_text(
            commands,
            &format!("N {}", points.len()),
            BitmapTextStyle {
                x: 0.635,
                y: 0.868,
                cell: 0.0038,
                color: Color::rgb(0.68, 0.76, 0.74),
            },
        );
    } else {
        text.push_text(
            commands,
            "NO TIMING DATA",
            BitmapTextStyle {
                x: 0.635,
                y: 0.845,
                cell: 0.004,
                color: Color::rgb(0.68, 0.72, 0.72),
            },
        );
    }
}

fn push_result_density_graph(commands: &mut Vec<DrawCommand>, rect: Rect, density: &[u8]) {
    if density.is_empty() {
        return;
    }
    let max = density.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bar_w = (rect.width / density.len().max(1) as f32).max(0.001);
    for (index, value) in density.iter().enumerate() {
        if *value == 0 {
            continue;
        }
        let height = rect.height * (*value as f32 / max);
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: rect.x + index as f32 * bar_w,
                y: rect.y + rect.height - height,
                width: bar_w * 0.8,
                height,
            },
            color: Color::rgba(0.64, 0.75, 0.9, 0.75),
        });
    }
}

fn push_result_timing_distribution(
    commands: &mut Vec<DrawCommand>,
    rect: Rect,
    points: &[ResultTimingPoint],
) {
    if points.is_empty() {
        return;
    }
    const BUCKETS: usize = 21;
    const RANGE_MS: f32 = 50.0;
    let mut counts = [0u32; BUCKETS];
    for point in points {
        let delta_ms = (point.delta_us as f32 / 1_000.0).clamp(-RANGE_MS, RANGE_MS);
        let bucket =
            (((delta_ms + RANGE_MS) / (RANGE_MS * 2.0)) * (BUCKETS as f32 - 1.0)).round() as usize;
        counts[bucket.min(BUCKETS - 1)] += 1;
    }
    let max = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bar_w = rect.width / BUCKETS as f32;
    for (index, count) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let height = rect.height * (*count as f32 / max);
        let t = index as f32 / (BUCKETS - 1) as f32;
        let color = if t < 0.5 {
            Color::rgba(0.45, 0.86, 0.96, 0.78)
        } else {
            Color::rgba(0.96, 0.52, 0.64, 0.78)
        };
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: rect.x + index as f32 * bar_w,
                y: rect.y + rect.height - height,
                width: bar_w * 0.8,
                height,
            },
            color,
        });
    }
}

fn format_timing_ms(value: f32) -> String {
    format!("{value:.2}")
}

fn push_judge_line(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
) {
    let image = skin_manifest.play_judge_line_image();
    let line_y = judge_line_y(board, lift);
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
            linear_filter: false,
        }],
    );
}

fn note_rect_y(board: Rect, lift: f32, progress_to_hit: f32) -> f32 {
    play_object_y(board, lift, progress_to_hit) - NOTE_HEIGHT
}

fn play_object_y(board: Rect, lift: f32, progress_to_hit: f32) -> f32 {
    let judge_y = judge_line_y(board, lift);
    judge_y - progress_to_hit.clamp(0.0, 1.0) * (judge_y - board.y)
}

fn push_play_bar_line(
    commands: &mut Vec<DrawCommand>,
    skin: &SkinContext,
    skin_state: crate::skin::SkinDrawState,
    board: Rect,
    lift: f32,
    bar: &crate::snapshot::VisibleBarLine,
    skin_offsets: SkinOffsetValues,
) {
    let start = commands.len();
    let items = skin.document_bar_line_items(bar.y, skin_state);
    if items.is_empty() {
        push_bar_line_rect_geometry(commands, board, lift, bar.y, skin_offsets);
    } else {
        let items = skin.apply_play_skin_global_offset(items, skin_state);
        append_skin_render_items(commands, &items);
    }
    apply_bar_line_alpha_offset(&mut commands[start..], skin_offsets);
}

/// beatoraja `SkinObject.prepareColor` 相当。小節線コマンド列に alpha offset を加算する。
fn apply_bar_line_alpha_offset(commands: &mut [DrawCommand], skin_offsets: SkinOffsetValues) {
    let offset = skin_offsets.get(SKIN_OFFSET_BAR_LINE).unwrap_or_default();
    if offset.a == 0 {
        return;
    }
    let alpha_delta = offset.a as f32 / 255.0;
    for command in commands {
        match command {
            DrawCommand::Image { tint, .. } | DrawCommand::RotatedImage { tint, .. } => {
                tint.a = (tint.a + alpha_delta).clamp(0.0, 1.0);
            }
            DrawCommand::Rect { color, .. } => {
                color.a = (color.a + alpha_delta).clamp(0.0, 1.0);
            }
            DrawCommand::RectBatch { rects, .. } => {
                for rect in Arc::make_mut(rects) {
                    rect.color.a = (rect.color.a + alpha_delta).clamp(0.0, 1.0);
                }
            }
            DrawCommand::Text { .. } => {}
        }
    }
}

fn push_bar_line_rect_geometry(
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
    bar_y: f32,
    skin_offsets: SkinOffsetValues,
) {
    let y = play_object_y(board, lift, bar_y);
    let offset = skin_offsets.get(SKIN_OFFSET_BAR_LINE).unwrap_or_default();
    let height = (0.004 + offset.h as f32 / 1080.0).max(0.0);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: board.x, y, width: board.width, height },
        color: Color::rgba(0.45, 0.48, 0.5, 1.0),
    });
}

fn judge_line_y(board: Rect, lift: f32) -> f32 {
    let lift_offset = lift.clamp(0.0, 1.0) * board.height;
    let raw = board.y + board.height * JUDGE_LINE_Y_RATIO - lift_offset;
    raw.max(board.y)
}

fn push_receptors(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
    lane_width: f32,
    active_lanes: &[Lane],
) {
    let receptor = skin_manifest.play_receptor_image();
    let lift_offset = lift.clamp(0.0, 1.0) * board.height;
    let receptor_y = (board.y + board.height * 0.825 - lift_offset).max(board.y);
    for (display_index, &lane) in active_lanes.iter().enumerate() {
        let x = board.x + display_index as f32 * lane_width;
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
                linear_filter: false,
            }],
        );
    }
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
            linear_filter: false,
        }],
    );
}

fn play_progress(snapshot: &RenderSnapshot) -> f32 {
    if snapshot.duration.0 <= 0 {
        0.0
    } else {
        (snapshot.time.0 as f32 / snapshot.duration.0 as f32).clamp(0.0, 1.0)
    }
}

fn end_of_note(snapshot: &RenderSnapshot) -> bool {
    snapshot.duration.0 > 0 && snapshot.time.0 >= snapshot.duration.0
}

fn push_play_text(text: &TextRenderer, commands: &mut Vec<DrawCommand>, snapshot: &RenderSnapshot) {
    push_play_status_text(text, commands, snapshot);
    push_judge_count_text(text, commands, snapshot);
    let metadata = difficulty_level_label(&snapshot.difficulty_name, &snapshot.play_level, "");
    if !metadata.is_empty() {
        text.push_text(
            commands,
            &metadata,
            BitmapTextStyle {
                x: 0.055,
                y: 0.155,
                cell: 0.0045,
                color: Color::rgb(0.62, 0.76, 0.72),
            },
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
        let region_count = skin_context.document().map(|d| d.judge_region_count()).unwrap_or(1);
        let region = crate::skin::lane_judge_region(
            judgement.lane.index(),
            bmz_core::lane::LANE_COUNT,
            region_count,
        );
        skin_context.document_judge_items(
            &judgement.text,
            snapshot.combo,
            ((snapshot.time.0 - judgement.time.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
            snapshot.skin_offsets,
            region,
        )
    });
    let has_judge_image = judge_image.is_some();
    if let Some(judge_items) = judge_image {
        append_skin_render_items(commands, &judge_items);
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
            .filter(|item| {
                !matches!(item, SkinRenderItem::Text { text, .. } if text == &text_values[0].1)
                    && !matches!(item, SkinRenderItem::Text { text, .. } if text == &snapshot.combo.to_string())
            })
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
    push_note_skin_image(commands, lane, rect, skin_manifest.play_note_image());
}

fn push_ln_start_skin(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
) {
    push_note_skin_image(commands, lane, rect, skin_manifest.play_ln_start_image());
}

fn push_ln_end_skin(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
) {
    push_note_skin_image(commands, lane, rect, skin_manifest.play_ln_end_image());
}

fn push_note_skin_image(
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
    note: SkinImageManifest,
) {
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
            linear_filter: false,
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
                    font_id: None,
                    size: 0.05,
                    bitmap_size: None,
                    color: Color::rgb(0.94, 0.98, 1.0),
                    layer: TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
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
                    font_id: None,
                    size: 0.03,
                    bitmap_size: None,
                    color: Color::rgb(0.96, 0.92, 0.54),
                    layer: TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
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

fn push_default_failed_overlay(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let Some(elapsed_ms) = snapshot.failed_elapsed_ms else {
        return;
    };
    let alpha = (elapsed_ms as f32 / 700.0).clamp(0.0, 0.82);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        color: Color::rgba(0.0, 0.0, 0.0, alpha),
    });
    let label = "FAILED";
    let cell = 0.02;
    text.push_text(
        commands,
        label,
        BitmapTextStyle {
            x: 0.5 - label_width(label, cell) / 2.0,
            y: 0.43,
            cell,
            color: Color::rgba(1.0, 0.24, 0.28, alpha.clamp(0.35, 1.0)),
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
    active_lanes: &[Lane],
) {
    for (display_index, &lane) in active_lanes.iter().enumerate() {
        let center_x = board.x + display_index as f32 * lane_width + lane_width / 2.0;
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

fn long_note_body_color(mode: LongNoteMode) -> Color {
    match mode {
        LongNoteMode::Ln => LONG_NOTE_BODY_COLOR,
        LongNoteMode::Cn => CN_BODY_COLOR,
        LongNoteMode::Hcn => HCN_BODY_COLOR,
    }
}

fn judgement_lane_flash_color(snapshot: &RenderSnapshot, lane: Lane) -> Option<Color> {
    let judgement = snapshot.recent_judgements.iter().rev().find(|judgement| {
        judgement.lane == lane
            && !judgement.is_miss
            && (0..=220_000).contains(&(snapshot.time.0 - judgement.time.0))
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
        Lane::Key8 => "1'",
        Lane::Key9 => "2'",
        Lane::Key10 => "3'",
        Lane::Key11 => "4'",
        Lane::Key12 => "5'",
        Lane::Key13 => "6'",
        Lane::Key14 => "7'",
        Lane::Scratch2 => "S2",
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
        Lane::Key8 => "Z",
        Lane::Key9 => "S",
        Lane::Key10 => "X",
        Lane::Key11 => "D",
        Lane::Key12 => "C",
        Lane::Key13 => "F",
        Lane::Key14 => "V",
        Lane::Scratch2 => "LS",
    }
}

fn label_width(label: &str, cell: f32) -> f32 {
    let chars = label.chars().count() as f32;
    if chars == 0.0 { 0.0 } else { (chars * 3.0 + (chars - 1.0)) * cell }
}

#[cfg(test)]
mod tests {
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use crate::skin::{SkinDocument, SkinDocumentTexture, SkinImageSize, SkinTextureId};
    use crate::snapshot::{
        DisplayInput, DisplayJudgeCounts, DisplayJudgement, LongBodyState, NoteVisualKind,
        RenderSnapshot, VisibleBarLine, VisibleLongNote, VisibleNote,
    };

    use super::*;

    #[test]
    fn play_plan_renders_long_note_body() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_long_notes.push(VisibleLongNote {
            lane: Lane::Key4,
            mode: bmz_chart::model::LongNoteMode::Ln,
            head_y: 0.1,
            tail_y: 0.7,
            body_state: LongBodyState::Inactive,
        });

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        // 胴体は LONG_NOTE_BODY_COLOR の Rect。終端(tail)が始端(head)より画面上方にある。
        let body = plan.commands.iter().find_map(|command| match command {
            DrawCommand::Rect { rect, color } if *color == LONG_NOTE_BODY_COLOR => Some(*rect),
            _ => None,
        });
        let body = body.expect("long note body rect should be present");
        assert!(body.height > NOTE_HEIGHT, "body should be taller than a tap note");
        let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };
        assert!(approx_eq(body.y, play_object_y(board, 0.0, 0.7)));
        assert!(approx_eq(body.y + body.height, play_object_y(board, 0.0, 0.1)));
    }

    #[test]
    fn play_plan_colors_long_note_body_by_mode() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_long_notes.push(VisibleLongNote {
            lane: Lane::Key4,
            mode: LongNoteMode::Cn,
            head_y: 0.1,
            tail_y: 0.7,
            body_state: LongBodyState::Inactive,
        });
        snapshot.visible_long_notes.push(VisibleLongNote {
            lane: Lane::Key6,
            mode: LongNoteMode::Hcn,
            head_y: 0.1,
            tail_y: 0.7,
            body_state: LongBodyState::Inactive,
        });

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(
            |command| matches!(command, DrawCommand::Rect { color, .. } if *color == CN_BODY_COLOR)
        ));
        assert!(plan.commands.iter().any(
            |command| matches!(command, DrawCommand::Rect { color, .. } if *color == HCN_BODY_COLOR)
        ));
    }

    #[test]
    fn play_plan_includes_lanes_notes_and_bar_lines() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.5,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
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
            kind: NoteVisualKind::Tap,
            processed_judge: None,
        });
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.5,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
        });
        snapshot.visible_notes[Lane::Key2.index()].push(VisibleNote {
            lane: Lane::Key2,
            time: TimeUs(1_000),
            y: 0.5,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
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
    fn result_plan_uses_skin_document_for_result_and_course_result_types() {
        use crate::scene::ResultSnapshot;
        use crate::skin::SkinTextureId;
        use crate::snapshot::FastSlowJudgeCounts;
        use bmz_core::clear::ClearType;

        for skin_type in [7, 15] {
            let json = r#"{
                "type": __TYPE__,
                "name": "test",
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "x.png"}],
                "image": [{"id": "logo", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8}],
                "destination": [
                    {"id": "logo", "dst": [{"x": 0, "y": 0, "w": 8, "h": 8}]}
                ]
            }"#
            .replace("__TYPE__", &skin_type.to_string());
            let document: crate::skin::SkinDocument = serde_json::from_str(&json).unwrap();
            let manifest: SkinManifest = toml::from_str("").unwrap();
            let source_texture = crate::skin::SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(99),
                source_size: crate::skin::SkinImageSize { width: 64.0, height: 64.0 },
            };
            let skin =
                SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
            let snapshot = ResultSnapshot {
                clear_type: ClearType::Normal,
                arrange: "NORMAL".to_string(),
                lane_shuffle_pattern: Vec::new(),
                ex_score: 100,
                ex_score_rate: 0.5,
                max_combo: 50,
                bp: 0,
                cb: 0,
                gauge_value: 80.0,
                gauge_type: bmz_core::clear::GaugeType::Normal as i32,
                total_notes: 100,
                grade_diff_display: crate::scene::ResultGradeDiffDisplay::default(),
                duration_ms: 0,
                initial_bpm: 0.0,
                min_bpm: 0.0,
                max_bpm: 0.0,
                main_bpm: 0.0,
                total_gauge: 0.0,
                judge_rank: None,
                key_mode: bmz_core::lane::KeyMode::default(),
                result_gauge_graph_type: bmz_core::clear::GaugeType::Normal as i32,
                judge_counts: DisplayJudgeCounts::default(),
                fast_slow_counts: FastSlowJudgeCounts::default(),
                score_history_id: 0,
                replay_saved: false,
                replay_slots: [false; 4],
                saved_replay_slots: [false; 4],
                best_ex_score: None,
                best_clear_type: None,
                target_ex_score: None,
                best_max_combo: None,
                target_max_combo: None,
                best_bp: None,
                target_bp: None,
                previous_best_ex_score: None,
                previous_best_max_combo: None,
                previous_best_bp: None,
                target_clear_type: None,
                elapsed_time: TimeUs(0),
                fadeout_elapsed: None,
                title: String::new(),
                subtitle: String::new(),
                artist: String::new(),
                subartist: String::new(),
                genre: String::new(),
                difficulty_name: String::new(),
                play_level: String::new(),
                graph: crate::snapshot::ResultGraphSnapshot::default(),
                overlay: crate::snapshot::OverlaySnapshot::default(),
            };

            let plan = DrawPlan::from_scene_with_skin(
                &AppSceneSnapshot::Result(snapshot),
                &skin,
                &mut crate::skin::DynamicTimerRuntime::default(),
            );

            assert!(plan.commands.iter().any(|command| matches!(
                command,
                DrawCommand::Image { texture, .. } if *texture == TextureId(99)
            )));
        }
    }

    #[test]
    fn result_plan_supplies_result_judge_graph_data_to_skin_document() {
        use crate::scene::ResultSnapshot;
        use crate::snapshot::{FastSlowJudgeCounts, ResultGraphSnapshot, ResultJudgeGraphBucket};
        use bmz_core::clear::ClearType;

        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "name": "test",
                "w": 100,
                "h": 100,
                "judgegraph": [{"id": "jg", "type": 1, "backTexOff": 1, "noGap": 1}],
                "destination": [
                    {"id": "jg", "dst": [{"x": 0, "y": 0, "w": 100, "h": 50}]}
                ]
            }"#,
        )
        .unwrap();
        let skin = SkinContext::from_manifest_and_document(
            toml::from_str("").unwrap(),
            document,
            std::iter::empty(),
        );
        let snapshot = ResultSnapshot {
            clear_type: ClearType::Normal,
            arrange: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 100,
            ex_score_rate: 0.5,
            max_combo: 50,
            bp: 0,
            cb: 0,
            gauge_value: 80.0,
            gauge_type: bmz_core::clear::GaugeType::Normal as i32,
            total_notes: 100,
            grade_diff_display: crate::scene::ResultGradeDiffDisplay::default(),
            duration_ms: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            main_bpm: 0.0,
            total_gauge: 0.0,
            judge_rank: None,
            key_mode: bmz_core::lane::KeyMode::default(),
            result_gauge_graph_type: bmz_core::clear::GaugeType::Normal as i32,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 0,
            replay_saved: false,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            target_bp: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_clear_type: None,
            elapsed_time: TimeUs(0),
            fadeout_elapsed: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            graph: ResultGraphSnapshot {
                judge_graph_density: vec![1, 3, 2],
                judge_graph_buckets: vec![ResultJudgeGraphBucket { values: [0, 0, 1, 0, 0, 0] }],
                ..ResultGraphSnapshot::default()
            },
            overlay: crate::snapshot::OverlaySnapshot::default(),
        };

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Result(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| {
            draw_command_has_rect_color(command, |Color { r, g, b, .. }| {
                (*r - 0.0).abs() < 0.01 && (*g - 1.0).abs() < 0.01 && (*b - 0.53).abs() < 0.01
            })
        }));
    }

    #[test]
    fn result_skin_state_sets_beatoraja_result_timers() {
        let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
            panic!("sample result scene");
        };
        snapshot.elapsed_time = TimeUs(500_000);

        let pending = build_result_skin_draw_state(&snapshot, 1000);
        assert_eq!(pending.result_graph_begin_ms, Some(500));
        assert_eq!(pending.result_graph_end_ms, Some(500));
        assert_eq!(pending.result_update_score_ms, None);

        snapshot.elapsed_time = TimeUs(1_500_000);
        let active = build_result_skin_draw_state(&snapshot, 1000);
        assert_eq!(active.result_update_score_ms, Some(500));

        let immediate = build_result_skin_draw_state(&snapshot, 0);
        assert_eq!(immediate.result_update_score_ms, Some(1500));
    }

    #[test]
    fn result_skin_state_maps_arrange_option() {
        let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
            panic!("sample result scene");
        };
        snapshot.arrange = "S-RANDOM-EX".to_string();

        let state = build_result_skin_draw_state(&snapshot, 0);

        assert_eq!(state.select_arrange_index, 9);
        assert_eq!(state.result_arrange_index, 9);
    }

    #[test]
    fn result_skin_state_maps_random_lane_pattern() {
        let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
            panic!("sample result scene");
        };
        let mut pattern = (0..bmz_core::lane::LANE_COUNT as u8).collect::<Vec<_>>();
        pattern[bmz_core::lane::Lane::Key1.index()] = bmz_core::lane::Lane::Key7.index() as u8;
        snapshot.arrange = "RANDOM".to_string();
        snapshot.lane_shuffle_pattern = pattern;

        let state = build_result_skin_draw_state(&snapshot, 0);

        assert_eq!(state.result_arrange_index, 2);
        assert_eq!(state.result_random_lane_refs[0], 7);
    }

    #[test]
    fn result_skin_state_falls_back_to_timing_points_for_average_timing() {
        let AppSceneSnapshot::Result(mut snapshot) = crate::sample::sample_result_scene() else {
            panic!("sample result scene");
        };
        snapshot.graph.timing_points = vec![
            crate::snapshot::ResultTimingPoint {
                time_ms: 0,
                delta_us: -12_000,
                judge: bmz_core::judge::Judge::Great,
            },
            crate::snapshot::ResultTimingPoint {
                time_ms: 1000,
                delta_us: 20_000,
                judge: bmz_core::judge::Judge::PGreat,
            },
        ];

        let state = build_result_skin_draw_state(&snapshot, 0);

        assert_eq!(state.average_timing_ms, Some(4.0));
        assert_eq!(state.stddev_timing_ms, Some(16.0));
    }

    #[test]
    fn result_plan_renders_gaugegraph_from_result_graph_data() {
        use crate::scene::ResultSnapshot;
        use crate::snapshot::{FastSlowJudgeCounts, ResultGaugeGraphPoint, ResultGraphSnapshot};
        use bmz_core::clear::ClearType;

        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "name": "test",
                "w": 100,
                "h": 100,
                "gaugegraph": [{"id": "gg"}],
                "destination": [
                    {"id": "gg", "dst": [{"x": 0, "y": 0, "w": 100, "h": 50}]}
                ]
            }"#,
        )
        .unwrap();
        let skin = SkinContext::from_manifest_and_document(
            toml::from_str("").unwrap(),
            document,
            std::iter::empty(),
        );
        let snapshot = ResultSnapshot {
            clear_type: ClearType::Normal,
            arrange: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 100,
            ex_score_rate: 0.5,
            max_combo: 50,
            bp: 0,
            cb: 0,
            gauge_value: 80.0,
            gauge_type: bmz_core::clear::GaugeType::Normal as i32,
            total_notes: 100,
            grade_diff_display: crate::scene::ResultGradeDiffDisplay::default(),
            duration_ms: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            main_bpm: 0.0,
            total_gauge: 0.0,
            judge_rank: None,
            key_mode: bmz_core::lane::KeyMode::default(),
            result_gauge_graph_type: bmz_core::clear::GaugeType::Normal as i32,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 0,
            replay_saved: false,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            target_bp: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_clear_type: None,
            elapsed_time: TimeUs(2_000_000),
            fadeout_elapsed: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            graph: ResultGraphSnapshot {
                gauge_points: vec![
                    ResultGaugeGraphPoint {
                        time_ms: 0,
                        value: 20.0,
                        border: 80.0,
                        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
                    },
                    ResultGaugeGraphPoint {
                        time_ms: 1_000,
                        value: 90.0,
                        border: 80.0,
                        gauge_type: bmz_core::clear::GaugeType::Normal as i32,
                    },
                ],
                ..ResultGraphSnapshot::default()
            },
            overlay: crate::snapshot::OverlaySnapshot::default(),
        };

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Result(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color: Color { r, g, b, .. }, .. }
                if (*r - 0.0).abs() < 0.01 && (*g - 1.0).abs() < 0.01 && (*b - 0.0).abs() < 0.01
        )));
    }

    #[test]
    fn result_plan_renders_timing_distribution_from_result_graph_data() {
        use crate::scene::ResultSnapshot;
        use crate::snapshot::{
            FastSlowJudgeCounts, ResultGraphSnapshot, ResultTimingDistribution, ResultTimingPoint,
        };
        use bmz_core::clear::ClearType;
        use bmz_core::judge::Judge;

        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "name": "test",
                "w": 100,
                "h": 100,
                "timingdistributiongraph": [{"id": "td", "graphColor": "00FF00FF"}],
                "destination": [
                    {"id": "td", "dst": [{"x": 0, "y": 0, "w": 100, "h": 50}]}
                ]
            }"#,
        )
        .unwrap();
        let skin = SkinContext::from_manifest_and_document(
            toml::from_str("").unwrap(),
            document,
            std::iter::empty(),
        );
        let mut timing_distribution = ResultTimingDistribution::default();
        timing_distribution.add(-12);
        timing_distribution.add(8);
        let snapshot = ResultSnapshot {
            clear_type: ClearType::Normal,
            arrange: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score: 100,
            ex_score_rate: 0.5,
            max_combo: 50,
            bp: 0,
            cb: 0,
            gauge_value: 80.0,
            gauge_type: bmz_core::clear::GaugeType::Normal as i32,
            total_notes: 100,
            grade_diff_display: crate::scene::ResultGradeDiffDisplay::default(),
            duration_ms: 0,
            initial_bpm: 0.0,
            min_bpm: 0.0,
            max_bpm: 0.0,
            main_bpm: 0.0,
            total_gauge: 0.0,
            judge_rank: None,
            key_mode: bmz_core::lane::KeyMode::default(),
            result_gauge_graph_type: bmz_core::clear::GaugeType::Normal as i32,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            score_history_id: 0,
            replay_saved: false,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            best_ex_score: None,
            best_clear_type: None,
            target_ex_score: None,
            best_max_combo: None,
            target_max_combo: None,
            best_bp: None,
            target_bp: None,
            previous_best_ex_score: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_clear_type: None,
            elapsed_time: TimeUs(0),
            fadeout_elapsed: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            graph: ResultGraphSnapshot {
                timing_distribution,
                timing_points: vec![
                    ResultTimingPoint { time_ms: 0, delta_us: -12_000, judge: Judge::Great },
                    ResultTimingPoint { time_ms: 100, delta_us: 8_000, judge: Judge::PGreat },
                ],
                ..ResultGraphSnapshot::default()
            },
            overlay: crate::snapshot::OverlaySnapshot::default(),
        };

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Result(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color: Color { r, g, b, .. }, .. }
                if (*r - 0.0).abs() < 0.01 && (*g - 1.0).abs() < 0.01 && (*b - 0.0).abs() < 0.01
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
            kind: NoteVisualKind::Tap,
            processed_judge: None,
        });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == TextureId(42)
        )));
    }

    #[test]
    fn play_skin_document_renders_bar_lines_in_note_area() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [
                        { "x": 10, "y": 20, "w": 5, "h": 60 },
                        { "x": 15, "y": 20, "w": 5, "h": 60 },
                        { "x": 20, "y": 20, "w": 5, "h": 60 },
                        { "x": 25, "y": 20, "w": 5, "h": 60 },
                        { "x": 30, "y": 20, "w": 5, "h": 60 },
                        { "x": 35, "y": 20, "w": 5, "h": 60 },
                        { "x": 40, "y": 20, "w": 5, "h": 60 },
                        { "x": 45, "y": 20, "w": 5, "h": 60 }
                    ],
                    "group": [
                        {
                            "id": "section-line",
                            "dst": [
                                { "x": 10, "y": 25, "w": 40, "h": 2, "r": 64, "g": 128, "b": 255, "a": 200 }
                            ]
                        }
                    ]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(77),
            source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let mut snapshot = RenderSnapshot::default();
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(1_000), y: 0.5 });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, tint, .. }
                if *texture == TextureId(77)
                    && approx_eq(rect.x, 0.1)
                    && approx_eq(rect.y + rect.height, 0.45)
                    && approx_eq(rect.width, 0.4)
                    && approx_eq(rect.height, 0.02)
                    && approx_eq(tint.r, 64.0 / 255.0)
                    && approx_eq(tint.g, 128.0 / 255.0)
                    && approx_eq(tint.b, 1.0)
                    && approx_eq(tint.a, 200.0 / 255.0)
        )));
    }

    #[test]
    fn play_skin_document_applies_bar_line_offset_height_and_alpha() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2, "a": 200 }]
                    }]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(77),
            source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let mut snapshot = RenderSnapshot::default();
        snapshot.skin_offsets.set(
            SKIN_OFFSET_BAR_LINE,
            crate::skin_offset::SkinOffsetValue { h: 3, a: -50, ..Default::default() },
        );
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(1_000), y: 0.5 });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, tint, .. }
                if *texture == TextureId(77)
                    && approx_eq(rect.height, 0.05)
                    && approx_eq(tint.a, 150.0 / 255.0)
        )));
    }

    #[test]
    fn default_play_bar_line_applies_height_and_alpha_offset() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.skin_offsets.set(
            SKIN_OFFSET_BAR_LINE,
            crate::skin_offset::SkinOffsetValue { h: 4, a: -128, ..Default::default() },
        );
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(1_000), y: 0.5 });

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color }
                if approx_eq(rect.height, 0.004 + 4.0 / 1080.0)
                    && approx_eq(color.a, 127.0 / 255.0)
        )));
    }

    #[test]
    fn play_skin_document_ignores_notes_offset_on_bar_lines() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "offset": 30,
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2, "a": 200 }]
                    }]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(77),
            source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let mut snapshot = RenderSnapshot::default();
        snapshot
            .skin_offsets
            .set(30, crate::skin_offset::SkinOffsetValue { h: 20, ..Default::default() });
        snapshot.skin_offsets.set(
            SKIN_OFFSET_BAR_LINE,
            crate::skin_offset::SkinOffsetValue { h: 5, a: -50, ..Default::default() },
        );
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(1_000), y: 0.5 });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, tint, .. }
                if *texture == TextureId(77)
                    && approx_eq(rect.height, 0.07)
                    && approx_eq(tint.a, 150.0 / 255.0)
        )));
    }

    #[test]
    fn play_skin_document_without_group_falls_back_to_bar_line_offset_rect() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let skin = SkinContext::from_manifest_and_document(manifest, document, []);
        let mut snapshot = RenderSnapshot::default();
        snapshot.skin_offsets.set(
            SKIN_OFFSET_BAR_LINE,
            crate::skin_offset::SkinOffsetValue { h: 4, a: -128, ..Default::default() },
        );
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(1_000), y: 0.5 });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color }
                if approx_eq(rect.height, 0.004 + 4.0 / 1080.0)
                    && approx_eq(color.a, 127.0 / 255.0)
        )));
    }

    #[test]
    fn play_skin_document_applies_bar_line_alpha_after_global_offset() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "line.png"}],
                "image": [{"id": "section-line", "src": 1, "x": 0, "y": 0, "w": 1, "h": 1}],
                "note": {
                    "dst": [{ "x": 10, "y": 20, "w": 5, "h": 60 }],
                    "group": [{
                        "id": "section-line",
                        "dst": [{ "x": 10, "y": 20, "w": 40, "h": 2, "a": 255 }]
                    }]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(77),
            source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let mut snapshot = RenderSnapshot::default();
        snapshot
            .skin_offsets
            .set(10, crate::skin_offset::SkinOffsetValue { w: 20, ..Default::default() });
        snapshot.skin_offsets.set(
            SKIN_OFFSET_BAR_LINE,
            crate::skin_offset::SkinOffsetValue { a: -64, ..Default::default() },
        );
        snapshot.bar_lines.push(VisibleBarLine { time: TimeUs(1_000), y: 0.5 });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == TextureId(77) && approx_eq(tint.a, 191.0 / 255.0)
        )));
    }

    #[test]
    fn play_skin_document_places_hit_timing_note_bottom_on_judge_line() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "note.png"}],
                "image": [{"id": "note", "src": 1, "x": 0, "y": 0, "w": 1, "h": 36}],
                "note": {
                    "note": ["note"],
                    "dst": [
                        { "x": 10, "y": 20, "w": 5, "h": 60 }
                    ]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(78),
            source_size: crate::skin::SkinImageSize { width: 1.0, height: 1.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.0,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
        });

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, .. }
                if *texture == TextureId(78)
                    && approx_eq(rect.y + rect.height, 0.8)
                    && approx_eq(rect.height, 0.36)
        )));
    }

    #[test]
    fn skin_lane_height_uses_document_note_area_for_lane_cover_offsets() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 1920,
                "h": 1080,
                "note": {
                    "dst": [
                        { "x": 100, "y": 357, "w": 10, "h": 723 },
                        { "x": 110, "y": 357, "w": 10, "h": 723 },
                        { "x": 120, "y": 357, "w": 10, "h": 723 },
                        { "x": 130, "y": 357, "w": 10, "h": 723 },
                        { "x": 140, "y": 357, "w": 10, "h": 723 },
                        { "x": 150, "y": 357, "w": 10, "h": 723 },
                        { "x": 160, "y": 357, "w": 10, "h": 723 },
                        { "x": 170, "y": 357, "w": 10, "h": 723 }
                    ]
                }
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let skin = SkinContext::from_manifest_and_document(manifest, document, []);

        assert!(approx_eq(skin_lane_height_px(&skin, 1080.0), 723.0));
    }

    #[test]
    fn play_skin_ready_timer_starts_after_load_timers() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "loadstart": 500,
                "loadend": 3000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [
                    {"id": "panel", "op": [80], "dst": [
                        {"time": 0, "x": 80, "y": 0, "w": 10, "h": 10}
                    ]},
                    {"id": "panel", "timer": 40, "dst": [
                        {"time": 0, "x": 0, "y": 0, "w": 10, "h": 10},
                        {"time": 1000, "x": 50, "y": 0, "w": 10, "h": 10}
                    ]}
                ]
            }"#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(99),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let before_ready = RenderSnapshot {
            time: TimeUs(-1_000_000),
            play_elapsed_time: TimeUs(3_000_000),
            ready_elapsed_time: None,
            ..Default::default()
        };
        let after_ready = RenderSnapshot {
            time: TimeUs(-1_000_000),
            play_elapsed_time: TimeUs(4_000_000),
            ready_elapsed_time: Some(TimeUs(500_000)),
            ..Default::default()
        };

        let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
        let before_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(before_ready),
            &skin,
            &mut dynamic_timers,
        );
        let after_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(after_ready),
            &skin,
            &mut dynamic_timers,
        );

        assert!(before_plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, .. }
                if *texture == TextureId(99) && approx_eq(rect.x, 0.8)
        )));
        assert!(after_plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, .. }
                if *texture == TextureId(99) && approx_eq(rect.x, 0.25)
        )));
        assert!(!after_plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, .. }
                if *texture == TextureId(99) && approx_eq(rect.x, 0.8)
        )));
    }

    #[test]
    fn play_skin_loaded_after_load_delay_without_ready_timer() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "loadstart": 500,
                "loadend": 3000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [
                    {"id": "panel", "op": [81], "dst": [
                        {"time": 0, "x": 20, "y": 0, "w": 10, "h": 10}
                    ]}
                ]
            }"#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(99),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let loaded_before_ready = RenderSnapshot {
            time: TimeUs(-1_000_000),
            play_elapsed_time: TimeUs(3_500_000),
            ready_elapsed_time: None,
            ..Default::default()
        };

        let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(loaded_before_ready),
            &skin,
            &mut dynamic_timers,
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, .. }
                if *texture == TextureId(99) && approx_eq(rect.x, 0.2)
        )));
    }

    #[test]
    fn play_skin_untimed_intro_uses_scene_elapsed_without_loadend_offset() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "loadend": 3000,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [{"id": "panel", "loop": 1600, "dst": [
                    {"time": 1400, "x": 0, "y": 0, "w": 10, "h": 10, "a": 0},
                    {"time": 1600, "a": 255}
                ]}]
            }"#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(99),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let before_intro = RenderSnapshot {
            time: TimeUs(-1_000_000),
            play_elapsed_time: TimeUs(0),
            ..Default::default()
        };
        let during_intro = RenderSnapshot {
            time: TimeUs(-1_000_000),
            play_elapsed_time: TimeUs(1_500_000),
            ..Default::default()
        };

        let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
        let before_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(before_intro),
            &skin,
            &mut dynamic_timers,
        );
        let during_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(during_intro),
            &skin,
            &mut dynamic_timers,
        );

        assert!(!before_plan
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))));
        assert!(during_plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == TextureId(99) && approx_eq(tint.a, 128.0 / 255.0)
        )));
    }

    #[test]
    fn play_skin_play_timer_is_inactive_before_chart_start() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "source": [{"id": 1, "path": "panel.png"}],
                "image": [{"id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10}],
                "destination": [{"id": "panel", "timer": 41, "dst": [
                    {"time": 0, "x": 0, "y": 0, "w": 10, "h": 10}
                ]}]
            }"#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(99),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 10.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let before_start = RenderSnapshot {
            time: TimeUs(-1),
            play_elapsed_time: TimeUs(500_000),
            ..Default::default()
        };
        let after_start = RenderSnapshot {
            time: TimeUs(0),
            play_elapsed_time: TimeUs(500_000),
            ..Default::default()
        };

        let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
        let before_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(before_start),
            &skin,
            &mut dynamic_timers,
        );
        let after_plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(after_start),
            &skin,
            &mut dynamic_timers,
        );

        assert!(!before_plan
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))));
        assert!(after_plan
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::Image { texture, .. } if *texture == TextureId(99))));
    }

    #[test]
    fn play_plan_maps_normalized_note_y_to_distinct_screen_positions() {
        let mut snapshot = RenderSnapshot::default();
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(1_000),
            y: 0.75,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
        });
        snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
            lane: Lane::Key1,
            time: TimeUs(2_000),
            y: 0.25,
            kind: NoteVisualKind::Tap,
            processed_judge: None,
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

        assert!(note_ys.iter().any(|y| approx_eq(*y, 0.2255)));
        assert!(note_ys.iter().any(|y| approx_eq(*y, 0.6125)));
    }

    #[test]
    fn play_plan_places_hit_timing_note_on_judge_line() {
        let board = Rect { x: 0.18, y: 0.05, width: 0.64, height: 0.9 };

        assert!(approx_eq(note_rect_y(board, 0.0, 0.0) + NOTE_HEIGHT, judge_line_y(board, 0.0)));
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
    fn default_play_plan_includes_failed_overlay() {
        let snapshot = RenderSnapshot { failed_elapsed_ms: Some(500), ..Default::default() };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text == "FAILED"
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { color, .. } if color.a > 0.0
        )));
    }

    #[test]
    fn select_plan_has_non_empty_commands() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(Default::default()));

        assert!(!plan.commands.is_empty());
    }

    #[test]
    fn decide_plan_activates_fadeout_timer_destinations() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 6,
                "w": 100,
                "h": 100,
                "destination": [
                    { "id": -110, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 100, "h": 100, "a": 0 },
                        { "time": 200, "a": 255 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let skin = SkinContext::from_manifest_and_document(manifest, document, Vec::new());
        let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();

        let inactive = plan_decide(&RenderSnapshot::default(), &skin, &mut dynamic_timers);
        let active = plan_decide(
            &RenderSnapshot { fadeout_elapsed_ms: Some(100), ..RenderSnapshot::default() },
            &skin,
            &mut dynamic_timers,
        );

        assert!(!inactive.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Rect {
                    rect: Rect { x, y, width, height },
                    color: Color { r, g, b, a },
                } if approx_eq(*x, 0.0)
                    && approx_eq(*y, 0.0)
                    && approx_eq(*width, 1.0)
                    && approx_eq(*height, 1.0)
                    && approx_eq(*r, 0.0)
                    && approx_eq(*g, 0.0)
                    && approx_eq(*b, 0.0)
                    && approx_eq(*a, 128.0 / 255.0)
            )
        }));
        assert!(active.commands.iter().any(|command| {
            matches!(
                command,
                DrawCommand::Rect {
                    rect: Rect { x, y, width, height },
                    color: Color { r, g, b, a },
                } if approx_eq(*x, 0.0)
                    && approx_eq(*y, 0.0)
                    && approx_eq(*width, 1.0)
                    && approx_eq(*height, 1.0)
                    && approx_eq(*r, 0.0)
                    && approx_eq(*g, 0.0)
                    && approx_eq(*b, 0.0)
                    && approx_eq(*a, 128.0 / 255.0)
            )
        }));
    }

    #[test]
    fn select_detail_panel_shows_gas_state() {
        let snapshot = crate::scene::SelectSnapshot {
            option_panel: 3,
            gauge_auto_shift: "BEST CLEAR".to_string(),
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text == "GAS      BEST CLEAR"
        )));
    }

    #[test]
    fn custom_select_skin_does_not_force_stagefile_fullscreen_fallback() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "image": [{ "id": "panel", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [{ "id": "panel", "dst": [{ "x": 10, "y": 10, "w": 10, "h": 10 }] }]
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let skin = SkinContext::from_manifest_and_document(
            manifest,
            document,
            [SkinDocumentTexture {
                source_id: "1".to_string(),
                texture: SkinTextureId(1),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            }],
        );
        let mut dynamic_timers = crate::skin::DynamicTimerRuntime::default();
        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Select(crate::scene::SelectSnapshot {
                stage_background: true,
                stage_image_size: Some(SkinImageSize { width: 640.0, height: 480.0 }),
                ..Default::default()
            }),
            &skin,
            &mut dynamic_timers,
        );

        assert!(!plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, rect, .. }
                if *texture == SELECT_STAGE_TEXTURE
                    && approx_eq(rect.x, 0.0)
                    && approx_eq(rect.y, 0.0)
                    && approx_eq(rect.width, 1.0)
                    && approx_eq(rect.height, 1.0)
        )));
    }

    #[test]
    fn select_plan_renders_all_snapshot_rows() {
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
        assert_eq!(row_count, 20);
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("DIFFICULTY NORMAL") && text.contains("LEVEL 0")
        )));
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
        let judge_counts = DisplayJudgeCounts::default();
        let fast_slow_counts = FastSlowJudgeCounts::default();
        let graph = ResultGraphSnapshot::default();
        let plan = plan_result_fallback(ResultFallbackSummary {
            clear_type: "Normal",
            ex_score: 0,
            ex_score_rate: 1.5,
            max_combo: 0,
            gauge_value: 0.0,
            total_notes: 100,
            judge_counts: &judge_counts,
            fast_slow_counts: &fast_slow_counts,
            graph: &graph,
            score_history_id: 1,
            replay_saved: true,
            difficulty_name: "",
            play_level: "",
            grade_diff: String::new(),
        });

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Rect { rect, color } if rect.width == 0.72 && *color == Color::rgb(0.55, 0.78, 0.86)
        )));
    }

    #[test]
    fn result_plan_includes_extended_summary_text() {
        let judge_counts = DisplayJudgeCounts::default();
        let fast_slow_counts = FastSlowJudgeCounts::default();
        let graph = ResultGraphSnapshot::default();
        let plan = plan_result_fallback(ResultFallbackSummary {
            clear_type: "Normal",
            ex_score: 1500,
            ex_score_rate: 0.75,
            max_combo: 500,
            gauge_value: 82.0,
            total_notes: 1000,
            judge_counts: &judge_counts,
            fast_slow_counts: &fast_slow_counts,
            graph: &graph,
            score_history_id: 42,
            replay_saved: true,
            difficulty_name: "HYPER",
            play_level: "10",
            grade_diff: "AA+56".to_string(),
        });

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { style, .. } if style.color == Color::rgb(0.72, 0.84, 0.86)
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("DIFFICULTY HYPER") && text.contains("LEVEL 10")
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("GRADE AA+56")
        )));
        assert_eq!(format_percent(0.754), "75%");
    }

    #[test]
    fn result_plan_includes_stat_detail_panels() {
        let judge_counts =
            DisplayJudgeCounts { pgreat: 12, great: 8, good: 4, bad: 2, poor: 1, empty_poor: 3 };
        let fast_slow_counts = FastSlowJudgeCounts {
            fast_pgreat: 7,
            slow_pgreat: 5,
            fast_great: 3,
            slow_great: 5,
            fast_good: 1,
            slow_good: 3,
            fast_bad: 1,
            slow_bad: 1,
            fast_poor: 0,
            slow_poor: 1,
            fast_empty_poor: 2,
            slow_empty_poor: 1,
        };
        let graph = ResultGraphSnapshot {
            timing_points: vec![
                ResultTimingPoint {
                    time_ms: 100,
                    delta_us: -12_000,
                    judge: bmz_core::judge::Judge::Great,
                },
                ResultTimingPoint {
                    time_ms: 200,
                    delta_us: 8_000,
                    judge: bmz_core::judge::Judge::PGreat,
                },
            ],
            judge_graph_density: vec![1, 3, 2],
            ..ResultGraphSnapshot::default()
        };

        let plan = plan_result_fallback(ResultFallbackSummary {
            clear_type: "Normal",
            ex_score: 1500,
            ex_score_rate: 0.75,
            max_combo: 500,
            gauge_value: 82.0,
            total_notes: 1000,
            judge_counts: &judge_counts,
            fast_slow_counts: &fast_slow_counts,
            graph: &graph,
            score_history_id: 42,
            replay_saved: true,
            difficulty_name: "HYPER",
            play_level: "10",
            grade_diff: "AA+56".to_string(),
        });

        for label in ["JUDGE DETAILS", "FAST/SLOW DETAILS", "TIMING DETAILS"] {
            assert!(plan.commands.iter().any(|command| matches!(
                command,
                DrawCommand::Text { text, .. } if text == label
            )));
        }
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.starts_with("AVG ")
        )));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text == "F 5  S 10"
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
            difficulty_name: "ANOTHER".to_string(),
            play_level: "12".to_string(),
            ..Default::default()
        };

        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot));

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, tint, .. }
                if *texture == DEFAULT_JUDGE_LINE_TEXTURE && *tint == skin_image_tint(Lane::Key1)
        )));
        // デフォルトスキンではグルーブゲージを描画しない。
        assert!(!plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == DEFAULT_GAUGE_FRAME_TEXTURE
        )));
        assert!(!plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, .. } if *texture == DEFAULT_GAUGE_FILL_TEXTURE
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
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Text { text, .. } if text.contains("DIFFICULTY ANOTHER") && text.contains("LEVEL 12")
        )));
    }

    #[test]
    fn play_plan_uses_snapshot_arrange_for_skin_imageset() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "arrange.png" }],
                "image": [
                    { "id": "normal", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "mirror", "src": 1, "x": 10, "y": 0, "w": 10, "h": 10 },
                    { "id": "random", "src": 1, "x": 20, "y": 0, "w": 10, "h": 10 }
                ],
                "imageset": [
                    { "id": "arrange", "ref": 42, "images": ["normal", "mirror", "random"] }
                ],
                "destination": [
                    { "id": "arrange", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: crate::skin::SkinTextureId(77),
            source_size: crate::skin::SkinImageSize { width: 30.0, height: 10.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let snapshot = RenderSnapshot { arrange: "RANDOM".to_string(), ..Default::default() };

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, uv, .. }
                if *texture == TextureId(77) && (uv.x - 20.0 / 30.0).abs() < 0.001
        )));
    }

    #[test]
    fn play_plan_uses_snapshot_arrange_for_ref_image() {
        let document: crate::skin::SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "arrange.png" }],
                "image": [
                    { "id": "arrange", "src": 1, "x": 0, "y": 0, "w": 10, "h": 30, "divy": 3, "len": 3, "ref": 42 }
                ],
                "destination": [
                    { "id": "arrange", "dst": [{ "time": 0, "x": 10, "y": 20, "w": 20, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let source_texture = crate::skin::SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: crate::skin::SkinTextureId(77),
            source_size: crate::skin::SkinImageSize { width: 10.0, height: 30.0 },
        };
        let skin = SkinContext::from_manifest_and_document(manifest, document, [source_texture]);
        let snapshot = RenderSnapshot { arrange: "RANDOM".to_string(), ..Default::default() };

        let plan = DrawPlan::from_scene_with_skin(
            &AppSceneSnapshot::Play(snapshot),
            &skin,
            &mut crate::skin::DynamicTimerRuntime::default(),
        );

        assert!(plan.commands.iter().any(|command| matches!(
            command,
            DrawCommand::Image { texture, uv, .. }
                if *texture == TextureId(77) && (uv.y - 20.0 / 30.0).abs() < 0.001
        )));
    }

    #[test]
    fn play_plan_routes_recent_judge_text_through_default_skin() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key2,
                judge: Judge::PGreat,
                side: Some(TimingSide::Fast),
                text: "PGREAT FAST".to_string(),
                combo: 1,
                delta_us: -3_000,
                time: TimeUs(920_000),
                is_miss: false,
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
                judge: Judge::PGreat,
                side: Some(TimingSide::Fast),
                text: "PGREAT FAST".to_string(),
                combo: 1,
                delta_us: -3_000,
                time: TimeUs(920_000),
                is_miss: false,
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
                judge: Judge::EmptyPoor,
                side: Some(TimingSide::Slow),
                text: "EMPTY POOR SLOW".to_string(),
                combo: 0,
                delta_us: 50_000,
                time: TimeUs(980_000),
                is_miss: false,
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
                judge: Judge::Bad,
                side: Some(TimingSide::Slow),
                text: "BAD SLOW".to_string(),
                combo: 0,
                delta_us: 88_000,
                time: TimeUs(700_000),
                is_miss: false,
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

    #[test]
    fn row_status_label_shows_not_owned_for_unregistered_songs() {
        let unowned = SelectRowSnapshot {
            in_library: false,
            table_level: "12".to_string(),
            ..SelectRowSnapshot::default()
        };
        assert_eq!(row_status_label(Some(&unowned)), "NOT OWNED");
    }

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
        })
    }

    #[test]
    fn fallback_bga_uses_normal_blend_for_video_layer_textures() {
        // 動画 BGA Layer は beatoraja の `ffmpeg.frag` 相当で黒クロマキー
        // をかけないため、`is_video` が立っているときは Normal を選ぶ。
        use crate::snapshot::DisplayBgaFrame;
        let snapshot = RenderSnapshot {
            has_bga: true,
            bga_enabled: true,
            bga_stretch: 1,
            bga_base: Some(DisplayBgaFrame::opaque(100, 256.0, 256.0)),
            bga_layer: Some(DisplayBgaFrame::opaque_video(201, 640.0, 360.0)),
            bga_layer2: Some(DisplayBgaFrame::opaque(102, 256.0, 256.0)),
            ..Default::default()
        };
        let mut commands = Vec::new();
        push_fallback_bga_background(&mut commands, &snapshot);
        let blends: Vec<(u32, BlendMode)> = commands
            .iter()
            .filter_map(|cmd| match cmd {
                DrawCommand::Image { texture, blend, .. } => Some((texture.0, *blend)),
                _ => None,
            })
            .collect();
        assert_eq!(
            blends,
            vec![(100, BlendMode::Normal), (201, BlendMode::Normal), (102, BlendMode::LayerMask),]
        );
    }

    #[test]
    fn fallback_bga_uses_layer_mask_blend_for_layer_textures() {
        // BGA Layer / Layer2 は beatoraja の `layer.frag` 相当の黒クロマキー
        // (`BlendMode::LayerMask`) を使うことを担保する。
        // bl.jpg のような黒画像 Layer が Base を完全に覆い隠さないために必要。
        use crate::snapshot::DisplayBgaFrame;
        let snapshot = RenderSnapshot {
            has_bga: true,
            bga_enabled: true,
            bga_stretch: 1,
            bga_base: Some(DisplayBgaFrame::opaque(100, 256.0, 256.0)),
            bga_layer: Some(DisplayBgaFrame::opaque(101, 256.0, 256.0)),
            bga_layer2: Some(DisplayBgaFrame::opaque(102, 256.0, 256.0)),
            ..Default::default()
        };
        let mut commands = Vec::new();
        push_fallback_bga_background(&mut commands, &snapshot);
        let blends: Vec<(u32, BlendMode)> = commands
            .iter()
            .filter_map(|cmd| match cmd {
                DrawCommand::Image { texture, blend, .. } => Some((texture.0, *blend)),
                _ => None,
            })
            .collect();
        assert_eq!(
            blends,
            vec![
                (100, BlendMode::Normal),
                (101, BlendMode::LayerMask),
                (102, BlendMode::LayerMask),
            ]
        );
    }

    #[test]
    fn bga_fullscreen_geometry_letterbox_preserves_aspect() {
        let (rect, uv) = bga_fullscreen_geometry(1920.0, 1080.0, 1);
        assert!((rect.width - 1.0).abs() < f32::EPSILON);
        assert!((rect.height - (1080.0 / 1920.0)).abs() < 0.001);
        assert!((uv.width - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn miss_poor_does_not_flash_lane() {
        let snapshot = RenderSnapshot {
            time: TimeUs(1_000_000),
            recent_judgements: vec![DisplayJudgement {
                lane: Lane::Key3,
                judge: Judge::Poor,
                side: Some(TimingSide::Slow),
                text: "POOR SLOW".to_string(),
                combo: 0,
                delta_us: 50_000,
                time: TimeUs(950_000),
                is_miss: true,
            }],
            ..Default::default()
        };

        // 見逃しPOORでは判定ラインフラッシュを出さない
        assert_eq!(judgement_lane_flash_color(&snapshot, Lane::Key3), None);
        // 打鍵判定（is_miss=false）では通常通りフラッシュが出る
        let mut with_hit = snapshot.clone();
        with_hit.recent_judgements[0].is_miss = false;
        assert!(judgement_lane_flash_color(&with_hit, Lane::Key3).is_some());
    }

    fn approx_eq(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.0001
    }

    fn draw_command_has_rect_color(
        command: &DrawCommand,
        predicate: impl Fn(&Color) -> bool,
    ) -> bool {
        match command {
            DrawCommand::Rect { color, .. } => predicate(color),
            DrawCommand::RectBatch { rects, .. } => rects.iter().any(|rect| predicate(&rect.color)),
            _ => false,
        }
    }
}
