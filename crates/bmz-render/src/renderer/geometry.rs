use super::*;

pub(super) const RECT_INSTANCE_FLOATS: usize = 8;
pub(super) const RECT_INSTANCE_BYTES: usize = RECT_INSTANCE_FLOATS * std::mem::size_of::<f32>();
pub(super) const IMAGE_INSTANCE_FLOATS: usize = 18;
pub(super) const IMAGE_INSTANCE_BYTES: usize = IMAGE_INSTANCE_FLOATS * std::mem::size_of::<f32>();
pub(super) const TEXT_INSTANCE_FLOATS: usize = 12;
pub(super) const TEXT_INSTANCE_BYTES: usize = TEXT_INSTANCE_FLOATS * std::mem::size_of::<f32>();
pub(super) const TEXT_ATLAS_WIDTH: u32 = 1024;
pub(super) const TEXT_ATLAS_PADDING: u32 = 1;
pub(super) const VECTOR_TEXT_SUPERSAMPLE_SCALE: f32 = 2.0;
/// グリフは永続キャッシュされるため、選曲画面のスクロールなどで文字種が増え続けると
/// アトラス高さが単調増加する。wgpu の `max_texture_dimension_2d` (一般に 16384) を
/// 超えると `create_texture` がパニックするので、上限に達したらフレーム境界でキャッシュを
/// 捨てて作り直す。1 フレーム分のグリフは十分この高さに収まるため、リセットしても破綻しない。
pub(super) const TEXT_ATLAS_MAX_HEIGHT: u32 = 8192;
pub(super) const TEXT_LAYOUT_CACHE_MAX_ENTRIES: usize = 4096;

pub(super) fn normalized_extent_to_pixels(normalized: f32, surface_extent: u32) -> u32 {
    if normalized <= f32::EPSILON || surface_extent == 0 {
        return 0;
    }
    (normalized * surface_extent as f32).ceil().clamp(1.0, surface_extent.max(1) as f32) as u32
}

