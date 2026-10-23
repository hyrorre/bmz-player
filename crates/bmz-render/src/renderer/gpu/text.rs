use super::*;

impl WgpuRenderer {
    pub(in crate::renderer) fn set_default_font_coverage(
        &mut self,
        coverage: bmz_font::FontCoverage,
    ) {
        self.default_font_coverage = coverage;
        self.default_fonts = load_default_font_fallbacks(coverage, &self.default_font_search_paths);
        self.reset_text_atlas();
    }

    pub(in crate::renderer) fn set_default_font_search_paths(&mut self, paths: Vec<PathBuf>) {
        if self.default_font_search_paths == paths {
            return;
        }
        self.default_font_search_paths = paths;
        self.default_fonts = load_default_font_fallbacks(
            self.default_font_coverage,
            &self.default_font_search_paths,
        );
        self.reset_text_atlas();
    }

    pub(in crate::renderer) fn build_text_frame(
        &mut self,
        plan: &DrawPlan,
        font_sets: &SkinFontSets,
        surface: SurfaceSize,
    ) -> TextFrame {
        if !surface.is_drawable() {
            return TextFrame::default();
        }
        build_text_frame_with_font_sets(
            plan,
            &self.default_fonts,
            font_sets,
            surface,
            &mut self.text_atlas,
        )
    }

    /// 文字インスタンスバッファの確保と転送。
    pub(in crate::renderer) fn upload_text_instances(&mut self, frame: &TextFrame) {
        self.ensure_text_buffer(frame.instances.len());
        if let Some(buffer) = &self.text_buffer
            && !frame.instances.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &frame.instances);
        }
    }

    /// 文字 atlas テクスチャの確保と転送。
    pub(in crate::renderer) fn upload_text_atlas(
        &mut self,
        frame: &TextFrame,
    ) -> TextAtlasUploadStats {
        if frame.size.width == 0 || frame.size.height == 0 {
            return TextAtlasUploadStats::default();
        }
        let started = tracing::enabled!(target: "bmz_render::text_atlas", tracing::Level::DEBUG)
            .then(Instant::now);

        let recreate_texture = self.text_texture_size != frame.size || self.text_texture.is_none();
        self.ensure_text_texture(frame.size);
        let texture = self.text_texture.as_ref().expect("text texture exists after ensure");
        let atlas = &self.text_atlas;
        let queue = &self.queue;
        let stats = write_text_atlas(
            frame,
            recreate_texture,
            || atlas.pixels_for_size(frame.size),
            |origin, size, pixels| {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: origin.0, y: origin.1, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    pixels,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size.width * 4),
                        rows_per_image: Some(size.height),
                    },
                    wgpu::Extent3d {
                        width: size.width,
                        height: size.height,
                        depth_or_array_layers: 1,
                    },
                );
            },
        );
        if let Some(started) = started {
            tracing::debug!(target: "bmz_render::text_atlas", frame = atlas.frame,
                upload_us = started.elapsed().as_micros(), writes = stats.writes,
                bytes = stats.bytes, recreate_texture, "text atlas upload");
        }
        stats
    }

    pub(in crate::renderer) fn reset_text_atlas(&mut self) {
        self.text_atlas =
            TextAtlasCache::for_texture_limit(self.device.limits().max_texture_dimension_2d);
        self.text_texture = None;
        self.text_texture_view = None;
        self.text_bind_group = None;
        self.text_texture_size = AtlasSize::default();
    }

    pub(in crate::renderer) fn ensure_text_buffer(&mut self, used_bytes: usize) {
        let Some(capacity) = grown_text_buffer_capacity(used_bytes, self.text_buffer_capacity)
        else {
            return;
        };
        self.text_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render text instance buffer"),
            size: capacity as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.text_buffer_capacity = capacity;
    }

    pub(in crate::renderer) fn ensure_text_texture(&mut self, size: AtlasSize) {
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

    pub(in crate::renderer) fn text_bind_group(&mut self) -> Option<wgpu::BindGroup> {
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
}
