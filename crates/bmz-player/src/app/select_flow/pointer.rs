use super::*;

impl WinitApp {
    pub(super) fn route_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if self.jobs.profile_change.is_some() {
            return;
        }
        if self.route_viewer_mouse_wheel(delta) {
            return;
        }
        if self.viewer_waiting {
            return;
        }
        if let Some(change) = lane_cover_wheel_change(delta)
            && (self.play.active_play.is_some() || self.play.pending_play_start.is_some())
        {
            self.apply_play_lane_action(PlayLaneAction::LaneCoverDelta(lane_cover_change_step(
                change,
            )));
            return;
        }
        if matches!(self.view_state(), AppViewState::Result) {
            if let Some(select_move) = select_wheel_move(delta) {
                let rows = match select_move {
                    SelectMove::Previous => -1,
                    SelectMove::Next => 1,
                    _ => 0,
                };
                self.scroll_result_ir_rows(rows);
            }
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            return;
        }
        if in_settings_stack(&self.select.folder_stack) && self.select.settings_edit.is_some() {
            let direction = settings_edit_direction_from_mouse_wheel(delta);
            if direction != 0 {
                self.adjust_settings_edit(direction);
            }
            return;
        }
        if let Some(select_move) = select_wheel_move(delta) {
            self.move_selection(select_move);
        }
    }

    pub(super) fn route_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if self.jobs.profile_change.is_some() {
            return;
        }
        if self.viewer_waiting {
            self.select.select_slider_dragging_type = None;
            return;
        }
        if state == ElementState::Released {
            self.select.select_slider_dragging_type = None;
            return;
        }
        if state != ElementState::Pressed {
            return;
        }
        let Some((x, y)) = self.cursor_position_normalized() else {
            return;
        };
        if matches!(self.view_state(), AppViewState::Result) {
            self.select.select_slider_dragging_type = None;
            if button == MouseButton::Left && self.result.result_exit.is_none() {
                let AppSceneSnapshot::Result(snapshot) = self.scene_snapshot() else {
                    return;
                };
                if let Some(hit) = self.renderer.result_skin_slider_hit(&snapshot, x, y) {
                    self.select.select_slider_dragging_type = Some(hit.slider_type);
                    self.apply_result_slider_hit(hit);
                    return;
                }
                self.handle_result_skin_click(x, y);
            }
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            self.select.select_slider_dragging_type = None;
            return;
        }
        if button == MouseButton::Left
            && !in_settings_stack(&self.select.folder_stack)
            && self.select_search_word_hit(x, y)
        {
            if !self.select.search.is_active() {
                self.set_search_mode(true);
                tracing::info!("entered song search mode from mouse click");
            } else {
                self.search_cursor_to_end();
            }
            return;
        }
        let snapshot = self.select_snapshot();
        if button == MouseButton::Left
            && let Some(hit) = self.renderer.select_skin_slider_hit(&snapshot, x, y)
        {
            self.select.select_slider_dragging_type = Some(hit.slider_type);
            self.apply_select_slider_hit(hit);
            return;
        }
        let Some(hit) = self.renderer.select_skin_click_hit(&snapshot, x, y) else {
            return;
        };
        self.handle_select_skin_click(hit, button, x, y);
    }

    pub(super) fn handle_result_skin_click(&mut self, x: f32, y: f32) {
        let AppSceneSnapshot::Result(snapshot) = self.scene_snapshot() else {
            return;
        };
        let Some(hit) = self.renderer.result_skin_click_hit(&snapshot, x, y) else {
            return;
        };
        let SkinClickTarget::Event { event_id, .. } = hit.target else {
            return;
        };
        match result_skin_click_action(event_id) {
            Some(ResultSkinClickAction::SetPanel(panel)) => {
                self.set_result_panel(panel);
            }
            Some(ResultSkinClickAction::SelectIrScope(tab)) => {
                self.select_result_ir_scope(tab);
            }
            Some(ResultSkinClickAction::ToggleIrScope) => {
                self.toggle_result_ir_scope();
            }
            Some(ResultSkinClickAction::ToggleFavoriteChart) => {
                self.toggle_favorite_chart_result();
            }
            Some(ResultSkinClickAction::SaveReplay(slot)) => {
                if self.result.finished_course.is_some() {
                    self.save_finished_course_replay_slot(slot);
                } else {
                    self.save_finished_play_replay_slot(slot);
                }
            }
            Some(ResultSkinClickAction::ResetDailyStatistics) => {
                self.reset_daily_statistics();
            }
            None => {
                let _ = self.renderer.dispatch_result_skin_runtime_event(event_id);
            }
        }
    }

    pub(super) fn route_select_slider_drag(&mut self) {
        if self.viewer_waiting {
            self.select.select_slider_dragging_type = None;
            return;
        }
        if self.select.select_slider_dragging_type.is_none() {
            return;
        }
        let Some((x, y)) = self.cursor_position_normalized() else {
            return;
        };
        if matches!(self.view_state(), AppViewState::Result) {
            if self.result.result_exit.is_some() {
                return;
            }
            let AppSceneSnapshot::Result(snapshot) = self.scene_snapshot() else {
                return;
            };
            if let Some(hit) = self.renderer.result_skin_slider_hit(&snapshot, x, y) {
                self.apply_result_slider_hit(hit);
            }
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            return;
        }
        let snapshot = self.select_snapshot();
        if let Some(hit) = self.renderer.select_skin_slider_hit(&snapshot, x, y) {
            self.apply_select_slider_hit(hit);
        }
    }

    pub(super) fn cursor_position_normalized(&self) -> Option<(f32, f32)> {
        let window = self.window.as_ref()?;
        let position = self.ui.last_cursor_position?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some((
            (position.x as f32 / size.width as f32).clamp(0.0, 1.0),
            (position.y as f32 / size.height as f32).clamp(0.0, 1.0),
        ))
    }

    pub(super) fn select_search_word_hit(&self, x: f32, y: f32) -> bool {
        let snapshot = self.select_snapshot();
        let Some(rect) = self.renderer.select_skin_search_input_rect(&snapshot) else {
            return false;
        };
        x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    pub(super) fn search_cursor_to_end(&mut self) {
        self.select.search.cursor_to_end();
        self.update_search_ime_cursor_area();
    }

    pub(super) fn apply_select_slider_hit(&mut self, hit: SkinSliderHit) {
        match hit.slider_type {
            1 => self.apply_select_scroll_slider(hit.value),
            17..=19 => {
                let value = volume_f32_to_unit(hit.value);
                let mix = &mut self.boot.profile_config.audio_mix;
                match hit.slider_type {
                    17 if mix.master_volume != value => {
                        mix.master_volume = value;
                        self.sync_realtime_profile_settings();
                        tracing::info!(value, "select skin master volume changed");
                    }
                    18 if mix.key_volume != value => {
                        mix.key_volume = value;
                        self.sync_realtime_profile_settings();
                        tracing::info!(value, "select skin key volume changed");
                    }
                    19 if mix.bgm_volume != value => {
                        mix.bgm_volume = value;
                        self.sync_realtime_profile_settings();
                        tracing::info!(value, "select skin bgm volume changed");
                    }
                    _ => {}
                }
            }
            _ => {
                tracing::debug!(slider_type = hit.slider_type, "unsupported select skin slider");
            }
        }
    }

    pub(super) fn apply_result_slider_hit(&mut self, hit: SkinSliderHit) {
        if hit.slider_type == 8 {
            if let Some(result_ir) = &mut self.result.result_ir {
                result_ir.set_skin_scroll_rate(hit.value);
            }
        } else {
            tracing::debug!(slider_type = hit.slider_type, "unsupported result skin slider");
        }
    }

    pub(super) fn apply_select_scroll_slider(&mut self, value: f32) {
        if self.select.ir_battle.active {
            let len =
                self.select.select_ir.battle_entries_for(self.select.ir_battle.source_sha256).len();
            let Some(next) = select_scroll_slider_index(value, len) else {
                return;
            };
            if self.select.ir_battle.cursor != next {
                self.select.ir_battle.cursor = next;
                self.restart_select_bar_timer_without_scroll(Instant::now());
                self.play_system_sound(crate::system_sound::SoundType::Scratch);
            }
            return;
        }
        let Some(next) = select_scroll_slider_index(value, self.select.select_items.len()) else {
            return;
        };
        if self.select.selected_index != next {
            self.select.selected_index = next;
            self.sync_selected_play_mode();
            self.reset_selected_replay_slot();
            self.restart_select_bar_timer_without_scroll(Instant::now());
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
    }

    pub(super) fn handle_select_skin_click(
        &mut self,
        hit: SkinClickHit,
        button: MouseButton,
        x: f32,
        y: f32,
    ) {
        match hit.target {
            SkinClickTarget::SelectRow { row_index } => {
                self.handle_select_row_click(row_index, button);
            }
            SkinClickTarget::Event { event_id, click } => {
                let Some(arg) = select_click_event_arg(click, button, hit.rect, x, y) else {
                    return;
                };
                self.execute_select_skin_event(event_id, arg);
            }
        }
    }

    pub(super) fn handle_select_row_click(&mut self, row_index: u32, button: MouseButton) {
        if self.select.ir_battle.active {
            let len =
                self.select.select_ir.battle_entries_for(self.select.ir_battle.source_sha256).len();
            match select_row_click_action(
                row_index,
                button,
                self.select.ir_battle.cursor,
                len,
                false,
            ) {
                Some(SelectRowClickAction::Select(next)) => {
                    self.select.ir_battle.cursor = next;
                    self.restart_select_bar_timer_without_scroll(Instant::now());
                    self.play_system_sound(crate::system_sound::SoundType::Scratch);
                }
                Some(SelectRowClickAction::EnterOrPlay) => {
                    self.start_selected_battle();
                }
                Some(SelectRowClickAction::ExitFolder)
                | Some(SelectRowClickAction::CancelSettingsEdit) => {
                    self.close_select_ir_battle();
                }
                None => {}
            }
            return;
        }
        if in_settings_stack(&self.select.folder_stack) && button == MouseButton::Left {
            if self.select.settings_edit.is_some() {
                self.commit_settings_edit();
                return;
            }
            let selected_setting =
                self.select.select_items.get(row_index as usize).and_then(|item| match item {
                    SelectItem::Config(row) => Some((Some(row.entry_id), None)),
                    SelectItem::AppConfig(row) => Some((None, Some(row.entry_id))),
                    _ => None,
                });
            if let Some((profile_entry, app_entry)) = selected_setting {
                self.select.selected_index = row_index as usize;
                self.reset_selected_replay_slot();
                self.restart_select_bar_timer_without_scroll(Instant::now());
                if let Some(entry_id) = profile_entry {
                    self.begin_settings_edit(entry_id);
                } else if let Some(entry_id) = app_entry {
                    self.begin_app_settings_edit(entry_id);
                }
                return;
            }
        }
        match select_row_click_action(
            row_index,
            button,
            self.select.selected_index,
            self.select.select_items.len(),
            self.select.settings_edit.is_some(),
        ) {
            Some(SelectRowClickAction::Select(next)) => {
                self.select.selected_index = next;
                self.sync_selected_play_mode();
                self.reset_selected_replay_slot();
                self.restart_select_bar_timer_without_scroll(Instant::now());
                self.play_system_sound(crate::system_sound::SoundType::Scratch);
            }
            Some(SelectRowClickAction::EnterOrPlay) => self.enter_or_play_selected(),
            Some(SelectRowClickAction::CancelSettingsEdit) => self.cancel_settings_edit(),
            Some(SelectRowClickAction::ExitFolder) => self.exit_folder(),
            None => {}
        }
    }
}
