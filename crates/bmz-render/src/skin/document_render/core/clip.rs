macro_rules! skin_document_render_core_clip_methods {
    () => {
    fn result_judge_pie_destination_item(
        &self,
        destination: &SkinDestinationDef,
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        sources: &HashMap<String, SkinDocumentTexture>,
    ) -> Option<SkinRenderItem> {
        if state.result_failed.is_none() || destination.id != "judge_graph" {
            return None;
        }
        let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
        let mut frame = resolve_destination_frame(destination, elapsed, enabled_options, state)?;
        let image = skin_image_for_destination_id(destination.id.as_str(), images)?;
        let is_hidden_cover_destination = self
            .hidden_cover
            .iter()
            .any(|cover| cover.id == destination.id && !is_lift_lane_cover_id(&cover.id));
        apply_skin_offset_to_frame(destination, &mut frame, state, is_hidden_cover_destination);
        if !destination_mouse_rect_contains(destination, frame, state) {
            return None;
        }
        let (r, g, b) = result_judge_pie_segment_color(destination, image, frame, state)?;
        frame.r = r;
        frame.g = g;
        frame.b = b;
        let source = resolve_document_source(sources, &image.src)?;
        let pixel_rect = skin_image_pixel_rect(image);
        let uv = skin_image_texture_region_for_state(
            image,
            source.source_size,
            state,
            pixel_rect,
        );
        let (rect, uv) = stretch_skin_image_geometry(
            destination.stretch,
            normalize_skin_frame_rect(frame, self.w, self.h),
            uv,
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

    fn destination_looks_like_pre_notes_judge_line(
        &self,
        destination: &SkinDestinationDef,
        images: &SkinImageLookup<'_>,
        enabled_options: &[i32],
        state: &SkinDrawState,
        next_destination: Option<&SkinDestinationDef>,
    ) -> bool {
        if !matches!(next_destination, Some(next) if next.id == "notes")
            || destination.timer.is_some()
            || !destination_uses_lift_offset_only(destination)
            || skin_image_for_destination_id(destination.id.as_str(), images).is_none()
        {
            return false;
        }
        let Some(frame) = resolve_destination_frame(destination, 0, enabled_options, state) else {
            return false;
        };
        if frame.w < 100 || frame.h <= 0 || frame.h > 48 {
            return false;
        }
        let Some(note) = &self.note else {
            return false;
        };
        flatten_dst_entries(&note.dst, enabled_options).into_iter().any(|note_frame| {
            let Some(note_y) = note_frame.y else {
                return false;
            };
            frame.y >= note_y && frame.y <= note_y.saturating_add(64)
        })
    }

    /// `hiddenCover.disapearLine` をレーンカバー系 (HIDDEN / SUDDEN+ / LIFT) のクロップ境界として使う。
    fn disappear_line_for_lane_cover_clip(&self) -> Option<(i32, bool)> {
        let cover = self.hidden_cover.first()?;
        (cover.disappear_line > 0)
            .then_some((cover.disappear_line, cover.is_disappear_line_link_lift))
    }

    fn should_clip_image_at_disappear_line(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
    ) -> bool {
        if self.hidden_cover.is_empty() {
            return false;
        }
        if is_lift_lane_cover_id(&destination.id) || is_lift_lane_cover_id(&image.id) {
            return true;
        }
        destination_uses_lift_offset_only(destination)
            && self.hidden_cover.iter().any(|cover| cover.src == image.src)
    }

    /// `liftcover` 系 ID のみ。`offset: 3` だけの destination (判定線・数値表示など) は対象外。
    fn should_skip_lift_lane_cover_render(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
    ) -> bool {
        is_lift_lane_cover_id(&destination.id) || is_lift_lane_cover_id(&image.id)
    }

    /// LIFT 用 image は `offset: 3` で既にリフト分だけ動くため、`hiddenCover` の
    /// `isDisappearLineLinkLift` は二重適用しない。
    fn link_lift_for_lane_cover_clip(
        &self,
        destination: &SkinDestinationDef,
        image: &SkinImageDef,
        link_lift: bool,
    ) -> bool {
        if is_lift_lane_cover_id(&destination.id)
            || is_lift_lane_cover_id(&image.id)
            || destination_uses_lift_offset_only(destination)
        {
            return false;
        }
        link_lift
    }
    };
}

pub(in crate::skin::document_render) use skin_document_render_core_clip_methods;
