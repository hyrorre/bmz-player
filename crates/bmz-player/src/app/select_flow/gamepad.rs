use super::*;

impl WinitApp {
    pub(super) fn consume_captured_gamepad_events(&mut self) {
        let should_log_raw_input = self.should_log_gamepad_key_config_raw_input();
        let configs = gamepad_scratch_configs(&self.boot.profile_config.input);
        let slots =
            resolve_gamepad_runtime_slots(&self.boot.app_config.input, self.gamepad.as_ref());
        let Some(gamepad) = &mut self.gamepad else { return };
        gamepad.set_analog_config(
            configs,
            crate::input::gamepad::GamepadSlotMap::from_device_ids(slots),
        );
        let backend_name = gamepad.name();
        let output = gamepad.poll();
        if self.jobs.profile_change.is_some() {
            return;
        }
        if self.input.should_discard_gamepad_output(self.ui.focused) {
            for event in output
                .buttons
                .iter()
                .filter(|event| should_route_gamepad_event_while_discarding(event.pressed))
            {
                self.route_gamepad_button_event(event);
            }
            if self.ui.focused
                && let Some(pressed_buttons) = output.pressed_buttons.as_deref()
            {
                self.resync_gamepad_pressed_controls(pressed_buttons);
            }
            self.reset_select_analog_scroll();
            self.reset_play_analog_scroll();
            self.clear_result_ir_scroll_input();
            return;
        }
        if should_log_raw_input {
            for event in &output.raw_events {
                log_gamepad_key_config_raw_event(backend_name, event);
            }
        }
        #[cfg(all(windows, feature = "experimental-gameinput"))]
        if let Some(diagnostics) = gamepad.gameinput_diagnostics()
            && diagnostics.reading_count > 0
        {
            tracing::trace!(
                reading_count = diagnostics.reading_count,
                oldest_reading_age_us = diagnostics.oldest_reading_age_us,
                "GameInput main-thread poll"
            );
        }
        for event in &output.buttons {
            self.route_gamepad_button_event(event);
        }
        if self.ui.focused
            && let Some(pressed_buttons) = output.pressed_buttons.as_deref()
        {
            self.resync_gamepad_pressed_controls(pressed_buttons);
        }
        for tick in &output.axis_ticks {
            // キーコンフィグ待ち受け中は合成 Press を待たず、生 tick から直接捕捉する。
            // 軸が active のままでも (押しっぱなし扱いで Press が出なくても) 確実に拾える。
            let control = format!("{}{}", tick.name, if tick.ticks > 0 { "+" } else { "-" });
            if self.capture_egui_key_config_axis(&control) {
                continue;
            }
            if self.select.key_config_edit.as_ref().is_some_and(|session| session.listening) {
                self.apply_key_config_gamepad(&control);
                continue;
            }
            self.route_gamepad_axis_ticks(tick.device_id, &tick.name, tick.ticks);
        }
    }

    pub(super) fn route_gamepad_button_event(
        &mut self,
        event: &crate::input::gamepad::GamepadButtonEvent,
    ) {
        if self.jobs.profile_change.is_some() {
            return;
        }
        let control_event = ControlInputEvent::gamepad(event.device_id, &event.name, event.pressed);
        self.input.track_control(&control_event);
        // holdは物理状態を正とする。2回押しなどの単発操作はこの後のフィルターを通す。
        self.sync_select_holds_from_pressed_controls();
        self.sync_play_control_holds_from_pressed_controls();
        if self.viewer_waiting {
            self.sync_viewer_wait_exit_holds();
            return;
        }
        if self.capture_egui_key_config_gamepad(&event.name, event.pressed) {
            return;
        }
        let mut device_event = crate::input::gamepad::to_device_input_event(event);
        if should_bypass_analog_scratch_bounce(
            event,
            self.play.play_option_input.as_ref().map(|input| &input.binding),
        ) {
            device_event.bounce_policy = InputBouncePolicy::Bypass;
        }
        let Some(device_event) = self.filter_app_input_bounce(device_event) else {
            return;
        };
        let practice_config = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        let _ = (practice_config, device_event); // Gameplay input was delivered by InputCapture.

        if self.viewer_waiting {
            return;
        }
        self.route_gamepad_button(
            event.device_id,
            &event.name,
            event.pressed,
            event.synthesized_analog_axis,
        );
    }