pub(super) fn encode_local_rect_batch(rects: &[RectCommand], bounds: Rect) -> Vec<u8> {
    if bounds.width <= f32::EPSILON || bounds.height <= f32::EPSILON {
        return Vec::new();
    }
    let mut bytes = Vec::with_capacity(rects.len() * RECT_INSTANCE_BYTES);
    for command in rects {
        let rect = command.rect;
        let color = command.color;
        let local = Rect {
            x: (rect.x - bounds.x) / bounds.width,
            y: (rect.y - bounds.y) / bounds.height,
            width: rect.width / bounds.width,
            height: rect.height / bounds.height,
        };
        bytes.extend_from_slice(bytemuck::bytes_of(&[
            local.x,
            local.y,
            local.width,
            local.height,
            color.r,
            color.g,
            color.b,
            color.a,
        ]));
    }
    bytes
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct AtlasSize {
    pub(super) width: u32,
    pub(super) height: u32,
}

#[derive(Debug, Default)]
pub(super) struct TextFrame {
    pub(super) size: AtlasSize,
    #[cfg(test)]
    pub(super) pixels: Vec<u8>,
    pub(super) dirty_regions: Vec<TextAtlasDirtyRegion>,
    pub(super) instances: Vec<u8>,
    /// `DrawCommand::Text` ごとに生成された quad 数を、commands 内の出現順で持つ。
    /// 描画ステップ単位で text instance buffer をスライスするのに使う。
    pub(super) command_quad_counts: Vec<usize>,
    /// `DrawCommand::Text` ごとに生成された caret 矩形。
    pub(super) command_caret_rects: Vec<Option<RectCommand>>,
}

#[derive(Debug, Clone)]
pub(super) struct TextAtlasDirtyRegion {
    pub(super) origin: (u32, u32),
    pub(super) size: AtlasSize,
    pub(super) pixels: Vec<u8>,
}

/// `DrawPlan.commands` の順序を保ったまま 1 レンダーパスで描くための描画ステップ。
/// 連続する同種コマンドは 1 ステップにまとめる。image は texture/blend/linear が
/// 変わるか、別種コマンドを挟むたびに分割する。
#[derive(Debug, Clone, PartialEq)]
pub(super) enum DrawStep {
    /// A nonempty pixel scissor, or an empty intersection that suppresses drawing.
    Scissor { rect: Option<ScissorRect> },
    /// rect instance buffer 内のバイト範囲。
    Rects { range: Range<usize> },
    /// image instance buffer 内のバイト範囲。
    Image { texture: TextureId, blend: BlendMode, linear: bool, range: Range<usize> },
    /// text instance buffer 内のバイト範囲。atlas テクスチャは全 text で共有する。
    Text { range: Range<usize> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScissorRect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

impl ScissorRect {
    fn full(size: SurfaceSize) -> Option<Self> {
        size.is_drawable().then_some(Self { x: 0, y: 0, width: size.width, height: size.height })
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then(|| Self { x, y, width: right - x, height: bottom - y })
    }

    /// ScissorStack fixes each bottom-origin component with Java Math.round,
    /// then flips negative extents. Round before converting back to top origin;
    /// rounding both endpoints would produce different widths at half pixels.
    pub(super) fn from_canvas(
        rect: Rect,
        viewport: CanvasViewport,
        size: SurfaceSize,
    ) -> Option<Self> {
        let rect = viewport.transform_rect(rect);
        if !size.is_drawable()
            || ![rect.x, rect.y, rect.width, rect.height].into_iter().all(f32::is_finite)
        {
            return None;
        }
        let round = |value: f64| (value + 0.5).floor();
        let surface_width = f64::from(size.width);
        let surface_height = f64::from(size.height);
        // Project with float arithmetic just like libGDX. Widening normalized
        // values first can move an exact projected half pixel below the tie.
        let mut x = round(f64::from(rect.x * size.width as f32));
        let mut y = round(f64::from((1.0 - rect.y - rect.height) * size.height as f32));
        let mut width = round(f64::from(rect.width * size.width as f32));
        let mut height = round(f64::from(rect.height * size.height as f32));
        if width < 0.0 {
            x += width;
            width = -width;
        }
        if height < 0.0 {
            y += height;
            height = -height;
        }
        let left = x.clamp(0.0, surface_width) as u32;
        let right = (x + width).clamp(0.0, surface_width) as u32;
        let top = (surface_height - y - height).clamp(0.0, surface_height) as u32;
        let bottom = (surface_height - y).clamp(0.0, surface_height) as u32;
        (right > left && bottom > top).then(|| Self {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// `DrawPlan` を GPU 描画用のバッファ列と順序付きステップ列へ変換した結果。
#[derive(Default)]
pub(super) struct PlanGeometry {
    pub(super) rects: Vec<u8>,
    pub(super) images: Vec<u8>,
    pub(super) steps: Vec<DrawStep>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct DrawStepStats {
    pub(super) steps: usize,
    pub(super) rect_steps: usize,
    pub(super) image_steps: usize,
    pub(super) text_steps: usize,
    pub(super) rect_instances: usize,
    pub(super) image_instances: usize,
    pub(super) text_instances: usize,
}

impl PlanGeometry {
    pub(super) fn stats(&self) -> DrawStepStats {
        let mut stats = DrawStepStats {
            steps: self.steps.len(),
            rect_instances: self.rects.len() / RECT_INSTANCE_BYTES,
            image_instances: self.images.len() / IMAGE_INSTANCE_BYTES,
            ..Default::default()
        };
        for step in &self.steps {
            match step {
                DrawStep::Scissor { .. } => {}
                DrawStep::Rects { .. } => {
                    stats.rect_steps += 1;
                }
                DrawStep::Image { .. } => {
                    stats.image_steps += 1;
                }
                DrawStep::Text { range } => {
                    stats.text_steps += 1;
                    stats.text_instances += range.len() / TEXT_INSTANCE_BYTES;
                }
            }
        }
        stats
    }
}

#[derive(Clone, Copy)]
pub(super) struct PlanGeometryDrawResources<'pass> {
    pub(super) rect_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) rect_buffer: Option<&'pass wgpu::Buffer>,
    pub(super) image_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) image_add_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) image_multiply_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) image_subtract_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) image_premultiplied_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) image_layer_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) image_bind_groups: &'pass [wgpu::BindGroup],
    pub(super) image_buffer: Option<&'pass wgpu::Buffer>,
    pub(super) text_pipeline: &'pass wgpu::RenderPipeline,
    pub(super) text_bind_group: Option<&'pass wgpu::BindGroup>,
    pub(super) text_buffer: Option<&'pass wgpu::Buffer>,
}

