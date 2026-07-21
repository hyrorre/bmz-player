use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::mpsc;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
#[cfg(windows)]
use std::time::Duration;
use std::time::Instant;

use ab_glyph::{Font, FontArc, FontVec, Glyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result, anyhow};
use image::ImageEncoder;

use crate::assets::{RgbaImageAsset, load_png_rgba};
use crate::bitmap_font::{BitmapFont, load_bitmap_font};
use crate::plan::{
    Color, DrawCommand, DrawPlan, Point, Rect, RectBatchCache, RectBatchCacheKey, RectCommand,
    TextAlign, TextCaret, TextOverflow, TextStyle, TextureId, UvRect,
};
use crate::scene::AppSceneSnapshot;
use crate::skin::{
    BlendMode, DynamicTimerRuntime, SkinClickHit, SkinContext, SkinDocument, SkinImageSize,
    SkinSliderHit,
};
use crate::ui::{EguiFrame, EguiPainter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WgpuBackend {
    #[default]
    Auto,
    Vulkan,
    Metal,
    Dx12,
    Gl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WgpuPresentMode {
    #[default]
    Fifo,
    FifoRelaxed,
    Immediate,
    Mailbox,
}

/// Surfaceへ実際に適用されたpresent設定。要求modeがGPU/OSで利用できない場合、
/// `effective_mode`はfallback後の値になる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePresentationStatus {
    pub requested_mode: WgpuPresentMode,
    pub effective_mode: &'static str,
    pub maximum_frame_latency: u32,
}

/// 入力から表示までの待ちを最小化するため、通常modeでswapchainに許可する最大の
/// in-flight frame数。MailboxだけはDX12でこの値×monitor HzにFPSが制限されるため、
/// 既定値2を維持してFast VSyncがrefresh rateそのものへ落ちるのを避ける。
const LOW_LATENCY_MAXIMUM_FRAME_LATENCY: u32 = 1;
const MAILBOX_MAXIMUM_FRAME_LATENCY: u32 = 2;

impl WgpuBackend {
    pub fn to_wgpu(self) -> wgpu::Backends {
        match self {
            Self::Auto => auto_wgpu_backends(),
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Metal => wgpu::Backends::METAL,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Gl => wgpu::Backends::GL,
        }
    }
}

/// 設定 UI に表示できるレンダリングバックエンドを、現在の OS / feature 構成から返す。
///
/// wgpu の `enabled_backend_features` は、対象プラットフォームとビルド時に有効な
/// backend feature を反映する。`Auto` は常に利用可能な論理選択肢として含める。
pub fn available_wgpu_backends() -> Vec<WgpuBackend> {
    [WgpuBackend::Auto, WgpuBackend::Vulkan, WgpuBackend::Metal, WgpuBackend::Dx12, WgpuBackend::Gl]
        .into_iter()
        .filter(|backend| {
            *backend == WgpuBackend::Auto
                || wgpu::Instance::enabled_backend_features().contains(backend.to_wgpu())
        })
        .collect()
}

fn auto_wgpu_backends() -> wgpu::Backends {
    #[cfg(target_os = "linux")]
    {
        // Prefer Vulkan on Linux. GL/GLES remains available only as an
        // explicit fallback when Vulkan surface/device creation fails.
        wgpu::Backends::VULKAN
    }

    #[cfg(target_os = "windows")]
    {
        // Prefer DirectX 12 on Windows. Vulkan and GL remain available only as
        // explicit fallbacks when DirectX 12 surface/device creation fails.
        wgpu::Backends::DX12
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        wgpu::Backends::all()
    }
}

fn fallback_wgpu_backends(backend: WgpuBackend) -> &'static [WgpuBackend] {
    match backend {
        #[cfg(target_os = "linux")]
        WgpuBackend::Auto => &[WgpuBackend::Vulkan, WgpuBackend::Gl],
        #[cfg(target_os = "windows")]
        WgpuBackend::Auto => &[WgpuBackend::Dx12, WgpuBackend::Vulkan, WgpuBackend::Gl],
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        WgpuBackend::Auto => &[WgpuBackend::Auto],
        WgpuBackend::Vulkan => &[WgpuBackend::Vulkan],
        WgpuBackend::Metal => &[WgpuBackend::Metal],
        WgpuBackend::Dx12 => &[WgpuBackend::Dx12],
        WgpuBackend::Gl => &[WgpuBackend::Gl],
    }
}

#[derive(Default)]
pub struct Renderer {
    last_scene: Option<AppSceneSnapshot>,
    last_plan: Option<DrawPlan>,
    play_skin_context: SkinContext,
    select_skin_context: SkinContext,
    decide_skin_context: SkinContext,
    result_skin_context: SkinContext,
    last_plan_canvas_policy: CanvasRenderPolicy,
    pending_textures: Vec<PendingTexture>,
    fonts: HashMap<String, FontArc>,
    bitmap_fonts: HashMap<String, BitmapFont>,
    gpu: Option<WgpuRenderer>,
    pending_egui: Option<EguiFrame>,
    pending_screenshot: Option<ScreenshotRequest>,
    play_dynamic_timer_runtime: DynamicTimerRuntime,
    select_dynamic_timer_runtime: DynamicTimerRuntime,
    decide_dynamic_timer_runtime: DynamicTimerRuntime,
    result_dynamic_timer_runtime: DynamicTimerRuntime,
    last_frame_timings: Option<RenderFrameTimings>,
    /// サーフェス生成時および `set_present_mode` で参照する希望 present mode。
    present_mode: WgpuPresentMode,
    backend: WgpuBackend,
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
struct GpuRenderTimings {
    draw_us: u128,
    text_us: u128,
    geometry_us: u128,
    upload_us: u128,
    submit_us: u128,
    surface_us: u128,
    bind_us: u128,
    encode_us: u128,
    queue_us: u128,
    present_us: u128,
    steps: usize,
    rect_steps: usize,
    image_steps: usize,
    text_steps: usize,
    rect_instances: usize,
    image_instances: usize,
    text_instances: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CanvasFitMode {
    #[default]
    Expand,
    Contain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanvasSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CanvasRenderPolicy {
    fit_mode: CanvasFitMode,
    canvas_size: Option<CanvasSize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CanvasViewport {
    rect: Rect,
    content_size: SurfaceSize,
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
    fn skin_document(document: &SkinDocument) -> Self {
        Self {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: document.w.max(1), height: document.h.max(1) }),
        }
    }
}

impl CanvasViewport {
    fn from_policy(surface: SurfaceSize, policy: CanvasRenderPolicy) -> Self {
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

    fn content_size(self) -> SurfaceSize {
        self.content_size
    }

    fn is_identity(self) -> bool {
        self.rect == Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }

    fn transform_rect(self, rect: Rect) -> Rect {
        Rect {
            x: self.rect.x + rect.x * self.rect.width,
            y: self.rect.y + rect.y * self.rect.height,
            width: rect.width * self.rect.width,
            height: rect.height * self.rect.height,
        }
    }

    fn surface_to_canvas_point(self, x: f32, y: f32) -> Option<(f32, f32)> {
        let right = self.rect.x + self.rect.width;
        let bottom = self.rect.y + self.rect.height;
        if x < self.rect.x || x > right || y < self.rect.y || y > bottom {
            return None;
        }
        Some(((x - self.rect.x) / self.rect.width, (y - self.rect.y) / self.rect.height))
    }

    fn transform_rect_command(self, command: RectCommand) -> RectCommand {
        RectCommand { rect: self.transform_rect(command.rect), color: command.color }
    }

    fn transform_rect_batch_cache(self, cache: RectBatchCache) -> RectBatchCache {
        RectBatchCache { bounds: self.transform_rect(cache.bounds), ..cache }
    }

    fn transform_text_instances(self, instances: &mut [u8]) {
        for instance in instances.chunks_exact_mut(TEXT_INSTANCE_BYTES) {
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

    fn transform_text_caret_rects(self, rects: &mut [Option<RectCommand>]) {
        for rect in rects.iter_mut().flatten() {
            *rect = self.transform_rect_command(*rect);
        }
    }

    fn content_size_for_rect(surface: SurfaceSize, rect: Rect) -> SurfaceSize {
        SurfaceSize {
            width: normalized_extent_to_pixels(rect.width, surface.width),
            height: normalized_extent_to_pixels(rect.height, surface.height),
        }
    }
}

fn validate_rgba_texture(width: u32, height: u32, rgba: &[u8]) -> Result<()> {
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

struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    rect_pipeline: wgpu::RenderPipeline,
    rect_buffer: Option<wgpu::Buffer>,
    rect_buffer_capacity: usize,
    image_pipeline: wgpu::RenderPipeline,
    image_add_pipeline: wgpu::RenderPipeline,
    image_layer_pipeline: wgpu::RenderPipeline,
    image_bind_group_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    image_sampler_linear: wgpu::Sampler,
    image_textures: HashMap<TextureId, PreparedTexture>,
    image_bind_group_cache: HashMap<(TextureId, bool), wgpu::BindGroup>,
    image_bind_group_scratch: Vec<wgpu::BindGroup>,
    geometry_scratch: PlanGeometry,
    offscreen_rect_batches: HashMap<OffscreenRectBatchTextureKey, TextureId>,
    next_offscreen_rect_batch_texture_id: u32,
    image_buffer: Option<wgpu::Buffer>,
    image_buffer_capacity: usize,
    text_pipeline: wgpu::RenderPipeline,
    text_bind_group_layout: wgpu::BindGroupLayout,
    text_sampler: wgpu::Sampler,
    text_texture: Option<wgpu::Texture>,
    text_texture_view: Option<wgpu::TextureView>,
    text_bind_group: Option<wgpu::BindGroup>,
    text_texture_size: AtlasSize,
    text_atlas: TextAtlasCache,
    text_buffer: Option<wgpu::Buffer>,
    text_buffer_capacity: usize,
    font: Option<FontArc>,
    egui: EguiPainter,
    pending_screenshot_readbacks: Vec<ScreenshotReadback>,
    screenshot_save_jobs: Vec<ScreenshotSaveJob>,
    // Drop the surface after GPU resources so Linux native contexts are
    // released before the window/display teardown.
    surface: wgpu::Surface<'static>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OffscreenRectBatchTextureKey {
    key: RectBatchCacheKey,
    width: u32,
    height: u32,
}

const OFFSCREEN_RECT_BATCH_TEXTURE_BASE: u32 = 0xF000_0000;
const OFFSCREEN_RECT_BATCH_TEXTURE_MAX_ENTRIES: usize = 64;

struct ScreenshotCapture {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
}

#[derive(Debug, Clone)]
struct ScreenshotRequest {
    path: PathBuf,
    copy_to_clipboard: bool,
}

struct ScreenshotReadback {
    request: ScreenshotRequest,
    capture: ScreenshotCapture,
    rx: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

struct ScreenshotSaveJob {
    path: PathBuf,
    handle: thread::JoinHandle<Result<ScreenshotSaveOutcome>>,
}

struct ScreenshotSaveOutcome {
    path: PathBuf,
    clipboard_result: Option<Result<()>>,
}

impl ScreenshotCapture {
    fn new(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let bytes_per_pixel = 4;
        let unpadded_bytes_per_row = width.saturating_mul(bytes_per_pixel);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align).saturating_mul(align);
        let buffer_size = u64::from(padded_bytes_per_row).saturating_mul(u64::from(height));
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render screenshot buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self { buffer, width, height, padded_bytes_per_row, format }
    }

    fn copy_from_surface(&self, encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    fn start_readback(&self) -> mpsc::Receiver<Result<(), wgpu::BufferAsyncError>> {
        let slice = self.buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        rx
    }

    fn mapped_rgba(&self) -> Vec<u8> {
        let slice = self.buffer.slice(..);
        let mapped = slice.get_mapped_range();
        let rgba = unpack_screenshot_rgba(
            &mapped,
            self.width,
            self.height,
            self.padded_bytes_per_row,
            self.format,
        );
        drop(mapped);
        self.buffer.unmap();
        rgba
    }
}

fn unpack_screenshot_rgba(
    mapped: &[u8],
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    let mut rgba = vec![0; width as usize * height as usize * 4];
    let row_bytes = width as usize * 4;
    let padded_row_bytes = padded_bytes_per_row as usize;
    for y in 0..height as usize {
        let src_offset = y * padded_row_bytes;
        let dst_offset = y * row_bytes;
        rgba[dst_offset..dst_offset + row_bytes]
            .copy_from_slice(&mapped[src_offset..src_offset + row_bytes]);
    }

    if matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }

    rgba
}

fn encode_screenshot_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .context("failed to encode screenshot as PNG")?;
    Ok(png)
}

fn save_screenshot_png(path: &Path, png: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, png)
        .with_context(|| format!("failed to save screenshot {}", path.display()))
}

#[cfg(not(windows))]
fn copy_screenshot_to_clipboard(width: u32, height: u32, rgba: &[u8], _png: &[u8]) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open clipboard")?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Borrowed(rgba),
        })
        .context("failed to copy screenshot to clipboard")
}

