use super::*;

pub(super) fn create_rect_pipeline(
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
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render rect pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
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
            entry_point: Some("fs_main"),
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
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn create_image_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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

pub(super) fn create_image_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bmz-render image sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

pub(super) fn create_image_sampler_linear(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bmz-render image sampler linear"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

pub(super) fn create_default_image_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> HashMap<TextureId, PreparedTexture> {
    let mut textures = HashMap::new();
    textures.insert(
        TextureId(0),
        create_rgba_texture(device, queue, TextureId(0), 1, 1, &[255, 255, 255, 255])
            .expect("valid fallback texture"),
    );
    textures
}

/// GPU テクスチャアップロード機能のハンドル。`wgpu::Device` / `Queue` の clone を
/// 保持し、メインスレッド外 (skin upload worker) から RGBA8 をアップロードできる。
/// `Renderer::gpu_uploader` で取得する。`Send + Clone`。
#[derive(Clone)]
pub struct GpuUploader {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
}

impl GpuUploader {
    /// RGBA8 バイト列を GPU テクスチャへアップロードして `PreparedTexture` を返す。
    pub fn upload(&self, width: u32, height: u32, rgba: &[u8]) -> Result<PreparedTexture> {
        create_rgba_texture(&self.device, &self.queue, TextureId(0), width, height, rgba)
    }
}

pub(super) fn create_rgba_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: TextureId,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<PreparedTexture> {
    validate_rgba_texture_for_device(
        device.limits().max_texture_dimension_2d,
        width,
        height,
        rgba,
    )?;
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
    write_rgba_texture(queue, &texture, width, height, rgba);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    tracing::debug!(texture_id = id.0, width, height, "registered render image texture");
    Ok(PreparedTexture { texture, view, width, height })
}

pub(super) fn validate_rgba_texture_for_device(
    limit: u32,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<()> {
    anyhow::ensure!(
        width <= limit && height <= limit,
        "image dimensions {width}x{height} exceed GPU texture limit {limit}"
    );
    validate_rgba_texture(width, height, rgba)
}

pub(super) fn write_rgba_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    rgba: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}

pub(super) fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    blend_mode: BlendMode,
) -> wgpu::RenderPipeline {
    let shader_source = match blend_mode {
        BlendMode::LayerMask => IMAGE_SHADER_LAYER,
        BlendMode::Normal
        | BlendMode::Add
        | BlendMode::Subtract
        | BlendMode::Multiply
        | BlendMode::Premultiplied => IMAGE_SHADER,
    };
    let label = match blend_mode {
        BlendMode::Normal => "bmz-render image pipeline",
        BlendMode::Add => "bmz-render additive image pipeline",
        BlendMode::Subtract => "bmz-render subtractive image pipeline",
        BlendMode::Multiply => "bmz-render multiply image pipeline",
        BlendMode::Premultiplied => "bmz-render premultiplied image pipeline",
        BlendMode::LayerMask => "bmz-render layer-mask image pipeline",
    };
    create_image_quad_pipeline(
        device,
        format,
        bind_group_layout,
        shader_source,
        label,
        Some(image_blend_state(blend_mode)),
    )
}

pub(super) fn create_upscale_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    create_image_quad_pipeline(
        device,
        format,
        bind_group_layout,
        IMAGE_SHADER,
        "bmz-render internal upscale pipeline",
        None,
    )
}

pub(super) fn create_image_quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader_source: &'static str,
    label: &'static str,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bmz-render image shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bmz-render image pipeline layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
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
                    wgpu::VertexAttribute {
                        offset: 48,
                        shader_location: 3,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                    wgpu::VertexAttribute {
                        offset: 64,
                        shader_location: 4,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

pub(super) fn image_blend_state(blend_mode: BlendMode) -> wgpu::BlendState {
    match blend_mode {
        BlendMode::Normal | BlendMode::LayerMask => wgpu::BlendState::ALPHA_BLENDING,
        BlendMode::Subtract => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::ReverseSubtract,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
        BlendMode::Multiply => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
        },
        BlendMode::Premultiplied => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        },
        BlendMode::Add => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        },
    }
}

pub(super) fn create_text_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
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

pub(super) fn create_text_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bmz-render text sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    })
}

pub(super) fn create_text_pipeline(
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
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bmz-render text pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
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
            entry_point: Some("fs_main"),
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
        multiview_mask: None,
        cache: None,
    })
}

pub(super) const RECT_SHADER: &str = r#"
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

pub(super) const IMAGE_SHADER: &str = r#"
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
    @location(3) rotation: vec4<f32>,
    @location(4) post_scale: vec2<f32>,
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
    let pivot = rotation.yz;
    let relative = (local - pivot) * rect.zw;
    let aspect = max(rotation.w, 0.0001);
    let relative_aspect = vec2<f32>(relative.x * aspect, relative.y);
    let c = cos(rotation.x);
    let s = sin(rotation.x);
    let rotated_aspect = vec2<f32>(
        relative_aspect.x * c - relative_aspect.y * s,
        relative_aspect.x * s + relative_aspect.y * c,
    );
    let rotated = vec2<f32>(rotated_aspect.x / aspect, rotated_aspect.y);
    let pos01 = rect.xy + pivot * rect.zw + rotated * post_scale;

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

// beatoraja の layer.frag 相当: BGA Layer / Layer2 用に黒ピクセルを透過する。
pub(super) const IMAGE_SHADER_LAYER: &str = r#"
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
    @location(3) rotation: vec4<f32>,
    @location(4) post_scale: vec2<f32>,
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
    let pivot = rotation.yz;
    let relative = (local - pivot) * rect.zw;
    let aspect = max(rotation.w, 0.0001);
    let relative_aspect = vec2<f32>(relative.x * aspect, relative.y);
    let c = cos(rotation.x);
    let s = sin(rotation.x);
    let rotated_aspect = vec2<f32>(
        relative_aspect.x * c - relative_aspect.y * s,
        relative_aspect.x * s + relative_aspect.y * c,
    );
    let rotated = vec2<f32>(rotated_aspect.x / aspect, rotated_aspect.y);
    let pos01 = rect.xy + pivot * rect.zw + rotated * post_scale;

    var out: VertexOutput;
    out.position = vec4<f32>(pos01.x * 2.0 - 1.0, 1.0 - pos01.y * 2.0, 0.0, 1.0);
    out.uv = uv_rect.xy + local * uv_rect.zw;
    out.tint = tint;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(image_texture, image_sampler, input.uv);
    if (sampled.r == 0.0 && sampled.g == 0.0 && sampled.b == 0.0) {
        return input.tint * vec4<f32>(sampled.rgb, 0.0);
    }
    return input.tint * sampled;
}
"#;

pub(super) const TEXT_SHADER: &str = r#"
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
    let glyph = textureSample(text_atlas, text_sampler, input.uv);
    // RGBA atlas: bitmap font は色付きグリフを保持。TTF は (1,1,1,coverage)。
    // tint と乗算してアルファブレンド用の最終色を出す。
    return glyph * input.color;
}
"#;
