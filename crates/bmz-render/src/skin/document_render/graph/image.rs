macro_rules! skin_document_render_graph_image_methods {
    () => {
        fn direct_source_image_render_item(
            &self,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let source_id = beatoraja_direct_image_source_id(&destination.id)?;
            let source = resolve_document_source(sources, &source_id)?;
            let uv = TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
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

        fn slider_render_item(
            &self,
            slider: &SkinSliderDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let progress = skin_slider_progress(slider, state)?;
            let source = sources.get(&slider.src)?;
            let source_width = source.source_size.width.max(1.0);
            let source_height = source.source_size.height.max(1.0);
            let (source_x, source_y, source_rect_width, source_rect_height) =
                resolve_skin_image_pixel_rect(
                    (slider.x, slider.y, slider.w, slider.h),
                    source_width,
                    source_height,
                );
            if source_rect_width <= 0 || source_rect_height <= 0 {
                return None;
            }

            // beatoraja JsonSkinObjectLoader splits SkinSlider's source rectangle into
            // divx*divy TextureRegions before SkinSourceImage applies timer/cycle.
            let divx = slider.divx.max(1).min(source_rect_width);
            let divy = slider.divy.max(1).min(source_rect_height);
            let frame_count = i64::from(divx) * i64::from(divy);
            let source_elapsed_ms = skin_timer_elapsed_ms(slider.timer, state).unwrap_or(0).max(0);
            let source_frame = skin_source_image_animation_frame_index(
                frame_count,
                slider.cycle,
                source_elapsed_ms,
            );
            let source_column = (source_frame % i64::from(divx)) as i32;
            let source_row = (source_frame / i64::from(divx)) as i32;
            let cell_width = source_rect_width / divx;
            let cell_height = source_rect_height / divy;
            let mut uv = skin_image_texture_region_for_pixel_bounds(
                (source_x + source_column * cell_width) as f32,
                (source_y + source_row * cell_height) as f32,
                cell_width as f32,
                cell_height as f32,
                source_width,
                source_height,
            );

            let mut frame = frame;
            let offset = (slider.range as f32 * progress).round() as i32;
            match slider.angle {
                0 => frame.y += offset,
                1 => frame.x += offset,
                2 => frame.y -= offset,
                3 => frame.x -= offset,
                _ => {}
            }
            if slider.slider_type == 4
                && let Some((disappear_line, link_lift)) = self.disappear_line_for_lane_cover_clip()
            {
                clip_skin_cover_to_disappear_line(
                    &mut frame,
                    &mut uv,
                    disappear_line,
                    link_lift,
                    state,
                );
                if frame.h <= 0 {
                    return None;
                }
            }
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                uv,
                source.source_size,
                self.w,
                self.h,
            );
            Some(SkinRenderItem::Image {
                texture: source.texture,
                rect,
                uv,
                tint: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
                ),
                blend: frame.blend(skin_blend_mode(destination.blend)),
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: Some(source.source_size),
                linear_filter: frame.linear_filter(destination.filter != 0),
            })
        }

        fn hidden_cover_render_item(
            &self,
            cover: &SkinHiddenCoverDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let source = sources.get(&cover.src)?;
            let source_width = source.source_size.width.max(1.0);
            let source_height = source.source_size.height.max(1.0);
            let mut frame = frame;
            let mut uv = TextureRegion {
                x: cover.x as f32 / source_width,
                y: cover.y as f32 / source_height,
                width: cover.w as f32 / source_width,
                height: cover.h as f32 / source_height,
            };
            clip_skin_cover_to_disappear_line(
                &mut frame,
                &mut uv,
                cover.disappear_line,
                cover.is_disappear_line_link_lift,
                state,
            );
            if frame.h <= 0 {
                return None;
            }
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                uv,
                source.source_size,
                self.w,
                self.h,
            );
            Some(SkinRenderItem::Image {
                texture: source.texture,
                rect,
                uv,
                tint: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
                ),
                blend: frame.blend(skin_blend_mode(destination.blend)),
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: Some(source.source_size),
                linear_filter: frame.linear_filter(destination.filter != 0),
            })
        }

        fn graph_render_item(
            &self,
            graph: &SkinGraphDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let source = sources.get(&graph.src)?;
            let (fill_multiplier, uv_ratio) = graph_fill_dimensions(graph, state);
            let fill_from_right = frame.w < 0;
            let source_w = source.source_size.width.max(1.0);
            let source_h = source.source_size.height.max(1.0);
            let base_uv = TextureRegion {
                x: graph.x as f32 / source_w,
                y: graph.y as f32 / source_h,
                width: graph.w as f32 / source_w,
                height: graph.h as f32 / source_h,
            };
            let dst = normalize_skin_frame_rect(frame, self.w, self.h);
            let (rect, uv) = if graph.angle == 1 {
                // vertical: fill from bottom up
                let clipped_h = dst.height * fill_multiplier;
                let uv_offset = base_uv.height * (1.0 - uv_ratio);
                (
                    Rect { y: dst.y + dst.height - clipped_h, height: clipped_h, ..dst },
                    TextureRegion {
                        y: base_uv.y + uv_offset,
                        height: base_uv.height * uv_ratio,
                        ..base_uv
                    },
                )
            } else {
                // horizontal: positive destinations fill from left. beatoraja keeps a
                // negative destination width and therefore fills leftwards from the
                // destination x; after rect normalization that is the right edge.
                let clipped_w = dst.width * fill_multiplier;
                (
                    Rect {
                        x: if fill_from_right { dst.x + dst.width - clipped_w } else { dst.x },
                        width: clipped_w,
                        ..dst
                    },
                    TextureRegion { width: base_uv.width * uv_ratio, ..base_uv },
                )
            };
            if rect.width <= 0.0 || rect.height <= 0.0 {
                return None;
            }
            Some(SkinRenderItem::Image {
                texture: source.texture,
                rect,
                uv,
                tint: Color::rgba(
                    frame.r as f32 / 255.0,
                    frame.g as f32 / 255.0,
                    frame.b as f32 / 255.0,
                    frame.a as f32 / 255.0,
                ),
                blend: frame.blend(BlendMode::Normal),
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: Some(source.source_size),
                linear_filter: frame.linear_filter(false),
            })
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_graph_image_methods;