    fn resync_gamepad_pressed_controls(
        &mut self,
        pressed_buttons: &[crate::input::gamepad::GamepadPressedButton],
    ) {
        self.input.replace_gamepad_pressed_controls(pressed_buttons);
        self.sync_select_holds_from_pressed_controls();
        self.sync_play_control_holds_from_pressed_controls();
        self.sync_viewer_wait_exit_holds();
    }

    pub(super) fn should_log_gamepad_key_config_raw_input(&self) -> bool {
        self.select
            .key_config_edit
            .as_ref()
            .is_some_and(|session| session.listening && session.target.slot().is_controller())
            || self
                .ui
                .egui
                .as_ref()
                .is_some_and(crate::ui::EguiLayer::key_config_controller_listening)
    }

    pub(super) fn route_gamepad_axis_ticks(&mut self, device: DeviceId, axis: &str, ticks: i32) {
        if self.viewer_waiting {
            return;
        }
        let practice_config = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        if practice_config
            && !self.play.play_e1_held
            && !self.play.play_e2_held
            && self.apply_practice_analog_cursor_ticks(device, axis, ticks)
        {
            return;
        }
        if self.apply_play_analog_option_ticks(axis, ticks) {
            return;
        }
        if self.accumulate_result_ir_analog_ticks(axis, ticks) {
            return;
        }
        self.accumulate_select_analog_ticks(axis, ticks);
    }

    pub(super) fn apply_practice_analog_cursor_ticks(
        &mut self,
        device: DeviceId,
        axis: &str,
        ticks: i32,
    ) -> bool {
        let is_config = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        if !is_config {
            return false;
        }
        if self.play.play_ending.is_some() {
            return true;
        }
        let Some(delta) = crate::app::play_flow_practice::practice_analog_cursor_delta(
            device,
            axis,
            ticks,
            self.play.play_option_input.as_ref(),
        ) else {
            return false;
        };
        let now = Instant::now();
        let idle = self.play.play_analog_last_tick_at.is_none_or(|last| {
            now.duration_since(last) > Duration::from_millis(SELECT_ANALOG_SCROLL_TOLERANCE_MS)
        });
        self.play.play_analog_last_tick_at = Some(now);
        if idle {
            self.play.play_analog_scroll_buffer = 0;
        }
        self.play.play_analog_scroll_buffer += delta;

        let ticks_per_scroll = self.boot.profile_config.input.analog_ticks_per_scroll.max(1) as i32;
        let steps =
            take_analog_scroll_steps(&mut self.play.play_analog_scroll_buffer, ticks_per_scroll);
        if let Some(practice) = &mut self.play.practice_session {
            for _ in 0..steps.abs() {
                crate::screens::practice::move_practice_cursor(
                    &mut practice.cursor,
                    practice.is_double,
                    steps > 0,
                );
            }
        }
        true
    }

