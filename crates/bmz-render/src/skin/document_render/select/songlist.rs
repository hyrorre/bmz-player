macro_rules! skin_document_render_select_songlist_methods {
    () => {
        fn select_songlist_items(
            &self,
            sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &SkinDrawState,
        ) -> Vec<SkinRenderItem> {
            let Some(songlist) = &self.songlist else {
                return Vec::new();
            };
            let mut items = Vec::new();
            let selected_row_position =
                select_snapshot_selected_row_position(&snapshot.rows, snapshot.selected_index)
                    as i32;
            let mut row_state = state.clone();
            for (row_position, row) in snapshot.rows.iter().enumerate() {
                let offset = row_position as i32 - selected_row_position;
                let slot = songlist.center + offset;
                if slot < 0 {
                    continue;
                }
                let selected = row_position as i32 == selected_row_position;
                let row_destinations = if selected { &songlist.liston } else { &songlist.listoff };
                let Some(row_destination) =
                    destination_entry_at(row_destinations, slot as usize, enabled_options)
                else {
                    continue;
                };
                Self::apply_select_songlist_render_row_state(
                    &mut row_state,
                    row,
                    snapshot.selected_replay_slot,
                );
                let elapsed = skin_timer_elapsed_ms(row_destination.timer, state).unwrap_or(0);
                let Some(mut row_frame) = resolve_destination_frame(
                    row_destination,
                    elapsed,
                    enabled_options,
                    &row_state,
                ) else {
                    continue;
                };
                self.apply_select_songlist_scroll_to_frame(
                    &mut row_frame,
                    songlist,
                    slot,
                    enabled_options,
                    &row_state,
                    snapshot.bar_scroll_direction,
                    snapshot.bar_scroll_progress,
                );
                let row_origin = (row_frame.x, row_frame.y);
                apply_skin_offset_to_frame(row_destination, &mut row_frame, state, false);
                if let Some(item) = self.select_bar_item(row, row_destination, row_frame, sources) {
                    items.push(item);
                }
                if select_row_shows_lamp(row) {
                    let clear_index = select_row_clear_index(row);
                    if snapshot.rival_selected {
                        items.extend(self.select_songlist_child_items_by_index(
                            &songlist.playerlamp,
                            clear_index,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        ));
                        items.extend(self.select_songlist_child_items_by_index(
                            &songlist.rivallamp,
                            row.rival_clear_index,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        ));
                    } else {
                        items.extend(self.select_songlist_child_items_by_index(
                            &songlist.lamp,
                            clear_index,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        ));
                    }
                }
                if select_row_shows_score_decorations(row) {
                    if select_row_shows_level(row) {
                        items.extend(self.select_songlist_level_items(
                            &songlist.level,
                            row,
                            row_origin,
                            images,
                            enabled_options,
                            &mut row_state,
                            sources,
                        ));
                    }
                    for label_index in select_row_label_indices(row) {
                        items.extend(self.select_songlist_child_items_by_index(
                            &songlist.label,
                            label_index,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        ));
                    }
                    if select_row_shows_course_trophy(row)
                        && let Some(trophy_index) = select_row_trophy_index(row)
                    {
                        items.extend(self.select_songlist_child_items_by_index(
                            &songlist.trophy,
                            trophy_index,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        ));
                    }
                    items.extend(self.select_songlist_all_child_items(
                        &songlist.judgegraph,
                        row,
                        row_origin,
                        images,
                        enabled_options,
                        &row_state,
                        sources,
                    ));
                    items.extend(self.select_songlist_all_child_items(
                        &songlist.bpmgraph,
                        row,
                        row_origin,
                        images,
                        enabled_options,
                        &row_state,
                        sources,
                    ));
                }
                if select_row_shows_folder_distribution(row) {
                    items.extend(self.select_songlist_all_child_items(
                        &songlist.graph,
                        row,
                        row_origin,
                        images,
                        enabled_options,
                        &row_state,
                        sources,
                    ));
                }
                items.extend(self.select_songlist_text_items(
                    row,
                    row_origin,
                    images,
                    enabled_options,
                    &row_state,
                    sources,
                ));
            }
            items
        }

        fn apply_select_songlist_scroll_to_frame(
            &self,
            frame: &mut ResolvedSkinFrame,
            songlist: &SkinSongListDef,
            slot: i32,
            enabled_options: &[i32],
            state: &SkinDrawState,
            direction: i32,
            progress: f32,
        ) {
            let direction = direction.signum();
            let progress = progress.clamp(0.0, 1.0);
            if direction == 0 || progress <= 0.0 {
                return;
            }
            let next_slot = slot + direction;
            if next_slot < 0 {
                return;
            }
            let next_selected = next_slot == songlist.center;
            let next_destinations =
                if next_selected { &songlist.liston } else { &songlist.listoff };
            let Some(next_destination) =
                destination_entry_at(next_destinations, next_slot as usize, enabled_options)
            else {
                return;
            };
            let elapsed = skin_timer_elapsed_ms(next_destination.timer, state).unwrap_or(0);
            let Some(next_frame) =
                resolve_destination_frame(next_destination, elapsed, enabled_options, state)
            else {
                return;
            };
            frame.x += ((next_frame.x - frame.x) as f32 * progress).round() as i32;
            frame.y += ((next_frame.y - frame.y) as f32 * progress).round() as i32;
        }

        fn select_songlist_all_child_items(
            &self,
            entries: &[DestinationListEntry],
            row: &SelectRowSnapshot,
            row_origin: (i32, i32),
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            let mut items = Vec::new();
            for destination in destination_entries(entries, enabled_options) {
                if let Some(judge_graph) =
                    self.judgegraph.iter().find(|graph| graph.id == destination.id)
                {
                    items.extend(self.select_note_distribution_graph_render_items(
                        row,
                        judge_graph,
                        destination,
                        row_origin,
                        enabled_options,
                        state,
                    ));
                    continue;
                }
                if let Some(bpm_graph) =
                    self.bpmgraph.iter().find(|graph| graph.id == destination.id)
                {
                    items.extend(self.select_bpmgraph_row_render_items(
                        row,
                        bpm_graph,
                        destination,
                        row_origin,
                        enabled_options,
                        state,
                    ));
                    continue;
                }
                if select_row_shows_folder_distribution(row)
                    && let Some(graph) = self.graph.iter().find(|graph| graph.id == destination.id)
                {
                    items.extend(self.select_folder_distribution_graph_render_items(
                        row,
                        graph,
                        destination,
                        row_origin,
                        enabled_options,
                        state,
                        sources,
                    ));
                    continue;
                }
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
            }
            items
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_select_songlist_methods;
