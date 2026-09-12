use super::*;

impl WgpuRenderer {
    pub(in crate::renderer) fn ensure_rect_buffer(&mut self, used_bytes: usize) {
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

    pub(in crate::renderer) fn ensure_image_buffer(&mut self, used_bytes: usize) {
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

    pub(in crate::renderer) fn offscreen_rect_batch_texture(
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

    pub(in crate::renderer) fn allocate_offscreen_rect_batch_texture_id(&mut self) -> TextureId {
        let texture_id = TextureId(self.next_offscreen_rect_batch_texture_id);
        self.next_offscreen_rect_batch_texture_id = self
            .next_offscreen_rect_batch_texture_id
            .checked_add(1)
            .unwrap_or(OFFSCREEN_RECT_BATCH_TEXTURE_BASE);
        texture_id
    }

    pub(in crate::renderer) fn render_rect_batch_to_offscreen_texture(
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

    pub(in crate::renderer) fn clear_offscreen_rect_batches(&mut self) {
        for texture_id in self.offscreen_rect_batches.drain().map(|(_, texture_id)| texture_id) {
            self.image_textures.remove(&texture_id);
            self.image_bind_group_cache
                .retain(|(cached_texture_id, _), _| *cached_texture_id != texture_id);
        }
        self.next_offscreen_rect_batch_texture_id = OFFSCREEN_RECT_BATCH_TEXTURE_BASE;
    }

    pub(in crate::renderer) fn configure_surface(&self) -> Result<()> {
        if let Some(surface) = &self.surface {
            configure_surface_checked(surface, &self.device, &self.config, &self.adapter_info)
        } else {
            Ok(())
        }
    }

    pub(in crate::renderer) fn configure_presentation(
        &mut self,
        present_mode: WgpuPresentMode,
        frame_latency_mode: WgpuFrameLatencyMode,
    ) -> Result<()> {
        configure_surface_settings(
            &mut self.config,
            present_mode,
            frame_latency_mode,
            &self.present_modes,
        );
        self.configure_surface()?;
        tracing::info!(
            requested = ?present_mode,
            effective = ?self.config.present_mode,
            available = ?self.present_modes,
            maximum_frame_latency = self.config.desired_maximum_frame_latency,
            "configured renderer present mode"
        );
        Ok(())
    }
}
