use super::*;

pub(super) const AMBIENT_MAX_DESTINATIONS: usize = 8;
const MAX_LAYERS: usize = 64;
const BASE_LONG_EDGE: f32 = 128.0;
const MAX_LONG_EDGE: f32 = 1024.0;

pub(super) fn ambient_texture_id(index: usize) -> TextureId {
    TextureId(0xE000_0000 + index as u32)
}

pub(super) struct AmbientResources {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    targets: Vec<AmbientTarget>,
}

struct AmbientTarget {
    size: SurfaceSize,
    source: PreparedTexture,
    intermediate: PreparedTexture,
    source_bind: wgpu::BindGroup,
    intermediate_bind: wgpu::BindGroup,
    blur_buffer: wgpu::Buffer,
    kernel_buffer: wgpu::Buffer,
    kernel_sigma: f32,
    rect_buffer: Option<wgpu::Buffer>,
    image_buffer: Option<wgpu::Buffer>,
    geometry: PlanGeometry,
    bindings: Vec<wgpu::BindGroup>,
}

fn blur_percent(blur: f32) -> f32 {
    if blur.is_finite() { blur.clamp(0.0, 100.0) } else { 50.0 }
}

fn target_long_edge(blur: f32) -> f32 {
    (BASE_LONG_EDGE * 50.0 / blur_percent(blur).max(1.0)).clamp(BASE_LONG_EDGE, MAX_LONG_EDGE)
}

/// Keep the full-screen mode's edge clamping, but give Spread a transparent
/// border wider than the Gaussian support so its output fades before clipping.
pub(super) fn ambient_output_rect(
    rect: Rect,
    blur: f32,
    fade_edges: bool,
    canvas: SurfaceSize,
) -> Rect {
    let blur = blur_percent(blur);
    if !fade_edges || blur == 0.0 || !visible_rect(rect) {
        return rect;
    }
    let long_edge =
        (rect.width.abs() * canvas.width as f32).max(rect.height.abs() * canvas.height as f32);
    let padding = long_edge * (3.0 * blur / 1600.0 + 2.0 / target_long_edge(blur));
    let px = padding / canvas.width.max(1) as f32;
    let py = padding / canvas.height.max(1) as f32;
    Rect {
        x: rect.x - px,
        y: rect.y - py,
        width: rect.width + px * 2.0,
        height: rect.height + py * 2.0,
    }
}

fn target_size(bounds: Rect, canvas: SurfaceSize, blur: f32) -> SurfaceSize {
    if !visible_rect(bounds) {
        return SurfaceSize { width: 1, height: 1 };
    }
    let width = bounds.width.abs() * canvas.width as f32;
    let height = bounds.height.abs() * canvas.height as f32;
    let long_edge = target_long_edge(blur);
    let scale = long_edge / width.max(height).max(1.0);
    SurfaceSize {
        width: (width * scale).round().clamp(1.0, long_edge) as u32,
        height: (height * scale).round().clamp(1.0, long_edge) as u32,
    }
}

/// Header = center weight / pair count; remaining vec4s = offset / pair weight.
/// Pair adjacent Gaussian taps for bilinear sampling (at most radius 24).
fn gaussian_kernel(sigma: f32) -> [[f32; 4]; 13] {
    let mut kernel = [[0.0; 4]; 13];
    kernel[0][0] = 1.0;
    if sigma < 0.001 {
        return kernel;
    }
    let radius = (sigma * 3.0).ceil().clamp(1.0, 24.0) as usize;
    let weight = |x: usize| (-((x * x) as f32) / (2.0 * sigma * sigma)).exp();
    let total = 1.0 + 2.0 * (1..=radius).map(weight).sum::<f32>();
    kernel[0][0] = 1.0 / total;
    for (pair, x) in (1..=radius).step_by(2).enumerate() {
        let a = weight(x);
        let b = if x < radius { weight(x + 1) } else { 0.0 };
        if a + b <= f32::MIN_POSITIVE {
            break;
        }
        kernel[pair + 1] = [x as f32 + b / (a + b), (a + b) / total, 0.0, 0.0];
        kernel[0][1] += 1.0;
    }
    kernel
}

