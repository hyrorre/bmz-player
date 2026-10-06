macro_rules! skin_document_render_graph_visualizer_methods {
    () => {
        fn hiterror_visualizer_render_items(
            &self,
            visualizer: &SkinHitErrorVisualizerDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
        ) -> Vec<SkinRenderItem> {
            if visualizer.hiterror_mode == 0 {
                return Vec::new();
            }
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            let frame_alpha = frame.a as f32 / 255.0;
            let blend = skin_blend_mode(destination.blend);
            let window = visualizer.window_length.clamp(1, 100) as usize;
            let width = visualizer.width.max(1) as f32;
            let line_width = visualizer.line_width.clamp(1, 4) as f32;
            let center_ms = visualizer.judge_width_millis.max(1) as f32;
            let judge_width_rate = width / (center_ms * 2.0 + 1.0);
            let line_color =
                skin_hex_color(&visualizer.line_color).unwrap_or(Color::rgba(0.6, 0.8, 1.0, 0.5));
            let center_color =
                skin_hex_color(&visualizer.center_color).unwrap_or(Color::rgba(1.0, 1.0, 1.0, 1.0));
            let canvas_h = rect.height.max(1.0);
            let mut items = Vec::new();
            let center_x = rect.x + rect.width / 2.0 - line_width / 2.0;
            items.push(SkinRenderItem::Rect {
                rect: Rect { x: center_x, y: rect.y, width: line_width, height: canvas_h },
                color: center_color.with_alpha(center_color.a * frame_alpha),
                blend,
            });
            let index = state.hit_error_ring_index;
            let recent = &state.hit_error_ring;
            for i in 1..=window {
                let ring_index = (index as i64 - window as i64 + i as i64)
                    .rem_euclid(bmz_gameplay::hit_error::HIT_ERROR_RING_LEN as i64)
                    as usize;
                let sample = recent[ring_index];
                if sample == bmz_gameplay::hit_error::HIT_ERROR_EMPTY {
                    continue;
                }
                let clamped = sample.clamp(
                    -visualizer.judge_width_millis as i64,
                    visualizer.judge_width_millis as i64,
                ) as f32;
                let x = rect.x + width / 2.0 - line_width / 2.0 - clamped * judge_width_rate;
                let alpha = if visualizer.color_mode == 0 {
                    line_color.a * (i as f32 / (window as f32 / 2.0)).min(1.0)
                } else {
                    line_color.a
                };
                let bar_h = if visualizer.draw_decay != 0 {
                    canvas_h * i as f32 / window as f32
                } else {
                    canvas_h
                };
                items.push(SkinRenderItem::Rect {
                    rect: Rect {
                        x,
                        y: rect.y + canvas_h - bar_h,
                        width: line_width,
                        height: bar_h,
                    },
                    color: Color::rgba(
                        line_color.r,
                        line_color.g,
                        line_color.b,
                        alpha * frame_alpha,
                    ),
                    blend,
                });
            }
            items
        }

        fn gaugegraph_render_items(
            &self,
            destination_index: usize,
            graph: &SkinGaugeGraphDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
            points: &[crate::snapshot::ResultGaugeGraphPoint],
            mut cache: Option<&mut ResultRenderCache>,
        ) -> Vec<SkinRenderItem> {
            let cached_points = state
                .result_gauge_graph_type
                .and_then(|gauge_type| cache.as_deref_mut()?.cached_gauge_points(gauge_type));
            let graph_revision = cached_points
                .as_ref()
                .map(|(revision, _)| *revision)
                .or_else(|| cache.as_deref().and_then(ResultRenderCache::gauge_graph_revision));
            let uncached_filtered_points = if cached_points.is_none() {
                state.result_gauge_graph_type.map(|gauge_type| {
                    points
                        .iter()
                        .copied()
                        .filter(|point| point.gauge_type == gauge_type)
                        .collect::<Vec<_>>()
                })
            } else {
                None
            };
            let points = cached_points
                .as_ref()
                .map(|(_, points)| points.as_ref())
                .or_else(|| {
                    uncached_filtered_points.as_deref().filter(|filtered| !filtered.is_empty())
                })
                .unwrap_or(points);
            if points.is_empty() {
                return Vec::new();
            }
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            let frame_alpha = frame.a as f32 / 255.0;
            let max = points
                .iter()
                .find_map(|point| (point.max > 0.0).then_some(point.max))
                .unwrap_or(state.gauge_max)
                .max(1.0);
            let display_gauge_type = state.result_gauge_graph_type.unwrap_or_else(|| {
                points.last().map(|point| point.gauge_type).unwrap_or(state.gauge_type)
            });
            let border = points.first().map(|point| point.border).unwrap_or(state.gauge_border);
            let color_index = gaugegraph_color_index(display_gauge_type);
            let colors = gaugegraph_colors(graph, color_index, frame_alpha);
            let line_w = (2.0 / self.w.max(1) as f32).max(0.001);
            let line_h = (2.0 / self.h.max(1) as f32).max(0.001);
            let render_progress = (state.elapsed_ms.max(0) as f32 / 1500.0).clamp(0.0, 1.0);
            let build = || {
                gaugegraph_rect_batch(
                    points,
                    GaugeGraphLayout {
                        rect,
                        max,
                        border,
                        colors,
                        line_width: line_w,
                        line_height: line_h,
                        render_progress,
                        additive: destination.blend == 2,
                    },
                )
            };
            let completed = render_progress >= 1.0;
            let key = graph_revision.map(|graph_revision| ResultGaugeGraphRectBatchCacheKey {
                destination_index,
                frame: frame.geometry_cache_frame(),
                graph_revision,
                display_gauge_type,
                gauge_max_bits: max.to_bits(),
                gauge_border_bits: border.to_bits(),
            });
            let rects = if completed {
                if let (Some(cache), Some(key)) = (cache, key) {
                    cache.cached_gauge_rect_batch(key, build)
                } else {
                    build()
                }
            } else {
                build()
            };
            let batch_cache = completed
                .then(|| key.and_then(|key| result_gauge_graph_rect_batch_cache(key, &rects)))
                .flatten();
            rect_batch_render_items(rects, batch_cache)
        }

        fn timing_visualizer_render_items(
            &self,
            visualizer: &SkinTimingVisualizerDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
            timing_points: &[crate::snapshot::ResultTimingPoint],
        ) -> Vec<SkinRenderItem> {
            if timing_points.is_empty() {
                return Vec::new();
            }
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            let frame_alpha = frame.a as f32 / 255.0;
            let blend = skin_blend_mode(destination.blend);
            let width = visualizer.width.max(1) as f32;
            let center_ms = visualizer.judge_width_millis.max(1) as f32;
            let line_w =
                (visualizer.line_width.clamp(1, 4) as f32 / self.w.max(1) as f32).max(0.001);
            let judge_width_rate = width / (center_ms * 2.0 + 1.0);
            let center_color = timing_color(&visualizer.center_color, frame_alpha);
            let base_line_color = timing_color(&visualizer.line_color, frame_alpha);
            let mut items = Vec::new();
            items.extend(timing_judge_band_items(
                rect,
                center_ms,
                frame_alpha,
                blend,
                timing_visualizer_judge_colors(visualizer),
                state,
            ));
            let center_x = rect.x + rect.width / 2.0 - line_w / 2.0;
            items.push(SkinRenderItem::Rect {
                rect: Rect { x: center_x, y: rect.y, width: line_w, height: rect.height },
                color: center_color,
                blend,
            });

            let window = timing_points.len().min(bmz_gameplay::hit_error::HIT_ERROR_RING_LEN);
            for (index, point) in timing_points.iter().rev().take(window).enumerate() {
                let delta_ms = point.delta_us as f32 / 1_000.0;
                if delta_ms.abs() > center_ms {
                    continue;
                }
                let x = rect.x + rect.width / 2.0 - line_w / 2.0
                    + delta_ms * judge_width_rate / width * rect.width;
                let age = (window - index) as f32 / window.max(1) as f32;
                let alpha = if visualizer.draw_decay == 1 { age } else { 1.0 };
                let color = judge_timing_color(point.judge, visualizer, base_line_color)
                    .with_alpha(base_line_color.a * alpha);
                let height =
                    if visualizer.draw_decay == 1 { rect.height * age } else { rect.height };
                items.push(SkinRenderItem::Rect {
                    rect: Rect { x, y: rect.y + rect.height - height, width: line_w, height },
                    color,
                    blend,
                });
            }
            items
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_graph_visualizer_methods;
