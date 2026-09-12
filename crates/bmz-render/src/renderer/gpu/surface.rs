use super::*;

impl WgpuRenderer {
    pub(in crate::renderer) fn resize(&mut self, size: SurfaceSize) -> Result<()> {
        if !size.is_drawable() {
            return Ok(());
        }

        anyhow::ensure!(
            self.export_target.is_none() || self.surface_size() == size,
            "offscreen target dimensions are fixed; attach a new target to resize"
        );

        self.config.width = size.width;
        self.config.height = size.height;
        self.clear_internal_scene_target();
        self.clear_offscreen_rect_batches();
        self.configure_surface()
    }

    pub(in crate::renderer) fn surface_size(&self) -> SurfaceSize {
        SurfaceSize { width: self.config.width, height: self.config.height }
    }

    pub(in crate::renderer) fn clear_internal_scene_target(&mut self) {
        self.internal_scene_target = None;
        self.upscale_rect = None;
    }

    pub(in crate::renderer) fn ensure_internal_scene_target(&mut self, size: SurfaceSize) {
        if self.internal_scene_target.as_ref().is_some_and(|target| target.size == size) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bmz-render internal scene target"),
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
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bmz-render internal scene bind group"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler_linear),
                },
            ],
        });
        self.internal_scene_target =
            Some(InternalSceneTarget { _texture: texture, view, bind_group, size });
        self.upscale_rect = None;
        tracing::info!(
            width = size.width,
            height = size.height,
            "created internal scene render target"
        );
    }

    pub(in crate::renderer) fn update_upscale_instance(
        &mut self,
        rect: Rect,
        surface_size: SurfaceSize,
    ) {
        if self.upscale_rect == Some(rect) {
            return;
        }
        let mut bytes = Vec::with_capacity(IMAGE_INSTANCE_BYTES);
        encode_image_instance(
            &mut bytes,
            &rect,
            &UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            &Color::rgb(1.0, 1.0, 1.0),
            0.0,
            Point { x: 0.5, y: 0.5 },
            surface_size.width as f32 / surface_size.height.max(1) as f32,
            Point { x: 1.0, y: 1.0 },
        );
        self.queue.write_buffer(&self.upscale_buffer, 0, &bytes);
        self.upscale_rect = Some(rect);
    }

    pub(in crate::renderer) fn poll_screenshot_work(&mut self) {
        if !self.pending_screenshot_readbacks.is_empty()
            && let Err(error) = self.device.poll(wgpu::PollType::Poll)
        {
            tracing::warn!(%error, "failed to poll screenshot readback work");
        }
        self.drain_ready_screenshot_readbacks();
        self.join_finished_screenshot_save_jobs();
    }

    pub(in crate::renderer) fn enqueue_screenshot_readback(
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

    pub(in crate::renderer) fn drain_ready_screenshot_readbacks(&mut self) {
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

    pub(in crate::renderer) fn finish_screenshot_readback(
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

    pub(in crate::renderer) fn join_finished_screenshot_save_jobs(&mut self) {
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

    pub(in crate::renderer) fn flush_pending_screenshots(&mut self) -> Result<()> {
        while !self.pending_screenshot_readbacks.is_empty() {
            self.device.poll(wgpu::PollType::wait_indefinitely())?;
            self.drain_ready_screenshot_readbacks();
        }
        while let Some(job) = self.screenshot_save_jobs.pop() {
            finish_screenshot_save_job(job);
        }
        Ok(())
    }

    pub(in crate::renderer) fn wait_idle_before_drop(&self) {
        if let Err(error) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            tracing::warn!(%error, "failed to wait for renderer device before surface drop");
        }
    }
}
