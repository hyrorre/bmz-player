macro_rules! skin_document_render_select_bar_methods {
    () => {
        fn select_bar_item(
            &self,
            row: &SelectRowSnapshot,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let imageset = self.imageset.iter().find(|set| set.id == destination.id)?;
            let image_index = select_row_bar_image_index(row);
            let image_id = select_row_slot_with_fallbacks(
                &imageset.images,
                image_index,
                select_row_bar_image_fallback_indices(row),
            )?;
            let image = self.image.iter().find(|image| image.id == *image_id)?;
            let source = resolve_document_source(sources, &image.src)?;
            // Bar sprites have always used the default state's timer (zero,
            // or an inactive timer falling back to zero), not the row timer.
            let elapsed = 0;
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                skin_image_texture_region(image, source.source_size, elapsed),
                source.source_size,
                self.w,
                self.h,
            );
            Some(skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                skin_blend_mode(destination.blend),
                Some(source.source_size),
                destination.filter != 0,
            ))
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_select_bar_methods;
