use super::*;

impl WinitApp {
    pub(super) fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        // Releases must clear the exit gesture even when a panel/modal consumes input.
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape)
            && event.state == ElementState::Released
        {
            self.select.select_exit_hold_started_at = None;
        }
        if self.jobs.profile_change.is_some() {
            return;
        }
        let mut event = event.clone();
        event.repeat = self.input.keyboard_repeat(event.physical_key, event.repeat);
        let event = &event;
        let control_event = ControlInputEvent::keyboard(event);
        self.input.track_control(&control_event);
        // Repeat suppression only applies to actions, never physical holds.
        self.sync_select_holds_from_pressed_controls();
        self.sync_play_control_holds_from_pressed_controls();
        // 以降の操作判定は、デバイスとキーコンフィグを解決した入力だけを見る。
        let input = self.resolve_control_event(control_event);
        if self.detail_options_blocks_held_input(&input) {
            return;
        }
        if self.viewer_waiting {
            if self.route_waiting_viewer_keyboard(event) {
                return;
            }
            if !event.repeat {
                self.sync_viewer_wait_exit_holds();
            }
            return;
        }
        let accepted = event.repeat
            || key_event_to_device_input(event)
                .is_none_or(|device_event| self.filter_app_input_bounce(device_event).is_some());
        if accepted && self.route_viewer_keyboard(event) {
            return;
        }
        if self.route_select_option_session_event(&input, accepted) || !accepted {
            return;
        }
        let has_play_control_context =
            self.play.active_play.is_some() || self.play.pending_play_start.is_some();
        if should_route_quick_retry_input(
            input.pressed(),
            input.repeat(),
            self.play.play_ending.is_some(),
        ) && self.handle_quick_retry_control(&input)
        {
            return;
        }
        if input.pressed()
            && !input.repeat()
            && self.begin_play_fadeout_after_final_notes_control(&input)
        {
            return;
        }
        if self.play.play_ending.is_some() {
            return;
        }
        if has_play_control_context {
            self.update_play_e1_control_state(&input);
        }
        if has_play_control_context && self.update_play_exit_control_state(&input) {
            return;
        }
        if self.active_play_uses_playback_rate_keys()
            && is_unassigned_autoplay_replay_playback_rate_key(
                event.physical_key,
                self.play.play_option_input.as_ref(),
            )
        {
            self.sync_autoplay_replay_playback_rate();
            return;
        }
        let window_keyboard_gameplay_enabled = self.window_keyboard_gameplay_enabled();
        self.input.track_window_keyboard(
            event.physical_key,
            event.state,
            event.repeat,
            window_keyboard_gameplay_enabled,
            has_play_control_context,
        );
        if has_play_control_context
            && window_keyboard_gameplay_enabled
            && let Some(device_event) = key_event_to_device_input(event)
        {
            self.route_play_device_input(device_event);
        }
        let play_option_lane_action = if input.pressed() && !input.repeat() {
            play_option_control_for_input(
                &input,
                self.play.play_e1_held,
                self.play.play_e2_held,
                self.play.play_option_input.as_ref(),
                &self.boot.profile_config.input,
            )
            .and_then(|action| lane_action_from_option(action, false))
        } else {
            None
        };
        // プレイのレーン・E1〜E4 に割り当てたキーは、プレイ用ショートカットより優先する。
        let fixed_play_lane_action = if has_play_control_context
            && input
                .play()
                .is_some_and(|control| control.lane().is_some() || control.e_action().is_some())
        {
            None
        } else {
            keyboard_lane_action_with_fine(
                &input,
                &self.boot.profile_config.input,
                self.lane_cover_fine_adjust_held(),
            )
        };
        if has_play_control_context && !input.repeat() {
            self.update_lane_cover_key_hold(
                &input,
                matches!(fixed_play_lane_action, Some(PlayLaneAction::LaneCoverDelta(_))),
            );
        }
        if self.play.active_play.is_some() {
            self.route_active_play_keyboard(
                &input,
                play_option_lane_action,
                fixed_play_lane_action,
            );
            return;
        }
        if self.play.pending_decide.is_some() {
            self.route_pending_decide_keyboard(&input);
            return;
        }
        if self.play.pending_play_start.is_some() {
            self.route_pending_play_start_keyboard(
                &input,
                play_option_lane_action,
                fixed_play_lane_action,
            );
            return;
        }
        if self.is_course_intermediate_result() {
            self.route_course_intermediate_keyboard(&input);
            return;
        }
        if self.result.finished_play.is_some() && self.result.finished_course.is_none() {
            self.route_finished_play_keyboard(&input, false);
            return;
        }
        if self.result.finished_course.is_some() {
            self.route_finished_play_keyboard(&input, true);
            return;
        }
        self.route_select_keyboard(event, &input);
    }

    fn route_active_play_keyboard(
        &mut self,
        input: &ResolvedInput,
        play_option_lane_action: Option<PlayLaneAction>,
        fixed_play_lane_action: Option<PlayLaneAction>,
    ) {
        if self.play_lane_value_changing()
            && let Some(action) = play_option_lane_action
        {
            self.apply_play_lane_action(action);
            // E1+lane keys should still reach gameplay input so notes are judged
            // and key beams render while changing play options.
        }
        if let Some(action) = fixed_play_lane_action {
            self.apply_play_lane_action(action);
            return;
        }
        if input.pressed()
            && !input.repeat()
            && input.scene().standard() == Some(StandardKey::Escape)
        {
            self.stop_play_like_escape("escape pressed during play");
            return;
        }
        // Start / E1 の2回連続押し → レーンカバー表示切替。現在のプレイ設定で判定する。
        if input.pressed()
            && !input.repeat()
            && input.play().unwrap_or(input.scene()).is_play_start()
        {
            self.handle_play_start_double_press();
            // Start キーはゲームプレイ入力としても通すのでフォールスルー
        }
    }

    fn route_pending_decide_keyboard(&mut self, input: &ResolvedInput) {
        if self.update_decide_cancel_control_state(input) {
            return;
        }
        if let Some(action) = scene_decide_action(input) {
            self.begin_decide_fadeout(matches!(action, DecideAction::Cancel));
        }
    }

    fn route_pending_play_start_keyboard(
        &mut self,
        input: &ResolvedInput,
        play_option_lane_action: Option<PlayLaneAction>,
        fixed_play_lane_action: Option<PlayLaneAction>,
    ) {
        if input.pressed()
            && !input.repeat()
            && input.scene().standard() == Some(StandardKey::Escape)
        {
            self.stop_play_like_escape("escape pressed during play preload");
            return;
        }
        if let Some(action) = fixed_play_lane_action {
            self.apply_play_lane_action(action);
            return;
        }
        if let Some(action) = play_option_lane_action {
            self.apply_play_lane_action(action);
            return;
        }
        if input.pressed()
            && !input.repeat()
            && input.play().unwrap_or(input.scene()).is_play_start()
        {
            self.handle_play_start_double_press();
        }
    }

    fn route_course_intermediate_keyboard(&mut self, input: &ResolvedInput) {
        // コース曲間の中間リザルト: リトライ無効、次の曲へ進むだけ。Key6 の
        // ゲージグラフ切替のみ単曲リザルト同様に許可する。retry を持つ単曲
        // リザルト分岐より先に評価し、R/Key5/Key7 等での誤 retry を防ぐ。
        if self.request_result_exit_skip_for_control(input) {
            return;
        }
        if self.result.result_exit.is_none() && self.handle_course_intermediate_control(input) {
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && input.pressed()
            && !input.repeat()
            && let Some(slot) = digit_to_replay_slot(input.scene())
        {
            self.save_finished_play_replay_slot(slot);
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && scene_result_action(input).is_some()
        {
            // R / Enter / Escape いずれも次の曲へ進むだけ (retry/leave 区別なし)。
            self.begin_result_exit(self.course_intermediate_exit_action());
        }
    }

    /// 単曲 / コース（段位）リザルト。Key5/Key7 はフェードアウト後の hold 状態で
    /// retry arrange を決める。Key6 はゲージグラフ切替。
    fn route_finished_play_keyboard(&mut self, input: &ResolvedInput, course_result: bool) {
        // フェードアウト中でも Key5/Key7 の押下状態は追跡し、
        // アニメーション終了時の retry arrange 判定に使う。
        self.sync_result_lane_holds_from_pressed_controls();
        if self.request_result_exit_skip_for_control(input) {
            return;
        }
        // 終了アニメーション中 (result_exit=Some) は held 追跡のみで、
        // 新しいアクションは受け付けない。
        if self.result.result_exit.is_none() {
            let handled = if course_result {
                self.handle_course_result_control(input)
            } else {
                self.handle_result_control(input)
            };
            if handled {
                return;
            }
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && input.pressed()
            && !input.repeat()
            && let Some(slot) = digit_to_replay_slot(input.scene())
        {
            if course_result {
                self.save_finished_course_replay_slot(slot);
            } else {
                self.save_finished_play_replay_slot(slot);
            }
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && let Some(action) = scene_result_action(input)
        {
            self.apply_result_action(action, course_result);
        }
    }

    fn route_select_keyboard(&mut self, event: &winit::event::KeyEvent, input: &ResolvedInput) {
        let control = *input.scene();
        let pressed = input.pressed();
        let repeat = input.repeat();
        let standard = control.standard();
        if self.ui.egui.as_ref().is_some_and(|ui| ui.blocks_game_input(false)) {
            self.reset_detail_options_input();
            return;
        }
        if self.route_detail_options_input(input) {
            return;
        }
        if self.detail_options_enabled() && self.select.select_option_panel == 1 {
            if pressed && !repeat {
                if let Some(slot) = digit_to_replay_slot(&control) {
                    self.start_replay_for_selected(slot);
                } else {
                    let changed = if let Some(cycle) = target_cycle_from_input(&control) {
                        self.apply_target_option_cycle(cycle);
                        true
                    } else {
                        self.apply_play_option_input(&control)
                    };
                    if changed {
                        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                    }
                }
            }
            return;
        }
        if self.viewer_waiting {
            // 待機中はエディタからのIPCだけを再生入口にする。Escape長押しによる
            // 通常終了だけはSelectと同じ操作として残す。
            if standard == Some(StandardKey::Escape) {
                update_select_exit_hold(
                    &mut self.select.select_exit_hold_started_at,
                    event.state,
                    event.repeat,
                    Instant::now(),
                );
            }
            return;
        }
        if matches!(self.view_state(), AppViewState::Select)
            && self.select.select_option_panel == 0
            && control.is_ir_battle_hold()
            && let Some(source) = input.source()
        {
            if pressed && !repeat {
                let short_action = scene_select_action(input);
                if self.begin_select_ir_battle_hold(source.clone(), short_action) {
                    return;
                }
            } else if !pressed && self.finish_select_ir_battle_hold(source) {
                return;
            }
        }

        if matches!(self.view_state(), AppViewState::Select)
            && standard == Some(StandardKey::Tab)
            && pressed
            && !repeat
        {
            self.toggle_select_ir_battle();
            return;
        }

        if matches!(self.view_state(), AppViewState::Select)
            && !in_settings_stack(&self.select.folder_stack)
            && pressed
            && !repeat
            && let Some(action) = configurable_select_shortcut_action(&control)
        {
            self.apply_select_action(action, input.source());
            return;
        }

        // 検索モード中はテキスト入力を最優先で処理し、通常ナビゲーションは抑制する。
        // モード入りトリガ (`/`) も同じ select 画面チェックの直後に処理する。
        // 文字入力はキーコンフィグの対象外なので、生のキーイベントを渡す。
        if matches!(self.view_state(), AppViewState::Select)
            && !in_settings_stack(&self.select.folder_stack)
            && self.handle_search_key(event)
        {
            return;
        }

        if self.select.course_builder.is_some() && pressed && !repeat {
            match standard {
                Some(StandardKey::Escape) => {
                    self.cancel_select_course_builder();
                    return;
                }
                Some(StandardKey::Delete | StandardKey::Backspace) => {
                    self.undo_select_course_entry();
                    return;
                }
                _ => {}
            }
        }

        if self.select.ir_battle.active
            && standard == Some(StandardKey::Escape)
            && pressed
            && !repeat
        {
            self.close_select_ir_battle();
            return;
        }

        // LR2 / beatoraja と同じく、ESC はフォルダ内ならフォルダを閉じる。
        // ルートでだけ長押しでアプリ終了 (実際の exit は redraw 時にチェック) を受け付ける。
        if standard == Some(StandardKey::Escape) {
            if pressed && !repeat {
                let editing = in_settings_stack(&self.select.folder_stack)
                    && (self.select.key_config_edit.is_some()
                        || self.select.settings_edit.is_some());
                match select_escape_press(editing, self.select.folder_stack.len()) {
                    SelectEscapePress::CancelEdit => {
                        if self.select.key_config_edit.is_some() {
                            self.cancel_key_config_edit();
                        } else {
                            self.cancel_settings_edit();
                        }
                        return;
                    }
                    SelectEscapePress::ExitFolder => {
                        self.apply_select_action(SelectAction::ExitFolder, None);
                        return;
                    }
                    SelectEscapePress::ArmExitHold => {}
                }
            }
            update_select_exit_hold(
                &mut self.select.select_exit_hold_started_at,
                event.state,
                event.repeat,
                Instant::now(),
            );
            return;
        }

        if in_settings_stack(&self.select.folder_stack) {
            if !pressed {
                if let Some(source) = input.source() {
                    self.clear_select_hold_control(source);
                }
                return;
            }
            if self.select.key_config_edit.is_some() && !repeat {
                // キーコンフィグの入力待ちは、解決前の生のキー名で割り当てる。
                match standard {
                    Some(StandardKey::Delete | StandardKey::Backspace) => {
                        self.clear_key_config_binding();
                    }
                    _ => {
                        if let Some(PhysicalControl::KeyboardKey(name)) = input.physical() {
                            let name = name.clone();
                            self.apply_key_config_control(&name);
                        }
                    }
                }
                return;
            }
            if !should_route_settings_key_event(
                event.state,
                event.repeat,
                self.select.settings_edit.is_some(),
            ) {
                return;
            }
            let _ = self.route_settings_control(input);
            return;
        }

        if pressed
            && !repeat
            && self.select.select_option_panel == 0
            && self.select_ir_scope_toggle_is_e3()
            && control.is_e3()
            && self.toggle_select_ir_scope()
        {
            return;
        }

        if pressed
            && !repeat
            && should_toggle_select_judge_auto_adjust(
                &control,
                self.input.start_held,
                self.input.select_held,
            )
        {
            self.toggle_visual_offset_auto_adjust();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            return;
        }

        if pressed
            && !repeat
            && should_toggle_select_gauge_auto_shift(
                &control,
                self.input.start_held,
                self.input.select_held,
            )
        {
            self.toggle_gauge_auto_shift();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            return;
        }

        if control.is_start() || control.is_e2() {
            // パネルの開閉は押下集合から集約した E1/E2 の状態で決める。
            return;
        }

        if self.select.select_option_panel != 0 {
            // LR2 panels 2..6 are operated with the mouse; ASSIST / DETAIL are in F1 settings.
            if self.select.select_option_panel > 1 && self.lr2_select_active() {
                return;
            }
            if pressed
                && (!repeat
                    || (self.select.select_option_panel == 3
                        && green_number_delta_control(&control).is_some()))
            {
                let panel = self.select.select_option_panel;
                match panel {
                    1 => {
                        if let Some(slot) = digit_to_replay_slot(&control) {
                            if !self.start_replay_for_selected(slot) {
                                tracing::info!(slot, "Start+digit pressed but no replay available");
                            }
                            return;
                        }
                        if let Some(cycle) = target_cycle_from_input(&control) {
                            self.apply_target_option_cycle(cycle);
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                            return;
                        }
                        if self.apply_play_option_input(&control) {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    2 => self.apply_legacy_assist_input(&control),
                    3 if self.apply_detail_option_control(&control) => {
                        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                    }
                    _ => {}
                }
            }
            return;
        }

        if matches!(self.view_state(), AppViewState::Select) {
            if let Some(action) = scene_select_action(input) {
                self.apply_select_action(action, input.source());
            } else if !pressed && let Some(source) = input.source() {
                self.clear_select_hold_control(source);
            }
        }
    }
}
