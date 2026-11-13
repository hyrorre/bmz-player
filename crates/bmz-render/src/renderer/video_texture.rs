use super::*;

#[derive(Default)]
pub(super) struct VideoConversion {
    yuv420: Option<wgpu::RenderPipeline>,
    rgb24: Option<wgpu::RenderPipeline>,
    pending: Vec<TextureId>,
    padded_rgb_rows: Vec<u8>,
}

pub(super) struct VideoTexture {
    input: VideoInput,
    bindings: wgpu::BindGroup,
    dirty: bool,
}

enum VideoInput {
    Yuv420([wgpu::Texture; 3]),
    Rgb24(wgpu::Texture),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VideoFormat {
    Yuv420,
    Rgb24,
}

impl VideoInput {
    fn format(&self) -> VideoFormat {
        match self {
            Self::Yuv420(_) => VideoFormat::Yuv420,
            Self::Rgb24(_) => VideoFormat::Rgb24,
        }
    }
}

impl VideoFormat {
    fn pipeline(self, device: &wgpu::Device) -> wgpu::RenderPipeline {
        let entry = match self {
            Self::Yuv420 => "yuv420",
            Self::Rgb24 => "rgb24",
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("BGA video conversion"),
            source: wgpu::ShaderSource::Wgsl(include_str!("video_texture.wgsl").into()),
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BGA video conversion"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry),
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
        })
    }

    fn texture(
        self,
        device: &wgpu::Device,
        pipeline: &wgpu::RenderPipeline,
        width: u32,
        height: u32,
    ) -> PreparedTexture {
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
        let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        let input = match self {
            Self::Yuv420 => VideoInput::Yuv420(std::array::from_fn(|plane| {
                let shift = u32::from(plane != 0);
                make_texture(width >> shift, height >> shift, wgpu::TextureFormat::R8Unorm, usage)
            })),
            // Store four consecutive bytes per texel, so four RGB pixels fit
            // in three texels. Padding is required only at the end of a row.
            Self::Rgb24 => VideoInput::Rgb24(make_texture(
                (width * 3).div_ceil(4),
                height,
                wgpu::TextureFormat::Rgba8Uint,
                usage,
            )),
        };
        let (first_binding, textures) = match &input {
            VideoInput::Yuv420(textures) => (0, textures.as_slice()),
            VideoInput::Rgb24(texture) => (3, std::slice::from_ref(texture)),
        };
        let views: Vec<_> =
            textures.iter().map(|texture| texture.create_view(&Default::default())).collect();
        let entries: Vec<_> = views
            .iter()
            .enumerate()
            .map(|(index, view)| wgpu::BindGroupEntry {
                binding: first_binding + index as u32,
                resource: wgpu::BindingResource::TextureView(view),
            })
            .collect();
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BGA video input"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
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
            video: Some(Box::new(VideoTexture { input, bindings, dirty: false })),
        }
    }
}

