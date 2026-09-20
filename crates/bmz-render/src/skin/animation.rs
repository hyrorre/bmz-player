use super::*;

pub(super) fn destination_entry_at<'a>(
    entries: &'a [DestinationListEntry],
    index: usize,
    enabled_options: &[i32],
) -> Option<&'a SkinDestinationDef> {
    destination_entries(entries, enabled_options).into_iter().nth(index)
}

pub(super) fn destination_entries<'a>(
    entries: &'a [DestinationListEntry],
    enabled_options: &[i32],
) -> Vec<&'a SkinDestinationDef> {
    let mut result = Vec::new();
    for entry in entries {
        match entry {
            DestinationListEntry::Single(destination) => result.push(destination),
            DestinationListEntry::Conditional { if_ops, destinations } => {
                if test_skin_dst_if(if_ops, enabled_options) {
                    result.extend(destinations);
                }
            }
        }
    }
    result
}

/// Expands a dst entry list into animation frames, filtering conditional entries by `enabled_options`.
pub(super) fn flatten_dst_entries(
    dst: &[SkinDstEntry],
    enabled_options: &[i32],
) -> Vec<SkinAnimationDef> {
    let mut result = Vec::new();
    for entry in dst {
        match entry {
            SkinDstEntry::Frame(anim) => result.push(*anim),
            SkinDstEntry::Conditional { if_ops, frames } => {
                if test_skin_dst_if(if_ops, enabled_options) {
                    result.extend(frames.iter().copied());
                }
            }
        }
    }
    result
}

pub(super) fn apply_skin_offset_to_frame(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: &SkinDrawState,
    include_hidden_cover_offsets: bool,
) {
    apply_skin_offset_to_frame_inner(destination, frame, state, include_hidden_cover_offsets, false)
}

/// beatoraja の `SkinObject.setRelative(true)` 相当 (SkinNumber 等で使用)。
/// destination の offset を適用する際、x/y シフトはスキップし w/h/r/a のみ加算する。
pub(super) fn apply_skin_offset_to_frame_relative(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: &SkinDrawState,
) {
    apply_skin_offset_to_frame_inner(destination, frame, state, false, true)
}

pub(super) fn apply_skin_offset_to_frame_inner(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: &SkinDrawState,
    include_hidden_cover_offsets: bool,
    relative: bool,
) {
    let mut ids = normalized_destination_offset_ids(destination);
    if include_hidden_cover_offsets {
        push_unique_skin_offset_id(&mut ids, 3);
        push_unique_skin_offset_id(&mut ids, 5);
    }

    apply_skin_offset_ids_to_frame(&ids, frame, state, relative);
}

pub(super) fn apply_skin_offset_ids_to_frame(
    ids: &[i32],
    frame: &mut ResolvedSkinFrame,
    state: &SkinDrawState,
    relative: bool,
) {
    for &offset_id in ids {
        let Some(offset) = effective_skin_offset(offset_id, state) else {
            continue;
        };
        if !relative {
            // beatoraja: !relative のとき x/y は中央アンカーでシフト
            frame.x += offset.x - offset.w / 2;
            frame.y += offset.y - offset.h / 2;
        }
        frame.w += offset.w;
        frame.h += offset.h;
        frame.angle += offset.r;
        if frame.apply_offset_alpha {
            frame.a = (frame.a + offset.a).clamp(0, 255);
        }
    }
}

pub(super) fn normalized_destination_offset_ids(destination: &SkinDestinationDef) -> Vec<i32> {
    let mut ids =
        Vec::with_capacity(destination.offsets.len() + usize::from(destination.offset != 0));
    for &id in &destination.offsets {
        push_unique_skin_offset_id(&mut ids, id);
    }
    push_unique_skin_offset_id(&mut ids, destination.offset);
    ids
}

pub(super) fn push_unique_skin_offset_id(ids: &mut Vec<i32>, id: i32) {
    if (1..=BEATORAJA_SKIN_OFFSET_MAX).contains(&id) && !ids.contains(&id) {
        ids.push(id);
    }
}

pub(super) fn effective_skin_offset(id: i32, state: &SkinDrawState) -> Option<SkinOffsetValue> {
    if !(1..=BEATORAJA_SKIN_OFFSET_MAX).contains(&id) {
        return None;
    }
    let mut offset = state.skin_offsets.get(id).unwrap_or_default();
    match id {
        3 => offset.y = state.offset_lift_px,
        4 => offset.y = state.offset_lanecover_px,
        5 => {
            offset.y = state.offset_hidden_cover_px;
            offset.a = if state.hidden_enabled { 0 } else { -255 };
        }
        _ => {}
    }
    Some(offset)
}