pub(super) fn draw_plan_geometry<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    geometry: &'pass PlanGeometry,
    resources: PlanGeometryDrawResources<'pass>,
) {
    let mut image_step_index = 0_usize;
    let mut visible = true;
    for step in &geometry.steps {
        match step {
            DrawStep::Scissor { rect } => {
                visible = rect.is_some();
                if let Some(rect) = rect {
                    pass.set_scissor_rect(rect.x, rect.y, rect.width, rect.height);
                }
            }
            DrawStep::Rects { range } => {
                if !visible {
                    continue;
                }
                let Some(buffer) = resources.rect_buffer else {
                    continue;
                };
                let instance_count = (range.len() / RECT_INSTANCE_BYTES) as u32;
                if instance_count == 0 {
                    continue;
                }
                pass.set_pipeline(resources.rect_pipeline);
                pass.set_vertex_buffer(0, buffer.slice(range.start as u64..range.end as u64));
                pass.draw(0..6, 0..instance_count);
            }
            DrawStep::Image { blend, range, .. } => {
                let bind_group = &resources.image_bind_groups[image_step_index];
                image_step_index += 1;
                if !visible {
                    continue;
                }
                let Some(buffer) = resources.image_buffer else {
                    continue;
                };
                let instance_count = (range.len() / IMAGE_INSTANCE_BYTES) as u32;
                if instance_count == 0 {
                    continue;
                }
                pass.set_pipeline(match blend {
                    BlendMode::Normal => resources.image_pipeline,
                    BlendMode::Add => resources.image_add_pipeline,
                    BlendMode::Multiply => resources.image_multiply_pipeline,
                    BlendMode::Subtract => resources.image_subtract_pipeline,
                    BlendMode::Premultiplied => resources.image_premultiplied_pipeline,
                    BlendMode::LayerMask => resources.image_layer_pipeline,
                });
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(range.start as u64..range.end as u64));
                pass.draw(0..6, 0..instance_count);
            }
            DrawStep::Text { range } => {
                if !visible {
                    continue;
                }
                let (Some(bind_group), Some(buffer)) =
                    (resources.text_bind_group, resources.text_buffer)
                else {
                    continue;
                };
                let instance_count = (range.len() / TEXT_INSTANCE_BYTES) as u32;
                if instance_count == 0 {
                    continue;
                }
                pass.set_pipeline(resources.text_pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.slice(range.start as u64..range.end as u64));
                pass.draw(0..6, 0..instance_count);
            }
        }
    }
}

/// commands を 1 回走査し、rect/image インスタンスバッファと、コマンド順を尊重した
/// 描画ステップ列を作る。`text_frame` の `command_quad_counts` から各 Text コマンドが
/// 占める text instance buffer の範囲を割り出す。
#[cfg(test)]
pub(super) fn encode_plan_geometry(
    plan: &DrawPlan,
    text_frame: &TextFrame,
    surface_size: SurfaceSize,
) -> PlanGeometry {
    let viewport = CanvasViewport::from_policy(surface_size, CanvasRenderPolicy::default());
    let mut geometry = PlanGeometry::default();
    encode_plan_geometry_into(
        plan,
        text_frame,
        surface_size,
        viewport,
        &mut |_, _| None,
        &mut geometry,
    );
    geometry
}

#[cfg(test)]
pub(super) fn encode_plan_geometry_with_rect_batch_resolver(
    plan: &DrawPlan,
    text_frame: &TextFrame,
    surface_size: SurfaceSize,
    canvas_viewport: CanvasViewport,
    resolve_rect_batch_texture: &mut impl FnMut(&[RectCommand], RectBatchCache) -> Option<TextureId>,
) -> PlanGeometry {
    let mut geometry = PlanGeometry::default();
    encode_plan_geometry_into(
        plan,
        text_frame,
        surface_size,
        canvas_viewport,
        resolve_rect_batch_texture,
        &mut geometry,
    );
    geometry
}

