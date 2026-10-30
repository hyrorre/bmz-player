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
            self.select_songlist_part_items(sources, snapshot, images, enabled_options, state)
                .into_iter()
                .map(|(_, item)| item)
                .collect()
        }

        /// Songlist items in row order, each tagged with the entry list and index
        /// that drew it, so LR2 can place every bar part at its own DST position.
        fn select_songlist_part_items(
            &self,
            sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
            images: &SkinImageLookup<'_>,
            enabled_options: &[i32],
            state: &SkinDrawState,
        ) -> Vec<(SongListPartKey, SkinRenderItem)> {
            let Some(songlist) = &self.songlist else {
                return Vec::new();
            };
            let mut items = Vec::new();
            let tag = |part: &'static str, index: usize, row_state: &SkinDrawState| {
                let entries = match part {
                    "listoff" => Some(&songlist.listoff),
                    "liston" => Some(&songlist.liston),
                    "text" => Some(&songlist.text),
                    "level" => Some(&songlist.level),
                    "lamp" => Some(&songlist.lamp),
                    "playerlamp" => Some(&songlist.playerlamp),
                    "rivallamp" => Some(&songlist.rivallamp),
                    "lr2rival" => Some(&songlist.lr2rival),
                    "flash" => Some(&songlist.flash),
                    _ => None,
                };
                let order = self
                    .lr2
                    .then(|| {
                        entries
                            .and_then(|entries| {
                                songlist_destination_at(
                                    entries,
                                    index,
                                    enabled_options,
                                    row_state,
                                    true,
                                )
                            })
                            .and_then(|destination| destination.lr2_bar_part.as_ref())
                            .and_then(|part| part.order)
                    })
                    .flatten();
                move |item: SkinRenderItem| (SongListPartKey { part, index, order }, item)
            };
            let (rows, center_row_position) = snapshot.bar_rows_and_center();
            let (whole_scroll, scroll_direction, scroll_fraction) = snapshot.bar_scroll_slots();
            let mut row_state = state.clone();
            for (row_position, row) in rows.iter().enumerate() {
                let offset = row_position as i32 - center_row_position as i32;
                let slot = songlist.center + offset + whole_scroll;
                if slot < 0 {
                    continue;
                }
                let selected = slot == songlist.center;
                let row_destinations = if selected { &songlist.liston } else { &songlist.listoff };
                Self::apply_select_songlist_render_row_state(
                    &mut row_state,
                    row,
                    snapshot.selected_replay_slot,
                );
                let Some(row_destination) = songlist_destination_at(
                    row_destinations,
                    slot as usize,
                    enabled_options,
                    &row_state,
                    self.lr2,
                ) else {
                    continue;
                };
                let elapsed = skin_timer_elapsed_ms(row_destination.timer, state).unwrap_or(0);
                let Some(mut row_frame) = resolve_destination_frame(
                    row_destination,
                    elapsed,
                    enabled_options,
                    &row_state,
                ) else {
                    continue;
                };
                // The selection flash marks the fixed center slot. Moving it with
                // a row would make it jump when the visual window crosses a row.
                let flash_origin = (
                    row_frame.x,
                    row_frame.y + if songlist.lr2_bottom_origin { row_frame.h } else { 0 },
                );
                self.apply_select_songlist_scroll_to_frame(
                    &mut row_frame,
                    songlist,
                    slot,
                    enabled_options,
                    &row_state,
                    scroll_direction,
                    scroll_fraction,
                );
                let row_origin = (
                    row_frame.x,
                    row_frame.y + if songlist.lr2_bottom_origin { row_frame.h } else { 0 },
                );
                apply_skin_offset_to_frame(row_destination, &mut row_frame, state, false);
                if let Some(item) =
                    self.select_bar_item(row, row_destination, row_frame, sources, state)
                {
                    let part = if selected { "liston" } else { "listoff" };
                    if row_state.lua_runtime.is_some() {
                        // A Lua condition can change state when selecting the
                        // tag. Keep that evaluation and its original order.
                        items.push(tag(part, slot as usize, &row_state)(item));
                    } else {
                        items.push((
                            SongListPartKey {
                                part,
                                index: slot as usize,
                                order: self
                                    .lr2
                                    .then(|| {
                                        row_destination
                                            .lr2_bar_part
                                            .as_ref()
                                            .and_then(|part| part.order)
                                    })
                                    .flatten(),
                            },
                            item,
                        ));
                    }
                }
                if selected {
                    items.extend(
                        self.select_songlist_child_items_by_index(
                            &songlist.flash,
                            0,
                            flash_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        )
                        .into_iter()
                        .map(tag("flash", 0, &row_state)),
                    );
                }
                if select_row_shows_lamp(row) {
                    let clear_index = select_row_clear_index(row);
                    if snapshot.rival_selected
                        && (!self.lr2 || row.rival_ex_score.is_some_and(|ex| ex > 0))
                    {
                        items.extend(
                            self.select_songlist_child_items_by_index(
                                &songlist.playerlamp,
                                clear_index,
                                row_origin,
                                images,
                                enabled_options,
                                &row_state,
                                sources,
                            )
                            .into_iter()
                            .map(tag("playerlamp", clear_index, &row_state)),
                        );
                        items.extend(
                            self.select_songlist_child_items_by_index(
                                &songlist.rivallamp,
                                row.rival_clear_index,
                                row_origin,
                                images,
                                enabled_options,
                                &row_state,
                                sources,
                            )
                            .into_iter()
                            .map(tag(
                                "rivallamp",
                                row.rival_clear_index,
                                &row_state,
                            )),
                        );
                    } else {
                        items.extend(
                            self.select_songlist_child_items_by_index(
                                &songlist.lamp,
                                clear_index,
                                row_origin,
                                images,
                                enabled_options,
                                &row_state,
                                sources,
                            )
                            .into_iter()
                            .map(tag("lamp", clear_index, &row_state)),
                        );
                    }
                }
                if self.lr2
                    && let Some(rival_ex) = row.rival_ex_score.filter(|ex| *ex > 0)
                {
                    let player_ex = row.ex_score.unwrap_or(0);
                    let index = if player_ex == 0 {
                        3
                    } else {
                        match player_ex.cmp(&rival_ex) {
                            std::cmp::Ordering::Greater => 0,
                            std::cmp::Ordering::Less => 1,
                            std::cmp::Ordering::Equal => 2,
                        }
                    };
                    items.extend(
                        self.select_songlist_child_items_by_index(
                            &songlist.lr2rival,
                            index,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        )
                        .into_iter()
                        .map(tag("lr2rival", index, &row_state)),
                    );
                }
                if select_row_shows_score_decorations(row) {
                    if select_row_shows_level(row) {
                        let level_index =
                            select_row_difficulty_code(row).clamp(0, i64::MAX) as usize;
                        items.extend(
                            self.select_songlist_level_items(
                                &songlist.level,
                                row,
                                row_origin,
                                images,
                                enabled_options,
                                &mut row_state,
                                sources,
                            )
                            .into_iter()
                            .map(tag("level", level_index, &row_state)),
                        );
                    }
                    for mut label_index in
                        select_row_label_indices(row, snapshot.ln_policy_setting_index % 3)
                    {
                        if label_index >= 3
                            && !songlist.label.get(label_index).is_some_and(|entry| match entry {
                                DestinationListEntry::Single(destination) => images
                                    .get(&destination.id)
                                    .is_some_and(|image| sources.contains_key(&image.src)),
                                DestinationListEntry::Conditional { destinations, .. } => {
                                    destinations.iter().any(|destination| {
                                        images
                                            .get(&destination.id)
                                            .is_some_and(|image| sources.contains_key(&image.src))
                                    })
                                }
                            })
                        {
                            label_index = 0;
                        }
                        items.extend(
                            self.select_songlist_child_items_by_index(
                                &songlist.label,
                                label_index,
                                row_origin,
                                images,
                                enabled_options,
                                &row_state,
                                sources,
                            )
                            .into_iter()
                            .map(tag("label", label_index, &row_state)),
                        );
                    }
                    if select_row_shows_course_trophy(row)
                        && let Some(trophy_index) = select_row_trophy_index(row)
                    {
                        items.extend(
                            self.select_songlist_child_items_by_index(
                                &songlist.trophy,
                                trophy_index,
                                row_origin,
                                images,
                                enabled_options,
                                &row_state,
                                sources,
                            )
                            .into_iter()
                            .map(tag("trophy", trophy_index, &row_state)),
                        );
                    }
                    items.extend(
                        self.select_songlist_all_child_items(
                            &songlist.judgegraph,
                            row,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        )
                        .into_iter()
                        .map(tag("judgegraph", usize::MAX, &row_state)),
                    );
                    items.extend(
                        self.select_songlist_all_child_items(
                            &songlist.bpmgraph,
                            row,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        )
                        .into_iter()
                        .map(tag("bpmgraph", usize::MAX, &row_state)),
                    );
                }
                if select_row_shows_folder_distribution(row) || row.download_progress.is_some() {
                    items.extend(
                        self.select_songlist_all_child_items(
                            &songlist.graph,
                            row,
                            row_origin,
                            images,
                            enabled_options,
                            &row_state,
                            sources,
                        )
                        .into_iter()
                        .map(tag("graph", usize::MAX, &row_state)),
                    );
                }
                items.extend(
                    self.select_songlist_text_items(
                        row,
                        row_origin,
                        images,
                        enabled_options,
                        &row_state,
                        sources,
                    )
                    .into_iter()
                    .map(tag("text", usize::from(row.lr2_new_title), &row_state)),
                );
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
            let Some(next_destination) = songlist_destination_at(
                next_destinations,
                next_slot as usize,
                enabled_options,
                state,
                self.lr2,
            ) else {
                return;
            };
            let elapsed = skin_timer_elapsed_ms(next_destination.timer, state).unwrap_or(0);
            let Some(next_frame) =
                resolve_destination_frame(next_destination, elapsed, enabled_options, state)
            else {
                return;
            };
            let interpolate = |from: i32, to: i32| {
                (f64::from(from) + (f64::from(to) - f64::from(from)) * f64::from(progress)).round()
                    as i32
            };
            frame.x = interpolate(frame.x, next_frame.x);
            frame.y = interpolate(frame.y, next_frame.y);
            frame.w = interpolate(frame.w, next_frame.w);
            frame.h = interpolate(frame.h, next_frame.h);
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
                        false,
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
                if (select_row_shows_folder_distribution(row) || row.download_progress.is_some())
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