    pub(super) fn apply_play_analog_option_ticks(&mut self, axis: &str, ticks: i32) -> bool {
        let Some(delta) = play_analog_lane_cover_delta(axis, ticks, &self.select.select_keys)
        else {
            return false;
        };
        let mode = match (self.play.play_e1_held, self.play.play_e2_held) {
            (true, false) => PlayAnalogOptionMode::LaneCover,
            (false, true) => PlayAnalogOptionMode::GreenNumber,
            _ => {
                self.reset_play_analog_scroll();
                return false;
            }
        };
        let lane_value_changing = self
            .play
            .active_play
            .as_ref()
            .is_some_and(|active_play| active_play.running.session.lane_cover_changing)
            || self
                .play
                .pending_play_start
                .as_ref()
                .is_some_and(|pending| pending.lane.lane_cover_changing);
        if !lane_value_changing {
            self.reset_play_analog_scroll();
            return false;
        }

        let now = Instant::now();
        let idle = self.play.play_analog_last_tick_at.is_none_or(|t| {
            now.duration_since(t) > Duration::from_millis(SELECT_ANALOG_SCROLL_TOLERANCE_MS)
        });
        self.play.play_analog_last_tick_at = Some(now);
        if idle {
            self.play.play_analog_scroll_buffer = 0;
        }
        self.play.play_analog_scroll_buffer += delta;

        let ticks_per_scroll = self.boot.profile_config.input.analog_ticks_per_scroll.max(1) as i32;
        let steps =
            take_analog_scroll_steps(&mut self.play.play_analog_scroll_buffer, ticks_per_scroll);
        if steps == 0 {
            return true;
        }

        let change = if steps > 0 { LaneCoverChange::Down } else { LaneCoverChange::Up };
        let action = match mode {
            PlayAnalogOptionMode::LaneCover => PlayLaneAction::AnalogLaneCoverDelta(
                lane_cover_change_step(change) * steps.abs() as f32,
            ),
            PlayAnalogOptionMode::GreenNumber => PlayLaneAction::GreenNumberDelta(
                green_number_change_step(green_number_change_from_analog_steps(steps))
                    * steps.abs(),
            ),
        };
        self.apply_play_lane_action(action);
        true
    }

    /// 選曲画面のアナログスクラッチ tick を蓄積する。回転量比例スクロール用。
    pub(super) fn accumulate_select_analog_ticks(&mut self, axis: &str, ticks: i32) {
        if !matches!(self.view_state(), AppViewState::Select)
            || self.play.active_play.is_some()
            || self.play.pending_decide.is_some()
            || self.play.pending_play_start.is_some()
            || self.select.key_config_edit.is_some()
            || (self.select.select_option_panel > 1 && self.select.settings_edit.is_none())
        {
            return;
        }
        let Some(delta) = select_analog_scroll_delta(axis, ticks, &self.select.select_keys) else {
            return;
        };
        let now = Instant::now();
        // tick が途切れていたら古い端数を捨てる (beatoraja の 200ms tolerance 相当)
        let idle = self.select.select_analog_last_tick_at.is_none_or(|t| {
            now.duration_since(t) > Duration::from_millis(SELECT_ANALOG_SCROLL_TOLERANCE_MS)
        });
        self.select.select_analog_last_tick_at = Some(now);
        update_analog_scroll_buffer(
            &mut self.select.select_analog_scroll_buffer,
            &mut self.select.select_analog_suppress_until_idle,
            idle,
            delta,
        );
    }

    /// キーコンフィグ確定/キャンセル後、回転中のスクラッチが止まるまで
    /// アナログスクロールを無効化する。
    pub(super) fn suppress_select_analog_until_idle(&mut self) {
        self.select.select_analog_suppress_until_idle = true;
        self.select.select_analog_scroll_buffer = 0;
        self.select.select_analog_last_tick_at = Some(Instant::now());
    }

    pub(super) fn reset_select_analog_scroll(&mut self) {
        self.select.select_analog_scroll_buffer = 0;
        self.select.select_analog_last_tick_at = None;
        self.select.select_analog_suppress_until_idle = false;
    }

    pub(super) fn reset_play_analog_scroll(&mut self) {
        self.play.play_analog_scroll_buffer = 0;
        self.play.play_analog_last_tick_at = None;
    }