#[cfg(any(windows, test))]
fn screenshot_dibv5(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    const HEADER_SIZE: usize = 124;
    const BI_BITFIELDS: u32 = 3;
    const LCS_SRGB: u32 = 0x7352_4742;
    const LCS_GM_IMAGES: u32 = 4;

    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .context("screenshot width is too large for DIBV5")?;
    let expected_len = row_bytes
        .checked_mul(height as usize)
        .context("screenshot dimensions are too large for DIBV5")?;
    if rgba.len() != expected_len {
        return Err(anyhow!(
            "invalid screenshot RGBA length for DIBV5: expected {expected_len}, got {}",
            rgba.len()
        ));
    }
    let image_size = u32::try_from(expected_len).context("screenshot DIBV5 exceeds 4 GiB")?;
    let width = i32::try_from(width).context("screenshot width exceeds DIBV5 range")?;
    let height = i32::try_from(height).context("screenshot height exceeds DIBV5 range")?;

    let mut dib = vec![0; HEADER_SIZE];
    dib[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    dib[4..8].copy_from_slice(&width.to_le_bytes());
    dib[8..12].copy_from_slice(&height.to_le_bytes());
    dib[12..14].copy_from_slice(&1_u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32_u16.to_le_bytes());
    dib[16..20].copy_from_slice(&BI_BITFIELDS.to_le_bytes());
    dib[20..24].copy_from_slice(&image_size.to_le_bytes());
    dib[40..44].copy_from_slice(&0x00ff_0000_u32.to_le_bytes());
    dib[44..48].copy_from_slice(&0x0000_ff00_u32.to_le_bytes());
    dib[48..52].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());
    dib[52..56].copy_from_slice(&0xff00_0000_u32.to_le_bytes());
    dib[56..60].copy_from_slice(&LCS_SRGB.to_le_bytes());
    dib[108..112].copy_from_slice(&LCS_GM_IMAGES.to_le_bytes());

    dib.reserve(expected_len);
    for row in rgba.chunks_exact(row_bytes).rev() {
        for pixel in row.chunks_exact(4) {
            dib.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(dib)
}

#[cfg(windows)]
fn copy_screenshot_to_clipboard(width: u32, height: u32, rgba: &[u8], png: &[u8]) -> Result<()> {
    const CF_DIBV5: u32 = 17;
    const OPEN_ATTEMPTS: usize = 50;
    const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

    // Prepare every large allocation before taking the process-global Windows clipboard lock.
    let dib = screenshot_dibv5(width, height, rgba)?;
    let png_format = clipboard_win::register_format("PNG")
        .context("failed to register Windows PNG clipboard format")?;
    let mut attempt = 0;
    let clipboard = loop {
        match clipboard_win::Clipboard::new() {
            Ok(clipboard) => break clipboard,
            Err(_) if attempt < OPEN_ATTEMPTS => {
                attempt += 1;
                thread::sleep(OPEN_RETRY_DELAY);
            }
            Err(error) => {
                return Err(anyhow!(
                    "failed to open Windows clipboard after {} attempts: {error}",
                    attempt + 1
                ));
            }
        }
    };
    clipboard_win::empty().context("failed to empty Windows clipboard")?;
    clipboard_win::raw::set_without_clear(png_format.get(), png)
        .context("failed to set Windows PNG clipboard data")?;
    clipboard_win::raw::set_without_clear(CF_DIBV5, &dib)
        .context("failed to set Windows DIBV5 clipboard data")?;
    drop(clipboard);
    Ok(())
}

fn spawn_screenshot_save_job(
    request: ScreenshotRequest,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<ScreenshotSaveJob> {
    let path = request.path.clone();
    let thread_path = path.clone();
    let handle = thread::Builder::new()
        .name("bmz-screenshot-save".to_string())
        .spawn(move || {
            let started = Instant::now();
            let png = encode_screenshot_png(width, height, &rgba)?;
            let png_encode_ms = started.elapsed().as_millis() as u64;
            let write_started = Instant::now();
            save_screenshot_png(&request.path, &png)?;
            let png_write_ms = write_started.elapsed().as_millis() as u64;
            let clipboard_started = Instant::now();
            let clipboard_result = request
                .copy_to_clipboard
                .then(|| copy_screenshot_to_clipboard(width, height, &rgba, &png));
            let clipboard_ms = clipboard_started.elapsed().as_millis() as u64;
            tracing::debug!(
                path = %request.path.display(),
                png_encode_ms,
                png_write_ms,
                clipboard_ms,
                "screenshot save job completed"
            );
            Ok(ScreenshotSaveOutcome { path: request.path, clipboard_result })
        })
        .with_context(|| {
            format!("failed to spawn screenshot save thread for {}", thread_path.display())
        })?;
    Ok(ScreenshotSaveJob { path, handle })
}

fn finish_screenshot_save_job(job: ScreenshotSaveJob) {
    match job.handle.join() {
        Ok(Ok(outcome)) => match outcome.clipboard_result {
            Some(Ok(())) => tracing::info!(
                path = %outcome.path.display(),
                "screenshot saved and copied to clipboard"
            ),
            Some(Err(error)) => tracing::warn!(
                error = %format!("{error:#}"),
                path = %outcome.path.display(),
                "screenshot saved but clipboard copy failed"
            ),
            None => tracing::info!(path = %outcome.path.display(), "screenshot saved"),
        },
        Ok(Err(error)) => tracing::warn!(
            error = %format!("{error:#}"),
            path = %job.path.display(),
            "failed to save screenshot"
        ),
        Err(_) => tracing::warn!(
            path = %job.path.display(),
            "screenshot save thread panicked"
        ),
    }
}

#[derive(Debug, Clone)]
struct PendingTexture {
    id: TextureId,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// GPU へアップロード済みのテクスチャ。別スレッド (skin upload worker) で
/// 生成してメインスレッドへ送るために公開する。`wgpu::Texture` /
/// `wgpu::TextureView` はどちらも `Send` なのでスレッド間で受け渡しできる。
pub struct PreparedTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl Renderer {
    pub fn attach_surface<T>(&mut self, window: T, size: SurfaceSize) -> Result<()>
    where
        T: Into<wgpu::SurfaceTarget<'static>> + Clone,
    {
        if !size.is_drawable() {
            self.gpu = None;
            return Ok(());
        }

        let mut gpu =
            WgpuRenderer::new_with_fallbacks(window, size, self.present_mode, self.backend)?;
        for texture in self.pending_textures.drain(..) {
            gpu.upsert_rgba_texture(texture.id, texture.width, texture.height, &texture.rgba);
        }
        self.gpu = Some(gpu);
        Ok(())
    }

    /// Drop GPU resources that depend on the window surface while the app still
    /// owns the native window.
    pub fn detach_surface(&mut self) {
        self.pending_egui = None;
        let Some(gpu) = self.gpu.take() else {
            return;
        };
        gpu.wait_idle_before_drop();
    }

    pub fn upsert_rgba_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<()> {
        validate_rgba_texture(width, height, &rgba)?;
        if let Some(gpu) = &mut self.gpu {
            gpu.upsert_rgba_texture(id, width, height, &rgba);
        } else {
            self.pending_textures.push(PendingTexture { id, width, height, rgba });
        }
        Ok(())
    }

    pub fn upsert_rgba_texture_ref(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<()> {
        validate_rgba_texture(width, height, rgba)?;
        if let Some(gpu) = &mut self.gpu {
            gpu.upsert_rgba_texture(id, width, height, rgba);
        } else {
            self.pending_textures.push(PendingTexture { id, width, height, rgba: rgba.to_vec() });
        }
        Ok(())
    }

    pub fn upsert_image_asset(&mut self, id: TextureId, asset: &RgbaImageAsset) -> Result<()> {
        asset.validate()?;
        self.upsert_rgba_texture_ref(id, asset.width, asset.height, &asset.pixels)
    }

    /// skin upload worker 用に、GPU アップロード機能の clone を取り出す。
    /// surface 未接続 (`gpu` が None) の間は `None`。
    /// 返り値は `Send + Clone` なので別スレッドへ渡せる。
    pub fn gpu_uploader(&self) -> Option<GpuUploader> {
        self.gpu
            .as_ref()
            .map(|gpu| GpuUploader { device: gpu.device.clone(), queue: gpu.queue.clone() })
    }

    /// worker でアップロード済みの `PreparedTexture` をテクスチャ表へ差し込む。
    /// surface 未接続時は (worker が存在しないため通常起きないが) 無視する。
    pub fn insert_prepared_texture(&mut self, id: TextureId, prepared: PreparedTexture) {
        if let Some(gpu) = &mut self.gpu {
            gpu.image_textures.insert(id, prepared);
            gpu.image_bind_group_cache.retain(|(texture_id, _), _| *texture_id != id);
        } else {
            tracing::warn!(
                texture_id = id.0,
                "dropping prepared texture because gpu surface is not attached"
            );
        }
    }

    pub fn load_png_texture(&mut self, id: TextureId, path: &std::path::Path) -> Result<()> {
        let asset = load_png_rgba(path)?;
        self.upsert_image_asset(id, &asset)
    }

    pub fn load_font(&mut self, id: impl Into<String>, path: &std::path::Path) -> Result<()> {
        let id = id.into();
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read font: {}", path.display()))?;
        let font = FontArc::try_from_vec(bytes)
            .map_err(|error| anyhow!("failed to parse font {}: {error}", path.display()))?;
        self.insert_vector_font(id, font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
        Ok(())
    }

    pub fn load_bitmap_font(
        &mut self,
        id: impl Into<String>,
        path: &std::path::Path,
    ) -> Result<()> {
        let font = load_bitmap_font(path)?;
        self.insert_bitmap_font_entry(id.into(), font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
        Ok(())
    }

    /// 事前に読み込んだフォントバイト列を登録する。
    /// バックグラウンドスレッドで I/O を済ませた後に main スレッドから登録する用途。
    pub fn install_font_bytes(&mut self, id: impl Into<String>, bytes: Vec<u8>) -> Result<()> {
        let font = FontArc::try_from_vec(bytes)
            .map_err(|error| anyhow!("failed to parse font bytes: {error}"))?;
        self.insert_vector_font(id.into(), font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
        Ok(())
    }

    /// 事前にパース済みの bitmap font を登録する。
    pub fn install_bitmap_font(&mut self, id: impl Into<String>, font: BitmapFont) {
        self.insert_bitmap_font_entry(id.into(), font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
    }

    fn insert_vector_font(&mut self, id: String, font: FontArc) {
        self.bitmap_fonts.remove(&id);
        self.fonts.insert(id, font);
    }

    fn insert_bitmap_font_entry(&mut self, id: String, font: BitmapFont) {
        self.fonts.remove(&id);
        self.bitmap_fonts.insert(id, font);
    }

    pub fn set_skin_context(&mut self, skin_context: SkinContext) {
        self.set_play_skin_context(skin_context, false);
    }

    /// `preserve_dynamic_timers` が true のとき、プレイ中のスキン差し替え向けに
    /// `timer_observe_boolean` の経過時刻を維持する。
    pub fn set_play_skin_context(
        &mut self,
        skin_context: SkinContext,
        preserve_dynamic_timers: bool,
    ) {
        if !preserve_dynamic_timers {
            self.play_dynamic_timer_runtime.reset();
        }
        self.play_skin_context = skin_context;
    }

    pub fn set_select_skin_context(&mut self, skin_context: SkinContext) {
        self.select_dynamic_timer_runtime.reset();
        self.select_skin_context = skin_context;
    }

    pub fn set_decide_skin_context(&mut self, skin_context: SkinContext) {
        self.decide_dynamic_timer_runtime.reset();
        self.decide_skin_context = skin_context;
    }

    pub fn set_result_skin_context(&mut self, skin_context: SkinContext) {
        self.result_skin_context = skin_context;
        self.result_dynamic_timer_runtime.reset_for_document(self.result_skin_context.document());
    }

    /// リザルトスキンが定義する内部 runtime event を dispatch する。
    ///
    /// クリック入力などを app 層が解決した後に呼ぶ。event が未定義なら false。
    pub fn dispatch_result_skin_runtime_event(&mut self, event_id: i32) -> bool {
        let Some(document) = self.result_skin_context.document() else {
            return false;
        };
        self.result_dynamic_timer_runtime.dispatch_runtime_event(document, event_id)
    }

    /// 同じリザルトスキンで新しい scene に入る際、runtime state を初期化する。
    pub fn reset_result_skin_runtime(&mut self) {
        self.result_dynamic_timer_runtime.reset_for_document(self.result_skin_context.document());
    }

    /// リザルトスキンが宣言する終了フェードアウト時間 (ms)。
    /// ドキュメントスキンが無い場合や未指定の場合は 0 を返す。
    pub fn result_skin_fadeout_ms(&self) -> i32 {
        self.result_skin_context.document().map(|document| document.fadeout).unwrap_or(0).max(0)
    }

    pub fn result_skin_timer_animation_duration_ms(&self, timer: i32) -> i32 {
        self.result_skin_context.timer_animation_duration_ms(timer)
    }

    /// 選曲スキンの document (設定 UI が property/offset 定義を読むため公開)。
    pub fn select_skin_document(&self) -> Option<&SkinDocument> {
        self.select_skin_context.document()
    }

    pub fn select_skin_click_hit(
        &self,
        snapshot: &crate::scene::SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        let (x, y) = self.select_skin_canvas_point(x, y)?;
        self.select_skin_context.select_click_hit(snapshot, x, y)
    }

    pub fn result_skin_click_hit(
        &self,
        snapshot: &crate::scene::ResultSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        let (x, y) = self.result_skin_canvas_point(x, y)?;
        let document = self.result_skin_context.document()?;
        let mut state = crate::plan::result_skin_draw_state(snapshot, document.ranktime);
        state.start_input_ms =
            crate::skin::skin_start_input_elapsed_ms(state.elapsed_ms, document.input);
        self.result_skin_context.result_click_hit(&state, x, y)
    }

    pub fn result_skin_slider_hit(
        &self,
        snapshot: &crate::scene::ResultSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let (x, y) = self.result_skin_canvas_point(x, y)?;
        let document = self.result_skin_context.document()?;
        let mut state = crate::plan::result_skin_draw_state(snapshot, document.ranktime);
        state.start_input_ms =
            crate::skin::skin_start_input_elapsed_ms(state.elapsed_ms, document.input);
        self.result_skin_context.result_slider_hit(&state, x, y)
    }

    pub fn select_skin_slider_hit(
        &self,
        snapshot: &crate::scene::SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let (x, y) = self.select_skin_canvas_point(x, y)?;
        self.select_skin_context.select_slider_hit(snapshot, x, y)
    }

    /// プレイスキンの document。
    pub fn play_skin_document(&self) -> Option<&SkinDocument> {
        self.play_skin_context.document()
    }

    pub fn set_play_skin_user_selected_options(&mut self, enabled_options: Vec<i32>) -> bool {
        self.play_skin_context.set_user_selected_options(enabled_options)
    }

    pub fn play_skin_timer_animation_duration_ms(&self, timer: i32) -> i32 {
        self.play_skin_context.timer_animation_duration_ms(timer)
    }

    /// 決定スキンの document。
    pub fn decide_skin_document(&self) -> Option<&SkinDocument> {
        self.decide_skin_context.document()
    }

    /// リザルトスキンの document。
    pub fn result_skin_document(&self) -> Option<&SkinDocument> {
        self.result_skin_context.document()
    }

    pub fn resize_surface(&mut self, size: SurfaceSize) {
        let Some(gpu) = &mut self.gpu else {
            return;
        };
        if !size.is_drawable() {
            return;
        }

        gpu.resize(size);
    }

    pub fn render_scene(&mut self, scene: AppSceneSnapshot) -> Result<()> {
        self.render_scene_status(scene).map(|_| ())
    }

    pub fn render_scene_status(&mut self, scene: AppSceneSnapshot) -> Result<RenderSurfaceStatus> {
        let entering_scene = self.last_scene.as_ref().is_none_or(|previous| {
            std::mem::discriminant(previous) != std::mem::discriminant(&scene)
        });
        if entering_scene {
            match &scene {
                AppSceneSnapshot::Select(_) => self
                    .select_dynamic_timer_runtime
                    .reset_for_document(self.select_skin_context.document()),
                AppSceneSnapshot::Decide(_) => self
                    .decide_dynamic_timer_runtime
                    .reset_for_document(self.decide_skin_context.document()),
                AppSceneSnapshot::Play(_) => self
                    .play_dynamic_timer_runtime
                    .reset_for_document(self.play_skin_context.document()),
                AppSceneSnapshot::Result(_) => self
                    .result_dynamic_timer_runtime
                    .reset_for_document(self.result_skin_context.document()),
            }
        }
        let plan_start = Instant::now();
        let plan = match &scene {
            AppSceneSnapshot::Select(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.select_skin_context,
                &mut self.select_dynamic_timer_runtime,
            ),
            AppSceneSnapshot::Decide(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.decide_skin_context,
                &mut self.decide_dynamic_timer_runtime,
            ),
            AppSceneSnapshot::Play(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.play_skin_context,
                &mut self.play_dynamic_timer_runtime,
            ),
            AppSceneSnapshot::Result(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.result_skin_context,
                &mut self.result_dynamic_timer_runtime,
            ),
        };
        let plan_us = plan_start.elapsed().as_micros();
        let commands = plan.commands.len();
        self.last_plan_canvas_policy = self.canvas_policy_for_scene(&scene);
        self.last_scene = Some(scene);
        self.last_plan = Some(plan);

        let status = self.render_last_plan()?;
        self.last_frame_timings = Some(RenderFrameTimings {
            plan_us,
            commands,
            ..self.last_frame_timings.unwrap_or_default()
        });
        Ok(status)
    }

    /// 次の描画フレームで重ねる egui の描画データを差し込む。
    ///
    /// `render_scene_status` / `render_last_plan` の呼び出しで消費される。
    pub fn set_egui_frame(&mut self, frame: EguiFrame) {
        self.pending_egui = Some(frame);
    }

    pub fn set_present_mode(&mut self, present_mode: WgpuPresentMode) {
        if self.present_mode == present_mode {
            return;
        }
        self.present_mode = present_mode;
        if let Some(gpu) = &mut self.gpu {
            gpu.set_present_mode(present_mode);
            tracing::info!(requested = ?present_mode, "present mode updated");
        }
    }

    pub fn surface_presentation_status(&self) -> Option<SurfacePresentationStatus> {
        let gpu = self.gpu.as_ref()?;
        Some(SurfacePresentationStatus {
            requested_mode: self.present_mode,
            effective_mode: wgpu_present_mode_label(gpu.config.present_mode),
            maximum_frame_latency: gpu.config.desired_maximum_frame_latency,
        })
    }

    pub fn set_backend(&mut self, backend: WgpuBackend) {
        self.backend = backend;
    }

    pub fn render_last_plan(&mut self) -> Result<RenderSurfaceStatus> {
        let egui = self.pending_egui.take();
        let screenshot = self.pending_screenshot.take();
        let Some(gpu) = &mut self.gpu else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };
        let Some(plan) = &self.last_plan else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };

        let (status, gpu_timings) = gpu.render_plan(
            plan,
            self.last_plan_canvas_policy,
            &self.fonts,
            &self.bitmap_fonts,
            egui.as_ref(),
            screenshot.as_ref(),
        )?;
        self.last_frame_timings = Some(RenderFrameTimings {
            draw_us: gpu_timings.draw_us,
            text_us: gpu_timings.text_us,
            geometry_us: gpu_timings.geometry_us,
            upload_us: gpu_timings.upload_us,
            submit_us: gpu_timings.submit_us,
            surface_us: gpu_timings.surface_us,
            bind_us: gpu_timings.bind_us,
            encode_us: gpu_timings.encode_us,
            queue_us: gpu_timings.queue_us,
            present_us: gpu_timings.present_us,
            steps: gpu_timings.steps,
            rect_steps: gpu_timings.rect_steps,
            image_steps: gpu_timings.image_steps,
            text_steps: gpu_timings.text_steps,
            rect_instances: gpu_timings.rect_instances,
            image_instances: gpu_timings.image_instances,
            text_instances: gpu_timings.text_instances,
            ..self.last_frame_timings.unwrap_or_default()
        });
        Ok(status)
    }

    pub fn request_screenshot(&mut self, path: impl Into<PathBuf>) {
        self.pending_screenshot =
            Some(ScreenshotRequest { path: path.into(), copy_to_clipboard: false });
    }

    pub fn request_screenshot_with_clipboard(&mut self, path: impl Into<PathBuf>) {
        self.pending_screenshot =
            Some(ScreenshotRequest { path: path.into(), copy_to_clipboard: true });
    }

    /// 次の描画フレームでスクリーンショットを撮る予定があるか。
    ///
    /// 撮影フレームではトースト等の一時 UI を隠す判定に使う。
    pub fn has_pending_screenshot(&self) -> bool {
        self.pending_screenshot.is_some()
    }

    pub fn flush_pending_screenshots(&mut self) -> Result<()> {
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };
        gpu.flush_pending_screenshots()
    }

    pub fn last_scene(&self) -> Option<&AppSceneSnapshot> {
        self.last_scene.as_ref()
    }

    pub fn last_plan(&self) -> Option<&DrawPlan> {
        self.last_plan.as_ref()
    }

    pub fn last_frame_timings(&self) -> Option<RenderFrameTimings> {
        self.last_frame_timings
    }

    fn select_skin_canvas_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let Some(surface) = self.gpu.as_ref().map(WgpuRenderer::surface_size) else {
            return Some((x, y));
        };
        let viewport =
            CanvasViewport::from_policy(surface, self.select_skin_canvas_render_policy());
        viewport.surface_to_canvas_point(x, y)
    }

    fn result_skin_canvas_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let Some(surface) = self.gpu.as_ref().map(WgpuRenderer::surface_size) else {
            return Some((x, y));
        };
        let viewport =
            CanvasViewport::from_policy(surface, self.result_skin_canvas_render_policy());
        viewport.surface_to_canvas_point(x, y)
    }

    fn canvas_policy_for_scene(&self, scene: &AppSceneSnapshot) -> CanvasRenderPolicy {
        match scene {
            AppSceneSnapshot::Select(_) => self.select_skin_canvas_render_policy(),
            AppSceneSnapshot::Decide(_) => self.decide_skin_canvas_render_policy(),
            AppSceneSnapshot::Play(_) => self.play_skin_canvas_render_policy(),
            AppSceneSnapshot::Result(_) => self.result_skin_canvas_render_policy(),
        }
    }

    fn select_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.select_skin_context
            .document()
            .filter(|document| document.skin_type == 5)
            .map(CanvasRenderPolicy::skin_document)
            .unwrap_or_default()
    }

    fn decide_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.decide_skin_context
            .document()
            .filter(|document| document.skin_type == 6)
            .map(CanvasRenderPolicy::skin_document)
            .unwrap_or_default()
    }

    fn play_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.play_skin_context.document().map(CanvasRenderPolicy::skin_document).unwrap_or_default()
    }

    fn result_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.result_skin_context
            .document()
            .filter(|document| matches!(document.skin_type, 7 | 15))
            .map(CanvasRenderPolicy::skin_document)
            .unwrap_or_default()
    }
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("last_scene", &self.last_scene)
            .field("last_plan", &self.last_plan)
            .field("font_count", &self.fonts.len())
            .field("bitmap_font_count", &self.bitmap_fonts.len())
            .field("gpu_attached", &self.gpu.is_some())
            .finish()
    }
}