/// `note_y` progress (0=判定ライン, 1=最奥) を `note.dst` エリア内の正規化 Y に変換する。
/// LIFT (`offset_lift_px`) により判定ラインを上げ、スクロール範囲を縮める。
pub(super) fn note_progress_to_y(
    area: Rect,
    progress: f32,
    state: &SkinDrawState,
    canvas_h: f32,
) -> f32 {
    let judge_bottom = note_judge_bottom_y(area, state, canvas_h);
    let progress = progress.clamp(0.0, 1.0);
    judge_bottom - progress * (judge_bottom - area.y)
}

pub(super) fn note_judge_bottom_y(area: Rect, state: &SkinDrawState, canvas_h: f32) -> f32 {
    let lift_norm = state.offset_lift_px as f32 / canvas_h.max(1.0);
    let scroll_top = area.y;
    (area.y + area.height - lift_norm).max(scroll_top)
}

/// 小節線 (`note.group`) 向けオフセット適用。Notes offset (30) はノーツ専用のため除外する。
pub(super) fn apply_bar_line_skin_offsets_to_frame(
    destination: &SkinDestinationDef,
    frame: &mut ResolvedSkinFrame,
    state: &SkinDrawState,
) {
    let ids = normalized_destination_offset_ids(destination);
    apply_skin_offset_ids_to_frame(&ids, frame, state, false);
    apply_bar_line_offset_to_frame(frame, state);
}

pub(super) fn apply_bar_line_offset_to_frame(frame: &mut ResolvedSkinFrame, state: &SkinDrawState) {
    if let Some(offset) = state.skin_offsets.get(SKIN_OFFSET_BAR_LINE) {
        frame.h = (frame.h + offset.h).max(0);
    }
}

pub(super) fn apply_all_offset_to_render_item(
    item: SkinRenderItem,
    state: &SkinDrawState,
) -> SkinRenderItem {
    let Some(offset) = state.skin_offsets.get(OFFSET_ALL) else {
        return item;
    };
    if offset.x == 0 && offset.y == 0 && offset.w == 0 && offset.h == 0 {
        return item;
    }
    let scale_x = (offset.w + 100) as f32 / 100.0;
    let scale_y = (offset.h + 100) as f32 / 100.0;
    let translate_x = offset.x as f32 / 100.0;
    let translate_y = offset.y as f32 / 100.0;
    match item {
        SkinRenderItem::Image {
            texture,
            rect,
            uv,
            tint,
            blend,
            scale,
            border,
            source_size,
            linear_filter,
        } => SkinRenderItem::Image {
            texture,
            rect: apply_all_offset_to_rect(rect, scale_x, scale_y, translate_x, translate_y),
            uv,
            tint,
            blend,
            scale,
            border,
            source_size,
            linear_filter,
        },
        SkinRenderItem::RotatedImage {
            texture,
            rect,
            uv,
            tint,
            blend,
            source_size,
            linear_filter,
            angle_deg,
            center,
            post_scale,
        } => SkinRenderItem::RotatedImage {
            texture,
            rect: apply_all_offset_to_rotated_rect(
                rect,
                center,
                scale_x,
                scale_y,
                translate_x,
                translate_y,
            ),
            uv,
            tint,
            blend,
            source_size,
            linear_filter,
            angle_deg,
            center,
            post_scale: Point { x: post_scale.x * scale_x, y: post_scale.y * scale_y },
        },
        SkinRenderItem::Text { origin, text, style, caret, blend, post_scale } => {
            SkinRenderItem::Text {
                origin: apply_all_offset_to_point(
                    origin,
                    scale_x,
                    scale_y,
                    translate_x,
                    translate_y,
                ),
                text,
                style,
                caret,
                blend,
                post_scale: Point { x: post_scale.x * scale_x, y: post_scale.y * scale_y },
            }
        }
        SkinRenderItem::Rect { rect, color, blend } => SkinRenderItem::Rect {
            rect: apply_all_offset_to_rect(rect, scale_x, scale_y, translate_x, translate_y),
            color,
            blend,
        },
        SkinRenderItem::RectBatch { rects, cache } => SkinRenderItem::RectBatch {
            rects: rects
                .iter()
                .map(|command| RectCommand {
                    rect: apply_all_offset_to_rect(
                        command.rect,
                        scale_x,
                        scale_y,
                        translate_x,
                        translate_y,
                    ),
                    color: command.color,
                })
                .collect::<Vec<_>>()
                .into(),
            cache: cache.map(|cache| RectBatchCache {
                bounds: apply_all_offset_to_rect(
                    cache.bounds,
                    scale_x,
                    scale_y,
                    translate_x,
                    translate_y,
                ),
                ..cache
            }),
        },
    }
}

