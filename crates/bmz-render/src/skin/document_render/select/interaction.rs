macro_rules! skin_document_render_select_interaction_methods {
    () => {
        fn select_click_hit(
            &self,
            sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
            settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
            x: f32,
            y: f32,
        ) -> Option<SkinClickHit> {
            self.select_click_hits(sources, snapshot, settings_dest_index)
                .into_iter()
                .rev()
                .find(|hit| rect_contains(hit.rect, x, y))
        }

        fn result_click_hit(&self, state: &SkinDrawState, x: f32, y: f32) -> Option<SkinClickHit> {
            let enabled_options = self.enabled_options();
            let images: SkinImageLookup<'_> = self.image_map().into();
            let destinations = self.all_destinations(&enabled_options);
            destinations
                .into_iter()
                .filter(|destination| {
                    destination_ops_match(destination, &enabled_options, state)
                        && eval_skin_draw_condition(&destination.draw, state)
                })
                .filter_map(|destination| {
                    Some(SkinClickHit {
                        target: self.click_target_for_destination(destination, &images)?,
                        rect: self.destination_click_rect(destination, &enabled_options, state)?,
                    })
                })
                .rev()
                .find(|hit| rect_contains(hit.rect, x, y))
        }

        fn result_slider_hit(
            &self,
            state: &SkinDrawState,
            x: f32,
            y: f32,
        ) -> Option<SkinSliderHit> {
            let enabled_options = self.enabled_options();
            let destinations = self.all_destinations(&enabled_options);
            destinations
                .into_iter()
                .filter(|destination| {
                    destination_ops_match(destination, &enabled_options, state)
                        && eval_skin_draw_condition(&destination.draw, state)
                })
                .filter_map(|destination| {
                    let slider = self.slider.iter().find(|slider| slider.id == destination.id)?;
                    (slider.slider_type == 8).then_some(())?;
                    self.destination_slider_hit(slider, destination, &enabled_options, state, x, y)
                })
                .next_back()
        }

        fn select_slider_hit(
            &self,
            snapshot: &SelectSnapshot,
            settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
            x: f32,
            y: f32,
        ) -> Option<SkinSliderHit> {
            let (state, selected_row) = self.select_draw_state(snapshot, None);
            let enabled_options = self.enabled_options();
            self.all_destinations(&enabled_options)
                .into_iter()
                .filter_map(|destination| {
                    if !crate::select_settings_dest::test_select_destination_visible(
                        settings_dest_index,
                        destination,
                        &enabled_options,
                        &state,
                        snapshot,
                        selected_row,
                        eval_skin_draw_condition,
                        test_skin_ops,
                    ) {
                        return None;
                    }
                    let slider = self.slider.iter().find(|slider| slider.id == destination.id)?;
                    self.destination_slider_hit(slider, destination, &enabled_options, &state, x, y)
                })
                .next_back()
        }

        fn select_click_hits(
            &self,
            _sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
            settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        ) -> Vec<SkinClickHit> {
            let (state, selected_row) = self.select_draw_state(snapshot, None);
            let enabled_options = self.enabled_options();
            let images: SkinImageLookup<'_> = self.image_map().into();
            let mut hits = Vec::new();
            for destination in self.all_destinations(&enabled_options) {
                if destination.id
                    == self.songlist.as_ref().map(|list| list.id.as_str()).unwrap_or("")
                {
                    hits.extend(self.select_songlist_click_hits(
                        snapshot,
                        &enabled_options,
                        &state,
                    ));
                    continue;
                }
                if !crate::select_settings_dest::test_select_destination_visible(
                    settings_dest_index,
                    destination,
                    &enabled_options,
                    &state,
                    snapshot,
                    selected_row,
                    eval_skin_draw_condition,
                    test_skin_ops,
                ) {
                    continue;
                }
                let Some(target) = self.click_target_for_destination(destination, &images) else {
                    continue;
                };
                let Some(rect) = self.destination_click_rect(destination, &enabled_options, &state)
                else {
                    continue;
                };
                hits.push(SkinClickHit { target, rect });
            }
            hits
        }

        fn select_songlist_click_hits(
            &self,
            snapshot: &SelectSnapshot,
            enabled_options: &[i32],
            state: &SkinDrawState,
        ) -> Vec<SkinClickHit> {
            let Some(songlist) = &self.songlist else {
                return Vec::new();
            };
            let selected_row_position =
                select_snapshot_selected_row_position(&snapshot.rows, snapshot.selected_index)
                    as i32;
            let mut hits = Vec::new();
            let mut row_state = state.clone();
            for (row_position, row) in snapshot.rows.iter().enumerate() {
                let offset = row_position as i32 - selected_row_position;
                let slot = songlist.center + offset;
                if !songlist.clickable.contains(&slot) || slot < 0 {
                    continue;
                }
                let selected = row_position as i32 == selected_row_position;
                let row_destinations = if selected { &songlist.liston } else { &songlist.listoff };
                let Some(row_destination) =
                    destination_entry_at(row_destinations, slot as usize, enabled_options)
                else {
                    continue;
                };
                Self::apply_select_songlist_click_row_state(
                    &mut row_state,
                    row,
                    snapshot.selected_replay_slot,
                );
                let elapsed = skin_timer_elapsed_ms(row_destination.timer, state).unwrap_or(0);
                let Some(mut frame) = resolve_destination_frame(
                    row_destination,
                    elapsed,
                    enabled_options,
                    &row_state,
                ) else {
                    continue;
                };
                self.apply_select_songlist_scroll_to_frame(
                    &mut frame,
                    songlist,
                    slot,
                    enabled_options,
                    &row_state,
                    snapshot.bar_scroll_direction,
                    snapshot.bar_scroll_progress,
                );
                apply_skin_offset_to_frame(row_destination, &mut frame, &row_state, false);
                if !destination_mouse_rect_contains(row_destination, frame, &row_state) {
                    continue;
                }
                let rect = normalize_skin_frame_rect(frame, self.w, self.h);
                if rect.width <= 0.0 || rect.height <= 0.0 {
                    continue;
                }
                hits.push(SkinClickHit {
                    target: SkinClickTarget::SelectRow { row_index: row.index },
                    rect,
                });
            }
            hits
        }

        fn apply_select_songlist_render_row_state(
            state: &mut SkinDrawState,
            row: &SelectRowSnapshot,
            selected_replay_slot: Option<u8>,
        ) {
            state.select_play_level = select_row_level_number(row);
            state.play_level = select_row_level_number(row);
            state.table_song = !row.table_text_primary.is_empty();
            state.difficulty = select_row_difficulty_code(row);
            state.judge_rank = row.judge_rank;
            state.select_ex_score = row.ex_score;
            state.select_replay_slots = row.replay_slots;
            state.select_replay_index = select_row_replay_index(row, selected_replay_slot);
            state.select_clear_index = select_row_clear_index(row) as i64;
            state.select_favorite_song = row.favorite_song;
            state.select_favorite_chart = row.favorite_chart;
            state.select_folder_lamp_counts = row.folder_lamp_counts;
            state.select_row_kind = row.kind;
            state.select_course_constraints = row.course_constraints;
            state.select_is_folder = row.is_folder;
            state.select_in_library = row.in_library;
            state.chart_has_long_notes = (!state.in_settings
                && row.kind == SelectRowKind::Song
                && !row.is_folder
                && row.in_library)
                .then_some(row.has_long_notes);
            state.select_total_notes = row.total_notes;
            state.select_chart_normal_notes = row.chart_normal_notes;
            state.select_chart_long_notes = row.chart_long_notes;
            state.select_chart_scratch_notes = row.chart_scratch_notes;
            state.select_chart_long_scratch_notes = row.chart_long_scratch_notes;
            state.select_chart_mine_notes = row.chart_mine_notes;
            state.select_chart_density = row.chart_density;
            state.select_chart_peak_density = row.chart_peak_density;
            state.select_chart_end_density = row.chart_end_density;
            state.select_chart_total_gauge = row.chart_total_gauge;
            state.select_chart_main_bpm = row.chart_main_bpm;
            state.select_bpm = row.initial_bpm;
            state.select_min_bpm = row.min_bpm;
            state.select_max_bpm = row.max_bpm;
            state.min_bpm = row.min_bpm;
            state.max_bpm = row.max_bpm;
            state.main_bpm = row.chart_main_bpm;
            state.select_length_ms = row.length_ms;
            state.select_play_count = row.play_count;
            state.select_clear_count = row.clear_count;
            state.select_bp = row.bp;
            state.select_cb = row.cb;
            state.max_combo = row.max_combo.unwrap_or(0);
            state.total_notes = row.total_notes;
            state.gauge = row.gauge_value.unwrap_or(0.0);
            state.ex_score = row.ex_score.unwrap_or(0);
            state.select_chart_key_mode = row.chart_key_mode;
        }

        fn apply_select_songlist_click_row_state(
            state: &mut SkinDrawState,
            row: &SelectRowSnapshot,
            selected_replay_slot: Option<u8>,
        ) {
            Self::apply_select_songlist_render_row_state(state, row, selected_replay_slot);
        }

        fn click_target_for_destination(
            &self,
            destination: &SkinDestinationDef,
            images: &SkinImageLookup<'_>,
        ) -> Option<SkinClickTarget> {
            if destination.clickable == Some(false) {
                return None;
            }
            if let Some(event_id) = destination.act {
                return Some(SkinClickTarget::Event { event_id, click: destination.click });
            }
            if let Some(image) = images.get(destination.id.as_str())
                && destination.clickable.or(image.clickable).unwrap_or(image.act.is_some())
                && let Some(event_id) = image.act
            {
                return Some(SkinClickTarget::Event { event_id, click: image.click });
            }
            let imageset = self.imageset.iter().find(|set| set.id == destination.id)?;
            destination
                .clickable
                .or(imageset.clickable)
                .unwrap_or(imageset.act.is_some())
                .then_some(imageset.act)
                .flatten()
                .map(|event_id| SkinClickTarget::Event { event_id, click: imageset.click })
        }

        fn destination_click_rect(
            &self,
            destination: &SkinDestinationDef,
            enabled_options: &[i32],
            state: &SkinDrawState,
        ) -> Option<Rect> {
            let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
            let mut frame =
                resolve_destination_frame(destination, elapsed, enabled_options, state)?;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);
            if !destination_mouse_rect_contains(destination, frame, state) {
                return None;
            }
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            if rect.width <= 0.0 || rect.height <= 0.0 { None } else { Some(rect) }
        }

        fn destination_slider_hit(
            &self,
            slider: &SkinSliderDef,
            destination: &SkinDestinationDef,
            enabled_options: &[i32],
            state: &SkinDrawState,
            x: f32,
            y: f32,
        ) -> Option<SkinSliderHit> {
            if !slider.changeable || !matches!(slider.slider_type, 1 | 8 | 17..=19) {
                return None;
            }
            let elapsed = skin_timer_elapsed_ms(destination.timer, state)?;
            let mut frame =
                resolve_destination_frame(destination, elapsed, enabled_options, state)?;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);
            if !destination_mouse_rect_contains(destination, frame, state) {
                return None;
            }
            let mouse_x = x.clamp(0.0, 1.0) * self.w as f32;
            let mouse_y = (1.0 - y.clamp(0.0, 1.0)) * self.h as f32;
            let value = if slider.slider_type == 1 {
                scroll_slider_value_at(slider, frame, mouse_x, mouse_y)?
            } else {
                slider_value_at(slider, frame, mouse_x, mouse_y)?
            };
            Some(SkinSliderHit { slider_type: slider.slider_type, value })
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_select_interaction_methods;