    pub(super) fn accumulate_result_ir_analog_ticks(&mut self, axis: &str, ticks: i32) -> bool {
        if !matches!(self.view_state(), AppViewState::Result) {
            return false;
        }
        let Some(delta) = select_analog_scroll_delta(axis, ticks, &self.select.select_keys) else {
            return false;
        };
        if !self.result_ir_scroll_interactive() {
            self.reset_result_ir_analog_scroll();
            return true;
        }

        let now = Instant::now();
        let scroll = &mut self.result.result_ir_scroll;
        let idle = scroll.analog_last_tick_at.is_none_or(|last| {
            now.duration_since(last) > Duration::from_millis(SELECT_ANALOG_SCROLL_TOLERANCE_MS)
        });
        scroll.analog_last_tick_at = Some(now);
        if idle {
            scroll.analog_buffer = 0;
        }
        scroll.analog_buffer += delta;
        true
    }

    pub(super) fn advance_result_ir_analog_scroll(&mut self) {
        if !self.ui.focused
            || !matches!(self.view_state(), AppViewState::Result)
            || !self.result_ir_scroll_interactive()
        {
            self.reset_result_ir_analog_scroll();
            return;
        }
        let ticks_per_scroll = self.boot.profile_config.input.analog_ticks_per_scroll.max(1) as i32;
        let rows = take_analog_scroll_steps(
            &mut self.result.result_ir_scroll.analog_buffer,
            ticks_per_scroll,
        );
        for _ in 0..rows.abs() {
            self.scroll_result_ir_rows(rows.signum());
        }
    }

    pub(super) fn reset_result_ir_analog_scroll(&mut self) {
        let scroll = &mut self.result.result_ir_scroll;
        scroll.analog_buffer = 0;
        scroll.analog_last_tick_at = None;
    }

    /// 蓄積したアナログ tick を analog_ticks_per_scroll ごとに 1 移動へ変換する。
    /// beatoraja MusicSelectInputProcessor の analogScrollBuffer と同じ仕組み。
    pub(super) fn advance_select_analog_scroll(&mut self) {
        if !self.ui.focused {
            self.reset_select_analog_scroll();
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            self.reset_select_analog_scroll();
            return;
        }
        if self.select.key_config_edit.is_some() {
            self.reset_select_analog_scroll();
            return;
        }
        let ticks_per_scroll = self.boot.profile_config.input.analog_ticks_per_scroll.max(1) as i32;
        let mov = take_analog_scroll_steps(
            &mut self.select.select_analog_scroll_buffer,
            ticks_per_scroll,
        );
        if mov == 0 {
            return;
        }
        if self.select.settings_edit.is_some() {
            let direction = settings_edit_direction_from_analog_scroll(mov);
            for _ in 0..mov.abs() {
                self.adjust_settings_edit(direction);
            }
            return;
        }
        if self.select.select_option_panel > 1 {
            self.reset_select_analog_scroll();
            return;
        }
        if self.select.select_option_panel == 1 {
            let cycle = if mov > 0 { TargetCycle::Next } else { TargetCycle::Previous };
            for _ in 0..mov.abs() {
                self.apply_target_option_cycle(cycle);
            }
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        } else {
            for _ in 0..mov.abs() {
                self.move_selection_with_duration(
                    if mov > 0 { SelectMove::Next } else { SelectMove::Previous },
                    select_analog_scroll_duration(mov),
                );
            }
        }
    }

