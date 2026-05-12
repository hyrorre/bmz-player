use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;

use ab_glyph::{Font, FontArc, Glyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result, anyhow};

use crate::assets::{RgbaImageAsset, load_png_rgba};
use crate::plan::{
    Color, DrawCommand, DrawPlan, Point, TextAlign, TextOverflow, TextStyle, TextureId,
};
use crate::scene::AppSceneSnapshot;
use crate::skin::SkinContext;

#[derive(Default)]
pub struct Renderer {
    last_scene: Option<AppSceneSnapshot>,
    last_plan: Option<DrawPlan>,
    skin_context: SkinContext,
    pending_textures: Vec<PendingTexture>,
    gpu: Option<WgpuRenderer>,
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
    image_bind_group_layout: wgpu::BindGroupLayout,
    image_sampler: wgpu::Sampler,
    image_textures: HashMap<TextureId, GpuTexture>,
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
}

#[derive(Debug, Clone)]
struct PendingTexture {
    id: TextureId,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

struct GpuTexture {
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

        let mut gpu = WgpuRenderer::new(window, size)?;
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

    pub fn load_png_texture(&mut self, id: TextureId, path: &std::path::Path) -> Result<()> {
        let asset = load_png_rgba(path)?;
        self.upsert_image_asset(id, &asset)
    }

    pub fn set_skin_context(&mut self, skin_context: SkinContext) {
        self.skin_context = skin_context;
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
        let plan = DrawPlan::from_scene_with_skin(&scene, &self.skin_context);
        self.last_scene = Some(scene);
        self.last_plan = Some(plan);

        self.render_last_plan()
    }

    pub fn render_last_plan(&mut self) -> Result<RenderSurfaceStatus> {
        let Some(gpu) = &mut self.gpu else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };
        let Some(plan) = &self.last_plan else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };

        gpu.render_plan(plan)
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
            .field("gpu_attached", &self.gpu.is_some())
            .finish()
    }
}

impl WgpuRenderer {
    fn new<T>(window: T, size: SurfaceSize) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>>,
    {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).context("failed to create wgpu surface")?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .context("no compatible GPU adapter found")?;
        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("bmz-render device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ))
        .context("failed to request wgpu device")?;
        let config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("surface is not supported by the selected adapter"))?;
        surface.configure(&device, &config);
        let rect_pipeline = create_rect_pipeline(&device, config.format);
        let image_bind_group_layout = create_image_bind_group_layout(&device);
        let image_sampler = create_image_sampler(&device);
        let image_pipeline =
            create_image_pipeline(&device, config.format, &image_bind_group_layout);
        let image_textures = create_default_image_textures(&device, &queue);
        let text_bind_group_layout = create_text_bind_group_layout(&device);
        let text_sampler = create_text_sampler(&device);
        let text_pipeline = create_text_pipeline(&device, config.format, &text_bind_group_layout);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            rect_buffer: None,
            rect_buffer_capacity: 0,
            image_pipeline,
            image_bind_group_layout,
            image_sampler,
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

