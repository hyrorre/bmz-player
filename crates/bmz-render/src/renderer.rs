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

use ab_glyph::{Font, FontArc, FontVec, Glyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result, anyhow};

use crate::assets::{RgbaImageAsset, load_png_rgba};
use crate::bitmap_font::{BitmapFont, load_bitmap_font};
use crate::plan::{
    Color, DrawCommand, DrawPlan, Point, Rect, TextAlign, TextOverflow, TextStyle, TextureId,
    UvRect,
};
use crate::scene::AppSceneSnapshot;
use crate::skin::{BlendMode, DynamicTimerRuntime, SkinClickHit, SkinContext, SkinDocument};
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

impl WgpuBackend {
    pub fn to_wgpu(self) -> wgpu::Backends {
        match self {
            Self::Auto => wgpu::Backends::all(),
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Metal => wgpu::Backends::METAL,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Gl => wgpu::Backends::GL,
        }
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
    pending_textures: Vec<PendingTexture>,
    fonts: HashMap<String, FontArc>,
    bitmap_fonts: HashMap<String, BitmapFont>,
    gpu: Option<WgpuRenderer>,
    pending_egui: Option<EguiFrame>,
    pending_screenshot_path: Option<PathBuf>,
    play_dynamic_timer_runtime: DynamicTimerRuntime,
    select_dynamic_timer_runtime: DynamicTimerRuntime,
    decide_dynamic_timer_runtime: DynamicTimerRuntime,
    result_dynamic_timer_runtime: DynamicTimerRuntime,
    /// VSync の希望状態。サーフェス生成時および `set_vsync` で参照する。
    vsync: bool,
    backend: WgpuBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
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
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
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
    image_buffer: Option<wgpu::Buffer>,
    image_buffer_capacity: usize,
    text_pipeline: wgpu::RenderPipeline,
    text_bind_group_layout: wgpu::BindGroupLayout,
    text_sampler: wgpu::Sampler,
    text_texture: Option<wgpu::Texture>,
    text_texture_view: Option<wgpu::TextureView>,
    text_texture_size: AtlasSize,
    text_buffer: Option<wgpu::Buffer>,
    text_buffer_capacity: usize,
    font: Option<FontArc>,
    egui: EguiPainter,
}

struct ScreenshotCapture {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
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

