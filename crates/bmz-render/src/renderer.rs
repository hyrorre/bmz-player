use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;

use anyhow::{Context, Result, anyhow};

use crate::plan::DrawPlan;
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
        let clear_color = plan.clear.to_wgpu();
        self.last_scene = Some(scene);
        self.last_plan = Some(plan);

        if let Some(gpu) = &mut self.gpu {
            gpu.render_clear(clear_color)?;
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

        Ok(Self { surface, device, queue, config })
    }

    fn resize(&mut self, size: SurfaceSize) {
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render_clear(&mut self, color: wgpu::Color) -> Result<()> {
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
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bmz-render clear pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
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
