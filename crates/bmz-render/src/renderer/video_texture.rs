use super::*;

pub(super) struct VideoConversion {
    pipeline: wgpu::RenderPipeline,
    pending: Vec<TextureId>,
}

pub(super) struct VideoPlanes {
    textures: [wgpu::Texture; 3],
    bindings: wgpu::BindGroup,
    dirty: bool,
}

impl VideoConversion {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("BGA YUV420 conversion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("video_texture.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BGA YUV420 conversion"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, pending: Vec::new() }
    }

    fn texture(&self, device: &wgpu::Device, width: u32, height: u32) -> PreparedTexture {
        let make_texture = |width, height, format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("BGA video texture"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let textures = std::array::from_fn(|plane| {
            let shift = u32::from(plane != 0);
            make_texture(
                width >> shift,
                height >> shift,
                wgpu::TextureFormat::R8Unorm,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            )
        });
        let views = textures.each_ref().map(|texture| texture.create_view(&Default::default()));
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BGA YUV420 planes"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &std::array::from_fn::<_, 3, _>(|plane| wgpu::BindGroupEntry {
                binding: plane as u32,
                resource: wgpu::BindingResource::TextureView(&views[plane]),
            }),
        });
        let texture = make_texture(
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let view = texture.create_view(&Default::default());
        PreparedTexture {
            texture,
            view,
            width,
            height,
            yuv: Some(Box::new(VideoPlanes { textures, bindings, dirty: false })),
        }
    }
}

impl WgpuRenderer {
    pub(super) fn upsert_yuv420_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        mut pixels: &[u8],
    ) -> Result<()> {
        let limit = self.device.limits().max_texture_dimension_2d;
        anyhow::ensure!(
            width > 0
                && height > 0
                && width <= limit
                && height <= limit
                && width.is_multiple_of(2)
                && height.is_multiple_of(2),
            "invalid YUV420 dimensions"
        );
        let size = width as u64 * height as u64 * 3 / 2;
        anyhow::ensure!(pixels.len() as u64 == size, "invalid YUV420 payload size");

        if self.image_textures.get(&id).is_none_or(|image| {
            image.width != width || image.height != height || image.yuv.is_none()
        }) {
            let conversion =
                self.video_conversion.get_or_insert_with(|| VideoConversion::new(&self.device));
            self.image_textures.insert(id, conversion.texture(&self.device, width, height));
            self.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
            tracing::debug!(texture_id = id.0, width, height, "registered YUV420 BGA texture");
        }
        let planes = self.image_textures.get_mut(&id).unwrap().yuv.as_mut().unwrap();
        for texture in &planes.textures {
            let extent = texture.size();
            let bytes = (extent.width as usize) * (extent.height as usize);
            let (plane, rest) = pixels.split_at(bytes);
            pixels = rest;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                plane,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(extent.width),
                    rows_per_image: Some(extent.height),
                },
                extent,
            );
        }
        if !std::mem::replace(&mut planes.dirty, true) {
            self.video_conversion.as_mut().unwrap().pending.push(id);
        }
        Ok(())
    }

    pub(super) fn encode_video_conversions(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(conversion) = &mut self.video_conversion else { return };
        for id in conversion.pending.drain(..) {
            let Some(texture) = self.image_textures.get_mut(&id) else { continue };
            let Some(planes) = texture.yuv.as_mut() else { continue };
            if !std::mem::take(&mut planes.dirty) {
                continue;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("BGA YUV420 conversion"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                ..Default::default()
            });
            pass.set_pipeline(&conversion.pipeline);
            pass.set_bind_group(0, &planes.bindings, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn video_yuv_conversion_preserves_black_white_and_chroma() {
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 4, height: 2 }).unwrap();
    let id = TextureId(50000);
    let full = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
    renderer.last_plan = Some(DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Image {
            rect: full,
            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: id,
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            linear_filter: false,
        }],
    });
    // Two neutral luma levels and one saturated chroma block.
    renderer
        .upsert_yuv420_texture(id, 4, 2, &[16, 235, 81, 81, 16, 235, 81, 81, 128, 90, 128, 240])
        .unwrap();
    renderer.render_last_plan().unwrap();
    let pixels = renderer.read_offscreen_rgba().unwrap();
    assert_eq!(&pixels[..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
    assert!(pixels[8] >= 250 && pixels[9] <= 3 && pixels[10] <= 3);

    renderer.upsert_rgba_texture_ref(id, 4, 2, &[0, 0, 0, 255].repeat(8)).unwrap();
    renderer.render_last_plan().unwrap();
    assert_eq!(&renderer.read_offscreen_rgba().unwrap()[..4], &[0, 0, 0, 255]);
}
