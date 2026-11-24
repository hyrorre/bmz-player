use super::super::*;
use crate::plan::TextSegmentEnd;

pub(in crate::skin::document_render) fn distribution_label_render_item(
    label: &SkinGraphLabelDef,
    font: &str,
    segment: Rect,
    canvas_size: (u32, u32),
    vertical: bool,
    from_right: bool,
    alpha: i32,
) -> Option<SkinRenderItem> {
    let canvas_w = canvas_size.0.max(1) as f32;
    let canvas_h = canvas_size.1.max(1) as f32;
    let padding = 1.0;
    let segment_w = segment.width * canvas_w;
    let segment_h = segment.height * canvas_h;
    let (cross, length) = if vertical { (segment_w, segment_h) } else { (segment_h, segment_w) };
    if label.text.is_empty() || cross <= 2.0 * padding || length <= 2.0 * padding {
        return None;
    }
    let (x, y, end) = if vertical {
        (segment_w * 0.5, segment_h - padding, TextSegmentEnd::Bottom)
    } else if from_right {
        (padding, segment_h * 0.5, TextSegmentEnd::Left)
    } else {
        (segment_w - padding, segment_h * 0.5, TextSegmentEnd::Right)
    };
    let channel = if label.dark { 0.0 } else { 1.0 };
    Some(SkinRenderItem::Text {
        origin: Point { x: segment.x + x / canvas_w, y: segment.y + y / canvas_h },
        text: label.text.clone(),
        style: TextStyle {
            font_id: Some(font.to_owned()),
            size: (cross - 2.0 * padding) / canvas_h,
            bitmap_size: None,
            color: Color::rgba(channel, channel, channel, alpha.clamp(0, 255) as f32 / 255.0),
            layer: TextLayer::Ui,
            align: TextAlign::Left,
            max_width: (length - 2.0 * padding) / canvas_w,
            overflow: TextOverflow::FitSegment(end),
            wrapping: false,
            outline: None,
            shadow: None,
            gradient: None,
        },
        caret: None,
        blend: BlendMode::Normal,
        post_scale: Point { x: 1.0, y: 1.0 },
    })
}

