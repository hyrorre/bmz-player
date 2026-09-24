macro_rules! skin_document_render_select_render_methods {
    () => {
        fn select_render_items(
            &self,
            sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
        ) -> Vec<SkinRenderItem> {
            self.select_render_items_with_dynamic_timers(
                sources,
                snapshot,
                None,
                &crate::select_settings_dest::SelectSettingsDestIndex::default(),
                None,
            )
        }

        fn select_render_items_with_dynamic_timers(
            &self,
            sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
            dynamic_timers: Option<&mut DynamicTimerRuntime>,
            settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
            lua_draw_runtime: Option<Arc<dyn SkinLuaDrawRuntime>>,
        ) -> Vec<SkinRenderItem> {
            self.select_render_items_with_dynamic_timers_cached(
                sources,
                snapshot,
                dynamic_timers,
                settings_dest_index,
                lua_draw_runtime,
                None,
            )
        }

        fn select_render_items_with_dynamic_timers_cached(
            &self,
            sources: &HashMap<String, SkinDocumentTexture>,
            snapshot: &SelectSnapshot,
            dynamic_timers: Option<&mut DynamicTimerRuntime>,
            settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
            lua_draw_runtime: Option<Arc<dyn SkinLuaDrawRuntime>>,
            mut cache: Option<&mut SelectRenderCache>,
        ) -> Vec<SkinRenderItem> {
            let (mut state, selected_row) = self.select_draw_state(snapshot, dynamic_timers);
            let text = SkinTextState {
                player_name: &snapshot.player_name,
                title: select_detail_title(snapshot, selected_row),
                subtitle: select_detail_subtitle(snapshot, selected_row),
                artist: select_detail_artist(snapshot, selected_row),
                genre: select_detail_genre(snapshot, selected_row),
                difficulty_name: if snapshot.in_settings {
                    ""
                } else {
                    selected_row.map(|row| row.difficulty_name.as_str()).unwrap_or_default()
                },
                play_level: selected_row
                    .map(|row| row.level_display_override.as_deref().unwrap_or(&row.play_level))
                    .unwrap_or_default(),
                target: if snapshot.in_settings { "" } else { &snapshot.target },
                select_arrange: &snapshot.arrange,
                select_arrange_2p: &snapshot.arrange_2p,
                select_gauge: &snapshot.gauge,
                select_gauge_auto_shift: &snapshot.gauge_auto_shift,
                select_bottom_shiftable_gauge: &snapshot.bottom_shiftable_gauge,
                select_double_option: &snapshot.double_option,
                select_hs_fix: &snapshot.hs_fix,
                select_assist: &snapshot.assist,
                select_mode: &snapshot.select_mode,
                select_sort: &snapshot.select_sort,
                select_ln_mode: &snapshot.select_ln_mode,
                select_bga: &snapshot.bga,
                select_chart_replication: &snapshot.chart_replication_mode,
                select_judge_timing_auto_adjust: if snapshot.judge_timing_auto_adjust {
                    "ON"
                } else {
                    "OFF"
                },
                current_folder: &snapshot.current_folder,
                table_level: selected_row
                    .map(|row| {
                        if row.table_text_secondary.is_empty() {
                            row.table_level.as_str()
                        } else {
                            row.table_text_secondary.as_str()
                        }
                    })
                    .unwrap_or_default(),
                table_text_primary: selected_row
                    .map(|row| row.table_text_primary.as_str())
                    .unwrap_or_default(),
                table_text_secondary: selected_row
                    .map(|row| row.table_text_secondary.as_str())
                    .unwrap_or_default(),
                table_text_fallback: selected_row
                    .map(|row| row.table_text_fallback.as_str())
                    .unwrap_or_default(),
                course_titles: selected_row
                    .map(|row| string_array_refs(&row.course_titles))
                    .unwrap_or_default(),
                search_word: &snapshot.search_word,
                search_word_alpha: snapshot.search_word_alpha,
                search_caret_byte_index: snapshot.search_caret_byte_index,
                rival: &snapshot.rival_name,
                resolved_target_name: snapshot.resolved_target_name.as_deref(),
                ir_ranking: &snapshot.ir,
                ..SkinTextState::default()
            };

            let planning = cache.as_deref_mut().map(|cache| cache.cached_planning(self));
            let images = planning.as_ref().map_or_else(
                || self.image_map().into(),
                |p| SkinObjectLookup::Indexed { items: &self.image, indices: &p.objects.images },
            );
            let values = planning.as_ref().map_or_else(
                || {
                    SkinObjectLookup::Direct(
                        self.value.iter().map(|v| (v.id.as_str(), v)).collect(),
                    )
                },
                |p| SkinObjectLookup::Indexed { items: &self.value, indices: &p.objects.values },
            );
            let enabled_options_storage =
                planning.is_none().then(|| self.enabled_options()).unwrap_or_default();
            let enabled_options: &[i32] =
                planning.as_ref().map_or(enabled_options_storage.as_slice(), |planning| {
                    planning.enabled_options.as_ref()
                });
            if let Some(runtime) = lua_draw_runtime {
                state.lua_runtime = Some(SkinLuaRuntimeContext {
                    runtime,
                    enabled_options: planning.as_ref().map_or_else(
                        || Arc::from(enabled_options),
                        |planning| Arc::clone(&planning.enabled_options),
                    ),
                    text_values: Arc::new(lua_main_state_text_values(&state, &text)),
                });
            }
            with_lua_render_state(&state, || {
                let destinations = planning
                    .is_none()
                    .then(|| self.all_destinations(enabled_options))
                    .unwrap_or_default();
                let destination_count = planning
                    .as_ref()
                    .map_or(destinations.len(), |planning| planning.destinations.len());
                let search_input_anchors = select::select_search_input_anchors(
                    self,
                    snapshot,
                    settings_dest_index,
                    &state,
                    selected_row,
                    enabled_options,
                );
                let search_input_render_item =
                    |anchor: &select::SelectSearchInputAnchor<'_>| -> Option<SkinRenderItem> {
                        let mut item = self.text_render_item_with_draw_state(
                            anchor.text,
                            anchor.frame,
                            Some(&state),
                            &text,
                        )?;
                        if let SkinRenderItem::Text { style, caret, .. } = &mut item {
                            // SkinTextInput uses the configured system font at the
                            // destination height, independently of the skin text font.
                            style.font_id = None;
                            style.bitmap_size = None;
                            style.size = anchor.frame.h.abs().max(1) as f32 / self.h.max(1) as f32;
                            style.align = TextAlign::Left;
                            if let Some(caret) = caret {
                                caret.color = Color::rgb(1.0, 1.0, 1.0);
                            }
                        }
                        Some(item)
                    };
                let mut items = Vec::new();
                for destination_index in 0..destination_count {
                    let Some(destination) = planning
                        .as_ref()
                        .and_then(|planning| planning.destinations.get(destination_index).copied())
                        .and_then(|destination| destination.resolve(self))
                        .or_else(|| destinations.get(destination_index).copied())
                    else {
                        continue;
                    };
                    if destination.id
                        == self.songlist.as_ref().map(|list| list.id.as_str()).unwrap_or("")
                    {
                        items.extend(self.select_songlist_items(
                            sources,
                            snapshot,
                            &images,
                            enabled_options,
                            &state,
                        ));
                        continue;
                    }
                    // beatoraja keeps STRING_SEARCHWORD empty in the regular SkinText
                    // pass. Outside input mode, place BMZ's placeholder/feedback at
                    // the destination's original z position so later skin objects
                    // (notably option panels) can cover it. While editing, the input
                    // and caret remain a separate TextField-like overlay after the skin.
                    if self.text.iter().any(|text| text.ref_id == 30 && text.id == destination.id) {
                        if !snapshot.search_input_active
                            && let Some(anchor) = search_input_anchors
                                .iter()
                                .find(|anchor| std::ptr::eq(anchor.destination, destination))
                            && let Some(item) = search_input_render_item(anchor)
                        {
                            items.push(item);
                        }
                        continue;
                    }
                    if !crate::select_settings_dest::test_select_destination_visible(
                        settings_dest_index,
                        destination,
                        enabled_options,
                        &state,
                        snapshot,
                        selected_row,
                        eval_skin_draw_condition,
                        |ops, enabled_options, state| {
                            if ops.len() == destination.op.len()
                                && ops.iter().eq(destination.op.iter())
                            {
                                destination_ops_match(destination, enabled_options, state)
                            } else {
                                test_skin_ops(ops, enabled_options, state)
                            }
                        },
                    ) {
                        continue;
                    }
                    if let (Some(row), Some(judge_graph)) = (
                        selected_row.filter(|row| select_row_shows_score_decorations(row)),
                        self.judgegraph.iter().find(|graph| graph.id == destination.id),
                    ) {
                        items.extend(self.select_note_distribution_graph_render_items(
                            row,
                            judge_graph,
                            destination,
                            (0, 0),
                            enabled_options,
                            &state,
                        ));
                        continue;
                    }
                    if let (Some(row), Some(bpm_graph)) = (
                        selected_row.filter(|row| select_row_shows_score_decorations(row)),
                        self.bpmgraph.iter().find(|graph| graph.id == destination.id),
                    ) {
                        let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, &state) else {
                            continue;
                        };
                        let Some(mut frame) = resolve_destination_frame(
                            destination,
                            elapsed,
                            enabled_options,
                            &state,
                        ) else {
                            continue;
                        };
                        apply_skin_offset_to_frame(destination, &mut frame, &state, false);
                        if !destination_mouse_rect_contains(destination, frame, &state) {
                            continue;
                        }
                        items.extend(self.bpmgraph_render_items_with_segments(
                            bpm_graph,
                            destination,
                            frame,
                            &state,
                            &row.chart_bpm_graph_segments,
                        ));
                        continue;
                    }
                    if core::static_image_destination_cacheable(self, destination, &images)
                        && let Some(cache) = cache.as_deref_mut()
                    {
                        let cached = cache.cached_static_image_items(destination_index, || {
                            Arc::from(
                                self.resolve_destination_items(
                                    destination_index,
                                    destination,
                                    DestinationResolveContext {
                                        images: &images,
                                        values: &values,
                                        enabled_options,
                                        state: &state,
                                        text_state: &text,
                                        sources,
                                        runtime_graphs: SkinRuntimeGraphs::from_document(self),
                                        cache: None,
                                    },
                                )
                                .unwrap_or_default(),
                            )
                        });
                        items.extend(cached.iter().cloned());
                        continue;
                    }
                    if let Some(resolved) = self.resolve_destination_items(
                        destination_index,
                        destination,
                        DestinationResolveContext {
                            images: &images,
                            values: &values,
                            enabled_options,
                            state: &state,
                            text_state: &text,
                            sources,
                            runtime_graphs: SkinRuntimeGraphs::from_document(self),
                            cache: None,
                        },
                    ) {
                        items.extend(resolved);
                    }
                }
                if snapshot.search_input_active {
                    for anchor in &search_input_anchors {
                        if let Some(item) = search_input_render_item(anchor) {
                            items.push(item);
                        }
                    }
                }
                items
            })
        }

        fn select_search_input_rect(
            &self,
            snapshot: &SelectSnapshot,
            settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
        ) -> Option<Rect> {
            let (state, selected_row) = self.select_draw_state(snapshot, None);
            let enabled_options = self.enabled_options();
            let anchor = select::select_search_input_anchors(
                self,
                snapshot,
                settings_dest_index,
                &state,
                selected_row,
                &enabled_options,
            )
            .into_iter()
            .next_back()?;
            Some(normalize_skin_frame_rect(anchor.frame, self.w, self.h))
        }

        fn select_draw_state<'a>(
            &self,
            snapshot: &'a SelectSnapshot,
            mut dynamic_timers: Option<&mut DynamicTimerRuntime>,
        ) -> (SkinDrawState, Option<&'a SelectRowSnapshot>) {
            let selected_row =
                snapshot.rows.iter().find(|row| row.index == snapshot.selected_index);
            let selected_chart_has_long_notes = selected_row
                .filter(|row| {
                    !snapshot.in_settings
                        && row.kind == SelectRowKind::Song
                        && !row.is_folder
                        && row.in_library
                })
                .map(|row| row.has_long_notes);
            let selected_chart = selected_row.filter(|row| {
                !snapshot.in_settings
                    && row.kind == SelectRowKind::Song
                    && !row.is_folder
                    && row.in_library
            });
            let mut skin_attempt = snapshot.skin_attempt;
            if let Some(row) = selected_chart {
                skin_attempt.source_key_mode = skin_attempt.source_key_mode.or(row.chart_key_mode);
                skin_attempt.effective_key_mode =
                    skin_attempt.effective_key_mode.or(row.chart_key_mode);
                skin_attempt.source_ln_profile_bits =
                    skin_attempt.source_ln_profile_bits.or(row.source_ln_profile_bits);
                skin_attempt.has_bga = skin_attempt.has_bga.or(Some(row.has_bga));
                skin_attempt.has_random_sequence =
                    skin_attempt.has_random_sequence.or(Some(row.has_random));
            }
            let mouse_position = snapshot.mouse_position.map(|(x, y)| {
                (x.clamp(0.0, 1.0) * self.w as f32, (1.0 - y.clamp(0.0, 1.0)) * self.h as f32)
            });
            let duration_ms = snapshot.note_display_duration_ms;
            let duration_green_ms = duration_ms.map(duration_to_green_number_ms);
            let elapsed_ms =
                (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            let start_input_ms = dynamic_timers.as_deref_mut().map_or_else(
                || skin_start_input_elapsed_ms(elapsed_ms, self.input),
                |runtime| runtime.start_input_elapsed_ms(elapsed_ms, self.input),
            );
            let mut state = SkinDrawState {
                elapsed_ms,
                start_input_ms,
                current_fps: snapshot.current_fps,
                operating_time_ms: snapshot.operating_time_ms,
                logical_input_held: snapshot.skin_input.held,
                skin_offsets: snapshot.skin_offsets,
                select_bar_elapsed_ms: (snapshot.selection_time.0 / 1_000)
                    .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
                select_option_panel_elapsed_ms: (snapshot.option_panel_time.0 / 1_000)
                    .clamp(i32::MIN as i64, i32::MAX as i64)
                    as i32,
                select_option_panel_off_elapsed_ms: snapshot.option_panel_off_times.map(
                    |elapsed| {
                        elapsed.map(|elapsed| {
                            (elapsed.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
                        })
                    },
                ),
                select_option_panel: snapshot.option_panel,
                skin_attempt,
                select_arrange_index: select_arrange_index(&snapshot.arrange),
                select_arrange_2p_index: select_arrange_index(&snapshot.arrange_2p),
                select_extended_arrange_index: extended_arrange_index(&snapshot.arrange),
                select_extended_arrange_2p_index: extended_arrange_index(&snapshot.arrange_2p),
                select_double_option_index: select_double_option_index(&snapshot.double_option),
                select_hs_fix_index: select_hs_fix_index(&snapshot.hs_fix),
                select_gauge_index: select_gauge_index(&snapshot.gauge),
                select_gauge_auto_shift_index: select_gauge_auto_shift_index(
                    &snapshot.gauge_auto_shift,
                ),
                select_bottom_shiftable_gauge_index: select_bottom_shiftable_gauge_index(
                    &snapshot.bottom_shiftable_gauge,
                ),
                select_target_index: play_target_image_index(&snapshot.target),
                select_bga_index: select_bga_index(&snapshot.bga),
                judge_timing_offset_ms: snapshot.judge_timing_offset_ms,
                judge_timing_auto_adjust: snapshot.judge_timing_auto_adjust,
                lanecover_enabled: snapshot.lanecover_enabled,
                lift_enabled: snapshot.lift_enabled,
                hidden_enabled: snapshot.hidden_enabled,
                hispeed_auto_adjust: snapshot.hispeed_auto_adjust,
                player_stats: snapshot.player_stats.clone(),
                score_date_sec: snapshot.selected_score_date_sec,
                select_assist_index: select_assist_index(&snapshot.assist),
                assist_flags: snapshot.assist_flags,
                assist_extra_note_depth: snapshot.assist_extra_note_depth,
                assist_mine_mode: snapshot.assist_mine_mode,
                assist_scroll_mode: snapshot.assist_scroll_mode,
                assist_long_note_mode: snapshot.assist_long_note_mode,
                guide_se_enabled: snapshot.guide_se_enabled,
                constant_enabled: snapshot.constant_enabled,
                select_session_mode_index: select_session_mode_index(&snapshot.assist),
                select_mode_index: select_mode_index(&snapshot.select_mode),
                select_difficulty_filter_index: snapshot.select_difficulty_filter as usize,
                random_mix_options: snapshot.random_mix_options,
                select_sort_index: select_sort_index(&snapshot.select_sort),
                select_ln_mode_index: select_ln_mode_index(&snapshot.select_ln_mode),
                rule_mode_index: snapshot.rule_mode_index,
                ln_policy_setting_index: Some(snapshot.ln_policy_setting_index),
                ln_score_policy_index: snapshot.ln_score_policy_index,
                select_judge_algorithm_index: select_judge_algorithm_index(
                    &snapshot.judge_algorithm,
                ),
                hispeed: snapshot.hispeed,
                hispeed_mode_index: snapshot.hispeed_mode_index,
                base_hispeed_index: snapshot.base_hispeed_index,
                normal_hispeed_level: snapshot.normal_hispeed_level,
                hispeed_config_index: snapshot.hispeed_config_index,
                total_duration_ms: duration_ms.unwrap_or(0),
                duration_green_ms,
                select_scroll_progress: select_scroll_progress(snapshot),
                select_master_volume: snapshot.master_volume,
                select_key_volume: snapshot.key_volume,
                select_bgm_volume: snapshot.bgm_volume,
                select_has_banner: snapshot.banner_image,
                select_has_document: selected_row.is_some_and(|row| row.has_document),
                chart_has_long_notes: selected_chart_has_long_notes,
                has_stagefile: snapshot.stage_background,
                has_backbmp: snapshot.backbmp_image,
                select_folder_song_count: selected_row.and_then(select_row_folder_song_count),
                select_screen: true,
                select_play_level: selected_row.map(select_row_level_number).unwrap_or(0),
                play_level: selected_row.map(select_row_level_number).unwrap_or(0),
                table_song: selected_row.is_some_and(|row| !row.table_text_primary.is_empty()),
                min_bpm: selected_row.map(|row| row.min_bpm).unwrap_or(0.0),
                max_bpm: selected_row.map(|row| row.max_bpm).unwrap_or(0.0),
                has_bpm_stop: selected_row
                    .map(|row| row.chart_bpm_graph_segments.iter().any(|s| s.is_stop))
                    .unwrap_or(false),
                main_bpm: selected_row.map(|row| row.chart_main_bpm).unwrap_or(0.0),
                difficulty: selected_row.map(select_row_difficulty_code).unwrap_or(0),
                judge_rank: selected_row.and_then(|row| row.judge_rank),
                select_ex_score: selected_row.and_then(|row| row.ex_score),
                select_replay_slots: selected_row.map(|row| row.replay_slots).unwrap_or([false; 4]),
                select_replay_index: selected_row
                    .and_then(|row| select_row_replay_index(row, snapshot.selected_replay_slot)),
                select_clear_index: selected_row.map(select_row_clear_index).unwrap_or(0) as i64,
                select_favorite_song: selected_row.is_some_and(|row| row.favorite_song),
                select_favorite_chart: selected_row.is_some_and(|row| row.favorite_chart),
                select_replay_slot_rule_indices: snapshot.replay_slot_rule_indices,
                select_folder_lamp_counts: selected_row
                    .map(|row| row.folder_lamp_counts)
                    .unwrap_or([0; 11]),
                select_row_kind: selected_row.map(|row| row.kind).unwrap_or(SelectRowKind::Song),
                select_course_constraints: selected_row
                    .map(|row| row.course_constraints)
                    .unwrap_or_default(),
                select_is_folder: selected_row.is_some_and(|row| row.is_folder),
                select_in_library: selected_row.is_none_or(|row| row.in_library),
                select_total_notes: selected_row.map(|row| row.total_notes).unwrap_or(0),
                select_chart_normal_notes: selected_row
                    .map(|row| row.chart_normal_notes)
                    .unwrap_or(0),
                select_chart_long_notes: selected_row.map(|row| row.chart_long_notes).unwrap_or(0),
                select_chart_scratch_notes: selected_row
                    .map(|row| row.chart_scratch_notes)
                    .unwrap_or(0),
                select_chart_long_scratch_notes: selected_row
                    .map(|row| row.chart_long_scratch_notes)
                    .unwrap_or(0),
                select_chart_mine_notes: selected_row.map(|row| row.chart_mine_notes).unwrap_or(0),
                select_chart_density: selected_row.map(|row| row.chart_density).unwrap_or(0.0),
                select_chart_peak_density: selected_row
                    .map(|row| row.chart_peak_density)
                    .unwrap_or(0.0),
                select_chart_end_density: selected_row
                    .map(|row| row.chart_end_density)
                    .unwrap_or(0.0),
                select_chart_total_gauge: selected_row
                    .map(|row| row.chart_total_gauge)
                    .unwrap_or(0.0),
                select_chart_main_bpm: selected_row.map(|row| row.chart_main_bpm).unwrap_or(0.0),
                select_bpm: selected_row.map(|row| row.initial_bpm).unwrap_or(0.0),
                select_min_bpm: selected_row.map(|row| row.min_bpm).unwrap_or(0.0),
                select_max_bpm: selected_row.map(|row| row.max_bpm).unwrap_or(0.0),
                select_length_ms: selected_row.map(|row| row.length_ms).unwrap_or(0),
                select_play_count: selected_row.map(|row| row.play_count).unwrap_or(0),
                select_clear_count: selected_row.map(|row| row.clear_count).unwrap_or(0),
                select_bp: selected_row.and_then(|row| row.bp),
                select_cb: selected_row.and_then(|row| row.cb),
                judge_counts: selected_row.map(|row| row.judge_counts).unwrap_or_default(),
                fast_slow_counts: selected_row.and_then(|row| row.fast_slow_counts),
                max_combo: selected_row.and_then(|row| row.max_combo).unwrap_or(0),
                total_notes: selected_row.map(|row| row.total_notes).unwrap_or(0),
                past_notes: selected_row.map(|row| row.total_notes).unwrap_or(0),
                gauge: selected_row.and_then(|row| row.gauge_value).unwrap_or(0.0),
                gauge_auto_shift: snapshot.gauge_auto_shift != "OFF",
                ex_score: selected_row.and_then(|row| row.ex_score).unwrap_or(0),
                target_ex_score: snapshot.rival.as_ref().map(|rival| rival.ex_score),
                target_clear_index: snapshot.rival.as_ref().map(|rival| rival.clear_index),
                in_settings: snapshot.in_settings,
                settings_editing: snapshot.settings_editing,
                select_chart_key_mode: selected_row.and_then(|row| row.chart_key_mode),
                random_lane_refs: selected_row
                    .and_then(|row| row.chart_key_mode)
                    .map_or([0; SKIN_RANDOM_LANE_REF_COUNT], |key_mode| {
                        random_lane_refs(&snapshot.lane_shuffle_pattern, key_mode)
                    }),
                mouse_x: mouse_position.map(|position| position.0),
                mouse_y: mouse_position.map(|position| position.1),
                ir_ranking: snapshot.ir.clone(),
                rival_selected: snapshot.rival_selected,
                rival_ex_score: snapshot.rival.as_ref().map(|rival| i64::from(rival.ex_score)),
                rival_max_combo: snapshot.rival.as_ref().map(|rival| i64::from(rival.max_combo)),
                rival_bp: snapshot.rival.as_ref().map(|rival| i64::from(rival.bp)),
                rival_judge_counts: snapshot.rival.as_ref().and_then(|rival| {
                    rival.judge_counts.map(|counts| {
                        [counts.pgreat, counts.great, counts.good, counts.bad, counts.poor]
                    })
                }),
                ..SkinDrawState::default()
            };
            if let Some(runtime) = dynamic_timers {
                let now_ms = state.elapsed_ms;
                runtime.advance(self, &mut state, now_ms);
            }
            (state, selected_row)
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_select_render_methods;
