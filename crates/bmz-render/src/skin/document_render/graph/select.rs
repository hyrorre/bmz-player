macro_rules! skin_document_render_graph_select_methods {
    () => {
        fn select_note_distribution_graph_render_items(
            &self,
            row: &SelectRowSnapshot,
            graph: &SkinJudgeGraphDef,
            destination: &SkinDestinationDef,
            row_origin: (i32, i32),
            enabled_options: &[i32],
            state: &SkinDrawState,
        ) -> Vec<SkinRenderItem> {
            if row.chart_distribution.is_empty()
                || !destination_ops_match(destination, enabled_options, state)
                || !eval_skin_draw_condition(&destination.draw, state)
                || graph.graph_type() != 0
            {
                return Vec::new();
            }
            let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, state) else {
                return Vec::new();
            };
            let Some(mut frame) =
                resolve_destination_frame(destination, elapsed, enabled_options, state)
            else {
                return Vec::new();
            };
            frame.x += row_origin.0;
            frame.y += row_origin.1;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);
            if !destination_mouse_rect_contains(destination, frame, state) {
                return Vec::new();
            }

            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            if rect.width <= 0.0 || rect.height <= 0.0 {
                return Vec::new();
            }
            let frame_alpha = frame.a as f32 / 255.0;
            let blend = skin_blend_mode(destination.blend);
            let max_density = select_note_distribution_max_density(&row.chart_distribution) as f32;
            let count = row.chart_distribution.len().max(1) as f32;
            let pixel_w = 1.0 / self.w.max(1) as f32;
            let pixel_h = 1.0 / self.h.max(1) as f32;
            let gap_x = if graph.no_gap_x != 0 { 0.0 } else { pixel_w };
            let gap_y = if graph.no_gap != 0 { 0.0 } else { pixel_h };
            let bar_w = ((rect.width - gap_x * (count - 1.0)).max(pixel_w) / count).max(pixel_w);
            let colors = note_distribution_colors(frame_alpha);
            let mut items = Vec::new();
            if graph.back_tex_off == 0 {
                items.extend(select_note_distribution_background_items(
                    rect,
                    row.chart_distribution.len(),
                    max_density as u32,
                    frame_alpha,
                    blend,
                    pixel_w,
                    pixel_h,
                ));
            }
            let reveal = if graph.delay > 0 {
                (elapsed as f32 / graph.delay as f32).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let reveal_right = rect.x + rect.width * reveal;

            for (index, second) in row.chart_distribution.iter().enumerate() {
                let x = rect.x + index as f32 * (bar_w + gap_x);
                if x >= reveal_right {
                    break;
                }
                let visible_bar_w = bar_w.min((reveal_right - x).max(0.0));
                if visible_bar_w <= 0.0 {
                    continue;
                }
                let values = second.values();
                let iter: Box<dyn Iterator<Item = (usize, u16)>> = if graph.order_reverse != 0 {
                    Box::new(values.into_iter().enumerate().rev())
                } else {
                    Box::new(values.into_iter().enumerate())
                };
                let mut y_cursor = rect.y + rect.height;
                for (series, value) in iter {
                    if value == 0 {
                        continue;
                    }
                    let height = (rect.height * (value as f32 / max_density) - gap_y).max(pixel_h);
                    y_cursor -= height;
                    items.push(SkinRenderItem::Rect {
                        rect: Rect { x, y: y_cursor, width: visible_bar_w, height },
                        color: colors[series],
                        blend,
                    });
                    y_cursor -= gap_y;
                    if y_cursor <= rect.y {
                        break;
                    }
                }
            }

            items
        }

        fn select_bpmgraph_row_render_items(
            &self,
            row: &SelectRowSnapshot,
            graph: &SkinBpmGraphDef,
            destination: &SkinDestinationDef,
            row_origin: (i32, i32),
            enabled_options: &[i32],
            state: &SkinDrawState,
        ) -> Vec<SkinRenderItem> {
            if row.chart_bpm_graph_segments.is_empty()
                || !destination_ops_match(destination, enabled_options, state)
                || !eval_skin_draw_condition(&destination.draw, state)
            {
                return Vec::new();
            }
            let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, state) else {
                return Vec::new();
            };
            let Some(mut frame) =
                resolve_destination_frame(destination, elapsed, enabled_options, state)
            else {
                return Vec::new();
            };
            frame.x += row_origin.0;
            frame.y += row_origin.1;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);
            if !destination_mouse_rect_contains(destination, frame, state) {
                return Vec::new();
            }
            self.bpmgraph_render_items_with_segments(
                graph,
                destination,
                frame,
                state,
                &row.chart_bpm_graph_segments,
            )
        }

        fn bpmgraph_render_items_with_segments(
            &self,
            graph: &SkinBpmGraphDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            state: &SkinDrawState,
            segments: &[crate::chart_graph::BpmGraphSegment],
        ) -> Vec<SkinRenderItem> {
            if segments.is_empty() {
                return Vec::new();
            }
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            let frame_alpha = frame.a as f32 / 255.0;
            let blend = skin_blend_mode(destination.blend);
            let main_bpm = state.main_bpm.max(1.0);
            let canvas_w = self.w.max(1) as f32;
            let canvas_h = self.h.max(1) as f32;
            // lineWidth は canvas pixel 単位。正規化座標系に変換する。
            // 未指定 (0) のときは beatoraja デフォルトの 2 を使う。
            let canvas_line_px = if graph.line_width > 0 { graph.line_width } else { 2 } as f32;
            let line_w = canvas_line_px / canvas_w;
            let line_h = canvas_line_px / canvas_h;
            // beatoraja デフォルト色: main=緑, min=青, max=赤, other=黄, stop=紫, transition=灰
            let main_color = skin_hex_color(&graph.main_bpm_color)
                .unwrap_or(Color::rgba(0.0, 1.0, 0.0, 1.0))
                .with_alpha(frame_alpha);
            let min_color = skin_hex_color(&graph.min_bpm_color)
                .unwrap_or(Color::rgba(0.0, 0.0, 1.0, 1.0))
                .with_alpha(frame_alpha);
            let max_color = skin_hex_color(&graph.max_bpm_color)
                .unwrap_or(Color::rgba(1.0, 0.0, 0.0, 1.0))
                .with_alpha(frame_alpha);
            let other_color = skin_hex_color(&graph.other_bpm_color)
                .unwrap_or(Color::rgba(1.0, 1.0, 0.0, 1.0))
                .with_alpha(frame_alpha);
            let stop_color = skin_hex_color(&graph.stop_line_color)
                .unwrap_or(Color::rgba(1.0, 0.0, 1.0, 1.0))
                .with_alpha(frame_alpha);
            let transition_color = skin_hex_color(&graph.transition_line_color)
                .unwrap_or(Color::rgba(0.5, 0.5, 0.5, 1.0))
                .with_alpha(frame_alpha);
            // beatoraja: log10(bpm/mainbpm) を [log10(1/8), log10(8)] に正規化。
            // ratio=0 → グラフ上部 (低BPM / stop)、ratio=1 → グラフ下部 (高BPM)。
            let min_log: f32 = (1.0_f32 / 8.0).log10();
            let max_log: f32 = 8.0_f32.log10();
            let log_range = max_log - min_log;
            // bpm=0 (stop) は min 側にクランプされグラフ上部に描画される。
            let bpm_to_ratio = |bpm: f32| -> f32 {
                let r = (bpm / main_bpm).clamp(1.0 / 8.0, 8.0);
                ((r.log10() - min_log) / log_range).clamp(0.0, 1.0)
            };
            // ratio=0 → top (rect.y + rect.height)、ratio=1 → bottom (rect.y)
            let ratio_to_y =
                |ratio: f32| -> f32 { rect.y + rect.height * (1.0 - ratio) - line_h / 2.0 };
            let mut items = Vec::new();
            let mut prev_ratio: Option<f32> = None;
            for segment in segments {
                let x0 = rect.x + segment.start_ratio.clamp(0.0, 1.0) * rect.width;
                let x1 = rect.x + segment.end_ratio.clamp(0.0, 1.0) * rect.width;
                let bpm = if segment.is_stop { 0.0 } else { segment.bpm };
                let cur_ratio = bpm_to_ratio(bpm);
                // BPM変化点を transitionLineColor の縦線で繋ぐ (beatoraja 互換)。
                if let Some(prev) = prev_ratio {
                    let y_prev = ratio_to_y(prev);
                    let y_cur = ratio_to_y(cur_ratio);
                    let height = (y_prev - y_cur).abs() - line_h;
                    if height > 0.0 {
                        let y_bottom = y_prev.min(y_cur) + line_h;
                        items.push(SkinRenderItem::Rect {
                            rect: Rect { x: x0 - line_w / 2.0, y: y_bottom, width: line_w, height },
                            color: transition_color,
                            blend,
                        });
                    }
                }
                let y = ratio_to_y(cur_ratio);
                let color = if segment.is_stop {
                    stop_color
                } else if (segment.bpm - state.main_bpm).abs() < 0.5 {
                    main_color
                } else if (segment.bpm - state.min_bpm).abs() < 0.5 {
                    min_color
                } else if (segment.bpm - state.max_bpm).abs() < 0.5 {
                    max_color
                } else {
                    other_color
                };
                items.push(SkinRenderItem::Rect {
                    rect: Rect { x: x0, y, width: (x1 - x0).max(line_w), height: line_h },
                    color,
                    blend,
                });
                prev_ratio = Some(cur_ratio);
            }
            items
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_graph_select_methods;
