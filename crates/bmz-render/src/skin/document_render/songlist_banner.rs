use super::*;

#[cfg(test)]
mod tests;

pub(super) fn render(
    document: &SkinDocument,
    songlist: &SkinSongListDef,
    row: &SelectRowSnapshot,
    origin: (i32, i32),
    options: &[i32],
    state: &SkinDrawState,
) -> Vec<SkinRenderItem> {
    let mut items = Vec::new();
    if row.kind != SelectRowKind::Song || row.is_folder || !row.in_library {
        return items;
    }
    let Some(banner) = &row.banner else {
        return items;
    };
    for dst in &songlist.banner {
        if !destination_ops_match(dst, options, state)
            || !eval_skin_draw_condition(&dst.draw, state)
        {
            continue;
        }
        let Some(elapsed) = destination_timer_elapsed_ms(dst, state) else {
            continue;
        };
        let Some(mut frame) = resolve_destination_frame(dst, elapsed, options, state) else {
            continue;
        };
        frame.x += origin.0;
        frame.y += origin.1;
        apply_skin_offset_to_frame(dst, &mut frame, state, false);
        let clip = frame.take_clip(document.w, document.h);
        let (rect, uv) = stretch_skin_image_geometry(
            dst.stretch,
            normalize_skin_frame_rect(frame, document.w, document.h),
            TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            banner.size,
            document.w,
            document.h,
        );
        items.extend(wrap_skin_destination_clip(
            vec![skin_image_item_for_frame(
                banner.texture,
                rect,
                uv,
                frame,
                dst.center,
                skin_blend_mode(dst.blend),
                Some(banner.size),
                dst.filter != 0,
            )],
            clip,
        ));
    }
    items
}