    fn render_plan(&mut self, plan: &DrawPlan) -> Result<RenderSurfaceStatus> {
        if !(SurfaceSize { width: self.config.width, height: self.config.height }).is_drawable() {
            return Ok(RenderSurfaceStatus::SkippedZeroSize);
        }

        let rects = encode_rects(plan);
        let image_batches = encode_image_batches(plan);
        let images_len = image_batches.iter().map(|batch| batch.instances.len()).sum();
        let text_frame = self.build_text_frame(plan);
        self.ensure_rect_buffer(rects.len());
        if let Some(buffer) = &self.rect_buffer {
            if !rects.is_empty() {
                self.queue.write_buffer(buffer, 0, &rects);
            }
        }
        self.ensure_image_buffer(images_len);
        if let Some(buffer) = &self.image_buffer {
            if images_len > 0 {
                let mut offset = 0;
                for batch in &image_batches {
                    self.queue.write_buffer(buffer, offset, &batch.instances);
                    offset += batch.instances.len() as u64;
                }
            }
        }
        self.upload_text_frame(&text_frame);

        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.configure_surface();
                return Ok(RenderSurfaceStatus::Reconfigured);
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(RenderSurfaceStatus::TimedOut),
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err(anyhow!("wgpu surface is out of memory"));
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let image_bind_groups: Vec<_> =
            image_batches.iter().map(|batch| self.image_bind_group(batch.texture)).collect();
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let Some(buffer) = &self.rect_buffer {
                let instance_count = (rects.len() / RECT_INSTANCE_BYTES) as u32;
                if instance_count > 0 {
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_vertex_buffer(0, buffer.slice(..rects.len() as u64));
                    pass.draw(0..6, 0..instance_count);
                }
            }
            if let Some(buffer) = &self.image_buffer {
                let mut offset = 0_u64;
                for (batch, bind_group) in image_batches.iter().zip(image_bind_groups.iter()) {
                    let instance_count = (batch.instances.len() / IMAGE_INSTANCE_BYTES) as u32;
                    if instance_count > 0 {
                        let end = offset + batch.instances.len() as u64;
                        pass.set_pipeline(&self.image_pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_vertex_buffer(0, buffer.slice(offset..end));
                        pass.draw(0..6, 0..instance_count);
                        offset = end;
                    }
                }
            }
            if let Some(bind_group) = &text_bind_group {
                if let Some(buffer) = &self.text_buffer {
                    let instance_count = (text_frame.instances.len() / TEXT_INSTANCE_BYTES) as u32;
                    if instance_count > 0 {
                        pass.set_pipeline(&self.text_pipeline);
                        pass.set_bind_group(0, bind_group, &[]);
                        pass.set_vertex_buffer(
                            0,
                            buffer.slice(..text_frame.instances.len() as u64),
                        );
                        pass.draw(0..6, 0..instance_count);
                    }
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
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

    fn build_text_frame(&self, plan: &DrawPlan) -> TextFrame {
        let Some(font) = &self.font else {
            return TextFrame::default();
        };
        build_text_frame(
            plan,
            font,
            SurfaceSize { width: self.config.width, height: self.config.height },
        )
    }

    fn upload_text_frame(&mut self, frame: &TextFrame) {
        self.ensure_text_buffer(frame.instances.len());
        if let Some(buffer) = &self.text_buffer {
            if !frame.instances.is_empty() {
                self.queue.write_buffer(buffer, 0, &frame.instances);
            }
        }

        if frame.pixels.is_empty() || frame.size.width == 0 || frame.size.height == 0 {
            return;
        }

        self.ensure_text_texture(frame.size);
        let texture = self.text_texture.as_ref().expect("text texture exists after ensure");
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(frame.size.width),
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
            format: wgpu::TextureFormat::R8Unorm,
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

    fn image_bind_group(&self, texture_id: TextureId) -> wgpu::BindGroup {
        let texture = self.image_textures.get(&texture_id).unwrap_or_else(|| {
            self.image_textures.get(&TextureId(0)).expect("fallback texture is registered")
        });
        let _keep_texture_alive = &texture.texture;
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
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                },
            ],
        })
    }
}

const RECT_INSTANCE_FLOATS: usize = 8;
const RECT_INSTANCE_BYTES: usize = RECT_INSTANCE_FLOATS * std::mem::size_of::<f32>();
const IMAGE_INSTANCE_FLOATS: usize = 12;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageBatch {
    texture: TextureId,
    instances: Vec<u8>,
}

fn encode_rects(plan: &DrawPlan) -> Vec<u8> {
    let mut bytes = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Rect { rect, color } = command else {
            continue;
        };
        for value in [rect.x, rect.y, rect.width, rect.height, color.r, color.g, color.b, color.a] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

fn encode_image_batches(plan: &DrawPlan) -> Vec<ImageBatch> {
    let mut batches = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Image { rect, uv, texture, tint } = command else {
            continue;
        };
        let batch_index = batches
            .iter()
            .position(|batch: &ImageBatch| batch.texture == *texture)
            .unwrap_or_else(|| {
                batches.push(ImageBatch { texture: *texture, instances: Vec::new() });
                batches.len() - 1
            });
        let batch = &mut batches[batch_index];
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
        ] {
            batch.instances.extend_from_slice(&value.to_le_bytes());
        }
    }
    batches
}

