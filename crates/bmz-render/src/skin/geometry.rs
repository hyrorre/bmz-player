use super::*;

pub(super) fn wrap_ambient_destination(
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    width: u32,
    height: u32,
    mut items: Vec<SkinRenderItem>,
) -> Vec<SkinRenderItem> {
    if !destination.ambient || items.is_empty() {
        return items;
    }
    let percent = |value: f32, default: f32, maximum: f32| {
        if value.is_finite() { value.clamp(0.0, maximum) } else { default }
    };
    let blur = percent(destination.ambient_blur, 50.0, 100.0);
    let fade_edges = destination.ambient_mode == SkinAmbientMode::Spread;
    let mut bounds = normalize_skin_frame_rect(frame, width, height);
    if fade_edges {
        // Fit has already been applied to each BGA layer: omit letterbox space,
        // but retain the relative placement of differently shaped layers.
        let layer_rect = |item: &SkinRenderItem| match item {
            SkinRenderItem::Image { rect, .. }
            | SkinRenderItem::RotatedImage { rect, .. }
            | SkinRenderItem::Rect { rect, .. } => Some(*rect),
            _ => None,
        };
        let mut rects = items.iter().filter_map(layer_rect).filter(|rect| {
            rect.x.is_finite()
                && rect.y.is_finite()
                && rect.width.is_finite()
                && rect.height.is_finite()
                && rect.width > 0.0
                && rect.height > 0.0
        });
        if let Some(first) = rects.next() {
            bounds = rects.fold(first, |bounds, rect| {
                let x = bounds.x.min(rect.x);
                let y = bounds.y.min(rect.y);
                Rect {
                    x,
                    y,
                    width: (bounds.x + bounds.width).max(rect.x + rect.width) - x,
                    height: (bounds.y + bounds.height).max(rect.y + rect.height) - y,
                }
            });
        }
        let scale = 1.0 + percent(destination.ambient_spread, 20.0, 200.0) / 100.0;
        let cx = bounds.x + bounds.width / 2.0;
        let cy = bounds.y + bounds.height / 2.0;
        let expand = |rect: &mut Rect| {
            rect.x = cx + (rect.x - cx) * scale;
            rect.y = cy + (rect.y - cy) * scale;
            rect.width *= scale;
            rect.height *= scale;
        };
        expand(&mut bounds);
        for item in &mut items {
            match item {
                SkinRenderItem::Image { rect, .. }
                | SkinRenderItem::RotatedImage { rect, .. }
                | SkinRenderItem::Rect { rect, .. } => expand(rect),
                _ => {}
            }
        }
    }
    // Zero means no blur, including no low-resolution intermediate image.
    if blur == 0.0 {
        return items;
    }
    vec![SkinRenderItem::Ambient { rect: bounds, blur, fade_edges, layers: items }]
}

/// beatoraja `SkinObjectRenderer.setBlend` と同じ destination blend 対応。
pub(super) fn skin_blend_mode(blend: i32) -> BlendMode {
    match blend {
        2 => BlendMode::Add,
        4 => BlendMode::Multiply,
        _ => BlendMode::Normal,
    }
}

pub(super) fn multiply_bga_tints(destination: Color, bga: SkinBgaFrame) -> Color {
    Color::rgba(
        destination.r * bga.tint_r,
        destination.g * bga.tint_g,
        destination.b * bga.tint_b,
        destination.a * bga.tint_a,
    )
}

pub(super) fn bga_image_item(
    bga: SkinBgaFrame,
    stretch: i32,
    rect: Rect,
    tint: Color,
    blend: BlendMode,
    canvas_width: u32,
    canvas_height: u32,
    linear_filter: bool,
) -> SkinRenderItem {
    let (rect, uv) = stretch_skin_image_geometry(
        stretch,
        rect,
        TextureRegion::default(),
        bga.source_size,
        canvas_width,
        canvas_height,
    );
    SkinRenderItem::Image {
        texture: bga.texture,
        rect,
        uv,
        tint,
        blend,
        scale: SkinImageScale::Stretch,
        border: None,
        source_size: Some(bga.source_size),
        linear_filter,
    }
}

pub(super) fn special_image_render_item(
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    canvas_width: u32,
    canvas_height: u32,
) -> Option<SkinRenderItem> {
    let (base_r, base_g, base_b) = match destination.id.as_str() {
        "-110" => (0.0, 0.0, 0.0),
        "-111" => (1.0, 1.0, 1.0),
        _ => return None,
    };
    Some(SkinRenderItem::Rect {
        rect: normalize_skin_frame_rect(frame, canvas_width, canvas_height),
        color: Color::rgba(
            base_r * frame.r as f32 / 255.0,
            base_g * frame.g as f32 / 255.0,
            base_b * frame.b as f32 / 255.0,
            frame.a as f32 / 255.0,
        ),
        blend: skin_blend_mode(destination.blend),
    })
}

