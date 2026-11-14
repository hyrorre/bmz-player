use super::*;

fn blit_quad_bytes(rect: &Rect, aspect: f32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(IMAGE_INSTANCE_BYTES);
    encode_image_instance(
        &mut bytes,
        rect,
        &UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
        &Color::rgb(1.0, 1.0, 1.0),
        0.0,
        Point { x: 0.5, y: 0.5 },
        aspect,
        Point { x: 1.0, y: 1.0 },
    );
    bytes
}

impl WgpuRenderer {
    pub(in crate::renderer) fn resize(&mut self, size: SurfaceSize) -> Result<()> {
        anyhow::ensure!(
            self.surface_recovery.is_some() || !size.is_drawable() || self.surface_size() == size,
            "offscreen target dimensions are fixed; attach a new target to resize"
        );

        if let Some(recovery) = &mut self.surface_recovery {
            let mut desired = recovery.desired.clone();
            desired.width = size.width;
            desired.height = size.height;
            recovery.request(desired, SurfaceConfigureReason::Resize);
            self.retry_surface(Instant::now());
        }
        Ok(())
    }

    pub(in crate::renderer) fn surface_blocked_status(&self) -> Option<RenderSurfaceStatus> {
        self.surface_recovery.as_ref().and_then(SurfaceRecovery::blocked_status)
    }

    pub(in crate::renderer) fn invalidate_surface(&mut self, reason: SurfaceConfigureReason) {
        if let Some(recovery) = &mut self.surface_recovery {
            if reason == SurfaceConfigureReason::Lost {
                recovery.fail(
                    reason,
                    &self.config,
                    &self.adapter_info,
                    "surface lost; a new Surface is required",
                );
            } else {
                recovery.request(recovery.desired.clone(), reason);
                // Outdated must return to the window owner to sample the current
                // physical size. Never configure with the stored extent here.
            }
        }
    }

    pub(in crate::renderer) fn check_device_loss(&mut self) -> Result<bool> {
        if !self.device_loss.is_lost() {
            return Ok(false);
        }
        if let Some(recovery) = &mut self.surface_recovery {
            if let Some(message) = self.device_loss.take_message() {
                recovery.fail(
                    SurfaceConfigureReason::DeviceLost,
                    &self.config,
                    &self.adapter_info,
                    &message,
                );
            }
            Ok(true)
        } else {
            anyhow::bail!(
                "offscreen GPU device lost: {}",
                self.device_loss.take_message().unwrap_or_default()
            )
        }
    }

    pub(in crate::renderer) fn retry_surface(&mut self, now: Instant) -> bool {
        if self.check_device_loss().unwrap_or(true) {
            return false;
        }
        let Some(recovery) = &mut self.surface_recovery else { return false };
        let applied = recovery.retry(&mut self.config, &self.adapter_info, now, |desired| {
            #[cfg(test)]
            if let Some(test) = &mut self.surface_test {
                test.configure_calls += 1;
                if test.failures_remaining > 0 {
                    test.failures_remaining -= 1;
                    anyhow::bail!("injected surface configure failure");
                }
                if test.lose_device_during_configure {
                    self.device_loss.notify(
                        wgpu::DeviceLostReason::Unknown,
                        "injected loss with empty error scopes".into(),
                    );
                }
            }
            if let Some(surface) = &self.surface {
                configure_surface_checked(surface, &self.device, desired, &self.adapter_info)?;
            }
            anyhow::ensure!(!self.device_loss.is_lost(), "device lost during surface configure");
            Ok(())
        });
        if self.check_device_loss().unwrap_or(true) {
            return false;
        }
        if applied {
            self.clear_internal_scene_target();
            self.screenshot_target = None;
            self.clear_offscreen_rect_batches();
        }
        applied
    }

    pub(in crate::renderer) fn configure_presentation(
        &mut self,
        present_mode: WgpuPresentMode,
        frame_latency_mode: WgpuFrameLatencyMode,
        reason: SurfaceConfigureReason,
    ) {
        let Some(recovery) = &mut self.surface_recovery else { return };
        let was_available = recovery.blocked_status().is_none();
        let mut desired = recovery.desired.clone();
        configure_surface_settings(
            &mut desired,
            present_mode,
            frame_latency_mode,
            &self.present_modes,
        );
        recovery.request(desired, reason);
        // While recovering, let the event-loop retry publish the latest size to
        // skin loaders before the next scene. A setter must not silently apply a
        // previously failed resize without that notification.
        if was_available {
            self.retry_surface(Instant::now());
        }
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
        self.internal_scene_target = Some(self.create_scene_target(size, false));
        self.upscale_rect = None;
        tracing::info!(
            width = size.width,
            height = size.height,
            "created internal scene render target"
        );
    }

    pub(in crate::renderer) fn ensure_screenshot_target(&mut self, size: SurfaceSize) {
        if !self.screenshot_target.as_ref().is_some_and(|target| target.size == size) {
            self.screenshot_target = Some(self.create_scene_target(size, true));
        }
        if self.screenshot_blit_buffer.is_none() {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bmz-render screenshot blit quad"),
                size: IMAGE_INSTANCE_BYTES as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let rect = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
            self.queue.write_buffer(&buffer, 0, &blit_quad_bytes(&rect, 1.0));
            self.screenshot_blit_buffer = Some(buffer);
        }
    }

    fn create_scene_target(&self, size: SurfaceSize, readable: bool) -> SceneTarget {
        let mut usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let (label, sampler) = if readable {
            usage |= wgpu::TextureUsages::COPY_SRC;
            ("bmz-render screenshot target", &self.image_sampler)
        } else {
            ("bmz-render internal scene target", &self.image_sampler_linear)
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.config.format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        SceneTarget { texture, view, bind_group, size }
    }

    pub(in crate::renderer) fn update_upscale_instance(
        &mut self,
        rect: Rect,
        surface_size: SurfaceSize,
    ) {
        if self.upscale_rect == Some(rect) {
            return;
        }
        let aspect = surface_size.width as f32 / surface_size.height.max(1) as f32;
        self.queue.write_buffer(&self.upscale_buffer, 0, &blit_quad_bytes(&rect, aspect));
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