pub(super) fn encode_plan_geometry_into(
    plan: &DrawPlan,
    text_frame: &TextFrame,
    surface_size: SurfaceSize,
    canvas_viewport: CanvasViewport,
    resolve_rect_batch_texture: &mut impl FnMut(&[RectCommand], RectBatchCache) -> Option<TextureId>,
    geometry: &mut PlanGeometry,
) {
    let command_count = plan.commands.len();
    geometry.rects.clear();
    geometry.images.clear();
    geometry.steps.clear();
    geometry.rects.reserve(command_count.saturating_mul(RECT_INSTANCE_BYTES));
    geometry.images.reserve(command_count.saturating_mul(IMAGE_INSTANCE_BYTES));
    geometry.steps.reserve(command_count);
    let rects = &mut geometry.rects;
    let images = &mut geometry.images;
    let steps = &mut geometry.steps;
    let image_rotation_aspect = if surface_size.height == 0 {
        1.0
    } else {
        surface_size.width as f32 / surface_size.height as f32
    };
    // text instance buffer 上での現在位置 (quad 単位) と、次に参照する Text コマンド番号。
    let mut text_quad_cursor = 0_usize;
    let mut text_command_index = 0_usize;
    let mut ambient_index = 0_usize;
    let mut clips: Vec<Option<ScissorRect>> = Vec::new();
    let full_scissor = ScissorRect::full(surface_size);

    for command in &plan.commands {
        match command {
            DrawCommand::PushClip { rect } => {
                let parent = clips.last().copied().unwrap_or(full_scissor);
                let clip = parent.and_then(|parent| {
                    parent.intersect(ScissorRect::from_canvas(
                        *rect,
                        canvas_viewport,
                        surface_size,
                    )?)
                });
                clips.push(clip);
                steps.push(DrawStep::Scissor { rect: clip });
            }
            DrawCommand::PopClip => {
                clips.pop();
                steps.push(DrawStep::Scissor {
                    rect: clips.last().copied().unwrap_or(full_scissor),
                });
            }
            DrawCommand::Ambient { rect, blur, fade_edges, .. } => {
                let index = ambient_index;
                ambient_index += 1;
                let rect = canvas_viewport.transform_rect(ambient_output_rect(
                    *rect,
                    *blur,
                    *fade_edges,
                    canvas_viewport.content_size(),
                ));
                if index >= AMBIENT_MAX_DESTINATIONS || !visible_rect(rect) {
                    continue;
                }
                let start = images.len();
                encode_image_instance(
                    images,
                    &rect,
                    &UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    &Color::rgb(1.0, 1.0, 1.0),
                    0.0,
                    Point { x: 0.5, y: 0.5 },
                    image_rotation_aspect,
                    Point { x: 1.0, y: 1.0 },
                );
                push_or_extend_image(
                    steps,
                    ambient_texture_id(index),
                    BlendMode::Premultiplied,
                    true,
                    start..images.len(),
                );
            }
            DrawCommand::Rect { rect, color } => {
                let start = rects.len();
                let rect = canvas_viewport.transform_rect(*rect);
                if !visible_rect(rect) || !visible_alpha(color.a) {
                    continue;
                }
                rects.extend_from_slice(bytemuck::bytes_of(&[
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    color.r,
                    color.g,
                    color.b,
                    color.a,
                ]));
                push_or_extend_rects(steps, start..rects.len());
            }
            DrawCommand::RectBatch { rects: batch, cache } => {
                let transformed_batch: Vec<_> = batch
                    .iter()
                    .map(|command| canvas_viewport.transform_rect_command(*command))
                    .filter(|command| visible_rect(command.rect) && visible_alpha(command.color.a))
                    .collect();
                if transformed_batch.is_empty() {
                    continue;
                }
                if let Some(cache) = *cache
                    && let Some(texture) = resolve_rect_batch_texture(
                        &transformed_batch,
                        canvas_viewport.transform_rect_batch_cache(cache),
                    )
                {
                    let start = images.len();
                    let bounds = canvas_viewport.transform_rect(cache.bounds);
                    encode_image_instance(
                        images,
                        &bounds,
                        &UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                        &Color::rgb(1.0, 1.0, 1.0),
                        0.0,
                        Point { x: 0.5, y: 0.5 },
                        image_rotation_aspect,
                        Point { x: 1.0, y: 1.0 },
                    );
                    push_or_extend_image(
                        steps,
                        texture,
                        BlendMode::Premultiplied,
                        false,
                        start..images.len(),
                    );
                } else {
                    let start = rects.len();
                    for command in transformed_batch.iter() {
                        let rect = command.rect;
                        let color = command.color;
                        rects.extend_from_slice(bytemuck::bytes_of(&[
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            color.r,
                            color.g,
                            color.b,
                            color.a,
                        ]));
                    }
                    if rects.len() > start {
                        push_or_extend_rects(steps, start..rects.len());
                    }
                }
            }
            DrawCommand::Image { rect, uv, source_size, texture, tint, blend, linear_filter } => {
                let start = images.len();
                let rect = canvas_viewport.transform_rect(*rect);
                if !visible_rect(rect) || !visible_alpha(tint.a) {
                    continue;
                }
                let sampling_uv = sampling_uv_with_half_texel_inset(*uv, *source_size);
                encode_image_instance(
                    images,
                    &rect,
                    &sampling_uv,
                    tint,
                    0.0,
                    Point { x: 0.5, y: 0.5 },
                    image_rotation_aspect,
                    Point { x: 1.0, y: 1.0 },
                );
                push_or_extend_image(steps, *texture, *blend, *linear_filter, start..images.len());
            }
            DrawCommand::RotatedImage {
                rect,
                uv,
                source_size,
                texture,
                tint,
                blend,
                linear_filter,
                angle_rad,
                center,
                post_scale,
            } => {
                let start = images.len();
                let rect = canvas_viewport.transform_rect(*rect);
                if !visible_rect(rect) || !visible_alpha(tint.a) {
                    continue;
                }
                let sampling_uv = sampling_uv_with_half_texel_inset(*uv, *source_size);
                encode_image_instance(
                    images,
                    &rect,
                    &sampling_uv,
                    tint,
                    *angle_rad,
                    *center,
                    image_rotation_aspect,
                    *post_scale,
                );
                push_or_extend_image(steps, *texture, *blend, *linear_filter, start..images.len());
            }
            DrawCommand::Text { .. } => {
                let quad_count =
                    text_frame.command_quad_counts.get(text_command_index).copied().unwrap_or(0);
                let caret_rect =
                    text_frame.command_caret_rects.get(text_command_index).copied().flatten();
                text_command_index += 1;
                let start = text_quad_cursor * TEXT_INSTANCE_BYTES;
                text_quad_cursor += quad_count;
                let end = text_quad_cursor * TEXT_INSTANCE_BYTES;
                if quad_count > 0 {
                    push_or_extend_text(steps, start..end);
                }
                if let Some(command) = caret_rect {
                    let start = rects.len();
                    let rect = command.rect;
                    let color = command.color;
                    if !visible_rect(rect) || !visible_alpha(color.a) {
                        continue;
                    }
                    rects.extend_from_slice(bytemuck::bytes_of(&[
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        color.r,
                        color.g,
                        color.b,
                        color.a,
                    ]));
                    push_or_extend_rects(steps, start..rects.len());
                }
            }
        }
    }
}