macro_rules! skin_document_render_select_graph_methods {
    () => {
        fn select_folder_distribution_graph_render_items(
            &self,
            row: &SelectRowSnapshot,
            graph: &SkinGraphDef,
            destination: &SkinDestinationDef,
            row_origin: (i32, i32),
            enabled_options: &[i32],
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            // This graph slot follows a songlist row; screen-wide distributions use a destination.
            if graph.distribution.is_some() {
                return Vec::new();
            }
            let data = if select_row_shows_folder_distribution(row) {
                SelectBarGraph::Distribution(&row.folder_lamp_counts)
            } else if let Some(progress) = row.download_progress.filter(|_| !graph.lr2_distribution)
            {
                SelectBarGraph::Download(progress)
            } else {
                return Vec::new();
            };
            if !destination_ops_match(destination, enabled_options, state)
                || !eval_skin_draw_condition(&destination.draw, state)
            {
                return Vec::new();
            }
            let Some(elapsed) = destination_timer_elapsed_ms(destination, state) else {
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
            self.select_bar_graph_render_items(
                data,
                graph,
                destination,
                frame,
                elapsed,
                state,
                sources,
            )
        }

        fn select_bar_graph_render_items(
            &self,
            data: SelectBarGraph<'_>,
            graph: &SkinGraphDef,
            destination: &SkinDestinationDef,
            frame: ResolvedSkinFrame,
            elapsed: i32,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            let Some(source) = sources.get(&graph.src) else {
                return Vec::new();
            };
            let standalone = graph.distribution.is_some();
            let source_w = source.source_size.width.max(1.0);
            let source_h = source.source_size.height.max(1.0);
            let min_columns = if graph.lr2_distribution {
                1
            } else {
                match data {
                    SelectBarGraph::Distribution(counts) => counts.len() as i32,
                    SelectBarGraph::Download(_) => 11,
                }
            };
            let columns = graph.divx.max(min_columns);
            let rows = graph.divy.max(1);
            let width =
                if graph.lr2_distribution && graph.w == -1 { source_w as i32 } else { graph.w };
            let height =
                if graph.lr2_distribution && graph.h == -1 { source_h as i32 } else { graph.h };
            let cell_w = skin_grid_cell_size(width, columns);
            let cell_h = skin_grid_cell_size(height, rows);
            if cell_w <= 0 || cell_h <= 0 {
                return Vec::new();
            }
            let animation_frames = if graph.lr2_distribution {
                i64::from(columns) * i64::from(rows) / 11
            } else {
                i64::from(rows)
            };
            if animation_frames == 0 {
                return Vec::new();
            }
            let source_elapsed = if graph.lr2_distribution {
                skin_timer_elapsed_ms(graph.timer.filter(|timer| *timer > 0), state)
                    .unwrap_or(0)
                    .max(0)
            } else {
                elapsed
            };
            let animation_frame = skin_source_image_animation_frame_index(
                animation_frames,
                graph.cycle,
                source_elapsed,
            );
            let tint = Color::rgba(
                frame.r as f32 / 255.0,
                frame.g as f32 / 255.0,
                frame.b as f32 / 255.0,
                frame.a as f32 / 255.0,
            );
            let cell = |bucket: usize, rect: Rect| {
                let index = animation_frame
                    * if graph.lr2_distribution { 11 } else { i64::from(columns) }
                    + bucket as i64;
                let source_x = graph
                    .x
                    .saturating_add(cell_w.saturating_mul((index % i64::from(columns)) as i32));
                let source_y = graph
                    .y
                    .saturating_add(cell_h.saturating_mul((index / i64::from(columns)) as i32));
                SkinRenderItem::Image {
                    texture: source.texture,
                    rect,
                    uv: TextureRegion {
                        x: source_x as f32 / source_w,
                        y: source_y as f32 / source_h,
                        width: cell_w as f32 / source_w,
                        height: cell_h as f32 / source_h,
                    },
                    tint,
                    blend: if graph.lr2_distribution || standalone {
                        frame.blend(skin_blend_mode(destination.blend))
                    } else {
                        BlendMode::Normal
                    },
                    scale: SkinImageScale::Stretch,
                    border: None,
                    source_size: Some(source.source_size),
                    linear_filter: (graph.lr2_distribution || standalone)
                        && frame.linear_filter(destination.filter != 0),
                }
            };

            let dst = normalize_skin_frame_rect(frame, self.w, self.h);
            let counts = match data {
                SelectBarGraph::Distribution(counts) => counts,
                SelectBarGraph::Download(progress) => {
                    // Endless Dream: cell 0 fills the region, and the progress cell
                    // fills a 1px-inset bar from the left.
                    let mut items = vec![cell(0, dst)];
                    let fraction = progress.fraction.clamp(0.0, 1.0);
                    if fraction > 0.0 && frame.w.abs() > 2 && frame.h.abs() > 2 {
                        let mut inner = frame;
                        inner.x += frame.w.signum();
                        inner.y += frame.h.signum();
                        inner.w -= 2 * frame.w.signum();
                        inner.h -= 2 * frame.h.signum();
                        let mut rect = normalize_skin_frame_rect(inner, self.w, self.h);
                        if frame.w < 0 {
                            rect.x += rect.width * (1.0 - fraction);
                        }
                        rect.width *= fraction;
                        let lamp = usize::from(progress.lamp).min(columns as usize - 1);
                        items.push(cell(lamp, rect));
                    }
                    return items;
                }
            };

            let total: u64 = counts.iter().copied().map(u64::from).sum();
            if total == 0 {
                return Vec::new();
            }

            let vertical = standalone && graph.angle == 1;
            let from_right = standalone && frame.w < 0;
            let labels = graph.labels.as_ref().filter(|_| standalone);
            let mut items = Vec::new();
            let mut filled = 0u64;
            for (bucket, &count) in counts.iter().enumerate().rev() {
                if count == 0 {
                    continue;
                }
                let start = filled as f32 / total as f32;
                filled += u64::from(count);
                let end = filled as f32 / total as f32;
                let rect = if vertical {
                    Rect {
                        y: dst.y + dst.height * (1.0 - end),
                        height: dst.height * (end - start),
                        ..dst
                    }
                } else {
                    let offset = if from_right { 1.0 - end } else { start };
                    Rect { x: dst.x + dst.width * offset, width: dst.width * (end - start), ..dst }
                };
                if rect.width > 0.0 && rect.height > 0.0 {
                    items.push(cell(bucket, rect));
                    if let Some(labels) = labels
                        && let Some(label) = labels.items.iter().find(|label| label.index == bucket)
                        && let Some(item) = select::distribution_label_render_item(
                            label,
                            &labels.font,
                            rect,
                            (self.w, self.h),
                            vertical,
                            from_right,
                            frame.a,
                        )
                    {
                        items.push(item);
                    }
                }
            }
            items
        }

        fn select_songlist_level_items(
            &self,
            entries: &[DestinationListEntry],
            row: &SelectRowSnapshot,
            row_origin: (i32, i32),
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &mut SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            // "11+" / "12.5" show their leading integer; labels such as "???" or
            // "地力A" fall back to the chart's own level, as on LR2.
            let level_override =
                row.level_display_override.as_deref().and_then(table_level_display_number);
            let previous =
                std::mem::replace(&mut state.select_songlist_level_override, level_override);
            let level_index = select_row_difficulty_code(row).clamp(0, i64::MAX) as usize;
            let items = self.select_songlist_child_items_by_index(
                entries,
                level_index,
                row_origin,
                images,
                enabled_options,
                state,
                sources,
            );
            state.select_songlist_level_override = previous;
            items
        }

        fn select_songlist_child_items_by_index(
            &self,
            entries: &[DestinationListEntry],
            index: usize,
            row_origin: (i32, i32),
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            let Some(destination) =
                songlist_destination_at(entries, index, enabled_options, state, self.lr2)
            else {
                return Vec::new();
            };
            self.resolve_offset_destination_items(
                destination,
                row_origin,
                images,
                enabled_options,
                state,
                &SkinTextState::default(),
                sources,
            )
            .unwrap_or_default()
        }

        fn select_songlist_text_items(
            &self,
            destinations: &[&SkinDestinationDef],
            row: &SelectRowSnapshot,
            row_origin: (i32, i32),
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            let Some(songlist) = &self.songlist else {
                return Vec::new();
            };
            let text_state = SkinTextState {
                bar_text: row.display_bar_text(),
                table_level: if row.table_text_secondary.is_empty() {
                    &row.table_level
                } else {
                    &row.table_text_secondary
                },
                table_text_primary: &row.table_text_primary,
                table_text_secondary: &row.table_text_secondary,
                table_text_fallback: &row.table_text_fallback,
                ..SkinTextState::default()
            };
            let destination = if self.lr2 {
                songlist_destination_at(
                    &songlist.text,
                    usize::from(row.lr2_new_title),
                    enabled_options,
                    state,
                    true,
                )
            } else {
                select_row_slot_with_fallbacks(
                    destinations,
                    select_row_bar_text_index(row),
                    select_row_bar_text_fallback_indices(row),
                )
                .copied()
            };
            destination
                .and_then(|destination| {
                    self.resolve_offset_destination_items(
                        destination,
                        row_origin,
                        images,
                        enabled_options,
                        state,
                        &text_state,
                        sources,
                    )
                })
                .unwrap_or_default()
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_select_graph_methods;
