use crate::skin::*;

pub(in crate::skin::document_render) fn hiterror_visualizer_items(
    document: &SkinDocument,
    visualizer: &SkinHitErrorVisualizerDef,
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    state: &SkinDrawState,
) -> Vec<SkinRenderItem> {
    let window = visualizer.window_length.clamp(1, 100) as usize;
    let layout = HitErrorLayout {
        rect: normalize_skin_frame_rect(frame, document.w, document.h),
        width: visualizer.width.max(1) as f32,
        height: (window * 2) as f32,
        flip_x: frame.w < 0,
        flip_y: frame.h < 0,
        frame,
        blend: frame.blend(skin_blend_mode(destination.blend)),
    };
    if layout.rect.width == 0.0 || layout.rect.height == 0.0 {
        return Vec::new();
    }
    let line_width = visualizer.line_width.clamp(1, 4) as f32;
    let center_ms = visualizer.judge_width_millis.max(1) as f32;
    // All geometry is in SkinHitErrorVisualizer's internal pixmap until push().
    // Both line width and horizontal timing offsets scale with the destination.
    let center_x = ((layout.width - line_width) / 2.0).trunc();
    let timing_x = |sample: i64| {
        // BMZ records input minus note time (FAST < 0), opposite to beatoraja's
        // recentJudges. Preserve FAST on the left and SLOW on the right.
        center_x
            + (sample.clamp(-center_ms as i64, center_ms as i64) as f32 * layout.width
                / (center_ms * 2.0 + 1.0))
                .trunc()
    };
    let mut items = Vec::new();
    if visualizer.hiterror_mode == 1 {
        let areas = timing_judge_areas(state);
        let recent = &state.hit_error_ring;
        let index = state.hit_error_ring_index % recent.len();
        for age in (1..=window).rev() {
            let sample = recent[(index + recent.len() - window + age) % recent.len()];
            if sample == bmz_gameplay::hit_error::HIT_ERROR_EMPTY {
                continue;
            }
            let color = if visualizer.color_mode == 1 {
                let judge = areas
                    .iter()
                    .take(4)
                    .position(|area| {
                        sample as f32 > area.late_ms && (sample as f32) < area.early_ms
                    })
                    .unwrap_or(4);
                if judge == 4 && visualizer.transparent == 1 {
                    continue;
                }
                hit_error_color(
                    [
                        &visualizer.pg_color,
                        &visualizer.gr_color,
                        &visualizer.gd_color,
                        &visualizer.bd_color,
                        &visualizer.pr_color,
                    ][judge],
                )
            } else {
                let color = hit_error_color(&visualizer.line_color);
                color.with_alpha(color.a * (2.0 * age as f32 / window as f32).min(1.0))
            };
            let (y, height) = if visualizer.draw_decay == 1 {
                ((window - age) as f32, (age * 2) as f32)
            } else {
                (0.0, layout.height)
            };
            layout.push(&mut items, timing_x(sample), y, line_width, height, color);
        }
    }
    // The centre line covers individual hits, and the EMA covers the centre line.
    layout.push(
        &mut items,
        center_x,
        0.0,
        line_width,
        layout.height,
        hit_error_color(&visualizer.center_color),
    );
    let ema_x = timing_x(state.hit_error_ema.get(&visualizer.id).copied().unwrap_or(0));
    let ema_color = hit_error_color(&visualizer.ema_color);
    if matches!(visualizer.ema_mode, 1 | 3) {
        layout.push(&mut items, ema_x, 0.0, line_width, layout.height, ema_color);
    }
    if matches!(visualizer.ema_mode, 2 | 3) {
        let mut half_width = (layout.width * 0.01).trunc();
        if half_width as i64 % 2 != 0 {
            half_width += 1.0;
        }
        let tip_x = ema_x + (line_width / 2.0).trunc();
        let height = (layout.height / 3.0).trunc();
        // Rasterize the small pixmap triangle as scanlines. No new shared GPU
        // primitive or change to ordinary image/graph rendering is necessary.
        for y in 0..height as usize {
            let radius = half_width * (1.0 - y as f32 / height);
            layout.push(&mut items, tip_x - radius, y as f32, radius * 2.0, 1.0, ema_color);
        }
    }
    items
}

fn hit_error_color(value: &str) -> Color {
    skin_hex_color(value).unwrap_or(Color::rgba(1.0, 0.0, 0.0, 1.0))
}

struct HitErrorLayout {
    rect: Rect,
    width: f32,
    height: f32,
    flip_x: bool,
    flip_y: bool,
    frame: ResolvedSkinFrame,
    blend: BlendMode,
}

impl HitErrorLayout {
    fn push(
        &self,
        items: &mut Vec<SkinRenderItem>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    ) {
        // Pixmap drawing clips at the source bounds before destination scaling.
        let right = (x + width).min(self.width);
        let bottom = (y + height).min(self.height);
        let x = x.max(0.0);
        let y = y.max(0.0);
        if right <= x || bottom <= y || color.a == 0.0 {
            return;
        }
        let offset_x = if self.flip_x { self.width - right } else { x };
        let offset_y = if self.flip_y { self.height - bottom } else { y };
        items.push(SkinRenderItem::Rect {
            rect: Rect {
                x: self.rect.x + self.rect.width * offset_x / self.width,
                y: self.rect.y + self.rect.height * offset_y / self.height,
                width: self.rect.width * (right - x) / self.width,
                height: self.rect.height * (bottom - y) / self.height,
            },
            color: Color::rgba(
                color.r * self.frame.r as f32 / 255.0,
                color.g * self.frame.g as f32 / 255.0,
                color.b * self.frame.b as f32 / 255.0,
                color.a * self.frame.a as f32 / 255.0,
            ),
            blend: self.blend,
        });
    }
}