/// Only image/rect leaves are accepted: no nested effects or unbounded render targets.
fn local_layers(bounds: Rect, layers: &[DrawCommand]) -> DrawPlan {
    let local_rect = |rect: Rect| Rect {
        x: (rect.x - bounds.x) / bounds.width,
        y: (rect.y - bounds.y) / bounds.height,
        width: rect.width / bounds.width,
        height: rect.height / bounds.height,
    };
    let commands = if visible_rect(bounds) {
        layers
            .iter()
            .take(MAX_LAYERS)
            .filter_map(|command| {
                if !matches!(
                    command,
                    DrawCommand::Image { .. }
                        | DrawCommand::RotatedImage { .. }
                        | DrawCommand::Rect { .. }
                ) {
                    return None;
                }
                let mut command = command.clone();
                match &mut command {
                    DrawCommand::Image { rect, linear_filter, .. }
                    | DrawCommand::RotatedImage { rect, linear_filter, .. } => {
                        *rect = local_rect(*rect);
                        *linear_filter = true;
                    }
                    DrawCommand::Rect { rect, .. } => *rect = local_rect(*rect),
                    _ => return None,
                }
                Some(command)
            })
            .collect()
    } else {
        Vec::new()
    };
    DrawPlan { clear: Color::rgba(0.0, 0.0, 0.0, 0.0), commands }
}

