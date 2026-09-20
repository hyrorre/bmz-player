use super::*;

pub(super) fn skin_image_for_destination_id<'a>(
    destination_id: &str,
    images: &'a SkinImageLookup<'_>,
) -> Option<&'a SkinImageDef> {
    images.get(destination_id)
}

pub(super) fn beatoraja_direct_image_source_id(destination_id: &str) -> Option<String> {
    let id = destination_id.parse::<i32>().ok()?;
    (id < 0).then(|| (-id).to_string())
}

pub(super) fn is_lift_lane_cover_id(id: &str) -> bool {
    id.eq_ignore_ascii_case("liftcover")
        || id.eq_ignore_ascii_case("lift-cover")
        || id.eq_ignore_ascii_case("lift_cover")
        || id
            .as_bytes()
            .windows(b"liftcover".len())
            .any(|part| part.eq_ignore_ascii_case(b"liftcover"))
}

/// beatoraja `SkinHidden` 準拠: `disappear_line` より下 (y が小さい側) を切り、上側だけ残す。
/// 上端が消失ライン以下のときは描画しない。
pub(super) fn clip_skin_cover_to_disappear_line(
    frame: &mut ResolvedSkinFrame,
    uv: &mut TextureRegion,
    disappear_line: i32,
    link_lift: bool,
    state: &SkinDrawState,
) {
    if disappear_line <= 0 || frame.h <= 0 {
        return;
    }
    let mut disappear_y = disappear_line;
    if link_lift {
        disappear_y = disappear_y.saturating_add(state.offset_lift_px);
    }
    let bottom = frame.y;
    let top = bottom.saturating_add(frame.h);
    if top <= disappear_y {
        frame.h = 0;
        return;
    }
    // 下端が消失ライン以上なら加工不要 (SUDDEN+ の全開など)
    if bottom >= disappear_y {
        return;
    }
    // 消失ラインより下 (y が小さい側) だけ切り、上側を残す
    let original_h = frame.h.max(1);
    let new_h = top - disappear_y;
    let ratio = new_h as f32 / original_h as f32;
    frame.y = disappear_y;
    frame.h = new_h;
    uv.height *= ratio;
}

pub(super) fn normalize_skin_frame_rect(
    frame: ResolvedSkinFrame,
    canvas_width: u32,
    canvas_height: u32,
) -> Rect {
    let canvas_width = canvas_width.max(1) as f32;
    let canvas_height = canvas_height.max(1) as f32;
    let x0 = frame.x as f32;
    let x1 = (frame.x + frame.w) as f32;
    let y0 = frame.y as f32;
    let y1 = (frame.y + frame.h) as f32;
    Rect {
        x: x0.min(x1) / canvas_width,
        y: (canvas_height - y0.max(y1)) / canvas_height,
        width: (x1 - x0).abs() / canvas_width,
        height: (y1 - y0).abs() / canvas_height,
    }
}

pub(super) fn rect_contains(rect: Rect, x: f32, y: f32) -> bool {
    rect.x <= x && x <= rect.x + rect.width && rect.y <= y && y <= rect.y + rect.height
}

pub(super) fn destination_mouse_rect_contains(
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    state: &SkinDrawState,
) -> bool {
    let Some(mouse_rect) = destination.mouse_rect else {
        return true;
    };
    let (Some(mouse_x), Some(mouse_y)) = (state.mouse_x, state.mouse_y) else {
        return false;
    };
    let relative_x = mouse_x - frame.x as f32;
    let relative_y = mouse_y - frame.y as f32;
    let x0 = mouse_rect.x as f32;
    let x1 = (mouse_rect.x + mouse_rect.w) as f32;
    let y0 = mouse_rect.y as f32;
    let y1 = (mouse_rect.y + mouse_rect.h) as f32;
    x0.min(x1) <= relative_x
        && relative_x <= x0.max(x1)
        && y0.min(y1) <= relative_y
        && relative_y <= y0.max(y1)
}

pub(super) fn slider_value_at(
    slider: &SkinSliderDef,
    frame: ResolvedSkinFrame,
    x: f32,
    y: f32,
) -> Option<f32> {
    // beatoraja SkinSlider.mousePressed: hit track is based on destination origin +
    // range along the movement axis (not the thumb center / half size).
    let range = slider.range.unsigned_abs() as f32;
    if range <= f32::EPSILON {
        return None;
    }
    let frame_x = frame.x as f32;
    let frame_y = frame.y as f32;
    let frame_w = frame.w as f32;
    let frame_h = frame.h as f32;
    let value = match slider.angle {
        0 if frame_x <= x && x <= frame_x + frame_w && frame_y <= y && y <= frame_y + range => {
            (y - frame_y) / range
        }
        1 if frame_x <= x && x <= frame_x + range && frame_y <= y && y <= frame_y + frame_h => {
            (x - frame_x) / range
        }
        2 if frame_x <= x && x <= frame_x + frame_w && frame_y - range <= y && y <= frame_y => {
            (frame_y - y) / range
        }
        3 if frame_x - range <= x && x <= frame_x && frame_y <= y && y <= frame_y + frame_h => {
            (frame_x - x) / range
        }
        _ => return None,
    };
    Some(value.clamp(0.0, 1.0))
}

pub(super) fn scroll_slider_value_at(
    slider: &SkinSliderDef,
    frame: ResolvedSkinFrame,
    x: f32,
    y: f32,
) -> Option<f32> {
    slider_value_at(slider, frame, x, y)
}
