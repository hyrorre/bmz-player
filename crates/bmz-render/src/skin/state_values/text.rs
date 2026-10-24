use super::*;

pub(super) fn skin_text_align(align: i32) -> TextAlign {
    match align {
        1 => TextAlign::Center,
        2 => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

pub(super) fn skin_text_bitmap_size(
    text: &SkinTextDef,
    fonts: &[SkinFontDef],
    skin_height: u32,
    frame_h: i32,
) -> Option<f32> {
    if text.font.is_empty() {
        return None;
    }
    let font_id = text.font.rsplit_once(':').map_or(text.font.as_str(), |(_, id)| id);
    let font = fonts.iter().find(|font| font.id == text.font || font.id == font_id)?;
    let extension = Path::new(&font.path).extension()?.to_str()?;
    if !extension.eq_ignore_ascii_case("fnt") && !extension.eq_ignore_ascii_case("lr2font") {
        return None;
    }
    let bitmap_size = if text.size > 0 { text.size } else { frame_h.abs().max(1) };
    Some(bitmap_size as f32 / skin_height.max(1) as f32)
}

pub(super) fn skin_text_overflow(overflow: i32, shrink_mode: i32) -> TextOverflow {
    match overflow {
        1 if shrink_mode == 1 => TextOverflow::ShrinkUniform,
        1 => TextOverflow::Shrink,
        2 => TextOverflow::Truncate,
        _ => TextOverflow::Overflow,
    }
}

pub(super) fn skin_text_shadow(
    text: &SkinTextDef,
    skin_width: u32,
    skin_height: u32,
) -> Option<TextShadow> {
    let color = skin_hex_color(&text.shadow_color)?;
    if color.a <= 0.0 {
        return None;
    }
    Some(TextShadow {
        color,
        offset: Point {
            x: text.shadow_offset_x / skin_width.max(1) as f32,
            y: text.shadow_offset_y / skin_height.max(1) as f32,
        },
    })
}

pub(super) fn skin_text_outline(text: &SkinTextDef, skin_height: u32) -> Option<TextOutline> {
    if text.outline_width <= 0.0 {
        return None;
    }
    let color = skin_hex_color(&text.outline_color)?;
    if color.a <= 0.0 {
        return None;
    }
    Some(TextOutline { color, width: text.outline_width / skin_height.max(1) as f32 })
}

/// `colorTop` / `colorBottom` のグラデーション。片方だけなら、もう片方は白（文字色のまま）。
/// どちらも指定が無いか読めなければ `None`。色は文字色 `color` に掛ける。
pub(super) fn skin_text_gradient(
    text: &SkinTextDef,
    color: Color,
) -> Option<(Color, TextGradient)> {
    let top = skin_hex_color(&text.color_top);
    let bottom = skin_hex_color(&text.color_bottom);
    if top.is_none() && bottom.is_none() {
        return None;
    }
    let tint = |value: Option<Color>| {
        let value = value.unwrap_or(Color::rgb(1.0, 1.0, 1.0));
        Color::rgba(value.r * color.r, value.g * color.g, value.b * color.b, value.a * color.a)
    };
    let angle_deg = if text.gradient_angle.is_finite() { text.gradient_angle } else { 0.0 };
    Some((tint(top), TextGradient { color: tint(bottom), angle_deg }))
}

pub(super) fn skin_hex_color(value: &str) -> Option<Color> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    let a =
        if hex.len() == 8 { u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0 } else { 1.0 };
    Some(Color::rgba(r, g, b, a))
}

pub(super) fn skin_panel_render_items(
    panel: &SkinPanelDef,
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<SkinRenderItem> {
    let rect = normalize_skin_frame_rect(frame, canvas_width, canvas_height);
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Vec::new();
    }

    let blend = skin_blend_mode(destination.blend);
    let tint = |value: &str| {
        let color = skin_hex_color(value)?;
        Some(Color::rgba(
            color.r * frame.r.clamp(0, 255) as f32 / 255.0,
            color.g * frame.g.clamp(0, 255) as f32 / 255.0,
            color.b * frame.b.clamp(0, 255) as f32 / 255.0,
            color.a * frame.a.clamp(0, 255) as f32 / 255.0,
        ))
    };

    let mut items = Vec::with_capacity(5);
    if let Some(color) = tint(&panel.color)
        && color.a > 0.0
    {
        items.push(SkinRenderItem::Rect { rect, color, blend });
    }

    let Some(border_color) = tint(&panel.border_color).filter(|color| color.a > 0.0) else {
        return items;
    };
    if panel.border_width <= 0.0 {
        return items;
    }
    let border_x = (panel.border_width / canvas_width.max(1) as f32).min(rect.width * 0.5);
    let border_y = (panel.border_width / canvas_height.max(1) as f32).min(rect.height * 0.5);
    if border_x <= 0.0 || border_y <= 0.0 {
        return items;
    }
    items.extend([
        SkinRenderItem::Rect {
            rect: Rect { height: border_y, ..rect },
            color: border_color,
            blend,
        },
        SkinRenderItem::Rect {
            rect: Rect { y: rect.y + rect.height - border_y, height: border_y, ..rect },
            color: border_color,
            blend,
        },
        SkinRenderItem::Rect {
            rect: Rect {
                y: rect.y + border_y,
                width: border_x,
                height: (rect.height - border_y * 2.0).max(0.0),
                ..rect
            },
            color: border_color,
            blend,
        },
        SkinRenderItem::Rect {
            rect: Rect {
                x: rect.x + rect.width - border_x,
                y: rect.y + border_y,
                width: border_x,
                height: (rect.height - border_y * 2.0).max(0.0),
            },
            color: border_color,
            blend,
        },
    ]);
    items
}