fn build_text_frame(plan: &DrawPlan, font: &FontArc, surface: SurfaceSize) -> TextFrame {
    if !surface.is_drawable() {
        return TextFrame::default();
    }

    let mut builder = TextAtlasBuilder::new(TEXT_ATLAS_WIDTH);
    for command in &plan.commands {
        let DrawCommand::Text { origin, text, style } = command else {
            continue;
        };
        builder.push_text(origin, text, *style, font, surface);
    }
    builder.finish()
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

    fn push_text(
        &mut self,
        origin: &Point,
        text: &str,
        style: TextStyle,
        font: &FontArc,
        surface: SurfaceSize,
    ) {
        if let Some(shadow) = style.shadow.filter(|shadow| shadow.color.a > 0.0) {
            let mut shadow_style = style;
            shadow_style.color = shadow.color;
            shadow_style.outline = None;
            shadow_style.shadow = None;
            let shadow_origin =
                Point { x: origin.x + shadow.offset.x, y: origin.y + shadow.offset.y };
            self.push_text(&shadow_origin, text, shadow_style, font, surface);
        }
        if let Some(outline) = style.outline.filter(|outline| outline.color.a > 0.0) {
            let mut outline_style = style;
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
                self.push_text(&outline_origin, text, outline_style, font, surface);
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
                    origin_x, baseline_y, line, line_width, scale, style, font, surface,
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
                        let index = (dst_y * self.width + dst_x) as usize;
                        if let Some(pixel) = self.pixels.get_mut(index) {
                            *pixel = ((*pixel as f32).max(coverage * 255.0)) as u8;
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
        let needed = (self.width * height) as usize;
        if self.pixels.len() < needed {
            self.pixels.resize(needed, 0);
        }
    }

    fn atlas_height(&self) -> u32 {
        (self.pen_y + self.row_height).max(1)
    }

    fn finish(mut self) -> TextFrame {
        let height = self.atlas_height();
        self.pixels.resize((self.width * height) as usize, 0);
        let instances = encode_text_quads(&self.quads, self.width, height);
        TextFrame { size: AtlasSize { width: self.width, height }, pixels: self.pixels, instances }
    }
}

fn text_width_px<F: Font>(text: &str, font: &FontArc, scaled_font: &impl ScaleFont<F>) -> f32 {
    text.chars().map(|ch| scaled_font.h_advance(font.glyph_id(ch))).sum()
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
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render rect pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
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
            entry_point: "fs_main",
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
        multiview: None,
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
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    })
}

fn create_default_image_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> HashMap<TextureId, GpuTexture> {
    let mut textures = HashMap::new();
    textures.insert(
        TextureId(0),
        create_rgba_texture(device, queue, TextureId(0), 1, 1, &[255, 255, 255, 255]),
    );
    textures
}

fn create_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: TextureId,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> GpuTexture {
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
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    tracing::debug!(texture_id = id.0, width, height, "registered render image texture");
    GpuTexture { texture, view }
}

fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bmz-render image shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(IMAGE_SHADER)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bmz-render image pipeline layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render image pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
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
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
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
        multiview: None,
    })
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
        mipmap_filter: wgpu::FilterMode::Nearest,
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
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render text pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
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
            entry_point: "fs_main",
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
        multiview: None,
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
    out.tint = tint;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(image_texture, image_sampler, input.uv) * input.tint;
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
    let alpha = textureSample(text_atlas, text_sampler, input.uv).r;
    return vec4<f32>(input.color.rgb, input.color.a * alpha);
}
"#;

fn load_default_font() -> Option<FontArc> {
    for path in default_font_candidates() {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        match FontArc::try_from_vec(bytes) {
            Ok(font) => return Some(font),
            Err(error) => tracing::warn!(%error, path, "failed to load default render font"),
        }
    }
    tracing::warn!("no default render font found; text draw commands will be skipped");
    None
}

fn default_font_candidates() -> &'static [&'static str] {
    &[
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ]
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
        let frame = build_text_frame(&plan, &font, surface);

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
        let frame = build_text_frame(&plan, &font, surface);

        assert_eq!(frame.instances.len(), TEXT_INSTANCE_BYTES * 9);
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

    #[test]
    fn encode_rects_writes_one_instance_per_rect_command() {
        let plan = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot {
            chart_count: 1,
            ..Default::default()
        }));

        let bytes = encode_rects(&plan);
        let rect_count = plan
            .commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::Rect { .. }))
            .count();

        assert_eq!(bytes.len(), rect_count * RECT_INSTANCE_BYTES);
    }

    #[test]
    fn encode_image_batches_groups_instances_by_texture() {
        let plan = DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![
                DrawCommand::Image {
                    rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
                    uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    texture: crate::plan::TextureId(0),
                    tint: Color::rgb(1.0, 1.0, 1.0),
                },
                DrawCommand::Image {
                    rect: crate::plan::Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
                    uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    texture: crate::plan::TextureId(7),
                    tint: Color::rgb(1.0, 1.0, 1.0),
                },
            ],
        };

        let batches = encode_image_batches(&plan);

        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].instances.len(), IMAGE_INSTANCE_BYTES);
        assert_eq!(batches[1].instances.len(), IMAGE_INSTANCE_BYTES);
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