pub(super) fn visible_rect(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width.abs() > f32::EPSILON
        && rect.height.abs() > f32::EPSILON
}

pub(super) fn visible_alpha(alpha: f32) -> bool {
    alpha.is_finite() && alpha > 0.0
}

pub(super) fn sampling_uv_with_half_texel_inset(
    uv: UvRect,
    source_size: Option<SkinImageSize>,
) -> UvRect {
    let Some(source_size) = source_size else {
        return uv;
    };
    if !source_size.width.is_finite()
        || !source_size.height.is_finite()
        || source_size.width <= 0.0
        || source_size.height <= 0.0
    {
        return uv;
    }

    let (x, width) = if uv_axis_covers_full_texture(uv.x, uv.width) {
        (uv.x, uv.width)
    } else {
        inset_uv_axis_by_half_texel(uv.x, uv.width, source_size.width)
    };
    let (y, height) = if uv_axis_covers_full_texture(uv.y, uv.height) {
        (uv.y, uv.height)
    } else {
        inset_uv_axis_by_half_texel(uv.y, uv.height, source_size.height)
    };

    UvRect { x, y, width, height }
}

pub(super) fn uv_axis_covers_full_texture(origin: f32, extent: f32) -> bool {
    const EPSILON: f32 = 1.0e-6;
    origin.abs() <= EPSILON && (extent - 1.0).abs() <= EPSILON
}