    fn save_png(&self, device: &wgpu::Device, path: &Path) -> Result<()> {
        let slice = self.buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()
            .context("screenshot readback callback dropped")?
            .context("failed to map screenshot buffer")?;

        let mapped = slice.get_mapped_range();
        let mut rgba = vec![0; self.width as usize * self.height as usize * 4];
        let row_bytes = self.width as usize * 4;
        let padded_row_bytes = self.padded_bytes_per_row as usize;
        for y in 0..self.height as usize {
            let src_offset = y * padded_row_bytes;
            let dst_offset = y * row_bytes;
            rgba[dst_offset..dst_offset + row_bytes]
                .copy_from_slice(&mapped[src_offset..src_offset + row_bytes]);
        }
        drop(mapped);
        self.buffer.unmap();

        if matches!(
            self.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        image::save_buffer_with_format(
            path,
            &rgba,
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .with_context(|| format!("failed to save screenshot {}", path.display()))
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
}

impl Renderer {
    pub fn attach_surface<T>(&mut self, window: T, size: SurfaceSize) -> Result<()>
    where
        T: Into<wgpu::SurfaceTarget<'static>>,
    {
        if !size.is_drawable() {
            self.gpu = None;
            return Ok(());
        }

        let mut gpu = WgpuRenderer::new(window, size, self.vsync, self.backend)?;
        for texture in self.pending_textures.drain(..) {
            gpu.upsert_rgba_texture(texture.id, texture.width, texture.height, &texture.rgba);
        }
        self.gpu = Some(gpu);
        Ok(())
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

    pub fn upsert_image_asset(&mut self, id: TextureId, asset: &RgbaImageAsset) -> Result<()> {
        asset.validate()?;
        self.upsert_rgba_texture(id, asset.width, asset.height, asset.pixels.clone())
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
        self.fonts.insert(id, font);
        Ok(())
    }

    pub fn load_bitmap_font(
        &mut self,
        id: impl Into<String>,
        path: &std::path::Path,
    ) -> Result<()> {
        let font = load_bitmap_font(path)?;
        self.bitmap_fonts.insert(id.into(), font);
        Ok(())
    }

    /// 事前に読み込んだフォントバイト列を登録する。
    /// バックグラウンドスレッドで I/O を済ませた後に main スレッドから登録する用途。
    pub fn install_font_bytes(&mut self, id: impl Into<String>, bytes: Vec<u8>) -> Result<()> {
        let font = FontArc::try_from_vec(bytes)
            .map_err(|error| anyhow!("failed to parse font bytes: {error}"))?;
        self.fonts.insert(id.into(), font);
        Ok(())
    }

    /// 事前にパース済みの bitmap font を登録する。
    pub fn install_bitmap_font(&mut self, id: impl Into<String>, font: BitmapFont) {
        self.bitmap_fonts.insert(id.into(), font);
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
        self.result_dynamic_timer_runtime.reset();
        self.result_skin_context = skin_context;
    }

    fn dynamic_timer_runtime_for_scene(
        &mut self,
        scene: &AppSceneSnapshot,
    ) -> &mut DynamicTimerRuntime {
        match scene {
            AppSceneSnapshot::Select(_) => &mut self.select_dynamic_timer_runtime,
            AppSceneSnapshot::Decide(_) => &mut self.decide_dynamic_timer_runtime,
            AppSceneSnapshot::Play(_) => &mut self.play_dynamic_timer_runtime,
            AppSceneSnapshot::Result(_) => &mut self.result_dynamic_timer_runtime,
        }
    }

    /// リザルトスキンが宣言する終了フェードアウト時間 (ms)。
    /// ドキュメントスキンが無い場合や未指定の場合は 0 を返す。
    pub fn result_skin_fadeout_ms(&self) -> i32 {
        self.result_skin_context.document().map(|document| document.fadeout).unwrap_or(0).max(0)
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
        self.select_skin_context.select_click_hit(snapshot, x, y)
    }

    /// プレイスキンの document。
    pub fn play_skin_document(&self) -> Option<&SkinDocument> {
        self.play_skin_context.document()
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

    fn skin_context_for_scene(&self, scene: &AppSceneSnapshot) -> &SkinContext {
        match scene {
            AppSceneSnapshot::Select(_) => &self.select_skin_context,
            AppSceneSnapshot::Decide(_) => &self.decide_skin_context,
            AppSceneSnapshot::Play(_) => &self.play_skin_context,
            AppSceneSnapshot::Result(_) => &self.result_skin_context,
        }
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
        let skin = self.skin_context_for_scene(&scene).clone();
        let dynamic_timers = self.dynamic_timer_runtime_for_scene(&scene);
        let plan = DrawPlan::from_scene_with_skin(&scene, &skin, dynamic_timers);
        self.last_scene = Some(scene);
        self.last_plan = Some(plan);

        self.render_last_plan()
    }

    /// 次の描画フレームで重ねる egui の描画データを差し込む。
    ///
    /// `render_scene_status` / `render_last_plan` の呼び出しで消費される。
    pub fn set_egui_frame(&mut self, frame: EguiFrame) {
        self.pending_egui = Some(frame);
    }

    /// VSync の有効/無効を設定する。
    ///
    /// サーフェス生成済みなら present mode を即座に再構成する (要再起動なし)。
    /// 値が変わらない場合は何もしないため、毎フレーム呼んでも安全。
    pub fn set_vsync(&mut self, vsync: bool) {
        if self.vsync == vsync {
            return;
        }
        self.vsync = vsync;
        if let Some(gpu) = &mut self.gpu {
            gpu.set_present_mode(vsync);
            tracing::info!(vsync, "vsync updated");
        }
    }

    pub fn set_backend(&mut self, backend: WgpuBackend) {
        self.backend = backend;
    }

    pub fn render_last_plan(&mut self) -> Result<RenderSurfaceStatus> {
        let egui = self.pending_egui.take();
        let screenshot_path = self.pending_screenshot_path.take();
        let Some(gpu) = &mut self.gpu else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };
        let Some(plan) = &self.last_plan else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };

        gpu.render_plan(
            plan,
            &self.fonts,
            &self.bitmap_fonts,
            egui.as_ref(),
            screenshot_path.as_deref(),
        )
    }

    pub fn request_screenshot(&mut self, path: impl Into<PathBuf>) {
        self.pending_screenshot_path = Some(path.into());
    }

    pub fn last_scene(&self) -> Option<&AppSceneSnapshot> {
        self.last_scene.as_ref()
    }

    pub fn last_plan(&self) -> Option<&DrawPlan> {
        self.last_plan.as_ref()
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
    fn new<T>(window: T, size: SurfaceSize, vsync: bool, backend: WgpuBackend) -> Result<Self>
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
        let mut config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("surface is not supported by the selected adapter"))?;
        // sRGB フレームバッファだと PNG の sRGB 値が二重 gamma エンコードされて白っぽくなる。
        // beatoraja (libGDX) は GL_FRAMEBUFFER_SRGB を使わないため値をそのまま表示する。
        // それと合わせるため sRGB サフィックスを除去して non-sRGB サーフェスとして使う。
        config.format = config.format.remove_srgb_suffix();
        config.present_mode = present_mode_for(vsync);
        config.usage |= wgpu::TextureUsages::COPY_SRC;
        surface.configure(&device, &config);
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
            surface,
            device,
            queue,
            config,
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
            image_buffer: None,
            image_buffer_capacity: 0,
            text_pipeline,
            text_bind_group_layout,
            text_sampler,
            text_texture: None,
            text_texture_view: None,
            text_texture_size: AtlasSize::default(),
            text_buffer: None,
            text_buffer_capacity: 0,
            font: load_default_font(),
            egui,
        })
    }

