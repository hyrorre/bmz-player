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
            let Some(source) = sources.get(&graph.src) else {
                return Vec::new();
            };
            if !destination_ops_match(destination, enabled_options, state)
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

            let source_w = source.source_size.width.max(1.0);
            let source_h = source.source_size.height.max(1.0);
            let cell_w = skin_grid_cell_size(graph.w, graph.divx.max(11));
            let cell_h = skin_grid_cell_size(graph.h, graph.divy);
            if cell_w <= 0 || cell_h <= 0 {
                return Vec::new();
            }
            let animation_rows = graph.divy.max(1);
            let animation_row = if graph.cycle > 0 && animation_rows > 1 {
                (elapsed.rem_euclid(graph.cycle) * animation_rows / graph.cycle)
                    .min(animation_rows - 1)
            } else {
                0
            };
            let tint = Color::rgba(
                frame.r as f32 / 255.0,
                frame.g as f32 / 255.0,
                frame.b as f32 / 255.0,
                frame.a as f32 / 255.0,
            );
            let cell = |lamp_index: usize, rect: Rect| {
                let source_x = graph.x + cell_w * lamp_index as i32;
                let source_y = graph.y + cell_h * animation_row;
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
                    blend: BlendMode::Normal,
                    scale: SkinImageScale::Stretch,
                    border: None,
                    source_size: Some(source.source_size),
                    linear_filter: false,
                }
            };

            if !select_row_shows_folder_distribution(row) {
                let Some(progress) = row.download_progress else {
                    return Vec::new();
                };
                // Endless Dream: cell 0 fills the region, and the progress cell
                // fills a 1px-inset bar from the left.
                let mut items = vec![cell(0, normalize_skin_frame_rect(frame, self.w, self.h))];
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
                    let lamp = usize::from(progress.lamp).min(graph.divx.max(11) as usize - 1);
                    items.push(cell(lamp, rect));
                }
                return items;
            }

            let total: u32 = row.folder_lamp_counts.iter().sum();
            if total == 0 {
                return Vec::new();
            }

            let dst = normalize_skin_frame_rect(frame, self.w, self.h);
            let mut items = Vec::new();
            let mut filled = 0.0;
            for lamp_index in (0..row.folder_lamp_counts.len()).rev() {
                let count = row.folder_lamp_counts[lamp_index];
                if count == 0 {
                    continue;
                }
                let width = dst.width * (count as f32 / total as f32);
                if width <= 0.0 {
                    continue;
                }
                items.push(cell(lamp_index, Rect { x: dst.x + filled, width, ..dst }));
                filled += width;
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
            let mut items = Vec::new();
            let Some(destination) =
                songlist_destination_at(entries, index, enabled_options, state, self.lr2)
            else {
                return items;
            };
            if let Some(mut resolved) = self.resolve_offset_destination_items(
                destination,
                row_origin,
                images,
                enabled_options,
                state,
                &SkinTextState::default(),
                sources,
            ) {
                items.append(&mut resolved);
            }
            items
        }

        fn select_songlist_text_items(
            &self,
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
            let mut items = Vec::new();
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
            if self.lr2 {
                if let Some(destination) = songlist_destination_at(
                    &songlist.text,
                    usize::from(row.lr2_new_title),
                    enabled_options,
                    state,
                    true,
                ) && let Some(resolved) = self.resolve_offset_destination_items(
                    destination,
                    row_origin,
                    images,
                    enabled_options,
                    state,
                    &text_state,
                    sources,
                ) {
                    items.extend(resolved);
                }
                return items;
            }
            let destinations = destination_entries(&songlist.text, enabled_options);
            let Some(destination) = select_row_slot_with_fallbacks(
                &destinations,
                select_row_bar_text_index(row),
                select_row_bar_text_fallback_indices(row),
            )
            .copied() else {
                return items;
            };
            {
                if let Some(mut resolved) = self.resolve_offset_destination_items(
                    destination,
                    row_origin,
                    images,
                    enabled_options,
                    state,
                    &text_state,
                    sources,
                ) {
                    items.append(&mut resolved);
                }
            }
            items
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_select_graph_methods;