impl WgpuRenderer {
    fn new_with_fallbacks<T>(
        window: T,
        size: SurfaceSize,
        present_mode: WgpuPresentMode,
        backend: WgpuBackend,
    ) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>> + Clone,
    {
        let candidates = fallback_wgpu_backends(backend);
        let mut last_error = None;
        for candidate in candidates {
            match Self::new(window.clone(), size, present_mode, *candidate) {
                Ok(renderer) => {
                    if backend == WgpuBackend::Auto && *candidate != WgpuBackend::Auto {
                        tracing::info!(backend = ?candidate, "selected auto renderer backend");
                    }
                    return Ok(renderer);
                }
                Err(error) => {
                    tracing::warn!(
                        requested = ?backend,
                        candidate = ?candidate,
                        %error,
                        "failed to initialize renderer backend candidate"
                    );
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("no renderer backend candidates available")))
    }

    fn new<T>(
        window: T,
        size: SurfaceSize,
        present_mode: WgpuPresentMode,
        backend: WgpuBackend,
    ) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>>,
    {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backend.to_wgpu();
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance.create_surface(window).context("failed to create wgpu surface")?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .context("no compatible GPU adapter found")?;
        // beatoraja スキンには 8192px を超える縦長/横長 PNG (背景アニメシート等) が
        // 含まれることがある。Apple Silicon / モダンGPU 環境では 16384px までは許容
        // されるので、アダプタが報告する上限まで広げて取得する。
        let adapter_limits = adapter.limits();
        let required_limits = wgpu::Limits {
            max_texture_dimension_2d: adapter_limits.max_texture_dimension_2d,
            max_texture_dimension_1d: adapter_limits.max_texture_dimension_1d,
            ..Default::default()
        };
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("bmz-render device"),
            required_features: wgpu::Features::empty(),
            required_limits,
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .context("failed to request wgpu device")?;
        let capabilities = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("surface is not supported by the selected adapter"))?;
        // sRGB フレームバッファだと PNG の sRGB 値が二重 gamma エンコードされて白っぽくなる。
        // beatoraja (libGDX) は GL_FRAMEBUFFER_SRGB を使わないため値をそのまま表示する。
        // それと合わせるため sRGB サフィックスを除去して non-sRGB サーフェスとして使う。
        config.format = config.format.remove_srgb_suffix();
        configure_surface_settings(&mut config, present_mode, &capabilities.present_modes);
        surface.configure(&device, &config);
        tracing::info!(
            requested = ?present_mode,
            effective = ?config.present_mode,
            available = ?capabilities.present_modes,
            maximum_frame_latency = config.desired_maximum_frame_latency,
            backend = ?backend,
            "configured renderer present mode"
        );
        let rect_pipeline = create_rect_pipeline(&device, config.format);
        let image_bind_group_layout = create_image_bind_group_layout(&device);
        let image_sampler = create_image_sampler(&device);
        let image_sampler_linear = create_image_sampler_linear(&device);
        let image_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::Normal,
        );
        let image_add_pipeline =
            create_image_pipeline(&device, config.format, &image_bind_group_layout, BlendMode::Add);
        let image_layer_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::LayerMask,
        );
        let image_textures = create_default_image_textures(&device, &queue);
        let text_bind_group_layout = create_text_bind_group_layout(&device);
        let text_sampler = create_text_sampler(&device);
        let text_pipeline = create_text_pipeline(&device, config.format, &text_bind_group_layout);
        let egui = EguiPainter::new(&device, config.format);

        Ok(Self {
            device,
            queue,
            config,
            present_modes: capabilities.present_modes,
            rect_pipeline,
            rect_buffer: None,
            rect_buffer_capacity: 0,
            image_pipeline,
            image_add_pipeline,
            image_layer_pipeline,
            image_bind_group_layout,
            image_sampler,
            image_sampler_linear,
            image_textures,
            image_bind_group_cache: HashMap::new(),
            image_bind_group_scratch: Vec::new(),
            geometry_scratch: PlanGeometry::default(),
            offscreen_rect_batches: HashMap::new(),
            next_offscreen_rect_batch_texture_id: OFFSCREEN_RECT_BATCH_TEXTURE_BASE,
            image_buffer: None,
            image_buffer_capacity: 0,
            text_pipeline,
            text_bind_group_layout,
            text_sampler,
            text_texture: None,
            text_texture_view: None,
            text_bind_group: None,
            text_texture_size: AtlasSize::default(),
            text_atlas: TextAtlasCache::new(TEXT_ATLAS_WIDTH),
            text_buffer: None,
            text_buffer_capacity: 0,
            font: load_default_font(),
            egui,
            pending_screenshot_readbacks: Vec::new(),
            screenshot_save_jobs: Vec::new(),
            surface,
        })
    }

    fn resize(&mut self, size: SurfaceSize) {
        if !size.is_drawable() {
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.clear_offscreen_rect_batches();
        self.configure_surface();
    }

    fn surface_size(&self) -> SurfaceSize {
        SurfaceSize { width: self.config.width, height: self.config.height }
    }

    fn poll_screenshot_work(&mut self) {
        if !self.pending_screenshot_readbacks.is_empty()
            && let Err(error) = self.device.poll(wgpu::PollType::Poll)
        {
            tracing::warn!(%error, "failed to poll screenshot readback work");
        }
        self.drain_ready_screenshot_readbacks();
        self.join_finished_screenshot_save_jobs();
    }

    fn enqueue_screenshot_readback(
        &mut self,
        request: ScreenshotRequest,
        capture: ScreenshotCapture,
    ) {
        let rx = capture.start_readback();
        tracing::debug!(
            path = %request.path.display(),
            width = capture.width,
            height = capture.height,
            "screenshot readback queued"
        );
        self.pending_screenshot_readbacks.push(ScreenshotReadback { request, capture, rx });
    }

    fn drain_ready_screenshot_readbacks(&mut self) {
        let mut index = 0;
        while index < self.pending_screenshot_readbacks.len() {
            match self.pending_screenshot_readbacks[index].rx.try_recv() {
                Ok(result) => {
                    let readback = self.pending_screenshot_readbacks.swap_remove(index);
                    self.finish_screenshot_readback(readback, result);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    index += 1;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    let readback = self.pending_screenshot_readbacks.swap_remove(index);
                    tracing::warn!(
                        path = %readback.request.path.display(),
                        "screenshot readback callback dropped"
                    );
                }
            }
        }
    }

    fn finish_screenshot_readback(
        &mut self,
        readback: ScreenshotReadback,
        result: Result<(), wgpu::BufferAsyncError>,
    ) {
        if let Err(error) = result {
            tracing::warn!(
                %error,
                path = %readback.request.path.display(),
                "failed to map screenshot buffer"
            );
            return;
        }

        let width = readback.capture.width;
        let height = readback.capture.height;
        let rgba = readback.capture.mapped_rgba();
        tracing::debug!(
            path = %readback.request.path.display(),
            width,
            height,
            "screenshot readback completed"
        );
        match spawn_screenshot_save_job(readback.request, width, height, rgba) {
            Ok(job) => self.screenshot_save_jobs.push(job),
            Err(error) => {
                tracing::warn!(%error, "failed to start screenshot save job");
            }
        }
    }

    fn join_finished_screenshot_save_jobs(&mut self) {
        let mut index = 0;
        while index < self.screenshot_save_jobs.len() {
            if self.screenshot_save_jobs[index].handle.is_finished() {
                let job = self.screenshot_save_jobs.swap_remove(index);
                finish_screenshot_save_job(job);
            } else {
                index += 1;
            }
        }
    }

    fn flush_pending_screenshots(&mut self) -> Result<()> {
        while !self.pending_screenshot_readbacks.is_empty() {
            self.device.poll(wgpu::PollType::wait_indefinitely())?;
            self.drain_ready_screenshot_readbacks();
        }
        while let Some(job) = self.screenshot_save_jobs.pop() {
            finish_screenshot_save_job(job);
        }
        Ok(())
    }

    fn wait_idle_before_drop(&self) {
        if let Err(error) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            tracing::warn!(%error, "failed to wait for renderer device before surface drop");
        }
    }

    fn render_plan(
        &mut self,
        plan: &DrawPlan,
        canvas_policy: CanvasRenderPolicy,
        fonts: &HashMap<String, FontArc>,
        bitmap_fonts: &HashMap<String, BitmapFont>,
        egui: Option<&EguiFrame>,
        screenshot_request: Option<&ScreenshotRequest>,
    ) -> Result<(RenderSurfaceStatus, GpuRenderTimings)> {
        let draw_start = Instant::now();
        let mut timings = GpuRenderTimings::default();
        self.poll_screenshot_work();
        // egui のテクスチャ更新は、描画をスキップするフレームでも必ず適用する。
        // TexturesDelta は累積ストリームのため、取りこぼすと後続フレームの
        // 部分更新が未確保テクスチャを参照して panic する。
        if let Some(frame) = egui {
            self.egui.update_textures(&self.device, &self.queue, frame);
        }

        let surface_size = SurfaceSize { width: self.config.width, height: self.config.height };
        if !surface_size.is_drawable() {
            timings.draw_us = draw_start.elapsed().as_micros();
            return Ok((RenderSurfaceStatus::SkippedZeroSize, timings));
        }
        let canvas_viewport = CanvasViewport::from_policy(surface_size, canvas_policy);

        let text_start = Instant::now();
        let mut text_frame =
            self.build_text_frame(plan, fonts, bitmap_fonts, canvas_viewport.content_size());
        if !canvas_viewport.is_identity() {
            canvas_viewport.transform_text_instances(&mut text_frame.instances);
            canvas_viewport.transform_text_caret_rects(&mut text_frame.command_caret_rects);
        }
        timings.text_us = text_start.elapsed().as_micros();
        let geometry_start = Instant::now();
        let mut geometry = std::mem::take(&mut self.geometry_scratch);
        encode_plan_geometry_into(
            plan,
            &text_frame,
            surface_size,
            canvas_viewport,
            &mut |rects, cache| self.offscreen_rect_batch_texture(rects, cache, surface_size),
            &mut geometry,
        );
        let geometry_stats = geometry.stats();
        timings.steps = geometry_stats.steps;
        timings.rect_steps = geometry_stats.rect_steps;
        timings.image_steps = geometry_stats.image_steps;
        timings.text_steps = geometry_stats.text_steps;
        timings.rect_instances = geometry_stats.rect_instances;
        timings.image_instances = geometry_stats.image_instances;
        timings.text_instances = geometry_stats.text_instances;
        timings.geometry_us = geometry_start.elapsed().as_micros();
        let upload_start = Instant::now();
        self.ensure_rect_buffer(geometry.rects.len());
        if let Some(buffer) = &self.rect_buffer
            && !geometry.rects.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &geometry.rects);
        }
        self.ensure_image_buffer(geometry.images.len());
        if let Some(buffer) = &self.image_buffer
            && !geometry.images.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &geometry.images);
        }
        self.upload_text_frame(&text_frame);
        timings.upload_us = upload_start.elapsed().as_micros();

        let submit_start = Instant::now();
        let surface_start = Instant::now();
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure_surface();
                timings.surface_us = surface_start.elapsed().as_micros();
                timings.submit_us = submit_start.elapsed().as_micros();
                timings.draw_us = draw_start.elapsed().as_micros();
                self.geometry_scratch = geometry;
                return Ok((RenderSurfaceStatus::Reconfigured, timings));
            }
            wgpu::CurrentSurfaceTexture::Timeout => {
                timings.surface_us = surface_start.elapsed().as_micros();
                timings.submit_us = submit_start.elapsed().as_micros();
                timings.draw_us = draw_start.elapsed().as_micros();
                self.geometry_scratch = geometry;
                return Ok((RenderSurfaceStatus::TimedOut, timings));
            }
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
                timings.surface_us = surface_start.elapsed().as_micros();
                timings.submit_us = submit_start.elapsed().as_micros();
                timings.draw_us = draw_start.elapsed().as_micros();
                self.geometry_scratch = geometry;
                return Ok((RenderSurfaceStatus::TimedOut, timings));
            }
        };
        timings.surface_us = surface_start.elapsed().as_micros();
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // image ステップごとの bind group を、レンダーパスが encoder を借りる前に作る。
        // steps 内の image ステップと同じ順序で並ぶ。
        let bind_start = Instant::now();
        let mut image_bind_groups = std::mem::take(&mut self.image_bind_group_scratch);
        image_bind_groups.clear();
        image_bind_groups.reserve(geometry_stats.image_steps);
        for step in &geometry.steps {
            if let DrawStep::Image { texture, linear, .. } = step {
                image_bind_groups.push(self.image_bind_group(*texture, *linear));
            }
        }
        let text_bind_group = self.text_bind_group();
        timings.bind_us = bind_start.elapsed().as_micros();
        let encode_start = Instant::now();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bmz-render clear encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bmz-render clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(plan.clear.to_wgpu()),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // DrawPlan.commands の順序を保ったまま rect / image / text を交互に描く。
            // これにより skin が画像をテキストより前面に置く演出も正しく重なる。
            let mut image_step_index = 0_usize;
            for step in &geometry.steps {
                match step {
                    DrawStep::Rects { range } => {
                        let Some(buffer) = &self.rect_buffer else {
                            continue;
                        };
                        let instance_count = (range.len() / RECT_INSTANCE_BYTES) as u32;
                        if instance_count == 0 {
                            continue;
                        }
                        pass.set_pipeline(&self.rect_pipeline);
                        pass.set_vertex_buffer(
                            0,
                            buffer.slice(range.start as u64..range.end as u64),
                        );
                        pass.draw(0..6, 0..instance_count);
                    }
                    DrawStep::Image { blend, range, .. } => {
                        let bind_group = &image_bind_groups[image_step_index];
                        image_step_index += 1;
                        let Some(buffer) = &self.image_buffer else {
                            continue;
                        };
                        let instance_count = (range.len() / IMAGE_INSTANCE_BYTES) as u32;
                        if instance_count == 0 {
                            continue;
                        }
                        pass.set_pipeline(match blend {
                            BlendMode::Normal => &self.image_pipeline,
                            BlendMode::Add => &self.image_add_pipeline,
                            BlendMode::LayerMask => &self.image_layer_pipeline,
                        });
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_vertex_buffer(
                            0,
                            buffer.slice(range.start as u64..range.end as u64),
                        );
                        pass.draw(0..6, 0..instance_count);
                    }
                    DrawStep::Text { range } => {
                        let (Some(bind_group), Some(buffer)) =
                            (&text_bind_group, &self.text_buffer)
                        else {
                            continue;
                        };
                        let instance_count = (range.len() / TEXT_INSTANCE_BYTES) as u32;
                        if instance_count == 0 {
                            continue;
                        }
                        pass.set_pipeline(&self.text_pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_vertex_buffer(
                            0,
                            buffer.slice(range.start as u64..range.end as u64),
                        );
                        pass.draw(0..6, 0..instance_count);
                    }
                }
            }
        }

        // ゲーム / スキン描画の上に egui を重ねる。staging 用 CommandBuffer は
        // egui パスを含む encoder より前に submit する必要がある。
        let egui_staging = match egui {
            Some(frame) => self.egui.paint(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                frame,
                [self.config.width, self.config.height],
            ),
            None => Vec::new(),
        };

        let screenshot = screenshot_request.map(|request| {
            let capture = ScreenshotCapture::new(
                &self.device,
                self.config.width,
                self.config.height,
                self.config.format,
            );
            capture.copy_from_surface(&mut encoder, &output.texture);
            (request.clone(), capture)
        });
        let command_buffer = encoder.finish();
        timings.encode_us = encode_start.elapsed().as_micros();
        let queue_start = Instant::now();
        self.queue.submit(egui_staging.into_iter().chain(std::iter::once(command_buffer)));
        timings.queue_us = queue_start.elapsed().as_micros();
        if let Some((request, capture)) = screenshot {
            self.enqueue_screenshot_readback(request, capture);
        }
        if let Some(frame) = egui {
            self.egui.free_textures(frame);
        }
        self.image_bind_group_scratch = image_bind_groups;
        self.geometry_scratch = geometry;
        let present_start = Instant::now();
        output.present();
        timings.present_us = present_start.elapsed().as_micros();
        timings.submit_us = submit_start.elapsed().as_micros();
        timings.draw_us = draw_start.elapsed().as_micros();
        Ok((RenderSurfaceStatus::Rendered, timings))
    }

    fn ensure_rect_buffer(&mut self, used_bytes: usize) {
        if used_bytes == 0 || used_bytes <= self.rect_buffer_capacity {
            return;
        }

        let capacity = used_bytes.next_power_of_two();
        self.rect_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render rect instance buffer"),
            size: capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.rect_buffer_capacity = capacity;
    }

    fn ensure_image_buffer(&mut self, used_bytes: usize) {
        if used_bytes == 0 || used_bytes <= self.image_buffer_capacity {
            return;
        }

        let capacity = used_bytes.next_power_of_two();
        self.image_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render image instance buffer"),
            size: capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.image_buffer_capacity = capacity;
    }

    fn offscreen_rect_batch_texture(
        &mut self,
        rects: &[RectCommand],
        cache: RectBatchCache,
        surface_size: SurfaceSize,
    ) -> Option<TextureId> {
        if rects.is_empty() || !surface_size.is_drawable() {
            return None;
        }
        let width = normalized_extent_to_pixels(cache.bounds.width, surface_size.width);
        let height = normalized_extent_to_pixels(cache.bounds.height, surface_size.height);
        if width == 0 || height == 0 {
            return None;
        }
        let key = OffscreenRectBatchTextureKey { key: cache.key, width, height };
        if let Some(texture) = self.offscreen_rect_batches.get(&key) {
            return Some(*texture);
        }
        if self.offscreen_rect_batches.len() >= OFFSCREEN_RECT_BATCH_TEXTURE_MAX_ENTRIES {
            self.clear_offscreen_rect_batches();
        }
        let texture_id = self.allocate_offscreen_rect_batch_texture_id();
        self.render_rect_batch_to_offscreen_texture(texture_id, rects, cache.bounds, width, height);
        self.offscreen_rect_batches.insert(key, texture_id);
        Some(texture_id)
    }

    fn allocate_offscreen_rect_batch_texture_id(&mut self) -> TextureId {
        let texture_id = TextureId(self.next_offscreen_rect_batch_texture_id);
        self.next_offscreen_rect_batch_texture_id = self
            .next_offscreen_rect_batch_texture_id
            .checked_add(1)
            .unwrap_or(OFFSCREEN_RECT_BATCH_TEXTURE_BASE);
        texture_id
    }

    fn render_rect_batch_to_offscreen_texture(
        &mut self,
        texture_id: TextureId,
        rects: &[RectCommand],
        bounds: Rect,
        width: u32,
        height: u32,
    ) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bmz-render offscreen rect batch"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let rect_bytes = encode_local_rect_batch(rects, bounds);
        if !rect_bytes.is_empty() {
            let rect_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bmz-render offscreen rect batch buffer"),
                size: rect_bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&rect_buffer, 0, &rect_bytes);
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bmz-render offscreen rect batch encoder"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bmz-render offscreen rect batch pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..6, 0..(rect_bytes.len() / RECT_INSTANCE_BYTES) as u32);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
        self.image_textures.insert(texture_id, PreparedTexture { texture, view, width, height });
        self.image_bind_group_cache
            .retain(|(cached_texture_id, _), _| *cached_texture_id != texture_id);
    }

    fn clear_offscreen_rect_batches(&mut self) {
        for texture_id in self.offscreen_rect_batches.drain().map(|(_, texture_id)| texture_id) {
            self.image_textures.remove(&texture_id);
            self.image_bind_group_cache
                .retain(|(cached_texture_id, _), _| *cached_texture_id != texture_id);
        }
        self.next_offscreen_rect_batch_texture_id = OFFSCREEN_RECT_BATCH_TEXTURE_BASE;
    }

    fn configure_surface(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    fn set_present_mode(&mut self, present_mode: WgpuPresentMode) {
        configure_surface_settings(&mut self.config, present_mode, &self.present_modes);
        self.configure_surface();
        tracing::info!(
            requested = ?present_mode,
            effective = ?self.config.present_mode,
            available = ?self.present_modes,
            maximum_frame_latency = self.config.desired_maximum_frame_latency,
            "configured renderer present mode"
        );
    }

    fn build_text_frame(
        &mut self,
        plan: &DrawPlan,
        fonts: &HashMap<String, FontArc>,
        bitmap_fonts: &HashMap<String, BitmapFont>,
        surface: SurfaceSize,
    ) -> TextFrame {
        if !surface.is_drawable() {
            return TextFrame::default();
        }
        let Some(default_font) = self.font.clone() else {
            return TextFrame::default();
        };
        build_text_frame_with_cache(
            plan,
            &default_font,
            fonts,
            bitmap_fonts,
            surface,
            &mut self.text_atlas,
        )
    }

    fn upload_text_frame(&mut self, frame: &TextFrame) {
        self.ensure_text_buffer(frame.instances.len());
        if let Some(buffer) = &self.text_buffer
            && !frame.instances.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &frame.instances);
        }

        if frame.size.width == 0 || frame.size.height == 0 {
            return;
        }

        let recreate_texture = self.text_texture_size != frame.size || self.text_texture.is_none();
        self.ensure_text_texture(frame.size);
        let texture = self.text_texture.as_ref().expect("text texture exists after ensure");
        if recreate_texture {
            let pixels = self.text_atlas.pixels_for_size(frame.size);
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(frame.size.width * 4),
                    rows_per_image: Some(frame.size.height),
                },
                wgpu::Extent3d {
                    width: frame.size.width,
                    height: frame.size.height,
                    depth_or_array_layers: 1,
                },
            );
            return;
        }

        for region in &frame.dirty_regions {
            if region.pixels.is_empty() || region.size.width == 0 || region.size.height == 0 {
                continue;
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: region.origin.0, y: region.origin.1, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &region.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(region.size.width * 4),
                    rows_per_image: Some(region.size.height),
                },
                wgpu::Extent3d {
                    width: region.size.width,
                    height: region.size.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn reset_text_atlas(&mut self) {
        self.text_atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        self.text_texture = None;
        self.text_texture_view = None;
        self.text_bind_group = None;
        self.text_texture_size = AtlasSize::default();
    }

    fn ensure_text_buffer(&mut self, used_bytes: usize) {
        if used_bytes == 0 || used_bytes <= self.text_buffer_capacity {
            return;
        }

        let capacity = used_bytes.next_power_of_two();
        self.text_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render text instance buffer"),
            size: capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.text_buffer_capacity = capacity;
    }

    fn ensure_text_texture(&mut self, size: AtlasSize) {
        if self.text_texture_size == size && self.text_texture.is_some() {
            return;
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bmz-render text atlas"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.text_texture = Some(texture);
        self.text_texture_view = Some(view);
        self.text_bind_group = None;
        self.text_texture_size = size;
    }

    fn text_bind_group(&mut self) -> Option<wgpu::BindGroup> {
        if let Some(bind_group) = &self.text_bind_group {
            return Some(bind_group.clone());
        }
        let view = self.text_texture_view.as_ref()?;
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bmz-render text bind group"),
            layout: &self.text_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.text_sampler),
                },
            ],
        });
        self.text_bind_group = Some(bind_group.clone());
        Some(bind_group)
    }

    fn upsert_rgba_texture(&mut self, id: TextureId, width: u32, height: u32, rgba: &[u8]) {
        if let Some(texture) = self.image_textures.get(&id)
            && texture.width == width
            && texture.height == height
        {
            write_rgba_texture(&self.queue, &texture.texture, width, height, rgba);
            return;
        }
        let texture = create_rgba_texture(&self.device, &self.queue, id, width, height, rgba);
        self.image_textures.insert(id, texture);
        self.image_bind_group_cache.retain(|(texture_id, _), _| *texture_id != id);
    }

    fn image_bind_group(&mut self, texture_id: TextureId, linear: bool) -> wgpu::BindGroup {
        let resolved_texture_id =
            if self.image_textures.contains_key(&texture_id) { texture_id } else { TextureId(0) };
        if let Some(bind_group) =
            self.image_bind_group_cache.get(&(resolved_texture_id, linear)).cloned()
        {
            return bind_group;
        }
        let texture =
            self.image_textures.get(&resolved_texture_id).expect("fallback texture is registered");
        let _keep_texture_alive = &texture.texture;
        let sampler = if linear { &self.image_sampler_linear } else { &self.image_sampler };
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bmz-render image bind group"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        self.image_bind_group_cache.insert((resolved_texture_id, linear), bind_group.clone());
        bind_group
    }
}

const RECT_INSTANCE_FLOATS: usize = 8;
const RECT_INSTANCE_BYTES: usize = RECT_INSTANCE_FLOATS * std::mem::size_of::<f32>();
const IMAGE_INSTANCE_FLOATS: usize = 16;
const IMAGE_INSTANCE_BYTES: usize = IMAGE_INSTANCE_FLOATS * std::mem::size_of::<f32>();
const TEXT_INSTANCE_FLOATS: usize = 12;
const TEXT_INSTANCE_BYTES: usize = TEXT_INSTANCE_FLOATS * std::mem::size_of::<f32>();
const TEXT_ATLAS_WIDTH: u32 = 1024;
const TEXT_ATLAS_PADDING: u32 = 1;
const VECTOR_TEXT_SUPERSAMPLE_SCALE: f32 = 2.0;
/// グリフは永続キャッシュされるため、選曲画面のスクロールなどで文字種が増え続けると
/// アトラス高さが単調増加する。wgpu の `max_texture_dimension_2d` (一般に 16384) を
/// 超えると `create_texture` がパニックするので、上限に達したらフレーム境界でキャッシュを
/// 捨てて作り直す。1 フレーム分のグリフは十分この高さに収まるため、リセットしても破綻しない。
const TEXT_ATLAS_MAX_HEIGHT: u32 = 8192;
const TEXT_LAYOUT_CACHE_MAX_ENTRIES: usize = 4096;

fn normalized_extent_to_pixels(normalized: f32, surface_extent: u32) -> u32 {
    if normalized <= f32::EPSILON || surface_extent == 0 {
        return 0;
    }
    (normalized * surface_extent as f32).ceil().clamp(1.0, surface_extent.max(1) as f32) as u32
}

fn encode_local_rect_batch(rects: &[RectCommand], bounds: Rect) -> Vec<u8> {
    if bounds.width <= f32::EPSILON || bounds.height <= f32::EPSILON {
        return Vec::new();
    }
    let mut bytes = Vec::with_capacity(rects.len() * RECT_INSTANCE_BYTES);
    for command in rects {
        let rect = command.rect;
        let color = command.color;
        let local = Rect {
            x: (rect.x - bounds.x) / bounds.width,
            y: (rect.y - bounds.y) / bounds.height,
            width: rect.width / bounds.width,
            height: rect.height / bounds.height,
        };
        bytes.extend_from_slice(bytemuck::bytes_of(&[
            local.x,
            local.y,
            local.width,
            local.height,
            color.r,
            color.g,
            color.b,
            color.a,
        ]));
    }
    bytes
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AtlasSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Default)]
struct TextFrame {
    size: AtlasSize,
    #[cfg(test)]
    pixels: Vec<u8>,
    dirty_regions: Vec<TextAtlasDirtyRegion>,
    instances: Vec<u8>,
    /// `DrawCommand::Text` ごとに生成された quad 数を、commands 内の出現順で持つ。
    /// 描画ステップ単位で text instance buffer をスライスするのに使う。
    command_quad_counts: Vec<usize>,
    /// `DrawCommand::Text` ごとに生成された caret 矩形。
    command_caret_rects: Vec<Option<RectCommand>>,
}

#[derive(Debug, Clone)]
struct TextAtlasDirtyRegion {
    origin: (u32, u32),
    size: AtlasSize,
    pixels: Vec<u8>,
}

/// `DrawPlan.commands` の順序を保ったまま 1 レンダーパスで描くための描画ステップ。
/// 連続する同種コマンドは 1 ステップにまとめる。image は texture/blend/linear が
/// 変わるか、別種コマンドを挟むたびに分割する。
#[derive(Debug, Clone, PartialEq)]
enum DrawStep {
    /// rect instance buffer 内のバイト範囲。
    Rects { range: Range<usize> },
    /// image instance buffer 内のバイト範囲。
    Image { texture: TextureId, blend: BlendMode, linear: bool, range: Range<usize> },
    /// text instance buffer 内のバイト範囲。atlas テクスチャは全 text で共有する。
    Text { range: Range<usize> },
}