impl WgpuRenderer {
    pub(super) fn upsert_video_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        format: VideoFormat,
        pixels: &[u8],
    ) -> Result<()> {
        let limit = self.device.limits().max_texture_dimension_2d;
        anyhow::ensure!(
            width > 0 && height > 0 && width <= limit && height <= limit,
            "invalid video texture dimensions"
        );
        let rgb_size = u64::from(width) * u64::from(height) * 3;
        let size = match format {
            VideoFormat::Rgb24 => rgb_size,
            VideoFormat::Yuv420 => {
                anyhow::ensure!(
                    width.is_multiple_of(2) && height.is_multiple_of(2),
                    "invalid YUV420 dimensions"
                );
                rgb_size / 2
            }
        };
        anyhow::ensure!(pixels.len() as u64 == size, "invalid video texture payload size");

        let conversion = self.video_conversion.get_or_insert_with(VideoConversion::default);
        if self.image_textures.get(&id).is_none_or(|image| {
            image.width != width
                || image.height != height
                || image.video.as_ref().is_none_or(|video| video.input.format() != format)
        }) {
            let pipeline = match format {
                VideoFormat::Yuv420 => &mut conversion.yuv420,
                VideoFormat::Rgb24 => &mut conversion.rgb24,
            }
            .get_or_insert_with(|| format.pipeline(&self.device));
            self.image_textures.insert(id, format.texture(&self.device, pipeline, width, height));
            self.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
            tracing::debug!(
                texture_id = id.0,
                width,
                height,
                ?format,
                "registered packed BGA texture"
            );
        }
        let video = self.image_textures.get_mut(&id).unwrap().video.as_mut().unwrap();
        match &video.input {
            VideoInput::Yuv420(textures) => {
                let mut pixels = pixels;
                for texture in textures {
                    let extent = texture.size();
                    let bytes = (extent.width as usize) * (extent.height as usize);
                    let (plane, rest) = pixels.split_at(bytes);
                    pixels = rest;
                    write_video_input(&self.queue, texture, plane, extent.width);
                }
            }
            VideoInput::Rgb24(texture) => {
                let row_bytes = (width * 3) as usize;
                let pitch = (texture.width() * 4) as usize;
                let pixels = if row_bytes == pitch {
                    pixels
                } else {
                    let padded = &mut conversion.padded_rgb_rows;
                    padded.resize(pitch * height as usize, 0);
                    for (src, dst) in
                        pixels.chunks_exact(row_bytes).zip(padded.chunks_exact_mut(pitch))
                    {
                        dst[..row_bytes].copy_from_slice(src);
                    }
                    padded
                };
                write_video_input(&self.queue, texture, pixels, pitch as u32);
            }
        }
        if !std::mem::replace(&mut video.dirty, true) {
            conversion.pending.push(id);
        }
        Ok(())
    }

    pub(super) fn encode_video_conversions(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(conversion) = &mut self.video_conversion else { return };
        for id in conversion.pending.drain(..) {
            let Some(texture) = self.image_textures.get_mut(&id) else { continue };
            let Some(video) = texture.video.as_mut() else { continue };
            if !std::mem::take(&mut video.dirty) {
                continue;
            }
            let pipeline = match video.input.format() {
                VideoFormat::Yuv420 => &conversion.yuv420,
                VideoFormat::Rgb24 => &conversion.rgb24,
            }
            .as_ref()
            .unwrap();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("BGA video conversion"),
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
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &video.bindings, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}

fn write_video_input(queue: &wgpu::Queue, texture: &wgpu::Texture, bytes: &[u8], pitch: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(pitch),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}

#[test]
#[ignore = "requires a GPU"]
fn video_rgb24_conversion_matches_rgba_with_padding_and_blending() {
    let mut renderer = Renderer::default();
    let width = 5;
    let height = 2;
    renderer.attach_offscreen(SurfaceSize { width, height }).unwrap();
    let id = TextureId(50000);
    renderer.last_plan = Some(video_test_plan(id, 0.5));

    let rgb: Vec<_> = (0..30u8).map(|i| i.wrapping_mul(23)).collect();
    let rgba: Vec<_> =
        rgb.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
    renderer.upsert_rgba_texture_ref(id, width, height, &rgba).unwrap();
    renderer.render_last_plan().unwrap();
    let expected = renderer.read_offscreen_rgba().unwrap();
    assert_ne!(&expected[..4], &expected[4..8]);

    renderer.upsert_rgb24_texture(id, width, height, &rgb).unwrap();
    renderer.render_last_plan().unwrap();
    assert_eq!(renderer.read_offscreen_rgba().unwrap(), expected);

    let rgb: Vec<_> = rgb.into_iter().rev().collect();
    renderer.upsert_rgb24_texture(id, width, height, &rgb).unwrap();
    renderer.render_last_plan().unwrap();
    let actual = renderer.read_offscreen_rgba().unwrap();
    assert_ne!(actual, expected);
    let rgba: Vec<_> =
        rgb.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect();
    renderer.upsert_rgba_texture_ref(id, width, height, &rgba).unwrap();
    renderer.render_last_plan().unwrap();
    assert_eq!(renderer.read_offscreen_rgba().unwrap(), actual);
}

#[cfg(test)]
fn video_test_plan(id: TextureId, opacity: f32) -> DrawPlan {
    DrawPlan {
        clear: Color::rgb(0.0, 0.0, 0.0),
        commands: vec![DrawCommand::Image {
            rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: id,
            tint: Color::rgba(1.0, 1.0, 1.0, opacity),
            blend: BlendMode::Normal,
            linear_filter: false,
        }],
    }
}

#[test]
#[ignore = "requires a GPU"]
fn video_yuv_conversion_preserves_black_white_and_chroma() {
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 4, height: 2 }).unwrap();
    let id = TextureId(50000);
    renderer.last_plan = Some(video_test_plan(id, 1.0));
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
