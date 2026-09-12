pub(super) struct WgpuRenderer {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) adapter_info: wgpu::AdapterInfo,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) present_modes: Vec<wgpu::PresentMode>,
    pub(super) rect_pipeline: wgpu::RenderPipeline,
    pub(super) rect_buffer: Option<wgpu::Buffer>,
    pub(super) rect_buffer_capacity: usize,
    pub(super) image_pipeline: wgpu::RenderPipeline,
    pub(super) image_add_pipeline: wgpu::RenderPipeline,
    pub(super) image_multiply_pipeline: wgpu::RenderPipeline,
    pub(super) image_premultiplied_pipeline: wgpu::RenderPipeline,
    pub(super) image_layer_pipeline: wgpu::RenderPipeline,
    pub(super) image_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) image_sampler: wgpu::Sampler,
    pub(super) image_sampler_linear: wgpu::Sampler,
    pub(super) upscale_pipeline: wgpu::RenderPipeline,
    pub(super) upscale_buffer: wgpu::Buffer,
    pub(super) upscale_rect: Option<Rect>,
    pub(super) internal_scene_target: Option<InternalSceneTarget>,
    pub(super) image_textures: HashMap<TextureId, PreparedTexture>,
    pub(super) image_bind_group_cache: HashMap<(TextureId, bool), wgpu::BindGroup>,
    pub(super) image_bind_group_scratch: Vec<wgpu::BindGroup>,
    pub(super) geometry_scratch: PlanGeometry,
    pub(super) offscreen_rect_batches: HashMap<OffscreenRectBatchTextureKey, TextureId>,
    pub(super) next_offscreen_rect_batch_texture_id: u32,
    pub(super) image_buffer: Option<wgpu::Buffer>,
    pub(super) image_buffer_capacity: usize,
    pub(super) text_pipeline: wgpu::RenderPipeline,
    pub(super) text_bind_group_layout: wgpu::BindGroupLayout,
    pub(super) text_sampler: wgpu::Sampler,
    pub(super) text_texture: Option<wgpu::Texture>,
    pub(super) text_texture_view: Option<wgpu::TextureView>,
    pub(super) text_bind_group: Option<wgpu::BindGroup>,
    pub(super) text_texture_size: AtlasSize,
    pub(super) text_atlas: TextAtlasCache,
    pub(super) text_buffer: Option<wgpu::Buffer>,
    pub(super) text_buffer_capacity: usize,
    pub(super) default_fonts: FontFallbackChain,
    pub(super) default_font_coverage: bmz_font::FontCoverage,
    pub(super) default_font_search_paths: Vec<PathBuf>,
    pub(super) egui: EguiPainter,
    pub(super) pending_screenshot_readbacks: Vec<ScreenshotReadback>,
    pub(super) screenshot_save_jobs: Vec<ScreenshotSaveJob>,
    // Drop the surface after GPU resources so Linux native contexts are
    // released before the window/display teardown.
    pub(super) surface: Option<wgpu::Surface<'static>>,
    pub(super) export_target: Option<wgpu::Texture>,
}

pub(super) struct InternalSceneTarget {
    pub(super) _texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) size: SurfaceSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct OffscreenRectBatchTextureKey {
    pub(super) key: RectBatchCacheKey,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) const OFFSCREEN_RECT_BATCH_TEXTURE_BASE: u32 = 0xF000_0000;
pub(super) const OFFSCREEN_RECT_BATCH_TEXTURE_MAX_ENTRIES: usize = 64;

#[derive(Debug, Clone)]
pub(super) struct PendingTexture {
    pub(super) id: TextureId,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
}

/// GPU へアップロード済みのテクスチャ。別スレッド (skin upload worker) で
/// 生成してメインスレッドへ送るために公開する。`wgpu::Texture` /
/// `wgpu::TextureView` はどちらも `Send` なのでスレッド間で受け渡しできる。
pub struct PreparedTexture {
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
    pub(super) width: u32,
    pub(super) height: u32,
}
use super::*;
