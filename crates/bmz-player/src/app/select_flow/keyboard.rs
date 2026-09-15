use super::*;

impl WinitApp {
    pub(super) fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        let mut event = event.clone();
        event.repeat = self.input.keyboard_repeat(event.physical_key, event.repeat);
        let event = &event;
        let control_event = ControlInputEvent::keyboard(event);
        self.input.track_control(&control_event);
        // Repeat suppression only applies to actions, never physical holds.
        self.sync_select_holds_from_pressed_controls();
        self.sync_play_control_holds_from_pressed_controls();
        if self.viewer_waiting {
            if self.route_waiting_viewer_keyboard(event) {
                return;
            }
            if !event.repeat {
                self.sync_viewer_wait_exit_holds();
            }
            return;
        }
        if !event.repeat
            && let Some(device_event) = key_event_to_device_input(event)
            && self.filter_app_input_bounce(device_event).is_none()
        {
            return;
        }
        if self.route_viewer_keyboard(event) {
            return;
        }
        let play_control = control_event.name.as_deref();
        let play_physical_control = control_event.physical.as_ref();
        let has_play_control_context =
            self.play.active_play.is_some() || self.play.pending_play_start.is_some();
        if should_route_quick_retry_input(
            control_event.pressed,
            control_event.repeat,
            self.play.play_ending.is_some(),
        ) && let Some(control) = play_control
            && self.handle_quick_retry_control(control)
        {
            return;
        }
        if control_event.pressed
            && !control_event.repeat
            && let Some(control) = play_control
            && self.begin_play_fadeout_after_final_notes_control(control)
        {
            return;
        }
        if self.play.play_ending.is_some() {
            return;
        }
        if has_play_control_context && let Some(control) = play_physical_control.as_ref() {
            self.update_play_e1_control_state(
                W_KEYBOARD_DEVICE_ID,
                control,
                event.state == ElementState::Pressed,
            );
        }
        if has_play_control_context
            && let Some(control) = play_physical_control.as_ref()
            && self.update_play_exit_control_state(
                W_KEYBOARD_DEVICE_ID,
                control,
                event.state == ElementState::Pressed,
            )
        {
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
        let play_option_lane_action = if event.state == ElementState::Pressed && !event.repeat {
            play_physical_control
                .as_ref()
                .and_then(|control| {
                    play_option_control_for_input(
                        W_KEYBOARD_DEVICE_ID,
                        control,
                        self.play.play_e1_held,
                        self.play.play_e2_held,
                        self.play.play_option_input.as_ref(),
                        &self.boot.profile_config.input,
                    )
                })
                .and_then(|action| lane_action_from_option(action, false))
        } else {
            None
        };
        let fixed_play_lane_action = if has_play_control_context
            && play_physical_control.is_some_and(|control| {
                self.play.play_option_input.as_ref().is_some_and(|input| {
                    input.resolves_lane(W_KEYBOARD_DEVICE_ID, control)
                        || [
                            InputActionConfig::E1,
                            InputActionConfig::E2,
                            InputActionConfig::E3,
                            InputActionConfig::E4,
                        ]
                        .into_iter()
                        .any(|action| input.is_action(W_KEYBOARD_DEVICE_ID, control, action))
                })
            }) {
            None
        } else {
            keyboard_lane_action(&control_event, &self.boot.profile_config.input)
        };
        if self.play.active_play.is_some() {
            self.route_active_play_keyboard(
                event,
                &control_event,
                play_option_lane_action,
                fixed_play_lane_action,
            );
            return;
        }
        if self.play.pending_decide.is_some() {
            self.route_pending_decide_keyboard(event, &control_event);
            return;
        }
        if self.play.pending_play_start.is_some() {
            self.route_pending_play_start_keyboard(
                event,
                &control_event,
                play_option_lane_action,
                fixed_play_lane_action,
            );
            return;
        }
        if self.is_course_intermediate_result() {
            self.route_course_intermediate_keyboard(event, &control_event);
            return;
        }
        if self.result.finished_play.is_some() && self.result.finished_course.is_none() {
            self.route_finished_play_keyboard(event, &control_event);
            return;
        }
        if self.result.finished_course.is_some() {
            self.route_finished_course_keyboard(event, &control_event);
            return;
        }
        self.route_select_keyboard(event, &control_event);
    }