pub(super) fn inset_uv_axis_by_half_texel(
    origin: f32,
    extent: f32,
    source_extent: f32,
) -> (f32, f32) {
    if !origin.is_finite()
        || !extent.is_finite()
        || !source_extent.is_finite()
        || source_extent <= 1.0
    {
        return (origin, extent);
    }

    let texel = 1.0 / source_extent;
    let half_texel = texel * 0.5;
    if extent > texel {
        (origin + half_texel, extent - texel)
    } else if extent < -texel {
        (origin - half_texel, extent + texel)
    } else {
        (origin, extent)
    }
}

/// image インスタンス 1 件 (16 float) をバッファ末尾へ書き込む。
pub(super) fn encode_image_instance(
    images: &mut Vec<u8>,
    rect: &Rect,
    uv: &UvRect,
    tint: &Color,
    angle_rad: f32,
    center: Point,
    rotation_aspect: f32,
    post_scale: Point,
) {
    images.extend_from_slice(bytemuck::bytes_of(&[
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        uv.x,
        uv.y,
        uv.width,
        uv.height,
        tint.r,
        tint.g,
        tint.b,
        tint.a,
        angle_rad,
        center.x,
        center.y,
        rotation_aspect,
        post_scale.x,
        post_scale.y,
    ]));
}

pub(super) fn push_or_extend_rects(steps: &mut Vec<DrawStep>, range: Range<usize>) {
    match steps.last_mut() {
        Some(DrawStep::Rects { range: existing }) => existing.end = range.end,
        _ => steps.push(DrawStep::Rects { range }),
    }
}

pub(super) fn push_or_extend_text(steps: &mut Vec<DrawStep>, range: Range<usize>) {
    match steps.last_mut() {
        Some(DrawStep::Text { range: existing }) => existing.end = range.end,
        _ => steps.push(DrawStep::Text { range }),
    }
}

pub(super) fn push_or_extend_image(
    steps: &mut Vec<DrawStep>,
    texture: TextureId,
    blend: BlendMode,
    linear: bool,
    range: Range<usize>,
) {
    if let Some(DrawStep::Image { texture: t, blend: b, linear: l, range: existing }) =
        steps.last_mut()
        && *t == texture
        && *b == blend
        && *l == linear
    {
        existing.end = range.end;
        return;
    }
    steps.push(DrawStep::Image { texture, blend, linear, range });
}

#[cfg(test)]
pub(super) fn build_text_frame(
    plan: &DrawPlan,
    default_font: &FontArc,
    fonts: &HashMap<String, FontArc>,
    bitmap_fonts: &HashMap<String, BitmapFont>,
    surface: SurfaceSize,
) -> TextFrame {
    if !surface.is_drawable() {
        return TextFrame::default();
    }

    let mut builder = TextAtlasBuilder::new(TEXT_ATLAS_WIDTH);
    // 各 Text コマンドが生成した quad 数を記録し、描画ステップへ分割できるようにする。
    let mut command_quad_counts = Vec::new();
    let mut command_caret_rects = Vec::new();
    for command in &plan.commands {
        let DrawCommand::Text { origin, text, style, caret, post_scale } = command else {
            continue;
        };
        let quads_before = builder.quads.len();
        if let Some(bitmap_font) =
            style.font_id.as_ref().and_then(|font_id| bitmap_fonts.get(font_id))
        {
            builder.push_bitmap_text(origin, text, style.clone(), bitmap_font, surface);
            scale_text_quads(&mut builder.quads[quads_before..], *origin, *post_scale);
            command_caret_rects.push(
                caret
                    .and_then(|caret| {
                        bitmap_text_caret_rect(origin, text, style, bitmap_font, surface, caret)
                    })
                    .map(|caret| scale_text_rect_command(caret, *origin, *post_scale)),
            );
        } else {
            let font = style
                .font_id
                .as_ref()
                .and_then(|font_id| fonts.get(font_id))
                .unwrap_or(default_font);
            builder.push_text(origin, text, style.clone(), font, surface);
            scale_text_quads(&mut builder.quads[quads_before..], *origin, *post_scale);
            command_caret_rects.push(
                caret
                    .and_then(|caret| {
                        vector_text_caret_rect(origin, text, style, font, surface, caret)
                    })
                    .map(|caret| scale_text_rect_command(caret, *origin, *post_scale)),
            );
        }
        command_quad_counts.push(builder.quads.len() - quads_before);
    }
    let mut frame = builder.finish();
    frame.command_quad_counts = command_quad_counts;
    frame.command_caret_rects = command_caret_rects;
    frame
}
