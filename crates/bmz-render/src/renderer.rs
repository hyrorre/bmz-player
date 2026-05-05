use std::borrow::Cow;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;

use anyhow::{Context, Result, anyhow};

use crate::plan::{DrawCommand, DrawPlan};
use crate::scene::AppSceneSnapshot;

#[derive(Default)]
pub struct Renderer {
    last_scene: Option<AppSceneSnapshot>,
    last_plan: Option<DrawPlan>,
    gpu: Option<WgpuRenderer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

impl SurfaceSize {
    pub fn is_drawable(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

struct WgpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    rect_pipeline: wgpu::RenderPipeline,
    rect_buffer: Option<wgpu::Buffer>,
    rect_buffer_capacity: usize,
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

        self.gpu = Some(WgpuRenderer::new(window, size)?);
        Ok(())
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
        let plan = DrawPlan::from_scene(&scene);
        self.last_scene = Some(scene);
        self.last_plan = Some(plan);

        if let Some(gpu) = &mut self.gpu {
            gpu.render_plan(self.last_plan.as_ref().expect("plan was just stored"))?;
        }

        Ok(())
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

        Ok(Self {
            surface,
            device,
            queue,
            config,
            rect_pipeline,
            rect_buffer: None,
            rect_buffer_capacity: 0,
        })
    }

    fn resize(&mut self, size: SurfaceSize) {
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render_plan(&mut self, plan: &DrawPlan) -> Result<()> {
        let rects = encode_rects(plan);
        self.ensure_rect_buffer(rects.len());
        if let Some(buffer) = &self.rect_buffer {
            if !rects.is_empty() {
                self.queue.write_buffer(buffer, 0, &rects);
            }
        }

        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(error) => return Err(error).context("failed to acquire surface texture"),
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
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
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
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
}

const RECT_INSTANCE_FLOATS: usize = 8;
const RECT_INSTANCE_BYTES: usize = RECT_INSTANCE_FLOATS * std::mem::size_of::<f32>();

fn encode_rects(plan: &DrawPlan) -> Vec<u8> {
    let mut bytes = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Rect { rect, color } = command;
        for value in [rect.x, rect.y, rect.width, rect.height, color.r, color.g, color.b, color.a] {
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
    fn scene_clear_colors_are_distinct() {
        let select = DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot::default()));
        let play = DrawPlan::from_scene(&AppSceneSnapshot::Play(Default::default()));

        assert_ne!(select.clear, play.clear);
    }
}