/// `DrawPlan` を GPU 描画用のバッファ列と順序付きステップ列へ変換した結果。
#[derive(Default)]
struct PlanGeometry {
    rects: Vec<u8>,
    images: Vec<u8>,
    steps: Vec<DrawStep>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DrawStepStats {
    steps: usize,
    rect_steps: usize,
    image_steps: usize,
    text_steps: usize,
    rect_instances: usize,
    image_instances: usize,
    text_instances: usize,
}

impl PlanGeometry {
    fn stats(&self) -> DrawStepStats {
        let mut stats = DrawStepStats {
            steps: self.steps.len(),
            rect_instances: self.rects.len() / RECT_INSTANCE_BYTES,
            image_instances: self.images.len() / IMAGE_INSTANCE_BYTES,
            ..Default::default()
        };
        for step in &self.steps {
            match step {
                DrawStep::Rects { .. } => {
                    stats.rect_steps += 1;
                }
                DrawStep::Image { .. } => {
                    stats.image_steps += 1;
                }
                DrawStep::Text { range } => {
                    stats.text_steps += 1;
                    stats.text_instances += range.len() / TEXT_INSTANCE_BYTES;
                }
            }
        }
        stats
    }
}

/// commands を 1 回走査し、rect/image インスタンスバッファと、コマンド順を尊重した
/// 描画ステップ列を作る。`text_frame` の `command_quad_counts` から各 Text コマンドが
/// 占める text instance buffer の範囲を割り出す。
#[cfg(test)]
fn encode_plan_geometry(
    plan: &DrawPlan,
    text_frame: &TextFrame,
    surface_size: SurfaceSize,
) -> PlanGeometry {
    let viewport = CanvasViewport::from_policy(surface_size, CanvasRenderPolicy::default());
    let mut geometry = PlanGeometry::default();
    encode_plan_geometry_into(
        plan,
        text_frame,
        surface_size,
        viewport,
        &mut |_, _| None,
        &mut geometry,
    );
    geometry
}

#[cfg(test)]
fn encode_plan_geometry_with_rect_batch_resolver(
    plan: &DrawPlan,
    text_frame: &TextFrame,
    surface_size: SurfaceSize,
    canvas_viewport: CanvasViewport,
    resolve_rect_batch_texture: &mut impl FnMut(&[RectCommand], RectBatchCache) -> Option<TextureId>,
) -> PlanGeometry {
    let mut geometry = PlanGeometry::default();
    encode_plan_geometry_into(
        plan,
        text_frame,
        surface_size,
        canvas_viewport,
        resolve_rect_batch_texture,
        &mut geometry,
    );
    geometry
}

fn encode_plan_geometry_into(
    plan: &DrawPlan,
    text_frame: &TextFrame,
    surface_size: SurfaceSize,
    canvas_viewport: CanvasViewport,
    resolve_rect_batch_texture: &mut impl FnMut(&[RectCommand], RectBatchCache) -> Option<TextureId>,
    geometry: &mut PlanGeometry,
) {
    let command_count = plan.commands.len();
    geometry.rects.clear();
    geometry.images.clear();
    geometry.steps.clear();
    geometry.rects.reserve(command_count.saturating_mul(RECT_INSTANCE_BYTES));
    geometry.images.reserve(command_count.saturating_mul(IMAGE_INSTANCE_BYTES));
    geometry.steps.reserve(command_count);
    let rects = &mut geometry.rects;
    let images = &mut geometry.images;
    let steps = &mut geometry.steps;
    let image_rotation_aspect = if surface_size.height == 0 {
        1.0
    } else {
        surface_size.width as f32 / surface_size.height as f32
    };
    // text instance buffer 上での現在位置 (quad 単位) と、次に参照する Text コマンド番号。
    let mut text_quad_cursor = 0_usize;
    let mut text_command_index = 0_usize;

    for command in &plan.commands {
        match command {
            DrawCommand::Rect { rect, color } => {
                let start = rects.len();
                let rect = canvas_viewport.transform_rect(*rect);
                if !visible_rect(rect) || !visible_alpha(color.a) {
                    continue;
                }
                rects.extend_from_slice(bytemuck::bytes_of(&[
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    color.r,
                    color.g,
                    color.b,
                    color.a,
                ]));
                push_or_extend_rects(steps, start..rects.len());
            }
            DrawCommand::RectBatch { rects: batch, cache } => {
                let transformed_batch: Vec<_> = batch
                    .iter()
                    .map(|command| canvas_viewport.transform_rect_command(*command))
                    .filter(|command| visible_rect(command.rect) && visible_alpha(command.color.a))
                    .collect();
                if transformed_batch.is_empty() {
                    continue;
                }
                if let Some(cache) = *cache
                    && let Some(texture) = resolve_rect_batch_texture(
                        &transformed_batch,
                        canvas_viewport.transform_rect_batch_cache(cache),
                    )
                {
                    let start = images.len();
                    let bounds = canvas_viewport.transform_rect(cache.bounds);
                    encode_image_instance(
                        images,
                        &bounds,
                        &UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                        &Color::rgb(1.0, 1.0, 1.0),
                        0.0,
                        Point { x: 0.5, y: 0.5 },
                        image_rotation_aspect,
                    );
                    push_or_extend_image(
                        steps,
                        texture,
                        BlendMode::Normal,
                        false,
                        start..images.len(),
                    );
                } else {
                    let start = rects.len();
                    for command in transformed_batch.iter() {
                        let rect = command.rect;
                        let color = command.color;
                        rects.extend_from_slice(bytemuck::bytes_of(&[
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            color.r,
                            color.g,
                            color.b,
                            color.a,
                        ]));
                    }
                    if rects.len() > start {
                        push_or_extend_rects(steps, start..rects.len());
                    }
                }
            }
            DrawCommand::Image { rect, uv, source_size, texture, tint, blend, linear_filter } => {
                let start = images.len();
                let rect = canvas_viewport.transform_rect(*rect);
                if !visible_rect(rect) || !visible_alpha(tint.a) {
                    continue;
                }
                let sampling_uv = sampling_uv_with_half_texel_inset(*uv, *source_size);
                encode_image_instance(
                    images,
                    &rect,
                    &sampling_uv,
                    tint,
                    0.0,
                    Point { x: 0.5, y: 0.5 },
                    image_rotation_aspect,
                );
                push_or_extend_image(steps, *texture, *blend, *linear_filter, start..images.len());
            }
            DrawCommand::RotatedImage {
                rect,
                uv,
                source_size,
                texture,
                tint,
                blend,
                linear_filter,
                angle_rad,
                center,
            } => {
                let start = images.len();
                let rect = canvas_viewport.transform_rect(*rect);
                if !visible_rect(rect) || !visible_alpha(tint.a) {
                    continue;
                }
                let sampling_uv = sampling_uv_with_half_texel_inset(*uv, *source_size);
                encode_image_instance(
                    images,
                    &rect,
                    &sampling_uv,
                    tint,
                    *angle_rad,
                    *center,
                    image_rotation_aspect,
                );
                push_or_extend_image(steps, *texture, *blend, *linear_filter, start..images.len());
            }
            DrawCommand::Text { .. } => {
                let quad_count =
                    text_frame.command_quad_counts.get(text_command_index).copied().unwrap_or(0);
                let caret_rect =
                    text_frame.command_caret_rects.get(text_command_index).copied().flatten();
                text_command_index += 1;
                let start = text_quad_cursor * TEXT_INSTANCE_BYTES;
                text_quad_cursor += quad_count;
                let end = text_quad_cursor * TEXT_INSTANCE_BYTES;
                if quad_count > 0 {
                    push_or_extend_text(steps, start..end);
                }
                if let Some(command) = caret_rect {
                    let start = rects.len();
                    let rect = command.rect;
                    let color = command.color;
                    if !visible_rect(rect) || !visible_alpha(color.a) {
                        continue;
                    }
                    rects.extend_from_slice(bytemuck::bytes_of(&[
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        color.r,
                        color.g,
                        color.b,
                        color.a,
                    ]));
                    push_or_extend_rects(steps, start..rects.len());
                }
            }
        }
    }
}

fn visible_rect(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width.abs() > f32::EPSILON
        && rect.height.abs() > f32::EPSILON
}

fn visible_alpha(alpha: f32) -> bool {
    alpha.is_finite() && alpha > 0.0
}

fn sampling_uv_with_half_texel_inset(uv: UvRect, source_size: Option<SkinImageSize>) -> UvRect {
    let Some(source_size) = source_size else {
        return uv;
    };
    if !source_size.width.is_finite()
        || !source_size.height.is_finite()
        || source_size.width <= 0.0
        || source_size.height <= 0.0
    {
        return uv;
    }

    let (x, width) = if uv_axis_covers_full_texture(uv.x, uv.width) {
        (uv.x, uv.width)
    } else {
        inset_uv_axis_by_half_texel(uv.x, uv.width, source_size.width)
    };
    let (y, height) = if uv_axis_covers_full_texture(uv.y, uv.height) {
        (uv.y, uv.height)
    } else {
        inset_uv_axis_by_half_texel(uv.y, uv.height, source_size.height)
    };

    UvRect { x, y, width, height }
}

fn uv_axis_covers_full_texture(origin: f32, extent: f32) -> bool {
    const EPSILON: f32 = 1.0e-6;
    origin.abs() <= EPSILON && (extent - 1.0).abs() <= EPSILON
}

fn inset_uv_axis_by_half_texel(origin: f32, extent: f32, source_extent: f32) -> (f32, f32) {
    if !origin.is_finite()
        || !extent.is_finite()
        || !source_extent.is_finite()
        || source_extent <= 1.0
    {
        return (origin, extent);
    }

    let texel = 1.0 / source_extent;
    let half_texel = texel * 0.5;
    if extent > texel {
        (origin + half_texel, extent - texel)
    } else if extent < -texel {
        (origin - half_texel, extent + texel)
    } else {
        (origin, extent)
    }
}

/// image インスタンス 1 件 (16 float) をバッファ末尾へ書き込む。
fn encode_image_instance(
    images: &mut Vec<u8>,
    rect: &Rect,
    uv: &UvRect,
    tint: &Color,
    angle_rad: f32,
    center: Point,
    rotation_aspect: f32,
) {
    images.extend_from_slice(bytemuck::bytes_of(&[
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        uv.x,
        uv.y,
        uv.width,
        uv.height,
        tint.r,
        tint.g,
        tint.b,
        tint.a,
        angle_rad,
        center.x,
        center.y,
        rotation_aspect,
    ]));
}

fn push_or_extend_rects(steps: &mut Vec<DrawStep>, range: Range<usize>) {
    match steps.last_mut() {
        Some(DrawStep::Rects { range: existing }) => existing.end = range.end,
        _ => steps.push(DrawStep::Rects { range }),
    }
}

fn push_or_extend_text(steps: &mut Vec<DrawStep>, range: Range<usize>) {
    match steps.last_mut() {
        Some(DrawStep::Text { range: existing }) => existing.end = range.end,
        _ => steps.push(DrawStep::Text { range }),
    }
}

fn push_or_extend_image(
    steps: &mut Vec<DrawStep>,
    texture: TextureId,
    blend: BlendMode,
    linear: bool,
    range: Range<usize>,
) {
    if let Some(DrawStep::Image { texture: t, blend: b, linear: l, range: existing }) =
        steps.last_mut()
        && *t == texture
        && *b == blend
        && *l == linear
    {
        existing.end = range.end;
        return;
    }
    steps.push(DrawStep::Image { texture, blend, linear, range });
}

#[cfg(test)]
fn build_text_frame(
    plan: &DrawPlan,
    default_font: &FontArc,
    fonts: &HashMap<String, FontArc>,
    bitmap_fonts: &HashMap<String, BitmapFont>,
    surface: SurfaceSize,
) -> TextFrame {
    if !surface.is_drawable() {
        return TextFrame::default();
    }

    let mut builder = TextAtlasBuilder::new(TEXT_ATLAS_WIDTH);
    // 各 Text コマンドが生成した quad 数を記録し、描画ステップへ分割できるようにする。
    let mut command_quad_counts = Vec::new();
    let mut command_caret_rects = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Text { origin, text, style, caret } = command else {
            continue;
        };
        let quads_before = builder.quads.len();
        if let Some(bitmap_font) =
            style.font_id.as_ref().and_then(|font_id| bitmap_fonts.get(font_id))
        {
            builder.push_bitmap_text(origin, text, style.clone(), bitmap_font, surface);
            command_caret_rects.push(caret.and_then(|caret| {
                bitmap_text_caret_rect(origin, text, style, bitmap_font, surface, caret)
            }));
        } else {
            let font = style
                .font_id
                .as_ref()
                .and_then(|font_id| fonts.get(font_id))
                .unwrap_or(default_font);
            builder.push_text(origin, text, style.clone(), font, surface);
            command_caret_rects.push(caret.and_then(|caret| {
                vector_text_caret_rect(origin, text, style, font, surface, caret)
            }));
        }
        command_quad_counts.push(builder.quads.len() - quads_before);
    }
    let mut frame = builder.finish();
    frame.command_quad_counts = command_quad_counts;
    frame.command_caret_rects = command_caret_rects;
    frame
}

fn build_text_frame_with_cache(
    plan: &DrawPlan,
    default_font: &FontArc,
    fonts: &HashMap<String, FontArc>,
    bitmap_fonts: &HashMap<String, BitmapFont>,
    surface: SurfaceSize,
    atlas: &mut TextAtlasCache,
) -> TextFrame {
    if !surface.is_drawable() {
        return TextFrame::default();
    }

    atlas.begin_frame();
    let mut builder = CachedTextFrameBuilder::new(atlas);
    let mut command_quad_counts = Vec::new();
    let mut command_caret_rects = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Text { origin, text, style, caret } = command else {
            continue;
        };
        let quads_before = builder.quads.len();
        if let Some(font_id) = style.font_id.as_deref()
            && let Some(bitmap_font) = bitmap_fonts.get(font_id)
        {
            builder.push_bitmap_text(origin, text, style.clone(), font_id, bitmap_font, surface);
            command_caret_rects.push(caret.and_then(|caret| {
                bitmap_text_caret_rect(origin, text, style, bitmap_font, surface, caret)
            }));
        } else {
            let font_id = style.font_id.as_deref().unwrap_or(DEFAULT_TEXT_FONT_ID);
            let font = style
                .font_id
                .as_ref()
                .and_then(|font_id| fonts.get(font_id))
                .unwrap_or(default_font);
            builder.push_text(origin, text, style.clone(), font_id, font, surface);
            command_caret_rects.push(caret.and_then(|caret| {
                vector_text_caret_rect(origin, text, style, font, surface, caret)
            }));
        }
        command_quad_counts.push(builder.quads.len() - quads_before);
    }
    builder.finish(command_quad_counts, command_caret_rects)
}

