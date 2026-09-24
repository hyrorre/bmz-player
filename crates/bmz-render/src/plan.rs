use std::sync::Arc;

use bmz_chart::model::LongNoteMode;
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

use crate::scene::{AppSceneSnapshot, SelectRowKind, SelectRowSnapshot, SelectSnapshot};
use crate::skin::{
    Animation, BlendMode, NumberSlot, SkinContext, SkinDefinition, SkinDocument,
    SkinDocumentRenderExt, SkinImageManifest, SkinImageSize, SkinManifest, SkinObject,
    SkinObjectId, SkinPhase, SkinPlacement, SkinRenderContext, SkinRenderItem, SkinSource,
    SkinTextState, SkinTextureId, TextSlot, append_skin_render_item, append_skin_render_items,
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
/// Play-only LOADINGFILE / READYFILE frame. Runtime skin image ID stays 100.
pub const PLAY_PRESENTATION_TEXTURE: TextureId = TextureId(23);
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
        source_size: Option<SkinImageSize>,
        texture: TextureId,
        tint: Color,
        blend: BlendMode,
        linear_filter: bool,
    },
    RotatedImage {
        rect: Rect,
        uv: UvRect,
        source_size: Option<SkinImageSize>,
        texture: TextureId,
        tint: Color,
        blend: BlendMode,
        linear_filter: bool,
        angle_rad: f32,
        center: Point,
        post_scale: Point,
    },
    Text {
        origin: Point,
        text: String,
        style: TextStyle,
        caret: Option<TextCaret>,
        post_scale: Point,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextCaret {
    pub byte_index: usize,
    pub color: Color,
}

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
    /// Preserve text height and compress only its width, matching beatoraja.
    Shrink,
    /// Shrink both axes uniformly and vertically center the result.
    ShrinkUniform,
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
        skin.begin_frame();
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
    state: &mut crate::skin::SkinDrawState,
    now_ms: i32,
) {
    if let Some(document) = skin.document() {
        runtime.advance(document, state, now_ms);
    }
}

fn push_fullscreen_image(commands: &mut Vec<DrawCommand>, texture: TextureId) {
    commands.push(DrawCommand::Image {
        rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        source_size: None,
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
        source_size: Some(SkinImageSize { width: frame.width, height: frame.height }),
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
        source_size: None,
        texture: SELECT_BANNER_TEXTURE,
        tint: Color::rgb(1.0, 1.0, 1.0),
        blend: BlendMode::Normal,
        linear_filter: true,
    });
}

mod decide;
mod play;
#[path = "plan/play_helpers/format.rs"]
mod play_helper_format;
#[path = "plan/play_helpers/geometry.rs"]
mod play_helper_geometry;
#[path = "plan/play_helpers/overlay.rs"]
mod play_helper_overlay;
#[path = "plan/play_helpers/skin.rs"]
mod play_helper_skin;
mod result;
mod select;

use decide::*;
use play::*;
use play_helper_format::*;
use play_helper_geometry::*;
use play_helper_overlay::*;
use play_helper_skin::*;
use result::*;
use select::*;

pub use result::result_skin_draw_state;
pub(crate) use result::result_skin_draw_state_for_document;

#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