    pub(super) fn route_gamepad_button(
        &mut self,
        device: DeviceId,
        button: &str,
        pressed: bool,
        synthesized_analog_axis: bool,
    ) {
        let control_event = ControlInputEvent::gamepad(device, button, pressed);
        let physical_control =
            control_event.physical.as_ref().expect("gamepad control always has a physical value");
        let has_play_control_context =
            self.play.active_play.is_some() || self.play.pending_play_start.is_some();
        if pressed
            && self.select.key_config_edit.is_none()
            && self.select.select_keys.is_screenshot(button)
        {
            self.request_manual_screenshot();
            return;
        }
        if self.route_practice_gamepad_control(device, button, pressed, synthesized_analog_axis) {
            return;
        }
        if should_route_quick_retry_input(pressed, false, self.play.play_ending.is_some())
            && self.handle_quick_retry_control(button)
        {
            return;
        }
        if pressed && self.begin_play_fadeout_after_final_notes_control(button) {
            return;
        }
        if self.play.play_ending.is_some() {
            return;
        }
        let play_e1_control = has_play_control_context
            && self.update_play_e1_control_state(device, physical_control, pressed);
        if has_play_control_context
            && self.update_play_exit_control_state(device, physical_control, pressed)
        {
            return;
        }
        let play_option_control = pressed.then(|| {
            play_option_control_for_input(
                device,
                physical_control,
                self.play.play_e1_held,
                self.play.play_e2_held,
                self.play.play_option_input.as_ref(),
                &self.boot.profile_config.input,
            )
        });
        let play_option_control = play_option_control.flatten();
        let play_option_lane_action = play_option_control
            .and_then(|action| lane_action_from_option(action, button.starts_with("Axis")));
        if pressed {
            let lane_cover_changing = self
                .play
                .active_play
                .as_ref()
                .is_some_and(|play| play.running.session.lane_cover_changing);
            if lane_cover_changing && play_option_control.is_some() {
                let Some(action) = play_option_lane_action else {
                    return;
                };
                self.apply_play_lane_action(action);
                // Gamepad play input was already queued in consume_captured_gamepad_events.
            }
        }
        if !pressed {
            if matches!(self.view_state(), AppViewState::Select)
                && self.select.select_option_panel == 0
                && self.select.select_keys.is_ui_key4(button)
                && self.finish_select_ir_battle_hold(button)
            {
                return;
            }
            if in_settings_stack(&self.select.folder_stack) {
                self.clear_select_hold_control(button);
                return;
            }
            self.update_select_e_action_hold(button, false);
            if self.select.select_keys.is_start(button) {
                self.set_start_held(false);
            } else if self.select.select_keys.is_e2_action(button) || matches!(button, "Select") {
                self.set_select_held(false);
            }
            return;
        }

        self.update_select_e_action_hold(button, true);

        // プレイ中: Start / E1 の2回連続押しでレーンカバー表示切替。
        // プレイ入力自体は push_shared_event で処理済み。
        if self.play.active_play.is_some() {
            if play_e1_control {
                self.handle_play_start_double_press();
            }
            return;
        }

        if self.play.pending_decide.is_some() {
            if self.update_decide_cancel_control_state(button, pressed) {
                return;
            }
            if let Some(action) = scene_decide_action(&control_event, &self.select.select_keys) {
                self.begin_decide_fadeout(matches!(action, DecideAction::Cancel));
            }
            return;
        }

        if self.play.pending_play_start.is_some() {
            if play_option_control.is_some() {
                if let Some(action) = play_option_lane_action {
                    self.apply_play_lane_action(action);
                }
                return;
            }
            if play_e1_control {
                self.handle_play_start_double_press();
            }
            return;
        }

        // コース曲間の中間リザルト: リトライ無効、次の曲へ進むだけ。
        // retry を持つ単曲リザルト分岐より先に評価する。
        if self.is_course_intermediate_result() {
            let control = PhysicalControl::GamepadButton(button.to_string());
            if self.request_result_exit_skip_for_control(&control, pressed, false) {
                return;
            }
            if self.result.result_exit.is_none() {
                if self.handle_course_intermediate_control(&control, pressed, false) {
                    return;
                }
                if self.result_input_ready() && scene_result_action(&control_event).is_some() {
                    self.begin_result_exit(self.course_intermediate_exit_action());
                }
            }
            return;
        }

        // リザルト画面
        if self.result.finished_play.is_some() && self.result.finished_course.is_none() {
            let control = PhysicalControl::GamepadButton(button.to_string());
            // フェードアウト中でも Key5/Key7 の押下状態は追跡する。
            self.track_result_lane_hold(&control, pressed);
            if self.request_result_exit_skip_for_control(&control, pressed, false) {
                return;
            }
            // 終了アニメーション中 (result_exit=Some) は held 追跡のみ行う。
            if self.result.result_exit.is_none() {
                if self.handle_result_control(&control, pressed, false) {
                    return;
                }
                if self.result_input_ready()
                    && let Some(action) = scene_result_action(&control_event)
                {
                    self.apply_result_action(action, false);
                }
            }
            return;
        }

        // コース（段位）リザルト: Key5/Key7 はフェードアウト後の hold 状態で
        // retry arrange を決める。Button1/Start は同配置リトライ。
        if self.result.finished_course.is_some() {
            let control = PhysicalControl::GamepadButton(button.to_string());
            self.track_result_lane_hold(&control, pressed);
            if self.request_result_exit_skip_for_control(&control, pressed, false) {
                return;
            }
            if self.result.result_exit.is_none() {
                if self.handle_course_result_control(&control, pressed, false) {
                    return;
                }
                if self.result_input_ready()
                    && let Some(action) = scene_result_action(&control_event)
                {
                    self.apply_result_action(action, true);
                }
            }
            return;
        }

        if in_settings_stack(&self.select.folder_stack) {
            if self.select.key_config_edit.as_ref().is_some_and(|session| session.listening) {
                if pressed {
                    self.apply_key_config_gamepad(button);
                }
                return;
            }
            if pressed {
                let _ = self.route_settings_control(button);
            }
            return;
        }

        if self.select.select_option_panel == 0
            && self.select.select_keys.is_ui_key4(button)
            && self.begin_select_ir_battle_hold(
                button,
                scene_select_action(&control_event, &self.select.select_keys),
            )
        {
            return;
        }

        if self.select.select_option_panel == 0
            && self.select_ir_scope_toggle_is_e3()
            && self.is_select_ir_scope_toggle_control(button)
            && self.toggle_select_ir_scope()
        {
            return;
        }

        if should_toggle_select_gauge_auto_shift(
            button,
            self.input.start_held,
            self.input.select_held,
            &self.select.select_keys,
        ) {
            self.toggle_gauge_auto_shift();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            if self.select.select_keys.is_e2_action(button) {
                self.set_select_held(true);
            }
            return;
        }

        if should_toggle_select_judge_auto_adjust(
            button,
            self.input.start_held,
            self.input.select_held,
            &self.select.select_keys,
        ) {
            self.toggle_visual_offset_auto_adjust();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            if self.select.select_keys.is_e2_action(button) {
                self.set_select_held(true);
            }
            return;
        }

        if self.select.select_keys.is_start(button) {
            self.set_start_held(true);
            return;
        }

        if self.select.select_keys.is_e2_action(button) || matches!(button, "Select") {
            self.set_select_held(true);
            return;
        }

        if self.select.select_option_panel != 0 {
            if self.select.select_option_panel == 1
                && let Some(cycle) = target_cycle_from_control(button, &self.select.select_keys)
            {
                if button.starts_with("Axis") {
                    return;
                }
                self.apply_target_option_cycle(cycle);
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                return;
            }
            let option_changed = match self.select.select_option_panel {
                1 => self.apply_gamepad_play_option_control(device, button),
                2 => self.apply_gamepad_assist_option_control(device, button),
                3 => self.apply_detail_option_control(button),
                _ => false,
            };
            if option_changed {
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            return;
        }

        if matches!(self.view_state(), AppViewState::Select) {
            // アナログ軸にバインドされたスクラッチは tick 比例スクロール
            // (advance_select_analog_scroll) で処理する。beatoraja の isNonAnalogPressed 相当。
            if button.starts_with("Axis")
                && (self.select.select_keys.is_select_scratch_up(button)
                    || self.select.select_keys.is_select_scratch_down(button))
            {
                return;
            }
            if let Some(action) = scene_select_action(&control_event, &self.select.select_keys) {
                self.apply_select_action(action, Some(button));
            }
        }
    }
}