const DEFAULT_TEXT_FONT_ID: &str = "<default>";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextGlyphKey {
    kind: TextGlyphKind,
    font_id: String,
    ch: char,
    scale_bits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum TextGlyphKind {
    Vector,
    Bitmap,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextLayoutKey {
    kind: TextGlyphKind,
    font_id: String,
    text: String,
    origin_x_bits: u32,
    origin_y_bits: u32,
    surface_width: u32,
    surface_height: u32,
    style: TextLayoutStyleKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextLayoutStyleKey {
    size_bits: u32,
    bitmap_size_bits: Option<u32>,
    color: ColorKey,
    align: u8,
    max_width_bits: u32,
    overflow: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ColorKey {
    r_bits: u32,
    g_bits: u32,
    b_bits: u32,
    a_bits: u32,
}

#[derive(Debug, Clone)]
struct CachedGlyph {
    atlas_origin: (u32, u32),
    width: u32,
    height: u32,
    display_width: f32,
    display_height: f32,
    offset_x: f32,
    offset_y: f32,
}

#[derive(Debug)]
struct TextLayoutCache {
    entries: HashMap<TextLayoutKey, Vec<TextQuad>>,
}

impl TextLayoutCache {
    fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn cached(&self, key: &TextLayoutKey) -> Option<Vec<TextQuad>> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: TextLayoutKey, quads: &[TextQuad]) {
        if self.entries.len() >= TEXT_LAYOUT_CACHE_MAX_ENTRIES {
            self.entries.clear();
        }
        self.entries.insert(key, quads.to_vec());
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug)]
struct TextAtlasCache {
    width: u32,
    pen_x: u32,
    pen_y: u32,
    row_height: u32,
    pixels: Vec<u8>,
    glyphs: HashMap<TextGlyphKey, CachedGlyph>,
    layouts: TextLayoutCache,
    dirty_regions: Vec<TextAtlasDirtyRegion>,
}

impl TextAtlasCache {
    fn new(width: u32) -> Self {
        Self {
            width,
            pen_x: 0,
            pen_y: 0,
            row_height: 0,
            pixels: Vec::new(),
            glyphs: HashMap::new(),
            layouts: TextLayoutCache::new(),
            dirty_regions: Vec::new(),
        }
    }

    fn begin_frame(&mut self) {
        self.dirty_regions.clear();
        // アトラス高さが上限に達したらキャッシュを捨てて作り直す。フレーム描画前に
        // 行うので、このフレームのグリフは新しいアトラスへ再ラスタライズされる。
        if self.atlas_height() >= TEXT_ATLAS_MAX_HEIGHT {
            self.clear();
        }
    }

    fn clear(&mut self) {
        self.pen_x = 0;
        self.pen_y = 0;
        self.row_height = 0;
        self.pixels.clear();
        self.glyphs.clear();
        self.layouts.clear();
        self.dirty_regions.clear();
    }

    fn atlas_height(&self) -> u32 {
        (self.pen_y + self.row_height).max(1)
    }

    fn size(&self) -> AtlasSize {
        AtlasSize { width: self.width, height: self.atlas_height() }
    }

    fn pixels_for_size(&self, size: AtlasSize) -> Vec<u8> {
        let mut pixels = self.pixels.clone();
        pixels.resize((size.width * size.height * 4) as usize, 0);
        pixels
    }

    fn cached_layout(&self, key: &TextLayoutKey) -> Option<Vec<TextQuad>> {
        self.layouts.cached(key)
    }

    fn insert_layout(&mut self, key: TextLayoutKey, quads: &[TextQuad]) {
        self.layouts.insert(key, quads);
    }

    fn cached_vector_glyph(
        &mut self,
        font_id: &str,
        ch: char,
        scale: PxScale,
        font: &FontArc,
    ) -> Option<CachedGlyph> {
        let key = TextGlyphKey {
            kind: TextGlyphKind::Vector,
            font_id: font_id.to_string(),
            ch,
            scale_bits: scale.x.to_bits(),
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return Some(glyph.clone());
        }

        let raster_scale = PxScale {
            x: scale.x * VECTOR_TEXT_SUPERSAMPLE_SCALE,
            y: scale.y * VECTOR_TEXT_SUPERSAMPLE_SCALE,
        };
        let scaled_font = font.as_scaled(raster_scale);
        let baseline_y = scaled_font.ascent();
        let glyph =
            Glyph { id: font.glyph_id(ch), scale: raster_scale, position: point(0.0, baseline_y) };
        let outlined = font.outline_glyph(glyph)?;
        let bounds = outlined.px_bounds();
        let width = bounds.width().ceil().max(0.0) as u32;
        let height = bounds.height().ceil().max(0.0) as u32;
        if width == 0 || height == 0 {
            return None;
        }

        let mut pixels = vec![0; (width * height * 4) as usize];
        outlined.draw(|x, y, coverage| {
            let index = ((y * width + x) * 4) as usize;
            let coverage_u8 = (coverage * 255.0).clamp(0.0, 255.0) as u8;
            if let Some(pixel) = pixels.get_mut(index..index + 4)
                && coverage_u8 >= pixel[3]
            {
                pixel[0] = 255;
                pixel[1] = 255;
                pixel[2] = 255;
                pixel[3] = coverage_u8;
            }
        });

        Some(self.insert_glyph_pixels(
            key,
            width,
            height,
            width as f32 / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            height as f32 / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            bounds.min.x / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            (bounds.min.y - baseline_y) / VECTOR_TEXT_SUPERSAMPLE_SCALE,
            pixels,
        ))
    }

    fn cached_bitmap_glyph(
        &mut self,
        font_id: &str,
        ch: char,
        glyph: crate::bitmap_font::BitmapFontGlyph,
        page: &crate::bitmap_font::BitmapFontPage,
        font: &BitmapFont,
        scale: f32,
    ) -> CachedGlyph {
        let key = TextGlyphKey {
            kind: TextGlyphKind::Bitmap,
            font_id: font_id.to_string(),
            ch,
            scale_bits: scale.to_bits(),
        };
        if let Some(glyph) = self.glyphs.get(&key) {
            return glyph.clone();
        }

        let width = (glyph.width as f32 * scale).ceil().max(1.0) as u32;
        let height = (glyph.height as f32 * scale).ceil().max(1.0) as u32;
        let pixels = rasterized_bitmap_glyph_pixels(glyph, page, scale, width, height);

        self.insert_glyph_pixels(
            key,
            width,
            height,
            width as f32,
            height as f32,
            glyph.xoffset as f32 * scale,
            (glyph.yoffset as f32 - font.ascent) * scale,
            pixels,
        )
    }

    fn insert_glyph_pixels(
        &mut self,
        key: TextGlyphKey,
        width: u32,
        height: u32,
        display_width: f32,
        display_height: f32,
        offset_x: f32,
        offset_y: f32,
        pixels: Vec<u8>,
    ) -> CachedGlyph {
        let atlas_origin = self.reserve(width, height);
        self.blit_region(atlas_origin, width, height, &pixels);
        self.dirty_regions.push(TextAtlasDirtyRegion {
            origin: atlas_origin,
            size: AtlasSize { width, height },
            pixels,
        });
        let cached = CachedGlyph {
            atlas_origin,
            width,
            height,
            display_width,
            display_height,
            offset_x,
            offset_y,
        };
        self.glyphs.insert(key, cached.clone());
        cached
    }

    fn reserve(&mut self, glyph_width: u32, glyph_height: u32) -> (u32, u32) {
        let padded_width = glyph_width + TEXT_ATLAS_PADDING * 2;
        let padded_height = glyph_height + TEXT_ATLAS_PADDING * 2;
        if self.pen_x + padded_width > self.width {
            self.pen_x = 0;
            self.pen_y += self.row_height;
            self.row_height = 0;
        }

        let origin = (self.pen_x + TEXT_ATLAS_PADDING, self.pen_y + TEXT_ATLAS_PADDING);
        self.pen_x += padded_width;
        self.row_height = self.row_height.max(padded_height);
        self.ensure_height(self.pen_y + self.row_height);
        origin
    }

    fn ensure_height(&mut self, height: u32) {
        let needed = (self.width * height * 4) as usize;
        if self.pixels.len() < needed {
            self.pixels.resize(needed, 0);
        }
    }

    fn blit_region(&mut self, atlas_origin: (u32, u32), width: u32, height: u32, pixels: &[u8]) {
        for y in 0..height {
            let dst_start = (((atlas_origin.1 + y) * self.width + atlas_origin.0) * 4) as usize;
            let src_start = (y * width * 4) as usize;
            let len = (width * 4) as usize;
            if let (Some(dst), Some(src)) = (
                self.pixels.get_mut(dst_start..dst_start + len),
                pixels.get(src_start..src_start + len),
            ) {
                dst.copy_from_slice(src);
            }
        }
    }
}

fn text_layout_key(
    kind: TextGlyphKind,
    font_id: &str,
    origin: &Point,
    text: &str,
    style: &TextStyle,
    surface: SurfaceSize,
) -> Option<TextLayoutKey> {
    if style.wrapping || style.outline.is_some() || style.shadow.is_some() {
        return None;
    }
    Some(TextLayoutKey {
        kind,
        font_id: font_id.to_string(),
        text: text.to_string(),
        origin_x_bits: origin.x.to_bits(),
        origin_y_bits: origin.y.to_bits(),
        surface_width: surface.width,
        surface_height: surface.height,
        style: TextLayoutStyleKey {
            size_bits: style.size.to_bits(),
            bitmap_size_bits: style.bitmap_size.map(f32::to_bits),
            color: ColorKey {
                r_bits: style.color.r.to_bits(),
                g_bits: style.color.g.to_bits(),
                b_bits: style.color.b.to_bits(),
                a_bits: style.color.a.to_bits(),
            },
            align: text_align_key(style.align),
            max_width_bits: style.max_width.to_bits(),
            overflow: text_overflow_key(style.overflow),
        },
    })
}

fn text_align_key(align: TextAlign) -> u8 {
    match align {
        TextAlign::Left => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    }
}

fn text_overflow_key(overflow: TextOverflow) -> u8 {
    match overflow {
        TextOverflow::Overflow => 0,
        TextOverflow::Shrink => 1,
        TextOverflow::Truncate => 2,
    }
}

struct CachedTextFrameBuilder<'a> {
    atlas: &'a mut TextAtlasCache,
    quads: Vec<TextQuad>,
}

impl<'a> CachedTextFrameBuilder<'a> {
    fn new(atlas: &'a mut TextAtlasCache) -> Self {
        Self { atlas, quads: Vec::new() }
    }

    fn push_bitmap_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        font_id: &str,
        font: &BitmapFont,
        surface: SurfaceSize,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style.clone();
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_bitmap_text(&shadow_origin, text, shadow_style, font_id, font, surface);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style.clone();
            outline_style.color = outline.color;
            outline_style.outline = None;
            outline_style.shadow = None;
            let outline_y = outline.width;
            let outline_x = outline.width * surface.height as f32 / surface.width as f32;
            for (offset_x, offset_y) in [
                (-outline_x, -outline_y),
                (0.0, -outline_y),
                (outline_x, -outline_y),
                (-outline_x, 0.0),
                (outline_x, 0.0),
                (-outline_x, outline_y),
                (0.0, outline_y),
                (outline_x, outline_y),
            ] {
                let outline_origin = Point { x: origin.x + offset_x, y: origin.y + offset_y };
                self.push_bitmap_text(
                    &outline_origin,
                    text,
                    outline_style.clone(),
                    font_id,
                    font,
                    surface,
                );
            }
        }

        let layout_key =
            text_layout_key(TextGlyphKind::Bitmap, font_id, origin, text, &style, surface);
        if let Some(cached) = layout_key.as_ref().and_then(|key| self.atlas.cached_layout(key)) {
            self.quads.extend(cached);
            return;
        }
        let quads_before = self.quads.len();

        let design_size = if font.size > 0 { font.size } else { font.line_height.max(1) };
        let bitmap_size = style.bitmap_size.unwrap_or(style.size);
        let mut scale = (bitmap_size * surface.height as f32 / design_size as f32).max(0.01);
        let original_scale = scale;
        let mut text_width = bitmap_text_width_px(text, font, scale);
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let text = if max_width > 0.0 && text_width > max_width {
            match style.overflow {
                TextOverflow::Overflow => std::borrow::Cow::Borrowed(text),
                TextOverflow::Shrink => {
                    scale = (scale * max_width / text_width).max(0.01);
                    std::borrow::Cow::Borrowed(text)
                }
                TextOverflow::Truncate => std::borrow::Cow::Owned(truncate_bitmap_text_to_width(
                    text, font, max_width, scale,
                )),
            }
        } else {
            std::borrow::Cow::Borrowed(text)
        };
        text_width = bitmap_text_width_px(&text, font, scale);
        let align_offset = text_align_offset_px(style.align, max_width, text_width);
        let mut cursor_x = origin.x * surface.width as f32 + align_offset;
        let shrink_offset_y =
            if matches!(style.overflow, TextOverflow::Shrink) && scale < original_scale {
                (design_size as f32 * (original_scale - scale)) / 2.0
            } else {
                0.0
            };
        let text_top_y = origin.y * surface.height as f32 + shrink_offset_y;

        for ch in text.chars() {
            let Some(glyph) = font.glyphs.get(&ch) else {
                continue;
            };
            if glyph.width > 0
                && glyph.height > 0
                && let Some(page) = font.pages.get(&glyph.page)
            {
                let cached = self.atlas.cached_bitmap_glyph(font_id, ch, *glyph, page, font, scale);
                self.quads.push(TextQuad {
                    x: (cursor_x + cached.offset_x) / surface.width as f32,
                    y: (text_top_y + cached.offset_y) / surface.height as f32,
                    width: cached.display_width / surface.width as f32,
                    height: cached.display_height / surface.height as f32,
                    atlas_origin: cached.atlas_origin,
                    glyph_width: cached.width,
                    glyph_height: cached.height,
                    color: style.color,
                });
            }
            cursor_x += glyph.xadvance as f32 * scale;
        }

        if let Some(key) = layout_key {
            self.atlas.insert_layout(key, &self.quads[quads_before..]);
        }
    }

    fn push_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        font_id: &str,
        font: &FontArc,
        surface: SurfaceSize,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style.clone();
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_text(&shadow_origin, text, shadow_style, font_id, font, surface);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style.clone();
            outline_style.color = outline.color;
            outline_style.outline = None;
            outline_style.shadow = None;
            let outline_y = outline.width;
            let outline_x = outline.width * surface.height as f32 / surface.width as f32;
            for (offset_x, offset_y) in [
                (-outline_x, -outline_y),
                (0.0, -outline_y),
                (outline_x, -outline_y),
                (-outline_x, 0.0),
                (outline_x, 0.0),
                (-outline_x, outline_y),
                (0.0, outline_y),
                (outline_x, outline_y),
            ] {
                let outline_origin = Point { x: origin.x + offset_x, y: origin.y + offset_y };
                self.push_text(
                    &outline_origin,
                    text,
                    outline_style.clone(),
                    font_id,
                    font,
                    surface,
                );
            }
        }

        let layout_key =
            text_layout_key(TextGlyphKind::Vector, font_id, origin, text, &style, surface);
        if let Some(cached) = layout_key.as_ref().and_then(|key| self.atlas.cached_layout(key)) {
            self.quads.extend(cached);
            return;
        }
        let quads_before = self.quads.len();

        let mut px_size = (style.size * surface.height as f32).max(1.0);
        let original_px_size = px_size;
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let mut text = std::borrow::Cow::Borrowed(text);
        let mut scale = PxScale::from(px_size);
        let mut scaled_font = font.as_scaled(scale);
        let mut text_width = text_width_px(&text, font, &scaled_font);
        if style.wrapping && max_width > 0.0 {
            let lines = wrap_text_to_width(&text, font, &scaled_font, max_width);
            let line_height = (scaled_font.ascent() - scaled_font.descent()
                + scaled_font.line_gap())
            .max(px_size);
            let origin_x = origin.x * surface.width as f32;
            let first_baseline_y = origin.y * surface.height as f32 + scaled_font.ascent();
            for (index, line) in lines.iter().enumerate() {
                let line_width = text_width_px(line, font, &scaled_font);
                let baseline_y = first_baseline_y + line_height * index as f32;
                self.push_text_line(
                    origin_x,
                    baseline_y,
                    line,
                    line_width,
                    scale,
                    style.clone(),
                    font_id,
                    font,
                    surface,
                );
            }
            return;
        }
        if max_width > 0.0 && text_width > max_width {
            match style.overflow {
                TextOverflow::Overflow => {}
                TextOverflow::Shrink => {
                    px_size = (px_size * max_width / text_width).max(1.0);
                    scale = PxScale::from(px_size);
                    scaled_font = font.as_scaled(scale);
                    text_width = text_width_px(&text, font, &scaled_font);
                }
                TextOverflow::Truncate => {
                    text = std::borrow::Cow::Owned(truncate_text_to_width(
                        &text,
                        font,
                        &scaled_font,
                        max_width,
                    ));
                    text_width = text_width_px(&text, font, &scaled_font);
                }
            }
        }
        let cursor_x = origin.x * surface.width as f32;
        let shrink_offset_y =
            if matches!(style.overflow, TextOverflow::Shrink) && px_size < original_px_size {
                (original_px_size - px_size) / 2.0
            } else {
                0.0
            };
        let baseline_y = origin.y * surface.height as f32 + shrink_offset_y + scaled_font.ascent();

        self.push_text_line(
            cursor_x, baseline_y, &text, text_width, scale, style, font_id, font, surface,
        );

        if let Some(key) = layout_key {
            self.atlas.insert_layout(key, &self.quads[quads_before..]);
        }
    }

    fn push_text_line(
        &mut self,
        origin_x: f32,
        baseline_y: f32,
        text: &str,
        text_width: f32,
        scale: PxScale,
        style: TextStyle,
        font_id: &str,
        font: &FontArc,
        surface: SurfaceSize,
    ) {
        let scaled_font = font.as_scaled(scale);
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let align_offset = text_align_offset_px(style.align, max_width, text_width);
        let mut cursor_x = origin_x + align_offset;

        for ch in text.chars() {
            let glyph_id = font.glyph_id(ch);
            let advance = scaled_font.h_advance(glyph_id);
            if let Some(cached) = self.atlas.cached_vector_glyph(font_id, ch, scale, font) {
                self.quads.push(TextQuad {
                    x: (cursor_x + cached.offset_x) / surface.width as f32,
                    y: (baseline_y + cached.offset_y) / surface.height as f32,
                    width: cached.display_width / surface.width as f32,
                    height: cached.display_height / surface.height as f32,
                    atlas_origin: cached.atlas_origin,
                    glyph_width: cached.width,
                    glyph_height: cached.height,
                    color: style.color,
                });
            }
            cursor_x += advance;
        }
    }

    fn finish(
        self,
        command_quad_counts: Vec<usize>,
        command_caret_rects: Vec<Option<RectCommand>>,
    ) -> TextFrame {
        let size = self.atlas.size();
        let instances = encode_text_quads(&self.quads, size.width, size.height);
        TextFrame {
            size,
            #[cfg(test)]
            pixels: Vec::new(),
            dirty_regions: std::mem::take(&mut self.atlas.dirty_regions),
            instances,
            command_quad_counts,
            command_caret_rects,
        }
    }
}

#[cfg(test)]
struct TextAtlasBuilder {
    width: u32,
    pen_x: u32,
    pen_y: u32,
    row_height: u32,
    pixels: Vec<u8>,
    quads: Vec<TextQuad>,
}

#[derive(Debug, Clone)]
struct TextQuad {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    atlas_origin: (u32, u32),
    glyph_width: u32,
    glyph_height: u32,
    color: Color,
}

#[cfg(test)]
impl TextAtlasBuilder {
    fn new(width: u32) -> Self {
        Self { width, pen_x: 0, pen_y: 0, row_height: 0, pixels: Vec::new(), quads: Vec::new() }
    }

    fn push_bitmap_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        font: &BitmapFont,
        surface: SurfaceSize,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style.clone();
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_bitmap_text(&shadow_origin, text, shadow_style, font, surface);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style.clone();
            outline_style.color = outline.color;
            outline_style.outline = None;
            outline_style.shadow = None;
            let outline_y = outline.width;
            let outline_x = outline.width * surface.height as f32 / surface.width as f32;
            for (offset_x, offset_y) in [
                (-outline_x, -outline_y),
                (0.0, -outline_y),
                (outline_x, -outline_y),
                (-outline_x, 0.0),
                (outline_x, 0.0),
                (-outline_x, outline_y),
                (0.0, outline_y),
                (outline_x, outline_y),
            ] {
                let outline_origin = Point { x: origin.x + offset_x, y: origin.y + offset_y };
                self.push_bitmap_text(&outline_origin, text, outline_style.clone(), font, surface);
            }
        }

        let design_size = if font.size > 0 { font.size } else { font.line_height.max(1) };
        let bitmap_size = style.bitmap_size.unwrap_or(style.size);
        let mut scale = (bitmap_size * surface.height as f32 / design_size as f32).max(0.01);
        let original_scale = scale;
        let mut text_width = bitmap_text_width_px(text, font, scale);
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let text = if max_width > 0.0 && text_width > max_width {
            match style.overflow {
                TextOverflow::Overflow => std::borrow::Cow::Borrowed(text),
                TextOverflow::Shrink => {
                    scale = (scale * max_width / text_width).max(0.01);
                    std::borrow::Cow::Borrowed(text)
                }
                TextOverflow::Truncate => std::borrow::Cow::Owned(truncate_bitmap_text_to_width(
                    text, font, max_width, scale,
                )),
            }
        } else {
            std::borrow::Cow::Borrowed(text)
        };
        text_width = bitmap_text_width_px(&text, font, scale);
        let align_offset = text_align_offset_px(style.align, max_width, text_width);
        let mut cursor_x = origin.x * surface.width as f32 + align_offset;
        let shrink_offset_y =
            if matches!(style.overflow, TextOverflow::Shrink) && scale < original_scale {
                (design_size as f32 * (original_scale - scale)) / 2.0
            } else {
                0.0
            };
        let text_top_y = origin.y * surface.height as f32 + shrink_offset_y;

        for ch in text.chars() {
            let Some(glyph) = font.glyphs.get(&ch) else {
                continue;
            };
            if glyph.width > 0 && glyph.height > 0 {
                let Some(page) = font.pages.get(&glyph.page) else {
                    cursor_x += glyph.xadvance as f32 * scale;
                    continue;
                };
                let glyph_width = (glyph.width as f32 * scale).ceil().max(1.0) as u32;
                let glyph_height = (glyph.height as f32 * scale).ceil().max(1.0) as u32;
                let atlas_origin = self.reserve(glyph_width, glyph_height);
                self.blit_bitmap_glyph(
                    atlas_origin,
                    glyph_width,
                    glyph_height,
                    *glyph,
                    page,
                    scale,
                );
                let x = (cursor_x + glyph.xoffset as f32 * scale) / surface.width as f32;
                let y = (text_top_y + (glyph.yoffset as f32 - font.ascent) * scale)
                    / surface.height as f32;
                self.quads.push(TextQuad {
                    x,
                    y,
                    width: glyph_width as f32 / surface.width as f32,
                    height: glyph_height as f32 / surface.height as f32,
                    atlas_origin,
                    glyph_width,
                    glyph_height,
                    color: style.color,
                });
            }
            cursor_x += glyph.xadvance as f32 * scale;
        }
    }

    fn blit_bitmap_glyph(
        &mut self,
        atlas_origin: (u32, u32),
        glyph_width: u32,
        glyph_height: u32,
        glyph: crate::bitmap_font::BitmapFontGlyph,
        page: &crate::bitmap_font::BitmapFontPage,
        scale: f32,
    ) {
        let pixels = rasterized_bitmap_glyph_pixels(glyph, page, scale, glyph_width, glyph_height);
        for dst_y in 0..glyph_height {
            for dst_x in 0..glyph_width {
                let src_index = ((dst_y * glyph_width + dst_x) * 4) as usize;
                let Some(src) = pixels.get(src_index..src_index + 4) else {
                    continue;
                };
                let dst_index =
                    (((atlas_origin.1 + dst_y) * self.width + atlas_origin.0 + dst_x) * 4) as usize;
                let Some(dst) = self.pixels.get_mut(dst_index..dst_index + 4) else {
                    continue;
                };
                // ビットマップフォントは RGBA を保持して描画したい (色付きグリフ対応)。
                // 同じアトラス位置に重ねたい場合は alpha が大きい方を採用。
                if src[3] >= dst[3] {
                    dst.copy_from_slice(src);
                }
            }
        }
    }

    fn push_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        font: &FontArc,
        surface: SurfaceSize,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style.clone();
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_text(&shadow_origin, text, shadow_style, font, surface);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style.clone();
            outline_style.color = outline.color;
            outline_style.outline = None;
            outline_style.shadow = None;
            let outline_y = outline.width;
            let outline_x = outline.width * surface.height as f32 / surface.width as f32;
            for (offset_x, offset_y) in [
                (-outline_x, -outline_y),
                (0.0, -outline_y),
                (outline_x, -outline_y),
                (-outline_x, 0.0),
                (outline_x, 0.0),
                (-outline_x, outline_y),
                (0.0, outline_y),
                (outline_x, outline_y),
            ] {
                let outline_origin = Point { x: origin.x + offset_x, y: origin.y + offset_y };
                self.push_text(&outline_origin, text, outline_style.clone(), font, surface);
            }
        }

        let mut px_size = (style.size * surface.height as f32).max(1.0);
        let original_px_size = px_size;
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let mut text = std::borrow::Cow::Borrowed(text);
        let mut scale = PxScale::from(px_size);
        let mut scaled_font = font.as_scaled(scale);
        let mut text_width = text_width_px(&text, font, &scaled_font);
        if style.wrapping && max_width > 0.0 {
            let lines = wrap_text_to_width(&text, font, &scaled_font, max_width);
            let line_height = (scaled_font.ascent() - scaled_font.descent()
                + scaled_font.line_gap())
            .max(px_size);
            let origin_x = origin.x * surface.width as f32;
            let first_baseline_y = origin.y * surface.height as f32 + scaled_font.ascent();
            for (index, line) in lines.iter().enumerate() {
                let line_width = text_width_px(line, font, &scaled_font);
                let baseline_y = first_baseline_y + line_height * index as f32;
                self.push_text_line(
                    origin_x,
                    baseline_y,
                    line,
                    line_width,
                    scale,
                    style.clone(),
                    font,
                    surface,
                );
            }
            return;
        }
        if max_width > 0.0 && text_width > max_width {
            match style.overflow {
                TextOverflow::Overflow => {}
                TextOverflow::Shrink => {
                    px_size = (px_size * max_width / text_width).max(1.0);
                    scale = PxScale::from(px_size);
                    scaled_font = font.as_scaled(scale);
                    text_width = text_width_px(&text, font, &scaled_font);
                }
                TextOverflow::Truncate => {
                    text = std::borrow::Cow::Owned(truncate_text_to_width(
                        &text,
                        font,
                        &scaled_font,
                        max_width,
                    ));
                    text_width = text_width_px(&text, font, &scaled_font);
                }
            }
        }
        let cursor_x = origin.x * surface.width as f32;
        let shrink_offset_y =
            if matches!(style.overflow, TextOverflow::Shrink) && px_size < original_px_size {
                (original_px_size - px_size) / 2.0
            } else {
                0.0
            };
        let baseline_y = origin.y * surface.height as f32 + shrink_offset_y + scaled_font.ascent();

        self.push_text_line(cursor_x, baseline_y, &text, text_width, scale, style, font, surface);
    }

    fn push_text_line(
        &mut self,
        origin_x: f32,
        baseline_y: f32,
        text: &str,
        text_width: f32,
        scale: PxScale,
        style: TextStyle,
        font: &FontArc,
        surface: SurfaceSize,
    ) {
        let scaled_font = font.as_scaled(scale);
        let max_width = style.max_width.max(0.0) * surface.width as f32;
        let align_offset = text_align_offset_px(style.align, max_width, text_width);
        let mut cursor_x = origin_x + align_offset;

        for ch in text.chars() {
            let glyph_id = font.glyph_id(ch);
            let advance = scaled_font.h_advance(glyph_id);
            let glyph = Glyph { id: glyph_id, scale, position: point(cursor_x, baseline_y) };
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let glyph_width = bounds.width().ceil().max(0.0) as u32;
                let glyph_height = bounds.height().ceil().max(0.0) as u32;
                if glyph_width > 0 && glyph_height > 0 {
                    let atlas_origin = self.reserve(glyph_width, glyph_height);
                    outlined.draw(|x, y, coverage| {
                        let dst_x = atlas_origin.0 + x;
                        let dst_y = atlas_origin.1 + y;
                        let index = ((dst_y * self.width + dst_x) * 4) as usize;
                        let coverage_u8 = (coverage * 255.0).clamp(0.0, 255.0) as u8;
                        if let Some(pixel) = self.pixels.get_mut(index..index + 4) {
                            // TTF グリフは白 RGB に coverage を alpha として書く。
                            // 既存値より coverage が大きい場合のみ上書き。
                            if coverage_u8 >= pixel[3] {
                                pixel[0] = 255;
                                pixel[1] = 255;
                                pixel[2] = 255;
                                pixel[3] = coverage_u8;
                            }
                        }
                    });
                    self.quads.push(TextQuad {
                        x: bounds.min.x / surface.width as f32,
                        y: bounds.min.y / surface.height as f32,
                        width: glyph_width as f32 / surface.width as f32,
                        height: glyph_height as f32 / surface.height as f32,
                        atlas_origin,
                        glyph_width,
                        glyph_height,
                        color: style.color,
                    });
                }
            }
            cursor_x += advance;
        }
    }

    fn reserve(&mut self, glyph_width: u32, glyph_height: u32) -> (u32, u32) {
        let padded_width = glyph_width + TEXT_ATLAS_PADDING * 2;
        let padded_height = glyph_height + TEXT_ATLAS_PADDING * 2;
        if self.pen_x + padded_width > self.width {
            self.pen_x = 0;
            self.pen_y += self.row_height;
            self.row_height = 0;
        }

        let origin = (self.pen_x + TEXT_ATLAS_PADDING, self.pen_y + TEXT_ATLAS_PADDING);
        self.pen_x += padded_width;
        self.row_height = self.row_height.max(padded_height);
        self.ensure_height(self.pen_y + self.row_height);
        origin
    }

    fn ensure_height(&mut self, height: u32) {
        let needed = (self.width * height * 4) as usize;
        if self.pixels.len() < needed {
            self.pixels.resize(needed, 0);
        }
    }

    fn atlas_height(&self) -> u32 {
        (self.pen_y + self.row_height).max(1)
    }

    fn finish(mut self) -> TextFrame {
        let height = self.atlas_height();
        self.pixels.resize((self.width * height * 4) as usize, 0);
        let instances = encode_text_quads(&self.quads, self.width, height);
        TextFrame {
            size: AtlasSize { width: self.width, height },
            #[cfg(test)]
            pixels: self.pixels,
            dirty_regions: Vec::new(),
            instances,
            command_quad_counts: Vec::new(),
            command_caret_rects: Vec::new(),
        }
    }
}

