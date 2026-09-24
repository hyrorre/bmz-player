macro_rules! skin_document_render_core_resolve_methods {
    () => {
        fn resolve_destination_items(
            &self,
            destination_index: usize,
            destination: &SkinDestinationDef,
            context: DestinationResolveContext<'_, '_>,
        ) -> Option<Vec<SkinRenderItem>> {
            let DestinationResolveContext {
                images,
                values,
                enabled_options,
                state,
                text_state,
                sources,
                runtime_graphs,
                cache,
                number_cache,
            } = context;
            if let Some(judge_def) = self.judge.iter().find(|judge| judge.id == destination.id) {
                // SkinJudge 自身に destination が設定されている場合は、beatoraja の
                // `super.prepare` と同じく外側の timer/op/draw も先に評価する。
                // dst が空なら SkinJudge constructor の既定 destination が残る。
                if !destination.dst.is_empty() {
                    if !destination_ops_match(destination, enabled_options, state)
                        || !eval_skin_draw_condition(&destination.draw, state)
                    {
                        return None;
                    }
                    let outer_elapsed = destination_timer_elapsed_ms(destination, state)?;
                    resolve_destination_frame(destination, outer_elapsed, enabled_options, state)?;
                }
                let region = judge_def.index.clamp(0, MAX_JUDGE_REGIONS as i32 - 1) as usize;
                let elapsed = state.judge_ms[region]?;
                let judge_image_index = state.judge_index[region]?;
                return self.judge_render_items_for_def(
                    judge_def,
                    judge_image_index,
                    state.judge_combo[region],
                    elapsed,
                    sources,
                    state,
                );
            }

            let value_for_destination = values.get(destination.id.as_str());
            let elapsed = destination_timer_elapsed_ms(destination, state).or_else(|| {
                value_for_destination
                    .filter(|value| {
                        pre_ready_lane_cover_value_destination(destination, value, state)
                    })
                    .map(|_| 0)
            })?;
            let mut frame =
                resolve_destination_frame(destination, elapsed, enabled_options, state)?;
            let is_hidden_cover_destination = self
                .hidden_cover
                .iter()
                .any(|cover| cover.id == destination.id && !is_lift_lane_cover_id(&cover.id));
            let is_lift_cover_destination =
                self.lift_cover.iter().any(|cover| cover.id == destination.id);
            apply_skin_offset_to_frame(destination, &mut frame, state, is_hidden_cover_destination);
            if is_lift_cover_destination && !destination_uses_skin_offset(destination, 3) {
                apply_skin_offset_ids_to_frame(&[3], &mut frame, state, false);
            }
            if !destination_mouse_rect_contains(destination, frame, state) {
                return None;
            }
            if let Some(panel) = self.panel.iter().find(|panel| panel.id == destination.id) {
                return Some(skin_panel_render_items(panel, destination, frame, self.w, self.h));
            }
            if let Some(visualizer) =
                self.hiterror_visualizer.iter().find(|visualizer| visualizer.id == destination.id)
            {
                return Some(self.hiterror_visualizer_render_items(
                    visualizer,
                    destination,
                    frame,
                    state,
                ));
            }
            if let Some(visualizer) =
                self.timingvisualizer.iter().find(|visualizer| visualizer.id == destination.id)
            {
                return Some(self.timing_visualizer_render_items(
                    visualizer,
                    destination,
                    frame,
                    state,
                    runtime_graphs.result_timing_points,
                ));
            }
            if let Some(graph) =
                self.timingdistributiongraph.iter().find(|graph| graph.id == destination.id)
            {
                return Some(self.timing_distribution_graph_render_items(
                    graph,
                    destination,
                    frame,
                    state,
                    runtime_graphs.result_timing_points,
                    runtime_graphs.result_timing_distribution,
                ));
            }
            if let Some(gauge_graph) =
                self.gaugegraph.iter().find(|graph| graph.id == destination.id)
            {
                return Some(self.gaugegraph_render_items(
                    destination_index,
                    gauge_graph,
                    destination,
                    frame,
                    state,
                    runtime_graphs.result_gauge_graph_points,
                    cache,
                ));
            }
            if let Some(judge_graph) =
                self.judgegraph.iter().find(|graph| graph.id == destination.id)
            {
                return Some(self.judgegraph_render_items(
                    destination_index,
                    judge_graph,
                    destination,
                    frame,
                    elapsed,
                    state,
                    runtime_graphs,
                    cache,
                ));
            }
            if let Some(bpm_graph) = self.bpmgraph.iter().find(|graph| graph.id == destination.id) {
                return Some(self.bpmgraph_render_items_with_segments(
                    bpm_graph,
                    destination,
                    frame,
                    state,
                    runtime_graphs.play_bpm_graph_segments,
                ));
            }
            if let Some(pmchara) = self.pmchara.iter().find(|pmchara| pmchara.id == destination.id)
            {
                return Some(pm_chara_render_items(
                    pmchara,
                    destination,
                    frame,
                    elapsed,
                    state,
                    sources,
                    self.w,
                    self.h,
                ));
            }
            if let Some(item) = self.direct_source_image_render_item(destination, frame, sources) {
                return Some(vec![item]);
            }
            if let Some(items) =
                self.resolve_image_destination_items(destination, frame, images, state, sources)
            {
                return items;
            }

            if let Some(items) = self.resolve_bga_destination_items(destination, frame, state) {
                return items;
            }

            // imageset (キービーム・ボム等) を destination 自身のタイマー駆動で描画する。
            // timer が非アクティブな destination は上の skin_timer_elapsed_ms で除外済み。
            if let Some(items) =
                self.resolve_imageset_destination_items(destination, frame, images, state, sources)
            {
                return items;
            }

            if let Some(value) = value_for_destination {
                let number = skin_value_number_for_destination(value, state)?;
                let signed_render = signed_number_render_for_value(value, state);
                // Evaluate the value callback before consulting the cache: a
                // stable return value does not imply a side-effect-free closure.
                if let Some(cache) = cache.map(|cache| &mut cache.numbers).or(number_cache) {
                    return Some(cache.render(
                        self,
                        destination_index,
                        &value.id,
                        number,
                        frame,
                        elapsed,
                        sources,
                        signed_render,
                    ));
                }
                return Some(self.value_number_render_items(
                    &value.id,
                    number,
                    ResolvedSkinFrame::default(),
                    frame,
                    elapsed,
                    sources,
                    false,
                    None,
                    signed_render,
                ));
            }

            if let Some(graph) = self.graph.iter().find(|graph| graph.id == destination.id) {
                return self.graph_render_item(graph, frame, state, sources).map(|item| vec![item]);
            }

            if let Some(text) = self.text.iter().find(|text| text.id == destination.id)
                && let Some(item) =
                    self.text_render_item_with_draw_state(text, frame, Some(state), text_state)
            {
                return Some(vec![item]);
            }

            if let Some(slider) = self.slider.iter().find(|slider| slider.id == destination.id)
                && let Some(item) =
                    self.slider_render_item(slider, destination, frame, state, sources)
            {
                return Some(vec![item]);
            }

            if self.destination_uses_skin_gauge_overlay_render(destination) {
                return self.resolve_gauge_destination_items(
                    destination,
                    enabled_options,
                    state,
                    sources,
                );
            }

            if let Some(item) = special_image_render_item(destination, frame, self.w, self.h) {
                return Some(vec![item]);
            }

            if let Some(lift_cover) =
                self.lift_cover.iter().find(|cover| cover.id == destination.id)
            {
                return self
                    .hidden_cover_render_item(lift_cover, destination, frame, state, sources)
                    .map(|item| vec![item]);
            }
            let hidden_cover = self.hidden_cover.iter().find(|cover| cover.id == destination.id)?;
            self.hidden_cover_render_item(hidden_cover, destination, frame, state, sources)
                .map(|item| vec![item])
        }

        fn resolve_image_destination_items(
            &self,
            destination: &SkinDestinationDef,
            mut frame: ResolvedSkinFrame,
            images: &SkinImageLookup<'_>,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Option<Vec<SkinRenderItem>>> {
            let image = skin_image_for_destination_id(destination.id.as_str(), images)?;
            if self.should_skip_lift_lane_cover_render(destination, image)
                && state.offset_lift_px == 0
            {
                return Some(None);
            }
            if let Some((r, g, b)) =
                result_judge_pie_segment_color(destination, image, frame, state)
            {
                frame.r = r;
                frame.g = g;
                frame.b = b;
            }
            let Some(source) = resolve_document_source(sources, &image.src) else {
                return Some(None);
            };
            let pixel_rect = skin_image_pixel_rect(image);
            let mut uv =
                skin_image_texture_region_for_state(image, source.source_size, state, pixel_rect);
            if self.should_clip_image_at_disappear_line(destination, image)
                && let Some((disappear_line, link_lift)) = self.disappear_line_for_lane_cover_clip()
            {
                clip_skin_cover_to_disappear_line(
                    &mut frame,
                    &mut uv,
                    disappear_line,
                    self.link_lift_for_lane_cover_clip(destination, image, link_lift),
                    state,
                );
                if frame.h <= 0 {
                    return Some(None);
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
            Some(Some(vec![skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                skin_blend_mode(destination.blend),
                Some(source.source_size),
                destination.filter != 0,
            )]))
        }

        fn resolve_bga_destination_items(
            &self,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
        ) -> Option<Option<Vec<SkinRenderItem>>> {
            if !self.bga.as_ref().is_some_and(|bga| bga.id == destination.id) {
                return None;
            }
            if !state.has_bga || !state.bga_enabled {
                return Some(None);
            }

            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            let blend = skin_blend_mode(destination.blend);
            let destination_tint = Color::rgba(1.0, 1.0, 1.0, frame.a as f32 / 255.0);
            let stretch =
                if destination.stretch < 0 { state.bga_stretch } else { destination.stretch };
            let linear_filter = destination.filter != 0;
            let mut items = Vec::new();
            if let Some(bga) = state.bga_poor {
                items.push(bga_image_item(
                    bga,
                    stretch,
                    rect,
                    multiply_bga_tints(destination_tint, bga),
                    blend,
                    self.w,
                    self.h,
                    linear_filter,
                ));
            } else if let Some(bga) = state.bga_base {
                items.push(bga_image_item(
                    bga,
                    stretch,
                    rect,
                    multiply_bga_tints(destination_tint, bga),
                    blend,
                    self.w,
                    self.h,
                    linear_filter,
                ));
            }

            if state.bga_poor.is_none() {
                for bga in [state.bga_layer, state.bga_layer2].into_iter().flatten() {
                    let layer_blend =
                        if matches!(blend, BlendMode::Add | BlendMode::Multiply) || bga.is_video {
                            blend
                        } else {
                            BlendMode::LayerMask
                        };
                    items.push(bga_image_item(
                        bga,
                        stretch,
                        rect,
                        multiply_bga_tints(destination_tint, bga),
                        layer_blend,
                        self.w,
                        self.h,
                        linear_filter,
                    ));
                }
            }
            if items.is_empty() {
                items.push(SkinRenderItem::Rect {
                    rect,
                    color: Color::rgba(0.0, 0.0, 0.0, frame.a as f32 / 255.0),
                    blend,
                });
            }
            Some(Some(items))
        }

        fn resolve_imageset_destination_items(
            &self,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            images: &SkinImageLookup<'_>,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Option<Vec<SkinRenderItem>>> {
            let imageset = self.imageset.iter().find(|set| set.id == destination.id)?;
            let image_id = if let Some(index) = skin_state_imageset_index(imageset.ref_id, state) {
                imageset.images.get(index.min(imageset.images.len().saturating_sub(1))).cloned()
            } else {
                let judge_index = imageset_ref_lane(imageset.ref_id)
                    .and_then(|lane| state.lane_judge[lane.index()]);
                imageset_image_for_index(imageset, judge_index)
            };
            let Some(image_id) = image_id else {
                return Some(None);
            };
            let Some(image) = images.get(image_id.as_str()) else {
                return Some(None);
            };
            let Some(source) = resolve_document_source(sources, &image.src) else {
                return Some(None);
            };
            let pixel_rect = skin_image_pixel_rect(image);
            let (rect, uv) = stretch_skin_image_geometry(
                destination.stretch,
                normalize_skin_frame_rect(frame, self.w, self.h),
                skin_image_texture_region_for_state(image, source.source_size, state, pixel_rect),
                source.source_size,
                self.w,
                self.h,
            );
            Some(Some(vec![skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                frame,
                destination.center,
                skin_blend_mode(destination.blend),
                Some(source.source_size),
                destination.filter != 0,
            )]))
        }

        fn resolve_offset_destination_items(
            &self,
            destination: &SkinDestinationDef,
            offset: (i32, i32),
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &SkinDrawState,
            text_state: &SkinTextState<'_>,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Vec<SkinRenderItem>> {
            if !destination_ops_match(destination, enabled_options, state)
                || !eval_skin_draw_condition(&destination.draw, state)
            {
                return None;
            }
            let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
            let mut frame =
                resolve_destination_frame(destination, elapsed, enabled_options, state)?;
            frame.x += offset.0;
            frame.y += offset.1;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);

            if let Some(panel) = self.panel.iter().find(|panel| panel.id == destination.id) {
                return Some(skin_panel_render_items(panel, destination, frame, self.w, self.h));
            }

            if let Some(image) = skin_image_for_destination_id(destination.id.as_str(), images) {
                if self.should_skip_lift_lane_cover_render(destination, image)
                    && state.offset_lift_px == 0
                {
                    return None;
                }
                let source = resolve_document_source(sources, &image.src)?;
                let pixel_rect = skin_image_pixel_rect(image);
                let mut uv = skin_image_texture_region_for_state(
                    image,
                    source.source_size,
                    state,
                    pixel_rect,
                );
                if self.should_clip_image_at_disappear_line(destination, image)
                    && let Some((disappear_line, link_lift)) =
                        self.disappear_line_for_lane_cover_clip()
                {
                    clip_skin_cover_to_disappear_line(
                        &mut frame,
                        &mut uv,
                        disappear_line,
                        self.link_lift_for_lane_cover_clip(destination, image, link_lift),
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
                return Some(vec![skin_image_item_for_frame(
                    source.texture,
                    rect,
                    uv,
                    frame,
                    destination.center,
                    skin_blend_mode(destination.blend),
                    Some(source.source_size),
                    destination.filter != 0,
                )]);
            }

            if let Some(value) = self.value.iter().find(|value| value.id == destination.id) {
                let number = skin_value_number_for_destination(value, state)?;
                let signed_render = signed_number_render_for_value(value, state);
                return Some(self.value_number_render_items(
                    &value.id,
                    number,
                    ResolvedSkinFrame::default(),
                    frame,
                    elapsed,
                    sources,
                    false,
                    None,
                    signed_render,
                ));
            }

            if let Some(graph) = self.graph.iter().find(|graph| graph.id == destination.id)
                && let Some(item) = self.graph_render_item(graph, frame, state, sources)
            {
                return Some(vec![item]);
            }

            if let Some(text) = self.text.iter().find(|text| text.id == destination.id)
                && let Some(item) =
                    self.text_render_item_with_draw_state(text, frame, Some(state), text_state)
            {
                return Some(vec![item]);
            }

            None
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_core_resolve_methods;