    fn resize(&mut self, size: SurfaceSize) {
        if !size.is_drawable() {
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.configure_surface();
    }

    fn render_plan(
        &mut self,
        plan: &DrawPlan,
        fonts: &HashMap<String, FontArc>,
        bitmap_fonts: &HashMap<String, BitmapFont>,
        egui: Option<&EguiFrame>,
        screenshot_path: Option<&Path>,
    ) -> Result<RenderSurfaceStatus> {
        // egui のテクスチャ更新は、描画をスキップするフレームでも必ず適用する。
        // TexturesDelta は累積ストリームのため、取りこぼすと後続フレームの
        // 部分更新が未確保テクスチャを参照して panic する。
        if let Some(frame) = egui {
            self.egui.update_textures(&self.device, &self.queue, frame);
        }

        if !(SurfaceSize { width: self.config.width, height: self.config.height }).is_drawable() {
            return Ok(RenderSurfaceStatus::SkippedZeroSize);
        }

        let text_frame = self.build_text_frame(plan, fonts, bitmap_fonts);
        let geometry = encode_plan_geometry(plan, &text_frame);
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

        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure_surface();
                return Ok(RenderSurfaceStatus::Reconfigured);
            }
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(RenderSurfaceStatus::TimedOut),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
                return Ok(RenderSurfaceStatus::TimedOut);
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // image ステップごとの bind group を、レンダーパスが encoder を借りる前に作る。
        // steps 内の image ステップと同じ順序で並ぶ。
        let image_bind_groups: Vec<wgpu::BindGroup> = geometry
            .steps
            .iter()
            .filter_map(|step| match step {
                DrawStep::Image { texture, linear, .. } => {
                    Some(self.image_bind_group(*texture, *linear))
                }
                _ => None,
            })
            .collect();
        let text_bind_group = self.text_bind_group();
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

        let screenshot = screenshot_path.map(|path| {
            let capture = ScreenshotCapture::new(
                &self.device,
                self.config.width,
                self.config.height,
                self.config.format,
            );
            capture.copy_from_surface(&mut encoder, &output.texture);
            (path.to_path_buf(), capture)
        });
        self.queue.submit(egui_staging.into_iter().chain(std::iter::once(encoder.finish())));
        if let Some((path, capture)) = screenshot {
            capture.save_png(&self.device, &path)?;
            tracing::info!(path = %path.display(), "smoke screenshot saved");
        }
        if let Some(frame) = egui {
            self.egui.free_textures(frame);
        }
        output.present();
        Ok(RenderSurfaceStatus::Rendered)
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

    fn configure_surface(&self) {
        self.surface.configure(&self.device, &self.config);
    }

    fn set_present_mode(&mut self, vsync: bool) {
        self.config.present_mode = present_mode_for(vsync);
        self.configure_surface();
    }

    fn build_text_frame(
        &self,
        plan: &DrawPlan,
        fonts: &HashMap<String, FontArc>,
        bitmap_fonts: &HashMap<String, BitmapFont>,
    ) -> TextFrame {
        let Some(font) = &self.font else {
            return TextFrame::default();
        };
        build_text_frame(
            plan,
            font,
            fonts,
            bitmap_fonts,
            SurfaceSize { width: self.config.width, height: self.config.height },
        )
    }

    fn upload_text_frame(&mut self, frame: &TextFrame) {
        self.ensure_text_buffer(frame.instances.len());
        if let Some(buffer) = &self.text_buffer
            && !frame.instances.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &frame.instances);
        }

