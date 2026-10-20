#[derive(Default)]
pub struct Renderer {
    pub(super) last_scene: Option<AppSceneSnapshot>,
    pub(super) last_plan: Option<DrawPlan>,
    pub(super) play_skin_context: SkinContext,
    pub(super) select_skin_context: SkinContext,
    pub(super) decide_skin_context: SkinContext,
    pub(super) result_skin_context: SkinContext,
    pub(super) last_plan_canvas_policy: CanvasRenderPolicy,
    pub(super) pending_textures: Vec<PendingTexture>,
    pub(super) pending_sprite_sheets: Vec<PendingSpriteSheet>,
    pub(super) fonts: HashMap<String, TextFace>,
    pub(super) bitmap_fonts: HashMap<String, BitmapFont>,
    pub(super) gpu: Option<WgpuRenderer>,
    pub(super) pending_egui: Option<EguiFrame>,
    pub(super) pending_screenshot: Option<ScreenshotRequest>,
    pub(super) play_dynamic_timer_runtime: DynamicTimerRuntime,
    pub(super) select_dynamic_timer_runtime: DynamicTimerRuntime,
    pub(super) decide_dynamic_timer_runtime: DynamicTimerRuntime,
    pub(super) result_dynamic_timer_runtime: DynamicTimerRuntime,
    pub(super) last_frame_timings: Option<RenderFrameTimings>,
    /// サーフェス生成時および `set_present_mode` で参照する希望 present mode。
    pub(super) present_mode: WgpuPresentMode,
    /// サーフェス生成時および再構成時に参照するin-flight frame数の決定方法。
    pub(super) frame_latency_mode: WgpuFrameLatencyMode,
    pub(super) internal_resolution_mode: InternalResolutionMode,
    pub(super) backend: WgpuBackend,
    pub(super) default_font_coverage: bmz_font::FontCoverage,
    pub(super) default_font_search_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderFrameTimings {
    pub plan_us: u128,
    pub draw_us: u128,
    pub text_us: u128,
    pub geometry_us: u128,
    pub upload_us: u128,
    pub submit_us: u128,
    pub surface_us: u128,
    pub bind_us: u128,
    pub encode_us: u128,
    pub queue_us: u128,
    pub present_us: u128,
    pub commands: usize,
    pub steps: usize,
    pub rect_steps: usize,
    pub image_steps: usize,
    pub text_steps: usize,
    pub rect_instances: usize,
    pub image_instances: usize,
    pub text_instances: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct GpuRenderTimings {
    pub(super) draw_us: u128,
    pub(super) text_us: u128,
    pub(super) geometry_us: u128,
    pub(super) upload_us: u128,
    pub(super) submit_us: u128,
    pub(super) surface_us: u128,
    pub(super) bind_us: u128,
    pub(super) encode_us: u128,
    pub(super) queue_us: u128,
    pub(super) present_us: u128,
    pub(super) steps: usize,
    pub(super) rect_steps: usize,
    pub(super) image_steps: usize,
    pub(super) text_steps: usize,
    pub(super) rect_instances: usize,
    pub(super) image_instances: usize,
    pub(super) text_instances: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum CanvasFitMode {
    #[default]
    Expand,
    Contain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CanvasSize {
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CanvasRenderPolicy {
    pub(super) fit_mode: CanvasFitMode,
    pub(super) canvas_size: Option<CanvasSize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct CanvasViewport {
    pub(super) rect: Rect,
    pub(super) content_size: SurfaceSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSurfaceStatus {
    Rendered,
    SkippedNoSurface,
    SkippedZeroSize,
    Reconfigured,
    TimedOut,
}

impl SurfaceSize {
    pub fn is_drawable(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

impl CanvasRenderPolicy {
    pub(super) fn skin_document(document: &SkinDocument) -> Self {
        Self {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: document.w.max(1), height: document.h.max(1) }),
        }
    }

    pub(super) fn internal_render_size(
        self,
        surface: SurfaceSize,
        mode: InternalResolutionMode,
    ) -> Option<SurfaceSize> {
        if mode == InternalResolutionMode::Native || !surface.is_drawable() {
            return None;
        }
        let canvas = self.canvas_size?;
        let content = CanvasViewport::from_policy(surface, self).content_size();
        // 小さいwindowでスキン解像度へ一度拡大してから縮小すると、負荷だけが増える。
        // 両辺が表示領域より小さい場合に限り内部targetを使う。
        (canvas.width < content.width && canvas.height < content.height)
            .then_some(SurfaceSize { width: canvas.width, height: canvas.height })
    }
}

impl CanvasViewport {
    pub(super) fn from_policy(surface: SurfaceSize, policy: CanvasRenderPolicy) -> Self {
        let full =
            Self { rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }, content_size: surface };
        if !surface.is_drawable() || policy.fit_mode == CanvasFitMode::Expand {
            return full;
        }

        let Some(canvas) = policy.canvas_size else {
            return full;
        };
        if canvas.width == 0 || canvas.height == 0 {
            return full;
        }

        let surface_aspect = surface.width as f32 / surface.height as f32;
        let canvas_aspect = canvas.width as f32 / canvas.height as f32;
        if !surface_aspect.is_finite() || !canvas_aspect.is_finite() {
            return full;
        }
        if (surface_aspect - canvas_aspect).abs() <= f32::EPSILON {
            return full;
        }

        let rect = if surface_aspect > canvas_aspect {
            let width = canvas_aspect / surface_aspect;
            Rect { x: (1.0 - width) * 0.5, y: 0.0, width, height: 1.0 }
        } else {
            let height = surface_aspect / canvas_aspect;
            Rect { x: 0.0, y: (1.0 - height) * 0.5, width: 1.0, height }
        };
        Self { rect, content_size: Self::content_size_for_rect(surface, rect) }
    }

    pub(super) fn content_size(self) -> SurfaceSize {
        self.content_size
    }

    pub(super) fn is_identity(self) -> bool {
        self.rect == Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }

    pub(super) fn transform_rect(self, rect: Rect) -> Rect {
        Rect {
            x: self.rect.x + rect.x * self.rect.width,
            y: self.rect.y + rect.y * self.rect.height,
            width: rect.width * self.rect.width,
            height: rect.height * self.rect.height,
        }
    }

    pub(super) fn surface_to_canvas_point(self, x: f32, y: f32) -> Option<(f32, f32)> {
        let right = self.rect.x + self.rect.width;
        let bottom = self.rect.y + self.rect.height;
        if x < self.rect.x || x > right || y < self.rect.y || y > bottom {
            return None;
        }
        Some(((x - self.rect.x) / self.rect.width, (y - self.rect.y) / self.rect.height))
    }

    /// Like [`Self::surface_to_canvas_point`], with points outside the canvas
    /// moved to its nearest edge.
    pub(super) fn surface_to_canvas_point_clamped(self, x: f32, y: f32) -> (f32, f32) {
        (
            ((x - self.rect.x) / self.rect.width).clamp(0.0, 1.0),
            ((y - self.rect.y) / self.rect.height).clamp(0.0, 1.0),
        )
    }

    pub(super) fn transform_rect_command(self, command: RectCommand) -> RectCommand {
        RectCommand { rect: self.transform_rect(command.rect), color: command.color }
    }

    pub(super) fn transform_rect_batch_cache(self, cache: RectBatchCache) -> RectBatchCache {
        RectBatchCache { bounds: self.transform_rect(cache.bounds), ..cache }
    }

    pub(super) fn transform_text_instances(self, instances: &mut [u8]) {
        for instance in instances.as_chunks_mut::<TEXT_INSTANCE_BYTES>().0 {
            let x = f32::from_le_bytes(instance[0..4].try_into().unwrap());
            let y = f32::from_le_bytes(instance[4..8].try_into().unwrap());
            let width = f32::from_le_bytes(instance[8..12].try_into().unwrap());
            let height = f32::from_le_bytes(instance[12..16].try_into().unwrap());
            let rect = self.transform_rect(Rect { x, y, width, height });
            instance[0..4].copy_from_slice(&rect.x.to_le_bytes());
            instance[4..8].copy_from_slice(&rect.y.to_le_bytes());
            instance[8..12].copy_from_slice(&rect.width.to_le_bytes());
            instance[12..16].copy_from_slice(&rect.height.to_le_bytes());
        }
    }

    pub(super) fn transform_text_caret_rects(self, rects: &mut [Option<RectCommand>]) {
        for rect in rects.iter_mut().flatten() {
            *rect = self.transform_rect_command(*rect);
        }
    }

    pub(super) fn content_size_for_rect(surface: SurfaceSize, rect: Rect) -> SurfaceSize {
        SurfaceSize {
            width: normalized_extent_to_pixels(rect.width, surface.width),
            height: normalized_extent_to_pixels(rect.height, surface.height),
        }
    }
}

pub(super) fn validate_rgba_texture(width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(anyhow!("texture dimensions must be non-zero"));
    }
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(anyhow!(
            "rgba texture length mismatch: expected {expected} bytes, got {}",
            rgba.len()
        ));
    }
    Ok(())
}
use super::*;