fn text_width_px<F: Font>(text: &str, font: &FontArc, scaled_font: &impl ScaleFont<F>) -> f32 {
    text.chars().map(|ch| scaled_font.h_advance(font.glyph_id(ch))).sum()
}

fn bitmap_text_width_px(text: &str, font: &BitmapFont, scale: f32) -> f32 {
    text.chars()
        .filter_map(|ch| font.glyphs.get(&ch))
        .map(|glyph| glyph.xadvance as f32 * scale)
        .sum()
}

fn rasterized_bitmap_glyph_pixels(
    glyph: crate::bitmap_font::BitmapFontGlyph,
    page: &crate::bitmap_font::BitmapFontPage,
    scale: f32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut pixels = vec![0; (width * height * 4) as usize];
    let nearest = is_integer_scale(scale);
    for dst_y in 0..height {
        for dst_x in 0..width {
            let color = if nearest {
                sample_bitmap_glyph_nearest(glyph, page, dst_x, dst_y, scale)
            } else {
                sample_bitmap_glyph_bilinear(glyph, page, dst_x, dst_y, scale)
            };
            let dst_index = ((dst_y * width + dst_x) * 4) as usize;
            if let Some(dst) = pixels.get_mut(dst_index..dst_index + 4)
                && color[3] >= dst[3]
            {
                dst.copy_from_slice(&color);
            }
        }
    }
    pixels
}

fn is_integer_scale(scale: f32) -> bool {
    (scale - scale.round()).abs() <= 0.001
}

fn sample_bitmap_glyph_nearest(
    glyph: crate::bitmap_font::BitmapFontGlyph,
    page: &crate::bitmap_font::BitmapFontPage,
    dst_x: u32,
    dst_y: u32,
    scale: f32,
) -> [u8; 4] {
    let local_x = (dst_x as f32 / scale).floor() as i32;
    let local_y = (dst_y as f32 / scale).floor() as i32;
    bitmap_glyph_pixel(glyph, page, local_x, local_y)
}

fn sample_bitmap_glyph_bilinear(
    glyph: crate::bitmap_font::BitmapFontGlyph,
    page: &crate::bitmap_font::BitmapFontPage,
    dst_x: u32,
    dst_y: u32,
    scale: f32,
) -> [u8; 4] {
    let max_x = glyph.width.saturating_sub(1) as f32;
    let max_y = glyph.height.saturating_sub(1) as f32;
    let src_x = ((dst_x as f32 + 0.5) / scale - 0.5).clamp(0.0, max_x);
    let src_y = ((dst_y as f32 + 0.5) / scale - 0.5).clamp(0.0, max_y);
    let x0 = src_x.floor() as i32;
    let y0 = src_y.floor() as i32;
    let x1 = (x0 + 1).min(max_x as i32);
    let y1 = (y0 + 1).min(max_y as i32);
    let tx = src_x - x0 as f32;
    let ty = src_y - y0 as f32;

    let p00 = bitmap_glyph_pixel(glyph, page, x0, y0);
    let p10 = bitmap_glyph_pixel(glyph, page, x1, y0);
    let p01 = bitmap_glyph_pixel(glyph, page, x0, y1);
    let p11 = bitmap_glyph_pixel(glyph, page, x1, y1);
    blend_bitmap_pixels([
        (p00, (1.0 - tx) * (1.0 - ty)),
        (p10, tx * (1.0 - ty)),
        (p01, (1.0 - tx) * ty),
        (p11, tx * ty),
    ])
}

fn bitmap_glyph_pixel(
    glyph: crate::bitmap_font::BitmapFontGlyph,
    page: &crate::bitmap_font::BitmapFontPage,
    local_x: i32,
    local_y: i32,
) -> [u8; 4] {
    if glyph.width == 0 || glyph.height == 0 {
        return [0, 0, 0, 0];
    }
    let local_x = local_x.clamp(0, glyph.width.saturating_sub(1) as i32) as u32;
    let local_y = local_y.clamp(0, glyph.height.saturating_sub(1) as i32) as u32;
    let src_x = glyph.x + local_x;
    let src_y = glyph.y + local_y;
    if src_x >= page.image.width || src_y >= page.image.height {
        return [0, 0, 0, 0];
    }
    let src_index = ((src_y * page.image.width + src_x) * 4) as usize;
    page.image
        .pixels
        .get(src_index..src_index + 4)
        .map(|src| [src[0], src[1], src[2], src[3]])
        .unwrap_or([0, 0, 0, 0])
}