    fn route_active_play_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
        play_option_lane_action: Option<PlayLaneAction>,
        fixed_play_lane_action: Option<PlayLaneAction>,
    ) {
        let play_control = control_event.name.as_deref();
        let lane_cover_changing = self
            .play
            .active_play
            .as_ref()
            .is_some_and(|play| play.running.session.lane_cover_changing);
        if lane_cover_changing && let Some(action) = play_option_lane_action {
            self.apply_play_lane_action(action);
            // E1+lane keys should still reach gameplay input so notes are judged
            // and key beams render while changing play options.
        }
        if let Some(action) = fixed_play_lane_action {
            self.apply_play_lane_action(action);
            return;
        }
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape)
            && event.state == ElementState::Pressed
            && !event.repeat
        {
            self.stop_play_like_escape("escape pressed during play");
            return;
        }
        // Start / E1 の2回連続押し → レーンカバー表示切替
        if control_event.pressed
            && !control_event.repeat
            && let Some(control) = play_control
            && self.select.select_keys.is_start(control)
        {
            self.handle_play_start_double_press();
            // Start キーはゲームプレイ入力としても通すのでフォールスルー
        }
    }

    fn route_pending_decide_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
    ) {
        if let Some(control) = control_event.name.as_deref()
            && !event.repeat
            && self
                .update_decide_cancel_control_state(control, event.state == ElementState::Pressed)
        {
            return;
        }
        if let Some(action) = scene_decide_action(control_event, &self.select.select_keys) {
            self.begin_decide_fadeout(matches!(action, DecideAction::Cancel));
        }
    }

    fn route_pending_play_start_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
        play_option_lane_action: Option<PlayLaneAction>,
        fixed_play_lane_action: Option<PlayLaneAction>,
    ) {
        let play_control = control_event.name.as_deref();
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape)
            && event.state == ElementState::Pressed
            && !event.repeat
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
        if event.state == ElementState::Pressed
            && !event.repeat
            && let Some(control) = play_control
            && self.select.select_keys.is_start(control)
        {
            self.handle_play_start_double_press();
        }
    }

    fn route_course_intermediate_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
    ) {
        // コース曲間の中間リザルト: リトライ無効、次の曲へ進むだけ。Key6 の
        // ゲージグラフ切替のみ単曲リザルト同様に許可する。retry を持つ単曲
        // リザルト分岐より先に評価し、R/Key5/Key7 等での誤 retry を防ぐ。
        let pressed = event.state == ElementState::Pressed;
        if self.request_result_exit_skip_for_key(event.physical_key, event.state, event.repeat) {
            return;
        }
        if self.result.result_exit.is_none()
            && let Some(control) = physical_key_to_control(event.physical_key)
            && self.handle_course_intermediate_control(&control, pressed, event.repeat)
        {
            return;
        }
        if let Some(control) = physical_key_to_control(event.physical_key)
            && self.request_result_exit_skip_for_control(&control, pressed, event.repeat)
        {
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && event.state == ElementState::Pressed
            && !event.repeat
            && let Some(slot) = digit_to_replay_slot(event.physical_key)
        {
            self.save_finished_play_replay_slot(slot);
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && scene_result_action(control_event).is_some()
        {
            // R / Enter / Escape いずれも次の曲へ進むだけ (retry/leave 区別なし)。
            self.begin_result_exit(self.course_intermediate_exit_action());
        }
    }

    fn route_finished_play_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
    ) {
        let pressed = event.state == ElementState::Pressed;
        if let Some(control) = physical_key_to_control(event.physical_key) {
            // フェードアウト中でも Key5/Key7 の押下状態は追跡し、
            // アニメーション終了時の retry arrange 判定に使う。
            self.track_result_lane_hold(&control, pressed);
            if self.request_result_exit_skip_for_key(event.physical_key, event.state, event.repeat)
                || self.request_result_exit_skip_for_control(&control, pressed, event.repeat)
            {
                return;
            }
            // 終了アニメーション中 (result_exit=Some) は held 追跡のみで、
            // 新しいアクションは受け付けない。
            if self.result.result_exit.is_none()
                && self.handle_result_control(&control, pressed, event.repeat)
            {
                return;
            }
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && event.state == ElementState::Pressed
            && !event.repeat
            && let Some(slot) = digit_to_replay_slot(event.physical_key)
        {
            self.save_finished_play_replay_slot(slot);
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && let Some(action) = scene_result_action(control_event)
        {
            self.apply_result_action(action, false);
        }
    }

    fn route_finished_course_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
    ) {
        // コース（段位）リザルト: Key5/Key7 はフェードアウト後の hold 状態で
        // retry arrange を決める。Key6 はゲージグラフ切替。
        let pressed = event.state == ElementState::Pressed;
        if let Some(control) = physical_key_to_control(event.physical_key) {
            self.track_result_lane_hold(&control, pressed);
            if self.request_result_exit_skip_for_key(event.physical_key, event.state, event.repeat)
                || self.request_result_exit_skip_for_control(&control, pressed, event.repeat)
            {
                return;
            }
            if self.result.result_exit.is_none()
                && self.handle_course_result_control(&control, pressed, event.repeat)
            {
                return;
            }
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && event.state == ElementState::Pressed
            && !event.repeat
            && let Some(slot) = digit_to_replay_slot(event.physical_key)
        {
            self.save_finished_course_replay_slot(slot);
            return;
        }
        if self.result.result_exit.is_none()
            && self.result_input_ready()
            && let Some(action) = scene_result_action(control_event)
        {
            self.apply_result_action(action, true);
        }
    }

    fn route_select_keyboard(
        &mut self,
        event: &winit::event::KeyEvent,
        control_event: &ControlInputEvent,
    ) {
        if self.viewer_waiting {
            // 待機中はエディタからのIPCだけを再生入口にする。Escape長押しによる
            // 通常終了だけはSelectと同じ操作として残す。
            if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                match event.state {
                    ElementState::Pressed => {
                        if self.select.select_exit_hold_started_at.is_none() {
                            self.select.select_exit_hold_started_at = Some(Instant::now());
                        }
                    }
                    ElementState::Released => {
                        self.select.select_exit_hold_started_at = None;
                    }
                }
            }
            return;
        }
        if matches!(self.view_state(), AppViewState::Select)
            && self.select.select_option_panel == 0
            && let Some(control) = physical_key_name(event.physical_key)
            && self.select.select_keys.is_ui_key4(&control)
        {
            match event.state {
                ElementState::Pressed if !event.repeat => {
                    let short_action = scene_select_action(control_event, &self.select.select_keys);
                    if self.begin_select_ir_battle_hold(&control, short_action) {
                        return;
                    }
                }
                ElementState::Released if self.finish_select_ir_battle_hold(&control) => return,
                _ => {}
            }
        }

        if matches!(self.view_state(), AppViewState::Select)
            && event.physical_key == PhysicalKey::Code(KeyCode::Tab)
            && event.state == ElementState::Pressed
            && !event.repeat
        {
            self.toggle_select_ir_battle();
            return;
        }

        if matches!(self.view_state(), AppViewState::Select)
            && !in_settings_stack(&self.select.folder_stack)
            && event.state == ElementState::Pressed
            && !event.repeat
            && let Some(control) = physical_key_name(event.physical_key)
            && let Some(action) =
                configurable_select_shortcut_action(&control, &self.select.select_keys)
        {
            self.apply_select_action(action, Some(&control));
            return;
        }

        if matches!(self.view_state(), AppViewState::Select)
            && event.state == ElementState::Released
            && let Some(control) = physical_key_name(event.physical_key)
        {
            self.update_select_e_action_hold(&control, false);
        }

        // 検索モード中はテキスト入力を最優先で処理し、通常ナビゲーションは抑制する。
        // モード入りトリガ (`/`) も同じ select 画面チェックの直後に処理する。
        if matches!(self.view_state(), AppViewState::Select)
            && !in_settings_stack(&self.select.folder_stack)
            && self.handle_search_key(event)
        {
            return;
        }

        if self.select.course_builder.is_some()
            && event.state == ElementState::Pressed
            && !event.repeat
        {
            match event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.cancel_select_course_builder();
                    return;
                }
                PhysicalKey::Code(KeyCode::Delete | KeyCode::Backspace) => {
                    self.undo_select_course_entry();
                    return;
                }
                _ => {}
            }
        }

        if self.select.ir_battle.active
            && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
            && event.state == ElementState::Pressed
            && !event.repeat
        {
            self.close_select_ir_battle();
            return;
        }

        // Select 画面で ESC 長押し → アプリ終了 (実際の exit は redraw 時にチェック)。
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            if in_settings_stack(&self.select.folder_stack)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                if self.select.key_config_edit.is_some() {
                    self.cancel_key_config_edit();
                    return;
                }
                if self.select.settings_edit.is_some() {
                    self.cancel_settings_edit();
                    return;
                }
            }
            match event.state {
                ElementState::Pressed => {
                    if self.select.select_exit_hold_started_at.is_none() {
                        self.select.select_exit_hold_started_at = Some(Instant::now());
                    }
                }
                ElementState::Released => {
                    self.select.select_exit_hold_started_at = None;
                }
            }
            return;
        }

        if in_settings_stack(&self.select.folder_stack) {
            if event.state == ElementState::Released
                && let Some(control_name) = physical_key_name(event.physical_key)
            {
                self.clear_select_hold_control(&control_name);
                return;
            }
            if self.select.key_config_edit.is_some()
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                if event.physical_key == PhysicalKey::Code(KeyCode::Delete)
                    || event.physical_key == PhysicalKey::Code(KeyCode::Backspace)
                {
                    self.clear_key_config_binding();
                    return;
                }
                if let Some(control) = physical_key_name(event.physical_key) {
                    if control == "Escape" {
                        self.cancel_key_config_edit();
                    } else if control == "Delete" || control == "Backspace" {
                        self.clear_key_config_binding();
                    } else {
                        self.apply_key_config_control(&control);
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
            if let Some(control) = physical_key_name(event.physical_key) {
                self.route_settings_control(&control);
            } else {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowUp) => {
                        let _ = self.route_settings_control("ArrowUp");
                    }
                    PhysicalKey::Code(KeyCode::ArrowDown) => {
                        let _ = self.route_settings_control("ArrowDown");
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        let _ = self.route_settings_control("ArrowLeft");
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        let _ = self.route_settings_control("ArrowRight");
                    }
                    PhysicalKey::Code(KeyCode::Enter) => {
                        let _ = self.route_settings_control("Enter");
                    }
                    PhysicalKey::Code(KeyCode::Space) => {
                        let _ = self.route_settings_control("Space");
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        let _ = self.route_settings_control("Escape");
                    }
                    _ => {}
                }
            }
            return;
        }

        if let Some(control) = physical_key_name(event.physical_key) {
            self.update_select_e_action_hold(&control, event.state == ElementState::Pressed);
        }

        if event.state == ElementState::Pressed
            && !event.repeat
            && self.select.select_option_panel == 0
            && self.select_ir_scope_toggle_is_e3()
            && let Some(control) = physical_key_name(event.physical_key)
            && self.is_select_ir_scope_toggle_control(&control)
            && self.toggle_select_ir_scope()
        {
            return;
        }

        if is_select_start_key(event.physical_key, &self.select.select_keys) {
            self.set_start_held(event.state == ElementState::Pressed);
            return;
        }

        if event.state == ElementState::Pressed
            && !event.repeat
            && let Some(control) = physical_key_name(event.physical_key)
            && should_toggle_select_judge_auto_adjust(
                &control,
                self.input.start_held,
                self.input.select_held,
                &self.select.select_keys,
            )
        {
            self.toggle_visual_offset_auto_adjust();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            if is_select_modifier_key(event.physical_key, &self.select.select_keys) {
                self.set_select_held(true);
            }
            return;
        }

        if event.state == ElementState::Pressed
            && !event.repeat
            && let Some(control) = physical_key_name(event.physical_key)
            && should_toggle_select_gauge_auto_shift(
                &control,
                self.input.start_held,
                self.input.select_held,
                &self.select.select_keys,
            )
        {
            self.toggle_gauge_auto_shift();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            if is_select_modifier_key(event.physical_key, &self.select.select_keys) {
                self.set_select_held(true);
            }
            return;
        }

        if is_select_modifier_key(event.physical_key, &self.select.select_keys) {
            self.set_select_held(event.state == ElementState::Pressed);
            return;
        }

        if self.select.select_option_panel != 0 {
            if event.state == ElementState::Pressed
                && (!event.repeat
                    || (self.select.select_option_panel == 3
                        && physical_key_name(event.physical_key).is_some_and(|control| {
                            green_number_delta_control(&control, &self.select.select_keys).is_some()
                        })))
            {
                match self.select.select_option_panel {
                    1 => {
                        if let Some(slot) = digit_to_replay_slot(event.physical_key) {
                            if !self.start_replay_for_selected(slot) {
                                tracing::info!(slot, "Start+digit pressed but no replay available");
                            }
                            return;
                        }
                        if let Some(cycle) = target_cycle_from_key(event.physical_key) {
                            self.apply_target_option_cycle(cycle);
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                            return;
                        }
                        if let Some(control) = physical_key_name(event.physical_key)
                            && let Some(cycle) =
                                target_cycle_from_control(&control, &self.select.select_keys)
                        {
                            self.apply_target_option_cycle(cycle);
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                            return;
                        }
                        if let Some(control) = physical_key_name(event.physical_key)
                            && self.apply_play_option_control(&control)
                        {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    2 => {
                        if let Some(control) = physical_key_name(event.physical_key)
                            && self.apply_assist_option_control(&control)
                        {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    3 => {
                        if let Some(control) = physical_key_name(event.physical_key)
                            && self.apply_detail_option_control(&control)
                        {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    _ => {}
                }
            }
            return;
        }

        if matches!(self.view_state(), AppViewState::Select) {
            if let Some(action) = scene_select_action(control_event, &self.select.select_keys) {
                self.apply_select_action(action, control_event.name.as_deref());
            } else if event.state == ElementState::Released
                && let Some(control_name) = control_event.name.as_deref()
            {
                self.clear_select_hold_control(control_name);
            }
        }
    }
}
