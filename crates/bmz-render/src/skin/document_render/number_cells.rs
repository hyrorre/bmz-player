use super::*;

pub(super) struct Layout {
    pub max_digits: usize,
    pub align: i32,
    pub center: i32,
    pub blend: BlendMode,
    pub filter: bool,
    pub hidden_cell: Option<u32>,
}

/// Geometry/UV path shared with ordinary value numbers. Cells are already
/// decoded; no numeric ref is involved in songlist fields.
pub(super) fn render(
    document: &SkinDocument,
    value: &SkinValueDef,
    digits: &[impl Copy + Into<u32>],
    layout: Layout,
    base_frame: ResolvedSkinFrame,
    frame: ResolvedSkinFrame,
    elapsed_ms: i32,
    source: &SkinDocumentTexture,
) -> Vec<SkinRenderItem> {
    let Layout { max_digits, align, center, blend, filter, hidden_cell } = layout;
    let divx = value.divx.max(1);
    let divy = value.divy.max(1);
    let width = if value.w == -1 { source.source_size.width.round() as i32 } else { value.w };
    let height = if value.h == -1 { source.source_size.height.round() as i32 } else { value.h };
    let cell_width_px = (width / divx) as f32;
    let cell_height_px = (height / divy) as f32;
    if cell_width_px <= 0.0 || cell_height_px <= 0.0 {
        return Vec::new();
    }
    // 桁間スペース (space フィールド、px 単位)
    let digit_step = frame.w + value.space;
    // 先頭の空き桁数 (align のためのオフセット計算に使用)
    let shiftbase = max_digits.saturating_sub(digits.len());
    // align=0: 右寄せ (デフォルト), align=1: 左寄せ, align=2: 中央
    let shift = match align {
        1 => digit_step * shiftbase as i32,
        2 => digit_step * shiftbase as i32 / 2,
        _ => 0,
    };
    let lr2_center_shift = if value.lr2_number && align == 2 {
        digit_step
            * digits
                .iter()
                .copied()
                .map(Into::into)
                .filter(|digit| matches!(*digit, 10 | 22 | 255))
                .count() as i32
            / 2
    } else {
        0
    };

    digits
        .iter()
        .copied()
        .map(Into::into)
        .enumerate()
        .filter(|(_, digit)| Some(*digit) != hidden_cell)
        .map(|(index, digit)| {
            let digit_position = (shiftbase + index) as i32;
            let enlarge = value.qwilight_animation.as_ref().map_or(0, |a| {
                (f64::from(a.enlarge)
                    * (1.0
                        - f64::from(elapsed_ms.max(0)) * f64::from(a.fps)
                            / (a.count.max(1) as f64 * 1000.0))
                        .clamp(0.0, 1.0))
                .round() as i32
            });
            let rect = normalize_skin_frame_rect(
                ResolvedSkinFrame {
                    x: base_frame.x + frame.x + digit_step * digit_position - shift
                        + lr2_center_shift
                        - enlarge,
                    y: base_frame.y + frame.y - enlarge,
                    w: frame.w + 2 * enlarge,
                    h: frame.h + 2 * enlarge,
                    ..frame
                },
                document.w,
                document.h,
            );
            let uv = SkinDocument::value_digit_texture_region(
                value,
                digit,
                elapsed_ms,
                source.source_size,
                cell_width_px,
                cell_height_px,
                divx,
                divy,
            );
            skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                center,
                blend,
                Some(source.source_size),
                filter,
            )
        })
        .collect()
}
