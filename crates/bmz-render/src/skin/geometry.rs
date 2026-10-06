use super::*;

pub(super) fn wrap_skin_destination_clip(
    mut items: Vec<SkinRenderItem>,
    clip: Option<Rect>,
) -> Vec<SkinRenderItem> {
    if let Some(rect) = clip
        && !items.is_empty()
    {
        items.insert(0, SkinRenderItem::PushClip { rect });
        items.push(SkinRenderItem::PopClip);
    }
    items
}

pub(super) fn destination_has_clip(destination: &SkinDestinationDef) -> bool {
    destination.dst.iter().any(|entry| {
        let frames = match entry {
            SkinDstEntry::Frame(frame) => std::slice::from_ref(frame),
            SkinDstEntry::Conditional { frames, .. } => frames.as_slice(),
        };
        frames.iter().any(|frame| {
            [frame.clip_x, frame.clip_y, frame.clip_w, frame.clip_h].iter().any(Option::is_some)
        })
    })
}

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
        3 => BlendMode::Subtract,
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
    pub(super) lr2_style: Option<SkinLr2FrameStyle>,
    pub(super) time: i32,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) w: i32,
    pub(super) h: i32,
    pub(super) clip: ResolvedSkinClip,
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
            lr2_style: None,
            time: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            clip: ResolvedSkinClip::default(),
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

impl ResolvedSkinFrame {
    /// Shape caches describe already resolved geometry/color, not its timeline
    /// or the scissor applied after compositing the cached texture.
    pub(super) fn geometry_cache_frame(mut self) -> Self {
        self.time = 0;
        self.acc = 0;
        self.clip = ResolvedSkinClip::default();
        self.apply_offset_alpha = true;
        self
    }

    /// Remove clip before caching object geometry: changing only the scissor
    /// must not invalidate its shape/number cache.
    pub(super) fn take_clip(&mut self, width: u32, height: u32) -> Option<Rect> {
        let [x, y, w, h] = std::mem::take(&mut self.clip).values()?;
        (w > 0.0 && h > 0.0).then(|| Rect {
            x: x / width.max(1) as f32,
            y: 1.0 - (y + h) / height.max(1) as f32,
            width: w / width.max(1) as f32,
            height: h / height.max(1) as f32,
        })
    }

    pub(super) fn blend(self, fallback: BlendMode) -> BlendMode {
        self.lr2_style.map_or(fallback, |style| skin_blend_mode(style.blend))
    }

    pub(super) fn linear_filter(self, fallback: bool) -> bool {
        self.lr2_style.map_or(fallback, |style| style.filter != 0)
    }

    pub(super) fn center(self, fallback: i32) -> i32 {
        self.lr2_style.map_or(fallback, |style| style.center)
    }
}

/// Float bits retain subpixel interpolation while keeping frame cache keys Eq/Hash.
/// Missing components remain missing until inherited from an earlier keyframe.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(super) struct ResolvedSkinClip([Option<u32>; 4]);

impl ResolvedSkinClip {
    pub(super) fn inherit(&mut self, animation: &SkinAnimationDef) {
        for (component, value) in self.0.iter_mut().zip([
            animation.clip_x,
            animation.clip_y,
            animation.clip_w,
            animation.clip_h,
        ]) {
            if let Some(value) = value.filter(|&value| value != i32::MIN) {
                *component = Some((value as f32).to_bits());
            }
        }
    }

    fn values(self) -> Option<[f32; 4]> {
        let [x, y, w, h] = self.0;
        Some([f32::from_bits(x?), f32::from_bits(y?), f32::from_bits(w?), f32::from_bits(h?)])
    }

    pub(super) fn interpolate(self, end: Self, rate: f32) -> Self {
        let (Some(start), Some(end)) = (self.values(), end.values()) else {
            // A later complete clip does not activate an incomplete start frame.
            return self;
        };
        Self(std::array::from_fn(|i| Some((start[i] + (end[i] - start[i]) * rate).to_bits())))
    }

    pub(super) fn offset(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(values) = self.values() {
            let delta = [x, y, w, h];
            self.0 = std::array::from_fn(|i| Some((values[i] + delta[i]).to_bits()));
        }
    }
}

pub(super) fn lr2_note_destination(
    document: &SkinDocument,
    lane: Lane,
    key_mode: KeyMode,
) -> Option<&SkinDestinationDef> {
    document.note.as_ref()?.lr2_dst.get(beatoraja_note_index(lane, key_mode))?.as_ref()
}

pub(super) fn lr2_note_frame(
    document: &SkinDocument,
    lane: Lane,
    key_mode: KeyMode,
    options: &[i32],
    state: &SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    let destination = lr2_note_destination(document, lane, key_mode)?;
    if !destination_ops_match(destination, options, state) {
        return None;
    }
    resolve_destination_frame(
        destination,
        destination_timer_elapsed_ms(destination, state)?,
        options,
        state,
    )
}

pub(super) fn note_lane_area_for_state(
    document: &SkinDocument,
    lane: Lane,
    key_mode: KeyMode,
    options: &[i32],
    state: &SkinDrawState,
) -> Option<Rect> {
    let mut area = document.note_lane_area(lane, key_mode, options)?;
    if lr2_note_destination(document, lane, key_mode).is_some() {
        let frame = lr2_note_frame(document, lane, key_mode, options, state)?;
        area.x = frame.x as f32 / document.w.max(1) as f32;
        area.width = frame.w as f32 / document.w.max(1) as f32;
        // Scroll distance remains relative to the initial lane height. Animating
        // the judgement position moves notes without changing their speed.
        area.y = 1.0 - frame.y as f32 / document.h.max(1) as f32 - area.height;
    }
    Some(area)
}