pub(super) fn stretch_skin_image_geometry(
    stretch: i32,
    rect: Rect,
    uv: TextureRegion,
    source_size: SkinImageSize,
    canvas_width: u32,
    canvas_height: u32,
) -> (Rect, TextureRegion) {
    if stretch <= 0 || rect.width <= 0.0 || rect.height <= 0.0 {
        return (rect, uv);
    }

    let canvas_width = canvas_width.max(1) as f32;
    let canvas_height = canvas_height.max(1) as f32;
    let source_width = (uv.width.abs() * source_size.width).max(1.0);
    let source_height = (uv.height.abs() * source_size.height).max(1.0);
    let rect_px = SkinPixelRect {
        x: rect.x * canvas_width,
        y: rect.y * canvas_height,
        width: rect.width * canvas_width,
        height: rect.height * canvas_height,
    };

    let (rect_px, uv) = match stretch {
        1 => (fit_inner_rect(rect_px, source_width, source_height), uv),
        2 => (fit_outer_rect(rect_px, source_width, source_height), uv),
        3 => fit_outer_trimmed_rect(rect_px, uv, source_width, source_height),
        4 => (fit_width_rect(rect_px, source_width, source_height), uv),
        5 => fit_width_trimmed_rect(rect_px, uv, source_width, source_height),
        6 => (fit_height_rect(rect_px, source_width, source_height), uv),
        7 => fit_height_trimmed_rect(rect_px, uv, source_width, source_height),
        8 => (fit_no_expanding_rect(rect_px, source_width, source_height), uv),
        9 => (resize_about_center(rect_px, source_width, source_height), uv),
        10 => fit_no_resize_trimmed_rect(rect_px, uv, source_width, source_height),
        _ => (rect_px, uv),
    };

    (
        Rect {
            x: rect_px.x / canvas_width,
            y: rect_px.y / canvas_height,
            width: rect_px.width / canvas_width,
            height: rect_px.height / canvas_height,
        },
        uv,
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SkinPixelRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

pub(super) fn fit_inner_rect(
    rect: SkinPixelRect,
    source_width: f32,
    source_height: f32,
) -> SkinPixelRect {
    let scale_x = rect.width / source_width;
    let scale_y = rect.height / source_height;
    if scale_x <= scale_y {
        resize_about_center(rect, rect.width, source_height * scale_x)
    } else {
        resize_about_center(rect, source_width * scale_y, rect.height)
    }
}

pub(super) fn fit_outer_rect(
    rect: SkinPixelRect,
    source_width: f32,
    source_height: f32,
) -> SkinPixelRect {
    let scale_x = rect.width / source_width;
    let scale_y = rect.height / source_height;
    if scale_x >= scale_y {
        resize_about_center(rect, rect.width, source_height * scale_x)
    } else {
        resize_about_center(rect, source_width * scale_y, rect.height)
    }
}

pub(super) fn fit_width_rect(
    rect: SkinPixelRect,
    source_width: f32,
    source_height: f32,
) -> SkinPixelRect {
    resize_about_center(rect, rect.width, source_height * rect.width / source_width)
}

pub(super) fn fit_height_rect(
    rect: SkinPixelRect,
    source_width: f32,
    source_height: f32,
) -> SkinPixelRect {
    resize_about_center(rect, source_width * rect.height / source_height, rect.height)
}

pub(super) fn fit_no_expanding_rect(
    rect: SkinPixelRect,
    source_width: f32,
    source_height: f32,
) -> SkinPixelRect {
    let scale = (rect.width / source_width).min(rect.height / source_height).min(1.0);
    resize_about_center(rect, source_width * scale, source_height * scale)
}

pub(super) fn fit_outer_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let scale_x = rect.width / source_width;
    let scale_y = rect.height / source_height;
    if scale_x >= scale_y {
        fit_height_or_trim(rect, uv, source_height * scale_x)
    } else {
        fit_width_or_trim(rect, uv, source_width * scale_y)
    }
}

pub(super) fn fit_width_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let scale = rect.width / source_width;
    fit_height_or_trim(rect, uv, source_height * scale)
}

pub(super) fn fit_height_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let scale = rect.height / source_height;
    fit_width_or_trim(rect, uv, source_width * scale)
}

pub(super) fn fit_no_resize_trimmed_rect(
    rect: SkinPixelRect,
    uv: TextureRegion,
    source_width: f32,
    source_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    let (rect, uv) = fit_width_or_trim(rect, uv, source_width);
    fit_height_or_trim(rect, uv, source_height)
}

pub(super) fn fit_width_or_trim(
    rect: SkinPixelRect,
    uv: TextureRegion,
    target_width: f32,
) -> (SkinPixelRect, TextureRegion) {
    if rect.width < target_width {
        let visible_ratio = (rect.width / target_width).clamp(0.0, 1.0);
        let trim = uv.width * (1.0 - visible_ratio) * 0.5;
        (rect, TextureRegion { x: uv.x + trim, width: uv.width - trim * 2.0, ..uv })
    } else {
        (resize_about_center(rect, target_width, rect.height), uv)
    }
}

pub(super) fn fit_height_or_trim(
    rect: SkinPixelRect,
    uv: TextureRegion,
    target_height: f32,
) -> (SkinPixelRect, TextureRegion) {
    if rect.height < target_height {
        let visible_ratio = (rect.height / target_height).clamp(0.0, 1.0);
        let trim = uv.height * (1.0 - visible_ratio) * 0.5;
        (rect, TextureRegion { y: uv.y + trim, height: uv.height - trim * 2.0, ..uv })
    } else {
        (resize_about_center(rect, rect.width, target_height), uv)
    }
}

pub(super) fn resize_about_center(rect: SkinPixelRect, width: f32, height: f32) -> SkinPixelRect {
    SkinPixelRect {
        x: rect.x + (rect.width - width) * 0.5,
        y: rect.y + (rect.height - height) * 0.5,
        width,
        height,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ResolvedSkinFrame {
    pub(super) time: i32,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) acc: i32,
    pub(super) a: i32,
    pub(super) r: i32,
    pub(super) g: i32,
    pub(super) b: i32,
    pub(super) angle: i32,
    /// beatoraja `prepareColor` がこのフレームで offset.a を加算するか。
    pub(super) apply_offset_alpha: bool,
}

impl Default for ResolvedSkinFrame {
    fn default() -> Self {
        Self {
            time: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            acc: 0,
            a: 255,
            r: 255,
            g: 255,
            b: 255,
            angle: 0,
            apply_offset_alpha: true,
        }
    }
}