fn blend_bitmap_pixels(samples: [([u8; 4], f32); 4]) -> [u8; 4] {
    let mut alpha = 0.0;
    let mut premul = [0.0; 3];
    for (pixel, weight) in samples {
        let a = pixel[3] as f32 / 255.0;
        alpha += a * weight;
        for channel in 0..3 {
            premul[channel] += (pixel[channel] as f32 / 255.0) * a * weight;
        }
    }
    if alpha <= f32::EPSILON {
        return [0, 0, 0, 0];
    }
    [
        ((premul[0] / alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((premul[1] / alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((premul[2] / alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

fn text_align_offset_px(align: TextAlign, max_width: f32, text_width: f32) -> f32 {
    match align {
        TextAlign::Left => 0.0,
        TextAlign::Center if max_width > 0.0 => (max_width - text_width) / 2.0,
        TextAlign::Center => -text_width / 2.0,
        TextAlign::Right if max_width > 0.0 => max_width - text_width,
        TextAlign::Right => -text_width,
    }
}

fn vector_text_caret_rect(
    origin: &Point,
    text: &str,
    style: &TextStyle,
    font: &FontArc,
    surface: SurfaceSize,
    caret: TextCaret,
) -> Option<RectCommand> {
    if style.wrapping || !surface.is_drawable() {
        return None;
    }
    let mut px_size = (style.size * surface.height as f32).max(1.0);
    let original_px_size = px_size;
    let max_width = style.max_width.max(0.0) * surface.width as f32;
    let mut visible = Cow::Borrowed(text);
    let mut scale = PxScale::from(px_size);
    let mut scaled_font = font.as_scaled(scale);
    let mut text_width = text_width_px(&visible, font, &scaled_font);
    if max_width > 0.0 && text_width > max_width {
        match style.overflow {
            TextOverflow::Overflow => {}
            TextOverflow::Shrink => {
                px_size = (px_size * max_width / text_width).max(1.0);
                scale = PxScale::from(px_size);
                scaled_font = font.as_scaled(scale);
                text_width = text_width_px(&visible, font, &scaled_font);
            }
            TextOverflow::Truncate => {
                visible =
                    Cow::Owned(truncate_text_to_width(&visible, font, &scaled_font, max_width));
                text_width = text_width_px(&visible, font, &scaled_font);
            }
        }
    }
    let align_offset = text_align_offset_px(style.align, max_width, text_width);
    let cursor = clamp_text_byte_index(&visible, caret.byte_index);
    let prefix_width = text_width_px(&visible[..cursor], font, &scaled_font);
    let shrink_offset_y =
        if matches!(style.overflow, TextOverflow::Shrink) && px_size < original_px_size {
            (original_px_size - px_size) / 2.0
        } else {
            0.0
        };
    let x = (origin.x * surface.width as f32 + align_offset + prefix_width) / surface.width as f32;
    let y = (origin.y * surface.height as f32 + shrink_offset_y) / surface.height as f32;
    Some(RectCommand {
        rect: Rect {
            x,
            y,
            width: (2.0 / surface.width as f32).max(0.001),
            height: (px_size / surface.height as f32).max(0.001),
        },
        color: caret.color,
    })
}

fn bitmap_text_caret_rect(
    origin: &Point,
    text: &str,
    style: &TextStyle,
    font: &BitmapFont,
    surface: SurfaceSize,
    caret: TextCaret,
) -> Option<RectCommand> {
    if style.wrapping || !surface.is_drawable() {
        return None;
    }
    let design_size = if font.size > 0 { font.size } else { font.line_height.max(1) };
    let bitmap_size = style.bitmap_size.unwrap_or(style.size);
    let mut scale = (bitmap_size * surface.height as f32 / design_size as f32).max(0.01);
    let original_scale = scale;
    let max_width = style.max_width.max(0.0) * surface.width as f32;
    let mut text_width = bitmap_text_width_px(text, font, scale);
    let visible = if max_width > 0.0 && text_width > max_width {
        match style.overflow {
            TextOverflow::Overflow => Cow::Borrowed(text),
            TextOverflow::Shrink => {
                scale = (scale * max_width / text_width).max(0.01);
                Cow::Borrowed(text)
            }
            TextOverflow::Truncate => {
                Cow::Owned(truncate_bitmap_text_to_width(text, font, max_width, scale))
            }
        }
    } else {
        Cow::Borrowed(text)
    };
    text_width = bitmap_text_width_px(&visible, font, scale);
    let align_offset = text_align_offset_px(style.align, max_width, text_width);
    let cursor = clamp_text_byte_index(&visible, caret.byte_index);
    let prefix_width = bitmap_text_width_px(&visible[..cursor], font, scale);
    let shrink_offset_y =
        if matches!(style.overflow, TextOverflow::Shrink) && scale < original_scale {
            (design_size as f32 * (original_scale - scale)) / 2.0
        } else {
            0.0
        };
    let caret_height = design_size as f32 * scale;
    let x = (origin.x * surface.width as f32 + align_offset + prefix_width) / surface.width as f32;
    let y = (origin.y * surface.height as f32 + shrink_offset_y) / surface.height as f32;
    Some(RectCommand {
        rect: Rect {
            x,
            y,
            width: (2.0 / surface.width as f32).max(0.001),
            height: (caret_height / surface.height as f32).max(0.001),
        },
        color: caret.color,
    })
}

fn clamp_text_byte_index(text: &str, byte_index: usize) -> usize {
    let mut byte_index = byte_index.min(text.len());
    while byte_index > 0 && !text.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

fn truncate_bitmap_text_to_width(
    text: &str,
    font: &BitmapFont,
    max_width: f32,
    scale: f32,
) -> String {
    let mut width = 0.0;
    let mut result = String::new();
    for ch in text.chars() {
        let Some(glyph) = font.glyphs.get(&ch) else {
            continue;
        };
        let advance = glyph.xadvance as f32 * scale;
        if width + advance > max_width {
            break;
        }
        width += advance;
        result.push(ch);
    }
    result
}

fn wrap_text_to_width<F: Font>(
    text: &str,
    font: &FontArc,
    scaled_font: &impl ScaleFont<F>,
    max_width: f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for source_line in text.split('\n') {
        let mut line = String::new();
        let mut width = 0.0;
        for ch in source_line.chars() {
            let advance = scaled_font.h_advance(font.glyph_id(ch));
            if !line.is_empty() && width + advance > max_width {
                lines.push(std::mem::take(&mut line));
                width = 0.0;
            }
            line.push(ch);
            width += advance;
        }
        lines.push(line);
    }
    lines
}

fn truncate_text_to_width<F: Font>(
    text: &str,
    font: &FontArc,
    scaled_font: &impl ScaleFont<F>,
    max_width: f32,
) -> String {
    let mut width = 0.0;
    let mut result = String::new();
    for ch in text.chars() {
        let advance = scaled_font.h_advance(font.glyph_id(ch));
        if width + advance > max_width {
            break;
        }
        width += advance;
        result.push(ch);
    }
    result
}

fn encode_text_quads(quads: &[TextQuad], atlas_width: u32, atlas_height: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(quads.len() * TEXT_INSTANCE_BYTES);
    for quad in quads {
        for value in [
            quad.x,
            quad.y,
            quad.width,
            quad.height,
            quad.atlas_origin.0 as f32 / atlas_width as f32,
            quad.atlas_origin.1 as f32 / atlas_height as f32,
            quad.glyph_width as f32 / atlas_width as f32,
            quad.glyph_height as f32 / atlas_height as f32,
            quad.color.r,
            quad.color.g,
            quad.color.b,
            quad.color.a,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn create_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bmz-render rect shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(RECT_SHADER)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bmz-render rect pipeline layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render rect pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: RECT_INSTANCE_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 16,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_image_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bmz-render image bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_image_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bmz-render image sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

fn create_image_sampler_linear(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bmz-render image sampler linear"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

fn create_default_image_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> HashMap<TextureId, PreparedTexture> {
    let mut textures = HashMap::new();
    textures.insert(
        TextureId(0),
        create_rgba_texture(device, queue, TextureId(0), 1, 1, &[255, 255, 255, 255]),
    );
    textures
}

/// GPU テクスチャアップロード機能のハンドル。`wgpu::Device` / `Queue` の clone を
/// 保持し、メインスレッド外 (skin upload worker) から RGBA8 をアップロードできる。
/// `Renderer::gpu_uploader` で取得する。`Send + Clone`。
#[derive(Clone)]
pub struct GpuUploader {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuUploader {
    /// RGBA8 バイト列を GPU テクスチャへアップロードして `PreparedTexture` を返す。
    pub fn upload(&self, width: u32, height: u32, rgba: &[u8]) -> PreparedTexture {
        create_rgba_texture(&self.device, &self.queue, TextureId(0), width, height, rgba)
    }
}

fn create_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: TextureId,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> PreparedTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bmz-render image texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_rgba_texture(queue, &texture, width, height, rgba);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    tracing::debug!(texture_id = id.0, width, height, "registered render image texture");
    PreparedTexture { texture, view, width, height }
}

fn write_rgba_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    rgba: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}

fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    blend_mode: BlendMode,
) -> wgpu::RenderPipeline {
    let shader_source = match blend_mode {
        BlendMode::LayerMask => IMAGE_SHADER_LAYER,
        BlendMode::Normal | BlendMode::Add => IMAGE_SHADER,
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bmz-render image shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bmz-render image pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(match blend_mode {
            BlendMode::Normal => "bmz-render image pipeline",
            BlendMode::Add => "bmz-render additive image pipeline",
            BlendMode::LayerMask => "bmz-render layer-mask image pipeline",
        }),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: IMAGE_INSTANCE_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 16,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 32,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 48,
                        shader_location: 3,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(image_blend_state(blend_mode)),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn image_blend_state(blend_mode: BlendMode) -> wgpu::BlendState {
    match blend_mode {
        BlendMode::Normal | BlendMode::LayerMask => wgpu::BlendState::ALPHA_BLENDING,
        BlendMode::Add => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
    }
}

fn create_text_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bmz-render text bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_text_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bmz-render text sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

fn create_text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bmz-render text shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TEXT_SHADER)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bmz-render text pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render text pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: TEXT_INSTANCE_BYTES as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 16,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 32,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

const RECT_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let local = corners[vertex_index];
    let pos01 = rect.xy + local * rect.zw;

    var out: VertexOutput;
    out.position = vec4<f32>(pos01.x * 2.0 - 1.0, 1.0 - pos01.y * 2.0, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

const IMAGE_SHADER: &str = r#"
@group(0) @binding(0)
var image_texture: texture_2d<f32>;
@group(0) @binding(1)
var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) rotation: vec4<f32>,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let local = corners[vertex_index];
    let pivot = rotation.yz;
    let relative = (local - pivot) * rect.zw;
    let aspect = max(rotation.w, 0.0001);
    let relative_aspect = vec2<f32>(relative.x * aspect, relative.y);
    let c = cos(rotation.x);
    let s = sin(rotation.x);
    let rotated_aspect = vec2<f32>(
        relative_aspect.x * c - relative_aspect.y * s,
        relative_aspect.x * s + relative_aspect.y * c,
    );
    let rotated = vec2<f32>(rotated_aspect.x / aspect, rotated_aspect.y);
    let pos01 = rect.xy + pivot * rect.zw + rotated;

    var out: VertexOutput;
    out.position = vec4<f32>(pos01.x * 2.0 - 1.0, 1.0 - pos01.y * 2.0, 0.0, 1.0);
    out.uv = uv_rect.xy + local * uv_rect.zw;
    out.tint = tint;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(image_texture, image_sampler, input.uv) * input.tint;
}
"#;

// beatoraja の layer.frag 相当: BGA Layer / Layer2 用に黒ピクセルを透過する。
const IMAGE_SHADER_LAYER: &str = r#"
@group(0) @binding(0)
var image_texture: texture_2d<f32>;
@group(0) @binding(1)
var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) rotation: vec4<f32>,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let local = corners[vertex_index];
    let pivot = rotation.yz;
    let relative = (local - pivot) * rect.zw;
    let aspect = max(rotation.w, 0.0001);
    let relative_aspect = vec2<f32>(relative.x * aspect, relative.y);
    let c = cos(rotation.x);
    let s = sin(rotation.x);
    let rotated_aspect = vec2<f32>(
        relative_aspect.x * c - relative_aspect.y * s,
        relative_aspect.x * s + relative_aspect.y * c,
    );
    let rotated = vec2<f32>(rotated_aspect.x / aspect, rotated_aspect.y);
    let pos01 = rect.xy + pivot * rect.zw + rotated;

    var out: VertexOutput;
    out.position = vec4<f32>(pos01.x * 2.0 - 1.0, 1.0 - pos01.y * 2.0, 0.0, 1.0);
    out.uv = uv_rect.xy + local * uv_rect.zw;
    out.tint = tint;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(image_texture, image_sampler, input.uv);
    if (sampled.r == 0.0 && sampled.g == 0.0 && sampled.b == 0.0) {
        return input.tint * vec4<f32>(sampled.rgb, 0.0);
    }
    return input.tint * sampled;
}
"#;

const TEXT_SHADER: &str = r#"
@group(0) @binding(0)
var text_atlas: texture_2d<f32>;
@group(0) @binding(1)
var text_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @location(0) rect: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) color: vec4<f32>,
) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let local = corners[vertex_index];
    let pos01 = rect.xy + local * rect.zw;

    var out: VertexOutput;
    out.position = vec4<f32>(pos01.x * 2.0 - 1.0, 1.0 - pos01.y * 2.0, 0.0, 1.0);
    out.uv = uv_rect.xy + local * uv_rect.zw;
    out.color = color;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let glyph = textureSample(text_atlas, text_sampler, input.uv);
    // RGBA atlas: bitmap font は色付きグリフを保持。TTF は (1,1,1,coverage)。
    // tint と乗算してアルファブレンド用の最終色を出す。
    return glyph * input.color;
}
"#;

fn resolve_wgpu_present_mode(
    requested: WgpuPresentMode,
    available: &[wgpu::PresentMode],
) -> wgpu::PresentMode {
    let preferred: &[wgpu::PresentMode] = match requested {
        WgpuPresentMode::Fifo => &[wgpu::PresentMode::Fifo],
        WgpuPresentMode::FifoRelaxed => &[wgpu::PresentMode::FifoRelaxed, wgpu::PresentMode::Fifo],
        WgpuPresentMode::Immediate => &[
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::FifoRelaxed,
            wgpu::PresentMode::Fifo,
        ],
        WgpuPresentMode::Mailbox => {
            &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::FifoRelaxed, wgpu::PresentMode::Fifo]
        }
    };
    if let Some(mode) = preferred.iter().copied().find(|mode| available.contains(mode)) {
        return mode;
    }
    let fallback = available.first().copied().unwrap_or(wgpu::PresentMode::Fifo);
    tracing::warn!(
        requested = ?requested,
        available = ?available,
        fallback = ?fallback,
        "requested present mode is unavailable; using fallback"
    );
    fallback
}

fn configure_surface_settings(
    config: &mut wgpu::SurfaceConfiguration,
    requested_present_mode: WgpuPresentMode,
    available_present_modes: &[wgpu::PresentMode],
) {
    config.present_mode =
        resolve_wgpu_present_mode(requested_present_mode, available_present_modes);
    config.desired_maximum_frame_latency = match config.present_mode {
        wgpu::PresentMode::Mailbox => MAILBOX_MAXIMUM_FRAME_LATENCY,
        _ => LOW_LATENCY_MAXIMUM_FRAME_LATENCY,
    };
    config.usage |= wgpu::TextureUsages::COPY_SRC;
}

fn wgpu_present_mode_label(mode: wgpu::PresentMode) -> &'static str {
    match mode {
        wgpu::PresentMode::AutoVsync => "AutoVsync",
        wgpu::PresentMode::AutoNoVsync => "AutoNoVsync",
        wgpu::PresentMode::Fifo => "Fifo",
        wgpu::PresentMode::FifoRelaxed => "FifoRelaxed",
        wgpu::PresentMode::Immediate => "Immediate",
        wgpu::PresentMode::Mailbox => "Mailbox",
    }
}

fn load_default_font() -> Option<FontArc> {
    if let Some(resolved) = bmz_font::resolve_system_font(true) {
        return load_font_from_resolved(&resolved);
    }

    if let Some(resolved) = bmz_font::resolve_system_font(false) {
        tracing::warn!(
            "no Japanese-capable font found; default skin text will fall back to a Latin-only font"
        );
        return load_font_from_resolved(&resolved);
    }

    tracing::warn!("no default render font found; text draw commands will be skipped");
    None
}

fn load_font_from_resolved(resolved: &bmz_font::ResolvedFont) -> Option<FontArc> {
    let source = bmz_font::resolved_font_source(resolved);
    let bytes = bmz_font::read_resolved_font_bytes(resolved).ok()?;
    match FontVec::try_from_vec_and_index(bytes, resolved.font_index) {
        Ok(font) => Some(FontArc::from(font)),
        Err(error) => {
            tracing::warn!(%error, source, "failed to load default render font");
            None
        }
    }
}

/// egui など外部 UI 向けに、日本語表示が可能なフォントファイルの生バイト列を返す。
///
/// OS フォント DB から CJK 対応 face を font-kit 経由で解決し、
/// collection index 付きでファイル全体を返す。
pub fn load_japanese_font_bytes() -> Option<Vec<u8>> {
    let resolved = bmz_font::resolve_system_font(true)?;
    let bytes = bmz_font::read_resolved_font_bytes(&resolved).ok()?;
    if bmz_font::font_supports_japanese(&bytes, resolved.font_index) {
        Some(bytes)
    } else {
        tracing::warn!("no Japanese-capable font found for egui; text may render as tofu");
        None
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = noop_waker();
    let mut context = TaskContext::from_waker(&waker);
    let mut future = Box::pin(future);

    loop {
        match Pin::new(&mut future).poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::yield_now(),
        }
    }
}

fn noop_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(clone, wake, wake_by_ref, drop))
    }

    // SAFETY: The vtable functions do not dereference the null data pointer.
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

#[cfg(test)]
mod tests {
    use crate::scene::{AppSceneSnapshot, SelectSnapshot};

    use super::*;

    fn test_surface_size() -> SurfaceSize {
        SurfaceSize { width: 16, height: 9 }
    }

    #[test]
    fn screenshot_png_is_encoded_once_for_file_and_clipboard_use() {
        let png = encode_screenshot_png(1, 1, &[0x12, 0x34, 0x56, 0x78]).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .unwrap()
            .into_rgba8();
        assert_eq!(decoded.into_raw(), [0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn screenshot_dibv5_has_bottom_up_bgra_pixels() {
        let rgba = [
            1, 2, 3, 4, 5, 6, 7, 8, // top row
            9, 10, 11, 12, 13, 14, 15, 16, // bottom row
        ];
        let dib = screenshot_dibv5(2, 2, &rgba).unwrap();

        assert_eq!(dib.len(), 124 + rgba.len());
        assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 124);
        assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 2);
        assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(dib[14..16].try_into().unwrap()), 32);
        assert_eq!(u32::from_le_bytes(dib[16..20].try_into().unwrap()), 3);
        assert_eq!(&dib[124..], &[11, 10, 9, 12, 15, 14, 13, 16, 3, 2, 1, 4, 7, 6, 5, 8]);
    }

    #[test]
    fn screenshot_dibv5_rejects_mismatched_rgba_length() {
        let error = screenshot_dibv5(2, 2, &[0; 15]).unwrap_err();
        assert!(error.to_string().contains("expected 16, got 15"));
    }

    #[test]
    fn text_align_offset_anchors_zero_width_text_like_beatoraja() {
        assert_eq!(text_align_offset_px(TextAlign::Left, 0.0, 80.0), 0.0);
        assert_eq!(text_align_offset_px(TextAlign::Center, 0.0, 80.0), -40.0);
        assert_eq!(text_align_offset_px(TextAlign::Right, 0.0, 80.0), -80.0);
    }

    #[test]
    fn text_align_offset_uses_box_width_when_present() {
        assert_eq!(text_align_offset_px(TextAlign::Center, 120.0, 80.0), 20.0);
        assert_eq!(text_align_offset_px(TextAlign::Right, 120.0, 80.0), 40.0);
    }

    fn test_bitmap_font() -> BitmapFont {
        let mut pages = HashMap::new();
        pages.insert(
            0,
            crate::bitmap_font::BitmapFontPage {
                id: 0,
                path: std::path::PathBuf::from("page.png"),
                image: crate::assets::RgbaImageAsset {
                    width: 1,
                    height: 1,
                    pixels: vec![255, 255, 255, 255],
                },
            },
        );
        let mut glyphs = HashMap::new();
        glyphs.insert(
            'A',
            crate::bitmap_font::BitmapFontGlyph {
                id: 'A',
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                xoffset: 0,
                yoffset: 0,
                xadvance: 1,
                page: 0,
            },
        );
        BitmapFont {
            size: 10,
            line_height: 10,
            base: 8,
            ascent: 7.0,
            scale_width: 1,
            scale_height: 1,
            pages,
            glyphs,
        }
    }

    fn assert_approx(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.0001, "expected {expected}, got {actual}");
    }

    fn font_supports_japanese<F: Font>(font: &F) -> bool {
        font.glyph_id('あ').0 != 0 && font.glyph_id('日').0 != 0
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_renderer_backend_prefers_vulkan_on_linux() {
        assert_eq!(auto_wgpu_backends(), wgpu::Backends::VULKAN);
        assert_eq!(
            fallback_wgpu_backends(WgpuBackend::Auto),
            &[WgpuBackend::Vulkan, WgpuBackend::Gl]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn auto_renderer_backend_prefers_dx12_on_windows() {
        assert_eq!(auto_wgpu_backends(), wgpu::Backends::DX12);
        assert_eq!(
            fallback_wgpu_backends(WgpuBackend::Auto),
            &[WgpuBackend::Dx12, WgpuBackend::Vulkan, WgpuBackend::Gl]
        );
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    #[test]
    fn auto_renderer_backend_keeps_default_candidates_on_other_platforms() {
        assert_eq!(auto_wgpu_backends(), wgpu::Backends::all());
        assert_eq!(fallback_wgpu_backends(WgpuBackend::Auto), &[WgpuBackend::Auto]);
    }

    #[test]
    fn present_mode_fallbacks_follow_vsync_mode_semantics() {
        use wgpu::PresentMode::{Fifo, FifoRelaxed, Mailbox};

        assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::Fifo, &[Fifo]), Fifo);
        assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::FifoRelaxed, &[Fifo]), Fifo);
        assert_eq!(
            resolve_wgpu_present_mode(WgpuPresentMode::Immediate, &[Mailbox, Fifo]),
            Mailbox
        );
        assert_eq!(
            resolve_wgpu_present_mode(WgpuPresentMode::Immediate, &[FifoRelaxed, Fifo]),
            FifoRelaxed
        );
        assert_eq!(
            resolve_wgpu_present_mode(WgpuPresentMode::Mailbox, &[FifoRelaxed, Fifo]),
            FifoRelaxed
        );
        assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::Mailbox, &[Fifo]), Fifo);
    }

    #[test]
    fn surface_settings_prioritize_low_latency_and_preserve_capture_usage() {
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8Unorm,
            width: 1,
            height: 1,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };

        configure_surface_settings(
            &mut config,
            WgpuPresentMode::Mailbox,
            &[wgpu::PresentMode::Fifo],
        );

        assert_eq!(config.desired_maximum_frame_latency, 1);
        assert_eq!(config.present_mode, wgpu::PresentMode::Fifo);
        assert!(config.usage.contains(wgpu::TextureUsages::COPY_SRC));

        configure_surface_settings(
            &mut config,
            WgpuPresentMode::Mailbox,
            &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo],
        );
        assert_eq!(config.present_mode, wgpu::PresentMode::Mailbox);
        assert_eq!(config.desired_maximum_frame_latency, 2);
    }

    #[test]
    fn screenshot_unpack_removes_row_padding() {
        let mapped =
            [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0];

        let rgba = unpack_screenshot_rgba(&mapped, 2, 2, 12, wgpu::TextureFormat::Rgba8Unorm);

        assert_eq!(rgba, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    }

    #[test]
    fn screenshot_unpack_converts_bgra_to_rgba() {
        let mapped = [10, 20, 30, 40];

        let rgba = unpack_screenshot_rgba(&mapped, 1, 1, 4, wgpu::TextureFormat::Bgra8Unorm);

        assert_eq!(rgba, vec![30, 20, 10, 40]);
    }

    #[test]
    fn renderer_records_last_scene() {
        let mut renderer = Renderer::default();
        let scene = AppSceneSnapshot::Select(SelectSnapshot {
            chart_count: 1,
            selected_index: 0,
            selected_chart_id: Some(7),
            selected_title: "test".to_string(),
            rows: Vec::new(),
            ..Default::default()
        });

        renderer.render_scene(scene.clone()).unwrap();

        assert_eq!(renderer.last_scene(), Some(&scene));
        assert!(renderer.last_plan().is_some());
    }

    #[test]
    fn select_skin_context_update_does_not_reset_play_dynamic_timers() {
        use crate::skin::{
            SKIN_DYNAMIC_TIMER_BASE, SkinDocument, SkinDrawState, SkinDynamicTimerDef, SkinManifest,
        };

        let mut renderer = Renderer::default();
        let mut document: SkinDocument =
            serde_json::from_str(r#"{ "type": 0, "w": 100, "h": 100 }"#).unwrap();
        document.dynamic_timers.push(SkinDynamicTimerDef {
            id: SKIN_DYNAMIC_TIMER_BASE,
            observe: "number(0) >= 0".to_string(),
        });
        let manifest: SkinManifest = SkinManifest::default();
        let context =
            SkinContext::from_manifest_and_document(manifest, document.clone(), Vec::new());
        renderer.set_play_skin_context(context, false);

        let mut state = SkinDrawState::default();
        renderer.play_dynamic_timer_runtime.advance(&document, &mut state, 5_000);
        assert_eq!(state.dynamic_timer_ms[0], Some(0));

        renderer.play_dynamic_timer_runtime.advance(&document, &mut state, 8_000);
        assert_eq!(state.dynamic_timer_ms[0], Some(3_000));

        renderer.set_select_skin_context(SkinContext::default());

        renderer.play_dynamic_timer_runtime.advance(&document, &mut state, 9_000);
        assert_eq!(state.dynamic_timer_ms[0], Some(4_000));
    }

    #[test]
    fn result_skin_fadeout_ms_reads_document_or_defaults_to_zero() {
        use crate::skin::{SkinContext, SkinDocument, SkinManifest};

        let mut renderer = Renderer::default();
        // ドキュメントスキン未設定なら 0 (フェードアウトなし)。
        assert_eq!(renderer.result_skin_fadeout_ms(), 0);

        let document: SkinDocument =
            serde_json::from_str(r#"{ "type": 7, "w": 100, "h": 100, "fadeout": 300 }"#).unwrap();
        let manifest: SkinManifest = SkinManifest::default();
        renderer.set_result_skin_context(SkinContext::from_manifest_and_document(
            manifest,
            document,
            [],
        ));

        assert_eq!(renderer.result_skin_fadeout_ms(), 300);
    }

    #[test]
    fn result_skin_timer_animation_duration_reads_document_or_defaults_to_zero() {
        use crate::skin::{SkinContext, SkinDocument, SkinManifest};

        let mut renderer = Renderer::default();
        assert_eq!(renderer.result_skin_timer_animation_duration_ms(2), 0);

        let document: SkinDocument = serde_json::from_str(
            r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "destination": [{
                    "id": "fadeout",
                    "timer": 2,
                    "dst": [{ "time": 0 }, { "time": 500 }]
                }]
            }"#,
        )
        .unwrap();
        let manifest: SkinManifest = SkinManifest::default();
        renderer.set_result_skin_context(SkinContext::from_manifest_and_document(
            manifest,
            document,
            [],
        ));

        assert_eq!(renderer.result_skin_timer_animation_duration_ms(2), 500);
    }

    #[test]
    fn surface_size_requires_non_zero_dimensions() {
        assert!(SurfaceSize { width: 1, height: 1 }.is_drawable());
        assert!(!SurfaceSize { width: 0, height: 1 }.is_drawable());
        assert!(!SurfaceSize { width: 1, height: 0 }.is_drawable());
    }

    #[test]
    fn canvas_viewport_expand_uses_full_surface() {
        let viewport = CanvasViewport::from_policy(
            SurfaceSize { width: 320, height: 240 },
            CanvasRenderPolicy::default(),
        );

        assert_eq!(viewport.rect, Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 });
        assert!(viewport.is_identity());
        assert_eq!(viewport.content_size(), SurfaceSize { width: 320, height: 240 });
    }

    #[test]
    fn canvas_viewport_contain_same_aspect_uses_identity_transform() {
        let viewport = CanvasViewport::from_policy(
            SurfaceSize { width: 2560, height: 1440 },
            CanvasRenderPolicy {
                fit_mode: CanvasFitMode::Contain,
                canvas_size: Some(CanvasSize { width: 1920, height: 1080 }),
            },
        );

        assert!(viewport.is_identity());
        assert_eq!(viewport.content_size(), SurfaceSize { width: 2560, height: 1440 });
    }

    #[test]
    fn canvas_viewport_contain_letterboxes_tall_surface() {
        let viewport = CanvasViewport::from_policy(
            SurfaceSize { width: 1000, height: 1000 },
            CanvasRenderPolicy {
                fit_mode: CanvasFitMode::Contain,
                canvas_size: Some(CanvasSize { width: 16, height: 9 }),
            },
        );

        assert_approx(viewport.rect.x, 0.0);
        assert_approx(viewport.rect.y, 0.21875);
        assert_approx(viewport.rect.width, 1.0);
        assert_approx(viewport.rect.height, 0.5625);
        assert!(!viewport.is_identity());
        assert_eq!(viewport.content_size(), SurfaceSize { width: 1000, height: 563 });
    }

    #[test]
    fn canvas_viewport_maps_surface_points_back_to_canvas() {
        let viewport = CanvasViewport::from_policy(
            SurfaceSize { width: 1000, height: 1000 },
            CanvasRenderPolicy {
                fit_mode: CanvasFitMode::Contain,
                canvas_size: Some(CanvasSize { width: 16, height: 9 }),
            },
        );

        let (x, y) = viewport.surface_to_canvas_point(0.5, 0.5).unwrap();
        assert_approx(x, 0.5);
        assert_approx(y, 0.5);
        assert!(viewport.surface_to_canvas_point(0.5, 0.1).is_none());
    }

    #[test]
    fn plan_geometry_applies_canvas_viewport_to_images() {
        let surface = SurfaceSize { width: 1000, height: 1000 };
        let viewport = CanvasViewport::from_policy(
            surface,
            CanvasRenderPolicy {
                fit_mode: CanvasFitMode::Contain,
                canvas_size: Some(CanvasSize { width: 16, height: 9 }),
            },
        );
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Image {
                rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                source_size: None,
                texture: TextureId(0),
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                linear_filter: false,
            }],
        };

        let geometry = encode_plan_geometry_with_rect_batch_resolver(
            &plan,
            &TextFrame::default(),
            surface,
            viewport,
            &mut |_, _| None,
        );
        let floats: Vec<f32> = geometry
            .images
            .chunks_exact(std::mem::size_of::<f32>())
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        assert_approx(floats[0], 0.0);
        assert_approx(floats[1], 0.21875);
        assert_approx(floats[2], 1.0);
        assert_approx(floats[3], 0.5625);
    }

    #[test]
    fn sampling_uv_insets_subregions_by_half_texel() {
        let uv = sampling_uv_with_half_texel_inset(
            UvRect { x: 0.25, y: 0.5, width: 0.125, height: 0.25 },
            Some(SkinImageSize { width: 256.0, height: 128.0 }),
        );

        assert_approx(uv.x, 0.25 + 0.5 / 256.0);
        assert_approx(uv.y, 0.5 + 0.5 / 128.0);
        assert_approx(uv.width, 0.125 - 1.0 / 256.0);
        assert_approx(uv.height, 0.25 - 1.0 / 128.0);
    }

    #[test]
    fn sampling_uv_keeps_full_texture_axes_unchanged() {
        let uv = sampling_uv_with_half_texel_inset(
            UvRect { x: 0.0, y: 0.25, width: 1.0, height: 0.5 },
            Some(SkinImageSize { width: 256.0, height: 128.0 }),
        );

        assert_approx(uv.x, 0.0);
        assert_approx(uv.width, 1.0);
        assert_approx(uv.y, 0.25 + 0.5 / 128.0);
        assert_approx(uv.height, 0.5 - 1.0 / 128.0);
    }

    #[test]
    fn sampling_uv_does_not_collapse_single_texel_regions() {
        let uv = sampling_uv_with_half_texel_inset(
            UvRect { x: 0.25, y: 0.5, width: 1.0 / 256.0, height: 1.0 / 128.0 },
            Some(SkinImageSize { width: 256.0, height: 128.0 }),
        );

        assert_approx(uv.x, 0.25);
        assert_approx(uv.y, 0.5);
        assert_approx(uv.width, 1.0 / 256.0);
        assert_approx(uv.height, 1.0 / 128.0);
    }

    #[test]
    fn text_instances_are_transformed_into_canvas_viewport() {
        let viewport = CanvasViewport::from_policy(
            SurfaceSize { width: 1000, height: 1000 },
            CanvasRenderPolicy {
                fit_mode: CanvasFitMode::Contain,
                canvas_size: Some(CanvasSize { width: 16, height: 9 }),
            },
        );
        let mut instances = Vec::new();
        for value in [0.1_f32, 0.2, 0.3, 0.4, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0] {
            instances.extend_from_slice(&value.to_le_bytes());
        }

        viewport.transform_text_instances(&mut instances);
        let floats: Vec<f32> = instances
            .chunks_exact(std::mem::size_of::<f32>())
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        assert_approx(floats[0], 0.1);
        assert_approx(floats[1], 0.33125);
        assert_approx(floats[2], 0.3);
        assert_approx(floats[3], 0.225);
        assert_approx(floats[4], 0.0);
        assert_approx(floats[8], 1.0);
    }

    #[test]
    fn text_wrapping_splits_lines_by_max_width() {
        let Some(font) = load_default_font() else { return };
        let scale = PxScale::from(16.0);
        let scaled_font = font.as_scaled(scale);
        let one_char_width = text_width_px("W", &font, &scaled_font);
        let lines = wrap_text_to_width("WWW", &font, &scaled_font, one_char_width * 1.5);

        assert_eq!(lines, vec!["W", "W", "W"]);
    }

    #[test]
    fn text_shadow_emits_extra_text_instances() {
        let Some(font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 320, height: 240 };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "A".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: Some(crate::plan::TextShadow {
                        color: Color::rgba(0.0, 0.0, 0.0, 0.5),
                        offset: Point { x: 0.01, y: 0.01 },
                    }),
                },
            }],
        };
        let frame = build_text_frame(&plan, &font, &HashMap::new(), &HashMap::new(), surface);

        assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES * 2);
    }

    #[test]
    fn cached_text_frame_only_marks_new_glyphs_dirty() {
        let Some(font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 320, height: 240 };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "FPS 20".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

        let first = build_text_frame_with_cache(
            &plan,
            &font,
            &HashMap::new(),
            &HashMap::new(),
            surface,
            &mut atlas,
        );
        let second = build_text_frame_with_cache(
            &plan,
            &font,
            &HashMap::new(),
            &HashMap::new(),
            surface,
            &mut atlas,
        );

        assert!(!first.instances.is_empty());
        assert!(!first.dirty_regions.is_empty());
        assert_eq!(second.instances.len(), first.instances.len());
        assert!(second.dirty_regions.is_empty());
    }

    #[test]
    fn cached_text_frame_reuses_static_text_layouts() {
        let Some(font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 320, height: 240 };
        let style = TextStyle {
            font_id: None,
            size: 0.1,
            bitmap_size: None,
            color: Color::rgb(1.0, 1.0, 1.0),
            layer: crate::plan::TextLayer::Skin,
            align: TextAlign::Left,
            max_width: 0.0,
            overflow: TextOverflow::Overflow,
            wrapping: false,
            outline: None,
            shadow: None,
        };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                DrawCommand::Text {
                    origin: Point { x: 0.1, y: 0.1 },
                    text: "STATIC".to_string(),
                    caret: None,
                    style: style.clone(),
                },
                DrawCommand::Text {
                    origin: Point { x: 0.1, y: 0.1 },
                    text: "STATIC".to_string(),
                    caret: None,
                    style,
                },
            ],
        };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);

        let first = build_text_frame_with_cache(
            &plan,
            &font,
            &HashMap::new(),
            &HashMap::new(),
            surface,
            &mut atlas,
        );
        let layout_count = atlas.layouts.len();
        let second = build_text_frame_with_cache(
            &plan,
            &font,
            &HashMap::new(),
            &HashMap::new(),
            surface,
            &mut atlas,
        );

        assert!(!first.instances.is_empty());
        assert_eq!(layout_count, 1);
        assert_eq!(second.instances, first.instances);
        assert!(second.dirty_regions.is_empty());
    }

    #[test]
    fn cached_vector_glyph_uses_supersampled_atlas_pixels() {
        let Some(font) = load_default_font() else { return };
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        let Some(glyph) =
            atlas.cached_vector_glyph(DEFAULT_TEXT_FONT_ID, 'A', PxScale::from(24.0), &font)
        else {
            return;
        };

        assert!(glyph.width as f32 > glyph.display_width);
        assert!(glyph.height as f32 > glyph.display_height);
    }

    #[test]
    fn text_atlas_resets_when_height_reaches_limit() {
        let mut atlas = TextAtlasCache::new(TEXT_ATLAS_WIDTH);
        // 上限を超える行を積み、アトラス高さを限界まで成長させる。
        let glyph_height = 64;
        while atlas.atlas_height() < TEXT_ATLAS_MAX_HEIGHT {
            for _ in 0..(TEXT_ATLAS_WIDTH / 32) {
                atlas.reserve(16, glyph_height);
            }
        }
        assert!(atlas.atlas_height() >= TEXT_ATLAS_MAX_HEIGHT);

        // フレーム境界でリセットされ、GPU テクスチャ上限を超えない高さに戻る。
        atlas.begin_frame();
        assert_eq!(atlas.pen_y, 0);
        assert_eq!(atlas.pen_x, 0);
        assert!(atlas.atlas_height() < TEXT_ATLAS_MAX_HEIGHT);
        assert!(atlas.glyphs.is_empty());
        assert_eq!(atlas.layouts.len(), 0);
    }

    #[test]
    fn text_outline_emits_surrounding_text_instances() {
        let Some(font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 320, height: 240 };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "A".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: Some(crate::plan::TextOutline {
                        color: Color::rgba(0.0, 0.0, 0.0, 0.5),
                        width: 0.01,
                    }),
                    shadow: None,
                },
            }],
        };
        let frame = build_text_frame(&plan, &font, &HashMap::new(), &HashMap::new(), surface);

        assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES * 9);
    }

    #[test]
    fn load_default_font_prefers_japanese_capable_font() {
        let Some(font) = load_default_font() else { return };
        // CJK 対応フォントが環境にあれば、必ずそれが採用されていなければならない。
        let cjk_available = bmz_font::resolve_system_font(true).is_some();
        if cjk_available {
            assert!(font_supports_japanese(&font));
        }
    }

    #[test]
    fn japanese_text_emits_glyph_quads_with_default_font() {
        let Some(font) = load_default_font() else { return };
        if !font_supports_japanese(&font) {
            return;
        }
        let surface = SurfaceSize { width: 320, height: 240 };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "日本語と記号★♪".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };
        let frame = build_text_frame(&plan, &font, &HashMap::new(), &HashMap::new(), surface);

        assert!(!frame.instances.is_empty());
        assert!(frame.pixels.contains(&255));
    }

    #[test]
    fn bitmap_font_text_uses_registered_font() {
        let Some(default_font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 320, height: 240 };
        let mut pages = HashMap::new();
        pages.insert(
            0,
            crate::bitmap_font::BitmapFontPage {
                id: 0,
                path: std::path::PathBuf::from("page.png"),
                image: crate::assets::RgbaImageAsset {
                    width: 1,
                    height: 1,
                    pixels: vec![255, 255, 255, 255],
                },
            },
        );
        let mut glyphs = HashMap::new();
        glyphs.insert(
            'A',
            crate::bitmap_font::BitmapFontGlyph {
                id: 'A',
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                xoffset: 0,
                yoffset: 0,
                xadvance: 1,
                page: 0,
            },
        );
        let mut bitmap_fonts = HashMap::new();
        bitmap_fonts.insert(
            "bitmap".to_string(),
            BitmapFont {
                size: 10,
                line_height: 10,
                base: 8,
                ascent: 7.0,
                scale_width: 1,
                scale_height: 1,
                pages,
                glyphs,
            },
        );
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "A".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.1,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };

        let frame = build_text_frame(&plan, &default_font, &HashMap::new(), &bitmap_fonts, surface);

        assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES);
        assert!(frame.pixels.contains(&255));
    }

    #[test]
    fn bitmap_glyph_non_integer_scale_uses_interpolated_alpha() {
        let page = crate::bitmap_font::BitmapFontPage {
            id: 0,
            path: std::path::PathBuf::from("page.png"),
            image: crate::assets::RgbaImageAsset {
                width: 2,
                height: 1,
                pixels: vec![255, 255, 255, 0, 255, 255, 255, 255],
            },
        };
        let glyph = crate::bitmap_font::BitmapFontGlyph {
            id: 'A',
            x: 0,
            y: 0,
            width: 2,
            height: 1,
            xoffset: 0,
            yoffset: 0,
            xadvance: 2,
            page: 0,
        };

        let pixels = rasterized_bitmap_glyph_pixels(glyph, &page, 1.5, 3, 1);
        let middle_alpha = pixels[7];

        assert!(middle_alpha > 0 && middle_alpha < 255);
    }

    #[test]
    fn bitmap_font_text_positions_glyphs_from_destination_baseline() {
        let Some(default_font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 100, height: 100 };
        let mut pages = HashMap::new();
        pages.insert(
            0,
            crate::bitmap_font::BitmapFontPage {
                id: 0,
                path: std::path::PathBuf::from("page.png"),
                image: crate::assets::RgbaImageAsset {
                    width: 1,
                    height: 1,
                    pixels: vec![255, 255, 255, 255],
                },
            },
        );
        let mut glyphs = HashMap::new();
        glyphs.insert(
            'A',
            crate::bitmap_font::BitmapFontGlyph {
                id: 'A',
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                xoffset: 0,
                yoffset: 7,
                xadvance: 1,
                page: 0,
            },
        );
        let mut bitmap_fonts = HashMap::new();
        bitmap_fonts.insert(
            "bitmap".to_string(),
            BitmapFont {
                size: 30,
                line_height: 45,
                base: 34,
                ascent: 12.0,
                scale_width: 1,
                scale_height: 1,
                pages,
                glyphs,
            },
        );
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "A".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.3,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };

        let frame = build_text_frame(&plan, &default_font, &HashMap::new(), &bitmap_fonts, surface);
        let y = f32::from_le_bytes(frame.instances[4..8].try_into().unwrap());

        assert!((y - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn bitmap_font_shrink_keeps_text_vertically_centered_in_destination() {
        let Some(default_font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 100, height: 100 };
        let mut pages = HashMap::new();
        pages.insert(
            0,
            crate::bitmap_font::BitmapFontPage {
                id: 0,
                path: std::path::PathBuf::from("page.png"),
                image: crate::assets::RgbaImageAsset {
                    width: 10,
                    height: 10,
                    pixels: vec![255; 10 * 10 * 4],
                },
            },
        );
        let mut glyphs = HashMap::new();
        glyphs.insert(
            'A',
            crate::bitmap_font::BitmapFontGlyph {
                id: 'A',
                x: 0,
                y: 0,
                width: 10,
                height: 10,
                xoffset: 0,
                yoffset: 7,
                xadvance: 10,
                page: 0,
            },
        );
        let mut bitmap_fonts = HashMap::new();
        bitmap_fonts.insert(
            "bitmap".to_string(),
            BitmapFont {
                size: 10,
                line_height: 10,
                base: 7,
                ascent: 7.0,
                scale_width: 10,
                scale_height: 10,
                pages,
                glyphs,
            },
        );
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "AAAA".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.2,
                    bitmap_size: None,
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.4,
                    overflow: TextOverflow::Shrink,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };

        let frame = build_text_frame(&plan, &default_font, &HashMap::new(), &bitmap_fonts, surface);
        let y = f32::from_le_bytes(frame.instances[4..8].try_into().unwrap());

        assert!((y - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn bitmap_font_text_uses_bitmap_size_for_scale() {
        let Some(default_font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 100, height: 100 };
        let mut pages = HashMap::new();
        pages.insert(
            0,
            crate::bitmap_font::BitmapFontPage {
                id: 0,
                path: std::path::PathBuf::from("page.png"),
                image: crate::assets::RgbaImageAsset {
                    width: 1,
                    height: 1,
                    pixels: vec![255, 255, 255, 255],
                },
            },
        );
        let mut glyphs = HashMap::new();
        glyphs.insert(
            'A',
            crate::bitmap_font::BitmapFontGlyph {
                id: 'A',
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                xoffset: 0,
                yoffset: 0,
                xadvance: 1,
                page: 0,
            },
        );
        let mut bitmap_fonts = HashMap::new();
        bitmap_fonts.insert(
            "bitmap".to_string(),
            BitmapFont {
                size: 10,
                line_height: 10,
                base: 8,
                ascent: 7.0,
                scale_width: 1,
                scale_height: 1,
                pages,
                glyphs,
            },
        );
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "A".to_string(),
                caret: None,
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.3,
                    bitmap_size: Some(0.1),
                    color: Color::rgb(1.0, 1.0, 1.0),
                    layer: crate::plan::TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            }],
        };

        let frame = build_text_frame(&plan, &default_font, &HashMap::new(), &bitmap_fonts, surface);
        let width = f32::from_le_bytes(frame.instances[8..12].try_into().unwrap());

        assert!((width - 0.01).abs() < f32::EPSILON);
    }

    #[test]
    fn rendering_without_surface_is_skipped() {
        let mut renderer = Renderer::default();
        let scene = AppSceneSnapshot::Select(SelectSnapshot::default());

        assert_eq!(
            renderer.render_scene_status(scene).unwrap(),
            RenderSurfaceStatus::SkippedNoSurface
        );
    }

    #[test]
    fn scene_clear_colors_are_distinct() {
        let select = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot::default()));
        let play = DrawPlan::from_scene(&AppSceneSnapshot::Play(Default::default()));

        assert_ne!(select.clear, play.clear);
    }

    fn sample_image(texture: u32, blend: BlendMode) -> DrawCommand {
        DrawCommand::Image {
            rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
            uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: crate::plan::TextureId(texture),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend,
            linear_filter: false,
        }
    }

    fn sample_rect() -> DrawCommand {
        DrawCommand::Rect {
            rect: crate::plan::Rect { x: 0.0, y: 0.0, width: 0.1, height: 0.1 },
            color: Color::rgb(1.0, 1.0, 1.0),
        }
    }

    fn sample_text() -> DrawCommand {
        DrawCommand::Text {
            origin: Point { x: 0.1, y: 0.1 },
            text: "x".to_string(),
            caret: None,
            style: TextStyle {
                font_id: None,
                size: 0.1,
                bitmap_size: None,
                color: Color::rgb(1.0, 1.0, 1.0),
                layer: crate::plan::TextLayer::Skin,
                align: TextAlign::Left,
                max_width: 0.0,
                overflow: TextOverflow::Overflow,
                wrapping: false,
                outline: None,
                shadow: None,
            },
        }
    }

    #[test]
    fn vector_text_caret_rect_uses_font_advance_without_changing_text() {
        let Some(font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 1000, height: 100 };
        let style = TextStyle {
            font_id: None,
            size: 0.1,
            bitmap_size: None,
            color: Color::rgb(1.0, 1.0, 1.0),
            layer: crate::plan::TextLayer::Skin,
            align: TextAlign::Left,
            max_width: 0.0,
            overflow: TextOverflow::Overflow,
            wrapping: false,
            outline: None,
            shadow: None,
        };
        let caret = TextCaret { byte_index: "A".len(), color: Color::rgb(0.8, 0.9, 1.0) };

        let rect =
            vector_text_caret_rect(&Point { x: 0.1, y: 0.2 }, "AB", &style, &font, surface, caret)
                .expect("caret rect");

        let scaled = font.as_scaled(PxScale::from(10.0));
        let expected_x = (100.0 + text_width_px("A", &font, &scaled)) / 1000.0;
        assert_approx(rect.rect.x, expected_x);
        assert_approx(rect.rect.y, 0.2);
        assert_approx(rect.rect.height, 0.1);
        assert_eq!(rect.color, caret.color);
    }

    #[test]
    fn plan_geometry_draws_text_caret_after_text() {
        let mut plan = DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands: vec![sample_text()] };
        let DrawCommand::Text { caret, .. } = &mut plan.commands[0] else {
            unreachable!();
        };
        *caret = Some(TextCaret { byte_index: 1, color: Color::rgb(1.0, 1.0, 1.0) });
        let text_frame = TextFrame {
            command_quad_counts: vec![1],
            command_caret_rects: vec![Some(RectCommand {
                rect: Rect { x: 0.2, y: 0.3, width: 0.01, height: 0.1 },
                color: Color::rgb(1.0, 1.0, 1.0),
            })],
            ..TextFrame::default()
        };

        let geometry = encode_plan_geometry(&plan, &text_frame, test_surface_size());

        assert_eq!(
            geometry.steps,
            vec![
                DrawStep::Text { range: 0..TEXT_INSTANCE_BYTES },
                DrawStep::Rects { range: 0..RECT_INSTANCE_BYTES },
            ]
        );
        assert_eq!(geometry.rects.len(), RECT_INSTANCE_BYTES);
    }

    #[test]
    fn plan_geometry_encodes_one_rect_instance_per_rect_command() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot {
            chart_count: 1,
            ..Default::default()
        }));

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
        let rect_count = plan
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Rect { .. }))
            .count();

        assert_eq!(geometry.rects.len(), rect_count * RECT_INSTANCE_BYTES);
    }

    #[test]
    fn plan_geometry_encodes_rect_batch_instances() {
        let rect = crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::RectBatch {
                rects: std::sync::Arc::from([
                    crate::plan::RectCommand { rect, color: Color::rgb(1.0, 0.0, 0.0) },
                    crate::plan::RectCommand { rect, color: Color::rgb(0.0, 1.0, 0.0) },
                ]),
                cache: None,
            }],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());

        assert_eq!(geometry.rects.len(), RECT_INSTANCE_BYTES * 2);
        assert_eq!(
            geometry.stats(),
            DrawStepStats { steps: 1, rect_steps: 1, rect_instances: 2, ..Default::default() }
        );
    }

    #[test]
    fn plan_geometry_skips_invisible_rects_and_images() {
        let visible_rect = crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
        let zero_width_rect = crate::plan::Rect { width: 0.0, ..visible_rect };
        let mut transparent_image = sample_image(0, BlendMode::Normal);
        let DrawCommand::Image { tint, .. } = &mut transparent_image else { panic!() };
        *tint = Color::rgba(1.0, 1.0, 1.0, 0.0);
        let mut zero_size_image = sample_image(1, BlendMode::Normal);
        let DrawCommand::Image { rect, .. } = &mut zero_size_image else { panic!() };
        rect.height = 0.0;

        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                DrawCommand::Rect { rect: visible_rect, color: Color::rgba(1.0, 0.0, 0.0, 0.0) },
                DrawCommand::Rect { rect: zero_width_rect, color: Color::rgb(0.0, 1.0, 0.0) },
                DrawCommand::RectBatch {
                    rects: std::sync::Arc::from([
                        crate::plan::RectCommand {
                            rect: visible_rect,
                            color: Color::rgba(1.0, 1.0, 1.0, 0.0),
                        },
                        crate::plan::RectCommand {
                            rect: zero_width_rect,
                            color: Color::rgb(1.0, 1.0, 1.0),
                        },
                    ]),
                    cache: None,
                },
                transparent_image,
                zero_size_image,
            ],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());

        assert!(geometry.rects.is_empty());
        assert!(geometry.images.is_empty());
        assert_eq!(geometry.steps, Vec::new());
        assert_eq!(geometry.stats(), DrawStepStats::default());
    }

    #[test]
    fn plan_geometry_can_replace_cached_rect_batch_with_image_instance() {
        let rect = crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
        let cache =
            crate::plan::RectBatchCache { key: crate::plan::RectBatchCacheKey(42), bounds: rect };
        let texture = TextureId(123);
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::RectBatch {
                rects: std::sync::Arc::from([crate::plan::RectCommand {
                    rect,
                    color: Color::rgb(1.0, 0.0, 0.0),
                }]),
                cache: Some(cache),
            }],
        };

        let geometry = encode_plan_geometry_with_rect_batch_resolver(
            &plan,
            &TextFrame::default(),
            test_surface_size(),
            CanvasViewport::from_policy(test_surface_size(), CanvasRenderPolicy::default()),
            &mut |_, _| Some(texture),
        );

        assert!(geometry.rects.is_empty());
        assert_eq!(
            geometry.stats(),
            DrawStepStats { steps: 1, image_steps: 1, image_instances: 1, ..Default::default() }
        );
        assert_eq!(
            geometry.steps,
            vec![DrawStep::Image {
                texture,
                blend: BlendMode::Normal,
                linear: false,
                range: 0..IMAGE_INSTANCE_BYTES,
            }]
        );
    }

    #[test]
    fn plan_geometry_groups_consecutive_images_by_texture() {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                sample_image(0, BlendMode::Normal),
                sample_image(0, BlendMode::Normal),
                sample_image(7, BlendMode::Normal),
            ],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
        let image_step_sizes: Vec<_> = geometry
            .steps
            .iter()
            .filter_map(|step| match step {
                DrawStep::Image { range, .. } => Some(range.len()),
                _ => None,
            })
            .collect();

        assert_eq!(image_step_sizes, vec![IMAGE_INSTANCE_BYTES * 2, IMAGE_INSTANCE_BYTES]);
        assert_eq!(geometry.images.len(), IMAGE_INSTANCE_BYTES * 3);
        assert_eq!(
            geometry.stats(),
            DrawStepStats { steps: 2, image_steps: 2, image_instances: 3, ..Default::default() }
        );
    }

    #[test]
    fn plan_geometry_separates_image_blend_modes() {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![sample_image(0, BlendMode::Normal), sample_image(0, BlendMode::Add)],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
        let blends: Vec<_> = geometry
            .steps
            .iter()
            .filter_map(|step| match step {
                DrawStep::Image { blend, .. } => Some(*blend),
                _ => None,
            })
            .collect();

        assert_eq!(blends, vec![BlendMode::Normal, BlendMode::Add]);
    }

    #[test]
    fn additive_image_blend_uses_source_alpha() {
        let blend = image_blend_state(BlendMode::Add);

        assert_eq!(blend.color.src_factor, wgpu::BlendFactor::SrcAlpha);
        assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
    }

    #[test]
    fn plan_geometry_splits_image_steps_around_other_commands() {
        // 同じテクスチャの画像でも、間に rect を挟めば別ステップになる。
        // rect が2枚の画像の「間」に描かれ、commands の順序が保たれることの回帰テスト。
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                sample_image(1, BlendMode::Normal),
                sample_rect(),
                sample_image(1, BlendMode::Normal),
            ],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());

        assert_eq!(geometry.steps.len(), 3);
        assert!(matches!(geometry.steps[0], DrawStep::Image { .. }));
        assert!(matches!(geometry.steps[1], DrawStep::Rects { .. }));
        assert!(matches!(geometry.steps[2], DrawStep::Image { .. }));
    }

    #[test]
    fn plan_geometry_orders_text_steps_by_command_position() {
        // Text コマンドが Image より前にあれば、描画ステップも Image より前 (背面) になる。
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![sample_text(), sample_image(1, BlendMode::Normal)],
        };
        // sample_text が 2 quad を生成したと仮定したテキストフレーム。
        let text_frame = TextFrame { command_quad_counts: vec![2], ..TextFrame::default() };

        let geometry = encode_plan_geometry(&plan, &text_frame, test_surface_size());

        assert_eq!(geometry.steps.len(), 2);
        assert_eq!(geometry.steps[0], DrawStep::Text { range: 0..TEXT_INSTANCE_BYTES * 2 });
        assert!(matches!(geometry.steps[1], DrawStep::Image { .. }));
        assert_eq!(
            geometry.stats(),
            DrawStepStats {
                steps: 2,
                image_steps: 1,
                text_steps: 1,
                image_instances: 1,
                text_instances: 2,
                ..Default::default()
            }
        );
    }

    #[test]
    fn plan_geometry_writes_rotation_instance_data() {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::RotatedImage {
                rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
                uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                source_size: None,
                texture: crate::plan::TextureId(0),
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                linear_filter: false,
                angle_rad: 1.25,
                center: Point { x: 0.0, y: 1.0 },
            }],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default(), test_surface_size());
        let floats: Vec<f32> = geometry
            .images
            .chunks_exact(std::mem::size_of::<f32>())
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        assert_eq!(floats.len(), IMAGE_INSTANCE_FLOATS);
        assert_eq!(floats[12], 1.25);
        assert_eq!(floats[13], 0.0);
        assert_eq!(floats[14], 1.0);
        assert!((floats[15] - 16.0 / 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn validate_rgba_texture_rejects_invalid_payloads() {
        assert!(validate_rgba_texture(1, 1, &[255, 255, 255, 255]).is_ok());
        assert!(validate_rgba_texture(0, 1, &[255, 255, 255, 255]).is_err());
        assert!(validate_rgba_texture(1, 1, &[255]).is_err());
    }

    #[test]
    fn renderer_queues_texture_assets_before_surface_attach() {
        let mut renderer = Renderer::default();
        let asset =
            crate::assets::RgbaImageAsset { width: 1, height: 1, pixels: vec![255, 0, 0, 255] };

        renderer.upsert_image_asset(crate::plan::TextureId(9), &asset).unwrap();

        assert_eq!(renderer.pending_textures.len(), 1);
        assert_eq!(renderer.pending_textures[0].id, crate::plan::TextureId(9));
    }

    #[test]
    fn installing_vector_font_replaces_stale_bitmap_font_with_same_id() {
        let Some(font) = load_default_font() else { return };
        let mut renderer = Renderer::default();

        renderer.insert_bitmap_font_entry("play:0".to_string(), test_bitmap_font());
        renderer.insert_vector_font("play:0".to_string(), font);

        assert!(renderer.fonts.contains_key("play:0"));
        assert!(!renderer.bitmap_fonts.contains_key("play:0"));
    }

    #[test]
    fn installing_bitmap_font_replaces_stale_vector_font_with_same_id() {
        let Some(font) = load_default_font() else { return };
        let mut renderer = Renderer::default();

        renderer.insert_vector_font("play:0".to_string(), font);
        renderer.insert_bitmap_font_entry("play:0".to_string(), test_bitmap_font());

        assert!(renderer.bitmap_fonts.contains_key("play:0"));
        assert!(!renderer.fonts.contains_key("play:0"));
    }
}