fn write_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut Option<wgpu::Buffer>,
    bytes: &[u8],
) {
    if bytes.is_empty() {
        return;
    }
    if buffer.as_ref().is_none_or(|buffer| buffer.size() < bytes.len() as u64) {
        *buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render ambient instances"),
            size: bytes.len().next_power_of_two() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    queue.write_buffer(buffer.as_ref().unwrap(), 0, bytes);
}

impl WgpuRenderer {
    fn ambient_target_texture(&self, size: SurfaceSize) -> PreparedTexture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bmz-render ambient target"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        PreparedTexture { texture, view, width: size.width, height: size.height }
    }

    fn ambient_binding(
        &self,
        texture: &PreparedTexture,
        kernel: &wgpu::Buffer,
        layout: &wgpu::BindGroupLayout,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bmz-render ambient binding"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler_linear),
                },
                wgpu::BindGroupEntry { binding: 2, resource: kernel.as_entire_binding() },
            ],
        })
    }

    fn new_ambient_target(
        &mut self,
        index: usize,
        size: SurfaceSize,
        layout: &wgpu::BindGroupLayout,
    ) -> AmbientTarget {
        let source = self.ambient_target_texture(size);
        let intermediate = self.ambient_target_texture(size);
        let kernel_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render ambient kernel"),
            size: 13 * 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let source_bind = self.ambient_binding(&source, &kernel_buffer, layout);
        let intermediate_bind = self.ambient_binding(&intermediate, &kernel_buffer, layout);
        let id = ambient_texture_id(index);
        let output = self.ambient_target_texture(size);
        self.image_textures.insert(id, output);
        self.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
        let mut bytes = Vec::new();
        for (x, y) in [(1.0 / size.width as f32, 0.0), (0.0, 1.0 / size.height as f32)] {
            encode_image_instance(
                &mut bytes,
                &Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                &UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                &Color::rgba(x, y, 0.0, 1.0),
                0.0,
                Point { x: 0.5, y: 0.5 },
                1.0,
                Point { x: 1.0, y: 1.0 },
            );
        }
        let mut blur_buffer = None;
        write_buffer(&self.device, &self.queue, &mut blur_buffer, &bytes);
        AmbientTarget {
            size,
            source,
            intermediate,
            source_bind,
            intermediate_bind,
            blur_buffer: blur_buffer.unwrap(),
            kernel_buffer,
            kernel_sigma: -1.0,
            rect_buffer: None,
            image_buffer: None,
            geometry: PlanGeometry::default(),
            bindings: Vec::new(),
        }
    }

    pub(super) fn prepare_ambient(&mut self, plan: &DrawPlan, canvas: SurfaceSize) {
        let mut groups = plan
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Ambient { rect, blur, fade_edges, layers } => {
                    Some((*rect, *blur, *fade_edges, layers))
                }
                _ => None,
            })
            .take(AMBIENT_MAX_DESTINATIONS)
            .peekable();
        if groups.peek().is_none() {
            if let Some(resources) = self.ambient.take() {
                for index in 0..resources.targets.len() {
                    let id = ambient_texture_id(index);
                    self.image_textures.remove(&id);
                    self.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
                }
            }
            return;
        }
        let mut resources = self.ambient.take().unwrap_or_else(|| {
            let bind_group_layout =
                self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("bmz-render ambient layout"),
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
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });
            let pipeline = create_image_quad_pipeline(
                &self.device,
                self.config.format,
                &bind_group_layout,
                include_str!("ambient.wgsl"),
                "bmz-render ambient blur",
                None,
            );
            AmbientResources { pipeline, bind_group_layout, targets: Vec::new() }
        });
        let mut count = 0;
        for (index, (content, blur, fade_edges, layers)) in groups.enumerate() {
            count += 1;
            let bounds = ambient_output_rect(content, blur, fade_edges, canvas);
            let size = target_size(bounds, canvas, blur);
            if index == resources.targets.len() {
                resources.targets.push(self.new_ambient_target(
                    index,
                    size,
                    &resources.bind_group_layout,
                ));
            } else if resources.targets[index].size != size {
                resources.targets[index] =
                    self.new_ambient_target(index, size, &resources.bind_group_layout);
            }
            let target = &mut resources.targets[index];
            let content_long = (content.width.abs() * canvas.width as f32)
                .max(content.height.abs() * canvas.height as f32);
            let output_long = (bounds.width.abs() * canvas.width as f32)
                .max(bounds.height.abs() * canvas.height as f32)
                .max(1.0);
            let sigma = (content_long / output_long
                * size.width.max(size.height) as f32
                * blur_percent(blur)
                / 1600.0)
                .clamp(0.0, 8.0);
            if target.kernel_sigma != sigma {
                self.queue.write_buffer(
                    &target.kernel_buffer,
                    0,
                    bytemuck::cast_slice(&gaussian_kernel(sigma)),
                );
                target.kernel_sigma = sigma;
            }
            let plan = local_layers(bounds, layers);
            encode_plan_geometry_into(
                &plan,
                &TextFrame::default(),
                size,
                CanvasViewport::from_policy(size, CanvasRenderPolicy::default()),
                &mut |_, _| None,
                &mut target.geometry,
            );
            write_buffer(
                &self.device,
                &self.queue,
                &mut target.rect_buffer,
                &target.geometry.rects,
            );
            write_buffer(
                &self.device,
                &self.queue,
                &mut target.image_buffer,
                &target.geometry.images,
            );
            target.bindings.clear();
            for step in &target.geometry.steps {
                if let DrawStep::Image { texture, linear, .. } = step {
                    target.bindings.push(self.image_bind_group(*texture, *linear));
                }
            }
        }
        for index in count..resources.targets.len() {
            let id = ambient_texture_id(index);
            self.image_textures.remove(&id);
            self.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
        }
        resources.targets.truncate(count);
        self.ambient = Some(resources);
    }

    /// Runs after pending video texture uploads, in the same submission as the main scene.
    pub(super) fn encode_ambient(&self, encoder: &mut wgpu::CommandEncoder) {
        let Some(resources) = &self.ambient else {
            return;
        };
        for (index, target) in resources.targets.iter().enumerate() {
            {
                let mut pass = ambient_pass(encoder, &target.source.view);
                draw_plan_geometry(
                    &mut pass,
                    &target.geometry,
                    PlanGeometryDrawResources {
                        rect_pipeline: &self.rect_pipeline,
                        rect_buffer: target.rect_buffer.as_ref(),
                        image_pipeline: &self.image_pipeline,
                        image_add_pipeline: &self.image_add_pipeline,
                        image_multiply_pipeline: &self.image_multiply_pipeline,
                        image_subtract_pipeline: &self.image_subtract_pipeline,
                        image_premultiplied_pipeline: &self.image_premultiplied_pipeline,
                        image_layer_pipeline: &self.image_layer_pipeline,
                        image_bind_groups: &target.bindings,
                        image_buffer: target.image_buffer.as_ref(),
                        text_pipeline: &self.text_pipeline,
                        text_bind_group: None,
                        text_buffer: None,
                    },
                );
            }
            let output = &self.image_textures[&ambient_texture_id(index)];
            for (axis, (view, binding)) in [
                (&target.intermediate.view, &target.source_bind),
                (&output.view, &target.intermediate_bind),
            ]
            .into_iter()
            .enumerate()
            {
                let mut pass = ambient_pass(encoder, view);
                pass.set_pipeline(&resources.pipeline);
                pass.set_bind_group(0, binding, &[]);
                pass.set_vertex_buffer(0, target.blur_buffer.slice(..));
                pass.draw(0..6, axis as u32..axis as u32 + 1);
            }
        }
    }
}

fn ambient_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bmz-render ambient pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