        if frame.pixels.is_empty() || frame.size.width == 0 || frame.size.height == 0 {
            return;
        }

        self.ensure_text_texture(frame.size);
        let texture = self.text_texture.as_ref().expect("text texture exists after ensure");
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
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
        self.text_texture_size = size;
    }

    fn text_bind_group(&self) -> Option<wgpu::BindGroup> {
        let view = self.text_texture_view.as_ref()?;
        Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        }))
    }

    fn upsert_rgba_texture(&mut self, id: TextureId, width: u32, height: u32, rgba: &[u8]) {
        let texture = create_rgba_texture(&self.device, &self.queue, id, width, height, rgba);
        self.image_textures.insert(id, texture);
    }

    fn image_bind_group(&self, texture_id: TextureId, linear: bool) -> wgpu::BindGroup {
        let texture = self.image_textures.get(&texture_id).unwrap_or_else(|| {
            self.image_textures.get(&TextureId(0)).expect("fallback texture is registered")
        });
        let _keep_texture_alive = &texture.texture;
        let sampler = if linear { &self.image_sampler_linear } else { &self.image_sampler };
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        })
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AtlasSize {
    width: u32,
    height: u32,
}

#[derive(Debug, Default)]
struct TextFrame {
    size: AtlasSize,
    pixels: Vec<u8>,
    instances: Vec<u8>,
    /// `DrawCommand::Text` ごとに生成された quad 数を、commands 内の出現順で持つ。
    /// 描画ステップ単位で text instance buffer をスライスするのに使う。
    command_quad_counts: Vec<usize>,
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
struct PlanGeometry {
    rects: Vec<u8>,
    images: Vec<u8>,
    steps: Vec<DrawStep>,
}

/// commands を 1 回走査し、rect/image インスタンスバッファと、コマンド順を尊重した
/// 描画ステップ列を作る。`text_frame` の `command_quad_counts` から各 Text コマンドが
/// 占める text instance buffer の範囲を割り出す。
fn encode_plan_geometry(plan: &DrawPlan, text_frame: &TextFrame) -> PlanGeometry {
    let mut rects = Vec::new();
    let mut images = Vec::new();
    let mut steps: Vec<DrawStep> = Vec::new();
    // text instance buffer 上での現在位置 (quad 単位) と、次に参照する Text コマンド番号。
    let mut text_quad_cursor = 0_usize;
    let mut text_command_index = 0_usize;

    for command in &plan.commands {
        match command {
            DrawCommand::Rect { rect, color } => {
                let start = rects.len();
                for value in
                    [rect.x, rect.y, rect.width, rect.height, color.r, color.g, color.b, color.a]
                {
                    rects.extend_from_slice(&value.to_le_bytes());
                }
                push_or_extend_rects(&mut steps, start..rects.len());
            }
            DrawCommand::Image { rect, uv, texture, tint, blend, linear_filter } => {
                let start = images.len();
                encode_image_instance(&mut images, rect, uv, tint, 0.0, Point { x: 0.5, y: 0.5 });
                push_or_extend_image(
                    &mut steps,
                    *texture,
                    *blend,
                    *linear_filter,
                    start..images.len(),
                );
            }
            DrawCommand::RotatedImage {
                rect,
                uv,
                texture,
                tint,
                blend,
                linear_filter,
                angle_rad,
                center,
            } => {
                let start = images.len();
                encode_image_instance(&mut images, rect, uv, tint, *angle_rad, *center);
                push_or_extend_image(
                    &mut steps,
                    *texture,
                    *blend,
                    *linear_filter,
                    start..images.len(),
                );
            }
            DrawCommand::Text { .. } => {
                let quad_count =
                    text_frame.command_quad_counts.get(text_command_index).copied().unwrap_or(0);
                text_command_index += 1;
                let start = text_quad_cursor * TEXT_INSTANCE_BYTES;
                text_quad_cursor += quad_count;
                let end = text_quad_cursor * TEXT_INSTANCE_BYTES;
                if quad_count > 0 {
                    push_or_extend_text(&mut steps, start..end);
                }
            }
        }
    }

    PlanGeometry { rects, images, steps }
}

/// image インスタンス 1 件 (16 float) をバッファ末尾へ書き込む。
fn encode_image_instance(
    images: &mut Vec<u8>,
    rect: &Rect,
    uv: &UvRect,
    tint: &Color,
    angle_rad: f32,
    center: Point,
) {
    for value in [
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
        0.0,
    ] {
        images.extend_from_slice(&value.to_le_bytes());
    }
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
    for command in &plan.commands {
        let DrawCommand::Text { origin, text, style } = command else {
            continue;
        };
        let quads_before = builder.quads.len();
        if let Some(bitmap_font) =
            style.font_id.as_ref().and_then(|font_id| bitmap_fonts.get(font_id))
        {
            builder.push_bitmap_text(origin, text, style.clone(), bitmap_font, surface);
        } else {
            let font = style
                .font_id
                .as_ref()
                .and_then(|font_id| fonts.get(font_id))
                .unwrap_or(default_font);
            builder.push_text(origin, text, style.clone(), font, surface);
        }
        command_quad_counts.push(builder.quads.len() - quads_before);
    }
    let mut frame = builder.finish();
    frame.command_quad_counts = command_quad_counts;
    frame
}

struct TextAtlasBuilder {
    width: u32,
    pen_x: u32,
    pen_y: u32,
    row_height: u32,
    pixels: Vec<u8>,
    quads: Vec<TextQuad>,
}

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
        let mut scale = (style.size * surface.height as f32 / design_size as f32).max(0.01);
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
        let align_offset = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center if max_width > 0.0 => (max_width - text_width) / 2.0,
            TextAlign::Right if max_width > 0.0 => max_width - text_width,
            _ => 0.0,
        };
        let mut cursor_x = origin.x * surface.width as f32 + align_offset.max(0.0);
        let text_top_y = origin.y * surface.height as f32;

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
        for dst_y in 0..glyph_height {
            for dst_x in 0..glyph_width {
                let src_x = glyph.x + (dst_x as f32 / scale).floor() as u32;
                let src_y = glyph.y + (dst_y as f32 / scale).floor() as u32;
                if src_x >= page.image.width || src_y >= page.image.height {
                    continue;
                }
                let src_index = ((src_y * page.image.width + src_x) * 4) as usize;
                let Some(src) = page.image.pixels.get(src_index..src_index + 4) else {
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
        let baseline_y = origin.y * surface.height as f32 + scaled_font.ascent();

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
        let align_offset = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center if max_width > 0.0 => (max_width - text_width) / 2.0,
            TextAlign::Right if max_width > 0.0 => max_width - text_width,
            _ => 0.0,
        };
        let mut cursor_x = origin_x + align_offset.max(0.0);

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
            pixels: self.pixels,
            instances,
            command_quad_counts: Vec::new(),
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
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
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
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    tracing::debug!(texture_id = id.0, width, height, "registered render image texture");
    PreparedTexture { texture, view }
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
    let c = cos(rotation.x);
    let s = sin(rotation.x);
    let rotated = vec2<f32>(
        relative.x * c - relative.y * s,
        relative.x * s + relative.y * c,
    );
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
    let c = cos(rotation.x);
    let s = sin(rotation.x);
    let rotated = vec2<f32>(
        relative.x * c - relative.y * s,
        relative.x * s + relative.y * c,
    );
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

/// VSync 設定に対応する wgpu の present mode。
///
/// `Auto*` は環境に応じて `Fifo` / `Immediate` / `Mailbox` へ解決され、
/// 常にサポートされるため安全に使える。
fn present_mode_for(vsync: bool) -> wgpu::PresentMode {
    if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync }
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

    fn font_supports_japanese<F: Font>(font: &F) -> bool {
        font.glyph_id('あ').0 != 0 && font.glyph_id('日').0 != 0
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
        let manifest: SkinManifest = toml::from_str("").unwrap();
        let context =
            SkinContext::from_manifest_and_document(manifest, document.clone(), Vec::new());
        renderer.set_play_skin_context(context, false);

        let state = SkinDrawState::default();
        let seeded = renderer.play_dynamic_timer_runtime.advance(&document, state, 5_000);
        assert_eq!(seeded.dynamic_timer_ms[0], Some(0));

        let progressed = renderer.play_dynamic_timer_runtime.advance(&document, state, 8_000);
        assert_eq!(progressed.dynamic_timer_ms[0], Some(3_000));

        renderer.set_select_skin_context(SkinContext::default());

        let continued = renderer.play_dynamic_timer_runtime.advance(&document, state, 9_000);
        assert_eq!(continued.dynamic_timer_ms[0], Some(4_000));
    }

    #[test]
    fn result_skin_fadeout_ms_reads_document_or_defaults_to_zero() {
        use crate::skin::{SkinContext, SkinDocument, SkinManifest};

        let mut renderer = Renderer::default();
        // ドキュメントスキン未設定なら 0 (フェードアウトなし)。
        assert_eq!(renderer.result_skin_fadeout_ms(), 0);

        let document: SkinDocument =
            serde_json::from_str(r#"{ "type": 7, "w": 100, "h": 100, "fadeout": 300 }"#).unwrap();
        let manifest: SkinManifest = toml::from_str("").unwrap();
        renderer.set_result_skin_context(SkinContext::from_manifest_and_document(
            manifest,
            document,
            [],
        ));

        assert_eq!(renderer.result_skin_fadeout_ms(), 300);
    }

    #[test]
    fn surface_size_requires_non_zero_dimensions() {
        assert!(SurfaceSize { width: 1, height: 1 }.is_drawable());
        assert!(!SurfaceSize { width: 0, height: 1 }.is_drawable());
        assert!(!SurfaceSize { width: 1, height: 0 }.is_drawable());
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
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
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
    fn text_outline_emits_surrounding_text_instances() {
        let Some(font) = load_default_font() else { return };
        let surface = SurfaceSize { width: 320, height: 240 };
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Text {
                origin: Point { x: 0.1, y: 0.1 },
                text: "A".to_string(),
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
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
                style: TextStyle {
                    font_id: None,
                    size: 0.1,
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
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.1,
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
                style: TextStyle {
                    font_id: Some("bitmap".to_string()),
                    size: 0.3,
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
            style: TextStyle {
                font_id: None,
                size: 0.1,
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
    fn plan_geometry_encodes_one_rect_instance_per_rect_command() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot {
            chart_count: 1,
            ..Default::default()
        }));

        let geometry = encode_plan_geometry(&plan, &TextFrame::default());
        let rect_count = plan
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Rect { .. }))
            .count();

        assert_eq!(geometry.rects.len(), rect_count * RECT_INSTANCE_BYTES);
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

        let geometry = encode_plan_geometry(&plan, &TextFrame::default());
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
    }

    #[test]
    fn plan_geometry_separates_image_blend_modes() {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![sample_image(0, BlendMode::Normal), sample_image(0, BlendMode::Add)],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default());
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

        let geometry = encode_plan_geometry(&plan, &TextFrame::default());

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

        let geometry = encode_plan_geometry(&plan, &text_frame);

        assert_eq!(geometry.steps.len(), 2);
        assert_eq!(geometry.steps[0], DrawStep::Text { range: 0..TEXT_INSTANCE_BYTES * 2 });
        assert!(matches!(geometry.steps[1], DrawStep::Image { .. }));
    }

    #[test]
    fn plan_geometry_writes_rotation_instance_data() {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::RotatedImage {
                rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
                uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                texture: crate::plan::TextureId(0),
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                linear_filter: false,
                angle_rad: 1.25,
                center: Point { x: 0.0, y: 1.0 },
            }],
        };

        let geometry = encode_plan_geometry(&plan, &TextFrame::default());
        let floats: Vec<f32> = geometry
            .images
            .chunks_exact(std::mem::size_of::<f32>())
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();

        assert_eq!(floats.len(), IMAGE_INSTANCE_FLOATS);
        assert_eq!(floats[12], 1.25);
        assert_eq!(floats[13], 0.0);
        assert_eq!(floats[14], 1.0);
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
}