pub(super) fn apply_all_offset_to_rotated_rect(
    rect: Rect,
    center: Point,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Rect {
    let pivot = Point { x: rect.x + center.x * rect.width, y: rect.y + center.y * rect.height };
    let transformed_pivot =
        apply_all_offset_to_point(pivot, scale_x, scale_y, translate_x, translate_y);
    Rect {
        x: transformed_pivot.x - center.x * rect.width,
        y: transformed_pivot.y - center.y * rect.height,
        width: rect.width,
        height: rect.height,
    }
}

pub(super) fn apply_all_offset_to_rect(
    rect: Rect,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Rect {
    Rect {
        x: rect.x * scale_x + translate_x,
        // beatoraja の SpriteBatch transform は左下原点。BMZ の左上原点へ
        // 変換すると、縦拡縮には原点差の `1 - scale_y` が加わる。
        y: rect.y * scale_y + 1.0 - scale_y - translate_y,
        width: rect.width * scale_x,
        height: rect.height * scale_y,
    }
}

pub(super) fn apply_all_offset_to_point(
    point: Point,
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
) -> Point {
    Point { x: point.x * scale_x + translate_x, y: point.y * scale_y + 1.0 - scale_y - translate_y }
}

pub(super) fn result_judge_pie_segment_color(
    destination: &SkinDestinationDef,
    image: &SkinImageDef,
    frame: ResolvedSkinFrame,
    state: &SkinDrawState,
) -> Option<(i32, i32, i32)> {
    if state.result_failed.is_none()
        || destination.id != "judge_graph"
        || image.id != "judge_graph"
        || image.w != 140
        || image.h != 8
        || frame.w != 140
        || frame.h != 8
        || frame.angle == 0
    {
        return None;
    }

    let counts = state.judge_counts;
    let total = counts.pgreat + counts.great + counts.good + counts.bad + counts.poor;
    if total == 0 {
        return None;
    }
    let total = total as f32;
    let sweep = (frame.angle - 90).clamp(0, 360) as f32;
    let poor = 360.0 * counts.poor as f32 / total;
    let bad = 360.0 * (counts.poor + counts.bad) as f32 / total;
    let good = 360.0 * (counts.poor + counts.bad + counts.good) as f32 / total;
    let great = 360.0 * (counts.poor + counts.bad + counts.good + counts.great) as f32 / total;

    Some(if sweep < poor {
        (217, 68, 35)
    } else if sweep < bad {
        (226, 135, 42)
    } else if sweep <= good {
        (240, 190, 15)
    } else if sweep <= great {
        (240, 239, 10)
    } else {
        (8, 179, 239)
    })
}

pub(super) fn skin_image_item_for_frame(
    texture: SkinTextureId,
    rect: Rect,
    mut uv: TextureRegion,
    frame: ResolvedSkinFrame,
    center: i32,
    blend: BlendMode,
    source_size: Option<SkinImageSize>,
    linear_filter: bool,
) -> SkinRenderItem {
    // beatoraja forwards signed destination sizes to SpriteBatch. BMZ keeps
    // rectangles normalized for hit testing and clipping, so preserve the sign
    // as a reversed UV extent instead. This reproduces `w = -101`/`h < 0`
    // image mirroring without emitting a negative on-screen rectangle.
    if frame.w < 0 {
        uv.x += uv.width;
        uv.width = -uv.width;
    }
    if frame.h < 0 {
        uv.y += uv.height;
        uv.height = -uv.height;
    }
    let tint = Color::rgba(
        frame.r as f32 / 255.0,
        frame.g as f32 / 255.0,
        frame.b as f32 / 255.0,
        frame.a as f32 / 255.0,
    );
    if frame.angle == 0 {
        return SkinRenderItem::Image {
            texture,
            rect,
            uv,
            tint,
            blend,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size,
            linear_filter,
        };
    }
    SkinRenderItem::RotatedImage {
        texture,
        rect,
        uv,
        tint,
        blend,
        source_size,
        linear_filter,
        // beatoraja/LibGDX rotates in bottom-origin coordinates, where a
        // positive angle is counter-clockwise. BMZ renders in top-origin
        // coordinates, so negate the destination angle to preserve the same
        // on-screen direction.
        angle_deg: -frame.angle as f32,
        center: skin_rotation_center(center),
        post_scale: Point { x: 1.0, y: 1.0 },
    }
}

pub(super) fn skin_rotation_center(center: i32) -> Point {
    const CENTER_X: [f32; 10] = [0.5, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0];
    const CENTER_Y_BOTTOM_ORIGIN: [f32; 10] = [0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0];
    let index = usize::try_from(center).ok().filter(|index| *index < CENTER_X.len()).unwrap_or(0);
    Point { x: CENTER_X[index], y: 1.0 - CENTER_Y_BOTTOM_ORIGIN[index] }
}

pub(super) fn resolve_destination_frame(
    destination: &SkinDestinationDef,
    elapsed_ms: i32,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    if let [SkinDstEntry::Frame(animation)] = destination.dst.as_slice() {
        return resolve_single_destination_frame(destination, *animation, elapsed_ms, state);
    }
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    // `cycle` はアニメーション終端（最後のキーフレーム時刻）。
    let cycle = animations.iter().filter_map(|a| a.time).max().unwrap_or(0);
    let loop_point = destination.loop_time.unwrap_or(0);
    let elapsed_ms = match loop_point {
        // loop:負値 → ループせず、終端を過ぎたら描画しない（READY やボム等の単発演出）。
        loop_point if loop_point < 0 => {
            if elapsed_ms > cycle {
                return None;
            }
            elapsed_ms
        }
        // loop未指定または0以上 → 終端到達後 loop_point 時刻へループバック。
        loop_point => resolve_loop_elapsed(loop_point, elapsed_ms, cycle),
    };
    let acc = destination_interpolation_acc_from_frames(&animations);
    let fixed_color = destination_frames_have_fixed_color(&animations);
    let mut frame = ResolvedSkinFrame::default();
    let mut previous = None;
    for animation in &animations {
        apply_skin_animation(&mut frame, animation, state);
        if frame.time <= elapsed_ms {
            previous = Some(frame);
            continue;
        }
        // previous=None は最初のキーフレーム時刻より前 → destination はまだ表示開始
        // していない。beatoraja 同様、開始時刻前のオブジェクトは描画しない。
        return previous.map(|previous| {
            let mut interpolated = interpolate_skin_frame(previous, frame, elapsed_ms, acc);
            interpolated.apply_offset_alpha = fixed_color || elapsed_ms == previous.time;
            interpolated
        });
    }
    previous.or_else(|| animations.first().map(|_| frame))
}

pub(super) fn resolve_single_destination_frame(
    destination: &SkinDestinationDef,
    animation: SkinAnimationDef,
    elapsed_ms: i32,
    state: &SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    let cycle = animation.time.unwrap_or(0);
    let loop_point = destination.loop_time.unwrap_or(0);
    let elapsed_ms = match loop_point {
        loop_point if loop_point < 0 => {
            if elapsed_ms > cycle {
                return None;
            }
            elapsed_ms
        }
        loop_point => resolve_loop_elapsed(loop_point, elapsed_ms, cycle),
    };
    let mut frame = ResolvedSkinFrame::default();
    apply_skin_animation(&mut frame, &animation, state);
    (frame.time <= elapsed_ms).then_some(frame)
}

/// Returns the fully inherited terminal frame without applying destination
/// loop-back. Editable text actors in beatoraja use the skin destination as an
/// input anchor, while the entered text itself is drawn by a separate
/// `TextField`. The input overlay therefore needs the settled destination
/// geometry rather than the repeating animation time.
pub(super) fn resolve_destination_terminal_frame(
    destination: &SkinDestinationDef,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    let mut frame = ResolvedSkinFrame::default();
    let mut resolved = false;
    for animation in animations {
        apply_skin_animation(&mut frame, &animation, state);
        resolved = true;
    }
    resolved.then_some(frame)
}

pub(super) fn resolve_destination_frame_until_end(
    destination: &SkinDestinationDef,
    elapsed_ms: i32,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> Option<ResolvedSkinFrame> {
    if matches!(destination.loop_time, Some(loop_point) if loop_point > 0) {
        return resolve_destination_frame(destination, elapsed_ms, enabled_options, state);
    }
    let animations = flatten_dst_entries(&destination.dst, enabled_options);
    let last_time = animations.iter().filter_map(|a| a.time).max()?;
    if elapsed_ms > last_time {
        return None;
    }
    resolve_destination_frame(destination, elapsed_ms, enabled_options, state)
}

/// beatoraja の `loop` セマンティクスでアニメーション内の経過時刻を求める。
///
/// `loop` フィールドはループ「周期」ではなく、終端到達後に戻る「ループバック地点」。
/// - `loop_point >= 0` かつ `elapsed >= cycle`: `[loop_point, cycle)` 区間を繰り返す。
///   `loop_point >= cycle`（`loop == 終端` を含む）の場合は終端で停止し、
///   アニメーションは1回再生して最終フレームを保持する。
/// - `loop_point < 0`: ループしない（終端後の非表示は呼び出し側で判定）。
pub(super) fn resolve_loop_elapsed(loop_point: i32, elapsed_ms: i32, cycle: i32) -> i32 {
    if loop_point >= 0 && elapsed_ms >= cycle {
        let span = cycle - loop_point;
        if span > 0 { (elapsed_ms - loop_point).rem_euclid(span) + loop_point } else { cycle }
    } else {
        elapsed_ms
    }
}

pub(super) fn interpolate_skin_frame(
    start: ResolvedSkinFrame,
    end: ResolvedSkinFrame,
    elapsed_ms: i32,
    acc: i32,
) -> ResolvedSkinFrame {
    let duration = end.time - start.time;
    if duration <= 0 {
        return end;
    }
    let t = eased_skin_frame_rate(
        ((elapsed_ms - start.time) as f32 / duration as f32).clamp(0.0, 1.0),
        acc,
    );
    ResolvedSkinFrame {
        time: elapsed_ms,
        x: interpolate_i32(start.x, end.x, t),
        y: interpolate_i32(start.y, end.y, t),
        w: interpolate_i32(start.w, end.w, t),
        h: interpolate_i32(start.h, end.h, t),
        acc: end.acc,
        a: interpolate_i32(start.a, end.a, t),
        r: interpolate_i32(start.r, end.r, t),
        g: interpolate_i32(start.g, end.g, t),
        b: interpolate_i32(start.b, end.b, t),
        angle: interpolate_i32(start.angle, end.angle, t),
        apply_offset_alpha: start.apply_offset_alpha,
    }
}

pub(super) fn destination_frames_have_fixed_color(animations: &[SkinAnimationDef]) -> bool {
    let defaults = ResolvedSkinFrame::default();
    let mut current = (defaults.r, defaults.g, defaults.b, defaults.a);
    let mut previous = None;
    for animation in animations {
        current = (
            animation.r.unwrap_or(current.0),
            animation.g.unwrap_or(current.1),
            animation.b.unwrap_or(current.2),
            animation.a.unwrap_or(current.3),
        );
        if previous.is_some_and(|color| color != current) {
            return false;
        }
        previous = Some(current);
    }
    true
}

pub(super) fn destination_interpolation_acc_from_frames(animations: &[SkinAnimationDef]) -> i32 {
    // Only acc is relevant here; resolving geometry constructed a full default
    // draw state per keyframe and also evaluated unrelated height expressions.
    animations.iter().filter_map(|animation| animation.acc).find(|&acc| acc != 0).unwrap_or(0)
}

pub(super) fn eased_skin_frame_rate(t: f32, acc: i32) -> f32 {
    match acc {
        1 => t * t,
        2 => 1.0 - (t - 1.0) * (t - 1.0),
        3 => 0.0,
        _ => t,
    }
}

pub(super) fn interpolate_i32(start: i32, end: i32, t: f32) -> i32 {
    (start as f32 + (end - start) as f32 * t).round() as i32
}

pub(super) fn apply_skin_animation(
    frame: &mut ResolvedSkinFrame,
    animation: &SkinAnimationDef,
    state: &SkinDrawState,
) {
    if let Some(time) = animation.time {
        frame.time = time;
    }
    if let Some(x) = animation.x {
        frame.x = x;
    }
    if let Some(y) = animation.y {
        frame.y = y;
    }
    if let Some(w) = animation.w {
        frame.w = w;
    }
    if let Some(h) = animation.h {
        frame.h = h;
    }
    if let Some(expr) = animation.h_expr
        && let Some(h) = skin_frame_expr_value(expr, state)
    {
        frame.h = h;
    }
    if let Some(acc) = animation.acc {
        frame.acc = acc;
    }
    if let Some(a) = animation.a {
        frame.a = a;
    }
    if let Some(r) = animation.r {
        frame.r = r;
    }
    if let Some(g) = animation.g {
        frame.g = g;
    }
    if let Some(b) = animation.b {
        frame.b = b;
    }
    if let Some(angle) = animation.angle {
        frame.angle = angle;
    }
}

pub(super) fn destination_uses_skin_offset(
    destination: &SkinDestinationDef,
    offset_id: i32,
) -> bool {
    destination.offset == offset_id || destination.offsets.contains(&offset_id)
}

pub(super) fn destination_uses_lift_offset_only(destination: &SkinDestinationDef) -> bool {
    destination_uses_skin_offset(destination, 3)
        && !destination_uses_skin_offset(destination, 4)
        && !destination_uses_skin_offset(destination, 5)
}
