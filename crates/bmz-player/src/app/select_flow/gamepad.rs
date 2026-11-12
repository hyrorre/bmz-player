use super::*;

impl WinitApp {
    pub(super) fn consume_captured_gamepad_events(&mut self) {
        let should_log_raw_input = self.should_log_gamepad_key_config_raw_input();
        let configs = gamepad_scratch_configs(&self.boot.profile_config.input);
        let slots = crate::input::gamepad::GamepadSlotMap::from_device_ids(
            resolve_gamepad_runtime_slots(&self.boot.app_config.input, self.gamepad.as_ref()),
        );
        self.update_gamepad_slot_assignment(slots);
        let Some(gamepad) = &mut self.gamepad else { return };
        gamepad.set_analog_config(configs, slots);
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

    /// 論理スロットの割り当てが変わったら、押下中の入力を新しい割り当てで解決し直す。
    pub(super) fn update_gamepad_slot_assignment(
        &mut self,
        slots: crate::input::gamepad::GamepadSlotMap,
    ) {
        if self.input.gamepad_slots == slots {
            return;
        }
        self.input.gamepad_slots = slots;
        // 旧割り当てで始めた長押しは、新しい割り当てへ Release だけを送らないよう破棄する。
        self.clear_select_hold();
        self.reset_select_analog_scroll();
        self.reset_play_analog_scroll();
        self.sync_select_holds_from_pressed_controls();
        self.reconcile_select_option_session();
    }

    pub(super) fn route_gamepad_button_event(
        &mut self,
        event: &crate::input::gamepad::GamepadButtonEvent,
    ) {
        if self.jobs.profile_change.is_some() {
            return;
        }
        let control_event = ControlInputEvent::gamepad_button(event);
        self.input.track_control(&control_event);
        // holdは物理状態を正とする。2回押しなどの単発操作はこの後のフィルターを通す。
        self.sync_select_holds_from_pressed_controls();
        self.sync_play_control_holds_from_pressed_controls();
        let input = self.resolve_control_event(control_event);
        if self.detail_options_blocks_held_input(&input) {
            return;
        }
        if self.viewer_waiting {
            self.sync_viewer_wait_exit_holds();
            return;
        }
        // キーコンフィグの入力待ちは解決前の生の名前を使い、通常操作へ流さない。
        if self.capture_egui_key_config_gamepad(&event.name, event.pressed) {
            return;
        }
        if in_settings_stack(&self.select.folder_stack)
            && self.select.key_config_edit.as_ref().is_some_and(|session| session.listening)
        {
            if event.pressed {
                self.apply_key_config_gamepad(&event.name);
            }
            return;
        }
        if matches!(self.view_state(), AppViewState::Select)
            && self.ui.egui.as_ref().is_some_and(|ui| ui.blocks_game_input(false))
        {
            self.reset_detail_options_input();
            return;
        }
        let mut device_event = crate::input::gamepad::to_device_input_event(event);
        if should_bypass_analog_scratch_bounce(
            event,
            self.play.play_option_input.as_ref().map(|input| &input.binding),
        ) {
            device_event.bounce_policy = InputBouncePolicy::Bypass;
        }
        let device_event = self.filter_app_input_bounce(device_event);
        if self.route_select_option_session_event(&input, device_event.is_some()) {
            return;
        }
        if device_event.is_none() {
            return;
        }
        if self.route_detail_options_input(&input) {
            return;
        }
        if self.detail_options_enabled() && self.select.select_option_panel == 1 {
            if input.pressed() {
                let changed = if let Some(cycle) = target_cycle_from_input(input.scene()) {
                    if input.synthesized_analog_axis() {
                        return;
                    }
                    self.apply_target_option_cycle(cycle);
                    true
                } else {
                    self.apply_play_option_input(input.scene())
                };
                if changed {
                    self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                }
            }
            return;
        }
        // Gameplay input was delivered by InputCapture.
        if self.viewer_waiting {
            return;
        }
        self.route_gamepad_button(&input);
    }

    fn resync_gamepad_pressed_controls(
        &mut self,
        pressed_buttons: &[crate::input::gamepad::GamepadPressedButton],
    ) {
        self.input.replace_gamepad_pressed_controls(pressed_buttons);
        self.sync_select_holds_from_pressed_controls();
        self.reconcile_select_option_session();
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
        // tick ごとにデバイスと方向を解決してから合算する。
        let tick = self.resolve_gamepad_axis_tick(device, axis, ticks);
        if self.apply_play_analog_option_ticks(tick.play_scroll) {
            return;
        }
        if self.accumulate_result_ir_analog_ticks(tick.scene_scroll) {
            return;
        }
        self.accumulate_select_analog_ticks(tick.scene_scroll);
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

    /// プレイ中の E1/E2 + アナログSCR。現在のプレイ用 SCR 設定で解決した寄与を受け取る。
    pub(super) fn apply_play_analog_option_ticks(&mut self, play_scroll: Option<i32>) -> bool {
        let Some(delta) = play_scroll else {
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
            // 回転量が1段階に届かなくても、スクラッチが反応した時点でFloatingを再計算する。
            if mode == PlayAnalogOptionMode::LaneCover {
                self.apply_play_lane_action(PlayLaneAction::AnalogLaneCoverDelta(0.0));
            }
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
    pub(super) fn accumulate_select_analog_ticks(&mut self, scene_scroll: Option<i32>) {
        if !matches!(self.view_state(), AppViewState::Select)
            || self.play.active_play.is_some()
            || self.play.pending_decide.is_some()
            || self.play.pending_play_start.is_some()
            || self.select.key_config_edit.is_some()
            || (self.select.select_option_panel == 3 && self.select.settings_edit.is_none())
            || self.ui.egui.as_ref().is_some_and(|ui| ui.blocks_game_input(false))
        {
            return;
        }
        let Some(delta) = scene_scroll else {
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

    pub(super) fn accumulate_result_ir_analog_ticks(&mut self, scene_scroll: Option<i32>) -> bool {
        if !matches!(self.view_state(), AppViewState::Result) {
            return false;
        }
        let Some(delta) = scene_scroll else {
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
        if self.ui.egui.as_ref().is_some_and(|ui| ui.blocks_game_input(false)) {
            self.reset_select_analog_scroll();
            return;
        }
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
        if self.detail_options_active() {
            self.move_detail_options_with_scroll(mov, select_analog_scroll_duration(mov));
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
            self.move_selection_with_scroll(
                if mov > 0 { SelectMove::Next } else { SelectMove::Previous },
                mov,
                select_analog_scroll_duration(mov),
            );
        }
    }

    pub(super) fn route_gamepad_button(&mut self, input: &ResolvedInput) {
        let pressed = input.pressed();
        let synthesized_analog_axis = input.synthesized_analog_axis();
        let has_play_control_context =
            self.play.active_play.is_some() || self.play.pending_play_start.is_some();
        let control = *input.scene();
        if pressed
            && self.select.key_config_edit.is_none()
            && control.has_shortcut(InputActionConfig::Screenshot)
        {
            self.request_manual_screenshot();
            return;
        }
        if self.route_practice_gamepad_control(input) {
            return;
        }
        if should_route_quick_retry_input(pressed, false, self.play.play_ending.is_some())
            && self.handle_quick_retry_control(input)
        {
            return;
        }
        if pressed && self.begin_play_fadeout_after_final_notes_control(input) {
            return;
        }
        if self.play.play_ending.is_some() {
            return;
        }
        let play_e1_control = has_play_control_context && self.update_play_e1_control_state(input);
        if has_play_control_context && self.update_play_exit_control_state(input) {
            return;
        }
        let play_option_control = pressed
            .then(|| {
                play_option_control_for_input(
                    input,
                    self.play.play_e1_held,
                    self.play.play_e2_held,
                    self.play.play_option_input.as_ref(),
                    &self.boot.profile_config.input,
                )
            })
            .flatten();
        let play_option_lane_action = play_option_control
            .and_then(|action| lane_action_from_option(action, synthesized_analog_axis));
        if has_play_control_context {
            self.update_lane_cover_key_hold(input, false);
        }
        if pressed {
            let lane_cover_changing =
                self.play.active_play.is_some() && self.play_lane_value_changing();
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
                && let Some(source) = input.source()
                && self.finish_select_ir_battle_hold(source)
            {
                return;
            }
            if let Some(source) = input.source() {
                self.clear_select_hold_control(source);
            }
            // E1/E2 の押下状態は入力元ごとの押下集合から集約済み。
            return;
        }

        // プレイ中: Start / E1 の2回連続押しでレーンカバー表示切替。
        // プレイ入力自体は push_shared_event で処理済み。
        if self.play.active_play.is_some() {
            if play_e1_control {
                self.handle_play_start_double_press();
            }
            return;
        }

        if self.play.pending_decide.is_some() {
            if self.update_decide_cancel_control_state(input) {
                return;
            }
            if let Some(action) = scene_decide_action(input) {
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
            if self.request_result_exit_skip_for_control(input) {
                return;
            }
            if self.result.result_exit.is_none() {
                if self.handle_course_intermediate_control(input) {
                    return;
                }
                if self.result_input_ready() && scene_result_action(input).is_some() {
                    self.begin_result_exit(self.course_intermediate_exit_action());
                }
            }
            return;
        }

        // リザルト画面
        if self.result.finished_play.is_some() && self.result.finished_course.is_none() {
            // フェードアウト中でも Key5/Key7 の押下状態は追跡する。
            self.sync_result_lane_holds_from_pressed_controls();
            if self.request_result_exit_skip_for_control(input) {
                return;
            }
            // 終了アニメーション中 (result_exit=Some) は held 追跡のみ行う。
            if self.result.result_exit.is_none() {
                if self.handle_result_control(input) {
                    return;
                }
                if self.result_input_ready()
                    && let Some(action) = scene_result_action(input)
                {
                    self.apply_result_action(action, false);
                }
            }
            return;
        }

        // コース（段位）リザルト: Key5/Key7 はフェードアウト後の hold 状態で
        // retry arrange を決める。コントローラーに固定の操作はなく、鍵盤の割り当てだけで判定する。
        if self.result.finished_course.is_some() {
            self.sync_result_lane_holds_from_pressed_controls();
            if self.request_result_exit_skip_for_control(input) {
                return;
            }
            if self.result.result_exit.is_none() {
                if self.handle_course_result_control(input) {
                    return;
                }
                if self.result_input_ready()
                    && let Some(action) = scene_result_action(input)
                {
                    self.apply_result_action(action, true);
                }
            }
            return;
        }

        if in_settings_stack(&self.select.folder_stack) {
            let _ = self.route_settings_control(input);
            return;
        }

        if self.select.select_option_panel == 0
            && control.is_ir_battle_hold()
            && let Some(source) = input.source()
            && self.begin_select_ir_battle_hold(source.clone(), scene_select_action(input))
        {
            return;
        }

        if self.select.select_option_panel == 0
            && self.select_ir_scope_toggle_is_e3()
            && control.is_e3()
            && self.toggle_select_ir_scope()
        {
            return;
        }

        if should_toggle_select_gauge_auto_shift(
            &control,
            self.input.start_held,
            self.input.select_held,
        ) {
            self.toggle_gauge_auto_shift();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            return;
        }

        if should_toggle_select_judge_auto_adjust(
            &control,
            self.input.start_held,
            self.input.select_held,
        ) {
            self.toggle_visual_offset_auto_adjust();
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
            if self.select.select_option_panel == 1
                && let Some(cycle) = target_cycle_from_input(&control)
            {
                if synthesized_analog_axis {
                    return;
                }
                self.apply_target_option_cycle(cycle);
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                return;
            }
            let option_changed = match self.select.select_option_panel {
                1 => self.apply_play_option_input(&control),
                2 => {
                    self.apply_legacy_assist_input(&control);
                    return;
                }
                3 => self.apply_detail_option_control(&control),
                _ => false,
            };
            if option_changed {
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            return;
        }

        if matches!(self.view_state(), AppViewState::Select)
            && let Some(action) = scene_select_action(input)
        {
            self.apply_select_action(action, input.source());
        }
    }
}
