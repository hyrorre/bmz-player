macro_rules! skin_document_render_play_value_methods {
    () => {
        fn value_digit_texture_region(
            value: &SkinValueDef,
            digit: u32,
            elapsed_ms: i32,
            source_size: SkinImageSize,
            cell_width_px: f32,
            cell_height_px: f32,
            divx: i32,
            divy: i32,
        ) -> TextureRegion {
            let source_width = source_size.width.max(1.0);
            let source_height = source_size.height.max(1.0);
            if value.lr2_number {
                let stride = lr2_number_frame_cells(value).unwrap_or(1);
                let frames = divx.saturating_mul(divy) / stride;
                let animation = if value.cycle > 0 {
                    (i64::from(elapsed_ms.rem_euclid(value.cycle)) * i64::from(frames)
                        / i64::from(value.cycle)) as i32
                } else {
                    0
                };
                let cell = animation * stride + digit as i32;
                return TextureRegion {
                    x: (value.x as f32 + cell_width_px * (cell % divx) as f32) / source_width,
                    y: (value.y as f32 + cell_height_px * (cell / divx) as f32) / source_height,
                    width: cell_width_px / source_width,
                    height: cell_height_px / source_height,
                };
            }
            let digit_column = digit as i32 % divx;
            let digit_row = digit as i32 / divx;
            let animation_rows = divy.saturating_sub(digit_row).max(1);
            let animation_row = if value.cycle > 0 && animation_rows > 1 {
                (elapsed_ms.rem_euclid(value.cycle) * animation_rows / value.cycle)
                    .min(animation_rows - 1)
            } else {
                0
            };
            let source_row = (digit_row + animation_row).min(divy - 1);
            TextureRegion {
                x: (value.x as f32 + cell_width_px * digit_column as f32) / source_width,
                y: (value.y as f32 + cell_height_px * source_row as f32) / source_height,
                width: cell_width_px / source_width,
                height: cell_height_px / source_height,
            }
        }

        fn gauge_image_render_item(
            &self,
            image_id: &str,
            rect: Rect,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
            tint: Color,
            blend: BlendMode,
            linear_filter: bool,
        ) -> Option<SkinRenderItem> {
            let image = self.image.iter().find(|image| image.id == image_id)?;
            let source = resolve_document_source(sources, &image.src)?;
            let uv = skin_image_texture_region(image, source.source_size, elapsed_ms);
            let (rect, uv) =
                stretch_skin_image_geometry(0, rect, uv, source.source_size, self.w, self.h);
            Some(SkinRenderItem::Image {
                texture: source.texture,
                rect,
                uv,
                tint,
                blend,
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: Some(source.source_size),
                linear_filter,
            })
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_play_value_methods;
